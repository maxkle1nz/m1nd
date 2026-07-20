//! Human View v2 F0a — the SystemBlock store MCP verbs (Slices 2 + 3).
//!
//! The verbs that serve the per-project-brain sidecar store defined in
//! [`crate::system_blocks`]:
//! - `system_blocks_snapshot` (READ) — the whole store, or an honest "no skeleton"
//! - `system_blocks_seed_import` (WRITE) — seed -> fresh store (`store_version = 1`)
//! - `system_blocks_ratify` (WRITE) — flip candidate blocks to ratified (OCC)
//! - `receipt_import` (WRITE) — attach anti-poison-checked evidence (OCC)
//! - `system_blocks_reconcile` (WRITE, Slice 3) — resolve membership vs the real
//!   file list, bump moved boundaries, surface the real unmapped (OCC)
//! - `receipt_recompute` (READ, Slice 3) — per-receipt fresh/stale, history intact
//! - `system_blocks_archive` (WRITE, Slice 3) — retire/restore a block (OCC)
//! - `system_blocks_delete` (WRITE, Slice 3) — permanently remove a block (OCC)
//!
//! Every WRITE is OCC-keyed on the `store_version` the caller read (PRD §3.1); a
//! stale write is rejected and nothing is applied. The writers live in the
//! read-only-attach deny-list (`server::READ_ONLY_DENIED_TOOLS`); the snapshot and
//! recompute reads are allowed. The store lives in the brain's runtime dir
//! (`runtime_root`), alongside the brain's other runtime artifacts (F0-TECH §1) —
//! no new root is invented.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{json, Value};

use m1nd_core::error::{M1ndError, M1ndResult};

use crate::session::SessionState;
use crate::skeleton_scan::{
    self, CandidateNamingMode, ScanProgressEvent, SkeletonCoherence, SkeletonScanOptions,
};
use crate::system_blocks::{
    self, archive_in_dir, candidate_edit_in_dir, candidate_lease_in_dir, delete_in_dir,
    import_receipt_in_dir, import_seed_into_dir, recompute_in_dir, reconcile_in_dir,
    skeleton_candidate_in_dir, ArchiveMode, LeaseAction, Receipt, SeedError, SystemBlockStore,
    DEFAULT_LEASE_TTL_SECS,
};
use crate::util::now_ms;

/// Map a domain [`SeedError`] onto the MCP error surface. Every honest refusal
/// (conflict, stale scope, already-present, bad evidence, unknown block) becomes
/// an `InvalidParams` whose detail carries the keyword the caller acts on
/// (`conflict`, `stale_scope`, `already_present`, …).
fn seed_err(tool: &str, err: SeedError) -> M1ndError {
    M1ndError::InvalidParams {
        tool: tool.to_string(),
        detail: err.to_string(),
    }
}

/// The brain runtime dir the sidecar store lives in (F0-TECH §1). This is the SAME
/// dir the brain persists its other artifacts to (graph snapshot, plasticity,
/// antibodies) — never a freshly invented root.
pub(crate) fn store_dir(state: &SessionState) -> PathBuf {
    state.runtime_root.clone()
}

// ---------------------------------------------------------------------------
// system_blocks_snapshot (READ)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SnapshotInput {
    #[allow(dead_code)]
    pub agent_id: Option<String>,
}

/// `system_blocks_snapshot` (READ). Returns the whole store, or an honest
/// "no skeleton yet" when the brain has no store. Never writes — safe under a
/// read-only attach.
pub fn handle_system_blocks_snapshot(
    state: &mut SessionState,
    _input: SnapshotInput,
) -> M1ndResult<Value> {
    let dir = store_dir(state);
    match SystemBlockStore::load(&dir).map_err(|e| seed_err("system_blocks_snapshot", e))? {
        Some(store) => {
            let block_ids = store
                .blocks
                .iter()
                .map(|block| block.block_id.clone())
                .collect::<Vec<_>>();
            let skeleton_coherence = skeleton_scan::skeleton_coherence(
                state.project_root_display().as_deref(),
                Some((&store.skeleton.skeleton_id, &block_ids)),
            )
            .map(|coherence| match coherence {
                SkeletonCoherence::Ok => json!({ "status": "ok" }),
                SkeletonCoherence::Mismatch {
                    expected_slug,
                    found_slug,
                } => json!({
                    "status": "mismatch",
                    "expected_slug": expected_slug,
                    "found_slug": found_slug,
                }),
            })
            .unwrap_or(Value::Null);

            Ok(json!({
                "present": true,
                "store_version": store.store_version,
                "block_count": store.blocks.len(),
                "skeleton_coherence": skeleton_coherence,
                "store": store,
            }))
        }
        None => Ok(json!({
            "present": false,
            "skeleton_coherence": Value::Null,
            "honest": "no skeleton yet — import a seed or run a scan",
        })),
    }
}

// ---------------------------------------------------------------------------
// system_blocks_seed_import (WRITE)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SeedImportInput {
    #[allow(dead_code)]
    pub agent_id: Option<String>,
    /// Inline seed JSON. Mutually exclusive with `seed_path`.
    pub seed_json: Option<String>,
    /// Repo-relative path to a seed file (same anti-absolute law as the seed's own
    /// paths). Mutually exclusive with `seed_json`.
    pub seed_path: Option<String>,
    /// Overwrite an existing store instead of refusing (`already_present`).
    #[serde(default)]
    pub force: bool,
}

/// `system_blocks_seed_import` (WRITE). Converts a validated seed into a fresh
/// store (`store_version = 1`). An existing store is refused unless `force`.
pub fn handle_system_blocks_seed_import(
    state: &mut SessionState,
    input: SeedImportInput,
) -> M1ndResult<Value> {
    const TOOL: &str = "system_blocks_seed_import";
    let dir = store_dir(state);
    let raw = match (input.seed_json, input.seed_path) {
        (Some(_), Some(_)) => {
            return Err(M1ndError::InvalidParams {
                tool: TOOL.to_string(),
                detail: "pass exactly one of seed_json or seed_path, not both".to_string(),
            })
        }
        (None, None) => {
            return Err(M1ndError::InvalidParams {
                tool: TOOL.to_string(),
                detail: "pass seed_json (inline) or seed_path (repo-relative)".to_string(),
            })
        }
        (Some(j), None) => j,
        (None, Some(p)) => read_repo_relative_seed(state, TOOL, &p)?,
    };
    let outcome = import_seed_into_dir(&dir, &raw, input.force).map_err(|e| seed_err(TOOL, e))?;
    let mut out = json!({
        "present": true,
        "store_version": outcome.store.store_version,
        "block_count": outcome.store.blocks.len(),
        "overwritten": outcome.overwritten,
    });
    if outcome.overwritten {
        out["warning"] =
            json!("an existing store was overwritten (force=true) — its live state is gone");
    }
    Ok(out)
}

