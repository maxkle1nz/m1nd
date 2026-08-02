// === m1nd-mcp/src/protocol/surgical.rs ===
//
// Input/Output types for surgical_context and apply.
//
// Conventions (matching core.rs / layers.rs / perspective.rs):
//   - Input:  #[derive(Clone, Debug, Deserialize)]
//   - Output: #[derive(Clone, Debug, Serialize)]
//   - All inputs require `agent_id: String`
//   - Optional params use Option<T> or serde default helpers

use crate::protocol::layers::{HeuristicSignals, HeuristicsSurfaceRef};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// m1nd.heuristics_surface
// ---------------------------------------------------------------------------

/// Input for m1nd.heuristics_surface.
///
/// Returns an explicit explainability surface for a code target using the
/// same heuristic substrate as surgical_context/apply_batch.
#[derive(Clone, Debug, Deserialize)]
pub struct HeuristicsSurfaceInput {
    pub agent_id: String,
    #[serde(default)]
    pub node_id: Option<String>,
    #[serde(default)]
    pub file_path: Option<String>,
}

/// Output for m1nd.heuristics_surface.
#[derive(Clone, Debug, Serialize)]
pub struct HeuristicsSurfaceOutput {
    pub node_id: String,
    pub file_path: String,
    pub resolved_by: String,
    pub heuristic_summary: SurgicalHeuristicSummary,
    pub elapsed_ms: f64,
}

// ---------------------------------------------------------------------------
// m1nd.surgical_context
// ---------------------------------------------------------------------------

/// Input for m1nd.surgical_context.
///
/// Returns everything needed to surgically edit a single file:
/// file contents + graph neighbourhood + provenance.
#[derive(Clone, Debug, Deserialize)]
pub struct SurgicalContextInput {
    /// Absolute or workspace-relative path to the file being edited.
    pub file_path: String,
    /// Calling agent identifier (required by all m1nd tools).
    pub agent_id: String,
    /// Optional: narrow context to a specific symbol (function / struct / class name).
    /// When provided, only the symbol's line range + its direct neighbours are returned.
    #[serde(default)]
    pub symbol: Option<String>,
    /// BFS radius for graph neighbourhood. Default: 1.
    #[serde(default = "default_radius")]
    pub radius: u32,
    /// Include test files in the neighbourhood. Default: true.
    #[serde(default = "default_true")]
    pub include_tests: bool,
    /// Maximum neighbours returned in each of callers/callees/tests.
    /// Default: 50. Set higher explicitly to widen the neighbourhood.
    #[serde(default)]
    pub max_neighbours_per_side: Option<usize>,
    /// Maximum primary-file lines returned. Default: 400.
    #[serde(default)]
    pub max_primary_file_lines: Option<usize>,
    /// Maximum primary-file characters returned after the line cap.
    /// Default: 40,000.
    #[serde(default)]
    pub max_primary_file_chars: Option<usize>,
}

fn default_radius() -> u32 {
    1
}
fn default_true() -> bool {
    true
}

/// Output for m1nd.surgical_context.
#[derive(Clone, Debug, Serialize)]
pub struct SurgicalContextOutput {
    /// Absolute path of the file (resolved).
    pub file_path: String,
    /// Full contents of the file as a UTF-8 string.
    pub file_contents: String,
    /// Total number of lines in the file.
    pub line_count: u32,
    /// Graph node ID for this file (empty string if not yet ingested).
    pub node_id: String,
    /// Symbols defined in this file with their line ranges.
    pub symbols: Vec<SurgicalSymbol>,
    /// Focused symbol details (populated when `symbol` input is given).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focused_symbol: Option<SurgicalSymbol>,
    /// Neighbourhood: files / modules that call into this file.
    pub callers: Vec<SurgicalNeighbour>,
    /// Neighbourhood: files / modules this file calls into.
    pub callees: Vec<SurgicalNeighbour>,
    /// Neighbourhood: test files that cover this file.
    pub tests: Vec<SurgicalNeighbour>,
    /// Total callers discovered before the per-side cap.
    pub total_callers: usize,
    /// Total callees discovered before the per-side cap.
    pub total_callees: usize,
    /// Total test neighbours discovered before the per-side cap.
    pub total_tests: usize,
    /// Total unique neighbours across callers/callees/tests before caps.
    pub total_neighbours: usize,
    /// True when primary content or any neighbour list was capped.
    pub truncated: bool,
    /// True when file_contents was capped by line and/or character limits.
    pub primary_truncated: bool,
    /// Heuristic explanation for why this file may be risky to patch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heuristic_summary: Option<SurgicalHeuristicSummary>,
    /// Elapsed milliseconds.
    pub elapsed_ms: f64,
}

