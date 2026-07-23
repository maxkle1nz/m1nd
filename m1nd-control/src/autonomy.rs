//! Constitutional autonomy contracts for M1nd 10.
//!
//! This module is deliberately dependency-inward. It defines canonical wire
//! contracts and fail-closed structural/semantic validators, but it does not
//! verify signatures, persist records, provide protected clocks, or perform
//! runtime side effects. Every signature remains explicitly opaque until a
//! caller supplies the G2 cryptographic and protected-store implementation.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::policy::{ActiveMode, AuthorityVariant, AutonomyTier, Effect, RiskClass};
use crate::{
    canonical_json, digest_canonical, digest_domain_bytes, ActionId, CanonicalError,
    OpaqueSignature, CANONICALIZATION_VERSION,
};

pub const SAFETY_KERNEL_SCHEMA: &str = "m1nd-safety-kernel-v1";
pub const CONSTITUTION_SCHEMA: &str = "m1nd-constitution-store-v1";
pub const CONSTITUTION_AMENDMENT_SCHEMA: &str = "m1nd-constitution-amendment-v1";
pub const INDEPENDENCE_SPEC_SCHEMA: &str = "m1nd-independence-spec-v1";
pub const AUTONOMY_EPOCH_SCHEMA: &str = "m1nd-autonomy-epoch-v1";
pub const AUTONOMY_GRANT_SCHEMA: &str = "m1nd-autonomy-grant-v1";
pub const SOVEREIGN_ACTION_INTENT_SCHEMA: &str = "m1nd-sovereign-action-intent-v1";
pub const SENTINEL_VERDICT_SCHEMA: &str = "m1nd-sentinel-verdict-v1";
pub const SENTINEL_RED_OUTBOX_SCHEMA: &str = "m1nd-sentinel-red-outbox-v1";
pub const RED_LATCH_RECEIPT_SCHEMA: &str = "m1nd-red-latch-receipt-v1";
pub const SAFETY_ACTION_INTENT_SCHEMA: &str = "m1nd-safety-action-intent-v1";
pub const SAFETY_CAPABILITY_SCHEMA: &str = "m1nd-safety-capability-v1";
pub const AUTHORITY_DECISION_SCHEMA: &str = "m1nd-authority-decision-v1";
pub const AUTONOMY_CAPABILITY_SCHEMA: &str = "m1nd-autonomy-capability-v1";
pub const AUTONOMY_ACTIVATION_RECEIPT_SCHEMA: &str = "m1nd-autonomy-activation-receipt-v1";

pub const SAFETY_KERNEL_DIGEST_DOMAIN: &str = "m1nd-safety-kernel-v1";
pub const CONSTITUTION_DIGEST_DOMAIN: &str = "m1nd-constitution-v1";
pub const CONSTITUTION_AMENDMENT_DIGEST_DOMAIN: &str = "m1nd-constitution-amendment-v1";
pub const INDEPENDENCE_SPEC_DIGEST_DOMAIN: &str = "m1nd-independence-spec-v1";
pub const AUTONOMY_GRANT_DIGEST_DOMAIN: &str = "m1nd-autonomy-grant-v1";
pub const AUTONOMY_GRANTS_DIGEST_DOMAIN: &str = "m1nd-autonomy-grants-v1";
pub const SOVEREIGN_INTENT_DIGEST_DOMAIN: &str = "m1nd-sovereign-intent-v1";
pub const SENTINEL_VERDICT_DIGEST_DOMAIN: &str = "m1nd-sentinel-verdict-v1";
pub const SENTINEL_RED_OUTBOX_RECORD_DIGEST_DOMAIN: &str = "m1nd-sentinel-red-outbox-record-v1";
pub const RED_LATCH_RECEIPT_DIGEST_DOMAIN: &str = "m1nd-red-latch-receipt-v1";
pub const SAFETY_ACTION_INTENT_DIGEST_DOMAIN: &str = "m1nd-safety-action-intent-v1";
pub const SAFETY_EFFECTS_DIGEST_DOMAIN: &str = "m1nd-safety-negative-effects-v1";
pub const SAFETY_CAPABILITY_DIGEST_DOMAIN: &str = "m1nd-safety-capability-v1";
pub const AUTHORITY_DECISION_DIGEST_DOMAIN: &str = "m1nd-authority-decision-v1";
pub const AUTONOMY_CAPABILITY_DIGEST_DOMAIN: &str = "m1nd-autonomy-capability-v1";
pub const AUTONOMY_ACTIVATION_RECEIPT_DIGEST_DOMAIN: &str = "m1nd-autonomy-activation-receipt-v1";
pub const AUTONOMY_EPOCH_REFERENCE_DIGEST_DOMAIN: &str = "m1nd-autonomy-epoch-reference-v1";

