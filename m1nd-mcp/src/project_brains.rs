//! Two-Tier Brain (interim variant) — owner-hosted per-project brain stores.
//!
//! TWO-TIER-BRAIN-PRD context: the canonical end state is process-per-repo
//! (bridge spawn-on-miss, runtime in `<repo>/.m1nd/`, `m1nd init` as the only
//! birth — Slices 2/3). This module ships the PRD's sanctioned INTERIM cut that
//! makes m1nd functional on any repo TODAY: the ONE served owner hosts MULTIPLE
//! graphs — its bound dev graph (untouched, exactly as before) plus N per-project
//! brains, each a full [`SessionState`] with its own store under
//! `<owner runtime_root>/project-brains/<fingerprint(project_root)>/`.
//!
//! Why owner-side stores and not `<repo>/.m1nd/`: writing inside the caller's
//! repo is bound in the PRD to the consented `m1nd init` birth ceremony
//! (TT-INV-8, "no silent births") that this interim deliberately does NOT ship;
//! an owner-side store needs no write into anyone's repo and reuses the #230
//! runtime-root-anchored persistence exactly (each store warm-boots from its own
//! `graph_snapshot.json`).
//!
//! Reuse audit (mother rule): a project brain is born through the SAME
//! `McpServer::new` boot path the owner itself uses (snapshot warm-boot when the
//! store exists, fresh graph when it does not), acquires its lease through the
//! SAME `InstanceHandle` machinery (the per-`runtime_root` lease is exclusive
//! even inside one process), and
//! is filled through the SAME `dispatch_tool("ingest")` path every agent uses.
//! The only net-new surface is this registry map and the store-dir naming.
//!
//! Resolution, bootstrap, and arbitrary SessionState callbacks are internal
//! authority surfaces, not a public Rust embedding API.
//!
//! ```compile_fail
//! use m1nd_mcp::project_brains::ProjectBrainRegistry;
//! # let registry = ProjectBrainRegistry::new("brains".into(), None);
//! let _ = registry.resolve("/repo");
//! ```
//!
//! ```compile_fail
//! use m1nd_mcp::project_brains::ProjectBrainRegistry;
//! # let registry = ProjectBrainRegistry::new("brains".into(), None);
//! let _ = registry.bootstrap("/repo", &serde_json::json!({}));
//! ```
//!
//! ```compile_fail
//! use m1nd_mcp::project_brains::ProjectBrainRegistry;
//! # let registry = ProjectBrainRegistry::new("brains".into(), None);
//! let _ = registry.execute_target_runtime(
//!     unimplemented!(),
//!     None,
//!     true,
//!     false,
//!     |_raw_session| Ok::<(), m1nd_mcp::runtime_jobs::RuntimeJobFailure>(()),
//! );
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, Weak};
use std::time::{Duration, Instant};

use m1nd_core::error::{M1ndError, M1ndResult};
use parking_lot::{Condvar, Mutex, RwLock};

use crate::session::SessionState;

use crate::brain_runtime::{
    project_brain_id, recover_checkpoint_for_boot, BrainActorHandle, BrainBootRecovery,
    BrainCheckpointAuthority, BrainReadSnapshot, BrainRecoveryV1, BrainRuntimeError,
    BrainRuntimeHealthV1, BrainSessionCell, BrainVersionV1, UnboundBrainCheckpointAuthority,
    DEFAULT_BRAIN_ACTOR_QUEUE_CAPACITY,
};
use crate::checkpoint_store::CheckpointAckV1;
use crate::runtime_jobs::{
    RuntimeJobContext, RuntimeJobError, RuntimeJobFailure, RuntimeJobRegistry, RuntimeJobRequestV1,
    RuntimeJobSuccess,
};

/// Default warm-brain cap (§C9.1 F18): how many project brains the owner keeps
/// hydrated in memory at once. The bound dev graph is NOT counted here — it lives
/// on `AppState::session`, never in this map, so it can never be evicted. A cap of
/// 4 means the eviction gate arms before brain #5, exactly as the ladder rung
/// specifies ("before the owner hosts brain #5"). Override via the constructor.
pub const DEFAULT_WARM_BRAIN_CAP: usize = 4;

/// Store-dir manifest: records which project root a store belongs to, so a
/// warm-boot can verify the fingerprint really is this root's brain (hash
/// collisions and moved directories resolve to an honest miss, never a silent
/// wrong-brain bind). Inert data only — no binary paths, no exec directives
/// (PRD §9.4 posture, applied to the owner-side store).
const MANIFEST_FILE: &str = "project_brain.json";

/// The dir under the owner's `runtime_root` that holds all project-brain stores.
pub const PROJECT_BRAINS_DIR: &str = "project-brains";

/// A warm brain plus its LRU access tick. The tick is bumped on every resolve so
/// the eviction gate can pick the least-recently-used victim on a linear scan
/// (the cap is tiny — an O(cap) scan beats an ordered-map dependency, mother
/// rule). `Clone` hands out the `Arc` without the tick.
struct WarmBrain {
    brain: Arc<BrainSessionCell>,
    /// Monotonic last-touch stamp from the registry's own counter — clock-free so
    /// eviction order is deterministic in tests, never wall-time dependent.
    last_used: u64,
    /// Lazily started actor. The `OnceLock` makes one queue/checkpoint writer the
    /// only winner even when several requests discover the same warm brain.
    runtime: Arc<OnceLock<Result<Arc<BrainActorHandle>, String>>>,
    /// Explicit startup receipt retained even before the actor is first used.
    recovery: Option<BrainBootRecovery>,
}

struct BoundRuntime {
    session: Arc<BrainSessionCell>,
    runtime: Arc<BrainActorHandle>,
}

struct RegistryLifecycle {
    accepting: bool,
    active: usize,
    shutdown_started: bool,
}

#[cfg(test)]
struct ReadSnapshotTestHook {
    entered: std::sync::mpsc::SyncSender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

struct RegistryAdmissionGuard<'a> {
    registry: &'a ProjectBrainRegistry,
}

impl Drop for RegistryAdmissionGuard<'_> {
    fn drop(&mut self) {
        let mut lifecycle = self.registry.lifecycle.lock();
        lifecycle.active = lifecycle
            .active
            .checked_sub(1)
            .expect("registry admission count cannot underflow");
        if lifecycle.active == 0 {
            self.registry.lifecycle_drained.notify_all();
        }
    }
}

/// Registry-derived binding for one external mutation actor. The selector is
/// only a lookup hint: the returned brain Arc and actor id are revalidated
/// against the registry (or the bound owner) before any job can be enqueued.
pub(crate) struct ExternalMutationActorBindingV1 {
    pub(crate) brain: Arc<BrainSessionCell>,
    /// Exact canonical root owned by this actor. Mutating transports compare
    /// the current caller root against this value; ancestry is never enough.
    pub(crate) actor_root: String,
    pub(crate) selected_project_root: Option<String>,
    pub(crate) bound: bool,
    pub(crate) brain_id: String,
}

/// Registry of owner-hosted per-project brains, keyed by canonicalized project
/// root. Lives on `AppState` beside (never inside) the bound session.
pub struct ProjectBrainRegistry {
    /// Live brains. The map lock is held only for lookup/insert/evict — never
    /// across an engine build or an ingest (those run on the unshared brain
    /// first). Bounded by `capacity`: the LRU eviction gate (§C9.1) persists then
    /// drops the least-recently-used brain before the map exceeds the cap.
    brains: Mutex<HashMap<String, WarmBrain>>,
    /// Per-canonical-root single-flight gates. A dormant warm boot constructs a
    /// `SessionState`, which acquires the runtime lease before the brain can be
    /// inserted into `brains`; without this gate two same-process resolvers can
    /// mint competing owners and only discover the loser at map insertion.
    hydration_locks: Mutex<HashMap<String, Weak<Mutex<()>>>>,
    /// Global admission fence around hydration. Shutdown takes the write side,
    /// which first drains every in-flight dormant boot/bootstrap and then keeps
    /// new hydrations out while it snapshots and stops the warm set.
    hydration_admission: RwLock<()>,
    /// `<owner runtime_root>/project-brains`.
    base_dir: PathBuf,
    /// The owner's registry dir, so project-brain instances/leases land in the
    /// SAME phonebook (`brain_kind:"project"` tells them apart — mission D rides
    /// the existing `list_instances` surface, zero new listing code).
    registry_dir: Option<PathBuf>,
    /// Warm-brain cap (§C9.1). The map never holds more than this many project
    /// brains hydrated; the bound dev graph is not in the map, so it is never a
    /// candidate. Zero would evict on every insert, so it is clamped to ≥1.
    capacity: usize,
    /// Monotonic LRU clock — bumped on every touch so the newest touch always has
    /// the highest stamp and the eviction victim is `min(last_used)`.
    tick: AtomicU64,
    /// F11-b: the owner-process naming facts, stamped into every project brain
    /// this registry boots (a hosted brain's scan must reach the SAME announced
    /// runnerd + owner secret the bound session does). `None` when the owner has
    /// no announce surface — scans then fall back to heuristic naming.
    runnerd_naming: Option<crate::runnerd_owner::NamingRunnerHandle>,
    /// Narrow authority adapter used by checkpoint creation/recovery. The
    /// default is explicitly unbound and never claims external anti-rollback.
    checkpoint_authority: Arc<dyn BrainCheckpointAuthority>,
    /// Per-brain actor mailbox bound.
    actor_queue_capacity: usize,
    /// Global durable worker bound for this owner registry.
    max_runtime_jobs: usize,
    /// Opened lazily because historical constructors are infallible.
    runtime_jobs: OnceLock<Result<RuntimeJobRegistry, String>>,
    /// Linear lifecycle admission. Every entrypoint retains an RAII guard until
    /// its actor/job lazy initialization and terminal reply have completed.
    lifecycle: Mutex<RegistryLifecycle>,
    lifecycle_drained: Condvar,
    /// The owner's bound/default brain uses the same serial actor contract as
    /// hosted brains. It is opened lazily so historical construction remains
    /// infallible and tests that never dispatch do not create checkpoints.
    bound_runtime: OnceLock<Result<BoundRuntime, String>>,
    #[cfg(test)]
    read_snapshot_test_hook: Mutex<Option<ReadSnapshotTestHook>>,
}

impl ProjectBrainRegistry {
    /// Build a registry with the default warm-brain cap
    /// ([`DEFAULT_WARM_BRAIN_CAP`]).
    pub fn new(base_dir: PathBuf, registry_dir: Option<PathBuf>) -> Self {
        Self::with_capacity(base_dir, registry_dir, DEFAULT_WARM_BRAIN_CAP)
    }

    /// Build a registry with an explicit warm-brain cap. `capacity` is clamped to
    /// ≥1 (a zero cap would evict a brain the instant it was inserted). Surfaced
    /// for the eviction-gate battery case, which pins the bound at a small K to
    /// force eviction with a handful of scratch brains.
    pub fn with_capacity(
        base_dir: PathBuf,
        registry_dir: Option<PathBuf>,
        capacity: usize,
    ) -> Self {
        Self {
            brains: Mutex::new(HashMap::new()),
            hydration_locks: Mutex::new(HashMap::new()),
            hydration_admission: RwLock::new(()),
            base_dir,
            registry_dir,
            capacity: capacity.max(1),
            tick: AtomicU64::new(0),
            runnerd_naming: None,
            checkpoint_authority: Arc::new(UnboundBrainCheckpointAuthority),
            actor_queue_capacity: DEFAULT_BRAIN_ACTOR_QUEUE_CAPACITY,
            max_runtime_jobs: crate::runtime_jobs::DEFAULT_MAX_IN_FLIGHT_JOBS,
            runtime_jobs: OnceLock::new(),
            lifecycle: Mutex::new(RegistryLifecycle {
                accepting: true,
                active: 0,
                shutdown_started: false,
            }),
            lifecycle_drained: Condvar::new(),
            bound_runtime: OnceLock::new(),
            #[cfg(test)]
            read_snapshot_test_hook: Mutex::new(None),
        }
    }

    #[cfg(test)]
    pub(crate) fn install_read_snapshot_test_hook(
        &self,
        entered: std::sync::mpsc::SyncSender<()>,
        release: std::sync::mpsc::Receiver<()>,
    ) {
        *self.read_snapshot_test_hook.lock() = Some(ReadSnapshotTestHook { entered, release });
    }

    /// Thread the owner-process naming facts (F11-b) into every project brain this
    /// registry boots. Builder-style, called once at HTTP-owner construction.
    pub fn with_runnerd_naming(mut self, handle: crate::runnerd_owner::NamingRunnerHandle) -> Self {
        self.runnerd_naming = Some(handle);
        self
    }

    /// Install the real external-authority checkpoint adapter without coupling
    /// this registry to AuthorityRuntime/MissionService concrete types.
    pub fn with_checkpoint_authority(
        mut self,
        authority: Arc<dyn BrainCheckpointAuthority>,
    ) -> Self {
        self.checkpoint_authority = authority;
        self
    }

    /// Configure bounded global worker concurrency and the bounded per-brain
    /// actor queue. Values are clamped to one. Call before first runtime use.
    pub fn with_runtime_limits(mut self, max_runtime_jobs: usize, actor_queue: usize) -> Self {
        self.max_runtime_jobs = max_runtime_jobs.max(1);
        self.actor_queue_capacity = actor_queue.max(1);
        self
    }

    /// Next monotonic LRU stamp.
    fn next_tick(&self) -> u64 {
        self.tick.fetch_add(1, Ordering::Relaxed)
    }

    /// The warm-brain cap this registry enforces (§C9.1).
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Stable authority/job binding id for one canonical project root.
    pub fn brain_id_for(&self, project_root: &str) -> String {
        project_brain_id(&Self::canonical_key(project_root))
    }

    /// Acquire a short immutable snapshot through the per-brain actor. The
    /// snapshot closure cannot mutate SessionState and the bounded queue refuses
    /// excess work immediately.
    pub(crate) fn read_runtime_snapshot<S, Read>(
        &self,
        project_root: &str,
        read: Read,
    ) -> M1ndResult<BrainReadSnapshot<S>>
    where
        S: serde::Serialize + serde::de::DeserializeOwned + Send + 'static,
        Read: FnOnce(&SessionState) -> Result<S, RuntimeJobFailure> + Send + 'static,
    {
        let _admission = self.enter_lifecycle()?;
        let key = Self::canonical_key(project_root);
        if self.try_resolve(&key)?.is_none() {
            return Err(M1ndError::PersistenceFailed(format!(
                "project brain '{key}' is not registered"
            )));
        }
        self.runtime_for_key(&key)?
            .try_read_snapshot(read)
            .map_err(brain_runtime_m1nd_error)
    }

    /// Acquire a snapshot for either the bound owner or a hosted brain through
    /// that brain's serial actor. The target identity is checked so a selector
    /// can never enqueue work on one actor while retaining another brain's Arc.
    pub(crate) fn read_target_runtime_snapshot<S, Read>(
        &self,
        target: Arc<BrainSessionCell>,
        selected_project_root: Option<&str>,
        bound: bool,
        read: Read,
    ) -> M1ndResult<BrainReadSnapshot<S>>
    where
        S: serde::Serialize + serde::de::DeserializeOwned + Send + 'static,
        Read: FnOnce(&SessionState) -> Result<S, RuntimeJobFailure> + Send + 'static,
    {
        let _admission = self.enter_lifecycle()?;
        #[cfg(test)]
        let test_hook = self.read_snapshot_test_hook.lock().take();
        self.runtime_for_target(target, selected_project_root, bound)?
            .try_read_snapshot(move |state| {
                #[cfg(test)]
                if let Some(hook) = test_hook {
                    hook.entered.send(()).map_err(|error| {
                        RuntimeJobFailure::new("test_probe_failed", error.to_string())
                    })?;
                    hook.release.recv().map_err(|error| {
                        RuntimeJobFailure::new("test_probe_failed", error.to_string())
                    })?;
                }
                read(state)
            })
            .map_err(brain_runtime_m1nd_error)
    }

