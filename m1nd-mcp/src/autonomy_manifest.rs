//! Read-only projection of the protected G9 autonomy authority into G1.
//!
//! The protected runtime remains the sole owner of active mode, epochs, grants,
//! issuance fences, and safety state.  This module only takes a validated,
//! committed point-in-time copy and converts it into manifest facts.  It cannot
//! activate a mode, mint a capability, or treat mechanical support as authority.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use m1nd_control::autonomy::{
    compute_autonomy_epoch_reference_digest, AuthorityDecisionV1, AutonomyCapabilityV1,
    SafetyState, SentinelVerdictV1,
};
use m1nd_control::autonomy_runtime::{
    AutonomyAdmissionReceiptV1, AutonomyArtifactVerifier, AutonomyRuntimeAssurance,
    AutonomyRuntimeError, AutonomyRuntimeStateV1, AutonomyRuntimeStore, ProtectedAutonomyPhaseV1,
    ProtectedAutonomyRootBackend, ProtectedAutonomyRootV1,
};
use m1nd_control::{
    ActionId, ActiveMode, AuthorityCapabilityV1, AuthorityFact, AuthorityFreshness,
    AuthorityStatus, AuthorityVariant, AutonomyFact, AutonomyTier, OpaqueSignature,
    AUTHORITY_CAPABILITY_SCHEMA,
};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt;

pub const AUTONOMY_MANIFEST_PROJECTION_SCHEMA: &str = "m1nd-autonomy-manifest-projection-v1";
pub const AUTHORITY_JOURNAL_ID: &str = "authority_journal";
pub const AUTONOMY_EPOCH_AUTHORITY_ID: &str = "autonomy_epoch";
pub const CONSTITUTION_AUTHORITY_ID: &str = "constitution";
pub const INTENT_CORE_STORE_AUTHORITY_ID: &str = "intent_core_store";
pub const SENTINEL_OUTBOX_AUTHORITY_ID: &str = "sentinel_outbox";

/// Complete constitutional evidence required before an autonomous positive
/// capability may enter the ordinary G2 authority transaction.  A free-form
/// digest is intentionally insufficient: the protected G9 owner must validate
/// and consume these exact objects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutonomyAuthorityEvidenceV1 {
    pub intent_digest: String,
    pub decision: AuthorityDecisionV1,
    pub capability: AutonomyCapabilityV1,
    pub sentinel: Option<SentinelVerdictV1>,
}

/// Point-in-time result returned while the protected G9 owner lock is still
/// held.  The receipt proves one-shot admission; the projection binds G2 and
/// the manifest to the post-consumption protected state/root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutonomyAdmissionOutcomeV1 {
    pub receipt: AutonomyAdmissionReceiptV1,
    pub projection: AutonomyManifestProjectionV1,
}

/// Object-safe positive-authority seam installed into the owner transport.
/// No implementation means autonomous positive authority is unavailable.
pub trait AutonomyAdmissionOwner: AutonomyManifestReader + Send + Sync {
    fn assurance(&self) -> AutonomyRuntimeAssurance;

    fn admit(
        &self,
        evidence: &AutonomyAuthorityEvidenceV1,
        now_ms: u64,
    ) -> Result<AutonomyAdmissionOutcomeV1, AutonomyManifestProjectionError>;
}

/// Owner-observed G2 bindings that the constitutional artifacts must match
/// exactly before the G9 capability is consumed.
pub struct ExpectedAutonomyAuthorityBindingV1<'a> {
    pub generic_capability: &'a AuthorityCapabilityV1,
    pub target_action: &'a str,
    pub payload_digest: &'a str,
    pub subject_id: &'a str,
    pub organism_id: &'a str,
    pub repo_id: &'a str,
    pub brain_id: &'a str,
    pub mission_id: Option<&'a str>,
    pub mission_head_id: Option<&'a str>,
    pub authority_decision_digest: &'a str,
    pub applicable_grant_id: Option<&'a str>,
    pub applicable_tier: Option<AutonomyTier>,
}

