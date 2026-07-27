//! Per-brain actor boundary for durable background work.
//!
//! A worker receives only a caller-selected immutable read snapshot. It returns
//! a proposal; the proposal can reach [`SessionState`] only on the brain actor,
//! after an epoch/generation/revision OCC check. The actor persists the session
//! and publishes a content-addressed checkpoint before reporting success.
//!
//! This module deliberately does not depend on `AuthorityRuntime` or
//! `MissionService`. Their checkpoint roots enter through the narrow
//! [`BrainCheckpointAuthority`] adapter. The default adapter records an explicit
//! *unbound* authority scope; it is useful for graph/session durability, but is
//! not evidence of AuthorityWAL or autonomy anti-rollback protection.
//!
//! External crates cannot construct the raw cell or acquire either compatibility
//! guard. Protocol transports must enter through the crate-owned actor router.
//!
//! ```compile_fail
//! use m1nd_mcp::brain_runtime::BrainSessionCell;
//! # fn probe(cell: &BrainSessionCell) {
//! let _raw_guard = cell.lock();
//! # }
//! ```
//!
//! ```compile_fail
//! use m1nd_mcp::brain_runtime::BrainSessionCell;
//! # fn probe(cell: &BrainSessionCell) {
//! let _raw_guard = cell.try_lock();
//! # }
//! ```
//!
//! ```compile_fail
//! use m1nd_mcp::brain_runtime::BrainSessionGuard;
//! ```
//!
//! ```compile_fail
//! use m1nd_mcp::brain_runtime::BrainActorHandle;
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use parking_lot::{Condvar, Mutex as ParkingMutex, MutexGuard as ParkingMutexGuard};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::checkpoint_store::{
    preview_checkpoint_manifest, CheckpointAckV1, CheckpointAuthorityValidationReceiptV1,
    CheckpointAuthorityValidator, CheckpointCreateV1, CheckpointError,
    CheckpointExternalAuthorityRefsV1, CheckpointFallbackReceiptV1, CheckpointFaultInjector,
    CheckpointFileInputV1, CheckpointLoadDisposition, CheckpointManifestV1, CheckpointStore,
    LoadedCheckpointV1, NoCheckpointFaults, GRAPH_SNAPSHOT_LOGICAL_NAME, INGEST_ROOTS_LOGICAL_NAME,
};
use crate::runtime_jobs::{RuntimeJobContext, RuntimeJobFailure, RuntimeJobSuccess};
use crate::session::{
    CheckpointCandidatePresence, CheckpointPersistenceStage, SessionCheckpointCandidate,
    SessionState,
};

/// Shared compatibility cell for a brain session. Short legacy readers retain
/// the familiar `lock`/`try_lock` API, while the brain actor can *check out* the
/// whole SessionState and release the mutex before filesystem/network/analysis
/// work. The Condvar makes a legacy reader wait for actor ownership to finish;
/// no mutex guard spans the actor operation.
pub struct BrainSessionCell {
    state: ParkingMutex<Option<SessionState>>,
    /// Set before actor startup checks out the session and cleared only after
    /// the actor thread has joined. Future mutable compatibility access must
    /// refuse while this fence is active; it is the process-local half of the
    /// single-writer invariant.
    actor_active: AtomicBool,
    /// A checkpoint failure after a successful in-memory mutation makes the
    /// checked-out state non-authoritative.  Keeping that value readable while
    /// `CURRENT` still names the previous checkpoint would publish an unacked
    /// postimage.  Quarantine is sticky for the lifetime of this cell; restart
    /// until the actor's autonomous reconciler selects and validates either the
    /// candidate or its predecessor.
    quarantine_reason: ParkingMutex<Option<String>>,
    /// Retain the quarantined value without exposing it.  Dropping SessionState
    /// here would also drop its writer/instance lease while this failed owner is
    /// still alive, allowing another writer to race the required CURRENT-based
    /// recovery.
    quarantined_state: ParkingMutex<Option<SessionState>>,
    available: Condvar,
}

impl BrainSessionCell {
    pub(crate) fn new(state: SessionState) -> Self {
        Self {
            state: ParkingMutex::new(Some(state)),
            actor_active: AtomicBool::new(false),
            quarantine_reason: ParkingMutex::new(None),
            quarantined_state: ParkingMutex::new(None),
            available: Condvar::new(),
        }
    }

    fn claim_actor(self: &Arc<Self>) -> Result<BrainActorActivation, BrainRuntimeError> {
        // Drain every pre-actor guard before raising the ownership fence. The
        // old CAS-first order allowed a caller that already held `lock()` to
        // keep mutating interior capabilities while actor_active was true and
        // startup waited in `checkout()`. Holding the storage mutex makes the
        // guard-to-actor handoff linearizable; new guards double-check the CAS
        // after acquiring this same mutex and therefore cannot slip behind it.
        if self.actor_active.load(Ordering::Acquire) {
            return Err(BrainRuntimeError::ActorAlreadyActive);
        }
        let state = self.state.lock();
        if let Some(reason) = self.quarantine_reason.lock().clone() {
            return Err(BrainRuntimeError::DegradedPersistence(reason));
        }
        if state.is_none() {
            return Err(BrainRuntimeError::Persistence(
                "brain session is unavailable during actor ownership handoff".to_string(),
            ));
        }
        self.actor_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| BrainRuntimeError::ActorAlreadyActive)?;
        drop(state);
        Ok(BrainActorActivation {
            cell: Arc::clone(self),
            active: true,
        })
    }

    #[cfg(test)]
    fn is_actor_active(&self) -> bool {
        self.actor_active.load(Ordering::Acquire)
    }

    pub(crate) fn lock(&self) -> BrainSessionGuard<'_> {
        self.read().unwrap_or_else(|error| panic!("{error}"))
    }

    pub(crate) fn read(&self) -> Result<BrainSessionGuard<'_>, BrainRuntimeError> {
        if let Some(reason) = self.quarantine_reason.lock().clone() {
            return Err(BrainRuntimeError::DegradedPersistence(reason));
        }
        if self.actor_active.load(Ordering::Acquire) {
            return Err(BrainRuntimeError::ActorAlreadyActive);
        }
        let mut guard = self.state.lock();
        while guard.is_none() {
            if let Some(reason) = self.quarantine_reason.lock().clone() {
                return Err(BrainRuntimeError::DegradedPersistence(reason));
            }
            self.available.wait(&mut guard);
        }
        if let Some(reason) = self.quarantine_reason.lock().clone() {
            return Err(BrainRuntimeError::DegradedPersistence(reason));
        }
        // Actor startup may have won after the optimistic check but before the
        // storage mutex became available. Never hand out a SessionState guard
        // once the single-writer fence is active: `&SessionState` still contains
        // interior-mutable graph/process capabilities.
        if self.actor_active.load(Ordering::Acquire) {
            return Err(BrainRuntimeError::ActorAlreadyActive);
        }
        Ok(BrainSessionGuard { guard })
    }

    pub(crate) fn try_lock(&self) -> Option<BrainSessionGuard<'_>> {
        if self.actor_active.load(Ordering::Acquire) || self.quarantine_reason.lock().is_some() {
            return None;
        }
        let guard = self.state.try_lock()?;
        guard.as_ref()?;
        if self.actor_active.load(Ordering::Acquire) {
            return None;
        }
        Some(BrainSessionGuard { guard })
    }

    pub(crate) fn lock_mut_before_actor(
        &self,
    ) -> Result<BrainSessionMutGuard<'_>, BrainRuntimeError> {
        if self.actor_active.load(Ordering::Acquire) {
            return Err(BrainRuntimeError::ActorAlreadyActive);
        }
        let mut guard = self.state.lock();
        while guard.is_none() {
            if let Some(reason) = self.quarantine_reason.lock().clone() {
                return Err(BrainRuntimeError::DegradedPersistence(reason));
            }
            self.available.wait(&mut guard);
        }
        if let Some(reason) = self.quarantine_reason.lock().clone() {
            return Err(BrainRuntimeError::DegradedPersistence(reason));
        }
        // Double-check after acquiring the storage mutex. If actor startup won
        // the race, it owns all future mutation even though it is waiting for
        // this guard to be released.
        if self.actor_active.load(Ordering::Acquire) {
            return Err(BrainRuntimeError::ActorAlreadyActive);
        }
        Ok(BrainSessionMutGuard { guard })
    }

    /// Revoke a hosted brain's process registry lease after its actor has
    /// checkpointed and joined. The cell may still have external Arc holders;
    /// explicit revocation prevents those inert references from keeping a
    /// stopped writer discoverable. Bound owners release through McpServer.
    pub(crate) fn release_hosted_instance_after_actor_stop(&self) -> Result<(), BrainRuntimeError> {
        let mut state = self.lock_mut_before_actor()?;
        state
            .instance
            .release()
            .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))
    }

    fn checkout(self: &Arc<Self>) -> Result<CheckedOutSession, BrainRuntimeError> {
        let mut guard = self.state.lock();
        while guard.is_none() {
            if let Some(reason) = self.quarantine_reason.lock().clone() {
                return Err(BrainRuntimeError::DegradedPersistence(reason));
            }
            self.available.wait(&mut guard);
        }
        if let Some(reason) = self.quarantine_reason.lock().clone() {
            return Err(BrainRuntimeError::DegradedPersistence(reason));
        }
        let state = guard.take().expect("brain session became available");
        drop(guard);
        Ok(CheckedOutSession {
            cell: Arc::clone(self),
            state: Some(state),
        })
    }

    fn replace(&self, state: SessionState) {
        let mut guard = self.state.lock();
        debug_assert!(guard.is_none(), "brain session replaced twice");
        debug_assert!(
            self.quarantine_reason.lock().is_none(),
            "quarantined brain session cannot be republished"
        );
        *guard = Some(state);
        drop(guard);
        self.available.notify_all();
    }

    fn quarantine(&self, state: SessionState, reason: String) {
        let mut quarantined_state = self.quarantined_state.lock();
        debug_assert!(
            quarantined_state.is_none(),
            "brain session quarantined twice"
        );
        *quarantined_state = Some(state);
        drop(quarantined_state);
        let mut quarantine = self.quarantine_reason.lock();
        if quarantine.is_none() {
            *quarantine = Some(reason);
        }
        drop(quarantine);
        self.available.notify_all();
    }

    fn checkout_quarantined_for_recovery(
        self: &Arc<Self>,
    ) -> Result<QuarantinedSessionRecovery, BrainRuntimeError> {
        let reason = self.quarantine_reason.lock().clone().ok_or_else(|| {
            BrainRuntimeError::Persistence(
                "checkpoint reconciliation requested for a session that is not quarantined"
                    .to_string(),
            )
        })?;
        let state = self.quarantined_state.lock().take().ok_or_else(|| {
            BrainRuntimeError::Persistence(format!(
                "quarantined session vault is empty while recovery is required: {reason}"
            ))
        })?;
        Ok(QuarantinedSessionRecovery {
            cell: Arc::clone(self),
            state: Some(state),
        })
    }

    fn return_quarantined_after_failed_recovery(&self, state: SessionState) {
        let mut quarantined = self.quarantined_state.lock();
        debug_assert!(
            quarantined.is_none(),
            "quarantined recovery returned state twice"
        );
        *quarantined = Some(state);
        drop(quarantined);
        self.available.notify_all();
    }

    // The error must return ownership of the quarantined state so the caller
    // can restore it without cloning or dropping authority-bearing runtime data.
    #[allow(clippy::result_large_err)]
    fn publish_reconciled(
        &self,
        state: SessionState,
    ) -> Result<(), (SessionState, BrainRuntimeError)> {
        let mut available = self.state.lock();
        if available.is_some() {
            return Err((
                state,
                BrainRuntimeError::Persistence(
                    "reconciled session refused to replace an available live state".to_string(),
                ),
            ));
        }
        if self.quarantined_state.lock().is_some() {
            return Err((
                state,
                BrainRuntimeError::Persistence(
                    "reconciled session refused while the quarantine vault was occupied"
                        .to_string(),
                ),
            ));
        }
        *self.quarantine_reason.lock() = None;
        *available = Some(state);
        drop(available);
        self.available.notify_all();
        Ok(())
    }

    #[cfg(test)]
    fn quarantine_detail(&self) -> Option<String> {
        self.quarantine_reason.lock().clone()
    }

    #[cfg(test)]
    pub(crate) fn storage_mutex_available(&self) -> bool {
        self.state.try_lock().is_some()
    }
}

struct BrainActorActivation {
    cell: Arc<BrainSessionCell>,
    active: bool,
}

impl BrainActorActivation {
    fn release(&mut self) {
        if self.active {
            self.cell.actor_active.store(false, Ordering::Release);
            self.active = false;
            self.cell.available.notify_all();
        }
    }
}

impl Drop for BrainActorActivation {
    fn drop(&mut self) {
        self.release();
    }
}

pub(crate) struct BrainSessionGuard<'a> {
    guard: ParkingMutexGuard<'a, Option<SessionState>>,
}

impl std::ops::Deref for BrainSessionGuard<'_> {
    type Target = SessionState;

    fn deref(&self) -> &Self::Target {
        self.guard
            .as_ref()
            .expect("brain session guard always contains state")
    }
}

pub(crate) struct BrainSessionMutGuard<'a> {
    guard: ParkingMutexGuard<'a, Option<SessionState>>,
}

impl std::ops::Deref for BrainSessionMutGuard<'_> {
    type Target = SessionState;

    fn deref(&self) -> &Self::Target {
        self.guard
            .as_ref()
            .expect("mutable brain session guard always contains state")
    }
}

impl std::ops::DerefMut for BrainSessionMutGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard
            .as_mut()
            .expect("mutable brain session guard always contains state")
    }
}

struct CheckedOutSession {
    cell: Arc<BrainSessionCell>,
    state: Option<SessionState>,
}

impl CheckedOutSession {
    /// Remove the unacknowledged postimage from every readable path and make
    /// future checkout/legacy reads fail closed. The value remains retained in
    /// an inaccessible vault so its writer lease is not released early; the
    /// actor reconciler restores the selected CURRENT/predecessor in-process.
    fn quarantine(mut self, reason: String) {
        if let Some(state) = self.state.take() {
            self.cell.quarantine(state, reason);
        }
    }
}

impl std::ops::Deref for CheckedOutSession {
    type Target = SessionState;

    fn deref(&self) -> &Self::Target {
        self.state
            .as_ref()
            .expect("checked-out brain session contains state")
    }
}

impl std::ops::DerefMut for CheckedOutSession {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.state
            .as_mut()
            .expect("checked-out brain session contains state")
    }
}

impl Drop for CheckedOutSession {
    fn drop(&mut self) {
        if let Some(state) = self.state.take() {
            self.cell.replace(state);
        }
    }
}

struct QuarantinedSessionRecovery {
    cell: Arc<BrainSessionCell>,
    state: Option<SessionState>,
}

impl QuarantinedSessionRecovery {
    fn publish(mut self) -> Result<(), BrainRuntimeError> {
        if let Some(state) = self.state.take() {
            match self.cell.publish_reconciled(state) {
                Ok(()) => Ok(()),
                Err((state, error)) => {
                    self.state = Some(state);
                    Err(error)
                }
            }
        } else {
            Err(BrainRuntimeError::Persistence(
                "reconciled session publication lost its state".to_string(),
            ))
        }
    }
}

impl std::ops::Deref for QuarantinedSessionRecovery {
    type Target = SessionState;

    fn deref(&self) -> &Self::Target {
        self.state
            .as_ref()
            .expect("quarantined recovery contains session state")
    }
}

impl std::ops::DerefMut for QuarantinedSessionRecovery {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.state
            .as_mut()
            .expect("quarantined recovery contains session state")
    }
}

impl Drop for QuarantinedSessionRecovery {
    fn drop(&mut self) {
        if let Some(state) = self.state.take() {
            self.cell.return_quarantined_after_failed_recovery(state);
        }
    }
}

pub const BRAIN_CHECKPOINT_DIRECTORY: &str = "checkpoint-store";
pub const DEFAULT_BRAIN_ACTOR_QUEUE_CAPACITY: usize = 8;
pub const BRAIN_VERSION_SCHEMA: &str = "m1nd-brain-version-v1";
pub const BRAIN_RECOVERY_SCHEMA: &str = "m1nd-brain-recovery-v1";
pub const BRAIN_RUNTIME_HEALTH_SCHEMA: &str = "m1nd-brain-runtime-health-v3";

const GRAPH_SCHEMA_ID: &str = "m1nd-graph-snapshot";
const GRAPH_SCHEMA_VERSION: u32 = m1nd_core::snapshot::SNAPSHOT_VERSION;
const ROOTS_SCHEMA_ID: &str = "m1nd-ingest-roots";
const ROOTS_SCHEMA_VERSION: &str = "1";
const SIDECAR_SCHEMA_ID: &str = "m1nd-session-sidecar";
const SIDECAR_SCHEMA_VERSION: &str = "1";
const WORKING_SET_SCHEMA: &str = "m1nd-session-working-set-v1";
const WORKING_SET_LOGICAL_NAME: &str = "session_working_set";
const WORKING_SET_RELATIVE_PATH: &str = "checkpoint-working-set-v1.json";
const UNBOUND_AUTHORITY_VALIDATOR_ID: &str = "m1nd-project-brain-unbound-authority-v1";

/// Files written by `SessionState::persist` that are sufficient to warm-boot
/// the graph and its in-memory sidecars. Runtime logs, document artifact bodies,
/// and external authority stores are intentionally outside this list.
const OPTIONAL_SESSION_SIDECARS: &[(&str, &str)] = &[
    ("binary_graph_snapshot", "graph_snapshot.bin"),
    ("plasticity_state", "plasticity_state.json"),
    ("antibodies", "antibodies.json"),
    ("tremor_state", "tremor_state.json"),
    ("trust_state", "trust_state.json"),
    ("calibration_state", "calibration_state.json"),
    ("temporal_state", "temporal_state_v1.json"),
    ("boot_memory_state", "boot_memory_state.json"),
    ("boot_config", "boot_config_v1.json"),
    ("boot_kv_migration", "boot_kv_migration_v1.json"),
    (
        "boot_kv_migration_journal",
        "boot_kv_migration_journal_v1.json",
    ),
    ("daemon_state", "daemon_state.json"),
    ("daemon_alerts", "daemon_alerts.json"),
    ("auto_ingest_state", "auto_ingest_state.json"),
    ("document_cache_index", "document_cache_index.json"),
    ("embeddings_cache", "embeddings_cache.bin"),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointWorkingSetV1 {
    schema: String,
    candidate_state_digest: String,
    entries: Vec<CheckpointWorkingSetEntryV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointWorkingSetEntryV1 {
    relative_path: String,
    #[serde(flatten)]
    presence: CheckpointWorkingSetPresenceV1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
enum CheckpointWorkingSetPresenceV1 {
    Present {
        logical_name: String,
        content_digest: String,
    },
    Absent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrainVersionV1 {
    pub schema: &'static str,
    pub epoch: u64,
    pub generation: u64,
    pub revision: u64,
}

impl BrainVersionV1 {
    pub fn fresh() -> Self {
        Self {
            schema: BRAIN_VERSION_SCHEMA,
            epoch: 1,
            generation: 0,
            revision: 0,
        }
    }

    fn from_manifest(manifest: &CheckpointManifestV1) -> Self {
        Self {
            schema: BRAIN_VERSION_SCHEMA,
            epoch: manifest.epoch,
            generation: manifest.generation,
            revision: manifest.revision,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BrainReadSnapshot<S> {
    pub brain_id: String,
    pub version: BrainVersionV1,
    pub value: S,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrainRecoveryV1 {
    pub schema: String,
    pub checkpoint_id: String,
    pub disposition: CheckpointLoadDisposition,
    pub authority_receipt: CheckpointAuthorityValidationReceiptV1,
    pub fallback_receipt: Option<CheckpointFallbackReceiptV1>,
}

/// Read-only actor health copied without entering the brain command queue or
/// taking the SessionState lock. A persistence failure therefore remains
/// observable even while the actor is busy, and never turns health into a
/// second mutation path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BrainRuntimeHealthV1 {
    pub schema: String,
    pub brain_id: String,
    pub status: String,
    pub accepting: bool,
    pub queue_capacity: usize,
    pub degraded_persistence: bool,
    pub degraded_fallback: bool,
    pub lease_status: String,
    pub last_lease_error: Option<String>,
    pub last_persistence_error: Option<String>,
    pub version: BrainVersionV1,
    pub current_checkpoint_id: Option<String>,
    pub in_doubt_checkpoint_id: Option<String>,
}

#[derive(Clone, Debug)]
struct BrainRuntimeHealthState {
    degraded_persistence: bool,
    degraded_fallback: bool,
    recovery_pending: bool,
    actor_persistence_error: Option<String>,
    lease_error: Option<String>,
    last_persistence_error: Option<String>,
    version: BrainVersionV1,
    current_checkpoint_id: Option<String>,
    in_doubt_checkpoint_id: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct BrainBootRecovery {
    pub manifest: CheckpointManifestV1,
    pub receipt: BrainRecoveryV1,
    managed_working_paths: BTreeSet<String>,
}

/// Adapter owned by the authority layer. Snapshotting and validation are kept
/// together so a provider cannot publish refs that it is unable to revalidate.
pub trait BrainCheckpointAuthority: Send + Sync {
    /// Snapshot authority-owned roots for one brain. The adapter deliberately
    /// receives no `SessionState`: authority code must not gain a second live
    /// handle to actor-owned graph/session memory while a checkpoint is being
    /// assembled.
    fn snapshot_refs(&self, brain_id: &str) -> Result<CheckpointExternalAuthorityRefsV1, String>;

    fn validate_checkpoint(
        &self,
        manifest: &CheckpointManifestV1,
        external_authority_refs_digest: &str,
    ) -> Result<CheckpointAuthorityValidationReceiptV1, String>;
}

/// Honest default while the external authority adapter is not wired. Every
/// digest is a domain-separated marker saying that authority surface is absent;
/// validation accepts only those exact markers and advertises that fact in its
/// validator id. This protects graph/session integrity, not external anti-rollback.
#[derive(Clone, Debug, Default)]
pub struct UnboundBrainCheckpointAuthority;

impl UnboundBrainCheckpointAuthority {
    fn refs() -> CheckpointExternalAuthorityRefsV1 {
        CheckpointExternalAuthorityRefsV1 {
            system_block_store_version: 0,
            mission_heads_index_digest: domain_digest("mission-heads-unbound"),
            authority_wal_root_digest: domain_digest("authority-wal-unbound"),
            intent_core_store_root_digest: domain_digest("intent-core-unbound"),
            sentinel_outbox_watermark_digest: domain_digest("sentinel-outbox-unbound"),
            autonomy_epoch_record_digest: domain_digest("autonomy-epoch-unbound"),
        }
    }
}

impl BrainCheckpointAuthority for UnboundBrainCheckpointAuthority {
    fn snapshot_refs(&self, _brain_id: &str) -> Result<CheckpointExternalAuthorityRefsV1, String> {
        Ok(Self::refs())
    }

    fn validate_checkpoint(
        &self,
        manifest: &CheckpointManifestV1,
        external_authority_refs_digest: &str,
    ) -> Result<CheckpointAuthorityValidationReceiptV1, String> {
        if manifest.external_authority_refs != Self::refs() {
            return Err(
                "checkpoint declares external authority roots but the registry has only the unbound authority adapter"
                    .to_string(),
            );
        }
        CheckpointAuthorityValidationReceiptV1::verified(
            UNBOUND_AUTHORITY_VALIDATOR_ID,
            &manifest.checkpoint_id,
            external_authority_refs_digest,
            domain_digest(&format!(
                "unbound-protected-root:{external_authority_refs_digest}"
            )),
            now_unix_ms().map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())
    }
}

struct AuthorityValidatorAdapter<'a>(&'a dyn BrainCheckpointAuthority);

impl CheckpointAuthorityValidator for AuthorityValidatorAdapter<'_> {
    fn validate(
        &self,
        manifest: &CheckpointManifestV1,
        external_authority_refs_digest: &str,
    ) -> Result<CheckpointAuthorityValidationReceiptV1, String> {
        self.0
            .validate_checkpoint(manifest, external_authority_refs_digest)
    }
}

#[derive(Debug)]
pub enum BrainRuntimeError {
    ActorAlreadyActive,
    QueueFull {
        brain_id: String,
        capacity: usize,
    },
    ActorStopped(String),
    ReplyChannelClosed(String),
    SnapshotRead(RuntimeJobFailure),
    SnapshotStale {
        expected: BrainVersionV1,
        observed: BrainVersionV1,
    },
    BrainBindingMismatch {
        expected: String,
        observed: String,
    },
    SnapshotRevisionMismatch {
        expected: u64,
        observed: u64,
    },
    Checkpoint(CheckpointError),
    CheckpointCommittedUnconfirmed {
        checkpoint_id: String,
        detail: String,
    },
    CheckpointBoundaryIndeterminate {
        candidate_checkpoint_id: String,
        observed_current_checkpoint_id: Option<String>,
        detail: String,
    },
    Persistence(String),
    DegradedPersistence(String),
    Worker(String),
}

impl BrainRuntimeError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::ActorAlreadyActive => "brain_actor_already_active",
            Self::QueueFull { .. } => "brain_actor_queue_full",
            Self::ActorStopped(_) => "brain_actor_stopped",
            Self::ReplyChannelClosed(_) => "brain_actor_reply_closed",
            Self::SnapshotRead(_) => "brain_snapshot_read_failed",
            Self::SnapshotStale { .. } => "brain_snapshot_stale",
            Self::BrainBindingMismatch { .. } => "brain_binding_mismatch",
            Self::SnapshotRevisionMismatch { .. } => "brain_snapshot_revision_mismatch",
            Self::Checkpoint(error) => error.code(),
            Self::CheckpointCommittedUnconfirmed { .. } => "brain_checkpoint_committed_unconfirmed",
            Self::CheckpointBoundaryIndeterminate { .. } => {
                "brain_checkpoint_boundary_indeterminate"
            }
            Self::Persistence(_) => "brain_persistence_failed",
            Self::DegradedPersistence(_) => "brain_degraded_persistence",
            Self::Worker(_) => "brain_worker_failed",
        }
    }

    pub fn into_job_failure(self) -> RuntimeJobFailure {
        RuntimeJobFailure::new(self.code(), self.to_string())
    }

    fn in_doubt_checkpoint_id(&self) -> Option<&str> {
        match self {
            Self::CheckpointCommittedUnconfirmed { checkpoint_id, .. } => Some(checkpoint_id),
            Self::CheckpointBoundaryIndeterminate {
                candidate_checkpoint_id,
                ..
            } => Some(candidate_checkpoint_id),
            _ => None,
        }
    }
}

impl fmt::Display for BrainRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ActorAlreadyActive => {
                write!(formatter, "brain session already has an active actor owner")
            }
            Self::QueueFull { brain_id, capacity } => write!(
                formatter,
                "brain actor queue for '{brain_id}' is full at capacity {capacity}"
            ),
            Self::ActorStopped(brain_id) => {
                write!(formatter, "brain actor '{brain_id}' is stopped")
            }
            Self::ReplyChannelClosed(brain_id) => {
                write!(
                    formatter,
                    "brain actor '{brain_id}' closed its reply channel"
                )
            }
            Self::SnapshotRead(failure) => {
                write!(
                    formatter,
                    "snapshot read failed ({}): {}",
                    failure.code, failure.message
                )
            }
            Self::SnapshotStale { expected, observed } => write!(
                formatter,
                "stale brain proposal: expected ({},{},{}), observed ({},{},{})",
                expected.epoch,
                expected.generation,
                expected.revision,
                observed.epoch,
                observed.generation,
                observed.revision
            ),
            Self::BrainBindingMismatch { expected, observed } => write!(
                formatter,
                "runtime job brain binding mismatch: expected '{expected}', observed '{observed}'"
            ),
            Self::SnapshotRevisionMismatch { expected, observed } => write!(
                formatter,
                "runtime job snapshot revision mismatch: expected {expected}, observed {observed}"
            ),
            Self::Checkpoint(error) => write!(formatter, "brain checkpoint failed: {error}"),
            Self::CheckpointCommittedUnconfirmed {
                checkpoint_id,
                detail,
            } => write!(
                formatter,
                "brain checkpoint '{checkpoint_id}' is CURRENT but could not be authoritatively confirmed: {detail}"
            ),
            Self::CheckpointBoundaryIndeterminate {
                candidate_checkpoint_id,
                observed_current_checkpoint_id,
                detail,
            } => write!(
                formatter,
                "brain checkpoint boundary is indeterminate for candidate '{candidate_checkpoint_id}' (observed CURRENT {observed_current_checkpoint_id:?}): {detail}"
            ),
            Self::Persistence(detail) => write!(formatter, "brain persistence failed: {detail}"),
            Self::DegradedPersistence(detail) => {
                write!(
                    formatter,
                    "brain is degraded by persistence failure: {detail}"
                )
            }
            Self::Worker(detail) => write!(formatter, "brain worker failed: {detail}"),
        }
    }
}

