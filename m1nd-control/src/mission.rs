use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{canonical_json, digest_canonical, CanonicalError, OpaqueSignature};

pub const MISSION_TRANSITION_INTENT_SCHEMA: &str = "m1nd-mission-transition-intent-v1";
pub const EXECUTION_DISPATCH_SCHEMA: &str = "m1nd-execution-dispatch-v1";
pub const EXECUTION_DISPATCH_ACK_SCHEMA: &str = "m1nd-execution-dispatch-ack-v1";
pub const EXECUTION_RESULT_SCHEMA: &str = "m1nd-execution-result-v1";
pub const REVIEW_RESULT_SCHEMA: &str = "m1nd-review-result-v1";

pub const MISSION_TRANSITION_INTENT_DIGEST_DOMAIN: &str = "m1nd-mission-transition-intent-v1";
pub const MISSION_TRANSITION_PAYLOAD_DIGEST_DOMAIN: &str = "m1nd-mission-transition-payload-v1";
pub const EXECUTION_DISPATCH_DIGEST_DOMAIN: &str = "m1nd-execution-dispatch-v1";
pub const EXECUTION_DISPATCH_ACK_DIGEST_DOMAIN: &str = "m1nd-execution-dispatch-ack-v1";
pub const EXECUTION_RESULT_DIGEST_DOMAIN: &str = "m1nd-execution-result-v1";
pub const REVIEW_RESULT_DIGEST_DOMAIN: &str = "m1nd-review-result-v1";
pub const EXECUTION_RESULT_SIGNATURE_DOMAIN: &str = "m1nd-execution-result-signature-v1";
pub const REVIEW_RESULT_SIGNATURE_DOMAIN: &str = "m1nd-review-result-signature-v1";

pub const DEFAULT_MISSION_CLOCK_SKEW_MS: u64 = 30_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionState {
    Judging,
    Revising,
    Dispatching,
    Executing,
    Gate,
    Review,
    MergeWait,
    Landed,
    Failed,
    Archived,
}

impl MissionState {
    pub const ALL: [Self; 10] = [
        Self::Judging,
        Self::Revising,
        Self::Dispatching,
        Self::Executing,
        Self::Gate,
        Self::Review,
        Self::MergeWait,
        Self::Landed,
        Self::Failed,
        Self::Archived,
    ];

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Landed | Self::Failed | Self::Archived)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    MissionService,
    Author,
    Reviewer,
    Runner,
}

