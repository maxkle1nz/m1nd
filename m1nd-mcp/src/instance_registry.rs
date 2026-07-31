//! Process-instance discovery and writer-lease registry.
//!
//! The lifecycle capability is deliberately crate-private. External callers may
//! inspect the public registry records, but they cannot acquire, clone, release,
//! or heartbeat the process-owned writer handle.
//!
//! ```compile_fail
//! use m1nd_mcp::instance_registry::InstanceHandle;
//! ```

use crate::util::now_ms;
use m1nd_core::error::{M1ndError, M1ndResult};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, Weak};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::task::JoinHandle;
use tokio::time::{interval, Duration};

const INSTANCE_DIR_NAME: &str = "instances";
const LEASE_DIR_NAME: &str = "leases";
const DEFAULT_REGISTRY_SUBDIR: &str = ".m1nd/registry";
const STALE_AFTER_MS: u64 = 30_000;

/// Windows share mode for the crash-released lock files (`*.owner.lock`,
/// `.lease-mutations.guard`). Sharing READ lets read-only registry inspection —
/// `list_instances`, `doctor`, and durable-tree snapshots — open a lock while it
/// is held, matching Unix `flock`, which never blocks readers. A competing
/// WRITER still collides because its write access is not shared, so single-owner
/// exclusion is unchanged: `acquire_os`/`LeaseMutationGuard` still see
/// `ERROR_SHARING_VIOLATION` (32) from a second acquirer and treat it as "held".
/// Opening these files fully exclusive (`share_mode(0)`) is what made a Windows
/// reader fail with error 32 where the Unix reader succeeds.
#[cfg(windows)]
const LOCK_FILE_SHARE_MODE: u32 = windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstanceRegistryEntry {
    pub instance_id: String,
    pub workspace_root: String,
    pub runtime_root: String,
    pub graph_source: String,
    pub plasticity_state: String,
    pub pid: u32,
    pub bind: Option<String>,
    pub port: Option<u16>,
    pub started_at_ms: u64,
    pub last_heartbeat_ms: u64,
    pub mode: String,
    pub status: String,
    #[serde(default)]
    pub owner_live: Option<bool>,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub conflicts: Vec<String>,
    /// Two-Tier Brain: which kind of brain this instance hosts. `None` (the
    /// serde default, so the ~54k legacy entries parse unchanged) means the
    /// classic single bound/dev graph. `Some("project")` marks an owner-hosted
    /// per-project brain (TWO-TIER-BRAIN interim variant) so `doctor`/list can
    /// distinguish the dev graph from the project brains it also hosts.
    #[serde(default)]
    pub brain_kind: Option<String>,
}

/// Acquisition mode for an instance.
///
/// `ReadWrite` takes the exclusive PID+heartbeat lease (one per `runtime_root`),
/// exactly as before. `ReadOnly` never takes a lease: it only registers a
/// discoverable `instances/<id>.json` entry and always succeeds, even while a
/// live `ReadWrite` owner holds the lease.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InstanceMode {
    ReadWrite,
    ReadOnly,
}

impl InstanceMode {
    /// On-disk string used in the `mode` field. Kept stable for backward
    /// compatibility with the ~54k existing lease/instance JSON files.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            InstanceMode::ReadWrite => "read_write",
            InstanceMode::ReadOnly => "read_only",
        }
    }

    /// Parse the on-disk `mode` string. Anything that is not exactly
    /// `"read_only"` is treated as `ReadWrite` so legacy/unknown values keep
    /// their historical (exclusive) meaning.
    // Infallible, default-on-unknown conversion — the std `FromStr` trait would
    // force a never-used `Err` type, so an inherent method is the right shape.
    #[allow(clippy::should_implement_trait)]
    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "read_only" => InstanceMode::ReadOnly,
            _ => InstanceMode::ReadWrite,
        }
    }
}

/// Result of a dead-lease garbage-collection sweep.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GcReport {
    /// Number of files removed from the `leases/` directory.
    pub leases_removed: usize,
    /// Number of files removed from the `instances/` directory.
    pub instances_removed: usize,
    /// Total number of JSON entries inspected across both directories.
    pub scanned: usize,
}

#[derive(Debug)]
pub(crate) struct InstanceHandle {
    inner: Arc<Mutex<InstanceHandleInner>>,
}

/// Revocable, heartbeat-only projection of one unique [`InstanceHandle`].
///
/// The weak reference cannot extend the owner's lifetime, and the only allowed
/// operation checks the owner's `released` bit while holding the same mutex that
/// linearizes release and file removal.
#[derive(Debug)]
pub(crate) struct InstanceHeartbeatPermit {
    inner: Weak<Mutex<InstanceHandleInner>>,
}

#[derive(Debug)]
struct InstanceHandleInner {
    entry: InstanceRegistryEntry,
    registry_root: PathBuf,
    entry_path: PathBuf,
    /// `Some` only for `ReadWrite` handles. `ReadOnly` handles hold no
    /// exclusive lease, so they have no lease file to refresh or remove.
    lock_path: Option<PathBuf>,
    /// Crash-released, per-runtime OS lock held for the full writer lifetime.
    /// Modern contenders cannot replace a merely stale heartbeat while the
    /// original process still owns this guard. Legacy owners without the guard
    /// remain recoverable through the historical PID+heartbeat lease rule.
    owner_lifetime_guard: Option<OwnerLifetimeGuard>,
    mode: InstanceMode,
    /// Linear revocation bit. Set before release removes either registry file;
    /// every persistence path checks it under this same mutex.
    released: bool,
}

impl InstanceHandle {
    /// Acquire in the default `ReadWrite` mode. This is the unique lifecycle
    /// capability for the runtime root; same-PID duplicates are refused too.
    pub(crate) fn acquire(
        workspace_root: &Path,
        runtime_root: &Path,
        graph_source: &Path,
        plasticity_state: &Path,
        registry_root: Option<&Path>,
    ) -> M1ndResult<Self> {
        Self::acquire_with_mode(
            workspace_root,
            runtime_root,
            graph_source,
            plasticity_state,
            registry_root,
            InstanceMode::ReadWrite,
        )
    }

    /// Acquire with an explicit mode.
    ///
    /// `ReadWrite` is the exclusive PID+heartbeat lease: a same-PID duplicate is
    /// always refused, and a live, non-stale foreign owner also returns
    /// `AlreadyExists`.
    ///
    /// `ReadOnly` always succeeds and never touches the lease file. It only
    /// writes an `instances/<id>.json` entry (with `mode:"read_only"`) so the
    /// attacher is discoverable via `list_instances`. Multiple `ReadOnly`
    /// attachers and one `ReadWrite` owner coexist with zero conflict.
    pub(crate) fn acquire_with_mode(
        workspace_root: &Path,
        runtime_root: &Path,
        graph_source: &Path,
        plasticity_state: &Path,
        registry_root: Option<&Path>,
        mode: InstanceMode,
    ) -> M1ndResult<Self> {
        let workspace_root = canonicalish(workspace_root)?;
        let runtime_root = canonicalish(runtime_root)?;
        let graph_source = canonicalish(graph_source)?;
        let plasticity_state = canonicalish(plasticity_state)?;
        let registry_root = registry_root
            .map(canonicalish)
            .transpose()?
            .unwrap_or_else(default_registry_root);

        fs::create_dir_all(registry_root.join(INSTANCE_DIR_NAME))?;
        fs::create_dir_all(registry_root.join(LEASE_DIR_NAME))?;

        let lease_file = registry_root
            .join(LEASE_DIR_NAME)
            .join(format!("{}.json", fingerprint_path(&runtime_root)));

        let now_ms = now_ms();
        let instance_id = generate_instance_id(&workspace_root, &runtime_root, now_ms);
        let entry = InstanceRegistryEntry {
            instance_id: instance_id.clone(),
            workspace_root: workspace_root.to_string_lossy().to_string(),
            runtime_root: runtime_root.to_string_lossy().to_string(),
            graph_source: graph_source.to_string_lossy().to_string(),
            plasticity_state: plasticity_state.to_string_lossy().to_string(),
            pid: std::process::id(),
            bind: None,
            port: None,
            started_at_ms: now_ms,
            last_heartbeat_ms: now_ms,
            mode: mode.as_str().into(),
            status: "starting".into(),
            owner_live: Some(true),
            stale: false,
            conflicts: Vec::new(),
            brain_kind: None,
        };

        let entry_path = registry_root
            .join(INSTANCE_DIR_NAME)
            .join(format!("{}.json", instance_id));

        // ReadWrite claims the canonical lease with create_new/O_EXCL while the
        // per-lease mutation guard serializes identity-checked stale recovery.
        // ReadOnly remains discovery-only and never touches either primitive.
        let (lock_path, owner_lifetime_guard) = match mode {
            InstanceMode::ReadWrite => {
                let lifetime_guard = OwnerLifetimeGuard::acquire(&lease_file)?;
                claim_readwrite_lease(&lease_file, &entry)?;
                (Some(lease_file), Some(lifetime_guard))
            }
            InstanceMode::ReadOnly => (None, None),
        };
        if let Err(error) = save_json_atomic(&entry_path, &entry) {
            if let Some(lock_path) = &lock_path {
                // Acquisition is not published until both files exist. If the
                // discovery entry fails, relinquish only the exclusive lease
                // this attempt just created before returning the original error.
                if let Err(cleanup_error) = remove_owned_lease_file(lock_path, &entry) {
                    return Err(M1ndError::Io(std::io::Error::other(format!(
                            "instance discovery write failed: {error}; exclusive lease cleanup failed: {cleanup_error}"
                        ))));
                }
            }
            return Err(error);
        }

        Ok(Self {
            inner: Arc::new(Mutex::new(InstanceHandleInner {
                entry,
                registry_root,
                entry_path,
                lock_path,
                owner_lifetime_guard,
                mode,
                released: false,
            })),
        })
    }

    pub(crate) fn set_running_endpoint(&mut self, bind: String, port: u16) -> M1ndResult<()> {
        let mut inner = self.inner.lock();
        ensure_instance_active(&inner)?;
        inner.entry.bind = Some(bind);
        inner.entry.port = Some(port);
        inner.entry.status = "running".into();
        inner.entry.last_heartbeat_ms = now_ms();
        persist_handle_inner(&inner)
    }

    /// Withdraw an HTTP endpoint without releasing the process-owned instance.
    /// Used by a background HTTP sidecar whose stdio owner remains alive.
    pub(crate) fn clear_running_endpoint(&mut self) -> M1ndResult<()> {
        let mut inner = self.inner.lock();
        ensure_instance_active(&inner)?;
        inner.entry.bind = None;
        inner.entry.port = None;
        inner.entry.last_heartbeat_ms = now_ms();
        persist_handle_inner(&inner)
    }

