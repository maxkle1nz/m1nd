//! Internal, authority-agnostic transaction adapter for `source.edit.commit`.
//!
//! The public raw tool remains fail-closed at the generic action gate. This
//! module accepts no lease, ratifier, authority variant, action, or effect from
//! the wire request. A typed owner service first inspects the exact preview and
//! proof mark, obtains an action-bound authorization over the returned canonical
//! object, and then injects the trusted [`SourceEditPreparedContextV1`].
//!
//! Source bytes and graph reconciliation are deliberately separate commits.
//! This transaction makes the source file old-or-new and emits a conservation
//! receipt with `graph_resync_required=true`; it never runs repository code and
//! never folds ingest into the filesystem atomic section.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use m1nd_control::{digest_canonical, Effect, Ingress};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::brain_runtime::BrainSessionCell;
use crate::light_author_handlers::LockGuard;
use crate::session::{EditPreviewState, ProofReadyMark, SessionState};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

pub(crate) const SOURCE_EDIT_COMMIT_REQUEST_SCHEMA: &str = "m1nd-source-edit-commit-request-v1";
pub(crate) const SOURCE_EDIT_COMMIT_SEMANTIC_PAYLOAD_SCHEMA: &str =
    "m1nd-source-edit-commit-semantic-payload-v1";
pub(crate) const SOURCE_EDIT_COMMIT_SEMANTIC_PAYLOAD_DIGEST_DOMAIN: &str =
    "m1nd-source-edit-commit-semantic-payload-v1";
pub(crate) const EXTERNAL_MUTATION_OPERATION_OBJECT_SCHEMA: &str =
    "m1nd-external-mutation-operation-object-v1";
pub(crate) const EXTERNAL_MUTATION_OPERATION_OBJECT_DIGEST_DOMAIN: &str =
    "m1nd-external-mutation-operation-object-v1";
pub(crate) const SOURCE_EDIT_COMMIT_ACTION: &str = "source.edit.commit";
pub(crate) const SOURCE_EDIT_OPERATION_VERSION: u64 = 1;

const SOURCE_EDIT_PROOF_SCOPE_SCHEMA: &str = "m1nd-source-edit-proof-scope-v1";
const SOURCE_EDIT_PROOF_SCOPE_DIGEST_DOMAIN: &str = "m1nd-source-edit-proof-scope-v1";
const SOURCE_EDIT_PROOF_MARK_DIGEST_DOMAIN: &str = "m1nd-source-edit-proof-mark-v1";
const SOURCE_EDIT_TRANSACTION_SCHEMA: &str = "m1nd-source-edit-transaction-v1";
const SOURCE_EDIT_TRANSACTION_DIGEST_DOMAIN: &str = "m1nd-source-edit-transaction-v1";
const SOURCE_EDIT_DESCRIPTOR_SCHEMA: &str = "m1nd-source-edit-descriptor-v1";
const SOURCE_EDIT_DESCRIPTOR_DIGEST_DOMAIN: &str = "m1nd-source-edit-descriptor-v1";
const SOURCE_EDIT_PRE_STAGE_INTENT_SCHEMA: &str = "m1nd-source-edit-pre-stage-intent-v1";
const SOURCE_EDIT_PRE_STAGE_INTENT_DIGEST_DOMAIN: &str = "m1nd-source-edit-pre-stage-intent-v1";
const SOURCE_EDIT_PRE_STAGE_ABORT_SCHEMA: &str = "m1nd-source-edit-pre-stage-abort-v1";
const SOURCE_EDIT_PRE_STAGE_ABORT_DIGEST_DOMAIN: &str = "m1nd-source-edit-pre-stage-abort-v1";
const SOURCE_EDIT_PRE_STAGE_ABORT_COMPLETION_SCHEMA: &str =
    "m1nd-source-edit-pre-stage-abort-completion-v1";
const SOURCE_EDIT_PRE_STAGE_ABORT_COMPLETION_DIGEST_DOMAIN: &str =
    "m1nd-source-edit-pre-stage-abort-completion-v1";
const SOURCE_EDIT_STAGE_SCHEMA: &str = "m1nd-source-edit-stage-v1";
const SOURCE_EDIT_STAGE_DIGEST_DOMAIN: &str = "m1nd-source-edit-stage-v1";
const SOURCE_EDIT_STAGE_ABORT_SCHEMA: &str = "m1nd-source-edit-stage-abort-v1";
const SOURCE_EDIT_STAGE_ABORT_DIGEST_DOMAIN: &str = "m1nd-source-edit-stage-abort-v1";
const SOURCE_EDIT_STAGE_ABORT_COMPLETION_SCHEMA: &str =
    "m1nd-source-edit-stage-abort-completion-v1";
const SOURCE_EDIT_STAGE_ABORT_COMPLETION_DIGEST_DOMAIN: &str =
    "m1nd-source-edit-stage-abort-completion-v1";
const SOURCE_EDIT_JOURNAL_EVENT_SCHEMA: &str = "m1nd-source-edit-journal-event-v1";
const SOURCE_EDIT_JOURNAL_EVENT_DIGEST_DOMAIN: &str = "m1nd-source-edit-journal-event-v1";
const SOURCE_EDIT_OUTCOME_SCHEMA: &str = "m1nd-source-edit-outcome-v1";
const SOURCE_EDIT_OUTCOME_DIGEST_DOMAIN: &str = "m1nd-source-edit-outcome-v1";
const SOURCE_EDIT_TERMINAL_RECEIPT_SCHEMA: &str = "m1nd-source-edit-terminal-receipt-v1";
const SOURCE_EDIT_TERMINAL_RECEIPT_DIGEST_DOMAIN: &str = "m1nd-source-edit-terminal-receipt-v1";
const SOURCE_EDIT_TX_DIRECTORY: &str = "source-edit-transactions-v1";
const PRE_STAGE_INTENT_PREFIX: &str = ".pre-stage-intent-";
const PRE_STAGE_ABORT_PREFIX: &str = ".pre-stage-abort-";
const PRE_STAGE_ABORT_COMPLETION_PREFIX: &str = ".pre-stage-abort-completion-";
const PREVIEW_TTL_MS: u64 = 5 * 60 * 1000;
const MAX_SOURCE_EDIT_BYTES: usize = 16 * 1024 * 1024;
const SAME_UID_TOCTOU_LIMITATION: &str = "same_uid_parent_entry_replacement_toctou_not_proven";
#[cfg(windows)]
const WINDOWS_DIRECTORY_FSYNC_LIMITATION: &str = "windows_directory_fsync_primitive_not_proven";

/// The only wire-shaped portion of the typed request. Authority subject,
/// action, effects, lease, mission binding, and ratifier are not caller fields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceEditCommitRequestV1 {
    pub schema: String,
    pub preview_id: String,
}

impl SourceEditCommitRequestV1 {
    pub(crate) const SCHEMA: &'static str = SOURCE_EDIT_COMMIT_REQUEST_SCHEMA;

    pub(crate) fn new(preview_id: impl Into<String>) -> Self {
        Self {
            schema: Self::SCHEMA.to_string(),
            preview_id: preview_id.into(),
        }
    }
}

/// Canonical action payload inspected before authorization. Every field is
/// recomputed from owner-held preview/proof/session state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceEditCommitSemanticPayloadV1 {
    pub schema: String,
    pub preview_id: String,
    pub target_identity: String,
    pub expected_target_sha256: String,
    pub candidate_sha256: String,
    pub expected_graph_generation: u64,
    pub proof_scope_digest: String,
    pub proof_mark_digest: String,
}

/// Read-only pre-authorization result. It carries no authority claim.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SourceEditCommitIntentV1 {
    pub semantic_payload: SourceEditCommitSemanticPayloadV1,
    pub semantic_payload_digest: String,
    pub proof_expires_at_ms: u64,
    pub preview_created_at_ms: u64,
}

/// Canonical outer object verified by the G2/G3 authority service.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceEditOperationObjectV1 {
    pub schema: String,
    pub semantic_action: String,
    pub ingress: Ingress,
    pub brain_id: String,
    pub mission_id: Option<String>,
    pub mission_head_id: Option<String>,
    pub operation_version: u64,
    pub semantic_payload_digest: String,
}

