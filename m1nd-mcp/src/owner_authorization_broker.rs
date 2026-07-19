//! Durable one-shot G2→G3 authorization broker.
//!
//! Lock order is normative and intentionally small:
//! `OWNER_AUTHORITY_TRANSACTION_V1` → AuthorityRuntime owner serial → broker
//! journal → AuthorityWAL writer. The broker never calls a provider while it
//! holds the WAL lock. A sovereign WAL finalization is exposed only as one
//! callback executed while the named linearization lock is held, so RED/freeze,
//! epoch and expiry revalidation cannot race the commit marker.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use m1nd_control::{
    canonical_json_string, digest_canonical, ActiveMode, AuthorityTransactionV1, AuthorityWalPhase,
    CanonicalError,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::authority_runtime::{
    AuthorityAuthorizationReceiptV1, AuthorityRuntimeStatusV1, AuthorizationAuthorityV1,
};
use crate::authority_wal::VerifiedAuthorityWalCommitWitnessV1;
use crate::light_author_handlers::LockGuard;
use crate::mission_service::{
    AuthenticatedAuthorityContextV1, AUTHENTICATED_AUTHORITY_CONTEXT_SCHEMA,
};
use crate::protected_journal_head::{
    advance_protected_head, verify_or_initialize_protected_head, ProtectedJournalHeadSnapshotV1,
    SharedProtectedJournalHeadBackendV1, OWNER_AUTHORIZATION_BROKER_HEAD_DOMAIN,
};

pub const OWNER_AUTHORIZATION_LEASE_SCHEMA: &str = "m1nd-owner-authorization-lease-v1";
pub const OWNER_AUTHORIZATION_BROKER_RECORD_SCHEMA: &str =
    "m1nd-owner-authorization-broker-record-v1";
pub const OWNER_AUTHORIZATION_BROKER_RECORD_DIGEST_DOMAIN: &str =
    "m1nd-owner-authorization-broker-record-v1";
pub const OWNER_AUTHORIZATION_RESERVATION_DIGEST_DOMAIN: &str =
    "m1nd-owner-authorization-reservation-v1";
pub const OWNER_AUTHORITY_TRANSACTION_LINEARIZATION_POINT: &str = "OWNER_AUTHORITY_TRANSACTION_V1";

const BROKER_JOURNAL_FILE: &str = "owner-authorization-broker.jsonl";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorizationLeaseStateV1 {
    Unused,
    Reserved,
    Consumed,
    Aborted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuthorizationTerminalKindV1 {
    OperationAdmitted,
    ExternalMutationCommitted,
    ReadCompleted,
    WalCommitted,
    Aborted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationReservationV1 {
    pub reservation_id: String,
    pub lease_id: String,
    /// Exact object executed by the consumer. For ordinary/service operations
    /// this equals the receipt object. For landing it is the final transaction
    /// digest whose signed payload/snapshot fields bind the receipt object.
    pub operation_object_digest: String,
    /// Present only for WAL-backed reservations and pinned from the validated
    /// outer transaction. Recovery witnesses must match it exactly.
    pub transaction_id: Option<String>,
    pub transport_session_id: String,
    pub ingress_context_digest: String,
    pub reserved_at: u64,
    pub reservation_expires_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityWalCommitWitnessV1 {
    pub transaction_id: String,
    pub phase: AuthorityWalPhase,
    pub transaction_digest: String,
    pub authorization_snapshot_digest: String,
    pub terminal_record_digest: String,
    pub committed_at: u64,
}

/// Durable witness emitted by the typed external-mutation journal.  The
/// constructor is crate-private so a caller cannot manufacture the value that
/// spends a lease; only the journal can seal and verify a COMMIT record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalMutationCommitWitnessV1 {
    pub reservation_id: String,
    pub lease_id: String,
    pub operation_object_digest: String,
    pub authorization_snapshot_digest: String,
    pub journal_record_digest: String,
    pub committed_at: u64,
}

#[derive(Clone, Debug)]
pub struct VerifiedExternalMutationCommitWitnessV1 {
    witness: ExternalMutationCommitWitnessV1,
}

/// Opaque negative witness emitted only by a fully replayed, protected external
/// journal whose exact operation is still PREPARED and carries no COMMIT fields.
/// It permits expiry-time cleanup of FINALIZATION_PREPARED without guessing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalMutationPreparedAbortWitnessV1 {
    pub reservation_id: String,
    pub lease_id: String,
    pub operation_object_digest: String,
    pub authorization_snapshot_digest: String,
    pub prepared_at: u64,
}

#[derive(Clone, Debug)]
pub struct VerifiedExternalMutationPreparedAbortWitnessV1 {
    witness: ExternalMutationPreparedAbortWitnessV1,
}

/// Opaque proof that a fully replayed protected external journal contains no
/// operation for the exact reserved lease/object. It is used only before any
/// outer PREPARED record exists (for example, process death after reserve or
/// private staging).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalMutationJournalAbsenceWitnessV1 {
    pub reservation_id: String,
    pub lease_id: String,
    pub operation_object_digest: String,
    pub authorization_snapshot_digest: String,
}

#[derive(Clone, Debug)]
pub struct VerifiedExternalMutationJournalAbsenceWitnessV1 {
    witness: ExternalMutationJournalAbsenceWitnessV1,
}

impl VerifiedExternalMutationJournalAbsenceWitnessV1 {
    pub(crate) fn new(witness: ExternalMutationJournalAbsenceWitnessV1) -> Self {
        Self { witness }
    }

    pub fn witness(&self) -> &ExternalMutationJournalAbsenceWitnessV1 {
        &self.witness
    }
}

impl VerifiedExternalMutationPreparedAbortWitnessV1 {
    pub(crate) fn new(witness: ExternalMutationPreparedAbortWitnessV1) -> Self {
        Self { witness }
    }

    pub fn witness(&self) -> &ExternalMutationPreparedAbortWitnessV1 {
        &self.witness
    }
}

impl VerifiedExternalMutationCommitWitnessV1 {
    pub(crate) fn new(witness: ExternalMutationCommitWitnessV1) -> Self {
        Self { witness }
    }

    pub fn witness(&self) -> &ExternalMutationCommitWitnessV1 {
        &self.witness
    }