/// Cross-contract equality gate between the generic cryptographic G2 envelope
/// and the constitutional G9 records.  This is deliberately stricter than a
/// label/digest check: every shared identity, action, scope, epoch-adjacent and
/// replay binding must name the same operation.
pub fn validate_autonomy_authority_binding(
    evidence: &AutonomyAuthorityEvidenceV1,
    expected: ExpectedAutonomyAuthorityBindingV1<'_>,
) -> Result<(), AutonomyManifestProjectionError> {
    let generic = expected.generic_capability;
    let constitutional = &evidence.capability.core;
    let variant = evidence.decision.authority_variant();
    if !matches!(
        variant,
        AuthorityVariant::Policy | AuthorityVariant::AgentQuorum
    ) {
        return Err(AutonomyManifestProjectionError::AuthorityBindingMismatch {
            field: "authority_variant",
        });
    }
    for (field, expected_value, observed) in [
        (
            "intent_digest",
            evidence.intent_digest.as_str(),
            constitutional.intent_digest.as_str(),
        ),
        (
            "decision_digest",
            evidence.decision.decision_digest(),
            constitutional.decision_digest.as_str(),
        ),
        (
            "wire_authority_decision_digest",
            expected.authority_decision_digest,
            evidence.decision.decision_digest(),
        ),
        (
            "capability_id",
            generic.capability_id.as_str(),
            constitutional.capability_id.as_str(),
        ),
        (
            "issuer_subject_id",
            generic.issuer_subject_id.as_str(),
            constitutional.issuer_subject_id.as_str(),
        ),
        (
            "caller_subject_id",
            expected.subject_id,
            constitutional.caller_subject_id.as_str(),
        ),
        (
            "generic_subject_id",
            expected.subject_id,
            generic.subject_id.as_str(),
        ),
        (
            "audience",
            generic.audience.as_str(),
            constitutional.audience.as_str(),
        ),
        (
            "organism_id",
            expected.organism_id,
            constitutional.organism_id.as_str(),
        ),
        ("repo_id", expected.repo_id, constitutional.repo_id.as_str()),
        (
            "brain_id",
            expected.brain_id,
            constitutional.brain_id.as_str(),
        ),
        (
            "action",
            expected.target_action,
            constitutional.semantic_action_id.as_str(),
        ),
        (
            "generic_action",
            expected.target_action,
            generic.action.as_str(),
        ),
        (
            "payload_digest",
            expected.payload_digest,
            constitutional.payload_digest.as_str(),
        ),
        (
            "generic_payload_digest",
            expected.payload_digest,
            generic.payload_digest.as_str(),
        ),
        (
            "policy_registry_digest",
            generic.policy_registry_digest.as_str(),
            constitutional.action_policy_registry_digest.as_str(),
        ),
        (
            "constitution_digest",
            generic.constitution_digest.as_str(),
            constitutional.constitution_digest.as_str(),
        ),
        (
            "nonce",
            generic.nonce.as_str(),
            constitutional.nonce.as_str(),
        ),
    ] {
        if expected_value != observed {
            return Err(AutonomyManifestProjectionError::AuthorityBindingMismatch { field });
        }
    }
    if generic.organism_id != expected.organism_id
        || generic.brain_id != expected.brain_id
        || generic.mission_id.as_deref() != expected.mission_id
        || generic.mission_head_id.as_deref() != expected.mission_head_id
        || constitutional.mission_id.as_deref() != expected.mission_id
        || constitutional.mission_head_id.as_deref() != expected.mission_head_id
        || generic.authority_variant != variant
        || constitutional.required_authority_variant != variant
        || generic.active_mode != constitutional.active_mode
        || generic.issued_at != constitutional.issued_at
        || generic.expires_at != constitutional.expires_at
        || expected.applicable_grant_id != Some(constitutional.grant_id.as_str())
        || expected.applicable_tier != Some(constitutional.effective_tier)
    {
        return Err(AutonomyManifestProjectionError::AuthorityBindingMismatch {
            field: "constitutional_authority_envelope",
        });
    }
    Ok(())
}

