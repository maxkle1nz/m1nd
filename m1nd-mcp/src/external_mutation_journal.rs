//! Durable journal for the closed external-mutation consumer.
//!
//! The broker owns authority linearization; this journal owns the domain
//! transaction decision.  A PREPARED record is durable before broker
//! finalization begins.  The broker callback may then append one exact COMMIT
//! record and publish the staged domain update.  A callback error after broker
//! FINALIZATION_PREPARED is never interpreted as an abort.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::fs::File;
#[cfg(unix)]
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use m1nd_control::{digest_canonical, CanonicalError};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::light_author_handlers::LockGuard;
use crate::owner_authorization_broker::{
    AuthorizationReservationV1, ExternalMutationCommitWitnessV1,
    ExternalMutationJournalAbsenceWitnessV1, ExternalMutationPreparedAbortWitnessV1,
    VerifiedExternalMutationCommitWitnessV1, VerifiedExternalMutationJournalAbsenceWitnessV1,
    VerifiedExternalMutationPreparedAbortWitnessV1,
};
use crate::protected_journal_head::{
    advance_protected_head, verify_or_initialize_protected_head, ProtectedJournalHeadSnapshotV1,
    SharedProtectedJournalHeadBackendV1,
};

pub const EXTERNAL_MUTATION_JOURNAL_RECORD_SCHEMA: &str =
    "m1nd-external-mutation-journal-record-v2";
pub const EXTERNAL_MUTATION_JOURNAL_RECORD_DIGEST_DOMAIN: &str =
    "m1nd-external-mutation-journal-record-v2";
pub const EXTERNAL_MUTATION_OPERATION_ID_DIGEST_DOMAIN: &str =
    "m1nd-external-mutation-operation-id-v1";
pub const EXTERNAL_MUTATION_JOURNAL_HEAD_DOMAIN: &str = "m1nd-external-mutation-journal-head-v1";
pub const EXTERNAL_MUTATION_PUBLISHED_RESULT_SCHEMA: &str =
    "m1nd-external-mutation-published-result-v1";
pub const EXTERNAL_MUTATION_PUBLISHED_RESULT_DIGEST_DOMAIN: &str =
    "m1nd-external-mutation-published-result-v1";
pub const BRAIN_PROMOTE_RECONCILIATION_RECEIPT_SCHEMA: &str =
    "m1nd-brain-promote-reconciliation-receipt-v1";
pub const BRAIN_PROMOTE_RECONCILIATION_RECEIPT_DIGEST_DOMAIN: &str =
    "m1nd-brain-promote-reconciliation-receipt-v1";
pub const BRAIN_PROMOTE_CHECKPOINT_ACK_SCHEMA: &str = "m1nd-checkpoint-ack-v1";
pub const BRAIN_PROMOTE_CHECKPOINT_ACK_DIGEST_DOMAIN: &str = "m1nd-brain-promote-checkpoint-ack-v1";

const JOURNAL_FILE: &str = "external-mutations.jsonl";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExternalMutationJournalPhaseV1 {
    Prepared,
    Committed,
    Reconciled,
    Published,
    RecoveryRequired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalMutationPrepareV1 {
    pub semantic_action: String,
    pub payload_digest: String,
    pub operation_object_digest: String,
    pub operation_version: u64,
    /// Exact durable actor identity bound by authority and used for recovery
    /// routing. This is never a filesystem/root selector.
    pub actor_brain_id: String,
    /// Canonical transport-selected project/root route, when one exists.
    /// Domain ownership checks use this fact; actor lookup never does.
    pub route_selector: Option<String>,
    pub mission_id: Option<String>,
    pub mission_head_id: Option<String>,
    /// Adapter-owned durable recovery description.  It contains no authority
    /// assertions and is never used to choose a semantic action.
    pub recovery_payload: Value,
}

/// Sealed, replay-only result. Once PUBLISHED, callers are answered from this
/// receipt and adapters are never re-entered against newer domain state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalMutationPublishedResultV1 {
    pub schema: String,
    pub semantic_action: String,
    pub semantic_payload_digest: String,
    pub operation_object_digest: String,
    pub outcome_digest: String,
    pub graph_resync_required: bool,
    pub reconciliation_state: String,
    pub result: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrainPromoteCheckpointAckV1 {
    pub schema: String,
    pub checkpoint_id: String,
    pub brain_id: String,
    pub epoch: u64,
    pub generation: u64,
    pub revision: u64,
    pub current_pointer_digest: String,
    pub confirmed_at_unix_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrainPromoteReconciliationReceiptV1 {
    pub schema: String,
    pub operation_id: String,
    pub operation_object_digest: String,
    pub source_brain_id: String,
    pub reconciliation_brain_id: String,
    pub medulla_path: String,
    pub medulla_postimage_sha256: String,
    pub adapter: String,
    pub mode: String,
    pub incremental: bool,
    pub namespace: String,
    pub ingest_output_digest: String,
    pub graph_generation_before: u64,
    pub graph_generation_after: u64,
    pub checkpoint_ack: BrainPromoteCheckpointAckV1,
    pub checkpoint_ack_digest: String,
    pub reconciled_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalMutationJournalEntryV1 {
    pub operation_id: String,
    pub reservation_id: String,
    pub lease_id: String,
    pub authorization_snapshot_digest: String,
    pub prepare: ExternalMutationPrepareV1,
    pub phase: ExternalMutationJournalPhaseV1,
    pub outcome_digest: Option<String>,
    pub commit_record_digest: Option<String>,
    pub committed_at: Option<u64>,
    pub published_result: Option<ExternalMutationPublishedResultV1>,
    pub published_result_digest: Option<String>,
    pub reconciliation_receipt: Option<BrainPromoteReconciliationReceiptV1>,
    pub reconciliation_receipt_digest: Option<String>,
    pub prepared_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExternalMutationJournalRecordCoreV1 {
    sequence: u64,
    entry: ExternalMutationJournalEntryV1,
    previous_record_digest: Option<String>,
    recorded_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExternalMutationJournalRecordV1 {
    schema: String,
    core: ExternalMutationJournalRecordCoreV1,
    record_digest: String,
}

impl ExternalMutationJournalRecordV1 {
    fn seal(&mut self) -> Result<(), CanonicalError> {
        self.record_digest =
            digest_canonical(EXTERNAL_MUTATION_JOURNAL_RECORD_DIGEST_DOMAIN, &self.core)?;
        Ok(())
    }

    fn validate(
        &self,
        expected_sequence: u64,
        expected_previous: Option<&str>,
    ) -> Result<(), ExternalMutationJournalError> {
        let recomputed =
            digest_canonical(EXTERNAL_MUTATION_JOURNAL_RECORD_DIGEST_DOMAIN, &self.core)?;
        let schema_matches = self.schema == EXTERNAL_MUTATION_JOURNAL_RECORD_SCHEMA;
        let sequence_matches = self.core.sequence == expected_sequence;
        let previous_matches = self.core.previous_record_digest.as_deref() == expected_previous;
        let digest_shape_valid = is_digest(&self.record_digest);
        let digest_matches = recomputed == self.record_digest;
        if !schema_matches
            || !sequence_matches
            || !previous_matches
            || !digest_shape_valid
            || !digest_matches
        {
            return Err(ExternalMutationJournalError::Corruption {
                detail: format!(
                    "journal chain mismatch at sequence {expected_sequence}: schema={schema_matches}, sequence={sequence_matches}, previous={previous_matches}, digest_shape={digest_shape_valid}, digest={digest_matches}, recorded_digest={}, recomputed_digest={recomputed}",
                    self.record_digest
                ),
            });
        }
        validate_entry(&self.core.entry)?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum ExternalMutationJournalError {
    Io {
        operation: &'static str,
        source: std::io::Error,
    },
    Json(serde_json::Error),
    Canonical(CanonicalError),
    WriterLock(String),
    ProtectedHead(String),
    Corruption {
        detail: String,
    },
    Refused {
        code: &'static str,
        detail: String,
    },
    Poisoned,
}

impl ExternalMutationJournalError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Refused { code, .. } => code,
            Self::Corruption { .. } => "external_mutation_journal_corruption",
            Self::ProtectedHead(_) => "external_mutation_journal_rollback_detected",
            Self::Poisoned => "external_mutation_journal_poisoned",
            Self::Io { .. } | Self::Json(_) | Self::Canonical(_) | Self::WriterLock(_) => {
                "external_mutation_journal_unavailable"
            }
        }
    }

    fn refused(code: &'static str, detail: impl Into<String>) -> Self {
        Self::Refused {
            code,
            detail: detail.into(),
        }
    }
}

impl fmt::Display for ExternalMutationJournalError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, source } => write!(formatter, "{operation}: {source}"),
            Self::Json(error) => write!(formatter, "external mutation journal JSON: {error}"),
            Self::Canonical(error) => {
                write!(formatter, "external mutation canonicalization: {error}")
            }
            Self::WriterLock(detail) => write!(formatter, "external journal writer lock: {detail}"),
            Self::ProtectedHead(detail) => write!(formatter, "external journal head: {detail}"),
            Self::Corruption { detail } => {
                write!(formatter, "external journal corruption: {detail}")
            }
            Self::Refused { code, detail } => write!(formatter, "{code}: {detail}"),
            Self::Poisoned => formatter.write_str("external mutation journal is poisoned"),
        }
    }
}

