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
use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_core::graph::Graph;
use m1nd_core::types::{NodeId, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Append-only audit ledger
// ---------------------------------------------------------------------------
// Every successful X-RAY bulk WRITE (a `commit` that actually applied/painted
// ≥1 change in xray_retag / xray_paint / xray_apply) appends exactly ONE JSON
// line to a ledger file beside the runtime graph snapshot, so a write is
// traceable and manually reversible. The `xray_ledger` read verb plays it back.
//
// FAIL-SOFT: ledger I/O NEVER fails the underlying op. If the path can't be
// resolved or the append errors, the write still succeeds — the ledger is an
// audit convenience, not a correctness dependency. Best-effort, logs nothing.

/// File name of the append-only audit ledger, written beside the graph
/// snapshot (`<graph_path parent>/xray.ledger.jsonl`). `.jsonl` (not `.json`)
/// so it carries one self-contained JSON record per line.
const LEDGER_FILE_NAME: &str = "xray.ledger.jsonl";

/// Cap on the number of per-change entries embedded in one ledger record. A
/// bulk write touching more than this records only the first `LEDGER_CHANGES_CAP`
/// and adds a `changes_truncated` total so the line stays bounded.
const LEDGER_CHANGES_CAP: usize = 1000;

/// Resolve the ledger path beside the session's runtime graph snapshot:
/// `<graph_path parent>/xray.ledger.jsonl`. Returns `None` when the graph path
/// has no parent (the caller then SKIPS ledger writing — fail-soft).
fn ledger_path_for(state: &SessionState) -> Option<PathBuf> {
    state
        .graph_path
        .parent()
        .map(|dir| dir.join(LEDGER_FILE_NAME))
}

/// Count the lines already in the ledger file (0 if it doesn't exist or can't
/// be read). The next `seq` is this count + 1. Best-effort: any read error
/// yields 0 so the append still proceeds (seq is the order source of truth,
/// derived from prior line count under the server's single-instance lock).
fn ledger_line_count(ledger_path: &Path) -> u64 {
    let Ok(file) = std::fs::File::open(ledger_path) else {
        return 0;
    };
    io::BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .count() as u64
}

/// Append exactly one JSON record line to the ledger, returning the new `seq`
/// (= prior line count + 1). Pure-ish over a `Path` so it can be unit-tested
/// directly. Creates the file if absent; appends otherwise. The `seq` field
/// inside `record` is set by the CALLER before serialization — this helper does
/// not mutate the record; it only computes and returns the seq it expects the
/// caller used (read-count-then-append, fine under the single-instance lock).
fn append_ledger(ledger_path: &Path, record: &serde_json::Value) -> io::Result<u64> {
    let seq = ledger_line_count(ledger_path) + 1;
    let mut line = serde_json::to_string(record).map_err(io::Error::other)?;
    line.push('\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(ledger_path)?;
    file.write_all(line.as_bytes())?;
    Ok(seq)
}

/// Best-effort epoch-seconds timestamp for a ledger record (`ts`). Not the
/// ordering source (that is `seq`); included only as a convenience. `None` if
/// the clock is before the epoch.
fn now_epoch_secs() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Build a single ledger record JSON object. `seq` is read-then-stamped (prior
/// line count + 1) so the on-disk `seq` is exact. `changes` is capped at
/// [`LEDGER_CHANGES_CAP`]; a `changes_truncated` total is added when it overflows.
fn build_ledger_record(
    seq: u64,
    verb: &str,
    version: &str,
    summary: serde_json::Value,
    mut changes: Vec<serde_json::Value>,
) -> serde_json::Value {
    let total = changes.len();
    let truncated = total > LEDGER_CHANGES_CAP;
    if truncated {
        changes.truncate(LEDGER_CHANGES_CAP);
    }
    let mut record = serde_json::json!({
        "seq": seq,
        "verb": verb,
        "mode": "commit",
        "version": version,
        "summary": summary,
        "changes": changes,
    });
    if truncated {
        record["changes_truncated"] = serde_json::json!(total);
    }
    if let Some(ts) = now_epoch_secs() {
        record["ts"] = serde_json::json!(ts);
    }
    record
}

/// Best-effort ledger write for a committed bulk op. Resolves the path beside
/// the graph snapshot, computes the next `seq`, stamps it into the record, and
/// appends one line. Any failure (no parent dir, I/O error) is swallowed: the
/// underlying op already succeeded and the ledger must never fail it.
fn record_ledger(
    state: &SessionState,
    verb: &str,
    version: &str,
    summary: serde_json::Value,
    changes: Vec<serde_json::Value>,
) {
    let Some(path) = ledger_path_for(state) else {
        return; // unresolved path -> skip silently (fail-soft)
    };
    // seq = prior line count + 1, computed up front so the stamped record
    // matches what append writes (single-instance lock makes this race-free).
    let seq = ledger_line_count(&path) + 1;
    let record = build_ledger_record(seq, verb, version, summary, changes);
    let _ = append_ledger(&path, &record);
}

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
    /// Cross-call OCC token. When `Some(v)` on a `commit`, the handler recomputes
    /// the selection `version` fingerprint and ABORTS (writes nothing) if it no
    /// longer equals `v` — i.e. a selected node's tags changed since the caller's
    /// dry_run. `None` (default) keeps the original, unconditional-commit
    /// behavior. Obtain the token from a prior `dry_run`'s `version` field.
    #[serde(default)]
    pub expect_version: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct XrayCounts {
    /// Nodes the selector matched.
    pub selected: u32,
    /// Selected nodes whose tag set the op would change.
    pub planned: u32,
    /// Selected nodes the op would leave unchanged (e.g. add of a present tag).
    pub skipped_noop: u32,
    /// Cross-call OCC: count of selected nodes when a commit aborted because
    /// `expect_version` no longer matched the recomputed `version`. 0 otherwise
    /// (the within-lock plan/apply window carries no conflict).
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
    /// "dry_run", "committed", or "aborted_conflicts" (cross-call OCC mismatch).
    pub status: String,
    pub counts: XrayCounts,
    /// First few planned changes (cap 5), for the agent to eyeball before commit.
    pub planned_sample: Vec<XrayPlannedSample>,
    /// First few conflict ids (cap 5). Empty unless an `expect_version` mismatch
    /// aborted the commit, in which case it names the selected nodes.
    pub conflicts_sample: Vec<String>,
    /// Content fingerprint of the CURRENTLY SELECTED nodes' tag state (hex). The
    /// caller passes this back as `expect_version` on a later `commit` to guard
    /// against concurrent tag changes between dry_run and commit (cross-call OCC).
    pub version: String,
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

/// True if `nt` is a code SYMBOL (a declaration that can be annotated above its
/// line), as opposed to a container (File/Directory/Module), a reference, or a
/// domain/business node. `AnnotateSymbol` with `node_type: None` means "any
/// SYMBOL type", so this is the allowlist that omission resolves to — it stops
/// annotations from landing at file/module/document/concept locations now that
/// ingest populates provenance broadly. When `node_type` IS set, exact-match
/// (`node_type_to_u8`) is used instead and this allowlist is bypassed.
///
/// Covers the code-declaration kinds the tree-sitter extractors actually emit
/// (Function, Class, Struct, Enum, Type) while excluding File, Directory,
/// Module, Reference, Concept, and the domain variants (Material..Cost, System,
/// Custom).
fn is_symbol_node_type(nt: NodeType) -> bool {
    matches!(
        nt,
        NodeType::Function | NodeType::Class | NodeType::Struct | NodeType::Enum | NodeType::Type
    )
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

/// Cross-call OCC fingerprint over the CURRENTLY SELECTED nodes' tag state.
///
/// Per node we hash `external_id + "\x00" + sorted(tags).join(",")` into a u64
/// with the same non-cryptographic `DefaultHasher` the rest of this crate uses
/// (see the hashing note above `content_hash`), then XOR-fold the per-node
/// digests so the result is order-independent over the selection. Sorting the
/// tags first makes the digest insensitive to a node's internal tag order, so it
/// flips only when a node's *tag set* (or the selection itself) actually changes
/// — exactly the concurrent-edit signal the OCC guard needs.
fn selection_version(graph: &Graph, ext: &[String], selected: &[usize]) -> String {
    let mut fold: u64 = 0;
    for &idx in selected {
        let mut tags: Vec<&str> = graph.node_tags(NodeId::new(idx as u32));
        tags.sort_unstable();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        ext[idx].hash(&mut hasher);
        0u8.hash(&mut hasher); // explicit field separator
        tags.join(",").hash(&mut hasher);
        fold ^= hasher.finish();
    }
    format!("{fold:016x}")
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
    retag_graph_inner(graph, input, None)
}

/// Shared implementation. `ledger`, when `Some`, collects one
/// `{ node, before, after }` record per node ACTUALLY mutated (commit only),
/// captured in the SAME pass that mutates so before/after are exact. Production
/// passes `Some` only on commit (the handler), `None` on dry_run; tests reuse
/// `retag_graph` (None) for the existing assertions.
fn retag_graph_inner(
    graph: &mut Graph,
    input: &XrayRetagInput,
    mut ledger: Option<&mut Vec<serde_json::Value>>,
) -> XrayRetagOutput {
    let ext = node_to_ext_map(graph);
    let selected = select_nodes(graph, &input.selector, &ext);

    // Fingerprint the CURRENT selection state up front. On a guarded commit this
    // is the "actual" version recomputed over the freshly-selected nodes; if it
    // no longer matches the caller's `expect_version`, a selected node's tags
    // (or the selection) changed between dry_run and commit — abort, write
    // nothing, and hand back the current `version` so the caller can re-plan.
    let version = selection_version(graph, &ext, &selected);

    let commit = input.mode == XrayMode::Commit;

    if commit {
        if let Some(expected) = &input.expect_version {
            if expected != &version {
                let conflicts_sample = selected
                    .iter()
                    .take(SAMPLE_CAP)
                    .map(|&idx| ext[idx].clone())
                    .collect();
                return XrayRetagOutput {
                    verb: "xray_retag",
                    status: "aborted_conflicts".to_string(),
                    counts: XrayCounts {
                        selected: selected.len() as u32,
                        // No plan/apply was performed; flag the whole selection
                        // as conflicting so the caller sees the contention size.
                        // Zero selected => zero conflicts (no phantom 1).
                        conflicts: selected.len() as u32,
                        ..Default::default()
                    },
                    planned_sample: Vec::new(),
                    conflicts_sample,
                    version,
                };
            }
        }
    }

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
                let before_owned: Vec<String> = before.iter().map(|s| s.to_string()).collect();
                if planned_sample.len() < SAMPLE_CAP {
                    planned_sample.push(XrayPlannedSample {
                        id: ext[idx].clone(),
                        before: before_owned.clone(),
                        after: after.clone(),
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
                    // Audit ledger: record exact before/after for the node we
                    // just mutated (captured in the same pass, so it's precise).
                    if let Some(changes) = ledger.as_deref_mut() {
                        changes.push(serde_json::json!({
                            "node": ext[idx],
                            "before": before_owned,
                            "after": after,
                        }));
                    }
                }
            }
            None => counts.skipped_noop += 1,
        }
    }

    // Within ONE write lock, plan and apply see the same graph, so `conflicts`
    // stays 0 on this path — the within-call window is closed by construction.
    // Cross-call OCC (a tag set that changed BETWEEN two xray_retag calls) is
    // handled above via `expect_version` vs the recomputed `version`.
    XrayRetagOutput {
        verb: "xray_retag",
        status: if commit { "committed" } else { "dry_run" }.to_string(),
        counts,
        planned_sample,
        conflicts_sample: Vec::new(),
        version,
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
    // Collect per-node before/after only on commit (the ledger is appended once
    // the commit is confirmed to have applied ≥1 change).
    let mut changes: Vec<serde_json::Value> = Vec::new();
    let output = {
        let mut graph = state.graph.write();
        let ledger = if input.mode == XrayMode::Commit {
            Some(&mut changes)
        } else {
            None
        };
        retag_graph_inner(&mut graph, &input, ledger)
    };

    if input.mode == XrayMode::Commit && output.counts.applied > 0 {
        // Graph-write bookkeeping, mirroring `learn` (tools.rs) and the other
        // in-place writers: bump the generation so optimistic locks keyed on it
        // see the change, invalidate perspective caches, and mark lock baselines
        // stale (otherwise `lock.diff` fast-paths to no_changes over a committed
        // mutation — see lock_handlers.rs).
        state.bump_graph_generation();
        state.invalidate_all_perspectives();
        state.mark_all_lock_baselines_stale();
        // Persist via the session choke point: graph is source of truth, the
        // call is a no-op in read-only attach, and it writes to the canonical
        // graph_path. Not added to PROOF_GATED_WRITE_TOOLS on purpose — this
        // mutates graph metadata (tags), not agent-supplied source files.
        state.persist()?;
        // Append-only audit ledger (best-effort; never fails the op). Recorded
        // after persist so a ledgered line corresponds to a durable graph write.
        record_ledger(
            state,
            "xray_retag",
            &output.version,
            serde_json::json!({
                "selected": output.counts.selected,
                "planned": output.counts.planned,
                "skipped_noop": output.counts.skipped_noop,
                "applied": output.counts.applied,
            }),
            changes,
        );
    }

    serde_json::to_value(output).map_err(m1nd_core::error::M1ndError::Serde)
}

// ===========================================================================
// X-RAY physical-write verb: `xray_apply` — atomic source-file codemod
// ===========================================================================
// WARNING: this verb WRITES SOURCE FILES TO DISK.
//
// One agent call applies an idempotent, deterministic text transform across many
// source files via an ATOMIC 2-phase apply with content-hash optimistic
// concurrency. dry_run is the default; commit is the explicit opt-in to write.
//
// Algorithm (ported from xray/slice3_apply_atomic.py):
//   SELECT  read + content-hash + plan (skip no-ops; idempotent)
//   STAGE   write `<file>.xray.tmp`, flush + fsync, NEVER touching the original
//   REHASH  re-hash ALL originals; if any drifted since SELECT -> CONFLICT
//   ABORT   on any conflict (or any stage I/O error): delete every temp,
//           write ZERO originals (all-or-nothing)
//   SWAP    else atomic `rename(tmp, original)` for every staged pair
//
// SAFETY MODEL: this verb is intentionally NOT wired into PROOF_GATED_WRITE_TOOLS
// yet — integrating with the existing proof-gate is a deliberate follow-up. For
// now the guard rails are: dry-run-by-default, read-only-attach-denied (see
// READ_ONLY_DENIED_TOOLS in server.rs), root-confinement (canonical containment
// under workspace_root), and a forbidden-artifact filter (runtime/VCS/build).
//
// HASHING: the OCC guard hash is an *in-process-only* content fingerprint — it is
// never persisted, never compared across processes, and only ever compared to a
// re-hash of the same file inside the same apply call. We therefore use the same
// non-cryptographic `DefaultHasher` content hash the rest of this crate already
// uses (see `simple_content_hash` in tools.rs / daemon_handlers.rs). `sha2` is
// NOT a direct dependency of m1nd-mcp (it only reaches us transitively via the
// optional `serve`-feature `rust-embed`), so reaching for it would mean adding a
// new direct dep — unnecessary for an internal optimistic-concurrency guard.

/// File selector for `xray_apply`. Paths are resolved relative to the project
/// root (the session's `workspace_root`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct XrayFileSelector {
    /// Optional path prefix (relative to project root) to narrow the walk.
    #[serde(default)]
    pub path_prefix: Option<String>,
    /// File extensions to include (e.g. `["rs"]`). Empty = any extension.
    #[serde(default)]
    pub extensions: Vec<String>,
}

/// Where an `AnnotateSymbol` annotation is inserted relative to the symbol. The
/// MVP supports only [`AnnotatePosition::Above`] (a new line immediately above
/// the symbol's `line_start`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotatePosition {
    #[default]
    Above,
}

/// Default for [`XrayTransform::AnnotateSymbol::position`] (so the field can be
/// omitted by the agent and still default to `above`).
fn default_annotate_position() -> AnnotatePosition {
    AnnotatePosition::Above
}

/// The transform to apply. An enum so more transforms can slot in later.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum XrayTransform {
    /// Ensure a header tag exists in the first 3 lines; idempotent insert.
    /// FILE-driven: planned by reading each selected file's text.
    EnsureHeaderTag { tag: String },
    /// Insert `annotation` as its own line immediately ABOVE each selected
    /// symbol node's `line_start`. GRAPH-driven: symbols are selected from the
    /// live graph (tree-sitter provenance captured at ingest), so this is an
    /// AST-targeted edit that re-parses NOTHING — it reuses the line ranges the
    /// graph already holds. Applied bottom-up per file so earlier line numbers
    /// stay valid. Idempotent against the CURRENT provenance (a symbol whose
    /// immediately-preceding line already equals `annotation` is skipped) — and,
    /// like every commit of this verb, a write sets `graph_resync_required`: the
    /// caller must re-ingest before re-running so provenance reflects the shifted
    /// lines (a re-run on stale line numbers can no longer find the symbol).
    AnnotateSymbol {
        annotation: String,
        /// Optional exact node-type filter, expressed as the canonical u8 (see
        /// [`node_type_to_u8`] — File=0, Function=2, Struct=4, …). `None` = any.
        #[serde(default)]
        node_type: Option<u8>,
        /// Insertion position. MVP: only `above` (the default).
        #[serde(default = "default_annotate_position")]
        position: AnnotatePosition,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct XrayApplyInput {
    pub selector: XrayFileSelector,
    pub transform: XrayTransform,
    #[serde(default)]
    pub mode: XrayMode,
    /// Cross-call OCC token. When `Some(v)` on a `commit`, the engine recomputes
    /// the planned-files fingerprint after SELECT and ABORTS *before staging*
    /// (writes nothing) if it no longer equals `v` — i.e. a planned file's
    /// on-disk content changed since the caller's dry_run. This complements the
    /// existing within-call stage→rehash guard. `None` (default) keeps the
    /// original behavior. Obtain the token from a prior `dry_run`'s `version`.
    #[serde(default)]
    pub expect_version: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct XrayApplyCounts {
    /// Files the selector + safety filters yielded.
    pub matched: u32,
    /// Files the transform would change.
    pub planned: u32,
    /// Matched files where the transform is a no-op (idempotent hit).
    pub skipped_noop: u32,
    /// Matched files skipped because their bytes are not valid UTF-8 (binary).
    /// Never planned/staged/written — a binary file is left byte-for-byte intact.
    pub skipped_binary: u32,
    /// Files actually written (0 on dry_run / abort).
    pub applied: u32,
    /// Files whose content drifted between STAGE and REHASH.
    pub conflicts: u32,
    /// AST selection signal (graph-driven transforms only, e.g.
    /// `AnnotateSymbol`): how many symbol NODES the selector targeted across all
    /// files BEFORE per-line idempotency/skip filtering. 0 for file-driven
    /// transforms (`EnsureHeaderTag`) so the agent can tell that AST selection
    /// ran and how many symbols it matched.
    pub symbols_matched: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct XrayApplyPlannedSample {
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct XrayApplyOutput {
    pub verb: &'static str,
    /// "dry_run" | "committed" | "partial" | "aborted_conflicts".
    pub status: String,
    pub counts: XrayApplyCounts,
    /// First few planned paths (cap 5), for the agent to eyeball before commit.
    pub planned_sample: Vec<XrayApplyPlannedSample>,
    /// First few conflict file names (cap 5). On a `partial` swap this names the
    /// file whose rename failed (the boundary between swapped and not-yet-swapped).
    pub conflicts_sample: Vec<String>,
    /// Content fingerprint of the PLANNED files' current on-disk content (hex),
    /// order-independent over path. The caller passes this back as
    /// `expect_version` on a later `commit` to guard against concurrent file
    /// edits between dry_run and commit (cross-call OCC, checked before staging).
    pub version: String,
    /// True ONLY on a successful commit that wrote ≥1 source file
    /// (`status == "committed" && applied > 0`). When set, the in-memory graph is
    /// now STALE versus disk: the bytes changed but the graph was not re-ingested.
    /// The caller must trigger a re-ingest to reconcile. Always false on dry_run,
    /// abort, or a partial swap.
    pub graph_resync_required: bool,
}

/// In-process content fingerprint for the OCC guard. Non-cryptographic by design
/// (see the hashing note above): only ever compared to a re-hash of the same
/// file within one apply call.
fn content_hash(bytes: &[u8]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Cross-call OCC fingerprint over the PLANNED files' current on-disk content.
///
/// Per file we hash `path + "\0" + content_hash` (the SELECT-phase guard hex)
/// into a u64 with the same non-cryptographic `DefaultHasher`, then XOR-fold the
/// per-file digests so the result is order-independent over path. Keying in the
/// path is what stops two identical-content files (or a 64-bit content-hash
/// collision) from cancelling each other out under the fold — without it, a
/// concurrent edit could be masked. Flips whenever any planned file's path set
/// or bytes change on disk.
fn plan_version(entries: &[(PathBuf, String)]) -> String {
    let mut fold: u64 = 0;
    for (path, guard) in entries {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        path.to_string_lossy().hash(&mut hasher);
        0u8.hash(&mut hasher); // explicit field separator
        guard.hash(&mut hasher);
        fold ^= hasher.finish();
    }
    format!("{fold:016x}")
}

/// Pure FILE transform: returns `Some(new_content)` if it changes the file,
/// `None` if it is a no-op. Only FILE-driven variants are handled here;
/// GRAPH-driven variants (e.g. [`XrayTransform::AnnotateSymbol`]) are planned by
/// [`build_annotate_plan`] from graph provenance and never reach this function,
/// so they are a deliberate `None` (no per-file-text plan).
fn apply_transform(content: &str, transform: &XrayTransform) -> Option<String> {
    match transform {
        // GRAPH-driven: planned elsewhere from tree-sitter provenance. Never
        // routed through the file walk, so a no-op here by construction.
        XrayTransform::AnnotateSymbol { .. } => None,
        XrayTransform::EnsureHeaderTag { tag } => {
            // Look at the first 3 lines (keepends-equivalent). If the tag is
            // already a substring there -> no-op (idempotent).
            let lines: Vec<&str> = content.split_inclusive('\n').collect();
            let head: String = lines.iter().take(3).copied().collect();
            if head.contains(tag.as_str()) {
                return None;
            }
            // Insert AFTER line 0 if line 0 starts with `//`, else at line 0.
            let insert_at = if lines.first().is_some_and(|l| l.starts_with("//")) {
                1
            } else {
                0
            };
            let mut out = String::with_capacity(content.len() + tag.len() + 1);
            for line in lines.iter().take(insert_at) {
                out.push_str(line);
            }
            out.push_str(tag);
            out.push('\n');
            for line in lines.iter().skip(insert_at) {
                out.push_str(line);
            }
            Some(out)
        }
    }
}

/// Pure, unit-testable apply engine (no `SessionState`). See the algorithm note
/// at the top of this section. Infallible by design: any stage-phase I/O error
/// is treated as a hard abort that deletes every temp written so far and returns
/// status "aborted_conflicts" with ZERO originals touched.
pub fn apply_files(
    paths: &[PathBuf],
    transform: &XrayTransform,
    mode: XrayMode,
    expect_version: Option<&str>,
) -> XrayApplyOutput {
    apply_files_inner(paths, transform, mode, expect_version, None, None, None)
}

/// Like [`apply_files`] but collects, into `ledger`, one
/// `{ path, before_hash, after_hash }` record per file ACTUALLY swapped to disk
/// (every file on a clean commit; only the swapped prefix on a `partial`). Used
/// by the handler to append the audit ledger. `before_hash` is the SELECT-phase
/// guard hash; `after_hash` is the staged new content's hash.
pub fn apply_files_with_ledger(
    paths: &[PathBuf],
    transform: &XrayTransform,
    mode: XrayMode,
    expect_version: Option<&str>,
    ledger: &mut Vec<serde_json::Value>,
) -> XrayApplyOutput {
    apply_files_inner(
        paths,
        transform,
        mode,
        expect_version,
        None,
        None,
        Some(ledger),
    )
}

/// Test seam: a callback fired between STAGE and REHASH, receiving the original
/// paths so a test can mutate one mid-apply (mirrors the Python `tamper(plan)`).
type TamperHook<'a> = Option<&'a dyn Fn(&[PathBuf])>;

/// Test seam: a callback fired AFTER REHASH and BEFORE the SWAP loop, receiving
/// the staged `(original, tmp)` pairs in swap order so a test can sabotage a
/// SPECIFIC target's rename (e.g. replace a not-first original with a directory)
/// to exercise the partial-swap path. Production always passes `None`.
type BeforeSwapHook<'a> = Option<&'a dyn Fn(&[(PathBuf, PathBuf)])>;

/// Shared implementation. `tamper`, when `Some`, is invoked AFTER the STAGE phase
/// and BEFORE the REHASH phase, receiving the list of original paths so a test
/// can mutate one mid-apply to exercise the OCC-conflict path (mirrors the
/// Python `tamper(plan)` placement). `before_swap`, when `Some`, is invoked AFTER
/// REHASH and BEFORE the SWAP loop to exercise the partial-swap path. Production
/// always passes `None` for both.
fn apply_files_inner(
    paths: &[PathBuf],
    transform: &XrayTransform,
    mode: XrayMode,
    expect_version: Option<&str>,
    tamper: TamperHook<'_>,
    before_swap: BeforeSwapHook<'_>,
    mut ledger: Option<&mut Vec<serde_json::Value>>,
) -> XrayApplyOutput {
    let mut counts = XrayApplyCounts {
        matched: paths.len() as u32,
        ..Default::default()
    };

    // ---- SELECT: read + guard-hash + plan (FILE-driven transform) ----
    // plan entries: (path, guard_hash, new_bytes)
    let mut plan: Vec<(PathBuf, String, Vec<u8>)> = Vec::new();
    for p in paths {
        let Ok(bytes) = std::fs::read(p) else {
            // Unreadable file: skip without panicking (never unwrap a read).
            continue;
        };
        let guard = content_hash(&bytes);
        // Require valid UTF-8. A lossy decode would replace invalid bytes with
        // U+FFFD and silently CORRUPT a binary file once the transform staged it
        // back. Instead: skip non-UTF-8 (binary) files entirely — never
        // plan/stage/write them — and count them honestly so they can never
        // appear in `planned`/`applied`.
        let Ok(current) = std::str::from_utf8(&bytes) else {
            counts.skipped_binary += 1;
            continue;
        };
        match apply_transform(current, transform) {
            None => {
                counts.skipped_noop += 1;
            }
            Some(new_content) => {
                counts.planned += 1;
                plan.push((p.clone(), guard, new_content.into_bytes()));
            }
        }
    }

    // Hand the computed plan to the SHARED atomic applier (STAGE→REHASH→SWAP,
    // OCC, dry_run/commit, ledger). The graph-driven `AnnotateSymbol` path
    // builds its own `plan` + `counts` and calls the SAME applier — there is no
    // second write path.
    run_atomic_apply(
        plan,
        counts,
        mode,
        expect_version,
        tamper,
        before_swap,
        ledger,
    )
}

/// The SHARED atomic applier: takes an already-computed `plan` of
/// `(path, guard_hash, new_bytes)` plus seed `counts` (the planning phase already
/// stamped `matched`/`planned`/`skipped_noop`/`skipped_binary`/`symbols_matched`)
/// and runs the STAGE→REHASH→SWAP engine with content-hash OCC. Both the
/// file-driven [`apply_files_inner`] and the graph-driven `AnnotateSymbol` path
/// funnel through here so the on-disk write semantics are identical and live in
/// ONE place. `guard_hash` is the SELECT-phase content hash of each original.
#[allow(clippy::too_many_arguments)]
fn run_atomic_apply(
    plan: Vec<(PathBuf, String, Vec<u8>)>,
    mut counts: XrayApplyCounts,
    mode: XrayMode,
    expect_version: Option<&str>,
    tamper: TamperHook<'_>,
    before_swap: BeforeSwapHook<'_>,
    mut ledger: Option<&mut Vec<serde_json::Value>>,
) -> XrayApplyOutput {
    // Cross-call OCC fingerprint over the planned files' current bytes (the
    // SELECT-phase guard hashes), keyed by path. Order-independent over path.
    let version = plan_version(
        &plan
            .iter()
            .map(|(p, g, _)| (p.clone(), g.clone()))
            .collect::<Vec<_>>(),
    );

    // ---- DRY RUN: report the plan, write nothing ----
    if mode != XrayMode::Commit {
        let planned_sample = plan
            .iter()
            .take(SAMPLE_CAP)
            .map(|(p, _, _)| XrayApplyPlannedSample {
                path: p.to_string_lossy().into_owned(),
            })
            .collect();
        return XrayApplyOutput {
            verb: "xray_apply",
            status: "dry_run".to_string(),
            counts,
            planned_sample,
            conflicts_sample: Vec::new(),
            version,
            graph_resync_required: false,
        };
    }

    // ---- CROSS-CALL OCC GUARD: abort BEFORE staging if the planned files'
    // current content no longer matches the caller's expectation. This closes
    // the window BETWEEN dry_run and commit (a concurrent ingest / another
    // agent), complementing the within-call stage→rehash guard below. Writes
    // nothing: no temps are staged, no originals touched.
    if let Some(expected) = expect_version {
        if expected != version {
            let conflicts_sample = plan
                .iter()
                .take(SAMPLE_CAP)
                .map(|(p, _, _)| file_label(p))
                .collect();
            // Zero planned => zero conflicts (no phantom 1).
            counts.conflicts = plan.len() as u32;
            counts.applied = 0;
            return XrayApplyOutput {
                verb: "xray_apply",
                status: "aborted_conflicts".to_string(),
                counts,
                planned_sample: Vec::new(),
                conflicts_sample,
                version,
                graph_resync_required: false,
            };
        }
    }

    // ---- COMMIT · STAGE (phase 1): write all `.xray.tmp`, fsync, touch no original ----
    let mut temps: Vec<(PathBuf, PathBuf)> = Vec::new(); // (original, tmp)
    let cleanup = |temps: &[(PathBuf, PathBuf)]| {
        for (_orig, tmp) in temps {
            let _ = std::fs::remove_file(tmp);
        }
    };
    // Helper: a stage-phase hard abort. Deletes every temp WE staged so far,
    // writes ZERO originals (all-or-nothing), and reports the offending file as a
    // conflict. Does NOT touch `tmp` here — a pre-existing temp (symlink or stale
    // regular file) is the caller's to inspect, never ours to silently remove.
    let stage_abort = |temps: &[(PathBuf, PathBuf)], mut counts: XrayApplyCounts, label: String| {
        cleanup(temps);
        counts.conflicts = 1;
        counts.applied = 0;
        XrayApplyOutput {
            verb: "xray_apply",
            status: "aborted_conflicts".to_string(),
            counts,
            planned_sample: Vec::new(),
            conflicts_sample: vec![label],
            version: version.clone(),
            graph_resync_required: false,
        }
    };

    for (p, _guard, new_bytes) in &plan {
        let tmp = tmp_path_for(p);

        // Defensive pre-check: refuse to stage if `tmp` already exists as a
        // SYMLINK. `symlink_metadata` does NOT follow the link, so this catches a
        // sibling `<file>.xray.tmp` that points OUTSIDE workspace_root — writing
        // through it (and later renaming it over the original) would escape the
        // verb's root confinement. Treat it as a conflict; never remove/retry it.
        if let Ok(meta) = std::fs::symlink_metadata(&tmp) {
            if meta.file_type().is_symlink() {
                return stage_abort(
                    &temps,
                    counts,
                    format!("{}: refusing pre-existing symlink temp", file_label(p)),
                );
            }
        }

        // create_new(true): fails with ErrorKind::AlreadyExists if `tmp` already
        // exists (INCLUDING a symlink — which it never follows or truncates) and
        // never clobbers a pre-existing target. A stale regular `.xray.tmp` from a
        // crashed run also aborts here — acceptable and safe (zero originals
        // touched); the operator removes it and retries.
        let staged = (|| -> std::io::Result<()> {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)?;
            f.write_all(new_bytes)?;
            f.flush()?;
            f.sync_all()?; // fsync the staged temp
            Ok(())
        })();
        match staged {
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // A pre-existing temp blocked create-new. Do NOT remove it (it is
                // not ours), do NOT retry. Abort the whole batch as a conflict.
                return stage_abort(
                    &temps,
                    counts,
                    format!("{}: pre-existing temp blocks staging", file_label(p)),
                );
            }
            Err(_) => {
                // Any OTHER stage I/O error: delete every temp WE wrote so far
                // (including this one if it partially exists), write ZERO
                // originals, report the file that failed to stage.
                let _ = std::fs::remove_file(&tmp);
                return stage_abort(&temps, counts, file_label(p));
            }
            Ok(()) => {}
        }
        temps.push((p.clone(), tmp));
    }

    // ---- test seam: simulate concurrent edits between STAGE and REHASH ----
    if let Some(tamper) = tamper {
        let originals: Vec<PathBuf> = plan.iter().map(|(p, _, _)| p.clone()).collect();
        tamper(&originals);
    }

    // ---- REHASH (OCC): re-read + re-hash every original; collect drift ----
    let mut conflicts: Vec<String> = Vec::new();
    for (p, guard, _) in &plan {
        let drifted = match std::fs::read(p) {
            Ok(bytes) => content_hash(&bytes) != *guard,
            // A file that vanished/became unreadable since SELECT counts as drift.
            Err(_) => true,
        };
        if drifted {
            conflicts.push(file_label(p));
        }
    }

    if !conflicts.is_empty() {
        // ABORT all-or-nothing: delete every temp, write ZERO originals.
        cleanup(&temps);
        counts.conflicts = conflicts.len() as u32;
        counts.applied = 0;
        let conflicts_sample = conflicts.into_iter().take(SAMPLE_CAP).collect();
        return XrayApplyOutput {
            verb: "xray_apply",
            status: "aborted_conflicts".to_string(),
            counts,
            planned_sample: Vec::new(),
            conflicts_sample,
            version,
            graph_resync_required: false,
        };
    }

    // ---- test seam: sabotage a specific target's rename before the SWAP loop ----
    if let Some(before_swap) = before_swap {
        before_swap(&temps);
    }

    // ---- SWAP (phase 2): atomic rename of every staged temp over its original ----
    // Track how many renames actually succeeded so a mid-loop failure reports the
    // TRUTH instead of "aborted, applied 0". The swap is the one non-atomic edge:
    // files 0..swapped are already live, the rest are still staged temps.
    let mut swapped: usize = 0;
    for (i, (orig, tmp)) in temps.iter().enumerate() {
        if let Err(e) = std::fs::rename(tmp, orig) {
            // Same-filesystem rename should not fail here; if it does, stop.
            if swapped == 0 {
                // NOTHING swapped yet: the whole batch can be cleanly abandoned.
                // Delete every temp, write ZERO originals — identical to the
                // all-or-nothing abort paths above.
                cleanup(&temps);
                counts.conflicts = 1;
                counts.applied = 0;
                return XrayApplyOutput {
                    verb: "xray_apply",
                    status: "aborted_conflicts".to_string(),
                    counts,
                    planned_sample: Vec::new(),
                    conflicts_sample: vec![file_label(orig)],
                    version,
                    graph_resync_required: false,
                };
            }
            // PARTIAL: files 0..i are already swapped (live on disk). We must NOT
            // lie ("aborted, applied 0") and must NOT delete the not-yet-swapped
            // temps (i..) — leaving them lets a retry of the same call complete
            // the remaining renames (the transform is idempotent + the guard hash
            // still matches the not-yet-swapped originals). fsync the dirs whose
            // contents we DID change so the partial swap is durable.
            let already = &temps[..swapped];
            fsync_parent_dirs(already);
            counts.applied = swapped as u32;
            counts.conflicts = 1;
            // Audit ledger: record ONLY the files actually swapped to disk
            // (plan[..swapped]) — the not-yet-swapped temps weren't written.
            if let Some(changes) = ledger.as_deref_mut() {
                collect_apply_changes(changes, &plan[..swapped]);
            }
            return XrayApplyOutput {
                verb: "xray_apply",
                status: "partial".to_string(),
                counts,
                planned_sample: Vec::new(),
                conflicts_sample: vec![format!("{}: rename failed: {e}", file_label(orig))],
                version,
                // Partial: ≥1 source byte changed on disk, graph is stale. But a
                // retry is expected to finish the swap, so honesty over the
                // already-written files: signal a resync is required.
                graph_resync_required: true,
            };
        }
        swapped = i + 1;
    }

    // All renames succeeded. fsync each DISTINCT parent directory so the
    // name->inode swaps (not just the staged bytes) are durable across a crash.
    fsync_parent_dirs(&temps);

    counts.applied = counts.planned;
    // Audit ledger: every planned file was swapped to disk on a clean commit.
    if let Some(changes) = ledger {
        collect_apply_changes(changes, &plan);
    }
    let planned_sample = plan
        .iter()
        .take(SAMPLE_CAP)
        .map(|(p, _, _)| XrayApplyPlannedSample {
            path: p.to_string_lossy().into_owned(),
        })
        .collect();
    let applied = counts.applied;
    XrayApplyOutput {
        verb: "xray_apply",
        status: "committed".to_string(),
        counts,
        planned_sample,
        conflicts_sample: Vec::new(),
        version,
        // A successful commit rewrote source bytes; the in-memory graph is now
        // stale versus disk and must be re-ingested to reconcile.
        graph_resync_required: applied > 0,
    }
}

// ---------------------------------------------------------------------------
// AnnotateSymbol — graph-driven, AST-targeted plan (no re-parse)
// ---------------------------------------------------------------------------
// The proof-grown insight: m1nd already parsed every file with tree-sitter at
// ingest, so each symbol node carries `NodeProvenance { source_path, line_start,
// line_end }`. "AST-precise apply" = query the graph for symbol nodes, read their
// provenance line ranges, and insert annotations at exactly those lines — no
// re-parsing. Selection mirrors `XrayFileSelector` semantics adapted to NODES.

/// One AST target resolved from the graph: a file plus the 1-based `line_start`
/// of a symbol whose annotation goes immediately ABOVE that line.
type SymbolTarget = (PathBuf, u32);

/// PHASE A (under the graph READ lock, NO file IO): resolve the selector to
/// symbol nodes and collect their `(absolute source path, line_start)` targets.
///
/// A node is selected only if it satisfies EVERY provided predicate:
///   * module (derived from external_id via [`module_of`]) matches
///     `selector.path_prefix` when set;
///   * `node_type` (canonical u8) equals the requested `node_type` when set;
///   * the node has provenance with a non-empty `source_path` and
///     `line_start >= 1`;
///   * the resolved absolute path stays under `root` and is not a forbidden
///     runtime/VCS/build artifact (reuses [`is_forbidden_path`] +
///     `starts_with(root)` containment — the SAME confinement the file walk
///     uses, so a symbol whose source escapes root is dropped).
///
/// Returns `(targets, symbols_matched)` where `symbols_matched` counts every
/// node that passed selection (BEFORE per-line idempotency, which happens in
/// PHASE B once the files are read). Holds only the graph read lock; touches no
/// files.
fn collect_symbol_targets(
    graph: &Graph,
    root: &Path,
    node_type: Option<u8>,
    path_prefix: Option<&str>,
    extensions: &[String],
) -> (Vec<SymbolTarget>, u32) {
    let ext = node_to_ext_map(graph);
    let n = graph.num_nodes() as usize;
    let mut targets: Vec<SymbolTarget> = Vec::new();
    let mut symbols_matched: u32 = 0;
    for (i, external_id) in ext.iter().enumerate().take(n) {
        let nt = graph.nodes.node_type[i];
        // node_type: when SET, exact match on canonical u8; when OMITTED, accept
        // only code SYMBOL types (the schema's "any SYMBOL type"), so file/module/
        // document/concept nodes — which also carry provenance after ingest — are
        // never annotated.
        match node_type {
            Some(want) => {
                if node_type_to_u8(nt) != want {
                    continue;
                }
            }
            None => {
                if !is_symbol_node_type(nt) {
                    continue;
                }
            }
        }
        // path_prefix: the node's MODULE (first path segment of external_id)
        // must start with the requested prefix (NODE-adapted selector).
        if let Some(prefix) = path_prefix {
            match module_of(external_id) {
                Some(module) if module.starts_with(prefix) => {}
                _ => continue,
            }
        }
        // provenance: need a non-empty source_path and a 1-based line_start.
        let prov = graph.resolve_node_provenance(NodeId::new(i as u32));
        let (Some(source), Some(line_start)) = (prov.source_path, prov.line_start) else {
            continue;
        };
        if source.is_empty() || line_start < 1 {
            continue;
        }
        // extension filter: a symbol whose provenance source_path points at a file
        // outside the requested extensions is dropped (e.g. `extensions: ["rs"]`
        // excludes a `.py`/`.ts` symbol). Empty `extensions` = any. SAME check the
        // file-driven walk applies via `is_included`, shared through
        // `extension_allowed`. Applied on the relative source path (the join below
        // preserves the extension, so it's equivalent and cheaper).
        if !extension_allowed(Path::new(source.as_str()), extensions) {
            continue;
        }
        // Resolve to an absolute path under root, then confine: forbidden
        // artifacts and any path escaping root are dropped (same rules as the
        // file walk). source_path is relative to the project root.
        let abs = root.join(&source);
        if is_forbidden_path(&abs) {
            continue;
        }
        // Containment via canonicalized parent (the file may legitimately not
        // exist yet — confine on the lexical join AND, when resolvable, the
        // canonical form). A symlink/`..` that escapes root is refused.
        if !path_confined_to_root(&abs, root) {
            continue;
        }
        symbols_matched += 1;
        targets.push((abs, line_start));
    }
    (targets, symbols_matched)
}

/// True if `abs` is confined under `root`. Confinement is checked lexically on
/// the joined path AND, when the path (or its existing parent) can be
/// canonicalized, on the canonical form — so a `..` or symlink that escapes root
/// is refused even though the lexical join looked fine. A file that doesn't
/// exist yet still passes via the lexical + parent-canonical check.
fn path_confined_to_root(abs: &Path, root: &Path) -> bool {
    // Lexical guard: no `..` component may climb out (cheap, catches `a/../../x`).
    let mut depth: i64 = 0;
    for comp in abs
        .strip_prefix(root)
        .into_iter()
        .flat_map(|r| r.components())
    {
        match comp {
            std::path::Component::ParentDir => depth -= 1,
            std::path::Component::CurDir => {}
            _ => depth += 1,
        }
        if depth < 0 {
            return false;
        }
    }
    if !abs.starts_with(root) {
        return false;
    }
    // Canonical guard (defeats symlink escapes). Prefer canonicalizing the file;
    // if it doesn't exist, canonicalize the nearest existing ancestor.
    if let Ok(canon) = abs.canonicalize() {
        return canon.starts_with(root);
    }
    let mut cur = abs.parent();
    while let Some(p) = cur {
        if let Ok(canon) = p.canonicalize() {
            return canon.starts_with(root);
        }
        cur = p.parent();
    }
    // Nothing canonicalizable: fall back to the lexical result (already passed).
    true
}

/// PHASE B (graph lock DROPPED — does file IO): turn graph targets into an
/// atomic-apply plan. Groups targets by file, reads each file once, and inserts
/// `annotation + "\n"` above each target `line_start`.
///
/// CRITICAL invariants:
///   * BOTTOM-UP: inserts are applied highest-line-first so earlier line numbers
///     stay valid as lines shift.
///   * IDEMPOTENT: a target whose immediately-preceding line already equals
///     `annotation` (trimmed) is skipped — re-running never stacks duplicates.
///   * Non-UTF-8 files are skipped (binary guard, counted in `skipped_binary`).
///
/// Seeds `counts.matched` = number of distinct files touched, `planned` = files
/// whose content actually changes, `skipped_noop` = files where every target was
/// already annotated, and `symbols_matched` is passed through from PHASE A.
/// Returns `(plan, counts)` for [`run_atomic_apply`].
fn build_annotate_plan(
    targets: Vec<SymbolTarget>,
    annotation: &str,
    symbols_matched: u32,
) -> (Vec<(PathBuf, String, Vec<u8>)>, XrayApplyCounts) {
    // Group line_starts by file (dedup + descending for bottom-up insert).
    let mut by_file: BTreeMap<PathBuf, Vec<u32>> = BTreeMap::new();
    for (path, line_start) in targets {
        by_file.entry(path).or_default().push(line_start);
    }

    let mut counts = XrayApplyCounts {
        matched: by_file.len() as u32,
        symbols_matched,
        ..Default::default()
    };
    let mut plan: Vec<(PathBuf, String, Vec<u8>)> = Vec::new();
    let annotation_trimmed = annotation.trim();

    for (path, mut line_starts) in by_file {
        let Ok(bytes) = std::fs::read(&path) else {
            // Unreadable file: skip without panicking (never unwrap a read).
            continue;
        };
        let guard = content_hash(&bytes);
        let Ok(current) = std::str::from_utf8(&bytes) else {
            counts.skipped_binary += 1;
            continue;
        };
        // Keepends split so the rebuild is byte-exact (trailing-newline safe).
        let mut lines: Vec<String> = current.split_inclusive('\n').map(str::to_owned).collect();

        // BOTTOM-UP: highest line first so lower indices remain valid.
        line_starts.sort_unstable();
        line_starts.dedup();
        line_starts.reverse();

        let mut changed = false;
        for ls in line_starts {
            // 1-based line_start -> 0-based vec index of the symbol's line.
            let idx = (ls - 1) as usize;
            if idx > lines.len() {
                continue; // provenance points past EOF — skip defensively.
            }
            // IDEMPOTENT: skip if the immediately-preceding line already equals
            // the annotation (trimmed). idx == 0 has no preceding line.
            if idx > 0 {
                let prev = lines[idx - 1].trim_end_matches(['\n', '\r']).trim();
                if prev == annotation_trimmed {
                    continue;
                }
            }
            // Insert `annotation + "\n"` as its own line ABOVE the symbol line.
            lines.insert(idx, format!("{annotation}\n"));
            changed = true;
        }

        if changed {
            counts.planned += 1;
            plan.push((path, guard, lines.concat().into_bytes()));
        } else {
            counts.skipped_noop += 1;
        }
    }

    (plan, counts)
}

/// Append one `{ path, before_hash, after_hash }` audit-ledger record per plan
/// entry that was swapped to disk. `before_hash` is the SELECT-phase guard hash
/// (the original's content hash); `after_hash` is the staged new content's hash.
/// `path` is the file path the engine operated on (absolute); the handler
/// relativizes it against the project root before the line is written.
fn collect_apply_changes(
    changes: &mut Vec<serde_json::Value>,
    swapped: &[(PathBuf, String, Vec<u8>)],
) {
    for (path, before_hash, new_bytes) in swapped {
        changes.push(serde_json::json!({
            "path": path.to_string_lossy(),
            "before_hash": before_hash,
            "after_hash": content_hash(new_bytes),
        }));
    }
}

/// fsync the DISTINCT parent directory of every swapped (original) path, so the
/// directory entry rewritten by `rename` is durable (the staged temp was already
/// `sync_all`'d; this commits the name->inode swap). Dedups the parent set and
/// ignores any dir that can't be opened/synced on the platform (e.g. Windows,
/// where directory fsync is not generally available).
fn fsync_parent_dirs(swapped: &[(PathBuf, PathBuf)]) {
    let mut seen: std::collections::BTreeSet<PathBuf> = std::collections::BTreeSet::new();
    for (orig, _tmp) in swapped {
        if let Some(parent) = orig.parent() {
            if !seen.insert(parent.to_path_buf()) {
                continue;
            }
            if let Ok(dir) = std::fs::File::open(parent) {
                let _ = dir.sync_all();
            }
        }
    }
}

/// `<path>.xray.tmp` sibling for the staging phase.
fn tmp_path_for(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_os_string();
    s.push(".xray.tmp");
    PathBuf::from(s)
}

/// Short, human-readable label for a conflict/sample entry (file name only).
fn file_label(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string_lossy().into_owned())
}

/// True if this path must NEVER be touched: m1nd runtime artifacts, VCS, or
/// build dirs, plus our own staging temps.
fn is_forbidden_path(p: &Path) -> bool {
    // Lowercase the candidate name so the checks also hold on case-insensitive
    // filesystems (macOS/Windows): `GRAPH_SNAPSHOT.JSON` / `.XRAY.TMP` must be
    // caught the same as their lowercase forms.
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    // Forbidden file NAMES (runtime artifacts regenerated by daemon/ingest).
    if name == "graph_snapshot.json"
        || name == "daemon_alerts.json"
        || name == "document_cache_index.json"
        || name == "ingest_roots.json"
        || name.ends_with("_state.json")
        || name.ends_with(".xray.tmp")
        || name == LEDGER_FILE_NAME
    {
        return true;
    }
    // Forbidden path SEGMENTS (VCS / build / deps). Lowercased for the same
    // case-insensitive-filesystem reason.
    let path_str = p.to_string_lossy().to_lowercase();
    let path_str = path_str.replace('\\', "/"); // normalize for Windows
    if path_str.contains("/.git/")
        || path_str.contains("/target/")
        || path_str.contains("/node_modules/")
    {
        return true;
    }
    false
}

/// Recursively collect candidate files under `dir` that pass every safety filter
/// in `is_included`. Forbidden directories are pruned so we never descend into
/// `.git` / `target` / `node_modules`.
fn collect_files(dir: &Path, root: &Path, selector: &XrayFileSelector, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            // Prune forbidden dirs early (also skips `.git`/`target`/`node_modules`).
            if is_forbidden_path(&path) {
                continue;
            }
            let dir_name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            if matches!(
                dir_name.as_deref(),
                Some(".git" | "target" | "node_modules")
            ) {
                continue;
            }
            collect_files(&path, root, selector, out);
        } else if ft.is_file() && is_included(&path, root, selector) {
            out.push(path);
        }
    }
}

/// Per-file safety + selector gate. Includes only if ALL hold (see handler doc).
/// True if `path`'s extension is allowed by `wanted`. Empty `wanted` = any
/// extension (no filter). Case-insensitive on both sides (`.RS` matches `rs`),
/// so this is the SINGLE source of truth shared by the file-driven walk
/// ([`is_included`]) and the graph-driven `AnnotateSymbol` selection
/// ([`collect_symbol_targets`]) — keeping `extensions: ["rs"]` excluding a
/// `.py`/`.ts` symbol consistent across both apply paths.
fn extension_allowed(path: &Path, wanted: &[String]) -> bool {
    if wanted.is_empty() {
        return true;
    }
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .is_some_and(|e| wanted.iter().any(|w| w.to_lowercase() == e))
}

fn is_included(path: &Path, root: &Path, selector: &XrayFileSelector) -> bool {
    // (5) never touch runtime/VCS/build artifacts or our own temps.
    if is_forbidden_path(path) {
        return false;
    }
    // (2) canonical containment under the canonical root.
    let Ok(canon) = path.canonicalize() else {
        return false;
    };
    if !canon.starts_with(root) {
        return false;
    }
    // (3) extension filter (case-insensitive, so `.RS` matches `rs` on
    // case-insensitive filesystems and however the caller cased the wanted list).
    if !extension_allowed(path, &selector.extensions) {
        return false;
    }
    // (4) path_prefix: the path relative to root must start with the prefix.
    if let Some(prefix) = selector.path_prefix.as_deref() {
        let Ok(rel) = path.strip_prefix(root) else {
            return false;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let prefix_norm = prefix.trim_start_matches("./").replace('\\', "/");
        if !rel_str.starts_with(&prefix_norm) {
            return false;
        }
    }
    true
}

/// X-RAY physical-write handler. Resolves the project root from
/// `state.workspace_root` (which `infer_workspace_root` computes to AVOID managed
/// runtime dirs), walks the source tree under the safety filters, then calls the
/// pure engine. Refuses to write anything if the root cannot be resolved.
///
/// NOTE: intentionally NOT wired into PROOF_GATED_WRITE_TOOLS — see the section
/// note above. Safety here is dry-run-default + read-only-denied + root-confinement.
pub fn handle_xray_apply(
    state: &mut SessionState,
    input: XrayApplyInput,
) -> M1ndResult<serde_json::Value> {
    let root: PathBuf = state
        .workspace_root
        .as_deref()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: "xray_apply".to_string(),
            detail: "project root (workspace_root) could not be resolved; refusing to write"
                .to_string(),
        })?;
    let root = root.canonicalize().map_err(|e| M1ndError::InvalidParams {
        tool: "xray_apply".to_string(),
        detail: format!("could not canonicalize project root; refusing to write: {e}"),
    })?;

    // Collect per-file before/after hashes for the audit ledger only when a write
    // can happen (commit). The engine populates this for every file swapped to
    // disk (all on a clean commit, the swapped prefix on a partial).
    let mut changes: Vec<serde_json::Value> = Vec::new();
    let commit = input.mode == XrayMode::Commit;

    let output = match &input.transform {
        // GRAPH-driven AST transform: select symbol NODES from the live graph
        // (tree-sitter provenance), resolve their line ranges, build a bottom-up
        // idempotent insert plan, then funnel through the SAME atomic applier.
        XrayTransform::AnnotateSymbol {
            annotation,
            node_type,
            position: _, // MVP: only `above`, enforced by the enum default.
        } => {
            // PHASE A: under the graph READ lock, collect (abs_path, line_start)
            // targets. No file IO here, and the lock is dropped before STAGE/SWAP.
            let (targets, symbols_matched) = {
                let graph = state.graph.read();
                collect_symbol_targets(
                    &graph,
                    &root,
                    *node_type,
                    input.selector.path_prefix.as_deref(),
                    &input.selector.extensions,
                )
            };
            // PHASE B (lock dropped): read files + build the plan.
            let (plan, counts) = build_annotate_plan(targets, annotation, symbols_matched);
            // SHARED atomic applier — production passes no test seams. Ledger is
            // collected only on commit (mirrors the file-driven branch).
            let ledger = commit.then_some(&mut changes);
            run_atomic_apply(
                plan,
                counts,
                input.mode,
                input.expect_version.as_deref(),
                None,
                None,
                ledger,
            )
        }
        // FILE-driven transform: walk the source tree under the safety filters.
        XrayTransform::EnsureHeaderTag { .. } => {
            // Narrow the walk start to root.join(prefix) when a prefix is given
            // (and it stays a dir under root); the per-file gate re-checks anyway.
            let walk_start = match input.selector.path_prefix.as_deref() {
                Some(prefix) => {
                    let joined = root.join(prefix.trim_start_matches("./"));
                    if joined.is_dir() {
                        joined
                    } else {
                        root.clone()
                    }
                }
                None => root.clone(),
            };
            let mut matched: Vec<PathBuf> = Vec::new();
            collect_files(&walk_start, &root, &input.selector, &mut matched);
            matched.sort();
            if commit {
                apply_files_with_ledger(
                    &matched,
                    &input.transform,
                    input.mode,
                    input.expect_version.as_deref(),
                    &mut changes,
                )
            } else {
                apply_files(
                    &matched,
                    &input.transform,
                    input.mode,
                    input.expect_version.as_deref(),
                )
            }
        }
    };

    // Append the audit ledger only when source bytes were actually written
    // (committed or partial with ≥1 swap). The engine recorded absolute paths;
    // relativize each against the project root for a compact, portable record.
    let wrote =
        (output.status == "committed" || output.status == "partial") && output.counts.applied > 0;
    if wrote {
        for change in &mut changes {
            if let Some(abs) = change.get("path").and_then(|p| p.as_str()) {
                let rel = Path::new(abs)
                    .strip_prefix(&root)
                    .map(|r| r.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| abs.to_string());
                change["path"] = serde_json::json!(rel);
            }
        }
        record_ledger(
            state,
            "xray_apply",
            &output.version,
            serde_json::json!({
                "matched": output.counts.matched,
                "planned": output.counts.planned,
                "skipped_noop": output.counts.skipped_noop,
                "skipped_binary": output.counts.skipped_binary,
                "applied": output.counts.applied,
                "conflicts": output.counts.conflicts,
                "symbols_matched": output.counts.symbols_matched,
            }),
            changes,
        );
    }
    serde_json::to_value(output).map_err(M1ndError::Serde)
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

/// On-disk North-Star manifest file shape. `ratified` is a top-level flag that
/// gives a FILE-sourced manifest teeth (drives the gate's block/caution
/// decision); the rule fields (`forbid`/`layer_order`/`require_exists`) are
/// flattened into the inner [`XrayManifest`]. Unknown `_*` context keys are
/// IGNORED by default serde behavior (do NOT add `deny_unknown_fields`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct XrayManifestFile {
    #[serde(default)]
    pub ratified: bool,
    #[serde(flatten)]
    pub manifest: XrayManifest,
}

/// Load a North-Star manifest from a JSON file. Returns `(manifest, ratified)`
/// or `None` if the file is missing or unparseable (fail soft — log nothing).
fn load_manifest_file(path: &Path) -> Option<(XrayManifest, bool)> {
    let bytes = std::fs::read(path).ok()?;
    let parsed: XrayManifestFile = serde_json::from_slice(&bytes).ok()?;
    Some((parsed.manifest, parsed.ratified))
}

/// Resolved manifest + provenance. `ratified` is meaningful only when the source
/// is a file (it drives the gate's block/caution decision for file-sourced rules).
struct ResolvedManifest {
    manifest: XrayManifest,
    /// `"inline"` | `"file:<path>"` | `"none"`.
    source: String,
    /// True only when a file-sourced manifest declared `ratified: true`.
    ratified: bool,
}

/// Manifest resolution precedence shared by orient/gate/paint:
///  (a) INLINE manifest non-empty (any of forbid/layer_order/require_exists
///      populated) -> use it (source "inline", ratified=false here; the gate
///      keeps using its own `manifest_ratified` input for inline).
///  (b) else if `manifest_path` is Some -> load that file (source "file:<path>").
///  (c) else auto-discover `<workspace_root>/xray.manifest.json`.
///  (d) else empty manifest (source "none").
fn resolve_manifest(
    inline: &XrayManifest,
    manifest_path: Option<&str>,
    workspace_root: Option<&str>,
) -> ResolvedManifest {
    let inline_nonempty = !inline.forbid.is_empty()
        || !inline.layer_order.is_empty()
        || !inline.require_exists.is_empty();
    if inline_nonempty {
        return ResolvedManifest {
            manifest: inline.clone(),
            source: "inline".to_string(),
            ratified: false,
        };
    }
    if let Some(p) = manifest_path {
        if let Some((m, ratified)) = load_manifest_file(Path::new(p)) {
            return ResolvedManifest {
                manifest: m,
                source: format!("file:{p}"),
                ratified,
            };
        }
        // explicit path that failed to load -> fall through to none (honest)
    } else if let Some(root) = workspace_root {
        let candidate = Path::new(root).join("xray.manifest.json");
        if let Some((m, ratified)) = load_manifest_file(&candidate) {
            let disp = candidate.to_string_lossy().into_owned();
            return ResolvedManifest {
                manifest: m,
                source: format!("file:{disp}"),
                ratified,
            };
        }
    }
    ResolvedManifest {
        manifest: XrayManifest::default(),
        source: "none".to_string(),
        ratified: false,
    }
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
    /// Optional path to a North-Star manifest JSON file. Used only when the
    /// inline `manifest` is empty; takes precedence over auto-discovery of
    /// `<workspace_root>/xray.manifest.json`.
    #[serde(default)]
    pub manifest_path: Option<String>,
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
    /// Provenance of the manifest actually used: `"inline"`, `"file:<path>"`,
    /// or `"none"`.
    pub manifest_source: String,
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

/// THE shared layer-rule predicate. A cross-module edge `a -> b` diverges from
/// the manifest if `forbid` contains `(a, b)` OR both modules sit in
/// `layer_order` and `b` is at a *higher* layer than `a`. Returns the rule name
/// that flagged it (`"forbid"` / `"layer"`), or `None` if the edge converges.
///
/// Both `xray_orient` (erosion ledger) and `xray_gate` (pre-edit guardrail)
/// evaluate edges through THIS one function — there is no second copy of the
/// rule logic to drift.
fn classify_edge(manifest: &XrayManifest, a: &str, b: &str) -> Option<&'static str> {
    if manifest.forbid.iter().any(|(fa, fb)| fa == a && fb == b) {
        return Some("forbid");
    }
    let layer_index =
        |m: &str| -> Option<usize> { manifest.layer_order.iter().position(|x| x == m) };
    if let (Some(ia), Some(ib)) = (layer_index(a), layer_index(b)) {
        if ib > ia {
            return Some("layer");
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Pure core: conformance ledger over a finalized Graph (unit-testable, no
// SessionState). Walks the live CSR; classifies cross-module boundary edges.
// ---------------------------------------------------------------------------

/// Compute the conformance ledger. Pure over a finalized `Graph` — the CSR must
/// be populated (live server / post-`finalize()`). The manifest is RESOLVED by
/// the caller (inline / file / auto-discover) and passed in alongside its
/// provenance `manifest_source`, so the pure core has no filesystem concern.
pub fn orient_graph(
    graph: &Graph,
    input: &XrayOrientInput,
    manifest: &XrayManifest,
    manifest_source: String,
) -> XrayOrientOutput {
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

            if let Some(rule) = classify_edge(manifest, src_mod, dst_mod) {
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
    for need in &manifest.require_exists {
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
        manifest_source,
    }
}

/// MCP handler for `xray_orient`. Read-only: holds the graph *read* lock for the
/// computation, never mutates, never persists (safe under read-only attach).
/// Resolves the manifest (inline / file / auto-discover) before locking.
pub fn handle_xray_orient(
    state: &mut SessionState,
    input: XrayOrientInput,
) -> M1ndResult<serde_json::Value> {
    let resolved = resolve_manifest(
        &input.manifest,
        input.manifest_path.as_deref(),
        state.workspace_root.as_deref(),
    );
    let output = {
        let graph = state.graph.read();
        orient_graph(&graph, &input, &resolved.manifest, resolved.source)
    };
    serde_json::to_value(output).map_err(m1nd_core::error::M1ndError::Serde)
}

// ===========================================================================
// X-RAY read verb: `xray_gate` — the North-Star guardrail (read-only)
// ===========================================================================
// Before an agent edits code it asks ONE question: "am I about to violate the
// North Star?" The verb takes the node being edited plus the modules the change
// would add an outgoing dependency to, evaluates BOTH the node's existing
// outgoing cross-module edges AND the planned ones through the SAME rule
// predicate `xray_orient` uses (`classify_edge`), and returns clear/caution/
// blocked.
//
// ANTI-GUARDRAIL-FATIGUE: a violation only `blocked`s when the manifest is
// RATIFIED (`manifest_ratified: true`). Until then a violation is `caution` —
// the instrument is informative, not obstructive, while the North Star is still
// being negotiated. An empty manifest (no rules) is always `clear` (honest:
// nothing declared to violate).
//
// Read-only: takes the graph *read* lock, never mutates, never persists. Safe in
// read-only attach (hence NOT in `READ_ONLY_DENIED_TOOLS`).

#[derive(Debug, Clone, Default, Deserialize)]
pub struct XrayGateInput {
    /// external_id of the node about to be edited.
    pub node: String,
    /// Module names this change would add an OUTGOING dependency to. Each is
    /// evaluated as a planned edge `node_module -> M`.
    #[serde(default)]
    pub planned_imports: Vec<String>,
    #[serde(default)]
    pub manifest: XrayManifest,
    /// When `true`, any violation (existing or planned) escalates the verdict to
    /// `blocked`. When `false` (default), a violation is only `caution` — the
    /// North Star is not yet ratified, so the gate informs without obstructing.
    /// Used only when the resolved manifest source is INLINE; a FILE-sourced
    /// manifest's own `ratified` flag overrides this.
    #[serde(default)]
    pub manifest_ratified: bool,
    /// Optional path to a North-Star manifest JSON file. Used only when the
    /// inline `manifest` is empty; takes precedence over auto-discovery of
    /// `<workspace_root>/xray.manifest.json`. The file's `ratified` flag drives
    /// the block/caution decision for file-sourced rules.
    #[serde(default)]
    pub manifest_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct XrayGateViolation {
    pub from_module: String,
    pub to_module: String,
    /// Which rule flagged it: `forbid` or `layer`.
    pub rule: &'static str,
    /// `existing` (a live outgoing edge) or `planned` (from `planned_imports`).
    pub kind: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct XrayGateOutput {
    pub verb: &'static str,
    /// The node external_id the gate evaluated (echoed back).
    pub node: String,
    /// Module the node belongs to (empty if the node is unmapped / not found).
    pub node_module: String,
    /// `clear` | `caution` | `blocked`.
    pub verdict: String,
    /// Live outgoing cross-module edges of `node` that violate the manifest.
    pub existing_violations: Vec<XrayGateViolation>,
    /// Planned outgoing edges (from `planned_imports`) that would violate it.
    pub planned_violations: Vec<XrayGateViolation>,
    /// Short human strings explaining each violation / the all-clear.
    pub reasons: Vec<String>,
    /// Provenance of the manifest actually used: `"inline"`, `"file:<path>"`,
    /// or `"none"`.
    pub manifest_source: String,
}

/// Pure gate logic over a finalized `Graph` (unit-testable, no `SessionState`).
/// Read-only: walks the node's live outgoing CSR, never mutates. The manifest is
/// RESOLVED by the caller; `ratified` is the EFFECTIVE ratified flag (the inline
/// `manifest_ratified` input when source is inline, else the file's own flag),
/// and `manifest_source` is the resolved provenance.
pub fn gate_graph(
    graph: &Graph,
    input: &XrayGateInput,
    manifest: &XrayManifest,
    ratified: bool,
    manifest_source: String,
) -> XrayGateOutput {
    // 1. Resolve the node. Not found -> honest "clear": there is nothing to gate.
    let Some(nid) = graph.resolve_id(&input.node) else {
        return XrayGateOutput {
            verb: "xray_gate",
            node: input.node.clone(),
            node_module: String::new(),
            verdict: "clear".to_string(),
            existing_violations: Vec::new(),
            planned_violations: Vec::new(),
            reasons: vec!["node not in graph".to_string()],
            manifest_source,
        };
    };

    let ext = node_to_ext_map(graph);
    let idx = nid.as_usize();
    let node_module = ext
        .get(idx)
        .and_then(|id| module_of(id))
        .map(|m| m.to_string())
        .unwrap_or_default();

    // 5. Empty manifest -> always clear (honest: nothing declared to violate).
    // Uses the RESOLVED manifest, so a file-loaded manifest carrying rules is
    // NOT treated as empty.
    let manifest_empty = manifest.forbid.is_empty() && manifest.layer_order.is_empty();
    if node_module.is_empty() || manifest_empty {
        let reason = if node_module.is_empty() {
            "node has no derivable module"
        } else {
            "manifest declares no rules"
        };
        return XrayGateOutput {
            verb: "xray_gate",
            node: input.node.clone(),
            node_module,
            verdict: "clear".to_string(),
            existing_violations: Vec::new(),
            planned_violations: Vec::new(),
            reasons: vec![reason.to_string()],
            manifest_source,
        };
    }

    let mut existing_violations: Vec<XrayGateViolation> = Vec::new();
    let mut reasons: Vec<String> = Vec::new();

    // 2. EXISTING violations: walk the node's live outgoing imports/depends_on
    // edges, derive the target module, evaluate the shared rule predicate.
    for e in graph.csr.out_range(nid) {
        let rel = graph.strings.resolve(graph.csr.relations[e]);
        if rel != "imports" && rel != "depends_on" {
            continue;
        }
        let dst = graph.csr.targets[e].as_usize();
        let Some(dst_id) = ext.get(dst) else { continue };
        let Some(dst_mod) = module_of(dst_id) else {
            continue;
        };
        if dst_mod == node_module {
            continue; // intra-module edges can't violate a cross-module rule
        }
        if let Some(rule) = classify_edge(manifest, &node_module, dst_mod) {
            reasons.push(format!(
                "existing {rule}: {node_module} -> {dst_mod} (via {rel})"
            ));
            existing_violations.push(XrayGateViolation {
                from_module: node_module.clone(),
                to_module: dst_mod.to_string(),
                rule,
                kind: "existing",
            });
        }
    }

    // 3. PLANNED violations: evaluate edge node_module -> M for each planned M.
    let mut planned_violations: Vec<XrayGateViolation> = Vec::new();
    for m in &input.planned_imports {
        if m == &node_module {
            continue; // a self-edge to one's own module is not a cross-module rule
        }
        if let Some(rule) = classify_edge(manifest, &node_module, m) {
            reasons.push(format!("planned {rule}: {node_module} -> {m}"));
            planned_violations.push(XrayGateViolation {
                from_module: node_module.clone(),
                to_module: m.clone(),
                rule,
                kind: "planned",
            });
        }
    }

    // 4. Verdict: violations + ratified => blocked; violations + not ratified =>
    // caution (anti-fatigue); no violations => clear.
    let any = !existing_violations.is_empty() || !planned_violations.is_empty();
    let verdict = if any {
        if ratified {
            "blocked"
        } else {
            "caution"
        }
    } else {
        "clear"
    };
    if !any {
        reasons.push("no North-Star violation".to_string());
    } else if !ratified {
        reasons.push("manifest not ratified — caution, not blocked".to_string());
    }

    XrayGateOutput {
        verb: "xray_gate",
        node: input.node.clone(),
        node_module,
        verdict: verdict.to_string(),
        existing_violations,
        planned_violations,
        reasons,
        manifest_source,
    }
}

/// MCP handler for `xray_gate`. Read-only: holds the graph *read* lock for the
/// computation, never mutates, never persists (safe under read-only attach).
/// Resolves the manifest (inline / file / auto-discover) and picks the EFFECTIVE
/// ratified flag (inline -> `manifest_ratified` input; file -> the file's own
/// `ratified`) before locking.
pub fn handle_xray_gate(
    state: &mut SessionState,
    input: XrayGateInput,
) -> M1ndResult<serde_json::Value> {
    let resolved = resolve_manifest(
        &input.manifest,
        input.manifest_path.as_deref(),
        state.workspace_root.as_deref(),
    );
    let effective_ratified = if resolved.source == "inline" {
        input.manifest_ratified
    } else {
        resolved.ratified
    };
    let output = {
        let graph = state.graph.read();
        gate_graph(
            &graph,
            &input,
            &resolved.manifest,
            effective_ratified,
            resolved.source,
        )
    };
    serde_json::to_value(output).map_err(m1nd_core::error::M1ndError::Serde)
}

// ===========================================================================
// X-RAY write verb: `xray_paint` — the PAINT pass (persist proof-state tags)
// ===========================================================================
// One agent call classifies every in-scope node into a STRUCTURAL proof-state
// from REAL graph signals and writes that state as a persistent tag
// `xray:state:<state>` — closing the loop so proof-states become QUERYABLE tags
// instead of ephemeral per-call computations (xray_orient reports them; this
// verb makes them durable graph metadata).
//
// CLASSIFICATION (honest / proof-grown — these are STRUCTURAL labels, not
// confirmed verdicts). For each in-scope node:
//   * `erosion-candidate` — the node is the SOURCE of a cross-module edge that
//     the shared `classify_edge` predicate flags as a divergence against the
//     input manifest. HONEST: a candidate (the manifest may not be ratified),
//     never a confirmed violation — same stance as xray_orient.
//   * else `bedrock` — the node has REFERENCE in-degree > 0 (something imports/
//     calls/references/depends_on it): it is load-bearing.
//   * else `overgrowth` — the node is an orphan over the reference relations
//     (off-lattice): nothing points at it.
// In-degree is computed over the REFERENCE relations only (`imports`, `calls`,
// `references`, `depends_on`) by walking every node's outgoing CSR range ONCE
// and counting targets. BLUEPRINT is a manifest-level ABSENCE (a require_exists
// that has no node), so it is NEVER painted on a node — only the three states
// above are node tags.
//
// IDEMPOTENT RE-PAINT: before adding the computed state tag, every existing
// `xray:state:*` tag on the node is removed, so re-running REPLACES the state
// and never accumulates stale states. A node already carrying exactly the
// computed state is left unchanged (no spurious "painted").
//
// dry_run (DEFAULT) computes the classification + counts and writes NOTHING.
// commit applies the tag swap via the shipped columnar mutators and persists
// the graph snapshot through the session save choke point. Mutates graph
// metadata (tags) only — never source files — so, like xray_retag, it is NOT in
// PROOF_GATED_WRITE_TOOLS but IS in READ_ONLY_DENIED_TOOLS.

/// Reference relations that carry "X is used by Y" semantics — the edges whose
/// in-degree makes a node load-bearing (BEDROCK) vs orphan (OVERGROWTH).
const REFERENCE_RELATIONS: &[&str] = &["imports", "calls", "references", "depends_on"];

const STATE_TAG_PREFIX: &str = "xray:state:";

#[derive(Debug, Clone, Default, Deserialize)]
pub struct XrayPaintInput {
    /// Optional external_id path-prefix filter. Only nodes whose external_id
    /// starts with this prefix are classified and (on commit) painted.
    #[serde(default)]
    pub scope: Option<String>,
    /// North-star ruleset used only to flag `erosion-candidate` source nodes
    /// (via the shared `classify_edge`). Empty manifest => no erosion candidates
    /// (honest: instrument not aimed), every referenced node is bedrock and every
    /// orphan is overgrowth.
    #[serde(default)]
    pub manifest: XrayManifest,
    /// Optional path to a North-Star manifest JSON file. Used only when the
    /// inline `manifest` is empty; takes precedence over auto-discovery of
    /// `<workspace_root>/xray.manifest.json`.
    #[serde(default)]
    pub manifest_path: Option<String>,
    #[serde(default)]
    pub mode: XrayMode,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct XrayPaintCounts {
    /// In-scope nodes classified.
    pub scanned: u32,
    /// Nodes classified `bedrock` (has PROOF EVIDENCE: test-exercised or grounded).
    pub bedrock: u32,
    /// Nodes classified `overgrowth` (orphan — zero incoming reference edges).
    pub overgrowth: u32,
    /// Nodes classified `unproven` (used — incoming refs — but no proof evidence).
    pub unproven: u32,
    /// Nodes classified `erosion-candidate` (source of a flagged cross-module edge).
    pub erosion_candidate: u32,
    /// Nodes whose `xray:state:*` tag set was actually replaced (0 on dry_run;
    /// on commit, the nodes whose state tag changed).
    pub painted: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct XrayPaintOutput {
    pub verb: &'static str,
    /// "dry_run" or "committed".
    pub status: String,
    pub counts: XrayPaintCounts,
    /// Content fingerprint of the in-scope nodes' CURRENT tag state (hex), reusing
    /// the same OCC fold as xray_retag. Lets a caller correlate a dry_run plan with
    /// the commit it later runs.
    pub version: String,
    /// Fraction of scanned (in-scope, non-skipped) nodes classified `bedrock`
    /// (have proof evidence): `bedrock / scanned`, rounded to 3 decimals. 0.0 when
    /// nothing is in scope.
    pub proof_coverage: f64,
    /// Provenance of the manifest actually used: `"inline"`, `"file:<path>"`,
    /// or `"none"`.
    pub manifest_source: String,
}

/// The four structural proof-states a node can be painted with. Named honestly:
/// these are STRUCTURAL classifications from real graph signals, not confirmed
/// verdicts (erosion is a *candidate*; `bedrock` means it carries proof
/// EVIDENCE — test-exercised or grounded — not a confirmed proof).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaintState {
    Bedrock,
    Overgrowth,
    Unproven,
    ErosionCandidate,
}

impl PaintState {
    /// The full persistent tag for this state (`xray:state:<state>`).
    fn tag(self) -> &'static str {
        match self {
            PaintState::Bedrock => "xray:state:bedrock",
            PaintState::Overgrowth => "xray:state:overgrowth",
            PaintState::Unproven => "xray:state:unproven",
            PaintState::ErosionCandidate => "xray:state:erosion-candidate",
        }
    }
}

/// True if a node's external_id denotes a TEST source file. We look at the path
/// portion (after `file::`, up to the first `::` kind separator).
pub(crate) fn is_test_source(external_id: &str) -> bool {
    let rest = external_id.strip_prefix(FILE_PREFIX).unwrap_or(external_id);
    let path = rest.split("::").next().unwrap_or(rest);
    path.contains("/tests/")
        || path.contains("/test_")
        || path.ends_with("_test.rs")
        || path.ends_with("/tests.rs")
        || path == "tests.rs"
}

/// Indices of nodes that have PROOF EVIDENCE: either they are the target of a
/// structural edge (imports/calls/references) FROM a test-source node, or they
/// have an incoming `grounded_in` edge. Computed in one CSR pass.
fn exercised_set(graph: &Graph, ext: &[String]) -> Vec<bool> {
    let n = graph.num_nodes() as usize;
    let mut exercised = vec![false; n];
    for i in 0..n {
        let src_is_test = ext.get(i).is_some_and(|e| is_test_source(e));
        for e in graph.csr.out_range(NodeId::new(i as u32)) {
            let rel = graph.strings.resolve(graph.csr.relations[e]);
            let dst = graph.csr.targets[e].as_usize();
            if dst >= n {
                continue;
            }
            // grounded_in evidence: any incoming grounded_in marks the target.
            if rel == "grounded_in" {
                exercised[dst] = true;
                continue;
            }
            // test-exercise evidence: a test-source node referencing a target.
            if src_is_test && (rel == "imports" || rel == "calls" || rel == "references") {
                exercised[dst] = true;
            }
        }
    }
    exercised
}

/// Build a reference in-degree map over the whole graph in ONE pass: for every
/// node, walk its outgoing CSR range and, for each edge whose relation is a
/// REFERENCE relation, bump the target's in-degree. Index by node index.
fn reference_indegree(graph: &Graph) -> Vec<u32> {
    let n = graph.num_nodes() as usize;
    let mut indeg = vec![0u32; n];
    for i in 0..n {
        for e in graph.csr.out_range(NodeId::new(i as u32)) {
            let rel = graph.strings.resolve(graph.csr.relations[e]);
            if !REFERENCE_RELATIONS.contains(&rel) {
                continue;
            }
            let dst = graph.csr.targets[e].as_usize();
            if dst < n {
                indeg[dst] += 1;
            }
        }
    }
    indeg
}

/// Set of node indices that are the SOURCE of at least one cross-module edge the
/// `classify_edge` predicate flags against the manifest (the `erosion-candidate`
/// source set). Walks the live CSR once, reusing `module_of` + `classify_edge` —
/// the same derivation `xray_orient`/`xray_gate` use, no second copy of the rule.
fn erosion_source_set(graph: &Graph, ext: &[String], manifest: &XrayManifest) -> Vec<bool> {
    let n = graph.num_nodes() as usize;
    let mut flagged = vec![false; n];
    for i in 0..n {
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
            let dst_mod = match ext.get(dst).and_then(|d| module_of(d)) {
                Some(m) => m,
                None => continue,
            };
            if src_mod == dst_mod {
                continue;
            }
            if classify_edge(manifest, src_mod, dst_mod).is_some() {
                flagged[i] = true;
                break; // one flagged edge is enough to mark the source
            }
        }
    }
    flagged
}

/// Precedence: erosion-candidate > bedrock (has proof evidence) > overgrowth
/// (orphan, zero incoming refs) > unproven (used but no proof evidence).
fn classify_node(indegree: u32, is_exercised: bool, is_erosion_source: bool) -> PaintState {
    if is_erosion_source {
        PaintState::ErosionCandidate
    } else if is_exercised {
        PaintState::Bedrock
    } else if indegree == 0 {
        PaintState::Overgrowth
    } else {
        PaintState::Unproven
    }
}

/// Resolve the workspace manifesto (auto-discovered xray.manifest.json) and
/// classify EVERY node's conformance state in one shot, for callers that want to
/// bias by architectural intent (e.g. seek). Returns None when no usable
/// (non-empty) manifesto resolves, so callers stay zero-cost by default. Reuses
/// the exact same predicates as orient/gate/paint — no second evaluator.
pub(crate) fn resolve_node_conformance(
    graph: &Graph,
    workspace_root: Option<&str>,
) -> Option<(Vec<PaintState>, String)> {
    let resolved = resolve_manifest(&XrayManifest::default(), None, workspace_root);
    let m = &resolved.manifest;
    if m.forbid.is_empty() && m.layer_order.is_empty() && m.require_exists.is_empty() {
        return None;
    }
    let ext = node_to_ext_map(graph);
    let erosion = erosion_source_set(graph, &ext, m);
    let exercised = exercised_set(graph, &ext);
    let indeg = reference_indegree(graph);
    let n = graph.num_nodes() as usize;
    let states = (0..n)
        .map(|i| {
            classify_node(
                indeg.get(i).copied().unwrap_or(0),
                exercised.get(i).copied().unwrap_or(false),
                erosion.get(i).copied().unwrap_or(false),
            )
        })
        .collect();
    Some((states, resolved.source))
}

/// Pure paint core over a finalized `Graph` (unit-testable, no `SessionState`).
/// On `DryRun` the graph is read only; on `Commit` each in-scope node's
/// `xray:state:*` tags are replaced (remove existing state tags, add the computed
/// one) via the shipped columnar mutators. Persistence is the handler's job. The
/// manifest is RESOLVED by the caller and passed in with its provenance.
pub fn paint_graph(
    graph: &mut Graph,
    input: &XrayPaintInput,
    manifest: &XrayManifest,
    manifest_source: String,
) -> XrayPaintOutput {
    paint_graph_inner(graph, input, manifest, manifest_source, None)
}

/// Shared implementation. `ledger`, when `Some`, collects one
/// `{ node, before, after }` record per node whose `xray:state:*` tag set was
/// ACTUALLY replaced (commit only), captured in the same pass as the mutation so
/// before/after are exact. Production passes `Some` only on commit (the handler);
/// tests reuse `paint_graph` (None).
fn paint_graph_inner(
    graph: &mut Graph,
    input: &XrayPaintInput,
    manifest: &XrayManifest,
    manifest_source: String,
    mut ledger: Option<&mut Vec<serde_json::Value>>,
) -> XrayPaintOutput {
    let ext = node_to_ext_map(graph);
    let n = graph.num_nodes() as usize;
    let scope = input.scope.as_deref();

    let in_scope = |idx: usize| -> bool {
        scope.is_none_or(|p| ext.get(idx).is_some_and(|e| e.starts_with(p)))
    };

    // One-pass signals over the WHOLE graph (counted edges from any source,
    // including out-of-scope ones — a node referenced/exercised from outside the
    // scope is still load-bearing / proven).
    let indeg = reference_indegree(graph);
    let exercised = exercised_set(graph, &ext);
    let erosion = erosion_source_set(graph, &ext, manifest);

    let selected: Vec<usize> = (0..n).filter(|&i| in_scope(i)).collect();
    let version = selection_version(graph, &ext, &selected);

    let commit = input.mode == XrayMode::Commit;
    let mut counts = XrayPaintCounts {
        scanned: selected.len() as u32,
        ..Default::default()
    };

    for &i in &selected {
        let state = classify_node(indeg[i], exercised[i], erosion[i]);
        match state {
            PaintState::Bedrock => counts.bedrock += 1,
            PaintState::Overgrowth => counts.overgrowth += 1,
            PaintState::Unproven => counts.unproven += 1,
            PaintState::ErosionCandidate => counts.erosion_candidate += 1,
        }

        let nid = NodeId::new(i as u32);
        let new_tag = state.tag();

        // Existing state tags on this node (any `xray:state:*`).
        let existing_state_tags: Vec<String> = graph
            .node_tags(nid)
            .iter()
            .filter(|t| t.starts_with(STATE_TAG_PREFIX))
            .map(|s| s.to_string())
            .collect();

        // The node already carries EXACTLY the computed state (and no other
        // state tag) => no change needed.
        let already_correct = existing_state_tags.len() == 1 && existing_state_tags[0] == new_tag;
        if already_correct {
            continue;
        }

        // The node's state tag set WOULD change. `painted` counts only nodes
        // actually written, so it stays 0 on dry_run (mirrors xray_retag's
        // `applied`); commit performs the replace and counts it.
        if commit {
            // Capture the FULL tag set before the swap for the audit ledger.
            let before_owned: Option<Vec<String>> = ledger
                .as_deref()
                .map(|_| graph.node_tags(nid).iter().map(|s| s.to_string()).collect());
            // Replace: drop every existing state tag, then add the computed one.
            if !existing_state_tags.is_empty() {
                let to_remove: Vec<&str> = existing_state_tags.iter().map(String::as_str).collect();
                graph.remove_node_tags(nid, &to_remove);
            }
            graph.add_node_tags(nid, &[new_tag]);
            counts.painted += 1;
            // Audit ledger: exact before/after for the node we just repainted.
            if let (Some(changes), Some(before_owned)) = (ledger.as_deref_mut(), before_owned) {
                let after_owned: Vec<String> =
                    graph.node_tags(nid).iter().map(|s| s.to_string()).collect();
                changes.push(serde_json::json!({
                    "node": ext[i],
                    "before": before_owned,
                    "after": after_owned,
                }));
            }
        }
    }

    // proof_coverage = bedrock / scanned (non-skipped), rounded to 3 decimals.
    // Guard divide-by-zero: nothing in scope -> 0.0.
    let proof_coverage = if counts.scanned == 0 {
        0.0
    } else {
        let raw = counts.bedrock as f64 / counts.scanned as f64;
        (raw * 1000.0).round() / 1000.0
    };

    XrayPaintOutput {
        verb: "xray_paint",
        status: if commit { "committed" } else { "dry_run" }.to_string(),
        counts,
        version,
        proof_coverage,
        manifest_source,
    }
}

/// MCP handler for `xray_paint`. Holds the graph write lock only for the
/// classify/paint, drops it, then — on commit with changes — persists through the
/// session's single save choke point (a no-op under read-only attach, and the
/// dispatcher additionally denies the verb in read-only attach).
pub fn handle_xray_paint(
    state: &mut SessionState,
    input: XrayPaintInput,
) -> M1ndResult<serde_json::Value> {
    let resolved = resolve_manifest(
        &input.manifest,
        input.manifest_path.as_deref(),
        state.workspace_root.as_deref(),
    );
    let mut changes: Vec<serde_json::Value> = Vec::new();
    let output = {
        let mut graph = state.graph.write();
        let ledger = if input.mode == XrayMode::Commit {
            Some(&mut changes)
        } else {
            None
        };
        paint_graph_inner(
            &mut graph,
            &input,
            &resolved.manifest,
            resolved.source,
            ledger,
        )
    };

    if input.mode == XrayMode::Commit && output.counts.painted > 0 {
        // Graph-write bookkeeping, mirroring `learn` and xray_retag: bump the
        // generation so optimistic locks keyed on it see the change, invalidate
        // perspective caches, and mark lock baselines stale (otherwise `lock.diff`
        // fast-paths to no_changes over a committed mutation).
        state.bump_graph_generation();
        state.invalidate_all_perspectives();
        state.mark_all_lock_baselines_stale();
        // Persist via the session choke point. Like xray_retag this mutates graph
        // metadata (tags), not agent-supplied source files — so it is deliberately
        // NOT added to PROOF_GATED_WRITE_TOOLS.
        state.persist()?;
        // Append-only audit ledger (best-effort; never fails the op).
        record_ledger(
            state,
            "xray_paint",
            &output.version,
            serde_json::json!({
                "scanned": output.counts.scanned,
                "bedrock": output.counts.bedrock,
                "overgrowth": output.counts.overgrowth,
                "unproven": output.counts.unproven,
                "erosion_candidate": output.counts.erosion_candidate,
                "painted": output.counts.painted,
            }),
            changes,
        );
    }

    serde_json::to_value(output).map_err(m1nd_core::error::M1ndError::Serde)
}

// ===========================================================================
// X-RAY read verb: `xray_ledger` — replay the append-only audit ledger
// ===========================================================================
// One agent call reads back the audit ledger that xray_retag / xray_paint /
// xray_apply append to on every committed bulk write. Returns the LAST `limit`
// entries (most recent first), optionally filtered by verb. Missing ledger ->
// empty list (honest, not an error). READ-ONLY: never writes, safe in read-only
// attach (hence NOT in READ_ONLY_DENIED_TOOLS).

#[derive(Debug, Clone, Default, Deserialize)]
pub struct XrayLedgerInput {
    /// Max entries to return (most recent first). Defaults to 20.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Optional verb-name filter (e.g. "xray_paint"). When set, only records
    /// whose `verb` equals this are returned (and counted toward `limit`).
    #[serde(default)]
    pub verb: Option<String>,
}

const LEDGER_DEFAULT_LIMIT: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub struct XrayLedgerOutput {
    pub verb: &'static str,
    /// The matching ledger records (most recent first, capped at `limit`).
    pub entries: Vec<serde_json::Value>,
    /// Total lines in the ledger file BEFORE the verb filter (0 if missing).
    pub total_entries: usize,
    /// Resolved ledger path, or `null` if it couldn't be resolved.
    pub ledger_path: Option<String>,
}

/// Pure-ish ledger reader: parse every line of `ledger_path` as JSON, optionally
/// filter by `verb`, return the LAST `limit` matching entries most-recent-first,
/// alongside the TOTAL line count (pre-filter). Missing/unreadable file -> empty
/// entries + total 0. Unparseable lines are skipped (counted in neither total nor
/// entries) — the ledger is append-only JSONL, so a corrupt line is tolerated.
fn read_ledger(
    ledger_path: &Path,
    limit: usize,
    verb_filter: Option<&str>,
) -> (Vec<serde_json::Value>, usize) {
    let Ok(file) = std::fs::File::open(ledger_path) else {
        return (Vec::new(), 0);
    };
    // Stream the append-only JSONL, retaining only the last `limit` MATCHING
    // entries in a bounded ring buffer, so a small read never allocates the whole
    // audit history (each record may carry up to 1000 change entries). `total`
    // still counts every parsed record.
    let mut total = 0usize;
    let mut recent: std::collections::VecDeque<serde_json::Value> =
        std::collections::VecDeque::with_capacity(limit.min(1024));
    for line in io::BufReader::new(file).lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) else {
            continue;
        };
        total += 1;
        let matches = match verb_filter {
            Some(want) => value.get("verb").and_then(|x| x.as_str()) == Some(want),
            None => true,
        };
        if !matches || limit == 0 {
            continue;
        }
        recent.push_back(value);
        if recent.len() > limit {
            recent.pop_front();
        }
    }
    // Most recent first.
    let entries: Vec<serde_json::Value> = recent.into_iter().rev().collect();
    (entries, total)
}

/// MCP handler for `xray_ledger`. READ-ONLY: resolves the ledger path beside the
/// graph snapshot and replays it; never writes, never persists (safe under
/// read-only attach).
pub fn handle_xray_ledger(
    state: &mut SessionState,
    input: XrayLedgerInput,
) -> M1ndResult<serde_json::Value> {
    let limit = input.limit.unwrap_or(LEDGER_DEFAULT_LIMIT);
    let path = ledger_path_for(state);
    let (entries, total_entries) = match &path {
        Some(p) => read_ledger(p, limit, input.verb.as_deref()),
        None => (Vec::new(), 0),
    };
    let output = XrayLedgerOutput {
        verb: "xray_ledger",
        entries,
        total_entries,
        ledger_path: path.map(|p| p.to_string_lossy().into_owned()),
    };
    serde_json::to_value(output).map_err(m1nd_core::error::M1ndError::Serde)
}

// ---------------------------------------------------------------------------
// Tests (pure logic against a Graph — no SessionState needed)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use m1nd_core::graph::{Graph, NodeProvenanceInput};
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
            expect_version: None,
        }
    }

    /// Same as `input` but with a cross-call OCC `expect_version` token.
    fn input_expect(
        selector: XraySelector,
        op: XrayTagOp,
        tags: &[&str],
        mode: XrayMode,
        expect_version: Option<String>,
    ) -> XrayRetagInput {
        XrayRetagInput {
            expect_version,
            ..input(selector, op, tags, mode)
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
    // xray_retag — cross-call OCC (expect_version)
    // -----------------------------------------------------------------------

    #[test]
    fn dry_run_returns_nonempty_version() {
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
        assert!(!out.version.is_empty(), "dry_run must surface a version");
        // 16-hex-char digest from the DefaultHasher fold.
        assert_eq!(out.version.len(), 16);
    }

    #[test]
    fn commit_with_matching_expect_version_succeeds() {
        let mut g = sample_graph();
        let sel = XraySelector {
            filter_tags: vec!["rust".to_string()],
            ..Default::default()
        };
        // Capture the version with a dry_run, then commit guarded by it.
        let dry = retag_graph(
            &mut g,
            &input(
                sel.clone(),
                XrayTagOp::Add,
                &["xray:bedrock"],
                XrayMode::DryRun,
            ),
        );
        let out = retag_graph(
            &mut g,
            &input_expect(
                sel,
                XrayTagOp::Add,
                &["xray:bedrock"],
                XrayMode::Commit,
                Some(dry.version.clone()),
            ),
        );
        assert_eq!(out.status, "committed");
        assert!(out.counts.applied > 0);
        assert_eq!(out.counts.applied, 3);
        assert_eq!(out.counts.conflicts, 0);
        assert!(g
            .node_tags(g.resolve_id("file::a.rs::fn::foo").unwrap())
            .contains(&"xray:bedrock"));
    }

    #[test]
    fn stale_expect_version_aborts_commit_and_mutates_nothing() {
        let mut g = sample_graph();
        let sel = XraySelector {
            filter_tags: vec!["rust".to_string()],
            ..Default::default()
        };
        // Capture the version at dry_run time.
        let dry = retag_graph(
            &mut g,
            &input(
                sel.clone(),
                XrayTagOp::Add,
                &["xray:bedrock"],
                XrayMode::DryRun,
            ),
        );
        let stale_version = dry.version.clone();

        // Simulate a CONCURRENT change: mutate a selected node's tags directly on
        // the graph between the caller's dry_run and its guarded commit. This
        // moves the live selection fingerprint away from `stale_version`.
        let victim = g.resolve_id("file::a.rs::fn::foo").unwrap();
        g.add_node_tags(victim, &["concurrent:edit"]);

        // Snapshot the post-tamper tag sets to prove the call mutates nothing.
        let before: Vec<Vec<String>> = [
            "file::a.rs::fn::foo",
            "file::b.rs::fn::bar",
            "file::a.rs::struct::Cfg",
        ]
        .iter()
        .map(|ext| {
            g.node_tags(g.resolve_id(ext).unwrap())
                .iter()
                .map(|s| s.to_string())
                .collect()
        })
        .collect();

        let out = retag_graph(
            &mut g,
            &input_expect(
                sel,
                XrayTagOp::Add,
                &["xray:bedrock"],
                XrayMode::Commit,
                Some(stale_version),
            ),
        );

        assert_eq!(out.status, "aborted_conflicts");
        assert_eq!(out.counts.applied, 0);
        assert!(out.counts.conflicts >= 1);
        // The reported version is the CURRENT one, so the caller can re-plan.
        assert!(!out.version.is_empty());

        // Graph is unchanged BY THIS CALL: no node gained `xray:bedrock`.
        for (i, ext) in [
            "file::a.rs::fn::foo",
            "file::b.rs::fn::bar",
            "file::a.rs::struct::Cfg",
        ]
        .iter()
        .enumerate()
        {
            let now: Vec<String> = g
                .node_tags(g.resolve_id(ext).unwrap())
                .iter()
                .map(|s| s.to_string())
                .collect();
            assert_eq!(
                now, before[i],
                "{ext} must be untouched by the aborted call"
            );
            assert!(
                !now.iter().any(|t| t == "xray:bedrock"),
                "{ext} must not gain the planned tag on abort"
            );
        }
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
            manifest_path: None,
        }
    }

    /// Test wrapper: resolve the inline manifest (no file, no workspace_root) and
    /// call the pure `orient_graph`. Keeps existing call-sites a one-name change.
    fn orient_g(graph: &Graph, input: &XrayOrientInput) -> XrayOrientOutput {
        let resolved = resolve_manifest(&input.manifest, input.manifest_path.as_deref(), None);
        orient_graph(graph, input, &resolved.manifest, resolved.source)
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
        let out = orient_g(&g, &orient_input(XrayManifest::default()));

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
        let out = orient_g(&g, &orient_input(manifest));

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
        let out = orient_g(&g, &orient_input(manifest));
        assert_eq!(out.counts.erosion_candidates, 1);
        assert_eq!(out.erosion_candidates[0].rule, "layer");

        // Reverse the order (modB below modA): modA -> modB is now downward
        // (allowed) -> converges, no candidate.
        let g2 = orient_graph_fixture();
        let manifest2 = XrayManifest {
            layer_order: vec!["modB".to_string(), "modA".to_string()],
            ..Default::default()
        };
        let out2 = orient_g(&g2, &orient_input(manifest2));
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
        let out = orient_g(&g, &orient_input(manifest));

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
            manifest_path: None,
        };
        let out = orient_g(&g, &input);
        assert_eq!(out.modules.get("modA"), Some(&2));
        assert_eq!(out.modules.get("modB"), None);
        assert_eq!(out.counts.modules, 1);
        // modA's source node is in scope, so the modA->modB edge still counts
        assert_eq!(out.dependency_matrix.get("modA->modB"), Some(&1));
    }

    // -----------------------------------------------------------------------
    // xray_apply — atomic physical-write engine (sandboxed in temp_dir)
    // -----------------------------------------------------------------------

    use std::sync::atomic::{AtomicU64, Ordering};

    const TEST_TAG: &str = "//! @xray:state:bedrock";

    /// Test seam: invoke the engine with a tamper callback fired between STAGE
    /// and REHASH (mirrors the Python `tamper(plan)`).
    fn apply_files_with_tamper(
        paths: &[PathBuf],
        transform: &XrayTransform,
        mode: XrayMode,
        tamper: impl Fn(&[PathBuf]),
    ) -> XrayApplyOutput {
        apply_files_inner(paths, transform, mode, None, Some(&tamper), None, None)
    }

    /// Test seam: invoke the engine with a `before_swap` callback fired after
    /// REHASH and before the SWAP loop, so a test can sabotage a specific
    /// target's rename to exercise the partial-swap path.
    fn apply_files_with_before_swap(
        paths: &[PathBuf],
        transform: &XrayTransform,
        mode: XrayMode,
        before_swap: impl Fn(&[(PathBuf, PathBuf)]),
    ) -> XrayApplyOutput {
        apply_files_inner(paths, transform, mode, None, None, Some(&before_swap), None)
    }

    fn ensure_tag() -> XrayTransform {
        XrayTransform::EnsureHeaderTag {
            tag: TEST_TAG.to_string(),
        }
    }

    /// Unique sandbox dir in `std::env::temp_dir()` — NEVER the real repo.
    fn fresh_sandbox() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("xray_apply_test_{}_{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Write `count` `.rs` files lacking the tag; return their paths.
    fn seed_untagged(dir: &Path, count: usize) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for i in 0..count {
            let p = dir.join(format!("file_{i}.rs"));
            std::fs::write(&p, format!("// file {i}\nfn main() {{}}\n")).unwrap();
            out.push(p);
        }
        out
    }

    fn first3_contains_tag(p: &Path) -> bool {
        let content = std::fs::read_to_string(p).unwrap();
        content
            .split_inclusive('\n')
            .take(3)
            .collect::<String>()
            .contains(TEST_TAG)
    }

    #[test]
    fn dry_run_plans_but_writes_nothing() {
        let dir = fresh_sandbox();
        let paths = seed_untagged(&dir, 4);

        let out = apply_files(&paths, &ensure_tag(), XrayMode::DryRun, None);
        assert_eq!(out.verb, "xray_apply");
        assert_eq!(out.status, "dry_run");
        assert_eq!(out.counts.matched, 4);
        assert_eq!(out.counts.planned, 4);
        assert_eq!(out.counts.applied, 0);
        assert!(!out.planned_sample.is_empty());

        // Nothing was written: every original is unchanged (tag absent).
        for p in &paths {
            assert!(!first3_contains_tag(p), "dry_run must not write {p:?}");
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn commit_applies_tag_to_all_files() {
        let dir = fresh_sandbox();
        let paths = seed_untagged(&dir, 4);

        let out = apply_files(&paths, &ensure_tag(), XrayMode::Commit, None);
        assert_eq!(out.status, "committed");
        assert_eq!(out.counts.applied, 4);
        assert_eq!(out.counts.planned, 4);
        assert_eq!(out.counts.conflicts, 0);

        // The tag now lives in the first 3 lines of every file.
        for p in &paths {
            assert!(first3_contains_tag(p), "commit must tag {p:?}");
        }
        // No staging temps left behind.
        assert!(no_temps_remain(&dir));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn idempotent_recommit_plans_zero() {
        let dir = fresh_sandbox();
        let paths = seed_untagged(&dir, 4);

        // First commit tags everything.
        let first = apply_files(&paths, &ensure_tag(), XrayMode::Commit, None);
        assert_eq!(first.counts.applied, 4);

        // Re-commit: the transform is now a no-op for every file.
        let again = apply_files(&paths, &ensure_tag(), XrayMode::Commit, None);
        assert_eq!(again.status, "committed");
        assert_eq!(again.counts.planned, 0);
        assert_eq!(again.counts.skipped_noop, 4);
        assert_eq!(again.counts.applied, 0);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn contended_apply_aborts_whole_batch() {
        let dir = fresh_sandbox();
        let paths = seed_untagged(&dir, 4);

        // Tamper: append bytes to ONE original between STAGE and REHASH so its
        // content-hash no longer matches the guard captured at SELECT.
        let victim = paths[2].clone();
        let out = apply_files_with_tamper(&paths, &ensure_tag(), XrayMode::Commit, move |_| {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&victim)
                .unwrap();
            f.write_all(b"\n// concurrent edit\n").unwrap();
        });

        assert_eq!(out.status, "aborted_conflicts");
        assert_eq!(out.counts.applied, 0);
        assert!(out.counts.conflicts >= 1);

        // Zero writes happened: NONE of the non-tampered originals carry the tag.
        for (i, p) in paths.iter().enumerate() {
            if i == 2 {
                continue; // the tampered file; appended text, never tagged
            }
            assert!(
                !first3_contains_tag(p),
                "abort must leave {p:?} untouched (no tag)"
            );
        }
        // No staging temps remain anywhere in the sandbox.
        assert!(no_temps_remain(&dir));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn plan_version_keys_in_path_so_identical_content_does_not_cancel() {
        // Two DIFFERENT files with the SAME content hash. With the old raw-XOR
        // fold (content hash only) these would cancel to 0; keying the path in
        // must keep the fold non-zero and path-sensitive.
        let same_hash = content_hash(b"identical bytes");
        let a = (PathBuf::from("/repo/a.rs"), same_hash.clone());
        let b = (PathBuf::from("/repo/b.rs"), same_hash.clone());
        let folded = plan_version(&[a.clone(), b.clone()]);
        assert_ne!(
            folded, "0000000000000000",
            "path keying must prevent cancel"
        );

        // Same two paths, one's content changes -> version must flip.
        let b_changed = (PathBuf::from("/repo/b.rs"), content_hash(b"other bytes"));
        let folded2 = plan_version(&[a, b_changed]);
        assert_ne!(folded, folded2, "a content change must flip the version");
    }

    #[test]
    fn partial_swap_after_first_success_reports_partial() {
        let dir = fresh_sandbox();
        let paths = seed_untagged(&dir, 4);
        // `matched` would be sorted in the handler; mirror that so the swap order
        // is deterministic and the FIRST rename lands before the sabotaged one.
        let mut ordered = paths.clone();
        ordered.sort();

        // Sabotage the SECOND target's rename: after REHASH (so it passes the
        // drift guard), replace temps[1]'s ORIGINAL with a directory. Renaming a
        // file over an existing directory fails on Unix and Windows, so the first
        // swap succeeds and the second fails -> partial.
        let victim_orig = ordered[1].clone();
        let out = apply_files_with_before_swap(
            &ordered,
            &ensure_tag(),
            XrayMode::Commit,
            move |_temps| {
                std::fs::remove_file(&victim_orig).unwrap();
                std::fs::create_dir(&victim_orig).unwrap();
            },
        );

        // The swap partially applied: file 0 is live, the rest are not.
        assert_eq!(out.status, "partial");
        assert_eq!(out.counts.applied, 1, "exactly the first file was swapped");
        assert!(out.counts.conflicts >= 1);
        // A partial swap left the graph stale versus disk.
        assert!(out.graph_resync_required);
        // The conflict sample names the file whose rename failed.
        assert!(
            out.conflicts_sample
                .iter()
                .any(|c| c.contains(&file_label(&ordered[1]))),
            "conflicts_sample must name the failing path: {:?}",
            out.conflicts_sample
        );

        // The first file is actually swapped (tagged, no temp left).
        assert!(first3_contains_tag(&ordered[0]), "file 0 must be swapped");
        let tmp0 = tmp_path_for(&ordered[0]);
        assert!(!tmp0.exists(), "swapped file's temp must be gone");

        // The NOT-yet-swapped temps are LEFT in place so a retry can complete
        // (we only clean up when nothing was swapped). ordered[2] / ordered[3]
        // were never renamed, so their temps must still exist.
        for p in [&ordered[2], &ordered[3]] {
            let tmp = tmp_path_for(p);
            assert!(
                tmp.exists(),
                "not-yet-swapped temp must be retained for retry: {tmp:?}"
            );
            assert!(
                !first3_contains_tag(p),
                "not-yet-swapped original must be untouched: {p:?}"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// True if no `*.xray.tmp` file remains directly in `dir`.
    fn no_temps_remain(dir: &Path) -> bool {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().ends_with(".xray.tmp"))
    }

    // -----------------------------------------------------------------------
    // xray_apply — cross-call OCC (expect_version)
    // -----------------------------------------------------------------------

    #[test]
    fn apply_dry_run_returns_version() {
        let dir = fresh_sandbox();
        let paths = seed_untagged(&dir, 4);

        let out = apply_files(&paths, &ensure_tag(), XrayMode::DryRun, None);
        assert_eq!(out.status, "dry_run");
        assert!(!out.version.is_empty(), "dry_run must surface a version");
        assert_eq!(out.version.len(), 16);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_commit_with_matching_expect_version_applies() {
        let dir = fresh_sandbox();
        let paths = seed_untagged(&dir, 4);

        // Capture the version via dry_run, then commit guarded by it.
        let dry = apply_files(&paths, &ensure_tag(), XrayMode::DryRun, None);
        let out = apply_files(
            &paths,
            &ensure_tag(),
            XrayMode::Commit,
            Some(dry.version.as_str()),
        );
        assert_eq!(out.status, "committed");
        assert_eq!(out.counts.applied, 4);
        assert_eq!(out.counts.conflicts, 0);
        for p in &paths {
            assert!(first3_contains_tag(p), "commit must tag {p:?}");
        }
        assert!(no_temps_remain(&dir));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_stale_expect_version_aborts_before_staging() {
        let dir = fresh_sandbox();
        let paths = seed_untagged(&dir, 4);

        // Capture the version with a dry_run.
        let dry = apply_files(&paths, &ensure_tag(), XrayMode::DryRun, None);
        let stale_version = dry.version.clone();

        // Externally modify ONE target file between dry_run and the guarded
        // commit (a concurrent ingest / another agent). The planned-files
        // fingerprint now differs from `stale_version`.
        {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&paths[1])
                .unwrap();
            f.write_all(b"\n// concurrent edit\n").unwrap();
        }
        let tampered_after = std::fs::read_to_string(&paths[1]).unwrap();

        let out = apply_files(
            &paths,
            &ensure_tag(),
            XrayMode::Commit,
            Some(stale_version.as_str()),
        );
        assert_eq!(out.status, "aborted_conflicts");
        assert_eq!(out.counts.applied, 0);
        assert!(out.counts.conflicts >= 1);

        // NO file was written: no original carries the tag, and the externally
        // modified file still holds exactly its concurrent edit (untouched here).
        for p in &paths {
            assert!(!first3_contains_tag(p), "aborted commit must not tag {p:?}");
        }
        assert_eq!(
            std::fs::read_to_string(&paths[1]).unwrap(),
            tampered_after,
            "the concurrently edited file must be left exactly as the edit left it"
        );
        // Nothing was staged.
        assert!(no_temps_remain(&dir));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// FIX A — STAGE must never write through a pre-existing SYMLINK temp.
    ///
    /// A workspace contains `victim.rs` (planned for tagging) AND a sibling
    /// `victim.rs.xray.tmp` that is a symlink pointing OUTSIDE the sandbox root.
    /// The old `File::create(tmp)` followed that symlink and truncated/wrote its
    /// target, then renamed it over `victim.rs` — escaping root confinement. With
    /// create-new + symlink_metadata pre-check the commit must ABORT, write ZERO
    /// originals, and leave the OUTSIDE target byte-for-byte intact.
    #[cfg(unix)]
    #[test]
    fn stage_refuses_preexisting_symlink_temp_and_does_not_escape_root() {
        use std::os::unix::fs::symlink;

        let dir = fresh_sandbox();

        // The OUTSIDE target the attacker's symlink points at — a sibling of the
        // sandbox dir, NOT under it. Seed it with known content we will assert is
        // never modified.
        let outside = dir
            .parent()
            .unwrap()
            .join(format!("xray_escape_target_{}.txt", std::process::id()));
        let outside_content = b"OUTSIDE-DO-NOT-TOUCH\n";
        std::fs::write(&outside, outside_content).unwrap();

        // The planned victim file (untagged .rs) and a control file that WOULD be
        // tagged if the batch were not aborted all-or-nothing.
        let victim = dir.join("victim.rs");
        std::fs::write(&victim, "// victim\nfn main() {}\n").unwrap();
        let control = dir.join("control.rs");
        std::fs::write(&control, "// control\nfn main() {}\n").unwrap();

        // Plant the malicious temp: victim.rs.xray.tmp -> the OUTSIDE target.
        let mal_tmp = tmp_path_for(&victim);
        symlink(&outside, &mal_tmp).unwrap();

        // victim sorts before control, so victim is staged first and trips the
        // symlink guard before anything else is written.
        let mut paths = vec![victim.clone(), control.clone()];
        paths.sort();

        let out = apply_files(&paths, &ensure_tag(), XrayMode::Commit, None);

        // (1) The commit aborted — nothing was committed.
        assert_eq!(
            out.status, "aborted_conflicts",
            "a pre-existing symlink temp must abort the commit"
        );
        assert_eq!(out.counts.applied, 0, "abort must write zero originals");
        assert!(out.counts.conflicts >= 1);
        assert!(
            out.conflicts_sample
                .iter()
                .any(|c| c.contains("victim.rs") && c.contains("symlink")),
            "conflict must name the refused symlink temp: {:?}",
            out.conflicts_sample
        );

        // (2) The OUTSIDE target is UNCHANGED — not truncated, not written.
        assert_eq!(
            std::fs::read(&outside).unwrap(),
            outside_content,
            "the symlink target OUTSIDE root must be byte-for-byte intact"
        );

        // (3) victim.rs and control.rs are unchanged (all-or-nothing abort).
        assert!(!first3_contains_tag(&victim), "victim.rs must be untouched");
        assert!(
            !first3_contains_tag(&control),
            "control.rs must be untouched"
        );

        // We refused — never removed — the attacker's symlink temp; it is still
        // a symlink, proving we did not silently delete/retry it.
        let meta = std::fs::symlink_metadata(&mal_tmp).unwrap();
        assert!(
            meta.file_type().is_symlink(),
            "the refused symlink temp must be left in place, not removed"
        );

        let _ = std::fs::remove_file(&mal_tmp);
        let _ = std::fs::remove_file(&outside);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// FIX B — a non-UTF-8 (binary) file must be SKIPPED, never corrupted.
    ///
    /// An empty-extension selector can match a binary file. The old
    /// `String::from_utf8_lossy` replaced invalid bytes with U+FFFD and staged
    /// that corruption back. Now: invalid UTF-8 is skipped (counted as
    /// skipped_binary, never planned/applied) while a sibling valid `.rs` is
    /// still tagged.
    #[test]
    fn commit_skips_binary_file_and_tags_valid_sibling() {
        let dir = fresh_sandbox();

        // A genuinely invalid-UTF-8 file (lone 0xff/0xfe are not valid UTF-8).
        let binary = dir.join("blob.bin");
        let binary_bytes: &[u8] = &[0xff, 0xfe, 0x00, 0x01];
        std::fs::write(&binary, binary_bytes).unwrap();

        // A valid sibling that SHOULD be tagged.
        let valid = dir.join("ok.rs");
        std::fs::write(&valid, "// ok\nfn main() {}\n").unwrap();

        let mut paths = vec![binary.clone(), valid.clone()];
        paths.sort();

        let out = apply_files(&paths, &ensure_tag(), XrayMode::Commit, None);

        assert_eq!(out.status, "committed");
        // The binary file is skipped as binary: never planned, never applied.
        assert_eq!(out.counts.skipped_binary, 1, "binary file must be skipped");
        assert_eq!(out.counts.planned, 1, "only the valid sibling is planned");
        assert_eq!(out.counts.applied, 1, "only the valid sibling is written");

        // The binary file is byte-for-byte intact (not corrupted by U+FFFD).
        assert_eq!(
            std::fs::read(&binary).unwrap(),
            binary_bytes,
            "binary file must be left exactly as it was"
        );
        // No skipped binary leaked into the planned sample.
        assert!(
            !out.planned_sample
                .iter()
                .any(|s| s.path.ends_with("blob.bin")),
            "a skipped binary must never appear in planned_sample"
        );

        // The valid sibling IS tagged.
        assert!(first3_contains_tag(&valid), "valid .rs must be tagged");
        assert!(no_temps_remain(&dir));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // AnnotateSymbol — graph-driven AST-targeted apply (no re-parse)
    // -----------------------------------------------------------------------

    /// Drive the AnnotateSymbol path exactly as `handle_xray_apply` does:
    /// PHASE A collect targets from the graph (read), PHASE B build the plan from
    /// files, then the SHARED atomic applier (no test seams). `root` must be the
    /// CANONICAL sandbox root (the handler canonicalizes before confinement).
    fn annotate(
        graph: &Graph,
        root: &Path,
        annotation: &str,
        node_type: Option<u8>,
        path_prefix: Option<&str>,
        mode: XrayMode,
    ) -> XrayApplyOutput {
        annotate_ext(graph, root, annotation, node_type, path_prefix, &[], mode)
    }

    /// Like [`annotate`] but with an explicit `extensions` filter, mirroring the
    /// `XrayFileSelector.extensions` semantics on the graph-driven path.
    fn annotate_ext(
        graph: &Graph,
        root: &Path,
        annotation: &str,
        node_type: Option<u8>,
        path_prefix: Option<&str>,
        extensions: &[String],
        mode: XrayMode,
    ) -> XrayApplyOutput {
        let (targets, symbols_matched) =
            collect_symbol_targets(graph, root, node_type, path_prefix, extensions);
        let (plan, counts) = build_annotate_plan(targets, annotation, symbols_matched);
        run_atomic_apply(plan, counts, mode, None, None, None, None)
    }

    /// Build a graph with two Function symbols and one Struct symbol, all whose
    /// provenance points at `modA/src/x.rs`, plus a sandbox file ≥8 lines.
    /// Returns `(graph, canonical_root, file_path)`.
    fn annotate_fixture() -> (Graph, PathBuf, PathBuf) {
        let root = fresh_sandbox();
        let root = root.canonicalize().unwrap();
        let rel = "modA/src/x.rs";
        let file = root.join(rel);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        // 10 lines. fn foo at line 3, struct Cfg at line 6, fn bar at line 8.
        let body = "// header\n\
                    use std::io;\n\
                    fn foo() {}\n\
                    \n\
                    // a comment\n\
                    struct Cfg {}\n\
                    \n\
                    pub fn bar() {}\n\
                    // tail\n\
                    // eof\n";
        std::fs::write(&file, body).unwrap();

        let mut g = Graph::new();
        let foo = g
            .add_node(
                "file::modA/src/x.rs::fn::foo",
                "foo",
                NodeType::Function,
                &[],
                0.0,
                0.0,
            )
            .unwrap();
        g.set_node_provenance(
            foo,
            NodeProvenanceInput {
                source_path: Some(rel),
                line_start: Some(3),
                line_end: Some(3),
                ..Default::default()
            },
        );
        let cfg = g
            .add_node(
                "file::modA/src/x.rs::struct::Cfg",
                "Cfg",
                NodeType::Struct,
                &[],
                0.0,
                0.0,
            )
            .unwrap();
        g.set_node_provenance(
            cfg,
            NodeProvenanceInput {
                source_path: Some(rel),
                line_start: Some(6),
                line_end: Some(6),
                ..Default::default()
            },
        );
        let bar = g
            .add_node(
                "file::modA/src/x.rs::fn::bar",
                "bar",
                NodeType::Function,
                &[],
                0.0,
                0.0,
            )
            .unwrap();
        g.set_node_provenance(
            bar,
            NodeProvenanceInput {
                source_path: Some(rel),
                line_start: Some(8),
                line_end: Some(8),
                ..Default::default()
            },
        );
        g.finalize().unwrap();
        (g, root, file)
    }

    const ANNOT: &str = "// @xray:reviewed";

    #[test]
    fn annotate_dry_run_matches_symbols_writes_nothing() {
        let (g, root, file) = annotate_fixture();
        let before = std::fs::read_to_string(&file).unwrap();

        // Two functions match node_type=Function (foo, bar); struct excluded.
        let out = annotate(
            &g,
            &root,
            ANNOT,
            Some(node_type_to_u8(NodeType::Function)),
            None,
            XrayMode::DryRun,
        );
        assert_eq!(out.status, "dry_run");
        assert_eq!(out.counts.symbols_matched, 2, "AST selected 2 functions");
        assert_eq!(out.counts.planned, 1, "both functions live in one file");
        assert_eq!(out.counts.applied, 0);

        // Disk untouched on dry_run.
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn annotate_commit_inserts_above_each_function_bottom_up() {
        let (g, root, file) = annotate_fixture();

        let out = annotate(
            &g,
            &root,
            ANNOT,
            Some(node_type_to_u8(NodeType::Function)),
            None,
            XrayMode::Commit,
        );
        assert_eq!(out.status, "committed");
        assert_eq!(out.counts.symbols_matched, 2);
        assert_eq!(out.counts.applied, 1);
        assert!(out.graph_resync_required, "a write must flag resync");

        let lines: Vec<String> = std::fs::read_to_string(&file)
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        // The annotation must sit immediately above `fn foo` and `pub fn bar`,
        // and NOT above the struct (node_type filtered it out). Bottom-up insert
        // keeps both placements correct in the same file.
        let foo_i = lines.iter().position(|l| l.contains("fn foo")).unwrap();
        assert_eq!(lines[foo_i - 1].trim(), ANNOT, "annotation above fn foo");
        let bar_i = lines.iter().position(|l| l.contains("pub fn bar")).unwrap();
        assert_eq!(
            lines[bar_i - 1].trim(),
            ANNOT,
            "annotation above pub fn bar"
        );
        // The struct line must NOT be annotated (its preceding line is unchanged).
        let cfg_i = lines.iter().position(|l| l.contains("struct Cfg")).unwrap();
        assert_ne!(lines[cfg_i - 1].trim(), ANNOT, "struct must be untouched");
        // Exactly two annotation lines were inserted (no extras).
        let n_annot = lines.iter().filter(|l| l.trim() == ANNOT).count();
        assert_eq!(n_annot, 2, "exactly 2 inserts");

        assert!(no_temps_remain(&root));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn annotate_is_idempotent_on_recommit() {
        let (mut g, root, file) = annotate_fixture();
        let first = annotate(
            &g,
            &root,
            ANNOT,
            Some(node_type_to_u8(NodeType::Function)),
            None,
            XrayMode::Commit,
        );
        assert_eq!(first.counts.applied, 1);
        let after_first = std::fs::read_to_string(&file).unwrap();

        // The verb signalled `graph_resync_required` on the first write: the file
        // bytes changed but the in-memory graph still holds STALE line numbers.
        // The honest contract is that a correct re-run happens AFTER re-ingest,
        // which refreshes provenance to the new line positions. Simulate that
        // re-ingest by re-pointing each symbol's provenance at its new line
        // (foo shifted 3->4, bar shifted 8->10 by the two inserts above).
        g.set_node_provenance(
            g.resolve_id("file::modA/src/x.rs::fn::foo").unwrap(),
            NodeProvenanceInput {
                source_path: Some("modA/src/x.rs"),
                line_start: Some(4),
                line_end: Some(4),
                ..Default::default()
            },
        );
        g.set_node_provenance(
            g.resolve_id("file::modA/src/x.rs::fn::bar").unwrap(),
            NodeProvenanceInput {
                source_path: Some("modA/src/x.rs"),
                line_start: Some(10),
                line_end: Some(10),
                ..Default::default()
            },
        );

        // Re-run on the refreshed graph: every target's preceding line already
        // equals the annotation, so nothing is planned and no duplicates stack.
        let second = annotate(
            &g,
            &root,
            ANNOT,
            Some(node_type_to_u8(NodeType::Function)),
            None,
            XrayMode::Commit,
        );
        assert_eq!(second.status, "committed");
        assert_eq!(second.counts.symbols_matched, 2, "AST still matched 2");
        assert_eq!(second.counts.planned, 0, "idempotent: nothing to do");
        assert_eq!(second.counts.applied, 0);
        // File byte-identical to after the first run (no duplicate annotations).
        assert_eq!(std::fs::read_to_string(&file).unwrap(), after_first);
        let n_annot = after_first.lines().filter(|l| l.trim() == ANNOT).count();
        assert_eq!(n_annot, 2, "no duplicate annotation lines");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn annotate_skips_symbol_whose_source_escapes_root() {
        let (mut g, root, _file) = annotate_fixture();
        // A symbol whose provenance source_path climbs OUT of root via `..`.
        let escapee = g
            .add_node(
                "file::escape/src/evil.rs::fn::evil",
                "evil",
                NodeType::Function,
                &[],
                0.0,
                0.0,
            )
            .unwrap();
        g.set_node_provenance(
            escapee,
            NodeProvenanceInput {
                source_path: Some("../../../../etc/evil.rs"),
                line_start: Some(1),
                line_end: Some(1),
                ..Default::default()
            },
        );

        let (targets, symbols_matched) = collect_symbol_targets(
            &g,
            &root,
            Some(node_type_to_u8(NodeType::Function)),
            None,
            &[],
        );
        // foo + bar are confined; the escapee is dropped BEFORE it can be a target.
        assert_eq!(symbols_matched, 2, "escaping symbol must not be matched");
        assert!(
            targets.iter().all(|(p, _)| p.starts_with(&root)),
            "every target must be confined under root"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn annotate_path_prefix_scopes_by_module() {
        let (g, root, file) = annotate_fixture();
        // Module prefix that matches modA -> both functions selected.
        let hit = annotate(
            &g,
            &root,
            ANNOT,
            Some(node_type_to_u8(NodeType::Function)),
            Some("modA"),
            XrayMode::DryRun,
        );
        assert_eq!(hit.counts.symbols_matched, 2);
        // A prefix matching nothing -> zero symbols, zero plan.
        let miss = annotate(
            &g,
            &root,
            ANNOT,
            Some(node_type_to_u8(NodeType::Function)),
            Some("modZ"),
            XrayMode::DryRun,
        );
        assert_eq!(miss.counts.symbols_matched, 0);
        assert_eq!(miss.counts.planned, 0);
        // file unchanged either way (dry_run).
        assert!(std::fs::read_to_string(&file).unwrap().lines().count() >= 8);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn annotate_extensions_filter_excludes_non_matching_symbols() {
        // FIX 2: the graph-driven path must honor `selector.extensions` exactly
        // like the file-driven walk. A `.rs` function and a `.py` function both
        // carry provenance; `extensions: ["rs"]` must select ONLY the `.rs` one.
        let root = fresh_sandbox();
        let root = root.canonicalize().unwrap();

        let rs_rel = "src/a.rs";
        let rs_file = root.join(rs_rel);
        std::fs::create_dir_all(rs_file.parent().unwrap()).unwrap();
        std::fs::write(&rs_file, "// l1\n// l2\nfn rs_fn() {}\n// l4\n").unwrap();

        let py_rel = "src/b.py";
        let py_file = root.join(py_rel);
        std::fs::write(&py_file, "# l1\n# l2\ndef py_fn():\n    pass\n").unwrap();

        let mut g = Graph::new();
        let rs = g
            .add_node(
                "file::src/a.rs::fn::rs_fn",
                "rs_fn",
                NodeType::Function,
                &[],
                0.0,
                0.0,
            )
            .unwrap();
        g.set_node_provenance(
            rs,
            NodeProvenanceInput {
                source_path: Some(rs_rel),
                line_start: Some(3),
                line_end: Some(3),
                ..Default::default()
            },
        );
        let py = g
            .add_node(
                "file::src/b.py::fn::py_fn",
                "py_fn",
                NodeType::Function,
                &[],
                0.0,
                0.0,
            )
            .unwrap();
        g.set_node_provenance(
            py,
            NodeProvenanceInput {
                source_path: Some(py_rel),
                line_start: Some(3),
                line_end: Some(3),
                ..Default::default()
            },
        );
        g.finalize().unwrap();

        // extensions: ["rs"] -> only the .rs symbol matches.
        let only_rs = annotate_ext(
            &g,
            &root,
            ANNOT,
            Some(node_type_to_u8(NodeType::Function)),
            None,
            &["rs".to_string()],
            XrayMode::DryRun,
        );
        assert_eq!(
            only_rs.counts.symbols_matched, 1,
            "extensions=[rs] must exclude the .py symbol"
        );

        // empty extensions -> both symbols match (unchanged behavior).
        let both = annotate_ext(
            &g,
            &root,
            ANNOT,
            Some(node_type_to_u8(NodeType::Function)),
            None,
            &[],
            XrayMode::DryRun,
        );
        assert_eq!(both.counts.symbols_matched, 2, "empty extensions = any");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn annotate_omitted_node_type_excludes_file_and_module_nodes() {
        // FIX 3: `node_type: None` means "any SYMBOL type". A File node and a
        // Module node now carry provenance after ingest, but they are containers,
        // not symbols — omitting node_type must NOT annotate them. Only the
        // Function (a symbol) is selected.
        let root = fresh_sandbox();
        let root = root.canonicalize().unwrap();
        let rel = "src/c.rs";
        let file = root.join(rel);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "// l1\n// l2\nfn real_fn() {}\n// l4\n").unwrap();

        let mut g = Graph::new();
        // A File node pointing at line 1 of the file (containers get provenance too).
        let file_node = g
            .add_node("file::src/c.rs", "c.rs", NodeType::File, &[], 0.0, 0.0)
            .unwrap();
        g.set_node_provenance(
            file_node,
            NodeProvenanceInput {
                source_path: Some(rel),
                line_start: Some(1),
                line_end: Some(4),
                ..Default::default()
            },
        );
        // A Module node, also with provenance.
        let mod_node = g
            .add_node(
                "file::src/c.rs::mod::inner",
                "inner",
                NodeType::Module,
                &[],
                0.0,
                0.0,
            )
            .unwrap();
        g.set_node_provenance(
            mod_node,
            NodeProvenanceInput {
                source_path: Some(rel),
                line_start: Some(2),
                line_end: Some(2),
                ..Default::default()
            },
        );
        // The actual symbol.
        let fn_node = g
            .add_node(
                "file::src/c.rs::fn::real_fn",
                "real_fn",
                NodeType::Function,
                &[],
                0.0,
                0.0,
            )
            .unwrap();
        g.set_node_provenance(
            fn_node,
            NodeProvenanceInput {
                source_path: Some(rel),
                line_start: Some(3),
                line_end: Some(3),
                ..Default::default()
            },
        );
        g.finalize().unwrap();

        // node_type omitted -> only the Function symbol is selected; File + Module
        // (containers) are excluded.
        let (targets, matched) = collect_symbol_targets(&g, &root, None, None, &[]);
        assert_eq!(
            matched, 1,
            "omitted node_type must match only the symbol, not File/Module"
        );
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].1, 3, "the one target is the function at line 3");

        // Sanity: explicitly requesting File still matches the File node (exact
        // match path is unchanged when node_type IS set).
        let (_, file_matched) =
            collect_symbol_targets(&g, &root, Some(node_type_to_u8(NodeType::File)), None, &[]);
        assert_eq!(file_matched, 1, "explicit node_type=File still matches it");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn safety_skips_forbidden_and_outside_root() {
        // Forbidden: runtime artifacts, VCS/build segments, our own temps.
        assert!(is_forbidden_path(Path::new("/x/graph_snapshot.json")));
        assert!(is_forbidden_path(Path::new("/x/plasticity_state.json")));
        assert!(is_forbidden_path(Path::new("/x/anything_state.json")));
        assert!(is_forbidden_path(Path::new("/x/daemon_alerts.json")));
        assert!(is_forbidden_path(Path::new("/x/document_cache_index.json")));
        assert!(is_forbidden_path(Path::new("/x/ingest_roots.json")));
        assert!(is_forbidden_path(Path::new("/repo/target/debug/foo.rs")));
        assert!(is_forbidden_path(Path::new("/repo/.git/config")));
        assert!(is_forbidden_path(Path::new(
            "/repo/node_modules/pkg/index.js"
        )));
        assert!(is_forbidden_path(Path::new("/repo/src/foo.rs.xray.tmp")));
        // Allowed: a normal source file.
        assert!(!is_forbidden_path(Path::new("/repo/src/foo.rs")));
    }

    // -----------------------------------------------------------------------
    // xray_gate — North-Star pre-edit guardrail (read-only)
    // -----------------------------------------------------------------------

    /// Build a gate input over `node` with the given manifest / planned imports.
    fn gate_input(
        node: &str,
        planned_imports: &[&str],
        manifest: XrayManifest,
        manifest_ratified: bool,
    ) -> XrayGateInput {
        XrayGateInput {
            node: node.to_string(),
            planned_imports: planned_imports.iter().map(|s| s.to_string()).collect(),
            manifest,
            manifest_ratified,
            manifest_path: None,
        }
    }

    /// Test wrapper: resolve the inline manifest (no file, no workspace_root),
    /// compute the effective ratified flag the same way the handler does, and
    /// call the pure `gate_graph`. Keeps existing call-sites a one-name change.
    fn gate_g(graph: &Graph, input: &XrayGateInput) -> XrayGateOutput {
        let resolved = resolve_manifest(&input.manifest, input.manifest_path.as_deref(), None);
        let effective_ratified = if resolved.source == "inline" {
            input.manifest_ratified
        } else {
            resolved.ratified
        };
        gate_graph(
            graph,
            input,
            &resolved.manifest,
            effective_ratified,
            resolved.source,
        )
    }

    fn forbid_a_to_b() -> XrayManifest {
        XrayManifest {
            forbid: vec![("modA".to_string(), "modB".to_string())],
            ..Default::default()
        }
    }

    #[test]
    fn gate_empty_manifest_is_clear() {
        // (a) empty manifest -> a node in modA gates "clear".
        let g = orient_graph_fixture();
        let out = gate_g(
            &g,
            &gate_input(
                "file::modA/src/lib.rs::fn::a_main",
                &[],
                XrayManifest::default(),
                false,
            ),
        );
        assert_eq!(out.verb, "xray_gate");
        assert_eq!(out.node_module, "modA");
        assert_eq!(out.verdict, "clear");
        assert!(out.existing_violations.is_empty());
        assert!(out.planned_violations.is_empty());
    }

    #[test]
    fn gate_existing_violation_unratified_is_caution() {
        // (b) forbid (modA,modB), modA node that imports modB, NOT ratified
        //     -> "caution", existing_violations has 1.
        let g = orient_graph_fixture();
        let out = gate_g(
            &g,
            &gate_input(
                "file::modA/src/lib.rs::fn::a_main",
                &[],
                forbid_a_to_b(),
                false,
            ),
        );
        assert_eq!(out.verdict, "caution");
        assert_eq!(out.existing_violations.len(), 1);
        let v = &out.existing_violations[0];
        assert_eq!(v.from_module, "modA");
        assert_eq!(v.to_module, "modB");
        assert_eq!(v.rule, "forbid");
        assert_eq!(v.kind, "existing");
        assert!(out.planned_violations.is_empty());
    }

    #[test]
    fn gate_existing_violation_ratified_is_blocked() {
        // (c) same as (b) but manifest_ratified: true -> "blocked".
        let g = orient_graph_fixture();
        let out = gate_g(
            &g,
            &gate_input(
                "file::modA/src/lib.rs::fn::a_main",
                &[],
                forbid_a_to_b(),
                true,
            ),
        );
        assert_eq!(out.verdict, "blocked");
        assert_eq!(out.existing_violations.len(), 1);
    }

    #[test]
    fn gate_planned_import_violation_ratified_is_blocked() {
        // (d) planned_imports ["modB"] from a modA node with forbid (modA,modB),
        //     ratified -> "blocked" with a planned_violation. Use a_util, which
        //     has NO outgoing edges, so the block comes purely from the plan.
        let g = orient_graph_fixture();
        let out = gate_g(
            &g,
            &gate_input(
                "file::modA/src/util.rs::fn::a_util",
                &["modB"],
                forbid_a_to_b(),
                true,
            ),
        );
        assert_eq!(out.verdict, "blocked");
        assert!(
            out.existing_violations.is_empty(),
            "a_util has no outgoing edges"
        );
        assert_eq!(out.planned_violations.len(), 1);
        let v = &out.planned_violations[0];
        assert_eq!(v.from_module, "modA");
        assert_eq!(v.to_module, "modB");
        assert_eq!(v.rule, "forbid");
        assert_eq!(v.kind, "planned");
    }

    #[test]
    fn gate_unknown_node_is_clear() {
        // (e) unknown node external_id -> "clear".
        let g = orient_graph_fixture();
        let out = gate_g(
            &g,
            &gate_input(
                "file::nope/does/not::exist",
                &["modB"],
                forbid_a_to_b(),
                true,
            ),
        );
        assert_eq!(out.verdict, "clear");
        assert_eq!(out.node_module, "");
        assert!(out.existing_violations.is_empty());
        assert!(out.planned_violations.is_empty());
        assert!(out.reasons.iter().any(|r| r.contains("not in graph")));
    }

    #[test]
    fn gate_layer_rule_also_routes_through_shared_predicate() {
        // Cross-check: the layer axis (not just forbid) gates too, proving gate
        // and orient share the one `classify_edge` predicate.
        let g = orient_graph_fixture();
        let manifest = XrayManifest {
            layer_order: vec!["modA".to_string(), "modB".to_string()],
            ..Default::default()
        };
        let out = gate_g(
            &g,
            &gate_input("file::modA/src/lib.rs::fn::a_main", &[], manifest, true),
        );
        assert_eq!(out.verdict, "blocked");
        assert_eq!(out.existing_violations.len(), 1);
        assert_eq!(out.existing_violations[0].rule, "layer");
    }

    // -----------------------------------------------------------------------
    // xray_paint — persist structural proof-state tags (dry-run/commit)
    // -----------------------------------------------------------------------

    /// Three modules with reference edges chosen to exercise every state:
    ///   - modA/a_main  (0): imports modB/b_core -> a cross-module SOURCE
    ///   - modB/b_core  (2): TARGET of modA's import -> reference in-degree 1
    ///   - modA/a_util  (1): no incoming reference edges -> orphan
    ///
    /// Finalized so the CSR is populated.
    fn paint_graph_fixture() -> Graph {
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
        .unwrap(); // 1 — orphan (nothing references it)
        g.add_node(
            "file::modB/src/lib.rs::fn::b_core",
            "b_core",
            NodeType::Function,
            &["rust"],
            0.0,
            0.0,
        )
        .unwrap(); // 2 — referenced by modA -> bedrock

        // cross-module reference edge modA -> modB (imports). Makes b_core
        // referenced (in-degree 1) AND a_main a cross-module source.
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
        g.finalize().unwrap();
        g
    }

    fn paint_input(scope: Option<&str>, manifest: XrayManifest, mode: XrayMode) -> XrayPaintInput {
        XrayPaintInput {
            scope: scope.map(|s| s.to_string()),
            manifest,
            manifest_path: None,
            mode,
        }
    }

    /// Test wrapper: resolve the inline manifest (no file, no workspace_root) and
    /// call the pure `paint_graph`. Keeps existing call-sites a one-name change.
    fn paint_g(graph: &mut Graph, input: &XrayPaintInput) -> XrayPaintOutput {
        let resolved = resolve_manifest(&input.manifest, input.manifest_path.as_deref(), None);
        paint_graph(graph, input, &resolved.manifest, resolved.source)
    }

    /// The single `xray:state:*` tag on a node, if any (test helper).
    fn state_tag_of(g: &Graph, ext: &str) -> Vec<String> {
        g.node_tags(g.resolve_id(ext).unwrap())
            .iter()
            .filter(|t| t.starts_with(STATE_TAG_PREFIX))
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn paint_dry_run_counts_split_and_writes_nothing() {
        let mut g = paint_graph_fixture();
        let out = paint_g(
            &mut g,
            &paint_input(None, XrayManifest::default(), XrayMode::DryRun),
        );

        assert_eq!(out.verb, "xray_paint");
        assert_eq!(out.status, "dry_run");
        assert_eq!(out.counts.scanned, 3);
        // Evidence-based classifier: nothing is test-exercised or grounded.
        // b_core is referenced from a NON-test source (a_main) with no proof
        // evidence -> unproven (NOT bedrock). a_main + a_util have zero incoming
        // reference edges -> overgrowth.
        assert_eq!(out.counts.bedrock, 0);
        assert_eq!(out.counts.unproven, 1);
        assert_eq!(out.counts.overgrowth, 2);
        assert_eq!(out.counts.erosion_candidate, 0);
        // proof_coverage = bedrock(0) / scanned(3) = 0.0; no manifest source.
        assert_eq!(out.proof_coverage, 0.0);
        assert_eq!(out.manifest_source, "none");
        // dry_run writes nothing.
        assert_eq!(out.counts.painted, 0);
        assert!(!out.version.is_empty());
        assert_eq!(out.version.len(), 16);

        // No state tag was written on any node.
        for ext in [
            "file::modA/src/lib.rs::fn::a_main",
            "file::modA/src/util.rs::fn::a_util",
            "file::modB/src/lib.rs::fn::b_core",
        ] {
            assert!(state_tag_of(&g, ext).is_empty(), "{ext} must be unpainted");
        }
    }

    #[test]
    fn paint_commit_writes_state_tags() {
        let mut g = paint_graph_fixture();
        let out = paint_g(
            &mut g,
            &paint_input(None, XrayManifest::default(), XrayMode::Commit),
        );

        assert_eq!(out.status, "committed");
        assert_eq!(out.counts.scanned, 3);
        assert_eq!(out.counts.bedrock, 0);
        assert_eq!(out.counts.unproven, 1);
        assert_eq!(out.counts.overgrowth, 2);
        assert_eq!(out.counts.painted, 3);

        // Referenced from a non-test source, no proof evidence -> unproven.
        assert_eq!(
            state_tag_of(&g, "file::modB/src/lib.rs::fn::b_core"),
            vec!["xray:state:unproven".to_string()]
        );
        // Orphan node -> overgrowth.
        assert_eq!(
            state_tag_of(&g, "file::modA/src/util.rs::fn::a_util"),
            vec!["xray:state:overgrowth".to_string()]
        );
    }

    #[test]
    fn paint_repaint_is_idempotent_no_accumulation() {
        let mut g = paint_graph_fixture();

        // First commit paints everything.
        let first = paint_g(
            &mut g,
            &paint_input(None, XrayManifest::default(), XrayMode::Commit),
        );
        assert_eq!(first.counts.painted, 3);

        // Second commit: every node already carries exactly its computed state,
        // so nothing is painted again — and exactly ONE state tag per node.
        let second = paint_g(
            &mut g,
            &paint_input(None, XrayManifest::default(), XrayMode::Commit),
        );
        assert_eq!(second.counts.painted, 0);

        for ext in [
            "file::modA/src/lib.rs::fn::a_main",
            "file::modA/src/util.rs::fn::a_util",
            "file::modB/src/lib.rs::fn::b_core",
        ] {
            assert_eq!(
                state_tag_of(&g, ext).len(),
                1,
                "{ext} must carry exactly one xray:state:* tag after re-paint"
            );
        }
    }

    #[test]
    fn paint_manifest_flags_erosion_candidate_source() {
        let mut g = paint_graph_fixture();
        // Forbid modA -> modB: a_main is the SOURCE of that cross-module import,
        // so it is reclassified bedrock/overgrowth -> erosion-candidate.
        let manifest = XrayManifest {
            forbid: vec![("modA".to_string(), "modB".to_string())],
            ..Default::default()
        };
        let out = paint_g(&mut g, &paint_input(None, manifest, XrayMode::Commit));

        assert_eq!(out.status, "committed");
        assert_eq!(out.counts.erosion_candidate, 1);
        // a_main is the flagged source.
        assert_eq!(
            state_tag_of(&g, "file::modA/src/lib.rs::fn::a_main"),
            vec!["xray:state:erosion-candidate".to_string()]
        );
        // b_core is referenced from a non-test source, no proof evidence,
        // and not a flagged source -> unproven.
        assert_eq!(
            state_tag_of(&g, "file::modB/src/lib.rs::fn::b_core"),
            vec!["xray:state:unproven".to_string()]
        );
        // a_util stays overgrowth (orphan, not a source).
        assert_eq!(
            state_tag_of(&g, "file::modA/src/util.rs::fn::a_util"),
            vec!["xray:state:overgrowth".to_string()]
        );
    }

    #[test]
    fn paint_scope_narrows_the_painted_set() {
        let mut g = paint_graph_fixture();
        let out = paint_g(
            &mut g,
            &paint_input(
                Some("file::modA"),
                XrayManifest::default(),
                XrayMode::Commit,
            ),
        );
        // Only the two modA nodes are in scope.
        assert_eq!(out.counts.scanned, 2);
        assert_eq!(out.counts.painted, 2);
        // modB node is out of scope -> untouched.
        assert!(state_tag_of(&g, "file::modB/src/lib.rs::fn::b_core").is_empty());
        // a_main is referenced from outside its own module? No — a_main is a
        // SOURCE, not a target; it has no incoming reference edge -> overgrowth.
        assert_eq!(
            state_tag_of(&g, "file::modA/src/lib.rs::fn::a_main"),
            vec!["xray:state:overgrowth".to_string()]
        );
    }

    // -----------------------------------------------------------------------
    // PART A — manifest file loading + resolution precedence
    // -----------------------------------------------------------------------

    /// Unique sandbox dir in `std::env::temp_dir()` for manifest-file tests
    /// (mirrors `fresh_sandbox`). NEVER the real repo.
    fn fresh_manifest_dir() -> PathBuf {
        static MCOUNTER: AtomicU64 = AtomicU64::new(0);
        let n = MCOUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("xray_manifest_test_{}_{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_manifest_file_ignores_unknown_keys_and_reads_ratified() {
        let dir = fresh_manifest_dir();
        let path = dir.join("xray.manifest.json");
        // Includes an `_about` key to prove unknown fields are ignored.
        std::fs::write(
            &path,
            r#"{"ratified":true,"layer_order":["x","y"],"forbid":[],"require_exists":["seek"],"_about":"ignored"}"#,
        )
        .unwrap();

        let (manifest, ratified) = load_manifest_file(&path).expect("file must parse");
        assert!(ratified, "ratified flag must be read as true");
        assert_eq!(manifest.layer_order, vec!["x".to_string(), "y".to_string()]);
        assert_eq!(manifest.require_exists, vec!["seek".to_string()]);
        assert!(manifest.forbid.is_empty());

        // A missing path returns None (fail soft).
        let missing = dir.join("does_not_exist.json");
        assert!(load_manifest_file(&missing).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolve_manifest_file_path_vs_inline_precedence() {
        let dir = fresh_manifest_dir();
        let path = dir.join("xray.manifest.json");
        std::fs::write(
            &path,
            r#"{"ratified":true,"layer_order":["a","b"],"forbid":[],"require_exists":[]}"#,
        )
        .unwrap();
        let path_str = path.to_string_lossy().into_owned();

        // Empty inline + manifest_path -> loads the file (source "file:<path>").
        let empty = XrayManifest::default();
        let resolved = resolve_manifest(&empty, Some(path_str.as_str()), None);
        assert_eq!(resolved.source, format!("file:{path_str}"));
        assert!(resolved.ratified, "file's ratified flag must be carried");
        assert_eq!(
            resolved.manifest.layer_order,
            vec!["a".to_string(), "b".to_string()]
        );

        // Non-empty inline + a manifest_path still present -> inline wins.
        let inline = XrayManifest {
            layer_order: vec!["inline_only".to_string()],
            ..Default::default()
        };
        let resolved2 = resolve_manifest(&inline, Some(path_str.as_str()), None);
        assert_eq!(resolved2.source, "inline");
        assert!(!resolved2.ratified, "inline source is never file-ratified");
        assert_eq!(
            resolved2.manifest.layer_order,
            vec!["inline_only".to_string()]
        );

        // Explicit path that fails to load -> falls straight to "none" (no
        // auto-discovery), per spec.
        let bad = dir.join("nope.json");
        let resolved3 =
            resolve_manifest(&empty, Some(bad.to_string_lossy().as_ref()), Some("/tmp"));
        assert_eq!(resolved3.source, "none");
        assert!(!resolved3.ratified);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // -----------------------------------------------------------------------
    // PART B — evidence-based paint (test-exercise + grounded -> bedrock)
    // -----------------------------------------------------------------------

    /// True if `external_id` carries exactly the given `xray:state:*` tag.
    fn has_state(g: &Graph, ext: &str, tag: &str) -> bool {
        state_tag_of(g, ext) == vec![tag.to_string()]
    }

    /// A test-source node `calls` a code node -> the code node is exercised.
    /// Plus a non-test `caller` that `calls` a `used` node (used-but-unproven),
    /// and an orphan node. Finalized so the CSR is populated.
    fn evidence_graph_fixture() -> Graph {
        let mut g = Graph::new();
        // idx0: a TEST source node (path contains /tests/).
        g.add_node(
            "file::m1nd-core/tests/x.rs::fn::t",
            "t",
            NodeType::Function,
            &["rust"],
            0.0,
            0.0,
        )
        .unwrap(); // 0
                   // idx1: a real code node, exercised by the test above -> bedrock.
        g.add_node(
            "file::m1nd-core/src/lib.rs::fn::real",
            "real",
            NodeType::Function,
            &["rust"],
            0.0,
            0.0,
        )
        .unwrap(); // 1
                   // idx2: a non-test caller in another module (so its calls are cross-module
                   // but, crucially, it is NOT a test source).
        g.add_node(
            "file::m1nd-mcp/src/caller.rs::fn::caller",
            "caller",
            NodeType::Function,
            &["rust"],
            0.0,
            0.0,
        )
        .unwrap(); // 2
                   // idx3: referenced by the non-test caller -> used but unproven.
        g.add_node(
            "file::m1nd-mcp/src/used.rs::fn::used",
            "used",
            NodeType::Function,
            &["rust"],
            0.0,
            0.0,
        )
        .unwrap(); // 3
                   // idx4: orphan — nothing references it.
        g.add_node(
            "file::m1nd-mcp/src/orphan.rs::fn::orphan",
            "orphan",
            NodeType::Function,
            &["rust"],
            0.0,
            0.0,
        )
        .unwrap(); // 4

        // test-source t -> real (calls): marks `real` as exercised (bedrock).
        g.add_edge(
            NodeId::new(0),
            NodeId::new(1),
            "calls",
            FiniteF32::new(1.0),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.0),
        )
        .unwrap();
        // non-test caller -> used (calls): gives `used` indegree 1, no evidence.
        g.add_edge(
            NodeId::new(2),
            NodeId::new(3),
            "calls",
            FiniteF32::new(1.0),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.0),
        )
        .unwrap();
        g.finalize().unwrap();
        g
    }

    #[test]
    fn is_test_source_keys_off_path_after_file_prefix() {
        assert!(is_test_source("file::m1nd-core/tests/x.rs::fn::t"));
        assert!(is_test_source("file::pkg/test_helpers.rs::fn::h"));
        assert!(is_test_source("file::pkg/src/foo_test.rs::fn::f"));
        assert!(is_test_source("file::pkg/src/tests.rs::fn::f"));
        assert!(is_test_source("tests.rs"));
        // NOT a test source: ordinary src path.
        assert!(!is_test_source("file::m1nd-core/src/lib.rs::fn::real"));
        assert!(!is_test_source("file::m1nd-mcp/src/caller.rs::fn::caller"));
    }

    #[test]
    fn paint_test_exercised_node_is_bedrock() {
        let mut g = evidence_graph_fixture();
        let out = paint_g(
            &mut g,
            &paint_input(None, XrayManifest::default(), XrayMode::Commit),
        );
        assert_eq!(out.status, "committed");
        // `real` is the target of a `calls` edge FROM a test source -> bedrock.
        assert!(
            has_state(
                &g,
                "file::m1nd-core/src/lib.rs::fn::real",
                "xray:state:bedrock"
            ),
            "test-exercised node must be bedrock"
        );
    }

    #[test]
    fn paint_used_but_not_exercised_node_is_unproven() {
        let mut g = evidence_graph_fixture();
        paint_g(
            &mut g,
            &paint_input(None, XrayManifest::default(), XrayMode::Commit),
        );
        // `used` has indegree 1 (from the non-test caller) but no proof evidence.
        assert!(
            has_state(
                &g,
                "file::m1nd-mcp/src/used.rs::fn::used",
                "xray:state:unproven"
            ),
            "used-but-unexercised node must be unproven"
        );
    }

    #[test]
    fn paint_orphan_node_is_overgrowth() {
        let mut g = evidence_graph_fixture();
        paint_g(
            &mut g,
            &paint_input(None, XrayManifest::default(), XrayMode::Commit),
        );
        // `orphan` has zero incoming edges -> overgrowth.
        assert!(
            has_state(
                &g,
                "file::m1nd-mcp/src/orphan.rs::fn::orphan",
                "xray:state:overgrowth"
            ),
            "orphan node must be overgrowth"
        );
    }

    #[test]
    fn paint_proof_coverage_is_bedrock_over_scanned() {
        let mut g = evidence_graph_fixture();
        let out = paint_g(
            &mut g,
            &paint_input(None, XrayManifest::default(), XrayMode::DryRun),
        );
        // 5 nodes scanned. Exactly ONE (`real`) is bedrock. The test source `t`
        // itself is an orphan (overgrowth); `used` is unproven; `caller` and
        // `orphan` are overgrowth. proof_coverage = 1/5 = 0.2 (rounded 3 dp).
        assert_eq!(out.counts.scanned, 5);
        assert_eq!(out.counts.bedrock, 1);
        assert_eq!(out.proof_coverage, 0.2);
    }

    #[test]
    fn paint_grounded_in_edge_marks_target_bedrock() {
        // A `grounded_in` incoming edge is proof evidence even from a non-test
        // source. Build: code node `claim` grounded_in evidence node `ev` means
        // the EDGE points claim -> ev with relation grounded_in, so `ev` is the
        // exercised target. Mirror that: source has an outgoing grounded_in.
        let mut g = Graph::new();
        g.add_node(
            "file::m1nd-core/src/a.rs::fn::claim",
            "claim",
            NodeType::Function,
            &["rust"],
            0.0,
            0.0,
        )
        .unwrap(); // 0
        g.add_node(
            "file::m1nd-core/src/b.rs::fn::ev",
            "ev",
            NodeType::Function,
            &["rust"],
            0.0,
            0.0,
        )
        .unwrap(); // 1
        g.add_edge(
            NodeId::new(0),
            NodeId::new(1),
            "grounded_in",
            FiniteF32::new(1.0),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.0),
        )
        .unwrap();
        g.finalize().unwrap();

        let out = paint_g(
            &mut g,
            &paint_input(None, XrayManifest::default(), XrayMode::Commit),
        );
        assert_eq!(out.status, "committed");
        // `ev` is the target of a grounded_in edge -> bedrock (proof evidence).
        assert!(
            has_state(&g, "file::m1nd-core/src/b.rs::fn::ev", "xray:state:bedrock"),
            "grounded_in target must be bedrock"
        );
    }

    #[test]
    fn paint_idempotent_repaint_under_new_classifier_one_tag() {
        let mut g = evidence_graph_fixture();
        // First commit paints every node with its evidence-based state.
        let first = paint_g(
            &mut g,
            &paint_input(None, XrayManifest::default(), XrayMode::Commit),
        );
        assert_eq!(first.counts.scanned, 5);
        // Second commit: nothing should change; exactly ONE xray:state:* per node.
        let second = paint_g(
            &mut g,
            &paint_input(None, XrayManifest::default(), XrayMode::Commit),
        );
        assert_eq!(second.counts.painted, 0);
        for ext in [
            "file::m1nd-core/tests/x.rs::fn::t",
            "file::m1nd-core/src/lib.rs::fn::real",
            "file::m1nd-mcp/src/caller.rs::fn::caller",
            "file::m1nd-mcp/src/used.rs::fn::used",
            "file::m1nd-mcp/src/orphan.rs::fn::orphan",
        ] {
            assert_eq!(
                state_tag_of(&g, ext).len(),
                1,
                "{ext} must carry exactly one xray:state:* tag after re-paint"
            );
        }
    }

    // -----------------------------------------------------------------------
    // X-RAY audit ledger — append helper + record shape + reader
    // -----------------------------------------------------------------------

    use crate::server::McpConfig;
    use m1nd_core::domain::DomainConfig;

    /// Read every line of a ledger file into parsed JSON values (test helper).
    fn read_all_ledger_lines(path: &Path) -> Vec<serde_json::Value> {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                serde_json::from_str::<serde_json::Value>(l).expect("each line parses as JSON")
            })
            .collect()
    }

    #[test]
    fn append_ledger_increments_seq_and_writes_one_line_each() {
        let dir = fresh_sandbox();
        let ledger = dir.join("xray.ledger.jsonl");

        let rec1 = serde_json::json!({ "seq": 1, "verb": "xray_retag", "v": "a" });
        let seq1 = append_ledger(&ledger, &rec1).expect("first append");
        assert_eq!(seq1, 1, "first append must return seq 1");

        let rec2 = serde_json::json!({ "seq": 2, "verb": "xray_paint", "v": "b" });
        let seq2 = append_ledger(&ledger, &rec2).expect("second append");
        assert_eq!(seq2, 2, "second append must return seq 2");

        let lines = read_all_ledger_lines(&ledger);
        assert_eq!(lines.len(), 2, "file must have exactly 2 lines");
        assert_eq!(lines[0]["verb"], "xray_retag");
        assert_eq!(lines[1]["verb"], "xray_paint");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_ledger_record_truncates_changes_over_cap() {
        // One over the cap: only the first LEDGER_CHANGES_CAP survive, and the
        // total is surfaced as changes_truncated.
        let over = LEDGER_CHANGES_CAP + 5;
        let changes: Vec<serde_json::Value> = (0..over)
            .map(|i| serde_json::json!({ "node": format!("n{i}") }))
            .collect();
        let record = build_ledger_record(
            7,
            "xray_retag",
            "deadbeef",
            serde_json::json!({ "applied": over }),
            changes,
        );
        assert_eq!(record["seq"], 7);
        assert_eq!(record["mode"], "commit");
        assert_eq!(
            record["changes"].as_array().unwrap().len(),
            LEDGER_CHANGES_CAP
        );
        assert_eq!(record["changes_truncated"], serde_json::json!(over));

        // Under the cap: no changes_truncated key at all.
        let small = build_ledger_record(
            1,
            "xray_paint",
            "v",
            serde_json::json!({}),
            vec![serde_json::json!({ "node": "x" })],
        );
        assert!(small.get("changes_truncated").is_none());
        assert_eq!(small["changes"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn read_ledger_filters_by_verb_and_returns_most_recent_first() {
        let dir = fresh_sandbox();
        let ledger = dir.join("xray.ledger.jsonl");
        for (i, verb) in ["xray_retag", "xray_paint", "xray_retag", "xray_apply"]
            .iter()
            .enumerate()
        {
            let rec =
                build_ledger_record((i + 1) as u64, verb, "v", serde_json::json!({}), Vec::new());
            append_ledger(&ledger, &rec).unwrap();
        }

        // No filter, limit 2: the two MOST RECENT records, newest first.
        let (entries, total) = read_ledger(&ledger, 2, None);
        assert_eq!(total, 4, "total must count every line pre-filter");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["seq"], 4, "most recent first");
        assert_eq!(entries[1]["seq"], 3);

        // verb filter: only the two xray_retag records, newest first.
        let (retags, total2) = read_ledger(&ledger, 20, Some("xray_retag"));
        assert_eq!(total2, 4, "total is pre-filter even when filtering");
        assert_eq!(retags.len(), 2);
        assert_eq!(retags[0]["seq"], 3);
        assert_eq!(retags[1]["seq"], 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_ledger_missing_file_is_empty_not_error() {
        let dir = fresh_sandbox();
        let missing = dir.join("nope.ledger.jsonl");
        let (entries, total) = read_ledger(&missing, 20, None);
        assert!(entries.is_empty());
        assert_eq!(total, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_forbidden_path_excludes_the_audit_ledger() {
        // xray_apply must never treat its own append-only audit log as a source
        // file (it lives beside graph_snapshot.json, inside workspace_root).
        assert!(is_forbidden_path(Path::new("/repo/xray.ledger.jsonl")));
        assert!(is_forbidden_path(Path::new("/repo/XRAY.LEDGER.JSONL"))); // case-insensitive
        assert!(is_forbidden_path(Path::new("/repo/graph_snapshot.json")));
        assert!(!is_forbidden_path(Path::new("/repo/src/main.rs")));
    }

    // -----------------------------------------------------------------------
    // X-RAY audit ledger — handler-level (SessionState in a temp dir)
    // -----------------------------------------------------------------------

    /// Build a writable SessionState whose graph_path lives in a temp runtime
    /// dir, seeded with `g`. The ledger lands beside graph_path.
    fn ledger_state(root: &Path, g: Graph) -> SessionState {
        let runtime_dir = root.join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph_snapshot.json"),
            plasticity_state: runtime_dir.join("plasticity_state.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };
        SessionState::initialize(g, &config, DomainConfig::code()).expect("init session")
    }

    /// The ledger path the handler will use for `state` (beside graph_path).
    fn state_ledger_path(state: &SessionState) -> PathBuf {
        ledger_path_for(state).expect("graph_path has a parent")
    }

    #[test]
    fn retag_commit_writes_one_ledger_entry_with_exact_before_after() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = ledger_state(temp.path(), sample_graph());
        let ledger = state_ledger_path(&state);
        assert!(!ledger.exists(), "no ledger before any commit");

        // Commit: add a tag to all rust-tagged nodes (all 3 in sample_graph).
        let sel = XraySelector {
            filter_tags: vec!["rust".to_string()],
            ..Default::default()
        };
        let value = handle_xray_retag(
            &mut state,
            input(sel, XrayTagOp::Add, &["xray:bedrock"], XrayMode::Commit),
        )
        .expect("retag commit");
        assert_eq!(value["status"], "committed");

        let lines = read_all_ledger_lines(&ledger);
        assert_eq!(
            lines.len(),
            1,
            "exactly one ledger line per committed write"
        );
        let rec = &lines[0];
        assert_eq!(rec["seq"], 1);
        assert_eq!(rec["verb"], "xray_retag");
        assert_eq!(rec["mode"], "commit");
        assert_eq!(rec["summary"]["applied"], 3);
        let changes = rec["changes"].as_array().unwrap();
        assert_eq!(changes.len(), 3, "one change record per mutated node");
        // foo had ["rust","rust:visibility:private"]; after adds xray:bedrock.
        let foo = changes
            .iter()
            .find(|c| c["node"] == "file::a.rs::fn::foo")
            .expect("foo recorded");
        let before: Vec<String> = foo["before"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        let after: Vec<String> = foo["after"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert!(
            !before.contains(&"xray:bedrock".to_string()),
            "before is pre-state"
        );
        assert!(
            after.contains(&"xray:bedrock".to_string()),
            "after has the new tag"
        );
    }

    #[test]
    fn retag_dry_run_writes_no_ledger_entry() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = ledger_state(temp.path(), sample_graph());
        let ledger = state_ledger_path(&state);

        let sel = XraySelector {
            filter_tags: vec!["rust".to_string()],
            ..Default::default()
        };
        let value = handle_xray_retag(
            &mut state,
            input(sel, XrayTagOp::Add, &["xray:bedrock"], XrayMode::DryRun),
        )
        .expect("retag dry_run");
        assert_eq!(value["status"], "dry_run");
        assert!(!ledger.exists(), "dry_run must write NO ledger");
    }

    #[test]
    fn xray_ledger_reads_back_entries_with_limit_and_verb_filter() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = ledger_state(temp.path(), sample_graph());

        // Two committed writes: retag then paint, producing two ledger lines.
        let sel = XraySelector {
            filter_tags: vec!["rust".to_string()],
            ..Default::default()
        };
        handle_xray_retag(
            &mut state,
            input(sel, XrayTagOp::Add, &["xray:bedrock"], XrayMode::Commit),
        )
        .expect("retag commit");
        handle_xray_paint(
            &mut state,
            XrayPaintInput {
                scope: None,
                manifest: XrayManifest::default(),
                manifest_path: None,
                mode: XrayMode::Commit,
            },
        )
        .expect("paint commit");

        // Read all back: 2 entries, most recent (paint) first.
        let all = handle_xray_ledger(&mut state, XrayLedgerInput::default()).expect("ledger read");
        assert_eq!(all["verb"], "xray_ledger");
        assert_eq!(all["total_entries"], 2);
        let entries = all["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["verb"], "xray_paint", "most recent first");
        assert_eq!(entries[1]["verb"], "xray_retag");
        assert!(all["ledger_path"].is_string());

        // limit 1: only the most recent.
        let limited = handle_xray_ledger(
            &mut state,
            XrayLedgerInput {
                limit: Some(1),
                verb: None,
            },
        )
        .expect("ledger read limited");
        assert_eq!(limited["entries"].as_array().unwrap().len(), 1);
        assert_eq!(limited["entries"][0]["verb"], "xray_paint");

        // verb filter: only the retag line, but total is still 2.
        let filtered = handle_xray_ledger(
            &mut state,
            XrayLedgerInput {
                limit: None,
                verb: Some("xray_retag".to_string()),
            },
        )
        .expect("ledger read filtered");
        assert_eq!(filtered["total_entries"], 2);
        let fe = filtered["entries"].as_array().unwrap();
        assert_eq!(fe.len(), 1);
        assert_eq!(fe[0]["verb"], "xray_retag");
    }

    #[test]
    fn xray_ledger_missing_file_returns_empty_list() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut state = ledger_state(temp.path(), sample_graph());
        // No commit happened, so no ledger file exists yet.
        let out = handle_xray_ledger(&mut state, XrayLedgerInput::default()).expect("ledger read");
        assert_eq!(out["total_entries"], 0);
        assert!(out["entries"].as_array().unwrap().is_empty());
    }
}
