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

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use m1nd_control::autonomy::{
    IndependenceSpecV1, IMMUTABLE_FAILURE_DOMAINS, IMMUTABLE_VERIFIER_SEATS,
};

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
/// amendment (`docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md` §5). The single
/// source of truth lives in `m1nd-control` so the custody-ceremony receipt here
/// and the gate/autonomy receipts there name one literal; this crate re-exports
/// it. `m1nd-control::RATIFIED_CUSTODY_FLOORS` is the closed set validators check.
pub use m1nd_control::SECURE_ENCLAVE_CUSTODY_FLOOR_V1;

/// Stable module-level attestation identities. Re-attestation compares an opened
/// key's attestation against these. The production adapter reads the REAL
/// Security.framework attributes back via `SecKeyCopyAttributes` and proves Secure
/// Enclave residency (`kSecAttrTokenID` == `kSecAttrTokenIDSecureEnclave`) and EC
/// P-256 type (`kSecAttrKeyType` == `kSecAttrKeyTypeECSECPrimeRandom`) by
/// `CFEqual` against the framework's own constants, carrying the size the enclave
/// reports (`kSecAttrKeySizeInBits`) — attesting the KEY, not the request. The
/// access-control (presence) semantics are the store's seat class; that a
/// persisted key truly carries them is the owner's live conformance check.
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
    /// Canonical protected-root path this slot was sealed under. A slot sealed
    /// under one root cannot be replayed into another root sealed by the same key.
    root_binding: String,
    /// Organism/candidate context digest sealed into the slot for the same reason.
    context_digest: String,
    payload: serde_json::Value,
    /// Lowercase-hex enclave signature over
    /// `seal_message(domain, root_binding, context_digest, canonical(payload))`.
    seal: String,
}

/// A 0700, device/inode-pinned protected directory holding enclave-sealed slots.
/// Reads verify the seal through m1nd-control; the private key never leaves the
/// enclave. Shared by the config-epoch, runtime-epoch, and journal-head backends.
pub struct SealedProtectedRootV1 {
    root: PathBuf,
    root_binding: String,
    context_digest: String,
    pinned: DeviceInode,
    signer: Arc<dyn AuthoritySigner + Send + Sync>,
    verification_key: VerificationKeyV1,
}

