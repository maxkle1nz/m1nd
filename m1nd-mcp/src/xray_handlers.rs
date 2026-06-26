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
use std::path::{Path, PathBuf};

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
                        conflicts: selected.len().max(1) as u32,
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

/// The transform to apply. An enum so more transforms can slot in later.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum XrayTransform {
    /// Ensure a header tag exists in the first 3 lines; idempotent insert.
    EnsureHeaderTag { tag: String },
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
    /// Files actually written (0 on dry_run / abort).
    pub applied: u32,
    /// Files whose content drifted between STAGE and REHASH.
    pub conflicts: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct XrayApplyPlannedSample {
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct XrayApplyOutput {
    pub verb: &'static str,
    /// "dry_run" | "committed" | "aborted_conflicts".
    pub status: String,
    pub counts: XrayApplyCounts,
    /// First few planned paths (cap 5), for the agent to eyeball before commit.
    pub planned_sample: Vec<XrayApplyPlannedSample>,
    /// First few conflict file names (cap 5).
    pub conflicts_sample: Vec<String>,
    /// Content fingerprint of the PLANNED files' current on-disk content (hex),
    /// order-independent over path. The caller passes this back as
    /// `expect_version` on a later `commit` to guard against concurrent file
    /// edits between dry_run and commit (cross-call OCC, checked before staging).
    pub version: String,
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
/// Folds each file's SELECT-phase `content_hash` (a hex `u64`) by XOR, so the
/// digest is order-independent over path and flips whenever any planned file's
/// bytes change on disk. Reuses the same `DefaultHasher` content hash the engine
/// already computes at SELECT — no new hashing, no new dependency.
fn plan_version(guards: &[String]) -> String {
    let mut fold: u64 = 0;
    for g in guards {
        // `content_hash` always emits 16 hex chars; parse is infallible, but be
        // defensive (an unparsable guard folds in as 0 rather than panicking).
        fold ^= u64::from_str_radix(g, 16).unwrap_or(0);
    }
    format!("{fold:016x}")
}

/// Pure transform: returns `Some(new_content)` if it changes the file, `None`
/// if it is a no-op. Matching on the enum lets future variants slot in.
fn apply_transform(content: &str, transform: &XrayTransform) -> Option<String> {
    match transform {
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
    apply_files_inner(paths, transform, mode, expect_version, None)
}

/// Test seam: a callback fired between STAGE and REHASH, receiving the original
/// paths so a test can mutate one mid-apply (mirrors the Python `tamper(plan)`).
type TamperHook<'a> = Option<&'a dyn Fn(&[PathBuf])>;

/// Shared implementation. `tamper`, when `Some`, is invoked AFTER the STAGE phase
/// and BEFORE the REHASH phase, receiving the list of original paths so a test
/// can mutate one mid-apply to exercise the OCC-conflict path (mirrors the
/// Python `tamper(plan)` placement). Production always passes `None`.
fn apply_files_inner(
    paths: &[PathBuf],
    transform: &XrayTransform,
    mode: XrayMode,
    expect_version: Option<&str>,
    tamper: TamperHook<'_>,
) -> XrayApplyOutput {
    let mut counts = XrayApplyCounts {
        matched: paths.len() as u32,
        ..Default::default()
    };

    // ---- SELECT: read + guard-hash + plan ----
    // plan entries: (path, guard_hash, new_bytes)
    let mut plan: Vec<(PathBuf, String, Vec<u8>)> = Vec::new();
    for p in paths {
        let Ok(bytes) = std::fs::read(p) else {
            // Unreadable file: skip without panicking (never unwrap a read).
            continue;
        };
        let guard = content_hash(&bytes);
        let current = String::from_utf8_lossy(&bytes);
        match apply_transform(&current, transform) {
            None => {
                counts.skipped_noop += 1;
            }
            Some(new_content) => {
                counts.planned += 1;
                plan.push((p.clone(), guard, new_content.into_bytes()));
            }
        }
    }

    // Cross-call OCC fingerprint over the planned files' current bytes (the
    // SELECT-phase guard hashes). Order-independent over path.
    let version = plan_version(&plan.iter().map(|(_, g, _)| g.clone()).collect::<Vec<_>>());

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
            counts.conflicts = plan.len().max(1) as u32;
            counts.applied = 0;
            return XrayApplyOutput {
                verb: "xray_apply",
                status: "aborted_conflicts".to_string(),
                counts,
                planned_sample: Vec::new(),
                conflicts_sample,
                version,
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
    for (p, _guard, new_bytes) in &plan {
        let tmp = tmp_path_for(p);
        let staged = (|| -> std::io::Result<()> {
            use std::io::Write;
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(new_bytes)?;
            f.flush()?;
            f.sync_all()?; // fsync the staged temp
            Ok(())
        })();
        if staged.is_err() {
            // Hard abort on any stage I/O error: delete every temp written so
            // far (including this one if it partially exists), write ZERO
            // originals. Reported as "aborted_conflicts" with a synthetic entry
            // naming the file that failed to stage.
            let _ = std::fs::remove_file(&tmp);
            cleanup(&temps);
            let name = file_label(p);
            counts.conflicts = 1;
            counts.applied = 0;
            return XrayApplyOutput {
                verb: "xray_apply",
                status: "aborted_conflicts".to_string(),
                counts,
                planned_sample: Vec::new(),
                conflicts_sample: vec![name],
                version,
            };
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
        };
    }

    // ---- SWAP (phase 2): atomic rename of every staged temp over its original ----
    for (orig, tmp) in &temps {
        if let Err(_e) = std::fs::rename(tmp, orig) {
            // Same-filesystem rename should not fail here; if it does, stop and
            // clean up the remaining temps. Some originals may already be
            // swapped — this is the one non-atomic edge and matches the Python
            // behaviour (which lets an os.replace error bubble).
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
            };
        }
    }

    counts.applied = counts.planned;
    let planned_sample = plan
        .iter()
        .take(SAMPLE_CAP)
        .map(|(p, _, _)| XrayApplyPlannedSample {
            path: p.to_string_lossy().into_owned(),
        })
        .collect();
    XrayApplyOutput {
        verb: "xray_apply",
        status: "committed".to_string(),
        counts,
        planned_sample,
        conflicts_sample: Vec::new(),
        version,
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
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Forbidden file NAMES (runtime artifacts regenerated by daemon/ingest).
    if name == "graph_snapshot.json"
        || name == "daemon_alerts.json"
        || name == "document_cache_index.json"
        || name == "ingest_roots.json"
        || name.ends_with("_state.json")
        || name.ends_with(".xray.tmp")
    {
        return true;
    }
    // Forbidden path SEGMENTS (VCS / build / deps).
    let path_str = p.to_string_lossy();
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
    // (3) extension filter (case-sensitive on the ext string).
    if !selector.extensions.is_empty() {
        let ext_ok = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| selector.extensions.iter().any(|w| w == e));
        if !ext_ok {
            return false;
        }
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

    // Narrow the walk start to root.join(prefix) when a prefix is given (and it
    // stays a dir under root); the per-file gate re-checks the prefix anyway.
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

    let output = apply_files(
        &matched,
        &input.transform,
        input.mode,
        input.expect_version.as_deref(),
    );
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

            if let Some(rule) = classify_edge(&input.manifest, src_mod, dst_mod) {
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
    #[serde(default)]
    pub manifest_ratified: bool,
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
}

/// Pure gate logic over a finalized `Graph` (unit-testable, no `SessionState`).
/// Read-only: walks the node's live outgoing CSR, never mutates.
pub fn gate_graph(graph: &Graph, input: &XrayGateInput) -> XrayGateOutput {
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
    let manifest_empty = input.manifest.forbid.is_empty() && input.manifest.layer_order.is_empty();
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
        if let Some(rule) = classify_edge(&input.manifest, &node_module, dst_mod) {
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
        if let Some(rule) = classify_edge(&input.manifest, &node_module, m) {
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
        if input.manifest_ratified {
            "blocked"
        } else {
            "caution"
        }
    } else {
        "clear"
    };
    if !any {
        reasons.push("no North-Star violation".to_string());
    } else if !input.manifest_ratified {
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
    }
}

/// MCP handler for `xray_gate`. Read-only: holds the graph *read* lock for the
/// computation, never mutates, never persists (safe under read-only attach).
pub fn handle_xray_gate(
    state: &mut SessionState,
    input: XrayGateInput,
) -> M1ndResult<serde_json::Value> {
    let output = {
        let graph = state.graph.read();
        gate_graph(&graph, &input)
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
        apply_files_inner(paths, transform, mode, None, Some(&tamper))
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
        }
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
        let out = gate_graph(
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
        let out = gate_graph(
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
        let out = gate_graph(
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
        let out = gate_graph(
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
        let out = gate_graph(
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
        let out = gate_graph(
            &g,
            &gate_input("file::modA/src/lib.rs::fn::a_main", &[], manifest, true),
        );
        assert_eq!(out.verdict, "blocked");
        assert_eq!(out.existing_violations.len(), 1);
        assert_eq!(out.existing_violations[0].rule, "layer");
    }
}