    /// Execute a transport command on the selected brain actor. This is the
    /// common REST/MCP/stdio dispatch seam; overload is an immediate typed
    /// refusal from the bounded actor queue, never an unbounded mutex wait.
    pub(crate) fn execute_target_runtime<R, Execute>(
        &self,
        target: Arc<BrainSessionCell>,
        selected_project_root: Option<&str>,
        bound: bool,
        mutating: bool,
        execute: Execute,
    ) -> M1ndResult<R>
    where
        R: Send + 'static,
        Execute: FnOnce(&mut SessionState) -> Result<R, RuntimeJobFailure> + Send + 'static,
    {
        let _admission = self.enter_lifecycle()?;
        self.runtime_for_target(target, selected_project_root, bound)?
            .try_execute(mutating, execute)
            .map_err(brain_runtime_m1nd_error)
    }

    /// Execute one M1nd command transactionally while preserving its exact
    /// domain error for the transport. The sentinel failure is observed by the
    /// actor *inside* the callback, so any partial mutation is rolled back before
    /// the original error is returned to the caller.
    pub(crate) fn execute_target_m1nd<R, Execute>(
        &self,
        target: Arc<BrainSessionCell>,
        selected_project_root: Option<&str>,
        bound: bool,
        mutating: bool,
        execute: Execute,
    ) -> M1ndResult<R>
    where
        R: Send + 'static,
        Execute: FnOnce(&mut SessionState) -> M1ndResult<R> + Send + 'static,
    {
        const DOMAIN_ERROR_SENTINEL: &str = "m1nd_actor_domain_error";

        let _admission = self.enter_lifecycle()?;
        let original_error = Arc::new(Mutex::new(None));
        let captured_error = Arc::clone(&original_error);
        let actor_result = self
            .runtime_for_target(target, selected_project_root, bound)?
            .try_execute(mutating, move |state| {
                execute(state).map_err(|error| {
                    let message = error.to_string();
                    *captured_error.lock() = Some(error);
                    RuntimeJobFailure::new(DOMAIN_ERROR_SENTINEL, message)
                })
            });

        match actor_result {
            Ok(output) => Ok(output),
            Err(BrainRuntimeError::SnapshotRead(failure))
                if failure.code == DOMAIN_ERROR_SENTINEL =>
            {
                Err(original_error.lock().take().unwrap_or_else(|| {
                    M1ndError::PersistenceFailed(
                        "brain actor rolled back a domain error but lost its transport value"
                            .to_string(),
                    )
                }))
            }
            Err(error) => Err(brain_runtime_m1nd_error(error)),
        }
    }

    /// Execute one mutation on the selected actor and return the exact
    /// checkpoint ACK produced by that same actor turn.
    pub(crate) fn execute_target_runtime_with_checkpoint_ack<R, Execute>(
        &self,
        target: Arc<BrainSessionCell>,
        selected_project_root: Option<&str>,
        bound: bool,
        execute: Execute,
    ) -> M1ndResult<(R, CheckpointAckV1)>
    where
        R: Send + 'static,
        Execute: FnOnce(&mut SessionState) -> Result<R, RuntimeJobFailure> + Send + 'static,
    {
        let _admission = self.enter_lifecycle()?;
        self.runtime_for_target(target, selected_project_root, bound)?
            .try_execute_with_checkpoint_ack(execute)
            .map_err(brain_runtime_m1nd_error)
    }

    /// Deterministic identity of the bound/default actor for a concrete target.
    /// Kept beside `runtime_for_target` so durable receipts bind the same identity
    /// the registry will actually start.
    pub(crate) fn bound_brain_id_for_target(
        &self,
        target: Arc<BrainSessionCell>,
    ) -> M1ndResult<String> {
        let _admission = self.enter_lifecycle()?;
        self.runtime_for_target(target, None, true)
            .map(|runtime| runtime.brain_id().to_string())
    }

    /// Actor-safe bound-owner coverage predicate. The caller supplies the
    /// owner's configured session only as an identity handle; no SessionState
    /// guard or interior capability crosses this API.
    pub(crate) fn bound_covers_root(
        &self,
        target: Arc<BrainSessionCell>,
        root: &str,
    ) -> M1ndResult<bool> {
        let _admission = self.enter_lifecycle()?;
        let canonical = Self::canonical_key(root);
        self.runtime_for_target(target, None, true)?
            .try_read_snapshot(move |state| {
                Ok::<_, RuntimeJobFailure>(state.covers_root(&canonical))
            })
            .map(|snapshot| snapshot.value)
            .map_err(brain_runtime_m1nd_error)
    }

    pub(crate) fn bound_actor_root_for_target(
        &self,
        target: Arc<BrainSessionCell>,
    ) -> M1ndResult<String> {
        let _admission = self.enter_lifecycle()?;
        self.runtime_for_target(target, None, true)?
            .try_read_snapshot(|state| {
                state
                    .workspace_root
                    .as_deref()
                    .or_else(|| state.ingest_roots.first().map(String::as_str))
                    .map(Self::canonical_key)
                    .ok_or_else(|| {
                        RuntimeJobFailure::new(
                            "external_mutation_actor_root_missing",
                            "bound actor has no canonical workspace or ingest root",
                        )
                    })
            })
            .map(|snapshot| snapshot.value)
            .map_err(brain_runtime_m1nd_error)
    }

    /// Non-hydrating health of the already-started bound actor. This never
    /// enters the actor queue and never locks SessionState; `Ok(None)` means the
    /// bound actor has not been needed yet.
    pub(crate) fn bound_runtime_health(&self) -> M1ndResult<Option<BrainRuntimeHealthV1>> {
        let Some(opened) = self.bound_runtime.get() else {
            return Ok(None);
        };
        match opened {
            Ok(bound) => Ok(Some(bound.runtime.health_snapshot())),
            Err(error) => Err(M1ndError::PersistenceFailed(format!(
                "bound brain actor refused: {error}"
            ))),
        }
    }

    /// Resolve the exact actor target used by external source mutations. Hosted
    /// project brains take precedence, matching live MCP selection; otherwise
    /// the selector must be covered by the bound owner. Unknown selectors and
    /// Arc/actor identity mismatches remain hard failures.
    pub(crate) fn resolve_external_mutation_actor(
        &self,
        bound_target: Arc<BrainSessionCell>,
        selector: &str,
    ) -> M1ndResult<ExternalMutationActorBindingV1> {
        let _admission = self.enter_lifecycle()?;
        let canonical = Self::canonical_key(selector);
        if let Some(brain) = self.try_resolve(&canonical)? {
            let runtime =
                self.runtime_for_target(Arc::clone(&brain), Some(canonical.as_str()), false)?;
            return Ok(ExternalMutationActorBindingV1 {
                brain,
                actor_root: canonical.clone(),
                selected_project_root: Some(canonical),
                bound: false,
                brain_id: runtime.brain_id().to_string(),
            });
        }
        if !self.bound_covers_root(Arc::clone(&bound_target), &canonical)? {
            return Err(M1ndError::PersistenceFailed(format!(
                "external mutation selector '{canonical}' is neither hosted nor covered by the bound owner"
            )));
        }
        let actor_root = self.bound_actor_root_for_target(Arc::clone(&bound_target))?;
        let runtime = self.runtime_for_target(Arc::clone(&bound_target), None, true)?;
        Ok(ExternalMutationActorBindingV1 {
            brain: bound_target,
            actor_root,
            selected_project_root: None,
            bound: true,
            brain_id: runtime.brain_id().to_string(),
        })
    }

    /// Resolve the actor selected by one transport context while keeping the
    /// route/root selector separate from the actor's durable identity. A fresh
    /// MCP session legitimately has no sticky project-root selector; in that
    /// case the already-hosted bound owner is the only admissible actor.
    /// Related hosted roots are accepted only when the roster proves exactly
    /// one covering brain, matching the ordinary routing seam's abstain law.
    pub(crate) fn resolve_external_mutation_transport_actor(
        &self,
        bound_target: Arc<BrainSessionCell>,
        route_selector: Option<&str>,
    ) -> M1ndResult<ExternalMutationActorBindingV1> {
        let Some(selector) = route_selector else {
            let _admission = self.enter_lifecycle()?;
            let runtime = self.runtime_for_target(Arc::clone(&bound_target), None, true)?;
            let actor_root = self.bound_actor_root_for_target(Arc::clone(&bound_target))?;
            return Ok(ExternalMutationActorBindingV1 {
                brain: bound_target,
                actor_root,
                selected_project_root: None,
                bound: true,
                brain_id: runtime.brain_id().to_string(),
            });
        };

        let canonical = Self::canonical_key(selector);
        if self.try_resolve(&canonical)?.is_some()
            || self.bound_covers_root(Arc::clone(&bound_target), &canonical)?
        {
            return self.resolve_external_mutation_actor(bound_target, &canonical);
        }
        let covering = self.covering_brain(&canonical).ok_or_else(|| {
            M1ndError::PersistenceFailed(format!(
                "external mutation route selector '{canonical}' does not identify one existing hosted or bound brain"
            ))
        })?;
        self.resolve_external_mutation_actor(bound_target, &covering)
    }

    /// Resolve a durable journal/authority brain id back to its exact actor.
    /// Root selectors are deliberately not accepted on this seam.
    pub(crate) fn resolve_external_mutation_actor_by_id(
        &self,
        bound_target: Arc<BrainSessionCell>,
        actor_brain_id: &str,
    ) -> M1ndResult<ExternalMutationActorBindingV1> {
        let bound_id = self.bound_brain_id_for_target(Arc::clone(&bound_target))?;
        if actor_brain_id == bound_id {
            return self.resolve_external_mutation_transport_actor(bound_target, None);
        }

        let matching_roots = self
            .existing_brain_roots()
            .into_iter()
            .filter(|root| self.brain_id_for(root) == actor_brain_id)
            .collect::<Vec<_>>();
        if matching_roots.len() != 1 {
            return Err(M1ndError::PersistenceFailed(format!(
                "external mutation actor id '{actor_brain_id}' resolved to {} hosted roots",
                matching_roots.len()
            )));
        }
        self.resolve_external_mutation_actor(bound_target, &matching_roots[0])
    }

    /// Submit blocking preparation to the durable global job registry. Only the
    /// actor-controlled commit closure receives the proposal; stale OCC results
    /// become a FAILED terminal job and never invoke `apply`.
    pub(crate) fn submit_runtime_job<S, P, Read, Prepare, Apply>(
        &self,
        project_root: &str,
        request: RuntimeJobRequestV1,
        read: Read,
        prepare: Prepare,
        apply: Apply,
    ) -> M1ndResult<String>
    where
        S: serde::Serialize + serde::de::DeserializeOwned + Send + 'static,
        P: Send + 'static,
        Read: FnOnce(&SessionState) -> Result<S, RuntimeJobFailure> + Send + 'static,
        Prepare: FnOnce(RuntimeJobContext, BrainReadSnapshot<S>) -> Result<P, RuntimeJobFailure>
            + Send
            + 'static,
        Apply: FnOnce(&mut SessionState, P) -> Result<RuntimeJobSuccess, RuntimeJobFailure>
            + Send
            + 'static,
    {
        let _admission = self.enter_lifecycle()?;
        let key = Self::canonical_key(project_root);
        if self.try_resolve(&key)?.is_none() {
            return Err(M1ndError::PersistenceFailed(format!(
                "project brain '{key}' is not registered"
            )));
        }
        let runtime = self.runtime_for_key(&key)?;
        let snapshot = runtime
            .try_read_snapshot(read)
            .map_err(brain_runtime_m1nd_error)?;
        if request.binding.brain_id != snapshot.brain_id {
            return Err(brain_runtime_m1nd_error(
                BrainRuntimeError::BrainBindingMismatch {
                    expected: snapshot.brain_id,
                    observed: request.binding.brain_id,
                },
            ));
        }
        if request.snapshot_revision != snapshot.version.revision {
            return Err(brain_runtime_m1nd_error(
                BrainRuntimeError::SnapshotRevisionMismatch {
                    expected: snapshot.version.revision,
                    observed: request.snapshot_revision,
                },
            ));
        }
        let expected = snapshot.version;
        let commit_runtime = runtime.clone();
        self.runtime_job_registry()?
            .submit_prepared(
                request,
                move |context| prepare(context, snapshot),
                move |proposal| {
                    commit_runtime
                        .commit(expected, proposal, apply)
                        .map_err(BrainRuntimeError::into_job_failure)
                },
            )
            .map_err(runtime_job_m1nd_error)
    }

    /// Clone of the durable registry for status/cancel/wait surfaces. Opening is
    /// lazy and single-writer; failure is sticky and fail-closed.
    pub(crate) fn runtime_job_registry(&self) -> M1ndResult<RuntimeJobRegistry> {
        let _admission = self.enter_lifecycle()?;
        let opened = self.runtime_jobs.get_or_init(|| {
            RuntimeJobRegistry::open_with_max_in_flight(
                self.base_dir.join("runtime-jobs").join("jobs.jsonl"),
                self.max_runtime_jobs,
            )
            .map_err(|error| error.to_string())
        });
        match opened {
            Ok(registry) => Ok(registry.clone()),
            Err(error) => Err(M1ndError::PersistenceFailed(format!(
                "runtime job registry refused: {error}"
            ))),
        }
    }

    /// Recovery is never silent: exact CURRENT and degraded fallback both leave
    /// a typed authority/fallback receipt on the warm brain.
    pub fn recovery_receipt(&self, project_root: &str) -> Option<BrainRecoveryV1> {
        let key = Self::canonical_key(project_root);
        self.brains
            .lock()
            .get(&key)
            .and_then(|warm| warm.recovery.as_ref().map(|item| item.receipt.clone()))
    }

    /// Read one already-started actor's health without entering its bounded
    /// queue or locking SessionState. Health never hydrates a dormant brain.
    pub fn runtime_health(&self, project_root: &str) -> M1ndResult<BrainRuntimeHealthV1> {
        let key = Self::canonical_key(project_root);
        let runtime = self
            .brains
            .lock()
            .get(&key)
            .and_then(|warm| warm.runtime.get())
            .and_then(|opened| opened.as_ref().ok())
            .cloned()
            .ok_or_else(|| {
                M1ndError::PersistenceFailed(format!(
                    "project brain '{key}' has no started runtime actor"
                ))
            })?;
        Ok(runtime.health_snapshot())
    }

    /// Owner-wide, read-only health of actors that are already live. The map
    /// lock is released before copying each actor snapshot, and dormant brains
    /// are deliberately absent rather than hydrated as a side effect.
    pub fn runtime_health_snapshots(&self) -> Vec<BrainRuntimeHealthV1> {
        let runtimes = self
            .brains
            .lock()
            .values()
            .filter_map(|warm| warm.runtime.get())
            .filter_map(|opened| opened.as_ref().ok())
            .cloned()
            .collect::<Vec<_>>();
        let mut snapshots = runtimes
            .into_iter()
            .map(|runtime| runtime.health_snapshot())
            .collect::<Vec<_>>();
        if let Some(Ok(bound)) = self.bound_runtime.get() {
            snapshots.push(bound.runtime.health_snapshot());
        }
        snapshots.sort_by(|left, right| left.brain_id.cmp(&right.brain_id));
        snapshots
    }

    /// Explicit recovery seam for a transient persistence failure. It retries
    /// the full persist + checkpoint + CURRENT confirmation; no write admission
    /// is restored until the actor receives a real checkpoint ACK.
    pub(crate) fn retry_runtime_checkpoint(
        &self,
        project_root: &str,
    ) -> M1ndResult<CheckpointAckV1> {
        let _admission = self.enter_lifecycle()?;
        let key = Self::canonical_key(project_root);
        if self.try_resolve(&key)?.is_none() {
            return Err(M1ndError::PersistenceFailed(format!(
                "project brain '{key}' is not registered"
            )));
        }
        self.checkpoint_brain(&key)
    }

