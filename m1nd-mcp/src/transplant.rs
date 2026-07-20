//! m1nd-mcp/src/transplant.rs — the `transplant` verb (graph-addressed cross-file
//! move of a top-level `fn`).
//!
//! PROOF-OF-POSSIBILITY SPIKE (donors study §7.1). The verb:
//!   1. Resolves the symbol via the code graph (label + `Function` kind +
//!      provenance path), cross-checked against disk truth.
//!   2. Computes the dependency trichotomy from `calls` edges (private dep
//!      TRAVELS, shared dep STAYS + visibility bump + back-import).
//!   3. Widens the item region TEXTUALLY (the graph provenance is
//!      declaration-line-only — a reported gap), carrying doc comments,
//!      attributes and leading `//` trivia.
//!   4. Discovers referencing files via graph `calls`/`imports` edges, with a
//!      textual scan of the graph's file inventory as a cross-check/fallback.
//!   5. Writes source + dest + referencers ATOMICALLY through the existing
//!      `apply_batch` machinery (reuse-first: no bespoke atomic writer, no
//!      bespoke re-ingest). Any preflight refusal writes NOTHING.
//!
//! Scope is deliberately narrow (top-level `fn` items in Rust); everything the
//! graph could not answer confidently is surfaced honestly in [`TransplantOutput`]
//! (`refs_unresolved`, `dependency_source`, `referencer_source`) rather than
//! silently skipped — the spike's job is to measure what is POSSIBLE and feed the
//! PRD, never to fake a clean move.

use crate::protocol::surgical::{
    ApplyBatchInput, BatchEditItem, ProtectedZoneGesture, SharedDepReport, StateLeftBehind,
    TransplantCommitInput, TransplantCommitOutput, TransplantInput, TransplantOutput,
    TransplantPlannedFileReport, TransplantPreviewOutput,
};
use crate::session::{PlannedTransplantWrite, SessionState, TransplantPreviewState};
use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_core::types::{NodeId, NodeType};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Instant;

/// A `fn` node lifted out of the graph in a single read pass (so the graph lock
/// is held briefly and the planning logic works over plain data).
struct FnNode {
    idx: usize,
    name: String,
    /// Provenance source path exactly as the graph stores it (repo-relative).
    file: String,
    /// 1-based declaration line from provenance (`line_start`). NB: the graph
    /// records only the DECLARATION line, never the item's closing brace — the
    /// full span is recovered textually (see [`item_region`]).
    decl_line: u32,
}

/// The graph facts the planner needs, extracted under one short read lock.
struct GraphView {
    fns: Vec<FnNode>,
    /// Distinct file-node paths (repo-relative), for the textual referencer scan.
    file_paths: Vec<String>,
    /// Provenance source path for EVERY node index (empty when absent) — lets an
    /// edge's source node be mapped back to its file regardless of node kind.
    node_file: Vec<String>,
    /// Every edge as `(src_idx, tgt_idx, relation)` — the fixture graph is tiny;
    /// a production verb would query targeted adjacency instead (reported).
    edges: Vec<(usize, usize, String)>,
}

/// Build an `InvalidParams` refusal carrying `detail` (its Debug form is what the
/// battery greps, so refusal reasons must be self-describing, e.g. "collision").
fn refuse(detail: String) -> M1ndError {
    refuse_for("transplant", detail)
}

/// [`refuse`] with an explicit tool label — the two-phase verbs refuse under
/// their own names so the teaching error points at the right retry surface.
fn refuse_for(tool: &str, detail: String) -> M1ndError {
    M1ndError::InvalidParams {
        tool: tool.into(),
        detail,
    }
}

/// B1 — the full file set a transplant call will TOUCH, derived READ-ONLY for the
/// M1ND_PROOF_GATE preflight: source + dest + every file referencer discovery
/// names. This is a conservative SUPERSET of what the write ends up mutating (a
/// discovered referencer that later yields no rewritable site is still gated) —
/// over-asking for permits is safe; under-asking is exactly the hole B1 closes:
/// the verb writes files the caller never named, and the armed gate saw only
/// source+dest. Malformed params degrade to whatever paths ARE present (an empty
/// set makes the gate refuse as unproven, mirroring the other write tools).
pub fn proof_gate_touched_files(state: &SessionState, params: &serde_json::Value) -> Vec<String> {
    let mut out: Vec<String> = ["source_file", "dest_file"]
        .iter()
        .filter_map(|k| params.get(*k).and_then(|v| v.as_str()).map(str::to_string))
        .collect();
    let (Some(source_abs), Some(dest_abs)) = (
        params.get("source_file").and_then(|v| v.as_str()),
        params.get("dest_file").and_then(|v| v.as_str()),
    ) else {
        return out;
    };
    let Some(symbol) = params.get("symbol").and_then(|v| v.as_str()) else {
        return out;
    };
    let symbol = symbol.trim();
    let source_module = module_name(source_abs);
    let view = read_graph_view(state);
    let symbol_idx = view
        .fns
        .iter()
        .find(|f| f.name == symbol && file_matches(&f.file, source_abs))
        .map(|f| f.idx);
    let (referencers, _) = discover_referencers(
        &view,
        symbol_idx,
        source_abs,
        dest_abs,
        &source_module,
        symbol,
    );
    for r in referencers {
        if !out.iter().any(|o| paths_equal(o, &r)) {
            out.push(r);
        }
    }
    out
}

/// Staged-transplant TTL — same 5-minute window as the house `edit_preview`.
pub const TRANSPLANT_PREVIEW_TTL_MS: u64 = 5 * 60 * 1000;

/// The computed-but-unwritten result of a transplant (A2): every file the verb
/// will write with its FINAL content (fmt pass included) and base hash, the
/// per-file diff reports, and the candidate receipt. Producing this writes
/// nothing; landing it is [`apply_plan`].
struct TransplantPlan {
    planned: Vec<PlannedTransplantWrite>,
    reports: Vec<TransplantPlannedFileReport>,
    receipt: TransplantOutput,
}

/// The one-shot verb: plan + land in a single call (the original contract — the
/// two-phase pair below is the same computation split at the write boundary).
pub fn handle_transplant(
    state: &mut SessionState,
    input: TransplantInput,
) -> M1ndResult<TransplantOutput> {
    let start = Instant::now();
    let plan = plan_transplant(state, &input)?;
    // A1 — capture the moved symbols' node-addressed state BEFORE the write, while
    // their OLD nodes still live in the source file (the re-ingest recreates them).
    let moved_names: Vec<String> = std::iter::once(plan.receipt.moved_symbol.clone())
        .chain(plan.receipt.deps_travelled.iter().cloned())
        .collect();
    let captured = capture_moved_node_state(state, &moved_names, &input.source_file);
    let aged = apply_plan(state, &input.agent_id, &plan.planned)?;
    let state_left_behind = compute_state_left_behind(state, &captured, &input.dest_file);
    let mut receipt = plan.receipt;
    receipt.blocks_touched = aged;
    receipt.state_left_behind = state_left_behind;
    receipt.elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    Ok(receipt)
}

