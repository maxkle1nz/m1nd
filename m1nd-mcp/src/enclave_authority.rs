//! macOS Secure Enclave custody floor — amendment G9-A1.
//!
//! This module reimplements the *contract* of the proven h4nd Secure Enclave
//! adapter (`docs/proofs/m1nd10-g2-p256-secure-enclave-adapter-20260718.md`) —
//! the h4nd source is not in this repo. The contract is: explicit provisioning
//! bound to a permit (never open-or-create), `kSecAttrTokenIDSecureEnclave`,
//! re-attestation of token/type/size on every open, a non-exportable key, and
//! signing only through `SecKeyCreateSignature`. The human owner seat carries a
//! `kSecAccessControl` user-presence (biometric) requirement and is provisioned
//! only by the owner's ceremony — the agent never provisions it for real.
//!
//! Honesty carried by the amendment: this floor gives real hardware **key
//! custody** (a key that cannot be exported, only used) but the **anti-rollback**
//! of the protected epoch/journal roots remains filesystem-strength on a single
//! host. Receipts state that distinction explicitly. It does NOT claim multi-host
//! custody, hardware anti-rollback under physical attack, or root-compromise
//! resistance.
//!
//! P-256 signatures are verified only through m1nd-control's verifier (which owns
//! the p256 primitives and low-S DER normalization); this crate never links p256
//! for production verification.

use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use m1nd_control::{
    canonical_json, sign_authority_message, verify_authority_message_signature, AuthoritySigner,
    AuthoritySignerError, CryptographicIntegrity, IdentityStatus, OpaqueSignature,
    VerificationKeyV1, ECDSA_P256_SHA256_X962_ALGORITHM,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::authority_runtime::{
    ProtectedEpochAssurance, ProtectedEpochBackend, ProtectedEpochSnapshotV1,
};
use crate::authority_wal::{AuthorityWalCryptoAssurance, AuthorityWalRecordCrypto};
use crate::owner_security_config::{
    OwnerSecurityConfigRootAssuranceV1, OwnerSecurityConfigRootV1,
    ProtectedOwnerSecurityConfigRootBackendV1,
};
use crate::protected_journal_head::{
    ProtectedJournalHeadAssuranceV1, ProtectedJournalHeadBackendV1, ProtectedJournalHeadSnapshotV1,
};

/// The custody-floor identifier stamped on every G9/G10 receipt minted under this
/// amendment (`docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md` §5).
pub const SECURE_ENCLAVE_CUSTODY_FLOOR_V1: &str = "secure-enclave-single-host-v1";

/// Stable module-level attestation identities. The FFI layer maps the observed
/// Security.framework attributes (`kSecAttrTokenID`, `kSecAttrKeyType`,
/// `kSecAttrKeySizeInBits`) onto these; re-attestation compares against them.
pub const SECURE_ENCLAVE_TOKEN_ID: &str = "com.apple.setoken";
pub const SECURE_ENCLAVE_KEY_TYPE_EC_PRIME_RANDOM: &str = "EC_SEC_PRIME_RANDOM_256";
pub const SECURE_ENCLAVE_KEY_SIZE_BITS: u32 = 256;

/// How the enclave key is gated on use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnclaveAccessControlV1 {
    /// Unattended verifier/quorum seat: enclave-bound, non-exportable, usable by
    /// this process without interactive presence.
    PrivateKeyUsageNonExportable,
    /// Human owner seat: each signature additionally requires biometric user
    /// presence (`kSecAccessControlUserPresence`). The agent NEVER provisions
    /// this variant — that is the owner's biometric ceremony.
    UserPresenceBiometricNonExportable,
}

/// Observed (or pinned-expected) attributes of an enclave key. Re-attestation
/// refuses any drift fail-closed before a key is used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnclaveKeyAttestationV1 {
    pub token_id: String,
    pub key_type: String,
    pub key_size_bits: u32,
    pub access_control: EnclaveAccessControlV1,
}

impl EnclaveKeyAttestationV1 {
    /// The canonical attestation the custody floor requires for a given seat
    /// class: Secure Enclave token, EC P-256, 256-bit.
    pub fn canonical(access_control: EnclaveAccessControlV1) -> Self {
        Self {
            token_id: SECURE_ENCLAVE_TOKEN_ID.to_owned(),
            key_type: SECURE_ENCLAVE_KEY_TYPE_EC_PRIME_RANDOM.to_owned(),
            key_size_bits: SECURE_ENCLAVE_KEY_SIZE_BITS,
            access_control,
        }
    }

    /// Fail-closed re-attestation: the observed attributes must equal the pinned
    /// expectation AND independently satisfy the Secure Enclave token/type/size
    /// invariants. Any drift refuses rather than trusting the opened key.
    pub fn reattest(&self, expected: &EnclaveKeyAttestationV1) -> Result<(), EnclaveError> {
        if self != expected {
            return Err(EnclaveError::AttestationMismatch {
                expected: format!("{expected:?}"),
                observed: format!("{self:?}"),
            });
        }
        if self.token_id != SECURE_ENCLAVE_TOKEN_ID
            || self.key_type != SECURE_ENCLAVE_KEY_TYPE_EC_PRIME_RANDOM
            || self.key_size_bits != SECURE_ENCLAVE_KEY_SIZE_BITS
        {
            return Err(EnclaveError::AttestationMismatch {
                expected: "secure-enclave EC P-256 (256-bit) token".to_owned(),
                observed: format!("{self:?}"),
            });
        }
        Ok(())
    }
}

/// An explicit permit binding one provisioning to one ceremony context. A permit
/// for a different key/subject/context must not provision another key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnclaveProvisioningPermitV1 {
    pub key_id: String,
    pub subject_id: String,
    /// Keychain application label the key is filed under.
    pub application_label: String,
    pub access_control: EnclaveAccessControlV1,
    /// Digest of the exact ceremony/authority context this provisioning is bound
    /// to (candidate + authority receipt), mirroring the proven adapter's
    /// candidate-bound provisioning permit.
    pub bound_context_digest: String,
}