    /// Graceful owner shutdown: stop accepting, cancel/join global workers, then
    /// pause *all* actors, checkpoint+ACK *all* actors, and only then stop any of
    /// them. This two-phase fence prevents an early actor from relinquishing its
    /// lease while a later actor discovers a checkpoint failure. The returned
    /// ACKs are the exact eviction/restart proof, not a log-only claim.
    pub(crate) fn shutdown(&self, grace: Duration) -> M1ndResult<Vec<CheckpointAckV1>> {
        let started = Instant::now();
        let deadline = started.checked_add(grace).ok_or_else(|| {
            M1ndError::PersistenceFailed("owner shutdown deadline overflowed".to_string())
        })?;

        // Terminal lifecycle contract: one shutdown attempt closes admission
        // forever. Failures remain fail-closed; actors are never resumed behind
        // a registry that can no longer route recovery traffic.
        {
            let mut lifecycle = self.lifecycle.lock();
            if lifecycle.shutdown_started {
                return Err(M1ndError::PersistenceFailed(
                    "project brain registry shutdown is terminal and was already attempted"
                        .to_string(),
                ));
            }
            lifecycle.shutdown_started = true;
            lifecycle.accepting = false;
            if let Some(opened) = self.runtime_jobs.get() {
                let registry = opened.as_ref().map_err(|error| {
                    M1ndError::PersistenceFailed(format!(
                        "runtime job registry refused at lifecycle fence: {error}"
                    ))
                })?;
                registry.close_admission().map_err(runtime_job_m1nd_error)?;
            }
            while lifecycle.active > 0 {
                let remaining = remaining_shutdown_time(deadline, "lifecycle admission drain")?;
                let wait = self.lifecycle_drained.wait_for(&mut lifecycle, remaining);
                if wait.timed_out() && lifecycle.active > 0 {
                    return Err(M1ndError::PersistenceFailed(format!(
                        "owner shutdown timed out with {} admitted operation(s) still active",
                        lifecycle.active
                    )));
                }
            }
        }

        // Every hydration entrypoint is covered by the admission guard above;
        // after the drain this write lock is uncontended and freezes the warm
        // map topology for the remaining terminal phases.
        let _hydration_fence = self.hydration_admission.write();
        let mut errors = Vec::new();
        if let Some(opened) = self.runtime_jobs.get() {
            match opened {
                Ok(registry) => {
                    let remaining = remaining_shutdown_time(deadline, "runtime job shutdown")?;
                    if let Err(error) = registry.shutdown(remaining) {
                        errors.push(format!("runtime jobs: {error}"));
                    }
                }
                Err(error) => errors.push(format!(
                    "runtime job registry refused before shutdown: {error}"
                )),
            }
        }
        if !errors.is_empty() {
            return Err(M1ndError::PersistenceFailed(format!(
                "owner shutdown refused before actor initialization: {}",
                errors.join("; ")
            )));
        }

        let mut warm = self
            .brains
            .lock()
            .iter()
            .map(|(key, warm)| {
                (
                    key.clone(),
                    warm.brain.clone(),
                    warm.runtime.clone(),
                    warm.recovery.clone(),
                )
            })
            .collect::<Vec<_>>();
        warm.sort_by(|left, right| left.0.cmp(&right.0));
        let mut hosted = Vec::with_capacity(warm.len());
        for (key, brain, runtime_cell, recovery) in warm {
            remaining_shutdown_time(deadline, "hosted actor initialization")?;
            let runtime = match self.runtime_for_parts(&key, brain.clone(), &runtime_cell, recovery)
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    errors.push(format!("hosted brain '{key}' runtime: {error}"));
                    continue;
                }
            };
            hosted.push((key, brain, runtime_cell, runtime));
        }
        let bound = match self.bound_runtime.get() {
            Some(Ok(bound)) => Some(Arc::clone(&bound.runtime)),
            Some(Err(error)) => {
                errors.push(format!("bound brain actor refused: {error}"));
                None
            }
            None => None,
        };
        if !errors.is_empty() {
            return Err(M1ndError::PersistenceFailed(format!(
                "owner shutdown refused before actor checkpoint (0 checkpoint ACKs): {}",
                errors.join("; ")
            )));
        }

        let mut actors = hosted
            .iter()
            .map(|(key, _, _, runtime)| (format!("hosted brain '{key}'"), Arc::clone(runtime)))
            .chain(
                bound
                    .iter()
                    .map(|runtime| ("bound brain".to_string(), Arc::clone(runtime))),
            )
            .collect::<Vec<_>>();
        actors.sort_by(|left, right| left.1.brain_id().cmp(right.1.brain_id()));

        // Phase 1a: close admission everywhere before any checkpoint starts.
        for (label, runtime) in &actors {
            match runtime.pause() {
                Ok(()) => {}
                Err(error) => errors.push(format!("{label} pause: {error}")),
            }
        }
        if !errors.is_empty() {
            return Err(M1ndError::PersistenceFailed(format!(
                "owner shutdown refused before actor checkpoint (0 checkpoint ACKs): {}",
                errors.join("; ")
            )));
        }

        // Phase 1b: every actor must produce a durable ACK. A single failure is
        // terminal and leaves every actor paused/fenced; a timed-out helper may
        // still finish, but can never race newly admitted work or lease release.
        let mut acks = Vec::with_capacity(actors.len());
        for (label, runtime) in &actors {
            match checkpoint_actor_before_deadline(Arc::clone(runtime), deadline) {
                Ok(ack) => acks.push((runtime.brain_id().to_string(), ack)),
                Err(error) => errors.push(format!("{label} checkpoint: {error}")),
            }
        }
        if !errors.is_empty() {
            return Err(M1ndError::PersistenceFailed(format!(
                "owner shutdown checkpoint phase incomplete ({} checkpoint ACKs): {}",
                acks.len(),
                errors.join("; ")
            )));
        }

        // Phase 2: all postimages are now durable, so terminal actor release is
        // allowed. Stop failures remain terminal/fenced and never resume.
        for (key, brain, runtime_cell, runtime) in hosted {
            match stop_paused_actor_before_deadline(Arc::clone(&runtime), deadline) {
                Ok(()) => {
                    if let Err(error) = brain.release_hosted_instance_after_actor_stop() {
                        errors.push(format!(
                            "hosted brain '{key}' instance release after actor stop: {error}"
                        ));
                        continue;
                    }
                    // Remove only the exact brain stopped above; a replacement
                    // inserted by a racer remains authoritative.
                    let removed = {
                        let mut brains = self.brains.lock();
                        let exact = brains
                            .get(&key)
                            .is_some_and(|warm| Arc::ptr_eq(&warm.brain, &brain));
                        exact.then(|| brains.remove(&key)).flatten()
                    };
                    drop(runtime);
                    drop(runtime_cell);
                    drop(brain);
                    drop(removed);
                }
                Err(error) => {
                    errors.push(format!("hosted brain '{key}' stop: {error}"));
                }
            }
        }
        if let Some(runtime) = bound {
            if let Err(error) = stop_paused_actor_before_deadline(Arc::clone(&runtime), deadline) {
                errors.push(format!("bound brain stop: {error}"));
            }
        }
        if !errors.is_empty() {
            return Err(M1ndError::PersistenceFailed(format!(
                "owner shutdown stop phase incomplete ({} checkpoint ACKs): {}",
                acks.len(),
                errors.join("; ")
            )));
        }
        acks.sort_by(|left, right| left.0.cmp(&right.0));
        Ok(acks.into_iter().map(|(_, ack)| ack).collect())
    }

    fn enter_lifecycle(&self) -> M1ndResult<RegistryAdmissionGuard<'_>> {
        let mut lifecycle = self.lifecycle.lock();
        if !lifecycle.accepting {
            return Err(M1ndError::PersistenceFailed(
                "project brain registry is shutting down".to_string(),
            ));
        }
        lifecycle.active = lifecycle.active.checked_add(1).ok_or_else(|| {
            M1ndError::PersistenceFailed(
                "project brain registry admission counter overflowed".to_string(),
            )
        })?;
        Ok(RegistryAdmissionGuard { registry: self })
    }

    fn runtime_for_key(&self, key: &str) -> M1ndResult<Arc<BrainActorHandle>> {
        let (brain, runtime, recovery) = self
            .brains
            .lock()
            .get(key)
            .map(|warm| {
                (
                    warm.brain.clone(),
                    warm.runtime.clone(),
                    warm.recovery.clone(),
                )
            })
            .ok_or_else(|| {
                M1ndError::PersistenceFailed(format!("project brain '{key}' is not warm"))
            })?;
        self.runtime_for_parts(key, brain, &runtime, recovery)
    }

    fn runtime_for_target(
        &self,
        target: Arc<BrainSessionCell>,
        selected_project_root: Option<&str>,
        bound: bool,
    ) -> M1ndResult<Arc<BrainActorHandle>> {
        if bound {
            let opened = self.bound_runtime.get_or_init(|| {
                // This is the one explicit pre-actor compatibility guard.
                // `lock_mut_before_actor()` double-checks the ownership fence,
                // so a foreign/duplicate actor cannot be adopted silently by
                // this registry. No guard survives actor startup.
                let (runtime_root, identity) = {
                    let session = target
                        .lock_mut_before_actor()
                        .map_err(|error| error.to_string())?;
                    let identity = session
                        .workspace_root
                        .clone()
                        .or_else(|| session.ingest_roots.first().cloned())
                        .unwrap_or_else(|| session.runtime_root.to_string_lossy().into_owned());
                    (session.runtime_root.clone(), identity)
                };
                BrainActorHandle::start(
                    project_brain_id(&format!("bound:{identity}")),
                    target.clone(),
                    runtime_root.join(crate::brain_runtime::BRAIN_CHECKPOINT_DIRECTORY),
                    self.checkpoint_authority.clone(),
                    self.actor_queue_capacity,
                    None,
                )
                .map(|runtime| BoundRuntime {
                    session: target.clone(),
                    runtime,
                })
                .map_err(|error| error.to_string())
            });
            return match opened {
                Ok(bound_runtime) if Arc::ptr_eq(&bound_runtime.session, &target) => {
                    Ok(bound_runtime.runtime.clone())
                }
                Ok(_) => Err(M1ndError::PersistenceFailed(
                    "bound brain actor target changed after initialization".to_string(),
                )),
                Err(error) => Err(M1ndError::PersistenceFailed(format!(
                    "bound brain actor refused: {error}"
                ))),
            };
        }

        let (key, registered) = {
            let brains = self.brains.lock();
            if let Some(root) = selected_project_root {
                let key = Self::canonical_key(root);
                let registered =
                    brains
                        .get(&key)
                        .map(|warm| warm.brain.clone())
                        .ok_or_else(|| {
                            M1ndError::PersistenceFailed(format!(
                                "project brain '{key}' is not warm"
                            ))
                        })?;
                (key, registered)
            } else {
                brains
                    .iter()
                    .find(|(_, warm)| Arc::ptr_eq(&warm.brain, &target))
                    .map(|(key, warm)| (key.clone(), warm.brain.clone()))
                    .ok_or_else(|| {
                        M1ndError::PersistenceFailed(
                            "hosted brain dispatch target is not registered".to_string(),
                        )
                    })?
            }
        };
        if !Arc::ptr_eq(&registered, &target) {
            return Err(M1ndError::PersistenceFailed(format!(
                "project brain '{key}' target does not match its actor binding"
            )));
        }
        self.runtime_for_key(&key)
    }

    fn runtime_for_parts(
        &self,
        key: &str,
        brain: Arc<BrainSessionCell>,
        runtime: &Arc<OnceLock<Result<Arc<BrainActorHandle>, String>>>,
        recovery: Option<BrainBootRecovery>,
    ) -> M1ndResult<Arc<BrainActorHandle>> {
        let opened = runtime.get_or_init(|| {
            BrainActorHandle::start(
                project_brain_id(key),
                brain,
                self.store_dir_for(key)
                    .join(crate::brain_runtime::BRAIN_CHECKPOINT_DIRECTORY),
                self.checkpoint_authority.clone(),
                self.actor_queue_capacity,
                recovery,
            )
            .map_err(|error| error.to_string())
        });
        match opened {
            Ok(runtime) => Ok(runtime.clone()),
            Err(error) => Err(M1ndError::PersistenceFailed(format!(
                "project brain '{key}' actor refused: {error}"
            ))),
        }
    }

    fn checkpoint_brain(&self, key: &str) -> M1ndResult<CheckpointAckV1> {
        self.runtime_for_key(key)?
            .checkpoint_and_ack()
            .map_err(brain_runtime_m1nd_error)
    }

    fn brain_has_active_jobs(&self, brain_id: &str) -> M1ndResult<bool> {
        let Some(opened) = self.runtime_jobs.get() else {
            return Ok(false);
        };
        let registry = opened.as_ref().map_err(|error| {
            M1ndError::PersistenceFailed(format!("runtime job registry refused: {error}"))
        })?;
        Ok(registry
            .list()
            .map_err(runtime_job_m1nd_error)?
            .iter()
            .any(|job| job.binding.brain_id == brain_id && !job.state.is_terminal()))
    }

    /// How many project brains are hydrated in the map RIGHT NOW (diagnostics /
    /// the eviction-gate proof: assert the count never exceeds `capacity`).
    pub fn warm_len(&self) -> usize {
        self.brains.lock().len()
    }

    /// Canonical map key for a project root (resolves symlinks/`/tmp` aliases so
    /// one repo cannot become two brains; falls back to the raw string when the
    /// path does not resolve).
    pub fn canonical_key(root: &str) -> String {
        let trimmed = root.trim().trim_end_matches('/');
        Path::new(trimmed)
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| trimmed.to_string())
    }

    /// The on-disk store dir for a project root — one hashing scheme shared with
    /// the lease files (`instance_registry::fingerprint_path`).
    pub fn store_dir_for(&self, canonical_root: &str) -> PathBuf {
        self.base_dir
            .join(crate::instance_registry::fingerprint_path(Path::new(
                canonical_root,
            )))
    }

    /// True when a brain for this root is live in the map or dormant on disk.
    pub fn knows(&self, caller_root: &str) -> bool {
        let key = Self::canonical_key(caller_root);
        if self.brains.lock().contains_key(&key) {
            return true;
        }
        self.manifest_matches(&key)
    }

    fn manifest_matches(&self, key: &str) -> bool {
        let manifest = self.store_dir_for(key).join(MANIFEST_FILE);
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            return false;
        };
        serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v["project_root"].as_str().map(|s| s == key))
            .unwrap_or(false)
    }

    /// Resolve the live brain for `caller_root`, warm-booting it from its store
    /// if the owner restarted since it was created (#230 semantics per store).
    /// `Ok(None)` = this root has no project brain (the caller belongs to the
    /// bound graph or to reception). Checkpoint/eviction/boot failures remain
    /// errors: callers must not turn a durability refusal into an apparent
    /// unknown brain and silently route a mutation to another store.
    pub(crate) fn try_resolve(
        &self,
        caller_root: &str,
    ) -> M1ndResult<Option<Arc<BrainSessionCell>>> {
        let _admission = self.enter_lifecycle()?;
        let _hydration_admission = self.hydration_admission.read();
        let key = Self::canonical_key(caller_root);
        if let Some(brain) = self.touch_warm_brain(&key) {
            return Ok(Some(brain));
        }

        let hydration_lock = self.hydration_lock(&key);
        let _hydration = hydration_lock.lock();
        self.try_resolve_locked(&key)
    }

    /// Resolve while the caller owns this canonical root's hydration gate.
    /// The map is rechecked after gate acquisition so the loser of a concurrent
    /// resolve adopts the incumbent without constructing a second lease owner.
    fn try_resolve_locked(&self, key: &str) -> M1ndResult<Option<Arc<BrainSessionCell>>> {
        if let Some(brain) = self.touch_warm_brain(key) {
            return Ok(Some(brain));
        }
        if !self.manifest_matches(key) {
            return Ok(None);
        }
        // Dormant store → warm-boot OUTSIDE the map lock (engine build is slow).
        let (state, recovery) = self.boot_store_with_recovery(key)?;
        let built = Arc::new(BrainSessionCell::new(state));
        // Insert through the eviction gate: a warm-boot that grows the map past
        // the cap persists-then-drops the LRU victim before this brain lands.
        self.insert_with_eviction_recovery(key.to_string(), built, recovery)
            .map(Some)
    }

    fn touch_warm_brain(&self, key: &str) -> Option<Arc<BrainSessionCell>> {
        let mut map = self.brains.lock();
        let warm = map.get_mut(key)?;
        // This is now the most-recently-used brain, so it is the LAST the
        // eviction gate may drop.
        warm.last_used = self.next_tick();
        Some(warm.brain.clone())
    }

    fn hydration_lock(&self, key: &str) -> Arc<Mutex<()>> {
        let mut locks = self.hydration_locks.lock();
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(existing) = locks.get(key).and_then(Weak::upgrade) {
            return existing;
        }
        let lock = Arc::new(Mutex::new(()));
        locks.insert(key.to_string(), Arc::downgrade(&lock));
        lock
    }

    /// Compatibility-only lossy probe. New routing and mutation paths must use
    /// [`Self::try_resolve`] so persistence failures remain fail-closed.
    pub(crate) fn resolve(&self, caller_root: &str) -> Option<Arc<BrainSessionCell>> {
        self.try_resolve(caller_root).ok().flatten()
    }

    /// Boot (fresh or warm) a store's SessionState through the SAME path the
    /// served owner boots with: `McpServer::new` loads `graph_snapshot.json`
    /// when present, else starts an empty graph; plasticity and sidecars are
    /// anchored on the store dir (its `runtime_root`).
    fn boot_store(&self, key: &str) -> M1ndResult<SessionState> {
        self.boot_store_with_recovery(key).map(|(state, _)| state)
    }

    /// Restore an explicitly selected checkpoint before the canonical
    /// `McpServer::new` warm-boot path reads the store. Missing CURRENT remains a
    /// legacy/fresh boot; corrupt/unusable CURRENT fails closed.
    fn boot_store_with_recovery(
        &self,
        key: &str,
    ) -> M1ndResult<(SessionState, Option<BrainBootRecovery>)> {
        let store = self.store_dir_for(key);
        std::fs::create_dir_all(&store)?;
        let brain_id = project_brain_id(key);
        let recovery =
            recover_checkpoint_for_boot(&store, &brain_id, self.checkpoint_authority.as_ref())
                .map_err(brain_runtime_m1nd_error)?;
        let config = crate::server::McpConfig {
            graph_source: store.join("graph_snapshot.json"),
            plasticity_state: store.join("plasticity_state.json"),
            runtime_dir: Some(store.clone()),
            registry_dir: self.registry_dir.clone(),
            ..Default::default()
        };
        let mut state = crate::server::McpServer::new(config)?.into_session_state();
        // A project brain's workspace IS its project root — the manifest is its
        // birth record. Without this, a warm boot would infer the store dir
        // (graph_path_parent) and wear a dishonest fingerprint; the fresh-boot
        // path gets the same value re-set by `finalize_ingest` right after.
        state.workspace_root = Some(key.to_string());
        state.workspace_root_source = Some("project_brain_manifest".into());
        // F11-b: a hosted brain's scan reaches the same announced runnerd + owner
        // secret the bound session does (its OWN runtime root is its store dir,
        // never the secret's home).
        state.runnerd_naming = self.runnerd_naming.clone();
        // Stamp the registry entry so the shared phonebook can tell this brain
        // from the bound dev graph (best-effort: a failed stamp never blocks the
        // brain — the entry just stays kind-less like a legacy one).
        state.instance.set_brain_kind("project")?;
        Ok((state, recovery))
    }

    /// One-call bootstrap: create (or warm-resolve) the brain for
    /// `project_root`, ingest the repo into it, and return it with the ingest
    /// result. The caller (the HTTP routing layer) binds the wire session and
    /// composes the orientation packet.
    ///
    /// `ingest_args` are the caller's original `ingest` arguments; `path` is
    /// forced to the project root and `project_root` itself is stripped (it is a
    /// routing directive, not an adapter input).
    pub(crate) fn bootstrap(
        &self,
        project_root: &str,
        ingest_args: &serde_json::Value,
    ) -> M1ndResult<(Arc<BrainSessionCell>, serde_json::Value, bool)> {
        let _admission = self.enter_lifecycle()?;
        let _hydration_admission = self.hydration_admission.read();
        let key = Self::canonical_key(project_root);
        if !Path::new(&key).is_dir() {
            return Err(M1ndError::InvalidParams {
                tool: "ingest".into(),
                detail: format!(
                    "project_root '{project_root}' is not a directory on this machine — \
                     the one-call bootstrap ingests a local repo root"
                ),
            });
        }

        let hydration_lock = self.hydration_lock(&key);
        let (brain, reused) = {
            let _hydration = hydration_lock.lock();
            let existing = self.try_resolve_locked(&key)?;
            let reused = existing.is_some();
            let brain = match existing {
                Some(brain) => brain,
                None => {
                    // OVERLAP GUARD (field friction 2026-07-10: two twin brains for one
                    // project — a session opened in a repo's PARENT folder minted a second
                    // brain that re-ingested the repo from above; a git worktree of a
                    // brained repo grew its own orphan brain). BEFORE minting a brand-new
                    // brain, refuse a root that OVERLAPS an existing project brain — a
                    // child/parent directory of one, or a git worktree of a repo that
                    // already has a brain — unless the caller explicitly opts in. One repo
                    // must not grow two brains by accident: that doubles auto-ingest cost
                    // and fragments memories across stores. The escape hatch is a routing
                    // directive like `project_root` (stripped before the inner ingest,
                    // below): `allow_overlap:true` skips the guard and mints anyway — the
                    // exact same root stays warm-reuse, never a refusal (that is the `Some`
                    // arm above, which the guard never reaches).
                    let allow_overlap = ingest_args
                        .get("allow_overlap")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                    if !allow_overlap {
                        let existing_roots = self.existing_brain_roots();
                        match detect_root_overlap(&key, &existing_roots) {
                            RootOverlap::None => {}
                            overlap => return Err(overlap_refusal(&key, &overlap)),
                        }
                    }
                    let (state, recovery) = self.boot_store_with_recovery(&key)?;
                    // Birth record for warm-boots (inert data only). Counts stamped
                    // after ingest below so a DORMANT store still reports its size.
                    self.write_manifest(&key, None, None)?;
                    let built = Arc::new(BrainSessionCell::new(state));
                    // Through the eviction gate: bootstrapping brain #cap+1 persists
                    // then drops the LRU victim before this new brain lands, so the
                    // map never exceeds the cap (§C9.1).
                    self.insert_with_eviction_recovery(key.clone(), built, recovery)?
                }
            };
            (brain, reused)
        };

        // Ingest the caller's repo into ITS brain — the same dispatch path any
        // agent ingest takes, so adapter/include_dotfiles options ride along.
        let mut args = ingest_args.clone();
        if let Some(map) = args.as_object_mut() {
            map.remove("project_root");
            // `allow_overlap` is a routing directive for the mint decision, not an
            // ingest adapter input — strip it exactly like `project_root`.
            map.remove("allow_overlap");
            map.insert("path".into(), serde_json::Value::String(key.clone()));
        }
        let actor_args = args.clone();
        let actor_key = key.clone();
        let ingest_result =
            self.execute_target_m1nd(brain.clone(), Some(&key), false, true, move |state| {
                let prior_caller_root = state.caller_root.replace(actor_key);
                let dispatched = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::server::dispatch_tool(state, "ingest", &actor_args)
                }));
                state.caller_root = prior_caller_root;
                match dispatched {
                    Ok(result) => result,
                    Err(payload) => std::panic::resume_unwind(payload),
                }
            })?;

        // Stamp the ingested size into the manifest — the CHEAP source the Hall
        // reads for a dormant project brain's counts (parsing the multi-MB
        // graph_snapshot on a list call is banned). A project brain lives
        // in-process, warm-booted lazily: it has no "running" state and no lock,
        // so the Hall shows these recorded counts + freshness, never an
        // instance's process status.
        let node_count = ingest_result.get("node_count").and_then(|v| v.as_u64());
        let edge_count = ingest_result.get("edge_count").and_then(|v| v.as_u64());
        let _ = self.write_manifest(&key, node_count, edge_count);

        // The bootstrap write is not considered durable until the actor returns
        // an ACK bound to this brain/version. Failure is propagated; the warm
        // brain remains present and its actor is poisoned rather than pretending
        // the checkpoint landed.
        self.checkpoint_brain(&key)?;

        Ok((brain, ingest_result, reused))
    }

    /// THE EVICTION GATE (§C9.1). Insert `built` under `key`, persisting-then-
    /// dropping least-recently-used project brains first so the warm map never
    /// exceeds `capacity`. The bound dev graph is not in this map, so it is never
    /// a candidate — only project brains evict.
    ///
    /// Concurrency: a racer may have inserted `key` while `built` was booting
    /// outside the lock (both call sites boot before calling here). First insert
    /// wins — we return the incumbent and let `built` drop unpersisted (its store
    /// on disk is unchanged; nothing was mutated in it). A victim remains in the
    /// map until its persist succeeds and the registry proves that no request is
    /// still holding it. A failed or concurrently-busy victim therefore rejects
    /// the insertion instead of silently dropping newer in-memory state.
    fn insert_with_eviction(
        &self,
        key: String,
        built: Arc<BrainSessionCell>,
    ) -> M1ndResult<Arc<BrainSessionCell>> {
        self.insert_with_eviction_recovery(key, built, None)
    }

    fn insert_with_eviction_recovery(
        &self,
        key: String,
        built: Arc<BrainSessionCell>,
        recovery: Option<BrainBootRecovery>,
    ) -> M1ndResult<Arc<BrainSessionCell>> {
        // Persist is intentionally outside the registry lock: graph snapshots can
        // be large, and unrelated brain lookups must remain available. The map
        // entry itself stays present until the post-persist compare-and-remove.
        // A small bounded retry covers races without turning pressure into a spin.
        let max_attempts = self.capacity.saturating_mul(2).max(2);
        for _ in 0..max_attempts {
            let (victim_key, victim, expected_last_used, runtime_cell, victim_recovery) = {
                let mut map = self.brains.lock();
                if let Some(warm) = map.get_mut(&key) {
                    // Racer won — adopt the incumbent, touch it, discard `built`.
                    warm.last_used = self.next_tick();
                    return Ok(warm.brain.clone());
                }
                if map.len() < self.capacity {
                    map.insert(
                        key.clone(),
                        WarmBrain {
                            brain: built.clone(),
                            last_used: self.next_tick(),
                            runtime: Arc::new(OnceLock::new()),
                            recovery: recovery.clone(),
                        },
                    );
                    return Ok(built);
                }
                let (victim_key, warm) = map
                    .iter()
                    .min_by_key(|(_, warm)| warm.last_used)
                    .expect("a full positive-capacity brain map has an LRU victim");
                (
                    victim_key.clone(),
                    warm.brain.clone(),
                    warm.last_used,
                    warm.runtime.clone(),
                    warm.recovery.clone(),
                )
            };

            let runtime = self.runtime_for_parts(
                &victim_key,
                victim.clone(),
                &runtime_cell,
                victim_recovery,
            )?;
            // Freeze actor admission before checking active workers. A prepare
            // that has not yet registered cannot later enqueue a commit; an
            // already registered job makes this victim ineligible.
            runtime.pause().map_err(brain_runtime_m1nd_error)?;
            if self.brain_has_active_jobs(runtime.brain_id())? {
                runtime.resume();
                continue;
            }
            let eviction_ack = match runtime.checkpoint_while_paused() {
                Ok(ack) => ack,
                Err(error) => {
                    runtime.resume();
                    return Err(M1ndError::PersistenceFailed(format!(
                        "project brain '{victim_key}' could not checkpoint before eviction: {error}"
                    )));
                }
            };
            eviction_ack
                .eviction_permit(
                    runtime.brain_id(),
                    eviction_ack.epoch,
                    eviction_ack.generation,
                    eviction_ack.revision,
                )
                .map_err(|error| {
                    runtime.resume();
                    M1ndError::PersistenceFailed(format!(
                        "project brain '{victim_key}' checkpoint ACK did not permit eviction: {error}"
                    ))
                })?;

            let mut map = self.brains.lock();
            if let Some(warm) = map.get_mut(&key) {
                warm.last_used = self.next_tick();
                runtime.resume();
                return Ok(warm.brain.clone());
            }
            let unchanged = map.get(&victim_key).is_some_and(|warm| {
                Arc::ptr_eq(&warm.brain, &victim) && warm.last_used == expected_last_used
            });
            // Exactly four strong refs means only the map, this eviction
            // attempt, the actor state, and its single-writer activation fence
            // own the brain. Any additional ref is a caller; removing the map
            // entry would leave that caller holding an orphan identity. The map
            // lock prevents a new resolver from acquiring a ref between this
            // count and compare-and-remove, while `pause` prevents actor work.
            let idle = Arc::strong_count(&victim) == 4;
            if unchanged && idle {
                map.remove(&victim_key);
                drop(map);
                runtime
                    .stop_while_paused()
                    .map_err(brain_runtime_m1nd_error)?;
                // The map entry, actor cell, and this attempt were the only
                // owners admitted by the idle proof. Drop every one before a
                // later same-root hydration can reacquire its exclusive lease.
                drop(runtime);
                drop(runtime_cell);
                drop(victim);
                // Re-enter the loop: another inserter may have filled the slot
                // after removal, so insertion still goes through the same gate.
                continue;
            }
            drop(map);
            runtime.resume();
        }

        Err(M1ndError::PersistenceFailed(format!(
            "project brain capacity {} is saturated by concurrently active brains; no checkpointed idle victim was evicted",
            self.capacity
        )))
    }

    /// Write (or refresh) the store manifest. Records the project root (identity),
    /// the birth time (kept stable across refreshes), and the last known graph
    /// size + refresh time — the cheap, honest source for a DORMANT project
    /// brain's Hall counts. Inert data only (no binary paths, no exec).
    fn write_manifest(
        &self,
        canonical_root: &str,
        node_count: Option<u64>,
        edge_count: Option<u64>,
    ) -> M1ndResult<()> {
        let path = self.store_dir_for(canonical_root).join(MANIFEST_FILE);
        // Preserve the original created_ms across refreshes.
        let created_ms = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v["created_ms"].as_u64())
            .unwrap_or_else(crate::util::now_ms);
        let mut record = serde_json::json!({
            "schema": "m1nd-project-brain-v0",
            "project_root": canonical_root,
            "brain_kind": "project",
            "created_ms": created_ms,
        });
        if let (Some(n), Some(e)) = (node_count, edge_count) {
            record["node_count"] = serde_json::json!(n);
            record["edge_count"] = serde_json::json!(e);
            record["updated_ms"] = serde_json::json!(crate::util::now_ms());
        }
        std::fs::create_dir_all(self.store_dir_for(canonical_root))?;
        std::fs::write(&path, serde_json::to_string_pretty(&record)?)?;
        Ok(())
    }

    /// Register a project brain on disk so the routing layer can MOUNT it: write
    /// its `project_brain.json` birth record (identity + `brain_kind: project`)
    /// through the SAME `write_manifest` path a bootstrap uses, keyed by the given
    /// root. Idempotent: a store that already carries a matching manifest is left
    /// untouched (so counts a real ingest stamped survive). Returns the canonical
    /// key registered.
    ///
    /// This is the fix for the M5a-migration orphan (field report 2026-07-05T22:31):
    /// the offline `--medulla-migrate apply` moves `.light.md` files into a store dir
    /// but is pure-filesystem (holds no `SessionState`), so it cannot register the
    /// brain itself. The CLI seam calls this after a successful `apply` so the moved
    /// memories become reachable via `resolve`/`knows` (`manifest_matches`) instead
    /// of sitting in an unmountable store.
    pub fn ensure_registered(&self, root: &str) -> M1ndResult<String> {
        let key = Self::canonical_key(root);
        if !self.manifest_matches(&key) {
            // No manifest (or a stale one for a different root) → write the birth
            // record. `write_manifest` preserves an existing `created_ms`.
            self.write_manifest(&key, None, None)?;
        }
        Ok(key)
    }

    /// The store base dir (`<owner runtime_root>/project-brains`) — surfaced for
    /// diagnostics/tests and for the Hall's project-brain name resolution.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Live `(node_count, edge_count)` for a project brain that is warm in the
    /// map RIGHT NOW — the freshest truth for the Hall. `None` when the brain is
    /// dormant on disk (the caller then falls back to the manifest's recorded
    /// counts). Locks the map only briefly, then the brain's graph read-lock.
    pub fn warm_counts(&self, canonical_root: &str) -> Option<(u64, u64)> {
        let key = Self::canonical_key(canonical_root);
        self.brains.lock().get(&key)?;
        self.runtime_for_key(&key)
            .ok()?
            .try_read_snapshot(|state| {
                let graph = state.graph.read();
                Ok((graph.num_nodes() as u64, graph.num_edges() as u64))
            })
            .ok()
            .map(|snapshot| snapshot.value)
    }

    /// Live per-brain aliveness for a project brain that is warm in the map RIGHT
    /// NOW — the R14 partition source (TWO-TIER §9.5.1). Returns
    /// `(attached_sessions, query_count, calibration_armed)` read from the brain's
    /// OWN [`SessionState`]: its distinct wire-session count, its own
    /// `queries_processed`, and whether its calibration table is armed. `None` when
    /// the brain is dormant on disk — a dormant brain has no live wire sessions, so
    /// the caller renders these ABSENT (never a fabricated 0; TT-INV-2). Locks the
    /// map briefly, then the brain lock — never across an `.await`.
    pub fn warm_session_stats(&self, canonical_root: &str) -> Option<(u64, u64, bool)> {
        let key = Self::canonical_key(canonical_root);
        self.brains.lock().get(&key)?;
        self.runtime_for_key(&key)
            .ok()?
            .try_read_snapshot(|state| {
                Ok((
                    state.sessions.len() as u64,
                    state.queries_processed,
                    state.calibration_armed(),
                ))
            })
            .ok()
            .map(|snapshot| snapshot.value)
    }

    /// The COLD roster: every project brain this owner has ON DISK, read only from
    /// each store's inert `project_brain.json` manifest (never the multi-MB
    /// snapshot — listing ≠ warm-boot). This is the fix for the "hosted brain
    /// vanishes from the Hall after a restart" bug: the instance registry only
    /// re-lists a project brain once a routed call warm-boots it, but a brain that
    /// exists on disk is a brain the Hall must show (and `?brain=` can open) with
    /// zero routed calls. The caller unions this with the warm/registry view.
    ///
    /// Returns `(canonical_root, StoreFacts, store_dir)` per readable manifest.
    /// A store whose manifest is missing/unreadable is silently skipped (honest
    /// absence, never a fabricated entry). Inert read only (PRD §9.4 posture).
    pub fn disk_roster(&self) -> Vec<(String, StoreFacts, PathBuf)> {
        let Ok(entries) = std::fs::read_dir(&self.base_dir) else {
            return Vec::new(); // no project-brains dir yet → empty roster
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let store_dir = entry.path();
            if !store_dir.is_dir() {
                continue;
            }
            if let Some(facts) = store_facts_for_store(&store_dir) {
                let key = Self::canonical_key(&facts.project_root);
                out.push((key, facts, store_dir));
            }
        }
        out
    }

    /// Every project-brain root this owner knows RIGHT NOW — live in the warm map
    /// UNION dormant on disk — as canonical keys. This is the overlap guard's
    /// input: a would-be-new mint is classified against all of them. A warm brain
    /// always has a manifest on disk (it is written before the map insert), so the
    /// disk roster is a superset in practice; the union is defensive, not load-
    /// bearing. Inert read only (map keys + roster manifests, never a warm-boot).
    fn existing_brain_roots(&self) -> Vec<String> {
        let mut set: std::collections::HashSet<String> =
            self.brains.lock().keys().cloned().collect();
        for (root, _facts, _dir) in self.disk_roster() {
            set.insert(root);
        }
        set.into_iter().collect()
    }

    /// RECONNECT-REBIND (§C5.4, ladder R13). Given a `caller_root` that neither
    /// matches the bound graph nor resolves to a brain of its own, ask the disk
    /// roster: is there exactly ONE known project brain related to this caller by
    /// ancestry — the caller is UNDER a brain's root (a monorepo subdir), or a
    /// brain's root is UNDER the caller (the host was launched from a dir ABOVE the
    /// repo, the letter#49 shape where `caller_root` collapsed to the host cwd)?
    ///
    /// Returns that brain's canonical root when the relation is UNAMBIGUOUS —
    /// exactly one roster entry is on the caller's ancestry chain in either
    /// direction. Returns `None` when zero relate (a genuine unknown repo → the
    /// plain reception, unchanged) OR when more than one relate (ambiguous: nested
    /// brains / a workspace over several repos — the front desk must not fabricate a
    /// single pick; honesty over a guess). An exact-match root is NOT a rebind
    /// candidate here — that path is a silent bind, handled before this consult.
    ///
    /// Inert read only (roster manifests only, never a warm-boot) — a pure
    /// classification the routing seam layers onto the mismatch reception.
    pub fn covering_brain(&self, caller_root: &str) -> Option<String> {
        let caller_key = Self::canonical_key(caller_root);
        let caller_path = Path::new(&caller_key);
        let mut related: Vec<String> = Vec::new();
        for (brain_key, _facts, _dir) in self.disk_roster() {
            if brain_key == caller_key {
                // Exact match is a silent bind, not a rebind candidate — skip so it
                // can never surface as a mismatch suggestion.
                continue;
            }
            let brain_path = Path::new(&brain_key);
            // Related when one path is an ancestor of the other (either direction).
            let related_pair = path_starts_with_loosely(caller_path, brain_path)
                || path_starts_with_loosely(brain_path, caller_path);
            if related_pair && !related.iter().any(|r| r == &brain_key) {
                related.push(brain_key);
            }
        }
        match related.as_slice() {
            [only] => Some(only.clone()),
            _ => None, // 0 = unknown repo, >1 = ambiguous → honest plain reception
        }
    }
}

