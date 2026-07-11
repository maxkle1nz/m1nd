use crate::util::now_ms;
use m1nd_core::error::{M1ndError, M1ndResult};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::task::JoinHandle;
use tokio::time::{interval, Duration};

const INSTANCE_DIR_NAME: &str = "instances";
const LEASE_DIR_NAME: &str = "leases";
const DEFAULT_REGISTRY_SUBDIR: &str = ".m1nd/registry";
const STALE_AFTER_MS: u64 = 30_000;

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
pub enum InstanceMode {
    ReadWrite,
    ReadOnly,
}

impl InstanceMode {
    /// On-disk string used in the `mode` field. Kept stable for backward
    /// compatibility with the ~54k existing lease/instance JSON files.
    pub fn as_str(self) -> &'static str {
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
    pub fn from_str(value: &str) -> Self {
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

#[derive(Clone, Debug)]
pub struct InstanceHandle {
    inner: Arc<Mutex<InstanceHandleInner>>,
}

#[derive(Clone, Debug)]
struct InstanceHandleInner {
    entry: InstanceRegistryEntry,
    registry_root: PathBuf,
    entry_path: PathBuf,
    /// `Some` only for `ReadWrite` handles. `ReadOnly` handles hold no
    /// exclusive lease, so they have no lease file to refresh or remove.
    lock_path: Option<PathBuf>,
    mode: InstanceMode,
}

impl InstanceHandle {
    /// Acquire in the default `ReadWrite` mode. Behavior is identical to the
    /// historical single-argument-set version; existing callers are untouched.
    pub fn acquire(
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
    /// `ReadWrite` is the exclusive PID+heartbeat lease (unchanged from before):
    /// if a live, non-stale, foreign owner holds the lease for this
    /// `runtime_root`, this returns `AlreadyExists`.
    ///
    /// `ReadOnly` always succeeds and never touches the lease file. It only
    /// writes an `instances/<id>.json` entry (with `mode:"read_only"`) so the
    /// attacher is discoverable via `list_instances`. Multiple `ReadOnly`
    /// attachers and one `ReadWrite` owner coexist with zero conflict.
    pub fn acquire_with_mode(
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

        // ReadWrite is the only mode that contends for the exclusive lease.
        // ReadOnly never inspects, steals, or overwrites the lease file.
        if mode == InstanceMode::ReadWrite && lease_file.exists() {
            let existing: InstanceRegistryEntry = read_json(&lease_file)?;
            let live = is_pid_live(existing.pid);
            let stale = is_stale(existing.last_heartbeat_ms);
            if live && !stale && existing.pid != std::process::id() {
                return Err(M1ndError::Io(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!(
                        "runtime_root {} is already owned by instance {} (pid {})",
                        runtime_root.display(),
                        existing.instance_id,
                        existing.pid
                    ),
                )));
            }
        }

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

        // ReadOnly: discovery entry only, no exclusive lease.
        let lock_path = match mode {
            InstanceMode::ReadWrite => {
                save_json_atomic(&lease_file, &entry)?;
                Some(lease_file)
            }
            InstanceMode::ReadOnly => None,
        };
        save_json_atomic(&entry_path, &entry)?;

        Ok(Self {
            inner: Arc::new(Mutex::new(InstanceHandleInner {
                entry,
                registry_root,
                entry_path,
                lock_path,
                mode,
            })),
        })
    }

    pub fn set_running_endpoint(&self, bind: String, port: u16) -> M1ndResult<()> {
        let mut inner = self.inner.lock();
        inner.entry.bind = Some(bind);
        inner.entry.port = Some(port);
        inner.entry.status = "running".into();
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
    /// every boot. For a long-lived owner that is harmless (its dead-pid entry is
    /// GC'd on restart), but a hosted brain warm-boots repeatedly WITHIN one live
    /// owner (the eviction gate drops a brain's handle without `release`, so its
    /// `instances/<id>.json` lingers under a still-live pid, and the next resolve
    /// mints a NEW ephemeral id) — so the Hall listed one card PER boot. A brain
    /// entry therefore takes a DETERMINISTIC id, `inst_<hash(workspace+runtime)>`,
    /// stable across every warm-boot of the same store, so the boot UPSERTS the
    /// SAME file instead of minting a duplicate. Attachers never reach here
    /// (`ReadOnly` never calls `set_brain_kind`), so the N-attacher design keeps
    /// its ephemeral, per-attacher ids untouched.
    pub fn set_brain_kind(&self, brain_kind: &str) -> M1ndResult<()> {
        let mut inner = self.inner.lock();
        inner.entry.brain_kind = Some(brain_kind.to_string());

        let stable_id =
            stable_brain_instance_id(&inner.entry.workspace_root, &inner.entry.runtime_root);
        if inner.entry.instance_id != stable_id {
            // Drop the ephemeral-id file this handle wrote at `acquire`; the brain
            // is re-keyed onto its stable id (an atomic upsert-by-filename from
            // here on — the warm-boot rewrites this one file, never a new one).
            let ephemeral_path = inner.entry_path.clone();
            inner.entry.instance_id = stable_id.clone();
            inner.entry_path = inner
                .registry_root
                .join(INSTANCE_DIR_NAME)
                .join(format!("{stable_id}.json"));
            if ephemeral_path != inner.entry_path {
                let _ = fs::remove_file(&ephemeral_path);
            }
        }
        // Write the (re-keyed) entry + refresh the lease, THEN reconcile away any
        // stale duplicates of THIS store left by earlier ephemeral-id boots.
        persist_handle_inner(&inner)?;
        reconcile_brain_duplicates(&inner);
        Ok(())
    }

    pub fn mark_heartbeat(&self) -> M1ndResult<()> {
        let mut inner = self.inner.lock();
        inner.entry.last_heartbeat_ms = now_ms();
        if inner.entry.status == "starting" {
            inner.entry.status = "running".into();
        }
        persist_handle_inner(&inner)
    }

    pub fn mark_degraded(&self) -> M1ndResult<()> {
        let mut inner = self.inner.lock();
        inner.entry.status = "degraded".into();
        inner.entry.last_heartbeat_ms = now_ms();
        persist_handle_inner(&inner)
    }

    pub fn summary(&self) -> InstanceRegistryEntry {
        self.inner.lock().entry.clone()
    }

    pub fn registry_root(&self) -> PathBuf {
        self.inner.lock().registry_root.clone()
    }

    /// The mode this handle was acquired with.
    pub fn mode(&self) -> InstanceMode {
        self.inner.lock().mode
    }

    pub fn release(&self) -> M1ndResult<()> {
        let inner = self.inner.lock();
        // ReadOnly handles hold no lease; only the discovery entry is removed.
        if let Some(lock_path) = &inner.lock_path {
            let _ = fs::remove_file(lock_path);
        }
        let _ = fs::remove_file(&inner.entry_path);
        Ok(())
    }
}

pub fn spawn_heartbeat(instance: InstanceHandle) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(5));
        loop {
            ticker.tick().await;
            let _ = instance.mark_heartbeat();
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
    let _ = fs::remove_file(&lease_path);
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
        &mut report.scanned,
        &mut report.leases_removed,
    )?;
    gc_dead_in_dir(
        &registry_root.join(INSTANCE_DIR_NAME),
        &live,
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
        if fs::remove_file(&path).is_ok() {
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

fn persist_handle_inner(inner: &InstanceHandleInner) -> M1ndResult<()> {
    // Always refresh the discovery entry so heartbeats keep ReadOnly attachers
    // (and ReadWrite owners) visible/live in list_instances.
    save_json_atomic(&inner.entry_path, &inner.entry)?;
    // Refresh the exclusive lease only for ReadWrite handles.
    if let Some(lock_path) = &inner.lock_path {
        save_json_atomic(lock_path, &inner.entry)?;
    }
    Ok(())
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

/// Read-only discovery for the `--attach auto` thin client.
///
/// Given the caller's `runtime_root` (e.g. the cwd or an explicit `--runtime-dir`),
/// find the freshest live ReadWrite owner that is actually serving HTTP for that
/// runtime_root and return its base URL (e.g. `http://127.0.0.1:1337`).
///
/// This is PURE read-only registry inspection: it calls `list_instances` (which
/// only reads `instances/*.json`) and NEVER `acquire`/`acquire_with_mode`, so it
/// takes no lease and never contends the owner's exclusive PID+heartbeat lock.
///
/// Matching mirrors `acquire_with_mode`'s persistence exactly:
///   * the target `runtime_root` is canonicalized with the same `canonicalish`
///     semantics the owner used before writing `entry.runtime_root`, so the
///     string comparison lines up on macOS (`/tmp` → `/private/tmp`, symlinks…);
///   * only `mode == read_write`, `owner_live == Some(true)`, `stale == false`
///     entries that ALSO publish `bind`+`port` (the serve gate) survive;
///   * with multiple survivors the freshest by `last_heartbeat_ms` wins
///     (`list_instances` already sorts descending, so the first survivor is it).
///
/// On no match returns `Err(message)` distinguishing "no instances at all" from
/// "instances exist but none is a live serve ReadWrite owner for this runtime_root".
pub fn discover_serve_owner_base_url(
    runtime_root: &Path,
    registry_dir: Option<&Path>,
) -> Result<String, String> {
    // Canonicalize the target identically to how the owner stored it.
    let target = canonicalish(runtime_root)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| runtime_root.to_string_lossy().into_owned());

    let entries = list_instances(registry_dir)
        .map_err(|e| format!("failed to read instance registry: {e}"))?;

    if entries.is_empty() {
        return Err(format!(
            "no m1nd instances registered (looked for a live serve ReadWrite owner for runtime_root {target})"
        ));
    }

    // `list_instances` is already sorted freshest-first by last_heartbeat_ms, so
    // the FIRST entry that passes every gate is the freshest serve owner.
    for entry in &entries {
        if entry.runtime_root != target {
            continue;
        }
        if InstanceMode::from_str(&entry.mode) != InstanceMode::ReadWrite {
            continue;
        }
        if entry.owner_live != Some(true) || entry.stale {
            continue;
        }
        // Serve gate: stdio-only owners publish no bind/port and are unreachable.
        if let Some(url) = entry_base_url(entry) {
            return Ok(url);
        }
    }

    Err(format!(
        "no live serve ReadWrite owner for runtime_root {target}. \
Start one with: m1nd-mcp --serve --no-gui"
    ))
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

        let handle =
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
        let ro_a = InstanceHandle::acquire_with_mode(
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

        let ro = InstanceHandle::acquire_with_mode(
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
        // sweep — a 25k-file dir at boot must not stall the handshake.
        let started = std::time::Instant::now();
        let handle = spawn_boot_gc(live.registry_root());
        let spawn_elapsed = started.elapsed();
        assert!(
            spawn_elapsed < std::time::Duration::from_secs(1),
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

    // ── STABLE BRAIN ID + reconcile (the "duplicate workspace" field bug) ────────
    // Field repro (reproduced twice on-screen 2026-07-11): clicking "Open brain"
    // on a dormant project brain DUPLICATED its Hall card. Root cause: a brain
    // warm-boot minted a NEW ephemeral `instances/<id>.json` each time (the
    // eviction gate drops a brain's handle without `release`, so its entry lingers
    // under the still-live owner pid, which the dead-pid GC can never reap). The
    // fix: a brain entry (`set_brain_kind` project/medulla) takes a DETERMINISTIC
    // id, so the warm-boot UPSERTS one file, and a reconcile sweeps the stale twins
    // earlier ephemeral boots left behind.

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

    /// A second warm-boot of the SAME brain re-registers onto the SAME stable id
    /// and UPSERTS the one file — never a duplicate — and the second boot's content
    /// wins (proving it rewrote, not stacked). This is the exact "open twice = one
    /// card" proof at the registry layer.
    #[test]
    fn brain_reregister_upserts_one_stable_entry() {
        let temp = tempdir().unwrap();
        let registry = temp.path().join("registry");
        let (store, graph, plasticity) = brain_store(temp.path(), "fingerprintA");

        // Boot 1: acquire (ephemeral id) → stamp brain kind (re-key to stable).
        let stable_id = {
            let h1 = InstanceHandle::acquire(&store, &store, &graph, &plasticity, Some(&registry))
                .unwrap();
            h1.set_brain_kind("project").unwrap();
            let id = h1.summary().instance_id;
            assert_eq!(
                count_instance_files(&registry),
                1,
                "boot 1 writes one entry"
            );
            id
            // h1 dropped WITHOUT release — its stable-id entry lingers on disk,
            // exactly like an eviction-orphaned brain handle.
        };

        // Boot 2 (a warm-boot of the SAME store): a fresh handle, a distinguishable
        // endpoint, then the brain-kind stamp that re-keys onto the SAME stable id.
        let h2 =
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

        let brain =
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

        let brain =
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

        let a =
            InstanceHandle::acquire(&store_a, &store_a, &graph_a, &plasticity_a, Some(&registry))
                .unwrap();
        a.set_brain_kind("project").unwrap();
        let b =
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
