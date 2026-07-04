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

struct RecoveryAutoActionContext<'a> {
    agent_id: &'a str,
    observed_tool: &'a str,
    observed_proof_state: &'a str,
    observed_candidates: Option<u64>,
    scope: Option<&'a str>,
    reason: &'a str,
    source_kind: &'a str,
    arguments: &'a Value,
}

// ---------------------------------------------------------------------------
// QueryLogEntry — ring buffer entry for report
// ---------------------------------------------------------------------------
//
// Brand gate G1.5 (founder decision 2026-07-03): SavingsTracker and
// GlobalSavingsState were removed. They tallied unmeasured tokens-saved for the
// killed `savings` tool; `report`'s honest content leans on this query log, not
// on any token estimate.

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

/// A per-agent mark that a concrete edit target reached `proof_state ==
/// "ready_to_edit"` during this session (M1ND_PROOF_GATE). Ephemeral session
/// intent — NOT persisted; it lives only on `SessionState.proof_ready` and dies
/// with the process. Recorded by the surgical prover, consumed by the write gate.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ProofReadyMark {
    /// When the target was proved ready, in unix-epoch milliseconds.
    pub proved_at_ms: u64,
    /// Cache generation captured at proof time (for staleness inspection).
    pub generation: u64,
    /// Tool/evidence that established readiness (e.g. "surgical_context_v2").
    pub evidence: Option<String>,
}

/// A per-agent mark that an agent's scan/audit flagged a finding against a
/// concrete node during this session. Ephemeral session intent — NOT persisted;
/// it lives only on `SessionState.flagged_findings` and dies with the process.
/// Recorded when a scan/audit finding is assembled, consumed at edit/apply time
/// to emit a `proposed_antibody` ProactiveInsight (compounding negative memory).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FindingMark {
    /// When the finding was flagged, in unix-epoch milliseconds.
    pub flagged_at_ms: u64,
    /// Cache generation captured at flag time (for staleness inspection).
    pub generation: u64,
    /// Detector/pattern kind that produced the finding, e.g. "auth_boundary".
    pub kind: String,
    /// Severity bucket: "info" | "warning" | "critical".
    pub severity: String,
    /// File path of the flagged node, for display/template hints (may be empty).
    pub file_path: String,
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
    /// Path to the on-disk embedding cache (OPTIONAL `embed` feature). Derived
    /// from the runtime root; reused across warm boots and re-ingests.
    pub embeddings_cache_path: PathBuf,
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
    /// Per-wire-session caller root (hop-2 `M1nd-Caller-Root`); `None` = unknown
    /// (direct-HTTP / legacy bridge). Request-scoped: the HTTP layer stamps it
    /// from the incoming header before each dispatch, so a later call without the
    /// header does not inherit a stale value. Feeds First-Contact Reception's
    /// mismatch verdict (`reception_verdict`). See TWO-TIER-BRAIN-PRD §9.5.4.
    pub caller_root: Option<String>,
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
    /// OMEGA Move 0: conformal calibration table (per-signal measured τ /
    /// precision-at-coverage). Currently calibrates `predict`/co-change only.
    pub calibration_table: m1nd_core::calibration::CalibrationTable,
    /// Path to calibration_state.json persistence file.
    pub calibration_path: PathBuf,

    // --- v0.4.0: Query Log (savings tracker removed — brand gate G1.5) ---
    /// Query log ring buffer (capped at 1000 entries). Feeds `report`.
    pub query_log: Vec<QueryLogEntry>,
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
    /// Per-agent "proof ready" marks keyed by (agent_id, normalized repo-relative
    /// target). Ephemeral session intent — NOT persisted. Records that an agent
    /// has driven a target to `proof_state == "ready_to_edit"`; checked at edit
    /// time by the M1ND_PROOF_GATE write gate against the normalized edit target.
    pub proof_ready: HashMap<(String, String), ProofReadyMark>,
    /// Per-agent flagged findings keyed by (agent_id, node_id) where node_id is
    /// the node's external id. Ephemeral session intent — NOT persisted. Recorded
    /// when a scan/audit finding is assembled for an agent; consumed at edit/apply
    /// time to emit a `proposed_antibody` ProactiveInsight so the next agent's
    /// audit catches the same structural bug elsewhere (compounding negative
    /// memory). Dies with the process.
    pub flagged_findings: HashMap<(String, String), FindingMark>,
    /// Local document auto-ingest runtime.
    pub auto_ingest: AutoIngestState,
    /// Universal document artifact/cache index.
    pub document_cache: DocumentCacheState,
    /// Result of boot-time agent-memory auto-load, surfaced verbatim in
    /// `session_handshake` (and thus `trust_selftest`). `None` = the auto-load
    /// did not run (no agent-memory dir yet); never hidden.
    pub agent_memory_boot: Option<serde_json::Value>,

    /// Read-only attach mode. When true: `persist()` and every granular
    /// persist helper are no-ops, `should_persist()` is always false, queries
    /// take the immutable read path (`query_readonly`), and mutating tools are
    /// gated off in `dispatch_tool`. The instance holds no exclusive lease.
    pub read_only: bool,
    /// One-shot guard so the "skipping persist" line is logged only once.
    pub read_only_persist_logged: std::cell::Cell<bool>,
}

/// Upper bound on the ephemeral per-agent `flagged_findings` map. Keeps the
/// compounding-negative-memory store from growing without bound across a long
/// session; on overflow the oldest mark is evicted (see [`SessionState::note_finding`]).
const MAX_FLAGGED_FINDINGS: usize = 4096;

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