/// Heuristic risk summary for a surgical editing target.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SurgicalHeuristicSummary {
    /// Risk level derived from heuristic priors + blast radius.
    pub risk_level: String,
    /// 0.0-1.0 normalized heuristic risk score.
    pub risk_score: f32,
    /// Approximate number of reachable files within the blast-radius pass.
    pub blast_radius_files: usize,
    /// Human-readable blast radius severity.
    pub blast_radius_risk: String,
    /// Top affected file node IDs from blast-radius traversal.
    pub top_affected: Vec<String>,
    /// Number of recurring antibodies that reference this file/node.
    pub antibody_hits: usize,
    /// Shared trust/tremor heuristic signals.
    pub heuristic_signals: HeuristicSignals,
}

/// A proactive structural insight attached to a write result.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProactiveInsight {
    /// info | warning | critical
    pub severity: String,
    /// Stable insight family name.
    pub kind: String,
    /// Short human-readable explanation.
    pub message: String,
    /// 0.0-1.0 confidence score for ranking and display.
    pub confidence: f32,
    /// Small evidence packet the caller can surface directly.
    pub evidence: Vec<String>,
    /// Suggested next tool when a follow-up is useful.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_tool: Option<String>,
    /// Suggested target for the next tool when one is available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_target: Option<String>,
}

/// A symbol (function, struct, class, etc.) within the file.
#[derive(Clone, Debug, Serialize)]
pub struct SurgicalSymbol {
    pub name: String,
    #[serde(rename = "type")]
    pub symbol_type: String,
    pub line_start: u32,
    pub line_end: u32,
    /// Excerpt of the symbol's source (first 20 lines max).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
}

/// A neighbouring node in the graph.
#[derive(Clone, Debug, Serialize)]
pub struct SurgicalNeighbour {
    pub node_id: String,
    pub label: String,
    pub file_path: String,
    pub relation: String,
    pub edge_weight: f32,
}

// ---------------------------------------------------------------------------
// apply
// ---------------------------------------------------------------------------

/// Input for apply.
///
/// Writes new file contents to disk and triggers an incremental re-ingest
/// so the graph stays coherent with the updated source.
#[derive(Clone, Debug, Deserialize)]
pub struct ApplyInput {
    /// Absolute or workspace-relative path of the file to overwrite.
    pub file_path: String,
    /// Calling agent identifier.
    pub agent_id: String,
    /// New file contents (full replacement, UTF-8).
    pub new_content: String,
    /// Human-readable description of the edit (used in the apply log).
    #[serde(default)]
    pub description: Option<String>,
    /// Re-ingest after writing. Default: true.
    #[serde(default = "default_true")]
    pub reingest: bool,
}

/// Output for apply.
#[derive(Clone, Debug, Serialize)]
pub struct ApplyOutput {
    /// Absolute path that was written.
    pub file_path: String,
    /// Number of bytes written.
    pub bytes_written: usize,
    /// Lines added (unified diff summary).
    pub lines_added: i32,
    /// Lines removed (unified diff summary).
    pub lines_removed: i32,
    /// Whether an incremental re-ingest was triggered.
    pub reingested: bool,
    /// Node IDs that were updated or added during re-ingest.
    pub updated_node_ids: Vec<String>,
    /// Proactive structural follow-up suggestions attached to this write.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub proactive_insights: Vec<ProactiveInsight>,
    /// Elapsed milliseconds.
    pub elapsed_ms: f64,
}