/// Build the unsigned generic G2 envelope from the already-sealed G9
/// capability.  One owner mint operation therefore has one validity window,
/// nonce, exact semantic action and binding set; callers only supply the G2
/// signing-key metadata and must pass the result through `sign_capability`.
pub fn build_unsigned_g2_autonomy_capability(
    evidence: &AutonomyAuthorityEvidenceV1,
    issuer_key_id: impl Into<String>,
    algorithm: impl Into<String>,
    key_registry_epoch: u64,
) -> Result<AuthorityCapabilityV1, AutonomyManifestProjectionError> {
    let constitutional = &evidence.capability.core;
    if evidence.intent_digest != constitutional.intent_digest
        || evidence.decision.decision_digest() != constitutional.decision_digest
        || !matches!(
            constitutional.required_authority_variant,
            AuthorityVariant::Policy | AuthorityVariant::AgentQuorum
        )
    {
        return Err(AutonomyManifestProjectionError::AuthorityBindingMismatch {
            field: "constitutional_mint_source",
        });
    }
    let action = ActionId::new(&constitutional.semantic_action_id).map_err(|_| {
        AutonomyManifestProjectionError::AuthorityBindingMismatch {
            field: "semantic_action_id",
        }
    })?;
    if !action.is_semantic_catalog_id() {
        return Err(AutonomyManifestProjectionError::AuthorityBindingMismatch {
            field: "semantic_action_id",
        });
    }
    Ok(AuthorityCapabilityV1 {
        schema: AUTHORITY_CAPABILITY_SCHEMA.to_string(),
        capability_id: constitutional.capability_id.clone(),
        issuer_subject_id: constitutional.issuer_subject_id.clone(),
        issuer_key_id: issuer_key_id.into(),
        algorithm: algorithm.into(),
        subject_id: constitutional.caller_subject_id.clone(),
        audience: constitutional.audience.clone(),
        organism_id: constitutional.organism_id.clone(),
        brain_id: constitutional.brain_id.clone(),
        mission_id: constitutional.mission_id.clone(),
        mission_head_id: constitutional.mission_head_id.clone(),
        action,
        authority_variant: constitutional.required_authority_variant,
        active_mode: constitutional.active_mode,
        payload_digest: constitutional.payload_digest.clone(),
        policy_registry_digest: constitutional.action_policy_registry_digest.clone(),
        constitution_digest: constitutional.constitution_digest.clone(),
        key_registry_epoch,
        issued_at: constitutional.issued_at,
        expires_at: constitutional.expires_at,
        nonce: constitutional.nonce.clone(),
        signature: OpaqueSignature::new(String::new()),
    })
}

/// Binary capability and gate evidence are supplied separately so a runtime
/// can never infer "mechanically proven" merely because code exists or a mode
/// appears in the active constitution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutonomyManifestCapabilityPolicyV1 {
    pub supported_modes: BTreeSet<ActiveMode>,
    pub mechanically_proven_modes: BTreeSet<ActiveMode>,
}

impl AutonomyManifestCapabilityPolicyV1 {
    pub fn human_gated_only() -> Self {
        Self {
            supported_modes: BTreeSet::from([ActiveMode::HumanGated]),
            mechanically_proven_modes: BTreeSet::new(),
        }
    }

    pub fn validate(&self) -> Result<(), AutonomyManifestProjectionError> {
        if !self.supported_modes.contains(&ActiveMode::HumanGated) {
            return Err(AutonomyManifestProjectionError::InvalidCapabilityPolicy {
                reason: "HUMAN_GATED must remain supported as the fail-closed mode".to_string(),
            });
        }
        if !self
            .mechanically_proven_modes
            .is_subset(&self.supported_modes)
        {
            return Err(AutonomyManifestProjectionError::InvalidCapabilityPolicy {
                reason: "mechanically proven modes must be a subset of supported modes".to_string(),
            });
        }
        Ok(())
    }
}

/// A self-contained, validated manifest input.  All digests are copied from
/// one committed protected-root/state pair while the autonomy owner lock is
/// held; consumers never read the autonomy journal themselves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutonomyManifestProjectionV1 {
    pub schema: String,
    pub organism_id: String,
    pub repo_id: String,
    pub brain_id: String,
    pub observed_at: u64,
    pub state_generation: u64,
    pub state_digest: String,
    pub protected_root_digest: String,
    pub journal_sequence: u64,
    pub journal_record_digest: String,
    pub intent_store_root_digest: String,
    pub intent_count: u64,
    pub autonomy: AutonomyFact,
    pub authorities: BTreeMap<String, AuthorityFact>,
}

