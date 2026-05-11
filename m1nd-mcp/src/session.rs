// === crates/m1nd-mcp/src/session.rs ===

use m1nd_core::antibody::Antibody;
use m1nd_core::counterfactual::CounterfactualEngine;
use m1nd_core::domain::DomainConfig;
use m1nd_core::error::M1ndResult;
use m1nd_core::graph::{Graph, SharedGraph};
use m1nd_core::plasticity::PlasticityEngine;
use m1nd_core::query::QueryOrchestrator;
use m1nd_core::resonance::ResonanceEngine;
use m1nd_core::temporal::TemporalEngine;
use m1nd_core::topology::TopologyAnalyzer;
use m1nd_core::tremor::TremorRegistry;
use m1nd_core::trust::TrustLedger;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::auto_ingest::AutoIngestState;
use crate::instance_registry::{InstanceHandle, InstanceRegistryEntry};
use crate::perspective::state::{
    LockState, PeekSecurityConfig, PerspectiveLimits, PerspectiveState, WatchTrigger, WatcherEvent,
};
use crate::universal_docs::{load_document_cache, persist_document_cache, DocumentCacheState};

// ---------------------------------------------------------------------------
// AgentSession — per-agent session tracking
// ---------------------------------------------------------------------------

/// Lightweight session record for a connected agent.
pub struct AgentSession {
    pub agent_id: String,
    pub first_seen: Instant,
    pub last_seen: Instant,
    pub query_count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditPreviewState {
    pub preview_id: String,
    pub agent_id: String,
    pub file_path: String,
    pub new_content: String,
    pub source_hash: String,
    pub source_exists: bool,
    pub source_bytes: usize,
    pub source_line_count: usize,
    pub lines_added: i32,
    pub lines_removed: i32,
    pub bytes_written: usize,
    pub unified_diff: String,
    pub description: Option<String>,
    pub created_at_ms: u64,
}

// ---------------------------------------------------------------------------
// SavingsTracker — tracks estimated token savings from m1nd usage
// ---------------------------------------------------------------------------

/// Tracks estimated token savings from using m1nd instead of grep/Read.
pub struct SavingsTracker {
    pub queries_by_tool: HashMap<String, u64>,
    pub tokens_saved: u64,
    pub file_reads_avoided: u64,
    pub lines_avoided: u64,
}

impl Default for SavingsTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl SavingsTracker {
    pub fn new() -> Self {
        Self {
            queries_by_tool: HashMap::new(),
            tokens_saved: 0,
            file_reads_avoided: 0,
            lines_avoided: 0,
        }
    }

    /// Call after every successful tool dispatch.
    pub fn record(&mut self, tool: &str, _result_nodes: usize) {
        *self.queries_by_tool.entry(tool.to_string()).or_insert(0) += 1;
        let (tokens, files, lines) = match tool {
            "m1nd_activate" | "m1nd_seek" | "m1nd_search" => (750, 5, 500),
            "m1nd_impact" | "m1nd_predict" | "m1nd_counterfactual" => (1000, 8, 800),
            "m1nd_surgical_context" => (3200, 8, 300),
            "m1nd_surgical_context_v2" => (4800, 12, 400),
            "m1nd_hypothesize" | "m1nd_missing" => (1000, 5, 200),
            "m1nd_apply" | "m1nd_apply_batch" => (900, 3, 200),
            "m1nd_scan" => (1000, 4, 400),
            _ => (500, 2, 200),
        };
        self.tokens_saved += tokens;
        self.file_reads_avoided += files;
        self.lines_avoided += lines;
    }
}

// ---------------------------------------------------------------------------
// QueryLogEntry — ring buffer entry for report/savings
// ---------------------------------------------------------------------------

/// A log entry for each tool call.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryLogEntry {
    pub tool: String,
    pub agent_id: String,
    pub timestamp_ms: u64,
    pub elapsed_ms: f64,
    pub result_count: usize,
    pub query_preview: String,
}