// ---------------------------------------------------------------------------
// m1nd.edit_preview / m1nd.edit_commit
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
pub struct EditPreviewInput {
    pub file_path: String,
    pub agent_id: String,
    pub new_content: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceFileSnapshot {
    pub file_path: String,
    pub file_exists: bool,
    pub content_hash: String,
    pub bytes: usize,
    pub line_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CandidateDiffReport {
    pub unified_diff: String,
    pub lines_added: i32,
    pub lines_removed: i32,
    pub bytes_written: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct EditPreviewOutput {
    pub preview_id: String,
    pub file_path: String,
    pub snapshot: SourceFileSnapshot,
    pub diff: CandidateDiffReport,
    pub validation: PreviewValidationReport,
    pub elapsed_ms: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct PreviewValidationReport {
    pub source_changed: bool,
    pub candidate_is_empty: bool,
    pub candidate_equals_source: bool,
    pub ready_to_commit: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EditCommitInput {
    pub preview_id: String,
    pub agent_id: String,
    /// LLM must explicitly set true to confirm the commit.
    #[serde(default)]
    pub confirm: bool,
    #[serde(default = "default_true")]
    pub reingest: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct EditCommitOutput {
    pub preview_id: String,
    pub file_path: String,
    pub bytes_written: usize,
    pub lines_added: i32,
    pub lines_removed: i32,
    pub reingested: bool,
    pub updated_node_ids: Vec<String>,
    /// Proactive structural follow-up suggestions attached to this commit,
    /// forwarded from the underlying apply (includes `proposed_antibody`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub proactive_insights: Vec<ProactiveInsight>,
    pub elapsed_ms: f64,
}

// ---------------------------------------------------------------------------
// m1nd.surgical_context_v2
// ---------------------------------------------------------------------------

/// Input for m1nd.surgical_context_v2.
///
/// Extended version that also fetches source code for each connected file
/// (callers, callees, tests), respects per-file line caps, and returns
/// total_lines for context budget management.
#[derive(Clone, Debug, Deserialize)]
pub struct SurgicalContextV2Input {
    /// Absolute or workspace-relative path to the target file.
    pub file_path: String,
    /// Calling agent identifier.
    pub agent_id: String,
    /// Optional: narrow to a specific symbol within the file.
    #[serde(default)]
    pub symbol: Option<String>,
    /// BFS radius for graph neighbourhood. Default: 1.
    #[serde(default = "default_radius")]
    pub radius: u32,
    /// Include test files in the neighbourhood. Default: true.
    #[serde(default = "default_true")]
    pub include_tests: bool,
    /// Maximum number of connected files to include source for. Default: 5.
    #[serde(default = "default_max_connected_files")]
    pub max_connected_files: usize,
    /// Maximum lines to return per connected file. Default: 60.
    #[serde(default = "default_max_lines_per_file")]
    pub max_lines_per_file: usize,
    /// When true, prefer a smaller proof set over a wider neighborhood.
    #[serde(default)]
    pub proof_focused: bool,
    /// Maximum lines to return for the primary file. Default: 400.
    /// Lines beyond this limit are replaced with a truncation marker.
    #[serde(default)]
    pub max_primary_file_lines: Option<usize>,
    /// Maximum characters to return for the primary file. Default: 40,000.
    #[serde(default)]
    pub max_primary_file_chars: Option<usize>,
}

fn default_max_connected_files() -> usize {
    5
}
fn default_max_lines_per_file() -> usize {
    60
}

/// Source excerpt for a connected file in v2 context.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectedFileSource {
    /// Graph node ID for this connected file.
    pub node_id: String,
    /// Human-readable label.
    pub label: String,
    /// Absolute path to the file.
    pub file_path: String,
    /// How this file relates to the target: "caller", "callee", or "test".
    pub relation_type: String,
    /// Edge weight from the graph.
    pub edge_weight: f32,
    /// Source excerpt (up to max_lines_per_file lines).
    pub source_excerpt: String,
    /// Number of lines in the excerpt.
    pub excerpt_lines: usize,
    /// True when the file had more lines than max_lines_per_file.
    pub truncated: bool,
    /// Heuristic explanation for why this connected file may be risky.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heuristic_summary: Option<SurgicalHeuristicSummary>,
}

/// Output for m1nd.surgical_context_v2.
#[derive(Clone, Debug, Serialize)]
pub struct SurgicalContextV2Output {
    /// Absolute path of the target file (resolved).
    pub file_path: String,
    /// Full contents of the target file.
    pub file_contents: String,
    /// Total lines in the target file.
    pub line_count: u32,
    /// Graph node ID for the target file.
    pub node_id: String,
    /// Symbols defined in the target file.
    pub symbols: Vec<SurgicalSymbol>,
    /// Focused symbol (when `symbol` input provided).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focused_symbol: Option<SurgicalSymbol>,
    /// Connected files with source excerpts (callers + callees + tests combined,
    /// capped at max_connected_files, ordered by edge_weight descending).
    pub connected_files: Vec<ConnectedFileSource>,
    /// Heuristic explanation for why this file may be risky to patch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heuristic_summary: Option<SurgicalHeuristicSummary>,
    /// Suggested next tool for continuing edit preparation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_suggested_tool: Option<String>,
    /// Suggested next target for that tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_suggested_target: Option<String>,
    /// Short next-step hint for the agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_step_hint: Option<String>,
    /// Coarse cognitive stage for edit preparation.
    pub proof_state: String,
    /// SHA-256-bound disk state captured by the one-shot proof mark. Present
    /// only when `proof_state == "ready_to_edit"` and the mark was recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_target_digest: Option<String>,
    /// Exact graph generation bound into the proof mark.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_graph_generation: Option<u64>,
    /// Absolute TTL deadline of the proof mark (unix epoch milliseconds).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_expires_at_ms: Option<u64>,
    /// Sum of all lines returned: line_count + sum(excerpt_lines).
    pub total_lines: usize,
    /// Elapsed milliseconds.
    pub elapsed_ms: f64,
    /// True when the primary file_contents was capped by max_primary_file_lines.
    pub primary_truncated: bool,
}

// ---------------------------------------------------------------------------
// apply_batch
// ---------------------------------------------------------------------------

/// A single file edit within an apply_batch request.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchEditItem {
    /// Absolute or workspace-relative path of the file to write.
    pub file_path: String,
    /// New full contents for the file (UTF-8).
    pub new_content: String,
    /// Optional description for the apply log.
    #[serde(default)]
    pub description: Option<String>,
}

/// Per-file result within an apply_batch response.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchEditResult {
    /// Absolute path that was written (or attempted).
    pub file_path: String,
    /// True when this specific file was written successfully.
    pub success: bool,
    /// Unified diff for this file.
    pub diff: String,
    /// Lines added in this file.
    pub lines_added: i32,
    /// Lines removed in this file.
    pub lines_removed: i32,
    /// Failure reason when success=false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Input for apply_batch.
///
/// Writes multiple files atomically: either ALL succeed or NONE are written
/// (rollback on partial failure when atomic=true).
/// A single incremental re-ingest covers all modified files.
#[derive(Clone, Debug, Deserialize)]
pub struct ApplyBatchInput {
    /// Calling agent identifier.
    pub agent_id: String,
    /// Files to write. Empty list is a no-op (returns success immediately).
    pub edits: Vec<BatchEditItem>,
    /// When true (default), abort and rollback all writes if any single file fails.
    #[serde(default = "default_true")]
    pub atomic: bool,
    /// Re-ingest all modified files after writing. Default: true.
    #[serde(default = "default_true")]
    pub reingest: bool,
    /// Run post-write verification (impact + antibody_scan + layer violations).
    /// Returns a VerificationReport with verdict. Default: false.
    #[serde(default)]
    pub verify: bool,
}

/// Output for apply_batch.
#[derive(Clone, Debug, Serialize)]
pub struct ApplyBatchOutput {
    /// Stable identifier for correlating final output with live progress events.
    pub batch_id: String,
    /// True when all files were written successfully.
    pub all_succeeded: bool,
    /// Number of files successfully written.
    pub files_written: usize,
    /// Total files attempted.
    pub files_total: usize,
    /// Per-file results (one entry per input edit, in input order).
    pub results: Vec<BatchEditResult>,
    /// Whether a re-ingest was triggered (single pass covering all files).
    pub reingested: bool,
    /// Total bytes written across all files.
    pub total_bytes_written: usize,
    /// Post-write verification report (populated when verify=true).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<VerificationReport>,
    /// Proactive structural follow-up suggestions attached to this batch.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub proactive_insights: Vec<ProactiveInsight>,
    /// Suggested next tool after the batch finishes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_suggested_tool: Option<String>,
    /// Suggested target for the next tool when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_suggested_target: Option<String>,
    /// Short hint for the next step after the batch finishes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_step_hint: Option<String>,
    /// Cognitive state of the batch result: blocked, triaging, proving, ready_to_edit.
    pub proof_state: String,
    /// Human-readable final status for shells/UIs.
    pub status_message: String,
    /// Final phase key reached by the batch lifecycle.
    pub active_phase: String,
    /// Completed phases out of the known lifecycle phases.
    pub completed_phase_count: usize,
    /// Total lifecycle phases in the batch contract.
    pub phase_count: usize,
    /// Remaining phases after the current active phase.
    pub remaining_phase_count: usize,
    /// Final coarse-grained progress percentage for shells/UIs.
    pub progress_pct: f32,
    /// Next expected phase in the lifecycle when not yet done.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_phase: Option<String>,
    /// Streaming-friendly progress events emitted by the batch lifecycle.
    pub progress_events: Vec<ApplyBatchProgressEvent>,
    /// Structured phase history for UI progress rendering and future streaming.
    pub phases: Vec<ApplyBatchPhase>,
    /// Elapsed milliseconds.
    pub elapsed_ms: f64,
}

/// A completed or in-progress phase within apply_batch execution.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApplyBatchPhase {
    /// Phase key: validate, write, reingest, verify, done.
    pub phase: String,
    /// Stable phase order for shells/UIs that want to render a timeline.
    pub phase_index: usize,
    /// Phase status: completed, skipped, failed.
    pub status: String,
    /// Files completed by the time this phase finished.
    pub files_completed: usize,
    /// Total files in the batch.
    pub files_total: usize,
    /// Representative file for this phase when one file best explains the work.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_file: Option<String>,
    /// Coarse progress percentage when this phase finished.
    pub progress_pct: f32,
    /// Next expected phase after this one when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_phase: Option<String>,
    /// Elapsed milliseconds at the end of this phase.
    pub elapsed_ms: f64,
    /// Short status line for user-visible progress.
    pub message: String,
}

/// A streaming-friendly progress event for apply_batch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApplyBatchProgressEvent {
    /// Stable identifier for correlating this event with the parent batch run.
    pub batch_id: String,
    /// Event type: phase_completed or batch_completed.
    pub event_type: String,
    /// Phase key this event belongs to.
    pub phase: String,
    /// Stable phase order.
    pub phase_index: usize,
    /// Progress percentage at the time of the event.
    pub progress_pct: f32,
    /// Representative file for this event when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_file: Option<String>,
    /// Next expected phase after this event when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_phase: Option<String>,
    /// Cognitive state at the time of this event when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_state: Option<String>,
    /// Suggested next tool once this event carries enough information to hand off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_suggested_tool: Option<String>,
    /// Suggested target for the next tool when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_suggested_target: Option<String>,
    /// Short hint for the next step after this event when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_step_hint: Option<String>,
    /// Proactive structural follow-up suggestions when this event is ready to hand off.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub proactive_insights: Vec<ProactiveInsight>,
    /// Event timestamp in elapsed milliseconds from batch start.
    pub elapsed_ms: f64,
    /// Short status line for user-visible progress.
    pub message: String,
}

/// Post-write verification report for apply/apply_batch.
/// Automatically runs impact analysis, antibody scan, and layer violation check
/// on all modified files after writing.
///
/// Layer A: graph-diff (pre vs post node sets)
/// Layer B: anti-pattern detection (todo!() removal, unwrap, error handling)
/// Layer C: real graph BFS impact (2-hop blast radius via CSR edges)
/// Layer D: dynamic verification boundary. Repository code is never executed in
/// the owner process; fields below report `NOT_RUN` until an isolated verifier
/// exists.
#[derive(Clone, Debug, Serialize)]
pub struct VerificationReport {
    /// Overall verdict: SAFE, RISKY, or BROKEN.
    pub verdict: String,
    /// Files with high impact (many dependents affected).
    pub high_impact_files: Vec<VerificationImpact>,
    /// Antibody patterns triggered by the changes.
    pub antibodies_triggered: Vec<String>,
    /// Layer dependency violations introduced.
    pub layer_violations: Vec<String>,
    /// Total nodes affected across all modified files.
    pub total_affected_nodes: usize,
    /// Layer C: real BFS blast radius per file (2-hop reachability count).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub blast_radius: Vec<BlastRadiusEntry>,
    /// Layer D: number of tests executed (currently None: isolated runner absent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tests_run: Option<u32>,
    /// Layer D: number of tests that passed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tests_passed: Option<u32>,
    /// Layer D: number of tests that failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tests_failed: Option<u32>,
    /// Layer D: first 500 chars of test output on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_output: Option<String>,
    /// Post-write compilation status. Currently an explicit `not_run` reason;
    /// repository-controlled build hooks are not executed by the owner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compile_check: Option<String>,
    /// Verification elapsed milliseconds.
    pub verify_elapsed_ms: f64,
}

/// Layer C: BFS blast radius entry for a single modified file.
#[derive(Clone, Debug, Serialize)]
pub struct BlastRadiusEntry {
    /// File that was modified.
    pub file_path: String,
    /// Number of OTHER file-level nodes reachable within 2 hops.
    pub reachable_files: usize,
    /// Risk level derived from reachable_files: "low" (0-3), "medium" (4-10), "high" (11+).
    pub risk: String,
    /// Top affected node IDs (external IDs of reachable file nodes, max 5).
    pub top_affected: Vec<String>,
}

// ---------------------------------------------------------------------------
// view — lightweight file reader
// ---------------------------------------------------------------------------

/// Input for view.
///
/// Simple, fast file reading — replaces View/cat/head/tail.
/// No graph traversal, just reads the file and returns content with line numbers.
/// Auto-ingests the file into the graph if not already present.
#[derive(Clone, Debug, Deserialize)]
pub struct ViewInput {
    /// Absolute or workspace-relative path to the file.
    pub file_path: String,
    /// Calling agent identifier.
    pub agent_id: String,
    /// Start line (0-based). Default: 0 (beginning of file).
    #[serde(default)]
    pub offset: Option<usize>,
    /// Maximum number of lines to return. Default: all lines.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Auto-ingest the file if not already in the graph. Default: true.
    #[serde(default = "default_true")]
    pub auto_ingest: bool,
    /// Optional cap for returned characters after line-number formatting.
    #[serde(default)]
    pub max_output_chars: Option<usize>,
}

/// Output for view.
#[derive(Clone, Debug, Serialize)]
pub struct ViewOutput {
    /// Absolute path of the file (resolved).
    pub file_path: String,
    /// File content with line numbers.
    pub content: String,
    /// Total number of lines in the file.
    pub total_lines: usize,
    /// Start offset applied.
    pub offset: usize,
    /// Number of lines returned.
    pub lines_returned: usize,
    /// Whether the file was auto-ingested into the graph.
    pub auto_ingested: bool,
    /// Whether the returned content had to be truncated.
    pub truncated: bool,
    /// Inline summary when truncation or chunking occurs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_summary: Option<String>,
    /// Elapsed milliseconds.
    pub elapsed_ms: f64,
}

/// Input for m1nd.batch_view.
#[derive(Clone, Debug, Deserialize)]
pub struct BatchViewInput {
    pub agent_id: String,
    /// File paths and/or glob-like patterns to expand.
    pub files: Vec<String>,
    /// Maximum lines per file. Default: 100.
    #[serde(default = "default_batch_view_lines")]
    pub max_lines_per_file: usize,
    /// Add file summaries to each entry. Default: true.
    #[serde(default = "default_true")]
    pub summary_mode: bool,
    /// Auto-ingest discovered files before reading. Default: true.
    #[serde(default = "default_true")]
    pub auto_ingest: bool,
    /// Optional cap for the concatenated response body.
    #[serde(default)]
    pub max_output_chars: Option<usize>,
}

fn default_batch_view_lines() -> usize {
    100
}

#[derive(Clone, Debug, Serialize)]
pub struct BatchViewFileOutput {
    pub requested: String,
    pub file_path: String,
    pub total_lines: usize,
    pub lines_returned: usize,
    pub auto_ingested: bool,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub content: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct BatchViewOutput {
    pub files_read: usize,
    pub total_lines: usize,
    pub entries: Vec<BatchViewFileOutput>,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_summary: Option<String>,
    pub elapsed_ms: f64,
}

/// Impact summary for a single modified file.
#[derive(Clone, Debug, Serialize)]
pub struct VerificationImpact {
    /// File that was modified.
    pub file_path: String,
    /// Node ID in the graph.
    pub node_id: String,
    /// Number of nodes affected by this change.
    pub affected_count: usize,
    /// Risk level: "low", "medium", "high".
    pub risk: String,
    /// Top affected node IDs (max 5).
    pub top_affected: Vec<String>,
    /// Heuristic explanation for why this modified file is risky post-patch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heuristic_summary: Option<SurgicalHeuristicSummary>,
    /// Explorable reference for `m1nd.heuristics_surface` parity with validate-plan/report.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heuristics_surface_ref: Option<HeuristicsSurfaceRef>,
}

// ---------------------------------------------------------------------------
// m1nd.transplant — graph-addressed cross-file move of a top-level `fn`
// (design + proof addresses: `docs/TRANSPLANT-PRD.md`). The verb resolves a
// symbol via the graph, computes the dependency trichotomy from `calls` edges,
// and writes source/dest/referencers atomically through the apply_batch machinery.
// ---------------------------------------------------------------------------

/// Input for m1nd.transplant.
///
/// Addresses the moved item by `symbol` + `source_file` — the canonical v1 form
/// (owner decision D1: a `node_id` form waits for stable node identity across
/// re-ingest). Paths are absolute or workspace-relative and resolved against the
/// ingest roots.
#[derive(Clone, Debug, Deserialize)]
pub struct TransplantInput {
    /// Calling agent identifier (required by all m1nd tools).
    pub agent_id: String,
    /// Bare name of the top-level `fn` to move (matches the node label).
    pub symbol: String,
    /// File the symbol currently lives in.
    pub source_file: String,
    /// File the symbol is moved into (must already exist — PRD §7.3).
    pub dest_file: String,
    /// A3 — the explicit Money-Zone gesture. When a touched path (source, dest, or a
    /// derived referencer) matches a `ci/protected-zones.json` glob, the transplant
    /// refuses UNLESS this carries the caller's reason for crossing the guarded zone.
    /// Absent (the default) means "I did not intend to touch a protected zone" — a
    /// zone match then refuses, teaching the gesture. Recorded in the receipt when it
    /// unlocks a crossing.
    #[serde(default)]
    pub allow_protected: Option<String>,
}

/// A1 — node-addressed state a transplant could NOT carry to the moved symbol's
/// new home, because the re-ingest recreates the node under a new external_id (a
/// fn node id is `file::<path>::fn::<name>`, path-dependent; the OpenRewrite stable
/// identity is unimplemented). Each entry names the symbol, its old→new node id and
/// the orphaned payload, so the verb NEVER silently orphans node-bound state.
/// Following it fully needs owner-side wiring (a stable node id across re-ingest, or
/// a paint-tag registry) — reported here, not faked.
#[derive(Clone, Debug, Serialize)]
pub struct StateLeftBehind {
    /// The moved symbol whose node was recreated.
    pub symbol: String,
    /// The symbol's node id BEFORE the move (orphaned by the re-ingest).
    pub old_node_id: String,
    /// The symbol's node id AFTER the move (empty when the new node was not found).
    pub new_node_id: String,
    /// The class of orphaned state (currently `"xray_tags"`).
    pub kind: String,
    /// The orphaned payload: tags that were on the old node and the re-ingest did
    /// NOT reproduce under any namespace on the new node (the painted tags a move
    /// loses — structural tags the re-ingest regenerated are excluded).
    pub detail: Vec<String>,
}

/// A3 — the Money-Zone gesture a transplant recorded when it crossed a protected
/// zone with the caller's explicit `allow_protected` reason. Present in the receipt
/// only when a guarded zone was actually crossed, so the crossing is auditable.
#[derive(Clone, Debug, Serialize)]
pub struct ProtectedZoneGesture {
    /// The `ci/protected-zones.json` glob the touched file matched.
    pub zone: String,
    /// The reason the zone is guarded (from the config).
    pub zone_reason: String,
    /// The touched path (source, dest, or a derived referencer) that matched.
    pub matched_file: String,
    /// The caller's `allow_protected` reason that unlocked the crossing.
    pub gesture: String,
}

/// A dependency that STAYS in the source file but is shared by the moved item,
/// so it gains a visibility bump and is back-imported into the destination.
#[derive(Clone, Debug, Serialize)]
pub struct SharedDepReport {
    /// The dependency's symbol name.
    pub name: String,
    /// Visibility before the transplant (e.g. "private", "pub(crate)", "pub").
    pub visibility_before: String,
    /// Visibility after the transplant (bumped only when it was more private).
    pub visibility_after: String,
}

/// Output for m1nd.transplant. Honest fields (`refs_unresolved`,
/// `source_back_imported`) surface anything the verb could not confidently do,
/// so a caller never mistakes a silent skip for a clean move.
#[derive(Clone, Debug, Serialize)]
pub struct TransplantOutput {
    /// The symbol that was moved.
    pub moved_symbol: String,
    /// Source module name (file stem) the symbol left.
    pub source_module: String,
    /// Destination module name (file stem) the symbol entered.
    pub dest_module: String,
    /// Absolute paths written by this transplant, in write order.
    pub files_changed: Vec<String>,
    /// Private dependencies that travelled with the moved item (trichotomy).
    pub deps_travelled: Vec<String>,
    /// Shared dependencies that stayed, with their visibility bumps (trichotomy).
    pub deps_shared: Vec<SharedDepReport>,
    /// Files whose references to the moved symbol were rewritten to the new home.
    pub referencing_files: Vec<String>,
    /// Total number of reference sites rewritten across all referencing files.
    pub refs_rewritten: usize,
    /// Reference sites the verb refused to rewrite (e.g. grouped `use` imports) —
    /// surfaced honestly rather than silently skipped.
    pub refs_unresolved: Vec<String>,
    /// True when the source file kept a caller of the moved symbol and gained a
    /// back-import `use crate::<dest_module>::<symbol>;` (the self-use case).
    pub source_back_imported: bool,
    /// `use` statements carried from the source into the destination because the
    /// moved text references what they bind (rope's over-provision→prune law).
    pub imports_carried: Vec<String>,
    /// True when the moved fn was private and had to become `pub(crate)` in its
    /// new home so the source's back-import keeps compiling (E0603 otherwise).
    pub moved_visibility_bumped: bool,
    /// How the dependency trichotomy was derived: "graph_edges" or "textual".
    pub dependency_source: String,
    /// How referencing files were discovered: "graph_edges", "textual", or "both".
    pub referencer_source: String,
    /// §7.7 post-compute formatting status: "applied" when every touched file was
    /// piped through `rustfmt --edition 2021` before the atomic write; otherwise
    /// an honest note (rustfmt unavailable / rejected a file) — never a silent
    /// skip, because a fmt-gated repo would reprove CI without warning.
    pub rustfmt: String,
    /// D5b — SystemBlock ids whose ratified `boundary_version` this transplant aged
    /// because their membership claims a touched file (PRD §10 D5 option b). The
    /// bump stales those blocks' receipts by scope through the EXISTING rollup law,
    /// closing the lie-window where a symbol crossed a ratified boundary but the
    /// unchanged path-set membership left the receipts green. Empty when no skeleton
    /// is present or no block claims a touched file — the verb never silently ages a
    /// boundary.
    pub blocks_touched: Vec<String>,
    /// A1 — node-addressed state (currently xray/paint tags) the re-ingest orphaned
    /// when it recreated a moved symbol's node under a new id. Empty when nothing was
    /// left behind. The verb never silently orphans node-bound state; carrying it
    /// fully needs owner-side wiring (see [`StateLeftBehind`]).
    pub state_left_behind: Vec<StateLeftBehind>,
    /// A3 — the Money-Zone gesture recorded when this transplant crossed a protected
    /// zone with the caller's explicit `allow_protected` reason. `None` when no
    /// guarded zone was touched (the common case).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protected_zone: Option<ProtectedZoneGesture>,
    /// Elapsed milliseconds.
    pub elapsed_ms: f64,
}

// ---------------------------------------------------------------------------
// m1nd.transplant_preview / m1nd.transplant_commit (A2 — two-phase)
// ---------------------------------------------------------------------------

/// One planned file of a staged two-phase transplant. `base_hash` is the hash of
/// the ON-DISK content the plan was computed from — the commit re-validates every
/// one and refuses on any drift (the TOCTOU anchor), so a stale plan can never
/// clobber a file that moved on since the preview.
#[derive(Clone, Debug, Serialize)]
pub struct TransplantPlannedFileReport {
    pub file_path: String,
    pub base_hash: String,
    pub lines_added: i32,
    pub lines_removed: i32,
}

/// Output for m1nd.transplant_preview.
///
/// The preview computes EVERYTHING the one-shot verb would (all new contents,
/// referencer discovery, fmt pass, candidate receipt) and writes NOTHING; the
/// staged plan is redeemable via `transplant_commit{preview_id, confirm:true}`
/// within `ttl_ms`.
#[derive(Clone, Debug, Serialize)]
pub struct TransplantPreviewOutput {
    pub preview_id: String,
    /// Staged-plan time-to-live in milliseconds (5 min, mirroring edit_preview).
    pub ttl_ms: u64,
    /// Per-file plan: source + dest + every DERIVED referencer, in write order.
    pub files: Vec<TransplantPlannedFileReport>,
    /// The receipt the commit will finalize (timing re-stamped at commit).
    pub candidate: TransplantOutput,
    pub elapsed_ms: f64,
}

/// Input for m1nd.transplant_commit.
#[derive(Clone, Debug, Deserialize)]
pub struct TransplantCommitInput {
    pub preview_id: String,
    pub agent_id: String,
    /// The caller must explicitly set true to land the staged plan.
    #[serde(default)]
    pub confirm: bool,
}

/// Output for m1nd.transplant_commit.
#[derive(Clone, Debug, Serialize)]
pub struct TransplantCommitOutput {
    pub preview_id: String,
    /// The finalized transplant receipt (same shape as the one-shot verb's).
    pub receipt: TransplantOutput,
    pub elapsed_ms: f64,
}