impl AutonomyManifestProjectionV1 {
    pub fn from_committed_runtime(
        state: &AutonomyRuntimeStateV1,
        protected_root: &ProtectedAutonomyRootV1,
        capability_policy: &AutonomyManifestCapabilityPolicyV1,
        organism_id: &str,
        repo_id: &str,
        brain_id: &str,
        observed_at: u64,
    ) -> Result<Self, AutonomyManifestProjectionError> {
        capability_policy.validate()?;
        if protected_root.phase != ProtectedAutonomyPhaseV1::Committed {
            return Err(AutonomyManifestProjectionError::ProtectedRootNotCommitted);
        }
        let computed_state_digest = state.compute_state_digest()?;
        if computed_state_digest != state.state_digest {
            return Err(AutonomyManifestProjectionError::RuntimeMismatch {
                field: "state_digest",
            });
        }
        for (field, equal) in [
            (
                "protected_root.state_digest",
                protected_root.state_digest == state.state_digest,
            ),
            (
                "protected_root.state_generation",
                protected_root.state_generation == state.generation,
            ),
            (
                "protected_root.autonomy_epoch",
                protected_root.autonomy_epoch == state.autonomy_epoch.autonomy_epoch,
            ),
            (
                "protected_root.constitution_epoch",
                protected_root.constitution_epoch == state.constitution.core.constitution_epoch,
            ),
            (
                "protected_root.intent_store_root_digest",
                protected_root.intent_store_root_digest == state.intent_store_root_digest,
            ),
        ] {
            if !equal {
                return Err(AutonomyManifestProjectionError::RuntimeMismatch { field });
            }
        }
        if !capability_policy
            .supported_modes
            .contains(&state.autonomy_epoch.active_mode)
        {
            return Err(AutonomyManifestProjectionError::ActiveModeUnsupported);
        }
        if state.autonomy_epoch.active_mode != ActiveMode::HumanGated
            && state.autonomy_epoch.activation_receipt_id.is_none()
        {
            return Err(AutonomyManifestProjectionError::MissingActivationReceipt);
        }

        let autonomy_epoch_digest = compute_autonomy_epoch_reference_digest(&state.autonomy_epoch)?;
        let supported_modes = capability_policy
            .supported_modes
            .iter()
            .map(|mode| mode_name(*mode).to_string())
            .collect();
        let mechanically_proven_modes = capability_policy
            .mechanically_proven_modes
            .iter()
            .map(|mode| mode_name(*mode).to_string())
            .collect();
        let max_effective_tier_projection = state
            .active_grants
            .iter()
            .map(|grant| grant.core.max_tier)
            .max()
            .map(tier_name)
            .unwrap_or("NONE")
            .to_string();

        let autonomy = AutonomyFact {
            supported_modes,
            mechanically_proven_modes,
            active_mode: mode_name(state.autonomy_epoch.active_mode).to_string(),
            activation_receipt_id: state
                .autonomy_epoch
                .activation_receipt_id
                .clone()
                .unwrap_or_default(),
            constitution_digest: state.constitution.constitution_digest.clone(),
            constitution_epoch: state.constitution.core.constitution_epoch,
            safety_kernel_digest: state.kernel.kernel_digest.clone(),
            autonomy_epoch: state.autonomy_epoch.autonomy_epoch,
            grants_digest: state.autonomy_epoch.grants_digest.clone(),
            quorum_policy_digest: state.independence_spec.independence_spec_digest.clone(),
            max_effective_tier_projection,
            issuance_frozen: state.autonomy_epoch.issuance_frozen,
            sentinel_safety_state: safety_state_name(state.autonomy_epoch.safety_state).to_string(),
        };

        let authority = |revision: String, digest: String| AuthorityFact {
            revision,
            digest,
            observed_at,
            freshness: AuthorityFreshness::Fresh,
            status: AuthorityStatus::Available,
        };
        let mut authorities = BTreeMap::new();
        authorities.insert(
            AUTHORITY_JOURNAL_ID.to_string(),
            authority(
                protected_root.journal_sequence.to_string(),
                protected_root.journal_record_digest.clone(),
            ),
        );
        authorities.insert(
            AUTONOMY_EPOCH_AUTHORITY_ID.to_string(),
            authority(
                state.autonomy_epoch.autonomy_epoch.to_string(),
                autonomy_epoch_digest,
            ),
        );
        authorities.insert(
            CONSTITUTION_AUTHORITY_ID.to_string(),
            authority(
                state.constitution.core.constitution_epoch.to_string(),
                state.constitution.constitution_digest.clone(),
            ),
        );
        authorities.insert(
            INTENT_CORE_STORE_AUTHORITY_ID.to_string(),
            authority(
                state.intent_index.len().to_string(),
                state.intent_store_root_digest.clone(),
            ),
        );
        authorities.insert(
            SENTINEL_OUTBOX_AUTHORITY_ID.to_string(),
            authority(
                protected_root.sentinel_outbox_epoch.to_string(),
                state
                    .sentinel_outbox_tail
                    .as_ref()
                    .map(|outbox| outbox.record_digest.clone())
                    .unwrap_or_else(|| state.state_digest.clone()),
            ),
        );

        let projection = Self {
            schema: AUTONOMY_MANIFEST_PROJECTION_SCHEMA.to_string(),
            organism_id: organism_id.to_string(),
            repo_id: repo_id.to_string(),
            brain_id: brain_id.to_string(),
            observed_at,
            state_generation: state.generation,
            state_digest: state.state_digest.clone(),
            protected_root_digest: protected_root.root_digest.clone(),
            journal_sequence: protected_root.journal_sequence,
            journal_record_digest: protected_root.journal_record_digest.clone(),
            intent_store_root_digest: state.intent_store_root_digest.clone(),
            intent_count: state.intent_index.len() as u64,
            autonomy,
            authorities,
        };
        projection.validate()?;
        Ok(projection)
    }

