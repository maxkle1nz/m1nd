use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::policy::{ActionId, ActiveMode, AuthorityVariant};
use crate::{digest_canonical, digest_domain_bytes, CanonicalError, OpaqueSignature};

pub const ENROLLMENT_PUBLIC_KEY_DIGEST_DOMAIN: &str = "m1nd-enrollment-public-key-v1";
pub const ENROLLMENT_SCOPES_DIGEST_DOMAIN: &str = "m1nd-enrollment-scopes-v1";
pub const OWNER_CHALLENGE_DIGEST_DOMAIN: &str = "m1nd-owner-challenge-v1";
pub const HUMAN_APPROVAL_DIGEST_DOMAIN: &str = "m1nd-human-approval-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IdentityStatus {
    Active,
    Revoked,
    Expired,
    Rotated,
}

/// Result of structural validation for a contract carrying an opaque
/// signature. This state deliberately has no `Authenticated` variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentityIntegrityDisposition {
    OpaqueSignaturePresentUnverified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdentityStructuralValidation {
    pub integrity: IdentityIntegrityDisposition,
}

impl IdentityStructuralValidation {
    const fn opaque_signature_unverified() -> Self {
        Self {
            integrity: IdentityIntegrityDisposition::OpaqueSignaturePresentUnverified,
        }
    }
}

