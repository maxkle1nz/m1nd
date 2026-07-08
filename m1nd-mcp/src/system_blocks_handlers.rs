//! Human View v2 F0a — the SystemBlock store MCP verbs (Slice 2).
//!
//! Four verbs that serve the per-project-brain sidecar store defined in
//! [`crate::system_blocks`]:
//! - `system_blocks_snapshot` (READ) — the whole store, or an honest "no skeleton"
//! - `system_blocks_seed_import` (WRITE) — seed -> fresh store (`store_version = 1`)
//! - `system_blocks_ratify` (WRITE) — flip candidate blocks to ratified (OCC)
//! - `receipt_import` (WRITE) — attach anti-poison-checked evidence (OCC)
//!
//! Every WRITE is OCC-keyed on the `store_version` the caller read (PRD §3.1); a
//! stale write is rejected and nothing is applied. The three writers live in the
//! read-only-attach deny-list (`server::READ_ONLY_DENIED_TOOLS`); the snapshot
//! read is allowed. The store lives in the brain's runtime dir (`runtime_root`),
//! alongside the brain's other runtime artifacts (F0-TECH §1) — no new root is
//! invented. `receipt_recompute` (the expiry/staleness pass) is F0a slice 3, not
//! here.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{json, Value};

use m1nd_core::error::{M1ndError, M1ndResult};

use crate::session::SessionState;
use crate::system_blocks::{
    self, import_receipt_in_dir, import_seed_into_dir, ratify_in_dir, Receipt, SeedError,
    SystemBlockStore,
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
