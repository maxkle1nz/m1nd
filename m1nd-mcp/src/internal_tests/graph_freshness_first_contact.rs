//! Freshness at first contact — the in-process half of the battery.
//!
//! The process-restart half (a stale graph declared on `north`/`health`, the
//! named refresh clearing it) lives in `tests/graph_freshness_first_contact.rs`.
//! This file pins the parts that need a hand on `caller_root`, which no stdio
//! client can set: a caller working in a git worktree of the indexed repository,
//! at another commit, must get an honest signal — the drift, or a root mismatch —
//! and never silence. It also pins the cost side: a fresh graph adds nothing, and
//! the note is not a new verb.

use crate as m1nd_mcp;

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;

const AGENT: &str = "graph-freshness-internal-probe";

/// The registry size this change must leave untouched (criteria F4: no new verb).
const REGISTERED_TOOL_COUNT: usize = 147;

fn git_available() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

/// Run git in `dir` with no user or system configuration in play.
fn git(dir: &Path, git_home: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=freshness-fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .env("HOME", git_home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn write_crate(root: &Path, padding: usize) {
    std::fs::create_dir_all(root.join("src")).expect("mk src");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"repo-alpha\"\nversion = \"0.0.0\"\n",
    )
    .expect("Cargo.toml");
    let mut source = String::new();
    for index in 0..padding {
        source.push_str(&format!("pub fn filler_{index}() -> u64 {{ {index} }}\n"));
    }
    source.push_str("pub fn worktree_probe_target() -> u64 {\n    42\n}\n");
    std::fs::write(root.join("src/lib.rs"), source).expect("lib.rs");
}

fn canonical(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

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

/// A repository at C1, its graph built by the production code ingest.
struct Fixture {
    _temp: tempfile::TempDir,
    git_home: PathBuf,
    main: PathBuf,
    c1: String,
    state: SessionState,
}

fn indexed_fixture() -> Fixture {
    let temp = tempfile::tempdir().expect("tempdir");
    let git_home = temp.path().join("git-home");
    std::fs::create_dir_all(&git_home).expect("git home");
    let main = temp.path().join("repo-alpha");
    write_crate(&main, 0);
    git(&main, &git_home, &["init", "-q"]);
    git(&main, &git_home, &["add", "-A"]);
    git(&main, &git_home, &["commit", "-q", "-m", "c1"]);
    let c1 = git(&main, &git_home, &["rev-parse", "HEAD"]);

    let mut state = build_state(&temp.path().join("runtime"));
    let ingested = dispatch_tool(
        &mut state,
        "ingest",
        &json!({
            "agent_id": AGENT,
            "path": canonical(&main),
            "adapter": "code",
            "mode": "replace",
        }),
    )
    .expect("index the fixture at C1");
    assert!(
        ingested["node_count"].as_u64().unwrap_or(0) > 0,
        "the fixture must produce a graph: {ingested}"
    );
    Fixture {
        _temp: temp,
        git_home,
        main,
        c1,
        state,
    }
}

/// A worktree of the fixture repository on its own branch, one commit past C1.
fn worktree_at_another_commit(fixture: &Fixture, worktree: &Path, branch: &str) -> String {
    let path = worktree.to_string_lossy().to_string();
    git(
        &fixture.main,
        &fixture.git_home,
        &["worktree", "add", "-q", "-b", branch, &path],
    );
    write_crate(worktree, 12);
    git(
        worktree,
        &fixture.git_home,
        &["commit", "-q", "-am", branch],
    );
    let head = git(worktree, &fixture.git_home, &["rev-parse", "HEAD"]);
    assert_ne!(head, fixture.c1, "the worktree must sit at another commit");
    head
}

fn north(state: &mut SessionState) -> serde_json::Value {
    dispatch_tool(
        state,
        "north",
        &json!({ "agent_id": AGENT, "task": "where is worktree_probe_target defined" }),
    )
    .expect("north")
}

fn health(state: &mut SessionState) -> serde_json::Value {
    dispatch_tool(state, "health", &json!({ "agent_id": AGENT })).expect("health")
}

/// Honest = the packet either refuses to vouch for the caller's repo (reception
/// mismatch) or declares the drift against the caller's own checkout.
fn assert_honest(surface: &str, packet: &serde_json::Value, indexed: &str, head: &str) {
    let mismatch = packet["reception"]["match"].as_str() == Some("caller_root_mismatch");
    let drift = packet.get("graph_drift").is_some_and(|note| {
        note["indexed"].as_str() == Some(&indexed[..12])
            && note["head"].as_str() == Some(&head[..12])
    });
    assert!(
        mismatch || drift,
        "{surface}: a caller in a worktree at another commit got neither a root mismatch nor \
         a drift note — silence: {packet}"
    );
}

#[test]
fn a_caller_in_a_worktree_at_another_commit_is_never_met_with_silence() {
    if !git_available() {
        eprintln!("skipping: git is not on PATH");
        return;
    }
    let mut fixture = indexed_fixture();

    // Nested: the worktree lives INSIDE the indexed root, so the bound brain
    // covers it by path and reception alone stays silent.
    let nested = fixture.main.join(".worktrees").join("feature");
    let nested_head = worktree_at_another_commit(&fixture, &nested, "feature");
    fixture.state.caller_root = Some(canonical(&nested));
    let c1 = fixture.c1.clone();
    let packet = north(&mut fixture.state);
    assert_honest("north (nested worktree)", &packet, &c1, &nested_head);
    let packet = health(&mut fixture.state);
    assert_honest("health (nested worktree)", &packet, &c1, &nested_head);

    // Sibling: the worktree lives outside the indexed root.
    let sibling = fixture
        .main
        .parent()
        .expect("fixture parent")
        .join("repo-alpha-sibling");
    let sibling_head = worktree_at_another_commit(&fixture, &sibling, "sibling");
    fixture.state.caller_root = Some(canonical(&sibling));
    let packet = north(&mut fixture.state);
    assert_honest("north (sibling worktree)", &packet, &c1, &sibling_head);
    let packet = health(&mut fixture.state);
    assert_honest("health (sibling worktree)", &packet, &c1, &sibling_head);
}

#[test]
fn a_graph_at_head_adds_no_drift_key() {
    if !git_available() {
        eprintln!("skipping: git is not on PATH");
        return;
    }
    let mut fixture = indexed_fixture();
    fixture.state.caller_root = Some(canonical(&fixture.main));
    let packet = north(&mut fixture.state);
    assert!(packet.get("graph_drift").is_none(), "{packet}");
    let packet = health(&mut fixture.state);
    assert!(packet.get("graph_drift").is_none(), "{packet}");
}

#[test]
fn the_drift_note_is_not_a_new_verb() {
    let registered = m1nd_mcp::server::all_tool_schemas()["tools"]
        .as_array()
        .map(Vec::len)
        .unwrap_or(0);
    assert_eq!(registered, REGISTERED_TOOL_COUNT);
    assert_eq!(
        m1nd_mcp::action_routes::MCP_TOOL_ROUTE_NAMES.len(),
        REGISTERED_TOOL_COUNT
    );
}
