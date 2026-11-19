//! `impact` answers "what else usually changes when this changes".
//!
//! Git co-change catches the files an edit forgets to carry along — a sibling
//! manifest, the doc that describes the code, release metadata — which neither
//! text search nor the structural graph shows. These tests pin the compact
//! `co_change` field `impact` gains for that answer, driven through the same
//! policy gate + dispatch both transports run:
//!
//! * the field names the historical partners of the target's file, with their
//!   joint-commit count and the short sha of the latest joint commit;
//! * commits outside 2..=30 files teach nothing (initial imports, sweeps);
//! * the file itself and its direct (hop-1) structural neighbours are left
//!   out — the field adds what the graph does not already link;
//! * no history means no field, never an empty one;
//! * the field never grows past its byte budget;
//! * ties break by path, so the same graph and history give the same bytes;
//! * the history lives in a lenient sidecar OUTSIDE the strict checkpoint, so a
//!   runtime written by this binary still boots under the previous one.

use crate as m1nd_mcp;

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::server::{dispatch_tool, enforce_generic_action_policy, McpConfig};
use m1nd_mcp::session::SessionState;
use serde_json::{json, Value};
use std::path::Path;
use std::process::Command;

const AGENT: &str = "impact-co-change-probe";
const SIDECAR: &str = "co_change_index_v1.json";
/// The field's hard ceiling: about 300 tokens of agent context per call.
const FIELD_BYTE_BUDGET: usize = 1200;

fn build_state(runtime: &Path) -> SessionState {
    std::fs::create_dir_all(runtime).expect("runtime dir");
    let config = McpConfig {
        graph_source: runtime.join("graph_snapshot.json"),
        plasticity_state: runtime.join("plasticity_state.json"),
        runtime_dir: Some(runtime.to_path_buf()),
        registry_dir: Some(runtime.join("registry")),
        ..McpConfig::default()
    };
    SessionState::initialize(Graph::new(), &config, DomainConfig::code()).expect("init session")
}

fn canonical(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, content).expect("write");
}

fn append(path: &Path, line: &str) {
    let mut body = std::fs::read_to_string(path).unwrap_or_default();
    body.push_str(line);
    body.push('\n');
    write(path, &body);
}