/// Loose ancestry test (canonicalize + `/`-normalize + trailing-slash-safe prefix),
/// mirroring `SessionState::path_starts_with_loosely` so the reconnect roster
/// consult uses the SAME "is this path under that root" rule the reception mismatch
/// guard and the Two-Tier routing layer already share (one definition of "covers").
fn path_starts_with_loosely(path: &Path, root: &Path) -> bool {
    if root.as_os_str().is_empty() {
        return false;
    }
    if path.starts_with(root) {
        return true;
    }
    if let (Ok(path), Ok(root)) = (path.canonicalize(), root.canonicalize()) {
        if path.starts_with(root) {
            return true;
        }
    }
    let path_text = normalized_path_for_compare(path);
    let root_text = normalized_path_for_compare(root);
    if path_text == root_text {
        return true;
    }
    path_text.starts_with(&format!("{root_text}/"))
}

fn normalized_path_for_compare(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}

fn brain_runtime_m1nd_error(error: BrainRuntimeError) -> M1ndError {
    M1ndError::PersistenceFailed(format!("{}: {error}", error.code()))
}

fn remaining_shutdown_time(deadline: Instant, phase: &str) -> M1ndResult<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        Err(M1ndError::PersistenceFailed(format!(
            "owner shutdown deadline expired before {phase}"
        )))
    } else {
        Ok(remaining)
    }
}