    /// Two-Tier Brain: stamp the brain kind (e.g. `"project"`) onto this
    /// instance's registry entry and re-persist it, so `doctor`/`list_instances`
    /// can tell an owner-hosted per-project brain apart from the bound dev graph.
    /// The bound/dev owner never calls this, so its entry keeps `brain_kind:
    /// None`.
    ///
    /// STABLE-ID re-key (the "duplicate workspace" field bug): `acquire` mints an
    /// EPHEMERAL instance id (pid + clock + a per-process nonce) that changes on
    /// every boot. A brain entry instead takes a DETERMINISTIC id,
    /// `inst_<hash(workspace+runtime)>`, stable across clean release/reboot and
    /// able to reconcile duplicate files inherited from pre-linear-handle builds.
    /// Attachers never reach here (`ReadOnly` never calls `set_brain_kind`), so
    /// the N-attacher design keeps its ephemeral, per-attacher ids untouched.
    pub(crate) fn set_brain_kind(&mut self, brain_kind: &str) -> M1ndResult<()> {
        let mut inner = self.inner.lock();
        ensure_instance_active(&inner)?;
        let previous_entry = inner.entry.clone();
        let previous_entry_path = inner.entry_path.clone();
        let mut next_entry = previous_entry.clone();
        next_entry.brain_kind = Some(brain_kind.to_string());
        let stable_id =
            stable_brain_instance_id(&next_entry.workspace_root, &next_entry.runtime_root);
        next_entry.instance_id = stable_id.clone();
        let next_entry_path = inner
            .registry_root
            .join(INSTANCE_DIR_NAME)
            .join(format!("{stable_id}.json"));

        // Preserve the old in-memory identity/path and its discovery file until
        // the new entry is durable and the owned lease commits the re-key. A
        // pre-commit error therefore leaves Drop able to name the old lease.
        commit_brain_rekey(&inner, &previous_entry, &next_entry, &next_entry_path)?;

        // The lease write is the commit point. Publish its identity in memory
        // before best-effort cleanup so every later Drop/heartbeat names it.
        inner.entry = next_entry;
        inner.entry_path = next_entry_path;
        if previous_entry_path != inner.entry_path {
            let _ = remove_owned_registry_file(&previous_entry_path, &previous_entry);
        }

        // Reconcile away stale duplicates of this store only after commit.
        reconcile_brain_duplicates(&inner);
        Ok(())
    }

    pub(crate) fn mark_heartbeat(&mut self) -> M1ndResult<()> {
        let mut inner = self.inner.lock();
        mark_heartbeat_inner(&mut inner)
    }

    pub(crate) fn mark_degraded(&mut self) -> M1ndResult<()> {
        let mut inner = self.inner.lock();
        ensure_instance_active(&inner)?;
        inner.entry.status = "degraded".into();
        inner.entry.last_heartbeat_ms = now_ms();
        persist_handle_inner(&inner)
    }

    pub(crate) fn summary(&self) -> InstanceRegistryEntry {
        self.inner.lock().entry.clone()
    }

    pub(crate) fn registry_root(&self) -> PathBuf {
        self.inner.lock().registry_root.clone()
    }

    /// The mode this handle was acquired with.
    pub(crate) fn mode(&self) -> InstanceMode {
        self.inner.lock().mode
    }

    /// Mint the only background capability derived from this unique owner.
    /// It can refresh liveness and nothing else; it cannot keep the owner alive.
    pub(crate) fn heartbeat_permit(&self) -> InstanceHeartbeatPermit {
        InstanceHeartbeatPermit {
            inner: Arc::downgrade(&self.inner),
        }
    }

    /// Revoke this unique owner and remove only registry files that still name
    /// this acquisition. Idempotent: a second call retries any leftover cleanup.
    pub(crate) fn release(&mut self) -> M1ndResult<()> {
        let mut inner = self.inner.lock();
        // This is the linearization point. A heartbeat that completed before it
        // may have written once; we delete afterward. A heartbeat arriving after
        // it observes `released` and exits without touching the filesystem.
        inner.released = true;

        let mut first_error = None;
        // ReadOnly handles hold no lease; only the discovery entry is removed.
        if let Some(lock_path) = &inner.lock_path {
            if let Err(error) = remove_owned_lease_file(lock_path, &inner.entry) {
                first_error = Some(error);
            }
        }
        if let Err(error) = remove_owned_registry_file(&inner.entry_path, &inner.entry) {
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => {
                // Registry removal is the release commit. Only now may a
                // contender acquire the crash-released lifetime lock.
                inner.owner_lifetime_guard.take();
                Ok(())
            }
        }
    }
}

impl Drop for InstanceHandle {
    fn drop(&mut self) {
        if self.release().is_err() {
            // Fail-safe cleanup ownership: if registry removal failed, retain
            // the OS lifetime lock until process exit rather than let another
            // writer start against ambiguous files. Explicit release callers
            // can retry while the handle is alive; Drop has no such caller.
            std::mem::forget(Arc::clone(&self.inner));
        }
    }
}

impl InstanceHeartbeatPermit {
    /// `Ok(true)` means one heartbeat landed. `Ok(false)` is terminal: the
    /// unique owner was released or dropped and the task must exit.
    pub(crate) fn heartbeat(&self) -> M1ndResult<bool> {
        let Some(inner) = self.inner.upgrade() else {
            return Ok(false);
        };
        let mut inner = inner.lock();
        if inner.released {
            return Ok(false);
        }
        mark_heartbeat_inner(&mut inner)?;
        Ok(true)
    }
}

pub(crate) fn spawn_heartbeat(permit: InstanceHeartbeatPermit) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(5));
        loop {
            ticker.tick().await;
            match permit.heartbeat() {
                Ok(true) => {}
                Ok(false) => break,
                // Preserve the historical best-effort heartbeat posture for a
                // transient I/O error; release/drop is the explicit exit signal.
                Err(_) => continue,
            }
        }
    })
}

pub fn list_instances(registry_root: Option<&Path>) -> M1ndResult<Vec<InstanceRegistryEntry>> {
    let registry_root = registry_root
        .map(canonicalish)
        .transpose()?
        .unwrap_or_else(default_registry_root);
    let instances_dir = registry_root.join(INSTANCE_DIR_NAME);
    if !instances_dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();
    for item in fs::read_dir(instances_dir)? {
        let item = item?;
        let path = item.path();
        if path.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        match read_json::<InstanceRegistryEntry>(&path) {
            Ok(mut entry) => {
                entry.owner_live = Some(is_pid_live(entry.pid));
                entry.stale =
                    !entry.owner_live.unwrap_or(false) || is_stale(entry.last_heartbeat_ms);
                entries.push(entry);
            }
            Err(_) => continue,
        }
    }

    apply_conflicts(&mut entries);
    entries.sort_by(|a, b| {
        b.last_heartbeat_ms
            .cmp(&a.last_heartbeat_ms)
            .then_with(|| a.workspace_root.cmp(&b.workspace_root))
    });
    Ok(entries)
}

pub fn delete_instance_state(
    instance_id: &str,
    registry_root: Option<&Path>,
) -> M1ndResult<InstanceRegistryEntry> {
    let registry_root = registry_root
        .map(canonicalish)
        .transpose()?
        .unwrap_or_else(default_registry_root);
    let entry_path = registry_root
        .join(INSTANCE_DIR_NAME)
        .join(format!("{}.json", instance_id));
    let mut entry: InstanceRegistryEntry = read_json(&entry_path)?;
    entry.owner_live = Some(is_pid_live(entry.pid));
    entry.stale = !entry.owner_live.unwrap_or(false) || is_stale(entry.last_heartbeat_ms);

    if entry.owner_live.unwrap_or(false) {
        return Err(M1ndError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "cannot delete runtime state for live instance {} (pid {})",
                entry.instance_id, entry.pid
            ),
        )));
    }

    let runtime_root = PathBuf::from(&entry.runtime_root);
    let lease_path = registry_root
        .join(LEASE_DIR_NAME)
        .join(format!("{}.json", fingerprint_path(&runtime_root)));
    if runtime_root.exists() {
        let allowed = [
            "graph.json",
            "plasticity.json",
            "antibodies.json",
            "tremor_state.json",
            "trust_state.json",
            // savings_state.json is no longer written (savings tracker removed —
            // brand gate G1.5); kept here so legacy files get garbage-collected.
            "savings_state.json",
            "boot_memory_state.json",
            "daemon_state.json",
            "daemon_alerts.json",
            "ingest_roots.json",
            "auto_ingest_state.json",
            "document_cache.json",
            "cache_index.json",
        ];
        for name in allowed {
            let candidate = runtime_root.join(name);
            if candidate.exists() {
                let _ = fs::remove_file(candidate);
            }
        }
        if runtime_root.read_dir()?.next().is_none() {
            let _ = fs::remove_dir(&runtime_root);
        }
    }
    let _ = fs::remove_file(&entry_path);
    // Never let an explicit stale-state deletion unlink a successor that won
    // the canonical lease after this instance entry was inspected.
    let _ = remove_owned_lease_file(&lease_path, &entry);
    Ok(entry)
}

/// Garbage-collect dead lease and instance entries.
///
/// Scans both `leases/` and `instances/` under `registry_root` and removes any
/// JSON entry whose recorded `pid` is provably NOT live (via the per-sweep
/// live-pid snapshot). Entries owned by a live pid are NEVER removed. Any entry
/// that fails to read or parse is skipped (never deleted), so corrupt/foreign
/// files are left untouched. Safe to call while a live instance is running —
/// only provably-dead entries are removed.
///
/// The OS process table is read exactly ONCE per sweep (one `LivePids::snapshot`)
/// and the resulting live-pid set is reused for every entry across both
/// directories — so a boot sweep over a registry that has leaked tens of
/// thousands of stale files does a single process-table read, not one per entry.
pub fn gc_dead_leases(registry_root: &Path) -> std::io::Result<GcReport> {
    let mut report = GcReport::default();
    // One process-table read for the whole sweep.
    let live = LivePids::snapshot();
    gc_dead_in_dir(
        &registry_root.join(LEASE_DIR_NAME),
        &live,
        true,
        &mut report.scanned,
        &mut report.leases_removed,
    )?;
    gc_dead_in_dir(
        &registry_root.join(INSTANCE_DIR_NAME),
        &live,
        false,
        &mut report.scanned,
        &mut report.instances_removed,
    )?;
    Ok(report)
}

/// Spawn a best-effort, non-blocking boot-time sweep of dead lease/instance
/// entries.
///
/// Detached on its own OS thread (NOT the tokio reactor — boot runs in both
/// async and sync contexts) so it can NEVER delay the MCP `initialize` /
/// `tools/list` handshake, even against a registry that has leaked tens of
/// thousands of stale files. Errors are swallowed: a failed sweep must never
/// fail or stall startup. The owning process's own (live-pid) entry is never
/// touched, exactly as in `gc_dead_leases`.
///
/// Returns the `JoinHandle` so callers/tests *may* join for determinism; the
/// boot path drops it (fire-and-forget) and returns immediately.
pub fn spawn_boot_gc(registry_root: PathBuf) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let _ = gc_dead_leases(&registry_root);
    })
}

/// Sweep a single registry directory, removing only entries whose pid is dead
/// according to the pre-built per-sweep live-pid snapshot.
fn gc_dead_in_dir(
    dir: &Path,
    live: &LivePids,
    is_lease_dir: bool,
    scanned: &mut usize,
    removed: &mut usize,
) -> std::io::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for item in fs::read_dir(dir)? {
        // Skip unreadable directory entries rather than aborting the sweep.
        let item = match item {
            Ok(item) => item,
            Err(_) => continue,
        };
        let path = item.path();
        if path.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        // Conservative: any read/parse error -> skip (do NOT delete).
        let entry: InstanceRegistryEntry = match read_json(&path) {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        *scanned += 1;
        // NEVER remove an entry whose owning process is still alive.
        if live.is_live(entry.pid) {
            continue;
        }

        if is_lease_dir {
            // Serialize with claim/heartbeat/release, then re-read the exact
            // acquisition identity. A successor can never be removed based on
            // the dead predecessor observed before entering this critical
            // section.
            let _guard = match LeaseMutationGuard::acquire(&path) {
                Ok(guard) => guard,
                Err(_) => continue,
            };
            let verified: InstanceRegistryEntry = match read_json(&path) {
                Ok(verified) => verified,
                Err(_) => continue,
            };
            if !lease_identity_matches(&verified, &entry) || live.is_live(verified.pid) {
                continue;
            }
            if fs::remove_file(&path).is_ok() {
                *removed += 1;
            }
        } else if fs::remove_file(&path).is_ok() {
            *removed += 1;
        }
    }
    Ok(())
}

pub fn default_registry_root() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(DEFAULT_REGISTRY_SUBDIR);
    }
    PathBuf::from(".").join(DEFAULT_REGISTRY_SUBDIR)
}