/// A2 — `transplant_preview`: compute EVERYTHING (contents, referencers, fmt,
/// candidate receipt), write NOTHING, stage the plan under a TTL'd handle.
pub fn handle_transplant_preview(
    state: &mut SessionState,
    input: TransplantInput,
) -> M1ndResult<TransplantPreviewOutput> {
    let start = Instant::now();
    let plan = plan_transplant(state, &input)?;
    let preview_id = state.next_transplant_preview_id(&input.agent_id);
    let created_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    state.transplant_previews.insert(
        preview_id.clone(),
        TransplantPreviewState {
            preview_id: preview_id.clone(),
            agent_id: input.agent_id.clone(),
            symbol: input.symbol.trim().to_string(),
            source_file: input.source_file.clone(),
            dest_file: input.dest_file.clone(),
            planned: plan.planned,
            receipt: plan.receipt.clone(),
            created_at_ms,
        },
    );
    state.track_agent(&input.agent_id);
    Ok(TransplantPreviewOutput {
        preview_id,
        ttl_ms: TRANSPLANT_PREVIEW_TTL_MS,
        files: plan.reports,
        candidate: plan.receipt,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

/// A2 — `transplant_commit`: redeem a staged plan. Re-validates the base hash of
/// EVERY planned file (any drift → stale refusal, zero writes — the TOCTOU half
/// of A5 closed at the write boundary), then lands atomically via the same batch
/// path as the one-shot verb. The handle is consumed on success only.
pub fn handle_transplant_commit(
    state: &mut SessionState,
    input: TransplantCommitInput,
) -> M1ndResult<TransplantCommitOutput> {
    let start = Instant::now();

    if !input.confirm {
        return Err(refuse_for(
            "transplant_commit",
            format!(
                "confirm must be true to land a staged transplant. Hint: review the preview's files + candidate receipt, then resend the same preview_id with confirm=true. Example: {{\"preview_id\":\"{}\",\"agent_id\":\"{}\",\"confirm\":true}}",
                input.preview_id, input.agent_id
            ),
        ));
    }

    // Garbage-collect expired handles (mirrors edit_commit's TTL sweep).
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    state
        .transplant_previews
        .retain(|_, v| now_ms.saturating_sub(v.created_at_ms) < TRANSPLANT_PREVIEW_TTL_MS);

    let Some(staged) = state.transplant_previews.get(&input.preview_id).cloned() else {
        return Err(refuse_for(
            "transplant_commit",
            format!(
                "preview_id not found or expired (TTL=5min): {}. Hint: run transplant_preview again for the same symbol/source/dest to mint a fresh preview_id, then retry transplant_commit.",
                input.preview_id
            ),
        ));
    };
    if staged.agent_id != input.agent_id {
        return Err(refuse_for(
            "transplant_commit",
            "preview belongs to a different agent. Hint: retry with the agent_id that created the preview, or mint your own via transplant_preview.".to_string(),
        ));
    }

    // TOCTOU gate: every file the plan will WRITE must be byte-stable since the
    // preview — including the DERIVED referencers the caller never named. One
    // drifted file refuses the WHOLE commit before any write.
    for p in &staged.planned {
        let current = std::fs::read_to_string(&p.file_path).unwrap_or_default();
        if crate::surgical_handlers::content_hash(&current) != p.base_hash {
            return Err(refuse_for(
                "transplant_commit",
                format!(
                    "source_modified: '{}' changed since transplant_preview — the staged plan is stale and NOTHING was written. Hint: re-run transplant_preview against the current files, review the fresh diff, then commit.",
                    p.file_path
                ),
            ));
        }
    }

    // A1 — capture node-addressed state before the write (mirrors the one-shot verb).
    let moved_names: Vec<String> = std::iter::once(staged.receipt.moved_symbol.clone())
        .chain(staged.receipt.deps_travelled.iter().cloned())
        .collect();
    let captured = capture_moved_node_state(state, &moved_names, &staged.source_file);
    let aged = apply_plan(state, &input.agent_id, &staged.planned)?;
    let state_left_behind = compute_state_left_behind(state, &captured, &staged.dest_file);
    state.transplant_previews.remove(&input.preview_id);
    state.track_agent(&input.agent_id);

    let mut receipt = staged.receipt;
    receipt.blocks_touched = aged;
    receipt.state_left_behind = state_left_behind;
    receipt.elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    Ok(TransplantCommitOutput {
        preview_id: input.preview_id,
        receipt,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

/// Land a computed plan through the existing `apply_batch` machinery (atomic,
/// re-ingesting) — the ONE write path both the one-shot verb and the commit use.
/// Returns the SystemBlock ids whose ratified boundary the write aged (D5b).
fn apply_plan(
    state: &mut SessionState,
    agent_id: &str,
    planned: &[PlannedTransplantWrite],
) -> M1ndResult<Vec<String>> {
    let batch = ApplyBatchInput {
        agent_id: agent_id.to_string(),
        edits: planned
            .iter()
            .map(|p| BatchEditItem {
                file_path: p.file_path.clone(),
                new_content: p.new_content.clone(),
                description: p.description.clone(),
            })
            .collect(),
        atomic: true,
        reingest: true,
        verify: false,
    };
    let batch_out = crate::surgical_handlers::handle_apply_batch(state, batch)?;
    if !batch_out.all_succeeded {
        return Err(refuse(format!(
            "atomic transplant write failed: only {}/{} files written",
            batch_out.files_written, batch_out.files_total
        )));
    }

    // D5b — the receipt-aging event (PRD §10 D5 option b). The atomic write
    // landed, so any SystemBlock whose ratified membership CLAIMS a touched file
    // now has a symbol that crossed its boundary while its path-set membership is
    // unchanged (the lie-window `reconcile` cannot see). Bump those boundaries
    // through the store's OCC path — the EXISTING stale_scope law then ages their
    // receipts. Best-effort by construction: the write cannot be un-landed, so a
    // missing store (`Ok(None)`) or a store read/save error yields an honest empty
    // list rather than failing a COMPLETED transplant; `blocks_touched` then
    // reports exactly what was aged.
    let touched: Vec<String> = planned.iter().map(|p| p.file_path.clone()).collect();
    let dir = crate::system_blocks_handlers::store_dir(state);
    let aged = match crate::system_blocks::age_touched_boundaries_in_dir(&dir, &touched) {
        Ok(Some((_store, aged))) => aged,
        Ok(None) | Err(_) => Vec::new(),
    };
    Ok(aged)
}

// ===========================================================================
// A1 — node-addressed state that must not silently orphan across the re-ingest
// ===========================================================================

/// The external_id + tag set of a moved symbol's OLD node, captured BEFORE the
/// write so the verb can report what the re-ingest orphans (`state_left_behind`).
struct CapturedNodeState {
    symbol: String,
    old_node_id: String,
    old_tags: Vec<String>,
}

/// Read the external_id and tag set of the `Function` node named `name` whose
/// provenance file matches `file_abs`. `None` when no such node is in the graph
/// (an untracked symbol carries no node-addressed state to lose).
fn read_fn_node_state(
    state: &SessionState,
    name: &str,
    file_abs: &str,
) -> Option<(String, Vec<String>)> {
    let graph = state.graph.read();
    let n = graph.num_nodes() as usize;
    // Reverse map node index -> external_id (mirrors xray's node_to_ext_map).
    let mut idx_to_ext = vec![String::new(); n];
    for (&interned, &nid) in &graph.id_to_node {
        let idx = nid.as_usize();
        if idx < n {
            idx_to_ext[idx] = graph.strings.resolve(interned).to_string();
        }
    }
    for (i, ext) in idx_to_ext.iter().enumerate() {
        if graph.nodes.node_type[i] != NodeType::Function {
            continue;
        }
        if graph.strings.try_resolve(graph.nodes.label[i]) != Some(name) {
            continue;
        }
        let file = graph.nodes.provenance[i]
            .source_path
            .and_then(|s| graph.strings.try_resolve(s))
            .unwrap_or("");
        if !file_matches(file, file_abs) {
            continue;
        }
        let tags: Vec<String> = graph
            .node_tags(NodeId::new(i as u32))
            .iter()
            .map(|s| s.to_string())
            .collect();
        return Some((ext.clone(), tags));
    }
    None
}

/// A1 — capture the node-addressed state of every moved symbol BEFORE the write,
/// while its OLD node still lives in `source_abs`. Symbols with no graph node are
/// skipped (nothing to lose).
fn capture_moved_node_state(
    state: &SessionState,
    moved_names: &[String],
    source_abs: &str,
) -> Vec<CapturedNodeState> {
    moved_names
        .iter()
        .filter_map(|name| {
            read_fn_node_state(state, name, source_abs).map(|(ext, tags)| CapturedNodeState {
                symbol: name.clone(),
                old_node_id: ext,
                old_tags: tags,
            })
        })
        .collect()
}

/// A1 — after the re-ingest, diff each captured OLD node against the moved
/// symbol's NEW node in `dest_abs` and report the orphaned tags. A tag is orphaned
/// iff the new node carries NO tag in its top-level namespace (`ns:` before the
/// first colon): the re-ingest deterministically regenerates every structural
/// namespace (`rust:*`, the domain tag, …) on the new node, so a namespace present
/// there was carried; a namespace ABSENT there (e.g. an agent's `xray:` paint) was
/// genuinely lost. Empty payloads (nothing orphaned) yield no entry.
fn compute_state_left_behind(
    state: &SessionState,
    captured: &[CapturedNodeState],
    dest_abs: &str,
) -> Vec<StateLeftBehind> {
    let namespace = |t: &str| t.split(':').next().unwrap_or(t).to_string();
    let mut out = Vec::new();
    for c in captured {
        let (new_id, new_tags) = read_fn_node_state(state, &c.symbol, dest_abs).unwrap_or_default();
        let new_namespaces: BTreeSet<String> = new_tags.iter().map(|t| namespace(t)).collect();
        let orphaned: Vec<String> = c
            .old_tags
            .iter()
            .filter(|t| !new_namespaces.contains(&namespace(t)))
            .cloned()
            .collect();
        if orphaned.is_empty() {
            continue;
        }
        out.push(StateLeftBehind {
            symbol: c.symbol.clone(),
            old_node_id: c.old_node_id.clone(),
            new_node_id: new_id,
            kind: "xray_tags".to_string(),
            detail: orphaned,
        });
    }
    out
}

/// Phases 0–7.7: everything the verb computes BEFORE the write boundary. Pure
/// read — every refusal here provably writes nothing, and both the one-shot verb
/// and the two-phase preview are thin wrappers over this one function.
fn plan_transplant(state: &SessionState, input: &TransplantInput) -> M1ndResult<TransplantPlan> {
    let start = Instant::now();

    // --- Phase 0: paths, same-file refusal, read both files from DISK ---------
    let source_abs = input.source_file.clone();
    let dest_abs = input.dest_file.clone();
    if paths_equal(&source_abs, &dest_abs) {
        return Err(refuse(format!(
            "source_file and dest_file are the same file ('{source_abs}'); a transplant must move a symbol ACROSS files"
        )));
    }

    let source_text = std::fs::read_to_string(&source_abs)
        .map_err(|e| refuse(format!("cannot read source_file '{source_abs}': {e}")))?;
    // The spike requires an existing destination — creating a new module + wiring
    // `mod <dest>;` into lib.rs is a larger feature (a real PRD decision); refusing
    // is the honest boundary. Zero writes on this path.
    let dest_text = std::fs::read_to_string(&dest_abs).map_err(|_| {
        refuse(format!(
            "dest_file '{dest_abs}' does not exist or cannot be read; the transplant spike requires the destination module to already exist (creating + wiring a new module is out of spike scope)"
        ))
    })?;

    let symbol = input.symbol.trim().to_string();
    let source_module = module_name(&source_abs);
    let dest_module = module_name(&dest_abs);

    // --- Phase 0.4: A7 IMPOSED boundaries (make the "ideal-falso" unreachable) -
    // Both states below produce a SUCCESS receipt over a BROKEN build today. Until
    // the reverse-gate (A4) exists, this preflight is the only defense, so it must
    // REFUSE and teach — writing nothing (every early return here precedes Phase 8).
    //
    // (a) Poisonous module stems. `module_name` is the file stem and the verb
    //     synthesizes `crate::<stem>::…` paths; for lib/main/mod that path is
    //     invalid (the crate root is not a module named `lib`), so the move breaks.
    let poison = |m: &str| matches!(m, "lib" | "main" | "mod");
    if poison(&source_module) || poison(&dest_module) {
        let (role, path, stem) = if poison(&source_module) {
            ("source_file", &source_abs, &source_module)
        } else {
            ("dest_file", &dest_abs, &dest_module)
        };
        return Err(refuse(format!(
            "{role} '{path}' has the reserved module stem `{stem}` — the transplant would synthesize the invalid module path `crate::{stem}::…` (a crate root is not a module named `{stem}`). Move the symbol to/from a normal module file instead; no file is touched."
        )));
    }

    // (b) Cross-crate. source and dest must share ONE crate root (the nearest
    //     ancestor directory holding a Cargo.toml); a move across that boundary
    //     dangles the `crate::…` paths the verb rewrites. Refuse only when BOTH
    //     roots resolve and DIFFER — an unrooted fixture (no Cargo.toml) is left to
    //     the other preflights rather than falsely blamed as cross-crate.
    if let (Some(src_root), Some(dst_root)) = (crate_root(&source_abs), crate_root(&dest_abs)) {
        if src_root != dst_root {
            return Err(refuse(format!(
                "cross-crate move refused: source_file lives in crate root '{src_root}' but dest_file lives in crate root '{dst_root}'. transplant moves a symbol WITHIN one crate (it synthesizes `crate::…` paths that cannot cross a crate boundary); no file is touched."
            )));
        }
    }

    // --- Phase 0.5: dest-collision preflight on DISK TRUTH --------------------
    // The graph can be STALE here (the battery poisons dest AFTER ingest, without
    // re-ingesting) — disk is the only sound source. A collision writes NOTHING.
    //
    // A8: FULL-namespace collision. The moved item is always a top-level `fn`, but
    // a homonym of ANY top-level kind (struct/enum/trait/type/const/static/mod/…)
    // in the dest is E0428 after a success receipt — so tree-sitter scans every
    // top-level item and the refusal names the occupant kind. The textual `fn` scan
    // stays as the fallback for when the grammar cannot be loaded.
    if let Some(kind) = ts_dest_collision_kind(&dest_text, &symbol) {
        return Err(refuse(format!(
            "dest collision: '{dest_abs}' already defines a top-level `{kind} {symbol}` — moving `fn {symbol}` here would be a duplicate definition (E0428). No file is touched; move or rename the existing `{kind}` first."
        )));
    }
    if defines_fn(&dest_text, &symbol) {
        return Err(refuse(format!(
            "dest collision: '{dest_abs}' already defines `fn {symbol}` — the transplant is refused and no file is touched (move or rename the existing definition first)"
        )));
    }

    // --- Phase 1: resolve symbol + trichotomy FROM THE GRAPH ------------------
    let view = read_graph_view(state);

    // Disk is authoritative for "does the source still define this symbol?" — a
    // stale merge node must never let a moved-away symbol resolve again.
    if !defines_fn(&source_text, &symbol) {
        // Where does it actually live? Prefer a graph fn-node in another file;
        // fall back to a disk scan of the dest, then honest not-found.
        if let Some(other) = view
            .fns
            .iter()
            .find(|f| f.name == symbol && !file_matches(&f.file, &source_abs))
        {
            return Err(refuse(format!(
                "symbol `{symbol}` is not defined in source_file '{source_abs}' — it lives in '{}'. Point source_file at where the symbol actually is.",
                f_display(&other.file)
            )));
        }
        if defines_fn(&dest_text, &symbol) {
            return Err(refuse(format!(
                "symbol `{symbol}` is not in source_file '{source_abs}' — it already lives in dest_file '{dest_abs}' (already transplanted?)"
            )));
        }
        let mut here = source_fn_names(&source_text);
        here.sort();
        return Err(refuse(format!(
            "symbol `{symbol}` not found in source_file '{source_abs}'. Top-level functions defined there: [{}]",
            here.join(", ")
        )));
    }

    let source_lines: Vec<String> = source_text.lines().map(str::to_string).collect();
    let source_trailing_nl = source_text.ends_with('\n');

    // Locate the moved fn's declaration line. Prefer the graph's line_start hint;
    // fall back to a textual scan when the hint does not land on the decl.
    let graph_hint = view
        .fns
        .iter()
        .find(|f| f.name == symbol && file_matches(&f.file, &source_abs))
        .map(|f| f.decl_line);
    let decl_idx = locate_fn_decl(&source_lines, &symbol, graph_hint).ok_or_else(|| {
        refuse(format!(
            "internal: could not locate the declaration line of `fn {symbol}` in '{source_abs}'"
        ))
    })?;
    let hint_worked = graph_hint
        .map(|h| h >= 1 && (h as usize - 1) == decl_idx)
        .unwrap_or(false);

    // Parse the source ONCE: the true item extents (closing-brace rows) and the set
    // of TOP-LEVEL fns. This is what kills the brace-counting gap and stops a nested
    // helper from being treated as a movable top-level item.
    let ts_fns = ts_top_level_fns(&source_text);
    if !ts_fns.is_empty() && !ts_is_top_level(&ts_fns, &symbol) {
        return Err(refuse(format!(
            "symbol `{symbol}` is not a TOP-LEVEL `fn` in '{source_abs}' (it is nested in a module, a method, or a closure); the transplant spike moves only top-level free functions"
        )));
    }

    // Same-file fn nodes and the symbol's graph index (used for the edge queries).
    let samefile_fns: Vec<&FnNode> = view
        .fns
        .iter()
        .filter(|f| file_matches(&f.file, &source_abs))
        .collect();
    let symbol_idx = samefile_fns
        .iter()
        .find(|f| f.name == symbol)
        .map(|f| f.idx);

    // --- Phase 2: dependency trichotomy via `calls` edges ---------------------
    // moved (travels) = fixpoint of same-file callees whose EVERY same-file caller
    // is itself in the moved set; shared (stays) = same-file callees of the moved
    // set that still have an outside same-file caller.
    let (travelled_all, shared_names, dependency_source) = classify_dependencies(
        &view,
        &samefile_fns,
        symbol_idx,
        &symbol,
        &source_lines,
        decl_idx,
        &ts_fns,
    );
    // A travelled dep must itself be a TOP-LEVEL fn to be safely cut; a graph node
    // whose provenance is this file but which is actually nested in a module stays
    // put (cutting it would mangle the module).
    let travelled_names: Vec<String> = travelled_all
        .into_iter()
        .filter(|n| ts_fns.is_empty() || ts_is_top_level(&ts_fns, n))
        .collect();

    // --- Phase 3: cut regions (moved symbol + travelled deps) -----------------
    // Each region is widened up over trivia and down over one trailing blank.
    let mut move_targets: Vec<String> = vec![symbol.clone()];
    move_targets.extend(travelled_names.iter().cloned());

    let mut cut_ranges: Vec<(usize, usize)> = Vec::new(); // (first_trivia, last_trailing)
    let mut moved_texts: Vec<Vec<String>> = Vec::new(); // widened item WITHOUT trailing blank
    for name in &move_targets {
        let hint = view
            .fns
            .iter()
            .find(|f| f.name == *name && file_matches(&f.file, &source_abs))
            .map(|f| f.decl_line);
        let Some(d) = locate_fn_decl(&source_lines, name, hint) else {
            return Err(refuse(format!(
                "internal: travelled dependency `fn {name}` vanished from '{source_abs}' before the cut"
            )));
        };
        let (top, brace_end, trailing_end) =
            item_region(&source_lines, d, ts_end_row(&ts_fns, name, d));
        cut_ranges.push((top, trailing_end));
        moved_texts.push(source_lines[top..=brace_end].to_vec());
    }

    // --- Phase 3.5: carry the moved fn's OWN import needs (rope's law) --------
    // Over-provision: every top-level `use` of the source file is a candidate.
    // Prune: keep only those whose BOUND name (alias or last segment) appears as
    // an identifier in the moved text, and drop members the dest already binds
    // (an identical re-import is E0252) or that resolve locally in dest.
    let moved_blob = moved_texts
        .iter()
        .flat_map(|mt| mt.iter())
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    let mut carried_notes: Vec<String> = Vec::new();
    let (carried_use_lines, carried_bound_names) = carry_source_imports(
        &source_lines,
        &dest_text,
        &moved_blob,
        &source_module,
        &dest_module,
        &move_targets,
        &shared_names,
        &mut carried_notes,
    );

    // --- Phase 4: shared deps — visibility bump (source) + use lines (dest) ----
    let mut source_lines_bumped = source_lines.clone();
    let mut deps_shared: Vec<SharedDepReport> = Vec::new();
    let mut dest_use_lines: Vec<String> = Vec::new();
    for name in &shared_names {
        let hint = view
            .fns
            .iter()
            .find(|f| f.name == *name && file_matches(&f.file, &source_abs))
            .map(|f| f.decl_line);
        if let Some(d) = locate_fn_decl(&source_lines, name, hint) {
            let before = visibility_of(&source_lines[d]);
            let after = if before == "private" {
                bump_to_pub_crate(&mut source_lines_bumped[d]);
                "pub(crate)".to_string()
            } else {
                before.clone()
            };
            deps_shared.push(SharedDepReport {
                name: name.clone(),
                visibility_before: before,
                visibility_after: after,
            });
        }
        dest_use_lines.push(format!("use crate::{source_module}::{name};"));
    }

    // --- Phase 5: build the new SOURCE content --------------------------------
    let removed: BTreeSet<usize> = cut_ranges.iter().flat_map(|(a, b)| (*a..=*b)).collect();
    let mut new_source_lines: Vec<String> = source_lines_bumped
        .iter()
        .enumerate()
        .filter(|(i, _)| !removed.contains(i))
        .map(|(_, l)| l.clone())
        .collect();

    // Qualified self-references (`crate::<src_mod>::<symbol>(..)`) in the REMAINING
    // source code would dangle after the move — re-point them to the dest module.
    for l in new_source_lines.iter_mut() {
        *l = replace_qualified(l, &source_module, &dest_module, &symbol);
    }
    // Prune carried imports from the source when the REMAINDER no longer uses them
    // (conservative: a member still referenced anywhere outside use lines stays).
    prune_carried_source_imports(&mut new_source_lines, &carried_bound_names);
    // Self-use: if the source still calls the moved symbol BARE (not `::`-qualified)
    // after the cut, it needs a back-import `use crate::<dest_module>::<symbol>;`.
    let source_back_imported = new_source_lines
        .iter()
        .any(|l| references_bare_call(l, &symbol));
    if source_back_imported {
        let at = header_end(&new_source_lines);
        new_source_lines.insert(at, format!("use crate::{dest_module}::{symbol};"));
    }
    let new_source = join_lines(&new_source_lines, source_trailing_nl);

    // A PRIVATE moved fn that the source keeps calling must open up in its new
    // home — the back-import `use crate::<dest>::<symbol>;` cannot see a private
    // item across modules (E0603, proven by the self-hosting oracle).
    let mut moved_visibility_bumped = false;
    if source_back_imported {
        if let Some(first) = moved_texts.first_mut() {
            if let Some(decl) = first.iter().position(|l| line_defines_fn(l, &symbol)) {
                if visibility_of(&first[decl]) == "private" {
                    bump_to_pub_crate(&mut first[decl]);
                    moved_visibility_bumped = true;
                }
            }
        }
    }

    // --- Phase 6: build the new DEST content ----------------------------------
    let dest_lines: Vec<String> = dest_text.lines().map(str::to_string).collect();
    let dest_trailing_nl = dest_text.ends_with('\n');
    let insert_at = header_end(&dest_lines);
    let mut block: Vec<String> = Vec::new();
    for u in carried_use_lines.iter().chain(dest_use_lines.iter()) {
        block.push(u.clone());
    }
    if !dest_use_lines.is_empty() || !carried_use_lines.is_empty() {
        block.push(String::new());
    }
    for mt in &moved_texts {
        block.extend(mt.iter().cloned());
        block.push(String::new());
    }
    let mut new_dest_lines: Vec<String> = Vec::new();
    new_dest_lines.extend(dest_lines[..insert_at].iter().cloned());
    new_dest_lines.extend(block);
    new_dest_lines.extend(dest_lines[insert_at..].iter().cloned());
    let mut new_dest = join_lines(&new_dest_lines, dest_trailing_nl);

    // --- Phase 7: referencer discovery (graph edges + textual cross-check) -----
    let (referencing_files, referencer_source) = discover_referencers(
        &view,
        symbol_idx,
        &source_abs,
        &dest_abs,
        &source_module,
        &symbol,
    );

    // --- Phase 7.3: A3 protected-zone gate (the house Money-Zone law) -----------
    // With the full touched set known (source + dest + every DERIVED referencer),
    // enforce ci/protected-zones.json: any touched path inside a guarded zone
    // refuses the teach UNLESS the caller carried the explicit `allow_protected`
    // gesture. A pure preflight — every return here precedes the write, so a refusal
    // is byte-identity-safe. The gesture (when present) is recorded in the receipt.
    let protected_zone = enforce_protected_zones(
        &source_abs,
        &dest_abs,
        &referencing_files,
        input.allow_protected.as_deref(),
    )?;

    // Rewrite referencers BEFORE assembling the edit batch, so a referencer that
    // IS the destination mutates `new_dest` (post-insertion) instead of emitting a
    // second edit whose content — computed from the pre-insert dest text — would
    // clobber the insertion and DESTROY the item (proptest-found corruption).
    let mut refs_rewritten = 0usize;
    let mut refs_unresolved: Vec<String> = carried_notes;
    // Planned referencer writes, each paired with the BASE text it was computed
    // from (the hash is the commit's TOCTOU anchor; the text feeds the diff report).
    let mut referencer_planned: Vec<(PlannedTransplantWrite, String)> = Vec::new();
    for rf in &referencing_files {
        if paths_equal(rf, &source_abs) {
            // The source's own remaining references were already re-pointed in
            // Phase 5 (qualified rewrite + bare back-import).
            continue;
        }
        if paths_equal(rf, &dest_abs) {
            // Dest-as-referencer: the symbol is LOCAL after the move. Re-point its
            // qualified refs to the dest module and drop any now-self import.
            let rw =
                rewrite_referencer_text(&new_dest, &source_module, &dest_module, &symbol, rf, true);
            refs_rewritten += rw.rewritten;
            refs_unresolved.extend(rw.unresolved);
            new_dest = rw.new_text;
            continue;
        }
        let text = match std::fs::read_to_string(rf) {
            Ok(t) => t,
            Err(e) => {
                refs_unresolved.push(format!("{rf}: unreadable ({e})"));
                continue;
            }
        };
        let rw = rewrite_referencer_text(&text, &source_module, &dest_module, &symbol, rf, false);
        if rw.rewritten == 0 && rw.unresolved.is_empty() {
            // The graph pointed here but no textual site was rewritable — surface
            // it honestly instead of pretending the reference was handled.
            refs_unresolved.push(format!(
                "{rf}: referenced `{symbol}` per the graph but no rewritable `{source_module}::{symbol}` site was found (macro or generated code?)"
            ));
            continue;
        }
        refs_unresolved.extend(rw.unresolved);
        if rw.rewritten == 0 {
            continue; // detected-but-unresolved: reported above, file untouched
        }
        refs_rewritten += rw.rewritten;
        referencer_planned.push((
            PlannedTransplantWrite {
                file_path: rf.clone(),
                new_content: rw.new_text,
                base_hash: crate::surgical_handlers::content_hash(&text),
                description: Some(format!(
                    "transplant: re-point `{symbol}` to `{dest_module}`"
                )),
            },
            text,
        ));
    }

    // Assemble the plan in write order (source, dest, referencers), each entry
    // hashed against the exact on-disk text its content was computed FROM.
    let mut planned: Vec<PlannedTransplantWrite> = vec![
        PlannedTransplantWrite {
            file_path: source_abs.clone(),
            new_content: new_source,
            base_hash: crate::surgical_handlers::content_hash(&source_text),
            description: Some(format!("transplant: remove `{symbol}` (+ travelled deps)")),
        },
        PlannedTransplantWrite {
            file_path: dest_abs.clone(),
            new_content: new_dest,
            base_hash: crate::surgical_handlers::content_hash(&dest_text),
            description: Some(format!("transplant: receive `{symbol}`")),
        },
    ];
    let mut base_texts: Vec<String> = vec![source_text.clone(), dest_text.clone()];
    for (pw, base) in referencer_planned {
        planned.push(pw);
        base_texts.push(base);
    }
    let files_changed: Vec<String> = planned.iter().map(|p| p.file_path.clone()).collect();

    // --- Phase 7.7: rustfmt the COMPUTED contents before the write ------------
    // The oracle is `cargo check`, never fmt — but in a fmt-gated repo the verb's
    // assembled output (compact moved items, inserted use blocks) would reprove CI
    // with no warning in the receipt. Formatting happens on the computed text,
    // BEFORE apply_batch, so the atomic write + re-ingest see the FINAL bytes.
    // Honest fallback: an unavailable/failing rustfmt writes the unformatted text
    // and the receipt carries the note instead of a silent skip.
    let mut fmt_notes: Vec<String> = Vec::new();
    for p in planned.iter_mut() {
        match rustfmt_content(&p.new_content) {
            Ok(formatted) => p.new_content = formatted,
            Err(note) => {
                let unavailable = note.starts_with("rustfmt unavailable");
                fmt_notes.push(format!("{}: {note}", p.file_path));
                if unavailable {
                    break; // spawning again cannot succeed; one note explains all
                }
            }
        }
    }
    let rustfmt_status = if fmt_notes.is_empty() {
        "applied".to_string()
    } else {
        fmt_notes.join("; ")
    };

    // Per-file diff reports against the base texts, AFTER the fmt pass — the
    // preview shows exactly the bytes the commit will land.
    let reports: Vec<TransplantPlannedFileReport> = planned
        .iter()
        .zip(base_texts.iter())
        .map(|(p, base)| {
            let (lines_added, lines_removed) =
                crate::surgical_handlers::diff_summary(base, &p.new_content);
            TransplantPlannedFileReport {
                file_path: p.file_path.clone(),
                base_hash: p.base_hash.clone(),
                lines_added,
                lines_removed,
            }
        })
        .collect();

    let receipt = TransplantOutput {
        moved_symbol: symbol,
        source_module,
        dest_module,
        files_changed,
        deps_travelled: travelled_names,
        deps_shared,
        referencing_files,
        refs_rewritten,
        refs_unresolved,
        source_back_imported,
        imports_carried: carried_use_lines,
        moved_visibility_bumped,
        dependency_source: if hint_worked {
            dependency_source
        } else {
            format!("{dependency_source} (line-hint missed; decl located textually)")
        },
        referencer_source,
        rustfmt: rustfmt_status,
        // Filled by `apply_plan` at the write boundary — the plan (a pure read)
        // ages no boundary, so a preview honestly shows an empty set.
        blocks_touched: Vec::new(),
        // Filled by the two handlers around the write boundary (A1): a preview
        // orphans nothing, so it honestly shows an empty set here.
        state_left_behind: Vec::new(),
        // A3 — the Money-Zone gesture, when the preflight cleared a guarded crossing.
        protected_zone,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    };

    Ok(TransplantPlan {
        planned,
        reports,
        receipt,
    })
}

/// §7.7 — pipe one computed file content through `rustfmt --edition 2021`
/// (stdin → stdout). `Ok(formatted)` on success; `Err(note)` when rustfmt cannot
/// be spawned or rejects the content — the caller then writes the UNFORMATTED
/// text and surfaces the note in the receipt, because fmt must never block a
/// compile-correct move (the oracle is `cargo check`).
fn rustfmt_content(content: &str) -> Result<String, String> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};
    let child = Command::new("rustfmt")
        .args(["--edition", "2021"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return Err(format!("rustfmt unavailable: {e}")),
    };
    if let Some(mut stdin) = child.stdin.take() {
        if let Err(e) = stdin.write_all(content.as_bytes()) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("rustfmt stdin write failed: {e}"));
        }
    }
    match child.wait_with_output() {
        Ok(out) if out.status.success() => match String::from_utf8(out.stdout) {
            Ok(s) if !s.trim().is_empty() => Ok(s),
            Ok(_) => Err("rustfmt produced empty output".to_string()),
            Err(e) => Err(format!("rustfmt produced non-UTF8 output: {e}")),
        },
        Ok(out) => Err(format!(
            "rustfmt failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(e) => Err(format!("rustfmt did not run: {e}")),
    }
}

// ===========================================================================
// Graph extraction
// ===========================================================================

/// Lift the `fn` nodes, file paths and edges out of the live graph in one pass.
fn read_graph_view(state: &SessionState) -> GraphView {
    let graph = state.graph.read();
    let n = graph.num_nodes() as usize;
    let mut fns = Vec::new();
    let mut file_paths = Vec::new();
    let mut node_file = Vec::with_capacity(n);
    for i in 0..n {
        let nt = graph.nodes.node_type[i];
        let prov = &graph.nodes.provenance[i];
        let file = prov
            .source_path
            .and_then(|s| graph.strings.try_resolve(s))
            .unwrap_or("")
            .to_string();
        node_file.push(file.clone());
        match nt {
            NodeType::Function => {
                let name = graph
                    .strings
                    .try_resolve(graph.nodes.label[i])
                    .unwrap_or("")
                    .to_string();
                fns.push(FnNode {
                    idx: i,
                    name,
                    file,
                    decl_line: prov.line_start,
                });
            }
            NodeType::File if !file.is_empty() && !file_paths.contains(&file) => {
                file_paths.push(file);
            }
            _ => {}
        }
    }
    // Collect every forward edge once (fixture graphs are tiny). Production note:
    // a real verb would query out_range/in_range of the resolved nodes only.
    let mut edges = Vec::new();
    for i in 0..n {
        let nid = NodeId::new(i as u32);
        for pos in graph.csr.out_range(nid) {
            let tgt = graph.csr.targets[pos].as_usize();
            let rel = graph.strings.resolve(graph.csr.relations[pos]).to_string();
            edges.push((i, tgt, rel));
        }
    }
    GraphView {
        fns,
        file_paths,
        node_file,
        edges,
    }
}

/// Trichotomy from `calls` edges. Returns `(travelled, shared, source_label)`.
#[allow(clippy::too_many_arguments)]
fn classify_dependencies(
    view: &GraphView,
    samefile_fns: &[&FnNode],
    symbol_idx: Option<usize>,
    symbol: &str,
    source_lines: &[String],
    decl_idx: usize,
    ts_fns: &[TsFn],
) -> (Vec<String>, Vec<String>, String) {
    let samefile_idx: BTreeSet<usize> = samefile_fns.iter().map(|f| f.idx).collect();
    let name_of = |idx: usize| -> Option<String> {
        samefile_fns
            .iter()
            .find(|f| f.idx == idx)
            .map(|f| f.name.clone())
    };

    if let Some(sym_idx) = symbol_idx {
        // callees(n): same-file `calls` targets of n.
        let callees = |n: usize| -> Vec<usize> {
            view.edges
                .iter()
                .filter(|(s, t, r)| *s == n && r == "calls" && samefile_idx.contains(t))
                .map(|(_, t, _)| *t)
                .collect()
        };
        // same-file callers of m (excluding m itself).
        let callers = |m: usize| -> Vec<usize> {
            view.edges
                .iter()
                .filter(|(s, t, r)| *t == m && r == "calls" && samefile_idx.contains(s) && *s != m)
                .map(|(s, _, _)| *s)
                .collect()
        };

        // moved fixpoint.
        let mut moved: BTreeSet<usize> = BTreeSet::from([sym_idx]);
        loop {
            let mut grew = false;
            let reachable: BTreeSet<usize> = moved.iter().flat_map(|&n| callees(n)).collect();
            for m in reachable {
                if moved.contains(&m) {
                    continue;
                }
                if callers(m).iter().all(|c| moved.contains(c)) {
                    moved.insert(m);
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        // shared = same-file callees of the moved set that stayed out.
        let shared: BTreeSet<usize> = moved
            .iter()
            .flat_map(|&n| callees(n))
            .filter(|m| !moved.contains(m))
            .collect();

        let travelled: Vec<String> = moved
            .iter()
            .filter(|&&i| i != sym_idx)
            .filter_map(|&i| name_of(i))
            .collect();
        let shared_names: Vec<String> = shared.iter().filter_map(|&i| name_of(i)).collect();
        return (travelled, shared_names, "graph_edges".to_string());
    }

    // Textual fallback: the graph had no node for the symbol (no calls edges to
    // trust). Parse callees of the moved fn body and classify by counting other
    // same-file call sites. Reported honestly as "textual".
    let (travelled, shared) =
        classify_dependencies_textually(source_lines, symbol, decl_idx, ts_fns);
    (travelled, shared, "textual".to_string())
}

/// Textual trichotomy fallback: scan the moved fn body for `name(` calls to
/// sibling top-level fns, then a dep TRAVELS iff it has no other call site in the
/// file outside the moved region.
fn classify_dependencies_textually(
    source_lines: &[String],
    symbol: &str,
    decl_idx: usize,
    ts_fns: &[TsFn],
) -> (Vec<String>, Vec<String>) {
    let all_fns = source_fn_names(&source_lines.join("\n"));
    let (top, brace_end, _) =
        item_region(source_lines, decl_idx, ts_end_row(ts_fns, symbol, decl_idx));
    let body = source_lines[top..=brace_end].join("\n");
    let mut travelled = Vec::new();
    let mut shared = Vec::new();
    for f in &all_fns {
        if f == symbol {
            continue;
        }
        if !references_call(&body, f) {
            continue; // not a callee of the moved fn
        }
        // count call sites outside the moved region
        let mut outside = false;
        for (i, line) in source_lines.iter().enumerate() {
            if (top..=brace_end).contains(&i) {
                continue;
            }
            if references_call(line, f) {
                outside = true;
                break;
            }
        }
        if outside {
            shared.push(f.clone());
        } else {
            travelled.push(f.clone());
        }
    }
    (travelled, shared)
}

/// Referencing files: graph `calls`/`imports` edges targeting the symbol (from
/// nodes in OTHER files), unioned with a textual scan of the graph's file
/// inventory for `<src_mod>::<symbol>` (catches qualified paths the graph may
/// miss). Returns `(files, source_label)`.
fn discover_referencers(
    view: &GraphView,
    symbol_idx: Option<usize>,
    source_abs: &str,
    dest_abs: &str,
    source_module: &str,
    symbol: &str,
) -> (Vec<String>, String) {
    let mut graph_files: BTreeSet<String> = BTreeSet::new();
    if let Some(sym_idx) = symbol_idx {
        for (s, t, r) in &view.edges {
            if *t != sym_idx {
                continue;
            }
            if r != "calls" && r != "imports" && r != "references" {
                continue;
            }
            // file of the source node
            let file = file_of_node(view, *s);
            if let Some(f) = file {
                if !file_matches(&f, source_abs) {
                    graph_files.insert(f);
                }
            }
        }
    }

    // Textual cross-check over the graph's known files: a qualified
    // `<src_mod>::<symbol>` path, OR a glob `use …::<src_mod>::*;` combined with a
    // bare use of the symbol (the glob referencer forms NO graph edge when the
    // symbol is used as a value — the silent-miss hazard the harness pins).
    let needle = format!("{source_module}::{symbol}");
    let mut textual_files: BTreeSet<String> = BTreeSet::new();
    for f in &view.file_paths {
        if file_matches(f, source_abs) {
            continue;
        }
        // Resolve the repo-relative file path to something readable: prefer an
        // absolute sibling of source_abs when the stored path is relative.
        let abs = resolve_sibling(source_abs, f);
        if let Ok(text) = std::fs::read_to_string(&abs) {
            if text.contains(&needle)
                || (has_glob_of(&text, source_module) && uses_symbol_bare(&text, symbol))
            {
                textual_files.insert(abs);
            }
        }
    }
    // The DEST file may legitimately reference the symbol too (qualified path or
    // import); it is handled specially by the caller (rewritten in new_dest).
    if let Ok(text) = std::fs::read_to_string(dest_abs) {
        if text.contains(&needle) {
            textual_files.insert(dest_abs.to_string());
        }
    }

    // Normalize graph_files to absolute readable paths too.
    let graph_abs: BTreeSet<String> = graph_files
        .iter()
        .map(|f| resolve_sibling(source_abs, f))
        .collect();

    let union: BTreeSet<String> = graph_abs.union(&textual_files).cloned().collect();
    let label = match (graph_abs.is_empty(), textual_files.is_empty()) {
        (false, false) => "both",
        (false, true) => "graph_edges",
        (true, false) => "textual",
        (true, true) => "none",
    }
    .to_string();
    (union.into_iter().collect(), label)
}

fn file_of_node(view: &GraphView, idx: usize) -> Option<String> {
    // The per-node provenance path covers both fn nodes (their source file) and
    // file nodes (their own path).
    view.node_file.get(idx).filter(|f| !f.is_empty()).cloned()
}

// ===========================================================================
// Tree-sitter extent (parse, don't count braces)
// ===========================================================================

/// A top-level `fn` item located by tree-sitter: its name and 0-based row span
/// `[start_row, end_row]`, where `end_row` is the row of the item's closing brace.
struct TsFn {
    name: String,
    start_row: usize,
    end_row: usize,
}

/// Parse `source` and return every TOP-LEVEL `function_item` (a direct child of the
/// file root — a `fn` nested inside a `mod {}` is deliberately excluded, so a nested
/// helper is never mistaken for a movable top-level fn). Empty vec when the grammar
/// cannot be loaded/parsed; callers then fall back to the textual brace counter.
fn ts_top_level_fns(source: &str) -> Vec<TsFn> {
    let mut parser = tree_sitter::Parser::new();
    let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    if parser.set_language(&language).is_err() {
        return Vec::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };
    let root = tree.root_node();
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() == "function_item" {
            if let Some(name_node) = child.child_by_field_name("name") {
                if let Ok(name) = name_node.utf8_text(bytes) {
                    out.push(TsFn {
                        name: name.to_string(),
                        start_row: child.start_position().row,
                        end_row: child.end_position().row,
                    });
                }
            }
        }
    }
    out
}

/// A8 — scan `source` for a TOP-LEVEL item named `symbol` of ANY kind and return
/// the kind keyword ("fn"/"struct"/"enum"/"trait"/"type"/"const"/"static"/"mod"/
/// "union"), or `None`. The moved item is always a `fn` (value namespace), but the
/// spike refuses a homonym of any kind — a conservative over-refusal that makes the
/// E0428-after-success "ideal-falso" unreachable and teaches the caller exactly what
/// occupies the name. `None` when the grammar cannot be loaded (caller falls back to
/// the textual `fn` scan) or no top-level homonym exists.
fn ts_dest_collision_kind(source: &str, symbol: &str) -> Option<&'static str> {
    let mut parser = tree_sitter::Parser::new();
    let language: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    parser.set_language(&language).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();
    let bytes = source.as_bytes();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        let kind = match child.kind() {
            "function_item" => "fn",
            "struct_item" => "struct",
            "enum_item" => "enum",
            "trait_item" => "trait",
            "type_item" => "type",
            "const_item" => "const",
            "static_item" => "static",
            "mod_item" => "mod",
            "union_item" => "union",
            _ => continue,
        };
        if let Some(name_node) = child.child_by_field_name("name") {
            if name_node.utf8_text(bytes) == Ok(symbol) {
                return Some(kind);
            }
        }
    }
    None
}

/// The tree-sitter closing-brace row for the top-level `fn name` whose span covers
/// `decl_idx` (0-based). `None` when tree-sitter is unavailable or the decl is not a
/// top-level fn — callers then use brace counting.
fn ts_end_row(ts_fns: &[TsFn], name: &str, decl_idx: usize) -> Option<usize> {
    ts_fns
        .iter()
        .filter(|f| f.name == name)
        .find(|f| f.start_row <= decl_idx && decl_idx <= f.end_row)
        .map(|f| f.end_row)
}

/// True when `name` is a top-level fn per tree-sitter. Only meaningful when the
/// parse produced at least one fn — an empty list means "unknown", not "none", so
/// callers must guard with `!ts_fns.is_empty()` before treating a miss as authority.
fn ts_is_top_level(ts_fns: &[TsFn], name: &str) -> bool {
    ts_fns.iter().any(|f| f.name == name)
}

// ===========================================================================
// Textual helpers (item region, fn detection, rewrites)
// ===========================================================================

/// Return `(first_trivia_line, closing_brace_line, last_trailing_blank_line)` for
/// the item whose declaration is on `decl_idx`. Widens UP over contiguous `///`
/// docs, `#[...]` attributes and `//` comments, and DOWN over trailing blanks. The
/// closing-brace line is the tree-sitter parse result when available (`ts_end`) —
/// immune to a `}`/`{` inside a string, char or macro body — and falls back to the
/// brace counter otherwise.
fn item_region(lines: &[String], decl_idx: usize, ts_end: Option<usize>) -> (usize, usize, usize) {
    // widen up
    let mut top = decl_idx;
    while top > 0 {
        let t = lines[top - 1].trim_start();
        if t.starts_with("///")
            || t.starts_with("//!")
            || t.starts_with("#[")
            || t.starts_with("#![")
            || (t.starts_with("//") && !t.is_empty())
        {
            top -= 1;
        } else {
            break;
        }
    }
    // Prefer the PARSED extent; fall back to the brace counter.
    let brace_end = ts_end
        .filter(|&e| e >= decl_idx && e < lines.len())
        .unwrap_or_else(|| find_item_end(lines, decl_idx));
    // widen down over trailing blank lines
    let mut trailing = brace_end;
    while trailing + 1 < lines.len() && lines[trailing + 1].trim().is_empty() {
        trailing += 1;
    }
    (top, brace_end, trailing)
}

/// Mirror of `surgical_handlers::find_brace_end` (a private helper): track brace
/// depth from `start` to the line closing the item's block.
fn find_item_end(lines: &[String], start: usize) -> usize {
    let mut depth: i32 = 0;
    let mut opened = false;
    for (i, line) in lines.iter().enumerate().skip(start) {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                opened = true;
            } else if ch == '}' {
                depth -= 1;
                if opened && depth == 0 {
                    return i;
                }
            }
        }
    }
    (start + 50).min(lines.len().saturating_sub(1))
}

/// True when `text` contains a top-level `fn <name>` definition (single-line
/// signature). Strips leading visibility/qualifier keywords, then requires an
/// identifier boundary after the name (`(`, `<`, or whitespace).
fn defines_fn(text: &str, name: &str) -> bool {
    text.lines().any(|l| line_defines_fn(l, name))
}

fn line_defines_fn(line: &str, name: &str) -> bool {
    let mut t = line.trim_start();
    for kw in [
        "pub(crate) ",
        "pub(super) ",
        "pub ",
        "async ",
        "const ",
        "unsafe ",
        "default ",
    ] {
        while let Some(rest) = t.strip_prefix(kw) {
            t = rest;
        }
    }
    // handle any remaining `pub(...)` restriction (e.g. `pub(in path)`).
    if t.starts_with("pub(") {
        if let Some(close) = t.find(')') {
            t = t[close + 1..].trim_start();
        }
    }
    if let Some(rest) = t.strip_prefix("fn ") {
        let rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix(name) {
            return after.starts_with('(') || after.starts_with('<') || after.starts_with(' ');
        }
    }
    false
}

/// All top-level fn names defined in `text`.
fn source_fn_names(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for l in text.lines() {
        if let Some(name) = fn_name_on_line(l) {
            out.push(name);
        }
    }
    out
}

fn fn_name_on_line(line: &str) -> Option<String> {
    let mut t = line.trim_start();
    for kw in [
        "pub(crate) ",
        "pub(super) ",
        "pub ",
        "async ",
        "const ",
        "unsafe ",
        "default ",
    ] {
        while let Some(rest) = t.strip_prefix(kw) {
            t = rest;
        }
    }
    if let Some(rest) = t.strip_prefix("pub(") {
        if let Some(close) = rest.find(')') {
            t = rest[close + 1..].trim_start();
        }
    }
    let rest = t.strip_prefix("fn ")?;
    let rest = rest.trim_start();
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Locate the 0-based line index of the moved fn's declaration, preferring the
/// graph's 1-based `line_start` hint when it lands on the decl.
fn locate_fn_decl(lines: &[String], name: &str, hint_1based: Option<u32>) -> Option<usize> {
    if let Some(h) = hint_1based {
        if h >= 1 {
            let idx = h as usize - 1;
            if idx < lines.len() && line_defines_fn(&lines[idx], name) {
                return Some(idx);
            }
        }
    }
    lines.iter().position(|l| line_defines_fn(l, name))
}

/// Visibility of a fn declaration line: "pub", "pub(crate)"/"pub(...)", or "private".
fn visibility_of(line: &str) -> String {
    let t = line.trim_start();
    if t.starts_with("pub(") {
        let inner: String = t.chars().take_while(|c| *c != ' ').collect();
        return inner; // e.g. "pub(crate)"
    }
    if t.starts_with("pub ") {
        return "pub".to_string();
    }
    "private".to_string()
}

/// Rewrite a private `fn ...` decl line to `pub(crate) fn ...`, preserving indent.
fn bump_to_pub_crate(line: &mut String) {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);
    if rest.starts_with("fn ") {
        *line = format!("{indent}pub(crate) {rest}");
    }
}

/// Header region end (0-based insert index): skip contiguous leading module docs
/// (`//!`), inner attributes (`#![`), blank lines and `use`/`pub use` statements.
/// Everything after is the first real item (or item-attached trivia).
fn header_end(lines: &[String]) -> usize {
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim_start();
        if t.is_empty()
            || t.starts_with("//!")
            || t.starts_with("#![")
            || t.starts_with("use ")
            || t.starts_with("pub use ")
        {
            i += 1;
        } else {
            break;
        }
    }
    i
}

/// True when `line` uses `name` as a call (`name(`), a path segment (`name::`),
/// or an imported bare reference (`name` word-bounded). Comments are ignored.
fn references_call(line: &str, name: &str) -> bool {
    let code = match line.split_once("//") {
        Some((before, _)) => before,
        None => line,
    };
    let mut idx = 0;
    while let Some(pos) = code[idx..].find(name) {
        let start = idx + pos;
        let end = start + name.len();
        let before_ok = start == 0
            || !code[..start]
                .chars()
                .next_back()
                .map(|c| c.is_alphanumeric() || c == '_')
                .unwrap_or(false);
        let after = &code[end..];
        let after_ok = after
            .chars()
            .next()
            .map(|c| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(true);
        if before_ok && after_ok {
            // require it to look like use: followed by ( or :: or end/paren
            let a = after.trim_start();
            if a.starts_with('(') || a.starts_with("::") || a.starts_with(';') || a.is_empty() {
                return true;
            }
        }
        idx = end;
    }
    false
}

/// `references_call` restricted to BARE uses: the occurrence must NOT be preceded
/// by `::` (a qualified path is not evidence that an import is needed).
fn references_bare_call(line: &str, name: &str) -> bool {
    let code = match line.split_once("//") {
        Some((before, _)) => before,
        None => line,
    };
    let mut idx = 0;
    while let Some(pos) = code[idx..].find(name) {
        let start = idx + pos;
        let end = start + name.len();
        let before_ok = start == 0
            || !code[..start]
                .chars()
                .next_back()
                .map(|c| c.is_alphanumeric() || c == '_')
                .unwrap_or(false);
        let qualified = code[..start].ends_with("::");
        let after = &code[end..];
        let after_ok = after
            .chars()
            .next()
            .map(|c| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(true);
        if before_ok && after_ok && !qualified {
            let a = after.trim_start();
            if a.starts_with('(') || a.starts_with(';') || a.is_empty() {
                return true;
            }
        }
        idx = end;
    }
    false
}

/// Replace `<src_mod>::<symbol>` with `<dest_mod>::<symbol>` in one line, with an
/// identifier boundary AFTER the symbol (so `alpha::move_me_extra` never matches)
/// and no identifier char before `src_mod` (so `not_alpha::move_me` never
/// matches; a preceding `::` — `crate::alpha::move_me` — is exactly the
/// qualified form and DOES match). Count changes via [`count_qualified`].
fn replace_qualified(line: &str, src_mod: &str, dest_mod: &str, symbol: &str) -> String {
    let from = format!("{src_mod}::{symbol}");
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(pos) = rest.find(&from) {
        let before_ok = {
            let head = &rest[..pos];
            !head
                .chars()
                .next_back()
                .map(|c| c.is_alphanumeric() || c == '_')
                .unwrap_or(false)
        };
        let after = &rest[pos + from.len()..];
        let after_ok = !after
            .chars()
            .next()
            .map(|c| c.is_alphanumeric() || c == '_')
            .unwrap_or(false);
        out.push_str(&rest[..pos]);
        if before_ok && after_ok {
            out.push_str(dest_mod);
            out.push_str("::");
            out.push_str(symbol);
        } else {
            out.push_str(&from);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Count boundary-checked `<src_mod>::<symbol>` occurrences in a line.
fn count_qualified(line: &str, src_mod: &str, symbol: &str) -> usize {
    let from = format!("{src_mod}::{symbol}");
    let mut n = 0;
    let mut rest = line;
    while let Some(pos) = rest.find(&from) {
        let before_ok = !rest[..pos]
            .chars()
            .next_back()
            .map(|c| c.is_alphanumeric() || c == '_')
            .unwrap_or(false);
        let after = &rest[pos + from.len()..];
        let after_ok = !after
            .chars()
            .next()
            .map(|c| c.is_alphanumeric() || c == '_')
            .unwrap_or(false);
        if before_ok && after_ok {
            n += 1;
        }
        rest = after;
    }
    n
}

/// True when `text` has a top-level glob import of `src_mod`:
/// `use …::<src_mod>::*;` (with or without `pub`).
fn has_glob_of(text: &str, src_mod: &str) -> bool {
    text.lines().any(|l| {
        let t = l.trim();
        (t.starts_with("use ") || t.starts_with("pub use "))
            && (t.ends_with(&format!("::{src_mod}::*;")) || t.ends_with(&format!("{src_mod}::*;")))
    })
}

/// True when a NON-use line of `text` uses `symbol` bare (call, value, or path
/// head) — the companion signal to [`has_glob_of`] for glob-referencer discovery.
fn uses_symbol_bare(text: &str, symbol: &str) -> bool {
    text.lines().any(|l| {
        let t = l.trim_start();
        if t.starts_with("use ") || t.starts_with("pub use ") {
            return false;
        }
        references_bare_call(l, symbol)
    })
}

// ---------------------------------------------------------------------------
// Use-form-aware referencer rewriting (B2)
// ---------------------------------------------------------------------------

/// Result of rewriting one referencing file.
struct RefRewrite {
    new_text: String,
    /// Reference sites re-pointed (imports split/re-pointed, qualified paths
    /// rewritten, glob-covered uses given an explicit import).
    rewritten: usize,
    /// Sites detected but NOT resolved, as honest human-readable notes.
    unresolved: Vec<String>,
}

/// A logical top-level `use` statement (may span multiple physical lines after
/// rustfmt splits a long group): the joined text and its line span.
struct UseStmt {
    /// `use`/`pub use` statement text joined to ONE line, `;` included.
    text: String,
    start: usize,
    /// Inclusive end line index.
    end: usize,
}

/// Collect logical use statements at the top level of the file.
fn collect_use_stmts(lines: &[String]) -> Vec<UseStmt> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim_start();
        let is_use_start =
            (t.starts_with("use ") || t.starts_with("pub use ")) && !lines[i].starts_with(' ');
        if is_use_start {
            let mut joined = lines[i].trim().to_string();
            let start = i;
            let mut end = i;
            while !joined.contains(';') && end + 1 < lines.len() {
                end += 1;
                joined.push(' ');
                joined.push_str(lines[end].trim());
            }
            out.push(UseStmt {
                text: joined,
                start,
                end,
            });
            i = end + 1;
        } else {
            i += 1;
        }
    }
    out
}

/// Split a `{a, b as c, self}` group body into trimmed members.
fn split_group_members(inner: &str) -> Vec<String> {
    inner
        .split(',')
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .collect()
}

/// The bound name of a group member: `move_me` and `move_me as mm` both name
/// `move_me` (the alias is what enters scope, but matching is by source name).
fn member_source_name(member: &str) -> &str {
    member.split_whitespace().next().unwrap_or(member)
}

/// The name a member BINDS in scope: the alias when present, else the member
/// itself (`HashMap as HM` binds `HM`; `HashMap` binds `HashMap`).
fn member_bound_name(member: &str) -> &str {
    member
        .rsplit(" as ")
        .next()
        .map(str::trim)
        .unwrap_or(member)
}

/// True when `name` occurs as a standalone identifier anywhere in `blob`
/// (word-boundary on both sides). Over-approximates inside strings/comments —
/// acceptable for rope's over-provision law (an extra import is a warning, a
/// missing one is breakage).
fn ident_used(blob: &str, name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut idx = 0;
    while let Some(pos) = blob[idx..].find(name) {
        let s = idx + pos;
        let e = s + name.len();
        let before_ok = !blob[..s]
            .chars()
            .next_back()
            .map(|c| c.is_alphanumeric() || c == '_')
            .unwrap_or(false);
        let after_ok = !blob[e..]
            .chars()
            .next()
            .map(|c| c.is_alphanumeric() || c == '_')
            .unwrap_or(false);
        if before_ok && after_ok {
            return true;
        }
        idx = e;
    }
    false
}

/// The parsed shape of one logical use statement's body (after `use `/`pub use `
/// and before `;`): its path prefix and members (empty prefix members = plain).
enum UseShape {
    /// `use a::b::C;` or `use a::b::C as D;` — path + optional alias tail.
    Plain { path: String },
    /// `use a::b::{x, y as z};`
    Group {
        prefix: String,
        members: Vec<String>,
    },
    /// `use a::b::*;`
    Glob { prefix: String },
    /// Nested groups etc. — not rewritten, only reported.
    Other,
}

fn parse_use_body(body: &str) -> UseShape {
    if body.matches('{').count() > 1 {
        return UseShape::Other;
    }
    if let Some(brace) = body.find('{') {
        let prefix = body[..brace].trim_end_matches("::").trim().to_string();
        let inner = body[brace + 1..].trim_end_matches('}').trim();
        return UseShape::Group {
            prefix,
            members: split_group_members(inner),
        };
    }
    if let Some(prefix) = body.strip_suffix("::*") {
        return UseShape::Glob {
            prefix: prefix.trim().to_string(),
        };
    }
    UseShape::Plain {
        path: body.to_string(),
    }
}

/// B3 — carry the moved fn's own file-level import needs into the destination.
/// Returns `(carried_use_lines_for_dest, carried_bound_names_for_source_prune)`.
/// Honest limits (reported via `notes`): source globs are never carried, nested
/// groups are never rewritten.
#[allow(clippy::too_many_arguments)]
fn carry_source_imports(
    source_lines: &[String],
    dest_text: &str,
    moved_blob: &str,
    source_module: &str,
    dest_module: &str,
    move_targets: &[String],
    shared_names: &[String],
    notes: &mut Vec<String>,
) -> (Vec<String>, Vec<String>) {
    // Names the destination ALREADY binds: its own use statements, its top-level
    // fns, the arriving items, and the shared-dep back-imports built in Phase 4.
    let dest_lines: Vec<String> = dest_text.lines().map(str::to_string).collect();
    let mut dest_bound: BTreeSet<String> = BTreeSet::new();
    for st in collect_use_stmts(&dest_lines) {
        let t = st.text.trim();
        let lead = if t.starts_with("pub use ") {
            "pub use "
        } else {
            "use "
        };
        let body = t.trim_start_matches(lead).trim_end_matches(';').trim();
        match parse_use_body(body) {
            UseShape::Plain { path } => {
                // The bound name is the alias when present, else the last segment.
                let bound = if let Some((_, alias)) = path.rsplit_once(" as ") {
                    alias.trim().to_string()
                } else {
                    path.rsplit("::").next().unwrap_or(&path).trim().to_string()
                };
                dest_bound.insert(bound);
            }
            UseShape::Group { members, .. } => {
                for m in members {
                    dest_bound.insert(member_bound_name(&m).to_string());
                }
            }
            _ => {}
        }
    }
    for f in source_fn_names(dest_text) {
        dest_bound.insert(f);
    }
    for t in move_targets {
        dest_bound.insert(t.clone());
    }
    for s in shared_names {
        dest_bound.insert(s.clone());
    }

    let mut carried_lines: Vec<String> = Vec::new();
    let mut carried_bound: Vec<String> = Vec::new();
    for st in collect_use_stmts(source_lines) {
        let t = st.text.trim();
        let lead = if t.starts_with("pub use ") {
            "pub use "
        } else {
            "use "
        };
        let body = t.trim_start_matches(lead).trim_end_matches(';').trim();
        // A use that resolves inside the DEST module would self-import there; a
        // use of the SOURCE module path would dangle semantics — skip both (the
        // shared-dep machinery already back-imports what the moved code needs).
        let dest_seg = format!("::{dest_module}");
        let src_seg = format!("::{source_module}");
        let module_of = |prefix: &str| -> bool {
            prefix == dest_module
                || prefix.ends_with(&dest_seg)
                || prefix == source_module
                || prefix.ends_with(&src_seg)
        };
        match parse_use_body(body) {
            UseShape::Plain { path } => {
                let bound = if let Some((_, alias)) = path.rsplit_once(" as ") {
                    alias.trim().to_string()
                } else {
                    path.rsplit("::").next().unwrap_or(&path).trim().to_string()
                };
                let prefix = path
                    .rsplit_once("::")
                    .map(|(p, _)| p)
                    .unwrap_or("")
                    .trim_end_matches(" as")
                    .to_string();
                if module_of(&prefix) {
                    continue;
                }
                if ident_used(moved_blob, &bound) && !dest_bound.contains(&bound) {
                    carried_lines.push(format!("use {path};"));
                    carried_bound.push(bound);
                }
            }
            UseShape::Group { prefix, members } => {
                if module_of(&prefix) {
                    continue;
                }
                let needed: Vec<String> = members
                    .into_iter()
                    .filter(|m| {
                        let b = member_bound_name(m);
                        ident_used(moved_blob, b) && !dest_bound.contains(b)
                    })
                    .collect();
                match needed.len() {
                    0 => {}
                    1 => {
                        carried_bound.push(member_bound_name(&needed[0]).to_string());
                        carried_lines.push(format!("use {prefix}::{};", needed[0]));
                    }
                    _ => {
                        for m in &needed {
                            carried_bound.push(member_bound_name(m).to_string());
                        }
                        carried_lines.push(format!("use {prefix}::{{{}}};", needed.join(", ")));
                    }
                }
            }
            UseShape::Glob { prefix } => {
                if module_of(&prefix) {
                    continue;
                }
                notes.push(format!(
                    "source glob `use {prefix}::*;` was NOT carried to the destination — if the moved fn relied on names it provides, add explicit imports in the destination"
                ));
            }
            UseShape::Other => {
                notes.push(format!(
                    "source nested-group import `{}` was NOT analyzed for carrying — verify the moved fn's imports manually if it relied on it",
                    st.text.trim()
                ));
            }
        }
    }
    (carried_lines, carried_bound)
}

/// Remove carried imports from the post-cut source when its REMAINDER no longer
/// references the bound name (conservative: any remaining use keeps the member).
fn prune_carried_source_imports(new_source_lines: &mut Vec<String>, carried_bound: &[String]) {
    if carried_bound.is_empty() {
        return;
    }
    let stmts = collect_use_stmts(new_source_lines);
    // The remainder = every line OUTSIDE use statements.
    let mut in_use: BTreeSet<usize> = BTreeSet::new();
    for st in &stmts {
        for i in st.start..=st.end {
            in_use.insert(i);
        }
    }
    let remainder: String = new_source_lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !in_use.contains(i))
        .map(|(_, l)| l.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    let prunable: Vec<&String> = carried_bound
        .iter()
        .filter(|b| !ident_used(&remainder, b))
        .collect();
    if prunable.is_empty() {
        return;
    }

    let mut replacements: Vec<(usize, usize, Vec<String>)> = Vec::new();
    for st in &stmts {
        let t = st.text.trim();
        let lead = if t.starts_with("pub use ") {
            "pub use "
        } else {
            "use "
        };
        let body = t.trim_start_matches(lead).trim_end_matches(';').trim();
        match parse_use_body(body) {
            UseShape::Plain { path } => {
                let bound = if let Some((_, alias)) = path.rsplit_once(" as ") {
                    alias.trim().to_string()
                } else {
                    path.rsplit("::").next().unwrap_or(&path).trim().to_string()
                };
                if prunable.iter().any(|p| ***p == *bound) {
                    replacements.push((st.start, st.end, Vec::new()));
                }
            }
            UseShape::Group { prefix, members } => {
                let kept: Vec<String> = members
                    .iter()
                    .filter(|m| {
                        let b = member_bound_name(m);
                        !prunable.iter().any(|p| p.as_str() == b)
                    })
                    .cloned()
                    .collect();
                if kept.len() == members.len() {
                    continue;
                }
                let mut lines = Vec::new();
                match kept.len() {
                    0 => {}
                    1 => lines.push(format!("{lead}{prefix}::{};", kept[0])),
                    _ => lines.push(format!("{lead}{prefix}::{{{}}};", kept.join(", "))),
                }
                replacements.push((st.start, st.end, lines));
            }
            _ => {}
        }
    }
    replacements.sort_by_key(|(s, _, _)| *s);
    for (s, e, repl) in replacements.into_iter().rev() {
        new_source_lines.splice(s..=e, repl);
    }
}

/// Use-form-aware rewrite of a referencing file (B2). Handles:
///   - qualified paths `<src_mod>::<symbol>` on any code line (boundary-checked);
///   - direct imports `use …::<src_mod>::<symbol> [as alias];` (via the same
///     qualified rewrite — the alias tail survives untouched);
///   - grouped imports `use …::<src_mod>::{a, <symbol> [as alias], b};` — the
///     symbol's member is SPLIT OUT to a new dest import (alias preserved), the
///     rest of the group survives;
///   - glob imports `use …::<src_mod>::*;` — the moved symbol silently leaves the
///     glob's coverage, so an explicit `use …::<dest_mod>::<symbol>;` is added
///     right after the glob (unless `symbol_is_local`);
///   - nested groups (`{a::{b}, c}`) — DETECTED and reported, never guessed at.
///
/// `symbol_is_local` = rewriting the DEST file itself: the symbol is defined
/// locally after the move, so any import of it is dropped rather than re-pointed
/// (a self-import is E0255) and glob coverage needs no replacement.
fn rewrite_referencer_text(
    text: &str,
    src_mod: &str,
    dest_mod: &str,
    symbol: &str,
    file_label: &str,
    symbol_is_local: bool,
) -> RefRewrite {
    let lines: Vec<String> = text.lines().map(str::to_string).collect();
    let trailing_nl = text.ends_with('\n');
    let use_stmts = collect_use_stmts(&lines);

    let mut rewritten = 0usize;
    let mut unresolved: Vec<String> = Vec::new();
    // line index -> replacement lines (empty vec = drop the span's lines).
    let mut replacements: Vec<(usize, usize, Vec<String>)> = Vec::new();

    let src_seg = format!("::{src_mod}");
    for st in &use_stmts {
        let t = st.text.trim();
        let lead = if t.starts_with("pub use ") {
            "pub use "
        } else {
            "use "
        };
        let body = t
            .trim_start_matches(lead)
            .trim_end_matches(';')
            .trim()
            .to_string();

        // Nested groups: more than one `{` — honest refusal when the symbol may
        // be involved, silence otherwise (the statement cannot concern the move).
        if body.matches('{').count() > 1 {
            if body.contains(src_mod) && body.contains(symbol) {
                unresolved.push(format!(
                    "{file_label}:{}: nested-group import `{}` mentions `{src_mod}`/`{symbol}` — split it manually, the spike does not rewrite nested groups",
                    st.start + 1,
                    st.text.trim()
                ));
            }
            continue;
        }

        // Grouped import of the source module?
        if let Some(brace) = body.find('{') {
            let prefix = body[..brace].trim_end_matches("::").trim();
            let inner = body[brace + 1..].trim_end_matches('}').trim();
            let prefix_is_src = prefix == src_mod || prefix.ends_with(&src_seg);
            if !prefix_is_src {
                continue;
            }
            let members = split_group_members(inner);
            let Some(sym_member) = members
                .iter()
                .find(|m| member_source_name(m) == symbol)
                .cloned()
            else {
                continue;
            };
            let rest: Vec<String> = members
                .into_iter()
                .filter(|m| member_source_name(m) != symbol)
                .collect();
            let dest_prefix = if prefix == src_mod {
                dest_mod.to_string()
            } else {
                format!(
                    "{}{}",
                    prefix.strip_suffix(src_mod).unwrap_or(prefix),
                    dest_mod
                )
            };
            let mut new_lines: Vec<String> = Vec::new();
            match rest.len() {
                0 => {}
                1 => new_lines.push(format!("{lead}{prefix}::{};", rest[0])),
                _ => new_lines.push(format!("{lead}{prefix}::{{{}}};", rest.join(", "))),
            }
            if !symbol_is_local {
                new_lines.push(format!("{lead}{dest_prefix}::{sym_member};"));
            }
            replacements.push((st.start, st.end, new_lines));
            rewritten += 1;
            continue;
        }

        // Glob import of the source module?
        if body.ends_with("::*") {
            let prefix = body.trim_end_matches("::*").trim();
            let prefix_is_src = prefix == src_mod || prefix.ends_with(&src_seg);
            if !prefix_is_src {
                continue;
            }
            if !uses_symbol_bare(text, symbol) {
                continue; // glob present but the symbol is not used bare here
            }
            if symbol_is_local {
                continue; // dest: the symbol is local now, the glob needs no help
            }
            let dest_prefix = if prefix == src_mod {
                dest_mod.to_string()
            } else {
                format!(
                    "{}{}",
                    prefix.strip_suffix(src_mod).unwrap_or(prefix),
                    dest_mod
                )
            };
            let glob_line = lines[st.start..=st.end].join(" ");
            replacements.push((
                st.start,
                st.end,
                vec![
                    glob_line.trim_end().to_string(),
                    format!("use {dest_prefix}::{symbol};"),
                ],
            ));
            unresolved.push(format!(
                "{file_label}:{}: glob import `{}` covered `{symbol}` — an explicit `use {dest_prefix}::{symbol};` was added alongside (review whether the glob still earns its keep)",
                st.start + 1,
                st.text.trim()
            ));
            rewritten += 1;
            continue;
        }
    }

    // Apply span replacements bottom-up so indices stay valid.
    let mut new_lines = lines.clone();
    replacements.sort_by_key(|(s, _, _)| *s);
    for (s, e, repl) in replacements.into_iter().rev() {
        new_lines.splice(s..=e, repl);
    }

    // Generic qualified-path rewrite on every remaining line (covers direct
    // imports `use …::<src_mod>::<symbol> [as alias];` and code paths alike).
    // When the symbol is LOCAL (dest file), a direct import of it is DROPPED
    // instead of re-pointed — `use crate::<dest_mod>::<symbol>;` inside the very
    // module that now defines it is E0255.
    let mut drop_idx: Vec<usize> = Vec::new();
    for (i, l) in new_lines.iter_mut().enumerate() {
        let n = count_qualified(l, src_mod, symbol);
        if n > 0 {
            let t = l.trim_start();
            if symbol_is_local && (t.starts_with("use ") || t.starts_with("pub use ")) {
                drop_idx.push(i);
                rewritten += n;
                continue;
            }
            *l = replace_qualified(l, src_mod, dest_mod, symbol);
            rewritten += n;
        }
    }
    let new_text_lines: Vec<String> = new_lines
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !drop_idx.contains(i))
        .map(|(_, l)| l)
        .collect();

    RefRewrite {
        new_text: join_lines(&new_text_lines, trailing_nl),
        rewritten,
        unresolved,
    }
}

// ===========================================================================
// A3 — protected zones (the house Money-Zone law)
// ===========================================================================

/// One `ci/protected-zones.json` rule: a glob and the reason it is guarded.
#[derive(serde::Deserialize)]
struct ProtectedZoneRule {
    glob: String,
    #[serde(default)]
    reason: String,
}

/// The `ci/protected-zones.json` shape: `{"zones":[{"glob":"…","reason":"…"}]}`
/// (the house Money-Zone convention — a repo-level `ci/protected-zones.json`).
#[derive(serde::Deserialize)]
struct ProtectedZonesFile {
    #[serde(default)]
    zones: Vec<ProtectedZoneRule>,
}

/// Load `ci/protected-zones.json` by searching UPWARD from `anchor_abs` (mirrors
/// [`crate_root`]'s ascent), so a workspace's repo-level guard is found from any
/// crate. `Ok(None)` when no zone file exists (no interference). A present-but-
/// invalid config (unreadable, malformed JSON, or a bad glob) FAILS CLOSED with
/// `Err` — a money-zone guard that cannot be evaluated must never silently pass.
#[allow(clippy::type_complexity)]
fn load_protected_zones(
    anchor_abs: &str,
) -> Result<Option<Vec<(glob::Pattern, ProtectedZoneRule)>>, String> {
    let mut cur = Path::new(anchor_abs).parent();
    while let Some(dir) = cur {
        let candidate = dir.join("ci").join("protected-zones.json");
        if candidate.is_file() {
            let raw = std::fs::read_to_string(&candidate)
                .map_err(|e| format!("cannot read {}: {e}", candidate.display()))?;
            let parsed: ProtectedZonesFile = serde_json::from_str(&raw)
                .map_err(|e| format!("malformed {}: {e}", candidate.display()))?;
            let mut compiled = Vec::with_capacity(parsed.zones.len());
            for z in parsed.zones {
                let pat = glob::Pattern::new(&z.glob).map_err(|e| {
                    format!(
                        "invalid zone glob `{}` in {}: {e}",
                        z.glob,
                        candidate.display()
                    )
                })?;
                compiled.push((pat, z));
            }
            return Ok(Some(compiled));
        }
        cur = dir.parent();
    }
    Ok(None)
}

/// True when `file_abs` (usually absolute) falls under the repo-relative zone
/// `pat`. Matches the full normalized path and each path-component tail, so a
/// repo-relative glob (`src/money/**`) matches an absolute touched path by its tail.
fn zone_matches(pat: &glob::Pattern, file_abs: &str) -> bool {
    let norm = normalize(file_abs);
    if pat.matches(&norm) {
        return true;
    }
    let segs: Vec<&str> = norm.split('/').filter(|s| !s.is_empty()).collect();
    (0..segs.len()).any(|i| pat.matches(&segs[i..].join("/")))
}

/// A3 — enforce the Money-Zone gate over the full touched set (source + dest +
/// every derived referencer). When a touched path matches a guarded zone, refuse
/// (teaching the gesture) UNLESS `allow_protected` carries the caller's reason — in
/// which case return the recorded [`ProtectedZoneGesture`] for the receipt. No zone
/// file, or no match, returns `Ok(None)` (no interference). A broken config fails
/// closed. Pure — the caller invokes it in the pre-write preflight, so a refusal is
/// byte-identity-safe.
fn enforce_protected_zones(
    source_abs: &str,
    dest_abs: &str,
    referencers: &[String],
    allow_protected: Option<&str>,
) -> M1ndResult<Option<ProtectedZoneGesture>> {
    let zones = match load_protected_zones(source_abs) {
        Ok(None) => return Ok(None),
        Ok(Some(z)) => z,
        Err(detail) => {
            return Err(refuse(format!(
                "protected-zone guard is armed but its config is invalid: {detail}. Refusing to teach until ci/protected-zones.json is valid (the Money-Zone fails CLOSED). No file is touched."
            )));
        }
    };
    if zones.is_empty() {
        return Ok(None);
    }
    // Deterministic order: source, dest, then referencers (sorted for stability).
    let mut refs: Vec<&str> = referencers.iter().map(String::as_str).collect();
    refs.sort();
    let mut touched: Vec<&str> = vec![source_abs, dest_abs];
    touched.extend(refs);
    for f in touched {
        for (pat, rule) in &zones {
            if zone_matches(pat, f) {
                return match allow_protected {
                    Some(gesture) if !gesture.trim().is_empty() => Ok(Some(ProtectedZoneGesture {
                        zone: rule.glob.clone(),
                        zone_reason: rule.reason.clone(),
                        matched_file: f.to_string(),
                        gesture: gesture.to_string(),
                    })),
                    _ => Err(refuse(format!(
                        "protected zone: '{f}' matches the guarded zone `{}` ({}) — this transplant crosses the Money-Zone. Refusing without the explicit gesture: resend with `allow_protected` set to your reason for the crossing (e.g. \"allow_protected\":\"<why this move is safe>\"). No file is touched.",
                        rule.glob,
                        if rule.reason.is_empty() {
                            "no reason given"
                        } else {
                            &rule.reason
                        }
                    ))),
                };
            }
        }
    }
    Ok(None)
}

// ===========================================================================
// Path helpers
// ===========================================================================

fn module_name(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

/// The nearest ancestor directory of `file` that directly contains a `Cargo.toml`
/// — the crate root — canonicalized for a stable cross-platform comparison.
/// `None` when no ancestor holds a `Cargo.toml` (an unrooted fixture); the A7
/// cross-crate guard treats a `None` on either side as "cannot prove cross-crate"
/// and does NOT refuse on that basis.
fn crate_root(file: &str) -> Option<String> {
    let mut cur = Path::new(file).parent();
    while let Some(dir) = cur {
        if dir.join("Cargo.toml").is_file() {
            let canon = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
            return Some(normalize(&canon.to_string_lossy()));
        }
        cur = dir.parent();
    }
    None
}

fn paths_equal(a: &str, b: &str) -> bool {
    normalize(a) == normalize(b)
}

fn normalize(p: &str) -> String {
    p.replace('\\', "/")
}

/// Suffix match between a graph-stored (often repo-relative) path and a caller
/// (often absolute) path — mirrors `surgical_handlers::surgical_paths_match`.
fn file_matches(stored: &str, other: &str) -> bool {
    let a = normalize(stored.strip_prefix("file::").unwrap_or(stored));
    let b = normalize(other.strip_prefix("file::").unwrap_or(other));
    a == b || a.ends_with(&b) || b.ends_with(&a)
}

fn f_display(file: &str) -> String {
    file.strip_prefix("file::").unwrap_or(file).to_string()
}

/// Resolve a possibly-relative graph file path against the directory of an
/// absolute sibling (source_abs), so we can read referencing files from disk.
fn resolve_sibling(anchor_abs: &str, stored: &str) -> String {
    let stored = stored.strip_prefix("file::").unwrap_or(stored);
    if Path::new(stored).is_absolute() {
        return stored.to_string();
    }
    // stored like "src/gamma.rs"; anchor like "/tmp/x/src/alpha.rs".
    // Find the common suffix boundary: strip the anchor down to the repo root that
    // makes `root/stored` exist.
    let anchor = Path::new(anchor_abs);
    let mut cur = anchor.parent();
    while let Some(dir) = cur {
        let candidate = dir.join(stored);
        if candidate.exists() {
            return candidate.to_string_lossy().to_string();
        }
        cur = dir.parent();
    }
    stored.to_string()
}

fn join_lines(lines: &[String], trailing_nl: bool) -> String {
    let mut s = lines.join("\n");
    if trailing_nl {
        s.push('\n');
    }
    s
}
