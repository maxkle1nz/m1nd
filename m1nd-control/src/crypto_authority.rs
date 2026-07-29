//! Cryptographic authority verification for the G2 control plane.
//!
//! Structural G1 validation remains available for parsing and diagnostics, but
//! callers that cross an authority boundary must use the functions in this
//! module and require a verified [`CryptographicIntegrity`]. Signatures
//! cover canonical JSON with the signature field removed, wrapped in a
//! length-delimited domain separator. Verification then binds the signed record
//! to the caller's expected audience, subject, payload, brain, mission head,
//! schema, and autonomy mode before any replay claim is consumed.

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::{Signature, VerifyingKey};
use p256::ecdsa::VerifyingKey as P256VerifyingKey;
use p256::ecdsa::{signature::Verifier as _, Signature as P256Signature};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    canonical_json, digest_domain_bytes, ActionId, ActiveMode, AuthorityVariant, CanonicalError,
    HumanApprovalV1, HumanKeyRegistryV1, IdentityError, IdentityStatus, OpaqueSignature,
    OwnerChallengeV1, OwnerIdentityV1, ReplayClaimV1, ReplayLedger, ReplayLedgerError,
    ReplayReceiptV1, REPLAY_CLAIM_SCHEMA,
};

pub const ED25519_ALGORITHM: &str = "ED25519";
/// P-256 ECDSA over SHA-256. Public keys use the 65-byte uncompressed SEC1
/// encoding (`04 || X || Y`) and signatures use canonical ASN.1 DER.
///
/// This matches the wire shapes exposed by Apple's P-256 Secure Enclave
/// signing APIs without claiming that any particular signer is hardware-backed.
pub const ECDSA_P256_SHA256_X962_ALGORITHM: &str = "ECDSA_P256_SHA256_X962";
pub const VERIFICATION_KEY_REGISTRY_SCHEMA: &str = "m1nd-verification-key-registry-v1";
pub const AUTHORITY_CAPABILITY_SCHEMA: &str = "m1nd-authority-capability-v1";
pub const OWNER_CHALLENGE_SIGNED_SCHEMA: &str = "m1nd-owner-challenge-signed-v1";
pub const HUMAN_APPROVAL_SIGNED_SCHEMA: &str = "m1nd-human-approval-signed-v1";
pub const OWNER_CHALLENGE_SIGNATURE_DOMAIN: &str = "m1nd-owner-challenge-signature-v1";
pub const HUMAN_APPROVAL_SIGNATURE_DOMAIN: &str = "m1nd-human-approval-signature-v1";
pub const AUTHORITY_CAPABILITY_SIGNATURE_DOMAIN: &str = "m1nd-authority-capability-signature-v1";
pub const SIGNED_BODY_DIGEST_DOMAIN: &str = "m1nd-authority-signed-body-v1";
pub const DEFAULT_AUTHORITY_CLOCK_SKEW_MS: u64 = 30_000;

const SIGNATURE_MESSAGE_PREFIX: &[u8] = b"m1nd-authority-signature-message-v1\0";

/// Signing is an injected capability. This crate never generates, stores, or
/// exports private key material. A platform adapter may keep the private key in
/// Keychain, Secure Enclave (with a future supported algorithm adapter), HSM,
/// or another protected signer while exposing only this narrow operation.
pub trait AuthoritySigner {
    fn key_id(&self) -> &str;
    fn subject_id(&self) -> &str;
    fn algorithm(&self) -> &str;
    fn public_key_bytes(&self) -> Result<Vec<u8>, AuthoritySignerError>;
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, AuthoritySignerError>;
}

#[derive(Debug, Error)]
pub enum AuthoritySignerError {
    #[error("platform signer failed: {message}")]
    Platform { message: String },
}