    pub fn validate(&self) -> Result<(), AutonomyManifestProjectionError> {
        if self.schema != AUTONOMY_MANIFEST_PROJECTION_SCHEMA {
            return Err(AutonomyManifestProjectionError::InvalidSchema);
        }
        if self.organism_id.trim().is_empty()
            || self.repo_id.trim().is_empty()
            || self.brain_id.trim().is_empty()
        {
            return Err(AutonomyManifestProjectionError::OwnerScopeMissing);
        }
        for (field, digest) in [
            ("state_digest", self.state_digest.as_str()),
            ("protected_root_digest", self.protected_root_digest.as_str()),
            ("journal_record_digest", self.journal_record_digest.as_str()),
            (
                "intent_store_root_digest",
                self.intent_store_root_digest.as_str(),
            ),
            (
                "autonomy.constitution_digest",
                self.autonomy.constitution_digest.as_str(),
            ),
            (
                "autonomy.safety_kernel_digest",
                self.autonomy.safety_kernel_digest.as_str(),
            ),
            (
                "autonomy.grants_digest",
                self.autonomy.grants_digest.as_str(),
            ),
            (
                "autonomy.quorum_policy_digest",
                self.autonomy.quorum_policy_digest.as_str(),
            ),
        ] {
            require_digest(field, digest)?;
        }
        if !self
            .autonomy
            .supported_modes
            .contains(&self.autonomy.active_mode)
        {
            return Err(AutonomyManifestProjectionError::ActiveModeUnsupported);
        }
        if !self
            .autonomy
            .mechanically_proven_modes
            .is_subset(&self.autonomy.supported_modes)
        {
            return Err(AutonomyManifestProjectionError::InvalidCapabilityPolicy {
                reason: "mechanically proven modes must be a subset of supported modes".to_string(),
            });
        }
        if self.autonomy.active_mode != "HUMAN_GATED"
            && self.autonomy.activation_receipt_id.is_empty()
        {
            return Err(AutonomyManifestProjectionError::MissingActivationReceipt);
        }
        let expected = BTreeSet::from([
            AUTHORITY_JOURNAL_ID,
            AUTONOMY_EPOCH_AUTHORITY_ID,
            CONSTITUTION_AUTHORITY_ID,
            INTENT_CORE_STORE_AUTHORITY_ID,
            SENTINEL_OUTBOX_AUTHORITY_ID,
        ]);
        if self
            .authorities
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            != expected
        {
            return Err(AutonomyManifestProjectionError::AuthoritySetMismatch);
        }
        for authority in self.authorities.values() {
            if authority.status != AuthorityStatus::Available
                || authority.freshness != AuthorityFreshness::Fresh
                || authority.observed_at != self.observed_at
            {
                return Err(AutonomyManifestProjectionError::AuthorityNotFresh);
            }
            require_digest("authority.digest", &authority.digest)?;
        }
        Ok(())
    }
}

/// Object-safe reader installed into the served owner.  The HTTP manifest path
/// asks for one point-in-time projection; it never receives a mutable runtime
/// handle.
pub trait AutonomyManifestReader: Send + Sync {
    fn read_projection(
        &self,
        observed_at: u64,
    ) -> Result<AutonomyManifestProjectionV1, AutonomyManifestProjectionError>;
}

/// Shared owner adapter.  Mutation users and manifest readers synchronize on
/// the same runtime object; the projection copies its facts before returning.
pub struct SharedAutonomyRuntimeV1<B, V>
where
    B: ProtectedAutonomyRootBackend,
    V: AutonomyArtifactVerifier,
{
    store: Mutex<AutonomyRuntimeStore<B, V>>,
    capability_policy: AutonomyManifestCapabilityPolicyV1,
    assurance: AutonomyRuntimeAssurance,
}