impl Role {
    pub const ALL: [Self; 4] = [
        Self::MissionService,
        Self::Author,
        Self::Reviewer,
        Self::Runner,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionTransitionSource {
    MissionServiceDecision,
    AuthorProposal,
    ReviewResult,
    ExecutionDispatchAck,
    ExecutionResult,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IterationRule {
    Initialize,
    Preserve,
    Advance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissionTransitionRule {
    pub from: Option<MissionState>,
    pub to: MissionState,
    pub role: Role,
    pub source: MissionTransitionSource,
    pub iteration: IterationRule,
}

const fn rule(
    from: Option<MissionState>,
    to: MissionState,
    role: Role,
    source: MissionTransitionSource,
    iteration: IterationRule,
) -> MissionTransitionRule {
    MissionTransitionRule {
        from,
        to,
        role,
        source,
        iteration,
    }
}

/// The complete PRD G3 state machine. Absence from this table is refusal.
pub const MISSION_TRANSITION_RULES: &[MissionTransitionRule] = &[
    rule(
        None,
        MissionState::Judging,
        Role::MissionService,
        MissionTransitionSource::MissionServiceDecision,
        IterationRule::Initialize,
    ),
    rule(
        None,
        MissionState::Dispatching,
        Role::MissionService,
        MissionTransitionSource::MissionServiceDecision,
        IterationRule::Initialize,
    ),
    rule(
        Some(MissionState::Judging),
        MissionState::Dispatching,
        Role::Reviewer,
        MissionTransitionSource::ReviewResult,
        IterationRule::Preserve,
    ),
    rule(
        Some(MissionState::Judging),
        MissionState::Revising,
        Role::Reviewer,
        MissionTransitionSource::ReviewResult,
        IterationRule::Preserve,
    ),
    rule(
        Some(MissionState::Judging),
        MissionState::Failed,
        Role::Reviewer,
        MissionTransitionSource::ReviewResult,
        IterationRule::Preserve,
    ),
    rule(
        Some(MissionState::Revising),
        MissionState::Judging,
        Role::Author,
        MissionTransitionSource::AuthorProposal,
        IterationRule::Advance,
    ),
    rule(
        Some(MissionState::Revising),
        MissionState::Dispatching,
        Role::Author,
        MissionTransitionSource::AuthorProposal,
        IterationRule::Advance,
    ),
    rule(
        Some(MissionState::Dispatching),
        MissionState::Executing,
        Role::Runner,
        MissionTransitionSource::ExecutionDispatchAck,
        IterationRule::Preserve,
    ),
    rule(
        Some(MissionState::Dispatching),
        MissionState::Failed,
        Role::MissionService,
        MissionTransitionSource::MissionServiceDecision,
        IterationRule::Preserve,
    ),
    rule(
        Some(MissionState::Executing),
        MissionState::Gate,
        Role::Runner,
        MissionTransitionSource::ExecutionResult,
        IterationRule::Preserve,
    ),
    rule(
        Some(MissionState::Executing),
        MissionState::Failed,
        Role::Runner,
        MissionTransitionSource::ExecutionResult,
        IterationRule::Preserve,
    ),
    rule(
        Some(MissionState::Gate),
        MissionState::Review,
        Role::MissionService,
        MissionTransitionSource::MissionServiceDecision,
        IterationRule::Preserve,
    ),
    rule(
        Some(MissionState::Gate),
        MissionState::MergeWait,
        Role::MissionService,
        MissionTransitionSource::MissionServiceDecision,
        IterationRule::Preserve,
    ),
    rule(
        Some(MissionState::Gate),
        MissionState::Failed,
        Role::MissionService,
        MissionTransitionSource::MissionServiceDecision,
        IterationRule::Preserve,
    ),
    rule(
        Some(MissionState::Review),
        MissionState::Revising,
        Role::Reviewer,
        MissionTransitionSource::ReviewResult,
        IterationRule::Preserve,
    ),
    rule(
        Some(MissionState::Review),
        MissionState::MergeWait,
        Role::Reviewer,
        MissionTransitionSource::ReviewResult,
        IterationRule::Preserve,
    ),
    rule(
        Some(MissionState::Review),
        MissionState::Failed,
        Role::Reviewer,
        MissionTransitionSource::ReviewResult,
        IterationRule::Preserve,
    ),
    rule(
        Some(MissionState::MergeWait),
        MissionState::Landed,
        Role::MissionService,
        MissionTransitionSource::MissionServiceDecision,
        IterationRule::Preserve,
    ),
    rule(
        Some(MissionState::MergeWait),
        MissionState::Archived,
        Role::MissionService,
        MissionTransitionSource::MissionServiceDecision,
        IterationRule::Preserve,
    ),
];

pub fn mission_transition_rule(
    from: Option<MissionState>,
    to: MissionState,
) -> Option<MissionTransitionRule> {
    MISSION_TRANSITION_RULES
        .iter()
        .copied()
        .find(|candidate| candidate.from == from && candidate.to == to)
}

/// Structural validation never authenticates an opaque signature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissionIntegrityDisposition {
    OpaqueSignaturePresentUnverified,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractStructuralValidation {
    pub canonical_digest: String,
    pub integrity: MissionIntegrityDisposition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissionTransitionValidation {
    pub rule: MissionTransitionRule,
    pub intent_digest: String,
    pub integrity: MissionIntegrityDisposition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissionHeadSnapshot<'a> {
    pub head_id: &'a str,
    pub state: MissionState,
    pub iteration_id: u64,
    pub packet_digest: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissionHeadContext<'a> {
    pub brain_id: &'a str,
    pub mission_id: &'a str,
    pub head: Option<MissionHeadSnapshot<'a>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionTransitionIntentV1 {
    pub schema: String,
    pub transition_id: String,
    pub brain_id: String,
    pub mission_id: String,
    pub expected_head_id: Option<String>,
    pub from_state: Option<MissionState>,
    pub to_state: MissionState,
    pub iteration_id: u64,
    pub actor_id: String,
    pub role: Role,
    pub source: MissionTransitionSource,
    pub source_digest: String,
    pub capability_id: String,
    pub packet_digest: String,
    pub payload_digest: String,
    pub idempotency_key: String,
    pub causation_id: Option<String>,
    pub issued_at: u64,
    pub expires_at: u64,
    pub issuer: String,
    pub key_id: String,
    pub algorithm: String,
    pub intent_digest: String,
    pub signature: OpaqueSignature,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionDispatchState {
    Intent,
    Acked,
    Completed,
    Failed,
}

impl ExecutionDispatchState {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionDispatchV1 {
    pub schema: String,
    pub execution_id: String,
    pub brain_id: String,
    pub mission_id: String,
    pub mission_head_id: String,
    pub iteration_id: u64,
    pub packet_digest: String,
    pub runner_id: String,
    pub idempotency_key: String,
    pub issued_at: u64,
    pub deadline_at: u64,
    pub state: ExecutionDispatchState,
    pub issuer: String,
    pub key_id: String,
    pub algorithm: String,
    pub dispatch_digest: String,
    pub signature: OpaqueSignature,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionDispatchAckV1 {
    pub schema: String,
    pub ack_id: String,
    pub execution_id: String,
    pub dispatch_digest: String,
    pub brain_id: String,
    pub mission_id: String,
    pub mission_head_id: String,
    pub iteration_id: u64,
    pub runner_id: String,
    pub accepted_at: u64,
    pub deduplicated: bool,
    pub issuer: String,
    pub key_id: String,
    pub algorithm: String,
    pub ack_digest: String,
    pub signature: OpaqueSignature,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionOutcome {
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionResultV1 {
    pub schema: String,
    pub result_id: String,
    pub execution_id: String,
    pub dispatch_digest: String,
    pub brain_id: String,
    pub mission_id: String,
    pub mission_head_id: String,
    pub iteration_id: u64,
    pub runner_id: String,
    pub outcome: ExecutionOutcome,
    pub command: Vec<String>,
    pub exit_status: Option<i32>,
    pub started_at: u64,
    pub ended_at: u64,
    pub log_digest: String,
    pub failure_artifact_digest: Option<String>,
    pub issuer: String,
    pub key_id: String,
    pub algorithm: String,
    pub result_digest: String,
    pub signature: OpaqueSignature,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewDecision {
    Approve,
    Change,
    Reject,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewResultV1 {
    pub schema: String,
    pub result_id: String,
    pub brain_id: String,
    pub mission_id: String,
    pub mission_head_id: String,
    pub iteration_id: u64,
    pub reviewer_id: String,
    pub reviewed_state: MissionState,
    pub packet_digest: String,
    pub decision: ReviewDecision,
    pub verdict_digest: String,
    pub binding_changes_digest: Option<String>,
    pub gate_digest: Option<String>,
    pub candidate_digest: Option<String>,
    pub issued_at: u64,
    pub issuer: String,
    pub key_id: String,
    pub algorithm: String,
    pub result_digest: String,
    pub signature: OpaqueSignature,
}

#[derive(Debug, Error)]
pub enum MissionContractError {
    #[error("unsupported {contract} schema '{actual}'")]
    Schema {
        contract: &'static str,
        actual: String,
    },
    #[error("required field '{field}' is empty")]
    EmptyRequired { field: &'static str },
    #[error("required collection '{field}' is empty")]
    EmptyCollection { field: &'static str },
    #[error("iteration_id must be at least 1")]
    InvalidIteration,
    #[error("field '{field}' is not a lowercase SHA-256 digest")]
    InvalidDigest { field: &'static str },
    #[error("opaque signature is empty")]
    EmptyOpaqueSignature,
    #[error("{record} has invalid time order: {start} must be before {end}")]
    InvalidTimeOrder {
        record: &'static str,
        start: u64,
        end: u64,
    },
    #[error(
        "{record} was issued in the future at {issued_at}; latest allowed is {latest_allowed}"
    )]
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
    #[error("digest mismatch for '{field}': expected {expected}, observed {observed}")]
    DigestMismatch {
        field: &'static str,
        expected: String,
        observed: String,
    },
    #[error("illegal mission transition {from:?} -> {to:?}")]
    IllegalTransition {
        from: Option<MissionState>,
        to: MissionState,
    },
    #[error("wrong role for transition: expected {expected:?}, observed {observed:?}")]
    WrongRole { expected: Role, observed: Role },
    #[error("wrong source for transition: expected {expected:?}, observed {observed:?}")]
    WrongSource {
        expected: MissionTransitionSource,
        observed: MissionTransitionSource,
    },
    #[error("brain binding mismatch: expected '{expected}', observed '{observed}'")]
    BrainMismatch { expected: String, observed: String },
    #[error("mission binding mismatch: expected '{expected}', observed '{observed}'")]
    MissionMismatch { expected: String, observed: String },
    #[error("stale head: expected {expected:?}, observed {observed:?}")]
    StaleHead {
        expected: Option<String>,
        observed: Option<String>,
    },
    #[error("state binding mismatch: expected {expected:?}, observed {observed:?}")]
    StateMismatch {
        expected: Option<MissionState>,
        observed: Option<MissionState>,
    },
    #[error("stale iteration: expected {expected}, observed {observed}")]
    StaleIteration { expected: u64, observed: u64 },
    #[error("iteration overflow while advancing from {current}")]
    IterationOverflow { current: u64 },
    #[error("packet digest does not bind the current mission head")]
    PacketDigestMismatch,
    #[error("a revising transition must bind a new packet digest")]
    PacketDigestNotAdvanced,
    #[error("{contract} binding mismatch for field '{field}'")]
    BindingMismatch {
        contract: &'static str,
        field: &'static str,
    },
    #[error("dispatch state {state:?} cannot accept an execution ACK")]
    DispatchNotAccepting { state: ExecutionDispatchState },
    #[error("execution command must contain a non-empty executable")]
    EmptyExecutionCommand,
    #[error("successful execution must have exit_status=0 and no failure artifact")]
    InvalidSuccessfulExecution,
    #[error("failed execution must not have exit_status=0 and must carry a failure artifact")]
    InvalidFailedExecution,
    #[error("reviewed_state must be judging or review, observed {state:?}")]
    InvalidReviewedState { state: MissionState },
    #[error("review result field '{field}' is required for this state/decision")]
    MissingReviewField { field: &'static str },
    #[error("review result field '{field}' is forbidden for this state/decision")]
    ForbiddenReviewField { field: &'static str },
    #[error(transparent)]
    Canonical(#[from] CanonicalError),
}

impl MissionTransitionIntentV1 {
    pub fn payload_digest_for<T: Serialize + ?Sized>(
        payload: &T,
    ) -> Result<String, CanonicalError> {
        digest_canonical(MISSION_TRANSITION_PAYLOAD_DIGEST_DOMAIN, payload)
    }

    pub fn compute_intent_digest(&self) -> Result<String, CanonicalError> {
        digest_contract(
            MISSION_TRANSITION_INTENT_DIGEST_DOMAIN,
            self,
            "intent_digest",
        )
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.intent_digest = self.compute_intent_digest()?;
        Ok(())
    }

    /// Validate the total state-machine edge and all current-head bindings.
    /// Signature bytes remain opaque and explicitly unauthenticated here.
    pub fn validate<T: Serialize + ?Sized>(
        &self,
        context: MissionHeadContext<'_>,
        payload: &T,
        now_ms: u64,
    ) -> Result<MissionTransitionValidation, MissionContractError> {
        require_schema(
            "mission transition intent",
            &self.schema,
            MISSION_TRANSITION_INTENT_SCHEMA,
        )?;
        require_non_empty("transition_id", &self.transition_id)?;
        require_non_empty("brain_id", &self.brain_id)?;
        require_non_empty("mission_id", &self.mission_id)?;
        require_optional_non_empty("expected_head_id", self.expected_head_id.as_deref())?;
        require_non_empty("actor_id", &self.actor_id)?;
        require_non_empty("capability_id", &self.capability_id)?;
        require_non_empty("idempotency_key", &self.idempotency_key)?;
        require_optional_non_empty("causation_id", self.causation_id.as_deref())?;
        require_digest("source_digest", &self.source_digest)?;
        require_digest("packet_digest", &self.packet_digest)?;
        require_digest("payload_digest", &self.payload_digest)?;
        require_digest("intent_digest", &self.intent_digest)?;
        require_iteration(self.iteration_id)?;
        validate_signature_binding(&self.issuer, &self.key_id, &self.algorithm, &self.signature)?;
        validate_expiring_record(
            "mission transition intent",
            self.issued_at,
            self.expires_at,
            now_ms,
        )?;

        let expected_payload_digest = Self::payload_digest_for(payload)?;
        ensure_digest(
            "payload_digest",
            &expected_payload_digest,
            &self.payload_digest,
        )?;
        let expected_intent_digest = self.compute_intent_digest()?;
        ensure_digest(
            "intent_digest",
            &expected_intent_digest,
            &self.intent_digest,
        )?;

        if self.brain_id != context.brain_id {
            return Err(MissionContractError::BrainMismatch {
                expected: context.brain_id.to_string(),
                observed: self.brain_id.clone(),
            });
        }
        if self.mission_id != context.mission_id {
            return Err(MissionContractError::MissionMismatch {
                expected: context.mission_id.to_string(),
                observed: self.mission_id.clone(),
            });
        }

        let rule = mission_transition_rule(self.from_state, self.to_state).ok_or(
            MissionContractError::IllegalTransition {
                from: self.from_state,
                to: self.to_state,
            },
        )?;
        if self.role != rule.role {
            return Err(MissionContractError::WrongRole {
                expected: rule.role,
                observed: self.role,
            });
        }
        if self.source != rule.source {
            return Err(MissionContractError::WrongSource {
                expected: rule.source,
                observed: self.source,
            });
        }

        match context.head {
            None => {
                ensure_head(None, self.expected_head_id.as_deref())?;
                if self.from_state.is_some() {
                    return Err(MissionContractError::StateMismatch {
                        expected: None,
                        observed: self.from_state,
                    });
                }
                if self.iteration_id != 1 {
                    return Err(MissionContractError::StaleIteration {
                        expected: 1,
                        observed: self.iteration_id,
                    });
                }
            }
            Some(head) => {
                require_non_empty("context.head_id", head.head_id)?;
                require_iteration(head.iteration_id)?;
                require_digest("context.packet_digest", head.packet_digest)?;
                ensure_head(Some(head.head_id), self.expected_head_id.as_deref())?;
                if self.from_state != Some(head.state) {
                    return Err(MissionContractError::StateMismatch {
                        expected: Some(head.state),
                        observed: self.from_state,
                    });
                }
                let expected_iteration = match rule.iteration {
                    IterationRule::Initialize => 1,
                    IterationRule::Preserve => head.iteration_id,
                    IterationRule::Advance => head.iteration_id.checked_add(1).ok_or(
                        MissionContractError::IterationOverflow {
                            current: head.iteration_id,
                        },
                    )?,
                };
                if self.iteration_id != expected_iteration {
                    return Err(MissionContractError::StaleIteration {
                        expected: expected_iteration,
                        observed: self.iteration_id,
                    });
                }
                match rule.iteration {
                    IterationRule::Advance if self.packet_digest == head.packet_digest => {
                        return Err(MissionContractError::PacketDigestNotAdvanced)
                    }
                    IterationRule::Preserve if self.packet_digest != head.packet_digest => {
                        return Err(MissionContractError::PacketDigestMismatch)
                    }
                    IterationRule::Initialize
                    | IterationRule::Preserve
                    | IterationRule::Advance => {}
                }
            }
        }

        Ok(MissionTransitionValidation {
            rule,
            intent_digest: expected_intent_digest,
            integrity: MissionIntegrityDisposition::OpaqueSignaturePresentUnverified,
        })
    }
}

impl ExecutionDispatchV1 {
    pub fn compute_dispatch_digest(&self) -> Result<String, CanonicalError> {
        digest_contract(EXECUTION_DISPATCH_DIGEST_DOMAIN, self, "dispatch_digest")
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.dispatch_digest = self.compute_dispatch_digest()?;
        Ok(())
    }

    pub fn validate(
        &self,
        now_ms: u64,
    ) -> Result<ContractStructuralValidation, MissionContractError> {
        require_schema(
            "execution dispatch",
            &self.schema,
            EXECUTION_DISPATCH_SCHEMA,
        )?;
        for (field, value) in [
            ("execution_id", self.execution_id.as_str()),
            ("brain_id", self.brain_id.as_str()),
            ("mission_id", self.mission_id.as_str()),
            ("mission_head_id", self.mission_head_id.as_str()),
            ("runner_id", self.runner_id.as_str()),
            ("idempotency_key", self.idempotency_key.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        require_iteration(self.iteration_id)?;
        require_digest("packet_digest", &self.packet_digest)?;
        require_digest("dispatch_digest", &self.dispatch_digest)?;
        validate_signature_binding(&self.issuer, &self.key_id, &self.algorithm, &self.signature)?;
        validate_expiring_record(
            "execution dispatch",
            self.issued_at,
            self.deadline_at,
            now_ms,
        )?;
        let expected = self.compute_dispatch_digest()?;
        ensure_digest("dispatch_digest", &expected, &self.dispatch_digest)?;
        Ok(unverified_validation(expected))
    }
}

impl ExecutionDispatchAckV1 {
    pub fn compute_ack_digest(&self) -> Result<String, CanonicalError> {
        digest_contract(EXECUTION_DISPATCH_ACK_DIGEST_DOMAIN, self, "ack_digest")
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.ack_digest = self.compute_ack_digest()?;
        Ok(())
    }

    pub fn validate_against(
        &self,
        dispatch: &ExecutionDispatchV1,
    ) -> Result<ContractStructuralValidation, MissionContractError> {
        require_schema(
            "execution dispatch ACK",
            &self.schema,
            EXECUTION_DISPATCH_ACK_SCHEMA,
        )?;
        for (field, value) in [
            ("ack_id", self.ack_id.as_str()),
            ("execution_id", self.execution_id.as_str()),
            ("brain_id", self.brain_id.as_str()),
            ("mission_id", self.mission_id.as_str()),
            ("mission_head_id", self.mission_head_id.as_str()),
            ("runner_id", self.runner_id.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        require_iteration(self.iteration_id)?;
        require_digest("dispatch_digest", &self.dispatch_digest)?;
        require_digest("ack_digest", &self.ack_digest)?;
        validate_signature_binding(&self.issuer, &self.key_id, &self.algorithm, &self.signature)?;

        if !matches!(
            dispatch.state,
            ExecutionDispatchState::Intent | ExecutionDispatchState::Acked
        ) {
            return Err(MissionContractError::DispatchNotAccepting {
                state: dispatch.state,
            });
        }
        if self.accepted_at < dispatch.issued_at || self.accepted_at >= dispatch.deadline_at {
            return Err(MissionContractError::InvalidTimeOrder {
                record: "execution dispatch ACK",
                start: dispatch.issued_at,
                end: self.accepted_at,
            });
        }
        let expected_dispatch_digest = dispatch.compute_dispatch_digest()?;
        ensure_digest(
            "dispatch_digest",
            &expected_dispatch_digest,
            &self.dispatch_digest,
        )?;
        bind_dispatch_fields(
            "execution dispatch ACK",
            &self.execution_id,
            &self.brain_id,
            &self.mission_id,
            &self.mission_head_id,
            self.iteration_id,
            &self.runner_id,
            dispatch,
        )?;
        if self.issuer != self.runner_id {
            return Err(MissionContractError::BindingMismatch {
                contract: "execution dispatch ACK",
                field: "issuer",
            });
        }

        let expected = self.compute_ack_digest()?;
        ensure_digest("ack_digest", &expected, &self.ack_digest)?;
        Ok(unverified_validation(expected))
    }
}

impl ExecutionResultV1 {
    pub fn compute_result_digest(&self) -> Result<String, CanonicalError> {
        digest_contract(EXECUTION_RESULT_DIGEST_DOMAIN, self, "result_digest")
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.result_digest = self.compute_result_digest()?;
        Ok(())
    }

    /// Complete sealed result including `result_digest`, excluding only the
    /// signature. The digest was computed without digest/signature, so the
    /// signed subset is explicit and non-circular.
    pub fn canonical_signature_payload(&self) -> Result<Vec<u8>, CanonicalError> {
        canonical_signature_payload(self)
    }

    pub const fn expected_transition(&self) -> MissionState {
        match self.outcome {
            ExecutionOutcome::Succeeded => MissionState::Gate,
            ExecutionOutcome::Failed => MissionState::Failed,
        }
    }

    pub fn validate_against(
        &self,
        dispatch: &ExecutionDispatchV1,
        context: MissionHeadContext<'_>,
    ) -> Result<ContractStructuralValidation, MissionContractError> {
        require_schema("execution result", &self.schema, EXECUTION_RESULT_SCHEMA)?;
        for (field, value) in [
            ("result_id", self.result_id.as_str()),
            ("execution_id", self.execution_id.as_str()),
            ("brain_id", self.brain_id.as_str()),
            ("mission_id", self.mission_id.as_str()),
            ("mission_head_id", self.mission_head_id.as_str()),
            ("runner_id", self.runner_id.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        require_iteration(self.iteration_id)?;
        require_digest("dispatch_digest", &self.dispatch_digest)?;
        require_digest("log_digest", &self.log_digest)?;
        require_optional_digest(
            "failure_artifact_digest",
            self.failure_artifact_digest.as_deref(),
        )?;
        require_digest("result_digest", &self.result_digest)?;
        validate_signature_binding(&self.issuer, &self.key_id, &self.algorithm, &self.signature)?;
        if self.command.first().is_none_or(|value| value.is_empty()) {
            return Err(MissionContractError::EmptyExecutionCommand);
        }
        if self.ended_at < self.started_at {
            return Err(MissionContractError::InvalidTimeOrder {
                record: "execution result",
                start: self.started_at,
                end: self.ended_at,
            });
        }
        match self.outcome {
            ExecutionOutcome::Succeeded
                if self.exit_status != Some(0) || self.failure_artifact_digest.is_some() =>
            {
                return Err(MissionContractError::InvalidSuccessfulExecution)
            }
            ExecutionOutcome::Failed
                if self.exit_status == Some(0) || self.failure_artifact_digest.is_none() =>
            {
                return Err(MissionContractError::InvalidFailedExecution)
            }
            ExecutionOutcome::Succeeded | ExecutionOutcome::Failed => {}
        }

        let expected_dispatch_digest = dispatch.compute_dispatch_digest()?;
        ensure_digest(
            "dispatch_digest",
            &expected_dispatch_digest,
            &self.dispatch_digest,
        )?;
        bind_execution_dispatch_fields(
            "execution result",
            &self.execution_id,
            &self.brain_id,
            &self.mission_id,
            self.iteration_id,
            &self.runner_id,
            dispatch,
        )?;
        if self.issuer != self.runner_id {
            return Err(MissionContractError::BindingMismatch {
                contract: "execution result",
                field: "issuer",
            });
        }
        if self.brain_id != context.brain_id {
            return Err(MissionContractError::BrainMismatch {
                expected: context.brain_id.to_string(),
                observed: self.brain_id.clone(),
            });
        }
        if self.mission_id != context.mission_id {
            return Err(MissionContractError::MissionMismatch {
                expected: context.mission_id.to_string(),
                observed: self.mission_id.clone(),
            });
        }
        let head = context.head.ok_or(MissionContractError::StaleHead {
            expected: None,
            observed: Some(self.mission_head_id.clone()),
        })?;
        ensure_head(Some(head.head_id), Some(&self.mission_head_id))?;
        if head.state != MissionState::Executing {
            return Err(MissionContractError::StateMismatch {
                expected: Some(MissionState::Executing),
                observed: Some(head.state),
            });
        }
        if self.iteration_id != head.iteration_id {
            return Err(MissionContractError::StaleIteration {
                expected: head.iteration_id,
                observed: self.iteration_id,
            });
        }
        if dispatch.packet_digest != head.packet_digest {
            return Err(MissionContractError::PacketDigestMismatch);
        }

        let expected = self.compute_result_digest()?;
        ensure_digest("result_digest", &expected, &self.result_digest)?;
        Ok(unverified_validation(expected))
    }
}

impl ReviewResultV1 {
    pub fn compute_result_digest(&self) -> Result<String, CanonicalError> {
        digest_contract(REVIEW_RESULT_DIGEST_DOMAIN, self, "result_digest")
    }

    pub fn seal(&mut self) -> Result<(), CanonicalError> {
        self.result_digest = self.compute_result_digest()?;
        Ok(())
    }

    /// Complete sealed review including `result_digest`, excluding only the
    /// signature. The digest was computed without digest/signature, so the
    /// signed subset is explicit and non-circular.
    pub fn canonical_signature_payload(&self) -> Result<Vec<u8>, CanonicalError> {
        canonical_signature_payload(self)
    }

    pub fn expected_transition(&self) -> Result<MissionState, MissionContractError> {
        match (self.reviewed_state, self.decision) {
            (MissionState::Judging, ReviewDecision::Approve) => Ok(MissionState::Dispatching),
            (MissionState::Judging | MissionState::Review, ReviewDecision::Change) => {
                Ok(MissionState::Revising)
            }
            (MissionState::Judging | MissionState::Review, ReviewDecision::Reject) => {
                Ok(MissionState::Failed)
            }
            (MissionState::Review, ReviewDecision::Approve) => Ok(MissionState::MergeWait),
            (state, _) => Err(MissionContractError::InvalidReviewedState { state }),
        }
    }

    pub fn validate_against_head(
        &self,
        context: MissionHeadContext<'_>,
        now_ms: u64,
    ) -> Result<ContractStructuralValidation, MissionContractError> {
        require_schema("review result", &self.schema, REVIEW_RESULT_SCHEMA)?;
        for (field, value) in [
            ("result_id", self.result_id.as_str()),
            ("brain_id", self.brain_id.as_str()),
            ("mission_id", self.mission_id.as_str()),
            ("mission_head_id", self.mission_head_id.as_str()),
            ("reviewer_id", self.reviewer_id.as_str()),
        ] {
            require_non_empty(field, value)?;
        }
        require_iteration(self.iteration_id)?;
        require_digest("packet_digest", &self.packet_digest)?;
        require_digest("verdict_digest", &self.verdict_digest)?;
        require_optional_digest(
            "binding_changes_digest",
            self.binding_changes_digest.as_deref(),
        )?;
        require_optional_digest("gate_digest", self.gate_digest.as_deref())?;
        require_optional_digest("candidate_digest", self.candidate_digest.as_deref())?;
        require_digest("result_digest", &self.result_digest)?;
        validate_signature_binding(&self.issuer, &self.key_id, &self.algorithm, &self.signature)?;
        if self.issuer != self.reviewer_id {
            return Err(MissionContractError::BindingMismatch {
                contract: "review result",
                field: "issuer",
            });
        }
        validate_issued_at("review result", self.issued_at, now_ms)?;
        self.expected_transition()?;
        validate_review_shape(self)?;

        if self.brain_id != context.brain_id {
            return Err(MissionContractError::BrainMismatch {
                expected: context.brain_id.to_string(),
                observed: self.brain_id.clone(),
            });
        }
        if self.mission_id != context.mission_id {
            return Err(MissionContractError::MissionMismatch {
                expected: context.mission_id.to_string(),
                observed: self.mission_id.clone(),
            });
        }
        let head = context.head.ok_or(MissionContractError::StaleHead {
            expected: None,
            observed: Some(self.mission_head_id.clone()),
        })?;
        ensure_head(Some(head.head_id), Some(&self.mission_head_id))?;
        if self.reviewed_state != head.state {
            return Err(MissionContractError::StateMismatch {
                expected: Some(head.state),
                observed: Some(self.reviewed_state),
            });
        }
        if self.iteration_id != head.iteration_id {
            return Err(MissionContractError::StaleIteration {
                expected: head.iteration_id,
                observed: self.iteration_id,
            });
        }
        if self.packet_digest != head.packet_digest {
            return Err(MissionContractError::PacketDigestMismatch);
        }

        let expected = self.compute_result_digest()?;
        ensure_digest("result_digest", &expected, &self.result_digest)?;
        Ok(unverified_validation(expected))
    }
}

fn validate_review_shape(result: &ReviewResultV1) -> Result<(), MissionContractError> {
    let require = |field, present| {
        if present {
            Ok(())
        } else {
            Err(MissionContractError::MissingReviewField { field })
        }
    };
    let forbid = |field, present| {
        if present {
            Err(MissionContractError::ForbiddenReviewField { field })
        } else {
            Ok(())
        }
    };

    match (result.reviewed_state, result.decision) {
        (MissionState::Judging, ReviewDecision::Approve | ReviewDecision::Reject) => {
            forbid(
                "binding_changes_digest",
                result.binding_changes_digest.is_some(),
            )?;
            forbid("gate_digest", result.gate_digest.is_some())?;
            forbid("candidate_digest", result.candidate_digest.is_some())
        }
        (MissionState::Review, ReviewDecision::Approve) => {
            forbid(
                "binding_changes_digest",
                result.binding_changes_digest.is_some(),
            )?;
            require("gate_digest", result.gate_digest.is_some())?;
            require("candidate_digest", result.candidate_digest.is_some())
        }
        (MissionState::Review, ReviewDecision::Reject) => {
            forbid(
                "binding_changes_digest",
                result.binding_changes_digest.is_some(),
            )?;
            forbid("gate_digest", result.gate_digest.is_some())?;
            forbid("candidate_digest", result.candidate_digest.is_some())
        }
        (MissionState::Judging | MissionState::Review, ReviewDecision::Change) => {
            require(
                "binding_changes_digest",
                result.binding_changes_digest.is_some(),
            )?;
            forbid("gate_digest", result.gate_digest.is_some())?;
            forbid("candidate_digest", result.candidate_digest.is_some())
        }
        (state, _) => Err(MissionContractError::InvalidReviewedState { state }),
    }
}

#[allow(clippy::too_many_arguments)]
fn bind_dispatch_fields(
    contract: &'static str,
    execution_id: &str,
    brain_id: &str,
    mission_id: &str,
    mission_head_id: &str,
    iteration_id: u64,
    runner_id: &str,
    dispatch: &ExecutionDispatchV1,
) -> Result<(), MissionContractError> {
    for (field, matches) in [
        ("execution_id", execution_id == dispatch.execution_id),
        ("brain_id", brain_id == dispatch.brain_id),
        ("mission_id", mission_id == dispatch.mission_id),
        (
            "mission_head_id",
            mission_head_id == dispatch.mission_head_id,
        ),
        ("iteration_id", iteration_id == dispatch.iteration_id),
        ("runner_id", runner_id == dispatch.runner_id),
    ] {
        if !matches {
            return Err(MissionContractError::BindingMismatch { contract, field });
        }
    }
    Ok(())
}

fn bind_execution_dispatch_fields(
    contract: &'static str,
    execution_id: &str,
    brain_id: &str,
    mission_id: &str,
    iteration_id: u64,
    runner_id: &str,
    dispatch: &ExecutionDispatchV1,
) -> Result<(), MissionContractError> {
    for (field, matches) in [
        ("execution_id", execution_id == dispatch.execution_id),
        ("brain_id", brain_id == dispatch.brain_id),
        ("mission_id", mission_id == dispatch.mission_id),
        ("iteration_id", iteration_id == dispatch.iteration_id),
        ("runner_id", runner_id == dispatch.runner_id),
    ] {
        if !matches {
            return Err(MissionContractError::BindingMismatch { contract, field });
        }
    }
    Ok(())
}

fn require_schema(
    contract: &'static str,
    actual: &str,
    expected: &'static str,
) -> Result<(), MissionContractError> {
    if actual == expected {
        Ok(())
    } else {
        Err(MissionContractError::Schema {
            contract,
            actual: actual.to_string(),
        })
    }
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), MissionContractError> {
    if value.is_empty() {
        Err(MissionContractError::EmptyRequired { field })
    } else {
        Ok(())
    }
}

fn require_optional_non_empty(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), MissionContractError> {
    if value.is_some_and(str::is_empty) {
        Err(MissionContractError::EmptyRequired { field })
    } else {
        Ok(())
    }
}

fn require_iteration(iteration_id: u64) -> Result<(), MissionContractError> {
    if iteration_id == 0 {
        Err(MissionContractError::InvalidIteration)
    } else {
        Ok(())
    }
}

fn require_digest(field: &'static str, value: &str) -> Result<(), MissionContractError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(MissionContractError::InvalidDigest { field })
    }
}

fn require_optional_digest(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), MissionContractError> {
    if let Some(value) = value {
        require_digest(field, value)?;
    }
    Ok(())
}

fn validate_signature_binding(
    issuer: &str,
    key_id: &str,
    algorithm: &str,
    signature: &OpaqueSignature,
) -> Result<(), MissionContractError> {
    require_non_empty("issuer", issuer)?;
    require_non_empty("key_id", key_id)?;
    require_non_empty("algorithm", algorithm)?;
    if signature.is_empty() {
        return Err(MissionContractError::EmptyOpaqueSignature);
    }
    Ok(())
}

fn validate_issued_at(
    record: &'static str,
    issued_at: u64,
    now_ms: u64,
) -> Result<(), MissionContractError> {
    let latest_allowed = now_ms.saturating_add(DEFAULT_MISSION_CLOCK_SKEW_MS);
    if issued_at > latest_allowed {
        Err(MissionContractError::IssuedInFuture {
            record,
            issued_at,
            latest_allowed,
        })
    } else {
        Ok(())
    }
}

fn validate_expiring_record(
    record: &'static str,
    issued_at: u64,
    expires_at: u64,
    now_ms: u64,
) -> Result<(), MissionContractError> {
    if expires_at <= issued_at {
        return Err(MissionContractError::InvalidTimeOrder {
            record,
            start: issued_at,
            end: expires_at,
        });
    }
    validate_issued_at(record, issued_at, now_ms)?;
    if now_ms >= expires_at {
        return Err(MissionContractError::Expired {
            record,
            expires_at,
            now_ms,
        });
    }
    Ok(())
}

fn ensure_digest(
    field: &'static str,
    expected: &str,
    observed: &str,
) -> Result<(), MissionContractError> {
    if expected == observed {
        Ok(())
    } else {
        Err(MissionContractError::DigestMismatch {
            field,
            expected: expected.to_string(),
            observed: observed.to_string(),
        })
    }
}

fn ensure_head(expected: Option<&str>, observed: Option<&str>) -> Result<(), MissionContractError> {
    if expected == observed {
        Ok(())
    } else {
        Err(MissionContractError::StaleHead {
            expected: expected.map(str::to_string),
            observed: observed.map(str::to_string),
        })
    }
}

fn unverified_validation(canonical_digest: String) -> ContractStructuralValidation {
    ContractStructuralValidation {
        canonical_digest,
        integrity: MissionIntegrityDisposition::OpaqueSignaturePresentUnverified,
    }
}

fn digest_contract<T: Serialize + ?Sized>(
    domain: &str,
    value: &T,
    digest_field: &str,
) -> Result<String, CanonicalError> {
    let mut value = serde_json::to_value(value)?;
    let object = value
        .as_object_mut()
        .expect("mission contracts always serialize as JSON objects");
    object.remove(digest_field);
    object.remove("signature");
    digest_canonical(domain, &value)
}

fn canonical_signature_payload<T: Serialize + ?Sized>(
    value: &T,
) -> Result<Vec<u8>, CanonicalError> {
    let mut value = serde_json::to_value(value)?;
    let object = value
        .as_object_mut()
        .expect("mission contracts always serialize as JSON objects");
    object.remove("signature");
    canonical_json(&value)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};

    use super::*;

    const NOW: u64 = 1_500;
    const ISSUED: u64 = 1_000;
    const DEADLINE: u64 = 2_000;

    fn hash(byte: char) -> String {
        byte.to_string().repeat(64)
    }

    fn opaque_signature() -> OpaqueSignature {
        OpaqueSignature::new("opaque-signature-bytes")
    }

    fn payload() -> Value {
        json!({"kind": "fixture", "value": 7})
    }

    fn head_context<'a>(
        state: MissionState,
        iteration_id: u64,
        packet_digest: &'a str,
    ) -> MissionHeadContext<'a> {
        MissionHeadContext {
            brain_id: "brain-1",
            mission_id: "mission-1",
            head: Some(MissionHeadSnapshot {
                head_id: "head-1",
                state,
                iteration_id,
                packet_digest,
            }),
        }
    }

    fn empty_context() -> MissionHeadContext<'static> {
        MissionHeadContext {
            brain_id: "brain-1",
            mission_id: "mission-1",
            head: None,
        }
    }

    fn transition_fixture(rule: MissionTransitionRule) -> MissionTransitionIntentV1 {
        let packet_digest = match rule.iteration {
            IterationRule::Advance => hash('b'),
            IterationRule::Initialize | IterationRule::Preserve => hash('a'),
        };
        let mut intent = MissionTransitionIntentV1 {
            schema: MISSION_TRANSITION_INTENT_SCHEMA.to_string(),
            transition_id: "transition-1".to_string(),
            brain_id: "brain-1".to_string(),
            mission_id: "mission-1".to_string(),
            expected_head_id: rule.from.map(|_| "head-1".to_string()),
            from_state: rule.from,
            to_state: rule.to,
            iteration_id: match rule.iteration {
                IterationRule::Initialize | IterationRule::Preserve => 1,
                IterationRule::Advance => 2,
            },
            actor_id: "actor-1".to_string(),
            role: rule.role,
            source: rule.source,
            source_digest: hash('c'),
            capability_id: "capability-1".to_string(),
            packet_digest,
            payload_digest: MissionTransitionIntentV1::payload_digest_for(&payload()).unwrap(),
            idempotency_key: "idempotency-1".to_string(), // gitleaks:allow
            causation_id: None,
            issued_at: ISSUED,
            expires_at: DEADLINE,
            issuer: "owner-1".to_string(),
            key_id: "key-1".to_string(),
            algorithm: "opaque-test-algorithm".to_string(),
            intent_digest: hash('0'),
            signature: opaque_signature(),
        };
        intent.seal().unwrap();
        intent
    }

    fn context_for_rule(rule: MissionTransitionRule) -> MissionHeadContext<'static> {
        match rule.from {
            None => empty_context(),
            Some(state) => head_context(state, 1, concat_hash_a()),
        }
    }

    fn concat_hash_a() -> &'static str {
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    }

    fn dispatch_fixture() -> ExecutionDispatchV1 {
        let mut dispatch = ExecutionDispatchV1 {
            schema: EXECUTION_DISPATCH_SCHEMA.to_string(),
            execution_id: "execution-1".to_string(),
            brain_id: "brain-1".to_string(),
            mission_id: "mission-1".to_string(),
            mission_head_id: "head-1".to_string(),
            iteration_id: 1,
            packet_digest: hash('a'),
            runner_id: "runner-1".to_string(),
            idempotency_key: "dispatch-idempotency-1".to_string(),
            issued_at: ISSUED,
            deadline_at: DEADLINE,
            state: ExecutionDispatchState::Intent,
            issuer: "owner-1".to_string(),
            key_id: "owner-key-1".to_string(),
            algorithm: "opaque-test-algorithm".to_string(),
            dispatch_digest: hash('0'),
            signature: opaque_signature(),
        };
        dispatch.seal().unwrap();
        dispatch
    }

    fn ack_fixture(dispatch: &ExecutionDispatchV1) -> ExecutionDispatchAckV1 {
        let mut ack = ExecutionDispatchAckV1 {
            schema: EXECUTION_DISPATCH_ACK_SCHEMA.to_string(),
            ack_id: "ack-1".to_string(),
            execution_id: dispatch.execution_id.clone(),
            dispatch_digest: dispatch.compute_dispatch_digest().unwrap(),
            brain_id: dispatch.brain_id.clone(),
            mission_id: dispatch.mission_id.clone(),
            mission_head_id: dispatch.mission_head_id.clone(),
            iteration_id: dispatch.iteration_id,
            runner_id: dispatch.runner_id.clone(),
            accepted_at: 1_200,
            deduplicated: false,
            issuer: "runner-1".to_string(),
            key_id: "runner-key-1".to_string(),
            algorithm: "opaque-test-algorithm".to_string(),
            ack_digest: hash('0'),
            signature: opaque_signature(),
        };
        ack.seal().unwrap();
        ack
    }

    fn execution_result_fixture(
        dispatch: &ExecutionDispatchV1,
        outcome: ExecutionOutcome,
    ) -> ExecutionResultV1 {
        let (exit_status, failure_artifact_digest) = match outcome {
            ExecutionOutcome::Succeeded => (Some(0), None),
            ExecutionOutcome::Failed => (Some(1), Some(hash('f'))),
        };
        let mut result = ExecutionResultV1 {
            schema: EXECUTION_RESULT_SCHEMA.to_string(),
            result_id: "execution-result-1".to_string(),
            execution_id: dispatch.execution_id.clone(),
            dispatch_digest: dispatch.compute_dispatch_digest().unwrap(),
            brain_id: dispatch.brain_id.clone(),
            mission_id: dispatch.mission_id.clone(),
            mission_head_id: dispatch.mission_head_id.clone(),
            iteration_id: dispatch.iteration_id,
            runner_id: dispatch.runner_id.clone(),
            outcome,
            command: vec!["cargo".to_string(), "test".to_string()],
            exit_status,
            started_at: 1_250,
            ended_at: 1_400,
            log_digest: hash('d'),
            failure_artifact_digest,
            issuer: "runner-1".to_string(),
            key_id: "runner-key-1".to_string(),
            algorithm: "opaque-test-algorithm".to_string(),
            result_digest: hash('0'),
            signature: opaque_signature(),
        };
        result.seal().unwrap();
        result
    }

    fn review_result_fixture(
        reviewed_state: MissionState,
        decision: ReviewDecision,
    ) -> ReviewResultV1 {
        let (binding_changes_digest, gate_digest, candidate_digest) = match decision {
            ReviewDecision::Change => (Some(hash('b')), None, None),
            ReviewDecision::Approve if reviewed_state == MissionState::Review => {
                (None, Some(hash('d')), Some(hash('c')))
            }
            ReviewDecision::Approve | ReviewDecision::Reject => (None, None, None),
        };
        let mut result = ReviewResultV1 {
            schema: REVIEW_RESULT_SCHEMA.to_string(),
            result_id: "review-result-1".to_string(),
            brain_id: "brain-1".to_string(),
            mission_id: "mission-1".to_string(),
            mission_head_id: "head-1".to_string(),
            iteration_id: 1,
            reviewer_id: "reviewer-1".to_string(),
            reviewed_state,
            packet_digest: hash('a'),
            decision,
            verdict_digest: hash('e'),
            binding_changes_digest,
            gate_digest,
            candidate_digest,
            issued_at: 1_300,
            issuer: "reviewer-1".to_string(),
            key_id: "reviewer-key-1".to_string(),
            algorithm: "opaque-test-algorithm".to_string(),
            result_digest: hash('0'),
            signature: opaque_signature(),
        };
        result.seal().unwrap();
        result
    }

    fn assert_unknown_field_rejected<T: Serialize + DeserializeOwned>(value: &T) {
        let mut value = serde_json::to_value(value).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("unknown_field".to_string(), json!(true));
        assert!(serde_json::from_value::<T>(value).is_err());
    }

    #[test]
    fn transition_table_is_exact_total_and_has_no_duplicate_edge() {
        let expected: BTreeSet<(Option<MissionState>, MissionState)> = [
            (None, MissionState::Judging),
            (None, MissionState::Dispatching),
            (Some(MissionState::Judging), MissionState::Dispatching),
            (Some(MissionState::Judging), MissionState::Revising),
            (Some(MissionState::Judging), MissionState::Failed),
            (Some(MissionState::Revising), MissionState::Judging),
            (Some(MissionState::Revising), MissionState::Dispatching),
            (Some(MissionState::Dispatching), MissionState::Executing),
            (Some(MissionState::Dispatching), MissionState::Failed),
            (Some(MissionState::Executing), MissionState::Gate),
            (Some(MissionState::Executing), MissionState::Failed),
            (Some(MissionState::Gate), MissionState::Review),
            (Some(MissionState::Gate), MissionState::MergeWait),
            (Some(MissionState::Gate), MissionState::Failed),
            (Some(MissionState::Review), MissionState::Revising),
            (Some(MissionState::Review), MissionState::MergeWait),
            (Some(MissionState::Review), MissionState::Failed),
            (Some(MissionState::MergeWait), MissionState::Landed),
            (Some(MissionState::MergeWait), MissionState::Archived),
        ]
        .into_iter()
        .collect();

        let actual: BTreeSet<_> = MISSION_TRANSITION_RULES
            .iter()
            .map(|rule| (rule.from, rule.to))
            .collect();
        assert_eq!(MISSION_TRANSITION_RULES.len(), 19);
        assert_eq!(actual.len(), MISSION_TRANSITION_RULES.len());
        assert_eq!(actual, expected);

        for from in std::iter::once(None).chain(MissionState::ALL.into_iter().map(Some)) {
            for to in MissionState::ALL {
                assert_eq!(
                    mission_transition_rule(from, to).is_some(),
                    expected.contains(&(from, to)),
                    "unexpected table verdict for {from:?} -> {to:?}"
                );
            }
        }
    }

    #[test]
    fn every_legal_edge_validates_with_its_exact_role_source_and_iteration_law() {
        for rule in MISSION_TRANSITION_RULES.iter().copied() {
            let intent = transition_fixture(rule);
            let validation = intent
                .validate(context_for_rule(rule), &payload(), NOW)
                .unwrap_or_else(|error| panic!("{rule:?} should validate: {error}"));
            assert_eq!(validation.rule, rule);
            assert_eq!(
                validation.integrity,
                MissionIntegrityDisposition::OpaqueSignaturePresentUnverified
            );
            assert_eq!(validation.intent_digest, intent.intent_digest);
        }
    }

    #[test]
    fn every_legal_edge_refuses_every_wrong_role_and_wrong_source() {
        for rule in MISSION_TRANSITION_RULES.iter().copied() {
            for role in Role::ALL.into_iter().filter(|role| *role != rule.role) {
                let mut intent = transition_fixture(rule);
                intent.role = role;
                intent.seal().unwrap();
                assert!(matches!(
                    intent.validate(context_for_rule(rule), &payload(), NOW),
                    Err(MissionContractError::WrongRole { .. })
                ));
            }

            let mut intent = transition_fixture(rule);
            intent.source = match rule.source {
                MissionTransitionSource::MissionServiceDecision => {
                    MissionTransitionSource::AuthorProposal
                }
                _ => MissionTransitionSource::MissionServiceDecision,
            };
            intent.seal().unwrap();
            assert!(matches!(
                intent.validate(context_for_rule(rule), &payload(), NOW),
                Err(MissionContractError::WrongSource { .. })
            ));
        }
    }

    #[test]
    fn landed_failed_and_archived_are_terminal_without_hidden_edges() {
        for state in MissionState::ALL {
            let expected_terminal = matches!(
                state,
                MissionState::Landed | MissionState::Failed | MissionState::Archived
            );
            assert_eq!(state.is_terminal(), expected_terminal);
            if expected_terminal {
                for target in MissionState::ALL {
                    assert!(mission_transition_rule(Some(state), target).is_none());
                }
            }
        }
    }

    #[test]
    fn illegal_edges_are_refused_even_when_the_intent_is_otherwise_well_formed() {
        let illegal = [
            (Some(MissionState::Landed), MissionState::Executing),
            (Some(MissionState::Failed), MissionState::Landed),
            (Some(MissionState::Archived), MissionState::Judging),
            (Some(MissionState::Judging), MissionState::Landed),
            (Some(MissionState::Executing), MissionState::MergeWait),
        ];
        for (from, to) in illegal {
            let mut intent = transition_fixture(rule(
                from,
                to,
                Role::MissionService,
                MissionTransitionSource::MissionServiceDecision,
                IterationRule::Preserve,
            ));
            intent.seal().unwrap();
            let context = head_context(from.unwrap(), 1, concat_hash_a());
            assert!(matches!(
                intent.validate(context, &payload(), NOW),
                Err(MissionContractError::IllegalTransition {
                    from: observed_from,
                    to: observed_to,
                }) if observed_from == from && observed_to == to
            ));
        }
    }

    #[test]
    fn transition_refuses_stale_head_brain_mission_state_iteration_and_packet() {
        let rule =
            mission_transition_rule(Some(MissionState::Executing), MissionState::Gate).unwrap();
        let context = context_for_rule(rule);

        let mut intent = transition_fixture(rule);
        intent.expected_head_id = Some("stale-head".to_string());
        intent.seal().unwrap();
        assert!(matches!(
            intent.validate(context, &payload(), NOW),
            Err(MissionContractError::StaleHead { .. })
        ));

        let mut intent = transition_fixture(rule);
        intent.brain_id = "other-brain".to_string();
        intent.seal().unwrap();
        assert!(matches!(
            intent.validate(context, &payload(), NOW),
            Err(MissionContractError::BrainMismatch { .. })
        ));

        let mut intent = transition_fixture(rule);
        intent.mission_id = "other-mission".to_string();
        intent.seal().unwrap();
        assert!(matches!(
            intent.validate(context, &payload(), NOW),
            Err(MissionContractError::MissionMismatch { .. })
        ));

        let intent = transition_fixture(rule);
        let wrong_state = head_context(MissionState::Dispatching, 1, concat_hash_a());
        assert!(matches!(
            intent.validate(wrong_state, &payload(), NOW),
            Err(MissionContractError::StateMismatch { .. })
        ));

        let mut intent = transition_fixture(rule);
        intent.iteration_id = 2;
        intent.seal().unwrap();
        assert!(matches!(
            intent.validate(context, &payload(), NOW),
            Err(MissionContractError::StaleIteration {
                expected: 1,
                observed: 2
            })
        ));

        let mut intent = transition_fixture(rule);
        intent.packet_digest = hash('b');
        intent.seal().unwrap();
        assert!(matches!(
            intent.validate(context, &payload(), NOW),
            Err(MissionContractError::PacketDigestMismatch)
        ));
    }

    #[test]
    fn revising_requires_next_iteration_and_a_new_packet_digest() {
        let rule =
            mission_transition_rule(Some(MissionState::Revising), MissionState::Judging).unwrap();
        let context = context_for_rule(rule);

        let mut stale_iteration = transition_fixture(rule);
        stale_iteration.iteration_id = 1;
        stale_iteration.seal().unwrap();
        assert!(matches!(
            stale_iteration.validate(context, &payload(), NOW),
            Err(MissionContractError::StaleIteration {
                expected: 2,
                observed: 1
            })
        ));

        let mut same_packet = transition_fixture(rule);
        same_packet.packet_digest = hash('a');
        same_packet.seal().unwrap();
        assert!(matches!(
            same_packet.validate(context, &payload(), NOW),
            Err(MissionContractError::PacketDigestNotAdvanced)
        ));
    }

    #[test]
    fn opening_edges_require_empty_head_and_iteration_one() {
        let rule = mission_transition_rule(None, MissionState::Judging).unwrap();

        let mut wrong_iteration = transition_fixture(rule);
        wrong_iteration.iteration_id = 2;
        wrong_iteration.seal().unwrap();
        assert!(matches!(
            wrong_iteration.validate(empty_context(), &payload(), NOW),
            Err(MissionContractError::StaleIteration {
                expected: 1,
                observed: 2
            })
        ));

        let mut stale_head = transition_fixture(rule);
        stale_head.expected_head_id = Some("head-1".to_string());
        stale_head.seal().unwrap();
        assert!(matches!(
            stale_head.validate(empty_context(), &payload(), NOW),
            Err(MissionContractError::StaleHead { .. })
        ));
    }

    #[test]
    fn transition_payload_and_intent_are_canonically_bound() {
        let rule =
            mission_transition_rule(Some(MissionState::Executing), MissionState::Gate).unwrap();
        let context = context_for_rule(rule);
        let intent = transition_fixture(rule);

        assert!(matches!(
            intent.validate(context, &json!({"different": true}), NOW),
            Err(MissionContractError::DigestMismatch {
                field: "payload_digest",
                ..
            })
        ));

        let mut tampered = intent;
        tampered.actor_id = "other-actor".to_string();
        assert!(matches!(
            tampered.validate(context, &payload(), NOW),
            Err(MissionContractError::DigestMismatch {
                field: "intent_digest",
                ..
            })
        ));
    }

    #[test]
    fn dispatch_ack_and_execution_results_bind_exact_dispatch_identity() {
        let dispatch = dispatch_fixture();
        let execution_head = head_context(MissionState::Executing, 1, concat_hash_a());
        let dispatch_validation = dispatch.validate(NOW).unwrap();
        assert_eq!(
            dispatch_validation.integrity,
            MissionIntegrityDisposition::OpaqueSignaturePresentUnverified
        );

        let ack = ack_fixture(&dispatch);
        assert_eq!(
            ack.validate_against(&dispatch).unwrap().integrity,
            MissionIntegrityDisposition::OpaqueSignaturePresentUnverified
        );

        for field in ["brain", "mission", "head", "iteration", "runner"] {
            let mut stale = ack_fixture(&dispatch);
            match field {
                "brain" => stale.brain_id = "other-brain".to_string(),
                "mission" => stale.mission_id = "other-mission".to_string(),
                "head" => stale.mission_head_id = "other-head".to_string(),
                "iteration" => stale.iteration_id = 2,
                "runner" => stale.runner_id = "other-runner".to_string(),
                _ => unreachable!(),
            }
            stale.seal().unwrap();
            assert!(matches!(
                stale.validate_against(&dispatch),
                Err(MissionContractError::BindingMismatch { .. })
            ));
        }

        let success = execution_result_fixture(&dispatch, ExecutionOutcome::Succeeded);
        assert_eq!(success.expected_transition(), MissionState::Gate);
        success.validate_against(&dispatch, execution_head).unwrap();

        let failure = execution_result_fixture(&dispatch, ExecutionOutcome::Failed);
        assert_eq!(failure.expected_transition(), MissionState::Failed);
        failure.validate_against(&dispatch, execution_head).unwrap();

        let mut stale = success.clone();
        stale.iteration_id = 2;
        stale.seal().unwrap();
        assert!(matches!(
            stale.validate_against(&dispatch, execution_head),
            Err(MissionContractError::BindingMismatch {
                field: "iteration_id",
                ..
            })
        ));
    }

    #[test]
    fn dispatch_digest_binds_state_and_ack_requires_accepting_state_and_deadline() {
        let dispatch = dispatch_fixture();
        let mut tampered = dispatch.clone();
        tampered.state = ExecutionDispatchState::Acked;
        assert!(matches!(
            tampered.validate(NOW),
            Err(MissionContractError::DigestMismatch {
                field: "dispatch_digest",
                ..
            })
        ));

        let mut completed = dispatch.clone();
        completed.state = ExecutionDispatchState::Completed;
        completed.seal().unwrap();
        let mut ack = ack_fixture(&completed);
        ack.dispatch_digest = completed.compute_dispatch_digest().unwrap();
        ack.seal().unwrap();
        assert!(matches!(
            ack.validate_against(&completed),
            Err(MissionContractError::DispatchNotAccepting {
                state: ExecutionDispatchState::Completed
            })
        ));

        let mut late = ack_fixture(&dispatch);
        late.accepted_at = DEADLINE;
        late.seal().unwrap();
        assert!(matches!(
            late.validate_against(&dispatch),
            Err(MissionContractError::InvalidTimeOrder { .. })
        ));
    }

    #[test]
    fn execution_outcome_shape_is_fail_closed() {
        let dispatch = dispatch_fixture();
        let execution_head = head_context(MissionState::Executing, 1, concat_hash_a());

        let mut bad_success = execution_result_fixture(&dispatch, ExecutionOutcome::Succeeded);
        bad_success.exit_status = Some(1);
        bad_success.failure_artifact_digest = Some(hash('f'));
        bad_success.seal().unwrap();
        assert!(matches!(
            bad_success.validate_against(&dispatch, execution_head),
            Err(MissionContractError::InvalidSuccessfulExecution)
        ));

        let mut bad_failure = execution_result_fixture(&dispatch, ExecutionOutcome::Failed);
        bad_failure.failure_artifact_digest = None;
        bad_failure.seal().unwrap();
        assert!(matches!(
            bad_failure.validate_against(&dispatch, execution_head),
            Err(MissionContractError::InvalidFailedExecution)
        ));

        let mut empty_command = execution_result_fixture(&dispatch, ExecutionOutcome::Succeeded);
        empty_command.command.clear();
        empty_command.seal().unwrap();
        assert!(matches!(
            empty_command.validate_against(&dispatch, execution_head),
            Err(MissionContractError::EmptyExecutionCommand)
        ));
    }

    #[test]
    fn all_six_review_result_edges_validate_and_map_to_the_prd_destination() {
        let cases = [
            (
                MissionState::Judging,
                ReviewDecision::Approve,
                MissionState::Dispatching,
            ),
            (
                MissionState::Judging,
                ReviewDecision::Change,
                MissionState::Revising,
            ),
            (
                MissionState::Judging,
                ReviewDecision::Reject,
                MissionState::Failed,
            ),
            (
                MissionState::Review,
                ReviewDecision::Approve,
                MissionState::MergeWait,
            ),
            (
                MissionState::Review,
                ReviewDecision::Change,
                MissionState::Revising,
            ),
            (
                MissionState::Review,
                ReviewDecision::Reject,
                MissionState::Failed,
            ),
        ];

        for (reviewed_state, decision, expected) in cases {
            let result = review_result_fixture(reviewed_state, decision);
            assert_eq!(result.expected_transition().unwrap(), expected);
            let context = head_context(reviewed_state, 1, concat_hash_a());
            assert_eq!(
                result
                    .validate_against_head(context, NOW)
                    .unwrap()
                    .integrity,
                MissionIntegrityDisposition::OpaqueSignaturePresentUnverified
            );
        }
    }

    #[test]
    fn review_result_refuses_wrong_shape_and_stale_bindings() {
        let context = head_context(MissionState::Review, 1, concat_hash_a());

        let mut missing_gate = review_result_fixture(MissionState::Review, ReviewDecision::Approve);
        missing_gate.gate_digest = None;
        missing_gate.seal().unwrap();
        assert!(matches!(
            missing_gate.validate_against_head(context, NOW),
            Err(MissionContractError::MissingReviewField {
                field: "gate_digest"
            })
        ));

        let mut missing_changes =
            review_result_fixture(MissionState::Review, ReviewDecision::Change);
        missing_changes.binding_changes_digest = None;
        missing_changes.seal().unwrap();
        assert!(matches!(
            missing_changes.validate_against_head(context, NOW),
            Err(MissionContractError::MissingReviewField {
                field: "binding_changes_digest"
            })
        ));

        let valid = review_result_fixture(MissionState::Review, ReviewDecision::Approve);

        let mut stale_head = valid.clone();
        stale_head.mission_head_id = "other-head".to_string();
        stale_head.seal().unwrap();
        assert!(matches!(
            stale_head.validate_against_head(context, NOW),
            Err(MissionContractError::StaleHead { .. })
        ));

        let mut stale_iteration = valid.clone();
        stale_iteration.iteration_id = 2;
        stale_iteration.seal().unwrap();
        assert!(matches!(
            stale_iteration.validate_against_head(context, NOW),
            Err(MissionContractError::StaleIteration { .. })
        ));

        let mut stale_packet = valid;
        stale_packet.packet_digest = hash('b');
        stale_packet.seal().unwrap();
        assert!(matches!(
            stale_packet.validate_against_head(context, NOW),
            Err(MissionContractError::PacketDigestMismatch)
        ));
    }

    #[test]
    fn exact_wire_names_and_enum_encodings_are_stable() {
        let transition = transition_fixture(
            mission_transition_rule(Some(MissionState::Review), MissionState::MergeWait).unwrap(),
        );
        let transition_wire = serde_json::to_value(&transition).unwrap();
        assert_eq!(transition_wire["schema"], MISSION_TRANSITION_INTENT_SCHEMA);
        assert_eq!(transition_wire["from_state"], "review");
        assert_eq!(transition_wire["to_state"], "merge_wait");
        assert_eq!(transition_wire["role"], "reviewer");
        assert_eq!(transition_wire["source"], "review_result");
        assert_eq!(transition_wire.as_object().unwrap().len(), 24);

        let dispatch = dispatch_fixture();
        let dispatch_wire = serde_json::to_value(&dispatch).unwrap();
        assert_eq!(dispatch_wire["schema"], EXECUTION_DISPATCH_SCHEMA);
        assert_eq!(dispatch_wire["state"], "INTENT");
        assert_eq!(dispatch_wire.as_object().unwrap().len(), 17);

        let ack = ack_fixture(&dispatch);
        let ack_wire = serde_json::to_value(&ack).unwrap();
        assert_eq!(ack_wire["schema"], EXECUTION_DISPATCH_ACK_SCHEMA);
        assert_eq!(ack_wire["deduplicated"], false);
        assert_eq!(ack_wire.as_object().unwrap().len(), 16);

        let execution = execution_result_fixture(&dispatch, ExecutionOutcome::Succeeded);
        let execution_wire = serde_json::to_value(&execution).unwrap();
        assert_eq!(execution_wire["schema"], EXECUTION_RESULT_SCHEMA);
        assert_eq!(execution_wire["outcome"], "SUCCEEDED");
        assert_eq!(execution_wire.as_object().unwrap().len(), 21);

        let review = review_result_fixture(MissionState::Review, ReviewDecision::Approve);
        let review_wire = serde_json::to_value(&review).unwrap();
        assert_eq!(review_wire["schema"], REVIEW_RESULT_SCHEMA);
        assert_eq!(review_wire["reviewed_state"], "review");
        assert_eq!(review_wire["decision"], "APPROVE");
        assert_eq!(review_wire.as_object().unwrap().len(), 20);
    }

    #[test]
    fn every_versioned_contract_rejects_unknown_wire_fields() {
        let transition = transition_fixture(
            mission_transition_rule(Some(MissionState::Review), MissionState::MergeWait).unwrap(),
        );
        let dispatch = dispatch_fixture();
        let ack = ack_fixture(&dispatch);
        let execution = execution_result_fixture(&dispatch, ExecutionOutcome::Succeeded);
        let review = review_result_fixture(MissionState::Review, ReviewDecision::Approve);

        assert_unknown_field_rejected(&transition);
        assert_unknown_field_rejected(&dispatch);
        assert_unknown_field_rejected(&ack);
        assert_unknown_field_rejected(&execution);
        assert_unknown_field_rejected(&review);
    }

    #[test]
    fn canonical_digest_changes_on_bound_field_mutation_and_not_on_signature_bytes() {
        let dispatch = dispatch_fixture();
        let original = dispatch.compute_dispatch_digest().unwrap();

        let mut changed_packet = dispatch.clone();
        changed_packet.packet_digest = hash('b');
        assert_ne!(original, changed_packet.compute_dispatch_digest().unwrap());

        let mut changed_signature = dispatch;
        changed_signature.signature = OpaqueSignature::new("different-opaque-signature");
        assert_eq!(
            original,
            changed_signature.compute_dispatch_digest().unwrap()
        );
    }

    #[test]
    fn empty_opaque_signature_never_receives_an_authenticated_label() {
        let mut dispatch = dispatch_fixture();
        dispatch.signature = OpaqueSignature::new("");
        assert!(matches!(
            dispatch.validate(NOW),
            Err(MissionContractError::EmptyOpaqueSignature)
        ));

        let valid = dispatch_fixture().validate(NOW).unwrap();
        assert_eq!(
            valid.integrity,
            MissionIntegrityDisposition::OpaqueSignaturePresentUnverified
        );
    }
}