fn apply_conflicts(entries: &mut [InstanceRegistryEntry]) {
    let mut by_runtime: HashMap<String, usize> = HashMap::new();
    let mut by_workspace: HashMap<String, usize> = HashMap::new();
    for entry in entries.iter() {
        *by_runtime.entry(entry.runtime_root.clone()).or_insert(0) += 1;
        *by_workspace
            .entry(entry.workspace_root.clone())
            .or_insert(0) += 1;
    }

    for entry in entries.iter_mut() {
        if by_runtime.get(&entry.runtime_root).copied().unwrap_or(0) > 1 {
            entry.conflicts.push("shared_runtime_root".into());
        }
        if by_workspace
            .get(&entry.workspace_root)
            .copied()
            .unwrap_or(0)
            > 1
        {
            entry.conflicts.push("duplicate_workspace".into());
        }
        if entry.stale {
            entry.conflicts.push("stale_lock".into());
            if entry.status == "running" {
                entry.status = "stale".into();
            }
        }
    }
}

/// Short-lived cross-process serialization for registry lease mutations. The
/// lease itself is still claimed with `create_new`; this guard exists so stale
/// identity verification and removal form one critical section with claim,
/// heartbeat, re-key, release, explicit deletion, and lease GC.
///
/// One stable guard file is shared by the whole `leases/` directory, avoiding a
/// leaked sidecar for every historical runtime. A process-global mutex covers
/// sibling threads on every platform. On Unix `flock` is released by the kernel
/// if the process dies; on Windows an open handle with no sharing provides the
/// same crash-released cross-process lifetime.
static LEASE_MUTATION_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Same-process ownership table for per-runtime lifetime guards. OS locking is
/// authoritative across processes; this table closes platform-specific
/// same-process `flock` semantics and gives every runtime exactly one local
/// owner too.
fn owner_lifetime_paths() -> &'static std::sync::Mutex<HashSet<PathBuf>> {
    static PATHS: OnceLock<std::sync::Mutex<HashSet<PathBuf>>> = OnceLock::new();
    PATHS.get_or_init(|| std::sync::Mutex::new(HashSet::new()))
}

#[derive(Debug)]
struct OwnerLifetimeGuard {
    path: PathBuf,
    #[cfg(any(unix, windows))]
    file: fs::File,
}

impl OwnerLifetimeGuard {
    fn acquire(lease_path: &Path) -> M1ndResult<Self> {
        let path = lease_path.with_extension("owner.lock");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        {
            let mut owned = owner_lifetime_paths()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !owned.insert(path.clone()) {
                return Err(owner_lifetime_held_error(&path));
            }
        }

        let acquired = Self::acquire_os(path.clone());
        if acquired.is_err() {
            owner_lifetime_paths()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&path);
        }
        acquired
    }

    #[cfg(unix)]
    fn acquire_os(path: PathBuf) -> M1ndResult<Self> {
        use std::os::fd::AsRawFd;

        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        // SAFETY: file owns a valid descriptor for this guard's full lifetime.
        let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if result != 0 {
            let error = std::io::Error::last_os_error();
            return if error.kind() == std::io::ErrorKind::WouldBlock {
                Err(owner_lifetime_held_error(&path))
            } else {
                Err(M1ndError::Io(error))
            };
        }
        Ok(Self { path, file })
    }

    #[cfg(windows)]
    fn acquire_os(path: PathBuf) -> M1ndResult<Self> {
        use std::os::windows::fs::OpenOptionsExt;

        match fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .share_mode(LOCK_FILE_SHARE_MODE)
            .open(&path)
        {
            Ok(file) => Ok(Self { path, file }),
            Err(error) if matches!(error.raw_os_error(), Some(32) | Some(33)) => {
                Err(owner_lifetime_held_error(&path))
            }
            Err(error) => Err(M1ndError::Io(error)),
        }
    }

    #[cfg(not(any(unix, windows)))]
    fn acquire_os(path: PathBuf) -> M1ndResult<Self> {
        // The in-process table remains useful on niche targets. Their legacy
        // PID liveness fence below is deliberately stricter cross-process.
        Ok(Self { path })
    }
}

impl Drop for OwnerLifetimeGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;

            // SAFETY: the descriptor remains valid until fields drop.
            let _ = unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
        }
        owner_lifetime_paths()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.path);
    }
}

fn owner_lifetime_held_error(path: &Path) -> M1ndError {
    M1ndError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        format!(
            "runtime_root is already owned; writer lifetime guard {} is held by another live owner",
            path.display()
        ),
    ))
}

struct LeaseMutationGuard {
    #[cfg(any(unix, windows))]
    file: fs::File,
    // Declared after `file`: once Drop unlocks the OS primitive, automatic field
    // drop closes the file before releasing this in-process mutex.
    _in_process: std::sync::MutexGuard<'static, ()>,
}

impl LeaseMutationGuard {
    fn acquire(lease_path: &Path) -> M1ndResult<Self> {
        let in_process = LEASE_MUTATION_MUTEX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let guard_path = lease_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(".lease-mutations.guard");
        if let Some(parent) = guard_path.parent() {
            fs::create_dir_all(parent)?;
        }

        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;

            let file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&guard_path)?;
            loop {
                // SAFETY: `file` owns a valid descriptor for the full guard
                // lifetime; LOCK_EX has no pointer or aliasing requirements.
                let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
                if result == 0 {
                    return Ok(Self {
                        file,
                        _in_process: in_process,
                    });
                }
                let error = std::io::Error::last_os_error();
                if error.kind() != std::io::ErrorKind::Interrupted {
                    return Err(M1ndError::Io(error));
                }
            }
        }

        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;

            loop {
                match fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .share_mode(LOCK_FILE_SHARE_MODE)
                    .open(&guard_path)
                {
                    Ok(file) => {
                        return Ok(Self {
                            file,
                            _in_process: in_process,
                        });
                    }
                    Err(error) if matches!(error.raw_os_error(), Some(32) | Some(33)) => {
                        std::thread::yield_now();
                    }
                    Err(error) => return Err(M1ndError::Io(error)),
                }
            }
        }

        #[cfg(not(any(unix, windows)))]
        {
            let _ = guard_path;
            Ok(Self {
                _in_process: in_process,
            })
        }
    }
}

impl Drop for LeaseMutationGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;

            // SAFETY: the descriptor remains valid until fields are dropped
            // after this method. Unlock is best-effort; close also unlocks.
            let _ = unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
        }
    }
}

fn lease_identity_matches(
    observed: &InstanceRegistryEntry,
    expected: &InstanceRegistryEntry,
) -> bool {
    observed.instance_id == expected.instance_id
        && observed.pid == expected.pid
        && observed.started_at_ms == expected.started_at_ms
        && observed.runtime_root == expected.runtime_root
}

fn lease_blocks_acquisition(entry: &InstanceRegistryEntry) -> bool {
    entry.pid == std::process::id()
        || (is_pid_live(entry.pid) && !is_stale(entry.last_heartbeat_ms))
}

fn already_owned_error(
    requested: &InstanceRegistryEntry,
    existing: &InstanceRegistryEntry,
) -> M1ndError {
    M1ndError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        format!(
            "runtime_root {} is already owned by instance {} (pid {})",
            requested.runtime_root, existing.instance_id, existing.pid
        ),
    ))
}

/// Write a brand-new JSON file without ever replacing an existing path.
/// Serialization happens before the exclusive open. Once `create_new` succeeds,
/// any write or fsync error closes and removes that exact file before returning.
fn save_json_exclusive<T: Serialize>(path: &Path, value: &T) -> M1ndResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(value).map_err(|error| {
        M1ndError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("failed to serialize {}: {error}", path.display()),
        ))
    })?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let write_result = file.write_all(&json).and_then(|()| file.sync_all());
    if let Err(write_error) = write_result {
        drop(file);
        let cleanup_error = match fs::remove_file(path) {
            Ok(()) => None,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => Some(error),
        };
        return match cleanup_error {
            None => Err(M1ndError::Io(write_error)),
            Some(cleanup_error) => Err(M1ndError::Io(std::io::Error::new(
                write_error.kind(),
                format!(
                    "exclusive lease write failed for {}: {write_error}; cleanup failed: {cleanup_error}",
                    path.display()
                ),
            ))),
        };
    }
    Ok(())
}

/// Atomically claim a ReadWrite lease. Only `create_new` can produce a winner.
/// A stale/dead incumbent is re-read and identity-checked while the mutation
/// guard is held, removed, and retried without opening an overwrite seam.
fn claim_readwrite_lease(path: &Path, requested: &InstanceRegistryEntry) -> M1ndResult<()> {
    let _guard = LeaseMutationGuard::acquire(path)?;
    loop {
        match save_json_exclusive(path, requested) {
            Ok(()) => return Ok(()),
            Err(M1ndError::Io(error)) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }

        let existing: InstanceRegistryEntry = match read_json(path) {
            Ok(existing) => existing,
            Err(M1ndError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                continue;
            }
            Err(error) => return Err(error),
        };
        if lease_blocks_acquisition(&existing) {
            return Err(already_owned_error(requested, &existing));
        }

        // Re-read under the same guard immediately before removal. This catches
        // an identity transition or a legacy heartbeat that refreshed the lease
        // between inspection and the guarded cleanup attempt.
        let verified: InstanceRegistryEntry = match read_json(path) {
            Ok(verified) => verified,
            Err(M1ndError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                continue;
            }
            Err(error) => return Err(error),
        };
        if !lease_identity_matches(&verified, &existing) {
            continue;
        }
        if lease_blocks_acquisition(&verified) {
            return Err(already_owned_error(requested, &verified));
        }
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(M1ndError::Io(error)),
        }
        // Loop while retaining the guard: this attempt either wins through
        // create_new or observes a non-cooperating replacement and re-evaluates.
    }
}

fn ensure_owned_lease(path: &Path, expected: &InstanceRegistryEntry) -> M1ndResult<()> {
    let observed: InstanceRegistryEntry = read_json(path)?;
    if lease_identity_matches(&observed, expected) {
        return Ok(());
    }
    Err(M1ndError::Io(std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        format!(
            "instance {} no longer owns lease {} (now owned by {} pid {})",
            expected.instance_id,
            path.display(),
            observed.instance_id,
            observed.pid
        ),
    )))
}

fn remove_owned_lease_file(path: &Path, expected: &InstanceRegistryEntry) -> M1ndResult<()> {
    let _guard = LeaseMutationGuard::acquire(path)?;
    remove_owned_registry_file(path, expected)
}

fn persist_handle_inner(inner: &InstanceHandleInner) -> M1ndResult<()> {
    ensure_instance_active(inner)?;
    // A writer must still own the canonical lease before it can refresh either
    // registry projection. This prevents a stale predecessor from overwriting a
    // successor after identity-checked recovery.
    if let Some(lock_path) = &inner.lock_path {
        let _guard = LeaseMutationGuard::acquire(lock_path)?;
        ensure_owned_lease(lock_path, &inner.entry)?;
        save_json_atomic(lock_path, &inner.entry)?;
    }
    // ReadOnly handles reach only this discovery write. ReadWrite owners publish
    // it after the lease refresh above has confirmed continuing ownership.
    save_json_atomic(&inner.entry_path, &inner.entry)
}