impl Error for ExternalMutationJournalError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json(error) => Some(error),
            Self::Canonical(error) => Some(error),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for ExternalMutationJournalError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<CanonicalError> for ExternalMutationJournalError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

pub struct ExternalMutationJournalV1 {
    path: PathBuf,
    file: File,
    sequence: u64,
    tail_digest: Option<String>,
    durable_len: u64,
    entries: BTreeMap<String, ExternalMutationJournalEntryV1>,
    poisoned: bool,
    protected_head_backend: Option<SharedProtectedJournalHeadBackendV1>,
    protected_head: Option<ProtectedJournalHeadSnapshotV1>,
    _writer_lock: LockGuard,
}

impl ExternalMutationJournalV1 {
    pub(crate) fn open(
        root: impl AsRef<Path>,
        protected_head_backend: Option<SharedProtectedJournalHeadBackendV1>,
    ) -> Result<Self, ExternalMutationJournalError> {
        let root = root.as_ref();
        refuse_symlink(root)?;
        std::fs::create_dir_all(root).map_err(|source| ExternalMutationJournalError::Io {
            operation: "create_external_mutation_journal_root",
            source,
        })?;
        refuse_symlink(root)?;
        let writer_lock = LockGuard::acquire_in(root, "external-mutation-journal-v1")
            .map_err(|error| ExternalMutationJournalError::WriterLock(error.to_string()))?;
        let path = root.join(JOURNAL_FILE);
        refuse_symlink(&path)?;
        let existed = path.exists();
        let mut file =
            open_journal_no_follow(&path).map_err(|source| ExternalMutationJournalError::Io {
                operation: "open_external_mutation_journal",
                source,
            })?;
        if !existed {
            file.sync_all()
                .map_err(|source| ExternalMutationJournalError::Io {
                    operation: "sync_new_external_mutation_journal",
                    source,
                })?;
            #[cfg(unix)]
            sync_parent(&path).map_err(|source| ExternalMutationJournalError::Io {
                operation: "sync_new_external_mutation_journal_parent",
                source,
            })?;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|source| ExternalMutationJournalError::Io {
                operation: "read_external_mutation_journal",
                source,
            })?;
        let complete_len = if bytes.ends_with(b"\n") {
            bytes.len()
        } else {
            bytes
                .iter()
                .rposition(|byte| *byte == b'\n')
                .map_or(0, |index| index + 1)
        };
        let mut sequence = 0;
        let mut tail_digest: Option<String> = None;
        let mut entries = BTreeMap::new();
        for frame in bytes[..complete_len].split_inclusive(|byte| *byte == b'\n') {
            let line = &frame[..frame.len() - 1];
            if line.is_empty() {
                return Err(ExternalMutationJournalError::Corruption {
                    detail: format!("empty record at sequence {}", sequence + 1),
                });
            }
            let raw_record: Value = serde_json::from_slice(line)?;
            validate_record_shape(&raw_record, sequence + 1)?;
            let record: ExternalMutationJournalRecordV1 = serde_json::from_value(raw_record)?;
            record.validate(sequence + 1, tail_digest.as_deref())?;
            apply_record(&mut entries, &record)?;
            sequence += 1;
            tail_digest = Some(record.record_digest);
        }
        if complete_len != bytes.len() {
            truncate_journal_tail(&file, &path, complete_len as u64).map_err(|source| {
                ExternalMutationJournalError::Io {
                    operation: "truncate_torn_external_mutation_journal_tail",
                    source,
                }
            })?;
        }
        let protected_head = protected_head_backend
            .as_ref()
            .map(|backend| {
                verify_or_initialize_protected_head(
                    backend,
                    EXTERNAL_MUTATION_JOURNAL_HEAD_DOMAIN,
                    sequence,
                    tail_digest.clone(),
                )
                .map_err(ExternalMutationJournalError::ProtectedHead)
            })
            .transpose()?;
        Ok(Self {
            path,
            file,
            sequence,
            tail_digest,
            durable_len: complete_len as u64,
            entries,
            poisoned: false,
            protected_head_backend,
            protected_head,
            _writer_lock: writer_lock,
        })
    }

    pub fn entry(&self, operation_id: &str) -> Option<&ExternalMutationJournalEntryV1> {
        self.entries.get(operation_id)
    }

    pub(crate) fn entries(&self) -> Vec<ExternalMutationJournalEntryV1> {
        self.entries.values().cloned().collect()
    }

    pub(crate) fn find_by_lease_and_object(
        &self,
        lease_id: &str,
        operation_object_digest: &str,
    ) -> Option<ExternalMutationJournalEntryV1> {
        self.entries
            .values()
            .find(|entry| {
                entry.lease_id == lease_id
                    && entry.prepare.operation_object_digest == operation_object_digest
            })
            .cloned()
    }

    /// Reconstruct the opaque broker recovery witness only from a journal entry
    /// whose COMMITTED record survived full hash-chain replay.
    pub(crate) fn verified_commit_witness(
        &self,
        operation_id: &str,
    ) -> Result<VerifiedExternalMutationCommitWitnessV1, ExternalMutationJournalError> {
        let entry = self.entries.get(operation_id).ok_or_else(|| {
            ExternalMutationJournalError::refused(
                "external_mutation_operation_not_found",
                operation_id,
            )
        })?;
        if !matches!(
            entry.phase,
            ExternalMutationJournalPhaseV1::Committed
                | ExternalMutationJournalPhaseV1::Reconciled
                | ExternalMutationJournalPhaseV1::Published
                | ExternalMutationJournalPhaseV1::RecoveryRequired
        ) {
            return Err(ExternalMutationJournalError::refused(
                "external_mutation_not_committed",
                format!("operation is {:?}", entry.phase),
            ));
        }
        let journal_record_digest = entry.commit_record_digest.clone().ok_or_else(|| {
            ExternalMutationJournalError::Corruption {
                detail: "committed operation has no replay-verified commit record".to_string(),
            }
        })?;
        let committed_at =
            entry
                .committed_at
                .ok_or_else(|| ExternalMutationJournalError::Corruption {
                    detail: "committed operation has no sealed commit timestamp".to_string(),
                })?;
        Ok(VerifiedExternalMutationCommitWitnessV1::new(
            ExternalMutationCommitWitnessV1 {
                reservation_id: entry.reservation_id.clone(),
                lease_id: entry.lease_id.clone(),
                operation_object_digest: entry.prepare.operation_object_digest.clone(),
                authorization_snapshot_digest: entry.authorization_snapshot_digest.clone(),
                journal_record_digest,
                committed_at,
            },
        ))
    }