impl SealedProtectedRootV1 {
    /// Open an existing 0700 directory, refuse symlinks/non-0700 modes, and pin
    /// its (device, inode). The verification key must match the signer identity.
    /// `context_digest` binds every sealed slot to this organism/candidate context
    /// so a slot cannot be replayed into another root sealed by the same key.
    pub fn open(
        root: impl AsRef<Path>,
        context_digest: impl Into<String>,
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
        let context_digest = context_digest.into();
        if context_digest.is_empty() {
            return Err(EnclaveError::Filesystem(
                "sealed protected root requires a non-empty context digest".to_owned(),
            ));
        }
        let root = root.as_ref().to_path_buf();
        let pinned = pin_owner_only_directory(&root)?;
        let root_binding = fs::canonicalize(&root)
            .map_err(|error| {
                EnclaveError::Filesystem(format!("canonicalize {}: {error}", root.display()))
            })?
            .to_string_lossy()
            .into_owned();
        Ok(Self {
            root,
            root_binding,
            context_digest,
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
        // Anti-replay: the slot must have been sealed under THIS root and context,
        // not moved in from another root sealed by the same key. Checked
        // explicitly here and again cryptographically below (the seal covers the
        // binding taken from `self`, so a moved slot fails the signature too).
        if record.root_binding != self.root_binding || record.context_digest != self.context_digest
        {
            return Err(EnclaveError::SealVerification(
                "sealed record binding (root/context) does not match this root".to_owned(),
            ));
        }
        let canonical = canonical_json(&record.payload)
            .map_err(|error| EnclaveError::SealVerification(error.to_string()))?;
        let message = seal_message(domain, &self.root_binding, &self.context_digest, &canonical);
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
        let message = seal_message(domain, &self.root_binding, &self.context_digest, &canonical);
        let seal = sign_authority_message(&message, &self.verification_key, self.signer.as_ref())
            .map_err(|error| EnclaveError::Sign(error.to_string()))?;
        let record = SealedRecordV1 {
            schema: SEALED_RECORD_SCHEMA.to_owned(),
            domain: domain.to_owned(),
            issuer: self.verification_key.subject_id.clone(),
            key_id: self.verification_key.key_id.clone(),
            algorithm: self.verification_key.algorithm.clone(),
            root_binding: self.root_binding.clone(),
            context_digest: self.context_digest.clone(),
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

fn seal_message(
    domain: &str,
    root_binding: &str,
    context_digest: &str,
    canonical_payload: &[u8],
) -> Vec<u8> {
    let fields = [
        domain.as_bytes(),
        root_binding.as_bytes(),
        context_digest.as_bytes(),
        canonical_payload,
    ];
    let capacity = SEAL_MESSAGE_PREFIX.len() + fields.iter().map(|f| f.len() + 8).sum::<usize>();
    let mut message = Vec::with_capacity(capacity);
    message.extend_from_slice(SEAL_MESSAGE_PREFIX);
    for field in fields {
        message.extend_from_slice(&(field.len() as u64).to_be_bytes());
        message.extend_from_slice(field);
    }
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

// ===========================================================================
// Custody ceremony receipt (amendment G9-A1).
//
// The owner's explicit ceremony seals, BEFORE any quorum decision: the custody
// floor identifier, the key-custody-vs-anti-rollback attestation distinction,
// the four distinct enclave verifier-seat public keys with their failure
// domains, the human owner's biometric seat public key (owner_signature, never a
// voting seat), and the independence-spec and constitution digests the quorum
// binds to. THIS ceremony receipt is fail-closed on the declared custody floor.
// The gate receipt (`m1nd-control::release::GateReceiptCoreV1`) and the autonomy
// activation receipt (`m1nd-control::autonomy::AutonomyActivationReceiptCoreV1`)
// now carry `custody_floor` too, validated against the closed
// `m1nd-control::RATIFIED_CUSTODY_FLOORS` set across all three canonical mirrors
// (Rust/Python/Node). BLOCKING ORDER (G9-A1 ratification): the threading
// prerequisite is SATISFIED — the follow-up merged (feat/g9-custody-floor-
// threading), so no receipt can claim the floor without carrying it, and the
// owner's custody ceremony is no longer blocked by it.
// ===========================================================================

pub const ENCLAVE_CUSTODY_CEREMONY_SCHEMA: &str = "m1nd-enclave-custody-ceremony-receipt-v1";
const ENCLAVE_CUSTODY_CEREMONY_SEAL_DOMAIN: &str = "m1nd-enclave-custody-ceremony-v1";
const ENCLAVE_CUSTODY_CEREMONY_SLOT_FILE: &str = "custody-ceremony.sealed.json";

/// The honest attestation distinction the amendment requires on the record: what
/// the enclave really provides (key custody) versus what remains filesystem
/// strength (anti-rollback), plus the declared non-claims.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CustodyAttestationDistinctionV1 {
    pub key_custody: String,
    pub anti_rollback: String,
    pub non_claims: Vec<String>,
}

impl CustodyAttestationDistinctionV1 {
    pub fn secure_enclave_single_host() -> Self {
        Self {
            key_custody: "hardware-secure-enclave-non-exportable-p256".to_owned(),
            anti_rollback: "filesystem-strength-0700-single-host".to_owned(),
            non_claims: vec![
                "no multi-host custody".to_owned(),
                "no hardware anti-rollback under physical attack".to_owned(),
                "no resistance to a root-level compromise of this host".to_owned(),
            ],
        }
    }
}

/// One enclave-custodied verifier seat sealed by the ceremony.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CeremonyVerifierSeatV1 {
    pub principal_id: String,
    pub key_id: String,
    pub failure_domain: String,
    /// 65-byte uncompressed SEC1 P-256 public key (lowercase hex). Enclave-custodied.
    pub public_key: String,
    /// Lineage: the `bound_context_digest` of the enclave permit that provisioned
    /// this seat, sealed into the receipt so a seat cannot be lifted from another
    /// ceremony's provisioning.
    pub bound_context_digest: String,
}

/// The sealed custody-ceremony receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnclaveCustodyCeremonyReceiptV1 {
    pub schema: String,
    pub custody_floor: String,
    pub attestation: CustodyAttestationDistinctionV1,
    pub verifier_seats: Vec<CeremonyVerifierSeatV1>,
    /// The human owner's biometric seat public key. This is the owner_signature
    /// authority present even under AgentQuorum, NOT a voting quorum seat.
    pub owner_biometric_seat_public_key: String,
    pub independence_spec_digest: String,
    pub constitution_digest: String,
    pub sealed_at: u64,
}

impl EnclaveCustodyCeremonyReceiptV1 {
    /// Fail-closed validation: the declared custody floor, exactly
    /// IMMUTABLE_VERIFIER_SEATS distinct enclave verifier seats over at least
    /// IMMUTABLE_FAILURE_DOMAINS distinct failure domains, and an owner biometric
    /// seat that is never also one of the voting seats.
    pub fn validate(&self) -> Result<(), EnclaveError> {
        if self.schema != ENCLAVE_CUSTODY_CEREMONY_SCHEMA {
            return Err(EnclaveError::Ceremony(
                "unsupported custody ceremony schema".to_owned(),
            ));
        }
        if self.custody_floor != SECURE_ENCLAVE_CUSTODY_FLOOR_V1 {
            return Err(EnclaveError::Ceremony(format!(
                "custody ceremony must declare custody_floor '{SECURE_ENCLAVE_CUSTODY_FLOOR_V1}'"
            )));
        }
        if self.verifier_seats.len() != usize::from(IMMUTABLE_VERIFIER_SEATS) {
            return Err(EnclaveError::Ceremony(format!(
                "custody ceremony must seal exactly {IMMUTABLE_VERIFIER_SEATS} verifier seats"
            )));
        }
        let mut principals = BTreeSet::new();
        let mut key_ids = BTreeSet::new();
        let mut public_keys = BTreeSet::new();
        let mut failure_domains = BTreeSet::new();
        for seat in &self.verifier_seats {
            require_uncompressed_sec1_hex(&seat.public_key)?;
            require_lowercase_sha256_hex("seat.bound_context_digest", &seat.bound_context_digest)?;
            if seat.principal_id.is_empty()
                || seat.key_id.is_empty()
                || seat.failure_domain.is_empty()
            {
                return Err(EnclaveError::Ceremony(
                    "verifier seat has an empty field".to_owned(),
                ));
            }
            if !principals.insert(seat.principal_id.as_str()) {
                return Err(EnclaveError::Ceremony(
                    "duplicate verifier principal".to_owned(),
                ));
            }
            if !key_ids.insert(seat.key_id.as_str()) {
                return Err(EnclaveError::Ceremony(
                    "duplicate verifier key id".to_owned(),
                ));
            }
            if !public_keys.insert(seat.public_key.as_str()) {
                return Err(EnclaveError::Ceremony(
                    "duplicate verifier public key — each seat needs a distinct enclave key"
                        .to_owned(),
                ));
            }
            failure_domains.insert(seat.failure_domain.as_str());
        }
        if failure_domains.len() < usize::from(IMMUTABLE_FAILURE_DOMAINS) {
            return Err(EnclaveError::Ceremony(format!(
                "custody ceremony must span at least {IMMUTABLE_FAILURE_DOMAINS} distinct failure domains"
            )));
        }
        require_uncompressed_sec1_hex(&self.owner_biometric_seat_public_key)?;
        if public_keys.contains(self.owner_biometric_seat_public_key.as_str()) {
            return Err(EnclaveError::Ceremony(
                "the owner biometric seat must not also be a voting quorum seat".to_owned(),
            ));
        }
        require_lowercase_sha256_hex("independence_spec_digest", &self.independence_spec_digest)?;
        require_lowercase_sha256_hex("constitution_digest", &self.constitution_digest)?;
        Ok(())
    }

    /// Bind the sealed ceremony to the exact IndependenceSpecV1 the quorum uses:
    /// the spec digest and the set of (principal, key, failure-domain) seats must
    /// match, so the four seats are sealed BEFORE any quorum vote is counted.
    pub fn bind_independence_spec(&self, spec: &IndependenceSpecV1) -> Result<(), EnclaveError> {
        self.validate()?;
        if self.independence_spec_digest != spec.independence_spec_digest {
            return Err(EnclaveError::Ceremony(
                "sealed independence-spec digest does not match the presented spec".to_owned(),
            ));
        }
        let ceremony: BTreeSet<(&str, &str, &str)> = self
            .verifier_seats
            .iter()
            .map(|seat| {
                (
                    seat.principal_id.as_str(),
                    seat.key_id.as_str(),
                    seat.failure_domain.as_str(),
                )
            })
            .collect();
        let voting: BTreeSet<(&str, &str, &str)> = spec
            .core
            .voting_verifiers
            .iter()
            .map(|seat| {
                (
                    seat.principal_id.as_str(),
                    seat.key_id.as_str(),
                    seat.failure_domain.as_str(),
                )
            })
            .collect();
        if ceremony != voting {
            return Err(EnclaveError::Ceremony(
                "sealed verifier seats do not match the independence spec's voting verifiers"
                    .to_owned(),
            ));
        }
        Ok(())
    }
}

impl SealedProtectedRootV1 {
    /// Validate and enclave-seal the custody ceremony receipt.
    pub fn seal_custody_ceremony(
        &self,
        receipt: &EnclaveCustodyCeremonyReceiptV1,
    ) -> Result<(), EnclaveError> {
        receipt.validate()?;
        let value = serde_json::to_value(receipt)
            .map_err(|error| EnclaveError::Filesystem(error.to_string()))?;
        self.write_slot(
            ENCLAVE_CUSTODY_CEREMONY_SEAL_DOMAIN,
            ENCLAVE_CUSTODY_CEREMONY_SLOT_FILE,
            value,
        )
    }

    /// Read the sealed custody ceremony, verifying both the enclave seal and the
    /// fail-closed envelope invariants.
    pub fn read_custody_ceremony(
        &self,
    ) -> Result<Option<EnclaveCustodyCeremonyReceiptV1>, EnclaveError> {
        match self.read_slot(
            ENCLAVE_CUSTODY_CEREMONY_SEAL_DOMAIN,
            ENCLAVE_CUSTODY_CEREMONY_SLOT_FILE,
        )? {
            Some(value) => {
                let receipt: EnclaveCustodyCeremonyReceiptV1 = serde_json::from_value(value)
                    .map_err(|error| EnclaveError::SealVerification(error.to_string()))?;
                receipt.validate()?;
                Ok(Some(receipt))
            }
            None => Ok(None),
        }
    }
}

fn require_uncompressed_sec1_hex(value: &str) -> Result<(), EnclaveError> {
    let is_lower_hex = value.len() == 130
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if !is_lower_hex || !value.starts_with("04") {
        return Err(EnclaveError::PublicKeyEncoding(
            "seat public key is not 65-byte uncompressed SEC1 lowercase hex".to_owned(),
        ));
    }
    Ok(())
}

fn require_lowercase_sha256_hex(field: &str, value: &str) -> Result<(), EnclaveError> {
    let is_digest = value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if !is_digest {
        return Err(EnclaveError::Ceremony(format!(
            "{field} is not a lowercase sha-256 hex digest"
        )));
    }
    Ok(())
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
    Ceremony(String),
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
            Self::Ceremony(detail) => write!(formatter, "custody ceremony: {detail}"),
        }
    }
}

impl Error for EnclaveError {}

// ===========================================================================
// Real Security.framework key store (macOS).
//
// This is the production `SecureEnclaveKeyStoreV1` adapter. It is fully
// implemented and compile-verified on macOS, but NOT_RUN in CI and never
// exercised by an agent.
//
// HARD PREREQUISITE (not optional ceremony scope): persistence requires the
// calling binary to be codesigned with a `KeychainAccessGroups` entitlement.
// `provision` files the key into the data-protection keychain
// (`Location::DataProtectionKeychain`) — the ONLY keychain a Secure Enclave key
// can be made permanent in — and both that write and the `resolve_persisted_key`
// query are scoped to it (`kSecUseDataProtectionKeychain`). An unsigned or
// unentitled binary cannot persist or resolve the key at all, so `open`/`sign`
// would fail closed. This is a precondition of the owner's ceremony, not a
// runtime nicety.
//
// Custody is keyed by `kSecAttrLabel` (the high-level `GenerateKeyOptions` exposes
// no `kSecAttrApplicationTag`): `provision` sets a distinct label per key and
// refuses a label already present (never-open-or-create); `open` resolves by that
// label and fails closed on zero or more-than-one match. `provision` attests the
// created key by reading its real attributes back; `open` attests token/type/size
// via `SecKeyCopyAttributes`; `sign` signs through `SecKeyCreateSignature`, and
// for the biometric owner seat the key's `kSecAccessControl` gates it on user
// presence only the owner's live ceremony can satisfy. The unit tests exercise the
// custody path through the software mock; the real persistence proof (provision ->
// process restart -> open/sign on a signed, entitled binary) runs only in that
// ceremony.
// ===========================================================================

/// Production Secure Enclave key store over Apple's Security.framework. A store is
/// bound to ONE seat class (`access_control`): the agent-held verifier/quorum seat
/// or the owner's biometric seat. `provision` refuses a permit for a different
/// class, and `open` attests the resolved key against this class.
///
/// Custody is keyed by `kSecAttrLabel`, not `kSecAttrApplicationTag`:
/// `GenerateKeyOptions` (the only key-creation surface the high-level crate
/// exposes) files the key under a label via `set_label`, and the search resolves
/// it by the same label. A distinct label per `key_id` is the custody handle, and
/// ANY existing item sharing that label makes provision AND open fail closed
/// (never-open-or-create; ambiguous custody is refused).
pub struct SecurityFrameworkEnclaveKeyStore {
    keychain_label_prefix: String,
    subject_id: String,
    access_control: EnclaveAccessControlV1,
}

impl SecurityFrameworkEnclaveKeyStore {
    pub fn new(
        keychain_label_prefix: impl Into<String>,
        subject_id: impl Into<String>,
        access_control: EnclaveAccessControlV1,
    ) -> Self {
        Self {
            keychain_label_prefix: keychain_label_prefix.into(),
            subject_id: subject_id.into(),
            access_control,
        }
    }