/// A successfully opened enclave key: its public representation and the observed
/// attestation. The private key never leaves the enclave.
#[derive(Clone, Debug)]
pub struct EnclaveOpenedKeyV1 {
    pub key_id: String,
    pub subject_id: String,
    /// 65-byte uncompressed SEC1 public key (`0x04 || X || Y`).
    pub public_key_sec1: Vec<u8>,
    pub attestation: EnclaveKeyAttestationV1,
    /// Opaque platform reference the keystore uses to address the key when
    /// signing. Never a private key.
    pub platform_ref: String,
}

/// The narrow, mockable boundary to the platform key store. The real
/// implementation is a Security.framework adapter; tests inject a software P-256
/// mock so the whole custody path is proven without enclave hardware or biometry.
pub trait SecureEnclaveKeyStoreV1: Send + Sync {
    /// Explicitly provision a NEW enclave key bound to `permit`. This is never an
    /// open-or-create: if a key with the permit's id already exists, provisioning
    /// fails closed rather than silently adopting it.
    fn provision(
        &self,
        permit: &EnclaveProvisioningPermitV1,
    ) -> Result<EnclaveOpenedKeyV1, EnclaveError>;

    /// Open an EXISTING key and return its observed public key and attestation for
    /// re-attestation by the caller.
    fn open(&self, key_id: &str) -> Result<EnclaveOpenedKeyV1, EnclaveError>;

    /// Sign `message` with the opened key through `SecKeyCreateSignature`,
    /// returning raw ASN.1 DER (possibly high-S; m1nd-control normalizes to
    /// canonical low-S downstream).
    fn sign(&self, opened: &EnclaveOpenedKeyV1, message: &[u8]) -> Result<Vec<u8>, EnclaveError>;
}

/// Provision an *agent-held* enclave seat (a verifier/quorum key). Refuses the
/// biometric human-owner seat fail-closed: the human seat is provisioned only by
/// the owner's ceremony, never by an agent path.
pub fn provision_agent_enclave_seat(
    key_store: &dyn SecureEnclaveKeyStoreV1,
    permit: &EnclaveProvisioningPermitV1,
) -> Result<EnclaveOpenedKeyV1, EnclaveError> {
    if permit.access_control == EnclaveAccessControlV1::UserPresenceBiometricNonExportable {
        return Err(EnclaveError::HumanSeatProvisioningRefused);
    }
    let opened = key_store.provision(permit)?;
    opened
        .attestation
        .reattest(&EnclaveKeyAttestationV1::canonical(permit.access_control))?;
    require_uncompressed_sec1(&opened.public_key_sec1)?;
    Ok(opened)
}

/// A P-256 Secure Enclave signer. Construction opens an existing key and
/// re-attests it, so a signer that exists is a signer whose token/type/size were
/// verified. Signing goes through the key store's `SecKeyCreateSignature`.
pub struct SecureEnclaveSigner {
    key_store: Arc<dyn SecureEnclaveKeyStoreV1>,
    opened: EnclaveOpenedKeyV1,
    public_key_sec1: Vec<u8>,
}

impl SecureEnclaveSigner {
    /// Open an existing enclave key and re-attest it against the pinned
    /// expectation before it can sign anything.
    pub fn open_attested(
        key_store: Arc<dyn SecureEnclaveKeyStoreV1>,
        key_id: &str,
        expected: &EnclaveKeyAttestationV1,
    ) -> Result<Self, EnclaveError> {
        let opened = key_store.open(key_id)?;
        opened.attestation.reattest(expected)?;
        require_uncompressed_sec1(&opened.public_key_sec1)?;
        let public_key_sec1 = opened.public_key_sec1.clone();
        Ok(Self {
            key_store,
            opened,
            public_key_sec1,
        })
    }

    pub fn public_key_sec1(&self) -> &[u8] {
        &self.public_key_sec1
    }

    /// The pinned public verification-key record for this signer, as the config
    /// and quorum ceremonies register it. Public material only.
    pub fn verification_key(&self, created_at: u64, activated_at: u64) -> VerificationKeyV1 {
        VerificationKeyV1 {
            key_id: self.opened.key_id.clone(),
            subject_id: self.opened.subject_id.clone(),
            algorithm: ECDSA_P256_SHA256_X962_ALGORITHM.to_owned(),
            public_key: hex_lower(&self.public_key_sec1),
            created_at,
            activated_at,
            expires_at: None,
            revoked_at: None,
            rotated_at: None,
            replacement_key_id: None,
            status: IdentityStatus::Active,
        }
    }
}

impl AuthoritySigner for SecureEnclaveSigner {
    fn key_id(&self) -> &str {
        &self.opened.key_id
    }

    fn subject_id(&self) -> &str {
        &self.opened.subject_id
    }

    fn algorithm(&self) -> &str {
        ECDSA_P256_SHA256_X962_ALGORITHM
    }

    fn public_key_bytes(&self) -> Result<Vec<u8>, AuthoritySignerError> {
        Ok(self.public_key_sec1.clone())
    }

    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, AuthoritySignerError> {
        self.key_store
            .sign(&self.opened, message)
            .map_err(|error| AuthoritySignerError::platform(error.to_string()))
    }
}

/// Production AuthorityWAL record crypto backed by an enclave P-256 signer.
/// `sign` normalizes the enclave's DER to canonical low-S through m1nd-control;
/// `verify` runs entirely through m1nd-control's verifier — this crate never
/// links p256 for production crypto. Assurance is `ProductionCryptographic`.
pub struct EnclaveBackedWalRecordCrypto {
    signer: Arc<dyn AuthoritySigner + Send + Sync>,
    verification_key: VerificationKeyV1,
}

