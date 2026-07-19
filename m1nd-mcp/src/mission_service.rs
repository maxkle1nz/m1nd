//! M1ND-10 G3 owner-side mission state and landing service.
//!
//! This module is intentionally transport-independent. It owns the only public
//! API that can append a G3 [`MissionLetterV1`], validates the closed mission
//! state machine from `m1nd-control`, and implements an internal landing
//! transaction over [`AuthorityWal`]. HTTP/MCP wiring is deliberately outside
//! this slice.
//!
//! # Authentication boundary
//!
//! `AuthenticatedAuthorityContextV1` is an *injected result* from a future G2
//! identity/signature verifier. This module checks its bindings, lifetime and
//! verification-receipt digest, but it does not authenticate keys itself.
//! AuthorityWAL record signatures are created only after the writer assigns the
//! exact sequence and previous-root fields, through an injected owner signer;
//! request contexts cannot supply phase signatures. Production open remains
//! fail-closed until that signer/verifier is installed.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
#[cfg(any(target_vendor = "apple", target_os = "linux", target_os = "android"))]
use std::ffi::CString;
use std::ffi::OsString;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
#[cfg(any(target_vendor = "apple", target_os = "linux", target_os = "android"))]
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use m1nd_control::{
    digest_canonical, ActiveMode, AuthorityTransactionV1, AuthorityVariant, AuthorityWalAbortV1,
    AuthorityWalCommitV1, AuthorityWalPayloadV1, AuthorityWalPhase, AuthorityWalPrepareV1,
    AuthorityWalProvisionalV1, CanonicalError, CapabilityKind, Effect, ExecutionDispatchAckV1,
    ExecutionDispatchV1, ExecutionOutcome, ExecutionResultV1, Ingress, MissionContractError,
    MissionHeadContext, MissionHeadSnapshot, MissionState, MissionTransitionIntentV1,
    MissionTransitionSource, OpaqueSignature, PositiveAuthorityTransactionV1, ReviewResultV1, Role,
    CANONICALIZATION_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::authority_wal::{
    AuthorityWal, AuthorityWalAppendOutcome, AuthorityWalError, AuthorityWalRecordCrypto,
    AuthorityWalTerminalOutcome, SoftwareTestAuthorityWalRecordCrypto,
};
use crate::execution_dispatch::{
    DispatchMutation, ExecutionDispatchError, ExecutionMissionHeadV1, OwnerDispatchEntryV1,
    OwnerExecutionOutbox, OwnerIntentRegistration, OwnerReconciliationAction, RunnerInboxEntryV1,
    RunnerInboxState, EXECUTION_MISSION_HEAD_SCHEMA,
};
use crate::protected_journal_head::SharedProtectedJournalHeadBackendV1;
use crate::system_blocks::ReceiptType;

pub const MISSION_SERVICE_CONFIG_SCHEMA: &str = "m1nd-mission-service-config-v1";
pub const AUTHENTICATED_AUTHORITY_CONTEXT_SCHEMA: &str = "m1nd-authenticated-authority-context-v1";
pub const EVIDENCE_REF_SCHEMA: &str = "m1nd-evidence-ref-v1";
pub const RECEIPT_CANDIDATE_SCHEMA: &str = "m1nd-receipt-candidate-v1";
pub const RECEIPT_SCHEMA: &str = "m1nd-receipt-v1";
pub const MISSION_SERVICE_DECISION_SCHEMA: &str = "m1nd-mission-service-decision-v1";
pub const AUTHOR_PROPOSAL_SCHEMA: &str = "m1nd-author-proposal-v1";
pub const MISSION_TRANSITION_PAYLOAD_SCHEMA: &str = "m1nd-mission-transition-payload-v1";
pub const MISSION_LETTER_V1_SCHEMA: &str = "m1nd-mission-letter-v1";
pub const MISSION_SERVICE_STATE_SCHEMA: &str = "m1nd-mission-service-state-v1";
pub const LAND_REQUEST_SCHEMA: &str = "m1nd-land-request-v1";
pub const LAND_INTENT_CORE_SCHEMA: &str = "m1nd-land-intent-core-v1";
pub const LAND_PROVISIONAL_PLAN_SCHEMA: &str = "m1nd-land-provisional-plan-v1";
pub const LAND_OUTCOME_SCHEMA: &str = "m1nd-land-outcome-v1";
pub const MISSION_SERVICE_EXECUTION_OUTBOX_FILE: &str = "execution-owner-outbox.jsonl";

pub(crate) const MISSION_SERVICE_STATE_FILE: &str = "mission-service-state.json";
const MISSION_SERVICE_WAL_FILE: &str = "authority.wal.jsonl";
const MISSION_SERVICE_PLANS_DIR: &str = "land-plans";
static DURABLE_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const EVIDENCE_REF_DIGEST_DOMAIN: &str = "m1nd-evidence-ref-v1";
const RECEIPT_CANDIDATE_DIGEST_DOMAIN: &str = "m1nd-receipt-candidate-v1";
const RECEIPT_DIGEST_DOMAIN: &str = "m1nd-receipt-v1";
const MISSION_SERVICE_DECISION_DIGEST_DOMAIN: &str = "m1nd-mission-service-decision-v1";
const AUTHOR_PROPOSAL_DIGEST_DOMAIN: &str = "m1nd-author-proposal-v1";
const MISSION_LETTER_DIGEST_DOMAIN: &str = "m1nd-mission-letter-v1";
const LAND_INTENT_DIGEST_DOMAIN: &str = "m1nd-land-intent-core-v1";
const LAND_PROVISIONAL_DIGEST_DOMAIN: &str = "m1nd-land-provisional-v1";
const LAND_OUTCOME_DIGEST_DOMAIN: &str = "m1nd-land-outcome-v1";
const IDEMPOTENCY_SCOPE_DIGEST_DOMAIN: &str = "m1nd-mission-idempotency-scope-v1";

#[derive(Debug)]
pub enum MissionServiceError {
    Io(io::Error),
    Json(serde_json::Error),
    Canonical(CanonicalError),
    MissionContract(MissionContractError),
    AuthorityWal(AuthorityWalError),
    ExecutionDispatch(ExecutionDispatchError),
    Refused { code: &'static str, detail: String },
    Corruption { detail: String },
    SimulatedCrash { point: LandCrashPoint },
    ExecutionSimulatedCrash { point: ExecutionLifecycleCrashPoint },
}

impl MissionServiceError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "mission_service_io",
            Self::Json(_) => "mission_service_json",
            Self::Canonical(_) => "canonicalization_failed",
            Self::MissionContract(error) => match error {
                MissionContractError::IllegalTransition { .. } => "illegal_transition",
                MissionContractError::BrainMismatch { .. } => "brain_mismatch",
                MissionContractError::MissionMismatch { .. } => "mission_mismatch",
                MissionContractError::StaleHead { .. } => "stale_head",
                MissionContractError::StaleIteration { .. } => "stale_iteration",
                MissionContractError::WrongRole { .. }
                | MissionContractError::WrongSource { .. }
                | MissionContractError::BindingMismatch { .. } => "wrong_author",
                MissionContractError::StateMismatch { .. } => "state_mismatch",
                MissionContractError::PacketDigestMismatch
                | MissionContractError::PacketDigestNotAdvanced => "packet_mismatch",
                _ => "mission_contract_refused",
            },
            Self::AuthorityWal(_) => "authority_wal_refused",
            Self::ExecutionDispatch(error) => error.code(),
            Self::Refused { code, .. } => code,
            Self::Corruption { .. } => "mission_service_corruption",
            Self::SimulatedCrash { .. } => "simulated_crash",
            Self::ExecutionSimulatedCrash { .. } => "simulated_execution_lifecycle_crash",
        }
    }

    pub(crate) fn refused(code: &'static str, detail: impl Into<String>) -> Self {
        Self::Refused {
            code,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for MissionServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "MissionService I/O error: {error}"),
            Self::Json(error) => write!(formatter, "MissionService JSON error: {error}"),
            Self::Canonical(error) => {
                write!(formatter, "MissionService canonicalization error: {error}")
            }
            Self::MissionContract(error) => write!(formatter, "mission contract refused: {error}"),
            Self::AuthorityWal(error) => write!(formatter, "AuthorityWAL refused: {error}"),
            Self::ExecutionDispatch(error) => {
                write!(formatter, "execution dispatch lifecycle refused: {error}")
            }
            Self::Refused { code, detail } => write!(formatter, "{code}: {detail}"),
            Self::Corruption { detail } => {
                write!(formatter, "mission_service_corruption: {detail}")
            }
            Self::SimulatedCrash { point } => write!(formatter, "simulated crash at {point:?}"),
            Self::ExecutionSimulatedCrash { point } => {
                write!(
                    formatter,
                    "simulated execution lifecycle crash at {point:?}"
                )
            }
        }
    }
}

impl Error for MissionServiceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Canonical(error) => Some(error),
            Self::MissionContract(error) => Some(error),
            Self::AuthorityWal(error) => Some(error),
            Self::ExecutionDispatch(error) => Some(error),
            Self::Refused { .. }
            | Self::Corruption { .. }
            | Self::SimulatedCrash { .. }
            | Self::ExecutionSimulatedCrash { .. } => None,
        }
    }
}

impl From<io::Error> for MissionServiceError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for MissionServiceError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<CanonicalError> for MissionServiceError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<MissionContractError> for MissionServiceError {
    fn from(error: MissionContractError) -> Self {
        Self::MissionContract(error)
    }
}

impl From<AuthorityWalError> for MissionServiceError {
    fn from(error: AuthorityWalError) -> Self {
        Self::AuthorityWal(error)
    }
}

impl From<ExecutionDispatchError> for MissionServiceError {
    fn from(error: ExecutionDispatchError) -> Self {
        Self::ExecutionDispatch(error)
    }
}

pub type MissionServiceResult<T> = Result<T, MissionServiceError>;

/// Object-safe owner seam around the exact AuthorityWAL COMMIT append. A
/// production G2 broker uses it to revalidate and consume a one-shot lease
/// under the named owner linearization point. Test-only/direct callers may use
/// `land`, which remains useful for core state-machine proof but is not the
/// production broker path.
pub trait AuthorityWalCommitCoordinator: Send + Sync {
    fn append_commit(
        &self,
        authority: &AuthenticatedAuthorityContextV1,
        transaction: &AuthorityTransactionV1,
        committed_at: u64,
        append: &mut dyn FnMut() -> MissionServiceResult<AuthorityWalAppendOutcome>,
    ) -> MissionServiceResult<AuthorityWalAppendOutcome>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthenticationDisposition {
    UpstreamAuthenticationTrustedNotReverified,
}

/// An already-authenticated authority result injected by the owner boundary.
/// Constructing this value is not itself authentication; callers must only
/// construct it from a successful G2 verifier result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthenticatedAuthorityContextV1 {
    pub schema: String,
    pub organism_id: String,
    pub brain_id: String,
    pub subject_id: String,
    pub role: Role,
    pub capability_id: String,
    pub capability_kind: Option<CapabilityKind>,
    pub authority_variant: AuthorityVariant,
    pub active_mode: ActiveMode,
    pub mission_id: Option<String>,
    pub mission_head_id: Option<String>,
    pub transport_session_id: String,
    pub ingress_context_digest: String,
    pub action_id: String,
    pub ingress: Ingress,
    pub complete_effects: BTreeSet<Effect>,
    pub verified_object_digest: String,
    pub authorization_snapshot_digest: String,
    pub authority_decision_digest: Option<String>,
    pub identity_role_binding_digest: Option<String>,
    pub upstream_verification_receipt_digest: String,
    pub protected_time_evidence_digest: String,
    pub constitution_digest: String,
    pub constitution_epoch: u64,
    pub autonomy_epoch: u64,
    pub protected_epoch: u64,
    pub policy_registry_digest: String,
    pub authorization_lease_id: String,
    pub authorization_reservation_id: String,
    pub authenticated_at: u64,
    pub expires_at: u64,
}

impl AuthenticatedAuthorityContextV1 {
    pub const fn authentication_disposition(&self) -> AuthenticationDisposition {
        AuthenticationDisposition::UpstreamAuthenticationTrustedNotReverified
    }