    /// Produce an opaque negative witness only after open/replay/protected-head
    /// verification established that this exact operation remains PREPARED and
    /// has no partial COMMIT fields.
    pub(crate) fn verified_prepared_abort_witness(
        &self,
        operation_id: &str,
    ) -> Result<VerifiedExternalMutationPreparedAbortWitnessV1, ExternalMutationJournalError> {
        let entry = self.entries.get(operation_id).ok_or_else(|| {
            ExternalMutationJournalError::refused(
                "external_mutation_operation_not_found",
                operation_id,
            )
        })?;
        if entry.phase != ExternalMutationJournalPhaseV1::Prepared
            || entry.commit_record_digest.is_some()
            || entry.outcome_digest.is_some()
            || entry.committed_at.is_some()
        {
            return Err(ExternalMutationJournalError::refused(
                "external_mutation_not_exact_prepared",
                format!("operation is {:?} or carries COMMIT fields", entry.phase),
            ));
        }
        Ok(VerifiedExternalMutationPreparedAbortWitnessV1::new(
            ExternalMutationPreparedAbortWitnessV1 {
                reservation_id: entry.reservation_id.clone(),
                lease_id: entry.lease_id.clone(),
                operation_object_digest: entry.prepare.operation_object_digest.clone(),
                authorization_snapshot_digest: entry.authorization_snapshot_digest.clone(),
                prepared_at: entry.prepared_at,
            },
        ))
    }

    /// Produce an opaque no-journal witness only after this journal has been
    /// fully replayed and its protected head verified. Any entry sharing the
    /// one-shot lease or reservation makes absence unprovable.
    pub(crate) fn verified_operation_absence_witness(
        &self,
        reservation_id: &str,
        lease_id: &str,
        operation_object_digest: &str,
        authorization_snapshot_digest: &str,
    ) -> Result<VerifiedExternalMutationJournalAbsenceWitnessV1, ExternalMutationJournalError> {
        if !is_digest(reservation_id)
            || lease_id.trim().is_empty()
            || !is_digest(operation_object_digest)
            || !is_digest(authorization_snapshot_digest)
        {
            return Err(ExternalMutationJournalError::refused(
                "external_mutation_journal_absence_binding_invalid",
                "absence witness requires exact reservation, lease, object, and receipt digests",
            ));
        }
        if self
            .entries
            .values()
            .any(|entry| entry.reservation_id == reservation_id || entry.lease_id == lease_id)
        {
            return Err(ExternalMutationJournalError::refused(
                "external_mutation_journal_operation_present",
                "protected journal contains a binding for the reserved operation",
            ));
        }
        Ok(VerifiedExternalMutationJournalAbsenceWitnessV1::new(
            ExternalMutationJournalAbsenceWitnessV1 {
                reservation_id: reservation_id.to_string(),
                lease_id: lease_id.to_string(),
                operation_object_digest: operation_object_digest.to_string(),
                authorization_snapshot_digest: authorization_snapshot_digest.to_string(),
            },
        ))
    }

    pub(crate) fn prepare(
        &mut self,
        reservation: &AuthorizationReservationV1,
        authorization_snapshot_digest: &str,
        prepare: ExternalMutationPrepareV1,
        now_ms: u64,
    ) -> Result<ExternalMutationJournalEntryV1, ExternalMutationJournalError> {
        if reservation.operation_object_digest != prepare.operation_object_digest
            || !is_digest(authorization_snapshot_digest)
        {
            return Err(ExternalMutationJournalError::refused(
                "external_mutation_prepare_binding_mismatch",
                "prepare does not bind the reserved object and signed receipt",
            ));
        }
        let operation_id = digest_canonical(
            EXTERNAL_MUTATION_OPERATION_ID_DIGEST_DOMAIN,
            &(
                reservation.reservation_id.as_str(),
                reservation.lease_id.as_str(),
                authorization_snapshot_digest,
                &prepare,
            ),
        )?;
        if self.entries.contains_key(&operation_id) {
            return Err(ExternalMutationJournalError::refused(
                "external_mutation_duplicate_prepare",
                operation_id,
            ));
        }
        let entry = ExternalMutationJournalEntryV1 {
            operation_id,
            reservation_id: reservation.reservation_id.clone(),
            lease_id: reservation.lease_id.clone(),
            authorization_snapshot_digest: authorization_snapshot_digest.to_string(),
            prepare,
            phase: ExternalMutationJournalPhaseV1::Prepared,
            outcome_digest: None,
            commit_record_digest: None,
            committed_at: None,
            published_result: None,
            published_result_digest: None,
            reconciliation_receipt: None,
            reconciliation_receipt_digest: None,
            prepared_at: now_ms,
            updated_at: now_ms,
        };
        self.append(entry.clone(), now_ms)?;
        Ok(entry)
    }

    pub(crate) fn commit(
        &mut self,
        operation_id: &str,
        outcome_digest: String,
        committed_at: u64,
    ) -> Result<VerifiedExternalMutationCommitWitnessV1, ExternalMutationJournalError> {
        if !is_digest(&outcome_digest) {
            return Err(ExternalMutationJournalError::refused(
                "external_mutation_outcome_digest_invalid",
                "outcome digest must be canonical SHA-256",
            ));
        }
        let mut entry = self.entries.get(operation_id).cloned().ok_or_else(|| {
            ExternalMutationJournalError::refused(
                "external_mutation_prepare_not_found",
                operation_id,
            )
        })?;
        if entry.phase != ExternalMutationJournalPhaseV1::Prepared {
            return Err(ExternalMutationJournalError::refused(
                "external_mutation_not_prepared",
                format!("operation is {:?}", entry.phase),
            ));
        }
        entry.phase = ExternalMutationJournalPhaseV1::Committed;
        entry.outcome_digest = Some(outcome_digest);
        entry.committed_at = Some(committed_at);
        entry.updated_at = committed_at;
        let record_digest = self.append(entry.clone(), committed_at)?;
        let mut persisted = entry;
        persisted.commit_record_digest = Some(record_digest.clone());
        self.entries
            .insert(operation_id.to_string(), persisted.clone());
        Ok(VerifiedExternalMutationCommitWitnessV1::new(
            ExternalMutationCommitWitnessV1 {
                reservation_id: persisted.reservation_id,
                lease_id: persisted.lease_id,
                operation_object_digest: persisted.prepare.operation_object_digest,
                authorization_snapshot_digest: persisted.authorization_snapshot_digest,
                journal_record_digest: record_digest,
                committed_at,
            },
        ))
    }

    pub(crate) fn mark_reconciled(
        &mut self,
        operation_id: &str,
        receipt: BrainPromoteReconciliationReceiptV1,
        published_result: ExternalMutationPublishedResultV1,
        now_ms: u64,
    ) -> Result<(), ExternalMutationJournalError> {
        let mut entry = self.entries.get(operation_id).cloned().ok_or_else(|| {
            ExternalMutationJournalError::refused(
                "external_mutation_operation_not_found",
                operation_id,
            )
        })?;
        let receipt_digest =
            digest_canonical(BRAIN_PROMOTE_RECONCILIATION_RECEIPT_DIGEST_DOMAIN, &receipt)?;
        let published_result_digest = digest_canonical(
            EXTERNAL_MUTATION_PUBLISHED_RESULT_DIGEST_DOMAIN,
            &published_result,
        )?;
        if entry.phase == ExternalMutationJournalPhaseV1::Reconciled {
            if entry.reconciliation_receipt.as_ref() == Some(&receipt)
                && entry.reconciliation_receipt_digest.as_deref() == Some(receipt_digest.as_str())
                && entry.published_result.as_ref() == Some(&published_result)
                && entry.published_result_digest.as_deref()
                    == Some(published_result_digest.as_str())
            {
                return Ok(());
            }
            return Err(ExternalMutationJournalError::refused(
                "brain_promote_reconciliation_receipt_mismatch",
                "RECONCILED operation already carries a different receipt",
            ));
        }
        if !matches!(
            entry.phase,
            ExternalMutationJournalPhaseV1::Committed
                | ExternalMutationJournalPhaseV1::RecoveryRequired
        ) {
            return Err(ExternalMutationJournalError::refused(
                "brain_promote_not_committed_for_reconciliation",
                format!("operation is {:?}", entry.phase),
            ));
        }
        entry.phase = ExternalMutationJournalPhaseV1::Reconciled;
        entry.reconciliation_receipt = Some(receipt);
        entry.reconciliation_receipt_digest = Some(receipt_digest);
        entry.published_result = Some(published_result);
        entry.published_result_digest = Some(published_result_digest);
        entry.updated_at = now_ms;
        validate_reconciliation_receipt_binding(
            &entry,
            entry
                .reconciliation_receipt
                .as_ref()
                .expect("receipt just set"),
        )?;
        validate_published_result_binding(
            &entry,
            entry.published_result.as_ref().expect("result just set"),
        )?;
        self.append(entry, now_ms)?;
        Ok(())
    }

