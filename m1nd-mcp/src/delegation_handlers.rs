//! ORGANISM ladder R6 — the delegation layer: `delegate` / `debrief`.
//!
//! `delegate` is a read-only fused composer in `north`'s class: ONE call turns
//! the retrieval half of a subagent's spec into a grounded, honest
//! `m1nd-delegation-packet-v0` — ranked anchors, a memory slice with age +
//! author, known static dependents, a staleness header, an explicit list of what
//! m1nd could NOT determine, and a deterministic `prompt_markdown` an agent can
//! read straight. `debrief` is the ONLY mutation, and it mutates only through
//! existing verbs (`memorize` / `learn`): it grades the spawned agent's real diff
//! against the packet and teaches the graph, so every delegation makes the next
//! packet smarter.
//!
//! REUSE MAP (NEXTGEN-AGENT-PRD §O.12.3): `delegate` composes `trust_selftest`
//! (binding), the `am_i_stale` inventory-hash baseline (staleness), `orient` +
//! `focus` (anchors + sufficiency), north's L1GHT recall (memory slice), and
//! `impact` (known static dependents). `debrief` composes `memorize`
//! (`handle_light_author`) + `learn` (`handle_learn`). The registry mirrors the
//! mission-store file-per-record pattern (`save_mission`/`load_mission`).
//!
//! THE CHILD LAW (ORGANISM-PRD §C5.3, constitutional): the packet's
//! `mission.binding` NAMES the brain the child must land on — it is the SAME datum
//! reception (`M1nd-Caller-Root` ↔ `covers_root`) compares against, at two hops.
//! The child never chooses; it VERIFIES the binding via reception (silent on
//! match). [`binding_workspace_root`] is the shared accessor that makes this link
//! explicit and testable.
//!
//! SLICE SCOPE (§O.12.10): slice 1 = the project-tier packet WITHOUT stage-5
//! enrichment (no `predict`/`trust`/`tremor`/`xray_gate` — each omission adds a
//! `non_claims` line); slice 2 = `debrief` + the outcomes ledger. Killed items
//! (§O.12.2) are NOT built: no `force`, no `ambiguous`-as-abstain, no typed packet
//! struct, no CLI subcommand, no graph locks, no auto-debrief; the outcome enum is
//! exactly `success | failure | partial`.

use crate::layer_handlers;
use crate::light_author_handlers::{handle_light_author, LightAuthorInput, LightClaim};
use crate::protocol::layers;
use crate::session::SessionState;
use crate::tools;
use crate::util::now_ms;
use m1nd_core::error::{M1ndError, M1ndResult};
use serde_json::{json, Value};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Uncalibrated constants (§O.12.5). Named, module-local, flagged uncalibrated:
// swept only at N ≥ 30 debriefed delegations (§O.12.8). No quality number rides
// on them until then.
// ---------------------------------------------------------------------------

/// A `gathering` sufficiency below this top_score has activated no coherent
/// subgraph — the knee-test verdict, not a new metric. UNCALIBRATED guess.
const DELEGATE_MIN_TOP_SCORE: f32 = 0.35;
/// Paired with the top_score floor: below this captured share the window is too
/// thin to scope a mission honestly. UNCALIBRATED guess.
const DELEGATE_MIN_CAPTURED: f32 = 0.5;
/// The number of debriefed delegations before any abstain constant or budget is
/// tuned (§O.12.8). Until then `calibration.calibrated` is always false.
const CALIBRATION_NEEDED_ROWS: u64 = 30;
/// Packet time-to-live: a packet older than this is stale by construction (the
/// graph and disk both drift under it). 4 hours (§O.12.4).
const PACKET_TTL_MS: u64 = 4 * 60 * 60 * 1000;
/// Default token budget for the rendered packet — a cap, not a quota (§O.12.4).
/// `pub` so the routing layer reuses the SAME default when it re-renders the packet
/// after folding the medulla feed (M7), rather than duplicating the constant.
pub const DEFAULT_BUDGET_TOKENS: u64 = 2000;
/// Hard ceiling: beyond this the packet competes with the work itself.
pub const HARD_BUDGET_TOKENS: u64 = 8000;
/// Default blast-radius cap for the dependents pass.
const DEFAULT_MAX_NODES: usize = 40;

// ---------------------------------------------------------------------------
// The child-reception link (ORGANISM-PRD §C5.3, constitutional).
// ---------------------------------------------------------------------------

/// The workspace root the packet's `mission.binding` NAMES — the exact datum the
/// child verifies via reception (`covers_root` / `reception_verdict`).
///
/// This is the single accessor that makes the §C5.3 child law real in code:
/// `M1nd-Caller-Root` (reception's wire fact) and `mission.binding.workspace_root`
/// (the packet's wire fact) are the SAME datum at two hops. A child that lands
/// with `caller_root == binding_workspace_root(&state)` gets
/// `state.covers_root(caller_root) == true` → SILENT reception (no block); a
/// mismatched root gets the reception block. The packet writes exactly this value
/// so the two hops are provably identical.
///
/// It is the brain's real project root (`project_root_display`, the repo it maps),
/// falling back to `workspace_root`. `None` only when the brain has no roots
/// (empty graph) — in which case `delegate` never emits a packet anyway.
pub fn binding_workspace_root(state: &SessionState) -> Option<String> {
    state
        .project_root_display()
        .or_else(|| state.workspace_root.clone())
}

// ---------------------------------------------------------------------------
// Registry — dumb file-per-record store (the debrief join key). Mirrors the
// mission-store pattern (mission_handlers::save_mission/load_mission) exactly.
// ---------------------------------------------------------------------------

fn delegation_dir(state: &SessionState) -> PathBuf {
    state.runtime_root.join("delegations")
}

fn validate_delegation_id(delegation_id: &str) -> M1ndResult<()> {
    let valid = delegation_id.starts_with("dlg_")
        && delegation_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-');
    if valid {
        Ok(())
    } else {
        Err(M1ndError::InvalidParams {
            tool: "delegate".into(),
            detail: "delegation_id must be a generated dlg_* id with no path separators".into(),
        })
    }
}

fn delegation_path(state: &SessionState, delegation_id: &str) -> M1ndResult<PathBuf> {
    validate_delegation_id(delegation_id)?;
    Ok(delegation_dir(state).join(format!("{delegation_id}.json")))
}

/// Persist one delegation record as `<delegation_id>.json` — a dumb record, the
/// debrief join key. The record IS the packet's structured half plus a `status`.
fn save_delegation(state: &SessionState, record: &Value) -> M1ndResult<()> {
    let delegation_id = record
        .get("delegation_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: "delegate".into(),
            detail: "delegation record has no delegation_id".into(),
        })?;
    let dir = delegation_dir(state);
    fs::create_dir_all(&dir).map_err(M1ndError::Io)?;
    let path = delegation_path(state, delegation_id)?;
    let body = serde_json::to_string_pretty(record).map_err(M1ndError::Serde)?;
    fs::write(path, body).map_err(M1ndError::Io)
}

/// Load a delegation record by id. Unknown id is a hard error — no guessing
/// (§O.12.6 step 1).
fn load_delegation(state: &SessionState, delegation_id: &str) -> M1ndResult<Value> {
    let path = delegation_path(state, delegation_id)?;
    let body = fs::read_to_string(&path).map_err(|error| M1ndError::InvalidParams {
        tool: "debrief".into(),
        detail: format!("delegation_id {delegation_id} could not be loaded: {error}"),
    })?;
    serde_json::from_str(&body).map_err(M1ndError::Serde)
}

/// Live sibling records (status still `live`), for the same-worktree overlap
/// cross-check (§O.12.6 step 3). Best-effort: a read error yields an empty set.
fn live_sibling_records(state: &SessionState, exclude_id: &str) -> Vec<Value> {
    let dir = delegation_dir(state);
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(body) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(rec) = serde_json::from_str::<Value>(&body) else {
            continue;
        };
        let id = rec.get("delegation_id").and_then(|v| v.as_str());
        let status = rec.get("status").and_then(|v| v.as_str()).unwrap_or("");
        if id == Some(exclude_id) {
            continue;
        }
        if status == "live" {
            out.push(rec);
        }
    }
    out
}

/// The outcomes ledger path — one JSONL row appended per debrief (§O.12.8).
fn outcomes_path(state: &SessionState) -> PathBuf {
    delegation_dir(state).join("outcomes.jsonl")
}