/// Read a repo-relative seed file. Enforces the same anti-absolute-path law the
/// seed itself obeys, then resolves it against the brain's workspace root.
fn read_repo_relative_seed(state: &SessionState, tool: &str, rel: &str) -> M1ndResult<String> {
    system_blocks::validate_repo_relative_path(rel).map_err(|e| seed_err(tool, e))?;
    let root = state
        .workspace_root
        .as_ref()
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: tool.to_string(),
            detail:
                "no workspace root is bound to this brain — pass seed_json instead of seed_path"
                    .to_string(),
        })?;
    let full = Path::new(root).join(rel);
    std::fs::read_to_string(&full).map_err(|e| M1ndError::InvalidParams {
        tool: tool.to_string(),
        detail: format!("cannot read seed_path '{rel}': {e}"),
    })
}

// ---------------------------------------------------------------------------
// skeleton_candidate (WRITE) — F0c-a scan -> candidate store/revision
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SkeletonCandidateInput {
    #[allow(dead_code)]
    pub agent_id: Option<String>,
    /// The store_version the caller read. `None` is valid only when no store exists.
    #[serde(default)]
    pub expected_store_version: Option<u64>,
    /// UI review queue hint. The backend emits every block; this is echoed only.
    #[serde(default)]
    pub review_limit: Option<usize>,
    /// `auto` (default) calls the pinned live naming-runner through the announced
    /// runnerd when one is live (F11-b §2a) — per-block packets, per-block
    /// timeout, hostile-output sanitization (o5), per-block heuristic fallback;
    /// with NO live runnerd it behaves exactly like the offline scan (all
    /// heuristic). `heuristic` skips the runner explicitly.
    #[serde(default)]
    pub naming: Option<String>,
}

/// `skeleton_candidate` (WRITE). Scans the bound repo graph + file list into a
/// candidate seed, then applies the F0c-a transaction state machine: absent store
/// creates v1; candidate store is replaced wholesale with heranca-zero; ratified
/// store receives only `candidate_revision`.
pub fn handle_skeleton_candidate(
    state: &mut SessionState,
    input: SkeletonCandidateInput,
) -> M1ndResult<Value> {
    const TOOL: &str = "skeleton_candidate";
    let naming = CandidateNamingMode::parse(input.naming.as_deref()).map_err(|detail| {
        M1ndError::InvalidParams {
            tool: TOOL.to_string(),
            detail,
        }
    })?;
    let root = state
        .code_root_path()
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: TOOL.to_string(),
            detail: "no CODE root is bound to this brain — skeleton_candidate needs the repo \
                 (a hosted brain's raw workspace is its store dir, never scanned)"
                .to_string(),
        })?;
    let file_list =
        system_blocks::repo_file_list(Path::new(&root)).map_err(|e| seed_err(TOOL, e))?;
    // Slice 2 (docs/uml/scan-loading.md): narrate the pipeline's real phase
    // boundaries on the EXISTING `/api/events` SSE channel. The file list is in
    // hand — `file_list` with its count. Every emit is a fact + fail-open; the
    // verb's response is untouched, so a client that never listens sees today's
    // behavior exactly.
    emit_scan_progress(state, &ScanProgressEvent::file_list(file_list.len()));
    let source_commit = git_head_commit(Path::new(&root)).unwrap_or_default();
    let repo_id = Path::new(&root)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repo")
        .to_string();
    // `clustering` — announce the Louvain + directory-module pass with the real
    // graph size BEFORE it runs (the heavy community detection lives inside
    // `scan_input_from_graph`). Two cheap read locks; the emit holds neither.
    let (node_count, edge_count) = {
        let graph = state.graph.read();
        (graph.num_nodes() as usize, graph.num_edges())
    };
    emit_scan_progress(
        state,
        &ScanProgressEvent::clustering(node_count, edge_count),
    );
    let scan_input = {
        let graph = state.graph.read();
        skeleton_scan::scan_input_from_graph(
            &graph,
            repo_id,
            ".".to_string(),
            source_commit,
            file_list,
        )
    };
    let mut scan = skeleton_scan::scan_skeleton(
        scan_input,
        SkeletonScanOptions {
            naming,
            ..SkeletonScanOptions::default()
        },
    );
    // F11-b §2a: with `naming:"auto"`, a LIVE announced runnerd, and the shared
    // secret on disk, call the pinned naming-runner (one packet per block) and
    // land `named_by:"runner"` / `needs_owner_naming:false` BEFORE the store
    // transaction — runner absent/timeout/parse-fail falls back to the honest
    // heuristic PER BLOCK (partial is normal). With NO announced runnerd this
    // whole branch is skipped and the scan output stays byte-identical to the
    // offline behavior (the F0c-a fallback note included).
    if naming == CandidateNamingMode::Auto {
        if let Some(handle) = state.runnerd_naming.clone() {
            let secret = crate::runnerd_owner::read_secret(&handle.owner_runtime_root);
            let live = !handle.registry.live_ports().is_empty();
            if let (Some(secret), true, false) = (secret, live, scan.naming_packets.is_empty()) {
                // `naming` — the slow phase: this many blocks, plus the budget's
                // wave ESTIMATE (`blocks.div_ceil(4)`, the divisor `scan_naming_timeout`
                // uses). ONE opaque daemon call follows; the client's elapsed clock
                // narrates the wait — no fabricated per-wave sub-progress.
                let packet_count = scan.naming_packets.len();
                emit_scan_progress(
                    state,
                    &ScanProgressEvent::naming(packet_count, packet_count.div_ceil(4).max(1)),
                );
                let outcome = crate::naming_runner::run_scan_naming(
                    &handle,
                    &secret,
                    &scan.naming_packets,
                    &mut scan.seed,
                    &mut scan.report.blocks,
                );
                let total = scan.naming_packets.len();
                let naming_report = &mut scan.report.naming;
                match &outcome.transport_error {
                    Some(err) => {
                        // A daemon was announced but no call completed — every
                        // block stays heuristic; say so, never silently.
                        naming_report.applied = "heuristic".to_string();
                        naming_report.runner_available = false;
                        naming_report.note = format!(
                            "naming-runner call failed: {err}; heuristic provisional names were used"
                        );
                    }
                    None => {
                        naming_report.runner_available = true;
                        let named = outcome.named.len();
                        if named == total {
                            naming_report.applied = "runner".to_string();
                            naming_report.note =
                                format!("live naming-runner named {named}/{total} blocks");
                        } else if named == 0 {
                            naming_report.applied = "heuristic".to_string();
                            naming_report.note = format!(
                                "live naming-runner named 0/{total} blocks; heuristic provisional names were used"
                            );
                        } else {
                            naming_report.applied = "runner_partial".to_string();
                            naming_report.note = format!(
                                "live naming-runner named {named}/{total} blocks; {} fell back to heuristic",
                                total - named
                            );
                        }
                    }
                }
            }
        }
    }
    let dir = store_dir(state);
    // `persisting` — the candidate seed is being written to the store.
    emit_scan_progress(
        state,
        &ScanProgressEvent::persisting(scan.seed.blocks.len()),
    );
    let (store, summary) =
        match skeleton_candidate_in_dir(&dir, scan.seed.clone(), input.expected_store_version) {
            Ok(landed) => landed,
            Err(e) => {
                // Terminal `failed` with the honest owner string, THEN surface the
                // error exactly as before (the response grammar is unchanged).
                emit_scan_progress(state, &ScanProgressEvent::failed(e.to_string()));
                return Err(seed_err(TOOL, e));
            }
        };
    // Terminal `done` — the store landed.
    emit_scan_progress(state, &ScanProgressEvent::done(scan.seed.blocks.len()));

    Ok(json!({
        "present": true,
        "store_version": store.store_version,
        "transaction_state": summary.transaction_state.as_str(),
        "candidate_revision_written": summary.candidate_revision_written,
        "block_count": scan.seed.blocks.len(),
        "review_limit": input.review_limit.unwrap_or(16),
        "candidate_seed": scan.seed,
        "report": scan.report,
    }))
}