/// Transactional stable-id transition for `set_brain_kind`.
///
/// For a real re-key, the new discovery entry is staged first and the guarded
/// lease update is the commit point. Before that point the caller retains its
/// old in-memory identity/path and old discovery entry. If lease verification or
/// persistence fails, the staged target is removed or its displaced predecessor
/// is restored. Once this returns Ok, callers must publish `next` in memory
/// before any fallible cleanup.
fn commit_brain_rekey(
    inner: &InstanceHandleInner,
    previous: &InstanceRegistryEntry,
    next: &InstanceRegistryEntry,
    next_entry_path: &Path,
) -> M1ndResult<()> {
    ensure_instance_active(inner)?;
    let is_rekey = inner.entry_path != next_entry_path;

    if let Some(lock_path) = &inner.lock_path {
        let _guard = LeaseMutationGuard::acquire(lock_path)?;
        ensure_owned_lease(lock_path, previous)?;

        if is_rekey {
            // A stable-id card from an inherited boot may already occupy the
            // target. Preserve it so a lease-commit failure can restore the
            // exact pre-transaction registry topology instead of deleting it.
            let displaced_target: Option<InstanceRegistryEntry> = if next_entry_path.exists() {
                Some(read_json(next_entry_path)?)
            } else {
                None
            };
            save_json_atomic(next_entry_path, next)?;
            if let Err(commit_error) = save_json_atomic(lock_path, next) {
                let rollback = match displaced_target {
                    Some(ref displaced) => save_json_atomic(next_entry_path, displaced),
                    None => remove_owned_registry_file(next_entry_path, next),
                };
                return match rollback {
                    Ok(()) => Err(commit_error),
                    Err(rollback_error) => Err(M1ndError::Io(std::io::Error::other(format!(
                            "brain re-key lease commit failed: {commit_error}; target rollback failed: {rollback_error}"
                        )))),
                };
            }
        } else {
            save_json_atomic(lock_path, next)?;
        }

        if !is_rekey {
            if let Err(error) = save_json_atomic(next_entry_path, next) {
                // Identity is unchanged in the non-re-key case. Restore the old
                // payload best-effort; even if rollback I/O also fails, Drop can
                // still identify and remove the lease by acquisition identity.
                let _ = save_json_atomic(lock_path, previous);
                return Err(error);
            }
        }
    } else {
        save_json_atomic(next_entry_path, next)?;
    }

    Ok(())
}

fn ensure_instance_active(inner: &InstanceHandleInner) -> M1ndResult<()> {
    if inner.released {
        return Err(M1ndError::Io(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            format!(
                "instance {} has already released its registry lifecycle capability",
                inner.entry.instance_id
            ),
        )));
    }
    Ok(())
}

fn mark_heartbeat_inner(inner: &mut InstanceHandleInner) -> M1ndResult<()> {
    ensure_instance_active(inner)?;
    inner.entry.last_heartbeat_ms = now_ms();
    if inner.entry.status == "starting" {
        inner.entry.status = "running".into();
    }
    persist_handle_inner(inner)
}

/// Remove a registry file only when it still belongs to this exact acquisition.
/// This prevents a late Drop from deleting a successor/foreign owner that has
/// atomically replaced the shared lease path.
fn remove_owned_registry_file(path: &Path, expected: &InstanceRegistryEntry) -> M1ndResult<()> {
    if !path.exists() {
        return Ok(());
    }
    let observed: InstanceRegistryEntry = read_json(path)?;
    if !lease_identity_matches(&observed, expected) {
        return Ok(());
    }
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(M1ndError::Io(error)),
    }
}

/// Reconcile the `instances/` dir after a brain re-keys onto its stable id: drop
/// every OTHER entry for the SAME `(workspace_root, runtime_root)` store — the
/// duplicate cards earlier ephemeral-id boots minted for this exact brain. This
/// is what lets the inheritance of pre-fix duplicates die on the next boot of
/// each brain, instead of lingering forever under a still-live owner pid (which
/// the dead-pid boot GC can never reap).
///
/// A live `read_only` attacher is NEVER a duplicate — N attachers coexist with
/// one brain by design (a foreign process attached to the same store is not a
/// twin), so it is preserved; everything else that shares this store's exact
/// roots is a stale twin and is removed. Only same-store entries are candidates —
/// a different brain (different roots) is never inspected. Best-effort: a
/// corrupt/foreign entry that fails to parse is skipped, never force-deleted; the
/// survivor's own (stable-id) entry is skipped by path. One process-table read is
/// shared across the sweep via a single `LivePids` snapshot.
fn reconcile_brain_duplicates(keep: &InstanceHandleInner) {
    let dir = keep.registry_root.join(INSTANCE_DIR_NAME);
    let read = match fs::read_dir(&dir) {
        Ok(read) => read,
        Err(_) => return,
    };
    let live = LivePids::snapshot();
    for item in read.flatten() {
        let path = item.path();
        if path.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        // Never remove the survivor's own (stable-id) entry.
        if path == keep.entry_path {
            continue;
        }
        let entry: InstanceRegistryEntry = match read_json(&path) {
            Ok(entry) => entry,
            Err(_) => continue, // corrupt/foreign → skip, never delete
        };
        // Only entries for the SAME store are candidates for removal.
        if entry.workspace_root != keep.entry.workspace_root
            || entry.runtime_root != keep.entry.runtime_root
        {
            continue;
        }
        // A live read_only attacher shares the store by design — not a duplicate.
        if InstanceMode::from_str(&entry.mode) == InstanceMode::ReadOnly && live.is_live(entry.pid)
        {
            continue;
        }
        let _ = fs::remove_file(&path);
    }
}

/// Monotonic per-process nonce so that two instances acquired in the same
/// process for the same (workspace, runtime) within a single millisecond clock
/// tick never collide on `instance_id`. The pid already disambiguates across
/// processes; this disambiguates within one (e.g. a ReadWrite owner and a
/// ReadOnly attacher started back-to-back, or test setups).
static INSTANCE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn generate_instance_id(workspace_root: &Path, runtime_root: &Path, now_ms: u64) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    workspace_root.to_string_lossy().hash(&mut hasher);
    runtime_root.to_string_lossy().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    now_ms.hash(&mut hasher);
    INSTANCE_SEQ
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .hash(&mut hasher);
    format!("inst_{:x}", hasher.finish())
}

/// Deterministic instance id for a BRAIN entry (project/medulla): a pure function
/// of `(workspace_root, runtime_root)`, stable across every warm-boot of the same
/// store so the boot UPSERTS one `instances/<id>.json` instead of minting a new
/// file each time (the duplicate-card field bug — see [`InstanceHandle::set_brain_kind`]).
/// Reuses the module's `DefaultHasher` string-hashing scheme (the same one
/// [`generate_instance_id`]/[`fingerprint_path`] use — mother rule, one scheme),
/// dropping only the pid/clock/seq nonce that makes `generate_instance_id`
/// ephemeral by construction.
fn stable_brain_instance_id(workspace_root: &str, runtime_root: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    workspace_root.hash(&mut hasher);
    runtime_root.hash(&mut hasher);
    format!("inst_{:x}", hasher.finish())
}

/// Stable per-path fingerprint (hash of the path string). Used to name the
/// lease file for a runtime_root, and reused by the Two-Tier project-brain
/// registry to name each brain's owner-side store dir by its caller_root, so
/// the two agree on one hashing scheme (`pub(crate)`, one keyword — the smallest
/// change that lets the store-dir naming reuse this instead of forking a second
/// hex-of-path helper).
pub(crate) fn fingerprint_path(path: &Path) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn is_stale(last_heartbeat_ms: u64) -> bool {
    now_ms().saturating_sub(last_heartbeat_ms) > STALE_AFTER_MS
}

/// A point-in-time snapshot of which PIDs are live, built from a SINGLE read of
/// the OS process table. Construct once per sweep and reuse for every entry so
/// the process table is not re-read per registry entry.
///
/// Conservative by construction: if the platform is unsupported, or the process
/// refresh failed/returned an empty table, the snapshot is `Unknown` and every
/// pid is reported LIVE — so a GC sweep never deletes an entry it cannot prove
/// dead. Only a successfully-built `Known` set can ever report a pid as dead.
enum LivePids {
    /// Platform unsupported or the refresh failed/was empty -> treat every pid
    /// as live (never delete).
    Unknown,
    /// Successfully read live PIDs; only these are live, everything else dead.
    Known(HashSet<u32>),
}

impl LivePids {
    /// Read the OS process table exactly once. No subprocess is spawned.
    fn snapshot() -> Self {
        if !sysinfo::IS_SUPPORTED_SYSTEM {
            return LivePids::Unknown;
        }

        let mut system = System::new();
        let refreshed = system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().without_tasks(),
        );

        if refreshed == 0 && system.processes().is_empty() {
            return LivePids::Unknown;
        }

        LivePids::Known(system.processes().keys().map(|pid| pid.as_u32()).collect())
    }

    /// Conservative membership: `Unknown` -> always live; `Known` -> only pids
    /// present in the snapshot are live.
    fn is_live(&self, pid: u32) -> bool {
        match self {
            LivePids::Unknown => true,
            LivePids::Known(set) => set.contains(&pid),
        }
    }
}

/// Single-PID liveness for the non-sweep callers (lease-collision check,
/// `list_instances`, `delete_instance_state`). Reads the process table once per
/// call via a fresh [`LivePids`] snapshot — acceptable for these one-off checks.
/// The GC sweep does NOT use this; it shares one snapshot across all entries.
fn is_pid_live(pid: u32) -> bool {
    LivePids::snapshot().is_live(pid)
}

fn canonicalish(path: &Path) -> std::io::Result<PathBuf> {
    if path.exists() {
        return fs::canonicalize(path);
    }
    if let Some(parent) = path.parent() {
        if parent.exists() {
            let canonical_parent = fs::canonicalize(parent)?;
            if let Some(name) = path.file_name() {
                return Ok(canonical_parent.join(name));
            }
        }
    }
    Ok(path.to_path_buf())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> M1ndResult<T> {
    let raw = fs::read_to_string(path)?;
    serde_json::from_str(&raw).map_err(|error| {
        M1ndError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid json at {}: {error}", path.display()),
        ))
    })
}

fn save_json_atomic<T: Serialize>(path: &Path, value: &T) -> M1ndResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value).map_err(|error| {
        M1ndError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("failed to serialize {}: {error}", path.display()),
        ))
    })?;
    let temp = path.with_extension("tmp");
    fs::write(&temp, json)?;
    fs::rename(temp, path)?;
    Ok(())
}

/// Build the reachable base URL for a registry entry, applying the same rule the
/// HTTP server uses for self-advertisement: a wildcard `0.0.0.0` bind is rewritten
/// to a loopback `127.0.0.1`, any other bind is used verbatim. Returns `None` when
/// the entry has no published port (a stdio-only owner that never called
/// `set_running_endpoint`), which makes it un-attachable by construction.
pub fn entry_base_url(entry: &InstanceRegistryEntry) -> Option<String> {
    let port = entry.port?;
    let bind = entry.bind.as_deref().unwrap_or("127.0.0.1");
    let host = if bind == "0.0.0.0" { "127.0.0.1" } else { bind };
    Some(format!("http://{}:{}", host, port))
}

/// Which discovery question found the owner an `--attach auto` client resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnerDiscovery {
    /// Question 1 — a live serve owner whose `runtime_root` IS the client's.
    RuntimeRoot,
    /// Question 2 — a live serve owner whose declared roots COVER the caller's
    /// repo. `declared_root` is the owner root that covered it; `caller_root` is
    /// the root actually compared (canonical, worktree-resolved).
    IngestCoverage {
        declared_root: String,
        caller_root: String,
    },
}

/// The owner `--attach auto` resolved, and how it was found.
#[derive(Clone, Debug)]
pub struct DiscoveredOwner {
    /// Reachable base URL, e.g. `http://127.0.0.1:1338`.
    pub base_url: String,
    /// The OWNER's own runtime root — NOT necessarily the client's. This is the
    /// directory that holds its `http-auth-token-v1`, so the attach bridge can
    /// authenticate against an owner it does not share a runtime root with.
    pub runtime_root: PathBuf,
    /// Which question answered.
    pub discovery: OwnerDiscovery,
}

/// True when this entry is an attachable live serve ReadWrite owner: the four
/// gates `--attach auto` has always applied, in one place so both discovery
/// questions can never drift apart.
fn is_attachable_serve_owner(entry: &InstanceRegistryEntry) -> bool {
    InstanceMode::from_str(&entry.mode) == InstanceMode::ReadWrite
        && entry.owner_live == Some(true)
        && !entry.stale
        // Serve gate: stdio-only owners publish no bind/port and are unreachable.
        && entry_base_url(entry).is_some()
}