pub const IMMUTABLE_VERIFIER_SEATS: u16 = 4;
pub const IMMUTABLE_QUORUM_THRESHOLD: u16 = 3;
pub const IMMUTABLE_FAILURE_DOMAINS: u16 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutonomyIntegrityDisposition {
    OpaqueSignaturePresentUnverified,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutonomyStructuralValidation {
    pub computed_digest: String,
    pub integrity: AutonomyIntegrityDisposition,
}

impl AutonomyStructuralValidation {
    fn opaque(computed_digest: String) -> Self {
        Self {
            computed_digest,
            integrity: AutonomyIntegrityDisposition::OpaqueSignaturePresentUnverified,
        }
    }
}

#[derive(Debug, Error)]
pub enum AutonomyContractError {
    #[error("unsupported {contract} schema '{actual}'")]
    Schema {
        contract: &'static str,
        actual: String,
    },
    #[error("required field '{field}' is empty")]
    EmptyRequired { field: &'static str },
    #[error("required collection '{field}' is empty")]
    EmptyCollection { field: &'static str },
    #[error("digest field '{field}' is not a lowercase SHA-256 hex digest")]
    InvalidDigest { field: &'static str },
    #[error("opaque signature field '{field}' is empty")]
    EmptySignature { field: &'static str },
    #[error("{record} has invalid time order: {issued_at} >= {expires_at}")]
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
    #[error("{record} was issued in the future at {issued_at}; validation time is {now_ms}")]
    IssuedInFuture {
        record: &'static str,
        issued_at: u64,
        now_ms: u64,
    },
    #[error("digest mismatch for '{field}': expected {expected}, observed {observed}")]
    DigestMismatch {
        field: &'static str,
        expected: String,
        observed: String,
    },
    #[error("binding mismatch for '{field}'")]
    BindingMismatch { field: &'static str },
    #[error("epoch mismatch for '{field}': expected {expected}, observed {observed}")]
    EpochMismatch {
        field: &'static str,
        expected: u64,
        observed: u64,
    },
    #[error("mode mismatch for '{field}': expected {expected:?}, observed {observed:?}")]
    ModeMismatch {
        field: &'static str,
        expected: ActiveMode,
        observed: ActiveMode,
    },
    #[error("authority mismatch: expected {expected:?}, observed {observed:?}")]
    AuthorityMismatch {
        expected: AuthorityVariant,
        observed: AuthorityVariant,
    },
    #[error("immutable safety-kernel invariant violated: {rule}")]
    KernelFloor { rule: &'static str },
    #[error("constitutional invariant violated: {rule}")]
    ConstitutionInvariant { rule: &'static str },
    #[error("autonomy invariant violated: {rule}")]
    Invariant { rule: &'static str },
    #[error("self-promotion or self-ratification by subject '{subject_id}' is forbidden")]
    SelfAuthorization { subject_id: String },
    #[error("quorum principal/key/context membership is not independent")]
    NonIndependentQuorum,
    #[error("quorum has {approvals} approvals; at least {required} are required")]
    InsufficientQuorum { approvals: usize, required: usize },
    #[error("quorum contains a dissent or abstention")]
    QuorumNotUnanimouslyResolvable,
    #[error("action class '{action_class}' is outside the grant")]
    ActionOutsideGrant { action_class: String },
    #[error("risk class {risk_class:?} is outside the grant")]
    RiskOutsideGrant { risk_class: RiskClass },
    #[error("resource/environment scope does not equal the grant scope")]
    ScopeOutsideGrant,
    #[error("requested budget {requested} exceeds remaining grant budget {remaining}")]
    BudgetExceeded { requested: u64, remaining: u64 },
    #[error("effect {effect:?} is not a negative safety-kernel effect")]
    PositiveEffectInSafety { effect: Effect },
    #[error("safety effect set differs from the immutable kernel allow-list")]
    SafetyAllowListMismatch,
    #[error("intent core contains a forbidden post-intent field '{field}'")]
    CyclicIntentField { field: String },
    #[error("activation transition {previous:?} -> {target:?} is not allowed")]
    InvalidActivationTransition {
        previous: ActiveMode,
        target: ActiveMode,
    },
    #[error(transparent)]
    Canonical(#[from] CanonicalError),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyKernelCoreV1 {
    pub kernel_id: String,
    pub verifier_binary_digest: String,
    pub canonicalization_version: String,
    pub pinned_external_root_key: String,
    pub verified_boot_policy_digest: String,
    pub immutable_invariants_digest: String,
    pub minimum_verifier_seats: u16,
    pub minimum_quorum_threshold: u16,
    pub minimum_failure_domains: u16,
    pub proposer_executor_nonvoting: bool,
    pub sentinel_required_and_nonvoting: bool,
    pub sentinel_red_absolute_veto: bool,
    pub sentinel_outbox_antirollback_required: bool,
    pub sentinel_identity_key_binary_policy_digest: String,
    pub safety_actuator_identity_key_binary_policy_digest: String,
    pub required_sentinel_unavailable_fail_closed: bool,
    pub audit_wal_tamper_detection_required: bool,
    pub epoch_freeze_and_rollback_required: bool,
    pub old_runtime_approval_required: bool,
    pub allowed_negative_effects: BTreeSet<Effect>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyKernelV1 {
    pub schema: String,
    pub core: SafetyKernelCoreV1,
    pub kernel_digest: String,
    pub external_root_signature: OpaqueSignature,
}

impl SafetyKernelV1 {
    pub fn canonical_negative_effects() -> BTreeSet<Effect> {
        [
            Effect::FreezeIssuance,
            Effect::EpochFence,
            Effect::EpochBump,
            Effect::RevokeCapability,
            Effect::AbortPrepared,
            Effect::DemoteGrant,
            Effect::RollbackSignedCandidate,
        ]
        .into_iter()
        .collect()
    }

    pub fn compute_kernel_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(SAFETY_KERNEL_DIGEST_DOMAIN, &self.core)
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.kernel_digest = self.compute_kernel_digest()?;
        Ok(())
    }

    pub fn validate(&self) -> Result<AutonomyStructuralValidation, AutonomyContractError> {
        require_schema("SafetyKernelV1", &self.schema, SAFETY_KERNEL_SCHEMA)?;
        require_non_empty("kernel_id", &self.core.kernel_id)?;
        require_non_empty(
            "canonicalization_version",
            &self.core.canonicalization_version,
        )?;
        if self.core.canonicalization_version != CANONICALIZATION_VERSION {
            return Err(AutonomyContractError::KernelFloor {
                rule: "canonicalization version must equal the runtime-pinned version",
            });
        }
        require_non_empty(
            "pinned_external_root_key",
            &self.core.pinned_external_root_key,
        )?;
        for (field, digest) in [
            ("verifier_binary_digest", &self.core.verifier_binary_digest),
            (
                "verified_boot_policy_digest",
                &self.core.verified_boot_policy_digest,
            ),
            (
                "immutable_invariants_digest",
                &self.core.immutable_invariants_digest,
            ),
            (
                "sentinel_identity_key_binary_policy_digest",
                &self.core.sentinel_identity_key_binary_policy_digest,
            ),
            (
                "safety_actuator_identity_key_binary_policy_digest",
                &self.core.safety_actuator_identity_key_binary_policy_digest,
            ),
        ] {
            require_digest(field, digest)?;
        }
        if self.core.minimum_verifier_seats != IMMUTABLE_VERIFIER_SEATS {
            return Err(AutonomyContractError::KernelFloor {
                rule: "verifier seats must be pinned to four",
            });
        }
        if self.core.minimum_quorum_threshold != IMMUTABLE_QUORUM_THRESHOLD {
            return Err(AutonomyContractError::KernelFloor {
                rule: "quorum threshold must be pinned to three-of-four",
            });
        }
        if self.core.minimum_failure_domains != IMMUTABLE_FAILURE_DOMAINS {
            return Err(AutonomyContractError::KernelFloor {
                rule: "at least three independent failure domains are immutable",
            });
        }
        for (enabled, rule) in [
            (
                self.core.proposer_executor_nonvoting,
                "proposer and executor must remain non-voting",
            ),
            (
                self.core.sentinel_required_and_nonvoting,
                "the independent sentinel must remain required and non-voting",
            ),
            (
                self.core.sentinel_red_absolute_veto,
                "sentinel RED must remain an absolute veto",
            ),
            (
                self.core.sentinel_outbox_antirollback_required,
                "sentinel outbox anti-rollback is mandatory",
            ),
            (
                self.core.required_sentinel_unavailable_fail_closed,
                "required sentinel unavailability must fail closed",
            ),
            (
                self.core.audit_wal_tamper_detection_required,
                "audit/WAL tamper detection is mandatory",
            ),
            (
                self.core.epoch_freeze_and_rollback_required,
                "epoch freeze and rollback are mandatory",
            ),
            (
                self.core.old_runtime_approval_required,
                "governance adoption requires old-runtime approval",
            ),
        ] {
            if !enabled {
                return Err(AutonomyContractError::KernelFloor { rule });
            }
        }
        if self.core.allowed_negative_effects != Self::canonical_negative_effects() {
            return Err(AutonomyContractError::SafetyAllowListMismatch);
        }
        require_digest("kernel_digest", &self.kernel_digest)?;
        let computed = self.compute_kernel_digest()?;
        require_digest_equality("kernel_digest", &computed, &self.kernel_digest)?;
        require_signature("external_root_signature", &self.external_root_signature)?;
        Ok(AutonomyStructuralValidation::opaque(computed))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifierSeatV1 {
    pub principal_id: String,
    pub key_id: String,
    pub failure_domain: String,
    pub parent_session_context_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndependenceSpecCoreV1 {
    pub constitution_epoch: u64,
    pub voting_verifiers: Vec<VerifierSeatV1>,
    pub quorum_threshold: u16,
    pub minimum_failure_domains: u16,
    pub blind_isolation_policy_digest: String,
    pub nonvoting_sentinel_id: String,
    pub proposer_executor_nonvoting: bool,
    pub sentinel_nonvoting: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndependenceSpecV1 {
    pub schema: String,
    pub core: IndependenceSpecCoreV1,
    pub independence_spec_digest: String,
}

impl IndependenceSpecV1 {
    pub fn compute_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(INDEPENDENCE_SPEC_DIGEST_DOMAIN, &self.core)
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.independence_spec_digest = self.compute_digest()?;
        Ok(())
    }

    pub fn validate_against_kernel(
        &self,
        kernel: &SafetyKernelV1,
    ) -> Result<(), AutonomyContractError> {
        kernel.validate()?;
        require_schema("IndependenceSpecV1", &self.schema, INDEPENDENCE_SPEC_SCHEMA)?;
        if self.core.voting_verifiers.len() != usize::from(kernel.core.minimum_verifier_seats) {
            return Err(AutonomyContractError::KernelFloor {
                rule: "constitution must preserve the four frozen voting seats",
            });
        }
        if self.core.quorum_threshold < kernel.core.minimum_quorum_threshold
            || usize::from(self.core.quorum_threshold) > self.core.voting_verifiers.len()
        {
            return Err(AutonomyContractError::KernelFloor {
                rule: "constitution cannot reduce the three-of-four quorum",
            });
        }
        if self.core.minimum_failure_domains < kernel.core.minimum_failure_domains {
            return Err(AutonomyContractError::KernelFloor {
                rule: "constitution cannot reduce failure-domain diversity",
            });
        }
        if !self.core.proposer_executor_nonvoting || !self.core.sentinel_nonvoting {
            return Err(AutonomyContractError::KernelFloor {
                rule: "proposer, executor, and sentinel remain non-voting",
            });
        }
        require_non_empty("nonvoting_sentinel_id", &self.core.nonvoting_sentinel_id)?;
        require_digest(
            "blind_isolation_policy_digest",
            &self.core.blind_isolation_policy_digest,
        )?;

        let mut principals = BTreeSet::new();
        let mut keys = BTreeSet::new();
        let mut contexts = BTreeSet::new();
        let mut domains = BTreeSet::new();
        for seat in &self.core.voting_verifiers {
            require_non_empty("verifier.principal_id", &seat.principal_id)?;
            require_non_empty("verifier.key_id", &seat.key_id)?;
            require_non_empty("verifier.failure_domain", &seat.failure_domain)?;
            require_digest(
                "verifier.parent_session_context_digest",
                &seat.parent_session_context_digest,
            )?;
            if !principals.insert(&seat.principal_id)
                || !keys.insert(&seat.key_id)
                || !contexts.insert(&seat.parent_session_context_digest)
            {
                return Err(AutonomyContractError::NonIndependentQuorum);
            }
            domains.insert(&seat.failure_domain);
        }
        if domains.len() < usize::from(self.core.minimum_failure_domains) {
            return Err(AutonomyContractError::NonIndependentQuorum);
        }
        if principals.contains(&self.core.nonvoting_sentinel_id) {
            return Err(AutonomyContractError::NonIndependentQuorum);
        }
        require_digest("independence_spec_digest", &self.independence_spec_digest)?;
        let computed = self.compute_digest()?;
        require_digest_equality(
            "independence_spec_digest",
            &computed,
            &self.independence_spec_digest,
        )
    }

    pub fn verifier_principals(&self) -> BTreeSet<&str> {
        self.core
            .voting_verifiers
            .iter()
            .map(|seat| seat.principal_id.as_str())
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstitutionCoreV1 {
    pub constitution_epoch: u64,
    pub previous_constitution_digest: Option<String>,
    pub effective_at: u64,
    pub expires_at: u64,
    pub allowed_autonomy_modes: BTreeSet<ActiveMode>,
    pub objectives: BTreeSet<String>,
    pub non_goals: BTreeSet<String>,
    pub resource_scope_digest: String,
    pub risk_budget_action_policy_digest: String,
    pub independence_spec_digest: String,
    pub metric_specs_digest: String,
    pub canary_requirements_digest: String,
    pub rollback_requirements_digest: String,
    pub amendment_rules_digest: String,
    pub previous_governance_runtime_digest: String,
    pub adopting_governance_runtime_digest: String,
    pub old_runtime_approval_digest: Option<String>,
    pub issuer_subject_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstitutionStoreV1 {
    pub schema: String,
    pub core: ConstitutionCoreV1,
    pub constitution_digest: String,
    pub signature: OpaqueSignature,
}

impl ConstitutionStoreV1 {
    pub fn compute_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(CONSTITUTION_DIGEST_DOMAIN, &self.core)
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.constitution_digest = self.compute_digest()?;
        Ok(())
    }

    pub fn validate(
        &self,
        independence: &IndependenceSpecV1,
        kernel: &SafetyKernelV1,
        now_ms: u64,
    ) -> Result<AutonomyStructuralValidation, AutonomyContractError> {
        require_schema("ConstitutionStoreV1", &self.schema, CONSTITUTION_SCHEMA)?;
        require_time_window(
            "ConstitutionStoreV1",
            self.core.effective_at,
            self.core.expires_at,
            now_ms,
        )?;
        if self.core.allowed_autonomy_modes.is_empty() {
            return Err(AutonomyContractError::EmptyCollection {
                field: "allowed_autonomy_modes",
            });
        }
        if !self
            .core
            .allowed_autonomy_modes
            .contains(&ActiveMode::HumanGated)
        {
            return Err(AutonomyContractError::ConstitutionInvariant {
                rule: "HUMAN_GATED must remain an allowed fail-closed mode",
            });
        }
        require_non_empty("issuer_subject_id", &self.core.issuer_subject_id)?;
        for (field, digest) in [
            ("resource_scope_digest", &self.core.resource_scope_digest),
            (
                "risk_budget_action_policy_digest",
                &self.core.risk_budget_action_policy_digest,
            ),
            (
                "independence_spec_digest",
                &self.core.independence_spec_digest,
            ),
            ("metric_specs_digest", &self.core.metric_specs_digest),
            (
                "canary_requirements_digest",
                &self.core.canary_requirements_digest,
            ),
            (
                "rollback_requirements_digest",
                &self.core.rollback_requirements_digest,
            ),
            ("amendment_rules_digest", &self.core.amendment_rules_digest),
            (
                "previous_governance_runtime_digest",
                &self.core.previous_governance_runtime_digest,
            ),
            (
                "adopting_governance_runtime_digest",
                &self.core.adopting_governance_runtime_digest,
            ),
        ] {
            require_digest(field, digest)?;
        }
        if let Some(previous) = &self.core.previous_constitution_digest {
            require_digest("previous_constitution_digest", previous)?;
        }
        if let Some(approval) = &self.core.old_runtime_approval_digest {
            require_digest("old_runtime_approval_digest", approval)?;
        }
        if self.core.constitution_epoch == 0 {
            if self.core.previous_constitution_digest.is_some()
                || self.core.old_runtime_approval_digest.is_some()
            {
                return Err(AutonomyContractError::ConstitutionInvariant {
                    rule: "bootstrap constitution cannot claim a previous constitution/runtime approval",
                });
            }
        } else if self.core.previous_constitution_digest.is_none()
            || self.core.old_runtime_approval_digest.is_none()
        {
            return Err(AutonomyContractError::ConstitutionInvariant {
                rule:
                    "non-bootstrap constitution requires previous digest and old-runtime approval",
            });
        }
        if self.core.constitution_epoch > 0
            && self.core.previous_governance_runtime_digest
                == self.core.adopting_governance_runtime_digest
        {
            return Err(AutonomyContractError::ConstitutionInvariant {
                rule: "a governance runtime cannot approve its own adoption",
            });
        }
        independence.validate_against_kernel(kernel)?;
        if independence.core.constitution_epoch != self.core.constitution_epoch {
            return Err(AutonomyContractError::EpochMismatch {
                field: "independence.constitution_epoch",
                expected: self.core.constitution_epoch,
                observed: independence.core.constitution_epoch,
            });
        }
        require_digest_equality(
            "independence_spec_digest",
            &independence.independence_spec_digest,
            &self.core.independence_spec_digest,
        )?;
        require_digest("constitution_digest", &self.constitution_digest)?;
        let computed = self.compute_digest()?;
        require_digest_equality("constitution_digest", &computed, &self.constitution_digest)?;
        require_signature("constitution.signature", &self.signature)?;
        Ok(AutonomyStructuralValidation::opaque(computed))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstitutionAmendmentCoreV1 {
    pub amendment_id: String,
    pub previous_constitution_digest: String,
    pub previous_constitution_epoch: u64,
    pub proposed_constitution_digest: String,
    pub proposed_constitution_epoch: u64,
    pub proposer_subject_id: String,
    pub proposed_runtime_subject_id: String,
    pub approval_issuer_subject_id: String,
    pub previous_runtime_digest: String,
    pub proposed_runtime_digest: String,
    pub old_runtime_approval_digest: String,
    pub authority_decision_digest: String,
    pub prepare_receipt_digest: String,
    pub canary_receipts_digest: String,
    pub rollback_plan_digest: String,
    pub prepared_at: u64,
    pub activates_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstitutionAmendmentV1 {
    pub schema: String,
    pub core: ConstitutionAmendmentCoreV1,
    pub amendment_digest: String,
    pub old_runtime_signature: OpaqueSignature,
}

impl ConstitutionAmendmentV1 {
    pub fn compute_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(CONSTITUTION_AMENDMENT_DIGEST_DOMAIN, &self.core)
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.amendment_digest = self.compute_digest()?;
        Ok(())
    }

    pub fn validate_transition(
        &self,
        previous: &ConstitutionStoreV1,
        proposed: &ConstitutionStoreV1,
        proposed_independence: &IndependenceSpecV1,
        kernel: &SafetyKernelV1,
        now_ms: u64,
    ) -> Result<AutonomyStructuralValidation, AutonomyContractError> {
        require_schema(
            "ConstitutionAmendmentV1",
            &self.schema,
            CONSTITUTION_AMENDMENT_SCHEMA,
        )?;
        require_non_empty("amendment_id", &self.core.amendment_id)?;
        require_non_empty("proposer_subject_id", &self.core.proposer_subject_id)?;
        require_non_empty(
            "proposed_runtime_subject_id",
            &self.core.proposed_runtime_subject_id,
        )?;
        require_non_empty(
            "approval_issuer_subject_id",
            &self.core.approval_issuer_subject_id,
        )?;
        if self.core.proposed_runtime_subject_id == self.core.approval_issuer_subject_id {
            return Err(AutonomyContractError::SelfAuthorization {
                subject_id: self.core.proposed_runtime_subject_id.clone(),
            });
        }
        if self.core.prepared_at >= self.core.activates_at || now_ms < self.core.activates_at {
            return Err(AutonomyContractError::ConstitutionInvariant {
                rule: "amendment requires a completed delay before activation",
            });
        }
        for (field, digest) in [
            (
                "previous_constitution_digest",
                &self.core.previous_constitution_digest,
            ),
            (
                "proposed_constitution_digest",
                &self.core.proposed_constitution_digest,
            ),
            (
                "previous_runtime_digest",
                &self.core.previous_runtime_digest,
            ),
            (
                "proposed_runtime_digest",
                &self.core.proposed_runtime_digest,
            ),
            (
                "old_runtime_approval_digest",
                &self.core.old_runtime_approval_digest,
            ),
            (
                "authority_decision_digest",
                &self.core.authority_decision_digest,
            ),
            ("prepare_receipt_digest", &self.core.prepare_receipt_digest),
            ("canary_receipts_digest", &self.core.canary_receipts_digest),
            ("rollback_plan_digest", &self.core.rollback_plan_digest),
        ] {
            require_digest(field, digest)?;
        }
        require_digest_equality(
            "previous_constitution_digest",
            &previous.constitution_digest,
            &self.core.previous_constitution_digest,
        )?;
        require_digest_equality(
            "proposed_constitution_digest",
            &proposed.constitution_digest,
            &self.core.proposed_constitution_digest,
        )?;
        if self.core.previous_constitution_epoch != previous.core.constitution_epoch
            || self.core.proposed_constitution_epoch != proposed.core.constitution_epoch
            || proposed.core.constitution_epoch != previous.core.constitution_epoch + 1
        {
            return Err(AutonomyContractError::ConstitutionInvariant {
                rule: "amendment epochs must form one monotonic previous-to-proposed step",
            });
        }
        if proposed.core.previous_constitution_digest.as_deref()
            != Some(previous.constitution_digest.as_str())
            || proposed.core.old_runtime_approval_digest.as_deref()
                != Some(self.core.old_runtime_approval_digest.as_str())
            || proposed.core.previous_governance_runtime_digest != self.core.previous_runtime_digest
            || proposed.core.adopting_governance_runtime_digest != self.core.proposed_runtime_digest
            || proposed.core.effective_at != self.core.activates_at
        {
            return Err(AutonomyContractError::ConstitutionInvariant {
                rule: "proposed constitution does not bind the exact amendment/old-runtime facts",
            });
        }
        proposed.validate(proposed_independence, kernel, now_ms)?;
        require_digest("amendment_digest", &self.amendment_digest)?;
        let computed = self.compute_digest()?;
        require_digest_equality("amendment_digest", &computed, &self.amendment_digest)?;
        require_signature("old_runtime_signature", &self.old_runtime_signature)?;
        Ok(AutonomyStructuralValidation::opaque(computed))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GrantStatus {
    Active,
    Revoked,
    Demoted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetEnvelopeV1 {
    pub unit: String,
    pub limit: u64,
    pub consumed: u64,
    pub reset_epoch: u64,
}

impl BudgetEnvelopeV1 {
    pub fn remaining(&self) -> Result<u64, AutonomyContractError> {
        require_non_empty("budget.unit", &self.unit)?;
        self.limit
            .checked_sub(self.consumed)
            .ok_or(AutonomyContractError::Invariant {
                rule: "grant budget consumption exceeds its limit",
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutonomyGrantCoreV1 {
    pub grant_id: String,
    pub subject_id: String,
    pub role_id: String,
    pub mode: ActiveMode,
    pub max_tier: AutonomyTier,
    pub action_classes: BTreeSet<String>,
    pub risk_domains: BTreeSet<RiskClass>,
    pub resource_environment_scope_digest: String,
    pub budget: BudgetEnvelopeV1,
    pub constitution_epoch: u64,
    pub autonomy_epoch: u64,
    pub issued_at: u64,
    pub expires_at: u64,
    pub promotion_receipt_id: String,
    pub status: GrantStatus,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutonomyGrantV1 {
    pub schema: String,
    pub core: AutonomyGrantCoreV1,
    pub grant_digest: String,
    pub owner_signature: OpaqueSignature,
}

impl AutonomyGrantV1 {
    pub fn compute_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(AUTONOMY_GRANT_DIGEST_DOMAIN, &self.core)
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.grant_digest = self.compute_digest()?;
        Ok(())
    }

    pub fn validate_at(&self, now_ms: u64) -> Result<(), AutonomyContractError> {
        require_schema("AutonomyGrantV1", &self.schema, AUTONOMY_GRANT_SCHEMA)?;
        for (field, value) in [
            ("grant_id", &self.core.grant_id),
            ("subject_id", &self.core.subject_id),
            ("role_id", &self.core.role_id),
            ("promotion_receipt_id", &self.core.promotion_receipt_id),
        ] {
            require_non_empty(field, value)?;
        }
        if self.core.action_classes.is_empty() {
            return Err(AutonomyContractError::EmptyCollection {
                field: "action_classes",
            });
        }
        if self.core.risk_domains.is_empty() {
            return Err(AutonomyContractError::EmptyCollection {
                field: "risk_domains",
            });
        }
        require_digest(
            "resource_environment_scope_digest",
            &self.core.resource_environment_scope_digest,
        )?;
        self.core.budget.remaining()?;
        require_time_window(
            "AutonomyGrantV1",
            self.core.issued_at,
            self.core.expires_at,
            now_ms,
        )?;
        if self.core.status != GrantStatus::Active {
            return Err(AutonomyContractError::Invariant {
                rule: "only ACTIVE autonomy grants authorize positive work",
            });
        }
        if self.core.mode == ActiveMode::HumanGated && self.core.max_tier > AutonomyTier::A1Propose
        {
            return Err(AutonomyContractError::Invariant {
                rule: "HUMAN_GATED grants cannot exceed A1_PROPOSE",
            });
        }
        require_digest("grant_digest", &self.grant_digest)?;
        let computed = self.compute_digest()?;
        require_digest_equality("grant_digest", &computed, &self.grant_digest)?;
        require_signature("grant.owner_signature", &self.owner_signature)
    }

    pub fn authorize_scope(
        &self,
        action_class: &str,
        risk_class: RiskClass,
        resource_environment_scope_digest: &str,
        requested_budget: u64,
        effective_tier: AutonomyTier,
    ) -> Result<(), AutonomyContractError> {
        if !self.core.action_classes.contains(action_class) {
            return Err(AutonomyContractError::ActionOutsideGrant {
                action_class: action_class.to_string(),
            });
        }
        if !self.core.risk_domains.contains(&risk_class) {
            return Err(AutonomyContractError::RiskOutsideGrant { risk_class });
        }
        if self.core.resource_environment_scope_digest != resource_environment_scope_digest {
            return Err(AutonomyContractError::ScopeOutsideGrant);
        }
        let remaining = self.core.budget.remaining()?;
        if requested_budget > remaining {
            return Err(AutonomyContractError::BudgetExceeded {
                requested: requested_budget,
                remaining,
            });
        }
        if effective_tier > self.core.max_tier {
            return Err(AutonomyContractError::Invariant {
                rule: "effective tier exceeds the scoped grant tier",
            });
        }
        Ok(())
    }
}

pub fn compute_grants_digest(grants: &[AutonomyGrantV1]) -> Result<String, AutonomyContractError> {
    let mut by_id = BTreeMap::new();
    for grant in grants {
        require_digest("grant_digest", &grant.grant_digest)?;
        if by_id
            .insert(grant.core.grant_id.as_str(), grant.grant_digest.as_str())
            .is_some()
        {
            return Err(AutonomyContractError::Invariant {
                rule: "autonomy grant ids must be unique",
            });
        }
    }
    Ok(digest_canonical(AUTONOMY_GRANTS_DIGEST_DOMAIN, &by_id)?)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SafetyState {
    Healthy,
    Frozen,
    PendingRed,
    Recovering,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutonomyEpochV1 {
    pub schema: String,
    pub autonomy_epoch: u64,
    pub active_mode: ActiveMode,
    pub activation_receipt_id: Option<String>,
    pub constitution_digest: String,
    pub constitution_epoch: u64,
    pub grants_digest: String,
    pub issuance_frozen: bool,
    pub safety_state: SafetyState,
    pub protected_root_signature: OpaqueSignature,
}

impl AutonomyEpochV1 {
    pub fn validate_bootstrap(
        &self,
        constitution: &ConstitutionStoreV1,
        grants: &[AutonomyGrantV1],
        now_ms: u64,
    ) -> Result<(), AutonomyContractError> {
        self.validate_common(constitution, grants, now_ms)?;
        if self.autonomy_epoch != 0
            || self.active_mode != ActiveMode::HumanGated
            || self.activation_receipt_id.is_some()
        {
            return Err(AutonomyContractError::Invariant {
                rule:
                    "authoritative bootstrap is epoch zero HUMAN_GATED with no activation receipt",
            });
        }
        Ok(())
    }

    pub fn validate_common(
        &self,
        constitution: &ConstitutionStoreV1,
        grants: &[AutonomyGrantV1],
        now_ms: u64,
    ) -> Result<(), AutonomyContractError> {
        require_schema("AutonomyEpochV1", &self.schema, AUTONOMY_EPOCH_SCHEMA)?;
        require_digest("constitution_digest", &self.constitution_digest)?;
        require_digest("grants_digest", &self.grants_digest)?;
        require_digest_equality(
            "constitution_digest",
            &constitution.constitution_digest,
            &self.constitution_digest,
        )?;
        if self.constitution_epoch != constitution.core.constitution_epoch {
            return Err(AutonomyContractError::EpochMismatch {
                field: "constitution_epoch",
                expected: constitution.core.constitution_epoch,
                observed: self.constitution_epoch,
            });
        }
        if !constitution
            .core
            .allowed_autonomy_modes
            .contains(&self.active_mode)
        {
            return Err(AutonomyContractError::ConstitutionInvariant {
                rule: "active mode is not permitted by the active constitution",
            });
        }
        let grants_digest = compute_grants_digest(grants)?;
        require_digest_equality("grants_digest", &grants_digest, &self.grants_digest)?;
        if self.active_mode != ActiveMode::HumanGated && grants.is_empty() {
            return Err(AutonomyContractError::EmptyCollection {
                field: "autonomous_mode_grants",
            });
        }
        for grant in grants {
            grant.validate_at(now_ms)?;
            if grant.core.mode != self.active_mode {
                return Err(AutonomyContractError::ModeMismatch {
                    field: "grant.mode",
                    expected: self.active_mode,
                    observed: grant.core.mode,
                });
            }
            if grant.core.constitution_epoch != self.constitution_epoch {
                return Err(AutonomyContractError::EpochMismatch {
                    field: "grant.constitution_epoch",
                    expected: self.constitution_epoch,
                    observed: grant.core.constitution_epoch,
                });
            }
            if grant.core.autonomy_epoch != self.autonomy_epoch {
                return Err(AutonomyContractError::EpochMismatch {
                    field: "grant.autonomy_epoch",
                    expected: self.autonomy_epoch,
                    observed: grant.core.autonomy_epoch,
                });
            }
        }
        match self.safety_state {
            SafetyState::Healthy if self.issuance_frozen => {
                return Err(AutonomyContractError::Invariant {
                    rule: "HEALTHY epoch cannot have issuance frozen",
                });
            }
            SafetyState::Frozen | SafetyState::PendingRed | SafetyState::Recovering
                if !self.issuance_frozen =>
            {
                return Err(AutonomyContractError::Invariant {
                    rule: "non-HEALTHY epoch must freeze issuance",
                });
            }
            _ => {}
        }
        if self.autonomy_epoch > 0 {
            require_non_empty_option("activation_receipt_id", &self.activation_receipt_id)?;
        }
        require_signature("protected_root_signature", &self.protected_root_signature)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentCoreRefV1 {
    pub intent_digest: String,
    pub canonicalization_version: String,
    pub content_address: String,
}

impl IntentCoreRefV1 {
    pub fn for_sovereign_digest(intent_digest: impl Into<String>) -> Self {
        let intent_digest = intent_digest.into();
        Self {
            content_address: format!("intent:sha256:{intent_digest}"),
            canonicalization_version: CANONICALIZATION_VERSION.to_string(),
            intent_digest,
        }
    }

    pub fn for_safety_digest(intent_digest: impl Into<String>) -> Self {
        let intent_digest = intent_digest.into();
        Self {
            content_address: format!("intent:safety:sha256:{intent_digest}"),
            canonicalization_version: CANONICALIZATION_VERSION.to_string(),
            intent_digest,
        }
    }

    pub fn validate(&self, safety: bool) -> Result<(), AutonomyContractError> {
        require_digest("intent_ref.intent_digest", &self.intent_digest)?;
        if self.canonicalization_version != CANONICALIZATION_VERSION {
            return Err(AutonomyContractError::Invariant {
                rule: "intent ref canonicalization version is not runtime-pinned",
            });
        }
        let expected = if safety {
            format!("intent:safety:sha256:{}", self.intent_digest)
        } else {
            format!("intent:sha256:{}", self.intent_digest)
        };
        if self.content_address != expected {
            return Err(AutonomyContractError::BindingMismatch {
                field: "intent_ref.content_address",
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SovereignIntentCoreV1 {
    pub action_class: String,
    /// Exact G2 semantic action id. Grants remain scoped by `action_class`,
    /// while this dotted id prevents one member of a class from authorizing a
    /// different catalog action.
    pub semantic_action_id: String,
    pub action_payload_digest: String,
    pub issuer_subject_id: String,
    pub decision_subject_id: String,
    pub caller_subject_id: String,
    pub audience: String,
    pub proposer_subject_id: String,
    pub executor_subject_id: Option<String>,
    pub promotion_target_subject_id: Option<String>,
    pub ratification_target_subject_id: Option<String>,
    pub delegation_grant_digest: Option<String>,
    pub required_authority_variant: AuthorityVariant,
    pub action_policy_registry_digest: String,
    pub classifier_decision_digest: String,
    pub applicable_grant_id: Option<String>,
    pub applicable_grant_digest: Option<String>,
    pub organism_id: String,
    pub repo_id: String,
    pub brain_id: String,
    pub mission_id: Option<String>,
    pub mission_head_id: Option<String>,
    pub block_id: Option<String>,
    pub candidate_digest: Option<String>,
    pub promotion_subject_id: Option<String>,
    pub active_mode: ActiveMode,
    pub effective_tier: AutonomyTier,
    pub risk_class: RiskClass,
    pub risk_scope_digest: String,
    pub resource_environment_scope_digest: String,
    pub requested_budget: u64,
    pub constitution_digest: String,
    pub constitution_epoch: u64,
    pub autonomy_epoch: u64,
    pub expected_store_epoch: u64,
    pub expected_store_version: u64,
    pub expected_boundary_version: u64,
    pub expected_contract_version: u64,
    pub metric_spec_digest: String,
    pub evidence_digest: String,
    pub rollout_plan_digest: String,
    pub rollback_plan_digest: String,
    pub nonce: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SovereignActionIntentV1 {
    pub schema: String,
    pub core: SovereignIntentCoreV1,
    pub intent_digest: String,
    pub intent_core_ref: IntentCoreRefV1,
}

impl SovereignActionIntentV1 {
    pub fn compute_intent_digest(&self) -> Result<String, CanonicalError> {
        digest_versioned_core(
            SOVEREIGN_INTENT_DIGEST_DOMAIN,
            CANONICALIZATION_VERSION,
            &self.core,
        )
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.intent_digest = self.compute_intent_digest()?;
        self.intent_core_ref = IntentCoreRefV1::for_sovereign_digest(self.intent_digest.clone());
        Ok(())
    }

    pub fn validate_canonical_core(
        &self,
        now_ms: u64,
    ) -> Result<AutonomyStructuralValidation, AutonomyContractError> {
        require_schema(
            "SovereignActionIntentV1",
            &self.schema,
            SOVEREIGN_ACTION_INTENT_SCHEMA,
        )?;
        require_time_window(
            "SovereignActionIntentV1",
            self.core.issued_at,
            self.core.expires_at,
            now_ms,
        )?;
        self.validate_no_self_authorization()?;
        if !ActionId::new(&self.core.semantic_action_id)
            .is_ok_and(|action| action.is_semantic_catalog_id())
        {
            return Err(AutonomyContractError::Invariant {
                rule: "sovereign intent semantic_action_id is not a canonical G2 action id",
            });
        }
        require_digest("intent_digest", &self.intent_digest)?;
        let computed = self.compute_intent_digest()?;
        require_digest_equality("intent_digest", &computed, &self.intent_digest)?;
        self.intent_core_ref.validate(false)?;
        require_digest_equality(
            "intent_core_ref.intent_digest",
            &self.intent_digest,
            &self.intent_core_ref.intent_digest,
        )?;
        Ok(AutonomyStructuralValidation::opaque(computed))
    }

    pub fn validate(
        &self,
        epoch: &AutonomyEpochV1,
        grant: Option<&AutonomyGrantV1>,
        now_ms: u64,
    ) -> Result<AutonomyStructuralValidation, AutonomyContractError> {
        require_schema(
            "SovereignActionIntentV1",
            &self.schema,
            SOVEREIGN_ACTION_INTENT_SCHEMA,
        )?;
        self.validate_canonical_core(now_ms)?;
        for (field, value) in [
            ("action_class", &self.core.action_class),
            ("semantic_action_id", &self.core.semantic_action_id),
            ("issuer_subject_id", &self.core.issuer_subject_id),
            ("decision_subject_id", &self.core.decision_subject_id),
            ("caller_subject_id", &self.core.caller_subject_id),
            ("audience", &self.core.audience),
            ("proposer_subject_id", &self.core.proposer_subject_id),
            ("organism_id", &self.core.organism_id),
            ("repo_id", &self.core.repo_id),
            ("brain_id", &self.core.brain_id),
            ("nonce", &self.core.nonce),
        ] {
            require_non_empty(field, value)?;
        }
        validate_optional_non_empty("executor_subject_id", &self.core.executor_subject_id)?;
        validate_optional_non_empty(
            "promotion_target_subject_id",
            &self.core.promotion_target_subject_id,
        )?;
        validate_optional_non_empty(
            "ratification_target_subject_id",
            &self.core.ratification_target_subject_id,
        )?;
        validate_optional_non_empty("mission_id", &self.core.mission_id)?;
        validate_optional_non_empty("mission_head_id", &self.core.mission_head_id)?;
        validate_optional_non_empty("block_id", &self.core.block_id)?;
        validate_optional_non_empty("promotion_subject_id", &self.core.promotion_subject_id)?;
        validate_optional_non_empty("applicable_grant_id", &self.core.applicable_grant_id)?;
        for (field, digest) in [
            ("action_payload_digest", &self.core.action_payload_digest),
            (
                "action_policy_registry_digest",
                &self.core.action_policy_registry_digest,
            ),
            (
                "classifier_decision_digest",
                &self.core.classifier_decision_digest,
            ),
            ("risk_scope_digest", &self.core.risk_scope_digest),
            (
                "resource_environment_scope_digest",
                &self.core.resource_environment_scope_digest,
            ),
            ("constitution_digest", &self.core.constitution_digest),
            ("metric_spec_digest", &self.core.metric_spec_digest),
            ("evidence_digest", &self.core.evidence_digest),
            ("rollout_plan_digest", &self.core.rollout_plan_digest),
            ("rollback_plan_digest", &self.core.rollback_plan_digest),
        ] {
            require_digest(field, digest)?;
        }
        for (field, digest) in [
            (
                "delegation_grant_digest",
                &self.core.delegation_grant_digest,
            ),
            ("candidate_digest", &self.core.candidate_digest),
            (
                "applicable_grant_digest",
                &self.core.applicable_grant_digest,
            ),
        ] {
            if let Some(digest) = digest {
                require_digest(field, digest)?;
            }
        }
        require_time_window(
            "SovereignActionIntentV1",
            self.core.issued_at,
            self.core.expires_at,
            now_ms,
        )?;
        if !matches!(
            self.core.required_authority_variant,
            AuthorityVariant::Human | AuthorityVariant::Policy | AuthorityVariant::AgentQuorum
        ) {
            return Err(AutonomyContractError::Invariant {
                rule: "sovereign positive intent requires HUMAN, POLICY, or AGENT_QUORUM authority",
            });
        }
        if self.core.active_mode != epoch.active_mode {
            return Err(AutonomyContractError::ModeMismatch {
                field: "intent.active_mode",
                expected: epoch.active_mode,
                observed: self.core.active_mode,
            });
        }
        if self.core.constitution_epoch != epoch.constitution_epoch {
            return Err(AutonomyContractError::EpochMismatch {
                field: "intent.constitution_epoch",
                expected: epoch.constitution_epoch,
                observed: self.core.constitution_epoch,
            });
        }
        if self.core.autonomy_epoch != epoch.autonomy_epoch {
            return Err(AutonomyContractError::EpochMismatch {
                field: "intent.autonomy_epoch",
                expected: epoch.autonomy_epoch,
                observed: self.core.autonomy_epoch,
            });
        }
        require_digest_equality(
            "intent.constitution_digest",
            &epoch.constitution_digest,
            &self.core.constitution_digest,
        )?;
        if epoch.issuance_frozen || epoch.safety_state != SafetyState::Healthy {
            return Err(AutonomyContractError::Invariant {
                rule:
                    "positive intent is forbidden while issuance is frozen or safety is non-HEALTHY",
            });
        }
        if self.core.caller_subject_id != self.core.decision_subject_id
            && self.core.delegation_grant_digest.is_none()
        {
            return Err(AutonomyContractError::Invariant {
                rule: "caller/decision subject divergence requires an explicit delegation grant",
            });
        }
        self.validate_no_self_authorization()?;

        match self.core.required_authority_variant {
            AuthorityVariant::Human => {
                if grant.is_some()
                    || self.core.applicable_grant_id.is_some()
                    || self.core.applicable_grant_digest.is_some()
                {
                    return Err(AutonomyContractError::Invariant {
                        rule: "HUMAN authority cannot be smuggled through an autonomy grant",
                    });
                }
            }
            AuthorityVariant::Policy | AuthorityVariant::AgentQuorum => {
                if self.core.applicable_grant_id.is_none()
                    || self.core.applicable_grant_digest.is_none()
                {
                    return Err(AutonomyContractError::Invariant {
                        rule: "autonomous positive intent requires grant id and digest",
                    });
                }
                let grant = grant.ok_or(AutonomyContractError::Invariant {
                    rule: "autonomous positive intent requires an exact scoped grant",
                })?;
                grant.validate_at(now_ms)?;
                self.validate_grant_binding(grant)?;
            }
            _ => unreachable!("non-positive variants were rejected above"),
        }

        require_digest("intent_digest", &self.intent_digest)?;
        let computed = self.compute_intent_digest()?;
        require_digest_equality("intent_digest", &computed, &self.intent_digest)?;
        self.intent_core_ref.validate(false)?;
        require_digest_equality(
            "intent_core_ref.intent_digest",
            &self.intent_digest,
            &self.intent_core_ref.intent_digest,
        )?;
        Ok(AutonomyStructuralValidation::opaque(computed))
    }

    fn validate_no_self_authorization(&self) -> Result<(), AutonomyContractError> {
        let authorities = [
            self.core.issuer_subject_id.as_str(),
            self.core.decision_subject_id.as_str(),
            self.core.proposer_subject_id.as_str(),
        ];
        for target in [
            self.core.promotion_target_subject_id.as_deref(),
            self.core.ratification_target_subject_id.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if authorities.contains(&target)
                || self.core.executor_subject_id.as_deref() == Some(target)
            {
                return Err(AutonomyContractError::SelfAuthorization {
                    subject_id: target.to_string(),
                });
            }
        }
        if self.core.action_class == "ratify" {
            if let (Some(executor), Some(target)) = (
                self.core.executor_subject_id.as_deref(),
                self.core.ratification_target_subject_id.as_deref(),
            ) {
                if executor == target {
                    return Err(AutonomyContractError::SelfAuthorization {
                        subject_id: executor.to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_grant_binding(&self, grant: &AutonomyGrantV1) -> Result<(), AutonomyContractError> {
        if self.core.applicable_grant_id.as_deref() != Some(grant.core.grant_id.as_str())
            || self.core.applicable_grant_digest.as_deref() != Some(grant.grant_digest.as_str())
        {
            return Err(AutonomyContractError::BindingMismatch {
                field: "applicable_grant",
            });
        }
        if grant.core.subject_id != self.core.decision_subject_id {
            return Err(AutonomyContractError::BindingMismatch {
                field: "grant.subject_id",
            });
        }
        if grant.core.mode != self.core.active_mode {
            return Err(AutonomyContractError::ModeMismatch {
                field: "grant.mode",
                expected: self.core.active_mode,
                observed: grant.core.mode,
            });
        }
        if grant.core.constitution_epoch != self.core.constitution_epoch
            || grant.core.autonomy_epoch != self.core.autonomy_epoch
        {
            return Err(AutonomyContractError::BindingMismatch {
                field: "grant.epochs",
            });
        }
        grant.authorize_scope(
            &self.core.action_class,
            self.core.risk_class,
            &self.core.resource_environment_scope_digest,
            self.core.requested_budget,
            self.core.effective_tier,
        )
    }

    /// Fail-closed generic JSON admission used by durable intent stores.
    /// `deny_unknown_fields` already prevents these keys in typed decoding;
    /// this explicit guard makes the acyclic contract independently testable.
    pub fn reject_cyclic_json_fields(
        value: &serde_json::Value,
    ) -> Result<(), AutonomyContractError> {
        const FORBIDDEN: &[&str] = &[
            "intent_digest",
            "intent_core_ref",
            "sentinel_verdict_digest",
            "verdict_digest",
            "votes",
            "decision_digest",
            "authority_decision",
            "capability_id",
            "capability_digest",
            "transaction_id",
            "transaction_digest",
        ];
        let object = value.as_object().ok_or(AutonomyContractError::Invariant {
            rule: "intent core must be a JSON object",
        })?;
        for field in FORBIDDEN {
            if object.contains_key(*field) {
                return Err(AutonomyContractError::CyclicIntentField {
                    field: (*field).to_string(),
                });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SentinelVerdict {
    Green,
    Red,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SentinelVerdictCoreV1 {
    pub verdict_id: String,
    pub sentinel_identity_key_binary_policy_digest: String,
    pub intent_digest: String,
    pub intent_core_ref: IntentCoreRefV1,
    pub intent_canonicalization_version: String,
    pub metric_evidence_rollback_digest: String,
    pub risk_scope_digest: String,
    pub constitution_epoch: u64,
    pub autonomy_epoch: u64,
    pub nonce: String,
    pub issued_at: u64,
    pub expires_at: u64,
    pub verdict: SentinelVerdict,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SentinelVerdictV1 {
    pub schema: String,
    pub core: SentinelVerdictCoreV1,
    pub verdict_digest: String,
    pub signature: OpaqueSignature,
}

impl SentinelVerdictV1 {
    pub fn compute_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(SENTINEL_VERDICT_DIGEST_DOMAIN, &self.core)
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.verdict_digest = self.compute_digest()?;
        Ok(())
    }

    pub fn validate_for_intent(
        &self,
        intent: &SovereignActionIntentV1,
        kernel: &SafetyKernelV1,
        now_ms: u64,
    ) -> Result<AutonomyStructuralValidation, AutonomyContractError> {
        intent.validate_canonical_core(now_ms)?;
        require_schema("SentinelVerdictV1", &self.schema, SENTINEL_VERDICT_SCHEMA)?;
        require_non_empty("verdict_id", &self.core.verdict_id)?;
        require_non_empty("sentinel.nonce", &self.core.nonce)?;
        require_time_window(
            "SentinelVerdictV1",
            self.core.issued_at,
            self.core.expires_at,
            now_ms,
        )?;
        for (field, digest) in [
            (
                "sentinel_identity_key_binary_policy_digest",
                &self.core.sentinel_identity_key_binary_policy_digest,
            ),
            ("sentinel.intent_digest", &self.core.intent_digest),
            (
                "metric_evidence_rollback_digest",
                &self.core.metric_evidence_rollback_digest,
            ),
            ("sentinel.risk_scope_digest", &self.core.risk_scope_digest),
        ] {
            require_digest(field, digest)?;
        }
        require_digest_equality(
            "sentinel_identity_key_binary_policy_digest",
            &kernel.core.sentinel_identity_key_binary_policy_digest,
            &self.core.sentinel_identity_key_binary_policy_digest,
        )?;
        require_digest_equality(
            "sentinel.intent_digest",
            &intent.intent_digest,
            &self.core.intent_digest,
        )?;
        require_digest_equality(
            "sentinel.risk_scope_digest",
            &intent.core.risk_scope_digest,
            &self.core.risk_scope_digest,
        )?;
        if self.core.intent_canonicalization_version != CANONICALIZATION_VERSION {
            return Err(AutonomyContractError::BindingMismatch {
                field: "sentinel.intent_canonicalization_version",
            });
        }
        if self.core.intent_core_ref != intent.intent_core_ref {
            return Err(AutonomyContractError::BindingMismatch {
                field: "sentinel.intent_core_ref",
            });
        }
        if self.core.constitution_epoch != intent.core.constitution_epoch
            || self.core.autonomy_epoch != intent.core.autonomy_epoch
            || self.core.nonce != intent.core.nonce
        {
            return Err(AutonomyContractError::BindingMismatch {
                field: "sentinel.epoch_or_nonce",
            });
        }
        require_digest("verdict_digest", &self.verdict_digest)?;
        let computed = self.compute_digest()?;
        require_digest_equality("verdict_digest", &computed, &self.verdict_digest)?;
        require_signature("sentinel.signature", &self.signature)?;
        Ok(AutonomyStructuralValidation::opaque(computed))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RedOutboxState {
    Pending,
    LatchAcknowledged,
    ActuatorAcknowledged,
    Terminal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SentinelRedOutboxCoreV1 {
    pub red_verdict_digest: String,
    pub source_intent_digest: String,
    pub outbox_epoch: u64,
    pub previous_outbox_root_digest: Option<String>,
    pub signed_outbox_root_digest: String,
    pub protected_latest_outbox_epoch: u64,
    pub delivery_attempt: u64,
    pub journal_latch_ack: bool,
    pub actuator_ack: bool,
    pub terminal_safety_transaction_id: Option<String>,
    pub state: RedOutboxState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SentinelRedOutboxV1 {
    pub schema: String,
    pub core: SentinelRedOutboxCoreV1,
    pub record_digest: String,
    pub root_signature: OpaqueSignature,
}

impl SentinelRedOutboxV1 {
    pub fn compute_record_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(SENTINEL_RED_OUTBOX_RECORD_DIGEST_DOMAIN, &self.core)
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.record_digest = self.compute_record_digest()?;
        Ok(())
    }

    pub fn validate(
        &self,
        verdict: &SentinelVerdictV1,
        intent: &SovereignActionIntentV1,
        expected_previous: Option<&SentinelRedOutboxV1>,
    ) -> Result<AutonomyStructuralValidation, AutonomyContractError> {
        require_schema(
            "SentinelRedOutboxV1",
            &self.schema,
            SENTINEL_RED_OUTBOX_SCHEMA,
        )?;
        if verdict.core.verdict != SentinelVerdict::Red {
            return Err(AutonomyContractError::Invariant {
                rule: "only RED sentinel verdicts enter the durable RED outbox",
            });
        }
        require_digest_equality(
            "outbox.red_verdict_digest",
            &verdict.verdict_digest,
            &self.core.red_verdict_digest,
        )?;
        require_digest_equality(
            "outbox.source_intent_digest",
            &intent.intent_digest,
            &self.core.source_intent_digest,
        )?;
        require_digest(
            "signed_outbox_root_digest",
            &self.core.signed_outbox_root_digest,
        )?;
        if self.core.outbox_epoch == 0
            || self.core.protected_latest_outbox_epoch < self.core.outbox_epoch
            || self.core.delivery_attempt == 0
        {
            return Err(AutonomyContractError::Invariant {
                rule: "RED outbox epochs must be positive and protected against rollback",
            });
        }
        match expected_previous {
            None => {
                if self.core.outbox_epoch != 1 || self.core.previous_outbox_root_digest.is_some() {
                    return Err(AutonomyContractError::Invariant {
                        rule: "first RED outbox record is epoch one with no previous root",
                    });
                }
            }
            Some(previous) => {
                if self.core.outbox_epoch != previous.core.outbox_epoch + 1
                    || self.core.previous_outbox_root_digest.as_deref()
                        != Some(previous.core.signed_outbox_root_digest.as_str())
                {
                    return Err(AutonomyContractError::Invariant {
                        rule: "RED outbox records must form a gapless signed root chain",
                    });
                }
            }
        }
        validate_red_outbox_state(&self.core)?;
        require_digest("outbox.record_digest", &self.record_digest)?;
        let computed = self.compute_record_digest()?;
        require_digest_equality("outbox.record_digest", &computed, &self.record_digest)?;
        require_signature("outbox.root_signature", &self.root_signature)?;
        Ok(AutonomyStructuralValidation::opaque(computed))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RedLatchState {
    Pending,
    Committing,
    Terminal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedLatchCoreV1 {
    pub latch_receipt_id: String,
    pub red_verdict_digest: String,
    pub source_intent_digest: String,
    pub sentinel_outbox_epoch: u64,
    pub sentinel_outbox_root_digest: String,
    pub latched_at: u64,
    pub protected_time_evidence_digest: String,
    pub constitution_epoch: u64,
    pub autonomy_epoch: u64,
    pub latch_epoch: u64,
    pub exact_affected_scope_digest: String,
    pub allowed_negative_actions_digest: String,
    pub rollback_candidate_plan_digest: String,
    pub immutable_negative_mandate_digest: String,
    pub committing_transaction_id: Option<String>,
    pub commit_marker_digest: Option<String>,
    pub terminal_safety_transaction_id: Option<String>,
    pub state: RedLatchState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RedLatchReceiptV1 {
    pub schema: String,
    pub core: RedLatchCoreV1,
    pub latch_receipt_digest: String,
    pub owner_kernel_signature: OpaqueSignature,
}

impl RedLatchReceiptV1 {
    pub fn compute_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(RED_LATCH_RECEIPT_DIGEST_DOMAIN, &self.core)
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.latch_receipt_digest = self.compute_digest()?;
        Ok(())
    }

    pub fn validate(
        &self,
        outbox: &SentinelRedOutboxV1,
        verdict: &SentinelVerdictV1,
        intent: &SovereignActionIntentV1,
        kernel: &SafetyKernelV1,
    ) -> Result<AutonomyStructuralValidation, AutonomyContractError> {
        require_schema("RedLatchReceiptV1", &self.schema, RED_LATCH_RECEIPT_SCHEMA)?;
        require_non_empty("latch_receipt_id", &self.core.latch_receipt_id)?;
        if verdict.core.verdict != SentinelVerdict::Red {
            return Err(AutonomyContractError::Invariant {
                rule: "a RED latch requires a RED sentinel verdict",
            });
        }
        for (field, expected, observed) in [
            (
                "latch.red_verdict_digest",
                verdict.verdict_digest.as_str(),
                self.core.red_verdict_digest.as_str(),
            ),
            (
                "latch.source_intent_digest",
                intent.intent_digest.as_str(),
                self.core.source_intent_digest.as_str(),
            ),
            (
                "latch.sentinel_outbox_root_digest",
                outbox.core.signed_outbox_root_digest.as_str(),
                self.core.sentinel_outbox_root_digest.as_str(),
            ),
        ] {
            require_digest_equality(field, expected, observed)?;
        }
        if self.core.sentinel_outbox_epoch != outbox.core.outbox_epoch
            || self.core.constitution_epoch != intent.core.constitution_epoch
            || self.core.autonomy_epoch != intent.core.autonomy_epoch
            || self.core.latch_epoch == 0
        {
            return Err(AutonomyContractError::BindingMismatch {
                field: "latch.epochs",
            });
        }
        for (field, digest) in [
            (
                "protected_time_evidence_digest",
                &self.core.protected_time_evidence_digest,
            ),
            (
                "exact_affected_scope_digest",
                &self.core.exact_affected_scope_digest,
            ),
            (
                "allowed_negative_actions_digest",
                &self.core.allowed_negative_actions_digest,
            ),
            (
                "rollback_candidate_plan_digest",
                &self.core.rollback_candidate_plan_digest,
            ),
            (
                "immutable_negative_mandate_digest",
                &self.core.immutable_negative_mandate_digest,
            ),
        ] {
            require_digest(field, digest)?;
        }
        let expected_effects_digest =
            compute_safety_effects_digest(&kernel.core.allowed_negative_effects)?;
        require_digest_equality(
            "allowed_negative_actions_digest",
            &expected_effects_digest,
            &self.core.allowed_negative_actions_digest,
        )?;
        validate_red_latch_state(&self.core)?;
        require_digest("latch_receipt_digest", &self.latch_receipt_digest)?;
        let computed = self.compute_digest()?;
        require_digest_equality(
            "latch_receipt_digest",
            &computed,
            &self.latch_receipt_digest,
        )?;
        require_signature("latch.owner_kernel_signature", &self.owner_kernel_signature)?;
        Ok(AutonomyStructuralValidation::opaque(computed))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyActionIntentCoreV1 {
    pub safety_attempt_id: String,
    pub attempt_sequence: u64,
    pub rebased_from_attempt_digest: Option<String>,
    pub source_intent_digest: String,
    pub source_intent_core_ref: IntentCoreRefV1,
    pub sentinel_red_verdict_digest: String,
    pub red_latch_receipt_digest: String,
    pub actuator_identity_key_binary_policy_digest: String,
    pub expected_constitution_epoch: u64,
    pub expected_autonomy_epoch: u64,
    pub affected_grants_scope_digest: String,
    pub negative_effects: BTreeSet<Effect>,
    pub allowed_negative_actions_digest: String,
    pub rollback_candidate_plan_digest: String,
    pub nonce: String,
    pub attempt_idempotency_key: String,
    pub issued_at: u64,
    pub valid_while_latch_pending: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyActionIntentV1 {
    pub schema: String,
    pub core: SafetyActionIntentCoreV1,
    pub safety_intent_digest: String,
    pub safety_intent_core_ref: IntentCoreRefV1,
}

impl SafetyActionIntentV1 {
    pub fn compute_digest(&self) -> Result<String, CanonicalError> {
        digest_versioned_core(
            SAFETY_ACTION_INTENT_DIGEST_DOMAIN,
            CANONICALIZATION_VERSION,
            &self.core,
        )
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.safety_intent_digest = self.compute_digest()?;
        self.safety_intent_core_ref =
            IntentCoreRefV1::for_safety_digest(self.safety_intent_digest.clone());
        Ok(())
    }

    pub fn validate(
        &self,
        source_intent: &SovereignActionIntentV1,
        red_verdict: &SentinelVerdictV1,
        latch: &RedLatchReceiptV1,
        kernel: &SafetyKernelV1,
        current_constitution_epoch: u64,
        current_autonomy_epoch: u64,
    ) -> Result<AutonomyStructuralValidation, AutonomyContractError> {
        require_schema(
            "SafetyActionIntentV1",
            &self.schema,
            SAFETY_ACTION_INTENT_SCHEMA,
        )?;
        for (field, value) in [
            ("safety_attempt_id", &self.core.safety_attempt_id),
            ("safety.nonce", &self.core.nonce),
            (
                "attempt_idempotency_key",
                &self.core.attempt_idempotency_key,
            ),
        ] {
            require_non_empty(field, value)?;
        }
        if self.core.attempt_sequence == 0 {
            return Err(AutonomyContractError::Invariant {
                rule: "safety attempt sequence starts at one",
            });
        }
        if self.core.attempt_sequence == 1 && self.core.rebased_from_attempt_digest.is_some() {
            return Err(AutonomyContractError::Invariant {
                rule: "first safety attempt cannot be rebased",
            });
        }
        if self.core.attempt_sequence > 1 && self.core.rebased_from_attempt_digest.is_none() {
            return Err(AutonomyContractError::Invariant {
                rule: "retried safety attempt must identify the prior attempt digest",
            });
        }
        if let Some(digest) = &self.core.rebased_from_attempt_digest {
            require_digest("rebased_from_attempt_digest", digest)?;
        }
        for (field, expected, observed) in [
            (
                "safety.source_intent_digest",
                source_intent.intent_digest.as_str(),
                self.core.source_intent_digest.as_str(),
            ),
            (
                "safety.sentinel_red_verdict_digest",
                red_verdict.verdict_digest.as_str(),
                self.core.sentinel_red_verdict_digest.as_str(),
            ),
            (
                "safety.red_latch_receipt_digest",
                latch.latch_receipt_digest.as_str(),
                self.core.red_latch_receipt_digest.as_str(),
            ),
            (
                "safety.actuator_identity_key_binary_policy_digest",
                kernel
                    .core
                    .safety_actuator_identity_key_binary_policy_digest
                    .as_str(),
                self.core
                    .actuator_identity_key_binary_policy_digest
                    .as_str(),
            ),
            (
                "safety.affected_grants_scope_digest",
                latch.core.exact_affected_scope_digest.as_str(),
                self.core.affected_grants_scope_digest.as_str(),
            ),
            (
                "safety.allowed_negative_actions_digest",
                latch.core.allowed_negative_actions_digest.as_str(),
                self.core.allowed_negative_actions_digest.as_str(),
            ),
            (
                "safety.rollback_candidate_plan_digest",
                latch.core.rollback_candidate_plan_digest.as_str(),
                self.core.rollback_candidate_plan_digest.as_str(),
            ),
        ] {
            require_digest_equality(field, expected, observed)?;
        }
        if self.core.source_intent_core_ref != source_intent.intent_core_ref {
            return Err(AutonomyContractError::BindingMismatch {
                field: "safety.source_intent_core_ref",
            });
        }
        if red_verdict.core.verdict != SentinelVerdict::Red
            || latch.core.state != RedLatchState::Pending
            || !self.core.valid_while_latch_pending
        {
            return Err(AutonomyContractError::Invariant {
                rule: "safety intent is valid only for an unresolved PENDING RED latch",
            });
        }
        if self.core.expected_constitution_epoch != current_constitution_epoch
            || self.core.expected_autonomy_epoch != current_autonomy_epoch
        {
            return Err(AutonomyContractError::BindingMismatch {
                field: "safety.expected_epochs",
            });
        }
        validate_safety_effects(&self.core.negative_effects, kernel)?;
        let expected_effects_digest = compute_safety_effects_digest(&self.core.negative_effects)?;
        require_digest_equality(
            "safety.allowed_negative_actions_digest",
            &expected_effects_digest,
            &self.core.allowed_negative_actions_digest,
        )?;
        require_digest("safety_intent_digest", &self.safety_intent_digest)?;
        let computed = self.compute_digest()?;
        require_digest_equality(
            "safety_intent_digest",
            &computed,
            &self.safety_intent_digest,
        )?;
        self.safety_intent_core_ref.validate(true)?;
        require_digest_equality(
            "safety_intent_core_ref.intent_digest",
            &self.safety_intent_digest,
            &self.safety_intent_core_ref.intent_digest,
        )?;
        Ok(AutonomyStructuralValidation::opaque(computed))
    }

    pub fn validate_retry_authority_unchanged(
        &self,
        previous: &SafetyActionIntentV1,
    ) -> Result<(), AutonomyContractError> {
        if self.core.attempt_sequence != previous.core.attempt_sequence + 1
            || self.core.rebased_from_attempt_digest.as_deref()
                != Some(previous.safety_intent_digest.as_str())
        {
            return Err(AutonomyContractError::BindingMismatch {
                field: "safety.retry_sequence",
            });
        }
        let mut current = self.core.clone();
        let mut prior = previous.core.clone();
        current.safety_attempt_id.clear();
        prior.safety_attempt_id.clear();
        current.attempt_sequence = 0;
        prior.attempt_sequence = 0;
        current.rebased_from_attempt_digest = None;
        prior.rebased_from_attempt_digest = None;
        current.nonce.clear();
        prior.nonce.clear();
        current.attempt_idempotency_key.clear();
        prior.attempt_idempotency_key.clear();
        current.expected_autonomy_epoch = 0;
        prior.expected_autonomy_epoch = 0;
        current.issued_at = 0;
        prior.issued_at = 0;
        if current != prior {
            return Err(AutonomyContractError::BindingMismatch {
                field: "safety.retry_immutable_mandate",
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyCapabilityCoreV1 {
    pub capability_id: String,
    pub safety_intent_digest: String,
    pub safety_intent_core_ref: IntentCoreRefV1,
    pub safety_attempt_id: String,
    pub source_intent_digest: String,
    pub sentinel_red_verdict_digest: String,
    pub red_latch_receipt_digest: String,
    pub actuator_identity_key_binary_policy_digest: String,
    pub expected_constitution_epoch: u64,
    pub expected_autonomy_epoch: u64,
    pub affected_grants_scope_digest: String,
    pub negative_effects: BTreeSet<Effect>,
    pub allowed_negative_actions_digest: String,
    pub rollback_candidate_plan_digest: String,
    pub nonce: String,
    pub idempotency_key: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyCapabilityV1 {
    pub schema: String,
    pub core: SafetyCapabilityCoreV1,
    pub capability_digest: String,
    pub actuator_signature: OpaqueSignature,
}

impl SafetyCapabilityV1 {
    pub fn compute_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(SAFETY_CAPABILITY_DIGEST_DOMAIN, &self.core)
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.capability_digest = self.compute_digest()?;
        Ok(())
    }

    pub fn validate(
        &self,
        safety_intent: &SafetyActionIntentV1,
        latch: &RedLatchReceiptV1,
        kernel: &SafetyKernelV1,
        now_ms: u64,
    ) -> Result<AutonomyStructuralValidation, AutonomyContractError> {
        require_schema("SafetyCapabilityV1", &self.schema, SAFETY_CAPABILITY_SCHEMA)?;
        for (field, value) in [
            ("safety.capability_id", &self.core.capability_id),
            ("safety.capability.nonce", &self.core.nonce),
            ("safety.idempotency_key", &self.core.idempotency_key),
        ] {
            require_non_empty(field, value)?;
        }
        require_time_window(
            "SafetyCapabilityV1",
            self.core.issued_at,
            self.core.expires_at,
            now_ms,
        )?;
        for (field, expected, observed) in [
            (
                "safety_capability.safety_intent_digest",
                safety_intent.safety_intent_digest.as_str(),
                self.core.safety_intent_digest.as_str(),
            ),
            (
                "safety_capability.source_intent_digest",
                safety_intent.core.source_intent_digest.as_str(),
                self.core.source_intent_digest.as_str(),
            ),
            (
                "safety_capability.sentinel_red_verdict_digest",
                safety_intent.core.sentinel_red_verdict_digest.as_str(),
                self.core.sentinel_red_verdict_digest.as_str(),
            ),
            (
                "safety_capability.red_latch_receipt_digest",
                latch.latch_receipt_digest.as_str(),
                self.core.red_latch_receipt_digest.as_str(),
            ),
            (
                "safety_capability.actuator_identity_key_binary_policy_digest",
                kernel
                    .core
                    .safety_actuator_identity_key_binary_policy_digest
                    .as_str(),
                self.core
                    .actuator_identity_key_binary_policy_digest
                    .as_str(),
            ),
            (
                "safety_capability.affected_grants_scope_digest",
                safety_intent.core.affected_grants_scope_digest.as_str(),
                self.core.affected_grants_scope_digest.as_str(),
            ),
            (
                "safety_capability.allowed_negative_actions_digest",
                safety_intent.core.allowed_negative_actions_digest.as_str(),
                self.core.allowed_negative_actions_digest.as_str(),
            ),
            (
                "safety_capability.rollback_candidate_plan_digest",
                safety_intent.core.rollback_candidate_plan_digest.as_str(),
                self.core.rollback_candidate_plan_digest.as_str(),
            ),
        ] {
            require_digest_equality(field, expected, observed)?;
        }
        if self.core.safety_intent_core_ref != safety_intent.safety_intent_core_ref
            || self.core.safety_attempt_id != safety_intent.core.safety_attempt_id
            || self.core.expected_constitution_epoch
                != safety_intent.core.expected_constitution_epoch
            || self.core.expected_autonomy_epoch != safety_intent.core.expected_autonomy_epoch
            || self.core.nonce != safety_intent.core.nonce
            || self.core.idempotency_key != safety_intent.core.attempt_idempotency_key
            || self.core.negative_effects != safety_intent.core.negative_effects
        {
            return Err(AutonomyContractError::BindingMismatch {
                field: "safety_capability.intent_bindings",
            });
        }
        if latch.core.state != RedLatchState::Pending {
            return Err(AutonomyContractError::Invariant {
                rule: "safety capability can only prepare against a PENDING RED latch",
            });
        }
        validate_safety_effects(&self.core.negative_effects, kernel)?;
        require_digest("safety.capability_digest", &self.capability_digest)?;
        let computed = self.compute_digest()?;
        require_digest_equality(
            "safety.capability_digest",
            &computed,
            &self.capability_digest,
        )?;
        require_signature("safety.actuator_signature", &self.actuator_signature)?;
        Ok(AutonomyStructuralValidation::opaque(computed))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QuorumVoteDisposition {
    Approve,
    Dissent,
    Abstain,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuorumVoteV1 {
    pub verifier_principal_id: String,
    pub verifier_key_id: String,
    pub failure_domain: String,
    pub parent_session_context_digest: String,
    pub intent_digest: String,
    pub constitution_digest: String,
    pub candidate_digest: Option<String>,
    pub evidence_digest: String,
    pub rollout_plan_digest: String,
    pub rollback_plan_digest: String,
    pub disposition: QuorumVoteDisposition,
    pub signature: OpaqueSignature,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentQuorumDecisionEvidenceV1 {
    pub independence_spec: IndependenceSpecV1,
    pub votes: Vec<QuorumVoteV1>,
    pub sentinel_verdict_digest: String,
}

impl AgentQuorumDecisionEvidenceV1 {
    pub fn validate(
        &self,
        intent: &SovereignActionIntentV1,
        constitution: &ConstitutionStoreV1,
        kernel: &SafetyKernelV1,
        sentinel_verdict_digest: &str,
    ) -> Result<(), AutonomyContractError> {
        self.independence_spec.validate_against_kernel(kernel)?;
        require_digest_equality(
            "quorum.independence_spec_digest",
            &constitution.core.independence_spec_digest,
            &self.independence_spec.independence_spec_digest,
        )?;
        if self.independence_spec.core.constitution_epoch != intent.core.constitution_epoch {
            return Err(AutonomyContractError::EpochMismatch {
                field: "quorum.constitution_epoch",
                expected: intent.core.constitution_epoch,
                observed: self.independence_spec.core.constitution_epoch,
            });
        }
        require_digest_equality(
            "quorum.sentinel_verdict_digest",
            sentinel_verdict_digest,
            &self.sentinel_verdict_digest,
        )?;
        let proposer = intent.core.proposer_subject_id.as_str();
        let executor = intent.core.executor_subject_id.as_deref();
        let promotion_target = intent.core.promotion_target_subject_id.as_deref();
        let ratification_target = intent.core.ratification_target_subject_id.as_deref();
        let seats: BTreeMap<_, _> = self
            .independence_spec
            .core
            .voting_verifiers
            .iter()
            .map(|seat| (seat.principal_id.as_str(), seat))
            .collect();
        if seats.contains_key(proposer)
            || executor.is_some_and(|subject| seats.contains_key(subject))
            || promotion_target.is_some_and(|subject| seats.contains_key(subject))
            || ratification_target.is_some_and(|subject| seats.contains_key(subject))
        {
            return Err(AutonomyContractError::SelfAuthorization {
                subject_id: proposer.to_string(),
            });
        }
        let mut voted = BTreeSet::new();
        let mut approval_domains = BTreeSet::new();
        let mut approvals = 0usize;
        if self.votes.len() > seats.len() {
            return Err(AutonomyContractError::NonIndependentQuorum);
        }
        for vote in &self.votes {
            let Some(seat) = seats.get(vote.verifier_principal_id.as_str()) else {
                return Err(AutonomyContractError::NonIndependentQuorum);
            };
            if !voted.insert(vote.verifier_principal_id.as_str())
                || vote.verifier_key_id != seat.key_id
                || vote.failure_domain != seat.failure_domain
                || vote.parent_session_context_digest != seat.parent_session_context_digest
            {
                return Err(AutonomyContractError::NonIndependentQuorum);
            }
            require_digest_equality(
                "vote.intent_digest",
                &intent.intent_digest,
                &vote.intent_digest,
            )?;
            require_digest_equality(
                "vote.constitution_digest",
                &intent.core.constitution_digest,
                &vote.constitution_digest,
            )?;
            for (field, expected, observed) in [
                (
                    "vote.evidence_digest",
                    intent.core.evidence_digest.as_str(),
                    vote.evidence_digest.as_str(),
                ),
                (
                    "vote.rollout_plan_digest",
                    intent.core.rollout_plan_digest.as_str(),
                    vote.rollout_plan_digest.as_str(),
                ),
                (
                    "vote.rollback_plan_digest",
                    intent.core.rollback_plan_digest.as_str(),
                    vote.rollback_plan_digest.as_str(),
                ),
            ] {
                require_digest_equality(field, expected, observed)?;
            }
            if vote.candidate_digest != intent.core.candidate_digest {
                return Err(AutonomyContractError::BindingMismatch {
                    field: "vote.candidate_digest",
                });
            }
            require_signature("quorum.vote.signature", &vote.signature)?;
            match vote.disposition {
                QuorumVoteDisposition::Approve => {
                    approvals += 1;
                    approval_domains.insert(vote.failure_domain.as_str());
                }
                QuorumVoteDisposition::Dissent | QuorumVoteDisposition::Abstain => {
                    return Err(AutonomyContractError::QuorumNotUnanimouslyResolvable);
                }
            }
        }
        let required = usize::from(self.independence_spec.core.quorum_threshold);
        if approvals < required {
            return Err(AutonomyContractError::InsufficientQuorum {
                approvals,
                required,
            });
        }
        if approval_domains.len() < usize::from(self.independence_spec.core.minimum_failure_domains)
        {
            return Err(AutonomyContractError::NonIndependentQuorum);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityDecisionBindingV1 {
    pub decision_id: String,
    pub intent_digest: String,
    pub intent_core_ref: IntentCoreRefV1,
    pub intent_canonicalization_version: String,
    pub required_authority_variant: AuthorityVariant,
    pub issuer_subject_id: String,
    pub decision_subject_id: String,
    pub caller_subject_id: String,
    pub audience: String,
    pub proposer_subject_id: String,
    pub executor_subject_id: Option<String>,
    pub promotion_target_subject_id: Option<String>,
    pub ratification_target_subject_id: Option<String>,
    pub delegation_grant_digest: Option<String>,
    pub action_policy_registry_digest: String,
    pub classifier_decision_digest: String,
    pub constitution_digest: String,
    pub constitution_epoch: u64,
    pub autonomy_epoch: u64,
    pub active_mode: ActiveMode,
    pub grant_id: Option<String>,
    pub effective_tier: Option<AutonomyTier>,
    pub action_class: String,
    pub semantic_action_id: String,
    pub risk_class: RiskClass,
    pub risk_scope_digest: String,
    pub resource_environment_scope_digest: String,
    pub requested_budget: u64,
    pub sentinel_required: bool,
    pub sentinel_verdict_digest: Option<String>,
    pub action_payload_digest: String,
}

impl AuthorityDecisionBindingV1 {
    fn validate_for_intent(
        &self,
        intent: &SovereignActionIntentV1,
        authority: AuthorityVariant,
        sentinel: Option<&SentinelVerdictV1>,
    ) -> Result<(), AutonomyContractError> {
        require_non_empty("decision_id", &self.decision_id)?;
        for (field, expected, observed) in [
            (
                "decision.intent_digest",
                intent.intent_digest.as_str(),
                self.intent_digest.as_str(),
            ),
            (
                "decision.issuer_subject_id",
                intent.core.issuer_subject_id.as_str(),
                self.issuer_subject_id.as_str(),
            ),
            (
                "decision.decision_subject_id",
                intent.core.decision_subject_id.as_str(),
                self.decision_subject_id.as_str(),
            ),
            (
                "decision.caller_subject_id",
                intent.core.caller_subject_id.as_str(),
                self.caller_subject_id.as_str(),
            ),
            (
                "decision.audience",
                intent.core.audience.as_str(),
                self.audience.as_str(),
            ),
            (
                "decision.proposer_subject_id",
                intent.core.proposer_subject_id.as_str(),
                self.proposer_subject_id.as_str(),
            ),
            (
                "decision.action_policy_registry_digest",
                intent.core.action_policy_registry_digest.as_str(),
                self.action_policy_registry_digest.as_str(),
            ),
            (
                "decision.classifier_decision_digest",
                intent.core.classifier_decision_digest.as_str(),
                self.classifier_decision_digest.as_str(),
            ),
            (
                "decision.constitution_digest",
                intent.core.constitution_digest.as_str(),
                self.constitution_digest.as_str(),
            ),
            (
                "decision.action_class",
                intent.core.action_class.as_str(),
                self.action_class.as_str(),
            ),
            (
                "decision.semantic_action_id",
                intent.core.semantic_action_id.as_str(),
                self.semantic_action_id.as_str(),
            ),
            (
                "decision.risk_scope_digest",
                intent.core.risk_scope_digest.as_str(),
                self.risk_scope_digest.as_str(),
            ),
            (
                "decision.resource_environment_scope_digest",
                intent.core.resource_environment_scope_digest.as_str(),
                self.resource_environment_scope_digest.as_str(),
            ),
            (
                "decision.action_payload_digest",
                intent.core.action_payload_digest.as_str(),
                self.action_payload_digest.as_str(),
            ),
        ] {
            if expected != observed {
                return Err(AutonomyContractError::BindingMismatch { field });
            }
        }
        if self.intent_core_ref != intent.intent_core_ref
            || self.intent_canonicalization_version != CANONICALIZATION_VERSION
            || self.required_authority_variant != authority
            || self.required_authority_variant != intent.core.required_authority_variant
            || self.executor_subject_id != intent.core.executor_subject_id
            || self.promotion_target_subject_id != intent.core.promotion_target_subject_id
            || self.ratification_target_subject_id != intent.core.ratification_target_subject_id
            || self.delegation_grant_digest != intent.core.delegation_grant_digest
            || self.constitution_epoch != intent.core.constitution_epoch
            || self.autonomy_epoch != intent.core.autonomy_epoch
            || self.active_mode != intent.core.active_mode
            || self.risk_class != intent.core.risk_class
            || self.requested_budget != intent.core.requested_budget
        {
            return Err(AutonomyContractError::BindingMismatch {
                field: "decision.intent_bindings",
            });
        }
        let expected_grant_id = if matches!(
            authority,
            AuthorityVariant::Policy | AuthorityVariant::AgentQuorum
        ) {
            intent.core.applicable_grant_id.as_deref()
        } else {
            None
        };
        let expected_tier = matches!(
            authority,
            AuthorityVariant::Policy | AuthorityVariant::AgentQuorum
        )
        .then_some(intent.core.effective_tier);
        if self.grant_id.as_deref() != expected_grant_id || self.effective_tier != expected_tier {
            return Err(AutonomyContractError::BindingMismatch {
                field: "decision.grant_or_tier",
            });
        }
        match (self.sentinel_required, sentinel) {
            (true, Some(verdict)) => {
                if verdict.core.verdict != SentinelVerdict::Green
                    || self.sentinel_verdict_digest.as_deref()
                        != Some(verdict.verdict_digest.as_str())
                {
                    return Err(AutonomyContractError::BindingMismatch {
                        field: "decision.sentinel_verdict_digest",
                    });
                }
            }
            (true, None) => {
                return Err(AutonomyContractError::Invariant {
                    rule: "sentinel-required decision has no exact GREEN verdict",
                });
            }
            (false, None) if self.sentinel_verdict_digest.is_none() => {}
            (false, _) => {
                return Err(AutonomyContractError::BindingMismatch {
                    field: "decision.sentinel_exemption",
                });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanAuthorityDecisionCoreV1 {
    pub binding: AuthorityDecisionBindingV1,
    pub human_approval_digest: String,
    pub human_decision_digest: String,
    pub human_key_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanAuthorityDecisionV1 {
    pub schema: String,
    pub core: HumanAuthorityDecisionCoreV1,
    pub decision_digest: String,
    pub owner_signature: OpaqueSignature,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyAuthorityDecisionCoreV1 {
    pub binding: AuthorityDecisionBindingV1,
    pub policy_digest: String,
    pub matched_clauses_digest: String,
    pub risk_budget_scope_digest: String,
    pub proof_receipts_digest: String,
    pub sentinel_exemption_clause_digest: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyAuthorityDecisionV1 {
    pub schema: String,
    pub core: PolicyAuthorityDecisionCoreV1,
    pub decision_digest: String,
    pub owner_signature: OpaqueSignature,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentQuorumAuthorityDecisionCoreV1 {
    pub binding: AuthorityDecisionBindingV1,
    pub quorum: AgentQuorumDecisionEvidenceV1,
    pub evidence_rollout_rollback_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentQuorumAuthorityDecisionV1 {
    pub schema: String,
    pub core: AgentQuorumAuthorityDecisionCoreV1,
    pub decision_digest: String,
    pub owner_signature: OpaqueSignature,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyAuthorityDecisionCoreV1 {
    pub decision_id: String,
    pub safety_intent_digest: String,
    pub safety_intent_core_ref: IntentCoreRefV1,
    pub safety_capability_digest: String,
    pub sentinel_red_verdict_digest: String,
    pub red_latch_receipt_digest: String,
    pub negative_effects: BTreeSet<Effect>,
    pub positive_authority_decision_forbidden: bool,
    pub issuer_subject_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyAuthorityDecisionV1 {
    pub schema: String,
    pub core: SafetyAuthorityDecisionCoreV1,
    pub decision_digest: String,
    pub safety_kernel_signature: OpaqueSignature,
}

/// Exactly one authority variant. SAFETY is a negative-only decision envelope;
/// it can never mint [`AutonomyCapabilityV1`] or authorize a positive action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "authority_kind",
    content = "authority_decision",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum AuthorityDecisionV1 {
    Human(HumanAuthorityDecisionV1),
    Policy(PolicyAuthorityDecisionV1),
    AgentQuorum(AgentQuorumAuthorityDecisionV1),
    Safety(SafetyAuthorityDecisionV1),
}

impl AuthorityDecisionV1 {
    pub const fn authority_variant(&self) -> AuthorityVariant {
        match self {
            Self::Human(_) => AuthorityVariant::Human,
            Self::Policy(_) => AuthorityVariant::Policy,
            Self::AgentQuorum(_) => AuthorityVariant::AgentQuorum,
            Self::Safety(_) => AuthorityVariant::SafetyKernel,
        }
    }

    pub fn decision_digest(&self) -> &str {
        match self {
            Self::Human(decision) => &decision.decision_digest,
            Self::Policy(decision) => &decision.decision_digest,
            Self::AgentQuorum(decision) => &decision.decision_digest,
            Self::Safety(decision) => &decision.decision_digest,
        }
    }

    pub fn positive_binding(&self) -> Option<&AuthorityDecisionBindingV1> {
        match self {
            Self::Human(decision) => Some(&decision.core.binding),
            Self::Policy(decision) => Some(&decision.core.binding),
            Self::AgentQuorum(decision) => Some(&decision.core.binding),
            Self::Safety(_) => None,
        }
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        match self {
            Self::Human(decision) => {
                decision.decision_digest =
                    digest_canonical(AUTHORITY_DECISION_DIGEST_DOMAIN, &decision.core)?;
            }
            Self::Policy(decision) => {
                decision.decision_digest =
                    digest_canonical(AUTHORITY_DECISION_DIGEST_DOMAIN, &decision.core)?;
            }
            Self::AgentQuorum(decision) => {
                decision.decision_digest =
                    digest_canonical(AUTHORITY_DECISION_DIGEST_DOMAIN, &decision.core)?;
            }
            Self::Safety(decision) => {
                decision.decision_digest =
                    digest_canonical(AUTHORITY_DECISION_DIGEST_DOMAIN, &decision.core)?;
            }
        }
        Ok(())
    }

    pub fn validate_positive(
        &self,
        intent: &SovereignActionIntentV1,
        constitution: &ConstitutionStoreV1,
        kernel: &SafetyKernelV1,
        sentinel: Option<&SentinelVerdictV1>,
        now_ms: u64,
    ) -> Result<AutonomyStructuralValidation, AutonomyContractError> {
        intent.validate_canonical_core(now_ms)?;
        if let Some(verdict) = sentinel {
            verdict.validate_for_intent(intent, kernel, now_ms)?;
        }
        let (binding, computed, signature) = match self {
            Self::Human(decision) => {
                require_schema(
                    "AuthorityDecisionV1::Human",
                    &decision.schema,
                    AUTHORITY_DECISION_SCHEMA,
                )?;
                decision.core.binding.validate_for_intent(
                    intent,
                    AuthorityVariant::Human,
                    sentinel,
                )?;
                for (field, digest) in [
                    (
                        "human_approval_digest",
                        &decision.core.human_approval_digest,
                    ),
                    (
                        "human_decision_digest",
                        &decision.core.human_decision_digest,
                    ),
                ] {
                    require_digest(field, digest)?;
                }
                require_non_empty("human_key_id", &decision.core.human_key_id)?;
                (
                    &decision.core.binding,
                    digest_canonical(AUTHORITY_DECISION_DIGEST_DOMAIN, &decision.core)?,
                    &decision.owner_signature,
                )
            }
            Self::Policy(decision) => {
                require_schema(
                    "AuthorityDecisionV1::Policy",
                    &decision.schema,
                    AUTHORITY_DECISION_SCHEMA,
                )?;
                decision.core.binding.validate_for_intent(
                    intent,
                    AuthorityVariant::Policy,
                    sentinel,
                )?;
                if decision.core.binding.active_mode != ActiveMode::PolicyAutonomous {
                    return Err(AutonomyContractError::Invariant {
                        rule: "POLICY decision is authoritative only in POLICY_AUTONOMOUS",
                    });
                }
                for (field, digest) in [
                    ("policy_digest", &decision.core.policy_digest),
                    (
                        "matched_clauses_digest",
                        &decision.core.matched_clauses_digest,
                    ),
                    (
                        "risk_budget_scope_digest",
                        &decision.core.risk_budget_scope_digest,
                    ),
                    (
                        "proof_receipts_digest",
                        &decision.core.proof_receipts_digest,
                    ),
                ] {
                    require_digest(field, digest)?;
                }
                match (
                    decision.core.binding.sentinel_required,
                    &decision.core.sentinel_exemption_clause_digest,
                ) {
                    (false, Some(digest)) => {
                        require_digest("sentinel_exemption_clause_digest", digest)?
                    }
                    (true, None) => {}
                    _ => {
                        return Err(AutonomyContractError::Invariant {
                            rule: "policy sentinel exemption must exist exactly when sentinel is not required",
                        });
                    }
                }
                (
                    &decision.core.binding,
                    digest_canonical(AUTHORITY_DECISION_DIGEST_DOMAIN, &decision.core)?,
                    &decision.owner_signature,
                )
            }
            Self::AgentQuorum(decision) => {
                require_schema(
                    "AuthorityDecisionV1::AgentQuorum",
                    &decision.schema,
                    AUTHORITY_DECISION_SCHEMA,
                )?;
                decision.core.binding.validate_for_intent(
                    intent,
                    AuthorityVariant::AgentQuorum,
                    sentinel,
                )?;
                if decision.core.binding.active_mode == ActiveMode::HumanGated {
                    return Err(AutonomyContractError::Invariant {
                        rule: "AGENT_QUORUM cannot create sovereign authority in HUMAN_GATED",
                    });
                }
                let verdict = sentinel.ok_or(AutonomyContractError::Invariant {
                    rule: "agent quorum requires the exact independent GREEN sentinel verdict",
                })?;
                decision.core.quorum.validate(
                    intent,
                    constitution,
                    kernel,
                    &verdict.verdict_digest,
                )?;
                require_digest(
                    "evidence_rollout_rollback_digest",
                    &decision.core.evidence_rollout_rollback_digest,
                )?;
                (
                    &decision.core.binding,
                    digest_canonical(AUTHORITY_DECISION_DIGEST_DOMAIN, &decision.core)?,
                    &decision.owner_signature,
                )
            }
            Self::Safety(_) => {
                return Err(AutonomyContractError::Invariant {
                    rule: "SAFETY authority is negative-only and cannot validate as positive",
                });
            }
        };
        require_digest("decision_digest", self.decision_digest())?;
        require_digest_equality("decision_digest", &computed, self.decision_digest())?;
        require_signature("authority_decision.owner_signature", signature)?;
        if binding.constitution_digest != constitution.constitution_digest
            || binding.constitution_epoch != constitution.core.constitution_epoch
        {
            return Err(AutonomyContractError::BindingMismatch {
                field: "decision.active_constitution",
            });
        }
        Ok(AutonomyStructuralValidation::opaque(computed))
    }

    pub fn validate_safety(
        &self,
        safety_intent: &SafetyActionIntentV1,
        capability: &SafetyCapabilityV1,
        latch: &RedLatchReceiptV1,
        kernel: &SafetyKernelV1,
    ) -> Result<AutonomyStructuralValidation, AutonomyContractError> {
        let Self::Safety(decision) = self else {
            return Err(AutonomyContractError::Invariant {
                rule: "positive authority variant cannot enter a safety transaction",
            });
        };
        require_schema(
            "AuthorityDecisionV1::Safety",
            &decision.schema,
            AUTHORITY_DECISION_SCHEMA,
        )?;
        require_non_empty("safety.decision_id", &decision.core.decision_id)?;
        require_non_empty(
            "safety.decision.issuer_subject_id",
            &decision.core.issuer_subject_id,
        )?;
        if !decision.core.positive_authority_decision_forbidden {
            return Err(AutonomyContractError::Invariant {
                rule: "SAFETY decision must explicitly forbid positive authority",
            });
        }
        for (field, expected, observed) in [
            (
                "safety_decision.safety_intent_digest",
                safety_intent.safety_intent_digest.as_str(),
                decision.core.safety_intent_digest.as_str(),
            ),
            (
                "safety_decision.safety_capability_digest",
                capability.capability_digest.as_str(),
                decision.core.safety_capability_digest.as_str(),
            ),
            (
                "safety_decision.sentinel_red_verdict_digest",
                safety_intent.core.sentinel_red_verdict_digest.as_str(),
                decision.core.sentinel_red_verdict_digest.as_str(),
            ),
            (
                "safety_decision.red_latch_receipt_digest",
                latch.latch_receipt_digest.as_str(),
                decision.core.red_latch_receipt_digest.as_str(),
            ),
        ] {
            require_digest_equality(field, expected, observed)?;
        }
        if decision.core.safety_intent_core_ref != safety_intent.safety_intent_core_ref
            || decision.core.negative_effects != safety_intent.core.negative_effects
        {
            return Err(AutonomyContractError::BindingMismatch {
                field: "safety_decision.intent_bindings",
            });
        }
        validate_safety_effects(&decision.core.negative_effects, kernel)?;
        require_digest("safety.decision_digest", &decision.decision_digest)?;
        let computed = digest_canonical(AUTHORITY_DECISION_DIGEST_DOMAIN, &decision.core)?;
        require_digest_equality(
            "safety.decision_digest",
            &computed,
            &decision.decision_digest,
        )?;
        require_signature(
            "safety_decision.safety_kernel_signature",
            &decision.safety_kernel_signature,
        )?;
        Ok(AutonomyStructuralValidation::opaque(computed))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutonomyCapabilityCoreV1 {
    pub capability_id: String,
    pub intent_digest: String,
    pub intent_core_ref: IntentCoreRefV1,
    pub intent_canonicalization_version: String,
    pub decision_digest: String,
    pub decision_policy_digest: String,
    pub required_authority_variant: AuthorityVariant,
    pub action_policy_registry_digest: String,
    pub classifier_decision_digest: String,
    pub constitution_digest: String,
    pub constitution_epoch: u64,
    pub autonomy_epoch: u64,
    pub organism_id: String,
    pub repo_id: String,
    pub issuer_subject_id: String,
    pub decision_subject_id: String,
    pub caller_subject_id: String,
    pub proposer_subject_id: String,
    pub executor_subject_id: Option<String>,
    pub promotion_target_subject_id: Option<String>,
    pub ratification_target_subject_id: Option<String>,
    pub delegation_grant_digest: Option<String>,
    pub audience: String,
    pub active_mode: ActiveMode,
    pub activation_receipt_id: Option<String>,
    pub grant_id: String,
    pub grant_digest: String,
    pub effective_tier: AutonomyTier,
    pub action_class: String,
    pub semantic_action_id: String,
    pub risk_class: RiskClass,
    pub risk_scope_digest: String,
    pub sentinel_verdict_digest: Option<String>,
    pub brain_id: String,
    pub mission_id: Option<String>,
    pub mission_head_id: Option<String>,
    pub block_id: Option<String>,
    pub candidate_digest: Option<String>,
    pub promotion_subject_id: Option<String>,
    pub resource_environment_scope_digest: String,
    pub requested_budget: u64,
    pub expected_store_epoch: u64,
    pub expected_store_version: u64,
    pub expected_boundary_version: u64,
    pub expected_contract_version: u64,
    pub idempotency_key: String,
    pub payload_digest: String,
    pub nonce: String,
    pub issued_at: u64,
    pub expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutonomyCapabilityV1 {
    pub schema: String,
    pub core: AutonomyCapabilityCoreV1,
    pub capability_digest: String,
    pub owner_signature: OpaqueSignature,
}

impl AutonomyCapabilityV1 {
    pub fn compute_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(AUTONOMY_CAPABILITY_DIGEST_DOMAIN, &self.core)
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.capability_digest = self.compute_digest()?;
        Ok(())
    }

    pub fn validate(
        &self,
        decision: &AuthorityDecisionV1,
        intent: &SovereignActionIntentV1,
        grant: &AutonomyGrantV1,
        epoch: &AutonomyEpochV1,
        sentinel: Option<&SentinelVerdictV1>,
        now_ms: u64,
    ) -> Result<AutonomyStructuralValidation, AutonomyContractError> {
        require_schema(
            "AutonomyCapabilityV1",
            &self.schema,
            AUTONOMY_CAPABILITY_SCHEMA,
        )?;
        intent.validate_canonical_core(now_ms)?;
        let decision_binding =
            decision
                .positive_binding()
                .ok_or(AutonomyContractError::Invariant {
                    rule: "SAFETY decision cannot mint a positive autonomy capability",
                })?;
        let authority = decision.authority_variant();
        if !matches!(
            authority,
            AuthorityVariant::Policy | AuthorityVariant::AgentQuorum
        ) {
            return Err(AutonomyContractError::Invariant {
                rule: "autonomy capability requires POLICY or AGENT_QUORUM decision",
            });
        }
        let expected_decision_policy_digest = match decision {
            AuthorityDecisionV1::Policy(policy) => policy.core.policy_digest.as_str(),
            AuthorityDecisionV1::AgentQuorum(quorum) => quorum
                .core
                .quorum
                .independence_spec
                .independence_spec_digest
                .as_str(),
            AuthorityDecisionV1::Human(_) | AuthorityDecisionV1::Safety(_) => {
                unreachable!("non-autonomous variants were rejected above")
            }
        };
        require_digest_equality(
            "capability.decision_policy_digest",
            expected_decision_policy_digest,
            &self.core.decision_policy_digest,
        )?;
        for (field, value) in [
            ("capability_id", &self.core.capability_id),
            ("organism_id", &self.core.organism_id),
            ("repo_id", &self.core.repo_id),
            ("capability.issuer_subject_id", &self.core.issuer_subject_id),
            (
                "capability.decision_subject_id",
                &self.core.decision_subject_id,
            ),
            ("capability.caller_subject_id", &self.core.caller_subject_id),
            (
                "capability.proposer_subject_id",
                &self.core.proposer_subject_id,
            ),
            ("capability.audience", &self.core.audience),
            ("capability.grant_id", &self.core.grant_id),
            ("capability.action_class", &self.core.action_class),
            (
                "capability.semantic_action_id",
                &self.core.semantic_action_id,
            ),
            ("capability.brain_id", &self.core.brain_id),
            ("capability.idempotency_key", &self.core.idempotency_key),
            ("capability.nonce", &self.core.nonce),
        ] {
            require_non_empty(field, value)?;
        }
        if !ActionId::new(&self.core.semantic_action_id)
            .is_ok_and(|action| action.is_semantic_catalog_id())
        {
            return Err(AutonomyContractError::Invariant {
                rule: "autonomy capability semantic_action_id is not a canonical G2 action id",
            });
        }
        require_time_window(
            "AutonomyCapabilityV1",
            self.core.issued_at,
            self.core.expires_at,
            now_ms,
        )?;
        if self.core.issued_at < intent.core.issued_at
            || self.core.expires_at > intent.core.expires_at
        {
            return Err(AutonomyContractError::Invariant {
                rule: "capability lifetime must be inside its immutable intent lifetime",
            });
        }
        if epoch.issuance_frozen || epoch.safety_state != SafetyState::Healthy {
            return Err(AutonomyContractError::Invariant {
                rule: "positive capability mint/validation is fenced by safety state",
            });
        }
        for (field, expected, observed) in [
            (
                "capability.intent_digest",
                intent.intent_digest.as_str(),
                self.core.intent_digest.as_str(),
            ),
            (
                "capability.decision_digest",
                decision.decision_digest(),
                self.core.decision_digest.as_str(),
            ),
            (
                "capability.action_policy_registry_digest",
                intent.core.action_policy_registry_digest.as_str(),
                self.core.action_policy_registry_digest.as_str(),
            ),
            (
                "capability.classifier_decision_digest",
                intent.core.classifier_decision_digest.as_str(),
                self.core.classifier_decision_digest.as_str(),
            ),
            (
                "capability.constitution_digest",
                intent.core.constitution_digest.as_str(),
                self.core.constitution_digest.as_str(),
            ),
            (
                "capability.organism_id",
                intent.core.organism_id.as_str(),
                self.core.organism_id.as_str(),
            ),
            (
                "capability.repo_id",
                intent.core.repo_id.as_str(),
                self.core.repo_id.as_str(),
            ),
            (
                "capability.issuer_subject_id",
                intent.core.issuer_subject_id.as_str(),
                self.core.issuer_subject_id.as_str(),
            ),
            (
                "capability.decision_subject_id",
                intent.core.decision_subject_id.as_str(),
                self.core.decision_subject_id.as_str(),
            ),
            (
                "capability.caller_subject_id",
                intent.core.caller_subject_id.as_str(),
                self.core.caller_subject_id.as_str(),
            ),
            (
                "capability.proposer_subject_id",
                intent.core.proposer_subject_id.as_str(),
                self.core.proposer_subject_id.as_str(),
            ),
            (
                "capability.audience",
                intent.core.audience.as_str(),
                self.core.audience.as_str(),
            ),
            (
                "capability.grant_id",
                grant.core.grant_id.as_str(),
                self.core.grant_id.as_str(),
            ),
            (
                "capability.grant_digest",
                grant.grant_digest.as_str(),
                self.core.grant_digest.as_str(),
            ),
            (
                "capability.action_class",
                intent.core.action_class.as_str(),
                self.core.action_class.as_str(),
            ),
            (
                "capability.semantic_action_id",
                intent.core.semantic_action_id.as_str(),
                self.core.semantic_action_id.as_str(),
            ),
            (
                "capability.risk_scope_digest",
                intent.core.risk_scope_digest.as_str(),
                self.core.risk_scope_digest.as_str(),
            ),
            (
                "capability.brain_id",
                intent.core.brain_id.as_str(),
                self.core.brain_id.as_str(),
            ),
            (
                "capability.resource_environment_scope_digest",
                intent.core.resource_environment_scope_digest.as_str(),
                self.core.resource_environment_scope_digest.as_str(),
            ),
            (
                "capability.payload_digest",
                intent.core.action_payload_digest.as_str(),
                self.core.payload_digest.as_str(),
            ),
            (
                "capability.nonce",
                intent.core.nonce.as_str(),
                self.core.nonce.as_str(),
            ),
        ] {
            if expected != observed {
                return Err(AutonomyContractError::BindingMismatch { field });
            }
        }
        if self.core.intent_core_ref != intent.intent_core_ref
            || self.core.intent_canonicalization_version != CANONICALIZATION_VERSION
            || self.core.required_authority_variant != authority
            || self.core.required_authority_variant != intent.core.required_authority_variant
            || self.core.constitution_epoch != intent.core.constitution_epoch
            || self.core.autonomy_epoch != intent.core.autonomy_epoch
            || self.core.active_mode != intent.core.active_mode
            || self.core.active_mode != epoch.active_mode
            || self.core.executor_subject_id != intent.core.executor_subject_id
            || self.core.promotion_target_subject_id != intent.core.promotion_target_subject_id
            || self.core.ratification_target_subject_id
                != intent.core.ratification_target_subject_id
            || self.core.delegation_grant_digest != intent.core.delegation_grant_digest
            || self.core.effective_tier != intent.core.effective_tier
            || self.core.risk_class != intent.core.risk_class
            || self.core.mission_id != intent.core.mission_id
            || self.core.mission_head_id != intent.core.mission_head_id
            || self.core.block_id != intent.core.block_id
            || self.core.candidate_digest != intent.core.candidate_digest
            || self.core.promotion_subject_id != intent.core.promotion_subject_id
            || self.core.requested_budget != intent.core.requested_budget
            || self.core.expected_store_epoch != intent.core.expected_store_epoch
            || self.core.expected_store_version != intent.core.expected_store_version
            || self.core.expected_boundary_version != intent.core.expected_boundary_version
            || self.core.expected_contract_version != intent.core.expected_contract_version
        {
            return Err(AutonomyContractError::BindingMismatch {
                field: "capability.intent_bindings",
            });
        }
        if self.core.activation_receipt_id != epoch.activation_receipt_id {
            return Err(AutonomyContractError::BindingMismatch {
                field: "capability.activation_receipt_id",
            });
        }
        if decision_binding
            != &AuthorityDecisionBindingV1::from_intent(
                intent,
                decision_binding.decision_id.clone(),
                decision_binding.sentinel_required,
                decision_binding.sentinel_verdict_digest.clone(),
            )
        {
            return Err(AutonomyContractError::BindingMismatch {
                field: "capability.authority_decision_binding",
            });
        }
        grant.validate_at(now_ms)?;
        intent.validate_grant_binding(grant)?;
        if let Some(verdict) = sentinel {
            require_time_window(
                "SentinelVerdictV1",
                verdict.core.issued_at,
                verdict.core.expires_at,
                now_ms,
            )?;
            if self.core.sentinel_verdict_digest.as_deref() != Some(verdict.verdict_digest.as_str())
                || verdict.core.verdict != SentinelVerdict::Green
                || verdict.core.intent_digest != intent.intent_digest
                || verdict.core.intent_core_ref != intent.intent_core_ref
                || verdict.core.constitution_epoch != intent.core.constitution_epoch
                || verdict.core.autonomy_epoch != intent.core.autonomy_epoch
                || verdict.core.nonce != intent.core.nonce
            {
                return Err(AutonomyContractError::BindingMismatch {
                    field: "capability.sentinel_verdict_digest",
                });
            }
        } else if self.core.sentinel_verdict_digest.is_some() {
            return Err(AutonomyContractError::BindingMismatch {
                field: "capability.sentinel_verdict_digest",
            });
        }
        require_digest("capability_digest", &self.capability_digest)?;
        let computed = self.compute_digest()?;
        require_digest_equality("capability_digest", &computed, &self.capability_digest)?;
        require_signature("capability.owner_signature", &self.owner_signature)?;
        Ok(AutonomyStructuralValidation::opaque(computed))
    }
}

impl AuthorityDecisionBindingV1 {
    pub fn from_intent(
        intent: &SovereignActionIntentV1,
        decision_id: String,
        sentinel_required: bool,
        sentinel_verdict_digest: Option<String>,
    ) -> Self {
        let autonomous = matches!(
            intent.core.required_authority_variant,
            AuthorityVariant::Policy | AuthorityVariant::AgentQuorum
        );
        Self {
            decision_id,
            intent_digest: intent.intent_digest.clone(),
            intent_core_ref: intent.intent_core_ref.clone(),
            intent_canonicalization_version: CANONICALIZATION_VERSION.to_string(),
            required_authority_variant: intent.core.required_authority_variant,
            issuer_subject_id: intent.core.issuer_subject_id.clone(),
            decision_subject_id: intent.core.decision_subject_id.clone(),
            caller_subject_id: intent.core.caller_subject_id.clone(),
            audience: intent.core.audience.clone(),
            proposer_subject_id: intent.core.proposer_subject_id.clone(),
            executor_subject_id: intent.core.executor_subject_id.clone(),
            promotion_target_subject_id: intent.core.promotion_target_subject_id.clone(),
            ratification_target_subject_id: intent.core.ratification_target_subject_id.clone(),
            delegation_grant_digest: intent.core.delegation_grant_digest.clone(),
            action_policy_registry_digest: intent.core.action_policy_registry_digest.clone(),
            classifier_decision_digest: intent.core.classifier_decision_digest.clone(),
            constitution_digest: intent.core.constitution_digest.clone(),
            constitution_epoch: intent.core.constitution_epoch,
            autonomy_epoch: intent.core.autonomy_epoch,
            active_mode: intent.core.active_mode,
            grant_id: if autonomous {
                intent.core.applicable_grant_id.clone()
            } else {
                None
            },
            effective_tier: autonomous.then_some(intent.core.effective_tier),
            action_class: intent.core.action_class.clone(),
            semantic_action_id: intent.core.semantic_action_id.clone(),
            risk_class: intent.core.risk_class,
            risk_scope_digest: intent.core.risk_scope_digest.clone(),
            resource_environment_scope_digest: intent
                .core
                .resource_environment_scope_digest
                .clone(),
            requested_budget: intent.core.requested_budget,
            sentinel_required,
            sentinel_verdict_digest,
            action_payload_digest: intent.core.action_payload_digest.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutonomyActivationReceiptCoreV1 {
    pub receipt_id: String,
    pub previous_mode_epoch_digest: String,
    pub previous_mode: ActiveMode,
    pub previous_constitution_epoch: u64,
    pub previous_autonomy_epoch: u64,
    pub previous_activation_receipt_id: Option<String>,
    pub target_constitution_digest: String,
    pub target_constitution_epoch: u64,
    pub activated_autonomy_epoch: u64,
    pub activated_mode: ActiveMode,
    pub grants_digest: String,
    pub release_candidate_digest: String,
    pub gate_receipts_digest: String,
    pub g9_canary_receipts_digest: String,
    pub authority_decision_digest: String,
    pub prior_authority_variant: AuthorityVariant,
    /// Custody floor of the authority custody era under which this activation
    /// receipt was minted (era-scoped; a successor Path-A era will carry a
    /// different value). This is NOT a candidate property — it records the floor
    /// the minting era stood on. Drawn from the ratified constant / ceremony
    /// receipt, never from request payload, and validated against the closed
    /// [`crate::RATIFIED_CUSTODY_FLOORS`] set in `validate_transition`.
    ///
    /// Schema disposition matches the gate receipt: `custody_floor` joins
    /// `m1nd-autonomy-activation-receipt-v1` without a version bump. This receipt
    /// is Rust-only and regenerable, so there is no frozen canon to migrate.
    pub custody_floor: String,
    pub rollback_plan_digest: String,
    pub activates_at: u64,
    pub issuer_subject_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutonomyActivationReceiptV1 {
    pub schema: String,
    pub core: AutonomyActivationReceiptCoreV1,
    pub receipt_digest: String,
    pub signature: OpaqueSignature,
}

#[derive(Clone, Copy, Debug)]
pub struct AutonomyActivationValidationContext<'a> {
    pub exact_release_candidate_digest: &'a str,
    pub authority_decision: &'a AuthorityDecisionV1,
    pub now_ms: u64,
}

impl AutonomyActivationReceiptV1 {
    pub fn compute_digest(&self) -> Result<String, CanonicalError> {
        self.compute_digest_without_id()
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.receipt_digest = self.compute_digest()?;
        self.core.receipt_id = format!("autonomy-activation:{}", self.receipt_digest);
        Ok(())
    }

    fn compute_digest_without_id(&self) -> Result<String, CanonicalError> {
        let mut core = self.core.clone();
        core.receipt_id.clear();
        digest_canonical(AUTONOMY_ACTIVATION_RECEIPT_DIGEST_DOMAIN, &core)
    }

    pub fn validate_transition(
        &self,
        previous: &AutonomyEpochV1,
        target: &AutonomyEpochV1,
        target_constitution: &ConstitutionStoreV1,
        target_grants: &[AutonomyGrantV1],
        context: AutonomyActivationValidationContext<'_>,
    ) -> Result<AutonomyStructuralValidation, AutonomyContractError> {
        require_schema(
            "AutonomyActivationReceiptV1",
            &self.schema,
            AUTONOMY_ACTIVATION_RECEIPT_SCHEMA,
        )?;
        require_non_empty("activation.receipt_id", &self.core.receipt_id)?;
        require_non_empty("activation.issuer_subject_id", &self.core.issuer_subject_id)?;
        require_non_empty("activation.custody_floor", &self.core.custody_floor)?;
        if !crate::is_ratified_custody_floor(&self.core.custody_floor) {
            // Fail-closed on the closed RATIFIED_CUSTODY_FLOORS set: an activation
            // cannot claim a custody floor the era never ratified.
            return Err(AutonomyContractError::Invariant {
                rule: "activation custody_floor is outside the ratified closed set",
            });
        }
        if context.now_ms < self.core.activates_at {
            return Err(AutonomyContractError::Invariant {
                rule: "activation receipt is not effective yet",
            });
        }
        let previous_digest = compute_autonomy_epoch_reference_digest(previous)?;
        require_digest_equality(
            "activation.previous_mode_epoch_digest",
            &previous_digest,
            &self.core.previous_mode_epoch_digest,
        )?;
        require_digest_equality(
            "activation.target_constitution_digest",
            &target_constitution.constitution_digest,
            &self.core.target_constitution_digest,
        )?;
        let grants_digest = compute_grants_digest(target_grants)?;
        require_digest_equality(
            "activation.grants_digest",
            &grants_digest,
            &self.core.grants_digest,
        )?;
        require_digest_equality(
            "activation.release_candidate_digest",
            context.exact_release_candidate_digest,
            &self.core.release_candidate_digest,
        )?;
        require_digest_equality(
            "activation.authority_decision_digest",
            context.authority_decision.decision_digest(),
            &self.core.authority_decision_digest,
        )?;
        for (field, digest) in [
            (
                "activation.gate_receipts_digest",
                &self.core.gate_receipts_digest,
            ),
            (
                "activation.g9_canary_receipts_digest",
                &self.core.g9_canary_receipts_digest,
            ),
            (
                "activation.rollback_plan_digest",
                &self.core.rollback_plan_digest,
            ),
        ] {
            require_digest(field, digest)?;
        }
        if self.core.previous_mode != previous.active_mode
            || self.core.previous_constitution_epoch != previous.constitution_epoch
            || self.core.previous_autonomy_epoch != previous.autonomy_epoch
            || self.core.previous_activation_receipt_id != previous.activation_receipt_id
        {
            return Err(AutonomyContractError::BindingMismatch {
                field: "activation.previous_mode_epoch",
            });
        }
        if self.core.target_constitution_epoch != target_constitution.core.constitution_epoch
            || self.core.activated_autonomy_epoch != previous.autonomy_epoch + 1
            || target.autonomy_epoch != self.core.activated_autonomy_epoch
            || target.constitution_epoch != self.core.target_constitution_epoch
            || target.constitution_digest != self.core.target_constitution_digest
            || target.active_mode != self.core.activated_mode
            || target.grants_digest != self.core.grants_digest
            || target.activation_receipt_id.as_deref() != Some(self.core.receipt_id.as_str())
        {
            return Err(AutonomyContractError::BindingMismatch {
                field: "activation.target_mode_epoch",
            });
        }
        match (previous.active_mode, target.active_mode) {
            (ActiveMode::HumanGated, ActiveMode::PolicyAutonomous)
            | (ActiveMode::PolicyAutonomous, ActiveMode::FullAutonomy) => {}
            (previous, target) => {
                return Err(AutonomyContractError::InvalidActivationTransition {
                    previous,
                    target,
                });
            }
        }
        target.validate_common(target_constitution, target_grants, context.now_ms)?;
        let required_prior_authority = match previous.active_mode {
            ActiveMode::HumanGated | ActiveMode::PolicyAutonomous => AuthorityVariant::Human,
            ActiveMode::FullAutonomy => AuthorityVariant::AgentQuorum,
        };
        if self.core.prior_authority_variant != required_prior_authority
            || context.authority_decision.authority_variant() != required_prior_authority
        {
            return Err(AutonomyContractError::AuthorityMismatch {
                expected: required_prior_authority,
                observed: context.authority_decision.authority_variant(),
            });
        }
        require_digest("activation.receipt_digest", &self.receipt_digest)?;
        let computed = self.compute_digest_without_id()?;
        require_digest_equality("activation.receipt_digest", &computed, &self.receipt_digest)?;
        if self.core.receipt_id != format!("autonomy-activation:{computed}") {
            return Err(AutonomyContractError::BindingMismatch {
                field: "activation.receipt_id",
            });
        }
        require_signature("activation.signature", &self.signature)?;
        Ok(AutonomyStructuralValidation::opaque(computed))
    }
}

pub fn compute_safety_effects_digest(
    effects: &BTreeSet<Effect>,
) -> Result<String, AutonomyContractError> {
    if effects.is_empty() {
        return Err(AutonomyContractError::EmptyCollection {
            field: "negative_effects",
        });
    }
    for effect in effects {
        if !effect.is_negative_safety() {
            return Err(AutonomyContractError::PositiveEffectInSafety { effect: *effect });
        }
    }
    Ok(digest_canonical(SAFETY_EFFECTS_DIGEST_DOMAIN, effects)?)
}

pub fn compute_autonomy_epoch_reference_digest(
    epoch: &AutonomyEpochV1,
) -> Result<String, CanonicalError> {
    digest_canonical(AUTONOMY_EPOCH_REFERENCE_DIGEST_DOMAIN, epoch)
}

fn validate_safety_effects(
    effects: &BTreeSet<Effect>,
    kernel: &SafetyKernelV1,
) -> Result<(), AutonomyContractError> {
    compute_safety_effects_digest(effects)?;
    if effects != &kernel.core.allowed_negative_effects {
        return Err(AutonomyContractError::SafetyAllowListMismatch);
    }
    Ok(())
}

fn validate_red_outbox_state(core: &SentinelRedOutboxCoreV1) -> Result<(), AutonomyContractError> {
    let valid = match core.state {
        RedOutboxState::Pending => {
            !core.journal_latch_ack
                && !core.actuator_ack
                && core.terminal_safety_transaction_id.is_none()
        }
        RedOutboxState::LatchAcknowledged => {
            core.journal_latch_ack
                && !core.actuator_ack
                && core.terminal_safety_transaction_id.is_none()
        }
        RedOutboxState::ActuatorAcknowledged => {
            core.journal_latch_ack
                && core.actuator_ack
                && core.terminal_safety_transaction_id.is_none()
        }
        RedOutboxState::Terminal => {
            core.journal_latch_ack
                && core.actuator_ack
                && core
                    .terminal_safety_transaction_id
                    .as_deref()
                    .is_some_and(|value| !value.is_empty())
        }
    };
    if !valid {
        return Err(AutonomyContractError::Invariant {
            rule: "RED outbox state contradicts its durable acknowledgements",
        });
    }
    Ok(())
}

fn validate_red_latch_state(core: &RedLatchCoreV1) -> Result<(), AutonomyContractError> {
    let valid = match core.state {
        RedLatchState::Pending => {
            core.committing_transaction_id.is_none()
                && core.commit_marker_digest.is_none()
                && core.terminal_safety_transaction_id.is_none()
        }
        RedLatchState::Committing => {
            core.committing_transaction_id
                .as_deref()
                .is_some_and(|value| !value.is_empty())
                && core.commit_marker_digest.is_some()
                && core.terminal_safety_transaction_id.is_none()
        }
        RedLatchState::Terminal => {
            core.committing_transaction_id
                .as_deref()
                .is_some_and(|value| !value.is_empty())
                && core.commit_marker_digest.is_some()
                && core.terminal_safety_transaction_id == core.committing_transaction_id
        }
    };
    if !valid {
        return Err(AutonomyContractError::Invariant {
            rule: "RED latch state contradicts its commit claim/marker/terminal transaction",
        });
    }
    if let Some(digest) = &core.commit_marker_digest {
        require_digest("latch.commit_marker_digest", digest)?;
    }
    Ok(())
}

fn digest_versioned_core<T: Serialize + ?Sized>(
    domain: &str,
    canonicalization_version: &str,
    core: &T,
) -> Result<String, CanonicalError> {
    let canonical = canonical_json(core)?;
    let mut payload = Vec::with_capacity(8 + canonicalization_version.len() + canonical.len());
    payload.extend_from_slice(&(canonicalization_version.len() as u64).to_be_bytes());
    payload.extend_from_slice(canonicalization_version.as_bytes());
    payload.extend_from_slice(&canonical);
    Ok(digest_domain_bytes(domain, &payload))
}

fn require_schema(
    contract: &'static str,
    actual: &str,
    expected: &str,
) -> Result<(), AutonomyContractError> {
    if actual != expected {
        return Err(AutonomyContractError::Schema {
            contract,
            actual: actual.to_string(),
        });
    }
    Ok(())
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), AutonomyContractError> {
    if value.trim().is_empty() {
        return Err(AutonomyContractError::EmptyRequired { field });
    }
    Ok(())
}

fn validate_optional_non_empty(
    field: &'static str,
    value: &Option<String>,
) -> Result<(), AutonomyContractError> {
    if value
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(AutonomyContractError::EmptyRequired { field });
    }
    Ok(())
}

fn require_non_empty_option(
    field: &'static str,
    value: &Option<String>,
) -> Result<(), AutonomyContractError> {
    match value.as_deref() {
        Some(value) if !value.trim().is_empty() => Ok(()),
        _ => Err(AutonomyContractError::EmptyRequired { field }),
    }
}

fn require_digest(field: &'static str, value: &str) -> Result<(), AutonomyContractError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AutonomyContractError::InvalidDigest { field });
    }
    Ok(())
}

fn require_digest_equality(
    field: &'static str,
    expected: &str,
    observed: &str,
) -> Result<(), AutonomyContractError> {
    require_digest(field, expected)?;
    require_digest(field, observed)?;
    if expected != observed {
        return Err(AutonomyContractError::DigestMismatch {
            field,
            expected: expected.to_string(),
            observed: observed.to_string(),
        });
    }
    Ok(())
}

fn require_signature(
    field: &'static str,
    signature: &OpaqueSignature,
) -> Result<(), AutonomyContractError> {
    if signature.is_empty() {
        return Err(AutonomyContractError::EmptySignature { field });
    }
    Ok(())
}

fn require_time_window(
    record: &'static str,
    issued_at: u64,
    expires_at: u64,
    now_ms: u64,
) -> Result<(), AutonomyContractError> {
    if issued_at >= expires_at {
        return Err(AutonomyContractError::InvalidTimeOrder {
            record,
            issued_at,
            expires_at,
        });
    }
    if issued_at > now_ms {
        return Err(AutonomyContractError::IssuedInFuture {
            record,
            issued_at,
            now_ms,
        });
    }
    if now_ms >= expires_at {
        return Err(AutonomyContractError::Expired {
            record,
            expires_at,
            now_ms,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const NOW: u64 = 1_500;
    const ISSUED_AT: u64 = 1_000;
    const EXPIRES_AT: u64 = 3_000;

    fn hash(character: char) -> String {
        character.to_string().repeat(64)
    }

    fn signature(label: &str) -> OpaqueSignature {
        OpaqueSignature::new(label)
    }

    fn kernel_fixture() -> SafetyKernelV1 {
        let mut kernel = SafetyKernelV1 {
            schema: SAFETY_KERNEL_SCHEMA.to_string(),
            core: SafetyKernelCoreV1 {
                kernel_id: "kernel-1".to_string(),
                verifier_binary_digest: hash('a'),
                canonicalization_version: CANONICALIZATION_VERSION.to_string(),
                pinned_external_root_key: "offline-root-key-1".to_string(),
                verified_boot_policy_digest: hash('b'),
                immutable_invariants_digest: hash('c'),
                minimum_verifier_seats: IMMUTABLE_VERIFIER_SEATS,
                minimum_quorum_threshold: IMMUTABLE_QUORUM_THRESHOLD,
                minimum_failure_domains: IMMUTABLE_FAILURE_DOMAINS,
                proposer_executor_nonvoting: true,
                sentinel_required_and_nonvoting: true,
                sentinel_red_absolute_veto: true,
                sentinel_outbox_antirollback_required: true,
                sentinel_identity_key_binary_policy_digest: hash('d'),
                safety_actuator_identity_key_binary_policy_digest: hash('e'),
                required_sentinel_unavailable_fail_closed: true,
                audit_wal_tamper_detection_required: true,
                epoch_freeze_and_rollback_required: true,
                old_runtime_approval_required: true,
                allowed_negative_effects: SafetyKernelV1::canonical_negative_effects(),
            },
            kernel_digest: hash('0'),
            external_root_signature: signature("opaque-external-root"),
        };
        kernel.seal().unwrap();
        kernel
    }

    fn independence_fixture(epoch: u64) -> IndependenceSpecV1 {
        let mut spec = IndependenceSpecV1 {
            schema: INDEPENDENCE_SPEC_SCHEMA.to_string(),
            core: IndependenceSpecCoreV1 {
                constitution_epoch: epoch,
                voting_verifiers: vec![
                    VerifierSeatV1 {
                        principal_id: "verifier-1".to_string(),
                        key_id: "verifier-key-1".to_string(),
                        failure_domain: "provider-a/model-a/runtime-a".to_string(),
                        parent_session_context_digest: hash('1'),
                    },
                    VerifierSeatV1 {
                        principal_id: "verifier-2".to_string(),
                        key_id: "verifier-key-2".to_string(),
                        failure_domain: "provider-b/model-b/runtime-b".to_string(),
                        parent_session_context_digest: hash('2'),
                    },
                    VerifierSeatV1 {
                        principal_id: "verifier-3".to_string(),
                        key_id: "verifier-key-3".to_string(),
                        failure_domain: "provider-c/model-c/runtime-c".to_string(),
                        parent_session_context_digest: hash('3'),
                    },
                    VerifierSeatV1 {
                        principal_id: "verifier-4".to_string(),
                        key_id: "verifier-key-4".to_string(),
                        failure_domain: "provider-c/model-c/runtime-c".to_string(),
                        parent_session_context_digest: hash('4'),
                    },
                ],
                quorum_threshold: IMMUTABLE_QUORUM_THRESHOLD,
                minimum_failure_domains: IMMUTABLE_FAILURE_DOMAINS,
                blind_isolation_policy_digest: hash('5'),
                nonvoting_sentinel_id: "sentinel-1".to_string(),
                proposer_executor_nonvoting: true,
                sentinel_nonvoting: true,
            },
            independence_spec_digest: hash('0'),
        };
        spec.seal().unwrap();
        spec
    }

    fn constitution_fixture(independence: &IndependenceSpecV1) -> ConstitutionStoreV1 {
        let mut constitution = ConstitutionStoreV1 {
            schema: CONSTITUTION_SCHEMA.to_string(),
            core: ConstitutionCoreV1 {
                constitution_epoch: independence.core.constitution_epoch,
                previous_constitution_digest: None,
                effective_at: 500,
                expires_at: 5_000,
                allowed_autonomy_modes: [
                    ActiveMode::HumanGated,
                    ActiveMode::PolicyAutonomous,
                    ActiveMode::FullAutonomy,
                ]
                .into_iter()
                .collect(),
                objectives: ["correctness".to_string()].into_iter().collect(),
                non_goals: ["unbounded authority".to_string()].into_iter().collect(),
                resource_scope_digest: hash('6'),
                risk_budget_action_policy_digest: hash('7'),
                independence_spec_digest: independence.independence_spec_digest.clone(),
                metric_specs_digest: hash('8'),
                canary_requirements_digest: hash('9'),
                rollback_requirements_digest: hash('a'),
                amendment_rules_digest: hash('b'),
                previous_governance_runtime_digest: hash('c'),
                adopting_governance_runtime_digest: hash('d'),
                old_runtime_approval_digest: None,
                issuer_subject_id: "external-bootstrap-root".to_string(),
            },
            constitution_digest: hash('0'),
            signature: signature("opaque-constitution"),
        };
        constitution.seal().unwrap();
        constitution
    }

    fn grant_fixture(
        constitution_epoch: u64,
        autonomy_epoch: u64,
        mode: ActiveMode,
    ) -> AutonomyGrantV1 {
        let mut grant = AutonomyGrantV1 {
            schema: AUTONOMY_GRANT_SCHEMA.to_string(),
            core: AutonomyGrantCoreV1 {
                grant_id: "grant-agent-a".to_string(),
                subject_id: "agent-a".to_string(),
                role_id: "autonomous-lander".to_string(),
                mode,
                max_tier: AutonomyTier::A3AutonomousLand,
                action_classes: ["land".to_string(), "diagnose".to_string()]
                    .into_iter()
                    .collect(),
                risk_domains: [RiskClass::Low, RiskClass::Medium].into_iter().collect(),
                resource_environment_scope_digest: hash('e'),
                budget: BudgetEnvelopeV1 {
                    unit: "work-units".to_string(),
                    limit: 100,
                    consumed: 20,
                    reset_epoch: autonomy_epoch,
                },
                constitution_epoch,
                autonomy_epoch,
                issued_at: ISSUED_AT,
                expires_at: EXPIRES_AT,
                promotion_receipt_id: "promotion-receipt-1".to_string(),
                status: GrantStatus::Active,
            },
            grant_digest: hash('0'),
            owner_signature: signature("opaque-grant"),
        };
        grant.seal().unwrap();
        grant
    }

    fn epoch_fixture(
        constitution: &ConstitutionStoreV1,
        grant: &AutonomyGrantV1,
    ) -> AutonomyEpochV1 {
        AutonomyEpochV1 {
            schema: AUTONOMY_EPOCH_SCHEMA.to_string(),
            autonomy_epoch: grant.core.autonomy_epoch,
            active_mode: grant.core.mode,
            activation_receipt_id: Some("activation-existing".to_string()),
            constitution_digest: constitution.constitution_digest.clone(),
            constitution_epoch: constitution.core.constitution_epoch,
            grants_digest: compute_grants_digest(std::slice::from_ref(grant)).unwrap(),
            issuance_frozen: false,
            safety_state: SafetyState::Healthy,
            protected_root_signature: signature("opaque-protected-root"),
        }
    }

    fn intent_fixture(
        constitution: &ConstitutionStoreV1,
        epoch: &AutonomyEpochV1,
        grant: &AutonomyGrantV1,
        authority: AuthorityVariant,
    ) -> SovereignActionIntentV1 {
        let autonomous = matches!(
            authority,
            AuthorityVariant::Policy | AuthorityVariant::AgentQuorum
        );
        let mut intent = SovereignActionIntentV1 {
            schema: SOVEREIGN_ACTION_INTENT_SCHEMA.to_string(),
            core: SovereignIntentCoreV1 {
                action_class: "land".to_string(),
                semantic_action_id: "mission.service.land".to_string(),
                action_payload_digest: hash('1'),
                issuer_subject_id: if authority == AuthorityVariant::AgentQuorum {
                    "constitutional-council".to_string()
                } else if authority == AuthorityVariant::Human {
                    "owner-human".to_string()
                } else {
                    "policy-engine".to_string()
                },
                decision_subject_id: if authority == AuthorityVariant::Human {
                    "owner-human".to_string()
                } else {
                    grant.core.subject_id.clone()
                },
                caller_subject_id: if authority == AuthorityVariant::Human {
                    "owner-human".to_string()
                } else {
                    grant.core.subject_id.clone()
                },
                audience: "m1nd-owner".to_string(),
                proposer_subject_id: "agent-b".to_string(),
                executor_subject_id: Some("agent-c".to_string()),
                promotion_target_subject_id: None,
                ratification_target_subject_id: None,
                delegation_grant_digest: None,
                required_authority_variant: authority,
                action_policy_registry_digest: hash('2'),
                classifier_decision_digest: hash('3'),
                applicable_grant_id: autonomous.then(|| grant.core.grant_id.clone()),
                applicable_grant_digest: autonomous.then(|| grant.grant_digest.clone()),
                organism_id: "organism-1".to_string(),
                repo_id: "repo-1".to_string(),
                brain_id: "brain-1".to_string(),
                mission_id: Some("mission-1".to_string()),
                mission_head_id: Some("head-1".to_string()),
                block_id: Some("block-1".to_string()),
                candidate_digest: Some(hash('4')),
                promotion_subject_id: None,
                active_mode: epoch.active_mode,
                effective_tier: grant.core.max_tier,
                risk_class: RiskClass::Low,
                risk_scope_digest: hash('5'),
                resource_environment_scope_digest: grant
                    .core
                    .resource_environment_scope_digest
                    .clone(),
                requested_budget: 10,
                constitution_digest: constitution.constitution_digest.clone(),
                constitution_epoch: epoch.constitution_epoch,
                autonomy_epoch: epoch.autonomy_epoch,
                expected_store_epoch: 11,
                expected_store_version: 12,
                expected_boundary_version: 13,
                expected_contract_version: 14,
                metric_spec_digest: hash('6'),
                evidence_digest: hash('7'),
                rollout_plan_digest: hash('8'),
                rollback_plan_digest: hash('9'),
                nonce: "intent-nonce-1".to_string(),
                issued_at: ISSUED_AT,
                expires_at: EXPIRES_AT,
            },
            intent_digest: hash('0'),
            intent_core_ref: IntentCoreRefV1::for_sovereign_digest(hash('0')),
        };
        intent.seal().unwrap();
        intent
    }

    fn sentinel_fixture(
        intent: &SovereignActionIntentV1,
        kernel: &SafetyKernelV1,
        verdict: SentinelVerdict,
    ) -> SentinelVerdictV1 {
        let mut sentinel = SentinelVerdictV1 {
            schema: SENTINEL_VERDICT_SCHEMA.to_string(),
            core: SentinelVerdictCoreV1 {
                verdict_id: "sentinel-verdict-1".to_string(),
                sentinel_identity_key_binary_policy_digest: kernel
                    .core
                    .sentinel_identity_key_binary_policy_digest
                    .clone(),
                intent_digest: intent.intent_digest.clone(),
                intent_core_ref: intent.intent_core_ref.clone(),
                intent_canonicalization_version: CANONICALIZATION_VERSION.to_string(),
                metric_evidence_rollback_digest: hash('a'),
                risk_scope_digest: intent.core.risk_scope_digest.clone(),
                constitution_epoch: intent.core.constitution_epoch,
                autonomy_epoch: intent.core.autonomy_epoch,
                nonce: intent.core.nonce.clone(),
                issued_at: ISSUED_AT + 10,
                expires_at: EXPIRES_AT - 10,
                verdict,
            },
            verdict_digest: hash('0'),
            signature: signature("opaque-sentinel"),
        };
        sentinel.seal().unwrap();
        sentinel
    }

    struct PolicyFixture {
        kernel: SafetyKernelV1,
        independence: IndependenceSpecV1,
        constitution: ConstitutionStoreV1,
        grant: AutonomyGrantV1,
        epoch: AutonomyEpochV1,
        intent: SovereignActionIntentV1,
        sentinel: SentinelVerdictV1,
        decision: AuthorityDecisionV1,
        capability: AutonomyCapabilityV1,
    }

    fn policy_fixture() -> PolicyFixture {
        let kernel = kernel_fixture();
        let independence = independence_fixture(0);
        let constitution = constitution_fixture(&independence);
        let grant = grant_fixture(0, 1, ActiveMode::PolicyAutonomous);
        let epoch = epoch_fixture(&constitution, &grant);
        let intent = intent_fixture(&constitution, &epoch, &grant, AuthorityVariant::Policy);
        let sentinel = sentinel_fixture(&intent, &kernel, SentinelVerdict::Green);
        let binding = AuthorityDecisionBindingV1::from_intent(
            &intent,
            "policy-decision-1".to_string(),
            true,
            Some(sentinel.verdict_digest.clone()),
        );
        let mut decision = AuthorityDecisionV1::Policy(PolicyAuthorityDecisionV1 {
            schema: AUTHORITY_DECISION_SCHEMA.to_string(),
            core: PolicyAuthorityDecisionCoreV1 {
                binding,
                policy_digest: hash('b'),
                matched_clauses_digest: hash('c'),
                risk_budget_scope_digest: hash('d'),
                proof_receipts_digest: hash('e'),
                sentinel_exemption_clause_digest: None,
            },
            decision_digest: hash('0'),
            owner_signature: signature("opaque-policy-decision"),
        });
        decision.seal().unwrap();

        let mut capability = AutonomyCapabilityV1 {
            schema: AUTONOMY_CAPABILITY_SCHEMA.to_string(),
            core: AutonomyCapabilityCoreV1 {
                capability_id: "capability-1".to_string(),
                intent_digest: intent.intent_digest.clone(),
                intent_core_ref: intent.intent_core_ref.clone(),
                intent_canonicalization_version: CANONICALIZATION_VERSION.to_string(),
                decision_digest: decision.decision_digest().to_string(),
                decision_policy_digest: hash('b'),
                required_authority_variant: AuthorityVariant::Policy,
                action_policy_registry_digest: intent.core.action_policy_registry_digest.clone(),
                classifier_decision_digest: intent.core.classifier_decision_digest.clone(),
                constitution_digest: intent.core.constitution_digest.clone(),
                constitution_epoch: intent.core.constitution_epoch,
                autonomy_epoch: intent.core.autonomy_epoch,
                organism_id: intent.core.organism_id.clone(),
                repo_id: intent.core.repo_id.clone(),
                issuer_subject_id: intent.core.issuer_subject_id.clone(),
                decision_subject_id: intent.core.decision_subject_id.clone(),
                caller_subject_id: intent.core.caller_subject_id.clone(),
                proposer_subject_id: intent.core.proposer_subject_id.clone(),
                executor_subject_id: intent.core.executor_subject_id.clone(),
                promotion_target_subject_id: intent.core.promotion_target_subject_id.clone(),
                ratification_target_subject_id: intent.core.ratification_target_subject_id.clone(),
                delegation_grant_digest: intent.core.delegation_grant_digest.clone(),
                audience: intent.core.audience.clone(),
                active_mode: intent.core.active_mode,
                activation_receipt_id: epoch.activation_receipt_id.clone(),
                grant_id: grant.core.grant_id.clone(),
                grant_digest: grant.grant_digest.clone(),
                effective_tier: intent.core.effective_tier,
                action_class: intent.core.action_class.clone(),
                semantic_action_id: intent.core.semantic_action_id.clone(),
                risk_class: intent.core.risk_class,
                risk_scope_digest: intent.core.risk_scope_digest.clone(),
                sentinel_verdict_digest: Some(sentinel.verdict_digest.clone()),
                brain_id: intent.core.brain_id.clone(),
                mission_id: intent.core.mission_id.clone(),
                mission_head_id: intent.core.mission_head_id.clone(),
                block_id: intent.core.block_id.clone(),
                candidate_digest: intent.core.candidate_digest.clone(),
                promotion_subject_id: intent.core.promotion_subject_id.clone(),
                resource_environment_scope_digest: intent
                    .core
                    .resource_environment_scope_digest
                    .clone(),
                requested_budget: intent.core.requested_budget,
                expected_store_epoch: intent.core.expected_store_epoch,
                expected_store_version: intent.core.expected_store_version,
                expected_boundary_version: intent.core.expected_boundary_version,
                expected_contract_version: intent.core.expected_contract_version,
                idempotency_key: "capability-idempotency-1".to_string(), // gitleaks:allow
                payload_digest: intent.core.action_payload_digest.clone(),
                nonce: intent.core.nonce.clone(),
                issued_at: ISSUED_AT + 20,
                expires_at: EXPIRES_AT - 20,
            },
            capability_digest: hash('0'),
            owner_signature: signature("opaque-capability"),
        };
        capability.seal().unwrap();
        PolicyFixture {
            kernel,
            independence,
            constitution,
            grant,
            epoch,
            intent,
            sentinel,
            decision,
            capability,
        }
    }

    fn safety_chain(
        fixture: &PolicyFixture,
    ) -> (
        SentinelVerdictV1,
        SentinelRedOutboxV1,
        RedLatchReceiptV1,
        SafetyActionIntentV1,
        SafetyCapabilityV1,
        AuthorityDecisionV1,
    ) {
        let red = sentinel_fixture(&fixture.intent, &fixture.kernel, SentinelVerdict::Red);
        let mut outbox = SentinelRedOutboxV1 {
            schema: SENTINEL_RED_OUTBOX_SCHEMA.to_string(),
            core: SentinelRedOutboxCoreV1 {
                red_verdict_digest: red.verdict_digest.clone(),
                source_intent_digest: fixture.intent.intent_digest.clone(),
                outbox_epoch: 1,
                previous_outbox_root_digest: None,
                signed_outbox_root_digest: hash('1'),
                protected_latest_outbox_epoch: 1,
                delivery_attempt: 1,
                journal_latch_ack: false,
                actuator_ack: false,
                terminal_safety_transaction_id: None,
                state: RedOutboxState::Pending,
            },
            record_digest: hash('0'),
            root_signature: signature("opaque-outbox-root"),
        };
        outbox.seal().unwrap();

        let effects = SafetyKernelV1::canonical_negative_effects();
        let effects_digest = compute_safety_effects_digest(&effects).unwrap();
        let mut latch = RedLatchReceiptV1 {
            schema: RED_LATCH_RECEIPT_SCHEMA.to_string(),
            core: RedLatchCoreV1 {
                latch_receipt_id: "red-latch-1".to_string(),
                red_verdict_digest: red.verdict_digest.clone(),
                source_intent_digest: fixture.intent.intent_digest.clone(),
                sentinel_outbox_epoch: outbox.core.outbox_epoch,
                sentinel_outbox_root_digest: outbox.core.signed_outbox_root_digest.clone(),
                latched_at: NOW,
                protected_time_evidence_digest: hash('2'),
                constitution_epoch: fixture.intent.core.constitution_epoch,
                autonomy_epoch: fixture.intent.core.autonomy_epoch,
                latch_epoch: 1,
                exact_affected_scope_digest: hash('3'),
                allowed_negative_actions_digest: effects_digest.clone(),
                rollback_candidate_plan_digest: hash('4'),
                immutable_negative_mandate_digest: hash('5'),
                committing_transaction_id: None,
                commit_marker_digest: None,
                terminal_safety_transaction_id: None,
                state: RedLatchState::Pending,
            },
            latch_receipt_digest: hash('0'),
            owner_kernel_signature: signature("opaque-kernel-latch"),
        };
        latch.seal().unwrap();

        let mut safety_intent = SafetyActionIntentV1 {
            schema: SAFETY_ACTION_INTENT_SCHEMA.to_string(),
            core: SafetyActionIntentCoreV1 {
                safety_attempt_id: "safety-attempt-1".to_string(),
                attempt_sequence: 1,
                rebased_from_attempt_digest: None,
                source_intent_digest: fixture.intent.intent_digest.clone(),
                source_intent_core_ref: fixture.intent.intent_core_ref.clone(),
                sentinel_red_verdict_digest: red.verdict_digest.clone(),
                red_latch_receipt_digest: latch.latch_receipt_digest.clone(),
                actuator_identity_key_binary_policy_digest: fixture
                    .kernel
                    .core
                    .safety_actuator_identity_key_binary_policy_digest
                    .clone(),
                expected_constitution_epoch: fixture.intent.core.constitution_epoch,
                expected_autonomy_epoch: fixture.intent.core.autonomy_epoch,
                affected_grants_scope_digest: latch.core.exact_affected_scope_digest.clone(),
                negative_effects: effects.clone(),
                allowed_negative_actions_digest: effects_digest.clone(),
                rollback_candidate_plan_digest: latch.core.rollback_candidate_plan_digest.clone(),
                nonce: "safety-nonce-1".to_string(),
                attempt_idempotency_key: "safety-idempotency-1".to_string(), // gitleaks:allow
                issued_at: NOW,
                valid_while_latch_pending: true,
            },
            safety_intent_digest: hash('0'),
            safety_intent_core_ref: IntentCoreRefV1::for_safety_digest(hash('0')),
        };
        safety_intent.seal().unwrap();

        let mut capability = SafetyCapabilityV1 {
            schema: SAFETY_CAPABILITY_SCHEMA.to_string(),
            core: SafetyCapabilityCoreV1 {
                capability_id: "safety-capability-1".to_string(),
                safety_intent_digest: safety_intent.safety_intent_digest.clone(),
                safety_intent_core_ref: safety_intent.safety_intent_core_ref.clone(),
                safety_attempt_id: safety_intent.core.safety_attempt_id.clone(),
                source_intent_digest: safety_intent.core.source_intent_digest.clone(),
                sentinel_red_verdict_digest: red.verdict_digest.clone(),
                red_latch_receipt_digest: latch.latch_receipt_digest.clone(),
                actuator_identity_key_binary_policy_digest: safety_intent
                    .core
                    .actuator_identity_key_binary_policy_digest
                    .clone(),
                expected_constitution_epoch: safety_intent.core.expected_constitution_epoch,
                expected_autonomy_epoch: safety_intent.core.expected_autonomy_epoch,
                affected_grants_scope_digest: safety_intent
                    .core
                    .affected_grants_scope_digest
                    .clone(),
                negative_effects: effects.clone(),
                allowed_negative_actions_digest: effects_digest,
                rollback_candidate_plan_digest: safety_intent
                    .core
                    .rollback_candidate_plan_digest
                    .clone(),
                nonce: safety_intent.core.nonce.clone(),
                idempotency_key: safety_intent.core.attempt_idempotency_key.clone(),
                issued_at: NOW,
                expires_at: EXPIRES_AT - 1,
            },
            capability_digest: hash('0'),
            actuator_signature: signature("opaque-actuator"),
        };
        capability.seal().unwrap();

        let mut decision = AuthorityDecisionV1::Safety(SafetyAuthorityDecisionV1 {
            schema: AUTHORITY_DECISION_SCHEMA.to_string(),
            core: SafetyAuthorityDecisionCoreV1 {
                decision_id: "safety-decision-1".to_string(),
                safety_intent_digest: safety_intent.safety_intent_digest.clone(),
                safety_intent_core_ref: safety_intent.safety_intent_core_ref.clone(),
                safety_capability_digest: capability.capability_digest.clone(),
                sentinel_red_verdict_digest: red.verdict_digest.clone(),
                red_latch_receipt_digest: latch.latch_receipt_digest.clone(),
                negative_effects: effects,
                positive_authority_decision_forbidden: true,
                issuer_subject_id: "safety-kernel".to_string(),
            },
            decision_digest: hash('0'),
            safety_kernel_signature: signature("opaque-safety-kernel"),
        });
        decision.seal().unwrap();
        (red, outbox, latch, safety_intent, capability, decision)
    }

    fn quorum_decision_fixture(
        fixture: &PolicyFixture,
    ) -> (
        SovereignActionIntentV1,
        SentinelVerdictV1,
        AuthorityDecisionV1,
    ) {
        let intent = intent_fixture(
            &fixture.constitution,
            &fixture.epoch,
            &fixture.grant,
            AuthorityVariant::AgentQuorum,
        );
        let sentinel = sentinel_fixture(&intent, &fixture.kernel, SentinelVerdict::Green);
        let votes = fixture
            .independence
            .core
            .voting_verifiers
            .iter()
            .map(|seat| QuorumVoteV1 {
                verifier_principal_id: seat.principal_id.clone(),
                verifier_key_id: seat.key_id.clone(),
                failure_domain: seat.failure_domain.clone(),
                parent_session_context_digest: seat.parent_session_context_digest.clone(),
                intent_digest: intent.intent_digest.clone(),
                constitution_digest: intent.core.constitution_digest.clone(),
                candidate_digest: intent.core.candidate_digest.clone(),
                evidence_digest: intent.core.evidence_digest.clone(),
                rollout_plan_digest: intent.core.rollout_plan_digest.clone(),
                rollback_plan_digest: intent.core.rollback_plan_digest.clone(),
                disposition: QuorumVoteDisposition::Approve,
                signature: signature("opaque-quorum-vote"),
            })
            .collect();
        let binding = AuthorityDecisionBindingV1::from_intent(
            &intent,
            "quorum-decision-1".to_string(),
            true,
            Some(sentinel.verdict_digest.clone()),
        );
        let mut decision = AuthorityDecisionV1::AgentQuorum(AgentQuorumAuthorityDecisionV1 {
            schema: AUTHORITY_DECISION_SCHEMA.to_string(),
            core: AgentQuorumAuthorityDecisionCoreV1 {
                binding,
                quorum: AgentQuorumDecisionEvidenceV1 {
                    independence_spec: fixture.independence.clone(),
                    votes,
                    sentinel_verdict_digest: sentinel.verdict_digest.clone(),
                },
                evidence_rollout_rollback_digest: hash('f'),
            },
            decision_digest: hash('0'),
            owner_signature: signature("opaque-quorum-decision"),
        });
        decision.seal().unwrap();
        (intent, sentinel, decision)
    }

    #[test]
    fn valid_policy_chain_and_capability_are_structurally_bound_not_authenticated() {
        let fixture = policy_fixture();
        let kernel_validation = fixture.kernel.validate().unwrap();
        assert_eq!(
            kernel_validation.integrity,
            AutonomyIntegrityDisposition::OpaqueSignaturePresentUnverified
        );
        fixture
            .independence
            .validate_against_kernel(&fixture.kernel)
            .unwrap();
        fixture
            .constitution
            .validate(&fixture.independence, &fixture.kernel, NOW)
            .unwrap();
        fixture
            .epoch
            .validate_common(
                &fixture.constitution,
                std::slice::from_ref(&fixture.grant),
                NOW,
            )
            .unwrap();
        fixture
            .intent
            .validate(&fixture.epoch, Some(&fixture.grant), NOW)
            .unwrap();
        fixture
            .sentinel
            .validate_for_intent(&fixture.intent, &fixture.kernel, NOW)
            .unwrap();
        fixture
            .decision
            .validate_positive(
                &fixture.intent,
                &fixture.constitution,
                &fixture.kernel,
                Some(&fixture.sentinel),
                NOW,
            )
            .unwrap();
        let validation = fixture
            .capability
            .validate(
                &fixture.decision,
                &fixture.intent,
                &fixture.grant,
                &fixture.epoch,
                Some(&fixture.sentinel),
                NOW,
            )
            .unwrap();
        assert_eq!(
            validation.integrity,
            AutonomyIntegrityDisposition::OpaqueSignaturePresentUnverified
        );
    }

    #[test]
    fn semantic_action_id_is_exact_and_never_confused_with_grant_action_class() {
        let fixture = policy_fixture();

        let mut class_label_as_action = fixture.intent.clone();
        class_label_as_action.core.semantic_action_id = "land".to_string();
        class_label_as_action.seal().unwrap();
        assert!(matches!(
            class_label_as_action.validate(&fixture.epoch, Some(&fixture.grant), NOW),
            Err(AutonomyContractError::Invariant { .. })
        ));

        let mut different_catalog_action = fixture.capability.clone();
        different_catalog_action.core.semantic_action_id = "mission.service.ratify".to_string();
        different_catalog_action.seal().unwrap();
        assert!(matches!(
            different_catalog_action.validate(
                &fixture.decision,
                &fixture.intent,
                &fixture.grant,
                &fixture.epoch,
                Some(&fixture.sentinel),
                NOW,
            ),
            Err(AutonomyContractError::BindingMismatch {
                field: "capability.semantic_action_id"
            })
        ));
    }

    #[test]
    fn kernel_floors_and_negative_effect_allow_list_are_semantic_not_opaque() {
        let mut weak_quorum = kernel_fixture();
        weak_quorum.core.minimum_quorum_threshold = 2;
        weak_quorum.seal().unwrap();
        assert!(matches!(
            weak_quorum.validate(),
            Err(AutonomyContractError::KernelFloor { .. })
        ));

        let mut permits_land = kernel_fixture();
        permits_land
            .core
            .allowed_negative_effects
            .insert(Effect::GraphMutation);
        permits_land.seal().unwrap();
        assert!(matches!(
            permits_land.validate(),
            Err(AutonomyContractError::SafetyAllowListMismatch)
        ));

        let mut no_red_veto = kernel_fixture();
        no_red_veto.core.sentinel_red_absolute_veto = false;
        no_red_veto.seal().unwrap();
        assert!(matches!(
            no_red_veto.validate(),
            Err(AutonomyContractError::KernelFloor { .. })
        ));
    }

    #[test]
    fn independence_rejects_aliases_common_contexts_and_reduced_domains() {
        let kernel = kernel_fixture();
        let mut aliased = independence_fixture(0);
        aliased.core.voting_verifiers[1].key_id = aliased.core.voting_verifiers[0].key_id.clone();
        aliased.seal().unwrap();
        assert!(matches!(
            aliased.validate_against_kernel(&kernel),
            Err(AutonomyContractError::NonIndependentQuorum)
        ));

        let mut common_context = independence_fixture(0);
        common_context.core.voting_verifiers[2].parent_session_context_digest =
            common_context.core.voting_verifiers[0]
                .parent_session_context_digest
                .clone();
        common_context.seal().unwrap();
        assert!(matches!(
            common_context.validate_against_kernel(&kernel),
            Err(AutonomyContractError::NonIndependentQuorum)
        ));

        let mut two_domains = independence_fixture(0);
        two_domains.core.voting_verifiers[2].failure_domain =
            two_domains.core.voting_verifiers[0].failure_domain.clone();
        two_domains.core.voting_verifiers[3].failure_domain =
            two_domains.core.voting_verifiers[1].failure_domain.clone();
        two_domains.seal().unwrap();
        assert!(matches!(
            two_domains.validate_against_kernel(&kernel),
            Err(AutonomyContractError::NonIndependentQuorum)
        ));
    }

    #[test]
    fn grant_scope_risk_budget_tier_and_expiry_are_all_bound() {
        let fixture = policy_fixture();
        assert!(matches!(
            fixture.grant.authorize_scope(
                "release_promote",
                RiskClass::Low,
                &fixture.grant.core.resource_environment_scope_digest,
                1,
                AutonomyTier::A1Propose,
            ),
            Err(AutonomyContractError::ActionOutsideGrant { .. })
        ));
        assert!(matches!(
            fixture.grant.authorize_scope(
                "land",
                RiskClass::Critical,
                &fixture.grant.core.resource_environment_scope_digest,
                1,
                AutonomyTier::A1Propose,
            ),
            Err(AutonomyContractError::RiskOutsideGrant { .. })
        ));
        assert!(matches!(
            fixture.grant.authorize_scope(
                "land",
                RiskClass::Low,
                &hash('f'),
                1,
                AutonomyTier::A1Propose,
            ),
            Err(AutonomyContractError::ScopeOutsideGrant)
        ));
        assert!(matches!(
            fixture.grant.authorize_scope(
                "land",
                RiskClass::Low,
                &fixture.grant.core.resource_environment_scope_digest,
                81,
                AutonomyTier::A1Propose,
            ),
            Err(AutonomyContractError::BudgetExceeded { .. })
        ));
        assert!(matches!(
            fixture.grant.authorize_scope(
                "land",
                RiskClass::Low,
                &fixture.grant.core.resource_environment_scope_digest,
                1,
                AutonomyTier::A5FullAutonomy,
            ),
            Err(AutonomyContractError::Invariant { .. })
        ));
        assert!(matches!(
            fixture.grant.validate_at(EXPIRES_AT),
            Err(AutonomyContractError::Expired { .. })
        ));
    }

    #[test]
    fn intent_is_predecision_acyclic_and_blocks_self_authorization() {
        let fixture = policy_fixture();
        let core_json = serde_json::to_value(&fixture.intent.core).unwrap();
        SovereignActionIntentV1::reject_cyclic_json_fields(&core_json).unwrap();

        let mut cyclic = core_json.clone();
        cyclic
            .as_object_mut()
            .unwrap()
            .insert("sentinel_verdict_digest".to_string(), json!(hash('a')));
        assert!(matches!(
            SovereignActionIntentV1::reject_cyclic_json_fields(&cyclic),
            Err(AutonomyContractError::CyclicIntentField { .. })
        ));
        assert!(serde_json::from_value::<SovereignIntentCoreV1>(cyclic).is_err());

        let mut self_promote = fixture.intent.clone();
        self_promote.core.promotion_target_subject_id =
            Some(self_promote.core.decision_subject_id.clone());
        self_promote.seal().unwrap();
        assert!(matches!(
            self_promote.validate(&fixture.epoch, Some(&fixture.grant), NOW),
            Err(AutonomyContractError::SelfAuthorization { .. })
        ));

        let mut self_ratify = fixture.intent.clone();
        self_ratify.core.action_class = "ratify".to_string();
        self_ratify.core.ratification_target_subject_id =
            self_ratify.core.executor_subject_id.clone();
        self_ratify.seal().unwrap();
        assert!(matches!(
            self_ratify.validate(&fixture.epoch, Some(&fixture.grant), NOW),
            Err(AutonomyContractError::SelfAuthorization { .. })
                | Err(AutonomyContractError::ActionOutsideGrant { .. })
        ));

        let mut undelegated = fixture.intent.clone();
        undelegated.core.caller_subject_id = "different-caller".to_string();
        undelegated.core.delegation_grant_digest = None;
        undelegated.seal().unwrap();
        assert!(matches!(
            undelegated.validate(&fixture.epoch, Some(&fixture.grant), NOW),
            Err(AutonomyContractError::Invariant { .. })
        ));

        let mut post_verdict_mutation = fixture.intent.clone();
        post_verdict_mutation.core.requested_budget += 1;
        assert!(matches!(
            post_verdict_mutation.validate(&fixture.epoch, Some(&fixture.grant), NOW),
            Err(AutonomyContractError::DigestMismatch { .. })
        ));
    }

    #[test]
    fn human_gated_bootstrap_is_the_only_authoritative_bootstrap_shape() {
        let kernel = kernel_fixture();
        let independence = independence_fixture(0);
        let constitution = constitution_fixture(&independence);
        constitution.validate(&independence, &kernel, NOW).unwrap();
        let grants = Vec::new();
        let mut bootstrap = AutonomyEpochV1 {
            schema: AUTONOMY_EPOCH_SCHEMA.to_string(),
            autonomy_epoch: 0,
            active_mode: ActiveMode::HumanGated,
            activation_receipt_id: None,
            constitution_digest: constitution.constitution_digest.clone(),
            constitution_epoch: 0,
            grants_digest: compute_grants_digest(&grants).unwrap(),
            issuance_frozen: false,
            safety_state: SafetyState::Healthy,
            protected_root_signature: signature("opaque-bootstrap-root"),
        };
        bootstrap
            .validate_bootstrap(&constitution, &grants, NOW)
            .unwrap();

        bootstrap.active_mode = ActiveMode::FullAutonomy;
        assert!(bootstrap
            .validate_bootstrap(&constitution, &grants, NOW)
            .is_err());
        bootstrap.active_mode = ActiveMode::HumanGated;
        bootstrap.activation_receipt_id = Some("invented-activation".to_string());
        assert!(bootstrap
            .validate_bootstrap(&constitution, &grants, NOW)
            .is_err());
    }

    #[test]
    fn red_outbox_latch_safety_intent_capability_and_decision_bind_end_to_end() {
        let fixture = policy_fixture();
        let (red, outbox, latch, safety_intent, capability, decision) = safety_chain(&fixture);
        red.validate_for_intent(&fixture.intent, &fixture.kernel, NOW)
            .unwrap();
        outbox.validate(&red, &fixture.intent, None).unwrap();
        latch
            .validate(&outbox, &red, &fixture.intent, &fixture.kernel)
            .unwrap();
        safety_intent
            .validate(
                &fixture.intent,
                &red,
                &latch,
                &fixture.kernel,
                fixture.epoch.constitution_epoch,
                fixture.epoch.autonomy_epoch,
            )
            .unwrap();
        capability
            .validate(&safety_intent, &latch, &fixture.kernel, NOW)
            .unwrap();
        decision
            .validate_safety(&safety_intent, &capability, &latch, &fixture.kernel)
            .unwrap();
        assert!(matches!(
            decision.validate_positive(
                &fixture.intent,
                &fixture.constitution,
                &fixture.kernel,
                Some(&red),
                NOW,
            ),
            Err(AutonomyContractError::Invariant { .. })
        ));
        assert!(matches!(
            fixture
                .decision
                .validate_safety(&safety_intent, &capability, &latch, &fixture.kernel,),
            Err(AutonomyContractError::Invariant { .. })
        ));
    }

    #[test]
    fn safety_lane_rejects_positive_effect_scope_expansion_and_stale_epoch() {
        let fixture = policy_fixture();
        let (red, _outbox, latch, safety_intent, _capability, _decision) = safety_chain(&fixture);

        let mut positive = safety_intent.clone();
        positive.core.negative_effects.insert(Effect::GraphMutation);
        positive.seal().unwrap();
        assert!(matches!(
            positive.validate(
                &fixture.intent,
                &red,
                &latch,
                &fixture.kernel,
                fixture.epoch.constitution_epoch,
                fixture.epoch.autonomy_epoch,
            ),
            Err(AutonomyContractError::PositiveEffectInSafety { .. })
                | Err(AutonomyContractError::SafetyAllowListMismatch)
        ));

        let mut wider = safety_intent.clone();
        wider.core.affected_grants_scope_digest = hash('f');
        wider.seal().unwrap();
        assert!(matches!(
            wider.validate(
                &fixture.intent,
                &red,
                &latch,
                &fixture.kernel,
                fixture.epoch.constitution_epoch,
                fixture.epoch.autonomy_epoch,
            ),
            Err(AutonomyContractError::DigestMismatch { .. })
        ));

        assert!(matches!(
            safety_intent.validate(
                &fixture.intent,
                &red,
                &latch,
                &fixture.kernel,
                fixture.epoch.constitution_epoch,
                fixture.epoch.autonomy_epoch + 1,
            ),
            Err(AutonomyContractError::BindingMismatch { .. })
        ));
    }

    #[test]
    fn safety_retry_changes_only_attempt_freshness_and_current_autonomy_epoch() {
        let fixture = policy_fixture();
        let (_red, _outbox, _latch, first, _capability, _decision) = safety_chain(&fixture);
        let mut retry = first.clone();
        retry.core.safety_attempt_id = "safety-attempt-2".to_string();
        retry.core.attempt_sequence = 2;
        retry.core.rebased_from_attempt_digest = Some(first.safety_intent_digest.clone());
        retry.core.nonce = "safety-nonce-2".to_string();
        retry.core.attempt_idempotency_key = "safety-idempotency-2".to_string(); // gitleaks:allow
        retry.core.expected_autonomy_epoch += 1;
        retry.core.issued_at += 10;
        retry.seal().unwrap();
        retry.validate_retry_authority_unchanged(&first).unwrap();

        retry.core.affected_grants_scope_digest = hash('f');
        retry.seal().unwrap();
        assert!(matches!(
            retry.validate_retry_authority_unchanged(&first),
            Err(AutonomyContractError::BindingMismatch { .. })
        ));
    }

    #[test]
    fn red_outbox_and_latch_state_machines_are_fail_closed() {
        let fixture = policy_fixture();
        let (red, mut outbox, mut latch, _safety, _capability, _decision) = safety_chain(&fixture);
        outbox.core.state = RedOutboxState::Terminal;
        outbox.seal().unwrap();
        assert!(matches!(
            outbox.validate(&red, &fixture.intent, None),
            Err(AutonomyContractError::Invariant { .. })
        ));

        latch.core.state = RedLatchState::Committing;
        latch.core.committing_transaction_id = Some("safety-tx-1".to_string());
        latch.core.commit_marker_digest = None;
        latch.seal().unwrap();
        let valid_outbox = safety_chain(&fixture).1;
        assert!(matches!(
            latch.validate(&valid_outbox, &red, &fixture.intent, &fixture.kernel),
            Err(AutonomyContractError::Invariant { .. })
        ));
    }

    #[test]
    fn quorum_rejects_dissent_role_overlap_and_vote_binding_drift() {
        let fixture = policy_fixture();
        let (intent, sentinel, decision) = quorum_decision_fixture(&fixture);
        decision
            .validate_positive(
                &intent,
                &fixture.constitution,
                &fixture.kernel,
                Some(&sentinel),
                NOW,
            )
            .unwrap();

        let mut dissent = decision.clone();
        let AuthorityDecisionV1::AgentQuorum(dissent) = &mut dissent else {
            unreachable!()
        };
        dissent.core.quorum.votes[0].disposition = QuorumVoteDisposition::Dissent;
        let mut dissent = AuthorityDecisionV1::AgentQuorum(dissent.clone());
        dissent.seal().unwrap();
        assert!(matches!(
            dissent.validate_positive(
                &intent,
                &fixture.constitution,
                &fixture.kernel,
                Some(&sentinel),
                NOW,
            ),
            Err(AutonomyContractError::QuorumNotUnanimouslyResolvable)
        ));

        let mut role_overlap = intent.clone();
        role_overlap.core.proposer_subject_id = "verifier-1".to_string();
        role_overlap.seal().unwrap();
        let overlap_sentinel =
            sentinel_fixture(&role_overlap, &fixture.kernel, SentinelVerdict::Green);
        let mut overlap_decision = decision.clone();
        let AuthorityDecisionV1::AgentQuorum(overlap) = &mut overlap_decision else {
            unreachable!()
        };
        overlap.core.binding = AuthorityDecisionBindingV1::from_intent(
            &role_overlap,
            "quorum-role-overlap".to_string(),
            true,
            Some(overlap_sentinel.verdict_digest.clone()),
        );
        overlap.core.quorum.sentinel_verdict_digest = overlap_sentinel.verdict_digest.clone();
        for vote in &mut overlap.core.quorum.votes {
            vote.intent_digest = role_overlap.intent_digest.clone();
        }
        overlap_decision.seal().unwrap();
        assert!(matches!(
            overlap_decision.validate_positive(
                &role_overlap,
                &fixture.constitution,
                &fixture.kernel,
                Some(&overlap_sentinel),
                NOW,
            ),
            Err(AutonomyContractError::SelfAuthorization { .. })
        ));

        let mut changed_vote = decision;
        let AuthorityDecisionV1::AgentQuorum(changed_vote) = &mut changed_vote else {
            unreachable!()
        };
        changed_vote.core.quorum.votes[1].rollback_plan_digest = hash('f');
        let mut changed_vote = AuthorityDecisionV1::AgentQuorum(changed_vote.clone());
        changed_vote.seal().unwrap();
        assert!(matches!(
            changed_vote.validate_positive(
                &intent,
                &fixture.constitution,
                &fixture.kernel,
                Some(&sentinel),
                NOW,
            ),
            Err(AutonomyContractError::DigestMismatch { .. })
        ));
    }

    #[test]
    fn quorum_is_true_three_of_four_with_absence_but_submitted_non_approval_vetoes() {
        let fixture = policy_fixture();
        let (intent, sentinel, decision) = quorum_decision_fixture(&fixture);

        let mut one_absent = decision.clone();
        let AuthorityDecisionV1::AgentQuorum(one_absent_quorum) = &mut one_absent else {
            unreachable!()
        };
        one_absent_quorum.core.quorum.votes.pop();
        one_absent.seal().unwrap();
        one_absent
            .validate_positive(
                &intent,
                &fixture.constitution,
                &fixture.kernel,
                Some(&sentinel),
                NOW,
            )
            .unwrap();

        for disposition in [
            QuorumVoteDisposition::Dissent,
            QuorumVoteDisposition::Abstain,
        ] {
            let mut vetoed = decision.clone();
            let AuthorityDecisionV1::AgentQuorum(vetoed_quorum) = &mut vetoed else {
                unreachable!()
            };
            vetoed_quorum.core.quorum.votes[3].disposition = disposition;
            vetoed.seal().unwrap();
            assert!(matches!(
                vetoed.validate_positive(
                    &intent,
                    &fixture.constitution,
                    &fixture.kernel,
                    Some(&sentinel),
                    NOW,
                ),
                Err(AutonomyContractError::QuorumNotUnanimouslyResolvable)
            ));
        }

        let mut two_approvals = decision.clone();
        let AuthorityDecisionV1::AgentQuorum(two_approval_quorum) = &mut two_approvals else {
            unreachable!()
        };
        two_approval_quorum.core.quorum.votes.truncate(2);
        two_approvals.seal().unwrap();
        assert!(matches!(
            two_approvals.validate_positive(
                &intent,
                &fixture.constitution,
                &fixture.kernel,
                Some(&sentinel),
                NOW,
            ),
            Err(AutonomyContractError::InsufficientQuorum {
                approvals: 2,
                required: 3,
            })
        ));

        let mut wrong_membership = decision.clone();
        let AuthorityDecisionV1::AgentQuorum(wrong_membership_quorum) = &mut wrong_membership
        else {
            unreachable!()
        };
        wrong_membership_quorum
            .core
            .quorum
            .independence_spec
            .core
            .voting_verifiers
            .pop();
        wrong_membership_quorum
            .core
            .quorum
            .independence_spec
            .seal()
            .unwrap();
        wrong_membership.seal().unwrap();
        assert!(matches!(
            wrong_membership.validate_positive(
                &intent,
                &fixture.constitution,
                &fixture.kernel,
                Some(&sentinel),
                NOW,
            ),
            Err(AutonomyContractError::KernelFloor { .. })
        ));

        let mut two_approval_domains = decision;
        let AuthorityDecisionV1::AgentQuorum(two_domain_quorum) = &mut two_approval_domains else {
            unreachable!()
        };
        two_domain_quorum.core.quorum.votes.remove(1);
        two_approval_domains.seal().unwrap();
        assert!(matches!(
            two_approval_domains.validate_positive(
                &intent,
                &fixture.constitution,
                &fixture.kernel,
                Some(&sentinel),
                NOW,
            ),
            Err(AutonomyContractError::NonIndependentQuorum)
        ));
    }

    #[test]
    fn capability_refuses_subject_scope_epoch_budget_and_decision_variant_drift() {
        let fixture = policy_fixture();
        let mut changed_subject = fixture.capability.clone();
        changed_subject.core.decision_subject_id = "agent-other".to_string();
        changed_subject.seal().unwrap();
        assert!(matches!(
            changed_subject.validate(
                &fixture.decision,
                &fixture.intent,
                &fixture.grant,
                &fixture.epoch,
                Some(&fixture.sentinel),
                NOW,
            ),
            Err(AutonomyContractError::BindingMismatch { .. })
        ));

        let mut changed_scope = fixture.capability.clone();
        changed_scope.core.resource_environment_scope_digest = hash('f');
        changed_scope.seal().unwrap();
        assert!(changed_scope
            .validate(
                &fixture.decision,
                &fixture.intent,
                &fixture.grant,
                &fixture.epoch,
                Some(&fixture.sentinel),
                NOW,
            )
            .is_err());

        let mut changed_epoch = fixture.capability.clone();
        changed_epoch.core.autonomy_epoch += 1;
        changed_epoch.seal().unwrap();
        assert!(changed_epoch
            .validate(
                &fixture.decision,
                &fixture.intent,
                &fixture.grant,
                &fixture.epoch,
                Some(&fixture.sentinel),
                NOW,
            )
            .is_err());

        let mut changed_budget = fixture.capability.clone();
        changed_budget.core.requested_budget += 1;
        changed_budget.seal().unwrap();
        assert!(changed_budget
            .validate(
                &fixture.decision,
                &fixture.intent,
                &fixture.grant,
                &fixture.epoch,
                Some(&fixture.sentinel),
                NOW,
            )
            .is_err());

        let safety = safety_chain(&fixture).5;
        assert!(matches!(
            fixture.capability.validate(
                &safety,
                &fixture.intent,
                &fixture.grant,
                &fixture.epoch,
                Some(&fixture.sentinel),
                NOW,
            ),
            Err(AutonomyContractError::Invariant { .. })
        ));
    }

    #[test]
    fn constitution_amendment_is_old_runtime_authorized_and_preserves_kernel_floors() {
        let kernel = kernel_fixture();
        let previous_independence = independence_fixture(0);
        let previous = constitution_fixture(&previous_independence);
        let proposed_independence = independence_fixture(1);
        let mut proposed = ConstitutionStoreV1 {
            schema: CONSTITUTION_SCHEMA.to_string(),
            core: ConstitutionCoreV1 {
                constitution_epoch: 1,
                previous_constitution_digest: Some(previous.constitution_digest.clone()),
                effective_at: 2_000,
                expires_at: 6_000,
                allowed_autonomy_modes: previous.core.allowed_autonomy_modes.clone(),
                objectives: previous.core.objectives.clone(),
                non_goals: previous.core.non_goals.clone(),
                resource_scope_digest: previous.core.resource_scope_digest.clone(),
                risk_budget_action_policy_digest: previous
                    .core
                    .risk_budget_action_policy_digest
                    .clone(),
                independence_spec_digest: proposed_independence.independence_spec_digest.clone(),
                metric_specs_digest: previous.core.metric_specs_digest.clone(),
                canary_requirements_digest: previous.core.canary_requirements_digest.clone(),
                rollback_requirements_digest: previous.core.rollback_requirements_digest.clone(),
                amendment_rules_digest: previous.core.amendment_rules_digest.clone(),
                previous_governance_runtime_digest: hash('1'),
                adopting_governance_runtime_digest: hash('2'),
                old_runtime_approval_digest: Some(hash('3')),
                issuer_subject_id: "previous-runtime-governor".to_string(),
            },
            constitution_digest: hash('0'),
            signature: signature("opaque-proposed-constitution"),
        };
        proposed.seal().unwrap();
        let mut amendment = ConstitutionAmendmentV1 {
            schema: CONSTITUTION_AMENDMENT_SCHEMA.to_string(),
            core: ConstitutionAmendmentCoreV1 {
                amendment_id: "amendment-1".to_string(),
                previous_constitution_digest: previous.constitution_digest.clone(),
                previous_constitution_epoch: 0,
                proposed_constitution_digest: proposed.constitution_digest.clone(),
                proposed_constitution_epoch: 1,
                proposer_subject_id: "agent-proposer".to_string(),
                proposed_runtime_subject_id: "new-governance-runtime".to_string(),
                approval_issuer_subject_id: "old-governance-runtime".to_string(),
                previous_runtime_digest: hash('1'),
                proposed_runtime_digest: hash('2'),
                old_runtime_approval_digest: hash('3'),
                authority_decision_digest: hash('4'),
                prepare_receipt_digest: hash('5'),
                canary_receipts_digest: hash('6'),
                rollback_plan_digest: hash('7'),
                prepared_at: 1_500,
                activates_at: 2_000,
            },
            amendment_digest: hash('0'),
            old_runtime_signature: signature("opaque-old-runtime"),
        };
        amendment.seal().unwrap();
        amendment
            .validate_transition(&previous, &proposed, &proposed_independence, &kernel, 2_500)
            .unwrap();

        let mut self_adoption = amendment.clone();
        self_adoption.core.approval_issuer_subject_id =
            self_adoption.core.proposed_runtime_subject_id.clone();
        self_adoption.seal().unwrap();
        assert!(matches!(
            self_adoption.validate_transition(
                &previous,
                &proposed,
                &proposed_independence,
                &kernel,
                2_500,
            ),
            Err(AutonomyContractError::SelfAuthorization { .. })
        ));

        let mut weak_independence = proposed_independence.clone();
        weak_independence.core.quorum_threshold = 2;
        weak_independence.seal().unwrap();
        let mut weak_proposed = proposed.clone();
        weak_proposed.core.independence_spec_digest =
            weak_independence.independence_spec_digest.clone();
        weak_proposed.seal().unwrap();
        let mut weak_amendment = amendment.clone();
        weak_amendment.core.proposed_constitution_digest =
            weak_proposed.constitution_digest.clone();
        weak_amendment.seal().unwrap();
        assert!(matches!(
            weak_amendment.validate_transition(
                &previous,
                &weak_proposed,
                &weak_independence,
                &kernel,
                2_500,
            ),
            Err(AutonomyContractError::KernelFloor { .. })
        ));
    }

    fn human_decision_for_activation(intent: &SovereignActionIntentV1) -> AuthorityDecisionV1 {
        let mut binding = AuthorityDecisionBindingV1::from_intent(
            intent,
            "human-activation-decision".to_string(),
            false,
            None,
        );
        binding.required_authority_variant = AuthorityVariant::Human;
        binding.grant_id = None;
        binding.effective_tier = None;
        let mut decision = AuthorityDecisionV1::Human(HumanAuthorityDecisionV1 {
            schema: AUTHORITY_DECISION_SCHEMA.to_string(),
            core: HumanAuthorityDecisionCoreV1 {
                binding,
                human_approval_digest: hash('1'),
                human_decision_digest: hash('2'),
                human_key_id: "human-key-1".to_string(),
            },
            decision_digest: hash('0'),
            owner_signature: signature("opaque-owner-human-decision"),
        });
        decision.seal().unwrap();
        decision
    }

    #[test]
    fn activation_requires_previous_mode_epoch_authority_exact_release_and_one_step() {
        let fixture = policy_fixture();
        let previous = AutonomyEpochV1 {
            schema: AUTONOMY_EPOCH_SCHEMA.to_string(),
            autonomy_epoch: 0,
            active_mode: ActiveMode::HumanGated,
            activation_receipt_id: None,
            constitution_digest: fixture.constitution.constitution_digest.clone(),
            constitution_epoch: fixture.constitution.core.constitution_epoch,
            grants_digest: compute_grants_digest(&[]).unwrap(),
            issuance_frozen: false,
            safety_state: SafetyState::Healthy,
            protected_root_signature: signature("opaque-previous-root"),
        };
        let authority = human_decision_for_activation(&fixture.intent);
        let candidate = hash('3');
        let mut receipt = AutonomyActivationReceiptV1 {
            schema: AUTONOMY_ACTIVATION_RECEIPT_SCHEMA.to_string(),
            core: AutonomyActivationReceiptCoreV1 {
                receipt_id: String::new(),
                previous_mode_epoch_digest: compute_autonomy_epoch_reference_digest(&previous)
                    .unwrap(),
                previous_mode: ActiveMode::HumanGated,
                previous_constitution_epoch: previous.constitution_epoch,
                previous_autonomy_epoch: previous.autonomy_epoch,
                previous_activation_receipt_id: None,
                target_constitution_digest: fixture.constitution.constitution_digest.clone(),
                target_constitution_epoch: fixture.constitution.core.constitution_epoch,
                activated_autonomy_epoch: 1,
                activated_mode: ActiveMode::PolicyAutonomous,
                grants_digest: compute_grants_digest(std::slice::from_ref(&fixture.grant)).unwrap(),
                release_candidate_digest: candidate.clone(),
                gate_receipts_digest: hash('4'),
                g9_canary_receipts_digest: hash('5'),
                authority_decision_digest: authority.decision_digest().to_string(),
                prior_authority_variant: AuthorityVariant::Human,
                custody_floor: crate::SECURE_ENCLAVE_CUSTODY_FLOOR_V1.to_owned(),
                rollback_plan_digest: hash('6'),
                activates_at: NOW,
                issuer_subject_id: "owner-human".to_string(),
            },
            receipt_digest: hash('0'),
            signature: signature("opaque-activation"),
        };
        receipt.seal().unwrap();
        let mut target = fixture.epoch.clone();
        target.activation_receipt_id = Some(receipt.core.receipt_id.clone());
        receipt
            .validate_transition(
                &previous,
                &target,
                &fixture.constitution,
                std::slice::from_ref(&fixture.grant),
                AutonomyActivationValidationContext {
                    exact_release_candidate_digest: &candidate,
                    authority_decision: &authority,
                    now_ms: NOW,
                },
            )
            .unwrap();

        // Custody floor must be a member of the closed ratified set: a smuggled
        // "software" floor is refused up front by validate_transition.
        let mut smuggled = receipt.clone();
        smuggled.core.custody_floor = "software".to_owned();
        assert!(matches!(
            smuggled.validate_transition(
                &previous,
                &target,
                &fixture.constitution,
                std::slice::from_ref(&fixture.grant),
                AutonomyActivationValidationContext {
                    exact_release_candidate_digest: &candidate,
                    authority_decision: &authority,
                    now_ms: NOW,
                },
            ),
            Err(AutonomyContractError::Invariant { .. })
        ));

        assert!(matches!(
            receipt.validate_transition(
                &previous,
                &target,
                &fixture.constitution,
                std::slice::from_ref(&fixture.grant),
                AutonomyActivationValidationContext {
                    exact_release_candidate_digest: &hash('7'),
                    authority_decision: &authority,
                    now_ms: NOW,
                },
            ),
            Err(AutonomyContractError::DigestMismatch { .. })
        ));

        let mut skip_to_full = target.clone();
        skip_to_full.active_mode = ActiveMode::FullAutonomy;
        let mut skip_receipt = receipt.clone();
        skip_receipt.core.activated_mode = ActiveMode::FullAutonomy;
        skip_receipt.seal().unwrap();
        skip_to_full.activation_receipt_id = Some(skip_receipt.core.receipt_id.clone());
        assert!(matches!(
            skip_receipt.validate_transition(
                &previous,
                &skip_to_full,
                &fixture.constitution,
                std::slice::from_ref(&fixture.grant),
                AutonomyActivationValidationContext {
                    exact_release_candidate_digest: &candidate,
                    authority_decision: &authority,
                    now_ms: NOW,
                },
            ),
            Err(AutonomyContractError::InvalidActivationTransition { .. })
        ));

        let mut wrong_authority = authority.clone();
        let AuthorityDecisionV1::Human(human) = wrong_authority else {
            unreachable!()
        };
        wrong_authority = AuthorityDecisionV1::Policy(PolicyAuthorityDecisionV1 {
            schema: AUTHORITY_DECISION_SCHEMA.to_string(),
            core: PolicyAuthorityDecisionCoreV1 {
                binding: human.core.binding,
                policy_digest: hash('1'),
                matched_clauses_digest: hash('2'),
                risk_budget_scope_digest: hash('3'),
                proof_receipts_digest: hash('4'),
                sentinel_exemption_clause_digest: Some(hash('5')),
            },
            decision_digest: receipt.core.authority_decision_digest.clone(),
            owner_signature: signature("opaque-wrong-authority"),
        });
        assert!(matches!(
            receipt.validate_transition(
                &previous,
                &target,
                &fixture.constitution,
                std::slice::from_ref(&fixture.grant),
                AutonomyActivationValidationContext {
                    exact_release_candidate_digest: &candidate,
                    authority_decision: &wrong_authority,
                    now_ms: NOW,
                },
            ),
            Err(AutonomyContractError::AuthorityMismatch { .. })
                | Err(AutonomyContractError::DigestMismatch { .. })
        ));
    }

    #[test]
    fn every_top_level_contract_denies_unknown_fields() {
        let fixture = policy_fixture();
        let mut wire = serde_json::to_value(&fixture.capability).unwrap();
        wire.as_object_mut()
            .unwrap()
            .insert("unexpected".to_string(), json!(true));
        assert!(serde_json::from_value::<AutonomyCapabilityV1>(wire).is_err());

        let mut decision_wire = serde_json::to_value(&fixture.decision).unwrap();
        decision_wire
            .as_object_mut()
            .unwrap()
            .insert("second_authority".to_string(), json!("HUMAN"));
        assert!(serde_json::from_value::<AuthorityDecisionV1>(decision_wire).is_err());

        let mut intent_wire = serde_json::to_value(&fixture.intent).unwrap();
        intent_wire
            .as_object_mut()
            .unwrap()
            .insert("transaction_digest".to_string(), json!(hash('a')));
        assert!(serde_json::from_value::<SovereignActionIntentV1>(intent_wire).is_err());
    }
}