fn checkpoint_actor_before_deadline(
    runtime: Arc<BrainActorHandle>,
    deadline: Instant,
) -> Result<CheckpointAckV1, BrainRuntimeError> {
    if deadline <= Instant::now() {
        return Err(BrainRuntimeError::Worker(format!(
            "actor '{}' checkpoint was not started after the shutdown deadline",
            runtime.brain_id()
        )));
    }
    let brain_id = runtime.brain_id().to_string();
    let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name(format!("m1nd-checkpoint-{brain_id}"))
        .spawn(move || {
            let _ = reply_tx.send(runtime.checkpoint_while_paused());
        })
        .map_err(|error| {
            BrainRuntimeError::Worker(format!(
                "could not start bounded checkpoint worker for '{brain_id}': {error}"
            ))
        })?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    reply_rx
        .recv_timeout(remaining)
        .map_err(|error| match error {
            std::sync::mpsc::RecvTimeoutError::Timeout => BrainRuntimeError::Worker(format!(
                "actor '{brain_id}' did not checkpoint before the shutdown deadline"
            )),
            std::sync::mpsc::RecvTimeoutError::Disconnected => BrainRuntimeError::Worker(format!(
                "actor '{brain_id}' checkpoint worker disconnected before reporting an ACK"
            )),
        })?
}

fn stop_paused_actor_before_deadline(
    runtime: Arc<BrainActorHandle>,
    deadline: Instant,
) -> Result<(), BrainRuntimeError> {
    if deadline <= Instant::now() {
        return Err(BrainRuntimeError::Worker(format!(
            "actor '{}' stop was not started after the shutdown deadline",
            runtime.brain_id()
        )));
    }
    let brain_id = runtime.brain_id().to_string();
    let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name(format!("m1nd-stop-{brain_id}"))
        .spawn(move || {
            let _ = reply_tx.send(runtime.stop_while_paused());
        })
        .map_err(|error| {
            BrainRuntimeError::Worker(format!(
                "could not start bounded stop worker for '{brain_id}': {error}"
            ))
        })?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    reply_rx
        .recv_timeout(remaining)
        .map_err(|error| match error {
            std::sync::mpsc::RecvTimeoutError::Timeout => BrainRuntimeError::Worker(format!(
                "actor '{brain_id}' did not stop before the shutdown deadline"
            )),
            std::sync::mpsc::RecvTimeoutError::Disconnected => BrainRuntimeError::Worker(format!(
                "actor '{brain_id}' stop worker disconnected before reporting completion"
            )),
        })?
}

fn runtime_job_m1nd_error(error: RuntimeJobError) -> M1ndError {
    M1ndError::PersistenceFailed(format!("runtime job refused: {error}"))
}

/// How a would-be-new project root OVERLAPS an existing project brain. The mint
/// path refuses every non-[`RootOverlap::None`] class unless the caller passes
/// `allow_overlap`, so one repo never grows two brains by accident (double auto-
/// ingest cost + memories fragmented across stores). Comparison is always between
/// canonical roots; the exact same root (`key == existing`) is NOT an overlap —
/// that is the warm-reuse path, handled before the guard is ever consulted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootOverlap {
    /// No existing brain overlaps this root — minting is safe.
    None,
    /// The new root is INSIDE an existing brain's root (a subdirectory of a repo
    /// that already has its own brain). `existing` is that ancestor brain's root.
    Child { existing: String },
    /// An existing brain's root is INSIDE the new root (the new root is a PARENT
    /// folder of a repo that already has a brain — the mother-folder trap that
    /// re-ingests the child repo from above).
    Parent { existing: String },
    /// The new root is a git WORKTREE whose main repository already has a brain.
    /// `existing` is the conflicting brain root (often the main repo itself);
    /// `main_repo` is the shared repository the worktree checks out.
    Worktree { existing: String, main_repo: String },
}

/// Classify whether minting a brain for `key` would OVERLAP any brain in
/// `existing_roots` (every project-brain root this owner knows, live + on disk).
/// A pure classification over canonical paths — the guardrail the mint path
/// consults before it mints a second brain for a repo that already has one.
///
/// Order: direct containment (child/parent) is checked first against every
/// existing root; a git-worktree relation — siblings that share ONE repo, which
/// containment cannot see — is checked last. The only filesystem read is the
/// worktree probe (`<key>/.git`); child/parent is pure string comparison. Inputs
/// are canonicalized defensively so a caller need not pre-normalize.
pub fn detect_root_overlap(key: &str, existing_roots: &[String]) -> RootOverlap {
    let key = ProjectBrainRegistry::canonical_key(key);

    // 1. Direct containment against every existing brain root.
    for existing in existing_roots {
        let existing = ProjectBrainRegistry::canonical_key(existing);
        if existing == key {
            continue; // the exact same root is warm-reuse, never an overlap
        }
        if is_strict_descendant(&key, &existing) {
            return RootOverlap::Child { existing };
        }
        if is_strict_descendant(&existing, &key) {
            return RootOverlap::Parent { existing };
        }
    }

    // 2. Worktree: `<key>/.git` is a gitdir FILE → does its main repo have a brain?
    if let Some(main_repo) = worktree_main_repo(&key) {
        for existing in existing_roots {
            let existing = ProjectBrainRegistry::canonical_key(existing);
            let main_has_brain = existing == main_repo
                || is_strict_descendant(&main_repo, &existing)
                || is_strict_descendant(&existing, &main_repo);
            if main_has_brain {
                return RootOverlap::Worktree {
                    existing,
                    main_repo,
                };
            }
        }
    }

    RootOverlap::None
}

/// True when `path` is STRICTLY inside `root` (a proper descendant): canonical,
/// slash-normalized, trailing-slash-safe. Unlike [`path_starts_with_loosely`]
/// this is FALSE for equal paths — the exact-root case is warm-reuse, handled by
/// the caller, never an overlap.
fn is_strict_descendant(path: &str, root: &str) -> bool {
    let path = normalized_path_for_compare(Path::new(path));
    let root = normalized_path_for_compare(Path::new(root));
    if root.is_empty() || path == root {
        return false;
    }
    path.starts_with(&format!("{root}/"))
}

/// If `key` is a git WORKTREE, return its MAIN repository root; else `None`. A
/// worktree's `.git` is a FILE (`gitdir: <path>`), not a directory; when that
/// gitdir sits under `<main>/.git/worktrees/<name>`, the main repo is the prefix
/// before `/.git/worktrees/`. A relative gitdir is resolved against `key`, and
/// the returned root is canonicalized so it compares equal to a stored brain key
/// (macOS `/tmp` → `/private/tmp`, symlinks resolved). Inert read only.
fn worktree_main_repo(key: &str) -> Option<String> {
    let dot_git = Path::new(key).join(".git");
    // A real repo has `.git` as a DIRECTORY; only a worktree (or submodule) points
    // via a FILE. `symlink_metadata` so a symlinked `.git` is judged by the link.
    if !std::fs::symlink_metadata(&dot_git).ok()?.is_file() {
        return None;
    }
    let content = std::fs::read_to_string(&dot_git).ok()?;
    let target = content
        .lines()
        .next()?
        .trim()
        .strip_prefix("gitdir:")?
        .trim()
        .to_string();
    let gitdir = if Path::new(&target).is_absolute() {
        PathBuf::from(&target)
    } else {
        Path::new(key).join(&target)
    };
    const MARKER: &str = "/.git/worktrees/";
    let gitdir = normalized_path_for_compare(&gitdir);
    let idx = gitdir.find(MARKER)?;
    Some(ProjectBrainRegistry::canonical_key(&gitdir[..idx]))
}

/// The honest refusal for an overlapping mint (returned as an `ingest`
/// `InvalidParams` so it reaches the caller as a tool error). Names the class +
/// the conflicting existing root, teaches the TWO ways forward, and states the
/// cost. Mirrors the `synthetic:true` posture of `mission_post`: refuse by
/// default, an explicit escape hatch, a message that points at the RIGHT call.
fn overlap_refusal(key: &str, overlap: &RootOverlap) -> M1ndError {
    let (class, existing, relation) = match overlap {
        RootOverlap::Child { existing } => (
            "child",
            existing.as_str(),
            format!("is INSIDE '{existing}', which already has its own project brain"),
        ),
        RootOverlap::Parent { existing } => (
            "parent",
            existing.as_str(),
            format!("CONTAINS '{existing}', which already has its own project brain"),
        ),
        RootOverlap::Worktree { existing, main_repo } => (
            "worktree",
            existing.as_str(),
            format!(
                "is a git worktree of '{main_repo}', whose repository already has a project brain (rooted at '{existing}')"
            ),
        ),
        RootOverlap::None => {
            // Never built for None; stay total instead of panicking on a bad call.
            return M1ndError::InvalidParams {
                tool: "ingest".into(),
                detail: "internal: overlap_refusal called with no overlap".into(),
            };
        }
    };
    M1ndError::InvalidParams {
        tool: "ingest".into(),
        detail: format!(
            "overlap_{class}: project_root '{key}' {relation} — refused so this owner does not \
             mint a second, duplicate brain for the same repo. Two ways forward: \
             (a) bind to the existing brain: call ingest with project_root={existing} \
             (the usual case — you opened in the wrong directory); \
             (b) if you truly want a separate brain for this overlapping root, pass allow_overlap:true. \
             Duplicated brains double auto-ingest cost and fragment memories."
        ),
    }
}