/// The roots this owner's bound brain has DECLARED, read from the same file the
/// owner persists them to: `ingest_roots.json` beside its graph snapshot
/// (`SessionState::persist_ingest_roots` / `load_ingest_roots`).
///
/// Reading the owner's own file rather than adding a registry field is what makes
/// an owner of ANY version discoverable, and keeps the list current between
/// heartbeats — the root set changes on every ingest, the registry entry does not.
///
/// The set mirrors `SessionState::covers_root`'s own inputs (`workspace_root` +
/// the ingest roots), so "this brain's territory" keeps ONE definition.
fn entry_declared_roots(entry: &InstanceRegistryEntry) -> Vec<String> {
    let mut roots = vec![entry.workspace_root.clone()];
    let persist_root = Path::new(&entry.graph_source)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(&entry.runtime_root));
    if let Ok(raw) = fs::read_to_string(persist_root.join("ingest_roots.json")) {
        if let Ok(declared) = serde_json::from_str::<Vec<String>>(&raw) {
            roots.extend(declared);
        }
    }
    roots
}

/// The declared root of `entry` that COVERS `caller_key`, if any — canonicalized.
///
/// "Covers" is `SessionState::covers_root`'s question, asked with the house's own
/// rule: canonical path identity, whole components, NEVER a raw string prefix.
/// It is deliberately the loose predicate rather than SPEC-1's
/// `exact_declared_root`, because those two answer different questions.
/// `exact_declared_root` is the AUTHORITY-EXCLUSIVE one — it authorizes a WRITE
/// (a re-ingest that replaces a graph), where a subdirectory match would let any
/// subtree rewrite the whole repo. This is a ROUTING question: which owner do I
/// speak to. Discovery choosing an owner never widens what that owner authorizes;
/// every verb the bridge forwards still meets the owner's own floors, and the
/// hop-2 `M1nd-Caller-Root` header still carries the caller's TRUE root.
fn declared_root_covering(entry: &InstanceRegistryEntry, caller_key: &str) -> Option<String> {
    let caller = Path::new(caller_key);
    entry_declared_roots(entry).into_iter().find_map(|root| {
        let key = crate::project_brains::ProjectBrainRegistry::canonical_key(&root);
        crate::project_brains::path_starts_with_loosely(caller, Path::new(&key)).then_some(key)
    })
}

/// The caller root as DISCOVERY compares it: canonical identity (symlinks and the
/// `/tmp` → `/private/tmp` alias resolved), and a git WORKTREE resolved to its
/// MAIN repository — the house's own ruling that a worktree belongs to the repo it
/// checks out (`detect_root_overlap` refuses to mint it a second brain).
///
/// This normalization is discovery-local ON PURPOSE. The hop-2 `M1nd-Caller-Root`
/// header keeps the caller's true root: normalizing it there would forge an
/// exact-root claim for a worktree, which SPEC-1's `exact_declared_root` exists to
/// refuse. Being generous about which owner to TALK TO must never be generous
/// about what that owner authorizes.
fn discovery_caller_key(caller_root: &Path) -> String {
    let key =
        crate::project_brains::ProjectBrainRegistry::canonical_key(&caller_root.to_string_lossy());
    crate::project_brains::worktree_main_repo(&key).unwrap_or(key)
}

/// One candidate of the ingest-coverage question.
struct CoveringOwner {
    base_url: String,
    runtime_root: PathBuf,
    declared_root: String,
}

/// Ambiguity is the owner's to resolve, never auto-discovery's. Two live owners
/// covering one caller root is a real configuration question (which brain should
/// this repo speak to?), and a silent pick would send an agent's whole session to
/// the wrong brain. Refuse, and name every candidate so the choice is one command
/// away. This mirrors the routing seam's abstain law (`covering_brain`: exactly
/// one related brain answers, `> 1` abstains).
fn ambiguous_coverage_error(caller_key: &str, candidates: &[CoveringOwner]) -> String {
    let mut message = format!(
        "{} live serve owners declare a root covering caller root {caller_key}; \
ambiguity is the owner's to resolve, never auto-discovery's:",
        candidates.len()
    );
    for candidate in candidates {
        message.push_str(&format!(
            "\n  - {} (declares {}; runtime_root {})",
            candidate.base_url,
            candidate.declared_root,
            candidate.runtime_root.display()
        ));
    }
    message.push_str("\nName one explicitly: --attach <url>, or set M1ND_ATTACH_URL.");
    message
}

/// The honest refusal when NEITHER question can be answered. It teaches both
/// facts, because naming only the first is what sent three sessions looking for
/// the wrong repair: an agent told "no owner for your runtime_root" reaches for a
/// second owner, when the real answer was that a live owner already held the
/// graph and simply had not been asked the other question.
fn no_owner_error(
    runtime_root_target: &str,
    caller_key: Option<&str>,
    registered: usize,
    live_serve_owners: usize,
) -> String {
    let second_fact = match caller_key {
        Some(caller) => format!(
            "caller root {caller} — no live serve owner declares an ingest root covering it"
        ),
        None => "caller root — could not be resolved, so the ingest-root question could not be \
             asked (pass --runtime-dir, or set M1ND_WORKSPACE_ROOT)"
            .to_string(),
    };
    format!(
        "no live serve owner for this client, on either discovery question:\n  \
1. runtime_root {runtime_root_target} — no live serve ReadWrite owner holds it;\n  \
2. {second_fact}.\n\
({registered} instance(s) registered, {live_serve_owners} live serve owner(s) inspected.)\n\
Next: start an owner here (m1nd-mcp --serve --no-gui), ingest this repo into a \
running owner, or name one with --attach <url>."
    )
}

/// Read-only discovery for the `--attach auto` thin client.
///
/// This is PURE read-only registry inspection: it calls `list_instances` (which
/// only reads `instances/*.json`) and NEVER `acquire`/`acquire_with_mode`, so it
/// takes no lease and never contends the owner's exclusive PID+heartbeat lock.
///
/// Two questions are asked, in order — the second only if the first finds nothing.
///
/// Question 1 — "is there a live serve owner for MY runtime_root?" Matching
/// mirrors `acquire_with_mode`'s persistence exactly:
///   * the target `runtime_root` is canonicalized with the same `canonicalish`
///     semantics the owner used before writing `entry.runtime_root`, so the
///     string comparison lines up on macOS (`/tmp` → `/private/tmp`, symlinks…);
///   * only `mode == read_write`, `owner_live == Some(true)`, `stale == false`
///     entries that ALSO publish `bind`+`port` survive;
///   * with multiple survivors the freshest by `last_heartbeat_ms` wins
///     (`list_instances` already sorts descending, so the first survivor is it).
///
/// Question 2 — "is there a live serve owner that has INGESTED my repo?" Asking
/// only question 1 is a measured defect (project mailbox letter `opus5-guardian`,
/// 2026-07-30, three independent sessions): an agent working inside a repo gets a
/// stdio owner over that repo's EMPTY `.m1nd` runtime, while the machine's served
/// owner holds the full graph and already declares the repo among its ingest
/// roots — and auto-discovery, asking only about runtime roots, could not see it.
/// So the fallback walks the same live serve owners and attaches to the one whose
/// declared roots COVER the caller root. Still read-only, still no lease.
///
/// It is strictly a FALLBACK: an owner for the client's own runtime root always
/// wins, and `> 1` covering owner refuses rather than guesses.
pub fn discover_serve_owner(
    runtime_root: &Path,
    caller_root: Option<&Path>,
    registry_dir: Option<&Path>,
) -> Result<DiscoveredOwner, String> {
    // Canonicalize the target identically to how the owner stored it.
    let target = canonicalish(runtime_root)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| runtime_root.to_string_lossy().into_owned());

    let entries = list_instances(registry_dir)
        .map_err(|e| format!("failed to read instance registry: {e}"))?;

    // --- Question 1: an owner for this client's own runtime root. ---
    for entry in &entries {
        if entry.runtime_root != target || !is_attachable_serve_owner(entry) {
            continue;
        }
        if let Some(base_url) = entry_base_url(entry) {
            return Ok(DiscoveredOwner {
                base_url,
                runtime_root: PathBuf::from(&entry.runtime_root),
                discovery: OwnerDiscovery::RuntimeRoot,
            });
        }
    }

    // --- Question 2: an owner that has ingested this caller's repo. ---
    let live: Vec<&InstanceRegistryEntry> = entries
        .iter()
        .filter(|entry| is_attachable_serve_owner(entry))
        .collect();
    let caller_key = caller_root.map(discovery_caller_key);

    if let Some(caller_key) = caller_key.as_deref() {
        let mut candidates: Vec<CoveringOwner> = Vec::new();
        for entry in &live {
            let Some(base_url) = entry_base_url(entry) else {
                continue;
            };
            // Two entries on one listener are one owner, never an ambiguity.
            if candidates
                .iter()
                .any(|candidate| candidate.base_url == base_url)
            {
                continue;
            }
            if let Some(declared_root) = declared_root_covering(entry, caller_key) {
                candidates.push(CoveringOwner {
                    base_url,
                    runtime_root: PathBuf::from(&entry.runtime_root),
                    declared_root,
                });
            }
        }

        if candidates.len() > 1 {
            return Err(ambiguous_coverage_error(caller_key, &candidates));
        }
        if let Some(candidate) = candidates.pop() {
            return Ok(DiscoveredOwner {
                base_url: candidate.base_url,
                runtime_root: candidate.runtime_root,
                discovery: OwnerDiscovery::IngestCoverage {
                    declared_root: candidate.declared_root,
                    caller_root: caller_key.to_string(),
                },
            });
        }
    }

    Err(no_owner_error(
        &target,
        caller_key.as_deref(),
        entries.len(),
        live.len(),
    ))
}

/// Schema of the one-shot owner-discovery answer a non-Rust caller reads.
pub const OWNER_DISCOVERY_SCHEMA: &str = "m1nd-owner-discovery-v0";

/// [`discover_serve_owner`]'s answer in a wire shape a non-Rust caller can
/// branch on — today the npm agent CLI (`m1nd agent first-minute` / `context`),
/// which must decide whether to bridge to a live owner or boot its own runtime.
///
/// It carries the refusal text verbatim on the negative answer, because a client
/// that boots isolated has to be able to say WHY it is isolated.
#[derive(Clone, Debug, Serialize)]
pub struct OwnerDiscoveryProbe {
    pub schema: &'static str,
    /// True when a live serve ReadWrite owner answered either question.
    pub found: bool,
    /// The runtime root question 1 asked about (the client's own).
    pub client_runtime_root: String,
    /// The repo question 2 asked about, as the caller named it.
    pub caller_root: Option<String>,
    /// `runtime_root` or `ingest_coverage` — which question answered.
    pub discovery: Option<String>,
    /// Reachable base URL of the owner, e.g. `http://127.0.0.1:1338`.
    pub base_url: Option<String>,
    /// The OWNER's own runtime root — where its bearer token lives.
    pub owner_runtime_root: Option<String>,
    /// The owner's declared root that covered the caller (question 2 only).
    pub declared_root: Option<String>,
    /// The discovery's own refusal, verbatim, when nothing was found.
    pub reason: Option<String>,
}