impl EnclaveBackedWalRecordCrypto {
    /// Bind a P-256 signer to its pinned public verification key. Refuses any
    /// non-P-256 algorithm or an identity mismatch between signer and key.
    pub fn new(
        signer: Arc<dyn AuthoritySigner + Send + Sync>,
        verification_key: VerificationKeyV1,
    ) -> Result<Self, EnclaveError> {
        if signer.algorithm() != ECDSA_P256_SHA256_X962_ALGORITHM
            || verification_key.algorithm != ECDSA_P256_SHA256_X962_ALGORITHM
        {
            return Err(EnclaveError::Sign(
                "enclave WAL crypto is P-256 only".to_owned(),
            ));
        }
        if signer.key_id() != verification_key.key_id
            || signer.subject_id() != verification_key.subject_id
        {
            return Err(EnclaveError::Sign(
                "enclave signer identity does not match the pinned verification key".to_owned(),
            ));
        }
        Ok(Self {
            signer,
            verification_key,
        })
    }
}

impl AuthorityWalRecordCrypto for EnclaveBackedWalRecordCrypto {
    fn assurance(&self) -> AuthorityWalCryptoAssurance {
        AuthorityWalCryptoAssurance::ProductionCryptographic
    }

    fn issuer(&self) -> &str {
        &self.verification_key.subject_id
    }

    fn key_id(&self) -> &str {
        &self.verification_key.key_id
    }

    fn algorithm(&self) -> &str {
        ECDSA_P256_SHA256_X962_ALGORITHM
    }

    fn sign(&self, canonical_record_message: &[u8]) -> Result<String, String> {
        let signature = sign_authority_message(
            canonical_record_message,
            &self.verification_key,
            self.signer.as_ref(),
        )
        .map_err(|error| error.to_string())?;
        Ok(signature.as_str().to_owned())
    }

    fn verify(&self, canonical_record_message: &[u8], signature: &str) -> Result<(), String> {
        match verify_authority_message_signature(
            canonical_record_message,
            &OpaqueSignature::new(signature),
            &self.verification_key,
        ) {
            Ok(CryptographicIntegrity::VerifiedEcdsaP256Sha256X962) => Ok(()),
            Ok(CryptographicIntegrity::VerifiedEd25519) => {
                Err("enclave WAL crypto verified a non-P-256 signature".to_owned())
            }
            Err(error) => Err(error.to_string()),
        }
    }
}

fn require_uncompressed_sec1(bytes: &[u8]) -> Result<(), EnclaveError> {
    if bytes.len() != 65 || bytes.first() != Some(&0x04) {
        return Err(EnclaveError::PublicKeyEncoding(
            "enclave public key is not a 65-byte uncompressed SEC1 point".to_owned(),
        ));
    }
    Ok(())
}

fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut accumulator, byte| {
            let _ = write!(accumulator, "{byte:02x}");
            accumulator
        },
    )
}

// ===========================================================================
// Sealed, device/inode-pinned protected roots (macOS = unix).
//
// Each protected CAS root (config epoch, runtime epoch, journal heads) is a 0700
// owner-only directory whose (device, inode) identity is pinned at open. Slots
// are authenticity-sealed JSON files: the payload plus an enclave signature over
// its canonical bytes, verified on read through m1nd-control.
//
// Honesty carried by amendment G9-A1: the enclave seal proves AUTHENTICITY (this
// owner's enclave authored the record). Anti-rollback (freshness) is
// filesystem-strength — a 0700 no-follow directory plus monotonic epoch/sequence
// invariants — NOT hardware anti-rollback under a root-level or physical
// attacker. That is the amendment's declared non-claim, stated here, not hidden.
// ===========================================================================

const SEALED_RECORD_SCHEMA: &str = "m1nd-enclave-sealed-protected-record-v1";
const SEAL_MESSAGE_PREFIX: &[u8] = b"m1nd-enclave-protected-seal-v1\0";

const PROTECTED_EPOCH_SEAL_DOMAIN: &str = "m1nd-enclave-protected-runtime-epoch-v1";
const PROTECTED_EPOCH_SLOT_FILE: &str = "runtime-epoch.sealed.json";
const OWNER_CONFIG_ROOT_SEAL_DOMAIN: &str = "m1nd-enclave-owner-security-config-root-v1";
const OWNER_CONFIG_ROOT_SLOT_FILE: &str = "owner-security-config-root.sealed.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeviceInode {
    device: u64,
    inode: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SealedRecordV1 {
    schema: String,
    domain: String,
    issuer: String,
    key_id: String,
    algorithm: String,
    payload: serde_json::Value,
    /// Lowercase-hex enclave signature over `seal_message(domain, canonical(payload))`.
    seal: String,
}

/// A 0700, device/inode-pinned protected directory holding enclave-sealed slots.
/// Reads verify the seal through m1nd-control; the private key never leaves the
/// enclave. Shared by the config-epoch, runtime-epoch, and journal-head backends.
pub struct SealedProtectedRootV1 {
    root: PathBuf,
    pinned: DeviceInode,
    signer: Arc<dyn AuthoritySigner + Send + Sync>,
    verification_key: VerificationKeyV1,
}

impl SealedProtectedRootV1 {
    /// Open an existing 0700 directory, refuse symlinks/non-0700 modes, and pin
    /// its (device, inode). The verification key must match the signer identity.
    pub fn open(
        root: impl AsRef<Path>,
        signer: Arc<dyn AuthoritySigner + Send + Sync>,
        verification_key: VerificationKeyV1,
    ) -> Result<Self, EnclaveError> {
        if signer.key_id() != verification_key.key_id
            || signer.subject_id() != verification_key.subject_id
            || signer.algorithm() != verification_key.algorithm
        {
            return Err(EnclaveError::Sign(
                "sealing signer identity does not match the pinned verification key".to_owned(),
            ));
        }
        let root = root.as_ref().to_path_buf();
        let pinned = pin_owner_only_directory(&root)?;
        Ok(Self {
            root,
            pinned,
            signer,
            verification_key,
        })
    }

