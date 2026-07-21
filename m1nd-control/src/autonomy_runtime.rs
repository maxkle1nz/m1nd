//! Durable, fail-closed execution substrate for the constitutional autonomy contracts.
//!
//! `autonomy` defines the canonical records and semantic validators. This module
//! supplies the missing execution boundary: a content-addressed intent store, a
//! hash-chained two-phase journal, protected-root anti-rollback, explicit
//! signature-verifier injection, one-shot capability consumption, prior-authority
//! activation, and the RED outbox/latch fence. It deliberately ships no production
//! protected-store or signing-key implementation. A caller must inject those
//! platform capabilities; software-only fixtures are explicitly below production
//! assurance and can never satisfy a production configuration.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::autonomy::{
    compute_grants_digest, AgentQuorumDecisionEvidenceV1, AuthorityDecisionV1,
    AutonomyActivationReceiptV1, AutonomyActivationValidationContext, AutonomyContractError,
    AutonomyEpochV1, AutonomyGrantV1, ConstitutionStoreV1, IndependenceSpecV1, QuorumVoteV1,
    RedLatchReceiptV1, RedLatchState, RedOutboxState, SafetyActionIntentV1, SafetyCapabilityV1,
    SafetyKernelV1, SafetyState, SentinelRedOutboxV1, SentinelVerdict, SentinelVerdictV1,
    SovereignActionIntentV1,
};
use crate::{
    canonical_json, canonical_json_string, digest_canonical, digest_domain_bytes, ActiveMode,
    AuthorityVariant, AutonomyTier, CanonicalError, OpaqueSignature, CANONICALIZATION_VERSION,
};

pub const AUTONOMY_RUNTIME_STATE_SCHEMA: &str = "m1nd-autonomy-runtime-state-v1";
pub const AUTONOMY_PROTECTED_ROOT_SCHEMA: &str = "m1nd-autonomy-protected-root-v1";
pub const AUTONOMY_JOURNAL_RECORD_SCHEMA: &str = "m1nd-autonomy-journal-record-v1";
pub const INTENT_CORE_ENTRY_SCHEMA: &str = "m1nd-intent-core-entry-v1";
pub const TIER_EVIDENCE_SCHEMA: &str = "m1nd-autonomy-tier-evidence-v1";
pub const PENDING_RED_RUNTIME_SCHEMA: &str = "m1nd-pending-red-runtime-v1";
pub const AUTONOMY_ADMISSION_RECEIPT_SCHEMA: &str = "m1nd-autonomy-admission-receipt-v1";
pub const AUTONOMY_RECOVERY_RECEIPT_SCHEMA: &str = "m1nd-autonomy-recovery-receipt-v1";

pub const AUTONOMY_RUNTIME_STATE_DIGEST_DOMAIN: &str = "m1nd-autonomy-runtime-state-v1";
pub const AUTONOMY_PROTECTED_ROOT_DIGEST_DOMAIN: &str = "m1nd-autonomy-protected-root-v1";
pub const AUTONOMY_JOURNAL_RECORD_DIGEST_DOMAIN: &str = "m1nd-autonomy-journal-record-v1";
pub const INTENT_OBJECT_DIGEST_DOMAIN: &str = "m1nd-intent-object-v1";
pub const INTENT_STORE_ROOT_DIGEST_DOMAIN: &str = "m1nd-intent-store-root-v1";
pub const TIER_EVIDENCE_DIGEST_DOMAIN: &str = "m1nd-autonomy-tier-evidence-v1";
pub const TIER_EVIDENCE_SET_DIGEST_DOMAIN: &str = "m1nd-autonomy-tier-evidence-set-v1";
pub const RED_COMMIT_MARKER_DIGEST_DOMAIN: &str = "m1nd-red-commit-marker-v1";
pub const AUTONOMY_ADMISSION_RECEIPT_DIGEST_DOMAIN: &str = "m1nd-autonomy-admission-receipt-v1";
pub const AUTONOMY_RECOVERY_RECEIPT_DIGEST_DOMAIN: &str = "m1nd-autonomy-recovery-receipt-v1";
pub const QUORUM_VOTE_VERIFICATION_DIGEST_DOMAIN: &str =
    "m1nd-quorum-vote-verification-material-v1";

const JOURNAL_FILE_NAME: &str = "autonomy-journal.jsonl";
const INTENT_OBJECT_DIRECTORY: &str = "intent-objects";
const MAX_DEFAULT_INTENT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AutonomyRuntimeAssurance {
    SoftwareTestOnlyNotProduction,
    ProtectedProduction,
}

#[derive(Clone, Debug)]
pub struct AutonomyRuntimeConfig {
    pub root: PathBuf,
    pub durability_domain_id: String,
    pub organism_id: String,
    pub repo_id: String,
    pub brain_id: String,
    pub required_assurance: AutonomyRuntimeAssurance,
    pub max_intent_bytes: u64,
}

impl AutonomyRuntimeConfig {
    pub fn production(
        root: impl Into<PathBuf>,
        durability_domain_id: impl Into<String>,
        organism_id: impl Into<String>,
        repo_id: impl Into<String>,
        brain_id: impl Into<String>,
    ) -> Self {
        Self {
            root: root.into(),
            durability_domain_id: durability_domain_id.into(),
            organism_id: organism_id.into(),
            repo_id: repo_id.into(),
            brain_id: brain_id.into(),
            required_assurance: AutonomyRuntimeAssurance::ProtectedProduction,
            max_intent_bytes: MAX_DEFAULT_INTENT_BYTES,
        }
    }