impl Error for BrainRuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Checkpoint(error) => Some(error),
            _ => None,
        }
    }
}

impl From<CheckpointError> for BrainRuntimeError {
    fn from(error: CheckpointError) -> Self {
        Self::Checkpoint(error)
    }
}

type ActorOperation = Box<dyn FnOnce(&mut BrainActorState) + Send + 'static>;

enum ActorCommand {
    Run(ActorOperation),
    Stop {
        reply: SyncSender<Result<(), String>>,
        wait_for_reconciliation: bool,
    },
}

struct BrainActorAdmission {
    accepting: bool,
    lease_fenced: bool,
}

#[cfg(test)]
struct AdmissionRaceHook {
    entered: SyncSender<()>,
    release: Receiver<()>,
}

pub(crate) struct BrainActorHandle {
    brain_id: String,
    queue_capacity: usize,
    sender: SyncSender<ActorCommand>,
    /// Linearization point shared by every ordinary admission and every
    /// accepting -> paused/stopped transition. A successful enqueue holds this
    /// gate from the accepting check through `try_send`, so once `pause`
    /// returns every command admitted before the pause is already in the FIFO.
    admission: Arc<Mutex<BrainActorAdmission>>,
    health: Arc<Mutex<BrainRuntimeHealthState>>,
    join: Mutex<Option<JoinHandle<()>>>,
    activation: Mutex<Option<BrainActorActivation>>,
    heartbeat_stop: Arc<AtomicBool>,
    heartbeat_join: Mutex<Option<JoinHandle<()>>>,
    #[cfg(test)]
    admission_race_hook: Mutex<Option<AdmissionRaceHook>>,
    #[cfg(test)]
    pause_entry_probe: Mutex<Option<SyncSender<()>>>,
}

impl BrainActorHandle {
    pub(crate) fn start(
        brain_id: String,
        session: Arc<BrainSessionCell>,
        checkpoint_root: PathBuf,
        authority: Arc<dyn BrainCheckpointAuthority>,
        queue_capacity: usize,
        recovery: Option<BrainBootRecovery>,
    ) -> Result<Arc<Self>, BrainRuntimeError> {
        Self::start_with_faults(
            brain_id,
            session,
            checkpoint_root,
            authority,
            queue_capacity,
            recovery,
            Arc::new(NoCheckpointFaults),
        )
    }

    pub(crate) fn start_with_faults(
        brain_id: String,
        session: Arc<BrainSessionCell>,
        checkpoint_root: PathBuf,
        authority: Arc<dyn BrainCheckpointAuthority>,
        queue_capacity: usize,
        recovery: Option<BrainBootRecovery>,
        checkpoint_faults: Arc<dyn CheckpointFaultInjector>,
    ) -> Result<Arc<Self>, BrainRuntimeError> {
        let activation = session.claim_actor()?;
        let had_provided_recovery = recovery.is_some();
        // Acquire the checkpoint writer before taking SessionState out of its
        // compatibility cell. A duplicate actor start must return a typed
        // WriterLocked refusal without quarantining an otherwise healthy live
        // session that this attempted start never modified.
        let store = CheckpointStore::open(checkpoint_root)?;
        let mut boot_state = session.checkout()?;
        // Callers may have cloned interior Arcs before moving SessionState into
        // the cell. Detach them at the ownership handoff so no pre-actor graph
        // capability can mutate the actor's future baseline out of band.
        boot_state
            .rebind_detached_graph()
            .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
        let heartbeat_permit = boot_state.instance.heartbeat_permit();
        match heartbeat_permit.heartbeat() {
            Ok(true) => {}
            Ok(false) => {
                return Err(BrainRuntimeError::DegradedPersistence(format!(
                    "brain '{brain_id}' lost its instance owner before actor startup"
                )))
            }
            Err(error) => {
                return Err(BrainRuntimeError::DegradedPersistence(format!(
                    "brain '{brain_id}' could not prove its instance lease before actor startup: {error}"
                )))
            }
        }
        let mut boot_files_touched = false;
        // Reconcile recovery only after this actor owns the checkpoint-store
        // writer lock. A receipt obtained before `start` is advisory: CURRENT
        // may have advanced in the gap. Conversely, `None` must never baseline
        // canonical bytes when an existing CURRENT proves this is a restart.
        let start_result = (|| {
            let validator = AuthorityValidatorAdapter(authority.as_ref());
            let observed = store.load_with_fallback(
                &validator,
                now_unix_ms().map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?,
            );
            let recovery = match observed {
                Ok(loaded) => {
                    if loaded.manifest.brain_id != brain_id {
                        return Err(BrainRuntimeError::BrainBindingMismatch {
                            expected: brain_id.clone(),
                            observed: loaded.manifest.brain_id,
                        });
                    }
                    let verified_working_set = verified_working_set(&loaded)?;
                    let expected_candidate_digest =
                        verified_working_set.candidate_state_digest.clone();
                    let legacy_working_set = expected_candidate_digest.is_none();
                    let mut managed_working_paths = verified_working_set.paths;
                    managed_working_paths.extend(rejected_current_working_paths(
                        &store,
                        &loaded,
                        &validator,
                    )?);
                    if legacy_working_set {
                        managed_working_paths
                            .extend(predecessor_working_paths(&store, &loaded.manifest)?);
                    }
                    let live_candidate = checkpoint_candidate_snapshot(&mut boot_state)?;
                    let mut live_files = candidate_present_inputs(&live_candidate);
                    live_files.push(build_working_set_input(
                        &live_candidate,
                        &managed_working_paths,
                    )?);
                    let preserve_process_state = inventory_matches(&loaded.manifest, &live_files);
                    boot_files_touched = true;
                    restore_checkpoint(
                        &boot_state.runtime_root,
                        &loaded,
                        &managed_working_paths,
                    )?;
                    boot_state
                        .reload_authoritative_from_disk(preserve_process_state)
                        .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
                    if let Some(expected) = expected_candidate_digest {
                        let rebuilt = boot_state
                            .authoritative_checkpoint_state_digest()
                            .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
                        if rebuilt != expected {
                            return Err(BrainRuntimeError::Persistence(format!(
                                "strict checkpoint rebuild digest mismatch: expected {expected}, observed {}",
                                rebuilt
                            )));
                        }
                    }
                    Some(BrainBootRecovery {
                        receipt: BrainRecoveryV1 {
                            schema: BRAIN_RECOVERY_SCHEMA.to_string(),
                            checkpoint_id: loaded.manifest.checkpoint_id.clone(),
                            disposition: loaded.disposition,
                            authority_receipt: loaded.authority_receipt.clone(),
                            fallback_receipt: loaded.fallback_receipt.clone(),
                        },
                        manifest: loaded.manifest,
                        managed_working_paths,
                    })
                }
                Err(CheckpointError::PointerMissing) if !had_provided_recovery => None,
                Err(CheckpointError::PointerMissing) => {
                    return Err(BrainRuntimeError::Persistence(
                        "checkpoint recovery receipt exists but CURRENT disappeared before actor start"
                            .to_string(),
                    ))
                }
                Err(error) => return Err(BrainRuntimeError::Checkpoint(error)),
            };
            Ok(recovery)
        })();
        let recovery = match start_result {
            Ok(recovery) => recovery,
            Err(error) => {
                if boot_files_touched {
                    let detail = error.to_string();
                    boot_state.quarantine(detail);
                }
                return Err(error);
            }
        };
        let session_read_only = boot_state.read_only;
        drop(boot_state);
        let version = recovery
            .as_ref()
            .map(|recovery| BrainVersionV1::from_manifest(&recovery.manifest))
            .unwrap_or_else(BrainVersionV1::fresh);
        let current_manifest = recovery.as_ref().map(|recovery| recovery.manifest.clone());
        let managed_working_paths = recovery
            .as_ref()
            .map(|recovery| recovery.managed_working_paths.clone())
            .unwrap_or_default();
        let recovery_receipt = recovery.map(|recovery| recovery.receipt);
        let degraded_fallback = recovery_receipt.as_ref().is_some_and(|receipt| {
            receipt.disposition == CheckpointLoadDisposition::DegradedFallback
        });
        let health = Arc::new(Mutex::new(BrainRuntimeHealthState {
            degraded_persistence: false,
            degraded_fallback,
            recovery_pending: false,
            actor_persistence_error: None,
            lease_error: None,
            last_persistence_error: None,
            version,
            current_checkpoint_id: current_manifest
                .as_ref()
                .map(|manifest| manifest.checkpoint_id.clone()),
            in_doubt_checkpoint_id: None,
        }));
        let admission = Arc::new(Mutex::new(BrainActorAdmission {
            accepting: true,
            lease_fenced: false,
        }));
        let heartbeat_stop = Arc::new(AtomicBool::new(false));
        let mut heartbeat_join = Some(spawn_actor_heartbeat_worker(
            brain_id.clone(),
            heartbeat_permit,
            Arc::clone(&heartbeat_stop),
            Arc::clone(&admission),
            Arc::clone(&health),
        )?);
        let capacity = queue_capacity.max(1);
        let (sender, receiver) = mpsc::sync_channel(capacity);
        let thread_brain_id = brain_id.clone();
        let actor_health = Arc::clone(&health);
        // A writable actor must never accept its first mutation without an
        // authoritative old generation.  Otherwise a failed first checkpoint
        // could leave a canonical working file but no CURRENT pointer, and a
        // restart would mistake the unacked postimage for a legacy baseline.
        let bootstrap_checkpoint = current_manifest.is_none() && !session_read_only;
        let actor_state = BrainActorState {
            brain_id: thread_brain_id,
            session,
            store,
            authority,
            checkpoint_faults,
            version,
            current_manifest,
            recovery_receipt,
            degraded_fallback,
            read_only: session_read_only,
            persistence_failure: None,
            in_doubt_checkpoint_id: None,
            last_ack: None,
            pending_candidate_manifest: None,
            pending_reconciliation: None,
            pending_rollback: None,
            active_checkpoint_stage: None,
            managed_working_paths,
            deferred_read_publishes: 0,
            admission: Arc::clone(&admission),
            health: actor_health,
        };
        // Start the OS thread before crossing the bootstrap checkpoint commit
        // boundary. If thread creation fails, no candidate can have reached
        // CURRENT and ordinary RAII safely republishes the checked-out state.
        // The startup handshake reports conclusive PRE-CURRENT failure, while
        // a post-CURRENT/in-doubt bootstrap remains owned by the live actor and
        // enters its autonomous reconciliation loop.
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let join = match thread::Builder::new()
            .name(format!("m1nd-brain-{}", short_id(&brain_id)))
            .spawn(move || {
                let mut actor_state = actor_state;
                let startup_result = if bootstrap_checkpoint {
                    match actor_state.checkpoint_current() {
                        Ok(_) => Ok(()),
                        Err(_) if actor_state.has_pending_recovery() => Ok(()),
                        Err(error) => Err(error),
                    }
                } else {
                    Ok(())
                };
                let should_run = startup_result.is_ok();
                if startup_tx.send(startup_result).is_err() {
                    // If the starter disappeared after CURRENT became
                    // indeterminate, the actor still owns the only recovery
                    // packet. Reconcile it before observing channel teardown.
                    if actor_state.has_pending_recovery() {
                        run_actor(receiver, actor_state);
                    }
                    return;
                }
                if should_run {
                    run_actor(receiver, actor_state);
                }
            }) {
            Ok(join) => join,
            Err(error) => {
                stop_actor_heartbeat_worker(&heartbeat_stop, heartbeat_join.take())?;
                return Err(BrainRuntimeError::Worker(error.to_string()));
            }
        };
        match startup_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                drop(sender);
                let actor_join = join.join().map_err(|_| {
                    BrainRuntimeError::Worker(format!(
                        "brain actor '{}' panicked during failed bootstrap",
                        brain_id
                    ))
                });
                let heartbeat_join_result =
                    stop_actor_heartbeat_worker(&heartbeat_stop, heartbeat_join.take());
                actor_join?;
                heartbeat_join_result?;
                return Err(error);
            }
            Err(_) => {
                drop(sender);
                let actor_join = join.join().map_err(|_| {
                    BrainRuntimeError::Worker(format!(
                        "brain actor '{}' panicked before bootstrap handshake",
                        brain_id
                    ))
                });
                let heartbeat_join_result =
                    stop_actor_heartbeat_worker(&heartbeat_stop, heartbeat_join.take());
                actor_join?;
                heartbeat_join_result?;
                return Err(BrainRuntimeError::Worker(format!(
                    "brain actor '{}' closed its bootstrap handshake",
                    brain_id
                )));
            }
        }
        Ok(Arc::new(Self {
            brain_id,
            queue_capacity: capacity,
            sender,
            admission,
            health,
            join: Mutex::new(Some(join)),
            activation: Mutex::new(Some(activation)),
            heartbeat_stop,
            heartbeat_join: Mutex::new(heartbeat_join),
            #[cfg(test)]
            admission_race_hook: Mutex::new(None),
            #[cfg(test)]
            pause_entry_probe: Mutex::new(None),
        }))
    }

    pub fn brain_id(&self) -> &str {
        &self.brain_id
    }

    pub fn queue_capacity(&self) -> usize {
        self.queue_capacity
    }

    /// Snapshot health without queueing behind work or taking the session lock.
    pub fn health_snapshot(&self) -> BrainRuntimeHealthV1 {
        // Keep the global actor lock order admission -> health. `try_enqueue`
        // holds admission while consulting reconciliation health; reversing the
        // order here would permit an ABBA deadlock between status and dispatch.
        let admission = lock_unpoisoned(&self.admission);
        let transport_accepting = admission.accepting && !admission.lease_fenced;
        let lease_fenced = admission.lease_fenced;
        drop(admission);
        let health = lock_unpoisoned(&self.health).clone();
        let accepting = transport_accepting && !health.recovery_pending;
        let status = if health.recovery_pending {
            "reconciling"
        } else if health.degraded_fallback {
            "degraded_fallback"
        } else if health.degraded_persistence {
            "degraded_persistence"
        } else if accepting {
            "healthy"
        } else {
            "stopped"
        };
        BrainRuntimeHealthV1 {
            schema: BRAIN_RUNTIME_HEALTH_SCHEMA.to_string(),
            brain_id: self.brain_id.clone(),
            status: status.to_string(),
            accepting,
            queue_capacity: self.queue_capacity,
            degraded_persistence: health.degraded_persistence,
            degraded_fallback: health.degraded_fallback,
            lease_status: if lease_fenced {
                "fenced".to_string()
            } else {
                "healthy".to_string()
            },
            last_lease_error: health.lease_error,
            last_persistence_error: health.last_persistence_error,
            version: health.version,
            current_checkpoint_id: health.current_checkpoint_id,
            in_doubt_checkpoint_id: health.in_doubt_checkpoint_id,
        }
    }

    pub fn try_read_snapshot<S, Read>(
        &self,
        read: Read,
    ) -> Result<BrainReadSnapshot<S>, BrainRuntimeError>
    where
        S: Serialize + DeserializeOwned + Send + 'static,
        Read: FnOnce(&SessionState) -> Result<S, RuntimeJobFailure> + Send + 'static,
    {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let brain_id = self.brain_id.clone();
        self.try_enqueue(Box::new(move |state| {
            let result = state.read_snapshot(read);
            let _ = reply_tx.send(result);
        }))?;
        reply_rx
            .recv()
            .map_err(|_| BrainRuntimeError::ReplyChannelClosed(brain_id))?
    }

    /// Serialize one transport dispatch on the per-brain actor. The closure is
    /// the only code that receives `&mut SessionState`; callers never acquire the
    /// session mutex themselves. Mutating commands additionally cross the
    /// actor's persistence/checkpoint fence before success is reported.
    pub fn try_execute<R, Execute>(
        &self,
        mutating: bool,
        execute: Execute,
    ) -> Result<R, BrainRuntimeError>
    where
        R: Send + 'static,
        Execute: FnOnce(&mut SessionState) -> Result<R, RuntimeJobFailure> + Send + 'static,
    {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let brain_id = self.brain_id.clone();
        self.try_enqueue(Box::new(move |state| {
            let result = state.execute(mutating, execute);
            let _ = reply_tx.send(result);
        }))?;
        reply_rx
            .recv()
            .map_err(|_| BrainRuntimeError::ReplyChannelClosed(brain_id))?
    }

    /// Execute one mutation and return the exact durable checkpoint ACK created
    /// by that same actor turn. This is intentionally narrower than
    /// `try_execute`: callers that must seal persistence evidence cannot infer an
    /// ACK from a successful return or enqueue a second checkpoint later.
    pub fn try_execute_with_checkpoint_ack<R, Execute>(
        &self,
        execute: Execute,
    ) -> Result<(R, CheckpointAckV1), BrainRuntimeError>
    where
        R: Send + 'static,
        Execute: FnOnce(&mut SessionState) -> Result<R, RuntimeJobFailure> + Send + 'static,
    {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let brain_id = self.brain_id.clone();
        self.try_enqueue(Box::new(move |state| {
            let result = state.execute_with_checkpoint_ack(execute);
            let _ = reply_tx.send(result);
        }))?;
        reply_rx
            .recv()
            .map_err(|_| BrainRuntimeError::ReplyChannelClosed(brain_id))?
    }

    pub(crate) fn commit<P, Apply>(
        &self,
        expected: BrainVersionV1,
        proposal: P,
        apply: Apply,
    ) -> Result<RuntimeJobSuccess, BrainRuntimeError>
    where
        P: Send + 'static,
        Apply: FnOnce(&mut SessionState, P) -> Result<RuntimeJobSuccess, RuntimeJobFailure>
            + Send
            + 'static,
    {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let brain_id = self.brain_id.clone();
        self.try_enqueue(Box::new(move |state| {
            let result = state.commit(expected, proposal, apply);
            let _ = reply_tx.send(result);
        }))?;
        reply_rx
            .recv()
            .map_err(|_| BrainRuntimeError::ReplyChannelClosed(brain_id))?
    }

    pub(crate) fn checkpoint_and_ack(&self) -> Result<CheckpointAckV1, BrainRuntimeError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let brain_id = self.brain_id.clone();
        self.try_enqueue(Box::new(move |state| {
            let result = state.checkpoint_current();
            let _ = reply_tx.send(result);
        }))?;
        reply_rx
            .recv()
            .map_err(|_| BrainRuntimeError::ReplyChannelClosed(brain_id))?
    }

    pub(crate) fn recovery_receipt(&self) -> Result<Option<BrainRecoveryV1>, BrainRuntimeError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let brain_id = self.brain_id.clone();
        self.try_enqueue(Box::new(move |state| {
            let _ = reply_tx.send(Ok(state.recovery_receipt.clone()));
        }))?;
        reply_rx
            .recv()
            .map_err(|_| BrainRuntimeError::ReplyChannelClosed(brain_id))?
    }

    pub(crate) fn stop_after_checkpoint(&self) -> Result<CheckpointAckV1, BrainRuntimeError> {
        self.pause()?;
        let ack = match self.checkpoint_while_paused() {
            Ok(ack) => ack,
            Err(error) => {
                self.resume();
                return Err(error);
            }
        };
        self.stop_while_paused()?;
        Ok(ack)
    }

    pub(crate) fn pause(&self) -> Result<(), BrainRuntimeError> {
        #[cfg(test)]
        if let Some(probe) = lock_unpoisoned(&self.pause_entry_probe).take() {
            let _ = probe.send(());
        }
        let mut admission = lock_unpoisoned(&self.admission);
        if !admission.accepting {
            return Err(BrainRuntimeError::ActorStopped(self.brain_id.clone()));
        }
        admission.accepting = false;
        Ok(())
    }

    pub(crate) fn resume(&self) {
        if lock_unpoisoned(&self.join).is_some() {
            lock_unpoisoned(&self.admission).accepting = true;
        }
    }

    pub(crate) fn checkpoint_while_paused(&self) -> Result<CheckpointAckV1, BrainRuntimeError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let brain_id = self.brain_id.clone();
        self.sender
            .send(ActorCommand::Run(Box::new(move |state| {
                let result = state.checkpoint_current();
                let _ = reply_tx.send(result);
            })))
            .map_err(|_| BrainRuntimeError::ActorStopped(self.brain_id.clone()))?;
        reply_rx
            .recv()
            .map_err(|_| BrainRuntimeError::ReplyChannelClosed(brain_id))?
    }

    pub(crate) fn stop(&self) -> Result<(), BrainRuntimeError> {
        if let Some(error) = self.reconciliation_admission_error() {
            return Err(error);
        }
        // Serialize the terminal admission transition with every accepting
        // check + enqueue section just like `pause`, so Stop is FIFO-after all
        // commands admitted before it closed the gate.
        lock_unpoisoned(&self.admission).accepting = false;
        match self.stop_while_paused() {
            Ok(()) => Ok(()),
            Err(error) => {
                self.resume();
                Err(error)
            }
        }
    }

    pub(crate) fn stop_while_paused(&self) -> Result<(), BrainRuntimeError> {
        if let Some(error) = self.reconciliation_admission_error() {
            return Err(error);
        }
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        self.sender
            .send(ActorCommand::Stop {
                reply: reply_tx,
                wait_for_reconciliation: false,
            })
            .map_err(|_| BrainRuntimeError::ActorStopped(self.brain_id.clone()))?;
        reply_rx
            .recv()
            .map_err(|_| BrainRuntimeError::ReplyChannelClosed(self.brain_id.clone()))?
            .map_err(BrainRuntimeError::DegradedPersistence)?;
        let join = lock_unpoisoned(&self.join).take();
        if let Some(join) = join {
            join.join().map_err(|_| {
                BrainRuntimeError::Worker(format!("brain actor '{}' panicked", self.brain_id))
            })?;
        }
        self.stop_heartbeat_worker()?;
        if let Some(mut activation) = lock_unpoisoned(&self.activation).take() {
            activation.release();
        }
        Ok(())
    }

    fn stop_heartbeat_worker(&self) -> Result<(), BrainRuntimeError> {
        let join = lock_unpoisoned(&self.heartbeat_join).take();
        stop_actor_heartbeat_worker(&self.heartbeat_stop, join)
    }

    fn reconciliation_admission_error(&self) -> Option<BrainRuntimeError> {
        let health = lock_unpoisoned(&self.health);
        health.recovery_pending.then(|| {
            BrainRuntimeError::DegradedPersistence(
                health.last_persistence_error.clone().unwrap_or_else(|| {
                    health
                        .in_doubt_checkpoint_id
                        .as_ref()
                        .map(|checkpoint_id| {
                            format!(
                                "checkpoint '{checkpoint_id}' is awaiting autonomous reconciliation"
                            )
                        })
                        .unwrap_or_else(|| {
                            "authoritative rollback is awaiting autonomous reconciliation"
                                .to_string()
                        })
                }),
            )
        })
    }

    fn try_enqueue(&self, operation: ActorOperation) -> Result<(), BrainRuntimeError> {
        // Keep the accepting check and the actual channel admission inside one
        // critical section. `pause` takes this same gate before closing
        // admission; therefore it cannot return in the old check/send window.
        let admission = lock_unpoisoned(&self.admission);
        if !admission.accepting {
            return Err(BrainRuntimeError::ActorStopped(self.brain_id.clone()));
        }
        if admission.lease_fenced {
            let detail = lock_unpoisoned(&self.health)
                .lease_error
                .clone()
                .unwrap_or_else(|| "instance lease heartbeat is fenced".to_string());
            return Err(BrainRuntimeError::DegradedPersistence(detail));
        }
        // Health advertises `accepting=false` while a checkpoint boundary is
        // being reconciled. Enforce the same admission contract here instead
        // of silently queueing work that cannot observe an authoritative
        // SessionState until the retained candidate/predecessor is selected.
        if let Some(error) = self.reconciliation_admission_error() {
            return Err(error);
        }
        #[cfg(test)]
        if let Some(hook) = lock_unpoisoned(&self.admission_race_hook).take() {
            let _ = hook.entered.send(());
            let _ = hook.release.recv();
        }
        match self.sender.try_send(ActorCommand::Run(operation)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(BrainRuntimeError::QueueFull {
                brain_id: self.brain_id.clone(),
                capacity: self.queue_capacity,
            }),
            Err(TrySendError::Disconnected(_)) => {
                Err(BrainRuntimeError::ActorStopped(self.brain_id.clone()))
            }
        }
    }
}

impl Drop for BrainActorHandle {
    fn drop(&mut self) {
        lock_unpoisoned(&self.admission).accepting = false;
        let join = lock_unpoisoned(&self.join).take();
        let activation = lock_unpoisoned(&self.activation).take();
        let heartbeat_join = lock_unpoisoned(&self.heartbeat_join).take();
        let heartbeat_stop = Arc::clone(&self.heartbeat_stop);
        let Some(join) = join else {
            let _ = stop_actor_heartbeat_worker(&heartbeat_stop, heartbeat_join);
            if let Some(mut activation) = activation {
                activation.release();
            }
            return;
        };

        // Drop can race a queued command that has not yet crossed CURRENT, so
        // a health snapshot taken here cannot safely decide whether Stop may
        // discard a reconciliation packet. Transfer shutdown to a detached
        // guardian and let the actor itself defer Stop until pending recovery
        // is complete. This also avoids joining the actor from its own thread.
        let sender = self.sender.clone();
        let ownership = Arc::new(Mutex::new(Some((
            join,
            activation,
            sender,
            heartbeat_stop,
            heartbeat_join,
        ))));
        let guardian_ownership = Arc::clone(&ownership);
        let guardian_name = format!("m1nd-brain-guardian-{}", short_id(&self.brain_id));
        let spawned = thread::Builder::new().name(guardian_name).spawn(move || {
            let (join, activation, sender, heartbeat_stop, heartbeat_join) =
                lock_unpoisoned(&guardian_ownership)
                    .take()
                    .expect("brain guardian owns actor and heartbeat lifecycle");
            let (reply_tx, reply_rx) = mpsc::sync_channel(1);
            if sender
                .send(ActorCommand::Stop {
                    reply: reply_tx,
                    wait_for_reconciliation: true,
                })
                .is_ok()
            {
                let _ = reply_rx.recv();
            }
            let _ = join.join();
            let _ = stop_actor_heartbeat_worker(&heartbeat_stop, heartbeat_join);
            if let Some(mut activation) = activation {
                activation.release();
            }
        });
        if spawned.is_err() {
            // Thread creation failure must not release the actor fence while a
            // queued command may be crossing CURRENT. Leaking this tiny packet
            // is safer than enabling a second writer at an unknown boundary.
            std::mem::forget(ownership);
        }
    }
}

struct PendingCheckpointReconciliation {
    candidate_manifest: CheckpointManifestV1,
    previous_manifest: Option<CheckpointManifestV1>,
    stage: Option<CheckpointPersistenceStage>,
}