/// Global savings state, persisted to disk.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GlobalSavingsState {
    pub total_sessions: u64,
    pub total_queries: u64,
    pub total_tokens_saved: u64,
    pub total_file_reads_avoided: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BootMemoryState {
    pub entries: HashMap<String, BootMemoryEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BootMemoryEntry {
    pub key: String,
    pub value: Value,
    pub tags: Vec<String>,
    pub source_refs: Vec<String>,
    pub updated_at_ms: u64,
    pub updated_by_agent: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileInventoryEntry {
    pub external_id: String,
    pub file_path: String,
    pub size_bytes: u64,
    pub last_modified_ms: u64,
    pub language: String,
    pub commit_count: u32,
    pub loc: Option<u32>,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CoverageSessionState {
    pub started_at_ms: u64,
    pub visited_files: BTreeSet<String>,
    pub visited_nodes: BTreeSet<String>,
    pub tools_used: HashMap<String, u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DaemonRuntimeState {
    pub active: bool,
    pub started_at_ms: Option<u64>,
    pub last_tick_ms: Option<u64>,
    pub last_tick_trigger: Option<String>,
    pub watch_paths: Vec<String>,
    pub poll_interval_ms: u64,
    pub coalesce_window_ms: u64,
    pub pending_rerun: bool,
    pub tick_in_flight: bool,
    pub last_coalesced_event_ms: Option<u64>,
    pub coalesced_event_count: u64,
    pub tracked_files: HashMap<String, DaemonTrackedFile>,
    pub tick_count: u64,
    pub last_tick_duration_ms: Option<f64>,
    pub last_tick_changed_files: usize,
    pub last_tick_deleted_files: usize,
    pub last_tick_alerts_emitted: usize,
    pub idle_streak: u32,
    pub max_backoff_multiplier: u32,
    pub watch_backend: String,
    pub watch_backend_error: Option<String>,
    pub watch_events_seen: u64,
    pub watch_events_dropped: u64,
    pub last_watch_event_ms: Option<u64>,
    pub git_root: Option<String>,
    pub git_baseline_ref: Option<String>,
    pub git_baseline_kind: Option<String>,
    pub git_since_ref: Option<String>,
    pub git_head_ref: Option<String>,
    pub last_git_scan_ms: Option<u64>,
    pub last_git_changed_files: usize,
    pub git_backend_error: Option<String>,
    pub git_operation_in_progress: bool,
    pub git_operation_kind: Option<String>,
    pub deferred_ticks: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DaemonTrackedFile {
    pub external_id: String,
    pub file_path: String,
    pub last_modified_ms: u64,
    pub size_bytes: u64,
    pub sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DaemonAlert {
    pub alert_id: String,
    pub severity: String,
    pub kind: String,
    pub message: String,
    pub confidence: f32,
    pub evidence: Vec<String>,
    pub suggested_tool: Option<String>,
    pub suggested_target: Option<String>,
    pub file_path: Option<String>,
    pub node_id: Option<String>,
    pub created_at_ms: u64,
    pub acked: bool,
    pub acked_at_ms: Option<u64>,
}

pub type ApplyBatchProgressSink =
    Arc<dyn Fn(&crate::protocol::surgical::ApplyBatchProgressEvent) + Send + Sync>;

// ---------------------------------------------------------------------------
// SessionState — all server state in one place
// Replaces: 03-MCP Section 1.1 server internal state
// ---------------------------------------------------------------------------

/// Server session state. Owns the graph and all engine instances.
/// Single instance shared across all agent connections.
pub struct SessionState {
    /// Shared graph with RwLock for concurrent read access.
    pub graph: SharedGraph,
    /// Domain configuration (code, music, generic, etc.)
    pub domain: DomainConfig,
    /// Query orchestrator (owns HybridEngine, XLR, Semantic, etc.)
    pub orchestrator: QueryOrchestrator,
    /// Temporal engine (co-change, causal chains, decay, velocity, impact).
    pub temporal: TemporalEngine,
    /// Counterfactual engine.
    pub counterfactual: CounterfactualEngine,
    /// Topology analyzer.
    pub topology: TopologyAnalyzer,
    /// Resonance engine.
    pub resonance: ResonanceEngine,
    /// Plasticity engine.
    pub plasticity: PlasticityEngine,
    /// Query counter for auto-persist.
    pub queries_processed: u64,
    /// Auto-persist interval (persist every N queries).
    pub auto_persist_interval: u32,
    /// Server start time.
    pub start_time: Instant,
    /// Last persistence timestamp.
    pub last_persist_time: Option<Instant>,
    /// Path to graph snapshot file.
    pub graph_path: PathBuf,
    /// Path to plasticity state file.
    pub plasticity_path: PathBuf,
    /// Per-agent session tracking.
    pub sessions: HashMap<String, AgentSession>,
    /// In-memory preview states for Ultra Edit phase 1.
    pub edit_previews: HashMap<String, EditPreviewState>,

    // --- Perspective MCP state (12-PERSPECTIVE-SYNTHESIS) ---
    /// Generation counter: bumped on ingest, rebuild_engines (Theme 1).
    pub graph_generation: u64,
    /// Generation counter: bumped on learn (Theme 1).
    pub plasticity_generation: u64,
    /// Unified cache generation: max(graph_gen, plasticity_gen). Bumped on ALL mutations (Theme 1).
    pub cache_generation: u64,

    /// Perspective state per (agent_id, perspective_id) (Theme 2).
    pub perspectives: HashMap<(String, String), PerspectiveState>,
    /// Lock state per lock_id (Theme 2).
    pub locks: HashMap<String, LockState>,
    /// Per-agent monotonic counter for perspective IDs (Theme 2).
    pub perspective_counter: HashMap<String, u64>,
    /// Per-agent monotonic counter for lock IDs (Theme 2).
    pub lock_counter: HashMap<String, u64>,

    /// Pending watcher events queue (Theme 10).
    pub pending_watcher_events: Vec<WatcherEvent>,

    /// Hard caps for perspective/lock resources (Theme 5).
    pub perspective_limits: PerspectiveLimits,

    /// Peek security configuration (Theme 6).
    pub peek_security: PeekSecurityConfig,

    /// Ingest root paths for peek allow-list (Theme 6).
    /// Order is preserved oldest -> newest so path resolution can prefer the
    /// most recent matching root deterministically.
    pub ingest_roots: Vec<String>,
    /// Last known project root inferred from ingest or graph location.
    pub workspace_root: Option<String>,
    /// How `workspace_root` was inferred. This is diagnostic-only and helps
    /// agents distinguish real repo roots from Codex runtime session folders.
    pub workspace_root_source: Option<String>,
    /// Dedicated runtime root for persisted sidecar state.
    pub runtime_root: PathBuf,
    /// Registry + lease handle for this process instance.
    pub instance: InstanceHandle,
    /// Optional live sink for apply_batch progress emission.
    pub apply_batch_progress_sink: Option<ApplyBatchProgressSink>,

    // --- Superpowers: Antibody state ---
    /// All stored antibodies.
    pub antibodies: Vec<Antibody>,
    /// Path to antibodies persistence file.
    pub antibodies_path: PathBuf,
    /// Generation at last antibody scan (for "changed" scope).
    pub last_antibody_scan_generation: u64,

    // --- Superpowers: Tremor + Trust state ---
    /// Tremor registry: per-node time series of weight-change observations.
    pub tremor_registry: TremorRegistry,
    /// Path to tremor_state.json persistence file.
    pub tremor_path: PathBuf,
    /// Trust ledger: per-node actuarial defect records.
    pub trust_ledger: TrustLedger,
    /// Path to trust_state.json persistence file.
    pub trust_path: PathBuf,

    // --- v0.4.0: Savings + Query Log ---
    /// Savings tracker (token economy).
    pub savings_tracker: SavingsTracker,
    /// Query log ring buffer (capped at 1000 entries).
    pub query_log: Vec<QueryLogEntry>,
    /// Global savings state (persisted).
    pub global_savings: GlobalSavingsState,
    /// Path to savings_state.json persistence file.
    pub savings_path: PathBuf,
    /// Graph node count at session start.
    pub session_start_node_count: u32,
    /// Graph edge count at session start.
    pub session_start_edge_count: u64,
    /// Path to canonical boot memory persisted next to the graph.
    pub boot_memory_path: PathBuf,
    /// Hot runtime cache of canonical boot memory entries.
    pub boot_memory: HashMap<String, BootMemoryEntry>,
    /// Path to daemon state persisted next to the graph.
    pub daemon_state_path: PathBuf,
    /// Current persisted daemon runtime state.
    pub daemon_state: DaemonRuntimeState,
    /// Path to persisted daemon/proactive alerts.
    pub daemon_alerts_path: PathBuf,
    /// Persisted daemon/proactive alerts.
    pub daemon_alerts: Vec<DaemonAlert>,
    /// Lightweight metadata index for files seen during ingest or verification.
    pub file_inventory: HashMap<String, FileInventoryEntry>,
    /// Per-agent exploration coverage state for visited files/nodes.
    pub coverage_sessions: HashMap<String, CoverageSessionState>,
    /// Local document auto-ingest runtime.
    pub auto_ingest: AutoIngestState,
    /// Universal document artifact/cache index.
    pub document_cache: DocumentCacheState,
}

const WORKSPACE_ROOT_ENV_CANDIDATES: &[&str] = &[
    // Host-neutral contract. Any MCP host can set one of these.
    "M1ND_WORKSPACE_ROOT",
    "M1ND_PROJECT_ROOT",
    "M1ND_REPO_ROOT",
    "WORKSPACE_ROOT",
    "PROJECT_ROOT",
    "REPO_ROOT",
    // Known agent/editor host hints. These are opportunistic aliases; the
    // host-neutral M1ND_* variables above remain the preferred contract.
    "CLAUDE_PROJECT_DIR",
    "CLAUDE_WORKSPACE_ROOT",
    "ANTHROPIC_WORKSPACE_ROOT",
    "ANTIGRAVITY_WORKSPACE_ROOT",
    "ANTIGRAVITY_PROJECT_ROOT",
    "GEMINI_WORKSPACE_ROOT",
    "GEMINI_PROJECT_ROOT",
    "CURSOR_WORKSPACE_ROOT",
    "CURSOR_PROJECT_ROOT",
    "WINDSURF_WORKSPACE_ROOT",
    "WINDSURF_PROJECT_ROOT",
    "VSCODE_WORKSPACE",
    "VSCODE_CWD",
    // Package-manager/shell fallbacks. These are intentionally later because
    // shells can point at transient directories in some hosted runtimes.
    "INIT_CWD",
    "PWD",
    "OLDPWD",
];

const MANAGED_RUNTIME_PATH_MARKERS: &[&str] = &[
    "/.codex/m1nd-runtimes/",
    "\\.codex\\m1nd-runtimes\\",
    "/.claude/m1nd-runtimes/",
    "\\.claude\\m1nd-runtimes\\",
    "/.antigravity/m1nd-runtimes/",
    "\\.antigravity\\m1nd-runtimes\\",
    "/.gemini/m1nd-runtimes/",
    "\\.gemini\\m1nd-runtimes\\",
    "/.cursor/m1nd-runtimes/",
    "\\.cursor\\m1nd-runtimes\\",
    "/.windsurf/m1nd-runtimes/",
    "\\.windsurf\\m1nd-runtimes\\",
    "/.m1nd-runtimes/",
    "\\.m1nd-runtimes\\",
    "/m1nd-runtimes/",
    "\\m1nd-runtimes\\",
    "/mcp-runtimes/",
    "\\mcp-runtimes\\",
    "/agent-runtimes/",
    "\\agent-runtimes\\",
    "/sessions/ppid-",
    "\\sessions\\ppid-",
];

impl SessionState {
    pub fn binding_fingerprint(&self) -> serde_json::Value {
        let graph = self.graph.read();
        serde_json::json!({
            "schema": "m1nd-binding-fingerprint-v0",
            "process_id": std::process::id(),
            "current_exe": std::env::current_exe().ok().map(|path| path.to_string_lossy().to_string()),
            "runtime_root": self.runtime_root.to_string_lossy(),
            "graph_path": self.graph_path.to_string_lossy(),
            "plasticity_path": self.plasticity_path.to_string_lossy(),
            "workspace_root": self.workspace_root,
            "workspace_root_source": self.workspace_root_source,
            "ingest_roots": self.ingest_roots,
            "graph_path_exists": self.graph_path.exists(),
            "graph_generation": self.graph_generation,
            "plasticity_generation": self.plasticity_generation,
            "cache_generation": self.cache_generation,
            "node_count": graph.num_nodes() as u64,
            "edge_count": graph.num_edges() as u64,
            "graph_finalized": graph.finalized,
        })
    }

    pub fn graph_runtime_summary(&self) -> serde_json::Value {
        let graph = self.graph.read();
        serde_json::json!({
            "node_count": graph.num_nodes(),
            "edge_count": graph.num_edges(),
            "finalized": graph.finalized,
            "graph_generation": self.graph_generation,
            "plasticity_generation": self.plasticity_generation,
            "cache_generation": self.cache_generation,
            "ingest_root_count": self.ingest_roots.len(),
            "ingest_roots": self.ingest_roots,
            "workspace_root": self.workspace_root,
            "workspace_root_source": self.workspace_root_source,
            "runtime_root": self.runtime_root,
            "graph_path": self.graph_path,
            "graph_path_exists": self.graph_path.exists(),
        })
    }

    pub fn mini_graph_state(&self) -> serde_json::Value {
        let graph = self.graph.read();
        serde_json::json!({
            "node_count": graph.num_nodes(),
            "edge_count": graph.num_edges(),
            "finalized": graph.finalized,
            "graph_generation": self.graph_generation,
            "ingest_root_count": self.ingest_roots.len(),
            "workspace_root_known": self.workspace_root.is_some(),
            "workspace_root": self.workspace_root,
            "workspace_root_source": self.workspace_root_source,
            "graph_path_exists": self.graph_path.exists(),
            "runtime_root": self.runtime_root.to_string_lossy(),
        })
    }

    pub fn workspace_binding_mismatch(&self, scope: Option<&str>) -> Option<serde_json::Value> {
        let scope_path = Self::absolute_scope_path(scope?)?;
        let mut known_roots: Vec<(&str, PathBuf)> = Vec::new();
        if let Some(workspace_root) = self.workspace_root.as_deref() {
            known_roots.push(("workspace_root", PathBuf::from(workspace_root)));
        }
        for root in &self.ingest_roots {
            known_roots.push(("ingest_root", PathBuf::from(root)));
        }

        if known_roots
            .iter()
            .any(|(_, root)| Self::path_starts_with_loosely(&scope_path, root))
        {
            return None;
        }

        let requested_workspace_hint = Self::scope_workspace_hint(&scope_path);
        let requested_context_id = requested_workspace_hint
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("requested-workspace")
            .to_string();
        let known_root_values = known_roots
            .iter()
            .map(|(kind, root)| {
                serde_json::json!({
                    "kind": kind,
                    "path": root.to_string_lossy(),
                })
            })
            .collect::<Vec<_>>();

        Some(serde_json::json!({
            "schema": "m1nd-workspace-binding-mismatch-v0",
            "code": "wrong_workspace_binding",
            "requested_scope": scope.unwrap_or_default(),
            "requested_scope_path": scope_path.to_string_lossy(),
            "requested_workspace_hint": requested_workspace_hint.to_string_lossy(),
            "requested_context_id": requested_context_id,
            "active_workspace_root": self.workspace_root,
            "active_workspace_root_source": self.workspace_root_source,
            "active_ingest_roots": self.ingest_roots,
            "known_roots_checked": known_root_values,
            "runtime_root": self.runtime_root.to_string_lossy(),
            "message": "The requested absolute scope is outside the active m1nd workspace and ingest roots.",
            "suggested_fix": {
                "preferred": "start or rebind the MCP host with M1ND_WORKSPACE_ROOT set to requested_workspace_hint",
                "env": {
                    "M1ND_WORKSPACE_ROOT": requested_workspace_hint.to_string_lossy(),
                },
                "same_binding_alternative": "call ingest on requested_workspace_hint only if this session should intentionally switch or merge context",
                "cross_repo_alternative": "use federate_auto or federate when the task genuinely needs multiple repositories in one graph",
            },
            "non_claims": [
                "Context Guard does not switch workspace automatically.",
                "Context Guard does not ingest, federate, or mutate the active graph.",
                "Context Guard does not prove the requested workspace is the correct task target."
            ],
        }))
    }

    fn absolute_scope_path(scope: &str) -> Option<std::path::PathBuf> {
        let scope = scope.trim();
        if scope.is_empty() {
            return None;
        }
        let scope = scope.strip_prefix("file::").unwrap_or(scope);
        let candidate = std::path::PathBuf::from(scope);
        if candidate.is_absolute() {
            Some(candidate)
        } else {
            None
        }
    }

    fn path_starts_with_loosely(path: &std::path::Path, root: &std::path::Path) -> bool {
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

        let path_text = Self::normalized_path_for_compare(path);
        let root_text = Self::normalized_path_for_compare(root);
        if path_text == root_text {
            return true;
        }
        path_text.starts_with(&format!("{root_text}/"))
    }

    fn normalized_path_for_compare(path: &std::path::Path) -> String {
        path.to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_string()
    }

    fn scope_workspace_hint(scope_path: &std::path::Path) -> std::path::PathBuf {
        let start = if scope_path.is_file() {
            scope_path.parent().unwrap_or(scope_path)
        } else {
            scope_path
        };
        for ancestor in start.ancestors() {
            if ancestor.join(".git").exists()
                || ancestor.join("package.json").exists()
                || ancestor.join("Cargo.toml").exists()
                || ancestor.join("pyproject.toml").exists()
            {
                return ancestor.to_path_buf();
            }
        }
        start.to_path_buf()
    }

    pub fn doctor_recovery_payload(
        &self,
        agent_id: &str,
        observed_tool: &str,
        observed_proof_state: &str,
        observed_candidates: Option<u64>,
        scope: Option<&str>,
        error_text: Option<&str>,
    ) -> serde_json::Value {
        let mut arguments = serde_json::json!({
            "agent_id": agent_id,
            "observed_tool": observed_tool,
            "observed_proof_state": observed_proof_state,
        });
        if let Some(candidates) = observed_candidates {
            arguments["observed_candidates"] = serde_json::json!(candidates);
        }
        if let Some(scope) = scope.filter(|value| !value.trim().is_empty()) {
            arguments["scope"] = serde_json::json!(scope);
        }
        if let Some(error_text) = error_text.filter(|value| !value.trim().is_empty()) {
            arguments["error_text"] = serde_json::json!(error_text);
        }
        let workspace_binding_mismatch = self.workspace_binding_mismatch(scope);
        if let Some(mismatch) = workspace_binding_mismatch.clone() {
            arguments["workspace_binding_mismatch"] = mismatch;
        }

        let reason = if workspace_binding_mismatch.is_some() {
            "wrong workspace binding detected; doctor can confirm the active runtime root, workspace root, ingest roots, and requested absolute scope"
        } else {
            "retrieval returned blocked or zero actionable candidates; doctor can distinguish empty graph, stale binding, scope filtering, and session drift"
        };

        let mut payload = serde_json::json!({
            "suggested_tool": "doctor",
            "reason": reason,
            "arguments": arguments,
        });
        if let Some(mismatch) = workspace_binding_mismatch {
            payload["binding_issue"] = serde_json::json!("wrong_workspace_binding");
            payload["workspace_binding_mismatch"] = mismatch;
        }
        payload
    }

    pub fn recovery_playbook_payload(
        &self,
        agent_id: &str,
        observed_tool: &str,
        observed_proof_state: &str,
        observed_candidates: Option<u64>,
        scope: Option<&str>,
        error_text: Option<&str>,
    ) -> serde_json::Value {
        let mut arguments = serde_json::json!({
            "agent_id": agent_id,
            "observed_tool": observed_tool,
            "observed_proof_state": observed_proof_state,
        });
        if let Some(candidates) = observed_candidates {
            arguments["observed_candidates"] = serde_json::json!(candidates);
        }
        if let Some(scope) = scope.filter(|value| !value.trim().is_empty()) {
            arguments["scope"] = serde_json::json!(scope);
        }
        if let Some(error_text) = error_text.filter(|value| !value.trim().is_empty()) {
            arguments["error_text"] = serde_json::json!(error_text);
        }
        let workspace_binding_mismatch = self.workspace_binding_mismatch(scope);
        if let Some(mismatch) = workspace_binding_mismatch.clone() {
            arguments["workspace_binding_mismatch"] = mismatch;
        }

        let reason = if workspace_binding_mismatch.is_some() {
            "wrong workspace binding detected; recovery_playbook returns the ordered context selection path before shell fallback"
        } else {
            "retrieval returned blocked or zero actionable candidates; recovery_playbook returns the ordered agent recovery path before deeper diagnosis"
        };

        let mut payload = serde_json::json!({
            "suggested_tool": "recovery_playbook",
            "reason": reason,
            "arguments": arguments,
            "fallback_tool": "doctor",
        });
        if let Some(mismatch) = workspace_binding_mismatch {
            payload["binding_issue"] = serde_json::json!("wrong_workspace_binding");
            payload["workspace_binding_mismatch"] = mismatch;
        }
        payload
    }

    pub fn retrieval_failure_context(
        &self,
        agent_id: &str,
        observed_tool: &str,
        observed_proof_state: &str,
        observed_candidates: Option<u64>,
        scope: Option<&str>,
        error_text: Option<&str>,
    ) -> (Option<serde_json::Value>, Option<serde_json::Value>) {
        let needs_recovery = observed_proof_state == "blocked"
            || observed_candidates == Some(0)
            || self.workspace_binding_mismatch(scope).is_some();
        if !needs_recovery {
            return (None, None);
        }

        (
            Some(self.mini_graph_state()),
            Some(self.recovery_playbook_payload(
                agent_id,
                observed_tool,
                observed_proof_state,
                observed_candidates,
                scope,
                error_text,
            )),
        )
    }

    pub fn agent_runtime_contract(
        &self,
        agent_id: &str,
        observed_tool: &str,
        observed_proof_state: &str,
        observed_candidates: Option<u64>,
        scope: Option<&str>,
        error_text: Option<&str>,
    ) -> serde_json::Value {
        let workspace_binding_mismatch = self.workspace_binding_mismatch(scope);
        let graph = self.graph.read();
        let node_count = graph.num_nodes() as u64;
        let edge_count = graph.num_edges() as u64;
        let graph_finalized = graph.finalized;
        drop(graph);

        let graph_populated = node_count > 0;
        let observed_zero_candidates = observed_candidates == Some(0);
        let observed_blocked = observed_proof_state == "blocked";
        let needs_recovery = workspace_binding_mismatch.is_some()
            || !graph_populated
            || observed_blocked
            || observed_zero_candidates;
        let trust_mode = if workspace_binding_mismatch.is_some() {
            "wrong_workspace_binding"
        } else if !graph_populated {
            "needs_ingest"
        } else if observed_blocked || observed_zero_candidates {
            "retrieval_needs_recovery"
        } else {
            "full_trust"
        };
        let status = match trust_mode {
            "full_trust" => "ok",
            "retrieval_needs_recovery" => "triaging",
            _ => "blocked",
        };
        let recovery = if needs_recovery {
            Some(self.recovery_playbook_payload(
                agent_id,
                observed_tool,
                observed_proof_state,
                observed_candidates,
                scope,
                error_text,
            ))
        } else {
            None
        };
        let workspace_match = workspace_binding_mismatch.is_none();

        serde_json::json!({
            "schema": "m1nd-agent-runtime-contract-v0",
            "status": status,
            "proof_state": observed_proof_state,
            "trust_mode": trust_mode,
            "observed": {
                "tool": observed_tool,
                "candidates": observed_candidates,
                "error_text": error_text,
            },
            "session_identity": {
                "agent_id": agent_id,
                "tool": observed_tool,
                "process_id": std::process::id(),
                "binary": {
                    "name": "m1nd-mcp",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "current_exe": std::env::current_exe().ok().map(|path| path.to_string_lossy().to_string()),
                "runtime_root": self.runtime_root.to_string_lossy(),
            },
            "workspace_binding": {
                "requested_scope": scope,
                "active_workspace_root": self.workspace_root,
                "active_workspace_root_source": self.workspace_root_source,
                "active_ingest_roots": self.ingest_roots,
                "workspace_match": workspace_match,
                "mismatch": workspace_binding_mismatch,
            },
            "graph_identity": {
                "node_count": node_count,
                "edge_count": edge_count,
                "finalized": graph_finalized,
                "graph_generation": self.graph_generation,
                "plasticity_generation": self.plasticity_generation,
                "cache_generation": self.cache_generation,
                "ingest_root_count": self.ingest_roots.len(),
                "graph_path": self.graph_path.to_string_lossy(),
                "graph_path_exists": self.graph_path.exists(),
            },
            "next_suggested_tool": if needs_recovery { serde_json::Value::String("recovery_playbook".into()) } else { serde_json::Value::Null },
            "next_step_hint": if needs_recovery {
                serde_json::Value::String("Call recovery_playbook with the provided recovery.arguments payload before falling back to shell search.".into())
            } else {
                serde_json::Value::Null
            },
            "recovery": recovery.unwrap_or(serde_json::Value::Null),
            "non_claims": [
                "agent_runtime_contract does not repair the MCP host binding.",
                "agent_runtime_contract does not ingest or mutate the graph.",
                "agent_runtime_contract does not prove semantic retrieval correctness.",
                "agent_runtime_contract does not replace compiler, test, log, or direct file truth."
            ],
        })
    }

    pub fn instance_self_summary(&self) -> serde_json::Value {
        let instance: InstanceRegistryEntry = self.instance.summary();
        serde_json::json!({
            "instance": instance,
            "graph_state": self.graph_runtime_summary(),
            "active_agent_sessions": self.sessions.len(),
            "queries_processed": self.queries_processed,
            "last_persist_secs_ago": self.last_persist_time.map(|ts| ts.elapsed().as_secs_f64()),
        })
    }

    pub fn empty_graph_diagnostic(
        &self,
        tool: &str,
        scope: Option<&str>,
        hint: Option<&str>,
    ) -> serde_json::Value {
        let mut next_actions = vec![
            "run ingest against the intended repository or workspace".to_string(),
            "confirm the tool is querying the same active graph session used by the latest ingest"
                .to_string(),
        ];
        if scope.is_some() {
            next_actions.push(
                "retry with both absolute and graph-relative scope forms to detect normalization drift"
                    .to_string(),
            );
        }

        serde_json::json!({
            "error": {
                "code": "empty_graph",
                "message": format!("{} cannot operate because the active graph has zero nodes", tool),
                "tool": tool,
                "scope": scope,
                "hint": hint,
                "probable_causes": [
                    "the latest ingest did not populate the active graph",
                    "the handler is reading a different graph/session state than the latest ingest",
                    "scope or path normalization excluded the intended graph region"
                ],
                "next_actions": next_actions,
            },
            "graph_state": self.graph_runtime_summary(),
        })
    }

    fn infer_workspace_root(
        config: &crate::server::McpConfig,
        runtime_root: &std::path::Path,
    ) -> (std::path::PathBuf, String) {
        let current_dir = std::env::current_dir().ok();
        Self::infer_workspace_root_with_current_dir(config, runtime_root, current_dir.as_deref())
    }

    fn infer_workspace_root_with_current_dir(
        config: &crate::server::McpConfig,
        runtime_root: &std::path::Path,
        current_dir: Option<&std::path::Path>,
    ) -> (std::path::PathBuf, String) {
        let raw_graph_parent = config
            .graph_source
            .parent()
            .unwrap_or(runtime_root)
            .to_path_buf();
        let graph_parent = if raw_graph_parent.is_absolute() {
            raw_graph_parent
        } else if let Some(current_dir) = current_dir {
            current_dir.join(&raw_graph_parent)
        } else {
            runtime_root.join(&raw_graph_parent)
        };

        if !Self::looks_like_managed_runtime_path(&graph_parent, runtime_root) {
            return (graph_parent, "graph_path_parent".into());
        }

        for env_name in WORKSPACE_ROOT_ENV_CANDIDATES {
            let Ok(value) = std::env::var(env_name) else {
                continue;
            };
            let candidate = std::path::PathBuf::from(value);
            if Self::usable_workspace_candidate(&candidate, runtime_root) {
                return (candidate, format!("env:{env_name}"));
            }
        }

        if let Some(candidate) = current_dir {
            if Self::usable_workspace_candidate(candidate, runtime_root) {
                return (candidate.to_path_buf(), "current_dir".into());
            }
        }

        (graph_parent, "graph_path_parent_runtime_fallback".into())
    }

    fn usable_workspace_candidate(
        candidate: &std::path::Path,
        runtime_root: &std::path::Path,
    ) -> bool {
        candidate.is_dir() && !Self::looks_like_managed_runtime_path(candidate, runtime_root)
    }

    fn looks_like_managed_runtime_path(
        path: &std::path::Path,
        runtime_root: &std::path::Path,
    ) -> bool {
        if Self::path_matches_runtime_base(runtime_root) && path.starts_with(runtime_root) {
            return true;
        }
        Self::path_matches_runtime_base(path)
    }

    fn path_matches_runtime_base(path: &std::path::Path) -> bool {
        if let Ok(runtime_base) = std::env::var("M1ND_RUNTIME_BASE") {
            let runtime_base = std::path::PathBuf::from(runtime_base);
            if path.starts_with(runtime_base) {
                return true;
            }
        }
        let text = path.to_string_lossy();
        MANAGED_RUNTIME_PATH_MARKERS
            .iter()
            .any(|marker| text.contains(marker))
    }

    /// Initialize from a loaded graph. Builds all engines.
    /// Replaces: 03-MCP Section 1.2 startup sequence steps 3-6.
    pub fn initialize(
        graph: Graph,
        config: &crate::server::McpConfig,
        domain: DomainConfig,
    ) -> M1ndResult<Self> {
        // Build all engines from graph
        let orchestrator = QueryOrchestrator::build(&graph)?;
        let temporal = TemporalEngine::build(&graph)?;
        let counterfactual = CounterfactualEngine::with_defaults();
        let topology = TopologyAnalyzer::with_defaults();
        let resonance = ResonanceEngine::with_defaults();
        let plasticity =
            PlasticityEngine::new(&graph, m1nd_core::plasticity::PlasticityConfig::default());

        let shared = Arc::new(parking_lot::RwLock::new(graph));

        let runtime_root = config.runtime_dir.clone().unwrap_or_else(|| {
            config
                .graph_source
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf()
        });
        std::fs::create_dir_all(&runtime_root)?;
        let (workspace_root, workspace_root_source) =
            Self::infer_workspace_root(config, &runtime_root);
        let instance = InstanceHandle::acquire(
            &workspace_root,
            &runtime_root,
            &config.graph_source,
            &config.plasticity_state,
            config.registry_dir.as_deref(),
        )?;
        let ingest_roots = Self::load_ingest_roots(&config.graph_source);

        Ok(Self {
            graph: shared,
            domain,
            orchestrator,
            temporal,
            counterfactual,
            topology,
            resonance,
            plasticity,
            queries_processed: 0,
            auto_persist_interval: config.auto_persist_interval,
            start_time: Instant::now(),
            last_persist_time: None,
            graph_path: config.graph_source.clone(),
            plasticity_path: config.plasticity_state.clone(),
            sessions: HashMap::new(),
            edit_previews: HashMap::new(),
            // Perspective MCP state
            graph_generation: 0,
            plasticity_generation: 0,
            cache_generation: 0,
            perspectives: HashMap::new(),
            locks: HashMap::new(),
            perspective_counter: HashMap::new(),
            lock_counter: HashMap::new(),
            pending_watcher_events: Vec::new(),
            perspective_limits: PerspectiveLimits::default(),
            peek_security: PeekSecurityConfig::default(),
            ingest_roots,
            workspace_root: Some(workspace_root.to_string_lossy().to_string()),
            workspace_root_source: Some(workspace_root_source),
            runtime_root: runtime_root.clone(),
            instance,
            apply_batch_progress_sink: None,
            // Superpowers: Antibody state
            antibodies: {
                let ab_path = runtime_root.join("antibodies.json");
                m1nd_core::antibody::load_antibodies(&ab_path).unwrap_or_default()
            },
            antibodies_path: runtime_root.join("antibodies.json"),
            last_antibody_scan_generation: 0,
            // Superpowers: Tremor + Trust state
            tremor_registry: {
                let tr_path = runtime_root.join("tremor_state.json");
                m1nd_core::tremor::load_tremor_state(&tr_path)
                    .unwrap_or_else(|_| TremorRegistry::with_defaults())
            },
            tremor_path: runtime_root.join("tremor_state.json"),
            trust_ledger: {
                let tl_path = runtime_root.join("trust_state.json");
                m1nd_core::trust::load_trust_state(&tl_path).unwrap_or_else(|_| TrustLedger::new())
            },
            trust_path: runtime_root.join("trust_state.json"),
            // v0.4.0: Savings + Query Log
            savings_tracker: SavingsTracker::new(),
            query_log: Vec::new(),
            global_savings: {
                let sv_path = runtime_root.join("savings_state.json");
                std::fs::read_to_string(&sv_path)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default()
            },
            savings_path: runtime_root.join("savings_state.json"),
            session_start_node_count: 0,
            session_start_edge_count: 0,
            boot_memory_path: runtime_root.join("boot_memory_state.json"),
            boot_memory: {
                let boot_path = runtime_root.join("boot_memory_state.json");
                Self::load_boot_memory(&boot_path)
            },
            daemon_state_path: runtime_root.join("daemon_state.json"),
            daemon_state: {
                let path = runtime_root.join("daemon_state.json");
                Self::load_daemon_state(&path)
            },
            daemon_alerts_path: runtime_root.join("daemon_alerts.json"),
            daemon_alerts: {
                let path = runtime_root.join("daemon_alerts.json");
                Self::load_daemon_alerts(&path)
            },
            file_inventory: HashMap::new(),
            coverage_sessions: HashMap::new(),
            auto_ingest: AutoIngestState::load(&runtime_root),
            document_cache: load_document_cache(&runtime_root),
        })
    }

    /// Check if auto-persist should trigger. Returns true every N queries.
    pub fn should_persist(&self) -> bool {
        self.queries_processed > 0
            && self
                .queries_processed
                .is_multiple_of(self.auto_persist_interval as u64)
    }

    /// Persist all state to disk.
    ///
    /// Ordering: graph first (source of truth), then plasticity.
    /// If graph save fails, skip plasticity to avoid inconsistent state.
    /// If plasticity save fails after graph succeeds, log warning but don't crash.
    pub fn persist(&mut self) -> M1ndResult<()> {
        let _ = self.instance.mark_heartbeat();
        self.persist_ingest_roots();
        let graph = self.graph.read();

        // Graph is the source of truth — save it first.
        m1nd_core::snapshot::save_graph(&graph, &self.graph_path)?;

        // Graph succeeded. Now try plasticity — failure here is non-fatal.
        match self.plasticity.export_state(&graph) {
            Ok(states) => {
                if let Err(e) =
                    m1nd_core::snapshot::save_plasticity_state(&states, &self.plasticity_path)
                {
                    eprintln!(
                        "[m1nd] WARNING: graph saved but plasticity persist failed: {}",
                        e
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "[m1nd] WARNING: graph saved but plasticity export failed: {}",
                    e
                );
            }
        }

        // Antibodies — failure here is non-fatal.
        if !self.antibodies.is_empty() {
            if let Err(e) =
                m1nd_core::antibody::save_antibodies(&self.antibodies, &self.antibodies_path)
            {
                eprintln!("[m1nd] WARNING: antibody persist failed: {}", e);
            }
        }

        if let Err(e) = m1nd_core::trust::save_trust_state(&self.trust_ledger, &self.trust_path) {
            eprintln!("[m1nd] WARNING: trust persist failed: {}", e);
        }

        if let Err(e) =
            m1nd_core::tremor::save_tremor_state(&self.tremor_registry, &self.tremor_path)
        {
            eprintln!("[m1nd] WARNING: tremor persist failed: {}", e);
        }

        if let Err(e) = self.persist_boot_memory() {
            eprintln!("[m1nd] WARNING: boot memory persist failed: {}", e);
        }
        if let Err(e) = self.persist_daemon_state() {
            eprintln!("[m1nd] WARNING: daemon state persist failed: {}", e);
        }
        if let Err(e) = self.persist_daemon_alerts() {
            eprintln!("[m1nd] WARNING: daemon alert persist failed: {}", e);
        }
        if let Err(e) = self.auto_ingest.persist(&self.runtime_root) {
            eprintln!("[m1nd] WARNING: auto-ingest persist failed: {}", e);
        }
        if let Err(e) = persist_document_cache(&self.runtime_root, &self.document_cache) {
            eprintln!("[m1nd] WARNING: document cache persist failed: {}", e);
        }

        self.last_persist_time = Some(Instant::now());
        Ok(())
    }

    fn persist_ingest_roots(&mut self) {
        let workspace_root = self
            .workspace_root
            .clone()
            .or_else(|| Some(self.runtime_root.to_string_lossy().to_string()));
        let Some(root) = workspace_root else {
            return;
        };

        let root_path = std::path::Path::new(&root);
        let persist_root = if root_path.is_dir() {
            root_path.to_path_buf()
        } else {
            self.runtime_root.clone()
        };
        let ingest_roots_path = persist_root.join("ingest_roots.json");
        if let Ok(json) = serde_json::to_string_pretty(&self.ingest_roots) {
            if let Err(e) = std::fs::write(&ingest_roots_path, json) {
                eprintln!("[m1nd] WARNING: ingest roots persist failed: {}", e);
            }
        }
    }

    fn load_ingest_roots(graph_path: &std::path::Path) -> Vec<String> {
        let Some(root) = graph_path.parent() else {
            return Vec::new();
        };
        let ingest_roots_path = root.join("ingest_roots.json");
        std::fs::read_to_string(&ingest_roots_path)
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
            .unwrap_or_default()
    }

    pub fn persist_boot_memory(&self) -> M1ndResult<()> {
        let state = BootMemoryState {
            entries: self.boot_memory.clone(),
        };
        save_json_atomic(&self.boot_memory_path, &state)
    }

    fn load_boot_memory(path: &Path) -> HashMap<String, BootMemoryEntry> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<BootMemoryState>(&s).ok())
            .map(|state| state.entries)
            .unwrap_or_default()
    }

    pub fn persist_daemon_state(&self) -> M1ndResult<()> {
        save_json_atomic(&self.daemon_state_path, &self.daemon_state)
    }

    fn load_daemon_state(path: &Path) -> DaemonRuntimeState {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<DaemonRuntimeState>(&s).ok())
            .unwrap_or_default()
    }

    pub fn persist_daemon_alerts(&self) -> M1ndResult<()> {
        save_json_atomic(&self.daemon_alerts_path, &self.daemon_alerts)
    }

    fn load_daemon_alerts(path: &Path) -> Vec<DaemonAlert> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<DaemonAlert>>(&s).ok())
            .unwrap_or_default()
    }

    pub fn record_daemon_alert(&mut self, alert: DaemonAlert) {
        self.daemon_alerts.push(alert);
        if self.daemon_alerts.len() > 500 {
            let drain = self.daemon_alerts.len() - 500;
            self.daemon_alerts.drain(0..drain);
        }
    }

    pub fn reload_heuristic_sidecars(&mut self) {
        self.antibodies =
            m1nd_core::antibody::load_antibodies(&self.antibodies_path).unwrap_or_default();
        self.tremor_registry = m1nd_core::tremor::load_tremor_state(&self.tremor_path)
            .unwrap_or_else(|_| TremorRegistry::with_defaults());
        self.trust_ledger = m1nd_core::trust::load_trust_state(&self.trust_path)
            .unwrap_or_else(|_| TrustLedger::new());
    }

    /// Rebuild all engines after graph replacement (e.g. after ingest).
    /// Critical: SemanticEngine indexes, TemporalEngine, PlasticityEngine
    /// are all built from graph state and become stale on graph swap.
    ///
    /// Also invalidates all perspective and lock state (Theme 16).
    pub fn rebuild_engines(&mut self) -> M1ndResult<()> {
        // Scope the read lock so it's dropped before &mut self methods
        {
            let graph = self.graph.read();
            self.orchestrator = QueryOrchestrator::build(&graph)?;
            self.temporal = TemporalEngine::build(&graph)?;
            self.plasticity =
                PlasticityEngine::new(&graph, m1nd_core::plasticity::PlasticityConfig::default());
        }

        // Theme 16: invalidate all perspective and lock state after rebuild
        self.invalidate_all_perspectives();
        self.mark_all_lock_baselines_stale();
        self.graph_generation += 1;
        self.cache_generation = self.cache_generation.max(self.graph_generation);

        Ok(())
    }

    // --- Perspective MCP methods (12-PERSPECTIVE-SYNTHESIS) ---

    /// Bump graph generation (Theme 1). Called after ingest and rebuild_engines.
    pub fn bump_graph_generation(&mut self) {
        self.graph_generation += 1;
        self.cache_generation = self.cache_generation.max(self.graph_generation);
    }

    /// Bump plasticity generation (Theme 1). Called after learn.
    pub fn bump_plasticity_generation(&mut self) {
        self.plasticity_generation += 1;
        self.cache_generation = self.cache_generation.max(self.plasticity_generation);
    }

    /// Invalidate all perspectives (Theme 16).
    /// Sets stale=true, clears route caches, bumps route_set_version.
    /// Does NOT close perspectives — agents may still want them.
    pub fn invalidate_all_perspectives(&mut self) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        for state in self.perspectives.values_mut() {
            state.stale = true;
            state.route_cache = None;
            state.route_set_version = now_ms;
        }
    }

    /// Mark all lock baselines as stale (Theme 16).
    /// Does NOT release locks. lock.diff reports staleness and suggests lock.rebase.
    pub fn mark_all_lock_baselines_stale(&mut self) {
        for lock in self.locks.values_mut() {
            lock.baseline_stale = true;
        }
    }

    /// Get a perspective for an agent (Theme 2).
    pub fn get_perspective(
        &self,
        agent_id: &str,
        perspective_id: &str,
    ) -> Option<&PerspectiveState> {
        self.perspectives
            .get(&(agent_id.to_string(), perspective_id.to_string()))
    }

    /// Get a mutable perspective for an agent (Theme 2).
    pub fn get_perspective_mut(
        &mut self,
        agent_id: &str,
        perspective_id: &str,
    ) -> Option<&mut PerspectiveState> {
        self.perspectives
            .get_mut(&(agent_id.to_string(), perspective_id.to_string()))
    }

    /// Generate a new perspective ID for an agent (Theme 2).
    pub fn next_perspective_id(&mut self, agent_id: &str) -> String {
        let counter = self
            .perspective_counter
            .entry(agent_id.to_string())
            .or_insert(0);
        *counter += 1;
        let short_id = &agent_id[..agent_id.len().min(8)];
        format!("persp_{}_{:03}", short_id, counter)
    }

    /// Generate a new lock ID for an agent (Theme 2).
    pub fn next_lock_id(&mut self, agent_id: &str) -> String {
        let counter = self.lock_counter.entry(agent_id.to_string()).or_insert(0);
        *counter += 1;
        let short_id = &agent_id[..agent_id.len().min(8)];
        format!("lock_{}_{:03}", short_id, counter)
    }

    /// Count perspectives for an agent (for limit enforcement, Theme 5).
    pub fn agent_perspective_count(&self, agent_id: &str) -> usize {
        self.perspectives
            .keys()
            .filter(|(a, _)| a == agent_id)
            .count()
    }

    /// Count locks for an agent (for limit enforcement, Theme 5).
    pub fn agent_lock_count(&self, agent_id: &str) -> usize {
        self.locks
            .values()
            .filter(|l| l.agent_id == agent_id)
            .count()
    }

    /// Notify watchers after ingest/learn (Theme 10).
    /// Records (lock_id, trigger, timestamp) in pending_watcher_events.
    /// Diff computed lazily on next lock.diff call.
    pub fn notify_watchers(&mut self, trigger: WatchTrigger) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let matching_locks: Vec<String> = self
            .locks
            .values()
            .filter(|l| {
                l.watcher.as_ref().is_some_and(|w| {
                    matches!(
                        (&trigger, &w.strategy),
                        (
                            WatchTrigger::Ingest,
                            crate::perspective::state::WatchStrategy::OnIngest,
                        ) | (
                            WatchTrigger::Learn,
                            crate::perspective::state::WatchStrategy::OnLearn,
                        )
                    )
                })
            })
            .map(|l| l.lock_id.clone())
            .collect();

        for lock_id in matching_locks {
            self.pending_watcher_events.push(WatcherEvent {
                lock_id,
                trigger: trigger.clone(),
                timestamp_ms: now_ms,
            });
        }
    }

    /// Cleanup all state for an agent (called on session timeout, Theme 2).
    pub fn cleanup_agent_state(&mut self, agent_id: &str) {
        // Remove perspectives
        self.perspectives.retain(|(a, _), _| a != agent_id);
        // Remove locks owned by this agent
        let agent_locks: Vec<String> = self
            .locks
            .values()
            .filter(|l| l.agent_id == agent_id)
            .map(|l| l.lock_id.clone())
            .collect();
        for lock_id in &agent_locks {
            self.locks.remove(lock_id);
        }
        // Clean pending watcher events for removed locks
        self.pending_watcher_events
            .retain(|e| !agent_locks.contains(&e.lock_id));
        // Clean counters
        self.perspective_counter.remove(agent_id);
        self.lock_counter.remove(agent_id);
    }

    /// Estimate memory usage of perspective + lock state (Theme 5).
    /// Used for 50MB budget enforcement.
    pub fn perspective_and_lock_memory_bytes(&self) -> usize {
        // Rough estimate: serialize to JSON and measure
        let persp_size: usize = self
            .perspectives
            .values()
            .map(|p| {
                std::mem::size_of_val(p)
                    + p.navigation_history.len() * 100
                    + p.visited_nodes.len() * 40
            })
            .sum();
        let lock_size: usize = self
            .locks
            .values()
            .map(|l| {
                std::mem::size_of_val(l)
                    + l.baseline.nodes.len() * 40
                    + l.baseline.edges.len() * 120
            })
            .sum();
        persp_size + lock_size
    }

    /// Uptime in seconds.
    pub fn uptime_seconds(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64()
    }

    /// Track an agent session. Creates a new session if first contact,
    /// otherwise updates last_seen and increments query_count.
    pub fn track_agent(&mut self, agent_id: &str) {
        let _ = self.instance.mark_heartbeat();
        let now = Instant::now();
        let session = self
            .sessions
            .entry(agent_id.to_string())
            .or_insert_with(|| AgentSession {
                agent_id: agent_id.to_string(),
                first_seen: now,
                last_seen: now,
                query_count: 0,
            });
        session.last_seen = now;
        session.query_count += 1;
    }

    pub fn next_edit_preview_id(&self, agent_id: &str) -> String {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let short_id = &agent_id[..agent_id.len().min(8)];
        format!("preview_{}_{}", short_id, now_ms)
    }

    /// Log a tool call to the query log ring buffer (max 1000 entries).
    pub fn log_query(
        &mut self,
        tool: &str,
        agent_id: &str,
        elapsed_ms: f64,
        result_count: usize,
        query_preview: &str,
    ) {
        let entry = QueryLogEntry {
            tool: tool.to_string(),
            agent_id: agent_id.to_string(),
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            elapsed_ms,
            result_count,
            query_preview: query_preview.chars().take(100).collect(),
        };
        if self.query_log.len() >= 1000 {
            self.query_log.remove(0);
        }
        self.query_log.push(entry);
    }

    /// Persist global savings state to disk.
    pub fn persist_savings(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.global_savings) {
            let _ = std::fs::write(&self.savings_path, json);
        }
    }

    /// Generate a summary of active agent sessions for health output.
    pub fn session_summary(&self) -> Vec<serde_json::Value> {
        self.sessions
            .values()
            .map(|s| {
                serde_json::json!({
                    "agent_id": s.agent_id,
                    "first_seen_secs_ago": s.first_seen.elapsed().as_secs_f64(),
                    "last_seen_secs_ago": s.last_seen.elapsed().as_secs_f64(),
                    "query_count": s.query_count,
                })
            })
            .collect()
    }

    pub fn record_file_inventory(&mut self, entries: impl IntoIterator<Item = FileInventoryEntry>) {
        for entry in entries {
            self.file_inventory.insert(entry.external_id.clone(), entry);
        }
    }

    pub fn reset_file_inventory(&mut self) {
        self.file_inventory.clear();
    }

    pub fn note_coverage(
        &mut self,
        agent_id: &str,
        tool: &str,
        files: impl IntoIterator<Item = String>,
        nodes: impl IntoIterator<Item = String>,
    ) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let entry = self
            .coverage_sessions
            .entry(agent_id.to_string())
            .or_insert_with(|| CoverageSessionState {
                started_at_ms: now_ms,
                ..CoverageSessionState::default()
            });
        *entry.tools_used.entry(tool.to_string()).or_insert(0) += 1;
        for file in files {
            if !file.is_empty() {
                entry.visited_files.insert(file);
            }
        }
        for node in nodes {
            if !node.is_empty() {
                entry.visited_nodes.insert(node);
            }
        }
    }
}