    pub(crate) fn mark_published(
        &mut self,
        operation_id: &str,
        published_result: ExternalMutationPublishedResultV1,
        now_ms: u64,
    ) -> Result<(), ExternalMutationJournalError> {
        let mut entry = self.entries.get(operation_id).cloned().ok_or_else(|| {
            ExternalMutationJournalError::refused(
                "external_mutation_operation_not_found",
                operation_id,
            )
        })?;
        validate_published_result_binding(&entry, &published_result)?;
        let published_result_digest = digest_canonical(
            EXTERNAL_MUTATION_PUBLISHED_RESULT_DIGEST_DOMAIN,
            &published_result,
        )?;
        if entry.phase == ExternalMutationJournalPhaseV1::Published {
            if entry.published_result.as_ref() == Some(&published_result)
                && entry.published_result_digest.as_deref()
                    == Some(published_result_digest.as_str())
            {
                return Ok(());
            }
            return Err(ExternalMutationJournalError::refused(
                "external_mutation_published_result_mismatch",
                "PUBLISHED operation already carries a different sealed result",
            ));
        }
        if entry.phase == ExternalMutationJournalPhaseV1::Reconciled
            && (entry.published_result.as_ref() != Some(&published_result)
                || entry.published_result_digest.as_deref()
                    != Some(published_result_digest.as_str()))
        {
            return Err(ExternalMutationJournalError::refused(
                "external_mutation_published_result_mismatch",
                "RECONCILED operation may only publish its already sealed result",
            ));
        }
        let publishable = if entry.prepare.semantic_action == "brain.promote" {
            entry.phase == ExternalMutationJournalPhaseV1::Reconciled
                && entry.reconciliation_receipt.is_some()
                && entry.reconciliation_receipt_digest.is_some()
        } else {
            entry.phase == ExternalMutationJournalPhaseV1::Committed
                || (entry.phase == ExternalMutationJournalPhaseV1::RecoveryRequired
                    && entry.commit_record_digest.is_some()
                    && entry.committed_at.is_some())
        };
        if !publishable {
            return Err(ExternalMutationJournalError::refused(
                "external_mutation_not_committed",
                format!("operation is {:?}", entry.phase),
            ));
        }
        let published_at = now_ms.max(entry.updated_at);
        entry.phase = ExternalMutationJournalPhaseV1::Published;
        entry.published_result = Some(published_result);
        entry.published_result_digest = Some(published_result_digest);
        entry.updated_at = published_at;
        self.append(entry, published_at)?;
        Ok(())
    }

    pub(crate) fn mark_recovery_required(
        &mut self,
        operation_id: &str,
        now_ms: u64,
    ) -> Result<(), ExternalMutationJournalError> {
        let mut entry = self.entries.get(operation_id).cloned().ok_or_else(|| {
            ExternalMutationJournalError::refused(
                "external_mutation_operation_not_found",
                operation_id,
            )
        })?;
        if entry.phase != ExternalMutationJournalPhaseV1::Committed
            && !(entry.phase == ExternalMutationJournalPhaseV1::RecoveryRequired
                && entry.commit_record_digest.is_some()
                && entry.committed_at.is_some())
        {
            return Err(ExternalMutationJournalError::refused(
                "external_mutation_not_committed",
                format!("operation is {:?}", entry.phase),
            ));
        }
        entry.phase = ExternalMutationJournalPhaseV1::RecoveryRequired;
        entry.updated_at = now_ms;
        self.append(entry, now_ms)?;
        Ok(())
    }

    fn append(
        &mut self,
        entry: ExternalMutationJournalEntryV1,
        now_ms: u64,
    ) -> Result<String, ExternalMutationJournalError> {
        if self.poisoned {
            return Err(ExternalMutationJournalError::Poisoned);
        }
        let actual_len = self
            .file
            .metadata()
            .map_err(|source| ExternalMutationJournalError::Io {
                operation: "preflight_external_mutation_journal_length",
                source,
            })?
            .len();
        if actual_len != self.durable_len {
            self.poisoned = true;
            return Err(ExternalMutationJournalError::Corruption {
                detail: "journal length changed outside the held writer lock".to_string(),
            });
        }
        if let (Some(backend), Some(expected)) =
            (&self.protected_head_backend, &self.protected_head)
        {
            let observed = verify_or_initialize_protected_head(
                backend,
                EXTERNAL_MUTATION_JOURNAL_HEAD_DOMAIN,
                self.sequence,
                self.tail_digest.clone(),
            )
            .map_err(ExternalMutationJournalError::ProtectedHead)?;
            if &observed != expected {
                self.poisoned = true;
                return Err(ExternalMutationJournalError::ProtectedHead(
                    "protected head changed since journal open".to_string(),
                ));
            }
        }
        validate_entry(&entry)?;
        let mut record = ExternalMutationJournalRecordV1 {
            schema: EXTERNAL_MUTATION_JOURNAL_RECORD_SCHEMA.to_string(),
            core: ExternalMutationJournalRecordCoreV1 {
                sequence: self.sequence + 1,
                entry: entry.clone(),
                previous_record_digest: self.tail_digest.clone(),
                recorded_at: now_ms,
            },
            record_digest: String::new(),
        };
        record.seal()?;
        let mut bytes = serde_json::to_vec(&record)?;
        bytes.push(b'\n');
        if let Err(source) = self
            .file
            .write_all(&bytes)
            .and_then(|()| self.file.sync_all())
        {
            self.poisoned = true;
            return Err(ExternalMutationJournalError::Io {
                operation: "append_sync_external_mutation_record",
                source,
            });
        }
        let next_sequence = self.sequence + 1;
        let next_digest = record.record_digest.clone();
        if let (Some(backend), Some(expected)) =
            (&self.protected_head_backend, &self.protected_head)
        {
            match advance_protected_head(
                backend,
                EXTERNAL_MUTATION_JOURNAL_HEAD_DOMAIN,
                expected,
                next_sequence,
                next_digest.clone(),
            ) {
                Ok(next) => self.protected_head = Some(next),
                Err(detail) => {
                    self.poisoned = true;
                    return Err(ExternalMutationJournalError::ProtectedHead(detail));
                }
            }
        }
        self.sequence = next_sequence;
        self.tail_digest = Some(next_digest.clone());
        self.durable_len += bytes.len() as u64;
        self.entries.insert(entry.operation_id.clone(), entry);
        Ok(next_digest)
    }
}

/// This journal is an unshipped v1 surface, so there is deliberately no
/// best-effort legacy migration. Every frame must carry the complete current
/// shape, including explicit `null` for optional fields. A partial/older root
/// fails closed and must be replaced with a fresh root by its owner.
fn validate_record_shape(
    record: &Value,
    sequence: u64,
) -> Result<(), ExternalMutationJournalError> {
    let record = record
        .as_object()
        .ok_or_else(|| incomplete_record_shape(sequence, "record", "expected a JSON object"))?;
    require_exact_fields(
        record,
        &["schema", "core", "record_digest"],
        sequence,
        "record",
    )?;
    let core = record
        .get("core")
        .and_then(Value::as_object)
        .ok_or_else(|| incomplete_record_shape(sequence, "core", "expected a JSON object"))?;
    require_exact_fields(
        core,
        &["sequence", "entry", "previous_record_digest", "recorded_at"],
        sequence,
        "core",
    )?;
    let entry = core
        .get("entry")
        .and_then(Value::as_object)
        .ok_or_else(|| incomplete_record_shape(sequence, "entry", "expected a JSON object"))?;
    require_exact_fields(
        entry,
        &[
            "operation_id",
            "reservation_id",
            "lease_id",
            "authorization_snapshot_digest",
            "prepare",
            "phase",
            "outcome_digest",
            "commit_record_digest",
            "committed_at",
            "published_result",
            "published_result_digest",
            "reconciliation_receipt",
            "reconciliation_receipt_digest",
            "prepared_at",
            "updated_at",
        ],
        sequence,
        "entry",
    )?;
    let prepare = entry
        .get("prepare")
        .and_then(Value::as_object)
        .ok_or_else(|| incomplete_record_shape(sequence, "prepare", "expected a JSON object"))?;
    require_exact_fields(
        prepare,
        &[
            "semantic_action",
            "payload_digest",
            "operation_object_digest",
            "operation_version",
            "actor_brain_id",
            "route_selector",
            "mission_id",
            "mission_head_id",
            "recovery_payload",
        ],
        sequence,
        "prepare",
    )
}