impl<B, V> SharedAutonomyRuntimeV1<B, V>
where
    B: ProtectedAutonomyRootBackend,
    V: AutonomyArtifactVerifier,
{
    pub fn new(
        store: AutonomyRuntimeStore<B, V>,
        capability_policy: AutonomyManifestCapabilityPolicyV1,
    ) -> Result<Self, AutonomyManifestProjectionError> {
        capability_policy.validate()?;
        let assurance = store.assurance();
        Ok(Self {
            store: Mutex::new(store),
            capability_policy,
            assurance,
        })
    }

    pub fn with_store_mut<T>(
        &self,
        operation: impl FnOnce(&mut AutonomyRuntimeStore<B, V>) -> Result<T, AutonomyRuntimeError>,
    ) -> Result<T, AutonomyManifestProjectionError> {
        let mut store = self
            .store
            .lock()
            .map_err(|_| AutonomyManifestProjectionError::OwnerLockPoisoned)?;
        operation(&mut store).map_err(AutonomyManifestProjectionError::Runtime)
    }

    /// Consume the G9 capability and copy its resulting protected state under
    /// one lock.  This prevents a later RED/mode transition from being mixed
    /// into the admission receipt's manifest/authority binding.
    pub fn consume_and_project(
        &self,
        evidence: &AutonomyAuthorityEvidenceV1,
        now_ms: u64,
    ) -> Result<AutonomyAdmissionOutcomeV1, AutonomyManifestProjectionError> {
        let mut store = self
            .store
            .lock()
            .map_err(|_| AutonomyManifestProjectionError::OwnerLockPoisoned)?;
        let receipt = store.consume_autonomy_capability(
            &evidence.intent_digest,
            &evidence.decision,
            &evidence.capability,
            evidence.sentinel.as_ref(),
            now_ms,
        )?;
        let state = store.state()?;
        let protected_root = store
            .protected_root()
            .ok_or(AutonomyManifestProjectionError::ProtectedRootMissing)?;
        let (organism_id, repo_id, brain_id) = store.owner_scope();
        let projection = AutonomyManifestProjectionV1::from_committed_runtime(
            state,
            protected_root,
            &self.capability_policy,
            organism_id,
            repo_id,
            brain_id,
            now_ms,
        )?;
        if receipt.committed_state_digest != projection.state_digest
            || receipt.protected_root_digest != projection.protected_root_digest
        {
            return Err(AutonomyManifestProjectionError::AdmissionProjectionMismatch);
        }
        Ok(AutonomyAdmissionOutcomeV1 {
            receipt,
            projection,
        })
    }
}

impl<B, V> AutonomyAdmissionOwner for SharedAutonomyRuntimeV1<B, V>
where
    B: ProtectedAutonomyRootBackend + Send,
    V: AutonomyArtifactVerifier + Send,
{
    fn assurance(&self) -> AutonomyRuntimeAssurance {
        self.assurance
    }

    fn admit(
        &self,
        evidence: &AutonomyAuthorityEvidenceV1,
        now_ms: u64,
    ) -> Result<AutonomyAdmissionOutcomeV1, AutonomyManifestProjectionError> {
        self.consume_and_project(evidence, now_ms)
    }
}

impl<B, V> AutonomyManifestReader for SharedAutonomyRuntimeV1<B, V>
where
    B: ProtectedAutonomyRootBackend + Send,
    V: AutonomyArtifactVerifier + Send,
{
    fn read_projection(
        &self,
        observed_at: u64,
    ) -> Result<AutonomyManifestProjectionV1, AutonomyManifestProjectionError> {
        let store = self
            .store
            .lock()
            .map_err(|_| AutonomyManifestProjectionError::OwnerLockPoisoned)?;
        if store.is_poisoned() {
            return Err(AutonomyManifestProjectionError::RuntimePoisoned);
        }
        let state = store.state()?;
        let protected_root = store
            .protected_root()
            .ok_or(AutonomyManifestProjectionError::ProtectedRootMissing)?;
        let (organism_id, repo_id, brain_id) = store.owner_scope();
        AutonomyManifestProjectionV1::from_committed_runtime(
            state,
            protected_root,
            &self.capability_policy,
            organism_id,
            repo_id,
            brain_id,
            observed_at,
        )
    }
}