/// Trusted facts injected by the typed external-mutation service after exact
/// lease/authority verification. The adapter validates the values but never
/// reserves or consumes a lease itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceEditPreparedContextV1 {
    pub authority_subject_id: String,
    pub semantic_action: String,
    pub ingress: Ingress,
    pub semantic_payload_digest: String,
    pub operation_object_digest: String,
    pub expected_effects: BTreeSet<Effect>,
    pub brain_id: String,
    pub mission_id: Option<String>,
    pub mission_head_id: Option<String>,
    pub operation_version: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SourceEditPermissionV1 {
    pub readonly: bool,
    pub unix_mode: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceEditFileIdentityV1 {
    dev: Option<u64>,
    ino: Option<u64>,
    link_count: Option<u64>,
    len: u64,
}

#[derive(Clone, Debug)]
struct SourceEditTargetSnapshotV1 {
    bytes: Vec<u8>,
    sha256: String,
    permissions: SourceEditPermissionV1,
    identity: SourceEditFileIdentityV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SourceEditConservationV1 {
    pub target_identity_before: String,
    pub target_identity_after: String,
    pub target_count_before: u64,
    pub target_count_after: u64,
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub source_sha256_before: String,
    pub source_sha256_after: String,
    pub candidate_sha256: String,
    pub permissions_before: SourceEditPermissionV1,
    pub permissions_after: SourceEditPermissionV1,
    pub permissions_preserved: bool,
    pub graph_generation_prepared: u64,
    pub graph_generation_at_publish: u64,
    pub graph_resync_required: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceEditOutcomeStateV1 {
    AppliedGraphPending,
}

/// Durable pre-commit artifact. Creating this value never mutates the target:
/// it only seals the before-image, candidate, descriptor, and journal. The
/// outer authority journal binds `stage_digest`; only a later
/// `publish_after_commit` may replace source bytes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceEditStagedCommitCoreV1 {
    schema: String,
    transaction_id: String,
    operation_object_digest: String,
    semantic_payload_digest: String,
    pre_stage_intent_digest: String,
    target_identity: String,
    source_sha256_before: String,
    candidate_sha256: String,
    descriptor_digest: String,
    journal_root_digest: String,
    graph_generation_prepared: u64,
    assurance_limitations: Vec<String>,
    staged_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SourceEditStagedCommitV1 {
    #[serde(flatten)]
    core: SourceEditStagedCommitCoreV1,
    pub stage_digest: String,
}

impl SourceEditStagedCommitV1 {
    pub(crate) fn transaction_id(&self) -> &str {
        &self.core.transaction_id
    }

    pub(crate) fn operation_object_digest(&self) -> &str {
        &self.core.operation_object_digest
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceEditStageAbortReceiptCoreV1 {
    schema: String,
    transaction_id: String,
    operation_object_digest: String,
    semantic_payload_digest: String,
    stage_digest: String,
    descriptor_digest: String,
    journal_root_digest: String,
    managed_root: String,
    target_identity: String,
    source_sha256_before: String,
    candidate_sha256: String,
    bytes_before: u64,
    bytes_after: u64,
    permissions_before: SourceEditPermissionV1,
    candidate_temp_path: String,
    rollback_temp_path: String,
    backup_path: String,
    target_bytes_observed: bool,
    target_write_performed: bool,
    coordination_state_mutated: bool,
    aborted_at_ms: u64,
}

/// Durable proof that a PREPARED/no-COMMIT stage was discarded without
/// consulting or changing the managed target. The receipt remains in the
/// private transaction directory so an interrupted cleanup can resume exactly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SourceEditStageAbortReceiptV1 {
    #[serde(flatten)]
    core: SourceEditStageAbortReceiptCoreV1,
    pub abort_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceEditStageAbortCompletionCoreV1 {
    schema: String,
    transaction_id: String,
    operation_object_digest: String,
    stage_digest: String,
    descriptor_digest: String,
    abort_digest: String,
    target_bytes_observed: bool,
    target_write_performed: bool,
    coordination_state_mutated: bool,
    completed_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceEditStageAbortCompletionV1 {
    #[serde(flatten)]
    core: SourceEditStageAbortCompletionCoreV1,
    completion_digest: String,
}

impl SourceEditStageAbortReceiptV1 {
    pub(crate) fn transaction_id(&self) -> &str {
        &self.core.transaction_id
    }

    pub(crate) fn target_write_performed(&self) -> bool {
        self.core.target_write_performed
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceEditCommitOutcomeCoreV1 {
    schema: String,
    transaction_id: String,
    operation_object_digest: String,
    stage_digest: String,
    semantic_payload_digest: String,
    authority_subject_id: String,
    brain_id: String,
    mission_id: Option<String>,
    mission_head_id: Option<String>,
    state: SourceEditOutcomeStateV1,
    conservation: SourceEditConservationV1,
    journal_root_digest: String,
    graph_resync_required: bool,
    rollback_available: bool,
    assurance_limitations: Vec<String>,
    applied_at_ms: u64,
}

/// Durable physical-mutation result. This is intentionally not a claim that
/// graph reconciliation, outer AuthorityWAL finalization, or multi-OS recovery
/// has completed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SourceEditCommitOutcomeV1 {
    #[serde(flatten)]
    core: SourceEditCommitOutcomeCoreV1,
    pub outcome_digest: String,
}

impl SourceEditCommitOutcomeV1 {
    pub(crate) fn transaction_id(&self) -> &str {
        &self.core.transaction_id
    }

    pub(crate) fn operation_object_digest(&self) -> &str {
        &self.core.operation_object_digest
    }

    pub(crate) fn graph_resync_required(&self) -> bool {
        self.core.graph_resync_required
    }

    pub(crate) fn conservation(&self) -> &SourceEditConservationV1 {
        &self.core.conservation
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SourceEditTerminalStateV1 {
    FinalizedNew,
    RolledBackOld,
    RecoveredOld,
    RecoveredNew,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceEditTerminalReceiptCoreV1 {
    schema: String,
    transaction_id: String,
    operation_object_digest: String,
    terminal_state: SourceEditTerminalStateV1,
    source_sha256: String,
    graph_resync_required: bool,
    replay_reapplied_source_bytes: bool,
    journal_root_digest: String,
    assurance_limitations: Vec<String>,
    terminal_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SourceEditTerminalReceiptV1 {
    #[serde(flatten)]
    core: SourceEditTerminalReceiptCoreV1,
    pub receipt_digest: String,
}

impl SourceEditTerminalReceiptV1 {
    pub(crate) fn terminal_state(&self) -> SourceEditTerminalStateV1 {
        self.core.terminal_state
    }

    pub(crate) fn replay_reapplied_source_bytes(&self) -> bool {
        self.core.replay_reapplied_source_bytes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceEditRecoveryDecisionV1 {
    KeepNew,
    RestoreOld,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SourceEditJournalPhaseV1 {
    Prepared,
    BackupDurable,
    CandidateDurable,
    Staged,
    PublishIntent,
    Published,
    OutcomeDurable,
    Finalized,
    RollbackIntent,
    RolledBack,
    RecoveredOld,
    RecoveredNew,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceEditJournalEventCoreV1 {
    schema: String,
    transaction_id: String,
    sequence: u64,
    phase: SourceEditJournalPhaseV1,
    observed_target_sha256: String,
    previous_event_digest: Option<String>,
    at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceEditJournalEventV1 {
    #[serde(flatten)]
    core: SourceEditJournalEventCoreV1,
    event_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceEditDescriptorCoreV1 {
    schema: String,
    transaction_id: String,
    operation_object_digest: String,
    semantic_payload_digest: String,
    authority_subject_id: String,
    preview_id: String,
    brain_id: String,
    mission_id: Option<String>,
    mission_head_id: Option<String>,
    target_identity: String,
    managed_root: String,
    source_sha256_before: String,
    candidate_sha256: String,
    bytes_before: u64,
    bytes_after: u64,
    permissions_before: SourceEditPermissionV1,
    file_identity_before: SourceEditFileIdentityV1,
    graph_generation_prepared: u64,
    proof_mark_digest: String,
    proof_expires_at_ms: u64,
    candidate_temp_path: String,
    rollback_temp_path: String,
    backup_path: String,
    created_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceEditDescriptorV1 {
    #[serde(flatten)]
    core: SourceEditDescriptorCoreV1,
    descriptor_digest: String,
}

/// First durable artifact for a source edit. It lives beside (not inside) the
/// transaction directory, so even a crash before `mkdir(transaction_id)` leaves
/// an exact, authenticated cleanup/resume manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceEditPreStageIntentCoreV1 {
    schema: String,
    transaction_id: String,
    operation_object_digest: String,
    semantic_payload_digest: String,
    target_identity: String,
    managed_root: String,
    source_sha256_before: String,
    candidate_sha256: String,
    bytes_before: u64,
    bytes_after: u64,
    permissions_before: SourceEditPermissionV1,
    transaction_directory: String,
    descriptor_path: String,
    journal_path: String,
    stage_path: String,
    backup_path: String,
    candidate_temp_path: String,
    rollback_temp_path: String,
    descriptor: SourceEditDescriptorV1,
    created_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceEditPreStageIntentV1 {
    #[serde(flatten)]
    core: SourceEditPreStageIntentCoreV1,
    intent_digest: String,
}

/// Read-only boot/live inventory item. A recovery coordinator can bind this to
/// its outer journal and invoke the exact delete-only fallback without needing
/// the expired preview or proof payload.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SourceEditPreStageRecoveryV1 {
    pub transaction_id: String,
    pub operation_object_digest: String,
    pub intent_digest: String,
}

/// Exact restart binding for a durable stage whose outer-journal state must be
/// cross-checked by the external coordinator. It also remains discoverable
/// while a staged delete-only abort is interrupted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SourceEditStagedRecoveryV1 {
    pub transaction_id: String,
    pub operation_object_digest: String,
    pub stage_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceEditPreStageAbortReceiptCoreV1 {
    schema: String,
    pre_stage_intent: SourceEditPreStageIntentV1,
    target_bytes_observed: bool,
    target_write_performed: bool,
    coordination_state_mutated: bool,
    aborted_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SourceEditPreStageAbortReceiptV1 {
    #[serde(flatten)]
    core: SourceEditPreStageAbortReceiptCoreV1,
    pub abort_digest: String,
}

impl SourceEditPreStageAbortReceiptV1 {
    pub(crate) fn transaction_id(&self) -> &str {
        &self.core.pre_stage_intent.core.transaction_id
    }

    pub(crate) fn target_write_performed(&self) -> bool {
        self.core.target_write_performed
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceEditPreStageAbortCompletionCoreV1 {
    schema: String,
    transaction_id: String,
    operation_object_digest: String,
    intent_digest: String,
    abort_digest: String,
    target_bytes_observed: bool,
    target_write_performed: bool,
    coordination_state_mutated: bool,
    completed_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceEditPreStageAbortCompletionV1 {
    #[serde(flatten)]
    core: SourceEditPreStageAbortCompletionCoreV1,
    completion_digest: String,
}

/// Fully bound in-memory proposal. It is safe to move into the per-brain actor;
/// no authority decision is made here.
#[derive(Clone, Debug)]
pub(crate) struct PreparedSourceEditCommitV1 {
    intent: SourceEditCommitIntentV1,
    context: SourceEditPreparedContextV1,
    preview: EditPreviewState,
    source_bytes: Vec<u8>,
    candidate_bytes: Vec<u8>,
    managed_root: PathBuf,
    target_permissions: SourceEditPermissionV1,
    target_identity: SourceEditFileIdentityV1,
    transaction_id: String,
    transaction_root: PathBuf,
}

impl PreparedSourceEditCommitV1 {
    pub(crate) fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    pub(crate) fn stage(
        self,
        state: &mut SessionState,
    ) -> Result<SourceEditStagedCommitV1, SourceEditTransactionError> {
        self.stage_with_faults(state, &NoSourceEditFaults)
    }

    fn stage_with_faults<F: SourceEditFaults>(
        self,
        state: &mut SessionState,
        faults: &F,
    ) -> Result<SourceEditStagedCommitV1, SourceEditTransactionError> {
        create_parent_directories(&self.transaction_root)?;
        let directory = self.transaction_root.join(&self.transaction_id);
        let intent_path = pre_stage_intent_path(&self.transaction_root, &self.transaction_id);
        if managed_entry_exists(&intent_path)? {
            let pre_stage_intent: SourceEditPreStageIntentV1 =
                read_json(&intent_path, "pre_stage_intent_replay_read")?;
            validate_pre_stage_intent_for_prepared(&pre_stage_intent, &self)?;
            let path = stage_path(&directory);
            if managed_entry_exists(&path)? {
                if managed_entry_exists(&stage_abort_receipt_path(&directory))? {
                    return Err(SourceEditTransactionError::RecoveryRequired {
                        transaction_id: self.transaction_id,
                        detail: "staged delete-only abort is already in progress".to_string(),
                    });
                }
                let staged: SourceEditStagedCommitV1 = read_json(&path, "stage_replay_read")?;
                validate_stage(&staged)?;
                if staged.core.operation_object_digest != self.context.operation_object_digest
                    || staged.core.pre_stage_intent_digest != pre_stage_intent.intent_digest
                {
                    return Err(SourceEditTransactionError::ContextBinding(
                        "existing stage belongs to a different operation or pre-stage intent"
                            .to_string(),
                    ));
                }
                let descriptor: SourceEditDescriptorV1 =
                    read_json(&descriptor_path(&directory), "stage_replay_descriptor_read")?;
                validate_stage_descriptor_binding(&directory, &staged, &descriptor)?;
                let current =
                    read_target_snapshot(&self.managed_root, Path::new(&self.preview.file_path))?;
                if current.sha256 != self.intent.semantic_payload.expected_target_sha256
                    && current.sha256 != self.intent.semantic_payload.candidate_sha256
                {
                    return Err(SourceEditTransactionError::RecoveryRequired {
                        transaction_id: self.transaction_id,
                        detail: "existing stage found target at neither sealed digest".to_string(),
                    });
                }
                return Ok(staged);
            }
        } else if managed_entry_exists(&directory)? {
            return Err(SourceEditTransactionError::RecoveryRequired {
                transaction_id: self.transaction_id,
                detail: "transaction directory exists without its first-write pre-stage intent"
                    .to_string(),
            });
        }
        if state.read_only {
            return Err(SourceEditTransactionError::Preflight(
                "the selected brain session became read-only".to_string(),
            ));
        }
        if state.graph_generation != self.intent.semantic_payload.expected_graph_generation {
            return Err(SourceEditTransactionError::OccConflict(
                "graph generation changed after typed preflight".to_string(),
            ));
        }
        let live_preview = state
            .edit_previews
            .get(&self.preview.preview_id)
            .ok_or_else(|| {
                SourceEditTransactionError::OccConflict(
                    "preview disappeared after typed preflight".to_string(),
                )
            })?;
        if live_preview.agent_id != self.context.authority_subject_id
            || live_preview.source_sha256 != self.preview.source_sha256
            || live_preview.candidate_sha256 != self.preview.candidate_sha256
            || live_preview.new_content.as_bytes() != self.candidate_bytes
        {
            return Err(SourceEditTransactionError::OccConflict(
                "preview bindings changed after typed preflight".to_string(),
            ));
        }
        let current = read_target_snapshot(&self.managed_root, Path::new(&self.preview.file_path))?;
        if current.sha256 != self.intent.semantic_payload.expected_target_sha256
            || current.identity != self.target_identity
            || current.permissions != self.target_permissions
            || current.bytes != self.source_bytes
        {
            return Err(SourceEditTransactionError::OccConflict(
                "target bytes, identity, or permissions changed after typed preflight".to_string(),
            ));
        }
        let mark = state
            .validated_proof_ready_mark(
                &self.context.authority_subject_id,
                &self.intent.semantic_payload.target_identity,
            )
            .map_err(SourceEditTransactionError::Proof)?;
        if canonical_digest(SOURCE_EDIT_PROOF_MARK_DIGEST_DOMAIN, &mark)?
            != self.intent.semantic_payload.proof_mark_digest
        {
            return Err(SourceEditTransactionError::Proof(
                "proof mark changed after typed preflight".to_string(),
            ));
        }
        self.stage_artifacts(state, faults)
    }

    fn stage_artifacts<F: SourceEditFaults>(
        self,
        state: &mut SessionState,
        faults: &F,
    ) -> Result<SourceEditStagedCommitV1, SourceEditTransactionError> {
        create_parent_directories(&self.transaction_root)?;
        let directory = self.transaction_root.join(&self.transaction_id);
        let pre_stage_intent = load_or_create_pre_stage_intent(&self, faults)?;
        let descriptor = pre_stage_intent.core.descriptor.clone();
        if !managed_entry_exists(&directory)? {
            create_private_directory(&directory)?;
            faults.hit(
                &self.transaction_id,
                SourceEditFailpointV1::TransactionDirectoryDurable,
            )?;
        } else {
            validate_private_directory(&directory)?;
        }
        refuse_pre_stage_abort_in_progress(&self.transaction_root, &self.transaction_id)?;

        ensure_exact_descriptor(&directory, &descriptor)?;
        faults.hit(
            &self.transaction_id,
            SourceEditFailpointV1::DescriptorDurable,
        )?;
        ensure_staging_journal_phase(
            &directory,
            &self.transaction_id,
            SourceEditJournalPhaseV1::Prepared,
            &descriptor.core.source_sha256_before,
        )?;
        faults.hit(&self.transaction_id, SourceEditFailpointV1::PreparedJournal)?;

        ensure_exact_backup(&descriptor, &self.source_bytes)?;
        faults.hit(
            &self.transaction_id,
            SourceEditFailpointV1::BackupFileDurable,
        )?;
        ensure_staging_journal_phase(
            &directory,
            &self.transaction_id,
            SourceEditJournalPhaseV1::BackupDurable,
            &descriptor.core.source_sha256_before,
        )?;
        faults.hit(&self.transaction_id, SourceEditFailpointV1::BackupDurable)?;

        ensure_exact_candidate(&self.managed_root, &descriptor, &self.candidate_bytes)?;
        faults.hit(
            &self.transaction_id,
            SourceEditFailpointV1::CandidateFileDurable,
        )?;
        ensure_staging_journal_phase(
            &directory,
            &self.transaction_id,
            SourceEditJournalPhaseV1::CandidateDurable,
            &descriptor.core.source_sha256_before,
        )?;
        faults.hit(
            &self.transaction_id,
            SourceEditFailpointV1::CandidateDurable,
        )?;

        let candidate = read_target_snapshot(
            &self.managed_root,
            Path::new(&descriptor.core.candidate_temp_path),
        )?;
        if candidate.sha256 != descriptor.core.candidate_sha256
            || candidate.permissions != descriptor.core.permissions_before
        {
            return Err(SourceEditTransactionError::OccConflict(
                "staged candidate digest or permissions differ from the sealed plan".to_string(),
            ));
        }
        let current = read_target_snapshot(
            &self.managed_root,
            Path::new(&descriptor.core.target_identity),
        )?;
        if current.sha256 != descriptor.core.source_sha256_before
            || current.identity != descriptor.core.file_identity_before
            || current.permissions != descriptor.core.permissions_before
        {
            return Err(SourceEditTransactionError::OccConflict(
                "target changed while candidate was staged".to_string(),
            ));
        }
        if state.graph_generation != descriptor.core.graph_generation_prepared {
            return Err(SourceEditTransactionError::OccConflict(
                "graph generation changed while candidate was staged".to_string(),
            ));
        }
        let staged_event = ensure_staging_journal_phase(
            &directory,
            &self.transaction_id,
            SourceEditJournalPhaseV1::Staged,
            &descriptor.core.source_sha256_before,
        )?;
        faults.hit(&self.transaction_id, SourceEditFailpointV1::StagedJournal)?;
        let staged = seal_stage(SourceEditStagedCommitCoreV1 {
            schema: SOURCE_EDIT_STAGE_SCHEMA.to_string(),
            transaction_id: descriptor.core.transaction_id.clone(),
            operation_object_digest: descriptor.core.operation_object_digest.clone(),
            semantic_payload_digest: descriptor.core.semantic_payload_digest.clone(),
            pre_stage_intent_digest: pre_stage_intent.intent_digest,
            target_identity: descriptor.core.target_identity.clone(),
            source_sha256_before: descriptor.core.source_sha256_before.clone(),
            candidate_sha256: descriptor.core.candidate_sha256.clone(),
            descriptor_digest: descriptor.descriptor_digest.clone(),
            journal_root_digest: staged_event.event_digest,
            graph_generation_prepared: descriptor.core.graph_generation_prepared,
            assurance_limitations: assurance_limitations(),
            staged_at_ms: now_ms()?,
        })?;
        durable_json_new(&stage_path(&directory), &staged)?;
        faults.hit(&self.transaction_id, SourceEditFailpointV1::StageDurable)?;

        // This is the load-bearing pre-COMMIT invariant: staging is durable,
        // yet the managed target remains byte-, identity-, and mode-exact.
        let unchanged = read_target_snapshot(
            &self.managed_root,
            Path::new(&descriptor.core.target_identity),
        )?;
        if unchanged.sha256 != descriptor.core.source_sha256_before
            || unchanged.identity != descriptor.core.file_identity_before
            || unchanged.permissions != descriptor.core.permissions_before
        {
            return Err(SourceEditTransactionError::OccConflict(
                "target changed while the durable stage was sealed".to_string(),
            ));
        }
        Ok(staged)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SourceEditTransactionError {
    InvalidRequest(String),
    Preflight(String),
    Proof(String),
    OccConflict(String),
    ContextBinding(String),
    Io {
        phase: &'static str,
        detail: String,
    },
    RecoveryRequired {
        transaction_id: String,
        detail: String,
    },
    InjectedCrash {
        transaction_id: String,
        phase: &'static str,
    },
    ManualRecovery {
        transaction_id: String,
        detail: String,
    },
}

impl fmt::Display for SourceEditTransactionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(detail) => {
                write!(formatter, "invalid source edit request: {detail}")
            }
            Self::Preflight(detail) => write!(formatter, "source edit preflight refused: {detail}"),
            Self::Proof(detail) => write!(formatter, "source edit proof refused: {detail}"),
            Self::OccConflict(detail) => write!(formatter, "source edit OCC conflict: {detail}"),
            Self::ContextBinding(detail) => {
                write!(
                    formatter,
                    "source edit authority context mismatch: {detail}"
                )
            }
            Self::Io { phase, detail } => write!(formatter, "source edit I/O at {phase}: {detail}"),
            Self::RecoveryRequired {
                transaction_id,
                detail,
            } => write!(
                formatter,
                "source edit transaction {transaction_id} requires recovery: {detail}"
            ),
            Self::InjectedCrash {
                transaction_id,
                phase,
            } => write!(
                formatter,
                "simulated source edit crash for {transaction_id} at {phase}"
            ),
            Self::ManualRecovery {
                transaction_id,
                detail,
            } => write!(
                formatter,
                "source edit transaction {transaction_id} requires manual recovery: {detail}"
            ),
        }
    }
}

impl std::error::Error for SourceEditTransactionError {}

pub(crate) struct SourceEditCommitAdapterV1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceEditFailpointV1 {
    PreStageIntentDurable,
    TransactionDirectoryDurable,
    DescriptorDurable,
    PreparedJournal,
    BackupFileDurable,
    BackupDurable,
    CandidateFileDurable,
    CandidateDurable,
    StagedJournal,
    StageDurable,
    PublishIntent,
    AtomicRename,
    PublishedJournal,
    OutcomeDurable,
    RollbackIntent,
    RollbackRename,
    Finalize,
    AbortMarkerDurable,
    AbortCandidateRemoved,
    AbortRollbackRemoved,
    AbortBackupRemoved,
    AbortStageRemoved,
    AbortJournalRemoved,
    AbortDescriptorRemoved,
    AbortCompletionDurable,
    PreStageAbortMarkerDurable,
    PreStageAbortCandidateRemoved,
    PreStageAbortBackupRemoved,
    PreStageAbortJournalRemoved,
    PreStageAbortDescriptorRemoved,
    PreStageAbortDirectoryRemoved,
    PreStageAbortIntentRemoved,
    PreStageAbortCompletionDurable,
}

impl SourceEditFailpointV1 {
    const fn name(self) -> &'static str {
        match self {
            Self::PreStageIntentDurable => "pre_stage_intent_durable",
            Self::TransactionDirectoryDurable => "transaction_directory_durable",
            Self::DescriptorDurable => "descriptor_durable",
            Self::PreparedJournal => "prepared_journal",
            Self::BackupFileDurable => "backup_file_durable",
            Self::BackupDurable => "backup_durable",
            Self::CandidateFileDurable => "candidate_file_durable",
            Self::CandidateDurable => "candidate_durable",
            Self::StagedJournal => "staged_journal",
            Self::StageDurable => "stage_durable",
            Self::PublishIntent => "publish_intent",
            Self::AtomicRename => "atomic_rename",
            Self::PublishedJournal => "published_journal",
            Self::OutcomeDurable => "outcome_durable",
            Self::RollbackIntent => "rollback_intent",
            Self::RollbackRename => "rollback_rename",
            Self::Finalize => "finalize",
            Self::AbortMarkerDurable => "abort_marker_durable",
            Self::AbortCandidateRemoved => "abort_candidate_removed",
            Self::AbortRollbackRemoved => "abort_rollback_removed",
            Self::AbortBackupRemoved => "abort_backup_removed",
            Self::AbortStageRemoved => "abort_stage_removed",
            Self::AbortJournalRemoved => "abort_journal_removed",
            Self::AbortDescriptorRemoved => "abort_descriptor_removed",
            Self::AbortCompletionDurable => "abort_completion_durable",
            Self::PreStageAbortMarkerDurable => "pre_stage_abort_marker_durable",
            Self::PreStageAbortCandidateRemoved => "pre_stage_abort_candidate_removed",
            Self::PreStageAbortBackupRemoved => "pre_stage_abort_backup_removed",
            Self::PreStageAbortJournalRemoved => "pre_stage_abort_journal_removed",
            Self::PreStageAbortDescriptorRemoved => "pre_stage_abort_descriptor_removed",
            Self::PreStageAbortDirectoryRemoved => "pre_stage_abort_directory_removed",
            Self::PreStageAbortIntentRemoved => "pre_stage_abort_intent_removed",
            Self::PreStageAbortCompletionDurable => "pre_stage_abort_completion_durable",
        }
    }
}

trait SourceEditFaults {
    fn hit(
        &self,
        transaction_id: &str,
        point: SourceEditFailpointV1,
    ) -> Result<(), SourceEditTransactionError>;
}

struct NoSourceEditFaults;

impl SourceEditFaults for NoSourceEditFaults {
    fn hit(
        &self,
        _transaction_id: &str,
        _point: SourceEditFailpointV1,
    ) -> Result<(), SourceEditTransactionError> {
        Ok(())
    }
}

#[cfg(test)]
struct SourceEditTestCrashAt(SourceEditFailpointV1);

#[cfg(test)]
impl SourceEditFaults for SourceEditTestCrashAt {
    fn hit(
        &self,
        transaction_id: &str,
        point: SourceEditFailpointV1,
    ) -> Result<(), SourceEditTransactionError> {
        if point == self.0 {
            return Err(SourceEditTransactionError::InjectedCrash {
                transaction_id: transaction_id.to_string(),
                phase: point.name(),
            });
        }
        Ok(())
    }
}

impl SourceEditCommitAdapterV1 {
    pub(crate) fn expected_effects() -> BTreeSet<Effect> {
        BTreeSet::from([
            Effect::SourceFilesystemWrite,
            Effect::RuntimeStoreWrite,
            Effect::CoordinationRecord,
        ])
    }

    /// Inspect the owner-held preview/proof without consuming either. The
    /// authority service signs the returned semantic payload and outer object.
    #[cfg(test)]
    pub(crate) fn inspect(
        session: &BrainSessionCell,
        request: &SourceEditCommitRequestV1,
        authority_subject_id: &str,
    ) -> Result<SourceEditCommitIntentV1, SourceEditTransactionError> {
        let state = session.lock();
        Self::inspect_state(&state, request, authority_subject_id)
    }

    /// Actor-safe inspection seam. Callers that already own the selected brain
    /// turn must never reacquire the raw [`BrainSessionCell`] mutex.
    pub(crate) fn inspect_state(
        state: &SessionState,
        request: &SourceEditCommitRequestV1,
        authority_subject_id: &str,
    ) -> Result<SourceEditCommitIntentV1, SourceEditTransactionError> {
        inspect_locked(state, request, authority_subject_id).map(|inspection| inspection.intent)
    }

    pub(crate) fn operation_object(
        intent: &SourceEditCommitIntentV1,
        context: &SourceEditPreparedContextV1,
    ) -> SourceEditOperationObjectV1 {
        SourceEditOperationObjectV1 {
            schema: EXTERNAL_MUTATION_OPERATION_OBJECT_SCHEMA.to_string(),
            semantic_action: context.semantic_action.clone(),
            ingress: context.ingress,
            brain_id: context.brain_id.clone(),
            mission_id: context.mission_id.clone(),
            mission_head_id: context.mission_head_id.clone(),
            operation_version: context.operation_version,
            semantic_payload_digest: intent.semantic_payload_digest.clone(),
        }
    }

    pub(crate) fn operation_object_digest(
        intent: &SourceEditCommitIntentV1,
        context: &SourceEditPreparedContextV1,
    ) -> Result<String, SourceEditTransactionError> {
        canonical_digest(
            EXTERNAL_MUTATION_OPERATION_OBJECT_DIGEST_DOMAIN,
            &Self::operation_object(intent, context),
        )
    }

    /// Reopen every mutable owner-held input and bind it to the already
    /// verified external-mutation context. No lease is consumed here.
    #[cfg(test)]
    pub(crate) fn prepare(
        session: &BrainSessionCell,
        request: &SourceEditCommitRequestV1,
        context: &SourceEditPreparedContextV1,
    ) -> Result<PreparedSourceEditCommitV1, SourceEditTransactionError> {
        let state = session.lock();
        Self::prepare_in_actor(&state, request, context)
    }

    /// Reopen and bind every mutable input while the selected brain actor owns
    /// the state. This is the production source-edit staging seam: it accepts a
    /// borrowed state and therefore cannot accidentally nest a raw session lock.
    pub(crate) fn prepare_in_actor(
        state: &SessionState,
        request: &SourceEditCommitRequestV1,
        context: &SourceEditPreparedContextV1,
    ) -> Result<PreparedSourceEditCommitV1, SourceEditTransactionError> {
        if state.read_only {
            return Err(SourceEditTransactionError::Preflight(
                "the selected brain session is read-only".to_string(),
            ));
        }
        let inspected = inspect_locked(state, request, &context.authority_subject_id)?;
        validate_context(context, &inspected.intent)?;

        let operation_object_digest = Self::operation_object_digest(&inspected.intent, context)?;
        if operation_object_digest != context.operation_object_digest {
            return Err(SourceEditTransactionError::ContextBinding(format!(
                "operation object digest differs (expected={}, recomputed={operation_object_digest})",
                context.operation_object_digest
            )));
        }

        let transaction_id = canonical_digest(
            SOURCE_EDIT_TRANSACTION_DIGEST_DOMAIN,
            &(
                SOURCE_EDIT_TRANSACTION_SCHEMA,
                context.operation_object_digest.as_str(),
                inspected.intent.semantic_payload.proof_mark_digest.as_str(),
                inspected.intent.semantic_payload.candidate_sha256.as_str(),
                context.operation_version,
            ),
        )?;
        let transaction_root = canonical_runtime_root(state)?.join(SOURCE_EDIT_TX_DIRECTORY);

        Ok(PreparedSourceEditCommitV1 {
            intent: inspected.intent,
            context: context.clone(),
            preview: inspected.preview,
            source_bytes: inspected.snapshot.bytes,
            candidate_bytes: inspected.candidate_bytes,
            managed_root: inspected.managed_root,
            target_permissions: inspected.snapshot.permissions,
            target_identity: inspected.snapshot.identity,
            transaction_id,
            transaction_root,
        })
    }

    pub(crate) fn finalize(
        state: &mut SessionState,
        outcome: &SourceEditCommitOutcomeV1,
    ) -> Result<SourceEditTerminalReceiptV1, SourceEditTransactionError> {
        finalize_outcome(state, outcome, &NoSourceEditFaults)
    }

    /// Publish only after the outer external-mutation journal is COMMITTED and
    /// the authority broker reports CONSUMED. The durable stage digest is the
    /// exact pre-publish outcome bound by that outer COMMIT.
    pub(crate) fn publish_after_commit(
        state: &mut SessionState,
        staged: &SourceEditStagedCommitV1,
    ) -> Result<SourceEditCommitOutcomeV1, SourceEditTransactionError> {
        publish_after_commit(state, staged, &NoSourceEditFaults)
    }

    /// Callback-safe validation for the authority linearization point. This
    /// performs no write and specifically proves the target is still the old
    /// sealed file while the candidate remains durable and exact.
    pub(crate) fn revalidate_stage_before_commit(
        state: &SessionState,
        staged: &SourceEditStagedCommitV1,
    ) -> Result<(), SourceEditTransactionError> {
        revalidate_stage_before_commit(state, staged)
    }

    /// Restart-safe forward completion for COMMITTED-not-PUBLISHED outer
    /// operations. It loads the exact durable stage by id and refuses any
    /// operation/stage digest mismatch before touching the target.
    pub(crate) fn forward_complete_committed(
        state: &mut SessionState,
        transaction_id: &str,
        operation_object_digest: &str,
        stage_digest: &str,
    ) -> Result<SourceEditCommitOutcomeV1, SourceEditTransactionError> {
        let staged = Self::validate_committed_recovery_binding(
            state,
            transaction_id,
            operation_object_digest,
            stage_digest,
        )?;
        publish_after_commit(state, &staged, &NoSourceEditFaults)
    }

    /// Read-only validation performed before the outer broker advances from a
    /// durable COMMIT witness to CONSUMED. It binds all three recovery selectors
    /// to the exact descriptor and stage under this runtime root, so a forged
    /// transaction id cannot spend authority before the adapter refuses it.
    pub(crate) fn validate_committed_recovery_binding(
        state: &SessionState,
        transaction_id: &str,
        operation_object_digest: &str,
        stage_digest: &str,
    ) -> Result<SourceEditStagedCommitV1, SourceEditTransactionError> {
        let (directory, descriptor) = load_descriptor(state, transaction_id)?;
        if descriptor.core.operation_object_digest != operation_object_digest {
            return Err(SourceEditTransactionError::ContextBinding(
                "forward recovery operation digest differs from the sealed descriptor".to_string(),
            ));
        }
        let staged: SourceEditStagedCommitV1 =
            read_json(&stage_path(&directory), "stage_recovery_read")?;
        validate_stage(&staged)?;
        if staged.stage_digest != stage_digest
            || staged.core.operation_object_digest != operation_object_digest
            || staged.core.transaction_id != transaction_id
        {
            return Err(SourceEditTransactionError::ContextBinding(
                "forward recovery stage binding differs from outer COMMIT".to_string(),
            ));
        }
        Ok(staged)
    }

    /// Read-only boot/live inventory for attempts that did not reach a durable
    /// `stage.json`. The returned binding is sufficient for exact autonomous
    /// cleanup even when preview/proof memory was lost on restart.
    pub(crate) fn pending_pre_stage_recovery(
        state: &SessionState,
    ) -> Result<BTreeMap<String, SourceEditPreStageRecoveryV1>, SourceEditTransactionError> {
        pending_pre_stage_recovery(state)
    }

    /// Exact inventory for `stage.json`-durable attempts and interrupted staged
    /// aborts. The outer coordinator decides forward-complete versus abort from
    /// its own journal; this layer supplies the sealed stage digest required by
    /// either action.
    pub(crate) fn pending_staged_recovery(
        state: &SessionState,
    ) -> Result<BTreeMap<String, SourceEditStagedRecoveryV1>, SourceEditTransactionError> {
        pending_staged_recovery(state)
    }

    /// Delete-only fallback for a pre-stage orphan. This API has an immutable
    /// session reference, never opens or stats the managed target, and cannot
    /// consume preview/proof/permit coordination state.
    pub(crate) fn abort_pre_stage_without_target_write(
        state: &SessionState,
        transaction_id: &str,
        operation_object_digest: &str,
        intent_digest: &str,
    ) -> Result<SourceEditPreStageAbortReceiptV1, SourceEditTransactionError> {
        abort_pre_stage_without_target_write(
            state,
            transaction_id,
            operation_object_digest,
            intent_digest,
            &NoSourceEditFaults,
        )
    }

    /// Discard a PREPARED stage only when no outer COMMIT exists. This API is
    /// deliberately incapable of mutating session coordination state and never
    /// opens, stats, hashes, requires, or rewrites the managed target path.
    pub(crate) fn abort_staged_without_target_write(
        state: &SessionState,
        transaction_id: &str,
        operation_object_digest: &str,
        stage_digest: &str,
    ) -> Result<SourceEditStageAbortReceiptV1, SourceEditTransactionError> {
        abort_staged_without_target_write(
            state,
            transaction_id,
            operation_object_digest,
            stage_digest,
        )
    }

    pub(crate) fn rollback(
        state: &mut SessionState,
        outcome: &SourceEditCommitOutcomeV1,
    ) -> Result<SourceEditTerminalReceiptV1, SourceEditTransactionError> {
        rollback_outcome(state, outcome, &NoSourceEditFaults)
    }

    /// Recover one exact journal after restart. Recovery never reapplies the
    /// candidate: an old target stays old; a new target is either finalized in
    /// place or restored from the durable backup.
    pub(crate) fn recover_transaction(
        state: &mut SessionState,
        transaction_id: &str,
        operation_object_digest: &str,
        decision: SourceEditRecoveryDecisionV1,
    ) -> Result<SourceEditTerminalReceiptV1, SourceEditTransactionError> {
        recover_transaction(state, transaction_id, operation_object_digest, decision)
    }

    /// Read-only inventory for a restart coordinator. Terminal journals are
    /// omitted; returned values are transaction id -> operation digest.
    pub(crate) fn pending_recovery(
        state: &SessionState,
    ) -> Result<BTreeMap<String, String>, SourceEditTransactionError> {
        pending_recovery(state)
    }

    /// Service-level recovery tests need to exercise the real durable
    /// pre-stage inventory, not a fabricated directory. This deliberately
    /// stops after the candidate file is durable but before `stage.json`.
    #[cfg(test)]
    pub(crate) fn leave_pre_stage_orphan_for_test(
        prepared: PreparedSourceEditCommitV1,
        state: &mut SessionState,
    ) -> Result<(), SourceEditTransactionError> {
        match prepared.stage_with_faults(
            state,
            &SourceEditTestCrashAt(SourceEditFailpointV1::CandidateFileDurable),
        ) {
            Err(SourceEditTransactionError::InjectedCrash { .. }) => Ok(()),
            Err(error) => Err(error),
            Ok(_) => Err(SourceEditTransactionError::Preflight(
                "pre-stage test cut was not reached".to_string(),
            )),
        }
    }

    /// Leave an authentic, receipt-bearing interrupted delete-only cleanup so
    /// a fresh service boot must rediscover and finish it idempotently.
    #[cfg(test)]
    pub(crate) fn interrupt_pre_stage_cleanup_for_test(
        state: &SessionState,
        transaction_id: &str,
        operation_object_digest: &str,
        intent_digest: &str,
    ) -> Result<(), SourceEditTransactionError> {
        match abort_pre_stage_without_target_write(
            state,
            transaction_id,
            operation_object_digest,
            intent_digest,
            &SourceEditTestCrashAt(SourceEditFailpointV1::PreStageAbortCandidateRemoved),
        ) {
            Err(SourceEditTransactionError::InjectedCrash { .. }) => Ok(()),
            Err(error) => Err(error),
            Ok(_) => Err(SourceEditTransactionError::Preflight(
                "pre-stage cleanup test cut was not reached".to_string(),
            )),
        }
    }
}

struct InspectedSourceEditV1 {
    intent: SourceEditCommitIntentV1,
    preview: EditPreviewState,
    snapshot: SourceEditTargetSnapshotV1,
    candidate_bytes: Vec<u8>,
    managed_root: PathBuf,
}

#[derive(Serialize)]
struct SourceEditProofScopeV1<'a> {
    schema: &'static str,
    authority_subject_id: &'a str,
    target_identity: &'a str,
    graph_generation: u64,
}

fn inspect_locked(
    state: &SessionState,
    request: &SourceEditCommitRequestV1,
    authority_subject_id: &str,
) -> Result<InspectedSourceEditV1, SourceEditTransactionError> {
    validate_request(request, authority_subject_id)?;
    let now = now_ms()?;
    let preview = state
        .edit_previews
        .get(&request.preview_id)
        .cloned()
        .ok_or_else(|| {
            SourceEditTransactionError::Preflight(format!(
                "preview '{}' does not exist in this brain session",
                request.preview_id
            ))
        })?;
    if preview.agent_id != authority_subject_id {
        return Err(SourceEditTransactionError::ContextBinding(
            "authority subject is not the owner of the preview".to_string(),
        ));
    }
    if preview.created_at_ms > now.saturating_add(30_000)
        || now.saturating_sub(preview.created_at_ms) >= PREVIEW_TTL_MS
    {
        return Err(SourceEditTransactionError::Preflight(
            "preview is expired or has an invalid future timestamp".to_string(),
        ));
    }

    let (managed_root, canonical_target) = managed_target(state, Path::new(&preview.file_path))?;
    let snapshot = read_target_snapshot(&managed_root, &canonical_target)?;
    if snapshot.bytes.len() > MAX_SOURCE_EDIT_BYTES {
        return Err(SourceEditTransactionError::Preflight(format!(
            "source is larger than the {MAX_SOURCE_EDIT_BYTES}-byte typed edit limit"
        )));
    }
    let candidate_bytes = preview.new_content.as_bytes().to_vec();
    if candidate_bytes.len() > MAX_SOURCE_EDIT_BYTES {
        return Err(SourceEditTransactionError::Preflight(format!(
            "candidate is larger than the {MAX_SOURCE_EDIT_BYTES}-byte typed edit limit"
        )));
    }
    let candidate_sha256 = sha256_hex(&candidate_bytes);
    if snapshot.sha256 != preview.source_sha256 {
        return Err(SourceEditTransactionError::OccConflict(format!(
            "preview source SHA-256 changed (preview={}, current={})",
            preview.source_sha256, snapshot.sha256
        )));
    }
    if candidate_sha256 != preview.candidate_sha256 {
        return Err(SourceEditTransactionError::OccConflict(
            "preview candidate bytes no longer match its SHA-256 binding".to_string(),
        ));
    }
    if snapshot.bytes == candidate_bytes {
        return Err(SourceEditTransactionError::Preflight(
            "candidate equals the current source bytes".to_string(),
        ));
    }

    let target_text = path_text(&canonical_target);
    let proof_mark = state
        .validated_proof_ready_mark(authority_subject_id, &target_text)
        .map_err(SourceEditTransactionError::Proof)?;
    if proof_mark.target_identity != target_text {
        return Err(SourceEditTransactionError::Proof(
            "proof mark target identity differs from the canonical preview target".to_string(),
        ));
    }
    if proof_mark.graph_generation != state.graph_generation {
        return Err(SourceEditTransactionError::Proof(
            "proof mark graph generation is stale".to_string(),
        ));
    }
    if digest_without_prefix(&proof_mark.target_digest)? != snapshot.sha256 {
        return Err(SourceEditTransactionError::Proof(
            "proof mark target digest differs from current target bytes".to_string(),
        ));
    }

    let proof_scope_digest = canonical_digest(
        SOURCE_EDIT_PROOF_SCOPE_DIGEST_DOMAIN,
        &SourceEditProofScopeV1 {
            schema: SOURCE_EDIT_PROOF_SCOPE_SCHEMA,
            authority_subject_id,
            target_identity: &target_text,
            graph_generation: state.graph_generation,
        },
    )?;
    let proof_mark_digest = canonical_digest(SOURCE_EDIT_PROOF_MARK_DIGEST_DOMAIN, &proof_mark)?;
    let semantic_payload = SourceEditCommitSemanticPayloadV1 {
        schema: SOURCE_EDIT_COMMIT_SEMANTIC_PAYLOAD_SCHEMA.to_string(),
        preview_id: request.preview_id.clone(),
        target_identity: target_text,
        expected_target_sha256: snapshot.sha256.clone(),
        candidate_sha256,
        expected_graph_generation: state.graph_generation,
        proof_scope_digest,
        proof_mark_digest,
    };
    let semantic_payload_digest = canonical_digest(
        SOURCE_EDIT_COMMIT_SEMANTIC_PAYLOAD_DIGEST_DOMAIN,
        &semantic_payload,
    )?;
    Ok(InspectedSourceEditV1 {
        intent: SourceEditCommitIntentV1 {
            semantic_payload,
            semantic_payload_digest,
            proof_expires_at_ms: proof_mark.expires_at_ms,
            preview_created_at_ms: preview.created_at_ms,
        },
        preview,
        snapshot,
        candidate_bytes,
        managed_root,
    })
}

fn validate_request(
    request: &SourceEditCommitRequestV1,
    authority_subject_id: &str,
) -> Result<(), SourceEditTransactionError> {
    if request.schema != SOURCE_EDIT_COMMIT_REQUEST_SCHEMA {
        return Err(SourceEditTransactionError::InvalidRequest(format!(
            "schema must be '{SOURCE_EDIT_COMMIT_REQUEST_SCHEMA}'"
        )));
    }
    if request.preview_id.trim().is_empty() || request.preview_id.len() > 256 {
        return Err(SourceEditTransactionError::InvalidRequest(
            "preview_id must contain 1..=256 bytes".to_string(),
        ));
    }
    if authority_subject_id.trim().is_empty() || authority_subject_id.len() > 256 {
        return Err(SourceEditTransactionError::ContextBinding(
            "authority subject id must contain 1..=256 bytes".to_string(),
        ));
    }
    Ok(())
}

fn validate_context(
    context: &SourceEditPreparedContextV1,
    intent: &SourceEditCommitIntentV1,
) -> Result<(), SourceEditTransactionError> {
    if context.semantic_action != SOURCE_EDIT_COMMIT_ACTION {
        return Err(SourceEditTransactionError::ContextBinding(format!(
            "semantic_action must be '{SOURCE_EDIT_COMMIT_ACTION}'"
        )));
    }
    if context.ingress != Ingress::Mcp {
        return Err(SourceEditTransactionError::ContextBinding(
            "source.edit.commit typed consumer currently accepts MCP ingress only".to_string(),
        ));
    }
    if context.operation_version != SOURCE_EDIT_OPERATION_VERSION {
        return Err(SourceEditTransactionError::ContextBinding(format!(
            "operation_version must be {SOURCE_EDIT_OPERATION_VERSION}"
        )));
    }
    if context.expected_effects != SourceEditCommitAdapterV1::expected_effects() {
        return Err(SourceEditTransactionError::ContextBinding(
            "complete effects differ from the source.edit.commit catalog tuple".to_string(),
        ));
    }
    if context.semantic_payload_digest != intent.semantic_payload_digest {
        return Err(SourceEditTransactionError::ContextBinding(format!(
            "semantic payload digest differs (expected={}, current={})",
            context.semantic_payload_digest, intent.semantic_payload_digest
        )));
    }
    for (field, value) in [
        (
            "semantic_payload_digest",
            context.semantic_payload_digest.as_str(),
        ),
        (
            "operation_object_digest",
            context.operation_object_digest.as_str(),
        ),
    ] {
        if !is_digest(value) {
            return Err(SourceEditTransactionError::ContextBinding(format!(
                "{field} is not a canonical lowercase SHA-256 digest"
            )));
        }
    }
    if context.brain_id.trim().is_empty() || context.brain_id.len() > 512 {
        return Err(SourceEditTransactionError::ContextBinding(
            "brain_id must contain 1..=512 bytes".to_string(),
        ));
    }
    Ok(())
}

fn canonical_digest<T: Serialize + ?Sized>(
    domain: &str,
    value: &T,
) -> Result<String, SourceEditTransactionError> {
    digest_canonical(domain, value).map_err(|error| {
        SourceEditTransactionError::Preflight(format!(
            "canonical digest failed for domain '{domain}': {error}"
        ))
    })
}

fn now_ms() -> Result<u64, SourceEditTransactionError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .map_err(|error| SourceEditTransactionError::Preflight(error.to_string()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn digest_without_prefix(value: &str) -> Result<String, SourceEditTransactionError> {
    let digest = value.strip_prefix("sha256:").unwrap_or(value);
    if !is_digest(digest) {
        return Err(SourceEditTransactionError::Proof(
            "proof target digest is not canonical SHA-256".to_string(),
        ));
    }
    Ok(digest.to_string())
}

fn path_text(path: &Path) -> String {
    crate::scope::normalize_path_text(&path.to_string_lossy())
}

fn canonical_runtime_root(state: &SessionState) -> Result<PathBuf, SourceEditTransactionError> {
    let raw_metadata = fs::symlink_metadata(&state.runtime_root)
        .map_err(|error| io_error("runtime_root_lstat", error))?;
    if raw_metadata.file_type().is_symlink() || !raw_metadata.is_dir() {
        return Err(SourceEditTransactionError::Preflight(
            "owner runtime root must not be a symlink".to_string(),
        ));
    }
    let root = fs::canonicalize(&state.runtime_root).map_err(|error| {
        SourceEditTransactionError::Preflight(format!(
            "cannot canonicalize owner runtime root '{}': {error}",
            state.runtime_root.display()
        ))
    })?;
    let metadata = fs::symlink_metadata(&root).map_err(|error| io_error("runtime_root", error))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(SourceEditTransactionError::Preflight(
            "owner runtime root must be a real directory".to_string(),
        ));
    }
    Ok(root)
}

fn managed_target(
    state: &SessionState,
    raw_target: &Path,
) -> Result<(PathBuf, PathBuf), SourceEditTransactionError> {
    if !raw_target.is_absolute() {
        return Err(SourceEditTransactionError::Preflight(
            "preview target must already be an absolute owner-resolved path".to_string(),
        ));
    }
    let raw_metadata =
        fs::symlink_metadata(raw_target).map_err(|error| io_error("target_lstat", error))?;
    if raw_metadata.file_type().is_symlink() {
        return Err(SourceEditTransactionError::Preflight(
            "preview target is a symlink".to_string(),
        ));
    }
    let target =
        fs::canonicalize(raw_target).map_err(|error| io_error("target_canonical", error))?;
    let mut roots = state
        .ingest_roots
        .iter()
        .filter_map(|raw| fs::canonicalize(raw).ok())
        .filter(|root| root.is_dir() && target.starts_with(root))
        .collect::<Vec<_>>();
    roots.sort_by_key(|root| std::cmp::Reverse(root.components().count()));
    let managed_root = roots.into_iter().next().ok_or_else(|| {
        SourceEditTransactionError::Preflight(format!(
            "target '{}' is outside every bound managed root",
            target.display()
        ))
    })?;
    validate_no_symlink_components(&managed_root, &target)?;
    Ok((managed_root, target))
}

fn validate_no_symlink_components(
    root: &Path,
    target: &Path,
) -> Result<(), SourceEditTransactionError> {
    let relative = target.strip_prefix(root).map_err(|_| {
        SourceEditTransactionError::Preflight("target escapes managed root".to_string())
    })?;
    let mut cursor = root.to_path_buf();
    for component in relative.components() {
        match component {
            Component::Normal(value) => cursor.push(value),
            _ => {
                return Err(SourceEditTransactionError::Preflight(
                    "target contains a non-normal path component".to_string(),
                ))
            }
        }
        let metadata = fs::symlink_metadata(&cursor)
            .map_err(|error| io_error("path_component_lstat", error))?;
        if metadata.file_type().is_symlink() {
            return Err(SourceEditTransactionError::Preflight(format!(
                "symlink component refused: {}",
                cursor.display()
            )));
        }
    }
    Ok(())
}

fn read_target_snapshot(
    managed_root: &Path,
    target: &Path,
) -> Result<SourceEditTargetSnapshotV1, SourceEditTransactionError> {
    validate_no_symlink_components(managed_root, target)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut file = options
        .open(target)
        .map_err(|error| io_error("target_open_nofollow", error))?;
    let before = file
        .metadata()
        .map_err(|error| io_error("target_fstat_before", error))?;
    if !before.is_file() {
        return Err(SourceEditTransactionError::Preflight(
            "source edit target must be a regular file".to_string(),
        ));
    }
    #[cfg(unix)]
    {
        if before.nlink() != 1 {
            return Err(SourceEditTransactionError::Preflight(
                "hard-linked source targets are refused because atomic replacement would break alias conservation"
                    .to_string(),
            ));
        }
        let effective_uid = unsafe { libc::geteuid() } as u32;
        if before.uid() != effective_uid {
            return Err(SourceEditTransactionError::Preflight(
                "source target is not owned by the served owner's effective UID".to_string(),
            ));
        }
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| io_error("target_read", error))?;
    let after = file
        .metadata()
        .map_err(|error| io_error("target_fstat_after", error))?;
    let path_metadata =
        fs::symlink_metadata(target).map_err(|error| io_error("target_lstat_after", error))?;
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
        return Err(SourceEditTransactionError::OccConflict(
            "target path type changed while it was read".to_string(),
        ));
    }
    let identity_before = file_identity(&before);
    let identity_after = file_identity(&after);
    let identity_path = file_identity(&path_metadata);
    if identity_before != identity_after || identity_before != identity_path {
        return Err(SourceEditTransactionError::OccConflict(
            "target inode identity changed while it was read".to_string(),
        ));
    }
    Ok(SourceEditTargetSnapshotV1 {
        sha256: sha256_hex(&bytes),
        permissions: permission_snapshot(&before),
        identity: identity_before,
        bytes,
    })
}

fn file_identity(metadata: &fs::Metadata) -> SourceEditFileIdentityV1 {
    #[cfg(unix)]
    {
        SourceEditFileIdentityV1 {
            dev: Some(metadata.dev()),
            ino: Some(metadata.ino()),
            link_count: Some(metadata.nlink()),
            len: metadata.len(),
        }
    }
    #[cfg(not(unix))]
    {
        SourceEditFileIdentityV1 {
            dev: None,
            ino: None,
            link_count: None,
            len: metadata.len(),
        }
    }
}

fn permission_snapshot(metadata: &fs::Metadata) -> SourceEditPermissionV1 {
    #[cfg(unix)]
    {
        SourceEditPermissionV1 {
            readonly: metadata.permissions().readonly(),
            unix_mode: Some(metadata.permissions().mode()),
        }
    }
    #[cfg(not(unix))]
    {
        SourceEditPermissionV1 {
            readonly: metadata.permissions().readonly(),
            unix_mode: None,
        }
    }
}

fn apply_permissions(
    file: &File,
    permissions: &SourceEditPermissionV1,
) -> Result<(), SourceEditTransactionError> {
    #[cfg(unix)]
    let value = fs::Permissions::from_mode(permissions.unix_mode.ok_or_else(|| {
        SourceEditTransactionError::Preflight("missing Unix permission mode".to_string())
    })?);
    #[cfg(not(unix))]
    let value = {
        let mut value = file
            .metadata()
            .map_err(|error| io_error("permission_metadata", error))?
            .permissions();
        value.set_readonly(permissions.readonly);
        value
    };
    file.set_permissions(value)
        .map_err(|error| io_error("set_permissions", error))
}

fn io_error(phase: &'static str, error: impl fmt::Display) -> SourceEditTransactionError {
    SourceEditTransactionError::Io {
        phase,
        detail: error.to_string(),
    }
}

fn assurance_limitations() -> Vec<String> {
    let mut limitations = vec![SAME_UID_TOCTOU_LIMITATION.to_string()];
    #[cfg(windows)]
    limitations.push(WINDOWS_DIRECTORY_FSYNC_LIMITATION.to_string());
    limitations
}

fn create_private_directory(path: &Path) -> Result<(), SourceEditTransactionError> {
    fs::create_dir(path).map_err(|error| io_error("create_transaction_directory", error))?;
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| io_error("chmod_transaction_directory", error))?;
    fsync_parent(path)
}

fn validate_private_directory(path: &Path) -> Result<(), SourceEditTransactionError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| io_error("transaction_directory_lstat", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(SourceEditTransactionError::Preflight(
            "transaction directory is not a real directory".to_string(),
        ));
    }
    #[cfg(unix)]
    if metadata.uid() != unsafe { libc::geteuid() } as u32 {
        return Err(SourceEditTransactionError::Preflight(
            "transaction directory is not owned by the served owner".to_string(),
        ));
    }
    Ok(())
}

fn create_parent_directories(path: &Path) -> Result<(), SourceEditTransactionError> {
    fs::create_dir_all(path).map_err(|error| io_error("create_transaction_root", error))?;
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("transaction_root_lstat", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(SourceEditTransactionError::Preflight(
            "transaction root is not a real directory".to_string(),
        ));
    }
    #[cfg(unix)]
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| io_error("chmod_transaction_root", error))?;
    fsync_parent(path)
}

fn create_new_synced_file(
    path: &Path,
    bytes: &[u8],
    permissions: Option<&SourceEditPermissionV1>,
) -> Result<(), SourceEditTransactionError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut file = options
        .open(path)
        .map_err(|error| io_error("create_new_file", error))?;
    file.write_all(bytes)
        .map_err(|error| io_error("write_file", error))?;
    if let Some(permissions) = permissions {
        apply_permissions(&file, permissions)?;
    }
    file.sync_all()
        .map_err(|error| io_error("fsync_file", error))?;
    fsync_parent(path)
}

fn durable_json_new<T: Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), SourceEditTransactionError> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|error| io_error("serialize_json", error))?;
    create_new_synced_file(path, &bytes, None)
}

fn fsync_parent(path: &Path) -> Result<(), SourceEditTransactionError> {
    #[cfg(unix)]
    {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| io_error("fsync_parent", error))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

fn remove_file_if_exists(path: &Path) -> Result<(), SourceEditTransactionError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_error("cleanup_lstat", error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(SourceEditTransactionError::Preflight(format!(
            "refused cleanup of non-regular managed artifact '{}'",
            path.display()
        )));
    }
    fs::remove_file(path).map_err(|error| io_error("remove_file", error))?;
    fsync_parent(path)
}

fn descriptor_path(transaction_dir: &Path) -> PathBuf {
    transaction_dir.join("descriptor.json")
}

fn pre_stage_intent_path(transaction_root: &Path, transaction_id: &str) -> PathBuf {
    transaction_root.join(format!("{PRE_STAGE_INTENT_PREFIX}{transaction_id}.json"))
}

fn pre_stage_abort_receipt_path(transaction_root: &Path, transaction_id: &str) -> PathBuf {
    transaction_root.join(format!("{PRE_STAGE_ABORT_PREFIX}{transaction_id}.json"))
}

fn pre_stage_abort_completion_path(transaction_root: &Path, transaction_id: &str) -> PathBuf {
    transaction_root.join(format!(
        "{PRE_STAGE_ABORT_COMPLETION_PREFIX}{transaction_id}.json"
    ))
}

fn journal_path(transaction_dir: &Path) -> PathBuf {
    transaction_dir.join("journal.jsonl")
}

fn stage_path(transaction_dir: &Path) -> PathBuf {
    transaction_dir.join("stage.json")
}

fn stage_abort_receipt_path(transaction_dir: &Path) -> PathBuf {
    transaction_dir.join("stage-abort-receipt.json")
}

fn stage_abort_completion_path(transaction_dir: &Path) -> PathBuf {
    transaction_dir.join("stage-abort-completion.json")
}

fn outcome_path(transaction_dir: &Path) -> PathBuf {
    transaction_dir.join("outcome.json")
}

fn terminal_receipt_path(transaction_dir: &Path) -> PathBuf {
    transaction_dir.join("terminal-receipt.json")
}

fn descriptor_core_for_prepared(
    prepared: &PreparedSourceEditCommitV1,
    created_at_ms: u64,
) -> Result<SourceEditDescriptorCoreV1, SourceEditTransactionError> {
    let directory = prepared.transaction_root.join(&prepared.transaction_id);
    let candidate_temp = Path::new(&prepared.preview.file_path)
        .parent()
        .ok_or_else(|| SourceEditTransactionError::Preflight("target has no parent".to_string()))?
        .join(format!(
            ".m1nd-source-edit-{}.candidate",
            prepared.transaction_id
        ));
    let rollback_temp = candidate_temp.with_extension("rollback");
    Ok(SourceEditDescriptorCoreV1 {
        schema: SOURCE_EDIT_DESCRIPTOR_SCHEMA.to_string(),
        transaction_id: prepared.transaction_id.clone(),
        operation_object_digest: prepared.context.operation_object_digest.clone(),
        semantic_payload_digest: prepared.intent.semantic_payload_digest.clone(),
        authority_subject_id: prepared.context.authority_subject_id.clone(),
        preview_id: prepared.preview.preview_id.clone(),
        brain_id: prepared.context.brain_id.clone(),
        mission_id: prepared.context.mission_id.clone(),
        mission_head_id: prepared.context.mission_head_id.clone(),
        target_identity: prepared.intent.semantic_payload.target_identity.clone(),
        managed_root: path_text(&prepared.managed_root),
        source_sha256_before: prepared
            .intent
            .semantic_payload
            .expected_target_sha256
            .clone(),
        candidate_sha256: prepared.intent.semantic_payload.candidate_sha256.clone(),
        bytes_before: prepared.source_bytes.len() as u64,
        bytes_after: prepared.candidate_bytes.len() as u64,
        permissions_before: prepared.target_permissions.clone(),
        file_identity_before: prepared.target_identity.clone(),
        graph_generation_prepared: prepared.intent.semantic_payload.expected_graph_generation,
        proof_mark_digest: prepared.intent.semantic_payload.proof_mark_digest.clone(),
        proof_expires_at_ms: prepared.intent.proof_expires_at_ms,
        candidate_temp_path: path_text(&candidate_temp),
        rollback_temp_path: path_text(&rollback_temp),
        backup_path: path_text(&directory.join("before.bytes")),
        created_at_ms,
    })
}

fn seal_pre_stage_intent(
    core: SourceEditPreStageIntentCoreV1,
) -> Result<SourceEditPreStageIntentV1, SourceEditTransactionError> {
    let intent_digest = canonical_digest(SOURCE_EDIT_PRE_STAGE_INTENT_DIGEST_DOMAIN, &core)?;
    Ok(SourceEditPreStageIntentV1 {
        core,
        intent_digest,
    })
}

fn pre_stage_intent_for_prepared(
    prepared: &PreparedSourceEditCommitV1,
) -> Result<SourceEditPreStageIntentV1, SourceEditTransactionError> {
    let created_at_ms = now_ms()?;
    let descriptor = seal_descriptor(descriptor_core_for_prepared(prepared, created_at_ms)?)?;
    let directory = prepared.transaction_root.join(&prepared.transaction_id);
    seal_pre_stage_intent(SourceEditPreStageIntentCoreV1 {
        schema: SOURCE_EDIT_PRE_STAGE_INTENT_SCHEMA.to_string(),
        transaction_id: prepared.transaction_id.clone(),
        operation_object_digest: prepared.context.operation_object_digest.clone(),
        semantic_payload_digest: prepared.intent.semantic_payload_digest.clone(),
        target_identity: descriptor.core.target_identity.clone(),
        managed_root: descriptor.core.managed_root.clone(),
        source_sha256_before: descriptor.core.source_sha256_before.clone(),
        candidate_sha256: descriptor.core.candidate_sha256.clone(),
        bytes_before: descriptor.core.bytes_before,
        bytes_after: descriptor.core.bytes_after,
        permissions_before: descriptor.core.permissions_before.clone(),
        transaction_directory: path_text(&directory),
        descriptor_path: path_text(&descriptor_path(&directory)),
        journal_path: path_text(&journal_path(&directory)),
        stage_path: path_text(&stage_path(&directory)),
        backup_path: descriptor.core.backup_path.clone(),
        candidate_temp_path: descriptor.core.candidate_temp_path.clone(),
        rollback_temp_path: descriptor.core.rollback_temp_path.clone(),
        descriptor,
        created_at_ms,
    })
}

fn validate_pre_stage_intent(
    intent: &SourceEditPreStageIntentV1,
) -> Result<(), SourceEditTransactionError> {
    validate_descriptor(&intent.core.descriptor)?;
    let descriptor = &intent.core.descriptor;
    let directory = Path::new(&intent.core.transaction_directory);
    let target = Path::new(&intent.core.target_identity);
    let expected_candidate = target
        .parent()
        .ok_or_else(|| SourceEditTransactionError::Preflight("target has no parent".to_string()))?
        .join(format!(
            ".m1nd-source-edit-{}.candidate",
            intent.core.transaction_id
        ));
    if intent.core.schema != SOURCE_EDIT_PRE_STAGE_INTENT_SCHEMA
        || !is_digest(&intent.core.transaction_id)
        || !is_digest(&intent.core.operation_object_digest)
        || !is_digest(&intent.core.semantic_payload_digest)
        || !is_digest(&intent.core.source_sha256_before)
        || !is_digest(&intent.core.candidate_sha256)
        || canonical_digest(SOURCE_EDIT_PRE_STAGE_INTENT_DIGEST_DOMAIN, &intent.core)?
            != intent.intent_digest
        || descriptor.core.transaction_id != intent.core.transaction_id
        || descriptor.core.operation_object_digest != intent.core.operation_object_digest
        || descriptor.core.semantic_payload_digest != intent.core.semantic_payload_digest
        || descriptor.core.target_identity != intent.core.target_identity
        || descriptor.core.managed_root != intent.core.managed_root
        || descriptor.core.source_sha256_before != intent.core.source_sha256_before
        || descriptor.core.candidate_sha256 != intent.core.candidate_sha256
        || descriptor.core.bytes_before != intent.core.bytes_before
        || descriptor.core.bytes_after != intent.core.bytes_after
        || descriptor.core.permissions_before != intent.core.permissions_before
        || descriptor.core.created_at_ms != intent.core.created_at_ms
        || Path::new(&intent.core.descriptor_path) != descriptor_path(directory)
        || Path::new(&intent.core.journal_path) != journal_path(directory)
        || Path::new(&intent.core.stage_path) != stage_path(directory)
        || Path::new(&intent.core.backup_path) != directory.join("before.bytes")
        || Path::new(&intent.core.backup_path) != Path::new(&descriptor.core.backup_path)
        || Path::new(&intent.core.candidate_temp_path) != expected_candidate
        || Path::new(&intent.core.candidate_temp_path)
            != Path::new(&descriptor.core.candidate_temp_path)
        || Path::new(&intent.core.rollback_temp_path)
            != expected_candidate.with_extension("rollback")
        || Path::new(&intent.core.rollback_temp_path)
            != Path::new(&descriptor.core.rollback_temp_path)
        || Path::new(&intent.core.candidate_temp_path) == target
        || Path::new(&intent.core.rollback_temp_path) == target
    {
        return Err(SourceEditTransactionError::Preflight(
            "pre-stage intent integrity or canonical layout validation failed".to_string(),
        ));
    }
    Ok(())
}

fn validate_pre_stage_intent_for_prepared(
    intent: &SourceEditPreStageIntentV1,
    prepared: &PreparedSourceEditCommitV1,
) -> Result<(), SourceEditTransactionError> {
    validate_pre_stage_intent(intent)?;
    let expected_descriptor =
        descriptor_core_for_prepared(prepared, intent.core.descriptor.core.created_at_ms)?;
    let expected_directory = prepared.transaction_root.join(&prepared.transaction_id);
    if intent.core.descriptor.core != expected_descriptor
        || Path::new(&intent.core.transaction_directory) != expected_directory
    {
        return Err(SourceEditTransactionError::ContextBinding(
            "durable pre-stage intent differs from the prepared source edit".to_string(),
        ));
    }
    Ok(())
}

fn refuse_pre_stage_abort_in_progress(
    transaction_root: &Path,
    transaction_id: &str,
) -> Result<(), SourceEditTransactionError> {
    if managed_entry_exists(&pre_stage_abort_receipt_path(
        transaction_root,
        transaction_id,
    ))? || managed_entry_exists(&pre_stage_abort_completion_path(
        transaction_root,
        transaction_id,
    ))? {
        return Err(SourceEditTransactionError::RecoveryRequired {
            transaction_id: transaction_id.to_string(),
            detail: "delete-only pre-stage abort is already in progress or complete".to_string(),
        });
    }
    Ok(())
}

fn load_or_create_pre_stage_intent<F: SourceEditFaults>(
    prepared: &PreparedSourceEditCommitV1,
    faults: &F,
) -> Result<SourceEditPreStageIntentV1, SourceEditTransactionError> {
    refuse_pre_stage_abort_in_progress(&prepared.transaction_root, &prepared.transaction_id)?;
    let path = pre_stage_intent_path(&prepared.transaction_root, &prepared.transaction_id);
    if managed_entry_exists(&path)? {
        let intent = read_json(&path, "pre_stage_intent_read")?;
        validate_pre_stage_intent_for_prepared(&intent, prepared)?;
        return Ok(intent);
    }
    let intent = pre_stage_intent_for_prepared(prepared)?;
    durable_json_new(&path, &intent)?;
    faults.hit(
        &prepared.transaction_id,
        SourceEditFailpointV1::PreStageIntentDurable,
    )?;
    Ok(intent)
}

fn ensure_exact_descriptor(
    directory: &Path,
    descriptor: &SourceEditDescriptorV1,
) -> Result<(), SourceEditTransactionError> {
    let path = descriptor_path(directory);
    if managed_entry_exists(&path)? {
        let existing: SourceEditDescriptorV1 = read_json(&path, "descriptor_resume_read")?;
        validate_descriptor(&existing)?;
        if existing != *descriptor {
            return Err(SourceEditTransactionError::ContextBinding(
                "existing descriptor differs from the first-write intent".to_string(),
            ));
        }
        return Ok(());
    }
    durable_json_new(&path, descriptor)
}

fn ensure_exact_backup(
    descriptor: &SourceEditDescriptorV1,
    source_bytes: &[u8],
) -> Result<(), SourceEditTransactionError> {
    let path = Path::new(&descriptor.core.backup_path);
    if managed_entry_exists(path)? {
        read_exact_abort_private_file(
            path,
            descriptor.core.bytes_before,
            &descriptor.core.source_sha256_before,
            &descriptor.core.transaction_id,
        )?;
        if fs::read(path).map_err(|error| io_error("backup_resume_read", error))? != source_bytes {
            return Err(SourceEditTransactionError::ContextBinding(
                "existing before-image differs from prepared bytes".to_string(),
            ));
        }
        return Ok(());
    }
    create_new_synced_file(path, source_bytes, None)
}

fn ensure_exact_candidate(
    managed_root: &Path,
    descriptor: &SourceEditDescriptorV1,
    candidate_bytes: &[u8],
) -> Result<(), SourceEditTransactionError> {
    let path = Path::new(&descriptor.core.candidate_temp_path);
    if managed_entry_exists(path)? {
        let candidate = read_target_snapshot(managed_root, path)?;
        if candidate.sha256 != descriptor.core.candidate_sha256
            || candidate.permissions != descriptor.core.permissions_before
            || candidate.bytes != candidate_bytes
        {
            return Err(SourceEditTransactionError::ContextBinding(
                "existing candidate differs from the first-write intent".to_string(),
            ));
        }
        return Ok(());
    }
    create_new_synced_file(
        path,
        candidate_bytes,
        Some(&descriptor.core.permissions_before),
    )
}

fn ensure_staging_journal_phase(
    directory: &Path,
    transaction_id: &str,
    phase: SourceEditJournalPhaseV1,
    observed_target_sha256: &str,
) -> Result<SourceEditJournalEventV1, SourceEditTransactionError> {
    let expected = [
        SourceEditJournalPhaseV1::Prepared,
        SourceEditJournalPhaseV1::BackupDurable,
        SourceEditJournalPhaseV1::CandidateDurable,
        SourceEditJournalPhaseV1::Staged,
    ];
    let requested = expected
        .iter()
        .position(|candidate| *candidate == phase)
        .ok_or_else(|| {
            SourceEditTransactionError::Preflight(
                "non-staging phase passed to staging journal resume".to_string(),
            )
        })?;
    let events = read_journal(&journal_path(directory), transaction_id)?;
    if events.len() > expected.len()
        || events.iter().enumerate().any(|(index, event)| {
            event.core.phase != expected[index]
                || event.core.observed_target_sha256 != observed_target_sha256
        })
        || events.len() < requested
    {
        return Err(SourceEditTransactionError::RecoveryRequired {
            transaction_id: transaction_id.to_string(),
            detail: "durable staging journal is not an exact resumable prefix".to_string(),
        });
    }
    if let Some(event) = events.get(requested) {
        return Ok(event.clone());
    }
    if events.len() != requested {
        return Err(SourceEditTransactionError::RecoveryRequired {
            transaction_id: transaction_id.to_string(),
            detail: "durable staging journal skipped a required phase".to_string(),
        });
    }
    append_journal_event(directory, transaction_id, phase, observed_target_sha256)
}

fn seal_descriptor(
    core: SourceEditDescriptorCoreV1,
) -> Result<SourceEditDescriptorV1, SourceEditTransactionError> {
    let descriptor_digest = canonical_digest(SOURCE_EDIT_DESCRIPTOR_DIGEST_DOMAIN, &core)?;
    Ok(SourceEditDescriptorV1 {
        core,
        descriptor_digest,
    })
}

fn validate_descriptor(
    descriptor: &SourceEditDescriptorV1,
) -> Result<(), SourceEditTransactionError> {
    if descriptor.core.schema != SOURCE_EDIT_DESCRIPTOR_SCHEMA
        || descriptor.core.transaction_id.is_empty()
        || !is_digest(&descriptor.core.operation_object_digest)
        || canonical_digest(SOURCE_EDIT_DESCRIPTOR_DIGEST_DOMAIN, &descriptor.core)?
            != descriptor.descriptor_digest
    {
        return Err(SourceEditTransactionError::Preflight(
            "transaction descriptor integrity validation failed".to_string(),
        ));
    }
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(
    path: &Path,
    phase: &'static str,
) -> Result<T, SourceEditTransactionError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| io_error(phase, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(SourceEditTransactionError::Preflight(format!(
            "refused non-regular managed artifact '{}'",
            path.display()
        )));
    }
    let bytes = fs::read(path).map_err(|error| io_error(phase, error))?;
    serde_json::from_slice(&bytes).map_err(|error| io_error(phase, error))
}

fn read_journal(
    path: &Path,
    expected_transaction_id: &str,
) -> Result<Vec<SourceEditJournalEventV1>, SourceEditTransactionError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let metadata = fs::symlink_metadata(path).map_err(|error| io_error("journal_lstat", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(SourceEditTransactionError::Preflight(
            "transaction journal is not a regular file".to_string(),
        ));
    }
    let file = File::open(path).map_err(|error| io_error("journal_open", error))?;
    let mut events = Vec::new();
    let mut previous = None;
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|error| io_error("journal_read", error))?;
        if line.trim().is_empty() {
            continue;
        }
        let event: SourceEditJournalEventV1 =
            serde_json::from_str(&line).map_err(|error| io_error("journal_parse", error))?;
        if event.core.schema != SOURCE_EDIT_JOURNAL_EVENT_SCHEMA
            || event.core.transaction_id != expected_transaction_id
            || event.core.sequence != events.len() as u64 + 1
            || event.core.previous_event_digest != previous
            || canonical_digest(SOURCE_EDIT_JOURNAL_EVENT_DIGEST_DOMAIN, &event.core)?
                != event.event_digest
        {
            return Err(SourceEditTransactionError::Preflight(
                "transaction journal chain validation failed".to_string(),
            ));
        }
        previous = Some(event.event_digest.clone());
        events.push(event);
    }
    Ok(events)
}

fn append_journal_event(
    transaction_dir: &Path,
    transaction_id: &str,
    phase: SourceEditJournalPhaseV1,
    observed_target_sha256: &str,
) -> Result<SourceEditJournalEventV1, SourceEditTransactionError> {
    let path = journal_path(transaction_dir);
    let events = read_journal(&path, transaction_id)?;
    let core = SourceEditJournalEventCoreV1 {
        schema: SOURCE_EDIT_JOURNAL_EVENT_SCHEMA.to_string(),
        transaction_id: transaction_id.to_string(),
        sequence: events.len() as u64 + 1,
        phase,
        observed_target_sha256: observed_target_sha256.to_string(),
        previous_event_digest: events.last().map(|event| event.event_digest.clone()),
        at_ms: now_ms()?,
    };
    let event = SourceEditJournalEventV1 {
        event_digest: canonical_digest(SOURCE_EDIT_JOURNAL_EVENT_DIGEST_DOMAIN, &core)?,
        core,
    };
    let mut options = OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    options
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut file = options
        .open(&path)
        .map_err(|error| io_error("journal_append_open", error))?;
    serde_json::to_writer(&mut file, &event)
        .map_err(|error| io_error("journal_serialize", error))?;
    file.write_all(b"\n")
        .map_err(|error| io_error("journal_append", error))?;
    file.sync_all()
        .map_err(|error| io_error("journal_fsync", error))?;
    fsync_parent(&path)?;
    Ok(event)
}

fn last_journal_phase(
    transaction_dir: &Path,
    transaction_id: &str,
) -> Result<Option<SourceEditJournalPhaseV1>, SourceEditTransactionError> {
    Ok(
        read_journal(&journal_path(transaction_dir), transaction_id)?
            .last()
            .map(|event| event.core.phase),
    )
}

fn journal_is_terminal(phase: Option<SourceEditJournalPhaseV1>) -> bool {
    matches!(
        phase,
        Some(
            SourceEditJournalPhaseV1::Finalized
                | SourceEditJournalPhaseV1::RolledBack
                | SourceEditJournalPhaseV1::RecoveredOld
                | SourceEditJournalPhaseV1::RecoveredNew
        )
    )
}

fn seal_stage(
    core: SourceEditStagedCommitCoreV1,
) -> Result<SourceEditStagedCommitV1, SourceEditTransactionError> {
    let stage_digest = canonical_digest(SOURCE_EDIT_STAGE_DIGEST_DOMAIN, &core)?;
    Ok(SourceEditStagedCommitV1 { core, stage_digest })
}

fn validate_stage(staged: &SourceEditStagedCommitV1) -> Result<(), SourceEditTransactionError> {
    if staged.core.schema != SOURCE_EDIT_STAGE_SCHEMA
        || !is_digest(&staged.core.transaction_id)
        || !is_digest(&staged.core.operation_object_digest)
        || !is_digest(&staged.core.pre_stage_intent_digest)
        || canonical_digest(SOURCE_EDIT_STAGE_DIGEST_DOMAIN, &staged.core)? != staged.stage_digest
    {
        return Err(SourceEditTransactionError::Preflight(
            "source edit durable stage integrity validation failed".to_string(),
        ));
    }
    Ok(())
}

fn seal_pre_stage_abort_receipt(
    core: SourceEditPreStageAbortReceiptCoreV1,
) -> Result<SourceEditPreStageAbortReceiptV1, SourceEditTransactionError> {
    let abort_digest = canonical_digest(SOURCE_EDIT_PRE_STAGE_ABORT_DIGEST_DOMAIN, &core)?;
    Ok(SourceEditPreStageAbortReceiptV1 { core, abort_digest })
}

fn validate_pre_stage_abort_receipt(
    receipt: &SourceEditPreStageAbortReceiptV1,
) -> Result<(), SourceEditTransactionError> {
    validate_pre_stage_intent(&receipt.core.pre_stage_intent)?;
    if receipt.core.schema != SOURCE_EDIT_PRE_STAGE_ABORT_SCHEMA
        || receipt.core.target_bytes_observed
        || receipt.core.target_write_performed
        || receipt.core.coordination_state_mutated
        || canonical_digest(SOURCE_EDIT_PRE_STAGE_ABORT_DIGEST_DOMAIN, &receipt.core)?
            != receipt.abort_digest
    {
        return Err(SourceEditTransactionError::Preflight(
            "pre-stage abort receipt integrity validation failed".to_string(),
        ));
    }
    Ok(())
}

fn seal_pre_stage_abort_completion(
    core: SourceEditPreStageAbortCompletionCoreV1,
) -> Result<SourceEditPreStageAbortCompletionV1, SourceEditTransactionError> {
    let completion_digest =
        canonical_digest(SOURCE_EDIT_PRE_STAGE_ABORT_COMPLETION_DIGEST_DOMAIN, &core)?;
    Ok(SourceEditPreStageAbortCompletionV1 {
        core,
        completion_digest,
    })
}

fn validate_pre_stage_abort_completion(
    completion: &SourceEditPreStageAbortCompletionV1,
) -> Result<(), SourceEditTransactionError> {
    if completion.core.schema != SOURCE_EDIT_PRE_STAGE_ABORT_COMPLETION_SCHEMA
        || completion.core.target_bytes_observed
        || completion.core.target_write_performed
        || completion.core.coordination_state_mutated
        || canonical_digest(
            SOURCE_EDIT_PRE_STAGE_ABORT_COMPLETION_DIGEST_DOMAIN,
            &completion.core,
        )? != completion.completion_digest
    {
        return Err(SourceEditTransactionError::Preflight(
            "pre-stage abort completion integrity validation failed".to_string(),
        ));
    }
    Ok(())
}

fn seal_stage_abort_receipt(
    core: SourceEditStageAbortReceiptCoreV1,
) -> Result<SourceEditStageAbortReceiptV1, SourceEditTransactionError> {
    let abort_digest = canonical_digest(SOURCE_EDIT_STAGE_ABORT_DIGEST_DOMAIN, &core)?;
    Ok(SourceEditStageAbortReceiptV1 { core, abort_digest })
}

fn validate_stage_abort_receipt(
    receipt: &SourceEditStageAbortReceiptV1,
) -> Result<(), SourceEditTransactionError> {
    if receipt.core.schema != SOURCE_EDIT_STAGE_ABORT_SCHEMA
        || !is_digest(&receipt.core.transaction_id)
        || !is_digest(&receipt.core.operation_object_digest)
        || receipt.core.target_bytes_observed
        || receipt.core.target_write_performed
        || receipt.core.coordination_state_mutated
        || canonical_digest(SOURCE_EDIT_STAGE_ABORT_DIGEST_DOMAIN, &receipt.core)?
            != receipt.abort_digest
    {
        return Err(SourceEditTransactionError::Preflight(
            "source edit stage-abort receipt integrity validation failed".to_string(),
        ));
    }
    Ok(())
}

fn seal_stage_abort_completion(
    core: SourceEditStageAbortCompletionCoreV1,
) -> Result<SourceEditStageAbortCompletionV1, SourceEditTransactionError> {
    let completion_digest =
        canonical_digest(SOURCE_EDIT_STAGE_ABORT_COMPLETION_DIGEST_DOMAIN, &core)?;
    Ok(SourceEditStageAbortCompletionV1 {
        core,
        completion_digest,
    })
}

fn validate_stage_abort_completion(
    completion: &SourceEditStageAbortCompletionV1,
) -> Result<(), SourceEditTransactionError> {
    if completion.core.schema != SOURCE_EDIT_STAGE_ABORT_COMPLETION_SCHEMA
        || completion.core.target_bytes_observed
        || completion.core.target_write_performed
        || completion.core.coordination_state_mutated
        || canonical_digest(
            SOURCE_EDIT_STAGE_ABORT_COMPLETION_DIGEST_DOMAIN,
            &completion.core,
        )? != completion.completion_digest
    {
        return Err(SourceEditTransactionError::Preflight(
            "source edit stage-abort completion integrity validation failed".to_string(),
        ));
    }
    Ok(())
}

fn seal_outcome(
    core: SourceEditCommitOutcomeCoreV1,
) -> Result<SourceEditCommitOutcomeV1, SourceEditTransactionError> {
    let outcome_digest = canonical_digest(SOURCE_EDIT_OUTCOME_DIGEST_DOMAIN, &core)?;
    Ok(SourceEditCommitOutcomeV1 {
        core,
        outcome_digest,
    })
}

fn validate_outcome(outcome: &SourceEditCommitOutcomeV1) -> Result<(), SourceEditTransactionError> {
    if outcome.core.schema != SOURCE_EDIT_OUTCOME_SCHEMA
        || canonical_digest(SOURCE_EDIT_OUTCOME_DIGEST_DOMAIN, &outcome.core)?
            != outcome.outcome_digest
    {
        return Err(SourceEditTransactionError::Preflight(
            "source edit outcome integrity validation failed".to_string(),
        ));
    }
    Ok(())
}

fn seal_terminal_receipt(
    core: SourceEditTerminalReceiptCoreV1,
) -> Result<SourceEditTerminalReceiptV1, SourceEditTransactionError> {
    let receipt_digest = canonical_digest(SOURCE_EDIT_TERMINAL_RECEIPT_DIGEST_DOMAIN, &core)?;
    Ok(SourceEditTerminalReceiptV1 {
        core,
        receipt_digest,
    })
}

fn validate_terminal_receipt(
    receipt: &SourceEditTerminalReceiptV1,
) -> Result<(), SourceEditTransactionError> {
    if receipt.core.schema != SOURCE_EDIT_TERMINAL_RECEIPT_SCHEMA
        || canonical_digest(SOURCE_EDIT_TERMINAL_RECEIPT_DIGEST_DOMAIN, &receipt.core)?
            != receipt.receipt_digest
    {
        return Err(SourceEditTransactionError::Preflight(
            "source edit terminal receipt integrity validation failed".to_string(),
        ));
    }
    Ok(())
}

fn transaction_dir(
    state: &SessionState,
    transaction_id: &str,
) -> Result<PathBuf, SourceEditTransactionError> {
    if !is_digest(transaction_id) {
        return Err(SourceEditTransactionError::InvalidRequest(
            "transaction id is not canonical SHA-256".to_string(),
        ));
    }
    Ok(canonical_runtime_root(state)?
        .join(SOURCE_EDIT_TX_DIRECTORY)
        .join(transaction_id))
}

fn load_descriptor(
    state: &SessionState,
    transaction_id: &str,
) -> Result<(PathBuf, SourceEditDescriptorV1), SourceEditTransactionError> {
    let directory = transaction_dir(state, transaction_id)?;
    let metadata = fs::symlink_metadata(&directory)
        .map_err(|error| io_error("transaction_directory_lstat", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(SourceEditTransactionError::Preflight(
            "transaction directory is not a real directory".to_string(),
        ));
    }
    let descriptor: SourceEditDescriptorV1 =
        read_json(&descriptor_path(&directory), "descriptor_read")?;
    validate_descriptor(&descriptor)?;
    if descriptor.core.transaction_id != transaction_id {
        return Err(SourceEditTransactionError::Preflight(
            "descriptor transaction id differs from directory identity".to_string(),
        ));
    }
    Ok((directory, descriptor))
}

fn existing_terminal_receipt(
    transaction_dir: &Path,
) -> Result<Option<SourceEditTerminalReceiptV1>, SourceEditTransactionError> {
    let path = terminal_receipt_path(transaction_dir);
    if !path.exists() {
        return Ok(None);
    }
    let receipt = read_json(&path, "terminal_receipt_read")?;
    validate_terminal_receipt(&receipt)?;
    Ok(Some(receipt))
}

fn validate_stage_descriptor_binding(
    directory: &Path,
    staged: &SourceEditStagedCommitV1,
    descriptor: &SourceEditDescriptorV1,
) -> Result<(), SourceEditTransactionError> {
    validate_stage(staged)?;
    validate_descriptor(descriptor)?;
    if staged.core.transaction_id != descriptor.core.transaction_id
        || staged.core.operation_object_digest != descriptor.core.operation_object_digest
        || staged.core.semantic_payload_digest != descriptor.core.semantic_payload_digest
        || staged.core.target_identity != descriptor.core.target_identity
        || staged.core.source_sha256_before != descriptor.core.source_sha256_before
        || staged.core.candidate_sha256 != descriptor.core.candidate_sha256
        || staged.core.descriptor_digest != descriptor.descriptor_digest
        || staged.core.graph_generation_prepared != descriptor.core.graph_generation_prepared
    {
        return Err(SourceEditTransactionError::ContextBinding(
            "durable stage differs from its sealed transaction descriptor".to_string(),
        ));
    }
    let transaction_root = directory.parent().ok_or_else(|| {
        SourceEditTransactionError::Preflight("transaction directory has no root".to_string())
    })?;
    let intent: SourceEditPreStageIntentV1 = read_json(
        &pre_stage_intent_path(transaction_root, &descriptor.core.transaction_id),
        "stage_pre_stage_intent_read",
    )?;
    validate_pre_stage_intent(&intent)?;
    if intent.intent_digest != staged.core.pre_stage_intent_digest
        || intent.core.descriptor != *descriptor
        || Path::new(&intent.core.transaction_directory) != directory
    {
        return Err(SourceEditTransactionError::ContextBinding(
            "durable stage differs from its first-write pre-stage intent".to_string(),
        ));
    }
    Ok(())
}

fn read_exact_backup(
    descriptor: &SourceEditDescriptorV1,
) -> Result<Vec<u8>, SourceEditTransactionError> {
    let path = Path::new(&descriptor.core.backup_path);
    let metadata = fs::symlink_metadata(path).map_err(|error| io_error("backup_lstat", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(SourceEditTransactionError::Preflight(
            "durable before-image is not a regular file".to_string(),
        ));
    }
    let bytes = fs::read(path).map_err(|error| io_error("backup_read", error))?;
    if bytes.len() as u64 != descriptor.core.bytes_before
        || sha256_hex(&bytes) != descriptor.core.source_sha256_before
    {
        return Err(SourceEditTransactionError::ManualRecovery {
            transaction_id: descriptor.core.transaction_id.clone(),
            detail: "durable before-image failed size/digest validation".to_string(),
        });
    }
    Ok(bytes)
}

fn read_exact_candidate(
    managed_root: &Path,
    descriptor: &SourceEditDescriptorV1,
) -> Result<SourceEditTargetSnapshotV1, SourceEditTransactionError> {
    let candidate = read_target_snapshot(
        managed_root,
        Path::new(&descriptor.core.candidate_temp_path),
    )?;
    if candidate.sha256 != descriptor.core.candidate_sha256
        || candidate.bytes.len() as u64 != descriptor.core.bytes_after
        || candidate.permissions != descriptor.core.permissions_before
    {
        return Err(SourceEditTransactionError::ManualRecovery {
            transaction_id: descriptor.core.transaction_id.clone(),
            detail: "durable candidate failed size/digest/permission validation".to_string(),
        });
    }
    Ok(candidate)
}

fn validated_stage_journal_root(
    directory: &Path,
    staged: &SourceEditStagedCommitV1,
) -> Result<SourceEditJournalPhaseV1, SourceEditTransactionError> {
    let events = read_journal(&journal_path(directory), staged.transaction_id())?;
    let event = events.last().ok_or_else(|| {
        SourceEditTransactionError::Preflight("durable stage journal is empty".to_string())
    })?;
    if event.event_digest != staged.core.journal_root_digest {
        return Err(SourceEditTransactionError::ContextBinding(
            "durable stage does not bind the journal root".to_string(),
        ));
    }
    Ok(event.core.phase)
}

fn revalidate_stage_before_commit(
    state: &SessionState,
    staged: &SourceEditStagedCommitV1,
) -> Result<(), SourceEditTransactionError> {
    let (directory, descriptor) = load_descriptor(state, staged.transaction_id())?;
    validate_stage_descriptor_binding(&directory, staged, &descriptor)?;
    if validated_stage_journal_root(&directory, staged)? != SourceEditJournalPhaseV1::Staged {
        return Err(SourceEditTransactionError::ContextBinding(
            "outer COMMIT callback requires the exact un-published staged journal root".to_string(),
        ));
    }
    if state.graph_generation != descriptor.core.graph_generation_prepared {
        return Err(SourceEditTransactionError::OccConflict(
            "graph generation changed between durable stage and outer COMMIT".to_string(),
        ));
    }
    let preview = state
        .edit_previews
        .get(&descriptor.core.preview_id)
        .ok_or_else(|| {
            SourceEditTransactionError::OccConflict(
                "preview disappeared between durable stage and outer COMMIT".to_string(),
            )
        })?;
    if preview.agent_id != descriptor.core.authority_subject_id
        || path_text(Path::new(&preview.file_path)) != descriptor.core.target_identity
        || preview.source_sha256 != descriptor.core.source_sha256_before
        || preview.candidate_sha256 != descriptor.core.candidate_sha256
    {
        return Err(SourceEditTransactionError::OccConflict(
            "preview bindings changed between durable stage and outer COMMIT".to_string(),
        ));
    }
    let mark = state
        .validated_proof_ready_mark(
            &descriptor.core.authority_subject_id,
            &descriptor.core.target_identity,
        )
        .map_err(SourceEditTransactionError::Proof)?;
    if canonical_digest(SOURCE_EDIT_PROOF_MARK_DIGEST_DOMAIN, &mark)?
        != descriptor.core.proof_mark_digest
    {
        return Err(SourceEditTransactionError::Proof(
            "proof mark changed between durable stage and outer COMMIT".to_string(),
        ));
    }
    let (managed_root, target) =
        managed_target(state, Path::new(&descriptor.core.target_identity))?;
    if path_text(&managed_root) != descriptor.core.managed_root {
        return Err(SourceEditTransactionError::ContextBinding(
            "managed root differs from the sealed descriptor".to_string(),
        ));
    }
    let current = read_target_snapshot(&managed_root, &target)?;
    if current.sha256 != descriptor.core.source_sha256_before
        || current.identity != descriptor.core.file_identity_before
        || current.permissions != descriptor.core.permissions_before
    {
        return Err(SourceEditTransactionError::OccConflict(
            "target changed between durable stage and outer COMMIT".to_string(),
        ));
    }
    read_exact_backup(&descriptor)?;
    read_exact_candidate(&managed_root, &descriptor)?;
    Ok(())
}

fn consume_coordination_after_publish(
    state: &mut SessionState,
    descriptor: &SourceEditDescriptorV1,
) -> Result<(), SourceEditTransactionError> {
    let key = (
        descriptor.core.authority_subject_id.clone(),
        descriptor.core.target_identity.clone(),
    );
    for mark in [
        state.proof_ready.get(&key),
        state.active_proof_permits.get(&key),
    ]
    .into_iter()
    .flatten()
    {
        if canonical_digest(SOURCE_EDIT_PROOF_MARK_DIGEST_DOMAIN, mark)?
            != descriptor.core.proof_mark_digest
        {
            return Err(SourceEditTransactionError::ContextBinding(
                "post-COMMIT proof slot contains a different mark".to_string(),
            ));
        }
    }
    if let Some(preview) = state.edit_previews.get(&descriptor.core.preview_id) {
        if preview.agent_id != descriptor.core.authority_subject_id
            || path_text(Path::new(&preview.file_path)) != descriptor.core.target_identity
            || preview.source_sha256 != descriptor.core.source_sha256_before
            || preview.candidate_sha256 != descriptor.core.candidate_sha256
        {
            return Err(SourceEditTransactionError::ContextBinding(
                "post-COMMIT preview slot contains different bindings".to_string(),
            ));
        }
    }
    state.proof_ready.remove(&key);
    state.active_proof_permits.remove(&key);
    state.edit_previews.remove(&descriptor.core.preview_id);
    Ok(())
}

fn publish_after_commit<F: SourceEditFaults>(
    state: &mut SessionState,
    staged: &SourceEditStagedCommitV1,
    faults: &F,
) -> Result<SourceEditCommitOutcomeV1, SourceEditTransactionError> {
    let (directory, descriptor) = load_descriptor(state, staged.transaction_id())?;
    validate_stage_descriptor_binding(&directory, staged, &descriptor)?;

    let locks_dir = directory
        .parent()
        .ok_or_else(|| {
            SourceEditTransactionError::Preflight("transaction has no root".to_string())
        })?
        .join(".locks");
    let lock_slug = sha256_hex(descriptor.core.target_identity.as_bytes());
    let _target_lock = LockGuard::acquire_in(&locks_dir, &lock_slug).map_err(|error| {
        SourceEditTransactionError::Preflight(format!(
            "cannot acquire source publish lock: {error}"
        ))
    })?;

    let (managed_root, target) =
        managed_target(state, Path::new(&descriptor.core.target_identity))?;
    if path_text(&managed_root) != descriptor.core.managed_root {
        return Err(SourceEditTransactionError::ContextBinding(
            "managed root differs from the sealed descriptor".to_string(),
        ));
    }

    let existing_outcome_path = outcome_path(&directory);
    if existing_outcome_path.exists() {
        let outcome: SourceEditCommitOutcomeV1 =
            read_json(&existing_outcome_path, "outcome_replay_read")?;
        validate_outcome(&outcome)?;
        if outcome.core.operation_object_digest != staged.core.operation_object_digest
            || outcome.core.stage_digest != staged.stage_digest
        {
            return Err(SourceEditTransactionError::ContextBinding(
                "existing outcome belongs to a different durable stage".to_string(),
            ));
        }
        let current = read_target_snapshot(&managed_root, &target)?;
        if current.sha256 != descriptor.core.candidate_sha256
            || current.permissions != descriptor.core.permissions_before
        {
            return Err(SourceEditTransactionError::RecoveryRequired {
                transaction_id: descriptor.core.transaction_id,
                detail: "durable outcome exists but target is not the exact candidate".to_string(),
            });
        }
        let phase = last_journal_phase(&directory, staged.transaction_id())?;
        let terminal = existing_terminal_receipt(&directory)?;
        if terminal.is_some() && !journal_is_terminal(phase) {
            return Err(SourceEditTransactionError::ContextBinding(
                "terminal source-edit receipt exists without a terminal journal phase".to_string(),
            ));
        }
        // A lost outer response may replay after this adapter already reached
        // FINALIZED. Never append OUTCOME_DURABLE behind a terminal event: that
        // would regress the durable state machine and manufacture a false boot
        // pending record. A terminal phase without its receipt is also left in
        // place so `finalize_outcome` can complete the interrupted receipt.
        if terminal.is_none()
            && !journal_is_terminal(phase)
            && phase != Some(SourceEditJournalPhaseV1::OutcomeDurable)
        {
            append_journal_event(
                &directory,
                staged.transaction_id(),
                SourceEditJournalPhaseV1::OutcomeDurable,
                &current.sha256,
            )?;
        }
        consume_coordination_after_publish(state, &descriptor)?;
        return Ok(outcome);
    }

    let current = read_target_snapshot(&managed_root, &target)?;
    if current.sha256 == descriptor.core.source_sha256_before {
        if current.identity != descriptor.core.file_identity_before
            || current.permissions != descriptor.core.permissions_before
        {
            return Err(SourceEditTransactionError::OccConflict(
                "target identity or permissions changed before committed publish".to_string(),
            ));
        }
        read_exact_backup(&descriptor)?;
        read_exact_candidate(&managed_root, &descriptor)?;
        let phase = last_journal_phase(&directory, staged.transaction_id())?;
        match phase {
            Some(SourceEditJournalPhaseV1::Staged) => {
                append_journal_event(
                    &directory,
                    staged.transaction_id(),
                    SourceEditJournalPhaseV1::PublishIntent,
                    &current.sha256,
                )?;
            }
            Some(SourceEditJournalPhaseV1::PublishIntent) => {}
            other => {
                return Err(SourceEditTransactionError::RecoveryRequired {
                    transaction_id: descriptor.core.transaction_id.clone(),
                    detail: format!("old target has incompatible publish journal phase: {other:?}"),
                })
            }
        }
        faults.hit(
            staged.transaction_id(),
            SourceEditFailpointV1::PublishIntent,
        )?;

        // Revalidate both directory entries while holding the transaction lock.
        // Arbitrary same-UID writers do not honor this lock, so that residual
        // parent-entry race remains explicitly surfaced in every receipt.
        let rechecked = read_target_snapshot(&managed_root, &target)?;
        if rechecked.sha256 != descriptor.core.source_sha256_before
            || rechecked.identity != descriptor.core.file_identity_before
            || rechecked.permissions != descriptor.core.permissions_before
        {
            return Err(SourceEditTransactionError::OccConflict(
                "target changed immediately before committed atomic publish".to_string(),
            ));
        }
        read_exact_candidate(&managed_root, &descriptor)?;
        fs::rename(Path::new(&descriptor.core.candidate_temp_path), &target)
            .map_err(|error| io_error("atomic_publish_rename", error))?;
        fsync_parent(&target)?;
        faults.hit(staged.transaction_id(), SourceEditFailpointV1::AtomicRename)?;
    } else if current.sha256 != descriptor.core.candidate_sha256 {
        return Err(SourceEditTransactionError::ManualRecovery {
            transaction_id: descriptor.core.transaction_id.clone(),
            detail: "committed publish found target at neither sealed digest".to_string(),
        });
    }

    let after = read_target_snapshot(&managed_root, &target)?;
    if after.sha256 != descriptor.core.candidate_sha256
        || after.permissions != descriptor.core.permissions_before
    {
        return Err(SourceEditTransactionError::RecoveryRequired {
            transaction_id: descriptor.core.transaction_id.clone(),
            detail: "post-publish digest or permissions do not match the sealed candidate"
                .to_string(),
        });
    }
    let events = read_journal(&journal_path(&directory), staged.transaction_id())?;
    let published = if let Some(event) = events
        .iter()
        .rev()
        .find(|event| event.core.phase == SourceEditJournalPhaseV1::Published)
    {
        event.clone()
    } else {
        let event = append_journal_event(
            &directory,
            staged.transaction_id(),
            SourceEditJournalPhaseV1::Published,
            &after.sha256,
        )?;
        faults.hit(
            staged.transaction_id(),
            SourceEditFailpointV1::PublishedJournal,
        )?;
        event
    };
    let conservation = SourceEditConservationV1 {
        target_identity_before: descriptor.core.target_identity.clone(),
        target_identity_after: descriptor.core.target_identity.clone(),
        target_count_before: 1,
        target_count_after: 1,
        bytes_before: descriptor.core.bytes_before,
        bytes_after: descriptor.core.bytes_after,
        source_sha256_before: descriptor.core.source_sha256_before.clone(),
        source_sha256_after: after.sha256.clone(),
        candidate_sha256: descriptor.core.candidate_sha256.clone(),
        permissions_before: descriptor.core.permissions_before.clone(),
        permissions_after: after.permissions.clone(),
        permissions_preserved: after.permissions == descriptor.core.permissions_before,
        graph_generation_prepared: descriptor.core.graph_generation_prepared,
        graph_generation_at_publish: state.graph_generation,
        graph_resync_required: true,
    };
    let outcome = seal_outcome(SourceEditCommitOutcomeCoreV1 {
        schema: SOURCE_EDIT_OUTCOME_SCHEMA.to_string(),
        transaction_id: descriptor.core.transaction_id.clone(),
        operation_object_digest: descriptor.core.operation_object_digest.clone(),
        stage_digest: staged.stage_digest.clone(),
        semantic_payload_digest: descriptor.core.semantic_payload_digest.clone(),
        authority_subject_id: descriptor.core.authority_subject_id.clone(),
        brain_id: descriptor.core.brain_id.clone(),
        mission_id: descriptor.core.mission_id.clone(),
        mission_head_id: descriptor.core.mission_head_id.clone(),
        state: SourceEditOutcomeStateV1::AppliedGraphPending,
        conservation,
        journal_root_digest: published.event_digest,
        graph_resync_required: true,
        rollback_available: true,
        assurance_limitations: assurance_limitations(),
        applied_at_ms: now_ms()?,
    })?;
    durable_json_new(&existing_outcome_path, &outcome)?;
    append_journal_event(
        &directory,
        staged.transaction_id(),
        SourceEditJournalPhaseV1::OutcomeDurable,
        &after.sha256,
    )?;
    faults.hit(
        staged.transaction_id(),
        SourceEditFailpointV1::OutcomeDurable,
    )?;
    consume_coordination_after_publish(state, &descriptor)?;
    Ok(outcome)
}

fn finalize_outcome<F: SourceEditFaults>(
    state: &mut SessionState,
    outcome: &SourceEditCommitOutcomeV1,
    faults: &F,
) -> Result<SourceEditTerminalReceiptV1, SourceEditTransactionError> {
    validate_outcome(outcome)?;
    let (directory, descriptor) = load_descriptor(state, outcome.transaction_id())?;
    if descriptor.core.operation_object_digest != outcome.core.operation_object_digest {
        return Err(SourceEditTransactionError::ContextBinding(
            "outcome and descriptor operation digests differ".to_string(),
        ));
    }
    if let Some(receipt) = existing_terminal_receipt(&directory)? {
        if receipt.core.transaction_id != outcome.core.transaction_id
            || receipt.core.operation_object_digest != outcome.core.operation_object_digest
        {
            return Err(SourceEditTransactionError::ContextBinding(
                "terminal receipt belongs to another operation".to_string(),
            ));
        }
        return Ok(receipt);
    }
    let (managed_root, target) =
        managed_target(state, Path::new(&descriptor.core.target_identity))?;
    if path_text(&managed_root) != descriptor.core.managed_root {
        return Err(SourceEditTransactionError::ContextBinding(
            "managed root differs from the sealed descriptor".to_string(),
        ));
    }
    let current = read_target_snapshot(&managed_root, &target)?;
    if current.sha256 != descriptor.core.candidate_sha256 {
        return Err(SourceEditTransactionError::RecoveryRequired {
            transaction_id: descriptor.core.transaction_id,
            detail: "finalize requires the exact sealed candidate bytes".to_string(),
        });
    }
    faults.hit(
        &descriptor.core.transaction_id,
        SourceEditFailpointV1::Finalize,
    )?;
    let event = append_journal_event(
        &directory,
        &descriptor.core.transaction_id,
        SourceEditJournalPhaseV1::Finalized,
        &current.sha256,
    )?;
    let receipt = seal_terminal_receipt(SourceEditTerminalReceiptCoreV1 {
        schema: SOURCE_EDIT_TERMINAL_RECEIPT_SCHEMA.to_string(),
        transaction_id: descriptor.core.transaction_id.clone(),
        operation_object_digest: descriptor.core.operation_object_digest.clone(),
        terminal_state: SourceEditTerminalStateV1::FinalizedNew,
        source_sha256: current.sha256,
        graph_resync_required: true,
        replay_reapplied_source_bytes: false,
        journal_root_digest: event.event_digest,
        assurance_limitations: assurance_limitations(),
        terminal_at_ms: now_ms()?,
    })?;
    durable_json_new(&terminal_receipt_path(&directory), &receipt)?;
    remove_file_if_exists(Path::new(&descriptor.core.backup_path))?;
    remove_file_if_exists(Path::new(&descriptor.core.candidate_temp_path))?;
    remove_file_if_exists(Path::new(&descriptor.core.rollback_temp_path))?;
    Ok(receipt)
}

fn rollback_outcome<F: SourceEditFaults>(
    state: &mut SessionState,
    outcome: &SourceEditCommitOutcomeV1,
    faults: &F,
) -> Result<SourceEditTerminalReceiptV1, SourceEditTransactionError> {
    validate_outcome(outcome)?;
    let (directory, descriptor) = load_descriptor(state, outcome.transaction_id())?;
    if descriptor.core.operation_object_digest != outcome.core.operation_object_digest {
        return Err(SourceEditTransactionError::ContextBinding(
            "outcome and descriptor operation digests differ".to_string(),
        ));
    }
    if let Some(receipt) = existing_terminal_receipt(&directory)? {
        return Ok(receipt);
    }
    restore_old_from_descriptor(
        state,
        &directory,
        &descriptor,
        SourceEditTerminalStateV1::RolledBackOld,
        SourceEditJournalPhaseV1::RolledBack,
        faults,
    )
}

fn restore_old_from_descriptor<F: SourceEditFaults>(
    state: &mut SessionState,
    directory: &Path,
    descriptor: &SourceEditDescriptorV1,
    terminal_state: SourceEditTerminalStateV1,
    terminal_phase: SourceEditJournalPhaseV1,
    faults: &F,
) -> Result<SourceEditTerminalReceiptV1, SourceEditTransactionError> {
    let (managed_root, target) =
        managed_target(state, Path::new(&descriptor.core.target_identity))?;
    if path_text(&managed_root) != descriptor.core.managed_root {
        return Err(SourceEditTransactionError::ContextBinding(
            "managed root differs from the sealed descriptor".to_string(),
        ));
    }
    let current = read_target_snapshot(&managed_root, &target)?;
    if current.sha256 == descriptor.core.source_sha256_before {
        return recovered_old_receipt(state, directory, descriptor, terminal_state, terminal_phase);
    }
    if current.sha256 != descriptor.core.candidate_sha256 {
        return Err(SourceEditTransactionError::ManualRecovery {
            transaction_id: descriptor.core.transaction_id.clone(),
            detail: "target is neither the sealed before nor after digest; refusing to overwrite a concurrent writer"
                .to_string(),
        });
    }
    let backup_path = Path::new(&descriptor.core.backup_path);
    let backup = fs::read(backup_path).map_err(|error| io_error("rollback_backup_read", error))?;
    if backup.len() as u64 != descriptor.core.bytes_before
        || sha256_hex(&backup) != descriptor.core.source_sha256_before
    {
        return Err(SourceEditTransactionError::ManualRecovery {
            transaction_id: descriptor.core.transaction_id.clone(),
            detail: "durable before-image failed size/digest validation".to_string(),
        });
    }
    append_journal_event(
        directory,
        &descriptor.core.transaction_id,
        SourceEditJournalPhaseV1::RollbackIntent,
        &current.sha256,
    )?;
    faults.hit(
        &descriptor.core.transaction_id,
        SourceEditFailpointV1::RollbackIntent,
    )?;
    let rollback_temp = Path::new(&descriptor.core.rollback_temp_path);
    if rollback_temp.exists() {
        let metadata = fs::symlink_metadata(rollback_temp)
            .map_err(|error| io_error("rollback_temp_lstat", error))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(SourceEditTransactionError::ManualRecovery {
                transaction_id: descriptor.core.transaction_id.clone(),
                detail: "rollback temp path is occupied by a non-regular entry".to_string(),
            });
        }
        let bytes =
            fs::read(rollback_temp).map_err(|error| io_error("rollback_temp_read", error))?;
        if sha256_hex(&bytes) != descriptor.core.source_sha256_before {
            return Err(SourceEditTransactionError::ManualRecovery {
                transaction_id: descriptor.core.transaction_id.clone(),
                detail: "existing rollback temp does not contain the sealed before-image"
                    .to_string(),
            });
        }
    } else {
        create_new_synced_file(
            rollback_temp,
            &backup,
            Some(&descriptor.core.permissions_before),
        )?;
    }
    let rechecked = read_target_snapshot(&managed_root, &target)?;
    if rechecked.sha256 != descriptor.core.candidate_sha256 {
        return Err(SourceEditTransactionError::ManualRecovery {
            transaction_id: descriptor.core.transaction_id.clone(),
            detail: "target changed immediately before rollback publish".to_string(),
        });
    }
    fs::rename(rollback_temp, &target).map_err(|error| io_error("rollback_rename", error))?;
    fsync_parent(&target)?;
    faults.hit(
        &descriptor.core.transaction_id,
        SourceEditFailpointV1::RollbackRename,
    )?;
    let restored = read_target_snapshot(&managed_root, &target)?;
    if restored.sha256 != descriptor.core.source_sha256_before
        || restored.permissions != descriptor.core.permissions_before
    {
        return Err(SourceEditTransactionError::ManualRecovery {
            transaction_id: descriptor.core.transaction_id.clone(),
            detail: "rollback publish did not restore exact bytes and permissions".to_string(),
        });
    }
    recovered_old_receipt(state, directory, descriptor, terminal_state, terminal_phase)
}

fn recovered_old_receipt(
    _state: &mut SessionState,
    directory: &Path,
    descriptor: &SourceEditDescriptorV1,
    terminal_state: SourceEditTerminalStateV1,
    terminal_phase: SourceEditJournalPhaseV1,
) -> Result<SourceEditTerminalReceiptV1, SourceEditTransactionError> {
    let event = append_journal_event(
        directory,
        &descriptor.core.transaction_id,
        terminal_phase,
        &descriptor.core.source_sha256_before,
    )?;
    let receipt = seal_terminal_receipt(SourceEditTerminalReceiptCoreV1 {
        schema: SOURCE_EDIT_TERMINAL_RECEIPT_SCHEMA.to_string(),
        transaction_id: descriptor.core.transaction_id.clone(),
        operation_object_digest: descriptor.core.operation_object_digest.clone(),
        terminal_state,
        source_sha256: descriptor.core.source_sha256_before.clone(),
        graph_resync_required: false,
        replay_reapplied_source_bytes: false,
        journal_root_digest: event.event_digest,
        assurance_limitations: assurance_limitations(),
        terminal_at_ms: now_ms()?,
    })?;
    if terminal_receipt_path(directory).exists() {
        let existing = existing_terminal_receipt(directory)?.ok_or_else(|| {
            SourceEditTransactionError::Preflight("terminal receipt disappeared".to_string())
        })?;
        if existing != receipt {
            return Err(SourceEditTransactionError::ManualRecovery {
                transaction_id: descriptor.core.transaction_id.clone(),
                detail: "a different terminal receipt already exists".to_string(),
            });
        }
    } else {
        durable_json_new(&terminal_receipt_path(directory), &receipt)?;
    }
    remove_file_if_exists(Path::new(&descriptor.core.backup_path))?;
    remove_file_if_exists(Path::new(&descriptor.core.candidate_temp_path))?;
    remove_file_if_exists(Path::new(&descriptor.core.rollback_temp_path))?;
    Ok(receipt)
}

fn recover_transaction(
    state: &mut SessionState,
    transaction_id: &str,
    operation_object_digest: &str,
    decision: SourceEditRecoveryDecisionV1,
) -> Result<SourceEditTerminalReceiptV1, SourceEditTransactionError> {
    let (directory, descriptor) = load_descriptor(state, transaction_id)?;
    if descriptor.core.operation_object_digest != operation_object_digest {
        return Err(SourceEditTransactionError::ContextBinding(
            "recovery operation digest differs from the sealed descriptor".to_string(),
        ));
    }
    if let Some(receipt) = existing_terminal_receipt(&directory)? {
        return Ok(receipt);
    }
    let (managed_root, target) =
        managed_target(state, Path::new(&descriptor.core.target_identity))?;
    let current = read_target_snapshot(&managed_root, &target)?;
    if current.sha256 == descriptor.core.source_sha256_before {
        return recovered_old_receipt(
            state,
            &directory,
            &descriptor,
            SourceEditTerminalStateV1::RecoveredOld,
            SourceEditJournalPhaseV1::RecoveredOld,
        );
    }
    if current.sha256 != descriptor.core.candidate_sha256 {
        return Err(SourceEditTransactionError::ManualRecovery {
            transaction_id: transaction_id.to_string(),
            detail: "target is neither old nor new; recovery refuses third-party bytes".to_string(),
        });
    }
    match decision {
        SourceEditRecoveryDecisionV1::RestoreOld => restore_old_from_descriptor(
            state,
            &directory,
            &descriptor,
            SourceEditTerminalStateV1::RecoveredOld,
            SourceEditJournalPhaseV1::RecoveredOld,
            &NoSourceEditFaults,
        ),
        SourceEditRecoveryDecisionV1::KeepNew => {
            let event = append_journal_event(
                &directory,
                transaction_id,
                SourceEditJournalPhaseV1::RecoveredNew,
                &current.sha256,
            )?;
            let receipt = seal_terminal_receipt(SourceEditTerminalReceiptCoreV1 {
                schema: SOURCE_EDIT_TERMINAL_RECEIPT_SCHEMA.to_string(),
                transaction_id: transaction_id.to_string(),
                operation_object_digest: operation_object_digest.to_string(),
                terminal_state: SourceEditTerminalStateV1::RecoveredNew,
                source_sha256: current.sha256,
                graph_resync_required: true,
                replay_reapplied_source_bytes: false,
                journal_root_digest: event.event_digest,
                assurance_limitations: assurance_limitations(),
                terminal_at_ms: now_ms()?,
            })?;
            durable_json_new(&terminal_receipt_path(&directory), &receipt)?;
            remove_file_if_exists(Path::new(&descriptor.core.backup_path))?;
            remove_file_if_exists(Path::new(&descriptor.core.candidate_temp_path))?;
            remove_file_if_exists(Path::new(&descriptor.core.rollback_temp_path))?;
            Ok(receipt)
        }
    }
}

fn marker_transaction_id(file_name: &str, prefix: &str) -> Option<String> {
    let value = file_name.strip_prefix(prefix)?.strip_suffix(".json")?;
    is_digest(value).then(|| value.to_string())
}

fn load_pre_stage_intent_from_root(
    transaction_root: &Path,
    transaction_id: &str,
) -> Result<SourceEditPreStageIntentV1, SourceEditTransactionError> {
    let intent: SourceEditPreStageIntentV1 = read_json(
        &pre_stage_intent_path(transaction_root, transaction_id),
        "pre_stage_recovery_intent_read",
    )?;
    validate_pre_stage_intent(&intent)?;
    if intent.core.transaction_id != transaction_id
        || Path::new(&intent.core.transaction_directory) != transaction_root.join(transaction_id)
    {
        return Err(SourceEditTransactionError::ContextBinding(
            "pre-stage intent differs from its transaction-root identity".to_string(),
        ));
    }
    Ok(intent)
}

fn pending_pre_stage_recovery(
    state: &SessionState,
) -> Result<BTreeMap<String, SourceEditPreStageRecoveryV1>, SourceEditTransactionError> {
    let root = canonical_runtime_root(state)?.join(SOURCE_EDIT_TX_DIRECTORY);
    if !managed_entry_exists(&root)? {
        return Ok(BTreeMap::new());
    }
    validate_private_directory(&root)?;
    let mut transaction_ids = BTreeSet::new();
    for entry in
        fs::read_dir(&root).map_err(|error| io_error("pre_stage_pending_read_dir", error))?
    {
        let entry = entry.map_err(|error| io_error("pre_stage_pending_entry", error))?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if let Some(transaction_id) = marker_transaction_id(&file_name, PRE_STAGE_INTENT_PREFIX)
            .or_else(|| marker_transaction_id(&file_name, PRE_STAGE_ABORT_PREFIX))
        {
            let file_type = entry
                .file_type()
                .map_err(|error| io_error("pre_stage_pending_entry_type", error))?;
            if file_type.is_symlink() || !file_type.is_file() {
                return Err(SourceEditTransactionError::Preflight(format!(
                    "pre-stage recovery marker is not a regular file: {file_name}"
                )));
            }
            transaction_ids.insert(transaction_id);
        }
    }

    let mut pending = BTreeMap::new();
    for transaction_id in transaction_ids {
        let completion_path = pre_stage_abort_completion_path(&root, &transaction_id);
        let abort_path = pre_stage_abort_receipt_path(&root, &transaction_id);
        if managed_entry_exists(&completion_path)? {
            let completion: SourceEditPreStageAbortCompletionV1 =
                read_json(&completion_path, "pre_stage_pending_completion_read")?;
            validate_pre_stage_abort_completion(&completion)?;
            if completion.core.transaction_id != transaction_id {
                return Err(SourceEditTransactionError::ContextBinding(
                    "pre-stage abort completion differs from its marker identity".to_string(),
                ));
            }
            if !managed_entry_exists(&abort_path)? {
                return Err(SourceEditTransactionError::Preflight(
                    "pre-stage abort completion exists without its receipt".to_string(),
                ));
            }
            let receipt: SourceEditPreStageAbortReceiptV1 =
                read_json(&abort_path, "pre_stage_pending_completed_receipt_read")?;
            validate_pre_stage_abort_receipt(&receipt)?;
            if completion.core.abort_digest != receipt.abort_digest
                || completion.core.intent_digest != receipt.core.pre_stage_intent.intent_digest
                || receipt.transaction_id() != transaction_id
            {
                return Err(SourceEditTransactionError::ContextBinding(
                    "pre-stage abort completion differs from its receipt".to_string(),
                ));
            }
            continue;
        }

        let recovery = if managed_entry_exists(&abort_path)? {
            let receipt: SourceEditPreStageAbortReceiptV1 =
                read_json(&abort_path, "pre_stage_pending_abort_read")?;
            validate_pre_stage_abort_receipt(&receipt)?;
            if receipt.transaction_id() != transaction_id {
                return Err(SourceEditTransactionError::ContextBinding(
                    "pre-stage abort receipt differs from its marker identity".to_string(),
                ));
            }
            SourceEditPreStageRecoveryV1 {
                transaction_id: transaction_id.clone(),
                operation_object_digest: receipt
                    .core
                    .pre_stage_intent
                    .core
                    .operation_object_digest
                    .clone(),
                intent_digest: receipt.core.pre_stage_intent.intent_digest.clone(),
            }
        } else {
            let intent = load_pre_stage_intent_from_root(&root, &transaction_id)?;
            let directory = root.join(&transaction_id);
            if managed_entry_exists(&stage_path(&directory))?
                || managed_entry_exists(&stage_abort_receipt_path(&directory))?
                || managed_entry_exists(&stage_abort_completion_path(&directory))?
                || managed_entry_exists(&outcome_path(&directory))?
                || managed_entry_exists(&terminal_receipt_path(&directory))?
            {
                continue;
            }
            SourceEditPreStageRecoveryV1 {
                transaction_id: transaction_id.clone(),
                operation_object_digest: intent.core.operation_object_digest.clone(),
                intent_digest: intent.intent_digest,
            }
        };
        pending.insert(transaction_id, recovery);
    }
    Ok(pending)
}

fn pending_staged_recovery(
    state: &SessionState,
) -> Result<BTreeMap<String, SourceEditStagedRecoveryV1>, SourceEditTransactionError> {
    let root = canonical_runtime_root(state)?.join(SOURCE_EDIT_TX_DIRECTORY);
    if !managed_entry_exists(&root)? {
        return Ok(BTreeMap::new());
    }
    validate_private_directory(&root)?;
    let mut pending = BTreeMap::new();
    for entry in fs::read_dir(&root).map_err(|error| io_error("staged_pending_read_dir", error))? {
        let entry = entry.map_err(|error| io_error("staged_pending_entry", error))?;
        let transaction_id = entry.file_name().to_string_lossy().into_owned();
        if !is_digest(&transaction_id) {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| io_error("staged_pending_entry_type", error))?;
        if file_type.is_symlink() || !file_type.is_dir() {
            return Err(SourceEditTransactionError::Preflight(format!(
                "staged transaction entry is not a real directory: {transaction_id}"
            )));
        }
        let directory = root.join(&transaction_id);
        let abort_path = stage_abort_receipt_path(&directory);
        if managed_entry_exists(&abort_path)? {
            let receipt: SourceEditStageAbortReceiptV1 =
                read_json(&abort_path, "staged_pending_abort_receipt_read")?;
            validate_stage_abort_receipt(&receipt)?;
            if receipt.transaction_id() != transaction_id {
                return Err(SourceEditTransactionError::ContextBinding(
                    "staged abort receipt differs from its directory".to_string(),
                ));
            }
            if managed_entry_exists(&stage_abort_completion_path(&directory))? {
                let completion: SourceEditStageAbortCompletionV1 = read_json(
                    &stage_abort_completion_path(&directory),
                    "staged_pending_abort_completion_read",
                )?;
                validate_stage_abort_completion(&completion)?;
                if completion.core.abort_digest != receipt.abort_digest
                    || completion.core.stage_digest != receipt.core.stage_digest
                {
                    return Err(SourceEditTransactionError::ContextBinding(
                        "staged abort completion differs from its receipt".to_string(),
                    ));
                }
                continue;
            }
            pending.insert(
                transaction_id.clone(),
                SourceEditStagedRecoveryV1 {
                    transaction_id,
                    operation_object_digest: receipt.core.operation_object_digest.clone(),
                    stage_digest: receipt.core.stage_digest.clone(),
                },
            );
            continue;
        }
        if managed_entry_exists(&stage_abort_completion_path(&directory))? {
            return Err(SourceEditTransactionError::Preflight(
                "staged abort completion exists without its receipt".to_string(),
            ));
        }
        if !managed_entry_exists(&stage_path(&directory))? {
            continue;
        }
        let (loaded_directory, descriptor) = load_descriptor(state, &transaction_id)?;
        let staged: SourceEditStagedCommitV1 =
            read_json(&stage_path(&directory), "staged_pending_stage_read")?;
        validate_stage_descriptor_binding(&loaded_directory, &staged, &descriptor)?;
        let events = read_journal(&journal_path(&directory), &transaction_id)?;
        if !events.iter().any(|event| {
            event.core.phase == SourceEditJournalPhaseV1::Staged
                && event.event_digest == staged.core.journal_root_digest
        }) {
            return Err(SourceEditTransactionError::ContextBinding(
                "staged recovery item does not bind a durable Staged journal event".to_string(),
            ));
        }
        if existing_terminal_receipt(&directory)?.is_some() {
            continue;
        }
        pending.insert(
            transaction_id.clone(),
            SourceEditStagedRecoveryV1 {
                transaction_id,
                operation_object_digest: staged.core.operation_object_digest.clone(),
                stage_digest: staged.stage_digest.clone(),
            },
        );
    }
    Ok(pending)
}

fn abort_pre_stage_without_target_write<F: SourceEditFaults>(
    state: &SessionState,
    transaction_id: &str,
    operation_object_digest: &str,
    intent_digest: &str,
    faults: &F,
) -> Result<SourceEditPreStageAbortReceiptV1, SourceEditTransactionError> {
    if !is_digest(transaction_id)
        || !is_digest(operation_object_digest)
        || !is_digest(intent_digest)
    {
        return Err(SourceEditTransactionError::ContextBinding(
            "pre-stage abort requires canonical transaction, operation, and intent digests"
                .to_string(),
        ));
    }
    let root = canonical_runtime_root(state)?.join(SOURCE_EDIT_TX_DIRECTORY);
    validate_private_directory(&root)?;
    let receipt_path = pre_stage_abort_receipt_path(&root, transaction_id);
    let receipt = if managed_entry_exists(&receipt_path)? {
        let receipt: SourceEditPreStageAbortReceiptV1 =
            read_json(&receipt_path, "pre_stage_abort_replay_read")?;
        validate_pre_stage_abort_receipt(&receipt)?;
        validate_pre_stage_abort_request(
            &receipt,
            transaction_id,
            operation_object_digest,
            intent_digest,
        )?;
        receipt
    } else {
        let intent = load_pre_stage_intent_from_root(&root, transaction_id)?;
        if intent.core.operation_object_digest != operation_object_digest
            || intent.intent_digest != intent_digest
        {
            return Err(SourceEditTransactionError::ContextBinding(
                "pre-stage abort differs from the first-write intent".to_string(),
            ));
        }
        refuse_durable_stage_for_pre_stage_abort(&root, &intent)?;
        let receipt = seal_pre_stage_abort_receipt(SourceEditPreStageAbortReceiptCoreV1 {
            schema: SOURCE_EDIT_PRE_STAGE_ABORT_SCHEMA.to_string(),
            pre_stage_intent: intent,
            target_bytes_observed: false,
            target_write_performed: false,
            coordination_state_mutated: false,
            aborted_at_ms: now_ms()?,
        })?;
        durable_json_new(&receipt_path, &receipt)?;
        faults.hit(
            transaction_id,
            SourceEditFailpointV1::PreStageAbortMarkerDurable,
        )?;
        receipt
    };
    cleanup_aborted_pre_stage(&root, &receipt, faults)?;
    Ok(receipt)
}

fn validate_pre_stage_abort_request(
    receipt: &SourceEditPreStageAbortReceiptV1,
    transaction_id: &str,
    operation_object_digest: &str,
    intent_digest: &str,
) -> Result<(), SourceEditTransactionError> {
    let intent = &receipt.core.pre_stage_intent;
    if intent.core.transaction_id != transaction_id
        || intent.core.operation_object_digest != operation_object_digest
        || intent.intent_digest != intent_digest
    {
        return Err(SourceEditTransactionError::ContextBinding(
            "pre-stage abort receipt differs from the exact recovery request".to_string(),
        ));
    }
    Ok(())
}

fn refuse_durable_stage_for_pre_stage_abort(
    root: &Path,
    intent: &SourceEditPreStageIntentV1,
) -> Result<(), SourceEditTransactionError> {
    let directory = root.join(&intent.core.transaction_id);
    for forbidden in [
        stage_path(&directory),
        outcome_path(&directory),
        terminal_receipt_path(&directory),
        stage_abort_receipt_path(&directory),
        stage_abort_completion_path(&directory),
    ] {
        if managed_entry_exists(&forbidden)? {
            return Err(SourceEditTransactionError::RecoveryRequired {
                transaction_id: intent.core.transaction_id.clone(),
                detail: format!(
                    "pre-stage fallback refused post-pre-stage artifact '{}'",
                    forbidden.display()
                ),
            });
        }
    }
    Ok(())
}

fn cleanup_aborted_pre_stage<F: SourceEditFaults>(
    root: &Path,
    receipt: &SourceEditPreStageAbortReceiptV1,
    faults: &F,
) -> Result<(), SourceEditTransactionError> {
    validate_pre_stage_abort_receipt(receipt)?;
    let intent = &receipt.core.pre_stage_intent;
    let transaction_id = &intent.core.transaction_id;
    let directory = root.join(transaction_id);
    if Path::new(&intent.core.transaction_directory) != directory {
        return Err(SourceEditTransactionError::ContextBinding(
            "pre-stage abort transaction directory escapes its canonical root".to_string(),
        ));
    }
    let completion_path = pre_stage_abort_completion_path(root, transaction_id);
    if managed_entry_exists(&completion_path)? {
        let completion: SourceEditPreStageAbortCompletionV1 =
            read_json(&completion_path, "pre_stage_abort_completion_replay_read")?;
        validate_pre_stage_abort_completion(&completion)?;
        if completion.core.transaction_id != *transaction_id
            || completion.core.operation_object_digest != intent.core.operation_object_digest
            || completion.core.intent_digest != intent.intent_digest
            || completion.core.abort_digest != receipt.abort_digest
        {
            return Err(SourceEditTransactionError::ContextBinding(
                "pre-stage abort completion differs from its receipt".to_string(),
            ));
        }
        return Ok(());
    }
    refuse_durable_stage_for_pre_stage_abort(root, intent)?;

    let managed_root = validate_abort_managed_root(&intent.core.managed_root)?;
    let candidate_path = Path::new(&intent.core.candidate_temp_path);
    if managed_entry_exists(candidate_path)? {
        let candidate = read_target_snapshot(&managed_root, candidate_path)?;
        if candidate.sha256 != intent.core.candidate_sha256
            || candidate.bytes.len() as u64 != intent.core.bytes_after
            || candidate.permissions != intent.core.permissions_before
        {
            return Err(SourceEditTransactionError::ManualRecovery {
                transaction_id: transaction_id.clone(),
                detail: "pre-stage candidate differs from the first-write intent".to_string(),
            });
        }
        remove_file_if_exists(candidate_path)?;
    }
    faults.hit(
        transaction_id,
        SourceEditFailpointV1::PreStageAbortCandidateRemoved,
    )?;

    if managed_entry_exists(Path::new(&intent.core.rollback_temp_path))? {
        return Err(SourceEditTransactionError::RecoveryRequired {
            transaction_id: transaction_id.clone(),
            detail: "pre-stage fallback found a rollback artifact".to_string(),
        });
    }
    let backup_path = Path::new(&intent.core.backup_path);
    if managed_entry_exists(backup_path)? {
        read_exact_abort_private_file(
            backup_path,
            intent.core.bytes_before,
            &intent.core.source_sha256_before,
            transaction_id,
        )?;
        remove_file_if_exists(backup_path)?;
    }
    faults.hit(
        transaction_id,
        SourceEditFailpointV1::PreStageAbortBackupRemoved,
    )?;

    let source_journal_path = journal_path(&directory);
    remove_file_if_exists(&source_journal_path)?;
    faults.hit(
        transaction_id,
        SourceEditFailpointV1::PreStageAbortJournalRemoved,
    )?;

    let source_descriptor_path = descriptor_path(&directory);
    if managed_entry_exists(&source_descriptor_path)? {
        let descriptor: SourceEditDescriptorV1 =
            read_json(&source_descriptor_path, "pre_stage_abort_descriptor_read")?;
        validate_descriptor(&descriptor)?;
        if descriptor != intent.core.descriptor {
            return Err(SourceEditTransactionError::ContextBinding(
                "pre-stage descriptor differs from the first-write intent".to_string(),
            ));
        }
        remove_file_if_exists(&source_descriptor_path)?;
    }
    faults.hit(
        transaction_id,
        SourceEditFailpointV1::PreStageAbortDescriptorRemoved,
    )?;

    if managed_entry_exists(&directory)? {
        validate_private_directory(&directory)?;
        let mut entries = fs::read_dir(&directory)
            .map_err(|error| io_error("pre_stage_abort_directory_read", error))?;
        if entries.next().is_some() {
            return Err(SourceEditTransactionError::RecoveryRequired {
                transaction_id: transaction_id.clone(),
                detail: "pre-stage transaction directory contains an unknown artifact".to_string(),
            });
        }
        fs::remove_dir(&directory)
            .map_err(|error| io_error("pre_stage_abort_remove_directory", error))?;
        fsync_parent(&directory)?;
    }
    faults.hit(
        transaction_id,
        SourceEditFailpointV1::PreStageAbortDirectoryRemoved,
    )?;

    let intent_path = pre_stage_intent_path(root, transaction_id);
    if managed_entry_exists(&intent_path)? {
        let existing: SourceEditPreStageIntentV1 =
            read_json(&intent_path, "pre_stage_abort_intent_cleanup_read")?;
        if existing != *intent {
            return Err(SourceEditTransactionError::ContextBinding(
                "pre-stage intent marker differs from the abort receipt".to_string(),
            ));
        }
        remove_file_if_exists(&intent_path)?;
    }
    faults.hit(
        transaction_id,
        SourceEditFailpointV1::PreStageAbortIntentRemoved,
    )?;

    let completion = seal_pre_stage_abort_completion(SourceEditPreStageAbortCompletionCoreV1 {
        schema: SOURCE_EDIT_PRE_STAGE_ABORT_COMPLETION_SCHEMA.to_string(),
        transaction_id: transaction_id.clone(),
        operation_object_digest: intent.core.operation_object_digest.clone(),
        intent_digest: intent.intent_digest.clone(),
        abort_digest: receipt.abort_digest.clone(),
        target_bytes_observed: false,
        target_write_performed: false,
        coordination_state_mutated: false,
        completed_at_ms: now_ms()?,
    })?;
    durable_json_new(&completion_path, &completion)?;
    faults.hit(
        transaction_id,
        SourceEditFailpointV1::PreStageAbortCompletionDurable,
    )?;
    Ok(())
}

fn abort_staged_without_target_write(
    state: &SessionState,
    transaction_id: &str,
    operation_object_digest: &str,
    stage_digest: &str,
) -> Result<SourceEditStageAbortReceiptV1, SourceEditTransactionError> {
    abort_staged_without_target_write_with_faults(
        state,
        transaction_id,
        operation_object_digest,
        stage_digest,
        &NoSourceEditFaults,
    )
}

fn abort_staged_without_target_write_with_faults<F: SourceEditFaults>(
    state: &SessionState,
    transaction_id: &str,
    operation_object_digest: &str,
    stage_digest: &str,
    faults: &F,
) -> Result<SourceEditStageAbortReceiptV1, SourceEditTransactionError> {
    if !is_digest(transaction_id) || !is_digest(operation_object_digest) || !is_digest(stage_digest)
    {
        return Err(SourceEditTransactionError::ContextBinding(
            "stage abort requires canonical transaction, operation, and stage digests".to_string(),
        ));
    }
    let directory = transaction_dir(state, transaction_id)?;
    let abort_path = stage_abort_receipt_path(&directory);
    let receipt = if managed_entry_exists(&abort_path)? {
        let receipt: SourceEditStageAbortReceiptV1 =
            read_json(&abort_path, "abort_receipt_replay_read")?;
        validate_stage_abort_receipt(&receipt)?;
        validate_abort_request_binding(
            &receipt,
            transaction_id,
            operation_object_digest,
            stage_digest,
        )?;
        receipt
    } else {
        let (loaded_directory, descriptor) = load_descriptor(state, transaction_id)?;
        if loaded_directory != directory
            || descriptor.core.operation_object_digest != operation_object_digest
        {
            return Err(SourceEditTransactionError::ContextBinding(
                "abort operation digest differs from the sealed descriptor".to_string(),
            ));
        }
        let staged: SourceEditStagedCommitV1 =
            read_json(&stage_path(&directory), "abort_stage_read")?;
        validate_stage_descriptor_binding(&directory, &staged, &descriptor)?;
        if staged.stage_digest != stage_digest
            || validated_stage_journal_root(&directory, &staged)?
                != SourceEditJournalPhaseV1::Staged
        {
            return Err(SourceEditTransactionError::ContextBinding(
                "abort requires the exact unpublished durable stage".to_string(),
            ));
        }
        refuse_post_stage_abort_artifacts(&directory, transaction_id)?;
        validate_abort_artifact_paths(&directory, &descriptor)?;
        let managed_root = validate_abort_managed_root(&descriptor.core.managed_root)?;
        read_exact_backup(&descriptor)?;
        read_exact_candidate(&managed_root, &descriptor)?;
        if managed_entry_exists(Path::new(&descriptor.core.rollback_temp_path))? {
            return Err(SourceEditTransactionError::ManualRecovery {
                transaction_id: transaction_id.to_string(),
                detail: "PREPARED abort found a rollback artifact".to_string(),
            });
        }
        let receipt = seal_stage_abort_receipt(SourceEditStageAbortReceiptCoreV1 {
            schema: SOURCE_EDIT_STAGE_ABORT_SCHEMA.to_string(),
            transaction_id: transaction_id.to_string(),
            operation_object_digest: operation_object_digest.to_string(),
            semantic_payload_digest: descriptor.core.semantic_payload_digest.clone(),
            stage_digest: stage_digest.to_string(),
            descriptor_digest: descriptor.descriptor_digest.clone(),
            journal_root_digest: staged.core.journal_root_digest.clone(),
            managed_root: descriptor.core.managed_root.clone(),
            target_identity: descriptor.core.target_identity.clone(),
            source_sha256_before: descriptor.core.source_sha256_before.clone(),
            candidate_sha256: descriptor.core.candidate_sha256.clone(),
            bytes_before: descriptor.core.bytes_before,
            bytes_after: descriptor.core.bytes_after,
            permissions_before: descriptor.core.permissions_before.clone(),
            candidate_temp_path: descriptor.core.candidate_temp_path.clone(),
            rollback_temp_path: descriptor.core.rollback_temp_path.clone(),
            backup_path: descriptor.core.backup_path.clone(),
            target_bytes_observed: false,
            target_write_performed: false,
            coordination_state_mutated: false,
            aborted_at_ms: now_ms()?,
        })?;
        durable_json_new(&abort_path, &receipt)?;
        faults.hit(transaction_id, SourceEditFailpointV1::AbortMarkerDurable)?;
        receipt
    };
    cleanup_aborted_stage(&directory, &receipt, faults)?;
    Ok(receipt)
}

fn validate_abort_request_binding(
    receipt: &SourceEditStageAbortReceiptV1,
    transaction_id: &str,
    operation_object_digest: &str,
    stage_digest: &str,
) -> Result<(), SourceEditTransactionError> {
    if receipt.core.transaction_id != transaction_id
        || receipt.core.operation_object_digest != operation_object_digest
        || receipt.core.stage_digest != stage_digest
    {
        return Err(SourceEditTransactionError::ContextBinding(
            "stage abort receipt differs from the exact recovery request".to_string(),
        ));
    }
    Ok(())
}

fn managed_entry_exists(path: &Path) -> Result<bool, SourceEditTransactionError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error("managed_entry_lstat", error)),
    }
}

fn refuse_post_stage_abort_artifacts(
    directory: &Path,
    transaction_id: &str,
) -> Result<(), SourceEditTransactionError> {
    for forbidden in [outcome_path(directory), terminal_receipt_path(directory)] {
        if managed_entry_exists(&forbidden)? {
            return Err(SourceEditTransactionError::ManualRecovery {
                transaction_id: transaction_id.to_string(),
                detail: format!(
                    "PREPARED abort found a post-stage artifact '{}'",
                    forbidden.display()
                ),
            });
        }
    }
    Ok(())
}

fn validate_abort_managed_root(raw: &str) -> Result<PathBuf, SourceEditTransactionError> {
    let root = PathBuf::from(raw);
    let metadata =
        fs::symlink_metadata(&root).map_err(|error| io_error("abort_managed_root_lstat", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(SourceEditTransactionError::Preflight(
            "abort managed root is not a real directory".to_string(),
        ));
    }
    Ok(root)
}

fn validate_abort_artifact_paths(
    directory: &Path,
    descriptor: &SourceEditDescriptorV1,
) -> Result<(), SourceEditTransactionError> {
    let target = Path::new(&descriptor.core.target_identity);
    let expected_candidate = target
        .parent()
        .ok_or_else(|| SourceEditTransactionError::Preflight("target has no parent".to_string()))?
        .join(format!(
            ".m1nd-source-edit-{}.candidate",
            descriptor.core.transaction_id
        ));
    let expected_rollback = expected_candidate.with_extension("rollback");
    if Path::new(&descriptor.core.candidate_temp_path) != expected_candidate
        || Path::new(&descriptor.core.rollback_temp_path) != expected_rollback
        || Path::new(&descriptor.core.backup_path) != directory.join("before.bytes")
        || Path::new(&descriptor.core.candidate_temp_path) == target
        || Path::new(&descriptor.core.rollback_temp_path) == target
    {
        return Err(SourceEditTransactionError::ContextBinding(
            "stage abort artifact paths differ from the canonical transaction layout".to_string(),
        ));
    }
    Ok(())
}

fn validate_abort_receipt_paths(
    directory: &Path,
    receipt: &SourceEditStageAbortReceiptV1,
) -> Result<(), SourceEditTransactionError> {
    let target = Path::new(&receipt.core.target_identity);
    let expected_candidate = target
        .parent()
        .ok_or_else(|| SourceEditTransactionError::Preflight("target has no parent".to_string()))?
        .join(format!(
            ".m1nd-source-edit-{}.candidate",
            receipt.core.transaction_id
        ));
    if Path::new(&receipt.core.candidate_temp_path) != expected_candidate
        || Path::new(&receipt.core.rollback_temp_path)
            != expected_candidate.with_extension("rollback")
        || Path::new(&receipt.core.backup_path) != directory.join("before.bytes")
        || Path::new(&receipt.core.candidate_temp_path) == target
        || Path::new(&receipt.core.rollback_temp_path) == target
    {
        return Err(SourceEditTransactionError::ContextBinding(
            "stage abort receipt paths differ from the canonical transaction layout".to_string(),
        ));
    }
    Ok(())
}

fn read_exact_abort_private_file(
    path: &Path,
    expected_len: u64,
    expected_sha256: &str,
    transaction_id: &str,
) -> Result<(), SourceEditTransactionError> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| io_error("abort_file_lstat", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(SourceEditTransactionError::Preflight(format!(
            "abort artifact '{}' is not a regular file",
            path.display()
        )));
    }
    let bytes = fs::read(path).map_err(|error| io_error("abort_file_read", error))?;
    if bytes.len() as u64 != expected_len || sha256_hex(&bytes) != expected_sha256 {
        return Err(SourceEditTransactionError::ManualRecovery {
            transaction_id: transaction_id.to_string(),
            detail: format!(
                "abort artifact '{}' failed exact validation",
                path.display()
            ),
        });
    }
    Ok(())
}

fn cleanup_aborted_stage<F: SourceEditFaults>(
    directory: &Path,
    receipt: &SourceEditStageAbortReceiptV1,
    faults: &F,
) -> Result<(), SourceEditTransactionError> {
    validate_stage_abort_receipt(receipt)?;
    validate_abort_receipt_paths(directory, receipt)?;
    refuse_post_stage_abort_artifacts(directory, receipt.transaction_id())?;
    let completion_path = stage_abort_completion_path(directory);
    if managed_entry_exists(&completion_path)? {
        let completion: SourceEditStageAbortCompletionV1 =
            read_json(&completion_path, "abort_completion_replay_read")?;
        validate_stage_abort_completion(&completion)?;
        if completion.core.transaction_id != receipt.core.transaction_id
            || completion.core.operation_object_digest != receipt.core.operation_object_digest
            || completion.core.stage_digest != receipt.core.stage_digest
            || completion.core.descriptor_digest != receipt.core.descriptor_digest
            || completion.core.abort_digest != receipt.abort_digest
        {
            return Err(SourceEditTransactionError::ContextBinding(
                "stage abort completion differs from the sealed abort receipt".to_string(),
            ));
        }
        return Ok(());
    }

    let managed_root = validate_abort_managed_root(&receipt.core.managed_root)?;
    let candidate_path = Path::new(&receipt.core.candidate_temp_path);
    if managed_entry_exists(candidate_path)? {
        let candidate = read_target_snapshot(&managed_root, candidate_path)?;
        if candidate.sha256 != receipt.core.candidate_sha256
            || candidate.bytes.len() as u64 != receipt.core.bytes_after
            || candidate.permissions != receipt.core.permissions_before
        {
            return Err(SourceEditTransactionError::ManualRecovery {
                transaction_id: receipt.core.transaction_id.clone(),
                detail: "abort candidate differs from the sealed artifact".to_string(),
            });
        }
        remove_file_if_exists(candidate_path)?;
    }
    faults.hit(
        receipt.transaction_id(),
        SourceEditFailpointV1::AbortCandidateRemoved,
    )?;

    let rollback_path = Path::new(&receipt.core.rollback_temp_path);
    if managed_entry_exists(rollback_path)? {
        let rollback = read_target_snapshot(&managed_root, rollback_path)?;
        if rollback.sha256 != receipt.core.source_sha256_before
            || rollback.bytes.len() as u64 != receipt.core.bytes_before
            || rollback.permissions != receipt.core.permissions_before
        {
            return Err(SourceEditTransactionError::ManualRecovery {
                transaction_id: receipt.core.transaction_id.clone(),
                detail: "abort rollback temp differs from the sealed before-image".to_string(),
            });
        }
        remove_file_if_exists(rollback_path)?;
    }
    faults.hit(
        receipt.transaction_id(),
        SourceEditFailpointV1::AbortRollbackRemoved,
    )?;

    let backup_path = Path::new(&receipt.core.backup_path);
    if managed_entry_exists(backup_path)? {
        read_exact_abort_private_file(
            backup_path,
            receipt.core.bytes_before,
            &receipt.core.source_sha256_before,
            receipt.transaction_id(),
        )?;
        remove_file_if_exists(backup_path)?;
    }
    faults.hit(
        receipt.transaction_id(),
        SourceEditFailpointV1::AbortBackupRemoved,
    )?;

    let staged_path = stage_path(directory);
    if managed_entry_exists(&staged_path)? {
        let staged: SourceEditStagedCommitV1 = read_json(&staged_path, "abort_stage_cleanup_read")?;
        validate_stage(&staged)?;
        if staged.stage_digest != receipt.core.stage_digest
            || staged.core.transaction_id != receipt.core.transaction_id
            || staged.core.operation_object_digest != receipt.core.operation_object_digest
            || staged.core.descriptor_digest != receipt.core.descriptor_digest
        {
            return Err(SourceEditTransactionError::ContextBinding(
                "stage artifact differs from the abort receipt".to_string(),
            ));
        }
        remove_file_if_exists(&staged_path)?;
    }
    faults.hit(
        receipt.transaction_id(),
        SourceEditFailpointV1::AbortStageRemoved,
    )?;

    let source_journal_path = journal_path(directory);
    if managed_entry_exists(&source_journal_path)? {
        let events = read_journal(&source_journal_path, receipt.transaction_id())?;
        let last = events.last().ok_or_else(|| {
            SourceEditTransactionError::Preflight("stage abort journal is empty".to_string())
        })?;
        if last.core.phase != SourceEditJournalPhaseV1::Staged
            || last.event_digest != receipt.core.journal_root_digest
        {
            return Err(SourceEditTransactionError::ContextBinding(
                "stage abort journal root differs from the abort receipt".to_string(),
            ));
        }
        remove_file_if_exists(&source_journal_path)?;
    }
    faults.hit(
        receipt.transaction_id(),
        SourceEditFailpointV1::AbortJournalRemoved,
    )?;

    let source_descriptor_path = descriptor_path(directory);
    if managed_entry_exists(&source_descriptor_path)? {
        let descriptor: SourceEditDescriptorV1 =
            read_json(&source_descriptor_path, "abort_descriptor_cleanup_read")?;
        validate_descriptor(&descriptor)?;
        if descriptor.core.transaction_id != receipt.core.transaction_id
            || descriptor.core.operation_object_digest != receipt.core.operation_object_digest
            || descriptor.descriptor_digest != receipt.core.descriptor_digest
        {
            return Err(SourceEditTransactionError::ContextBinding(
                "descriptor artifact differs from the abort receipt".to_string(),
            ));
        }
        remove_file_if_exists(&source_descriptor_path)?;
    }
    faults.hit(
        receipt.transaction_id(),
        SourceEditFailpointV1::AbortDescriptorRemoved,
    )?;

    let completion = seal_stage_abort_completion(SourceEditStageAbortCompletionCoreV1 {
        schema: SOURCE_EDIT_STAGE_ABORT_COMPLETION_SCHEMA.to_string(),
        transaction_id: receipt.core.transaction_id.clone(),
        operation_object_digest: receipt.core.operation_object_digest.clone(),
        stage_digest: receipt.core.stage_digest.clone(),
        descriptor_digest: receipt.core.descriptor_digest.clone(),
        abort_digest: receipt.abort_digest.clone(),
        target_bytes_observed: false,
        target_write_performed: false,
        coordination_state_mutated: false,
        completed_at_ms: now_ms()?,
    })?;
    durable_json_new(&completion_path, &completion)?;
    faults.hit(
        receipt.transaction_id(),
        SourceEditFailpointV1::AbortCompletionDurable,
    )?;
    Ok(())
}

fn pending_recovery(
    state: &SessionState,
) -> Result<BTreeMap<String, String>, SourceEditTransactionError> {
    let root = canonical_runtime_root(state)?.join(SOURCE_EDIT_TX_DIRECTORY);
    if !root.exists() {
        return Ok(BTreeMap::new());
    }
    let metadata =
        fs::symlink_metadata(&root).map_err(|error| io_error("pending_root_lstat", error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(SourceEditTransactionError::Preflight(
            "pending transaction root is not a real directory".to_string(),
        ));
    }
    let pre_stage = pending_pre_stage_recovery(state)?;
    let mut pending = pre_stage
        .iter()
        .map(|(transaction_id, recovery)| {
            (
                transaction_id.clone(),
                recovery.operation_object_digest.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for entry in fs::read_dir(&root).map_err(|error| io_error("pending_read_dir", error))? {
        let entry = entry.map_err(|error| io_error("pending_dir_entry", error))?;
        let transaction_id = entry.file_name().to_string_lossy().into_owned();
        if transaction_id == ".locks" {
            let metadata = entry
                .file_type()
                .map_err(|error| io_error("pending_lock_dir_type", error))?;
            if metadata.is_symlink() || !metadata.is_dir() {
                return Err(SourceEditTransactionError::Preflight(
                    "source edit lock root is not a real directory".to_string(),
                ));
            }
            continue;
        }
        if marker_transaction_id(&transaction_id, PRE_STAGE_INTENT_PREFIX).is_some()
            || marker_transaction_id(&transaction_id, PRE_STAGE_ABORT_PREFIX).is_some()
            || marker_transaction_id(&transaction_id, PRE_STAGE_ABORT_COMPLETION_PREFIX).is_some()
        {
            continue;
        }
        if !is_digest(&transaction_id) {
            return Err(SourceEditTransactionError::Preflight(format!(
                "unexpected entry in transaction root: {transaction_id}"
            )));
        }
        let directory = root.join(&transaction_id);
        if pre_stage.contains_key(&transaction_id)
            && !managed_entry_exists(&descriptor_path(&directory))?
        {
            continue;
        }
        let abort_path = stage_abort_receipt_path(&directory);
        if managed_entry_exists(&abort_path)? {
            let receipt: SourceEditStageAbortReceiptV1 =
                read_json(&abort_path, "pending_abort_receipt_read")?;
            validate_stage_abort_receipt(&receipt)?;
            if receipt.core.transaction_id != transaction_id {
                return Err(SourceEditTransactionError::ContextBinding(
                    "abort receipt transaction differs from its directory".to_string(),
                ));
            }
            let completion_path = stage_abort_completion_path(&directory);
            if managed_entry_exists(&completion_path)? {
                let completion: SourceEditStageAbortCompletionV1 =
                    read_json(&completion_path, "pending_abort_completion_read")?;
                validate_stage_abort_completion(&completion)?;
                if completion.core.abort_digest != receipt.abort_digest
                    || completion.core.transaction_id != transaction_id
                {
                    return Err(SourceEditTransactionError::ContextBinding(
                        "abort completion differs from its receipt".to_string(),
                    ));
                }
            } else {
                pending.insert(transaction_id, receipt.core.operation_object_digest);
            }
            continue;
        }
        if managed_entry_exists(&stage_abort_completion_path(&directory))? {
            return Err(SourceEditTransactionError::Preflight(
                "abort completion exists without its sealed receipt".to_string(),
            ));
        }
        let (directory, descriptor) = load_descriptor(state, &transaction_id)?;
        let phase = last_journal_phase(&directory, &transaction_id)?;
        if existing_terminal_receipt(&directory)?.is_none() || !journal_is_terminal(phase) {
            pending.insert(transaction_id, descriptor.core.operation_object_digest);
        }
    }
    Ok(pending)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority_runtime::{
        AuthorityAuthorizationReceiptCoreV1, AuthorityAuthorizationReceiptV1,
        AuthorityRuntimeStateCoreV1, AuthorityRuntimeStateV1, AuthorityRuntimeStatusV1,
        AuthorityVerificationAssurance, AuthorizationAuthorityV1, ProtectedEpochAssurance,
    };
    use crate::owner_authorization_broker::{
        AuthorizationLeaseStateV1, ExternalMutationCommitWitnessV1, OwnerAuthorityLinearizationV1,
        OwnerAuthorizationBrokerConfigV1, OwnerAuthorizationBrokerV1,
        VerifiedExternalMutationCommitWitnessV1,
    };
    use crate::protocol::surgical::EditPreviewInput;
    use crate::server::McpConfig;
    use m1nd_control::{
        ActionId, ActiveMode, AuthorityVariant, CapabilityKind, ReachablePolicyTupleV1, RiskClass,
        Role,
    };
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::Graph;

    const SUBJECT: &str = "agent-source-editor";
    const BEFORE: &str = "pub fn before() -> u8 { 1 }\n";
    const AFTER: &str = "pub fn after() -> u8 { 2 }\n";
    const BROKER_NOW: u64 = 10_000;

    struct Fixture {
        _temp: tempfile::TempDir,
        cell: BrainSessionCell,
        target: PathBuf,
        request: SourceEditCommitRequestV1,
    }

    #[derive(Clone, Copy)]
    struct CrashAt(SourceEditFailpointV1);

    impl SourceEditFaults for CrashAt {
        fn hit(
            &self,
            transaction_id: &str,
            point: SourceEditFailpointV1,
        ) -> Result<(), SourceEditTransactionError> {
            if point == self.0 {
                return Err(SourceEditTransactionError::InjectedCrash {
                    transaction_id: transaction_id.to_string(),
                    phase: point.name(),
                });
            }
            Ok(())
        }
    }

    fn fixture() -> Fixture {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo = temp.path().join("repo");
        let runtime = temp.path().join("runtime");
        let target = repo.join("src/lib.rs");
        fs::create_dir_all(target.parent().expect("target parent")).expect("source tree");
        fs::create_dir_all(&runtime).expect("runtime");
        fs::write(&target, BEFORE).expect("before bytes");
        #[cfg(unix)]
        fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).expect("target mode");
        let config = McpConfig {
            graph_source: runtime.join("graph.json"),
            plasticity_state: runtime.join("plasticity.json"),
            runtime_dir: Some(runtime),
            ..McpConfig::default()
        };
        let mut state =
            SessionState::initialize(Graph::new(), &config, DomainConfig::code()).expect("state");
        state.ingest_roots = vec![path_text(&repo)];
        state.workspace_root = Some(path_text(&repo));
        let preview = super::super::handle_edit_preview(
            &mut state,
            EditPreviewInput {
                file_path: path_text(&target),
                agent_id: SUBJECT.to_string(),
                new_content: AFTER.to_string(),
                description: Some("typed transaction test".to_string()),
            },
        )
        .expect("preview");
        state
            .note_proof_ready(SUBJECT, &path_text(&target), "test")
            .expect("proof");
        Fixture {
            _temp: temp,
            cell: BrainSessionCell::new(state),
            target,
            request: SourceEditCommitRequestV1 {
                schema: SOURCE_EDIT_COMMIT_REQUEST_SCHEMA.to_string(),
                preview_id: preview.preview_id,
            },
        }
    }

    fn restarted_state(fixture: &Fixture) -> SessionState {
        fixture
            .cell
            .release_hosted_instance_after_actor_stop()
            .expect("release the simulated dead owner before restart");
        let repo = fixture
            .target
            .parent()
            .and_then(Path::parent)
            .expect("repo root")
            .to_path_buf();
        let runtime = fixture._temp.path().join("runtime");
        let config = McpConfig {
            graph_source: runtime.join("graph.json"),
            plasticity_state: runtime.join("plasticity.json"),
            runtime_dir: Some(runtime),
            ..McpConfig::default()
        };
        let mut state =
            SessionState::initialize(Graph::new(), &config, DomainConfig::code()).expect("restart");
        state.ingest_roots = vec![path_text(&repo)];
        state.workspace_root = Some(path_text(&repo));
        assert!(state.edit_previews.is_empty());
        assert!(state.proof_ready.is_empty());
        assert!(state.active_proof_permits.is_empty());
        state
    }

    fn broker_hash(label: &str) -> String {
        canonical_digest("source-edit-broker-boundary-test-v1", &label).expect("digest")
    }

    fn source_edit_receipt_and_status(
        operation_object_digest: &str,
        complete_effects: BTreeSet<Effect>,
    ) -> (AuthorityAuthorizationReceiptV1, AuthorityRuntimeStatusV1) {
        let action = ActionId::new(SOURCE_EDIT_COMMIT_ACTION).expect("action");
        let receipt = AuthorityAuthorizationReceiptV1::new_for_broker_test(
            AuthorityAuthorizationReceiptCoreV1 {
                organism_id: "organism-source-edit-test".to_string(),
                repo_id: "repo-source-edit-test".to_string(),
                brain_id: "project-brain-test".to_string(),
                subject_id: SUBJECT.to_string(),
                role: Role::Author,
                capability_id: "capability-source-edit-test".to_string(),
                capability_kind: Some(CapabilityKind::Human),
                verified_object_digest: operation_object_digest.to_string(),
                mission_id: Some("mission-test".to_string()),
                mission_head_id: Some("head-test".to_string()),
                transport_session_id: "source-edit-transport".to_string(),
                ingress_context_digest: broker_hash("ingress"),
                action: action.clone(),
                ingress: Ingress::Mcp,
                complete_effects,
                active_mode: ActiveMode::HumanGated,
                constitution_digest: broker_hash("constitution"),
                constitution_epoch: 7,
                autonomy_epoch: 0,
                protected_epoch_at_decision: 11,
                policy_registry_digest: broker_hash("policy"),
                exact_policy_tuple: ReachablePolicyTupleV1 {
                    ingress: Ingress::Mcp,
                    action,
                    active_mode: ActiveMode::HumanGated,
                    subject_id: SUBJECT.to_string(),
                    authority_variant: AuthorityVariant::Human,
                    applicable_grant_id: None,
                    applicable_tier: None,
                    risk_class: RiskClass::Critical,
                },
                authority_decision_digest: Some(broker_hash("decision")),
                autonomy_admission_receipt_digest: None,
                autonomy_committed_state_digest: None,
                autonomy_protected_root_digest: None,
                authority: AuthorizationAuthorityV1::Positive {
                    variant: AuthorityVariant::Human,
                    assurance: AuthorityVerificationAssurance::SoftwareTestOnlyNotProven,
                },
                authority_body_digest: broker_hash("body"),
                replay_sequence: 3,
                journal_sequence: 11,
                journal_root_digest: broker_hash("journal"),
                protected_epoch: 11,
                authorized_at: BROKER_NOW,
                expires_at: BROKER_NOW + 1_000,
            },
        );
        let core = &receipt.core;
        let status = AuthorityRuntimeStatusV1 {
            state: AuthorityRuntimeStateV1::new_for_broker_test(AuthorityRuntimeStateCoreV1 {
                organism_id: core.organism_id.clone(),
                repo_id: core.repo_id.clone(),
                brain_id: core.brain_id.clone(),
                audience: "m1nd-runtime".to_string(),
                revision: 10,
                active_mode: core.active_mode,
                activation_receipt_id: None,
                constitution_digest: core.constitution_digest.clone(),
                constitution_epoch: core.constitution_epoch,
                autonomy_epoch: core.autonomy_epoch,
                grants_digest: broker_hash("grants"),
                policy_registry_digest: core.policy_registry_digest.clone(),
                action_catalog_digest: broker_hash("catalog"),
                safety_kernel_digest: broker_hash("kernel"),
                safety_actuator_identity_key_binary_policy_digest: broker_hash("actuator"),
                issuance_frozen: false,
                safety_state: m1nd_control::autonomy::SafetyState::Healthy,
                protected_epoch: core.protected_epoch,
                journal_sequence: core.journal_sequence,
                journal_root_digest: core.journal_root_digest.clone(),
                replay_sequence: core.replay_sequence,
                replay_root_digest: Some(broker_hash("replay")),
                updated_at: core.authorized_at,
            }),
            protected_epoch_assurance: ProtectedEpochAssurance::SoftwareTestOnlyNotProven,
            positive_verification_assurance:
                AuthorityVerificationAssurance::SoftwareTestOnlyNotProven,
            semantic_catalog_entries: 1,
            transport_schema_parity_proven: false,
            multi_artifact_atomicity_proven: false,
            automatic_crash_recovery_proven: true,
        };
        (receipt, status)
    }

    fn context(intent: &SourceEditCommitIntentV1, subject: &str) -> SourceEditPreparedContextV1 {
        let mut context = SourceEditPreparedContextV1 {
            authority_subject_id: subject.to_string(),
            semantic_action: SOURCE_EDIT_COMMIT_ACTION.to_string(),
            ingress: Ingress::Mcp,
            semantic_payload_digest: intent.semantic_payload_digest.clone(),
            operation_object_digest: "0".repeat(64),
            expected_effects: SourceEditCommitAdapterV1::expected_effects(),
            brain_id: "project-brain-test".to_string(),
            mission_id: Some("mission-test".to_string()),
            mission_head_id: Some("head-test".to_string()),
            operation_version: SOURCE_EDIT_OPERATION_VERSION,
        };
        context.operation_object_digest =
            SourceEditCommitAdapterV1::operation_object_digest(intent, &context)
                .expect("operation digest");
        context
    }

    fn prepare(fixture: &Fixture) -> (PreparedSourceEditCommitV1, SourceEditPreparedContextV1) {
        let intent = SourceEditCommitAdapterV1::inspect(&fixture.cell, &fixture.request, SUBJECT)
            .expect("inspect");
        let context = context(&intent, SUBJECT);
        let prepared =
            SourceEditCommitAdapterV1::prepare(&fixture.cell, &fixture.request, &context)
                .expect("prepare");
        (prepared, context)
    }

    #[test]
    fn request_is_subject_free_and_canonical_context_is_exact() {
        let fixture = fixture();
        let decoded = serde_json::from_value::<SourceEditCommitRequestV1>(serde_json::json!({
            "schema": SOURCE_EDIT_COMMIT_REQUEST_SCHEMA,
            "preview_id": fixture.request.preview_id,
            "agent_id": SUBJECT,
        }));
        assert!(decoded.is_err(), "wire request must reject caller agent_id");
        let wrong = SourceEditCommitAdapterV1::inspect(
            &fixture.cell,
            &fixture.request,
            "different-subject",
        )
        .expect_err("subject must own preview and proof");
        assert!(matches!(
            wrong,
            SourceEditTransactionError::ContextBinding(_)
        ));

        let intent = SourceEditCommitAdapterV1::inspect(&fixture.cell, &fixture.request, SUBJECT)
            .expect("inspect");
        assert_eq!(
            intent.semantic_payload.expected_target_sha256,
            sha256_hex(BEFORE.as_bytes())
        );
        assert_eq!(
            intent.semantic_payload.candidate_sha256,
            sha256_hex(AFTER.as_bytes())
        );
        assert!(is_digest(&intent.semantic_payload.proof_scope_digest));
        assert!(is_digest(&intent.semantic_payload.proof_mark_digest));
        let mut context = context(&intent, SUBJECT);
        context.expected_effects.remove(&Effect::CoordinationRecord);
        assert!(
            SourceEditCommitAdapterV1::prepare(&fixture.cell, &fixture.request, &context)
                .expect_err("incomplete effects must refuse")
                .to_string()
                .contains("complete effects")
        );
    }

    #[test]
    fn stage_is_non_mutating_then_publish_is_atomic_replay_safe_and_graph_explicit() {
        let fixture = fixture();
        let (prepared, _context) = prepare(&fixture);
        let stage_replay = prepared.clone();
        let transaction_id = prepared.transaction_id().to_string();
        let before_stage = fs::metadata(&fixture.target).expect("before stage metadata");
        let mut state = fixture
            .cell
            .lock_mut_before_actor()
            .expect("source-edit fixture is pre-actor");
        assert!(state
            .edit_previews
            .contains_key(&fixture.request.preview_id));
        assert!(state.is_proof_ready(SUBJECT, &path_text(&fixture.target)));
        assert!(state.active_proof_permits.is_empty());
        let staged = prepared.stage(&mut state).expect("durable stage");
        assert_eq!(
            fs::read(&fixture.target).expect("target after stage"),
            BEFORE.as_bytes(),
            "durable staging must not mutate target bytes"
        );
        #[cfg(unix)]
        assert_eq!(
            before_stage.ino(),
            fs::metadata(&fixture.target)
                .expect("target metadata after stage")
                .ino(),
            "durable staging must not replace the target inode"
        );
        assert!(state
            .edit_previews
            .contains_key(&fixture.request.preview_id));
        assert!(state.is_proof_ready(SUBJECT, &path_text(&fixture.target)));
        assert!(state.active_proof_permits.is_empty());
        SourceEditCommitAdapterV1::revalidate_stage_before_commit(&state, &staged)
            .expect("read-only outer COMMIT callback validation");
        assert_eq!(
            fs::read(&fixture.target).expect("target after callback"),
            BEFORE.as_bytes(),
            "outer COMMIT callback validation must not mutate target bytes"
        );
        assert!(state
            .edit_previews
            .contains_key(&fixture.request.preview_id));
        assert!(state.is_proof_ready(SUBJECT, &path_text(&fixture.target)));
        assert!(state.active_proof_permits.is_empty());
        let replayed_stage = stage_replay
            .stage(&mut state)
            .expect("idempotent stage replay");
        assert_eq!(replayed_stage.stage_digest, staged.stage_digest);

        let outcome = SourceEditCommitAdapterV1::publish_after_commit(&mut state, &staged)
            .expect("publish after outer COMMIT and broker CONSUMED");
        assert_eq!(fs::read(&fixture.target).expect("after"), AFTER.as_bytes());
        assert!(!state
            .edit_previews
            .contains_key(&fixture.request.preview_id));
        assert!(!state.is_proof_ready(SUBJECT, &path_text(&fixture.target)));
        assert!(state.active_proof_permits.is_empty());
        assert!(outcome.graph_resync_required());
        assert_eq!(outcome.conservation().target_count_before, 1);
        assert_eq!(outcome.conservation().target_count_after, 1);
        assert!(outcome.conservation().permissions_preserved);
        assert!(outcome
            .core
            .assurance_limitations
            .contains(&SAME_UID_TOCTOU_LIMITATION.to_string()));
        let inode_before_replay = fs::metadata(&fixture.target).expect("metadata");
        let replayed = SourceEditCommitAdapterV1::publish_after_commit(&mut state, &staged)
            .expect("idempotent publish replay");
        assert!(!state
            .edit_previews
            .contains_key(&fixture.request.preview_id));
        assert!(state.proof_ready.is_empty());
        assert!(state.active_proof_permits.is_empty());
        assert_eq!(replayed.outcome_digest, outcome.outcome_digest);
        #[cfg(unix)]
        assert_eq!(
            inode_before_replay.ino(),
            fs::metadata(&fixture.target)
                .expect("metadata replay")
                .ino(),
            "replay must not rename/reapply candidate bytes"
        );
        let receipt = SourceEditCommitAdapterV1::finalize(&mut state, &outcome)
            .expect("finalize physical outcome");
        assert_eq!(
            receipt.terminal_state(),
            SourceEditTerminalStateV1::FinalizedNew
        );
        assert!(!receipt.replay_reapplied_source_bytes());
        assert!(SourceEditCommitAdapterV1::pending_recovery(&state)
            .expect("pending")
            .is_empty());
        let (_, descriptor) = load_descriptor(&state, &transaction_id).expect("descriptor");
        assert!(!Path::new(&descriptor.core.backup_path).exists());
        assert!(!Path::new(&descriptor.core.candidate_temp_path).exists());
        assert!(!Path::new(&descriptor.core.rollback_temp_path).exists());
    }

    #[test]
    fn real_broker_callback_observes_old_target_and_consumes_before_publish() {
        let fixture = fixture();
        let (prepared, context) = prepare(&fixture);
        let mut state = fixture
            .cell
            .lock_mut_before_actor()
            .expect("source-edit fixture is pre-actor");
        let staged = prepared.stage(&mut state).expect("durable stage");
        assert_eq!(
            fs::read(&fixture.target).expect("target after stage"),
            BEFORE.as_bytes()
        );

        let (receipt, status) = source_edit_receipt_and_status(
            &context.operation_object_digest,
            context.expected_effects.clone(),
        );
        let broker_root = fixture._temp.path().join("source-edit-broker");
        let mut broker = OwnerAuthorizationBrokerV1::open(
            OwnerAuthorizationBrokerConfigV1 {
                root: broker_root,
                reservation_ttl_ms: 2_000,
                minimum_terminal_retention_ms: 500,
            },
            OwnerAuthorityLinearizationV1::default(),
        )
        .expect("broker");
        broker
            .issue("source-edit-lease", receipt.clone(), BROKER_NOW)
            .expect("issue");
        let reservation = broker
            .reserve(
                "source-edit-lease",
                &receipt.core.transport_session_id,
                &receipt.core.ingress_context_digest,
                &context.operation_object_digest,
                BROKER_NOW + 1,
            )
            .expect("reserve");
        let callback_observed_old = std::cell::Cell::new(false);
        let consumed = broker
            .finalize_external_mutation(&reservation, &status, BROKER_NOW + 2, || {
                SourceEditCommitAdapterV1::revalidate_stage_before_commit(&state, &staged)
                    .map_err(|error| error.to_string())?;
                if fs::read(&fixture.target).map_err(|error| error.to_string())?
                    != BEFORE.as_bytes()
                {
                    return Err("broker callback observed source mutation".to_string());
                }
                callback_observed_old.set(true);
                Ok(VerifiedExternalMutationCommitWitnessV1::new(
                    ExternalMutationCommitWitnessV1 {
                        reservation_id: reservation.reservation_id.clone(),
                        lease_id: reservation.lease_id.clone(),
                        operation_object_digest: reservation.operation_object_digest.clone(),
                        authorization_snapshot_digest: receipt.receipt_digest.clone(),
                        journal_record_digest: broker_hash("committed-stage-record"),
                        committed_at: BROKER_NOW + 2,
                    },
                ))
            })
            .expect("broker consume");
        assert!(callback_observed_old.get());
        assert_eq!(consumed.state, AuthorizationLeaseStateV1::Consumed);
        assert_eq!(
            fs::read(&fixture.target).expect("target after CONSUMED"),
            BEFORE.as_bytes(),
            "the real broker callback and CONSUMED transition must precede publication"
        );

        SourceEditCommitAdapterV1::publish_after_commit(&mut state, &staged)
            .expect("publish only after broker CONSUMED");
        assert_eq!(
            fs::read(&fixture.target).expect("target after publish"),
            AFTER.as_bytes()
        );
    }

    fn coordination_snapshot(state: &SessionState) -> Vec<u8> {
        let mut previews = state
            .edit_previews
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    serde_json::to_value(value).expect("preview JSON"),
                )
            })
            .collect::<Vec<_>>();
        previews.sort_by(|left, right| left.0.cmp(&right.0));
        let mut ready = state
            .proof_ready
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    serde_json::to_value(value).expect("proof JSON"),
                )
            })
            .collect::<Vec<_>>();
        ready.sort_by(|left, right| left.0.cmp(&right.0));
        let mut active = state
            .active_proof_permits
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    serde_json::to_value(value).expect("permit JSON"),
                )
            })
            .collect::<Vec<_>>();
        active.sort_by(|left, right| left.0.cmp(&right.0));
        serde_json::to_vec(&(previews, ready, active)).expect("coordination snapshot")
    }

    #[test]
    fn every_pre_stage_write_cut_resumes_idempotently_with_live_prepared_inputs() {
        let phases = [
            SourceEditFailpointV1::PreStageIntentDurable,
            SourceEditFailpointV1::TransactionDirectoryDurable,
            SourceEditFailpointV1::DescriptorDurable,
            SourceEditFailpointV1::PreparedJournal,
            SourceEditFailpointV1::BackupFileDurable,
            SourceEditFailpointV1::BackupDurable,
            SourceEditFailpointV1::CandidateFileDurable,
            SourceEditFailpointV1::CandidateDurable,
            SourceEditFailpointV1::StagedJournal,
            SourceEditFailpointV1::StageDurable,
        ];
        for phase in phases {
            let fixture = fixture();
            let (prepared, context) = prepare(&fixture);
            let retry = prepared.clone();
            let transaction_id = prepared.transaction_id().to_string();
            let mut state = fixture
                .cell
                .lock_mut_before_actor()
                .expect("source-edit fixture is pre-actor");
            let coordination_before = coordination_snapshot(&state);
            let target_before = fs::metadata(&fixture.target).expect("target before staging cut");
            let error = prepared
                .stage_with_faults(&mut state, &CrashAt(phase))
                .expect_err("pre-stage write cut");
            assert!(matches!(
                error,
                SourceEditTransactionError::InjectedCrash { .. }
            ));
            assert_eq!(coordination_snapshot(&state), coordination_before);
            assert_eq!(
                fs::read(&fixture.target).expect("target after pre-stage cut"),
                BEFORE.as_bytes()
            );
            #[cfg(unix)]
            assert_eq!(
                fs::metadata(&fixture.target)
                    .expect("target after pre-stage cut metadata")
                    .ino(),
                target_before.ino()
            );

            let staged = retry.stage(&mut state).expect("idempotent stage resume");
            assert_eq!(staged.transaction_id(), transaction_id);
            assert_eq!(
                staged.operation_object_digest(),
                context.operation_object_digest
            );
            assert_eq!(coordination_snapshot(&state), coordination_before);
            assert_eq!(
                fs::read(&fixture.target).expect("target after resumed stage"),
                BEFORE.as_bytes()
            );
            assert!(
                SourceEditCommitAdapterV1::pending_pre_stage_recovery(&state)
                    .expect("pre-stage inventory after resume")
                    .is_empty()
            );
        }
    }

    #[test]
    fn every_pre_stage_write_cut_has_restart_delete_only_fallback() {
        let phases = [
            SourceEditFailpointV1::PreStageIntentDurable,
            SourceEditFailpointV1::TransactionDirectoryDurable,
            SourceEditFailpointV1::DescriptorDurable,
            SourceEditFailpointV1::PreparedJournal,
            SourceEditFailpointV1::BackupFileDurable,
            SourceEditFailpointV1::BackupDurable,
            SourceEditFailpointV1::CandidateFileDurable,
            SourceEditFailpointV1::CandidateDurable,
            SourceEditFailpointV1::StagedJournal,
        ];
        for phase in phases {
            let fixture = fixture();
            let (prepared, context) = prepare(&fixture);
            let transaction_id = prepared.transaction_id().to_string();
            let mut state = fixture
                .cell
                .lock_mut_before_actor()
                .expect("source-edit fixture is pre-actor");
            let coordination_before = coordination_snapshot(&state);
            let target_before = fs::metadata(&fixture.target).expect("target before restart cut");
            prepared
                .stage_with_faults(&mut state, &CrashAt(phase))
                .expect_err("pre-stage restart cut");
            assert_eq!(coordination_snapshot(&state), coordination_before);
            drop(state);

            let restarted = restarted_state(&fixture);
            let restarted_coordination = coordination_snapshot(&restarted);
            let inventory = SourceEditCommitAdapterV1::pending_pre_stage_recovery(&restarted)
                .expect("boot pre-stage inventory");
            let recovery = inventory
                .get(&transaction_id)
                .expect("exact pre-stage orphan");
            assert_eq!(
                recovery.operation_object_digest,
                context.operation_object_digest
            );
            let receipt = SourceEditCommitAdapterV1::abort_pre_stage_without_target_write(
                &restarted,
                &transaction_id,
                &recovery.operation_object_digest,
                &recovery.intent_digest,
            )
            .expect("delete-only pre-stage fallback");
            assert_eq!(receipt.transaction_id(), transaction_id);
            assert!(!receipt.target_write_performed());
            assert_eq!(coordination_snapshot(&restarted), restarted_coordination);
            assert_eq!(
                fs::read(&fixture.target).expect("target after fallback"),
                BEFORE.as_bytes()
            );
            #[cfg(unix)]
            assert_eq!(
                fs::metadata(&fixture.target)
                    .expect("target after fallback metadata")
                    .ino(),
                target_before.ino()
            );
            assert!(
                SourceEditCommitAdapterV1::pending_pre_stage_recovery(&restarted)
                    .expect("inventory after fallback")
                    .is_empty()
            );
            assert!(SourceEditCommitAdapterV1::pending_recovery(&restarted)
                .expect("unified inventory after fallback")
                .is_empty());
            let replay = SourceEditCommitAdapterV1::abort_pre_stage_without_target_write(
                &restarted,
                &transaction_id,
                &context.operation_object_digest,
                &recovery.intent_digest,
            )
            .expect("idempotent completed fallback replay");
            assert_eq!(replay.abort_digest, receipt.abort_digest);
        }
    }

    #[test]
    fn every_pre_stage_abort_cut_is_rediscovered_after_terminal_outer_abort() {
        let cleanup_phases = [
            SourceEditFailpointV1::PreStageAbortMarkerDurable,
            SourceEditFailpointV1::PreStageAbortCandidateRemoved,
            SourceEditFailpointV1::PreStageAbortBackupRemoved,
            SourceEditFailpointV1::PreStageAbortJournalRemoved,
            SourceEditFailpointV1::PreStageAbortDescriptorRemoved,
            SourceEditFailpointV1::PreStageAbortDirectoryRemoved,
            SourceEditFailpointV1::PreStageAbortIntentRemoved,
            SourceEditFailpointV1::PreStageAbortCompletionDurable,
        ];
        for cleanup_phase in cleanup_phases {
            let fixture = fixture();
            let (prepared, context) = prepare(&fixture);
            let transaction_id = prepared.transaction_id().to_string();
            let mut state = fixture
                .cell
                .lock_mut_before_actor()
                .expect("source-edit fixture is pre-actor");
            prepared
                .stage_with_faults(
                    &mut state,
                    &CrashAt(SourceEditFailpointV1::CandidateFileDurable),
                )
                .expect_err("seed pre-stage orphan");
            drop(state);

            // The outer coordinator has already durably terminalized its
            // no-journal reservation as ABORTED. Internal discovery deliberately
            // depends only on the durable intent/abort receipt, not lease state.
            let first_boot = restarted_state(&fixture);
            let recovery = SourceEditCommitAdapterV1::pending_pre_stage_recovery(&first_boot)
                .expect("first boot inventory")
                .remove(&transaction_id)
                .expect("seed orphan inventory");
            let error = abort_pre_stage_without_target_write(
                &first_boot,
                &transaction_id,
                &context.operation_object_digest,
                &recovery.intent_digest,
                &CrashAt(cleanup_phase),
            )
            .expect_err("pre-stage abort cleanup cut");
            assert!(matches!(
                error,
                SourceEditTransactionError::InjectedCrash { .. }
            ));
            drop(first_boot);

            let second_boot = restarted_state(&fixture);
            let inventory = SourceEditCommitAdapterV1::pending_pre_stage_recovery(&second_boot)
                .expect("repeat boot inventory after terminal outer abort");
            if cleanup_phase == SourceEditFailpointV1::PreStageAbortCompletionDurable {
                assert!(
                    inventory.is_empty(),
                    "durable cleanup completion is already terminal"
                );
            } else {
                let rediscovered = inventory
                    .get(&transaction_id)
                    .expect("interrupted cleanup must be rediscovered");
                assert_eq!(rediscovered.intent_digest, recovery.intent_digest);
                assert_eq!(
                    rediscovered.operation_object_digest,
                    context.operation_object_digest
                );
            }
            let coordination_before = coordination_snapshot(&second_boot);
            let target_before = fs::metadata(&fixture.target).expect("target before cleanup retry");
            let receipt = SourceEditCommitAdapterV1::abort_pre_stage_without_target_write(
                &second_boot,
                &transaction_id,
                &context.operation_object_digest,
                &recovery.intent_digest,
            )
            .expect("resume cleanup after terminal outer abort");
            assert!(!receipt.target_write_performed());
            assert_eq!(coordination_snapshot(&second_boot), coordination_before);
            assert_eq!(
                fs::read(&fixture.target).expect("target after cleanup retry"),
                BEFORE.as_bytes()
            );
            #[cfg(unix)]
            assert_eq!(
                fs::metadata(&fixture.target)
                    .expect("target after cleanup retry metadata")
                    .ino(),
                target_before.ino()
            );
            assert!(
                SourceEditCommitAdapterV1::pending_pre_stage_recovery(&second_boot)
                    .expect("terminal cleanup inventory")
                    .is_empty()
            );
        }
    }

    #[test]
    fn stage_durable_before_outer_prepare_has_exact_abortable_restart_inventory() {
        let fixture = fixture();
        let (prepared, context) = prepare(&fixture);
        let transaction_id = prepared.transaction_id().to_string();
        let mut state = fixture
            .cell
            .lock_mut_before_actor()
            .expect("source-edit fixture is pre-actor");
        let coordination_before = coordination_snapshot(&state);
        let target_before = fs::metadata(&fixture.target).expect("target before StageDurable cut");
        prepared
            .stage_with_faults(&mut state, &CrashAt(SourceEditFailpointV1::StageDurable))
            .expect_err("crash after stage but before outer PREPARED");
        assert_eq!(coordination_snapshot(&state), coordination_before);
        assert_eq!(
            fs::read(&fixture.target).expect("target after StageDurable cut"),
            BEFORE.as_bytes()
        );
        drop(state);

        // The outer coordinator sees no operation journal, durably ABORTS the
        // reservation, and obtains the exact stage digest from this inventory.
        let restarted = restarted_state(&fixture);
        assert!(
            SourceEditCommitAdapterV1::pending_pre_stage_recovery(&restarted)
                .expect("pre-stage inventory")
                .is_empty()
        );
        let staged_inventory = SourceEditCommitAdapterV1::pending_staged_recovery(&restarted)
            .expect("staged restart inventory");
        let recovery = staged_inventory
            .get(&transaction_id)
            .expect("exact staged orphan");
        assert_eq!(
            recovery.operation_object_digest,
            context.operation_object_digest
        );
        assert!(is_digest(&recovery.stage_digest));
        let restarted_coordination = coordination_snapshot(&restarted);
        let receipt = SourceEditCommitAdapterV1::abort_staged_without_target_write(
            &restarted,
            &transaction_id,
            &recovery.operation_object_digest,
            &recovery.stage_digest,
        )
        .expect("delete-only staged abort after outer ABORT");
        assert!(!receipt.target_write_performed());
        assert_eq!(coordination_snapshot(&restarted), restarted_coordination);
        assert_eq!(
            fs::read(&fixture.target).expect("target after staged abort"),
            BEFORE.as_bytes()
        );
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&fixture.target)
                .expect("target after staged abort metadata")
                .ino(),
            target_before.ino()
        );
        assert!(
            SourceEditCommitAdapterV1::pending_staged_recovery(&restarted)
                .expect("staged inventory after abort")
                .is_empty()
        );
    }

    #[test]
    fn abort_staged_cleanup_never_observes_or_reverts_candidate_or_third_party_target() {
        for independent_bytes in [AFTER.as_bytes(), b"independent third-party bytes\n"] {
            let fixture = fixture();
            let (prepared, context) = prepare(&fixture);
            let transaction_id = prepared.transaction_id().to_string();
            let mut state = fixture
                .cell
                .lock_mut_before_actor()
                .expect("source-edit fixture is pre-actor");
            let staged = prepared.stage(&mut state).expect("stage");
            let (_, descriptor) = load_descriptor(&state, &transaction_id).expect("descriptor");
            let coordination_before = coordination_snapshot(&state);
            fs::write(&fixture.target, independent_bytes).expect("independent target writer");
            let target_metadata_before_abort =
                fs::metadata(&fixture.target).expect("target metadata before abort");
            let wrong = SourceEditCommitAdapterV1::abort_staged_without_target_write(
                &state,
                &transaction_id,
                &context.operation_object_digest,
                &"0".repeat(64),
            )
            .expect_err("wrong stage digest must refuse before cleanup");
            assert!(matches!(
                wrong,
                SourceEditTransactionError::ContextBinding(_)
            ));

            let receipt = SourceEditCommitAdapterV1::abort_staged_without_target_write(
                &state,
                &transaction_id,
                &context.operation_object_digest,
                &staged.stage_digest,
            )
            .expect("abort exact unpublished stage");
            assert_eq!(receipt.transaction_id(), transaction_id);
            assert!(!receipt.target_write_performed());
            assert_eq!(coordination_snapshot(&state), coordination_before);
            assert_eq!(
                fs::read(&fixture.target).expect("target after abort"),
                independent_bytes
            );
            #[cfg(unix)]
            assert_eq!(
                fs::metadata(&fixture.target)
                    .expect("target metadata after abort")
                    .ino(),
                target_metadata_before_abort.ino(),
                "abort must never replace an independently changed target"
            );
            let directory = transaction_dir(&state, &transaction_id).expect("tx dir");
            let removed = [
                PathBuf::from(&descriptor.core.candidate_temp_path),
                PathBuf::from(&descriptor.core.rollback_temp_path),
                PathBuf::from(&descriptor.core.backup_path),
                stage_path(&directory),
                journal_path(&directory),
                descriptor_path(&directory),
            ];
            for removed in removed {
                assert!(!managed_entry_exists(&removed).expect("removed lstat"));
            }
            let replay = SourceEditCommitAdapterV1::abort_staged_without_target_write(
                &state,
                &transaction_id,
                &context.operation_object_digest,
                &staged.stage_digest,
            )
            .expect("idempotent abort replay");
            assert_eq!(replay.abort_digest, receipt.abort_digest);
            assert!(SourceEditCommitAdapterV1::pending_recovery(&state)
                .expect("pending")
                .is_empty());
        }
    }

    #[test]
    fn every_abort_cleanup_cut_resumes_after_restart_without_target_or_coordination_write() {
        let phases = [
            SourceEditFailpointV1::AbortMarkerDurable,
            SourceEditFailpointV1::AbortCandidateRemoved,
            SourceEditFailpointV1::AbortRollbackRemoved,
            SourceEditFailpointV1::AbortBackupRemoved,
            SourceEditFailpointV1::AbortStageRemoved,
            SourceEditFailpointV1::AbortJournalRemoved,
            SourceEditFailpointV1::AbortDescriptorRemoved,
            SourceEditFailpointV1::AbortCompletionDurable,
        ];
        for phase in phases {
            let fixture = fixture();
            let (prepared, context) = prepare(&fixture);
            let transaction_id = prepared.transaction_id().to_string();
            let mut state = fixture
                .cell
                .lock_mut_before_actor()
                .expect("source-edit fixture is pre-actor");
            let staged = prepared.stage(&mut state).expect("stage");
            let coordination_before = coordination_snapshot(&state);
            let independent = b"restart-owned independent target\n";
            fs::write(&fixture.target, independent).expect("independent target write");
            let target_before_abort = fs::metadata(&fixture.target).expect("target metadata");
            let error = abort_staged_without_target_write_with_faults(
                &state,
                &transaction_id,
                &context.operation_object_digest,
                &staged.stage_digest,
                &CrashAt(phase),
            )
            .expect_err("abort cleanup failpoint");
            assert!(matches!(
                error,
                SourceEditTransactionError::InjectedCrash { .. }
            ));
            assert_eq!(coordination_snapshot(&state), coordination_before);
            assert_eq!(
                fs::read(&fixture.target).expect("target after cut"),
                independent
            );
            drop(state);

            let restarted = restarted_state(&fixture);
            let staged_inventory = SourceEditCommitAdapterV1::pending_staged_recovery(&restarted)
                .expect("staged cleanup restart inventory");
            if phase == SourceEditFailpointV1::AbortCompletionDurable {
                assert!(staged_inventory.is_empty());
            } else {
                let recovery = staged_inventory
                    .get(&transaction_id)
                    .expect("interrupted staged abort remains exactly discoverable");
                assert_eq!(recovery.stage_digest, staged.stage_digest);
                assert_eq!(
                    recovery.operation_object_digest,
                    context.operation_object_digest
                );
            }
            let receipt = SourceEditCommitAdapterV1::abort_staged_without_target_write(
                &restarted,
                &transaction_id,
                &context.operation_object_digest,
                &staged.stage_digest,
            )
            .expect("resume abort cleanup after restart");
            assert!(!receipt.target_write_performed());
            assert_eq!(
                fs::read(&fixture.target).expect("target after retry"),
                independent
            );
            #[cfg(unix)]
            assert_eq!(
                fs::metadata(&fixture.target)
                    .expect("target metadata after retry")
                    .ino(),
                target_before_abort.ino()
            );
            assert!(SourceEditCommitAdapterV1::pending_recovery(&restarted)
                .expect("pending after retry")
                .is_empty());
            assert!(
                SourceEditCommitAdapterV1::pending_staged_recovery(&restarted)
                    .expect("staged pending after retry")
                    .is_empty()
            );
        }
    }

    #[test]
    fn explicit_rollback_restores_exact_bytes_permissions_and_cleans_temps() {
        let fixture = fixture();
        let (prepared, _) = prepare(&fixture);
        let mut state = fixture
            .cell
            .lock_mut_before_actor()
            .expect("source-edit fixture is pre-actor");
        let staged = prepared.stage(&mut state).expect("stage");
        let outcome = SourceEditCommitAdapterV1::publish_after_commit(&mut state, &staged)
            .expect("publish after commit");
        let receipt = SourceEditCommitAdapterV1::rollback(&mut state, &outcome).expect("rollback");
        assert_eq!(
            receipt.terminal_state(),
            SourceEditTerminalStateV1::RolledBackOld
        );
        assert_eq!(
            fs::read(&fixture.target).expect("restored"),
            BEFORE.as_bytes()
        );
        #[cfg(unix)]
        assert_eq!(
            fs::metadata(&fixture.target)
                .expect("restored metadata")
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
        let (_, descriptor) =
            load_descriptor(&state, outcome.transaction_id()).expect("descriptor");
        assert!(!Path::new(&descriptor.core.backup_path).exists());
        assert!(!Path::new(&descriptor.core.candidate_temp_path).exists());
        assert!(!Path::new(&descriptor.core.rollback_temp_path).exists());
    }

    #[test]
    fn target_occ_and_symlink_replacement_fail_closed() {
        let occ_fixture = fixture();
        fs::write(&occ_fixture.target, "concurrent\n").expect("concurrent edit");
        assert!(matches!(
            SourceEditCommitAdapterV1::inspect(&occ_fixture.cell, &occ_fixture.request, SUBJECT),
            Err(SourceEditTransactionError::OccConflict(_))
        ));

        #[cfg(unix)]
        {
            let fixture = fixture();
            let other = fixture.target.with_file_name("other.rs");
            fs::write(&other, BEFORE).expect("other");
            fs::remove_file(&fixture.target).expect("remove target");
            std::os::unix::fs::symlink(&other, &fixture.target).expect("symlink target");
            let error =
                SourceEditCommitAdapterV1::inspect(&fixture.cell, &fixture.request, SUBJECT)
                    .expect_err("symlink must refuse");
            assert!(error.to_string().contains("symlink"));
        }
    }

    #[test]
    fn every_stage_and_publish_crash_recovers_without_precommit_mutation_or_reapply() {
        let stage_phases = [
            SourceEditFailpointV1::PreparedJournal,
            SourceEditFailpointV1::BackupDurable,
            SourceEditFailpointV1::CandidateDurable,
            SourceEditFailpointV1::StageDurable,
        ];
        for phase in stage_phases {
            let fixture = fixture();
            let (prepared, context) = prepare(&fixture);
            let transaction_id = prepared.transaction_id().to_string();
            let before = fs::metadata(&fixture.target).expect("pre-stage metadata");
            let mut state = fixture
                .cell
                .lock_mut_before_actor()
                .expect("source-edit fixture is pre-actor");
            let error = prepared
                .stage_with_faults(&mut state, &CrashAt(phase))
                .expect_err("injected stage crash");
            assert!(matches!(
                error,
                SourceEditTransactionError::InjectedCrash { .. }
            ));
            assert_eq!(
                fs::read(&fixture.target).expect("post-stage-crash bytes"),
                BEFORE.as_bytes(),
                "no stage failpoint may mutate the target"
            );
            #[cfg(unix)]
            assert_eq!(
                before.ino(),
                fs::metadata(&fixture.target)
                    .expect("post-stage-crash metadata")
                    .ino()
            );
            drop(state);
            let mut restarted = restarted_state(&fixture);
            let receipt = SourceEditCommitAdapterV1::recover_transaction(
                &mut restarted,
                &transaction_id,
                &context.operation_object_digest,
                SourceEditRecoveryDecisionV1::RestoreOld,
            )
            .expect("abort uncommitted stage");
            assert_eq!(
                receipt.terminal_state(),
                SourceEditTerminalStateV1::RecoveredOld
            );
        }

        let publish_phases = [
            SourceEditFailpointV1::PublishIntent,
            SourceEditFailpointV1::AtomicRename,
            SourceEditFailpointV1::PublishedJournal,
            SourceEditFailpointV1::OutcomeDurable,
        ];
        for phase in publish_phases {
            let fixture = fixture();
            let (prepared, context) = prepare(&fixture);
            let transaction_id = prepared.transaction_id().to_string();
            let mut state = fixture
                .cell
                .lock_mut_before_actor()
                .expect("source-edit fixture is pre-actor");
            let staged = prepared
                .stage(&mut state)
                .expect("stage before outer commit");
            SourceEditCommitAdapterV1::revalidate_stage_before_commit(&state, &staged)
                .expect("COMMIT callback revalidation");
            assert_eq!(
                fs::read(&fixture.target).expect("callback bytes"),
                BEFORE.as_bytes()
            );
            let error = publish_after_commit(&mut state, &staged, &CrashAt(phase))
                .expect_err("injected crash");
            assert!(matches!(
                error,
                SourceEditTransactionError::InjectedCrash { .. }
            ));
            let before_recovery = fs::metadata(&fixture.target).expect("pre-recovery metadata");
            let was_new =
                fs::read(&fixture.target).expect("pre-recovery bytes") == AFTER.as_bytes();
            // Ephemeral preview/proof state dies on a real SessionState restart.
            // A durable outer COMMIT still forward-completes from stage alone.
            drop(state);
            let mut restarted = restarted_state(&fixture);
            let outcome = SourceEditCommitAdapterV1::forward_complete_committed(
                &mut restarted,
                &transaction_id,
                &context.operation_object_digest,
                &staged.stage_digest,
            )
            .expect("forward-complete committed stage");
            let bytes = fs::read(&fixture.target).expect("recovered bytes");
            assert_eq!(bytes, AFTER.as_bytes());
            assert_eq!(outcome.core.stage_digest, staged.stage_digest);
            if was_new {
                #[cfg(unix)]
                assert_eq!(
                    before_recovery.ino(),
                    fs::metadata(&fixture.target)
                        .expect("post-recovery metadata")
                        .ino(),
                    "recovery must keep already-published new bytes in place"
                );
            }
            let terminal = SourceEditCommitAdapterV1::finalize(&mut restarted, &outcome)
                .expect("finalize recovered publish");
            assert!(!terminal.replay_reapplied_source_bytes());
        }
    }

    #[test]
    fn rollback_and_finalize_failpoints_are_restart_recoverable() {
        for phase in [
            SourceEditFailpointV1::RollbackIntent,
            SourceEditFailpointV1::RollbackRename,
        ] {
            let rollback_fixture = fixture();
            let (prepared, context) = prepare(&rollback_fixture);
            let mut state = rollback_fixture
                .cell
                .lock_mut_before_actor()
                .expect("source-edit rollback fixture is pre-actor");
            let staged = prepared.stage(&mut state).expect("stage");
            let outcome = SourceEditCommitAdapterV1::publish_after_commit(&mut state, &staged)
                .expect("publish");
            assert!(matches!(
                rollback_outcome(&mut state, &outcome, &CrashAt(phase)),
                Err(SourceEditTransactionError::InjectedCrash { .. })
            ));
            drop(state);
            let mut restarted = restarted_state(&rollback_fixture);
            let recovered = SourceEditCommitAdapterV1::recover_transaction(
                &mut restarted,
                outcome.transaction_id(),
                &context.operation_object_digest,
                SourceEditRecoveryDecisionV1::RestoreOld,
            )
            .expect("recover rollback");
            assert_eq!(
                recovered.terminal_state(),
                SourceEditTerminalStateV1::RecoveredOld
            );
            assert_eq!(
                fs::read(&rollback_fixture.target).expect("old"),
                BEFORE.as_bytes()
            );
        }

        let fixture = fixture();
        let (prepared, _) = prepare(&fixture);
        let mut state = fixture
            .cell
            .lock_mut_before_actor()
            .expect("source-edit fixture is pre-actor");
        let staged = prepared.stage(&mut state).expect("stage");
        let outcome =
            SourceEditCommitAdapterV1::publish_after_commit(&mut state, &staged).expect("publish");
        assert!(matches!(
            finalize_outcome(
                &mut state,
                &outcome,
                &CrashAt(SourceEditFailpointV1::Finalize)
            ),
            Err(SourceEditTransactionError::InjectedCrash { .. })
        ));
        drop(state);
        let mut restarted = restarted_state(&fixture);
        SourceEditCommitAdapterV1::finalize(&mut restarted, &outcome)
            .expect("retry finalize after restart");
    }
}