#[derive(Clone)]
struct PendingAuthoritativeRollback {
    authoritative_manifest: Option<CheckpointManifestV1>,
    version_before: BrainVersionV1,
    stage: Option<CheckpointPersistenceStage>,
}

/// O(1) witness of the durable shape of a session, captured on both sides of an
/// actor callback.
///
/// The actor used to answer "did this turn change durable state?" by serializing
/// the whole world twice and comparing SHA-256 digests. That question is asked on
/// every call, and the answer for a graph verb is *always yes* — plasticity Step 8
/// legitimately rewrites edge weights on every read (`query()` calls
/// `plasticity.update(graph, ..)`), so the byte digest always moves and every read
/// published a full durable checkpoint of the entire state.
///
/// These counters answer the question the fence actually cares about — did the
/// callback change the *structure* of the graph, or the session's own generations —
/// without touching a byte of the state. `Graph::generation` is incremented by
/// `add_node`/`add_edge` and deliberately NOT by plasticity (FM-PL-006 only asserts
/// it), which is exactly the read-versus-mutation line.
#[derive(Clone, Copy, PartialEq, Eq)]
struct DurableWitnessV1 {
    session_generations: (u64, u64, u64),
    graph_generation: m1nd_core::types::Generation,
}

impl DurableWitnessV1 {
    fn capture(state: &SessionState) -> Self {
        Self {
            session_generations: session_generation_tuple(state),
            graph_generation: state.graph.read().generation,
        }
    }
}

struct BrainActorState {
    brain_id: String,
    session: Arc<BrainSessionCell>,
    store: CheckpointStore,
    authority: Arc<dyn BrainCheckpointAuthority>,
    checkpoint_faults: Arc<dyn CheckpointFaultInjector>,
    version: BrainVersionV1,
    current_manifest: Option<CheckpointManifestV1>,
    recovery_receipt: Option<BrainRecoveryV1>,
    degraded_fallback: bool,
    read_only: bool,
    persistence_failure: Option<String>,
    in_doubt_checkpoint_id: Option<String>,
    last_ack: Option<CheckpointAckV1>,
    pending_candidate_manifest: Option<CheckpointManifestV1>,
    pending_reconciliation: Option<PendingCheckpointReconciliation>,
    pending_rollback: Option<PendingAuthoritativeRollback>,
    /// Retained before authority snapshotting/manifest preview so a failure
    /// that occurs before the full reconciliation packet exists can still
    /// close the exact PRE-CURRENT staging capability.
    active_checkpoint_stage: Option<CheckpointPersistenceStage>,
    /// Every canonical path ever owned by the selected generation or a
    /// candidate attempted by this actor. Post-CURRENT projection removes
    /// entries absent from the new explicit PRESENT/ABSENT inventory.
    managed_working_paths: BTreeSet<String>,
    /// Read turns whose only durable claim was a routine staged persist, held
    /// back from publishing a whole-brain checkpoint. Reset by every checkpoint.
    deferred_read_publishes: u32,
    admission: Arc<Mutex<BrainActorAdmission>>,
    health: Arc<Mutex<BrainRuntimeHealthState>>,
}

impl BrainActorState {
    fn has_pending_recovery(&self) -> bool {
        self.pending_reconciliation.is_some() || self.pending_rollback.is_some()
    }

    fn clear_rollback_packet(&mut self) {
        self.pending_rollback = None;
        self.active_checkpoint_stage = None;
    }

    fn ensure_checkpointable(&self) -> Result<(), BrainRuntimeError> {
        let admission = lock_unpoisoned(&self.admission);
        if admission.lease_fenced {
            let detail = lock_unpoisoned(&self.health)
                .lease_error
                .clone()
                .unwrap_or_else(|| "instance lease heartbeat is fenced".to_string());
            return Err(BrainRuntimeError::DegradedPersistence(detail));
        }
        drop(admission);
        if self.read_only {
            return Err(BrainRuntimeError::Persistence(
                "read-only brain actor refuses mutation/checkpoint admission".to_string(),
            ));
        }
        if self.degraded_fallback {
            return Err(BrainRuntimeError::Persistence(
                "brain booted from a degraded fallback receipt; CURRENT repair is required before mutation/checkpoint"
                    .to_string(),
            ));
        }
        Ok(())
    }

    fn ensure_writable(&self) -> Result<(), BrainRuntimeError> {
        self.ensure_checkpointable()?;
        if let Some(detail) = &self.persistence_failure {
            return Err(BrainRuntimeError::DegradedPersistence(detail.clone()));
        }
        Ok(())
    }

    fn publish_health(&self) {
        let mut health = lock_unpoisoned(&self.health);
        health.actor_persistence_error = self.persistence_failure.clone();
        health.degraded_persistence =
            health.actor_persistence_error.is_some() || health.lease_error.is_some();
        health.degraded_fallback = self.degraded_fallback;
        health.recovery_pending = self.has_pending_recovery();
        health.last_persistence_error = health
            .actor_persistence_error
            .clone()
            .or_else(|| health.lease_error.clone());
        health.version = self.version;
        health.current_checkpoint_id = self
            .current_manifest
            .as_ref()
            .map(|manifest| manifest.checkpoint_id.clone());
        health.in_doubt_checkpoint_id = self.in_doubt_checkpoint_id.clone();
    }

    fn mark_persistence_failure(&mut self, error: &BrainRuntimeError) {
        self.persistence_failure = Some(error.to_string());
        self.publish_health();
    }

    fn restore_authoritative_working_files(
        &self,
        runtime_root: &Path,
    ) -> Result<(), BrainRuntimeError> {
        let Some(manifest) = self.current_manifest.as_ref() else {
            return Err(BrainRuntimeError::Persistence(
                "cannot restore failed mutation: no authoritative CURRENT manifest exists"
                    .to_string(),
            ));
        };
        let files = self
            .store
            .read_verified_manifest_files(manifest)
            .map_err(BrainRuntimeError::Checkpoint)?;
        let authoritative_paths = manifest
            .file_inventory
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect::<BTreeSet<_>>();

        // `SessionState::persist` owns this finite working set. Remove a file
        // that a failed mutation newly materialized but that the authoritative
        // checkpoint says was absent, then restore every authoritative byte.
        for relative_path in &self.managed_working_paths {
            validate_relative_path(relative_path)?;
            if !authoritative_paths.contains(relative_path.as_str()) {
                remove_regular_working_file_if_present(runtime_root, relative_path)?;
            }
        }
        for (file, bytes) in files {
            validate_relative_path(&file.relative_path)?;
            atomic_restore_file(runtime_root, &file.relative_path, &bytes)?;
        }
        Ok(())
    }

    fn verify_reloaded_state_digest(
        &self,
        state: &mut SessionState,
    ) -> Result<(), BrainRuntimeError> {
        let Some(manifest) = self.current_manifest.as_ref() else {
            return Err(BrainRuntimeError::Persistence(
                "cannot verify reloaded state without an authoritative CURRENT manifest"
                    .to_string(),
            ));
        };
        let files = self
            .store
            .read_verified_manifest_files(manifest)
            .map_err(BrainRuntimeError::Checkpoint)?;
        let Some((_, bytes)) = files
            .iter()
            .find(|(file, _)| file.logical_name == WORKING_SET_LOGICAL_NAME)
        else {
            // Legacy checkpoints did not seal an in-memory candidate digest.
            // They remain manifest-verified but cannot claim exact rebuild.
            return Ok(());
        };
        let working_set: CheckpointWorkingSetV1 = serde_json::from_slice(bytes)
            .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
        if working_set.schema != WORKING_SET_SCHEMA {
            return Err(BrainRuntimeError::Persistence(
                "authoritative working-set schema changed during rollback".to_string(),
            ));
        }
        let rebuilt = state
            .authoritative_checkpoint_state_digest()
            .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
        if rebuilt != working_set.candidate_state_digest {
            return Err(BrainRuntimeError::Persistence(format!(
                "authoritative rollback rebuilt a different durable state: expected {}, observed {}",
                working_set.candidate_state_digest, rebuilt
            )));
        }
        Ok(())
    }

    fn quarantine_failed_state(
        &mut self,
        mut state: CheckedOutSession,
        version_before: BrainVersionV1,
        mut error: BrainRuntimeError,
    ) -> BrainRuntimeError {
        let must_quarantine = if let Some(checkpoint_id) = error.in_doubt_checkpoint_id() {
            self.in_doubt_checkpoint_id = Some(checkpoint_id.to_string());
            true
        } else {
            let retained_stage = self
                .pending_reconciliation
                .as_ref()
                .and_then(|pending| pending.stage.clone())
                .or_else(|| {
                    self.pending_rollback
                        .as_ref()
                        .and_then(|pending| pending.stage.clone())
                })
                .or_else(|| self.active_checkpoint_stage.clone());
            self.in_doubt_checkpoint_id = None;
            let rollback = match catch_unwind(AssertUnwindSafe(|| {
                if let Some(stage) = retained_stage {
                    state
                        .abort_checkpoint_staging(stage)
                        .map_err(|stage_error| {
                            BrainRuntimeError::Persistence(format!(
                                "authoritative rollback could not close the rejected stage: {stage_error}"
                            ))
                        })?;
                    if let Some(pending) = self.pending_reconciliation.as_mut() {
                        pending.stage = None;
                    }
                    if let Some(pending) = self.pending_rollback.as_mut() {
                        pending.stage = None;
                    }
                    self.active_checkpoint_stage = None;
                }
                if self.current_manifest.is_none() {
                    // No CURRENT means candidate-first staging is the only
                    // possible write owner. Once that token is aborted, the
                    // checked-out boot preimage is authoritative and can be
                    // republished without reading friendly defaults.
                    Ok(())
                } else {
                    let runtime_root = state.runtime_root.clone();
                    self.restore_authoritative_working_files(&runtime_root)
                        .and_then(|()| {
                            state
                                .reload_authoritative_from_disk(false)
                                .map_err(|reload_error| {
                                    BrainRuntimeError::Persistence(format!(
                                        "strict in-memory reload of authoritative checkpoint failed: {reload_error}"
                                    ))
                                })?;
                            self.verify_reloaded_state_digest(&mut state)
                        })
                }
            })) {
                Ok(result) => result,
                Err(payload) => Err(BrainRuntimeError::Worker(format!(
                    "authoritative checkpoint rollback panicked: {}",
                    panic_payload_detail(payload)
                ))),
            };
            if let Err(restore_error) = rollback {
                error = BrainRuntimeError::Persistence(format!(
                    "{}; authoritative working-file rollback also failed: {}",
                    error, restore_error
                ));
                // Never quarantine without a durable in-process recovery owner.
                // Boundary packets retain their candidate/predecessor pair; a
                // pre-boundary callback or stage failure gets an explicit
                // authoritative rollback packet for the autonomous loop.
                if !self.has_pending_recovery() {
                    self.pending_rollback = Some(PendingAuthoritativeRollback {
                        authoritative_manifest: self.current_manifest.clone(),
                        version_before,
                        stage: self.active_checkpoint_stage.clone(),
                    });
                }
                true
            } else {
                self.version = self
                    .current_manifest
                    .as_ref()
                    .map(BrainVersionV1::from_manifest)
                    .unwrap_or(version_before);
                self.pending_reconciliation = None;
                self.clear_rollback_packet();
                false
            }
        };
        if must_quarantine {
            self.mark_persistence_failure(&error);
            let detail = error.to_string();
            state.quarantine(detail);
        } else {
            // A conclusively PRE-CURRENT failure whose authoritative preimage
            // was restored is a failed operation, not a permanently degraded
            // brain. Keep the actor available so an agent can retry after the
            // external fault clears without requiring a process restart.
            self.clear_persistence_failure();
        }
        error
    }

    fn clear_persistence_failure(&mut self) {
        self.persistence_failure = None;
        self.in_doubt_checkpoint_id = None;
        self.publish_health();
    }

    fn reconcile_pending_rollback(&mut self) -> Result<bool, BrainRuntimeError> {
        self.ensure_checkpointable()?;
        let Some(pending) = self.pending_rollback.clone() else {
            return Ok(false);
        };
        let mut state = self.session.checkout_quarantined_for_recovery()?;
        if let Some(stage) = pending.stage {
            state
                .abort_checkpoint_staging(stage)
                .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
            if let Some(retained) = self.pending_rollback.as_mut() {
                retained.stage = None;
            }
            self.active_checkpoint_stage = None;
        }

        match pending.authoritative_manifest {
            Some(expected_manifest) => {
                let validator = AuthorityValidatorAdapter(self.authority.as_ref());
                let loaded = self
                    .store
                    .load_current(&validator)
                    .map_err(BrainRuntimeError::Checkpoint)?;
                if loaded.manifest != expected_manifest {
                    return Err(BrainRuntimeError::CheckpointBoundaryIndeterminate {
                        candidate_checkpoint_id: expected_manifest.checkpoint_id,
                        observed_current_checkpoint_id: Some(loaded.manifest.checkpoint_id),
                        detail: "authoritative rollback loaded a different CURRENT manifest"
                            .to_string(),
                    });
                }
                let working_set = verified_working_set(&loaded)?;
                self.managed_working_paths.extend(working_set.paths);
                project_checkpoint_working_set(
                    &state.runtime_root,
                    &loaded,
                    &self.managed_working_paths,
                )?;
                state
                    .reload_authoritative_from_disk(false)
                    .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
                self.current_manifest = Some(loaded.manifest.clone());
                self.verify_reloaded_state_digest(&mut state)?;
                self.version = BrainVersionV1::from_manifest(&loaded.manifest);
                self.last_ack = None;
            }
            None => match self.store.current_pointer() {
                Err(CheckpointError::PointerMissing) => {
                    self.version = pending.version_before;
                    self.current_manifest = None;
                    self.last_ack = None;
                }
                Ok(pointer) => {
                    return Err(BrainRuntimeError::CheckpointBoundaryIndeterminate {
                        candidate_checkpoint_id: "bootstrap-pre-current".to_string(),
                        observed_current_checkpoint_id: Some(pointer.current_checkpoint_id),
                        detail: "bootstrap rollback refuses an unexpected CURRENT generation"
                            .to_string(),
                    });
                }
                Err(error) => return Err(BrainRuntimeError::Checkpoint(error)),
            },
        }

        // Publication is the final linearization point. The recovery guard
        // retains the state on refusal, and the packet is cleared only after
        // the cell accepts the rebuilt authoritative value.
        state.publish()?;
        self.pending_rollback = None;
        self.active_checkpoint_stage = None;
        self.clear_persistence_failure();
        Ok(true)
    }

    fn reconcile_pending_checkpoint(&mut self) -> Result<bool, BrainRuntimeError> {
        self.ensure_checkpointable()?;
        let Some(pending) = self.pending_reconciliation.as_ref() else {
            return Ok(false);
        };
        let candidate_manifest = pending.candidate_manifest.clone();
        let previous_manifest = pending.previous_manifest.clone();
        let stage = pending.stage.clone();
        let mut state = self.session.checkout_quarantined_for_recovery()?;
        let pointer = self
            .store
            .current_pointer()
            .map_err(BrainRuntimeError::Checkpoint)?;
        let validator = AuthorityValidatorAdapter(self.authority.as_ref());

        if pointer.current_checkpoint_id == candidate_manifest.checkpoint_id {
            let ack = self
                .store
                .reconcile_current_manifest(&candidate_manifest, self.checkpoint_faults.as_ref())
                .map_err(BrainRuntimeError::Checkpoint)?;
            let loaded = self
                .store
                .load_current(&validator)
                .map_err(BrainRuntimeError::Checkpoint)?;
            if loaded.manifest != candidate_manifest {
                return Err(BrainRuntimeError::CheckpointBoundaryIndeterminate {
                    candidate_checkpoint_id: candidate_manifest.checkpoint_id,
                    observed_current_checkpoint_id: Some(pointer.current_checkpoint_id),
                    detail:
                        "reconciliation loaded a manifest different from the retained candidate"
                            .to_string(),
                });
            }
            let working_set = verified_working_set(&loaded)?;
            let expected_candidate_digest =
                working_set.candidate_state_digest.ok_or_else(|| {
                    BrainRuntimeError::Persistence(
                        "retained candidate is missing exact working-set digest metadata"
                            .to_string(),
                    )
                })?;
            self.managed_working_paths.extend(working_set.paths);
            project_checkpoint_working_set(
                &state.runtime_root,
                &loaded,
                &self.managed_working_paths,
            )?;
            if let Some(stage) = stage {
                let rebuilt = Self::candidate_with_panic_fence(&state, &stage)?;
                if rebuilt.state_digest != expected_candidate_digest {
                    return Err(BrainRuntimeError::Persistence(format!(
                        "in-process candidate reconciliation digest mismatch: expected {expected_candidate_digest}, observed {}",
                        rebuilt.state_digest
                    )));
                }
                state
                    .apply_staged_post_commit_effects(&stage)
                    .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
                Self::finish_stage_with_panic_fence(&mut state, stage)?;
                if let Some(retained) = self.pending_reconciliation.as_mut() {
                    retained.stage = None;
                }
                self.active_checkpoint_stage = None;
            } else {
                let rebuilt = state
                    .authoritative_checkpoint_state_digest()
                    .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
                if rebuilt != expected_candidate_digest {
                    return Err(BrainRuntimeError::Persistence(format!(
                        "reconciled candidate publication digest mismatch: expected {expected_candidate_digest}, observed {rebuilt}"
                    )));
                }
            }
            self.version = BrainVersionV1::from_manifest(&loaded.manifest);
            self.current_manifest = Some(loaded.manifest);
            self.last_ack = Some(ack);
        } else if previous_manifest
            .as_ref()
            .is_some_and(|previous| previous.checkpoint_id == pointer.current_checkpoint_id)
        {
            let previous = previous_manifest.expect("previous branch requires manifest");
            if let Some(stage) = stage {
                state
                    .abort_checkpoint_staging(stage)
                    .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
                if let Some(retained) = self.pending_reconciliation.as_mut() {
                    retained.stage = None;
                }
                self.active_checkpoint_stage = None;
            }
            let loaded = self
                .store
                .load_current(&validator)
                .map_err(BrainRuntimeError::Checkpoint)?;
            if loaded.manifest != previous {
                return Err(BrainRuntimeError::CheckpointBoundaryIndeterminate {
                    candidate_checkpoint_id: candidate_manifest.checkpoint_id,
                    observed_current_checkpoint_id: Some(pointer.current_checkpoint_id),
                    detail:
                        "reconciliation loaded a manifest different from the retained predecessor"
                            .to_string(),
                });
            }
            let working_set = verified_working_set(&loaded)?;
            let expected_candidate_digest = working_set.candidate_state_digest;
            self.managed_working_paths.extend(working_set.paths);
            project_checkpoint_working_set(
                &state.runtime_root,
                &loaded,
                &self.managed_working_paths,
            )?;
            state
                .reload_authoritative_from_disk(false)
                .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
            if let Some(expected) = expected_candidate_digest {
                let rebuilt = state
                    .authoritative_checkpoint_state_digest()
                    .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
                if rebuilt != expected {
                    return Err(BrainRuntimeError::Persistence(format!(
                        "in-process predecessor reconciliation digest mismatch: expected {expected}, observed {}",
                        rebuilt
                    )));
                }
            }
            self.version = BrainVersionV1::from_manifest(&loaded.manifest);
            self.current_manifest = Some(loaded.manifest);
            self.last_ack = None;
        } else {
            return Err(BrainRuntimeError::CheckpointBoundaryIndeterminate {
                candidate_checkpoint_id: candidate_manifest.checkpoint_id,
                observed_current_checkpoint_id: Some(pointer.current_checkpoint_id),
                detail: "reconciliation refuses a CURRENT pointer outside the retained candidate/predecessor pair"
                    .to_string(),
            });
        }

        state.publish()?;
        self.pending_reconciliation = None;
        self.active_checkpoint_stage = None;
        // Publish the selected authoritative SessionState before health reopens
        // admission. Observers must never see `healthy/accepting=true` while
        // the cell is still quarantined, even for one scheduling window.
        self.clear_persistence_failure();
        Ok(true)
    }

    fn refresh_external_generation(&mut self, state: &SessionState) {
        let observed = session_generation(state);
        if observed > self.version.generation {
            self.version.generation = observed;
            self.version.revision = self.version.revision.saturating_add(1);
            self.publish_health();
        }
    }

    /// Open the candidate-first persistence stage and arm the authoritative
    /// rollback packet. Deliberately serializes NOTHING: a turn that ends up
    /// changing no durable state must not pay for a preimage nobody reads, and an
    /// argument-validation refusal must not pay for one before it is even allowed
    /// to refuse.
    fn begin_state_stage(
        &mut self,
        state: &mut SessionState,
    ) -> Result<CheckpointPersistenceStage, BrainRuntimeError> {
        let stage = state
            .begin_checkpoint_staging()
            .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
        self.active_checkpoint_stage = Some(stage.clone());
        self.pending_rollback = Some(PendingAuthoritativeRollback {
            authoritative_manifest: self.current_manifest.clone(),
            version_before: self.version,
            stage: Some(stage.clone()),
        });
        Ok(stage)
    }

    /// Open the stage AND serialize the current state as the candidate.
    ///
    /// Only [`Self::checkpoint_current`] needs this: it publishes the state as it
    /// stands, with no callback in between, so its candidate IS the preimage.
    /// Every callback path uses [`Self::begin_state_stage`] and serializes once,
    /// at the end, if it turns out to owe a checkpoint — the pre-callback
    /// "baseline" those paths used to take was read by nothing and cost a full
    /// ~100 MB serialization of the brain per call.
    fn begin_state_stage_with_candidate(
        &mut self,
        state: &mut SessionState,
    ) -> Result<(CheckpointPersistenceStage, SessionCheckpointCandidate), BrainRuntimeError> {
        let stage = self.begin_state_stage(state)?;
        match Self::candidate_with_panic_fence(state, &stage) {
            Ok(candidate) => Ok((stage, candidate)),
            Err(error) => match Self::finish_stage_with_panic_fence(state, stage) {
                Ok(_) => {
                    self.active_checkpoint_stage = None;
                    self.pending_rollback = None;
                    Err(error)
                }
                Err(close_error) => Err(BrainRuntimeError::Persistence(format!(
                    "{error}; persistence stage cleanup also failed: {close_error}"
                ))),
            },
        }
    }