#[derive(Debug)]
pub enum AutonomyManifestProjectionError {
    InvalidSchema,
    OwnerScopeMissing,
    InvalidCapabilityPolicy { reason: String },
    ProtectedRootMissing,
    ProtectedRootNotCommitted,
    RuntimePoisoned,
    OwnerLockPoisoned,
    RuntimeMismatch { field: &'static str },
    ActiveModeUnsupported,
    MissingActivationReceipt,
    AuthoritySetMismatch,
    AuthorityNotFresh,
    AdmissionProjectionMismatch,
    AuthorityBindingMismatch { field: &'static str },
    InvalidDigest { field: &'static str },
    Runtime(AutonomyRuntimeError),
    Canonical(m1nd_control::CanonicalError),
}

impl fmt::Display for AutonomyManifestProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSchema => {
                formatter.write_str("autonomy manifest projection schema is invalid")
            }
            Self::OwnerScopeMissing => {
                formatter.write_str("autonomy manifest projection owner scope is incomplete")
            }
            Self::InvalidCapabilityPolicy { reason } => {
                write!(formatter, "autonomy capability policy is invalid: {reason}")
            }
            Self::ProtectedRootMissing => formatter.write_str("protected autonomy root is absent"),
            Self::ProtectedRootNotCommitted => {
                formatter.write_str("protected autonomy root is not COMMITTED")
            }
            Self::RuntimePoisoned => formatter.write_str("autonomy runtime is poisoned"),
            Self::OwnerLockPoisoned => formatter.write_str("autonomy owner lock is poisoned"),
            Self::RuntimeMismatch { field } => {
                write!(
                    formatter,
                    "autonomy runtime/protected-root mismatch at {field}"
                )
            }
            Self::ActiveModeUnsupported => {
                formatter.write_str("active autonomy mode is not in the binary-supported mode set")
            }
            Self::MissingActivationReceipt => formatter
                .write_str("autonomous active mode lacks its prior-authority activation receipt"),
            Self::AuthoritySetMismatch => formatter.write_str(
                "autonomy manifest authority set is incomplete or contains an unknown owner",
            ),
            Self::AuthorityNotFresh => formatter.write_str(
                "autonomy manifest authority is not one fresh point-in-time observation",
            ),
            Self::AdmissionProjectionMismatch => formatter.write_str(
                "autonomy admission receipt differs from its protected post-consumption projection",
            ),
            Self::AuthorityBindingMismatch { field } => {
                write!(
                    formatter,
                    "autonomy/G2 authority binding differs at '{field}'"
                )
            }
            Self::InvalidDigest { field } => {
                write!(
                    formatter,
                    "field '{field}' is not a lowercase SHA-256 digest"
                )
            }
            Self::Runtime(error) => write!(formatter, "autonomy runtime: {error}"),
            Self::Canonical(error) => write!(formatter, "autonomy canonicalization: {error}"),
        }
    }
}

impl Error for AutonomyManifestProjectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Runtime(error) => Some(error),
            Self::Canonical(error) => Some(error),
            _ => None,
        }
    }
}

impl From<AutonomyRuntimeError> for AutonomyManifestProjectionError {
    fn from(error: AutonomyRuntimeError) -> Self {
        Self::Runtime(error)
    }
}

impl From<m1nd_control::CanonicalError> for AutonomyManifestProjectionError {
    fn from(error: m1nd_control::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

fn require_digest(
    field: &'static str,
    digest: &str,
) -> Result<(), AutonomyManifestProjectionError> {
    if digest.len() == 64
        && digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(AutonomyManifestProjectionError::InvalidDigest { field })
    }
}

const fn mode_name(mode: ActiveMode) -> &'static str {
    match mode {
        ActiveMode::HumanGated => "HUMAN_GATED",
        ActiveMode::PolicyAutonomous => "POLICY_AUTONOMOUS",
        ActiveMode::FullAutonomy => "FULL_AUTONOMY",
    }
}

const fn tier_name(tier: AutonomyTier) -> &'static str {
    match tier {
        AutonomyTier::A0Observe => "A0_OBSERVE",
        AutonomyTier::A1Propose => "A1_PROPOSE",
        AutonomyTier::A2Execute => "A2_EXECUTE",
        AutonomyTier::A3AutonomousLand => "A3_AUTONOMOUS_LAND",
        AutonomyTier::A4AutonomousGovern => "A4_AUTONOMOUS_GOVERN",
        AutonomyTier::A5FullAutonomy => "A5_FULL_AUTONOMY",
    }
}