/// The running binary's semantic version, from Cargo at compile time.
pub const BINARY_VERSION: &str = env!("CARGO_PKG_VERSION");
/// The running binary's git short sha (+`-dirty`), embedded by `build.rs`.
/// `"unknown"` on builds without a `.git` (crates.io / vendored).
pub const BINARY_GIT_SHA: &str = env!("M1ND_GIT_SHA");

/// Parse the `version = "x.y.z"` value from a `Cargo.toml`'s `[package]` table
/// using only std. Returns the first `version = "..."` at zero indentation
/// (package-level, not a dependency's nested version) — good enough for the
/// warn-only self-repo lag heuristic. `None` if no such line is found.
pub(crate) fn parse_cargo_package_version(cargo_toml: &str) -> Option<String> {
    for line in cargo_toml.lines() {
        // Package-level keys sit at column 0; a dependency's inline `version =`
        // is always indented or on a `dep = { version = ... }` line.
        if !line.starts_with("version") {
            continue;
        }
        let rest = line["version".len()..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim();
        // Strip surrounding quotes (single or double).
        let value = rest
            .strip_prefix('"')
            .and_then(|s| s.split('"').next())
            .or_else(|| rest.strip_prefix('\'').and_then(|s| s.split('\'').next()));
        if let Some(value) = value {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// The last path component of a filesystem root — the human name of a repo
/// ("/Users/x/m1nd" → "m1nd"). Trailing slashes are tolerated; a rootless or
/// empty input returns the trimmed input unchanged (honest, never a panic).
/// Shared by the bound-brain display name and the project-brain listing so both
/// name a brain the same way. Mirrors the UI's `repoBasename` exactly.
pub(crate) fn basename_of(root: &str) -> String {
    root.trim()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| root.trim())
        .to_string()
}

impl SessionState {
    /// The version the bound repo's own `m1nd-mcp/Cargo.toml` declares, if a
    /// bound root (workspace_root, else any ingest root) actually contains one.
    /// This is the "am I testing against an old m1nd binary?" signal: the repo
    /// on disk says version X but this running binary is version Y. String
    /// compare only, warn-only — see [`SessionState::binary_version_info`].
    fn self_repo_declared_version(&self) -> Option<String> {
        let mut roots: Vec<&str> = Vec::new();
        if let Some(ws) = self.workspace_root.as_deref() {
            roots.push(ws);
        }
        for root in &self.ingest_roots {
            roots.push(root.as_str());
        }
        for root in roots {
            let manifest = PathBuf::from(root).join("m1nd-mcp").join("Cargo.toml");
            if let Ok(contents) = std::fs::read_to_string(&manifest) {
                if let Some(version) = parse_cargo_package_version(&contents) {
                    return Some(version);
                }
            }
        }
        None
    }

    /// Honest binary identity + drift detection, the core of the version-honesty
    /// moat. Always returns `binary_version` + `binary_git_sha`. When any drift
    /// signal fires it adds a `binary_drift` block; otherwise `binary_drift` is
    /// `null`. Three warn-first signals, all additive, none flips a verdict:
    ///
    ///   1. `M1ND_EXPECTED_VERSION` set and != running version.
    ///   2. `M1ND_EXPECTED_SHA` set and != running sha.
    ///   3. self-repo lag: the bound repo's `m1nd-mcp/Cargo.toml` declares a
    ///      version different from the running binary (`binary_lags_repo`) —
    ///      catches "experiment ran against a stale m1nd binary".
    ///
    /// Returns `(identity_json, drift_summary)` where `drift_summary` is a short
    /// one-line human warning when any signal fired, else `None`. Callers splice
    /// `drift_summary` into their honest notes/non_claims without changing the
    /// trust verdict.
    pub fn binary_version_info(&self) -> (serde_json::Value, Option<String>) {
        let running_version = BINARY_VERSION;
        let running_sha = BINARY_GIT_SHA;

        let expected_version = std::env::var("M1ND_EXPECTED_VERSION")
            .ok()
            .filter(|v| !v.trim().is_empty());
        let expected_sha = std::env::var("M1ND_EXPECTED_SHA")
            .ok()
            .filter(|v| !v.trim().is_empty());

        let version_mismatch = expected_version
            .as_deref()
            .map(|expected| expected.trim() != running_version)
            .unwrap_or(false);
        let sha_mismatch = expected_sha
            .as_deref()
            .map(|expected| expected.trim() != running_sha)
            .unwrap_or(false);

        let repo_version = self.self_repo_declared_version();
        let repo_lags = repo_version
            .as_deref()
            .map(|repo| repo != running_version)
            .unwrap_or(false);

        let drift = version_mismatch || sha_mismatch || repo_lags;

        let identity = if drift {
            let mut warnings: Vec<String> = Vec::new();
            if version_mismatch {
                warnings.push(format!(
                    "expected version {} but running {}",
                    expected_version.as_deref().unwrap_or(""),
                    running_version
                ));
            }
            if sha_mismatch {
                warnings.push(format!(
                    "expected sha {} but running {}",
                    expected_sha.as_deref().unwrap_or(""),
                    running_sha
                ));
            }
            if repo_lags {
                warnings.push(format!(
                    "bound repo m1nd-mcp/Cargo.toml declares {} but running {} (binary_lags_repo — likely testing against a stale binary)",
                    repo_version.as_deref().unwrap_or(""),
                    running_version
                ));
            }
            serde_json::json!({
                "binary_version": running_version,
                "binary_git_sha": running_sha,
                "binary_drift": {
                    "schema": "m1nd-binary-drift-v0",
                    "drift_detected": true,
                    "expected_version": expected_version,
                    "expected_sha": expected_sha,
                    "running_version": running_version,
                    "running_sha": running_sha,
                    "version_mismatch": version_mismatch,
                    "sha_mismatch": sha_mismatch,
                    "binary_lags_repo": repo_lags,
                    "repo_declared_version": repo_version,
                    "warning": warnings.join("; "),
                },
            })
        } else {
            serde_json::json!({
                "binary_version": running_version,
                "binary_git_sha": running_sha,
                "binary_drift": serde_json::Value::Null,
            })
        };

        let summary = if drift {
            Some(format!(
                "binary_drift: this m1nd-mcp is {running_version} ({running_sha}) which does not match the expected/repo binary — verify you are not testing against a stale binary"
            ))
        } else {
            None
        };

        (identity, summary)
    }

    pub fn binding_fingerprint(&self) -> serde_json::Value {
        let (binary_info, _drift_summary) = self.binary_version_info();
        let graph = self.graph.read();
        serde_json::json!({
            "schema": "m1nd-binding-fingerprint-v0",
            "process_id": std::process::id(),
            "current_exe": std::env::current_exe().ok().map(|path| path.to_string_lossy().to_string()),
            "binary_version": binary_info["binary_version"],
            "binary_git_sha": binary_info["binary_git_sha"],
            "binary_drift": binary_info["binary_drift"],
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

    /// Lightweight trust-mode band for the OMEGA Move 1 trust envelope, derived
    /// from the cheap in-memory binding reads only (no re-hash, no file scan).
    /// The honest CHEAP SUBSET of the handshake's trust_mode — see
    /// `trust_envelope::cheap_trust_mode_band` for the classification and what it
    /// deliberately does NOT observe (host surface, workspace mismatch).
    pub fn seek_binding_band(&self) -> &'static str {
        let (node_count, edge_count, finalized) = {
            let graph = self.graph.read();
            (
                graph.num_nodes() as u64,
                graph.num_edges() as u64,
                graph.finalized,
            )
        };
        crate::trust_envelope::cheap_trust_mode_band(node_count, edge_count, finalized)
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
        let binding_kind = Self::scope_binding_kind_for_mismatch(
            &scope_path,
            &requested_workspace_hint,
            &known_roots,
        );
        let partial_scope = binding_kind != "wrong_workspace_binding";
        let (scope_reliability, recommended_usage_mode, message) = match binding_kind {
            "nested_workspace_binding" => (
                "partial_subtree_truth",
                "partial_scope_orientation",
                "The active m1nd binding is inside the requested repository, so it can guide only that subtree until the repo root is bound.",
            ),
            "file_level_binding" => (
                "document_context_only",
                "partial_scope_orientation",
                "The active m1nd binding points at a file-level artifact inside the requested repository, so it is document context rather than codebase coverage.",
            ),
            _ => (
                "wrong_workspace",
                "isolated_probe_after_wrong_workspace_binding",
                "The requested absolute scope is outside the active m1nd workspace and ingest roots.",
            ),
        };
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
            "binding_kind": binding_kind,
            "partial_scope": partial_scope,
            "scope_reliability": scope_reliability,
            "recommended_usage_mode": recommended_usage_mode,
            "requested_scope": scope.unwrap_or_default(),
            "requested_scope_path": scope_path.to_string_lossy(),
            "requested_workspace_hint": requested_workspace_hint.to_string_lossy(),
            "requested_context_id": requested_context_id,
            "active_workspace_root": self.workspace_root,
            "active_workspace_root_source": self.workspace_root_source,
            "active_ingest_roots": self.ingest_roots,
            "known_roots_checked": known_root_values,
            "runtime_root": self.runtime_root.to_string_lossy(),
            "message": message,
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

    fn scope_binding_kind_for_mismatch(
        scope_path: &std::path::Path,
        requested_workspace_hint: &std::path::Path,
        known_roots: &[(&str, PathBuf)],
    ) -> &'static str {
        let partial_root = known_roots.iter().map(|(_, root)| root).find(|root| {
            Self::path_starts_with_loosely(root, requested_workspace_hint)
                || Self::path_starts_with_loosely(root, scope_path)
        });

        match partial_root {
            Some(root) if Self::is_file_level_binding_root(root) => "file_level_binding",
            Some(_) => "nested_workspace_binding",
            None => "wrong_workspace_binding",
        }
    }

    fn is_file_level_binding_root(root: &std::path::Path) -> bool {
        if root.is_file() {
            return true;
        }

        matches!(
            root.extension().and_then(|extension| extension.to_str()),
            Some(
                "bib"
                    | "doc"
                    | "docx"
                    | "html"
                    | "json"
                    | "l1ght"
                    | "light"
                    | "md"
                    | "pdf"
                    | "prd"
                    | "rst"
                    | "txt"
                    | "xml"
            )
        )
    }

    /// Degraded First-Contact Reception verdict (TWO-TIER-BRAIN-PRD §9.5.5).
    ///
    /// Returns a compact `reception` block ONLY when the caller's resolved root
    /// (hop-2 `M1nd-Caller-Root`) is KNOWN and does NOT fall under the bound
    /// workspace / ingest roots — the live Antigravity/Cherry failure, made loud.
    /// Returns `None` on:
    ///   (a) unknown caller root — honesty by omission, §9.5.4 absent ≠ wrong; and
    ///   (b) match — TT-INV-12 silence-when-matched (silent binding is legal only
    ///       when the caller's root matches the bound brain).
    ///
    /// Reuses the mismatch guard's `path_starts_with_loosely` (canonicalize +
    /// normalize) and its exact known-roots list (`workspace_root` + `ingest_roots`),
    /// lifted from the per-call `scope` opt-in to a first-contact default.
    pub fn reception_verdict(&self) -> Option<serde_json::Value> {
        // Unknown caller (direct-HTTP / legacy bridge sent no header) → cannot
        // compute the match; say nothing rather than raise a false alarm.
        let caller_root = self.caller_root.as_deref()?;

        // Caller falls under a bound root → legal silent bind, no packet.
        if self.covers_root(caller_root) {
            return None;
        }

        // Mismatch: the bound graph does NOT cover the caller's repo. Say so, and
        // hand the agent machine-actionable options. `ingest_your_repo` is now the
        // REAL one-call bootstrap (owner-hosted project brain — Two-Tier interim),
        // no longer a roadmap promise.
        Some(serde_json::json!({
            "schema": "m1nd-reception-degraded-v0",
            "match": "caller_root_mismatch",
            "caller_root": caller_root,
            "bound_workspace": self.workspace_root.clone().unwrap_or_default(),
            "honest": "this graph does NOT cover your repo",
            "options": [
                {
                    "action": "continue_bound",
                    "note": "keep using this graph, but treat its answers as NOT covering your current repo — verify against local files"
                },
                {
                    "action": "ingest_your_repo",
                    "call": format!("ingest with project_root={caller_root} — ONE call: creates a per-project brain inside this owner, ingests your repo into it, binds this session to it, and returns its north packet; thereafter every call from this root routes to YOUR brain automatically (silent on match)")
                }
            ]
        }))
    }

    /// True when `root` falls under this brain's bound territory — the
    /// `workspace_root` or any ingest root (the exact known-roots list the
    /// `workspace_binding_mismatch` guard compares against). ONE definition of
    /// "does this brain cover that caller?", shared by the reception verdict
    /// above and the Two-Tier HTTP routing layer.
    pub fn covers_root(&self, root: &str) -> bool {
        let mut known_roots: Vec<std::path::PathBuf> = Vec::new();
        if let Some(workspace_root) = self.workspace_root.as_deref() {
            known_roots.push(std::path::PathBuf::from(workspace_root));
        }
        for ingest_root in &self.ingest_roots {
            known_roots.push(std::path::PathBuf::from(ingest_root));
        }
        let candidate = std::path::Path::new(root);
        known_roots
            .iter()
            .any(|known| Self::path_starts_with_loosely(candidate, known))
    }

    /// The brain's real PROJECT root — the repo it maps, NOT its runtime sidecar.
    ///
    /// The Hall (HUMAN-LAYER-PRD §4A.3) must name brains by their project, never
    /// by plumbing: the bound dev graph's `workspace_root` is its `agent-memory`
    /// sidecar dir (inferred `graph_path_parent`), so naming from it leaks
    /// "agent-memory"/"claude". The true project is the primary *code* ingest
    /// root (e.g. `/Users/kle1nz/m1nd`). Precedence, mirroring
    /// `self_repo_declared_version`'s "which root is the repo" rule:
    ///
    /// 1. the first ingest root that is a real directory and is NOT a `.light.md`
    ///    memory sidecar nor an `agent-memory` runtime dir;
    /// 2. else `workspace_root` when it is not itself an `agent-memory` dir;
    /// 3. else the first ingest root of any kind;
    /// 4. else `workspace_root` (last resort — honest even if it is plumbing).
    ///
    /// A project brain always has its project root as ingest root #1, so it
    /// resolves to that; the bound graph skips its memory sidecars to reach the
    /// repo. Returns `None` only when the brain has no roots at all (empty graph).
    pub fn project_root_display(&self) -> Option<String> {
        let is_memory_sidecar = |p: &str| {
            p.ends_with(".light.md")
                || std::path::Path::new(p)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n == "agent-memory")
                    .unwrap_or(false)
        };
        // 1. First real code ingest root that is not a memory sidecar.
        for root in &self.ingest_roots {
            if !is_memory_sidecar(root) && std::path::Path::new(root).is_dir() {
                return Some(root.clone());
            }
        }
        // 2. workspace_root when it is a repo, not the agent-memory sidecar dir.
        if let Some(ws) = self.workspace_root.as_deref() {
            if !is_memory_sidecar(ws) {
                return Some(ws.to_string());
            }
        }
        // 3. any ingest root, then 4. workspace_root as the last honest fallback.
        self.ingest_roots
            .first()
            .cloned()
            .or_else(|| self.workspace_root.clone())
    }

    /// The Hall card / Brain Chip display name: the basename of
    /// [`project_root_display`] — "m1nd", "Cerrybubbles1" — never a runtime dir
    /// name ("claude") nor "agent-memory". `None` when the brain has no roots.
    pub fn display_name(&self) -> Option<String> {
        self.project_root_display().map(|root| basename_of(&root))
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

    fn recovery_call_arguments(
        &self,
        agent_id: &str,
        observed_tool: &str,
        observed_proof_state: &str,
        observed_candidates: Option<u64>,
        scope: Option<&str>,
        error_text: Option<&str>,
    ) -> (serde_json::Value, Option<serde_json::Value>) {
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

        (arguments, workspace_binding_mismatch)
    }

    fn recovery_auto_action_payload(
        &self,
        context: RecoveryAutoActionContext<'_>,
    ) -> serde_json::Value {
        let scope_key = if context
            .scope
            .filter(|value| !value.trim().is_empty())
            .is_some()
        {
            "scoped"
        } else {
            "unscoped"
        };
        let candidate_key = context
            .observed_candidates
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string());

        serde_json::json!({
            "schema": "m1nd-auto-action-v0",
            "status": "ready",
            "action_type": "tool_call",
            "tool": "recovery_playbook",
            "arguments": context.arguments,
            "source": {
                "kind": context.source_kind,
                "surface": "recovery_payload",
                "agent_id": context.agent_id,
                "observed_tool": context.observed_tool,
                "observed_proof_state": context.observed_proof_state,
            },
            "reason": context.reason,
            "expected_output_schema": "m1nd-recovery-playbook-v0",
            "safety": {
                "mutation": "read_only",
                "requires_confirmation": false,
                "side_effects": "none",
            },
            "idempotency_key": format!(
                "recovery_playbook:{}:{}:{}:{}:{}",
                context.agent_id, context.observed_tool, context.observed_proof_state, candidate_key, scope_key
            ),
        })
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
        let (arguments, workspace_binding_mismatch) = self.recovery_call_arguments(
            agent_id,
            observed_tool,
            observed_proof_state,
            observed_candidates,
            scope,
            error_text,
        );

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
        let (arguments, workspace_binding_mismatch) = self.recovery_call_arguments(
            agent_id,
            observed_tool,
            observed_proof_state,
            observed_candidates,
            scope,
            error_text,
        );

        let reason = if workspace_binding_mismatch.is_some() {
            "wrong workspace binding detected; recovery_playbook returns the ordered context selection path before shell fallback"
        } else {
            "retrieval blocked or the active graph is not yet trusted for this query; recovery_playbook returns the ordered agent recovery path before deeper diagnosis"
        };
        let source_kind = if workspace_binding_mismatch.is_some() {
            "wrong_workspace_binding"
        } else {
            "retrieval_needs_recovery"
        };
        let auto_action = self.recovery_auto_action_payload(RecoveryAutoActionContext {
            agent_id,
            observed_tool,
            observed_proof_state,
            observed_candidates,
            scope,
            reason,
            source_kind,
            arguments: &arguments,
        });

        let mut payload = serde_json::json!({
            "suggested_tool": "recovery_playbook",
            "reason": reason,
            "arguments": arguments,
            "fallback_tool": "doctor",
            "auto_action": auto_action,
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
        let graph_populated = {
            let graph = self.graph.read();
            graph.num_nodes() > 0
        };
        let needs_recovery = observed_proof_state == "blocked"
            || !graph_populated
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
        let observed_blocked = observed_proof_state == "blocked";
        let needs_recovery =
            workspace_binding_mismatch.is_some() || !graph_populated || observed_blocked;
        let trust_mode = if workspace_binding_mismatch.is_some() {
            "wrong_workspace_binding"
        } else if !graph_populated {
            "needs_ingest"
        } else if observed_blocked {
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
        let auto_action = recovery
            .as_ref()
            .and_then(|payload| payload.get("auto_action"))
            .cloned()
            .unwrap_or(serde_json::Value::Null);
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
                    "version": BINARY_VERSION,
                    "git_sha": BINARY_GIT_SHA,
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
            "auto_action": auto_action,
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
            // The bound brain's PROJECT identity — the Brain Chip's name source,
            // so the chip reads "m1nd" and never the agent-memory sidecar
            // (`graph_state.workspace_root`). Same derivation the Hall uses
            // (project_root_display / display_name), so chip and card agree.
            "display_name": self.display_name(),
            "project_root": self.project_root_display(),
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
        // Resolve the runtime root up front so the embedding cache (and its
        // directory) exist before any engine build writes to them.
        let runtime_root = config.runtime_dir.clone().unwrap_or_else(|| {
            config
                .graph_source
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf()
        });
        std::fs::create_dir_all(&runtime_root)?;
        // OPTIONAL `embed` feature: per-node embeddings are cached on disk next
        // to the snapshot so a warm boot reuses them instead of recomputing.
        // Ignored entirely when the `embed` feature is off.
        let embeddings_cache_path = runtime_root.join("embeddings_cache.bin");

        // Build all engines from graph (semantic reuses the embedding cache).
        // Only the writable owner persists the cache; a read-only attacher reuses
        // it but never writes (honoring the read-only "persistence disabled" contract).
        let orchestrator = QueryOrchestrator::build_with_cache(
            &graph,
            Some(&embeddings_cache_path),
            !config.read_only,
        )?;
        let temporal = TemporalEngine::build(&graph)?;
        let counterfactual = CounterfactualEngine::with_defaults();
        let topology = TopologyAnalyzer::with_defaults();
        let resonance = ResonanceEngine::with_defaults();
        let plasticity =
            PlasticityEngine::new(&graph, m1nd_core::plasticity::PlasticityConfig::default());

        let shared = Arc::new(parking_lot::RwLock::new(graph));
        let (workspace_root, workspace_root_source) =
            Self::infer_workspace_root(config, &runtime_root);
        let instance_mode = if config.read_only {
            crate::instance_registry::InstanceMode::ReadOnly
        } else {
            crate::instance_registry::InstanceMode::ReadWrite
        };
        let instance = InstanceHandle::acquire_with_mode(
            &workspace_root,
            &runtime_root,
            &config.graph_source,
            &config.plasticity_state,
            config.registry_dir.as_deref(),
            instance_mode,
        )?;
        if config.read_only {
            eprintln!(
                "[m1nd] read-only attach: holding no lease; persistence disabled; mutation tools gated."
            );
        }
        // Best-effort, non-blocking sweep of dead lease/instance entries at every
        // boot. The daemon-tick GC only runs when the daemon is active, so dead
        // entries otherwise leak unbounded (~25k observed live). Detached on its
        // own thread so it can NEVER delay the `initialize`/`tools/list`
        // handshake regardless of registry size; our own live-pid entry (just
        // written by `acquire_with_mode`) is never touched. Handle dropped:
        // fire-and-forget.
        let _ = crate::instance_registry::spawn_boot_gc(instance.registry_root());
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
            embeddings_cache_path,
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
            caller_root: None,
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
            calibration_table: {
                let cal_path = runtime_root.join("calibration_state.json");
                m1nd_core::calibration::load_calibration_state(&cal_path)
                    .unwrap_or_else(|_| m1nd_core::calibration::CalibrationTable::new())
            },
            calibration_path: runtime_root.join("calibration_state.json"),
            // v0.4.0: Query Log (savings tracker/state removed — brand gate G1.5)
            query_log: Vec::new(),
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
            proof_ready: HashMap::new(),
            flagged_findings: HashMap::new(),
            auto_ingest: AutoIngestState::load(&runtime_root),
            document_cache: load_document_cache(&runtime_root),
            agent_memory_boot: None,
            read_only: config.read_only,
            read_only_persist_logged: std::cell::Cell::new(false),
        })
    }

    /// Check if auto-persist should trigger. Returns true every N queries.
    ///
    /// Always false in read-only attach mode so the every-N-queries auto-persist
    /// never fires and the read-only process never writes to disk.
    pub fn should_persist(&self) -> bool {
        !self.read_only
            && self.queries_processed > 0
            && self
                .queries_processed
                .is_multiple_of(self.auto_persist_interval as u64)
    }

    /// Log the read-only persist skip exactly once per session.
    fn log_read_only_persist_skip(&self) {
        if !self.read_only_persist_logged.replace(true) {
            eprintln!("[m1nd] read-only attach: skipping persist");
        }
    }

    /// Run an orchestrator query, picking the lock + method by attach mode.
    ///
    /// In read-only mode this takes an immutable `graph.read()` borrow and calls
    /// [`QueryOrchestrator::query_readonly`], which skips plasticity Step 8 and
    /// never mutates the graph. In read-write mode it takes `graph.write()` and
    /// calls the normal `query`, preserving the historical mutate-on-query
    /// (plasticity) behavior. Centralizing this keeps every call site honest
    /// about the read-only contract.
    pub fn run_query(
        &mut self,
        config: &m1nd_core::query::QueryConfig,
    ) -> M1ndResult<m1nd_core::query::QueryResult> {
        if self.read_only {
            let graph = self.graph.read();
            self.orchestrator
                .query_readonly(&graph, config, &self.domain)
        } else {
            let mut graph = self.graph.write();
            self.orchestrator.query(&mut graph, config, &self.domain)
        }
    }

    /// Persist all state to disk.
    ///
    /// Ordering: graph first (source of truth), then plasticity.
    /// If graph save fails, skip plasticity to avoid inconsistent state.
    /// If plasticity save fails after graph succeeds, log warning but don't crash.
    pub fn persist(&mut self) -> M1ndResult<()> {
        // HARD SAFETY: a read-only attach must never write to disk. This is the
        // single choke point every persist call site funnels through, so this
        // early return protects the writer's on-disk state from corruption.
        if self.read_only {
            self.log_read_only_persist_skip();
            return Ok(());
        }
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

        if let Err(e) = m1nd_core::calibration::save_calibration_state(
            &self.calibration_table,
            &self.calibration_path,
        ) {
            eprintln!("[m1nd] WARNING: calibration persist failed: {}", e);
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
        let persist_root = self
            .graph_path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| self.runtime_root.clone());
        if let Err(e) = std::fs::create_dir_all(&persist_root) {
            eprintln!("[m1nd] WARNING: ingest roots persist dir failed: {}", e);
            return;
        }
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
        if self.read_only {
            self.log_read_only_persist_skip();
            return Ok(());
        }
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
        if self.read_only {
            self.log_read_only_persist_skip();
            return Ok(());
        }
        save_json_atomic(&self.daemon_state_path, &self.daemon_state)
    }

    fn load_daemon_state(path: &Path) -> DaemonRuntimeState {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<DaemonRuntimeState>(&s).ok())
            .unwrap_or_default()
    }

    pub fn persist_daemon_alerts(&self) -> M1ndResult<()> {
        if self.read_only {
            self.log_read_only_persist_skip();
            return Ok(());
        }
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
        self.calibration_table =
            m1nd_core::calibration::load_calibration_state(&self.calibration_path)
                .unwrap_or_else(|_| m1nd_core::calibration::CalibrationTable::new());
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
            self.orchestrator = QueryOrchestrator::build_with_cache(
                &graph,
                Some(&self.embeddings_cache_path),
                !self.read_only,
            )?;
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

    /// Record that `agent_id` drove `raw_target` to `proof_state ==
    /// "ready_to_edit"` (M1ND_PROOF_GATE). `raw_target` may be absolute,
    /// repo-relative, or `file::`-prefixed; it is normalized through
    /// [`crate::scope::normalize_scope_path`] so the recorded key compares equal
    /// to the key the write gate derives from the about-to-edit path. A target
    /// that normalizes to `None` (empty/repo-root) is skipped so a malformed
    /// target never silently grants edit permission. `evidence` names the prover.
    pub fn note_proof_ready(&mut self, agent_id: &str, raw_target: &str, evidence: &str) {
        let Some(target) = crate::scope::normalize_scope_path(Some(raw_target), &self.ingest_roots)
        else {
            return;
        };
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        self.proof_ready.insert(
            (agent_id.to_string(), target),
            ProofReadyMark {
                proved_at_ms: now_ms,
                generation: self.cache_generation,
                evidence: Some(evidence.to_string()),
            },
        );
    }

    /// Whether `agent_id` has a proof-ready mark for `raw_target` (normalized via
    /// the same [`crate::scope::normalize_scope_path`] used when recording). A
    /// target that normalizes to `None` is treated as not-proved.
    pub fn is_proof_ready(&self, agent_id: &str, raw_target: &str) -> bool {
        let Some(target) = crate::scope::normalize_scope_path(Some(raw_target), &self.ingest_roots)
        else {
            return false;
        };
        self.proof_ready
            .contains_key(&(agent_id.to_string(), target))
    }

    /// Borrow the proof-ready mark for inspection (staleness/evidence), mirroring
    /// [`Self::get_perspective`].
    pub fn get_proof_ready(&self, agent_id: &str, raw_target: &str) -> Option<&ProofReadyMark> {
        let target = crate::scope::normalize_scope_path(Some(raw_target), &self.ingest_roots)?;
        self.proof_ready.get(&(agent_id.to_string(), target))
    }

    /// Record that `agent_id`'s scan/audit flagged a finding against `node_id`
    /// (an opaque external id) this session. Mirrors [`Self::note_proof_ready`]
    /// but keys on the raw `node_id` directly (NO path normalization) so the
    /// recorder (scan/audit) and the reader (edit/apply) agree on the external-id
    /// form. Ephemeral — never persisted. The map is capped at
    /// [`MAX_FLAGGED_FINDINGS`] entries; the oldest entry is evicted on overflow
    /// so a long-running session cannot grow it without bound.
    pub fn note_finding(
        &mut self,
        agent_id: &str,
        node_id: &str,
        kind: &str,
        severity: &str,
        file_path: &str,
    ) {
        if node_id.is_empty() {
            return;
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let key = (agent_id.to_string(), node_id.to_string());
        if !self.flagged_findings.contains_key(&key)
            && self.flagged_findings.len() >= MAX_FLAGGED_FINDINGS
        {
            // Evict the oldest mark to keep the ephemeral map bounded.
            if let Some(oldest) = self
                .flagged_findings
                .iter()
                .min_by_key(|(_, mark)| mark.flagged_at_ms)
                .map(|(k, _)| k.clone())
            {
                self.flagged_findings.remove(&oldest);
            }
        }
        self.flagged_findings.insert(
            key,
            FindingMark {
                flagged_at_ms: now_ms,
                generation: self.cache_generation,
                kind: kind.to_string(),
                severity: severity.to_string(),
                file_path: file_path.to_string(),
            },
        );
    }

    /// Borrow the flagged-finding mark for `(agent_id, node_id)` for inspection.
    pub fn get_finding(&self, agent_id: &str, node_id: &str) -> Option<&FindingMark> {
        self.flagged_findings
            .get(&(agent_id.to_string(), node_id.to_string()))
    }

    /// Consume (remove and return) the flagged-finding mark for `(agent_id,
    /// node_id)`. Used at edit/apply time so a single fix emits the
    /// `proposed_antibody` once and does not re-propose on subsequent writes.
    pub fn take_finding(&mut self, agent_id: &str, node_id: &str) -> Option<FindingMark> {
        self.flagged_findings
            .remove(&(agent_id.to_string(), node_id.to_string()))
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
    fn display_name_is_the_repo_basename_not_the_agent_memory_sidecar() {
        // The exact leak Max saw: a bound dev graph whose workspace_root is its
        // `agent-memory` runtime sidecar (inferred graph_path_parent), with the
        // real repo + memory `.light.md` files among its ingest roots. The Hall
        // name must be the REPO basename ("m1nd"), never "agent-memory".
        let temp = tempfile::tempdir().expect("tempdir");
        let repo = temp.path().join("m1nd");
        std::fs::create_dir_all(&repo).expect("repo dir");
        let agent_memory = temp
            .path()
            .join("runtimes")
            .join("claude")
            .join("agent-memory");
        std::fs::create_dir_all(&agent_memory).expect("agent-memory dir");

        let config = McpConfig {
            graph_source: temp.path().join("graph_snapshot.json"),
            plasticity_state: temp.path().join("plasticity_state.json"),
            runtime_dir: Some(temp.path().to_path_buf()),
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");
        // Mimic the bound brain's real state: workspace = agent-memory sidecar,
        // ingest_roots = the repo first, then memory sidecar files + the dir.
        state.workspace_root = Some(agent_memory.to_string_lossy().to_string());
        state.ingest_roots = vec![
            repo.to_string_lossy().to_string(),
            agent_memory
                .join("some-memory.light.md")
                .to_string_lossy()
                .to_string(),
            agent_memory.to_string_lossy().to_string(),
        ];

        assert_eq!(
            state.display_name().as_deref(),
            Some("m1nd"),
            "the bound brain must be named by its repo, not the agent-memory sidecar"
        );
        assert_eq!(
            state.project_root_display().as_deref(),
            Some(repo.to_string_lossy().as_ref()),
            "the project root must resolve to the real repo directory"
        );
    }

    #[test]
    fn display_name_falls_back_to_workspace_when_no_code_root() {
        // A brain with only its agent-memory workspace and NO real code ingest
        // root still returns an honest name (the workspace basename), never None
        // and never a panic — absence is absence, not a crash.
        let temp = tempfile::tempdir().expect("tempdir");
        let config = McpConfig {
            graph_source: temp.path().join("graph_snapshot.json"),
            plasticity_state: temp.path().join("plasticity_state.json"),
            runtime_dir: Some(temp.path().to_path_buf()),
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");
        state.ingest_roots = vec![];
        state.workspace_root = Some("/Users/x/solo-repo".to_string());
        assert_eq!(state.display_name().as_deref(), Some("solo-repo"));
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
    fn ingest_roots_persist_next_to_graph_not_workspace_hint() {
        let _guard = env_lock().lock().expect("env lock");
        let _env = EnvGuard::clear_workspace_hints();

        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("project");
        let runtime = temp.path().join("runtime");
        std::fs::create_dir_all(&workspace).expect("workspace dir");
        std::fs::create_dir_all(&runtime).expect("runtime dir");
        std::env::set_var("M1ND_WORKSPACE_ROOT", &workspace);

        let config = McpConfig {
            graph_source: runtime.join("graph_snapshot.json"),
            plasticity_state: runtime.join("plasticity_state.json"),
            runtime_dir: Some(runtime.clone()),
            ..McpConfig::default()
        };

        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");
        state.ingest_roots = vec![workspace.to_string_lossy().to_string()];
        state.persist_ingest_roots();

        assert!(runtime.join("ingest_roots.json").exists());
        assert!(!workspace.join("ingest_roots.json").exists());
        let persisted = std::fs::read_to_string(runtime.join("ingest_roots.json"))
            .expect("persisted ingest roots");
        let persisted_roots: Vec<String> =
            serde_json::from_str(&persisted).expect("persisted ingest roots json");
        assert!(persisted_roots.contains(&workspace.to_string_lossy().to_string()));
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
    fn workspace_binding_mismatch_classifies_nested_workspace_binding() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo = temp.path().join("repo");
        let nested = repo.join("docs").join("prds");
        std::fs::create_dir_all(&nested).expect("nested workspace");
        std::fs::write(repo.join("package.json"), "{\"name\":\"repo\"}\n").expect("manifest");

        let config = McpConfig {
            graph_source: temp.path().join("runtime").join("graph_snapshot.json"),
            plasticity_state: temp.path().join("runtime").join("plasticity_state.json"),
            runtime_dir: Some(temp.path().join("runtime")),
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");
        state.workspace_root = Some(nested.to_string_lossy().to_string());

        let repo_scope = repo.to_string_lossy().to_string();
        let mismatch = state
            .workspace_binding_mismatch(Some(&repo_scope))
            .expect("nested workspace should be partial binding");

        assert_eq!(mismatch["code"], "wrong_workspace_binding");
        assert_eq!(mismatch["binding_kind"], "nested_workspace_binding");
        assert_eq!(mismatch["partial_scope"], true);
        assert_eq!(
            mismatch["recommended_usage_mode"],
            "partial_scope_orientation"
        );
    }

    #[test]
    fn workspace_binding_mismatch_classifies_file_level_binding() {
        let _guard = env_lock().lock().expect("env lock");
        let _env = EnvGuard::clear_workspace_hints();

        let temp = tempfile::tempdir().expect("tempdir");
        let repo = temp.path().join("repo");
        let doc = repo.join("docs").join("PRD.md");
        std::fs::create_dir_all(doc.parent().expect("doc parent")).expect("docs");
        std::fs::write(repo.join("package.json"), "{\"name\":\"repo\"}\n").expect("manifest");
        std::fs::write(&doc, "# PRD\n").expect("doc");

        let config = McpConfig {
            graph_source: temp.path().join("runtime").join("graph_snapshot.json"),
            plasticity_state: temp.path().join("runtime").join("plasticity_state.json"),
            runtime_dir: Some(temp.path().join("runtime")),
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");
        state.workspace_root = None;
        state.ingest_roots = vec![doc.to_string_lossy().to_string()];

        let repo_scope = repo.to_string_lossy().to_string();
        let mismatch = state
            .workspace_binding_mismatch(Some(&repo_scope))
            .expect("file-level ingest root should be partial binding");

        assert_eq!(mismatch["code"], "wrong_workspace_binding");
        assert_eq!(mismatch["binding_kind"], "file_level_binding");
        assert_eq!(mismatch["partial_scope"], true);
        assert_eq!(mismatch["scope_reliability"], "document_context_only");
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
        assert_eq!(contract["auto_action"]["schema"], "m1nd-auto-action-v0");
        assert_eq!(contract["auto_action"]["status"], "ready");
        assert_eq!(contract["auto_action"]["tool"], "recovery_playbook");
        assert_eq!(
            contract["recovery"]["auto_action"]["safety"]["requires_confirmation"],
            false
        );
        assert_eq!(
            contract["session_identity"]["binary"]["version"],
            env!("CARGO_PKG_VERSION")
        );
    }

    #[test]
    fn agent_runtime_contract_keeps_zero_candidates_without_blocked_proof_in_full_trust() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        std::fs::create_dir_all(workspace.join("src")).expect("workspace src");

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

        let contract =
            state.agent_runtime_contract("jimi", "seek", "triaging", Some(0), None, None);

        assert_eq!(contract["trust_mode"], "full_trust");
        assert_eq!(contract["status"], "ok");
        assert_eq!(contract["auto_action"], serde_json::Value::Null);
        assert_eq!(contract["recovery"], serde_json::Value::Null);
        assert_eq!(contract["next_suggested_tool"], serde_json::Value::Null);
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