    /// Replace the live graph with a deep clone so an Arc a callback escaped can
    /// no longer reach actor-owned state. This is the isolation fence, kept
    /// independent of candidate serialization so a turn can pay for the fence
    /// without paying to serialize the whole brain.
    fn rebind_after_callback(state: &mut SessionState) -> Result<(), BrainRuntimeError> {
        let rebind_started = Instant::now();
        let result = match catch_unwind(AssertUnwindSafe(|| state.rebind_detached_graph())) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(BrainRuntimeError::Persistence(format!(
                "could not detach actor-owned graph after callback: {error}"
            ))),
            Err(payload) => Err(BrainRuntimeError::Worker(format!(
                "detaching actor-owned graph panicked: {}",
                panic_payload_detail(payload)
            ))),
        };
        log_brain_stage("  rebind_detached_graph", rebind_started);
        result
    }

    /// The isolation fence for turns that do NOT serialize a candidate.
    ///
    /// A deep clone of a 17k-node graph is not free, and a turn that publishes
    /// nothing is exactly the turn that must stay cheap. The fence is only needed
    /// when a second owner of the graph Arc actually exists: the only way a
    /// callback can reach actor-owned state after its boundary is by having kept
    /// an `Arc` clone alive, and that is precisely what `strong_count > 1` reports.
    /// If both counts are at their floor the actor is the sole owner, nothing can
    /// alias it, and rebinding would only burn a full encode/decode. Racing the
    /// check is not possible in the direction that matters: a new strong clone can
    /// only be minted FROM a live strong clone or by upgrading a `Weak`, and both
    /// are counted here BEFORE the stage closes.
    fn rebind_if_callback_escaped_graph(state: &mut SessionState) -> Result<(), BrainRuntimeError> {
        // `weak_count` matters as much as `strong_count`: a callback that escaped a
        // `Weak` can upgrade it after this check and reach the live graph again.
        if Arc::strong_count(&state.graph) > 1 || Arc::weak_count(&state.graph) > 0 {
            return Self::rebind_after_callback(state);
        }
        Ok(())
    }

    fn post_callback_candidate(
        state: &mut SessionState,
        stage: &CheckpointPersistenceStage,
    ) -> Result<SessionCheckpointCandidate, BrainRuntimeError> {
        Self::rebind_after_callback(state)?;
        let candidate_started = Instant::now();
        let candidate = Self::candidate_with_panic_fence(state, stage);
        log_brain_stage("  checkpoint_candidate", candidate_started);
        candidate
    }

    // Each argument is independent rollback evidence captured at a different
    // boundary; keeping them explicit avoids an ambiguously partially-filled packet.
    #[allow(clippy::too_many_arguments)]
    /// Close out a refused callback by restoring the authoritative preimage.
    ///
    /// A refused command is always rolled back, including in-memory state the
    /// checkpoint inventory does not carry (`queries_processed` and friends):
    /// reloading from the authoritative checkpoint is the only mechanism that
    /// reverts those, and `domain_error_is_returned_exactly_and_partial_mutation_is_rolled_back`
    /// holds that line.
    fn callback_failure(
        &mut self,
        mut state: CheckedOutSession,
        version_before: BrainVersionV1,
        stage: CheckpointPersistenceStage,
        callback_error: BrainRuntimeError,
    ) -> BrainRuntimeError {
        let post = match Self::post_callback_candidate(&mut state, &stage) {
            Ok(candidate) => candidate,
            Err(_witness_error) => {
                // A callback can leave the live graph temporarily inconsistent
                // with derived sidecars (for example, an interior mutation from
                // a read callback). Failure to serialize that postimage is
                // evidence that rollback is required, not evidence that the
                // authoritative preimage is unrecoverable. Attempt the same
                // fail-closed restore/rebuild path used for a classified
                // mutation, and quarantine only if that rollback itself fails.
                return self.rollback_callback_state(state, version_before, stage, callback_error);
            }
        };

        self.managed_working_paths
            .extend(post.files.iter().map(|file| file.relative_path.clone()));
        self.rollback_callback_state(state, version_before, stage, callback_error)
    }

    fn rollback_callback_state(
        &mut self,
        mut state: CheckedOutSession,
        version_before: BrainVersionV1,
        stage: CheckpointPersistenceStage,
        callback_error: BrainRuntimeError,
    ) -> BrainRuntimeError {
        let runtime_root = state.runtime_root.clone();
        let rollback = match catch_unwind(AssertUnwindSafe(|| {
            state
                .abort_checkpoint_staging(stage)
                .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
            if let Some(pending) = self.pending_rollback.as_mut() {
                pending.stage = None;
            }
            self.active_checkpoint_stage = None;
            self.restore_authoritative_working_files(&runtime_root)
                .and_then(|()| {
                    state
                        .reload_authoritative_from_disk(false)
                        .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
                    self.verify_reloaded_state_digest(&mut state)
                })
        })) {
            Ok(result) => result,
            Err(payload) => Err(BrainRuntimeError::Worker(format!(
                "callback rollback panicked: {}",
                panic_payload_detail(payload)
            ))),
        };
        match rollback {
            Ok(()) => {
                self.version = self
                    .current_manifest
                    .as_ref()
                    .map(BrainVersionV1::from_manifest)
                    .unwrap_or(version_before);
                self.clear_rollback_packet();
                self.publish_health();
                callback_error
            }
            Err(rollback_error) => self.quarantine_failed_state(
                state,
                version_before,
                BrainRuntimeError::Persistence(format!(
                    "{callback_error}; authoritative callback rollback failed: {rollback_error}"
                )),
            ),
        }
    }

    fn read_snapshot<S, Read>(
        &mut self,
        read: Read,
    ) -> Result<BrainReadSnapshot<S>, BrainRuntimeError>
    where
        S: Serialize + DeserializeOwned,
        Read: FnOnce(&SessionState) -> Result<S, RuntimeJobFailure>,
    {
        let session = Arc::clone(&self.session);
        let mut state = session.checkout()?;
        self.refresh_external_generation(&state);
        let version_before = self.version;
        let stage = match self.begin_state_stage(&mut state) {
            Ok(stage) => stage,
            Err(error) => {
                return Err(self.quarantine_failed_state(state, version_before, error));
            }
        };
        let baseline_witness = DurableWitnessV1::capture(&state);
        let value = match catch_unwind(AssertUnwindSafe(|| read(&state))) {
            Ok(Ok(value)) => value,
            Ok(Err(failure)) => {
                let error = BrainRuntimeError::SnapshotRead(failure);
                return Err(self.callback_failure(state, version_before, stage, error));
            }
            Err(payload) => {
                let error = BrainRuntimeError::Worker(format!(
                    "read snapshot callback panicked: {}",
                    panic_payload_detail(payload)
                ));
                return Err(self.callback_failure(state, version_before, stage, error));
            }
        };
        // This runs on EVERY transport call (brain resolution asks the actor
        // whether the bound brain covers the caller root), so it must not
        // serialize the world to answer. The byte digest it used to compare was
        // also unsound here: `rebind_detached_graph` rebuilds `edge_plasticity`
        // from the current weights, so once a graph verb had drifted a weight the
        // postimage could never match the preimage and an honest read was refused
        // as a mutation attempt.
        if DurableWitnessV1::capture(&state) != baseline_witness {
            let error = BrainRuntimeError::Worker(
                "read snapshot callback attempted to mutate actor-owned durable state".to_string(),
            );
            return Err(self.callback_failure(state, version_before, stage, error));
        }
        // The mutation verdict is about what the CALLBACK did, so it is decided
        // above, before this fence rebuilds the graph. Any Arc the callback
        // escaped now points at a detached clone.
        if let Err(error) = Self::rebind_after_callback(&mut state) {
            return Err(self.rollback_callback_state(state, version_before, stage, error));
        }
        if let Err(error) = Self::finish_stage_with_panic_fence(&mut state, stage) {
            return Err(self.quarantine_failed_state(state, version_before, error));
        }
        self.clear_rollback_packet();
        let detached = serde_json::to_vec(&value).map_err(|error| {
            BrainRuntimeError::Worker(format!(
                "read snapshot value is not detach-serializable: {error}"
            ))
        })?;
        let value = serde_json::from_slice(&detached).map_err(|error| {
            BrainRuntimeError::Worker(format!(
                "read snapshot value could not be detached: {error}"
            ))
        })?;
        Ok(BrainReadSnapshot {
            brain_id: self.brain_id.clone(),
            version: self.version,
            value,
        })
    }

    fn execute<R, Execute>(
        &mut self,
        mutating: bool,
        execute: Execute,
    ) -> Result<R, BrainRuntimeError>
    where
        Execute: FnOnce(&mut SessionState) -> Result<R, RuntimeJobFailure>,
    {
        if mutating {
            self.ensure_writable()?;
        }
        let turn_started = Instant::now();
        let session = Arc::clone(&self.session);
        let mut state = session.checkout()?;
        self.refresh_external_generation(&state);
        let version_before = self.version;
        let staged = Instant::now();
        let stage = match self.begin_state_stage(&mut state) {
            Ok(stage) => stage,
            Err(error) => {
                return Err(self.quarantine_failed_state(state, version_before, error));
            }
        };
        let baseline_witness = DurableWitnessV1::capture(&state);
        log_brain_stage("begin_state_transaction", staged);
        let callback_started = Instant::now();
        let output = match catch_unwind(AssertUnwindSafe(|| execute(&mut state))) {
            Ok(Ok(output)) => output,
            Ok(Err(failure)) => {
                let error = BrainRuntimeError::SnapshotRead(failure);
                return Err(self.callback_failure(state, version_before, stage, error));
            }
            Err(payload) => {
                let error = BrainRuntimeError::Worker(format!(
                    "brain command callback panicked: {}",
                    panic_payload_detail(payload)
                ));
                return Err(self.callback_failure(state, version_before, stage, error));
            }
        };

        log_brain_stage("callback", callback_started);

        if let Err(error) = self.ensure_checkpointable() {
            return Err(self.callback_failure(state, version_before, stage, error));
        }

        // Decide whether this turn owes a durable checkpoint BEFORE serializing
        // anything, keeping the reasons it might apart.
        //
        // A read turn routinely dirties small, regenerable sidecars: plasticity
        // Step 8 rewrites edge weights on every graph verb, and the
        // freshness-by-traffic daemon tick calls `persist_daemon_state` on nearly
        // every dispatch. Publishing a whole-brain checkpoint for that is what made
        // a warm `seek` cost seconds and grew the store by ~113 MB per read. That
        // drift is DEFERRED here and flushed by the debounce below, by the next
        // real mutation, or by the shutdown checkpoint.
        //
        // Everything else is sealed on the spot: a classified mutation, a
        // structural change under a callback that claimed to be read-only, and a
        // queued post-CURRENT effect.
        //
        // NOTE what this decision CANNOT see. The witness watches graph structure
        // and the session generations; a verb that writes only a durable SIDECAR
        // (the antibody store, the trust ledger, daemon state, the document cache)
        // moves neither. Such a verb owes its durability to being classified a
        // mutation or to reaching a persist choke point — `session.rs` holds that
        // invariant mechanically (`no_undeclared_durable_sidecar_writer_exists`).
        let publish_requested = match state.checkpoint_publish_required(&stage) {
            Ok(requested) => requested,
            Err(error) => {
                return Err(self.quarantine_failed_state(
                    state,
                    version_before,
                    BrainRuntimeError::Persistence(error.to_string()),
                ));
            }
        };
        // A queued post-CURRENT effect is the one persist reason that CANNOT be
        // deferred: `finish_checkpoint_staging` refuses to close a stage while one
        // is outstanding, and only the checkpoint path drains it. Folding it into
        // the deferrable `publish_requested` would send the turn down the debounce
        // branch and straight into `quarantine_failed_state`. No PRODUCTION verb
        // reaches it today — `persist`, the only verb that queues one, is a
        // classified mutation — but ANY read-classified callback that queues an
        // effect does, and the quarantine is one line of code away.
        let staged_effect_pending = state.has_unresolved_staged_effects();
        let witness_moved = DurableWitnessV1::capture(&state) != baseline_witness;
        let debounce = state.auto_persist_interval.max(1);
        let debounce_due = publish_requested && self.deferred_read_publishes + 1 >= debounce;
        let durable_state_changed =
            mutating || witness_moved || staged_effect_pending || debounce_due;
        if publish_requested && !durable_state_changed {
            self.deferred_read_publishes = self.deferred_read_publishes.saturating_add(1);
        }
        if durable_state_changed && !mutating {
            if let Err(error) = self.ensure_writable() {
                return Err(self.callback_failure(state, version_before, stage, error));
            }
        }
        if durable_state_changed {
            let post_started = Instant::now();
            let candidate = match Self::post_callback_candidate(&mut state, &stage) {
                Ok(candidate) => candidate,
                Err(error) => {
                    return Err(self.rollback_callback_state(
                        state,
                        version_before,
                        stage,
                        BrainRuntimeError::Worker(format!(
                            "brain command callback produced an invalid actor-owned state: {error}"
                        )),
                    ));
                }
            };
            log_brain_stage("post_callback_candidate", post_started);
            let observed = session_generation(&state);
            self.version.generation = self.version.generation.saturating_add(1).max(observed);
            self.version.revision = self.version.revision.saturating_add(1);
            let checkpoint_started = Instant::now();
            if let Err(error) = self.checkpoint_with_panic_fence(&mut state, stage, candidate) {
                return Err(self.quarantine_failed_state(state, version_before, error));
            }
            log_brain_stage("checkpoint", checkpoint_started);
            self.deferred_read_publishes = 0;
            self.clear_persistence_failure();
        } else {
            // The deferring branch still owes the ISOLATION FENCE. Before this
            // branch existed every turn rebound through `post_callback_candidate`;
            // skipping it here would let an Arc a callback escaped keep aliasing
            // the live actor graph BETWEEN turns — mutations through it would land
            // with no classification at all, and would race the next turn's
            // witness capture. `rebind_if_callback_escaped_graph` keeps the fence
            // and keeps the turn O(1) when nothing escaped, which is every honest
            // read.
            if let Err(error) = Self::rebind_if_callback_escaped_graph(&mut state) {
                return Err(self.rollback_callback_state(state, version_before, stage, error));
            }
            if let Err(error) = Self::finish_stage_with_panic_fence(&mut state, stage) {
                return Err(self.quarantine_failed_state(state, version_before, error));
            }
            self.clear_rollback_packet();
        }
        log_brain_stage(
            if mutating {
                "TURN(mutating)"
            } else {
                "TURN(read)"
            },
            turn_started,
        );

        Ok(output)
    }

    fn execute_with_checkpoint_ack<R, Execute>(
        &mut self,
        execute: Execute,
    ) -> Result<(R, CheckpointAckV1), BrainRuntimeError>
    where
        Execute: FnOnce(&mut SessionState) -> Result<R, RuntimeJobFailure>,
    {
        self.ensure_writable()?;
        let session = Arc::clone(&self.session);
        let mut state = session.checkout()?;
        self.refresh_external_generation(&state);
        let version_before = self.version;
        let stage = match self.begin_state_stage(&mut state) {
            Ok(stage) => stage,
            Err(error) => {
                return Err(self.quarantine_failed_state(state, version_before, error));
            }
        };
        let output = match catch_unwind(AssertUnwindSafe(|| execute(&mut state))) {
            Ok(Ok(output)) => output,
            Ok(Err(failure)) => {
                let error = BrainRuntimeError::SnapshotRead(failure);
                return Err(self.callback_failure(state, version_before, stage, error));
            }
            Err(payload) => {
                let error = BrainRuntimeError::Worker(format!(
                    "checkpointed brain callback panicked: {}",
                    panic_payload_detail(payload)
                ));
                return Err(self.callback_failure(state, version_before, stage, error));
            }
        };
        if let Err(error) = self.ensure_checkpointable() {
            return Err(self.callback_failure(state, version_before, stage, error));
        }
        let candidate = match Self::post_callback_candidate(&mut state, &stage) {
            Ok(candidate) => candidate,
            Err(error) => {
                return Err(self.rollback_callback_state(
                    state,
                    version_before,
                    stage,
                    BrainRuntimeError::Worker(format!(
                        "checkpointed callback produced an invalid actor-owned state: {error}"
                    )),
                ));
            }
        };
        let observed = session_generation(&state);
        self.version.generation = self.version.generation.saturating_add(1).max(observed);
        self.version.revision = self.version.revision.saturating_add(1);
        let ack = match self.checkpoint_with_panic_fence(&mut state, stage, candidate) {
            Ok(ack) => ack,
            Err(error) => {
                return Err(self.quarantine_failed_state(state, version_before, error));
            }
        };
        self.clear_persistence_failure();
        Ok((output, ack))
    }

    fn commit<P, Apply>(
        &mut self,
        expected: BrainVersionV1,
        proposal: P,
        apply: Apply,
    ) -> Result<RuntimeJobSuccess, BrainRuntimeError>
    where
        Apply: FnOnce(&mut SessionState, P) -> Result<RuntimeJobSuccess, RuntimeJobFailure>,
    {
        self.ensure_writable()?;
        let session = Arc::clone(&self.session);
        let mut state = session.checkout()?;
        self.refresh_external_generation(&state);
        let version_before = self.version;
        if expected != self.version {
            return Err(BrainRuntimeError::SnapshotStale {
                expected,
                observed: self.version,
            });
        }
        let stage = match self.begin_state_stage(&mut state) {
            Ok(stage) => stage,
            Err(error) => {
                return Err(self.quarantine_failed_state(state, version_before, error));
            }
        };

        let success = match catch_unwind(AssertUnwindSafe(|| apply(&mut state, proposal))) {
            Ok(Ok(success)) => success,
            Ok(Err(failure)) => {
                let error = BrainRuntimeError::SnapshotRead(failure);
                return Err(self.callback_failure(state, version_before, stage, error));
            }
            Err(payload) => {
                let error = BrainRuntimeError::Worker(format!(
                    "brain proposal apply callback panicked: {}",
                    panic_payload_detail(payload)
                ));
                return Err(self.callback_failure(state, version_before, stage, error));
            }
        };
        if let Err(error) = self.ensure_checkpointable() {
            return Err(self.callback_failure(state, version_before, stage, error));
        }
        let candidate = match Self::post_callback_candidate(&mut state, &stage) {
            Ok(candidate) => candidate,
            Err(error) => {
                return Err(self.rollback_callback_state(
                    state,
                    version_before,
                    stage,
                    BrainRuntimeError::Worker(format!(
                        "proposal callback produced an invalid actor-owned state: {error}"
                    )),
                ));
            }
        };
        let observed = session_generation(&state);
        self.version.generation = self.version.generation.saturating_add(1).max(observed);
        self.version.revision = self.version.revision.saturating_add(1);

        if let Err(error) = self.checkpoint_with_panic_fence(&mut state, stage, candidate) {
            return Err(self.quarantine_failed_state(state, version_before, error));
        }
        self.clear_persistence_failure();
        Ok(success)
    }

    fn checkpoint_current(&mut self) -> Result<CheckpointAckV1, BrainRuntimeError> {
        self.ensure_checkpointable()?;
        let session = Arc::clone(&self.session);
        let mut state = session.checkout()?;
        self.refresh_external_generation(&state);
        let version_before = self.version;
        let (stage, candidate) = match self.begin_state_stage_with_candidate(&mut state) {
            Ok(transaction) => transaction,
            Err(error) => {
                return Err(self.quarantine_failed_state(state, version_before, error));
            }
        };
        match self.checkpoint_with_panic_fence(&mut state, stage, candidate) {
            Ok(ack) => {
                self.clear_persistence_failure();
                Ok(ack)
            }
            Err(error) => Err(self.quarantine_failed_state(state, version_before, error)),
        }
    }

    fn candidate_with_panic_fence(
        state: &SessionState,
        stage: &CheckpointPersistenceStage,
    ) -> Result<SessionCheckpointCandidate, BrainRuntimeError> {
        match catch_unwind(AssertUnwindSafe(|| state.checkpoint_candidate(stage))) {
            Ok(result) => result.map_err(|error| BrainRuntimeError::Persistence(error.to_string())),
            Err(payload) => Err(BrainRuntimeError::Worker(format!(
                "checkpoint candidate serialization panicked before CURRENT: {}",
                panic_payload_detail(payload)
            ))),
        }
    }

    fn finish_stage_with_panic_fence(
        state: &mut SessionState,
        stage: CheckpointPersistenceStage,
    ) -> Result<bool, BrainRuntimeError> {
        match catch_unwind(AssertUnwindSafe(|| state.finish_checkpoint_staging(stage))) {
            Ok(result) => result.map_err(|error| BrainRuntimeError::Persistence(error.to_string())),
            Err(payload) => Err(BrainRuntimeError::Worker(format!(
                "persistence stage cleanup panicked: {}",
                panic_payload_detail(payload)
            ))),
        }
    }

    fn checkpoint_with_panic_fence(
        &mut self,
        state: &mut SessionState,
        stage: CheckpointPersistenceStage,
        candidate: SessionCheckpointCandidate,
    ) -> Result<CheckpointAckV1, BrainRuntimeError> {
        self.active_checkpoint_stage = Some(stage.clone());
        match catch_unwind(AssertUnwindSafe(|| {
            self.checkpoint_state(state, stage, candidate)
        })) {
            Ok(result) => {
                self.pending_candidate_manifest = None;
                if result.is_ok() {
                    self.active_checkpoint_stage = None;
                }
                result
            }
            Err(payload) => {
                let detail = format!(
                    "checkpoint transaction panicked: {}",
                    panic_payload_detail(payload)
                );
                let Some(candidate) = self.pending_candidate_manifest.take() else {
                    return Err(BrainRuntimeError::Worker(detail));
                };
                let candidate_id = candidate.checkpoint_id.clone();
                let cached_current_id = self
                    .current_manifest
                    .as_ref()
                    .map(|manifest| manifest.checkpoint_id.clone());
                match self.store.current_pointer() {
                    Ok(pointer) if pointer.current_checkpoint_id == candidate_id => {
                        self.current_manifest = Some(candidate);
                        self.last_ack = None;
                        Err(BrainRuntimeError::CheckpointCommittedUnconfirmed {
                            checkpoint_id: candidate_id,
                            detail,
                        })
                    }
                    Ok(pointer)
                        if cached_current_id.as_deref()
                            == Some(pointer.current_checkpoint_id.as_str()) =>
                    {
                        Err(BrainRuntimeError::Worker(detail))
                    }
                    Ok(pointer) => Err(BrainRuntimeError::CheckpointBoundaryIndeterminate {
                        candidate_checkpoint_id: candidate_id,
                        observed_current_checkpoint_id: Some(pointer.current_checkpoint_id),
                        detail,
                    }),
                    Err(CheckpointError::PointerMissing) if cached_current_id.is_none() => {
                        Err(BrainRuntimeError::Worker(detail))
                    }
                    Err(pointer_error) => {
                        self.last_ack = None;
                        Err(BrainRuntimeError::CheckpointBoundaryIndeterminate {
                            candidate_checkpoint_id: candidate_id,
                            observed_current_checkpoint_id: None,
                            detail: format!(
                                "{detail}; CURRENT boundary could not be classified: {pointer_error}"
                            ),
                        })
                    }
                }
            }
        }
    }

    fn checkpoint_state(
        &mut self,
        state: &mut SessionState,
        stage: CheckpointPersistenceStage,
        candidate: SessionCheckpointCandidate,
    ) -> Result<CheckpointAckV1, BrainRuntimeError> {
        let working_set_file = build_working_set_input(&candidate, &self.managed_working_paths)?;
        let mut candidate_owned_paths = BTreeSet::from([WORKING_SET_RELATIVE_PATH.to_string()]);
        let mut files = Vec::new();
        for file in candidate.files {
            candidate_owned_paths.insert(file.relative_path.clone());
            if let CheckpointCandidatePresence::Present(bytes) = file.presence {
                files.push(CheckpointFileInputV1 {
                    logical_name: file.logical_name,
                    relative_path: file.relative_path,
                    schema_id: file.schema_id,
                    schema_version: file.schema_version,
                    bytes,
                });
            }
        }
        files.push(working_set_file);
        self.managed_working_paths
            .extend(candidate_owned_paths.iter().cloned());
        let refs = self
            .authority
            .snapshot_refs(&self.brain_id)
            .map_err(BrainRuntimeError::Persistence)?;

        let graph_digest = digest_for_logical(&files, GRAPH_SNAPSHOT_LOGICAL_NAME);
        let roots_digest = digest_for_logical(&files, INGEST_ROOTS_LOGICAL_NAME);
        let unchanged = self.current_manifest.as_ref().is_some_and(|manifest| {
            manifest.epoch == self.version.epoch
                && manifest.generation == self.version.generation
                && manifest.revision == self.version.revision
                && manifest.graph_snapshot_digest == graph_digest
                && manifest.ingest_roots_digest == roots_digest
                && manifest.external_authority_refs == refs
                && inventory_matches(manifest, &files)
        });

        let (created_at_unix_ms, previous_checkpoint_id) = if unchanged {
            let manifest = self
                .current_manifest
                .as_ref()
                .expect("unchanged requires a current manifest");
            (
                manifest.created_at_unix_ms,
                manifest.previous_checkpoint_id.clone(),
            )
        } else {
            // A direct/legacy write may change persisted bytes without bumping
            // SessionState's generation counters. Only in that case (the actor
            // version has not already advanced beyond CURRENT) synthesize one
            // OCC step. Actor commits already advance before entering here.
            if self.current_manifest.as_ref().is_some_and(|manifest| {
                (
                    self.version.epoch,
                    self.version.generation,
                    self.version.revision,
                ) <= (manifest.epoch, manifest.generation, manifest.revision)
            }) {
                self.version.generation = self.version.generation.saturating_add(1);
                self.version.revision = self.version.revision.saturating_add(1);
            }
            (
                now_unix_ms().map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?,
                self.current_manifest
                    .as_ref()
                    .map(|manifest| manifest.checkpoint_id.clone()),
            )
        };

        let mut schema_versions = BTreeMap::new();
        schema_versions.insert(
            GRAPH_SCHEMA_ID.to_string(),
            GRAPH_SCHEMA_VERSION.to_string(),
        );
        schema_versions.insert(
            ROOTS_SCHEMA_ID.to_string(),
            ROOTS_SCHEMA_VERSION.to_string(),
        );
        schema_versions.insert(
            SIDECAR_SCHEMA_ID.to_string(),
            SIDECAR_SCHEMA_VERSION.to_string(),
        );
        schema_versions.insert(WORKING_SET_SCHEMA.to_string(), "1".to_string());
        // Candidate contents are extensible (for example universal-document
        // inventory and body artifacts).  Bind every emitted file schema into
        // the manifest instead of relying on a hand-maintained fixed list;
        // otherwise a newly governed artifact reaches the checkpoint builder
        // without an authority-visible version pin.  A single schema id may
        // never describe two wire versions in one atomic candidate.
        for file in &files {
            match schema_versions.get(&file.schema_id) {
                Some(version) if version != &file.schema_version => {
                    return Err(BrainRuntimeError::Checkpoint(CheckpointError::Refused {
                        code: "checkpoint_schema_version_conflict",
                        detail: format!(
                            "schema {:?} is pinned as {:?} but file {:?} declares {:?}",
                            file.schema_id, version, file.logical_name, file.schema_version
                        ),
                    }));
                }
                Some(_) => {}
                None => {
                    schema_versions.insert(file.schema_id.clone(), file.schema_version.clone());
                }
            }
        }
        let input = CheckpointCreateV1 {
            brain_id: self.brain_id.clone(),
            epoch: self.version.epoch,
            generation: self.version.generation,
            revision: self.version.revision,
            schema_versions,
            files: std::mem::take(&mut files),
            external_authority_refs: refs,
            created_at_unix_ms,
            expected_current_checkpoint_id: previous_checkpoint_id,
        };
        let candidate_manifest =
            preview_checkpoint_manifest(&input).map_err(BrainRuntimeError::Checkpoint)?;
        self.pending_candidate_manifest = Some(candidate_manifest.clone());
        self.pending_reconciliation = Some(PendingCheckpointReconciliation {
            candidate_manifest: candidate_manifest.clone(),
            previous_manifest: self.current_manifest.clone(),
            stage: Some(stage.clone()),
        });
        self.pending_rollback = None;
        let candidate_id = candidate_manifest.checkpoint_id.clone();
        let cached_current_id = self
            .current_manifest
            .as_ref()
            .map(|manifest| manifest.checkpoint_id.clone());
        let ack = match self
            .store
            .create_checkpoint(input.clone(), self.checkpoint_faults.as_ref())
        {
            Ok(ack) => ack,
            Err(first_error) => match self
                .store
                .create_checkpoint(input, self.checkpoint_faults.as_ref())
            {
                Ok(ack) => ack,
                Err(retry_error) => match self.store.current_pointer() {
                    Ok(pointer) if pointer.current_checkpoint_id == candidate_id => {
                        self.current_manifest = Some(candidate_manifest);
                        self.last_ack = None;
                        return Err(BrainRuntimeError::CheckpointCommittedUnconfirmed {
                            checkpoint_id: candidate_id,
                            detail: format!(
                                "checkpoint create/confirm failed twice after CURRENT selected the candidate (first: {first_error}; retry: {retry_error})"
                            ),
                        });
                    }
                    Ok(pointer)
                        if cached_current_id.as_deref()
                            == Some(pointer.current_checkpoint_id.as_str()) =>
                    {
                        return Err(BrainRuntimeError::Checkpoint(retry_error));
                    }
                    Ok(pointer) => {
                        return Err(BrainRuntimeError::CheckpointBoundaryIndeterminate {
                            candidate_checkpoint_id: candidate_id,
                            observed_current_checkpoint_id: Some(
                                pointer.current_checkpoint_id,
                            ),
                            detail: format!(
                                "checkpoint create/confirm failed twice and CURRENT names a third generation (first: {first_error}; retry: {retry_error})"
                            ),
                        });
                    }
                    Err(CheckpointError::PointerMissing) if cached_current_id.is_none() => {
                        return Err(BrainRuntimeError::Checkpoint(retry_error));
                    }
                    Err(pointer_error) => {
                        // The write may have crossed rename(CURRENT) and failed
                        // before its parent fsync/readback. An unreadable pointer
                        // cannot prove PRE_FLIP, so fail into the in-doubt state.
                        self.last_ack = None;
                        return Err(BrainRuntimeError::CheckpointBoundaryIndeterminate {
                            candidate_checkpoint_id: candidate_id,
                            observed_current_checkpoint_id: None,
                            detail: format!(
                                "checkpoint create/confirm failed and CURRENT could not classify the boundary (first: {first_error}; retry: {retry_error}; pointer: {pointer_error})"
                            ),
                        });
                    }
                },
            },
        };
        // A CheckpointAck is proof that CURRENT names this exact manifest even
        // if the following external-authority validation cannot be completed.
        self.current_manifest = Some(candidate_manifest.clone());
        self.last_ack = Some(ack.clone());
        let validator = AuthorityValidatorAdapter(self.authority.as_ref());
        let loaded = self.store.load_current(&validator).map_err(|error| {
            BrainRuntimeError::CheckpointCommittedUnconfirmed {
                checkpoint_id: candidate_id.clone(),
                detail: format!(
                    "CURRENT ACK succeeded but authoritative load/validation failed: {error}"
                ),
            }
        })?;
        if loaded.manifest != candidate_manifest {
            return Err(BrainRuntimeError::CheckpointCommittedUnconfirmed {
                checkpoint_id: candidate_id,
                detail: "CURRENT authoritative readback returned a different manifest".to_string(),
            });
        }
        project_checkpoint_working_set(
            &state.runtime_root,
            &loaded,
            &self.managed_working_paths,
        )
        .map_err(|error| BrainRuntimeError::CheckpointCommittedUnconfirmed {
            checkpoint_id: candidate_id.clone(),
            detail: format!(
                "CURRENT and authority validation succeeded but canonical projection failed: {error}"
            ),
        })?;
        state
            .apply_staged_post_commit_effects(&stage)
            .map_err(|error| BrainRuntimeError::CheckpointCommittedUnconfirmed {
                checkpoint_id: candidate_id.clone(),
                detail: format!(
                    "CURRENT and canonical projection succeeded but a typed post-commit effect failed: {error}"
                ),
            })?;
        Self::finish_stage_with_panic_fence(state, stage).map_err(|error| {
            BrainRuntimeError::CheckpointCommittedUnconfirmed {
                checkpoint_id: candidate_id.clone(),
                detail: format!(
                    "CURRENT and canonical projection succeeded but persistence staging could not close: {error}"
                ),
            }
        })?;
        self.current_manifest = Some(loaded.manifest);
        self.last_ack = Some(ack.clone());
        self.pending_reconciliation = None;
        Ok(ack)
    }
}