fn git_head_commit(root: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Emit one scan-phase event through the session's optional sink (slice 2,
/// docs/uml/scan-loading.md). Fail-open by construction: no sink wired (every path
/// but the HTTP/stdio `skeleton_candidate` dispatch) is a silent no-op, and the
/// wired sink itself ignores a failed broadcast send — narration can never break
/// the scan.
fn emit_scan_progress(state: &SessionState, event: &ScanProgressEvent) {
    if let Some(sink) = state.scan_progress_sink.as_ref() {
        sink(event);
    }
}

// ---------------------------------------------------------------------------
// system_blocks_ratify (WRITE)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RatifyInput {
    #[allow(dead_code)]
    pub agent_id: Option<String>,
    /// The `store_version` the caller read (OCC key, PRD §3.1).
    pub expected_store_version: u64,
    /// Blocks to ratify; `None`/absent ratifies every block.
    #[serde(default)]
    pub block_ids: Option<Vec<String>>,
    /// Who ratified (stamped into the skeleton's ratification record).
    pub ratifier: String,
}

/// `system_blocks_ratify` (WRITE). Flips the targeted blocks `candidate ->
/// ratified` and their membership `proposed -> ratified`, stamps the skeleton's
/// ratification (method `verb`, now), and bumps `store_version`. OCC-checked.
pub fn handle_system_blocks_ratify(
    _state: &mut SessionState,
    _input: RatifyInput,
) -> M1ndResult<Value> {
    const TOOL: &str = "system_blocks_ratify";
    // A client-controlled string is not evidence of a human gesture. Direct
    // ratification stays closed until a typed G2/G3 lease consumer invokes the
    // domain transition through an authority-bound path.
    Err(M1ndError::InvalidParams {
        tool: TOOL.to_string(),
        detail: "sovereign_authority_required: direct system_blocks_ratify is disabled; a client-supplied origin token (including 'human-ui') grants no authority, and no exact typed G2/G3 ratification lease path is installed"
            .to_string(),
    })
}

// ---------------------------------------------------------------------------
// receipt_import (WRITE)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ReceiptImportInput {
    #[allow(dead_code)]
    pub agent_id: Option<String>,
    /// The `store_version` the caller read (OCC key, PRD §3.1).
    pub expected_store_version: u64,
    /// The block the receipt is evidence for.
    pub block_id: String,
    /// The receipt itself (full [`Receipt`] shape; unknown fields are rejected).
    pub receipt: Receipt,
    /// The ORIGIN of the import gesture (sovereign-stamp arc). Validated server-side
    /// against the closed [`RECEIPT_IMPORT_HUMAN_ORIGINS`] allow-list. A HUMAN gesture
    /// composes an allow-listed value — `"human-ui"` from the owner's screen, or
    /// `"human-touchid"` from the h4nd tray's native prompt behind Touch ID; a runner/agent
    /// MCP client never does. Absent, empty, or any off-list value refuses the call —
    /// landing a receipt is the human's signature, not an agent's write (the same law
    /// `ratify` carries).
    #[serde(default)]
    pub imported_via: Option<String>,
}

/// The CLOSED allow-list of HUMAN origin tokens that pass the `receipt_import` gate.
/// Two gestures compose a legitimate import today: `"human-ui"` (the owner's web screen)
/// and `"human-touchid"` (the h4nd tray's native fact-prompt, landed behind a real Touch
/// ID / OS-password gesture — sovereign-stamp arc step 2, which now EXISTS). The remaining
/// native gestures — `"human-tray"`, `"human-tray-batch"` — join this list in LATER steps,
/// and only WHEN their components exist: a new origin is a code change + a test here, never
/// a silently-trusted client string. Absent, empty, or any off-list value is refused.
const RECEIPT_IMPORT_HUMAN_ORIGINS: &[&str] = &["human-ui", "human-touchid"];

