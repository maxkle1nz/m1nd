//! Freshness at first contact: is the served code graph behind the checkout?
//!
//! A full code ingest records the commit its walk observed at `HEAD`
//! ([`IndexedCommit`]). `north` and `health` compare it with the `HEAD` of the
//! checkout the caller works in and, only when the two differ, carry one short
//! `graph_drift` note: the indexed commit, the current `HEAD`, how many commits
//! and files separate them, and the exact refresh call. A graph at `HEAD` adds
//! nothing to either packet.
//!
//! PERSISTENCE IS ADVISORY. The commit lives in the [`INDEXED_VCS_STATE_FILE`]
//! sidecar, published only AFTER the graph it describes is durable (direct
//! `persist`, or the brain actor's post-CURRENT effects), with a temp file and a
//! rename. It is never part of a checkpoint or of the strict checkpoint/rebuild
//! digest, so a runtime this code has ingested still boots under a binary that
//! predates it. Reading it back is lenient everywhere: missing, unparsable,
//! recorded for a root the brain does not declare, or recorded for another code
//! graph (a crash between the graph write and this one) all mean "unknown", and
//! unknown means no note — never a guessed one, never a failed boot.
//!
//! The comparison is read-only. At a matching `HEAD` it costs one
//! `git rev-parse`; at a drifted `HEAD` the counts (`rev-list --count`,
//! `diff --name-only` between the two commits — no index, no working tree) are
//! measured once and memoized per (indexed commit, checkout, `HEAD`), so the
//! next call at the same `HEAD` costs one `git rev-parse` again. The check
//! writes nothing: not the runtime, not the repository, not the index.
//!
//! Whose `HEAD`? The bound root's, unless the caller's root is known and sits in
//! a checkout of its own nested inside the bound root that shares the bound
//! root's repository — a git worktree. That caller reads files at its own
//! commit, so the note is measured against its checkout and names it, because a
//! refresh of the bound root would not bring the graph to that commit. A caller
//! outside the bound root is already told by `reception` that this graph does
//! not cover it.

use crate::session::SessionState;
use m1nd_core::graph::Graph;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Runtime-root sidecar that records the indexed commit next to the graph.
pub const INDEXED_VCS_STATE_FILE: &str = "indexed_vcs_state.json";
/// Dot-prefixed, so a leftover from a crash is hidden from a code walk even
/// when the runtime lives inside the scanned root.
const INDEXED_VCS_STATE_TEMP_FILE: &str = ".indexed_vcs_state.json.tmp";
const INDEXED_VCS_STATE_SCHEMA: &str = "m1nd-indexed-vcs-state-v1";
/// Abbreviated commit length used in the note: unambiguous in practice and
/// a fraction of the bytes of a full object id.
const SHORT_OID_LEN: usize = 12;

/// The commit the live code graph was built from, and the root that was
/// scanned (canonical — the exact root a refresh must name).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexedCommit {
    pub head_oid: String,
    pub root: String,
}

impl IndexedCommit {
    pub fn new(head_oid: &str, root: &str) -> Self {
        Self {
            head_oid: head_oid.to_string(),
            root: root.to_string(),
        }
    }
}

/// The sidecar record: the commit, the root, and a witness of the code graph
/// it was published for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexedVcsStateV1 {
    schema: String,
    head_oid: String,
    root: String,
    code_witness: String,
}

impl IndexedVcsStateV1 {
    fn is_valid(&self) -> bool {
        self.schema == INDEXED_VCS_STATE_SCHEMA
            && is_object_id(&self.head_oid)
            && !self.root.trim().is_empty()
            && self.code_witness.len() == 64
    }
}

