//! Git co-change history: what else usually changes when this file changes.
//!
//! A code refresh/replace of a git root reads the history once, keeps the
//! commits that touched 2..=30 files (an initial import or a repository-wide
//! sweep teaches nothing about coupling), and records it in a runtime sidecar.
//! Two consumers read it:
//!
//! * `impact` derives its compact `co_change` field from it on demand — the
//!   files that historically changed together with the target's file and that
//!   the graph does not already link to it directly;
//! * every graph install re-seeds the query orchestrator's co-change matrix
//!   (the git half of `predict`) from it, because each install rebuilds that
//!   matrix from structure alone.
//!
//! The history is read with its own `git log` instead of reusing the walker's
//! commit groups: those are de-duplicated by file set (two identical commits
//! count once) and carry no commit identity, and the walker's exact log bytes
//! are pinned by the strict VCS context digest, so they cannot be widened.
//!
//! COMPATIBILITY: the sidecar is NOT a checkpoint file. It is outside the
//! strict working set and the rebuild digest, so a runtime written by this
//! binary boots unchanged under a binary that has never heard of it. Reading is
//! lenient — a missing, corrupt, foreign-root or unknown-schema sidecar means
//! "no history", never an error. The seeded matrix uses the existing temporal
//! state schema; nothing new enters the strict state.

use crate::protocol::core::CoChangePartnerOutput;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

/// Runtime-root file name. Listed in `tools::RUNTIME_OWNED_ROOT_FILES`.
pub const CO_CHANGE_INDEX_FILE: &str = "co_change_index_v1.json";
const CO_CHANGE_INDEX_SCHEMA: &str = "m1nd-co-change-index-v1";
/// Commits touching fewer files carry no pair; more is a sweep, not coupling.
pub const CO_CHANGE_MIN_FILES: usize = 2;
pub const CO_CHANGE_MAX_FILES: usize = 30;
/// Newest commits read per root. Bounds the read, the sidecar and the matrix
/// seed on very long histories; recent coupling is the relevant coupling.
pub const CO_CHANGE_MAX_COMMITS: usize = 10_000;
/// At most this many partners in `impact`'s `co_change` field.
pub const IMPACT_CO_CHANGE_MAX_PARTNERS: usize = 5;
/// Hard ceiling on the serialized field: about 300 tokens of agent context.
pub const IMPACT_CO_CHANGE_BYTE_BUDGET: usize = 1200;
const SHORT_SHA_LEN: usize = 10;

