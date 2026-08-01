// === crates/m1nd-mcp/src/session.rs ===

use m1nd_core::antibody::Antibody;
use m1nd_core::counterfactual::CounterfactualEngine;
use m1nd_core::domain::DomainConfig;
use m1nd_core::error::{M1ndError, M1ndResult};
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
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::auto_ingest::AutoIngestState;
use crate::boot_kv_migration::BootKvCheckpointInventoryV1;
use crate::instance_registry::{InstanceHandle, InstanceRegistryEntry};
use crate::perspective::state::{
    LockState, PeekSecurityConfig, PerspectiveLimits, PerspectiveState, WatchTrigger, WatcherEvent,
};
use crate::universal_docs::{
    load_document_artifact_inventory_friendly, load_document_cache, DocumentArtifactInventory,
    DocumentArtifactPresence, DocumentCacheState,
};

// ---------------------------------------------------------------------------
// AgentSession — per-agent session tracking
// ---------------------------------------------------------------------------

/// Lightweight session record for a connected agent.
pub struct AgentSession {
    pub agent_id: String,
    pub first_seen: Instant,
    pub last_seen: Instant,
    pub query_count: u64,
    // --- ORGANISM-INSIDE P1 — the durable-presence beat state (askGOD verdict
    //     2026-07-13). In-memory; projected to a sidecar by the throttled beat. ---
    /// Epoch ms of first contact — the durable age the sidecar renders (`Instant`
    /// is process-local and meaningless across an owner restart).
    pub first_seen_ms: u64,
    /// Throttle clock for the presence beat (at most one disk write per
    /// [`crate::presence::PRESENCE_BEAT_THROTTLE_MS`]). `None` until the first beat.
    pub last_presence_beat: Option<Instant>,
    /// Epoch ms of the last mutating verb this session dispatched (the OBSERVED
    /// mutation level), stamped by [`SessionState::note_mutation_observed`].
    pub mutation_observed_at_ms: Option<u64>,
    /// DECLARED enrichment from `session_handshake` (optional, honest-absent).
    pub declared_kind: Option<String>,
    pub declared_theme: Option<String>,
    pub declared_intent: Option<String>,
    pub declared_worktree: Option<String>,
    pub declared_working_set: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditPreviewState {
    pub preview_id: String,
    pub agent_id: String,
    pub file_path: String,
    pub new_content: String,
    pub source_hash: String,
    /// Canonical authority-grade digest of the exact bytes observed by preview.
    /// `source_hash` above remains only for legacy preview compatibility.
    pub source_sha256: String,
    /// Canonical digest of the exact candidate bytes staged by preview.
    pub candidate_sha256: String,
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

/// One planned write of a staged two-phase transplant (A2): the exact content
/// the commit will land and the hash of the on-disk text the plan was computed
/// FROM (`base_hash` — the TOCTOU anchor: any drift refuses the commit).
#[derive(Clone, Debug)]
pub struct PlannedTransplantWrite {
    pub file_path: String,
    pub new_content: String,
    pub base_hash: String,
    pub description: Option<String>,
}

/// A staged `transplant_preview` awaiting `transplant_commit` (A2, PRD §4.2).
/// Mirrors [`EditPreviewState`] (same 5-min TTL, same consume-on-commit law) but
/// carries the FULL multi-file plan — source + dest + every derived referencer —
/// plus the candidate receipt the commit finalizes.
#[derive(Clone, Debug)]
pub struct TransplantPreviewState {
    pub preview_id: String,
    pub agent_id: String,
    pub symbol: String,
    pub source_file: String,
    pub dest_file: String,
    /// Planned writes in write order (source, dest, referencers).
    pub planned: Vec<PlannedTransplantWrite>,
    /// The receipt computed at plan time; the commit re-stamps its timing.
    pub receipt: crate::protocol::surgical::TransplantOutput,
    pub created_at_ms: u64,
}

/// Generation-bound lexical document prepared once and reused by narrative
/// `seek` calls. It is deliberately runtime-only: graph_generation is the
/// authoritative invalidation fence, so no stale on-disk search index can be
/// mistaken for current graph truth.
#[derive(Clone, Debug, Default)]
pub(crate) struct SeekFileIndexDocument {
    pub path: String,
    pub length: usize,
    pub term_counts: HashMap<String, u32>,
    pub entity_terms: HashSet<String>,
    pub file_idx: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SeekFileIndexCache {
    pub graph_generation: u64,
    pub documents: Vec<SeekFileIndexDocument>,
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

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct BootMemoryState {
    pub entries: HashMap<String, BootMemoryEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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
    /// Absolute expiry of this one-shot proof mark.
    pub expires_at_ms: u64,
    /// Graph generation captured at proof time. Any ingest/rebuild invalidates it.
    pub graph_generation: u64,
    /// Canonical absolute target identity, including its workspace-root scope.
    pub target_identity: String,
    /// Exact on-disk state at proof time (`sha256:<hex>` or `missing`).
    pub target_digest: String,
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
    /// Gardener v1 — BURST BACKLOG. External ids detected as changed but not yet
    /// re-ingested (a burst bigger than one tick's `max_files` budget). The tick
    /// detects ONCE (one git diff / inventory compare per burst), pushes the whole
    /// changed set here, advances `git_since_ref` immediately (the backlog owns
    /// the tail), and drains up to `max_files` per tick — so a thousand-file
    /// checkout is ONE detection plus bounded drain ticks, and NO file is lost to
    /// the old truncate-then-advance hole. FIFO drain: completeness over recency
    /// (no starvation; a single burst lands in one detection anyway, newest-first
    /// within the batch). `serde(default)`: pre-gardener daemon_state.json files
    /// lack this field and must keep deserializing (a failed parse would fall
    /// back to Default and silently DISARM a resumed daemon).
    #[serde(default)]
    pub pending_backlog: Vec<String>,
    /// Gardener v1 — AUTO-RECONCILE quiet-window deadline. Set (and PUSHED) by
    /// every tick that saw activity; when a quiet tick passes it with an empty
    /// backlog, the daemon reconciles the RATIFIED system-blocks store (with
    /// voluntary lease yield and a 1-retry OCC policy). `None` = nothing owed.
    #[serde(default)]
    pub reconcile_due_at_ms: Option<u64>,
    /// When the last auto-reconcile actually ran (status honesty).
    #[serde(default)]
    pub last_auto_reconcile_ms: Option<u64>,
    /// How many auto-reconciles this daemon has run since it was armed.
    #[serde(default)]
    pub auto_reconcile_runs: u64,
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

/// Optional live sink for `skeleton_candidate` scan-phase progress emission
/// (docs/uml/scan-loading.md slice 2). The HTTP owner wires it per-request; on
/// every other path it stays `None` and the scan emits nothing (retrocompat).
pub type ScanProgressSink = Arc<dyn Fn(&crate::skeleton_scan::ScanProgressEvent) + Send + Sync>;

const CHECKPOINT_GRAPH_SCHEMA_ID: &str = "m1nd-graph-snapshot";
const CHECKPOINT_ROOTS_SCHEMA_ID: &str = "m1nd-ingest-roots";
const CHECKPOINT_SIDECAR_SCHEMA_ID: &str = "m1nd-session-sidecar";
const CHECKPOINT_ROOTS_SCHEMA_VERSION: &str = "1";
const CHECKPOINT_SIDECAR_SCHEMA_VERSION: &str = "1";

/// Explicit working-set decision for a candidate-first checkpoint. `Absent` is
/// not the same as omission: it instructs post-commit projection/rollback to
/// remove a previously owned file when this generation no longer owns it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CheckpointCandidatePresence {
    Present(Vec<u8>),
    Absent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SessionCheckpointCandidateFile {
    pub logical_name: String,
    pub relative_path: String,
    pub schema_id: String,
    pub schema_version: String,
    pub presence: CheckpointCandidatePresence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SessionCheckpointCandidate {
    pub files: Vec<SessionCheckpointCandidateFile>,
    /// Whether any legacy handler requested persistence while staged. This is
    /// diagnostic evidence only; candidate construction always serializes the
    /// complete in-memory state regardless of the flag.
    pub persist_requested: bool,
    /// Domain-separated digest over the complete PRESENT/ABSENT inventory.
    /// Useful as a mutation witness before deciding whether a callback refusal
    /// is safe to return without degrading the actor.
    pub state_digest: String,
}

/// Opaque capability proving that the caller owns the active SessionState
/// persistence stage. It is Clone (but not Copy) so the actor can retain a
/// reconciliation token across post-CURRENT confirmation. Finishing once
/// invalidates every retained clone; an abandoned transaction remains staged
/// until authoritative recovery replaces it.
#[derive(Clone, Debug)]
pub(crate) struct CheckpointPersistenceStage {
    id: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PersistenceStageState {
    id: u64,
    persist_requested: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StagedBinarySnapshotEffect {
    relative_path: String,
    graph_v4_json: Vec<u8>,
}

/// Fully decoded authoritative state prepared off to the side before a recovery
/// is allowed to replace any live owner. Keeping this separate from
/// `SessionState` is the fail-closed boundary: a missing/corrupt sidecar leaves
/// the existing in-memory session byte-for-byte untouched.
struct StrictRecoveryState {
    graph: Graph,
    orchestrator: QueryOrchestrator,
    temporal: TemporalEngine,
    counterfactual: CounterfactualEngine,
    topology: TopologyAnalyzer,
    resonance: ResonanceEngine,
    plasticity: PlasticityEngine,
    ingest_roots: Vec<String>,
    antibodies: Vec<Antibody>,
    tremor_registry: TremorRegistry,
    trust_ledger: TrustLedger,
    calibration_table: m1nd_core::calibration::CalibrationTable,
    boot_memory: HashMap<String, BootMemoryEntry>,
    daemon_state: DaemonRuntimeState,
    daemon_alerts: Vec<DaemonAlert>,
    auto_ingest: AutoIngestState,
    document_cache: DocumentCacheState,
    document_artifacts: DocumentArtifactInventory,
    boot_kv_checkpoint_inventory: BootKvCheckpointInventoryV1,
}

// ---------------------------------------------------------------------------
// SessionState — all server state in one place
// Replaces: 03-MCP Section 1.1 server internal state
// ---------------------------------------------------------------------------

/// Server session state. Owns the graph and all engine instances.
/// Single instance shared across all agent connections.
///
/// Instance lifecycle authority is intentionally not part of the public state
/// surface; external readers cannot capture it before the brain actor starts.
///
/// ```compile_fail
/// use m1nd_mcp::server::{McpConfig, McpServer};
///
/// let state = McpServer::new(McpConfig::default())
///     .unwrap()
///     .into_session_state();
/// let _escaped_lifecycle_capability = state.instance;
/// ```
pub struct SessionState {
    /// Exact friendly-boot construction contract retained for diagnostics and
    /// process configuration. Strict recovery does not reconstruct through this
    /// config; it reuses the existing process-owned handles in place.
    pub(crate) boot_config: crate::server::McpConfig,
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
    /// Atomic sidecar containing both graph-bound co-change matrices.
    pub temporal_state_path: PathBuf,
    /// Path to the on-disk embedding cache (OPTIONAL `embed` feature). Derived
    /// from the runtime root; reused across warm boots and re-ingests.
    pub embeddings_cache_path: PathBuf,
    /// Per-agent session tracking.
    pub sessions: HashMap<String, AgentSession>,
    /// In-memory preview states for Ultra Edit phase 1.
    pub edit_previews: HashMap<String, EditPreviewState>,
    /// Staged two-phase transplants (A2): preview_id → full multi-file plan.
    /// Same in-memory/TTL discipline as `edit_previews`.
    pub transplant_previews: HashMap<String, TransplantPreviewState>,
    /// Lazily-built, graph-generation-fenced file lexical index for narrative
    /// seek. Rebuilt after ingest/mutation; never persisted or trusted across boot.
    pub(crate) seek_file_index: Option<SeekFileIndexCache>,

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
    /// True while this dispatch was routed by an EXPLICIT brain selector
    /// (REST `?brain=`, or the wire's own selected-brain route) rather than by
    /// the caller's own root. Request-scoped exactly like `caller_root`: the
    /// transport sets it before dispatch and restores it after.
    ///
    /// It exists for the authority-exclusive predicates that must never be
    /// satisfied by a selector (`GENESIS-INGEST-CONSUMERS-SPEC.md` §1.2
    /// SPEC-1g): a selector says WHICH brain to talk to, never that the caller
    /// legitimately inhabits that brain's root. `false` on every other path.
    pub explicit_brain_selector: bool,
    /// Dedicated runtime root for persisted sidecar state.
    pub runtime_root: PathBuf,
    /// F11-b: the owner-process naming facts (the runnerd announce registry + the
    /// OWNER runtime root where `runnerd.secret` lives), threaded in by the HTTP
    /// owner at boot — into the bound session and every hosted project brain.
    /// `None` on a stdio owner (no announce surface): `skeleton_candidate` with
    /// `naming:"auto"` then falls back to heuristic naming exactly as before.
    pub runnerd_naming: Option<crate::runnerd_owner::NamingRunnerHandle>,
    /// Registry + lease handle for this process instance. Crate-private because
    /// even `&SessionState` would otherwise expose a cloneable process capability
    /// across the actor ownership fence.
    pub(crate) instance: InstanceHandle,
    /// Optional live sink for apply_batch progress emission.
    pub apply_batch_progress_sink: Option<ApplyBatchProgressSink>,
    /// Optional live sink for `skeleton_candidate` scan-phase progress emission
    /// (docs/uml/scan-loading.md slice 2). Wired per-request by the HTTP owner
    /// while dispatching `skeleton_candidate`; `None` on every other path, so the
    /// scan then runs byte-identically and emits nothing.
    pub scan_progress_sink: Option<ScanProgressSink>,

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
    /// Durable per-verb call counters — the ONLY thing m1nd records about its
    /// own use, and deliberately the smallest thing that answers "which verbs
    /// are called, how often". Verb names and counts only; see
    /// `crate::verb_usage` before adding a field. Written by
    /// [`SessionState::record_verb_call`] from ONE seam
    /// (`server::dispatch_generic_tool`); read back through `report`.
    pub verb_usage: crate::verb_usage::VerbUsageLedger,
    /// Graph node count at session start.
    pub session_start_node_count: u32,
    /// Graph edge count at session start.
    pub session_start_edge_count: u64,
    /// Path to the legacy Boot KV compatibility tombstone. Writable owners
    /// retire it into Boot Config/L1GHT during initialization.
    pub boot_memory_path: PathBuf,
    /// Legacy hot cache (empty after migration; retained only for read-only
    /// attachment to a pre-migration runtime).
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
    /// Marks atomically consumed by the dispatch proof middleware and exposed
    /// only for the duration of that single synchronous physical-write call.
    /// Handlers re-check these immediately before publishing bytes to close the
    /// proof-check -> write TOCTOU window. Never persisted.
    pub active_proof_permits: HashMap<(String, String), ProofReadyMark>,
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
    /// Canonical universal bodies staged in memory until the checkpoint actor
    /// publishes their PRESENT/ABSENT decisions after CURRENT.
    pub(crate) document_artifacts: DocumentArtifactInventory,
    /// Result of boot-time agent-memory auto-load, surfaced verbatim in
    /// `session_handshake` (and thus `trust_selftest`). `None` = the auto-load
    /// did not run (no agent-memory dir yet); never hidden.
    pub agent_memory_boot: Option<serde_json::Value>,

    /// Validated at boot under the writer lease. The migration is immutable
    /// during a session, so checkpointing can include its fixed files and
    /// dynamic L1GHT working set without a racy filesystem rediscovery.
    boot_kv_checkpoint_inventory: BootKvCheckpointInventoryV1,

    /// Actor-owned candidate-first persistence fence. `Cell` lets granular
    /// `&self` persistence helpers turn a write into a staged intent marker.
    persistence_stage: std::cell::Cell<Option<PersistenceStageState>>,
    next_persistence_stage_id: u64,
    staged_binary_snapshot_effects: Vec<StagedBinarySnapshotEffect>,

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

/// Proof marks are intentionally short-lived and one-shot. Five minutes matches
/// the edit-preview OCC window while still forcing a fresh graph/disk read for a
/// later write.
pub const PROOF_READY_TTL_MS: u64 = 5 * 60 * 1000;

/// Budget Law (§C1.3.4) — how many ingest roots the binding fingerprint carries
/// inline. The fingerprint answers ONE question ("am I bound to the m1nd I think
/// I am?") and it rides on every `north`, so it must cost a fixed number of
/// bytes no matter how many roots the brain accumulates. Ten is the smallest
/// head that still shows the founding roots — the array is ordered oldest →
/// newest ([`crate::tools`] ingest tracking), so the head is both the identity-
/// bearing prefix and STABLE across writes, which is what makes cross-seam
/// fingerprint comparison (`compare_binding_fingerprint`) meaningful. The whole
/// array is served by `doctor` under `runtime_state.ingest_roots`.
pub const FINGERPRINT_INGEST_ROOTS_HEAD: usize = 10;

/// The canonical TT-INV-2 gap LABEL for a caller root that no project brain
/// covers while the medulla legitimately serves its cross-project doctrine
/// (TWO-TIER-BRAIN-PRD §9.5 · §10.4 rung 3). Doc-only until P1; this is now the
/// real symbol the medulla-only read fallback stamps on `reception`.
pub const PROJECT_BRAIN_ABSENT: &str = "project_brain_absent";

/// The one honest sentence for the `project_brain_absent` gap, authored ONCE so
/// every degraded read beat (north's `honest_gaps`) speaks it byte-equal. It
/// names the label, states what IS served (the medulla's cross-project doctrine,
/// legitimately) and what is NOT (code anchors for the caller's repo), and gives
/// the honest recovery — the SAME closed-bootstrap posture the write path uses
/// (never an invented `m1nd init` birth, which is unbuilt today).
pub const PROJECT_BRAIN_ABSENT_GAP: &str = "project_brain_absent — no project brain covers your caller root; the medulla's cross-project doctrine is served as a legitimate transversal feed, but no code anchors for your repo exist. Creating a project brain is unavailable until the typed bootstrap consumer is installed (TT-INV-2 · TWO-TIER-BRAIN-PRD §10.4 rung 3).";

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
/// Exact full source commit captured by build.rs. Unlike `BINARY_GIT_SHA`, this
/// value has no display suffix and can be compared byte-for-byte with Git HEAD.
pub const BINARY_BUILD_SOURCE_COMMIT: &str = env!("M1ND_BUILD_SOURCE_COMMIT");
/// Whether tracked/untracked source differed from that commit at build time.
/// A dirty build is useful for development but can never be release-coherent.
pub const BINARY_BUILD_SOURCE_DIRTY: &str = env!("M1ND_BUILD_SOURCE_DIRTY");

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

/// A path is a "memory sidecar" when it is an individual `.light.md` claim file
/// or the `agent-memory` runtime store directory itself — i.e. durable L1GHT
/// memory, NOT a code root. Used both to skip sidecars when resolving the repo
/// display name AND to keep the ingest write-path from minting a per-file ingest
/// root for every claim (Budget Law §C1.3.4: the store DIR is the one root, not
/// each sidecar). One definition, both call sites.
pub(crate) fn is_memory_sidecar(p: &str) -> bool {
    p.ends_with(".light.md")
        || std::path::Path::new(p)
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == "agent-memory")
            .unwrap_or(false)
}

/// The last path component of a filesystem root — the human name of a repo
/// ("/Users/<name>/m1nd" → "m1nd"). Separator-agnostic: splits on BOTH '/' and '\\'
/// so a Windows backslash path ("C:\\Users\\<name>\\m1nd" → "m1nd") names its repo
/// the same as a POSIX one. Trailing separators are tolerated; a rootless or
/// empty input returns the trimmed input unchanged (honest, never a panic).
/// Shared by the bound-brain display name and the project-brain listing so both
/// name a brain the same way. Mirrors the UI's `repoBasename` exactly.
pub(crate) fn basename_of(root: &str) -> String {
    let is_sep = |c: char| c == '/' || c == '\\';
    root.trim()
        .trim_end_matches(is_sep)
        .rsplit(is_sep)
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
        // Budget Law (§C1.3.4 "fixed-cost binding"): the fingerprint rides on
        // EVERY `north` — the verb doctrine makes every agent call it first — so
        // it may not carry a block that grows with the brain. Measured on the
        // live owner 2026-07-24: 380 roots = 25,907 bytes (~6.5k tokens) per
        // call. The array is head-truncated to a fixed cost; the omission is
        // DECLARED, never silent (honesty contract), the real total always
        // ships, and the pointer names the surface that serves the whole list.
        let omitted = self
            .ingest_roots
            .len()
            .saturating_sub(FINGERPRINT_INGEST_ROOTS_HEAD);
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
            "ingest_roots": self.ingest_roots.iter().take(FINGERPRINT_INGEST_ROOTS_HEAD).collect::<Vec<_>>(),
            "ingest_root_count": self.ingest_roots.len(),
            "ingest_roots_truncated": omitted > 0,
            "ingest_roots_omitted": omitted,
            "ingest_roots_full_surface": if omitted > 0 {
                serde_json::Value::String(
                    "doctor -> runtime_state.ingest_roots (the whole array; this block carries the oldest-first head only)".into(),
                )
            } else {
                serde_json::Value::Null
            },
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
        // Budget Law (§C1.3.4 "no duplicate serialization"): a north packet embeds
        // both this `graph_state` and the fingerprint, so listing the roots array in
        // both duplicated it byte-identical and blew the packet budget. Here we carry
        // only the COUNT. The fingerprint no longer holds the canonical array either
        // — it carries a fixed head plus the same count and an explicit truncation
        // declaration (see `binding_fingerprint`); the whole array is served by
        // `doctor` under `runtime_state.ingest_roots`.
        serde_json::json!({
            "node_count": graph.num_nodes(),
            "edge_count": graph.num_edges(),
            "finalized": graph.finalized,
            "graph_generation": self.graph_generation,
            "plasticity_generation": self.plasticity_generation,
            "cache_generation": self.cache_generation,
            "ingest_root_count": self.ingest_roots.len(),
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
    /// workspace / ingest roots — the live Antigravity/project-b failure, made loud.
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

        // Mismatch: the bound graph does NOT cover the caller's repo. Say so, but
        // never advertise the internal bootstrap seam as a public repair. The
        // generic route is POSITIVE_SOVEREIGN and no exact typed G2/G3 consumer
        // exists yet.
        // Base mismatch block. Its shape is a CONTRACT with two consumers that
        // read it back: `human_view` (reads `honest`/`caller_root`/`bound_workspace`
        // verbatim into the S3 card) and `mcp_http::enrich_reception_with_roster`
        // (gates on `match == "caller_root_mismatch"` and rewrites the
        // `bootstrap_unavailable` option). Every field below stays in place; the
        // medulla enrichment is strictly ADDITIVE.
        let mut block = serde_json::json!({
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
                    "action": "bootstrap_unavailable",
                    "code": "brain_bootstrap_consumer_not_installed",
                    "note": "creating or rebinding a project brain is unavailable until an exact typed G2/G3 bootstrap consumer is installed; no mutation was attempted"
                }
            ]
        });

        // P1 medulla-only read fallback (TWO-TIER-BRAIN-PRD §9.5 · §10.4 rung 3 ·
        // TT-INV-2). When the bound store is the MEDULLA, a mismatch is NOT a
        // misbinding to distrust wholesale — it is the brainless-root case the
        // canon names: the medulla's cross-project doctrine + promoted memory is
        // served as a LEGITIMATE transversal source, while no project brain maps
        // the caller's repo. Label it `project_brain_absent` and reframe the
        // continue option so the agent trusts the DOCTRINE and distrusts only the
        // CODE answers (the `honest` line stays byte-exact — it speaks of the CODE
        // graph, which genuinely does not cover the repo, and the human_view card
        // pins it). A project-brain mismatch (a real misbind) keeps the plain
        // "don't trust" block below untouched.
        if self.is_medulla_store() {
            if let Some(obj) = block.as_object_mut() {
                obj.insert(PROJECT_BRAIN_ABSENT.to_string(), serde_json::json!(true));
                obj.insert("medulla_served".to_string(), serde_json::json!(true));
                if let Some(first) = obj
                    .get_mut("options")
                    .and_then(|o| o.as_array_mut())
                    .and_then(|opts| opts.first_mut())
                    .and_then(|o| o.as_object_mut())
                {
                    first.insert(
                        "note".to_string(),
                        serde_json::json!(
                            "the medulla's promoted doctrine + memory is served as a legitimate cross-project source; treat only CODE answers as NOT covering your repo — verify those against local files"
                        ),
                    );
                }
            }
        }
        Some(block)
    }

    /// The "brainless root" condition (MEDULLA-PRD §2.3 S2): the caller's resolved
    /// root is KNOWN, THIS store is the medulla, and the medulla does not cover
    /// that root. It is the SAME triple the WRITE path refuses inline
    /// (`light_author_handlers` `brainless_root`, left untouched); on the READ path
    /// (`north`) it is the medulla-only fallback — serve the medulla's
    /// cross-project doctrine as a legitimate feed, cut the foreign code anchors,
    /// and label `project_brain_absent` (TWO-TIER-BRAIN-PRD §9.5 · §10.4 rung 3).
    /// `false` on an unknown caller root (absent ≠ wrong) and on a project brain
    /// (it owns its own answers).
    pub fn caller_root_is_brainless(&self) -> bool {
        if !self.is_medulla_store() {
            return false;
        }
        match self.caller_root.as_deref() {
            Some(root) => !self.covers_root(root),
            None => false,
        }
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

    /// The exact-root predicate — `covers_root`'s AUTHORITY-EXCLUSIVE sibling
    /// (`docs/GENESIS-INGEST-CONSUMERS-SPEC.md` §1.2, verdict RC-1).
    ///
    /// `covers_root` above is a PREFIX test by design, and it stays one: it
    /// answers "may this brain legitimately serve that caller's questions?", and
    /// a caller deep inside a repo is legitimately served by that repo's brain.
    /// It is the wrong question for a WRITE. `<root>/m1nd-ui` is covered by the
    /// brain at `<root>`, so reusing the prefix test here would let any
    /// subdirectory rewrite the whole repo's graph — the verdict's kill-shot.
    ///
    /// So this one asks a different question, and answers it with EQUALITY of
    /// `canonical_key`s, never a prefix and never a textual comparison:
    ///
    /// - `canonical_key` resolves symlinks and the `/tmp` → `/private/tmp` alias
    ///   (spec R-J), so two spellings of one directory reach ONE decision;
    /// - it FALLS BACK to the raw string when a path does not resolve
    ///   (`project_brains.rs`), which alone would let two textually-equal
    ///   NONEXISTENT paths "match" — so unresolvable paths are refused here
    ///   explicitly, before any comparison (SPEC-1b);
    /// - an explicit brain selector is refused outright: `?brain=` says WHICH
    ///   brain to talk to, never that the caller inhabits that brain's root
    ///   (SPEC-1g). It is folded into `refresh_root_not_exact` on purpose, so a
    ///   selector cannot even be DISTINGUISHED from the plain miss — it must buy
    ///   nothing at all, not even information.
    ///
    /// `Ok(canonical_root)` is the canonical key of the declared root the caller
    /// exactly inhabits. `Err(code)` is a stable refusal code.
    pub fn exact_declared_root(&self, caller_root: &str) -> Result<String, &'static str> {
        use crate::project_brains::ProjectBrainRegistry;

        if self.explicit_brain_selector {
            return Err("refresh_root_not_exact");
        }
        let trimmed = caller_root.trim().trim_end_matches('/');
        if trimmed.is_empty() || !std::path::Path::new(trimmed).exists() {
            return Err("refresh_root_unresolvable");
        }
        let caller_key = ProjectBrainRegistry::canonical_key(trimmed);

        let declared = self
            .workspace_root
            .iter()
            .chain(self.ingest_roots.iter())
            .filter(|root| std::path::Path::new(root.trim().trim_end_matches('/')).exists())
            .map(|root| ProjectBrainRegistry::canonical_key(root))
            .any(|declared| declared == caller_key);

        if declared {
            Ok(caller_key)
        } else {
            Err("refresh_root_not_exact")
        }
    }

    /// Every declared root this brain holds, canonicalized — the list the
    /// exact-root predicate compares against, surfaced so a refusal can name it.
    pub fn declared_roots_canonical(&self) -> Vec<String> {
        use crate::project_brains::ProjectBrainRegistry;

        let mut roots: Vec<String> = self
            .workspace_root
            .iter()
            .chain(self.ingest_roots.iter())
            .map(|root| ProjectBrainRegistry::canonical_key(root))
            .collect();
        roots.sort();
        roots.dedup();
        roots
    }

    /// The brain's real PROJECT root — the repo it maps, NOT its runtime sidecar.
    ///
    /// The Hall (HUMAN-LAYER-PRD §4A.3) must name brains by their project, never
    /// by plumbing: the bound dev graph's `workspace_root` is its `agent-memory`
    /// sidecar dir (inferred `graph_path_parent`), so naming from it leaks
    /// "agent-memory"/"claude". The true project is the primary *code* ingest
    /// root (e.g. `<repo-root>`). Precedence, mirroring
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
    /// [`project_root_display`] — "m1nd", "project-b" — never a runtime dir
    /// name ("claude") nor "agent-memory". `None` when the brain has no roots.
    pub fn display_name(&self) -> Option<String> {
        self.project_root_display().map(|root| basename_of(&root))
    }

    /// The display name ONLY when it comes from a REAL code root (resolver cases
    /// 1-2: an ingest root or a repo workspace) — `None` on the plumbing
    /// fallbacks (cases 3-4), where the "display" is a runtime dir. Guards that
    /// arm on brain identity (the mission_post brain guard) use THIS, so a bare
    /// session never refuses over plumbing.
    pub fn code_root_display_name(&self) -> Option<String> {
        self.code_root_path().map(|root| basename_of(&root))
    }

    /// The PATH of the real code root (the same cases 1-2 as
    /// [`code_root_display_name`]): a non-sidecar ingest root, else a workspace
    /// that is a .git repo. `None` on plumbing — a hosted brain's raw
    /// `workspace_root` is its STORE dir (where memory sidecars live), so the
    /// repo-file-list surfaces (reconcile, skeleton_candidate) MUST use this,
    /// never `workspace_root` directly: the first virgin-repo scan listed the
    /// brain's `.light.md` memories as the repo because of exactly that.
    pub fn code_root_path(&self) -> Option<String> {
        for root in &self.ingest_roots {
            if !is_memory_sidecar(root) && std::path::Path::new(root).is_dir() {
                return Some(root.clone());
            }
        }
        if let Some(ws) = self.workspace_root.as_deref() {
            if !is_memory_sidecar(ws) && std::path::Path::new(ws).join(".git").exists() {
                return Some(ws.to_string());
            }
        }
        None
    }

    /// #326-family auto-heal (load/resolve seam). A prior memorize / agent-memory
    /// merge could DEMOTE `workspace_root` onto the `agent-memory` store dir (the
    /// write-path bug fixed in `handle_ingest`); brains already carrying that
    /// flipped state on disk answer the REST seam with a store-dir workspace_root,
    /// so every `caller_root` comparison mis-matches. When `workspace_root` is a
    /// memory sidecar BUT a real code root is still resolvable from the ingest
    /// roots, repair it to the code root with one honest log line. Idempotent and
    /// self-limiting: a no-op when `workspace_root` is already a code root or when
    /// no code root is resolvable (a genuine pure-memory / medulla store keeps its
    /// sidecar workspace_root). This de-flips the corrupted bound owner on its next
    /// boot without any manual data surgery.
    pub fn heal_workspace_root(&mut self) {
        let flipped = self
            .workspace_root
            .as_deref()
            .map(is_memory_sidecar)
            .unwrap_or(false);
        if !flipped {
            return;
        }
        // `code_root_path` never returns a sidecar: in the flipped state it falls
        // through to the first non-sidecar ingest root that is a real directory.
        if let Some(code_root) = self.code_root_path() {
            if self.workspace_root.as_deref() != Some(code_root.as_str()) {
                let from = self.workspace_root.clone().unwrap_or_default();
                eprintln!(
                    "[m1nd] healed workspace_root: {} -> {} (the #326 family)",
                    from, code_root
                );
                self.workspace_root = Some(code_root);
            }
        }
    }

    /// True when THIS store is the medulla — the owner's own memory-of-doctrine
    /// store, not a per-project brain (MEDULLA-PRD §4.1: the tier IS the directory).
    ///
    /// A project brain is born through [`ProjectBrainRegistry::boot_store`], which
    /// stamps `workspace_root_source = "project_brain_manifest"` — the one honest
    /// signal that a session is a routed per-project store rather than the bound
    /// owner. Everything else (the bound dev graph today; the owner's own store)
    /// is the medulla store: its `agent-memory/` dir holds `promoted` /
    /// doctrine-born claims (post-migration) and is what every session's default
    /// beat reads beside its own project memory.
    pub fn is_medulla_store(&self) -> bool {
        self.workspace_root_source.as_deref() != Some("project_brain_manifest")
    }

    /// The `Origin-Brain` label to stamp on a claim born in THIS store
    /// (MEDULLA-PRD §6 · §3.3 frontmatter grammar):
    /// - a per-project brain → its project root (`/path/to/repo`), the WHERE-born;
    /// - the medulla store → the literal `medulla` (doctrine-born, no project origin).
    ///
    /// This is metadata, not a security boundary — it is rendered everywhere so
    /// promotion/recall can say which brain a claim came from. Absent on legacy
    /// files means "unknown", never faked (TT-INV-2 / MED-INV-4).
    pub fn origin_brain(&self) -> String {
        if self.is_medulla_store() {
            "medulla".to_string()
        } else {
            // A project brain: its project root is the origin. Prefer the real
            // repo root over the runtime sidecar, mirroring display naming.
            self.project_root_display()
                .or_else(|| self.workspace_root.clone())
                .unwrap_or_else(|| "medulla".to_string())
        }
    }

    /// How many durable L1GHT memory claims exist on disk in this brain's store
    /// (`<runtime_root>/agent-memory/*.light.md`). This is the ground-truth count
    /// of the memory store itself, independent of whether recall surfaced any of
    /// them for a given task — so a beat that finds no task-relevant hit can still
    /// tell the truth ("the store HAS N memories") instead of the false absence
    /// "no durable memory yet". Cheap dir read; `0` when the store dir is absent.
    pub fn light_memory_count(&self) -> usize {
        let dir = self.runtime_root.join("agent-memory");
        std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().to_string_lossy().ends_with(".light.md"))
            .count()
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
    ///
    /// Raw session construction is an owner-internal authority seam. External
    /// crates must enter through the supported MCP/HTTP transports instead.
    ///
    /// ```compile_fail,E0624
    /// use m1nd_core::{domain::DomainConfig, graph::Graph};
    /// use m1nd_mcp::{server::McpConfig, session::SessionState};
    ///
    /// let config = McpConfig::default();
    /// let _state = SessionState::initialize(Graph::new(), &config, DomainConfig::code());
    /// ```
    pub(crate) fn initialize(
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
        let mut orchestrator = QueryOrchestrator::build_with_cache(
            &graph,
            Some(&embeddings_cache_path),
            !config.read_only,
        )?;
        let mut temporal = TemporalEngine::build(&graph)?;
        let temporal_state_path = runtime_root.join(crate::temporal_state::TEMPORAL_STATE_FILE);
        match crate::temporal_state::load_temporal_state(&temporal_state_path, &graph) {
            Ok(Some((primary, orchestrator_matrix))) => {
                temporal.co_change = primary;
                orchestrator.temporal.co_change = orchestrator_matrix;
            }
            Ok(None) => {}
            // A co-change matrix is indexed by the graph it was learned on, so a
            // sidecar bound to a different graph must never be adopted. Refusing
            // the whole boot over it is the wrong consequence: it takes every MCP
            // tool down and leaves hand-deleting the file as the only recovery.
            // Drop it like every other stale sidecar on this path and relearn.
            Err(M1ndError::SchemaDrift { reason }) => {
                eprintln!(
                    "[m1nd] WARNING: co-change state at {} does not match the loaded graph ({reason}); continuing without it — the matrix will be relearned and rewritten on the next persist",
                    temporal_state_path.display()
                );
            }
            Err(error) => return Err(error),
        }
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
        } else {
            // M1ND-10 G6: retire the arbitrary Boot KV before serving any
            // request. The migration is journaled/idempotent; a corrupt or
            // incomplete plan fails boot rather than silently reviving dual
            // writers.
            crate::boot_kv_migration::migrate_boot_kv(&runtime_root)?;
        }
        let boot_kv_checkpoint_inventory =
            crate::boot_kv_migration::checkpoint_inventory(&runtime_root)?;
        // Best-effort, non-blocking sweep of dead lease/instance entries at every
        // boot. The daemon-tick GC only runs when the daemon is active, so dead
        // entries otherwise leak unbounded (~25k observed live). Detached on its
        // own thread so it can NEVER delay the `initialize`/`tools/list`
        // handshake regardless of registry size; our own live-pid entry (just
        // written by `acquire_with_mode`) is never touched. Handle dropped:
        // fire-and-forget.
        let _ = crate::instance_registry::spawn_boot_gc(instance.registry_root());
        // P1 (ORGANISM-INSIDE): reclaim orphan presence sidecars left by sessions
        // that were live when the owner last restarted (the verdict's flagged
        // risk: boot-GC must sweep stale sidecars post-restart). Detached, error-
        // swallowing, never able to delay boot. Read-time filtering already hides
        // stale presences; this only reclaims their files.
        {
            let registry_root = instance.registry_root();
            let _ = std::thread::Builder::new()
                .name("m1nd-presence-boot-gc".into())
                .spawn(move || {
                    let _ = crate::presence::gc_stale(&registry_root);
                });
        }
        let ingest_roots = Self::load_ingest_roots(&config.graph_source);
        let document_cache = load_document_cache(&runtime_root);
        // Compatibility is intentionally confined to friendly boot. A legacy
        // runtime without the v1 inventory is reconstructed in memory from its
        // exact current bodies; strict checkpoint recovery requires the sidecar.
        let document_artifacts =
            load_document_artifact_inventory_friendly(&runtime_root, &document_cache)?;

        let mut state = Self {
            boot_config: config.clone(),
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
            temporal_state_path,
            embeddings_cache_path,
            sessions: HashMap::new(),
            edit_previews: HashMap::new(),
            transplant_previews: HashMap::new(),
            seek_file_index: None,
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
            explicit_brain_selector: false,
            runtime_root: runtime_root.clone(),
            // Threaded in by the HTTP owner at boot; None on stdio (no announce).
            runnerd_naming: None,
            instance,
            apply_batch_progress_sink: None,
            scan_progress_sink: None,
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
            verb_usage: crate::verb_usage::VerbUsageLedger::load(&runtime_root),
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
            active_proof_permits: HashMap::new(),
            flagged_findings: HashMap::new(),
            auto_ingest: AutoIngestState::load(&runtime_root),
            document_cache,
            document_artifacts,
            agent_memory_boot: None,
            boot_kv_checkpoint_inventory,
            persistence_stage: std::cell::Cell::new(None),
            next_persistence_stage_id: 1,
            staged_binary_snapshot_effects: Vec::new(),
            read_only: config.read_only,
            read_only_persist_logged: std::cell::Cell::new(false),
        };
        // #326-family auto-heal at the boot/load seam: if a prior memorize left
        // `workspace_root` demoted onto the agent-memory store dir while a real code
        // root survives in the ingest roots, repair it before the session serves a
        // single request (de-flips the corrupted bound owner on its next boot).
        state.heal_workspace_root();
        Ok(state)
    }

    fn read_required_recovery_file(path: &Path, logical_name: &str) -> M1ndResult<Vec<u8>> {
        crate::checkpoint_store::read_regular_checkpoint_input(path).map_err(|error| {
            M1ndError::CorruptState {
                reason: format!(
                    "strict recovery could not read required {logical_name} '{}': {error}",
                    path.display()
                ),
            }
        })
    }

    /// Reject fields/defaults that a compatibility-oriented serde decoder would
    /// otherwise silently erase. Object key order and insignificant JSON
    /// whitespace remain irrelevant, but the decoded current-schema projection
    /// must be semantically identical to the checkpoint payload.
    fn verify_current_json_projection(
        logical_name: &str,
        observed: &[u8],
        projected: &[u8],
    ) -> M1ndResult<()> {
        let observed: Value = serde_json::from_slice(observed)?;
        let projected: Value = serde_json::from_slice(projected)?;
        if observed != projected {
            return Err(M1ndError::CorruptState {
                reason: format!(
                    "strict recovery refused non-current or lossy {logical_name} checkpoint payload"
                ),
            });
        }
        Ok(())
    }

    /// Decode every required authoritative owner before touching `self`. This is
    /// intentionally not implemented through `initialize`: recovery must not
    /// acquire a discovery handle, refresh the registry, run migration/GC, load
    /// an embedding cache, create directories, or substitute friendly defaults.
    fn prepare_strict_recovery_state(&self) -> M1ndResult<StrictRecoveryState> {
        let ingest_roots_path = self
            .graph_path
            .parent()
            .unwrap_or(&self.runtime_root)
            .join("ingest_roots.json");
        let auto_ingest_path = self.runtime_root.join("auto_ingest_state.json");
        let document_cache_path = self.runtime_root.join("document_cache_index.json");
        let document_artifact_inventory_path =
            crate::universal_docs::document_artifact_inventory_path(&self.runtime_root);

        // Read the complete fixed working set first. Required candidate files
        // are never allowed to decay into defaults during authoritative recovery.
        let graph_bytes = Self::read_required_recovery_file(&self.graph_path, "graph_snapshot")?;
        let ingest_roots_bytes =
            Self::read_required_recovery_file(&ingest_roots_path, "ingest_roots")?;
        let plasticity_bytes =
            Self::read_required_recovery_file(&self.plasticity_path, "plasticity_state")?;
        let antibodies_bytes =
            Self::read_required_recovery_file(&self.antibodies_path, "antibodies")?;
        let tremor_bytes = Self::read_required_recovery_file(&self.tremor_path, "tremor_state")?;
        let trust_bytes = Self::read_required_recovery_file(&self.trust_path, "trust_state")?;
        let calibration_bytes =
            Self::read_required_recovery_file(&self.calibration_path, "calibration_state")?;
        let temporal_bytes =
            Self::read_required_recovery_file(&self.temporal_state_path, "temporal_state")?;
        let daemon_state_bytes =
            Self::read_required_recovery_file(&self.daemon_state_path, "daemon_state")?;
        let daemon_alerts_bytes =
            Self::read_required_recovery_file(&self.daemon_alerts_path, "daemon_alerts")?;
        let auto_ingest_bytes =
            Self::read_required_recovery_file(&auto_ingest_path, "auto_ingest_state")?;
        let document_cache_bytes =
            Self::read_required_recovery_file(&document_cache_path, "document_cache_index")?;
        let document_artifact_inventory_bytes = Self::read_required_recovery_file(
            &document_artifact_inventory_path,
            "document_artifact_inventory",
        )?;

        let mut graph = m1nd_core::snapshot::decode_graph_json(&graph_bytes)?;
        if !graph.finalized && graph.num_nodes() > 0 {
            graph.finalize()?;
        }

        let plasticity_states =
            m1nd_core::plasticity::decode_plasticity_state_json(&plasticity_bytes)?;
        let expected_synapses = graph.csr.num_edges();
        if plasticity_states.len() != expected_synapses
            || plasticity_states
                .iter()
                .any(|state| state.direction.is_none() || state.inhibitory.is_none())
        {
            return Err(M1ndError::CorruptState {
                reason: format!(
                    "strict recovery requires one current-identity plasticity row per CSR edge: rows={}, edges={expected_synapses}",
                    plasticity_states.len()
                ),
            });
        }
        let mut plasticity =
            PlasticityEngine::new(&graph, m1nd_core::plasticity::PlasticityConfig::default());
        let applied = plasticity.import_state(&mut graph, &plasticity_states)? as usize;
        if applied != expected_synapses {
            return Err(M1ndError::CorruptState {
                reason: format!(
                    "strict recovery applied {applied} plasticity rows for {expected_synapses} CSR edges"
                ),
            });
        }
        let projected_plasticity =
            m1nd_core::plasticity::encode_plasticity_state_json(&plasticity.export_state(&graph)?)?;
        Self::verify_current_json_projection(
            "plasticity_state",
            &plasticity_bytes,
            &projected_plasticity,
        )?;
        Self::verify_current_json_projection(
            "graph_snapshot",
            &graph_bytes,
            &m1nd_core::snapshot::encode_graph_json(&graph)?,
        )?;

        // Build graph-derived engines without an embedding-cache path. `build`
        // is the pure in-memory constructor; it neither reads nor writes working
        // files. Only the two explicitly checkpointed temporal matrices replace
        // their derived bootstrap values.
        let mut orchestrator = QueryOrchestrator::build(&graph)?;
        let orchestrator_applied = orchestrator
            .plasticity
            .import_state(&mut graph, &plasticity_states)?
            as usize;
        if orchestrator_applied != expected_synapses {
            return Err(M1ndError::CorruptState {
                reason: format!(
                    "strict recovery applied {orchestrator_applied} orchestrator plasticity rows for {expected_synapses} CSR edges"
                ),
            });
        }
        let mut temporal = TemporalEngine::build(&graph)?;
        let (primary_temporal, orchestrator_temporal) =
            crate::temporal_state::decode_temporal_state(&temporal_bytes, &graph)?;
        temporal.co_change = primary_temporal;
        orchestrator.temporal.co_change = orchestrator_temporal;
        Self::verify_current_json_projection(
            "temporal_state",
            &temporal_bytes,
            &crate::temporal_state::encode_temporal_state(
                &graph,
                &temporal.co_change,
                &orchestrator.temporal.co_change,
            )?,
        )?;

        let ingest_roots: Vec<String> = serde_json::from_slice(&ingest_roots_bytes)?;
        Self::verify_current_json_projection(
            "ingest_roots",
            &ingest_roots_bytes,
            &canonical_json_bytes(&ingest_roots)?,
        )?;

        let antibodies = m1nd_core::antibody::decode_antibodies_json(&antibodies_bytes)?;
        Self::verify_current_json_projection(
            "antibodies",
            &antibodies_bytes,
            &m1nd_core::antibody::encode_antibodies_json(&antibodies)?,
        )?;
        let tremor_registry = m1nd_core::tremor::decode_tremor_state_json(&tremor_bytes)?;
        Self::verify_current_json_projection(
            "tremor_state",
            &tremor_bytes,
            &m1nd_core::tremor::encode_tremor_state_json(&tremor_registry)?,
        )?;
        let trust_ledger = m1nd_core::trust::decode_trust_state_json(&trust_bytes)?;
        Self::verify_current_json_projection(
            "trust_state",
            &trust_bytes,
            &m1nd_core::trust::encode_trust_state_json(&trust_ledger)?,
        )?;
        let calibration_table =
            m1nd_core::calibration::decode_calibration_state_json(&calibration_bytes)?;
        Self::verify_current_json_projection(
            "calibration_state",
            &calibration_bytes,
            &m1nd_core::calibration::encode_calibration_state_json(&calibration_table)?,
        )?;

        let daemon_state: DaemonRuntimeState = serde_json::from_slice(&daemon_state_bytes)?;
        if daemon_state
            .last_tick_duration_ms
            .is_some_and(|value| !value.is_finite())
        {
            return Err(M1ndError::CorruptState {
                reason: "strict recovery daemon state has a non-finite tick duration".into(),
            });
        }
        Self::verify_current_json_projection(
            "daemon_state",
            &daemon_state_bytes,
            &canonical_json_bytes(&daemon_state)?,
        )?;
        let daemon_alerts: Vec<DaemonAlert> = serde_json::from_slice(&daemon_alerts_bytes)?;
        if daemon_alerts
            .iter()
            .any(|alert| !alert.confidence.is_finite())
        {
            return Err(M1ndError::CorruptState {
                reason: "strict recovery daemon alerts have a non-finite confidence".into(),
            });
        }
        Self::verify_current_json_projection(
            "daemon_alerts",
            &daemon_alerts_bytes,
            &canonical_json_bytes(&daemon_alerts)?,
        )?;

        // AutoIngest's compatibility loader is private to its module. Fence it
        // with a no-follow read before and after, then demand an exact semantic
        // round-trip through its canonical checkpoint encoder. Any missing,
        // malformed, unknown, defaulted, or concurrently replaced payload fails.
        let auto_ingest = AutoIngestState::load(&self.runtime_root);
        let auto_ingest_after =
            Self::read_required_recovery_file(&auto_ingest_path, "auto_ingest_state")?;
        if auto_ingest_bytes != auto_ingest_after {
            return Err(M1ndError::CorruptState {
                reason: "auto-ingest checkpoint changed during strict recovery".into(),
            });
        }
        Self::verify_current_json_projection(
            "auto_ingest_state",
            &auto_ingest_bytes,
            &auto_ingest.encode_checkpoint_state()?,
        )?;

        let document_cache: DocumentCacheState = serde_json::from_slice(&document_cache_bytes)?;
        Self::verify_current_json_projection(
            "document_cache_index",
            &document_cache_bytes,
            &crate::universal_docs::encode_document_cache(&document_cache)?,
        )?;
        let document_artifacts = crate::universal_docs::decode_document_artifact_inventory_strict(
            &self.runtime_root,
            &document_cache,
            &document_artifact_inventory_bytes,
        )?;

        // This function only reads and validates the exact fixed/dynamic Boot KV
        // ownership set. It deliberately does not invoke the migration writer.
        let boot_kv_checkpoint_inventory =
            crate::boot_kv_migration::checkpoint_inventory(&self.runtime_root)?;
        let boot_memory = match boot_kv_checkpoint_inventory
            .fixed_file(crate::boot_kv_migration::LEGACY_BOOT_KV_FILE)
            .ok_or_else(|| M1ndError::CorruptState {
                reason: "strict Boot KV inventory omitted the legacy fixed path".into(),
            })? {
            Some(bytes) => serde_json::from_slice::<BootMemoryState>(bytes)?.entries,
            None => HashMap::new(),
        };

        Ok(StrictRecoveryState {
            graph,
            orchestrator,
            temporal,
            counterfactual: CounterfactualEngine::with_defaults(),
            topology: TopologyAnalyzer::with_defaults(),
            resonance: ResonanceEngine::with_defaults(),
            plasticity,
            ingest_roots,
            antibodies,
            tremor_registry,
            trust_ledger,
            calibration_table,
            boot_memory,
            daemon_state,
            daemon_alerts,
            auto_ingest,
            document_cache,
            document_artifacts,
            boot_kv_checkpoint_inventory,
        })
    }

    /// Rebuild this session from canonical working files after the brain actor
    /// restored a validated checkpoint. All durable owners are decoded before
    /// the swap; process-owned handles/paths/config remain on the existing
    /// session and recovery performs no filesystem write.
    pub(crate) fn reload_authoritative_from_disk(
        &mut self,
        preserve_process_state: bool,
    ) -> M1ndResult<()> {
        let recovered = self.prepare_strict_recovery_state()?;

        self.graph = Arc::new(parking_lot::RwLock::new(recovered.graph));
        self.orchestrator = recovered.orchestrator;
        self.temporal = recovered.temporal;
        self.counterfactual = recovered.counterfactual;
        self.topology = recovered.topology;
        self.resonance = recovered.resonance;
        self.plasticity = recovered.plasticity;
        self.ingest_roots = recovered.ingest_roots;
        self.antibodies = recovered.antibodies;
        self.tremor_registry = recovered.tremor_registry;
        self.trust_ledger = recovered.trust_ledger;
        self.calibration_table = recovered.calibration_table;
        self.boot_memory = recovered.boot_memory;
        // Recovery preserves the exact explicitly checkpointed daemon payload.
        // Friendly boot sanitization is intentionally not applied here: changing
        // these bytes would make the working-set digest unverifiable.
        self.daemon_state = recovered.daemon_state;
        self.daemon_alerts = recovered.daemon_alerts;
        self.auto_ingest = recovered.auto_ingest;
        self.document_cache = recovered.document_cache;
        self.document_artifacts = recovered.document_artifacts;
        self.boot_kv_checkpoint_inventory = recovered.boot_kv_checkpoint_inventory;

        // These values are process-only/derived and are never authoritative
        // checkpoint owners. A stale graph index, one-call proof capability, or
        // abandoned persistence capability must not cross the recovery fence.
        self.seek_file_index = None;
        self.active_proof_permits.clear();
        self.persistence_stage.set(None);
        self.staged_binary_snapshot_effects.clear();
        self.read_only_persist_logged.set(false);

        if !preserve_process_state {
            self.sessions.clear();
            self.edit_previews.clear();
            self.graph_generation = 0;
            self.plasticity_generation = 0;
            self.cache_generation = 0;
            self.perspectives.clear();
            self.locks.clear();
            self.perspective_counter.clear();
            self.lock_counter.clear();
            self.pending_watcher_events.clear();
            self.query_log.clear();
            self.session_start_node_count = 0;
            self.session_start_edge_count = 0;
            self.file_inventory.clear();
            self.coverage_sessions.clear();
            self.proof_ready.clear();
            self.flagged_findings.clear();
            self.queries_processed = 0;
            self.start_time = Instant::now();
            self.last_persist_time = None;
            self.last_antibody_scan_generation = 0;
            self.agent_memory_boot = None;
        }
        Ok(())
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

    /// Enter actor-owned candidate-first mode. While this capability is live,
    /// every SessionState/AutoIngest persistence choke point records intent and
    /// returns success without touching canonical working files.
    pub(crate) fn begin_checkpoint_staging(&mut self) -> M1ndResult<CheckpointPersistenceStage> {
        if let Some(active) = self.persistence_stage.get() {
            return Err(M1ndError::PersistenceFailed(format!(
                "checkpoint persistence transaction {} is already active",
                active.id
            )));
        }
        if !self.staged_binary_snapshot_effects.is_empty() {
            return Err(M1ndError::PersistenceFailed(
                "unresolved staged binary snapshot effects block a new checkpoint transaction"
                    .into(),
            ));
        }
        let id = self.next_persistence_stage_id;
        self.next_persistence_stage_id =
            self.next_persistence_stage_id
                .checked_add(1)
                .ok_or_else(|| {
                    M1ndError::PersistenceFailed(
                        "checkpoint persistence transaction id exhausted".into(),
                    )
                })?;
        self.auto_ingest.begin_checkpoint_staging(id)?;
        self.persistence_stage.set(Some(PersistenceStageState {
            id,
            persist_requested: false,
        }));
        Ok(CheckpointPersistenceStage { id })
    }

    fn verify_checkpoint_stage(&self, stage: &CheckpointPersistenceStage) -> M1ndResult<()> {
        let active = self.persistence_stage.get();
        if active.map(|value| value.id) != Some(stage.id) {
            return Err(M1ndError::PersistenceFailed(format!(
                "checkpoint persistence staging token mismatch: active={:?}, observed={}",
                active.map(|value| value.id),
                stage.id
            )));
        }
        self.auto_ingest.verify_checkpoint_staging(stage.id)
    }

    fn note_staged_persist(&self) -> bool {
        let Some(mut stage) = self.persistence_stage.get() else {
            return false;
        };
        stage.persist_requested = true;
        self.persistence_stage.set(Some(stage));
        true
    }

    /// Declare that a durable sidecar owner changed in a way only the checkpoint
    /// inventory carries, without demanding an immediate whole-brain write.
    ///
    /// This is the floor for a verb that is NOT classified a mutation but still
    /// dirties durable state (an antibody scan bumping match counters, a document
    /// verb refreshing its cache row). Without it the drift is invisible: the
    /// actor's witness only sees graph structure and session generations, no
    /// persist flag is raised, the staged-persist debounce never advances, and
    /// the change survives only until the process dies. With it the turn joins the
    /// debounce and is flushed by it, by the next real mutation, or by the
    /// shutdown checkpoint.
    ///
    /// Outside an actor stage this is deliberately a no-op — those owners have no
    /// eager writer of their own anyway, exactly as before.
    ///
    /// Every SHIPPED caller runs inside an actor stage. A `&mut SessionState` is
    /// reachable only through `BrainSessionCell::checkout`, whose call sites are
    /// the five actor turn primitives — `read_snapshot`, `execute`,
    /// `execute_with_checkpoint_ack`, `commit`, `checkpoint_current` — each of
    /// which opens the stage before the callback runs, plus actor startup, which
    /// runs no verb. Every transport seam dispatches inside one of the five, and
    /// so do the pull-based `auto_ingest::tick` and the owner daemon loop, whose
    /// every branch is wrapped in `actor_execute`. The drift is therefore never
    /// dropped in production. But it IS silent, and a future caller from boot, a
    /// CLI path, or a spawned task would lose its drift with no alarm.
    ///
    /// That guard cannot live here as a runtime assert. A pre-actor session and a
    /// session the actor simply has not staged yet are the SAME shape at this
    /// level, and the unstaged shape is legitimate and tested: with a stage open
    /// both `Self::persist` and `AutoIngestState::persist` record intent instead
    /// of writing, so the tests that drive these handlers against a bare
    /// `SessionState` and then reload from disk are exercising exactly the
    /// unstaged fallback. Asserting a stage here fails those, and would say
    /// nothing about the caller that actually escapes the actor.
    ///
    /// The guard is instead a source-level one, in the same registry that already
    /// classifies durable writers: `no_undeclared_staged_drift_caller_exists`
    /// requires every shipped caller of this function to be declared in
    /// `DURABLE_SIDECAR_WRITERS`, and `durable_writer_routes_agree_with_the_read_only_classification`
    /// requires that row to name a real routed verb. A boot/CLI/spawn caller has
    /// no such verb, so it cannot be declared quietly — it fails at the moment it
    /// is written, not only if some test happens to run it.
    pub(crate) fn note_durable_sidecar_drift(&self) {
        let _ = self.note_staged_persist();
    }

    /// Serialize the fixed candidate inventory directly from live in-memory
    /// owners. Kept stage-agnostic so strict recovery can produce the same state
    /// witness without manufacturing a persistence capability.
    fn checkpoint_candidate_files(&self) -> M1ndResult<Vec<SessionCheckpointCandidateFile>> {
        let mut files = Vec::new();
        let (graph_bytes, temporal_bytes, plasticity_bytes) = {
            let graph = self.graph.read();
            let graph_bytes = m1nd_core::snapshot::encode_graph_json(&graph)?;
            let temporal_bytes = crate::temporal_state::encode_temporal_state(
                &graph,
                &self.temporal.co_change,
                &self.orchestrator.temporal.co_change,
            )?;
            let plasticity = self.plasticity.export_state(&graph)?;
            let plasticity_bytes =
                m1nd_core::plasticity::encode_plasticity_state_json(&plasticity)?;
            (graph_bytes, temporal_bytes, plasticity_bytes)
        };

        push_checkpoint_candidate_file(
            &mut files,
            &self.runtime_root,
            "graph_snapshot",
            &self.graph_path,
            CHECKPOINT_GRAPH_SCHEMA_ID,
            &m1nd_core::snapshot::SNAPSHOT_VERSION.to_string(),
            CheckpointCandidatePresence::Present(graph_bytes),
        )?;
        let ingest_roots_path = self
            .graph_path
            .parent()
            .unwrap_or(&self.runtime_root)
            .join("ingest_roots.json");
        push_checkpoint_candidate_file(
            &mut files,
            &self.runtime_root,
            "ingest_roots",
            &ingest_roots_path,
            CHECKPOINT_ROOTS_SCHEMA_ID,
            CHECKPOINT_ROOTS_SCHEMA_VERSION,
            CheckpointCandidatePresence::Present(canonical_json_bytes(&self.ingest_roots)?),
        )?;

        let sidecars = [
            (
                "plasticity_state",
                self.plasticity_path.as_path(),
                CheckpointCandidatePresence::Present(plasticity_bytes),
            ),
            (
                "antibodies",
                self.antibodies_path.as_path(),
                CheckpointCandidatePresence::Present(m1nd_core::antibody::encode_antibodies_json(
                    &self.antibodies,
                )?),
            ),
            (
                "tremor_state",
                self.tremor_path.as_path(),
                CheckpointCandidatePresence::Present(m1nd_core::tremor::encode_tremor_state_json(
                    &self.tremor_registry,
                )?),
            ),
            (
                "trust_state",
                self.trust_path.as_path(),
                CheckpointCandidatePresence::Present(m1nd_core::trust::encode_trust_state_json(
                    &self.trust_ledger,
                )?),
            ),
            (
                "calibration_state",
                self.calibration_path.as_path(),
                CheckpointCandidatePresence::Present(
                    m1nd_core::calibration::encode_calibration_state_json(&self.calibration_table)?,
                ),
            ),
            (
                "temporal_state",
                self.temporal_state_path.as_path(),
                CheckpointCandidatePresence::Present(temporal_bytes),
            ),
        ];
        for (logical_name, path, presence) in sidecars {
            push_checkpoint_candidate_file(
                &mut files,
                &self.runtime_root,
                logical_name,
                path,
                CHECKPOINT_SIDECAR_SCHEMA_ID,
                CHECKPOINT_SIDECAR_SCHEMA_VERSION,
                presence,
            )?;
        }

        for (logical_name, relative_path) in [
            (
                "boot_memory_state",
                crate::boot_kv_migration::LEGACY_BOOT_KV_FILE,
            ),
            ("boot_config", crate::boot_kv_migration::BOOT_CONFIG_FILE),
            (
                "boot_kv_migration",
                crate::boot_kv_migration::MIGRATION_MARKER_FILE,
            ),
            (
                "boot_kv_migration_journal",
                crate::boot_kv_migration::MIGRATION_JOURNAL_FILE,
            ),
        ] {
            let presence = self
                .boot_kv_checkpoint_inventory
                .fixed_file(relative_path)
                .ok_or_else(|| M1ndError::CorruptState {
                    reason: format!(
                        "Boot KV checkpoint inventory omitted fixed path '{relative_path}'"
                    ),
                })?
                .as_ref()
                .map(|bytes| CheckpointCandidatePresence::Present(bytes.clone()))
                .unwrap_or(CheckpointCandidatePresence::Absent);
            push_checkpoint_candidate_file(
                &mut files,
                &self.runtime_root,
                logical_name,
                &self.runtime_root.join(relative_path),
                CHECKPOINT_SIDECAR_SCHEMA_ID,
                CHECKPOINT_SIDECAR_SCHEMA_VERSION,
                presence,
            )?;
        }
        for (index, (relative_path, bytes)) in self
            .boot_kv_checkpoint_inventory
            .migrated_lights()
            .enumerate()
        {
            push_checkpoint_candidate_file(
                &mut files,
                &self.runtime_root,
                &format!("boot_kv_migrated_light_{index}"),
                &self.runtime_root.join(relative_path),
                CHECKPOINT_SIDECAR_SCHEMA_ID,
                CHECKPOINT_SIDECAR_SCHEMA_VERSION,
                CheckpointCandidatePresence::Present(bytes.to_vec()),
            )?;
        }

        let auto_ingest_path = self.runtime_root.join("auto_ingest_state.json");
        let document_cache_path = self.runtime_root.join("document_cache_index.json");
        let document_artifact_inventory_path =
            crate::universal_docs::document_artifact_inventory_path(&self.runtime_root);
        let binary_snapshot_path = self.graph_path.with_extension("bin");
        if self
            .daemon_state
            .last_tick_duration_ms
            .is_some_and(|value| !value.is_finite())
            || self
                .daemon_alerts
                .iter()
                .any(|alert| !alert.confidence.is_finite())
        {
            return Err(M1ndError::CorruptState {
                reason: "daemon checkpoint state contains a non-finite value".into(),
            });
        }
        for (logical_name, path, presence) in [
            (
                "daemon_state",
                self.daemon_state_path.as_path(),
                CheckpointCandidatePresence::Present(canonical_json_bytes(&self.daemon_state)?),
            ),
            (
                "daemon_alerts",
                self.daemon_alerts_path.as_path(),
                CheckpointCandidatePresence::Present(canonical_json_bytes(&self.daemon_alerts)?),
            ),
            (
                "auto_ingest_state",
                auto_ingest_path.as_path(),
                CheckpointCandidatePresence::Present(self.auto_ingest.encode_checkpoint_state()?),
            ),
            (
                "document_cache_index",
                document_cache_path.as_path(),
                CheckpointCandidatePresence::Present(crate::universal_docs::encode_document_cache(
                    &self.document_cache,
                )?),
            ),
            (
                "embeddings_cache",
                self.embeddings_cache_path.as_path(),
                // The cache is derived and has no complete in-memory owner.
                // Explicit absence is safer than checkpointing stale disk bytes.
                CheckpointCandidatePresence::Absent,
            ),
            (
                "binary_graph_snapshot",
                binary_snapshot_path.as_path(),
                // Binary snapshots are a derived, explicitly requested export.
                // Their exact graph source is queued in memory and materialized
                // only after CURRENT; otherwise stale exports are removed.
                CheckpointCandidatePresence::Absent,
            ),
        ] {
            push_checkpoint_candidate_file(
                &mut files,
                &self.runtime_root,
                logical_name,
                path,
                CHECKPOINT_SIDECAR_SCHEMA_ID,
                CHECKPOINT_SIDECAR_SCHEMA_VERSION,
                presence,
            )?;
        }

        crate::universal_docs::validate_inventory_against_cache(
            &self.runtime_root,
            &self.document_artifacts,
            &self.document_cache,
        )?;
        push_checkpoint_candidate_file(
            &mut files,
            &self.runtime_root,
            "document_artifact_inventory",
            &document_artifact_inventory_path,
            crate::universal_docs::DOCUMENT_ARTIFACT_INVENTORY_SCHEMA_ID,
            crate::universal_docs::DOCUMENT_ARTIFACT_SCHEMA_VERSION,
            CheckpointCandidatePresence::Present(
                crate::universal_docs::encode_document_artifact_inventory(
                    &self.document_artifacts,
                )?,
            ),
        )?;

        for artifact in self.document_artifacts.files() {
            let presence = match &artifact.presence {
                DocumentArtifactPresence::Present(bytes) => {
                    CheckpointCandidatePresence::Present(bytes.clone())
                }
                DocumentArtifactPresence::Absent => CheckpointCandidatePresence::Absent,
            };
            push_checkpoint_candidate_file(
                &mut files,
                &self.runtime_root,
                &artifact.logical_name,
                &self.runtime_root.join(&artifact.relative_path),
                crate::universal_docs::DOCUMENT_ARTIFACT_SCHEMA_ID,
                crate::universal_docs::DOCUMENT_ARTIFACT_SCHEMA_VERSION,
                presence,
            )?;
        }

        let mut logical_names = HashSet::new();
        let mut relative_paths = HashSet::new();
        for file in &files {
            if !logical_names.insert(file.logical_name.as_str()) {
                return Err(M1ndError::CorruptState {
                    reason: format!(
                        "candidate checkpoint contains duplicate logical name '{}'",
                        file.logical_name
                    ),
                });
            }
            if !relative_paths.insert(file.relative_path.as_str()) {
                return Err(M1ndError::CorruptState {
                    reason: format!(
                        "candidate checkpoint contains duplicate owned path '{}'",
                        file.relative_path
                    ),
                });
            }
        }
        Ok(files)
    }

    /// Serialize the exact candidate directly from live in-memory owners. No
    /// filesystem write or working-file read occurs here. Every fixed managed
    /// path is represented explicitly as PRESENT or ABSENT, and every path is
    /// lexically confined beneath the runtime root before bytes are returned.
    pub(crate) fn checkpoint_candidate(
        &self,
        stage: &CheckpointPersistenceStage,
    ) -> M1ndResult<SessionCheckpointCandidate> {
        self.verify_checkpoint_stage(stage)?;
        let files = self.checkpoint_candidate_files()?;
        let session_requested = self
            .persistence_stage
            .get()
            .is_some_and(|active| active.persist_requested);
        let auto_ingest_requested = self.auto_ingest.checkpoint_persist_requested(stage.id)?;
        // Only the PRESENT/ABSENT working set is authoritative and therefore
        // restart-reconstructible. Derived post-CURRENT effects are tracked by
        // `persist_requested` and the live stage, but never folded into this
        // digest unless their bytes are also sealed in the candidate.
        let state_digest = checkpoint_candidate_digest(&files);
        Ok(SessionCheckpointCandidate {
            files,
            persist_requested: session_requested || auto_ingest_requested,
            state_digest,
        })
    }

    /// Cheap, stage-preserving answer to "must this transaction publish a
    /// checkpoint?". It reports exactly the two persist flags
    /// [`Self::checkpoint_candidate`] folds into `persist_requested`, plus any
    /// queued post-CURRENT effect (which only the checkpoint path can drain),
    /// WITHOUT serializing the ~100 MB candidate to find out.
    ///
    /// The read path asks this question on every single call. Answering it by
    /// serializing graph + temporal + plasticity is what turned a warm `seek`
    /// into seconds of work.
    /// A derived post-CURRENT effect is queued and only the checkpoint path can
    /// drain it. Unlike a routine persist request, this cannot be deferred: the
    /// stage refuses to close while one is outstanding.
    pub(crate) fn has_unresolved_staged_effects(&self) -> bool {
        !self.staged_binary_snapshot_effects.is_empty()
    }

    pub(crate) fn checkpoint_publish_required(
        &self,
        stage: &CheckpointPersistenceStage,
    ) -> M1ndResult<bool> {
        self.verify_checkpoint_stage(stage)?;
        if self.has_unresolved_staged_effects() {
            return Ok(true);
        }
        let session_requested = self
            .persistence_stage
            .get()
            .is_some_and(|active| active.persist_requested);
        Ok(session_requested || self.auto_ingest.checkpoint_persist_requested(stage.id)?)
    }

    /// Stage-free witness of the currently rebuilt authoritative in-memory
    /// working set. The brain actor compares this with the digest stored in the
    /// checkpoint working-set envelope after rollback/reconciliation. An active
    /// stage or unresolved derived effect is refused rather than omitted.
    pub(crate) fn authoritative_checkpoint_state_digest(&self) -> M1ndResult<String> {
        if let Some(active) = self.persistence_stage.get() {
            return Err(M1ndError::PersistenceFailed(format!(
                "authoritative digest is unavailable while checkpoint transaction {} is active",
                active.id
            )));
        }
        if !self.staged_binary_snapshot_effects.is_empty() {
            return Err(M1ndError::PersistenceFailed(
                "authoritative digest is unavailable with unresolved derived effects".into(),
            ));
        }
        let files = self.checkpoint_candidate_files()?;
        Ok(checkpoint_candidate_digest(&files))
    }

    /// Replace the live SharedGraph with a v4 encode/decode deep clone. Any Arc
    /// escaped by an untrusted callback continues to point at the detached old
    /// graph and can no longer mutate the actor-owned state after the callback
    /// boundary.
    pub(crate) fn rebind_detached_graph(&mut self) -> M1ndResult<()> {
        let bytes = {
            let graph = self.graph.read();
            m1nd_core::snapshot::encode_graph_json(&graph)?
        };
        let graph = m1nd_core::snapshot::decode_graph_json(&bytes)?;
        self.graph = Arc::new(parking_lot::RwLock::new(graph));
        Ok(())
    }

    /// Actor-aware implementation of `persist(format="bin")`. During a
    /// checkpoint transaction it captures the exact graph as v4 JSON and queues
    /// a typed derived effect; no binary/canonical path is touched. Outside an
    /// actor transaction it preserves the historical immediate write behavior.
    pub(crate) fn persist_binary_snapshot(&mut self) -> M1ndResult<PathBuf> {
        let path = self.graph_path.with_extension("bin");
        if self.persistence_stage.get().is_some() {
            let relative_path = strict_runtime_relative_path(&self.runtime_root, &path)?;
            let graph_v4_json = {
                let graph = self.graph.read();
                m1nd_core::snapshot::encode_graph_json(&graph)?
            };
            self.staged_binary_snapshot_effects
                .retain(|effect| effect.relative_path != relative_path);
            self.staged_binary_snapshot_effects
                .push(StagedBinarySnapshotEffect {
                    relative_path,
                    graph_v4_json,
                });
            let _ = self.note_staged_persist();
            return Ok(path);
        }

        refuse_non_regular_checkpoint_target(&path)?;
        let graph = self.graph.read();
        m1nd_core::snapshot_bin::save_graph(&graph, &path)?;
        let plasticity = self.plasticity.export_state(&graph)?;
        m1nd_core::snapshot::save_plasticity_state(&plasticity, &self.plasticity_path)?;
        Ok(path)
    }

    /// Materialize queued derived exports after CURRENT selected and validated
    /// the candidate. Graph bytes come from the staged v4 payload, not from a
    /// potentially changed live Arc. Effects remain queued on any error so the
    /// transaction cannot close or report a false success.
    pub(crate) fn apply_staged_post_commit_effects(
        &mut self,
        stage: &CheckpointPersistenceStage,
    ) -> M1ndResult<usize> {
        self.verify_checkpoint_stage(stage)?;
        for effect in &self.staged_binary_snapshot_effects {
            let path = self.runtime_root.join(&effect.relative_path);
            let observed = strict_runtime_relative_path(&self.runtime_root, &path)?;
            if observed != effect.relative_path {
                return Err(M1ndError::CorruptState {
                    reason: "staged binary snapshot path changed identity".into(),
                });
            }
            refuse_non_regular_checkpoint_target(&path)?;
            let graph = m1nd_core::snapshot::decode_graph_json(&effect.graph_v4_json)?;
            m1nd_core::snapshot_bin::save_graph(&graph, &path)?;
        }
        let applied = self.staged_binary_snapshot_effects.len();
        self.staged_binary_snapshot_effects.clear();
        Ok(applied)
    }

    /// Release the persistence fence only after the caller has either confirmed
    /// CURRENT or restored an authoritative preimage. A wrong token never
    /// clears the fence.
    pub(crate) fn finish_checkpoint_staging(
        &mut self,
        stage: CheckpointPersistenceStage,
    ) -> M1ndResult<bool> {
        self.verify_checkpoint_stage(&stage)?;
        if !self.staged_binary_snapshot_effects.is_empty() {
            return Err(M1ndError::PersistenceFailed(format!(
                "{} staged binary snapshot effect(s) were not applied after CURRENT",
                self.staged_binary_snapshot_effects.len()
            )));
        }
        let session_requested = self
            .persistence_stage
            .get()
            .is_some_and(|active| active.persist_requested);
        let auto_ingest_requested = self.auto_ingest.finish_checkpoint_staging(stage.id)?;
        self.persistence_stage.set(None);
        Ok(session_requested || auto_ingest_requested)
    }

    /// Abandon an actor candidate after the callback was refused and before
    /// restoring the authoritative checkpoint. No staged post-CURRENT effect
    /// may survive into the reload, and closing the token here is required so
    /// strict reload does not mistake the rejected transaction for a live one.
    pub(crate) fn abort_checkpoint_staging(
        &mut self,
        stage: CheckpointPersistenceStage,
    ) -> M1ndResult<()> {
        self.verify_checkpoint_stage(&stage)?;
        self.staged_binary_snapshot_effects.clear();
        let _ = self.auto_ingest.finish_checkpoint_staging(stage.id)?;
        self.persistence_stage.set(None);
        Ok(())
    }

    /// Persist all state to disk.
    ///
    /// Ordering: graph first (source of truth), then every durable sidecar.
    /// The actor checkpoint fence requires an all-or-error result: logging and
    /// continuing would let a readable old sidecar masquerade as the new state.
    pub fn persist(&mut self) -> M1ndResult<()> {
        // HARD SAFETY: a read-only attach must never write to disk. This is the
        // single choke point every persist call site funnels through, so this
        // early return protects the writer's on-disk state from corruption.
        if self.read_only {
            self.log_read_only_persist_skip();
            return Ok(());
        }
        if self.note_staged_persist() {
            return Ok(());
        }
        self.instance.mark_heartbeat()?;
        self.persist_ingest_roots()?;
        let graph = self.graph.read();

        // Graph is the source of truth — save it first.
        m1nd_core::snapshot::save_graph(&graph, &self.graph_path)?;

        // Temporal evidence is graph-bound and affects action authorization.
        // Unlike advisory sidecars below, failure is fatal: publishing a
        // complete persist while silently discarding learned co-change state
        // would make restart behavior non-reproducible.
        crate::temporal_state::save_temporal_state(
            &self.temporal_state_path,
            &graph,
            &self.temporal.co_change,
            &self.orchestrator.temporal.co_change,
        )?;

        let plasticity = self.plasticity.export_state(&graph)?;
        m1nd_core::snapshot::save_plasticity_state(&plasticity, &self.plasticity_path)?;
        m1nd_core::antibody::save_antibodies(&self.antibodies, &self.antibodies_path)?;
        m1nd_core::trust::save_trust_state(&self.trust_ledger, &self.trust_path)?;
        m1nd_core::calibration::save_calibration_state(
            &self.calibration_table,
            &self.calibration_path,
        )?;
        m1nd_core::tremor::save_tremor_state(&self.tremor_registry, &self.tremor_path)?;
        self.persist_boot_memory()?;
        self.persist_daemon_state()?;
        self.persist_daemon_alerts()?;
        self.auto_ingest.persist(&self.runtime_root)?;
        // Universal cache index, inventory, and bodies are candidate-only.
        // Their sole physical publisher is the brain checkpoint projection
        // after CURRENT; a legacy/direct persist must never expose them early.

        self.last_persist_time = Some(Instant::now());
        Ok(())
    }

    fn persist_ingest_roots(&mut self) -> M1ndResult<()> {
        let persist_root = self
            .graph_path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| self.runtime_root.clone());
        std::fs::create_dir_all(&persist_root)?;
        let ingest_roots_path = persist_root.join("ingest_roots.json");
        save_json_atomic(&ingest_roots_path, &self.ingest_roots)
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
        if self.note_staged_persist() {
            return Ok(());
        }
        if crate::boot_kv_migration::migration_status(&self.runtime_root)?.is_some() {
            // The compatibility source is retired. Do not even rewrite the
            // empty tombstone: there is exactly one active sink per entry type.
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
        if self.note_staged_persist() {
            return Ok(());
        }
        if self
            .daemon_state
            .last_tick_duration_ms
            .is_some_and(|value| !value.is_finite())
        {
            return Err(M1ndError::CorruptState {
                reason: "daemon state contains a non-finite tick duration".into(),
            });
        }
        save_json_atomic(&self.daemon_state_path, &self.daemon_state)
    }

    fn load_daemon_state(path: &Path) -> DaemonRuntimeState {
        let mut state = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<DaemonRuntimeState>(&s).ok())
            .unwrap_or_default();
        // RESUME SANITIZATION (gardener v1). `active` legitimately survives a
        // boot — an armed daemon stays armed across restart AND across an LRU
        // eviction re-resolve (the per-brain opt-in is this file in the brain's
        // own store dir). But two kinds of field describe the RUNTIME, not the
        // config, and resuming them verbatim breaks the resume:
        //  - `tick_in_flight`/`pending_rerun` are in-process reentrancy flags.
        //    Every traffic tick persists MID-tick (while `tick_in_flight` is
        //    true) and the post-tick `false` lives only in memory, so the disk
        //    almost always carries `tick_in_flight: true`. Resuming it verbatim
        //    WEDGES the daemon forever: `run_daemon_tick` sees a tick "in
        //    flight" that died with the old process and refuses every new tick.
        //  - `watch_backend == "native_fs"` asserts a LIVE notify watcher. Only
        //    the stdio serve() loop owns one (`refresh_daemon_watcher`); a
        //    freshly booted state has none, and on the HTTP owner none will
        //    ever exist — resuming the label verbatim makes `daemon_status`
        //    LIE about an event consumer. Downgrade to the honest "polling";
        //    the stdio loop re-arms and restores the label only when a real
        //    watcher starts. (`git_native_fs` survives: it names the per-tick
        //    git-diff detection, true on every transport.)
        state.tick_in_flight = false;
        state.pending_rerun = false;
        if state.watch_backend == "native_fs" {
            state.watch_backend = "polling".into();
        }
        state
    }

    pub fn persist_daemon_alerts(&self) -> M1ndResult<()> {
        if self.read_only {
            self.log_read_only_persist_skip();
            return Ok(());
        }
        if self.note_staged_persist() {
            return Ok(());
        }
        if self
            .daemon_alerts
            .iter()
            .any(|alert| !alert.confidence.is_finite())
        {
            return Err(M1ndError::CorruptState {
                reason: "daemon alerts contain a non-finite confidence".into(),
            });
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
        self.proof_ready.clear();
        self.active_proof_permits.clear();

        Ok(())
    }

    /// Take the live graph's learned synapses out before a graph replacement
    /// overwrites them.
    ///
    /// The counters live in `graph.edge_plasticity`, not in the engine, so the
    /// replacement destroys them the moment the new graph is installed — this
    /// has to be called BEFORE the swap. Fail-open in both directions: a graph
    /// with no edges and a graph that cannot be exported both carry nothing,
    /// and the persisted sidecar alone is used.
    pub(crate) fn export_learned_synapses_before_replacement(
        &self,
    ) -> Vec<m1nd_core::plasticity::SynapticState> {
        let graph = self.graph.read();
        if graph.csr.num_edges() == 0 {
            return Vec::new();
        }
        match self.plasticity.export_state(&graph) {
            Ok(states) => states,
            Err(error) => {
                eprintln!(
                    "[m1nd] could not carry the live synaptic state across the graph replacement ({error}); continuing with the persisted sidecar alone"
                );
                Vec::new()
            }
        }
    }

    /// Re-apply learned plasticity to a graph that has just replaced the live
    /// one. Returns how many synapses were restored.
    ///
    /// A replacement graph's `edge_plasticity` arrays are born zeroed
    /// (`Graph::add_edge`), so without this the Hebbian layer is erased by
    /// every ingest and the next persist writes the zeros over the sidecar —
    /// measured in the field as 73,332 synaptic rows with not one non-zero
    /// counter among them. The restore itself is the ordinary import, which
    /// binds by the `(source, target, relation)` label triple and therefore
    /// survives the renumbering an ingest does; `carry_forward_synaptic_state`
    /// only decides which record describes each identity.
    ///
    /// Call it IMMEDIATELY after `rebuild_engines`, and never before: the
    /// rebuild installs two fresh engines whose `query_count` is zero, and
    /// `import_state` is what seeds them from the restored recency. Any persist
    /// between the swap and this call would publish the zeros.
    ///
    /// Fail-open throughout, the standing posture for a sidecar: an unreadable
    /// or corrupt file degrades to "counters start over" with one honest line,
    /// never to a failed ingest.
    pub(crate) fn restore_learned_synapses_after_replacement(
        &mut self,
        carried: Vec<m1nd_core::plasticity::SynapticState>,
    ) -> usize {
        let persisted = if self.plasticity_path.exists() {
            match m1nd_core::snapshot::load_plasticity_state(&self.plasticity_path) {
                Ok(states) => states,
                Err(error) => {
                    eprintln!(
                        "[m1nd] Failed to load plasticity state ({error}), continuing without it"
                    );
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        let states = m1nd_core::plasticity::carry_forward_synaptic_state(carried, persisted);
        if states.is_empty() {
            return 0;
        }

        let mut graph = self.graph.write();
        // BOTH engines restore, exactly as the friendly boot, strict recovery
        // and the legacy adoption do. `self.orchestrator.plasticity` is the
        // engine `activate`/`query` actually update (query.rs `query()` step 8)
        // and it stamps its own `query_count` into `last_used_query`; left at
        // zero beside a graph carrying restored counts, the first strengthen
        // would mark a just-used edge 1 — older than everything the restore
        // brought back. Re-applying the same validated plan to the same
        // topology is idempotent and cannot fail where the first import
        // succeeded.
        let imported = self
            .plasticity
            .import_state(&mut graph, &states)
            .and_then(|applied| {
                self.orchestrator
                    .plasticity
                    .import_state(&mut graph, &states)
                    .map(|_| applied)
            });
        match imported {
            Ok(applied) => applied as usize,
            Err(error) => {
                // `import_state` validates the whole record set before touching
                // one slot, so a refusal leaves the graph untouched — but it can
                // leave the two engines disagreeing about the query counter.
                // Drop both for clean ones bound to the replacement rather than
                // let that skew reach the persist at the end of this ingest.
                self.plasticity = PlasticityEngine::new(
                    &graph,
                    m1nd_core::plasticity::PlasticityConfig::default(),
                );
                self.orchestrator.plasticity = PlasticityEngine::new(
                    &graph,
                    m1nd_core::plasticity::PlasticityConfig::default(),
                );
                eprintln!(
                    "[m1nd] Failed to import plasticity state ({error}), continuing without it"
                );
                0
            }
        }
    }

    // --- Perspective MCP methods (12-PERSPECTIVE-SYNTHESIS) ---

    /// Bump graph generation (Theme 1). Called after ingest and rebuild_engines.
    pub fn bump_graph_generation(&mut self) {
        self.graph_generation += 1;
        self.cache_generation = self.cache_generation.max(self.graph_generation);
        self.proof_ready.clear();
        self.active_proof_permits.clear();
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

    /// Whether this brain's `predict` calibration is armed — a measured conformal
    /// τ exists for this repo (TWO-TIER §9.5.1 card field G2). Uncalibrated brains
    /// cap `predict` verdicts at `abstain` (`tools.rs`), so the Hall renders "not
    /// measured on this repo yet". A cheap per-brain read for the R14 partition.
    pub fn calibration_armed(&self) -> bool {
        self.calibration_table
            .get(m1nd_core::calibration::CALIBRATION_SIGNAL_PREDICT)
            .is_some()
    }

    /// Track an agent session. Creates a new session if first contact,
    /// otherwise updates last_seen and increments query_count.
    ///
    /// P1: all four dispatch seams funnel through here, so this is the single
    /// choke point for the durable-presence BEAT — a throttled, fail-open
    /// projection of this live session to a sidecar (`crate::presence`) so the
    /// control room (cockpit / Hall / north) can see the team.
    pub fn track_agent(&mut self, agent_id: &str) {
        let _ = self.instance.mark_heartbeat();
        let now = Instant::now();
        let now_ms = crate::util::now_ms();
        let session = self
            .sessions
            .entry(agent_id.to_string())
            .or_insert_with(|| AgentSession {
                agent_id: agent_id.to_string(),
                first_seen: now,
                last_seen: now,
                query_count: 0,
                first_seen_ms: now_ms,
                last_presence_beat: None,
                mutation_observed_at_ms: None,
                declared_kind: None,
                declared_theme: None,
                declared_intent: None,
                declared_worktree: None,
                declared_working_set: Vec::new(),
            });
        session.last_seen = now;
        session.query_count += 1;
        self.beat_presence(agent_id, now, now_ms);
    }

    /// Stamp the OBSERVED mutation level (verdict c): this session just
    /// dispatched a verb `server::read_only_denied` classifies as mutating. Pure
    /// in-memory — the throttled beat carries it to the sidecar. Never a write
    /// per call, never able to break the tool call it rides.
    pub fn note_mutation_observed(&mut self, agent_id: &str) {
        if let Some(session) = self.sessions.get_mut(agent_id) {
            session.mutation_observed_at_ms = Some(crate::util::now_ms());
            // A changed signal forces the NEXT beat to write promptly (bypass the
            // throttle for state changes; pure read spam still stays throttled).
            session.last_presence_beat = None;
        }
    }

    /// Record the DECLARED presence enrichment from a `session_handshake` call
    /// (all fields optional, honest-absent). Only overwrites a field when the
    /// caller declared it, so a later bare handshake never erases an earlier
    /// declaration. Applied to the tracked session (created if first contact).
    pub fn set_presence_declaration(
        &mut self,
        agent_id: &str,
        kind: Option<String>,
        theme: Option<String>,
        intent: Option<String>,
        worktree: Option<String>,
        working_set: Vec<String>,
    ) {
        let now = Instant::now();
        let now_ms = crate::util::now_ms();
        let session = self
            .sessions
            .entry(agent_id.to_string())
            .or_insert_with(|| AgentSession {
                agent_id: agent_id.to_string(),
                first_seen: now,
                last_seen: now,
                query_count: 0,
                first_seen_ms: now_ms,
                last_presence_beat: None,
                mutation_observed_at_ms: None,
                declared_kind: None,
                declared_theme: None,
                declared_intent: None,
                declared_worktree: None,
                declared_working_set: Vec::new(),
            });
        let mut changed = false;
        if kind.is_some() {
            session.declared_kind = kind;
            changed = true;
        }
        if theme.is_some() {
            session.declared_theme = theme;
            changed = true;
        }
        if intent.is_some() {
            session.declared_intent = intent;
            changed = true;
        }
        if worktree.is_some() {
            session.declared_worktree = worktree;
            changed = true;
        }
        if !working_set.is_empty() {
            session.declared_working_set = working_set;
            changed = true;
        }
        if changed {
            // Force the next beat to carry the fresh declaration (bypass throttle).
            session.last_presence_beat = None;
        }
    }

    /// The throttled, fail-open half of the presence beat: at most one disk
    /// write per session per [`crate::presence::PRESENCE_BEAT_THROTTLE_MS`].
    /// Composes the record from this session's own measured/declared facts and
    /// upserts its sidecar. A broken sidecar write can NEVER break a tool call
    /// (wrapped in the vigil fail-open guard).
    fn beat_presence(&mut self, agent_id: &str, now: Instant, now_ms: u64) {
        // A presence needs a served brain — an unbound (pre-ingest) session has
        // no brain roster to join. Honest-absent until bound.
        let Some(brain) = self.workspace_root.clone() else {
            return;
        };
        // Throttle.
        let due = match self
            .sessions
            .get(agent_id)
            .and_then(|s| s.last_presence_beat)
        {
            Some(prev) => {
                now.duration_since(prev).as_millis()
                    >= crate::presence::PRESENCE_BEAT_THROTTLE_MS as u128
            }
            None => true,
        };
        if !due {
            return;
        }
        let record = match self.compose_presence(agent_id, &brain, now_ms) {
            Some(record) => record,
            None => return,
        };
        let registry_root = self.instance.registry_root();
        crate::server::vigil_fail_open("presence beat", "track_agent", || {
            crate::presence::write_presence(&registry_root, &record)
        });
        if let Some(session) = self.sessions.get_mut(agent_id) {
            session.last_presence_beat = Some(now);
        }
    }

    /// Compose the durable presence record for `agent_id` from this session's own
    /// facts: measured binding (`brain`/`caller_root`), the in-memory session
    /// counters, the DECLARED handshake enrichment, and the MEASURED `task_ref`
    /// (the agent's own open mission charter). `None` when the agent has no
    /// tracked session yet.
    fn compose_presence(
        &self,
        agent_id: &str,
        brain: &str,
        now_ms: u64,
    ) -> Option<crate::presence::PresenceRecord> {
        let session = self.sessions.get(agent_id)?;
        let task_ref =
            crate::mission_handlers::latest_open_mission_for(&self.runtime_root, agent_id);
        Some(crate::presence::PresenceRecord {
            schema: crate::presence::PRESENCE_SCHEMA.to_string(),
            presence_id: crate::presence::stable_presence_id(agent_id, brain),
            agent_id: agent_id.to_string(),
            brain: brain.to_string(),
            caller_root: self.caller_root.clone(),
            kind: session.declared_kind.clone(),
            theme: session.declared_theme.clone(),
            worktree: session.declared_worktree.clone(),
            working_set: session.declared_working_set.clone(),
            task_ref,
            mutation: crate::presence::MutationSignal {
                observed_at_ms: session.mutation_observed_at_ms,
                declared_intent: session.declared_intent.clone(),
            },
            first_seen_ms: session.first_seen_ms,
            last_beat_ms: now_ms,
            query_count: session.query_count,
            ttl_ms: crate::presence::PRESENCE_TTL_MS,
        })
    }

    /// The live presence roster for the SERVED brain plus any derived collisions
    /// — the read surface the cockpit, north, and `/api/health` share. Scoped to
    /// this session's own brain ("this brain", never a cross-brain aggregate);
    /// empty when the session is unbound. Fail-open: an unreadable registry
    /// yields an empty roster, never an error.
    pub fn presence_roster(
        &self,
    ) -> (
        Vec<crate::presence::PresenceRecord>,
        Vec<crate::presence::Collision>,
    ) {
        let Some(brain) = self.workspace_root.as_deref() else {
            return (Vec::new(), Vec::new());
        };
        let now = crate::util::now_ms();
        let registry_root = self.instance.registry_root();
        let roster = crate::presence::roster_for_brain(&registry_root, brain, now);
        let collisions = crate::presence::collisions_in(&roster, now);
        (roster, collisions)
    }

    pub fn next_edit_preview_id(&self, agent_id: &str) -> String {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let short_id = &agent_id[..agent_id.len().min(8)];
        format!("preview_{}_{}", short_id, now_ms)
    }

    /// Mint a staged-transplant handle (A2), mirroring [`Self::next_edit_preview_id`]
    /// with a verb-naming prefix so an agent can tell the two handle families apart.
    pub fn next_transplant_preview_id(&self, agent_id: &str) -> String {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let short_id = &agent_id[..agent_id.len().min(8)];
        format!("transplant_preview_{}_{}", short_id, now_ms)
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

    /// Record ONE dispatched verb in the durable usage ledger.
    ///
    /// The counterpart of [`Self::log_query`], and deliberately its opposite in
    /// every dimension that matters: the query log is per-agent, per-session,
    /// capped, in-memory, and carries a query preview; this carries a verb name
    /// and three counters, survives restarts, and holds NOTHING a caller wrote.
    /// `tool_name` is mapped onto a compiled route name before it is stored —
    /// caller text never reaches the file (`crate::verb_usage`).
    ///
    /// Two properties this must keep: it is called from exactly ONE seam
    /// (`server::dispatch_generic_tool`), so the counters cannot disagree with
    /// themselves; and its disk write is fail-open, so a broken counter file
    /// costs a log line rather than the agent's tool call.
    pub fn record_verb_call(
        &mut self,
        tool_name: &str,
        outcome: crate::verb_usage::VerbCallOutcome,
    ) {
        let verb = crate::verb_usage::canonical_verb(tool_name);
        let now_ms = crate::util::now_ms();
        self.verb_usage.record(verb, outcome, now_ms);
        if self.read_only {
            // Attach-mode never writes the owner's runtime root. The counts
            // still accumulate in memory for this session's own `report`.
            return;
        }
        let ledger = &mut self.verb_usage;
        crate::server::vigil_fail_open("verb usage counters", verb, || ledger.flush_if_due(now_ms));
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

    /// Resolve a proof target to one canonical absolute identity. Unlike the
    /// legacy repo-relative key, this preserves which ingest root owns the file,
    /// so equal relative paths in two brains/scopes cannot share a mark.
    fn proof_target_identity(&self, raw_target: &str) -> Result<String, String> {
        if self.ingest_roots.is_empty() {
            return Err("no ingest roots are bound".to_string());
        }
        let raw = raw_target
            .trim()
            .strip_prefix("file::")
            .unwrap_or(raw_target.trim());
        if raw.is_empty() {
            return Err("target is empty".to_string());
        }

        let requested = Path::new(raw);
        let candidate = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            // Mirror surgical apply resolution exactly: newest existing match,
            // otherwise the newest bound root for a not-yet-created file.
            self.ingest_roots
                .iter()
                .rev()
                .map(|root| Path::new(root).join(requested))
                .find(|path| path.exists())
                .or_else(|| {
                    self.ingest_roots
                        .last()
                        .map(|root| Path::new(root).join(requested))
                })
                .ok_or_else(|| "target has no resolvable ingest root".to_string())?
        };

        let identity = if candidate.exists() {
            candidate
                .canonicalize()
                .map_err(|error| format!("cannot canonicalize target: {error}"))?
        } else {
            let parent = candidate
                .parent()
                .ok_or_else(|| "target has no parent directory".to_string())?;
            let file_name = candidate
                .file_name()
                .ok_or_else(|| "target has no file name".to_string())?;
            parent
                .canonicalize()
                .map_err(|error| format!("cannot canonicalize target parent: {error}"))?
                .join(file_name)
        };

        let inside_bound_root = self.ingest_roots.iter().any(|root| {
            Path::new(root)
                .canonicalize()
                .is_ok_and(|canonical_root| identity.starts_with(canonical_root))
        });
        if !inside_bound_root {
            return Err(format!(
                "target '{}' escapes every bound ingest root",
                identity.display()
            ));
        }
        Ok(crate::scope::normalize_path_text(
            &identity.to_string_lossy(),
        ))
    }

    fn proof_target_digest(target_identity: &str) -> Result<String, String> {
        let path = Path::new(target_identity);
        match std::fs::read(path) {
            Ok(bytes) => Ok(format!(
                "sha256:{}",
                crate::util::hex_lower(&Sha256::digest(bytes))
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // Missing is a first-class disk state, not an empty-file hash.
                Ok("missing".to_string())
            }
            Err(error) => Err(format!("cannot read proof target: {error}")),
        }
    }

    fn validate_proof_mark(
        &self,
        agent_id: &str,
        raw_target: &str,
        active: bool,
    ) -> Result<(String, ProofReadyMark), String> {
        let target = self.proof_target_identity(raw_target)?;
        let key = (agent_id.to_string(), target.clone());
        let marks = if active {
            &self.active_proof_permits
        } else {
            &self.proof_ready
        };
        let mark = marks.get(&key).cloned().ok_or_else(|| {
            "proof mark is missing for this agent and exact target scope".to_string()
        })?;
        if mark.target_identity != target {
            return Err("proof target identity no longer matches".to_string());
        }
        if mark.graph_generation != self.graph_generation {
            return Err(format!(
                "proof graph generation is stale (proved={}, current={})",
                mark.graph_generation, self.graph_generation
            ));
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(u64::MAX);
        if now_ms > mark.expires_at_ms {
            return Err("proof mark TTL expired".to_string());
        }
        let disk_digest = Self::proof_target_digest(&target)?;
        if disk_digest != mark.target_digest {
            return Err(format!(
                "proof target digest changed (proved={}, current={})",
                mark.target_digest, disk_digest
            ));
        }
        Ok((target, mark))
    }

    /// Record a generation/digest/TTL-bound, one-shot proof mark. Failure is
    /// explicit: a prover may never report ready while silently failing to bind
    /// the exact disk target.
    pub fn note_proof_ready(
        &mut self,
        agent_id: &str,
        raw_target: &str,
        evidence: &str,
    ) -> Result<ProofReadyMark, String> {
        self.note_proof_ready_inner(agent_id, raw_target, evidence, None)
    }

    /// Production prover entry: additionally requires that the exact bytes the
    /// agent inspected still equal disk at mark creation. This prevents a race
    /// from binding a changed file that was never part of the proof packet.
    pub fn note_proof_ready_for_content(
        &mut self,
        agent_id: &str,
        raw_target: &str,
        evidence: &str,
        inspected_content: &[u8],
    ) -> Result<ProofReadyMark, String> {
        self.note_proof_ready_inner(
            agent_id,
            raw_target,
            evidence,
            Some(format!(
                "sha256:{}",
                crate::util::hex_lower(&Sha256::digest(inspected_content))
            )),
        )
    }

    fn note_proof_ready_inner(
        &mut self,
        agent_id: &str,
        raw_target: &str,
        evidence: &str,
        inspected_digest: Option<String>,
    ) -> Result<ProofReadyMark, String> {
        if agent_id.trim().is_empty() {
            return Err("agent_id is empty".to_string());
        }
        let target_identity = self.proof_target_identity(raw_target)?;
        let target_digest = Self::proof_target_digest(&target_identity)?;
        if let Some(inspected_digest) = inspected_digest {
            if inspected_digest != target_digest {
                return Err(format!(
                    "target changed while proof context was being assembled (inspected={inspected_digest}, current={target_digest})"
                ));
            }
        }
        let proved_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as u64)
            .unwrap_or(0);
        let mark = ProofReadyMark {
            proved_at_ms,
            expires_at_ms: proved_at_ms.saturating_add(PROOF_READY_TTL_MS),
            graph_generation: self.graph_generation,
            target_identity: target_identity.clone(),
            target_digest,
            evidence: Some(evidence.to_string()),
        };
        self.proof_ready
            .insert((agent_id.to_string(), target_identity), mark.clone());
        Ok(mark)
    }

    /// True only while every live binding of the mark still matches disk.
    pub fn is_proof_ready(&self, agent_id: &str, raw_target: &str) -> bool {
        self.validate_proof_mark(agent_id, raw_target, false)
            .is_ok()
    }

    /// Borrow the mark for inspection. This accessor deliberately returns stale
    /// marks too; callers that authorize writes must use validation/consumption.
    pub fn get_proof_ready(&self, agent_id: &str, raw_target: &str) -> Option<&ProofReadyMark> {
        let target = self.proof_target_identity(raw_target).ok()?;
        self.proof_ready.get(&(agent_id.to_string(), target))
    }

    /// Return a fully revalidated proof mark for an internal typed mutation
    /// consumer. This is inspection only: authority and one-shot consumption
    /// remain the responsibility of the caller's trusted dispatch path.
    pub(crate) fn validated_proof_ready_mark(
        &self,
        agent_id: &str,
        raw_target: &str,
    ) -> Result<ProofReadyMark, String> {
        self.validate_proof_mark(agent_id, raw_target, false)
            .map(|(_, mark)| mark)
    }

    /// Atomically validate all targets, then move all marks Ready -> Consumed.
    /// If any target is stale/missing, none are consumed. Returned identities
    /// are the cleanup token for the synchronous dispatcher.
    pub fn consume_proof_ready_targets(
        &mut self,
        agent_id: &str,
        raw_targets: &[String],
    ) -> Result<Vec<String>, String> {
        let mut validated = Vec::with_capacity(raw_targets.len());
        let mut seen = BTreeSet::new();
        for raw_target in raw_targets {
            // Name the exact offending target in the refusal. A source write may
            // touch DERIVED files the caller never named (e.g. a transplant's
            // referencers); naming the unproven one makes the fail-closed refusal
            // actionable — "Run surgical_context_v2 for each exact target" points
            // at a concrete path instead of a generic scope.
            let (identity, mark) = self
                .validate_proof_mark(agent_id, raw_target, false)
                .map_err(|detail| format!("{raw_target}: {detail}"))?;
            if seen.insert(identity.clone()) {
                validated.push((identity, mark));
            }
        }
        if validated.is_empty() {
            return Err("physical write resolved no proof targets".to_string());
        }
        let mut identities = Vec::with_capacity(validated.len());
        for (identity, mark) in validated {
            let key = (agent_id.to_string(), identity.clone());
            self.proof_ready.remove(&key);
            self.active_proof_permits.insert(key, mark);
            identities.push(identity);
        }
        Ok(identities)
    }

    /// Re-check the consumed permit immediately before publishing source bytes.
    pub fn validate_active_proof_permit(
        &self,
        agent_id: &str,
        raw_target: &str,
    ) -> Result<(), String> {
        self.validate_proof_mark(agent_id, raw_target, true)
            .map(|_| ())
    }

    /// Drop dispatcher-scoped consumed permits on both success and failure.
    pub fn clear_active_proof_permits(&mut self, agent_id: &str, identities: &[String]) {
        for identity in identities {
            self.active_proof_permits
                .remove(&(agent_id.to_string(), identity.clone()));
        }
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

fn strict_runtime_relative_path(runtime_root: &Path, path: &Path) -> M1ndResult<String> {
    let relative = path.strip_prefix(runtime_root).map_err(|_| {
        M1ndError::PersistenceFailed(format!(
            "checkpoint-managed path '{}' escapes runtime root '{}'",
            path.display(),
            runtime_root.display()
        ))
    })?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(M1ndError::PersistenceFailed(format!(
            "checkpoint-managed path '{}' is not a strict relative file path",
            path.display()
        )));
    }
    relative.to_str().map(str::to_string).ok_or_else(|| {
        M1ndError::PersistenceFailed(format!(
            "checkpoint-managed path '{}' is not UTF-8",
            path.display()
        ))
    })
}

fn push_checkpoint_candidate_file(
    files: &mut Vec<SessionCheckpointCandidateFile>,
    runtime_root: &Path,
    logical_name: &str,
    path: &Path,
    schema_id: &str,
    schema_version: &str,
    presence: CheckpointCandidatePresence,
) -> M1ndResult<()> {
    if logical_name.trim().is_empty()
        || schema_id.trim().is_empty()
        || schema_version.trim().is_empty()
    {
        return Err(M1ndError::CorruptState {
            reason: "candidate checkpoint metadata contains an empty identifier".into(),
        });
    }
    files.push(SessionCheckpointCandidateFile {
        logical_name: logical_name.to_string(),
        relative_path: strict_runtime_relative_path(runtime_root, path)?,
        schema_id: schema_id.to_string(),
        schema_version: schema_version.to_string(),
        presence,
    });
    Ok(())
}

fn refuse_non_regular_checkpoint_target(path: &Path) -> M1ndResult<()> {
    let parent = path.parent().ok_or_else(|| {
        M1ndError::PersistenceFailed("post-commit derived target has no parent".into())
    })?;
    let parent_metadata = std::fs::symlink_metadata(parent)?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        return Err(M1ndError::PersistenceFailed(format!(
            "post-commit derived target parent '{}' is not a real directory",
            parent.display()
        )));
    }
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(M1ndError::PersistenceFailed(format!(
                "post-commit derived target '{}' is not a regular no-follow file",
                path.display()
            )))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn checkpoint_candidate_digest(files: &[SessionCheckpointCandidateFile]) -> String {
    fn update_field(hasher: &mut Sha256, bytes: &[u8]) {
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }

    let mut ordered = files.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        left.logical_name
            .cmp(&right.logical_name)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    let mut hasher = Sha256::new();
    hasher.update(b"m1nd/session-checkpoint-candidate/v1\0");
    hasher.update((ordered.len() as u64).to_be_bytes());
    for file in ordered {
        update_field(&mut hasher, file.logical_name.as_bytes());
        update_field(&mut hasher, file.relative_path.as_bytes());
        update_field(&mut hasher, file.schema_id.as_bytes());
        update_field(&mut hasher, file.schema_version.as_bytes());
        match &file.presence {
            CheckpointCandidatePresence::Present(bytes) => {
                hasher.update([1]);
                update_field(&mut hasher, bytes);
            }
            CheckpointCandidatePresence::Absent => hasher.update([0]),
        }
    }
    crate::util::hex_lower(&hasher.finalize())
}

fn canonical_json_bytes<T: Serialize>(value: &T) -> M1ndResult<Vec<u8>> {
    fn canonicalize(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.into_iter().map(canonicalize).collect())
            }
            serde_json::Value::Object(values) => {
                let mut entries = values.into_iter().collect::<Vec<_>>();
                entries.sort_by(|left, right| left.0.cmp(&right.0));
                let mut sorted = serde_json::Map::new();
                for (key, value) in entries {
                    sorted.insert(key, canonicalize(value));
                }
                serde_json::Value::Object(sorted)
            }
            scalar => scalar,
        }
    }

    let value = serde_json::to_value(value)?;
    Ok(serde_json::to_vec_pretty(&canonicalize(value))?)
}

pub(crate) fn save_json_atomic<T: Serialize>(path: &Path, value: &T) -> M1ndResult<()> {
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
    use super::{
        basename_of, CheckpointCandidatePresence, ProofReadyMark, SeekFileIndexCache, SessionState,
        FINGERPRINT_INGEST_ROOTS_HEAD, WORKSPACE_ROOT_ENV_CANDIDATES,
    };
    use crate::server::McpConfig;
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::Graph;
    use m1nd_core::types::{EdgeDirection, FiniteF32, NodeId, NodeType};
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, OnceLock};

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

    fn snapshot_regular_files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn visit(root: &Path, current: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
            let mut entries = std::fs::read_dir(current)
                .expect("read snapshot directory")
                .collect::<Result<Vec<_>, _>>()
                .expect("read snapshot entries");
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                let path = entry.path();
                let metadata = std::fs::symlink_metadata(&path).expect("snapshot metadata");
                if metadata.is_dir() {
                    visit(root, &path, files);
                } else if metadata.is_file() {
                    files.insert(
                        path.strip_prefix(root)
                            .expect("relative snapshot path")
                            .to_path_buf(),
                        std::fs::read(path).expect("snapshot file"),
                    );
                }
            }
        }

        let mut files = BTreeMap::new();
        visit(root, root, &mut files);
        files
    }

    fn project_candidate_for_test(runtime: &Path, candidate: &super::SessionCheckpointCandidate) {
        for file in &candidate.files {
            let path = runtime.join(&file.relative_path);
            match &file.presence {
                CheckpointCandidatePresence::Present(bytes) => {
                    std::fs::create_dir_all(path.parent().expect("candidate parent"))
                        .expect("create candidate parent");
                    std::fs::write(path, bytes).expect("project candidate file");
                }
                CheckpointCandidatePresence::Absent => match std::fs::remove_file(path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => panic!("remove candidate absence: {error}"),
                },
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // The durability class guard
    //
    // The brain actor decides "does this turn owe a durable checkpoint?" from an
    // O(1) witness of GRAPH STRUCTURE plus the session generations, from the
    // command's read/write CLASSIFICATION, and from the staged-persist debounce.
    // A verb that writes a durable SIDECAR — the antibody store, the trust
    // ledger, daemon state, the document cache — moves none of those on its own.
    // If it is neither classified a mutation nor routed through a persist choke
    // point, its write is durable to nobody: the ack returns, the debounce
    // counter never advances, and `kill -9` loses it (or resurrects what it
    // deleted). That is exactly how `antibody_create` slipped through.
    //
    // `READ_ONLY_DENIED_TOOLS` was designed as the read-only-attach gate and
    // became, by accident, the durability classifier. These three tests make the
    // accident explicit and make the next omission fail LOUD instead of silent:
    //   1. the sidecar inventory is frozen — a new durable file forces a verdict;
    //   2. every declared writer's route agrees with the live classification;
    //   3. the source is scanned for writers, so a NEW one that forgets the
    //      table fails here instead of shipping a silent durability hole.
    // ─────────────────────────────────────────────────────────────────────────

    /// In-memory owners on `SessionState` whose ONLY durability channel is an
    /// explicit persist request or a mutating classification.
    ///
    /// Deliberately excluded, each because it already has a channel of its own:
    /// `graph` (watched by `DurableWitnessV1`), `plasticity` and `temporal`
    /// (regenerable learning drift, excluded by design — FM-PL-006),
    /// `auto_ingest` (owns `checkpoint_persist_requested`), the boot-KV
    /// inventory (rebuilt from `boot_memory`, which IS listed), and
    /// `verb_usage` — the per-verb call counters own an eager throttled writer
    /// (`VerbUsageLedger::flush_if_due`, the presence-beat shape) and are NOT
    /// brain knowledge: they are telemetry about traffic, so a graph rollback
    /// must not roll back the fact that calls happened, and their declared loss
    /// contract is "the counts start over".
    const DURABLE_SIDECAR_OWNERS: &[&str] = &[
        "antibodies",
        "tremor_registry",
        "trust_ledger",
        "calibration_table",
        "daemon_alerts",
        "daemon_state",
        "ingest_roots",
        "document_cache",
        "document_artifacts",
        "boot_memory",
    ];

    /// The fixed logical names a checkpoint candidate carries for a brain with no
    /// migrated boot-KV lights and no ingested documents. Adding a durable file
    /// without classifying its writers fails `durable_sidecar_inventory_is_frozen`.
    const FROZEN_CHECKPOINT_INVENTORY: &[&str] = &[
        "antibodies",
        "auto_ingest_state",
        "binary_graph_snapshot",
        "boot_config",
        "boot_kv_migration",
        "boot_kv_migration_journal",
        "boot_memory_state",
        "calibration_state",
        "daemon_alerts",
        "daemon_state",
        "document_artifact_inventory",
        "document_cache_index",
        "embeddings_cache",
        "graph_snapshot",
        "ingest_roots",
        "plasticity_state",
        "temporal_state",
        "tremor_state",
        "trust_state",
    ];

    /// How a durable-sidecar writer earns its place in a checkpoint.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum DurabilityRoute {
        /// The verb is in `READ_ONLY_DENIED_TOOLS`: the actor classifies the turn
        /// a mutation and publishes on that very turn.
        ClassifiedMutation,
        /// The write reaches a persist choke point (`persist`,
        /// `persist_daemon_state`, `persist_daemon_alerts`, `persist_boot_memory`,
        /// `AutoIngestState::persist`, or `note_durable_sidecar_drift`), so it
        /// joins the staged-persist debounce. Declared loss window: up to
        /// `auto_persist_interval` deferring turns before the flush.
        StagedPersistDebounce,
        /// Not reached through verb dispatch at all — an owner-side loop whose
        /// writes ride the enclosing tick's own persist.
        OwnerLoop,
    }

    /// Every function outside `#[cfg(test)]` that writes a durable sidecar owner,
    /// with the verb it serves and how that verb earns durability. Kept in lockstep
    /// with the source by `no_undeclared_durable_sidecar_writer_exists`.
    const DURABLE_SIDECAR_WRITERS: &[(&str, &str, DurabilityRoute)] = &[
        // ── classified mutations: published on the turn they are acked ──
        (
            "start",
            "auto_ingest_start",
            DurabilityRoute::ClassifiedMutation,
        ),
        (
            "handle_daemon_start",
            "daemon_start",
            DurabilityRoute::ClassifiedMutation,
        ),
        (
            "handle_antibody_create",
            "antibody_create",
            DurabilityRoute::ClassifiedMutation,
        ),
        (
            "finalize_ingest_with_inventory",
            "ingest",
            DurabilityRoute::ClassifiedMutation,
        ),
        // SPEC-1's freshness door. It writes `ingest_roots` for exactly one
        // reason: to RESTORE the value it captured before committing, so
        // SPEC-1d's root-set invariance holds mechanically even if the finalize
        // path below it ever grows a new root writer. That restore runs inside
        // the same classified `ingest` turn, which is what makes it durable.
        (
            "handle_ingest_refresh",
            "ingest",
            DurabilityRoute::ClassifiedMutation,
        ),
        ("handle_learn", "learn", DurabilityRoute::ClassifiedMutation),
        // ── debounced: durable within the staged-persist window, not on the turn ──
        (
            "tick",
            "auto_ingest_tick",
            DurabilityRoute::StagedPersistDebounce,
        ),
        (
            "handle_boot_memory",
            "boot_memory",
            DurabilityRoute::StagedPersistDebounce,
        ),
        (
            "handle_alerts_ack",
            "alerts_ack",
            DurabilityRoute::StagedPersistDebounce,
        ),
        (
            "handle_daemon_stop",
            "daemon_stop",
            DurabilityRoute::StagedPersistDebounce,
        ),
        (
            "handle_daemon_tick",
            "daemon_tick",
            DurabilityRoute::StagedPersistDebounce,
        ),
        (
            "run_auto_reconcile",
            "daemon_tick",
            DurabilityRoute::StagedPersistDebounce,
        ),
        (
            "settle_auto_reconcile_outcome",
            "daemon_tick",
            DurabilityRoute::StagedPersistDebounce,
        ),
        (
            "handle_antibody_scan",
            "antibody_scan",
            DurabilityRoute::StagedPersistDebounce,
        ),
        (
            "handle_calibrate_envelope",
            "calibrate_envelope",
            DurabilityRoute::StagedPersistDebounce,
        ),
        (
            "handle_calibrate_predict",
            "calibrate_predict",
            DurabilityRoute::StagedPersistDebounce,
        ),
        (
            "document_bindings",
            "document_bindings",
            DurabilityRoute::StagedPersistDebounce,
        ),
        (
            "document_drift",
            "document_drift",
            DurabilityRoute::StagedPersistDebounce,
        ),
        (
            "refresh_document_cache_entry",
            "document_resolve",
            DurabilityRoute::StagedPersistDebounce,
        ),
        (
            "ensure_cache_root_in_ingest_roots",
            "document_resolve",
            DurabilityRoute::StagedPersistDebounce,
        ),
        (
            "daemon_loop_view",
            "daemon_status",
            DurabilityRoute::StagedPersistDebounce,
        ),
        // ── owner-side loops: no verb, covered by the enclosing tick's persist ──
        (
            "persist_daemon_alerts_from_insights",
            "(apply/apply_batch/daemon_tick)",
            DurabilityRoute::OwnerLoop,
        ),
        (
            "refresh_daemon_watcher",
            "(owner daemon loop)",
            DurabilityRoute::OwnerLoop,
        ),
        (
            "run_daemon_tick",
            "(owner daemon loop)",
            DurabilityRoute::OwnerLoop,
        ),
        ("serve", "(owner daemon loop)", DurabilityRoute::OwnerLoop),
    ];

    /// Drop every `#[cfg(test)]` item so the scan only sees shipped code.
    fn strip_cfg_test_items(lines: &[&str]) -> Vec<String> {
        let mut kept = Vec::new();
        let mut index = 0usize;
        while index < lines.len() {
            let trimmed = lines[index].trim_start();
            if trimmed.starts_with("#[cfg(test)]") || trimmed.starts_with("#[cfg(all(test") {
                let mut item = index + 1;
                while item < lines.len()
                    && (lines[item].trim().is_empty() || lines[item].trim_start().starts_with("#["))
                {
                    item += 1;
                }
                if item >= lines.len() {
                    break;
                }
                if lines[item].contains('{') {
                    let mut depth = 0i32;
                    let mut cursor = item;
                    while cursor < lines.len() {
                        depth += lines[cursor].matches('{').count() as i32;
                        depth -= lines[cursor].matches('}').count() as i32;
                        cursor += 1;
                        if depth <= 0 {
                            break;
                        }
                    }
                    index = cursor;
                } else {
                    index = item + 1;
                }
                continue;
            }
            kept.push(lines[index].to_string());
            index += 1;
        }
        kept
    }

    fn collapse_for_scan(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut pending_space = false;
        for ch in text.chars() {
            if ch.is_whitespace() {
                pending_space = !out.is_empty();
                continue;
            }
            if pending_space && ch != '.' && !out.ends_with('.') {
                out.push(' ');
            }
            pending_space = false;
            out.push(ch);
        }
        out
    }

    fn declared_fn_name(line: &str) -> Option<String> {
        let mut rest = line.trim_start();
        for prefix in ["pub(crate) ", "pub(super) ", "pub(in crate) ", "pub "] {
            if let Some(stripped) = rest.strip_prefix(prefix) {
                rest = stripped.trim_start();
            }
        }
        for prefix in ["const ", "async ", "unsafe ", "extern \"C\" "] {
            if let Some(stripped) = rest.strip_prefix(prefix) {
                rest = stripped.trim_start();
            }
        }
        let rest = rest.strip_prefix("fn ")?;
        let name = rest
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .next()?;
        (!name.is_empty()).then(|| name.to_string())
    }

    /// True when the text right after `state.<owner>` is a write: an assignment
    /// into the owner (or a field of it), or a mutating call anywhere along its
    /// field chain. The chain walk stops at the first call, so
    /// `state.ingest_roots.iter().map(..).collect()` is a read and
    /// `state.document_cache.entries.get_mut(..)` is a write.
    fn is_write_after_owner(rest_of_window: &str, rest_of_line: &str) -> bool {
        const MUTATORS: &[&str] = &[
            "push",
            "retain",
            "insert",
            "remove",
            "clear",
            "pop",
            "extend",
            "truncate",
            "drain",
            "sort",
            "sort_by",
            "sort_unstable",
            "iter_mut",
            "get_mut",
            "values_mut",
            "entry",
            "append",
            "dedup",
            "set",
            "record_defect",
            "record_false_alarm",
            "record_partial",
            "record_observation",
        ];
        let mut cursor = rest_of_window;
        while let Some(after_dot) = cursor.strip_prefix('.') {
            let end = after_dot
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(after_dot.len());
            if end == 0 {
                break;
            }
            let (name, tail) = after_dot.split_at(end);
            if tail.starts_with('(') {
                return MUTATORS.contains(&name);
            }
            cursor = tail;
        }
        // Assignment is single-line by construction: `owner.field = value;`.
        let statement = rest_of_line.split(';').next().unwrap_or("");
        let bytes = statement.as_bytes();
        for (index, byte) in bytes.iter().enumerate() {
            if *byte != b'=' {
                continue;
            }
            if matches!(bytes.get(index + 1), Some(b'=') | Some(b'>')) {
                continue;
            }
            if index > 0 && matches!(bytes[index - 1], b'=' | b'!' | b'<' | b'>') {
                continue;
            }
            return true;
        }
        false
    }

    /// Recursive on purpose: real handler code lives in `surgical_handlers/`,
    /// `external_mutation_service/`, `protocol/` and `perspective/` too, and a
    /// scan that only saw the top level would miss a writer placed there.
    fn collect_rust_sources(directory: &Path, found: &mut Vec<PathBuf>) {
        let mut entries = std::fs::read_dir(directory)
            .expect("read crate source directory")
            .map(|entry| entry.expect("source dir entry").path())
            .collect::<Vec<_>>();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                // `internal_tests` is `#[cfg(test)]` at every declaration site.
                if path
                    .file_name()
                    .is_some_and(|name| name == "internal_tests")
                {
                    continue;
                }
                collect_rust_sources(&path, found);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }

    /// Scan the shipped source for functions that write a durable sidecar owner.
    /// `session.rs` and `brain_runtime.rs` are excluded: they ARE the durability
    /// machinery, not consumers of it.
    fn scan_durable_sidecar_writers() -> BTreeMap<String, String> {
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut writers = BTreeMap::new();
        let mut files = Vec::new();
        collect_rust_sources(&source_root, &mut files);
        for path in files {
            let name = path
                .strip_prefix(&source_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            // These two ARE the durability machinery, not consumers of it.
            if name == "session.rs" || name == "brain_runtime.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read crate source file");
            let raw = text.lines().collect::<Vec<_>>();
            let lines = strip_cfg_test_items(&raw);
            let mut current = String::from("<file scope>");
            for index in 0..lines.len() {
                if let Some(declared) = declared_fn_name(&lines[index]) {
                    current = declared;
                }
                let window_end = (index + 3).min(lines.len());
                let window = collapse_for_scan(&lines[index..window_end].join(" "));
                let line = collapse_for_scan(&lines[index]);
                for owner in DURABLE_SIDECAR_OWNERS {
                    let key = format!("state.{owner}");
                    let borrowed = format!("&mut {key}");
                    let hit = window.contains(&borrowed)
                        || window.find(&key).is_some_and(|at| {
                            let after_window = &window[at + key.len()..];
                            let after_line = line
                                .find(&key)
                                .map(|at| &line[at + key.len()..])
                                .unwrap_or("");
                            is_write_after_owner(after_window, after_line)
                        });
                    if hit {
                        writers
                            .entry(current.clone())
                            .or_insert_with(|| format!("{name}:{owner}"));
                        break;
                    }
                }
            }
        }
        writers
    }

    /// A new durable file in the checkpoint inventory must not slip in without a
    /// verdict on who writes it and how that write becomes durable.
    #[test]
    fn durable_sidecar_inventory_is_frozen() {
        let (_temp, _runtime, _registry, state) = strict_recovery_fixture();
        let mut observed = state
            .checkpoint_candidate_files()
            .expect("candidate inventory")
            .into_iter()
            .map(|file| file.logical_name)
            .collect::<Vec<_>>();
        observed.sort();
        let expected = FROZEN_CHECKPOINT_INVENTORY
            .iter()
            .map(|name| name.to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            observed, expected,
            "the durable checkpoint inventory changed. Classify every writer of the \
             new/renamed file in DURABLE_SIDECAR_WRITERS (and add its owner field to \
             DURABLE_SIDECAR_OWNERS) before updating this list"
        );
    }

    /// The declared route of every writer must agree with the live read-only
    /// classification. Dropping a verb from `READ_ONLY_DENIED_TOOLS` — or adding
    /// one and forgetting the table — fails here.
    #[test]
    fn durable_writer_routes_agree_with_the_read_only_classification() {
        for (function, verb, route) in DURABLE_SIDECAR_WRITERS {
            match route {
                DurabilityRoute::ClassifiedMutation => {
                    assert!(
                        crate::server::read_only_denied(verb, &serde_json::json!({})),
                        "{function} writes a durable sidecar and is declared a classified \
                         mutation, but '{verb}' is not in READ_ONLY_DENIED_TOOLS — the write \
                         would be durable to nobody"
                    );
                }
                DurabilityRoute::StagedPersistDebounce => {
                    assert!(
                        !crate::server::read_only_denied(verb, &serde_json::json!({})),
                        "{function} is declared debounced but '{verb}' is now a classified \
                         mutation — promote the row to ClassifiedMutation"
                    );
                    assert!(
                        crate::action_routes::MCP_TOOL_ROUTE_NAMES.contains(verb),
                        "{function} names '{verb}', which is not a routed MCP tool"
                    );
                }
                DurabilityRoute::OwnerLoop => {}
            }
        }
    }

    /// The guard that closes the class: the shipped source is scanned for durable
    /// sidecar writers, and every one must be declared. A new verb that writes a
    /// sidecar and forgets its durability fails HERE, loudly, instead of shipping
    /// an ack that promises a persistence nobody performed.
    ///
    /// HONEST LIMIT — this is a TEXTUAL heuristic, not semantic analysis. It sees
    /// the direct `state.<owner>` write shape that every writer uses today (and
    /// this crate has no `macro_rules!`, so nothing hides behind expansion). It
    /// would NOT see a rebinding writer (`let s = &mut state; s.antibodies…`), a
    /// write behind a differently-named helper, or anything outside
    /// `m1nd-mcp/src`. None of those shapes exists today; if one appears, this
    /// guard goes quiet rather than red. Do not read a green here as proof of
    /// total coverage — read it as proof that the shape we do write is declared.
    #[test]
    fn no_undeclared_durable_sidecar_writer_exists() {
        let observed = scan_durable_sidecar_writers();
        let declared = DURABLE_SIDECAR_WRITERS
            .iter()
            .map(|(function, ..)| *function)
            .collect::<std::collections::BTreeSet<_>>();

        let undeclared = observed
            .iter()
            .filter(|(function, _)| !declared.contains(function.as_str()))
            .map(|(function, evidence)| format!("{function} ({evidence})"))
            .collect::<Vec<_>>();
        assert!(
            undeclared.is_empty(),
            "these functions write a durable checkpoint sidecar but are absent from \
             DURABLE_SIDECAR_WRITERS: {undeclared:?}. Decide how each write becomes durable \
             — a mutating classification in READ_ONLY_DENIED_TOOLS, or a persist choke point \
             (`state.persist()` / `note_durable_sidecar_drift()`) — then declare it"
        );

        let stale = declared
            .iter()
            .filter(|function| !observed.contains_key(**function))
            .collect::<Vec<_>>();
        assert!(
            stale.is_empty(),
            "these DURABLE_SIDECAR_WRITERS rows no longer write a durable sidecar: {stale:?}. \
             Remove them so the table keeps meaning something"
        );
    }

    /// Scan the shipped source for functions that call `note_durable_sidecar_drift`.
    /// Only `session.rs` is excluded — it DEFINES and documents the function, so
    /// every mention there is machinery, not a caller.
    fn scan_staged_drift_callers() -> BTreeMap<String, String> {
        const DRIFT_CALL: &str = "note_durable_sidecar_drift(";

        let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut callers = BTreeMap::new();
        let mut files = Vec::new();
        collect_rust_sources(&source_root, &mut files);
        for path in files {
            let name = path
                .strip_prefix(&source_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            if name == "session.rs" {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read crate source file");
            let raw = text.lines().collect::<Vec<_>>();
            let lines = strip_cfg_test_items(&raw);
            let mut current = String::from("<file scope>");
            for line in &lines {
                if let Some(declared) = declared_fn_name(line) {
                    current = declared;
                }
                if collapse_for_scan(line).contains(DRIFT_CALL) {
                    // File only, like the sibling scan: `lines` is the cfg(test)-stripped
                    // view, so its index is NOT the line number in the real file.
                    callers
                        .entry(current.clone())
                        .or_insert_with(|| name.clone());
                }
            }
        }
        callers
    }

    /// The oracle for the drift note itself, and the reason it is not a runtime
    /// assert inside `note_durable_sidecar_drift`.
    ///
    /// Outside an actor stage that function is a deliberate no-op, so a caller
    /// reached from boot, a CLI path, or a spawned task would lose its drift in
    /// silence. A stage check at the call site cannot catch that: a pre-actor
    /// session is indistinguishable from a bare one, and the bare shape is
    /// legitimate — the crate's own tests drive these handlers without an actor
    /// precisely to exercise the unstaged direct-persist path.
    ///
    /// So the guard is here instead, and it is stronger: every shipped caller must
    /// be declared in `DURABLE_SIDECAR_WRITERS`, which
    /// `durable_writer_routes_agree_with_the_read_only_classification` forces to
    /// name a real routed verb. A boot/CLI/spawn caller has no verb to name, so it
    /// fails when it is WRITTEN rather than only if some test happens to run it.
    #[test]
    fn no_undeclared_staged_drift_caller_exists() {
        let observed = scan_staged_drift_callers();
        assert!(
            !observed.is_empty(),
            "the drift-caller scan found nothing — it has gone blind (renamed function \
             or moved call shape), which would make this guard silently useless"
        );

        let declared = DURABLE_SIDECAR_WRITERS
            .iter()
            .map(|(function, ..)| *function)
            .collect::<std::collections::BTreeSet<_>>();
        let undeclared = observed
            .iter()
            .filter(|(function, _)| !declared.contains(function.as_str()))
            .map(|(function, evidence)| format!("{function} ({evidence})"))
            .collect::<Vec<_>>();
        assert!(
            undeclared.is_empty(),
            "these functions note durable sidecar drift but are absent from \
             DURABLE_SIDECAR_WRITERS: {undeclared:?}. The note is a NO-OP outside an actor \
             stage, so declare the verb this runs under — if there is none, this caller is \
             reached from boot, a CLI path, or a spawned task and its drift is lost: route it \
             through a staged actor turn instead"
        );
    }

    fn strict_recovery_fixture() -> (tempfile::TempDir, PathBuf, PathBuf, SessionState) {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = temp.path().join("runtime");
        let registry = temp.path().join("registry");
        let config = McpConfig {
            graph_source: runtime.join("graph_snapshot.json"),
            plasticity_state: runtime.join("plasticity_state.json"),
            runtime_dir: Some(runtime.clone()),
            registry_dir: Some(registry.clone()),
            ..McpConfig::default()
        };
        let mut graph = Graph::new();
        graph
            .add_node("node::source", "source", NodeType::Function, &[], 1.0, 0.5)
            .expect("source node");
        graph
            .add_node("node::target", "target", NodeType::Function, &[], 1.0, 0.5)
            .expect("target node");
        graph
            .add_edge(
                NodeId::new(0),
                NodeId::new(1),
                "calls",
                FiniteF32::new(0.8),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.5),
            )
            .expect("edge");
        graph.finalize().expect("finalize");
        let mut state = SessionState::initialize(graph, &config, DomainConfig::code())
            .expect("initialize strict fixture");
        state.daemon_state.tick_in_flight = true;
        state.daemon_state.pending_rerun = true;
        state.daemon_state.watch_backend = "native_fs".into();
        let stage = state
            .begin_checkpoint_staging()
            .expect("stage strict fixture candidate");
        let candidate = state
            .checkpoint_candidate(&stage)
            .expect("strict fixture candidate");
        project_candidate_for_test(&runtime, &candidate);
        state
            .finish_checkpoint_staging(stage)
            .expect("finish strict fixture candidate");
        (temp, runtime, registry, state)
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
    fn candidate_staging_suppresses_working_writes_and_is_deterministic() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = temp.path().join("runtime");
        let graph_path = runtime.join("graph.json");
        let daemon_path = runtime.join("daemon_state.json");
        let auto_ingest_path = runtime.join("auto_ingest_state.json");
        let binary_path = runtime.join("graph.bin");
        let config = McpConfig {
            graph_source: graph_path.clone(),
            plasticity_state: runtime.join("plasticity.json"),
            runtime_dir: Some(runtime.clone()),
            ..McpConfig::default()
        };
        let mut graph = Graph::new();
        graph
            .add_node("file::one", "one", NodeType::File, &[], 0.0, 0.0)
            .expect("add node");
        graph.finalize().expect("finalize");
        let mut state = SessionState::initialize(graph, &config, DomainConfig::code())
            .expect("initialize session");

        let stage = state
            .begin_checkpoint_staging()
            .expect("begin checkpoint staging");
        assert!(state.begin_checkpoint_staging().is_err());
        state.persist().expect("staged full persist");
        state
            .persist_daemon_state()
            .expect("staged granular persist");
        state
            .auto_ingest
            .persist(&runtime)
            .expect("staged auto-ingest persist");
        let before_binary = state
            .checkpoint_candidate(&stage)
            .expect("candidate before derived export");
        assert_eq!(
            state
                .persist_binary_snapshot()
                .expect("queue binary snapshot"),
            binary_path
        );
        assert!(!graph_path.exists());
        assert!(!daemon_path.exists());
        assert!(!auto_ingest_path.exists());
        assert!(!binary_path.exists());

        let first = state.checkpoint_candidate(&stage).expect("first candidate");
        let second = state
            .checkpoint_candidate(&stage)
            .expect("second candidate");
        assert_eq!(first, second);
        assert_eq!(
            before_binary.state_digest, first.state_digest,
            "a derived post-CURRENT binary export is not part of the authoritative candidate digest"
        );
        assert!(first.persist_requested);
        assert_eq!(first.state_digest.len(), 64);
        assert!(matches!(
            first
                .files
                .iter()
                .find(|file| file.logical_name == "document_artifact_inventory")
                .expect("explicit document artifact inventory candidate")
                .presence,
            CheckpointCandidatePresence::Present(_)
        ));
        let graph_file = first
            .files
            .iter()
            .find(|file| file.logical_name == "graph_snapshot")
            .expect("graph candidate");
        let CheckpointCandidatePresence::Present(graph_bytes) = &graph_file.presence else {
            panic!("graph must be present")
        };
        let snapshot: serde_json::Value = serde_json::from_slice(graph_bytes).expect("graph JSON");
        assert_eq!(snapshot["version"], m1nd_core::snapshot::SNAPSHOT_VERSION);
        assert!(matches!(
            first
                .files
                .iter()
                .find(|file| file.logical_name == "embeddings_cache")
                .expect("explicit derived cache decision")
                .presence,
            CheckpointCandidatePresence::Absent
        ));

        assert_eq!(
            state
                .apply_staged_post_commit_effects(&stage)
                .expect("apply derived exports"),
            1
        );
        assert_eq!(
            m1nd_core::snapshot_bin::load_graph(&binary_path)
                .expect("load staged binary")
                .num_nodes(),
            1
        );
        assert!(state
            .finish_checkpoint_staging(stage)
            .expect("finish staging"));
        state.persist().expect("direct persist after staging");
        assert!(graph_path.exists());
        assert!(daemon_path.exists());
        assert!(auto_ingest_path.exists());
    }

    #[test]
    fn checkpoint_stage_clone_retains_capability_but_cannot_finish_twice() {
        let (_temp, _runtime, _registry, mut state) = strict_recovery_fixture();
        let stage = state.begin_checkpoint_staging().expect("begin stage");
        let retained = stage.clone();
        assert_eq!(
            state
                .checkpoint_candidate(&stage)
                .expect("original token")
                .state_digest,
            state
                .checkpoint_candidate(&retained)
                .expect("retained token")
                .state_digest
        );
        state
            .finish_checkpoint_staging(stage)
            .expect("finish original token");
        assert!(state.checkpoint_candidate(&retained).is_err());
    }

    #[test]
    fn strict_reload_is_pure_reuses_instance_and_matches_working_set_digest() {
        let (_temp, runtime, registry, mut state) = strict_recovery_fixture();
        let expected_digest = state
            .authoritative_checkpoint_state_digest()
            .expect("authoritative digest before mutation");
        let instance_id = state.instance.summary().instance_id;
        let registry_entry = registry
            .join("instances")
            .join(format!("{instance_id}.json"));
        let registry_bytes = std::fs::read(&registry_entry).expect("registry entry");
        let runtime_before = snapshot_regular_files(&runtime);

        // Explicit durable daemon bytes must be restored exactly. These three
        // values are sanitized only by friendly boot, never by checkpoint
        // recovery, because changing them would invalidate the stored digest.
        state.daemon_state.tick_in_flight = false;
        state.daemon_state.pending_rerun = false;
        state.daemon_state.watch_backend = "polling".into();
        state.ingest_roots.push("postimage-only".into());
        state.seek_file_index = Some(SeekFileIndexCache::default());
        state.active_proof_permits.insert(
            ("agent".into(), "target".into()),
            ProofReadyMark {
                proved_at_ms: 1,
                expires_at_ms: 2,
                graph_generation: state.graph_generation,
                target_identity: "target".into(),
                target_digest: "missing".into(),
                evidence: Some("test".into()),
            },
        );
        let _abandoned_stage = state.begin_checkpoint_staging().expect("abandoned stage");

        state
            .reload_authoritative_from_disk(false)
            .expect("strict authoritative reload");

        assert_eq!(state.instance.summary().instance_id, instance_id);
        assert_eq!(
            std::fs::read(&registry_entry).expect("registry entry after reload"),
            registry_bytes,
            "strict reload must not heartbeat or rewrite the registry"
        );
        assert_eq!(snapshot_regular_files(&runtime), runtime_before);
        assert!(state.daemon_state.tick_in_flight);
        assert!(state.daemon_state.pending_rerun);
        assert_eq!(state.daemon_state.watch_backend, "native_fs");
        assert!(state.seek_file_index.is_none());
        assert!(state.active_proof_permits.is_empty());
        assert!(state.persistence_stage.get().is_none());
        assert_eq!(
            state
                .authoritative_checkpoint_state_digest()
                .expect("digest after rebuild"),
            expected_digest
        );
    }

    #[test]
    fn strict_reload_is_all_or_nothing_on_corrupt_current_sidecar() {
        let (_temp, _runtime, registry, mut state) = strict_recovery_fixture();
        state.ingest_roots.push("uncommitted-postimage".into());
        let postimage_digest = state
            .authoritative_checkpoint_state_digest()
            .expect("postimage digest");
        let instance_id = state.instance.summary().instance_id;
        let registry_entry = registry
            .join("instances")
            .join(format!("{instance_id}.json"));
        let registry_bytes = std::fs::read(&registry_entry).expect("registry entry");
        std::fs::write(&state.trust_path, b"{not-json").expect("corrupt trust sidecar");

        state
            .reload_authoritative_from_disk(false)
            .expect_err("corrupt required sidecar must fail closed");
        assert_eq!(
            state.ingest_roots.last().map(String::as_str),
            Some("uncommitted-postimage")
        );
        assert_eq!(
            state
                .authoritative_checkpoint_state_digest()
                .expect("failed reload leaves live state untouched"),
            postimage_digest
        );
        assert_eq!(state.instance.summary().instance_id, instance_id);
        assert_eq!(
            std::fs::read(registry_entry).expect("registry after failed reload"),
            registry_bytes
        );
    }

    #[test]
    fn strict_reload_rejects_every_missing_required_current_sidecar() {
        let (_temp, runtime, _registry, mut state) = strict_recovery_fixture();
        let required = [
            state.graph_path.clone(),
            runtime.join("ingest_roots.json"),
            state.plasticity_path.clone(),
            state.antibodies_path.clone(),
            state.tremor_path.clone(),
            state.trust_path.clone(),
            state.calibration_path.clone(),
            state.temporal_state_path.clone(),
            state.daemon_state_path.clone(),
            state.daemon_alerts_path.clone(),
            runtime.join("auto_ingest_state.json"),
            runtime.join("document_cache_index.json"),
            crate::universal_docs::document_artifact_inventory_path(&runtime),
        ];
        for (index, path) in required.into_iter().enumerate() {
            let backup = runtime.join(format!("missing-required-{index}.bak"));
            std::fs::rename(&path, &backup).expect("hide required sidecar");
            let result = state.reload_authoritative_from_disk(false);
            std::fs::rename(&backup, &path).expect("restore required sidecar");
            assert!(
                result.is_err(),
                "missing required sidecar was accepted: {}",
                path.display()
            );
        }
    }

    #[test]
    fn strict_reload_rejects_incomplete_plasticity_and_lossy_auto_ingest() {
        let (_temp, runtime, _registry, mut state) = strict_recovery_fixture();
        let plasticity: serde_json::Value = serde_json::from_slice(
            &std::fs::read(&state.plasticity_path).expect("plasticity bytes"),
        )
        .expect("plasticity value");
        let empty = serde_json::to_vec_pretty(&serde_json::json!([])).expect("empty rows");
        std::fs::write(&state.plasticity_path, empty).expect("incomplete plasticity");
        assert!(state.reload_authoritative_from_disk(false).is_err());
        std::fs::write(
            &state.plasticity_path,
            serde_json::to_vec_pretty(&plasticity).expect("plasticity restore"),
        )
        .expect("restore plasticity");

        let auto_path = runtime.join("auto_ingest_state.json");
        let mut auto: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&auto_path).expect("auto-ingest bytes"))
                .expect("auto-ingest value");
        auto["future_field"] = serde_json::json!(true);
        std::fs::write(
            &auto_path,
            serde_json::to_vec_pretty(&auto).expect("auto-ingest json"),
        )
        .expect("write lossy auto-ingest");
        assert!(state.reload_authoritative_from_disk(false).is_err());
    }

    #[test]
    fn candidate_refuses_paths_outside_runtime_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = temp.path().join("runtime");
        let config = McpConfig {
            graph_source: temp.path().join("outside-graph.json"),
            plasticity_state: runtime.join("plasticity.json"),
            runtime_dir: Some(runtime),
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");
        let stage = state.begin_checkpoint_staging().expect("begin staging");
        let error = state
            .checkpoint_candidate(&stage)
            .expect_err("external graph path must be refused");
        assert!(error.to_string().contains("escapes runtime root"));
    }

    #[test]
    fn read_only_session_can_use_staging_as_a_non_writing_snapshot_fence() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = McpConfig {
            graph_source: temp.path().join("graph.json"),
            plasticity_state: temp.path().join("plasticity.json"),
            runtime_dir: Some(temp.path().to_path_buf()),
            read_only: true,
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize read-only session");
        let stage = state
            .begin_checkpoint_staging()
            .expect("read snapshot fence");
        state.persist().expect("read-only persist remains a no-op");
        let candidate = state
            .checkpoint_candidate(&stage)
            .expect("detached read witness");
        assert!(!candidate.persist_requested);
        assert!(!config.graph_source.exists());
        assert!(!state
            .finish_checkpoint_staging(stage)
            .expect("finish read fence"));
    }

    #[test]
    fn graph_rebind_neutralizes_an_escaped_shared_graph_arc() {
        let temp = tempfile::tempdir().expect("tempdir");
        let config = McpConfig {
            graph_source: temp.path().join("graph.json"),
            plasticity_state: temp.path().join("plasticity.json"),
            runtime_dir: Some(temp.path().to_path_buf()),
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");
        let escaped = Arc::clone(&state.graph);
        state.rebind_detached_graph().expect("deep-clone graph");
        assert!(!Arc::ptr_eq(&escaped, &state.graph));
        escaped
            .write()
            .add_node("escaped", "escaped", NodeType::File, &[], 0.0, 0.0)
            .expect("mutate detached graph");
        assert_eq!(escaped.read().num_nodes(), 1);
        assert_eq!(state.graph.read().num_nodes(), 0);
    }

    #[test]
    fn session_persist_and_restart_restore_both_temporal_matrices() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime).expect("runtime");
        let config = McpConfig {
            graph_source: runtime.join("graph_snapshot.json"),
            plasticity_state: runtime.join("plasticity_state.json"),
            runtime_dir: Some(runtime.clone()),
            registry_dir: Some(temp.path().join("registry")),
            ..McpConfig::default()
        };
        let mut graph = Graph::new();
        for index in 0..3 {
            graph
                .add_node(
                    &format!("node_{index}"),
                    &format!("node_{index}"),
                    NodeType::File,
                    &[],
                    0.0,
                    0.0,
                )
                .expect("add node");
        }
        graph.finalize().expect("finalize");

        let mut first = SessionState::initialize(graph, &config, DomainConfig::code())
            .expect("first initialize");
        for _ in 0..3 {
            first
                .temporal
                .co_change
                .note_node_appearance(NodeId::new(0));
            first
                .temporal
                .co_change
                .note_node_appearance(NodeId::new(1));
            first
                .temporal
                .co_change
                .record_co_change(NodeId::new(0), NodeId::new(1), 0.0)
                .expect("learn primary");
        }
        for _ in 0..6 {
            first
                .orchestrator
                .temporal
                .co_change
                .note_node_appearance(NodeId::new(0));
            first
                .orchestrator
                .temporal
                .co_change
                .note_node_appearance(NodeId::new(2));
            first
                .orchestrator
                .temporal
                .co_change
                .record_co_change(NodeId::new(0), NodeId::new(2), 0.0)
                .expect("learn orchestrator");
        }
        let expected_primary = first.temporal.co_change.predict(NodeId::new(0), 8);
        let expected_orchestrator = first
            .orchestrator
            .temporal
            .co_change
            .predict(NodeId::new(0), 8);
        first.persist().expect("persist complete session");
        drop(first);

        let restored_graph =
            m1nd_core::snapshot::load_graph(&config.graph_source).expect("load persisted graph");
        let restored = SessionState::initialize(restored_graph, &config, DomainConfig::code())
            .expect("restart initialize");
        assert_eq!(
            restored.temporal.co_change.predict(NodeId::new(0), 8),
            expected_primary
        );
        assert_eq!(
            restored
                .orchestrator
                .temporal
                .co_change
                .predict(NodeId::new(0), 8),
            expected_orchestrator
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
        state.workspace_root = Some("/Users/<name>/solo-repo".to_string());
        assert_eq!(state.display_name().as_deref(), Some("solo-repo"));
    }

    // #326-family auto-heal: a brain whose workspace_root was demoted onto its
    // agent-memory store dir (the field-reported bound-owner flip) is repaired to
    // the real code root when one survives in the ingest roots.
    #[test]
    fn heal_workspace_root_undemotes_flipped_bound_brain() {
        let temp = tempfile::tempdir().expect("tempdir");
        let code_root = temp.path().join("m1nd");
        std::fs::create_dir_all(&code_root).expect("code root");
        let agent_memory = temp
            .path()
            .join("runtimes")
            .join("claude")
            .join("agent-memory");
        std::fs::create_dir_all(&agent_memory).expect("agent-memory dir");

        let config = McpConfig {
            graph_source: temp.path().join("graph.json"),
            plasticity_state: temp.path().join("plasticity.json"),
            runtime_dir: Some(temp.path().to_path_buf()),
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");

        // Arrange the flipped production state: workspace_root = the store dir,
        // the real code root still present among the ingest roots.
        state.workspace_root = Some(agent_memory.to_string_lossy().to_string());
        state.ingest_roots = vec![
            code_root.to_string_lossy().to_string(),
            agent_memory.to_string_lossy().to_string(),
        ];

        state.heal_workspace_root();

        assert_eq!(
            state.workspace_root.as_deref(),
            Some(code_root.to_string_lossy().as_ref()),
            "the flipped workspace_root must be healed back to the code root"
        );
    }

    // The heal is self-limiting: a genuine pure-memory / medulla store (no code
    // root in its ingest roots) keeps its sidecar workspace_root untouched.
    #[test]
    fn heal_workspace_root_is_noop_without_a_code_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let agent_memory = temp.path().join("agent-memory");
        std::fs::create_dir_all(&agent_memory).expect("agent-memory dir");

        let config = McpConfig {
            graph_source: temp.path().join("graph.json"),
            plasticity_state: temp.path().join("plasticity.json"),
            runtime_dir: Some(temp.path().to_path_buf()),
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");
        state.workspace_root = Some(agent_memory.to_string_lossy().to_string());
        state.ingest_roots = vec![agent_memory.to_string_lossy().to_string()];

        state.heal_workspace_root();

        assert_eq!(
            state.workspace_root.as_deref(),
            Some(agent_memory.to_string_lossy().as_ref()),
            "a pure-memory store with no code root keeps its sidecar workspace_root"
        );
    }

    // The heal is wired into the boot/load seam: a brain that boots with a
    // store-dir workspace_root (graph under `agent-memory`) but whose persisted
    // ingest_roots carry a real code root is healed by `initialize` itself,
    // de-flipping the corrupted bound owner on its next boot.
    #[test]
    fn initialize_heals_flipped_workspace_root_from_ingest_roots() {
        let temp = tempfile::tempdir().expect("tempdir");
        let code_root = temp.path().join("m1nd");
        std::fs::create_dir_all(&code_root).expect("code root");
        // The graph lives inside the agent-memory store dir, so the boot-time
        // inference sets workspace_root to that sidecar (the flipped shape).
        let agent_memory = temp.path().join("agent-memory");
        std::fs::create_dir_all(&agent_memory).expect("agent-memory dir");
        // Persist ingest_roots beside the graph (what `load_ingest_roots` reads).
        std::fs::write(
            agent_memory.join("ingest_roots.json"),
            serde_json::to_string(&vec![
                code_root.to_string_lossy().to_string(),
                agent_memory.to_string_lossy().to_string(),
            ])
            .expect("serialize roots"),
        )
        .expect("write ingest_roots.json");

        let config = McpConfig {
            graph_source: agent_memory.join("graph.json"),
            plasticity_state: agent_memory.join("plasticity.json"),
            runtime_dir: Some(agent_memory.clone()),
            ..McpConfig::default()
        };
        let state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");

        assert_eq!(
            state.workspace_root.as_deref(),
            Some(code_root.to_string_lossy().as_ref()),
            "initialize must heal the store-dir workspace_root to the code root at boot"
        );
    }

    #[test]
    fn basename_of_is_separator_agnostic() {
        // POSIX still works (the '/' path).
        assert_eq!(basename_of("/path/to/repo"), "repo");
        assert_eq!(basename_of("~/m1nd"), "m1nd");
        assert_eq!(basename_of("/path/to/repo/"), "repo", "trailing slash");
        // Windows backslash paths: the chronic red Windows CI case — a '\\'
        // separator must name the same repo a '/' separator does.
        assert_eq!(
            basename_of(r"C:\Users\<name>\m1nd"),
            "m1nd",
            "backslash path must yield the repo basename, not the whole string"
        );
        assert_eq!(
            basename_of(r"C:\Users\<name>\m1nd\"),
            "m1nd",
            "trailing backslash tolerated"
        );
        // UNC path (\\server\share\repo).
        assert_eq!(basename_of(r"\\server\share\repo"), "repo", "UNC path");
        // Mixed separators (some tools emit these on Windows).
        assert_eq!(
            basename_of(r"C:\Users\<name>/m1nd"),
            "m1nd",
            "mixed separators"
        );
        // A rootless bare name is returned unchanged (honest, never a panic).
        assert_eq!(basename_of("repo"), "repo");
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

    /// Build a bare medulla session for the reception tests. Returns the state,
    /// the tempdir (kept alive), plus the created brain_root + caller_root dirs.
    fn reception_state() -> (
        tempfile::TempDir,
        SessionState,
        std::path::PathBuf,
        std::path::PathBuf,
    ) {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = temp.path().join("runtime");
        let brain_root = temp.path().join("repo-alpha");
        let caller_root = temp.path().join("repo-beta");
        std::fs::create_dir_all(&runtime).expect("runtime dir");
        std::fs::create_dir_all(&brain_root).expect("brain root");
        std::fs::create_dir_all(&caller_root).expect("caller root");
        let config = McpConfig {
            graph_source: runtime.join("graph_snapshot.json"),
            plasticity_state: runtime.join("plasticity_state.json"),
            runtime_dir: Some(runtime),
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");
        state.workspace_root = Some(brain_root.to_string_lossy().to_string());
        state.ingest_roots = vec![brain_root.to_string_lossy().to_string()];
        (temp, state, brain_root, caller_root)
    }

    /// P1: a medulla store whose caller root no project brain covers stamps the
    /// canonical `project_brain_absent` label ADDITIVELY — every field the two
    /// downstream consumers read (`human_view`: honest/caller_root/bound_workspace;
    /// `enrich_reception_with_roster`: match + the bootstrap_unavailable option)
    /// stays in place. The continue option is reframed to trust the doctrine.
    #[test]
    fn reception_verdict_medulla_mismatch_labels_project_brain_absent_additively() {
        let (_temp, mut state, brain_root, caller_root) = reception_state();
        state.workspace_root_source = None; // the medulla is not a project manifest
        state.caller_root = Some(caller_root.to_string_lossy().to_string());
        assert!(state.is_medulla_store());
        assert!(state.caller_root_is_brainless());

        let r = state.reception_verdict().expect("mismatch reception");
        // Additive: the contract fields are all preserved.
        assert_eq!(r["match"], "caller_root_mismatch");
        assert_eq!(r["caller_root"], caller_root.to_string_lossy().as_ref());
        assert_eq!(r["bound_workspace"], brain_root.to_string_lossy().as_ref());
        assert_eq!(
            r["honest"], "this graph does NOT cover your repo",
            "the honest CODE-coverage line is byte-stable (human_view pins it)"
        );
        // The canonical label + medulla-served signal.
        assert_eq!(r["project_brain_absent"], true);
        assert_eq!(r["medulla_served"], true);
        let opts = r["options"].as_array().expect("options array");
        assert!(
            opts.iter().any(|o| o["action"] == "bootstrap_unavailable"),
            "the roster-enrich seam still finds its option: {r}"
        );
        let cont = opts
            .iter()
            .find(|o| o["action"] == "continue_bound")
            .expect("continue_bound option");
        assert!(
            cont["note"]
                .as_str()
                .unwrap()
                .contains("legitimate cross-project source"),
            "the continue option trusts the doctrine, distrusts only code: {cont}"
        );
    }

    /// P1: a genuine PROJECT-brain misbind (a manifest-source store whose caller
    /// root it does not cover) is NOT the medulla fallback — it keeps the plain
    /// "don't trust" block and never claims `project_brain_absent`.
    #[test]
    fn reception_verdict_project_brain_mismatch_stays_a_plain_distrust_block() {
        let (_temp, mut state, _brain_root, caller_root) = reception_state();
        state.workspace_root_source = Some("project_brain_manifest".into());
        state.caller_root = Some(caller_root.to_string_lossy().to_string());
        assert!(!state.is_medulla_store());
        assert!(
            !state.caller_root_is_brainless(),
            "a project brain is never brainless"
        );

        let r = state.reception_verdict().expect("mismatch reception");
        assert_eq!(r["match"], "caller_root_mismatch");
        assert!(
            r.get("project_brain_absent").is_none(),
            "a project-brain misbind is not project_brain_absent: {r}"
        );
        assert!(r.get("medulla_served").is_none());
        let opts = r["options"].as_array().expect("options array");
        let cont = opts
            .iter()
            .find(|o| o["action"] == "continue_bound")
            .expect("continue_bound option");
        assert!(
            cont["note"]
                .as_str()
                .unwrap()
                .contains("verify against local files"),
            "the plain distrust wording is intact: {cont}"
        );
    }

    /// P1: a covered caller gets silence (TT-INV-12) and is never brainless.
    #[test]
    fn reception_verdict_covered_caller_is_silent() {
        let (_temp, mut state, brain_root, _caller_root) = reception_state();
        state.caller_root = Some(brain_root.to_string_lossy().to_string());
        assert!(state.covers_root(&brain_root.to_string_lossy()));
        assert!(!state.caller_root_is_brainless());
        assert!(
            state.reception_verdict().is_none(),
            "a covered caller flows silently (TT-INV-12)"
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

    fn proof_test_state(root: &Path) -> SessionState {
        let runtime = root.join(".m1nd-test-runtime");
        std::fs::create_dir_all(&runtime).expect("runtime");
        let config = McpConfig {
            graph_source: runtime.join("graph.json"),
            plasticity_state: runtime.join("plasticity.json"),
            runtime_dir: Some(runtime),
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize proof state");
        state.ingest_roots = vec![root.to_string_lossy().into_owned()];
        state.workspace_root = Some(root.to_string_lossy().into_owned());
        state
    }

    #[test]
    fn proof_mark_binds_digest_generation_ttl_agent_and_exact_scope() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("src/lib.rs");
        std::fs::create_dir_all(target.parent().expect("parent")).expect("src");
        std::fs::write(&target, "pub fn one() {}\n").expect("target");
        let mut state = proof_test_state(temp.path());

        let raced = state
            .note_proof_ready_for_content(
                "agent-a",
                &target.to_string_lossy(),
                "test",
                b"stale inspected bytes\n",
            )
            .expect_err("uninspected current bytes must not be marked");
        assert!(raced.contains("changed while proof context"));

        let mark = state
            .note_proof_ready("agent-a", &target.to_string_lossy(), "test")
            .expect("proof mark");
        assert!(mark.target_digest.starts_with("sha256:"));
        assert_eq!(mark.graph_generation, state.graph_generation);
        assert!(mark.expires_at_ms > mark.proved_at_ms);
        assert!(state.is_proof_ready("agent-a", &target.to_string_lossy()));
        assert!(!state.is_proof_ready("agent-b", &target.to_string_lossy()));

        std::fs::write(&target, "pub fn changed() {}\n").expect("mutate target");
        assert!(!state.is_proof_ready("agent-a", &target.to_string_lossy()));
        assert!(state
            .consume_proof_ready_targets("agent-a", &[target.to_string_lossy().into_owned()],)
            .expect_err("changed digest must refuse")
            .contains("digest changed"));
    }

    #[test]
    fn proof_mark_consumption_is_atomic_and_one_shot() {
        let temp = tempfile::tempdir().expect("tempdir");
        let first = temp.path().join("first.rs");
        let second = temp.path().join("second.rs");
        std::fs::write(&first, "one\n").expect("first");
        std::fs::write(&second, "two\n").expect("second");
        let mut state = proof_test_state(temp.path());
        state
            .note_proof_ready("agent", &first.to_string_lossy(), "test")
            .expect("first proof");

        let error = state
            .consume_proof_ready_targets(
                "agent",
                &[
                    first.to_string_lossy().into_owned(),
                    second.to_string_lossy().into_owned(),
                ],
            )
            .expect_err("all-or-none consumption");
        assert!(error.contains("missing"));
        assert!(state.is_proof_ready("agent", &first.to_string_lossy()));

        state
            .note_proof_ready("agent", &second.to_string_lossy(), "test")
            .expect("second proof");
        let identities = state
            .consume_proof_ready_targets(
                "agent",
                &[
                    first.to_string_lossy().into_owned(),
                    second.to_string_lossy().into_owned(),
                ],
            )
            .expect("consume both");
        assert!(!state.is_proof_ready("agent", &first.to_string_lossy()));
        state
            .validate_active_proof_permit("agent", &first.to_string_lossy())
            .expect("active permit");
        state.clear_active_proof_permits("agent", &identities);
        assert!(state
            .validate_active_proof_permit("agent", &first.to_string_lossy())
            .is_err());
        assert!(state
            .consume_proof_ready_targets("agent", &[first.to_string_lossy().into_owned()])
            .is_err());
    }

    #[test]
    fn proof_mark_generation_bump_and_ttl_expiry_invalidate() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("lib.rs");
        std::fs::write(&target, "one\n").expect("target");
        let mut state = proof_test_state(temp.path());
        let mark = state
            .note_proof_ready("agent", &target.to_string_lossy(), "test")
            .expect("proof");
        state
            .proof_ready
            .get_mut(&("agent".to_string(), mark.target_identity.clone()))
            .expect("stored mark")
            .expires_at_ms = 0;
        assert!(!state.is_proof_ready("agent", &target.to_string_lossy()));

        state
            .note_proof_ready("agent", &target.to_string_lossy(), "test")
            .expect("fresh proof");
        state.bump_graph_generation();
        assert!(!state.is_proof_ready("agent", &target.to_string_lossy()));
        assert!(state.proof_ready.is_empty());
    }

    // ---------------------------------------------------------------------
    // Budget Law (§C1.3.4) — the binding fingerprint is a FIXED-COST identity
    // block. It rides on every `north`, the verb doctrine makes every agent
    // call first, so any variable-size array inside it is a per-call tax.
    // ---------------------------------------------------------------------

    fn state_with_ingest_roots(temp: &Path, count: usize) -> SessionState {
        let config = McpConfig {
            graph_source: temp.join("graph_snapshot.json"),
            plasticity_state: temp.join("plasticity_state.json"),
            runtime_dir: Some(temp.to_path_buf()),
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("initialize session");
        // Field-shaped roots: ~70 bytes each, the average measured on the live
        // owner (25,907 bytes / 380 entries).
        state.ingest_roots = (0..count)
            .map(|index| {
                format!(
                    "/srv/workspaces/project-alpha/crates/engine/src/module_{index:04}/handlers.rs"
                )
            })
            .collect();
        state
    }

    /// RED-first guard for the packet budget. Measured on the live owner
    /// 2026-07-24: 380 ingest roots serialized to 25,907 bytes (~6.5k tokens)
    /// inside `binding_fingerprint`, burned on EVERY `north` call — and 360 of
    /// those entries were individual files, not repo roots. The fingerprint's
    /// job is binding identity, so it carries a stable HEAD of the roots plus
    /// the real total; the omission is declared, never silent.
    #[test]
    fn binding_fingerprint_ingest_roots_stay_within_packet_budget() {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = state_with_ingest_roots(temp.path(), 400);

        let fingerprint = state.binding_fingerprint();
        let serialized = serde_json::to_string(&fingerprint).expect("serialize fingerprint");
        assert!(
            serialized.len() < 4_000,
            "binding_fingerprint must stay under the 4,000-byte packet budget with 400 \
             ingest roots, got {} bytes",
            serialized.len()
        );

        // Honest truncation: head kept, total always stated, omission declared.
        let head = fingerprint["ingest_roots"]
            .as_array()
            .expect("ingest_roots array");
        assert_eq!(head.len(), FINGERPRINT_INGEST_ROOTS_HEAD);
        assert_eq!(head[0], state.ingest_roots[0]);
        assert_eq!(
            head[FINGERPRINT_INGEST_ROOTS_HEAD - 1],
            state.ingest_roots[FINGERPRINT_INGEST_ROOTS_HEAD - 1]
        );
        assert_eq!(
            fingerprint["ingest_root_count"].as_u64(),
            Some(400),
            "the real total is never hidden by truncation"
        );
        assert_eq!(
            fingerprint["ingest_roots_truncated"],
            serde_json::json!(true)
        );
        assert_eq!(
            fingerprint["ingest_roots_omitted"].as_u64(),
            Some(400 - FINGERPRINT_INGEST_ROOTS_HEAD as u64)
        );
        let surface = fingerprint["ingest_roots_full_surface"]
            .as_str()
            .expect("truncation names where the full list is served");
        assert!(
            surface.contains("doctor"),
            "the pointer must name the tool that serves the whole array, got {surface:?}"
        );
    }

    /// The declaration is always present, so an agent reads it unconditionally
    /// instead of inferring truth from a missing key. Below the head size the
    /// array is complete and the fingerprint says so.
    #[test]
    fn binding_fingerprint_declares_untruncated_ingest_roots() {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = state_with_ingest_roots(temp.path(), 3);

        let fingerprint = state.binding_fingerprint();
        let roots = fingerprint["ingest_roots"]
            .as_array()
            .expect("ingest_roots array");
        assert_eq!(roots.len(), 3);
        assert_eq!(fingerprint["ingest_root_count"].as_u64(), Some(3));
        assert_eq!(
            fingerprint["ingest_roots_truncated"],
            serde_json::json!(false)
        );
        assert_eq!(fingerprint["ingest_roots_omitted"].as_u64(), Some(0));
        assert_eq!(
            fingerprint["ingest_roots_full_surface"],
            serde_json::Value::Null,
            "no truncation means no elsewhere to point at"
        );
    }

    /// The Budget Law's original clause still holds: `graph_runtime_summary`
    /// carries the COUNT only — the head lives in the fingerprint and the array
    /// is never serialized twice in one packet.
    #[test]
    fn graph_runtime_summary_still_carries_only_the_root_count() {
        let temp = tempfile::tempdir().expect("tempdir");
        let state = state_with_ingest_roots(temp.path(), 400);

        let summary = state.graph_runtime_summary();
        assert!(summary.get("ingest_roots").is_none());
        assert_eq!(summary["ingest_root_count"].as_u64(), Some(400));
    }
}
