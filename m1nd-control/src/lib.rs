//! Canonical, dependency-inward control-plane contracts for m1nd.
//!
//! G1 structural validators remain parsing/diagnostic contracts and never
//! authenticate opaque bytes. G2 authority boundaries use the explicit
//! cryptographic verifier and replay ledger exported by this crate.

mod action_catalog;
mod authority_wal;
pub mod autonomy;
pub mod autonomy_runtime;
mod canonical;
mod crypto_authority;
mod envelope;
mod identity;
mod manifest;
mod mission;
mod policy;
mod release;
mod replay_ledger;

use serde::{Deserialize, Serialize};

pub use action_catalog::{
    m1nd10_action_catalog, ActionCatalogEntryV1, ActionCatalogError, ActionCatalogV1,
    ActionCatalogValidation, AuthorityFloor, ACTION_CATALOG_DIGEST_DOMAIN, ACTION_CATALOG_SCHEMA,
    M1ND10_ACTION_CATALOG_VERSION,
};
pub use authority_wal::{
    AuthorityTransactionBindingV1, AuthorityTransactionStructuralValidation,
    AuthorityTransactionV1, AuthorityWalAbortV1, AuthorityWalCommitV1, AuthorityWalContractError,
    AuthorityWalIntegrityDisposition, AuthorityWalPayloadV1, AuthorityWalPhase,
    AuthorityWalPrepareV1, AuthorityWalProvisionalV1, AuthorityWalRecordStructuralValidation,
    AuthorityWalRecordV1, CapabilityKind, PositiveAuthorityTransactionV1,
    SafetyKernelTransactionV1, AUTHORITY_TRANSACTION_DIGEST_DOMAIN,
    AUTHORITY_TRANSACTION_SIGNATURE_DOMAIN, AUTHORITY_WAL_PAYLOAD_DIGEST_DOMAIN,
    AUTHORITY_WAL_RECORD_DIGEST_DOMAIN, AUTHORITY_WAL_RECORD_SCHEMA,
    AUTHORITY_WAL_RECORD_SIGNATURE_DOMAIN, POSITIVE_AUTHORITY_TRANSACTION_SCHEMA,
    SAFETY_KERNEL_TRANSACTION_SCHEMA,
};
pub use canonical::{
    canonical_json, canonical_json_string, digest_canonical, digest_domain_bytes, CanonicalError,
    CANONICALIZATION_VERSION,
};
pub use crypto_authority::{
    sign_authority_message, sign_canonical_authority_payload, sign_capability, sign_human_approval,
    sign_owner_challenge, verify_authority_message_signature,
    verify_canonical_authority_payload_signature, verify_capability, verify_capability_once,
    verify_human_approval, verify_human_approval_once, verify_owner_challenge,
    verify_owner_challenge_once, AuthorityCapabilityV1, AuthorityCryptoError, AuthoritySigner,
    AuthoritySignerError, CapabilityVerificationContext, ChallengeVerificationContext,
    CryptographicIntegrity, VerificationKeyRegistryV1, VerificationKeyV1, VerifiedArtifact,
    VerifiedAuthorityOnceV1, VerifiedAuthorityV1, AUTHORITY_CAPABILITY_SCHEMA,
    AUTHORITY_CAPABILITY_SIGNATURE_DOMAIN, DEFAULT_AUTHORITY_CLOCK_SKEW_MS,
    ECDSA_P256_SHA256_X962_ALGORITHM, ED25519_ALGORITHM, HUMAN_APPROVAL_SIGNATURE_DOMAIN,
    HUMAN_APPROVAL_SIGNED_SCHEMA, OWNER_CHALLENGE_SIGNATURE_DOMAIN, OWNER_CHALLENGE_SIGNED_SCHEMA,
    SIGNED_BODY_DIGEST_DOMAIN, VERIFICATION_KEY_REGISTRY_SCHEMA,
};
pub use envelope::{
    CausalEnvelopeV1, EnvelopeError, EnvelopeValidation, EnvelopeValidationContext, EventClass,
    EventClassRequirements, IntegrityDisposition, DEFAULT_CLOCK_SKEW_MS, PAYLOAD_DIGEST_DOMAIN,
    REPLAY_KEY_DOMAIN,
};
pub use identity::{
    ClientIdentityV1, EnrollmentEvidenceV1, HumanApprovalV1, HumanDecisionV1, HumanKeyRegistryV1,
    HumanKeyV1, IdentityError, IdentityIntegrityDisposition, IdentityStatus,
    IdentityStructuralValidation, OwnerChallengeV1, OwnerIdentityV1,
    ENROLLMENT_PUBLIC_KEY_DIGEST_DOMAIN, ENROLLMENT_SCOPES_DIGEST_DOMAIN,
    HUMAN_APPROVAL_DIGEST_DOMAIN, OWNER_CHALLENGE_DIGEST_DOMAIN,
};
pub use manifest::{
    ArchitectureFact, AuthorityFact, AuthorityFreshness, AuthorityStatus, AutonomyFact,
    CapabilitiesFact, GraphFact, ManifestCoherence, ManifestIssue, ManifestIssueKind,
    ManifestVerification, OrganismManifestV1, ReleaseProvenanceFact, RuntimeFact, SchemasFact,
    SourceFact, UiFact, ARCHITECTURE_AUTHORITY_ID, GRAPH_AUTHORITY_ID, MANIFEST_DIGEST_DOMAIN,
    ORGANISM_MANIFEST_SCHEMA, RELEASE_AUTHORITY_ID, RUNTIME_BINARY_AUTHORITY_ID,
    SOURCE_AUTHORITY_ID, UI_BUNDLE_AUTHORITY_ID,
};
pub use mission::{
    mission_transition_rule, ContractStructuralValidation, ExecutionDispatchAckV1,
    ExecutionDispatchState, ExecutionDispatchV1, ExecutionOutcome, ExecutionResultV1,
    IterationRule, MissionContractError, MissionHeadContext, MissionHeadSnapshot,
    MissionIntegrityDisposition, MissionState, MissionTransitionIntentV1, MissionTransitionRule,
    MissionTransitionSource, MissionTransitionValidation, ReviewDecision, ReviewResultV1, Role,
    DEFAULT_MISSION_CLOCK_SKEW_MS, EXECUTION_DISPATCH_ACK_DIGEST_DOMAIN,
    EXECUTION_DISPATCH_ACK_SCHEMA, EXECUTION_DISPATCH_DIGEST_DOMAIN, EXECUTION_DISPATCH_SCHEMA,
    EXECUTION_RESULT_DIGEST_DOMAIN, EXECUTION_RESULT_SCHEMA, EXECUTION_RESULT_SIGNATURE_DOMAIN,
    MISSION_TRANSITION_INTENT_DIGEST_DOMAIN, MISSION_TRANSITION_INTENT_SCHEMA,
    MISSION_TRANSITION_PAYLOAD_DIGEST_DOMAIN, MISSION_TRANSITION_RULES,
    REVIEW_RESULT_DIGEST_DOMAIN, REVIEW_RESULT_SCHEMA, REVIEW_RESULT_SIGNATURE_DOMAIN,
};
pub use policy::{
    ActionEffectFloorV1, ActionId, ActionPolicyRegistryV1, ActionPolicyRuleV1, ActiveMode,
    AuthorityVariant, AutonomyTier, Effect, Ingress, PolicyError, PolicyValidation,
    ReachablePolicyTupleV1, RiskClass, ACTION_POLICY_DIGEST_DOMAIN, ACTION_POLICY_REGISTRY_SCHEMA,
};
pub use release::{
    FindingSeverity, FindingStatus, GateId, GateReceiptCoreV1, GateReceiptV1, GateVerdict,
    IndependentAdversarialReviewCoreV1, IndependentAdversarialReviewReceiptV1, MetricSpecCoreV1,
    MetricSpecV1, ReleaseCandidateManifestCoreV1, ReleaseCandidateManifestV1, ReleaseContractError,
    ReleaseEvidenceSetV1, ReleaseFindingV1, ReleaseIntegrityDisposition,
    ReleaseStructuralValidation, GATE_RECEIPT_DIGEST_DOMAIN, GATE_RECEIPT_SCHEMA,
    INDEPENDENT_REVIEW_RECEIPT_DIGEST_DOMAIN, INDEPENDENT_REVIEW_RECEIPT_SCHEMA,
    METRIC_SPEC_DIGEST_DOMAIN, METRIC_SPEC_SCHEMA, RELEASE_CANDIDATE_DIGEST_DOMAIN,
    RELEASE_CANDIDATE_SCHEMA,
};
pub use replay_ledger::{
    MemoryReplayLedger, PersistentReplayLedger, ReplayClaimV1, ReplayDurability, ReplayLedger,
    ReplayLedgerError, ReplayReceiptV1, REPLAY_CLAIM_DIGEST_DOMAIN, REPLAY_CLAIM_SCHEMA,
    REPLAY_LEDGER_RECORD_DIGEST_DOMAIN, REPLAY_LEDGER_RECORD_SCHEMA, REPLAY_SCOPE_DIGEST_DOMAIN,
};

/// A signature that has a wire type but no verification semantics in G1.
///
/// Possession of this value proves only that opaque bytes were structurally
/// present. Callers must not interpret it as authentic until a G2 verifier has
/// checked algorithm, key lifecycle, trust domain, signature bytes, and replay
/// state.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OpaqueSignature(String);

impl OpaqueSignature {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<&str> for OpaqueSignature {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for OpaqueSignature {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}