/// `receipt_import` (WRITE). Attaches a receipt to a block after the human-origin
/// gate and the anti-poison gates (OCC, block exists, scope binds to the block's
/// CURRENT versions, evidence contract, captured execution-window coherence) all
/// pass; bumps `store_version`.
pub fn handle_receipt_import(
    state: &mut SessionState,
    input: ReceiptImportInput,
) -> M1ndResult<Value> {
    const TOOL: &str = "receipt_import";
    // Origin gate: landing a receipt is a HUMAN gesture, exactly as ratify is (the
    // sovereign-stamp verdict, step 0). The open hole this closes: only ratify carried
    // the mechanical mirror; `receipt_import` — the OTHER human write that bumps
    // `store_version` — had NONE, so an agent could land evidence by simply calling it.
    // Now both doors require a human origin token, validated server-side against a
    // CLOSED allow-list (const in code — a new origin is a code change + a test, never a
    // client string). The owner's screen stamps `imported_via:"human-ui"`; a
    // runner/agent MCP client never composes it. It holds on BOTH seams (the MCP wire
    // and REST route through this one `dispatch_tool` — the #333 parity lesson). As with
    // ratify, the token is forgeable on an unauthenticated loopback, so this closes the
    // CHEAP vector (an agent that calls receipt_import by reflex/deceit), NOT a malicious
    // same-UID process — the real cryptographic elevation (Touch ID) is step 2 of the arc.
    let origin = input.imported_via.as_deref().unwrap_or("");
    if !RECEIPT_IMPORT_HUMAN_ORIGINS.contains(&origin) {
        return Ok(json!({
            "ok": false,
            "schema": "m1nd-system-block-write-v0",
            "refused": "human_gesture_required",
            "tool": TOOL,
            "field": "imported_via",
            "allowed_origins": RECEIPT_IMPORT_HUMAN_ORIGINS,
            "lesson": "landing a receipt is the human gesture — the owner's screen sends it; agents never do",
        }));
    }
    let dir = store_dir(state);
    let store = import_receipt_in_dir(
        &dir,
        input.expected_store_version,
        &input.block_id,
        input.receipt,
    )
    .map_err(|e| seed_err(TOOL, e))?;
    let receipt_count = store
        .blocks
        .iter()
        .find(|b| b.block_id == input.block_id)
        .map(|b| b.receipts.len())
        .unwrap_or(0);
    Ok(json!({
        "store_version": store.store_version,
        "block_id": input.block_id,
        "receipt_count": receipt_count,
    }))
}

// ---------------------------------------------------------------------------
// system_blocks_reconcile (WRITE) — the architectural git status (Slice 3)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ReconcileInput {
    #[allow(dead_code)]
    pub agent_id: Option<String>,
    /// The `store_version` the caller read (OCC key, PRD §3.1).
    pub expected_store_version: u64,
    /// An explicit repo-relative file list to reconcile against. When absent, the
    /// list is read from the brain's bound workspace root (git, else a walk).
    #[serde(default)]
    pub file_list: Option<Vec<String>>,
}

/// `system_blocks_reconcile` (WRITE). Resolves every block's membership (exact +
/// globs) against the real file list, records baseline fingerprints, bumps the
/// `boundary_version` of blocks whose resolved set moved (which makes their
/// previously-earned receipts stale by scope), and surfaces the real unmapped. The
/// whole reconcile is one atomic OCC mutation: on any change `store_version` bumps
/// once; a no-op reconcile changes nothing.
pub fn handle_system_blocks_reconcile(
    state: &mut SessionState,
    input: ReconcileInput,
) -> M1ndResult<Value> {
    const TOOL: &str = "system_blocks_reconcile";
    let dir = store_dir(state);
    let file_list = match input.file_list {
        Some(list) => list,
        None => {
            let root = state
                .code_root_path()
                .ok_or_else(|| M1ndError::InvalidParams {
                    tool: TOOL.to_string(),
                    detail: "no CODE root is bound to this brain — pass file_list explicitly \
                         (a hosted brain's raw workspace is its store dir, never scanned)"
                        .to_string(),
                })?;
            system_blocks::repo_file_list(Path::new(&root)).map_err(|e| seed_err(TOOL, e))?
        }
    };
    let (store, report) = reconcile_in_dir(&dir, input.expected_store_version, &file_list)
        .map_err(|e| seed_err(TOOL, e))?;
    let mut out = serde_json::to_value(&report).map_err(M1ndError::Serde)?;
    out["store_version"] = json!(store.store_version);
    out["file_count"] = json!(file_list.len());
    Ok(out)
}

// ---------------------------------------------------------------------------
// receipt_recompute (READ) — receipt freshness pass (Slice 3)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ReceiptRecomputeInput {
    #[allow(dead_code)]
    pub agent_id: Option<String>,
    /// Recompute only this block; omit to recompute every block.
    #[serde(default)]
    pub block_id: Option<String>,
}

/// `receipt_recompute` (READ). Re-evaluates each receipt against its block's CURRENT
/// `(block_id, boundary_version, contract_version)` and `expires_on`, returning
/// per-receipt `fresh` / `stale{reason}`. A pure read — receipts are never deleted
/// (history is history); the report IS the truth. Safe under a read-only attach.
pub fn handle_receipt_recompute(
    state: &mut SessionState,
    input: ReceiptRecomputeInput,
) -> M1ndResult<Value> {
    const TOOL: &str = "receipt_recompute";
    let dir = store_dir(state);
    let now = now_iso8601();
    let report =
        recompute_in_dir(&dir, input.block_id.as_deref(), &now).map_err(|e| seed_err(TOOL, e))?;
    let mut out = serde_json::to_value(&report).map_err(M1ndError::Serde)?;
    out["evaluated_at"] = json!(now);
    Ok(out)
}

// ---------------------------------------------------------------------------
// system_blocks_archive (WRITE) — retire/restore a block (Slice 3)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ArchiveInput {
    #[allow(dead_code)]
    pub agent_id: Option<String>,
    /// The `store_version` the caller read (OCC key, PRD §3.1).
    pub expected_store_version: u64,
    /// The blocks to archive or restore.
    pub block_ids: Vec<String>,
    /// `"archive"` (retire, remembering the prior state) or `"restore"` (return to
    /// the real prior state).
    pub mode: String,
}

