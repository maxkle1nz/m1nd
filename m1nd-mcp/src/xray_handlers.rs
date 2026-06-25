// === m1nd-mcp/src/xray_handlers.rs ===
//! X-RAY write verb: `xray_retag` — one agent call that fans a tag mutation
//! across every node matching a selector, with a dry-run-by-default /
//! explicit-commit contract.
//!
//! The agent supplies a SELECTOR (any-match tags, exact node_type, external_id
//! path prefix) plus a TRANSFORM (add / remove / set a tag set). The tool plans
//! the change across all matches, returns a sample, and only mutates+persists
//! when `mode == "commit"`. Tags are a cold-path node column (see
//! `Graph::add_node_tags`), so the commit needs no graph rebuild — it reuses the
//! shipped columnar mutators and persists via the session's single save choke
//! point (`SessionState::persist`).

use crate::session::SessionState;
use m1nd_core::error::M1ndResult;
use m1nd_core::graph::Graph;
use m1nd_core::types::{NodeId, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Input / output types
// ---------------------------------------------------------------------------

/// Selector: a node is selected only if it satisfies *every* provided predicate
/// (predicate AND). Within `filter_tags`, an *any-match* is enough (tag OR) —
/// the same semantics as the layer-handler tag filter it mirrors. An empty
/// selector (no predicates) matches all nodes.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct XraySelector {
    /// Node matches if it carries at least one of these tags (any-match).
    #[serde(default)]
    pub filter_tags: Vec<String>,
    /// Exact node-type match, expressed as the canonical u8 (see
    /// `m1nd_core::snapshot` numbering: File=0, Function=2, …, Custom=100+v).
    #[serde(default)]
    pub node_type: Option<u8>,
    /// Node matches if its external_id starts with this prefix.
    #[serde(default)]
    pub path_prefix: Option<String>,
}

/// Tag transform to apply to each selected node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum XrayTagOp {
    /// Add `tags` (idempotent — already-present tags are no-ops).
    Add,
    /// Remove `tags` (absent tags are no-ops).
    Remove,
    /// Replace the node's entire tag set with `tags`.
    Set,
}

/// Execution mode. Defaults to [`XrayMode::DryRun`]: plan only, mutate nothing,
/// persist nothing. `commit` is the explicit opt-in to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum XrayMode {
    #[default]
    DryRun,
    Commit,
}