/// The real project root a store belongs to, read from its `project_brain.json`
/// manifest. This is how the Hall recovers a hosted brain's true identity: a
/// project brain's registry entry stores its FINGERPRINT store dir as its
/// `workspace_root` (the hash that leaked into the Hall), while the manifest in
/// that store names the repo it actually maps. `None` = no readable manifest
/// (not a resolvable project brain). Inert read only — no exec, no binary paths
/// (PRD §9.4 posture).
pub fn project_root_for_store(store_dir: &Path) -> Option<String> {
    store_facts_for_store(store_dir).map(|f| f.project_root)
}

/// The cheap Hall facts for a project brain store, read ONLY from its inert
/// `project_brain.json` manifest (never the multi-MB snapshot): identity +
/// last-recorded size + freshness. `node_count`/`edge_count` are `None` for a
/// pre-counts manifest (honest absence, not zero); the Hall then shows counts
/// only if the brain is warm in the map. `None` = no readable manifest.
pub fn store_facts_for_store(store_dir: &Path) -> Option<StoreFacts> {
    let text = std::fs::read_to_string(store_dir.join(MANIFEST_FILE)).ok()?;
    let record = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    let project_root = record["project_root"].as_str()?.to_string();
    Some(StoreFacts {
        project_root,
        node_count: record["node_count"].as_u64(),
        edge_count: record["edge_count"].as_u64(),
        // Freshness floor: the last recorded update, else the birth time.
        updated_ms: record["updated_ms"]
            .as_u64()
            .or_else(|| record["created_ms"].as_u64()),
    })
}

/// Inert facts about a project brain, from its manifest (see
/// [`store_facts_for_store`]).
#[derive(Clone, Debug)]
pub struct StoreFacts {
    pub project_root: String,
    pub node_count: Option<u64>,
    pub edge_count: Option<u64>,
    pub updated_ms: Option<u64>,
}

#[cfg(test)]
// Persistence fixtures exercise real embedding-cache/checkpoint I/O. Match the
// owner's five-second shutdown budget so workspace stress cannot turn scheduler
// delay into a false lifecycle failure; dedicated deadline tests stay tighter.
const TEST_PERSISTING_ACTOR_SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

#[cfg(test)]
mod external_mutation_actor_binding_tests {
    use super::*;
    use serde_json::json;

    fn session_cell(runtime_root: &Path, workspace_root: &Path) -> Arc<BrainSessionCell> {
        std::fs::create_dir_all(runtime_root).expect("actor binding runtime");
        let config = crate::server::McpConfig {
            graph_source: runtime_root.join("graph.json"),
            plasticity_state: runtime_root.join("plasticity.json"),
            runtime_dir: Some(runtime_root.to_path_buf()),
            ..Default::default()
        };
        let mut state = crate::server::McpServer::new(config)
            .expect("actor binding server")
            .into_session_state();
        state.workspace_root = Some(workspace_root.to_string_lossy().into_owned());
        state.ingest_roots = vec![workspace_root.to_string_lossy().into_owned()];
        Arc::new(BrainSessionCell::new(state))
    }

    #[test]
    fn hosted_actor_binding_survives_restart_and_refuses_unknown_or_foreign_target() {
        let temp = tempfile::tempdir().expect("actor binding tempdir");
        let repo = temp.path().join("hosted-repo");
        std::fs::create_dir_all(repo.join("src")).expect("hosted repo");
        std::fs::write(repo.join("src/lib.rs"), "pub fn hosted() -> u8 { 1 }\n")
            .expect("hosted source");
        let repo = repo.canonicalize().expect("canonical hosted repo");
        let repo_text = repo.to_string_lossy().into_owned();
        let base = temp.path().join("project-brains");
        let bound_root = temp.path().join("bound-owner");
        std::fs::create_dir_all(&bound_root).expect("bound root");
        let bound = session_cell(&temp.path().join("bound-runtime"), &bound_root);

        let registry = ProjectBrainRegistry::new(base.clone(), None);
        let (hosted, _, _) = registry
            .bootstrap(&repo_text, &json!({"agent_id": "actor-binding"}))
            .expect("bootstrap hosted actor");
        let first = registry
            .resolve_external_mutation_actor(Arc::clone(&bound), &repo_text)
            .expect("resolve hosted actor");
        assert!(!first.bound);
        assert!(Arc::ptr_eq(&first.brain, &hosted));
        assert_eq!(first.brain_id, registry.brain_id_for(&repo_text));
        let expected_brain_id = first.brain_id.clone();
        drop(first);
        drop(hosted);
        registry
            .shutdown(TEST_PERSISTING_ACTOR_SHUTDOWN_GRACE)
            .expect("shutdown first hosted registry");
        drop(registry);

        let restarted = ProjectBrainRegistry::new(base, None);
        let rebound = restarted
            .resolve_external_mutation_actor(Arc::clone(&bound), &repo_text)
            .expect("warm-restart hosted actor binding");
        assert!(!rebound.bound);
        assert_eq!(rebound.brain_id, expected_brain_id);
        assert_eq!(
            rebound.selected_project_root.as_deref(),
            Some(repo_text.as_str())
        );
        let exact_by_id = restarted
            .resolve_external_mutation_actor_by_id(Arc::clone(&bound), &expected_brain_id)
            .expect("durable recovery actor id resolves exactly");
        assert_eq!(exact_by_id.brain_id, expected_brain_id);
        assert!(Arc::ptr_eq(&exact_by_id.brain, &rebound.brain));
        assert!(
            restarted
                .resolve_external_mutation_actor_by_id(Arc::clone(&bound), &repo_text)
                .is_err(),
            "recovery seam must never reinterpret a root as an actor id"
        );

        let unknown = temp.path().join("unknown-repo");
        std::fs::create_dir_all(&unknown).expect("unknown repo");
        assert!(restarted
            .resolve_external_mutation_actor(Arc::clone(&bound), &unknown.to_string_lossy())
            .is_err());

        let foreign = session_cell(&temp.path().join("foreign-runtime"), &unknown);
        let mismatch = restarted.execute_target_runtime(
            foreign,
            rebound.selected_project_root.as_deref(),
            false,
            false,
            |_state| Ok(()),
        );
        assert!(
            mismatch.is_err(),
            "foreign Arc must not inherit hosted actor identity"
        );
        drop(rebound);
        restarted
            .shutdown(TEST_PERSISTING_ACTOR_SHUTDOWN_GRACE)
            .expect("shutdown restarted hosted registry");
    }

    #[test]
    fn domain_error_is_returned_exactly_and_partial_mutation_is_rolled_back() {
        let temp = tempfile::tempdir().expect("domain rollback tempdir");
        let root = temp.path().join("bound");
        std::fs::create_dir_all(&root).expect("bound root");
        let session = session_cell(&temp.path().join("runtime"), &root);
        let registry = ProjectBrainRegistry::new(temp.path().join("brains"), None);

        let error = registry
            .execute_target_m1nd(Arc::clone(&session), None, true, true, |state| {
                state.queries_processed = 77;
                Err::<(), _>(M1ndError::InvalidParams {
                    tool: "fixture".to_string(),
                    detail: "refuse after partial mutation".to_string(),
                })
            })
            .expect_err("domain refusal must cross the actor as an error");
        assert!(matches!(
            error,
            M1ndError::InvalidParams { ref tool, ref detail }
                if tool == "fixture" && detail == "refuse after partial mutation"
        ));

        let observed = registry
            .execute_target_runtime(Arc::clone(&session), None, true, false, |state| {
                Ok(state.queries_processed)
            })
            .expect("read rolled-back state");
        assert_eq!(
            observed, 0,
            "an erroring command cannot checkpoint its partial postimage"
        );
        registry
            .shutdown(Duration::from_secs(2))
            .expect("shutdown rollback fixture");
    }
}

#[cfg(test)]
mod hydration_single_flight_tests {
    use super::*;
    use serde_json::json;
    use std::sync::Barrier;

    #[test]
    fn concurrent_dormant_resolve_reuses_one_session_and_one_instance_owner() {
        let temp = tempfile::tempdir().expect("hydration single-flight tempdir");
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("fixture repo");
        std::fs::write(repo.join("src/lib.rs"), "pub fn fixture() -> u8 { 1 }\n")
            .expect("fixture source");
        let repo = repo.canonicalize().expect("canonical fixture repo");
        let root = repo.to_string_lossy().into_owned();
        let base = temp.path().join("project-brains");
        let registry_dir = temp.path().join("registry");

        // Birth and checkpoint the store, then discard the warm process state so
        // every racing resolver starts from the exact dormant path.
        let first = ProjectBrainRegistry::new(base.clone(), Some(registry_dir.clone()));
        let (brain, _, reused) = first
            .bootstrap(&root, &json!({"agent_id": "single-flight-birth"}))
            .expect("birth project brain");
        assert!(!reused);
        drop(brain);
        first
            .shutdown(TEST_PERSISTING_ACTOR_SHUTDOWN_GRACE)
            .expect("checkpoint dormant fixture");
        drop(first);

        let registry = Arc::new(ProjectBrainRegistry::new(base, Some(registry_dir.clone())));
        let racers = 8;
        let barrier = Arc::new(Barrier::new(racers));
        let joins = (0..racers)
            .map(|_| {
                let registry = Arc::clone(&registry);
                let barrier = Arc::clone(&barrier);
                let root = root.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    registry
                        .try_resolve(&root)
                        .expect("concurrent resolve must not lose the instance race")
                        .expect("dormant manifest must resolve")
                })
            })
            .collect::<Vec<_>>();
        let brains = joins
            .into_iter()
            .map(|join| join.join().expect("resolver thread"))
            .collect::<Vec<_>>();
        for brain in brains.iter().skip(1) {
            assert!(
                Arc::ptr_eq(&brains[0], brain),
                "all same-root resolvers must adopt the single-flight winner"
            );
        }

        let store = registry.store_dir_for(&root);
        let expected_runtime_root = store
            .canonicalize()
            .expect("canonical single-flight store")
            .to_string_lossy()
            .into_owned();
        let listed = crate::instance_registry::list_instances(Some(&registry_dir))
            .expect("list isolated registry");
        let owners = listed
            .iter()
            .filter(|entry| {
                entry.runtime_root == expected_runtime_root
                    && entry.mode == "read_write"
                    && !entry.stale
            })
            .count();
        assert_eq!(
            owners, 1,
            "one dormant root may have only one live owner; listed={listed:#?}"
        );

        drop(brains);
        registry
            .shutdown(TEST_PERSISTING_ACTOR_SHUTDOWN_GRACE)
            .expect("shutdown single-flight winner");
    }
}

#[cfg(test)]
mod lifecycle_shutdown_tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn bound_fixture(
        temporary: &tempfile::TempDir,
    ) -> (Arc<ProjectBrainRegistry>, Arc<BrainSessionCell>) {
        let runtime = temporary.path().join("bound-runtime");
        let registry_dir = temporary.path().join("registry");
        let state = crate::server::McpServer::new(crate::server::McpConfig {
            graph_source: runtime.join("graph_snapshot.json"),
            plasticity_state: runtime.join("plasticity_state.json"),
            runtime_dir: Some(runtime.clone()),
            registry_dir: Some(registry_dir.clone()),
            ..Default::default()
        })
        .expect("bound owner")
        .into_session_state();
        (
            Arc::new(ProjectBrainRegistry::new(
                runtime.join(PROJECT_BRAINS_DIR),
                Some(registry_dir),
            )),
            Arc::new(BrainSessionCell::new(state)),
        )
    }

    #[test]
    fn shutdown_closes_transport_and_retained_job_admission_before_drain() {
        let temporary = tempfile::tempdir().expect("shutdown fixture");
        let (registry, session) = bound_fixture(&temporary);
        let jobs = registry
            .runtime_job_registry()
            .expect("runtime job registry");
        assert!(jobs.health_snapshot().expect("job health").accepting);

        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(0);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        let inflight_registry = Arc::clone(&registry);
        let inflight_session = Arc::clone(&session);
        let inflight = std::thread::spawn(move || {
            inflight_registry.execute_target_m1nd(
                inflight_session,
                None,
                true,
                false,
                move |_state| {
                    entered_tx.send(()).expect("announce admitted callback");
                    release_rx.recv().expect("release admitted callback");
                    Ok(())
                },
            )
        });
        entered_rx.recv().expect("callback entered");

        let (shutdown_tx, shutdown_rx) = std::sync::mpsc::sync_channel(1);
        let shutdown_registry = Arc::clone(&registry);
        let shutdown = std::thread::spawn(move || {
            shutdown_tx
                .send(shutdown_registry.shutdown(Duration::from_secs(3)))
                .expect("report shutdown");
        });

        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if !registry.lifecycle.lock().accepting {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "shutdown admission fence did not close"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !jobs.health_snapshot().expect("fenced job health").accepting,
            "a retained job-registry clone must close at the same lifecycle fence"
        );

        let rejected_ran = Arc::new(AtomicBool::new(false));
        let rejected_marker = Arc::clone(&rejected_ran);
        let rejection = registry
            .execute_target_m1nd(Arc::clone(&session), None, true, false, move |_state| {
                rejected_marker.store(true, Ordering::SeqCst);
                Ok(())
            })
            .expect_err("new transport work must be refused after the fence");
        assert!(rejection.to_string().contains("shutting down"));
        assert!(!rejected_ran.load(Ordering::SeqCst));
        assert!(
            matches!(
                shutdown_rx.try_recv(),
                Err(std::sync::mpsc::TryRecvError::Empty)
            ),
            "shutdown cannot complete while admitted work remains active"
        );

        release_tx.send(()).expect("release inflight callback");
        inflight
            .join()
            .expect("inflight thread")
            .expect("inflight callback");
        let acks = shutdown_rx
            .recv()
            .expect("shutdown result")
            .expect("checkpointed shutdown");
        shutdown.join().expect("shutdown thread");
        assert_eq!(acks.len(), 1, "the bound actor must return one final ACK");
        session
            .lock_mut_before_actor()
            .expect("bound actor returned session")
            .instance
            .release()
            .expect("release bound owner");
    }

    #[test]
    fn shutdown_releases_hosted_instance_despite_stale_arc_and_permit() {
        let temporary = tempfile::tempdir().expect("hosted shutdown fixture");
        let repo = temporary.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("fixture repo");
        std::fs::write(repo.join("src/lib.rs"), "pub fn fixture() -> u8 { 1 }\n")
            .expect("fixture source");
        let root = repo
            .canonicalize()
            .expect("canonical repo")
            .to_string_lossy()
            .into_owned();
        let base = temporary.path().join("project-brains");
        let registry_dir = temporary.path().join("registry");
        let first = ProjectBrainRegistry::new(base.clone(), Some(registry_dir.clone()));
        let (stale_brain, _, reused) = first
            .bootstrap(&root, &json!({"agent_id": "shutdown-stale-arc"}))
            .expect("bootstrap hosted brain");
        assert!(!reused);
        let (old_summary, old_permit) = first
            .execute_target_m1nd(
                Arc::clone(&stale_brain),
                Some(&root),
                false,
                false,
                |state| Ok((state.instance.summary(), state.instance.heartbeat_permit())),
            )
            .expect("capture hosted lifecycle facts");

        let acks = first
            .shutdown(Duration::from_secs(3))
            .expect("shutdown hosted owner");
        assert_eq!(acks.len(), 1);
        assert!(
            crate::instance_registry::list_instances(Some(&registry_dir))
                .expect("list after hosted shutdown")
                .iter()
                .all(|entry| entry.instance_id != old_summary.instance_id),
            "hosted discovery entry must be removed while stale Arc remains alive"
        );
        assert!(!old_permit.heartbeat().expect("old permit verdict"));
        assert!(
            stale_brain.try_lock().is_some(),
            "stopped actor must return SessionState into the stale cell"
        );

        let successor = ProjectBrainRegistry::new(base, Some(registry_dir.clone()));
        let successor_brain = successor
            .try_resolve(&root)
            .expect("warm resolve successor")
            .expect("hosted manifest survives shutdown");
        let successor_summary = successor_brain
            .read()
            .expect("successor is not actor-owned yet")
            .instance
            .summary();
        assert!(
            crate::instance_registry::list_instances(Some(&registry_dir))
                .expect("list successor")
                .iter()
                .any(|entry| {
                    entry.instance_id == successor_summary.instance_id
                        && entry.runtime_root == successor_summary.runtime_root
                        && !entry.stale
                }),
            "a successor must acquire and publish the same store even while the stale Arc lives"
        );
        assert!(
            !old_permit
                .heartbeat()
                .expect("old permit remains revoked after successor"),
            "revoked permit cannot overwrite a successor owner"
        );
        successor
            .shutdown(Duration::from_secs(3))
            .expect("shutdown successor");
    }
}