fn save_json_atomic<T: Serialize>(path: &Path, value: &T) -> M1ndResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    let payload = serde_json::to_vec_pretty(value)?;
    std::fs::write(&tmp, payload)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SessionState, WORKSPACE_ROOT_ENV_CANDIDATES};
    use crate::server::McpConfig;
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::Graph;
    use m1nd_core::types::NodeType;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct EnvGuard {
        saved: Vec<(&'static str, Option<String>)>,
    }

    impl EnvGuard {
        fn clear_workspace_hints() -> Self {
            let saved = WORKSPACE_ROOT_ENV_CANDIDATES
                .iter()
                .map(|name| (*name, std::env::var(name).ok()))
                .collect::<Vec<_>>();
            for name in WORKSPACE_ROOT_ENV_CANDIDATES {
                std::env::remove_var(name);
            }
            Self { saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (name, value) in &self.saved {
                if let Some(value) = value {
                    std::env::set_var(name, value);
                } else {
                    std::env::remove_var(name);
                }
            }
        }
    }

    #[test]
    fn workspace_root_uses_graph_parent_for_normal_graph_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = McpConfig {
            graph_source: temp.path().join("graph_snapshot.json"),
            plasticity_state: temp.path().join("plasticity_state.json"),
            runtime_dir: Some(temp.path().to_path_buf()),
            ..McpConfig::default()
        };

        let state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");

        assert_eq!(
            state.workspace_root.as_deref(),
            Some(temp.path().to_string_lossy().as_ref())
        );
        assert_eq!(
            state.workspace_root_source.as_deref(),
            Some("graph_path_parent")
        );
    }

    #[test]
    fn workspace_root_uses_env_hint_for_codex_runtime_graph_path() {
        let _guard = env_lock().lock().expect("env lock");
        let _env = EnvGuard::clear_workspace_hints();

        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("project");
        let runtime = temp
            .path()
            .join(".codex")
            .join("m1nd-runtimes")
            .join("hash")
            .join("sessions")
            .join("ppid-1-pid-2");
        std::fs::create_dir_all(&workspace).expect("workspace dir");
        std::fs::create_dir_all(&runtime).expect("runtime dir");
        std::env::set_var("M1ND_WORKSPACE_ROOT", &workspace);

        let config = McpConfig {
            graph_source: runtime.join("graph_snapshot.json"),
            plasticity_state: runtime.join("plasticity_state.json"),
            runtime_dir: Some(runtime),
            ..McpConfig::default()
        };

        let state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");

        assert_eq!(
            state.workspace_root.as_deref(),
            Some(workspace.to_string_lossy().as_ref())
        );
        assert_eq!(
            state.workspace_root_source.as_deref(),
            Some("env:M1ND_WORKSPACE_ROOT")
        );
    }

    #[test]
    fn workspace_root_uses_claude_hint_for_managed_runtime_graph_path() {
        let _guard = env_lock().lock().expect("env lock");
        let _env = EnvGuard::clear_workspace_hints();

        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("claude-project");
        let runtime = temp
            .path()
            .join(".claude")
            .join("m1nd-runtimes")
            .join("hash")
            .join("sessions")
            .join("ppid-1-pid-2");
        std::fs::create_dir_all(&workspace).expect("workspace dir");
        std::fs::create_dir_all(&runtime).expect("runtime dir");
        std::env::set_var("CLAUDE_PROJECT_DIR", &workspace);

        let config = McpConfig {
            graph_source: runtime.join("graph_snapshot.json"),
            plasticity_state: runtime.join("plasticity_state.json"),
            runtime_dir: Some(runtime),
            ..McpConfig::default()
        };

        let state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");

        assert_eq!(
            state.workspace_root.as_deref(),
            Some(workspace.to_string_lossy().as_ref())
        );
        assert_eq!(
            state.workspace_root_source.as_deref(),
            Some("env:CLAUDE_PROJECT_DIR")
        );
    }

    #[test]
    fn workspace_root_uses_host_hint_for_relative_graph_inside_managed_runtime() {
        let _guard = env_lock().lock().expect("env lock");
        let _env = EnvGuard::clear_workspace_hints();

        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("claude-project");
        let runtime = temp
            .path()
            .join(".claude")
            .join("m1nd-runtimes")
            .join("hash")
            .join("sessions")
            .join("ppid-1-pid-2");
        std::fs::create_dir_all(&workspace).expect("workspace dir");
        std::fs::create_dir_all(&runtime).expect("runtime dir");
        std::env::set_var("CLAUDE_PROJECT_DIR", &workspace);

        let config = McpConfig {
            graph_source: std::path::PathBuf::from("./graph_snapshot.json"),
            plasticity_state: std::path::PathBuf::from("./plasticity_state.json"),
            runtime_dir: Some(runtime.clone()),
            ..McpConfig::default()
        };

        let (workspace_root, workspace_root_source) =
            SessionState::infer_workspace_root_with_current_dir(&config, &runtime, Some(&runtime));

        assert_eq!(workspace_root, workspace);
        assert_eq!(workspace_root_source.as_str(), "env:CLAUDE_PROJECT_DIR");
    }

    #[test]
    fn workspace_root_prefers_pwd_over_oldpwd_for_managed_runtime_graph_path() {
        let _guard = env_lock().lock().expect("env lock");
        let _env = EnvGuard::clear_workspace_hints();

        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("active-project");
        let stale_workspace = temp.path().join("stale-project");
        let runtime = temp
            .path()
            .join(".codex")
            .join("m1nd-runtimes")
            .join("hash")
            .join("sessions")
            .join("ppid-1-pid-2");
        std::fs::create_dir_all(&workspace).expect("workspace dir");
        std::fs::create_dir_all(&stale_workspace).expect("stale workspace dir");
        std::fs::create_dir_all(&runtime).expect("runtime dir");
        std::env::set_var("PWD", &workspace);
        std::env::set_var("OLDPWD", &stale_workspace);

        let config = McpConfig {
            graph_source: std::path::PathBuf::from("./graph_snapshot.json"),
            plasticity_state: std::path::PathBuf::from("./plasticity_state.json"),
            runtime_dir: Some(runtime.clone()),
            ..McpConfig::default()
        };

        let (workspace_root, workspace_root_source) =
            SessionState::infer_workspace_root_with_current_dir(&config, &runtime, Some(&runtime));

        assert_eq!(workspace_root, workspace);
        assert_eq!(workspace_root_source.as_str(), "env:PWD");
    }

    #[test]
    fn workspace_binding_mismatch_detects_absolute_scope_outside_active_roots() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        let other = temp.path().join("other");
        std::fs::create_dir_all(workspace.join("src")).expect("workspace src");
        std::fs::create_dir_all(other.join("src")).expect("other src");
        std::fs::write(
            workspace.join("Cargo.toml"),
            "[package]\nname='workspace'\n",
        )
        .expect("workspace manifest");
        std::fs::write(other.join("Cargo.toml"), "[package]\nname='other'\n")
            .expect("other manifest");

        let config = McpConfig {
            graph_source: workspace.join("graph_snapshot.json"),
            plasticity_state: workspace.join("plasticity_state.json"),
            runtime_dir: Some(workspace.clone()),
            ..McpConfig::default()
        };
        let state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");

        let other_scope = other.join("src").to_string_lossy().to_string();
        let mismatch = state
            .workspace_binding_mismatch(Some(&other_scope))
            .expect("scope outside workspace should be flagged");

        assert_eq!(mismatch["code"], "wrong_workspace_binding");
        assert_eq!(
            mismatch["requested_workspace_hint"].as_str(),
            Some(other.to_string_lossy().as_ref())
        );
        assert_eq!(
            mismatch["active_workspace_root"].as_str(),
            Some(workspace.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn workspace_binding_mismatch_ignores_absolute_scope_inside_active_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        std::fs::create_dir_all(workspace.join("src")).expect("workspace src");

        let config = McpConfig {
            graph_source: workspace.join("graph_snapshot.json"),
            plasticity_state: workspace.join("plasticity_state.json"),
            runtime_dir: Some(workspace.clone()),
            ..McpConfig::default()
        };
        let state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");

        let workspace_scope = workspace.join("src").to_string_lossy().to_string();
        assert!(state
            .workspace_binding_mismatch(Some(&workspace_scope))
            .is_none());
    }

    #[test]
    fn agent_runtime_contract_surfaces_wrong_workspace_recovery() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        let other = temp.path().join("other");
        std::fs::create_dir_all(workspace.join("src")).expect("workspace src");
        std::fs::create_dir_all(other.join("src")).expect("other src");
        std::fs::write(other.join("Cargo.toml"), "[package]\nname='other'\n")
            .expect("other manifest");

        let mut graph = Graph::new();
        graph
            .add_node("file::src/lib.rs", "lib.rs", NodeType::File, &[], 0.0, 0.0)
            .expect("add file node");
        graph.finalize().expect("finalize graph");
        let config = McpConfig {
            graph_source: workspace.join("graph_snapshot.json"),
            plasticity_state: workspace.join("plasticity_state.json"),
            runtime_dir: Some(workspace.clone()),
            ..McpConfig::default()
        };
        let state = SessionState::initialize(graph, &config, DomainConfig::code())
            .expect("initialize session");

        let other_scope = other.join("src").to_string_lossy().to_string();
        let contract = state.agent_runtime_contract(
            "jimi",
            "seek",
            "blocked",
            Some(0),
            Some(&other_scope),
            None,
        );

        assert_eq!(contract["schema"], "m1nd-agent-runtime-contract-v0");
        assert_eq!(contract["trust_mode"], "wrong_workspace_binding");
        assert_eq!(contract["workspace_binding"]["workspace_match"], false);
        assert_eq!(
            contract["workspace_binding"]["mismatch"]["code"],
            "wrong_workspace_binding"
        );
        assert_eq!(contract["recovery"]["suggested_tool"], "recovery_playbook");
        assert_eq!(
            contract["session_identity"]["binary"]["version"],
            env!("CARGO_PKG_VERSION")
        );
    }

    #[test]
    fn workspace_root_uses_antigravity_hint_for_generic_agent_runtime_graph_path() {
        let _guard = env_lock().lock().expect("env lock");
        let _env = EnvGuard::clear_workspace_hints();

        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("antigravity-project");
        let runtime = temp
            .path()
            .join("agent-runtimes")
            .join("hash")
            .join("sessions")
            .join("ppid-1-pid-2");
        std::fs::create_dir_all(&workspace).expect("workspace dir");
        std::fs::create_dir_all(&runtime).expect("runtime dir");
        std::env::set_var("ANTIGRAVITY_WORKSPACE_ROOT", &workspace);

        let config = McpConfig {
            graph_source: runtime.join("graph_snapshot.json"),
            plasticity_state: runtime.join("plasticity_state.json"),
            runtime_dir: Some(runtime),
            ..McpConfig::default()
        };

        let state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");

        assert_eq!(
            state.workspace_root.as_deref(),
            Some(workspace.to_string_lossy().as_ref())
        );
        assert_eq!(
            state.workspace_root_source.as_deref(),
            Some("env:ANTIGRAVITY_WORKSPACE_ROOT")
        );
    }
}
