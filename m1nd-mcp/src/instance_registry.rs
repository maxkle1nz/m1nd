use m1nd_core::error::{M1ndError, M1ndResult};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
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
/// JSON entry whose recorded `pid` is provably NOT live (via `is_pid_live`).
/// Entries owned by a live pid are NEVER removed. Any entry that fails to read
/// or parse is skipped (never deleted), so corrupt/foreign files are left
/// untouched. Safe to call while a live instance is running — only
/// provably-dead entries are removed.
pub fn gc_dead_leases(registry_root: &Path) -> std::io::Result<GcReport> {
    let mut report = GcReport::default();
    gc_dead_in_dir(
        &registry_root.join(LEASE_DIR_NAME),
        &mut report.scanned,
        &mut report.leases_removed,
    )?;
    gc_dead_in_dir(
        &registry_root.join(INSTANCE_DIR_NAME),
        &mut report.scanned,
        &mut report.instances_removed,
    )?;
    Ok(report)
}

/// Sweep a single registry directory, removing only entries whose pid is dead.
fn gc_dead_in_dir(dir: &Path, scanned: &mut usize, removed: &mut usize) -> std::io::Result<()> {
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
        if is_pid_live(entry.pid) {
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

fn fingerprint_path(path: &Path) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn is_stale(last_heartbeat_ms: u64) -> bool {
    now_ms().saturating_sub(last_heartbeat_ms) > STALE_AFTER_MS
}

fn is_pid_live(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .output()
            .map(|output| {
                output.status.success()
                    || String::from_utf8_lossy(&output.stderr)
                        .to_ascii_lowercase()
                        .contains("operation not permitted")
            })
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        #[cfg(windows)]
        {
            Command::new("tasklist")
                .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
                .output()
                .map(|output| {
                    if !output.status.success() {
                        return false;
                    }
                    let stdout = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
                    stdout.contains(&pid.to_string()) && !stdout.contains("no tasks")
                })
                .unwrap_or(false)
        }
        #[cfg(not(windows))]
        {
            let _ = pid;
            false
        }
    }
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Child;
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
        assert_eq!(InstanceMode::from_str("read_write"), InstanceMode::ReadWrite);
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
        let read_only = instances
            .iter()
            .filter(|e| e.mode == "read_only")
            .count();
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
        let dead_entry_path = registry
            .join(INSTANCE_DIR_NAME)
            .join("inst_dead.json");
        let dead_lease_path = registry
            .join(LEASE_DIR_NAME)
            .join("deadfingerprint.json");
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
}
