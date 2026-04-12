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

#[derive(Clone, Debug)]
pub struct InstanceHandle {
    inner: Arc<Mutex<InstanceHandleInner>>,
}

#[derive(Clone, Debug)]
struct InstanceHandleInner {
    entry: InstanceRegistryEntry,
    registry_root: PathBuf,
    entry_path: PathBuf,
    lock_path: PathBuf,
}

impl InstanceHandle {
    pub fn acquire(
        workspace_root: &Path,
        runtime_root: &Path,
        graph_source: &Path,
        plasticity_state: &Path,
        registry_root: Option<&Path>,
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

        let lock_path = registry_root
            .join(LEASE_DIR_NAME)
            .join(format!("{}.json", fingerprint_path(&runtime_root)));
        if lock_path.exists() {
            let existing: InstanceRegistryEntry = read_json(&lock_path)?;
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
            mode: "read_write".into(),
            status: "starting".into(),
            owner_live: Some(true),
            stale: false,
            conflicts: Vec::new(),
        };

        let entry_path = registry_root
            .join(INSTANCE_DIR_NAME)
            .join(format!("{}.json", instance_id));
        save_json_atomic(&lock_path, &entry)?;
        save_json_atomic(&entry_path, &entry)?;

        Ok(Self {
            inner: Arc::new(Mutex::new(InstanceHandleInner {
                entry,
                registry_root,
                entry_path,
                lock_path,
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

    pub fn release(&self) -> M1ndResult<()> {
        let inner = self.inner.lock();
        let _ = fs::remove_file(&inner.lock_path);
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
    save_json_atomic(&inner.entry_path, &inner.entry)?;
    save_json_atomic(&inner.lock_path, &inner.entry)
}

fn generate_instance_id(workspace_root: &Path, runtime_root: &Path, now_ms: u64) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    workspace_root.to_string_lossy().hash(&mut hasher);
    runtime_root.to_string_lossy().hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    now_ms.hash(&mut hasher);
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
        let _ = pid;
        true
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
    use tempfile::tempdir;

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
        let mut foreign = first.summary();
        foreign.instance_id = "inst_foreign".into();
        foreign.pid = 1;
        foreign.last_heartbeat_ms = now_ms();
        let lock_path = registry.join(LEASE_DIR_NAME).join(format!(
            "{}.json",
            fingerprint_path(&canonicalish(&runtime).unwrap())
        ));
        save_json_atomic(&lock_path, &foreign).unwrap();
        let err =
            InstanceHandle::acquire(&workspace, &runtime, &graph, &plasticity, Some(&registry))
                .unwrap_err();
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
}