/// Signed enrollment/handshake evidence. The signature is intentionally
/// opaque in this contract crate; validation proves only structural and digest
/// bindings, never authenticity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentEvidenceV1 {
    pub enrollment_id: String,
    pub issuer: String,
    pub issuer_key_id: String,
    pub algorithm: String,
    pub subject_id: String,
    pub key_id: String,
    pub public_key_digest: String,
    pub app_host_identity: String,
    pub scopes_digest: String,
    pub audience: String,
    pub nonce: String,
    pub session_context_digest: String,
    pub issued_at: u64,
    pub expires_at: u64,
    pub signature: OpaqueSignature,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientIdentityV1 {
    pub subject_id: String,
    pub key_id: String,
    pub public_key: String,
    pub app_host_identity: String,
    pub enrollment_evidence: EnrollmentEvidenceV1,
    pub scopes: BTreeSet<String>,
    pub created_at: u64,
    pub revoked_at: Option<u64>,
    pub status: IdentityStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerIdentityV1 {
    pub owner_id: String,
    pub key_id: String,
    pub non_exportable_public_key: String,
    pub pinned_trust_anchor: String,
    pub protected_latest_epoch: u64,
}

/// One enrolled human key. Historical revoked, expired, and rotated records
/// remain representable in the registry but cannot approve a challenge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanKeyV1 {
    pub key_id: String,
    pub subject_id: String,
    pub platform: String,
    pub public_key: String,
    pub attestation_class: String,
    pub created_at: u64,
    pub rotated_at: Option<u64>,
    pub revoked_at: Option<u64>,
    pub status: IdentityStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanKeyRegistryV1 {
    pub owner_id: String,
    pub registry_epoch: u64,
    pub keys: BTreeMap<String, HumanKeyV1>,
}

/// Full owner-signed immutable challenge from PRD 6.5. Optional bindings stay
/// explicit in the wire; validation rejects empty option values and incomplete
/// mission bindings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerChallengeV1 {
    pub challenge_id: String,
    pub intent_digest: String,
    pub intent_core_ref: String,
    pub intent_canonicalization_version: String,
    pub organism_id: String,
    pub repo_id: String,
    pub issuer_subject_id: String,
    pub decision_subject_id: String,
    pub caller_subject_id: String,
    pub proposer_subject_id: String,
    pub executor_subject_id: Option<String>,
    pub delegation_grant_digest: Option<String>,
    pub audience: String,
    pub session_context_digest: String,
    pub action: ActionId,
    pub required_authority_variant: AuthorityVariant,
    pub action_policy_registry_digest: String,
    pub classifier_decision_digest: String,
    pub active_mode: ActiveMode,
    pub constitution_digest: String,
    pub constitution_epoch: u64,
    pub autonomy_epoch: u64,
    pub brain_id: String,
    pub mission_id: Option<String>,
    pub mission_head_id: Option<String>,
    pub block_id: Option<String>,
    pub candidate_digest: Option<String>,
    pub risk_scope_digest: String,
    pub expected_store_epoch: u64,
    pub expected_store_version: u64,
    pub expected_boundary_version: u64,
    pub expected_contract_version: u64,
    pub idempotency_key: String,
    pub payload_digest: String,
    pub canonical_summary: String,
    pub nonce: String,
    pub expires_at: u64,
    pub owner_signature: OpaqueSignature,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanApprovalV1 {
    pub challenge_id: String,
    pub canonical_challenge_digest: String,
    pub key_id: String,
    pub subject_id: String,
    pub user_presence_flags: String,
    pub counter: u64,
    pub signature: OpaqueSignature,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanDecisionV1 {
    pub decision_id: String,
    pub intent_digest: String,
    pub intent_core_ref: String,
    pub intent_canonicalization_version: String,
    pub human_approval_digest: String,
    pub issuer_subject_id: String,
}

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("required field '{field}' is empty")]
    EmptyRequired { field: &'static str },
    #[error("required collection '{field}' is empty")]
    EmptyCollection { field: &'static str },
    #[error("collection '{field}' contains an empty value")]
    EmptyCollectionValue { field: &'static str },
    #[error("{record} was issued in the future at {issued_at}; validation time is {now_ms}")]
    IssuedInFuture {
        record: &'static str,
        issued_at: u64,
        now_ms: u64,
    },
    #[error(
        "{record} has invalid time order: issued/created {issued_at}, expires/changes {expires_at}"
    )]
    InvalidTimeOrder {
        record: &'static str,
        issued_at: u64,
        expires_at: u64,
    },
    #[error("{record} expired at {expires_at}; validation time is {now_ms}")]
    Expired {
        record: &'static str,
        expires_at: u64,
        now_ms: u64,
    },
    #[error("{record} has expired status")]
    ExpiredStatus { record: &'static str },
    #[error("{record} is revoked")]
    Revoked { record: &'static str },
    #[error("{record} is rotated")]
    Rotated { record: &'static str },
    #[error("{record} has inactive status {status:?}")]
    InactiveStatus {
        record: &'static str,
        status: IdentityStatus,
    },
    #[error("enrollment evidence field '{field}' does not bind the client identity")]
    EnrollmentBindingMismatch { field: &'static str },
    #[error("human key map key '{map_key}' does not equal embedded key_id '{embedded_key_id}'")]
    HumanKeyMapMismatch {
        map_key: String,
        embedded_key_id: String,
    },
    #[error("human key '{key_id}' is not enrolled")]
    HumanKeyNotFound { key_id: String },
    #[error("owner challenge issuer '{issuer}' does not match pinned owner '{owner_id}'")]
    WrongOwner { issuer: String, owner_id: String },
    #[error("owner challenge must require HUMAN authority, observed {actual:?}")]
    WrongChallengeAuthority { actual: AuthorityVariant },
    #[error("HUMAN owner challenge is not valid in active mode {active_mode:?}")]
    WrongChallengeMode { active_mode: ActiveMode },
    #[error("mission_id and mission_head_id must be present together")]
    IncompleteMissionBinding,
    #[error("challenge expired at {expires_at}; validation time is {now_ms}")]
    ChallengeExpired { expires_at: u64, now_ms: u64 },
    #[error("approval challenge_id does not match the immutable owner challenge")]
    ChallengeIdMismatch,
    #[error("approval digest does not match the complete canonical owner challenge: expected {expected}, observed {observed}")]
    ChallengeDigestMismatch { expected: String, observed: String },
    #[error("approval subject '{approval_subject}' does not match challenge decision subject '{decision_subject}'")]
    ApprovalDecisionSubjectMismatch {
        approval_subject: String,
        decision_subject: String,
    },
    #[error(
        "approval subject '{approval_subject}' does not match enrolled key subject '{key_subject}'"
    )]
    ApprovalKeySubjectMismatch {
        approval_subject: String,
        key_subject: String,
    },
    #[error("human decision field '{field}' does not bind the immutable challenge")]
    DecisionChallengeMismatch { field: &'static str },
    #[error("human decision approval digest mismatch: expected {expected}, observed {observed}")]
    ApprovalDigestMismatch { expected: String, observed: String },
    #[error(transparent)]
    Canonical(#[from] CanonicalError),
}

impl EnrollmentEvidenceV1 {
    pub fn public_key_digest(public_key: &str) -> String {
        digest_domain_bytes(ENROLLMENT_PUBLIC_KEY_DIGEST_DOMAIN, public_key.as_bytes())
    }

    pub fn scopes_digest(scopes: &BTreeSet<String>) -> Result<String, CanonicalError> {
        digest_canonical(ENROLLMENT_SCOPES_DIGEST_DOMAIN, scopes)
    }

    pub fn validate(&self, now_ms: u64) -> Result<IdentityStructuralValidation, IdentityError> {
        for (field, value) in [
            ("enrollment_id", self.enrollment_id.as_str()),
            ("issuer", self.issuer.as_str()),
            ("issuer_key_id", self.issuer_key_id.as_str()),
            ("algorithm", self.algorithm.as_str()),
            ("subject_id", self.subject_id.as_str()),
            ("key_id", self.key_id.as_str()),
            ("public_key_digest", self.public_key_digest.as_str()),
            ("app_host_identity", self.app_host_identity.as_str()),
            ("scopes_digest", self.scopes_digest.as_str()),
            ("audience", self.audience.as_str()),
            ("nonce", self.nonce.as_str()),
            (
                "session_context_digest",
                self.session_context_digest.as_str(),
            ),
        ] {
            require_non_empty(field, value)?;
        }
        require_signature("signature", &self.signature)?;
        if self.expires_at <= self.issued_at {
            return Err(IdentityError::InvalidTimeOrder {
                record: "enrollment evidence",
                issued_at: self.issued_at,
                expires_at: self.expires_at,
            });
        }
        if self.issued_at > now_ms {
            return Err(IdentityError::IssuedInFuture {
                record: "enrollment evidence",
                issued_at: self.issued_at,
                now_ms,
            });
        }
        if now_ms >= self.expires_at {
            return Err(IdentityError::Expired {
                record: "enrollment evidence",
                expires_at: self.expires_at,
                now_ms,
            });
        }
        Ok(IdentityStructuralValidation::opaque_signature_unverified())
    }
}

impl ClientIdentityV1 {
    /// Validate current lifecycle and exact enrollment bindings. This does not
    /// verify the enrollment signature.
    pub fn validate(&self, now_ms: u64) -> Result<IdentityStructuralValidation, IdentityError> {
        for (field, value) in [
            ("subject_id", self.subject_id.as_str()),
            ("key_id", self.key_id.as_str()),
            ("public_key", self.public_key.as_str()),
            ("app_host_identity", self.app_host_identity.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        if self.scopes.is_empty() {
            return Err(IdentityError::EmptyCollection { field: "scopes" });
        }
        if self.scopes.iter().any(|scope| scope.trim().is_empty()) {
            return Err(IdentityError::EmptyCollectionValue { field: "scopes" });
        }
        if self.created_at > now_ms {
            return Err(IdentityError::IssuedInFuture {
                record: "client identity",
                issued_at: self.created_at,
                now_ms,
            });
        }
        match self.status {
            IdentityStatus::Active if self.revoked_at.is_none() => {}
            IdentityStatus::Revoked | IdentityStatus::Active => {
                return Err(IdentityError::Revoked {
                    record: "client identity",
                });
            }
            status => {
                return Err(IdentityError::InactiveStatus {
                    record: "client identity",
                    status,
                });
            }
        }

        self.enrollment_evidence.validate(now_ms)?;
        for (field, matches) in [
            (
                "subject_id",
                self.enrollment_evidence.subject_id == self.subject_id,
            ),
            ("key_id", self.enrollment_evidence.key_id == self.key_id),
            (
                "app_host_identity",
                self.enrollment_evidence.app_host_identity == self.app_host_identity,
            ),
            (
                "public_key_digest",
                self.enrollment_evidence.public_key_digest
                    == EnrollmentEvidenceV1::public_key_digest(&self.public_key),
            ),
            (
                "scopes_digest",
                self.enrollment_evidence.scopes_digest
                    == EnrollmentEvidenceV1::scopes_digest(&self.scopes)?,
            ),
        ] {
            if !matches {
                return Err(IdentityError::EnrollmentBindingMismatch { field });
            }
        }
        if self.created_at < self.enrollment_evidence.issued_at {
            return Err(IdentityError::InvalidTimeOrder {
                record: "client identity enrollment",
                issued_at: self.enrollment_evidence.issued_at,
                expires_at: self.created_at,
            });
        }

        Ok(IdentityStructuralValidation::opaque_signature_unverified())
    }

    /// Add the pinned owner/key equality required before a later cryptographic
    /// verifier may inspect the enrollment signature.
    pub fn validate_for_owner(
        &self,
        owner: &OwnerIdentityV1,
        now_ms: u64,
    ) -> Result<IdentityStructuralValidation, IdentityError> {
        let validation = self.validate(now_ms)?;
        owner.validate()?;
        for (field, matches) in [
            ("issuer", self.enrollment_evidence.issuer == owner.owner_id),
            (
                "issuer_key_id",
                self.enrollment_evidence.issuer_key_id == owner.key_id,
            ),
        ] {
            if !matches {
                return Err(IdentityError::EnrollmentBindingMismatch { field });
            }
        }
        Ok(validation)
    }
}

impl OwnerIdentityV1 {
    pub fn validate(&self) -> Result<(), IdentityError> {
        for (field, value) in [
            ("owner_id", self.owner_id.as_str()),
            ("key_id", self.key_id.as_str()),
            (
                "non_exportable_public_key",
                self.non_exportable_public_key.as_str(),
            ),
            ("pinned_trust_anchor", self.pinned_trust_anchor.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        Ok(())
    }
}

impl HumanKeyV1 {
    fn validate_record(&self, now_ms: u64) -> Result<(), IdentityError> {
        for (field, value) in [
            ("key_id", self.key_id.as_str()),
            ("subject_id", self.subject_id.as_str()),
            ("platform", self.platform.as_str()),
            ("public_key", self.public_key.as_str()),
            ("attestation_class", self.attestation_class.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        if self.created_at > now_ms {
            return Err(IdentityError::IssuedInFuture {
                record: "human key",
                issued_at: self.created_at,
                now_ms,
            });
        }
        for changed_at in [self.rotated_at, self.revoked_at].into_iter().flatten() {
            if changed_at < self.created_at {
                return Err(IdentityError::InvalidTimeOrder {
                    record: "human key lifecycle",
                    issued_at: self.created_at,
                    expires_at: changed_at,
                });
            }
        }
        if self.status == IdentityStatus::Active
            && (self.rotated_at.is_some() || self.revoked_at.is_some())
        {
            return Err(IdentityError::InactiveStatus {
                record: "human key with terminal lifecycle timestamp",
                status: self.status,
            });
        }
        if self.status == IdentityStatus::Revoked && self.revoked_at.is_none() {
            return Err(IdentityError::InactiveStatus {
                record: "human key without revoked_at",
                status: self.status,
            });
        }
        if self.status == IdentityStatus::Rotated && self.rotated_at.is_none() {
            return Err(IdentityError::InactiveStatus {
                record: "human key without rotated_at",
                status: self.status,
            });
        }
        Ok(())
    }

    fn validate_for_approval(&self, now_ms: u64) -> Result<(), IdentityError> {
        self.validate_record(now_ms)?;
        match self.status {
            IdentityStatus::Active => Ok(()),
            IdentityStatus::Revoked => Err(IdentityError::Revoked {
                record: "human key",
            }),
            IdentityStatus::Expired => Err(IdentityError::ExpiredStatus {
                record: "human key",
            }),
            IdentityStatus::Rotated => Err(IdentityError::Rotated {
                record: "human key",
            }),
        }
    }
}

impl HumanKeyRegistryV1 {
    pub fn validate(&self, now_ms: u64) -> Result<(), IdentityError> {
        require_non_empty("owner_id", &self.owner_id)?;
        if self.keys.is_empty() {
            return Err(IdentityError::EmptyCollection { field: "keys" });
        }
        for (map_key, key) in &self.keys {
            require_non_empty("keys map key", map_key)?;
            key.validate_record(now_ms)?;
            if map_key != &key.key_id {
                return Err(IdentityError::HumanKeyMapMismatch {
                    map_key: map_key.clone(),
                    embedded_key_id: key.key_id.clone(),
                });
            }
        }
        Ok(())
    }

    fn active_key(&self, key_id: &str, now_ms: u64) -> Result<&HumanKeyV1, IdentityError> {
        let key = self
            .keys
            .get(key_id)
            .ok_or_else(|| IdentityError::HumanKeyNotFound {
                key_id: key_id.to_owned(),
            })?;
        key.validate_for_approval(now_ms)?;
        Ok(key)
    }
}

impl OwnerChallengeV1 {
    pub fn canonical_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(OWNER_CHALLENGE_DIGEST_DOMAIN, self)
    }

    /// Validate owner pin, lifecycle, HUMAN-only variant, and all immutable
    /// structural fields. The owner signature remains explicitly unverified.
    pub fn validate(
        &self,
        owner: &OwnerIdentityV1,
        now_ms: u64,
    ) -> Result<IdentityStructuralValidation, IdentityError> {
        owner.validate()?;
        for (field, value) in [
            ("challenge_id", self.challenge_id.as_str()),
            ("intent_digest", self.intent_digest.as_str()),
            ("intent_core_ref", self.intent_core_ref.as_str()),
            (
                "intent_canonicalization_version",
                self.intent_canonicalization_version.as_str(),
            ),
            ("organism_id", self.organism_id.as_str()),
            ("repo_id", self.repo_id.as_str()),
            ("issuer_subject_id", self.issuer_subject_id.as_str()),
            ("decision_subject_id", self.decision_subject_id.as_str()),
            ("caller_subject_id", self.caller_subject_id.as_str()),
            ("proposer_subject_id", self.proposer_subject_id.as_str()),
            ("audience", self.audience.as_str()),
            (
                "session_context_digest",
                self.session_context_digest.as_str(),
            ),
            ("action", self.action.as_str()),
            (
                "action_policy_registry_digest",
                self.action_policy_registry_digest.as_str(),
            ),
            (
                "classifier_decision_digest",
                self.classifier_decision_digest.as_str(),
            ),
            ("constitution_digest", self.constitution_digest.as_str()),
            ("brain_id", self.brain_id.as_str()),
            ("risk_scope_digest", self.risk_scope_digest.as_str()),
            ("idempotency_key", self.idempotency_key.as_str()),
            ("payload_digest", self.payload_digest.as_str()),
            ("canonical_summary", self.canonical_summary.as_str()),
            ("nonce", self.nonce.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        for (field, value) in [
            ("executor_subject_id", self.executor_subject_id.as_deref()),
            (
                "delegation_grant_digest",
                self.delegation_grant_digest.as_deref(),
            ),
            ("mission_id", self.mission_id.as_deref()),
            ("mission_head_id", self.mission_head_id.as_deref()),
            ("block_id", self.block_id.as_deref()),
            ("candidate_digest", self.candidate_digest.as_deref()),
        ] {
            validate_optional_non_empty(field, value)?;
        }
        require_signature("owner_signature", &self.owner_signature)?;

        if self.issuer_subject_id != owner.owner_id {
            return Err(IdentityError::WrongOwner {
                issuer: self.issuer_subject_id.clone(),
                owner_id: owner.owner_id.clone(),
            });
        }
        if self.required_authority_variant != AuthorityVariant::Human {
            return Err(IdentityError::WrongChallengeAuthority {
                actual: self.required_authority_variant,
            });
        }
        if self.active_mode == ActiveMode::FullAutonomy {
            return Err(IdentityError::WrongChallengeMode {
                active_mode: self.active_mode,
            });
        }
        if self.mission_id.is_some() != self.mission_head_id.is_some() {
            return Err(IdentityError::IncompleteMissionBinding);
        }
        if now_ms >= self.expires_at {
            return Err(IdentityError::ChallengeExpired {
                expires_at: self.expires_at,
                now_ms,
            });
        }

        Ok(IdentityStructuralValidation::opaque_signature_unverified())
    }
}

impl HumanApprovalV1 {
    pub fn canonical_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(HUMAN_APPROVAL_DIGEST_DOMAIN, self)
    }

    /// Validate immutable challenge binding and active key lifecycle. Neither
    /// the owner nor human opaque signature is treated as authentic.
    pub fn validate(
        &self,
        challenge: &OwnerChallengeV1,
        owner: &OwnerIdentityV1,
        registry: &HumanKeyRegistryV1,
        now_ms: u64,
    ) -> Result<IdentityStructuralValidation, IdentityError> {
        for (field, value) in [
            ("challenge_id", self.challenge_id.as_str()),
            (
                "canonical_challenge_digest",
                self.canonical_challenge_digest.as_str(),
            ),
            ("key_id", self.key_id.as_str()),
            ("subject_id", self.subject_id.as_str()),
            ("user_presence_flags", self.user_presence_flags.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        require_signature("signature", &self.signature)?;
        challenge.validate(owner, now_ms)?;
        registry.validate(now_ms)?;
        if registry.owner_id != owner.owner_id {
            return Err(IdentityError::WrongOwner {
                issuer: registry.owner_id.clone(),
                owner_id: owner.owner_id.clone(),
            });
        }
        if self.challenge_id != challenge.challenge_id {
            return Err(IdentityError::ChallengeIdMismatch);
        }
        let expected_challenge_digest = challenge.canonical_digest()?;
        if self.canonical_challenge_digest != expected_challenge_digest {
            return Err(IdentityError::ChallengeDigestMismatch {
                expected: expected_challenge_digest,
                observed: self.canonical_challenge_digest.clone(),
            });
        }
        if self.subject_id != challenge.decision_subject_id {
            return Err(IdentityError::ApprovalDecisionSubjectMismatch {
                approval_subject: self.subject_id.clone(),
                decision_subject: challenge.decision_subject_id.clone(),
            });
        }
        let key = registry.active_key(&self.key_id, now_ms)?;
        if self.subject_id != key.subject_id {
            return Err(IdentityError::ApprovalKeySubjectMismatch {
                approval_subject: self.subject_id.clone(),
                key_subject: key.subject_id.clone(),
            });
        }

        Ok(IdentityStructuralValidation::opaque_signature_unverified())
    }
}

impl HumanDecisionV1 {
    /// Validate that the decision copies the immutable intent and exact full
    /// approval digest. No capability is created and no signature is verified.
    pub fn validate(
        &self,
        challenge: &OwnerChallengeV1,
        approval: &HumanApprovalV1,
        owner: &OwnerIdentityV1,
        registry: &HumanKeyRegistryV1,
        now_ms: u64,
    ) -> Result<IdentityStructuralValidation, IdentityError> {
        for (field, value) in [
            ("decision_id", self.decision_id.as_str()),
            ("intent_digest", self.intent_digest.as_str()),
            ("intent_core_ref", self.intent_core_ref.as_str()),
            (
                "intent_canonicalization_version",
                self.intent_canonicalization_version.as_str(),
            ),
            ("human_approval_digest", self.human_approval_digest.as_str()),
            ("issuer_subject_id", self.issuer_subject_id.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        approval.validate(challenge, owner, registry, now_ms)?;

        for (field, matches) in [
            (
                "intent_digest",
                self.intent_digest == challenge.intent_digest,
            ),
            (
                "intent_core_ref",
                self.intent_core_ref == challenge.intent_core_ref,
            ),
            (
                "intent_canonicalization_version",
                self.intent_canonicalization_version == challenge.intent_canonicalization_version,
            ),
            (
                "issuer_subject_id",
                self.issuer_subject_id == challenge.issuer_subject_id,
            ),
        ] {
            if !matches {
                return Err(IdentityError::DecisionChallengeMismatch { field });
            }
        }

        let expected_approval_digest = approval.canonical_digest()?;
        if self.human_approval_digest != expected_approval_digest {
            return Err(IdentityError::ApprovalDigestMismatch {
                expected: expected_approval_digest,
                observed: self.human_approval_digest.clone(),
            });
        }

        Ok(IdentityStructuralValidation::opaque_signature_unverified())
    }
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), IdentityError> {
    if value.trim().is_empty() {
        return Err(IdentityError::EmptyRequired { field });
    }
    Ok(())
}

fn validate_optional_non_empty(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), IdentityError> {
    if value.is_some_and(|value| value.trim().is_empty()) {
        return Err(IdentityError::EmptyRequired { field });
    }
    Ok(())
}

fn require_signature(
    field: &'static str,
    signature: &OpaqueSignature,
) -> Result<(), IdentityError> {
    if signature.as_str().trim().is_empty() {
        return Err(IdentityError::EmptyRequired { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;

    const NOW: u64 = 1_000;

    fn owner() -> OwnerIdentityV1 {
        OwnerIdentityV1 {
            owner_id: "owner:local".into(),
            key_id: "owner-key:1".into(),
            non_exportable_public_key: "owner-public-key".into(),
            pinned_trust_anchor: "sha256:owner-anchor".into(),
            protected_latest_epoch: 7,
        }
    }

    fn scopes() -> BTreeSet<String> {
        BTreeSet::from(["graph:read".into(), "graph:write".into()])
    }

    fn enrollment() -> EnrollmentEvidenceV1 {
        EnrollmentEvidenceV1 {
            enrollment_id: "enrollment:1".into(),
            issuer: "owner:local".into(),
            issuer_key_id: "owner-key:1".into(),
            algorithm: "OPAQUE_TEST_ONLY".into(),
            subject_id: "client:codex".into(),
            key_id: "client-key:1".into(),
            public_key_digest: EnrollmentEvidenceV1::public_key_digest("client-public-key"),
            app_host_identity: "app:codex@host:local".into(),
            scopes_digest: EnrollmentEvidenceV1::scopes_digest(&scopes()).unwrap(),
            audience: "m1nd-owner".into(),
            nonce: "enrollment-nonce:1".into(),
            session_context_digest: "sha256:session-context".into(),
            issued_at: 900,
            expires_at: 1_100,
            signature: OpaqueSignature::new("opaque-enrollment-signature"),
        }
    }

    fn client() -> ClientIdentityV1 {
        ClientIdentityV1 {
            subject_id: "client:codex".into(),
            key_id: "client-key:1".into(),
            public_key: "client-public-key".into(),
            app_host_identity: "app:codex@host:local".into(),
            enrollment_evidence: enrollment(),
            scopes: scopes(),
            created_at: 910,
            revoked_at: None,
            status: IdentityStatus::Active,
        }
    }

    fn human_key(status: IdentityStatus) -> HumanKeyV1 {
        HumanKeyV1 {
            key_id: "human-key:1".into(),
            subject_id: "human:owner".into(),
            platform: "macos-secure-enclave".into(),
            public_key: "human-public-key".into(),
            attestation_class: "USER_VERIFICATION".into(),
            created_at: 100,
            rotated_at: None,
            revoked_at: None,
            status,
        }
    }

    fn registry_with(key: HumanKeyV1) -> HumanKeyRegistryV1 {
        HumanKeyRegistryV1 {
            owner_id: "owner:local".into(),
            registry_epoch: 3,
            keys: BTreeMap::from([(key.key_id.clone(), key)]),
        }
    }

    fn challenge() -> OwnerChallengeV1 {
        OwnerChallengeV1 {
            challenge_id: "challenge:1".into(),
            intent_digest: "sha256:intent".into(),
            intent_core_ref: "intent:sha256:intent".into(),
            intent_canonicalization_version: "m1nd-canonical-json-v1".into(),
            organism_id: "organism:local".into(),
            repo_id: "repo:m1nd".into(),
            issuer_subject_id: "owner:local".into(),
            decision_subject_id: "human:owner".into(),
            caller_subject_id: "client:codex".into(),
            proposer_subject_id: "human:owner".into(),
            executor_subject_id: Some("runner:local".into()),
            delegation_grant_digest: None,
            audience: "m1nd-owner".into(),
            session_context_digest: "sha256:session-context".into(),
            action: ActionId::new("land").unwrap(),
            required_authority_variant: AuthorityVariant::Human,
            action_policy_registry_digest: "sha256:policy".into(),
            classifier_decision_digest: "sha256:classifier".into(),
            active_mode: ActiveMode::HumanGated,
            constitution_digest: "sha256:constitution".into(),
            constitution_epoch: 1,
            autonomy_epoch: 1,
            brain_id: "brain:default".into(),
            mission_id: Some("mission:1".into()),
            mission_head_id: Some("letter:3".into()),
            block_id: Some("block:landing".into()),
            candidate_digest: Some("sha256:candidate".into()),
            risk_scope_digest: "sha256:risk-scope".into(),
            expected_store_epoch: 8,
            expected_store_version: 12,
            expected_boundary_version: 4,
            expected_contract_version: 2,
            idempotency_key: "idem:land:1".into(),
            payload_digest: "sha256:payload".into(),
            canonical_summary: "Land candidate sha256:candidate".into(),
            nonce: "challenge-nonce:1".into(),
            expires_at: 1_100,
            owner_signature: OpaqueSignature::new("opaque-owner-signature"),
        }
    }

    fn approval(challenge: &OwnerChallengeV1) -> HumanApprovalV1 {
        HumanApprovalV1 {
            challenge_id: challenge.challenge_id.clone(),
            canonical_challenge_digest: challenge.canonical_digest().unwrap(),
            key_id: "human-key:1".into(),
            subject_id: "human:owner".into(),
            user_presence_flags: "UP,UV".into(),
            counter: 42,
            signature: OpaqueSignature::new("opaque-human-signature"),
        }
    }

    fn decision(challenge: &OwnerChallengeV1, approval: &HumanApprovalV1) -> HumanDecisionV1 {
        HumanDecisionV1 {
            decision_id: "decision:1".into(),
            intent_digest: challenge.intent_digest.clone(),
            intent_core_ref: challenge.intent_core_ref.clone(),
            intent_canonicalization_version: challenge.intent_canonicalization_version.clone(),
            human_approval_digest: approval.canonical_digest().unwrap(),
            issuer_subject_id: challenge.issuer_subject_id.clone(),
        }
    }

    #[test]
    fn client_identity_exact_wire_shape_and_unknown_fields() {
        let identity = client();
        let Value::Object(object) = serde_json::to_value(&identity).unwrap() else {
            panic!("identity must be an object");
        };
        assert_eq!(
            object.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "app_host_identity",
                "created_at",
                "enrollment_evidence",
                "key_id",
                "public_key",
                "revoked_at",
                "scopes",
                "status",
                "subject_id",
            ])
        );
        let mut value = serde_json::to_value(&identity).unwrap();
        value["ambient_uid_is_authority"] = json!(true);
        assert!(serde_json::from_value::<ClientIdentityV1>(value).is_err());
    }

    #[test]
    fn enrollment_requires_nonce_audience_session_context_and_live_window() {
        let mut identity = client();
        let validation = identity.validate(NOW).unwrap();
        assert_eq!(
            validation.integrity,
            IdentityIntegrityDisposition::OpaqueSignaturePresentUnverified
        );

        identity.enrollment_evidence.nonce.clear();
        assert!(matches!(
            identity.validate(NOW),
            Err(IdentityError::EmptyRequired { field: "nonce" })
        ));

        let mut expired = client();
        expired.enrollment_evidence.expires_at = NOW;
        assert!(matches!(
            expired.validate(NOW),
            Err(IdentityError::Expired {
                record: "enrollment evidence",
                ..
            })
        ));
    }

    #[test]
    fn enrollment_digest_bindings_cannot_be_swapped() {
        let mut identity = client();
        identity.public_key = "attacker-public-key".into();
        assert!(matches!(
            identity.validate(NOW),
            Err(IdentityError::EnrollmentBindingMismatch {
                field: "public_key_digest"
            })
        ));

        let mut identity = client();
        identity.scopes.insert("source:write".into());
        assert!(matches!(
            identity.validate(NOW),
            Err(IdentityError::EnrollmentBindingMismatch {
                field: "scopes_digest"
            })
        ));
    }

    #[test]
    fn enrollment_issuer_and_key_are_pinned_to_owner_identity() {
        let identity = client();
        assert!(identity.validate_for_owner(&owner(), NOW).is_ok());

        let mut wrong_owner = owner();
        wrong_owner.key_id = "owner-key:attacker".into();
        assert!(matches!(
            identity.validate_for_owner(&wrong_owner, NOW),
            Err(IdentityError::EnrollmentBindingMismatch {
                field: "issuer_key_id"
            })
        ));
    }

    #[test]
    fn revoked_and_expired_identities_fail_closed() {
        let mut revoked = client();
        revoked.status = IdentityStatus::Revoked;
        revoked.revoked_at = Some(950);
        assert!(matches!(
            revoked.validate(NOW),
            Err(IdentityError::Revoked { .. })
        ));

        let mut expired = client();
        expired.status = IdentityStatus::Expired;
        assert!(matches!(
            expired.validate(NOW),
            Err(IdentityError::InactiveStatus {
                status: IdentityStatus::Expired,
                ..
            })
        ));
    }

    #[test]
    fn challenge_has_exact_immutable_wire_fields_and_denies_unknown_fields() {
        let challenge = challenge();
        let Value::Object(object) = serde_json::to_value(&challenge).unwrap() else {
            panic!("challenge must be an object");
        };
        assert_eq!(object.len(), 38);
        for required in [
            "intent_digest",
            "intent_core_ref",
            "audience",
            "session_context_digest",
            "nonce",
            "owner_signature",
            "action_policy_registry_digest",
            "classifier_decision_digest",
        ] {
            assert!(object.contains_key(required), "missing {required}");
        }

        let mut value = serde_json::to_value(&challenge).unwrap();
        value["authority_bypass"] = json!(true);
        assert!(serde_json::from_value::<OwnerChallengeV1>(value).is_err());
    }

    #[test]
    fn challenge_is_human_only_owner_pinned_and_half_open_expiry() {
        let mut value = challenge();
        assert_eq!(
            value.validate(&owner(), NOW).unwrap().integrity,
            IdentityIntegrityDisposition::OpaqueSignaturePresentUnverified
        );

        value.required_authority_variant = AuthorityVariant::Policy;
        assert!(matches!(
            value.validate(&owner(), NOW),
            Err(IdentityError::WrongChallengeAuthority { .. })
        ));

        let mut expired = challenge();
        expired.expires_at = NOW;
        assert!(matches!(
            expired.validate(&owner(), NOW),
            Err(IdentityError::ChallengeExpired { .. })
        ));
    }

    #[test]
    fn approval_binds_complete_challenge_bytes_and_active_human_key() {
        let owner = owner();
        let registry = registry_with(human_key(IdentityStatus::Active));
        let challenge = challenge();
        let approval = approval(&challenge);
        assert_eq!(
            approval
                .validate(&challenge, &owner, &registry, NOW)
                .unwrap()
                .integrity,
            IdentityIntegrityDisposition::OpaqueSignaturePresentUnverified
        );

        let mut mutated_challenge = challenge.clone();
        mutated_challenge.payload_digest = "sha256:different-payload".into();
        assert!(matches!(
            approval.validate(&mutated_challenge, &owner, &registry, NOW),
            Err(IdentityError::ChallengeDigestMismatch { .. })
        ));

        let mut unknown = serde_json::to_value(&approval).unwrap();
        unknown["authentic"] = json!(true);
        assert!(serde_json::from_value::<HumanApprovalV1>(unknown).is_err());
    }

    #[test]
    fn revoked_expired_and_rotated_human_keys_cannot_approve() {
        let owner = owner();
        let challenge = challenge();
        let approval = approval(&challenge);

        let mut revoked_key = human_key(IdentityStatus::Revoked);
        revoked_key.revoked_at = Some(900);
        assert!(matches!(
            approval.validate(&challenge, &owner, &registry_with(revoked_key), NOW),
            Err(IdentityError::Revoked {
                record: "human key"
            })
        ));

        assert!(matches!(
            approval.validate(
                &challenge,
                &owner,
                &registry_with(human_key(IdentityStatus::Expired)),
                NOW
            ),
            Err(IdentityError::ExpiredStatus {
                record: "human key"
            })
        ));

        let mut rotated_key = human_key(IdentityStatus::Rotated);
        rotated_key.rotated_at = Some(900);
        assert!(matches!(
            approval.validate(&challenge, &owner, &registry_with(rotated_key), NOW),
            Err(IdentityError::Rotated {
                record: "human key"
            })
        ));
    }

    #[test]
    fn decision_binds_same_intent_and_full_approval_digest() {
        let owner = owner();
        let registry = registry_with(human_key(IdentityStatus::Active));
        let challenge = challenge();
        let approval = approval(&challenge);
        let decision = decision(&challenge, &approval);
        assert!(decision
            .validate(&challenge, &approval, &owner, &registry, NOW)
            .is_ok());

        let mut mutated_approval = approval.clone();
        mutated_approval.counter += 1;
        assert!(matches!(
            decision.validate(&challenge, &mutated_approval, &owner, &registry, NOW),
            Err(IdentityError::ApprovalDigestMismatch { .. })
        ));

        let mut wrong_intent = decision;
        wrong_intent.intent_digest = "sha256:other-intent".into();
        assert!(matches!(
            wrong_intent.validate(&challenge, &approval, &owner, &registry, NOW),
            Err(IdentityError::DecisionChallengeMismatch {
                field: "intent_digest"
            })
        ));
    }

    #[test]
    fn owner_and_registry_exact_wire_shapes_reject_unknown_fields() {
        fn wire_keys(value: Value) -> BTreeSet<String> {
            value
                .as_object()
                .expect("contract must serialize as an object")
                .keys()
                .cloned()
                .collect()
        }

        assert_eq!(
            wire_keys(serde_json::to_value(owner()).unwrap()),
            BTreeSet::from([
                "key_id".into(),
                "non_exportable_public_key".into(),
                "owner_id".into(),
                "pinned_trust_anchor".into(),
                "protected_latest_epoch".into(),
            ])
        );
        assert_eq!(
            wire_keys(serde_json::to_value(enrollment()).unwrap()),
            BTreeSet::from([
                "algorithm".into(),
                "app_host_identity".into(),
                "audience".into(),
                "enrollment_id".into(),
                "expires_at".into(),
                "issued_at".into(),
                "issuer".into(),
                "issuer_key_id".into(),
                "key_id".into(),
                "nonce".into(),
                "public_key_digest".into(),
                "scopes_digest".into(),
                "session_context_digest".into(),
                "signature".into(),
                "subject_id".into(),
            ])
        );
        assert_eq!(
            wire_keys(serde_json::to_value(human_key(IdentityStatus::Active)).unwrap()),
            BTreeSet::from([
                "attestation_class".into(),
                "created_at".into(),
                "key_id".into(),
                "platform".into(),
                "public_key".into(),
                "revoked_at".into(),
                "rotated_at".into(),
                "status".into(),
                "subject_id".into(),
            ])
        );

        let registry = registry_with(human_key(IdentityStatus::Active));
        let challenge = challenge();
        let approval = approval(&challenge);
        assert_eq!(
            wire_keys(serde_json::to_value(&registry).unwrap()),
            BTreeSet::from(["keys".into(), "owner_id".into(), "registry_epoch".into()])
        );
        assert_eq!(
            wire_keys(serde_json::to_value(&approval).unwrap()),
            BTreeSet::from([
                "canonical_challenge_digest".into(),
                "challenge_id".into(),
                "counter".into(),
                "key_id".into(),
                "signature".into(),
                "subject_id".into(),
                "user_presence_flags".into(),
            ])
        );
        assert_eq!(
            wire_keys(serde_json::to_value(decision(&challenge, &approval)).unwrap()),
            BTreeSet::from([
                "decision_id".into(),
                "human_approval_digest".into(),
                "intent_canonicalization_version".into(),
                "intent_core_ref".into(),
                "intent_digest".into(),
                "issuer_subject_id".into(),
            ])
        );

        let mut registry_value = serde_json::to_value(registry).unwrap();
        registry_value["fallback_key"] = json!("ambient-user");
        assert!(serde_json::from_value::<HumanKeyRegistryV1>(registry_value).is_err());

        let mut human_key_value = serde_json::to_value(human_key(IdentityStatus::Active)).unwrap();
        human_key_value["ambient_uid"] = json!(501);
        assert!(serde_json::from_value::<HumanKeyV1>(human_key_value).is_err());
    }

    #[test]
    fn empty_opaque_signatures_are_structurally_rejected_not_authenticated() {
        let mut challenge = challenge();
        challenge.owner_signature = OpaqueSignature::new(" ");
        assert!(matches!(
            challenge.validate(&owner(), NOW),
            Err(IdentityError::EmptyRequired {
                field: "owner_signature"
            })
        ));
    }
}