    fn revalidate_identity(&self) -> Result<(), EnclaveError> {
        let current = pin_owner_only_directory(&self.root)?;
        if current != self.pinned {
            return Err(EnclaveError::Filesystem(format!(
                "protected root identity changed: pinned {:?}, observed {current:?}",
                self.pinned
            )));
        }
        Ok(())
    }

    /// Read and authenticity-verify a slot, returning its payload or `None` when
    /// the slot does not exist.
    fn read_slot(
        &self,
        domain: &str,
        slot_file: &str,
    ) -> Result<Option<serde_json::Value>, EnclaveError> {
        self.revalidate_identity()?;
        let bytes = match read_file_no_follow(&self.root.join(slot_file))? {
            Some(bytes) => bytes,
            None => return Ok(None),
        };
        let record: SealedRecordV1 = serde_json::from_slice(&bytes)
            .map_err(|error| EnclaveError::SealVerification(error.to_string()))?;
        if record.schema != SEALED_RECORD_SCHEMA || record.domain != domain {
            return Err(EnclaveError::SealVerification(
                "sealed record schema/domain mismatch".to_owned(),
            ));
        }
        if record.issuer != self.verification_key.subject_id
            || record.key_id != self.verification_key.key_id
            || record.algorithm != self.verification_key.algorithm
        {
            return Err(EnclaveError::SealVerification(
                "sealed record identity does not match the pinned key".to_owned(),
            ));
        }
        let canonical = canonical_json(&record.payload)
            .map_err(|error| EnclaveError::SealVerification(error.to_string()))?;
        let message = seal_message(domain, &canonical);
        match verify_authority_message_signature(
            &message,
            &OpaqueSignature::new(&record.seal),
            &self.verification_key,
        ) {
            Ok(CryptographicIntegrity::VerifiedEcdsaP256Sha256X962) => Ok(Some(record.payload)),
            Ok(CryptographicIntegrity::VerifiedEd25519) => Err(EnclaveError::SealVerification(
                "sealed record is not a P-256 enclave seal".to_owned(),
            )),
            Err(error) => Err(EnclaveError::SealVerification(error.to_string())),
        }
    }

    /// Seal `payload` with the enclave and atomically publish the slot: sign the
    /// canonical bytes, write a fresh 0600 temp, fsync, rename, fsync the dir.
    fn write_slot(
        &self,
        domain: &str,
        slot_file: &str,
        payload: serde_json::Value,
    ) -> Result<(), EnclaveError> {
        self.revalidate_identity()?;
        let canonical = canonical_json(&payload)
            .map_err(|error| EnclaveError::Filesystem(error.to_string()))?;
        let message = seal_message(domain, &canonical);
        let seal = sign_authority_message(&message, &self.verification_key, self.signer.as_ref())
            .map_err(|error| EnclaveError::Sign(error.to_string()))?;
        let record = SealedRecordV1 {
            schema: SEALED_RECORD_SCHEMA.to_owned(),
            domain: domain.to_owned(),
            issuer: self.verification_key.subject_id.clone(),
            key_id: self.verification_key.key_id.clone(),
            algorithm: self.verification_key.algorithm.clone(),
            payload,
            seal: seal.as_str().to_owned(),
        };
        let serialized = serde_json::to_vec(&record)
            .map_err(|error| EnclaveError::Filesystem(error.to_string()))?;
        atomic_write_no_follow(&self.root, slot_file, &serialized)
    }
}

fn pin_owner_only_directory(root: &Path) -> Result<DeviceInode, EnclaveError> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| EnclaveError::Filesystem(format!("stat {}: {error}", root.display())))?;
    if !metadata.is_dir() {
        return Err(EnclaveError::Filesystem(format!(
            "protected root is not a directory: {}",
            root.display()
        )));
    }
    let mode = metadata.permissions().mode() & 0o777;
    if mode != 0o700 {
        return Err(EnclaveError::Filesystem(format!(
            "protected root must be mode 0700, observed {mode:o}: {}",
            root.display()
        )));
    }
    Ok(DeviceInode {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

fn read_file_no_follow(path: &Path) -> Result<Option<Vec<u8>>, EnclaveError> {
    let mut file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(EnclaveError::Filesystem(format!(
                "open {}: {error}",
                path.display()
            )))
        }
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| EnclaveError::Filesystem(error.to_string()))?;
    Ok(Some(bytes))
}

fn atomic_write_no_follow(root: &Path, slot_file: &str, bytes: &[u8]) -> Result<(), EnclaveError> {
    let temp = root.join(format!("{slot_file}.tmp"));
    let final_path = root.join(slot_file);
    // A stale temp symlink must not redirect the write; start from a clean temp.
    let _ = fs::remove_file(&temp);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&temp)
        .map_err(|error| EnclaveError::Filesystem(format!("create {}: {error}", temp.display())))?;
    file.write_all(bytes)
        .map_err(|error| EnclaveError::Filesystem(error.to_string()))?;
    file.sync_all()
        .map_err(|error| EnclaveError::Filesystem(error.to_string()))?;
    fs::rename(&temp, &final_path).map_err(|error| EnclaveError::Filesystem(error.to_string()))?;
    File::open(root)
        .and_then(|dir| dir.sync_all())
        .map_err(|error| EnclaveError::Filesystem(error.to_string()))?;
    Ok(())
}