impl AuthoritySignerError {
    pub fn platform(message: impl Into<String>) -> Self {
        Self::Platform {
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationKeyV1 {
    pub key_id: String,
    pub subject_id: String,
    pub algorithm: String,
    /// Lowercase hexadecimal public key bytes in the canonical encoding for
    /// `algorithm`: 32 raw bytes for Ed25519, or 65-byte uncompressed SEC1 for
    /// P-256. This is public material; no private-key storage claim is inferred.
    pub public_key: String,
    pub created_at: u64,
    pub activated_at: u64,
    pub expires_at: Option<u64>,
    pub revoked_at: Option<u64>,
    pub rotated_at: Option<u64>,
    pub replacement_key_id: Option<String>,
    pub status: IdentityStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationKeyRegistryV1 {
    pub schema: String,
    pub registry_epoch: u64,
    pub keys: BTreeMap<String, VerificationKeyV1>,
}

impl VerificationKeyRegistryV1 {
    pub fn validate(
        &self,
        now_ms: u64,
        max_future_clock_skew_ms: u64,
    ) -> Result<(), AuthorityCryptoError> {
        if self.schema != VERIFICATION_KEY_REGISTRY_SCHEMA {
            return Err(AuthorityCryptoError::SchemaMismatch {
                field: "verification_key_registry.schema",
                expected: VERIFICATION_KEY_REGISTRY_SCHEMA.to_owned(),
                observed: self.schema.clone(),
            });
        }
        if self.keys.is_empty() {
            return Err(AuthorityCryptoError::EmptyRegistry);
        }
        for (map_key, key) in &self.keys {
            if map_key != &key.key_id {
                return Err(AuthorityCryptoError::KeyMapMismatch {
                    map_key: map_key.clone(),
                    embedded_key_id: key.key_id.clone(),
                });
            }
            key.validate_record(now_ms, max_future_clock_skew_ms)?;
        }
        self.validate_rotation_chains()?;
        Ok(())
    }

    pub fn resolve_active(
        &self,
        key_id: &str,
        expected_subject_id: &str,
        now_ms: u64,
        max_future_clock_skew_ms: u64,
    ) -> Result<&VerificationKeyV1, AuthorityCryptoError> {
        self.validate(now_ms, max_future_clock_skew_ms)?;
        let key = self
            .keys
            .get(key_id)
            .ok_or_else(|| AuthorityCryptoError::KeyNotFound {
                key_id: key_id.to_owned(),
            })?;
        if key.subject_id != expected_subject_id {
            return Err(AuthorityCryptoError::KeySubjectMismatch {
                key_id: key_id.to_owned(),
                expected: expected_subject_id.to_owned(),
                observed: key.subject_id.clone(),
            });
        }
        match key.status {
            IdentityStatus::Active => {}
            IdentityStatus::Revoked => {
                return Err(AuthorityCryptoError::KeyRevoked {
                    key_id: key_id.to_owned(),
                });
            }
            IdentityStatus::Rotated => {
                return Err(AuthorityCryptoError::KeyRotated {
                    key_id: key_id.to_owned(),
                    replacement_key_id: key.replacement_key_id.clone(),
                });
            }
            IdentityStatus::Expired => {
                return Err(AuthorityCryptoError::KeyExpired {
                    key_id: key_id.to_owned(),
                    expires_at: key.expires_at.unwrap_or_default(),
                    now_ms,
                });
            }
        }
        let latest_allowed = now_ms.saturating_add(max_future_clock_skew_ms);
        if key.activated_at > latest_allowed {
            return Err(AuthorityCryptoError::KeyNotYetActive {
                key_id: key_id.to_owned(),
                activated_at: key.activated_at,
                latest_allowed,
            });
        }
        if let Some(expires_at) = key.expires_at {
            if now_ms >= expires_at {
                return Err(AuthorityCryptoError::KeyExpired {
                    key_id: key_id.to_owned(),
                    expires_at,
                    now_ms,
                });
            }
        }
        Ok(key)
    }

    fn validate_rotation_chains(&self) -> Result<(), AuthorityCryptoError> {
        for start in self.keys.values() {
            if start.status != IdentityStatus::Rotated {
                continue;
            }
            let mut seen = BTreeSet::new();
            let mut current = start;
            while current.status == IdentityStatus::Rotated {
                if !seen.insert(current.key_id.clone()) {
                    return Err(AuthorityCryptoError::RotationCycle {
                        key_id: current.key_id.clone(),
                    });
                }
                let replacement_key_id =
                    current.replacement_key_id.as_deref().ok_or_else(|| {
                        AuthorityCryptoError::InvalidKeyLifecycle {
                            key_id: current.key_id.clone(),
                            reason: "ROTATED key has no replacement_key_id".to_owned(),
                        }
                    })?;
                let replacement = self.keys.get(replacement_key_id).ok_or_else(|| {
                    AuthorityCryptoError::RotationTargetNotFound {
                        key_id: current.key_id.clone(),
                        replacement_key_id: replacement_key_id.to_owned(),
                    }
                })?;
                if replacement.subject_id != current.subject_id {
                    return Err(AuthorityCryptoError::RotationSubjectMismatch {
                        key_id: current.key_id.clone(),
                        replacement_key_id: replacement.key_id.clone(),
                    });
                }
                current = replacement;
            }
        }
        Ok(())
    }
}

impl VerificationKeyV1 {
    fn validate_record(
        &self,
        now_ms: u64,
        max_future_clock_skew_ms: u64,
    ) -> Result<(), AuthorityCryptoError> {
        for (field, value) in [
            ("key_id", self.key_id.as_str()),
            ("subject_id", self.subject_id.as_str()),
            ("algorithm", self.algorithm.as_str()),
            ("public_key", self.public_key.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        validate_optional_non_empty("replacement_key_id", self.replacement_key_id.as_deref())?;
        decode_public_key(&self.algorithm, &self.public_key)?;
        if self.activated_at < self.created_at {
            return Err(AuthorityCryptoError::InvalidKeyLifecycle {
                key_id: self.key_id.clone(),
                reason: "activated_at precedes created_at".to_owned(),
            });
        }
        let latest_allowed = now_ms.saturating_add(max_future_clock_skew_ms);
        if self.created_at > latest_allowed {
            return Err(AuthorityCryptoError::KeyCreatedInFuture {
                key_id: self.key_id.clone(),
                created_at: self.created_at,
                latest_allowed,
            });
        }
        if let Some(expires_at) = self.expires_at {
            if expires_at <= self.activated_at {
                return Err(AuthorityCryptoError::InvalidKeyLifecycle {
                    key_id: self.key_id.clone(),
                    reason: "expires_at is not later than activated_at".to_owned(),
                });
            }
        }
        for (field, timestamp) in [
            ("revoked_at", self.revoked_at),
            ("rotated_at", self.rotated_at),
        ] {
            if timestamp.is_some_and(|value| value < self.activated_at) {
                return Err(AuthorityCryptoError::InvalidKeyLifecycle {
                    key_id: self.key_id.clone(),
                    reason: format!("{field} precedes activated_at"),
                });
            }
        }
        match self.status {
            IdentityStatus::Active => {
                if self.revoked_at.is_some()
                    || self.rotated_at.is_some()
                    || self.replacement_key_id.is_some()
                {
                    return Err(AuthorityCryptoError::InvalidKeyLifecycle {
                        key_id: self.key_id.clone(),
                        reason: "ACTIVE key carries terminal lifecycle fields".to_owned(),
                    });
                }
                if self
                    .expires_at
                    .is_some_and(|expires_at| now_ms >= expires_at)
                {
                    return Err(AuthorityCryptoError::InvalidKeyLifecycle {
                        key_id: self.key_id.clone(),
                        reason: "ACTIVE key is already expired".to_owned(),
                    });
                }
            }
            IdentityStatus::Revoked => {
                if self.revoked_at.is_none()
                    || self.rotated_at.is_some()
                    || self.replacement_key_id.is_some()
                {
                    return Err(AuthorityCryptoError::InvalidKeyLifecycle {
                        key_id: self.key_id.clone(),
                        reason: "REVOKED key requires only revoked_at".to_owned(),
                    });
                }
            }
            IdentityStatus::Rotated => {
                if self.rotated_at.is_none()
                    || self.replacement_key_id.is_none()
                    || self.revoked_at.is_some()
                    || self.replacement_key_id.as_deref() == Some(self.key_id.as_str())
                {
                    return Err(AuthorityCryptoError::InvalidKeyLifecycle {
                        key_id: self.key_id.clone(),
                        reason: "ROTATED key requires rotated_at and a different replacement key"
                            .to_owned(),
                    });
                }
            }
            IdentityStatus::Expired => {
                if self.expires_at.is_none()
                    || self.expires_at.is_some_and(|value| value > now_ms)
                    || self.revoked_at.is_some()
                    || self.rotated_at.is_some()
                    || self.replacement_key_id.is_some()
                {
                    return Err(AuthorityCryptoError::InvalidKeyLifecycle {
                        key_id: self.key_id.clone(),
                        reason: "EXPIRED key requires only an elapsed expires_at".to_owned(),
                    });
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityCapabilityV1 {
    pub schema: String,
    pub capability_id: String,
    pub issuer_subject_id: String,
    pub issuer_key_id: String,
    pub algorithm: String,
    pub subject_id: String,
    pub audience: String,
    pub organism_id: String,
    pub brain_id: String,
    pub mission_id: Option<String>,
    pub mission_head_id: Option<String>,
    pub action: ActionId,
    pub authority_variant: AuthorityVariant,
    pub active_mode: ActiveMode,
    pub payload_digest: String,
    pub policy_registry_digest: String,
    pub constitution_digest: String,
    pub key_registry_epoch: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub nonce: String,
    pub signature: OpaqueSignature,
}

impl AuthorityCapabilityV1 {
    pub fn signed_body_digest(&self) -> Result<String, AuthorityCryptoError> {
        signed_body_digest(AUTHORITY_CAPABILITY_SIGNATURE_DOMAIN, self, "signature")
    }

    fn validate_structural(
        &self,
        now_ms: u64,
        max_future_clock_skew_ms: u64,
    ) -> Result<(), AuthorityCryptoError> {
        if self.schema != AUTHORITY_CAPABILITY_SCHEMA {
            return Err(AuthorityCryptoError::SchemaMismatch {
                field: "capability.schema",
                expected: AUTHORITY_CAPABILITY_SCHEMA.to_owned(),
                observed: self.schema.clone(),
            });
        }
        for (field, value) in [
            ("capability_id", self.capability_id.as_str()),
            ("issuer_subject_id", self.issuer_subject_id.as_str()),
            ("issuer_key_id", self.issuer_key_id.as_str()),
            ("algorithm", self.algorithm.as_str()),
            ("subject_id", self.subject_id.as_str()),
            ("audience", self.audience.as_str()),
            ("organism_id", self.organism_id.as_str()),
            ("brain_id", self.brain_id.as_str()),
            ("action", self.action.as_str()),
            ("payload_digest", self.payload_digest.as_str()),
            (
                "policy_registry_digest",
                self.policy_registry_digest.as_str(),
            ),
            ("constitution_digest", self.constitution_digest.as_str()),
            ("nonce", self.nonce.as_str()),
            ("signature", self.signature.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        validate_optional_non_empty("mission_id", self.mission_id.as_deref())?;
        validate_optional_non_empty("mission_head_id", self.mission_head_id.as_deref())?;
        if self.mission_id.is_some() != self.mission_head_id.is_some() {
            return Err(AuthorityCryptoError::IncompleteMissionBinding);
        }
        integrity_for_algorithm(&self.algorithm)?;
        if !self.authority_variant.is_positive_sovereign() {
            return Err(AuthorityCryptoError::NonSovereignCapability {
                authority_variant: self.authority_variant,
            });
        }
        if self.expires_at <= self.issued_at {
            return Err(AuthorityCryptoError::InvalidTimeOrder {
                record: "authority capability",
                issued_at: self.issued_at,
                expires_at: self.expires_at,
            });
        }
        let latest_allowed = now_ms.saturating_add(max_future_clock_skew_ms);
        if self.issued_at > latest_allowed {
            return Err(AuthorityCryptoError::IssuedInFuture {
                record: "authority capability",
                issued_at: self.issued_at,
                latest_allowed,
            });
        }
        if now_ms >= self.expires_at {
            return Err(AuthorityCryptoError::Expired {
                record: "authority capability",
                expires_at: self.expires_at,
                now_ms,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ChallengeVerificationContext<'a> {
    pub now_ms: u64,
    pub max_future_clock_skew_ms: u64,
    pub expected_schema: &'a str,
    pub expected_audience: &'a str,
    pub expected_decision_subject_id: &'a str,
    pub expected_payload_digest: &'a str,
    pub expected_brain_id: &'a str,
    pub expected_mission_id: Option<&'a str>,
    pub expected_mission_head_id: Option<&'a str>,
    pub expected_active_mode: ActiveMode,
}

#[derive(Clone, Copy, Debug)]
pub struct CapabilityVerificationContext<'a> {
    pub now_ms: u64,
    pub max_future_clock_skew_ms: u64,
    pub expected_schema: &'a str,
    pub expected_audience: &'a str,
    pub expected_subject_id: &'a str,
    pub expected_payload_digest: &'a str,
    pub expected_organism_id: &'a str,
    pub expected_brain_id: &'a str,
    pub expected_mission_id: Option<&'a str>,
    pub expected_mission_head_id: Option<&'a str>,
    pub expected_action: &'a str,
    pub expected_authority_variant: AuthorityVariant,
    pub expected_active_mode: ActiveMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifiedArtifact {
    OwnerChallenge,
    HumanApproval,
    AuthorityCapability,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CryptographicIntegrity {
    VerifiedEd25519,
    VerifiedEcdsaP256Sha256X962,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedAuthorityV1 {
    pub artifact: VerifiedArtifact,
    pub integrity: CryptographicIntegrity,
    pub signed_schema: &'static str,
    pub signing_domain: &'static str,
    pub key_id: String,
    pub subject_id: String,
    pub signed_body_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedAuthorityOnceV1 {
    pub authority: VerifiedAuthorityV1,
    pub replay: ReplayReceiptV1,
}

#[derive(Debug, Error)]
pub enum AuthorityCryptoError {
    #[error("required authority field '{field}' is empty")]
    EmptyRequired { field: &'static str },
    #[error("verification key registry is empty")]
    EmptyRegistry,
    #[error("schema/context mismatch for {field}: expected '{expected}', observed '{observed}'")]
    SchemaMismatch {
        field: &'static str,
        expected: String,
        observed: String,
    },
    #[error(
        "authority context mismatch for {field}: expected '{expected}', observed '{observed}'"
    )]
    ContextMismatch {
        field: &'static str,
        expected: String,
        observed: String,
    },
    #[error(
        "verification key map key '{map_key}' does not match embedded key id '{embedded_key_id}'"
    )]
    KeyMapMismatch {
        map_key: String,
        embedded_key_id: String,
    },
    #[error("verification key '{key_id}' was not found")]
    KeyNotFound { key_id: String },
    #[error("verification key '{key_id}' subject mismatch: expected '{expected}', observed '{observed}'")]
    KeySubjectMismatch {
        key_id: String,
        expected: String,
        observed: String,
    },
    #[error("verification key '{key_id}' is revoked")]
    KeyRevoked { key_id: String },
    #[error("verification key '{key_id}' is rotated to {replacement_key_id:?}")]
    KeyRotated {
        key_id: String,
        replacement_key_id: Option<String>,
    },
    #[error("verification key '{key_id}' expired at {expires_at}; validation time is {now_ms}")]
    KeyExpired {
        key_id: String,
        expires_at: u64,
        now_ms: u64,
    },
    #[error("verification key '{key_id}' is not active until {activated_at}; latest allowed is {latest_allowed}")]
    KeyNotYetActive {
        key_id: String,
        activated_at: u64,
        latest_allowed: u64,
    },
    #[error("verification key '{key_id}' was created at {created_at}; latest allowed is {latest_allowed}")]
    KeyCreatedInFuture {
        key_id: String,
        created_at: u64,
        latest_allowed: u64,
    },
    #[error("invalid lifecycle for verification key '{key_id}': {reason}")]
    InvalidKeyLifecycle { key_id: String, reason: String },
    #[error("rotation target '{replacement_key_id}' for key '{key_id}' was not found")]
    RotationTargetNotFound {
        key_id: String,
        replacement_key_id: String,
    },
    #[error("rotation changes subject between key '{key_id}' and '{replacement_key_id}'")]
    RotationSubjectMismatch {
        key_id: String,
        replacement_key_id: String,
    },
    #[error("verification key rotation cycle includes '{key_id}'")]
    RotationCycle { key_id: String },
    #[error("unsupported signature algorithm '{actual}'")]
    UnsupportedAlgorithm { actual: String },
    #[error("invalid public key encoding")]
    PublicKeyEncoding,
    #[error("invalid signature encoding")]
    SignatureEncoding,
    #[error("signature verification failed")]
    SignatureInvalid,
    #[error("signer field '{field}' mismatch: expected '{expected}', observed '{observed}'")]
    SignerMismatch {
        field: &'static str,
        expected: String,
        observed: String,
    },
    #[error("pinned public key mismatch for '{key_id}'")]
    PinnedPublicKeyMismatch { key_id: String },
    #[error("record is not a JSON object or lacks signature field '{field}'")]
    SignatureWireShape { field: &'static str },
    #[error("mission_id and mission_head_id must be present together")]
    IncompleteMissionBinding,
    #[error(
        "capability authority variant {authority_variant:?} is not positive sovereign authority"
    )]
    NonSovereignCapability { authority_variant: AuthorityVariant },
    #[error("{record} has invalid time order: issued {issued_at}, expires {expires_at}")]
    InvalidTimeOrder {
        record: &'static str,
        issued_at: u64,
        expires_at: u64,
    },
    #[error("{record} issued_at {issued_at} exceeds permitted future time {latest_allowed}")]
    IssuedInFuture {
        record: &'static str,
        issued_at: u64,
        latest_allowed: u64,
    },
    #[error("{record} expired at {expires_at}; validation time is {now_ms}")]
    Expired {
        record: &'static str,
        expires_at: u64,
        now_ms: u64,
    },
    #[error(transparent)]
    StructuralIdentity(#[from] IdentityError),
    #[error(transparent)]
    Signer(#[from] AuthoritySignerError),
    #[error(transparent)]
    Replay(#[from] ReplayLedgerError),
    #[error(transparent)]
    Canonical(#[from] CanonicalError),
}

pub fn sign_owner_challenge(
    challenge: &mut OwnerChallengeV1,
    owner: &OwnerIdentityV1,
    keys: &VerificationKeyRegistryV1,
    signer: &dyn AuthoritySigner,
    now_ms: u64,
    max_future_clock_skew_ms: u64,
) -> Result<(), AuthorityCryptoError> {
    let mut structural = challenge.clone();
    structural.owner_signature = OpaqueSignature::new("pending-signature");
    structural.validate(owner, now_ms)?;
    let key = keys.resolve_active(
        &owner.key_id,
        &owner.owner_id,
        now_ms,
        max_future_clock_skew_ms,
    )?;
    ensure_pinned_public_key(&owner.key_id, &owner.non_exportable_public_key, key)?;
    challenge.owner_signature = sign_record(
        challenge,
        "owner_signature",
        OWNER_CHALLENGE_SIGNATURE_DOMAIN,
        key,
        signer,
    )?;
    Ok(())
}

pub fn sign_human_approval(
    approval: &mut HumanApprovalV1,
    challenge: &OwnerChallengeV1,
    owner: &OwnerIdentityV1,
    human_keys: &HumanKeyRegistryV1,
    verification_keys: &VerificationKeyRegistryV1,
    signer: &dyn AuthoritySigner,
    context: ChallengeVerificationContext<'_>,
) -> Result<(), AuthorityCryptoError> {
    verify_owner_challenge(challenge, owner, verification_keys, context)?;
    let mut structural = approval.clone();
    structural.signature = OpaqueSignature::new("pending-signature");
    structural.validate(challenge, owner, human_keys, context.now_ms)?;
    let key = verification_keys.resolve_active(
        &approval.key_id,
        &approval.subject_id,
        context.now_ms,
        context.max_future_clock_skew_ms,
    )?;
    let enrolled =
        human_keys
            .keys
            .get(&approval.key_id)
            .ok_or_else(|| AuthorityCryptoError::KeyNotFound {
                key_id: approval.key_id.clone(),
            })?;
    ensure_pinned_public_key(&approval.key_id, &enrolled.public_key, key)?;
    approval.signature = sign_record(
        approval,
        "signature",
        HUMAN_APPROVAL_SIGNATURE_DOMAIN,
        key,
        signer,
    )?;
    Ok(())
}

pub fn sign_capability(
    capability: &mut AuthorityCapabilityV1,
    keys: &VerificationKeyRegistryV1,
    signer: &dyn AuthoritySigner,
    now_ms: u64,
    max_future_clock_skew_ms: u64,
) -> Result<(), AuthorityCryptoError> {
    let mut structural = capability.clone();
    structural.signature = OpaqueSignature::new("pending-signature");
    structural.validate_structural(now_ms, max_future_clock_skew_ms)?;
    let key = keys.resolve_active(
        &capability.issuer_key_id,
        &capability.issuer_subject_id,
        now_ms,
        max_future_clock_skew_ms,
    )?;
    if capability.algorithm != key.algorithm {
        return Err(AuthorityCryptoError::ContextMismatch {
            field: "capability.algorithm",
            expected: key.algorithm.clone(),
            observed: capability.algorithm.clone(),
        });
    }
    capability.signature = sign_record(
        capability,
        "signature",
        AUTHORITY_CAPABILITY_SIGNATURE_DOMAIN,
        key,
        signer,
    )?;
    Ok(())
}

pub fn verify_owner_challenge(
    challenge: &OwnerChallengeV1,
    owner: &OwnerIdentityV1,
    keys: &VerificationKeyRegistryV1,
    context: ChallengeVerificationContext<'_>,
) -> Result<VerifiedAuthorityV1, AuthorityCryptoError> {
    challenge.validate(owner, context.now_ms)?;
    expect(
        "challenge.signed_schema",
        context.expected_schema,
        OWNER_CHALLENGE_SIGNED_SCHEMA,
    )?;
    expect(
        "challenge.audience",
        context.expected_audience,
        &challenge.audience,
    )?;
    expect(
        "challenge.decision_subject_id",
        context.expected_decision_subject_id,
        &challenge.decision_subject_id,
    )?;
    expect(
        "challenge.payload_digest",
        context.expected_payload_digest,
        &challenge.payload_digest,
    )?;
    expect(
        "challenge.brain_id",
        context.expected_brain_id,
        &challenge.brain_id,
    )?;
    expect_optional(
        "challenge.mission_id",
        context.expected_mission_id,
        challenge.mission_id.as_deref(),
    )?;
    expect_optional(
        "challenge.mission_head_id",
        context.expected_mission_head_id,
        challenge.mission_head_id.as_deref(),
    )?;
    expect_debug(
        "challenge.active_mode",
        context.expected_active_mode,
        challenge.active_mode,
    )?;

    let key = keys.resolve_active(
        &owner.key_id,
        &owner.owner_id,
        context.now_ms,
        context.max_future_clock_skew_ms,
    )?;
    ensure_pinned_public_key(&owner.key_id, &owner.non_exportable_public_key, key)?;
    verify_record_signature(
        challenge,
        "owner_signature",
        &challenge.owner_signature,
        OWNER_CHALLENGE_SIGNATURE_DOMAIN,
        key,
    )?;
    Ok(VerifiedAuthorityV1 {
        artifact: VerifiedArtifact::OwnerChallenge,
        integrity: integrity_for_algorithm(&key.algorithm)?,
        signed_schema: OWNER_CHALLENGE_SIGNED_SCHEMA,
        signing_domain: OWNER_CHALLENGE_SIGNATURE_DOMAIN,
        key_id: key.key_id.clone(),
        subject_id: key.subject_id.clone(),
        signed_body_digest: signed_body_digest(
            OWNER_CHALLENGE_SIGNATURE_DOMAIN,
            challenge,
            "owner_signature",
        )?,
    })
}

pub fn verify_owner_challenge_once(
    challenge: &OwnerChallengeV1,
    owner: &OwnerIdentityV1,
    keys: &VerificationKeyRegistryV1,
    context: ChallengeVerificationContext<'_>,
    replay_ledger: &mut dyn ReplayLedger,
) -> Result<VerifiedAuthorityOnceV1, AuthorityCryptoError> {
    let authority = verify_owner_challenge(challenge, owner, keys, context)?;
    let claim = ReplayClaimV1 {
        schema: REPLAY_CLAIM_SCHEMA.to_owned(),
        namespace: "owner-challenge".to_owned(),
        issuer_subject_id: owner.owner_id.clone(),
        key_id: owner.key_id.clone(),
        subject_id: challenge.decision_subject_id.clone(),
        nonce: challenge.nonce.clone(),
        object_digest: authority.signed_body_digest.clone(),
        issued_at: 0,
        expires_at: challenge.expires_at,
    };
    let replay = replay_ledger.consume(&claim, context.now_ms, context.max_future_clock_skew_ms)?;
    Ok(VerifiedAuthorityOnceV1 { authority, replay })
}

pub fn verify_human_approval(
    approval: &HumanApprovalV1,
    challenge: &OwnerChallengeV1,
    owner: &OwnerIdentityV1,
    human_keys: &HumanKeyRegistryV1,
    verification_keys: &VerificationKeyRegistryV1,
    context: ChallengeVerificationContext<'_>,
) -> Result<VerifiedAuthorityV1, AuthorityCryptoError> {
    verify_owner_challenge(challenge, owner, verification_keys, context)?;
    approval.validate(challenge, owner, human_keys, context.now_ms)?;
    let key = verification_keys.resolve_active(
        &approval.key_id,
        &approval.subject_id,
        context.now_ms,
        context.max_future_clock_skew_ms,
    )?;
    let enrolled =
        human_keys
            .keys
            .get(&approval.key_id)
            .ok_or_else(|| AuthorityCryptoError::KeyNotFound {
                key_id: approval.key_id.clone(),
            })?;
    ensure_pinned_public_key(&approval.key_id, &enrolled.public_key, key)?;
    verify_record_signature(
        approval,
        "signature",
        &approval.signature,
        HUMAN_APPROVAL_SIGNATURE_DOMAIN,
        key,
    )?;
    Ok(VerifiedAuthorityV1 {
        artifact: VerifiedArtifact::HumanApproval,
        integrity: integrity_for_algorithm(&key.algorithm)?,
        signed_schema: HUMAN_APPROVAL_SIGNED_SCHEMA,
        signing_domain: HUMAN_APPROVAL_SIGNATURE_DOMAIN,
        key_id: key.key_id.clone(),
        subject_id: key.subject_id.clone(),
        signed_body_digest: signed_body_digest(
            HUMAN_APPROVAL_SIGNATURE_DOMAIN,
            approval,
            "signature",
        )?,
    })
}

pub fn verify_human_approval_once(
    approval: &HumanApprovalV1,
    challenge: &OwnerChallengeV1,
    owner: &OwnerIdentityV1,
    human_keys: &HumanKeyRegistryV1,
    verification_keys: &VerificationKeyRegistryV1,
    context: ChallengeVerificationContext<'_>,
    replay_ledger: &mut dyn ReplayLedger,
) -> Result<VerifiedAuthorityOnceV1, AuthorityCryptoError> {
    let authority = verify_human_approval(
        approval,
        challenge,
        owner,
        human_keys,
        verification_keys,
        context,
    )?;
    let claim = ReplayClaimV1 {
        schema: REPLAY_CLAIM_SCHEMA.to_owned(),
        namespace: "human-approval".to_owned(),
        issuer_subject_id: approval.subject_id.clone(),
        key_id: approval.key_id.clone(),
        subject_id: approval.subject_id.clone(),
        nonce: challenge.nonce.clone(),
        object_digest: authority.signed_body_digest.clone(),
        issued_at: 0,
        expires_at: challenge.expires_at,
    };
    let replay = replay_ledger.consume(&claim, context.now_ms, context.max_future_clock_skew_ms)?;
    Ok(VerifiedAuthorityOnceV1 { authority, replay })
}

pub fn verify_capability(
    capability: &AuthorityCapabilityV1,
    keys: &VerificationKeyRegistryV1,
    context: CapabilityVerificationContext<'_>,
) -> Result<VerifiedAuthorityV1, AuthorityCryptoError> {
    capability.validate_structural(context.now_ms, context.max_future_clock_skew_ms)?;
    expect(
        "capability.schema",
        context.expected_schema,
        &capability.schema,
    )?;
    expect(
        "capability.audience",
        context.expected_audience,
        &capability.audience,
    )?;
    expect(
        "capability.subject_id",
        context.expected_subject_id,
        &capability.subject_id,
    )?;
    expect(
        "capability.payload_digest",
        context.expected_payload_digest,
        &capability.payload_digest,
    )?;
    expect(
        "capability.organism_id",
        context.expected_organism_id,
        &capability.organism_id,
    )?;
    expect(
        "capability.brain_id",
        context.expected_brain_id,
        &capability.brain_id,
    )?;
    expect_optional(
        "capability.mission_id",
        context.expected_mission_id,
        capability.mission_id.as_deref(),
    )?;
    expect_optional(
        "capability.mission_head_id",
        context.expected_mission_head_id,
        capability.mission_head_id.as_deref(),
    )?;
    expect(
        "capability.action",
        context.expected_action,
        capability.action.as_str(),
    )?;
    expect_debug(
        "capability.authority_variant",
        context.expected_authority_variant,
        capability.authority_variant,
    )?;
    expect_debug(
        "capability.active_mode",
        context.expected_active_mode,
        capability.active_mode,
    )?;
    if capability.key_registry_epoch != keys.registry_epoch {
        return Err(AuthorityCryptoError::ContextMismatch {
            field: "capability.key_registry_epoch",
            expected: keys.registry_epoch.to_string(),
            observed: capability.key_registry_epoch.to_string(),
        });
    }

    let key = keys.resolve_active(
        &capability.issuer_key_id,
        &capability.issuer_subject_id,
        context.now_ms,
        context.max_future_clock_skew_ms,
    )?;
    if capability.algorithm != key.algorithm {
        return Err(AuthorityCryptoError::ContextMismatch {
            field: "capability.algorithm",
            expected: key.algorithm.clone(),
            observed: capability.algorithm.clone(),
        });
    }
    verify_record_signature(
        capability,
        "signature",
        &capability.signature,
        AUTHORITY_CAPABILITY_SIGNATURE_DOMAIN,
        key,
    )?;
    Ok(VerifiedAuthorityV1 {
        artifact: VerifiedArtifact::AuthorityCapability,
        integrity: integrity_for_algorithm(&key.algorithm)?,
        signed_schema: AUTHORITY_CAPABILITY_SCHEMA,
        signing_domain: AUTHORITY_CAPABILITY_SIGNATURE_DOMAIN,
        key_id: key.key_id.clone(),
        subject_id: capability.subject_id.clone(),
        signed_body_digest: capability.signed_body_digest()?,
    })
}

pub fn verify_capability_once(
    capability: &AuthorityCapabilityV1,
    keys: &VerificationKeyRegistryV1,
    context: CapabilityVerificationContext<'_>,
    replay_ledger: &mut dyn ReplayLedger,
) -> Result<VerifiedAuthorityOnceV1, AuthorityCryptoError> {
    let authority = verify_capability(capability, keys, context)?;
    let claim = ReplayClaimV1 {
        schema: REPLAY_CLAIM_SCHEMA.to_owned(),
        namespace: "authority-capability".to_owned(),
        issuer_subject_id: capability.issuer_subject_id.clone(),
        key_id: capability.issuer_key_id.clone(),
        subject_id: capability.subject_id.clone(),
        nonce: capability.nonce.clone(),
        object_digest: authority.signed_body_digest.clone(),
        issued_at: capability.issued_at,
        expires_at: capability.expires_at,
    };
    let replay = replay_ledger.consume(&claim, context.now_ms, context.max_future_clock_skew_ms)?;
    Ok(VerifiedAuthorityOnceV1 { authority, replay })
}

fn sign_record<T: Serialize + ?Sized>(
    record: &T,
    signature_field: &'static str,
    domain: &'static str,
    key: &VerificationKeyV1,
    signer: &dyn AuthoritySigner,
) -> Result<OpaqueSignature, AuthorityCryptoError> {
    expect_signer("key_id", &key.key_id, signer.key_id())?;
    expect_signer("subject_id", &key.subject_id, signer.subject_id())?;
    expect_signer("algorithm", &key.algorithm, signer.algorithm())?;
    let pinned_public_key = decode_public_key(&key.algorithm, &key.public_key)?;
    let signer_public_key = signer.public_key_bytes()?;
    if signer_public_key != pinned_public_key.canonical_bytes() {
        return Err(AuthorityCryptoError::PinnedPublicKeyMismatch {
            key_id: key.key_id.clone(),
        });
    }
    let canonical = canonical_without_signature(record, signature_field)?;
    let message = signature_message(domain, &canonical);
    let signature = signer.sign(&message)?;
    let signature = canonicalize_and_verify_signature(&pinned_public_key, &message, &signature)?;
    Ok(OpaqueSignature::new(hex_lower(&signature)))
}

/// Sign an already canonicalized, explicitly selected non-circular body using
/// the same length-delimited authority signature envelope as first-class G2
/// artifacts. The caller owns selection of the signed subset; this helper
/// still pins signer identity/algorithm/public key to the supplied key record.
pub fn sign_canonical_authority_payload(
    domain: &'static str,
    canonical_payload: &[u8],
    key: &VerificationKeyV1,
    signer: &dyn AuthoritySigner,
) -> Result<OpaqueSignature, AuthorityCryptoError> {
    sign_authority_message(&signature_message(domain, canonical_payload), key, signer)
}

/// Sign an exact, already-framed authority message with an injected protected
/// signer and return the canonical lowercase-hex signature — low-S ASN.1 DER for
/// P-256 — after verifying it against the pinned key. No domain envelope is
/// applied here: the framing is entirely the caller's, so this is the production
/// seam for protected signers whose raw output must be canonicalized by the crate
/// that owns the P-256 primitives. A Secure Enclave key returns a possibly
/// high-S DER signature from `SecKeyCreateSignature`; normalizing it here lets
/// callers such as m1nd-mcp sign through the enclave without ever linking p256.
pub fn sign_authority_message(
    message: &[u8],
    key: &VerificationKeyV1,
    signer: &dyn AuthoritySigner,
) -> Result<OpaqueSignature, AuthorityCryptoError> {
    expect_signer("key_id", &key.key_id, signer.key_id())?;
    expect_signer("subject_id", &key.subject_id, signer.subject_id())?;
    expect_signer("algorithm", &key.algorithm, signer.algorithm())?;
    let pinned_public_key = decode_public_key(&key.algorithm, &key.public_key)?;
    if signer.public_key_bytes()? != pinned_public_key.canonical_bytes() {
        return Err(AuthorityCryptoError::PinnedPublicKeyMismatch {
            key_id: key.key_id.clone(),
        });
    }
    let signature = signer.sign(message)?;
    let signature = canonicalize_and_verify_signature(&pinned_public_key, message, &signature)?;
    Ok(OpaqueSignature::new(hex_lower(&signature)))
}

/// Verify a signature over an already canonicalized, explicitly selected
/// signed subset. Key activity/time/subject resolution remains the caller's
/// responsibility through `VerificationKeyRegistryV1::resolve_active`.
pub fn verify_canonical_authority_payload_signature(
    domain: &'static str,
    canonical_payload: &[u8],
    signature: &OpaqueSignature,
    key: &VerificationKeyV1,
) -> Result<CryptographicIntegrity, AuthorityCryptoError> {
    let message = signature_message(domain, canonical_payload);
    verify_authority_message_signature(&message, signature, key)
}

/// Verify a signature over an authority message whose domain framing was
/// already applied by the owning protocol.
///
/// This deliberately performs no key lookup or lifecycle decision. Callers
/// must first resolve the supplied key through
/// [`VerificationKeyRegistryV1::resolve_active`]. The narrow raw-message seam
/// exists for protocols whose framing predates the generic authority envelope;
/// it still enforces the pinned key encoding, canonical lowercase signature
/// encoding, and the algorithm-specific cryptographic verifier in this crate.
pub fn verify_authority_message_signature(
    message: &[u8],
    signature: &OpaqueSignature,
    key: &VerificationKeyV1,
) -> Result<CryptographicIntegrity, AuthorityCryptoError> {
    let verifying_key = decode_public_key(&key.algorithm, &key.public_key)?;
    let signature_bytes =
        decode_lower_hex(signature.as_str()).ok_or(AuthorityCryptoError::SignatureEncoding)?;
    let canonical = canonicalize_and_verify_signature(&verifying_key, message, &signature_bytes)?;
    if canonical != signature_bytes {
        return Err(AuthorityCryptoError::SignatureEncoding);
    }
    integrity_for_algorithm(&key.algorithm)
}

fn verify_record_signature<T: Serialize + ?Sized>(
    record: &T,
    signature_field: &'static str,
    signature: &OpaqueSignature,
    domain: &'static str,
    key: &VerificationKeyV1,
) -> Result<(), AuthorityCryptoError> {
    let verifying_key = decode_public_key(&key.algorithm, &key.public_key)?;
    let signature_bytes =
        decode_lower_hex(signature.as_str()).ok_or(AuthorityCryptoError::SignatureEncoding)?;
    let canonical = canonical_without_signature(record, signature_field)?;
    let message = signature_message(domain, &canonical);
    let canonical = canonicalize_and_verify_signature(&verifying_key, &message, &signature_bytes)?;
    if canonical != signature_bytes {
        return Err(AuthorityCryptoError::SignatureEncoding);
    }
    Ok(())
}

fn signed_body_digest<T: Serialize + ?Sized>(
    domain: &'static str,
    record: &T,
    signature_field: &'static str,
) -> Result<String, AuthorityCryptoError> {
    let canonical = canonical_without_signature(record, signature_field)?;
    Ok(digest_domain_bytes(
        SIGNED_BODY_DIGEST_DOMAIN,
        &signature_message(domain, &canonical),
    ))
}

fn canonical_without_signature<T: Serialize + ?Sized>(
    record: &T,
    signature_field: &'static str,
) -> Result<Vec<u8>, AuthorityCryptoError> {
    let mut value = serde_json::to_value(record).map_err(CanonicalError::from)?;
    let object = value
        .as_object_mut()
        .ok_or(AuthorityCryptoError::SignatureWireShape {
            field: signature_field,
        })?;
    if object.remove(signature_field).is_none() {
        return Err(AuthorityCryptoError::SignatureWireShape {
            field: signature_field,
        });
    }
    canonical_json(&value).map_err(AuthorityCryptoError::from)
}

fn signature_message(domain: &str, canonical_payload: &[u8]) -> Vec<u8> {
    let mut message = Vec::with_capacity(
        SIGNATURE_MESSAGE_PREFIX.len() + domain.len() + canonical_payload.len() + 16,
    );
    message.extend_from_slice(SIGNATURE_MESSAGE_PREFIX);
    message.extend_from_slice(&(domain.len() as u64).to_be_bytes());
    message.extend_from_slice(domain.as_bytes());
    message.extend_from_slice(&(canonical_payload.len() as u64).to_be_bytes());
    message.extend_from_slice(canonical_payload);
    message
}

fn ensure_pinned_public_key(
    key_id: &str,
    pinned: &str,
    key: &VerificationKeyV1,
) -> Result<(), AuthorityCryptoError> {
    let pinned = decode_public_key(&key.algorithm, pinned)?;
    let registered = decode_public_key(&key.algorithm, &key.public_key)?;
    if pinned.canonical_bytes() != registered.canonical_bytes() {
        return Err(AuthorityCryptoError::PinnedPublicKeyMismatch {
            key_id: key_id.to_owned(),
        });
    }
    Ok(())
}

enum DecodedPublicKey {
    Ed25519(VerifyingKey),
    P256(P256VerifyingKey),
}

impl DecodedPublicKey {
    fn canonical_bytes(&self) -> Vec<u8> {
        match self {
            Self::Ed25519(key) => key.as_bytes().to_vec(),
            Self::P256(key) => key.to_sec1_point(false).as_bytes().to_vec(),
        }
    }
}

fn decode_public_key(
    algorithm: &str,
    value: &str,
) -> Result<DecodedPublicKey, AuthorityCryptoError> {
    match algorithm {
        ED25519_ALGORITHM => {
            let bytes = decode_lower_hex_fixed::<32>(value)
                .ok_or(AuthorityCryptoError::PublicKeyEncoding)?;
            VerifyingKey::from_bytes(&bytes)
                .map(DecodedPublicKey::Ed25519)
                .map_err(|_| AuthorityCryptoError::PublicKeyEncoding)
        }
        ECDSA_P256_SHA256_X962_ALGORITHM => {
            let bytes = decode_lower_hex(value).ok_or(AuthorityCryptoError::PublicKeyEncoding)?;
            if bytes.len() != 65 || bytes.first() != Some(&0x04) {
                return Err(AuthorityCryptoError::PublicKeyEncoding);
            }
            let key = P256VerifyingKey::from_sec1_bytes(&bytes)
                .map_err(|_| AuthorityCryptoError::PublicKeyEncoding)?;
            if key.to_sec1_point(false).as_bytes() != bytes {
                return Err(AuthorityCryptoError::PublicKeyEncoding);
            }
            Ok(DecodedPublicKey::P256(key))
        }
        actual => Err(AuthorityCryptoError::UnsupportedAlgorithm {
            actual: actual.to_owned(),
        }),
    }
}

fn integrity_for_algorithm(
    algorithm: &str,
) -> Result<CryptographicIntegrity, AuthorityCryptoError> {
    match algorithm {
        ED25519_ALGORITHM => Ok(CryptographicIntegrity::VerifiedEd25519),
        ECDSA_P256_SHA256_X962_ALGORITHM => Ok(CryptographicIntegrity::VerifiedEcdsaP256Sha256X962),
        actual => Err(AuthorityCryptoError::UnsupportedAlgorithm {
            actual: actual.to_owned(),
        }),
    }
}

fn canonicalize_and_verify_signature(
    key: &DecodedPublicKey,
    message: &[u8],
    signature: &[u8],
) -> Result<Vec<u8>, AuthorityCryptoError> {
    match key {
        DecodedPublicKey::Ed25519(key) => {
            let bytes: [u8; 64] = signature
                .try_into()
                .map_err(|_| AuthorityCryptoError::SignatureEncoding)?;
            key.verify_strict(message, &Signature::from_bytes(&bytes))
                .map_err(|_| AuthorityCryptoError::SignatureInvalid)?;
            Ok(bytes.to_vec())
        }
        DecodedPublicKey::P256(key) => {
            let parsed = P256Signature::from_der(signature)
                .map_err(|_| AuthorityCryptoError::SignatureEncoding)?;
            // `ecdsa` 0.17 made `normalize_s` infallible: it used to return
            // `None` when `s` was already low and `Some(flipped)` otherwise,
            // so the old spelling was `.normalize_s().unwrap_or(parsed)`. The
            // value produced is identical — always the low-S representative.
            let normalized = parsed.normalize_s();
            key.verify(message, &normalized)
                .map_err(|_| AuthorityCryptoError::SignatureInvalid)?;
            Ok(normalized.to_der().as_bytes().to_vec())
        }
    }
}

fn decode_lower_hex(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2)
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    let mut output = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        output.push((hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?);
    }
    Some(output)
}

fn decode_lower_hex_fixed<const N: usize>(value: &str) -> Option<[u8; N]> {
    decode_lower_hex(value)?.try_into().ok()
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), AuthorityCryptoError> {
    if value.trim().is_empty() {
        return Err(AuthorityCryptoError::EmptyRequired { field });
    }
    Ok(())
}

fn validate_optional_non_empty(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), AuthorityCryptoError> {
    if value.is_some_and(|value| value.trim().is_empty()) {
        return Err(AuthorityCryptoError::EmptyRequired { field });
    }
    Ok(())
}

fn expect(field: &'static str, expected: &str, observed: &str) -> Result<(), AuthorityCryptoError> {
    if expected != observed {
        return Err(AuthorityCryptoError::ContextMismatch {
            field,
            expected: expected.to_owned(),
            observed: observed.to_owned(),
        });
    }
    Ok(())
}

fn expect_optional(
    field: &'static str,
    expected: Option<&str>,
    observed: Option<&str>,
) -> Result<(), AuthorityCryptoError> {
    if expected != observed {
        return Err(AuthorityCryptoError::ContextMismatch {
            field,
            expected: format!("{expected:?}"),
            observed: format!("{observed:?}"),
        });
    }
    Ok(())
}

fn expect_debug<T: std::fmt::Debug + PartialEq>(
    field: &'static str,
    expected: T,
    observed: T,
) -> Result<(), AuthorityCryptoError> {
    if expected != observed {
        return Err(AuthorityCryptoError::ContextMismatch {
            field,
            expected: format!("{expected:?}"),
            observed: format!("{observed:?}"),
        });
    }
    Ok(())
}

fn expect_signer(
    field: &'static str,
    expected: &str,
    observed: &str,
) -> Result<(), AuthorityCryptoError> {
    if expected != observed {
        return Err(AuthorityCryptoError::SignerMismatch {
            field,
            expected: expected.to_owned(),
            observed: observed.to_owned(),
        });
    }
    Ok(())
}