/// `system_blocks_archive` (WRITE). Archives blocks (flip to `archived`, remembering
/// the prior state so a restore is honest) or restores them (return to that prior
/// state). Archived blocks are excluded from active rollup counts — the backend only
/// MARKS the state, it never deletes data. OCC-checked.
pub fn handle_system_blocks_archive(
    state: &mut SessionState,
    input: ArchiveInput,
) -> M1ndResult<Value> {
    const TOOL: &str = "system_blocks_archive";
    let mode = match input.mode.as_str() {
        "archive" => ArchiveMode::Archive,
        "restore" => ArchiveMode::Restore,
        other => {
            return Err(M1ndError::InvalidParams {
                tool: TOOL.to_string(),
                detail: format!("mode must be \"archive\" or \"restore\", got \"{other}\""),
            })
        }
    };
    if input.block_ids.is_empty() {
        return Err(M1ndError::InvalidParams {
            tool: TOOL.to_string(),
            detail: "block_ids must name at least one block".to_string(),
        });
    }
    let dir = store_dir(state);
    let (_store, summary) =
        archive_in_dir(&dir, input.expected_store_version, &input.block_ids, mode)
            .map_err(|e| seed_err(TOOL, e))?;
    Ok(json!({
        "store_version": summary.store_version,
        "mode": summary.mode,
        "changed_block_ids": summary.changed_block_ids,
    }))
}

// ---------------------------------------------------------------------------
// system_blocks_delete (WRITE) — permanently remove a block (Slice 3)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DeleteInput {
    #[allow(dead_code)]
    pub agent_id: Option<String>,
    /// The `store_version` the caller read (OCC key, PRD §3.1).
    pub expected_store_version: u64,
    /// The block to remove.
    pub block_id: String,
    /// Mandatory guard: a delete drops the block and all its receipts permanently.
    /// Without it the call refuses and suggests archive.
    #[serde(default)]
    pub force: bool,
}

/// `system_blocks_delete` (WRITE). Removes a block from the store FOR REAL and
/// reports how many receipts died with it. `force:true` is mandatory — without it
/// the call refuses honestly and suggests archive (which keeps the history).
/// OCC-checked.
pub fn handle_system_blocks_delete(
    state: &mut SessionState,
    input: DeleteInput,
) -> M1ndResult<Value> {
    const TOOL: &str = "system_blocks_delete";
    let dir = store_dir(state);
    let (_store, summary) = delete_in_dir(
        &dir,
        input.expected_store_version,
        &input.block_id,
        input.force,
    )
    .map_err(|e| seed_err(TOOL, e))?;
    Ok(json!({
        "store_version": summary.store_version,
        "deleted_block_id": summary.deleted_block_id,
        "receipts_removed": summary.receipts_removed,
        "warning": format!(
            "block '{}' and its {} receipt(s) were permanently deleted",
            summary.deleted_block_id, summary.receipts_removed
        ),
    }))
}

// ---------------------------------------------------------------------------
// candidate_edit (WRITE) — F11-a typed batch edit on a candidate skeleton
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CandidateEditInput {
    #[allow(dead_code)]
    pub agent_id: Option<String>,
    /// The `store_version` the caller read (OCC key, PRD §3.1).
    pub expected_store_version: u64,
    /// The typed edit ops (rename/merge/split/move_member/resolve_seam/assign_unmapped).
    pub ops: Vec<crate::candidate_edit::EditOp>,
    /// The authoring seat for provenance (§1c): `"owner"` (the GUI, default) stamps
    /// `named_by:owner`; `"runner"` (an agent seat) stamps `named_by:runner`.
    #[serde(default)]
    pub by: Option<String>,
}

/// `candidate_edit` (WRITE, F11-a). Applies a typed batch of edits to the CANDIDATE
/// skeleton under one OCC transaction with preflight-on-a-clone (o1): the whole batch
/// is validated against a working copy and only a total success persists (once) and
/// bumps `store_version` (once). A `ratified` skeleton refuses every op
/// (`skeleton_not_candidate`, §1a); a failing op returns its index honestly.
pub fn handle_candidate_edit(
    state: &mut SessionState,
    input: CandidateEditInput,
) -> M1ndResult<Value> {
    const TOOL: &str = "candidate_edit";
    let seat = crate::candidate_edit::EditSeat::parse(input.by.as_deref()).map_err(|detail| {
        M1ndError::InvalidParams {
            tool: TOOL.to_string(),
            detail,
        }
    })?;
    let dir = store_dir(state);
    let store = candidate_edit_in_dir(&dir, input.expected_store_version, &input.ops, seat)
        .map_err(|e| seed_err(TOOL, e))?;
    Ok(json!({
        "store_version": store.store_version,
        "block_count": store.blocks.len(),
        "ops_applied": input.ops.len(),
        "store": store,
    }))
}

// ---------------------------------------------------------------------------
// candidate_lease (WRITE) — F11-a advisory curation lease (o4)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CandidateLeaseInput {
    /// The agent identity that holds/refreshes/releases the lease — REQUIRED (the
    /// lease is keyed on it; the owner is the single serialization point, o4).
    pub agent_id: String,
    /// `"acquire"` | `"refresh"` | `"release"`.
    pub action: String,
    /// The lease lifetime in seconds; omit for the default. Used by acquire/refresh.
    #[serde(default)]
    pub ttl_secs: Option<u64>,
}

/// `candidate_lease` (WRITE, F11-a). The advisory curation lease (o4): `acquire` is an
/// atomic compare-and-set (granted iff the lease is free, expired, or already this
/// agent's), `refresh` extends the owner-held TTL, and `release` clears it — an
/// expired lease is reclaimable by anyone (no dead-agent trap). It is ADVISORY:
/// `candidate_edit` never requires a held lease, and the lease never bumps
/// `store_version`, so it can never block the owner or invalidate a pending edit.
pub fn handle_candidate_lease(
    state: &mut SessionState,
    input: CandidateLeaseInput,
) -> M1ndResult<Value> {
    const TOOL: &str = "candidate_lease";
    let action = LeaseAction::parse(&input.action).map_err(|detail| M1ndError::InvalidParams {
        tool: TOOL.to_string(),
        detail,
    })?;
    let ttl = input.ttl_secs.unwrap_or(DEFAULT_LEASE_TTL_SECS);
    let ms = now_ms();
    let now_iso = iso8601_from_ms(ms);
    let until_iso = iso8601_from_ms(ms.saturating_add(ttl.saturating_mul(1000)));
    let dir = store_dir(state);
    let (store, summary) =
        candidate_lease_in_dir(&dir, action, &input.agent_id, &now_iso, &until_iso)
            .map_err(|e| seed_err(TOOL, e))?;
    Ok(json!({
        "state": summary.state,
        "curating_by": summary.curating_by,
        "curating_until": summary.curating_until,
        // The lease is advisory bookkeeping — it never bumps the OCC counter.
        "store_version": store.store_version,
    }))
}