    /// The `kSecAttrLabel` this store files `key_id` under (and resolves it by).
    fn keychain_label(&self, key_id: &str) -> String {
        format!("{}.{key_id}", self.keychain_label_prefix)
    }

    fn access_control_flags(
        access_control: EnclaveAccessControlV1,
    ) -> Result<security_framework::access_control::SecAccessControl, EnclaveError> {
        use core_foundation::base::CFOptionFlags;
        use security_framework::access_control::{ProtectionMode, SecAccessControl};
        // Apple's kSecAccessControl flag values: private-key usage (1 << 30) and
        // user presence (1 << 0). The agent path never provisions the biometric
        // seat (guarded upstream by provision_agent_enclave_seat).
        const PRIVATE_KEY_USAGE: CFOptionFlags = 1 << 30;
        const USER_PRESENCE: CFOptionFlags = 1 << 0;
        let flags = match access_control {
            EnclaveAccessControlV1::PrivateKeyUsageNonExportable => PRIVATE_KEY_USAGE,
            EnclaveAccessControlV1::UserPresenceBiometricNonExportable => {
                PRIVATE_KEY_USAGE | USER_PRESENCE
            }
        };
        // Pin the protection class to WhenUnlockedThisDeviceOnly (Apple's guidance
        // for Secure Enclave keys) instead of the crate default WhenUnlocked
        // (`create_with_flags` == `create_with_protection(None, …)`). The key is
        // already hardware-non-exportable, so the real-world delta is small, but a
        // now-persisting key should not be nominally weaker than the guidance.
        SecAccessControl::create_with_protection(
            Some(ProtectionMode::AccessibleWhenUnlockedThisDeviceOnly),
            flags,
        )
        .map_err(|error| EnclaveError::Provisioning(error.to_string()))
    }