#[derive(Debug, Clone, Deserialize)]
pub struct XrayRetagInput {
    pub selector: XraySelector,
    pub op: XrayTagOp,
    pub tags: Vec<String>,
    #[serde(default)]
    pub mode: XrayMode,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct XrayCounts {
    /// Nodes the selector matched.
    pub selected: u32,
    /// Selected nodes whose tag set the op would change.
    pub planned: u32,
    /// Selected nodes the op would leave unchanged (e.g. add of a present tag).
    pub skipped_noop: u32,
    /// Reserved for cross-call OCC; always 0 in the single-lock path.
    pub conflicts: u32,
    /// Nodes actually mutated (0 on dry_run, == planned on commit).
    pub applied: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct XrayPlannedSample {
    pub id: String,
    pub before: Vec<String>,
    pub after: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct XrayRetagOutput {
    pub verb: &'static str,
    /// "dry_run" or "committed".
    pub status: String,
    pub counts: XrayCounts,
    /// First few planned changes (cap 5), for the agent to eyeball before commit.
    pub planned_sample: Vec<XrayPlannedSample>,
    /// First few conflict ids (cap 5). Empty in the single-lock path.
    pub conflicts_sample: Vec<String>,
}

const SAMPLE_CAP: usize = 5;

// ---------------------------------------------------------------------------
// node_type comparison
// ---------------------------------------------------------------------------

/// Canonical NodeType -> u8 (mirrors `m1nd_core::snapshot` numbering, including
/// the Custom(v) => 100 + v convention). Kept local because the core helper is
/// private; the mapping is stable and shared by the on-disk format.
fn node_type_to_u8(nt: NodeType) -> u8 {
    match nt {
        NodeType::File => 0,
        NodeType::Directory => 1,
        NodeType::Function => 2,
        NodeType::Class => 3,
        NodeType::Struct => 4,
        NodeType::Enum => 5,
        NodeType::Type => 6,
        NodeType::Module => 7,
        NodeType::Reference => 8,
        NodeType::Concept => 9,
        NodeType::Material => 10,
        NodeType::Process => 11,
        NodeType::Product => 12,
        NodeType::Supplier => 13,
        NodeType::Regulatory => 14,
        NodeType::System => 15,
        NodeType::Cost => 16,
        NodeType::Custom(v) => 100u8.saturating_add(v),
    }
}

// ---------------------------------------------------------------------------
// Pure core: selector + plan + (optional) apply against a Graph
// ---------------------------------------------------------------------------

/// Reverse map node index -> external_id (falls back to label, mirroring
/// `l5_build_node_to_ext_map`). Used for selector path_prefix and sample ids.
fn node_to_ext_map(graph: &Graph) -> Vec<String> {
    let n = graph.num_nodes() as usize;
    let mut map = vec![String::new(); n];
    for (&interned, &nid) in &graph.id_to_node {
        let idx = nid.as_usize();
        if idx < n {
            map[idx] = graph.strings.resolve(interned).to_string();
        }
    }
    for (i, entry) in map.iter_mut().enumerate().take(n) {
        if entry.is_empty() {
            *entry = graph.strings.resolve(graph.nodes.label[i]).to_string();
        }
    }
    map
}

/// Compute the tag set this op would produce, given the current set. Returns
/// `Some(after)` if it differs from `current`, `None` if it is a no-op.
fn plan_after(op: XrayTagOp, current: &[&str], tags: &[String]) -> Option<Vec<String>> {
    let cur: Vec<String> = current.iter().map(|s| s.to_string()).collect();
    let after: Vec<String> = match op {
        XrayTagOp::Add => {
            let mut next = cur.clone();
            for t in tags {
                if !next.iter().any(|c| c == t) {
                    next.push(t.clone());
                }
            }
            next
        }
        XrayTagOp::Remove => cur
            .iter()
            .filter(|c| !tags.iter().any(|t| t == *c))
            .cloned()
            .collect(),
        XrayTagOp::Set => tags.to_vec(),
    };
    if after == cur {
        None
    } else {
        Some(after)
    }
}

/// Resolve the selector to the matching node indices (a node must satisfy every
/// provided predicate).
fn select_nodes(graph: &Graph, selector: &XraySelector, ext: &[String]) -> Vec<usize> {
    let n = graph.num_nodes() as usize;
    (0..n)
        .filter(|&i| {
            // path_prefix: external_id starts_with
            if let Some(prefix) = &selector.path_prefix {
                if !ext[i].starts_with(prefix.as_str()) {
                    return false;
                }
            }
            // node_type: exact match on canonical u8
            if let Some(want) = selector.node_type {
                if node_type_to_u8(graph.nodes.node_type[i]) != want {
                    return false;
                }
            }
            // filter_tags: any-match
            if !selector.filter_tags.is_empty() {
                let node_tags = graph.node_tags(NodeId::new(i as u32));
                if !selector
                    .filter_tags
                    .iter()
                    .any(|want| node_tags.contains(&want.as_str()))
                {
                    return false;
                }
            }
            true
        })
        .collect()
}

/// Pure selector + plan + (commit-only) apply against a `Graph`. Unit-testable
/// without a `SessionState`. On `mode == DryRun` the graph is read only; on
/// `mode == Commit` the planned nodes are mutated in place via the shipped
/// columnar mutators. Persistence is the caller's job (the handler).
pub fn retag_graph(graph: &mut Graph, input: &XrayRetagInput) -> XrayRetagOutput {
    let ext = node_to_ext_map(graph);
    let selected = select_nodes(graph, &input.selector, &ext);

    let commit = input.mode == XrayMode::Commit;
    let mut counts = XrayCounts {
        selected: selected.len() as u32,
        ..Default::default()
    };
    let mut planned_sample: Vec<XrayPlannedSample> = Vec::new();

    // Plan first against an immutable view so the sample reflects the pre-state,
    // then (on commit) apply per planned node. The plan is deterministic, so a
    // second pass to apply yields exactly the planned set.
    let tag_refs: Vec<&str> = input.tags.iter().map(String::as_str).collect();

    for &idx in &selected {
        let nid = NodeId::new(idx as u32);
        let before = graph.node_tags(nid);
        match plan_after(input.op, &before, &input.tags) {
            Some(after) => {
                counts.planned += 1;
                if planned_sample.len() < SAMPLE_CAP {
                    planned_sample.push(XrayPlannedSample {
                        id: ext[idx].clone(),
                        before: before.iter().map(|s| s.to_string()).collect(),
                        after,
                    });
                }
                if commit {
                    match input.op {
                        XrayTagOp::Add => {
                            graph.add_node_tags(nid, &tag_refs);
                        }
                        XrayTagOp::Remove => {
                            graph.remove_node_tags(nid, &tag_refs);
                        }
                        XrayTagOp::Set => {
                            graph.set_node_tags(nid, &tag_refs);
                        }
                    }
                    counts.applied += 1;
                }
            }
            None => counts.skipped_noop += 1,
        }
    }

    // `conflicts` stays 0: under one write lock the plan and apply see the same
    // graph, so there is no optimistic-concurrency window. True cross-call OCC
    // (detect a tag set that changed between two xray_retag calls) is a later slice.
    XrayRetagOutput {
        verb: "xray_retag",
        status: if commit { "committed" } else { "dry_run" }.to_string(),
        counts,
        planned_sample,
        conflicts_sample: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// MCP handler for `xray_retag`. Holds the graph write lock only for the
/// plan/apply, drops it, then — on commit — persists through the session's
/// single save choke point (which honours read-only attach).
pub fn handle_xray_retag(
    state: &mut SessionState,
    input: XrayRetagInput,
) -> M1ndResult<serde_json::Value> {
    let output = {
        let mut graph = state.graph.write();
        retag_graph(&mut graph, &input)
    };

    if input.mode == XrayMode::Commit && output.counts.applied > 0 {
        // Persist via the session choke point: graph is source of truth, the
        // call is a no-op in read-only attach, and it writes to the canonical
        // graph_path. Not added to PROOF_GATED_WRITE_TOOLS on purpose — this
        // mutates graph metadata (tags), not agent-supplied source files.
        state.persist()?;
    }

    serde_json::to_value(output).map_err(m1nd_core::error::M1ndError::Serde)
}

// ===========================================================================
// X-RAY read verb: `xray_orient` — structural conformance LEDGER (read-only)
// ===========================================================================
// One agent call computes a conformance ledger over the *live* graph: it
// derives each node's MODULE from its external_id, walks the boundary edges
// (`imports` / `depends_on`), builds a cross-module dependency matrix, and
// classifies each cross-module edge against a MANIFESTO of layer rules into
// convergence vs divergence (EROSION candidates). It also runs an existence
// axis: each `require_exists` substring is present (BEDROCK) or absent
// (BLUEPRINT) in the live external_ids.
//
// HONESTY (proof-grown): divergences are `erosion_candidates`, NEVER confirmed
// violations. The verb reports; it does not over-claim. With an empty manifest
// it just reports the matrix + module census — the instrument is "not aimed
// yet", so the candidate list is empty by construction.
//
// Read-only: takes the graph *read* lock, never mutates, never persists. Safe
// in read-only attach (hence NOT in `READ_ONLY_DENIED_TOOLS`).

const EROSION_CAP: usize = 25;
const FILE_PREFIX: &str = "file::";

/// A north-star layer/forbid ruleset the agent supplies. Empty by default — an
/// empty manifest yields an empty `erosion_candidates` list (honest: the
/// instrument is not aimed yet, we only report structure).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct XrayManifest {
    /// Hard "module A must not depend on module B" pairs.
    #[serde(default)]
    pub forbid: Vec<(String, String)>,
    /// Modules ordered low -> high. A module may depend only on modules at its
    /// own level or LOWER; depending on a *higher* layer is a candidate
    /// divergence. Modules absent from this list are unconstrained by the
    /// layer axis (only the `forbid` axis applies to them).
    #[serde(default)]
    pub layer_order: Vec<String>,
    /// Existence intents: each substring MUST appear in some node's external_id
    /// (present => BEDROCK, absent => BLUEPRINT).
    #[serde(default)]
    pub require_exists: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct XrayOrientInput {
    /// Optional external_id path-prefix filter. Only nodes whose external_id
    /// starts with this prefix are counted, and only edges whose *source* node
    /// is in scope contribute to the matrix.
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub manifest: XrayManifest,
}

#[derive(Debug, Clone, Serialize)]
pub struct XrayErosionCandidate {
    pub from_module: String,
    pub to_module: String,
    /// Which rule flagged it: `forbid` or `layer`.
    pub rule: &'static str,
    /// The boundary relation that carried the edge (`imports` / `depends_on`).
    pub via: String,
    /// Source node external_id (file:: prefix stripped for readability).
    pub from: String,
    /// Target node external_id (file:: prefix stripped for readability).
    pub to: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct XrayExistence {
    pub require: String,
    /// `BEDROCK` (substring present in some external_id) or `BLUEPRINT` (absent).
    pub state: &'static str,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct XrayOrientCounts {
    pub modules: u32,
    /// Cross-module boundary edges counted into the matrix.
    pub boundary_edges: u32,
    pub erosion_candidates: u32,
    pub blueprint: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct XrayOrientOutput {
    pub verb: &'static str,
    pub scope: Option<String>,
    /// module -> node_count (code-node census by module).
    pub modules: BTreeMap<String, u32>,
    /// "A->B" -> count of cross-module boundary edges.
    pub dependency_matrix: BTreeMap<String, u32>,
    /// Cross-module edges that diverge from the manifesto (cap `EROSION_CAP`).
    pub erosion_candidates: Vec<XrayErosionCandidate>,
    pub existence: Vec<XrayExistence>,
    pub counts: XrayOrientCounts,
}

// ---------------------------------------------------------------------------
// Module derivation
// ---------------------------------------------------------------------------

/// Module of a node = first path segment of its external_id after the `file::`
/// prefix (e.g. `file::m1nd-core/src/x.rs::fn::y` -> `m1nd-core`). Non-`file::`
/// ids (and an empty first segment) yield `None` ("unmapped" — skipped).
fn module_of(external_id: &str) -> Option<&str> {
    let rest = external_id.strip_prefix(FILE_PREFIX)?;
    // Path is everything up to the first `::` type/kind separator.
    let path = rest.split("::").next().unwrap_or(rest);
    let seg = path.split('/').next().unwrap_or(path);
    if seg.is_empty() {
        None
    } else {
        Some(seg)
    }
}

/// Strip the `file::` prefix for compact, readable sample ids.
fn strip_file_prefix(id: &str) -> String {
    id.strip_prefix(FILE_PREFIX).unwrap_or(id).to_string()
}

// ---------------------------------------------------------------------------
// Pure core: conformance ledger over a finalized Graph (unit-testable, no
// SessionState). Walks the live CSR; classifies cross-module boundary edges.
// ---------------------------------------------------------------------------

/// Compute the conformance ledger. Pure over a finalized `Graph` — the CSR must
/// be populated (live server / post-`finalize()`).
pub fn orient_graph(graph: &Graph, input: &XrayOrientInput) -> XrayOrientOutput {
    let ext = node_to_ext_map(graph);
    let n = graph.num_nodes() as usize;
    let scope = input.scope.as_deref();

    let in_scope = |idx: usize| -> bool {
        scope.is_none_or(|p| ext.get(idx).is_some_and(|e| e.starts_with(p)))
    };

    // --- module census (in-scope, mappable code nodes only) ---
    let mut modules: BTreeMap<String, u32> = BTreeMap::new();
    for (i, id) in ext.iter().enumerate().take(n) {
        if !in_scope(i) {
            continue;
        }
        if let Some(m) = module_of(id) {
            *modules.entry(m.to_string()).or_insert(0) += 1;
        }
    }

    // --- manifesto lookups ---
    let forbid: std::collections::HashSet<(&str, &str)> = input
        .manifest
        .forbid
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let layer_index =
        |m: &str| -> Option<usize> { input.manifest.layer_order.iter().position(|x| x == m) };

    // A cross-module edge A->B diverges if `forbid` contains (A,B) OR both are
    // in `layer_order` and B sits at a *higher* layer than A. Returns the rule
    // name that flagged it, or None if it converges.
    let classify = |a: &str, b: &str| -> Option<&'static str> {
        if forbid.contains(&(a, b)) {
            return Some("forbid");
        }
        if let (Some(ia), Some(ib)) = (layer_index(a), layer_index(b)) {
            if ib > ia {
                return Some("layer");
            }
        }
        None
    };

    // --- boundary edge walk over the live CSR ---
    let mut dependency_matrix: BTreeMap<String, u32> = BTreeMap::new();
    let mut erosion_candidates: Vec<XrayErosionCandidate> = Vec::new();
    let mut boundary_edges: u32 = 0;
    let mut erosion_total: u32 = 0;

    for i in 0..n {
        if !in_scope(i) {
            continue;
        }
        let src_mod = match module_of(&ext[i]) {
            Some(m) => m,
            None => continue,
        };
        for e in graph.csr.out_range(NodeId::new(i as u32)) {
            let rel = graph.strings.resolve(graph.csr.relations[e]);
            if rel != "imports" && rel != "depends_on" {
                continue;
            }
            let dst = graph.csr.targets[e].as_usize();
            let dst_id = match ext.get(dst) {
                Some(d) => d.as_str(),
                None => continue,
            };
            let dst_mod = match module_of(dst_id) {
                Some(m) => m,
                None => continue,
            };
            if src_mod == dst_mod {
                continue; // intra-module edges are ignored
            }
            boundary_edges += 1;
            *dependency_matrix
                .entry(format!("{src_mod}->{dst_mod}"))
                .or_insert(0) += 1;

            if let Some(rule) = classify(src_mod, dst_mod) {
                erosion_total += 1;
                if erosion_candidates.len() < EROSION_CAP {
                    erosion_candidates.push(XrayErosionCandidate {
                        from_module: src_mod.to_string(),
                        to_module: dst_mod.to_string(),
                        rule,
                        via: rel.to_string(),
                        from: strip_file_prefix(&ext[i]),
                        to: strip_file_prefix(dst_id),
                    });
                }
            }
        }
    }

    // --- existence axis: BEDROCK (present) vs BLUEPRINT (absent) ---
    // Match against in-scope external_ids only, so `scope` narrows existence too.
    let haystack: Vec<&str> = ext
        .iter()
        .enumerate()
        .take(n)
        .filter(|&(i, _)| in_scope(i))
        .map(|(_, id)| id.as_str())
        .collect();
    let mut existence: Vec<XrayExistence> = Vec::new();
    let mut blueprint: u32 = 0;
    for need in &input.manifest.require_exists {
        let present = haystack.iter().any(|id| id.contains(need.as_str()));
        if !present {
            blueprint += 1;
        }
        existence.push(XrayExistence {
            require: need.clone(),
            state: if present { "BEDROCK" } else { "BLUEPRINT" },
        });
    }

    XrayOrientOutput {
        verb: "xray_orient",
        scope: input.scope.clone(),
        counts: XrayOrientCounts {
            modules: modules.len() as u32,
            boundary_edges,
            erosion_candidates: erosion_total,
            blueprint,
        },
        modules,
        dependency_matrix,
        erosion_candidates,
        existence,
    }
}

/// MCP handler for `xray_orient`. Read-only: holds the graph *read* lock for the
/// computation, never mutates, never persists (safe under read-only attach).
pub fn handle_xray_orient(
    state: &mut SessionState,
    input: XrayOrientInput,
) -> M1ndResult<serde_json::Value> {
    let output = {
        let graph = state.graph.read();
        orient_graph(&graph, &input)
    };
    serde_json::to_value(output).map_err(m1nd_core::error::M1ndError::Serde)
}

// ---------------------------------------------------------------------------
// Tests (pure logic against a Graph — no SessionState needed)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use m1nd_core::graph::Graph;
    use m1nd_core::types::NodeType;

    fn sample_graph() -> Graph {
        let mut g = Graph::new();
        g.add_node(
            "file::a.rs::fn::foo",
            "foo",
            NodeType::Function,
            &["rust", "rust:visibility:private"],
            0.0,
            0.0,
        )
        .unwrap();
        g.add_node(
            "file::b.rs::fn::bar",
            "bar",
            NodeType::Function,
            &["rust", "rust:visibility:pub"],
            0.0,
            0.0,
        )
        .unwrap();
        g.add_node(
            "file::a.rs::struct::Cfg",
            "Cfg",
            NodeType::Struct,
            &["rust"],
            0.0,
            0.0,
        )
        .unwrap();
        g.finalize().unwrap();
        g
    }

    fn input(
        selector: XraySelector,
        op: XrayTagOp,
        tags: &[&str],
        mode: XrayMode,
    ) -> XrayRetagInput {
        XrayRetagInput {
            selector,
            op,
            tags: tags.iter().map(|s| s.to_string()).collect(),
            mode,
        }
    }

    #[test]
    fn dry_run_selects_and_plans_but_mutates_nothing() {
        let mut g = sample_graph();
        let sel = XraySelector {
            filter_tags: vec!["rust".to_string()],
            ..Default::default()
        };
        let out = retag_graph(
            &mut g,
            &input(sel, XrayTagOp::Add, &["xray:bedrock"], XrayMode::DryRun),
        );

        assert_eq!(out.status, "dry_run");
        assert_eq!(out.counts.selected, 3);
        assert_eq!(out.counts.planned, 3);
        assert_eq!(out.counts.applied, 0);
        assert!(!out.planned_sample.is_empty());

        // Nothing was actually written.
        let n = g.resolve_id("file::a.rs::fn::foo").unwrap();
        assert!(!g.node_tags(n).contains(&"xray:bedrock"));
    }

    #[test]
    fn commit_applies_and_node_tags_reflect_it() {
        let mut g = sample_graph();
        let sel = XraySelector {
            filter_tags: vec!["rust".to_string()],
            ..Default::default()
        };
        let out = retag_graph(
            &mut g,
            &input(sel, XrayTagOp::Add, &["xray:bedrock"], XrayMode::Commit),
        );

        assert_eq!(out.status, "committed");
        assert_eq!(out.counts.planned, 3);
        assert_eq!(out.counts.applied, 3);

        for ext in [
            "file::a.rs::fn::foo",
            "file::b.rs::fn::bar",
            "file::a.rs::struct::Cfg",
        ] {
            let n = g.resolve_id(ext).unwrap();
            assert!(g.node_tags(n).contains(&"xray:bedrock"), "{ext} not tagged");
        }
    }

    #[test]
    fn idempotent_second_commit_plans_zero() {
        let mut g = sample_graph();
        let sel = XraySelector {
            filter_tags: vec!["rust".to_string()],
            ..Default::default()
        };
        let first = retag_graph(
            &mut g,
            &input(
                sel.clone(),
                XrayTagOp::Add,
                &["xray:bedrock"],
                XrayMode::Commit,
            ),
        );
        assert_eq!(first.counts.applied, 3);

        let second = retag_graph(
            &mut g,
            &input(sel, XrayTagOp::Add, &["xray:bedrock"], XrayMode::Commit),
        );
        // Already present everywhere -> nothing planned, nothing applied.
        assert_eq!(second.counts.selected, 3);
        assert_eq!(second.counts.planned, 0);
        assert_eq!(second.counts.skipped_noop, 3);
        assert_eq!(second.counts.applied, 0);
    }

    #[test]
    fn remove_op_only_counts_present_tags() {
        let mut g = sample_graph();
        let sel = XraySelector {
            filter_tags: vec!["rust:visibility:pub".to_string()],
            ..Default::default()
        };
        let out = retag_graph(
            &mut g,
            &input(
                sel,
                XrayTagOp::Remove,
                &["rust:visibility:pub"],
                XrayMode::Commit,
            ),
        );
        // Only bar has the pub tag.
        assert_eq!(out.counts.selected, 1);
        assert_eq!(out.counts.planned, 1);
        assert_eq!(out.counts.applied, 1);

        let n = g.resolve_id("file::b.rs::fn::bar").unwrap();
        assert!(!g.node_tags(n).contains(&"rust:visibility:pub"));
    }

    #[test]
    fn selector_by_path_prefix_scopes_the_mutation() {
        let mut g = sample_graph();
        let sel = XraySelector {
            path_prefix: Some("file::a.rs".to_string()),
            ..Default::default()
        };
        let out = retag_graph(
            &mut g,
            &input(sel, XrayTagOp::Add, &["xray:scoped"], XrayMode::Commit),
        );
        // Two nodes live under file::a.rs (foo + Cfg); bar is under file::b.rs.
        assert_eq!(out.counts.selected, 2);
        assert_eq!(out.counts.applied, 2);

        assert!(g
            .node_tags(g.resolve_id("file::a.rs::fn::foo").unwrap())
            .contains(&"xray:scoped"));
        assert!(g
            .node_tags(g.resolve_id("file::a.rs::struct::Cfg").unwrap())
            .contains(&"xray:scoped"));
        assert!(!g
            .node_tags(g.resolve_id("file::b.rs::fn::bar").unwrap())
            .contains(&"xray:scoped"));
    }

    #[test]
    fn selector_by_node_type_filters_exactly() {
        let mut g = sample_graph();
        // Struct == 4 in the canonical numbering.
        let sel = XraySelector {
            node_type: Some(node_type_to_u8(NodeType::Struct)),
            ..Default::default()
        };
        let out = retag_graph(
            &mut g,
            &input(sel, XrayTagOp::Add, &["xray:struct"], XrayMode::Commit),
        );
        assert_eq!(out.counts.selected, 1);
        assert_eq!(out.counts.applied, 1);
        assert!(g
            .node_tags(g.resolve_id("file::a.rs::struct::Cfg").unwrap())
            .contains(&"xray:struct"));
    }

    #[test]
    fn set_op_replaces_whole_tag_set() {
        let mut g = sample_graph();
        let sel = XraySelector {
            path_prefix: Some("file::a.rs::fn::foo".to_string()),
            ..Default::default()
        };
        let out = retag_graph(
            &mut g,
            &input(sel, XrayTagOp::Set, &["only", "these"], XrayMode::Commit),
        );
        assert_eq!(out.counts.applied, 1);
        let mut tags = g.node_tags(g.resolve_id("file::a.rs::fn::foo").unwrap());
        tags.sort_unstable();
        assert_eq!(tags, vec!["only", "these"]);
    }

    // -----------------------------------------------------------------------
    // xray_orient — structural conformance ledger (read-only)
    // -----------------------------------------------------------------------

    use m1nd_core::types::{EdgeDirection, FiniteF32};

    /// Two modules (modA, modB) with a single cross-module `imports` edge
    /// modA -> modB, plus one intra-module edge inside modA. Finalized so the
    /// CSR is populated.
    fn orient_graph_fixture() -> Graph {
        let mut g = Graph::new();
        g.add_node(
            "file::modA/src/lib.rs::fn::a_main",
            "a_main",
            NodeType::Function,
            &["rust"],
            0.0,
            0.0,
        )
        .unwrap(); // 0
        g.add_node(
            "file::modA/src/util.rs::fn::a_util",
            "a_util",
            NodeType::Function,
            &["rust"],
            0.0,
            0.0,
        )
        .unwrap(); // 1
        g.add_node(
            "file::modB/src/lib.rs::fn::b_core",
            "b_core",
            NodeType::Function,
            &["rust"],
            0.0,
            0.0,
        )
        .unwrap(); // 2

        // cross-module boundary edge: modA -> modB
        g.add_edge(
            NodeId::new(0),
            NodeId::new(2),
            "imports",
            FiniteF32::new(1.0),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.0),
        )
        .unwrap();
        // intra-module edge inside modA (must be ignored by the matrix)
        g.add_edge(
            NodeId::new(0),
            NodeId::new(1),
            "imports",
            FiniteF32::new(1.0),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.0),
        )
        .unwrap();
        g.finalize().unwrap();
        g
    }

    fn orient_input(manifest: XrayManifest) -> XrayOrientInput {
        XrayOrientInput {
            scope: None,
            manifest,
        }
    }

    #[test]
    fn module_of_derives_first_path_segment() {
        assert_eq!(
            module_of("file::m1nd-core/src/x.rs::fn::y"),
            Some("m1nd-core")
        );
        assert_eq!(module_of("file::modB/src/lib.rs::fn::b"), Some("modB"));
        // non-file:: id -> unmapped
        assert_eq!(module_of("concept::foo"), None);
        assert_eq!(module_of("plain-label"), None);
    }

    #[test]
    fn empty_manifest_reports_matrix_with_zero_erosion() {
        let g = orient_graph_fixture();
        let out = orient_graph(&g, &orient_input(XrayManifest::default()));

        assert_eq!(out.verb, "xray_orient");
        // census: two modules, modA has 2 nodes, modB has 1
        assert_eq!(out.modules.get("modA"), Some(&2));
        assert_eq!(out.modules.get("modB"), Some(&1));
        assert_eq!(out.counts.modules, 2);
        // only the cross-module edge counts; intra-module modA->modA is ignored
        assert_eq!(out.dependency_matrix.get("modA->modB"), Some(&1));
        assert_eq!(out.dependency_matrix.len(), 1);
        assert_eq!(out.counts.boundary_edges, 1);
        // honest: instrument not aimed -> no candidates
        assert!(out.erosion_candidates.is_empty());
        assert_eq!(out.counts.erosion_candidates, 0);
    }

    #[test]
    fn forbid_rule_flags_one_erosion_candidate() {
        let g = orient_graph_fixture();
        let manifest = XrayManifest {
            forbid: vec![("modA".to_string(), "modB".to_string())],
            ..Default::default()
        };
        let out = orient_graph(&g, &orient_input(manifest));

        assert_eq!(out.erosion_candidates.len(), 1);
        assert_eq!(out.counts.erosion_candidates, 1);
        let c = &out.erosion_candidates[0];
        assert_eq!(c.from_module, "modA");
        assert_eq!(c.to_module, "modB");
        assert_eq!(c.rule, "forbid");
        assert_eq!(c.via, "imports");
        assert_eq!(c.from, "modA/src/lib.rs::fn::a_main");
        assert_eq!(c.to, "modB/src/lib.rs::fn::b_core");
    }

    #[test]
    fn layer_order_flags_dependency_on_higher_layer() {
        let g = orient_graph_fixture();
        // modA below modB: modA depending on a HIGHER layer (modB) diverges.
        let manifest = XrayManifest {
            layer_order: vec!["modA".to_string(), "modB".to_string()],
            ..Default::default()
        };
        let out = orient_graph(&g, &orient_input(manifest));
        assert_eq!(out.counts.erosion_candidates, 1);
        assert_eq!(out.erosion_candidates[0].rule, "layer");

        // Reverse the order (modB below modA): modA -> modB is now downward
        // (allowed) -> converges, no candidate.
        let g2 = orient_graph_fixture();
        let manifest2 = XrayManifest {
            layer_order: vec!["modB".to_string(), "modA".to_string()],
            ..Default::default()
        };
        let out2 = orient_graph(&g2, &orient_input(manifest2));
        assert_eq!(out2.counts.erosion_candidates, 0);
        assert!(out2.erosion_candidates.is_empty());
    }

    #[test]
    fn require_exists_resolves_bedrock_vs_blueprint() {
        let g = orient_graph_fixture();
        let manifest = XrayManifest {
            require_exists: vec!["modA".to_string(), "nope_absent".to_string()],
            ..Default::default()
        };
        let out = orient_graph(&g, &orient_input(manifest));

        assert_eq!(out.existence.len(), 2);
        let bedrock = out.existence.iter().find(|e| e.require == "modA").unwrap();
        assert_eq!(bedrock.state, "BEDROCK");
        let blueprint = out
            .existence
            .iter()
            .find(|e| e.require == "nope_absent")
            .unwrap();
        assert_eq!(blueprint.state, "BLUEPRINT");
        assert_eq!(out.counts.blueprint, 1);
    }

    #[test]
    fn scope_narrows_census_and_matrix() {
        let g = orient_graph_fixture();
        // Scope to modA only: modB nodes drop out, the cross-module edge's
        // source is still in scope but the dependency_matrix still records the
        // edge (target module is derived from the edge, not scope-filtered).
        let input = XrayOrientInput {
            scope: Some("file::modA".to_string()),
            manifest: XrayManifest::default(),
        };
        let out = orient_graph(&g, &input);
        assert_eq!(out.modules.get("modA"), Some(&2));
        assert_eq!(out.modules.get("modB"), None);
        assert_eq!(out.counts.modules, 1);
        // modA's source node is in scope, so the modA->modB edge still counts
        assert_eq!(out.dependency_matrix.get("modA->modB"), Some(&1));
    }
}