fn spawn_actor_heartbeat_worker(
    brain_id: String,
    permit: crate::instance_registry::InstanceHeartbeatPermit,
    stop: Arc<AtomicBool>,
    admission: Arc<Mutex<BrainActorAdmission>>,
    health: Arc<Mutex<BrainRuntimeHealthState>>,
) -> Result<JoinHandle<()>, BrainRuntimeError> {
    let thread_name = format!("m1nd-brain-heartbeat-{}", short_id(&brain_id));
    let error_brain_id = brain_id.clone();
    thread::Builder::new()
        .name(thread_name)
        .spawn(move || {
            let worker = catch_unwind(AssertUnwindSafe(|| {
                let mut interval = Duration::from_secs(5);
                loop {
                    thread::park_timeout(interval);
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    match permit.heartbeat() {
                        Ok(true) => {
                            publish_lease_fence(&admission, &health, None);
                            interval = Duration::from_secs(5);
                        }
                        Ok(false) => {
                            publish_lease_fence(
                                &admission,
                                &health,
                                Some(format!(
                                    "brain '{brain_id}' instance owner was released while its actor was live"
                                )),
                            );
                            break;
                        }
                        Err(error) => {
                            publish_lease_fence(
                                &admission,
                                &health,
                                Some(format!(
                                    "brain '{brain_id}' instance heartbeat failed: {error}"
                                )),
                            );
                            // Keep the lifetime guard and keep retrying quickly.
                            // A successful ownership-checked heartbeat clears
                            // only the lease fence; pause/stop admission remains.
                            interval = Duration::from_secs(1);
                        }
                    }
                }
            }));
            if let Err(payload) = worker {
                publish_lease_fence(
                    &admission,
                    &health,
                    Some(format!(
                        "brain '{brain_id}' heartbeat worker panicked: {}",
                        panic_payload_detail(payload)
                    )),
                );
            }
        })
        .map_err(|error| {
            BrainRuntimeError::Worker(format!(
                "could not start heartbeat worker for brain '{error_brain_id}': {error}"
            ))
        })
}

fn publish_lease_fence(
    admission: &Arc<Mutex<BrainActorAdmission>>,
    health: &Arc<Mutex<BrainRuntimeHealthState>>,
    error: Option<String>,
) {
    let mut admission = lock_unpoisoned(admission);
    admission.lease_fenced = error.is_some();
    let mut health = lock_unpoisoned(health);
    health.lease_error = error;
    health.degraded_persistence =
        health.actor_persistence_error.is_some() || health.lease_error.is_some();
    health.last_persistence_error = health
        .actor_persistence_error
        .clone()
        .or_else(|| health.lease_error.clone());
}

fn stop_actor_heartbeat_worker(
    stop: &Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
) -> Result<(), BrainRuntimeError> {
    stop.store(true, Ordering::Release);
    if let Some(join) = join {
        join.thread().unpark();
        join.join().map_err(|_| {
            BrainRuntimeError::Worker("brain heartbeat worker panicked during join".to_string())
        })?;
    }
    Ok(())
}

fn run_actor(receiver: Receiver<ActorCommand>, mut state: BrainActorState) {
    let mut reconciliation_backoff = Duration::from_millis(50);
    let mut deferred_stop: Option<SyncSender<Result<(), String>>> = None;
    loop {
        if state.has_pending_recovery() {
            let reconciliation = catch_unwind(AssertUnwindSafe(|| {
                if state.pending_reconciliation.is_some() {
                    state.reconcile_pending_checkpoint()
                } else {
                    state.reconcile_pending_rollback()
                }
            }));
            match reconciliation {
                Ok(Ok(true)) => reconciliation_backoff = Duration::from_millis(50),
                Ok(Ok(false)) => {}
                Ok(Err(error)) => {
                    state.persistence_failure =
                        Some(format!("checkpoint reconciliation attempt failed: {error}"));
                    state.publish_health();
                }
                Err(payload) => {
                    // Authority/fault adapters are injected code. A panic must
                    // not kill the only thread that owns the quarantined stage;
                    // QuarantinedSessionRecovery's Drop has already returned
                    // SessionState to its vault, so retain the packet and retry.
                    state.persistence_failure = Some(format!(
                        "checkpoint reconciliation panicked: {}",
                        panic_payload_detail(payload)
                    ));
                    state.publish_health();
                }
            }
        }
        if !state.has_pending_recovery() {
            if let Some(reply) = deferred_stop.take() {
                let _ = reply.send(Ok(()));
                break;
            }
        }

        let command = if state.has_pending_recovery() {
            match receiver.recv_timeout(reconciliation_backoff) {
                Ok(command) => command,
                Err(RecvTimeoutError::Timeout) => {
                    reconciliation_backoff = reconciliation_backoff
                        .saturating_mul(2)
                        .min(Duration::from_secs(2));
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => {
                    // The public handle can disappear while a candidate is in
                    // doubt. Channel disconnect is not permission to discard
                    // that recovery packet; finish autonomously, then the
                    // non-pending recv path will observe disconnect and exit.
                    thread::sleep(reconciliation_backoff);
                    reconciliation_backoff = reconciliation_backoff
                        .saturating_mul(2)
                        .min(Duration::from_secs(2));
                    continue;
                }
            }
        } else {
            match receiver.recv() {
                Ok(command) => command,
                Err(_) => break,
            }
        };
        match command {
            ActorCommand::Run(operation) => operation(&mut state),
            ActorCommand::Stop {
                reply,
                wait_for_reconciliation,
            } => {
                if state.has_pending_recovery() {
                    if wait_for_reconciliation && deferred_stop.is_none() {
                        deferred_stop = Some(reply);
                    } else {
                        let _ = reply.send(Err(
                            "checkpoint reconciliation owns the actor; stop was refused"
                                .to_string(),
                        ));
                    }
                } else {
                    let _ = reply.send(Ok(()));
                    break;
                }
            }
        }
    }
}

/// Restore CURRENT (or its declared fallback) into the canonical session files
/// before `McpServer::new` reads them. A missing CURRENT means legacy/fresh boot;
/// a corrupt pointer or unusable generation is a hard refusal, never fresh boot.
pub(crate) fn recover_checkpoint_for_boot(
    runtime_root: &Path,
    brain_id: &str,
    authority: &dyn BrainCheckpointAuthority,
) -> Result<Option<BrainBootRecovery>, BrainRuntimeError> {
    let store = CheckpointStore::open(runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY))?;
    let validator = AuthorityValidatorAdapter(authority);
    let loaded = match store.load_with_fallback(
        &validator,
        now_unix_ms().map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?,
    ) {
        Ok(loaded) => loaded,
        Err(CheckpointError::PointerMissing) => return Ok(None),
        Err(error) => return Err(BrainRuntimeError::Checkpoint(error)),
    };
    if loaded.manifest.brain_id != brain_id {
        return Err(BrainRuntimeError::BrainBindingMismatch {
            expected: brain_id.to_string(),
            observed: loaded.manifest.brain_id.clone(),
        });
    }
    let verified_working_set = verified_working_set(&loaded)?;
    let legacy_working_set = verified_working_set.candidate_state_digest.is_none();
    let mut managed_working_paths = verified_working_set.paths;
    managed_working_paths.extend(rejected_current_working_paths(&store, &loaded, &validator)?);
    if legacy_working_set {
        managed_working_paths.extend(predecessor_working_paths(&store, &loaded.manifest)?);
    }
    restore_checkpoint(runtime_root, &loaded, &managed_working_paths)?;
    let receipt = BrainRecoveryV1 {
        schema: BRAIN_RECOVERY_SCHEMA.to_string(),
        checkpoint_id: loaded.manifest.checkpoint_id.clone(),
        disposition: loaded.disposition,
        authority_receipt: loaded.authority_receipt.clone(),
        fallback_receipt: loaded.fallback_receipt.clone(),
    };
    Ok(Some(BrainBootRecovery {
        manifest: loaded.manifest,
        receipt,
        managed_working_paths,
    }))
}

fn predecessor_working_paths(
    store: &CheckpointStore,
    manifest: &CheckpointManifestV1,
) -> Result<BTreeSet<String>, BrainRuntimeError> {
    let Some(previous_checkpoint_id) = manifest.previous_checkpoint_id.as_deref() else {
        return Ok(BTreeSet::new());
    };
    let previous = store
        .read_verified_manifest(previous_checkpoint_id)
        .map_err(BrainRuntimeError::Checkpoint)?;
    Ok(previous
        .file_inventory
        .into_iter()
        .map(|file| file.relative_path)
        .collect())
}

/// When CURRENT names an unusable successor, the selected fallback alone does
/// not describe paths first introduced by that rejected generation. Leaving
/// those paths in place would expose a postimage that no accepted checkpoint
/// owns. We therefore authenticate the rejected content-addressed manifest and
/// its working-set blob through the same authority validator, require it to be
/// the direct monotonic successor of the selected fallback, and use only its
/// path inventory for removal. Any missing binding fails boot closed.
fn rejected_current_working_paths(
    store: &CheckpointStore,
    loaded: &LoadedCheckpointV1,
    validator: &dyn CheckpointAuthorityValidator,
) -> Result<BTreeSet<String>, BrainRuntimeError> {
    if loaded.disposition != CheckpointLoadDisposition::DegradedFallback {
        return Ok(BTreeSet::new());
    }
    let receipt = loaded.fallback_receipt.as_ref().ok_or_else(|| {
        BrainRuntimeError::Persistence(
            "degraded fallback is missing its rejected-CURRENT receipt".to_string(),
        )
    })?;
    if receipt.selected_checkpoint_id != loaded.manifest.checkpoint_id {
        return Err(BrainRuntimeError::Persistence(
            "fallback receipt does not select the loaded checkpoint".to_string(),
        ));
    }
    let (rejected, bytes) = store
        .read_authorized_manifest_file(
            &receipt.requested_checkpoint_id,
            WORKING_SET_LOGICAL_NAME,
            validator,
        )
        .map_err(BrainRuntimeError::Checkpoint)?;
    if rejected.brain_id != loaded.manifest.brain_id {
        return Err(BrainRuntimeError::BrainBindingMismatch {
            expected: loaded.manifest.brain_id.clone(),
            observed: rejected.brain_id,
        });
    }
    if rejected.previous_checkpoint_id.as_deref() != Some(loaded.manifest.checkpoint_id.as_str()) {
        return Err(BrainRuntimeError::Persistence(
            "rejected CURRENT is not the direct successor of the selected fallback".to_string(),
        ));
    }
    let selected_version = (
        loaded.manifest.epoch,
        loaded.manifest.generation,
        loaded.manifest.revision,
    );
    let rejected_version = (rejected.epoch, rejected.generation, rejected.revision);
    if rejected_version <= selected_version {
        return Err(BrainRuntimeError::Persistence(
            "rejected CURRENT version is not newer than the selected fallback".to_string(),
        ));
    }
    Ok(verified_working_set_bytes(&rejected, &bytes)?.paths)
}

fn restore_checkpoint(
    runtime_root: &Path,
    loaded: &LoadedCheckpointV1,
    predecessor_paths: &BTreeSet<String>,
) -> Result<(), BrainRuntimeError> {
    let authoritative_paths = loaded
        .manifest
        .file_inventory
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect::<BTreeSet<_>>();
    let mut managed_paths = BTreeSet::from(["graph_snapshot.json", "ingest_roots.json"]);
    managed_paths.extend(
        OPTIONAL_SESSION_SIDECARS
            .iter()
            .map(|(_, relative_path)| *relative_path),
    );
    managed_paths.extend(predecessor_paths.iter().map(String::as_str));
    for relative_path in managed_paths {
        validate_relative_path(relative_path)?;
        if !authoritative_paths.contains(relative_path) {
            remove_regular_working_file_if_present(runtime_root, relative_path)?;
        }
    }
    for file in &loaded.manifest.file_inventory {
        validate_relative_path(&file.relative_path)?;
        let bytes = loaded.read_file(&file.logical_name)?;
        atomic_restore_file(runtime_root, &file.relative_path, &bytes)?;
    }
    Ok(())
}

/// Materialize the already-committed immutable generation into legacy working
/// files. CURRENT remains the sole commit point: this function is called only
/// after ACK + authority readback. The actor retains the union of paths it has
/// ever owned so a later explicit ABSENT decision removes a stale dynamic file
/// instead of silently leaving it reachable.
fn project_checkpoint_working_set(
    runtime_root: &Path,
    loaded: &LoadedCheckpointV1,
    managed_working_paths: &BTreeSet<String>,
) -> Result<(), BrainRuntimeError> {
    let mut effective_managed_paths = managed_working_paths.clone();
    effective_managed_paths.extend(verified_working_set_paths(loaded)?);
    let present = loaded
        .manifest
        .file_inventory
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect::<BTreeSet<_>>();
    for relative_path in &effective_managed_paths {
        validate_relative_path(relative_path)?;
        if !present.contains(relative_path.as_str()) {
            remove_regular_working_file_if_present(runtime_root, relative_path)?;
        }
    }
    for file in &loaded.manifest.file_inventory {
        validate_relative_path(&file.relative_path)?;
        let bytes = loaded
            .read_file(&file.logical_name)
            .map_err(BrainRuntimeError::Checkpoint)?;
        atomic_restore_file(runtime_root, &file.relative_path, &bytes)?;
    }
    Ok(())
}

fn digest_for_logical(files: &[CheckpointFileInputV1], logical_name: &str) -> String {
    files
        .iter()
        .find(|file| file.logical_name == logical_name)
        .map(|file| sha256_bytes(&file.bytes))
        .unwrap_or_default()
}

fn candidate_present_inputs(candidate: &SessionCheckpointCandidate) -> Vec<CheckpointFileInputV1> {
    candidate
        .files
        .iter()
        .filter_map(|file| match &file.presence {
            CheckpointCandidatePresence::Present(bytes) => Some(CheckpointFileInputV1 {
                logical_name: file.logical_name.clone(),
                relative_path: file.relative_path.clone(),
                schema_id: file.schema_id.clone(),
                schema_version: file.schema_version.clone(),
                bytes: bytes.clone(),
            }),
            CheckpointCandidatePresence::Absent => None,
        })
        .collect()
}

fn build_working_set_input(
    candidate: &SessionCheckpointCandidate,
    previously_managed: &BTreeSet<String>,
) -> Result<CheckpointFileInputV1, BrainRuntimeError> {
    if candidate
        .files
        .iter()
        .any(|file| file.relative_path == WORKING_SET_RELATIVE_PATH)
    {
        return Err(BrainRuntimeError::Persistence(format!(
            "candidate path '{}' collides with actor working-set metadata",
            WORKING_SET_RELATIVE_PATH
        )));
    }
    let by_path = candidate
        .files
        .iter()
        .map(|file| (file.relative_path.as_str(), file))
        .collect::<BTreeMap<_, _>>();
    if by_path.len() != candidate.files.len() {
        return Err(BrainRuntimeError::Persistence(
            "candidate working set contains duplicate relative paths".to_string(),
        ));
    }
    let mut paths = previously_managed.clone();
    paths.remove(WORKING_SET_RELATIVE_PATH);
    paths.extend(
        candidate
            .files
            .iter()
            .map(|file| file.relative_path.clone()),
    );
    let mut entries = Vec::with_capacity(paths.len());
    for relative_path in paths {
        validate_relative_path(&relative_path)?;
        let presence = match by_path.get(relative_path.as_str()) {
            Some(file) => match &file.presence {
                CheckpointCandidatePresence::Present(bytes) => {
                    CheckpointWorkingSetPresenceV1::Present {
                        logical_name: file.logical_name.clone(),
                        content_digest: sha256_bytes(bytes),
                    }
                }
                CheckpointCandidatePresence::Absent => CheckpointWorkingSetPresenceV1::Absent,
            },
            None => CheckpointWorkingSetPresenceV1::Absent,
        };
        entries.push(CheckpointWorkingSetEntryV1 {
            relative_path,
            presence,
        });
    }
    let envelope = CheckpointWorkingSetV1 {
        schema: WORKING_SET_SCHEMA.to_string(),
        candidate_state_digest: candidate.state_digest.clone(),
        entries,
    };
    let bytes = serde_json::to_vec(&envelope)
        .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
    Ok(CheckpointFileInputV1 {
        logical_name: WORKING_SET_LOGICAL_NAME.to_string(),
        relative_path: WORKING_SET_RELATIVE_PATH.to_string(),
        schema_id: WORKING_SET_SCHEMA.to_string(),
        schema_version: "1".to_string(),
        bytes,
    })
}

struct VerifiedWorkingSetV1 {
    paths: BTreeSet<String>,
    candidate_state_digest: Option<String>,
}

fn verified_working_set(
    loaded: &LoadedCheckpointV1,
) -> Result<VerifiedWorkingSetV1, BrainRuntimeError> {
    let bytes = match loaded.read_file(WORKING_SET_LOGICAL_NAME) {
        Ok(bytes) => bytes,
        Err(CheckpointError::UnknownLogicalFile(name)) if name == WORKING_SET_LOGICAL_NAME => {
            return Ok(VerifiedWorkingSetV1 {
                paths: loaded
                    .manifest
                    .file_inventory
                    .iter()
                    .map(|file| file.relative_path.clone())
                    .collect(),
                // Legacy checkpoints predate complete candidate digests. They
                // remain loadable through their manifest inventory, but never
                // gain a false exact-rebuild proof.
                candidate_state_digest: None,
            });
        }
        Err(error) => return Err(BrainRuntimeError::Checkpoint(error)),
    };
    verified_working_set_bytes(&loaded.manifest, &bytes)
}

fn verified_working_set_bytes(
    manifest: &CheckpointManifestV1,
    bytes: &[u8],
) -> Result<VerifiedWorkingSetV1, BrainRuntimeError> {
    let working_set: CheckpointWorkingSetV1 = serde_json::from_slice(bytes)
        .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
    if working_set.schema != WORKING_SET_SCHEMA
        || working_set.candidate_state_digest.len() != 64
        || !working_set
            .candidate_state_digest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(BrainRuntimeError::Persistence(
            "checkpoint working-set metadata has an invalid schema or candidate digest".to_string(),
        ));
    }
    let control = manifest
        .file_inventory
        .iter()
        .find(|file| file.logical_name == WORKING_SET_LOGICAL_NAME)
        .ok_or_else(|| {
            BrainRuntimeError::Persistence(
                "checkpoint working-set bytes are not declared by the manifest".to_string(),
            )
        })?;
    if control.relative_path != WORKING_SET_RELATIVE_PATH
        || control.schema_id != WORKING_SET_SCHEMA
        || control.schema_version != "1"
    {
        return Err(BrainRuntimeError::Persistence(
            "checkpoint working-set manifest binding is invalid".to_string(),
        ));
    }

    let mut paths = BTreeSet::from([WORKING_SET_RELATIVE_PATH.to_string()]);
    let mut previous: Option<&str> = None;
    for entry in &working_set.entries {
        validate_relative_path(&entry.relative_path)?;
        if entry.relative_path == WORKING_SET_RELATIVE_PATH
            || previous.is_some_and(|value| value >= entry.relative_path.as_str())
        {
            return Err(BrainRuntimeError::Persistence(
                "checkpoint working-set paths are duplicated, unsorted, or recursive".to_string(),
            ));
        }
        previous = Some(&entry.relative_path);
        let manifest_file = manifest
            .file_inventory
            .iter()
            .find(|file| file.relative_path == entry.relative_path);
        match (&entry.presence, manifest_file) {
            (
                CheckpointWorkingSetPresenceV1::Present {
                    logical_name,
                    content_digest,
                },
                Some(file),
            ) if file.logical_name == *logical_name && file.content_digest == *content_digest => {}
            (CheckpointWorkingSetPresenceV1::Absent, None) => {}
            _ => {
                return Err(BrainRuntimeError::Persistence(format!(
                    "checkpoint working-set presence does not match manifest for '{}'",
                    entry.relative_path
                )))
            }
        }
        paths.insert(entry.relative_path.clone());
    }
    for file in &manifest.file_inventory {
        if file.logical_name != WORKING_SET_LOGICAL_NAME && !paths.contains(&file.relative_path) {
            return Err(BrainRuntimeError::Persistence(format!(
                "checkpoint manifest path '{}' is omitted from working-set metadata",
                file.relative_path
            )));
        }
    }
    Ok(VerifiedWorkingSetV1 {
        paths,
        candidate_state_digest: Some(working_set.candidate_state_digest),
    })
}

fn verified_working_set_paths(
    loaded: &LoadedCheckpointV1,
) -> Result<BTreeSet<String>, BrainRuntimeError> {
    verified_working_set(loaded).map(|working_set| working_set.paths)
}

fn checkpoint_candidate_snapshot(
    state: &mut SessionState,
) -> Result<SessionCheckpointCandidate, BrainRuntimeError> {
    let stage = state
        .begin_checkpoint_staging()
        .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
    let candidate = match catch_unwind(AssertUnwindSafe(|| state.checkpoint_candidate(&stage))) {
        Ok(result) => result.map_err(|error| BrainRuntimeError::Persistence(error.to_string())),
        Err(payload) => Err(BrainRuntimeError::Worker(format!(
            "candidate snapshot panicked: {}",
            panic_payload_detail(payload)
        ))),
    };
    let close = match catch_unwind(AssertUnwindSafe(|| state.finish_checkpoint_staging(stage))) {
        Ok(result) => result.map_err(|error| BrainRuntimeError::Persistence(error.to_string())),
        Err(payload) => Err(BrainRuntimeError::Worker(format!(
            "candidate snapshot stage cleanup panicked: {}",
            panic_payload_detail(payload)
        ))),
    };
    match (candidate, close) {
        (Ok(candidate), Ok(_)) => Ok(candidate),
        (Err(error), Ok(_)) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(candidate_error), Err(close_error)) => Err(BrainRuntimeError::Persistence(format!(
            "{candidate_error}; candidate snapshot stage cleanup also failed: {close_error}"
        ))),
    }
}

fn inventory_matches(manifest: &CheckpointManifestV1, files: &[CheckpointFileInputV1]) -> bool {
    if manifest.file_inventory.len() != files.len() {
        return false;
    }
    files.iter().all(|input| {
        manifest.file_inventory.iter().any(|file| {
            file.logical_name == input.logical_name
                && file.relative_path == input.relative_path
                && file.schema_id == input.schema_id
                && file.schema_version == input.schema_version
                && file.content_digest == sha256_bytes(&input.bytes)
                && file.byte_len == input.bytes.len() as u64
        })
    })
}

fn atomic_restore_file(
    runtime_root: &Path,
    relative_path: &str,
    bytes: &[u8],
) -> Result<(), BrainRuntimeError> {
    validate_relative_path(relative_path)?;
    let root_metadata = fs::symlink_metadata(runtime_root).map_err(|error| {
        BrainRuntimeError::Persistence(format!(
            "inspect restore runtime root '{}': {error}",
            runtime_root.display()
        ))
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(BrainRuntimeError::Persistence(format!(
            "restore runtime root '{}' is not a real directory",
            runtime_root.display()
        )));
    }
    let path = runtime_root.join(relative_path);
    let parent = path.parent().ok_or_else(|| {
        BrainRuntimeError::Persistence(format!("restore path '{}' has no parent", path.display()))
    })?;
    prepare_restore_parent(runtime_root, Path::new(relative_path))?;
    let (mut temporary, mut file) = create_restore_temporary(parent, &path)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
    drop(file);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(BrainRuntimeError::Persistence(format!(
                "restore refuses symlink target '{}'",
                path.display()
            )));
        }
        Ok(metadata) if metadata.is_dir() => {
            let mut entries = fs::read_dir(&path)
                .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
            if entries.next().is_some() {
                return Err(BrainRuntimeError::Persistence(format!(
                    "restore refuses non-empty directory target '{}'",
                    path.display()
                )));
            }
            fs::remove_dir(&path)
                .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
        }
        Ok(metadata) if !metadata.is_file() => {
            return Err(BrainRuntimeError::Persistence(format!(
                "restore refuses non-regular target '{}'",
                path.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(BrainRuntimeError::Persistence(error.to_string()));
        }
    }
    fs::rename(temporary.path(), &path)
        .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
    temporary.disarm();
    sync_directory(parent).map_err(|error| BrainRuntimeError::Persistence(error.to_string()))
}

fn prepare_restore_parent(
    runtime_root: &Path,
    relative_path: &Path,
) -> Result<(), BrainRuntimeError> {
    let Some(relative_parent) = relative_path.parent() else {
        return Ok(());
    };
    let mut current = runtime_root.to_path_buf();
    for component in relative_parent.components() {
        let Component::Normal(name) = component else {
            return Err(BrainRuntimeError::Persistence(format!(
                "restore parent '{}' is not a strict relative path",
                relative_path.display()
            )));
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(BrainRuntimeError::Persistence(format!(
                    "restore parent component '{}' is not a real directory",
                    current.display()
                )))
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current)
                    .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
                let parent = current.parent().ok_or_else(|| {
                    BrainRuntimeError::Persistence(format!(
                        "restore parent '{}' has no parent",
                        current.display()
                    ))
                })?;
                sync_directory(parent)
                    .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
            }
            Err(error) => return Err(BrainRuntimeError::Persistence(error.to_string())),
        }
    }
    Ok(())
}

static RESTORE_TEMP_NONCE: AtomicU64 = AtomicU64::new(0);

struct RestoreTemporary {
    path: PathBuf,
    armed: bool,
}

impl RestoreTemporary {
    fn path(&self) -> &Path {
        &self.path
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for RestoreTemporary {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn create_restore_temporary(
    parent: &Path,
    destination: &Path,
) -> Result<(RestoreTemporary, File), BrainRuntimeError> {
    let destination_digest = domain_digest(&destination.to_string_lossy());
    for _ in 0..32 {
        let nonce = RESTORE_TEMP_NONCE.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            ".restore-{}-{destination_digest}-{nonce}",
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        match options.open(&path) {
            Ok(file) => return Ok((RestoreTemporary { path, armed: true }, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(BrainRuntimeError::Persistence(error.to_string())),
        }
    }
    Err(BrainRuntimeError::Persistence(format!(
        "could not allocate a unique restore temporary beside '{}'",
        destination.display()
    )))
}

fn remove_regular_working_file_if_present(
    runtime_root: &Path,
    relative_path: &str,
) -> Result<(), BrainRuntimeError> {
    validate_relative_path(relative_path)?;
    if !restore_parent_chain_exists(runtime_root, Path::new(relative_path))? {
        return Ok(());
    }
    let path = runtime_root.join(relative_path);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(BrainRuntimeError::Persistence(format!(
                "rollback refuses non-regular working file '{}'",
                path.display()
            )))
        }
        Ok(_) => {
            fs::remove_file(&path)
                .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
            if let Some(parent) = path.parent() {
                sync_directory(parent)
                    .map_err(|error| BrainRuntimeError::Persistence(error.to_string()))?;
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(BrainRuntimeError::Persistence(error.to_string())),
    }
}

fn restore_parent_chain_exists(
    runtime_root: &Path,
    relative_path: &Path,
) -> Result<bool, BrainRuntimeError> {
    let root_metadata = fs::symlink_metadata(runtime_root).map_err(|error| {
        BrainRuntimeError::Persistence(format!(
            "inspect working runtime root '{}': {error}",
            runtime_root.display()
        ))
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(BrainRuntimeError::Persistence(format!(
            "working runtime root '{}' is not a real directory",
            runtime_root.display()
        )));
    }
    let Some(relative_parent) = relative_path.parent() else {
        return Ok(true);
    };
    let mut current = runtime_root.to_path_buf();
    for component in relative_parent.components() {
        let Component::Normal(name) = component else {
            return Err(BrainRuntimeError::Persistence(format!(
                "working parent '{}' is not a strict relative path",
                relative_path.display()
            )));
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(BrainRuntimeError::Persistence(format!(
                    "working parent component '{}' is not a real directory",
                    current.display()
                )))
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(BrainRuntimeError::Persistence(error.to_string())),
        }
    }
    Ok(true)
}

fn validate_relative_path(value: &str) -> Result<(), BrainRuntimeError> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(BrainRuntimeError::Persistence(format!(
            "checkpoint restore path refused: '{value}'"
        )));
    }
    Ok(())
}

/// Per-stage actor timing, opt-in via `M1ND_BRAIN_TIMING=1`. The read path is
/// the hottest code in the product and its cost is invisible from the outside:
/// one HTTP duration cannot tell a slow retrieval from a slow checkpoint. This
/// prints the actual boundary each turn crossed, so a regression is diagnosed
/// with numbers instead of a guess.
fn brain_timing_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("M1ND_BRAIN_TIMING")
            .map(|value| !matches!(value.as_str(), "" | "0" | "false"))
            .unwrap_or(false)
    })
}

fn log_brain_stage(stage: &str, started: Instant) {
    if brain_timing_enabled() {
        eprintln!(
            "[m1nd brain-timing] {stage} {:.3}s",
            started.elapsed().as_secs_f64()
        );
    }
}

fn session_generation(state: &SessionState) -> u64 {
    state
        .graph_generation
        .max(state.plasticity_generation)
        .max(state.cache_generation)
}

fn session_generation_tuple(state: &SessionState) -> (u64, u64, u64) {
    (
        state.graph_generation,
        state.plasticity_generation,
        state.cache_generation,
    )
}

fn now_unix_ms() -> Result<u64, &'static str> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before UNIX_EPOCH")?
        .as_millis()
        .try_into()
        .map_err(|_| "unix timestamp does not fit u64")
}