    /// Resolve the single persisted enclave key filed under this store's
    /// `kSecAttrLabel` for `key_id` via `SecItemCopyMatching` (the high-level
    /// `ItemSearchOptions` wraps it), returning the private-key reference. `None`
    /// means no such key exists; more than one match fails closed — custody must
    /// never be ambiguous. This one query is also the production
    /// never-open-or-create duplicate guard.
    ///
    /// The query is scoped to the data-protection keychain
    /// (`ignore_legacy_keychains`, i.e. `kSecUseDataProtectionKeychain`) so it sees
    /// the SAME scope `provision` writes into via `Location::DataProtectionKeychain`
    /// — Secure Enclave keys live only there. Without matching scope the query
    /// would silently never find the provisioned key.
    fn resolve_persisted_key(
        &self,
        key_id: &str,
    ) -> Result<Option<security_framework::key::SecKey>, EnclaveError> {
        use security_framework::item::{
            ItemClass, ItemSearchOptions, KeyClass, Limit, Reference, SearchResult,
        };
        let label = self.keychain_label(key_id);
        let results = ItemSearchOptions::new()
            .class(ItemClass::key())
            .key_class(KeyClass::private())
            // SENTINEL: this ignore_legacy_keychains() (kSecUseDataProtectionKeychain)
            // MUST stay paired with provision's set_location(DataProtectionKeychain).
            // Both are OSX_10_15-gated; if a refactor drops the provision-side location
            // but keeps this, the feature could silently no-op the query scope.
            .ignore_legacy_keychains()
            .label(&label)
            .load_refs(true)
            .limit(Limit::Max(2))
            .search()
            .map_err(|error| {
                EnclaveError::Open(format!("keychain item query for label '{label}': {error}"))
            })?;
        let mut resolved = None;
        for result in results {
            if let SearchResult::Ref(Reference::Key(key)) = result {
                if resolved.is_some() {
                    return Err(EnclaveError::Open(format!(
                        "ambiguous enclave custody: more than one key filed under label '{label}'"
                    )));
                }
                resolved = Some(key);
            }
        }
        Ok(resolved)
    }

