//! End-to-end coverage for the `why` closure verdict (m1nd MOVE #3). The verdict
//! is m1nd's honesty surface: when a load-bearing edge on the answer path rests
//! on a reference m1nd could NOT cleanly resolve (an ambiguous same-name guess,
//! or a dropped target), `why` must report `closure.state == "blocked"` and list
//! the offending edge. A path whose every load-bearing edge resolved cleanly
//! must report `closure.state == "closed"`.
//!
//! These tests drive the REAL `dispatch_tool` ingest+why path so the provenance
//! tags are written by the actual resolver, not synthesised — proving the verdict
//! reflects genuine binding uncertainty.
//!
//! Fixture grounding (verified by ingesting the canonical two-file Rust project):
//! the source file node `file::src/main.rs` carries an `m1nd:edge:unresolved`
//! provenance tag because one of its `use`/`mod` references did not resolve to a
//! single target in this minimal graph — so the `file::src/main.rs --imports-->
//! Helper` edge is honestly "dangling" and `why(main.rs, Helper)` must be
//! `blocked`. The downstream `file::src/helper.rs --contains--> Helper` edge has a
//! clean source (`helper.rs` is untagged), so `why(helper.rs, Helper)` is
//! `closed`.

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

fn build_state(root: &Path) -> SessionState {
    let config = McpConfig {
        graph_source: root.join("graph_snapshot.json"),
        plasticity_state: root.join("plasticity_state.json"),
        runtime_dir: Some(root.to_path_buf()),
        ..McpConfig::default()
    };
    SessionState::initialize(Graph::new(), &config, DomainConfig::code()).expect("init session")
}

fn call(state: &mut SessionState, tool: &str, params: serde_json::Value) -> serde_json::Value {
    dispatch_tool(state, tool, &params).expect("tool call")
}

fn unique_temp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("m1nd_closure_{tag}_{}_{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("create unique temp dir");
    dir
}

/// Write the canonical two-file Rust project and code-ingest it, returning the
/// initialized session with a populated graph.
fn ingest_two_file_project(root: &Path) -> SessionState {
    let proj = root.join("proj");
    fs::create_dir_all(proj.join("src")).expect("proj src dir");
    fs::write(proj.join("src/helper.rs"), "pub struct Helper;\n").expect("write helper");
    fs::write(
        proj.join("src/main.rs"),
        "mod helper;\nuse crate::helper::Helper;\npub fn build(_: Helper) {}\n",
    )
    .expect("write main");

    let mut state = build_state(root);
    let ingest = call(
        &mut state,
        "ingest",
        json!({
            "agent_id": "tester",
            "path": proj.to_string_lossy().to_string(),
            "adapter": "code",
            "mode": "replace",
        }),
    );
    let edge_count = ingest
        .get("edge_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        edge_count > 2,
        "code ingest must leave structural edges in the live graph, got: {ingest}"
    );
    state
}

/// `why(main.rs, Helper)` rides the `file::src/main.rs --imports--> Helper` edge
/// whose SOURCE node (`main.rs`) carries the `m1nd:edge:unresolved` provenance
/// tag (a real dropped reference at ingest). The verdict must therefore be
/// `blocked`, listing the offending edge with the `unresolved` reason — m1nd
/// admitting the answer rests on an edge it could not cleanly resolve.
#[test]
fn why_reports_blocked_when_path_rests_on_dangling_edge() {
    let root = unique_temp_dir("blocked");
    let mut state = ingest_two_file_project(&root);

    let result = call(
        &mut state,
        "why",
        json!({
            "agent_id": "tester",
            "source": "file::src/main.rs",
            "target": "file::src/helper.rs::struct::Helper",
            "max_hops": 5,
        }),
    );

    assert_eq!(
        result.get("found").and_then(|v| v.as_bool()),
        Some(true),
        "why must find the import edge, got: {result}"
    );

    let closure = result
        .get("closure")
        .expect("closure present on why output");
    assert_eq!(
        closure.get("state").and_then(|v| v.as_str()),
        Some("blocked"),
        "a path whose load-bearing edge source is tagged must be blocked, got: {result}"
    );
    let dangling = closure
        .get("dangling_edges")
        .and_then(|v| v.as_array())
        .expect("dangling_edges array");
    assert!(
        !dangling.is_empty(),
        "blocked verdict must list the offending edge, got: {closure}"
    );
    let offender = &dangling[0];
    assert!(
        offender
            .get("source")
            .and_then(|v| v.as_str())
            .map(|s| s.contains("main.rs"))
            .unwrap_or(false),
        "offending edge source must be the tagged file node (main.rs), got: {offender}"
    );
    assert_eq!(
        offender.get("reason").and_then(|v| v.as_str()),
        Some("unresolved"),
        "offending edge reason must be 'unresolved', got: {offender}"
    );

    fs::remove_dir_all(&root).ok();
}

/// `why(helper.rs, Helper)` rides the `file::src/helper.rs --contains--> Helper`
/// edge whose SOURCE node (`helper.rs`) is NOT tagged — a cleanly-resolved edge.
/// With every load-bearing edge clean the verdict must be `closed` and list no
/// dangling edges. This is the honest counterpart to the blocked case above.
#[test]
fn why_reports_closed_when_path_is_fully_clean() {
    let root = unique_temp_dir("closed");
    let mut state = ingest_two_file_project(&root);

    let result = call(
        &mut state,
        "why",
        json!({
            "agent_id": "tester",
            "source": "file::src/helper.rs",
            "target": "file::src/helper.rs::struct::Helper",
            "max_hops": 5,
        }),
    );

    assert_eq!(
        result.get("found").and_then(|v| v.as_bool()),
        Some(true),
        "why must find the clean contains edge, got: {result}"
    );

    let closure = result
        .get("closure")
        .expect("closure present on why output");
    assert_eq!(
        closure.get("state").and_then(|v| v.as_str()),
        Some("closed"),
        "a fully-clean path must be closed, got: {result}"
    );
    assert_eq!(
        closure
            .get("dangling_edges")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(0),
        "a closed verdict must list no dangling edges, got: {closure}"
    );

    fs::remove_dir_all(&root).ok();
}

/// An unresolvable target yields no node pair, so there is no path and nothing
/// to be incomplete: the verdict must still be present and `closed` (empty),
/// never `blocked`.
#[test]
fn why_reports_closed_when_target_not_found() {
    let root = unique_temp_dir("nopath");
    let mut state = ingest_two_file_project(&root);

    let result = call(
        &mut state,
        "why",
        json!({
            "agent_id": "tester",
            "source": "file::src/main.rs",
            "target": "file::nonexistent.rs",
            "max_hops": 5,
        }),
    );

    let closure = result
        .get("closure")
        .expect("closure present even when a node is not found");
    assert_eq!(
        closure.get("state").and_then(|v| v.as_str()),
        Some("closed"),
        "no path means nothing to be incomplete -> closed, got: {result}"
    );
    assert_eq!(
        closure
            .get("dangling_edges")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(0),
        "not-found closure must list no dangling edges, got: {closure}"
    );

    fs::remove_dir_all(&root).ok();
}