    /// Explicit fixture-only constructor. The name and assurance marker are
    /// intentionally impossible to confuse with production proof.
    pub fn software_test_only(
        root: impl Into<PathBuf>,
        durability_domain_id: impl Into<String>,
        organism_id: impl Into<String>,
        repo_id: impl Into<String>,
        brain_id: impl Into<String>,
    ) -> Self {
        Self {
            root: root.into(),
            durability_domain_id: durability_domain_id.into(),
            organism_id: organism_id.into(),
            repo_id: repo_id.into(),
            brain_id: brain_id.into(),
            required_assurance: AutonomyRuntimeAssurance::SoftwareTestOnlyNotProduction,
            max_intent_bytes: MAX_DEFAULT_INTENT_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProtectedAutonomyPhaseV1 {
    Prepared,
    Committed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedAutonomyRootV1 {
    pub schema: String,
    pub phase: ProtectedAutonomyPhaseV1,
    pub transition_id: String,
    pub journal_sequence: u64,
    pub journal_record_digest: String,
    pub state_digest: String,
    pub state_generation: u64,
    pub autonomy_epoch: u64,
    pub constitution_epoch: u64,
    pub intent_store_root_digest: String,
    pub sentinel_outbox_epoch: u64,
    pub red_latch_epoch: u64,
    pub root_digest: String,
}

impl ProtectedAutonomyRootV1 {
    fn compute_digest(&self) -> Result<String, CanonicalError> {
        let mut material = self.clone();
        material.root_digest.clear();
        digest_canonical(AUTONOMY_PROTECTED_ROOT_DIGEST_DOMAIN, &material)
    }

    fn seal(&mut self) -> Result<(), CanonicalError> {
        self.root_digest = self.compute_digest()?;
        Ok(())
    }

    fn validate(&self) -> Result<(), AutonomyRuntimeError> {
        if self.schema != AUTONOMY_PROTECTED_ROOT_SCHEMA {
            return Err(AutonomyRuntimeError::CorruptProtectedRoot {
                reason: format!("unsupported schema '{}'", self.schema),
            });
        }
        require_non_empty("protected_root.transition_id", &self.transition_id)?;
        require_digest(
            "protected_root.journal_record_digest",
            &self.journal_record_digest,
        )?;
        require_digest("protected_root.state_digest", &self.state_digest)?;
        require_digest(
            "protected_root.intent_store_root_digest",
            &self.intent_store_root_digest,
        )?;
        require_digest("protected_root.root_digest", &self.root_digest)?;
        let computed = self.compute_digest()?;
        if computed != self.root_digest {
            return Err(AutonomyRuntimeError::CorruptProtectedRoot {
                reason: "self-digest mismatch".to_owned(),
            });
        }
        Ok(())
    }
}

/// Platform-owned protected storage. Implementations must provide a real
/// compare-and-swap over the complete root; ordinary files must report only
/// `SoftwareTestOnlyNotProduction` assurance.
pub trait ProtectedAutonomyRootBackend {
    fn assurance(&self) -> AutonomyRuntimeAssurance;
    fn load(&self) -> Result<Option<ProtectedAutonomyRootV1>, String>;
    fn compare_and_swap(
        &mut self,
        expected: Option<&ProtectedAutonomyRootV1>,
        next: &ProtectedAutonomyRootV1,
    ) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AutonomyArtifactKindV1 {
    SafetyKernel,
    Constitution,
    AutonomyEpoch,
    AutonomyGrant,
    ActivationReceipt,
    RecoveryReceipt,
    AuthorityDecision,
    QuorumVote,
    AutonomyCapability,
    SentinelVerdict,
    SentinelRedOutbox,
    RedLatchReceipt,
    SafetyCapability,
    TierEvidence,
}

pub struct AutonomyVerificationRequestV1<'a> {
    pub kind: AutonomyArtifactKindV1,
    pub artifact_digest: &'a str,
    pub subject_id: &'a str,
    pub signature: &'a OpaqueSignature,
    pub canonical_bytes: &'a [u8],
    pub identity_key_binary_policy_digest: Option<&'a str>,
    pub now_ms: u64,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct QuorumVoteVerificationMaterialV1<'a> {
    verifier_principal_id: &'a str,
    verifier_key_id: &'a str,
    failure_domain: &'a str,
    parent_session_context_digest: &'a str,
    intent_digest: &'a str,
    constitution_digest: &'a str,
    candidate_digest: Option<&'a str>,
    evidence_digest: &'a str,
    rollout_plan_digest: &'a str,
    rollback_plan_digest: &'a str,
    disposition: crate::autonomy::QuorumVoteDisposition,
}

impl<'a> From<&'a QuorumVoteV1> for QuorumVoteVerificationMaterialV1<'a> {
    fn from(vote: &'a QuorumVoteV1) -> Self {
        Self {
            verifier_principal_id: &vote.verifier_principal_id,
            verifier_key_id: &vote.verifier_key_id,
            failure_domain: &vote.failure_domain,
            parent_session_context_digest: &vote.parent_session_context_digest,
            intent_digest: &vote.intent_digest,
            constitution_digest: &vote.constitution_digest,
            candidate_digest: vote.candidate_digest.as_deref(),
            evidence_digest: &vote.evidence_digest,
            rollout_plan_digest: &vote.rollout_plan_digest,
            rollback_plan_digest: &vote.rollback_plan_digest,
            disposition: vote.disposition,
        }
    }
}

/// Signature verification is injected so the control crate never creates,
/// imports, or stores private keys. A verifier must authenticate the canonical
/// bytes in the appropriate domain and enforce key lifecycle/pinning.
pub trait AutonomyArtifactVerifier {
    fn assurance(&self) -> AutonomyRuntimeAssurance;
    fn verify(&self, request: AutonomyVerificationRequestV1<'_>) -> Result<(), String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StoredIntentKindV1 {
    Sovereign,
    Safety,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentCoreEntryV1 {
    pub schema: String,
    pub intent_digest: String,
    pub content_address: String,
    pub canonicalization_version: String,
    pub kind: StoredIntentKindV1,
    pub canonical_bytes_digest: String,
    pub byte_len: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutonomyTierEvidenceV1 {
    pub schema: String,
    pub subject_id: String,
    pub tier: AutonomyTier,
    pub evaluator_subject_id: String,
    pub proposer_subject_id: String,
    pub executor_subject_id: Option<String>,
    pub verifier_principals: BTreeSet<String>,
    pub failure_domains: BTreeSet<String>,
    pub previous_evidence_digest: Option<String>,
    pub exact_release_candidate_digest: String,
    pub shadow_receipt_digest: String,
    pub canary_receipt_digest: String,
    pub rollback_receipt_digest: String,
    pub metric_receipt_digest: String,
    pub recorded_at: u64,
    pub evidence_digest: String,
    pub evaluator_signature: OpaqueSignature,
}

impl AutonomyTierEvidenceV1 {
    pub fn compute_digest(&self) -> Result<String, CanonicalError> {
        let mut material = self.clone();
        material.evidence_digest.clear();
        material.evaluator_signature = OpaqueSignature::new("");
        digest_canonical(TIER_EVIDENCE_DIGEST_DOMAIN, &material)
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.evidence_digest = self.compute_digest()?;
        Ok(())
    }

    fn validate(
        &self,
        previous: Option<&AutonomyTierEvidenceV1>,
    ) -> Result<(), AutonomyRuntimeError> {
        if self.schema != TIER_EVIDENCE_SCHEMA {
            return Err(AutonomyRuntimeError::InvalidTierEvidence {
                reason: format!("unsupported schema '{}'", self.schema),
            });
        }
        for (field, value) in [
            ("tier_evidence.subject_id", self.subject_id.as_str()),
            (
                "tier_evidence.evaluator_subject_id",
                self.evaluator_subject_id.as_str(),
            ),
            (
                "tier_evidence.proposer_subject_id",
                self.proposer_subject_id.as_str(),
            ),
        ] {
            require_non_empty(field, value)?;
        }
        for (field, digest) in [
            (
                "tier_evidence.exact_release_candidate_digest",
                &self.exact_release_candidate_digest,
            ),
            (
                "tier_evidence.shadow_receipt_digest",
                &self.shadow_receipt_digest,
            ),
            (
                "tier_evidence.canary_receipt_digest",
                &self.canary_receipt_digest,
            ),
            (
                "tier_evidence.rollback_receipt_digest",
                &self.rollback_receipt_digest,
            ),
            (
                "tier_evidence.metric_receipt_digest",
                &self.metric_receipt_digest,
            ),
            ("tier_evidence.evidence_digest", &self.evidence_digest),
        ] {
            require_digest(field, digest)?;
        }
        if self.evaluator_signature.is_empty() {
            return Err(AutonomyRuntimeError::InvalidTierEvidence {
                reason: "evaluator signature is empty".to_owned(),
            });
        }
        if self.subject_id == self.evaluator_subject_id
            || self.subject_id == self.proposer_subject_id
            || self.executor_subject_id.as_deref() == Some(self.subject_id.as_str())
            || self.verifier_principals.contains(&self.subject_id)
        {
            return Err(AutonomyRuntimeError::SelfPromotion {
                subject_id: self.subject_id.clone(),
            });
        }
        if self.verifier_principals.contains(&self.proposer_subject_id)
            || self
                .executor_subject_id
                .as_ref()
                .is_some_and(|subject| self.verifier_principals.contains(subject))
        {
            return Err(AutonomyRuntimeError::InvalidTierEvidence {
                reason: "proposer/executor cannot occupy a verifier seat".to_owned(),
            });
        }
        if self.tier >= AutonomyTier::A4AutonomousGovern
            && (self.verifier_principals.len() != 4 || self.failure_domains.len() < 3)
        {
            return Err(AutonomyRuntimeError::InvalidTierEvidence {
                reason: "A4/A5 evidence requires four verifier seats and three failure domains"
                    .to_owned(),
            });
        }
        match previous {
            None => {
                if self.tier != AutonomyTier::A0Observe || self.previous_evidence_digest.is_some() {
                    return Err(AutonomyRuntimeError::TierSequence {
                        subject_id: self.subject_id.clone(),
                        expected: AutonomyTier::A0Observe,
                        observed: self.tier,
                    });
                }
            }
            Some(previous) => {
                let expected = next_tier(previous.tier).ok_or_else(|| {
                    AutonomyRuntimeError::InvalidTierEvidence {
                        reason: "A5 is terminal for shadow/canary evidence".to_owned(),
                    }
                })?;
                if self.tier != expected
                    || self.previous_evidence_digest.as_deref()
                        != Some(previous.evidence_digest.as_str())
                {
                    return Err(AutonomyRuntimeError::TierSequence {
                        subject_id: self.subject_id.clone(),
                        expected,
                        observed: self.tier,
                    });
                }
            }
        }
        let computed = self.compute_digest()?;
        if computed != self.evidence_digest {
            return Err(AutonomyRuntimeError::InvalidTierEvidence {
                reason: "evidence digest mismatch".to_owned(),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingRedRuntimeV1 {
    pub schema: String,
    pub source_intent_digest: String,
    pub verdict: SentinelVerdictV1,
    pub outbox: SentinelRedOutboxV1,
    pub latch: Option<RedLatchReceiptV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutonomyRecoveryReceiptCoreV1 {
    pub receipt_id: String,
    pub frozen_state_digest: String,
    pub frozen_epoch_reference_digest: String,
    pub frozen_autonomy_epoch: u64,
    pub last_valid_mode: ActiveMode,
    pub required_authority_variant: AuthorityVariant,
    pub recovery_intent_digest: String,
    pub authority_decision_digest: String,
    pub sentinel_verdict_digest: String,
    pub terminal_red_latch_receipt_digest: String,
    pub remediation_evidence_digest: String,
    pub rollback_validation_digest: String,
    pub target_autonomy_epoch: u64,
    pub target_constitution_digest: String,
    pub target_constitution_epoch: u64,
    pub issuer_subject_id: String,
    pub issued_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutonomyRecoveryReceiptV1 {
    pub schema: String,
    pub core: AutonomyRecoveryReceiptCoreV1,
    pub receipt_digest: String,
    pub signature: OpaqueSignature,
}

impl AutonomyRecoveryReceiptV1 {
    pub fn compute_digest(&self) -> Result<String, CanonicalError> {
        let mut core = self.core.clone();
        core.receipt_id.clear();
        digest_canonical(AUTONOMY_RECOVERY_RECEIPT_DIGEST_DOMAIN, &core)
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.receipt_digest = self.compute_digest()?;
        self.core.receipt_id = format!("autonomy-recovery:{}", self.receipt_digest);
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutonomyRuntimeStateV1 {
    pub schema: String,
    pub durability_domain_id: String,
    pub generation: u64,
    pub kernel: SafetyKernelV1,
    pub independence_spec: IndependenceSpecV1,
    pub constitution: ConstitutionStoreV1,
    pub autonomy_epoch: AutonomyEpochV1,
    pub active_grants: Vec<AutonomyGrantV1>,
    pub activation_receipts: BTreeMap<String, AutonomyActivationReceiptV1>,
    pub recovery_receipts: BTreeMap<String, AutonomyRecoveryReceiptV1>,
    pub tier_evidence: BTreeMap<String, Vec<AutonomyTierEvidenceV1>>,
    pub intent_index: BTreeMap<String, IntentCoreEntryV1>,
    pub intent_store_root_digest: String,
    pub consumed_capability_digests: BTreeSet<String>,
    pub sentinel_outbox_tail: Option<SentinelRedOutboxV1>,
    pub last_red_latch_epoch: u64,
    pub terminal_red_latches: BTreeMap<String, RedLatchReceiptV1>,
    pub last_valid_mode_before_freeze: Option<ActiveMode>,
    pub pending_red: Option<PendingRedRuntimeV1>,
    pub state_digest: String,
}

impl AutonomyRuntimeStateV1 {
    pub fn compute_state_digest(&self) -> Result<String, CanonicalError> {
        let mut material = self.clone();
        material.state_digest.clear();
        digest_canonical(AUTONOMY_RUNTIME_STATE_DIGEST_DOMAIN, &material)
    }

    fn seal(&mut self) -> Result<(), CanonicalError> {
        self.intent_store_root_digest = compute_intent_store_root(&self.intent_index)?;
        self.state_digest = self.compute_state_digest()?;
        Ok(())
    }

    pub fn active_mode(&self) -> ActiveMode {
        self.autonomy_epoch.active_mode
    }

    pub fn latest_tier_evidence(&self, subject_id: &str) -> Option<&AutonomyTierEvidenceV1> {
        self.tier_evidence
            .get(subject_id)
            .and_then(|lane| lane.last())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AutonomyTransitionKindV1 {
    Bootstrap,
    IntentStored,
    TierEvidenceRecorded,
    ModeActivated,
    CapabilityConsumed,
    SentinelRedPersisted,
    RedLatched,
    RedSafetyCommitted,
    RecoveredFromFrozen,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "phase",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
enum AutonomyJournalPayloadV1 {
    Prepare {
        transition_id: String,
        transition_kind: AutonomyTransitionKindV1,
        previous_state_digest: Option<String>,
        next_state: Box<AutonomyRuntimeStateV1>,
    },
    Commit {
        transition_id: String,
        prepare_record_digest: String,
        committed_state_digest: String,
    },
    Abort {
        transition_id: String,
        prepare_record_digest: String,
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AutonomyJournalRecordV1 {
    schema: String,
    sequence: u64,
    previous_record_digest: Option<String>,
    payload: AutonomyJournalPayloadV1,
    record_digest: String,
}

impl AutonomyJournalRecordV1 {
    fn compute_digest(&self) -> Result<String, CanonicalError> {
        let mut material = self.clone();
        material.record_digest.clear();
        digest_canonical(AUTONOMY_JOURNAL_RECORD_DIGEST_DOMAIN, &material)
    }

    fn seal(&mut self) -> Result<(), CanonicalError> {
        self.record_digest = self.compute_digest()?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutonomyAdmissionReceiptV1 {
    pub schema: String,
    pub intent_digest: String,
    pub decision_digest: String,
    pub capability_digest: String,
    pub authority_variant: AuthorityVariant,
    pub committed_state_digest: String,
    pub protected_root_digest: String,
    pub receipt_digest: String,
}

impl AutonomyAdmissionReceiptV1 {
    fn seal(&mut self) -> Result<(), CanonicalError> {
        let mut material = self.clone();
        material.receipt_digest.clear();
        self.receipt_digest =
            digest_canonical(AUTONOMY_ADMISSION_RECEIPT_DIGEST_DOMAIN, &material)?;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum AutonomyRuntimeError {
    #[error("autonomy runtime configuration field '{field}' is empty")]
    EmptyConfiguration { field: &'static str },
    #[error("required field '{field}' is empty")]
    EmptyRequired { field: &'static str },
    #[error("field '{field}' is not a lowercase SHA-256 digest")]
    InvalidDigest { field: &'static str },
    #[error("path is a symbolic link and is refused: {path}")]
    SymlinkRefused { path: PathBuf },
    #[error("autonomy runtime I/O failed during {operation} at {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("protected-root backend failed during {operation}: {message}")]
    ProtectedBackend {
        operation: &'static str,
        message: String,
    },
    #[error("artifact verifier failed for {kind:?}: {message}")]
    Verification {
        kind: AutonomyArtifactKindV1,
        message: String,
    },
    #[error("{component} assurance {actual:?} is below required {required:?}")]
    AssuranceTooLow {
        component: &'static str,
        required: AutonomyRuntimeAssurance,
        actual: AutonomyRuntimeAssurance,
    },
    #[error("autonomy journal is corrupt at line {line}: {reason}")]
    CorruptJournal { line: usize, reason: String },
    #[error("protected autonomy root is corrupt: {reason}")]
    CorruptProtectedRoot { reason: String },
    #[error("anti-rollback check failed: {reason}")]
    AntiRollback { reason: String },
    #[error("journal changed outside this owner (expected {expected_len} bytes, observed {observed_len})")]
    ConcurrentModification {
        expected_len: u64,
        observed_len: u64,
    },
    #[error("autonomy runtime handle is poisoned after an ambiguous durable transition")]
    Poisoned,
    #[error("autonomy runtime is not bootstrapped")]
    NotBootstrapped,
    #[error("autonomy runtime is already bootstrapped")]
    AlreadyBootstrapped,
    #[error("runtime state is invalid: {reason}")]
    InvalidState { reason: String },
    #[error("tier evidence for '{subject_id}' expected {expected:?}, observed {observed:?}")]
    TierSequence {
        subject_id: String,
        expected: AutonomyTier,
        observed: AutonomyTier,
    },
    #[error("invalid autonomy tier evidence: {reason}")]
    InvalidTierEvidence { reason: String },
    #[error(
        "subject '{subject_id}' cannot authorize, evaluate, execute, or verify its own promotion"
    )]
    SelfPromotion { subject_id: String },
    #[error("intent object '{intent_digest}' is absent")]
    IntentMissing { intent_digest: String },
    #[error("intent object '{intent_digest}' is corrupt: {reason}")]
    IntentCorrupt {
        intent_digest: String,
        reason: String,
    },
    #[error("intent object exceeds configured limit ({observed} > {limit} bytes)")]
    IntentTooLarge { observed: u64, limit: u64 },
    #[error("intent digest '{intent_digest}' already resolves to different bytes")]
    IntentCollision { intent_digest: String },
    #[error("positive authority is frozen by safety state")]
    PositiveAuthorityFrozen,
    #[error("capability '{capability_digest}' was already consumed")]
    CapabilityReplay { capability_digest: String },
    #[error("activation receipt '{receipt_id}' was already consumed")]
    ActivationReplay { receipt_id: String },
    #[error("activation target grants are not backed by exact shadow/canary evidence: {reason}")]
    MissingPromotionEvidence { reason: String },
    #[error("no pending RED exists")]
    NoPendingRed,
    #[error("RED transition is invalid: {reason}")]
    InvalidRedTransition { reason: String },
    #[error("recovery transition is invalid: {reason}")]
    InvalidRecovery { reason: String },
    #[error("recovery receipt '{receipt_id}' was already consumed")]
    RecoveryReplay { receipt_id: String },
    #[error(transparent)]
    Contract(#[from] AutonomyContractError),
    #[error(transparent)]
    Canonical(#[from] CanonicalError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

struct PreparedReplay {
    record: AutonomyJournalRecordV1,
    state: AutonomyRuntimeStateV1,
}

struct CommitReplay {
    record: AutonomyJournalRecordV1,
    prepare_record_digest: String,
    state: AutonomyRuntimeStateV1,
}

struct JournalReplay {
    next_sequence: u64,
    tail_digest: Option<String>,
    known_len: u64,
    latest_commit: Option<CommitReplay>,
    commits_by_prepare: BTreeMap<String, CommitReplay>,
    pending: Option<PreparedReplay>,
}

pub struct AutonomyRuntimeStore<B, V>
where
    B: ProtectedAutonomyRootBackend,
    V: AutonomyArtifactVerifier,
{
    config: AutonomyRuntimeConfig,
    backend: B,
    verifier: V,
    journal_path: PathBuf,
    object_directory: PathBuf,
    journal: File,
    next_sequence: u64,
    tail_digest: Option<String>,
    known_len: u64,
    protected_root: Option<ProtectedAutonomyRootV1>,
    state: Option<AutonomyRuntimeStateV1>,
    poisoned: bool,
}

impl<B, V> AutonomyRuntimeStore<B, V>
where
    B: ProtectedAutonomyRootBackend,
    V: AutonomyArtifactVerifier,
{
    pub fn open(
        config: AutonomyRuntimeConfig,
        backend: B,
        verifier: V,
        now_ms: u64,
    ) -> Result<Self, AutonomyRuntimeError> {
        if config.durability_domain_id.trim().is_empty() {
            return Err(AutonomyRuntimeError::EmptyConfiguration {
                field: "durability_domain_id",
            });
        }
        for (field, value) in [
            ("organism_id", config.organism_id.as_str()),
            ("repo_id", config.repo_id.as_str()),
            ("brain_id", config.brain_id.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(AutonomyRuntimeError::EmptyConfiguration { field });
            }
        }
        ensure_assurance(
            "protected-root backend",
            config.required_assurance,
            backend.assurance(),
        )?;
        ensure_assurance(
            "artifact verifier",
            config.required_assurance,
            verifier.assurance(),
        )?;
        refuse_symlink_if_present(&config.root)?;
        fs::create_dir_all(&config.root).map_err(|source| AutonomyRuntimeError::Io {
            operation: "create_runtime_root",
            path: config.root.clone(),
            source,
        })?;
        let object_directory = config.root.join(INTENT_OBJECT_DIRECTORY);
        refuse_symlink_if_present(&object_directory)?;
        fs::create_dir_all(&object_directory).map_err(|source| AutonomyRuntimeError::Io {
            operation: "create_intent_object_directory",
            path: object_directory.clone(),
            source,
        })?;
        let journal_path = config.root.join(JOURNAL_FILE_NAME);
        refuse_symlink_if_present(&journal_path)?;
        let existed = journal_path.exists();
        let journal = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&journal_path)
            .map_err(|source| AutonomyRuntimeError::Io {
                operation: "open_journal",
                path: journal_path.clone(),
                source,
            })?;
        if !existed {
            journal
                .sync_all()
                .map_err(|source| AutonomyRuntimeError::Io {
                    operation: "sync_new_journal",
                    path: journal_path.clone(),
                    source,
                })?;
            sync_directory(&config.root, "sync_runtime_root")?;
        }
        let mut reader = journal
            .try_clone()
            .map_err(|source| AutonomyRuntimeError::Io {
                operation: "clone_journal_for_replay",
                path: journal_path.clone(),
                source,
            })?;
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|source| AutonomyRuntimeError::Io {
                operation: "read_journal",
                path: journal_path.clone(),
                source,
            })?;
        let replay = replay_journal(&bytes)?;
        let protected_root =
            backend
                .load()
                .map_err(|message| AutonomyRuntimeError::ProtectedBackend {
                    operation: "load",
                    message,
                })?;
        if let Some(root) = &protected_root {
            root.validate()?;
        }
        let mut store = Self {
            config,
            backend,
            verifier,
            journal_path,
            object_directory,
            journal,
            next_sequence: replay.next_sequence,
            tail_digest: replay.tail_digest.clone(),
            known_len: replay.known_len,
            protected_root,
            state: None,
            poisoned: false,
        };
        store.recover(replay, now_ms)?;
        if let Some(state) = store.state.as_ref() {
            store.validate_state(state, now_ms)?;
            store.verify_state_artifacts(state, now_ms)?;
            store.verify_all_intent_objects(state, now_ms)?;
        }
        Ok(store)
    }

    pub fn state(&self) -> Result<&AutonomyRuntimeStateV1, AutonomyRuntimeError> {
        self.state
            .as_ref()
            .ok_or(AutonomyRuntimeError::NotBootstrapped)
    }

    pub fn protected_root(&self) -> Option<&ProtectedAutonomyRootV1> {
        self.protected_root.as_ref()
    }

    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    /// Lowest assurance jointly supplied by the protected-root backend and
    /// artifact verifier that were admitted when this store was opened.
    pub fn assurance(&self) -> AutonomyRuntimeAssurance {
        self.backend.assurance().min(self.verifier.assurance())
    }

    /// Protected owner scope pinned when the store was opened. Projection
    /// adapters copy this tuple so a served owner cannot attach a valid but
    /// foreign G9 runtime to its own G2 authority state.
    pub fn owner_scope(&self) -> (&str, &str, &str) {
        (
            &self.config.organism_id,
            &self.config.repo_id,
            &self.config.brain_id,
        )
    }

    pub fn bootstrap(
        &mut self,
        kernel: SafetyKernelV1,
        independence_spec: IndependenceSpecV1,
        constitution: ConstitutionStoreV1,
        autonomy_epoch: AutonomyEpochV1,
        now_ms: u64,
    ) -> Result<(), AutonomyRuntimeError> {
        self.ensure_usable()?;
        if self.state.is_some() || self.protected_root.is_some() {
            return Err(AutonomyRuntimeError::AlreadyBootstrapped);
        }
        kernel.validate()?;
        constitution.validate(&independence_spec, &kernel, now_ms)?;
        autonomy_epoch.validate_bootstrap(&constitution, &[], now_ms)?;
        let mut state = AutonomyRuntimeStateV1 {
            schema: AUTONOMY_RUNTIME_STATE_SCHEMA.to_owned(),
            durability_domain_id: self.config.durability_domain_id.clone(),
            generation: 1,
            kernel,
            independence_spec,
            constitution,
            autonomy_epoch,
            active_grants: Vec::new(),
            activation_receipts: BTreeMap::new(),
            recovery_receipts: BTreeMap::new(),
            tier_evidence: BTreeMap::new(),
            intent_index: BTreeMap::new(),
            intent_store_root_digest: compute_intent_store_root(&BTreeMap::new())?,
            consumed_capability_digests: BTreeSet::new(),
            sentinel_outbox_tail: None,
            last_red_latch_epoch: 0,
            terminal_red_latches: BTreeMap::new(),
            last_valid_mode_before_freeze: None,
            pending_red: None,
            state_digest: String::new(),
        };
        state.seal()?;
        self.validate_state(&state, now_ms)?;
        self.verify_state_artifacts(&state, now_ms)?;
        self.commit_transition(AutonomyTransitionKindV1::Bootstrap, state, now_ms)
    }

    /// Record mechanically checked shadow/canary evidence. This never mutates
    /// `AutonomyEpochV1`, grants, active mode, or activation receipts.
    pub fn record_tier_evidence(
        &mut self,
        evidence: AutonomyTierEvidenceV1,
        now_ms: u64,
    ) -> Result<(), AutonomyRuntimeError> {
        self.ensure_positive_unfrozen()?;
        let current = self.state()?.clone();
        let previous = current
            .tier_evidence
            .get(&evidence.subject_id)
            .and_then(|lane| lane.last());
        evidence.validate(previous)?;
        self.verify_artifact(
            AutonomyArtifactKindV1::TierEvidence,
            &evidence.evidence_digest,
            &evidence.evaluator_subject_id,
            &evidence.evaluator_signature,
            &evidence,
            now_ms,
        )?;
        let mut next = current;
        next.generation = next.generation.saturating_add(1);
        next.tier_evidence
            .entry(evidence.subject_id.clone())
            .or_default()
            .push(evidence);
        next.seal()?;
        self.commit_transition(AutonomyTransitionKindV1::TierEvidenceRecorded, next, now_ms)
    }

    pub fn persist_sovereign_intent(
        &mut self,
        intent: &SovereignActionIntentV1,
        now_ms: u64,
    ) -> Result<IntentCoreEntryV1, AutonomyRuntimeError> {
        self.ensure_positive_unfrozen()?;
        intent.validate_canonical_core(now_ms)?;
        let entry = self.persist_intent_object(
            StoredIntentKindV1::Sovereign,
            &intent.intent_digest,
            &intent.intent_core_ref.content_address,
            intent,
        )?;
        self.index_persisted_intent(entry.clone(), now_ms)?;
        Ok(entry)
    }

    /// Persist the sole sovereign intent admitted while terminally frozen.
    /// This does not authorize work: it only makes the exact recovery proposal
    /// durable so the last valid authority and a fresh GREEN sentinel can judge it.
    pub fn persist_recovery_intent(
        &mut self,
        intent: &SovereignActionIntentV1,
        now_ms: u64,
    ) -> Result<IntentCoreEntryV1, AutonomyRuntimeError> {
        self.ensure_usable()?;
        intent.validate_canonical_core(now_ms)?;
        self.validate_recovery_intent_shape(self.state()?, intent)?;
        let entry = self.persist_intent_object(
            StoredIntentKindV1::Sovereign,
            &intent.intent_digest,
            &intent.intent_core_ref.content_address,
            intent,
        )?;
        self.index_persisted_intent(entry.clone(), now_ms)?;
        Ok(entry)
    }

    pub fn persist_safety_intent(
        &mut self,
        safety_intent: &SafetyActionIntentV1,
        now_ms: u64,
    ) -> Result<IntentCoreEntryV1, AutonomyRuntimeError> {
        self.ensure_usable()?;
        let state = self.state()?.clone();
        let pending = state
            .pending_red
            .as_ref()
            .ok_or(AutonomyRuntimeError::NoPendingRed)?;
        let latch = pending
            .latch
            .as_ref()
            .ok_or(AutonomyRuntimeError::NoPendingRed)?;
        let source = self.resolve_sovereign_intent(&pending.source_intent_digest, now_ms)?;
        safety_intent.validate(
            &source,
            &pending.verdict,
            latch,
            &state.kernel,
            state.autonomy_epoch.constitution_epoch,
            state.autonomy_epoch.autonomy_epoch,
        )?;
        let entry = self.persist_intent_object(
            StoredIntentKindV1::Safety,
            &safety_intent.safety_intent_digest,
            &safety_intent.safety_intent_core_ref.content_address,
            safety_intent,
        )?;
        self.index_persisted_intent(entry.clone(), now_ms)?;
        Ok(entry)
    }

    pub fn resolve_sovereign_intent(
        &self,
        intent_digest: &str,
        now_ms: u64,
    ) -> Result<SovereignActionIntentV1, AutonomyRuntimeError> {
        let bytes = self.resolve_intent_bytes(intent_digest, StoredIntentKindV1::Sovereign)?;
        let intent: SovereignActionIntentV1 = serde_json::from_slice(&bytes)?;
        if canonical_json(&intent)? != bytes {
            return Err(AutonomyRuntimeError::IntentCorrupt {
                intent_digest: intent_digest.to_owned(),
                reason: "object is not canonical JSON".to_owned(),
            });
        }
        intent.validate_canonical_core(now_ms)?;
        if intent.intent_digest != intent_digest {
            return Err(AutonomyRuntimeError::IntentCorrupt {
                intent_digest: intent_digest.to_owned(),
                reason: "embedded sovereign intent digest mismatch".to_owned(),
            });
        }
        Ok(intent)
    }

    pub fn resolve_safety_intent(
        &self,
        intent_digest: &str,
    ) -> Result<SafetyActionIntentV1, AutonomyRuntimeError> {
        let bytes = self.resolve_intent_bytes(intent_digest, StoredIntentKindV1::Safety)?;
        let intent: SafetyActionIntentV1 = serde_json::from_slice(&bytes)?;
        if canonical_json(&intent)? != bytes || intent.safety_intent_digest != intent_digest {
            return Err(AutonomyRuntimeError::IntentCorrupt {
                intent_digest: intent_digest.to_owned(),
                reason: "non-canonical safety intent or embedded digest mismatch".to_owned(),
            });
        }
        Ok(intent)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn activate_mode(
        &mut self,
        activation_intent_digest: &str,
        authority_decision: &AuthorityDecisionV1,
        sentinel: Option<&SentinelVerdictV1>,
        receipt: AutonomyActivationReceiptV1,
        target_constitution: ConstitutionStoreV1,
        target_independence_spec: IndependenceSpecV1,
        target_epoch: AutonomyEpochV1,
        target_grants: Vec<AutonomyGrantV1>,
        exact_release_candidate_digest: &str,
        now_ms: u64,
    ) -> Result<(), AutonomyRuntimeError> {
        self.ensure_positive_unfrozen()?;
        let current = self.state()?.clone();
        if current
            .activation_receipts
            .contains_key(&receipt.core.receipt_id)
        {
            return Err(AutonomyRuntimeError::ActivationReplay {
                receipt_id: receipt.core.receipt_id.clone(),
            });
        }
        let activation_intent = self.resolve_sovereign_intent(activation_intent_digest, now_ms)?;
        if activation_intent.core.action_class != "autonomy.activate"
            || activation_intent.core.candidate_digest.as_deref()
                != Some(exact_release_candidate_digest)
        {
            return Err(AutonomyRuntimeError::MissingPromotionEvidence {
                reason: "activation authority intent does not bind autonomy.activate and the exact candidate"
                    .to_owned(),
            });
        }
        activation_intent.validate(&current.autonomy_epoch, None, now_ms)?;
        authority_decision.validate_positive(
            &activation_intent,
            &current.constitution,
            &current.kernel,
            sentinel,
            now_ms,
        )?;
        receipt.validate_transition(
            &current.autonomy_epoch,
            &target_epoch,
            &target_constitution,
            &target_grants,
            AutonomyActivationValidationContext {
                exact_release_candidate_digest,
                authority_decision,
                now_ms,
            },
        )?;
        if receipt.core.issuer_subject_id != activation_intent.core.issuer_subject_id
            || receipt.core.issuer_subject_id != activation_intent.core.decision_subject_id
        {
            return Err(AutonomyRuntimeError::MissingPromotionEvidence {
                reason: "activation receipt issuer is not the exact prior-mode decision subject"
                    .to_owned(),
            });
        }
        let promoted_subjects: BTreeSet<&str> = target_grants
            .iter()
            .map(|grant| grant.core.subject_id.as_str())
            .collect();
        if promoted_subjects.contains(receipt.core.issuer_subject_id.as_str())
            || promoted_subjects.contains(activation_intent.core.proposer_subject_id.as_str())
            || activation_intent
                .core
                .executor_subject_id
                .as_deref()
                .is_some_and(|subject| promoted_subjects.contains(subject))
        {
            return Err(AutonomyRuntimeError::SelfPromotion {
                subject_id: receipt.core.issuer_subject_id.clone(),
            });
        }
        self.validate_target_grant_evidence(
            &current,
            &target_grants,
            exact_release_candidate_digest,
            &receipt,
        )?;
        target_constitution.validate(&target_independence_spec, &current.kernel, now_ms)?;
        self.verify_artifact(
            AutonomyArtifactKindV1::AuthorityDecision,
            authority_decision.decision_digest(),
            &receipt.core.issuer_subject_id,
            authority_decision_signature(authority_decision),
            authority_decision,
            now_ms,
        )?;
        self.verify_artifact(
            AutonomyArtifactKindV1::ActivationReceipt,
            &receipt.receipt_digest,
            &receipt.core.issuer_subject_id,
            &receipt.signature,
            &receipt,
            now_ms,
        )?;
        self.verify_artifact(
            AutonomyArtifactKindV1::Constitution,
            &target_constitution.constitution_digest,
            &target_constitution.core.issuer_subject_id,
            &target_constitution.signature,
            &target_constitution,
            now_ms,
        )?;
        for grant in &target_grants {
            self.verify_artifact(
                AutonomyArtifactKindV1::AutonomyGrant,
                &grant.grant_digest,
                &grant.core.subject_id,
                &grant.owner_signature,
                grant,
                now_ms,
            )?;
        }
        self.verify_artifact(
            AutonomyArtifactKindV1::AutonomyEpoch,
            &target_epoch_reference_digest(&target_epoch)?,
            &receipt.core.issuer_subject_id,
            &target_epoch.protected_root_signature,
            &target_epoch,
            now_ms,
        )?;
        let mut next = current;
        next.generation = next.generation.saturating_add(1);
        next.independence_spec = target_independence_spec;
        next.constitution = target_constitution;
        next.autonomy_epoch = target_epoch;
        next.active_grants = target_grants;
        next.activation_receipts
            .insert(receipt.core.receipt_id.clone(), receipt);
        next.seal()?;
        self.commit_transition(AutonomyTransitionKindV1::ModeActivated, next, now_ms)
    }

    pub fn consume_autonomy_capability(
        &mut self,
        intent_digest: &str,
        decision: &AuthorityDecisionV1,
        capability: &crate::autonomy::AutonomyCapabilityV1,
        sentinel: Option<&SentinelVerdictV1>,
        now_ms: u64,
    ) -> Result<AutonomyAdmissionReceiptV1, AutonomyRuntimeError> {
        self.ensure_positive_unfrozen()?;
        let current = self.state()?.clone();
        if current
            .consumed_capability_digests
            .contains(&capability.capability_digest)
        {
            return Err(AutonomyRuntimeError::CapabilityReplay {
                capability_digest: capability.capability_digest.clone(),
            });
        }
        let intent = self.resolve_sovereign_intent(intent_digest, now_ms)?;
        if intent.core.organism_id != self.config.organism_id
            || intent.core.repo_id != self.config.repo_id
            || intent.core.brain_id != self.config.brain_id
        {
            return Err(AutonomyRuntimeError::InvalidState {
                reason: "sovereign intent identity differs from the installed autonomy owner"
                    .to_owned(),
            });
        }
        let grant = current
            .active_grants
            .iter()
            .find(|grant| grant.core.grant_id == capability.core.grant_id)
            .ok_or_else(|| AutonomyRuntimeError::InvalidState {
                reason: "capability grant is not active in the protected epoch".to_owned(),
            })?;
        intent.validate(&current.autonomy_epoch, Some(grant), now_ms)?;
        decision.validate_positive(
            &intent,
            &current.constitution,
            &current.kernel,
            sentinel,
            now_ms,
        )?;
        capability.validate(
            decision,
            &intent,
            grant,
            &current.autonomy_epoch,
            sentinel,
            now_ms,
        )?;
        if decision.authority_variant() == AuthorityVariant::AgentQuorum {
            let AuthorityDecisionV1::AgentQuorum(quorum) = decision else {
                unreachable!("authority_variant already proved the enum arm");
            };
            self.validate_quorum_runtime(&quorum.core.quorum, &intent, sentinel, &current, now_ms)?;
        }
        self.verify_artifact(
            AutonomyArtifactKindV1::AuthorityDecision,
            decision.decision_digest(),
            &intent.core.issuer_subject_id,
            authority_decision_signature(decision),
            decision,
            now_ms,
        )?;
        self.verify_artifact(
            AutonomyArtifactKindV1::AutonomyCapability,
            &capability.capability_digest,
            &capability.core.issuer_subject_id,
            &capability.owner_signature,
            capability,
            now_ms,
        )?;
        if let Some(verdict) = sentinel {
            self.verify_artifact_with_identity_policy(
                AutonomyArtifactKindV1::SentinelVerdict,
                &verdict.verdict_digest,
                "pinned-sentinel",
                &verdict.signature,
                verdict,
                Some(
                    &current
                        .kernel
                        .core
                        .sentinel_identity_key_binary_policy_digest,
                ),
                now_ms,
            )?;
        }
        let mut next = current;
        next.generation = next.generation.saturating_add(1);
        next.consumed_capability_digests
            .insert(capability.capability_digest.clone());
        next.seal()?;
        self.commit_transition(AutonomyTransitionKindV1::CapabilityConsumed, next, now_ms)?;
        let state = self.state()?;
        let mut receipt = AutonomyAdmissionReceiptV1 {
            schema: AUTONOMY_ADMISSION_RECEIPT_SCHEMA.to_owned(),
            intent_digest: intent.intent_digest,
            decision_digest: decision.decision_digest().to_owned(),
            capability_digest: capability.capability_digest.clone(),
            authority_variant: decision.authority_variant(),
            committed_state_digest: state.state_digest.clone(),
            protected_root_digest: self
                .protected_root
                .as_ref()
                .expect("committed transition always has a protected root")
                .root_digest
                .clone(),
            receipt_digest: String::new(),
        };
        receipt.seal()?;
        Ok(receipt)
    }

    pub fn persist_sentinel_red(
        &mut self,
        source_intent_digest: &str,
        verdict: SentinelVerdictV1,
        outbox: SentinelRedOutboxV1,
        now_ms: u64,
    ) -> Result<(), AutonomyRuntimeError> {
        self.ensure_usable()?;
        let current = self.state()?.clone();
        if current.autonomy_epoch.safety_state == SafetyState::Frozen {
            return Err(AutonomyRuntimeError::InvalidRedTransition {
                reason: "terminally FROZEN state must recover before accepting another RED"
                    .to_owned(),
            });
        }
        if current.pending_red.is_some() {
            return Err(AutonomyRuntimeError::InvalidRedTransition {
                reason: "an unresolved RED already owns the positive-authority fence".to_owned(),
            });
        }
        let source = self.resolve_sovereign_intent(source_intent_digest, now_ms)?;
        verdict.validate_for_intent(&source, &current.kernel, now_ms)?;
        if verdict.core.verdict != SentinelVerdict::Red {
            return Err(AutonomyRuntimeError::InvalidRedTransition {
                reason: "only a RED verdict enters the durable outbox".to_owned(),
            });
        }
        outbox.validate(&verdict, &source, current.sentinel_outbox_tail.as_ref())?;
        if outbox.core.state != RedOutboxState::Pending {
            return Err(AutonomyRuntimeError::InvalidRedTransition {
                reason: "first delivery record must be PENDING".to_owned(),
            });
        }
        self.verify_artifact_with_identity_policy(
            AutonomyArtifactKindV1::SentinelVerdict,
            &verdict.verdict_digest,
            "pinned-sentinel",
            &verdict.signature,
            &verdict,
            Some(
                &current
                    .kernel
                    .core
                    .sentinel_identity_key_binary_policy_digest,
            ),
            now_ms,
        )?;
        self.verify_artifact_with_identity_policy(
            AutonomyArtifactKindV1::SentinelRedOutbox,
            &outbox.record_digest,
            "pinned-sentinel",
            &outbox.root_signature,
            &outbox,
            Some(
                &current
                    .kernel
                    .core
                    .sentinel_identity_key_binary_policy_digest,
            ),
            now_ms,
        )?;
        let mut next = current;
        next.generation = next.generation.saturating_add(1);
        next.autonomy_epoch.issuance_frozen = true;
        next.autonomy_epoch.safety_state = SafetyState::PendingRed;
        next.sentinel_outbox_tail = Some(outbox.clone());
        next.pending_red = Some(PendingRedRuntimeV1 {
            schema: PENDING_RED_RUNTIME_SCHEMA.to_owned(),
            source_intent_digest: source_intent_digest.to_owned(),
            verdict,
            outbox,
            latch: None,
        });
        next.seal()?;
        self.commit_transition(AutonomyTransitionKindV1::SentinelRedPersisted, next, now_ms)
    }

    pub fn latch_sentinel_red(
        &mut self,
        acknowledged_outbox: SentinelRedOutboxV1,
        latch: RedLatchReceiptV1,
        now_ms: u64,
    ) -> Result<(), AutonomyRuntimeError> {
        self.ensure_usable()?;
        let current = self.state()?.clone();
        let pending = current
            .pending_red
            .as_ref()
            .ok_or(AutonomyRuntimeError::NoPendingRed)?;
        if pending.latch.is_some() {
            return Err(AutonomyRuntimeError::InvalidRedTransition {
                reason: "RED is already latched".to_owned(),
            });
        }
        let source = self.resolve_sovereign_intent(&pending.source_intent_digest, now_ms)?;
        acknowledged_outbox.validate(
            &pending.verdict,
            &source,
            current.sentinel_outbox_tail.as_ref(),
        )?;
        if acknowledged_outbox.core.state != RedOutboxState::LatchAcknowledged {
            return Err(AutonomyRuntimeError::InvalidRedTransition {
                reason: "owner latch append requires a LATCH_ACKNOWLEDGED outbox record".to_owned(),
            });
        }
        latch.validate(
            &acknowledged_outbox,
            &pending.verdict,
            &source,
            &current.kernel,
        )?;
        if latch.core.state != RedLatchState::Pending {
            return Err(AutonomyRuntimeError::InvalidRedTransition {
                reason: "new RED latch must be PENDING".to_owned(),
            });
        }
        self.verify_artifact_with_identity_policy(
            AutonomyArtifactKindV1::SentinelRedOutbox,
            &acknowledged_outbox.record_digest,
            "pinned-sentinel",
            &acknowledged_outbox.root_signature,
            &acknowledged_outbox,
            Some(
                &current
                    .kernel
                    .core
                    .sentinel_identity_key_binary_policy_digest,
            ),
            now_ms,
        )?;
        self.verify_artifact_with_identity_policy(
            AutonomyArtifactKindV1::RedLatchReceipt,
            &latch.latch_receipt_digest,
            "safety-kernel",
            &latch.owner_kernel_signature,
            &latch,
            Some(&current.kernel.kernel_digest),
            now_ms,
        )?;
        let mut next = current;
        next.generation = next.generation.saturating_add(1);
        next.sentinel_outbox_tail = Some(acknowledged_outbox.clone());
        let pending = next.pending_red.as_mut().expect("cloned pending RED");
        pending.outbox = acknowledged_outbox;
        next.last_red_latch_epoch = latch.core.latch_epoch;
        pending.latch = Some(latch);
        next.seal()?;
        self.commit_transition(AutonomyTransitionKindV1::RedLatched, next, now_ms)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn commit_red_safety_transition(
        &mut self,
        safety_intent_digest: &str,
        capability: &SafetyCapabilityV1,
        decision: &AuthorityDecisionV1,
        terminal_outbox: SentinelRedOutboxV1,
        terminal_latch: RedLatchReceiptV1,
        terminal_transaction_id: &str,
        now_ms: u64,
    ) -> Result<(), AutonomyRuntimeError> {
        self.ensure_usable()?;
        require_non_empty("terminal_transaction_id", terminal_transaction_id)?;
        let current = self.state()?.clone();
        let pending = current
            .pending_red
            .as_ref()
            .ok_or(AutonomyRuntimeError::NoPendingRed)?;
        let pending_latch = pending
            .latch
            .as_ref()
            .ok_or(AutonomyRuntimeError::NoPendingRed)?;
        if pending_latch.core.state != RedLatchState::Pending {
            return Err(AutonomyRuntimeError::InvalidRedTransition {
                reason: "only a PENDING latch can linearize a safety transaction".to_owned(),
            });
        }
        let source = self.resolve_sovereign_intent(&pending.source_intent_digest, now_ms)?;
        let safety_intent = self.resolve_safety_intent(safety_intent_digest)?;
        safety_intent.validate(
            &source,
            &pending.verdict,
            pending_latch,
            &current.kernel,
            current.autonomy_epoch.constitution_epoch,
            current.autonomy_epoch.autonomy_epoch,
        )?;
        capability.validate(&safety_intent, pending_latch, &current.kernel, now_ms)?;
        decision.validate_safety(&safety_intent, capability, pending_latch, &current.kernel)?;
        if current
            .consumed_capability_digests
            .contains(&capability.capability_digest)
        {
            return Err(AutonomyRuntimeError::CapabilityReplay {
                capability_digest: capability.capability_digest.clone(),
            });
        }
        terminal_outbox.validate(
            &pending.verdict,
            &source,
            current.sentinel_outbox_tail.as_ref(),
        )?;
        if terminal_outbox.core.state != RedOutboxState::Terminal
            || terminal_outbox
                .core
                .terminal_safety_transaction_id
                .as_deref()
                != Some(terminal_transaction_id)
        {
            return Err(AutonomyRuntimeError::InvalidRedTransition {
                reason: "terminal outbox does not acknowledge the winning safety transaction"
                    .to_owned(),
            });
        }
        terminal_latch.validate(&terminal_outbox, &pending.verdict, &source, &current.kernel)?;
        let expected_marker = red_commit_marker_digest(
            terminal_transaction_id,
            decision.decision_digest(),
            &current.state_digest,
            current.autonomy_epoch.autonomy_epoch.saturating_add(1),
        )?;
        if terminal_latch.core.state != RedLatchState::Terminal
            || terminal_latch.core.committing_transaction_id.as_deref()
                != Some(terminal_transaction_id)
            || terminal_latch
                .core
                .terminal_safety_transaction_id
                .as_deref()
                != Some(terminal_transaction_id)
            || terminal_latch.core.commit_marker_digest.as_deref() != Some(expected_marker.as_str())
            || !same_red_mandate(pending_latch, &terminal_latch)
        {
            return Err(AutonomyRuntimeError::InvalidRedTransition {
                reason: "terminal latch changes the immutable RED mandate or commit claim"
                    .to_owned(),
            });
        }
        if current
            .terminal_red_latches
            .contains_key(&terminal_latch.core.latch_epoch.to_string())
        {
            return Err(AutonomyRuntimeError::InvalidRedTransition {
                reason: "terminal RED latch epoch is already committed".to_owned(),
            });
        }
        self.verify_artifact_with_identity_policy(
            AutonomyArtifactKindV1::SafetyCapability,
            &capability.capability_digest,
            "pinned-safety-actuator",
            &capability.actuator_signature,
            capability,
            Some(
                &current
                    .kernel
                    .core
                    .safety_actuator_identity_key_binary_policy_digest,
            ),
            now_ms,
        )?;
        self.verify_artifact_with_identity_policy(
            AutonomyArtifactKindV1::AuthorityDecision,
            decision.decision_digest(),
            "safety-kernel",
            authority_decision_signature(decision),
            decision,
            Some(&current.kernel.kernel_digest),
            now_ms,
        )?;
        self.verify_artifact_with_identity_policy(
            AutonomyArtifactKindV1::SentinelRedOutbox,
            &terminal_outbox.record_digest,
            "pinned-sentinel",
            &terminal_outbox.root_signature,
            &terminal_outbox,
            Some(
                &current
                    .kernel
                    .core
                    .sentinel_identity_key_binary_policy_digest,
            ),
            now_ms,
        )?;
        self.verify_artifact_with_identity_policy(
            AutonomyArtifactKindV1::RedLatchReceipt,
            &terminal_latch.latch_receipt_digest,
            "safety-kernel",
            &terminal_latch.owner_kernel_signature,
            &terminal_latch,
            Some(&current.kernel.kernel_digest),
            now_ms,
        )?;
        let frozen_from_mode = current.autonomy_epoch.active_mode;
        let red_lineage_receipt_id = format!("red-latch:{}", terminal_latch.latch_receipt_digest);
        let mut next = current;
        next.generation = next.generation.saturating_add(1);
        next.consumed_capability_digests
            .insert(capability.capability_digest.clone());
        next.autonomy_epoch.autonomy_epoch = next.autonomy_epoch.autonomy_epoch.saturating_add(1);
        next.autonomy_epoch.active_mode = ActiveMode::HumanGated;
        next.autonomy_epoch.activation_receipt_id = Some(red_lineage_receipt_id);
        next.autonomy_epoch.issuance_frozen = true;
        next.autonomy_epoch.safety_state = SafetyState::Frozen;
        next.active_grants.clear();
        next.autonomy_epoch.grants_digest = compute_grants_digest(&[])?;
        next.sentinel_outbox_tail = Some(terminal_outbox);
        next.last_red_latch_epoch = terminal_latch.core.latch_epoch;
        next.terminal_red_latches
            .insert(terminal_latch.core.latch_epoch.to_string(), terminal_latch);
        next.last_valid_mode_before_freeze = Some(frozen_from_mode);
        next.pending_red = None;
        next.seal()?;
        self.commit_transition(AutonomyTransitionKindV1::RedSafetyCommitted, next, now_ms)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn recover_from_frozen(
        &mut self,
        recovery_intent_digest: &str,
        authority_decision: &AuthorityDecisionV1,
        sentinel: &SentinelVerdictV1,
        receipt: AutonomyRecoveryReceiptV1,
        target_epoch: AutonomyEpochV1,
        now_ms: u64,
    ) -> Result<(), AutonomyRuntimeError> {
        self.ensure_usable()?;
        let current = self.state()?.clone();
        if current
            .recovery_receipts
            .contains_key(&receipt.core.receipt_id)
        {
            return Err(AutonomyRuntimeError::RecoveryReplay {
                receipt_id: receipt.core.receipt_id.clone(),
            });
        }
        let intent = self.resolve_sovereign_intent(recovery_intent_digest, now_ms)?;
        self.validate_recovery_intent_shape(&current, &intent)?;
        sentinel.validate_for_intent(&intent, &current.kernel, now_ms)?;
        if sentinel.core.verdict != SentinelVerdict::Green {
            return Err(AutonomyRuntimeError::InvalidRecovery {
                reason: "recovery requires a fresh exact GREEN sentinel verdict".to_owned(),
            });
        }
        authority_decision.validate_positive(
            &intent,
            &current.constitution,
            &current.kernel,
            Some(sentinel),
            now_ms,
        )?;
        if authority_decision.authority_variant() == AuthorityVariant::AgentQuorum {
            let AuthorityDecisionV1::AgentQuorum(quorum) = authority_decision else {
                unreachable!("authority_variant already proved the enum arm");
            };
            self.validate_quorum_runtime(
                &quorum.core.quorum,
                &intent,
                Some(sentinel),
                &current,
                now_ms,
            )?;
        }
        self.validate_recovery_receipt(
            &current,
            &intent,
            authority_decision,
            sentinel,
            &receipt,
            &target_epoch,
            now_ms,
        )?;
        self.verify_artifact(
            AutonomyArtifactKindV1::AuthorityDecision,
            authority_decision.decision_digest(),
            &receipt.core.issuer_subject_id,
            authority_decision_signature(authority_decision),
            authority_decision,
            now_ms,
        )?;
        self.verify_artifact_with_identity_policy(
            AutonomyArtifactKindV1::SentinelVerdict,
            &sentinel.verdict_digest,
            "pinned-sentinel",
            &sentinel.signature,
            sentinel,
            Some(
                &current
                    .kernel
                    .core
                    .sentinel_identity_key_binary_policy_digest,
            ),
            now_ms,
        )?;
        self.verify_artifact(
            AutonomyArtifactKindV1::RecoveryReceipt,
            &receipt.receipt_digest,
            &receipt.core.issuer_subject_id,
            &receipt.signature,
            &receipt,
            now_ms,
        )?;
        self.verify_artifact(
            AutonomyArtifactKindV1::AutonomyEpoch,
            &target_epoch_reference_digest(&target_epoch)?,
            &receipt.core.issuer_subject_id,
            &target_epoch.protected_root_signature,
            &target_epoch,
            now_ms,
        )?;
        let mut next = current;
        next.generation = next.generation.saturating_add(1);
        next.autonomy_epoch = target_epoch;
        next.active_grants.clear();
        next.tier_evidence.clear();
        next.last_valid_mode_before_freeze = None;
        next.recovery_receipts
            .insert(receipt.core.receipt_id.clone(), receipt);
        next.seal()?;
        self.commit_transition(AutonomyTransitionKindV1::RecoveredFromFrozen, next, now_ms)
    }

    fn validate_quorum_runtime(
        &self,
        evidence: &AgentQuorumDecisionEvidenceV1,
        intent: &SovereignActionIntentV1,
        sentinel: Option<&SentinelVerdictV1>,
        state: &AutonomyRuntimeStateV1,
        now_ms: u64,
    ) -> Result<(), AutonomyRuntimeError> {
        let sentinel = sentinel.ok_or_else(|| AutonomyRuntimeError::InvalidState {
            reason: "agent quorum path requires the exact GREEN sentinel verdict".to_owned(),
        })?;
        evidence.validate(
            intent,
            &state.constitution,
            &state.kernel,
            &sentinel.verdict_digest,
        )?;
        for vote in &evidence.votes {
            let material = QuorumVoteVerificationMaterialV1::from(vote);
            let vote_digest = digest_canonical(QUORUM_VOTE_VERIFICATION_DIGEST_DOMAIN, &material)?;
            self.verify_artifact(
                AutonomyArtifactKindV1::QuorumVote,
                &vote_digest,
                &vote.verifier_principal_id,
                &vote.signature,
                &material,
                now_ms,
            )?;
        }
        Ok(())
    }

    fn validate_recovery_intent_shape(
        &self,
        state: &AutonomyRuntimeStateV1,
        intent: &SovereignActionIntentV1,
    ) -> Result<(), AutonomyRuntimeError> {
        if state.autonomy_epoch.safety_state != SafetyState::Frozen
            || !state.autonomy_epoch.issuance_frozen
            || state.pending_red.is_some()
            || !state.active_grants.is_empty()
        {
            return Err(AutonomyRuntimeError::InvalidRecovery {
                reason: "recovery intent requires terminal FROZEN state with zero grants"
                    .to_owned(),
            });
        }
        let last_mode = state.last_valid_mode_before_freeze.ok_or_else(|| {
            AutonomyRuntimeError::InvalidRecovery {
                reason: "last valid authority mode was not preserved at freeze".to_owned(),
            }
        })?;
        if !state
            .terminal_red_latches
            .contains_key(&state.last_red_latch_epoch.to_string())
        {
            return Err(AutonomyRuntimeError::InvalidRecovery {
                reason: "latest terminal RED latch is not durably retained".to_owned(),
            });
        }
        let required_authority = recovery_authority_for_mode(last_mode);
        if intent.core.action_class != "autonomy.recover"
            || intent.core.active_mode != last_mode
            || intent.core.required_authority_variant != required_authority
            || intent.core.constitution_digest != state.constitution.constitution_digest
            || intent.core.constitution_epoch != state.autonomy_epoch.constitution_epoch
            || intent.core.autonomy_epoch != state.autonomy_epoch.autonomy_epoch
            || intent.core.issuer_subject_id != intent.core.decision_subject_id
            || intent.core.caller_subject_id != intent.core.decision_subject_id
            || intent.core.applicable_grant_id.is_some()
            || intent.core.applicable_grant_digest.is_some()
            || intent.core.promotion_target_subject_id.is_some()
            || intent.core.ratification_target_subject_id.is_some()
            || intent.core.promotion_subject_id.is_some()
        {
            return Err(AutonomyRuntimeError::InvalidRecovery {
                reason: "recovery intent does not bind the last authority and exact frozen epoch"
                    .to_owned(),
            });
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn validate_recovery_receipt(
        &self,
        state: &AutonomyRuntimeStateV1,
        intent: &SovereignActionIntentV1,
        decision: &AuthorityDecisionV1,
        sentinel: &SentinelVerdictV1,
        receipt: &AutonomyRecoveryReceiptV1,
        target_epoch: &AutonomyEpochV1,
        now_ms: u64,
    ) -> Result<(), AutonomyRuntimeError> {
        if receipt.schema != AUTONOMY_RECOVERY_RECEIPT_SCHEMA {
            return Err(AutonomyRuntimeError::InvalidRecovery {
                reason: format!("unsupported recovery receipt schema '{}'", receipt.schema),
            });
        }
        require_non_empty("recovery.receipt_id", &receipt.core.receipt_id)?;
        require_non_empty(
            "recovery.issuer_subject_id",
            &receipt.core.issuer_subject_id,
        )?;
        for (field, value) in [
            (
                "recovery.frozen_state_digest",
                &receipt.core.frozen_state_digest,
            ),
            (
                "recovery.frozen_epoch_reference_digest",
                &receipt.core.frozen_epoch_reference_digest,
            ),
            (
                "recovery.recovery_intent_digest",
                &receipt.core.recovery_intent_digest,
            ),
            (
                "recovery.authority_decision_digest",
                &receipt.core.authority_decision_digest,
            ),
            (
                "recovery.sentinel_verdict_digest",
                &receipt.core.sentinel_verdict_digest,
            ),
            (
                "recovery.terminal_red_latch_receipt_digest",
                &receipt.core.terminal_red_latch_receipt_digest,
            ),
            (
                "recovery.remediation_evidence_digest",
                &receipt.core.remediation_evidence_digest,
            ),
            (
                "recovery.rollback_validation_digest",
                &receipt.core.rollback_validation_digest,
            ),
            (
                "recovery.target_constitution_digest",
                &receipt.core.target_constitution_digest,
            ),
            ("recovery.receipt_digest", &receipt.receipt_digest),
        ] {
            require_digest(field, value)?;
        }
        if receipt.signature.is_empty() || receipt.core.issued_at > now_ms {
            return Err(AutonomyRuntimeError::InvalidRecovery {
                reason: "recovery receipt is unsigned or not yet effective".to_owned(),
            });
        }
        let computed = receipt.compute_digest()?;
        if computed != receipt.receipt_digest
            || receipt.core.receipt_id != format!("autonomy-recovery:{computed}")
        {
            return Err(AutonomyRuntimeError::InvalidRecovery {
                reason: "recovery receipt id/digest mismatch".to_owned(),
            });
        }
        let last_mode = state.last_valid_mode_before_freeze.ok_or_else(|| {
            AutonomyRuntimeError::InvalidRecovery {
                reason: "last valid mode is absent".to_owned(),
            }
        })?;
        let terminal_latch = state
            .terminal_red_latches
            .get(&state.last_red_latch_epoch.to_string())
            .ok_or_else(|| AutonomyRuntimeError::InvalidRecovery {
                reason: "latest terminal RED latch is absent".to_owned(),
            })?;
        let expected_target_epoch = state
            .autonomy_epoch
            .autonomy_epoch
            .checked_add(1)
            .ok_or_else(|| AutonomyRuntimeError::InvalidRecovery {
                reason: "autonomy epoch overflow".to_owned(),
            })?;
        if terminal_latch.core.state != RedLatchState::Terminal
            || receipt.core.frozen_state_digest != state.state_digest
            || receipt.core.frozen_epoch_reference_digest
                != target_epoch_reference_digest(&state.autonomy_epoch)?
            || receipt.core.frozen_autonomy_epoch != state.autonomy_epoch.autonomy_epoch
            || receipt.core.last_valid_mode != last_mode
            || receipt.core.required_authority_variant != recovery_authority_for_mode(last_mode)
            || decision.authority_variant() != receipt.core.required_authority_variant
            || receipt.core.recovery_intent_digest != intent.intent_digest
            || receipt.core.authority_decision_digest != decision.decision_digest()
            || receipt.core.sentinel_verdict_digest != sentinel.verdict_digest
            || receipt.core.terminal_red_latch_receipt_digest != terminal_latch.latch_receipt_digest
            || receipt.core.remediation_evidence_digest != intent.core.action_payload_digest
            || receipt.core.remediation_evidence_digest != intent.core.evidence_digest
            || receipt.core.rollback_validation_digest != intent.core.rollback_plan_digest
            || receipt.core.target_autonomy_epoch != expected_target_epoch
            || receipt.core.target_constitution_digest != state.constitution.constitution_digest
            || receipt.core.target_constitution_epoch != state.constitution.core.constitution_epoch
            || receipt.core.issuer_subject_id != intent.core.issuer_subject_id
            || receipt.core.issuer_subject_id != intent.core.decision_subject_id
        {
            return Err(AutonomyRuntimeError::InvalidRecovery {
                reason: "recovery receipt does not bind the frozen state, last authority, evidence, and target"
                    .to_owned(),
            });
        }
        if target_epoch.autonomy_epoch != expected_target_epoch
            || target_epoch.active_mode != ActiveMode::HumanGated
            || target_epoch.activation_receipt_id.as_deref()
                != Some(receipt.core.receipt_id.as_str())
            || target_epoch.constitution_digest != state.constitution.constitution_digest
            || target_epoch.constitution_epoch != state.constitution.core.constitution_epoch
            || target_epoch.grants_digest != compute_grants_digest(&[])?
            || target_epoch.issuance_frozen
            || target_epoch.safety_state != SafetyState::Healthy
        {
            return Err(AutonomyRuntimeError::InvalidRecovery {
                reason: "recovery target must be the exact next HUMAN_GATED/A0 healthy epoch"
                    .to_owned(),
            });
        }
        target_epoch.validate_common(&state.constitution, &[], now_ms)?;
        Ok(())
    }

    fn validate_target_grant_evidence(
        &self,
        current: &AutonomyRuntimeStateV1,
        target_grants: &[AutonomyGrantV1],
        exact_candidate: &str,
        receipt: &AutonomyActivationReceiptV1,
    ) -> Result<(), AutonomyRuntimeError> {
        let evidence_set_digest = compute_tier_evidence_set_digest(&current.tier_evidence)?;
        if evidence_set_digest != receipt.core.g9_canary_receipts_digest {
            return Err(AutonomyRuntimeError::MissingPromotionEvidence {
                reason: "activation receipt does not bind the exact accumulated G9 evidence set"
                    .to_owned(),
            });
        }
        for grant in target_grants {
            let evidence = current
                .tier_evidence
                .get(&grant.core.subject_id)
                .and_then(|lane| {
                    lane.iter()
                        .find(|evidence| evidence.tier == grant.core.max_tier)
                })
                .ok_or_else(|| AutonomyRuntimeError::MissingPromotionEvidence {
                    reason: format!(
                        "subject '{}' lacks {:?} evidence",
                        grant.core.subject_id, grant.core.max_tier
                    ),
                })?;
            if evidence.exact_release_candidate_digest != exact_candidate
                || grant.core.promotion_receipt_id != evidence.evidence_digest
            {
                return Err(AutonomyRuntimeError::MissingPromotionEvidence {
                    reason: format!(
                        "grant '{}' is not bound to exact candidate/evidence",
                        grant.core.grant_id
                    ),
                });
            }
            match grant.core.mode {
                ActiveMode::HumanGated if grant.core.max_tier > AutonomyTier::A1Propose => {
                    return Err(AutonomyRuntimeError::MissingPromotionEvidence {
                        reason: "HUMAN_GATED cannot activate execution tiers".to_owned(),
                    });
                }
                ActiveMode::PolicyAutonomous
                    if grant.core.max_tier > AutonomyTier::A3AutonomousLand =>
                {
                    return Err(AutonomyRuntimeError::MissingPromotionEvidence {
                        reason: "POLICY_AUTONOMOUS is capped at A3 in this ratified rollout"
                            .to_owned(),
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn persist_intent_object<T: Serialize>(
        &self,
        kind: StoredIntentKindV1,
        intent_digest: &str,
        content_address: &str,
        intent: &T,
    ) -> Result<IntentCoreEntryV1, AutonomyRuntimeError> {
        require_digest("intent_digest", intent_digest)?;
        let bytes = canonical_json(intent)?;
        if bytes.len() as u64 > self.config.max_intent_bytes {
            return Err(AutonomyRuntimeError::IntentTooLarge {
                observed: bytes.len() as u64,
                limit: self.config.max_intent_bytes,
            });
        }
        let object_digest = digest_domain_bytes(INTENT_OBJECT_DIGEST_DOMAIN, &bytes);
        let path = self.intent_object_path(intent_digest)?;
        if path.exists() {
            refuse_symlink_if_present(&path)?;
            let existing = read_limited(&path, self.config.max_intent_bytes)?;
            if existing != bytes {
                return Err(AutonomyRuntimeError::IntentCollision {
                    intent_digest: intent_digest.to_owned(),
                });
            }
        } else {
            write_atomic_fsynced(&path, &bytes)?;
            sync_directory(&self.object_directory, "sync_intent_object_directory")?;
        }
        Ok(IntentCoreEntryV1 {
            schema: INTENT_CORE_ENTRY_SCHEMA.to_owned(),
            intent_digest: intent_digest.to_owned(),
            content_address: content_address.to_owned(),
            canonicalization_version: CANONICALIZATION_VERSION.to_owned(),
            kind,
            canonical_bytes_digest: object_digest,
            byte_len: bytes.len() as u64,
        })
    }

    fn index_persisted_intent(
        &mut self,
        entry: IntentCoreEntryV1,
        now_ms: u64,
    ) -> Result<(), AutonomyRuntimeError> {
        let mut next = self.state()?.clone();
        if let Some(existing) = next.intent_index.get(&entry.intent_digest) {
            if existing != &entry {
                return Err(AutonomyRuntimeError::IntentCollision {
                    intent_digest: entry.intent_digest,
                });
            }
            return Ok(());
        }
        next.generation = next.generation.saturating_add(1);
        next.intent_index.insert(entry.intent_digest.clone(), entry);
        next.seal()?;
        self.commit_transition(AutonomyTransitionKindV1::IntentStored, next, now_ms)
    }

    fn resolve_intent_bytes(
        &self,
        intent_digest: &str,
        expected_kind: StoredIntentKindV1,
    ) -> Result<Vec<u8>, AutonomyRuntimeError> {
        let state = self.state()?;
        let entry = state.intent_index.get(intent_digest).ok_or_else(|| {
            AutonomyRuntimeError::IntentMissing {
                intent_digest: intent_digest.to_owned(),
            }
        })?;
        if entry.kind != expected_kind {
            return Err(AutonomyRuntimeError::IntentCorrupt {
                intent_digest: intent_digest.to_owned(),
                reason: "stored intent kind mismatch".to_owned(),
            });
        }
        let path = self.intent_object_path(intent_digest)?;
        let bytes = read_limited(&path, self.config.max_intent_bytes)?;
        if bytes.len() as u64 != entry.byte_len
            || digest_domain_bytes(INTENT_OBJECT_DIGEST_DOMAIN, &bytes)
                != entry.canonical_bytes_digest
        {
            return Err(AutonomyRuntimeError::IntentCorrupt {
                intent_digest: intent_digest.to_owned(),
                reason: "byte length or content digest mismatch".to_owned(),
            });
        }
        Ok(bytes)
    }

    fn intent_object_path(&self, digest: &str) -> Result<PathBuf, AutonomyRuntimeError> {
        require_digest("intent_digest", digest)?;
        Ok(self.object_directory.join(format!("{digest}.json")))
    }

    fn verify_all_intent_objects(
        &self,
        state: &AutonomyRuntimeStateV1,
        now_ms: u64,
    ) -> Result<(), AutonomyRuntimeError> {
        for (digest, entry) in &state.intent_index {
            match entry.kind {
                StoredIntentKindV1::Sovereign => {
                    self.resolve_sovereign_intent(digest, now_ms)?;
                }
                StoredIntentKindV1::Safety => {
                    self.resolve_safety_intent(digest)?;
                }
            }
        }
        Ok(())
    }

    fn ensure_usable(&self) -> Result<(), AutonomyRuntimeError> {
        if self.poisoned {
            Err(AutonomyRuntimeError::Poisoned)
        } else {
            Ok(())
        }
    }

    fn ensure_positive_unfrozen(&self) -> Result<(), AutonomyRuntimeError> {
        self.ensure_usable()?;
        let state = self.state()?;
        if state.autonomy_epoch.issuance_frozen
            || state.autonomy_epoch.safety_state != SafetyState::Healthy
            || state.pending_red.is_some()
        {
            return Err(AutonomyRuntimeError::PositiveAuthorityFrozen);
        }
        Ok(())
    }

    fn validate_state(
        &self,
        state: &AutonomyRuntimeStateV1,
        now_ms: u64,
    ) -> Result<(), AutonomyRuntimeError> {
        if state.schema != AUTONOMY_RUNTIME_STATE_SCHEMA {
            return Err(AutonomyRuntimeError::InvalidState {
                reason: format!("unsupported schema '{}'", state.schema),
            });
        }
        if state.durability_domain_id != self.config.durability_domain_id {
            return Err(AutonomyRuntimeError::InvalidState {
                reason: "durability-domain binding mismatch".to_owned(),
            });
        }
        state.kernel.validate()?;
        state
            .constitution
            .validate(&state.independence_spec, &state.kernel, now_ms)?;
        state
            .autonomy_epoch
            .validate_common(&state.constitution, &state.active_grants, now_ms)?;
        let intent_root = compute_intent_store_root(&state.intent_index)?;
        if intent_root != state.intent_store_root_digest {
            return Err(AutonomyRuntimeError::InvalidState {
                reason: "intent-store root digest mismatch".to_owned(),
            });
        }
        for (map_digest, entry) in &state.intent_index {
            if entry.schema != INTENT_CORE_ENTRY_SCHEMA
                || map_digest != &entry.intent_digest
                || entry.canonicalization_version != CANONICALIZATION_VERSION
            {
                return Err(AutonomyRuntimeError::InvalidState {
                    reason: "intent index key/schema/canonicalization mismatch".to_owned(),
                });
            }
            require_digest("intent_entry.intent_digest", &entry.intent_digest)?;
            require_digest(
                "intent_entry.canonical_bytes_digest",
                &entry.canonical_bytes_digest,
            )?;
        }
        for (subject, lane) in &state.tier_evidence {
            if lane.is_empty() {
                return Err(AutonomyRuntimeError::InvalidState {
                    reason: format!("empty tier-evidence lane for '{subject}'"),
                });
            }
            let mut previous = None;
            for evidence in lane {
                if evidence.subject_id != *subject {
                    return Err(AutonomyRuntimeError::InvalidState {
                        reason: "tier-evidence lane subject mismatch".to_owned(),
                    });
                }
                evidence.validate(previous)?;
                previous = Some(evidence);
            }
        }
        for (receipt_id, receipt) in &state.activation_receipts {
            if receipt_id != &receipt.core.receipt_id {
                return Err(AutonomyRuntimeError::InvalidState {
                    reason: "activation receipt map key mismatch".to_owned(),
                });
            }
        }
        for (receipt_id, receipt) in &state.recovery_receipts {
            if receipt_id != &receipt.core.receipt_id
                || receipt.schema != AUTONOMY_RECOVERY_RECEIPT_SCHEMA
                || receipt.compute_digest()? != receipt.receipt_digest
                || receipt.core.receipt_id
                    != format!("autonomy-recovery:{}", receipt.receipt_digest)
                || receipt.signature.is_empty()
            {
                return Err(AutonomyRuntimeError::InvalidState {
                    reason: "recovery receipt map contains an invalid receipt".to_owned(),
                });
            }
        }
        let mut maximum_latch_epoch = 0;
        for (latch_epoch, latch) in &state.terminal_red_latches {
            let parsed_epoch =
                latch_epoch
                    .parse::<u64>()
                    .map_err(|_| AutonomyRuntimeError::InvalidState {
                        reason: "terminal RED latch map key is not a canonical epoch".to_owned(),
                    })?;
            maximum_latch_epoch = maximum_latch_epoch.max(parsed_epoch);
            if parsed_epoch != latch.core.latch_epoch
                || latch.core.state != RedLatchState::Terminal
                || latch.owner_kernel_signature.is_empty()
            {
                return Err(AutonomyRuntimeError::InvalidState {
                    reason: "terminal RED latch history is invalid".to_owned(),
                });
            }
            require_digest(
                "terminal_red_latch.receipt_digest",
                &latch.latch_receipt_digest,
            )?;
        }
        match &state.pending_red {
            None => {
                if state.autonomy_epoch.safety_state == SafetyState::PendingRed {
                    return Err(AutonomyRuntimeError::InvalidState {
                        reason: "PENDING_RED epoch lacks its durable RED pipeline".to_owned(),
                    });
                }
            }
            Some(pending) => {
                if pending.schema != PENDING_RED_RUNTIME_SCHEMA
                    || pending.verdict.core.verdict != SentinelVerdict::Red
                    || pending.source_intent_digest != pending.verdict.core.intent_digest
                    || state.sentinel_outbox_tail.as_ref() != Some(&pending.outbox)
                    || !state.autonomy_epoch.issuance_frozen
                    || state.autonomy_epoch.safety_state != SafetyState::PendingRed
                {
                    return Err(AutonomyRuntimeError::InvalidState {
                        reason: "pending RED pipeline is not exactly bound to the epoch/outbox"
                            .to_owned(),
                    });
                }
                if let Some(latch) = &pending.latch {
                    maximum_latch_epoch = maximum_latch_epoch.max(latch.core.latch_epoch);
                    if state.last_red_latch_epoch != latch.core.latch_epoch {
                        return Err(AutonomyRuntimeError::InvalidState {
                            reason: "protected RED latch watermark differs from pending latch"
                                .to_owned(),
                        });
                    }
                }
            }
        }
        if state.last_red_latch_epoch != maximum_latch_epoch {
            return Err(AutonomyRuntimeError::InvalidState {
                reason: "RED latch watermark does not equal the latest durable latch".to_owned(),
            });
        }
        match state.last_valid_mode_before_freeze {
            Some(_)
                if state.autonomy_epoch.safety_state != SafetyState::Frozen
                    || state.pending_red.is_some()
                    || !state
                        .terminal_red_latches
                        .contains_key(&state.last_red_latch_epoch.to_string()) =>
            {
                return Err(AutonomyRuntimeError::InvalidState {
                    reason: "last valid mode exists outside a terminal FROZEN state".to_owned(),
                });
            }
            Some(_) => {}
            None if state.autonomy_epoch.safety_state == SafetyState::Frozen => {
                return Err(AutonomyRuntimeError::InvalidState {
                    reason: "terminal FROZEN state lost its last valid authority mode".to_owned(),
                });
            }
            None => {}
        }
        if state.autonomy_epoch.autonomy_epoch > 0 {
            let lineage_id = state
                .autonomy_epoch
                .activation_receipt_id
                .as_deref()
                .ok_or_else(|| AutonomyRuntimeError::InvalidState {
                    reason: "non-bootstrap epoch lacks an authority-lineage receipt".to_owned(),
                })?;
            let recognized = state.activation_receipts.contains_key(lineage_id)
                || state.recovery_receipts.contains_key(lineage_id)
                || state
                    .terminal_red_latches
                    .values()
                    .any(|latch| lineage_id == format!("red-latch:{}", latch.latch_receipt_digest));
            if !recognized {
                return Err(AutonomyRuntimeError::InvalidState {
                    reason: "active epoch authority-lineage receipt is not retained".to_owned(),
                });
            }
        }
        require_digest("runtime.state_digest", &state.state_digest)?;
        if state.compute_state_digest()? != state.state_digest {
            return Err(AutonomyRuntimeError::InvalidState {
                reason: "state self-digest mismatch".to_owned(),
            });
        }
        Ok(())
    }

    fn verify_state_artifacts(
        &self,
        state: &AutonomyRuntimeStateV1,
        now_ms: u64,
    ) -> Result<(), AutonomyRuntimeError> {
        self.verify_artifact(
            AutonomyArtifactKindV1::SafetyKernel,
            &state.kernel.kernel_digest,
            &state.kernel.core.pinned_external_root_key,
            &state.kernel.external_root_signature,
            &state.kernel,
            now_ms,
        )?;
        self.verify_artifact(
            AutonomyArtifactKindV1::Constitution,
            &state.constitution.constitution_digest,
            &state.constitution.core.issuer_subject_id,
            &state.constitution.signature,
            &state.constitution,
            now_ms,
        )?;
        self.verify_artifact(
            AutonomyArtifactKindV1::AutonomyEpoch,
            &target_epoch_reference_digest(&state.autonomy_epoch)?,
            &state.constitution.core.issuer_subject_id,
            &state.autonomy_epoch.protected_root_signature,
            &state.autonomy_epoch,
            now_ms,
        )?;
        for grant in &state.active_grants {
            self.verify_artifact(
                AutonomyArtifactKindV1::AutonomyGrant,
                &grant.grant_digest,
                &grant.core.subject_id,
                &grant.owner_signature,
                grant,
                now_ms,
            )?;
        }
        for receipt in state.activation_receipts.values() {
            self.verify_artifact(
                AutonomyArtifactKindV1::ActivationReceipt,
                &receipt.receipt_digest,
                &receipt.core.issuer_subject_id,
                &receipt.signature,
                receipt,
                now_ms,
            )?;
        }
        for receipt in state.recovery_receipts.values() {
            self.verify_artifact(
                AutonomyArtifactKindV1::RecoveryReceipt,
                &receipt.receipt_digest,
                &receipt.core.issuer_subject_id,
                &receipt.signature,
                receipt,
                now_ms,
            )?;
        }
        for lane in state.tier_evidence.values() {
            for evidence in lane {
                self.verify_artifact(
                    AutonomyArtifactKindV1::TierEvidence,
                    &evidence.evidence_digest,
                    &evidence.evaluator_subject_id,
                    &evidence.evaluator_signature,
                    evidence,
                    now_ms,
                )?;
            }
        }
        if let Some(outbox) = &state.sentinel_outbox_tail {
            self.verify_artifact_with_identity_policy(
                AutonomyArtifactKindV1::SentinelRedOutbox,
                &outbox.record_digest,
                "pinned-sentinel",
                &outbox.root_signature,
                outbox,
                Some(&state.kernel.core.sentinel_identity_key_binary_policy_digest),
                now_ms,
            )?;
        }
        for latch in state.terminal_red_latches.values() {
            self.verify_artifact_with_identity_policy(
                AutonomyArtifactKindV1::RedLatchReceipt,
                &latch.latch_receipt_digest,
                "safety-kernel",
                &latch.owner_kernel_signature,
                latch,
                Some(&state.kernel.kernel_digest),
                now_ms,
            )?;
        }
        if let Some(pending) = &state.pending_red {
            self.verify_artifact_with_identity_policy(
                AutonomyArtifactKindV1::SentinelVerdict,
                &pending.verdict.verdict_digest,
                "pinned-sentinel",
                &pending.verdict.signature,
                &pending.verdict,
                Some(&state.kernel.core.sentinel_identity_key_binary_policy_digest),
                now_ms,
            )?;
            if let Some(latch) = &pending.latch {
                self.verify_artifact_with_identity_policy(
                    AutonomyArtifactKindV1::RedLatchReceipt,
                    &latch.latch_receipt_digest,
                    "safety-kernel",
                    &latch.owner_kernel_signature,
                    latch,
                    Some(&state.kernel.kernel_digest),
                    now_ms,
                )?;
            }
        }
        Ok(())
    }

    fn verify_artifact<T: Serialize>(
        &self,
        kind: AutonomyArtifactKindV1,
        artifact_digest: &str,
        subject_id: &str,
        signature: &OpaqueSignature,
        artifact: &T,
        now_ms: u64,
    ) -> Result<(), AutonomyRuntimeError> {
        self.verify_artifact_with_identity_policy(
            kind,
            artifact_digest,
            subject_id,
            signature,
            artifact,
            None,
            now_ms,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_artifact_with_identity_policy<T: Serialize>(
        &self,
        kind: AutonomyArtifactKindV1,
        artifact_digest: &str,
        subject_id: &str,
        signature: &OpaqueSignature,
        artifact: &T,
        identity_key_binary_policy_digest: Option<&str>,
        now_ms: u64,
    ) -> Result<(), AutonomyRuntimeError> {
        require_digest("artifact_digest", artifact_digest)?;
        require_non_empty("artifact.subject_id", subject_id)?;
        if let Some(policy_digest) = identity_key_binary_policy_digest {
            require_digest("artifact.identity_key_binary_policy_digest", policy_digest)?;
        }
        if signature.is_empty() {
            return Err(AutonomyRuntimeError::Verification {
                kind,
                message: "empty signature".to_owned(),
            });
        }
        let bytes = canonical_json(artifact)?;
        self.verifier
            .verify(AutonomyVerificationRequestV1 {
                kind,
                artifact_digest,
                subject_id,
                signature,
                canonical_bytes: &bytes,
                identity_key_binary_policy_digest,
                now_ms,
            })
            .map_err(|message| AutonomyRuntimeError::Verification { kind, message })
    }

    fn recover(&mut self, replay: JournalReplay, now_ms: u64) -> Result<(), AutonomyRuntimeError> {
        match self.protected_root.clone() {
            None => {
                if replay.latest_commit.is_some() {
                    return Err(AutonomyRuntimeError::AntiRollback {
                        reason: "durable commits exist but protected root is absent".to_owned(),
                    });
                }
                if let Some(pending) = replay.pending {
                    self.append_abort(&pending.record, "prepare never reached protected storage")?;
                }
                self.state = None;
            }
            Some(root) if root.phase == ProtectedAutonomyPhaseV1::Committed => {
                let commit =
                    replay
                        .latest_commit
                        .ok_or_else(|| AutonomyRuntimeError::AntiRollback {
                            reason: "protected committed root has no journal commit".to_owned(),
                        })?;
                if commit.record.record_digest != root.journal_record_digest
                    || commit.record.sequence != root.journal_sequence
                    || commit.state.state_digest != root.state_digest
                {
                    return Err(AutonomyRuntimeError::AntiRollback {
                        reason: "protected committed root does not equal the latest journal commit"
                            .to_owned(),
                    });
                }
                validate_root_state_projection(&root, &commit.state)?;
                self.state = Some(commit.state);
                if let Some(pending) = replay.pending {
                    let previous = match &pending.record.payload {
                        AutonomyJournalPayloadV1::Prepare {
                            previous_state_digest,
                            ..
                        } => previous_state_digest.as_deref(),
                        _ => None,
                    };
                    if previous != self.state.as_ref().map(|state| state.state_digest.as_str()) {
                        return Err(AutonomyRuntimeError::AntiRollback {
                            reason:
                                "dangling prepare is not based on the protected committed state"
                                    .to_owned(),
                        });
                    }
                    self.append_abort(&pending.record, "protected root remained on prior commit")?;
                }
            }
            Some(root) => {
                let prepare_digest = root.journal_record_digest.clone();
                if let Some(commit) = replay.commits_by_prepare.get(&prepare_digest) {
                    validate_root_state_projection(&root, &commit.state)?;
                    let committed = protected_root_for_record(
                        ProtectedAutonomyPhaseV1::Committed,
                        &root.transition_id,
                        &commit.record,
                        &commit.state,
                    )?;
                    self.backend
                        .compare_and_swap(Some(&root), &committed)
                        .map_err(|message| AutonomyRuntimeError::ProtectedBackend {
                            operation: "recover_commit_compare_and_swap",
                            message,
                        })?;
                    self.protected_root = Some(committed);
                    self.state = Some(commit.state.clone());
                } else {
                    let pending =
                        replay
                            .pending
                            .ok_or_else(|| AutonomyRuntimeError::AntiRollback {
                                reason: "protected PREPARED root has neither prepare nor commit"
                                    .to_owned(),
                            })?;
                    if pending.record.record_digest != root.journal_record_digest
                        || pending.record.sequence != root.journal_sequence
                        || pending.state.state_digest != root.state_digest
                    {
                        return Err(AutonomyRuntimeError::AntiRollback {
                            reason: "protected PREPARED root does not match journal prepare"
                                .to_owned(),
                        });
                    }
                    validate_root_state_projection(&root, &pending.state)?;
                    self.validate_state(&pending.state, now_ms)?;
                    self.verify_state_artifacts(&pending.state, now_ms)?;
                    let transition_id = root.transition_id.clone();
                    let commit = self.append_commit(
                        &transition_id,
                        &pending.record.record_digest,
                        &pending.state.state_digest,
                    )?;
                    let committed = protected_root_for_record(
                        ProtectedAutonomyPhaseV1::Committed,
                        &transition_id,
                        &commit,
                        &pending.state,
                    )?;
                    self.backend
                        .compare_and_swap(Some(&root), &committed)
                        .map_err(|message| AutonomyRuntimeError::ProtectedBackend {
                            operation: "recover_forward_compare_and_swap",
                            message,
                        })?;
                    self.protected_root = Some(committed);
                    self.state = Some(pending.state);
                }
            }
        }
        Ok(())
    }

    fn commit_transition(
        &mut self,
        kind: AutonomyTransitionKindV1,
        mut next: AutonomyRuntimeStateV1,
        now_ms: u64,
    ) -> Result<(), AutonomyRuntimeError> {
        self.ensure_usable()?;
        next.seal()?;
        self.validate_state(&next, now_ms)?;
        self.verify_state_artifacts(&next, now_ms)?;
        let previous_state_digest = self.state.as_ref().map(|state| state.state_digest.clone());
        let transition_id = format!(
            "autonomy-transition:{}",
            digest_canonical(
                "m1nd-autonomy-transition-id-v1",
                &(
                    kind,
                    previous_state_digest.as_deref(),
                    next.state_digest.as_str(),
                    self.next_sequence,
                ),
            )?
        );
        let prepare = self.append_record(AutonomyJournalPayloadV1::Prepare {
            transition_id: transition_id.clone(),
            transition_kind: kind,
            previous_state_digest,
            next_state: Box::new(next.clone()),
        })?;
        let prepared_root = protected_root_for_record(
            ProtectedAutonomyPhaseV1::Prepared,
            &transition_id,
            &prepare,
            &next,
        )?;
        if let Err(message) = self
            .backend
            .compare_and_swap(self.protected_root.as_ref(), &prepared_root)
        {
            self.poisoned = true;
            return Err(AutonomyRuntimeError::ProtectedBackend {
                operation: "prepare_compare_and_swap",
                message,
            });
        }
        self.protected_root = Some(prepared_root.clone());
        let commit =
            match self.append_commit(&transition_id, &prepare.record_digest, &next.state_digest) {
                Ok(commit) => commit,
                Err(error) => {
                    self.poisoned = true;
                    return Err(error);
                }
            };
        let committed_root = protected_root_for_record(
            ProtectedAutonomyPhaseV1::Committed,
            &transition_id,
            &commit,
            &next,
        )?;
        if let Err(message) = self
            .backend
            .compare_and_swap(Some(&prepared_root), &committed_root)
        {
            self.poisoned = true;
            return Err(AutonomyRuntimeError::ProtectedBackend {
                operation: "commit_compare_and_swap",
                message,
            });
        }
        self.protected_root = Some(committed_root);
        self.state = Some(next);
        Ok(())
    }

    fn append_commit(
        &mut self,
        transition_id: &str,
        prepare_record_digest: &str,
        committed_state_digest: &str,
    ) -> Result<AutonomyJournalRecordV1, AutonomyRuntimeError> {
        self.append_record(AutonomyJournalPayloadV1::Commit {
            transition_id: transition_id.to_owned(),
            prepare_record_digest: prepare_record_digest.to_owned(),
            committed_state_digest: committed_state_digest.to_owned(),
        })
    }

    fn append_abort(
        &mut self,
        prepare: &AutonomyJournalRecordV1,
        reason: &str,
    ) -> Result<AutonomyJournalRecordV1, AutonomyRuntimeError> {
        let transition_id = match &prepare.payload {
            AutonomyJournalPayloadV1::Prepare { transition_id, .. } => transition_id.clone(),
            _ => {
                return Err(AutonomyRuntimeError::CorruptJournal {
                    line: prepare.sequence as usize,
                    reason: "attempted to abort a non-PREPARE record".to_owned(),
                })
            }
        };
        self.append_record(AutonomyJournalPayloadV1::Abort {
            transition_id,
            prepare_record_digest: prepare.record_digest.clone(),
            reason: reason.to_owned(),
        })
    }

    fn append_record(
        &mut self,
        payload: AutonomyJournalPayloadV1,
    ) -> Result<AutonomyJournalRecordV1, AutonomyRuntimeError> {
        let observed_len = self
            .journal
            .metadata()
            .map_err(|source| AutonomyRuntimeError::Io {
                operation: "read_journal_length_before_append",
                path: self.journal_path.clone(),
                source,
            })?
            .len();
        if observed_len != self.known_len {
            self.poisoned = true;
            return Err(AutonomyRuntimeError::ConcurrentModification {
                expected_len: self.known_len,
                observed_len,
            });
        }
        let mut record = AutonomyJournalRecordV1 {
            schema: AUTONOMY_JOURNAL_RECORD_SCHEMA.to_owned(),
            sequence: self.next_sequence,
            previous_record_digest: self.tail_digest.clone(),
            payload,
            record_digest: String::new(),
        };
        record.seal()?;
        let mut bytes = canonical_json_string(&record)?.into_bytes();
        bytes.push(b'\n');
        if let Err(source) = self.journal.write_all(&bytes) {
            self.poisoned = true;
            return Err(AutonomyRuntimeError::Io {
                operation: "append_journal_record",
                path: self.journal_path.clone(),
                source,
            });
        }
        if let Err(source) = self.journal.sync_all() {
            self.poisoned = true;
            return Err(AutonomyRuntimeError::Io {
                operation: "sync_journal_record",
                path: self.journal_path.clone(),
                source,
            });
        }
        self.known_len = self.known_len.saturating_add(bytes.len() as u64);
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.tail_digest = Some(record.record_digest.clone());
        Ok(record)
    }
}

fn replay_journal(bytes: &[u8]) -> Result<JournalReplay, AutonomyRuntimeError> {
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err(AutonomyRuntimeError::CorruptJournal {
            line: bytes.iter().filter(|byte| **byte == b'\n').count() + 1,
            reason: "unterminated or torn tail record".to_owned(),
        });
    }
    let mut sequence = 1_u64;
    let mut tail_digest: Option<String> = None;
    let mut pending: Option<PreparedReplay> = None;
    let mut latest_commit = None;
    let mut commits_by_prepare = BTreeMap::new();
    let lines: Vec<&[u8]> = bytes.split(|byte| *byte == b'\n').collect();
    for (index, line) in lines.iter().copied().enumerate() {
        if line.is_empty() {
            if index + 1 == lines.len() {
                continue;
            }
            return Err(AutonomyRuntimeError::CorruptJournal {
                line: index + 1,
                reason: "blank journal record".to_owned(),
            });
        }
        let line_number = index + 1;
        let record: AutonomyJournalRecordV1 =
            serde_json::from_slice(line).map_err(|error| AutonomyRuntimeError::CorruptJournal {
                line: line_number,
                reason: format!("invalid JSON: {error}"),
            })?;
        if canonical_json(&record)? != line {
            return Err(AutonomyRuntimeError::CorruptJournal {
                line: line_number,
                reason: "record is not canonical JSON".to_owned(),
            });
        }
        if record.schema != AUTONOMY_JOURNAL_RECORD_SCHEMA
            || record.sequence != sequence
            || record.previous_record_digest != tail_digest
            || record.compute_digest()? != record.record_digest
        {
            return Err(AutonomyRuntimeError::CorruptJournal {
                line: line_number,
                reason: "schema/sequence/chain/self-digest mismatch".to_owned(),
            });
        }
        match &record.payload {
            AutonomyJournalPayloadV1::Prepare {
                transition_id,
                previous_state_digest,
                next_state,
                ..
            } => {
                require_non_empty("journal.transition_id", transition_id)?;
                if pending.is_some() {
                    return Err(AutonomyRuntimeError::CorruptJournal {
                        line: line_number,
                        reason: "nested PREPARE without COMMIT/ABORT".to_owned(),
                    });
                }
                if previous_state_digest.as_deref()
                    != latest_commit
                        .as_ref()
                        .map(|commit: &CommitReplay| commit.state.state_digest.as_str())
                {
                    return Err(AutonomyRuntimeError::CorruptJournal {
                        line: line_number,
                        reason: "PREPARE previous state is not the latest committed state"
                            .to_owned(),
                    });
                }
                if next_state.compute_state_digest()? != next_state.state_digest {
                    return Err(AutonomyRuntimeError::CorruptJournal {
                        line: line_number,
                        reason: "prepared state self-digest mismatch".to_owned(),
                    });
                }
                pending = Some(PreparedReplay {
                    record: record.clone(),
                    state: (**next_state).clone(),
                });
            }
            AutonomyJournalPayloadV1::Commit {
                transition_id,
                prepare_record_digest,
                committed_state_digest,
            } => {
                let prepared =
                    pending
                        .take()
                        .ok_or_else(|| AutonomyRuntimeError::CorruptJournal {
                            line: line_number,
                            reason: "COMMIT without PREPARE".to_owned(),
                        })?;
                let prepared_transition = match &prepared.record.payload {
                    AutonomyJournalPayloadV1::Prepare { transition_id, .. } => transition_id,
                    _ => unreachable!("pending replay stores only PREPARE"),
                };
                if transition_id != prepared_transition
                    || prepare_record_digest != &prepared.record.record_digest
                    || committed_state_digest != &prepared.state.state_digest
                {
                    return Err(AutonomyRuntimeError::CorruptJournal {
                        line: line_number,
                        reason: "COMMIT does not bind its exact PREPARE/state".to_owned(),
                    });
                }
                let committed = CommitReplay {
                    record: record.clone(),
                    prepare_record_digest: prepare_record_digest.clone(),
                    state: prepared.state,
                };
                commits_by_prepare.insert(prepare_record_digest.clone(), clone_commit(&committed));
                latest_commit = Some(committed);
            }
            AutonomyJournalPayloadV1::Abort {
                transition_id,
                prepare_record_digest,
                reason,
            } => {
                let prepared =
                    pending
                        .take()
                        .ok_or_else(|| AutonomyRuntimeError::CorruptJournal {
                            line: line_number,
                            reason: "ABORT without PREPARE".to_owned(),
                        })?;
                let prepared_transition = match &prepared.record.payload {
                    AutonomyJournalPayloadV1::Prepare { transition_id, .. } => transition_id,
                    _ => unreachable!("pending replay stores only PREPARE"),
                };
                if transition_id != prepared_transition
                    || prepare_record_digest != &prepared.record.record_digest
                    || reason.trim().is_empty()
                {
                    return Err(AutonomyRuntimeError::CorruptJournal {
                        line: line_number,
                        reason: "ABORT does not bind its exact PREPARE/reason".to_owned(),
                    });
                }
            }
        }
        sequence = sequence.saturating_add(1);
        tail_digest = Some(record.record_digest);
    }
    Ok(JournalReplay {
        next_sequence: sequence,
        tail_digest,
        known_len: bytes.len() as u64,
        latest_commit,
        commits_by_prepare,
        pending,
    })
}

fn clone_commit(commit: &CommitReplay) -> CommitReplay {
    CommitReplay {
        record: commit.record.clone(),
        prepare_record_digest: commit.prepare_record_digest.clone(),
        state: commit.state.clone(),
    }
}

fn protected_root_for_record(
    phase: ProtectedAutonomyPhaseV1,
    transition_id: &str,
    record: &AutonomyJournalRecordV1,
    state: &AutonomyRuntimeStateV1,
) -> Result<ProtectedAutonomyRootV1, AutonomyRuntimeError> {
    let mut root = ProtectedAutonomyRootV1 {
        schema: AUTONOMY_PROTECTED_ROOT_SCHEMA.to_owned(),
        phase,
        transition_id: transition_id.to_owned(),
        journal_sequence: record.sequence,
        journal_record_digest: record.record_digest.clone(),
        state_digest: state.state_digest.clone(),
        state_generation: state.generation,
        autonomy_epoch: state.autonomy_epoch.autonomy_epoch,
        constitution_epoch: state.autonomy_epoch.constitution_epoch,
        intent_store_root_digest: state.intent_store_root_digest.clone(),
        sentinel_outbox_epoch: state
            .sentinel_outbox_tail
            .as_ref()
            .map_or(0, |outbox| outbox.core.outbox_epoch),
        red_latch_epoch: state.last_red_latch_epoch,
        root_digest: String::new(),
    };
    root.seal()?;
    Ok(root)
}

fn validate_root_state_projection(
    root: &ProtectedAutonomyRootV1,
    state: &AutonomyRuntimeStateV1,
) -> Result<(), AutonomyRuntimeError> {
    let outbox_epoch = state
        .sentinel_outbox_tail
        .as_ref()
        .map_or(0, |outbox| outbox.core.outbox_epoch);
    let latch_epoch = state.last_red_latch_epoch;
    if root.state_digest != state.state_digest
        || root.state_generation != state.generation
        || root.autonomy_epoch != state.autonomy_epoch.autonomy_epoch
        || root.constitution_epoch != state.autonomy_epoch.constitution_epoch
        || root.intent_store_root_digest != state.intent_store_root_digest
        || root.sentinel_outbox_epoch != outbox_epoch
        || root.red_latch_epoch != latch_epoch
    {
        return Err(AutonomyRuntimeError::AntiRollback {
            reason: "protected root projection differs from journal state".to_owned(),
        });
    }
    Ok(())
}

fn compute_intent_store_root(
    entries: &BTreeMap<String, IntentCoreEntryV1>,
) -> Result<String, CanonicalError> {
    digest_canonical(INTENT_STORE_ROOT_DIGEST_DOMAIN, entries)
}

pub fn compute_tier_evidence_set_digest(
    lanes: &BTreeMap<String, Vec<AutonomyTierEvidenceV1>>,
) -> Result<String, CanonicalError> {
    let digest_map: BTreeMap<&str, Vec<&str>> = lanes
        .iter()
        .map(|(subject, lane)| {
            (
                subject.as_str(),
                lane.iter()
                    .map(|evidence| evidence.evidence_digest.as_str())
                    .collect(),
            )
        })
        .collect();
    digest_canonical(TIER_EVIDENCE_SET_DIGEST_DOMAIN, &digest_map)
}

pub fn red_commit_marker_digest(
    transaction_id: &str,
    safety_decision_digest: &str,
    previous_state_digest: &str,
    next_autonomy_epoch: u64,
) -> Result<String, CanonicalError> {
    #[derive(Serialize)]
    struct Marker<'a> {
        transaction_id: &'a str,
        safety_decision_digest: &'a str,
        previous_state_digest: &'a str,
        next_autonomy_epoch: u64,
    }
    digest_canonical(
        RED_COMMIT_MARKER_DIGEST_DOMAIN,
        &Marker {
            transaction_id,
            safety_decision_digest,
            previous_state_digest,
            next_autonomy_epoch,
        },
    )
}

fn same_red_mandate(left: &RedLatchReceiptV1, right: &RedLatchReceiptV1) -> bool {
    left.core.latch_receipt_id == right.core.latch_receipt_id
        && left.core.red_verdict_digest == right.core.red_verdict_digest
        && left.core.source_intent_digest == right.core.source_intent_digest
        && left.core.protected_time_evidence_digest == right.core.protected_time_evidence_digest
        && left.core.constitution_epoch == right.core.constitution_epoch
        && left.core.autonomy_epoch == right.core.autonomy_epoch
        && left.core.latch_epoch == right.core.latch_epoch
        && left.core.exact_affected_scope_digest == right.core.exact_affected_scope_digest
        && left.core.allowed_negative_actions_digest == right.core.allowed_negative_actions_digest
        && left.core.rollback_candidate_plan_digest == right.core.rollback_candidate_plan_digest
        && left.core.immutable_negative_mandate_digest
            == right.core.immutable_negative_mandate_digest
}

fn target_epoch_reference_digest(epoch: &AutonomyEpochV1) -> Result<String, CanonicalError> {
    digest_canonical("m1nd-autonomy-epoch-reference-v1", epoch)
}

fn authority_decision_signature(decision: &AuthorityDecisionV1) -> &OpaqueSignature {
    match decision {
        AuthorityDecisionV1::Human(value) => &value.owner_signature,
        AuthorityDecisionV1::Policy(value) => &value.owner_signature,
        AuthorityDecisionV1::AgentQuorum(value) => &value.owner_signature,
        AuthorityDecisionV1::Safety(value) => &value.safety_kernel_signature,
    }
}

fn recovery_authority_for_mode(mode: ActiveMode) -> AuthorityVariant {
    match mode {
        ActiveMode::HumanGated | ActiveMode::PolicyAutonomous => AuthorityVariant::Human,
        ActiveMode::FullAutonomy => AuthorityVariant::AgentQuorum,
    }
}

fn next_tier(tier: AutonomyTier) -> Option<AutonomyTier> {
    match tier {
        AutonomyTier::A0Observe => Some(AutonomyTier::A1Propose),
        AutonomyTier::A1Propose => Some(AutonomyTier::A2Execute),
        AutonomyTier::A2Execute => Some(AutonomyTier::A3AutonomousLand),
        AutonomyTier::A3AutonomousLand => Some(AutonomyTier::A4AutonomousGovern),
        AutonomyTier::A4AutonomousGovern => Some(AutonomyTier::A5FullAutonomy),
        AutonomyTier::A5FullAutonomy => None,
    }
}

fn ensure_assurance(
    component: &'static str,
    required: AutonomyRuntimeAssurance,
    actual: AutonomyRuntimeAssurance,
) -> Result<(), AutonomyRuntimeError> {
    if actual < required {
        Err(AutonomyRuntimeError::AssuranceTooLow {
            component,
            required,
            actual,
        })
    } else {
        Ok(())
    }
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), AutonomyRuntimeError> {
    if value.trim().is_empty() {
        Err(AutonomyRuntimeError::EmptyRequired { field })
    } else {
        Ok(())
    }
}

fn require_digest(field: &'static str, value: &str) -> Result<(), AutonomyRuntimeError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Err(AutonomyRuntimeError::InvalidDigest { field })
    } else {
        Ok(())
    }
}

fn refuse_symlink_if_present(path: &Path) -> Result<(), AutonomyRuntimeError> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            return Err(AutonomyRuntimeError::SymlinkRefused {
                path: path.to_path_buf(),
            });
        }
    }
    Ok(())
}

fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>, AutonomyRuntimeError> {
    refuse_symlink_if_present(path)?;
    let metadata = fs::metadata(path).map_err(|source| AutonomyRuntimeError::Io {
        operation: "stat_intent_object",
        path: path.to_path_buf(),
        source,
    })?;
    if metadata.len() > limit {
        return Err(AutonomyRuntimeError::IntentTooLarge {
            observed: metadata.len(),
            limit,
        });
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|source| AutonomyRuntimeError::Io {
            operation: "read_intent_object",
            path: path.to_path_buf(),
            source,
        })?;
    Ok(bytes)
}

fn write_atomic_fsynced(path: &Path, bytes: &[u8]) -> Result<(), AutonomyRuntimeError> {
    let parent = path
        .parent()
        .ok_or_else(|| AutonomyRuntimeError::InvalidState {
            reason: "intent object path has no parent".to_owned(),
        })?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AutonomyRuntimeError::InvalidState {
            reason: "intent object filename is not UTF-8".to_owned(),
        })?;
    let temporary = parent.join(format!(".{file_name}.{}.tmp", std::process::id()));
    if temporary.exists() {
        return Err(AutonomyRuntimeError::ConcurrentModification {
            expected_len: 0,
            observed_len: fs::metadata(&temporary).map_or(1, |metadata| metadata.len()),
        });
    }
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|source| AutonomyRuntimeError::Io {
                operation: "create_intent_temporary",
                path: temporary.clone(),
                source,
            })?;
        file.write_all(bytes)
            .map_err(|source| AutonomyRuntimeError::Io {
                operation: "write_intent_temporary",
                path: temporary.clone(),
                source,
            })?;
        file.sync_all().map_err(|source| AutonomyRuntimeError::Io {
            operation: "sync_intent_temporary",
            path: temporary.clone(),
            source,
        })?;
        fs::rename(&temporary, path).map_err(|source| AutonomyRuntimeError::Io {
            operation: "rename_intent_object",
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn sync_directory(path: &Path, operation: &'static str) -> Result<(), AutonomyRuntimeError> {
    // Windows refuses fsync on directory handles (ACCESS_DENIED); durable
    // renames there rely on MoveFileEx write-through semantics instead.
    #[cfg(windows)]
    {
        let _ = (path, operation);
        Ok(())
    }
    #[cfg(not(windows))]
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|source| AutonomyRuntimeError::Io {
            operation,
            path: path.to_path_buf(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tempfile::TempDir;

    use super::*;
    use crate::autonomy::*;
    use crate::{Effect, RiskClass};

    const NOW: u64 = 10_000;
    const ISSUED_AT: u64 = 9_000;
    const EXPIRES_AT: u64 = 20_000;

    #[derive(Clone, Default)]
    struct SharedProtectedBackend {
        inner: Arc<Mutex<BackendState>>,
    }

    #[derive(Default)]
    struct BackendState {
        root: Option<ProtectedAutonomyRootV1>,
        cas_calls: u64,
        fail_on_call: Option<u64>,
    }

    impl SharedProtectedBackend {
        fn fail_on_call(&self, call: u64) {
            self.inner.lock().unwrap().fail_on_call = Some(call);
        }

        fn force_root(&self, root: Option<ProtectedAutonomyRootV1>) {
            self.inner.lock().unwrap().root = root;
        }
    }

    impl ProtectedAutonomyRootBackend for SharedProtectedBackend {
        fn assurance(&self) -> AutonomyRuntimeAssurance {
            AutonomyRuntimeAssurance::SoftwareTestOnlyNotProduction
        }

        fn load(&self) -> Result<Option<ProtectedAutonomyRootV1>, String> {
            Ok(self.inner.lock().unwrap().root.clone())
        }

        fn compare_and_swap(
            &mut self,
            expected: Option<&ProtectedAutonomyRootV1>,
            next: &ProtectedAutonomyRootV1,
        ) -> Result<(), String> {
            let mut state = self.inner.lock().unwrap();
            state.cas_calls = state.cas_calls.saturating_add(1);
            if state.fail_on_call == Some(state.cas_calls) {
                state.fail_on_call = None;
                return Err("injected protected CAS interruption".to_owned());
            }
            if state.root.as_ref() != expected {
                return Err("compare-and-swap mismatch".to_owned());
            }
            state.root = Some(next.clone());
            Ok(())
        }
    }

    #[derive(Clone, Copy)]
    struct SoftwareTestVerifier;

    impl AutonomyArtifactVerifier for SoftwareTestVerifier {
        fn assurance(&self) -> AutonomyRuntimeAssurance {
            AutonomyRuntimeAssurance::SoftwareTestOnlyNotProduction
        }

        fn verify(&self, request: AutonomyVerificationRequestV1<'_>) -> Result<(), String> {
            let requires_pinned_policy = matches!(
                request.kind,
                AutonomyArtifactKindV1::SentinelVerdict
                    | AutonomyArtifactKindV1::SentinelRedOutbox
                    | AutonomyArtifactKindV1::RedLatchReceipt
                    | AutonomyArtifactKindV1::SafetyCapability
            );
            let policy_is_bound = request
                .identity_key_binary_policy_digest
                .is_some_and(|digest| digest.len() == 64);
            if request.signature.as_str().starts_with("test-signature:")
                && !request.canonical_bytes.is_empty()
                && request.artifact_digest.len() == 64
                && !request.subject_id.is_empty()
                && (!requires_pinned_policy || policy_is_bound)
            {
                Ok(())
            } else {
                Err("software fixture rejected signature or binding".to_owned())
            }
        }
    }

    type TestStore = AutonomyRuntimeStore<SharedProtectedBackend, SoftwareTestVerifier>;

    struct ActivatedFixture {
        _temp: TempDir,
        backend: SharedProtectedBackend,
        store: TestStore,
        candidate: String,
        grant: AutonomyGrantV1,
    }

    fn digest(label: &str) -> String {
        digest_canonical("m1nd-autonomy-runtime-test-v1", &label).unwrap()
    }

    fn signature(label: &str) -> OpaqueSignature {
        OpaqueSignature::new(format!("test-signature:{label}"))
    }

    fn config(root: &Path) -> AutonomyRuntimeConfig {
        AutonomyRuntimeConfig::software_test_only(
            root,
            "test-authority-durability-domain",
            "organism-1",
            "repo-1",
            "brain-1",
        )
    }

    fn kernel_fixture() -> SafetyKernelV1 {
        let mut kernel = SafetyKernelV1 {
            schema: SAFETY_KERNEL_SCHEMA.to_owned(),
            core: SafetyKernelCoreV1 {
                kernel_id: "kernel-1".to_owned(),
                verifier_binary_digest: digest("verifier-binary"),
                canonicalization_version: CANONICALIZATION_VERSION.to_owned(),
                pinned_external_root_key: "offline-root-key-1".to_owned(),
                verified_boot_policy_digest: digest("verified-boot"),
                immutable_invariants_digest: digest("immutable-invariants"),
                minimum_verifier_seats: IMMUTABLE_VERIFIER_SEATS,
                minimum_quorum_threshold: IMMUTABLE_QUORUM_THRESHOLD,
                minimum_failure_domains: IMMUTABLE_FAILURE_DOMAINS,
                proposer_executor_nonvoting: true,
                sentinel_required_and_nonvoting: true,
                sentinel_red_absolute_veto: true,
                sentinel_outbox_antirollback_required: true,
                sentinel_identity_key_binary_policy_digest: digest("sentinel-pin"),
                safety_actuator_identity_key_binary_policy_digest: digest("actuator-pin"),
                required_sentinel_unavailable_fail_closed: true,
                audit_wal_tamper_detection_required: true,
                epoch_freeze_and_rollback_required: true,
                old_runtime_approval_required: true,
                allowed_negative_effects: SafetyKernelV1::canonical_negative_effects(),
            },
            kernel_digest: digest("unsealed-kernel"),
            external_root_signature: signature("kernel"),
        };
        kernel.seal().unwrap();
        kernel
    }

    fn independence_fixture(epoch: u64) -> IndependenceSpecV1 {
        let mut independence = IndependenceSpecV1 {
            schema: INDEPENDENCE_SPEC_SCHEMA.to_owned(),
            core: IndependenceSpecCoreV1 {
                constitution_epoch: epoch,
                voting_verifiers: vec![
                    seat(
                        "verifier-1",
                        "key-1",
                        "provider-a/model-a/runtime-a",
                        "ctx-1",
                    ),
                    seat(
                        "verifier-2",
                        "key-2",
                        "provider-b/model-b/runtime-b",
                        "ctx-2",
                    ),
                    seat(
                        "verifier-3",
                        "key-3",
                        "provider-c/model-c/runtime-c",
                        "ctx-3",
                    ),
                    seat(
                        "verifier-4",
                        "key-4",
                        "provider-c/model-c/runtime-c",
                        "ctx-4",
                    ),
                ],
                quorum_threshold: IMMUTABLE_QUORUM_THRESHOLD,
                minimum_failure_domains: IMMUTABLE_FAILURE_DOMAINS,
                blind_isolation_policy_digest: digest("blind-isolation"),
                nonvoting_sentinel_id: "sentinel-1".to_owned(),
                proposer_executor_nonvoting: true,
                sentinel_nonvoting: true,
            },
            independence_spec_digest: digest("unsealed-independence"),
        };
        independence.seal().unwrap();
        independence
    }

    fn seat(
        principal_id: &str,
        key_id: &str,
        failure_domain: &str,
        context: &str,
    ) -> VerifierSeatV1 {
        VerifierSeatV1 {
            principal_id: principal_id.to_owned(),
            key_id: key_id.to_owned(),
            failure_domain: failure_domain.to_owned(),
            parent_session_context_digest: digest(context),
        }
    }

    fn constitution_fixture(independence: &IndependenceSpecV1) -> ConstitutionStoreV1 {
        let mut constitution = ConstitutionStoreV1 {
            schema: CONSTITUTION_SCHEMA.to_owned(),
            core: ConstitutionCoreV1 {
                constitution_epoch: independence.core.constitution_epoch,
                previous_constitution_digest: None,
                effective_at: 1_000,
                expires_at: 100_000,
                allowed_autonomy_modes: [
                    ActiveMode::HumanGated,
                    ActiveMode::PolicyAutonomous,
                    ActiveMode::FullAutonomy,
                ]
                .into_iter()
                .collect(),
                objectives: ["correctness".to_owned()].into_iter().collect(),
                non_goals: ["unbounded-authority".to_owned()].into_iter().collect(),
                resource_scope_digest: digest("resource-scope"),
                risk_budget_action_policy_digest: digest("risk-budget-policy"),
                independence_spec_digest: independence.independence_spec_digest.clone(),
                metric_specs_digest: digest("metric-specs"),
                canary_requirements_digest: digest("canary-requirements"),
                rollback_requirements_digest: digest("rollback-requirements"),
                amendment_rules_digest: digest("amendment-rules"),
                previous_governance_runtime_digest: digest("previous-runtime"),
                adopting_governance_runtime_digest: digest("adopting-runtime"),
                old_runtime_approval_digest: None,
                issuer_subject_id: "external-bootstrap-root".to_owned(),
            },
            constitution_digest: digest("unsealed-constitution"),
            signature: signature("constitution"),
        };
        constitution.seal().unwrap();
        constitution
    }

    fn bootstrap_epoch(constitution: &ConstitutionStoreV1) -> AutonomyEpochV1 {
        AutonomyEpochV1 {
            schema: AUTONOMY_EPOCH_SCHEMA.to_owned(),
            autonomy_epoch: 0,
            active_mode: ActiveMode::HumanGated,
            activation_receipt_id: None,
            constitution_digest: constitution.constitution_digest.clone(),
            constitution_epoch: constitution.core.constitution_epoch,
            grants_digest: compute_grants_digest(&[]).unwrap(),
            issuance_frozen: false,
            safety_state: SafetyState::Healthy,
            protected_root_signature: signature("bootstrap-epoch"),
        }
    }

    fn open_bootstrapped(temp: &TempDir, backend: SharedProtectedBackend) -> TestStore {
        let mut store =
            AutonomyRuntimeStore::open(config(temp.path()), backend, SoftwareTestVerifier, NOW)
                .unwrap();
        let kernel = kernel_fixture();
        let independence = independence_fixture(0);
        let constitution = constitution_fixture(&independence);
        let epoch = bootstrap_epoch(&constitution);
        store
            .bootstrap(kernel, independence, constitution, epoch, NOW)
            .unwrap();
        store
    }

    fn tier_evidence(
        subject: &str,
        tier: AutonomyTier,
        previous: Option<&AutonomyTierEvidenceV1>,
        candidate: &str,
    ) -> AutonomyTierEvidenceV1 {
        let mut evidence = AutonomyTierEvidenceV1 {
            schema: TIER_EVIDENCE_SCHEMA.to_owned(),
            subject_id: subject.to_owned(),
            tier,
            evaluator_subject_id: "independent-evaluator".to_owned(),
            proposer_subject_id: "promotion-proposer".to_owned(),
            executor_subject_id: Some("promotion-executor".to_owned()),
            verifier_principals: [
                "verifier-1".to_owned(),
                "verifier-2".to_owned(),
                "verifier-3".to_owned(),
                "verifier-4".to_owned(),
            ]
            .into_iter()
            .collect(),
            failure_domains: [
                "provider-a/model-a/runtime-a".to_owned(),
                "provider-b/model-b/runtime-b".to_owned(),
                "provider-c/model-c/runtime-c".to_owned(),
            ]
            .into_iter()
            .collect(),
            previous_evidence_digest: previous.map(|value| value.evidence_digest.clone()),
            exact_release_candidate_digest: candidate.to_owned(),
            shadow_receipt_digest: digest(&format!("shadow-{subject}-{tier:?}")),
            canary_receipt_digest: digest(&format!("canary-{subject}-{tier:?}")),
            rollback_receipt_digest: digest(&format!("rollback-{subject}-{tier:?}")),
            metric_receipt_digest: digest(&format!("metric-{subject}-{tier:?}")),
            recorded_at: NOW,
            evidence_digest: digest("unsealed-tier-evidence"),
            evaluator_signature: signature("tier-evaluator"),
        };
        evidence.seal().unwrap();
        evidence
    }

    fn record_evidence_through(
        store: &mut TestStore,
        subject: &str,
        last_tier: AutonomyTier,
        candidate: &str,
    ) -> AutonomyTierEvidenceV1 {
        let tiers = [
            AutonomyTier::A0Observe,
            AutonomyTier::A1Propose,
            AutonomyTier::A2Execute,
            AutonomyTier::A3AutonomousLand,
            AutonomyTier::A4AutonomousGovern,
            AutonomyTier::A5FullAutonomy,
        ];
        let mut previous: Option<AutonomyTierEvidenceV1> = None;
        for tier in tiers {
            let evidence = tier_evidence(subject, tier, previous.as_ref(), candidate);
            store.record_tier_evidence(evidence.clone(), NOW).unwrap();
            previous = Some(evidence);
            if tier == last_tier {
                break;
            }
        }
        previous.unwrap()
    }

    fn grant_fixture(
        subject: &str,
        mode: ActiveMode,
        tier: AutonomyTier,
        promotion_receipt_id: String,
        autonomy_epoch: u64,
    ) -> AutonomyGrantV1 {
        let mut grant = AutonomyGrantV1 {
            schema: AUTONOMY_GRANT_SCHEMA.to_owned(),
            core: AutonomyGrantCoreV1 {
                grant_id: format!("grant-{subject}"),
                subject_id: subject.to_owned(),
                role_id: "autonomous-operator".to_owned(),
                mode,
                max_tier: tier,
                action_classes: ["land".to_owned(), "diagnose".to_owned()]
                    .into_iter()
                    .collect(),
                risk_domains: [RiskClass::Low, RiskClass::Medium].into_iter().collect(),
                resource_environment_scope_digest: digest("grant-resource-scope"),
                budget: BudgetEnvelopeV1 {
                    unit: "work-units".to_owned(),
                    limit: 100,
                    consumed: 0,
                    reset_epoch: autonomy_epoch,
                },
                constitution_epoch: 0,
                autonomy_epoch,
                issued_at: ISSUED_AT,
                expires_at: EXPIRES_AT,
                promotion_receipt_id,
                status: GrantStatus::Active,
            },
            grant_digest: digest("unsealed-grant"),
            owner_signature: signature("grant"),
        };
        grant.seal().unwrap();
        grant
    }

    fn intent_fixture(
        state: &AutonomyRuntimeStateV1,
        grant: Option<&AutonomyGrantV1>,
        authority: AuthorityVariant,
        action_class: &str,
        candidate: Option<String>,
    ) -> SovereignActionIntentV1 {
        let autonomous = matches!(
            authority,
            AuthorityVariant::Policy | AuthorityVariant::AgentQuorum
        );
        let decision_subject = if autonomous { "agent-a" } else { "owner-human" };
        let issuer = match authority {
            AuthorityVariant::Human => "owner-human",
            AuthorityVariant::Policy => "policy-engine",
            AuthorityVariant::AgentQuorum => "constitutional-council",
            _ => panic!("fixture supports positive authority only"),
        };
        let mut intent = SovereignActionIntentV1 {
            schema: SOVEREIGN_ACTION_INTENT_SCHEMA.to_owned(),
            core: SovereignIntentCoreV1 {
                action_class: action_class.to_owned(),
                semantic_action_id: format!("mission.service.{action_class}"),
                action_payload_digest: digest(&format!("payload-{action_class}")),
                issuer_subject_id: issuer.to_owned(),
                decision_subject_id: decision_subject.to_owned(),
                caller_subject_id: decision_subject.to_owned(),
                audience: "m1nd-owner".to_owned(),
                proposer_subject_id: "action-proposer".to_owned(),
                executor_subject_id: Some("action-executor".to_owned()),
                promotion_target_subject_id: None,
                ratification_target_subject_id: None,
                delegation_grant_digest: None,
                required_authority_variant: authority,
                action_policy_registry_digest: digest("action-policy"),
                classifier_decision_digest: digest("classifier-decision"),
                applicable_grant_id: autonomous.then(|| grant.unwrap().core.grant_id.clone()),
                applicable_grant_digest: autonomous.then(|| grant.unwrap().grant_digest.clone()),
                organism_id: "organism-1".to_owned(),
                repo_id: "repo-1".to_owned(),
                brain_id: "brain-1".to_owned(),
                mission_id: Some("mission-1".to_owned()),
                mission_head_id: Some("head-1".to_owned()),
                block_id: Some("block-1".to_owned()),
                candidate_digest: candidate,
                promotion_subject_id: None,
                active_mode: state.autonomy_epoch.active_mode,
                effective_tier: grant.map_or(AutonomyTier::A1Propose, |value| value.core.max_tier),
                risk_class: RiskClass::Low,
                risk_scope_digest: digest("risk-scope"),
                resource_environment_scope_digest: grant.map_or_else(
                    || digest("human-resource-scope"),
                    |value| value.core.resource_environment_scope_digest.clone(),
                ),
                requested_budget: 1,
                constitution_digest: state.constitution.constitution_digest.clone(),
                constitution_epoch: state.autonomy_epoch.constitution_epoch,
                autonomy_epoch: state.autonomy_epoch.autonomy_epoch,
                expected_store_epoch: 11,
                expected_store_version: 12,
                expected_boundary_version: 13,
                expected_contract_version: 14,
                metric_spec_digest: digest("metric-spec"),
                evidence_digest: digest("evidence"),
                rollout_plan_digest: digest("rollout"),
                rollback_plan_digest: digest("rollback"),
                nonce: format!("nonce-{action_class}-{authority:?}"),
                issued_at: ISSUED_AT,
                expires_at: EXPIRES_AT,
            },
            intent_digest: digest("unsealed-intent"),
            intent_core_ref: IntentCoreRefV1::for_sovereign_digest(digest("unsealed-intent")),
        };
        intent.seal().unwrap();
        intent
    }

    fn human_decision(intent: &SovereignActionIntentV1) -> AuthorityDecisionV1 {
        let binding = AuthorityDecisionBindingV1::from_intent(
            intent,
            "human-activation-decision".to_owned(),
            false,
            None,
        );
        let mut decision = AuthorityDecisionV1::Human(HumanAuthorityDecisionV1 {
            schema: AUTHORITY_DECISION_SCHEMA.to_owned(),
            core: HumanAuthorityDecisionCoreV1 {
                binding,
                human_approval_digest: digest("human-approval"),
                human_decision_digest: digest("human-decision"),
                human_key_id: "human-key-1".to_owned(),
            },
            decision_digest: digest("unsealed-human-decision"),
            owner_signature: signature("human-decision"),
        });
        decision.seal().unwrap();
        decision
    }

    fn recovery_intent(state: &AutonomyRuntimeStateV1) -> SovereignActionIntentV1 {
        let last_mode = state
            .last_valid_mode_before_freeze
            .expect("terminal fixture preserves its last mode");
        let authority = recovery_authority_for_mode(last_mode);
        let mut intent = intent_fixture(state, None, authority, "autonomy.recover", None);
        intent.core.active_mode = last_mode;
        intent.core.action_payload_digest = digest("recovery-remediation-evidence");
        intent.core.evidence_digest = intent.core.action_payload_digest.clone();
        intent.core.rollback_plan_digest = digest("recovery-rollback-validation");
        intent.core.nonce = "nonce-autonomy-recover-terminal-1".to_owned();
        intent.seal().unwrap();
        intent
    }

    fn human_recovery_decision(
        intent: &SovereignActionIntentV1,
        sentinel: &SentinelVerdictV1,
    ) -> AuthorityDecisionV1 {
        let binding = AuthorityDecisionBindingV1::from_intent(
            intent,
            "human-recovery-decision".to_owned(),
            true,
            Some(sentinel.verdict_digest.clone()),
        );
        let mut decision = AuthorityDecisionV1::Human(HumanAuthorityDecisionV1 {
            schema: AUTHORITY_DECISION_SCHEMA.to_owned(),
            core: HumanAuthorityDecisionCoreV1 {
                binding,
                human_approval_digest: digest("human-recovery-approval"),
                human_decision_digest: digest("human-recovery-decision"),
                human_key_id: "human-key-1".to_owned(),
            },
            decision_digest: digest("unsealed-human-recovery-decision"),
            owner_signature: signature("human-recovery-decision"),
        });
        decision.seal().unwrap();
        decision
    }

    fn recovery_receipt(
        state: &AutonomyRuntimeStateV1,
        intent: &SovereignActionIntentV1,
        decision: &AuthorityDecisionV1,
        sentinel: &SentinelVerdictV1,
    ) -> AutonomyRecoveryReceiptV1 {
        let last_mode = state
            .last_valid_mode_before_freeze
            .expect("terminal fixture preserves its last mode");
        let terminal_latch = state
            .terminal_red_latches
            .get(&state.last_red_latch_epoch.to_string())
            .unwrap();
        let mut receipt = AutonomyRecoveryReceiptV1 {
            schema: AUTONOMY_RECOVERY_RECEIPT_SCHEMA.to_owned(),
            core: AutonomyRecoveryReceiptCoreV1 {
                receipt_id: String::new(),
                frozen_state_digest: state.state_digest.clone(),
                frozen_epoch_reference_digest: target_epoch_reference_digest(&state.autonomy_epoch)
                    .unwrap(),
                frozen_autonomy_epoch: state.autonomy_epoch.autonomy_epoch,
                last_valid_mode: last_mode,
                required_authority_variant: recovery_authority_for_mode(last_mode),
                recovery_intent_digest: intent.intent_digest.clone(),
                authority_decision_digest: decision.decision_digest().to_owned(),
                sentinel_verdict_digest: sentinel.verdict_digest.clone(),
                terminal_red_latch_receipt_digest: terminal_latch.latch_receipt_digest.clone(),
                remediation_evidence_digest: intent.core.action_payload_digest.clone(),
                rollback_validation_digest: intent.core.rollback_plan_digest.clone(),
                target_autonomy_epoch: state.autonomy_epoch.autonomy_epoch + 1,
                target_constitution_digest: state.constitution.constitution_digest.clone(),
                target_constitution_epoch: state.constitution.core.constitution_epoch,
                issuer_subject_id: intent.core.issuer_subject_id.clone(),
                issued_at: NOW,
            },
            receipt_digest: digest("unsealed-recovery-receipt"),
            signature: signature("recovery-receipt"),
        };
        receipt.seal().unwrap();
        receipt
    }

    fn recovered_epoch(
        state: &AutonomyRuntimeStateV1,
        receipt: &AutonomyRecoveryReceiptV1,
    ) -> AutonomyEpochV1 {
        AutonomyEpochV1 {
            schema: AUTONOMY_EPOCH_SCHEMA.to_owned(),
            autonomy_epoch: state.autonomy_epoch.autonomy_epoch + 1,
            active_mode: ActiveMode::HumanGated,
            activation_receipt_id: Some(receipt.core.receipt_id.clone()),
            constitution_digest: state.constitution.constitution_digest.clone(),
            constitution_epoch: state.constitution.core.constitution_epoch,
            grants_digest: compute_grants_digest(&[]).unwrap(),
            issuance_frozen: false,
            safety_state: SafetyState::Healthy,
            protected_root_signature: signature("recovered-epoch"),
        }
    }

    fn activate_policy_fixture() -> ActivatedFixture {
        let temp = TempDir::new().unwrap();
        let backend = SharedProtectedBackend::default();
        let mut store = open_bootstrapped(&temp, backend.clone());
        let candidate = digest("release-candidate");
        let evidence = record_evidence_through(
            &mut store,
            "agent-a",
            AutonomyTier::A3AutonomousLand,
            &candidate,
        );
        let grant = grant_fixture(
            "agent-a",
            ActiveMode::PolicyAutonomous,
            AutonomyTier::A3AutonomousLand,
            evidence.evidence_digest,
            1,
        );
        let activation_intent = intent_fixture(
            store.state().unwrap(),
            None,
            AuthorityVariant::Human,
            "autonomy.activate",
            Some(candidate.clone()),
        );
        store
            .persist_sovereign_intent(&activation_intent, NOW)
            .unwrap();
        let decision = human_decision(&activation_intent);
        let previous = store.state().unwrap().autonomy_epoch.clone();
        let constitution = store.state().unwrap().constitution.clone();
        let independence = store.state().unwrap().independence_spec.clone();
        let evidence_set =
            compute_tier_evidence_set_digest(&store.state().unwrap().tier_evidence).unwrap();
        let mut receipt = AutonomyActivationReceiptV1 {
            schema: AUTONOMY_ACTIVATION_RECEIPT_SCHEMA.to_owned(),
            core: AutonomyActivationReceiptCoreV1 {
                receipt_id: String::new(),
                previous_mode_epoch_digest: compute_autonomy_epoch_reference_digest(&previous)
                    .unwrap(),
                previous_mode: ActiveMode::HumanGated,
                previous_constitution_epoch: previous.constitution_epoch,
                previous_autonomy_epoch: previous.autonomy_epoch,
                previous_activation_receipt_id: None,
                target_constitution_digest: constitution.constitution_digest.clone(),
                target_constitution_epoch: constitution.core.constitution_epoch,
                activated_autonomy_epoch: 1,
                activated_mode: ActiveMode::PolicyAutonomous,
                grants_digest: compute_grants_digest(std::slice::from_ref(&grant)).unwrap(),
                release_candidate_digest: candidate.clone(),
                gate_receipts_digest: digest("g0-g9-gates"),
                g9_canary_receipts_digest: evidence_set,
                authority_decision_digest: decision.decision_digest().to_owned(),
                prior_authority_variant: AuthorityVariant::Human,
                custody_floor: crate::SECURE_ENCLAVE_CUSTODY_FLOOR_V1.to_owned(),
                rollback_plan_digest: digest("activation-rollback"),
                activates_at: NOW,
                issuer_subject_id: "owner-human".to_owned(),
            },
            receipt_digest: digest("unsealed-activation"),
            signature: signature("activation"),
        };
        receipt.seal().unwrap();
        let target_epoch = AutonomyEpochV1 {
            schema: AUTONOMY_EPOCH_SCHEMA.to_owned(),
            autonomy_epoch: 1,
            active_mode: ActiveMode::PolicyAutonomous,
            activation_receipt_id: Some(receipt.core.receipt_id.clone()),
            constitution_digest: constitution.constitution_digest.clone(),
            constitution_epoch: constitution.core.constitution_epoch,
            grants_digest: compute_grants_digest(std::slice::from_ref(&grant)).unwrap(),
            issuance_frozen: false,
            safety_state: SafetyState::Healthy,
            protected_root_signature: signature("policy-epoch"),
        };
        store
            .activate_mode(
                &activation_intent.intent_digest,
                &decision,
                None,
                receipt,
                constitution,
                independence,
                target_epoch,
                vec![grant.clone()],
                &candidate,
                NOW,
            )
            .unwrap();
        ActivatedFixture {
            _temp: temp,
            backend,
            store,
            candidate,
            grant,
        }
    }

    fn sentinel_fixture(
        intent: &SovereignActionIntentV1,
        kernel: &SafetyKernelV1,
        verdict: SentinelVerdict,
    ) -> SentinelVerdictV1 {
        let mut value = SentinelVerdictV1 {
            schema: SENTINEL_VERDICT_SCHEMA.to_owned(),
            core: SentinelVerdictCoreV1 {
                verdict_id: format!("sentinel-{verdict:?}"),
                sentinel_identity_key_binary_policy_digest: kernel
                    .core
                    .sentinel_identity_key_binary_policy_digest
                    .clone(),
                intent_digest: intent.intent_digest.clone(),
                intent_core_ref: intent.intent_core_ref.clone(),
                intent_canonicalization_version: CANONICALIZATION_VERSION.to_owned(),
                metric_evidence_rollback_digest: digest("sentinel-metrics"),
                risk_scope_digest: intent.core.risk_scope_digest.clone(),
                constitution_epoch: intent.core.constitution_epoch,
                autonomy_epoch: intent.core.autonomy_epoch,
                nonce: intent.core.nonce.clone(),
                issued_at: ISSUED_AT + 10,
                expires_at: EXPIRES_AT - 10,
                verdict,
            },
            verdict_digest: digest("unsealed-sentinel"),
            signature: signature("sentinel"),
        };
        value.seal().unwrap();
        value
    }

    fn quorum_decision(
        state: &AutonomyRuntimeStateV1,
        intent: &SovereignActionIntentV1,
        sentinel: &SentinelVerdictV1,
    ) -> AuthorityDecisionV1 {
        let votes = state
            .independence_spec
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
                signature: signature(&format!("quorum-vote-{}", seat.principal_id)),
            })
            .collect();
        let binding = AuthorityDecisionBindingV1::from_intent(
            intent,
            format!("quorum-decision-{}", &intent.intent_digest[..12]),
            true,
            Some(sentinel.verdict_digest.clone()),
        );
        let mut decision = AuthorityDecisionV1::AgentQuorum(AgentQuorumAuthorityDecisionV1 {
            schema: AUTHORITY_DECISION_SCHEMA.to_owned(),
            core: AgentQuorumAuthorityDecisionCoreV1 {
                binding,
                quorum: AgentQuorumDecisionEvidenceV1 {
                    independence_spec: state.independence_spec.clone(),
                    votes,
                    sentinel_verdict_digest: sentinel.verdict_digest.clone(),
                },
                evidence_rollout_rollback_digest: digest("quorum-evidence-rollout-rollback"),
            },
            decision_digest: digest("unsealed-quorum-decision"),
            owner_signature: signature("quorum-decision"),
        });
        decision.seal().unwrap();
        decision
    }

    fn autonomy_capability(
        state: &AutonomyRuntimeStateV1,
        intent: &SovereignActionIntentV1,
        decision: &AuthorityDecisionV1,
        grant: &AutonomyGrantV1,
        sentinel: &SentinelVerdictV1,
    ) -> AutonomyCapabilityV1 {
        let mut capability = AutonomyCapabilityV1 {
            schema: AUTONOMY_CAPABILITY_SCHEMA.to_owned(),
            core: AutonomyCapabilityCoreV1 {
                capability_id: format!("capability-{}", &intent.intent_digest[..12]),
                intent_digest: intent.intent_digest.clone(),
                intent_core_ref: intent.intent_core_ref.clone(),
                intent_canonicalization_version: CANONICALIZATION_VERSION.to_owned(),
                decision_digest: decision.decision_digest().to_owned(),
                decision_policy_digest: state.independence_spec.independence_spec_digest.clone(),
                required_authority_variant: AuthorityVariant::AgentQuorum,
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
                activation_receipt_id: state.autonomy_epoch.activation_receipt_id.clone(),
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
                idempotency_key: format!("capability-once-{}", &intent.intent_digest[..12]),
                payload_digest: intent.core.action_payload_digest.clone(),
                nonce: intent.core.nonce.clone(),
                issued_at: ISSUED_AT + 20,
                expires_at: EXPIRES_AT - 20,
            },
            capability_digest: digest("unsealed-capability"),
            owner_signature: signature("autonomy-capability"),
        };
        capability.seal().unwrap();
        capability
    }

    fn outbox_record(
        verdict: &SentinelVerdictV1,
        intent: &SovereignActionIntentV1,
        previous: Option<&SentinelRedOutboxV1>,
        state: RedOutboxState,
        terminal_transaction_id: Option<String>,
    ) -> SentinelRedOutboxV1 {
        let epoch = previous.map_or(1, |value| value.core.outbox_epoch + 1);
        let (journal_latch_ack, actuator_ack) = match state {
            RedOutboxState::Pending => (false, false),
            RedOutboxState::LatchAcknowledged => (true, false),
            RedOutboxState::ActuatorAcknowledged | RedOutboxState::Terminal => (true, true),
        };
        let mut outbox = SentinelRedOutboxV1 {
            schema: SENTINEL_RED_OUTBOX_SCHEMA.to_owned(),
            core: SentinelRedOutboxCoreV1 {
                red_verdict_digest: verdict.verdict_digest.clone(),
                source_intent_digest: intent.intent_digest.clone(),
                outbox_epoch: epoch,
                previous_outbox_root_digest: previous
                    .map(|value| value.core.signed_outbox_root_digest.clone()),
                signed_outbox_root_digest: digest(&format!("outbox-root-{epoch}")),
                protected_latest_outbox_epoch: epoch,
                delivery_attempt: epoch,
                journal_latch_ack,
                actuator_ack,
                terminal_safety_transaction_id: terminal_transaction_id,
                state,
            },
            record_digest: digest("unsealed-outbox"),
            root_signature: signature("sentinel-outbox"),
        };
        outbox.seal().unwrap();
        outbox
    }

    fn pending_latch(
        verdict: &SentinelVerdictV1,
        intent: &SovereignActionIntentV1,
        outbox: &SentinelRedOutboxV1,
        kernel: &SafetyKernelV1,
    ) -> RedLatchReceiptV1 {
        let effects_digest =
            compute_safety_effects_digest(&kernel.core.allowed_negative_effects).unwrap();
        let mut latch = RedLatchReceiptV1 {
            schema: RED_LATCH_RECEIPT_SCHEMA.to_owned(),
            core: RedLatchCoreV1 {
                latch_receipt_id: "red-latch-1".to_owned(),
                red_verdict_digest: verdict.verdict_digest.clone(),
                source_intent_digest: intent.intent_digest.clone(),
                sentinel_outbox_epoch: outbox.core.outbox_epoch,
                sentinel_outbox_root_digest: outbox.core.signed_outbox_root_digest.clone(),
                latched_at: NOW,
                protected_time_evidence_digest: digest("protected-time"),
                constitution_epoch: intent.core.constitution_epoch,
                autonomy_epoch: intent.core.autonomy_epoch,
                latch_epoch: 1,
                exact_affected_scope_digest: digest("affected-grants-scope"),
                allowed_negative_actions_digest: effects_digest,
                rollback_candidate_plan_digest: digest("red-rollback-plan"),
                immutable_negative_mandate_digest: digest("negative-mandate"),
                committing_transaction_id: None,
                commit_marker_digest: None,
                terminal_safety_transaction_id: None,
                state: RedLatchState::Pending,
            },
            latch_receipt_digest: digest("unsealed-latch"),
            owner_kernel_signature: signature("red-latch"),
        };
        latch.seal().unwrap();
        latch
    }

    fn safety_attempt(
        source: &SovereignActionIntentV1,
        verdict: &SentinelVerdictV1,
        latch: &RedLatchReceiptV1,
        kernel: &SafetyKernelV1,
    ) -> (
        SafetyActionIntentV1,
        SafetyCapabilityV1,
        AuthorityDecisionV1,
    ) {
        let effects = kernel.core.allowed_negative_effects.clone();
        let mut intent = SafetyActionIntentV1 {
            schema: SAFETY_ACTION_INTENT_SCHEMA.to_owned(),
            core: SafetyActionIntentCoreV1 {
                safety_attempt_id: "safety-attempt-1".to_owned(),
                attempt_sequence: 1,
                rebased_from_attempt_digest: None,
                source_intent_digest: source.intent_digest.clone(),
                source_intent_core_ref: source.intent_core_ref.clone(),
                sentinel_red_verdict_digest: verdict.verdict_digest.clone(),
                red_latch_receipt_digest: latch.latch_receipt_digest.clone(),
                actuator_identity_key_binary_policy_digest: kernel
                    .core
                    .safety_actuator_identity_key_binary_policy_digest
                    .clone(),
                expected_constitution_epoch: source.core.constitution_epoch,
                expected_autonomy_epoch: source.core.autonomy_epoch,
                affected_grants_scope_digest: latch.core.exact_affected_scope_digest.clone(),
                negative_effects: effects.clone(),
                allowed_negative_actions_digest: latch.core.allowed_negative_actions_digest.clone(),
                rollback_candidate_plan_digest: latch.core.rollback_candidate_plan_digest.clone(),
                nonce: "safety-nonce-1".to_owned(),
                attempt_idempotency_key: "safety-idempotency-1".to_owned(), // gitleaks:allow
                issued_at: NOW,
                valid_while_latch_pending: true,
            },
            safety_intent_digest: digest("unsealed-safety-intent"),
            safety_intent_core_ref: IntentCoreRefV1::for_safety_digest(digest(
                "unsealed-safety-intent",
            )),
        };
        intent.seal().unwrap();
        let mut capability = SafetyCapabilityV1 {
            schema: SAFETY_CAPABILITY_SCHEMA.to_owned(),
            core: SafetyCapabilityCoreV1 {
                capability_id: "safety-capability-1".to_owned(),
                safety_intent_digest: intent.safety_intent_digest.clone(),
                safety_intent_core_ref: intent.safety_intent_core_ref.clone(),
                safety_attempt_id: intent.core.safety_attempt_id.clone(),
                source_intent_digest: intent.core.source_intent_digest.clone(),
                sentinel_red_verdict_digest: verdict.verdict_digest.clone(),
                red_latch_receipt_digest: latch.latch_receipt_digest.clone(),
                actuator_identity_key_binary_policy_digest: intent
                    .core
                    .actuator_identity_key_binary_policy_digest
                    .clone(),
                expected_constitution_epoch: intent.core.expected_constitution_epoch,
                expected_autonomy_epoch: intent.core.expected_autonomy_epoch,
                affected_grants_scope_digest: intent.core.affected_grants_scope_digest.clone(),
                negative_effects: effects.clone(),
                allowed_negative_actions_digest: intent
                    .core
                    .allowed_negative_actions_digest
                    .clone(),
                rollback_candidate_plan_digest: intent.core.rollback_candidate_plan_digest.clone(),
                nonce: intent.core.nonce.clone(),
                idempotency_key: intent.core.attempt_idempotency_key.clone(),
                issued_at: NOW,
                expires_at: EXPIRES_AT - 1,
            },
            capability_digest: digest("unsealed-safety-capability"),
            actuator_signature: signature("safety-capability"),
        };
        capability.seal().unwrap();
        let mut decision = AuthorityDecisionV1::Safety(SafetyAuthorityDecisionV1 {
            schema: AUTHORITY_DECISION_SCHEMA.to_owned(),
            core: SafetyAuthorityDecisionCoreV1 {
                decision_id: "safety-decision-1".to_owned(),
                safety_intent_digest: intent.safety_intent_digest.clone(),
                safety_intent_core_ref: intent.safety_intent_core_ref.clone(),
                safety_capability_digest: capability.capability_digest.clone(),
                sentinel_red_verdict_digest: verdict.verdict_digest.clone(),
                red_latch_receipt_digest: latch.latch_receipt_digest.clone(),
                negative_effects: effects,
                positive_authority_decision_forbidden: true,
                issuer_subject_id: "safety-kernel".to_owned(),
            },
            decision_digest: digest("unsealed-safety-decision"),
            safety_kernel_signature: signature("safety-decision"),
        });
        decision.seal().unwrap();
        (intent, capability, decision)
    }

    fn terminal_latch(
        pending: &RedLatchReceiptV1,
        outbox: &SentinelRedOutboxV1,
        transaction_id: &str,
        marker: String,
    ) -> RedLatchReceiptV1 {
        let mut terminal = pending.clone();
        terminal.core.sentinel_outbox_epoch = outbox.core.outbox_epoch;
        terminal.core.sentinel_outbox_root_digest = outbox.core.signed_outbox_root_digest.clone();
        terminal.core.committing_transaction_id = Some(transaction_id.to_owned());
        terminal.core.commit_marker_digest = Some(marker);
        terminal.core.terminal_safety_transaction_id = Some(transaction_id.to_owned());
        terminal.core.state = RedLatchState::Terminal;
        terminal.owner_kernel_signature = signature("terminal-red-latch");
        terminal.seal().unwrap();
        terminal
    }

    #[test]
    fn production_configuration_refuses_software_only_backend_and_verifier() {
        let temp = TempDir::new().unwrap();
        let result = AutonomyRuntimeStore::open(
            AutonomyRuntimeConfig::production(
                temp.path(),
                "production-domain",
                "organism-1",
                "repo-1",
                "brain-1",
            ),
            SharedProtectedBackend::default(),
            SoftwareTestVerifier,
            NOW,
        );
        assert!(matches!(
            result,
            Err(AutonomyRuntimeError::AssuranceTooLow {
                required: AutonomyRuntimeAssurance::ProtectedProduction,
                actual: AutonomyRuntimeAssurance::SoftwareTestOnlyNotProduction,
                ..
            })
        ));
    }

    #[test]
    fn bootstrap_is_durable_human_gated_and_prepared_recovery_forward_completes() {
        let temp = TempDir::new().unwrap();
        let backend = SharedProtectedBackend::default();
        backend.fail_on_call(2);
        let mut store = AutonomyRuntimeStore::open(
            config(temp.path()),
            backend.clone(),
            SoftwareTestVerifier,
            NOW,
        )
        .unwrap();
        let kernel = kernel_fixture();
        let independence = independence_fixture(0);
        let constitution = constitution_fixture(&independence);
        let error = store
            .bootstrap(
                kernel,
                independence,
                constitution.clone(),
                bootstrap_epoch(&constitution),
                NOW,
            )
            .unwrap_err();
        assert!(matches!(
            error,
            AutonomyRuntimeError::ProtectedBackend {
                operation: "commit_compare_and_swap",
                ..
            }
        ));
        assert!(store.is_poisoned());
        drop(store);

        let recovered =
            AutonomyRuntimeStore::open(config(temp.path()), backend, SoftwareTestVerifier, NOW)
                .unwrap();
        assert_eq!(
            recovered.state().unwrap().active_mode(),
            ActiveMode::HumanGated
        );
        assert_eq!(recovered.state().unwrap().autonomy_epoch.autonomy_epoch, 0);
        assert!(recovered.state().unwrap().activation_receipts.is_empty());
        assert_eq!(
            recovered.protected_root().unwrap().phase,
            ProtectedAutonomyPhaseV1::Committed
        );
    }

    #[test]
    fn shadow_canary_a0_through_a5_never_changes_active_mode() {
        let temp = TempDir::new().unwrap();
        let backend = SharedProtectedBackend::default();
        let mut store = open_bootstrapped(&temp, backend);
        let candidate = digest("candidate-a0-a5");
        let last = record_evidence_through(
            &mut store,
            "agent-a",
            AutonomyTier::A5FullAutonomy,
            &candidate,
        );
        assert_eq!(last.tier, AutonomyTier::A5FullAutonomy);
        assert_eq!(store.state().unwrap().active_mode(), ActiveMode::HumanGated);
        assert_eq!(store.state().unwrap().autonomy_epoch.autonomy_epoch, 0);
        assert!(store.state().unwrap().active_grants.is_empty());

        let mut self_promoting =
            tier_evidence("agent-b", AutonomyTier::A0Observe, None, &candidate);
        self_promoting.evaluator_subject_id = "agent-b".to_owned();
        self_promoting.seal().unwrap();
        assert!(matches!(
            store.record_tier_evidence(self_promoting, NOW),
            Err(AutonomyRuntimeError::SelfPromotion { .. })
        ));
    }

    #[test]
    fn exact_prior_authority_activation_is_explicit_and_survives_restart() {
        let fixture = activate_policy_fixture();
        assert_eq!(
            fixture.store.state().unwrap().active_mode(),
            ActiveMode::PolicyAutonomous
        );
        assert_eq!(
            fixture.store.state().unwrap().active_grants,
            vec![fixture.grant]
        );
        assert_eq!(fixture.store.state().unwrap().activation_receipts.len(), 1);
        let root = fixture._temp.path().to_path_buf();
        let backend = fixture.backend.clone();
        drop(fixture.store);
        let reopened =
            AutonomyRuntimeStore::open(config(&root), backend, SoftwareTestVerifier, NOW).unwrap();
        assert_eq!(
            reopened.state().unwrap().active_mode(),
            ActiveMode::PolicyAutonomous
        );
        assert_eq!(reopened.state().unwrap().autonomy_epoch.autonomy_epoch, 1);
    }

    #[test]
    fn prior_authority_cannot_issue_its_own_target_grant() {
        let temp = TempDir::new().unwrap();
        let backend = SharedProtectedBackend::default();
        let mut store = open_bootstrapped(&temp, backend);
        let candidate = digest("self-grant-candidate");
        let evidence = record_evidence_through(
            &mut store,
            "owner-human",
            AutonomyTier::A3AutonomousLand,
            &candidate,
        );
        let grant = grant_fixture(
            "owner-human",
            ActiveMode::PolicyAutonomous,
            AutonomyTier::A3AutonomousLand,
            evidence.evidence_digest,
            1,
        );
        let activation_intent = intent_fixture(
            store.state().unwrap(),
            None,
            AuthorityVariant::Human,
            "autonomy.activate",
            Some(candidate.clone()),
        );
        store
            .persist_sovereign_intent(&activation_intent, NOW)
            .unwrap();
        let decision = human_decision(&activation_intent);
        let previous = store.state().unwrap().autonomy_epoch.clone();
        let constitution = store.state().unwrap().constitution.clone();
        let independence = store.state().unwrap().independence_spec.clone();
        let evidence_set =
            compute_tier_evidence_set_digest(&store.state().unwrap().tier_evidence).unwrap();
        let mut receipt = AutonomyActivationReceiptV1 {
            schema: AUTONOMY_ACTIVATION_RECEIPT_SCHEMA.to_owned(),
            core: AutonomyActivationReceiptCoreV1 {
                receipt_id: String::new(),
                previous_mode_epoch_digest: compute_autonomy_epoch_reference_digest(&previous)
                    .unwrap(),
                previous_mode: ActiveMode::HumanGated,
                previous_constitution_epoch: previous.constitution_epoch,
                previous_autonomy_epoch: previous.autonomy_epoch,
                previous_activation_receipt_id: None,
                target_constitution_digest: constitution.constitution_digest.clone(),
                target_constitution_epoch: constitution.core.constitution_epoch,
                activated_autonomy_epoch: 1,
                activated_mode: ActiveMode::PolicyAutonomous,
                grants_digest: compute_grants_digest(std::slice::from_ref(&grant)).unwrap(),
                release_candidate_digest: candidate.clone(),
                gate_receipts_digest: digest("self-grant-gates"),
                g9_canary_receipts_digest: evidence_set,
                authority_decision_digest: decision.decision_digest().to_owned(),
                prior_authority_variant: AuthorityVariant::Human,
                custody_floor: crate::SECURE_ENCLAVE_CUSTODY_FLOOR_V1.to_owned(),
                rollback_plan_digest: digest("self-grant-rollback"),
                activates_at: NOW,
                issuer_subject_id: "owner-human".to_owned(),
            },
            receipt_digest: digest("unsealed-self-grant-activation"),
            signature: signature("self-grant-activation"),
        };
        receipt.seal().unwrap();
        let target_epoch = AutonomyEpochV1 {
            schema: AUTONOMY_EPOCH_SCHEMA.to_owned(),
            autonomy_epoch: 1,
            active_mode: ActiveMode::PolicyAutonomous,
            activation_receipt_id: Some(receipt.core.receipt_id.clone()),
            constitution_digest: constitution.constitution_digest.clone(),
            constitution_epoch: constitution.core.constitution_epoch,
            grants_digest: compute_grants_digest(std::slice::from_ref(&grant)).unwrap(),
            issuance_frozen: false,
            safety_state: SafetyState::Healthy,
            protected_root_signature: signature("self-grant-epoch"),
        };
        assert!(matches!(
            store.activate_mode(
                &activation_intent.intent_digest,
                &decision,
                None,
                receipt,
                constitution,
                independence,
                target_epoch,
                vec![grant],
                &candidate,
                NOW,
            ),
            Err(AutonomyRuntimeError::SelfPromotion { subject_id })
                if subject_id == "owner-human"
        ));
        assert_eq!(store.state().unwrap().active_mode(), ActiveMode::HumanGated);
        assert!(store.state().unwrap().active_grants.is_empty());
        assert!(store.state().unwrap().activation_receipts.is_empty());
    }

    #[test]
    fn agent_quorum_three_of_four_verifies_each_vote_and_is_one_shot() {
        let mut fixture = activate_policy_fixture();
        let intent = intent_fixture(
            fixture.store.state().unwrap(),
            Some(&fixture.grant),
            AuthorityVariant::AgentQuorum,
            "land",
            Some(fixture.candidate.clone()),
        );
        fixture
            .store
            .persist_sovereign_intent(&intent, NOW)
            .unwrap();
        let kernel = fixture.store.state().unwrap().kernel.clone();
        let sentinel = sentinel_fixture(&intent, &kernel, SentinelVerdict::Green);
        let mut decision = quorum_decision(fixture.store.state().unwrap(), &intent, &sentinel);
        let AuthorityDecisionV1::AgentQuorum(quorum) = &mut decision else {
            unreachable!("fixture always emits AGENT_QUORUM")
        };
        quorum.core.quorum.votes.pop();
        decision.seal().unwrap();
        let AuthorityDecisionV1::AgentQuorum(quorum) = &decision else {
            unreachable!("fixture always emits AGENT_QUORUM")
        };
        assert_eq!(quorum.core.quorum.votes.len(), 3);
        assert_eq!(
            quorum
                .core
                .quorum
                .votes
                .iter()
                .filter(|vote| vote.disposition == QuorumVoteDisposition::Approve)
                .count(),
            3
        );
        assert_eq!(
            quorum
                .core
                .quorum
                .votes
                .iter()
                .map(|vote| vote.failure_domain.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            3
        );

        let mut invalid_vote_decision = decision.clone();
        let AuthorityDecisionV1::AgentQuorum(invalid_quorum) = &mut invalid_vote_decision else {
            unreachable!("fixture always emits AGENT_QUORUM")
        };
        invalid_quorum.core.quorum.votes[0].signature = OpaqueSignature::new("invalid-vote");
        invalid_vote_decision.seal().unwrap();
        let invalid_capability = autonomy_capability(
            fixture.store.state().unwrap(),
            &intent,
            &invalid_vote_decision,
            &fixture.grant,
            &sentinel,
        );
        let invalid_vote_error = fixture
            .store
            .consume_autonomy_capability(
                &intent.intent_digest,
                &invalid_vote_decision,
                &invalid_capability,
                Some(&sentinel),
                NOW,
            )
            .unwrap_err();
        assert!(
            matches!(
                invalid_vote_error,
                AutonomyRuntimeError::Verification {
                    kind: AutonomyArtifactKindV1::QuorumVote,
                    ..
                }
            ),
            "unexpected invalid-vote error: {invalid_vote_error:?}"
        );

        let capability = autonomy_capability(
            fixture.store.state().unwrap(),
            &intent,
            &decision,
            &fixture.grant,
            &sentinel,
        );
        let admission = fixture
            .store
            .consume_autonomy_capability(
                &intent.intent_digest,
                &decision,
                &capability,
                Some(&sentinel),
                NOW,
            )
            .unwrap();
        assert_eq!(admission.authority_variant, AuthorityVariant::AgentQuorum);
        assert!(matches!(
            fixture.store.consume_autonomy_capability(
                &intent.intent_digest,
                &decision,
                &capability,
                Some(&sentinel),
                NOW,
            ),
            Err(AutonomyRuntimeError::CapabilityReplay { .. })
        ));

        let mut self_authorizing_intent = intent_fixture(
            fixture.store.state().unwrap(),
            Some(&fixture.grant),
            AuthorityVariant::AgentQuorum,
            "diagnose",
            Some(fixture.candidate.clone()),
        );
        self_authorizing_intent.core.proposer_subject_id = "verifier-1".to_owned();
        self_authorizing_intent.seal().unwrap();
        fixture
            .store
            .persist_sovereign_intent(&self_authorizing_intent, NOW)
            .unwrap();
        let sentinel = sentinel_fixture(&self_authorizing_intent, &kernel, SentinelVerdict::Green);
        let decision = quorum_decision(
            fixture.store.state().unwrap(),
            &self_authorizing_intent,
            &sentinel,
        );
        let capability = autonomy_capability(
            fixture.store.state().unwrap(),
            &self_authorizing_intent,
            &decision,
            &fixture.grant,
            &sentinel,
        );
        assert!(matches!(
            fixture.store.consume_autonomy_capability(
                &self_authorizing_intent.intent_digest,
                &decision,
                &capability,
                Some(&sentinel),
                NOW,
            ),
            Err(AutonomyRuntimeError::Contract(
                AutonomyContractError::SelfAuthorization { .. }
            ))
        ));
    }

    #[test]
    fn concurrent_owner_and_journal_or_protected_root_rollback_fail_closed() {
        let temp = TempDir::new().unwrap();
        let backend = SharedProtectedBackend::default();
        let mut first = open_bootstrapped(&temp, backend.clone());
        let mut second = AutonomyRuntimeStore::open(
            config(temp.path()),
            backend.clone(),
            SoftwareTestVerifier,
            NOW,
        )
        .unwrap();
        let candidate = digest("concurrent-candidate");
        first
            .record_tier_evidence(
                tier_evidence("agent-a", AutonomyTier::A0Observe, None, &candidate),
                NOW,
            )
            .unwrap();
        assert!(matches!(
            second.record_tier_evidence(
                tier_evidence("agent-b", AutonomyTier::A0Observe, None, &candidate),
                NOW,
            ),
            Err(AutonomyRuntimeError::ConcurrentModification { .. })
        ));
        drop(first);
        drop(second);

        let journal = temp.path().join(JOURNAL_FILE_NAME);
        let bytes = fs::read(&journal).unwrap();
        fs::write(&journal, &bytes[..bytes.len() / 2]).unwrap();
        assert!(matches!(
            AutonomyRuntimeStore::open(
                config(temp.path()),
                backend.clone(),
                SoftwareTestVerifier,
                NOW,
            ),
            Err(AutonomyRuntimeError::CorruptJournal { .. })
                | Err(AutonomyRuntimeError::AntiRollback { .. })
        ));

        let fresh = TempDir::new().unwrap();
        let rollback_backend = SharedProtectedBackend::default();
        let store = open_bootstrapped(&fresh, rollback_backend.clone());
        let root = store.protected_root().unwrap().clone();
        drop(store);
        rollback_backend.force_root(None);
        assert!(matches!(
            AutonomyRuntimeStore::open(
                config(fresh.path()),
                rollback_backend,
                SoftwareTestVerifier,
                NOW,
            ),
            Err(AutonomyRuntimeError::AntiRollback { .. })
        ));
        assert_eq!(root.phase, ProtectedAutonomyPhaseV1::Committed);
    }

    #[test]
    fn red_outbox_latch_and_negative_transaction_are_durable_and_absolute() {
        let mut fixture = activate_policy_fixture();
        let intent = intent_fixture(
            fixture.store.state().unwrap(),
            Some(&fixture.grant),
            AuthorityVariant::Policy,
            "land",
            Some(fixture.candidate.clone()),
        );
        fixture
            .store
            .persist_sovereign_intent(&intent, NOW)
            .unwrap();
        let kernel = fixture.store.state().unwrap().kernel.clone();
        let red = sentinel_fixture(&intent, &kernel, SentinelVerdict::Red);
        let pending_outbox = outbox_record(&red, &intent, None, RedOutboxState::Pending, None);
        fixture
            .store
            .persist_sentinel_red(
                &intent.intent_digest,
                red.clone(),
                pending_outbox.clone(),
                NOW,
            )
            .unwrap();
        assert!(matches!(
            fixture.store.record_tier_evidence(
                tier_evidence("agent-b", AutonomyTier::A0Observe, None, &fixture.candidate,),
                NOW,
            ),
            Err(AutonomyRuntimeError::PositiveAuthorityFrozen)
        ));
        let acknowledged = outbox_record(
            &red,
            &intent,
            Some(&pending_outbox),
            RedOutboxState::LatchAcknowledged,
            None,
        );
        let latch = pending_latch(&red, &intent, &acknowledged, &kernel);
        fixture
            .store
            .latch_sentinel_red(acknowledged.clone(), latch.clone(), NOW)
            .unwrap();

        let (safety_intent, capability, decision) = safety_attempt(&intent, &red, &latch, &kernel);
        fixture
            .store
            .persist_safety_intent(&safety_intent, NOW)
            .unwrap();
        let transaction_id = "safety-transaction-1";
        let terminal_outbox = outbox_record(
            &red,
            &intent,
            Some(&acknowledged),
            RedOutboxState::Terminal,
            Some(transaction_id.to_owned()),
        );
        let marker = red_commit_marker_digest(
            transaction_id,
            decision.decision_digest(),
            &fixture.store.state().unwrap().state_digest,
            fixture.store.state().unwrap().autonomy_epoch.autonomy_epoch + 1,
        )
        .unwrap();
        let terminal_latch = terminal_latch(&latch, &terminal_outbox, transaction_id, marker);
        fixture
            .store
            .commit_red_safety_transition(
                &safety_intent.safety_intent_digest,
                &capability,
                &decision,
                terminal_outbox,
                terminal_latch,
                transaction_id,
                NOW,
            )
            .unwrap();
        let state = fixture.store.state().unwrap();
        assert_eq!(state.active_mode(), ActiveMode::HumanGated);
        assert_eq!(state.autonomy_epoch.autonomy_epoch, 2);
        assert!(state.autonomy_epoch.issuance_frozen);
        assert_eq!(state.autonomy_epoch.safety_state, SafetyState::Frozen);
        assert!(state.active_grants.is_empty());
        assert!(state.pending_red.is_none());
        assert!(state
            .consumed_capability_digests
            .contains(&capability.capability_digest));

        let root = fixture._temp.path().to_path_buf();
        let backend = fixture.backend.clone();
        drop(fixture.store);
        let mut reopened =
            AutonomyRuntimeStore::open(config(&root), backend.clone(), SoftwareTestVerifier, NOW)
                .unwrap();
        assert_eq!(reopened.state().unwrap().autonomy_epoch.autonomy_epoch, 2);
        assert_eq!(reopened.protected_root().unwrap().sentinel_outbox_epoch, 3);
        assert_eq!(reopened.protected_root().unwrap().red_latch_epoch, 1);
        assert_eq!(
            reopened.state().unwrap().last_valid_mode_before_freeze,
            Some(ActiveMode::PolicyAutonomous)
        );
        assert_eq!(reopened.state().unwrap().terminal_red_latches.len(), 1);
        assert!(matches!(
            reopened.persist_sentinel_red(&intent.intent_digest, red, pending_outbox, NOW,),
            Err(AutonomyRuntimeError::InvalidRedTransition { .. })
        ));

        let recovery_intent = recovery_intent(reopened.state().unwrap());
        reopened
            .persist_recovery_intent(&recovery_intent, NOW)
            .unwrap();
        let recovery_sentinel = sentinel_fixture(
            &recovery_intent,
            &reopened.state().unwrap().kernel,
            SentinelVerdict::Green,
        );
        let recovery_decision = human_recovery_decision(&recovery_intent, &recovery_sentinel);
        let recovery_receipt = recovery_receipt(
            reopened.state().unwrap(),
            &recovery_intent,
            &recovery_decision,
            &recovery_sentinel,
        );
        let target_epoch = recovered_epoch(reopened.state().unwrap(), &recovery_receipt);
        reopened
            .recover_from_frozen(
                &recovery_intent.intent_digest,
                &recovery_decision,
                &recovery_sentinel,
                recovery_receipt.clone(),
                target_epoch.clone(),
                NOW,
            )
            .unwrap();
        let recovered = reopened.state().unwrap();
        assert_eq!(recovered.autonomy_epoch.autonomy_epoch, 3);
        assert_eq!(recovered.active_mode(), ActiveMode::HumanGated);
        assert_eq!(recovered.autonomy_epoch.safety_state, SafetyState::Healthy);
        assert!(!recovered.autonomy_epoch.issuance_frozen);
        assert!(recovered.active_grants.is_empty());
        assert!(recovered.tier_evidence.is_empty());
        assert!(recovered.last_valid_mode_before_freeze.is_none());
        assert!(recovered
            .recovery_receipts
            .contains_key(&recovery_receipt.core.receipt_id));
        assert!(matches!(
            reopened.recover_from_frozen(
                &recovery_intent.intent_digest,
                &recovery_decision,
                &recovery_sentinel,
                recovery_receipt,
                target_epoch,
                NOW,
            ),
            Err(AutonomyRuntimeError::RecoveryReplay { .. })
        ));
        reopened
            .record_tier_evidence(
                tier_evidence(
                    "agent-a",
                    AutonomyTier::A0Observe,
                    None,
                    &digest("post-recovery-candidate"),
                ),
                NOW,
            )
            .unwrap();

        drop(reopened);
        let final_reopen =
            AutonomyRuntimeStore::open(config(&root), backend, SoftwareTestVerifier, NOW).unwrap();
        assert_eq!(
            final_reopen.state().unwrap().autonomy_epoch.safety_state,
            SafetyState::Healthy
        );
        assert_eq!(
            final_reopen.state().unwrap().autonomy_epoch.autonomy_epoch,
            3
        );
        assert_eq!(final_reopen.state().unwrap().recovery_receipts.len(), 1);
    }

    #[test]
    fn kernel_negative_allow_list_cannot_be_used_for_positive_payload_effects() {
        let kernel = kernel_fixture();
        assert!(kernel
            .core
            .allowed_negative_effects
            .iter()
            .all(|effect| effect.is_negative_safety()));
        assert!(!kernel
            .core
            .allowed_negative_effects
            .contains(&Effect::SourceFilesystemWrite));
        assert!(!kernel
            .core
            .allowed_negative_effects
            .contains(&Effect::SovereignMutation));
    }
}