fn seal_message(domain: &str, canonical_payload: &[u8]) -> Vec<u8> {
    let mut message =
        Vec::with_capacity(SEAL_MESSAGE_PREFIX.len() + domain.len() + canonical_payload.len() + 16);
    message.extend_from_slice(SEAL_MESSAGE_PREFIX);
    message.extend_from_slice(&(domain.len() as u64).to_be_bytes());
    message.extend_from_slice(domain.as_bytes());
    message.extend_from_slice(&(canonical_payload.len() as u64).to_be_bytes());
    message.extend_from_slice(canonical_payload);
    message
}

fn journal_head_slot_file(domain: &str) -> String {
    let digest = Sha256::digest(domain.as_bytes());
    format!("journal-head-{}.sealed.json", hex_lower(digest.as_slice()))
}

/// Production `ProtectedEpochBackend` for the AuthorityRuntime anti-rollback
/// epoch, sealed by the enclave. Assurance is `HardwareProtectedAttested`.
pub struct SecureEnclaveProtectedEpochBackend {
    root: SealedProtectedRootV1,
}

impl SecureEnclaveProtectedEpochBackend {
    pub fn new(root: SealedProtectedRootV1) -> Self {
        Self { root }
    }
}

impl ProtectedEpochBackend for SecureEnclaveProtectedEpochBackend {
    fn assurance(&self) -> ProtectedEpochAssurance {
        ProtectedEpochAssurance::HardwareProtectedAttested
    }

    fn read_latest(&self) -> Result<Option<ProtectedEpochSnapshotV1>, String> {
        match self
            .root
            .read_slot(PROTECTED_EPOCH_SEAL_DOMAIN, PROTECTED_EPOCH_SLOT_FILE)
            .map_err(|error| error.to_string())?
        {
            Some(value) => Ok(Some(
                serde_json::from_value(value).map_err(|error| error.to_string())?,
            )),
            None => Ok(None),
        }
    }

    fn compare_and_advance(
        &mut self,
        expected: Option<&ProtectedEpochSnapshotV1>,
        next: &ProtectedEpochSnapshotV1,
    ) -> Result<(), String> {
        let current = self.read_latest()?;
        if current.as_ref() != expected {
            return Err("sealed protected epoch compare-and-swap mismatch".to_string());
        }
        let expected_epoch = expected.map_or(0, |snapshot| snapshot.epoch);
        if next.epoch != expected_epoch.saturating_add(1) {
            return Err("sealed protected epoch must advance exactly once".to_string());
        }
        let value = serde_json::to_value(next).map_err(|error| error.to_string())?;
        self.root
            .write_slot(
                PROTECTED_EPOCH_SEAL_DOMAIN,
                PROTECTED_EPOCH_SLOT_FILE,
                value,
            )
            .map_err(|error| error.to_string())
    }
}

/// Production owner-security-config anti-rollback root, sealed by the enclave.
pub struct SecureEnclaveOwnerSecurityConfigRootBackend {
    root: SealedProtectedRootV1,
}

impl SecureEnclaveOwnerSecurityConfigRootBackend {
    pub fn new(root: SealedProtectedRootV1) -> Self {
        Self { root }
    }
}

impl ProtectedOwnerSecurityConfigRootBackendV1 for SecureEnclaveOwnerSecurityConfigRootBackend {
    fn assurance(&self) -> OwnerSecurityConfigRootAssuranceV1 {
        OwnerSecurityConfigRootAssuranceV1::HardwareProtectedAttested
    }

    fn read_latest(&self) -> Result<Option<OwnerSecurityConfigRootV1>, String> {
        match self
            .root
            .read_slot(OWNER_CONFIG_ROOT_SEAL_DOMAIN, OWNER_CONFIG_ROOT_SLOT_FILE)
            .map_err(|error| error.to_string())?
        {
            Some(value) => Ok(Some(
                serde_json::from_value(value).map_err(|error| error.to_string())?,
            )),
            None => Ok(None),
        }
    }

    fn compare_and_advance(
        &mut self,
        expected: Option<&OwnerSecurityConfigRootV1>,
        next: &OwnerSecurityConfigRootV1,
    ) -> Result<(), String> {
        let current = self.read_latest()?;
        if current.as_ref() != expected {
            return Err("sealed owner-config root compare-and-swap mismatch".to_string());
        }
        let expected_epoch = expected.map_or(1, |root| root.config_epoch.saturating_add(1));
        if next.config_epoch != expected_epoch {
            return Err("sealed owner-config root epoch must advance exactly once".to_string());
        }
        let value = serde_json::to_value(next).map_err(|error| error.to_string())?;
        self.root
            .write_slot(
                OWNER_CONFIG_ROOT_SEAL_DOMAIN,
                OWNER_CONFIG_ROOT_SLOT_FILE,
                value,
            )
            .map_err(|error| error.to_string())
    }
}

/// Production broker/WAL anti-rollback journal-head backend, sealed by the
/// enclave. One sealed slot per journal domain.
pub struct SecureEnclaveJournalHeadBackend {
    root: SealedProtectedRootV1,
}

impl SecureEnclaveJournalHeadBackend {
    pub fn new(root: SealedProtectedRootV1) -> Self {
        Self { root }
    }
}

impl ProtectedJournalHeadBackendV1 for SecureEnclaveJournalHeadBackend {
    fn assurance(&self) -> ProtectedJournalHeadAssuranceV1 {
        ProtectedJournalHeadAssuranceV1::HardwareProtectedAttested
    }

    fn read_latest(&self, domain: &str) -> Result<Option<ProtectedJournalHeadSnapshotV1>, String> {
        match self
            .root
            .read_slot(domain, &journal_head_slot_file(domain))
            .map_err(|error| error.to_string())?
        {
            Some(value) => {
                let snapshot: ProtectedJournalHeadSnapshotV1 =
                    serde_json::from_value(value).map_err(|error| error.to_string())?;
                if snapshot.domain != domain {
                    return Err("sealed journal head domain mismatch".to_string());
                }
                Ok(Some(snapshot))
            }
            None => Ok(None),
        }
    }

