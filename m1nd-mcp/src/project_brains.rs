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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use m1nd_core::error::{M1ndError, M1ndResult};
use parking_lot::Mutex;

use crate::session::SessionState;

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
    brain: Arc<Mutex<SessionState>>,
    /// Monotonic last-touch stamp from the registry's own counter — clock-free so
    /// eviction order is deterministic in tests, never wall-time dependent.
    last_used: u64,
}

/// Registry of owner-hosted per-project brains, keyed by canonicalized project
/// root. Lives on `AppState` beside (never inside) the bound session.
pub struct ProjectBrainRegistry {
    /// Live brains. The map lock is held only for lookup/insert/evict — never
    /// across an engine build or an ingest (those run on the unshared brain
    /// first). Bounded by `capacity`: the LRU eviction gate (§C9.1) persists then
    /// drops the least-recently-used brain before the map exceeds the cap.
    brains: Mutex<HashMap<String, WarmBrain>>,
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
            base_dir,
            registry_dir,
            capacity: capacity.max(1),
            tick: AtomicU64::new(0),
            runnerd_naming: None,
        }
    }

    /// Thread the owner-process naming facts (F11-b) into every project brain this
    /// registry boots. Builder-style, called once at HTTP-owner construction.
    pub fn with_runnerd_naming(mut self, handle: crate::runnerd_owner::NamingRunnerHandle) -> Self {
        self.runnerd_naming = Some(handle);
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
    /// `None` = this root has no project brain (the caller belongs to the bound
    /// graph or to reception).
    pub fn resolve(&self, caller_root: &str) -> Option<Arc<Mutex<SessionState>>> {
        let key = Self::canonical_key(caller_root);
        {
            let mut map = self.brains.lock();
            if let Some(warm) = map.get_mut(&key) {
                // Touch: this is now the most-recently-used brain, so it is the
                // LAST the eviction gate would drop.
                warm.last_used = self.next_tick();
                return Some(warm.brain.clone());
            }
        }
        if !self.manifest_matches(&key) {
            return None;
        }
        // Dormant store → warm-boot OUTSIDE the map lock (engine build is slow).
        let state = self.boot_store(&key).ok()?;
        let built = Arc::new(Mutex::new(state));
        // Insert through the eviction gate: a warm-boot that grows the map past
        // the cap persists-then-drops the LRU victim before this brain lands.
        Some(self.insert_with_eviction(key, built))
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
        // F11-b: a hosted brain's scan reaches the same announced runnerd + owner
        // secret the bound session does (its OWN runtime root is its store dir,
        // never the secret's home).
        state.runnerd_naming = self.runnerd_naming.clone();
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
                // Birth record for warm-boots (inert data only). Counts stamped
                // after ingest below so a DORMANT store still reports its size.
                self.write_manifest(&key, None, None)?;
                let built = Arc::new(Mutex::new(state));
                // Through the eviction gate: bootstrapping brain #cap+1 persists
                // then drops the LRU victim before this new brain lands, so the
                // map never exceeds the cap (§C9.1).
                self.insert_with_eviction(key.clone(), built)
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

        // Stamp the ingested size into the manifest — the CHEAP source the Hall
        // reads for a dormant project brain's counts (parsing the multi-MB
        // graph_snapshot on a list call is banned). A project brain lives
        // in-process, warm-booted lazily: it has no "running" state and no lock,
        // so the Hall shows these recorded counts + freshness, never an
        // instance's process status.
        let node_count = ingest_result.get("node_count").and_then(|v| v.as_u64());
        let edge_count = ingest_result.get("edge_count").and_then(|v| v.as_u64());
        let _ = self.write_manifest(&key, node_count, edge_count);

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
    /// on disk is unchanged; nothing was mutated in it). Otherwise we evict down
    /// to room, then insert `built` fresh.
    fn insert_with_eviction(
        &self,
        key: String,
        built: Arc<Mutex<SessionState>>,
    ) -> Arc<Mutex<SessionState>> {
        // Collect victims under the lock, but persist + drop them OUTSIDE it (a
        // graph snapshot write is slow and must not stall other routed calls).
        let victims: Vec<Arc<Mutex<SessionState>>>;
        let resolved: Arc<Mutex<SessionState>>;
        {
            let mut map = self.brains.lock();
            if let Some(warm) = map.get_mut(&key) {
                // Racer won — adopt the incumbent, touch it, discard `built`.
                warm.last_used = self.next_tick();
                return warm.brain.clone();
            }
            // Evict LRU victims until inserting one more stays within the cap.
            let mut evicted = Vec::new();
            while map.len() + 1 > self.capacity {
                let Some(victim_key) = map
                    .iter()
                    .min_by_key(|(_, w)| w.last_used)
                    .map(|(k, _)| k.clone())
                else {
                    break; // map empty (cap is ≥1, so this cannot loop forever)
                };
                if let Some(warm) = map.remove(&victim_key) {
                    evicted.push(warm.brain);
                }
            }
            let last_used = self.next_tick();
            map.insert(
                key,
                WarmBrain {
                    brain: built.clone(),
                    last_used,
                },
            );
            victims = evicted;
            resolved = built;
        }
        // PERSIST-ON-EVICT: flush each victim's graph to its store BEFORE the Arc
        // drops, so a later routed call warm-boots it back identical (#230/#262).
        // Best-effort per victim: a persist failure is logged, never fatal — the
        // gate's job is to bound memory; the store keeps its last good snapshot.
        for victim in victims {
            if let Err(e) = victim.lock().persist() {
                eprintln!("[m1nd] WARNING: project-brain persist-on-evict failed: {e}");
            }
        }
        resolved
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
        let brain = self.brains.lock().get(&key).map(|w| w.brain.clone())?;
        let state = brain.lock();
        let g = state.graph.read();
        Some((g.num_nodes() as u64, g.num_edges() as u64))
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
        let brain = self.brains.lock().get(&key).map(|w| w.brain.clone())?;
        let state = brain.lock();
        Some((
            state.sessions.len() as u64,
            state.queries_processed,
            state.calibration_armed(),
        ))
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
        let state_a = reg.boot_store(&key_a).expect("boot A");
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
        let brain_a = Arc::new(Mutex::new(state_a));
        reg.insert_with_eviction(key_a.clone(), brain_a);
        assert_eq!(reg.warm_len(), 1, "A is the sole warm brain");
        assert!(
            !snapshot_a.exists(),
            "A's mutation is still only in memory — no snapshot yet"
        );

        // Insert brain B → cap is 1 → A (the LRU, and only) is evicted. The gate
        // MUST persist A before dropping it.
        let state_b = reg.boot_store(&key_b).expect("boot B");
        let brain_b = Arc::new(Mutex::new(state_b));
        reg.insert_with_eviction(key_b.clone(), brain_b);

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
            reg.insert_with_eviction(key, Arc::new(Mutex::new(state)));
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