    /// Read the resolved key's REAL attributes via `SecKeyCopyAttributes` and prove
    /// its Secure Enclave residency and EC P-256 type by `CFEqual` against the
    /// framework's own constants — attesting the KEY, not the request. The key size
    /// is the value the enclave actually reports. The `access_control` class is the
    /// store's seat class (a store is bound to one seat class); the live
    /// `kSecAccessControl` conformance check — that the persisted key truly carries
    /// the presence semantics — is the owner's ceremony step, not readable here.
    fn attest_persisted_key(
        &self,
        key: &security_framework::key::SecKey,
    ) -> Result<EnclaveKeyAttestationV1, EnclaveError> {
        use core_foundation::base::{CFEqual, CFGetTypeID, TCFType, ToVoid};
        use core_foundation::number::CFNumber;
        use security_framework_sys::item::{
            kSecAttrKeySizeInBits, kSecAttrKeyType, kSecAttrKeyTypeECSECPrimeRandom,
            kSecAttrTokenID, kSecAttrTokenIDSecureEnclave,
        };

        let attributes = key.attributes();

        let token = attributes
            .find(unsafe { kSecAttrTokenID.to_void() })
            .ok_or_else(|| {
                EnclaveError::Open("enclave key attributes missing kSecAttrTokenID".to_owned())
            })?;
        if unsafe { CFEqual(token.cast(), kSecAttrTokenIDSecureEnclave.cast()) } == 0 {
            return Err(EnclaveError::AttestationMismatch {
                expected: format!("token {SECURE_ENCLAVE_TOKEN_ID}"),
                observed: "key is not resident in the Secure Enclave token".to_owned(),
            });
        }

        let key_type = attributes
            .find(unsafe { kSecAttrKeyType.to_void() })
            .ok_or_else(|| {
                EnclaveError::Open("enclave key attributes missing kSecAttrKeyType".to_owned())
            })?;
        if unsafe { CFEqual(key_type.cast(), kSecAttrKeyTypeECSECPrimeRandom.cast()) } == 0 {
            return Err(EnclaveError::AttestationMismatch {
                expected: SECURE_ENCLAVE_KEY_TYPE_EC_PRIME_RANDOM.to_owned(),
                observed: "key type is not EC prime random".to_owned(),
            });
        }

        let size = attributes
            .find(unsafe { kSecAttrKeySizeInBits.to_void() })
            .ok_or_else(|| {
                EnclaveError::Open(
                    "enclave key attributes missing kSecAttrKeySizeInBits".to_owned(),
                )
            })?;
        // Verify the CF type before wrapping it: the only otherwise-unchecked
        // conversion in the attestation. A non-CFNumber value fails closed rather
        // than being reinterpreted as a number.
        if unsafe { CFGetTypeID(size.cast()) } != CFNumber::type_id() {
            return Err(EnclaveError::Open(
                "enclave key size attribute is not a CFNumber".to_owned(),
            ));
        }
        let key_size_bits = unsafe { CFNumber::wrap_under_get_rule(size.cast()) }
            .to_i32()
            .and_then(|bits| u32::try_from(bits).ok())
            .ok_or_else(|| {
                EnclaveError::Open("enclave key size is not a non-negative integer".to_owned())
            })?;

        Ok(EnclaveKeyAttestationV1 {
            token_id: SECURE_ENCLAVE_TOKEN_ID.to_owned(),
            key_type: SECURE_ENCLAVE_KEY_TYPE_EC_PRIME_RANDOM.to_owned(),
            key_size_bits,
            access_control: self.access_control,
        })
    }