    pub(crate) fn validate_for(
        &self,
        brain_id: &str,
        object_digest: &str,
        now_ms: u64,
    ) -> MissionServiceResult<()> {
        require_schema(
            "authenticated authority context",
            &self.schema,
            AUTHENTICATED_AUTHORITY_CONTEXT_SCHEMA,
        )?;
        for (field, value) in [
            ("authority.organism_id", self.organism_id.as_str()),
            ("authority.brain_id", self.brain_id.as_str()),
            ("authority.subject_id", self.subject_id.as_str()),
            ("authority.capability_id", self.capability_id.as_str()),
            (
                "authority.transport_session_id",
                self.transport_session_id.as_str(),
            ),
            ("authority.action_id", self.action_id.as_str()),
            (
                "authority.authorization_lease_id",
                self.authorization_lease_id.as_str(),
            ),
        ] {
            require_non_empty(field, value)?;
        }
        if self.complete_effects.is_empty() {
            return Err(MissionServiceError::refused(
                "incomplete_authority_context",
                "authenticated authority context has no complete effect set",
            ));
        }
        for (field, digest) in [
            (
                "verified_object_digest",
                self.verified_object_digest.as_str(),
            ),
            (
                "authorization_snapshot_digest",
                self.authorization_snapshot_digest.as_str(),
            ),
            (
                "upstream_verification_receipt_digest",
                self.upstream_verification_receipt_digest.as_str(),
            ),
            (
                "protected_time_evidence_digest",
                self.protected_time_evidence_digest.as_str(),
            ),
            (
                "ingress_context_digest",
                self.ingress_context_digest.as_str(),
            ),
            ("constitution_digest", self.constitution_digest.as_str()),
            (
                "policy_registry_digest",
                self.policy_registry_digest.as_str(),
            ),
            (
                "authorization_reservation_id",
                self.authorization_reservation_id.as_str(),
            ),
        ] {
            require_digest(field, digest)?;
        }
        for (field, digest) in [
            (
                "authority_decision_digest",
                self.authority_decision_digest.as_deref(),
            ),
            (
                "identity_role_binding_digest",
                self.identity_role_binding_digest.as_deref(),
            ),
        ] {
            require_optional_digest(field, digest)?;
        }
        if self.brain_id != brain_id {
            return Err(MissionServiceError::refused(
                "brain_mismatch",
                format!("expected '{brain_id}', observed '{}'", self.brain_id),
            ));
        }
        if self.verified_object_digest != object_digest {
            return Err(MissionServiceError::refused(
                "upstream_verification_binding_mismatch",
                "the authenticated context does not bind the submitted object digest",
            ));
        }
        if self.authenticated_at > now_ms || now_ms >= self.expires_at {
            return Err(MissionServiceError::refused(
                "authenticated_context_expired",
                format!(
                    "authenticated_at={}, expires_at={}, now={now_ms}",
                    self.authenticated_at, self.expires_at
                ),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalBlockBindingV1 {
    pub block_id: String,
    pub store_version: u64,
    pub boundary_version: u32,
    pub contract_version: u32,
    pub resolution_hash: String,
}

impl CanonicalBlockBindingV1 {
    fn validate(&self) -> MissionServiceResult<()> {
        require_non_empty("block_id", &self.block_id)?;
        if self.store_version == 0 || self.boundary_version == 0 || self.contract_version == 0 {
            return Err(MissionServiceError::refused(
                "invalid_canonical_block_binding",
                "store, boundary and contract versions must all be at least one",
            ));
        }
        require_digest("resolution_hash", &self.resolution_hash)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalEvidenceAnchorV1 {
    pub locator: String,
    pub sha256: String,
    pub producer_id: String,
}

impl CanonicalEvidenceAnchorV1 {
    fn validate(&self) -> MissionServiceResult<()> {
        require_non_empty("evidence.locator", &self.locator)?;
        require_non_empty("evidence.producer_id", &self.producer_id)?;
        require_digest("evidence.sha256", &self.sha256)?;
        if looks_absolute(&self.locator) {
            return Err(MissionServiceError::refused(
                "absolute_evidence_locator",
                "public evidence locators must be repo-relative or URI-shaped",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionServiceConfigV1 {
    pub schema: String,
    pub organism_id: String,
    pub brain_id: String,
    pub mission_service_actor_id: String,
    pub canonical_blocks: Vec<CanonicalBlockBindingV1>,
    pub canonical_evidence: Vec<CanonicalEvidenceAnchorV1>,
}

impl MissionServiceConfigV1 {
    fn validate(&self) -> MissionServiceResult<()> {
        require_schema(
            "MissionService config",
            &self.schema,
            MISSION_SERVICE_CONFIG_SCHEMA,
        )?;
        require_non_empty("organism_id", &self.organism_id)?;
        require_non_empty("brain_id", &self.brain_id)?;
        require_non_empty("mission_service_actor_id", &self.mission_service_actor_id)?;
        if self.canonical_blocks.is_empty() {
            return Err(MissionServiceError::refused(
                "empty_canonical_block_catalog",
                "MissionService cannot validate block/scope bindings without a canonical block",
            ));
        }
        let mut ids = BTreeSet::new();
        let mut store_versions = BTreeSet::new();
        for block in &self.canonical_blocks {
            block.validate()?;
            if !ids.insert(block.block_id.clone()) {
                return Err(MissionServiceError::refused(
                    "duplicate_canonical_block",
                    &block.block_id,
                ));
            }
            store_versions.insert(block.store_version);
        }
        if store_versions.len() != 1 {
            return Err(MissionServiceError::refused(
                "incoherent_store_version",
                "all canonical block bindings must project the same global store_version",
            ));
        }
        let mut evidence = BTreeSet::new();
        for anchor in &self.canonical_evidence {
            anchor.validate()?;
            if !evidence.insert((anchor.locator.clone(), anchor.sha256.clone())) {
                return Err(MissionServiceError::refused(
                    "duplicate_evidence_anchor",
                    &anchor.locator,
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRefV1 {
    pub schema: String,
    pub kind: String,
    pub locator: String,
    pub sha256: String,
    pub producer_id: String,
    pub command: Option<Vec<String>>,
    pub started_at: Option<u64>,
    pub ended_at: Option<u64>,
    pub retention_status: String,
    pub evidence_digest: String,
}

impl EvidenceRefV1 {
    pub fn compute_evidence_digest(&self) -> Result<String, CanonicalError> {
        digest_without_fields(EVIDENCE_REF_DIGEST_DOMAIN, self, &["evidence_digest"])
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.evidence_digest = self.compute_evidence_digest()?;
        Ok(())
    }

    fn validate(&self) -> MissionServiceResult<()> {
        require_schema("evidence ref", &self.schema, EVIDENCE_REF_SCHEMA)?;
        for (field, value) in [
            ("evidence.kind", self.kind.as_str()),
            ("evidence.locator", self.locator.as_str()),
            ("evidence.producer_id", self.producer_id.as_str()),
            ("evidence.retention_status", self.retention_status.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        if looks_absolute(&self.locator) {
            return Err(MissionServiceError::refused(
                "absolute_evidence_locator",
                &self.locator,
            ));
        }
        require_digest("evidence.sha256", &self.sha256)?;
        require_digest("evidence.evidence_digest", &self.evidence_digest)?;
        if let Some(command) = &self.command {
            if command.first().is_none_or(|part| part.trim().is_empty()) {
                return Err(MissionServiceError::refused(
                    "empty_evidence_command",
                    "command must contain a non-empty executable",
                ));
            }
        }
        match (self.started_at, self.ended_at) {
            (Some(start), Some(end)) if start <= end => {}
            (None, None) => {}
            _ => {
                return Err(MissionServiceError::refused(
                    "invalid_evidence_window",
                    "started_at and ended_at must be both absent or ordered",
                ))
            }
        }
        let expected = self.compute_evidence_digest()?;
        if expected != self.evidence_digest {
            return Err(MissionServiceError::refused(
                "evidence_digest_mismatch",
                format!("expected {expected}, observed {}", self.evidence_digest),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptCandidateV1 {
    pub schema: String,
    pub candidate_id: String,
    pub brain_id: String,
    pub mission_id: String,
    pub mission_head_id: String,
    pub iteration_id: u64,
    pub block_id: String,
    pub store_version: u64,
    pub boundary_version: u32,
    pub contract_version: u32,
    pub execution_result_digest: String,
    pub receipt_type: ReceiptType,
    pub evidence_refs: Vec<EvidenceRefV1>,
    pub synthetic: bool,
    pub issuer: String,
    pub key_id: String,
    pub algorithm: String,
    pub candidate_digest: String,
    pub signature: OpaqueSignature,
}

impl ReceiptCandidateV1 {
    pub fn compute_candidate_digest(&self) -> Result<String, CanonicalError> {
        digest_without_fields(
            RECEIPT_CANDIDATE_DIGEST_DOMAIN,
            self,
            &["candidate_id", "candidate_digest", "signature"],
        )
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.candidate_digest = self.compute_candidate_digest()?;
        self.candidate_id = format!("cand:{}", self.candidate_digest);
        Ok(())
    }

    fn validate_structural(&self) -> MissionServiceResult<()> {
        require_schema("receipt candidate", &self.schema, RECEIPT_CANDIDATE_SCHEMA)?;
        for (field, value) in [
            ("candidate_id", self.candidate_id.as_str()),
            ("candidate.brain_id", self.brain_id.as_str()),
            ("candidate.mission_id", self.mission_id.as_str()),
            ("candidate.mission_head_id", self.mission_head_id.as_str()),
            ("candidate.block_id", self.block_id.as_str()),
            ("candidate.issuer", self.issuer.as_str()),
            ("candidate.key_id", self.key_id.as_str()),
            ("candidate.algorithm", self.algorithm.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        if self.iteration_id == 0
            || self.store_version == 0
            || self.boundary_version == 0
            || self.contract_version == 0
        {
            return Err(MissionServiceError::refused(
                "invalid_candidate_scope",
                "iteration/store/boundary/contract versions must be at least one",
            ));
        }
        require_digest(
            "candidate.execution_result_digest",
            &self.execution_result_digest,
        )?;
        require_digest("candidate.candidate_digest", &self.candidate_digest)?;
        if self.signature.is_empty() {
            return Err(MissionServiceError::refused(
                "empty_candidate_signature",
                "candidate signature is structurally absent",
            ));
        }
        if self.evidence_refs.is_empty() {
            return Err(MissionServiceError::refused(
                "empty_candidate_evidence",
                "receipt candidate must contain at least one evidence ref",
            ));
        }
        for evidence in &self.evidence_refs {
            evidence.validate()?;
        }
        let expected = self.compute_candidate_digest()?;
        if expected != self.candidate_digest || self.candidate_id != format!("cand:{expected}") {
            return Err(MissionServiceError::refused(
                "candidate_digest_mismatch",
                "candidate id/digest does not match canonical candidate bytes",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptScopeV1 {
    pub block_id: String,
    pub store_version: u64,
    pub boundary_version: u32,
    pub contract_version: u32,
    pub resolution_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptValidityV1 {
    pub valid: bool,
    pub expires_at: Option<u64>,
    pub stales_on: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptImportAuditV1 {
    pub imported_by: String,
    pub imported_at: u64,
    pub expected_store_version: u64,
    pub resulting_store_version: u64,
    pub authority_snapshot_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptV1 {
    pub schema: String,
    pub receipt_id: String,
    pub receipt_digest: String,
    pub transaction_id: String,
    pub brain_id: String,
    pub mission_id: String,
    pub mission_head_id: String,
    pub iteration_id: u64,
    pub candidate_digest: String,
    pub receipt_type: ReceiptType,
    pub scope: ReceiptScopeV1,
    pub evidence_refs: Vec<EvidenceRefV1>,
    pub validity: ReceiptValidityV1,
    pub emitter: String,
    pub import_audit: ReceiptImportAuditV1,
    pub issuer: String,
    pub key_id: String,
    pub algorithm: String,
    pub signature: OpaqueSignature,
}

impl ReceiptV1 {
    pub fn compute_receipt_digest(&self) -> Result<String, CanonicalError> {
        digest_without_fields(
            RECEIPT_DIGEST_DOMAIN,
            self,
            &["receipt_id", "receipt_digest", "signature"],
        )
    }

    fn seal(&mut self) -> Result<(), CanonicalError> {
        self.receipt_digest = self.compute_receipt_digest()?;
        self.receipt_id = format!("rcp:{}", self.receipt_digest);
        Ok(())
    }

    fn validate(&self) -> MissionServiceResult<()> {
        require_schema("receipt", &self.schema, RECEIPT_SCHEMA)?;
        for (field, value) in [
            ("receipt_id", self.receipt_id.as_str()),
            ("receipt.transaction_id", self.transaction_id.as_str()),
            ("receipt.brain_id", self.brain_id.as_str()),
            ("receipt.mission_id", self.mission_id.as_str()),
            ("receipt.mission_head_id", self.mission_head_id.as_str()),
            ("receipt.scope.block_id", self.scope.block_id.as_str()),
            ("receipt.emitter", self.emitter.as_str()),
            ("receipt.issuer", self.issuer.as_str()),
            ("receipt.key_id", self.key_id.as_str()),
            ("receipt.algorithm", self.algorithm.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        for (field, digest) in [
            ("receipt_digest", self.receipt_digest.as_str()),
            ("candidate_digest", self.candidate_digest.as_str()),
            ("resolution_hash", self.scope.resolution_hash.as_str()),
            (
                "authority_snapshot_digest",
                self.import_audit.authority_snapshot_digest.as_str(),
            ),
        ] {
            require_digest(field, digest)?;
        }
        if self.iteration_id == 0
            || self.scope.store_version == 0
            || self.scope.boundary_version == 0
            || self.scope.contract_version == 0
            || self.import_audit.resulting_store_version
                != self.import_audit.expected_store_version.saturating_add(1)
            || self.import_audit.expected_store_version != self.scope.store_version
        {
            return Err(MissionServiceError::refused(
                "invalid_receipt_scope",
                "receipt scope/import versions are incoherent",
            ));
        }
        if !self.validity.valid || self.evidence_refs.is_empty() || self.signature.is_empty() {
            return Err(MissionServiceError::refused(
                "invalid_receipt",
                "receipt must be valid, signed, and evidence-backed",
            ));
        }
        for evidence in &self.evidence_refs {
            evidence.validate()?;
        }
        let expected = self.compute_receipt_digest()?;
        if self.receipt_digest != expected || self.receipt_id != format!("rcp:{expected}") {
            return Err(MissionServiceError::refused(
                "receipt_digest_mismatch",
                "receipt id/digest does not match canonical receipt core bytes",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionServiceDecisionV1 {
    pub schema: String,
    pub decision_id: String,
    pub issuer: String,
    pub reason_digest: String,
    pub decision_digest: String,
}

impl MissionServiceDecisionV1 {
    pub fn compute_decision_digest(&self) -> Result<String, CanonicalError> {
        digest_without_fields(
            MISSION_SERVICE_DECISION_DIGEST_DOMAIN,
            self,
            &["decision_digest"],
        )
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.decision_digest = self.compute_decision_digest()?;
        Ok(())
    }

    fn validate(&self, expected_issuer: &str) -> MissionServiceResult<()> {
        require_schema(
            "mission service decision",
            &self.schema,
            MISSION_SERVICE_DECISION_SCHEMA,
        )?;
        require_non_empty("decision_id", &self.decision_id)?;
        require_non_empty("decision.issuer", &self.issuer)?;
        require_digest("decision.reason_digest", &self.reason_digest)?;
        require_digest("decision.decision_digest", &self.decision_digest)?;
        if self.issuer != expected_issuer {
            return Err(MissionServiceError::refused(
                "wrong_author",
                format!(
                    "MissionService decision issuer must be '{expected_issuer}', observed '{}'",
                    self.issuer
                ),
            ));
        }
        if self.compute_decision_digest()? != self.decision_digest {
            return Err(MissionServiceError::refused(
                "decision_digest_mismatch",
                "MissionService decision bytes changed after sealing",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorProposalV1 {
    pub schema: String,
    pub proposal_id: String,
    pub author_id: String,
    pub proposal_digest: String,
}

impl AuthorProposalV1 {
    pub fn compute_proposal_digest(&self) -> Result<String, CanonicalError> {
        digest_without_fields(AUTHOR_PROPOSAL_DIGEST_DOMAIN, self, &["proposal_digest"])
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.proposal_digest = self.compute_proposal_digest()?;
        Ok(())
    }

    fn validate(&self, expected_author: &str) -> MissionServiceResult<()> {
        require_schema("author proposal", &self.schema, AUTHOR_PROPOSAL_SCHEMA)?;
        require_non_empty("proposal_id", &self.proposal_id)?;
        require_non_empty("proposal.author_id", &self.author_id)?;
        require_digest("proposal.proposal_digest", &self.proposal_digest)?;
        if self.author_id != expected_author {
            return Err(MissionServiceError::refused(
                "wrong_author",
                format!(
                    "proposal author must be '{expected_author}', observed '{}'",
                    self.author_id
                ),
            ));
        }
        if self.compute_proposal_digest()? != self.proposal_digest {
            return Err(MissionServiceError::refused(
                "proposal_digest_mismatch",
                "author proposal bytes changed after sealing",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "evidence_kind",
    content = "evidence",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum MissionTransitionEvidenceV1 {
    MissionServiceDecision {
        decision: MissionServiceDecisionV1,
        dispatch: Option<ExecutionDispatchV1>,
    },
    AuthorProposal {
        proposal: AuthorProposalV1,
        dispatch: Option<ExecutionDispatchV1>,
    },
    ReviewResult {
        result: ReviewResultV1,
        dispatch: Option<ExecutionDispatchV1>,
    },
    ExecutionDispatchAck {
        ack: ExecutionDispatchAckV1,
    },
    ExecutionResult {
        result: ExecutionResultV1,
        candidate: Option<ReceiptCandidateV1>,
    },
}

impl MissionTransitionEvidenceV1 {
    const fn source(&self) -> MissionTransitionSource {
        match self {
            Self::MissionServiceDecision { .. } => MissionTransitionSource::MissionServiceDecision,
            Self::AuthorProposal { .. } => MissionTransitionSource::AuthorProposal,
            Self::ReviewResult { .. } => MissionTransitionSource::ReviewResult,
            Self::ExecutionDispatchAck { .. } => MissionTransitionSource::ExecutionDispatchAck,
            Self::ExecutionResult { .. } => MissionTransitionSource::ExecutionResult,
        }
    }

    fn source_digest(&self) -> &str {
        match self {
            Self::MissionServiceDecision { decision, .. } => &decision.decision_digest,
            Self::AuthorProposal { proposal, .. } => &proposal.proposal_digest,
            Self::ReviewResult { result, .. } => &result.result_digest,
            Self::ExecutionDispatchAck { ack } => &ack.ack_digest,
            Self::ExecutionResult { result, .. } => &result.result_digest,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionTransitionPayloadV1 {
    pub schema: String,
    pub brain_id: String,
    pub mission_id: String,
    pub block_id: String,
    pub expected_store_version: u64,
    pub expected_boundary_version: u32,
    pub expected_contract_version: u32,
    pub evidence: MissionTransitionEvidenceV1,
}

impl MissionTransitionPayloadV1 {
    fn validate_shape(&self) -> MissionServiceResult<()> {
        require_schema(
            "mission transition payload",
            &self.schema,
            MISSION_TRANSITION_PAYLOAD_SCHEMA,
        )?;
        require_non_empty("payload.brain_id", &self.brain_id)?;
        require_non_empty("payload.mission_id", &self.mission_id)?;
        require_non_empty("payload.block_id", &self.block_id)?;
        if self.expected_store_version == 0
            || self.expected_boundary_version == 0
            || self.expected_contract_version == 0
        {
            return Err(MissionServiceError::refused(
                "invalid_payload_scope",
                "store, boundary and contract versions must be at least one",
            ));
        }
        require_digest("payload.source_digest", self.evidence.source_digest())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionLetterV1 {
    pub schema: String,
    pub head_id: String,
    pub brain_id: String,
    pub mission_id: String,
    pub mission_seq: u64,
    pub previous_head_id: Option<String>,
    pub state: MissionState,
    pub iteration_id: u64,
    pub packet_digest: String,
    pub block_id: String,
    pub store_version: u64,
    pub boundary_version: u32,
    pub contract_version: u32,
    pub source: MissionTransitionSource,
    pub source_digest: String,
    pub authored_by: String,
    pub transaction_id: Option<String>,
    pub execution_dispatch: Option<ExecutionDispatchV1>,
    pub execution_result_digest: Option<String>,
    pub review_result_digest: Option<String>,
    pub receipt_candidate: Option<ReceiptCandidateV1>,
    pub committed_receipt_id: Option<String>,
    pub created_at: u64,
}

impl MissionLetterV1 {
    pub fn compute_head_digest(&self) -> Result<String, CanonicalError> {
        digest_without_fields(MISSION_LETTER_DIGEST_DOMAIN, self, &["head_id"])
    }

    fn seal(&mut self) -> Result<(), CanonicalError> {
        self.head_id = format!("mlt:{}", self.compute_head_digest()?);
        Ok(())
    }

    fn validate_structural(&self, mission_service_actor_id: &str) -> MissionServiceResult<()> {
        require_schema("mission letter", &self.schema, MISSION_LETTER_V1_SCHEMA)?;
        for (field, value) in [
            ("letter.head_id", self.head_id.as_str()),
            ("letter.brain_id", self.brain_id.as_str()),
            ("letter.mission_id", self.mission_id.as_str()),
            ("letter.block_id", self.block_id.as_str()),
            ("letter.authored_by", self.authored_by.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        if self.mission_seq == 0
            || self.iteration_id == 0
            || self.store_version == 0
            || self.boundary_version == 0
            || self.contract_version == 0
        {
            return Err(MissionServiceError::refused(
                "invalid_mission_letter_versions",
                "mission seq, iteration and scope versions must be at least one",
            ));
        }
        require_optional_non_empty("letter.previous_head_id", self.previous_head_id.as_deref())?;
        require_optional_non_empty("letter.transaction_id", self.transaction_id.as_deref())?;
        require_optional_non_empty(
            "letter.committed_receipt_id",
            self.committed_receipt_id.as_deref(),
        )?;
        require_digest("letter.packet_digest", &self.packet_digest)?;
        require_digest("letter.source_digest", &self.source_digest)?;
        require_optional_digest(
            "letter.execution_result_digest",
            self.execution_result_digest.as_deref(),
        )?;
        require_optional_digest(
            "letter.review_result_digest",
            self.review_result_digest.as_deref(),
        )?;
        if self.authored_by != mission_service_actor_id {
            return Err(MissionServiceError::refused(
                "wrong_mission_letter_author",
                format!(
                    "only MissionService actor '{mission_service_actor_id}' may author letters"
                ),
            ));
        }
        if let Some(candidate) = &self.receipt_candidate {
            candidate.validate_structural()?;
        }
        let expected = format!("mlt:{}", self.compute_head_digest()?);
        if self.head_id != expected {
            return Err(MissionServiceError::refused(
                "mission_letter_digest_mismatch",
                format!("expected {expected}, observed {}", self.head_id),
            ));
        }
        match self.state {
            MissionState::Landed
                if self.transaction_id.is_none() || self.committed_receipt_id.is_none() =>
            {
                return Err(MissionServiceError::refused(
                    "landed_without_transactional_receipt",
                    "landed letter requires transaction_id and committed_receipt_id",
                ));
            }
            MissionState::Landed => {}
            _ if self.committed_receipt_id.is_some() => {
                return Err(MissionServiceError::refused(
                    "receipt_on_non_landed_letter",
                    "only a landed letter may carry committed_receipt_id",
                ))
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransitionReplayV1 {
    intent_digest: String,
    head_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LandReplayV1 {
    intent_digest: String,
    transaction_digest: String,
    brain_id: String,
    mission_id: String,
    expected_head_id: String,
    candidate_id: String,
    expected_candidate_digest: String,
    expected_store_version: u64,
    outcome: LandOutcomeV1,
}

impl LandReplayV1 {
    fn matches_request(&self, request: &LandRequestV1, transaction_digest: &str) -> bool {
        self.transaction_digest == transaction_digest
            && self.brain_id == request.brain_id
            && self.mission_id == request.mission_id
            && self.expected_head_id == request.expected_head_id
            && self.candidate_id == request.candidate_id
            && self.expected_candidate_digest == request.expected_candidate_digest
            && self.expected_store_version == request.expected_store_version
            && self.outcome.idempotency_key == request.idempotency_key
            && self.outcome.transaction_id == request.transaction.binding().transaction_id
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MissionServiceStateV1 {
    schema: String,
    organism_id: String,
    brain_id: String,
    mission_service_actor_id: String,
    state_version: u64,
    canonical_blocks: Vec<CanonicalBlockBindingV1>,
    canonical_evidence: Vec<CanonicalEvidenceAnchorV1>,
    letters: Vec<MissionLetterV1>,
    receipts: Vec<ReceiptV1>,
    transition_replays: BTreeMap<String, TransitionReplayV1>,
    land_replays: BTreeMap<String, LandReplayV1>,
}

impl MissionServiceStateV1 {
    fn from_config(config: &MissionServiceConfigV1) -> Self {
        Self {
            schema: MISSION_SERVICE_STATE_SCHEMA.to_string(),
            organism_id: config.organism_id.clone(),
            brain_id: config.brain_id.clone(),
            mission_service_actor_id: config.mission_service_actor_id.clone(),
            state_version: 1,
            canonical_blocks: config.canonical_blocks.clone(),
            canonical_evidence: config.canonical_evidence.clone(),
            letters: Vec::new(),
            receipts: Vec::new(),
            transition_replays: BTreeMap::new(),
            land_replays: BTreeMap::new(),
        }
    }

    fn head(&self, mission_id: &str) -> Option<&MissionLetterV1> {
        self.letters
            .iter()
            .rev()
            .find(|letter| letter.mission_id == mission_id)
    }

    fn block(&self, block_id: &str) -> Option<&CanonicalBlockBindingV1> {
        self.canonical_blocks
            .iter()
            .find(|block| block.block_id == block_id)
    }

    fn validate(&self) -> MissionServiceResult<()> {
        require_schema(
            "MissionService state",
            &self.schema,
            MISSION_SERVICE_STATE_SCHEMA,
        )?;
        require_non_empty("state.organism_id", &self.organism_id)?;
        require_non_empty("state.brain_id", &self.brain_id)?;
        require_non_empty(
            "state.mission_service_actor_id",
            &self.mission_service_actor_id,
        )?;
        if self.state_version == 0 {
            return Err(MissionServiceError::Corruption {
                detail: "state_version is zero".to_string(),
            });
        }
        let config = MissionServiceConfigV1 {
            schema: MISSION_SERVICE_CONFIG_SCHEMA.to_string(),
            organism_id: self.organism_id.clone(),
            brain_id: self.brain_id.clone(),
            mission_service_actor_id: self.mission_service_actor_id.clone(),
            canonical_blocks: self.canonical_blocks.clone(),
            canonical_evidence: self.canonical_evidence.clone(),
        };
        config.validate()?;

        let mut heads: BTreeMap<&str, &MissionLetterV1> = BTreeMap::new();
        let mut head_ids = BTreeSet::new();
        for letter in &self.letters {
            letter.validate_structural(&self.mission_service_actor_id)?;
            if letter.brain_id != self.brain_id {
                return Err(MissionServiceError::Corruption {
                    detail: format!("letter {} binds another brain", letter.head_id),
                });
            }
            if !head_ids.insert(letter.head_id.as_str()) {
                return Err(MissionServiceError::Corruption {
                    detail: format!("duplicate mission head id {}", letter.head_id),
                });
            }
            let previous = heads.get(letter.mission_id.as_str()).copied();
            let rule = m1nd_control::mission_transition_rule(
                previous.map(|head| head.state),
                letter.state,
            )
            .ok_or_else(|| MissionServiceError::Corruption {
                detail: format!(
                    "persisted illegal transition {:?} -> {:?}",
                    previous.map(|head| head.state),
                    letter.state
                ),
            })?;
            if letter.source != rule.source {
                return Err(MissionServiceError::Corruption {
                    detail: format!("letter {} has the wrong transition source", letter.head_id),
                });
            }
            match previous {
                None => {
                    if letter.mission_seq != 1
                        || letter.previous_head_id.is_some()
                        || letter.iteration_id != 1
                    {
                        return Err(MissionServiceError::Corruption {
                            detail: format!("invalid genesis letter {}", letter.head_id),
                        });
                    }
                }
                Some(head) => {
                    if head.state.is_terminal() {
                        return Err(MissionServiceError::Corruption {
                            detail: format!("terminal mission {} was extended", head.mission_id),
                        });
                    }
                    if letter.mission_seq != head.mission_seq + 1
                        || letter.previous_head_id.as_deref() != Some(head.head_id.as_str())
                        || letter.block_id != head.block_id
                    {
                        return Err(MissionServiceError::Corruption {
                            detail: format!("mission chain fork at {}", letter.head_id),
                        });
                    }
                    let expected_iteration = match rule.iteration {
                        m1nd_control::IterationRule::Initialize => 1,
                        m1nd_control::IterationRule::Preserve => head.iteration_id,
                        m1nd_control::IterationRule::Advance => head
                            .iteration_id
                            .checked_add(1)
                            .ok_or_else(|| MissionServiceError::Corruption {
                                detail: "mission iteration overflow".to_string(),
                            })?,
                    };
                    if letter.iteration_id != expected_iteration {
                        return Err(MissionServiceError::Corruption {
                            detail: format!("wrong iteration at {}", letter.head_id),
                        });
                    }
                    match rule.iteration {
                        m1nd_control::IterationRule::Advance
                            if letter.packet_digest == head.packet_digest =>
                        {
                            return Err(MissionServiceError::Corruption {
                                detail: "advancing iteration reused packet digest".to_string(),
                            })
                        }
                        m1nd_control::IterationRule::Preserve
                            if letter.packet_digest != head.packet_digest =>
                        {
                            return Err(MissionServiceError::Corruption {
                                detail: "preserving transition changed packet digest".to_string(),
                            })
                        }
                        _ => {}
                    }
                }
            }
            heads.insert(letter.mission_id.as_str(), letter);
        }

        let mut receipt_ids = BTreeSet::new();
        let mut receipt_transactions = BTreeSet::new();
        for receipt in &self.receipts {
            receipt.validate()?;
            if receipt.brain_id != self.brain_id
                || !receipt_ids.insert(receipt.receipt_id.as_str())
                || !receipt_transactions.insert(receipt.transaction_id.as_str())
            {
                return Err(MissionServiceError::Corruption {
                    detail: format!("duplicate or cross-brain receipt {}", receipt.receipt_id),
                });
            }
            let landed = self.letters.iter().any(|letter| {
                letter.state == MissionState::Landed
                    && letter.transaction_id.as_deref() == Some(receipt.transaction_id.as_str())
                    && letter.committed_receipt_id.as_deref() == Some(receipt.receipt_id.as_str())
            });
            if !landed {
                return Err(MissionServiceError::Corruption {
                    detail: format!(
                        "receipt {} is visible without its landed letter",
                        receipt.receipt_id
                    ),
                });
            }
        }
        for letter in self
            .letters
            .iter()
            .filter(|letter| letter.state == MissionState::Landed)
        {
            if !self.receipts.iter().any(|receipt| {
                Some(receipt.receipt_id.as_str()) == letter.committed_receipt_id.as_deref()
                    && Some(receipt.transaction_id.as_str()) == letter.transaction_id.as_deref()
            }) {
                return Err(MissionServiceError::Corruption {
                    detail: format!(
                        "landed letter {} is visible without its receipt",
                        letter.head_id
                    ),
                });
            }
        }
        for (scope, replay) in &self.transition_replays {
            require_digest("transition_replay.scope", scope)?;
            require_digest("transition_replay.intent_digest", &replay.intent_digest)?;
            if !self
                .letters
                .iter()
                .any(|letter| letter.head_id == replay.head_id)
            {
                return Err(MissionServiceError::Corruption {
                    detail: format!(
                        "transition replay for scope {scope} points at missing head {}",
                        replay.head_id
                    ),
                });
            }
        }
        for (scope, replay) in &self.land_replays {
            require_digest("land_replay.scope", scope)?;
            require_digest("land_replay.intent_digest", &replay.intent_digest)?;
            require_digest("land_replay.transaction_digest", &replay.transaction_digest)?;
            require_digest(
                "land_replay.expected_candidate_digest",
                &replay.expected_candidate_digest,
            )?;
            for (field, value) in [
                ("land_replay.brain_id", replay.brain_id.as_str()),
                ("land_replay.mission_id", replay.mission_id.as_str()),
                (
                    "land_replay.expected_head_id",
                    replay.expected_head_id.as_str(),
                ),
                ("land_replay.candidate_id", replay.candidate_id.as_str()),
            ] {
                require_non_empty(field, value)?;
            }
            replay.outcome.validate()?;
            if replay.expected_store_version == 0
                || replay.brain_id != self.brain_id
                || !self.receipts.iter().any(|receipt| {
                    receipt.receipt_id == replay.outcome.receipt_id
                        && receipt.transaction_id == replay.outcome.transaction_id
                })
            {
                return Err(MissionServiceError::Corruption {
                    detail: format!("invalid land replay for scope {scope}"),
                });
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransitionOutcomeV1 {
    pub letter: MissionLetterV1,
    pub deduplicated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandIntentCoreV1 {
    pub schema: String,
    pub brain_id: String,
    pub mission_id: String,
    pub expected_head_id: String,
    pub candidate_id: String,
    pub expected_candidate_digest: String,
    pub block_id: String,
    pub expected_store_version: u64,
    pub expected_boundary_version: u32,
    pub expected_contract_version: u32,
    pub resolution_hash: String,
    pub idempotency_key: String,
}

impl LandIntentCoreV1 {
    pub fn compute_intent_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(LAND_INTENT_DIGEST_DOMAIN, self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandRequestV1 {
    pub schema: String,
    pub brain_id: String,
    pub mission_id: String,
    pub expected_head_id: String,
    pub candidate_id: String,
    pub expected_candidate_digest: String,
    pub expected_store_version: u64,
    pub idempotency_key: String,
    pub transaction: AuthorityTransactionV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandOutcomeV1 {
    pub schema: String,
    pub transaction_id: String,
    pub idempotency_key: String,
    pub intent_digest: String,
    pub receipt_id: String,
    pub receipt_digest: String,
    pub letter_id: String,
    pub resulting_store_version: u64,
    pub outcome_digest: String,
    pub deduplicated: bool,
}

impl LandOutcomeV1 {
    fn compute_outcome_digest(&self) -> Result<String, CanonicalError> {
        digest_without_fields(
            LAND_OUTCOME_DIGEST_DOMAIN,
            self,
            &["outcome_digest", "deduplicated"],
        )
    }

    fn seal(&mut self) -> Result<(), CanonicalError> {
        self.outcome_digest = self.compute_outcome_digest()?;
        Ok(())
    }

    fn validate(&self) -> MissionServiceResult<()> {
        require_schema("land outcome", &self.schema, LAND_OUTCOME_SCHEMA)?;
        for (field, value) in [
            ("land.transaction_id", self.transaction_id.as_str()),
            ("land.idempotency_key", self.idempotency_key.as_str()),
            ("land.receipt_id", self.receipt_id.as_str()),
            ("land.letter_id", self.letter_id.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        for (field, digest) in [
            ("land.intent_digest", self.intent_digest.as_str()),
            ("land.receipt_digest", self.receipt_digest.as_str()),
            ("land.outcome_digest", self.outcome_digest.as_str()),
        ] {
            require_digest(field, digest)?;
        }
        if self.resulting_store_version == 0
            || self.compute_outcome_digest()? != self.outcome_digest
        {
            return Err(MissionServiceError::refused(
                "land_outcome_digest_mismatch",
                "land outcome does not match canonical outcome bytes",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LandProvisionalBundleV1 {
    receipt: ReceiptV1,
    landed_letter: MissionLetterV1,
    outcome: LandOutcomeV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LandProvisionalPlanV1 {
    schema: String,
    transaction: AuthorityTransactionV1,
    bundle: LandProvisionalBundleV1,
    provisional_effects_digest: String,
    created_at: u64,
}

impl LandProvisionalPlanV1 {
    fn compute_provisional_effects_digest(&self) -> Result<String, CanonicalError> {
        digest_canonical(LAND_PROVISIONAL_DIGEST_DOMAIN, &self.bundle)
    }

    fn validate(&self, mission_service_actor_id: &str) -> MissionServiceResult<()> {
        require_schema(
            "land provisional plan",
            &self.schema,
            LAND_PROVISIONAL_PLAN_SCHEMA,
        )?;
        self.transaction
            .validate()
            .map_err(AuthorityWalError::from)?;
        self.bundle.receipt.validate()?;
        self.bundle
            .landed_letter
            .validate_structural(mission_service_actor_id)?;
        self.bundle.outcome.validate()?;
        require_digest(
            "provisional_effects_digest",
            &self.provisional_effects_digest,
        )?;
        if self.compute_provisional_effects_digest()? != self.provisional_effects_digest {
            return Err(MissionServiceError::Corruption {
                detail: "land provisional bundle digest mismatch".to_string(),
            });
        }
        let binding = self.transaction.binding();
        if self.bundle.receipt.transaction_id != binding.transaction_id
            || self.bundle.landed_letter.transaction_id.as_deref()
                != Some(binding.transaction_id.as_str())
            || self.bundle.outcome.transaction_id != binding.transaction_id
            || self.bundle.receipt.receipt_id != self.bundle.outcome.receipt_id
            || self.bundle.landed_letter.committed_receipt_id.as_deref()
                != Some(self.bundle.receipt.receipt_id.as_str())
        {
            return Err(MissionServiceError::Corruption {
                detail: "land plan transaction/receipt/letter bindings diverge".to_string(),
            });
        }
        Ok(())
    }

    fn matches_request(&self, request: &LandRequestV1) -> bool {
        let candidate = self.bundle.landed_letter.receipt_candidate.as_ref();
        self.transaction == request.transaction
            && self.bundle.receipt.brain_id == request.brain_id
            && self.bundle.receipt.mission_id == request.mission_id
            && self.bundle.receipt.mission_head_id == request.expected_head_id
            && self.bundle.landed_letter.previous_head_id.as_deref()
                == Some(request.expected_head_id.as_str())
            && candidate.is_some_and(|candidate| {
                candidate.candidate_id == request.candidate_id
                    && candidate.candidate_digest == request.expected_candidate_digest
            })
            && self.bundle.receipt.candidate_digest == request.expected_candidate_digest
            && self.bundle.receipt.import_audit.expected_store_version
                == request.expected_store_version
            && self.bundle.outcome.idempotency_key == request.idempotency_key
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyMutationIngress {
    RawMissionPost,
    ReceiptImport,
    RawLanded,
}

impl LegacyMutationIngress {
    pub const fn action_id(self) -> &'static str {
        match self {
            Self::RawMissionPost => "mission_post",
            Self::ReceiptImport => "receipt_import",
            Self::RawLanded => "landed",
        }
    }
}

/// Public ingress guard for legacy direct writes. It always refuses before
/// inspecting capability: no capability can turn a bypass into MissionService.
pub fn refuse_external_legacy_mutation(
    ingress: LegacyMutationIngress,
    _authority: Option<&AuthenticatedAuthorityContextV1>,
) -> MissionServiceResult<()> {
    Err(MissionServiceError::refused(
        "legacy_direct_mutation_refused",
        format!(
            "external '{}' is permanently refused; use MissionService",
            ingress.action_id()
        ),
    ))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LandCrashPoint {
    AfterPrepare,
    AfterProvisional,
    AfterCommit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionLifecycleCrashPoint {
    DispatchLetterPersisted,
    AckJournaled,
    ExecutingLetterPersisted,
    ResultJournaled,
    TerminalLetterPersisted,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionServiceRecoveryReportV1 {
    pub committed_forward_completed: u64,
    pub uncommitted_aborted: u64,
    pub already_terminal: u64,
}

pub struct MissionService {
    root: PathBuf,
    state_path: PathBuf,
    plans_dir: PathBuf,
    state: MissionServiceStateV1,
    wal: AuthorityWal,
    execution_outbox: OwnerExecutionOutbox,
    recovery_report: MissionServiceRecoveryReportV1,
}

impl MissionService {
    pub fn open(
        root: impl AsRef<Path>,
        config: MissionServiceConfigV1,
    ) -> MissionServiceResult<Self> {
        Self::open_internal(root, config, None, None)
    }

    /// Explicit battery-only constructor. Production callers must inject a
    /// protected owner-key adapter with `open_with_wal_record_crypto`; the
    /// ordinary `open` constructor remains fail-closed for WAL mutation.
    pub fn open_software_test_not_production(
        root: impl AsRef<Path>,
        config: MissionServiceConfigV1,
    ) -> MissionServiceResult<Self> {
        Self::open_with_wal_record_crypto(
            root,
            config,
            Arc::new(
                SoftwareTestAuthorityWalRecordCrypto::explicit_not_production(
                    b"m1nd-authority-wal-explicit-test-secret-v1",
                ),
            ),
        )
    }

    pub fn open_with_wal_record_crypto(
        root: impl AsRef<Path>,
        config: MissionServiceConfigV1,
        wal_record_crypto: Arc<dyn AuthorityWalRecordCrypto>,
    ) -> MissionServiceResult<Self> {
        Self::open_internal(root, config, Some(wal_record_crypto), None)
    }

    pub(crate) fn open_with_wal_record_crypto_and_protected_head(
        root: impl AsRef<Path>,
        config: MissionServiceConfigV1,
        wal_record_crypto: Arc<dyn AuthorityWalRecordCrypto>,
        protected_head_backend: SharedProtectedJournalHeadBackendV1,
    ) -> MissionServiceResult<Self> {
        Self::open_internal(
            root,
            config,
            Some(wal_record_crypto),
            Some(protected_head_backend),
        )
    }

    fn open_internal(
        root: impl AsRef<Path>,
        config: MissionServiceConfigV1,
        wal_record_crypto: Option<Arc<dyn AuthorityWalRecordCrypto>>,
        protected_head_backend: Option<SharedProtectedJournalHeadBackendV1>,
    ) -> MissionServiceResult<Self> {
        config.validate()?;
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(&root)?;
        let state_path = root.join(MISSION_SERVICE_STATE_FILE);
        let plans_dir = root.join(MISSION_SERVICE_PLANS_DIR);
        std::fs::create_dir_all(&plans_dir)?;
        let state = if state_path.exists() {
            let state: MissionServiceStateV1 =
                serde_json::from_slice(&std::fs::read(&state_path)?)?;
            if state.organism_id != config.organism_id
                || state.brain_id != config.brain_id
                || state.mission_service_actor_id != config.mission_service_actor_id
            {
                return Err(MissionServiceError::refused(
                    "mission_service_identity_mismatch",
                    "persisted MissionService identity differs from requested config",
                ));
            }
            state
        } else {
            let state = MissionServiceStateV1::from_config(&config);
            persist_state(&state_path, &state)?;
            state
        };
        state.validate()?;
        let wal = match (wal_record_crypto, protected_head_backend) {
            (Some(crypto), Some(protected_head_backend)) => {
                AuthorityWal::open_with_record_crypto_and_protected_head(
                    root.join(MISSION_SERVICE_WAL_FILE),
                    crypto,
                    protected_head_backend,
                )?
            }
            (Some(crypto), None) => {
                AuthorityWal::open_with_record_crypto(root.join(MISSION_SERVICE_WAL_FILE), crypto)?
            }
            (None, None) => AuthorityWal::open(root.join(MISSION_SERVICE_WAL_FILE))?,
            (None, Some(_)) => {
                return Err(MissionServiceError::refused(
                    "authority_wal_crypto_required",
                    "a protected WAL head cannot be installed without explicit record crypto",
                ));
            }
        };
        let execution_outbox =
            OwnerExecutionOutbox::open(root.join(MISSION_SERVICE_EXECUTION_OUTBOX_FILE))?;
        let mut service = Self {
            root,
            state_path,
            plans_dir,
            state,
            wal,
            execution_outbox,
            recovery_report: MissionServiceRecoveryReportV1::default(),
        };
        service.recovery_report = service.recover_plans(crate::util::now_ms())?;
        service.repair_execution_lifecycle_from_letters()?;
        Ok(service)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn recovery_report(&self) -> &MissionServiceRecoveryReportV1 {
        &self.recovery_report
    }

    pub fn head(&self, mission_id: &str) -> Option<&MissionLetterV1> {
        self.state.head(mission_id)
    }

    pub fn receipt(&self, receipt_id: &str) -> Option<&ReceiptV1> {
        self.state
            .receipts
            .iter()
            .find(|receipt| receipt.receipt_id == receipt_id)
    }

    pub fn receipts(&self) -> &[ReceiptV1] {
        &self.state.receipts
    }

    /// Canonical persisted mission-letter chain, exposed read-only for owner
    /// projections such as G5. Consumers cannot append or replace letters.
    pub fn letters(&self) -> &[MissionLetterV1] {
        &self.state.letters
    }

    pub fn state_version(&self) -> u64 {
        self.state.state_version
    }

    pub fn brain_id(&self) -> &str {
        &self.state.brain_id
    }

    pub fn execution_dispatch(&self, execution_id: &str) -> Option<&OwnerDispatchEntryV1> {
        self.execution_outbox.get(execution_id)
    }

    pub fn execution_reconciliation_actions(&self, now_ms: u64) -> Vec<OwnerReconciliationAction> {
        self.execution_outbox.reconcile(now_ms)
    }

    pub fn canonical_land_intent(
        &self,
        mission_id: &str,
        expected_head_id: &str,
        candidate_id: &str,
        expected_candidate_digest: &str,
        expected_store_version: u64,
        idempotency_key: &str,
    ) -> MissionServiceResult<LandIntentCoreV1> {
        let head = self.canonical_merge_wait_head(
            mission_id,
            expected_head_id,
            candidate_id,
            expected_candidate_digest,
            expected_store_version,
        )?;
        let candidate = head
            .receipt_candidate
            .as_ref()
            .expect("canonical_merge_wait_head proved candidate exists");
        let block = self.canonical_block(&head.block_id)?.clone();
        Ok(LandIntentCoreV1 {
            schema: LAND_INTENT_CORE_SCHEMA.to_string(),
            brain_id: self.state.brain_id.clone(),
            mission_id: mission_id.to_string(),
            expected_head_id: expected_head_id.to_string(),
            candidate_id: candidate.candidate_id.clone(),
            expected_candidate_digest: candidate.candidate_digest.clone(),
            block_id: head.block_id.clone(),
            expected_store_version,
            expected_boundary_version: block.boundary_version,
            expected_contract_version: block.contract_version,
            resolution_hash: block.resolution_hash,
            idempotency_key: idempotency_key.to_string(),
        })
    }

    pub(crate) fn transition(
        &mut self,
        authority: &AuthenticatedAuthorityContextV1,
        intent: &MissionTransitionIntentV1,
        payload: &MissionTransitionPayloadV1,
        now_ms: u64,
    ) -> MissionServiceResult<TransitionOutcomeV1> {
        self.transition_internal(authority, intent, payload, now_ms, None)
    }

    #[cfg(test)]
    pub(crate) fn transition_execution_until_crash_for_test(
        &mut self,
        authority: &AuthenticatedAuthorityContextV1,
        intent: &MissionTransitionIntentV1,
        payload: &MissionTransitionPayloadV1,
        now_ms: u64,
        crash_point: ExecutionLifecycleCrashPoint,
    ) -> MissionServiceResult<TransitionOutcomeV1> {
        self.transition_internal(authority, intent, payload, now_ms, Some(crash_point))
    }

    fn transition_internal(
        &mut self,
        authority: &AuthenticatedAuthorityContextV1,
        intent: &MissionTransitionIntentV1,
        payload: &MissionTransitionPayloadV1,
        now_ms: u64,
        crash_point: Option<ExecutionLifecycleCrashPoint>,
    ) -> MissionServiceResult<TransitionOutcomeV1> {
        payload.validate_shape()?;
        authority.validate_for(&self.state.brain_id, &intent.intent_digest, now_ms)?;
        if authority.subject_id != intent.actor_id
            || authority.role != intent.role
            || authority.capability_id != intent.capability_id
        {
            return Err(MissionServiceError::refused(
                "wrong_author",
                "authenticated subject/role/capability does not bind transition intent",
            ));
        }
        if payload.brain_id != self.state.brain_id || payload.brain_id != intent.brain_id {
            return Err(MissionServiceError::refused(
                "brain_mismatch",
                "transition payload/intent does not bind the served brain",
            ));
        }
        if payload.mission_id != intent.mission_id {
            return Err(MissionServiceError::refused(
                "mission_mismatch",
                "transition payload and intent mission ids differ",
            ));
        }
        if payload.evidence.source() != intent.source
            || payload.evidence.source_digest() != intent.source_digest
        {
            return Err(MissionServiceError::refused(
                "transition_source_mismatch",
                "typed evidence source/digest does not bind the transition intent",
            ));
        }
        if MissionTransitionIntentV1::payload_digest_for(payload)? != intent.payload_digest {
            return Err(MissionServiceError::refused(
                "transition_payload_mismatch",
                "submitted payload bytes differ from the authenticated transition intent",
            ));
        }

        let replay_scope = idempotency_scope(
            &self.state.organism_id,
            &self.state.brain_id,
            &intent.actor_id,
            "mission_transition",
            &intent.idempotency_key,
        )?;
        if let Some(replay) = self.state.transition_replays.get(&replay_scope) {
            if replay.intent_digest != intent.intent_digest {
                return Err(MissionServiceError::refused(
                    "idempotency_conflict",
                    "transition idempotency key is already bound to another intent",
                ));
            }
            let letter = self
                .state
                .letters
                .iter()
                .find(|letter| letter.head_id == replay.head_id)
                .ok_or_else(|| MissionServiceError::Corruption {
                    detail: "transition replay points at a missing letter".to_string(),
                })?
                .clone();
            self.prepare_execution_lifecycle(payload, now_ms, None)?;
            self.finalize_execution_lifecycle(&letter, payload, None)?;
            return Ok(TransitionOutcomeV1 {
                letter,
                deduplicated: true,
            });
        }

        let current = self.state.head(&intent.mission_id).cloned();
        let head_snapshot = current.as_ref().map(|head| MissionHeadSnapshot {
            head_id: head.head_id.as_str(),
            state: head.state,
            iteration_id: head.iteration_id,
            packet_digest: head.packet_digest.as_str(),
        });
        intent.validate(
            MissionHeadContext {
                brain_id: &self.state.brain_id,
                mission_id: &intent.mission_id,
                head: head_snapshot,
            },
            payload,
            now_ms,
        )?;

        let block = self.canonical_block(&payload.block_id)?.clone();
        self.validate_scope(
            payload.expected_store_version,
            payload.expected_boundary_version,
            payload.expected_contract_version,
            &block,
        )?;
        if let Some(head) = &current {
            if head.block_id != payload.block_id {
                return Err(MissionServiceError::refused(
                    "block_mismatch",
                    format!(
                        "mission head binds '{}', payload binds '{}'",
                        head.block_id, payload.block_id
                    ),
                ));
            }
            self.validate_scope(
                head.store_version,
                head.boundary_version,
                head.contract_version,
                &block,
            )?;
        }

        let derived = self.validate_transition_evidence(
            authority,
            intent,
            payload,
            current.as_ref(),
            now_ms,
        )?;
        let mut letter = MissionLetterV1 {
            schema: MISSION_LETTER_V1_SCHEMA.to_string(),
            head_id: String::new(),
            brain_id: self.state.brain_id.clone(),
            mission_id: intent.mission_id.clone(),
            mission_seq: current.as_ref().map_or(1, |head| head.mission_seq + 1),
            previous_head_id: current.as_ref().map(|head| head.head_id.clone()),
            state: intent.to_state,
            iteration_id: intent.iteration_id,
            packet_digest: intent.packet_digest.clone(),
            block_id: payload.block_id.clone(),
            store_version: block.store_version,
            boundary_version: block.boundary_version,
            contract_version: block.contract_version,
            source: intent.source,
            source_digest: intent.source_digest.clone(),
            authored_by: self.state.mission_service_actor_id.clone(),
            transaction_id: None,
            execution_dispatch: derived.execution_dispatch,
            execution_result_digest: derived.execution_result_digest,
            review_result_digest: derived.review_result_digest,
            receipt_candidate: derived.receipt_candidate,
            committed_receipt_id: None,
            created_at: now_ms,
        };
        letter.seal()?;
        letter.validate_structural(&self.state.mission_service_actor_id)?;

        let mut next = self.state.clone();
        next.letters.push(letter.clone());
        next.transition_replays.insert(
            replay_scope,
            TransitionReplayV1 {
                intent_digest: intent.intent_digest.clone(),
                head_id: letter.head_id.clone(),
            },
        );
        next.state_version =
            next.state_version
                .checked_add(1)
                .ok_or_else(|| MissionServiceError::Corruption {
                    detail: "state_version overflow".to_string(),
                })?;
        next.validate()?;
        self.prepare_execution_lifecycle(payload, now_ms, crash_point)?;
        persist_state(&self.state_path, &next)?;
        self.state = next;
        self.finalize_execution_lifecycle(&letter, payload, crash_point)?;
        Ok(TransitionOutcomeV1 {
            letter,
            deduplicated: false,
        })
    }

    /// Persist a canonical DISPATCHING letter and its owner INTENT through the
    /// single execution lifecycle. The signed dispatch must already bind the
    /// exact current mission head (or the genesis transition anchor).
    pub(crate) fn create_execution_dispatch(
        &mut self,
        authority: &AuthenticatedAuthorityContextV1,
        intent: &MissionTransitionIntentV1,
        payload: &MissionTransitionPayloadV1,
        now_ms: u64,
    ) -> MissionServiceResult<TransitionOutcomeV1> {
        if intent.to_state != MissionState::Dispatching
            || execution_dispatch_from_payload(payload).is_none()
        {
            return Err(MissionServiceError::refused(
                "execution_dispatch_creation_mismatch",
                "create_execution_dispatch requires a DISPATCHING transition with a signed dispatch",
            ));
        }
        self.transition_internal(authority, intent, payload, now_ms, None)
    }

    /// Reconcile a durable runner STARTED/ACKED snapshot into the legal
    /// DISPATCHING -> EXECUTING edge. A STARTED snapshot must be accompanied by
    /// the exact signed ACK in `payload`; an ACKED snapshot must contain that
    /// same ACK durably on the runner side.
    pub(crate) fn reconcile_runner_started_snapshot(
        &mut self,
        authority: &AuthenticatedAuthorityContextV1,
        snapshot: &RunnerInboxEntryV1,
        intent: &MissionTransitionIntentV1,
        payload: &MissionTransitionPayloadV1,
        now_ms: u64,
    ) -> MissionServiceResult<TransitionOutcomeV1> {
        snapshot.validate_for_service()?;
        if !matches!(
            snapshot.state,
            RunnerInboxState::Started | RunnerInboxState::Acked
        ) {
            return Err(MissionServiceError::refused(
                "runner_snapshot_not_started",
                format!("expected STARTED or ACKED, observed {:?}", snapshot.state),
            ));
        }
        let ack = match &payload.evidence {
            MissionTransitionEvidenceV1::ExecutionDispatchAck { ack } => ack,
            _ => {
                return Err(MissionServiceError::refused(
                    "runner_started_evidence_mismatch",
                    "runner STARTED reconciliation requires execution ACK evidence",
                ))
            }
        };
        if intent.to_state != MissionState::Executing
            || snapshot.dispatch.execution_id != ack.execution_id
        {
            return Err(MissionServiceError::refused(
                "runner_started_evidence_mismatch",
                "runner snapshot, ACK, and EXECUTING transition do not bind one execution",
            ));
        }
        if snapshot.state == RunnerInboxState::Acked && snapshot.ack.as_ref() != Some(ack) {
            return Err(MissionServiceError::refused(
                "runner_ack_snapshot_mismatch",
                "ACKED runner snapshot does not contain the exact submitted ACK",
            ));
        }
        let started_at = snapshot.started_at.ok_or_else(|| {
            MissionServiceError::refused(
                "runner_snapshot_missing_start",
                "runner snapshot has no durable process start",
            )
        })?;
        if ack.accepted_at < started_at {
            return Err(MissionServiceError::refused(
                "runner_ack_predates_start",
                "execution ACK predates the durable runner process start",
            ));
        }
        self.validate_runner_snapshot_against_execution_record(snapshot, false)?;
        if self.transition_replay_scope_exists(intent)? {
            return self.transition_internal(authority, intent, payload, now_ms, None);
        }
        self.validate_runner_snapshot_against_head(snapshot, MissionState::Dispatching)?;
        self.transition_internal(authority, intent, payload, now_ms, None)
    }

    /// Reconcile a durable runner terminal snapshot into the legal
    /// EXECUTING -> GATE/FAILED edge while preserving exact result, candidate,
    /// process-start, dispatch, and mission-head bindings.
    pub(crate) fn reconcile_runner_terminal_snapshot(
        &mut self,
        authority: &AuthenticatedAuthorityContextV1,
        snapshot: &RunnerInboxEntryV1,
        intent: &MissionTransitionIntentV1,
        payload: &MissionTransitionPayloadV1,
        now_ms: u64,
    ) -> MissionServiceResult<TransitionOutcomeV1> {
        snapshot.validate_for_service()?;
        if !snapshot.state.is_terminal() {
            return Err(MissionServiceError::refused(
                "runner_snapshot_not_terminal",
                format!(
                    "expected terminal runner snapshot, observed {:?}",
                    snapshot.state
                ),
            ));
        }
        let result = match &payload.evidence {
            MissionTransitionEvidenceV1::ExecutionResult { result, .. } => result,
            _ => {
                return Err(MissionServiceError::refused(
                    "runner_terminal_evidence_mismatch",
                    "runner terminal reconciliation requires execution result evidence",
                ))
            }
        };
        if snapshot.result.as_ref() != Some(result)
            || result.expected_transition() != intent.to_state
        {
            return Err(MissionServiceError::refused(
                "runner_terminal_evidence_mismatch",
                "runner terminal snapshot does not contain the exact submitted result/outcome",
            ));
        }
        self.validate_runner_snapshot_against_execution_record(snapshot, true)?;
        if self.transition_replay_scope_exists(intent)? {
            return self.transition_internal(authority, intent, payload, now_ms, None);
        }
        self.validate_runner_snapshot_against_head(snapshot, MissionState::Executing)?;
        let current = self.state.head(&intent.mission_id).ok_or_else(|| {
            MissionServiceError::refused(
                "missing_mission_head",
                "terminal runner snapshot cannot open a mission",
            )
        })?;
        let observed_head = snapshot.executing_head.as_ref().ok_or_else(|| {
            MissionServiceError::refused(
                "runner_snapshot_missing_executing_head",
                "terminal runner snapshot does not bind the EXECUTING mission head",
            )
        })?;
        if observed_head.head_id != current.head_id
            || observed_head.iteration_id != current.iteration_id
            || observed_head.packet_digest != current.packet_digest
        {
            return Err(MissionServiceError::refused(
                "runner_snapshot_head_mismatch",
                "terminal runner snapshot does not bind the exact canonical EXECUTING head",
            ));
        }
        self.transition_internal(authority, intent, payload, now_ms, None)
    }

    fn transition_replay_scope_exists(
        &self,
        intent: &MissionTransitionIntentV1,
    ) -> MissionServiceResult<bool> {
        let replay_scope = idempotency_scope(
            &self.state.organism_id,
            &self.state.brain_id,
            &intent.actor_id,
            "mission_transition",
            &intent.idempotency_key,
        )?;
        Ok(self.state.transition_replays.contains_key(&replay_scope))
    }

    fn validate_runner_snapshot_against_execution_record(
        &self,
        snapshot: &RunnerInboxEntryV1,
        require_executing_head: bool,
    ) -> MissionServiceResult<()> {
        let entry = self
            .execution_outbox
            .get(&snapshot.dispatch.execution_id)
            .ok_or_else(|| {
                MissionServiceError::refused(
                    "unknown_execution_dispatch",
                    "runner snapshot has no canonical owner execution record",
                )
            })?;
        if entry.dispatch != snapshot.dispatch {
            return Err(MissionServiceError::refused(
                "runner_snapshot_dispatch_mismatch",
                "runner snapshot dispatch differs from the canonical owner execution record",
            ));
        }
        if require_executing_head && entry.executing_head != snapshot.executing_head {
            return Err(MissionServiceError::refused(
                "runner_snapshot_head_mismatch",
                "terminal runner snapshot differs from the canonical owner EXECUTING head",
            ));
        }
        Ok(())
    }

    fn validate_runner_snapshot_against_head(
        &self,
        snapshot: &RunnerInboxEntryV1,
        expected_state: MissionState,
    ) -> MissionServiceResult<()> {
        let head = self
            .state
            .head(&snapshot.dispatch.mission_id)
            .ok_or_else(|| {
                MissionServiceError::refused(
                    "missing_mission_head",
                    "runner snapshot cannot open a mission",
                )
            })?;
        let dispatch = head.execution_dispatch.as_ref().ok_or_else(|| {
            MissionServiceError::refused(
                "missing_execution_dispatch",
                "canonical mission head has no execution dispatch",
            )
        })?;
        if head.state != expected_state
            || snapshot.dispatch != *dispatch
            || head.brain_id != snapshot.dispatch.brain_id
            || head.mission_id != snapshot.dispatch.mission_id
            || head.iteration_id != snapshot.dispatch.iteration_id
            || head.packet_digest != snapshot.dispatch.packet_digest
        {
            return Err(MissionServiceError::refused(
                "runner_snapshot_dispatch_mismatch",
                "runner snapshot does not bind the exact canonical mission dispatch/head",
            ));
        }
        Ok(())
    }

    fn prepare_execution_lifecycle(
        &mut self,
        payload: &MissionTransitionPayloadV1,
        now_ms: u64,
        crash_point: Option<ExecutionLifecycleCrashPoint>,
    ) -> MissionServiceResult<()> {
        if let Some(dispatch) = execution_dispatch_from_payload(payload) {
            match self.execution_outbox.preflight_intent(dispatch, now_ms)? {
                OwnerIntentRegistration::Registered | OwnerIntentRegistration::Deduplicated => {}
            }
        }
        match &payload.evidence {
            MissionTransitionEvidenceV1::ExecutionDispatchAck { ack } => {
                let _ = self.execution_outbox.record_ack(ack.clone(), now_ms)?;
                if crash_point == Some(ExecutionLifecycleCrashPoint::AckJournaled) {
                    return Err(MissionServiceError::ExecutionSimulatedCrash {
                        point: ExecutionLifecycleCrashPoint::AckJournaled,
                    });
                }
            }
            MissionTransitionEvidenceV1::ExecutionResult { result, .. } => {
                let _ = self
                    .execution_outbox
                    .record_result(result.clone(), now_ms)?;
                if crash_point == Some(ExecutionLifecycleCrashPoint::ResultJournaled) {
                    return Err(MissionServiceError::ExecutionSimulatedCrash {
                        point: ExecutionLifecycleCrashPoint::ResultJournaled,
                    });
                }
            }
            MissionTransitionEvidenceV1::MissionServiceDecision { .. }
            | MissionTransitionEvidenceV1::AuthorProposal { .. }
            | MissionTransitionEvidenceV1::ReviewResult { .. } => {}
        }
        Ok(())
    }

    fn finalize_execution_lifecycle(
        &mut self,
        letter: &MissionLetterV1,
        payload: &MissionTransitionPayloadV1,
        crash_point: Option<ExecutionLifecycleCrashPoint>,
    ) -> MissionServiceResult<()> {
        if letter.state == MissionState::Dispatching {
            if crash_point == Some(ExecutionLifecycleCrashPoint::DispatchLetterPersisted) {
                return Err(MissionServiceError::ExecutionSimulatedCrash {
                    point: ExecutionLifecycleCrashPoint::DispatchLetterPersisted,
                });
            }
            let dispatch = execution_dispatch_from_payload(payload).ok_or_else(|| {
                MissionServiceError::Corruption {
                    detail: "DISPATCHING letter was built without dispatch evidence".to_string(),
                }
            })?;
            match self
                .execution_outbox
                .register_intent(dispatch.clone(), letter.created_at)?
            {
                OwnerIntentRegistration::Registered | OwnerIntentRegistration::Deduplicated => {}
            }
        }

        match &payload.evidence {
            MissionTransitionEvidenceV1::ExecutionDispatchAck { ack } => {
                if crash_point == Some(ExecutionLifecycleCrashPoint::ExecutingLetterPersisted) {
                    return Err(MissionServiceError::ExecutionSimulatedCrash {
                        point: ExecutionLifecycleCrashPoint::ExecutingLetterPersisted,
                    });
                }
                let head = execution_head_from_letter(letter)?;
                match self.execution_outbox.mark_executing_transition(
                    &ack.execution_id,
                    &ack.ack_digest,
                    head,
                    letter.created_at,
                )? {
                    DispatchMutation::Applied | DispatchMutation::Deduplicated => {}
                }
            }
            MissionTransitionEvidenceV1::ExecutionResult { result, .. } => {
                if crash_point == Some(ExecutionLifecycleCrashPoint::TerminalLetterPersisted) {
                    return Err(MissionServiceError::ExecutionSimulatedCrash {
                        point: ExecutionLifecycleCrashPoint::TerminalLetterPersisted,
                    });
                }
                match self.execution_outbox.mark_result_transition_applied(
                    &result.execution_id,
                    &result.result_digest,
                    letter.head_id.clone(),
                    letter.created_at,
                )? {
                    DispatchMutation::Applied | DispatchMutation::Deduplicated => {}
                }
            }
            MissionTransitionEvidenceV1::MissionServiceDecision { .. }
            | MissionTransitionEvidenceV1::AuthorProposal { .. }
            | MissionTransitionEvidenceV1::ReviewResult { .. } => {}
        }
        Ok(())
    }

    fn repair_execution_lifecycle_from_letters(&mut self) -> MissionServiceResult<()> {
        let mut seen_executions = BTreeSet::new();
        let dispatch_origins: Vec<_> = self
            .state
            .letters
            .iter()
            .filter(|letter| letter.state == MissionState::Dispatching)
            .filter_map(|letter| {
                letter.execution_dispatch.as_ref().and_then(|dispatch| {
                    seen_executions
                        .insert(dispatch.execution_id.clone())
                        .then(|| (dispatch.clone(), letter.created_at))
                })
            })
            .collect();
        for (dispatch, registered_at) in dispatch_origins {
            match self
                .execution_outbox
                .register_intent(dispatch, registered_at)?
            {
                OwnerIntentRegistration::Registered | OwnerIntentRegistration::Deduplicated => {}
            }
        }

        let actions = self.execution_outbox.reconcile(crate::util::now_ms());
        for action in actions {
            match action {
                OwnerReconciliationAction::ApplyExecutingTransition { execution_id, ack } => {
                    let letter = self
                        .state
                        .letters
                        .iter()
                        .find(|letter| {
                            letter.state == MissionState::Executing
                                && letter.source == MissionTransitionSource::ExecutionDispatchAck
                                && letter.source_digest == ack.ack_digest
                                && letter
                                    .execution_dispatch
                                    .as_ref()
                                    .is_some_and(|dispatch| dispatch.execution_id == execution_id)
                        })
                        .cloned();
                    if let Some(letter) = letter {
                        let head = execution_head_from_letter(&letter)?;
                        let _ = self.execution_outbox.mark_executing_transition(
                            &execution_id,
                            &ack.ack_digest,
                            head,
                            letter.created_at,
                        )?;
                    }
                }
                OwnerReconciliationAction::ApplyResultTransition {
                    execution_id,
                    result,
                    target_state,
                } => {
                    let letter =
                        self.state
                            .letters
                            .iter()
                            .find(|letter| {
                                letter.state == target_state
                                    && letter.source == MissionTransitionSource::ExecutionResult
                                    && letter.execution_result_digest.as_deref()
                                        == Some(result.result_digest.as_str())
                                    && letter.execution_dispatch.as_ref().is_some_and(|dispatch| {
                                        dispatch.execution_id == execution_id
                                    })
                            })
                            .cloned();
                    if let Some(letter) = letter {
                        let _ = self.execution_outbox.mark_result_transition_applied(
                            &execution_id,
                            &result.result_digest,
                            letter.head_id,
                            letter.created_at,
                        )?;
                    }
                }
                OwnerReconciliationAction::RedeliverIntent { .. }
                | OwnerReconciliationAction::ExpireIntent { .. }
                | OwnerReconciliationAction::AwaitResult { .. }
                | OwnerReconciliationAction::Settled { .. } => {}
            }
        }
        Ok(())
    }

    pub(crate) fn land(
        &mut self,
        authority: &AuthenticatedAuthorityContextV1,
        request: &LandRequestV1,
        now_ms: u64,
    ) -> MissionServiceResult<LandOutcomeV1> {
        self.land_internal(authority, request, now_ms, None, None)
    }

    pub(crate) fn land_with_commit_coordinator(
        &mut self,
        authority: &AuthenticatedAuthorityContextV1,
        request: &LandRequestV1,
        now_ms: u64,
        coordinator: &dyn AuthorityWalCommitCoordinator,
    ) -> MissionServiceResult<LandOutcomeV1> {
        self.land_internal(authority, request, now_ms, None, Some(coordinator))
    }

    #[cfg(test)]
    pub(crate) fn land_until_crash_for_test(
        &mut self,
        authority: &AuthenticatedAuthorityContextV1,
        request: &LandRequestV1,
        now_ms: u64,
        crash_point: LandCrashPoint,
    ) -> MissionServiceResult<LandOutcomeV1> {
        self.land_internal(authority, request, now_ms, Some(crash_point), None)
    }

    pub(crate) fn reconcile(
        &mut self,
        now_ms: u64,
    ) -> MissionServiceResult<MissionServiceRecoveryReportV1> {
        let report = self.recover_plans(now_ms)?;
        self.repair_execution_lifecycle_from_letters()?;
        self.recovery_report = report.clone();
        Ok(report)
    }

    fn land_internal(
        &mut self,
        authority: &AuthenticatedAuthorityContextV1,
        request: &LandRequestV1,
        now_ms: u64,
        crash_point: Option<LandCrashPoint>,
        commit_coordinator: Option<&dyn AuthorityWalCommitCoordinator>,
    ) -> MissionServiceResult<LandOutcomeV1> {
        require_schema("land request", &request.schema, LAND_REQUEST_SCHEMA)?;
        for (field, value) in [
            ("land.brain_id", request.brain_id.as_str()),
            ("land.mission_id", request.mission_id.as_str()),
            ("land.expected_head_id", request.expected_head_id.as_str()),
            ("land.candidate_id", request.candidate_id.as_str()),
            ("land.idempotency_key", request.idempotency_key.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        require_digest(
            "land.expected_candidate_digest",
            &request.expected_candidate_digest,
        )?;
        let transaction_digest = request.transaction.transaction_digest().to_string();
        authority.validate_for(&self.state.brain_id, &transaction_digest, now_ms)?;
        request
            .transaction
            .validate()
            .map_err(AuthorityWalError::from)?;

        let binding = request.transaction.binding();
        let replay_scope = idempotency_scope(
            &binding.organism_id,
            &binding.brain_id,
            &binding.subject_id,
            &binding.action_id,
            &request.idempotency_key,
        )?;
        if let Some(replay) = self.state.land_replays.get(&replay_scope) {
            self.validate_land_authority_bindings(authority, request, now_ms, false)?;
            if replay.intent_digest != binding.intent_digest
                || !replay.matches_request(request, &transaction_digest)
            {
                return Err(MissionServiceError::refused(
                    "idempotency_conflict",
                    "land idempotency key is already bound to different request/transaction bytes",
                ));
            }
            if self.wal.committed_transaction(&binding.transaction_id) != Some(&request.transaction)
            {
                return Err(MissionServiceError::Corruption {
                    detail: "land replay lacks its exact historical WAL COMMIT".to_string(),
                });
            }
            let mut outcome = replay.outcome.clone();
            outcome.deduplicated = true;
            return Ok(outcome);
        }

        if let Some(terminal) = self.wal.terminal_outcome(&binding.transaction_id).cloned() {
            self.validate_land_authority_bindings(authority, request, now_ms, false)?;
            let plan = self
                .read_plans()?
                .into_iter()
                .find(|plan| plan.transaction.binding().transaction_id == terminal.transaction_id)
                .ok_or_else(|| MissionServiceError::Corruption {
                    detail: "terminal WAL transaction has no durable land plan".to_string(),
                })?;
            plan.validate(&self.state.mission_service_actor_id)?;
            if plan.transaction != request.transaction || !plan.matches_request(request) {
                return Err(MissionServiceError::refused(
                    "idempotency_conflict",
                    "terminal land transaction is bound to different request bytes",
                ));
            }
            if terminal.idempotency_key != binding.idempotency_key
                || terminal.terminal_outcome_digest != plan.bundle.outcome.outcome_digest
            {
                return Err(MissionServiceError::Corruption {
                    detail: "terminal WAL outcome diverges from its durable land plan".to_string(),
                });
            }
            return self.replay_terminal_land(terminal, &replay_scope, &binding.intent_digest);
        }

        let core = self.canonical_land_intent(
            &request.mission_id,
            &request.expected_head_id,
            &request.candidate_id,
            &request.expected_candidate_digest,
            request.expected_store_version,
            &request.idempotency_key,
        )?;
        self.validate_land_authority_bindings(authority, request, now_ms, true)?;
        let intent_digest = core.compute_intent_digest()?;
        let positive = match &request.transaction {
            AuthorityTransactionV1::PositiveAuthority(transaction) => transaction,
            AuthorityTransactionV1::SafetyKernel(_) => {
                return Err(MissionServiceError::refused(
                    "land_requires_positive_authority",
                    "SAFETY_KERNEL transaction cannot perform positive landing",
                ))
            }
        };
        if binding.intent_digest != intent_digest
            || positive.action_payload_digest != intent_digest
            || binding.intent_canonicalization_version != CANONICALIZATION_VERSION
        {
            return Err(MissionServiceError::refused(
                "land_intent_binding_mismatch",
                "transaction does not bind canonical owner-reread LandIntentCoreV1",
            ));
        }

        let head = self
            .canonical_merge_wait_head(
                &request.mission_id,
                &request.expected_head_id,
                &request.candidate_id,
                &request.expected_candidate_digest,
                request.expected_store_version,
            )?
            .clone();
        let candidate = head
            .receipt_candidate
            .as_ref()
            .expect("canonical_merge_wait_head proved candidate exists")
            .clone();
        if candidate.synthetic {
            return Err(MissionServiceError::refused(
                "unlandable_candidate",
                "synthetic candidate can never land",
            ));
        }
        self.validate_candidate_against_catalog(&candidate)?;
        let block = self.canonical_block(&head.block_id)?.clone();

        let mut receipt = ReceiptV1 {
            schema: RECEIPT_SCHEMA.to_string(),
            receipt_id: String::new(),
            receipt_digest: String::new(),
            transaction_id: binding.transaction_id.clone(),
            brain_id: self.state.brain_id.clone(),
            mission_id: head.mission_id.clone(),
            mission_head_id: head.head_id.clone(),
            iteration_id: head.iteration_id,
            candidate_digest: candidate.candidate_digest.clone(),
            receipt_type: candidate.receipt_type,
            scope: ReceiptScopeV1 {
                block_id: block.block_id.clone(),
                store_version: block.store_version,
                boundary_version: block.boundary_version,
                contract_version: block.contract_version,
                resolution_hash: block.resolution_hash.clone(),
            },
            evidence_refs: candidate.evidence_refs.clone(),
            validity: ReceiptValidityV1 {
                valid: true,
                expires_at: None,
                stales_on: vec![
                    "store_version".to_string(),
                    "boundary_version".to_string(),
                    "contract_version".to_string(),
                    "resolution_hash".to_string(),
                ],
            },
            emitter: self.state.mission_service_actor_id.clone(),
            import_audit: ReceiptImportAuditV1 {
                imported_by: authority.subject_id.clone(),
                imported_at: now_ms,
                expected_store_version: block.store_version,
                resulting_store_version: block.store_version.checked_add(1).ok_or_else(|| {
                    MissionServiceError::Corruption {
                        detail: "store_version overflow".to_string(),
                    }
                })?,
                authority_snapshot_digest: authority.authorization_snapshot_digest.clone(),
            },
            issuer: positive.issuer.clone(),
            key_id: positive.key_id.clone(),
            algorithm: positive.algorithm.clone(),
            signature: positive.signature.clone(),
        };
        receipt.seal()?;
        receipt.validate()?;

        let mut landed_letter = MissionLetterV1 {
            schema: MISSION_LETTER_V1_SCHEMA.to_string(),
            head_id: String::new(),
            brain_id: self.state.brain_id.clone(),
            mission_id: head.mission_id.clone(),
            mission_seq: head.mission_seq + 1,
            previous_head_id: Some(head.head_id.clone()),
            state: MissionState::Landed,
            iteration_id: head.iteration_id,
            packet_digest: head.packet_digest.clone(),
            block_id: head.block_id.clone(),
            store_version: receipt.import_audit.resulting_store_version,
            boundary_version: block.boundary_version,
            contract_version: block.contract_version,
            source: MissionTransitionSource::MissionServiceDecision,
            source_digest: intent_digest.clone(),
            authored_by: self.state.mission_service_actor_id.clone(),
            transaction_id: Some(binding.transaction_id.clone()),
            execution_dispatch: head.execution_dispatch.clone(),
            execution_result_digest: head.execution_result_digest.clone(),
            review_result_digest: head.review_result_digest.clone(),
            receipt_candidate: Some(candidate),
            committed_receipt_id: Some(receipt.receipt_id.clone()),
            created_at: now_ms,
        };
        landed_letter.seal()?;
        landed_letter.validate_structural(&self.state.mission_service_actor_id)?;

        let mut outcome = LandOutcomeV1 {
            schema: LAND_OUTCOME_SCHEMA.to_string(),
            transaction_id: binding.transaction_id.clone(),
            idempotency_key: binding.idempotency_key.clone(),
            intent_digest: intent_digest.clone(),
            receipt_id: receipt.receipt_id.clone(),
            receipt_digest: receipt.receipt_digest.clone(),
            letter_id: landed_letter.head_id.clone(),
            resulting_store_version: receipt.import_audit.resulting_store_version,
            outcome_digest: String::new(),
            deduplicated: false,
        };
        outcome.seal()?;
        outcome.validate()?;

        let bundle = LandProvisionalBundleV1 {
            receipt,
            landed_letter,
            outcome: outcome.clone(),
        };
        let mut plan = LandProvisionalPlanV1 {
            schema: LAND_PROVISIONAL_PLAN_SCHEMA.to_string(),
            transaction: request.transaction.clone(),
            bundle,
            provisional_effects_digest: String::new(),
            created_at: now_ms,
        };
        plan.provisional_effects_digest = plan.compute_provisional_effects_digest()?;
        plan.validate(&self.state.mission_service_actor_id)?;
        self.persist_plan(&plan)?;

        let prepare = AuthorityWalPayloadV1::Prepare(Box::new(AuthorityWalPrepareV1 {
            transaction: request.transaction.clone(),
        }));
        match self.append_wal_phase(&plan, prepare, now_ms)? {
            AuthorityWalAppendOutcome::Appended { .. } => {}
            AuthorityWalAppendOutcome::TerminalReplay(terminal) => {
                return self.replay_terminal_land(terminal, &replay_scope, &intent_digest)
            }
        }
        if crash_point == Some(LandCrashPoint::AfterPrepare) {
            return Err(MissionServiceError::SimulatedCrash {
                point: LandCrashPoint::AfterPrepare,
            });
        }

        self.append_wal_phase(
            &plan,
            AuthorityWalPayloadV1::Provisional(AuthorityWalProvisionalV1 {
                provisional_effects_digest: plan.provisional_effects_digest.clone(),
            }),
            now_ms,
        )?;
        if crash_point == Some(LandCrashPoint::AfterProvisional) {
            return Err(MissionServiceError::SimulatedCrash {
                point: LandCrashPoint::AfterProvisional,
            });
        }

        let commit_payload = AuthorityWalPayloadV1::Commit(AuthorityWalCommitV1 {
            committed_at: now_ms,
            protected_time_evidence_digest: authority.protected_time_evidence_digest.clone(),
            authorization_snapshot_digest: authority.authorization_snapshot_digest.clone(),
            terminal_outcome_digest: outcome.outcome_digest.clone(),
        });
        if let Some(coordinator) = commit_coordinator {
            let mut append = || self.append_wal_phase(&plan, commit_payload.clone(), now_ms);
            coordinator.append_commit(authority, &request.transaction, now_ms, &mut append)?;
        } else {
            self.append_wal_phase(&plan, commit_payload, now_ms)?;
        }
        if crash_point == Some(LandCrashPoint::AfterCommit) {
            return Err(MissionServiceError::SimulatedCrash {
                point: LandCrashPoint::AfterCommit,
            });
        }

        self.apply_committed_bundle(&plan, &replay_scope)?;
        Ok(outcome)
    }

    fn validate_transition_evidence(
        &self,
        authority: &AuthenticatedAuthorityContextV1,
        intent: &MissionTransitionIntentV1,
        payload: &MissionTransitionPayloadV1,
        current: Option<&MissionLetterV1>,
        now_ms: u64,
    ) -> MissionServiceResult<DerivedTransitionArtifacts> {
        let inherited_dispatch = current.and_then(|head| head.execution_dispatch.clone());
        let inherited_candidate = current.and_then(|head| head.receipt_candidate.clone());
        let inherited_execution_result =
            current.and_then(|head| head.execution_result_digest.clone());
        let inherited_review_result = current.and_then(|head| head.review_result_digest.clone());

        match &payload.evidence {
            MissionTransitionEvidenceV1::MissionServiceDecision { decision, dispatch } => {
                decision.validate(&self.state.mission_service_actor_id)?;
                if authority.subject_id != self.state.mission_service_actor_id
                    || authority.role != Role::MissionService
                {
                    return Err(MissionServiceError::refused(
                        "wrong_author",
                        "MissionServiceDecision requires the authenticated MissionService actor",
                    ));
                }
                let dispatch = match intent.to_state {
                    MissionState::Dispatching => Some(
                        dispatch
                            .as_ref()
                            .ok_or_else(|| {
                                MissionServiceError::refused(
                                    "missing_execution_dispatch",
                                    "transition to dispatching requires a durable dispatch intent",
                                )
                            })?
                            .clone(),
                    ),
                    _ if dispatch.is_some() => {
                        return Err(MissionServiceError::refused(
                            "unexpected_execution_dispatch",
                            "dispatch is only valid on a transition to dispatching",
                        ))
                    }
                    _ => inherited_dispatch,
                };
                if intent.to_state == MissionState::Dispatching {
                    let dispatch = dispatch.as_ref().expect("dispatching was checked above");
                    self.validate_dispatch(dispatch, current, intent, now_ms)?;
                }
                if intent.to_state == MissionState::MergeWait && inherited_candidate.is_none() {
                    return Err(MissionServiceError::refused(
                        "missing_receipt_candidate",
                        "merge_wait requires the canonical candidate inherited from the gate head",
                    ));
                }
                Ok(DerivedTransitionArtifacts {
                    execution_dispatch: dispatch,
                    execution_result_digest: inherited_execution_result,
                    review_result_digest: inherited_review_result,
                    receipt_candidate: if intent.to_state == MissionState::Revising {
                        None
                    } else {
                        inherited_candidate
                    },
                })
            }
            MissionTransitionEvidenceV1::AuthorProposal { proposal, dispatch } => {
                proposal.validate(&authority.subject_id)?;
                let dispatch = match intent.to_state {
                    MissionState::Dispatching => Some(
                        dispatch
                            .as_ref()
                            .ok_or_else(|| {
                                MissionServiceError::refused(
                                    "missing_execution_dispatch",
                                    "author proposal to dispatching requires a dispatch",
                                )
                            })?
                            .clone(),
                    ),
                    _ if dispatch.is_some() => {
                        return Err(MissionServiceError::refused(
                            "unexpected_execution_dispatch",
                            "dispatch is only valid on a transition to dispatching",
                        ))
                    }
                    _ => None,
                };
                if intent.to_state == MissionState::Dispatching {
                    let dispatch = dispatch.as_ref().expect("dispatching was checked above");
                    self.validate_dispatch(dispatch, current, intent, now_ms)?;
                }
                Ok(DerivedTransitionArtifacts {
                    execution_dispatch: dispatch,
                    execution_result_digest: None,
                    review_result_digest: None,
                    receipt_candidate: None,
                })
            }
            MissionTransitionEvidenceV1::ReviewResult { result, dispatch } => {
                let head = current.ok_or_else(|| {
                    MissionServiceError::refused(
                        "missing_mission_head",
                        "review result cannot open a new mission",
                    )
                })?;
                result.validate_against_head(head_context(head), now_ms)?;
                if result.expected_transition()? != intent.to_state
                    || result.reviewer_id != authority.subject_id
                    || result.issuer != authority.subject_id
                {
                    return Err(MissionServiceError::refused(
                        "wrong_author",
                        "review result decision/reviewer does not bind transition authority",
                    ));
                }
                let dispatch = match intent.to_state {
                    MissionState::Dispatching => Some(
                        dispatch
                            .as_ref()
                            .ok_or_else(|| {
                                MissionServiceError::refused(
                                    "missing_execution_dispatch",
                                    "approved judging result requires a dispatch intent",
                                )
                            })?
                            .clone(),
                    ),
                    _ if dispatch.is_some() => {
                        return Err(MissionServiceError::refused(
                            "unexpected_execution_dispatch",
                            "review dispatch is only valid when transitioning to dispatching",
                        ))
                    }
                    _ => inherited_dispatch,
                };
                if intent.to_state == MissionState::Dispatching {
                    let dispatch = dispatch.as_ref().expect("dispatching was checked above");
                    self.validate_dispatch(dispatch, current, intent, now_ms)?;
                }
                if intent.to_state == MissionState::MergeWait {
                    let candidate = inherited_candidate.as_ref().ok_or_else(|| {
                        MissionServiceError::refused(
                            "missing_receipt_candidate",
                            "approved review requires the canonical candidate",
                        )
                    })?;
                    if result.candidate_digest.as_deref()
                        != Some(candidate.candidate_digest.as_str())
                    {
                        return Err(MissionServiceError::refused(
                            "candidate_mismatch",
                            "review result candidate digest differs from canonical head candidate",
                        ));
                    }
                }
                Ok(DerivedTransitionArtifacts {
                    execution_dispatch: dispatch,
                    execution_result_digest: inherited_execution_result,
                    review_result_digest: Some(result.result_digest.clone()),
                    receipt_candidate: if intent.to_state == MissionState::Revising {
                        None
                    } else {
                        inherited_candidate
                    },
                })
            }
            MissionTransitionEvidenceV1::ExecutionDispatchAck { ack } => {
                let head = current.ok_or_else(|| {
                    MissionServiceError::refused(
                        "missing_mission_head",
                        "execution ACK cannot open a new mission",
                    )
                })?;
                let dispatch = head.execution_dispatch.as_ref().ok_or_else(|| {
                    MissionServiceError::refused(
                        "missing_execution_dispatch",
                        "dispatching head has no canonical dispatch intent",
                    )
                })?;
                ack.validate_against(dispatch)?;
                if ack.runner_id != authority.subject_id || ack.issuer != authority.subject_id {
                    return Err(MissionServiceError::refused(
                        "wrong_author",
                        "ACK issuer/runner differs from authenticated runner",
                    ));
                }
                if intent.to_state != MissionState::Executing {
                    return Err(MissionServiceError::refused(
                        "ack_transition_mismatch",
                        "execution ACK can only produce executing",
                    ));
                }
                Ok(DerivedTransitionArtifacts {
                    execution_dispatch: Some(dispatch.clone()),
                    execution_result_digest: None,
                    review_result_digest: None,
                    receipt_candidate: None,
                })
            }
            MissionTransitionEvidenceV1::ExecutionResult { result, candidate } => {
                let head = current.ok_or_else(|| {
                    MissionServiceError::refused(
                        "missing_mission_head",
                        "execution result cannot open a new mission",
                    )
                })?;
                let dispatch = head.execution_dispatch.as_ref().ok_or_else(|| {
                    MissionServiceError::refused(
                        "missing_execution_dispatch",
                        "executing head has no canonical dispatch",
                    )
                })?;
                result.validate_against(dispatch, head_context(head))?;
                if result.expected_transition() != intent.to_state
                    || result.runner_id != authority.subject_id
                    || result.issuer != authority.subject_id
                {
                    return Err(MissionServiceError::refused(
                        "wrong_author",
                        "execution result outcome/runner does not bind transition authority",
                    ));
                }
                let candidate = match result.outcome {
                    ExecutionOutcome::Succeeded => {
                        let candidate = candidate.as_ref().ok_or_else(|| {
                            MissionServiceError::refused(
                                "missing_receipt_candidate",
                                "successful execution must produce a versioned receipt candidate",
                            )
                        })?;
                        self.validate_candidate_for_result(candidate, result, head)?;
                        Some(candidate.clone())
                    }
                    ExecutionOutcome::Failed if candidate.is_some() => {
                        return Err(MissionServiceError::refused(
                            "candidate_on_failed_execution",
                            "failed execution cannot propose a receipt candidate",
                        ))
                    }
                    ExecutionOutcome::Failed => None,
                };
                Ok(DerivedTransitionArtifacts {
                    execution_dispatch: Some(dispatch.clone()),
                    execution_result_digest: Some(result.result_digest.clone()),
                    review_result_digest: None,
                    receipt_candidate: candidate,
                })
            }
        }
    }

    fn validate_dispatch(
        &self,
        dispatch: &ExecutionDispatchV1,
        current: Option<&MissionLetterV1>,
        intent: &MissionTransitionIntentV1,
        now_ms: u64,
    ) -> MissionServiceResult<()> {
        dispatch.validate(now_ms)?;
        if dispatch.brain_id != self.state.brain_id
            || dispatch.mission_id != intent.mission_id
            || dispatch.iteration_id != intent.iteration_id
            || dispatch.packet_digest != intent.packet_digest
        {
            return Err(MissionServiceError::refused(
                "execution_dispatch_binding_mismatch",
                "dispatch does not bind brain/mission/iteration/packet",
            ));
        }
        match current {
            Some(head) if dispatch.mission_head_id != head.head_id => {
                return Err(MissionServiceError::refused(
                    "stale_head",
                    "dispatch does not bind the exact current head",
                ))
            }
            None if dispatch.mission_head_id != intent.transition_id => {
                return Err(MissionServiceError::refused(
                    "dispatch_genesis_binding_mismatch",
                    "a genesis direct dispatch must bind transition_id as its pre-head anchor",
                ))
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_candidate_for_result(
        &self,
        candidate: &ReceiptCandidateV1,
        result: &ExecutionResultV1,
        head: &MissionLetterV1,
    ) -> MissionServiceResult<()> {
        candidate.validate_structural()?;
        if candidate.brain_id != self.state.brain_id
            || candidate.mission_id != head.mission_id
            || candidate.mission_head_id != head.head_id
            || candidate.iteration_id != head.iteration_id
            || candidate.block_id != head.block_id
            || candidate.execution_result_digest != result.result_digest
            || candidate.issuer != result.runner_id
        {
            return Err(MissionServiceError::refused(
                "candidate_binding_mismatch",
                "candidate does not bind exact execution/head/mission/block/issuer",
            ));
        }
        self.validate_candidate_against_catalog(candidate)
    }

    fn validate_candidate_against_catalog(
        &self,
        candidate: &ReceiptCandidateV1,
    ) -> MissionServiceResult<()> {
        candidate.validate_structural()?;
        let block = self.canonical_block(&candidate.block_id)?;
        self.validate_scope(
            candidate.store_version,
            candidate.boundary_version,
            candidate.contract_version,
            block,
        )?;
        for evidence in &candidate.evidence_refs {
            let canonical = self.state.canonical_evidence.iter().any(|anchor| {
                anchor.locator == evidence.locator
                    && anchor.sha256 == evidence.sha256
                    && anchor.producer_id == evidence.producer_id
            });
            if !canonical {
                return Err(MissionServiceError::refused(
                    "invented_evidence_anchor",
                    format!(
                        "evidence '{}' / {} is absent from the owner canonical catalog",
                        evidence.locator, evidence.sha256
                    ),
                ));
            }
        }
        Ok(())
    }

    fn validate_land_authority_bindings(
        &self,
        authority: &AuthenticatedAuthorityContextV1,
        request: &LandRequestV1,
        now_ms: u64,
        require_current_transaction_window: bool,
    ) -> MissionServiceResult<()> {
        if request.brain_id != self.state.brain_id || request.expected_store_version == 0 {
            return Err(MissionServiceError::refused(
                "brain_mismatch",
                "land request does not bind the served brain/current store",
            ));
        }
        let binding = request.transaction.binding();
        let positive = match &request.transaction {
            AuthorityTransactionV1::PositiveAuthority(positive) => positive,
            AuthorityTransactionV1::SafetyKernel(_) => {
                return Err(MissionServiceError::refused(
                    "land_requires_positive_authority",
                    "land cannot use the safety transaction variant",
                ))
            }
        };
        if binding.organism_id != self.state.organism_id
            || binding.brain_id != self.state.brain_id
            || binding.subject_id != authority.subject_id
            || binding.action_id != "land"
            || binding.idempotency_key != request.idempotency_key
            || binding.capability_id != authority.capability_id
            || Some(binding.capability_kind) != authority.capability_kind
            || binding.expected_head_id.as_deref() != Some(request.expected_head_id.as_str())
            || binding.expected_active_mode != authority.active_mode
            || binding.authorization_snapshot_digest != authority.authorization_snapshot_digest
            || (require_current_transaction_window
                && (binding.issued_at > now_ms || now_ms >= binding.expires_at))
        {
            return Err(MissionServiceError::refused(
                "land_authority_binding_mismatch",
                "transaction binding differs from authenticated land request/context",
            ));
        }
        if positive.required_authority_variant != authority.authority_variant
            || Some(positive.authority_decision_digest.as_str())
                != authority.authority_decision_digest.as_deref()
            || Some(positive.identity_role_binding_digest.as_str())
                != authority.identity_role_binding_digest.as_deref()
            || positive.expected_store_version != request.expected_store_version
        {
            return Err(MissionServiceError::refused(
                "land_positive_authority_mismatch",
                "positive transaction reservations differ from authenticated authority",
            ));
        }
        Ok(())
    }

    fn canonical_merge_wait_head(
        &self,
        mission_id: &str,
        expected_head_id: &str,
        candidate_id: &str,
        expected_candidate_digest: &str,
        expected_store_version: u64,
    ) -> MissionServiceResult<&MissionLetterV1> {
        let head = self.state.head(mission_id).ok_or_else(|| {
            MissionServiceError::refused("unknown_mission", format!("mission '{mission_id}'"))
        })?;
        if head.head_id != expected_head_id {
            return Err(MissionServiceError::refused(
                "stale_head",
                format!(
                    "expected head '{expected_head_id}', canonical head is '{}'",
                    head.head_id
                ),
            ));
        }
        if head.state != MissionState::MergeWait || head.state.is_terminal() {
            return Err(MissionServiceError::refused(
                "mission_not_merge_wait",
                format!("canonical mission state is {:?}", head.state),
            ));
        }
        if head.store_version != expected_store_version {
            return Err(MissionServiceError::refused(
                "stale_store",
                format!(
                    "expected store version {expected_store_version}, head saw {}",
                    head.store_version
                ),
            ));
        }
        let candidate = head.receipt_candidate.as_ref().ok_or_else(|| {
            MissionServiceError::refused(
                "missing_receipt_candidate",
                "merge_wait head has no canonical candidate",
            )
        })?;
        if candidate.candidate_id != candidate_id
            || candidate.candidate_digest != expected_candidate_digest
        {
            return Err(MissionServiceError::refused(
                "candidate_mismatch",
                "client equality check does not match owner-reread canonical candidate",
            ));
        }
        self.validate_candidate_against_catalog(candidate)?;
        Ok(head)
    }

    fn canonical_block(&self, block_id: &str) -> MissionServiceResult<&CanonicalBlockBindingV1> {
        self.state.block(block_id).ok_or_else(|| {
            MissionServiceError::refused(
                "unknown_block",
                format!("canonical block '{block_id}' does not exist"),
            )
        })
    }

    fn validate_scope(
        &self,
        store_version: u64,
        boundary_version: u32,
        contract_version: u32,
        block: &CanonicalBlockBindingV1,
    ) -> MissionServiceResult<()> {
        if store_version != block.store_version {
            return Err(MissionServiceError::refused(
                "stale_store",
                format!(
                    "expected store version {store_version}, canonical is {}",
                    block.store_version
                ),
            ));
        }
        if boundary_version != block.boundary_version {
            return Err(MissionServiceError::refused(
                "stale_boundary",
                format!(
                    "expected boundary version {boundary_version}, canonical is {}",
                    block.boundary_version
                ),
            ));
        }
        if contract_version != block.contract_version {
            return Err(MissionServiceError::refused(
                "stale_contract",
                format!(
                    "expected contract version {contract_version}, canonical is {}",
                    block.contract_version
                ),
            ));
        }
        Ok(())
    }

    fn plan_path(&self, transaction: &AuthorityTransactionV1) -> PathBuf {
        self.plans_dir
            .join(format!("{}.json", transaction.transaction_digest()))
    }

    fn persist_plan(&self, plan: &LandProvisionalPlanV1) -> MissionServiceResult<()> {
        let path = self.plan_path(&plan.transaction);
        if path.exists() {
            let existing: LandProvisionalPlanV1 = serde_json::from_slice(&std::fs::read(&path)?)?;
            if existing != *plan {
                return Err(MissionServiceError::refused(
                    "transaction_plan_conflict",
                    "transaction digest already names different provisional bytes",
                ));
            }
            return Ok(());
        }
        write_json_new_durable(&path, plan)
    }

    fn append_wal_phase(
        &mut self,
        plan: &LandProvisionalPlanV1,
        payload: AuthorityWalPayloadV1,
        recorded_at: u64,
    ) -> MissionServiceResult<AuthorityWalAppendOutcome> {
        let record = m1nd_control::AuthorityWalRecordV1::draft(
            &plan.transaction,
            payload,
            recorded_at,
            "assigned-by-authority-wal",
            "assigned-by-authority-wal",
            "ASSIGNED_BY_AUTHORITY_WAL",
            OpaqueSignature::new("assigned-by-authority-wal"),
        );
        Ok(self.wal.append(record)?)
    }

    fn apply_committed_bundle(
        &mut self,
        plan: &LandProvisionalPlanV1,
        replay_scope: &str,
    ) -> MissionServiceResult<()> {
        plan.validate(&self.state.mission_service_actor_id)?;
        if self
            .wal
            .committed_transaction(plan.transaction.binding().transaction_id.as_str())
            != Some(&plan.transaction)
        {
            return Err(MissionServiceError::refused(
                "uncommitted_visibility_refused",
                "provisional receipt/letter are invisible until durable WAL COMMIT",
            ));
        }
        let transaction_id = plan.transaction.binding().transaction_id.as_str();
        let already_applied = self.state.receipts.iter().any(|receipt| {
            receipt.transaction_id == transaction_id
                && receipt.receipt_id == plan.bundle.receipt.receipt_id
        }) && self.state.letters.iter().any(|letter| {
            letter.transaction_id.as_deref() == Some(transaction_id)
                && letter.head_id == plan.bundle.landed_letter.head_id
        });
        if already_applied {
            return Ok(());
        }
        let current = self
            .state
            .head(&plan.bundle.landed_letter.mission_id)
            .ok_or_else(|| MissionServiceError::Corruption {
                detail: "committed land plan references a missing mission".to_string(),
            })?;
        if current.head_id
            != plan
                .bundle
                .landed_letter
                .previous_head_id
                .as_deref()
                .unwrap_or_default()
            || current.state != MissionState::MergeWait
        {
            return Err(MissionServiceError::Corruption {
                detail: format!(
                    "committed land plan cannot extend canonical head {}",
                    current.head_id
                ),
            });
        }
        let expected_store = plan.bundle.receipt.import_audit.expected_store_version;
        if self
            .state
            .canonical_blocks
            .iter()
            .any(|block| block.store_version != expected_store)
        {
            return Err(MissionServiceError::Corruption {
                detail: "committed land plan store reservation no longer matches".to_string(),
            });
        }

        let mut next = self.state.clone();
        next.receipts.push(plan.bundle.receipt.clone());
        next.letters.push(plan.bundle.landed_letter.clone());
        let resulting = plan.bundle.receipt.import_audit.resulting_store_version;
        for block in &mut next.canonical_blocks {
            block.store_version = resulting;
        }
        next.land_replays.insert(
            replay_scope.to_string(),
            LandReplayV1 {
                intent_digest: plan.transaction.binding().intent_digest.clone(),
                transaction_digest: plan.transaction.transaction_digest().to_string(),
                brain_id: plan.bundle.receipt.brain_id.clone(),
                mission_id: plan.bundle.receipt.mission_id.clone(),
                expected_head_id: plan.bundle.receipt.mission_head_id.clone(),
                candidate_id: plan
                    .bundle
                    .landed_letter
                    .receipt_candidate
                    .as_ref()
                    .ok_or_else(|| MissionServiceError::Corruption {
                        detail: "landed plan is missing its canonical candidate".to_string(),
                    })?
                    .candidate_id
                    .clone(),
                expected_candidate_digest: plan.bundle.receipt.candidate_digest.clone(),
                expected_store_version: plan.bundle.receipt.import_audit.expected_store_version,
                outcome: plan.bundle.outcome.clone(),
            },
        );
        next.state_version =
            next.state_version
                .checked_add(1)
                .ok_or_else(|| MissionServiceError::Corruption {
                    detail: "state_version overflow".to_string(),
                })?;
        next.validate()?;
        persist_state(&self.state_path, &next)?;
        self.state = next;
        Ok(())
    }

    fn replay_terminal_land(
        &mut self,
        terminal: AuthorityWalTerminalOutcome,
        replay_scope: &str,
        intent_digest: &str,
    ) -> MissionServiceResult<LandOutcomeV1> {
        if terminal.phase != AuthorityWalPhase::Commit {
            return Err(MissionServiceError::refused(
                "transaction_previously_aborted",
                format!("transaction {} is terminal ABORT", terminal.transaction_id),
            ));
        }
        let plans = self.read_plans()?;
        let plan = plans
            .into_iter()
            .find(|plan| plan.transaction.binding().transaction_id == terminal.transaction_id)
            .ok_or_else(|| MissionServiceError::Corruption {
                detail: "terminal WAL replay has no durable land plan".to_string(),
            })?;
        if plan.transaction.binding().intent_digest != intent_digest {
            return Err(MissionServiceError::refused(
                "idempotency_conflict",
                "terminal WAL replay binds another intent",
            ));
        }
        self.apply_committed_bundle(&plan, replay_scope)?;
        let mut outcome = plan.bundle.outcome;
        outcome.deduplicated = true;
        Ok(outcome)
    }

    fn recover_plans(
        &mut self,
        now_ms: u64,
    ) -> MissionServiceResult<MissionServiceRecoveryReportV1> {
        let plans = self.read_plans()?;
        let mut report = MissionServiceRecoveryReportV1::default();
        for plan in plans {
            plan.validate(&self.state.mission_service_actor_id)?;
            let binding = plan.transaction.binding();
            let replay_scope = idempotency_scope(
                &binding.organism_id,
                &binding.brain_id,
                &binding.subject_id,
                &binding.action_id,
                &binding.idempotency_key,
            )?;
            match self.wal.terminal_outcome(&binding.transaction_id).cloned() {
                Some(terminal) if terminal.phase == AuthorityWalPhase::Commit => {
                    self.apply_committed_bundle(&plan, &replay_scope)?;
                    report.committed_forward_completed += 1;
                }
                Some(_) => report.already_terminal += 1,
                None => {
                    // The immutable plan is written before PREPARE. If PREPARE did
                    // not make it to disk, append it now solely so the same
                    // transaction can be terminally ABORTED. If it did, the
                    // duplicate is expected and no new reservation is created.
                    let prepare = AuthorityWalPayloadV1::Prepare(Box::new(AuthorityWalPrepareV1 {
                        transaction: plan.transaction.clone(),
                    }));
                    match self.append_wal_phase(&plan, prepare, now_ms) {
                        Ok(AuthorityWalAppendOutcome::TerminalReplay(terminal))
                            if terminal.phase == AuthorityWalPhase::Commit =>
                        {
                            self.apply_committed_bundle(&plan, &replay_scope)?;
                            report.committed_forward_completed += 1;
                            continue;
                        }
                        Ok(AuthorityWalAppendOutcome::TerminalReplay(_)) => {
                            report.already_terminal += 1;
                            continue;
                        }
                        Ok(AuthorityWalAppendOutcome::Appended { .. })
                        | Err(MissionServiceError::AuthorityWal(
                            AuthorityWalError::DuplicateTransaction { .. }
                            | AuthorityWalError::DuplicateIdempotency { .. },
                        )) => {}
                        Err(error) => return Err(error),
                    }
                    let abort_at = now_ms.max(binding.issued_at);
                    self.append_wal_phase(
                        &plan,
                        AuthorityWalPayloadV1::Abort(AuthorityWalAbortV1 {
                            aborted_at: abort_at,
                            reason_digest: plan.provisional_effects_digest.clone(),
                            terminal_outcome_digest: plan.bundle.outcome.outcome_digest.clone(),
                        }),
                        abort_at,
                    )?;
                    report.uncommitted_aborted += 1;
                }
            }
        }
        Ok(report)
    }

    fn read_plans(&self) -> MissionServiceResult<Vec<LandProvisionalPlanV1>> {
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(&self.plans_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) == Some("json") {
                paths.push(path);
            }
        }
        paths.sort();
        let mut plans = Vec::with_capacity(paths.len());
        for path in paths {
            let plan: LandProvisionalPlanV1 = serde_json::from_slice(&std::fs::read(&path)?)?;
            plans.push(plan);
        }
        Ok(plans)
    }
}

#[derive(Default)]
struct DerivedTransitionArtifacts {
    execution_dispatch: Option<ExecutionDispatchV1>,
    execution_result_digest: Option<String>,
    review_result_digest: Option<String>,
    receipt_candidate: Option<ReceiptCandidateV1>,
}

fn execution_dispatch_from_payload(
    payload: &MissionTransitionPayloadV1,
) -> Option<&ExecutionDispatchV1> {
    match &payload.evidence {
        MissionTransitionEvidenceV1::MissionServiceDecision { dispatch, .. }
        | MissionTransitionEvidenceV1::AuthorProposal { dispatch, .. }
        | MissionTransitionEvidenceV1::ReviewResult { dispatch, .. } => dispatch.as_ref(),
        MissionTransitionEvidenceV1::ExecutionDispatchAck { .. }
        | MissionTransitionEvidenceV1::ExecutionResult { .. } => None,
    }
}

fn execution_head_from_letter(
    letter: &MissionLetterV1,
) -> MissionServiceResult<ExecutionMissionHeadV1> {
    if letter.state != MissionState::Executing {
        return Err(MissionServiceError::Corruption {
            detail: format!(
                "execution lifecycle expected EXECUTING letter, observed {:?}",
                letter.state
            ),
        });
    }
    Ok(ExecutionMissionHeadV1 {
        schema: EXECUTION_MISSION_HEAD_SCHEMA.to_string(),
        head_id: letter.head_id.clone(),
        state: letter.state,
        iteration_id: letter.iteration_id,
        packet_digest: letter.packet_digest.clone(),
    })
}

fn head_context(head: &MissionLetterV1) -> MissionHeadContext<'_> {
    MissionHeadContext {
        brain_id: &head.brain_id,
        mission_id: &head.mission_id,
        head: Some(MissionHeadSnapshot {
            head_id: &head.head_id,
            state: head.state,
            iteration_id: head.iteration_id,
            packet_digest: &head.packet_digest,
        }),
    }
}

fn idempotency_scope(
    organism_id: &str,
    brain_id: &str,
    subject_id: &str,
    action_id: &str,
    idempotency_key: &str,
) -> MissionServiceResult<String> {
    #[derive(Serialize)]
    struct Scope<'a> {
        organism_id: &'a str,
        brain_id: &'a str,
        subject_id: &'a str,
        action_id: &'a str,
        idempotency_key: &'a str,
    }
    Ok(digest_canonical(
        IDEMPOTENCY_SCOPE_DIGEST_DOMAIN,
        &Scope {
            organism_id,
            brain_id,
            subject_id,
            action_id,
            idempotency_key,
        },
    )?)
}

fn persist_state(path: &Path, state: &MissionServiceStateV1) -> MissionServiceResult<()> {
    let parent = usable_parent(path);
    std::fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(state)?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&bytes)?;
    file.flush()?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)?;
    sync_parent_directory(parent)?;
    Ok(())
}

fn write_json_new_durable<T: Serialize>(path: &Path, value: &T) -> MissionServiceResult<()> {
    write_json_new_durable_with(path, value, |file, bytes| file.write_all(bytes))
}

fn write_json_new_durable_with<T, F>(
    path: &Path,
    value: &T,
    write_bytes: F,
) -> MissionServiceResult<()>
where
    T: Serialize,
    F: FnOnce(&mut File, &[u8]) -> io::Result<()>,
{
    let parent = usable_parent(path);
    std::fs::create_dir_all(parent)?;
    let bytes = serde_json::to_vec_pretty(value)?;
    let (temporary, mut file) = create_unique_temporary(path)?;
    let prepare_result = (|| -> io::Result<()> {
        write_bytes(&mut file, &bytes)?;
        file.flush()?;
        file.sync_all()
    })();
    if let Err(error) = prepare_result {
        drop(file);
        cleanup_temporary_after_error(&temporary, &error)?;
        return Err(error.into());
    }
    drop(file);

    if let Err(error) = rename_noclobber(&temporary, path) {
        cleanup_temporary_after_error(&temporary, &error)?;
        return Err(error.into());
    }
    sync_parent_directory(parent)?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn write_json_new_durable_partial_failure_for_test<T: Serialize>(
    path: &Path,
    value: &T,
) -> MissionServiceResult<()> {
    write_json_new_durable_with(path, value, |file, bytes| {
        let partial_len = (bytes.len() / 2).max(1);
        file.write_all(&bytes[..partial_len])?;
        Err(io::Error::other("injected partial write failure"))
    })
}

fn create_unique_temporary(path: &Path) -> io::Result<(PathBuf, File)> {
    let parent = usable_parent(path);
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("durable path '{}' has no file name", path.display()),
        )
    })?;
    for _ in 0..128 {
        let sequence = DURABLE_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut temporary_name = OsString::from(".");
        temporary_name.push(file_name);
        temporary_name.push(format!(".{}.{}.tmp", std::process::id(), sequence));
        let temporary = parent.join(temporary_name);
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "could not allocate a unique durable temporary beside '{}'",
            path.display()
        ),
    ))
}

fn cleanup_temporary_after_error(temporary: &Path, primary: &io::Error) -> io::Result<()> {
    match std::fs::remove_file(temporary) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(cleanup) => Err(io::Error::new(
            cleanup.kind(),
            format!(
                "{primary}; additionally failed to remove temporary '{}': {cleanup}",
                temporary.display()
            ),
        )),
    }
}

#[cfg(any(target_vendor = "apple", target_os = "linux", target_os = "android"))]
fn path_cstring(path: &Path) -> io::Result<CString> {
    CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("path '{}' contains an interior NUL", path.display()),
        )
    })
}

#[cfg(target_vendor = "apple")]
fn rename_noclobber(source: &Path, destination: &Path) -> io::Result<()> {
    let source = path_cstring(source)?;
    let destination = path_cstring(destination)?;
    // SAFETY: both C strings are NUL-terminated and remain alive for the call;
    // `RENAME_EXCL` asks the kernel to atomically refuse an existing target.
    let result = unsafe {
        libc::renameatx_np(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_EXCL,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn rename_noclobber(source: &Path, destination: &Path) -> io::Result<()> {
    let source = path_cstring(source)?;
    let destination = path_cstring(destination)?;
    // SAFETY: both C strings are NUL-terminated and remain alive for the call;
    // `RENAME_NOREPLACE` asks the kernel to atomically refuse an existing target.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(any(target_vendor = "apple", target_os = "linux", target_os = "android")))]
fn rename_noclobber(source: &Path, destination: &Path) -> io::Result<()> {
    std::fs::hard_link(source, destination)?;
    std::fs::remove_file(source)
}

fn digest_without_fields<T: Serialize>(
    domain: &str,
    value: &T,
    fields: &[&str],
) -> Result<String, CanonicalError> {
    let mut value = serde_json::to_value(value)?;
    if let Value::Object(object) = &mut value {
        for field in fields {
            object.remove(*field);
        }
    }
    digest_canonical(domain, &value)
}

fn require_schema(
    contract: &'static str,
    actual: &str,
    expected: &'static str,
) -> MissionServiceResult<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(MissionServiceError::refused(
            "unsupported_schema",
            format!("{contract}: expected '{expected}', observed '{actual}'"),
        ))
    }
}

fn require_non_empty(field: &'static str, value: &str) -> MissionServiceResult<()> {
    if value.trim().is_empty() {
        Err(MissionServiceError::refused("empty_required_field", field))
    } else {
        Ok(())
    }
}

fn require_optional_non_empty(
    field: &'static str,
    value: Option<&str>,
) -> MissionServiceResult<()> {
    match value {
        Some(value) => require_non_empty(field, value),
        None => Ok(()),
    }
}

fn require_digest(field: &'static str, value: &str) -> MissionServiceResult<()> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(MissionServiceError::refused("invalid_digest", field))
    }
}

fn require_optional_digest(field: &'static str, value: Option<&str>) -> MissionServiceResult<()> {
    match value {
        Some(value) => require_digest(field, value),
        None => Ok(()),
    }
}

fn looks_absolute(value: &str) -> bool {
    let value = value.trim();
    if value.starts_with('/') || value.starts_with('~') || value.starts_with("\\\\") {
        return true;
    }
    let bytes = value.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

fn usable_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path) -> io::Result<()> {
    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path) -> io::Result<()> {
    Ok(())
}
