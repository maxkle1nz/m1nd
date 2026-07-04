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
//! SAME `InstanceHandle` machinery (the per-`runtime_root` lease conflicts only
//! across processes — `instance_registry.rs` accepts N leases for one PID), and
//! is filled through the SAME `dispatch_tool("ingest")` path every agent uses.
//! The only net-new surface is this registry map and the store-dir naming.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use m1nd_core::error::{M1ndError, M1ndResult};
use parking_lot::Mutex;

use crate::session::SessionState;

/// Store-dir manifest: records which project root a store belongs to, so a
/// warm-boot can verify the fingerprint really is this root's brain (hash
/// collisions and moved directories resolve to an honest miss, never a silent
/// wrong-brain bind). Inert data only — no binary paths, no exec directives
/// (PRD §9.4 posture, applied to the owner-side store).
const MANIFEST_FILE: &str = "project_brain.json";

/// The dir under the owner's `runtime_root` that holds all project-brain stores.
pub const PROJECT_BRAINS_DIR: &str = "project-brains";

/// Registry of owner-hosted per-project brains, keyed by canonicalized project
/// root. Lives on `AppState` beside (never inside) the bound session.
pub struct ProjectBrainRegistry {
    /// Live brains. The map lock is held only for lookup/insert — never across
    /// an engine build or an ingest (those run on the unshared brain first).
    brains: Mutex<HashMap<String, Arc<Mutex<SessionState>>>>,
    /// `<owner runtime_root>/project-brains`.
    base_dir: PathBuf,
    /// The owner's registry dir, so project-brain instances/leases land in the
    /// SAME phonebook (`brain_kind:"project"` tells them apart — mission D rides
    /// the existing `list_instances` surface, zero new listing code).
    registry_dir: Option<PathBuf>,
}

impl ProjectBrainRegistry {
    pub fn new(base_dir: PathBuf, registry_dir: Option<PathBuf>) -> Self {
        Self {
            brains: Mutex::new(HashMap::new()),
            base_dir,
            registry_dir,
        }
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
    /// `None` = this root has no project brain (the caller belongs to the bound
    /// graph or to reception).
    pub fn resolve(&self, caller_root: &str) -> Option<Arc<Mutex<SessionState>>> {
        let key = Self::canonical_key(caller_root);
        if let Some(brain) = self.brains.lock().get(&key) {
            return Some(brain.clone());
        }
        if !self.manifest_matches(&key) {
            return None;
        }
        // Dormant store → warm-boot OUTSIDE the map lock (engine build is slow).
        let state = self.boot_store(&key).ok()?;
        let built = Arc::new(Mutex::new(state));
        let mut map = self.brains.lock();
        // A racer may have booted it meanwhile — first insert wins, both callers
        // get the same brain.
        Some(map.entry(key).or_insert(built).clone())
    }

    /// Boot (fresh or warm) a store's SessionState through the SAME path the
    /// served owner boots with: `McpServer::new` loads `graph_snapshot.json`
    /// when present, else starts an empty graph; plasticity and sidecars are
    /// anchored on the store dir (its `runtime_root`).
    fn boot_store(&self, key: &str) -> M1ndResult<SessionState> {
        let store = self.store_dir_for(key);
        std::fs::create_dir_all(&store)?;
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
        // Stamp the registry entry so the shared phonebook can tell this brain
        // from the bound dev graph (best-effort: a failed stamp never blocks the
        // brain — the entry just stays kind-less like a legacy one).
        let _ = state.instance.set_brain_kind("project");
        Ok(state)
    }

    /// One-call bootstrap: create (or warm-resolve) the brain for
    /// `project_root`, ingest the repo into it, and return it with the ingest
    /// result. The caller (the HTTP routing layer) binds the wire session and
    /// composes the orientation packet.
    ///
    /// `ingest_args` are the caller's original `ingest` arguments; `path` is
    /// forced to the project root and `project_root` itself is stripped (it is a
    /// routing directive, not an adapter input).
    pub fn bootstrap(
        &self,
        project_root: &str,
        ingest_args: &serde_json::Value,
    ) -> M1ndResult<(Arc<Mutex<SessionState>>, serde_json::Value, bool)> {
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

        let existing = self.resolve(&key);
        let reused = existing.is_some();
        let brain = match existing {
            Some(brain) => brain,
            None => {
                let state = self.boot_store(&key)?;
                // Birth record for warm-boots (inert data only).
                let manifest = self.store_dir_for(&key).join(MANIFEST_FILE);
                let record = serde_json::json!({
                    "schema": "m1nd-project-brain-v0",
                    "project_root": key,
                    "created_ms": crate::util::now_ms(),
                });
                std::fs::write(&manifest, serde_json::to_string_pretty(&record)?)?;
                let built = Arc::new(Mutex::new(state));
                self.brains
                    .lock()
                    .entry(key.clone())
                    .or_insert(built)
                    .clone()
            }
        };

        // Ingest the caller's repo into ITS brain — the same dispatch path any
        // agent ingest takes, so adapter/include_dotfiles options ride along.
        let mut args = ingest_args.clone();
        if let Some(map) = args.as_object_mut() {
            map.remove("project_root");
            map.insert("path".into(), serde_json::Value::String(key.clone()));
        }
        let ingest_result = {
            let mut state = brain.lock();
            state.caller_root = Some(key.clone());
            let result = crate::server::dispatch_tool(&mut state, "ingest", &args)?;
            // Persist immediately so the brain warm-boots even if the owner dies
            // before its auto-persist interval (#230 per-store durability).
            let agent_id = args
                .get("agent_id")
                .cloned()
                .unwrap_or_else(|| serde_json::Value::String("bootstrap".into()));
            let _ = crate::server::dispatch_tool(
                &mut state,
                "persist",
                &serde_json::json!({ "agent_id": agent_id, "action": "save" }),
            );
            result
        };

        Ok((brain, ingest_result, reused))
    }

    /// The store base dir (`<owner runtime_root>/project-brains`) — surfaced for
    /// diagnostics/tests and for the Hall's project-brain name resolution.
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
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
    let text = std::fs::read_to_string(store_dir.join(MANIFEST_FILE)).ok()?;
    let record = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    record["project_root"].as_str().map(|s| s.to_string())
}