#[cfg(test)]
mod eviction_gate_tests {
    use super::*;

    /// PERSIST-ON-EVICT teeth (§C9.1). The kill-9 battery case in
    /// `tests/two_tier_project_brains.rs` proves the map bound + that every brain
    /// warm-boots after a hard kill; but bootstrap auto-persists, so that test
    /// cannot isolate the persist-on-evict step from the bootstrap persist. This
    /// unit test does: it mutates a brain's IN-MEMORY graph AFTER its last persist
    /// (a node added directly, exactly the shape of any non-auto-persisting graph
    /// mutation like `learn`/`apply`), then forces its eviction and asserts the
    /// mutation reached the on-disk snapshot — i.e. the eviction gate flushed it.
    ///
    /// RED without persist-on-evict: the victim is dropped with the added node
    /// still only in memory, its store snapshot is never written, and the reload
    /// below finds no snapshot (0 nodes) — the exact "16:44 at scale" data loss
    /// the gate exists to prevent.
    #[test]
    fn eviction_persists_unpersisted_state() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let base = tmp.path().join("project-brains");
        // Cap 1: inserting a second brain must evict the first.
        let reg = ProjectBrainRegistry::with_capacity(base, None, 1);

        let root_a = tmp.path().join("repo-a").to_string_lossy().to_string();
        let root_b = tmp.path().join("repo-b").to_string_lossy().to_string();
        let key_a = ProjectBrainRegistry::canonical_key(&root_a);
        let key_b = ProjectBrainRegistry::canonical_key(&root_b);

        // Boot brain A (fresh empty graph, its snapshot path under A's store).
        let mut state_a = reg.boot_store(&key_a).expect("boot A");
        let store_a = reg.store_dir_for(&key_a);
        let snapshot_a = store_a.join("graph_snapshot.json");
        assert!(
            !snapshot_a.exists(),
            "precondition: A has no snapshot on disk yet"
        );

        // Mutate A's IN-MEMORY graph AFTER any persist — this state exists ONLY in
        // memory until something flushes it.
        {
            let mut g = state_a.graph.write();
            g.add_node(
                "evict::sentinel",
                "evict_sentinel",
                m1nd_core::types::NodeType::Function,
                &[],
                0.0,
                0.0,
            )
            .expect("add sentinel node");
            // Rebuild the CSR so the graph is query/persist-ready (the ingest path
            // does this; a raw add_node leaves the CSR stale).
            g.finalize().expect("finalize A's graph");
        }
        // A raw graph mutation must rebuild every graph-sized derived engine
        // before the strict checkpoint conservation fence can persist it.
        state_a
            .rebuild_engines()
            .expect("rebuild graph-sized sidecars for checkpoint");
        let brain_a = Arc::new(BrainSessionCell::new(state_a));
        reg.insert_with_eviction(key_a.clone(), brain_a)
            .expect("insert A");
        assert_eq!(reg.warm_len(), 1, "A is the sole warm brain");
        assert!(
            !snapshot_a.exists(),
            "A's mutation is still only in memory — no snapshot yet"
        );

        // Insert brain B → cap is 1 → A (the LRU, and only) is evicted. The gate
        // MUST persist A before dropping it.
        let state_b = reg.boot_store(&key_b).expect("boot B");
        let brain_b = Arc::new(BrainSessionCell::new(state_b));
        reg.insert_with_eviction(key_b.clone(), brain_b)
            .expect("checkpoint A then insert B");

        assert_eq!(reg.warm_len(), 1, "map stays at the cap after B lands");
        assert!(
            reg.warm_counts(&key_b).is_some(),
            "B is the surviving warm brain"
        );
        assert!(
            reg.warm_counts(&key_a).is_none(),
            "A was evicted from the warm map"
        );

        // THE PROOF: A's on-disk snapshot now exists AND carries the sentinel node
        // added after its last persist — persist-on-evict flushed it.
        assert!(
            snapshot_a.exists(),
            "persist-on-evict must have written A's snapshot before dropping it"
        );
        let reloaded = m1nd_core::snapshot::load_graph(&snapshot_a).expect("reload A's store");
        let sentinel = reloaded
            .strings
            .lookup("evict::sentinel")
            .and_then(|interned| reloaded.id_to_node.get(&interned));
        assert!(
            sentinel.is_some(),
            "A's evicted snapshot must contain the node mutated after its last \
             persist — persist-on-evict is the only thing that could have saved it"
        );
    }

    /// A checkpoint error must be a failed insertion, never a warning followed
    /// by eviction. The old best-effort gate removed A first and then logged the
    /// failed persist, irreversibly orphaning A's newest in-memory state.
    #[test]
    fn failed_checkpoint_keeps_victim_warm_and_rejects_new_brain() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let reg = ProjectBrainRegistry::with_capacity(tmp.path().join("pb"), None, 1);
        let key_a =
            ProjectBrainRegistry::canonical_key(&tmp.path().join("repo-a").to_string_lossy());
        let key_b =
            ProjectBrainRegistry::canonical_key(&tmp.path().join("repo-b").to_string_lossy());

        let mut state_a = reg.boot_store(&key_a).expect("boot A");
        // A regular file cannot be the parent of graph_snapshot.tmp, making the
        // graph checkpoint fail deterministically on every supported filesystem.
        let blocked_parent = reg
            .store_dir_for(&key_a)
            .join("checkpoint-parent-is-a-file");
        std::fs::write(&blocked_parent, b"not a directory").expect("write blocker");
        state_a.graph_path = blocked_parent.join("graph_snapshot.json");
        reg.insert_with_eviction(key_a.clone(), Arc::new(BrainSessionCell::new(state_a)))
            .expect("insert A");

        let state_b = reg.boot_store(&key_b).expect("boot B");
        let error = match reg
            .insert_with_eviction(key_b.clone(), Arc::new(BrainSessionCell::new(state_b)))
        {
            Ok(_) => panic!("B must not land when A's checkpoint fails"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("could not checkpoint before eviction"),
            "failure names the checkpoint boundary: {error}"
        );
        assert_eq!(reg.warm_len(), 1, "failed insertion cannot exceed the cap");
        assert!(
            reg.brains.lock().contains_key(&key_a),
            "A remains the fenced authoritative map entry after its checkpoint failure"
        );
        assert!(
            reg.warm_counts(&key_b).is_none(),
            "B is not inserted after a failed checkpoint"
        );
    }

    /// The public routing resolver must preserve the same refusal. Treating this
    /// as `None` would make callers believe B is merely unknown and could route a
    /// write to the bound/default brain after A failed to checkpoint.
    #[test]
    fn try_resolve_propagates_checkpoint_failure_instead_of_falling_back() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let reg = ProjectBrainRegistry::with_capacity(tmp.path().join("pb"), None, 1);
        let key_a =
            ProjectBrainRegistry::canonical_key(&tmp.path().join("repo-a").to_string_lossy());
        let key_b =
            ProjectBrainRegistry::canonical_key(&tmp.path().join("repo-b").to_string_lossy());

        let mut state_a = reg.boot_store(&key_a).expect("boot A");
        let blocked_parent = reg
            .store_dir_for(&key_a)
            .join("checkpoint-parent-is-a-file");
        std::fs::write(&blocked_parent, b"not a directory").expect("write blocker");
        state_a.graph_path = blocked_parent.join("graph_snapshot.json");
        reg.insert_with_eviction(key_a.clone(), Arc::new(BrainSessionCell::new(state_a)))
            .expect("insert A");
        reg.write_manifest(&key_b, Some(0), Some(0))
            .expect("register dormant B");

        let error = match reg.try_resolve(&key_b) {
            Ok(_) => panic!("B resolution must fail closed when A cannot checkpoint"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("could not checkpoint before eviction"),
            "resolution names the durability refusal: {error}"
        );
        assert!(
            reg.brains.lock().contains_key(&key_a),
            "the failed victim remains authoritative and fenced in the warm map"
        );
        assert!(
            reg.warm_counts(&key_b).is_none(),
            "the newcomer cannot land after a failed checkpoint"
        );
    }

    /// The bound dev graph is not in this map, and eviction only ever touches
    /// project brains: a cap of K holds at most K project brains, no matter how
    /// many distinct roots resolve through the registry.
    #[test]
    fn map_never_exceeds_capacity() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let reg = ProjectBrainRegistry::with_capacity(tmp.path().join("pb"), None, 3);
        for i in 0..10 {
            let key = ProjectBrainRegistry::canonical_key(
                &tmp.path().join(format!("r{i}")).to_string_lossy(),
            );
            let state = reg.boot_store(&key).expect("boot");
            reg.insert_with_eviction(key, Arc::new(BrainSessionCell::new(state)))
                .expect("checkpoint LRU then insert");
            assert!(
                reg.warm_len() <= 3,
                "warm map exceeded cap after insert {i}: {}",
                reg.warm_len()
            );
        }
        assert_eq!(reg.warm_len(), 3, "map sits at the cap after churn");
    }

    /// RECONNECT-REBIND roster consult (§C5.4, ladder R13). `covering_brain` reads
    /// the disk roster and returns the UNIQUE brain related to a caller by ancestry —
    /// the classification the routing seam layers onto a mismatch reception so an
    /// existing brain is preferred over the host cwd. This pins every branch:
    ///   - caller UNDER a brain root (monorepo subdir) → that brain;
    ///   - brain root UNDER the caller (the letter#49 host-cwd shape) → that brain;
    ///   - no relation → None (unknown repo, plain reception);
    ///   - >1 related → None (ambiguous, honesty over a guess);
    ///   - an EXACT match → None (a silent bind, never a rebind suggestion).
    #[test]
    fn covering_brain_prefers_the_unique_related_brain_and_abstains_on_ambiguity() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let reg = ProjectBrainRegistry::with_capacity(tmp.path().join("pb"), None, 8);

        // A brain on disk for `<tmp>/workspace/repo-a`. Create the dir so its key
        // canonicalizes to the SAME spelling the roster reports (macOS /tmp →
        // /private/tmp), then write its manifest — the ONLY input covering_brain
        // reads (no warm-boot).
        let workspace = tmp.path().join("workspace");
        let repo_a = workspace.join("repo-a");
        std::fs::create_dir_all(&repo_a).expect("mk repo-a");
        let key_a = ProjectBrainRegistry::canonical_key(&repo_a.to_string_lossy());
        reg.write_manifest(&key_a, Some(1), Some(0))
            .expect("manifest A");

        // (letter#49 shape) caller = the workspace ABOVE the repo → the repo brain.
        assert_eq!(
            reg.covering_brain(&workspace.to_string_lossy()),
            Some(key_a.clone()),
            "a brain root UNDER the caller (host-cwd-above-repo) must be found"
        );

        // caller = a subdir INSIDE the repo → the repo brain (monorepo subdir shape).
        let subdir = repo_a.join("src").join("deep");
        std::fs::create_dir_all(&subdir).expect("mk subdir");
        assert_eq!(
            reg.covering_brain(&subdir.to_string_lossy()),
            Some(key_a.clone()),
            "a caller UNDER a brain root must be found"
        );

        // caller = the brain root EXACTLY → None (that is a silent bind, not a rebind).
        assert_eq!(
            reg.covering_brain(&repo_a.to_string_lossy()),
            None,
            "an exact-match root is a silent bind, never a mismatch suggestion"
        );

        // caller = an unrelated sibling → None (genuine unknown repo).
        let stranger = tmp.path().join("elsewhere").join("stranger");
        std::fs::create_dir_all(&stranger).expect("mk stranger");
        assert_eq!(
            reg.covering_brain(&stranger.to_string_lossy()),
            None,
            "an unrelated root has no covering brain (plain reception)"
        );

        // Add a SECOND brain also under the workspace → the workspace now relates to
        // two brains → ambiguous → None (the front desk must not fabricate a pick).
        let repo_b = workspace.join("repo-b");
        std::fs::create_dir_all(&repo_b).expect("mk repo-b");
        let key_b = ProjectBrainRegistry::canonical_key(&repo_b.to_string_lossy());
        reg.write_manifest(&key_b, Some(1), Some(0))
            .expect("manifest B");
        assert_eq!(
            reg.covering_brain(&workspace.to_string_lossy()),
            None,
            "two brains under one caller root is ambiguous → honest None, not a guess"
        );
        // But a caller inside repo-a still resolves uniquely to repo-a (repo-b is not
        // on its ancestry chain).
        assert_eq!(
            reg.covering_brain(&subdir.to_string_lossy()),
            Some(key_a),
            "a caller deep inside one repo still resolves to that repo unambiguously"
        );
    }
}

#[cfg(test)]
mod daemon_rearm_tests {
    //! Gardener v1 (verdict leg 2): the per-brain daemon SURVIVES the LRU
    //! eviction gate. `insert_with_eviction` drops the whole SessionState (and
    //! with it any live watcher) — "armed today, dead in a week, no alert".
    //! The re-arm lives on the registry's warm-boot/resolve path: `boot_store`
    //! → `SessionState::initialize` → `load_daemon_state` resumes `active`
    //! (transient flags sanitized), and the next routed call's traffic tick
    //! advances it. Lazy by construction: the resume never scans — the first
    //! traffic tick does the inventory work, after the listener, on demand.
    use super::*;
    use serde_json::json;