/// Run git hermetically: fixed identity, no signing, no hooks, no system or
/// user config, so the fixture history is the same on every machine.
fn git(repo: &Path, args: &[&str], step: u64) -> String {
    let date = format!("{} +0000", 1_767_225_600 + step * 60);
    let output = Command::new("git")
        .current_dir(repo)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", repo.join(".fixture-gitconfig"))
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.com")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.com")
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_DATE", &date)
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "core.longpaths=true",
        ])
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("git {args:?}: {error}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// Commit the current tree after appending one line to each of `paths`.
/// Returns the new commit's full sha.
fn commit(repo: &Path, paths: &[&str], step: u64) -> String {
    for path in paths {
        let marker = if path.ends_with(".rs") {
            format!("// rev {step}")
        } else if path.ends_with(".toml") {
            format!("# rev {step}")
        } else {
            format!("rev {step}")
        };
        append(&repo.join(path), &marker);
    }
    git(repo, &["add", "-A"], step);
    git(
        repo,
        &[
            "commit",
            "--no-verify",
            "-q",
            "-m",
            &format!("chore: step {step}"),
        ],
        step,
    );
    git(repo, &["rev-parse", "HEAD"], step)
}

fn short(sha: &str) -> String {
    sha[..10].to_string()
}

/// The fixture crate. `a.rs` calls into `c.rs` (a structural edge); `b.rs`,
/// the manifest and the docs are joined to `a.rs` only by history.
fn write_tree(repo: &Path) {
    write(
        &repo.join("Cargo.toml"),
        "[package]\nname = \"repo-alpha\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    );
    write(
        &repo.join("src/lib.rs"),
        "pub mod a;\npub mod b;\npub mod c;\n",
    );
    write(
        &repo.join("src/a.rs"),
        "use crate::c;\n\npub fn alpha() -> i64 {\n    c::gamma() + 1\n}\n",
    );
    write(&repo.join("src/b.rs"), "pub fn beta() -> i64 {\n    2\n}\n");
    write(
        &repo.join("src/c.rs"),
        "pub fn gamma() -> i64 {\n    3\n}\n",
    );
    write(&repo.join("docs/alpha.md"), "# Alpha\n");
    write(&repo.join("CHANGELOG.md"), "# Changelog\n");
    for index in 0..25 {
        write(&repo.join(format!("fill/f{index:02}.txt")), "filler\n");
    }
}

struct History {
    cargo_last: String,
    docs_last: String,
    b_last: String,
}

/// The crafted history. Expected partners of `src/a.rs`:
/// `Cargo.toml` x3, `docs/alpha.md` x2, `src/b.rs` x1; `src/c.rs` x2 but it is
/// structural; `CHANGELOG.md` and `src/lib.rs` appear only in commits with
/// more than 30 files, which teach nothing.
fn write_history(repo: &Path) -> History {
    write_tree(repo);
    git(repo, &["init", "-q"], 0);
    // 32 files: the initial import is filtered out.
    git(repo, &["add", "-A"], 0);
    git(
        repo,
        &["commit", "--no-verify", "-q", "-m", "chore: import"],
        0,
    );
    commit(repo, &["src/a.rs", "Cargo.toml"], 1);
    commit(repo, &["src/a.rs", "Cargo.toml", "docs/alpha.md"], 2);
    let docs_last = commit(repo, &["src/a.rs", "docs/alpha.md"], 3);
    let cargo_last = commit(repo, &["src/a.rs", "Cargo.toml"], 4);
    commit(repo, &["src/a.rs", "src/c.rs"], 5);
    commit(repo, &["src/a.rs", "src/c.rs"], 6);
    let b_last = commit(repo, &["src/a.rs", "src/b.rs"], 7);
    commit(repo, &["src/b.rs"], 8);
    // 31 files: a sweep, filtered out.
    let mut sweep: Vec<String> = vec!["src/a.rs".into(), "CHANGELOG.md".into()];
    sweep.extend((0..29).map(|index| format!("fill/f{index:02}.txt")));
    let sweep_refs: Vec<&str> = sweep.iter().map(String::as_str).collect();
    commit(repo, &sweep_refs, 9);
    History {
        cargo_last,
        docs_last,
        b_last,
    }
}

/// Seed the brain the way the field seeds one (trusted library ingest, then
/// declare the root), then refresh through the agent-callable door.
fn seed_and_refresh(state: &mut SessionState, repo: &Path) -> Value {
    let (graph, _) = m1nd_ingest::Ingestor::new(m1nd_ingest::IngestConfig {
        root: repo.to_path_buf(),
        parallelism: 1,
        ..m1nd_ingest::IngestConfig::default()
    })
    .ingest()
    .expect("trusted fixture ingest");
    {
        let mut live = state.graph.write();
        *live = graph;
        if !live.finalized {
            live.finalize().expect("finalize seeded graph");
        }
    }
    state.rebuild_engines().expect("rebuild engines");
    let declared = canonical(repo);
    state.ingest_roots = vec![declared.clone()];
    state.workspace_root = Some(declared.clone());
    state.persist().expect("persist the seeded brain");
    state.caller_root = Some(declared.clone());
    refresh(state, repo)
}

fn refresh(state: &mut SessionState, repo: &Path) -> Value {
    let payload = admit_then_dispatch(
        state,
        "ingest",
        &json!({ "path": canonical(repo), "agent_id": AGENT, "mode": "refresh" }),
    )
    .expect("refresh");
    assert_eq!(payload["ok"], json!(true), "refresh refused: {payload}");
    payload
}

fn admit_then_dispatch(
    state: &mut SessionState,
    tool: &str,
    params: &Value,
) -> Result<Value, String> {
    enforce_generic_action_policy(tool, params).map_err(|error| error.to_string())?;
    dispatch_tool(state, tool, params).map_err(|error| error.to_string())
}

/// Refresh restores managed agent memory, which reads process environment;
/// serialize with every other test that does the same.
fn memory_env_guard() -> std::sync::MutexGuard<'static, ()> {
    crate::tools::agent_memory_test_env_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

fn impact(state: &mut SessionState, node_id: &str) -> Value {
    admit_then_dispatch(
        state,
        "impact",
        &json!({ "node_id": node_id, "agent_id": AGENT }),
    )
    .expect("impact")
}

fn partners(output: &Value) -> Vec<(String, u64, String)> {
    output["co_change"]
        .as_array()
        .unwrap_or_else(|| panic!("impact carried no co_change field: {output}"))
        .iter()
        .map(|entry| {
            (
                entry["file"].as_str().expect("file").to_string(),
                entry["together"].as_u64().expect("together"),
                entry["last"].as_str().expect("last").to_string(),
            )
        })
        .collect()
}

#[test]
fn impact_names_the_files_that_historically_change_with_the_target() {
    let _env = memory_env_guard();
    let temp = tempfile::tempdir().expect("tempdir");
    let repo = temp.path().join("repo-alpha");
    let history = write_history(&repo);
    let mut state = build_state(&temp.path().join("runtime"));
    seed_and_refresh(&mut state, &repo);

    let output = impact(&mut state, "file::src/a.rs");
    assert_eq!(
        partners(&output),
        vec![
            ("Cargo.toml".to_string(), 3, short(&history.cargo_last)),
            ("docs/alpha.md".to_string(), 2, short(&history.docs_last)),
            ("src/b.rs".to_string(), 1, short(&history.b_last)),
        ],
        "partners must be ranked by joint commits, skip the structural src/c.rs, \
         and ignore commits outside 2..=30 files: {output}"
    );
    let mut entry_keys: Vec<&String> = output["co_change"][0]
        .as_object()
        .expect("entry object")
        .keys()
        .collect();
    entry_keys.sort();
    assert_eq!(entry_keys, vec!["file", "last", "together"]);

    // A file whose only multi-file commits were filtered out has no history.
    let lib = impact(&mut state, "file::src/lib.rs");
    assert!(
        lib.get("co_change").is_none(),
        "no qualifying history must mean no field: {lib}"
    );

    // A symbol answers for its file.
    let symbol = impact(&mut state, "file::src/a.rs::fn::alpha");
    assert_eq!(
        partners(&symbol).first().map(|partner| partner.0.as_str()),
        Some("Cargo.toml"),
        "a symbol inside src/a.rs must carry its file's history: {symbol}"
    );
}

#[test]
fn impact_has_no_co_change_field_without_git_history() {
    let _env = memory_env_guard();
    let temp = tempfile::tempdir().expect("tempdir");
    let repo = temp.path().join("repo-beta");
    write_tree(&repo);
    let mut state = build_state(&temp.path().join("runtime"));
    seed_and_refresh(&mut state, &repo);

    let output = impact(&mut state, "file::src/a.rs");
    assert!(output.get("blast_radius").is_some(), "{output}");
    assert!(
        output.get("co_change").is_none(),
        "a root without history must not grow the field: {output}"
    );
    assert!(
        !temp.path().join("runtime").join(SIDECAR).exists(),
        "no history must leave no sidecar behind"
    );
}

#[test]
fn co_change_field_stays_within_its_byte_budget() {
    let _env = memory_env_guard();
    let temp = tempfile::tempdir().expect("tempdir");
    let repo = temp.path().join("repo-gamma");
    write_tree(&repo);
    git(&repo, &["init", "-q"], 0);
    git(&repo, &["add", "-A"], 0);
    git(
        &repo,
        &["commit", "--no-verify", "-q", "-m", "chore: import"],
        0,
    );
    // Eight partners with long paths (kept under Windows' MAX_PATH together
    // with the temp dir), each joined to a.rs twice.
    let deep = "a-long-directory-name-that-keeps-going/".repeat(3);
    let long_paths: Vec<String> = (0..8)
        .map(|index| format!("docs/{deep}partner-{index:02}-long-name.md"))
        .collect();
    for path in &long_paths {
        write(&repo.join(path), "# partner\n");
    }
    let mut paths: Vec<&str> = long_paths.iter().map(String::as_str).collect();
    paths.push("src/a.rs");
    commit(&repo, &paths, 1);
    commit(&repo, &paths, 2);

    let mut state = build_state(&temp.path().join("runtime"));
    seed_and_refresh(&mut state, &repo);
    let output = impact(&mut state, "file::src/a.rs");
    let field = output
        .get("co_change")
        .unwrap_or_else(|| panic!("history exists, field expected: {output}"));
    let entries = field.as_array().expect("array");
    assert!(
        !entries.is_empty() && entries.len() <= 5,
        "at most five partners: {field}"
    );
    let bytes = serde_json::to_vec(field).expect("encode").len();
    assert!(
        bytes <= FIELD_BYTE_BUDGET,
        "co_change must stay within {FIELD_BYTE_BUDGET} bytes, got {bytes}: {field}"
    );
}

#[test]
fn co_change_is_deterministic_and_breaks_ties_by_path() {
    let _env = memory_env_guard();
    let temp = tempfile::tempdir().expect("tempdir");
    let repo = temp.path().join("repo-delta");
    write_tree(&repo);
    write(&repo.join("notes/zeta.md"), "z\n");
    write(&repo.join("notes/alpha.md"), "a\n");
    write(&repo.join("notes/mid.md"), "m\n");
    git(&repo, &["init", "-q"], 0);
    git(&repo, &["add", "-A"], 0);
    git(
        &repo,
        &["commit", "--no-verify", "-q", "-m", "chore: import"],
        0,
    );
    let tied = [
        "src/a.rs",
        "notes/zeta.md",
        "notes/alpha.md",
        "notes/mid.md",
    ];
    commit(&repo, &tied, 1);
    let last = commit(&repo, &tied, 2);

    let mut state = build_state(&temp.path().join("runtime"));
    seed_and_refresh(&mut state, &repo);
    let first = impact(&mut state, "file::src/a.rs");
    assert_eq!(
        partners(&first),
        vec![
            ("notes/alpha.md".to_string(), 2, short(&last)),
            ("notes/mid.md".to_string(), 2, short(&last)),
            ("notes/zeta.md".to_string(), 2, short(&last)),
        ],
        "equal evidence must order by path: {first}"
    );
    let second = impact(&mut state, "file::src/a.rs");
    assert_eq!(
        serde_json::to_vec(&first["co_change"]).unwrap(),
        serde_json::to_vec(&second["co_change"]).unwrap()
    );
    refresh(&mut state, &repo);
    let after_refresh = impact(&mut state, "file::src/a.rs");
    assert_eq!(
        serde_json::to_vec(&first["co_change"]).unwrap(),
        serde_json::to_vec(&after_refresh["co_change"]).unwrap(),
        "the same graph and history must give the same field"
    );
}

#[test]
fn the_co_change_sidecar_stays_outside_the_strict_checkpoint() {
    let _env = memory_env_guard();
    let temp = tempfile::tempdir().expect("tempdir");
    let repo = temp.path().join("repo-epsilon");
    write_history(&repo);
    let runtime = temp.path().join("runtime");
    let mut state = build_state(&runtime);
    seed_and_refresh(&mut state, &repo);

    let sidecar = runtime.join(SIDECAR);
    assert!(
        sidecar.exists(),
        "a refresh of a git root must leave the co-change sidecar"
    );
    let with_sidecar = state
        .authoritative_checkpoint_state_digest()
        .expect("digest with sidecar");

    // The strict digest is blind to the sidecar: removing or corrupting it
    // changes nothing an older binary would verify.
    let original = std::fs::read(&sidecar).expect("read sidecar");
    std::fs::remove_file(&sidecar).expect("remove sidecar");
    assert_eq!(
        state
            .authoritative_checkpoint_state_digest()
            .expect("digest without sidecar"),
        with_sidecar
    );
    let without = impact(&mut state, "file::src/a.rs");
    assert!(without.get("co_change").is_none(), "{without}");

    std::fs::write(&sidecar, b"{ not json").expect("corrupt sidecar");
    assert_eq!(
        state
            .authoritative_checkpoint_state_digest()
            .expect("digest with corrupt sidecar"),
        with_sidecar
    );
    let corrupt = impact(&mut state, "file::src/a.rs");
    assert!(
        corrupt.get("co_change").is_none(),
        "a corrupt sidecar must degrade to no field: {corrupt}"
    );
    assert!(corrupt.get("blast_radius").is_some());

    std::fs::write(&sidecar, &original).expect("restore sidecar");
    let restored = impact(&mut state, "file::src/a.rs");
    assert!(restored.get("co_change").is_some(), "{restored}");

    // The sidecar is runtime-owned state: a runtime living inside its root
    // must never ingest it as source.
    assert!(crate::tools::RUNTIME_OWNED_ROOT_FILES.contains(&SIDECAR));
}

#[test]
fn git_history_seeds_the_predict_matrix_with_true_commit_counts() {
    let _env = memory_env_guard();
    let temp = tempfile::tempdir().expect("tempdir");
    let repo = temp.path().join("repo-zeta");
    write_history(&repo);
    let mut state = build_state(&temp.path().join("runtime"));
    seed_and_refresh(&mut state, &repo);

    let co_count_of = |state: &SessionState, from: &str, to: &str| -> u32 {
        let graph = state.graph.read();
        let from = graph.resolve_id(from).expect("from node");
        let to = graph.resolve_id(to).expect("to node");
        state
            .orchestrator
            .temporal
            .co_change
            .predict(from, 200)
            .into_iter()
            .find(|entry| entry.target == to)
            .map(|entry| entry.co_count)
            .unwrap_or(0)
    };
    // Two identical two-file commits are two observations, not one.
    assert_eq!(co_count_of(&state, "file::src/a.rs", "file::src/c.rs"), 2);
    assert_eq!(co_count_of(&state, "file::src/a.rs", "file::src/b.rs"), 1);
    // Only commits with 2..=30 files count: lib.rs appears only in the import.
    assert_eq!(co_count_of(&state, "file::src/a.rs", "file::src/lib.rs"), 0);

    // Refresh rebuilds the engines; the seed is re-applied, never doubled.
    refresh(&mut state, &repo);
    assert_eq!(co_count_of(&state, "file::src/a.rs", "file::src/c.rs"), 2);

    // A managed-memory write rebuilds the engines too; history survives it.
    let memorized = admit_then_dispatch(
        &mut state,
        "memorize",
        &json!({
            "agent_id": AGENT,
            "node_label": "AlphaNote",
            "claims": [{
                "label": "AlphaClaim",
                "text": "alpha delegates to gamma.",
                "confidence": "high",
                "evidence": ["src/a.rs"]
            }]
        }),
    )
    .expect("memorize");
    assert_eq!(memorized["ok"], json!(true), "{memorized}");
    assert_eq!(co_count_of(&state, "file::src/a.rs", "file::src/c.rs"), 2);

    // `predict`'s git half now answers from history.
    let predicted = admit_then_dispatch(
        &mut state,
        "predict",
        &json!({ "changed_node": "file::src/a.rs", "agent_id": AGENT, "top_k": 50 }),
    )
    .expect("predict");
    assert!(
        predicted.to_string().contains("src/c.rs"),
        "predict must see the git-derived partner: {predicted}"
    );
}

#[test]
fn a_seeded_matrix_round_trips_through_the_strict_recovery_codec() {
    let _env = memory_env_guard();
    let temp = tempfile::tempdir().expect("tempdir");
    let repo = temp.path().join("repo-eta");
    write_history(&repo);
    let runtime = temp.path().join("runtime");
    let mut state = build_state(&runtime);
    seed_and_refresh(&mut state, &repo);
    assert!(
        state.orchestrator.temporal.co_change.num_entries() > 0
            && state
                .orchestrator
                .temporal
                .co_change
                .export_state(&state.graph.read())
                .expect("export")
                .is_learned,
        "the refresh must have seeded the matrix"
    );

    // The bytes an older binary reads back are the bytes it would write: the
    // seeded matrix uses the existing temporal schema, nothing new.
    let graph = state.graph.read();
    let encoded = crate::temporal_state::encode_temporal_state(
        &graph,
        &state.temporal.co_change,
        &state.orchestrator.temporal.co_change,
    )
    .expect("encode");
    let (primary, orchestrator) =
        crate::temporal_state::decode_temporal_state(&encoded, &graph).expect("decode");
    let reencoded = crate::temporal_state::encode_temporal_state(&graph, &primary, &orchestrator)
        .expect("re-encode");
    assert_eq!(encoded, reencoded);
    drop(graph);

    // A cold boot over the persisted runtime keeps the seeded evidence.
    state.persist().expect("persist");
    let snapshot = m1nd_core::snapshot::load_graph(&runtime.join("graph_snapshot.json"))
        .expect("load persisted graph");
    let config = McpConfig {
        graph_source: runtime.join("graph_snapshot.json"),
        plasticity_state: runtime.join("plasticity_state.json"),
        runtime_dir: Some(runtime.to_path_buf()),
        registry_dir: Some(temp.path().join("registry-reboot")),
        ..McpConfig::default()
    };
    let rebooted =
        SessionState::initialize(snapshot, &config, DomainConfig::code()).expect("reboot");
    assert_eq!(
        rebooted.orchestrator.temporal.co_change.num_entries(),
        state.orchestrator.temporal.co_change.num_entries()
    );
}