    /// The uncompressed SEC1 public point of `key` (65 bytes, `0x04 || X || Y`).
    fn public_key_sec1(key: &security_framework::key::SecKey) -> Result<Vec<u8>, EnclaveError> {
        let public_key = key.public_key().ok_or_else(|| {
            EnclaveError::Open("enclave key has no public representation".to_owned())
        })?;
        let external = public_key.external_representation().ok_or_else(|| {
            EnclaveError::Open("enclave public key has no external SEC1 representation".to_owned())
        })?;
        let public_key_sec1 = external.bytes().to_vec();
        require_uncompressed_sec1(&public_key_sec1)?;
        Ok(public_key_sec1)
    }

    /// Assemble the opened-key record from a resolved private key: its real
    /// attestation and its public SEC1 point.
    fn opened_from_key(
        &self,
        key_id: &str,
        key: &security_framework::key::SecKey,
    ) -> Result<EnclaveOpenedKeyV1, EnclaveError> {
        Ok(EnclaveOpenedKeyV1 {
            key_id: key_id.to_owned(),
            subject_id: self.subject_id.clone(),
            public_key_sec1: Self::public_key_sec1(key)?,
            attestation: self.attest_persisted_key(key)?,
            // The platform reference is the `kSecAttrLabel` the key is filed under.
            platform_ref: self.keychain_label(key_id),
        })
    }
}

impl SecureEnclaveKeyStoreV1 for SecurityFrameworkEnclaveKeyStore {
    fn provision(
        &self,
        permit: &EnclaveProvisioningPermitV1,
    ) -> Result<EnclaveOpenedKeyV1, EnclaveError> {
        use security_framework::item::Location;
        use security_framework::key::{GenerateKeyOptions, KeyType, SecKey, Token};

        // A store is bound to one seat class; refuse a permit for a different one so
        // an agent store cannot mint the biometric owner seat and vice versa.
        if permit.access_control != self.access_control {
            return Err(EnclaveError::Provisioning(
                "permit access-control class does not match this key store's seat class".to_owned(),
            ));
        }
        // Production never-open-or-create duplicate guard: refuse a label already
        // present in the Keychain rather than silently adopting or shadowing a key.
        if self.resolve_persisted_key(&permit.key_id)?.is_some() {
            return Err(EnclaveError::Provisioning(format!(
                "an enclave key already exists under label '{}'; \
                 provisioning is never open-or-create",
                self.keychain_label(&permit.key_id)
            )));
        }

        let access_control = Self::access_control_flags(permit.access_control)?;
        let mut options = GenerateKeyOptions::default();
        options
            .set_key_type(KeyType::ec())
            .set_size_in_bits(SECURE_ENCLAVE_KEY_SIZE_BITS)
            .set_token(Token::SecureEnclave)
            // PERSIST the key: without a location the created key is EPHEMERAL
            // (kSecAttrIsPermanent is only emitted when a location is set), so it
            // would never reach the Keychain and `open`/`sign` could never resolve
            // it. Secure Enclave keys can only be made permanent in the
            // data-protection keychain — the same scope `resolve_persisted_key`
            // queries. This requires a codesigned binary with a KeychainAccessGroups
            // entitlement (owner-ceremony prerequisite; see the module boundary note).
            // Note: to_dictionary also marks the PUBLIC key permanent, so a public-key
            // item may persist under the same kSecAttrLabel. resolve_persisted_key is
            // immune (it filters KeyClass::private); any future label-based maintenance
            // MUST filter by key class or it will see two items.
            .set_location(Location::DataProtectionKeychain)
            .set_label(self.keychain_label(&permit.key_id))
            .set_access_control(access_control);
        let private_key =
            SecKey::new(&options).map_err(|error| EnclaveError::Provisioning(error.to_string()))?;
        // Attest the KEY, not the request: read the created key's real token/type/
        // size back through SecKeyCopyAttributes (proving Secure Enclave residency
        // and EC P-256 against the framework's own constants). The size invariant is
        // then confirmed against the custody floor's pinned expectation.
        let opened = self.opened_from_key(&permit.key_id, &private_key)?;
        if opened.attestation.key_size_bits != SECURE_ENCLAVE_KEY_SIZE_BITS {
            return Err(EnclaveError::Provisioning(format!(
                "created key size {} != {SECURE_ENCLAVE_KEY_SIZE_BITS}",
                opened.attestation.key_size_bits
            )));
        }
        Ok(opened)
    }

    fn open(&self, key_id: &str) -> Result<EnclaveOpenedKeyV1, EnclaveError> {
        // Resolve the persisted key by label (SecItemCopyMatching) and attest the
        // resolved key's real attributes. No biometry is required to read a key
        // reference and its metadata; signing (below) is what user presence gates.
        // A missing key fails closed.
        let key = self.resolve_persisted_key(key_id)?.ok_or_else(|| {
            EnclaveError::Open(format!(
                "no persisted enclave key filed under label '{}'",
                self.keychain_label(key_id)
            ))
        })?;
        self.opened_from_key(key_id, &key)
    }