fn is_object_id(oid: &str) -> bool {
    (oid.len() == 40 || oid.len() == 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Digest of the code ingest's own nodes: every `file::` node id with its line
/// span, in sorted order. Recall and document merges add no `file::` node, so
/// `memorize` never disturbs it; any change to the code graph does. It binds the
/// sidecar to one code graph without touching the checkpoint.
pub(crate) fn code_graph_witness(graph: &Graph) -> String {
    let mut rows = graph
        .id_to_node
        .iter()
        .filter_map(|(interned, node)| {
            let id = graph.strings.resolve(*interned);
            if !id.starts_with("file::") {
                return None;
            }
            let provenance = graph
                .nodes
                .provenance
                .get(node.as_usize())
                .copied()
                .unwrap_or_default();
            Some((id.to_string(), provenance.line_start, provenance.line_end))
        })
        .collect::<Vec<_>>();
    rows.sort();
    let mut hasher = Sha256::new();
    hasher.update(b"m1nd/indexed-vcs-code-witness/v1\0");
    for (id, line_start, line_end) in rows {
        hasher.update((id.len() as u64).to_be_bytes());
        hasher.update(id.as_bytes());
        hasher.update(line_start.to_be_bytes());
        hasher.update(line_end.to_be_bytes());
    }
    crate::util::hex_lower(&hasher.finalize())
}

/// Read the sidecar back for `graph`. Every doubt is "unknown" (`None`), said
/// once on stderr, never an error: the record is advisory.
pub(crate) fn load_indexed_commit(
    runtime_root: &Path,
    graph: &Graph,
    declared_roots: &[String],
) -> Option<IndexedCommit> {
    let path = runtime_root.join(INDEXED_VCS_STATE_FILE);
    let unknown = |why: &str| {
        eprintln!(
            "[m1nd] indexed commit unknown: {} {why}; no drift note until the next code ingest",
            path.display()
        );
        None
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => return unknown("is unreadable"),
    };
    let Ok(record) = serde_json::from_slice::<IndexedVcsStateV1>(&bytes) else {
        return unknown("is not a valid record");
    };
    if !record.is_valid() {
        return unknown("is not a valid record");
    }
    let declared = declared_roots.iter().any(|root| {
        crate::project_brains::ProjectBrainRegistry::canonical_key(root) == record.root
    });
    if !declared {
        return unknown("names a root this brain does not declare");
    }
    if record.code_witness != code_graph_witness(graph) {
        return unknown("was recorded for another code graph");
    }
    Some(IndexedCommit {
        head_oid: record.head_oid,
        root: record.root,
    })
}

/// Publish `state.indexed_vcs` to the sidecar if it changed. Call only once the
/// graph it describes is durable. Best effort: a failed write removes the old
/// record (so it cannot describe the wrong graph) and leaves the commit to be
/// published again at the next durable point.
pub(crate) fn publish_indexed_commit(state: &mut SessionState) {
    if !state.indexed_vcs_unpublished || state.read_only {
        return;
    }
    let path = state.runtime_root.join(INDEXED_VCS_STATE_FILE);
    let result = match state.indexed_vcs.as_ref() {
        Some(indexed) => {
            let record = IndexedVcsStateV1 {
                schema: INDEXED_VCS_STATE_SCHEMA.to_string(),
                head_oid: indexed.head_oid.clone(),
                root: indexed.root.clone(),
                code_witness: code_graph_witness(&state.graph.read()),
            };
            write_atomically(&state.runtime_root, &path, &record)
        }
        None => match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        },
    };
    match result {
        Ok(()) => state.indexed_vcs_unpublished = false,
        Err(error) => {
            eprintln!(
                "[m1nd] WARNING: could not publish {} ({error}); the indexed commit stays unknown after a restart",
                path.display()
            );
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn write_atomically(
    runtime_root: &Path,
    path: &Path,
    record: &IndexedVcsStateV1,
) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(record).map_err(std::io::Error::other)?;
    let temp = runtime_root.join(INDEXED_VCS_STATE_TEMP_FILE);
    let written = (|| {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    written
}

/// Process-only memo of the last drift measurement. Never persisted.
#[derive(Clone, Debug, Default)]
pub struct GraphDriftCache {
    entry: Option<CachedDrift>,
}

#[derive(Clone, Debug)]
struct CachedDrift {
    indexed: String,
    checkout: PathBuf,
    head: String,
    note: serde_json::Value,
}

/// The checkout whose `HEAD` the caller reads, and that `HEAD`.
struct CheckoutView {
    dir: PathBuf,
    head: String,
    /// True when the caller works in another checkout (a worktree) of the
    /// indexed repository, nested inside the bound root.
    other_checkout: bool,
}

/// The `graph_drift` note, or `None` when the graph is at `HEAD` — or when no
/// commit was recorded or `HEAD` cannot be read, since then there is no
/// measured drift to state.
pub fn graph_drift_note(state: &mut SessionState) -> Option<serde_json::Value> {
    let indexed = state.indexed_vcs.clone()?;
    let bound = PathBuf::from(&indexed.root);
    let view = resolve_checkout(&bound, state.caller_root.as_deref())?;
    if view.head == indexed.head_oid {
        return None;
    }
    if let Some(cached) = state.graph_drift_cache.entry.as_ref() {
        if cached.indexed == indexed.head_oid
            && cached.checkout == view.dir
            && cached.head == view.head
        {
            return Some(cached.note.clone());
        }
    }
    let note = measure(&bound, &indexed, &view);
    state.graph_drift_cache.entry = Some(CachedDrift {
        indexed: indexed.head_oid,
        checkout: view.dir,
        head: view.head,
        note: note.clone(),
    });
    Some(note)
}

fn resolve_checkout(bound: &Path, caller_root: Option<&str>) -> Option<CheckoutView> {
    if let Some(nested) = caller_root.and_then(|caller| nested_checkout(bound, Path::new(caller))) {
        let same_repository = match (git_common_dir(&nested), git_common_dir(bound)) {
            (Some(caller_common), Some(bound_common)) => caller_common == bound_common,
            _ => false,
        };
        if same_repository {
            let head = head_oid(&nested)?;
            return Some(CheckoutView {
                dir: nested,
                head,
                other_checkout: true,
            });
        }
        // An independent repository nested in the root has another history;
        // the graph's history is the bound checkout's.
    }
    Some(CheckoutView {
        dir: bound.to_path_buf(),
        head: head_oid(bound)?,
        other_checkout: false,
    })
}

/// The nearest checkout root at or above `caller` and strictly below `bound`
/// (a directory holding a `.git` file or directory), if any.
fn nested_checkout(bound: &Path, caller: &Path) -> Option<PathBuf> {
    let bound = std::fs::canonicalize(bound).ok()?;
    let caller = std::fs::canonicalize(caller).ok()?;
    if caller == bound || !caller.starts_with(&bound) {
        return None;
    }
    let mut dir = caller.as_path();
    while dir != bound {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
        dir = dir.parent()?;
    }
    None
}

fn measure(bound: &Path, indexed: &IndexedCommit, view: &CheckoutView) -> serde_json::Value {
    // Worktrees share one object store, so both commits resolve from the bound
    // root, and the file count is scoped to the subtree that root indexes.
    let count = |range: String| {
        git_stdout(bound, &["rev-list", "--count", &range])
            .and_then(|out| String::from_utf8(out).ok())
            .and_then(|text| text.trim().parse::<u64>().ok())
    };
    let behind = count(format!("{}..{}", indexed.head_oid, view.head));
    let ahead = count(format!("{}..{}", view.head, indexed.head_oid));
    let files_changed = git_stdout(
        bound,
        &[
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            &indexed.head_oid,
            &view.head,
            "--",
            ".",
        ],
    )
    .map(|out| {
        out.split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .count() as u64
    });

    let mut note = serde_json::Map::new();
    note.insert("indexed".into(), short_oid(&indexed.head_oid).into());
    note.insert("head".into(), short_oid(&view.head).into());
    note.insert("behind".into(), serde_json::json!(behind));
    if let Some(ahead) = ahead.filter(|ahead| *ahead > 0) {
        note.insert("ahead".into(), serde_json::json!(ahead));
    }
    note.insert("files_changed".into(), serde_json::json!(files_changed));
    if view.other_checkout {
        note.insert(
            "checkout".into(),
            serde_json::json!(view.dir.to_string_lossy()),
        );
    } else {
        note.insert(
            "refresh".into(),
            serde_json::json!({ "tool": "ingest", "mode": "refresh", "path": indexed.root }),
        );
    }
    serde_json::Value::Object(note)
}

fn short_oid(oid: &str) -> String {
    oid.chars().take(SHORT_OID_LEN).collect()
}

fn head_oid(dir: &Path) -> Option<String> {
    let out = git_stdout(dir, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])?;
    let oid = String::from_utf8(out).ok()?.trim().to_string();
    ((oid.len() == 40 || oid.len() == 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then_some(oid)
}

fn git_common_dir(dir: &Path) -> Option<PathBuf> {
    let out = git_stdout(dir, &["rev-parse", "--git-common-dir"])?;
    let text = String::from_utf8(out).ok()?;
    let raw = PathBuf::from(text.trim());
    let absolute = if raw.is_absolute() {
        raw
    } else {
        dir.join(raw)
    };
    std::fs::canonicalize(absolute).ok()
}

/// Run a read-only git query in `dir`. `GIT_OPTIONAL_LOCKS=0` keeps git from
/// taking even the opportunistic index lock a status-like command may take.
fn git_stdout(dir: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code_graph(lines: u32) -> Graph {
        let mut graph = Graph::new();
        graph
            .add_node(
                "file::src/lib.rs",
                "src/lib.rs",
                m1nd_core::types::NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("file node");
        let node = graph
            .add_node(
                "file::src/lib.rs::fn::target",
                "target",
                m1nd_core::types::NodeType::Function,
                &[],
                0.0,
                0.0,
            )
            .expect("fn node");
        graph.set_node_provenance(
            node,
            m1nd_core::graph::NodeProvenanceInput {
                source_path: Some("src/lib.rs"),
                line_start: Some(lines),
                line_end: Some(lines + 2),
                ..Default::default()
            },
        );
        graph.finalize().expect("finalize");
        graph
    }

    fn record(root: &str, graph: &Graph) -> IndexedVcsStateV1 {
        IndexedVcsStateV1 {
            schema: INDEXED_VCS_STATE_SCHEMA.to_string(),
            head_oid: "a".repeat(40),
            root: root.to_string(),
            code_witness: code_graph_witness(graph),
        }
    }

    #[test]
    fn the_witness_moves_with_the_code_graph_only() {
        let base = code_graph(1);
        assert_eq!(
            code_graph_witness(&base),
            code_graph_witness(&code_graph(1))
        );
        assert_ne!(
            code_graph_witness(&base),
            code_graph_witness(&code_graph(13))
        );
        let mut with_memory = code_graph(1);
        with_memory
            .add_node(
                "light::agent-memory::claim",
                "a memorized claim",
                m1nd_core::types::NodeType::Concept,
                &[],
                0.0,
                0.0,
            )
            .expect("memory node");
        assert_eq!(code_graph_witness(&base), code_graph_witness(&with_memory));
    }

    #[test]
    fn every_doubtful_record_reads_back_as_unknown() {
        let dir = tempfile::tempdir().expect("tempdir");
        let runtime = dir.path().join("runtime");
        let root = dir.path().join("repo-alpha");
        std::fs::create_dir_all(&runtime).expect("runtime");
        std::fs::create_dir_all(&root).expect("root");
        let declared = vec![root.to_string_lossy().to_string()];
        let canonical = crate::project_brains::ProjectBrainRegistry::canonical_key(&declared[0]);
        let graph = code_graph(1);
        let path = runtime.join(INDEXED_VCS_STATE_FILE);
        let put = |record: &IndexedVcsStateV1| {
            write_atomically(&runtime, &path, record).expect("write record");
        };

        // Missing: unknown, silently.
        assert_eq!(load_indexed_commit(&runtime, &graph, &declared), None);

        // A sound record for this graph and root is the one case that is known.
        put(&record(&canonical, &graph));
        assert_eq!(
            load_indexed_commit(&runtime, &graph, &declared),
            Some(IndexedCommit::new(&"a".repeat(40), &canonical))
        );
        assert!(!runtime.join(INDEXED_VCS_STATE_TEMP_FILE).exists());

        // Recorded for another code graph (a crash between the two writes).
        put(&record(&canonical, &code_graph(13)));
        assert_eq!(load_indexed_commit(&runtime, &graph, &declared), None);

        // Recorded for a root this brain does not declare.
        put(&record("/elsewhere/repo-beta", &graph));
        assert_eq!(load_indexed_commit(&runtime, &graph, &declared), None);

        // Unparsable, truncated, or from an unknown schema.
        std::fs::write(&path, b"{not json").expect("garbage");
        assert_eq!(load_indexed_commit(&runtime, &graph, &declared), None);
        let mut foreign = record(&canonical, &graph);
        foreign.schema = "m1nd-indexed-vcs-state-v9".into();
        put(&foreign);
        assert_eq!(load_indexed_commit(&runtime, &graph, &declared), None);
        let mut short = record(&canonical, &graph);
        short.head_oid = "abc".into();
        put(&short);
        assert_eq!(load_indexed_commit(&runtime, &graph, &declared), None);
    }

    #[test]
    fn nested_checkout_finds_only_checkouts_strictly_inside_the_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let bound = dir.path().join("repo-alpha");
        let plain = bound.join("src").join("deep");
        let worktree = bound.join(".worktrees").join("feature");
        std::fs::create_dir_all(&plain).expect("plain dir");
        std::fs::create_dir_all(worktree.join("src")).expect("worktree dir");
        std::fs::write(worktree.join(".git"), "gitdir: elsewhere\n").expect("gitdir file");
        let canonical_worktree = std::fs::canonicalize(&worktree).expect("canonical");

        assert_eq!(nested_checkout(&bound, &bound), None);
        assert_eq!(nested_checkout(&bound, &plain), None);
        assert_eq!(
            nested_checkout(&bound, &worktree.join("src")),
            Some(canonical_worktree.clone())
        );
        assert_eq!(nested_checkout(&bound, &worktree), Some(canonical_worktree));
        assert_eq!(nested_checkout(&bound, dir.path()), None);
    }
}