    fn write_tiny_repo(root: &Path) {
        std::fs::create_dir_all(root.join("src")).expect("mk src");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"tiny\"\nversion = \"0.0.0\"\n",
        )
        .expect("Cargo.toml");
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn tiny_probe() -> i64 { 1 }\n",
        )
        .expect("lib.rs");
    }

    /// eviction→rearm: arm the daemon on a hosted brain (watch_paths defaulting
    /// to the BRAIN's ingest_roots, per the verdict), evict the brain through the
    /// real LRU gate, re-resolve it, and prove the daemon comes back armed AND
    /// ticks on the same transport seam the served owner uses per routed call.
    #[test]
    fn evicted_brains_armed_daemon_rearms_on_the_next_resolve() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // Cap 1: bootstrapping a second brain MUST evict the first.
        let reg = ProjectBrainRegistry::with_capacity(tmp.path().join("pb"), None, 1);

        let root_a = tmp.path().join("repo-a");
        write_tiny_repo(&root_a);
        let key_a = ProjectBrainRegistry::canonical_key(&root_a.to_string_lossy());
        let (brain_a, _ingest, reused) = reg
            .bootstrap(&root_a.to_string_lossy(), &json!({"agent_id": "t"}))
            .expect("bootstrap brain A");
        assert!(!reused, "A is a fresh mint");

        // ARM the daemon on brain A through the routed verb path, with EMPTY
        // watch_paths — the verdict's per-brain default: the brain's own
        // ingest_roots become the watch set.
        let (started_active, watched, pre_evict_call_ok, tick_count) = reg
            .execute_target_runtime(Arc::clone(&brain_a), Some(&key_a), false, true, |a| {
                let started = crate::server::dispatch_tool(
                    a,
                    "daemon_start",
                    &json!({"agent_id": "t", "poll_interval_ms": 1}),
                )
                .map_err(|error| {
                    RuntimeJobFailure::new("daemon_start_failed", error.to_string())
                })?;
                let watched = started["watch_paths"]
                    .as_array()
                    .ok_or_else(|| {
                        RuntimeJobFailure::new(
                            "daemon_watch_paths_missing",
                            "daemon_start omitted watch_paths",
                        )
                    })?
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .collect::<Vec<_>>();

                // One REAL traffic tick before the eviction — the lived
                // shape: an armed brain that has already advanced, then
                // goes cold.
                a.daemon_state.last_tick_ms = Some(0);
                let request = crate::protocol::core::JsonRpcRequest {
                    jsonrpc: "2.0".into(),
                    id: json!(0),
                    method: "tools/call".into(),
                    params: json!({
                        "name": "health",
                        "arguments": { "agent_id": "t" }
                    }),
                };
                let response = crate::server::handle_mcp_method(a, &request);
                Ok((
                    started["active"] == true,
                    watched,
                    response.error.is_none(),
                    a.daemon_state.tick_count,
                ))
            })
            .expect("daemon work crosses the hosted actor");
        assert!(started_active, "daemon_start arms the hosted brain");
        assert!(
            watched.iter().any(|path| path.contains("repo-a")),
            "empty watch_paths must default to the BRAIN's ingest_roots (got {watched:?})"
        );
        assert!(pre_evict_call_ok, "pre-evict traffic call succeeds");
        assert!(tick_count >= 1, "A ticked before eviction");
        drop(brain_a);

        // EVICT A through the real gate: cap is 1, so bootstrapping B drops A.
        let root_b = tmp.path().join("repo-b");
        write_tiny_repo(&root_b);
        reg.bootstrap(&root_b.to_string_lossy(), &json!({"agent_id": "t"}))
            .expect("bootstrap brain B evicts A");
        assert!(
            reg.warm_counts(&key_a).is_none(),
            "precondition: A was evicted from the warm map"
        );

        // RE-RESOLVE A: the daemon must come back ARMED (resume on the
        // warm-boot path) and TICK on the next traffic.
        let revived = reg.resolve(&key_a).expect("warm-boot A from its store");
        let (active, tick_in_flight, pending_rerun, before, after, trigger, call_ok) = reg
            .execute_target_runtime(revived, Some(&key_a), false, true, |a| {
                let active = a.daemon_state.active;
                let tick_in_flight = a.daemon_state.tick_in_flight;
                let pending_rerun = a.daemon_state.pending_rerun;

                // The SAME transport seam a routed call takes: the traffic
                // autotick, now inside dispatch_tool, reached here via
                // handle_mcp_method.
                a.daemon_state.last_tick_ms = Some(0);
                let before = a.daemon_state.tick_count;
                let request = crate::protocol::core::JsonRpcRequest {
                    jsonrpc: "2.0".into(),
                    id: json!(1),
                    method: "tools/call".into(),
                    params: json!({
                        "name": "health",
                        "arguments": { "agent_id": "t" }
                    }),
                };
                let response = crate::server::handle_mcp_method(a, &request);
                Ok((
                    active,
                    tick_in_flight,
                    pending_rerun,
                    before,
                    a.daemon_state.tick_count,
                    a.daemon_state.last_tick_trigger.clone(),
                    response.error.is_none(),
                ))
            })
            .expect("revived daemon work crosses the hosted actor");
        assert!(
            active,
            "the armed daemon must survive eviction via its store's daemon_state"
        );
        assert!(
            !tick_in_flight && !pending_rerun,
            "transient flags are sanitized on the warm-boot resume"
        );
        assert!(call_ok, "routed call must succeed");
        assert!(
            after > before,
            "the revived brain's daemon must tick on traffic"
        );
        assert_eq!(
            trigger.as_deref(),
            Some("traffic"),
            "freshness-by-traffic: the tick trigger is the routed call"
        );
    }
}

#[cfg(test)]
mod overlap_guard_tests {
    //! THE OVERLAP GUARD (field friction 2026-07-10: twin brains for one project).
    //! A brain existed for a repo; a session opened in the repo's PARENT folder and
    //! minted a SECOND brain that re-ingested the repo from above (double cost,
    //! fragmented memories); separately a git WORKTREE of a brained repo grew its own
    //! orphan brain. Law: before minting a NEW brain, the mint path refuses a root
    //! that OVERLAPS an existing brain (child / parent / worktree) unless the caller
    //! passes `allow_overlap:true`. The exact same root stays warm-reuse, never a
    //! refusal. These cases are the field incident turned into a battery.
    use super::*;
    use serde_json::json;

    /// A tiny but non-empty repo so a real bootstrap ingest produces > 0 nodes.
    fn write_tiny_repo(root: &Path) {
        std::fs::create_dir_all(root.join("src")).expect("mk src");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"tiny\"\nversion = \"0.0.0\"\n",
        )
        .expect("Cargo.toml");
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn tiny_probe() -> i64 { 1 }\n",
        )
        .expect("lib.rs");
    }

    fn reg_in(tmp: &Path) -> ProjectBrainRegistry {
        ProjectBrainRegistry::with_capacity(tmp.join("project-brains"), None, 8)
    }

    /// The refusal must be an `ingest` InvalidParams that names the class, the
    /// conflicting existing root, the bind-to-existing call, and the escape hatch.
    fn assert_overlap_refusal(err: &M1ndError, class: &str, conflicting: &str) {
        match err {
            M1ndError::InvalidParams { tool, detail } => {
                assert_eq!(tool, "ingest", "an overlap refusal is an ingest error");
                assert!(
                    detail.contains(&format!("overlap_{class}")),
                    "refusal must name the '{class}' class: {detail}"
                );
                assert!(
                    detail.contains(conflicting),
                    "refusal must name the conflicting root '{conflicting}': {detail}"
                );
                assert!(
                    detail.contains("project_root="),
                    "refusal must teach the bind-to-existing call: {detail}"
                );
                assert!(
                    detail.contains("allow_overlap"),
                    "refusal must teach the allow_overlap escape hatch: {detail}"
                );
            }
            other => panic!("expected an InvalidParams overlap refusal, got {other:?}"),
        }
    }

    /// bootstrap and REQUIRE a refusal (avoids `expect_err`, which would need the Ok
    /// tuple to be Debug — SessionState is not).
    fn bootstrap_expecting_refusal(
        reg: &ProjectBrainRegistry,
        root: &Path,
        args: &serde_json::Value,
    ) -> M1ndError {
        match reg.bootstrap(&root.to_string_lossy(), args) {
            Ok(_) => panic!(
                "expected an overlap refusal for {}, but it minted",
                root.display()
            ),
            Err(e) => e,
        }
    }

    // ---- (1)-(6): the field incident as a battery, through the real bootstrap ---

    /// (1) THE CHERRY CASE. A brain exists for `<tmp>/a/b`; opening in the PARENT
    /// `<tmp>/a` and bootstrapping is refused with the `parent` class, naming the
    /// existing child root — before any second brain is minted.
    #[test]
    fn parent_overlap_refuses_naming_the_existing_child() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let child = tmp.path().join("a").join("b");
        std::fs::create_dir_all(&child).expect("mk child");
        let parent = tmp.path().join("a");

        let reg = reg_in(tmp.path());
        let child_key = reg
            .ensure_registered(&child.to_string_lossy())
            .expect("register the existing child brain");

        let err = bootstrap_expecting_refusal(&reg, &parent, &json!({"agent_id": "t"}));
        assert_overlap_refusal(&err, "parent", &child_key);
        // Nothing minted: the parent has no brain and the warm map is untouched.
        assert!(
            !reg.knows(&parent.to_string_lossy()),
            "the parent must NOT have been minted after a refusal"
        );
        assert_eq!(reg.warm_len(), 0, "no brain is warm after a refusal");
    }

    /// (2) A brain exists for `<tmp>/a`; opening in the CHILD `<tmp>/a/b` (a monorepo
    /// subdir) and bootstrapping is refused with the `child` class.
    #[test]
    fn child_overlap_refuses() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let parent = tmp.path().join("a");
        let child = parent.join("b");
        std::fs::create_dir_all(&child).expect("mk child");

        let reg = reg_in(tmp.path());
        let parent_key = reg
            .ensure_registered(&parent.to_string_lossy())
            .expect("register the existing parent brain");

        let err = bootstrap_expecting_refusal(&reg, &child, &json!({"agent_id": "t"}));
        assert_overlap_refusal(&err, "child", &parent_key);
    }

    /// (3) A brain exists for a real git repo; opening in one of its git WORKTREES
    /// and bootstrapping is refused with the `worktree` class. Real git; skipped only
    /// where git is unavailable (the pure gitdir logic is proven separately below).
    #[test]
    fn worktree_overlap_refuses() {
        if std::process::Command::new("git")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("skipping worktree_overlap_refuses: git is not on PATH");
            return;
        }
        let tmp = tempfile::tempdir().expect("tempdir");
        let main = tmp.path().join("mainrepo");
        std::fs::create_dir_all(&main).expect("mk main");
        let git = |dir: &Path, args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .output()
                .expect("spawn git");
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&main, &["init", "-q"]);
        git(&main, &["config", "user.email", "t@example.invalid"]);
        git(&main, &["config", "user.name", "tester"]);
        std::fs::write(main.join("f.txt"), "x").expect("seed file");
        git(&main, &["add", "."]);
        git(&main, &["commit", "-q", "-m", "init"]);
        let wt = tmp.path().join("wt");
        git(&main, &["worktree", "add", "-q", wt.to_str().unwrap()]);
        assert!(
            wt.join(".git").is_file(),
            "precondition: a worktree's .git must be a gitdir FILE"
        );

        let reg = reg_in(tmp.path());
        let main_key = reg
            .ensure_registered(&main.to_string_lossy())
            .expect("register the main-repo brain");

        let err = bootstrap_expecting_refusal(&reg, &wt, &json!({"agent_id": "t"}));
        assert_overlap_refusal(&err, "worktree", &main_key);
    }

    /// (4) THE ESCAPE HATCH. The Cherry case with `allow_overlap:true` mints anyway —
    /// a real, separate brain for the overlapping parent root.
    #[test]
    fn allow_overlap_true_mints_anyway() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let child = tmp.path().join("a").join("b");
        std::fs::create_dir_all(&child).expect("mk child");
        let parent = tmp.path().join("a");
        write_tiny_repo(&parent);

        let reg = reg_in(tmp.path());
        reg.ensure_registered(&child.to_string_lossy())
            .expect("register the existing child brain");

        let (_brain, _ingest, reused) = reg
            .bootstrap(
                &parent.to_string_lossy(),
                &json!({"agent_id": "t", "allow_overlap": true}),
            )
            .expect("allow_overlap:true must mint over a detected overlap");
        assert!(!reused, "an overlap mint is a NEW brain, not a warm reuse");
        assert!(
            reg.knows(&parent.to_string_lossy()),
            "the parent brain must exist after an allow_overlap mint"
        );
    }

    /// (5) NO REGRESSION on the common path: a disjoint root — no containment and no
    /// worktree relation to any existing brain — mints normally.
    #[test]
    fn disjoint_root_mints_normally() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let existing = tmp.path().join("repo-x");
        std::fs::create_dir_all(&existing).expect("mk repo-x");
        let fresh = tmp.path().join("repo-y");
        write_tiny_repo(&fresh);

        let reg = reg_in(tmp.path());
        reg.ensure_registered(&existing.to_string_lossy())
            .expect("register an unrelated existing brain");

        let (_brain, _ingest, reused) = reg
            .bootstrap(&fresh.to_string_lossy(), &json!({"agent_id": "t"}))
            .expect("a disjoint root must mint normally (no false positive)");
        assert!(!reused, "a fresh disjoint root is a new brain");
        assert!(
            reg.knows(&fresh.to_string_lossy()),
            "the disjoint brain must exist after minting"
        );
    }

    /// (6) THE REGRESSION GUARD the overlap check must NEVER trip: bootstrapping the
    /// EXACT SAME root twice is warm-reuse the second time, never a refusal.
    #[test]
    fn same_root_warm_reuse_never_refuses() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path().join("repo");
        write_tiny_repo(&root);
        let reg = reg_in(tmp.path());

        let (_b1, _i1, reused1) = reg
            .bootstrap(&root.to_string_lossy(), &json!({"agent_id": "t"}))
            .expect("first bootstrap mints the brain");
        assert!(!reused1, "the first bootstrap is a fresh mint");

        let (_b2, _i2, reused2) = reg
            .bootstrap(&root.to_string_lossy(), &json!({"agent_id": "t"}))
            .expect("bootstrapping the SAME root again must NOT be refused as an overlap");
        assert!(
            reused2,
            "the second bootstrap of the same root is warm-reuse, never a refusal"
        );
    }

    // ---- Pure detection unit tests (no ingest, no git binary) ------------------

    /// child / parent / disjoint / exact-match, over canonical paths.
    #[test]
    fn detect_root_overlap_classifies_containment() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let a = tmp.path().join("a");
        let b = a.join("b");
        std::fs::create_dir_all(&b).expect("mk a/b");
        let c = tmp.path().join("c");
        std::fs::create_dir_all(&c).expect("mk c");
        let a_key = ProjectBrainRegistry::canonical_key(&a.to_string_lossy());
        let b_key = ProjectBrainRegistry::canonical_key(&b.to_string_lossy());
        let c_key = ProjectBrainRegistry::canonical_key(&c.to_string_lossy());

        // new root INSIDE an existing brain root → Child.
        assert_eq!(
            detect_root_overlap(&b_key, std::slice::from_ref(&a_key)),
            RootOverlap::Child {
                existing: a_key.clone()
            }
        );
        // an existing brain root INSIDE the new root → Parent.
        assert_eq!(
            detect_root_overlap(&a_key, std::slice::from_ref(&b_key)),
            RootOverlap::Parent {
                existing: b_key.clone()
            }
        );
        // disjoint sibling → None.
        assert_eq!(
            detect_root_overlap(&c_key, std::slice::from_ref(&a_key)),
            RootOverlap::None
        );
        // the exact same root is warm-reuse, never an overlap.
        assert_eq!(
            detect_root_overlap(&a_key, std::slice::from_ref(&a_key)),
            RootOverlap::None
        );
        // empty roster → None.
        assert_eq!(detect_root_overlap(&a_key, &[]), RootOverlap::None);
    }

    /// gitdir resolution + worktree classification, with a FABRICATED `.git` file
    /// (no git binary) — proves the worktree logic even where the real-git
    /// integration case above is skipped.
    #[test]
    fn detect_root_overlap_resolves_a_fabricated_worktree() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let main = tmp.path().join("mainrepo");
        std::fs::create_dir_all(&main).expect("mk main");
        let wt = tmp.path().join("wt");
        std::fs::create_dir_all(&wt).expect("mk wt");
        let main_key = ProjectBrainRegistry::canonical_key(&main.to_string_lossy());
        // A worktree's `.git` is a FILE pointing under `<main>/.git/worktrees/<name>`.
        std::fs::write(
            wt.join(".git"),
            format!("gitdir: {main_key}/.git/worktrees/wt\n"),
        )
        .expect("write the fabricated gitdir file");
        let wt_key = ProjectBrainRegistry::canonical_key(&wt.to_string_lossy());

        // The pure resolver recovers the main repo from the gitdir file.
        assert_eq!(worktree_main_repo(&wt_key), Some(main_key.clone()));
        // A plain directory (no `.git` file) is not a worktree.
        assert_eq!(worktree_main_repo(&main_key), None);

        // With a brain for the main repo, the worktree is an overlap.
        assert_eq!(
            detect_root_overlap(&wt_key, std::slice::from_ref(&main_key)),
            RootOverlap::Worktree {
                existing: main_key.clone(),
                main_repo: main_key.clone(),
            }
        );
        // With NO brain for the main repo, the worktree is free to mint.
        let unrelated =
            ProjectBrainRegistry::canonical_key(&tmp.path().join("unrelated").to_string_lossy());
        assert_eq!(
            detect_root_overlap(&wt_key, std::slice::from_ref(&unrelated)),
            RootOverlap::None
        );
    }

    /// A relative gitdir (some git setups write `../`-relative worktree pointers) is
    /// resolved against the worktree root before the main repo is extracted.
    #[test]
    fn worktree_main_repo_resolves_a_relative_gitdir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let main = tmp.path().join("mainrepo");
        std::fs::create_dir_all(main.join(".git").join("worktrees").join("wt"))
            .expect("mk main/.git/worktrees/wt");
        let wt = tmp.path().join("wt");
        std::fs::create_dir_all(&wt).expect("mk wt");
        // Relative pointer from the worktree up to the main repo's gitdir.
        std::fs::write(wt.join(".git"), "gitdir: ../mainrepo/.git/worktrees/wt\n")
            .expect("write relative gitdir file");
        let wt_key = ProjectBrainRegistry::canonical_key(&wt.to_string_lossy());
        let main_key = ProjectBrainRegistry::canonical_key(&main.to_string_lossy());
        assert_eq!(worktree_main_repo(&wt_key), Some(main_key));
    }
}