fn require_exact_fields(
    object: &serde_json::Map<String, Value>,
    expected: &[&str],
    sequence: u64,
    context: &str,
) -> Result<(), ExternalMutationJournalError> {
    let missing = expected
        .iter()
        .copied()
        .filter(|field| !object.contains_key(*field))
        .collect::<Vec<_>>();
    let unknown = object
        .keys()
        .filter(|field| !expected.contains(&field.as_str()))
        .map(String::as_str)
        .collect::<Vec<_>>();
    if missing.is_empty() && unknown.is_empty() {
        return Ok(());
    }
    Err(incomplete_record_shape(
        sequence,
        context,
        format!("missing={missing:?}, unknown={unknown:?}"),
    ))
}

fn incomplete_record_shape(
    sequence: u64,
    context: &str,
    detail: impl fmt::Display,
) -> ExternalMutationJournalError {
    ExternalMutationJournalError::Corruption {
        detail: format!(
            "legacy or incomplete journal record at sequence {sequence} ({context}: {detail}); \
             v2 is fresh-root-only and performs no implicit migration"
        ),
    }
}

fn apply_record(
    entries: &mut BTreeMap<String, ExternalMutationJournalEntryV1>,
    record: &ExternalMutationJournalRecordV1,
) -> Result<(), ExternalMutationJournalError> {
    let next = &record.core.entry;
    match entries.get(&next.operation_id) {
        None if next.phase == ExternalMutationJournalPhaseV1::Prepared => {}
        Some(previous)
            if previous.reservation_id == next.reservation_id
                && previous.lease_id == next.lease_id
                && previous.authorization_snapshot_digest == next.authorization_snapshot_digest
                && previous.prepare == next.prepare
                && commit_fields_follow_transition(previous, next)
                && legal_transition(previous.phase, next.phase) => {}
        _ => {
            return Err(ExternalMutationJournalError::Corruption {
                detail: format!("illegal transition for {}", next.operation_id),
            })
        }
    }
    let mut persisted = next.clone();
    if next.phase == ExternalMutationJournalPhaseV1::Committed {
        persisted.commit_record_digest = Some(record.record_digest.clone());
    } else if let Some(previous) = entries.get(&next.operation_id) {
        persisted.commit_record_digest = previous.commit_record_digest.clone();
    }
    entries.insert(next.operation_id.clone(), persisted);
    Ok(())
}

fn commit_fields_follow_transition(
    previous: &ExternalMutationJournalEntryV1,
    next: &ExternalMutationJournalEntryV1,
) -> bool {
    let commit_fields_match = next.outcome_digest == previous.outcome_digest
        && next.commit_record_digest == previous.commit_record_digest
        && next.committed_at == previous.committed_at;
    match (previous.phase, next.phase) {
        (ExternalMutationJournalPhaseV1::Prepared, ExternalMutationJournalPhaseV1::Committed) => {
            previous.outcome_digest.is_none()
                && previous.commit_record_digest.is_none()
                && previous.committed_at.is_none()
                && previous.published_result.is_none()
                && previous.published_result_digest.is_none()
                && previous.reconciliation_receipt.is_none()
                && previous.reconciliation_receipt_digest.is_none()
                && next.outcome_digest.is_some()
                && next.commit_record_digest.is_none()
                && next.committed_at.is_some()
                && next.published_result.is_none()
                && next.published_result_digest.is_none()
                && next.reconciliation_receipt.is_none()
                && next.reconciliation_receipt_digest.is_none()
        }
        (
            ExternalMutationJournalPhaseV1::Committed
            | ExternalMutationJournalPhaseV1::RecoveryRequired,
            ExternalMutationJournalPhaseV1::Reconciled,
        ) => {
            commit_fields_match
                && previous.published_result.is_none()
                && previous.published_result_digest.is_none()
                && next.published_result.is_some()
                && next.published_result_digest.is_some()
                && previous.reconciliation_receipt.is_none()
                && previous.reconciliation_receipt_digest.is_none()
                && next.reconciliation_receipt.is_some()
                && next.reconciliation_receipt_digest.is_some()
        }
        (
            ExternalMutationJournalPhaseV1::Committed
            | ExternalMutationJournalPhaseV1::RecoveryRequired,
            ExternalMutationJournalPhaseV1::Published
            | ExternalMutationJournalPhaseV1::RecoveryRequired,
        ) => {
            commit_fields_match
                && next.reconciliation_receipt == previous.reconciliation_receipt
                && next.reconciliation_receipt_digest == previous.reconciliation_receipt_digest
                && if next.phase == ExternalMutationJournalPhaseV1::Published {
                    previous.published_result.is_none()
                        && previous.published_result_digest.is_none()
                        && next.published_result.is_some()
                        && next.published_result_digest.is_some()
                } else {
                    next.published_result == previous.published_result
                        && next.published_result_digest == previous.published_result_digest
                }
        }
        (ExternalMutationJournalPhaseV1::Reconciled, ExternalMutationJournalPhaseV1::Published) => {
            commit_fields_match
                && next.reconciliation_receipt == previous.reconciliation_receipt
                && next.reconciliation_receipt_digest == previous.reconciliation_receipt_digest
                && next.published_result == previous.published_result
                && next.published_result_digest == previous.published_result_digest
        }
        _ => false,
    }
}

fn legal_transition(
    previous: ExternalMutationJournalPhaseV1,
    next: ExternalMutationJournalPhaseV1,
) -> bool {
    matches!(
        (previous, next),
        (
            ExternalMutationJournalPhaseV1::Prepared,
            ExternalMutationJournalPhaseV1::Committed
        ) | (
            ExternalMutationJournalPhaseV1::Committed,
            ExternalMutationJournalPhaseV1::Published
                | ExternalMutationJournalPhaseV1::Reconciled
                | ExternalMutationJournalPhaseV1::RecoveryRequired
        ) | (
            ExternalMutationJournalPhaseV1::RecoveryRequired,
            ExternalMutationJournalPhaseV1::RecoveryRequired
                | ExternalMutationJournalPhaseV1::Reconciled
                | ExternalMutationJournalPhaseV1::Published
        ) | (
            ExternalMutationJournalPhaseV1::Reconciled,
            ExternalMutationJournalPhaseV1::Published
        )
    )
}