/// Parse the outcomes ledger into rows for the calibration reducer. One JSON
/// object per non-blank line; unparseable lines are skipped (the ledger is
/// append-only and a torn tail line must never abort the read).
fn outcomes_rows(state: &SessionState) -> Vec<Value> {
    match fs::read_to_string(outcomes_path(state)) {
        Ok(body) => body
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<Value>(l).ok())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// The delegation-level calibration derived from the `outcomes.jsonl` ledger
/// (§O.12.8). `calibrated` is the ONLY field that prints below N ≥ 30 — the three
/// quality metrics stay `None` until then, because "no quality number prints
/// anywhere — bands and counts only" while uncalibrated (the exact sin `predict`'s
/// gate exists to prevent, aimed at m1nd's own self-grades).
#[derive(Debug, Clone, PartialEq)]
struct CalibrationMetrics {
    rows: u64,
    calibrated: bool,
    /// |touched ∩ (may_touch ∪ expected_change)| / |touched| across all rows.
    scope_precision: Option<f64>,
    /// |unpredicted| / |touched| across all rows (packet-quality miss, not a
    /// subagent sin).
    miss_rate: Option<f64>,
    /// P(failure | contact) − P(failure | stayed): did the guard mark genuinely
    /// fragile things? Positive ⇒ contacted dependents did correlate with failure.
    dependents_honesty: Option<f64>,
}

/// PURE reducer: `outcomes.jsonl` rows → calibration metrics (§O.12.8). No I/O, no
/// state — every input is a parsed ledger row, so it is exhaustively unit-testable
/// on synthetic rows. Metric definitions are lifted verbatim from §O.12.8:
///   scope_precision = Σ(in_scope + expected_change) / Σ(touched_count)
///   miss_rate       = Σ(unpredicted) / Σ(touched_count)
///   dependents_honesty = P(failure|contact) − P(failure|stayed)
/// The three quality numbers are withheld (`None`) until N ≥ 30 — bands and counts
/// only while uncalibrated. `calibrated` is purely `rows.len() >= 30`.
fn calibration_metrics_from_rows(rows: &[Value]) -> CalibrationMetrics {
    let n = rows.len() as u64;
    let calibrated = n >= CALIBRATION_NEEDED_ROWS;

    let field_u64 = |row: &Value, key: &str| -> u64 {
        row.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
    };

    let mut touched_total: u64 = 0;
    let mut predicted_total: u64 = 0; // in_scope + expected_change
    let mut unpredicted_total: u64 = 0;

    // dependents_honesty accumulators, partitioned by whether a row contacted any
    // known dependent.
    let mut contact_rows: u64 = 0;
    let mut contact_failures: u64 = 0;
    let mut stayed_rows: u64 = 0;
    let mut stayed_failures: u64 = 0;

    for row in rows {
        let touched = field_u64(row, "touched_count");
        touched_total += touched;
        predicted_total += field_u64(row, "in_scope") + field_u64(row, "expected_change");
        unpredicted_total += field_u64(row, "unpredicted");

        let is_failure = row.get("outcome").and_then(|v| v.as_str()) == Some("failure");
        if field_u64(row, "dependent_contact") > 0 {
            contact_rows += 1;
            if is_failure {
                contact_failures += 1;
            }
        } else {
            stayed_rows += 1;
            if is_failure {
                stayed_failures += 1;
            }
        }
    }

    // Quality numbers ONLY once calibrated; and only when the denominator exists.
    let scope_precision = (calibrated && touched_total > 0)
        .then(|| predicted_total as f64 / touched_total as f64);
    let miss_rate = (calibrated && touched_total > 0)
        .then(|| unpredicted_total as f64 / touched_total as f64);
    let dependents_honesty = (calibrated && contact_rows > 0 && stayed_rows > 0).then(|| {
        let p_fail_contact = contact_failures as f64 / contact_rows as f64;
        let p_fail_stayed = stayed_failures as f64 / stayed_rows as f64;
        p_fail_contact - p_fail_stayed
    });

    CalibrationMetrics {
        rows: n,
        calibrated,
        scope_precision,
        miss_rate,
        dependents_honesty,
    }
}

/// Render the packet's `calibration` block. `calibrated` / `rows` / `needed`
/// always print (the honest count-only header); the three quality numbers are
/// added ONLY once `calibrated` — while uncalibrated the block carries counts and
/// bands, never a number that reads as certified quality (§O.12.8).
fn calibration_block(metrics: &CalibrationMetrics) -> Value {
    let mut block = json!({
        "calibrated": metrics.calibrated,
        "rows": metrics.rows,
        "needed": CALIBRATION_NEEDED_ROWS,
    });
    if metrics.calibrated {
        if let Some(v) = metrics.scope_precision {
            block["scope_precision"] = json!(v);
        }
        if let Some(v) = metrics.miss_rate {
            block["miss_rate"] = json!(v);
        }
        if let Some(v) = metrics.dependents_honesty {
            block["dependents_honesty"] = json!(v);
        }
    }
    block
}

// ---------------------------------------------------------------------------
// Slice 1 — `delegate`: the read-only packet composer (north's class).
// ---------------------------------------------------------------------------

/// `delegate` — compose one grounded `m1nd-delegation-packet-v0` from the live
/// graph so a context-blind subagent behaves as if it had been in the
/// orchestrator's session, while never letting it pretend it was.
///
/// READ-ONLY: every composed piece is read-only (`trust_selftest`, the inventory
/// baseline, `orient`/`focus` over read-only query paths, north's L1GHT recall,
/// `impact`). It stays OFF `READ_ONLY_DENIED_TOOLS` — that omission IS its ambient
/// legality (§O.12.3). Its one side effect is the dumb registry write, the debrief
/// join key — a record of the packet, not a graph mutation.
pub fn handle_delegate(state: &mut SessionState, params: &Value) -> M1ndResult<Value> {
    let agent_id = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: "delegate".into(),
            detail: "delegate requires an `agent_id` string".into(),
        })?
        .to_string();
    let task = params
        .get("task")
        .and_then(|v| v.as_str())
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: "delegate".into(),
            detail: "delegate requires a `task` string describing what the subagent is to do"
                .into(),
        })?
        .to_string();
    if task.trim().is_empty() {
        return Err(M1ndError::InvalidParams {
            tool: "delegate".into(),
            detail: "delegate `task` must be non-empty".into(),
        });
    }

    // Optional scope + budget.
    let declared_paths: Vec<String> = params
        .get("scope")
        .and_then(|s| s.get("paths"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let seeds: Vec<String> = params
        .get("scope")
        .and_then(|s| s.get("seeds"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let budget_tokens = params
        .get("budget")
        .and_then(|b| b.get("tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_BUDGET_TOKENS)
        .min(HARD_BUDGET_TOKENS);
    let max_nodes = params
        .get("budget")
        .and_then(|b| b.get("max_nodes"))
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(DEFAULT_MAX_NODES);
    let subagent_hint = params
        .get("subagent_hint")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // 1. BINDING — one trust_selftest gives the verdict + graph state. This is the
    //    mother-pre-filled reception (§C5.3): it NAMES the brain the child must land
    //    on (`workspace_root` = binding_workspace_root, the covers_root datum).
    let trust = tools::handle_trust_selftest(
        state,
        crate::protocol::TrustSelftestInput {
            agent_id: agent_id.clone(),
            observed_tool_count: None,
            available_tools: Vec::new(),
            missing_tools: Vec::new(),
            observed_tool: None,
            observed_proof_state: None,
            observed_candidates: None,
            scope: None,
            error_text: None,
        },
    )?;
    let trust_mode = trust
        .get("verdict")
        .and_then(|v| v.as_str())
        .unwrap_or("orientation_only")
        .to_string();
    let binding_ok = trust.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
    let graph_populated = trust
        .get("checks")
        .and_then(|c| c.get("graph_populated"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    // The named brain (the child-law datum). None on an empty graph → we abstain
    // below with needs_ingest before any packet is emitted.
    let named_root = binding_workspace_root(state);
    let binding = json!({
        "trust_mode": trust_mode,
        "ok": binding_ok,
        "workspace_root": named_root.clone(),
        "graph_populated": graph_populated,
    });

    // ABSTAIN CLASS `no_graph` (§O.12.5): a degraded/empty binding → needs_ingest +
    // the recovery playbook, north's EXACT behavior. NEVER a packet on an empty
    // graph.
    if !graph_populated || named_root.is_none() {
        let recovery = trust
            .get("recovery_playbook")
            .cloned()
            .filter(|v| !v.is_null());
        return Ok(json!({
            "schema": "m1nd-delegation-packet-v0",
            "verdict": "needs_ingest",
            "binding": binding,
            "next_move": "Run ingest for the intended repo, then call delegate again to get a grounded packet.",
            "recovery_playbook": recovery.unwrap_or(Value::Null),
            "honest_gaps": [
                "The graph is empty or unbound — no packet can be composed until ingest runs."
            ],
            "non_claims": [
                "delegate never emits a packet on an empty graph — it returns the repair honestly.",
            ],
        }));
    }
    let named_root = named_root.expect("named_root checked non-None above");

    // 2. STALENESS HEADER (§O.12.2 fix 1). Reuse am_i_stale's baseline: how many
    //    ingested files have changed on disk since ingest, plus the graph
    //    generation. Best-effort HEAD; null on unknown, never faked.
    let staleness = compose_staleness(state);

    // 3. CONTEXT — reuse orient (anchors + focus_nodes) + focus (sufficiency),
    //    exactly as north composes them. Zero reimplementation.
    let orient = crate::server::handle_orient_for_delegate(state, &agent_id, &task, 8)?;
    let anchors = orient.get("anchors").cloned().unwrap_or(json!([]));
    let focus_nodes = orient.get("focus_nodes").cloned().unwrap_or(json!([]));

    let focus_out = layer_handlers::handle_focus(
        state,
        layers::FocusInput {
            goal: task.clone(),
            agent_id: agent_id.clone(),
            token_budget: 2000,
            top_k: 60,
            scope: None,
            node_types: Vec::new(),
            min_score: 0.1,
        },
    )?;
    let sufficiency = &focus_out.sufficiency;
    let sufficiency_json = serde_json::to_value(sufficiency).unwrap_or(Value::Null);

    // 4. SEEDS — resolve every explicit seed to a node. ABSTAIN CLASS
    //    `seeds_unresolvable` (§O.12.5): if EVERY explicit seed failed resolution,
    //    the injection channel is broken → abstain with evidence + next_move.
    let mut resolved_seeds: Vec<String> = Vec::new();
    let mut unresolved_seeds: Vec<String> = Vec::new();
    if !seeds.is_empty() {
        for seed in &seeds {
            if node_resolves(state, seed) {
                resolved_seeds.push(seed.clone());
            } else {
                unresolved_seeds.push(seed.clone());
            }
        }
        if resolved_seeds.is_empty() {
            return Ok(json!({
                "schema": "m1nd-delegation-packet-v0",
                "verdict": "abstain",
                "abstain_class": "seeds_unresolvable",
                "binding": binding,
                "evidence": {
                    "seeds_given": seeds,
                    "seeds_resolved": resolved_seeds,
                    "seeds_unresolved": unresolved_seeds,
                    "reason": "every explicit scope.seed failed node resolution — the only context-injection channel resolved to nothing",
                },
                "next_move": "Re-check the seed ids against `search`/`seek` results (the graph may use different node ids), then re-call delegate with resolvable seeds.",
                "non_claims": [
                    "delegate abstains rather than emit a packet whose seeds resolve to nothing.",
                ],
            }));
        }
    }

    // ABSTAIN CLASS `unscopable` (§O.12.5): the task activated no coherent subgraph
    // — gathering AND top_score below the floor AND captured below the floor. NEVER
    // a bare no: evidence + next_move.
    let unscopable = sufficiency.state == "gathering"
        && sufficiency.top_score < DELEGATE_MIN_TOP_SCORE
        && sufficiency.captured < DELEGATE_MIN_CAPTURED;
    if unscopable {
        return Ok(json!({
            "schema": "m1nd-delegation-packet-v0",
            "verdict": "abstain",
            "abstain_class": "unscopable",
            "binding": binding,
            "evidence": {
                "sufficiency": sufficiency_json,
                "min_top_score": DELEGATE_MIN_TOP_SCORE,
                "min_captured": DELEGATE_MIN_CAPTURED,
                "reason": "the task text activated no coherent subgraph — top_score and captured are both below the (uncalibrated) scoping floor",
            },
            "next_move": "Refine the task text to name concrete symbols/files, or pass scope.seeds pointing at the relevant nodes, then re-call delegate.",
            "non_claims": [
                "the scoping floor constants are UNCALIBRATED guesses, swept only at N >= 30 outcomes.",
                "delegate abstains rather than ground a subagent on an incoherent activation.",
            ],
        }));
    }

    // 5. MEMORY SLICE — north's L1GHT recall filter, scoped to the memory
    //    namespace so code nodes never compete for the window. Each row carries a
    //    real age + author, or an honest absence (never faked to fresh, §O.12.9).
    let memory = recall_memory_slice(state, &agent_id, &task);

    // 6. COVERAGE header (§O.12.2 fix 2): which anchor files resolved. Best-effort,
    //    file-level static granularity — stamped honestly.
    let (coverage_resolved, coverage_unresolved) = anchor_coverage(state, &anchors, &focus_nodes);

    // 7. KNOWN STATIC DEPENDENTS (§O.12.2 fix 2, was do_not_break). Reuse impact on
    //    the top focus node; best-effort — empty + truncated:false is fine if none
    //    resolve. `granularity: file_level_static` is stamped honestly.
    let (dependents, expected_change, dependents_truncated) =
        static_dependents(state, &agent_id, &focus_nodes, max_nodes);

    // 8. SCOPE — declared paths win; else derived_from activation anchors.
    let (may_touch, scope_derived_from) = if !declared_paths.is_empty() {
        (declared_paths.clone(), Value::Null)
    } else {
        (anchor_paths(&anchors, &focus_nodes), json!("activation"))
    };

    // 9. PROOF heuristic — a Rust repo gets cargo test / clippy; every repo gets a
    //    grounded m1nd re-orient call. Honest heuristic, not a promise.
    let is_rust = repo_is_rust(state);
    let mut suggested_shell: Vec<Value> = Vec::new();
    if is_rust {
        suggested_shell.push(json!("cargo test"));
        suggested_shell.push(json!("cargo clippy --all-targets -- -D warnings"));
    }
    let suggested_m1nd_calls = json!([
        { "tool": "am_i_stale", "why": "confirm nothing under you changed since this packet before editing" },
        { "tool": "seek", "why": "verify the anchors below against the live graph" },
    ]);

    // 10. HONEST GAPS + NON-CLAIMS. non_claims is NEVER empty; each slice-1 omitted
    //     stage-5 section adds one honest drop-out line. honest_gaps names what
    //     m1nd could NOT see.
    let mut honest_gaps: Vec<String> = Vec::new();
    if !binding_ok {
        honest_gaps.push(format!(
            "Binding is not full trust ({trust_mode}) — treat retrieval as orientation only and verify against local files."
        ));
    }
    if dependents.is_empty() {
        honest_gaps.push(
            "No static dependents resolved for the focus set — coupling is UNKNOWN, not absent. Grep for callers before assuming isolation.".into(),
        );
    }
    if !unresolved_seeds.is_empty() {
        honest_gaps.push(format!(
            "{} of {} explicit seeds did not resolve to a node; the packet grounds only on the resolvable ones.",
            unresolved_seeds.len(),
            seeds.len()
        ));
    }
    if let Some(fc) = staleness
        .get("files_changed_since_ingest")
        .and_then(|v| v.as_u64())
    {
        if fc > 0 {
            honest_gaps.push(format!(
                "{fc} ingested file(s) have changed on disk since ingest — the map may be behind the code; re-read touched files."
            ));
        }
    }
    honest_gaps.push(
        "Static dependents are regex-extracted, file-level only — trait dispatch, closures, macros and string-keyed coupling are invisible to this pass.".into(),
    );

    let non_claims = json!([
        "the map is a file-level static view, not a fence — the file wins on what-is, the packet outranks only assumption.",
        "delegate composes read-only verbs; it does not ingest, mutate, or repair the graph (its only write is the dumb registry record).",
        "the memory slice is the DEFAULT beat (project + medulla): each row is labeled `tier` + `origin_brain` — never `all-brains`, never another project's private claims.",
        "stage-5 enrichment is NOT in this packet: no predict/co_change_warnings section (coupling is unknown, not absent).",
        "stage-5 enrichment is NOT in this packet: no trust/tremor risk_map (edit risk is ungraded here).",
        "stage-5 enrichment is NOT in this packet: no xray_gate must_not_touch (no ratified fence exists — breach cannot fire yet).",
        "an absent memory age means unknown authored time, never freshly authored.",
        "packet-quality numbers are withheld until N >= 30 debriefed outcomes — counts only.",
    ]);

    // 11. IDs + envelope.
    let created_ms = now_ms();
    let delegation_id = format!("dlg_{}_{}", created_ms, id_suffix(&task, &agent_id));
    let expires_ms = created_ms + PACKET_TTL_MS;
    let calibration = calibration_metrics_from_rows(&outcomes_rows(state));

    // Build the structured packet (house style: json! map, sufficiency-gated —
    // droppable fields, no typed struct).
    let mut packet = json!({
        "schema": "m1nd-delegation-packet-v0",
        "verdict": "packet",
        "delegation_id": delegation_id,
        "created_ms": created_ms,
        "expires_ms": expires_ms,
        "staleness": staleness,
        "coverage": {
            "anchor_files_refs_resolved": coverage_resolved,
            "unresolved": coverage_unresolved,
            "granularity": "file_level_static",
        },
        "mission": {
            "task": task,
            "agent_id": agent_id,
            "binding": binding,
            "tier": "project",
        },
        "scope": {
            "may_touch": may_touch,
            "derived_from": scope_derived_from,
        },
        "context": {
            "anchors": anchors,
            "sufficiency": sufficiency_json,
            "memory": memory,
        },
        "known_static_dependents": {
            "expected_change": expected_change,
            "dependents": dependents,
            "truncated": dependents_truncated,
        },
        "proof": {
            "suggested_m1nd_calls": suggested_m1nd_calls,
            "suggested_shell": suggested_shell,
        },
        "ignored_tail": {
            "count": focus_out.ignored.count,
            "scanned": focus_out.ignored.scanned,
            "reason": focus_out.ignored.reason.clone(),
        },
        "calibration": calibration_block(&calibration),
        "honest_gaps": honest_gaps,
        "non_claims": non_claims,
    });
    if let Some(hint) = &subagent_hint {
        packet["mission"]["subagent_hint"] = json!(hint);
    }
    if !resolved_seeds.is_empty() || !unresolved_seeds.is_empty() {
        packet["scope"]["seeds"] = json!({
            "resolved": resolved_seeds,
            "unresolved": unresolved_seeds,
        });
    }

    // 12. RENDER — the deterministic, string-stable prompt_markdown (the one reader
    //     is the subagent). Built from the structured packet so the golden is stable.
    let prompt_markdown = render_delegation_packet(&packet, budget_tokens);
    packet["prompt_markdown"] = json!(prompt_markdown);

    // 13. REGISTRY WRITE — the dumb record, the debrief join key. Status `live`.
    let mut record = packet.clone();
    record["status"] = json!("live");
    save_delegation(state, &record)?;

    Ok(packet)
}

// ---------------------------------------------------------------------------
// delegate helpers — each reuses a shipped signal.
// ---------------------------------------------------------------------------

/// A short, deterministic-per-input id suffix from the task + agent. Not a
/// crypto id — just enough to disambiguate two delegations in the same ms. We use
/// a simple FNV-style fold over the bytes (no new dependency, per the no-uuid
/// rule), rendered base36.
fn id_suffix(task: &str, agent_id: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in task.bytes().chain(agent_id.bytes()) {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    // base36, 6 chars.
    let mut n = h;
    let mut s = String::new();
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    for _ in 0..6 {
        s.push(ALPHABET[(n % 36) as usize] as char);
        n /= 36;
    }
    s
}

/// Compose the staleness header from the `am_i_stale` baseline (inventory hashes
/// vs disk). Best-effort — HEAD is null on unknown, never faked.
fn compose_staleness(state: &SessionState) -> Value {
    // Count ingested files whose on-disk hash no longer matches the recorded one.
    let mut files_changed = 0u64;
    for entry in state.file_inventory.values() {
        let path = std::path::Path::new(&entry.file_path);
        if !path.exists() {
            files_changed += 1;
            continue;
        }
        let current = crate::audit_handlers::simple_content_hash(path);
        match (&entry.sha256, current) {
            (Some(known), Some(now)) if known != &now => files_changed += 1,
            _ => {}
        }
    }
    // graph generation: the graph's node count is a cheap, honest generation-ish
    // baseline (the debrief re-check compares against it). Best-effort HEAD.
    let graph_generation = {
        let graph = state.graph.read();
        graph.nodes.count as u64
    };
    let workspace_head = best_effort_git_head(state);
    json!({
        "graph_generation": graph_generation,
        "workspace_head": workspace_head,
        "files_changed_since_ingest": files_changed,
    })
}

/// Best-effort git HEAD of the workspace: read `.git/HEAD` and, if it points at a
/// ref, read that ref. NEVER shells out; null on any miss (honest unknown).
fn best_effort_git_head(state: &SessionState) -> Value {
    let Some(root) = binding_workspace_root(state) else {
        return Value::Null;
    };
    let git_head = std::path::Path::new(&root).join(".git").join("HEAD");
    let Ok(head) = fs::read_to_string(&git_head) else {
        return Value::Null;
    };
    let head = head.trim();
    if let Some(rf) = head.strip_prefix("ref: ") {
        let ref_path = std::path::Path::new(&root).join(".git").join(rf);
        if let Ok(sha) = fs::read_to_string(&ref_path) {
            return json!(sha.trim());
        }
        return Value::Null;
    }
    // Detached HEAD: the file holds the sha directly.
    if head.len() >= 7 && head.chars().all(|c| c.is_ascii_hexdigit()) {
        return json!(head);
    }
    Value::Null
}

/// Does a seed string resolve to a node in the graph?
fn node_resolves(state: &SessionState, seed: &str) -> bool {
    let graph = state.graph.read();
    graph.resolve_id(seed).is_some()
}

/// The L1GHT memory slice — north's exact recall (scoped to the `light::` id
/// namespace so code nodes never compete), each row `{claim, age_days|null,
/// source_agent, stale, tier, origin_brain}`.
///
/// M7 (ORGANISM R7 · MEDULLA-PRD §6, §8.2): every row is LABELED cargo — `tier`
/// (project | medulla, from the routed store's identity) + `origin_brain` (the
/// claim's OWN `Origin-Brain` stamp, falling back to the store's identity when the
/// file predates the stamp — unknown is rendered honestly, never faked, MED-INV-4).
/// This is the delegate composer labeling ITS OWN rows; the medulla doctrine feed
/// is folded in — already tier/origin-labeled — by the routing layer
/// (`mcp_http::serve_and_compose`), the ONE seam that can read across stores (M5b).
fn recall_memory_slice(state: &mut SessionState, agent_id: &str, task: &str) -> Value {
    const LIGHT_RECALL_SCOPE: &str = "light::";
    let now_day_ms: u64 = 24 * 60 * 60 * 1000;
    let stale_after_ms: u64 = 30 * now_day_ms;
    // The routed brain's own tier + fallback origin — computed once, mirrors
    // `store_memory_rows` (mcp_http.rs). A project brain labels its rows
    // `tier: project`; the bound owner (the medulla) labels them `tier: medulla`.
    let this_tier = if state.is_medulla_store() {
        "medulla"
    } else {
        "project"
    };
    let store_origin = state.origin_brain();
    let hits = layer_handlers::handle_seek(
        state,
        layers::SeekInput {
            query: task.to_string(),
            agent_id: agent_id.to_string(),
            top_k: 24,
            scope: Some(LIGHT_RECALL_SCOPE.to_string()),
            node_types: Vec::new(),
            min_score: 0.1,
            graph_rerank: true,
            conformance_aware: true,
            token_budget: None,
        },
    )
    .map(|o| o.results)
    .unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    let rows: Vec<Value> = hits
        .into_iter()
        .filter(|r| r.source_agent.is_some() || r.authored_ms_ago.is_some())
        .filter(|r| seen.insert(r.node_id.clone()))
        .take(5)
        .map(|r| {
            let age_days = r.authored_ms_ago.map(|ms| ms / now_day_ms);
            let stale = r.authored_ms_ago.map(|ms| ms > stale_after_ms);
            let mut obj = serde_json::Map::new();
            obj.insert("claim".into(), json!(r.label));
            obj.insert(
                "age_days".into(),
                age_days.map(|d| json!(d)).unwrap_or(Value::Null),
            );
            obj.insert(
                "source_agent".into(),
                r.source_agent
                    .clone()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
            if let Some(stale) = stale {
                obj.insert("stale".into(), json!(stale));
            }
            // M7 labels: prefer the claim's OWN Origin-Brain stamp; fall back to the
            // store's identity when the file predates the stamp (unknown → the
            // store's own origin, never a faked or absent label).
            let origin = r
                .origin_brain
                .clone()
                .unwrap_or_else(|| store_origin.clone());
            obj.insert("origin_brain".into(), json!(origin));
            obj.insert("tier".into(), json!(this_tier));
            // `node_id` is the fold identity the routing layer de-dupes on when it
            // composes the medulla feed into this slice (append_memory_rows).
            obj.insert("node_id".into(), json!(r.node_id));
            Value::Object(obj)
        })
        .collect();
    Value::Array(rows)
}

/// Anchor coverage: how many anchor/focus file paths m1nd resolved vs not.
/// Best-effort, file-level. Returns (resolved, unresolved) counts.
fn anchor_coverage(state: &SessionState, anchors: &Value, focus: &Value) -> (u64, u64) {
    let mut resolved = 0u64;
    let mut unresolved = 0u64;
    for path in collect_paths(anchors)
        .into_iter()
        .chain(collect_paths(focus))
    {
        // A path is "resolved" if it lives in the file inventory (m1nd saw it).
        let hit = state
            .file_inventory
            .values()
            .any(|e| e.file_path.ends_with(&path) || path.ends_with(&e.file_path));
        if hit {
            resolved += 1;
        } else {
            unresolved += 1;
        }
    }
    (resolved, unresolved)
}

/// Collect `path` strings from an anchors/focus_nodes array.
fn collect_paths(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.get("path").and_then(|p| p.as_str()))
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The `may_touch` derivation from activation: the distinct file paths of the
/// anchors + focus nodes.
fn anchor_paths(anchors: &Value, focus: &Value) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    collect_paths(anchors)
        .into_iter()
        .chain(collect_paths(focus))
        .filter(|p| seen.insert(p.clone()))
        .collect()
}

/// Known static dependents via `impact` on the top focus node. Best-effort:
/// empty + truncated:false when nothing resolves. Returns (dependents,
/// expected_change, truncated).
fn static_dependents(
    state: &mut SessionState,
    agent_id: &str,
    focus: &Value,
    max_nodes: usize,
) -> (Vec<Value>, Vec<Value>, bool) {
    let top_node = focus
        .as_array()
        .and_then(|a| a.first())
        .and_then(|n| n.get("node_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let Some(node_id) = top_node else {
        return (Vec::new(), Vec::new(), false);
    };
    // expected_change: the top focus node is the thing most likely to change.
    let expected_change = vec![json!({ "node_id": node_id })];
    let out = tools::handle_impact(
        state,
        crate::protocol::ImpactInput {
            node_id: node_id.clone(),
            agent_id: agent_id.to_string(),
            direction: "reverse".into(),
            include_causal_chains: false,
            max_nodes: Some(max_nodes),
        },
    );
    match out {
        Ok(impact) => {
            let dependents: Vec<Value> = impact
                .blast_radius
                .iter()
                .filter(|e| e.is_knowledge_citation != Some(true))
                .map(|e| {
                    json!({
                        "node_id": e.node_id,
                        "label": e.label,
                        "hop_distance": e.hop_distance,
                    })
                })
                .collect();
            (dependents, expected_change, impact.truncated)
        }
        // impact failed to resolve the node — honest empty, not a fabricated set.
        Err(_) => (Vec::new(), expected_change, false),
    }
}

/// Is this a Rust repo? Cheap: any ingested file ends in `.rs`, or a Cargo.toml
/// exists under the workspace root.
fn repo_is_rust(state: &SessionState) -> bool {
    if state
        .file_inventory
        .values()
        .any(|e| e.file_path.ends_with(".rs"))
    {
        return true;
    }
    if let Some(root) = binding_workspace_root(state) {
        return std::path::Path::new(&root).join("Cargo.toml").exists();
    }
    false
}

// ---------------------------------------------------------------------------
// The deterministic renderer (string-stable — golden tests). §O.12.4 rendering
// contract: the packet is an APPENDIX; opens with a precedence block; one-line
// mission echo; NO code bodies, NO tables — `- path:line  label — claim`
// line-items only; every degradation renders duty-coupled; the FINAL "what m1nd
// could NOT determine → your duties" section is NEVER dropped and closes with the
// report protocol.
// ---------------------------------------------------------------------------

/// Recover the source file path from a graph node id of the form
/// `file::<path>::kind::name` (or `file::<path>`). Returns `None` for a non-file
/// node id, so the renderer can fall back to `<no-path>` honestly.
fn path_from_node_id(node_id: &str) -> Option<String> {
    let rest = node_id.strip_prefix("file::")?;
    let path = rest.split("::").next().unwrap_or(rest);
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}

/// Render the packet's `prompt_markdown` deterministically from the structured
/// packet. String-stable: same packet in → byte-identical string out (the golden
/// property). `budget_tokens` is a CAP: sufficiency-gating drops the memory tail
/// and dependents tail under the cap; the honesty section is never dropped.
pub fn render_delegation_packet(packet: &Value, budget_tokens: u64) -> String {
    let mut out = String::new();
    let get_str = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();

    let delegation_id = get_str(packet, "delegation_id");
    let task = packet
        .get("mission")
        .map(|m| get_str(m, "task"))
        .unwrap_or_default();

    // --- appendix precedence block ---
    out.push_str("---\n");
    out.push_str(&format!(
        "## m1nd delegation packet — {delegation_id} (project-tier, APPENDIX)\n\n"
    ));
    out.push_str("Precedence: the orchestrator's brief above wins on WHAT TO DO. The file wins on WHAT IS. This packet outranks assumption ONLY — the map is not a fence.\n\n");

    // --- one-line mission echo (never a restatement) ---
    out.push_str(&format!("Mission (echo): {task}\n\n"));

    // --- binding (the child law) ---
    if let Some(binding) = packet.get("mission").and_then(|m| m.get("binding")) {
        let root = binding
            .get("workspace_root")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        let mode = binding
            .get("trust_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("orientation_only");
        out.push_str(&format!(
            "Binding: land on the brain named `{root}` (trust_mode: {mode}). Verify via reception — a silent bind means you matched; a reception block means you did NOT. Do not choose; confirm.\n\n"
        ));
    }

    // --- staleness (duty-coupled) ---
    if let Some(st) = packet.get("staleness") {
        let fc = st
            .get("files_changed_since_ingest")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if fc > 0 {
            out.push_str(&format!(
                "Staleness: {fc} ingested file(s) changed on disk since ingest → therefore you MUST re-read any file below before trusting its map line.\n\n"
            ));
        } else {
            out.push_str("Staleness: no ingested file changed since ingest (map is current as far as m1nd can see).\n\n");
        }
    }

    // --- scope ---
    if let Some(scope) = packet.get("scope") {
        let derived = scope
            .get("derived_from")
            .and_then(|v| v.as_str())
            .map(|s| format!(" (derived from {s} — not a declared boundary)"))
            .unwrap_or_default();
        out.push_str(&format!("### May touch{derived}\n"));
        if let Some(paths) = scope.get("may_touch").and_then(|v| v.as_array()) {
            if paths.is_empty() {
                out.push_str("- (none resolved — treat the whole activation as advisory)\n");
            }
            for p in paths.iter().filter_map(|p| p.as_str()) {
                out.push_str(&format!("- {p}\n"));
            }
        }
        out.push('\n');
    }

    // --- anchors (line-items, no code bodies) ---
    out.push_str("### Anchors (ranked — the map, not a fence)\n");
    if let Some(anchors) = packet
        .get("context")
        .and_then(|c| c.get("anchors"))
        .and_then(|a| a.as_array())
    {
        if anchors.is_empty() {
            out.push_str("- (no anchors activated)\n");
        }
        for a in anchors {
            // Prefer an explicit path; else recover it from the node id
            // (`file::<path>::kind::name`) so the line-item names a real file the
            // subagent can read — an anchor with no locus is useless.
            let path_owned = a
                .get("path")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    a.get("node_id")
                        .and_then(|v| v.as_str())
                        .and_then(path_from_node_id)
                })
                .unwrap_or_else(|| "<no-path>".to_string());
            let line = a
                .get("line_start")
                .and_then(|v| v.as_u64())
                .or_else(|| a.get("line").and_then(|v| v.as_u64()));
            let label = a.get("label").and_then(|v| v.as_str()).unwrap_or("");
            let loc = match line {
                Some(l) => format!("{path_owned}:{l}"),
                None => path_owned,
            };
            out.push_str(&format!("- {loc}  {label}\n"));
        }
    }
    out.push('\n');

    // --- memory slice (sufficiency-gated under the cap) ---
    // M7: each row is LABELED cargo — `tier` (doctrine vs project fact) + the brain
    // it was born in — so the child inherits WHICH tier/brain a claim came from, not
    // just the claim. Absent provenance renders "unknown", never faked (MED-INV-4).
    out.push_str("### Prior memory (tier · origin · author · age, or honest absence)\n");
    if let Some(mem) = packet
        .get("context")
        .and_then(|c| c.get("memory"))
        .and_then(|m| m.as_array())
    {
        if mem.is_empty() {
            out.push_str("- (no prior memory surfaced for this task)\n");
        }
        // Cap the memory rows rendered when the budget is tight (deterministic:
        // small budget → fewer rows, always the same rows for the same packet).
        let cap = if budget_tokens < 1000 { 2 } else { 5 };
        for m in mem.iter().take(cap) {
            let claim = m.get("claim").and_then(|v| v.as_str()).unwrap_or("");
            let tier = m.get("tier").and_then(|v| v.as_str()).unwrap_or("unknown");
            let origin = m
                .get("origin_brain")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let author = m
                .get("source_agent")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown-author");
            // Age is rendered from `age_days` (delegate rows) or `age_ms` (folded
            // medulla rows carry ms); absent → honestly "age unknown".
            let age = m
                .get("age_days")
                .and_then(|v| v.as_u64())
                .map(|d| format!("{d}d old"))
                .or_else(|| {
                    m.get("age_ms")
                        .and_then(|v| v.as_u64())
                        .map(|ms| format!("{}d old", ms / (24 * 60 * 60 * 1000)))
                })
                .unwrap_or_else(|| "age unknown".to_string());
            let stale = if m.get("stale").and_then(|v| v.as_bool()).unwrap_or(false) {
                " [STALE — verify against the file]"
            } else {
                ""
            };
            out.push_str(&format!(
                "- [{tier}] {claim} — {origin} · {author}, {age}{stale}\n"
            ));
        }
    }
    out.push('\n');

    // --- known static dependents (duty-coupled) ---
    out.push_str("### Known static dependents (file-level static — regex, NOT complete)\n");
    if let Some(dep) = packet
        .get("known_static_dependents")
        .and_then(|d| d.get("dependents"))
        .and_then(|d| d.as_array())
    {
        if dep.is_empty() {
            out.push_str("- (none resolved) → coupling is UNKNOWN, not absent: grep for callers of anything you change.\n");
        }
        let cap = if budget_tokens < 1000 { 5 } else { 20 };
        for d in dep.iter().take(cap) {
            let label = d.get("label").and_then(|v| v.as_str()).unwrap_or("");
            let node = d.get("node_id").and_then(|v| v.as_str()).unwrap_or("");
            out.push_str(&format!("- {label}  ({node})\n"));
        }
    }
    out.push('\n');

    // --- proof ---
    out.push_str("### Suggested proof\n");
    if let Some(shell) = packet
        .get("proof")
        .and_then(|p| p.get("suggested_shell"))
        .and_then(|s| s.as_array())
    {
        for c in shell.iter().filter_map(|c| c.as_str()) {
            out.push_str(&format!("- shell: `{c}`\n"));
        }
    }
    out.push('\n');

    // --- THE FINAL SECTION — never dropped, duty-coupled, closes with the report
    //     protocol. This is the honesty invariant §O.12.9 #4.
    out.push_str("### What m1nd could NOT determine → your duties\n");
    if let Some(gaps) = packet.get("honest_gaps").and_then(|g| g.as_array()) {
        for g in gaps.iter().filter_map(|g| g.as_str()) {
            out.push_str(&format!("- {g}\n"));
        }
    }
    out.push('\n');
    out.push_str(
        "Report protocol (REQUIRED — you hold no m1nd tool, this IS your participation):\n",
    );
    out.push_str(&format!(
        "Open your final message with `[m1nd {delegation_id}]` and carry two lines:\n"
    ));
    out.push_str("- DEVIATIONS: every path you touched OUTSIDE may_touch, or \"none\".\n");
    out.push_str("- FINDINGS: up to 3 durable discoveries worth memorizing, or \"none\".\n");

    out
}

// ---------------------------------------------------------------------------
// Slice 2 — `debrief`: the only mutation, via existing verbs.
// ---------------------------------------------------------------------------

/// `debrief` — grade the spawned agent's real diff against the packet and teach
/// the graph. The ONLY mutation in the delegation layer, and it mutates only
/// through existing verbs (`memorize` / `learn`). §O.12.6.
pub fn handle_debrief(state: &mut SessionState, params: &Value) -> M1ndResult<Value> {
    let agent_id = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: "debrief".into(),
            detail: "debrief requires an `agent_id` string (the grader)".into(),
        })?
        .to_string();
    let delegation_id = params
        .get("delegation_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: "debrief".into(),
            detail: "debrief requires a `delegation_id` string".into(),
        })?
        .to_string();
    let outcome = params
        .get("outcome")
        .and_then(|v| v.as_str())
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: "debrief".into(),
            detail: "debrief requires an `outcome`: success | failure | partial".into(),
        })?
        .to_string();
    // Outcome enum is EXACTLY success|failure|partial (§O.12.2 kill list — no
    // `unreviewed`).
    if !matches!(outcome.as_str(), "success" | "failure" | "partial") {
        return Err(M1ndError::InvalidParams {
            tool: "debrief".into(),
            detail: "outcome must be one of: success | failure | partial".into(),
        });
    }
    let subagent_id = params
        .get("subagent_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| agent_id.clone());
    let evidence = params.get("evidence").cloned().filter(|v| !v.is_null());
    let findings: Vec<Value> = params
        .get("findings")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    // (1) LOAD — unknown id is a hard error, no guessing.
    let record = load_delegation(state, &delegation_id)?;

    // (2) STALENESS RE-CHECK — did the graph move under the packet? Compare the
    //     packet's recorded graph_generation against the current one.
    let packet_generation = record
        .get("staleness")
        .and_then(|s| s.get("graph_generation"))
        .and_then(|v| v.as_u64());
    let current_generation = {
        let graph = state.graph.read();
        graph.nodes.count as u64
    };
    let graph_drifted = matches!(packet_generation, Some(g) if g != current_generation);

    // (3) TOUCHED SET — from `diff` (+++ b/ headers) or `touched_paths`.
    let touched = resolve_touched_set(params);
    // Cross-check against live sibling records (same-worktree contamination).
    let siblings = live_sibling_records(state, &delegation_id);
    let overlapping = touched_overlaps_siblings(&touched, &siblings);

    // (4) RE-INGEST — SKIPPED in this slice (honest caveat). The edit_commit
    //     incremental re-ingest seam is not reached here; conformance grades
    //     against the CURRENT graph state, not a freshly re-ingested one.
    //     (§O.12.10 slice-2 note: acceptable to skip the physical re-ingest.)
    let reingest_skipped = true;

    // (5) CONFORMANCE ALGEBRA per path. In slice 1/2 there is NO ratified
    //     must_not_touch fence, so `breach` cannot fire — the verdict string always
    //     carries fence existence.
    let may_touch: Vec<String> = record
        .get("scope")
        .and_then(|s| s.get("may_touch"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|p| p.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    let expected_change: Vec<String> = record
        .get("known_static_dependents")
        .and_then(|d| d.get("expected_change"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|e| e.get("node_id").and_then(|n| n.as_str()))
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    let dependents: Vec<String> = record
        .get("known_static_dependents")
        .and_then(|d| d.get("dependents"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|e| e.get("node_id").and_then(|n| n.as_str()))
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();

    let mut classified: Vec<Value> = Vec::new();
    let mut counts = ConformanceCounts::default();
    for path in &touched {
        let class = classify_path(path, &may_touch, &expected_change, &dependents);
        match class {
            "in_scope" => counts.in_scope += 1,
            "expected_change" => counts.expected_change += 1,
            "dependent_contact" => counts.dependent_contact += 1,
            "unpredicted" => counts.unpredicted += 1,
            _ => {}
        }
        classified.push(json!({ "path": path, "class": class }));
    }

    // worst-of: breached > unpredicted > stayed. No ratified fence exists → breach
    // is unreachable; the verdict string SAYS so (fence existence, §O.12.6 fix 4).
    let verdict = if counts.unpredicted > 0 {
        "unpredicted — the subagent touched paths m1nd did not predict (map feedback, not sin)"
    } else {
        "stayed — no ratified boundaries existed (breach is unreachable without a ratified must_not_touch fence)"
    };

    // (6) MEMORIZE — findings[] under the SUBAGENT's id; breach/unpredicted lessons
    //     under the GRADER's id. Clean runs memorize NOTHING (no filler).
    let mut memorized: Vec<Value> = Vec::new();
    for (i, finding) in findings.iter().enumerate() {
        let text = finding
            .as_str()
            .map(|s| s.to_string())
            .or_else(|| {
                finding
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            })
            .unwrap_or_default();
        if text.trim().is_empty() {
            continue;
        }
        // The node label is a MEANINGFUL summary of the finding text — not an
        // opaque id — so the next delegate's content recall can surface it (the
        // flywheel only closes if a finding is findable by what it says). The
        // delegation id rides the title for traceability, and `i` disambiguates
        // multiple findings from one debrief.
        let label = finding_node_label(&text, i);
        let out = handle_light_author(
            state,
            LightAuthorInput {
                agent_id: subagent_id.clone(),
                node_label: label.clone(),
                title: Some(format!("delegation finding ({delegation_id})")),
                state: Some("authored".into()),
                claims: vec![LightClaim {
                    label: label.clone(),
                    text: Some(text.clone()),
                    kind: Some("event".into()),
                    confidence: None,
                    ambiguity: None,
                    evidence: Vec::new(),
                    depends_on: Vec::new(),
                }],
                output_path: None,
                namespace: None,
                ingest_after: true,
                mode: "merge".into(),
                supersedes: None,
                origin_brain: None,
                origin_claim: None,
                promoted_by: None,
                promotion_reason: None,
                promoted_to: None,
                evidence_unverifiable: false,
                soul_source: None,
            },
        )?;
        memorized.push(json!({
            "under_agent": subagent_id,
            "path": out.get("path").cloned().unwrap_or(Value::Null),
            "kind": "finding",
        }));
    }
    // Unpredicted lessons: one memory under the GRADER's id (only when there was an
    // unpredicted touch — clean runs stay silent).
    if counts.unpredicted > 0 {
        let unpredicted_paths: Vec<&str> = classified
            .iter()
            .filter(|c| c.get("class").and_then(|v| v.as_str()) == Some("unpredicted"))
            .filter_map(|c| c.get("path").and_then(|v| v.as_str()))
            .collect();
        let text = format!(
            "delegation {delegation_id}: the subagent touched {} unpredicted path(s) the packet's static map missed: {}. The map was one anchor too tight here.",
            counts.unpredicted,
            unpredicted_paths.join(", ")
        );
        let label = format!("delegation-lesson-{delegation_id}");
        let out = handle_light_author(
            state,
            LightAuthorInput {
                agent_id: agent_id.clone(),
                node_label: label.clone(),
                title: Some("delegation map-miss lesson".into()),
                state: Some("authored".into()),
                claims: vec![LightClaim {
                    label: label.clone(),
                    text: Some(text),
                    kind: Some("event".into()),
                    confidence: None,
                    ambiguity: None,
                    evidence: unpredicted_paths.iter().map(|s| s.to_string()).collect(),
                    depends_on: Vec::new(),
                }],
                output_path: None,
                namespace: None,
                ingest_after: true,
                mode: "merge".into(),
                supersedes: None,
                origin_brain: None,
                origin_claim: None,
                promoted_by: None,
                promotion_reason: None,
                promoted_to: None,
                evidence_unverifiable: false,
                soul_source: None,
            },
        )?;
        memorized.push(json!({
            "under_agent": agent_id,
            "path": out.get("path").cloned().unwrap_or(Value::Null),
            "kind": "unpredicted_lesson",
        }));
    }

    // (7) TEACH asymmetrically via learn. unpredicted → learn(partial); a
    //     dependent that WAS contacted → learn(correct). Untouched dependents are
    //     NEVER punished (§O.12.6 step 7).
    let mut taught: Vec<Value> = Vec::new();
    let seed_node = expected_change.first().cloned();
    for c in &classified {
        let class = c.get("class").and_then(|v| v.as_str()).unwrap_or("");
        let path = c.get("path").and_then(|v| v.as_str()).unwrap_or("");
        match class {
            "unpredicted" => {
                // Plant the co-change signal predict consumes next round: nearest
                // seed ↔ touched, partial credit.
                let mut node_ids = Vec::new();
                if let Some(seed) = &seed_node {
                    node_ids.push(seed.clone());
                }
                let out = tools::handle_learn(
                    state,
                    crate::protocol::LearnInput {
                        query: path.to_string(),
                        agent_id: agent_id.clone(),
                        feedback: "partial".into(),
                        node_ids,
                        strength: crate::protocol::default_feedback_strength(),
                    },
                );
                taught.push(json!({
                    "path": path, "feedback": "partial",
                    "ok": out.is_ok(),
                }));
            }
            "dependent_contact" => {
                let node_ids = dependent_node_for_path(&record, path);
                let out = tools::handle_learn(
                    state,
                    crate::protocol::LearnInput {
                        query: path.to_string(),
                        agent_id: agent_id.clone(),
                        feedback: "correct".into(),
                        node_ids,
                        strength: crate::protocol::default_feedback_strength(),
                    },
                );
                taught.push(json!({
                    "path": path, "feedback": "correct",
                    "ok": out.is_ok(),
                }));
            }
            _ => {}
        }
    }

    // (8) FLIP the record to `debriefed` + append ONE outcomes.jsonl row. Stamp
    //     outcome_unverified UNLESS evidence came attached.
    let outcome_unverified = evidence.is_none();
    let mut updated = record.clone();
    updated["status"] = json!("debriefed");
    save_delegation(state, &updated)?;

    let sufficiency_at_delegate = record
        .get("context")
        .and_then(|c| c.get("sufficiency"))
        .cloned()
        .unwrap_or(Value::Null);
    let ledger_row = json!({
        "schema": "m1nd-delegation-outcome-v0",
        "ts": now_ms(),
        "delegation_id": delegation_id,
        "grader": agent_id,
        "subagent": subagent_id,
        "sufficiency_at_delegate": sufficiency_at_delegate,
        "packet_anchor_count": record
            .get("context").and_then(|c| c.get("anchors"))
            .and_then(|a| a.as_array()).map(|a| a.len()).unwrap_or(0),
        "packet_dependent_count": dependents.len(),
        "conformance_verdict": verdict,
        "touched_count": touched.len(),
        "in_scope": counts.in_scope,
        "expected_change": counts.expected_change,
        "dependent_contact": counts.dependent_contact,
        "unpredicted": counts.unpredicted,
        "outcome": outcome,
        "outcome_unverified": outcome_unverified,
        "graph_drifted": graph_drifted,
    });
    append_outcome_row(state, &ledger_row)?;

    // OUTPUT — two gradings separated. conformance grades subagent-vs-map; the
    // map-grade grades m1nd-vs-reality.
    let mut caveats: Vec<String> = Vec::new();
    if graph_drifted {
        caveats.push(
            "graph_drifted: the graph moved under this packet — the conformance grade is against the current graph, which differs from delegate-time.".into(),
        );
    }
    if reingest_skipped {
        caveats.push(
            "re-ingest skipped: touched files were NOT physically re-ingested in this slice; the grade is against the graph as it stood, not a re-parsed one.".into(),
        );
    }
    if overlapping {
        caveats.push(
            "sibling overlap: a live sibling delegation touches overlapping paths — conformance cannot fully attribute same-worktree edits.".into(),
        );
    }
    if outcome_unverified {
        caveats.push(
            "outcome_unverified: no evidence (proof command + exit status) was attached — the self-reported outcome is unverified and bounded in calibration.".into(),
        );
    }

    Ok(json!({
        "schema": "m1nd-debrief-v0",
        "delegation_id": delegation_id,
        "conformance": {
            "grades": "the subagent against the packet's map",
            "verdict": verdict,
            "classified": classified,
            "counts": {
                "in_scope": counts.in_scope,
                "expected_change": counts.expected_change,
                "dependent_contact": counts.dependent_contact,
                "unpredicted": counts.unpredicted,
            },
        },
        "map_grade": {
            "grades": "m1nd against reality",
            "unpredicted_is_map_feedback": counts.unpredicted,
            "note": "unpredicted touches are m1nd's map being one anchor too tight — not the subagent's sin.",
        },
        "outcome": outcome,
        "outcome_unverified": outcome_unverified,
        "learned": {
            "memorized": memorized,
            "taught": taught,
        },
        "caveats": caveats,
        "non_claims": [
            "conformance grades paths, never code quality — it NEVER says merge-safe.",
            "breach is unreachable without a ratified must_not_touch fence — this slice has none.",
            "untouched dependents are never punished — a guard that wasn't contacted was not wrong.",
            "a self-reported outcome without evidence is stamped outcome_unverified.",
        ],
    }))
}

#[derive(Default)]
struct ConformanceCounts {
    in_scope: u64,
    expected_change: u64,
    dependent_contact: u64,
    unpredicted: u64,
}

/// A meaningful, content-derived node label for a debrief finding: the first few
/// words of the finding text, so the next delegate's recall can surface it by
/// what it says (an opaque `dlg_…` label is unfindable). `idx` disambiguates
/// multiple findings from one debrief.
fn finding_node_label(text: &str, idx: usize) -> String {
    let summary: String = text
        .split_whitespace()
        .take(8)
        .collect::<Vec<_>>()
        .join(" ");
    let summary = summary.trim();
    if summary.is_empty() {
        format!("delegation finding {idx}")
    } else if idx == 0 {
        summary.to_string()
    } else {
        format!("{summary} ({idx})")
    }
}

/// Classify one touched path against the packet's scope sets. `new_territory` and
/// `breach` are not reachable in slice 1/2 (no ratified fence), so the reachable
/// classes are in_scope | expected_change | dependent_contact | unpredicted.
fn classify_path(
    path: &str,
    may_touch: &[String],
    expected_change: &[String],
    dependents: &[String],
) -> &'static str {
    let matches_any = |set: &[String]| {
        set.iter()
            .any(|s| s == path || s.ends_with(path) || path.ends_with(s.as_str()))
    };
    if matches_any(expected_change) {
        "expected_change"
    } else if matches_any(may_touch) {
        "in_scope"
    } else if matches_any(dependents) {
        "dependent_contact"
    } else {
        "unpredicted"
    }
}

/// The dependent node id(s) whose id matches `path`, for the `learn(correct)` teach.
fn dependent_node_for_path(record: &Value, path: &str) -> Vec<String> {
    record
        .get("known_static_dependents")
        .and_then(|d| d.get("dependents"))
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|e| e.get("node_id").and_then(|n| n.as_str()))
                .filter(|n| *n == path || n.ends_with(path) || path.ends_with(n))
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Resolve the touched set from `diff` (parse `+++ b/` headers) or `touched_paths`.
fn resolve_touched_set(params: &Value) -> Vec<String> {
    if let Some(paths) = params.get("touched_paths").and_then(|v| v.as_array()) {
        return paths
            .iter()
            .filter_map(|p| p.as_str())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    if let Some(diff) = params.get("diff").and_then(|v| v.as_str()) {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for line in diff.lines() {
            if let Some(rest) = line.strip_prefix("+++ b/") {
                let p = rest.trim().to_string();
                if !p.is_empty() && seen.insert(p.clone()) {
                    out.push(p);
                }
            }
        }
        return out;
    }
    Vec::new()
}

/// Does any touched path overlap a live sibling's may_touch set?
fn touched_overlaps_siblings(touched: &[String], siblings: &[Value]) -> bool {
    for sib in siblings {
        let sib_paths: Vec<&str> = sib
            .get("scope")
            .and_then(|s| s.get("may_touch"))
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|p| p.as_str()).collect())
            .unwrap_or_default();
        for t in touched {
            if sib_paths
                .iter()
                .any(|s| *s == t || s.ends_with(t.as_str()) || t.ends_with(*s))
            {
                return true;
            }
        }
    }
    false
}

/// Append one JSONL row to `outcomes.jsonl` (create + append).
fn append_outcome_row(state: &SessionState, row: &Value) -> M1ndResult<()> {
    let dir = delegation_dir(state);
    fs::create_dir_all(&dir).map_err(M1ndError::Io)?;
    let path = outcomes_path(state);
    let line = format!(
        "{}\n",
        serde_json::to_string(row).map_err(M1ndError::Serde)?
    );
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(M1ndError::Io)?;
    file.write_all(line.as_bytes()).map_err(M1ndError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one synthetic outcomes.jsonl row with the fields the calibration
    /// reducer consumes.
    fn row(
        touched: u64,
        in_scope: u64,
        expected_change: u64,
        unpredicted: u64,
        dependent_contact: u64,
        outcome: &str,
    ) -> Value {
        json!({
            "schema": "m1nd-delegation-outcome-v0",
            "touched_count": touched,
            "in_scope": in_scope,
            "expected_change": expected_change,
            "unpredicted": unpredicted,
            "dependent_contact": dependent_contact,
            "outcome": outcome,
        })
    }

    /// Below the N ≥ 30 floor the reducer reports `calibrated:false` and withholds
    /// EVERY quality number — bands and counts only (§O.12.8). This is the pre-fix
    /// world's honest header, now derived from the ledger instead of hardcoded.
    #[test]
    fn calibration_is_uncalibrated_and_number_free_below_thirty_rows() {
        let rows: Vec<Value> = (0..29)
            .map(|_| row(4, 1, 1, 2, 1, "failure"))
            .collect();
        let m = calibration_metrics_from_rows(&rows);

        assert_eq!(m.rows, 29);
        assert!(!m.calibrated, "29 rows is below the 30-row calibration floor");
        assert_eq!(m.scope_precision, None, "no quality number prints while uncalibrated");
        assert_eq!(m.miss_rate, None, "no quality number prints while uncalibrated");
        assert_eq!(m.dependents_honesty, None, "no quality number prints while uncalibrated");

        // And the rendered block carries only the honest header — no metric keys.
        let block = calibration_block(&m);
        assert_eq!(block["calibrated"], json!(false));
        assert_eq!(block["rows"], json!(29));
        assert!(block.get("scope_precision").is_none());
        assert!(block.get("miss_rate").is_none());
        assert!(block.get("dependents_honesty").is_none());
    }

    /// At N ≥ 30 the reducer flips `calibrated:true` and the three metrics match
    /// their §O.12.8 definitions computed by hand over the synthetic ledger.
    #[test]
    fn calibration_metrics_match_hand_computed_values_at_thirty_rows() {
        // 10 "contact" rows (dependent_contact=1), 6 of them failures.
        // 20 "stayed" rows (dependent_contact=0), 2 of them failures.
        // Every row: touched=4, in_scope=1, expected_change=1, unpredicted=2.
        let mut rows: Vec<Value> = Vec::new();
        for i in 0..10 {
            let outcome = if i < 6 { "failure" } else { "success" };
            rows.push(row(4, 1, 1, 2, 1, outcome));
        }
        for i in 0..20 {
            let outcome = if i < 2 { "failure" } else { "success" };
            rows.push(row(4, 1, 1, 2, 0, outcome));
        }
        assert_eq!(rows.len(), 30);

        let m = calibration_metrics_from_rows(&rows);

        assert_eq!(m.rows, 30);
        assert!(m.calibrated, "30 rows meets the calibration floor");

        // scope_precision = Σ(in_scope+expected_change) / Σ(touched)
        //                 = (30*2) / (30*4) = 60/120 = 0.5
        assert_eq!(m.scope_precision, Some(0.5));
        // miss_rate = Σ(unpredicted) / Σ(touched) = (30*2)/(30*4) = 0.5
        assert_eq!(m.miss_rate, Some(0.5));
        // dependents_honesty = P(fail|contact) - P(fail|stayed)
        //                    = 6/10 - 2/20 = 0.6 - 0.1 = 0.5
        assert_eq!(m.dependents_honesty, Some(0.5));

        // The rendered block now surfaces the certified numbers.
        let block = calibration_block(&m);
        assert_eq!(block["calibrated"], json!(true));
        assert_eq!(block["rows"], json!(30));
        assert_eq!(block["scope_precision"], json!(0.5));
        assert_eq!(block["miss_rate"], json!(0.5));
        assert_eq!(block["dependents_honesty"], json!(0.5));
    }

    /// Malformed / torn ledger lines are skipped, and zero-denominator guards keep
    /// the metrics `None` even at/above the row floor when nothing was touched.
    #[test]
    fn calibration_guards_zero_denominators() {
        // 30 rows that touched nothing (touched_count=0) — calibrated, but the
        // scope/miss denominators are zero so no ratio is fabricated.
        let rows: Vec<Value> = (0..30).map(|_| row(0, 0, 0, 0, 0, "success")).collect();
        let m = calibration_metrics_from_rows(&rows);
        assert!(m.calibrated);
        assert_eq!(m.scope_precision, None, "no divide-by-zero ratio");
        assert_eq!(m.miss_rate, None, "no divide-by-zero ratio");
        // All rows "stayed" (no contact) → the contact partition is empty → None.
        assert_eq!(m.dependents_honesty, None, "honesty needs both partitions");
    }
}