    fn sign(&self, opened: &EnclaveOpenedKeyV1, message: &[u8]) -> Result<Vec<u8>, EnclaveError> {
        use security_framework::key::Algorithm;

        let key = self.resolve_persisted_key(&opened.key_id)?.ok_or_else(|| {
            EnclaveError::Sign(format!(
                "no persisted enclave key filed under label '{}'",
                self.keychain_label(&opened.key_id)
            ))
        })?;
        // Bind the resolved key to the opened/attested key: the public material must
        // match, so a key swapped under the label after `open` cannot sign in its name.
        if Self::public_key_sec1(&key)? != opened.public_key_sec1 {
            return Err(EnclaveError::Sign(
                "resolved enclave key public material does not match the opened key".to_owned(),
            ));
        }
        // ECDSA over SHA-256 of the message, X9.62 DER — the exact scheme
        // m1nd-control's verifier expects. The enclave may return high-S DER;
        // m1nd-control normalizes it to canonical low-S downstream. For the biometric
        // owner seat, the key's kSecAccessControl gates this call on Touch ID user
        // presence: the agent cannot provoke a signature without the hardware and
        // biometry the owner alone holds — that is the live ceremony, NOT_RUN here.
        key.create_signature(Algorithm::ECDSASignatureMessageX962SHA256, message)
            .map_err(|error| EnclaveError::Sign(error.to_string()))
    }
}

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
    fn persisted_key_round_trips_from_provision_through_reopen() {
        // The mock persists keys in-process, so it proves the LOGICAL custody
        // contract: a key provisioned once is resolved back by the same key_id and
        // signs. The REAL adapter's persistence — provision, then resolve the key
        // after a PROCESS RESTART out of the data-protection Keychain — is exercised
        // only by the owner's ceremony on a codesigned, entitled binary; a mock
        // cannot model the ephemeral-vs-permanent Keychain distinction that the
        // production `set_location(DataProtectionKeychain)` fix addresses.
        let store: Arc<dyn SecureEnclaveKeyStoreV1> = Arc::new(MockEnclaveKeyStore::new(
            EnclaveAccessControlV1::PrivateKeyUsageNonExportable,
        ));
        let provisioned =
            provision_agent_enclave_seat(store.as_ref(), &agent_permit("persist-seat")).unwrap();

        // Re-resolve the SAME key by id (the open-by-label path) and confirm identity.
        let reopened = store.open("persist-seat").unwrap();
        assert_eq!(reopened.public_key_sec1, provisioned.public_key_sec1);
        assert_eq!(reopened.attestation, provisioned.attestation);

        // The reopened key signs and verifies end to end through m1nd-control.
        let expected = EnclaveKeyAttestationV1::canonical(
            EnclaveAccessControlV1::PrivateKeyUsageNonExportable,
        );
        let signer =
            SecureEnclaveSigner::open_attested(Arc::clone(&store), "persist-seat", &expected)
                .unwrap();
        let verification_key = signer.verification_key(0, 0);
        let crypto = EnclaveBackedWalRecordCrypto::new(Arc::new(signer), verification_key).unwrap();
        let message = b"persisted-enclave-round-trip";
        let signature = crypto.sign(message).unwrap();
        crypto.verify(message, &signature).unwrap();

        // A key that was never provisioned fails closed on open.
        assert!(store.open("never-provisioned").is_err());
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
        SealedProtectedRootV1::open(
            dir,
            "test-organism-context",
            Arc::new(signer),
            verification_key,
        )
        .unwrap()
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
            SealedProtectedRootV1::open(
                &dir,
                "test-organism-context",
                Arc::new(signer),
                verification_key
            ),
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

    #[test]
    fn sealed_slot_refuses_replay_across_roots_and_contexts() {
        let temp = TempDir::new().unwrap();
        let store = seat_store();
        // ONE enclave key; two 0700 roots. A slot sealed in root A must not verify
        // when moved into root B sealed by the same key.
        let signer = attested_signer(&store, "replay-seat");
        let key = signer.verification_key(0, 0);
        let signer: Arc<dyn AuthoritySigner + Send + Sync> = Arc::new(signer);
        let root_a_dir = make_dir_with_mode(temp.path(), "root-a", 0o700);
        let root_b_dir = make_dir_with_mode(temp.path(), "root-b", 0o700);

        let root_a =
            SealedProtectedRootV1::open(&root_a_dir, "ctx-1", Arc::clone(&signer), key.clone())
                .unwrap();
        let mut backend_a = SecureEnclaveProtectedEpochBackend::new(root_a);
        backend_a
            .compare_and_advance(
                None,
                &ProtectedEpochSnapshotV1 {
                    epoch: 1,
                    record_digest: "a".repeat(64),
                },
            )
            .unwrap();

        // Move the sealed slot file into root B and read it there.
        fs::copy(
            root_a_dir.join(PROTECTED_EPOCH_SLOT_FILE),
            root_b_dir.join(PROTECTED_EPOCH_SLOT_FILE),
        )
        .unwrap();
        let root_b =
            SealedProtectedRootV1::open(&root_b_dir, "ctx-1", Arc::clone(&signer), key.clone())
                .unwrap();
        assert!(SecureEnclaveProtectedEpochBackend::new(root_b)
            .read_latest()
            .is_err());

        // Same root, different sealed context is refused too.
        let root_a_wrong_ctx =
            SealedProtectedRootV1::open(&root_a_dir, "ctx-2", signer, key).unwrap();
        assert!(SecureEnclaveProtectedEpochBackend::new(root_a_wrong_ctx)
            .read_latest()
            .is_err());
    }

    fn seat_pubkey(seed: &str) -> String {
        let signing = SigningKey::from_slice(&scalar_for(seed)).unwrap();
        hex_lower(signing.verifying_key().to_encoded_point(false).as_bytes())
    }

    fn ceremony_seat(id: &str, domain: &str) -> CeremonyVerifierSeatV1 {
        CeremonyVerifierSeatV1 {
            principal_id: format!("principal-{id}"),
            key_id: id.to_owned(),
            failure_domain: domain.to_owned(),
            public_key: seat_pubkey(id),
            bound_context_digest: "e".repeat(64),
        }
    }

    fn ceremony_receipt() -> EnclaveCustodyCeremonyReceiptV1 {
        EnclaveCustodyCeremonyReceiptV1 {
            schema: ENCLAVE_CUSTODY_CEREMONY_SCHEMA.to_owned(),
            custody_floor: SECURE_ENCLAVE_CUSTODY_FLOOR_V1.to_owned(),
            attestation: CustodyAttestationDistinctionV1::secure_enclave_single_host(),
            verifier_seats: vec![
                ceremony_seat("seat-a", "provider-a/model-a/runtime-a"),
                ceremony_seat("seat-b", "provider-b/model-b/runtime-b"),
                ceremony_seat("seat-c", "provider-c/model-c/runtime-c"),
                ceremony_seat("seat-d", "provider-d/model-d/runtime-d"),
            ],
            owner_biometric_seat_public_key: seat_pubkey("owner-biometric"),
            independence_spec_digest: "a".repeat(64),
            constitution_digest: "b".repeat(64),
            sealed_at: 1_000,
        }
    }

    fn independence_spec_matching(receipt: &EnclaveCustodyCeremonyReceiptV1) -> IndependenceSpecV1 {
        use m1nd_control::autonomy::{
            IndependenceSpecCoreV1, VerifierSeatV1, IMMUTABLE_QUORUM_THRESHOLD,
            INDEPENDENCE_SPEC_SCHEMA,
        };
        let voting_verifiers = receipt
            .verifier_seats
            .iter()
            .map(|seat| VerifierSeatV1 {
                principal_id: seat.principal_id.clone(),
                key_id: seat.key_id.clone(),
                failure_domain: seat.failure_domain.clone(),
                parent_session_context_digest: "c".repeat(64),
            })
            .collect();
        IndependenceSpecV1 {
            schema: INDEPENDENCE_SPEC_SCHEMA.to_owned(),
            core: IndependenceSpecCoreV1 {
                constitution_epoch: 1,
                voting_verifiers,
                quorum_threshold: IMMUTABLE_QUORUM_THRESHOLD,
                minimum_failure_domains: IMMUTABLE_FAILURE_DOMAINS,
                blind_isolation_policy_digest: "d".repeat(64),
                nonvoting_sentinel_id: "sentinel".to_owned(),
                proposer_executor_nonvoting: true,
                sentinel_nonvoting: true,
            },
            independence_spec_digest: receipt.independence_spec_digest.clone(),
        }
    }

    #[test]
    fn custody_ceremony_validates_and_refuses_a_broken_envelope() {
        ceremony_receipt().validate().unwrap();

        // A G9 receipt without the declared custody floor fails closed.
        let mut no_floor = ceremony_receipt();
        no_floor.custody_floor = "software".to_owned();
        assert!(matches!(
            no_floor.validate(),
            Err(EnclaveError::Ceremony(_))
        ));

        // Not exactly four seats.
        let mut three = ceremony_receipt();
        three.verifier_seats.pop();
        assert!(three.validate().is_err());

        // Two seats sharing one enclave key are refused.
        let mut duplicate_key = ceremony_receipt();
        duplicate_key.verifier_seats[1].public_key =
            duplicate_key.verifier_seats[0].public_key.clone();
        assert!(duplicate_key.validate().is_err());

        // Fewer than three distinct failure domains.
        let mut two_domains = ceremony_receipt();
        two_domains.verifier_seats[2].failure_domain =
            two_domains.verifier_seats[0].failure_domain.clone();
        two_domains.verifier_seats[3].failure_domain =
            two_domains.verifier_seats[1].failure_domain.clone();
        assert!(two_domains.validate().is_err());

        // The owner biometric seat must never also be a voting seat.
        let mut owner_votes = ceremony_receipt();
        owner_votes.owner_biometric_seat_public_key =
            owner_votes.verifier_seats[0].public_key.clone();
        assert!(owner_votes.validate().is_err());
    }

    #[test]
    fn custody_ceremony_seals_reads_back_and_binds_to_the_independence_spec() {
        let temp = TempDir::new().unwrap();
        let store = seat_store();
        let dir = make_dir_with_mode(temp.path(), "ceremony-root", 0o700);
        let root = sealed_root_at(&dir, &store, "ceremony-seat");
        let receipt = ceremony_receipt();
        root.seal_custody_ceremony(&receipt).unwrap();
        assert_eq!(root.read_custody_ceremony().unwrap(), Some(receipt.clone()));

        // Binds to the matching spec; a mismatched seat set is refused.
        let spec = independence_spec_matching(&receipt);
        receipt.bind_independence_spec(&spec).unwrap();
        let mut mismatched = spec.clone();
        mismatched.core.voting_verifiers[0].key_id = "unexpected-key".to_owned();
        assert!(receipt.bind_independence_spec(&mismatched).is_err());
    }
}