fn domain_digest(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"m1nd-brain-runtime-v1");
    hasher.update([0]);
    hasher.update(value.as_bytes());
    hex_lower(&hasher.finalize())
}

pub fn project_brain_id(canonical_root: &str) -> String {
    format!("project-brain-{}", domain_digest(canonical_root))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn short_id(value: &str) -> &str {
    let start = value.len().saturating_sub(12);
    &value[start..]
}

fn panic_payload_detail(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> std::io::Result<()> {
    // Windows refuses fsync on directory handles; write-through covers renames —
    // the same platform law the rest of the workspace already follows.
    Ok(())
}

/// Type-level reminder used by registry wrappers: preparation receives a
/// `RuntimeJobContext` and immutable snapshot, never `&mut SessionState`.
pub type BrainPrepare<P, S> =
    fn(RuntimeJobContext, BrainReadSnapshot<S>) -> Result<P, RuntimeJobFailure>;

#[cfg(test)]
mod tests {
    use super::*;

    fn add_consistent_test_node(
        state: &mut SessionState,
        id: &str,
        label: &str,
    ) -> Result<(), RuntimeJobFailure> {
        state
            .graph
            .write()
            .add_node(
                id,
                label,
                m1nd_core::types::NodeType::Concept,
                &[],
                0.0,
                0.0,
            )
            .map_err(|error| RuntimeJobFailure::new("sentinel_add_failed", error.to_string()))?;
        let (primary, orchestrator) = {
            let graph = state.graph.read();
            let primary = m1nd_core::temporal::CoChangeMatrix::bootstrap(
                &graph,
                m1nd_core::temporal::DEFAULT_MATRIX_BUDGET,
            )
            .map_err(|error| {
                RuntimeJobFailure::new("primary_temporal_rebuild_failed", error.to_string())
            })?;
            let orchestrator = m1nd_core::temporal::CoChangeMatrix::bootstrap(
                &graph,
                m1nd_core::temporal::DEFAULT_MATRIX_BUDGET,
            )
            .map_err(|error| {
                RuntimeJobFailure::new("orchestrator_temporal_rebuild_failed", error.to_string())
            })?;
            (primary, orchestrator)
        };
        state.temporal.co_change = primary;
        state.orchestrator.temporal.co_change = orchestrator;
        state.graph_generation = state.graph_generation.saturating_add(1);
        Ok(())
    }

    fn test_state(runtime_root: &Path) -> SessionState {
        test_state_at(runtime_root, runtime_root.join("graph_snapshot.json"))
    }

    fn test_state_at(runtime_root: &Path, graph_path: PathBuf) -> SessionState {
        crate::server::McpServer::new(crate::server::McpConfig {
            graph_source: graph_path,
            plasticity_state: runtime_root.join("plasticity_state.json"),
            runtime_dir: Some(runtime_root.to_path_buf()),
            registry_dir: Some(runtime_root.join("registry")),
            ..Default::default()
        })
        .expect("boot test brain")
        .into_session_state()
    }

    fn stage_dynamic_document_artifact(
        state: &mut SessionState,
        source_path: &str,
    ) -> Result<String, RuntimeJobFailure> {
        use m1nd_ingest::canonical::{
            CanonicalDocument, ConfidenceLevel, DocumentMetadata, SourceKind,
        };

        let document = CanonicalDocument {
            doc_id: format!("canon::{source_path}"),
            source_path: source_path.to_string(),
            source_kind: SourceKind::Markdown,
            detected_type: "markdown".to_string(),
            producer: "brain-runtime-fallback-test".to_string(),
            content_hash: domain_digest(source_path),
            title: "Fallback-owned dynamic artifact".to_string(),
            plain_text: "candidate-only document body".to_string(),
            metadata: DocumentMetadata::default(),
            sections: Vec::new(),
            tables: Vec::new(),
            links: Vec::new(),
            citations: Vec::new(),
            entities: Vec::new(),
            claims: Vec::new(),
            code_candidates: Vec::new(),
            confidence: ConfidenceLevel::Parsed,
            structured_origin: serde_json::json!({}),
        };
        let artifacts = crate::universal_docs::encode_canonical_artifacts(
            &state.runtime_root,
            &[document],
            "fallback-test",
        )
        .map_err(|error| RuntimeJobFailure::new("artifact_encode_failed", error.to_string()))?;
        state
            .document_artifacts
            .stage_replacement(&artifacts)
            .map_err(|error| RuntimeJobFailure::new("artifact_stage_failed", error.to_string()))?;
        crate::universal_docs::ensure_cache_root_in_ingest_roots(state);
        let canonical_path = artifacts
            .entries
            .first()
            .ok_or_else(|| RuntimeJobFailure::new("artifact_missing", "encoder returned no entry"))?
            .canonical_markdown_path
            .clone();
        for entry in artifacts.entries {
            state
                .document_cache
                .entries
                .insert(entry.source_path.clone(), entry);
        }
        state.cache_generation = state.cache_generation.saturating_add(1);
        Ok(canonical_path)
    }

    struct FailingCheckpointAuthority {
        fail: Arc<AtomicBool>,
    }

    impl BrainCheckpointAuthority for FailingCheckpointAuthority {
        fn snapshot_refs(
            &self,
            brain_id: &str,
        ) -> Result<CheckpointExternalAuthorityRefsV1, String> {
            if self.fail.load(Ordering::SeqCst) {
                Err("injected checkpoint authority failure".to_string())
            } else {
                UnboundBrainCheckpointAuthority.snapshot_refs(brain_id)
            }
        }

        fn validate_checkpoint(
            &self,
            manifest: &CheckpointManifestV1,
            external_authority_refs_digest: &str,
        ) -> Result<CheckpointAuthorityValidationReceiptV1, String> {
            if self.fail.load(Ordering::SeqCst) {
                Err("injected checkpoint authority failure".to_string())
            } else {
                UnboundBrainCheckpointAuthority
                    .validate_checkpoint(manifest, external_authority_refs_digest)
            }
        }
    }

    struct ValidationFailCheckpointAuthority {
        fail_validation: Arc<AtomicBool>,
    }

    struct RejectOneCheckpointAuthority {
        rejected: Arc<Mutex<Option<String>>>,
    }

    impl BrainCheckpointAuthority for ValidationFailCheckpointAuthority {
        fn snapshot_refs(
            &self,
            brain_id: &str,
        ) -> Result<CheckpointExternalAuthorityRefsV1, String> {
            UnboundBrainCheckpointAuthority.snapshot_refs(brain_id)
        }

        fn validate_checkpoint(
            &self,
            manifest: &CheckpointManifestV1,
            external_authority_refs_digest: &str,
        ) -> Result<CheckpointAuthorityValidationReceiptV1, String> {
            if self.fail_validation.load(Ordering::SeqCst) {
                Err("injected post-CURRENT authority validation failure".to_string())
            } else {
                UnboundBrainCheckpointAuthority
                    .validate_checkpoint(manifest, external_authority_refs_digest)
            }
        }
    }

    impl BrainCheckpointAuthority for RejectOneCheckpointAuthority {
        fn snapshot_refs(
            &self,
            brain_id: &str,
        ) -> Result<CheckpointExternalAuthorityRefsV1, String> {
            UnboundBrainCheckpointAuthority.snapshot_refs(brain_id)
        }

        fn validate_checkpoint(
            &self,
            manifest: &CheckpointManifestV1,
            external_authority_refs_digest: &str,
        ) -> Result<CheckpointAuthorityValidationReceiptV1, String> {
            if lock_unpoisoned(&self.rejected).as_deref() == Some(manifest.checkpoint_id.as_str()) {
                return Err("selected checkpoint is rejected by protected authority".to_string());
            }
            UnboundBrainCheckpointAuthority
                .validate_checkpoint(manifest, external_authority_refs_digest)
        }
    }

    struct BlockingSnapshotAuthority {
        calls: AtomicU64,
        entered: SyncSender<()>,
        release: Mutex<Receiver<()>>,
    }

    struct SwitchableCheckpointFault {
        point: crate::checkpoint_store::CheckpointFaultPoint,
        enabled: AtomicBool,
        remaining: AtomicU64,
    }

    struct SwitchableCheckpointPanic {
        point: crate::checkpoint_store::CheckpointFaultPoint,
        enabled: AtomicBool,
    }

    impl SwitchableCheckpointFault {
        fn one_shot(point: crate::checkpoint_store::CheckpointFaultPoint) -> Self {
            Self {
                point,
                enabled: AtomicBool::new(false),
                remaining: AtomicU64::new(1),
            }
        }

        fn persistent(point: crate::checkpoint_store::CheckpointFaultPoint) -> Self {
            Self {
                point,
                enabled: AtomicBool::new(false),
                remaining: AtomicU64::new(u64::MAX),
            }
        }
    }

    impl CheckpointFaultInjector for SwitchableCheckpointFault {
        fn check(
            &self,
            event: &crate::checkpoint_store::CheckpointFaultEvent,
        ) -> Result<(), crate::checkpoint_store::InjectedCheckpointFault> {
            if event.point != self.point || !self.enabled.load(Ordering::SeqCst) {
                return Ok(());
            }
            let mut remaining = self.remaining.load(Ordering::SeqCst);
            loop {
                if remaining == 0 {
                    return Ok(());
                }
                if remaining == u64::MAX {
                    break;
                }
                match self.remaining.compare_exchange(
                    remaining,
                    remaining - 1,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => break,
                    Err(observed) => remaining = observed,
                }
            }
            Err(crate::checkpoint_store::InjectedCheckpointFault::new(
                "actor_test_fault",
                format!("injected actor checkpoint fault at {:?}", event.point),
            ))
        }
    }

    impl CheckpointFaultInjector for SwitchableCheckpointPanic {
        fn check(
            &self,
            event: &crate::checkpoint_store::CheckpointFaultEvent,
        ) -> Result<(), crate::checkpoint_store::InjectedCheckpointFault> {
            if self.enabled.load(Ordering::SeqCst) && event.point == self.point {
                panic!("injected checkpoint adapter panic at {:?}", event.point);
            }
            Ok(())
        }
    }

    impl BrainCheckpointAuthority for BlockingSnapshotAuthority {
        fn snapshot_refs(
            &self,
            brain_id: &str,
        ) -> Result<CheckpointExternalAuthorityRefsV1, String> {
            if self.calls.fetch_add(1, Ordering::SeqCst) == 1 {
                self.entered
                    .send(())
                    .map_err(|error| format!("announce blocked checkpoint: {error}"))?;
                lock_unpoisoned(&self.release)
                    .recv()
                    .map_err(|error| format!("release blocked checkpoint: {error}"))?;
            }
            UnboundBrainCheckpointAuthority.snapshot_refs(brain_id)
        }

        fn validate_checkpoint(
            &self,
            manifest: &CheckpointManifestV1,
            external_authority_refs_digest: &str,
        ) -> Result<CheckpointAuthorityValidationReceiptV1, String> {
            UnboundBrainCheckpointAuthority
                .validate_checkpoint(manifest, external_authority_refs_digest)
        }
    }

    #[test]
    fn checkpoint_working_set_presence_round_trips_strictly() {
        let envelope = CheckpointWorkingSetV1 {
            schema: WORKING_SET_SCHEMA.to_string(),
            candidate_state_digest: "a".repeat(64),
            entries: vec![
                CheckpointWorkingSetEntryV1 {
                    relative_path: "graph_snapshot.json".to_string(),
                    presence: CheckpointWorkingSetPresenceV1::Present {
                        logical_name: "graph_snapshot".to_string(),
                        content_digest: "b".repeat(64),
                    },
                },
                CheckpointWorkingSetEntryV1 {
                    relative_path: "graph_snapshot.bin".to_string(),
                    presence: CheckpointWorkingSetPresenceV1::Absent,
                },
            ],
        };
        let encoded = serde_json::to_vec(&envelope).expect("encode working-set metadata");
        let decoded: CheckpointWorkingSetV1 =
            serde_json::from_slice(&encoded).expect("decode working-set metadata");
        assert_eq!(decoded.entries.len(), 2);
        assert!(matches!(
            &decoded.entries[0].presence,
            CheckpointWorkingSetPresenceV1::Present {
                logical_name,
                content_digest,
            } if logical_name == "graph_snapshot" && content_digest == &"b".repeat(64)
        ));
        assert!(matches!(
            decoded.entries[1].presence,
            CheckpointWorkingSetPresenceV1::Absent
        ));

        let mut tampered: serde_json::Value =
            serde_json::from_slice(&encoded).expect("working-set JSON value");
        tampered["entries"][0]["unexpected"] = serde_json::Value::Bool(true);
        assert!(serde_json::from_value::<CheckpointWorkingSetV1>(tampered).is_err());
    }

    #[test]
    fn actor_one_shot_faults_retry_to_an_exact_ack_at_every_store_boundary() {
        use crate::checkpoint_store::CheckpointFaultPoint::*;

        let points = [
            CreateStagingDirectory,
            CreateBlobDirectory,
            WriteBlob,
            FsyncBlob,
            FsyncBlobDirectory,
            WriteManifest,
            FsyncManifest,
            FsyncStagingDirectory,
            RenameCheckpointDirectory,
            FsyncCheckpointParent,
            WriteCurrent,
            FsyncCurrent,
            RenameCurrent,
            FsyncCurrentParent,
            ConfirmCurrent,
        ];
        for (index, point) in points.into_iter().enumerate() {
            let temporary = tempfile::tempdir().expect("temporary runtime");
            let runtime_root = temporary.path().join("runtime");
            let checkpoint_root = runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY);
            let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
            let injector = Arc::new(SwitchableCheckpointFault::one_shot(point));
            let faults: Arc<dyn CheckpointFaultInjector> = injector.clone();
            let actor = BrainActorHandle::start_with_faults(
                format!("one-shot-fault-{index}"),
                Arc::clone(&session),
                checkpoint_root.clone(),
                Arc::new(UnboundBrainCheckpointAuthority),
                2,
                None,
                faults,
            )
            .expect("start actor before arming fault");
            injector.enabled.store(true, Ordering::SeqCst);
            let (_, ack) = actor
                .try_execute_with_checkpoint_ack(|state| {
                    state.graph_generation = state.graph_generation.saturating_add(1);
                    Ok::<(), RuntimeJobFailure>(())
                })
                .unwrap_or_else(|error| panic!("one-shot {point:?} did not recover: {error}"));
            let current: crate::checkpoint_store::CheckpointCurrentV1 = serde_json::from_slice(
                &std::fs::read(checkpoint_root.join("CURRENT"))
                    .expect("read CURRENT after one-shot recovery"),
            )
            .expect("decode CURRENT after one-shot recovery");
            assert_eq!(ack.checkpoint_id, current.current_checkpoint_id);
            assert_eq!(actor.health_snapshot().status, "healthy");
            actor.stop().expect("stop one-shot actor");
        }
    }

    #[test]
    fn persistent_post_current_fault_reconciles_in_process_after_fault_clears() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let checkpoint_root = runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY);
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let injector = Arc::new(SwitchableCheckpointFault::persistent(
            crate::checkpoint_store::CheckpointFaultPoint::FsyncCurrentParent,
        ));
        let faults: Arc<dyn CheckpointFaultInjector> = injector.clone();
        let actor = BrainActorHandle::start_with_faults(
            "persistent-post-current-fault".to_string(),
            Arc::clone(&session),
            checkpoint_root.clone(),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
            faults,
        )
        .expect("start actor before arming persistent fault");
        let baseline = actor
            .health_snapshot()
            .current_checkpoint_id
            .expect("baseline CURRENT");
        injector.enabled.store(true, Ordering::SeqCst);
        let error = actor
            .try_execute_with_checkpoint_ack(|state| {
                state.graph_generation = state.graph_generation.saturating_add(1);
                Ok::<(), RuntimeJobFailure>(())
            })
            .expect_err("persistent post-CURRENT fault cannot yield an ACK");
        assert_eq!(error.code(), "brain_checkpoint_committed_unconfirmed");
        let current: crate::checkpoint_store::CheckpointCurrentV1 = serde_json::from_slice(
            &std::fs::read(checkpoint_root.join("CURRENT"))
                .expect("candidate CURRENT remains readable"),
        )
        .expect("decode candidate CURRENT");
        assert_ne!(current.current_checkpoint_id, baseline);
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while actor.health_snapshot().status != "reconciling"
            && std::time::Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(actor.health_snapshot().status, "reconciling");
        let refused_during_reconciliation = actor
            .try_read_snapshot(|state| Ok::<u64, RuntimeJobFailure>(state.graph_generation))
            .expect_err("health accepting=false must also close actor admission");
        assert_eq!(
            refused_during_reconciliation.code(),
            "brain_degraded_persistence"
        );
        assert!(session.try_lock().is_none());
        let quarantined_read = match session.read() {
            Ok(_) => panic!("quarantined session must not publish a read guard"),
            Err(error) => error,
        };
        assert_eq!(quarantined_read.code(), "brain_degraded_persistence");
        let stop_refusal = actor
            .stop()
            .expect_err("stop must not discard the only reconciliation owner");
        assert_eq!(stop_refusal.code(), "brain_degraded_persistence");
        assert!(session.is_actor_active());

        injector.enabled.store(false, Ordering::SeqCst);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while actor.health_snapshot().status == "reconciling"
            && std::time::Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(20));
        }
        let recovered = actor.health_snapshot();
        assert_eq!(recovered.status, "healthy");
        assert!(recovered.accepting);
        assert_eq!(
            recovered.current_checkpoint_id.as_deref(),
            Some(current.current_checkpoint_id.as_str())
        );
        assert!(session.try_lock().is_none());
        actor.stop().expect("stop reconciled actor");
    }

    #[test]
    fn dropping_last_handle_delegates_in_doubt_recovery_to_a_guardian() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let injector = Arc::new(SwitchableCheckpointFault::persistent(
            crate::checkpoint_store::CheckpointFaultPoint::FsyncCurrentParent,
        ));
        let faults: Arc<dyn CheckpointFaultInjector> = injector.clone();
        let actor = BrainActorHandle::start_with_faults(
            "drop-reconciliation-guardian".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
            faults,
        )
        .expect("start actor before arming persistent fault");
        injector.enabled.store(true, Ordering::SeqCst);
        actor
            .try_execute_with_checkpoint_ack(|state| {
                state.graph_generation = state.graph_generation.saturating_add(1);
                Ok::<(), RuntimeJobFailure>(())
            })
            .expect_err("persistent post-CURRENT fault cannot yield an ACK");
        assert_eq!(actor.health_snapshot().status, "reconciling");
        let health = Arc::clone(&actor.health);

        drop(actor);
        assert!(
            session.is_actor_active(),
            "last-handle Drop released the actor fence before reconciliation"
        );
        assert!(session.try_lock().is_none());
        injector.enabled.store(false, Ordering::SeqCst);

        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while session.is_actor_active() && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !session.is_actor_active(),
            "guardian did not stop and release the actor after recovery: {:?}",
            lock_unpoisoned(&health).last_persistence_error
        );
        assert!(session.try_lock().is_some());
        assert!(session.quarantine_detail().is_none());
    }

    #[test]
    fn reconciliation_adapter_panic_is_fenced_and_retried_in_process() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let checkpoint_root = runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY);
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let injector = Arc::new(SwitchableCheckpointPanic {
            point: crate::checkpoint_store::CheckpointFaultPoint::FsyncCurrentParent,
            enabled: AtomicBool::new(false),
        });
        let faults: Arc<dyn CheckpointFaultInjector> = injector.clone();
        let actor = BrainActorHandle::start_with_faults(
            "reconciliation-panic-fence".to_string(),
            Arc::clone(&session),
            checkpoint_root,
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
            faults,
        )
        .expect("start actor before enabling panic");

        injector.enabled.store(true, Ordering::SeqCst);
        let error = actor
            .try_execute_with_checkpoint_ack(|state| {
                state.graph_generation = state.graph_generation.saturating_add(1);
                Ok::<(), RuntimeJobFailure>(())
            })
            .expect_err("post-CURRENT adapter panic cannot yield an ACK");
        assert_eq!(error.code(), "brain_checkpoint_committed_unconfirmed");
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while actor.health_snapshot().status != "reconciling"
            && std::time::Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(actor.health_snapshot().status, "reconciling");
        // Leave the injected panic armed across several retry windows. If the
        // panic escapes run_actor, disabling it below cannot recover the cell.
        thread::sleep(Duration::from_millis(250));
        assert_eq!(actor.health_snapshot().status, "reconciling");
        assert!(session.try_lock().is_none());

        injector.enabled.store(false, Ordering::SeqCst);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while actor.health_snapshot().status == "reconciling"
            && std::time::Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(actor.health_snapshot().status, "healthy");
        assert!(session.try_lock().is_none());
        actor.stop().expect("stop panic-fenced reconciler");
    }

    #[test]
    fn bootstrap_post_current_fault_still_starts_the_autonomous_reconciler() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let checkpoint_root = runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY);
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let injector = Arc::new(SwitchableCheckpointFault::persistent(
            crate::checkpoint_store::CheckpointFaultPoint::FsyncCurrentParent,
        ));
        injector.enabled.store(true, Ordering::SeqCst);
        let faults: Arc<dyn CheckpointFaultInjector> = injector.clone();

        let actor = BrainActorHandle::start_with_faults(
            "bootstrap-post-current-fault".to_string(),
            Arc::clone(&session),
            checkpoint_root.clone(),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
            faults,
        )
        .expect("an in-doubt bootstrap must retain a live reconciler");
        let reconciling = actor.health_snapshot();
        assert_eq!(reconciling.status, "reconciling");
        assert!(!reconciling.accepting);
        assert!(reconciling.in_doubt_checkpoint_id.is_some());
        assert!(session.try_lock().is_none());

        injector.enabled.store(false, Ordering::SeqCst);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while actor.health_snapshot().status == "reconciling"
            && std::time::Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(20));
        }
        let recovered = actor.health_snapshot();
        assert_eq!(recovered.status, "healthy");
        assert!(recovered.accepting);
        let current: crate::checkpoint_store::CheckpointCurrentV1 = serde_json::from_slice(
            &std::fs::read(checkpoint_root.join("CURRENT"))
                .expect("read reconciled bootstrap CURRENT"),
        )
        .expect("decode reconciled bootstrap CURRENT");
        assert_eq!(
            recovered.current_checkpoint_id.as_deref(),
            Some(current.current_checkpoint_id.as_str())
        );
        assert!(session.try_lock().is_none());
        actor.stop().expect("stop bootstrap reconciler");
    }

    #[test]
    fn bootstrap_pre_current_failure_republishes_state_for_an_agent_retry() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let checkpoint_root = runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY);
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let injector = Arc::new(SwitchableCheckpointFault::persistent(
            crate::checkpoint_store::CheckpointFaultPoint::CreateStagingDirectory,
        ));
        injector.enabled.store(true, Ordering::SeqCst);
        let faults: Arc<dyn CheckpointFaultInjector> = injector.clone();

        let error = match BrainActorHandle::start_with_faults(
            "bootstrap-pre-current-fault".to_string(),
            Arc::clone(&session),
            checkpoint_root.clone(),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
            Arc::clone(&faults),
        ) {
            Ok(actor) => {
                let _ = actor.stop();
                panic!("a persistent PRE-CURRENT fault must refuse startup")
            }
            Err(error) => error,
        };
        assert_eq!(error.code(), "checkpoint_fault_injected");
        assert!(
            !checkpoint_root.join("CURRENT").exists(),
            "PRE-CURRENT failure must not publish a pointer"
        );
        assert!(session.quarantine_detail().is_none());
        assert!(session.try_lock().is_some());
        assert!(!session.is_actor_active());

        injector.enabled.store(false, Ordering::SeqCst);
        let actor = BrainActorHandle::start_with_faults(
            "bootstrap-pre-current-fault".to_string(),
            Arc::clone(&session),
            checkpoint_root,
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
            faults,
        )
        .expect("the same cell must accept an autonomous agent retry");
        assert_eq!(actor.health_snapshot().status, "healthy");
        actor.stop().expect("stop retried bootstrap actor");
    }

    #[test]
    fn bootstrap_authority_snapshot_failure_closes_the_early_stage_for_retry() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let fail = Arc::new(AtomicBool::new(true));
        let authority: Arc<dyn BrainCheckpointAuthority> = Arc::new(FailingCheckpointAuthority {
            fail: Arc::clone(&fail),
        });

        let error = match BrainActorHandle::start(
            "bootstrap-authority-snapshot-fault".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::clone(&authority),
            2,
            None,
        ) {
            Ok(actor) => {
                let _ = actor.stop();
                panic!("authority snapshot refusal must fail bootstrap")
            }
            Err(error) => error,
        };
        assert_eq!(error.code(), "brain_persistence_failed");
        assert!(session.quarantine_detail().is_none());
        assert!(session.try_lock().is_some());
        assert!(!session.is_actor_active());

        fail.store(false, Ordering::SeqCst);
        let actor = BrainActorHandle::start(
            "bootstrap-authority-snapshot-fault".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            authority,
            2,
            None,
        )
        .expect("same cell retries after authority snapshot recovers");
        actor.stop().expect("stop authority-retried actor");
    }

    #[test]
    fn guarded_mutation_returns_the_exact_current_checkpoint_ack() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let checkpoint_root = runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY);
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "ack-proof-brain".to_string(),
            Arc::clone(&session),
            checkpoint_root.clone(),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");

        let (observed_generation, ack) = actor
            .try_execute_with_checkpoint_ack(|state| {
                state.graph_generation = state.graph_generation.saturating_add(1);
                Ok::<u64, RuntimeJobFailure>(state.graph_generation)
            })
            .expect("mutation and checkpoint ACK");
        let current: crate::checkpoint_store::CheckpointCurrentV1 = serde_json::from_slice(
            &std::fs::read(checkpoint_root.join("CURRENT")).expect("CURRENT bytes"),
        )
        .expect("CURRENT pointer");
        let health = actor.health_snapshot();
        assert_eq!(ack.brain_id, "ack-proof-brain");
        assert!(ack.generation >= observed_generation);
        assert!(ack.revision > 0);
        assert_eq!(ack.checkpoint_id, current.current_checkpoint_id);
        assert_eq!(ack.current_pointer_digest, current.pointer_digest);
        assert_eq!(
            health.current_checkpoint_id.as_deref(),
            Some(ack.checkpoint_id.as_str())
        );
        assert_eq!(health.version.generation, ack.generation);
        assert_eq!(health.version.revision, ack.revision);
        actor.stop().expect("stop actor");
    }

    #[test]
    fn duplicate_actor_start_refuses_without_quarantining_live_session() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let checkpoint_root = runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY);
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "single-writer-brain".to_string(),
            Arc::clone(&session),
            checkpoint_root.clone(),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start first actor");

        let duplicate = match BrainActorHandle::start(
            "single-writer-brain".to_string(),
            Arc::clone(&session),
            checkpoint_root,
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        ) {
            Ok(actor) => {
                let _ = actor.stop();
                panic!("second actor must not acquire the checkpoint writer")
            }
            Err(error) => error,
        };
        assert_eq!(duplicate.code(), "brain_actor_already_active");
        assert!(session.is_actor_active());
        let fenced = match session.lock_mut_before_actor() {
            Ok(_) => panic!("raw mutation must be fenced while actor owns the brain"),
            Err(error) => error,
        };
        assert_eq!(fenced.code(), "brain_actor_already_active");
        assert!(session.quarantine_detail().is_none());
        assert!(session.try_lock().is_none());
        let read_fenced = match session.read() {
            Ok(_) => panic!("raw read guard must be fenced while actor owns the brain"),
            Err(error) => error,
        };
        assert_eq!(read_fenced.code(), "brain_actor_already_active");
        actor
            .try_read_snapshot(|state| Ok::<u64, RuntimeJobFailure>(state.graph_generation))
            .expect("first actor remains usable after duplicate-start refusal");
        actor.stop().expect("stop first actor");
        assert!(!session.is_actor_active());
        assert!(session.lock_mut_before_actor().is_ok());
    }

    #[test]
    fn actor_claim_drains_preexisting_guard_before_raising_fence() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let checkpoint_root = runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY);
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let mut pre_actor = session
            .lock_mut_before_actor()
            .expect("acquire legacy boot guard before actor claim");
        let start_session = Arc::clone(&session);
        let (attempt_tx, attempt_rx) = mpsc::sync_channel(1);
        let starter = thread::spawn(move || {
            attempt_tx.send(()).expect("announce actor claim attempt");
            BrainActorHandle::start(
                "guard-handoff-brain".to_string(),
                start_session,
                checkpoint_root,
                Arc::new(UnboundBrainCheckpointAuthority),
                2,
                None,
            )
        });
        attempt_rx.recv().expect("actor claim thread entered");
        thread::sleep(Duration::from_millis(50));
        assert!(
            !session.is_actor_active(),
            "actor fence rose while a preexisting mutable guard remained live"
        );
        pre_actor.graph_generation = pre_actor.graph_generation.saturating_add(1);
        let guarded_generation = pre_actor.graph_generation;
        drop(pre_actor);

        let actor = starter
            .join()
            .expect("join actor starter")
            .expect("actor starts after guard drains");
        let observed = actor
            .try_read_snapshot(|state| Ok::<u64, RuntimeJobFailure>(state.graph_generation))
            .expect("read actor baseline after linearized handoff");
        assert_eq!(observed.value, guarded_generation);
        actor.stop().expect("stop handoff actor");
    }

    #[test]
    fn pause_linearizes_after_check_and_enqueue_and_checkpoint_drains_fifo() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "pause-admission-linearization".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            4,
            None,
        )
        .expect("start actor");
        let generation_before = actor.health_snapshot().version.generation;

        // Stop one producer exactly after it checked accepting=true and before
        // its try_send. This is the historical pause/admission race window.
        let (entered_tx, entered_rx) = mpsc::sync_channel(0);
        let (release_tx, release_rx) = mpsc::sync_channel(0);
        *lock_unpoisoned(&actor.admission_race_hook) = Some(AdmissionRaceHook {
            entered: entered_tx,
            release: release_rx,
        });
        let producer_actor = Arc::clone(&actor);
        let producer = thread::spawn(move || {
            producer_actor.try_execute(false, |state| {
                let generation = session_generation(state).saturating_add(1);
                state.graph_generation = generation;
                Ok::<u64, RuntimeJobFailure>(generation)
            })
        });
        entered_rx
            .recv()
            .expect("producer reached check/send race barrier");
        assert!(
            matches!(
                actor.admission.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            ),
            "accepting check/send window did not retain the admission gate"
        );

        // The pause probe fires immediately before pause attempts the same
        // gate. With the producer still pinned inside admission, pause cannot
        // report completion.
        let (pause_entered_tx, pause_entered_rx) = mpsc::sync_channel(0);
        *lock_unpoisoned(&actor.pause_entry_probe) = Some(pause_entered_tx);
        let (pause_done_tx, pause_done_rx) = mpsc::sync_channel(1);
        let pause_actor = Arc::clone(&actor);
        let pauser = thread::spawn(move || {
            let _ = pause_done_tx.send(pause_actor.pause());
        });
        pause_entered_rx
            .recv()
            .expect("pause reached admission gate");
        assert!(
            matches!(
                pause_done_rx.recv_timeout(Duration::from_millis(100)),
                Err(RecvTimeoutError::Timeout)
            ),
            "pause returned while a checked admission had not enqueued"
        );

        release_tx
            .send(())
            .expect("release producer to complete FIFO admission");
        pause_done_rx
            .recv()
            .expect("pause completion reply")
            .expect("pause after producer admission");
        pauser.join().expect("join pauser");

        // This checkpoint is sent only after pause returns. FIFO therefore
        // places it behind the command admitted before the pause, even if that
        // command has not finished executing yet.
        let paused_ack = actor
            .checkpoint_while_paused()
            .expect("checkpoint drains every pre-pause command");
        let admitted_generation = producer
            .join()
            .expect("join admitted producer")
            .expect("execute admitted command");
        assert!(admitted_generation > generation_before);
        assert!(
            paused_ack.generation >= admitted_generation,
            "paused checkpoint {} preceded admitted generation {admitted_generation}",
            paused_ack.generation
        );
        actor
            .stop_while_paused()
            .expect("stop actor while admission remains closed");
    }

    #[test]
    fn actor_start_detaches_graph_capabilities_cloned_before_cell_ownership() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let state = test_state(&runtime_root);
        let escaped_before_cell = Arc::clone(&state.graph);
        let session = Arc::new(BrainSessionCell::new(state));
        let actor = BrainActorHandle::start(
            "pre-cell-arc-detach".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor and detach pre-cell graph");

        escaped_before_cell
            .write()
            .add_node(
                "pre-cell-escape::sentinel",
                "pre-cell escaped sentinel",
                m1nd_core::types::NodeType::Concept,
                &[],
                0.0,
                0.0,
            )
            .expect("mutate detached pre-cell graph");
        let visible = actor
            .try_read_snapshot(|state| {
                Ok::<bool, RuntimeJobFailure>(
                    state
                        .graph
                        .read()
                        .resolve_id("pre-cell-escape::sentinel")
                        .is_some(),
                )
            })
            .expect("read actor-owned graph");
        assert!(!visible.value);
        actor.stop().expect("stop detached actor");
    }

    #[test]
    fn failed_atomic_restore_cleans_unique_temporary() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let target = runtime_root.join("nested").join("state.json");
        std::fs::create_dir_all(&target).expect("create conflicting target directory");
        std::fs::write(target.join("sentinel"), b"occupied")
            .expect("make target directory non-empty");

        let error = atomic_restore_file(&runtime_root, "nested/state.json", b"candidate")
            .expect_err("restore must refuse a non-empty directory target");
        assert_eq!(error.code(), "brain_persistence_failed");
        let leftovers = std::fs::read_dir(runtime_root.join("nested"))
            .expect("list restore parent")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with(".restore-"))
            .collect::<Vec<_>>();
        assert!(
            leftovers.is_empty(),
            "orphan restore temporaries: {leftovers:?}"
        );
    }

    #[test]
    fn staged_persist_does_not_publish_working_postimage_before_current() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let checkpoint_root = runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY);
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let (entered_tx, entered_rx) = mpsc::sync_channel(0);
        let (release_tx, release_rx) = mpsc::sync_channel(0);
        let actor = BrainActorHandle::start(
            "candidate-first-brain".to_string(),
            Arc::clone(&session),
            checkpoint_root,
            Arc::new(BlockingSnapshotAuthority {
                calls: AtomicU64::new(0),
                entered: entered_tx,
                release: Mutex::new(release_rx),
            }),
            2,
            None,
        )
        .expect("start actor");
        let command_actor = Arc::clone(&actor);
        let command = thread::spawn(move || {
            command_actor.try_execute_with_checkpoint_ack(|state| {
                add_consistent_test_node(
                    state,
                    "candidate-first::sentinel",
                    "candidate-first sentinel",
                )?;
                state
                    .persist()
                    .map_err(|error| RuntimeJobFailure::new("persist_failed", error.to_string()))?;
                Ok::<(), RuntimeJobFailure>(())
            })
        });

        entered_rx
            .recv_timeout(Duration::from_secs(30))
            .expect("candidate reached pre-CURRENT authority boundary");
        let before_current =
            m1nd_core::snapshot::load_graph(&runtime_root.join("graph_snapshot.json"))
                .expect("load canonical preimage while candidate is blocked");
        assert!(
            before_current
                .resolve_id("candidate-first::sentinel")
                .is_none(),
            "legacy working file exposed an uncommitted postimage"
        );
        release_tx.send(()).expect("release candidate commit");
        command
            .join()
            .expect("join candidate command")
            .expect("candidate checkpoint ACK");
        let after_current =
            m1nd_core::snapshot::load_graph(&runtime_root.join("graph_snapshot.json"))
                .expect("load post-commit projection");
        assert!(
            after_current
                .resolve_id("candidate-first::sentinel")
                .is_some(),
            "committed candidate was not projected after CURRENT"
        );
        actor.stop().expect("stop actor");
    }

    #[test]
    fn binary_derived_effect_checkpoint_digest_restarts_from_sealed_absence() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let graph_path = runtime_root.join("graph_snapshot.json");
        let binary_path = graph_path.with_extension("bin");
        let session = Arc::new(BrainSessionCell::new(test_state_at(
            &runtime_root,
            graph_path,
        )));
        let actor = BrainActorHandle::start(
            "binary-derived-restart-brain".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");

        let (_, ack) = actor
            .try_execute_with_checkpoint_ack(|state| {
                state.graph_generation = state.graph_generation.saturating_add(1);
                state.persist_binary_snapshot().map_err(|error| {
                    RuntimeJobFailure::new("binary_snapshot_failed", error.to_string())
                })?;
                Ok::<(), RuntimeJobFailure>(())
            })
            .expect("commit derived binary effect");
        assert!(binary_path.is_file(), "post-CURRENT effect was not applied");
        actor.stop().expect("stop actor before strict restart");

        let recovery = recover_checkpoint_for_boot(
            &runtime_root,
            "binary-derived-restart-brain",
            &UnboundBrainCheckpointAuthority,
        )
        .expect("strict restart validates reconstructible candidate digest")
        .expect("CURRENT exists");
        assert_eq!(recovery.manifest.checkpoint_id, ack.checkpoint_id);
        assert_eq!(
            recovery.receipt.disposition,
            CheckpointLoadDisposition::ExactCurrent
        );
        assert!(
            !binary_path.exists(),
            "derived BIN is sealed ABSENT and must not masquerade as authoritative state"
        );
    }

    #[test]
    fn recovery_removes_custom_binary_path_sealed_absent_in_working_set() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let graph_path = runtime_root.join("custom").join("brain.json");
        let binary_path = graph_path.with_extension("bin");
        let session = Arc::new(BrainSessionCell::new(test_state_at(
            &runtime_root,
            graph_path,
        )));
        let actor = BrainActorHandle::start(
            "custom-bin-recovery-brain".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");

        actor
            .try_execute_with_checkpoint_ack(|state| {
                state.graph_generation = state.graph_generation.saturating_add(1);
                state.persist_binary_snapshot().map_err(|error| {
                    RuntimeJobFailure::new("binary_snapshot_failed", error.to_string())
                })?;
                Ok::<(), RuntimeJobFailure>(())
            })
            .expect("commit typed binary effect");
        assert!(
            binary_path.is_file(),
            "post-commit BIN effect was not applied"
        );

        actor
            .try_execute_with_checkpoint_ack(|state| {
                state.graph_generation = state.graph_generation.saturating_add(1);
                Ok::<(), RuntimeJobFailure>(())
            })
            .expect("commit generation with BIN explicitly absent");
        assert!(
            !binary_path.exists(),
            "next explicit ABSENT projection did not remove custom BIN"
        );
        actor.stop().expect("stop actor");

        std::fs::write(&binary_path, b"stale crash residue")
            .expect("recreate stale custom BIN after actor stop");
        assert!(binary_path.exists());
        let recovery = recover_checkpoint_for_boot(
            &runtime_root,
            "custom-bin-recovery-brain",
            &UnboundBrainCheckpointAuthority,
        )
        .expect("recover CURRENT")
        .expect("CURRENT exists");
        assert_eq!(
            recovery.receipt.disposition,
            CheckpointLoadDisposition::ExactCurrent
        );
        assert!(
            !binary_path.exists(),
            "sealed ABSENT custom path survived recovery"
        );
    }

    #[test]
    fn degraded_fallback_removes_paths_introduced_only_by_rejected_current() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let checkpoint_root = runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY);
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "fallback-c-only-path-brain".to_string(),
            Arc::clone(&session),
            checkpoint_root.clone(),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor and publish fallback B");
        let artifact_path = actor
            .try_execute_with_checkpoint_ack(|state| {
                stage_dynamic_document_artifact(state, "docs/fallback-c-only.md")
            })
            .expect("publish successor C with a dynamic artifact")
            .0;
        actor.stop().expect("stop actor before boot recovery");
        assert!(Path::new(&artifact_path).is_file());

        let store = CheckpointStore::open(&checkpoint_root).expect("inspect current C");
        let current = store
            .load_current(&AuthorityValidatorAdapter(&UnboundBrainCheckpointAuthority))
            .expect("load intact current C");
        let current_id = current.manifest.checkpoint_id.clone();
        let corrupt_target = current
            .manifest
            .file_inventory
            .iter()
            .find(|file| file.logical_name == GRAPH_SNAPSHOT_LOGICAL_NAME)
            .map(|file| current.directory().join(&file.blob_path))
            .expect("non-working-set C blob");
        // LoadedCheckpoint retains the parent/root lease so blob reads remain
        // bound to the same namespace. Release that read capability before the
        // independent boot-recovery writer opens the store.
        drop(current);
        drop(store);
        std::fs::write(&corrupt_target, b"corrupt candidate graph")
            .expect("make C unusable without corrupting its working-set");

        let recovery = recover_checkpoint_for_boot(
            &runtime_root,
            "fallback-c-only-path-brain",
            &UnboundBrainCheckpointAuthority,
        )
        .expect("authenticate C inventory and recover B")
        .expect("CURRENT exists");
        assert_eq!(
            recovery.receipt.disposition,
            CheckpointLoadDisposition::DegradedFallback
        );
        assert_eq!(
            recovery
                .receipt
                .fallback_receipt
                .as_ref()
                .map(|receipt| receipt.requested_checkpoint_id.as_str()),
            Some(current_id.as_str())
        );
        assert!(
            !Path::new(&artifact_path).exists(),
            "C-only artifact survived restoration of fallback B"
        );
    }

    #[test]
    fn degraded_fallback_refuses_cleanup_when_rejected_working_set_is_corrupt() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let checkpoint_root = runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY);
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "fallback-untrusted-cleanup-brain".to_string(),
            Arc::clone(&session),
            checkpoint_root.clone(),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor and publish fallback B");
        let artifact_path = actor
            .try_execute_with_checkpoint_ack(|state| {
                stage_dynamic_document_artifact(state, "docs/untrusted-c-only.md")
            })
            .expect("publish successor C with a dynamic artifact")
            .0;
        actor.stop().expect("stop actor before boot recovery");

        let store = CheckpointStore::open(&checkpoint_root).expect("inspect current C");
        let current = store
            .load_current(&AuthorityValidatorAdapter(&UnboundBrainCheckpointAuthority))
            .expect("load intact current C");
        let working_set_path = current
            .manifest
            .file_inventory
            .iter()
            .find(|file| file.logical_name == WORKING_SET_LOGICAL_NAME)
            .map(|file| current.directory().join(&file.blob_path))
            .expect("C working-set blob");
        drop(store);
        std::fs::write(&working_set_path, b"untrusted working-set").expect("corrupt C working-set");

        let error = recover_checkpoint_for_boot(
            &runtime_root,
            "fallback-untrusted-cleanup-brain",
            &UnboundBrainCheckpointAuthority,
        )
        .expect_err("untrusted C inventory must fail boot closed");
        assert!(matches!(error, BrainRuntimeError::Checkpoint(_)));
        assert!(
            Path::new(&artifact_path).exists(),
            "recovery deleted a path without an authenticated C working-set"
        );
    }

    #[test]
    fn degraded_fallback_refuses_cleanup_when_rejected_current_lacks_authority() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let checkpoint_root = runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY);
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let rejected = Arc::new(Mutex::new(None));
        let authority: Arc<dyn BrainCheckpointAuthority> = Arc::new(RejectOneCheckpointAuthority {
            rejected: Arc::clone(&rejected),
        });
        let actor = BrainActorHandle::start(
            "fallback-authority-refusal-brain".to_string(),
            Arc::clone(&session),
            checkpoint_root,
            Arc::clone(&authority),
            2,
            None,
        )
        .expect("start actor and publish fallback B");
        let (artifact_path, candidate_ack) = actor
            .try_execute_with_checkpoint_ack(|state| {
                stage_dynamic_document_artifact(state, "docs/authority-rejected-c-only.md")
            })
            .expect("publish successor C with a dynamic artifact");
        actor.stop().expect("stop actor before boot recovery");
        *lock_unpoisoned(&rejected) = Some(candidate_ack.checkpoint_id);

        let error = recover_checkpoint_for_boot(
            &runtime_root,
            "fallback-authority-refusal-brain",
            authority.as_ref(),
        )
        .expect_err("unauthorized C inventory must fail boot closed");
        assert!(matches!(error, BrainRuntimeError::Checkpoint(_)));
        assert!(
            Path::new(&artifact_path).exists(),
            "recovery deleted a C-only path after C authority was rejected"
        );
    }

    #[test]
    fn read_callback_interior_mutation_is_rolled_back() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "read-mutation-brain".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");

        let error = actor
            .try_read_snapshot(|state| {
                state
                    .graph
                    .write()
                    .add_node(
                        "read-escape::sentinel",
                        "read escape sentinel",
                        m1nd_core::types::NodeType::Concept,
                        &[],
                        0.0,
                        0.0,
                    )
                    .map_err(|error| {
                        RuntimeJobFailure::new("sentinel_add_failed", error.to_string())
                    })?;
                Ok::<(), RuntimeJobFailure>(())
            })
            .expect_err("read callback mutation must be refused");
        assert_eq!(error.code(), "brain_worker_failed", "{error}");
        assert!(!actor.health_snapshot().degraded_persistence);
        let visible = actor
            .try_read_snapshot(|state| {
                Ok::<bool, RuntimeJobFailure>(
                    state
                        .graph
                        .read()
                        .resolve_id("read-escape::sentinel")
                        .is_some(),
                )
            })
            .expect("read actor-owned graph after rollback");
        assert!(!visible.value);
        actor.stop().expect("stop actor");
    }

    #[test]
    fn escaped_read_graph_arc_is_detached_from_actor_owner() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "arc-detach-brain".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");
        let escaped: Arc<Mutex<Option<m1nd_core::graph::SharedGraph>>> = Arc::new(Mutex::new(None));
        let escaped_from_callback = Arc::clone(&escaped);
        actor
            .try_read_snapshot(move |state| {
                *lock_unpoisoned(&escaped_from_callback) = Some(Arc::clone(&state.graph));
                Ok::<(), RuntimeJobFailure>(())
            })
            .expect("detached read snapshot");
        let old_graph = lock_unpoisoned(&escaped)
            .take()
            .expect("callback escaped its old graph Arc");
        old_graph
            .write()
            .add_node(
                "escaped-arc::sentinel",
                "escaped arc sentinel",
                m1nd_core::types::NodeType::Concept,
                &[],
                0.0,
                0.0,
            )
            .expect("mutate detached old graph");
        let visible = actor
            .try_read_snapshot(|state| {
                Ok::<bool, RuntimeJobFailure>(
                    state
                        .graph
                        .read()
                        .resolve_id("escaped-arc::sentinel")
                        .is_some(),
                )
            })
            .expect("read live actor graph");
        assert!(
            !visible.value,
            "escaped Arc still mutated actor-owned graph"
        );
        actor.stop().expect("stop actor");
    }

    #[test]
    fn execute_false_detects_durable_mutation_and_checkpoints_it() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "misclassified-command-brain".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");
        let baseline = actor
            .health_snapshot()
            .current_checkpoint_id
            .expect("baseline CURRENT");
        actor
            .try_execute(false, |state| {
                add_consistent_test_node(
                    state,
                    "misclassified::sentinel",
                    "misclassified sentinel",
                )?;
                Ok::<(), RuntimeJobFailure>(())
            })
            .expect("actor detects and seals durable mutation");
        let health = actor.health_snapshot();
        assert_ne!(
            health.current_checkpoint_id.as_deref(),
            Some(baseline.as_str())
        );
        let working = m1nd_core::snapshot::load_graph(&runtime_root.join("graph_snapshot.json"))
            .expect("load committed projection");
        assert!(working.resolve_id("misclassified::sentinel").is_some());
        actor.stop().expect("stop actor");
    }

    #[test]
    fn mutation_refusal_without_postimage_does_not_disable_actor() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "safe-refusal-brain".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");
        let error = actor
            .try_execute_with_checkpoint_ack::<(), _>(|_state| {
                Err(RuntimeJobFailure::new(
                    "invalid_request",
                    "request refused before mutation",
                ))
            })
            .expect_err("invalid request is a typed refusal");
        assert_eq!(error.code(), "brain_snapshot_read_failed");
        assert!(!actor.health_snapshot().degraded_persistence);
        actor
            .try_execute_with_checkpoint_ack(|state| {
                state.graph_generation = state.graph_generation.saturating_add(1);
                Ok::<(), RuntimeJobFailure>(())
            })
            .expect("later valid mutation remains admitted");
        actor.stop().expect("stop actor");
    }

    /// A graph verb legitimately rewrites non-structural node/edge numbers on
    /// every call (plasticity Step 8). That drift must NOT publish a durable
    /// checkpoint of the whole brain: before this was fixed, every single warm
    /// `seek` wrote a ~113 MB checkpoint, the store grew by one checkpoint per
    /// read, and a warm read cost seconds instead of milliseconds.
    #[test]
    fn execute_false_with_only_non_structural_drift_publishes_no_checkpoint() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "read-drift-brain".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");
        actor
            .try_execute(true, |state| {
                add_consistent_test_node(state, "drift::anchor", "drift anchor")
            })
            .expect("seed a node to drift");
        let baseline = actor
            .health_snapshot()
            .current_checkpoint_id
            .expect("CURRENT after the seeding mutation");

        for _ in 0..3 {
            actor
                .try_execute(false, |state| {
                    let mut graph = state.graph.write();
                    let previous = graph.nodes.change_frequency[0].get();
                    graph.nodes.change_frequency[0] =
                        m1nd_core::types::FiniteF32::new(previous + 0.125);
                    Ok::<(), RuntimeJobFailure>(())
                })
                .expect("non-structural drift on a read turn is admitted");
        }

        assert_eq!(
            actor.health_snapshot().current_checkpoint_id.as_deref(),
            Some(baseline.as_str()),
            "read turns must not publish a durable checkpoint for learning drift"
        );
        actor.stop().expect("stop actor");
    }

    /// The freshness-by-traffic daemon tick calls `persist_daemon_state` on
    /// nearly every dispatch, and under the old rule that single staged flag
    /// published a whole-brain checkpoint per read. The request is now debounced:
    /// it accumulates and flushes once, not once per call.
    #[test]
    fn read_turn_staged_persist_is_debounced_instead_of_published_per_call() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "read-debounce-brain".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");
        let debounce = actor
            .try_read_snapshot(|state| Ok::<u32, RuntimeJobFailure>(state.auto_persist_interval))
            .expect("read the debounce interval")
            .value
            .max(1);
        let baseline = actor
            .health_snapshot()
            .current_checkpoint_id
            .expect("baseline CURRENT");

        for turn in 1..debounce {
            actor
                .try_execute(false, |state| {
                    state
                        .persist_daemon_state()
                        .map_err(|error| RuntimeJobFailure::new("daemon_state", error.to_string()))
                })
                .expect("read turn with a routine staged persist");
            assert_eq!(
                actor.health_snapshot().current_checkpoint_id.as_deref(),
                Some(baseline.as_str()),
                "turn {turn} must not publish its own checkpoint"
            );
        }

        actor
            .try_execute(false, |state| {
                state
                    .persist_daemon_state()
                    .map_err(|error| RuntimeJobFailure::new("daemon_state", error.to_string()))
            })
            .expect("the debounced turn");
        assert_ne!(
            actor.health_snapshot().current_checkpoint_id.as_deref(),
            Some(baseline.as_str()),
            "the accumulated drift must be flushed once the debounce is due"
        );
        actor.stop().expect("stop actor");
    }

    fn single_node_antibody_pattern() -> crate::protocol::layers::AntibodyPatternInput {
        crate::protocol::layers::AntibodyPatternInput {
            nodes: vec![crate::protocol::layers::PatternNodeInput {
                role: "suspect".into(),
                node_type: Some("concept".into()),
                required_tags: Vec::new(),
                label_contains: Some("antibody-durability".into()),
            }],
            edges: Vec::new(),
            negative_edges: Vec::new(),
        }
    }

    /// `antibody_create` writes the `antibodies` checkpoint sidecar and NOTHING
    /// else: no node, no edge, no session generation. The actor's O(1) witness is
    /// blind to that by construction, so the verb's durability rests entirely on
    /// being classified a mutation. Before that classification existed the ack said
    /// "created" while the antibody lived only in memory — a `kill -9` lost it, or
    /// resurrected one that had been deleted.
    ///
    /// The `mutating` flag here comes from the REAL classifier, not a literal, so
    /// dropping `antibody_create` from `READ_ONLY_DENIED_TOOLS` fails this test.
    #[test]
    fn antibody_create_is_durable_on_the_turn_it_is_acked() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "antibody-durability-brain".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");
        let baseline = actor
            .health_snapshot()
            .current_checkpoint_id
            .expect("baseline CURRENT");

        let mutating = crate::server::read_only_denied(
            "antibody_create",
            &serde_json::json!({ "action": "create" }),
        );
        assert!(
            mutating,
            "antibody_create writes the antibodies sidecar, so the classifier must call it a mutation"
        );
        let created = actor
            .try_execute(mutating, |state| {
                crate::layer_handlers::handle_antibody_create(
                    state,
                    crate::protocol::layers::AntibodyCreateInput {
                        agent_id: "durability-test".into(),
                        action: "create".into(),
                        antibody_id: None,
                        name: Some("durability probe".into()),
                        description: Some("pins sidecar durability".into()),
                        severity: "warning".into(),
                        pattern: Some(single_node_antibody_pattern()),
                    },
                )
                .map_err(|error| RuntimeJobFailure::new("antibody_create", error.to_string()))
            })
            .expect("antibody_create is admitted");
        assert!(
            created.get("antibody_id").is_some(),
            "the ack claims creation"
        );

        let after_create = actor
            .health_snapshot()
            .current_checkpoint_id
            .expect("CURRENT after antibody_create");
        assert_ne!(
            after_create, baseline,
            "an acked antibody_create must be sealed in a durable checkpoint on its own turn"
        );
        let durable = actor
            .try_read_snapshot(|state| Ok::<usize, RuntimeJobFailure>(state.antibodies.len()))
            .expect("read the antibody store")
            .value;
        assert_eq!(durable, 1, "the created antibody is in the live store");
        actor.stop().expect("stop actor");
    }

    /// The isolation fence must survive the branch that publishes nothing. Every
    /// turn used to rebind through `post_callback_candidate`; the deferring read
    /// branch skips that, so without an explicit fence an `Arc` a callback escaped
    /// keeps aliasing the live actor graph BETWEEN turns. This is the execute-read
    /// mirror of `escaped_read_graph_arc_is_detached_from_actor_owner`.
    #[test]
    fn escaped_execute_read_graph_arc_is_detached_from_actor_owner() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "execute-arc-detach-brain".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");
        let escaped: Arc<Mutex<Option<m1nd_core::graph::SharedGraph>>> = Arc::new(Mutex::new(None));
        let escaped_from_callback = Arc::clone(&escaped);
        actor
            .try_execute(false, move |state| {
                *lock_unpoisoned(&escaped_from_callback) = Some(Arc::clone(&state.graph));
                Ok::<(), RuntimeJobFailure>(())
            })
            .expect("a read turn that escapes its graph Arc is admitted");
        let old_graph = lock_unpoisoned(&escaped)
            .take()
            .expect("callback escaped its old graph Arc");
        old_graph
            .write()
            .add_node(
                "execute-escaped-arc::sentinel",
                "execute escaped arc sentinel",
                m1nd_core::types::NodeType::Concept,
                &[],
                0.0,
                0.0,
            )
            .expect("mutate detached old graph");
        let visible = actor
            .try_read_snapshot(|state| {
                Ok::<bool, RuntimeJobFailure>(
                    state
                        .graph
                        .read()
                        .resolve_id("execute-escaped-arc::sentinel")
                        .is_some(),
                )
            })
            .expect("read live actor graph")
            .value;
        assert!(
            !visible,
            "an Arc escaped by a read-classified execute still mutated actor-owned graph"
        );
        actor.stop().expect("stop actor");
    }

    /// The fence above must stay O(1) on the hot path. An honest read leaves no
    /// second owner of the graph Arc, so nothing can alias the actor and the deep
    /// clone is skipped entirely — the graph the next turn sees is the SAME
    /// allocation. This is what keeps the deferring branch cheap; if it starts
    /// rebinding unconditionally, a warm read pays a full encode/decode again.
    #[test]
    fn execute_read_without_an_escaped_arc_does_not_rebind_the_graph() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "execute-no-rebind-brain".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");
        fn graph_identity(state: &SessionState) -> usize {
            Arc::as_ptr(&state.graph) as usize
        }
        let before = actor
            .try_execute(false, |state| {
                Ok::<usize, RuntimeJobFailure>(graph_identity(state))
            })
            .expect("first read turn");
        let after = actor
            .try_execute(false, |state| {
                Ok::<usize, RuntimeJobFailure>(graph_identity(state))
            })
            .expect("second read turn");
        assert_eq!(
            before, after,
            "a read turn with no escaped Arc must not pay for a graph deep clone"
        );
        actor.stop().expect("stop actor");
    }

    /// A queued post-CURRENT effect is the one persist reason that cannot be
    /// deferred: only the checkpoint path drains it and `finish_checkpoint_staging`
    /// refuses to close a stage while one is outstanding. Folding it into the
    /// deferrable persist request would send the turn into `quarantine_failed_state`
    /// instead of publishing.
    #[test]
    fn execute_read_with_an_unresolved_staged_effect_publishes_instead_of_deferring() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "staged-effect-brain".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");
        let baseline = actor
            .health_snapshot()
            .current_checkpoint_id
            .expect("baseline CURRENT");

        actor
            .try_execute(false, |state| {
                state
                    .persist_binary_snapshot()
                    .map(|_| ())
                    .map_err(|error| {
                        RuntimeJobFailure::new("persist_binary_snapshot", error.to_string())
                    })
            })
            .expect("a read turn that queues a derived export must still close cleanly");

        assert_ne!(
            actor.health_snapshot().current_checkpoint_id.as_deref(),
            Some(baseline.as_str()),
            "a queued post-CURRENT effect must publish on its own turn, never wait for the debounce"
        );
        assert!(
            !actor.health_snapshot().degraded_persistence,
            "the turn must publish, not quarantine"
        );
        actor.stop().expect("stop actor");
    }

    /// The perf contract of this change, held as an assertion instead of a lab
    /// note: a long run of honest reads publishes NOTHING, and the mutation that
    /// follows publishes EXACTLY ONE checkpoint.
    #[test]
    fn a_long_run_of_reads_publishes_nothing_and_one_mutation_publishes_exactly_one() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "read-volume-brain".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");
        actor
            .try_execute(true, |state| {
                add_consistent_test_node(state, "volume::anchor", "volume anchor")
            })
            .expect("seed the brain");
        let baseline = actor
            .health_snapshot()
            .current_checkpoint_id
            .expect("CURRENT after the seeding mutation");

        for _ in 0..70 {
            actor
                .try_execute(false, |state| {
                    let mut graph = state.graph.write();
                    let previous = graph.nodes.change_frequency[0].get();
                    graph.nodes.change_frequency[0] =
                        m1nd_core::types::FiniteF32::new(previous + 0.001);
                    Ok::<(), RuntimeJobFailure>(())
                })
                .expect("warm read turn");
        }
        assert_eq!(
            actor.health_snapshot().current_checkpoint_id.as_deref(),
            Some(baseline.as_str()),
            "70 reads must publish 0 checkpoints"
        );

        actor
            .try_execute(true, |state| {
                add_consistent_test_node(state, "volume::real", "a real mutation")
            })
            .expect("the real mutation");
        let after = actor
            .health_snapshot()
            .current_checkpoint_id
            .expect("CURRENT after the mutation");
        assert_ne!(after, baseline, "a real mutation publishes");

        for _ in 0..10 {
            actor
                .try_execute(false, |state| {
                    Ok::<u64, RuntimeJobFailure>(state.graph.read().generation.0)
                })
                .expect("read after the mutation");
        }
        assert_eq!(
            actor.health_snapshot().current_checkpoint_id.as_deref(),
            Some(after.as_str()),
            "the mutation published exactly one checkpoint and the reads after it published none"
        );
        actor.stop().expect("stop actor");
    }

    /// What the strict `read_snapshot` fence actually promises, pinned so the doc
    /// cannot drift from it again.
    ///
    /// It refuses a change to durable STRUCTURE (nodes, edges) and to the session
    /// generations. It does NOT refuse an interior column write — a tag, a
    /// provenance row, an edge weight — because answering that question needs a
    /// content digest, and this path runs on EVERY transport call. The digest it
    /// used to take was also unsound here: plasticity legitimately rewrites weights
    /// on every read, so honest reads were refused as mutation attempts.
    ///
    /// The compensating control is classification, not the witness: every verb that
    /// writes a graph tag or provenance column (`xray_retag`, `xray_paint`,
    /// `xray_apply`, `ingest`, `apply`) is in `READ_ONLY_DENIED_TOOLS`, so its
    /// durability comes from being a declared mutation.
    #[test]
    fn read_snapshot_fence_refuses_structure_and_admits_interior_column_drift() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "read-fence-contract-brain".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");
        actor
            .try_execute(true, |state| {
                add_consistent_test_node(state, "fence::anchor", "fence anchor")
            })
            .expect("seed a node to tag");

        let refused = actor
            .try_read_snapshot(|state| {
                state
                    .graph
                    .write()
                    .add_node(
                        "fence::structural",
                        "structural change",
                        m1nd_core::types::NodeType::Concept,
                        &[],
                        0.0,
                        0.0,
                    )
                    .map(|_| ())
                    .map_err(|error| RuntimeJobFailure::new("add_node", error.to_string()))
            })
            .expect_err("a structural change under the strict fence is refused");
        assert_eq!(refused.code(), "brain_worker_failed");

        let tagged = actor
            .try_read_snapshot(|state| {
                let mut graph = state.graph.write();
                let node = graph.resolve_id("fence::anchor").ok_or_else(|| {
                    RuntimeJobFailure::new("resolve", "anchor missing".to_string())
                })?;
                Ok::<usize, RuntimeJobFailure>(graph.add_node_tags(node, &["fence:interior"]))
            })
            .expect("an interior column write is ADMITTED — the fence is structural, by design");
        assert_eq!(tagged.value, 1);

        for verb in ["xray_retag", "xray_paint", "xray_apply", "ingest", "apply"] {
            assert!(
                crate::server::read_only_denied(verb, &serde_json::json!({})),
                "{verb} writes graph columns the witness cannot see, so classification must carry its durability"
            );
        }
        actor.stop().expect("stop actor");
    }

    #[test]
    fn guarded_mutation_pre_current_failure_rolls_back_and_remains_retryable() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let checkpoint_root = runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY);
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let fail = Arc::new(AtomicBool::new(false));
        let actor = BrainActorHandle::start(
            "ack-failure-brain".to_string(),
            Arc::clone(&session),
            checkpoint_root.clone(),
            Arc::new(FailingCheckpointAuthority {
                fail: Arc::clone(&fail),
            }),
            2,
            None,
        )
        .expect("start actor");
        let baseline = actor
            .health_snapshot()
            .current_checkpoint_id
            .expect("writable actor starts from a baseline CURRENT");

        fail.store(true, Ordering::SeqCst);
        let result = actor.try_execute_with_checkpoint_ack(|state| {
            add_consistent_test_node(state, "unacked::sentinel", "unacked sentinel")?;
            Ok::<(), RuntimeJobFailure>(())
        });
        assert!(result.is_err(), "checkpoint failure must not yield an ACK");
        let health = actor.health_snapshot();
        assert_eq!(health.status, "healthy");
        assert!(!health.degraded_persistence);
        assert_eq!(
            health.current_checkpoint_id.as_deref(),
            Some(baseline.as_str())
        );
        assert!(health.last_persistence_error.is_none());
        assert!(
            session.try_lock().is_none(),
            "actor-active preimage must remain reachable only through its queue"
        );
        assert!(session.quarantine_detail().is_none());
        let sentinel_visible = actor
            .try_read_snapshot(|state| {
                Ok::<bool, RuntimeJobFailure>(
                    state.graph.read().resolve_id("unacked::sentinel").is_some(),
                )
            })
            .expect("read authoritative preimage through actor");
        assert!(!sentinel_visible.value);
        let working = m1nd_core::snapshot::load_graph(&runtime_root.join("graph_snapshot.json"))
            .expect("pre-CURRENT failure restores canonical graph immediately");
        assert!(working.resolve_id("unacked::sentinel").is_none());
        fail.store(false, Ordering::SeqCst);
        let (_, retry_ack) = actor
            .try_execute_with_checkpoint_ack(|state| {
                state.graph_generation = state.graph_generation.saturating_add(1);
                Ok::<(), RuntimeJobFailure>(())
            })
            .expect("agent retry succeeds after the PRE-CURRENT fault clears");
        assert_ne!(retry_ack.checkpoint_id, baseline);
        actor.stop().expect("stop recovered actor");
    }

    #[test]
    fn guarded_mutation_partial_callback_error_rolls_back_without_sticky_degradation() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let checkpoint_root = runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY);
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "ack-callback-refusal-brain".to_string(),
            Arc::clone(&session),
            checkpoint_root.clone(),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");

        let baseline = actor
            .health_snapshot()
            .current_checkpoint_id
            .expect("baseline CURRENT");

        let result = actor.try_execute_with_checkpoint_ack::<(), _>(|state| {
            state
                .graph
                .write()
                .add_node(
                    "partial-error::sentinel",
                    "partial error sentinel",
                    m1nd_core::types::NodeType::Concept,
                    &[],
                    0.0,
                    0.0,
                )
                .map_err(|error| {
                    RuntimeJobFailure::new("sentinel_add_failed", error.to_string())
                })?;
            Err(RuntimeJobFailure::new(
                "injected_callback_refusal",
                "callback refused after partial mutation",
            ))
        });
        assert!(result.is_err());
        let current: crate::checkpoint_store::CheckpointCurrentV1 = serde_json::from_slice(
            &std::fs::read(checkpoint_root.join("CURRENT")).expect("CURRENT bytes"),
        )
        .expect("CURRENT pointer");
        assert_eq!(current.current_checkpoint_id, baseline);
        let health = actor.health_snapshot();
        assert!(!health.degraded_persistence);
        assert_eq!(
            health.current_checkpoint_id.as_deref(),
            Some(baseline.as_str())
        );
        assert!(health.in_doubt_checkpoint_id.is_none());
        assert!(session.try_lock().is_none());
        let sentinel_visible = actor
            .try_read_snapshot(|state| {
                Ok::<bool, RuntimeJobFailure>(
                    state
                        .graph
                        .read()
                        .resolve_id("partial-error::sentinel")
                        .is_some(),
                )
            })
            .expect("read callback rollback through actor");
        assert!(
            !sentinel_visible.value,
            "callback partial postimage must not survive in memory"
        );
        let working = m1nd_core::snapshot::load_graph(&runtime_root.join("graph_snapshot.json"))
            .expect("load rolled-back canonical graph");
        assert!(working.resolve_id("partial-error::sentinel").is_none());
        actor.stop().expect("stop actor");
    }

    #[test]
    fn failed_callback_rollback_keeps_an_autonomous_recovery_owner() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let graph_path = runtime_root.join("graph_snapshot.json");
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "callback-rollback-recovery-brain".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");
        let baseline = actor
            .health_snapshot()
            .current_checkpoint_id
            .expect("baseline CURRENT");
        let poisoned_graph_path = graph_path.clone();

        let error = actor
            .try_execute_with_checkpoint_ack::<(), _>(move |state| {
                add_consistent_test_node(
                    state,
                    "rollback-failure::sentinel",
                    "rollback failure sentinel",
                )?;
                std::fs::remove_file(&poisoned_graph_path).map_err(|error| {
                    RuntimeJobFailure::new("poison_graph_remove_failed", error.to_string())
                })?;
                std::fs::create_dir(&poisoned_graph_path).map_err(|error| {
                    RuntimeJobFailure::new("poison_graph_create_failed", error.to_string())
                })?;
                std::fs::write(poisoned_graph_path.join("rollback-blocker"), b"blocked")
                    .map_err(|error| {
                        RuntimeJobFailure::new("poison_graph_write_failed", error.to_string())
                    })?;
                Err(RuntimeJobFailure::new(
                    "injected_callback_refusal",
                    "callback refused after making its authoritative rollback temporarily impossible",
                ))
            })
            .expect_err("a failed callback rollback must return no ACK");
        assert_eq!(error.code(), "brain_persistence_failed", "{error}");

        let health = actor.health_snapshot();
        assert_eq!(health.status, "reconciling");
        assert!(!health.accepting);
        assert!(health.degraded_persistence);
        assert!(session.try_lock().is_none());
        let stop_error = actor
            .stop()
            .expect_err("stop must retain the only autonomous rollback owner");
        assert_eq!(stop_error.code(), "brain_degraded_persistence");

        std::fs::remove_file(graph_path.join("rollback-blocker"))
            .expect("remove temporary rollback blocker");
        std::fs::remove_dir(&graph_path).expect("clear temporary rollback obstruction");
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while actor.health_snapshot().status == "reconciling"
            && std::time::Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(20));
        }
        let recovered = actor.health_snapshot();
        assert_eq!(recovered.status, "healthy");
        assert!(recovered.accepting);
        assert!(!recovered.degraded_persistence);
        assert_eq!(
            recovered.current_checkpoint_id.as_deref(),
            Some(baseline.as_str())
        );
        let sentinel_visible = actor
            .try_read_snapshot(|state| {
                Ok::<bool, RuntimeJobFailure>(
                    state
                        .graph
                        .read()
                        .resolve_id("rollback-failure::sentinel")
                        .is_some(),
                )
            })
            .expect("read recovered authoritative graph");
        assert!(!sentinel_visible.value);
        actor.stop().expect("stop recovered actor");
    }

    #[test]
    fn guarded_mutation_callback_panic_is_caught_and_rolled_back() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let actor = BrainActorHandle::start(
            "ack-callback-panic-brain".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");
        let generation_before = actor
            .try_read_snapshot(|state| Ok::<u64, RuntimeJobFailure>(state.graph_generation))
            .expect("read generation before panic")
            .value;

        let result = actor.try_execute_with_checkpoint_ack::<(), _>(|state| {
            state.graph_generation = state.graph_generation.saturating_add(1);
            panic!("injected mutation panic")
        });
        let error = result.expect_err("panic must become a typed actor error");
        assert_eq!(error.code(), "brain_worker_failed");
        assert!(!actor.health_snapshot().degraded_persistence);
        assert!(session.try_lock().is_none());
        let generation_after = actor
            .try_read_snapshot(|state| Ok::<u64, RuntimeJobFailure>(state.graph_generation))
            .expect("read generation after rollback")
            .value;
        assert_eq!(generation_after, generation_before);
        actor.stop().expect("stop actor");
    }

    #[test]
    fn post_current_authority_failure_is_in_doubt_not_rolled_back() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let checkpoint_root = runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY);
        let session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let fail_validation = Arc::new(AtomicBool::new(false));
        let actor = BrainActorHandle::start(
            "post-current-in-doubt-brain".to_string(),
            Arc::clone(&session),
            checkpoint_root.clone(),
            Arc::new(ValidationFailCheckpointAuthority {
                fail_validation: Arc::clone(&fail_validation),
            }),
            2,
            None,
        )
        .expect("start actor");
        let baseline = actor
            .health_snapshot()
            .current_checkpoint_id
            .expect("baseline CURRENT");

        fail_validation.store(true, Ordering::SeqCst);
        let result = actor.try_execute_with_checkpoint_ack(|state| {
            state.graph_generation = state.graph_generation.saturating_add(1);
            Ok::<(), RuntimeJobFailure>(())
        });
        let error = result.expect_err("authority readback failure must return no usable ACK");
        assert_eq!(
            error.code(),
            "brain_checkpoint_committed_unconfirmed",
            "unexpected error: {error}"
        );
        let current: crate::checkpoint_store::CheckpointCurrentV1 = serde_json::from_slice(
            &std::fs::read(checkpoint_root.join("CURRENT")).expect("CURRENT bytes"),
        )
        .expect("CURRENT pointer");
        assert_ne!(current.current_checkpoint_id, baseline);
        let health = actor.health_snapshot();
        assert_eq!(health.status, "reconciling");
        assert!(!health.accepting);
        assert!(health.degraded_persistence);
        assert_eq!(
            health.current_checkpoint_id.as_deref(),
            Some(current.current_checkpoint_id.as_str())
        );
        assert_eq!(
            health.in_doubt_checkpoint_id.as_deref(),
            Some(current.current_checkpoint_id.as_str())
        );
        assert!(session.try_lock().is_none());

        fail_validation.store(false, Ordering::SeqCst);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while actor.health_snapshot().status == "reconciling"
            && std::time::Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(20));
        }
        let recovered = actor.health_snapshot();
        assert_eq!(recovered.status, "healthy");
        assert!(recovered.accepting);
        assert!(!recovered.degraded_persistence);
        assert_eq!(
            recovered.current_checkpoint_id.as_deref(),
            Some(current.current_checkpoint_id.as_str())
        );
        assert!(recovered.in_doubt_checkpoint_id.is_none());
        assert!(session.try_lock().is_none());
        actor.stop().expect("stop reconciled actor");
    }

    /// A checkpoint CURRENT captured while the runtime graph was EMPTY must never
    /// silently revert a populated `graph_snapshot.json` that the owner boot has
    /// already loaded into the session it is about to serve.
    ///
    /// This is the stdio owner's exact shape: `McpServer::new` loads the runtime
    /// graph, then `McpServer::start` opens the bound actor with `recovery: None`
    /// and the actor reconciles CURRENT by itself. When the runtime files changed
    /// out of band since that CURRENT — which is precisely what the 1.5 legacy
    /// snapshot adoption does, writing the pre-1.5 graph into the runtime root
    /// before any actor exists — the restore reverts them and the server reports
    /// "Loaded graph snapshot: N nodes" followed by "Server ready. 0 nodes".
    /// RED on purpose: it currently observes 0 served nodes after a boot that
    /// loaded 1. Deleting this `#[ignore]` is the acceptance gate for the fix —
    /// which is a durability-semantics decision (may a runtime graph that a boot
    /// legitimately loaded outrank a stale CURRENT?), not a local repair.
    #[test]
    #[ignore = "reproduces the unfixed empty-graph revert; see the doc comment above"]
    fn actor_start_serves_the_graph_the_owner_boot_loaded() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let brain_id = "owner-empty-graph-revert".to_string();
        let graph_path = runtime_root.join("graph_snapshot.json");

        // A prior boot checkpointed an EMPTY runtime. This CURRENT is what every
        // later actor start reconciles against.
        let empty_session = Arc::new(BrainSessionCell::new(test_state(&runtime_root)));
        let empty_actor = BrainActorHandle::start(
            brain_id.clone(),
            Arc::clone(&empty_session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor over the empty runtime");
        empty_actor
            .checkpoint_and_ack()
            .expect("checkpoint the empty runtime");
        empty_actor.stop().expect("stop the empty-runtime actor");
        drop(empty_actor);
        // Release the first boot's instance lease so the second boot can own the
        // same runtime root, exactly as a restarted process would.
        drop(empty_session);

        // The empty boot also persisted a co-change sidecar bound to the empty
        // graph, beside which `McpServer::new` refuses to load ANY populated
        // graph (SchemaDrift). That refusal is a separate defect and not what
        // this test pins, so the sidecar is cleared to isolate the revert.
        std::fs::remove_file(runtime_root.join("temporal_state_v1.json")).ok();

        // A populated snapshot lands in the runtime root out of band.
        let mut populated = m1nd_core::graph::Graph::new();
        populated
            .add_node(
                "file::src/lib.rs",
                "lib.rs",
                m1nd_core::types::NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("seed the adopted snapshot");
        m1nd_core::snapshot::save_graph(&populated, &graph_path)
            .expect("write the adopted snapshot");

        // The owner boot loads it — this is the "Loaded graph snapshot" line.
        let booted = test_state(&runtime_root);
        assert_eq!(
            booted.graph.read().num_nodes(),
            1,
            "the owner boot must load the populated runtime snapshot"
        );

        // Starting the bound actor must not take that graph away.
        let session = Arc::new(BrainSessionCell::new(booted));
        let actor = BrainActorHandle::start(
            brain_id,
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start the bound actor over the populated runtime");
        let served = actor
            .try_read_snapshot(|state| Ok::<u32, RuntimeJobFailure>(state.graph.read().num_nodes()))
            .expect("read the served node count")
            .value;
        actor.stop().expect("stop the bound actor");

        assert_eq!(
            served, 1,
            "the served session must expose the nodes the boot loaded, not a stale empty checkpoint"
        );
    }

    #[test]
    fn actor_checkout_releases_storage_mutex_during_long_command() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let state = crate::server::McpServer::new(crate::server::McpConfig {
            graph_source: runtime_root.join("graph_snapshot.json"),
            plasticity_state: runtime_root.join("plasticity_state.json"),
            runtime_dir: Some(runtime_root.clone()),
            registry_dir: Some(runtime_root.join("registry")),
            ..Default::default()
        })
        .expect("boot test brain")
        .into_session_state();
        let session = Arc::new(BrainSessionCell::new(state));
        let actor = BrainActorHandle::start(
            "lock-proof-brain".to_string(),
            Arc::clone(&session),
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");

        let (entered_tx, entered_rx) = mpsc::sync_channel(0);
        let (release_tx, release_rx) = mpsc::sync_channel(0);
        let command_actor = Arc::clone(&actor);
        let command = thread::spawn(move || {
            command_actor.try_execute(false, move |_state| {
                entered_tx.send(()).expect("announce actor checkout");
                release_rx.recv().expect("release long command");
                Ok::<(), RuntimeJobFailure>(())
            })
        });

        entered_rx.recv().expect("actor entered command");
        assert!(
            session.storage_mutex_available(),
            "the actor must own SessionState without retaining the storage mutex"
        );
        assert!(
            session.try_lock().is_none(),
            "legacy readers must not observe SessionState while the actor owns it"
        );
        assert_eq!(
            actor.health_snapshot().status,
            "healthy",
            "health remains available without the session storage mutex"
        );

        release_tx.send(()).expect("release actor command");
        command
            .join()
            .expect("command thread")
            .expect("actor command succeeds");
        assert!(
            session.try_lock().is_none(),
            "raw SessionState guards remain fenced between actor turns"
        );
        actor.stop().expect("stop actor");
    }

    fn force_instance_heartbeat_to_zero(
        registry_root: &Path,
        instance_id: &str,
    ) -> (PathBuf, PathBuf) {
        let entry_path = registry_root
            .join("instances")
            .join(format!("{instance_id}.json"));
        let lease_path = std::fs::read_dir(registry_root.join("leases"))
            .expect("lease directory")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
            .expect("writer lease");
        for path in [&entry_path, &lease_path] {
            let mut entry: crate::instance_registry::InstanceRegistryEntry =
                serde_json::from_slice(&std::fs::read(path).expect("heartbeat record"))
                    .expect("heartbeat record shape");
            entry.last_heartbeat_ms = 0;
            std::fs::write(
                path,
                serde_json::to_vec_pretty(&entry).expect("heartbeat record bytes"),
            )
            .expect("zero heartbeat record");
        }
        (entry_path, lease_path)
    }

    fn wait_for_matching_heartbeat(entry_path: &Path, lease_path: &Path) -> u64 {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let entry = std::fs::read(entry_path).ok().and_then(|bytes| {
                serde_json::from_slice::<crate::instance_registry::InstanceRegistryEntry>(&bytes)
                    .ok()
            });
            let lease = std::fs::read(lease_path).ok().and_then(|bytes| {
                serde_json::from_slice::<crate::instance_registry::InstanceRegistryEntry>(&bytes)
                    .ok()
            });
            if let (Some(entry), Some(lease)) = (entry, lease) {
                if entry.last_heartbeat_ms > 0 && entry.last_heartbeat_ms == lease.last_heartbeat_ms
                {
                    return entry.last_heartbeat_ms;
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "dedicated actor heartbeat did not refresh entry and lease"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn actor_heartbeat_advances_while_idle() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let state = test_state(&runtime_root);
        let registry_root = state.instance.registry_root();
        let instance_id = state.instance.summary().instance_id;
        let session = Arc::new(BrainSessionCell::new(state));
        let actor = BrainActorHandle::start(
            "idle-heartbeat-brain".to_string(),
            session,
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");

        let (entry_path, lease_path) =
            force_instance_heartbeat_to_zero(&registry_root, &instance_id);
        lock_unpoisoned(&actor.heartbeat_join)
            .as_ref()
            .expect("heartbeat worker")
            .thread()
            .unpark();
        assert!(wait_for_matching_heartbeat(&entry_path, &lease_path) > 0);
        actor.stop().expect("stop actor");
    }

    #[test]
    fn actor_heartbeat_advances_during_blocked_callback() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime_root = temporary.path().join("runtime");
        let state = test_state(&runtime_root);
        let registry_root = state.instance.registry_root();
        let instance_id = state.instance.summary().instance_id;
        let session = Arc::new(BrainSessionCell::new(state));
        let actor = BrainActorHandle::start(
            "blocked-heartbeat-brain".to_string(),
            session,
            runtime_root.join(BRAIN_CHECKPOINT_DIRECTORY),
            Arc::new(UnboundBrainCheckpointAuthority),
            2,
            None,
        )
        .expect("start actor");
        let (entered_tx, entered_rx) = mpsc::sync_channel(0);
        let (release_tx, release_rx) = mpsc::sync_channel(0);
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let command_actor = Arc::clone(&actor);
        let command = thread::spawn(move || {
            let result = command_actor.try_execute(false, move |_state| {
                entered_tx.send(()).expect("announce blocked callback");
                release_rx.recv().expect("release blocked callback");
                Ok::<(), RuntimeJobFailure>(())
            });
            done_tx.send(result).expect("report callback result");
        });
        entered_rx.recv().expect("callback entered");

        let (entry_path, lease_path) =
            force_instance_heartbeat_to_zero(&registry_root, &instance_id);
        lock_unpoisoned(&actor.heartbeat_join)
            .as_ref()
            .expect("heartbeat worker")
            .thread()
            .unpark();
        assert!(wait_for_matching_heartbeat(&entry_path, &lease_path) > 0);
        assert!(
            matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "heartbeat must advance while the actor callback is still blocked"
        );

        release_tx.send(()).expect("release callback");
        done_rx
            .recv()
            .expect("callback completion")
            .expect("callback succeeds");
        command.join().expect("command thread");
        actor.stop().expect("stop actor");
    }
}
