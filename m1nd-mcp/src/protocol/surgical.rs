// === m1nd-mcp/src/protocol/surgical.rs ===
//
// Input/Output types for m1nd.surgical_context and m1nd.apply.
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
// m1nd.apply
// ---------------------------------------------------------------------------

/// Input for m1nd.apply.
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

/// Output for m1nd.apply.
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
    /// Sum of all lines returned: line_count + sum(excerpt_lines).
    pub total_lines: usize,
    /// Elapsed milliseconds.
    pub elapsed_ms: f64,
}

// ---------------------------------------------------------------------------
// m1nd.apply_batch
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

/// Input for m1nd.apply_batch.
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

/// Output for m1nd.apply_batch.
#[derive(Clone, Debug, Serialize)]
pub struct ApplyBatchOutput {
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
    /// Elapsed milliseconds.
    pub elapsed_ms: f64,
}

/// Post-write verification report for apply/apply_batch.
/// Automatically runs impact analysis, antibody scan, and layer violation check
/// on all modified files after writing.
///
/// Layer A: graph-diff (pre vs post node sets)
/// Layer B: anti-pattern detection (todo!() removal, unwrap, error handling)
/// Layer C: real graph BFS impact (2-hop blast radius via CSR edges)
/// Layer D: affected test execution (cargo test / go test / pytest)
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
    /// Layer D: number of tests executed (None if test detection skipped).
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
    /// Post-write compilation check result.
    /// None = skipped (no recognized project type or verify=false).
    /// Some("ok") = compilation passed.
    /// Some("error: ...") = compilation failed (first 200 chars of stderr).
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
// m1nd.view — lightweight file reader
// ---------------------------------------------------------------------------

/// Input for m1nd.view.
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
}

/// Output for m1nd.view.
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
    /// Elapsed milliseconds.
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