    fn into_witness(self) -> ExternalMutationCommitWitnessV1 {
        self.witness
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationFinalizationSnapshotV1 {
    pub active_mode: ActiveMode,
    pub constitution_epoch: u64,
    pub autonomy_epoch: u64,
    pub protected_epoch: u64,
    pub journal_root_digest: String,
    pub revalidated_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationTerminalV1 {
    pub kind: AuthorizationTerminalKindV1,
    pub outcome_digest: String,
    pub wal_witness: Option<AuthorityWalCommitWitnessV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_mutation_witness: Option<ExternalMutationCommitWitnessV1>,
    pub terminal_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerAuthorizationLeaseV1 {
    pub schema: String,
    pub lease_id: String,
    pub authorization_receipt: AuthorityAuthorizationReceiptV1,
    pub state: AuthorizationLeaseStateV1,
    pub reservation: Option<AuthorizationReservationV1>,
    pub finalization_snapshot: Option<AuthorizationFinalizationSnapshotV1>,
    pub terminal: Option<AuthorizationTerminalV1>,
    pub issued_at: u64,
    pub expires_at: u64,
    pub retain_until: u64,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum BrokerJournalEventKindV1 {
    Issued,
    Reserved,
    FinalizationPrepared,
    Consumed,
    Aborted,
    GcTombstone,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrokerJournalRecordCoreV1 {
    sequence: u64,
    event_kind: BrokerJournalEventKindV1,
    lease_id: String,
    lease: Option<OwnerAuthorizationLeaseV1>,
    previous_record_digest: Option<String>,
    recorded_at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrokerJournalRecordV1 {
    schema: String,
    core: BrokerJournalRecordCoreV1,
    record_digest: String,
}

impl BrokerJournalRecordV1 {
    fn seal(&mut self) -> Result<(), CanonicalError> {
        self.record_digest =
            digest_canonical(OWNER_AUTHORIZATION_BROKER_RECORD_DIGEST_DOMAIN, &self.core)?;
        Ok(())
    }

    fn validate(
        &self,
        expected_sequence: u64,
        expected_previous: Option<&str>,
    ) -> Result<(), OwnerAuthorizationBrokerError> {
        if self.schema != OWNER_AUTHORIZATION_BROKER_RECORD_SCHEMA
            || self.core.sequence != expected_sequence
            || self.core.previous_record_digest.as_deref() != expected_previous
            || self.core.lease_id.trim().is_empty()
        {
            return Err(OwnerAuthorizationBrokerError::Corruption {
                detail: format!("broker chain mismatch at sequence {expected_sequence}"),
            });
        }
        let computed =
            digest_canonical(OWNER_AUTHORIZATION_BROKER_RECORD_DIGEST_DOMAIN, &self.core)?;
        if !is_digest(&self.record_digest) || computed != self.record_digest {
            return Err(OwnerAuthorizationBrokerError::Corruption {
                detail: format!("broker digest mismatch at sequence {expected_sequence}"),
            });
        }
        match self.core.event_kind {
            BrokerJournalEventKindV1::GcTombstone if self.core.lease.is_some() => {
                return Err(OwnerAuthorizationBrokerError::Corruption {
                    detail: "GC tombstone unexpectedly carries a lease".to_string(),
                });
            }
            BrokerJournalEventKindV1::GcTombstone => {}
            _ if self.core.lease.is_none() => {
                return Err(OwnerAuthorizationBrokerError::Corruption {
                    detail: "non-GC broker record omits lease snapshot".to_string(),
                });
            }
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct OwnerAuthorityLinearizationV1 {
    lock: Arc<Mutex<()>>,
}

impl Default for OwnerAuthorityLinearizationV1 {
    fn default() -> Self {
        Self {
            lock: Arc::new(Mutex::new(())),
        }
    }
}

impl OwnerAuthorityLinearizationV1 {
    pub const fn name(&self) -> &'static str {
        OWNER_AUTHORITY_TRANSACTION_LINEARIZATION_POINT
    }
}

#[derive(Clone, Debug)]
pub struct OwnerAuthorizationBrokerConfigV1 {
    pub root: PathBuf,
    pub reservation_ttl_ms: u64,
    pub minimum_terminal_retention_ms: u64,
}

#[derive(Debug)]
pub enum OwnerAuthorizationBrokerError {
    Io {
        operation: &'static str,
        source: std::io::Error,
    },
    Json(serde_json::Error),
    Canonical(CanonicalError),
    WriterLock(String),
    Corruption {
        detail: String,
    },
    Refused {
        code: &'static str,
        detail: String,
    },
    CommitCallback {
        detail: String,
    },
    ExternalCommitCallback {
        detail: String,
    },
    ProtectedHead {
        detail: String,
    },
    Poisoned,
}

impl OwnerAuthorizationBrokerError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Refused { code, .. } => code,
            Self::ExternalCommitCallback { .. } => "external_mutation_commit_failed",
            Self::Corruption { .. } => "authorization_broker_corruption",
            Self::CommitCallback { .. } => "authority_wal_commit_failed",
            Self::ProtectedHead { .. } => "authorization_broker_rollback_detected",
            Self::Poisoned => "authorization_broker_poisoned",
            Self::Io { .. } | Self::Json(_) | Self::Canonical(_) | Self::WriterLock(_) => {
                "authorization_broker_unavailable"
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

impl fmt::Display for OwnerAuthorizationBrokerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, source } => write!(formatter, "{operation}: {source}"),
            Self::Json(error) => write!(formatter, "broker JSON: {error}"),
            Self::Canonical(error) => write!(formatter, "broker canonicalization: {error}"),
            Self::WriterLock(detail) => write!(formatter, "broker writer lock: {detail}"),
            Self::Corruption { detail } => write!(formatter, "broker corruption: {detail}"),
            Self::Refused { code, detail } => write!(formatter, "{code}: {detail}"),
            Self::CommitCallback { detail } => write!(formatter, "WAL commit callback: {detail}"),
            Self::ExternalCommitCallback { detail } => {
                write!(formatter, "external mutation commit callback: {detail}")
            }
            Self::ProtectedHead { detail } => {
                write!(formatter, "broker protected head: {detail}")
            }
            Self::Poisoned => formatter.write_str("authorization broker is poisoned"),
        }
    }
}

impl Error for OwnerAuthorizationBrokerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json(error) => Some(error),
            Self::Canonical(error) => Some(error),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for OwnerAuthorizationBrokerError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<CanonicalError> for OwnerAuthorizationBrokerError {
    fn from(error: CanonicalError) -> Self {
        Self::Canonical(error)
    }
}

pub struct OwnerAuthorizationBrokerV1 {
    config: OwnerAuthorizationBrokerConfigV1,
    linearization: OwnerAuthorityLinearizationV1,
    path: PathBuf,
    file: File,
    sequence: u64,
    tail_digest: Option<String>,
    known_len: u64,
    leases: BTreeMap<String, OwnerAuthorizationLeaseV1>,
    poisoned: bool,
    protected_head_backend: Option<SharedProtectedJournalHeadBackendV1>,
    protected_head: Option<ProtectedJournalHeadSnapshotV1>,
    _writer_lock: LockGuard,
}

impl OwnerAuthorizationBrokerV1 {
    pub(crate) fn open(
        config: OwnerAuthorizationBrokerConfigV1,
        linearization: OwnerAuthorityLinearizationV1,
    ) -> Result<Self, OwnerAuthorizationBrokerError> {
        Self::open_internal(config, linearization, None)
    }

    pub(crate) fn open_with_protected_head(
        config: OwnerAuthorizationBrokerConfigV1,
        linearization: OwnerAuthorityLinearizationV1,
        protected_head_backend: SharedProtectedJournalHeadBackendV1,
    ) -> Result<Self, OwnerAuthorizationBrokerError> {
        Self::open_internal(config, linearization, Some(protected_head_backend))
    }

    fn open_internal(
        config: OwnerAuthorizationBrokerConfigV1,
        linearization: OwnerAuthorityLinearizationV1,
        protected_head_backend: Option<SharedProtectedJournalHeadBackendV1>,
    ) -> Result<Self, OwnerAuthorizationBrokerError> {
        if config.reservation_ttl_ms == 0 || config.minimum_terminal_retention_ms == 0 {
            return Err(OwnerAuthorizationBrokerError::refused(
                "invalid_broker_config",
                "reservation TTL and terminal retention must be non-zero",
            ));
        }
        refuse_symlink(&config.root)?;
        std::fs::create_dir_all(&config.root).map_err(|source| {
            OwnerAuthorizationBrokerError::Io {
                operation: "create_broker_root",
                source,
            }
        })?;
        refuse_symlink(&config.root)?;
        let writer_lock = LockGuard::acquire_in(&config.root, "owner-authorization-broker-v1")
            .map_err(|error| OwnerAuthorizationBrokerError::WriterLock(error.to_string()))?;
        let path = config.root.join(BROKER_JOURNAL_FILE);
        refuse_symlink(&path)?;
        let existed = path.exists();
        let mut open_options = OpenOptions::new();
        open_options.create(true).read(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            open_options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        let mut file =
            open_options
                .open(&path)
                .map_err(|source| OwnerAuthorizationBrokerError::Io {
                    operation: "open_broker_journal",
                    source,
                })?;
        if !existed {
            file.sync_all()
                .and_then(|()| sync_parent(&path))
                .map_err(|source| OwnerAuthorizationBrokerError::Io {
                    operation: "sync_new_broker_journal",
                    source,
                })?;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|source| OwnerAuthorizationBrokerError::Io {
                operation: "read_broker_journal",
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
        let mut leases = BTreeMap::new();
        for (index, frame) in bytes[..complete_len]
            .split_inclusive(|byte| *byte == b'\n')
            .enumerate()
        {
            let line = &frame[..frame.len() - 1];
            if line.is_empty() {
                return Err(OwnerAuthorizationBrokerError::Corruption {
                    detail: format!("empty broker record at line {}", index + 1),
                });
            }
            let record: BrokerJournalRecordV1 = serde_json::from_slice(line)?;
            record.validate(sequence + 1, tail_digest.as_deref())?;
            apply_replayed_record(&mut leases, &record)?;
            sequence += 1;
            tail_digest = Some(record.record_digest);
        }
        let protected_head = protected_head_backend
            .as_ref()
            .map(|backend| {
                verify_or_initialize_protected_head(
                    backend,
                    OWNER_AUTHORIZATION_BROKER_HEAD_DOMAIN,
                    sequence,
                    tail_digest.clone(),
                )
                .map_err(|detail| OwnerAuthorizationBrokerError::ProtectedHead { detail })
            })
            .transpose()?;
        if complete_len != bytes.len() {
            file.set_len(complete_len as u64)
                .and_then(|()| file.sync_all())
                .map_err(|source| OwnerAuthorizationBrokerError::Io {
                    operation: "truncate_torn_broker_tail",
                    source,
                })?;
        }
        Ok(Self {
            config,
            linearization,
            path,
            file,
            sequence,
            tail_digest,
            known_len: complete_len as u64,
            leases,
            poisoned: false,
            protected_head_backend,
            protected_head,
            _writer_lock: writer_lock,
        })
    }

    pub fn linearization_name(&self) -> &'static str {
        self.linearization.name()
    }

    pub fn lease(&self, lease_id: &str) -> Option<&OwnerAuthorizationLeaseV1> {
        self.leases.get(lease_id)
    }

    pub(crate) fn leases(&self) -> Vec<OwnerAuthorizationLeaseV1> {
        self.leases.values().cloned().collect()
    }

    /// Build the only production-shaped G3 context: a direct projection of an
    /// exact current reservation and its persisted G2 receipt. The optional
    /// identity-role digest is accepted only for the landing transaction whose
    /// complete digest was bound by `reserve_land`.
    pub fn mission_service_context(
        &self,
        reservation: &AuthorizationReservationV1,
        identity_role_binding_digest: Option<String>,
    ) -> Result<AuthenticatedAuthorityContextV1, OwnerAuthorizationBrokerError> {
        let lease = self.exact_reserved(reservation, reservation.reserved_at, false)?;
        if let Some(digest) = identity_role_binding_digest.as_deref() {
            if !is_digest(digest) {
                return Err(OwnerAuthorizationBrokerError::refused(
                    "invalid_identity_role_binding_digest",
                    "identity-role binding must be a canonical SHA-256 digest",
                ));
            }
        }
        let core = &lease.authorization_receipt.core;
        Ok(AuthenticatedAuthorityContextV1 {
            schema: AUTHENTICATED_AUTHORITY_CONTEXT_SCHEMA.to_string(),
            organism_id: core.organism_id.clone(),
            brain_id: core.brain_id.clone(),
            subject_id: core.subject_id.clone(),
            role: core.role,
            capability_id: core.capability_id.clone(),
            capability_kind: core.capability_kind,
            authority_variant: core.exact_policy_tuple.authority_variant,
            active_mode: core.active_mode,
            mission_id: core.mission_id.clone(),
            mission_head_id: core.mission_head_id.clone(),
            transport_session_id: core.transport_session_id.clone(),
            ingress_context_digest: core.ingress_context_digest.clone(),
            action_id: core.action.as_str().to_string(),
            ingress: core.ingress,
            complete_effects: core.complete_effects.clone(),
            verified_object_digest: reservation.operation_object_digest.clone(),
            authorization_snapshot_digest: lease.authorization_receipt.receipt_digest.clone(),
            authority_decision_digest: core.authority_decision_digest.clone(),
            identity_role_binding_digest,
            upstream_verification_receipt_digest: lease
                .authorization_receipt
                .receipt_digest
                .clone(),
            protected_time_evidence_digest: core.journal_root_digest.clone(),
            constitution_digest: core.constitution_digest.clone(),
            constitution_epoch: core.constitution_epoch,
            autonomy_epoch: core.autonomy_epoch,
            protected_epoch: core.protected_epoch,
            policy_registry_digest: core.policy_registry_digest.clone(),
            authorization_lease_id: lease.lease_id,
            authorization_reservation_id: reservation.reservation_id.clone(),
            authenticated_at: core.authorized_at,
            expires_at: reservation.reservation_expires_at.min(core.expires_at),
        })
    }

    pub(crate) fn issue(
        &mut self,
        lease_id: impl Into<String>,
        receipt: AuthorityAuthorizationReceiptV1,
        now_ms: u64,
    ) -> Result<OwnerAuthorizationLeaseV1, OwnerAuthorizationBrokerError> {
        let lease_id = lease_id.into();
        if lease_id.trim().is_empty()
            || !is_digest(&receipt.receipt_digest)
            || receipt.core.authorized_at > now_ms
            || now_ms >= receipt.core.expires_at
            || receipt.core.verified_object_digest.len() != 64
        {
            return Err(OwnerAuthorizationBrokerError::refused(
                "invalid_authorization_receipt",
                "receipt identity, digest, object, or time binding is invalid",
            ));
        }
        if self.leases.contains_key(&lease_id) {
            return Err(OwnerAuthorizationBrokerError::refused(
                "duplicate_authorization_lease",
                lease_id,
            ));
        }
        let lease = OwnerAuthorizationLeaseV1 {
            schema: OWNER_AUTHORIZATION_LEASE_SCHEMA.to_string(),
            lease_id: lease_id.clone(),
            authorization_receipt: receipt.clone(),
            state: AuthorizationLeaseStateV1::Unused,
            reservation: None,
            finalization_snapshot: None,
            terminal: None,
            issued_at: now_ms,
            expires_at: receipt.core.expires_at,
            retain_until: receipt
                .core
                .expires_at
                .saturating_add(self.config.minimum_terminal_retention_ms),
            revision: 1,
        };
        self.append_snapshot(BrokerJournalEventKindV1::Issued, &lease, now_ms)?;
        Ok(lease)
    }

    pub(crate) fn reserve(
        &mut self,
        lease_id: &str,
        transport_session_id: &str,
        ingress_context_digest: &str,
        operation_object_digest: &str,
        now_ms: u64,
    ) -> Result<AuthorizationReservationV1, OwnerAuthorizationBrokerError> {
        let current = self.leases.get(lease_id).ok_or_else(|| {
            OwnerAuthorizationBrokerError::refused("authorization_lease_not_found", lease_id)
        })?;
        if current.authorization_receipt.core.verified_object_digest != operation_object_digest {
            return Err(OwnerAuthorizationBrokerError::refused(
                "authorization_operation_binding_mismatch",
                "operation digest differs from the verified authorization object",
            ));
        }
        self.reserve_bound_operation(
            lease_id,
            transport_session_id,
            ingress_context_digest,
            operation_object_digest,
            None,
            now_ms,
        )
    }

    /// Reserve a landing operation without a circular receipt/transaction
    /// digest dependency. G2 authorizes the canonical LandIntent digest first;
    /// the final positive transaction then binds that digest plus the resulting
    /// receipt snapshot. The broker validates both and reserves the exact final
    /// transaction digest consumed by AuthorityWAL.
    pub(crate) fn reserve_land(
        &mut self,
        lease_id: &str,
        transport_session_id: &str,
        ingress_context_digest: &str,
        transaction: &AuthorityTransactionV1,
        now_ms: u64,
    ) -> Result<AuthorizationReservationV1, OwnerAuthorizationBrokerError> {
        transaction.validate().map_err(|error| {
            OwnerAuthorizationBrokerError::refused(
                "invalid_land_authority_transaction",
                error.to_string(),
            )
        })?;
        let current = self.leases.get(lease_id).ok_or_else(|| {
            OwnerAuthorizationBrokerError::refused("authorization_lease_not_found", lease_id)
        })?;
        validate_land_transaction_binding(&current.authorization_receipt, transaction, now_ms)?;
        self.reserve_bound_operation(
            lease_id,
            transport_session_id,
            ingress_context_digest,
            transaction.transaction_digest(),
            Some(transaction.binding().transaction_id.as_str()),
            now_ms,
        )
    }

    fn reserve_bound_operation(
        &mut self,
        lease_id: &str,
        transport_session_id: &str,
        ingress_context_digest: &str,
        operation_object_digest: &str,
        transaction_id: Option<&str>,
        now_ms: u64,
    ) -> Result<AuthorizationReservationV1, OwnerAuthorizationBrokerError> {
        if !is_digest(operation_object_digest) {
            return Err(OwnerAuthorizationBrokerError::refused(
                "invalid_authorization_operation_digest",
                "operation object must be a canonical SHA-256 digest",
            ));
        }
        let current = self.leases.get(lease_id).cloned().ok_or_else(|| {
            OwnerAuthorizationBrokerError::refused("authorization_lease_not_found", lease_id)
        })?;
        if current.state != AuthorizationLeaseStateV1::Unused {
            return Err(OwnerAuthorizationBrokerError::refused(
                "authorization_lease_not_unused",
                format!("lease is {:?}", current.state),
            ));
        }
        if now_ms >= current.expires_at
            || current.authorization_receipt.core.transport_session_id != transport_session_id
            || current.authorization_receipt.core.ingress_context_digest != ingress_context_digest
        {
            return Err(OwnerAuthorizationBrokerError::refused(
                "authorization_reservation_binding_mismatch",
                "lease is expired or transport/session binding differs",
            ));
        }
        let reservation_id = digest_canonical(
            OWNER_AUTHORIZATION_RESERVATION_DIGEST_DOMAIN,
            &(
                lease_id,
                current.authorization_receipt.receipt_digest.as_str(),
                transport_session_id,
                ingress_context_digest,
                operation_object_digest,
                transaction_id,
                now_ms,
                current.revision,
            ),
        )?;
        let reservation = AuthorizationReservationV1 {
            reservation_id,
            lease_id: lease_id.to_string(),
            operation_object_digest: operation_object_digest.to_string(),
            transaction_id: transaction_id.map(str::to_string),
            transport_session_id: transport_session_id.to_string(),
            ingress_context_digest: ingress_context_digest.to_string(),
            reserved_at: now_ms,
            reservation_expires_at: now_ms
                .saturating_add(self.config.reservation_ttl_ms)
                .min(current.expires_at),
        };
        let mut next = current;
        next.state = AuthorizationLeaseStateV1::Reserved;
        next.reservation = Some(reservation.clone());
        next.revision += 1;
        self.append_snapshot(BrokerJournalEventKindV1::Reserved, &next, now_ms)?;
        Ok(reservation)
    }

    /// The callback must append and fsync the exact AuthorityWAL COMMIT marker.
    /// It runs under the named owner linearization lock after the final current
    /// state revalidation and before the broker marks the lease CONSUMED.
    pub(crate) fn finalize_wal<F>(
        &mut self,
        reservation: &AuthorizationReservationV1,
        current_authority: &AuthorityRuntimeStatusV1,
        now_ms: u64,
        commit: F,
    ) -> Result<OwnerAuthorizationLeaseV1, OwnerAuthorizationBrokerError>
    where
        F: FnOnce() -> Result<VerifiedAuthorityWalCommitWitnessV1, String>,
    {
        let linearization = self.linearization.clone();
        let _linearization = linearization.lock.lock();
        let mut next = self.validate_for_finalization(reservation, current_authority, now_ms)?;
        let snapshot = finalization_snapshot(current_authority, now_ms);
        next.finalization_snapshot = Some(snapshot);
        next.revision += 1;
        self.append_snapshot(
            BrokerJournalEventKindV1::FinalizationPrepared,
            &next,
            now_ms,
        )?;
        let witness = match commit() {
            Ok(witness) => witness,
            Err(detail) => {
                // Once FINALIZATION_PREPARED is durable, an I/O error from the
                // WAL callback is an uncertain commit result: bytes may have
                // reached stable storage even though acknowledgement failed.
                // Never append ABORT here. Recovery must inspect the exact WAL
                // witness; without one it may abort only after reservation
                // expiry. This prevents ABORT from contradicting a COMMIT that
                // becomes visible after restart.
                return Err(OwnerAuthorizationBrokerError::CommitCallback { detail });
            }
        };
        validate_wal_witness(&next, witness.witness(), now_ms)?;
        let witness = witness.into_witness();
        let mut consumed = next;
        consumed.state = AuthorizationLeaseStateV1::Consumed;
        consumed.terminal = Some(AuthorizationTerminalV1 {
            kind: AuthorizationTerminalKindV1::WalCommitted,
            outcome_digest: witness.terminal_record_digest.clone(),
            wal_witness: Some(witness),
            external_mutation_witness: None,
            terminal_at: now_ms,
        });
        consumed.revision += 1;
        self.append_snapshot(BrokerJournalEventKindV1::Consumed, &consumed, now_ms)?;
        Ok(consumed)
    }

    /// Finalize a typed external mutation without opening the generic elevated
    /// dispatch surface.  This mirrors `finalize_wal`: current authority is
    /// revalidated and FINALIZATION_PREPARED is fsynced before the callback is
    /// allowed to commit the domain transaction.  Once that marker exists, a
    /// callback error is an uncertain result and MUST be recovered from the
    /// exact external-journal witness; this method never appends ABORT.
    pub(crate) fn finalize_external_mutation<F>(
        &mut self,
        reservation: &AuthorizationReservationV1,
        current_authority: &AuthorityRuntimeStatusV1,
        now_ms: u64,
        commit: F,
    ) -> Result<OwnerAuthorizationLeaseV1, OwnerAuthorizationBrokerError>
    where
        F: FnOnce() -> Result<VerifiedExternalMutationCommitWitnessV1, String>,
    {
        let linearization = self.linearization.clone();
        let _linearization = linearization.lock.lock();
        let mut next = self.validate_for_finalization(reservation, current_authority, now_ms)?;
        next.finalization_snapshot = Some(finalization_snapshot(current_authority, now_ms));
        next.revision += 1;
        self.append_snapshot(
            BrokerJournalEventKindV1::FinalizationPrepared,
            &next,
            now_ms,
        )?;
        let witness = commit()
            .map_err(|detail| OwnerAuthorizationBrokerError::ExternalCommitCallback { detail })?;
        validate_external_mutation_witness(&next, witness.witness(), now_ms)?;
        let witness = witness.into_witness();
        let mut consumed = next;
        consumed.state = AuthorizationLeaseStateV1::Consumed;
        consumed.terminal = Some(AuthorizationTerminalV1 {
            kind: AuthorizationTerminalKindV1::ExternalMutationCommitted,
            outcome_digest: witness.journal_record_digest.clone(),
            wal_witness: None,
            external_mutation_witness: Some(witness),
            terminal_at: now_ms,
        });
        consumed.revision += 1;
        self.append_snapshot(BrokerJournalEventKindV1::Consumed, &consumed, now_ms)?;
        Ok(consumed)
    }

    pub(crate) fn finalize_read(
        &mut self,
        reservation: &AuthorizationReservationV1,
        current_authority: &AuthorityRuntimeStatusV1,
        read_result_digest: String,
        now_ms: u64,
    ) -> Result<OwnerAuthorizationLeaseV1, OwnerAuthorizationBrokerError> {
        let linearization = self.linearization.clone();
        let _linearization = linearization.lock.lock();
        if !is_digest(&read_result_digest) {
            return Err(OwnerAuthorizationBrokerError::refused(
                "invalid_read_result_digest",
                "read finalization requires a canonical result digest",
            ));
        }
        let mut next = self.validate_for_finalization(reservation, current_authority, now_ms)?;
        next.finalization_snapshot = Some(finalization_snapshot(current_authority, now_ms));
        next.state = AuthorizationLeaseStateV1::Consumed;
        next.terminal = Some(AuthorizationTerminalV1 {
            kind: AuthorizationTerminalKindV1::ReadCompleted,
            outcome_digest: read_result_digest,
            wal_witness: None,
            external_mutation_witness: None,
            terminal_at: now_ms,
        });
        next.revision += 1;
        self.append_snapshot(BrokerJournalEventKindV1::Consumed, &next, now_ms)?;
        Ok(next)
    }

    /// Consume a non-WAL authorization before dispatch while the caller keeps
    /// its owner coordinator mutex held through the operation. Spending an
    /// authorization that later fails is safe and intentional; it prevents a
    /// failed or crashed call from replaying the same authority.
    pub(crate) fn admit_non_wal(
        &mut self,
        reservation: &AuthorizationReservationV1,
        current_authority: &AuthorityRuntimeStatusV1,
        now_ms: u64,
    ) -> Result<OwnerAuthorizationLeaseV1, OwnerAuthorizationBrokerError> {
        let linearization = self.linearization.clone();
        let _linearization = linearization.lock.lock();
        let mut next = self.validate_for_finalization(reservation, current_authority, now_ms)?;
        next.finalization_snapshot = Some(finalization_snapshot(current_authority, now_ms));
        next.state = AuthorizationLeaseStateV1::Consumed;
        next.terminal = Some(AuthorizationTerminalV1 {
            kind: AuthorizationTerminalKindV1::OperationAdmitted,
            outcome_digest: reservation.operation_object_digest.clone(),
            wal_witness: None,
            external_mutation_witness: None,
            terminal_at: now_ms,
        });
        next.revision += 1;
        self.append_snapshot(BrokerJournalEventKindV1::Consumed, &next, now_ms)?;
        Ok(next)
    }

    pub(crate) fn abort_reserved(
        &mut self,
        reservation: &AuthorizationReservationV1,
        reason_digest: String,
        now_ms: u64,
    ) -> Result<OwnerAuthorizationLeaseV1, OwnerAuthorizationBrokerError> {
        let linearization = self.linearization.clone();
        let _linearization = linearization.lock.lock();
        let current = self.exact_reserved(reservation, now_ms, false)?;
        self.abort_reserved_locked(&current, reason_digest, now_ms)
    }

    /// Recovery never guesses. A FINALIZATION_PREPARED lease becomes CONSUMED
    /// only with an exact committed witness; otherwise only an expired
    /// reservation is aborted. A live unexpired reservation remains RESERVED.
    pub(crate) fn recover_reserved(
        &mut self,
        lease_id: &str,
        committed_witness: Option<VerifiedAuthorityWalCommitWitnessV1>,
        now_ms: u64,
    ) -> Result<OwnerAuthorizationLeaseV1, OwnerAuthorizationBrokerError> {
        let linearization = self.linearization.clone();
        let _linearization = linearization.lock.lock();
        let current = self.leases.get(lease_id).cloned().ok_or_else(|| {
            OwnerAuthorizationBrokerError::refused("authorization_lease_not_found", lease_id)
        })?;
        if current.state != AuthorizationLeaseStateV1::Reserved {
            return Ok(current);
        }
        if let Some(witness) = committed_witness {
            if current.finalization_snapshot.is_none() {
                return Err(OwnerAuthorizationBrokerError::Corruption {
                    detail: "WAL commit exists without broker finalization snapshot".to_string(),
                });
            }
            validate_wal_witness(&current, witness.witness(), witness.witness().committed_at)?;
            let witness = witness.into_witness();
            let mut consumed = current;
            consumed.state = AuthorizationLeaseStateV1::Consumed;
            consumed.terminal = Some(AuthorizationTerminalV1 {
                kind: AuthorizationTerminalKindV1::WalCommitted,
                outcome_digest: witness.terminal_record_digest.clone(),
                wal_witness: Some(witness),
                external_mutation_witness: None,
                terminal_at: now_ms,
            });
            consumed.revision += 1;
            self.append_snapshot(BrokerJournalEventKindV1::Consumed, &consumed, now_ms)?;
            return Ok(consumed);
        }
        let expired = current
            .reservation
            .as_ref()
            .is_none_or(|reservation| now_ms >= reservation.reservation_expires_at);
        if !expired {
            return Ok(current);
        }
        let reason = digest_canonical(
            "m1nd-owner-authorization-recovery-abort-v1",
            &(lease_id, now_ms),
        )?;
        self.abort_reserved_locked(&current, reason, now_ms)
    }

    /// Recover a lease whose external mutation callback crossed its durable
    /// commit point before the broker could acknowledge consumption.  Absence
    /// of a witness never guesses success; after FINALIZATION_PREPARED the lease
    /// remains reserved for explicit recovery rather than becoming reusable.
    pub(crate) fn recover_external_reserved(
        &mut self,
        lease_id: &str,
        committed_witness: Option<VerifiedExternalMutationCommitWitnessV1>,
        now_ms: u64,
    ) -> Result<OwnerAuthorizationLeaseV1, OwnerAuthorizationBrokerError> {
        let linearization = self.linearization.clone();
        let _linearization = linearization.lock.lock();
        let current = self.leases.get(lease_id).cloned().ok_or_else(|| {
            OwnerAuthorizationBrokerError::refused("authorization_lease_not_found", lease_id)
        })?;
        if current.state != AuthorizationLeaseStateV1::Reserved {
            return Ok(current);
        }
        let Some(witness) = committed_witness else {
            // A crash before FINALIZATION_PREPARED created no uncertain commit
            // point. Once its reservation expires it may be terminally aborted.
            // After FINALIZATION_PREPARED, however, absence of the exact journal
            // witness never guesses failure and the lease remains RESERVED.
            let expired = current
                .reservation
                .as_ref()
                .is_none_or(|reservation| now_ms >= reservation.reservation_expires_at);
            if current.finalization_snapshot.is_none() && expired {
                let reason = digest_canonical(
                    "m1nd-owner-external-authorization-recovery-abort-v1",
                    &(lease_id, now_ms),
                )?;
                return self.abort_reserved_locked(&current, reason, now_ms);
            }
            return Ok(current);
        };
        if current.finalization_snapshot.is_none() {
            return Err(OwnerAuthorizationBrokerError::Corruption {
                detail: "external mutation commit exists without broker finalization snapshot"
                    .to_string(),
            });
        }
        validate_external_mutation_witness(&current, witness.witness(), now_ms)?;
        let witness = witness.into_witness();
        let mut consumed = current;
        consumed.state = AuthorizationLeaseStateV1::Consumed;
        consumed.terminal = Some(AuthorizationTerminalV1 {
            kind: AuthorizationTerminalKindV1::ExternalMutationCommitted,
            outcome_digest: witness.journal_record_digest.clone(),
            wal_witness: None,
            external_mutation_witness: Some(witness),
            terminal_at: now_ms,
        });
        consumed.revision += 1;
        self.append_snapshot(BrokerJournalEventKindV1::Consumed, &consumed, now_ms)?;
        Ok(consumed)
    }

    /// Resolve an external reservation, including one that crossed
    /// FINALIZATION_PREPARED, only when the protected external journal provides
    /// an opaque exact PREPARED-without-COMMIT witness. The caller must hold this
    /// broker writer while obtaining that witness, which excludes a live
    /// finalization callback and makes immediate abort safe without a timer.
    pub(crate) fn recover_external_prepared_without_commit(
        &mut self,
        lease_id: &str,
        prepared_witness: VerifiedExternalMutationPreparedAbortWitnessV1,
        now_ms: u64,
    ) -> Result<OwnerAuthorizationLeaseV1, OwnerAuthorizationBrokerError> {
        let linearization = self.linearization.clone();
        let _linearization = linearization.lock.lock();
        let current = self.leases.get(lease_id).cloned().ok_or_else(|| {
            OwnerAuthorizationBrokerError::refused("authorization_lease_not_found", lease_id)
        })?;
        if current.state != AuthorizationLeaseStateV1::Reserved {
            return Ok(current);
        }
        let reservation = current.reservation.as_ref().ok_or_else(|| {
            OwnerAuthorizationBrokerError::Corruption {
                detail: "reserved external lease has no reservation".to_string(),
            }
        })?;
        let witness = prepared_witness.witness();
        if witness.lease_id != current.lease_id
            || witness.reservation_id != reservation.reservation_id
            || witness.operation_object_digest != reservation.operation_object_digest
            || witness.authorization_snapshot_digest != current.authorization_receipt.receipt_digest
            || witness.prepared_at < reservation.reserved_at
            || witness.prepared_at >= current.expires_at
        {
            return Err(OwnerAuthorizationBrokerError::refused(
                "external_mutation_prepared_abort_witness_mismatch",
                "protected PREPARED witness differs from the exact reservation and receipt",
            ));
        }
        let reason = digest_canonical(
            "m1nd-owner-external-prepared-abort-v1",
            &(
                witness.lease_id.as_str(),
                witness.reservation_id.as_str(),
                witness.operation_object_digest.as_str(),
                witness.authorization_snapshot_digest.as_str(),
                witness.prepared_at,
                now_ms,
            ),
        )?;
        self.abort_reserved_locked(&current, reason, now_ms)
    }

    /// Abort the pre-PREPARED crash window only from an opaque protected
    /// journal-absence witness obtained while this broker writer is held.
    pub(crate) fn recover_external_reserved_without_journal(
        &mut self,
        lease_id: &str,
        absence_witness: VerifiedExternalMutationJournalAbsenceWitnessV1,
        now_ms: u64,
    ) -> Result<OwnerAuthorizationLeaseV1, OwnerAuthorizationBrokerError> {
        let linearization = self.linearization.clone();
        let _linearization = linearization.lock.lock();
        let current = self.leases.get(lease_id).cloned().ok_or_else(|| {
            OwnerAuthorizationBrokerError::refused("authorization_lease_not_found", lease_id)
        })?;
        if current.state != AuthorizationLeaseStateV1::Reserved {
            return Ok(current);
        }
        if current.finalization_snapshot.is_some() {
            return Err(OwnerAuthorizationBrokerError::Corruption {
                detail: "external finalization snapshot exists without outer journal PREPARED"
                    .to_string(),
            });
        }
        let reservation = current.reservation.as_ref().ok_or_else(|| {
            OwnerAuthorizationBrokerError::Corruption {
                detail: "reserved external lease has no reservation".to_string(),
            }
        })?;
        let witness = absence_witness.witness();
        if witness.lease_id != current.lease_id
            || witness.reservation_id != reservation.reservation_id
            || witness.operation_object_digest != reservation.operation_object_digest
            || witness.authorization_snapshot_digest != current.authorization_receipt.receipt_digest
        {
            return Err(OwnerAuthorizationBrokerError::refused(
                "external_mutation_journal_absence_witness_mismatch",
                "protected journal absence differs from the exact reservation and receipt",
            ));
        }
        let reason = digest_canonical(
            "m1nd-owner-external-no-journal-abort-v1",
            &(
                witness.lease_id.as_str(),
                witness.reservation_id.as_str(),
                witness.operation_object_digest.as_str(),
                witness.authorization_snapshot_digest.as_str(),
                now_ms,
            ),
        )?;
        self.abort_reserved_locked(&current, reason, now_ms)
    }

    /// GC requires both time retention and an external reference proof. The
    /// caller must return true only when no checkpoint, mission, release, WAL
    /// terminal outcome, or idempotency record references the lease.
    pub(crate) fn gc<F>(
        &mut self,
        now_ms: u64,
        unreferenced: F,
    ) -> Result<Vec<String>, OwnerAuthorizationBrokerError>
    where
        F: Fn(&OwnerAuthorizationLeaseV1) -> bool,
    {
        let linearization = self.linearization.clone();
        let _linearization = linearization.lock.lock();
        let candidates: Vec<String> = self
            .leases
            .values()
            .filter(|lease| {
                matches!(
                    lease.state,
                    AuthorizationLeaseStateV1::Consumed | AuthorizationLeaseStateV1::Aborted
                ) && now_ms >= lease.retain_until
                    && unreferenced(lease)
            })
            .map(|lease| lease.lease_id.clone())
            .collect();
        for lease_id in &candidates {
            self.append_tombstone(lease_id, now_ms)?;
        }
        Ok(candidates)
    }

    fn validate_for_finalization(
        &self,
        reservation: &AuthorizationReservationV1,
        current: &AuthorityRuntimeStatusV1,
        now_ms: u64,
    ) -> Result<OwnerAuthorizationLeaseV1, OwnerAuthorizationBrokerError> {
        // Check authority/receipt expiry before reservation TTL so callers and
        // recovery can distinguish a revoked/expired authority decision from a
        // merely stale reservation. Both paths remain fail closed.
        let lease = self.exact_reserved(reservation, now_ms, false)?;
        let receipt = &lease.authorization_receipt.core;
        if current.state.core.issuance_frozen
            || current.state.core.safety_state != m1nd_control::autonomy::SafetyState::Healthy
            || current.state.core.active_mode != receipt.active_mode
            || current.state.core.constitution_digest != receipt.constitution_digest
            || current.state.core.constitution_epoch != receipt.constitution_epoch
            || current.state.core.autonomy_epoch != receipt.autonomy_epoch
            || current.state.core.policy_registry_digest != receipt.policy_registry_digest
            || current.state.core.protected_epoch != receipt.protected_epoch
            || current.state.core.journal_root_digest != receipt.journal_root_digest
            || now_ms >= receipt.expires_at
        {
            return Err(OwnerAuthorizationBrokerError::refused(
                "authorization_state_changed_before_finalization",
                "freeze/RED, mode, policy, epoch, protected root, or expiry changed",
            ));
        }
        if now_ms >= reservation.reservation_expires_at {
            return Err(OwnerAuthorizationBrokerError::refused(
                "authorization_reservation_not_current",
                "reservation expired before operation finalization",
            ));
        }
        Ok(lease)
    }

    fn exact_reserved(
        &self,
        reservation: &AuthorizationReservationV1,
        now_ms: u64,
        require_unexpired: bool,
    ) -> Result<OwnerAuthorizationLeaseV1, OwnerAuthorizationBrokerError> {
        let lease = self
            .leases
            .get(&reservation.lease_id)
            .cloned()
            .ok_or_else(|| {
                OwnerAuthorizationBrokerError::refused(
                    "authorization_lease_not_found",
                    &reservation.lease_id,
                )
            })?;
        if lease.state != AuthorizationLeaseStateV1::Reserved
            || lease.reservation.as_ref() != Some(reservation)
            || (require_unexpired && now_ms >= reservation.reservation_expires_at)
        {
            return Err(OwnerAuthorizationBrokerError::refused(
                "authorization_reservation_not_current",
                "reservation is stale, replayed, or expired",
            ));
        }
        Ok(lease)
    }

    fn abort_reserved_locked(
        &mut self,
        current: &OwnerAuthorizationLeaseV1,
        reason_digest: String,
        now_ms: u64,
    ) -> Result<OwnerAuthorizationLeaseV1, OwnerAuthorizationBrokerError> {
        if !is_digest(&reason_digest) {
            return Err(OwnerAuthorizationBrokerError::refused(
                "invalid_abort_reason_digest",
                "abort reason must be a SHA-256 digest",
            ));
        }
        let mut aborted = current.clone();
        aborted.state = AuthorizationLeaseStateV1::Aborted;
        aborted.terminal = Some(AuthorizationTerminalV1 {
            kind: AuthorizationTerminalKindV1::Aborted,
            outcome_digest: reason_digest,
            wal_witness: None,
            external_mutation_witness: None,
            terminal_at: now_ms,
        });
        aborted.revision += 1;
        self.append_snapshot(BrokerJournalEventKindV1::Aborted, &aborted, now_ms)?;
        Ok(aborted)
    }

    fn append_snapshot(
        &mut self,
        event_kind: BrokerJournalEventKindV1,
        lease: &OwnerAuthorizationLeaseV1,
        now_ms: u64,
    ) -> Result<(), OwnerAuthorizationBrokerError> {
        validate_lease_shape(lease)?;
        let record = BrokerJournalRecordV1 {
            schema: OWNER_AUTHORIZATION_BROKER_RECORD_SCHEMA.to_string(),
            core: BrokerJournalRecordCoreV1 {
                sequence: self.sequence + 1,
                event_kind,
                lease_id: lease.lease_id.clone(),
                lease: Some(lease.clone()),
                previous_record_digest: self.tail_digest.clone(),
                recorded_at: now_ms,
            },
            record_digest: String::new(),
        };
        self.append_record(record)?;
        self.leases.insert(lease.lease_id.clone(), lease.clone());
        Ok(())
    }

    fn append_tombstone(
        &mut self,
        lease_id: &str,
        now_ms: u64,
    ) -> Result<(), OwnerAuthorizationBrokerError> {
        let record = BrokerJournalRecordV1 {
            schema: OWNER_AUTHORIZATION_BROKER_RECORD_SCHEMA.to_string(),
            core: BrokerJournalRecordCoreV1 {
                sequence: self.sequence + 1,
                event_kind: BrokerJournalEventKindV1::GcTombstone,
                lease_id: lease_id.to_string(),
                lease: None,
                previous_record_digest: self.tail_digest.clone(),
                recorded_at: now_ms,
            },
            record_digest: String::new(),
        };
        self.append_record(record)?;
        self.leases.remove(lease_id);
        Ok(())
    }

    fn append_record(
        &mut self,
        mut record: BrokerJournalRecordV1,
    ) -> Result<(), OwnerAuthorizationBrokerError> {
        if self.poisoned {
            return Err(OwnerAuthorizationBrokerError::Poisoned);
        }
        if let (Some(backend), Some(expected)) =
            (&self.protected_head_backend, &self.protected_head)
        {
            let observed = verify_or_initialize_protected_head(
                backend,
                OWNER_AUTHORIZATION_BROKER_HEAD_DOMAIN,
                self.sequence,
                self.tail_digest.clone(),
            )
            .map_err(|detail| OwnerAuthorizationBrokerError::ProtectedHead { detail })?;
            if &observed != expected {
                self.poisoned = true;
                return Err(OwnerAuthorizationBrokerError::ProtectedHead {
                    detail: "protected broker head changed outside owner serial".to_string(),
                });
            }
        }
        let observed_len = self
            .file
            .metadata()
            .map_err(|source| OwnerAuthorizationBrokerError::Io {
                operation: "broker_length_before_append",
                source,
            })?
            .len();
        if observed_len != self.known_len {
            self.poisoned = true;
            return Err(OwnerAuthorizationBrokerError::Corruption {
                detail: "broker journal length changed outside owner serial".to_string(),
            });
        }
        record.seal()?;
        record.validate(self.sequence + 1, self.tail_digest.as_deref())?;
        let mut bytes = canonical_json_string(&record)?.into_bytes();
        bytes.push(b'\n');
        if let Err(source) = self
            .file
            .write_all(&bytes)
            .and_then(|()| self.file.sync_all())
        {
            self.poisoned = true;
            return Err(OwnerAuthorizationBrokerError::Io {
                operation: "append_sync_broker_record",
                source,
            });
        }
        let next_sequence = self.sequence + 1;
        let next_digest = record.record_digest;
        if let (Some(backend), Some(expected)) =
            (&self.protected_head_backend, &self.protected_head)
        {
            match advance_protected_head(
                backend,
                OWNER_AUTHORIZATION_BROKER_HEAD_DOMAIN,
                expected,
                next_sequence,
                next_digest.clone(),
            ) {
                Ok(next) => self.protected_head = Some(next),
                Err(detail) => {
                    self.poisoned = true;
                    return Err(OwnerAuthorizationBrokerError::ProtectedHead { detail });
                }
            }
        }
        self.known_len += bytes.len() as u64;
        self.sequence = next_sequence;
        self.tail_digest = Some(next_digest);
        Ok(())
    }
}

fn finalization_snapshot(
    current: &AuthorityRuntimeStatusV1,
    now_ms: u64,
) -> AuthorizationFinalizationSnapshotV1 {
    AuthorizationFinalizationSnapshotV1 {
        active_mode: current.state.core.active_mode,
        constitution_epoch: current.state.core.constitution_epoch,
        autonomy_epoch: current.state.core.autonomy_epoch,
        protected_epoch: current.state.core.protected_epoch,
        journal_root_digest: current.state.core.journal_root_digest.clone(),
        revalidated_at: now_ms,
    }
}

fn validate_wal_witness(
    lease: &OwnerAuthorizationLeaseV1,
    witness: &AuthorityWalCommitWitnessV1,
    now_ms: u64,
) -> Result<(), OwnerAuthorizationBrokerError> {
    let reservation =
        lease
            .reservation
            .as_ref()
            .ok_or_else(|| OwnerAuthorizationBrokerError::Corruption {
                detail: "WAL finalization lease has no reservation".to_string(),
            })?;
    if witness.phase != AuthorityWalPhase::Commit
        || reservation.transaction_id.as_deref() != Some(witness.transaction_id.as_str())
        || witness.transaction_digest != reservation.operation_object_digest
        || witness.authorization_snapshot_digest != lease.authorization_receipt.receipt_digest
        || !is_digest(&witness.terminal_record_digest)
        || witness.committed_at > now_ms
        || witness.committed_at < lease.issued_at
        || witness.committed_at >= lease.expires_at
    {
        return Err(OwnerAuthorizationBrokerError::refused(
            "authority_wal_witness_binding_mismatch",
            "commit witness does not bind the reserved receipt/object/time",
        ));
    }
    Ok(())
}

fn validate_external_mutation_witness(
    lease: &OwnerAuthorizationLeaseV1,
    witness: &ExternalMutationCommitWitnessV1,
    now_ms: u64,
) -> Result<(), OwnerAuthorizationBrokerError> {
    let reservation =
        lease
            .reservation
            .as_ref()
            .ok_or_else(|| OwnerAuthorizationBrokerError::Corruption {
                detail: "external mutation finalization lease has no reservation".to_string(),
            })?;
    if witness.reservation_id != reservation.reservation_id
        || witness.lease_id != lease.lease_id
        || witness.operation_object_digest != reservation.operation_object_digest
        || witness.authorization_snapshot_digest != lease.authorization_receipt.receipt_digest
        || !is_digest(&witness.journal_record_digest)
        || witness.committed_at > now_ms
        || witness.committed_at < lease.issued_at
        || witness.committed_at >= lease.expires_at
    {
        return Err(OwnerAuthorizationBrokerError::refused(
            "external_mutation_witness_binding_mismatch",
            "commit witness does not bind the exact reservation, receipt, object, and time",
        ));
    }
    Ok(())
}

fn validate_land_transaction_binding(
    receipt: &AuthorityAuthorizationReceiptV1,
    transaction: &AuthorityTransactionV1,
    now_ms: u64,
) -> Result<(), OwnerAuthorizationBrokerError> {
    let positive = match transaction {
        AuthorityTransactionV1::PositiveAuthority(positive) => positive,
        AuthorityTransactionV1::SafetyKernel(_) => {
            return Err(OwnerAuthorizationBrokerError::refused(
                "land_requires_positive_authority",
                "landing cannot reserve a SAFETY_KERNEL transaction",
            ))
        }
    };
    let binding = &positive.binding;
    let core = &receipt.core;
    let receipt_variant = match &core.authority {
        AuthorizationAuthorityV1::Positive { variant, .. } => *variant,
        _ => {
            return Err(OwnerAuthorizationBrokerError::refused(
                "land_requires_positive_authority",
                "landing lease was not issued from positive sovereign authority",
            ))
        }
    };
    if core.action.as_str() != "mission.service.land"
        || core.verified_object_digest != positive.action_payload_digest
        || binding.organism_id != core.organism_id
        || binding.brain_id != core.brain_id
        || binding.subject_id != core.subject_id
        || binding.action_id != "land"
        || binding.capability_id != core.capability_id
        || Some(binding.capability_kind) != core.capability_kind
        || binding.expected_head_id != core.mission_head_id
        || binding.expected_active_mode != core.active_mode
        || binding.expected_constitution_epoch != core.constitution_epoch
        || binding.expected_autonomy_epoch != core.autonomy_epoch
        || core.authority_decision_digest.as_deref()
            != Some(positive.authority_decision_digest.as_str())
        || core.policy_registry_digest != positive.action_policy_registry_digest
        || receipt_variant != positive.required_authority_variant
        || binding.authorization_snapshot_digest != receipt.receipt_digest
        || binding.issued_at < core.authorized_at
        || binding.expires_at > core.expires_at
        || binding.issued_at > now_ms
        || now_ms >= binding.expires_at
    {
        return Err(OwnerAuthorizationBrokerError::refused(
            "land_authorization_receipt_binding_mismatch",
            "transaction does not bind the exact positive receipt, LandIntent object, policy, identity, epochs, and time window",
        ));
    }
    Ok(())
}

fn validate_lease_shape(
    lease: &OwnerAuthorizationLeaseV1,
) -> Result<(), OwnerAuthorizationBrokerError> {
    if lease.schema != OWNER_AUTHORIZATION_LEASE_SCHEMA
        || lease.lease_id.trim().is_empty()
        || lease.issued_at >= lease.expires_at
        || lease.revision == 0
        || !is_digest(&lease.authorization_receipt.receipt_digest)
    {
        return Err(OwnerAuthorizationBrokerError::Corruption {
            detail: "invalid authorization lease shape".to_string(),
        });
    }
    if let Some(reservation) = &lease.reservation {
        if reservation.lease_id != lease.lease_id
            || reservation.transport_session_id.trim().is_empty()
            || !is_digest(&reservation.reservation_id)
            || !is_digest(&reservation.operation_object_digest)
            || reservation
                .transaction_id
                .as_deref()
                .is_some_and(|transaction_id| transaction_id.trim().is_empty())
            || !is_digest(&reservation.ingress_context_digest)
            || reservation.reserved_at >= reservation.reservation_expires_at
            || reservation.reservation_expires_at > lease.expires_at
        {
            return Err(OwnerAuthorizationBrokerError::Corruption {
                detail: "invalid authorization reservation shape".to_string(),
            });
        }
    }
    let state_shape = match lease.state {
        AuthorizationLeaseStateV1::Unused => {
            lease.reservation.is_none()
                && lease.finalization_snapshot.is_none()
                && lease.terminal.is_none()
        }
        AuthorizationLeaseStateV1::Reserved => {
            lease.reservation.is_some() && lease.terminal.is_none()
        }
        AuthorizationLeaseStateV1::Consumed | AuthorizationLeaseStateV1::Aborted => {
            lease.reservation.is_some() && lease.terminal.is_some()
        }
    };
    if !state_shape {
        return Err(OwnerAuthorizationBrokerError::Corruption {
            detail: "lease state and reservation/finalization fields diverge".to_string(),
        });
    }
    if let Some(terminal) = lease.terminal.as_ref() {
        let witness_shape = match terminal.kind {
            AuthorizationTerminalKindV1::WalCommitted => {
                terminal.wal_witness.is_some() && terminal.external_mutation_witness.is_none()
            }
            AuthorizationTerminalKindV1::ExternalMutationCommitted => {
                terminal.wal_witness.is_none() && terminal.external_mutation_witness.is_some()
            }
            AuthorizationTerminalKindV1::OperationAdmitted
            | AuthorizationTerminalKindV1::ReadCompleted
            | AuthorizationTerminalKindV1::Aborted => {
                terminal.wal_witness.is_none() && terminal.external_mutation_witness.is_none()
            }
        };
        if !witness_shape || !is_digest(&terminal.outcome_digest) {
            return Err(OwnerAuthorizationBrokerError::Corruption {
                detail: "lease terminal kind and witness diverge".to_string(),
            });
        }
    }
    Ok(())
}

fn apply_replayed_record(
    leases: &mut BTreeMap<String, OwnerAuthorizationLeaseV1>,
    record: &BrokerJournalRecordV1,
) -> Result<(), OwnerAuthorizationBrokerError> {
    if record.core.event_kind == BrokerJournalEventKindV1::GcTombstone {
        if leases.remove(&record.core.lease_id).is_none() {
            return Err(OwnerAuthorizationBrokerError::Corruption {
                detail: "GC tombstone references an unknown lease".to_string(),
            });
        }
        return Ok(());
    }
    let lease = record
        .core
        .lease
        .as_ref()
        .expect("record validation proved lease");
    validate_lease_shape(lease)?;
    if lease.lease_id != record.core.lease_id {
        return Err(OwnerAuthorizationBrokerError::Corruption {
            detail: "record/lease id mismatch".to_string(),
        });
    }
    match leases.get(&lease.lease_id) {
        None if record.core.event_kind == BrokerJournalEventKindV1::Issued
            && lease.state == AuthorizationLeaseStateV1::Unused => {}
        Some(previous) if lease.revision == previous.revision + 1 => {
            let legal = matches!(
                (previous.state, lease.state, record.core.event_kind),
                (
                    AuthorizationLeaseStateV1::Unused,
                    AuthorizationLeaseStateV1::Reserved,
                    BrokerJournalEventKindV1::Reserved
                ) | (
                    AuthorizationLeaseStateV1::Reserved,
                    AuthorizationLeaseStateV1::Reserved,
                    BrokerJournalEventKindV1::FinalizationPrepared
                ) | (
                    AuthorizationLeaseStateV1::Reserved,
                    AuthorizationLeaseStateV1::Consumed,
                    BrokerJournalEventKindV1::Consumed
                ) | (
                    AuthorizationLeaseStateV1::Reserved,
                    AuthorizationLeaseStateV1::Aborted,
                    BrokerJournalEventKindV1::Aborted
                )
            );
            if !legal {
                return Err(OwnerAuthorizationBrokerError::Corruption {
                    detail: "illegal durable lease transition".to_string(),
                });
            }
        }
        _ => {
            return Err(OwnerAuthorizationBrokerError::Corruption {
                detail: "lease revision/event replay mismatch".to_string(),
            });
        }
    }
    leases.insert(lease.lease_id.clone(), lease.clone());
    Ok(())
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn refuse_symlink(path: &Path) -> Result<(), OwnerAuthorizationBrokerError> {
    if path
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(OwnerAuthorizationBrokerError::refused(
            "broker_symlink_refused",
            path.display().to_string(),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> std::io::Result<()> {
    File::open(path.parent().unwrap_or_else(|| Path::new(".")))?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use m1nd_control::{
        ActionId, AuthorityTransactionBindingV1, AuthorityVariant, CapabilityKind, Effect, Ingress,
        OpaqueSignature, PositiveAuthorityTransactionV1, ReachablePolicyTupleV1, RiskClass, Role,
        CANONICALIZATION_VERSION, POSITIVE_AUTHORITY_TRANSACTION_SCHEMA,
    };
    use tempfile::TempDir;

    use super::*;
    use crate::authority_runtime::{
        AuthorityAuthorizationReceiptCoreV1, AuthorityRuntimeStateCoreV1, AuthorityRuntimeStateV1,
        AuthorityVerificationAssurance, ProtectedEpochAssurance,
    };

    const NOW: u64 = 10_000;

    fn hash(label: &str) -> String {
        digest_canonical("broker-test-v1", &label).unwrap()
    }

    fn receipt(object: &str) -> AuthorityAuthorizationReceiptV1 {
        AuthorityAuthorizationReceiptV1::new_for_broker_test(AuthorityAuthorizationReceiptCoreV1 {
            organism_id: "organism-1".into(),
            repo_id: "repo-1".into(),
            brain_id: "brain-1".into(),
            subject_id: "human-1".into(),
            role: Role::Author,
            capability_id: "capability-1".into(),
            capability_kind: Some(CapabilityKind::Human),
            verified_object_digest: object.into(),
            mission_id: Some("mission-1".into()),
            mission_head_id: Some("head-1".into()),
            transport_session_id: "transport-1".into(),
            ingress_context_digest: hash("ingress"),
            action: ActionId::new("mission.service.land").unwrap(),
            ingress: Ingress::Rest,
            complete_effects: BTreeSet::from([
                Effect::MissionStateWrite,
                Effect::RuntimeStoreWrite,
                Effect::CoordinationRecord,
                Effect::SovereignMutation,
            ]),
            active_mode: ActiveMode::HumanGated,
            constitution_digest: hash("constitution"),
            constitution_epoch: 7,
            autonomy_epoch: 0,
            protected_epoch_at_decision: 11,
            policy_registry_digest: hash("policy"),
            exact_policy_tuple: ReachablePolicyTupleV1 {
                ingress: Ingress::Rest,
                action: ActionId::new("mission.service.land").unwrap(),
                active_mode: ActiveMode::HumanGated,
                subject_id: "human-1".into(),
                authority_variant: AuthorityVariant::Human,
                applicable_grant_id: None,
                applicable_tier: None,
                risk_class: RiskClass::Critical,
            },
            authority_decision_digest: Some(hash("decision")),
            autonomy_admission_receipt_digest: None,
            autonomy_committed_state_digest: None,
            autonomy_protected_root_digest: None,
            authority: AuthorizationAuthorityV1::Positive {
                variant: AuthorityVariant::Human,
                assurance: AuthorityVerificationAssurance::SoftwareTestOnlyNotProven,
            },
            authority_body_digest: hash("body"),
            replay_sequence: 3,
            journal_sequence: 11,
            journal_root_digest: hash("journal"),
            protected_epoch: 11,
            authorized_at: NOW,
            expires_at: NOW + 1_000,
        })
    }

    fn status(receipt: &AuthorityAuthorizationReceiptV1) -> AuthorityRuntimeStatusV1 {
        let core = &receipt.core;
        AuthorityRuntimeStatusV1 {
            state: AuthorityRuntimeStateV1::new_for_broker_test(AuthorityRuntimeStateCoreV1 {
                organism_id: core.organism_id.clone(),
                repo_id: core.repo_id.clone(),
                brain_id: core.brain_id.clone(),
                audience: "m1nd-runtime".into(),
                revision: 10,
                active_mode: core.active_mode,
                activation_receipt_id: None,
                constitution_digest: core.constitution_digest.clone(),
                constitution_epoch: core.constitution_epoch,
                autonomy_epoch: core.autonomy_epoch,
                grants_digest: hash("grants"),
                policy_registry_digest: core.policy_registry_digest.clone(),
                action_catalog_digest: hash("catalog"),
                safety_kernel_digest: hash("kernel"),
                safety_actuator_identity_key_binary_policy_digest: hash("actuator"),
                issuance_frozen: false,
                safety_state: m1nd_control::autonomy::SafetyState::Healthy,
                protected_epoch: core.protected_epoch,
                journal_sequence: core.journal_sequence,
                journal_root_digest: core.journal_root_digest.clone(),
                replay_sequence: core.replay_sequence,
                replay_root_digest: Some(hash("replay")),
                updated_at: core.authorized_at,
            }),
            protected_epoch_assurance: ProtectedEpochAssurance::SoftwareTestOnlyNotProven,
            positive_verification_assurance:
                AuthorityVerificationAssurance::SoftwareTestOnlyNotProven,
            semantic_catalog_entries: 1,
            transport_schema_parity_proven: false,
            multi_artifact_atomicity_proven: false,
            automatic_crash_recovery_proven: true,
        }
    }

    fn land_transaction(receipt: &AuthorityAuthorizationReceiptV1) -> AuthorityTransactionV1 {
        let intent_digest = receipt.core.verified_object_digest.clone();
        let mut transaction =
            AuthorityTransactionV1::PositiveAuthority(PositiveAuthorityTransactionV1 {
                schema: POSITIVE_AUTHORITY_TRANSACTION_SCHEMA.to_string(),
                binding: AuthorityTransactionBindingV1 {
                    transaction_id: "land-transaction-1".to_string(),
                    organism_id: receipt.core.organism_id.clone(),
                    brain_id: receipt.core.brain_id.clone(),
                    subject_id: receipt.core.subject_id.clone(),
                    action_id: "land".to_string(),
                    idempotency_key: "land-idempotency-1".to_string(), // gitleaks:allow
                    intent_core_ref: format!("intent:{intent_digest}"),
                    intent_digest: intent_digest.clone(),
                    intent_canonicalization_version: CANONICALIZATION_VERSION.to_string(),
                    capability_id: receipt.core.capability_id.clone(),
                    capability_kind: receipt.core.capability_kind.unwrap(),
                    nonce: "land-nonce-1".to_string(),
                    expected_head_id: receipt.core.mission_head_id.clone(),
                    expected_active_mode: receipt.core.active_mode,
                    expected_activation_receipt_id: None,
                    expected_constitution_epoch: receipt.core.constitution_epoch,
                    expected_autonomy_epoch: receipt.core.autonomy_epoch,
                    expected_store_epoch: 1,
                    sentinel_verdict_digest: None,
                    authorization_snapshot_digest: receipt.receipt_digest.clone(),
                    issued_at: NOW,
                    expires_at: NOW + 900,
                },
                authority_decision_digest: receipt.core.authority_decision_digest.clone().unwrap(),
                identity_role_binding_digest: hash("identity-role"),
                required_authority_variant: AuthorityVariant::Human,
                action_policy_registry_digest: receipt.core.policy_registry_digest.clone(),
                classifier_decision_digest: hash("classifier"),
                expected_pending_red_set_digest: hash("pending-red"),
                expected_red_latch_epoch: 0,
                expected_store_version: 1,
                expected_boundary_version: 1,
                expected_contract_version: 1,
                action_payload_digest: intent_digest,
                issuer: "owner-1".to_string(),
                key_id: "owner-key-1".to_string(),
                algorithm: "software-test".to_string(),
                transaction_digest: String::new(),
                signature: OpaqueSignature::new("software-test-transaction-signature"),
            });
        transaction.seal().unwrap();
        transaction
    }

    fn open_broker(temp: &TempDir) -> OwnerAuthorizationBrokerV1 {
        OwnerAuthorizationBrokerV1::open(
            OwnerAuthorizationBrokerConfigV1 {
                root: temp.path().to_path_buf(),
                reservation_ttl_ms: 2_000,
                minimum_terminal_retention_ms: 500,
            },
            OwnerAuthorityLinearizationV1::default(),
        )
        .unwrap()
    }

    #[test]
    fn protected_head_refuses_replacement_with_valid_older_broker_prefix() {
        let temp = TempDir::new().unwrap();
        let config = OwnerAuthorizationBrokerConfigV1 {
            root: temp.path().to_path_buf(),
            reservation_ttl_ms: 2_000,
            minimum_terminal_retention_ms: 500,
        };
        let protected =
            crate::protected_journal_head::SoftwareTestProtectedJournalHeadBackendV1::new();
        let shared = protected.clone().shared();
        let mut broker = OwnerAuthorizationBrokerV1::open_with_protected_head(
            config.clone(),
            OwnerAuthorityLinearizationV1::default(),
            Arc::clone(&shared),
        )
        .unwrap();
        broker
            .issue("protected-lease", receipt(&hash("protected-object")), NOW)
            .unwrap();
        assert_eq!(
            protected
                .snapshot(crate::protected_journal_head::OWNER_AUTHORIZATION_BROKER_HEAD_DOMAIN,)
                .unwrap()
                .record_sequence,
            1
        );
        drop(broker);

        std::fs::write(temp.path().join(BROKER_JOURNAL_FILE), []).unwrap();
        let reopened = OwnerAuthorizationBrokerV1::open_with_protected_head(
            config,
            OwnerAuthorityLinearizationV1::default(),
            shared,
        );
        assert!(matches!(
            reopened,
            Err(OwnerAuthorizationBrokerError::ProtectedHead { .. })
        ));
    }

    #[test]
    fn one_shot_wal_flow_revalidates_and_survives_restart() {
        let temp = TempDir::new().unwrap();
        let object = hash("transaction");
        let receipt = receipt(&object);
        let transaction = land_transaction(&receipt);
        let mut broker = open_broker(&temp);
        broker.issue("lease-1", receipt.clone(), NOW).unwrap();
        let reservation = broker
            .reserve_land(
                "lease-1",
                "transport-1",
                &hash("ingress"),
                &transaction,
                NOW + 1,
            )
            .unwrap();
        let witness = AuthorityWalCommitWitnessV1 {
            transaction_id: transaction.binding().transaction_id.clone(),
            phase: AuthorityWalPhase::Commit,
            transaction_digest: transaction.transaction_digest().to_string(),
            authorization_snapshot_digest: receipt.receipt_digest.clone(),
            terminal_record_digest: hash("terminal"),
            committed_at: NOW + 2,
        };
        let consumed = broker
            .finalize_wal(&reservation, &status(&receipt), NOW + 2, || {
                Ok(VerifiedAuthorityWalCommitWitnessV1::explicit_test_only(
                    witness.clone(),
                ))
            })
            .unwrap();
        assert_eq!(consumed.state, AuthorizationLeaseStateV1::Consumed);
        assert!(matches!(
            broker.reserve(
                "lease-1",
                "transport-1",
                &hash("ingress"),
                &receipt.core.verified_object_digest,
                NOW + 3,
            ),
            Err(OwnerAuthorizationBrokerError::Refused {
                code: "authorization_lease_not_unused",
                ..
            })
        ));
        drop(broker);
        let reopened = open_broker(&temp);
        assert_eq!(
            reopened.lease("lease-1").unwrap().state,
            AuthorizationLeaseStateV1::Consumed
        );
    }

    #[test]
    fn land_reservation_binds_intent_receipt_snapshot_and_final_transaction() {
        let temp = TempDir::new().unwrap();
        let land_receipt = receipt(&hash("canonical-land-intent"));
        let transaction = land_transaction(&land_receipt);
        let mut broker = open_broker(&temp);
        broker
            .issue("land-lease", land_receipt.clone(), NOW)
            .unwrap();
        let reservation = broker
            .reserve_land(
                "land-lease",
                "transport-1",
                &hash("ingress"),
                &transaction,
                NOW + 1,
            )
            .unwrap();
        assert_eq!(
            reservation.operation_object_digest,
            transaction.transaction_digest()
        );
        let identity = match &transaction {
            AuthorityTransactionV1::PositiveAuthority(positive) => {
                positive.identity_role_binding_digest.clone()
            }
            AuthorityTransactionV1::SafetyKernel(_) => unreachable!(),
        };
        let context = broker
            .mission_service_context(&reservation, Some(identity))
            .unwrap();
        assert_eq!(
            context.authorization_snapshot_digest,
            land_receipt.receipt_digest
        );
        assert_eq!(
            context.verified_object_digest,
            transaction.transaction_digest()
        );
        assert_eq!(context.action_id, "mission.service.land");

        let second_receipt = receipt(&hash("second-land-intent"));
        let mut wrong = land_transaction(&second_receipt);
        if let AuthorityTransactionV1::PositiveAuthority(positive) = &mut wrong {
            positive.binding.authorization_snapshot_digest = hash("wrong-snapshot");
        }
        wrong.seal().unwrap();
        broker
            .issue("wrong-land-lease", second_receipt, NOW)
            .unwrap();
        assert_eq!(
            broker
                .reserve_land(
                    "wrong-land-lease",
                    "transport-1",
                    &hash("ingress"),
                    &wrong,
                    NOW + 1,
                )
                .unwrap_err()
                .code(),
            "land_authorization_receipt_binding_mismatch"
        );
    }

    #[test]
    fn freeze_red_epoch_expiry_and_wrong_witness_fail_closed() {
        let cases = ["frozen", "red", "epoch", "expired"];
        for case in cases {
            let temp = TempDir::new().unwrap();
            let object = hash(case);
            let receipt = receipt(&object);
            let mut broker = open_broker(&temp);
            broker.issue("lease", receipt.clone(), NOW).unwrap();
            let reservation = broker
                .reserve("lease", "transport-1", &hash("ingress"), &object, NOW + 1)
                .unwrap();
            let mut current = status(&receipt);
            let finalize_at = if case == "expired" {
                NOW + 1_001
            } else {
                NOW + 2
            };
            match case {
                "frozen" => current.state.core.issuance_frozen = true,
                "red" => {
                    current.state.core.safety_state =
                        m1nd_control::autonomy::SafetyState::PendingRed
                }
                "epoch" => current.state.core.autonomy_epoch += 1,
                "expired" => {}
                _ => unreachable!(),
            }
            let called = std::cell::Cell::new(false);
            let error = broker
                .finalize_wal(&reservation, &current, finalize_at, || {
                    called.set(true);
                    unreachable!()
                })
                .unwrap_err();
            assert_eq!(
                error.code(),
                "authorization_state_changed_before_finalization"
            );
            assert!(!called.get());
        }

        let temp = TempDir::new().unwrap();
        let receipt = receipt(&hash("right"));
        let transaction = land_transaction(&receipt);
        let mut broker = open_broker(&temp);
        broker.issue("lease", receipt.clone(), NOW).unwrap();
        let reservation = broker
            .reserve_land(
                "lease",
                "transport-1",
                &hash("ingress"),
                &transaction,
                NOW + 1,
            )
            .unwrap();
        let wrong = AuthorityWalCommitWitnessV1 {
            transaction_id: "tx-wrong".into(),
            phase: AuthorityWalPhase::Commit,
            transaction_digest: hash("wrong"),
            authorization_snapshot_digest: receipt.receipt_digest.clone(),
            terminal_record_digest: hash("terminal"),
            committed_at: NOW + 2,
        };
        assert_eq!(
            broker
                .finalize_wal(&reservation, &status(&receipt), NOW + 2, || {
                    Ok(VerifiedAuthorityWalCommitWitnessV1::explicit_test_only(
                        wrong,
                    ))
                })
                .unwrap_err()
                .code(),
            "authority_wal_witness_binding_mismatch"
        );
    }

    #[test]
    fn uncertain_wal_callback_stays_prepared_until_exact_recovery() {
        let temp = TempDir::new().unwrap();
        let receipt = receipt(&hash("uncertain-commit"));
        let transaction = land_transaction(&receipt);
        let mut broker = open_broker(&temp);
        broker
            .issue("lease-uncertain", receipt.clone(), NOW)
            .unwrap();
        let reservation = broker
            .reserve_land(
                "lease-uncertain",
                "transport-1",
                &hash("ingress"),
                &transaction,
                NOW + 1,
            )
            .unwrap();
        let error = broker
            .finalize_wal(&reservation, &status(&receipt), NOW + 2, || {
                Err("WAL fsync acknowledgement is uncertain".to_string())
            })
            .unwrap_err();
        assert_eq!(error.code(), "authority_wal_commit_failed");
        let prepared = broker.lease("lease-uncertain").unwrap();
        assert_eq!(prepared.state, AuthorizationLeaseStateV1::Reserved);
        assert!(prepared.finalization_snapshot.is_some());
        assert!(prepared.terminal.is_none());
        drop(broker);

        let mut recovered = open_broker(&temp);
        assert_eq!(
            recovered
                .recover_reserved("lease-uncertain", None, NOW + 3)
                .unwrap()
                .state,
            AuthorizationLeaseStateV1::Reserved
        );
        let witness = AuthorityWalCommitWitnessV1 {
            transaction_id: transaction.binding().transaction_id.clone(),
            phase: AuthorityWalPhase::Commit,
            transaction_digest: transaction.transaction_digest().to_string(),
            authorization_snapshot_digest: receipt.receipt_digest,
            terminal_record_digest: hash("uncertain-terminal"),
            committed_at: NOW + 2,
        };
        assert_eq!(
            recovered
                .recover_reserved(
                    "lease-uncertain",
                    Some(VerifiedAuthorityWalCommitWitnessV1::explicit_test_only(
                        witness
                    )),
                    NOW + 4,
                )
                .unwrap()
                .state,
            AuthorizationLeaseStateV1::Consumed
        );
    }

    #[test]
    fn recovery_and_gc_are_conservative() {
        let temp = TempDir::new().unwrap();
        let receipt = receipt(&hash("read"));
        let mut broker = open_broker(&temp);
        broker.issue("lease-read", receipt.clone(), NOW).unwrap();
        let reservation = broker
            .reserve(
                "lease-read",
                "transport-1",
                &hash("ingress"),
                &receipt.core.verified_object_digest,
                NOW + 1,
            )
            .unwrap();
        assert_eq!(
            broker
                .recover_reserved("lease-read", None, NOW + 2)
                .unwrap()
                .state,
            AuthorizationLeaseStateV1::Reserved
        );
        assert_eq!(
            broker
                .recover_reserved("lease-read", None, reservation.reservation_expires_at)
                .unwrap()
                .state,
            AuthorizationLeaseStateV1::Aborted
        );
        assert!(broker.gc(NOW + 10_000, |_| false).unwrap().is_empty());
        assert_eq!(
            broker.gc(NOW + 10_000, |_| true).unwrap(),
            vec!["lease-read".to_string()]
        );
        drop(broker);
        assert!(open_broker(&temp).lease("lease-read").is_none());
    }
}