    fn compare_and_advance(
        &mut self,
        domain: &str,
        expected: Option<&ProtectedJournalHeadSnapshotV1>,
        next: &ProtectedJournalHeadSnapshotV1,
    ) -> Result<(), String> {
        if next.domain != domain {
            return Err("sealed journal-head domain mismatch".to_string());
        }
        let current = self.read_latest(domain)?;
        if current.as_ref() != expected {
            return Err("sealed journal-head compare-and-swap mismatch".to_string());
        }
        match expected {
            None if next.record_sequence != 0 || next.head_digest.is_some() => {
                return Err("initial sealed journal head must be the empty anchor".to_string());
            }
            Some(previous)
                if next.record_sequence != previous.record_sequence.saturating_add(1)
                    || next.head_digest.is_none() =>
            {
                return Err("sealed journal head must advance exactly one record".to_string());
            }
            _ => {}
        }
        let value = serde_json::to_value(next).map_err(|error| error.to_string())?;
        self.root
            .write_slot(domain, &journal_head_slot_file(domain), value)
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug)]
pub enum EnclaveError {
    Provisioning(String),
    Open(String),
    Sign(String),
    AttestationMismatch {
        expected: String,
        observed: String,
    },
    PublicKeyEncoding(String),
    /// The agent attempted to provision the biometric human-owner seat, which is
    /// the owner's ceremony alone.
    HumanSeatProvisioningRefused,
    Filesystem(String),
    SealVerification(String),
}

impl fmt::Display for EnclaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provisioning(detail) => {
                write!(formatter, "enclave provisioning failed: {detail}")
            }
            Self::Open(detail) => write!(formatter, "enclave key open failed: {detail}"),
            Self::Sign(detail) => write!(formatter, "enclave signing failed: {detail}"),
            Self::AttestationMismatch { expected, observed } => write!(
                formatter,
                "enclave attestation mismatch: expected {expected}, observed {observed}"
            ),
            Self::PublicKeyEncoding(detail) => {
                write!(formatter, "enclave public key encoding: {detail}")
            }
            Self::HumanSeatProvisioningRefused => formatter.write_str(
                "the biometric human-owner seat is provisioned only by the owner ceremony",
            ),
            Self::Filesystem(detail) => write!(formatter, "protected-root filesystem: {detail}"),
            Self::SealVerification(detail) => {
                write!(formatter, "protected-root seal verification: {detail}")
            }
        }
    }
}

