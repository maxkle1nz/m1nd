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
use crate::skeleton_scan::{self, CandidateNamingMode, SkeletonScanOptions};
use crate::system_blocks::{
    self, archive_in_dir, delete_in_dir, import_receipt_in_dir, import_seed_into_dir,
    ratify_in_dir, recompute_in_dir, reconcile_in_dir, skeleton_candidate_in_dir, ArchiveMode,
    Receipt, SeedError, SystemBlockStore,
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
fn store_dir(state: &SessionState) -> PathBuf {
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
        Some(store) => Ok(json!({
            "present": true,
            "store_version": store.store_version,
            "block_count": store.blocks.len(),
            "store": store,
        })),
        None => Ok(json!({
            "present": false,
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
    /// `auto` declares the runner hook but F0c-a intentionally falls back to
    /// heuristic naming; `heuristic` skips the hook explicitly.
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
    let source_commit = git_head_commit(Path::new(&root)).unwrap_or_default();
    let repo_id = Path::new(&root)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repo")
        .to_string();
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
    let scan = skeleton_scan::scan_skeleton(
        scan_input,
        SkeletonScanOptions {
            naming,
            ..SkeletonScanOptions::default()
        },
    );
    let dir = store_dir(state);
    let (store, summary) =
        skeleton_candidate_in_dir(&dir, scan.seed.clone(), input.expected_store_version)
            .map_err(|e| seed_err(TOOL, e))?;

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

// ---------------------------------------------------------------------------
// system_blocks_ratify (WRITE)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
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
    state: &mut SessionState,
    input: RatifyInput,
) -> M1ndResult<Value> {
    const TOOL: &str = "system_blocks_ratify";
    let dir = store_dir(state);
    let ratified_at = now_iso8601();
    let (store, summary) = ratify_in_dir(
        &dir,
        input.expected_store_version,
        input.block_ids.as_deref(),
        &input.ratifier,
        &ratified_at,
    )
    .map_err(|e| seed_err(TOOL, e))?;
    Ok(json!({
        "store_version": store.store_version,
        "ratified_block_ids": summary.ratified_block_ids,
        "skeleton_state": "ratified",
        "ratifier": input.ratifier,
        "ratified_at": ratified_at,
    }))
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
}

/// `receipt_import` (WRITE). Attaches a receipt to a block after the anti-poison
/// gates (OCC, block exists, scope binds to the block's CURRENT versions, evidence
/// contract) all pass; bumps `store_version`.
pub fn handle_receipt_import(
    state: &mut SessionState,
    input: ReceiptImportInput,
) -> M1ndResult<Value> {
    const TOOL: &str = "receipt_import";
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
fn iso8601_from_ms(ms: u64) -> String {
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
}
