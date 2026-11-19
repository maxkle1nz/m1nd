//! Freshness at first contact: is the served code graph behind the checkout?
//!
//! A full code ingest records the commit its walk observed at `HEAD`
//! ([`IndexedVcsStateV1`], persisted as [`INDEXED_VCS_STATE_FILE`] and carried
//! in every brain checkpoint). `north` and `health` compare it with the `HEAD`
//! of the checkout the caller works in and, only when the two differ, carry one
//! short `graph_drift` note: the indexed commit, the current `HEAD`, how many
//! commits and files separate them, and the exact refresh call. A graph at
//! `HEAD` adds nothing to either packet.
//!
//! The comparison is read-only. At a matching `HEAD` it costs one
//! `git rev-parse`; at a drifted `HEAD` the counts (`rev-list --count`,
//! `diff --name-only` between the two commits — no index, no working tree) are
//! measured once and memoized per (indexed commit, checkout, `HEAD`), so the
//! next call at the same `HEAD` costs one `git rev-parse` again. Nothing here
//! writes: not the runtime, not the repository, not the index.
//!
//! Whose `HEAD`? The bound root's, unless the caller's root is known and sits in
//! a checkout of its own nested inside the bound root that shares the bound
//! root's repository — a git worktree. That caller reads files at its own
//! commit, so the note is measured against its checkout and names it, because a
//! refresh of the bound root would not bring the graph to that commit. A caller
//! outside the bound root is already told by `reception` that this graph does
//! not cover it.

use crate::session::SessionState;
use m1nd_core::error::{M1ndError, M1ndResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Runtime-root file that persists the indexed commit next to the graph.
pub const INDEXED_VCS_STATE_FILE: &str = "indexed_vcs_state.json";
/// Checkpoint logical name of [`INDEXED_VCS_STATE_FILE`].
pub const INDEXED_VCS_STATE_LOGICAL_NAME: &str = "indexed_vcs_state";
const INDEXED_VCS_STATE_SCHEMA: &str = "m1nd-indexed-vcs-state-v1";
/// Abbreviated commit length used in the note: unambiguous in practice and
/// a fraction of the bytes of a full object id.
const SHORT_OID_LEN: usize = 12;

/// The commit a code graph was built from, and the root that was scanned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexedVcsStateV1 {
    pub schema: String,
    /// Full object id of the commit at `HEAD` when the ingest walked the root.
    pub head_oid: String,
    /// Canonical root the ingest scanned — the exact root a refresh must name.
    pub root: String,
}

impl IndexedVcsStateV1 {
    pub fn new(head_oid: &str, root: &str) -> Self {
        Self {
            schema: INDEXED_VCS_STATE_SCHEMA.to_string(),
            head_oid: head_oid.to_string(),
            root: root.to_string(),
        }
    }

    fn is_valid(&self) -> bool {
        self.schema == INDEXED_VCS_STATE_SCHEMA
            && (self.head_oid.len() == 40 || self.head_oid.len() == 64)
            && self.head_oid.bytes().all(|byte| byte.is_ascii_hexdigit())
            && !self.root.trim().is_empty()
    }

    /// Byte-deterministic encoding shared by direct persistence and the
    /// checkpoint candidate.
    pub fn encode(&self) -> M1ndResult<Vec<u8>> {
        Ok(serde_json::to_vec_pretty(self)?)
    }

    /// Strict decode: unknown fields, another schema, or a malformed commit id
    /// are corruption, never a silent default.
    pub fn decode(bytes: &[u8]) -> M1ndResult<Self> {
        let state: Self = serde_json::from_slice(bytes)?;
        if !state.is_valid() {
            return Err(M1ndError::CorruptState {
                reason: format!(
                    "{INDEXED_VCS_STATE_FILE} is not a valid {INDEXED_VCS_STATE_SCHEMA} record"
                ),
            });
        }
        Ok(state)
    }
}

/// Friendly boot: a missing file is "indexed commit unknown"; an unreadable or
/// malformed one is the same, said out loud. The note then stays silent until
/// the next code ingest records a commit again.
pub fn load_indexed_vcs_state(path: &Path) -> Option<IndexedVcsStateV1> {
    match std::fs::read(path) {
        Ok(bytes) => match IndexedVcsStateV1::decode(&bytes) {
            Ok(state) => Some(state),
            Err(error) => {
                eprintln!(
                    "[m1nd] WARNING: ignoring {} ({error}); the indexed commit is unknown until the next code ingest",
                    path.display()
                );
                None
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            eprintln!(
                "[m1nd] WARNING: cannot read {} ({error}); the indexed commit is unknown until the next code ingest",
                path.display()
            );
            None
        }
    }
}

/// Direct (non-checkpoint) persistence: write the record, or remove a record
/// that no longer describes the graph.
pub fn save_indexed_vcs_state(path: &Path, state: Option<&IndexedVcsStateV1>) -> M1ndResult<()> {
    match state {
        Some(state) => {
            let tmp = path.with_extension("json.tmp");
            std::fs::write(&tmp, state.encode()?)?;
            std::fs::rename(&tmp, path)?;
            Ok(())
        }
        None => match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        },
    }
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

fn measure(bound: &Path, indexed: &IndexedVcsStateV1, view: &CheckoutView) -> serde_json::Value {
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

    #[test]
    fn record_round_trips_and_refuses_corruption() {
        let state = IndexedVcsStateV1::new(&"a".repeat(40), "/repo-alpha");
        let bytes = state.encode().expect("encode");
        assert_eq!(IndexedVcsStateV1::decode(&bytes).expect("decode"), state);
        assert_eq!(bytes, state.encode().expect("deterministic encode"));

        let short = IndexedVcsStateV1::new("abc", "/repo-alpha");
        assert!(IndexedVcsStateV1::decode(&short.encode().unwrap()).is_err());
        let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        value["extra"] = serde_json::json!(1);
        assert!(IndexedVcsStateV1::decode(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[test]
    fn save_writes_then_removes_the_record() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(INDEXED_VCS_STATE_FILE);
        let state = IndexedVcsStateV1::new(&"b".repeat(40), "/repo-alpha");
        save_indexed_vcs_state(&path, Some(&state)).expect("save");
        assert_eq!(load_indexed_vcs_state(&path), Some(state));
        save_indexed_vcs_state(&path, None).expect("remove");
        assert!(!path.exists());
        save_indexed_vcs_state(&path, None).expect("absent stays absent");
        assert_eq!(load_indexed_vcs_state(&path), None);
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