// ---------------------------------------------------------------------------
// Timestamp — RFC3339 UTC, dependency-free (mirrors the repo's civil-date math)
// ---------------------------------------------------------------------------

/// The current instant as an RFC3339 UTC string (`YYYY-MM-DDTHH:MM:SSZ`), using
/// the repo's dependency-free civil-date math (as in `soul_handlers::ymd` and
/// `mailbox::days_from_civil`) rather than pulling in a datetime crate.
fn now_iso8601() -> String {
    iso8601_from_ms(now_ms())
}

/// Format epoch-millis as `YYYY-MM-DDTHH:MM:SSZ` (UTC). Howard Hinnant's civil
/// algorithm — the same day math the rest of the codebase already uses.
pub(crate) fn iso8601_from_ms(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (h, mi, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {

    #[test]
    fn scan_and_reconcile_use_the_code_root_never_the_store_dir() {
        // The first virgin-repo scan listed the brain's .light.md memories as the
        // repo: a hosted brain's raw workspace_root is its STORE dir. The handlers
        // must resolve the CODE root (a real ingest root) instead.
        let temp = tempfile::tempdir().expect("tempdir");
        let repo = temp.path().join("real-repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo");
        std::fs::write(repo.join("src/lib.rs"), "pub fn x() {}\n").expect("file");
        let store_dir = temp.path().join("brain-store/agent-memory");
        std::fs::create_dir_all(&store_dir).expect("store");
        std::fs::write(store_dir.join("doctrine.light.md"), "memory\n").expect("mem");

        let config = crate::server::McpConfig {
            graph_source: temp.path().join("g.json"),
            plasticity_state: temp.path().join("p.json"),
            runtime_dir: Some(temp.path().join("rt")),
            ..crate::server::McpConfig::default()
        };
        let mut state = crate::session::SessionState::initialize(
            m1nd_core::graph::Graph::new(),
            &config,
            m1nd_core::domain::DomainConfig::code(),
        )
        .expect("init");
        // The hosted-brain shape: workspace points at the STORE dir; the real repo
        // is an ingest root.
        state.workspace_root = Some(store_dir.to_string_lossy().to_string());
        state.ingest_roots = vec![repo.to_string_lossy().to_string()];

        assert_eq!(
            state.code_root_path().as_deref(),
            Some(repo.to_string_lossy().as_ref()),
            "the code root is the ingest root, never the store dir"
        );

        let out = handle_skeleton_candidate(
            &mut state,
            SkeletonCandidateInput {
                agent_id: Some("t".to_string()),
                expected_store_version: None,
                review_limit: None,
                naming: None,
            },
        )
        .expect("scan runs against the real repo");
        let members: Vec<String> = out["candidate_seed"]["blocks"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|b| b["membership"].as_array().cloned().into_iter().flatten())
            .filter_map(|m| m["path"].as_str().map(str::to_string))
            .collect();
        assert!(
            members.iter().all(|p| !p.contains(".light.md")),
            "no memory sidecar file may enter a candidate: {members:?}"
        );
        assert!(
            members.iter().any(|p| p.contains("lib.rs")),
            "the real repo's files are the candidate: {members:?}"
        );
    }

    use super::*;

    #[test]
    fn iso8601_epoch_and_known_instant() {
        assert_eq!(iso8601_from_ms(0), "1970-01-01T00:00:00Z");
        // 2026-07-09T12:34:56Z -> known epoch seconds.
        let ms = 1_783_600_496_000; // 2026-07-09T02:14:56Z
        let s = iso8601_from_ms(ms);
        assert!(s.starts_with("2026-07-09T"), "unexpected: {s}");
        assert!(s.ends_with('Z') && s.len() == 20, "rfc3339 shape: {s}");
    }

    // =======================================================================
    // F11-b — the scan→naming-runner wiring.
    // =======================================================================

    /// Build a session over a scratch repo (one real file), exactly like the
    /// code-root test above: the workspace is a store dir, the repo is the ingest
    /// root, so the scan resolves the CODE root.
    fn naming_test_state(
        temp: &tempfile::TempDir,
    ) -> (crate::session::SessionState, std::path::PathBuf) {
        let repo = temp.path().join("real-repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo");
        std::fs::write(repo.join("src/lib.rs"), "pub fn x() {}\n").expect("file");
        let store_dir = temp.path().join("brain-store/agent-memory");
        std::fs::create_dir_all(&store_dir).expect("store");

        let config = crate::server::McpConfig {
            graph_source: temp.path().join("g.json"),
            plasticity_state: temp.path().join("p.json"),
            runtime_dir: Some(temp.path().join("rt")),
            ..crate::server::McpConfig::default()
        };
        let mut state = crate::session::SessionState::initialize(
            m1nd_core::graph::Graph::new(),
            &config,
            m1nd_core::domain::DomainConfig::code(),
        )
        .expect("init");
        state.workspace_root = Some(store_dir.to_string_lossy().to_string());
        state.ingest_roots = vec![repo.to_string_lossy().to_string()];
        (state, repo)
    }

    /// F11-b regression: with NO announced runnerd the auto scan is byte-identical
    /// to the offline behavior — every block heuristic + needing the owner, and
    /// the F0c-a fallback note VERBATIM.
    #[test]
    fn skeleton_candidate_without_runnerd_stays_fully_heuristic() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (mut state, _repo) = naming_test_state(&temp);
        assert!(
            state.runnerd_naming.is_none(),
            "a fresh session carries no naming handle"
        );
        let out = handle_skeleton_candidate(
            &mut state,
            SkeletonCandidateInput {
                agent_id: Some("t".to_string()),
                expected_store_version: None,
                review_limit: None,
                naming: None, // auto
            },
        )
        .expect("scan runs");
        assert_eq!(out["report"]["naming"]["applied"], "heuristic");
        assert_eq!(out["report"]["naming"]["runner_available"], false);
        assert_eq!(
            out["report"]["naming"]["note"],
            "naming-runner hook is backend-declared but not invoked in F0c-a; heuristic provisional names were used",
            "the offline fallback note is byte-identical to the pre-F11-b behavior"
        );
        for block in out["candidate_seed"]["blocks"].as_array().unwrap() {
            assert_eq!(block["candidate_meta"]["named_by"], "heuristic");
            assert_eq!(block["candidate_meta"]["needs_owner_naming"], true);
        }
    }

    /// A canned fake runnerd `/name` daemon: accepts one connection, parses the
    /// request's block ids, answers every block ok with a clean name. Never a
    /// real runner, never an LLM.
    fn spawn_fake_name_daemon(expect_secret: &'static str) -> u16 {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().expect("accept");
            let mut req = Vec::new();
            let mut chunk = [0u8; 8192];
            loop {
                let n = sock.read(&mut chunk).expect("read");
                req.extend_from_slice(&chunk[..n]);
                if let Some(pos) = req.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&req[..pos]).to_string();
                    let want: usize = head
                        .lines()
                        .find_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            k.trim()
                                .eq_ignore_ascii_case("content-length")
                                .then(|| v.trim().parse().ok())?
                        })
                        .unwrap_or(0);
                    if req.len() >= pos + 4 + want {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let text = String::from_utf8_lossy(&req).to_string();
            assert!(
                text.contains(&format!("x-runnerd-secret: {expect_secret}")),
                "the owner must sign the /name call: {text}"
            );
            let body_start = text.find("\r\n\r\n").map(|p| p + 4).unwrap_or(0);
            let body: serde_json::Value =
                serde_json::from_str(&text[body_start..]).expect("request body parses");
            let results: Vec<serde_json::Value> = body["blocks"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|b| {
                    // The packet must carry paths, never file bodies.
                    assert!(
                        b["packet"]["member_paths"].is_array(),
                        "packet carries member paths: {b}"
                    );
                    json!({
                        "block_id": b["block_id"],
                        "ok": true,
                        "name": "Runner Named",
                        "purpose": "Named by the live naming-runner.",
                    })
                })
                .collect();
            let payload =
                serde_json::to_string(&json!({ "runner_id": "namer-1", "results": results }))
                    .unwrap();
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{payload}",
                payload.len()
            );
            sock.write_all(response.as_bytes()).expect("write");
        });
        port
    }

    const FAKE_NAMING_SECRET: &str =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn write_fake_naming_secret(owner_runtime_root: &std::path::Path) {
        let path = crate::runnerd_owner::secret_path(owner_runtime_root);
        std::fs::write(&path, FAKE_NAMING_SECRET).expect("secret");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .expect("private secret permissions");
        }
    }

    /// F11-b §2a end-to-end: an auto scan with a LIVE (fake) announced daemon
    /// lands runner names in the emitted seed, in the report, AND in the persisted
    /// store — needs_owner_naming false, so the o6 ratify gate opens without a
    /// human touch per block.
    #[test]
    fn skeleton_candidate_with_live_fake_daemon_lands_runner_names() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (mut state, _repo) = naming_test_state(&temp);

        // The owner runtime root carries the shared secret; the fake daemon is
        // announced in the registry — the two facts the naming path needs.
        let owner_rt = temp.path().join("owner-rt");
        std::fs::create_dir_all(&owner_rt).expect("owner rt");
        write_fake_naming_secret(&owner_rt);
        let port = spawn_fake_name_daemon(FAKE_NAMING_SECRET);
        let registry = std::sync::Arc::new(crate::runnerd_owner::RunnerdRegistry::default());
        registry.register(&["namer-1".to_string()], port, 1);
        state.runnerd_naming = Some(crate::runnerd_owner::NamingRunnerHandle {
            registry,
            owner_runtime_root: owner_rt,
        });

        let out = handle_skeleton_candidate(
            &mut state,
            SkeletonCandidateInput {
                agent_id: Some("t".to_string()),
                expected_store_version: None,
                review_limit: None,
                naming: None, // auto
            },
        )
        .expect("scan + naming runs");

        // The report says honestly that the runner named everything.
        assert_eq!(out["report"]["naming"]["applied"], "runner");
        assert_eq!(out["report"]["naming"]["runner_available"], true);
        let blocks = out["candidate_seed"]["blocks"].as_array().unwrap();
        assert!(!blocks.is_empty());
        for block in blocks {
            assert_eq!(block["name"], "Runner Named");
            assert_eq!(block["candidate_meta"]["named_by"], "runner");
            assert_eq!(
                block["candidate_meta"]["needs_owner_naming"], false,
                "runner-named blocks are ratifiable without an individual touch (0b)"
            );
        }
        // The PERSISTED store carries the runner names (the scan applied them
        // BEFORE the store transaction).
        let store = SystemBlockStore::load(&store_dir(&state))
            .expect("load")
            .expect("present");
        for block in &store.blocks {
            assert_eq!(block.name, "Runner Named");
            let meta = block.candidate_meta.as_ref().unwrap();
            assert_eq!(meta.named_by, crate::system_blocks::NamedBy::Runner);
            assert!(!meta.needs_owner_naming);
        }
    }

    // =======================================================================
    // Slice 2 — the scan narrates its real phases on the SSE channel
    // (docs/uml/scan-loading.md). The wire is mocked to a Vec: no HTTP, no
    // browser — the emit points and their order are what these pin.
    // =======================================================================

    /// Install a capturing scan-progress sink and return the shared buffer the
    /// emitted phase events land in.
    fn capture_scan_phases(
        state: &mut crate::session::SessionState,
    ) -> std::sync::Arc<std::sync::Mutex<Vec<ScanProgressEvent>>> {
        let captured = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink_buf = captured.clone();
        state.scan_progress_sink = Some(std::sync::Arc::new(move |event: &ScanProgressEvent| {
            sink_buf.lock().unwrap().push(event.clone());
        }));
        captured
    }

    fn phase_order(events: &[ScanProgressEvent]) -> Vec<String> {
        events.iter().map(|e| e.phase.clone()).collect()
    }

    /// With NO announced runnerd the naming phase is SKIPPED (naming stays heuristic
    /// inside clustering) — the honest order is file_list → clustering → persisting
    /// → done, and the verb's response is byte-unchanged.
    #[test]
    fn scan_progress_emits_ordered_phases_without_runner() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (mut state, _repo) = naming_test_state(&temp);
        assert!(state.runnerd_naming.is_none());
        let captured = capture_scan_phases(&mut state);

        let out = handle_skeleton_candidate(
            &mut state,
            SkeletonCandidateInput {
                agent_id: Some("t".to_string()),
                expected_store_version: None,
                review_limit: None,
                naming: None, // auto — but no runnerd, so no naming phase
            },
        )
        .expect("scan runs");
        assert_eq!(out["present"], true, "the verb's response is untouched");

        let events = captured.lock().unwrap();
        assert_eq!(
            phase_order(&events),
            vec!["file_list", "clustering", "persisting", "done"],
            "no live runner → no naming phase; the terminal is `done`"
        );
        assert!(
            events[0].file_count.is_some(),
            "file_list carries its count"
        );
        assert!(
            events[1].node_count.is_some(),
            "clustering carries the graph size"
        );
        let done = events.last().unwrap();
        assert_eq!(
            done.block_count,
            Some(out["block_count"].as_u64().unwrap() as usize),
            "done reports the block count that landed"
        );
    }

    /// With a LIVE (fake) announced daemon the naming phase appears BETWEEN
    /// clustering and persisting, carrying the block count + the budget's wave
    /// estimate (a fact, never a per-wave percentage).
    #[test]
    fn scan_progress_emits_naming_phase_with_live_runner() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (mut state, _repo) = naming_test_state(&temp);

        let owner_rt = temp.path().join("owner-rt");
        std::fs::create_dir_all(&owner_rt).expect("owner rt");
        write_fake_naming_secret(&owner_rt);
        let port = spawn_fake_name_daemon(FAKE_NAMING_SECRET);
        let registry = std::sync::Arc::new(crate::runnerd_owner::RunnerdRegistry::default());
        registry.register(&["namer-1".to_string()], port, 1);
        state.runnerd_naming = Some(crate::runnerd_owner::NamingRunnerHandle {
            registry,
            owner_runtime_root: owner_rt,
        });

        let captured = capture_scan_phases(&mut state);
        let out = handle_skeleton_candidate(
            &mut state,
            SkeletonCandidateInput {
                agent_id: Some("t".to_string()),
                expected_store_version: None,
                review_limit: None,
                naming: None, // auto
            },
        )
        .expect("scan + naming runs");
        assert_eq!(out["report"]["naming"]["applied"], "runner");

        let events = captured.lock().unwrap();
        assert_eq!(
            phase_order(&events),
            vec!["file_list", "clustering", "naming", "persisting", "done"],
            "the live runner adds the naming phase between clustering and persisting"
        );
        let naming = events.iter().find(|e| e.phase == "naming").unwrap();
        assert!(
            naming.block_count.unwrap_or(0) >= 1,
            "naming names at least one block"
        );
        assert!(
            naming.naming_waves.unwrap_or(0) >= 1,
            "the wave estimate is a positive fact, not a fraction"
        );
    }

    /// A persist OCC conflict emits the terminal `failed` phase (with the honest
    /// error) BEFORE the verb surfaces the same refusal — the SSE stream closes
    /// honestly and the emit is fail-open (the refusal still propagates unchanged).
    #[test]
    fn scan_progress_emits_failed_on_persist_conflict() {
        let temp = tempfile::tempdir().expect("tempdir");
        let (mut state, _repo) = naming_test_state(&temp);
        // First scan lands a candidate store at v1 (heuristic — no runner needed).
        handle_skeleton_candidate(
            &mut state,
            SkeletonCandidateInput {
                agent_id: Some("t".to_string()),
                expected_store_version: None,
                review_limit: None,
                naming: Some("heuristic".to_string()),
            },
        )
        .expect("first scan lands v1");

        // A second scan keyed on a STALE version conflicts at persist.
        let captured = capture_scan_phases(&mut state);
        let err = handle_skeleton_candidate(
            &mut state,
            SkeletonCandidateInput {
                agent_id: Some("t".to_string()),
                expected_store_version: Some(999),
                review_limit: None,
                naming: Some("heuristic".to_string()),
            },
        );
        assert!(err.is_err(), "a stale OCC key is refused (nothing applied)");

        let events = captured.lock().unwrap();
        assert!(
            events.iter().any(|e| e.phase == "persisting"),
            "persisting is announced before the failure"
        );
        let terminal = events.last().unwrap();
        assert_eq!(terminal.phase, "failed", "the terminal phase is `failed`");
        assert!(
            terminal.error.is_some(),
            "the failed phase carries the honest owner string"
        );
    }

    /// The event shape is phases + honest counts — NEVER a percentage/fraction
    /// field (the house honesty law holds on the wire too).
    #[test]
    fn scan_progress_event_shape_has_no_fabricated_fraction() {
        let naming = serde_json::to_value(ScanProgressEvent::naming(8, 2)).unwrap();
        assert_eq!(naming["phase"], "naming");
        assert_eq!(naming["block_count"], 8);
        assert_eq!(naming["naming_waves"], 2);

        for ev in [
            ScanProgressEvent::file_list(300),
            ScanProgressEvent::clustering(3210, 9000),
            ScanProgressEvent::naming(8, 2),
            ScanProgressEvent::persisting(12),
            ScanProgressEvent::done(12),
            ScanProgressEvent::failed("boom"),
        ] {
            let v = serde_json::to_value(&ev).unwrap();
            let obj = v.as_object().unwrap();
            for banned in ["percent", "pct", "progress", "fraction", "ratio"] {
                assert!(
                    !obj.contains_key(banned),
                    "phase {} must not carry a `{banned}` field",
                    ev.phase
                );
            }
        }
        let failed =
            serde_json::to_value(ScanProgressEvent::failed("clustering exploded")).unwrap();
        assert_eq!(failed["error"], "clustering exploded");
    }
}
