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
    ApplyBatchInput, BatchEditItem, SharedDepReport, TransplantInput, TransplantOutput,
};
use crate::session::SessionState;
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
    M1ndError::InvalidParams {
        tool: "transplant".into(),
        detail,
    }
}

pub fn handle_transplant(
    state: &mut SessionState,
    input: TransplantInput,
) -> M1ndResult<TransplantOutput> {
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

    // --- Phase 0.5: dest-collision preflight on DISK TRUTH --------------------
    // The graph can be STALE here (the battery poisons dest AFTER ingest, without
    // re-ingesting) — disk is the only sound source. A collision writes NOTHING.
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
    let (travelled_names, shared_names, dependency_source) = classify_dependencies(
        &view,
        &samefile_fns,
        symbol_idx,
        &symbol,
        &source_lines,
        decl_idx,
    );

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
        let (top, brace_end, trailing_end) = item_region(&source_lines, d);
        cut_ranges.push((top, trailing_end));
        moved_texts.push(source_lines[top..=brace_end].to_vec());
    }

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

    // Self-use: if the source still calls the moved symbol after the cut, it needs
    // a back-import `use crate::<dest_module>::<symbol>;` to keep compiling.
    let source_back_imported = new_source_lines.iter().any(|l| references_call(l, &symbol));
    if source_back_imported {
        let at = header_end(&new_source_lines);
        new_source_lines.insert(at, format!("use crate::{dest_module}::{symbol};"));
    }
    let new_source = join_lines(&new_source_lines, source_trailing_nl);

    // --- Phase 6: build the new DEST content ----------------------------------
    let dest_lines: Vec<String> = dest_text.lines().map(str::to_string).collect();
    let dest_trailing_nl = dest_text.ends_with('\n');
    let insert_at = header_end(&dest_lines);
    let mut block: Vec<String> = Vec::new();
    for u in &dest_use_lines {
        block.push(u.clone());
    }
    if !dest_use_lines.is_empty() {
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
    let new_dest = join_lines(&new_dest_lines, dest_trailing_nl);

    // --- Phase 7: referencer discovery (graph edges + textual cross-check) -----
    let (referencing_files, referencer_source) = discover_referencers(
        &view,
        symbol_idx,
        &source_abs,
        &dest_abs,
        &source_module,
        &symbol,
    );

    let mut edits: Vec<BatchEditItem> = vec![
        BatchEditItem {
            file_path: source_abs.clone(),
            new_content: new_source,
            description: Some(format!("transplant: remove `{symbol}` (+ travelled deps)")),
        },
        BatchEditItem {
            file_path: dest_abs.clone(),
            new_content: new_dest,
            description: Some(format!("transplant: receive `{symbol}`")),
        },
    ];

    let mut refs_rewritten = 0usize;
    let mut refs_unresolved: Vec<String> = Vec::new();
    let mut files_changed: Vec<String> = vec![source_abs.clone(), dest_abs.clone()];
    for rf in &referencing_files {
        let text = match std::fs::read_to_string(rf) {
            Ok(t) => t,
            Err(e) => {
                refs_unresolved.push(format!("{rf}: unreadable ({e})"));
                continue;
            }
        };
        let (rewritten, count, unresolved) =
            rewrite_referencer(&text, &source_module, &dest_module, &symbol);
        if count == 0 {
            // The graph pointed here but no textual site was rewritable — surface
            // it honestly instead of pretending the reference was handled.
            refs_unresolved.push(format!(
                "{rf}: referenced `{symbol}` per the graph but no rewritable `{source_module}::{symbol}` site was found (grouped/aliased use, or macro?)"
            ));
            continue;
        }
        refs_rewritten += count;
        refs_unresolved.extend(unresolved);
        files_changed.push(rf.clone());
        edits.push(BatchEditItem {
            file_path: rf.clone(),
            new_content: rewritten,
            description: Some(format!(
                "transplant: re-point `{symbol}` to `{dest_module}`"
            )),
        });
    }

    // --- Phase 8: ATOMIC write through the existing apply_batch machinery ------
    let batch = ApplyBatchInput {
        agent_id: input.agent_id.clone(),
        edits,
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

    Ok(TransplantOutput {
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
        dependency_source: if hint_worked {
            dependency_source
        } else {
            format!("{dependency_source} (line-hint missed; decl located textually)")
        },
        referencer_source,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
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
    let mut node_file = vec![String::new(); n];
    for i in 0..n {
        let nt = graph.nodes.node_type[i];
        let prov = &graph.nodes.provenance[i];
        let file = prov
            .source_path
            .and_then(|s| graph.strings.try_resolve(s))
            .unwrap_or("")
            .to_string();
        node_file[i] = file.clone();
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
            NodeType::File => {
                if !file.is_empty() && !file_paths.contains(&file) {
                    file_paths.push(file);
                }
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
fn classify_dependencies(
    view: &GraphView,
    samefile_fns: &[&FnNode],
    symbol_idx: Option<usize>,
    symbol: &str,
    source_lines: &[String],
    decl_idx: usize,
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
    let (travelled, shared) = classify_dependencies_textually(source_lines, symbol, decl_idx);
    (travelled, shared, "textual".to_string())
}

/// Textual trichotomy fallback: scan the moved fn body for `name(` calls to
/// sibling top-level fns, then a dep TRAVELS iff it has no other call site in the
/// file outside the moved region.
fn classify_dependencies_textually(
    source_lines: &[String],
    symbol: &str,
    decl_idx: usize,
) -> (Vec<String>, Vec<String>) {
    let all_fns = source_fn_names(&source_lines.join("\n"));
    let (top, brace_end, _) = item_region(source_lines, decl_idx);
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

    // Textual cross-check over the graph's known files.
    let needle = format!("{source_module}::{symbol}");
    let mut textual_files: BTreeSet<String> = BTreeSet::new();
    for f in &view.file_paths {
        if file_matches(f, source_abs) || file_matches(f, dest_abs) {
            continue;
        }
        // Resolve the repo-relative file path to something readable: prefer an
        // absolute sibling of source_abs when the stored path is relative.
        let abs = resolve_sibling(source_abs, f);
        if let Ok(text) = std::fs::read_to_string(&abs) {
            if text.contains(&needle) {
                textual_files.insert(abs);
            }
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
// Textual helpers (item region, fn detection, rewrites)
// ===========================================================================

/// Return `(first_trivia_line, closing_brace_line, last_trailing_blank_line)` for
/// the item whose declaration is on `decl_idx`. Widens UP over contiguous `///`
/// docs, `#[...]` attributes and `//` comments, and DOWN over trailing blanks.
fn item_region(lines: &[String], decl_idx: usize) -> (usize, usize, usize) {
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
    // find closing brace
    let brace_end = find_item_end(lines, decl_idx);
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

/// Rewrite every `<src_mod>::<symbol>` occurrence to `<dest_mod>::<symbol>`.
/// Returns `(new_text, sites_rewritten, unresolved_notes)`.
fn rewrite_referencer(
    text: &str,
    src_mod: &str,
    dest_mod: &str,
    symbol: &str,
) -> (String, usize, Vec<String>) {
    let from = format!("{src_mod}::{symbol}");
    let to = format!("{dest_mod}::{symbol}");
    let count = text.matches(&from).count();
    let new_text = text.replace(&from, &to);
    (new_text, count, Vec::new())
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