impl Error for EnclaveError {}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use p256::ecdsa::{signature::Signer as _, Signature, SigningKey};
    use parking_lot::Mutex;
    use sha2::{Digest, Sha256};
    use tempfile::TempDir;

    use super::*;

    /// Software P-256 stand-in for the Secure Enclave: it holds a signing key,
    /// produces the canonical attestation, and signs raw (possibly high-S) DER —
    /// exactly the non-deterministic reality m1nd-control must normalize.
    struct MockEnclaveKeyStore {
        keys: Mutex<BTreeMap<String, MockKey>>,
        access_control: EnclaveAccessControlV1,
        drift: Option<AttestationDrift>,
    }

    #[derive(Clone, Copy)]
    enum AttestationDrift {
        Token,
        KeyType,
        Size,
    }

    struct MockKey {
        signing_key: SigningKey,
        subject_id: String,
    }

    impl MockEnclaveKeyStore {
        fn new(access_control: EnclaveAccessControlV1) -> Self {
            Self {
                keys: Mutex::new(BTreeMap::new()),
                access_control,
                drift: None,
            }
        }

        fn with_drift(access_control: EnclaveAccessControlV1, drift: AttestationDrift) -> Self {
            Self {
                keys: Mutex::new(BTreeMap::new()),
                access_control,
                drift: Some(drift),
            }
        }

        fn attestation(&self) -> EnclaveKeyAttestationV1 {
            let mut attestation = EnclaveKeyAttestationV1::canonical(self.access_control);
            match self.drift {
                Some(AttestationDrift::Token) => {
                    attestation.token_id = "com.apple.software".to_owned()
                }
                Some(AttestationDrift::KeyType) => attestation.key_type = "RSA".to_owned(),
                Some(AttestationDrift::Size) => attestation.key_size_bits = 521,
                None => {}
            }
            attestation
        }

        fn opened(&self, key_id: &str, key: &MockKey) -> EnclaveOpenedKeyV1 {
            let public_key_sec1 = key
                .signing_key
                .verifying_key()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec();
            EnclaveOpenedKeyV1 {
                key_id: key_id.to_owned(),
                subject_id: key.subject_id.clone(),
                public_key_sec1,
                attestation: self.attestation(),
                platform_ref: format!("mock:{key_id}"),
            }
        }
    }

    fn scalar_for(key_id: &str) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"mock-enclave-scalar-v1\0");
        hasher.update(key_id.as_bytes());
        let mut bytes: [u8; 32] = hasher.finalize().into();
        // Keep the scalar comfortably below the group order for the test seed.
        bytes[0] = 0x01;
        bytes
    }

    impl SecureEnclaveKeyStoreV1 for MockEnclaveKeyStore {
        fn provision(
            &self,
            permit: &EnclaveProvisioningPermitV1,
        ) -> Result<EnclaveOpenedKeyV1, EnclaveError> {
            let mut keys = self.keys.lock();
            if keys.contains_key(&permit.key_id) {
                return Err(EnclaveError::Provisioning(
                    "key already exists; provisioning is never open-or-create".to_owned(),
                ));
            }
            let signing_key = SigningKey::from_slice(&scalar_for(&permit.key_id)).unwrap();
            let key = MockKey {
                signing_key,
                subject_id: permit.subject_id.clone(),
            };
            let opened = self.opened(&permit.key_id, &key);
            keys.insert(permit.key_id.clone(), key);
            Ok(opened)
        }

        fn open(&self, key_id: &str) -> Result<EnclaveOpenedKeyV1, EnclaveError> {
            let keys = self.keys.lock();
            let key = keys
                .get(key_id)
                .ok_or_else(|| EnclaveError::Open(format!("no such enclave key '{key_id}'")))?;
            Ok(self.opened(key_id, key))
        }

        fn sign(
            &self,
            opened: &EnclaveOpenedKeyV1,
            message: &[u8],
        ) -> Result<Vec<u8>, EnclaveError> {
            let keys = self.keys.lock();
            let key = keys
                .get(&opened.key_id)
                .ok_or_else(|| EnclaveError::Sign("no such enclave key".to_owned()))?;
            let signature: Signature = key.signing_key.sign(message);
            Ok(signature.to_der().as_bytes().to_vec())
        }
    }

    fn agent_permit(key_id: &str) -> EnclaveProvisioningPermitV1 {
        EnclaveProvisioningPermitV1 {
            key_id: key_id.to_owned(),
            subject_id: format!("subject-{key_id}"),
            application_label: format!("label-{key_id}"),
            access_control: EnclaveAccessControlV1::PrivateKeyUsageNonExportable,
            bound_context_digest: "ceremony-context-1".to_owned(),
        }
    }

    #[test]
    fn enclave_signer_signs_records_verifiable_offline_through_control() {
        let store: Arc<dyn SecureEnclaveKeyStoreV1> = Arc::new(MockEnclaveKeyStore::new(
            EnclaveAccessControlV1::PrivateKeyUsageNonExportable,
        ));
        provision_agent_enclave_seat(store.as_ref(), &agent_permit("seat-0")).unwrap();
        let expected = EnclaveKeyAttestationV1::canonical(
            EnclaveAccessControlV1::PrivateKeyUsageNonExportable,
        );
        let signer =
            SecureEnclaveSigner::open_attested(Arc::clone(&store), "seat-0", &expected).unwrap();
        let verification_key = signer.verification_key(0, 0);
        let crypto = EnclaveBackedWalRecordCrypto::new(Arc::new(signer), verification_key).unwrap();

        assert_eq!(
            crypto.assurance(),
            AuthorityWalCryptoAssurance::ProductionCryptographic
        );
        assert_eq!(crypto.algorithm(), ECDSA_P256_SHA256_X962_ALGORITHM);

        let message = b"m1nd-authority-wal-record-canonical-message";
        let signature = crypto.sign(message).unwrap();
        // Round-trips through the same control-plane verifier the offline path
        // uses; the enclave DER was normalized to canonical low-S by m1nd-control.
        crypto.verify(message, &signature).unwrap();
        assert!(crypto.verify(b"a different message", &signature).is_err());
    }

    #[test]
    fn reattestation_refuses_token_type_or_size_drift() {
        for drift in [
            AttestationDrift::Token,
            AttestationDrift::KeyType,
            AttestationDrift::Size,
        ] {
            let store: Arc<dyn SecureEnclaveKeyStoreV1> =
                Arc::new(MockEnclaveKeyStore::with_drift(
                    EnclaveAccessControlV1::PrivateKeyUsageNonExportable,
                    drift,
                ));
            // Provision writes a drifted key; opening it must re-attest and refuse.
            store.provision(&agent_permit("seat-drift")).unwrap();
            let expected = EnclaveKeyAttestationV1::canonical(
                EnclaveAccessControlV1::PrivateKeyUsageNonExportable,
            );
            let opened =
                SecureEnclaveSigner::open_attested(Arc::clone(&store), "seat-drift", &expected);
            assert!(matches!(
                opened,
                Err(EnclaveError::AttestationMismatch { .. })
            ));
        }
    }

    #[test]
    fn provisioning_is_never_open_or_create() {
        let store = MockEnclaveKeyStore::new(EnclaveAccessControlV1::PrivateKeyUsageNonExportable);
        provision_agent_enclave_seat(&store, &agent_permit("seat-1")).unwrap();
        let again = provision_agent_enclave_seat(&store, &agent_permit("seat-1"));
        assert!(matches!(again, Err(EnclaveError::Provisioning(_))));
    }

    #[test]
    fn agent_never_provisions_the_biometric_human_seat() {
        let store =
            MockEnclaveKeyStore::new(EnclaveAccessControlV1::UserPresenceBiometricNonExportable);
        let mut permit = agent_permit("owner-seat");
        permit.access_control = EnclaveAccessControlV1::UserPresenceBiometricNonExportable;
        let outcome = provision_agent_enclave_seat(&store, &permit);
        assert!(matches!(
            outcome,
            Err(EnclaveError::HumanSeatProvisioningRefused)
        ));
    }

    #[test]
    fn wal_crypto_refuses_signer_key_identity_mismatch() {
        let store: Arc<dyn SecureEnclaveKeyStoreV1> = Arc::new(MockEnclaveKeyStore::new(
            EnclaveAccessControlV1::PrivateKeyUsageNonExportable,
        ));
        provision_agent_enclave_seat(store.as_ref(), &agent_permit("seat-2")).unwrap();
        let expected = EnclaveKeyAttestationV1::canonical(
            EnclaveAccessControlV1::PrivateKeyUsageNonExportable,
        );
        let signer =
            SecureEnclaveSigner::open_attested(Arc::clone(&store), "seat-2", &expected).unwrap();
        let mut verification_key = signer.verification_key(0, 0);
        verification_key.key_id = "a-different-key-id".to_owned();
        assert!(matches!(
            EnclaveBackedWalRecordCrypto::new(Arc::new(signer), verification_key),
            Err(EnclaveError::Sign(_))
        ));
    }

    fn seat_store() -> Arc<dyn SecureEnclaveKeyStoreV1> {
        Arc::new(MockEnclaveKeyStore::new(
            EnclaveAccessControlV1::PrivateKeyUsageNonExportable,
        ))
    }

    fn attested_signer(
        store: &Arc<dyn SecureEnclaveKeyStoreV1>,
        key_id: &str,
    ) -> SecureEnclaveSigner {
        provision_agent_enclave_seat(store.as_ref(), &agent_permit(key_id)).unwrap();
        let expected = EnclaveKeyAttestationV1::canonical(
            EnclaveAccessControlV1::PrivateKeyUsageNonExportable,
        );
        SecureEnclaveSigner::open_attested(Arc::clone(store), key_id, &expected).unwrap()
    }

    fn sealed_root_at(
        dir: &Path,
        store: &Arc<dyn SecureEnclaveKeyStoreV1>,
        key_id: &str,
    ) -> SealedProtectedRootV1 {
        let signer = attested_signer(store, key_id);
        let verification_key = signer.verification_key(0, 0);
        SealedProtectedRootV1::open(dir, Arc::new(signer), verification_key).unwrap()
    }

    fn make_dir_with_mode(base: &Path, name: &str, mode: u32) -> PathBuf {
        let dir = base.join(name);
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(mode)).unwrap();
        dir
    }

    #[test]
    fn sealed_epoch_backend_seals_advances_and_reads_back() {
        let temp = TempDir::new().unwrap();
        let store = seat_store();
        let dir = make_dir_with_mode(temp.path(), "epoch-root", 0o700);
        let mut backend =
            SecureEnclaveProtectedEpochBackend::new(sealed_root_at(&dir, &store, "epoch-seat"));
        assert_eq!(
            backend.assurance(),
            ProtectedEpochAssurance::HardwareProtectedAttested
        );
        assert!(backend.read_latest().unwrap().is_none());

        let first = ProtectedEpochSnapshotV1 {
            epoch: 1,
            record_digest: "a".repeat(64),
        };
        backend.compare_and_advance(None, &first).unwrap();
        assert_eq!(backend.read_latest().unwrap(), Some(first.clone()));

        // Wrong expected predecessor and a skipped epoch both fail closed.
        let skip = ProtectedEpochSnapshotV1 {
            epoch: 3,
            record_digest: "c".repeat(64),
        };
        assert!(backend.compare_and_advance(Some(&first), &skip).is_err());
        assert!(backend.compare_and_advance(None, &skip).is_err());

        let second = ProtectedEpochSnapshotV1 {
            epoch: 2,
            record_digest: "b".repeat(64),
        };
        backend.compare_and_advance(Some(&first), &second).unwrap();
        assert_eq!(backend.read_latest().unwrap(), Some(second));
    }

    #[test]
    fn sealed_slot_refuses_a_tampered_payload() {
        let temp = TempDir::new().unwrap();
        let store = seat_store();
        let dir = make_dir_with_mode(temp.path(), "tamper-root", 0o700);
        let mut backend =
            SecureEnclaveProtectedEpochBackend::new(sealed_root_at(&dir, &store, "tamper-seat"));
        backend
            .compare_and_advance(
                None,
                &ProtectedEpochSnapshotV1 {
                    epoch: 1,
                    record_digest: "a".repeat(64),
                },
            )
            .unwrap();

        // Rewrite the payload on disk without re-sealing: the seal must no longer
        // verify, so the read fails closed rather than trusting the record.
        let slot = dir.join(PROTECTED_EPOCH_SLOT_FILE);
        let mut record: serde_json::Value =
            serde_json::from_slice(&fs::read(&slot).unwrap()).unwrap();
        record["payload"]["record_digest"] = serde_json::json!("b".repeat(64));
        fs::write(&slot, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(matches!(
            backend.read_latest(),
            Err(detail) if detail.contains("seal")
        ));
    }

    #[test]
    fn protected_root_refuses_a_non_0700_directory() {
        let temp = TempDir::new().unwrap();
        let store = seat_store();
        let dir = make_dir_with_mode(temp.path(), "loose-root", 0o755);
        let signer = attested_signer(&store, "loose-seat");
        let verification_key = signer.verification_key(0, 0);
        assert!(matches!(
            SealedProtectedRootV1::open(&dir, Arc::new(signer), verification_key),
            Err(EnclaveError::Filesystem(_))
        ));
    }

    #[test]
    fn sealed_journal_head_backend_enforces_anti_rollback_invariants() {
        let temp = TempDir::new().unwrap();
        let store = seat_store();
        let dir = make_dir_with_mode(temp.path(), "journal-root", 0o700);
        let mut backend =
            SecureEnclaveJournalHeadBackend::new(sealed_root_at(&dir, &store, "journal-seat"));
        let domain = "m1nd-authority-wal-head-v1";
        assert_eq!(
            backend.assurance(),
            ProtectedJournalHeadAssuranceV1::HardwareProtectedAttested
        );
        assert!(backend.read_latest(domain).unwrap().is_none());

        // The initial protected head must be the empty anchor.
        let bad_initial = ProtectedJournalHeadSnapshotV1::observed(domain, 1, Some("h".repeat(64)));
        assert!(backend
            .compare_and_advance(domain, None, &bad_initial)
            .is_err());

        let anchor = ProtectedJournalHeadSnapshotV1::observed(domain, 0, None);
        backend.compare_and_advance(domain, None, &anchor).unwrap();
        let next = ProtectedJournalHeadSnapshotV1::observed(domain, 1, Some("h".repeat(64)));
        backend
            .compare_and_advance(domain, Some(&anchor), &next)
            .unwrap();
        assert_eq!(backend.read_latest(domain).unwrap(), Some(next.clone()));

        // Skipping a sequence is refused.
        let skip = ProtectedJournalHeadSnapshotV1::observed(domain, 3, Some("z".repeat(64)));
        assert!(backend
            .compare_and_advance(domain, Some(&next), &skip)
            .is_err());
    }
}
