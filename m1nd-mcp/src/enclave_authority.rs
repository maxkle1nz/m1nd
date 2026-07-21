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
use std::sync::Arc;

use m1nd_control::{
    sign_authority_message, verify_authority_message_signature, AuthoritySigner,
    AuthoritySignerError, CryptographicIntegrity, IdentityStatus, OpaqueSignature,
    VerificationKeyV1, ECDSA_P256_SHA256_X962_ALGORITHM,
};

use crate::authority_wal::{AuthorityWalCryptoAssurance, AuthorityWalRecordCrypto};

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
}