#[derive(Debug, Serialize, Deserialize)]
struct CoChangeIndexV1 {
    schema: String,
    /// Canonical key of the root whose history this is. A brain that no longer
    /// declares this root ignores the file.
    root: String,
    /// Newest first (git log order). Each commit lists only files present in
    /// the scanned tree, and always at least two of them.
    commits: Vec<CoChangeCommitV1>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CoChangeCommitV1 {
    sha: String,
    files: Vec<String>,
}

impl CoChangeIndexV1 {
    fn is_valid(&self) -> bool {
        self.schema == CO_CHANGE_INDEX_SCHEMA
            && self.commits.iter().all(|commit| {
                commit.files.len() >= CO_CHANGE_MIN_FILES
                    && !commit.sha.is_empty()
                    && commit.sha.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
    }
}

struct CachedIndex {
    len: u64,
    modified: Option<SystemTime>,
    index: Option<Arc<CoChangeIndexV1>>,
}

fn cache() -> &'static Mutex<HashMap<PathBuf, CachedIndex>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedIndex>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn index_path(runtime_root: &Path) -> PathBuf {
    runtime_root.join(CO_CHANGE_INDEX_FILE)
}

/// Read one root's history: `git log` over HEAD, no merges, no rename
/// pairing, paths relative to `root`, at most the newest
/// [`CO_CHANGE_MAX_COMMITS`] commits. `None` when `root` is not inside a git
/// work tree, has no commits, or git output cannot be framed exactly.
fn read_git_commits(root: &Path) -> Option<Vec<(String, Vec<String>)>> {
    if !root.is_dir() {
        return None;
    }
    let max_count = format!("--max-count={CO_CHANGE_MAX_COMMITS}");
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "core.quotePath=false",
            "log",
            "--no-merges",
            "--no-renames",
            "--relative",
            "-z",
            "--format=%x00%H%x00",
            "--name-only",
            "--diff-filter=ACDMR",
            max_count.as_str(),
            "HEAD",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_git_log(&output.stdout)
}

/// NUL framing (same structure the walker parses): an empty boundary token,
/// the commit id, an empty terminator, then paths — the first prefixed by a
/// newline — until the next empty token. Any deviation rejects the whole read.
fn parse_git_log(stdout: &[u8]) -> Option<Vec<(String, Vec<String>)>> {
    let tokens: Vec<&[u8]> = stdout.split(|byte| *byte == 0).collect();
    let mut commits = Vec::new();
    let mut cursor = 0usize;
    while cursor < tokens.len() {
        if !tokens[cursor].is_empty() {
            return None;
        }
        cursor += 1;
        if cursor == tokens.len() {
            break;
        }
        let sha = std::str::from_utf8(tokens[cursor]).ok()?;
        if !(sha.len() == 40 || sha.len() == 64) || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        cursor += 1;
        if cursor >= tokens.len() || !tokens[cursor].is_empty() {
            return None;
        }
        cursor += 1;
        let mut files = Vec::new();
        let mut first = true;
        while cursor < tokens.len() && !tokens[cursor].is_empty() {
            let raw = if first {
                tokens[cursor].strip_prefix(b"\n")?
            } else {
                tokens[cursor]
            };
            files.push(std::str::from_utf8(raw).ok()?.to_string());
            first = false;
            cursor += 1;
        }
        commits.push((sha[..SHORT_SHA_LEN].to_ascii_lowercase(), files));
    }
    Some(commits)
}

/// Keep commits of 2..=30 changed files, then drop the files absent from the
/// scanned tree; a commit left with fewer than two files carries no pair.
fn build_index(
    root_key: String,
    commits: Vec<(String, Vec<String>)>,
    present: &BTreeSet<&str>,
) -> CoChangeIndexV1 {
    let commits = commits
        .into_iter()
        .filter(|(_, files)| (CO_CHANGE_MIN_FILES..=CO_CHANGE_MAX_FILES).contains(&files.len()))
        .filter_map(|(sha, files)| {
            let mut files: Vec<String> = files
                .into_iter()
                .filter(|file| present.contains(file.as_str()))
                .collect();
            files.sort();
            files.dedup();
            (files.len() >= CO_CHANGE_MIN_FILES).then_some(CoChangeCommitV1 { sha, files })
        })
        .collect();
    CoChangeIndexV1 {
        schema: CO_CHANGE_INDEX_SCHEMA.to_string(),
        root: root_key,
        commits,
    }
}

fn forget_cached(path: &Path) {
    cache().lock().remove(path);
}

/// Remove the sidecar (best effort): the graph no longer describes the root
/// whose history it holds.
pub(crate) fn clear(runtime_root: &Path) {
    let path = index_path(runtime_root);
    forget_cached(&path);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => eprintln!(
            "[m1nd] could not remove the co-change history at {}: {error}",
            path.display()
        ),
    }
}

/// Record the history of a freshly scanned code root. Lenient by design: a
/// root without usable history, or any I/O failure, leaves no sidecar and the
/// `co_change` field simply stays absent.
pub(crate) fn record_code_root(
    runtime_root: &Path,
    root: &str,
    discovered_files: &[m1nd_ingest::walker::DiscoveredFile],
) {
    let path = index_path(runtime_root);
    let Some(commits) = read_git_commits(Path::new(root)) else {
        clear(runtime_root);
        return;
    };
    let present: BTreeSet<&str> = discovered_files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect();
    let index = build_index(
        crate::project_brains::ProjectBrainRegistry::canonical_key(root),
        commits,
        &present,
    );
    if index.commits.is_empty() {
        clear(runtime_root);
        return;
    }
    let written = serde_json::to_vec(&index)
        .map_err(std::io::Error::other)
        .and_then(|bytes| write_atomically(&path, &bytes));
    forget_cached(&path);
    if let Err(error) = written {
        eprintln!(
            "[m1nd] could not record the co-change history at {}: {error}",
            path.display()
        );
        clear(runtime_root);
    }
}

fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let temp = parent.join(format!(
        ".{CO_CHANGE_INDEX_FILE}.tmp-{}",
        std::process::id()
    ));
    let result = std::fs::write(&temp, bytes).and_then(|()| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// The recorded history of a root this brain still declares, or `None`.
fn load(runtime_root: &Path, declared_roots: &[String]) -> Option<Arc<CoChangeIndexV1>> {
    let path = index_path(runtime_root);
    let metadata = std::fs::metadata(&path).ok()?;
    let (len, modified) = (metadata.len(), metadata.modified().ok());
    let index = {
        let mut cache = cache().lock();
        match cache.get(&path) {
            Some(hit) if hit.len == len && hit.modified == modified => hit.index.clone(),
            _ => {
                let index = std::fs::read(&path)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<CoChangeIndexV1>(&bytes).ok())
                    .filter(CoChangeIndexV1::is_valid)
                    .map(Arc::new);
                cache.insert(
                    path,
                    CachedIndex {
                        len,
                        modified,
                        index: index.clone(),
                    },
                );
                index
            }
        }
    }?;
    declared_roots
        .iter()
        .any(|root| root == &index.root)
        .then_some(index)
}

/// The file an external id belongs to (`file::src/a.rs::fn::alpha` ->
/// `src/a.rs`), or `None` for nodes that are not file-backed.
pub(crate) fn file_of_external_id(external_id: &str) -> Option<&str> {
    let rest = external_id.strip_prefix("file::")?;
    let file = rest.split("::").next().unwrap_or(rest);
    (!file.is_empty()).then_some(file)
}

/// `impact`'s `co_change` field: up to five files that historically changed
/// together with `target_file`, excluding `target_file` itself and every file
/// in `structural`. Ranked by joint commits, then by the most recent joint
/// commit, then by path; cut to the byte budget. `None` when nothing remains.
pub(crate) fn impact_partners(
    runtime_root: &Path,
    declared_roots: &[String],
    target_file: &str,
    structural: &BTreeSet<&str>,
) -> Option<Vec<CoChangePartnerOutput>> {
    let index = load(runtime_root, declared_roots)?;
    rank_partners(&index, target_file, structural)
}

fn rank_partners(
    index: &CoChangeIndexV1,
    target_file: &str,
    structural: &BTreeSet<&str>,
) -> Option<Vec<CoChangePartnerOutput>> {
    // file -> (joint commits, rank of the newest joint commit, its sha)
    let mut tally: BTreeMap<&str, (u32, usize, &str)> = BTreeMap::new();
    for (rank, commit) in index.commits.iter().enumerate() {
        if !commit.files.iter().any(|file| file == target_file) {
            continue;
        }
        for file in &commit.files {
            if file == target_file || structural.contains(file.as_str()) {
                continue;
            }
            tally
                .entry(file.as_str())
                .or_insert((0, rank, commit.sha.as_str()))
                .0 += 1;
        }
    }
    let mut ranked: Vec<(&str, (u32, usize, &str))> = tally.into_iter().collect();
    ranked.sort_by(
        |(file_a, (count_a, rank_a, _)), (file_b, (count_b, rank_b, _))| {
            count_b
                .cmp(count_a)
                .then(rank_a.cmp(rank_b))
                .then(file_a.cmp(file_b))
        },
    );

    let mut partners = Vec::new();
    // `[` and `]`, plus one comma between entries.
    let mut bytes = 2usize;
    for (file, (together, _, sha)) in ranked.into_iter().take(IMPACT_CO_CHANGE_MAX_PARTNERS) {
        let partner = CoChangePartnerOutput {
            file: file.to_string(),
            together,
            last: sha.to_string(),
        };
        let entry_bytes =
            serde_json::to_vec(&partner).map(|v| v.len()).ok()? + usize::from(!partners.is_empty());
        if bytes + entry_bytes > IMPACT_CO_CHANGE_BYTE_BUDGET {
            break;
        }
        bytes += entry_bytes;
        partners.push(partner);
    }
    (!partners.is_empty()).then_some(partners)
}

/// Re-seed the git half of the co-change matrix from the recorded history.
/// Called after every graph install (each one rebuilds the matrix from the
/// graph structure alone), so the seed is applied exactly once per rebuild.
pub(crate) fn seed_predict_matrix(state: &mut crate::session::SessionState) {
    let declared = state.declared_roots_canonical();
    let Some(index) = load(&state.runtime_root, &declared) else {
        return;
    };
    let groups: Vec<Vec<String>> = index
        .commits
        .iter()
        .map(|commit| commit.files.clone())
        .collect();
    let graph = Arc::clone(&state.graph);
    let graph = graph.read();
    if let Err(error) = state
        .orchestrator
        .temporal
        .co_change
        .populate_from_commit_groups(&graph, &groups)
    {
        eprintln!("[m1nd] co-change matrix seed skipped: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(commits: &[(&str, &[&str])]) -> CoChangeIndexV1 {
        CoChangeIndexV1 {
            schema: CO_CHANGE_INDEX_SCHEMA.to_string(),
            root: "/repo".to_string(),
            commits: commits
                .iter()
                .map(|(sha, files)| CoChangeCommitV1 {
                    sha: sha.to_string(),
                    files: files.iter().map(|file| file.to_string()).collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn framing_rejects_anything_but_exact_commit_records() {
        let sha = "a".repeat(40);
        let good = format!("\0{sha}\0\0\nsrc/a.rs\0docs/a.md\0");
        let parsed = parse_git_log(good.as_bytes()).expect("parse");
        assert_eq!(
            parsed,
            vec![(
                "a".repeat(SHORT_SHA_LEN),
                vec!["src/a.rs".to_string(), "docs/a.md".to_string()]
            )]
        );
        assert!(parse_git_log(b"not-a-boundary\0").is_none());
        assert!(parse_git_log(format!("\0{sha}\0\0src/a.rs\0").as_bytes()).is_none());
        assert!(parse_git_log(b"\0zz\0\0\nsrc/a.rs\0").is_none());
        assert_eq!(parse_git_log(b""), Some(Vec::new()));
    }

    #[test]
    fn index_keeps_two_to_thirty_file_commits_over_present_files() {
        let present: BTreeSet<&str> = ["a", "b", "c"].into_iter().collect();
        let sweep: Vec<String> = (0..31).map(|index| format!("f{index}")).collect();
        let mut sweep_with_a = sweep.clone();
        sweep_with_a[0] = "a".to_string();
        let built = build_index(
            "/repo".into(),
            vec![
                ("1".into(), vec!["a".into(), "b".into()]),
                ("2".into(), vec!["a".into()]),
                ("3".into(), sweep_with_a),
                ("4".into(), vec!["a".into(), "gone".into()]),
                ("5".into(), vec!["c".into(), "b".into(), "a".into()]),
            ],
            &present,
        );
        let kept: Vec<(&str, Vec<&str>)> = built
            .commits
            .iter()
            .map(|commit| {
                (
                    commit.sha.as_str(),
                    commit.files.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(
            kept,
            vec![("1", vec!["a", "b"]), ("5", vec!["a", "b", "c"])]
        );
    }

    #[test]
    fn ranking_is_count_then_recency_then_path_and_skips_structure() {
        let history = index(&[
            ("c3", &["t", "x", "y"]),
            ("c2", &["t", "z", "s"]),
            ("c1", &["t", "x", "z"]),
        ]);
        let structural: BTreeSet<&str> = ["s"].into_iter().collect();
        let ranked = rank_partners(&history, "t", &structural).expect("partners");
        let flat: Vec<(&str, u32, &str)> = ranked
            .iter()
            .map(|p| (p.file.as_str(), p.together, p.last.as_str()))
            .collect();
        assert_eq!(flat, vec![("x", 2, "c3"), ("z", 2, "c2"), ("y", 1, "c3")]);
        assert!(rank_partners(&history, "absent", &structural).is_none());
    }

    #[test]
    fn the_field_is_cut_to_its_byte_budget_in_rank_order() {
        let long: Vec<String> = (0..8)
            .map(|index| format!("docs/{}/partner-{index}.md", "deep".repeat(100)))
            .collect();
        let mut files: Vec<&str> = long.iter().map(String::as_str).collect();
        files.push("t");
        let history = index(&[("c2", &files), ("c1", &files)]);
        let ranked = rank_partners(&history, "t", &BTreeSet::new()).expect("partners");
        let bytes = serde_json::to_vec(&ranked).expect("encode").len();
        assert!(bytes <= IMPACT_CO_CHANGE_BYTE_BUDGET, "{bytes} bytes");
        assert!(!ranked.is_empty() && ranked.len() < IMPACT_CO_CHANGE_MAX_PARTNERS);
        assert_eq!(ranked[0].file, long[0], "rank order survives the cut");

        let short: Vec<String> = (0..8).map(|index| format!("p{index}")).collect();
        let mut files: Vec<&str> = short.iter().map(String::as_str).collect();
        files.push("t");
        let history = index(&[("c1", &files)]);
        let ranked = rank_partners(&history, "t", &BTreeSet::new()).expect("partners");
        assert_eq!(ranked.len(), IMPACT_CO_CHANGE_MAX_PARTNERS);
    }

    #[test]
    fn external_ids_map_to_their_file() {
        assert_eq!(file_of_external_id("file::src/a.rs"), Some("src/a.rs"));
        assert_eq!(
            file_of_external_id("file::src/a.rs::fn::alpha"),
            Some("src/a.rs")
        );
        assert_eq!(file_of_external_id("dir::src"), None);
        assert_eq!(file_of_external_id("file::"), None);
    }
}