fn validate_entry(
    entry: &ExternalMutationJournalEntryV1,
) -> Result<(), ExternalMutationJournalError> {
    if !is_digest(&entry.operation_id)
        || !is_digest(&entry.reservation_id)
        || entry.lease_id.trim().is_empty()
        || !is_digest(&entry.authorization_snapshot_digest)
        || !is_digest(&entry.prepare.payload_digest)
        || !is_digest(&entry.prepare.operation_object_digest)
        || entry.prepare.semantic_action.trim().is_empty()
        || entry.prepare.operation_version == 0
        || entry.prepare.actor_brain_id.trim().is_empty()
        || entry
            .prepare
            .route_selector
            .as_deref()
            .is_some_and(|route| route.trim().is_empty())
        || entry.prepared_at > entry.updated_at
        || entry
            .outcome_digest
            .as_deref()
            .is_some_and(|value| !is_digest(value))
        || entry
            .commit_record_digest
            .as_deref()
            .is_some_and(|value| !is_digest(value))
        || entry
            .published_result_digest
            .as_deref()
            .is_some_and(|value| !is_digest(value))
        || entry
            .reconciliation_receipt_digest
            .as_deref()
            .is_some_and(|value| !is_digest(value))
        || entry
            .committed_at
            .is_some_and(|value| value < entry.prepared_at || value > entry.updated_at)
        || (entry.phase == ExternalMutationJournalPhaseV1::Prepared
            && (entry.outcome_digest.is_some()
                || entry.commit_record_digest.is_some()
                || entry.committed_at.is_some()
                || entry.published_result.is_some()
                || entry.published_result_digest.is_some()
                || entry.reconciliation_receipt.is_some()
                || entry.reconciliation_receipt_digest.is_some()))
        || (entry.phase == ExternalMutationJournalPhaseV1::Committed
            && (entry.outcome_digest.is_none()
                || entry.commit_record_digest.is_some()
                || entry.committed_at.is_none()
                || entry.published_result.is_some()
                || entry.published_result_digest.is_some()
                || entry.reconciliation_receipt.is_some()
                || entry.reconciliation_receipt_digest.is_some()))
        || (entry.phase == ExternalMutationJournalPhaseV1::RecoveryRequired
            && (entry.outcome_digest.is_none()
                || entry.commit_record_digest.is_none()
                || entry.committed_at.is_none()
                || entry.published_result.is_some()
                || entry.published_result_digest.is_some()
                || entry.reconciliation_receipt.is_some()
                || entry.reconciliation_receipt_digest.is_some()))
        || (entry.phase == ExternalMutationJournalPhaseV1::Reconciled
            && (entry.prepare.semantic_action != "brain.promote"
                || entry.outcome_digest.is_none()
                || entry.commit_record_digest.is_none()
                || entry.committed_at.is_none()
                || entry.published_result.is_none()
                || entry.published_result_digest.is_none()
                || entry.reconciliation_receipt.is_none()
                || entry.reconciliation_receipt_digest.is_none()))
        || (entry.phase == ExternalMutationJournalPhaseV1::Published
            && (entry.outcome_digest.is_none()
                || entry.commit_record_digest.is_none()
                || entry.committed_at.is_none()
                || entry.published_result.is_none()
                || entry.published_result_digest.is_none()
                || (entry.prepare.semantic_action == "brain.promote"
                    && (entry.reconciliation_receipt.is_none()
                        || entry.reconciliation_receipt_digest.is_none()))
                || (entry.prepare.semantic_action != "brain.promote"
                    && (entry.reconciliation_receipt.is_some()
                        || entry.reconciliation_receipt_digest.is_some()))))
    {
        return Err(ExternalMutationJournalError::Corruption {
            detail: "invalid external mutation journal entry".to_string(),
        });
    }
    if matches!(
        entry.phase,
        ExternalMutationJournalPhaseV1::Reconciled | ExternalMutationJournalPhaseV1::Published
    ) && entry.prepare.semantic_action == "brain.promote"
    {
        let receipt = entry.reconciliation_receipt.as_ref().ok_or_else(|| {
            ExternalMutationJournalError::Corruption {
                detail: "reconciled promotion has no receipt".to_string(),
            }
        })?;
        validate_reconciliation_receipt_binding(entry, receipt)?;
        let expected =
            digest_canonical(BRAIN_PROMOTE_RECONCILIATION_RECEIPT_DIGEST_DOMAIN, receipt)?;
        if entry.reconciliation_receipt_digest.as_deref() != Some(expected.as_str()) {
            return Err(ExternalMutationJournalError::Corruption {
                detail: "promotion reconciliation receipt digest mismatch".to_string(),
            });
        }
    }
    if matches!(
        entry.phase,
        ExternalMutationJournalPhaseV1::Reconciled | ExternalMutationJournalPhaseV1::Published
    ) {
        let published_result = entry.published_result.as_ref().ok_or_else(|| {
            ExternalMutationJournalError::Corruption {
                detail: "reconciled/published entry has no sealed result".to_string(),
            }
        })?;
        validate_published_result_binding(entry, published_result)?;
        let expected = digest_canonical(
            EXTERNAL_MUTATION_PUBLISHED_RESULT_DIGEST_DOMAIN,
            published_result,
        )?;
        if entry.published_result_digest.as_deref() != Some(expected.as_str()) {
            return Err(ExternalMutationJournalError::Corruption {
                detail: "reconciled/published result digest mismatch".to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn validate_entry_for_test(
    entry: &ExternalMutationJournalEntryV1,
) -> Result<(), ExternalMutationJournalError> {
    validate_entry(entry)
}

fn validate_published_result_binding(
    entry: &ExternalMutationJournalEntryV1,
    published_result: &ExternalMutationPublishedResultV1,
) -> Result<(), ExternalMutationJournalError> {
    let actor_reconciled = matches!(
        entry.prepare.semantic_action.as_str(),
        "brain.promote" | "graph.ingest.replace" | "graph.ingest.merge_existing"
    );
    let expected_reconciliation_state = if actor_reconciled {
        "RECONCILED"
    } else if published_result.graph_resync_required {
        "PENDING_RECONCILIATION"
    } else {
        "NOT_REQUIRED"
    };
    if published_result.schema != EXTERNAL_MUTATION_PUBLISHED_RESULT_SCHEMA
        || published_result.semantic_action != entry.prepare.semantic_action
        || published_result.semantic_payload_digest != entry.prepare.payload_digest
        || published_result.operation_object_digest != entry.prepare.operation_object_digest
        || entry.outcome_digest.as_deref() != Some(published_result.outcome_digest.as_str())
        || !is_digest(&published_result.outcome_digest)
        || published_result.reconciliation_state != expected_reconciliation_state
        || (actor_reconciled && published_result.graph_resync_required)
    {
        return Err(ExternalMutationJournalError::refused(
            "external_mutation_published_result_binding_mismatch",
            "sealed result differs from the committed operation bindings",
        ));
    }
    Ok(())
}

fn validate_reconciliation_receipt_binding(
    entry: &ExternalMutationJournalEntryV1,
    receipt: &BrainPromoteReconciliationReceiptV1,
) -> Result<(), ExternalMutationJournalError> {
    let recovery_kind = entry
        .prepare
        .recovery_payload
        .get("kind")
        .and_then(Value::as_str);
    let recovery_medulla_path = entry
        .prepare
        .recovery_payload
        .get("medulla")
        .and_then(Value::as_object)
        .and_then(|medulla| medulla.get("target_path"))
        .and_then(Value::as_str);
    let recovery_medulla_postimage = entry
        .prepare
        .recovery_payload
        .get("medulla")
        .and_then(Value::as_object)
        .and_then(|medulla| medulla.get("after_sha256"))
        .and_then(Value::as_str);
    let recovery_reconciliation_brain_id = entry
        .prepare
        .recovery_payload
        .get("reconciliation_brain_id")
        .and_then(Value::as_str);
    let expected_checkpoint_ack_digest = digest_canonical(
        BRAIN_PROMOTE_CHECKPOINT_ACK_DIGEST_DOMAIN,
        &receipt.checkpoint_ack,
    )?;
    if receipt.schema != BRAIN_PROMOTE_RECONCILIATION_RECEIPT_SCHEMA
        || entry.prepare.semantic_action != "brain.promote"
        || recovery_kind != Some("brain_promote")
        || receipt.operation_id != entry.operation_id
        || receipt.operation_object_digest != entry.prepare.operation_object_digest
        || entry.prepare.route_selector.as_deref() != Some(receipt.source_brain_id.as_str())
        || receipt.reconciliation_brain_id.trim().is_empty()
        || recovery_reconciliation_brain_id != Some(receipt.reconciliation_brain_id.as_str())
        || receipt.medulla_path.trim().is_empty()
        || recovery_medulla_path != Some(receipt.medulla_path.as_str())
        || !is_digest(&receipt.medulla_postimage_sha256)
        || recovery_medulla_postimage != Some(receipt.medulla_postimage_sha256.as_str())
        || receipt.adapter != "light"
        || receipt.mode != "merge"
        || receipt.incremental
        || receipt.namespace != "light"
        || !is_digest(&receipt.ingest_output_digest)
        || receipt.graph_generation_after <= receipt.graph_generation_before
        || receipt.checkpoint_ack.schema != BRAIN_PROMOTE_CHECKPOINT_ACK_SCHEMA
        || !is_digest(&receipt.checkpoint_ack.checkpoint_id)
        || receipt.checkpoint_ack.brain_id != receipt.reconciliation_brain_id
        || receipt.checkpoint_ack.generation < receipt.graph_generation_after
        || !is_digest(&receipt.checkpoint_ack.current_pointer_digest)
        || receipt.checkpoint_ack.confirmed_at_unix_ms == 0
        || receipt.checkpoint_ack.confirmed_at_unix_ms > receipt.reconciled_at
        || receipt.checkpoint_ack_digest != expected_checkpoint_ack_digest
        || receipt.reconciled_at < entry.committed_at.unwrap_or(entry.prepared_at)
        || receipt.reconciled_at > entry.updated_at
    {
        return Err(ExternalMutationJournalError::refused(
            "brain_promote_reconciliation_receipt_binding_mismatch",
            "reconciliation receipt differs from the exact committed promotion",
        ));
    }
    Ok(())
}

fn refuse_symlink(path: &Path) -> Result<(), ExternalMutationJournalError> {
    if path
        .symlink_metadata()
        .is_ok_and(|metadata| metadata_is_link_or_reparse(&metadata))
    {
        return Err(ExternalMutationJournalError::refused(
            "external_mutation_journal_symlink_refused",
            path.display().to_string(),
        ));
    }
    Ok(())
}

fn metadata_is_link_or_reparse(metadata: &std::fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        crate::windows_durable_fs::is_reparse_point(metadata)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(unix)]
fn open_journal_no_follow(path: &Path) -> std::io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(windows)]
fn open_journal_no_follow(path: &Path) -> std::io::Result<File> {
    crate::windows_durable_fs::open_read_append_create_no_follow(path)
}

#[cfg(all(not(unix), not(windows)))]
fn open_journal_no_follow(_path: &Path) -> std::io::Result<File> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "external mutation journal no-follow durable open is not proven on this platform",
    ))
}

#[cfg(unix)]
fn truncate_journal_tail(file: &File, _path: &Path, complete_len: u64) -> std::io::Result<()> {
    file.set_len(complete_len)?;
    file.sync_all()
}

#[cfg(windows)]
fn truncate_journal_tail(_file: &File, path: &Path, complete_len: u64) -> std::io::Result<()> {
    // Append-only Windows journal handles cannot `SetEndOfFile`; truncate the
    // torn tail through a dedicated write handle instead.
    crate::windows_durable_fs::truncate_no_follow(path, complete_len)
}

#[cfg(all(not(unix), not(windows)))]
fn truncate_journal_tail(_file: &File, _path: &Path, _complete_len: u64) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "external mutation journal torn-tail truncation is not proven on this platform",
    ))
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    File::open(parent)?.sync_all()
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

    const PREPARED_AT: u64 = 100;
    const COMMITTED_AT: u64 = 110;
    const RECONCILED_AT: u64 = 120;
    const SOURCE_BRAIN_ID: &str = "project-brain-source";
    const RECONCILIATION_BRAIN_ID: &str = "medulla-brain";
    const MEDULLA_TARGET: &str = "/managed/medulla/promoted.light.md";

    fn test_digest(label: &str) -> String {
        digest_canonical("m1nd-external-mutation-journal-test-v1", &label).expect("test digest")
    }

    fn journal_length(root: &Path) -> u64 {
        std::fs::metadata(root.join(JOURNAL_FILE))
            .expect("journal metadata")
            .len()
    }

    fn committed_fixture(
        root: &Path,
    ) -> (
        ExternalMutationJournalV1,
        String,
        BrainPromoteReconciliationReceiptV1,
        ExternalMutationPublishedResultV1,
    ) {
        let operation_object_digest = test_digest("operation-object");
        let reservation = AuthorizationReservationV1 {
            reservation_id: test_digest("reservation"),
            lease_id: "lease-journal-test".to_string(),
            operation_object_digest: operation_object_digest.clone(),
            transaction_id: None,
            transport_session_id: "transport-journal-test".to_string(),
            ingress_context_digest: test_digest("ingress"),
            reserved_at: PREPARED_AT,
            reservation_expires_at: 1_000,
        };
        let payload_digest = test_digest("semantic-payload");
        let medulla_postimage_sha256 = test_digest("medulla-postimage");
        let prepare = ExternalMutationPrepareV1 {
            semantic_action: "brain.promote".to_string(),
            payload_digest: payload_digest.clone(),
            operation_object_digest: operation_object_digest.clone(),
            operation_version: 1,
            actor_brain_id: RECONCILIATION_BRAIN_ID.to_string(),
            route_selector: Some(SOURCE_BRAIN_ID.to_string()),
            mission_id: None,
            mission_head_id: None,
            recovery_payload: serde_json::json!({
                "kind": "brain_promote",
                "reconciliation_brain_id": RECONCILIATION_BRAIN_ID,
                "medulla": {
                    "target_path": MEDULLA_TARGET,
                    "after_sha256": medulla_postimage_sha256,
                },
            }),
        };
        let authorization_snapshot_digest = test_digest("authorization-snapshot");
        let outcome_digest = test_digest("outcome");
        let mut journal = ExternalMutationJournalV1::open(root, None).expect("open test journal");
        let prepared = journal
            .prepare(
                &reservation,
                &authorization_snapshot_digest,
                prepare,
                PREPARED_AT,
            )
            .expect("prepare operation");
        journal
            .commit(&prepared.operation_id, outcome_digest.clone(), COMMITTED_AT)
            .expect("commit operation");

        let checkpoint_ack = BrainPromoteCheckpointAckV1 {
            schema: BRAIN_PROMOTE_CHECKPOINT_ACK_SCHEMA.to_string(),
            checkpoint_id: test_digest("checkpoint-id"),
            brain_id: RECONCILIATION_BRAIN_ID.to_string(),
            epoch: 1,
            generation: 2,
            revision: 3,
            current_pointer_digest: test_digest("checkpoint-pointer"),
            confirmed_at_unix_ms: RECONCILED_AT,
        };
        let checkpoint_ack_digest =
            digest_canonical(BRAIN_PROMOTE_CHECKPOINT_ACK_DIGEST_DOMAIN, &checkpoint_ack)
                .expect("checkpoint ACK digest");
        let receipt = BrainPromoteReconciliationReceiptV1 {
            schema: BRAIN_PROMOTE_RECONCILIATION_RECEIPT_SCHEMA.to_string(),
            operation_id: prepared.operation_id.clone(),
            operation_object_digest: operation_object_digest.clone(),
            source_brain_id: SOURCE_BRAIN_ID.to_string(),
            reconciliation_brain_id: RECONCILIATION_BRAIN_ID.to_string(),
            medulla_path: MEDULLA_TARGET.to_string(),
            medulla_postimage_sha256,
            adapter: "light".to_string(),
            mode: "merge".to_string(),
            incremental: false,
            namespace: "light".to_string(),
            ingest_output_digest: test_digest("ingest-output"),
            graph_generation_before: 1,
            graph_generation_after: 2,
            checkpoint_ack,
            checkpoint_ack_digest,
            reconciled_at: RECONCILED_AT,
        };
        let published_result = ExternalMutationPublishedResultV1 {
            schema: EXTERNAL_MUTATION_PUBLISHED_RESULT_SCHEMA.to_string(),
            semantic_action: "brain.promote".to_string(),
            semantic_payload_digest: payload_digest,
            operation_object_digest,
            outcome_digest,
            graph_resync_required: false,
            reconciliation_state: "RECONCILED".to_string(),
            result: serde_json::json!({"promoted": true, "claim": "journal-test"}),
        };
        (journal, prepared.operation_id, receipt, published_result)
    }

    fn assert_invalid_reconciliation_without_append(
        mutate: impl FnOnce(&mut BrainPromoteReconciliationReceiptV1),
    ) {
        let root = tempfile::tempdir().expect("tempdir");
        let (mut journal, operation_id, mut receipt, published_result) =
            committed_fixture(root.path());
        mutate(&mut receipt);
        let length_before = journal_length(root.path());
        let error = journal
            .mark_reconciled(&operation_id, receipt, published_result, RECONCILED_AT)
            .expect_err("invalid reconciliation must refuse");
        assert_eq!(
            error.code(),
            "brain_promote_reconciliation_receipt_binding_mismatch"
        );
        assert_eq!(journal_length(root.path()), length_before);
        drop(journal);
        let reopened =
            ExternalMutationJournalV1::open(root.path(), None).expect("reopen after refusal");
        assert_eq!(
            reopened
                .entry(&operation_id)
                .expect("committed entry")
                .phase,
            ExternalMutationJournalPhaseV1::Committed
        );
    }

    #[test]
    fn reconciled_to_published_uses_exact_sealed_result_and_monotonic_time() {
        let root = tempfile::tempdir().expect("tempdir");
        let (mut journal, operation_id, receipt, published_result) = committed_fixture(root.path());
        journal
            .mark_reconciled(
                &operation_id,
                receipt.clone(),
                published_result.clone(),
                RECONCILED_AT,
            )
            .expect("mark reconciled");

        let reconciled_length = journal_length(root.path());
        journal
            .mark_reconciled(
                &operation_id,
                receipt.clone(),
                published_result.clone(),
                RECONCILED_AT.saturating_add(100),
            )
            .expect("exact reconciled replay");
        assert_eq!(journal_length(root.path()), reconciled_length);

        journal
            .mark_published(&operation_id, published_result.clone(), RECONCILED_AT - 1)
            .expect("publish exact reconciled result");
        let published = journal.entry(&operation_id).expect("published entry");
        assert_eq!(published.phase, ExternalMutationJournalPhaseV1::Published);
        assert_eq!(published.updated_at, RECONCILED_AT);
        assert_eq!(published.reconciliation_receipt.as_ref(), Some(&receipt));
        assert_eq!(published.published_result.as_ref(), Some(&published_result));

        let published_length = journal_length(root.path());
        journal
            .mark_published(&operation_id, published_result.clone(), 1)
            .expect("exact published replay");
        assert_eq!(journal_length(root.path()), published_length);
        drop(journal);

        let reopened =
            ExternalMutationJournalV1::open(root.path(), None).expect("reopen published journal");
        let replayed = reopened.entry(&operation_id).expect("replayed entry");
        assert_eq!(replayed.phase, ExternalMutationJournalPhaseV1::Published);
        assert_eq!(replayed.updated_at, RECONCILED_AT);
        assert_eq!(replayed.reconciliation_receipt.as_ref(), Some(&receipt));
        assert_eq!(replayed.published_result.as_ref(), Some(&published_result));
    }

    #[test]
    fn mismatched_result_receipt_ack_target_and_time_refuse_without_append() {
        let root = tempfile::tempdir().expect("tempdir");
        let (mut journal, operation_id, receipt, published_result) = committed_fixture(root.path());
        journal
            .mark_reconciled(
                &operation_id,
                receipt.clone(),
                published_result.clone(),
                RECONCILED_AT,
            )
            .expect("mark reconciled");

        let length_before_result = journal_length(root.path());
        let mut mismatched_result = published_result.clone();
        mismatched_result.result["tampered"] = Value::Bool(true);
        let result_error = journal
            .mark_published(
                &operation_id,
                mismatched_result,
                RECONCILED_AT.saturating_add(1),
            )
            .expect_err("mismatched result must refuse");
        assert_eq!(
            result_error.code(),
            "external_mutation_published_result_mismatch"
        );
        assert_eq!(journal_length(root.path()), length_before_result);

        let length_before_receipt = journal_length(root.path());
        let mut mismatched_receipt = receipt;
        mismatched_receipt.ingest_output_digest = test_digest("different-ingest-output");
        let receipt_error = journal
            .mark_reconciled(
                &operation_id,
                mismatched_receipt,
                published_result,
                RECONCILED_AT.saturating_add(1),
            )
            .expect_err("mismatched receipt must refuse");
        assert_eq!(
            receipt_error.code(),
            "brain_promote_reconciliation_receipt_mismatch"
        );
        assert_eq!(journal_length(root.path()), length_before_receipt);

        assert_invalid_reconciliation_without_append(|receipt| {
            receipt.checkpoint_ack.brain_id = "foreign-checkpoint-brain".to_string();
            receipt.checkpoint_ack_digest = digest_canonical(
                BRAIN_PROMOTE_CHECKPOINT_ACK_DIGEST_DOMAIN,
                &receipt.checkpoint_ack,
            )
            .expect("foreign ACK digest");
        });
        assert_invalid_reconciliation_without_append(|receipt| {
            receipt.medulla_path = "/foreign/medulla.light.md".to_string();
        });
        assert_invalid_reconciliation_without_append(|receipt| {
            receipt.reconciled_at = COMMITTED_AT - 1;
        });
    }

    #[test]
    fn reopen_rejects_semantically_invalid_but_resealed_terminal_record() {
        let root = tempfile::tempdir().expect("tempdir");
        let (mut journal, operation_id, receipt, published_result) = committed_fixture(root.path());
        journal
            .mark_reconciled(
                &operation_id,
                receipt,
                published_result.clone(),
                RECONCILED_AT,
            )
            .expect("mark reconciled");
        journal
            .mark_published(
                &operation_id,
                published_result,
                RECONCILED_AT.saturating_add(1),
            )
            .expect("mark published");
        drop(journal);

        let path = root.path().join(JOURNAL_FILE);
        let journal_text = std::fs::read_to_string(&path).expect("read journal");
        let mut records = journal_text.lines().map(str::to_string).collect::<Vec<_>>();
        let terminal = records.last_mut().expect("terminal record");
        let mut record: ExternalMutationJournalRecordV1 =
            serde_json::from_str(terminal).expect("decode terminal record");
        let receipt = record
            .core
            .entry
            .reconciliation_receipt
            .as_mut()
            .expect("terminal reconciliation receipt");
        receipt.medulla_path = "/foreign/medulla.light.md".to_string();
        let tampered_receipt_digest =
            digest_canonical(BRAIN_PROMOTE_RECONCILIATION_RECEIPT_DIGEST_DOMAIN, receipt)
                .expect("tampered receipt digest");
        record.core.entry.reconciliation_receipt_digest = Some(tampered_receipt_digest);
        record.seal().expect("reseal terminal record");
        *terminal = serde_json::to_string(&record).expect("encode terminal record");
        std::fs::write(&path, format!("{}\n", records.join("\n"))).expect("rewrite journal");

        let error = match ExternalMutationJournalV1::open(root.path(), None) {
            Ok(_) => panic!("semantically invalid resealed record must fail reopen"),
            Err(error) => error,
        };
        assert_eq!(
            error.code(),
            "brain_promote_reconciliation_receipt_binding_mismatch"
        );
    }

    #[test]
    fn legacy_ambiguous_brain_id_record_is_fresh_root_only() {
        let root = tempfile::tempdir().expect("tempdir");
        let (journal, _, _, _) = committed_fixture(root.path());
        drop(journal);

        let path = root.path().join(JOURNAL_FILE);
        let journal_text = std::fs::read_to_string(&path).expect("read journal");
        let mut records = journal_text.lines().map(str::to_string).collect::<Vec<_>>();
        let first = records.first_mut().expect("prepared record");
        let mut incomplete: Value = serde_json::from_str(first).expect("decode prepared record");
        let prepare = incomplete["core"]["entry"]["prepare"]
            .as_object_mut()
            .expect("prepare object");
        prepare.remove("actor_brain_id");
        prepare.remove("route_selector");
        prepare.insert(
            "brain_id".to_string(),
            Value::String(SOURCE_BRAIN_ID.to_string()),
        );
        *first = serde_json::to_string(&incomplete).expect("encode incomplete record");
        std::fs::write(&path, format!("{}\n", records.join("\n"))).expect("rewrite journal");

        let error = match ExternalMutationJournalV1::open(root.path(), None) {
            Ok(_) => panic!("legacy/incomplete record must fail reopen"),
            Err(error) => error,
        };
        assert_eq!(error.code(), "external_mutation_journal_corruption");
        assert!(error.to_string().contains("actor_brain_id"));
        assert!(error.to_string().contains("route_selector"));
        assert!(error.to_string().contains("brain_id"));
        assert!(error.to_string().contains("fresh-root-only"));
    }
}
