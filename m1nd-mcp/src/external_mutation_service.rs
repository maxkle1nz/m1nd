//! Closed, action-derived consumer for elevated non-mission mutations.
//!
//! The generic dispatcher remains ORDINARY-only.  This service accepts one
//! strict tagged union over Streamable-HTTP MCP, derives the semantic action and
//! complete effects owner-side, verifies the signed lease receipt, and spends
//! the lease through the broker's external-mutation finalizer.

mod graph_ingest_a2;

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use m1nd_control::{
    digest_canonical, ActionId, AuthorityFloor, AuthorityVariant, Effect, Ingress, RiskClass,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::action_consumers::{external_consumer_contract, TypedConsumerIdV1};
use crate::authority_wal::AuthorityWalRecordCrypto;
use crate::brain_runtime::BrainSessionCell;
use crate::external_mutation_journal::{
    BrainPromoteCheckpointAckV1, BrainPromoteReconciliationReceiptV1,
    ExternalMutationJournalEntryV1, ExternalMutationJournalError, ExternalMutationJournalPhaseV1,
    ExternalMutationJournalV1, ExternalMutationPrepareV1, ExternalMutationPublishedResultV1,
    BRAIN_PROMOTE_CHECKPOINT_ACK_DIGEST_DOMAIN, BRAIN_PROMOTE_CHECKPOINT_ACK_SCHEMA,
    BRAIN_PROMOTE_RECONCILIATION_RECEIPT_SCHEMA, EXTERNAL_MUTATION_PUBLISHED_RESULT_SCHEMA,
};
use crate::mission_service_transport::{
    AuthorityStatusReader, MissionServiceIngressV1, MissionServiceTransportContextV1,
};
use crate::owner_authorization_broker::{
    AuthorizationLeaseStateV1, AuthorizationReservationV1, AuthorizationTerminalKindV1,
    OwnerAuthorityLinearizationV1, OwnerAuthorizationBrokerConfigV1, OwnerAuthorizationBrokerError,
    OwnerAuthorizationBrokerV1, OwnerAuthorizationLeaseV1,
};
use crate::protected_journal_head::SharedProtectedJournalHeadBackendV1;
use crate::runtime_jobs::{
    RuntimeJobAuthorityBindingV1, RuntimeJobBindingV1, RuntimeJobFailure, RuntimeJobRegistry,
    RuntimeJobRequestV1, RuntimeJobState, RuntimeJobSuccess, RuntimeJobWait,
    RUNTIME_JOB_AUTHORITY_SCHEMA, RUNTIME_JOB_BINDING_SCHEMA,
};
use crate::system_blocks::{RatifySummary, SystemBlockStore};
pub use graph_ingest_a2::{GraphIngestA2InputV1, GraphIngestA2ModeV1, GraphIngestA2ParentV1};
use graph_ingest_a2::{
    GraphIngestA2InspectionSnapshotV1, GraphIngestA2RecoveryPayloadV1,
    GraphIngestA2SemanticPayloadV1, InspectedGraphIngestA2V1, StagedGraphIngestA2V1,
};

pub const EXTERNAL_MUTATION_REQUEST_SCHEMA: &str = "m1nd-external-mutation-request-v1";
pub const GRAPH_INGEST_PREVIEW_REQUEST_SCHEMA: &str = "m1nd-graph-ingest-preview-request-v1";
pub const GRAPH_INGEST_PREVIEW_RESPONSE_SCHEMA: &str = "m1nd-graph-ingest-preview-response-v1";
pub const GRAPH_INGEST_PREVIEW_ID_DIGEST_DOMAIN: &str = "m1nd-graph-ingest-preview-id-v1";
pub const EXTERNAL_MUTATION_RESPONSE_SCHEMA: &str = "m1nd-external-mutation-response-v1";
pub const EXTERNAL_MUTATION_REFUSAL_SCHEMA: &str = "m1nd-external-mutation-refusal-v1";
pub const EXTERNAL_MUTATION_OPERATION_OBJECT_SCHEMA: &str =
    "m1nd-external-mutation-operation-object-v1";
pub const EXTERNAL_MUTATION_OPERATION_OBJECT_DIGEST_DOMAIN: &str =
    "m1nd-external-mutation-operation-object-v1";
pub const SYSTEM_BLOCKS_RATIFY_PAYLOAD_SCHEMA: &str =
    "m1nd-system-blocks-ratify-semantic-payload-v1";
pub const SYSTEM_BLOCKS_RATIFY_PAYLOAD_DIGEST_DOMAIN: &str =
    "m1nd-system-blocks-ratify-semantic-payload-v1";
pub const BRAIN_PROMOTE_PAYLOAD_SCHEMA: &str = "m1nd-brain-promote-semantic-payload-v1";
pub const BRAIN_PROMOTE_PAYLOAD_DIGEST_DOMAIN: &str = "m1nd-brain-promote-semantic-payload-v1";
pub const SOURCE_EDIT_COMMIT_REQUEST_SCHEMA: &str = "m1nd-source-edit-commit-request-v1";
const SOURCE_EDIT_CHECKPOINT_ACK_DIGEST_DOMAIN: &str = "m1nd-source-edit-checkpoint-ack-v1";
pub const EXTERNAL_MUTATION_OPERATION_VERSION: u64 = 1;
const GRAPH_INGEST_SCAN_DEADLINE: Duration = Duration::from_secs(30);

type ExternalMutationFaultHookV1 =
    dyn Fn(&'static str) -> Result<(), String> + Send + Sync + 'static;

fn no_external_mutation_fault(_: &'static str) -> Result<(), String> {
    Ok(())
}

fn system_now_unix_ms() -> Result<u64, ExternalMutationError> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            ExternalMutationError::refused("graph_ingest_scan_clock_invalid", error.to_string())
        })?;
    Ok(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
}

fn system_clock_at_or_after(deadline_unix_ms: u64) -> bool {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX) >= deadline_unix_ms)
        .unwrap_or(true)
}

fn fresh_graph_ingest_scan_job_id() -> Result<String, ExternalMutationError> {
    let mut random = [0u8; 24];
    getrandom::fill(&mut random).map_err(|error| {
        ExternalMutationError::refused("graph_ingest_scan_job_id_failed", error.to_string())
    })?;
    Ok(format!("graph-ingest-scan-{}", hex_lower_bytes(&random)))
}

fn hex_lower_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn preview_graph_ingest_scan_binding(
    actor_brain_id: &str,
    transport_session_id: &str,
    ingress_context_digest: &str,
) -> Result<RuntimeJobBindingV1, ExternalMutationError> {
    Ok(RuntimeJobBindingV1 {
        schema: RUNTIME_JOB_BINDING_SCHEMA.to_string(),
        organism_id: "m1nd-owner".to_string(),
        brain_id: actor_brain_id.to_string(),
        mission_id: "none-graph-ingest-preview".to_string(),
        agent_id: transport_session_id.to_string(),
        action: ActionId::new("graph.ingest.preview").map_err(|error| {
            ExternalMutationError::refused("graph_ingest_scan_binding_invalid", error.to_string())
        })?,
        ingress: Ingress::Mcp,
        effects: BTreeSet::from([Effect::Read]),
        authority: RuntimeJobAuthorityBindingV1 {
            schema: RUNTIME_JOB_AUTHORITY_SCHEMA.to_string(),
            decision_id: format!("preview-{ingress_context_digest}"),
            authority_variant: AuthorityVariant::Ordinary,
            authority_epoch: 0,
            autonomy_epoch: 0,
            capability_id: None,
            authorization_digest: ingress_context_digest.to_string(),
        },
    })
}

fn authorized_graph_ingest_scan_binding(
    receipt: &crate::authority_runtime::AuthorityAuthorizationReceiptCoreV1,
    receipt_digest: &str,
) -> Result<RuntimeJobBindingV1, ExternalMutationError> {
    use crate::authority_runtime::AuthorizationAuthorityV1;

    let authority_variant = match &receipt.authority {
        AuthorizationAuthorityV1::OrdinarySession { .. } => AuthorityVariant::Ordinary,
        AuthorizationAuthorityV1::Positive { variant, .. }
        | AuthorizationAuthorityV1::Autonomous { variant, .. } => *variant,
        AuthorizationAuthorityV1::SafetyActuator { .. } => AuthorityVariant::SafetyKernel,
        AuthorizationAuthorityV1::ServiceIdentity { .. } => {
            return Err(ExternalMutationError::refused(
                "graph_ingest_scan_authority_invalid",
                "graph mutation scan cannot be bound to service-identity authority",
            ))
        }
    };
    Ok(RuntimeJobBindingV1 {
        schema: RUNTIME_JOB_BINDING_SCHEMA.to_string(),
        organism_id: receipt.organism_id.clone(),
        brain_id: receipt.brain_id.clone(),
        mission_id: receipt
            .mission_id
            .clone()
            .unwrap_or_else(|| "none-external-mutation".to_string()),
        agent_id: receipt.subject_id.clone(),
        action: receipt.action.clone(),
        ingress: receipt.ingress,
        effects: receipt.complete_effects.clone(),
        authority: RuntimeJobAuthorityBindingV1 {
            schema: RUNTIME_JOB_AUTHORITY_SCHEMA.to_string(),
            decision_id: receipt
                .authority_decision_digest
                .clone()
                .unwrap_or_else(|| receipt.authority_body_digest.clone()),
            authority_variant,
            authority_epoch: receipt.protected_epoch_at_decision,
            autonomy_epoch: receipt.autonomy_epoch,
            capability_id: Some(receipt.capability_id.clone()),
            authorization_digest: receipt_digest.to_string(),
        },
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceEditCommitRequestV1 {
    pub schema: String,
    pub preview_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphIngestPreviewRequestV1 {
    pub schema: String,
    pub request_id: String,
    pub mode: GraphIngestA2ModeV1,
    #[serde(default)]
    pub include_dotfiles: bool,
    #[serde(default)]
    pub dotfile_patterns: Vec<String>,
    pub parent: Option<GraphIngestA2ParentV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphIngestAuthorityBindingV1 {
    pub target_action: String,
    pub payload_digest: String,
    pub requested_effects: BTreeSet<Effect>,
    pub mission_id: Option<String>,
    pub mission_head_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphIngestPreviewResponseV1 {
    pub schema: String,
    pub request_id: String,
    pub preview_id: String,
    pub semantic_action: String,
    pub requested_effects: BTreeSet<Effect>,
    pub authority_floor: AuthorityFloor,
    pub risk_class: RiskClass,
    pub ingress: Ingress,
    pub route_selector: Option<String>,
    pub actor_brain_id: String,
    pub transport_session_id: String,
    pub ingress_context_digest: String,
    pub root_identity: String,
    pub expected_graph_generation: u64,
    pub expected_source_projection_digest: String,
    pub candidate_ownership_digest: String,
    pub candidate_source_projection_digest: String,
    pub candidate_pipeline_digest: String,
    /// Durable RuntimeJobRegistry identity for status/cancel/audit surfaces.
    pub scan_job_id: String,
    pub semantic_payload_digest: String,
    pub operation_object_digest: String,
    pub authority_binding: GraphIngestAuthorityBindingV1,
    pub execute_request: ExternalMutationRequestV1,
}

/// Closed external request.  `request_id` is correlation-only.  There is no
/// caller-selected action, effect set, ingress, ratifier, promoter, or authority
/// identity in any variant.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExternalMutationRequestV1 {
    SystemBlocksRatify {
        schema: String,
        request_id: String,
        expected_store_version: u64,
        #[serde(default)]
        block_ids: Option<Vec<String>>,
    },
    BrainPromote {
        schema: String,
        request_id: String,
        source_brain: String,
        claim: String,
        reason: String,
        expected_source_sha256: String,
        expected_medulla_sha256: Option<String>,
    },
    SourceEditCommit {
        schema: String,
        request_id: String,
        request: SourceEditCommitRequestV1,
    },
    GraphIngestReplace {
        schema: String,
        request_id: String,
        request: GraphIngestA2InputV1,
    },
    GraphIngestMergeExisting {
        schema: String,
        request_id: String,
        request: GraphIngestA2InputV1,
    },
}

impl ExternalMutationRequestV1 {
    pub fn schema(&self) -> &str {
        match self {
            Self::SystemBlocksRatify { schema, .. }
            | Self::BrainPromote { schema, .. }
            | Self::SourceEditCommit { schema, .. }
            | Self::GraphIngestReplace { schema, .. }
            | Self::GraphIngestMergeExisting { schema, .. } => schema,
        }
    }

    pub fn request_id(&self) -> &str {
        match self {
            Self::SystemBlocksRatify { request_id, .. }
            | Self::BrainPromote { request_id, .. }
            | Self::SourceEditCommit { request_id, .. }
            | Self::GraphIngestReplace { request_id, .. }
            | Self::GraphIngestMergeExisting { request_id, .. } => request_id,
        }
    }

    pub const fn semantic_action_id(&self) -> &'static str {
        match self {
            Self::SystemBlocksRatify { .. } => "system_blocks.ratify",
            Self::BrainPromote { .. } => "brain.promote",
            Self::SourceEditCommit { .. } => "source.edit.commit",
            Self::GraphIngestReplace { .. } => "graph.ingest.replace",
            Self::GraphIngestMergeExisting { .. } => "graph.ingest.merge_existing",
        }
    }

    pub fn validate_wire(&self) -> Result<(), ExternalMutationError> {
        if self.schema() != EXTERNAL_MUTATION_REQUEST_SCHEMA || self.request_id().trim().is_empty()
        {
            return Err(ExternalMutationError::refused(
                "invalid_external_mutation_request",
                "strict request schema and non-empty request id are required",
            ));
        }
        match self {
            Self::SystemBlocksRatify {
                expected_store_version,
                block_ids,
                ..
            } => {
                if *expected_store_version == 0
                    || block_ids.as_ref().is_some_and(|ids| {
                        ids.is_empty()
                            || ids.iter().any(|id| id.trim().is_empty())
                            || ids.iter().collect::<BTreeSet<_>>().len() != ids.len()
                    })
                {
                    return Err(ExternalMutationError::refused(
                        "invalid_system_blocks_ratify_request",
                        "positive OCC version and unique non-empty block ids are required",
                    ));
                }
            }
            Self::BrainPromote {
                source_brain,
                claim,
                reason,
                expected_source_sha256,
                expected_medulla_sha256,
                ..
            } => {
                if source_brain.trim().is_empty()
                    || claim.trim().is_empty()
                    || reason.trim().is_empty()
                    || !is_digest(expected_source_sha256)
                    || expected_medulla_sha256
                        .as_deref()
                        .is_some_and(|digest| !is_digest(digest))
                {
                    return Err(ExternalMutationError::refused(
                        "invalid_brain_promote_request",
                        "source, claim, reason, source digest, and optional medulla digest are required",
                    ));
                }
            }
            Self::SourceEditCommit { request, .. } => {
                if request.schema != SOURCE_EDIT_COMMIT_REQUEST_SCHEMA
                    || request.preview_id.trim().is_empty()
                {
                    return Err(ExternalMutationError::refused(
                        "invalid_source_edit_commit_request",
                        "strict source-edit schema and preview id are required",
                    ));
                }
            }
            Self::GraphIngestReplace { request, .. } => {
                request
                    .validate_shape(GraphIngestA2ModeV1::Replace)
                    .map_err(|error| ExternalMutationError::refused(error.code, error.detail))?;
            }
            Self::GraphIngestMergeExisting { request, .. } => {
                request
                    .validate_shape(GraphIngestA2ModeV1::MergeExisting)
                    .map_err(|error| ExternalMutationError::refused(error.code, error.detail))?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct ExternalPromotePathsV1 {
    pub source_store_dir: PathBuf,
    pub medulla_store_dir: PathBuf,
    pub medulla_runtime_root: PathBuf,
}

#[derive(Clone)]
pub(crate) struct ExternalMutationExecutionHostV1 {
    pub(crate) selected_brain: Arc<BrainSessionCell>,
    pub(crate) selected_actor_brain_id: String,
    /// Owner-hosted resolver used by the global recovery barrier. It must
    /// resolve every durable brain id, not only the request's selected brain.
    pub(crate) resolve_brain: Arc<ExternalMutationBrainResolverV1>,
    pub(crate) reconcile_promote: Arc<BrainPromoteReconcilerV1>,
    pub(crate) reconciliation_brain_id: String,
    pub(crate) promote_paths: Option<ExternalPromotePathsV1>,
    /// Owner-global bounded supervisor used for every filesystem scan. The
    /// selected brain remains encoded independently in each job binding.
    pub(crate) runtime_jobs: Result<RuntimeJobRegistry, String>,
}

pub(crate) type ExternalMutationBrainResolverV1 =
    dyn Fn(&str) -> Result<Arc<BrainSessionCell>, String> + Send + Sync + 'static;

pub struct BrainPromoteReconciliationRequestV1 {
    pub operation_id: String,
    pub operation_object_digest: String,
    pub source_brain_id: String,
    pub reconciliation_brain_id: String,
    pub medulla_path: PathBuf,
    pub medulla_postimage_sha256: String,
    pub authority_subject_id: String,
    job: ExternalMutationActorJobV1,
}

impl BrainPromoteReconciliationRequestV1 {
    /// Source edits and graph ingestion execute on the selected source brain;
    /// promotion reconciles on the bound medulla actor. Keeping that routing
    /// decision inside the sealed job prevents the HTTP bridge from guessing
    /// from caller-controlled fields.
    pub(crate) fn runs_on_source_brain_actor(&self) -> bool {
        match &self.job {
            ExternalMutationActorJobV1::Inspect(job) => job.runs_on_source_brain_actor(),
            ExternalMutationActorJobV1::GraphIngestPreview(_) => true,
            ExternalMutationActorJobV1::Ratify(_)
            | ExternalMutationActorJobV1::SourceEdit(_)
            | ExternalMutationActorJobV1::GraphIngest(_)
            | ExternalMutationActorJobV1::Maintenance(_) => true,
            ExternalMutationActorJobV1::Promote(_) => false,
        }
    }

    /// Read-only inspection must not manufacture a durability receipt. Every
    /// mutation, including recovery cleanup, is checkpointed by the same actor
    /// turn that performed it.
    pub(crate) fn requires_checkpoint_ack(&self) -> bool {
        !matches!(
            &self.job,
            ExternalMutationActorJobV1::Inspect(_)
                | ExternalMutationActorJobV1::GraphIngestPreview(_)
        )
    }

    /// Legacy ratify journals predate the durable actor id. They still carry
    /// the canonical source root, so recovery may resolve that exact actor and
    /// let the job revalidate ownership from SessionState. Maintenance jobs are
    /// similarly owner-derived and never accept a caller-selected actor.
    pub(crate) fn allows_resolved_actor_identity(&self) -> bool {
        matches!(&self.job, ExternalMutationActorJobV1::Maintenance(_))
            || matches!(&self.job, ExternalMutationActorJobV1::Ratify(job) if job.reconciliation_brain_id.is_empty())
    }

    pub(crate) fn actor_failure_code(&self) -> &'static str {
        match &self.job {
            ExternalMutationActorJobV1::Inspect(_) => "external_mutation_inspect_actor_job_failed",
            ExternalMutationActorJobV1::GraphIngestPreview(_) => {
                "graph_ingest_preview_actor_job_failed"
            }
            ExternalMutationActorJobV1::Ratify(_) => "system_blocks_ratify_actor_job_failed",
            ExternalMutationActorJobV1::Promote(_) => "brain_promote_actor_job_failed",
            ExternalMutationActorJobV1::GraphIngest(_) => "graph_ingest_a2_actor_job_failed",
            ExternalMutationActorJobV1::SourceEdit(_) => "source_edit_actor_job_failed",
            ExternalMutationActorJobV1::Maintenance(_) => {
                "external_mutation_maintenance_actor_job_failed"
            }
        }
    }

    pub(crate) fn execute(
        self,
        state: &mut crate::session::SessionState,
    ) -> Result<BrainPromoteReconciliationExecutionV1, String> {
        match self.job {
            ExternalMutationActorJobV1::Inspect(job) => (*job).execute(state),
            ExternalMutationActorJobV1::GraphIngestPreview(job) => (*job).execute(state),
            ExternalMutationActorJobV1::Ratify(job) => (*job).execute(state),
            ExternalMutationActorJobV1::Promote(job) => (*job).execute(state),
            ExternalMutationActorJobV1::GraphIngest(job) => (*job).execute(state),
            ExternalMutationActorJobV1::SourceEdit(job) => (*job).execute(state),
            ExternalMutationActorJobV1::Maintenance(job) => (*job).execute(state),
        }
    }
}

enum ExternalMutationActorJobV1 {
    Inspect(Box<ExternalMutationInspectActorJobV1>),
    GraphIngestPreview(Box<GraphIngestPreviewActorJobV1>),
    Ratify(Box<RatifyActorJobV1>),
    Promote(Box<BrainPromoteActorJobV1>),
    GraphIngest(Box<GraphIngestActorJobV1>),
    SourceEdit(Box<SourceEditActorJobV1>),
    Maintenance(Box<ExternalMutationMaintenanceActorJobV1>),
}

pub struct BrainPromoteReconciliationExecutionV1 {
    pub ingest_output: Value,
    pub publish_payload: Value,
    pub graph_generation_before: u64,
    pub graph_generation_after: u64,
    checkpoint_ack: Option<BrainPromoteCheckpointAckV1>,
}

impl BrainPromoteReconciliationExecutionV1 {
    fn empty(graph_generation: u64) -> Self {
        Self {
            ingest_output: Value::Null,
            publish_payload: Value::Null,
            graph_generation_before: graph_generation,
            graph_generation_after: graph_generation,
            checkpoint_ack: None,
        }
    }

    pub(crate) fn bind_checkpoint_ack(
        mut self,
        ack: &crate::checkpoint_store::CheckpointAckV1,
    ) -> Self {
        self.checkpoint_ack = Some(BrainPromoteCheckpointAckV1 {
            schema: ack.schema.clone(),
            checkpoint_id: ack.checkpoint_id.clone(),
            brain_id: ack.brain_id.clone(),
            epoch: ack.epoch,
            generation: ack.generation,
            revision: ack.revision,
            current_pointer_digest: ack.current_pointer_digest.clone(),
            confirmed_at_unix_ms: ack.confirmed_at_unix_ms,
        });
        self
    }
}

pub type BrainPromoteReconcilerV1 = dyn Fn(BrainPromoteReconciliationRequestV1) -> Result<BrainPromoteReconciliationExecutionV1, String>
    + Send
    + Sync
    + 'static;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalMutationResponseV1 {
    pub schema: String,
    pub request_id: String,
    pub semantic_action: String,
    pub semantic_payload_digest: String,
    pub operation_object_digest: String,
    pub authorization_lease_id: String,
    pub authorization_reservation_id: String,
    pub journal_operation_id: String,
    pub outcome_digest: String,
    pub graph_resync_required: bool,
    pub reconciliation_state: String,
    pub result: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalMutationRecoveryReportV1 {
    pub scanned: usize,
    pub broker_recovered: usize,
    pub forward_completed: usize,
    pub already_published: usize,
    pub safely_aborted_pre_finalization: usize,
    pub pending_uncertain: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalMutationConservationReportV1 {
    pub journal_entries: usize,
    pub broker_bound_entries: usize,
    pub prepared: usize,
    pub committed_or_recovery: usize,
    pub published: usize,
    pub anomalies: Vec<String>,
}

enum PreparedRecoveryDispositionV1 {
    Aborted,
    Pending,
    Advanced(Box<ExternalMutationJournalEntryV1>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalMutationRefusalV1 {
    pub schema: String,
    pub request_id: Option<String>,
    pub code: String,
    pub detail: String,
}

#[derive(Debug)]
pub enum ExternalMutationError {
    Refused {
        code: &'static str,
        detail: String,
    },
    Canonical(m1nd_control::CanonicalError),
    Broker(OwnerAuthorizationBrokerError),
    Journal(ExternalMutationJournalError),
    Io {
        operation: &'static str,
        source: std::io::Error,
    },
    Domain(String),
}

impl ExternalMutationError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Refused { code, .. } => code,
            Self::Canonical(_) => "external_mutation_canonicalization_failed",
            Self::Broker(error) => error.code(),
            Self::Journal(error) => error.code(),
            Self::Io { .. } => "external_mutation_io",
            Self::Domain(_) => "external_mutation_domain_refused",
        }
    }

    pub(crate) fn refused(code: &'static str, detail: impl Into<String>) -> Self {
        Self::Refused {
            code,
            detail: detail.into(),
        }
    }

    pub fn to_refusal(&self, request_id: Option<&str>) -> ExternalMutationRefusalV1 {
        ExternalMutationRefusalV1 {
            schema: EXTERNAL_MUTATION_REFUSAL_SCHEMA.to_string(),
            request_id: request_id.map(str::to_string),
            code: self.code().to_string(),
            detail: self.to_string(),
        }
    }
}

impl fmt::Display for ExternalMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused { code, detail } => write!(formatter, "{code}: {detail}"),
            Self::Canonical(error) => write!(formatter, "canonicalization: {error}"),
            Self::Broker(error) => write!(formatter, "authorization broker: {error}"),
            Self::Journal(error) => write!(formatter, "external mutation journal: {error}"),
            Self::Io { operation, source } => write!(formatter, "{operation}: {source}"),
            Self::Domain(detail) => formatter.write_str(detail),
        }
    }
}

impl Error for ExternalMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Canonical(error) => Some(error),
            Self::Broker(error) => Some(error),
            Self::Journal(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<m1nd_control::CanonicalError> for ExternalMutationError {
    fn from(error: m1nd_control::CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

impl From<OwnerAuthorizationBrokerError> for ExternalMutationError {
    fn from(error: OwnerAuthorizationBrokerError) -> Self {
        Self::Broker(error)
    }
}

impl From<ExternalMutationJournalError> for ExternalMutationError {
    fn from(error: ExternalMutationJournalError) -> Self {
        Self::Journal(error)
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ExternalMutationOperationObjectV1<'a> {
    schema: &'static str,
    semantic_action: &'a str,
    ingress: Ingress,
    brain_id: &'a str,
    mission_id: Option<&'a str>,
    mission_head_id: Option<&'a str>,
    operation_version: u64,
    semantic_payload_digest: &'a str,
}

fn external_operation_object_digest(
    semantic_action: &str,
    actor_brain_id: &str,
    semantic_payload_digest: &str,
) -> Result<String, ExternalMutationError> {
    digest_canonical(
        EXTERNAL_MUTATION_OPERATION_OBJECT_DIGEST_DOMAIN,
        &ExternalMutationOperationObjectV1 {
            schema: EXTERNAL_MUTATION_OPERATION_OBJECT_SCHEMA,
            semantic_action,
            ingress: Ingress::Mcp,
            brain_id: actor_brain_id,
            mission_id: None,
            mission_head_id: None,
            operation_version: EXTERNAL_MUTATION_OPERATION_VERSION,
            semantic_payload_digest,
        },
    )
    .map_err(ExternalMutationError::from)
}

fn graph_ingest_preview_id(
    transport_session_id: &str,
    ingress_context_digest: &str,
    route_selector: Option<&str>,
    actor_brain_id: &str,
    operation_object_digest: &str,
) -> Result<String, ExternalMutationError> {
    digest_canonical(
        GRAPH_INGEST_PREVIEW_ID_DIGEST_DOMAIN,
        &(
            GRAPH_INGEST_PREVIEW_RESPONSE_SCHEMA,
            transport_session_id,
            ingress_context_digest,
            route_selector,
            actor_brain_id,
            operation_object_digest,
        ),
    )
    .map_err(ExternalMutationError::from)
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct RatifySemanticPayloadV1<'a> {
    schema: &'static str,
    expected_store_version: u64,
    block_ids: Option<&'a [String]>,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct PromoteSemanticPayloadV1<'a> {
    schema: &'static str,
    source_brain: &'a str,
    claim: &'a str,
    reason: &'a str,
    expected_source_sha256: &'a str,
    expected_medulla_sha256: Option<&'a str>,
}

pub(crate) struct ExternalMutationServiceInputsV1 {
    pub journal_root: PathBuf,
    pub broker_config: OwnerAuthorizationBrokerConfigV1,
    pub linearization: OwnerAuthorityLinearizationV1,
    pub broker_operation: Arc<Mutex<()>>,
    pub current_authority: Arc<AuthorityStatusReader>,
    pub protected_journal_head: SharedProtectedJournalHeadBackendV1,
    pub receipt_crypto: Arc<dyn AuthorityWalRecordCrypto>,
    pub owner_clock: Arc<dyn Fn() -> u64 + Send + Sync>,
}

pub struct ExternalMutationServiceV1 {
    journal_root: PathBuf,
    broker_config: OwnerAuthorizationBrokerConfigV1,
    linearization: OwnerAuthorityLinearizationV1,
    broker_operation: Arc<Mutex<()>>,
    current_authority: Arc<AuthorityStatusReader>,
    protected_journal_head: SharedProtectedJournalHeadBackendV1,
    receipt_crypto: Arc<dyn AuthorityWalRecordCrypto>,
    owner_clock: Arc<dyn Fn() -> u64 + Send + Sync>,
    fault_hook: Arc<ExternalMutationFaultHookV1>,
}

impl ExternalMutationServiceV1 {
    pub(crate) fn from_owner_inputs(inputs: ExternalMutationServiceInputsV1) -> Self {
        Self {
            journal_root: inputs.journal_root,
            broker_config: inputs.broker_config,
            linearization: inputs.linearization,
            broker_operation: inputs.broker_operation,
            current_authority: inputs.current_authority,
            protected_journal_head: inputs.protected_journal_head,
            receipt_crypto: inputs.receipt_crypto,
            owner_clock: inputs.owner_clock,
            fault_hook: Arc::new(no_external_mutation_fault),
        }
    }

    #[cfg(test)]
    fn with_fault_hook_for_test(mut self, fault_hook: Arc<ExternalMutationFaultHookV1>) -> Self {
        self.fault_hook = fault_hook;
        self
    }

    fn hit_fault(&self, point: &'static str) -> Result<(), ExternalMutationError> {
        (self.fault_hook)(point).map_err(|detail| {
            ExternalMutationError::refused(
                "external_mutation_injected_crash",
                format!("{point}: {detail}"),
            )
        })
    }

    fn supervise_graph_ingest_scan(
        &self,
        host: &ExternalMutationExecutionHostV1,
        snapshot: GraphIngestA2InspectionSnapshotV1,
        binding: RuntimeJobBindingV1,
        request_id: &str,
    ) -> Result<(String, InspectedGraphIngestA2V1), ExternalMutationError> {
        let job_id = fresh_graph_ingest_scan_job_id()?;
        let idempotency_key = format!("idem-{job_id}");
        let deadline_unix_ms = system_now_unix_ms()?.saturating_add(
            u64::try_from(GRAPH_INGEST_SCAN_DEADLINE.as_millis()).unwrap_or(u64::MAX),
        );
        let snapshot_revision = snapshot.expected_graph_generation();
        let request = RuntimeJobRequestV1 {
            job_id: job_id.clone(),
            idempotency_key,
            binding,
            snapshot_revision,
            deadline_unix_ms,
        };
        let (result_tx, result_rx) = std::sync::mpsc::sync_channel(1);
        let fault_hook = Arc::clone(&self.fault_hook);
        let runtime_jobs = host.runtime_jobs.as_ref().map_err(|detail| {
            ExternalMutationError::refused(
                "graph_ingest_scan_job_refused",
                format!("runtime job registry is unavailable: {detail}"),
            )
        })?;
        runtime_jobs
            .submit_prepared(
                request,
                move |context| {
                    context.checkpoint()?;
                    (fault_hook)("during_graph_ingest_scan").map_err(|detail| {
                        RuntimeJobFailure::new("graph_ingest_scan_injected_failure", detail)
                    })?;
                    context.checkpoint()?;
                    let inspected = graph_ingest_a2::complete_inspection_off_actor_with_cancel(
                        snapshot,
                        || {
                            context.is_cancelled()
                                || system_clock_at_or_after(context.deadline_unix_ms)
                        },
                    )
                    .map_err(|error| RuntimeJobFailure::new(error.code, error.detail))?;
                    context.checkpoint()?;
                    Ok(inspected)
                },
                move |inspected| {
                    let output_digest = inspected.semantic_payload_digest.clone();
                    result_tx.send(inspected).map_err(|_| {
                        RuntimeJobFailure::new(
                            "graph_ingest_scan_result_receiver_lost",
                            "external mutation caller dropped before scan commit",
                        )
                    })?;
                    Ok(RuntimeJobSuccess::new(
                        "graph_ingest_scan_completed",
                        "immutable graph candidate prepared outside the brain actor",
                    )
                    .with_output_digest(output_digest))
                },
            )
            .map_err(|error| {
                ExternalMutationError::refused(
                    "graph_ingest_scan_job_refused",
                    format!("runtime job {job_id} for request {request_id}: {error}"),
                )
            })?;

        let wait = runtime_jobs
            .wait_terminal(
                &job_id,
                GRAPH_INGEST_SCAN_DEADLINE.saturating_add(Duration::from_secs(1)),
            )
            .map_err(|error| {
                ExternalMutationError::refused(
                    "graph_ingest_scan_job_status_failed",
                    format!("runtime job {job_id}: {error}"),
                )
            })?;
        match wait {
            RuntimeJobWait::Terminal(job) if job.state == RuntimeJobState::Succeeded => {
                let inspected = result_rx.recv().map_err(|_| {
                    ExternalMutationError::refused(
                        "graph_ingest_scan_result_missing",
                        format!("successful runtime job {job_id} published no candidate"),
                    )
                })?;
                Ok((job_id, inspected))
            }
            RuntimeJobWait::Terminal(job) => {
                let terminal = job.terminal_result.as_ref();
                Err(ExternalMutationError::refused(
                    "graph_ingest_scan_job_failed",
                    format!(
                        "runtime job {job_id} ended {:?}: {}: {}",
                        job.state,
                        terminal.map_or("terminal_result_missing", |value| value.code.as_str()),
                        terminal.map_or("no terminal detail", |value| value.message.as_str())
                    ),
                ))
            }
            RuntimeJobWait::ObservableNonTerminal(job) => {
                let _ = runtime_jobs.request_cancel(&job_id);
                Err(ExternalMutationError::refused(
                    "graph_ingest_scan_running_after_timeout",
                    format!(
                        "runtime job {job_id} remains {:?}; running_after_timeout={}, cancellation requested",
                        job.state, job.running_after_timeout
                    ),
                ))
            }
        }
    }

    pub(crate) fn dispatch_wire_json(
        &self,
        context: &MissionServiceTransportContextV1,
        body: &[u8],
        host: ExternalMutationExecutionHostV1,
    ) -> Result<ExternalMutationResponseV1, ExternalMutationError> {
        let request: ExternalMutationRequestV1 = serde_json::from_slice(body).map_err(|error| {
            ExternalMutationError::refused("invalid_external_mutation_request", error.to_string())
        })?;
        self.execute(context, request, host)
    }

    pub(crate) fn preview_graph_ingest(
        &self,
        context: &MissionServiceTransportContextV1,
        request: GraphIngestPreviewRequestV1,
        host: ExternalMutationExecutionHostV1,
    ) -> Result<GraphIngestPreviewResponseV1, ExternalMutationError> {
        if request.schema != GRAPH_INGEST_PREVIEW_REQUEST_SCHEMA
            || request.request_id.trim().is_empty()
        {
            return Err(ExternalMutationError::refused(
                "invalid_graph_ingest_preview_request",
                "strict preview schema and non-empty request id are required",
            ));
        }
        if context.ingress != MissionServiceIngressV1::McpStreamableHttp {
            return Err(ExternalMutationError::refused(
                "graph_ingest_preview_ingress_policy_disabled",
                "graph-ingest preview is MCP Streamable-HTTP only",
            ));
        }
        if context.authority_lease_id.is_some() {
            return Err(ExternalMutationError::refused(
                "graph_ingest_preview_lease_forbidden",
                "read-only preview never accepts or consumes an authorization lease",
            ));
        }
        let transport_session_id = required_transport_fact(
            context.transport_session_id.as_deref(),
            "missing_transport_session",
        )?;
        let ingress_context_digest = required_transport_fact(
            context.ingress_context_digest.as_deref(),
            "missing_ingress_context_digest",
        )?;
        let actor_brain_id =
            required_transport_fact(context.actor_brain_id.as_deref(), "missing_actor_brain_id")?;
        if host.selected_actor_brain_id != actor_brain_id
            || host.reconciliation_brain_id != actor_brain_id
        {
            return Err(ExternalMutationError::refused(
                "graph_ingest_preview_selected_actor_mismatch",
                "preview host differs from the exact owner-selected actor",
            ));
        }
        let semantic_action = request.mode.semantic_action();
        let contract =
            external_consumer_contract(semantic_action, Ingress::Mcp).map_err(|cell| {
                ExternalMutationError::refused(
                    "external_mutation_consumer_policy_disabled",
                    cell.detail,
                )
            })?;
        let journal_entries = self.open_journal()?.entries();
        let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
        let actor_request = BrainPromoteReconciliationRequestV1 {
            operation_id: format!("preview:{}", request.request_id),
            operation_object_digest: String::new(),
            source_brain_id: actor_brain_id.to_string(),
            reconciliation_brain_id: actor_brain_id.to_string(),
            medulla_path: PathBuf::new(),
            medulla_postimage_sha256: String::new(),
            authority_subject_id: String::new(),
            job: ExternalMutationActorJobV1::GraphIngestPreview(Box::new(
                GraphIngestPreviewActorJobV1 {
                    request: request.clone(),
                    journal_entries,
                    actor_brain_id: actor_brain_id.to_string(),
                    reply_tx,
                },
            )),
        };
        let routed = (host.reconcile_promote)(actor_request);
        let (mut input, snapshot) = match reply_rx.recv() {
            Ok(Ok(preview)) => {
                routed.map_err(|detail| {
                    ExternalMutationError::refused("graph_ingest_preview_actor_failed", detail)
                })?;
                preview
            }
            Ok(Err(error)) => return Err(error),
            Err(_) => {
                return Err(ExternalMutationError::refused(
                    "graph_ingest_preview_actor_failed",
                    routed
                        .err()
                        .unwrap_or_else(|| "preview actor exited without a reply".to_string()),
                ))
            }
        };
        let scan_binding = preview_graph_ingest_scan_binding(
            actor_brain_id,
            transport_session_id,
            ingress_context_digest,
        )?;
        let (scan_job_id, inspected) =
            self.supervise_graph_ingest_scan(&host, snapshot, scan_binding, &request.request_id)?;
        let semantic_payload_digest = inspected.semantic_payload_digest.clone();
        let operation_object_digest = external_operation_object_digest(
            semantic_action,
            actor_brain_id,
            &semantic_payload_digest,
        )?;
        let preview_id = graph_ingest_preview_id(
            transport_session_id,
            ingress_context_digest,
            context.route_selector.as_deref(),
            actor_brain_id,
            &operation_object_digest,
        )?;
        input.preview_id = preview_id.clone();
        let execute_request = match request.mode {
            GraphIngestA2ModeV1::Replace => ExternalMutationRequestV1::GraphIngestReplace {
                schema: EXTERNAL_MUTATION_REQUEST_SCHEMA.to_string(),
                request_id: request.request_id.clone(),
                request: input,
            },
            GraphIngestA2ModeV1::MergeExisting => {
                ExternalMutationRequestV1::GraphIngestMergeExisting {
                    schema: EXTERNAL_MUTATION_REQUEST_SCHEMA.to_string(),
                    request_id: request.request_id.clone(),
                    request: input,
                }
            }
        };
        let semantic = inspected.semantic_payload;
        Ok(GraphIngestPreviewResponseV1 {
            schema: GRAPH_INGEST_PREVIEW_RESPONSE_SCHEMA.to_string(),
            request_id: request.request_id,
            preview_id,
            semantic_action: semantic_action.to_string(),
            requested_effects: contract.expected_effects.clone(),
            authority_floor: contract.authority_floor,
            risk_class: contract.risk_class,
            ingress: Ingress::Mcp,
            route_selector: context.route_selector.clone(),
            actor_brain_id: actor_brain_id.to_string(),
            transport_session_id: transport_session_id.to_string(),
            ingress_context_digest: ingress_context_digest.to_string(),
            root_identity: semantic.root_identity,
            expected_graph_generation: semantic.expected_graph_generation,
            expected_source_projection_digest: semantic.expected_source_projection_digest,
            candidate_ownership_digest: semantic.candidate_ownership_digest,
            candidate_source_projection_digest: semantic.candidate_source_projection_digest,
            candidate_pipeline_digest: semantic.candidate_pipeline_digest,
            scan_job_id,
            semantic_payload_digest: semantic_payload_digest.clone(),
            operation_object_digest: operation_object_digest.clone(),
            authority_binding: GraphIngestAuthorityBindingV1 {
                target_action: semantic_action.to_string(),
                payload_digest: operation_object_digest,
                requested_effects: contract.expected_effects,
                mission_id: None,
                mission_head_id: None,
            },
            execute_request,
        })
    }

    pub(crate) fn execute(
        &self,
        context: &MissionServiceTransportContextV1,
        request: ExternalMutationRequestV1,
        host: ExternalMutationExecutionHostV1,
    ) -> Result<ExternalMutationResponseV1, ExternalMutationError> {
        request.validate_wire()?;
        if context.ingress != MissionServiceIngressV1::McpStreamableHttp {
            return Err(ExternalMutationError::refused(
                "external_mutation_ingress_policy_disabled",
                "the first typed external-mutation slice is MCP Streamable-HTTP only",
            ));
        }
        let transport_session_id = required_transport_fact(
            context.transport_session_id.as_deref(),
            "missing_transport_session",
        )?;
        let ingress_context_digest = required_transport_fact(
            context.ingress_context_digest.as_deref(),
            "missing_ingress_context_digest",
        )?;
        let lease_id = required_transport_fact(
            context.authority_lease_id.as_deref(),
            "missing_authorization_lease",
        )?;
        let brain_id =
            required_transport_fact(context.actor_brain_id.as_deref(), "missing_actor_brain_id")?;
        if host.selected_actor_brain_id != brain_id {
            return Err(ExternalMutationError::refused(
                "external_mutation_selected_actor_mismatch",
                "transport actor id differs from the owner-resolved execution host",
            ));
        }
        if let ExternalMutationRequestV1::BrainPromote { source_brain, .. } = &request {
            let selected_source = context
                .route_selector
                .as_deref()
                .map(crate::project_brains::ProjectBrainRegistry::canonical_key)
                .ok_or_else(|| {
                    ExternalMutationError::refused(
                        "brain_promote_route_selector_missing",
                        "promotion requires the exact owner-selected source root",
                    )
                })?;
            let requested_source =
                crate::project_brains::ProjectBrainRegistry::canonical_key(source_brain);
            if requested_source != selected_source {
                return Err(ExternalMutationError::refused(
                    "brain_promote_source_binding_mismatch",
                    "promotion source must be the exact owner-selected brain",
                ));
            }
        }
        let contract = external_consumer_contract(request.semantic_action_id(), Ingress::Mcp)
            .map_err(|disabled| {
                ExternalMutationError::refused(
                    "external_mutation_consumer_policy_disabled",
                    disabled.detail,
                )
            })?;
        if contract.consumer_id != TypedConsumerIdV1::ExternalMutationService {
            return Err(ExternalMutationError::refused(
                "external_mutation_consumer_mismatch",
                "owner registry routed the action to a different typed consumer",
            ));
        }

        let _operation = self.broker_operation.lock();
        let mut broker = self.open_broker()?;
        let recovery_resolver = Arc::clone(&host.resolve_brain);
        let mut resolve_recovery_brain = move |requested: &str| recovery_resolver(requested);
        self.recover_pending_with_broker(
            &mut broker,
            &mut resolve_recovery_brain,
            host.reconcile_promote.as_ref(),
        )?;
        // The broker writer lock is also cross-process. Read owner time only
        // after acquiring it and converging every older mutation: a process
        // queued behind another owner must not reserve against a timestamp or
        // domain preimage captured before that blocking boundary.
        let owner_now_ms = (self.owner_clock)();
        let lease = broker.lease(lease_id).cloned().ok_or_else(|| {
            ExternalMutationError::refused("authorization_lease_not_found", lease_id)
        })?;
        crate::authority_transport::verify_authorization_receipt(
            &lease.authorization_receipt,
            self.receipt_crypto.as_ref(),
        )
        .map_err(|error| ExternalMutationError::refused(error.code(), error.to_string()))?;
        let receipt = &lease.authorization_receipt.core;
        if receipt.action.as_str() != request.semantic_action_id()
            || receipt.ingress != Ingress::Mcp
            || receipt.brain_id != brain_id
            || receipt.transport_session_id != transport_session_id
            || receipt.ingress_context_digest != ingress_context_digest
            || receipt.mission_id.is_some()
            || receipt.mission_head_id.is_some()
            || receipt.complete_effects != contract.expected_effects
        {
            return Err(ExternalMutationError::refused(
                "external_mutation_authorization_binding_mismatch",
                "receipt differs from the exact action, transport session, ingress context, actor brain, mission/head, or effect contract",
            ));
        }
        // Lost-response replay must be recognized before reopening mutable
        // preimages: a successful ratification/promotion/source edit has already
        // changed or retired those inputs. The sealed journal + consumed receipt
        // bind the exact request; `request_id` remains correlation-only and is
        // intentionally replaced by the retry's value.
        let early_terminal = {
            let journal = self.open_journal()?;
            journal.find_by_lease_and_object(lease_id, &receipt.verified_object_digest)
        };
        if let Some(entry) = early_terminal {
            if entry.authorization_snapshot_digest != lease.authorization_receipt.receipt_digest
                || entry.prepare.semantic_action != request.semantic_action_id()
                || entry.prepare.actor_brain_id != brain_id
                || !request_matches_existing_entry(&request, &entry)?
            {
                return Err(ExternalMutationError::refused(
                    "external_mutation_terminal_replay_binding_mismatch",
                    "existing operation differs from this signed request",
                ));
            }
            match entry.phase {
                ExternalMutationJournalPhaseV1::Published => {
                    return self.replay_published_response(
                        &request,
                        &host,
                        &entry.prepare.payload_digest,
                        &receipt.verified_object_digest,
                        &entry,
                    );
                }
                ExternalMutationJournalPhaseV1::Committed
                | ExternalMutationJournalPhaseV1::RecoveryRequired
                | ExternalMutationJournalPhaseV1::Reconciled => {
                    return Err(ExternalMutationError::refused(
                        "external_mutation_recovery_barrier_incomplete",
                        "global admission recovery returned with a non-terminal committed operation",
                    ));
                }
                ExternalMutationJournalPhaseV1::Prepared => {
                    match self.recover_prepared_entry_with_broker(
                        &mut broker,
                        &entry,
                        host.reconcile_promote.as_ref(),
                    )? {
                        PreparedRecoveryDispositionV1::Aborted => {
                            return Err(ExternalMutationError::refused(
                            "external_mutation_prepared_aborted",
                            "the protected journal proved no COMMIT and the one-shot lease was safely aborted",
                            ));
                        }
                        PreparedRecoveryDispositionV1::Pending => {
                            return Err(ExternalMutationError::refused(
                            "external_mutation_recovery_pending",
                            "the exact PREPARED operation is still inside its recovery boundary",
                            ));
                        }
                        PreparedRecoveryDispositionV1::Advanced(current) => {
                            if current.phase == ExternalMutationJournalPhaseV1::Published {
                                return self.replay_published_response(
                                    &request,
                                    &host,
                                    &current.prepare.payload_digest,
                                    &receipt.verified_object_digest,
                                    &current,
                                );
                            }
                            return Err(ExternalMutationError::refused(
                                "external_mutation_recovery_barrier_incomplete",
                                format!("operation advanced only to {:?}", current.phase),
                            ));
                        }
                    }
                }
            }
        }

        if lease.state == AuthorizationLeaseStateV1::Aborted {
            return Err(ExternalMutationError::refused(
                "external_mutation_orphan_reservation_aborted_reauthorization_required",
                "the prior pre-PREPARED reservation was safely aborted; issue a fresh one-shot lease",
            ));
        }
        if lease.state != AuthorizationLeaseStateV1::Unused {
            return Err(ExternalMutationError::refused(
                "external_mutation_lease_state_without_journal",
                format!(
                    "lease is {:?} but has no exact outer journal entry",
                    lease.state
                ),
            ));
        }

        let journal_entries = self.open_journal()?.entries();
        let inspected = inspect_request_actor_only(
            &request,
            &host,
            receipt.subject_id.as_str(),
            owner_now_ms,
            &journal_entries,
            brain_id,
        )?;
        let inspected = match inspected {
            InspectedMutationV1::GraphIngestSnapshot(snapshot) => {
                let scan_binding = authorized_graph_ingest_scan_binding(
                    receipt,
                    &lease.authorization_receipt.receipt_digest,
                )?;
                let (_, completed) = self.supervise_graph_ingest_scan(
                    &host,
                    *snapshot,
                    scan_binding,
                    request.request_id(),
                )?;
                InspectedMutationV1::GraphIngest(Box::new(completed))
            }
            inspected => inspected,
        };
        let semantic_payload_digest = inspected.semantic_payload_digest().to_string();
        let operation_object_digest = external_operation_object_digest(
            request.semantic_action_id(),
            brain_id,
            &semantic_payload_digest,
        )?;
        if let Some(supplied_preview_id) = graph_ingest_request_preview_id(&request) {
            let expected_preview_id = graph_ingest_preview_id(
                transport_session_id,
                ingress_context_digest,
                context.route_selector.as_deref(),
                brain_id,
                &operation_object_digest,
            )?;
            if supplied_preview_id != expected_preview_id {
                return Err(ExternalMutationError::refused(
                    "graph_ingest_preview_binding_mismatch",
                    "execute request is not bound to this transport session, ingress context, selected route, actor, and operation preview",
                ));
            }
        }
        if receipt.verified_object_digest != operation_object_digest {
            return Err(ExternalMutationError::refused(
                "external_mutation_authorization_object_mismatch",
                "signed receipt does not bind the owner-derived operation object",
            ));
        }
        let reservation = broker.reserve(
            lease_id,
            transport_session_id,
            ingress_context_digest,
            &operation_object_digest,
            owner_now_ms,
        )?;
        let authorization_snapshot_digest = lease.authorization_receipt.receipt_digest.clone();
        self.hit_fault("after_reserve")?;

        let staged = match inspected {
            InspectedMutationV1::SourceEdit(inspected) => {
                return self.execute_source_edit_actor_transaction(
                    &request,
                    &host,
                    &mut broker,
                    &reservation,
                    *inspected,
                    &authorization_snapshot_digest,
                    lease_id,
                    semantic_payload_digest,
                    operation_object_digest,
                    &contract.expected_effects,
                    context.route_selector.clone(),
                    brain_id,
                    owner_now_ms,
                );
            }
            inspected => {
                inspected.stage(&reservation, &operation_object_digest, &self.journal_root)?
            }
        };
        self.hit_fault("after_stage")?;
        let planned_outcome_digest = staged.outcome_digest()?;
        let mut journal = self.open_journal()?;
        let journal_entry = journal.prepare(
            &reservation,
            &authorization_snapshot_digest,
            ExternalMutationPrepareV1 {
                semantic_action: request.semantic_action_id().to_string(),
                payload_digest: semantic_payload_digest.clone(),
                operation_object_digest: operation_object_digest.clone(),
                operation_version: EXTERNAL_MUTATION_OPERATION_VERSION,
                actor_brain_id: brain_id.to_string(),
                route_selector: context.route_selector.clone(),
                mission_id: None,
                mission_head_id: None,
                recovery_payload: staged.recovery_payload(),
            },
            owner_now_ms,
        )?;
        drop(journal);
        self.hit_fault("after_journal_prepared")?;

        match staged {
            StagedMutationV1::Ratify(staged) => self.execute_prepared_ratify(
                &request,
                &host,
                &mut broker,
                &reservation,
                staged,
                &journal_entry,
                lease_id,
                planned_outcome_digest,
            ),
            StagedMutationV1::Promote(staged) => self.execute_prepared_promote(
                &request,
                &host,
                &mut broker,
                &reservation,
                *staged,
                &journal_entry,
                lease_id,
                planned_outcome_digest,
            ),
            StagedMutationV1::GraphIngest(staged) => self.execute_prepared_graph_ingest(
                &request,
                &host,
                &mut broker,
                &reservation,
                *staged,
                &journal_entry,
                lease_id,
                planned_outcome_digest,
            ),
        }
    }

    fn replay_published_response(
        &self,
        request: &ExternalMutationRequestV1,
        _host: &ExternalMutationExecutionHostV1,
        semantic_payload_digest: &str,
        operation_object_digest: &str,
        entry: &ExternalMutationJournalEntryV1,
    ) -> Result<ExternalMutationResponseV1, ExternalMutationError> {
        let sealed = entry.published_result.as_ref().ok_or_else(|| {
            ExternalMutationError::refused(
                "external_mutation_terminal_replay_binding_mismatch",
                "PUBLISHED terminal has no sealed result",
            )
        })?;
        if entry.phase != ExternalMutationJournalPhaseV1::Published
            || sealed.semantic_action != request.semantic_action_id()
            || sealed.semantic_payload_digest != semantic_payload_digest
            || sealed.operation_object_digest != operation_object_digest
            || entry.outcome_digest.as_deref() != Some(sealed.outcome_digest.as_str())
        {
            return Err(ExternalMutationError::refused(
                "external_mutation_terminal_replay_binding_mismatch",
                "sealed PUBLISHED result differs from the requested terminal bindings",
            ));
        }
        Ok(response_from_sealed_result(
            request.request_id(),
            &entry.lease_id,
            &entry.reservation_id,
            &entry.operation_id,
            sealed,
            true,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_prepared_ratify(
        &self,
        request: &ExternalMutationRequestV1,
        host: &ExternalMutationExecutionHostV1,
        broker: &mut OwnerAuthorizationBrokerV1,
        reservation: &AuthorizationReservationV1,
        staged: StagedRatifyV1,
        prepared_entry: &ExternalMutationJournalEntryV1,
        lease_id: &str,
        planned_outcome_digest: Option<String>,
    ) -> Result<ExternalMutationResponseV1, ExternalMutationError> {
        let outcome_digest = planned_outcome_digest.ok_or_else(|| {
            ExternalMutationError::refused(
                "external_mutation_commit_outcome_missing",
                "ratification omitted its staged outcome digest",
            )
        })?;
        if outcome_digest != staged.outcome_digest()? {
            return Err(ExternalMutationError::refused(
                "system_blocks_ratify_outcome_binding_mismatch",
                "journal plan and staged ratification outcome differ",
            ));
        }
        let operation_id = prepared_entry.operation_id.clone();
        let execution = self.run_ratify_actor_handshake(
            prepared_entry,
            staged,
            true,
            host.reconcile_promote.as_ref(),
            |ready| {
                if ready.outcome_digest != outcome_digest {
                    return Err(ExternalMutationError::refused(
                        "system_blocks_ratify_actor_ready_binding_mismatch",
                        "actor READY outcome differs from the staged ratification",
                    ));
                }
                let current = (self.current_authority)().map_err(|detail| {
                    ExternalMutationError::refused("authority_runtime_unavailable", detail)
                })?;
                let finalization_now_ms = (self.owner_clock)();
                broker.finalize_external_mutation(
                    reservation,
                    &current,
                    finalization_now_ms,
                    || {
                        (self.fault_hook)("after_broker_finalization_prepared")?;
                        let mut journal = self.open_journal().map_err(|error| error.to_string())?;
                        let witness = journal
                            .commit(
                                &ready.operation_id,
                                ready.outcome_digest.clone(),
                                finalization_now_ms,
                            )
                            .map_err(|error| error.to_string())?;
                        (self.fault_hook)("after_journal_committed")?;
                        Ok(witness)
                    },
                )?;
                self.hit_fault("after_broker_consumed")
            },
        );
        let execution = match execution {
            Ok(execution) => execution,
            Err(error) => {
                self.mark_promote_recovery_required_if_committed(&operation_id);
                if self
                    .binding_is_committed(lease_id, &prepared_entry.prepare.operation_object_digest)
                {
                    return Err(ExternalMutationError::refused(
                        "external_mutation_recovery_required",
                        format!("committed ratification needs actor recovery: {error}"),
                    ));
                }
                return Err(error);
            }
        };
        let committed_entry = self
            .open_journal()?
            .entry(&operation_id)
            .cloned()
            .ok_or_else(|| {
                ExternalMutationError::refused(
                    "external_mutation_operation_not_found",
                    &operation_id,
                )
            })?;
        let result = MutationPublishResultV1 {
            payload: execution.publish_payload,
            graph_resync_required: false,
        };
        let published_result = seal_published_result(
            &committed_entry.prepare.semantic_action,
            &committed_entry.prepare.payload_digest,
            &committed_entry.prepare.operation_object_digest,
            committed_entry.outcome_digest.as_deref().ok_or_else(|| {
                ExternalMutationError::refused(
                    "external_mutation_committed_outcome_missing",
                    &operation_id,
                )
            })?,
            &result,
        );
        self.open_journal()?.mark_published(
            &operation_id,
            published_result.clone(),
            (self.owner_clock)().max(committed_entry.updated_at),
        )?;
        self.hit_fault("after_journal_published")?;
        Ok(response_from_sealed_result(
            request.request_id(),
            lease_id,
            &reservation.reservation_id,
            &operation_id,
            &published_result,
            false,
        ))
    }

    fn run_ratify_actor_handshake<F>(
        &self,
        entry: &ExternalMutationJournalEntryV1,
        staged: StagedRatifyV1,
        require_original_preimage: bool,
        reconciler: &BrainPromoteReconcilerV1,
        owner_ready: F,
    ) -> Result<BrainPromoteReconciliationExecutionV1, ExternalMutationError>
    where
        F: FnOnce(&RatifyActorReadyV1) -> Result<(), ExternalMutationError>,
    {
        let expected_outcome_digest = staged.outcome_digest()?;
        let expected_target = staged.target_path.clone();
        let expected_next_sha256 = staged.next_sha256.clone();
        let expected_reconciliation_brain_id = staged.reconciliation_brain_id.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (decision_tx, decision_rx) = std::sync::mpsc::sync_channel(1);
        let request = BrainPromoteReconciliationRequestV1 {
            operation_id: entry.operation_id.clone(),
            operation_object_digest: entry.prepare.operation_object_digest.clone(),
            source_brain_id: entry.prepare.actor_brain_id.clone(),
            reconciliation_brain_id: staged.reconciliation_brain_id.clone(),
            medulla_path: staged.target_path.clone(),
            medulla_postimage_sha256: staged.next_sha256.clone(),
            authority_subject_id: String::new(),
            job: ExternalMutationActorJobV1::Ratify(Box::new(RatifyActorJobV1 {
                staged,
                source_brain_id: entry.prepare.route_selector.clone().unwrap_or_default(),
                reconciliation_brain_id: expected_reconciliation_brain_id.clone(),
                operation_id: entry.operation_id.clone(),
                reservation_id: entry.reservation_id.clone(),
                require_original_preimage,
                fault_hook: Arc::clone(&self.fault_hook),
                ready_tx,
                decision_rx,
            })),
        };
        std::thread::scope(|scope| {
            let worker = scope.spawn(move || reconciler(request));
            let ready = match ready_rx.recv() {
                Ok(Ok(ready)) => ready,
                Ok(Err(detail)) => {
                    let _ = decision_tx.send(BrainPromoteActorDecisionV1::Abort);
                    let _ = worker.join();
                    return Err(ExternalMutationError::refused(
                        "system_blocks_ratify_actor_precommit_refused",
                        detail,
                    ));
                }
                Err(_) => {
                    let joined = worker.join().map_err(|_| {
                        ExternalMutationError::refused(
                            "system_blocks_ratify_actor_panicked",
                            "ratify actor panicked before READY",
                        )
                    })?;
                    return Err(ExternalMutationError::refused(
                        "system_blocks_ratify_actor_failed",
                        joined
                            .err()
                            .unwrap_or_else(|| "ratify actor exited without READY".to_string()),
                    ));
                }
            };
            if ready.operation_id != entry.operation_id
                || ready.operation_object_digest != entry.prepare.operation_object_digest
                || ready.outcome_digest != expected_outcome_digest
                || ready.target_path != expected_target
                || ready.next_sha256 != expected_next_sha256
                || ready.reconciliation_brain_id != expected_reconciliation_brain_id
            {
                let _ = decision_tx.send(BrainPromoteActorDecisionV1::Abort);
                let _ = worker.join();
                return Err(ExternalMutationError::refused(
                    "system_blocks_ratify_actor_ready_binding_mismatch",
                    "actor READY differs from the sealed ratification",
                ));
            }
            if let Err(error) = owner_ready(&ready) {
                let _ = decision_tx.send(BrainPromoteActorDecisionV1::Abort);
                let _ = worker.join();
                return Err(error);
            }
            decision_tx
                .send(BrainPromoteActorDecisionV1::Committed)
                .map_err(|_| {
                    ExternalMutationError::refused(
                        "system_blocks_ratify_actor_decision_disconnected",
                        "ratify actor dropped before receiving COMMITTED",
                    )
                })?;
            worker
                .join()
                .map_err(|_| {
                    ExternalMutationError::refused(
                        "system_blocks_ratify_actor_panicked",
                        "ratify actor panicked after COMMITTED",
                    )
                })?
                .map_err(|detail| {
                    ExternalMutationError::refused("system_blocks_ratify_actor_failed", detail)
                })
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_prepared_promote(
        &self,
        request: &ExternalMutationRequestV1,
        host: &ExternalMutationExecutionHostV1,
        broker: &mut OwnerAuthorizationBrokerV1,
        reservation: &AuthorizationReservationV1,
        staged: StagedPromoteV1,
        prepared_entry: &ExternalMutationJournalEntryV1,
        lease_id: &str,
        planned_outcome_digest: Option<String>,
    ) -> Result<ExternalMutationResponseV1, ExternalMutationError> {
        let outcome_digest = planned_outcome_digest.ok_or_else(|| {
            ExternalMutationError::refused(
                "external_mutation_commit_outcome_missing",
                "promotion omitted its staged outcome digest",
            )
        })?;
        let operation_id = prepared_entry.operation_id.clone();
        let execution = self.run_promote_actor_handshake(
            prepared_entry,
            staged,
            true,
            host.reconcile_promote.as_ref(),
            |ready| {
                if ready.outcome_digest != outcome_digest {
                    return Err(ExternalMutationError::refused(
                        "brain_promote_actor_ready_binding_mismatch",
                        "actor READY outcome differs from the staged promotion",
                    ));
                }
                let current = (self.current_authority)().map_err(|detail| {
                    ExternalMutationError::refused("authority_runtime_unavailable", detail)
                })?;
                let finalization_now_ms = (self.owner_clock)();
                broker.finalize_external_mutation(
                    reservation,
                    &current,
                    finalization_now_ms,
                    || {
                        (self.fault_hook)("after_broker_finalization_prepared")?;
                        let mut journal = self.open_journal().map_err(|error| error.to_string())?;
                        let witness = journal
                            .commit(
                                &ready.operation_id,
                                ready.outcome_digest.clone(),
                                finalization_now_ms,
                            )
                            .map_err(|error| error.to_string())?;
                        (self.fault_hook)("after_journal_committed")?;
                        Ok(witness)
                    },
                )?;
                self.hit_fault("after_broker_consumed")
            },
        );
        let execution = match execution {
            Ok(execution) => execution,
            Err(error) => {
                self.mark_promote_recovery_required_if_committed(&operation_id);
                let committed = self.open_journal().ok().and_then(|journal| {
                    journal.entry(&operation_id).map(|entry| {
                        matches!(
                            entry.phase,
                            ExternalMutationJournalPhaseV1::Committed
                                | ExternalMutationJournalPhaseV1::RecoveryRequired
                        )
                    })
                });
                if committed == Some(true) {
                    return Err(ExternalMutationError::refused(
                        "external_mutation_recovery_required",
                        format!("committed promotion needs forward recovery: {error}"),
                    ));
                }
                return Err(error);
            }
        };
        let committed_entry = self
            .open_journal()?
            .entry(&operation_id)
            .cloned()
            .ok_or_else(|| {
                ExternalMutationError::refused(
                    "external_mutation_operation_not_found",
                    &operation_id,
                )
            })?;
        let published_result = match self.seal_promote_reconciliation(&committed_entry, execution) {
            Ok(result) => result,
            Err(error) => {
                self.mark_promote_recovery_required_if_committed(&operation_id);
                return Err(ExternalMutationError::refused(
                    "external_mutation_recovery_required",
                    format!("promotion graph reconciliation needs recovery: {error}"),
                ));
            }
        };
        self.hit_fault("after_graph_reconciled")?;
        let reconciled_entry = self
            .open_journal()?
            .entry(&operation_id)
            .cloned()
            .ok_or_else(|| {
                ExternalMutationError::refused(
                    "external_mutation_operation_not_found",
                    &operation_id,
                )
            })?;
        self.open_journal()?.mark_published(
            &operation_id,
            published_result.clone(),
            (self.owner_clock)().max(reconciled_entry.updated_at),
        )?;
        self.hit_fault("after_journal_published")?;
        Ok(response_from_sealed_result(
            request.request_id(),
            lease_id,
            &reservation.reservation_id,
            &operation_id,
            &published_result,
            false,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_prepared_graph_ingest(
        &self,
        request: &ExternalMutationRequestV1,
        host: &ExternalMutationExecutionHostV1,
        broker: &mut OwnerAuthorizationBrokerV1,
        reservation: &AuthorizationReservationV1,
        staged: StagedGraphIngestA2V1,
        prepared_entry: &ExternalMutationJournalEntryV1,
        lease_id: &str,
        planned_outcome_digest: Option<String>,
    ) -> Result<ExternalMutationResponseV1, ExternalMutationError> {
        let outcome_digest = planned_outcome_digest.ok_or_else(|| {
            ExternalMutationError::refused(
                "external_mutation_commit_outcome_missing",
                "A2 graph ingest omitted its staged outcome digest",
            )
        })?;
        if outcome_digest != staged.outcome_digest {
            return Err(ExternalMutationError::refused(
                "graph_ingest_outcome_binding_mismatch",
                "journal plan and staged A2 outcome differ",
            ));
        }
        let operation_id = prepared_entry.operation_id.clone();
        let execution = self.run_graph_ingest_actor_handshake(
            prepared_entry,
            staged,
            true,
            host.reconcile_promote.as_ref(),
            |ready| {
                if ready.outcome_digest != outcome_digest {
                    return Err(ExternalMutationError::refused(
                        "graph_ingest_actor_ready_binding_mismatch",
                        "actor READY outcome differs from the staged A2 ingest",
                    ));
                }
                let current = (self.current_authority)().map_err(|detail| {
                    ExternalMutationError::refused("authority_runtime_unavailable", detail)
                })?;
                let finalization_now_ms = (self.owner_clock)();
                broker.finalize_external_mutation(
                    reservation,
                    &current,
                    finalization_now_ms,
                    || {
                        (self.fault_hook)("after_broker_finalization_prepared")?;
                        let mut journal = self.open_journal().map_err(|error| error.to_string())?;
                        let witness = journal
                            .commit(
                                &ready.operation_id,
                                ready.outcome_digest.clone(),
                                finalization_now_ms,
                            )
                            .map_err(|error| error.to_string())?;
                        (self.fault_hook)("after_journal_committed")?;
                        Ok(witness)
                    },
                )?;
                self.hit_fault("after_broker_consumed")
            },
        );
        let execution = match execution {
            Ok(execution) => execution,
            Err(error) => {
                self.mark_promote_recovery_required_if_committed(&operation_id);
                let committed = self.open_journal().ok().and_then(|journal| {
                    journal.entry(&operation_id).map(|entry| {
                        matches!(
                            entry.phase,
                            ExternalMutationJournalPhaseV1::Committed
                                | ExternalMutationJournalPhaseV1::RecoveryRequired
                        )
                    })
                });
                if committed == Some(true) {
                    return Err(ExternalMutationError::refused(
                        "external_mutation_recovery_required",
                        format!("committed A2 graph ingest needs forward recovery: {error}"),
                    ));
                }
                return Err(error);
            }
        };
        let committed_entry = self
            .open_journal()?
            .entry(&operation_id)
            .cloned()
            .ok_or_else(|| {
                ExternalMutationError::refused(
                    "external_mutation_operation_not_found",
                    &operation_id,
                )
            })?;
        let published_result = match self.seal_graph_ingest_checkpoint(&committed_entry, execution)
        {
            Ok(result) => result,
            Err(error) => {
                self.mark_promote_recovery_required_if_committed(&operation_id);
                return Err(ExternalMutationError::refused(
                    "external_mutation_recovery_required",
                    format!("A2 graph checkpoint sealing needs recovery: {error}"),
                ));
            }
        };
        self.hit_fault("after_graph_reconciled")?;
        let current_entry = self
            .open_journal()?
            .entry(&operation_id)
            .cloned()
            .ok_or_else(|| {
                ExternalMutationError::refused(
                    "external_mutation_operation_not_found",
                    &operation_id,
                )
            })?;
        self.open_journal()?.mark_published(
            &operation_id,
            published_result.clone(),
            (self.owner_clock)().max(current_entry.updated_at),
        )?;
        self.hit_fault("after_journal_published")?;
        Ok(response_from_sealed_result(
            request.request_id(),
            lease_id,
            &reservation.reservation_id,
            &operation_id,
            &published_result,
            false,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_source_edit_actor_transaction(
        &self,
        request: &ExternalMutationRequestV1,
        host: &ExternalMutationExecutionHostV1,
        broker: &mut OwnerAuthorizationBrokerV1,
        reservation: &AuthorizationReservationV1,
        inspected: InspectedSourceEditV1,
        authorization_snapshot_digest: &str,
        lease_id: &str,
        semantic_payload_digest: String,
        operation_object_digest: String,
        expected_effects: &BTreeSet<Effect>,
        route_selector: Option<String>,
        brain_id: &str,
        owner_now_ms: u64,
    ) -> Result<ExternalMutationResponseV1, ExternalMutationError> {
        let context =
            inspected.prepared_context(&operation_object_digest, expected_effects, brain_id)?;
        let reconciliation_brain_id = inspected.reconciliation_brain_id.clone();
        if reconciliation_brain_id.trim().is_empty() {
            return Err(ExternalMutationError::refused(
                "source_edit_actor_binding_missing",
                "source edit requires the exact selected-brain actor id",
            ));
        }

        let preview_id = inspected.request.preview_id.clone();
        let target_identity = inspected.intent.semantic_payload.target_identity.clone();
        let candidate_sha256 = inspected.intent.semantic_payload.candidate_sha256.clone();
        let authority_subject_id = inspected.authority_subject_id.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (decision_tx, decision_rx) = std::sync::mpsc::sync_channel(1);
        // The protected journal operation id depends on the actor-produced stage
        // digest, so the reservation id is the pre-PREPARED correlation id. The
        // exact journal id is materialized and checked while the actor waits READY.
        let actor_request = BrainPromoteReconciliationRequestV1 {
            operation_id: reservation.reservation_id.clone(),
            operation_object_digest: operation_object_digest.clone(),
            source_brain_id: brain_id.to_string(),
            reconciliation_brain_id: reconciliation_brain_id.clone(),
            medulla_path: PathBuf::from(target_identity),
            medulla_postimage_sha256: candidate_sha256,
            authority_subject_id,
            job: ExternalMutationActorJobV1::SourceEdit(Box::new(SourceEditActorJobV1 {
                mode: SourceEditActorModeV1::Prepare {
                    request: inspected.request,
                    context,
                },
                reconciliation_brain_id: reconciliation_brain_id.clone(),
                fault_hook: Arc::clone(&self.fault_hook),
                ready_tx,
                decision_rx,
            })),
        };
        let expected_ready = SourceEditActorExpectedReadyV1 {
            preview_id,
            operation_object_digest: operation_object_digest.clone(),
            reconciliation_brain_id,
            transaction_id: None,
            stage_digest: None,
        };
        let mut prepared_entry = None;
        let execution = self.run_source_edit_actor_handshake(
            actor_request,
            ready_rx,
            decision_tx,
            expected_ready,
            host.reconcile_promote.as_ref(),
            |ready| {
                self.hit_fault("after_stage")?;
                let mut journal = self.open_journal()?;
                let entry = journal.prepare(
                    reservation,
                    authorization_snapshot_digest,
                    ExternalMutationPrepareV1 {
                        semantic_action: request.semantic_action_id().to_string(),
                        payload_digest: semantic_payload_digest.clone(),
                        operation_object_digest: operation_object_digest.clone(),
                        operation_version: EXTERNAL_MUTATION_OPERATION_VERSION,
                        actor_brain_id: brain_id.to_string(),
                        route_selector: route_selector.clone(),
                        mission_id: None,
                        mission_head_id: None,
                        recovery_payload: ready
                            .staged
                            .recovery_payload(&ready.reconciliation_brain_id),
                    },
                    owner_now_ms,
                )?;
                drop(journal);
                self.hit_fault("after_journal_prepared")?;

                let current = (self.current_authority)().map_err(|detail| {
                    ExternalMutationError::refused("authority_runtime_unavailable", detail)
                })?;
                let finalization_now_ms = (self.owner_clock)();
                broker.finalize_external_mutation(
                    reservation,
                    &current,
                    finalization_now_ms,
                    || {
                        (self.fault_hook)("after_broker_finalization_prepared")?;
                        let mut journal = self.open_journal().map_err(|error| error.to_string())?;
                        let witness = journal
                            .commit(
                                &entry.operation_id,
                                ready.staged.outcome_digest().to_string(),
                                finalization_now_ms,
                            )
                            .map_err(|error| error.to_string())?;
                        (self.fault_hook)("after_journal_committed")?;
                        Ok(witness)
                    },
                )?;
                self.hit_fault("after_broker_consumed")?;
                let committed_entry = self
                    .open_journal()?
                    .entry(&entry.operation_id)
                    .cloned()
                    .ok_or_else(|| {
                        ExternalMutationError::refused(
                            "source_edit_committed_journal_missing",
                            &entry.operation_id,
                        )
                    })?;
                if committed_entry.phase != ExternalMutationJournalPhaseV1::Committed
                    || committed_entry.outcome_digest.as_deref()
                        != Some(ready.staged.outcome_digest())
                {
                    return Err(ExternalMutationError::refused(
                        "source_edit_committed_journal_mismatch",
                        "source actor READY differs from the durable outer COMMIT",
                    ));
                }
                prepared_entry = Some(committed_entry);
                Ok(())
            },
        );
        let execution = match execution {
            Ok(execution) => execution,
            Err(error) => {
                self.mark_binding_recovery_required_if_committed(
                    lease_id,
                    &operation_object_digest,
                );
                if self.binding_is_committed(lease_id, &operation_object_digest) {
                    return Err(ExternalMutationError::refused(
                        "external_mutation_recovery_required",
                        format!("committed source edit needs actor recovery: {error}"),
                    ));
                }
                return Err(error);
            }
        };
        let entry = prepared_entry.ok_or_else(|| {
            ExternalMutationError::refused(
                "source_edit_journal_prepare_missing",
                "source actor completed without its exact protected journal entry",
            )
        })?;
        let operation_id = entry.operation_id.clone();
        let published_result = match self.seal_source_edit_checkpoint(&entry, execution) {
            Ok(result) => result,
            Err(error) => {
                self.mark_binding_recovery_required_if_committed(
                    lease_id,
                    &operation_object_digest,
                );
                return Err(ExternalMutationError::refused(
                    "external_mutation_recovery_required",
                    format!("source-edit actor checkpoint needs recovery: {error}"),
                ));
            }
        };
        self.open_journal()?.mark_published(
            &operation_id,
            published_result.clone(),
            (self.owner_clock)().max(entry.updated_at),
        )?;
        self.hit_fault("after_journal_published")?;
        Ok(response_from_sealed_result(
            request.request_id(),
            lease_id,
            &reservation.reservation_id,
            &operation_id,
            &published_result,
            false,
        ))
    }

    fn run_source_edit_actor_handshake<F>(
        &self,
        request: BrainPromoteReconciliationRequestV1,
        ready_rx: Receiver<Result<SourceEditActorReadyV1, String>>,
        decision_tx: std::sync::mpsc::SyncSender<BrainPromoteActorDecisionV1>,
        expected: SourceEditActorExpectedReadyV1,
        reconciler: &BrainPromoteReconcilerV1,
        owner_ready: F,
    ) -> Result<BrainPromoteReconciliationExecutionV1, ExternalMutationError>
    where
        F: FnOnce(&SourceEditActorReadyV1) -> Result<(), ExternalMutationError>,
    {
        std::thread::scope(|scope| {
            let worker = scope.spawn(move || reconciler(request));
            let ready = match ready_rx.recv() {
                Ok(Ok(ready)) => ready,
                Ok(Err(detail)) => {
                    let _ = decision_tx.send(BrainPromoteActorDecisionV1::Abort);
                    let _ = worker.join();
                    return Err(ExternalMutationError::refused(
                        "source_edit_actor_precommit_refused",
                        detail,
                    ));
                }
                Err(_) => {
                    let joined = worker.join().map_err(|_| {
                        ExternalMutationError::refused(
                            "source_edit_actor_panicked",
                            "source-edit actor panicked before READY",
                        )
                    })?;
                    return Err(ExternalMutationError::refused(
                        "source_edit_actor_failed",
                        joined.err().unwrap_or_else(|| {
                            "source-edit actor exited without READY".to_string()
                        }),
                    ));
                }
            };
            if ready.staged.preview_id != expected.preview_id
                || ready.staged.operation_object_digest != expected.operation_object_digest
                || ready.reconciliation_brain_id != expected.reconciliation_brain_id
                || expected
                    .transaction_id
                    .as_deref()
                    .is_some_and(|value| value != ready.staged.transaction_id)
                || expected
                    .stage_digest
                    .as_deref()
                    .is_some_and(|value| value != ready.staged.outcome_digest())
            {
                let _ = decision_tx.send(BrainPromoteActorDecisionV1::Abort);
                let _ = worker.join();
                return Err(ExternalMutationError::refused(
                    "source_edit_actor_ready_binding_mismatch",
                    "actor READY differs from the sealed source-edit operation",
                ));
            }
            if let Err(error) = owner_ready(&ready) {
                let _ = decision_tx.send(BrainPromoteActorDecisionV1::Abort);
                let _ = worker.join();
                return Err(error);
            }
            decision_tx
                .send(BrainPromoteActorDecisionV1::Committed)
                .map_err(|_| {
                    ExternalMutationError::refused(
                        "source_edit_actor_decision_disconnected",
                        "source-edit actor dropped before receiving COMMITTED",
                    )
                })?;
            worker
                .join()
                .map_err(|_| {
                    ExternalMutationError::refused(
                        "source_edit_actor_panicked",
                        "source-edit actor panicked after COMMITTED",
                    )
                })?
                .map_err(|detail| {
                    ExternalMutationError::refused("source_edit_actor_failed", detail)
                })
        })
    }

    fn run_source_edit_recovery_actor_handshake<F>(
        &self,
        entry: &ExternalMutationJournalEntryV1,
        recovery: SourceEditRecoveryPayloadV1,
        reconciler: &BrainPromoteReconcilerV1,
        owner_ready: F,
    ) -> Result<BrainPromoteReconciliationExecutionV1, ExternalMutationError>
    where
        F: FnOnce(&SourceEditActorReadyV1) -> Result<(), ExternalMutationError>,
    {
        validate_source_edit_recovery_payload(entry, &recovery)?;
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (decision_tx, decision_rx) = std::sync::mpsc::sync_channel(1);
        let expected = SourceEditActorExpectedReadyV1 {
            preview_id: recovery.preview_id.clone(),
            operation_object_digest: recovery.operation_object_digest.clone(),
            reconciliation_brain_id: recovery.reconciliation_brain_id.clone(),
            transaction_id: Some(recovery.transaction_id.clone()),
            stage_digest: Some(recovery.stage_digest.clone()),
        };
        let request = BrainPromoteReconciliationRequestV1 {
            operation_id: entry.operation_id.clone(),
            operation_object_digest: entry.prepare.operation_object_digest.clone(),
            source_brain_id: entry.prepare.actor_brain_id.clone(),
            reconciliation_brain_id: recovery.reconciliation_brain_id.clone(),
            medulla_path: PathBuf::new(),
            medulla_postimage_sha256: recovery.stage_digest.clone(),
            authority_subject_id: String::new(),
            job: ExternalMutationActorJobV1::SourceEdit(Box::new(SourceEditActorJobV1 {
                mode: SourceEditActorModeV1::Recover {
                    source_brain_id: entry.prepare.route_selector.clone().unwrap_or_default(),
                    recovery,
                },
                reconciliation_brain_id: expected.reconciliation_brain_id.clone(),
                fault_hook: Arc::clone(&self.fault_hook),
                ready_tx,
                decision_rx,
            })),
        };
        self.run_source_edit_actor_handshake(
            request,
            ready_rx,
            decision_tx,
            expected,
            reconciler,
            owner_ready,
        )
    }

    fn seal_source_edit_checkpoint(
        &self,
        entry: &ExternalMutationJournalEntryV1,
        execution: BrainPromoteReconciliationExecutionV1,
    ) -> Result<ExternalMutationPublishedResultV1, ExternalMutationError> {
        let recovery: SourceEditRecoveryPayloadV1 =
            serde_json::from_value(entry.prepare.recovery_payload.clone()).map_err(|error| {
                ExternalMutationError::refused(
                    "external_mutation_recovery_payload_invalid",
                    error.to_string(),
                )
            })?;
        validate_source_edit_recovery_payload(entry, &recovery)?;
        let checkpoint_ack = execution.checkpoint_ack.ok_or_else(|| {
            ExternalMutationError::refused(
                "source_edit_checkpoint_ack_missing",
                "source-edit actor returned without its same-turn checkpoint ACK",
            )
        })?;
        if checkpoint_ack.schema != BRAIN_PROMOTE_CHECKPOINT_ACK_SCHEMA
            || checkpoint_ack.brain_id != recovery.reconciliation_brain_id
            || checkpoint_ack.checkpoint_id.trim().is_empty()
            || checkpoint_ack.epoch == 0
            || checkpoint_ack.generation == 0
            || checkpoint_ack.revision == 0
            || !is_digest(&checkpoint_ack.current_pointer_digest)
            || execution.graph_generation_before != execution.graph_generation_after
        {
            return Err(ExternalMutationError::refused(
                "source_edit_checkpoint_ack_mismatch",
                "checkpoint ACK does not bind the exact selected actor source-edit turn",
            ));
        }
        let checkpoint_ack_digest =
            digest_canonical(SOURCE_EDIT_CHECKPOINT_ACK_DIGEST_DOMAIN, &checkpoint_ack)?;
        let mut payload = execution.publish_payload;
        let object = payload.as_object_mut().ok_or_else(|| {
            ExternalMutationError::refused(
                "source_edit_publish_payload_invalid",
                "source-edit actor returned a non-object publish payload",
            )
        })?;
        object.insert("actor_checkpoint_required".to_string(), Value::Bool(true));
        object.insert(
            "actor_graph_generation_before".to_string(),
            json!(execution.graph_generation_before),
        );
        object.insert(
            "actor_graph_generation_after".to_string(),
            json!(execution.graph_generation_after),
        );
        object.insert(
            "checkpoint_ack".to_string(),
            serde_json::to_value(&checkpoint_ack).map_err(|error| {
                ExternalMutationError::refused(
                    "source_edit_checkpoint_ack_invalid",
                    error.to_string(),
                )
            })?,
        );
        object.insert(
            "checkpoint_ack_digest".to_string(),
            Value::String(checkpoint_ack_digest),
        );
        let result = MutationPublishResultV1 {
            payload,
            graph_resync_required: true,
        };
        Ok(seal_published_result(
            &entry.prepare.semantic_action,
            &entry.prepare.payload_digest,
            &entry.prepare.operation_object_digest,
            entry.outcome_digest.as_deref().ok_or_else(|| {
                ExternalMutationError::refused(
                    "external_mutation_committed_outcome_missing",
                    &entry.operation_id,
                )
            })?,
            &result,
        ))
    }

    fn run_graph_ingest_actor_handshake<F>(
        &self,
        entry: &ExternalMutationJournalEntryV1,
        staged: StagedGraphIngestA2V1,
        require_original_preimage: bool,
        reconciler: &BrainPromoteReconcilerV1,
        owner_ready: F,
    ) -> Result<BrainPromoteReconciliationExecutionV1, ExternalMutationError>
    where
        F: FnOnce(&GraphIngestActorReadyV1) -> Result<(), ExternalMutationError>,
    {
        // Candidate decode and source revalidation are deliberately outside the
        // actor queue. The actor receives immutable bytes plus a short state OCC
        // comparison; only installation and checkpoint publication run inside.
        let candidate = staged
            .load_durable_candidate(require_original_preimage)
            .map_err(|error| ExternalMutationError::refused(error.code, error.detail))?;
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (decision_tx, decision_rx) = std::sync::mpsc::sync_channel(1);
        let request = BrainPromoteReconciliationRequestV1 {
            operation_id: entry.operation_id.clone(),
            operation_object_digest: entry.prepare.operation_object_digest.clone(),
            source_brain_id: entry.prepare.actor_brain_id.clone(),
            reconciliation_brain_id: staged.reconciliation_brain_id.clone(),
            medulla_path: PathBuf::from(&staged.semantic_payload.root_identity),
            medulla_postimage_sha256: staged.ownership_manifest.source_projection_digest.clone(),
            authority_subject_id: staged.authority_subject_id.clone(),
            job: ExternalMutationActorJobV1::GraphIngest(Box::new(GraphIngestActorJobV1 {
                staged: staged.clone(),
                candidate,
                fault_hook: Arc::clone(&self.fault_hook),
                operation_id: entry.operation_id.clone(),
                require_original_preimage,
                ready_tx,
                decision_rx,
            })),
        };
        std::thread::scope(|scope| {
            let worker = scope.spawn(move || reconciler(request));
            let ready = match ready_rx.recv() {
                Ok(Ok(ready)) => ready,
                Ok(Err(detail)) => {
                    let _ = decision_tx.send(BrainPromoteActorDecisionV1::Abort);
                    let _ = worker.join();
                    return Err(ExternalMutationError::refused(
                        "graph_ingest_actor_precommit_refused",
                        detail,
                    ));
                }
                Err(_) => {
                    let joined = worker.join().map_err(|_| {
                        ExternalMutationError::refused(
                            "graph_ingest_actor_panicked",
                            "A2 graph-ingest actor panicked before READY",
                        )
                    })?;
                    return Err(ExternalMutationError::refused(
                        "graph_ingest_actor_failed",
                        joined
                            .err()
                            .unwrap_or_else(|| "actor exited without READY".to_string()),
                    ));
                }
            };
            if ready.operation_id != entry.operation_id
                || ready.operation_object_digest != entry.prepare.operation_object_digest
                || ready.outcome_digest != staged.outcome_digest
                || ready.root_identity != staged.semantic_payload.root_identity
                || ready.ownership_digest != staged.ownership_manifest.ownership_digest
            {
                let _ = decision_tx.send(BrainPromoteActorDecisionV1::Abort);
                let _ = worker.join();
                return Err(ExternalMutationError::refused(
                    "graph_ingest_actor_ready_binding_mismatch",
                    "actor READY differs from the sealed full-root A2 operation",
                ));
            }
            if let Err(error) = owner_ready(&ready) {
                let _ = decision_tx.send(BrainPromoteActorDecisionV1::Abort);
                let _ = worker.join();
                return Err(error);
            }
            decision_tx
                .send(BrainPromoteActorDecisionV1::Committed)
                .map_err(|_| {
                    ExternalMutationError::refused(
                        "graph_ingest_actor_decision_disconnected",
                        "A2 actor dropped before receiving COMMITTED",
                    )
                })?;
            worker
                .join()
                .map_err(|_| {
                    ExternalMutationError::refused(
                        "graph_ingest_actor_panicked",
                        "A2 graph-ingest actor panicked after COMMITTED",
                    )
                })?
                .map_err(|detail| {
                    ExternalMutationError::refused("graph_ingest_actor_failed", detail)
                })
        })
    }

    fn seal_graph_ingest_checkpoint(
        &self,
        entry: &ExternalMutationJournalEntryV1,
        execution: BrainPromoteReconciliationExecutionV1,
    ) -> Result<ExternalMutationPublishedResultV1, ExternalMutationError> {
        let expected_checkpoint_brain_id = execution
            .publish_payload
            .get("reconciliation_brain_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                ExternalMutationError::refused(
                    "graph_ingest_checkpoint_actor_binding_missing",
                    "A2 actor result omitted its sealed reconciliation brain id",
                )
            })?;
        let checkpoint_ack = execution.checkpoint_ack.ok_or_else(|| {
            ExternalMutationError::refused(
                "graph_ingest_checkpoint_ack_missing",
                "A2 actor mutation returned without its same-turn checkpoint ACK",
            )
        })?;
        if checkpoint_ack.schema != BRAIN_PROMOTE_CHECKPOINT_ACK_SCHEMA
            || execution.graph_generation_after <= execution.graph_generation_before
            // BrainVersion.generation is the maximum of every durable session
            // generation, not a synonym for graph_generation. A preceding
            // source transaction can legitimately leave cache/plasticity ahead
            // of the graph while this same-turn ACK still seals the exact A2
            // postimage. It may never trail the installed graph generation.
            || checkpoint_ack.generation < execution.graph_generation_after
            || checkpoint_ack.epoch == 0
            || checkpoint_ack.revision == 0
            || checkpoint_ack.brain_id != expected_checkpoint_brain_id
            || checkpoint_ack.checkpoint_id.trim().is_empty()
            || !is_digest(&checkpoint_ack.current_pointer_digest)
        {
            return Err(ExternalMutationError::refused(
                "graph_ingest_checkpoint_ack_mismatch",
                "checkpoint ACK does not bind the exact actor generation installed by A2",
            ));
        }
        let mut payload = execution.publish_payload;
        let object = payload.as_object_mut().ok_or_else(|| {
            ExternalMutationError::refused(
                "graph_ingest_publish_payload_invalid",
                "A2 actor returned a non-object publish payload",
            )
        })?;
        object.insert("ingest_output".to_string(), execution.ingest_output);
        object.insert(
            "graph_generation_before".to_string(),
            json!(execution.graph_generation_before),
        );
        object.insert(
            "graph_generation_after".to_string(),
            json!(execution.graph_generation_after),
        );
        object.insert(
            "checkpoint_ack".to_string(),
            serde_json::to_value(&checkpoint_ack).map_err(|error| {
                ExternalMutationError::refused(
                    "graph_ingest_checkpoint_ack_invalid",
                    error.to_string(),
                )
            })?,
        );
        let result = MutationPublishResultV1 {
            payload,
            graph_resync_required: false,
        };
        Ok(seal_published_result(
            &entry.prepare.semantic_action,
            &entry.prepare.payload_digest,
            &entry.prepare.operation_object_digest,
            entry.outcome_digest.as_deref().ok_or_else(|| {
                ExternalMutationError::refused(
                    "external_mutation_commit_outcome_missing",
                    "committed A2 journal entry has no outcome digest",
                )
            })?,
            &result,
        ))
    }

    fn run_promote_actor_handshake<F>(
        &self,
        entry: &ExternalMutationJournalEntryV1,
        staged: StagedPromoteV1,
        require_precommit_revalidation: bool,
        reconciler: &BrainPromoteReconcilerV1,
        owner_ready: F,
    ) -> Result<BrainPromoteReconciliationExecutionV1, ExternalMutationError>
    where
        F: FnOnce(&BrainPromoteActorReadyV1) -> Result<(), ExternalMutationError>,
    {
        let recovery: PromoteRecoveryPayloadV1 =
            serde_json::from_value(entry.prepare.recovery_payload.clone()).map_err(|error| {
                ExternalMutationError::refused(
                    "external_mutation_recovery_payload_invalid",
                    error.to_string(),
                )
            })?;
        let authority_subject_id = recovery.input.authority_subject_id;
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (decision_tx, decision_rx) = std::sync::mpsc::sync_channel(1);
        let request = BrainPromoteReconciliationRequestV1 {
            operation_id: entry.operation_id.clone(),
            operation_object_digest: entry.prepare.operation_object_digest.clone(),
            source_brain_id: entry.prepare.actor_brain_id.clone(),
            reconciliation_brain_id: recovery.reconciliation_brain_id.clone(),
            medulla_path: staged.medulla.target_path.clone(),
            medulla_postimage_sha256: staged.medulla.after_sha256.clone(),
            authority_subject_id,
            job: ExternalMutationActorJobV1::Promote(Box::new(BrainPromoteActorJobV1 {
                staged,
                fault_hook: Arc::clone(&self.fault_hook),
                operation_id: entry.operation_id.clone(),
                require_precommit_revalidation,
                ready_tx,
                decision_rx,
            })),
        };
        std::thread::scope(|scope| {
            let worker = scope.spawn(move || reconciler(request));
            let ready = match ready_rx.recv() {
                Ok(Ok(ready)) => ready,
                Ok(Err(detail)) => {
                    let _ = decision_tx.send(BrainPromoteActorDecisionV1::Abort);
                    let _ = worker.join();
                    return Err(ExternalMutationError::refused(
                        "brain_promote_actor_precommit_refused",
                        detail,
                    ));
                }
                Err(_) => {
                    let joined = worker.join().map_err(|_| {
                        ExternalMutationError::refused(
                            "brain_promote_actor_panicked",
                            "promotion actor worker panicked before READY",
                        )
                    })?;
                    return Err(ExternalMutationError::refused(
                        "brain_promote_reconciliation_failed",
                        joined
                            .err()
                            .unwrap_or_else(|| "actor exited without READY".to_string()),
                    ));
                }
            };
            if ready.operation_id != entry.operation_id
                || ready.operation_object_digest != entry.prepare.operation_object_digest
                || entry
                    .outcome_digest
                    .as_deref()
                    .is_some_and(|digest| digest != ready.outcome_digest)
                || ready.medulla_path != recovery.medulla.target_path
                || ready.medulla_postimage_sha256 != recovery.medulla.after_sha256
            {
                let _ = decision_tx.send(BrainPromoteActorDecisionV1::Abort);
                let _ = worker.join();
                return Err(ExternalMutationError::refused(
                    "brain_promote_actor_ready_binding_mismatch",
                    "actor READY differs from the sealed promotion operation",
                ));
            }
            if let Err(error) = owner_ready(&ready) {
                let _ = decision_tx.send(BrainPromoteActorDecisionV1::Abort);
                let _ = worker.join();
                return Err(error);
            }
            decision_tx
                .send(BrainPromoteActorDecisionV1::Committed)
                .map_err(|_| {
                    ExternalMutationError::refused(
                        "brain_promote_actor_decision_disconnected",
                        "actor dropped before receiving COMMITTED",
                    )
                })?;
            worker
                .join()
                .map_err(|_| {
                    ExternalMutationError::refused(
                        "brain_promote_actor_panicked",
                        "promotion actor worker panicked after COMMITTED",
                    )
                })?
                .map_err(|detail| {
                    ExternalMutationError::refused("brain_promote_reconciliation_failed", detail)
                })
        })
    }

    fn seal_promote_reconciliation(
        &self,
        entry: &ExternalMutationJournalEntryV1,
        execution: BrainPromoteReconciliationExecutionV1,
    ) -> Result<ExternalMutationPublishedResultV1, ExternalMutationError> {
        let recovery: PromoteRecoveryPayloadV1 =
            serde_json::from_value(entry.prepare.recovery_payload.clone()).map_err(|error| {
                ExternalMutationError::refused(
                    "external_mutation_recovery_payload_invalid",
                    error.to_string(),
                )
            })?;
        if recovery.kind != "brain_promote"
            || recovery.operation_object_digest != entry.prepare.operation_object_digest
            || recovery.outcome_digest != entry.outcome_digest.as_deref().unwrap_or_default()
            || execution.graph_generation_after <= execution.graph_generation_before
        {
            return Err(ExternalMutationError::refused(
                "brain_promote_reconciliation_binding_mismatch",
                "promotion execution differs from the committed operation",
            ));
        }
        let checkpoint_ack = execution.checkpoint_ack.ok_or_else(|| {
            ExternalMutationError::refused(
                "brain_promote_checkpoint_ack_missing",
                "reconciliation actor returned without its exact checkpoint ACK",
            )
        })?;
        let reconciled_at = (self.owner_clock)()
            .max(checkpoint_ack.confirmed_at_unix_ms)
            .max(entry.updated_at);
        let checkpoint_ack_digest =
            digest_canonical(BRAIN_PROMOTE_CHECKPOINT_ACK_DIGEST_DOMAIN, &checkpoint_ack)?;
        let result = MutationPublishResultV1 {
            payload: execution.publish_payload,
            graph_resync_required: false,
        };
        let receipt = BrainPromoteReconciliationReceiptV1 {
            schema: BRAIN_PROMOTE_RECONCILIATION_RECEIPT_SCHEMA.to_string(),
            operation_id: entry.operation_id.clone(),
            operation_object_digest: entry.prepare.operation_object_digest.clone(),
            source_brain_id: entry.prepare.route_selector.clone().ok_or_else(|| {
                ExternalMutationError::refused(
                    "brain_promote_route_selector_missing",
                    "promotion journal has no exact source-root selector",
                )
            })?,
            reconciliation_brain_id: recovery.reconciliation_brain_id,
            medulla_path: recovery.medulla.target_path.to_string_lossy().to_string(),
            medulla_postimage_sha256: recovery.medulla.after_sha256,
            adapter: "light".to_string(),
            mode: "merge".to_string(),
            incremental: false,
            namespace: "light".to_string(),
            ingest_output_digest: digest_canonical(
                "m1nd-brain-promote-reconciliation-ingest-output-v1",
                &execution.ingest_output,
            )?,
            graph_generation_before: execution.graph_generation_before,
            graph_generation_after: execution.graph_generation_after,
            checkpoint_ack,
            checkpoint_ack_digest,
            reconciled_at,
        };
        self.hit_fault("after_graph_checkpoint_ack")?;
        let published_result = seal_published_result(
            &entry.prepare.semantic_action,
            &entry.prepare.payload_digest,
            &entry.prepare.operation_object_digest,
            entry.outcome_digest.as_deref().ok_or_else(|| {
                ExternalMutationError::refused(
                    "external_mutation_committed_outcome_missing",
                    &entry.operation_id,
                )
            })?,
            &result,
        );
        self.open_journal()?.mark_reconciled(
            &entry.operation_id,
            receipt,
            published_result.clone(),
            reconciled_at,
        )?;
        Ok(published_result)
    }

    fn mark_promote_recovery_required_if_committed(&self, operation_id: &str) {
        if let Ok(mut journal) = self.open_journal() {
            let committed = journal.entry(operation_id).is_some_and(|entry| {
                matches!(
                    entry.phase,
                    ExternalMutationJournalPhaseV1::Committed
                        | ExternalMutationJournalPhaseV1::RecoveryRequired
                )
            });
            if committed {
                let _ = journal.mark_recovery_required(operation_id, (self.owner_clock)());
            }
        }
    }

    fn binding_is_committed(&self, lease_id: &str, operation_object_digest: &str) -> bool {
        self.open_journal()
            .ok()
            .and_then(|journal| journal.find_by_lease_and_object(lease_id, operation_object_digest))
            .is_some_and(|entry| {
                matches!(
                    entry.phase,
                    ExternalMutationJournalPhaseV1::Committed
                        | ExternalMutationJournalPhaseV1::RecoveryRequired
                        | ExternalMutationJournalPhaseV1::Reconciled
                )
            })
    }

    fn mark_binding_recovery_required_if_committed(
        &self,
        lease_id: &str,
        operation_object_digest: &str,
    ) {
        if let Ok(mut journal) = self.open_journal() {
            let operation_id = journal
                .find_by_lease_and_object(lease_id, operation_object_digest)
                .filter(|entry| {
                    matches!(
                        entry.phase,
                        ExternalMutationJournalPhaseV1::Committed
                            | ExternalMutationJournalPhaseV1::RecoveryRequired
                    )
                })
                .map(|entry| entry.operation_id);
            if let Some(operation_id) = operation_id {
                let _ = journal.mark_recovery_required(&operation_id, (self.owner_clock)());
            }
        }
    }

    /// Production replay coordinator. The boot seam supplies the exact hosted
    /// brain for stateful actions; promotion carries its full sealed filesystem
    /// plan in the journal. A journal COMMIT is first reconciled into broker
    /// CONSUMED, then its adapter is forward-completed idempotently, and only
    /// then may the journal advance to PUBLISHED.
    pub fn recover_pending<F>(
        &self,
        mut resolve_brain: F,
        reconcile_promote: Arc<BrainPromoteReconcilerV1>,
    ) -> Result<ExternalMutationRecoveryReportV1, ExternalMutationError>
    where
        F: FnMut(&str) -> Result<Arc<BrainSessionCell>, String>,
    {
        let _operation = self.broker_operation.lock();
        let mut broker = self.open_broker()?;
        self.recover_pending_with_broker(
            &mut broker,
            &mut resolve_brain,
            reconcile_promote.as_ref(),
        )
    }

    /// Global admission barrier. The caller holds both the process-local owner
    /// operation guard and the cross-process broker writer. Every replayable
    /// external mutation is converged before a new reservation may inspect or
    /// commit against domain state.
    fn recover_pending_with_broker<F>(
        &self,
        broker: &mut OwnerAuthorizationBrokerV1,
        _resolve_brain: &mut F,
        reconcile_promote: &BrainPromoteReconcilerV1,
    ) -> Result<ExternalMutationRecoveryReportV1, ExternalMutationError>
    where
        F: FnMut(&str) -> Result<Arc<BrainSessionCell>, String>,
    {
        let entries = {
            let journal = self.open_journal()?;
            journal.entries()
        };
        let journal_lease_ids = entries
            .iter()
            .map(|entry| entry.lease_id.clone())
            .collect::<BTreeSet<_>>();
        let mut report = ExternalMutationRecoveryReportV1::default();
        for mut entry in entries {
            report.scanned += 1;
            if entry.phase == ExternalMutationJournalPhaseV1::Prepared {
                match self.recover_prepared_entry_with_broker(broker, &entry, reconcile_promote)? {
                    PreparedRecoveryDispositionV1::Aborted => {
                        report.safely_aborted_pre_finalization += 1;
                        continue;
                    }
                    PreparedRecoveryDispositionV1::Pending => {
                        report.pending_uncertain += 1;
                        continue;
                    }
                    PreparedRecoveryDispositionV1::Advanced(current) => entry = *current,
                }
            }

            if entry.phase == ExternalMutationJournalPhaseV1::Published {
                if self.recover_broker_commit_with_broker(broker, &entry)? {
                    report.broker_recovered += 1;
                }
                report.already_published += 1;
                continue;
            }

            if entry.phase == ExternalMutationJournalPhaseV1::Reconciled {
                if self.recover_broker_commit_with_broker(broker, &entry)? {
                    report.broker_recovered += 1;
                }
                let published_result = entry.published_result.clone().ok_or_else(|| {
                    ExternalMutationError::refused(
                        "brain_promote_reconciled_result_missing",
                        &entry.operation_id,
                    )
                })?;
                self.open_journal()?.mark_published(
                    &entry.operation_id,
                    published_result,
                    (self.owner_clock)().max(entry.updated_at),
                )?;
                report.forward_completed += 1;
                continue;
            }

            if entry.prepare.semantic_action == "system_blocks.ratify" {
                let recovery: RatifyRecoveryPayloadV1 = serde_json::from_value(
                    entry.prepare.recovery_payload.clone(),
                )
                .map_err(|error| {
                    ExternalMutationError::refused(
                        "external_mutation_recovery_payload_invalid",
                        error.to_string(),
                    )
                })?;
                let staged = staged_ratify_from_recovery(recovery, &entry)?;
                let mut broker_recovered = false;
                let execution = self.run_ratify_actor_handshake(
                    &entry,
                    staged,
                    false,
                    reconcile_promote,
                    |_| {
                        broker_recovered =
                            self.recover_broker_commit_with_broker(broker, &entry)?;
                        Ok(())
                    },
                )?;
                if broker_recovered {
                    report.broker_recovered += 1;
                }
                let result = MutationPublishResultV1 {
                    payload: execution.publish_payload,
                    graph_resync_required: false,
                };
                let published_result = seal_published_result(
                    &entry.prepare.semantic_action,
                    &entry.prepare.payload_digest,
                    &entry.prepare.operation_object_digest,
                    entry.outcome_digest.as_deref().ok_or_else(|| {
                        ExternalMutationError::refused(
                            "external_mutation_committed_outcome_missing",
                            &entry.operation_id,
                        )
                    })?,
                    &result,
                );
                self.open_journal()?.mark_published(
                    &entry.operation_id,
                    published_result,
                    (self.owner_clock)().max(entry.updated_at),
                )?;
                report.forward_completed += 1;
                continue;
            }

            if entry.prepare.semantic_action == "brain.promote" {
                let recovery: PromoteRecoveryPayloadV1 = serde_json::from_value(
                    entry.prepare.recovery_payload.clone(),
                )
                .map_err(|error| {
                    ExternalMutationError::refused(
                        "external_mutation_recovery_payload_invalid",
                        error.to_string(),
                    )
                })?;
                let staged = staged_promote_from_recovery(recovery, &entry)?;
                if self.recover_broker_commit_with_broker(broker, &entry)? {
                    report.broker_recovered += 1;
                }
                let execution = self.run_promote_actor_handshake(
                    &entry,
                    staged,
                    false,
                    reconcile_promote,
                    |_| Ok(()),
                )?;
                let published_result = self.seal_promote_reconciliation(&entry, execution)?;
                self.hit_fault("after_graph_reconciled")?;
                let reconciled_entry = self
                    .open_journal()?
                    .entry(&entry.operation_id)
                    .cloned()
                    .ok_or_else(|| {
                        ExternalMutationError::refused(
                            "external_mutation_operation_not_found",
                            &entry.operation_id,
                        )
                    })?;
                self.open_journal()?.mark_published(
                    &entry.operation_id,
                    published_result,
                    (self.owner_clock)().max(reconciled_entry.updated_at),
                )?;
                report.forward_completed += 1;
                continue;
            }

            if matches!(
                entry.prepare.semantic_action.as_str(),
                "graph.ingest.replace" | "graph.ingest.merge_existing"
            ) {
                let recovery: GraphIngestA2RecoveryPayloadV1 = serde_json::from_value(
                    entry.prepare.recovery_payload.clone(),
                )
                .map_err(|error| {
                    ExternalMutationError::refused(
                        "graph_ingest_recovery_payload_invalid",
                        error.to_string(),
                    )
                })?;
                let staged = graph_ingest_a2::from_recovery(recovery, &entry, &self.journal_root)
                    .map_err(|error| {
                    ExternalMutationError::refused(error.code, error.detail)
                })?;
                if self.recover_broker_commit_with_broker(broker, &entry)? {
                    report.broker_recovered += 1;
                }
                let execution = self.run_graph_ingest_actor_handshake(
                    &entry,
                    staged,
                    false,
                    reconcile_promote,
                    |_| Ok(()),
                )?;
                let published_result = self.seal_graph_ingest_checkpoint(&entry, execution)?;
                self.hit_fault("after_graph_reconciled")?;
                let current_entry = self
                    .open_journal()?
                    .entry(&entry.operation_id)
                    .cloned()
                    .ok_or_else(|| {
                        ExternalMutationError::refused(
                            "external_mutation_operation_not_found",
                            &entry.operation_id,
                        )
                    })?;
                self.open_journal()?.mark_published(
                    &entry.operation_id,
                    published_result,
                    (self.owner_clock)().max(current_entry.updated_at),
                )?;
                report.forward_completed += 1;
                continue;
            }

            if entry.prepare.semantic_action == "source.edit.commit" {
                let recovery: SourceEditRecoveryPayloadV1 = serde_json::from_value(
                    entry.prepare.recovery_payload.clone(),
                )
                .map_err(|error| {
                    ExternalMutationError::refused(
                        "external_mutation_recovery_payload_invalid",
                        error.to_string(),
                    )
                })?;
                validate_source_edit_recovery_payload(&entry, &recovery)?;
                let mut broker_recovered = false;
                let execution = self.run_source_edit_recovery_actor_handshake(
                    &entry,
                    recovery,
                    reconcile_promote,
                    |_| {
                        broker_recovered =
                            self.recover_broker_commit_with_broker(broker, &entry)?;
                        Ok(())
                    },
                );
                let execution = match execution {
                    Ok(execution) => execution,
                    Err(error) => {
                        self.mark_binding_recovery_required_if_committed(
                            &entry.lease_id,
                            &entry.prepare.operation_object_digest,
                        );
                        return Err(ExternalMutationError::refused(
                            "external_mutation_recovery_required",
                            format!("committed source edit needs actor recovery: {error}"),
                        ));
                    }
                };
                if broker_recovered {
                    report.broker_recovered += 1;
                }
                let published_result = match self.seal_source_edit_checkpoint(&entry, execution) {
                    Ok(result) => result,
                    Err(error) => {
                        self.mark_binding_recovery_required_if_committed(
                            &entry.lease_id,
                            &entry.prepare.operation_object_digest,
                        );
                        return Err(ExternalMutationError::refused(
                            "external_mutation_recovery_required",
                            format!("source-edit recovery checkpoint needs recovery: {error}"),
                        ));
                    }
                };
                let current_entry = self
                    .open_journal()?
                    .entry(&entry.operation_id)
                    .cloned()
                    .ok_or_else(|| {
                        ExternalMutationError::refused(
                            "external_mutation_operation_not_found",
                            &entry.operation_id,
                        )
                    })?;
                self.open_journal()?.mark_published(
                    &entry.operation_id,
                    published_result,
                    (self.owner_clock)().max(current_entry.updated_at),
                )?;
                report.forward_completed += 1;
                continue;
            }

            let completion = self.forward_complete_committed_entry_with_broker(
                broker,
                &entry,
                reconcile_promote,
            )?;
            if completion.broker_recovered {
                report.broker_recovered += 1;
            }
            let result = completion.result.as_ref().ok_or_else(|| {
                ExternalMutationError::refused(
                    "external_mutation_publish_result_missing",
                    &entry.operation_id,
                )
            })?;
            let published_result = seal_published_result(
                &entry.prepare.semantic_action,
                &entry.prepare.payload_digest,
                &entry.prepare.operation_object_digest,
                entry.outcome_digest.as_deref().ok_or_else(|| {
                    ExternalMutationError::refused(
                        "external_mutation_committed_outcome_missing",
                        &entry.operation_id,
                    )
                })?,
                result,
            );
            let mut journal = self.open_journal()?;
            journal.mark_published(&entry.operation_id, published_result, (self.owner_clock)())?;
            report.forward_completed += 1;
        }
        let orphan_external_leases = broker
            .leases()
            .into_iter()
            .filter(|lease| {
                matches!(
                    lease.authorization_receipt.core.action.as_str(),
                    "system_blocks.ratify"
                        | "brain.promote"
                        | "source.edit.commit"
                        | "graph.ingest.replace"
                        | "graph.ingest.merge_existing"
                ) && !journal_lease_ids.contains(&lease.lease_id)
                    && lease.state == AuthorizationLeaseStateV1::Reserved
            })
            .map(|lease| lease.lease_id.clone())
            .collect::<Vec<_>>();
        for lease_id in orphan_external_leases {
            let lease = broker.lease(&lease_id).cloned().ok_or_else(|| {
                ExternalMutationError::refused("authorization_lease_not_found", &lease_id)
            })?;
            let reservation = lease.reservation.as_ref().ok_or_else(|| {
                ExternalMutationError::refused(
                    "external_mutation_orphan_reservation_invalid",
                    "reserved external lease has no reservation",
                )
            })?;
            let absence_witness = {
                let journal = self.open_journal()?;
                journal.verified_operation_absence_witness(
                    &reservation.reservation_id,
                    &lease.lease_id,
                    &reservation.operation_object_digest,
                    &lease.authorization_receipt.receipt_digest,
                )?
            };
            let recovered = broker.recover_external_reserved_without_journal(
                &lease_id,
                absence_witness,
                (self.owner_clock)(),
            )?;
            if recovered.state == AuthorizationLeaseStateV1::Aborted {
                report.safely_aborted_pre_finalization += 1;
            } else {
                return Err(ExternalMutationError::refused(
                    "external_mutation_orphan_abort_state_mismatch",
                    format!("orphan lease ended in {:?}", recovered.state),
                ));
            }
        }
        self.cleanup_source_orphans_without_journal(broker, &journal_lease_ids, reconcile_promote)?;
        self.cleanup_ratify_orphans_without_journal(broker, &journal_lease_ids, reconcile_promote)?;
        Ok(report)
    }

    /// `system_blocks.ratify` creates one reservation-derived sibling staging
    /// file before the outer PREPARED record exists. If the process dies in
    /// that narrow window, the protected broker/journal absence witness first
    /// transitions the lease to ABORTED above; only then may boot remove the
    /// exact owner-created scratch file. The managed store itself is never
    /// opened or changed by this cleanup path.
    fn cleanup_ratify_orphans_without_journal(
        &self,
        broker: &OwnerAuthorizationBrokerV1,
        journal_lease_ids: &BTreeSet<String>,
        reconciler: &BrainPromoteReconcilerV1,
    ) -> Result<(), ExternalMutationError> {
        for lease in broker.leases() {
            if lease.authorization_receipt.core.action.as_str() != "system_blocks.ratify"
                || journal_lease_ids.contains(&lease.lease_id)
                || lease.state != AuthorizationLeaseStateV1::Aborted
            {
                continue;
            }
            let reservation = lease.reservation.as_ref().ok_or_else(|| {
                ExternalMutationError::refused(
                    "ratify_orphan_reservation_invalid",
                    "aborted ratify lease has no exact reservation",
                )
            })?;
            let brain_id = lease.authorization_receipt.core.brain_id.as_str();
            self.run_maintenance_actor(
                brain_id,
                None,
                ExternalMutationMaintenanceOperationV1::RatifyOrphan {
                    reservation_id: reservation.reservation_id.clone(),
                },
                reconciler,
            )?;
        }
        Ok(())
    }

    fn cleanup_source_orphans_without_journal(
        &self,
        broker: &OwnerAuthorizationBrokerV1,
        journal_lease_ids: &BTreeSet<String>,
        reconciler: &BrainPromoteReconcilerV1,
    ) -> Result<(), ExternalMutationError> {
        let mut eligible_objects_by_brain = BTreeMap::<String, BTreeSet<String>>::new();
        for lease in broker.leases() {
            if lease.authorization_receipt.core.action.as_str() != "source.edit.commit"
                || journal_lease_ids.contains(&lease.lease_id)
                || !matches!(
                    lease.state,
                    AuthorizationLeaseStateV1::Reserved | AuthorizationLeaseStateV1::Aborted
                )
            {
                continue;
            }
            let reservation = lease.reservation.as_ref().ok_or_else(|| {
                ExternalMutationError::refused(
                    "source_edit_orphan_reservation_invalid",
                    "reserved/aborted source-edit lease has no exact reservation",
                )
            })?;
            eligible_objects_by_brain
                .entry(lease.authorization_receipt.core.brain_id.clone())
                .or_default()
                .insert(reservation.operation_object_digest.clone());
        }

        for (actor_brain_id, eligible_objects) in eligible_objects_by_brain {
            self.run_maintenance_actor(
                &actor_brain_id,
                None,
                ExternalMutationMaintenanceOperationV1::SourceOrphans { eligible_objects },
                reconciler,
            )?;
        }
        Ok(())
    }

    /// Boot may expose mutation services only after every replayable COMMIT is
    /// forward-complete and no callback-crossing remains ambiguous. A
    /// FINALIZATION_PREPARED broker record paired with a replay-verified journal
    /// PREPARED record is fail-closed here: the runtime is not exposed until an
    /// explicit negative-witness protocol or exact COMMIT witness resolves it.
    pub fn recover_for_boot<F>(
        &self,
        resolve_brain: F,
        reconcile_promote: Arc<BrainPromoteReconcilerV1>,
    ) -> Result<ExternalMutationRecoveryReportV1, ExternalMutationError>
    where
        F: FnMut(&str) -> Result<Arc<BrainSessionCell>, String>,
    {
        let report = self.recover_pending(resolve_brain, reconcile_promote)?;
        if report.pending_uncertain != 0 {
            return Err(ExternalMutationError::refused(
                "external_mutation_recovery_uncertain",
                format!(
                    "{} external mutation(s) crossed FINALIZATION_PREPARED without an exact COMMIT witness",
                    report.pending_uncertain
                ),
            ));
        }
        Ok(report)
    }

    fn run_maintenance_actor(
        &self,
        actor_brain_id: &str,
        route_selector: Option<&str>,
        operation: ExternalMutationMaintenanceOperationV1,
        reconciler: &BrainPromoteReconcilerV1,
    ) -> Result<(), ExternalMutationError> {
        let request = BrainPromoteReconciliationRequestV1 {
            operation_id: format!("maintenance:{actor_brain_id}"),
            operation_object_digest: String::new(),
            source_brain_id: actor_brain_id.to_string(),
            reconciliation_brain_id: actor_brain_id.to_string(),
            medulla_path: PathBuf::new(),
            medulla_postimage_sha256: String::new(),
            authority_subject_id: String::new(),
            job: ExternalMutationActorJobV1::Maintenance(Box::new(
                ExternalMutationMaintenanceActorJobV1 {
                    route_selector: route_selector.map(str::to_string),
                    operation,
                },
            )),
        };
        reconciler(request).map(|_| ()).map_err(|detail| {
            ExternalMutationError::refused("external_mutation_maintenance_actor_failed", detail)
        })
    }

    fn cleanup_prepared_entry(
        &self,
        entry: &ExternalMutationJournalEntryV1,
        reconciler: &BrainPromoteReconcilerV1,
    ) -> Result<(), ExternalMutationError> {
        match entry.prepare.semantic_action.as_str() {
            "system_blocks.ratify" => {
                let recovery: RatifyRecoveryPayloadV1 = serde_json::from_value(
                    entry.prepare.recovery_payload.clone(),
                )
                .map_err(|error| {
                    ExternalMutationError::refused(
                        "external_mutation_recovery_payload_invalid",
                        error.to_string(),
                    )
                })?;
                self.run_maintenance_actor(
                    &entry.prepare.actor_brain_id,
                    entry.prepare.route_selector.as_deref(),
                    ExternalMutationMaintenanceOperationV1::PreparedRatify {
                        recovery,
                        reservation_id: entry.reservation_id.clone(),
                    },
                    reconciler,
                )
            }
            "brain.promote" => {
                let recovery: PromoteRecoveryPayloadV1 = serde_json::from_value(
                    entry.prepare.recovery_payload.clone(),
                )
                .map_err(|error| {
                    ExternalMutationError::refused(
                        "external_mutation_recovery_payload_invalid",
                        error.to_string(),
                    )
                })?;
                if recovery.kind != "brain_promote"
                    || recovery.operation_object_digest != entry.prepare.operation_object_digest
                {
                    return Err(ExternalMutationError::refused(
                        "external_mutation_recovery_binding_mismatch",
                        "prepared promotion cleanup differs from the sealed operation",
                    ));
                }
                recovery.source.cleanup_unpublished()?;
                recovery.medulla.cleanup_unpublished()?;
                recovery.source_history.cleanup_unpublished()?;
                if let Some(history) = recovery.medulla_history {
                    history.cleanup_unpublished()?;
                }
                Ok(())
            }
            "source.edit.commit" => {
                let recovery: SourceEditRecoveryPayloadV1 = serde_json::from_value(
                    entry.prepare.recovery_payload.clone(),
                )
                .map_err(|error| {
                    ExternalMutationError::refused(
                        "external_mutation_recovery_payload_invalid",
                        error.to_string(),
                    )
                })?;
                validate_source_edit_recovery_payload(entry, &recovery)?;
                self.run_maintenance_actor(
                    &entry.prepare.actor_brain_id,
                    entry.prepare.route_selector.as_deref(),
                    ExternalMutationMaintenanceOperationV1::PreparedSource { recovery },
                    reconciler,
                )
            }
            "graph.ingest.replace" | "graph.ingest.merge_existing" => {
                let recovery: GraphIngestA2RecoveryPayloadV1 = serde_json::from_value(
                    entry.prepare.recovery_payload.clone(),
                )
                .map_err(|error| {
                    ExternalMutationError::refused(
                        "graph_ingest_recovery_payload_invalid",
                        error.to_string(),
                    )
                })?;
                if recovery.kind != graph_ingest_a2::GRAPH_INGEST_A2_RECOVERY_KIND
                    || recovery.semantic_payload.mode.semantic_action()
                        != entry.prepare.semantic_action
                    || recovery.operation_object_digest != entry.prepare.operation_object_digest
                    || recovery.ownership_manifest.coverage
                        != m1nd_ingest::ownership::OwnershipCoverageV1::Complete
                    || !recovery
                        .ownership_manifest
                        .verify_receipt()
                        .map_err(|error| {
                            ExternalMutationError::refused(
                                "graph_ingest_recovery_payload_invalid",
                                error.to_string(),
                            )
                        })?
                {
                    return Err(ExternalMutationError::refused(
                        "graph_ingest_recovery_binding_mismatch",
                        "prepared A2 payload differs from its sealed full-root operation",
                    ));
                }
                // A2 stages no target bytes before PREPARED. The negative
                // journal witness can abort the lease without domain cleanup.
                Ok(())
            }
            action => Err(ExternalMutationError::refused(
                "external_mutation_recovery_action_unknown",
                action,
            )),
        }
    }

    fn recover_prepared_entry_with_broker(
        &self,
        broker: &mut OwnerAuthorizationBrokerV1,
        entry: &ExternalMutationJournalEntryV1,
        reconciler: &BrainPromoteReconcilerV1,
    ) -> Result<PreparedRecoveryDispositionV1, ExternalMutationError> {
        // Normative cross-process order: broker -> protected external journal
        // -> target-private cleanup. Holding the broker lock proves no finalize
        // callback is active while the journal establishes exact no-COMMIT.
        let journal = self.open_journal()?;
        let current_entry = journal.entry(&entry.operation_id).cloned().ok_or_else(|| {
            ExternalMutationError::refused(
                "external_mutation_operation_not_found",
                &entry.operation_id,
            )
        })?;
        if current_entry.phase != ExternalMutationJournalPhaseV1::Prepared {
            drop(journal);
            return Ok(PreparedRecoveryDispositionV1::Advanced(Box::new(
                current_entry,
            )));
        }
        let negative_witness = journal.verified_prepared_abort_witness(&entry.operation_id)?;
        drop(journal);
        let recovered = broker.recover_external_prepared_without_commit(
            &entry.lease_id,
            negative_witness,
            (self.owner_clock)(),
        )?;
        let state = recovered.state;
        if state == AuthorizationLeaseStateV1::Aborted {
            self.cleanup_prepared_entry(&current_entry, reconciler)?;
        }
        match state {
            AuthorizationLeaseStateV1::Aborted => Ok(PreparedRecoveryDispositionV1::Aborted),
            AuthorizationLeaseStateV1::Reserved => Ok(PreparedRecoveryDispositionV1::Pending),
            _ => Err(ExternalMutationError::refused(
                "external_mutation_recovery_state_mismatch",
                format!("journal remained PREPARED while broker became {:?}", state),
            )),
        }
    }

    fn forward_complete_committed_entry_with_broker(
        &self,
        broker: &mut OwnerAuthorizationBrokerV1,
        entry: &ExternalMutationJournalEntryV1,
        reconciler: &BrainPromoteReconcilerV1,
    ) -> Result<ForwardCompletionV1, ExternalMutationError> {
        match entry.prepare.semantic_action.as_str() {
            "system_blocks.ratify" => {
                let recovery: RatifyRecoveryPayloadV1 = serde_json::from_value(
                    entry.prepare.recovery_payload.clone(),
                )
                .map_err(|error| {
                    ExternalMutationError::refused(
                        "external_mutation_recovery_payload_invalid",
                        error.to_string(),
                    )
                })?;
                let staged = staged_ratify_from_recovery(recovery, entry)?;
                let mut broker_recovered = false;
                let execution = self.run_ratify_actor_handshake(
                    entry,
                    staged,
                    false,
                    reconciler,
                    |_| {
                        broker_recovered =
                            self.recover_broker_commit_with_broker(broker, entry)?;
                        Ok(())
                    },
                )?;
                Ok(ForwardCompletionV1 {
                    broker_recovered,
                    result: Some(MutationPublishResultV1 {
                        payload: execution.publish_payload,
                        graph_resync_required: false,
                    }),
                })
            }
            "source.edit.commit" => {
                Err(ExternalMutationError::refused(
                    "source_edit_generic_recovery_forbidden",
                    "source-edit recovery must cross the guarded selected-brain actor checkpoint handshake",
                ))
            }
            "brain.promote" => Err(ExternalMutationError::refused(
                "brain_promote_generic_recovery_forbidden",
                "promotion recovery must cross the guarded actor handshake",
            )),
            "graph.ingest.replace" | "graph.ingest.merge_existing" => {
                Err(ExternalMutationError::refused(
                    "graph_ingest_generic_recovery_forbidden",
                    "A2 graph recovery must cross the guarded actor checkpoint handshake",
                ))
            }
            action => Err(ExternalMutationError::refused(
                "external_mutation_recovery_action_unknown",
                action,
            )),
        }
    }

    pub fn conservation_scan(
        &self,
    ) -> Result<ExternalMutationConservationReportV1, ExternalMutationError> {
        let _operation = self.broker_operation.lock();
        let broker = self.open_broker()?;
        let journal = self.open_journal()?;
        let entries = journal.entries();
        let journal_lease_ids = entries
            .iter()
            .map(|entry| entry.lease_id.clone())
            .collect::<BTreeSet<_>>();
        let mut report = ExternalMutationConservationReportV1 {
            journal_entries: entries.len(),
            ..ExternalMutationConservationReportV1::default()
        };
        for entry in entries {
            match entry.phase {
                ExternalMutationJournalPhaseV1::Prepared => report.prepared += 1,
                ExternalMutationJournalPhaseV1::Committed
                | ExternalMutationJournalPhaseV1::RecoveryRequired
                | ExternalMutationJournalPhaseV1::Reconciled => report.committed_or_recovery += 1,
                ExternalMutationJournalPhaseV1::Published => report.published += 1,
            }
            let Some(lease) = broker.lease(&entry.lease_id) else {
                report
                    .anomalies
                    .push(format!("{}:missing_broker_lease", entry.operation_id));
                continue;
            };
            let anomalies = entry_lease_conservation_anomalies(&entry, lease);
            if anomalies.is_empty() {
                report.broker_bound_entries += 1;
            } else {
                report.anomalies.extend(anomalies);
            }
        }
        for lease in broker.leases() {
            if matches!(
                lease.authorization_receipt.core.action.as_str(),
                "system_blocks.ratify"
                    | "brain.promote"
                    | "source.edit.commit"
                    | "graph.ingest.replace"
                    | "graph.ingest.merge_existing"
            ) && matches!(
                lease.state,
                AuthorizationLeaseStateV1::Reserved | AuthorizationLeaseStateV1::Consumed
            ) && !journal_lease_ids.contains(&lease.lease_id)
            {
                report
                    .anomalies
                    .push(format!("{}:external_lease_without_journal", lease.lease_id));
            }
        }
        Ok(report)
    }

    fn recover_broker_commit_with_broker(
        &self,
        broker: &mut OwnerAuthorizationBrokerV1,
        entry: &ExternalMutationJournalEntryV1,
    ) -> Result<bool, ExternalMutationError> {
        let was_reserved = broker
            .lease(&entry.lease_id)
            .is_some_and(|lease| lease.state == AuthorizationLeaseStateV1::Reserved);
        let witness = {
            let journal = self.open_journal()?;
            journal.verified_commit_witness(&entry.operation_id)?
        };
        let lease = broker.recover_external_reserved(
            &entry.lease_id,
            Some(witness),
            (self.owner_clock)(),
        )?;
        let terminal_matches = lease.terminal.as_ref().is_some_and(|terminal| {
            terminal
                .external_mutation_witness
                .as_ref()
                .is_some_and(|witness| {
                    Some(witness.journal_record_digest.as_str())
                        == entry.commit_record_digest.as_deref()
                        && witness.operation_object_digest == entry.prepare.operation_object_digest
                        && witness.reservation_id == entry.reservation_id
                })
        });
        if lease.state != AuthorizationLeaseStateV1::Consumed || !terminal_matches {
            return Err(ExternalMutationError::refused(
                "external_mutation_broker_recovery_mismatch",
                "journal COMMIT does not conserve into the exact broker terminal",
            ));
        }
        Ok(was_reserved)
    }

    fn open_broker(&self) -> Result<OwnerAuthorizationBrokerV1, ExternalMutationError> {
        OwnerAuthorizationBrokerV1::open_with_protected_head(
            self.broker_config.clone(),
            self.linearization.clone(),
            Arc::clone(&self.protected_journal_head),
        )
        .map_err(ExternalMutationError::Broker)
    }

    fn open_journal(&self) -> Result<ExternalMutationJournalV1, ExternalMutationError> {
        ExternalMutationJournalV1::open(
            &self.journal_root,
            Some(Arc::clone(&self.protected_journal_head)),
        )
        .map_err(ExternalMutationError::Journal)
    }
}

fn validate_source_edit_recovery_payload(
    entry: &ExternalMutationJournalEntryV1,
    recovery: &SourceEditRecoveryPayloadV1,
) -> Result<(), ExternalMutationError> {
    if recovery.kind != "source_edit_commit"
        || !recovery.graph_resync_required
        || !recovery.forward_complete_committed
        || recovery.preview_id.trim().is_empty()
        || recovery.reconciliation_brain_id.trim().is_empty()
        || recovery.operation_object_digest != entry.prepare.operation_object_digest
        || (entry.phase == ExternalMutationJournalPhaseV1::Prepared
            && entry.outcome_digest.is_some())
        || (entry.phase != ExternalMutationJournalPhaseV1::Prepared
            && entry.outcome_digest.as_deref() != Some(recovery.stage_digest.as_str()))
        || !is_digest(&recovery.transaction_id)
        || !is_digest(&recovery.operation_object_digest)
        || !is_digest(&recovery.stage_digest)
    {
        return Err(ExternalMutationError::refused(
            "external_mutation_recovery_binding_mismatch",
            "source edit recovery payload differs from its sealed outer operation",
        ));
    }
    Ok(())
}

fn entry_lease_conservation_anomalies(
    entry: &ExternalMutationJournalEntryV1,
    lease: &OwnerAuthorizationLeaseV1,
) -> Vec<String> {
    let mut anomalies = Vec::new();
    let operation_id = entry.operation_id.as_str();
    let receipt = &lease.authorization_receipt;
    let reservation_matches = lease.reservation.as_ref().is_some_and(|reservation| {
        reservation.reservation_id == entry.reservation_id
            && reservation.lease_id == entry.lease_id
            && reservation.operation_object_digest == entry.prepare.operation_object_digest
    });
    if !reservation_matches {
        anomalies.push(format!("{operation_id}:reservation_binding_mismatch"));
    }
    if lease.lease_id != entry.lease_id
        || receipt.receipt_digest != entry.authorization_snapshot_digest
        || receipt.core.verified_object_digest != entry.prepare.operation_object_digest
        || receipt.core.action.as_str() != entry.prepare.semantic_action
        || receipt.core.brain_id != entry.prepare.actor_brain_id
        || receipt.core.mission_id != entry.prepare.mission_id
        || receipt.core.mission_head_id != entry.prepare.mission_head_id
    {
        anomalies.push(format!("{operation_id}:authorization_binding_mismatch"));
    }

    let complete_commit = entry.commit_record_digest.is_some()
        && entry.outcome_digest.is_some()
        && entry.committed_at.is_some();
    let partial_commit = entry.commit_record_digest.is_some()
        || entry.outcome_digest.is_some()
        || entry.committed_at.is_some();
    match entry.phase {
        ExternalMutationJournalPhaseV1::Prepared => {
            if partial_commit {
                anomalies.push(format!("{operation_id}:prepared_carries_commit_fields"));
            }
            match lease.state {
                AuthorizationLeaseStateV1::Reserved => {
                    if lease.terminal.is_some() {
                        anomalies.push(format!("{operation_id}:reserved_has_terminal"));
                    }
                }
                AuthorizationLeaseStateV1::Aborted => {
                    let exact_abort = lease.terminal.as_ref().is_some_and(|terminal| {
                        let expected_reason = digest_canonical(
                            "m1nd-owner-external-prepared-abort-v1",
                            &(
                                entry.lease_id.as_str(),
                                entry.reservation_id.as_str(),
                                entry.prepare.operation_object_digest.as_str(),
                                entry.authorization_snapshot_digest.as_str(),
                                entry.prepared_at,
                                terminal.terminal_at,
                            ),
                        )
                        .ok();
                        terminal.kind == AuthorizationTerminalKindV1::Aborted
                            && terminal.wal_witness.is_none()
                            && terminal.external_mutation_witness.is_none()
                            && terminal.terminal_at >= entry.prepared_at
                            && expected_reason.as_deref() == Some(terminal.outcome_digest.as_str())
                    });
                    if !exact_abort {
                        anomalies.push(format!("{operation_id}:prepared_abort_not_exact"));
                    }
                }
                AuthorizationLeaseStateV1::Consumed => {
                    anomalies.push(format!("{operation_id}:prepared_with_consumed_lease"));
                }
                AuthorizationLeaseStateV1::Unused => {
                    anomalies.push(format!("{operation_id}:prepared_with_unused_lease"));
                }
            }
        }
        ExternalMutationJournalPhaseV1::Committed
        | ExternalMutationJournalPhaseV1::RecoveryRequired
        | ExternalMutationJournalPhaseV1::Reconciled
        | ExternalMutationJournalPhaseV1::Published => {
            if !complete_commit {
                anomalies.push(format!("{operation_id}:committed_fields_incomplete"));
            }
            if lease.finalization_snapshot.is_none() {
                anomalies.push(format!(
                    "{operation_id}:commit_without_finalization_snapshot"
                ));
            }
            match lease.state {
                AuthorizationLeaseStateV1::Reserved => {
                    if matches!(
                        entry.phase,
                        ExternalMutationJournalPhaseV1::Reconciled
                            | ExternalMutationJournalPhaseV1::Published
                    ) {
                        anomalies.push(format!("{operation_id}:published_with_reserved_lease"));
                    }
                    if lease.terminal.is_some() {
                        anomalies.push(format!("{operation_id}:reserved_has_terminal"));
                    }
                }
                AuthorizationLeaseStateV1::Consumed => {
                    let exact_terminal = match (
                        lease.terminal.as_ref(),
                        entry.commit_record_digest.as_deref(),
                        entry.committed_at,
                    ) {
                        (Some(terminal), Some(commit_record_digest), Some(committed_at)) => {
                            terminal.kind == AuthorizationTerminalKindV1::ExternalMutationCommitted
                                && terminal.outcome_digest == commit_record_digest
                                && terminal.wal_witness.is_none()
                                && terminal.terminal_at >= committed_at
                                && lease
                                    .finalization_snapshot
                                    .as_ref()
                                    .is_some_and(|snapshot| snapshot.revalidated_at == committed_at)
                                && terminal.external_mutation_witness.as_ref().is_some_and(
                                    |witness| {
                                        witness.lease_id == entry.lease_id
                                            && witness.reservation_id == entry.reservation_id
                                            && witness.operation_object_digest
                                                == entry.prepare.operation_object_digest
                                            && witness.authorization_snapshot_digest
                                                == entry.authorization_snapshot_digest
                                            && witness.journal_record_digest == commit_record_digest
                                            && witness.committed_at == committed_at
                                    },
                                )
                        }
                        _ => false,
                    };
                    if !exact_terminal {
                        anomalies.push(format!("{operation_id}:consumed_terminal_mismatch"));
                    }
                }
                AuthorizationLeaseStateV1::Unused => {
                    anomalies.push(format!("{operation_id}:commit_with_unused_lease"));
                }
                AuthorizationLeaseStateV1::Aborted => {
                    anomalies.push(format!("{operation_id}:commit_with_aborted_lease"));
                }
            }
        }
    }
    anomalies
}

struct MutationPublishResultV1 {
    payload: Value,
    graph_resync_required: bool,
}

struct ForwardCompletionV1 {
    broker_recovered: bool,
    result: Option<MutationPublishResultV1>,
}

fn seal_published_result(
    semantic_action: &str,
    semantic_payload_digest: &str,
    operation_object_digest: &str,
    outcome_digest: &str,
    result: &MutationPublishResultV1,
) -> ExternalMutationPublishedResultV1 {
    let actor_reconciled = matches!(
        semantic_action,
        "brain.promote" | "graph.ingest.replace" | "graph.ingest.merge_existing"
    );
    let graph_resync_required = if actor_reconciled {
        false
    } else {
        result.graph_resync_required
    };
    ExternalMutationPublishedResultV1 {
        schema: EXTERNAL_MUTATION_PUBLISHED_RESULT_SCHEMA.to_string(),
        semantic_action: semantic_action.to_string(),
        semantic_payload_digest: semantic_payload_digest.to_string(),
        operation_object_digest: operation_object_digest.to_string(),
        outcome_digest: outcome_digest.to_string(),
        graph_resync_required,
        reconciliation_state: if actor_reconciled {
            "RECONCILED".to_string()
        } else if graph_resync_required {
            "PENDING_RECONCILIATION".to_string()
        } else {
            "NOT_REQUIRED".to_string()
        },
        result: result.payload.clone(),
    }
}

fn response_from_sealed_result(
    request_id: &str,
    lease_id: &str,
    reservation_id: &str,
    journal_operation_id: &str,
    sealed: &ExternalMutationPublishedResultV1,
    terminal_replay: bool,
) -> ExternalMutationResponseV1 {
    let mut result = sealed.result.clone();
    if terminal_replay {
        if let Some(object) = result.as_object_mut() {
            object.insert("terminal_replay".to_string(), Value::Bool(true));
        }
    }
    ExternalMutationResponseV1 {
        schema: EXTERNAL_MUTATION_RESPONSE_SCHEMA.to_string(),
        request_id: request_id.to_string(),
        semantic_action: sealed.semantic_action.clone(),
        semantic_payload_digest: sealed.semantic_payload_digest.clone(),
        operation_object_digest: sealed.operation_object_digest.clone(),
        authorization_lease_id: lease_id.to_string(),
        authorization_reservation_id: reservation_id.to_string(),
        journal_operation_id: journal_operation_id.to_string(),
        outcome_digest: sealed.outcome_digest.clone(),
        graph_resync_required: sealed.graph_resync_required,
        reconciliation_state: sealed.reconciliation_state.clone(),
        result,
    }
}

enum InspectedMutationV1 {
    Ratify(Box<InspectedRatifyV1>),
    Promote(Box<InspectedPromoteV1>),
    SourceEdit(Box<InspectedSourceEditV1>),
    GraphIngestSnapshot(Box<GraphIngestA2InspectionSnapshotV1>),
    GraphIngest(Box<InspectedGraphIngestA2V1>),
}

impl InspectedMutationV1 {
    fn semantic_payload_digest(&self) -> &str {
        match self {
            Self::Ratify(value) => &value.semantic_payload_digest,
            Self::Promote(value) => &value.semantic_payload_digest,
            Self::SourceEdit(value) => &value.intent.semantic_payload_digest,
            Self::GraphIngestSnapshot(_) => {
                unreachable!("graph-ingest actor snapshots must be completed off actor")
            }
            Self::GraphIngest(value) => &value.semantic_payload_digest,
        }
    }

    fn stage(
        self,
        reservation: &AuthorizationReservationV1,
        operation_object_digest: &str,
        journal_root: &Path,
    ) -> Result<StagedMutationV1, ExternalMutationError> {
        match self {
            Self::Ratify(value) => value.stage(reservation, operation_object_digest),
            Self::Promote(value) => value.stage(reservation, operation_object_digest),
            Self::SourceEdit(_) => Err(ExternalMutationError::refused(
                "source_edit_actor_required",
                "source edit must stage and publish inside one selected-brain actor turn",
            )),
            Self::GraphIngestSnapshot(_) => Err(ExternalMutationError::refused(
                "graph_ingest_scan_incomplete",
                "graph-ingest actor snapshot was not completed by the off-actor scanner",
            )),
            Self::GraphIngest(value) => graph_ingest_a2::stage(
                *value,
                operation_object_digest,
                journal_root,
                &reservation.reservation_id,
            )
            .map(|staged| StagedMutationV1::GraphIngest(Box::new(staged)))
            .map_err(|error| ExternalMutationError::refused(error.code, error.detail)),
        }
    }
}

struct InspectedRatifyV1 {
    target_path: PathBuf,
    original_sha256: String,
    next_bytes: Vec<u8>,
    summary: RatifySummary,
    semantic_payload_digest: String,
    reconciliation_brain_id: String,
}

impl InspectedRatifyV1 {
    fn stage(
        self,
        reservation: &AuthorizationReservationV1,
        operation_object_digest: &str,
    ) -> Result<StagedMutationV1, ExternalMutationError> {
        let staging_path = sibling_staging_path(
            &self.target_path,
            &reservation.reservation_id,
            "system-blocks-ratify",
        )?;
        write_staging_file(&staging_path, &self.next_bytes)?;
        let next_sha256 = sha256_bytes(&self.next_bytes);
        Ok(StagedMutationV1::Ratify(StagedRatifyV1 {
            target_path: self.target_path,
            staging_path,
            original_sha256: self.original_sha256,
            next_sha256,
            operation_object_digest: operation_object_digest.to_string(),
            summary: self.summary,
            reconciliation_brain_id: self.reconciliation_brain_id,
        }))
    }
}

struct InspectedPromoteV1 {
    input: crate::promote_handlers::PromoteInput,
    paths: ExternalPromotePathsV1,
    source_path: PathBuf,
    medulla_path: PathBuf,
    source_slug: String,
    medulla_slug: String,
    source_sha256: String,
    medulla_sha256: Option<String>,
    semantic_payload_digest: String,
    reconciliation_brain_id: String,
    plan: crate::promote_handlers::ExternalPromotionPlanV1,
}

impl InspectedPromoteV1 {
    fn stage(
        self,
        reservation: &AuthorizationReservationV1,
        operation_object_digest: &str,
    ) -> Result<StagedMutationV1, ExternalMutationError> {
        let source_post_sha256 = sha256_bytes(self.plan.source_postimage.as_bytes());
        let medulla_post_sha256 = sha256_bytes(self.plan.medulla_postimage.as_bytes());
        let source_history_sha256 = sha256_bytes(self.plan.source_history.as_bytes());
        let medulla_history_sha256 = self
            .plan
            .medulla_history
            .as_deref()
            .map(|bytes| sha256_bytes(bytes.as_bytes()));

        let source_staging_path = sibling_staging_path(
            &self.source_path,
            &reservation.reservation_id,
            "brain-promote-source",
        )?;
        let medulla_staging_path = sibling_staging_path(
            &self.medulla_path,
            &reservation.reservation_id,
            "brain-promote-medulla",
        )?;
        let source_history_path = promotion_history_path(
            &self.paths.source_store_dir,
            &self.plan.source_slug,
            self.plan.promoted_at_ms,
        )?;
        let source_history_staging_path = sibling_staging_path(
            &source_history_path,
            &reservation.reservation_id,
            "brain-promote-source-history",
        )?;
        let (medulla_history_path, medulla_history_staging_path) =
            if self.plan.medulla_history.is_some() {
                let path = promotion_history_path(
                    &self.paths.medulla_store_dir,
                    &self.plan.medulla_slug,
                    self.plan.promoted_at_ms,
                )?;
                let staging = sibling_staging_path(
                    &path,
                    &reservation.reservation_id,
                    "brain-promote-medulla-history",
                )?;
                (Some(path), Some(staging))
            } else {
                (None, None)
            };

        write_staging_file(&source_staging_path, self.plan.source_postimage.as_bytes())?;
        write_staging_file(
            &medulla_staging_path,
            self.plan.medulla_postimage.as_bytes(),
        )?;
        write_staging_file(
            &source_history_staging_path,
            self.plan.source_history.as_bytes(),
        )?;
        if let (Some(staging), Some(history)) = (
            medulla_history_staging_path.as_ref(),
            self.plan.medulla_history.as_ref(),
        ) {
            write_staging_file(staging, history.as_bytes())?;
        }

        let outcome_digest = digest_canonical(
            "m1nd-external-promote-staged-outcome-v1",
            &(
                operation_object_digest,
                self.input.agent_id.as_str(),
                self.input.brain.as_str(),
                self.input.claim.as_str(),
                self.input.reason.as_str(),
                self.source_sha256.as_str(),
                self.medulla_sha256.as_deref(),
                source_post_sha256.as_str(),
                medulla_post_sha256.as_str(),
                source_history_sha256.as_str(),
                medulla_history_sha256.as_deref(),
                self.plan.promoted_at_ms,
                self.reconciliation_brain_id.as_str(),
            ),
        )?;
        Ok(StagedMutationV1::Promote(Box::new(StagedPromoteV1 {
            input: self.input,
            paths: self.paths,
            source: StagedFileV1 {
                target_path: self.source_path,
                staging_path: source_staging_path,
                expected_before_sha256: Some(self.source_sha256),
                after_sha256: source_post_sha256,
            },
            medulla: StagedFileV1 {
                target_path: self.medulla_path,
                staging_path: medulla_staging_path,
                expected_before_sha256: self.medulla_sha256,
                after_sha256: medulla_post_sha256,
            },
            source_history: StagedFileV1 {
                target_path: source_history_path,
                staging_path: source_history_staging_path,
                expected_before_sha256: None,
                after_sha256: source_history_sha256,
            },
            medulla_history: match (
                medulla_history_path,
                medulla_history_staging_path,
                medulla_history_sha256,
            ) {
                (Some(target_path), Some(staging_path), Some(after_sha256)) => Some(StagedFileV1 {
                    target_path,
                    staging_path,
                    expected_before_sha256: None,
                    after_sha256,
                }),
                (None, None, None) => None,
                _ => {
                    return Err(ExternalMutationError::refused(
                        "brain_promote_stage_incomplete",
                        "medulla history staging bindings are incomplete",
                    ))
                }
            },
            source_slug: self.plan.source_slug,
            medulla_slug: self.plan.medulla_slug,
            origin_brain: self.plan.origin_brain,
            origin_qualified: self.plan.origin_qualified,
            evidence_unverifiable: self.plan.evidence_unverifiable,
            promoted_at_ms: self.plan.promoted_at_ms,
            reconciliation_brain_id: self.reconciliation_brain_id,
            operation_object_digest: operation_object_digest.to_string(),
            outcome_digest,
        })))
    }
}

struct InspectedSourceEditV1 {
    request: crate::surgical_handlers::SourceEditCommitRequestV1,
    intent: crate::surgical_handlers::SourceEditCommitIntentV1,
    authority_subject_id: String,
    reconciliation_brain_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StagedFileV1 {
    target_path: PathBuf,
    staging_path: PathBuf,
    expected_before_sha256: Option<String>,
    after_sha256: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RatifyRecoveryPayloadV1 {
    kind: String,
    #[serde(default)]
    reconciliation_brain_id: String,
    target_path: PathBuf,
    staging_path: PathBuf,
    original_sha256: String,
    next_sha256: String,
    ratified_block_ids: Vec<String>,
    store_version: u64,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceEditRecoveryPayloadV1 {
    kind: String,
    preview_id: String,
    transaction_id: String,
    operation_object_digest: String,
    stage_digest: String,
    reconciliation_brain_id: String,
    graph_resync_required: bool,
    forward_complete_committed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PromoteRecoveryInputV1 {
    authority_subject_id: String,
    brain: String,
    claim: String,
    reason: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PromoteRecoveryPayloadV1 {
    kind: String,
    input: PromoteRecoveryInputV1,
    source: StagedFileV1,
    medulla: StagedFileV1,
    source_history: StagedFileV1,
    medulla_history: Option<StagedFileV1>,
    source_slug: String,
    medulla_slug: String,
    origin_brain: String,
    origin_qualified: bool,
    evidence_unverifiable: bool,
    promoted_at_ms: u64,
    reconciliation_brain_id: String,
    operation_object_digest: String,
    outcome_digest: String,
    forward_complete: bool,
}

#[derive(Clone)]
struct StagedPromoteV1 {
    input: crate::promote_handlers::PromoteInput,
    paths: ExternalPromotePathsV1,
    source: StagedFileV1,
    medulla: StagedFileV1,
    source_history: StagedFileV1,
    medulla_history: Option<StagedFileV1>,
    source_slug: String,
    medulla_slug: String,
    origin_brain: String,
    origin_qualified: bool,
    evidence_unverifiable: bool,
    promoted_at_ms: u64,
    reconciliation_brain_id: String,
    operation_object_digest: String,
    outcome_digest: String,
}

#[derive(Debug)]
struct BrainPromoteActorReadyV1 {
    operation_id: String,
    operation_object_digest: String,
    outcome_digest: String,
    medulla_path: PathBuf,
    medulla_postimage_sha256: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BrainPromoteActorDecisionV1 {
    Committed,
    Abort,
}

struct GraphIngestPreviewActorJobV1 {
    request: GraphIngestPreviewRequestV1,
    journal_entries: Vec<ExternalMutationJournalEntryV1>,
    actor_brain_id: String,
    reply_tx: SyncSender<
        Result<(GraphIngestA2InputV1, GraphIngestA2InspectionSnapshotV1), ExternalMutationError>,
    >,
}

impl GraphIngestPreviewActorJobV1 {
    fn execute(
        self,
        state: &mut crate::session::SessionState,
    ) -> Result<BrainPromoteReconciliationExecutionV1, String> {
        let graph_generation = state.graph_generation;
        let placeholder_preview_id = "0".repeat(64);
        let result = graph_ingest_a2::owner_derived_input(
            state,
            placeholder_preview_id,
            self.request.include_dotfiles,
            self.request.dotfile_patterns,
            self.request.parent,
        )
        .and_then(|input| {
            graph_ingest_a2::capture_inspection_snapshot(
                state,
                &input,
                self.request.mode,
                &self.journal_entries,
                &self.actor_brain_id,
                self.actor_brain_id.clone(),
                String::new(),
            )
            .map(|snapshot| (input, snapshot))
        })
        .map_err(|error| ExternalMutationError::refused(error.code, error.detail));
        self.reply_tx
            .send(result)
            .map_err(|_| "graph-ingest preview owner dropped before actor reply".to_string())?;
        Ok(BrainPromoteReconciliationExecutionV1::empty(
            graph_generation,
        ))
    }
}

struct ExternalMutationInspectActorJobV1 {
    request: ExternalMutationRequestV1,
    authority_subject_id: String,
    owner_now_ms: u64,
    journal_entries: Vec<ExternalMutationJournalEntryV1>,
    brain_id: String,
    reconciliation_brain_id: String,
    promote_paths: Option<ExternalPromotePathsV1>,
    reply_tx: SyncSender<Result<InspectedMutationV1, ExternalMutationError>>,
}

impl ExternalMutationInspectActorJobV1 {
    fn runs_on_source_brain_actor(&self) -> bool {
        matches!(
            self.request,
            ExternalMutationRequestV1::SystemBlocksRatify { .. }
                | ExternalMutationRequestV1::SourceEditCommit { .. }
                | ExternalMutationRequestV1::GraphIngestReplace { .. }
                | ExternalMutationRequestV1::GraphIngestMergeExisting { .. }
        )
    }

    fn execute(
        self,
        state: &mut crate::session::SessionState,
    ) -> Result<BrainPromoteReconciliationExecutionV1, String> {
        let graph_generation = state.graph_generation;
        match inspect_request_in_state(
            &self.request,
            state,
            self.promote_paths,
            &self.reconciliation_brain_id,
            &self.authority_subject_id,
            self.owner_now_ms,
            &self.journal_entries,
            &self.brain_id,
        ) {
            Ok(inspected) => {
                self.reply_tx
                    .send(Ok(inspected))
                    .map_err(|_| "inspection owner dropped before actor reply".to_string())?;
                Ok(BrainPromoteReconciliationExecutionV1::empty(
                    graph_generation,
                ))
            }
            Err(error) => {
                self.reply_tx
                    .send(Err(error))
                    .map_err(|_| "inspection owner dropped before actor reply".to_string())?;
                Ok(BrainPromoteReconciliationExecutionV1::empty(
                    graph_generation,
                ))
            }
        }
    }
}

#[derive(Clone, Debug)]
struct RatifyActorReadyV1 {
    operation_id: String,
    operation_object_digest: String,
    outcome_digest: String,
    target_path: PathBuf,
    next_sha256: String,
    reconciliation_brain_id: String,
}

struct RatifyActorJobV1 {
    staged: StagedRatifyV1,
    source_brain_id: String,
    reconciliation_brain_id: String,
    operation_id: String,
    reservation_id: String,
    require_original_preimage: bool,
    fault_hook: Arc<ExternalMutationFaultHookV1>,
    ready_tx: SyncSender<Result<RatifyActorReadyV1, String>>,
    decision_rx: Receiver<BrainPromoteActorDecisionV1>,
}

impl RatifyActorJobV1 {
    fn execute(
        self,
        state: &mut crate::session::SessionState,
    ) -> Result<BrainPromoteReconciliationExecutionV1, String> {
        let graph_generation = state.graph_generation;
        if !self.source_brain_id.is_empty() && !state.covers_root(&self.source_brain_id) {
            let detail = "resolved ratify actor does not own the selected brain root".to_string();
            let _ = self.ready_tx.send(Err(detail.clone()));
            return Err(detail);
        }
        let expected_target = SystemBlockStore::path_in(&state.runtime_root);
        let expected_staging = sibling_staging_path(
            &expected_target,
            &self.reservation_id,
            "system-blocks-ratify",
        )
        .map_err(|error| error.to_string())?;
        if self.staged.target_path != expected_target
            || self.staged.staging_path != expected_staging
            || (!self.reconciliation_brain_id.is_empty()
                && self.staged.reconciliation_brain_id != self.reconciliation_brain_id)
        {
            let detail =
                "ratify actor runtime differs from the sealed target, staging, or actor id"
                    .to_string();
            let _ = self.ready_tx.send(Err(detail.clone()));
            return Err(detail);
        }
        if self.require_original_preimage {
            if let Err(error) = self.staged.revalidate_before_commit() {
                let detail = error.to_string();
                let _ = self.ready_tx.send(Err(detail.clone()));
                return Err(detail);
            }
        } else if let Err(error) = self.staged.validate_recovery_preimage() {
            let detail = error.to_string();
            let _ = self.ready_tx.send(Err(detail.clone()));
            return Err(detail);
        }
        let ready = RatifyActorReadyV1 {
            operation_id: self.operation_id,
            operation_object_digest: self.staged.operation_object_digest.clone(),
            outcome_digest: self
                .staged
                .outcome_digest()
                .map_err(|error| error.to_string())?,
            target_path: self.staged.target_path.clone(),
            next_sha256: self.staged.next_sha256.clone(),
            reconciliation_brain_id: self.staged.reconciliation_brain_id.clone(),
        };
        self.ready_tx
            .send(Ok(ready))
            .map_err(|_| "ratify owner dropped before actor READY".to_string())?;
        match self.decision_rx.recv() {
            Ok(BrainPromoteActorDecisionV1::Committed) => {}
            Ok(BrainPromoteActorDecisionV1::Abort) => {
                return Err("ratify owner aborted before publication".to_string())
            }
            Err(_) => return Err("ratify owner disconnected before decision".to_string()),
        }
        let result = self.staged.publish().map_err(|error| error.to_string())?;
        (self.fault_hook)("after_domain_publish")?;
        Ok(BrainPromoteReconciliationExecutionV1 {
            ingest_output: Value::Null,
            publish_payload: result.payload,
            graph_generation_before: graph_generation,
            graph_generation_after: state.graph_generation,
            checkpoint_ack: None,
        })
    }
}

enum ExternalMutationMaintenanceOperationV1 {
    PreparedRatify {
        recovery: RatifyRecoveryPayloadV1,
        reservation_id: String,
    },
    PreparedSource {
        recovery: SourceEditRecoveryPayloadV1,
    },
    RatifyOrphan {
        reservation_id: String,
    },
    SourceOrphans {
        eligible_objects: BTreeSet<String>,
    },
}

struct ExternalMutationMaintenanceActorJobV1 {
    route_selector: Option<String>,
    operation: ExternalMutationMaintenanceOperationV1,
}

impl ExternalMutationMaintenanceActorJobV1 {
    fn execute(
        self,
        state: &mut crate::session::SessionState,
    ) -> Result<BrainPromoteReconciliationExecutionV1, String> {
        if self
            .route_selector
            .as_deref()
            .is_some_and(|route| !state.covers_root(route))
        {
            return Err(
                "resolved maintenance actor does not own the sealed source brain root".to_string(),
            );
        }
        match self.operation {
            ExternalMutationMaintenanceOperationV1::PreparedRatify {
                recovery,
                reservation_id,
            } => {
                let expected_target = SystemBlockStore::path_in(&state.runtime_root);
                let expected_staging =
                    sibling_staging_path(&expected_target, &reservation_id, "system-blocks-ratify")
                        .map_err(|error| error.to_string())?;
                if recovery.kind != "system_blocks_ratify"
                    || recovery.target_path != expected_target
                    || recovery.staging_path != expected_staging
                    || !is_digest(&recovery.original_sha256)
                    || !is_digest(&recovery.next_sha256)
                {
                    return Err(
                        "prepared ratify cleanup differs from its actor and reservation"
                            .to_string(),
                    );
                }
                StagedFileV1 {
                    target_path: recovery.target_path,
                    staging_path: recovery.staging_path,
                    expected_before_sha256: Some(recovery.original_sha256),
                    after_sha256: recovery.next_sha256,
                }
                .cleanup_unpublished()
                .map_err(|error| error.to_string())?;
            }
            ExternalMutationMaintenanceOperationV1::PreparedSource { recovery } => {
                crate::surgical_handlers::SourceEditCommitAdapterV1::abort_staged_without_target_write(
                    state,
                    &recovery.transaction_id,
                    &recovery.operation_object_digest,
                    &recovery.stage_digest,
                )
                .map_err(|error| error.to_string())?;
            }
            ExternalMutationMaintenanceOperationV1::RatifyOrphan { reservation_id } => {
                let target = SystemBlockStore::path_in(&state.runtime_root);
                let staging =
                    sibling_staging_path(&target, &reservation_id, "system-blocks-ratify")
                        .map_err(|error| error.to_string())?;
                remove_regular_file_if_present(&staging).map_err(|error| error.to_string())?;
            }
            ExternalMutationMaintenanceOperationV1::SourceOrphans { eligible_objects } => {
                let pre_stage = crate::surgical_handlers::SourceEditCommitAdapterV1::pending_pre_stage_recovery(state)
                    .map_err(|error| error.to_string())?;
                for recovery in pre_stage
                    .into_values()
                    .filter(|recovery| eligible_objects.contains(&recovery.operation_object_digest))
                {
                    crate::surgical_handlers::SourceEditCommitAdapterV1::abort_pre_stage_without_target_write(
                        state,
                        &recovery.transaction_id,
                        &recovery.operation_object_digest,
                        &recovery.intent_digest,
                    )
                    .map_err(|error| error.to_string())?;
                }
                let staged =
                    crate::surgical_handlers::SourceEditCommitAdapterV1::pending_staged_recovery(
                        state,
                    )
                    .map_err(|error| error.to_string())?;
                for recovery in staged
                    .into_values()
                    .filter(|recovery| eligible_objects.contains(&recovery.operation_object_digest))
                {
                    crate::surgical_handlers::SourceEditCommitAdapterV1::abort_staged_without_target_write(
                        state,
                        &recovery.transaction_id,
                        &recovery.operation_object_digest,
                        &recovery.stage_digest,
                    )
                    .map_err(|error| error.to_string())?;
                }
            }
        }
        Ok(BrainPromoteReconciliationExecutionV1::empty(
            state.graph_generation,
        ))
    }
}

#[derive(Debug)]
struct GraphIngestActorReadyV1 {
    operation_id: String,
    operation_object_digest: String,
    outcome_digest: String,
    root_identity: String,
    ownership_digest: String,
}

#[derive(Debug)]
struct SourceEditActorExpectedReadyV1 {
    preview_id: String,
    operation_object_digest: String,
    reconciliation_brain_id: String,
    transaction_id: Option<String>,
    stage_digest: Option<String>,
}

#[derive(Clone)]
struct SourceEditActorReadyV1 {
    staged: StagedSourceEditV1,
    reconciliation_brain_id: String,
}

enum SourceEditActorModeV1 {
    Prepare {
        request: crate::surgical_handlers::SourceEditCommitRequestV1,
        context: crate::surgical_handlers::SourceEditPreparedContextV1,
    },
    Recover {
        source_brain_id: String,
        recovery: SourceEditRecoveryPayloadV1,
    },
}

pub struct SourceEditActorJobV1 {
    mode: SourceEditActorModeV1,
    reconciliation_brain_id: String,
    fault_hook: Arc<ExternalMutationFaultHookV1>,
    ready_tx: SyncSender<Result<SourceEditActorReadyV1, String>>,
    decision_rx: Receiver<BrainPromoteActorDecisionV1>,
}

impl SourceEditActorJobV1 {
    pub(crate) fn execute(
        self,
        state: &mut crate::session::SessionState,
    ) -> Result<BrainPromoteReconciliationExecutionV1, String> {
        let graph_generation_before = state.graph_generation;
        let (staged, require_precommit_revalidation) = match &self.mode {
            SourceEditActorModeV1::Prepare { request, context } => {
                let prepared =
                    match crate::surgical_handlers::SourceEditCommitAdapterV1::prepare_in_actor(
                        state, request, context,
                    ) {
                        Ok(prepared) => prepared,
                        Err(error) => {
                            let detail = error.to_string();
                            let _ = self.ready_tx.send(Err(detail.clone()));
                            return Err(detail);
                        }
                    };
                let staged = match prepared.stage(state) {
                    Ok(staged) => staged,
                    Err(error) => {
                        let detail = error.to_string();
                        let _ = self.ready_tx.send(Err(detail.clone()));
                        return Err(detail);
                    }
                };
                (
                    StagedSourceEditV1 {
                        preview_id: request.preview_id.clone(),
                        transaction_id: staged.transaction_id().to_string(),
                        operation_object_digest: staged.operation_object_digest().to_string(),
                        staged,
                    },
                    true,
                )
            }
            SourceEditActorModeV1::Recover {
                source_brain_id,
                recovery,
            } => {
                if !source_brain_id.is_empty() && !state.covers_root(source_brain_id) {
                    let detail = "resolved source-edit actor does not own the committed brain root"
                        .to_string();
                    let _ = self.ready_tx.send(Err(detail.clone()));
                    return Err(detail);
                }
                let staged = match crate::surgical_handlers::SourceEditCommitAdapterV1::validate_committed_recovery_binding(
                    state,
                    &recovery.transaction_id,
                    &recovery.operation_object_digest,
                    &recovery.stage_digest,
                ) {
                    Ok(staged) => staged,
                    Err(error) => {
                        let detail = error.to_string();
                        let _ = self.ready_tx.send(Err(detail.clone()));
                        return Err(detail);
                    }
                };
                (
                    StagedSourceEditV1 {
                        preview_id: recovery.preview_id.clone(),
                        transaction_id: recovery.transaction_id.clone(),
                        operation_object_digest: recovery.operation_object_digest.clone(),
                        staged,
                    },
                    false,
                )
            }
        };
        if require_precommit_revalidation {
            if let Err(error) = staged.revalidate_before_commit(state) {
                let detail = error.to_string();
                let _ = self.ready_tx.send(Err(detail.clone()));
                return Err(detail);
            }
        }
        self.ready_tx
            .send(Ok(SourceEditActorReadyV1 {
                staged: staged.clone(),
                reconciliation_brain_id: self.reconciliation_brain_id,
            }))
            .map_err(|_| "source-edit owner dropped before actor READY".to_string())?;
        match self.decision_rx.recv() {
            Ok(BrainPromoteActorDecisionV1::Committed) => {}
            Ok(BrainPromoteActorDecisionV1::Abort) => {
                return Err("source-edit owner aborted before publication".to_string())
            }
            Err(_) => return Err("source-edit owner disconnected before decision".to_string()),
        }
        let result = staged.publish(state).map_err(|error| error.to_string())?;
        (self.fault_hook)("after_domain_publish")?;
        Ok(BrainPromoteReconciliationExecutionV1 {
            ingest_output: Value::Null,
            publish_payload: result.payload,
            graph_generation_before,
            graph_generation_after: state.graph_generation,
            checkpoint_ack: None,
        })
    }
}

pub struct GraphIngestActorJobV1 {
    staged: StagedGraphIngestA2V1,
    candidate: graph_ingest_a2::DurableGraphIngestCandidateV1,
    fault_hook: Arc<ExternalMutationFaultHookV1>,
    operation_id: String,
    require_original_preimage: bool,
    ready_tx: SyncSender<Result<GraphIngestActorReadyV1, String>>,
    decision_rx: Receiver<BrainPromoteActorDecisionV1>,
}

impl GraphIngestActorJobV1 {
    pub(crate) fn execute(
        self,
        state: &mut crate::session::SessionState,
    ) -> Result<BrainPromoteReconciliationExecutionV1, String> {
        let graph_generation_before = state.graph_generation;
        if let Err(error) = self
            .staged
            .revalidate_actor_preimage(state, self.require_original_preimage)
        {
            let detail = format!("{}: {}", error.code, error.detail);
            let _ = self.ready_tx.send(Err(detail.clone()));
            return Err(detail);
        }
        self.ready_tx
            .send(Ok(GraphIngestActorReadyV1 {
                operation_id: self.operation_id,
                operation_object_digest: self.staged.operation_object_digest.clone(),
                outcome_digest: self.staged.outcome_digest.clone(),
                root_identity: self.staged.semantic_payload.root_identity.clone(),
                ownership_digest: self.staged.ownership_manifest.ownership_digest.clone(),
            }))
            .map_err(|_| "graph-ingest owner dropped before actor READY".to_string())?;
        match self.decision_rx.recv() {
            Ok(BrainPromoteActorDecisionV1::Committed) => {}
            Ok(BrainPromoteActorDecisionV1::Abort) => {
                return Err("graph-ingest owner aborted before publication".to_string())
            }
            Err(_) => return Err("graph-ingest owner disconnected before decision".to_string()),
        }

        let ingest_output = crate::tools::install_complete_code_bundle(
            state,
            crate::protocol::core::IngestInput {
                path: self.staged.semantic_payload.root_identity.clone(),
                agent_id: self.staged.authority_subject_id.clone(),
                incremental: false,
                adapter: "code".to_string(),
                // Both authority modes install the complete source projection.
                // MergeExisting preserves the causal/authority distinction; it
                // never invokes the disabled exact-file fast path.
                mode: "replace".to_string(),
                namespace: None,
                include_dotfiles: self.staged.semantic_payload.include_dotfiles,
                dotfile_patterns: self.staged.semantic_payload.dotfile_patterns.clone(),
                project_root: None,
            },
            self.candidate.bundle,
            self.candidate.file_inventory,
        )
        .map_err(|error| error.to_string())?;
        (self.fault_hook)("after_domain_publish")?;
        if state.graph_generation <= graph_generation_before {
            return Err("A2 graph ingest did not advance graph generation".to_string());
        }
        let publish_payload = json!({
            "mode": self.staged.semantic_payload.mode,
            "root_identity": self.staged.semantic_payload.root_identity,
            "reconciliation_brain_id": self.staged.reconciliation_brain_id,
            "ownership_manifest": self.staged.ownership_manifest,
            "parent": self.staged.semantic_payload.parent,
            "candidate_ownership_digest": self.staged.semantic_payload.candidate_ownership_digest,
            "candidate_source_projection_digest": self.staged.semantic_payload.candidate_source_projection_digest,
            "candidate_pipeline_digest": self.staged.semantic_payload.candidate_pipeline_digest,
            "actor_checkpoint_required": true,
        });
        Ok(BrainPromoteReconciliationExecutionV1 {
            ingest_output,
            publish_payload,
            graph_generation_before,
            graph_generation_after: state.graph_generation,
            checkpoint_ack: None,
        })
    }
}

pub struct BrainPromoteActorJobV1 {
    staged: StagedPromoteV1,
    fault_hook: Arc<ExternalMutationFaultHookV1>,
    operation_id: String,
    require_precommit_revalidation: bool,
    ready_tx: SyncSender<Result<BrainPromoteActorReadyV1, String>>,
    decision_rx: Receiver<BrainPromoteActorDecisionV1>,
}

impl BrainPromoteActorJobV1 {
    pub(crate) fn execute(
        self,
        state: &mut crate::session::SessionState,
    ) -> Result<BrainPromoteReconciliationExecutionV1, String> {
        let expected_medulla_store = state.runtime_root.join("agent-memory");
        let expected_medulla_target =
            expected_medulla_store.join(format!("{}.light.md", self.staged.medulla_slug));
        let medulla_history_bound = self.staged.medulla_history.as_ref().is_none_or(|history| {
            history
                .target_path
                .parent()
                .is_some_and(|parent| parent == expected_medulla_store.join(".history"))
        });
        if self.staged.paths.medulla_runtime_root != state.runtime_root
            || self.staged.paths.medulla_store_dir != expected_medulla_store
            || self.staged.medulla.target_path != expected_medulla_target
            || !medulla_history_bound
        {
            let detail =
                "promotion reconciliation runtime root/store differs from the sealed medulla target"
                    .to_string();
            let _ = self.ready_tx.send(Err(detail.clone()));
            return Err(detail);
        }
        let graph_generation_before = state.graph_generation;
        // The actor is the sole owner of the promotion target locks. They stay
        // on this actor stack while the owner thread completes the broker and
        // journal handshake, then through all file writes and graph ingest.
        let target_locks = crate::promote_handlers::acquire_promote_target_locks(
            &self.staged.paths.source_store_dir,
            &self.staged.source_slug,
            &self.staged.paths.medulla_store_dir,
            &self.staged.medulla_slug,
        )
        .map_err(|error| error.to_string())?;
        if self.require_precommit_revalidation {
            if let Err(error) = self.staged.revalidate_before_commit() {
                let detail = error.to_string();
                let _ = self.ready_tx.send(Err(detail.clone()));
                return Err(detail);
            }
        }
        self.ready_tx
            .send(Ok(BrainPromoteActorReadyV1 {
                operation_id: self.operation_id,
                operation_object_digest: self.staged.operation_object_digest.clone(),
                outcome_digest: self.staged.outcome_digest.clone(),
                medulla_path: self.staged.medulla.target_path.clone(),
                medulla_postimage_sha256: self.staged.medulla.after_sha256.clone(),
            }))
            .map_err(|_| "promotion owner dropped before actor READY".to_string())?;
        // No timeout after READY: releasing the target locks before an explicit
        // COMMITTED/ABORT decision would reopen the stale-preimage window. A
        // dropped owner disconnects this channel and fails closed.
        match self.decision_rx.recv() {
            Ok(BrainPromoteActorDecisionV1::Committed) => {}
            Ok(BrainPromoteActorDecisionV1::Abort) => {
                return Err("promotion owner aborted before publication".to_string())
            }
            Err(_) => return Err("promotion owner disconnected before decision".to_string()),
        }
        let result = self
            .staged
            .publish(&target_locks, self.fault_hook.as_ref())
            .map_err(|error| error.to_string())?;
        (self.fault_hook)("after_domain_publish")?;
        require_file_digest(
            &self.staged.medulla.target_path,
            Some(&self.staged.medulla.after_sha256),
        )
        .map_err(|error| error.to_string())?;
        let ingest_output = crate::tools::handle_ingest(
            state,
            crate::protocol::core::IngestInput {
                path: self
                    .staged
                    .medulla
                    .target_path
                    .to_string_lossy()
                    .to_string(),
                agent_id: self.staged.input.agent_id.clone(),
                incremental: false,
                adapter: "light".to_string(),
                mode: "merge".to_string(),
                namespace: Some("light".to_string()),
                include_dotfiles: false,
                dotfile_patterns: vec![],
                project_root: None,
            },
        )
        .map_err(|error| error.to_string())?;
        if state.graph_generation <= graph_generation_before {
            return Err("brain promotion reconciliation did not mutate graph generation".into());
        }
        Ok(BrainPromoteReconciliationExecutionV1 {
            ingest_output,
            publish_payload: result.payload,
            graph_generation_before,
            graph_generation_after: state.graph_generation,
            checkpoint_ack: None,
        })
    }
}

impl InspectedSourceEditV1 {
    fn prepared_context(
        &self,
        operation_object_digest: &str,
        expected_effects: &BTreeSet<Effect>,
        brain_id: &str,
    ) -> Result<crate::surgical_handlers::SourceEditPreparedContextV1, ExternalMutationError> {
        let context = crate::surgical_handlers::SourceEditPreparedContextV1 {
            authority_subject_id: self.authority_subject_id.clone(),
            semantic_action: "source.edit.commit".to_string(),
            ingress: Ingress::Mcp,
            semantic_payload_digest: self.intent.semantic_payload_digest.clone(),
            operation_object_digest: operation_object_digest.to_string(),
            expected_effects: expected_effects.clone(),
            brain_id: brain_id.to_string(),
            mission_id: None,
            mission_head_id: None,
            operation_version: EXTERNAL_MUTATION_OPERATION_VERSION,
        };
        let recomputed =
            crate::surgical_handlers::SourceEditCommitAdapterV1::operation_object_digest(
                &self.intent,
                &context,
            )
            .map_err(|error| ExternalMutationError::Domain(error.to_string()))?;
        if recomputed != operation_object_digest {
            return Err(ExternalMutationError::refused(
                "source_edit_operation_object_mismatch",
                "source adapter and external service derived different operation objects",
            ));
        }
        Ok(context)
    }
}

enum StagedMutationV1 {
    Ratify(StagedRatifyV1),
    Promote(Box<StagedPromoteV1>),
    GraphIngest(Box<StagedGraphIngestA2V1>),
}

impl StagedMutationV1 {
    fn recovery_payload(&self) -> Value {
        match self {
            Self::Ratify(value) => json!({
                "kind": "system_blocks_ratify",
                "reconciliation_brain_id": value.reconciliation_brain_id,
                "target_path": value.target_path,
                "staging_path": value.staging_path,
                "original_sha256": value.original_sha256,
                "next_sha256": value.next_sha256,
                "ratified_block_ids": value.summary.ratified_block_ids,
                "store_version": value.summary.store_version,
            }),
            Self::Promote(value) => json!({
                "kind": "brain_promote",
                "input": {
                    "authority_subject_id": value.input.agent_id,
                    "brain": value.input.brain,
                    "claim": value.input.claim,
                    "reason": value.input.reason,
                },
                "source": value.source,
                "medulla": value.medulla,
                "source_history": value.source_history,
                "medulla_history": value.medulla_history,
                "source_slug": value.source_slug,
                "medulla_slug": value.medulla_slug,
                "origin_brain": value.origin_brain,
                "origin_qualified": value.origin_qualified,
                "evidence_unverifiable": value.evidence_unverifiable,
                "promoted_at_ms": value.promoted_at_ms,
                "reconciliation_brain_id": value.reconciliation_brain_id,
                "operation_object_digest": value.operation_object_digest,
                "outcome_digest": value.outcome_digest,
                "forward_complete": true,
            }),
            Self::GraphIngest(value) => value.recovery_payload(),
        }
    }

    fn outcome_digest(&self) -> Result<Option<String>, ExternalMutationError> {
        match self {
            Self::Ratify(value) => Ok(Some(digest_canonical(
                "m1nd-external-ratify-outcome-v1",
                &(
                    value.operation_object_digest.as_str(),
                    value.next_sha256.as_str(),
                    &value.summary.ratified_block_ids,
                    value.summary.store_version,
                ),
            )?)),
            Self::Promote(value) => Ok(Some(value.outcome_digest.clone())),
            Self::GraphIngest(value) => Ok(Some(value.outcome_digest.clone())),
        }
    }

    fn revalidate(
        &self,
        _locked_state: Option<&crate::session::SessionState>,
    ) -> Result<(), ExternalMutationError> {
        match self {
            Self::Ratify(value) => {
                require_file_digest(&value.target_path, Some(&value.original_sha256))?;
                require_file_digest(&value.staging_path, Some(&value.next_sha256))
            }
            Self::Promote(value) => value.revalidate_before_commit(),
            Self::GraphIngest(value) => value
                .load_durable_candidate(true)
                .map(|_| ())
                .map_err(|error| ExternalMutationError::refused(error.code, error.detail)),
        }
    }

    fn publish(
        &self,
        promote_target_locks: Option<&crate::promote_handlers::PromoteTargetLocksV1>,
        _locked_state: Option<&mut crate::session::SessionState>,
        fault_hook: &ExternalMutationFaultHookV1,
    ) -> Result<MutationPublishResultV1, ExternalMutationError> {
        match self {
            Self::Ratify(value) => value.publish(),
            Self::Promote(value) => {
                let target_locks = promote_target_locks.ok_or_else(|| {
                    ExternalMutationError::refused(
                        "brain_promote_target_lock_missing",
                        "promotion publication requires the exact shared source and medulla locks",
                    )
                })?;
                // Re-read both exact preimages while the shared locks are held.
                // `None` means the destination was absent and must remain absent;
                // appearance is an OCC conflict, never permission to overwrite.
                value.publish(target_locks, fault_hook)
            }
            Self::GraphIngest(_) => Err(ExternalMutationError::refused(
                "graph_ingest_actor_required",
                "A2 graph ingestion must publish inside the selected brain actor checkpoint turn",
            )),
        }
    }
}

impl StagedPromoteV1 {
    fn revalidate_before_commit(&self) -> Result<(), ExternalMutationError> {
        self.source.revalidate_precommit()?;
        self.medulla.revalidate_precommit()?;
        self.source_history.revalidate_precommit()?;
        if let Some(history) = &self.medulla_history {
            history.revalidate_precommit()?;
        }
        Ok(())
    }

    fn publish(
        &self,
        _target_locks: &crate::promote_handlers::PromoteTargetLocksV1,
        fault_hook: &ExternalMutationFaultHookV1,
    ) -> Result<MutationPublishResultV1, ExternalMutationError> {
        // Histories land before the corresponding live target. Every step is
        // idempotent old-or-new: a crash after the medulla copy but before the
        // project witness is resumed from the same sealed postimages.
        if let Some(history) = &self.medulla_history {
            history.forward_publish()?;
            fault_hook("after_promote_medulla_history").map_err(|detail| {
                ExternalMutationError::refused(
                    "external_mutation_injected_crash",
                    format!("after_promote_medulla_history: {detail}"),
                )
            })?;
        }
        self.medulla.forward_publish()?;
        fault_hook("after_promote_medulla_live").map_err(|detail| {
            ExternalMutationError::refused(
                "external_mutation_injected_crash",
                format!("after_promote_medulla_live: {detail}"),
            )
        })?;
        self.source_history.forward_publish()?;
        fault_hook("after_promote_source_history").map_err(|detail| {
            ExternalMutationError::refused(
                "external_mutation_injected_crash",
                format!("after_promote_source_history: {detail}"),
            )
        })?;
        self.source.forward_publish()?;
        fault_hook("after_promote_source_live").map_err(|detail| {
            ExternalMutationError::refused(
                "external_mutation_injected_crash",
                format!("after_promote_source_live: {detail}"),
            )
        })?;

        let outcome = crate::promote_handlers::PromoteOutcome {
            medulla_path: self.medulla.target_path.clone(),
            witness_path: self.source.target_path.clone(),
            medulla_slug: self.medulla_slug.clone(),
            origin_brain: self.origin_brain.clone(),
            origin_qualified: self.origin_qualified,
            evidence_unverifiable: self.evidence_unverifiable,
            medulla_claim_count: count_live_light_claims(&self.paths.medulla_store_dir),
            soft_cap: crate::promote_handlers::MEDULLA_SOFT_CAP,
        };
        Ok(MutationPublishResultV1 {
            payload: crate::promote_handlers::promote_response(&self.input, &outcome),
            graph_resync_required: true,
        })
    }
}

impl StagedFileV1 {
    fn validate_sealed(&self) -> Result<(), ExternalMutationError> {
        if !is_digest(&self.after_sha256)
            || self
                .expected_before_sha256
                .as_deref()
                .is_some_and(|digest| !is_digest(digest))
            || self.target_path == self.staging_path
        {
            return Err(ExternalMutationError::refused(
                "external_mutation_recovery_binding_mismatch",
                "staged file paths or digests are invalid",
            ));
        }
        Ok(())
    }

    fn revalidate_precommit(&self) -> Result<(), ExternalMutationError> {
        require_file_digest(&self.target_path, self.expected_before_sha256.as_deref())?;
        require_file_digest(&self.staging_path, Some(&self.after_sha256))
    }

    /// Recovery may observe either side of the idempotent rename, but never a
    /// third value. This is read-only and therefore safe before the broker's
    /// COMMITTED witness is recovered.
    fn validate_forward_preimage(&self) -> Result<(), ExternalMutationError> {
        self.validate_sealed()?;
        match self.target_path.symlink_metadata() {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                let actual = sha256_bytes(&std::fs::read(&self.target_path).map_err(|source| {
                    ExternalMutationError::Io {
                        operation: "read_external_mutation_recovery_target",
                        source,
                    }
                })?);
                if actual == self.after_sha256 {
                    if self.staging_path.exists() {
                        require_file_digest(&self.staging_path, Some(&self.after_sha256))?;
                    }
                    return Ok(());
                }
                if self.expected_before_sha256.as_deref() == Some(actual.as_str()) {
                    return require_file_digest(&self.staging_path, Some(&self.after_sha256));
                }
                Err(ExternalMutationError::refused(
                    "external_mutation_occ_conflict",
                    format!("{} is at neither sealed digest", self.target_path.display()),
                ))
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && self.expected_before_sha256.is_none() =>
            {
                require_file_digest(&self.staging_path, Some(&self.after_sha256))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(ExternalMutationError::refused(
                    "external_mutation_occ_conflict",
                    format!("{} disappeared", self.target_path.display()),
                ))
            }
            Err(source) => Err(ExternalMutationError::Io {
                operation: "inspect_external_mutation_recovery_target",
                source,
            }),
            _ => Err(ExternalMutationError::refused(
                "external_mutation_target_not_regular_file",
                self.target_path.display().to_string(),
            )),
        }
    }

    fn cleanup_unpublished(&self) -> Result<(), ExternalMutationError> {
        self.validate_sealed()?;
        match self.staging_path.symlink_metadata() {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                require_file_digest(&self.staging_path, Some(&self.after_sha256))?;
                std::fs::remove_file(&self.staging_path).map_err(|source| {
                    ExternalMutationError::Io {
                        operation: "remove_aborted_external_mutation_staging",
                        source,
                    }
                })?;
                sync_parent(&self.staging_path).map_err(|source| ExternalMutationError::Io {
                    operation: "sync_aborted_external_mutation_staging_parent",
                    source,
                })
            }
            Ok(_) => Err(ExternalMutationError::refused(
                "external_mutation_staging_not_regular_file",
                self.staging_path.display().to_string(),
            )),
            Err(source) => Err(ExternalMutationError::Io {
                operation: "inspect_aborted_external_mutation_staging",
                source,
            }),
        }
    }

    fn forward_publish(&self) -> Result<(), ExternalMutationError> {
        match self.target_path.symlink_metadata() {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                let actual = sha256_bytes(&std::fs::read(&self.target_path).map_err(|source| {
                    ExternalMutationError::Io {
                        operation: "read_external_mutation_publish_target",
                        source,
                    }
                })?);
                if actual == self.after_sha256 {
                    if self.staging_path.exists() {
                        require_file_digest(&self.staging_path, Some(&self.after_sha256))?;
                        std::fs::remove_file(&self.staging_path).map_err(|source| {
                            ExternalMutationError::Io {
                                operation: "remove_replayed_external_mutation_staging",
                                source,
                            }
                        })?;
                        sync_parent(&self.staging_path).map_err(|source| {
                            ExternalMutationError::Io {
                                operation: "sync_replayed_external_mutation_staging_parent",
                                source,
                            }
                        })?;
                    }
                    return Ok(());
                }
                if self.expected_before_sha256.as_deref() != Some(actual.as_str()) {
                    return Err(ExternalMutationError::refused(
                        "external_mutation_occ_conflict",
                        format!("{} is at neither sealed digest", self.target_path.display()),
                    ));
                }
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && self.expected_before_sha256.is_none() => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(ExternalMutationError::refused(
                    "external_mutation_occ_conflict",
                    format!("{} disappeared", self.target_path.display()),
                ))
            }
            Err(source) => {
                return Err(ExternalMutationError::Io {
                    operation: "inspect_external_mutation_publish_target",
                    source,
                })
            }
            _ => {
                return Err(ExternalMutationError::refused(
                    "external_mutation_target_not_regular_file",
                    self.target_path.display().to_string(),
                ))
            }
        }
        require_file_digest(&self.staging_path, Some(&self.after_sha256))?;
        std::fs::rename(&self.staging_path, &self.target_path).map_err(|source| {
            ExternalMutationError::Io {
                operation: "forward_publish_external_mutation_staging",
                source,
            }
        })?;
        sync_parent(&self.target_path).map_err(|source| ExternalMutationError::Io {
            operation: "sync_forward_published_external_mutation_parent",
            source,
        })?;
        require_file_digest(&self.target_path, Some(&self.after_sha256))
    }
}

fn staged_promote_from_recovery(
    recovery: PromoteRecoveryPayloadV1,
    entry: &ExternalMutationJournalEntryV1,
) -> Result<StagedPromoteV1, ExternalMutationError> {
    recovery.source.validate_sealed()?;
    recovery.medulla.validate_sealed()?;
    recovery.source_history.validate_sealed()?;
    if let Some(history) = &recovery.medulla_history {
        history.validate_sealed()?;
    }
    let source_store_dir = recovery
        .source
        .target_path
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            ExternalMutationError::refused(
                "external_mutation_recovery_binding_mismatch",
                "promotion source target has no store parent",
            )
        })?;
    let medulla_store_dir = recovery
        .medulla
        .target_path
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            ExternalMutationError::refused(
                "external_mutation_recovery_binding_mismatch",
                "promotion medulla target has no store parent",
            )
        })?;
    let medulla_runtime_root = medulla_store_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            ExternalMutationError::refused(
                "external_mutation_recovery_binding_mismatch",
                "promotion medulla store has no runtime parent",
            )
        })?;
    let expected_source_path = source_store_dir.join(format!("{}.light.md", recovery.source_slug));
    let expected_medulla_path =
        medulla_store_dir.join(format!("{}.light.md", recovery.medulla_slug));
    let history_parent_matches = recovery
        .source_history
        .target_path
        .parent()
        .is_some_and(|parent| parent == source_store_dir.join(".history"))
        && recovery.medulla_history.as_ref().is_none_or(|history| {
            history
                .target_path
                .parent()
                .is_some_and(|parent| parent == medulla_store_dir.join(".history"))
        });
    let recomputed_outcome = digest_canonical(
        "m1nd-external-promote-staged-outcome-v1",
        &(
            recovery.operation_object_digest.as_str(),
            recovery.input.authority_subject_id.as_str(),
            recovery.input.brain.as_str(),
            recovery.input.claim.as_str(),
            recovery.input.reason.as_str(),
            recovery
                .source
                .expected_before_sha256
                .as_deref()
                .unwrap_or_default(),
            recovery.medulla.expected_before_sha256.as_deref(),
            recovery.source.after_sha256.as_str(),
            recovery.medulla.after_sha256.as_str(),
            recovery.source_history.after_sha256.as_str(),
            recovery
                .medulla_history
                .as_ref()
                .map(|history| history.after_sha256.as_str()),
            recovery.promoted_at_ms,
            recovery.reconciliation_brain_id.as_str(),
        ),
    )?;
    if recovery.kind != "brain_promote"
        || !recovery.forward_complete
        || recovery.operation_object_digest != entry.prepare.operation_object_digest
        || recovery.outcome_digest != recomputed_outcome
        || entry.outcome_digest.as_deref() != Some(recovery.outcome_digest.as_str())
        || recovery.promoted_at_ms == 0
        || recovery.promoted_at_ms > entry.prepared_at
        || recovery.input.authority_subject_id.trim().is_empty()
        || recovery.input.brain.trim().is_empty()
        || recovery.input.claim.trim().is_empty()
        || recovery.input.reason.trim().is_empty()
        || recovery.reconciliation_brain_id.trim().is_empty()
        || recovery.source.expected_before_sha256.is_none()
        || recovery.source.target_path != expected_source_path
        || recovery.medulla.target_path != expected_medulla_path
        || !history_parent_matches
    {
        return Err(ExternalMutationError::refused(
            "external_mutation_recovery_binding_mismatch",
            "promotion recovery payload differs from its sealed COMMIT",
        ));
    }
    Ok(StagedPromoteV1 {
        input: crate::promote_handlers::PromoteInput {
            agent_id: recovery.input.authority_subject_id,
            brain: recovery.input.brain,
            claim: recovery.input.claim,
            reason: recovery.input.reason,
        },
        paths: ExternalPromotePathsV1 {
            source_store_dir,
            medulla_store_dir,
            medulla_runtime_root,
        },
        source: recovery.source,
        medulla: recovery.medulla,
        source_history: recovery.source_history,
        medulla_history: recovery.medulla_history,
        source_slug: recovery.source_slug,
        medulla_slug: recovery.medulla_slug,
        origin_brain: recovery.origin_brain,
        origin_qualified: recovery.origin_qualified,
        evidence_unverifiable: recovery.evidence_unverifiable,
        promoted_at_ms: recovery.promoted_at_ms,
        reconciliation_brain_id: recovery.reconciliation_brain_id,
        operation_object_digest: recovery.operation_object_digest,
        outcome_digest: recovery.outcome_digest,
    })
}

fn staged_ratify_from_recovery(
    recovery: RatifyRecoveryPayloadV1,
    entry: &ExternalMutationJournalEntryV1,
) -> Result<StagedRatifyV1, ExternalMutationError> {
    let recomputed_outcome = digest_canonical(
        "m1nd-external-ratify-outcome-v1",
        &(
            entry.prepare.operation_object_digest.as_str(),
            recovery.next_sha256.as_str(),
            &recovery.ratified_block_ids,
            recovery.store_version,
        ),
    )?;
    let staged_file = StagedFileV1 {
        target_path: recovery.target_path.clone(),
        staging_path: recovery.staging_path.clone(),
        expected_before_sha256: Some(recovery.original_sha256.clone()),
        after_sha256: recovery.next_sha256.clone(),
    };
    staged_file.validate_sealed()?;
    if recovery.kind != "system_blocks_ratify"
        || entry.outcome_digest.as_deref() != Some(recomputed_outcome.as_str())
        || recovery.store_version == 0
        || recovery.ratified_block_ids.is_empty()
    {
        return Err(ExternalMutationError::refused(
            "external_mutation_recovery_binding_mismatch",
            "ratify recovery payload is not bound to its committed outcome",
        ));
    }
    Ok(StagedRatifyV1 {
        target_path: recovery.target_path,
        staging_path: recovery.staging_path,
        original_sha256: recovery.original_sha256,
        next_sha256: recovery.next_sha256,
        operation_object_digest: entry.prepare.operation_object_digest.clone(),
        summary: RatifySummary {
            ratified_block_ids: recovery.ratified_block_ids,
            store_version: recovery.store_version,
        },
        reconciliation_brain_id: recovery.reconciliation_brain_id,
    })
}

#[derive(Clone)]
struct StagedSourceEditV1 {
    preview_id: String,
    transaction_id: String,
    operation_object_digest: String,
    staged: crate::surgical_handlers::SourceEditStagedCommitV1,
}

impl StagedSourceEditV1 {
    fn outcome_digest(&self) -> &str {
        &self.staged.stage_digest
    }

    fn recovery_payload(&self, reconciliation_brain_id: &str) -> Value {
        json!({
            "kind": "source_edit_commit",
            "preview_id": self.preview_id,
            "transaction_id": self.transaction_id,
            "operation_object_digest": self.operation_object_digest,
            "stage_digest": self.staged.stage_digest,
            "reconciliation_brain_id": reconciliation_brain_id,
            "graph_resync_required": true,
            "forward_complete_committed": true,
        })
    }

    fn revalidate_before_commit(
        &self,
        state: &crate::session::SessionState,
    ) -> Result<(), ExternalMutationError> {
        crate::surgical_handlers::SourceEditCommitAdapterV1::revalidate_stage_before_commit(
            state,
            &self.staged,
        )
        .map_err(|error| ExternalMutationError::Domain(error.to_string()))
    }

    fn publish(
        &self,
        state: &mut crate::session::SessionState,
    ) -> Result<MutationPublishResultV1, ExternalMutationError> {
        let outcome = crate::surgical_handlers::SourceEditCommitAdapterV1::publish_after_commit(
            state,
            &self.staged,
        )
        .map_err(|error| ExternalMutationError::Domain(error.to_string()))?;
        let terminal =
            crate::surgical_handlers::SourceEditCommitAdapterV1::finalize(state, &outcome)
                .map_err(|error| ExternalMutationError::Domain(error.to_string()))?;
        Ok(MutationPublishResultV1 {
            payload: json!({
                "source_edit_outcome": outcome,
                "source_edit_terminal": terminal,
                "graph_resync_required": true,
            }),
            graph_resync_required: true,
        })
    }
}

#[derive(Clone)]
struct StagedRatifyV1 {
    target_path: PathBuf,
    staging_path: PathBuf,
    original_sha256: String,
    next_sha256: String,
    operation_object_digest: String,
    summary: RatifySummary,
    reconciliation_brain_id: String,
}

impl StagedRatifyV1 {
    fn outcome_digest(&self) -> Result<String, ExternalMutationError> {
        digest_canonical(
            "m1nd-external-ratify-outcome-v1",
            &(
                self.operation_object_digest.as_str(),
                self.next_sha256.as_str(),
                &self.summary.ratified_block_ids,
                self.summary.store_version,
            ),
        )
        .map_err(ExternalMutationError::from)
    }

    fn staged_file(&self) -> StagedFileV1 {
        StagedFileV1 {
            target_path: self.target_path.clone(),
            staging_path: self.staging_path.clone(),
            expected_before_sha256: Some(self.original_sha256.clone()),
            after_sha256: self.next_sha256.clone(),
        }
    }

    fn revalidate_before_commit(&self) -> Result<(), ExternalMutationError> {
        self.staged_file().revalidate_precommit()
    }

    fn validate_recovery_preimage(&self) -> Result<(), ExternalMutationError> {
        self.staged_file().validate_forward_preimage()
    }

    fn publish(&self) -> Result<MutationPublishResultV1, ExternalMutationError> {
        self.staged_file().forward_publish()?;
        Ok(MutationPublishResultV1 {
            payload: json!({
                "ok": true,
                "ratified_block_ids": self.summary.ratified_block_ids,
                "store_version": self.summary.store_version,
                "ratifier_source": "SIGNED_AUTHORITY_SUBJECT",
            }),
            graph_resync_required: false,
        })
    }
}

fn inspect_request_actor_only(
    request: &ExternalMutationRequestV1,
    host: &ExternalMutationExecutionHostV1,
    authority_subject_id: &str,
    owner_now_ms: u64,
    journal_entries: &[ExternalMutationJournalEntryV1],
    brain_id: &str,
) -> Result<InspectedMutationV1, ExternalMutationError> {
    let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
    let actor_request = BrainPromoteReconciliationRequestV1 {
        operation_id: format!("inspect:{}", request.request_id()),
        operation_object_digest: String::new(),
        source_brain_id: brain_id.to_string(),
        reconciliation_brain_id: host.reconciliation_brain_id.clone(),
        medulla_path: PathBuf::new(),
        medulla_postimage_sha256: String::new(),
        authority_subject_id: authority_subject_id.to_string(),
        job: ExternalMutationActorJobV1::Inspect(Box::new(ExternalMutationInspectActorJobV1 {
            request: request.clone(),
            authority_subject_id: authority_subject_id.to_string(),
            owner_now_ms,
            journal_entries: journal_entries.to_vec(),
            brain_id: brain_id.to_string(),
            reconciliation_brain_id: host.reconciliation_brain_id.clone(),
            promote_paths: host.promote_paths.clone(),
            reply_tx,
        })),
    };
    let routed = (host.reconcile_promote)(actor_request);
    match reply_rx.recv() {
        Ok(Err(error)) => Err(error),
        Ok(Ok(inspected)) => {
            routed.map_err(|detail| {
                ExternalMutationError::refused("external_mutation_inspect_actor_failed", detail)
            })?;
            Ok(inspected)
        }
        Err(_) => Err(ExternalMutationError::refused(
            "external_mutation_inspect_actor_failed",
            routed
                .err()
                .unwrap_or_else(|| "inspection actor exited without a reply".to_string()),
        )),
    }
}

/// Test/domain helper for callers that deliberately exercise inspection
/// without the production RuntimeJobRegistry transport. Production preview and
/// execute paths call `inspect_request_actor_only` and supervise the scan.
#[cfg(test)]
fn inspect_request(
    request: &ExternalMutationRequestV1,
    host: &ExternalMutationExecutionHostV1,
    authority_subject_id: &str,
    owner_now_ms: u64,
    journal_entries: &[ExternalMutationJournalEntryV1],
    brain_id: &str,
) -> Result<InspectedMutationV1, ExternalMutationError> {
    match inspect_request_actor_only(
        request,
        host,
        authority_subject_id,
        owner_now_ms,
        journal_entries,
        brain_id,
    )? {
        InspectedMutationV1::GraphIngestSnapshot(snapshot) => {
            graph_ingest_a2::complete_inspection_off_actor(*snapshot)
                .map(|completed| InspectedMutationV1::GraphIngest(Box::new(completed)))
                .map_err(|error| ExternalMutationError::refused(error.code, error.detail))
        }
        inspected => Ok(inspected),
    }
}

#[allow(clippy::too_many_arguments)]
fn inspect_request_in_state(
    request: &ExternalMutationRequestV1,
    state: &crate::session::SessionState,
    promote_paths: Option<ExternalPromotePathsV1>,
    reconciliation_brain_id: &str,
    authority_subject_id: &str,
    owner_now_ms: u64,
    journal_entries: &[ExternalMutationJournalEntryV1],
    brain_id: &str,
) -> Result<InspectedMutationV1, ExternalMutationError> {
    match request {
        ExternalMutationRequestV1::SystemBlocksRatify {
            expected_store_version,
            block_ids,
            ..
        } => {
            let target_path = SystemBlockStore::path_in(&state.runtime_root);
            let original = read_regular_file(&target_path)?;
            let original_sha256 = sha256_bytes(&original);
            let mut store: SystemBlockStore =
                serde_json::from_slice(&original).map_err(|error| {
                    ExternalMutationError::Domain(format!("system block store decode: {error}"))
                })?;
            let summary = store
                .ratify(
                    *expected_store_version,
                    block_ids.as_deref(),
                    authority_subject_id,
                    &owner_now_ms.to_string(),
                )
                .map_err(|error| ExternalMutationError::Domain(error.to_string()))?;
            let next_bytes = serde_json::to_vec_pretty(&store).map_err(|error| {
                ExternalMutationError::Domain(format!("system block store encode: {error}"))
            })?;
            let semantic_payload_digest = digest_canonical(
                SYSTEM_BLOCKS_RATIFY_PAYLOAD_DIGEST_DOMAIN,
                &RatifySemanticPayloadV1 {
                    schema: SYSTEM_BLOCKS_RATIFY_PAYLOAD_SCHEMA,
                    expected_store_version: *expected_store_version,
                    block_ids: block_ids.as_deref(),
                },
            )?;
            Ok(InspectedMutationV1::Ratify(Box::new(InspectedRatifyV1 {
                target_path,
                original_sha256,
                next_bytes,
                summary,
                semantic_payload_digest,
                reconciliation_brain_id: reconciliation_brain_id.to_string(),
            })))
        }
        ExternalMutationRequestV1::BrainPromote {
            source_brain,
            claim,
            reason,
            expected_source_sha256,
            expected_medulla_sha256,
            ..
        } => {
            let paths = promote_paths.ok_or_else(|| {
                ExternalMutationError::refused(
                    "brain_promote_host_unavailable",
                    "owner did not resolve a source and medulla store for this request",
                )
            })?;
            let source_slug = crate::light_author_handlers::slugify(claim);
            let source_path = paths
                .source_store_dir
                .join(format!("{source_slug}.light.md"));
            require_file_digest(&source_path, Some(expected_source_sha256))?;
            let source = read_regular_file(&source_path)?;
            let source_text = std::str::from_utf8(&source).map_err(|error| {
                ExternalMutationError::Domain(format!("source claim is not UTF-8: {error}"))
            })?;
            let parsed = crate::promote_handlers::parse_light_claim(source_text);
            let medulla_slug = crate::light_author_handlers::slugify(
                parsed.frontmatter.node.as_deref().unwrap_or(claim),
            );
            let medulla_path = paths
                .medulla_store_dir
                .join(format!("{medulla_slug}.light.md"));
            require_file_digest(&medulla_path, expected_medulla_sha256.as_deref())?;
            let medulla = expected_medulla_sha256
                .as_ref()
                .map(|_| read_regular_file(&medulla_path))
                .transpose()?;
            let semantic_payload_digest = digest_canonical(
                BRAIN_PROMOTE_PAYLOAD_DIGEST_DOMAIN,
                &PromoteSemanticPayloadV1 {
                    schema: BRAIN_PROMOTE_PAYLOAD_SCHEMA,
                    source_brain,
                    claim,
                    reason,
                    expected_source_sha256,
                    expected_medulla_sha256: expected_medulla_sha256.as_deref(),
                },
            )?;
            let input = crate::promote_handlers::PromoteInput {
                agent_id: authority_subject_id.to_string(),
                brain: source_brain.clone(),
                claim: claim.clone(),
                reason: reason.clone(),
            };
            let plan = crate::promote_handlers::plan_external_promotion(
                &input,
                source_text,
                medulla
                    .as_deref()
                    .map(std::str::from_utf8)
                    .transpose()
                    .map_err(|error| {
                        ExternalMutationError::Domain(format!(
                            "medulla claim is not UTF-8: {error}"
                        ))
                    })?,
                owner_now_ms,
            )
            .map_err(|error| ExternalMutationError::Domain(error.to_string()))?;
            if plan.medulla_slug != medulla_slug {
                return Err(ExternalMutationError::refused(
                    "brain_promote_plan_binding_mismatch",
                    "promotion plan changed the owner-derived medulla slug",
                ));
            }
            Ok(InspectedMutationV1::Promote(Box::new(InspectedPromoteV1 {
                input,
                paths,
                source_path,
                medulla_path,
                source_slug,
                medulla_slug,
                source_sha256: expected_source_sha256.clone(),
                medulla_sha256: expected_medulla_sha256.clone(),
                semantic_payload_digest,
                reconciliation_brain_id: reconciliation_brain_id.to_string(),
                plan,
            })))
        }
        ExternalMutationRequestV1::SourceEditCommit { request, .. } => {
            let request = crate::surgical_handlers::SourceEditCommitRequestV1 {
                schema: request.schema.clone(),
                preview_id: request.preview_id.clone(),
            };
            let intent = crate::surgical_handlers::SourceEditCommitAdapterV1::inspect_state(
                state,
                &request,
                authority_subject_id,
            )
            .map_err(|error| ExternalMutationError::Domain(error.to_string()))?;
            Ok(InspectedMutationV1::SourceEdit(Box::new(
                InspectedSourceEditV1 {
                    request,
                    intent,
                    authority_subject_id: authority_subject_id.to_string(),
                    reconciliation_brain_id: reconciliation_brain_id.to_string(),
                },
            )))
        }
        ExternalMutationRequestV1::GraphIngestReplace { request, .. } => {
            let reconciliation_brain_id = if reconciliation_brain_id.is_empty() {
                graph_ingest_a2::selected_actor_id(state)
            } else {
                reconciliation_brain_id.to_string()
            };
            graph_ingest_a2::capture_inspection_snapshot(
                state,
                request,
                GraphIngestA2ModeV1::Replace,
                journal_entries,
                brain_id,
                reconciliation_brain_id,
                authority_subject_id.to_string(),
            )
            .map(|snapshot| InspectedMutationV1::GraphIngestSnapshot(Box::new(snapshot)))
            .map_err(|error| ExternalMutationError::refused(error.code, error.detail))
        }
        ExternalMutationRequestV1::GraphIngestMergeExisting { request, .. } => {
            let reconciliation_brain_id = if reconciliation_brain_id.is_empty() {
                graph_ingest_a2::selected_actor_id(state)
            } else {
                reconciliation_brain_id.to_string()
            };
            graph_ingest_a2::capture_inspection_snapshot(
                state,
                request,
                GraphIngestA2ModeV1::MergeExisting,
                journal_entries,
                brain_id,
                reconciliation_brain_id,
                authority_subject_id.to_string(),
            )
            .map(|snapshot| InspectedMutationV1::GraphIngestSnapshot(Box::new(snapshot)))
            .map_err(|error| ExternalMutationError::refused(error.code, error.detail))
        }
    }
}

fn request_matches_existing_entry(
    request: &ExternalMutationRequestV1,
    entry: &ExternalMutationJournalEntryV1,
) -> Result<bool, ExternalMutationError> {
    let matches = match request {
        ExternalMutationRequestV1::SystemBlocksRatify {
            expected_store_version,
            block_ids,
            ..
        } => {
            digest_canonical(
                SYSTEM_BLOCKS_RATIFY_PAYLOAD_DIGEST_DOMAIN,
                &RatifySemanticPayloadV1 {
                    schema: SYSTEM_BLOCKS_RATIFY_PAYLOAD_SCHEMA,
                    expected_store_version: *expected_store_version,
                    block_ids: block_ids.as_deref(),
                },
            )? == entry.prepare.payload_digest
        }
        ExternalMutationRequestV1::BrainPromote {
            source_brain,
            claim,
            reason,
            expected_source_sha256,
            expected_medulla_sha256,
            ..
        } => {
            digest_canonical(
                BRAIN_PROMOTE_PAYLOAD_DIGEST_DOMAIN,
                &PromoteSemanticPayloadV1 {
                    schema: BRAIN_PROMOTE_PAYLOAD_SCHEMA,
                    source_brain,
                    claim,
                    reason,
                    expected_source_sha256,
                    expected_medulla_sha256: expected_medulla_sha256.as_deref(),
                },
            )? == entry.prepare.payload_digest
        }
        ExternalMutationRequestV1::SourceEditCommit { request, .. } => {
            let recovery: SourceEditRecoveryPayloadV1 = serde_json::from_value(
                entry.prepare.recovery_payload.clone(),
            )
            .map_err(|error| {
                ExternalMutationError::refused(
                    "external_mutation_recovery_payload_invalid",
                    error.to_string(),
                )
            })?;
            recovery.preview_id == request.preview_id
                && recovery.operation_object_digest == entry.prepare.operation_object_digest
                && entry
                    .outcome_digest
                    .as_deref()
                    .is_none_or(|outcome| outcome == recovery.stage_digest)
        }
        ExternalMutationRequestV1::GraphIngestReplace { request, .. } => {
            graph_ingest_a2::request_matches_entry(request, GraphIngestA2ModeV1::Replace, entry)
                .map_err(|error| ExternalMutationError::refused(error.code, error.detail))?
        }
        ExternalMutationRequestV1::GraphIngestMergeExisting { request, .. } => {
            graph_ingest_a2::request_matches_entry(
                request,
                GraphIngestA2ModeV1::MergeExisting,
                entry,
            )
            .map_err(|error| ExternalMutationError::refused(error.code, error.detail))?
        }
    };
    Ok(matches)
}

fn graph_ingest_request_preview_id(request: &ExternalMutationRequestV1) -> Option<&str> {
    match request {
        ExternalMutationRequestV1::GraphIngestReplace { request, .. }
        | ExternalMutationRequestV1::GraphIngestMergeExisting { request, .. } => {
            Some(request.preview_id.as_str())
        }
        _ => None,
    }
}

fn required_transport_fact<'a>(
    value: Option<&'a str>,
    code: &'static str,
) -> Result<&'a str, ExternalMutationError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            ExternalMutationError::refused(code, "required owner-observed transport fact is absent")
        })
}

fn sibling_staging_path(
    target: &Path,
    reservation_id: &str,
    suffix: &str,
) -> Result<PathBuf, ExternalMutationError> {
    let reservation_prefix = reservation_id.get(..16).ok_or_else(|| {
        ExternalMutationError::refused(
            "external_mutation_reservation_id_invalid",
            "reservation id is too short for a canonical staging path",
        )
    })?;
    let parent = target.parent().ok_or_else(|| {
        ExternalMutationError::refused(
            "external_mutation_target_invalid",
            "target has no parent directory",
        )
    })?;
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            ExternalMutationError::refused(
                "external_mutation_target_invalid",
                "target filename is not valid UTF-8",
            )
        })?;
    Ok(parent.join(format!(".{file_name}.{suffix}.{}.tmp", reservation_prefix)))
}

fn promotion_history_path(
    store_dir: &Path,
    slug: &str,
    promoted_at_ms: u64,
) -> Result<PathBuf, ExternalMutationError> {
    let history_dir = store_dir.join(".history");
    match history_dir.symlink_metadata() {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => {
            return Err(ExternalMutationError::refused(
                "brain_promote_history_directory_invalid",
                history_dir.display().to_string(),
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(&history_dir).map_err(|source| ExternalMutationError::Io {
                operation: "create_brain_promote_history_directory",
                source,
            })?;
            sync_parent(&history_dir).map_err(|source| ExternalMutationError::Io {
                operation: "sync_brain_promote_history_parent",
                source,
            })?;
        }
        Err(source) => {
            return Err(ExternalMutationError::Io {
                operation: "inspect_brain_promote_history_directory",
                source,
            })
        }
    }
    Ok(history_dir.join(format!("{slug}.{promoted_at_ms}.light.md")))
}

fn write_staging_file(path: &Path, bytes: &[u8]) -> Result<(), ExternalMutationError> {
    if path.symlink_metadata().is_ok() {
        return Err(ExternalMutationError::refused(
            "external_mutation_staging_collision",
            path.display().to_string(),
        ));
    }
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|source| ExternalMutationError::Io {
            operation: "create_external_mutation_staging_file",
            source,
        })?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| ExternalMutationError::Io {
            operation: "write_sync_external_mutation_staging_file",
            source,
        })?;
    sync_parent(path).map_err(|source| ExternalMutationError::Io {
        operation: "sync_external_mutation_staging_parent",
        source,
    })
}

fn count_live_light_claims(store_dir: &Path) -> usize {
    std::fs::read_dir(store_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            !name.starts_with('.') && name.ends_with(".light.md") && entry.path().is_file()
        })
        .count()
}

fn read_regular_file(path: &Path) -> Result<Vec<u8>, ExternalMutationError> {
    let metadata = path
        .symlink_metadata()
        .map_err(|source| ExternalMutationError::Io {
            operation: "inspect_external_mutation_target",
            source,
        })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ExternalMutationError::refused(
            "external_mutation_target_not_regular_file",
            path.display().to_string(),
        ));
    }
    std::fs::read(path).map_err(|source| ExternalMutationError::Io {
        operation: "read_external_mutation_target",
        source,
    })
}

fn remove_regular_file_if_present(path: &Path) -> Result<(), ExternalMutationError> {
    match path.symlink_metadata() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            std::fs::remove_file(path).map_err(|source| ExternalMutationError::Io {
                operation: "remove_external_mutation_orphan_staging",
                source,
            })?;
            sync_parent(path).map_err(|source| ExternalMutationError::Io {
                operation: "sync_external_mutation_orphan_staging_parent",
                source,
            })
        }
        Ok(_) => Err(ExternalMutationError::refused(
            "external_mutation_orphan_staging_not_regular_file",
            path.display().to_string(),
        )),
        Err(source) => Err(ExternalMutationError::Io {
            operation: "inspect_external_mutation_orphan_staging",
            source,
        }),
    }
}

fn require_file_digest(path: &Path, expected: Option<&str>) -> Result<(), ExternalMutationError> {
    match (path.symlink_metadata(), expected) {
        (Err(error), None) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        (Ok(metadata), Some(expected))
            if metadata.is_file() && !metadata.file_type().is_symlink() =>
        {
            let actual =
                sha256_bytes(
                    &std::fs::read(path).map_err(|source| ExternalMutationError::Io {
                        operation: "read_external_mutation_occ_target",
                        source,
                    })?,
                );
            if actual == expected {
                Ok(())
            } else {
                Err(ExternalMutationError::refused(
                    "external_mutation_occ_conflict",
                    format!("{} digest changed", path.display()),
                ))
            }
        }
        (Ok(metadata), None) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            Err(ExternalMutationError::refused(
                "external_mutation_occ_conflict",
                format!("{} appeared after inspection", path.display()),
            ))
        }
        (Err(error), Some(_)) if error.kind() == std::io::ErrorKind::NotFound => {
            Err(ExternalMutationError::refused(
                "external_mutation_occ_conflict",
                format!("{} disappeared after inspection", path.display()),
            ))
        }
        (Err(source), _) => Err(ExternalMutationError::Io {
            operation: "inspect_external_mutation_occ_target",
            source,
        }),
        _ => Err(ExternalMutationError::refused(
            "external_mutation_target_not_regular_file",
            path.display().to_string(),
        )),
    }
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn sync_parent(path: &Path) -> std::io::Result<()> {
    // Windows refuses fsync on directory handles; write-through covers renames.
    #[cfg(windows)]
    {
        let _ = path;
        return Ok(());
    }
    #[cfg(not(windows))]
    std::fs::File::open(path.parent().unwrap_or_else(|| Path::new(".")))?.sync_all()
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;
    #[cfg(unix)]
    use std::{os::unix::fs::MetadataExt, os::unix::fs::PermissionsExt};

    use crate::authority_runtime::{
        AuthorityAuthorizationReceiptCoreV1, AuthorityAuthorizationReceiptV1,
        AuthorityRuntimeStateCoreV1, AuthorityRuntimeStateV1, AuthorityRuntimeStatusV1,
        AuthorityVerificationAssurance, AuthorizationAuthorityV1, ProtectedEpochAssurance,
        AUTHORIZATION_RECEIPT_SCHEMA, AUTHORIZATION_RECEIPT_SIGNATURE_DOMAIN,
    };
    use crate::authority_wal::SoftwareTestAuthorityWalRecordCrypto;
    use crate::protected_journal_head::{
        SharedProtectedJournalHeadBackendV1, SoftwareTestProtectedJournalHeadBackendV1,
    };
    use crate::server::McpConfig;
    use crate::session::SessionState;
    use m1nd_control::{
        ActionId, ActiveMode, AuthorityVariant, AutonomyTier, CapabilityKind, OpaqueSignature,
        ReachablePolicyTupleV1, RiskClass, Role,
    };
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::Graph;
    use tempfile::TempDir;

    const MATRIX_NOW_MS: u64 = 10_000;
    const MATRIX_LEASE_ID: &str = "external-promote-matrix-lease";
    const MATRIX_TRANSPORT_ID: &str = "external-promote-matrix-transport";

    fn matrix_hash(label: &str) -> String {
        digest_canonical("external-promote-matrix-test-v1", &label).expect("matrix digest")
    }

    fn request_with_id(
        mut request: ExternalMutationRequestV1,
        next_request_id: impl Into<String>,
    ) -> ExternalMutationRequestV1 {
        let next_request_id = next_request_id.into();
        match &mut request {
            ExternalMutationRequestV1::SystemBlocksRatify { request_id, .. }
            | ExternalMutationRequestV1::BrainPromote { request_id, .. }
            | ExternalMutationRequestV1::SourceEditCommit { request_id, .. }
            | ExternalMutationRequestV1::GraphIngestReplace { request_id, .. }
            | ExternalMutationRequestV1::GraphIngestMergeExisting { request_id, .. } => {
                *request_id = next_request_id;
            }
        }
        request
    }

    fn sign_matrix_receipt(
        mut receipt: AuthorityAuthorizationReceiptV1,
        crypto: &dyn AuthorityWalRecordCrypto,
    ) -> AuthorityAuthorizationReceiptV1 {
        receipt.schema = AUTHORIZATION_RECEIPT_SCHEMA.to_string();
        receipt.issuer = crypto.issuer().to_string();
        receipt.key_id = crypto.key_id().to_string();
        receipt.algorithm = crypto.algorithm().to_string();
        receipt.signature = OpaqueSignature::new("pending-owner-signature");
        let canonical = receipt
            .canonical_signature_payload()
            .expect("canonical receipt payload");
        const PREFIX: &[u8] = b"m1nd-runtime-authorization-receipt-signature-message-v1\0";
        let domain = AUTHORIZATION_RECEIPT_SIGNATURE_DOMAIN.as_bytes();
        let mut message = Vec::with_capacity(PREFIX.len() + domain.len() + canonical.len() + 16);
        message.extend_from_slice(PREFIX);
        message.extend_from_slice(&(domain.len() as u64).to_be_bytes());
        message.extend_from_slice(domain);
        message.extend_from_slice(&(canonical.len() as u64).to_be_bytes());
        message.extend_from_slice(&canonical);
        receipt.signature = OpaqueSignature::new(crypto.sign(&message).expect("test signature"));
        crate::authority_transport::verify_authorization_receipt(&receipt, crypto)
            .expect("signed matrix receipt verifies");
        receipt
    }

    fn matrix_status(receipt: &AuthorityAuthorizationReceiptV1) -> AuthorityRuntimeStatusV1 {
        let core = &receipt.core;
        AuthorityRuntimeStatusV1 {
            state: AuthorityRuntimeStateV1::new_for_broker_test(AuthorityRuntimeStateCoreV1 {
                organism_id: core.organism_id.clone(),
                repo_id: core.repo_id.clone(),
                brain_id: core.brain_id.clone(),
                audience: "m1nd-runtime".to_string(),
                revision: 1,
                active_mode: core.active_mode,
                activation_receipt_id: None,
                constitution_digest: core.constitution_digest.clone(),
                constitution_epoch: core.constitution_epoch,
                autonomy_epoch: core.autonomy_epoch,
                grants_digest: matrix_hash("grants"),
                policy_registry_digest: core.policy_registry_digest.clone(),
                action_catalog_digest: matrix_hash("catalog"),
                safety_kernel_digest: matrix_hash("kernel"),
                safety_actuator_identity_key_binary_policy_digest: matrix_hash("actuator"),
                issuance_frozen: false,
                safety_state: m1nd_control::autonomy::SafetyState::Healthy,
                protected_epoch: core.protected_epoch,
                journal_sequence: core.journal_sequence,
                journal_root_digest: core.journal_root_digest.clone(),
                replay_sequence: core.replay_sequence,
                replay_root_digest: Some(matrix_hash("replay")),
                updated_at: core.authorized_at,
            }),
            protected_epoch_assurance: ProtectedEpochAssurance::SoftwareTestOnlyNotProven,
            positive_verification_assurance:
                AuthorityVerificationAssurance::SoftwareTestOnlyNotProven,
            semantic_catalog_entries: 1,
            transport_schema_parity_proven: true,
            multi_artifact_atomicity_proven: false,
            automatic_crash_recovery_proven: true,
        }
    }

    struct IssuedMatrixAuthorityV1 {
        broker_config: OwnerAuthorizationBrokerConfigV1,
        linearization: OwnerAuthorityLinearizationV1,
        broker_operation: Arc<Mutex<()>>,
        current_authority: Arc<AuthorityStatusReader>,
        protected_journal_head: SharedProtectedJournalHeadBackendV1,
        receipt_crypto: Arc<dyn AuthorityWalRecordCrypto>,
        context: MissionServiceTransportContextV1,
        operation_object_digest: String,
    }

    #[allow(clippy::too_many_arguments)]
    fn issue_matrix_authority(
        root: &Path,
        request: &ExternalMutationRequestV1,
        host: &ExternalMutationExecutionHostV1,
        actor_brain_id: &str,
        route_selector: &str,
        subject: &str,
        lease_id: &str,
        transport_id: &str,
        label: &str,
    ) -> IssuedMatrixAuthorityV1 {
        let inspected = inspect_request(request, host, subject, MATRIX_NOW_MS, &[], actor_brain_id)
            .expect("matrix request inspection");
        let semantic_payload_digest = inspected.semantic_payload_digest().to_string();
        let operation_object_digest = digest_canonical(
            EXTERNAL_MUTATION_OPERATION_OBJECT_DIGEST_DOMAIN,
            &ExternalMutationOperationObjectV1 {
                schema: EXTERNAL_MUTATION_OPERATION_OBJECT_SCHEMA,
                semantic_action: request.semantic_action_id(),
                ingress: Ingress::Mcp,
                brain_id: actor_brain_id,
                mission_id: None,
                mission_head_id: None,
                operation_version: EXTERNAL_MUTATION_OPERATION_VERSION,
                semantic_payload_digest: &semantic_payload_digest,
            },
        )
        .expect("matrix operation object");
        let ingress_context_digest = matrix_hash(&format!("{label}-ingress-context"));
        let contract = external_consumer_contract(request.semantic_action_id(), Ingress::Mcp)
            .expect("external mutation contract");
        let crypto: Arc<dyn AuthorityWalRecordCrypto> = Arc::new(
            SoftwareTestAuthorityWalRecordCrypto::explicit_not_production(format!(
                "external-{label}-matrix-receipt"
            )),
        );
        let action = ActionId::new(request.semantic_action_id()).expect("matrix action id");
        let receipt = sign_matrix_receipt(
            AuthorityAuthorizationReceiptV1::new_for_broker_test(
                AuthorityAuthorizationReceiptCoreV1 {
                    organism_id: format!("organism-{label}-matrix"),
                    repo_id: format!("repo-{label}-matrix"),
                    brain_id: actor_brain_id.to_string(),
                    subject_id: subject.to_string(),
                    role: Role::Author,
                    capability_id: format!("capability-{label}-matrix"),
                    capability_kind: Some(CapabilityKind::Human),
                    verified_object_digest: operation_object_digest.clone(),
                    mission_id: None,
                    mission_head_id: None,
                    transport_session_id: transport_id.to_string(),
                    ingress_context_digest: ingress_context_digest.clone(),
                    action: action.clone(),
                    ingress: Ingress::Mcp,
                    complete_effects: contract.expected_effects,
                    active_mode: ActiveMode::HumanGated,
                    constitution_digest: matrix_hash(&format!("{label}-constitution")),
                    constitution_epoch: 7,
                    autonomy_epoch: 0,
                    protected_epoch_at_decision: 11,
                    policy_registry_digest: matrix_hash(&format!("{label}-policy")),
                    exact_policy_tuple: ReachablePolicyTupleV1 {
                        ingress: Ingress::Mcp,
                        action,
                        active_mode: ActiveMode::HumanGated,
                        subject_id: subject.to_string(),
                        authority_variant: AuthorityVariant::Human,
                        applicable_grant_id: None,
                        applicable_tier: None,
                        risk_class: RiskClass::Critical,
                    },
                    authority_decision_digest: Some(matrix_hash(&format!("{label}-decision"))),
                    autonomy_admission_receipt_digest: None,
                    autonomy_committed_state_digest: None,
                    autonomy_protected_root_digest: None,
                    authority: AuthorizationAuthorityV1::Positive {
                        variant: AuthorityVariant::Human,
                        assurance: AuthorityVerificationAssurance::SoftwareTestOnlyNotProven,
                    },
                    authority_body_digest: matrix_hash(&format!("{label}-authority-body")),
                    replay_sequence: 3,
                    journal_sequence: 11,
                    journal_root_digest: matrix_hash(&format!("{label}-journal-root")),
                    protected_epoch: 11,
                    authorized_at: MATRIX_NOW_MS,
                    expires_at: MATRIX_NOW_MS + 10_000,
                },
            ),
            crypto.as_ref(),
        );
        let status = matrix_status(&receipt);
        let current_authority: Arc<AuthorityStatusReader> = Arc::new(move || Ok(status.clone()));
        let protected_journal_head = SoftwareTestProtectedJournalHeadBackendV1::new().shared();
        let broker_config = OwnerAuthorizationBrokerConfigV1 {
            root: root.join("broker"),
            reservation_ttl_ms: 100,
            minimum_terminal_retention_ms: 100,
        };
        let linearization = OwnerAuthorityLinearizationV1::default();
        {
            let mut broker = OwnerAuthorizationBrokerV1::open_with_protected_head(
                broker_config.clone(),
                linearization.clone(),
                Arc::clone(&protected_journal_head),
            )
            .expect("matrix broker");
            broker
                .issue(lease_id, receipt, MATRIX_NOW_MS)
                .expect("matrix lease");
        }
        IssuedMatrixAuthorityV1 {
            broker_config,
            linearization,
            broker_operation: Arc::new(Mutex::new(())),
            current_authority,
            protected_journal_head,
            receipt_crypto: crypto,
            context: MissionServiceTransportContextV1 {
                ingress: MissionServiceIngressV1::McpStreamableHttp,
                transport_session_id: Some(transport_id.to_string()),
                ingress_context_digest: Some(ingress_context_digest),
                authority_lease_id: Some(lease_id.to_string()),
                caller_root: Some(route_selector.to_string()),
                route_selector: Some(route_selector.to_string()),
                actor_brain_id: Some(actor_brain_id.to_string()),
            },
            operation_object_digest,
        }
    }

    fn assert_first_and_replay_are_equivalent(
        first: &ExternalMutationResponseV1,
        replay: &ExternalMutationResponseV1,
    ) {
        let mut first_value = serde_json::to_value(first).expect("first response json");
        let mut replay_value = serde_json::to_value(replay).expect("replay response json");
        for value in [&mut first_value, &mut replay_value] {
            value
                .as_object_mut()
                .expect("response object")
                .remove("request_id");
            value
                .get_mut("result")
                .and_then(Value::as_object_mut)
                .expect("result object")
                .remove("terminal_replay");
        }
        assert_eq!(first_value, replay_value);
    }

    const RATIFY_MATRIX_LEASE_ID: &str = "external-ratify-matrix-lease";
    const RATIFY_MATRIX_TRANSPORT_ID: &str = "external-ratify-matrix-transport";

    fn retire_bound_owner_for_restart(
        actor_registry: &Arc<crate::project_brains::ProjectBrainRegistry>,
        brain: &Arc<BrainSessionCell>,
        label: &str,
    ) {
        actor_registry
            .shutdown(Duration::from_secs(2))
            .unwrap_or_else(|error| panic!("shutdown simulated dead {label} actor: {error}"));
        brain
            .lock_mut_before_actor()
            .unwrap_or_else(|error| panic!("recover simulated dead {label} owner: {error}"))
            .instance
            .release()
            .unwrap_or_else(|error| panic!("release simulated dead {label} owner: {error}"));
    }

    struct RatifyMatrixFixture {
        _temp: TempDir,
        runtime_root: PathBuf,
        journal_root: PathBuf,
        broker_config: OwnerAuthorizationBrokerConfigV1,
        linearization: OwnerAuthorityLinearizationV1,
        broker_operation: Arc<Mutex<()>>,
        current_authority: Arc<AuthorityStatusReader>,
        protected_journal_head: SharedProtectedJournalHeadBackendV1,
        receipt_crypto: Arc<dyn AuthorityWalRecordCrypto>,
        clock: Arc<AtomicU64>,
        request: ExternalMutationRequestV1,
        context: MissionServiceTransportContextV1,
        host: ExternalMutationExecutionHostV1,
        actor_registry: Arc<crate::project_brains::ProjectBrainRegistry>,
        brain_id: String,
        target_path: PathBuf,
        before_bytes: Vec<u8>,
        after_bytes: Vec<u8>,
    }

    impl RatifyMatrixFixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().expect("ratify matrix tempdir");
            let runtime_root = temp.path().join("ratify-brain");
            std::fs::create_dir_all(&runtime_root).expect("ratify runtime");
            let seed = crate::system_blocks::load_seed(include_str!(
                "../../docs/system-blocks/m1nd.seed.v0.json"
            ))
            .expect("real system-block seed");
            let store = SystemBlockStore::from_seed(seed);
            store.save(&runtime_root).expect("ratify store preimage");
            let target_path = SystemBlockStore::path_in(&runtime_root);
            let before_bytes = std::fs::read(&target_path).expect("ratify preimage bytes");
            let block_id = store.blocks.first().expect("seed block").block_id.clone();
            let subject = "owner:ratify-matrix";
            let mut expected_store = store.clone();
            expected_store
                .ratify(
                    1,
                    Some(std::slice::from_ref(&block_id)),
                    subject,
                    &MATRIX_NOW_MS.to_string(),
                )
                .expect("ratify expected postimage");
            let after_bytes = serde_json::to_vec_pretty(&expected_store)
                .expect("ratify expected postimage bytes");

            let config = McpConfig {
                graph_source: runtime_root.join("graph.json"),
                plasticity_state: runtime_root.join("plasticity.json"),
                runtime_dir: Some(runtime_root.clone()),
                ..McpConfig::default()
            };
            let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
                .expect("ratify session");
            let brain_id = runtime_root.to_string_lossy().to_string();
            state.workspace_root = Some(brain_id.clone());
            state.ingest_roots = vec![brain_id.clone()];
            let selected_brain = Arc::new(BrainSessionCell::new(state));
            let recovery_brain = Arc::clone(&selected_brain);
            let actor_registry = Arc::new(crate::project_brains::ProjectBrainRegistry::new(
                temp.path().join("ratify-project-brains"),
                None,
            ));
            let reconciliation_brain_id = actor_registry
                .bound_brain_id_for_target(Arc::clone(&selected_brain))
                .expect("ratify bound actor id");
            let recovery_brain_id = reconciliation_brain_id.clone();
            let actor_brain = Arc::clone(&selected_brain);
            let runtime_jobs = actor_registry
                .runtime_job_registry()
                .expect("ratify runtime jobs");
            let registry_for_actor = Arc::clone(&actor_registry);
            let host = ExternalMutationExecutionHostV1 {
                selected_brain,
                selected_actor_brain_id: reconciliation_brain_id.clone(),
                resolve_brain: Arc::new(move |requested| {
                    if requested == recovery_brain_id {
                        Ok(Arc::clone(&recovery_brain))
                    } else {
                        Err(format!("unexpected ratify recovery brain '{requested}'"))
                    }
                }),
                reconcile_promote: Arc::new(move |request| {
                    let requires_checkpoint_ack = request.requires_checkpoint_ack();
                    let allows_resolved_actor_identity = request.allows_resolved_actor_identity();
                    let actual_brain_id = registry_for_actor
                        .bound_brain_id_for_target(Arc::clone(&actor_brain))
                        .map_err(|error| error.to_string())?;
                    if actual_brain_id != request.reconciliation_brain_id
                        && !allows_resolved_actor_identity
                    {
                        return Err(format!(
                            "ratify actor mismatch: expected '{}', observed '{}'",
                            request.reconciliation_brain_id, actual_brain_id
                        ));
                    }
                    if requires_checkpoint_ack {
                        registry_for_actor
                            .execute_target_runtime_with_checkpoint_ack(
                                Arc::clone(&actor_brain),
                                None,
                                true,
                                move |state| {
                                    request.execute(state).map_err(|detail| {
                                        crate::runtime_jobs::RuntimeJobFailure::new(
                                            "system_blocks_ratify_actor_job_failed",
                                            detail,
                                        )
                                    })
                                },
                            )
                            .map(|(execution, ack)| execution.bind_checkpoint_ack(&ack))
                            .map_err(|error| error.to_string())
                    } else {
                        registry_for_actor
                            .execute_target_runtime(
                                Arc::clone(&actor_brain),
                                None,
                                true,
                                false,
                                move |state| {
                                    request.execute(state).map_err(|detail| {
                                        crate::runtime_jobs::RuntimeJobFailure::new(
                                            "external_mutation_inspect_actor_job_failed",
                                            detail,
                                        )
                                    })
                                },
                            )
                            .map_err(|error| error.to_string())
                    }
                }),
                reconciliation_brain_id: reconciliation_brain_id.clone(),
                promote_paths: None,
                runtime_jobs: Ok(runtime_jobs),
            };
            let request = ExternalMutationRequestV1::SystemBlocksRatify {
                schema: EXTERNAL_MUTATION_REQUEST_SCHEMA.to_string(),
                request_id: "ratify-matrix-initial-request".to_string(),
                expected_store_version: 1,
                block_ids: Some(vec![block_id]),
            };
            let authority = issue_matrix_authority(
                temp.path(),
                &request,
                &host,
                &reconciliation_brain_id,
                &brain_id,
                subject,
                RATIFY_MATRIX_LEASE_ID,
                RATIFY_MATRIX_TRANSPORT_ID,
                "ratify",
            );
            Self {
                journal_root: temp.path().join("external-journal"),
                broker_config: authority.broker_config,
                linearization: authority.linearization,
                broker_operation: authority.broker_operation,
                current_authority: authority.current_authority,
                protected_journal_head: authority.protected_journal_head,
                receipt_crypto: authority.receipt_crypto,
                clock: Arc::new(AtomicU64::new(MATRIX_NOW_MS)),
                request,
                context: authority.context,
                host,
                actor_registry,
                brain_id,
                target_path,
                before_bytes,
                after_bytes,
                runtime_root,
                _temp: temp,
            }
        }

        fn service(&self) -> ExternalMutationServiceV1 {
            let clock = Arc::clone(&self.clock);
            ExternalMutationServiceV1::from_owner_inputs(ExternalMutationServiceInputsV1 {
                journal_root: self.journal_root.clone(),
                broker_config: self.broker_config.clone(),
                linearization: self.linearization.clone(),
                broker_operation: Arc::clone(&self.broker_operation),
                current_authority: Arc::clone(&self.current_authority),
                protected_journal_head: Arc::clone(&self.protected_journal_head),
                receipt_crypto: Arc::clone(&self.receipt_crypto),
                owner_clock: Arc::new(move || clock.load(Ordering::SeqCst)),
            })
        }

        fn service_crashing_at(
            &self,
            cut: &'static str,
            fired: Arc<AtomicBool>,
        ) -> ExternalMutationServiceV1 {
            self.service()
                .with_fault_hook_for_test(Arc::new(move |point| {
                    if point == cut && !fired.swap(true, Ordering::SeqCst) {
                        Err("simulated process death".to_string())
                    } else {
                        Ok(())
                    }
                }))
        }

        fn restarted_brain(&self) -> Arc<BrainSessionCell> {
            let config = McpConfig {
                graph_source: self.runtime_root.join("graph.json"),
                plasticity_state: self.runtime_root.join("plasticity.json"),
                runtime_dir: Some(self.runtime_root.clone()),
                ..McpConfig::default()
            };
            let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
                .expect("restarted ratify session");
            state.workspace_root = Some(self.brain_id.clone());
            state.ingest_roots = vec![self.brain_id.clone()];
            Arc::new(BrainSessionCell::new(state))
        }

        fn recover_for_boot(
            &self,
            service: &ExternalMutationServiceV1,
        ) -> ExternalMutationRecoveryReportV1 {
            let expected_brain = self.host.selected_actor_brain_id.clone();
            retire_bound_owner_for_restart(
                &self.actor_registry,
                &self.host.selected_brain,
                "ratify",
            );
            let brain = self.restarted_brain();
            assert!(!Arc::ptr_eq(&brain, &self.host.selected_brain));
            let actor_registry = Arc::new(crate::project_brains::ProjectBrainRegistry::new(
                self.runtime_root.join("ratify-replay-project-brains"),
                None,
            ));
            let actor_brain = Arc::clone(&brain);
            let registry_for_actor = Arc::clone(&actor_registry);
            let reconciler: Arc<BrainPromoteReconcilerV1> = Arc::new(move |request| {
                let allows_resolved_actor_identity = request.allows_resolved_actor_identity();
                let actual_brain_id = registry_for_actor
                    .bound_brain_id_for_target(Arc::clone(&actor_brain))
                    .map_err(|error| error.to_string())?;
                if actual_brain_id != request.reconciliation_brain_id
                    && !allows_resolved_actor_identity
                {
                    return Err(format!(
                        "ratify recovery actor mismatch: expected '{}', observed '{}'",
                        request.reconciliation_brain_id, actual_brain_id
                    ));
                }
                registry_for_actor
                    .execute_target_runtime_with_checkpoint_ack(
                        Arc::clone(&actor_brain),
                        None,
                        true,
                        move |state| {
                            request.execute(state).map_err(|detail| {
                                crate::runtime_jobs::RuntimeJobFailure::new(
                                    "system_blocks_ratify_actor_job_failed",
                                    detail,
                                )
                            })
                        },
                    )
                    .map(|(execution, ack)| execution.bind_checkpoint_ack(&ack))
                    .map_err(|error| error.to_string())
            });
            service
                .recover_for_boot(
                    move |requested| {
                        if requested == expected_brain {
                            Ok(Arc::clone(&brain))
                        } else {
                            Err(format!("unexpected ratify brain {requested}"))
                        }
                    },
                    reconciler,
                )
                .expect("ratify boot recovery")
        }

        fn staging_paths(&self) -> Vec<PathBuf> {
            std::fs::read_dir(&self.runtime_root)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .is_some_and(|name| name.to_string_lossy().contains("system-blocks-ratify"))
                })
                .collect()
        }

        fn target_is_before(&self) -> bool {
            std::fs::read(&self.target_path).is_ok_and(|bytes| bytes == self.before_bytes)
        }

        fn target_is_after(&self) -> bool {
            std::fs::read(&self.target_path).is_ok_and(|bytes| bytes == self.after_bytes)
        }
    }

    #[test]
    fn ratify_outer_cut_matrix_recovers_boot_and_replays_exact_terminal() {
        let cuts = [
            ("after_reserve", MatrixRecoveryExpectation::SafeAbort),
            ("after_stage", MatrixRecoveryExpectation::SafeAbort),
            (
                "after_journal_prepared",
                MatrixRecoveryExpectation::SafeAbort,
            ),
            (
                "after_broker_finalization_prepared",
                MatrixRecoveryExpectation::SafeAbort,
            ),
            (
                "after_journal_committed",
                MatrixRecoveryExpectation::ForwardComplete,
            ),
            (
                "after_broker_consumed",
                MatrixRecoveryExpectation::ForwardComplete,
            ),
            (
                "after_domain_publish",
                MatrixRecoveryExpectation::ForwardComplete,
            ),
            (
                "after_journal_published",
                MatrixRecoveryExpectation::AlreadyPublished,
            ),
        ];
        for (cut, expectation) in cuts {
            let fixture = RatifyMatrixFixture::new();
            let fired = Arc::new(AtomicBool::new(false));
            let crashing = fixture.service_crashing_at(cut, Arc::clone(&fired));
            crashing
                .execute(
                    &fixture.context,
                    fixture.request.clone(),
                    fixture.host.clone(),
                )
                .expect_err(cut);
            assert!(fired.load(Ordering::SeqCst), "cut not reached: {cut}");

            let service = fixture.service();
            let report = fixture.recover_for_boot(&service);
            match expectation {
                MatrixRecoveryExpectation::SafeAbort => {
                    assert_eq!(report.safely_aborted_pre_finalization, 1, "{cut}");
                    assert!(fixture.target_is_before(), "{cut}");
                    assert!(fixture.staging_paths().is_empty(), "{cut}");
                }
                MatrixRecoveryExpectation::ForwardComplete => {
                    assert_eq!(report.forward_completed, 1, "{cut}");
                    assert!(fixture.target_is_after(), "{cut}");
                }
                MatrixRecoveryExpectation::AlreadyPublished => {
                    assert_eq!(report.already_published, 1, "{cut}");
                    assert!(fixture.target_is_after(), "{cut}");
                }
            }
            assert!(service
                .conservation_scan()
                .expect("ratify conservation")
                .anomalies
                .is_empty());
            if !matches!(expectation, MatrixRecoveryExpectation::SafeAbort) {
                let retry = request_with_id(fixture.request.clone(), format!("retry-{cut}"));
                let replay = service
                    .execute(&fixture.context, retry, fixture.host.clone())
                    .expect("ratify terminal replay");
                assert_eq!(replay.result["terminal_replay"], Value::Bool(true));
                assert_eq!(replay.reconciliation_state, "NOT_REQUIRED");
                assert!(fixture.target_is_after(), "replay changed ratify target");
            }
        }
    }

    #[test]
    fn ratify_first_response_and_replay_share_the_exact_sealed_result() {
        let fixture = RatifyMatrixFixture::new();
        let service = fixture.service();
        let first = service
            .execute(
                &fixture.context,
                fixture.request.clone(),
                fixture.host.clone(),
            )
            .expect("ratify first response");
        assert!(!first.graph_resync_required);
        assert_eq!(first.reconciliation_state, "NOT_REQUIRED");
        assert!(first.result.get("terminal_replay").is_none());
        let retry = request_with_id(fixture.request.clone(), "ratify-sealed-retry");
        let replay = service
            .execute(&fixture.context, retry, fixture.host.clone())
            .expect("ratify replay");
        assert_eq!(replay.result["terminal_replay"], Value::Bool(true));
        assert_first_and_replay_are_equivalent(&first, &replay);
    }

    #[test]
    fn ratify_recovery_refuses_mutated_blocks_or_store_version_before_broker_or_target_write() {
        for field in ["store_version", "ratified_block_ids"] {
            let fixture = RatifyMatrixFixture::new();
            let fired = Arc::new(AtomicBool::new(false));
            fixture
                .service_crashing_at("after_journal_committed", Arc::clone(&fired))
                .execute(
                    &fixture.context,
                    fixture.request.clone(),
                    fixture.host.clone(),
                )
                .expect_err("stop after sealed ratify commit");
            assert!(fired.load(Ordering::SeqCst));
            assert!(fixture.target_is_before());

            let service = fixture.service();
            let mut forged = service
                .open_journal()
                .expect("ratify journal")
                .entries()
                .into_iter()
                .next()
                .expect("committed ratify entry");
            match field {
                "store_version" => {
                    forged.prepare.recovery_payload["store_version"] = json!(999_u64);
                }
                "ratified_block_ids" => {
                    forged.prepare.recovery_payload["ratified_block_ids"] = json!(["forged-block"]);
                }
                _ => unreachable!(),
            }
            let lease_before = service
                .open_broker()
                .expect("broker before forged recovery")
                .lease(RATIFY_MATRIX_LEASE_ID)
                .cloned()
                .expect("ratify lease");
            let mut broker = service.open_broker().expect("ratify recovery broker");
            let error = match service.forward_complete_committed_entry_with_broker(
                &mut broker,
                &forged,
                fixture.host.reconcile_promote.as_ref(),
            ) {
                Err(error) => error,
                Ok(_) => panic!("mutated recovery payload must refuse"),
            };
            assert_eq!(error.code(), "external_mutation_recovery_binding_mismatch");
            drop(broker);
            let lease_after = service
                .open_broker()
                .expect("broker after forged recovery")
                .lease(RATIFY_MATRIX_LEASE_ID)
                .cloned()
                .expect("ratify lease");
            assert_eq!(lease_after.state, lease_before.state, "{field}");
            assert!(fixture.target_is_before(), "{field}");

            let report = fixture.recover_for_boot(&service);
            assert_eq!(report.forward_completed, 1, "{field}");
            assert!(fixture.target_is_after(), "{field}");
        }
    }

    #[test]
    fn ratify_recovery_refuses_foreign_target_owner_without_writing_either_store() {
        let fixture = RatifyMatrixFixture::new();
        let fired = Arc::new(AtomicBool::new(false));
        fixture
            .service_crashing_at("after_journal_committed", Arc::clone(&fired))
            .execute(
                &fixture.context,
                fixture.request.clone(),
                fixture.host.clone(),
            )
            .expect_err("stop after sealed ratify commit");
        assert!(fired.load(Ordering::SeqCst));
        assert!(fixture.target_is_before());

        let foreign_root = fixture._temp.path().join("foreign-ratify-brain");
        std::fs::create_dir_all(&foreign_root).expect("foreign ratify root");
        let foreign_before = b"foreign store sentinel";
        let foreign_target = SystemBlockStore::path_in(&foreign_root);
        std::fs::write(&foreign_target, foreign_before).expect("foreign sentinel");
        let config = McpConfig {
            graph_source: foreign_root.join("graph.json"),
            plasticity_state: foreign_root.join("plasticity.json"),
            runtime_dir: Some(foreign_root),
            ..McpConfig::default()
        };
        let foreign = Arc::new(BrainSessionCell::new(
            SessionState::initialize(Graph::new(), &config, DomainConfig::code())
                .expect("foreign ratify state"),
        ));
        let foreign_registry = Arc::new(crate::project_brains::ProjectBrainRegistry::new(
            fixture.runtime_root.join("foreign-ratify-project-brains"),
            None,
        ));
        let foreign_actor = Arc::clone(&foreign);
        let registry_for_actor = Arc::clone(&foreign_registry);
        let foreign_reconciler: Arc<BrainPromoteReconcilerV1> = Arc::new(move |request| {
            registry_for_actor
                .execute_target_runtime_with_checkpoint_ack(
                    Arc::clone(&foreign_actor),
                    None,
                    true,
                    move |state| {
                        request.execute(state).map_err(|detail| {
                            crate::runtime_jobs::RuntimeJobFailure::new(
                                "system_blocks_ratify_actor_job_failed",
                                detail,
                            )
                        })
                    },
                )
                .map(|(execution, ack)| execution.bind_checkpoint_ack(&ack))
                .map_err(|error| error.to_string())
        });
        let service = fixture.service();
        let lease_before = service
            .open_broker()
            .expect("ratify broker before foreign owner")
            .lease(RATIFY_MATRIX_LEASE_ID)
            .cloned()
            .expect("ratify lease before foreign owner");
        let error = service
            .recover_for_boot(move |_| Ok(Arc::clone(&foreign)), foreign_reconciler)
            .expect_err("foreign owner must refuse");
        assert_eq!(error.code(), "system_blocks_ratify_actor_precommit_refused");
        let lease_after = service
            .open_broker()
            .expect("ratify broker after foreign owner")
            .lease(RATIFY_MATRIX_LEASE_ID)
            .cloned()
            .expect("ratify lease after foreign owner");
        assert_eq!(lease_after.state, lease_before.state);
        assert!(fixture.target_is_before());
        assert_eq!(std::fs::read(&foreign_target).unwrap(), foreign_before);

        let report = fixture.recover_for_boot(&service);
        assert_eq!(report.forward_completed, 1);
        assert!(fixture.target_is_after());
        assert_eq!(std::fs::read(&foreign_target).unwrap(), foreign_before);
    }

    #[test]
    fn ratify_recovery_refuses_noncanonical_staging_path_before_broker_or_target_write() {
        let fixture = RatifyMatrixFixture::new();
        let fired = Arc::new(AtomicBool::new(false));
        fixture
            .service_crashing_at("after_journal_committed", Arc::clone(&fired))
            .execute(
                &fixture.context,
                fixture.request.clone(),
                fixture.host.clone(),
            )
            .expect_err("stop after ratify commit");
        assert!(fired.load(Ordering::SeqCst));
        let service = fixture.service();
        let mut forged = service
            .open_journal()
            .expect("ratify journal")
            .entries()
            .into_iter()
            .next()
            .expect("ratify committed entry");
        forged.prepare.recovery_payload["staging_path"] =
            serde_json::to_value(fixture.runtime_root.join(".forged-ratify-stage.tmp"))
                .expect("forged staging path");
        let lease_before = service
            .open_broker()
            .expect("ratify broker before forged staging")
            .lease(RATIFY_MATRIX_LEASE_ID)
            .cloned()
            .expect("ratify lease before forged staging");
        let mut broker = service.open_broker().expect("ratify recovery broker");
        let error = match service.forward_complete_committed_entry_with_broker(
            &mut broker,
            &forged,
            fixture.host.reconcile_promote.as_ref(),
        ) {
            Err(error) => error,
            Ok(_) => panic!("forged staging path must refuse"),
        };
        assert_eq!(error.code(), "system_blocks_ratify_actor_precommit_refused");
        drop(broker);
        let lease_after = service
            .open_broker()
            .expect("ratify broker after forged staging")
            .lease(RATIFY_MATRIX_LEASE_ID)
            .cloned()
            .expect("ratify lease after forged staging");
        assert_eq!(lease_after.state, lease_before.state);
        assert!(fixture.target_is_before());

        let report = fixture.recover_for_boot(&service);
        assert_eq!(report.forward_completed, 1);
        assert!(fixture.target_is_after());
    }

    const SOURCE_MATRIX_LEASE_ID: &str = "external-source-matrix-lease";
    const SOURCE_MATRIX_TRANSPORT_ID: &str = "external-source-matrix-transport";
    const SOURCE_MATRIX_SUBJECT: &str = "owner:source-matrix";
    const SOURCE_MATRIX_BEFORE: &str = "pub fn before() -> u8 { 1 }\n";
    const SOURCE_MATRIX_AFTER: &str = "pub fn after() -> u8 { 2 }\n";

    /// The source recovery matrix spawns real brain-actor threads and drives
    /// restart cycles whose previous owner releases *asynchronously* — the actor
    /// thread must drain and drop before `actor_active` clears. Running several of
    /// these fixtures at once starves that drain on a loaded CI runner, so the
    /// restart bind in `host_for_brain` can miss even its 30s wait. Serialize the
    /// family so only one source matrix fixture is live at a time: each cycle's
    /// actor is free to release before the next one binds. A poisoned latch is
    /// irrelevant (it carries no state), so recover from it.
    static SOURCE_MATRIX_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct SourceMatrixFixture {
        _temp: TempDir,
        repo_root: PathBuf,
        runtime_root: PathBuf,
        target_path: PathBuf,
        journal_root: PathBuf,
        broker_config: OwnerAuthorizationBrokerConfigV1,
        linearization: OwnerAuthorityLinearizationV1,
        broker_operation: Arc<Mutex<()>>,
        current_authority: Arc<AuthorityStatusReader>,
        protected_journal_head: SharedProtectedJournalHeadBackendV1,
        receipt_crypto: Arc<dyn AuthorityWalRecordCrypto>,
        clock: Arc<AtomicU64>,
        request: ExternalMutationRequestV1,
        context: MissionServiceTransportContextV1,
        host: ExternalMutationExecutionHostV1,
        actor_registry: Arc<crate::project_brains::ProjectBrainRegistry>,
        brain_id: String,
        operation_object_digest: String,
        actor_calls: Arc<AtomicU64>,
        checkpoint_acks: Arc<Mutex<Vec<BrainPromoteCheckpointAckV1>>>,
        // Declared LAST so it drops LAST: the latch must outlive the host /
        // actor-registry teardown above (fields drop in declaration order), or
        // the next fixture binds while this one's actor is still draining.
        _serial: std::sync::MutexGuard<'static, ()>,
    }

    impl SourceMatrixFixture {
        fn initialize_state(repo_root: &Path, runtime_root: &Path) -> SessionState {
            let config = McpConfig {
                graph_source: runtime_root.join("graph.json"),
                plasticity_state: runtime_root.join("plasticity.json"),
                runtime_dir: Some(runtime_root.to_path_buf()),
                ..McpConfig::default()
            };
            let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
                .expect("source matrix session");
            state.ingest_roots = vec![repo_root.to_string_lossy().to_string()];
            state.workspace_root = Some(repo_root.to_string_lossy().to_string());
            state
        }

        fn new() -> Self {
            let serial = SOURCE_MATRIX_SERIAL
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let temp = tempfile::tempdir().expect("source matrix tempdir");
            let repo_root = temp.path().join("repo");
            let runtime_root = temp.path().join("runtime");
            let target_path = repo_root.join("src/lib.rs");
            std::fs::create_dir_all(target_path.parent().expect("source parent"))
                .expect("source tree");
            std::fs::create_dir_all(&runtime_root).expect("source runtime");
            std::fs::write(&target_path, SOURCE_MATRIX_BEFORE).expect("source preimage");
            #[cfg(unix)]
            std::fs::set_permissions(&target_path, std::fs::Permissions::from_mode(0o640))
                .expect("source preimage mode");

            let mut state = Self::initialize_state(&repo_root, &runtime_root);
            let target_text = target_path.to_string_lossy().to_string();
            let preview = crate::surgical_handlers::handle_edit_preview(
                &mut state,
                crate::protocol::surgical::EditPreviewInput {
                    file_path: target_text.clone(),
                    agent_id: SOURCE_MATRIX_SUBJECT.to_string(),
                    new_content: SOURCE_MATRIX_AFTER.to_string(),
                    description: Some("outer source mutation matrix".to_string()),
                },
            )
            .expect("source edit preview");
            state
                .note_proof_ready(SOURCE_MATRIX_SUBJECT, &target_text, "matrix proof")
                .expect("source edit proof");
            let selected_brain = Arc::new(BrainSessionCell::new(state));
            let brain_id = repo_root.to_string_lossy().to_string();
            let recovery_brain = Arc::clone(&selected_brain);
            let actor_registry = Arc::new(crate::project_brains::ProjectBrainRegistry::new(
                temp.path().join("source-project-brains"),
                None,
            ));
            let reconciliation_brain_id = actor_registry
                .bound_brain_id_for_target(Arc::clone(&selected_brain))
                .expect("source bound actor id");
            let recovery_brain_id = reconciliation_brain_id.clone();
            let actor_brain = Arc::clone(&selected_brain);
            let runtime_jobs = actor_registry
                .runtime_job_registry()
                .expect("source runtime jobs");
            let registry_for_actor = Arc::clone(&actor_registry);
            let actor_calls = Arc::new(AtomicU64::new(0));
            let calls_for_actor = Arc::clone(&actor_calls);
            let checkpoint_acks = Arc::new(Mutex::new(Vec::new()));
            let acks_for_actor = Arc::clone(&checkpoint_acks);
            let host = ExternalMutationExecutionHostV1 {
                selected_brain,
                selected_actor_brain_id: reconciliation_brain_id.clone(),
                resolve_brain: Arc::new(move |requested| {
                    if requested == recovery_brain_id {
                        Ok(Arc::clone(&recovery_brain))
                    } else {
                        Err(format!("unexpected source recovery brain '{requested}'"))
                    }
                }),
                reconcile_promote: Arc::new(move |request| {
                    let requires_checkpoint_ack = request.requires_checkpoint_ack();
                    if requires_checkpoint_ack {
                        calls_for_actor.fetch_add(1, Ordering::SeqCst);
                    }
                    let allows_resolved_actor_identity = request.allows_resolved_actor_identity();
                    let actual_brain_id = registry_for_actor
                        .bound_brain_id_for_target(Arc::clone(&actor_brain))
                        .map_err(|error| error.to_string())?;
                    if actual_brain_id != request.reconciliation_brain_id
                        && !allows_resolved_actor_identity
                    {
                        return Err(format!(
                            "source actor mismatch: expected '{}', observed '{}'",
                            request.reconciliation_brain_id, actual_brain_id
                        ));
                    }
                    if requires_checkpoint_ack {
                        registry_for_actor
                            .execute_target_runtime_with_checkpoint_ack(
                                Arc::clone(&actor_brain),
                                None,
                                true,
                                move |state| {
                                    request.execute(state).map_err(|detail| {
                                        crate::runtime_jobs::RuntimeJobFailure::new(
                                            "source_edit_actor_job_failed",
                                            detail,
                                        )
                                    })
                                },
                            )
                            .map(|(execution, ack)| {
                                let execution = execution.bind_checkpoint_ack(&ack);
                                if let Some(ack) = execution.checkpoint_ack.clone() {
                                    acks_for_actor.lock().push(ack);
                                }
                                execution
                            })
                            .map_err(|error| error.to_string())
                    } else {
                        registry_for_actor
                            .execute_target_runtime(
                                Arc::clone(&actor_brain),
                                None,
                                true,
                                false,
                                move |state| {
                                    request.execute(state).map_err(|detail| {
                                        crate::runtime_jobs::RuntimeJobFailure::new(
                                            "external_mutation_inspect_actor_job_failed",
                                            detail,
                                        )
                                    })
                                },
                            )
                            .map_err(|error| error.to_string())
                    }
                }),
                reconciliation_brain_id: reconciliation_brain_id.clone(),
                promote_paths: None,
                runtime_jobs: Ok(runtime_jobs),
            };
            let request = ExternalMutationRequestV1::SourceEditCommit {
                schema: EXTERNAL_MUTATION_REQUEST_SCHEMA.to_string(),
                request_id: "source-matrix-initial-request".to_string(),
                request: SourceEditCommitRequestV1 {
                    schema: SOURCE_EDIT_COMMIT_REQUEST_SCHEMA.to_string(),
                    preview_id: preview.preview_id,
                },
            };
            let authority = issue_matrix_authority(
                temp.path(),
                &request,
                &host,
                &reconciliation_brain_id,
                &brain_id,
                SOURCE_MATRIX_SUBJECT,
                SOURCE_MATRIX_LEASE_ID,
                SOURCE_MATRIX_TRANSPORT_ID,
                "source",
            );
            Self {
                journal_root: temp.path().join("external-journal"),
                broker_config: authority.broker_config,
                linearization: authority.linearization,
                broker_operation: authority.broker_operation,
                current_authority: authority.current_authority,
                protected_journal_head: authority.protected_journal_head,
                receipt_crypto: authority.receipt_crypto,
                clock: Arc::new(AtomicU64::new(MATRIX_NOW_MS)),
                request,
                context: authority.context,
                host,
                brain_id,
                operation_object_digest: authority.operation_object_digest,
                actor_calls,
                checkpoint_acks,
                actor_registry,
                repo_root,
                runtime_root,
                target_path,
                _temp: temp,
                _serial: serial,
            }
        }

        fn service(&self) -> ExternalMutationServiceV1 {
            let clock = Arc::clone(&self.clock);
            ExternalMutationServiceV1::from_owner_inputs(ExternalMutationServiceInputsV1 {
                journal_root: self.journal_root.clone(),
                broker_config: self.broker_config.clone(),
                linearization: self.linearization.clone(),
                broker_operation: Arc::clone(&self.broker_operation),
                current_authority: Arc::clone(&self.current_authority),
                protected_journal_head: Arc::clone(&self.protected_journal_head),
                receipt_crypto: Arc::clone(&self.receipt_crypto),
                owner_clock: Arc::new(move || clock.load(Ordering::SeqCst)),
            })
        }

        fn service_crashing_at(
            &self,
            cut: &'static str,
            fired: Arc<AtomicBool>,
        ) -> ExternalMutationServiceV1 {
            self.service()
                .with_fault_hook_for_test(Arc::new(move |point| {
                    if point == cut && !fired.swap(true, Ordering::SeqCst) {
                        Err("simulated process death".to_string())
                    } else {
                        Ok(())
                    }
                }))
        }

        fn restarted_brain(&self) -> Arc<BrainSessionCell> {
            retire_bound_owner_for_restart(
                &self.actor_registry,
                &self.host.selected_brain,
                "source",
            );
            let state = Self::initialize_state(&self.repo_root, &self.runtime_root);
            assert!(state.edit_previews.is_empty());
            assert!(state.proof_ready.is_empty());
            assert!(state.active_proof_permits.is_empty());
            Arc::new(BrainSessionCell::new(state))
        }

        fn host_for_brain(
            &self,
            brain: Arc<BrainSessionCell>,
        ) -> (
            ExternalMutationExecutionHostV1,
            Arc<crate::project_brains::ProjectBrainRegistry>,
        ) {
            let expected_brain = self.host.selected_actor_brain_id.clone();
            let recovery_brain = Arc::clone(&brain);
            let actor_registry = Arc::new(crate::project_brains::ProjectBrainRegistry::new(
                self.runtime_root.join("source-replay-project-brains"),
                None,
            ));
            // The previous host's actor owner releases asynchronously; on slow
            // shared runners the new bind can race that release, so wait for it.
            let reconciliation_brain_id = {
                let mut attempt = 0;
                loop {
                    match actor_registry.bound_brain_id_for_target(Arc::clone(&brain)) {
                        Ok(id) => break id,
                        Err(_) if attempt < 300 => {
                            attempt += 1;
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                        Err(error) => {
                            panic!("source replay actor id after {attempt} waits: {error:?}")
                        }
                    }
                }
            };
            assert_eq!(
                reconciliation_brain_id, self.host.reconciliation_brain_id,
                "source actor identity must survive restart"
            );
            let actor_brain = Arc::clone(&brain);
            let runtime_jobs = actor_registry
                .runtime_job_registry()
                .expect("source replay runtime jobs");
            let registry_for_actor = Arc::clone(&actor_registry);
            let actor_calls = Arc::clone(&self.actor_calls);
            let checkpoint_acks = Arc::clone(&self.checkpoint_acks);
            let host = ExternalMutationExecutionHostV1 {
                selected_brain: brain,
                selected_actor_brain_id: reconciliation_brain_id.clone(),
                resolve_brain: Arc::new(move |requested| {
                    if requested == expected_brain {
                        Ok(Arc::clone(&recovery_brain))
                    } else {
                        Err(format!("unexpected source recovery brain '{requested}'"))
                    }
                }),
                reconcile_promote: Arc::new(move |request| {
                    let requires_checkpoint_ack = request.requires_checkpoint_ack();
                    if requires_checkpoint_ack {
                        actor_calls.fetch_add(1, Ordering::SeqCst);
                    }
                    let allows_resolved_actor_identity = request.allows_resolved_actor_identity();
                    let actual_brain_id = registry_for_actor
                        .bound_brain_id_for_target(Arc::clone(&actor_brain))
                        .map_err(|error| error.to_string())?;
                    if actual_brain_id != request.reconciliation_brain_id
                        && !allows_resolved_actor_identity
                    {
                        return Err(format!(
                            "source recovery actor mismatch: expected '{}', observed '{}'",
                            request.reconciliation_brain_id, actual_brain_id
                        ));
                    }
                    if requires_checkpoint_ack {
                        registry_for_actor
                            .execute_target_runtime_with_checkpoint_ack(
                                Arc::clone(&actor_brain),
                                None,
                                true,
                                move |state| {
                                    request.execute(state).map_err(|detail| {
                                        crate::runtime_jobs::RuntimeJobFailure::new(
                                            "source_edit_actor_job_failed",
                                            detail,
                                        )
                                    })
                                },
                            )
                            .map(|(execution, ack)| {
                                let execution = execution.bind_checkpoint_ack(&ack);
                                if let Some(ack) = execution.checkpoint_ack.clone() {
                                    checkpoint_acks.lock().push(ack);
                                }
                                execution
                            })
                            .map_err(|error| error.to_string())
                    } else {
                        registry_for_actor
                            .execute_target_runtime(
                                Arc::clone(&actor_brain),
                                None,
                                true,
                                false,
                                move |state| {
                                    request.execute(state).map_err(|detail| {
                                        crate::runtime_jobs::RuntimeJobFailure::new(
                                            "external_mutation_inspect_actor_job_failed",
                                            detail,
                                        )
                                    })
                                },
                            )
                            .map_err(|error| error.to_string())
                    }
                }),
                reconciliation_brain_id,
                promote_paths: None,
                runtime_jobs: Ok(runtime_jobs),
            };
            (host, actor_registry)
        }

        fn recover_for_boot(
            &self,
            service: &ExternalMutationServiceV1,
            brain: Arc<BrainSessionCell>,
        ) -> ExternalMutationRecoveryReportV1 {
            let expected_brain = self.host.selected_actor_brain_id.clone();
            let (recovery_host, recovery_registry) = self.host_for_brain(Arc::clone(&brain));
            let report = service
                .recover_for_boot(
                    move |requested| {
                        if requested == expected_brain {
                            Ok(Arc::clone(&brain))
                        } else {
                            Err(format!("unexpected source brain {requested}"))
                        }
                    },
                    Arc::clone(&recovery_host.reconcile_promote),
                )
                .expect("source boot recovery");
            recovery_registry
                .shutdown(Duration::from_secs(2))
                .expect("shutdown recovered source actor before restart inspection");
            report
        }

        fn adapter_prepared(&self) -> crate::surgical_handlers::PreparedSourceEditCommitV1 {
            let request = match &self.request {
                ExternalMutationRequestV1::SourceEditCommit { request, .. } => {
                    crate::surgical_handlers::SourceEditCommitRequestV1::new(
                        request.preview_id.clone(),
                    )
                }
                _ => unreachable!("source fixture request"),
            };
            let operation_object_digest = self.operation_object_digest.clone();
            let brain_id = self.host.selected_actor_brain_id.clone();
            self.actor_registry
                .execute_target_runtime(
                    Arc::clone(&self.host.selected_brain),
                    None,
                    true,
                    false,
                    move |state| {
                        let intent = crate::surgical_handlers::SourceEditCommitAdapterV1::inspect_state(
                            state,
                            &request,
                            SOURCE_MATRIX_SUBJECT,
                        )
                        .map_err(|error| {
                            crate::runtime_jobs::RuntimeJobFailure::new(
                                "source_adapter_inspect_failed",
                                error.to_string(),
                            )
                        })?;
                        let context = crate::surgical_handlers::SourceEditPreparedContextV1 {
                            authority_subject_id: SOURCE_MATRIX_SUBJECT.to_string(),
                            semantic_action: "source.edit.commit".to_string(),
                            ingress: Ingress::Mcp,
                            semantic_payload_digest: intent.semantic_payload_digest.clone(),
                            operation_object_digest,
                            expected_effects:
                                crate::surgical_handlers::SourceEditCommitAdapterV1::expected_effects(),
                            brain_id,
                            mission_id: None,
                            mission_head_id: None,
                            operation_version: EXTERNAL_MUTATION_OPERATION_VERSION,
                        };
                        crate::surgical_handlers::SourceEditCommitAdapterV1::prepare_in_actor(
                            state,
                            &request,
                            &context,
                        )
                        .map_err(|error| {
                            crate::runtime_jobs::RuntimeJobFailure::new(
                                "source_adapter_prepare_failed",
                                error.to_string(),
                            )
                        })
                    },
                )
                .expect("source adapter inspect and prepare inside actor")
        }

        fn stage_orphan_in_actor(
            &self,
            prepared: crate::surgical_handlers::PreparedSourceEditCommitV1,
        ) {
            self.actor_registry
                .execute_target_runtime(
                    Arc::clone(&self.host.selected_brain),
                    None,
                    true,
                    false,
                    move |state| {
                        prepared.stage(state).map(|_| ()).map_err(|error| {
                            crate::runtime_jobs::RuntimeJobFailure::new(
                                "source_edit_actor_test_setup_failed",
                                error.to_string(),
                            )
                        })
                    },
                )
                .expect("source durable orphan stage inside actor");
        }

        fn leave_interrupted_pre_stage_cleanup_in_actor(
            &self,
            prepared: crate::surgical_handlers::PreparedSourceEditCommitV1,
        ) {
            self.actor_registry
                .execute_target_runtime(
                    Arc::clone(&self.host.selected_brain),
                    None,
                    true,
                    false,
                    move |state| {
                        crate::surgical_handlers::SourceEditCommitAdapterV1::leave_pre_stage_orphan_for_test(
                            prepared, state,
                        )
                        .map_err(|error| {
                            crate::runtime_jobs::RuntimeJobFailure::new(
                                "source_edit_actor_test_setup_failed",
                                error.to_string(),
                            )
                        })?;
                        let recovery = crate::surgical_handlers::SourceEditCommitAdapterV1::pending_pre_stage_recovery(
                            state,
                        )
                        .map_err(|error| {
                            crate::runtime_jobs::RuntimeJobFailure::new(
                                "source_edit_actor_test_setup_failed",
                                error.to_string(),
                            )
                        })?
                        .into_values()
                        .next()
                        .ok_or_else(|| {
                            crate::runtime_jobs::RuntimeJobFailure::new(
                                "source_edit_actor_test_setup_failed",
                                "pre-stage orphan was not inventoried",
                            )
                        })?;
                        crate::surgical_handlers::SourceEditCommitAdapterV1::interrupt_pre_stage_cleanup_for_test(
                            state,
                            &recovery.transaction_id,
                            &recovery.operation_object_digest,
                            &recovery.intent_digest,
                        )
                        .map_err(|error| {
                            crate::runtime_jobs::RuntimeJobFailure::new(
                                "source_edit_actor_test_setup_failed",
                                error.to_string(),
                            )
                        })?;
                        Ok(())
                    },
                )
                .expect("leave interrupted pre-stage cleanup inside actor");
        }

        fn reserve_without_journal(&self) {
            let service = self.service();
            let mut broker = service.open_broker().expect("source orphan broker");
            broker
                .reserve(
                    SOURCE_MATRIX_LEASE_ID,
                    SOURCE_MATRIX_TRANSPORT_ID,
                    self.context
                        .ingress_context_digest
                        .as_deref()
                        .expect("source ingress digest"),
                    &self.operation_object_digest,
                    MATRIX_NOW_MS,
                )
                .expect("source orphan reservation");
        }

        fn target_bytes(&self) -> Vec<u8> {
            std::fs::read(&self.target_path).expect("source target bytes")
        }

        fn assert_only_managed_target_remains(&self) {
            let entries = std::fs::read_dir(self.target_path.parent().expect("source parent"))
                .expect("source parent entries")
                .map(|entry| entry.expect("source entry").path())
                .collect::<Vec<_>>();
            assert_eq!(entries, vec![self.target_path.clone()]);
        }

        fn pending_recovery_is_empty(&self, brain: Arc<BrainSessionCell>) -> bool {
            let registry = crate::project_brains::ProjectBrainRegistry::new(
                self.runtime_root.join("source-pending-read-project-brains"),
                None,
            );
            registry
                .read_target_runtime_snapshot(brain, None, true, |state| {
                    crate::surgical_handlers::SourceEditCommitAdapterV1::pending_recovery(state)
                        .map(|pending| pending.is_empty())
                        .map_err(|error| {
                            crate::runtime_jobs::RuntimeJobFailure::new(
                                "source_pending_recovery_read_failed",
                                error.to_string(),
                            )
                        })
                })
                .expect("source pending recovery actor snapshot")
                .value
        }
    }

    #[test]
    fn source_outer_cut_matrix_recovers_boot_preserves_target_and_replays_pending_reconciliation() {
        let cuts = [
            ("after_reserve", MatrixRecoveryExpectation::SafeAbort),
            ("after_stage", MatrixRecoveryExpectation::SafeAbort),
            (
                "after_journal_prepared",
                MatrixRecoveryExpectation::SafeAbort,
            ),
            (
                "after_broker_finalization_prepared",
                MatrixRecoveryExpectation::SafeAbort,
            ),
            (
                "after_journal_committed",
                MatrixRecoveryExpectation::ForwardComplete,
            ),
            (
                "after_broker_consumed",
                MatrixRecoveryExpectation::ForwardComplete,
            ),
            (
                "after_domain_publish",
                MatrixRecoveryExpectation::ForwardComplete,
            ),
            (
                "after_journal_published",
                MatrixRecoveryExpectation::AlreadyPublished,
            ),
        ];
        for (cut, expectation) in cuts {
            let fixture = SourceMatrixFixture::new();
            let metadata_before = std::fs::metadata(&fixture.target_path).expect("source metadata");
            let fired = Arc::new(AtomicBool::new(false));
            fixture
                .service_crashing_at(cut, Arc::clone(&fired))
                .execute(
                    &fixture.context,
                    fixture.request.clone(),
                    fixture.host.clone(),
                )
                .expect_err(cut);
            assert!(fired.load(Ordering::SeqCst), "cut not reached: {cut}");

            let service = fixture.service();
            let restarted = fixture.restarted_brain();
            let report = fixture.recover_for_boot(&service, Arc::clone(&restarted));
            match expectation {
                MatrixRecoveryExpectation::SafeAbort => {
                    assert_eq!(report.safely_aborted_pre_finalization, 1, "{cut}");
                    assert_eq!(
                        fixture.target_bytes(),
                        SOURCE_MATRIX_BEFORE.as_bytes(),
                        "{cut}"
                    );
                    #[cfg(unix)]
                    {
                        let after =
                            std::fs::metadata(&fixture.target_path).expect("source metadata after");
                        assert_eq!(after.ino(), metadata_before.ino(), "{cut}");
                        assert_eq!(
                            after.permissions().mode(),
                            metadata_before.permissions().mode()
                        );
                    }
                }
                MatrixRecoveryExpectation::ForwardComplete => {
                    assert_eq!(report.forward_completed, 1, "{cut}");
                    assert_eq!(
                        fixture.target_bytes(),
                        SOURCE_MATRIX_AFTER.as_bytes(),
                        "{cut}"
                    );
                }
                MatrixRecoveryExpectation::AlreadyPublished => {
                    assert_eq!(report.already_published, 1, "{cut}");
                    assert_eq!(
                        fixture.target_bytes(),
                        SOURCE_MATRIX_AFTER.as_bytes(),
                        "{cut}"
                    );
                }
            }
            #[cfg(unix)]
            assert_eq!(
                std::fs::metadata(&fixture.target_path)
                    .expect("source terminal metadata")
                    .permissions()
                    .mode(),
                metadata_before.permissions().mode(),
                "{cut}"
            );
            fixture.assert_only_managed_target_remains();
            assert!(
                fixture.pending_recovery_is_empty(Arc::clone(&restarted)),
                "{cut}: pending recovery remained"
            );
            assert!(service
                .conservation_scan()
                .expect("source conservation")
                .anomalies
                .is_empty());

            if !matches!(expectation, MatrixRecoveryExpectation::SafeAbort) {
                let (replay_host, replay_registry) = fixture.host_for_brain(Arc::clone(&restarted));
                let retry = request_with_id(fixture.request.clone(), format!("retry-{cut}"));
                let replay = service
                    .execute(&fixture.context, retry, replay_host)
                    .expect("source terminal replay");
                replay_registry
                    .shutdown(Duration::from_secs(2))
                    .expect("shutdown source replay actor");
                assert!(replay.graph_resync_required);
                assert_eq!(replay.reconciliation_state, "PENDING_RECONCILIATION");
                assert_eq!(replay.result["terminal_replay"], Value::Bool(true));
                assert_eq!(fixture.target_bytes(), SOURCE_MATRIX_AFTER.as_bytes());
            }
        }
    }

    #[test]
    fn source_first_response_and_replay_share_the_exact_pending_result() {
        let fixture = SourceMatrixFixture::new();
        assert!(
            fixture.host.selected_brain.lock_mut_before_actor().is_err(),
            "source fixture must fence raw mutable session access once its actor is active"
        );
        let service = fixture.service();
        let first = service
            .execute(
                &fixture.context,
                fixture.request.clone(),
                fixture.host.clone(),
            )
            .expect("source first response");
        assert!(first.graph_resync_required);
        assert_eq!(first.reconciliation_state, "PENDING_RECONCILIATION");
        assert!(first.result.get("terminal_replay").is_none());
        assert_eq!(fixture.actor_calls.load(Ordering::SeqCst), 1);
        let checkpoint_acks = fixture.checkpoint_acks.lock();
        assert_eq!(checkpoint_acks.len(), 1);
        let checkpoint_ack = &checkpoint_acks[0];
        assert_eq!(
            first.result["checkpoint_ack"]["checkpoint_id"],
            Value::String(checkpoint_ack.checkpoint_id.clone())
        );
        assert_eq!(
            first.result["checkpoint_ack"]["brain_id"],
            Value::String(fixture.host.reconciliation_brain_id.clone())
        );
        assert_eq!(
            first.result["checkpoint_ack"]["revision"],
            json!(checkpoint_ack.revision)
        );
        assert!(checkpoint_ack.revision > 0);
        assert_eq!(first.result["actor_checkpoint_required"], Value::Bool(true));
        drop(checkpoint_acks);
        let retry = request_with_id(fixture.request.clone(), "source-sealed-retry");
        let replay = service
            .execute(&fixture.context, retry, fixture.host.clone())
            .expect("source terminal replay");
        assert_eq!(replay.result["terminal_replay"], Value::Bool(true));
        assert_first_and_replay_are_equivalent(&first, &replay);
        assert_eq!(
            fixture.actor_calls.load(Ordering::SeqCst),
            1,
            "terminal replay must not re-enter the actor or reapply source bytes"
        );
    }

    #[test]
    fn source_actor_never_publishes_or_checkpoints_before_outer_committed() {
        let fixture = SourceMatrixFixture::new();
        let fired = Arc::new(AtomicBool::new(false));
        let error = fixture
            .service_crashing_at("after_journal_prepared", Arc::clone(&fired))
            .execute(
                &fixture.context,
                fixture.request.clone(),
                fixture.host.clone(),
            )
            .expect_err("PREPARED source edit must abort the waiting actor");
        assert!(fired.load(Ordering::SeqCst));
        assert_ne!(error.code(), "external_mutation_recovery_required");
        assert_eq!(fixture.target_bytes(), SOURCE_MATRIX_BEFORE.as_bytes());
        assert_eq!(fixture.actor_calls.load(Ordering::SeqCst), 1);
        assert!(fixture.checkpoint_acks.lock().is_empty());
        let entry = fixture
            .service()
            .open_journal()
            .expect("source PREPARED journal")
            .entries()
            .into_iter()
            .next()
            .expect("source PREPARED entry");
        assert_eq!(entry.phase, ExternalMutationJournalPhaseV1::Prepared);
        assert!(entry.outcome_digest.is_none());
    }

    #[test]
    fn source_refuses_foreign_actor_ack_then_recovers_through_exact_actor_checkpoint() {
        let fixture = SourceMatrixFixture::new();
        let mut forged_host = fixture.host.clone();
        let real_reconciler = Arc::clone(&forged_host.reconcile_promote);
        forged_host.reconcile_promote = Arc::new(move |request| {
            let mut execution = real_reconciler(request)?;
            if let Some(checkpoint_ack) = execution.checkpoint_ack.as_mut() {
                checkpoint_ack.brain_id = "project-brain-foreign".to_string();
            }
            Ok(execution)
        });
        let error = fixture
            .service()
            .execute(&fixture.context, fixture.request.clone(), forged_host)
            .expect_err("foreign checkpoint ACK must refuse publication sealing");
        assert_eq!(error.code(), "external_mutation_recovery_required");
        assert_eq!(fixture.target_bytes(), SOURCE_MATRIX_AFTER.as_bytes());
        let entry = fixture
            .service()
            .open_journal()
            .expect("source recovery-required journal")
            .entries()
            .into_iter()
            .next()
            .expect("source recovery-required entry");
        assert_eq!(
            entry.phase,
            ExternalMutationJournalPhaseV1::RecoveryRequired
        );

        let restarted = fixture.restarted_brain();
        let report = fixture.recover_for_boot(&fixture.service(), Arc::clone(&restarted));
        assert_eq!(report.forward_completed, 1);
        let published = fixture
            .service()
            .open_journal()
            .expect("source recovered journal")
            .entry(&entry.operation_id)
            .cloned()
            .expect("source recovered entry");
        assert_eq!(published.phase, ExternalMutationJournalPhaseV1::Published);
        let sealed_ack = &published
            .published_result
            .expect("source recovered result")
            .result["checkpoint_ack"];
        assert_eq!(
            sealed_ack["brain_id"],
            Value::String(fixture.host.reconciliation_brain_id.clone())
        );
    }

    #[test]
    fn source_service_boot_cleans_staged_orphan_without_target_write() {
        let fixture = SourceMatrixFixture::new();
        fixture.reserve_without_journal();
        let before = std::fs::metadata(&fixture.target_path).expect("source metadata");
        let prepared = fixture.adapter_prepared();
        fixture.stage_orphan_in_actor(prepared);
        assert_eq!(fixture.target_bytes(), SOURCE_MATRIX_BEFORE.as_bytes());

        let service = fixture.service();
        let restarted = fixture.restarted_brain();
        let report = fixture.recover_for_boot(&service, Arc::clone(&restarted));
        assert_eq!(report.safely_aborted_pre_finalization, 1);
        assert_eq!(fixture.target_bytes(), SOURCE_MATRIX_BEFORE.as_bytes());
        #[cfg(unix)]
        {
            let after = std::fs::metadata(&fixture.target_path).expect("source metadata after");
            assert_eq!(after.ino(), before.ino());
            assert_eq!(after.permissions().mode(), before.permissions().mode());
        }
        fixture.assert_only_managed_target_remains();
        assert!(fixture.pending_recovery_is_empty(restarted));
    }

    #[test]
    fn source_service_boot_rediscovers_and_finishes_interrupted_pre_stage_cleanup() {
        let fixture = SourceMatrixFixture::new();
        fixture.reserve_without_journal();
        let before = std::fs::metadata(&fixture.target_path).expect("source metadata");
        let prepared = fixture.adapter_prepared();
        fixture.leave_interrupted_pre_stage_cleanup_in_actor(prepared);
        assert_eq!(fixture.target_bytes(), SOURCE_MATRIX_BEFORE.as_bytes());

        let service = fixture.service();
        let restarted = fixture.restarted_brain();
        let report = fixture.recover_for_boot(&service, Arc::clone(&restarted));
        assert_eq!(report.safely_aborted_pre_finalization, 1);
        assert_eq!(fixture.target_bytes(), SOURCE_MATRIX_BEFORE.as_bytes());
        #[cfg(unix)]
        {
            let after = std::fs::metadata(&fixture.target_path).expect("source metadata after");
            assert_eq!(after.ino(), before.ino());
            assert_eq!(after.permissions().mode(), before.permissions().mode());
        }
        fixture.assert_only_managed_target_remains();
        assert!(fixture.pending_recovery_is_empty(restarted));
    }

    #[test]
    fn source_recovery_refuses_forged_transaction_operation_or_stage_before_broker_and_target() {
        for field in ["transaction_id", "operation_object_digest", "stage_digest"] {
            let fixture = SourceMatrixFixture::new();
            let metadata_before = std::fs::metadata(&fixture.target_path).expect("source metadata");
            let fired = Arc::new(AtomicBool::new(false));
            fixture
                .service_crashing_at("after_journal_committed", Arc::clone(&fired))
                .execute(
                    &fixture.context,
                    fixture.request.clone(),
                    fixture.host.clone(),
                )
                .expect_err("stop after source commit");
            assert!(fired.load(Ordering::SeqCst));
            let service = fixture.service();
            let mut forged = service
                .open_journal()
                .expect("source journal")
                .entries()
                .into_iter()
                .next()
                .expect("source committed entry");
            forged.prepare.recovery_payload[field] =
                json!(matrix_hash(&format!("forged-source-{field}")));
            let lease_before = service
                .open_broker()
                .expect("source broker before forged recovery")
                .lease(SOURCE_MATRIX_LEASE_ID)
                .cloned()
                .expect("source lease before forged recovery");
            let mut broker = service.open_broker().expect("source recovery broker");
            let error = match service.forward_complete_committed_entry_with_broker(
                &mut broker,
                &forged,
                fixture.host.reconcile_promote.as_ref(),
            ) {
                Err(error) => error,
                Ok(_) => panic!("forged source {field} must refuse"),
            };
            assert_eq!(error.code(), "source_edit_generic_recovery_forbidden");
            drop(broker);
            let lease_after = service
                .open_broker()
                .expect("source broker after forged recovery")
                .lease(SOURCE_MATRIX_LEASE_ID)
                .cloned()
                .expect("source lease after forged recovery");
            assert_eq!(lease_after.state, lease_before.state, "{field}");
            assert_eq!(
                fixture.target_bytes(),
                SOURCE_MATRIX_BEFORE.as_bytes(),
                "{field}"
            );
            #[cfg(unix)]
            {
                let metadata_after =
                    std::fs::metadata(&fixture.target_path).expect("source metadata after refusal");
                assert_eq!(metadata_after.ino(), metadata_before.ino(), "{field}");
                assert_eq!(
                    metadata_after.permissions().mode(),
                    metadata_before.permissions().mode(),
                    "{field}"
                );
            }

            let restarted = fixture.restarted_brain();
            let report = fixture.recover_for_boot(&service, restarted);
            assert_eq!(report.forward_completed, 1, "{field}");
            assert_eq!(
                fixture.target_bytes(),
                SOURCE_MATRIX_AFTER.as_bytes(),
                "{field}"
            );
        }
    }

    #[test]
    fn source_recovery_refuses_foreign_runtime_owner_before_broker_or_either_target_write() {
        let fixture = SourceMatrixFixture::new();
        let metadata_before = std::fs::metadata(&fixture.target_path).expect("source metadata");
        let fired = Arc::new(AtomicBool::new(false));
        fixture
            .service_crashing_at("after_journal_committed", Arc::clone(&fired))
            .execute(
                &fixture.context,
                fixture.request.clone(),
                fixture.host.clone(),
            )
            .expect_err("stop after source commit");
        assert!(fired.load(Ordering::SeqCst));

        let foreign_repo = fixture._temp.path().join("foreign-source-repo");
        let foreign_runtime = fixture._temp.path().join("foreign-source-runtime");
        let foreign_target = foreign_repo.join("src/lib.rs");
        std::fs::create_dir_all(foreign_target.parent().expect("foreign source parent"))
            .expect("foreign source tree");
        std::fs::create_dir_all(&foreign_runtime).expect("foreign source runtime");
        let foreign_before = b"foreign source sentinel\n";
        std::fs::write(&foreign_target, foreign_before).expect("foreign source sentinel");
        let foreign = Arc::new(BrainSessionCell::new(
            SourceMatrixFixture::initialize_state(&foreign_repo, &foreign_runtime),
        ));

        let service = fixture.service();
        let entry = service
            .open_journal()
            .expect("source journal")
            .entries()
            .into_iter()
            .next()
            .expect("source committed entry");
        let lease_before = service
            .open_broker()
            .expect("source broker before foreign owner")
            .lease(SOURCE_MATRIX_LEASE_ID)
            .cloned()
            .expect("source lease before foreign owner");
        let mut broker = service.open_broker().expect("source recovery broker");
        let error = match service.forward_complete_committed_entry_with_broker(
            &mut broker,
            &entry,
            fixture.host.reconcile_promote.as_ref(),
        ) {
            Err(error) => error,
            Ok(_) => panic!("foreign source runtime must refuse"),
        };
        assert_eq!(error.code(), "source_edit_generic_recovery_forbidden");
        drop(broker);
        let lease_after = service
            .open_broker()
            .expect("source broker after foreign owner")
            .lease(SOURCE_MATRIX_LEASE_ID)
            .cloned()
            .expect("source lease after foreign owner");
        assert_eq!(lease_after.state, lease_before.state);
        assert_eq!(fixture.target_bytes(), SOURCE_MATRIX_BEFORE.as_bytes());
        assert_eq!(std::fs::read(&foreign_target).unwrap(), foreign_before);
        #[cfg(unix)]
        assert_eq!(
            std::fs::metadata(&fixture.target_path)
                .expect("source metadata after foreign refusal")
                .ino(),
            metadata_before.ino()
        );

        let restarted = fixture.restarted_brain();
        let report = fixture.recover_for_boot(&service, restarted);
        assert_eq!(report.forward_completed, 1);
        assert_eq!(fixture.target_bytes(), SOURCE_MATRIX_AFTER.as_bytes());
        assert_eq!(std::fs::read(&foreign_target).unwrap(), foreign_before);
    }

    struct PromoteMatrixFixture {
        _temp: TempDir,
        journal_root: PathBuf,
        broker_config: OwnerAuthorizationBrokerConfigV1,
        linearization: OwnerAuthorityLinearizationV1,
        broker_operation: Arc<Mutex<()>>,
        current_authority: Arc<AuthorityStatusReader>,
        protected_journal_head: SharedProtectedJournalHeadBackendV1,
        receipt_crypto: Arc<dyn AuthorityWalRecordCrypto>,
        clock: Arc<AtomicU64>,
        request: ExternalMutationRequestV1,
        context: MissionServiceTransportContextV1,
        host: ExternalMutationExecutionHostV1,
        actor_registry: Arc<crate::project_brains::ProjectBrainRegistry>,
        brain_id: String,
        publish_targets: Vec<(PathBuf, Vec<u8>)>,
    }

    impl PromoteMatrixFixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().expect("matrix tempdir");
            let source_root = temp.path().join("project");
            let source_store = source_root.join("agent-memory");
            let medulla_runtime = temp.path().join("medulla");
            let medulla_store = medulla_runtime.join("agent-memory");
            std::fs::create_dir_all(&source_store).expect("source store");
            std::fs::create_dir_all(&medulla_store).expect("medulla store");

            let source_text = concat!(
                "---\n",
                "Node: crash-node\n",
                "State: verified\n",
                "Source-Agent: human:maintainer\n",
                "---\n",
                "# crash-node\n\n",
                "Source invariant.\n",
                "[⍂ entity: crash-node]\n",
                "[𝔻 confidence: 0.95]\n"
            );
            let medulla_text = concat!(
                "---\n",
                "Node: crash-node\n",
                "State: draft\n",
                "Source-Agent: agent:old\n",
                "---\n",
                "# crash-node\n\n",
                "Older weak claim.\n",
                "[⍂ entity: crash-node]\n",
                "[𝔻 confidence: 0.20]\n"
            );
            let source_path = source_store.join("crash-node.light.md");
            let medulla_path = medulla_store.join("crash-node.light.md");
            std::fs::write(&source_path, source_text).expect("source preimage");
            std::fs::write(&medulla_path, medulla_text).expect("medulla preimage");

            let brain_id = source_root.to_string_lossy().to_string();
            let subject = "owner:matrix";
            let promote_input = crate::promote_handlers::PromoteInput {
                agent_id: subject.to_string(),
                brain: brain_id.clone(),
                claim: "crash-node".to_string(),
                reason: "cross-brain invariant".to_string(),
            };
            let plan = crate::promote_handlers::plan_external_promotion(
                &promote_input,
                source_text,
                Some(medulla_text),
                MATRIX_NOW_MS,
            )
            .expect("four-write promotion plan");
            let medulla_history = plan
                .medulla_history
                .clone()
                .expect("matrix must supersede an old medulla claim");

            // The reconciliation actor is the medulla owner. Its runtime root
            // must be the exact root sealed into the promotion plan.
            let session_runtime = medulla_runtime.clone();
            std::fs::create_dir_all(&session_runtime).expect("session runtime");
            let config = McpConfig {
                graph_source: session_runtime.join("graph.json"),
                plasticity_state: session_runtime.join("plasticity.json"),
                runtime_dir: Some(session_runtime),
                ..McpConfig::default()
            };
            let state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
                .expect("matrix session");
            let selected_brain = Arc::new(BrainSessionCell::new(state));
            let recovery_brain = Arc::clone(&selected_brain);
            let reconciliation_brain = Arc::clone(&selected_brain);
            let reconciliation_registry =
                Arc::new(crate::project_brains::ProjectBrainRegistry::new(
                    temp.path().join("matrix-project-brains"),
                    None,
                ));
            let reconciliation_brain_id = reconciliation_registry
                .bound_brain_id_for_target(Arc::clone(&reconciliation_brain))
                .expect("matrix bound actor id");
            let runtime_jobs = reconciliation_registry
                .runtime_job_registry()
                .expect("matrix runtime jobs");
            let recovery_brain_id = reconciliation_brain_id.clone();
            let actor_registry = Arc::clone(&reconciliation_registry);
            let host = ExternalMutationExecutionHostV1 {
                selected_brain,
                selected_actor_brain_id: reconciliation_brain_id.clone(),
                resolve_brain: Arc::new(move |requested| {
                    if requested == recovery_brain_id {
                        Ok(Arc::clone(&recovery_brain))
                    } else {
                        Err(format!("unexpected recovery brain '{requested}'"))
                    }
                }),
                reconcile_promote: Arc::new(move |request| {
                    let requires_checkpoint_ack = request.requires_checkpoint_ack();
                    let allows_resolved_actor_identity = request.allows_resolved_actor_identity();
                    let actual_brain_id = actor_registry
                        .bound_brain_id_for_target(Arc::clone(&reconciliation_brain))
                        .map_err(|error| error.to_string())?;
                    if actual_brain_id != request.reconciliation_brain_id
                        && !allows_resolved_actor_identity
                    {
                        return Err(format!(
                            "reconciliation actor mismatch: expected '{}', observed '{}'",
                            request.reconciliation_brain_id, actual_brain_id
                        ));
                    }
                    if requires_checkpoint_ack {
                        actor_registry
                            .execute_target_runtime_with_checkpoint_ack(
                                Arc::clone(&reconciliation_brain),
                                None,
                                true,
                                move |state| {
                                    request.execute(state).map_err(|detail| {
                                        crate::runtime_jobs::RuntimeJobFailure::new(
                                            "brain_promote_actor_job_failed",
                                            detail,
                                        )
                                    })
                                },
                            )
                            .map(|(execution, ack)| execution.bind_checkpoint_ack(&ack))
                            .map_err(|error| error.to_string())
                    } else {
                        actor_registry
                            .execute_target_runtime(
                                Arc::clone(&reconciliation_brain),
                                None,
                                true,
                                false,
                                move |state| {
                                    request.execute(state).map_err(|detail| {
                                        crate::runtime_jobs::RuntimeJobFailure::new(
                                            "external_mutation_inspect_actor_job_failed",
                                            detail,
                                        )
                                    })
                                },
                            )
                            .map_err(|error| error.to_string())
                    }
                }),
                reconciliation_brain_id: reconciliation_brain_id.clone(),
                promote_paths: Some(ExternalPromotePathsV1 {
                    source_store_dir: source_store.clone(),
                    medulla_store_dir: medulla_store.clone(),
                    medulla_runtime_root: medulla_runtime,
                }),
                runtime_jobs: Ok(runtime_jobs),
            };
            let request = ExternalMutationRequestV1::BrainPromote {
                schema: EXTERNAL_MUTATION_REQUEST_SCHEMA.to_string(),
                request_id: "matrix-initial-request".to_string(),
                source_brain: brain_id.clone(),
                claim: "crash-node".to_string(),
                reason: "cross-brain invariant".to_string(),
                expected_source_sha256: sha256_bytes(source_text.as_bytes()),
                expected_medulla_sha256: Some(sha256_bytes(medulla_text.as_bytes())),
            };
            let inspected = inspect_request(
                &request,
                &host,
                subject,
                MATRIX_NOW_MS,
                &[],
                &reconciliation_brain_id,
            )
            .expect("matrix request inspects");
            let semantic_payload_digest = inspected.semantic_payload_digest().to_string();
            let operation_object_digest = digest_canonical(
                EXTERNAL_MUTATION_OPERATION_OBJECT_DIGEST_DOMAIN,
                &ExternalMutationOperationObjectV1 {
                    schema: EXTERNAL_MUTATION_OPERATION_OBJECT_SCHEMA,
                    semantic_action: request.semantic_action_id(),
                    ingress: Ingress::Mcp,
                    brain_id: &reconciliation_brain_id,
                    mission_id: None,
                    mission_head_id: None,
                    operation_version: EXTERNAL_MUTATION_OPERATION_VERSION,
                    semantic_payload_digest: &semantic_payload_digest,
                },
            )
            .expect("matrix operation object");
            let ingress_context_digest = matrix_hash("ingress-context");
            let contract = external_consumer_contract("brain.promote", Ingress::Mcp)
                .expect("external promote contract");
            let crypto: Arc<dyn AuthorityWalRecordCrypto> = Arc::new(
                SoftwareTestAuthorityWalRecordCrypto::explicit_not_production(
                    b"external-promote-matrix-receipt",
                ),
            );
            let receipt = sign_matrix_receipt(
                AuthorityAuthorizationReceiptV1::new_for_broker_test(
                    AuthorityAuthorizationReceiptCoreV1 {
                        organism_id: "organism-matrix".to_string(),
                        repo_id: "repo-matrix".to_string(),
                        brain_id: reconciliation_brain_id.clone(),
                        subject_id: subject.to_string(),
                        role: Role::Author,
                        capability_id: "capability-matrix".to_string(),
                        capability_kind: Some(CapabilityKind::Human),
                        verified_object_digest: operation_object_digest,
                        mission_id: None,
                        mission_head_id: None,
                        transport_session_id: MATRIX_TRANSPORT_ID.to_string(),
                        ingress_context_digest: ingress_context_digest.clone(),
                        action: ActionId::new("brain.promote").expect("action id"),
                        ingress: Ingress::Mcp,
                        complete_effects: contract.expected_effects.clone(),
                        active_mode: ActiveMode::HumanGated,
                        constitution_digest: matrix_hash("constitution"),
                        constitution_epoch: 7,
                        autonomy_epoch: 0,
                        protected_epoch_at_decision: 11,
                        policy_registry_digest: matrix_hash("policy"),
                        exact_policy_tuple: ReachablePolicyTupleV1 {
                            ingress: Ingress::Mcp,
                            action: ActionId::new("brain.promote").expect("policy action"),
                            active_mode: ActiveMode::HumanGated,
                            subject_id: subject.to_string(),
                            authority_variant: AuthorityVariant::Human,
                            applicable_grant_id: None,
                            applicable_tier: None,
                            risk_class: RiskClass::Critical,
                        },
                        authority_decision_digest: Some(matrix_hash("decision")),
                        autonomy_admission_receipt_digest: None,
                        autonomy_committed_state_digest: None,
                        autonomy_protected_root_digest: None,
                        authority: AuthorizationAuthorityV1::Positive {
                            variant: AuthorityVariant::Human,
                            assurance: AuthorityVerificationAssurance::SoftwareTestOnlyNotProven,
                        },
                        authority_body_digest: matrix_hash("authority-body"),
                        replay_sequence: 3,
                        journal_sequence: 11,
                        journal_root_digest: matrix_hash("journal-root"),
                        protected_epoch: 11,
                        authorized_at: MATRIX_NOW_MS,
                        expires_at: MATRIX_NOW_MS + 10_000,
                    },
                ),
                crypto.as_ref(),
            );
            let status = matrix_status(&receipt);
            let current_authority: Arc<AuthorityStatusReader> =
                Arc::new(move || Ok(status.clone()));
            let protected_journal_head = SoftwareTestProtectedJournalHeadBackendV1::new().shared();
            let broker_config = OwnerAuthorizationBrokerConfigV1 {
                root: temp.path().join("broker"),
                reservation_ttl_ms: 100,
                minimum_terminal_retention_ms: 100,
            };
            let linearization = OwnerAuthorityLinearizationV1::default();
            {
                let mut broker = OwnerAuthorizationBrokerV1::open_with_protected_head(
                    broker_config.clone(),
                    linearization.clone(),
                    Arc::clone(&protected_journal_head),
                )
                .expect("matrix broker");
                broker
                    .issue(MATRIX_LEASE_ID, receipt, MATRIX_NOW_MS)
                    .expect("matrix lease");
            }
            let context = MissionServiceTransportContextV1 {
                ingress: MissionServiceIngressV1::McpStreamableHttp,
                transport_session_id: Some(MATRIX_TRANSPORT_ID.to_string()),
                ingress_context_digest: Some(ingress_context_digest),
                authority_lease_id: Some(MATRIX_LEASE_ID.to_string()),
                caller_root: Some(brain_id.clone()),
                route_selector: Some(brain_id.clone()),
                actor_brain_id: Some(reconciliation_brain_id.clone()),
            };
            let publish_targets = vec![
                (
                    medulla_store
                        .join(".history")
                        .join(format!("crash-node.{MATRIX_NOW_MS}.light.md")),
                    medulla_history.into_bytes(),
                ),
                (medulla_path, plan.medulla_postimage.into_bytes()),
                (
                    source_store
                        .join(".history")
                        .join(format!("crash-node.{MATRIX_NOW_MS}.light.md")),
                    plan.source_history.into_bytes(),
                ),
                (source_path, plan.source_postimage.into_bytes()),
            ];
            Self {
                _temp: temp,
                journal_root: source_root.join("external-journal"),
                broker_config,
                linearization,
                broker_operation: Arc::new(Mutex::new(())),
                current_authority,
                protected_journal_head,
                receipt_crypto: crypto,
                clock: Arc::new(AtomicU64::new(MATRIX_NOW_MS)),
                request,
                context,
                host,
                actor_registry: reconciliation_registry,
                brain_id,
                publish_targets,
            }
        }

        fn graph_generation(&self) -> u64 {
            self.actor_registry
                .read_target_runtime_snapshot(
                    Arc::clone(&self.host.selected_brain),
                    None,
                    true,
                    |state| Ok(state.graph_generation),
                )
                .expect("promotion actor graph snapshot")
                .value
        }

        fn service(&self) -> ExternalMutationServiceV1 {
            let clock = Arc::clone(&self.clock);
            self.service_with_runtime(
                self.linearization.clone(),
                Arc::clone(&self.broker_operation),
                Arc::clone(&self.current_authority),
                Arc::new(move || clock.load(Ordering::SeqCst)),
            )
        }

        fn service_with_runtime(
            &self,
            linearization: OwnerAuthorityLinearizationV1,
            broker_operation: Arc<Mutex<()>>,
            current_authority: Arc<AuthorityStatusReader>,
            owner_clock: Arc<dyn Fn() -> u64 + Send + Sync>,
        ) -> ExternalMutationServiceV1 {
            ExternalMutationServiceV1::from_owner_inputs(ExternalMutationServiceInputsV1 {
                journal_root: self.journal_root.clone(),
                broker_config: self.broker_config.clone(),
                linearization,
                broker_operation,
                current_authority,
                protected_journal_head: Arc::clone(&self.protected_journal_head),
                receipt_crypto: Arc::clone(&self.receipt_crypto),
                owner_clock,
            })
        }

        fn service_crashing_at(
            &self,
            cut: &'static str,
            fired: Arc<AtomicBool>,
        ) -> ExternalMutationServiceV1 {
            self.service()
                .with_fault_hook_for_test(Arc::new(move |point| {
                    if point == cut && !fired.swap(true, Ordering::SeqCst) {
                        Err("simulated process death".to_string())
                    } else {
                        Ok(())
                    }
                }))
        }

        fn published_target_count(&self) -> usize {
            self.publish_targets
                .iter()
                .filter(|(path, expected)| {
                    std::fs::read(path).is_ok_and(|actual| actual.as_slice() == expected.as_slice())
                })
                .count()
        }

        fn publish_target_debug(&self) -> Vec<(PathBuf, Option<String>, String)> {
            self.publish_targets
                .iter()
                .map(|(path, expected)| {
                    (
                        path.clone(),
                        std::fs::read(path).ok().map(|actual| sha256_bytes(&actual)),
                        sha256_bytes(expected),
                    )
                })
                .collect()
        }

        fn recover(&self, service: &ExternalMutationServiceV1) -> ExternalMutationRecoveryReportV1 {
            let expected_brain = self.host.selected_actor_brain_id.clone();
            let brain = Arc::clone(&self.host.selected_brain);
            let reconcile_promote = Arc::clone(&self.host.reconcile_promote);
            service
                .recover_pending(
                    move |requested| {
                        if requested == expected_brain {
                            Ok(Arc::clone(&brain))
                        } else {
                            Err(format!("unexpected brain {requested}"))
                        }
                    },
                    reconcile_promote,
                )
                .expect("matrix recovery")
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum MatrixRecoveryExpectation {
        SafeAbort,
        ForwardComplete,
        AlreadyPublished,
    }

    #[test]
    fn external_promote_fourteen_cut_crash_matrix_and_terminal_replay() {
        let cuts = [
            ("after_reserve", 0, MatrixRecoveryExpectation::SafeAbort),
            ("after_stage", 0, MatrixRecoveryExpectation::SafeAbort),
            (
                "after_journal_prepared",
                0,
                MatrixRecoveryExpectation::SafeAbort,
            ),
            (
                "after_broker_finalization_prepared",
                0,
                MatrixRecoveryExpectation::SafeAbort,
            ),
            (
                "after_journal_committed",
                0,
                MatrixRecoveryExpectation::ForwardComplete,
            ),
            (
                "after_broker_consumed",
                0,
                MatrixRecoveryExpectation::ForwardComplete,
            ),
            (
                "after_promote_medulla_history",
                1,
                MatrixRecoveryExpectation::ForwardComplete,
            ),
            (
                "after_promote_medulla_live",
                2,
                MatrixRecoveryExpectation::ForwardComplete,
            ),
            (
                "after_promote_source_history",
                3,
                MatrixRecoveryExpectation::ForwardComplete,
            ),
            (
                "after_promote_source_live",
                4,
                MatrixRecoveryExpectation::ForwardComplete,
            ),
            (
                "after_domain_publish",
                4,
                MatrixRecoveryExpectation::ForwardComplete,
            ),
            (
                "after_graph_checkpoint_ack",
                4,
                MatrixRecoveryExpectation::ForwardComplete,
            ),
            (
                "after_graph_reconciled",
                4,
                MatrixRecoveryExpectation::ForwardComplete,
            ),
            (
                "after_journal_published",
                4,
                MatrixRecoveryExpectation::AlreadyPublished,
            ),
        ];
        for (cut, expected_pre_recovery_targets, expectation) in cuts {
            let fixture = PromoteMatrixFixture::new();
            let fired = Arc::new(AtomicBool::new(false));
            let crashing = fixture.service_crashing_at(cut, Arc::clone(&fired));
            let error = crashing
                .execute(
                    &fixture.context,
                    fixture.request.clone(),
                    fixture.host.clone(),
                )
                .expect_err(cut);
            assert!(fired.load(Ordering::SeqCst), "cut was not reached: {cut}");
            assert!(
                matches!(
                    error.code(),
                    "external_mutation_injected_crash"
                        | "external_mutation_commit_failed"
                        | "external_mutation_recovery_required"
                ),
                "unexpected {cut} error: {error}"
            );
            assert_eq!(
                fixture.published_target_count(),
                expected_pre_recovery_targets,
                "wrong visible prefix at {cut}"
            );

            // Every cut is recovered at the original timestamp. Broker-held
            // protected absence proves the pre-PREPARED cuts immediately; the
            // protected PREPARED witness proves every later safe abort.
            let recovery_service = fixture.service();
            let report = fixture.recover(&recovery_service);
            match expectation {
                MatrixRecoveryExpectation::SafeAbort => {
                    assert_eq!(report.safely_aborted_pre_finalization, 1, "{cut}");
                    assert_eq!(fixture.published_target_count(), 0, "{cut}");
                }
                MatrixRecoveryExpectation::ForwardComplete => {
                    assert_eq!(report.forward_completed, 1, "{cut}");
                    assert_eq!(
                        fixture.published_target_count(),
                        4,
                        "{cut}: {:?}",
                        fixture.publish_target_debug()
                    );
                }
                MatrixRecoveryExpectation::AlreadyPublished => {
                    assert_eq!(report.already_published, 1, "{cut}");
                    assert_eq!(fixture.published_target_count(), 4, "{cut}");
                }
            }
            let conservation = recovery_service
                .conservation_scan()
                .expect("matrix conservation scan");
            assert!(
                conservation.anomalies.is_empty(),
                "conservation anomaly at {cut}: {:?}",
                conservation.anomalies
            );

            if matches!(
                expectation,
                MatrixRecoveryExpectation::ForwardComplete
                    | MatrixRecoveryExpectation::AlreadyPublished
            ) {
                let entry = recovery_service
                    .open_journal()
                    .expect("matrix journal")
                    .entries()
                    .into_iter()
                    .next()
                    .expect("matrix terminal entry");
                let mut retry = fixture.request.clone();
                match &mut retry {
                    ExternalMutationRequestV1::BrainPromote { request_id, .. } => {
                        *request_id = format!("retry-{cut}");
                    }
                    _ => unreachable!("matrix request is promote"),
                }
                let response = recovery_service
                    .execute(&fixture.context, retry, fixture.host.clone())
                    .expect("lost-response terminal replay");
                assert_eq!(response.request_id, format!("retry-{cut}"));
                assert_eq!(response.journal_operation_id, entry.operation_id);
                assert_eq!(response.outcome_digest, entry.outcome_digest.unwrap());
                assert_eq!(response.result["terminal_replay"], Value::Bool(true));
                assert_eq!(fixture.published_target_count(), 4, "retry changed targets");
            }
        }
    }

    #[test]
    fn promote_first_response_and_terminal_replay_share_the_exact_sealed_result() {
        let fixture = PromoteMatrixFixture::new();
        let service = fixture.service();
        let first = service
            .execute(
                &fixture.context,
                fixture.request.clone(),
                fixture.host.clone(),
            )
            .expect("first promotion response");
        assert!(!first.graph_resync_required);
        assert_eq!(first.reconciliation_state, "RECONCILED");
        assert!(first.result.get("terminal_replay").is_none());

        let retry = request_with_id(fixture.request.clone(), "sealed-result-retry".to_string());
        let replay = service
            .execute(&fixture.context, retry, fixture.host.clone())
            .expect("terminal replay");
        assert_eq!(replay.request_id, "sealed-result-retry");
        assert_eq!(replay.result["terminal_replay"], Value::Bool(true));

        let mut first_value = serde_json::to_value(first).expect("first response json");
        let mut replay_value = serde_json::to_value(replay).expect("replay response json");
        for value in [&mut first_value, &mut replay_value] {
            value
                .as_object_mut()
                .expect("response object")
                .remove("request_id");
            value
                .get_mut("result")
                .and_then(Value::as_object_mut)
                .expect("result object")
                .remove("terminal_replay");
        }
        assert_eq!(first_value, replay_value);
    }

    #[test]
    fn promote_refuses_foreign_reconciliation_actor_before_target_graph_or_checkpoint() {
        let fixture = PromoteMatrixFixture::new();
        let service = fixture.service();
        let mut host = fixture.host.clone();
        host.reconciliation_brain_id = "project-brain-foreign".to_string();
        let graph_generation_before = fixture.graph_generation();
        let checkpoint_current = host
            .promote_paths
            .as_ref()
            .expect("promote paths")
            .medulla_runtime_root
            .join(crate::brain_runtime::BRAIN_CHECKPOINT_DIRECTORY)
            .join("CURRENT");
        let checkpoint_current_before =
            std::fs::read(&checkpoint_current).expect("baseline promotion actor CURRENT");

        let error = service
            .execute(&fixture.context, fixture.request.clone(), host.clone())
            .expect_err("foreign actor identity must refuse");
        assert_eq!(error.code(), "external_mutation_inspect_actor_failed");
        assert!(
            error.to_string().contains("reconciliation actor mismatch"),
            "unexpected foreign-actor refusal: {error}"
        );
        assert_eq!(fixture.published_target_count(), 0);
        assert_eq!(fixture.graph_generation(), graph_generation_before);
        assert_eq!(
            std::fs::read(&checkpoint_current).expect("CURRENT after foreign-actor refusal"),
            checkpoint_current_before,
            "foreign-actor refusal must not publish a new checkpoint"
        );
    }

    #[test]
    fn promote_refuses_runtime_root_remap_before_target_graph_or_checkpoint() {
        let fixture = PromoteMatrixFixture::new();
        let service = fixture.service();
        let mut host = fixture.host.clone();
        let actual_runtime_root = host
            .promote_paths
            .as_ref()
            .expect("promote paths")
            .medulla_runtime_root
            .clone();
        host.promote_paths
            .as_mut()
            .expect("promote paths")
            .medulla_runtime_root = actual_runtime_root.join("foreign-remap");
        let graph_generation_before = fixture.graph_generation();
        let checkpoint_current = actual_runtime_root
            .join(crate::brain_runtime::BRAIN_CHECKPOINT_DIRECTORY)
            .join("CURRENT");
        let checkpoint_current_before =
            std::fs::read(&checkpoint_current).expect("baseline promotion actor CURRENT");

        let error = service
            .execute(&fixture.context, fixture.request.clone(), host.clone())
            .expect_err("runtime-root remap must refuse");
        assert_eq!(error.code(), "brain_promote_actor_precommit_refused");
        assert_eq!(fixture.published_target_count(), 0);
        assert_eq!(fixture.graph_generation(), graph_generation_before);
        assert_eq!(
            std::fs::read(&checkpoint_current).expect("CURRENT after runtime-root refusal"),
            checkpoint_current_before,
            "runtime-root refusal must not publish a new checkpoint"
        );
    }

    #[test]
    fn reconciled_phase_publishes_only_its_exact_sealed_result_without_append_on_refusal() {
        let fixture = PromoteMatrixFixture::new();
        let fired = Arc::new(AtomicBool::new(false));
        let crashing = fixture.service_crashing_at("after_graph_reconciled", Arc::clone(&fired));
        crashing
            .execute(
                &fixture.context,
                fixture.request.clone(),
                fixture.host.clone(),
            )
            .expect_err("stop after durable RECONCILED");
        assert!(fired.load(Ordering::SeqCst));

        let service = fixture.service();
        let mut journal = service.open_journal().expect("reopen journal");
        let entry = journal
            .entries()
            .into_iter()
            .next()
            .expect("reconciled entry");
        assert_eq!(entry.phase, ExternalMutationJournalPhaseV1::Reconciled);
        let exact_result = entry
            .published_result
            .clone()
            .expect("sealed reconciled result");
        let mut different_result = exact_result.clone();
        different_result.result["tampered"] = Value::Bool(true);
        let journal_path = fixture.journal_root.join("external-mutations.jsonl");
        let length_before = std::fs::metadata(&journal_path)
            .expect("journal metadata")
            .len();
        let refusal = journal
            .mark_published(
                &entry.operation_id,
                different_result,
                entry.updated_at.saturating_add(1),
            )
            .expect_err("different RECONCILED result must refuse");
        assert_eq!(
            refusal.code(),
            "external_mutation_published_result_mismatch"
        );
        assert_eq!(
            std::fs::metadata(&journal_path)
                .expect("journal metadata")
                .len(),
            length_before
        );
        drop(journal);
        let mut journal = service.open_journal().expect("replay after refusal");
        assert_eq!(
            journal.entry(&entry.operation_id).expect("entry").phase,
            ExternalMutationJournalPhaseV1::Reconciled
        );
        journal
            .mark_published(
                &entry.operation_id,
                exact_result,
                entry.updated_at.saturating_add(1),
            )
            .expect("exact sealed result publishes");
    }

    #[test]
    fn reconciliation_receipt_replay_refuses_equal_generation_foreign_ack_postimage_and_time() {
        let fixture = PromoteMatrixFixture::new();
        let service = fixture.service();
        service
            .execute(
                &fixture.context,
                fixture.request.clone(),
                fixture.host.clone(),
            )
            .expect("publish promotion");
        let entry = service
            .open_journal()
            .expect("journal")
            .entries()
            .into_iter()
            .next()
            .expect("published entry");

        let assert_invalid = |mutated_receipt: BrainPromoteReconciliationReceiptV1| {
            let mut mutated = entry.clone();
            mutated.reconciliation_receipt_digest = Some(
                digest_canonical(
                    crate::external_mutation_journal::BRAIN_PROMOTE_RECONCILIATION_RECEIPT_DIGEST_DOMAIN,
                    &mutated_receipt,
                )
                .expect("receipt digest"),
            );
            mutated.reconciliation_receipt = Some(mutated_receipt);
            assert!(crate::external_mutation_journal::validate_entry_for_test(&mutated).is_err());
        };

        let valid = entry
            .reconciliation_receipt
            .clone()
            .expect("reconciliation receipt");
        let mut equal_generation = valid.clone();
        equal_generation.graph_generation_after = equal_generation.graph_generation_before;
        assert_invalid(equal_generation);

        let mut foreign_ack = valid.clone();
        foreign_ack.checkpoint_ack.brain_id = "project-brain-foreign".to_string();
        foreign_ack.checkpoint_ack_digest = digest_canonical(
            BRAIN_PROMOTE_CHECKPOINT_ACK_DIGEST_DOMAIN,
            &foreign_ack.checkpoint_ack,
        )
        .expect("checkpoint ACK digest");
        assert_invalid(foreign_ack);

        let mut foreign_postimage = valid.clone();
        foreign_postimage.medulla_postimage_sha256 = matrix_hash("foreign-postimage");
        assert_invalid(foreign_postimage);

        let mut future_receipt = valid;
        future_receipt.reconciled_at = entry.updated_at.saturating_add(1);
        assert_invalid(future_receipt);
    }

    #[test]
    fn live_retry_forward_completes_commit_and_partial_publish_without_boot_recovery() {
        for (cut, expected_prefix) in [
            ("after_journal_committed", 0),
            ("after_promote_medulla_live", 2),
        ] {
            let fixture = PromoteMatrixFixture::new();
            let fired = Arc::new(AtomicBool::new(false));
            let crashing = fixture.service_crashing_at(cut, Arc::clone(&fired));
            crashing
                .execute(
                    &fixture.context,
                    fixture.request.clone(),
                    fixture.host.clone(),
                )
                .expect_err(cut);
            assert!(fired.load(Ordering::SeqCst), "cut was not reached: {cut}");
            assert_eq!(fixture.published_target_count(), expected_prefix, "{cut}");

            let service = fixture.service();
            let retry = request_with_id(fixture.request.clone(), format!("live-retry-{cut}"));
            let response = service
                .execute(&fixture.context, retry, fixture.host.clone())
                .expect("same signed operation must heal inline without a boot coordinator");
            assert_eq!(response.request_id, format!("live-retry-{cut}"));
            assert_eq!(response.result["terminal_replay"], Value::Bool(true));
            assert_eq!(fixture.published_target_count(), 4, "{cut}");

            let entry = service
                .open_journal()
                .expect("journal")
                .entries()
                .into_iter()
                .next()
                .expect("operation");
            assert_eq!(entry.phase, ExternalMutationJournalPhaseV1::Published);
            let lease = service
                .open_broker()
                .expect("broker")
                .lease(MATRIX_LEASE_ID)
                .cloned()
                .expect("lease");
            assert_eq!(lease.state, AuthorizationLeaseStateV1::Consumed);
            assert!(
                entry_lease_conservation_anomalies(&entry, &lease).is_empty(),
                "inline recovery broke conservation at {cut}"
            );
        }
    }

    #[test]
    fn live_prepared_absence_aborts_reserve_and_stage_orphans_then_fresh_lease_succeeds() {
        for cut in ["after_reserve", "after_stage"] {
            let fixture = PromoteMatrixFixture::new();
            let fired = Arc::new(AtomicBool::new(false));
            let crashing = fixture.service_crashing_at(cut, Arc::clone(&fired));
            crashing
                .execute(
                    &fixture.context,
                    fixture.request.clone(),
                    fixture.host.clone(),
                )
                .expect_err(cut);
            assert!(fired.load(Ordering::SeqCst), "cut was not reached: {cut}");
            assert_eq!(fixture.clock.load(Ordering::SeqCst), MATRIX_NOW_MS);
            assert_eq!(fixture.published_target_count(), 0);

            let service = fixture.service();
            let orphan_retry =
                request_with_id(fixture.request.clone(), format!("orphan-same-lease-{cut}"));
            let error = service
                .execute(&fixture.context, orphan_retry, fixture.host.clone())
                .expect_err("orphan reservation must become terminally aborted inline");
            assert_eq!(
                error.code(),
                "external_mutation_orphan_reservation_aborted_reauthorization_required"
            );
            let old_lease = service
                .open_broker()
                .expect("broker")
                .lease(MATRIX_LEASE_ID)
                .cloned()
                .expect("old lease");
            assert_eq!(old_lease.state, AuthorizationLeaseStateV1::Aborted);

            let fresh_lease_id = format!("fresh-{cut}");
            {
                let mut broker = service.open_broker().expect("broker");
                broker
                    .issue(
                        fresh_lease_id.clone(),
                        old_lease.authorization_receipt.clone(),
                        MATRIX_NOW_MS,
                    )
                    .expect("fresh one-shot lease");
            }
            let mut fresh_context = fixture.context.clone();
            fresh_context.authority_lease_id = Some(fresh_lease_id);
            let fresh_request =
                request_with_id(fixture.request.clone(), format!("fresh-request-{cut}"));
            let response = service
                .execute(&fresh_context, fresh_request, fixture.host.clone())
                .expect("fresh lease for the identical operation must proceed");
            assert_eq!(response.semantic_action, "brain.promote");
            assert_eq!(fixture.published_target_count(), 4);
            assert!(service
                .conservation_scan()
                .expect("conservation")
                .anomalies
                .is_empty());
        }
    }

    #[test]
    fn published_retry_uses_sealed_result_and_never_touches_newer_target_state() {
        let fixture = PromoteMatrixFixture::new();
        let service = fixture.service();
        let first = service
            .execute(
                &fixture.context,
                fixture.request.clone(),
                fixture.host.clone(),
            )
            .expect("publish operation A");
        let newer_medulla = b"operation B medulla state\n";
        let newer_source = b"operation B source state\n";
        std::fs::write(&fixture.publish_targets[1].0, newer_medulla)
            .expect("publish newer medulla state B");
        std::fs::write(&fixture.publish_targets[3].0, newer_source)
            .expect("publish newer source state B");

        let retry = request_with_id(fixture.request.clone(), "retry-operation-a-after-b");
        let replay = service
            .execute(&fixture.context, retry, fixture.host.clone())
            .expect("A replay is served from its sealed journal result");
        assert_eq!(replay.request_id, "retry-operation-a-after-b");
        assert_eq!(replay.journal_operation_id, first.journal_operation_id);
        assert_eq!(replay.outcome_digest, first.outcome_digest);
        let mut replay_payload = replay.result.clone();
        assert_eq!(replay_payload["terminal_replay"], Value::Bool(true));
        replay_payload
            .as_object_mut()
            .expect("object response")
            .remove("terminal_replay");
        assert_eq!(replay_payload, first.result);
        assert_eq!(
            std::fs::read(&fixture.publish_targets[1].0).expect("medulla B"),
            newer_medulla
        );
        assert_eq!(
            std::fs::read(&fixture.publish_targets[3].0).expect("source B"),
            newer_source
        );
    }

    #[test]
    fn live_retry_immediately_aborts_exact_prepared_and_fences_reentry() {
        let fixture = PromoteMatrixFixture::new();
        let fired = Arc::new(AtomicBool::new(false));
        let crashing =
            fixture.service_crashing_at("after_broker_finalization_prepared", Arc::clone(&fired));
        crashing
            .execute(
                &fixture.context,
                fixture.request.clone(),
                fixture.host.clone(),
            )
            .expect_err("simulated death before outer COMMIT");
        assert!(fired.load(Ordering::SeqCst));
        assert_eq!(fixture.clock.load(Ordering::SeqCst), MATRIX_NOW_MS);
        assert_eq!(fixture.published_target_count(), 0);

        let service = fixture.service();
        let first_retry = request_with_id(fixture.request.clone(), "prepared-abort-retry-1");
        let first_error = service
            .execute(&fixture.context, first_retry, fixture.host.clone())
            .expect_err("protected exact PREPARED must abort immediately");
        assert_eq!(first_error.code(), "external_mutation_prepared_aborted");
        assert_eq!(fixture.clock.load(Ordering::SeqCst), MATRIX_NOW_MS);
        assert_eq!(fixture.published_target_count(), 0);

        let entry = service
            .open_journal()
            .expect("journal")
            .entries()
            .into_iter()
            .next()
            .expect("prepared entry");
        assert_eq!(entry.phase, ExternalMutationJournalPhaseV1::Prepared);
        let lease = service
            .open_broker()
            .expect("broker")
            .lease(MATRIX_LEASE_ID)
            .cloned()
            .expect("lease");
        assert_eq!(lease.state, AuthorizationLeaseStateV1::Aborted);
        assert!(entry_lease_conservation_anomalies(&entry, &lease).is_empty());

        let second_retry = request_with_id(fixture.request.clone(), "prepared-abort-retry-2");
        let second_error = service
            .execute(&fixture.context, second_retry, fixture.host.clone())
            .expect_err("aborted one-shot lease must fence every later retry");
        assert_eq!(second_error.code(), "external_mutation_prepared_aborted");
        assert_eq!(fixture.published_target_count(), 0);
    }

    #[test]
    fn active_callback_commit_and_independent_recovery_are_serialized_without_abba() {
        let fixture = PromoteMatrixFixture::new();
        let fired = Arc::new(AtomicBool::new(false));
        let callback_fired = Arc::clone(&fired);
        let (callback_entered_tx, callback_entered_rx) = mpsc::sync_channel(1);
        let (callback_release_tx, callback_release_rx) = mpsc::sync_channel(1);
        let callback_release_rx = Arc::new(Mutex::new(callback_release_rx));
        let crashing = fixture
            .service()
            .with_fault_hook_for_test(Arc::new(move |point| {
                if point == "after_broker_finalization_prepared"
                    && !callback_fired.swap(true, Ordering::SeqCst)
                {
                    callback_entered_tx
                        .send(())
                        .map_err(|error| error.to_string())?;
                    callback_release_rx
                        .lock()
                        .recv()
                        .map_err(|error| error.to_string())?;
                }
                Ok(())
            }));

        let execution_context = fixture.context.clone();
        let execution_request = fixture.request.clone();
        let execution_host = fixture.host.clone();
        let (execution_done_tx, execution_done_rx) = mpsc::sync_channel(1);
        let execution_thread = std::thread::spawn(move || {
            let result = crashing.execute(&execution_context, execution_request, execution_host);
            execution_done_tx
                .send(result)
                .expect("execution result receiver");
        });
        callback_entered_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("callback reached FINALIZATION_PREPARED");

        // Deliberately use a different process-local operation mutex and
        // linearization object. Only the durable broker writer lock is shared.
        let clock = Arc::clone(&fixture.clock);
        let recovery_service = fixture.service_with_runtime(
            OwnerAuthorityLinearizationV1::default(),
            Arc::new(Mutex::new(())),
            Arc::clone(&fixture.current_authority),
            Arc::new(move || clock.load(Ordering::SeqCst)),
        );
        let recovery_brain_id = fixture.host.selected_actor_brain_id.clone();
        let recovery_brain = Arc::clone(&fixture.host.selected_brain);
        let recovery_reconcile_promote = Arc::clone(&fixture.host.reconcile_promote);
        let (recovery_started_tx, recovery_started_rx) = mpsc::sync_channel(1);
        let (recovery_done_tx, recovery_done_rx) = mpsc::sync_channel(1);
        let recovery_thread = std::thread::spawn(move || {
            recovery_started_tx
                .send(())
                .expect("recovery start receiver");
            let result = recovery_service.recover_pending(
                move |requested| {
                    if requested == recovery_brain_id {
                        Ok(Arc::clone(&recovery_brain))
                    } else {
                        Err(format!("unexpected recovery brain {requested}"))
                    }
                },
                recovery_reconcile_promote,
            );
            recovery_done_tx
                .send(result)
                .expect("recovery result receiver");
        });
        recovery_started_rx.recv().expect("recovery started");
        assert!(matches!(
            recovery_done_rx.recv_timeout(Duration::from_millis(150)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));

        callback_release_tx.send(()).expect("release callback");
        let execution_response = execution_done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("execution must not deadlock")
            .expect("callback COMMIT wins");
        let recovery_report = recovery_done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("recovery must not deadlock")
            .expect("recovery converges after callback COMMIT");
        execution_thread.join().expect("execution thread");
        recovery_thread.join().expect("recovery thread");

        assert!(fired.load(Ordering::SeqCst));
        assert_eq!(execution_response.semantic_action, "brain.promote");
        assert_eq!(recovery_report.safely_aborted_pre_finalization, 0);
        assert_eq!(
            recovery_report.forward_completed + recovery_report.already_published,
            1
        );
        assert_eq!(fixture.published_target_count(), 4);
        let service = fixture.service();
        let entry = service
            .open_journal()
            .expect("journal")
            .entries()
            .into_iter()
            .next()
            .expect("operation");
        let lease = service
            .open_broker()
            .expect("broker")
            .lease(MATRIX_LEASE_ID)
            .cloned()
            .expect("lease");
        assert_eq!(entry.phase, ExternalMutationJournalPhaseV1::Published);
        assert_eq!(lease.state, AuthorizationLeaseStateV1::Consumed);
        assert!(entry_lease_conservation_anomalies(&entry, &lease).is_empty());
    }

    #[test]
    fn admission_clock_is_sampled_only_after_cross_process_broker_lock() {
        let fixture = PromoteMatrixFixture::new();
        let blocker = OwnerAuthorizationBrokerV1::open_with_protected_head(
            fixture.broker_config.clone(),
            OwnerAuthorityLinearizationV1::default(),
            Arc::clone(&fixture.protected_journal_head),
        )
        .expect("hold broker writer lock");
        let clock_value = Arc::new(AtomicU64::new(MATRIX_NOW_MS));
        let clock_calls = Arc::new(AtomicU64::new(0));
        let sampled_value = Arc::clone(&clock_value);
        let sampled_calls = Arc::clone(&clock_calls);
        let service = fixture.service_with_runtime(
            OwnerAuthorityLinearizationV1::default(),
            Arc::new(Mutex::new(())),
            Arc::clone(&fixture.current_authority),
            Arc::new(move || {
                sampled_calls.fetch_add(1, Ordering::SeqCst);
                sampled_value.load(Ordering::SeqCst)
            }),
        );
        let context = fixture.context.clone();
        let request = fixture.request.clone();
        let host = fixture.host.clone();
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let execution_thread = std::thread::spawn(move || {
            started_tx.send(()).expect("started receiver");
            done_tx
                .send(service.execute(&context, request, host))
                .expect("result receiver");
        });
        started_rx.recv().expect("execution started");
        assert!(matches!(
            done_rx.recv_timeout(Duration::from_millis(150)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        assert_eq!(clock_calls.load(Ordering::SeqCst), 0);

        clock_value.store(MATRIX_NOW_MS + 10_000, Ordering::SeqCst);
        drop(blocker);
        let error = done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("execution resumes after broker unlock")
            .expect_err("the newly sampled time is expired");
        execution_thread.join().expect("execution thread");
        assert_eq!(error.code(), "authorization_reservation_binding_mismatch");
        assert_eq!(clock_calls.load(Ordering::SeqCst), 1);
        assert_eq!(fixture.published_target_count(), 0);
        let lease = fixture
            .service()
            .open_broker()
            .expect("broker")
            .lease(MATRIX_LEASE_ID)
            .cloned()
            .expect("lease");
        assert_eq!(lease.state, AuthorizationLeaseStateV1::Unused);
    }

    #[test]
    fn finalization_clock_is_sampled_after_blocking_current_authority_read() {
        let fixture = PromoteMatrixFixture::new();
        let status = (fixture.current_authority)().expect("fixture authority");
        let (status_entered_tx, status_entered_rx) = mpsc::sync_channel(1);
        let (status_release_tx, status_release_rx) = mpsc::sync_channel(1);
        let status_release_rx = Arc::new(Mutex::new(status_release_rx));
        let blocking_status: Arc<AuthorityStatusReader> = Arc::new(move || {
            status_entered_tx
                .send(())
                .map_err(|error| error.to_string())?;
            status_release_rx
                .lock()
                .recv()
                .map_err(|error| error.to_string())?;
            Ok(status.clone())
        });
        let clock_value = Arc::new(AtomicU64::new(MATRIX_NOW_MS));
        let clock_calls = Arc::new(AtomicU64::new(0));
        let sampled_value = Arc::clone(&clock_value);
        let sampled_calls = Arc::clone(&clock_calls);
        let service = fixture.service_with_runtime(
            OwnerAuthorityLinearizationV1::default(),
            Arc::new(Mutex::new(())),
            blocking_status,
            Arc::new(move || {
                sampled_calls.fetch_add(1, Ordering::SeqCst);
                sampled_value.load(Ordering::SeqCst)
            }),
        );
        let context = fixture.context.clone();
        let request = fixture.request.clone();
        let host = fixture.host.clone();
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let execution_thread = std::thread::spawn(move || {
            done_tx
                .send(service.execute(&context, request, host))
                .expect("result receiver");
        });
        status_entered_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("final authority read reached");
        assert_eq!(clock_calls.load(Ordering::SeqCst), 1);

        clock_value.store(MATRIX_NOW_MS + 10_000, Ordering::SeqCst);
        status_release_tx.send(()).expect("release status reader");
        let error = done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("finalization must not deadlock")
            .expect_err("post-status clock sample must observe expiry");
        execution_thread.join().expect("execution thread");
        assert_eq!(
            error.code(),
            "authorization_state_changed_before_finalization"
        );
        assert_eq!(clock_calls.load(Ordering::SeqCst), 2);
        assert_eq!(fixture.published_target_count(), 0);
        let service = fixture.service();
        let entry = service
            .open_journal()
            .expect("journal")
            .entries()
            .into_iter()
            .next()
            .expect("prepared operation");
        let lease = service
            .open_broker()
            .expect("broker")
            .lease(MATRIX_LEASE_ID)
            .cloned()
            .expect("lease");
        assert_eq!(entry.phase, ExternalMutationJournalPhaseV1::Prepared);
        assert_eq!(lease.state, AuthorizationLeaseStateV1::Reserved);
        assert!(entry_lease_conservation_anomalies(&entry, &lease).is_empty());
    }

    #[test]
    fn conservation_state_table_rejects_prepared_consumed_and_terminal_witness_drift() {
        let fixture = PromoteMatrixFixture::new();
        let service = fixture.service();
        service
            .execute(
                &fixture.context,
                fixture.request.clone(),
                fixture.host.clone(),
            )
            .expect("normal promote terminal");
        let entry = service
            .open_journal()
            .expect("journal")
            .entries()
            .into_iter()
            .next()
            .expect("published entry");
        let lease = service
            .open_broker()
            .expect("broker")
            .lease(MATRIX_LEASE_ID)
            .cloned()
            .expect("consumed lease");
        assert!(
            entry_lease_conservation_anomalies(&entry, &lease).is_empty(),
            "real published terminal must satisfy the exact state table"
        );

        let mut forged_prepared = entry.clone();
        forged_prepared.phase = ExternalMutationJournalPhaseV1::Prepared;
        forged_prepared.commit_record_digest = None;
        forged_prepared.outcome_digest = None;
        forged_prepared.committed_at = None;
        let prepared_anomalies = entry_lease_conservation_anomalies(&forged_prepared, &lease);
        assert!(prepared_anomalies
            .iter()
            .any(|value| value.ends_with(":prepared_with_consumed_lease")));

        let mut forged_witness = lease.clone();
        forged_witness
            .terminal
            .as_mut()
            .and_then(|terminal| terminal.external_mutation_witness.as_mut())
            .expect("external terminal witness")
            .reservation_id = "forged-reservation".to_string();
        let witness_anomalies = entry_lease_conservation_anomalies(&entry, &forged_witness);
        assert!(witness_anomalies
            .iter()
            .any(|value| value.ends_with(":consumed_terminal_mismatch")));
    }

    #[test]
    fn promote_refuses_a_lease_for_b_source_before_path_resolution_or_mutation() {
        let fixture = PromoteMatrixFixture::new();
        let foreign_root = fixture._temp.path().join("foreign-project-b");
        let foreign_store = foreign_root.join("agent-memory");
        std::fs::create_dir_all(&foreign_store).expect("foreign store");
        let foreign_target = foreign_store.join("crash-node.light.md");
        let foreign_before = b"foreign-brain-sentinel";
        std::fs::write(&foreign_target, foreign_before).expect("foreign sentinel");
        let mut request = fixture.request.clone();
        match &mut request {
            ExternalMutationRequestV1::BrainPromote {
                source_brain,
                expected_source_sha256,
                ..
            } => {
                *source_brain = foreign_root.to_string_lossy().to_string();
                *expected_source_sha256 = sha256_bytes(foreign_before);
            }
            _ => unreachable!("fixture request is promote"),
        }
        let mut host = fixture.host.clone();
        host.promote_paths = Some(ExternalPromotePathsV1 {
            source_store_dir: foreign_store,
            medulla_store_dir: fixture.publish_targets[1]
                .0
                .parent()
                .expect("medulla store")
                .to_path_buf(),
            medulla_runtime_root: fixture._temp.path().join("medulla"),
        });
        let service = fixture.service();
        let error = service
            .execute(&fixture.context, request, host)
            .expect_err("A receipt cannot target B source");
        assert_eq!(error.code(), "brain_promote_source_binding_mismatch");
        assert_eq!(
            std::fs::read(&foreign_target).expect("foreign remains"),
            foreign_before
        );
        assert!(
            !fixture.journal_root.exists(),
            "binding refusal must precede journal creation"
        );
        let lease = service
            .open_broker()
            .expect("broker")
            .lease(MATRIX_LEASE_ID)
            .cloned()
            .expect("lease");
        assert_eq!(lease.state, AuthorizationLeaseStateV1::Unused);
        assert!(lease.reservation.is_none());
    }

    #[test]
    fn promote_partial_publish_forward_completes_idempotently() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source_store = temp.path().join("source").join("agent-memory");
        let medulla_runtime = temp.path().join("medulla");
        let medulla_store = medulla_runtime.join("agent-memory");
        std::fs::create_dir_all(source_store.join(".history")).expect("source dirs");
        std::fs::create_dir_all(medulla_store.join(".history")).expect("medulla dirs");

        let source_target = source_store.join("claim.light.md");
        let medulla_target = medulla_store.join("claim.light.md");
        let source_before = b"---\nState: verified\n---\nsource before\n";
        let source_after =
            b"---\nState: verified\nPromoted-To: medulla@claim@77\n---\nsource after\n";
        let medulla_after = b"---\nState: verified\n---\nmedulla after\n";
        let source_history_after = b"---\nState: outdated\n---\nsource before\n";
        std::fs::write(&source_target, source_before).expect("source before");

        let source_stage = source_store.join(".source.stage");
        let medulla_stage = medulla_store.join(".medulla.stage");
        let source_history_target = source_store.join(".history/claim.77.light.md");
        let source_history_stage = source_store.join(".history/.source-history.stage");
        write_staging_file(&source_stage, source_after).expect("source stage");
        write_staging_file(&medulla_stage, medulla_after).expect("medulla stage");
        write_staging_file(&source_history_stage, source_history_after)
            .expect("source history stage");

        let staged = StagedPromoteV1 {
            input: crate::promote_handlers::PromoteInput {
                agent_id: "owner:test".to_string(),
                brain: "/project".to_string(),
                claim: "claim".to_string(),
                reason: "shared invariant".to_string(),
            },
            paths: ExternalPromotePathsV1 {
                source_store_dir: source_store.clone(),
                medulla_store_dir: medulla_store.clone(),
                medulla_runtime_root: medulla_runtime,
            },
            source: StagedFileV1 {
                target_path: source_target.clone(),
                staging_path: source_stage,
                expected_before_sha256: Some(sha256_bytes(source_before)),
                after_sha256: sha256_bytes(source_after),
            },
            medulla: StagedFileV1 {
                target_path: medulla_target.clone(),
                staging_path: medulla_stage,
                expected_before_sha256: None,
                after_sha256: sha256_bytes(medulla_after),
            },
            source_history: StagedFileV1 {
                target_path: source_history_target.clone(),
                staging_path: source_history_stage,
                expected_before_sha256: None,
                after_sha256: sha256_bytes(source_history_after),
            },
            medulla_history: None,
            source_slug: "claim".to_string(),
            medulla_slug: "claim".to_string(),
            origin_brain: "/project".to_string(),
            origin_qualified: false,
            evidence_unverifiable: false,
            promoted_at_ms: 77,
            reconciliation_brain_id: "project-brain-test".to_string(),
            operation_object_digest: sha256_bytes(b"operation"),
            outcome_digest: sha256_bytes(b"outcome"),
        };

        // Simulate process death after the medulla target landed but before the
        // project witness/history did.
        {
            let locks = crate::promote_handlers::acquire_promote_target_locks(
                &source_store,
                "claim",
                &medulla_store,
                "claim",
            )
            .expect("initial locks");
            staged.medulla.forward_publish().expect("medulla landed");
            drop(locks);
        }
        assert_eq!(std::fs::read(&medulla_target).unwrap(), medulla_after);
        assert_eq!(std::fs::read(&source_target).unwrap(), source_before);

        for _ in 0..2 {
            let locks = crate::promote_handlers::acquire_promote_target_locks(
                &source_store,
                "claim",
                &medulla_store,
                "claim",
            )
            .expect("recovery locks");
            staged
                .publish(&locks, &no_external_mutation_fault)
                .expect("forward complete");
        }
        assert_eq!(std::fs::read(&medulla_target).unwrap(), medulla_after);
        assert_eq!(std::fs::read(&source_target).unwrap(), source_after);
        assert_eq!(
            std::fs::read(&source_history_target).unwrap(),
            source_history_after
        );
    }

    const A2_MATRIX_SUBJECT: &str = "agent:a2-matrix";
    const A2_MATRIX_BEFORE: &str = "pub fn a2_before() -> u8 { 1 }\n";
    const A2_MATRIX_AFTER: &str = "pub fn a2_after() -> u8 { 2 }\n";

    struct A2MatrixFixture {
        _temp: TempDir,
        repo_root: PathBuf,
        target_path: PathBuf,
        journal_root: PathBuf,
        broker_config: OwnerAuthorizationBrokerConfigV1,
        linearization: OwnerAuthorityLinearizationV1,
        broker_operation: Arc<Mutex<()>>,
        current_authority: Arc<AuthorityStatusReader>,
        current_status: Arc<Mutex<AuthorityRuntimeStatusV1>>,
        protected_journal_head: SharedProtectedJournalHeadBackendV1,
        receipt_crypto: Arc<dyn AuthorityWalRecordCrypto>,
        clock: Arc<AtomicU64>,
        brain_id: String,
        host: ExternalMutationExecutionHostV1,
        actor_registry: Arc<crate::project_brains::ProjectBrainRegistry>,
        reconciliation_brain_id: String,
        actor_calls: Arc<AtomicU64>,
        initial_request: ExternalMutationRequestV1,
        initial_context: MissionServiceTransportContextV1,
    }

    impl A2MatrixFixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().expect("A2 matrix tempdir");
            let repo_root = temp.path().join("repo");
            let runtime_root = temp.path().join("runtime");
            let target_path = repo_root.join("src/lib.rs");
            std::fs::create_dir_all(target_path.parent().expect("A2 source parent"))
                .expect("A2 source tree");
            std::fs::create_dir_all(&runtime_root).expect("A2 runtime");
            std::fs::write(&target_path, A2_MATRIX_BEFORE).expect("A2 source preimage");
            let repo_root = repo_root.canonicalize().expect("canonical A2 root");
            let root_identity = repo_root.to_string_lossy().into_owned();

            let config = McpConfig {
                graph_source: runtime_root.join("graph.json"),
                plasticity_state: runtime_root.join("plasticity.json"),
                runtime_dir: Some(runtime_root),
                ..McpConfig::default()
            };
            let mut graph = Graph::new();
            graph
                .finalize()
                .expect("A2 matrix graph must start with valid empty CSR storage");
            let mut state = SessionState::initialize(graph, &config, DomainConfig::code())
                .expect("A2 matrix session");
            state.ingest_roots = vec![root_identity.clone()];
            state.workspace_root = Some(root_identity.clone());
            let selected_brain = Arc::new(BrainSessionCell::new(state));
            let actor_registry = Arc::new(crate::project_brains::ProjectBrainRegistry::new(
                temp.path().join("A2-project-brains"),
                None,
            ));
            let reconciliation_brain_id = actor_registry
                .bound_brain_id_for_target(Arc::clone(&selected_brain))
                .expect("A2 bound actor id");
            let brain_id = reconciliation_brain_id.clone();
            let actor_calls = Arc::new(AtomicU64::new(0));
            let actor_calls_for_host = Arc::clone(&actor_calls);
            let actor_brain = Arc::clone(&selected_brain);
            let runtime_jobs = actor_registry
                .runtime_job_registry()
                .expect("A2 runtime jobs");
            let registry_for_host = Arc::clone(&actor_registry);
            let recovery_brain = Arc::clone(&selected_brain);
            let recovery_brain_id = reconciliation_brain_id.clone();
            let host = ExternalMutationExecutionHostV1 {
                selected_brain,
                selected_actor_brain_id: reconciliation_brain_id.clone(),
                resolve_brain: Arc::new(move |requested| {
                    if requested == recovery_brain_id {
                        Ok(Arc::clone(&recovery_brain))
                    } else {
                        Err(format!("unexpected A2 recovery brain '{requested}'"))
                    }
                }),
                reconcile_promote: Arc::new(move |request| {
                    let requires_checkpoint_ack = request.requires_checkpoint_ack();
                    if requires_checkpoint_ack {
                        actor_calls_for_host.fetch_add(1, Ordering::SeqCst);
                    }
                    let allows_resolved_actor_identity = request.allows_resolved_actor_identity();
                    let actual_brain_id = registry_for_host
                        .bound_brain_id_for_target(Arc::clone(&actor_brain))
                        .map_err(|error| error.to_string())?;
                    if actual_brain_id != request.reconciliation_brain_id
                        && !allows_resolved_actor_identity
                    {
                        return Err(format!(
                            "A2 actor mismatch: expected '{}', observed '{}'",
                            request.reconciliation_brain_id, actual_brain_id
                        ));
                    }
                    if requires_checkpoint_ack {
                        registry_for_host
                            .execute_target_runtime_with_checkpoint_ack(
                                Arc::clone(&actor_brain),
                                None,
                                true,
                                move |state| {
                                    request.execute(state).map_err(|detail| {
                                        crate::runtime_jobs::RuntimeJobFailure::new(
                                            "graph_ingest_a2_actor_job_failed",
                                            detail,
                                        )
                                    })
                                },
                            )
                            .map(|(execution, ack)| execution.bind_checkpoint_ack(&ack))
                            .map_err(|error| error.to_string())
                    } else {
                        registry_for_host
                            .execute_target_runtime(
                                Arc::clone(&actor_brain),
                                None,
                                true,
                                false,
                                move |state| {
                                    request.execute(state).map_err(|detail| {
                                        crate::runtime_jobs::RuntimeJobFailure::new(
                                            "external_mutation_inspect_actor_job_failed",
                                            detail,
                                        )
                                    })
                                },
                            )
                            .map_err(|error| error.to_string())
                    }
                }),
                reconciliation_brain_id: reconciliation_brain_id.clone(),
                promote_paths: None,
                runtime_jobs: Ok(runtime_jobs),
            };
            let mut initial_request =
                Self::replace_request_for(&actor_registry, &host, &repo_root, "A2-replace");
            let semantic_payload_digest = inspect_request(
                &initial_request,
                &host,
                A2_MATRIX_SUBJECT,
                MATRIX_NOW_MS,
                &[],
                &brain_id,
            )
            .expect("A2 initial inspection")
            .semantic_payload_digest()
            .to_string();
            let operation_object_digest =
                a2_operation_object_digest(&initial_request, &brain_id, &semantic_payload_digest);
            let receipt_crypto: Arc<dyn AuthorityWalRecordCrypto> = Arc::new(
                SoftwareTestAuthorityWalRecordCrypto::explicit_not_production(
                    b"external-A2-matrix-receipt",
                ),
            );
            let (initial_receipt, initial_context) = a2_authority_receipt(
                &initial_request,
                &brain_id,
                &root_identity,
                operation_object_digest.clone(),
                "A2-replace",
                MATRIX_NOW_MS,
                receipt_crypto.as_ref(),
            );
            bind_a2_graph_preview_to_context(
                &mut initial_request,
                &initial_context,
                &brain_id,
                &operation_object_digest,
            );
            let initial_status = matrix_status(&initial_receipt);
            let current_status = Arc::new(Mutex::new(initial_status));
            let status_for_reader = Arc::clone(&current_status);
            let current_authority: Arc<AuthorityStatusReader> =
                Arc::new(move || Ok(status_for_reader.lock().clone()));
            let protected_journal_head = SoftwareTestProtectedJournalHeadBackendV1::new().shared();
            let broker_config = OwnerAuthorizationBrokerConfigV1 {
                root: temp.path().join("A2-broker"),
                reservation_ttl_ms: 1_000,
                minimum_terminal_retention_ms: 1_000,
            };
            let linearization = OwnerAuthorityLinearizationV1::default();
            {
                let mut broker = OwnerAuthorizationBrokerV1::open_with_protected_head(
                    broker_config.clone(),
                    linearization.clone(),
                    Arc::clone(&protected_journal_head),
                )
                .expect("A2 broker");
                broker
                    .issue(
                        initial_context
                            .authority_lease_id
                            .as_deref()
                            .expect("A2 initial lease"),
                        initial_receipt,
                        MATRIX_NOW_MS,
                    )
                    .expect("A2 initial authority");
            }
            Self {
                journal_root: temp.path().join("A2-external-journal"),
                broker_config,
                linearization,
                broker_operation: Arc::new(Mutex::new(())),
                current_authority,
                current_status,
                protected_journal_head,
                receipt_crypto,
                clock: Arc::new(AtomicU64::new(MATRIX_NOW_MS)),
                brain_id,
                host,
                actor_registry,
                reconciliation_brain_id,
                actor_calls,
                initial_request,
                initial_context,
                repo_root,
                target_path,
                _temp: temp,
            }
        }

        fn service(&self) -> ExternalMutationServiceV1 {
            let clock = Arc::clone(&self.clock);
            ExternalMutationServiceV1::from_owner_inputs(ExternalMutationServiceInputsV1 {
                journal_root: self.journal_root.clone(),
                broker_config: self.broker_config.clone(),
                linearization: self.linearization.clone(),
                broker_operation: Arc::clone(&self.broker_operation),
                current_authority: Arc::clone(&self.current_authority),
                protected_journal_head: Arc::clone(&self.protected_journal_head),
                receipt_crypto: Arc::clone(&self.receipt_crypto),
                owner_clock: Arc::new(move || clock.load(Ordering::SeqCst)),
            })
        }

        fn service_crashing_at(&self, cut: &'static str) -> ExternalMutationServiceV1 {
            self.service()
                .with_fault_hook_for_test(Arc::new(move |point| {
                    if point == cut {
                        Err("simulated A2 process death".to_string())
                    } else {
                        Ok(())
                    }
                }))
        }

        fn preview_replace(
            &self,
            label: &str,
        ) -> (
            MissionServiceTransportContextV1,
            GraphIngestPreviewResponseV1,
        ) {
            let context =
                a2_preview_context(&self.brain_id, &self.repo_root.to_string_lossy(), label);
            let response = self
                .service()
                .preview_graph_ingest(
                    &context,
                    GraphIngestPreviewRequestV1 {
                        schema: GRAPH_INGEST_PREVIEW_REQUEST_SCHEMA.to_string(),
                        request_id: label.to_string(),
                        mode: GraphIngestA2ModeV1::Replace,
                        include_dotfiles: false,
                        dotfile_patterns: Vec::new(),
                        parent: None,
                    },
                    self.host.clone(),
                )
                .expect("A2 owner-derived replace preview");
            (context, response)
        }

        fn graph_preimage(&self) -> (u64, String) {
            Self::graph_preimage_for(&self.actor_registry, &self.host)
        }

        fn graph_preimage_for(
            actor_registry: &crate::project_brains::ProjectBrainRegistry,
            host: &ExternalMutationExecutionHostV1,
        ) -> (u64, String) {
            actor_registry
                .read_target_runtime_snapshot(
                    Arc::clone(&host.selected_brain),
                    None,
                    true,
                    |state| {
                        let digest =
                            m1nd_ingest::ownership::source_projection_digest(&state.graph.read())
                                .map_err(|error| {
                                crate::runtime_jobs::RuntimeJobFailure::new(
                                    "A2_graph_projection_failed",
                                    error.to_string(),
                                )
                            })?;
                        Ok((state.graph_generation, digest))
                    },
                )
                .expect("A2 actor graph snapshot")
                .value
        }

        fn replace_request_for(
            actor_registry: &crate::project_brains::ProjectBrainRegistry,
            host: &ExternalMutationExecutionHostV1,
            root: &Path,
            request_id: &str,
        ) -> ExternalMutationRequestV1 {
            let (expected_graph_generation, expected_source_projection_digest) =
                Self::graph_preimage_for(actor_registry, host);
            ExternalMutationRequestV1::GraphIngestReplace {
                schema: EXTERNAL_MUTATION_REQUEST_SCHEMA.to_string(),
                request_id: request_id.to_string(),
                request: GraphIngestA2InputV1 {
                    preview_id: "f".repeat(64),
                    root: root.to_string_lossy().into_owned(),
                    expected_graph_generation,
                    expected_source_projection_digest,
                    include_dotfiles: false,
                    dotfile_patterns: Vec::new(),
                    parent: None,
                },
            }
        }

        fn merge_request(
            &self,
            parent: GraphIngestA2ParentV1,
            request_id: &str,
        ) -> ExternalMutationRequestV1 {
            let (expected_graph_generation, expected_source_projection_digest) =
                self.graph_preimage();
            ExternalMutationRequestV1::GraphIngestMergeExisting {
                schema: EXTERNAL_MUTATION_REQUEST_SCHEMA.to_string(),
                request_id: request_id.to_string(),
                request: GraphIngestA2InputV1 {
                    preview_id: "f".repeat(64),
                    root: self.repo_root.to_string_lossy().into_owned(),
                    expected_graph_generation,
                    expected_source_projection_digest,
                    include_dotfiles: false,
                    dotfile_patterns: Vec::new(),
                    parent: Some(parent),
                },
            }
        }

        fn inspect_error(
            &self,
            request: &ExternalMutationRequestV1,
            entries: &[ExternalMutationJournalEntryV1],
        ) -> &'static str {
            match inspect_request(
                request,
                &self.host,
                A2_MATRIX_SUBJECT,
                self.clock.load(Ordering::SeqCst),
                entries,
                &self.brain_id,
            ) {
                Err(error) => error.code(),
                Ok(_) => panic!("A2 request must refuse"),
            }
        }

        fn issue_request(
            &self,
            request: &mut ExternalMutationRequestV1,
            label: &str,
        ) -> MissionServiceTransportContextV1 {
            let now_ms = self.clock.fetch_add(1, Ordering::SeqCst) + 1;
            let entries = self
                .service()
                .open_journal()
                .expect("A2 journal for authority")
                .entries();
            let semantic_payload_digest = inspect_request(
                request,
                &self.host,
                A2_MATRIX_SUBJECT,
                now_ms,
                &entries,
                &self.brain_id,
            )
            .expect("A2 authority inspection")
            .semantic_payload_digest()
            .to_string();
            let operation_object_digest =
                a2_operation_object_digest(request, &self.brain_id, &semantic_payload_digest);
            let (receipt, context) = a2_authority_receipt(
                request,
                &self.brain_id,
                &self.repo_root.to_string_lossy(),
                operation_object_digest.clone(),
                label,
                now_ms,
                self.receipt_crypto.as_ref(),
            );
            if matches!(
                request,
                ExternalMutationRequestV1::GraphIngestReplace { .. }
                    | ExternalMutationRequestV1::GraphIngestMergeExisting { .. }
            ) {
                bind_a2_graph_preview_to_context(
                    request,
                    &context,
                    &self.brain_id,
                    &operation_object_digest,
                );
            }
            *self.current_status.lock() = matrix_status(&receipt);
            self.service()
                .open_broker()
                .expect("A2 broker for authority")
                .issue(
                    context
                        .authority_lease_id
                        .as_deref()
                        .expect("A2 issued lease"),
                    receipt,
                    now_ms,
                )
                .expect("A2 sequential authority");
            context
        }

        fn execute_initial_replace(&self) -> ExternalMutationResponseV1 {
            self.service()
                .execute(
                    &self.initial_context,
                    self.initial_request.clone(),
                    self.host.clone(),
                )
                .expect("A2 sovereign replace")
        }

        fn execute_source_parent(&self, next_source: &str) -> GraphIngestA2ParentV1 {
            let target = self.target_path.to_string_lossy().into_owned();
            let target_for_actor = target.clone();
            let next_source = next_source.to_string();
            let preview = self
                .actor_registry
                .execute_target_runtime(
                    Arc::clone(&self.host.selected_brain),
                    None,
                    true,
                    true,
                    move |state| {
                        let preview = crate::surgical_handlers::handle_edit_preview(
                            state,
                            crate::protocol::surgical::EditPreviewInput {
                                file_path: target_for_actor.clone(),
                                agent_id: A2_MATRIX_SUBJECT.to_string(),
                                new_content: next_source,
                                description: Some("A2 causal source parent".to_string()),
                            },
                        )
                        .map_err(|error| {
                            crate::runtime_jobs::RuntimeJobFailure::new(
                                "A2_source_preview_failed",
                                error.to_string(),
                            )
                        })?;
                        state
                            .note_proof_ready(
                                A2_MATRIX_SUBJECT,
                                &target_for_actor,
                                "A2 source proof",
                            )
                            .map_err(|error| {
                                crate::runtime_jobs::RuntimeJobFailure::new(
                                    "A2_source_proof_failed",
                                    error.to_string(),
                                )
                            })?;
                        Ok(preview)
                    },
                )
                .expect("A2 source preview and proof inside actor");
            let mut request = ExternalMutationRequestV1::SourceEditCommit {
                schema: EXTERNAL_MUTATION_REQUEST_SCHEMA.to_string(),
                request_id: "A2-source-parent".to_string(),
                request: SourceEditCommitRequestV1 {
                    schema: SOURCE_EDIT_COMMIT_REQUEST_SCHEMA.to_string(),
                    preview_id: preview.preview_id,
                },
            };
            let context = self.issue_request(&mut request, "A2-source-parent");
            let response = self
                .service()
                .execute(&context, request, self.host.clone())
                .expect("A2 source parent commit");
            assert!(response.graph_resync_required);
            assert_eq!(response.reconciliation_state, "PENDING_RECONCILIATION");
            let entry = self
                .service()
                .open_journal()
                .expect("A2 parent journal")
                .entry(&response.journal_operation_id)
                .cloned()
                .expect("A2 parent entry");
            assert_eq!(entry.phase, ExternalMutationJournalPhaseV1::Published);
            GraphIngestA2ParentV1 {
                operation_id: response.journal_operation_id,
                lease_id: response.authorization_lease_id,
                reservation_id: response.authorization_reservation_id,
                operation_object_digest: response.operation_object_digest,
                semantic_payload_digest: response.semantic_payload_digest,
                outcome_digest: response.outcome_digest,
                published_result_digest: entry
                    .published_result_digest
                    .expect("A2 parent published result digest"),
            }
        }
    }

    fn a2_operation_object_digest(
        request: &ExternalMutationRequestV1,
        brain_id: &str,
        semantic_payload_digest: &str,
    ) -> String {
        digest_canonical(
            EXTERNAL_MUTATION_OPERATION_OBJECT_DIGEST_DOMAIN,
            &ExternalMutationOperationObjectV1 {
                schema: EXTERNAL_MUTATION_OPERATION_OBJECT_SCHEMA,
                semantic_action: request.semantic_action_id(),
                ingress: Ingress::Mcp,
                brain_id,
                mission_id: None,
                mission_head_id: None,
                operation_version: EXTERNAL_MUTATION_OPERATION_VERSION,
                semantic_payload_digest,
            },
        )
        .expect("A2 operation object")
    }

    fn a2_authority_receipt(
        request: &ExternalMutationRequestV1,
        brain_id: &str,
        route_selector: &str,
        operation_object_digest: String,
        label: &str,
        now_ms: u64,
        crypto: &dyn AuthorityWalRecordCrypto,
    ) -> (
        AuthorityAuthorizationReceiptV1,
        MissionServiceTransportContextV1,
    ) {
        let contract = external_consumer_contract(request.semantic_action_id(), Ingress::Mcp)
            .expect("A2 external contract");
        let autonomous = contract.authority_floor == m1nd_control::AuthorityFloor::ScopedGrantA2;
        let authority_variant = if autonomous {
            AuthorityVariant::AgentQuorum
        } else {
            AuthorityVariant::Human
        };
        let active_mode = if autonomous {
            ActiveMode::FullAutonomy
        } else {
            ActiveMode::HumanGated
        };
        let capability_kind = if autonomous {
            CapabilityKind::Autonomy
        } else {
            CapabilityKind::Human
        };
        let lease_id = format!("external-{label}-lease");
        let transport_id = format!("external-{label}-transport");
        let ingress_context_digest = matrix_hash(&format!("{label}-ingress"));
        let action = ActionId::new(request.semantic_action_id()).expect("A2 action id");
        let receipt = sign_matrix_receipt(
            AuthorityAuthorizationReceiptV1::new_for_broker_test(
                AuthorityAuthorizationReceiptCoreV1 {
                    organism_id: "organism-A2-matrix".to_string(),
                    repo_id: "repo-A2-matrix".to_string(),
                    brain_id: brain_id.to_string(),
                    subject_id: A2_MATRIX_SUBJECT.to_string(),
                    role: Role::Author,
                    capability_id: format!("capability-{label}"),
                    capability_kind: Some(capability_kind),
                    verified_object_digest: operation_object_digest,
                    mission_id: None,
                    mission_head_id: None,
                    transport_session_id: transport_id.clone(),
                    ingress_context_digest: ingress_context_digest.clone(),
                    action: action.clone(),
                    ingress: Ingress::Mcp,
                    complete_effects: contract.expected_effects,
                    active_mode,
                    constitution_digest: matrix_hash(&format!("{label}-constitution")),
                    constitution_epoch: 7,
                    autonomy_epoch: if autonomous { 1 } else { 0 },
                    protected_epoch_at_decision: 11,
                    policy_registry_digest: matrix_hash(&format!("{label}-policy")),
                    exact_policy_tuple: ReachablePolicyTupleV1 {
                        ingress: Ingress::Mcp,
                        action,
                        active_mode,
                        subject_id: A2_MATRIX_SUBJECT.to_string(),
                        authority_variant,
                        applicable_grant_id: autonomous.then(|| "grant:A2-ingest".to_string()),
                        applicable_tier: autonomous.then_some(AutonomyTier::A2Execute),
                        risk_class: contract.risk_class,
                    },
                    authority_decision_digest: Some(matrix_hash(&format!("{label}-decision"))),
                    autonomy_admission_receipt_digest: autonomous
                        .then(|| matrix_hash(&format!("{label}-admission"))),
                    autonomy_committed_state_digest: autonomous
                        .then(|| matrix_hash(&format!("{label}-autonomy-state"))),
                    autonomy_protected_root_digest: autonomous
                        .then(|| matrix_hash(&format!("{label}-autonomy-root"))),
                    authority: AuthorizationAuthorityV1::Positive {
                        variant: authority_variant,
                        assurance: AuthorityVerificationAssurance::SoftwareTestOnlyNotProven,
                    },
                    authority_body_digest: matrix_hash(&format!("{label}-authority-body")),
                    replay_sequence: now_ms,
                    journal_sequence: now_ms,
                    journal_root_digest: matrix_hash(&format!("{label}-journal-root")),
                    protected_epoch: 11,
                    authorized_at: now_ms,
                    expires_at: now_ms + 10_000,
                },
            ),
            crypto,
        );
        let context = MissionServiceTransportContextV1 {
            ingress: MissionServiceIngressV1::McpStreamableHttp,
            transport_session_id: Some(transport_id),
            ingress_context_digest: Some(ingress_context_digest),
            authority_lease_id: Some(lease_id),
            caller_root: Some(route_selector.to_string()),
            route_selector: Some(route_selector.to_string()),
            actor_brain_id: Some(brain_id.to_string()),
        };
        (receipt, context)
    }

    fn a2_preview_context(
        brain_id: &str,
        route_selector: &str,
        label: &str,
    ) -> MissionServiceTransportContextV1 {
        MissionServiceTransportContextV1 {
            ingress: MissionServiceIngressV1::McpStreamableHttp,
            transport_session_id: Some(format!("external-{label}-transport")),
            ingress_context_digest: Some(matrix_hash(&format!("{label}-ingress"))),
            authority_lease_id: None,
            caller_root: Some(route_selector.to_string()),
            route_selector: Some(route_selector.to_string()),
            actor_brain_id: Some(brain_id.to_string()),
        }
    }

    fn bind_a2_graph_preview_to_context(
        request: &mut ExternalMutationRequestV1,
        context: &MissionServiceTransportContextV1,
        brain_id: &str,
        operation_object_digest: &str,
    ) {
        let preview_id = graph_ingest_preview_id(
            context
                .transport_session_id
                .as_deref()
                .expect("A2 preview transport"),
            context
                .ingress_context_digest
                .as_deref()
                .expect("A2 preview ingress context"),
            context.route_selector.as_deref(),
            brain_id,
            operation_object_digest,
        )
        .expect("A2 owner preview id");
        match request {
            ExternalMutationRequestV1::GraphIngestReplace { request, .. }
            | ExternalMutationRequestV1::GraphIngestMergeExisting { request, .. } => {
                request.preview_id = preview_id;
            }
            _ => panic!("A2 graph preview binding requires graph ingest"),
        }
    }

    #[test]
    fn graph_ingest_preview_is_owner_derived_exact_and_read_only() {
        let fixture = A2MatrixFixture::new();
        let graph_before = fixture.graph_preimage();
        let actor_calls_before = fixture.actor_calls.load(Ordering::SeqCst);
        let journal_before = fixture
            .service()
            .open_journal()
            .expect("A2 preview journal before")
            .entries();
        let broker_before = fixture
            .service()
            .open_broker()
            .expect("A2 preview broker before")
            .leases();

        let (context, preview) = fixture.preview_replace("A2-preview-read-only");

        assert_eq!(preview.schema, GRAPH_INGEST_PREVIEW_RESPONSE_SCHEMA);
        assert_eq!(preview.semantic_action, "graph.ingest.replace");
        assert_eq!(preview.ingress, Ingress::Mcp);
        assert_eq!(preview.authority_floor, AuthorityFloor::PositiveSovereign);
        assert_eq!(preview.risk_class, RiskClass::Critical);
        assert_eq!(preview.actor_brain_id, fixture.brain_id);
        assert_eq!(
            preview.route_selector.as_deref(),
            Some(fixture.repo_root.to_string_lossy().as_ref())
        );
        assert_ne!(
            preview.route_selector.as_deref(),
            Some(preview.actor_brain_id.as_str()),
            "root selector and durable actor identity are different facts"
        );
        assert_eq!(
            preview.transport_session_id,
            context.transport_session_id.clone().unwrap()
        );
        assert_eq!(
            preview.ingress_context_digest,
            context.ingress_context_digest.clone().unwrap()
        );
        assert_eq!(preview.root_identity, fixture.repo_root.to_string_lossy());
        assert_eq!(preview.expected_graph_generation, graph_before.0);
        assert_eq!(preview.expected_source_projection_digest, graph_before.1);
        let scan_job = fixture
            .host
            .runtime_jobs
            .as_ref()
            .expect("preview runtime registry")
            .get(&preview.scan_job_id)
            .expect("preview scan runtime job");
        assert_eq!(scan_job.state, RuntimeJobState::Succeeded);
        assert_eq!(scan_job.binding.brain_id, fixture.brain_id);
        assert_eq!(scan_job.binding.action.as_str(), "graph.ingest.preview");
        assert_eq!(scan_job.binding.effects, BTreeSet::from([Effect::Read]));
        assert_eq!(
            scan_job
                .terminal_result
                .as_ref()
                .and_then(|result| result.output_digest.as_deref()),
            Some(preview.semantic_payload_digest.as_str())
        );
        for digest in [
            &preview.preview_id,
            &preview.candidate_ownership_digest,
            &preview.candidate_source_projection_digest,
            &preview.candidate_pipeline_digest,
            &preview.semantic_payload_digest,
            &preview.operation_object_digest,
        ] {
            assert_eq!(digest.len(), 64);
            assert!(digest.bytes().all(|byte| byte.is_ascii_hexdigit()));
        }
        assert_eq!(
            preview.authority_binding.target_action,
            preview.semantic_action
        );
        assert_eq!(
            preview.authority_binding.payload_digest,
            preview.operation_object_digest
        );
        assert_eq!(
            preview.authority_binding.requested_effects,
            preview.requested_effects
        );
        assert!(preview.authority_binding.mission_id.is_none());
        assert!(preview.authority_binding.mission_head_id.is_none());
        preview
            .execute_request
            .validate_wire()
            .expect("preview emits an exact executable request template");
        assert_eq!(
            graph_ingest_request_preview_id(&preview.execute_request),
            Some(preview.preview_id.as_str())
        );

        assert_eq!(fixture.graph_preimage(), graph_before);
        assert_eq!(
            fixture.actor_calls.load(Ordering::SeqCst),
            actor_calls_before,
            "read-only preview cannot request an actor checkpoint"
        );
        assert_eq!(
            fixture
                .service()
                .open_journal()
                .expect("A2 preview journal after")
                .entries(),
            journal_before
        );
        assert_eq!(
            fixture
                .service()
                .open_broker()
                .expect("A2 preview broker after")
                .leases(),
            broker_before
        );
    }

    #[test]
    fn graph_ingest_blocked_scan_keeps_actor_healthy_and_exposes_cancel_backpressure() {
        use std::sync::Barrier;

        let fixture = A2MatrixFixture::new();
        let limited_jobs = RuntimeJobRegistry::open_with_max_in_flight(
            fixture._temp.path().join("A2-limited-scan-jobs.jsonl"),
            1,
        )
        .expect("bounded scan registry");
        let mut host = fixture.host.clone();
        host.runtime_jobs = Ok(limited_jobs.clone());
        let worker_host = host.clone();
        let context = a2_preview_context(
            &fixture.brain_id,
            &fixture.repo_root.to_string_lossy(),
            "A2-blocked-scan",
        );
        let request = GraphIngestPreviewRequestV1 {
            schema: GRAPH_INGEST_PREVIEW_REQUEST_SCHEMA.to_string(),
            request_id: "A2-blocked-scan".to_string(),
            mode: GraphIngestA2ModeV1::Replace,
            include_dotfiles: false,
            dotfile_patterns: Vec::new(),
            parent: None,
        };
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let entered_for_hook = Arc::clone(&entered);
        let release_for_hook = Arc::clone(&release);
        let service = fixture
            .service()
            .with_fault_hook_for_test(Arc::new(move |point| {
                if point == "during_graph_ingest_scan" {
                    entered_for_hook.wait();
                    release_for_hook.wait();
                }
                Ok(())
            }));
        let worker = std::thread::spawn(move || {
            service.preview_graph_ingest(&context, request, worker_host)
        });
        entered.wait();

        let running = limited_jobs.list().expect("observable scan jobs");
        assert_eq!(running.len(), 1);
        assert_eq!(running[0].state, RuntimeJobState::Running);
        assert_eq!(running[0].binding.brain_id, fixture.brain_id);
        let health = limited_jobs.health_snapshot().expect("scan job health");
        assert_eq!(health.active_jobs, 1);
        assert_eq!(health.max_in_flight, 1);
        let graph_while_blocked = fixture.graph_preimage();
        assert_eq!(graph_while_blocked, fixture.graph_preimage());

        let busy_context = a2_preview_context(
            &fixture.brain_id,
            &fixture.repo_root.to_string_lossy(),
            "A2-backpressure",
        );
        let busy = fixture
            .service()
            .preview_graph_ingest(
                &busy_context,
                GraphIngestPreviewRequestV1 {
                    schema: GRAPH_INGEST_PREVIEW_REQUEST_SCHEMA.to_string(),
                    request_id: "A2-backpressure".to_string(),
                    mode: GraphIngestA2ModeV1::Replace,
                    include_dotfiles: false,
                    dotfile_patterns: Vec::new(),
                    parent: None,
                },
                host,
            )
            .expect_err("bounded registry must reject a second active scan");
        assert_eq!(busy.code(), "graph_ingest_scan_job_refused");

        let job_id = running[0].job_id.clone();
        let cancelling = limited_jobs
            .request_cancel(&job_id)
            .expect("explicit scan cancellation");
        assert_eq!(cancelling.state, RuntimeJobState::Cancelling);
        release.wait();
        let cancelled = worker
            .join()
            .expect("preview worker joined")
            .expect_err("cancelled scan cannot publish a preview");
        assert_eq!(cancelled.code(), "graph_ingest_scan_job_failed");
        let terminal = limited_jobs
            .wait_terminal(&job_id, Duration::from_secs(2))
            .expect("cancelled scan terminal");
        let RuntimeJobWait::Terminal(terminal) = terminal else {
            panic!("cancelled scan remained non-terminal")
        };
        assert_eq!(terminal.state, RuntimeJobState::Cancelled);
        assert_eq!(fixture.graph_preimage(), graph_while_blocked);
    }

    #[test]
    fn graph_ingest_actor_job_has_no_scanner_or_source_revalidation_calls() {
        let source = include_str!("external_mutation_service.rs");
        let actor_start = source
            .find("impl GraphIngestActorJobV1")
            .expect("graph actor implementation");
        let actor_end = source[actor_start..]
            .find("pub struct BrainPromoteActorJobV1")
            .map(|offset| actor_start + offset)
            .expect("next actor implementation");
        let actor = &source[actor_start..actor_end];
        for forbidden in [
            "Ingestor",
            "ingest_bundle",
            "build_complete_bundle",
            "complete_inspection_off_actor",
            "revalidate_sources",
        ] {
            assert!(
                !actor.contains(forbidden),
                "brain actor closure must not contain blocking scan call {forbidden}"
            );
        }
    }

    #[test]
    fn graph_ingest_execute_rejects_cross_actor_session_and_route_before_reserve() {
        let fixture = A2MatrixFixture::new();
        let (_preview_context, preview) = fixture.preview_replace("A2-negative-bindings");
        let mut execute_request = preview.execute_request;
        let context = fixture.issue_request(&mut execute_request, "A2-negative-bindings");
        let lease_id = context.authority_lease_id.clone().unwrap();
        let graph_before = fixture.graph_preimage();

        let mut cross_actor = context.clone();
        cross_actor.actor_brain_id = Some(matrix_hash("foreign-A2-actor"));
        let error = fixture
            .service()
            .execute(&cross_actor, execute_request.clone(), fixture.host.clone())
            .expect_err("cross-actor execute must fail closed");
        assert_eq!(error.code(), "external_mutation_selected_actor_mismatch");

        let mut cross_session = context.clone();
        cross_session.transport_session_id = Some("foreign-A2-session".to_string());
        let error = fixture
            .service()
            .execute(
                &cross_session,
                execute_request.clone(),
                fixture.host.clone(),
            )
            .expect_err("cross-session execute must fail closed");
        assert_eq!(
            error.code(),
            "external_mutation_authorization_binding_mismatch"
        );

        let mut cross_route = context.clone();
        cross_route.route_selector = Some(fixture.repo_root.join("foreign").display().to_string());
        let error = fixture
            .service()
            .execute(&cross_route, execute_request.clone(), fixture.host.clone())
            .expect_err("cross-route preview replay must fail closed");
        assert_eq!(error.code(), "graph_ingest_preview_binding_mismatch");

        assert_eq!(fixture.graph_preimage(), graph_before);
        assert!(fixture
            .service()
            .open_journal()
            .expect("A2 negative journal")
            .entries()
            .is_empty());
        assert_eq!(
            fixture
                .service()
                .open_broker()
                .expect("A2 negative broker")
                .lease(&lease_id)
                .expect("A2 negative lease")
                .state,
            AuthorizationLeaseStateV1::Unused
        );
    }

    #[test]
    fn graph_ingest_execute_reinspects_stale_preview_before_reserve() {
        let fixture = A2MatrixFixture::new();
        let (_preview_context, preview) = fixture.preview_replace("A2-stale-preview");
        let mut execute_request = preview.execute_request;
        let context = fixture.issue_request(&mut execute_request, "A2-stale-preview");
        let lease_id = context.authority_lease_id.clone().unwrap();
        fixture
            .actor_registry
            .execute_target_runtime(
                Arc::clone(&fixture.host.selected_brain),
                None,
                true,
                true,
                |state| {
                    state.graph_generation = state.graph_generation.saturating_add(1);
                    Ok(())
                },
            )
            .expect("advance A2 graph generation after preview");
        let stale_preimage = fixture.graph_preimage();

        let error = fixture
            .service()
            .execute(&context, execute_request, fixture.host.clone())
            .expect_err("stale owner preview must fail closed");
        assert_eq!(error.code(), "graph_ingest_stale_generation");
        assert_eq!(fixture.graph_preimage(), stale_preimage);
        assert!(fixture
            .service()
            .open_journal()
            .expect("A2 stale journal")
            .entries()
            .is_empty());
        assert_eq!(
            fixture
                .service()
                .open_broker()
                .expect("A2 stale broker")
                .lease(&lease_id)
                .expect("A2 stale lease")
                .state,
            AuthorizationLeaseStateV1::Unused
        );
    }

    #[test]
    fn graph_ingest_replace_is_checkpoint_bound_and_terminal_replay_is_idempotent() {
        let fixture = A2MatrixFixture::new();
        let (generation_before, projection_before) = fixture.graph_preimage();
        let first = fixture.execute_initial_replace();
        let (generation_after, projection_after) = fixture.graph_preimage();
        assert!(generation_after > generation_before);
        assert_ne!(projection_after, projection_before);
        assert_eq!(fixture.actor_calls.load(Ordering::SeqCst), 1);
        assert_eq!(first.semantic_action, "graph.ingest.replace");
        assert_eq!(first.reconciliation_state, "RECONCILED");
        let prepared = fixture
            .service()
            .open_journal()
            .expect("A2 sealed journal")
            .entry(&first.journal_operation_id)
            .cloned()
            .expect("A2 sealed operation");
        assert_eq!(prepared.prepare.actor_brain_id, fixture.brain_id);
        assert_eq!(
            prepared.prepare.route_selector.as_deref(),
            Some(fixture.repo_root.to_string_lossy().as_ref())
        );
        assert_ne!(
            prepared.prepare.actor_brain_id,
            prepared.prepare.route_selector.clone().unwrap()
        );
        assert_eq!(
            first.result["checkpoint_ack"]["brain_id"],
            Value::String(fixture.reconciliation_brain_id.clone())
        );
        assert_eq!(
            first.result["checkpoint_ack"]["generation"],
            Value::from(generation_after)
        );
        assert_eq!(
            first.result["candidate_source_projection_digest"],
            Value::String(projection_after.clone())
        );

        let replay_request = request_with_id(fixture.initial_request.clone(), "A2-replace-replay");
        let replay = fixture
            .service()
            .execute(
                &fixture.initial_context,
                replay_request,
                fixture.host.clone(),
            )
            .expect("A2 terminal replay");
        assert_eq!(replay.result["terminal_replay"], Value::Bool(true));
        assert_first_and_replay_are_equivalent(&first, &replay);
        assert_eq!(
            fixture.graph_preimage(),
            (generation_after, projection_after)
        );
        assert_eq!(fixture.actor_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn graph_ingest_never_mutates_before_committed_actor_decision_or_checkpoint_ack() {
        for cut in ["after_journal_prepared", "after_journal_committed"] {
            let fixture = A2MatrixFixture::new();
            let before = fixture.graph_preimage();
            fixture
                .service_crashing_at(cut)
                .execute(
                    &fixture.initial_context,
                    fixture.initial_request.clone(),
                    fixture.host.clone(),
                )
                .expect_err("A2 pre-publication cut must fail closed");
            assert_eq!(fixture.graph_preimage(), before, "cut={cut}");
            let entries = fixture
                .service()
                .open_journal()
                .expect("A2 cut journal")
                .entries();
            assert_eq!(entries.len(), 1, "cut={cut}");
            assert!(entries[0].published_result.is_none(), "cut={cut}");
            assert!(entries[0]
                .prepare
                .recovery_payload
                .get("ownership_manifest")
                .is_some());
            if cut == "after_journal_prepared" {
                assert_eq!(fixture.actor_calls.load(Ordering::SeqCst), 0);
                assert_eq!(entries[0].phase, ExternalMutationJournalPhaseV1::Prepared);
            } else {
                assert_eq!(fixture.actor_calls.load(Ordering::SeqCst), 1);
                assert!(matches!(
                    entries[0].phase,
                    ExternalMutationJournalPhaseV1::Committed
                        | ExternalMutationJournalPhaseV1::RecoveryRequired
                ));
            }
        }
    }

    #[test]
    fn graph_ingest_committed_recovery_uses_sealed_candidate_after_source_drift() {
        let fixture = A2MatrixFixture::new();
        let graph_before = fixture.graph_preimage();
        fixture
            .service_crashing_at("after_journal_committed")
            .execute(
                &fixture.initial_context,
                fixture.initial_request.clone(),
                fixture.host.clone(),
            )
            .expect_err("committed crash cut must require recovery");
        assert_eq!(fixture.graph_preimage(), graph_before);
        let entry = fixture
            .service()
            .open_journal()
            .expect("committed A2 journal")
            .entries()
            .into_iter()
            .next()
            .expect("committed A2 entry");
        let sealed_projection = entry.prepare.recovery_payload["ownership_manifest"]
            ["source_projection_digest"]
            .as_str()
            .expect("sealed candidate projection")
            .to_string();

        std::fs::write(&fixture.target_path, A2_MATRIX_AFTER)
            .expect("source drifts after COMMITTED");
        let recovered = fixture
            .service()
            .execute(
                &fixture.initial_context,
                request_with_id(
                    fixture.initial_request.clone(),
                    "A2-recover-after-source-drift",
                ),
                fixture.host.clone(),
            )
            .expect("recovery must use immutable candidate bytes, not the live tree");
        assert_eq!(recovered.result["terminal_replay"], Value::Bool(true));
        assert_eq!(fixture.graph_preimage().1, sealed_projection);
        assert_eq!(
            std::fs::read_to_string(&fixture.target_path).expect("drifted source remains"),
            A2_MATRIX_AFTER,
            "forward recovery must not rewrite source files"
        );
    }

    #[test]
    fn graph_ingest_committed_recovery_refuses_corrupt_candidate_before_graph_install() {
        let fixture = A2MatrixFixture::new();
        let graph_before = fixture.graph_preimage();
        fixture
            .service_crashing_at("after_journal_committed")
            .execute(
                &fixture.initial_context,
                fixture.initial_request.clone(),
                fixture.host.clone(),
            )
            .expect_err("committed crash cut must require recovery");
        let entry = fixture
            .service()
            .open_journal()
            .expect("committed A2 journal")
            .entries()
            .into_iter()
            .next()
            .expect("committed A2 entry");
        let relative = entry.prepare.recovery_payload["candidate_artifact"]["relative_path"]
            .as_str()
            .expect("sealed candidate path");
        let artifact = fixture.journal_root.join(relative);
        let mut bytes = std::fs::read(&artifact).expect("candidate bytes");
        let last = bytes.last_mut().expect("non-empty candidate");
        *last ^= 0x5a;
        std::fs::write(&artifact, bytes).expect("corrupt candidate in place");

        let error = fixture
            .service()
            .execute(
                &fixture.initial_context,
                request_with_id(
                    fixture.initial_request.clone(),
                    "A2-recover-corrupt-candidate",
                ),
                fixture.host.clone(),
            )
            .expect_err("corrupt immutable candidate must fail closed");
        assert_eq!(error.code(), "graph_ingest_candidate_artifact_corrupt");
        assert_eq!(fixture.graph_preimage(), graph_before);
        let current = fixture
            .service()
            .open_journal()
            .expect("A2 journal after corruption")
            .entries()
            .into_iter()
            .next()
            .expect("A2 entry remains");
        assert!(matches!(
            current.phase,
            ExternalMutationJournalPhaseV1::Committed
                | ExternalMutationJournalPhaseV1::RecoveryRequired
        ));
        assert!(current.published_result.is_none());
    }

    #[test]
    fn graph_ingest_merge_is_exact_autonomous_child_with_own_checkpoint_and_replay() {
        let fixture = A2MatrixFixture::new();
        fixture.execute_initial_replace();
        let parent = fixture.execute_source_parent(A2_MATRIX_AFTER);
        assert_eq!(
            std::fs::read_to_string(&fixture.target_path).unwrap(),
            A2_MATRIX_AFTER
        );
        let before_merge = fixture.graph_preimage();
        let mut request = fixture.merge_request(parent.clone(), "A2-merge");
        let context = fixture.issue_request(&mut request, "A2-merge");
        let first = fixture
            .service()
            .execute(&context, request.clone(), fixture.host.clone())
            .expect("A2 merge child");
        let after_merge = fixture.graph_preimage();
        assert!(after_merge.0 > before_merge.0);
        assert_ne!(after_merge.1, before_merge.1);
        assert_eq!(first.semantic_action, "graph.ingest.merge_existing");
        assert_eq!(first.reconciliation_state, "RECONCILED");
        assert_eq!(
            first.result["parent"],
            serde_json::to_value(parent).unwrap()
        );
        assert_eq!(
            first.result["checkpoint_ack"]["brain_id"],
            Value::String(fixture.reconciliation_brain_id.clone())
        );
        assert_eq!(
            fixture.actor_calls.load(Ordering::SeqCst),
            3,
            "replace, WIRE-REAL source parent, and merge each own one exact actor checkpoint"
        );

        let replay = fixture
            .service()
            .execute(
                &context,
                request_with_id(request, "A2-merge-replay"),
                fixture.host.clone(),
            )
            .expect("A2 merge terminal replay");
        assert_eq!(replay.result["terminal_replay"], Value::Bool(true));
        assert_first_and_replay_are_equivalent(&first, &replay);
        assert_eq!(fixture.graph_preimage(), after_merge);
        assert_eq!(
            fixture.actor_calls.load(Ordering::SeqCst),
            3,
            "terminal merge replay must not enter a fourth actor turn"
        );
    }

    #[test]
    fn graph_ingest_admission_refuses_stale_root_foreign_incomplete_and_causal_drift() {
        let fixture = A2MatrixFixture::new();
        let empty_entries = Vec::new();
        let mut stale_generation = fixture.initial_request.clone();
        if let ExternalMutationRequestV1::GraphIngestReplace { request, .. } = &mut stale_generation
        {
            request.expected_graph_generation += 1;
        }
        assert_eq!(
            fixture.inspect_error(&stale_generation, &empty_entries),
            "graph_ingest_stale_generation"
        );
        let mut stale_projection = fixture.initial_request.clone();
        if let ExternalMutationRequestV1::GraphIngestReplace { request, .. } = &mut stale_projection
        {
            request.expected_source_projection_digest = matrix_hash("stale-A2-projection");
        }
        assert_eq!(
            fixture.inspect_error(&stale_projection, &empty_entries),
            "graph_ingest_stale_projection"
        );
        let foreign_root = fixture._temp.path().join("foreign-root");
        std::fs::create_dir_all(&foreign_root).expect("foreign A2 root");
        let mut root_mismatch = fixture.initial_request.clone();
        if let ExternalMutationRequestV1::GraphIngestReplace { request, .. } = &mut root_mismatch {
            request.root = foreign_root.to_string_lossy().into_owned();
        }
        assert_eq!(
            fixture.inspect_error(&root_mismatch, &empty_entries),
            "graph_ingest_root_mismatch"
        );

        fixture.execute_initial_replace();
        let parent = fixture.execute_source_parent(A2_MATRIX_AFTER);
        let request = fixture.merge_request(parent.clone(), "A2-merge-admission");
        let entries = fixture
            .service()
            .open_journal()
            .expect("A2 admission journal")
            .entries();
        inspect_request(
            &request,
            &fixture.host,
            A2_MATRIX_SUBJECT,
            fixture.clock.load(Ordering::SeqCst),
            &entries,
            &fixture.brain_id,
        )
        .expect("exact A2 merge admission");

        let mut changed_controls = request.clone();
        if let ExternalMutationRequestV1::GraphIngestMergeExisting { request, .. } =
            &mut changed_controls
        {
            request.include_dotfiles = true;
        }
        assert_eq!(
            fixture.inspect_error(&changed_controls, &entries),
            "graph_ingest_discovery_controls_changed"
        );

        let mut forged_parent = request.clone();
        if let ExternalMutationRequestV1::GraphIngestMergeExisting { request, .. } =
            &mut forged_parent
        {
            request
                .parent
                .as_mut()
                .expect("A2 merge parent")
                .outcome_digest = matrix_hash("forged-A2-parent");
        }
        assert_eq!(
            fixture.inspect_error(&forged_parent, &entries),
            "graph_ingest_parent_binding_mismatch"
        );

        let mut superseded_entries = entries.clone();
        let parent_entry = superseded_entries
            .iter()
            .find(|entry| entry.operation_id == parent.operation_id)
            .expect("A2 source parent entry")
            .clone();
        let mut later_parent = parent_entry;
        later_parent.operation_id = "synthetic-later-source-parent".to_string();
        later_parent.updated_at += 1;
        superseded_entries.push(later_parent);
        assert_eq!(
            fixture.inspect_error(&request, &superseded_entries),
            "graph_ingest_parent_superseded"
        );

        let mut incomplete_entries = entries.clone();
        let baseline = incomplete_entries
            .iter_mut()
            .find(|entry| entry.prepare.semantic_action == "graph.ingest.replace")
            .expect("A2 baseline entry");
        baseline.prepare.recovery_payload["ownership_manifest"]["coverage"] =
            Value::String("INCOMPLETE".to_string());
        assert_eq!(
            fixture.inspect_error(&request, &incomplete_entries),
            "graph_ingest_baseline_untrustworthy"
        );

        let foreign_request = {
            fixture
                .actor_registry
                .execute_target_runtime(
                    Arc::clone(&fixture.host.selected_brain),
                    None,
                    true,
                    true,
                    |state| {
                        {
                            let mut graph = state.graph.write();
                            graph
                                .add_node(
                                    "file::foreign.rs",
                                    "foreign.rs",
                                    m1nd_core::types::NodeType::File,
                                    &[],
                                    0.0,
                                    0.0,
                                )
                                .map_err(|error| {
                                    crate::runtime_jobs::RuntimeJobFailure::new(
                                        "A2_foreign_node_failed",
                                        error.to_string(),
                                    )
                                })?;
                            graph.finalize().map_err(|error| {
                                crate::runtime_jobs::RuntimeJobFailure::new(
                                    "A2_foreign_graph_finalize_failed",
                                    error.to_string(),
                                )
                            })?;
                        }
                        state.rebuild_engines().map_err(|error| {
                            crate::runtime_jobs::RuntimeJobFailure::new(
                                "A2_foreign_engine_rebuild_failed",
                                error.to_string(),
                            )
                        })?;
                        Ok(())
                    },
                )
                .expect("foreign A2 code node inside actor");
            fixture.merge_request(parent, "A2-foreign-node")
        };
        assert_eq!(
            fixture.inspect_error(&foreign_request, &entries),
            "graph_ingest_foreign_nodes"
        );
    }

    #[test]
    fn graph_ingest_wire_refuses_missing_or_cross_mode_parent_before_authority() {
        let fixture = A2MatrixFixture::new();
        let parent = GraphIngestA2ParentV1 {
            operation_id: "source-operation".to_string(),
            lease_id: "source-lease".to_string(),
            reservation_id: "source-reservation".to_string(),
            operation_object_digest: matrix_hash("A2-parent-object"),
            semantic_payload_digest: matrix_hash("A2-parent-payload"),
            outcome_digest: matrix_hash("A2-parent-outcome"),
            published_result_digest: matrix_hash("A2-parent-published"),
        };
        let mut replace = fixture.initial_request.clone();
        if let ExternalMutationRequestV1::GraphIngestReplace { request, .. } = &mut replace {
            request.parent = Some(parent.clone());
        }
        assert_eq!(
            replace
                .validate_wire()
                .expect_err("replace parent forbidden")
                .code(),
            "graph_ingest_replace_parent_forbidden"
        );
        let mut merge = fixture.merge_request(parent, "A2-missing-parent");
        if let ExternalMutationRequestV1::GraphIngestMergeExisting { request, .. } = &mut merge {
            request.parent = None;
        }
        assert_eq!(
            merge
                .validate_wire()
                .expect_err("merge parent required")
                .code(),
            "graph_ingest_parent_required"
        );
    }

    #[test]
    fn promotion_locks_block_the_ordinary_medulla_writer() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source_store = temp.path().join("source").join("agent-memory");
        let medulla_runtime = temp.path().join("medulla");
        let medulla_store = medulla_runtime.join("agent-memory");
        std::fs::create_dir_all(&source_store).expect("source store");
        std::fs::create_dir_all(&medulla_store).expect("medulla store");
        let locks = crate::promote_handlers::acquire_promote_target_locks(
            &source_store,
            "source",
            &medulla_store,
            "node",
        )
        .expect("external locks");
        let target = medulla_store.join("node.light.md");
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let runtime = medulla_runtime.clone();
        let writer = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            let mut input = crate::light_author_handlers::LightAuthorInput {
                agent_id: "internal-writer".to_string(),
                node_label: "node".to_string(),
                title: None,
                state: Some("verified".to_string()),
                claims: vec![crate::light_author_handlers::LightClaim {
                    label: "node".to_string(),
                    text: None,
                    kind: Some("entity".to_string()),
                    confidence: Some("0.9".to_string()),
                    ambiguity: None,
                    evidence: Vec::new(),
                    depends_on: Vec::new(),
                }],
                namespace: None,
                ingest_after: false,
                mode: "merge".to_string(),
                supersedes: None,
                origin_brain: Some("medulla".to_string()),
                origin_claim: None,
                promoted_by: None,
                promotion_reason: None,
                promoted_to: None,
                evidence_unverifiable: false,
                soul_source: None,
            };
            let result = crate::light_author_handlers::write_light_memory_superseding(
                &mut input, &target, &runtime,
            );
            done_tx.send(result.is_ok()).unwrap();
        });
        started_rx.recv().expect("writer started");
        assert!(done_rx.recv_timeout(Duration::from_millis(100)).is_err());
        drop(locks);
        assert!(done_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("writer unblocked"));
        writer.join().expect("writer joined");
    }
}