const fn safety_state_name(state: SafetyState) -> &'static str {
    match state {
        SafetyState::Healthy => "HEALTHY",
        SafetyState::Frozen => "FROZEN",
        SafetyState::PendingRed => "PENDING_RED",
        SafetyState::Recovering => "RECOVERING",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(byte: char) -> String {
        std::iter::repeat_n(byte, 64).collect()
    }

    fn projection() -> AutonomyManifestProjectionV1 {
        let observed_at = 42;
        let authority = |revision: &str, digest: String| AuthorityFact {
            revision: revision.to_string(),
            digest,
            observed_at,
            freshness: AuthorityFreshness::Fresh,
            status: AuthorityStatus::Available,
        };
        AutonomyManifestProjectionV1 {
            schema: AUTONOMY_MANIFEST_PROJECTION_SCHEMA.to_string(),
            organism_id: "organism-1".to_string(),
            repo_id: "repo-1".to_string(),
            brain_id: "brain-1".to_string(),
            observed_at,
            state_generation: 1,
            state_digest: digest('a'),
            protected_root_digest: digest('b'),
            journal_sequence: 2,
            journal_record_digest: digest('c'),
            intent_store_root_digest: digest('d'),
            intent_count: 0,
            autonomy: AutonomyFact {
                supported_modes: BTreeSet::from(["HUMAN_GATED".to_string()]),
                mechanically_proven_modes: BTreeSet::new(),
                active_mode: "HUMAN_GATED".to_string(),
                activation_receipt_id: String::new(),
                constitution_digest: digest('e'),
                constitution_epoch: 0,
                safety_kernel_digest: digest('f'),
                autonomy_epoch: 0,
                grants_digest: digest('1'),
                quorum_policy_digest: digest('2'),
                max_effective_tier_projection: "NONE".to_string(),
                issuance_frozen: true,
                sentinel_safety_state: "FROZEN".to_string(),
            },
            authorities: BTreeMap::from([
                (
                    AUTHORITY_JOURNAL_ID.to_string(),
                    authority("2", digest('c')),
                ),
                (
                    AUTONOMY_EPOCH_AUTHORITY_ID.to_string(),
                    authority("0", digest('3')),
                ),
                (
                    CONSTITUTION_AUTHORITY_ID.to_string(),
                    authority("0", digest('e')),
                ),
                (
                    INTENT_CORE_STORE_AUTHORITY_ID.to_string(),
                    authority("0", digest('d')),
                ),
                (
                    SENTINEL_OUTBOX_AUTHORITY_ID.to_string(),
                    authority("0", digest('a')),
                ),
            ]),
        }
    }

    #[test]
    fn bootstrap_projection_is_valid_without_an_activation_receipt() {
        projection().validate().unwrap();
    }

    #[test]
    fn proven_support_and_active_authority_remain_distinct() {
        let mut value = projection();
        value
            .autonomy
            .mechanically_proven_modes
            .insert("FULL_AUTONOMY".to_string());
        assert!(matches!(
            value.validate(),
            Err(AutonomyManifestProjectionError::InvalidCapabilityPolicy { .. })
        ));

        value
            .autonomy
            .supported_modes
            .insert("FULL_AUTONOMY".to_string());
        value.validate().unwrap();
        assert_eq!(value.autonomy.active_mode, "HUMAN_GATED");
    }

    #[test]
    fn autonomous_active_mode_requires_prior_authority_receipt() {
        let mut value = projection();
        value
            .autonomy
            .supported_modes
            .insert("POLICY_AUTONOMOUS".to_string());
        value.autonomy.active_mode = "POLICY_AUTONOMOUS".to_string();
        assert!(matches!(
            value.validate(),
            Err(AutonomyManifestProjectionError::MissingActivationReceipt)
        ));
        value.autonomy.activation_receipt_id = "autonomy-activation:receipt".to_string();
        value.validate().unwrap();
    }

    #[test]
    fn stale_or_partial_authority_projection_is_refused() {
        let mut stale = projection();
        stale
            .authorities
            .get_mut(AUTHORITY_JOURNAL_ID)
            .unwrap()
            .observed_at += 1;
        assert!(matches!(
            stale.validate(),
            Err(AutonomyManifestProjectionError::AuthorityNotFresh)
        ));

        let mut partial = projection();
        partial.authorities.remove(SENTINEL_OUTBOX_AUTHORITY_ID);
        assert!(matches!(
            partial.validate(),
            Err(AutonomyManifestProjectionError::AuthoritySetMismatch)
        ));
    }
}