/// Ask [`discover_serve_owner`]'s two questions and project the answer.
///
/// PURE read-only registry inspection, exactly like the discovery it wraps: no
/// lease, no port, no graph. It exists so the boot decision of a non-Rust client
/// can be made by the SAME code `--attach auto` uses — never by a second
/// discovery that would drift from it.
pub fn probe_serve_owner(
    runtime_root: &Path,
    caller_root: Option<&Path>,
    registry_dir: Option<&Path>,
) -> OwnerDiscoveryProbe {
    let base = OwnerDiscoveryProbe {
        schema: OWNER_DISCOVERY_SCHEMA,
        found: false,
        client_runtime_root: runtime_root.to_string_lossy().into_owned(),
        caller_root: caller_root.map(|root| root.to_string_lossy().into_owned()),
        discovery: None,
        base_url: None,
        owner_runtime_root: None,
        declared_root: None,
        reason: None,
    };
    match discover_serve_owner(runtime_root, caller_root, registry_dir) {
        Ok(owner) => {
            let (discovery, declared_root) = match owner.discovery {
                OwnerDiscovery::RuntimeRoot => ("runtime_root", None),
                OwnerDiscovery::IngestCoverage { declared_root, .. } => {
                    ("ingest_coverage", Some(declared_root))
                }
            };
            OwnerDiscoveryProbe {
                found: true,
                discovery: Some(discovery.to_string()),
                base_url: Some(owner.base_url),
                owner_runtime_root: Some(owner.runtime_root.to_string_lossy().into_owned()),
                declared_root,
                ..base
            }
        }
        // The refusal travels VERBATIM: ambiguity naming both candidates, and
        // the two-fact "no owner on either question" text. A client that boots
        // its own runtime instead must be able to say why it is alone.
        Err(reason) => OwnerDiscoveryProbe {
            reason: Some(reason),
            ..base
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command};
    use tempfile::tempdir;

    fn spawn_live_pid_fixture() -> Child {
        #[cfg(windows)]
        {
            Command::new("powershell")
                .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"])
                .spawn()
                .expect("spawn live pid fixture")
        }
        #[cfg(not(windows))]
        {
            Command::new("sleep")
                .arg("30")
                .spawn()
                .expect("spawn live pid fixture")
        }
    }

    #[test]
    fn acquires_and_lists_single_instance() {
        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let runtime = temp.path().join("runtime");
        let graph = runtime.join("graph.json");
        let plasticity = runtime.join("plasticity.json");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&runtime).unwrap();
        let registry = temp.path().join("registry");

        let mut handle =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .unwrap();
        handle
            .set_running_endpoint("127.0.0.1".into(), 1337)
            .unwrap();

        let instances = list_instances(Some(&registry)).unwrap();
        assert_eq!(instances.len(), 1);
        assert_eq!(
            instances[0].workspace_root,
            canonicalish(&workspace).unwrap().to_string_lossy()
        );
        assert_eq!(instances[0].status, "running");
        assert!(instances[0].owner_live.unwrap_or(false));
    }

    #[test]
    fn rejects_live_runtime_root_collision_for_foreign_owner() {
        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let runtime = temp.path().join("runtime");
        let graph = runtime.join("graph.json");
        let plasticity = runtime.join("plasticity.json");
        let registry = temp.path().join("registry");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&runtime).unwrap();

        let first =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .unwrap();
        let mut foreign_owner = spawn_live_pid_fixture();
        let mut foreign = first.summary();
        foreign.instance_id = "inst_foreign".into();
        foreign.pid = foreign_owner.id();
        foreign.last_heartbeat_ms = now_ms();
        let lock_path = registry.join(LEASE_DIR_NAME).join(format!(
            "{}.json",
            fingerprint_path(&canonicalish(&runtime).unwrap())
        ));
        save_json_atomic(&lock_path, &foreign).unwrap();
        let err =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .unwrap_err();
        let _ = foreign_owner.kill();
        let _ = foreign_owner.wait();
        assert!(err.to_string().contains("already owned"));
    }

    #[test]
    fn marks_duplicate_workspaces_as_soft_conflicts() {
        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let registry = temp.path().join("registry");

        let runtime_a = temp.path().join("runtime-a");
        let runtime_b = temp.path().join("runtime-b");
        fs::create_dir_all(&runtime_a).unwrap();
        fs::create_dir_all(&runtime_b).unwrap();
        let graph_a = runtime_a.join("graph.json");
        let plasticity_a = runtime_a.join("plasticity.json");
        let graph_b = runtime_b.join("graph.json");
        let plasticity_b = runtime_b.join("plasticity.json");

        let _a = InstanceHandle::acquire(
            &workspace,
            &runtime_a,
            &graph_a,
            &plasticity_a,
            Some(&registry),
        )
        .unwrap();
        let _b = InstanceHandle::acquire(
            &workspace,
            &runtime_b,
            &graph_b,
            &plasticity_b,
            Some(&registry),
        )
        .unwrap();

        let instances = list_instances(Some(&registry)).unwrap();
        assert_eq!(instances.len(), 2);
        assert!(instances
            .iter()
            .all(|entry| entry.conflicts.contains(&"duplicate_workspace".to_string())));
    }

    #[test]
    fn deletes_stale_instance_runtime_state() {
        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let runtime = temp.path().join("runtime");
        let graph = runtime.join("graph.json");
        let plasticity = runtime.join("plasticity.json");
        let registry = temp.path().join("registry");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&runtime).unwrap();
        fs::write(runtime.join("graph.json"), "{}").unwrap();

        let handle =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .unwrap();
        let mut stale = handle.summary();
        stale.pid = u32::MAX - 1;
        stale.last_heartbeat_ms = 0;
        let entry_path = registry
            .join(INSTANCE_DIR_NAME)
            .join(format!("{}.json", stale.instance_id));
        save_json_atomic(&entry_path, &stale).unwrap();
        let lease_path = registry.join(LEASE_DIR_NAME).join(format!(
            "{}.json",
            fingerprint_path(&canonicalish(&runtime).unwrap())
        ));
        save_json_atomic(&lease_path, &stale).unwrap();

        let deleted = delete_instance_state(&stale.instance_id, Some(&registry)).unwrap();
        assert_eq!(deleted.instance_id, stale.instance_id);
        assert!(!runtime.exists());
        assert!(!entry_path.exists());
        assert!(!lease_path.exists());
    }

    #[test]
    fn refuses_to_delete_stale_but_live_instance_runtime_state() {
        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let runtime = temp.path().join("runtime");
        let graph = runtime.join("graph.json");
        let plasticity = runtime.join("plasticity.json");
        let registry = temp.path().join("registry");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&runtime).unwrap();
        fs::write(runtime.join("graph.json"), "{}").unwrap();

        let handle =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .unwrap();
        let mut stale = handle.summary();
        stale.last_heartbeat_ms = 0;
        let entry_path = registry
            .join(INSTANCE_DIR_NAME)
            .join(format!("{}.json", stale.instance_id));
        save_json_atomic(&entry_path, &stale).unwrap();
        let lease_path = registry.join(LEASE_DIR_NAME).join(format!(
            "{}.json",
            fingerprint_path(&canonicalish(&runtime).unwrap())
        ));
        save_json_atomic(&lease_path, &stale).unwrap();

        let error = delete_instance_state(&stale.instance_id, Some(&registry)).unwrap_err();
        assert!(error
            .to_string()
            .contains("cannot delete runtime state for live instance"));
        assert!(runtime.exists());
        assert!(entry_path.exists());
        assert!(lease_path.exists());
    }

    #[test]
    fn instance_mode_roundtrips_on_disk_string() {
        assert_eq!(InstanceMode::ReadWrite.as_str(), "read_write");
        assert_eq!(InstanceMode::ReadOnly.as_str(), "read_only");
        assert_eq!(
            InstanceMode::from_str("read_write"),
            InstanceMode::ReadWrite
        );
        assert_eq!(InstanceMode::from_str("read_only"), InstanceMode::ReadOnly);
        // Unknown/legacy values default to ReadWrite.
        assert_eq!(InstanceMode::from_str("whatever"), InstanceMode::ReadWrite);
    }

    #[test]
    fn readonly_attach_coexists_with_live_readwrite_owner() {
        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let runtime = temp.path().join("runtime");
        let graph = runtime.join("graph.json");
        let plasticity = runtime.join("plasticity.json");
        let registry = temp.path().join("registry");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&runtime).unwrap();

        let owner =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .unwrap();
        assert_eq!(owner.mode(), InstanceMode::ReadWrite);

        // Two ReadOnly attachers succeed even with a live ReadWrite owner.
        let mut ro_a = InstanceHandle::acquire_with_mode(
            &workspace,
            &runtime,
            &graph,
            &plasticity,
            Some(&registry),
            InstanceMode::ReadOnly,
        )
        .unwrap();
        let ro_b = InstanceHandle::acquire_with_mode(
            &workspace,
            &runtime,
            &graph,
            &plasticity,
            Some(&registry),
            InstanceMode::ReadOnly,
        )
        .unwrap();
        assert_eq!(ro_a.mode(), InstanceMode::ReadOnly);
        assert_eq!(ro_b.mode(), InstanceMode::ReadOnly);

        // The single exclusive lease still belongs to the ReadWrite owner.
        let lease_path = registry.join(LEASE_DIR_NAME).join(format!(
            "{}.json",
            fingerprint_path(&canonicalish(&runtime).unwrap())
        ));
        let lease: InstanceRegistryEntry = read_json(&lease_path).unwrap();
        assert_eq!(lease.instance_id, owner.summary().instance_id);
        assert_eq!(lease.mode, "read_write");

        // All three are discoverable; two carry read_only mode.
        let instances = list_instances(Some(&registry)).unwrap();
        assert_eq!(instances.len(), 3);
        let read_only = instances.iter().filter(|e| e.mode == "read_only").count();
        assert_eq!(read_only, 2);

        // ReadOnly release removes only its own discovery entry, never the lease.
        ro_a.release().unwrap();
        assert!(lease_path.exists());
        let after = list_instances(Some(&registry)).unwrap();
        assert_eq!(after.len(), 2);
    }

    #[test]
    fn readonly_acquire_never_creates_a_lease() {
        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let runtime = temp.path().join("runtime");
        let graph = runtime.join("graph.json");
        let plasticity = runtime.join("plasticity.json");
        let registry = temp.path().join("registry");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&runtime).unwrap();

        let mut ro = InstanceHandle::acquire_with_mode(
            &workspace,
            &runtime,
            &graph,
            &plasticity,
            Some(&registry),
            InstanceMode::ReadOnly,
        )
        .unwrap();
        let lease_path = registry.join(LEASE_DIR_NAME).join(format!(
            "{}.json",
            fingerprint_path(&canonicalish(&runtime).unwrap())
        ));
        assert!(!lease_path.exists());
        // Heartbeats keep the discovery entry fresh without creating a lease.
        ro.mark_heartbeat().unwrap();
        assert!(!lease_path.exists());
        assert_eq!(list_instances(Some(&registry)).unwrap().len(), 1);
    }

    fn lifecycle_paths(handle: &InstanceHandle) -> (PathBuf, Option<PathBuf>) {
        let entry = handle.summary();
        let registry = handle.registry_root();
        let entry_path = registry
            .join(INSTANCE_DIR_NAME)
            .join(format!("{}.json", entry.instance_id));
        let lease_path = (handle.mode() == InstanceMode::ReadWrite).then(|| {
            registry.join(LEASE_DIR_NAME).join(format!(
                "{}.json",
                fingerprint_path(Path::new(&entry.runtime_root))
            ))
        });
        (entry_path, lease_path)
    }

    fn assert_lifecycle_files_absent(entry_path: &Path, lease_path: Option<&Path>) {
        assert!(!entry_path.exists(), "instance entry must be absent");
        if let Some(lease_path) = lease_path {
            assert!(!lease_path.exists(), "writer lease must be absent");
        }
    }

    #[test]
    fn same_pid_readwrite_owner_is_refused_until_unique_handle_releases() {
        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let runtime = temp.path().join("runtime");
        let graph = runtime.join("graph.json");
        let plasticity = runtime.join("plasticity.json");
        let registry = temp.path().join("registry");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&runtime).unwrap();

        let mut first =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .unwrap();
        let error =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .expect_err("a live same-PID writer is still a duplicate owner");
        assert!(error.to_string().contains("already owned"));

        first.release().unwrap();
        let replacement =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .expect("release opens the only valid same-PID reacquisition seam");
        assert_ne!(
            replacement.summary().instance_id,
            first.summary().instance_id
        );
    }

    #[test]
    fn concurrent_readwrite_acquire_has_one_winner_without_lease_overwrite() {
        const CONTENDERS: usize = 16;

        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let runtime = temp.path().join("runtime");
        let graph = runtime.join("graph.json");
        let plasticity = runtime.join("plasticity.json");
        let registry = temp.path().join("registry");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&runtime).unwrap();

        let barrier = Arc::new(std::sync::Barrier::new(CONTENDERS));
        let mut threads = Vec::with_capacity(CONTENDERS);
        for _ in 0..CONTENDERS {
            let workspace = workspace.clone();
            let runtime = runtime.clone();
            let graph = graph.clone();
            let plasticity = plasticity.clone();
            let registry = registry.clone();
            let barrier = Arc::clone(&barrier);
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                    .map_err(|error| error.to_string())
            }));
        }

        let mut winners = Vec::new();
        let mut losers = Vec::new();
        for thread in threads {
            match thread.join().unwrap() {
                Ok(handle) => winners.push(handle),
                Err(error) => losers.push(error),
            }
        }

        assert_eq!(winners.len(), 1, "create_new admits exactly one writer");
        assert_eq!(losers.len(), CONTENDERS - 1);
        assert!(losers.iter().all(|error| error.contains("already owned")));

        let winner = winners.pop().unwrap();
        let winner_entry = winner.summary();
        let lease_path = registry.join(LEASE_DIR_NAME).join(format!(
            "{}.json",
            fingerprint_path(&canonicalish(&runtime).unwrap())
        ));
        let lease: InstanceRegistryEntry = read_json(&lease_path).unwrap();
        assert!(lease_identity_matches(&lease, &winner_entry));

        let instances = list_instances(Some(&registry)).unwrap();
        assert_eq!(instances.len(), 1, "losers publish no discovery entries");
        assert_eq!(instances[0].instance_id, winner_entry.instance_id);
    }

    #[test]
    fn lifetime_guard_blocks_live_stale_takeover_then_legacy_stale_remains_recoverable() {
        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let runtime = temp.path().join("runtime");
        let graph = runtime.join("graph.json");
        let plasticity = runtime.join("plasticity.json");
        let registry = temp.path().join("registry");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&runtime).unwrap();

        let first =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .unwrap();
        let mut foreign_process = spawn_live_pid_fixture();
        let mut foreign = first.summary();
        foreign.instance_id = "inst_foreign_stale".into();
        foreign.pid = foreign_process.id();
        foreign.last_heartbeat_ms = 0;
        let lease_path = registry.join(LEASE_DIR_NAME).join(format!(
            "{}.json",
            fingerprint_path(&canonicalish(&runtime).unwrap())
        ));
        save_json_atomic(&lease_path, &foreign).unwrap();

        let guarded_error =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .expect_err("a modern owner lifetime guard outranks a stale JSON heartbeat");
        assert!(guarded_error.to_string().contains("already owned"));

        // Dropping the modern owner crash-releases its OS guard. The injected
        // foreign JSON now models a legacy owner that predates lifetime locks;
        // its historical live-but-stale recovery rule remains compatible.
        drop(first);
        let replacement =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .expect("legacy foreign-stale owner remains recoverable after guard release");
        assert_ne!(replacement.summary().instance_id, foreign.instance_id);
        let _ = foreign_process.kill();
        let _ = foreign_process.wait();
    }

    #[test]
    fn release_revokes_permit_and_cannot_be_undone_by_late_heartbeats() {
        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let runtime = temp.path().join("runtime");
        let graph = runtime.join("graph.json");
        let plasticity = runtime.join("plasticity.json");
        let registry = temp.path().join("registry");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&runtime).unwrap();

        let mut owner =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .unwrap();
        let permit = owner.heartbeat_permit();
        let (entry_path, lease_path) = lifecycle_paths(&owner);
        assert!(permit.heartbeat().unwrap());

        owner.release().unwrap();
        assert_lifecycle_files_absent(&entry_path, lease_path.as_deref());
        for _ in 0..8 {
            assert!(!permit.heartbeat().unwrap());
        }
        assert_lifecycle_files_absent(&entry_path, lease_path.as_deref());
        owner.release().expect("release is idempotent");
    }

    #[test]
    fn heartbeat_permit_is_weak_and_drop_releases_the_unique_owner() {
        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let runtime = temp.path().join("runtime");
        let graph = runtime.join("graph.json");
        let plasticity = runtime.join("plasticity.json");
        let registry = temp.path().join("registry");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&runtime).unwrap();

        let (permit, entry_path, lease_path) = {
            let owner =
                InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                    .unwrap();
            let permit = owner.heartbeat_permit();
            let (entry_path, lease_path) = lifecycle_paths(&owner);
            (permit, entry_path, lease_path)
        };

        assert!(!permit.heartbeat().unwrap());
        assert_lifecycle_files_absent(&entry_path, lease_path.as_deref());
    }

    #[test]
    fn concurrent_heartbeat_cannot_resurrect_files_after_release_returns() {
        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let runtime = temp.path().join("runtime");
        let graph = runtime.join("graph.json");
        let plasticity = runtime.join("plasticity.json");
        let registry = temp.path().join("registry");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&runtime).unwrap();

        let mut owner =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .unwrap();
        let permit = owner.heartbeat_permit();
        let (entry_path, lease_path) = lifecycle_paths(&owner);
        let ready = Arc::new(std::sync::Barrier::new(2));
        let race = Arc::new(std::sync::Barrier::new(2));
        let worker_ready = Arc::clone(&ready);
        let worker_race = Arc::clone(&race);
        let worker = std::thread::spawn(move || {
            assert!(permit.heartbeat().unwrap());
            worker_ready.wait();
            worker_race.wait();
            while permit.heartbeat().unwrap() {
                std::thread::yield_now();
            }
        });

        ready.wait();
        race.wait();
        owner.release().unwrap();
        worker.join().unwrap();
        assert_lifecycle_files_absent(&entry_path, lease_path.as_deref());
    }

    #[test]
    fn clear_running_endpoint_withdraws_discovery_without_releasing_owner() {
        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let runtime = temp.path().join("runtime");
        let graph = runtime.join("graph.json");
        let plasticity = runtime.join("plasticity.json");
        let registry = temp.path().join("registry");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&runtime).unwrap();

        let mut owner =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .unwrap();
        owner
            .set_running_endpoint("127.0.0.1".into(), 1337)
            .unwrap();
        owner.clear_running_endpoint().unwrap();
        let summary = owner.summary();
        assert!(summary.bind.is_none());
        assert!(summary.port.is_none());
        assert!(lifecycle_paths(&owner).0.exists());
    }

    #[test]
    fn gc_removes_dead_entries_and_keeps_live_ones() {
        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let runtime = temp.path().join("runtime");
        let graph = runtime.join("graph.json");
        let plasticity = runtime.join("plasticity.json");
        let registry = temp.path().join("registry");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&runtime).unwrap();

        // Live owner (current pid) — must survive GC.
        let live =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .unwrap();
        let live_entry_path = registry
            .join(INSTANCE_DIR_NAME)
            .join(format!("{}.json", live.summary().instance_id));
        let live_lease_path = registry.join(LEASE_DIR_NAME).join(format!(
            "{}.json",
            fingerprint_path(&canonicalish(&runtime).unwrap())
        ));

        // Plant a dead lease + dead instance entry under a different runtime root.
        let mut dead = live.summary();
        dead.instance_id = "inst_dead".into();
        dead.pid = u32::MAX - 1; // never live
        dead.runtime_root = "/tmp/dead-runtime".into();
        let dead_entry_path = registry.join(INSTANCE_DIR_NAME).join("inst_dead.json");
        let dead_lease_path = registry.join(LEASE_DIR_NAME).join("deadfingerprint.json");
        save_json_atomic(&dead_entry_path, &dead).unwrap();
        save_json_atomic(&dead_lease_path, &dead).unwrap();

        // A corrupt file must be skipped, not deleted.
        let corrupt_path = registry.join(LEASE_DIR_NAME).join("corrupt.json");
        fs::write(&corrupt_path, "{ not valid json").unwrap();

        let report = gc_dead_leases(&registry).unwrap();
        assert_eq!(report.leases_removed, 1);
        assert_eq!(report.instances_removed, 1);
        // scanned counts only successfully-parsed entries.
        assert_eq!(report.scanned, 4);

        // Dead entries gone; live entries and the corrupt file remain.
        assert!(!dead_entry_path.exists());
        assert!(!dead_lease_path.exists());
        assert!(live_entry_path.exists());
        assert!(live_lease_path.exists());
        assert!(corrupt_path.exists());
    }

    // Boot path: `spawn_boot_gc` must sweep dead-pid entries while keeping the
    // live owner — mirrors `gc_removes_dead_entries_and_keeps_live_ones` but
    // drives the sweep through the boot entry point that `SessionState::initialize`
    // calls. Also proves the boot call is non-blocking: it returns a JoinHandle
    // *immediately* (before the sweep can finish), and the work completes only
    // once we join.
    #[test]
    fn boot_gc_sweeps_dead_entry_and_keeps_live_one() {
        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let runtime = temp.path().join("runtime");
        let graph = runtime.join("graph.json");
        let plasticity = runtime.join("plasticity.json");
        let registry = temp.path().join("registry");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&runtime).unwrap();

        // Live owner (current pid) — must survive the boot sweep.
        let live =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .unwrap();
        let live_entry_path = registry
            .join(INSTANCE_DIR_NAME)
            .join(format!("{}.json", live.summary().instance_id));
        let live_lease_path = registry.join(LEASE_DIR_NAME).join(format!(
            "{}.json",
            fingerprint_path(&canonicalish(&runtime).unwrap())
        ));

        // Plant a dead lease + dead instance entry (pid never live).
        let mut dead = live.summary();
        dead.instance_id = "inst_dead".into();
        dead.pid = u32::MAX - 1;
        dead.runtime_root = "/tmp/dead-runtime".into();
        let dead_entry_path = registry.join(INSTANCE_DIR_NAME).join("inst_dead.json");
        let dead_lease_path = registry.join(LEASE_DIR_NAME).join("deadfingerprint.json");
        save_json_atomic(&dead_entry_path, &dead).unwrap();
        save_json_atomic(&dead_lease_path, &dead).unwrap();

        // Drive the sweep through the boot entry point. `spawn_boot_gc` must
        // return the handle promptly (fire-and-forget) rather than block on the
        // sweep — a 25k-file dir at boot must not stall the handshake. This bound is
        // a coarse "returned rather than joined the sweep" net only: with a fixture
        // this small the sweep itself is milliseconds, so the number can be generous
        // without giving anything up, and a one-second ceiling on a bare
        // `thread::spawn` was measuring machine load.
        let started = std::time::Instant::now();
        let handle = spawn_boot_gc(live.registry_root());
        let spawn_elapsed = started.elapsed();
        assert!(
            spawn_elapsed < std::time::Duration::from_secs(30),
            "spawn_boot_gc must return immediately (non-blocking); took {:?}",
            spawn_elapsed,
        );

        // Join only to make the assertions deterministic (production drops it).
        handle.join().unwrap();

        // Dead entries swept; live owner kept.
        assert!(!dead_entry_path.exists());
        assert!(!dead_lease_path.exists());
        assert!(live_entry_path.exists());
        assert!(live_lease_path.exists());
    }

    // Regression for the once-per-sweep liveness design: a single
    // `gc_dead_leases` sweep over K planted dead-pid entries removes all K while
    // keeping the live owner — and spawns ZERO subprocesses for liveness.
    //
    // The no-subprocess property is guaranteed *by construction*: liveness now
    // flows through `LivePids` (one in-process `sysinfo` read shared across the
    // whole sweep), and the only `Command` spawns in this module are this test
    // module's `spawn_live_pid_fixture` (for foreign-owner collision tests),
    // which this test never calls. With K = many entries the old per-entry
    // `kill -0` path would have spawned K subprocesses; the new path spawns none
    // and reads the process table exactly once.
    #[test]
    fn gc_sweep_removes_k_dead_entries_keeps_live_without_subprocesses() {
        const K: usize = 64;

        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace");
        let runtime = temp.path().join("runtime");
        let graph = runtime.join("graph.json");
        let plasticity = runtime.join("plasticity.json");
        let registry = temp.path().join("registry");
        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&runtime).unwrap();

        // Live owner (current pid) — must survive the sweep.
        let live =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .unwrap();
        let live_entry_path = registry
            .join(INSTANCE_DIR_NAME)
            .join(format!("{}.json", live.summary().instance_id));
        let live_lease_path = registry.join(LEASE_DIR_NAME).join(format!(
            "{}.json",
            fingerprint_path(&canonicalish(&runtime).unwrap())
        ));

        // Plant K dead instance entries (each a never-live pid).
        let mut dead_paths = Vec::with_capacity(K);
        for i in 0..K {
            let mut dead = live.summary();
            dead.instance_id = format!("inst_dead_{i}");
            dead.pid = u32::MAX - 1 - i as u32; // never live
            dead.runtime_root = format!("/tmp/dead-runtime-{i}");
            let path = registry
                .join(INSTANCE_DIR_NAME)
                .join(format!("inst_dead_{i}.json"));
            save_json_atomic(&path, &dead).unwrap();
            dead_paths.push(path);
        }

        // A single sweep: one process-table read, no per-entry subprocesses.
        let report = gc_dead_leases(&registry).unwrap();

        // All K dead entries gone; live owner (entry + lease) kept.
        assert_eq!(report.instances_removed, K);
        for path in &dead_paths {
            assert!(!path.exists(), "dead entry should be swept: {path:?}");
        }
        assert!(live_entry_path.exists());
        assert!(live_lease_path.exists());
    }

    // ── STABLE BRAIN ID + inherited-duplicate reconcile ─────────────────────────
    // Field repro (reproduced twice on-screen 2026-07-11): clicking "Open brain"
    // on a dormant project brain DUPLICATED its Hall card. Root cause: a brain
    // warm-boot minted a NEW ephemeral `instances/<id>.json` each time. Linear
    // handles now release on Drop, while the deterministic brain id preserves
    // identity across clean boots and the reconcile still sweeps stale twins
    // inherited from pre-linear-handle builds.

    /// How many `instances/*.json` entries the registry holds.
    fn count_instance_files(registry: &Path) -> usize {
        fs::read_dir(registry.join(INSTANCE_DIR_NAME))
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
                    .count()
            })
            .unwrap_or(0)
    }

    /// The project-brain store shape: workspace_root == runtime_root == the store
    /// dir (mirrors `project_brains::boot_store`, whose `runtime_dir` IS the store
    /// and whose inferred workspace_root falls back to that same store dir).
    fn brain_store(temp: &Path, name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let store = temp.join("project-brains").join(name);
        fs::create_dir_all(&store).unwrap();
        let graph = store.join("graph_snapshot.json");
        let plasticity = store.join("plasticity_state.json");
        (store, graph, plasticity)
    }

    #[test]
    fn brain_rekey_lease_failure_rolls_back_staged_identity_for_drop_cleanup() {
        let temp = tempdir().unwrap();
        let registry = temp.path().join("registry");
        let (store, graph, plasticity) = brain_store(temp.path(), "fingerprintA");
        let mut brain =
            InstanceHandle::acquire(&store, &store, &graph, &plasticity, Some(&registry)).unwrap();
        let before = brain.summary();
        let (old_entry_path, lease_path) = lifecycle_paths(&brain);
        let lease_path = lease_path.unwrap();

        let stable_id = stable_brain_instance_id(&before.workspace_root, &before.runtime_root);
        let stable_entry_path = registry
            .join(INSTANCE_DIR_NAME)
            .join(format!("{stable_id}.json"));
        // The stable discovery entry stages successfully first. A directory at
        // the lease's exact `.tmp` path then deterministically fails the lease
        // commit, exercising rollback after the new identity reached disk.
        let fault_path = lease_path.with_extension("tmp");
        fs::create_dir_all(&fault_path).unwrap();

        let error = brain
            .set_brain_kind("project")
            .expect_err("faulted lease commit must abort and roll back the re-key");
        assert!(!error.to_string().is_empty());
        assert_eq!(brain.summary().instance_id, before.instance_id);
        assert_eq!(brain.summary().brain_kind, before.brain_kind);
        assert!(old_entry_path.exists());
        assert!(!stable_entry_path.exists());
        let lease: InstanceRegistryEntry = read_json(&lease_path).unwrap();
        assert!(lease_identity_matches(&lease, &before));

        fs::remove_dir(&fault_path).unwrap();
        drop(brain);
        assert_lifecycle_files_absent(&old_entry_path, Some(&lease_path));
    }

    /// A clean release followed by a second boot of the SAME brain re-registers
    /// onto the SAME stable id and leaves exactly one current file.
    #[test]
    fn brain_reregister_upserts_one_stable_entry() {
        let temp = tempdir().unwrap();
        let registry = temp.path().join("registry");
        let (store, graph, plasticity) = brain_store(temp.path(), "fingerprintA");

        // Boot 1: acquire (ephemeral id) → stamp brain kind (re-key to stable).
        let stable_id = {
            let mut h1 =
                InstanceHandle::acquire(&store, &store, &graph, &plasticity, Some(&registry))
                    .unwrap();
            h1.set_brain_kind("project").unwrap();
            let id = h1.summary().instance_id;
            assert_eq!(
                count_instance_files(&registry),
                1,
                "boot 1 writes one entry"
            );
            id
            // h1 drops here: linear Drop releases both registry files before the
            // next same-PID owner may acquire this runtime.
        };
        assert_eq!(
            count_instance_files(&registry),
            0,
            "clean owner Drop releases the stable entry before reboot"
        );

        // Boot 2 (a warm-boot of the SAME store): a fresh handle, a distinguishable
        // endpoint, then the brain-kind stamp that re-keys onto the SAME stable id.
        let mut h2 =
            InstanceHandle::acquire(&store, &store, &graph, &plasticity, Some(&registry)).unwrap();
        h2.set_running_endpoint("127.0.0.1".into(), 4321).unwrap();
        h2.set_brain_kind("project").unwrap();

        assert_eq!(
            h2.summary().instance_id,
            stable_id,
            "the same store re-registers onto the SAME stable id across boots"
        );
        assert_eq!(
            count_instance_files(&registry),
            1,
            "a warm-boot UPSERTS the one file — never a duplicate card"
        );
        // The single surviving entry carries boot 2's content (the upsert rewrote it).
        let entry_path = registry
            .join(INSTANCE_DIR_NAME)
            .join(format!("{stable_id}.json"));
        let on_disk: InstanceRegistryEntry = read_json(&entry_path).unwrap();
        assert_eq!(on_disk.instance_id, stable_id);
        assert_eq!(on_disk.port, Some(4321), "boot 2's content won the upsert");
        assert_eq!(on_disk.brain_kind.as_deref(), Some("project"));
    }

    /// A stale ephemeral twin of the SAME store — the inheritance an earlier
    /// ephemeral-id boot left behind, still under a LIVE owner pid (the exact case
    /// the dead-pid GC can never reap) — is reconciled away on the next brain
    /// registration.
    #[test]
    fn brain_reregister_reconciles_stale_ephemeral_twin() {
        let temp = tempdir().unwrap();
        let registry = temp.path().join("registry");
        let (store, graph, plasticity) = brain_store(temp.path(), "fingerprintA");

        let mut brain =
            InstanceHandle::acquire(&store, &store, &graph, &plasticity, Some(&registry)).unwrap();
        brain.set_brain_kind("project").unwrap();
        let stable_id = brain.summary().instance_id;

        // Plant a pre-fix duplicate: same (workspace_root, runtime_root), a DIFFERENT
        // (ephemeral) id, mode read_write, under THIS live process pid — so only the
        // reconcile (not the dead-pid GC) can ever remove it.
        let mut twin = brain.summary();
        twin.instance_id = "inst_stale_ephemeral_twin".into();
        twin.pid = std::process::id();
        let twin_path = registry
            .join(INSTANCE_DIR_NAME)
            .join("inst_stale_ephemeral_twin.json");
        save_json_atomic(&twin_path, &twin).unwrap();
        assert_eq!(count_instance_files(&registry), 2, "the twin is planted");

        // Re-register the brain → the reconcile sweeps the same-store twin.
        brain.set_brain_kind("project").unwrap();

        assert_eq!(
            count_instance_files(&registry),
            1,
            "the stale ephemeral twin is reconciled away on re-register"
        );
        assert!(!twin_path.exists(), "the twin entry file is gone");
        assert!(
            registry
                .join(INSTANCE_DIR_NAME)
                .join(format!("{stable_id}.json"))
                .exists(),
            "the brain's own stable entry survives the reconcile"
        );
    }

    /// The reconcile NEVER removes a live read_only attacher of the same store — N
    /// attachers coexist with one brain by design. Re-registering the brain leaves
    /// the two attacher entries intact (1 brain + 2 attachers = 3 entries).
    #[test]
    fn set_brain_kind_preserves_live_readonly_attachers() {
        let temp = tempdir().unwrap();
        let registry = temp.path().join("registry");
        let (store, graph, plasticity) = brain_store(temp.path(), "fingerprintA");

        let mut brain =
            InstanceHandle::acquire(&store, &store, &graph, &plasticity, Some(&registry)).unwrap();
        brain.set_brain_kind("project").unwrap();

        // Two ReadOnly attachers to the SAME store (they never call set_brain_kind,
        // so they keep their distinct ephemeral ids — the N-attacher design).
        let _ro_a = InstanceHandle::acquire_with_mode(
            &store,
            &store,
            &graph,
            &plasticity,
            Some(&registry),
            InstanceMode::ReadOnly,
        )
        .unwrap();
        let _ro_b = InstanceHandle::acquire_with_mode(
            &store,
            &store,
            &graph,
            &plasticity,
            Some(&registry),
            InstanceMode::ReadOnly,
        )
        .unwrap();
        assert_eq!(count_instance_files(&registry), 3, "brain + 2 attachers");

        // Re-register the brain → the reconcile must LEAVE the live attachers.
        brain.set_brain_kind("project").unwrap();
        assert_eq!(
            count_instance_files(&registry),
            3,
            "live read_only attachers are never reconciled away (N-attacher design)"
        );
        let read_only = list_instances(Some(&registry))
            .unwrap()
            .into_iter()
            .filter(|e| e.mode == "read_only")
            .count();
        assert_eq!(read_only, 2, "both attacher entries still discoverable");
    }

    /// Two DIFFERENT brains (distinct stores) each get their own stable entry — the
    /// reconcile of one store never touches another. Different workspaces → two
    /// entries.
    #[test]
    fn distinct_stores_get_distinct_stable_brain_entries() {
        let temp = tempdir().unwrap();
        let registry = temp.path().join("registry");
        let (store_a, graph_a, plasticity_a) = brain_store(temp.path(), "fingerprintA");
        let (store_b, graph_b, plasticity_b) = brain_store(temp.path(), "fingerprintB");

        let mut a =
            InstanceHandle::acquire(&store_a, &store_a, &graph_a, &plasticity_a, Some(&registry))
                .unwrap();
        a.set_brain_kind("project").unwrap();
        let mut b =
            InstanceHandle::acquire(&store_b, &store_b, &graph_b, &plasticity_b, Some(&registry))
                .unwrap();
        b.set_brain_kind("project").unwrap();

        assert_eq!(
            count_instance_files(&registry),
            2,
            "two distinct stores keep two distinct entries"
        );
        assert_ne!(
            a.summary().instance_id,
            b.summary().instance_id,
            "distinct stores hash to distinct stable ids"
        );
    }
}
