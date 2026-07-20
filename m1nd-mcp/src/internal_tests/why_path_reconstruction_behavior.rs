//! Behavioral coverage for the `why` MCP tool via the real `dispatch_tool`
//! harness. After code-ingesting a tiny two-file Rust project, `main.rs`
//! imports and uses `Helper` from `helper.rs`, which produces a real
//! cross-file edge `file::src/main.rs -> file::src/helper.rs::struct::Helper`
//! (the same edge the in-crate test `memorize_after_code_ingest_preserves_code_edges`
//! depends on). These tests assert that `why` reconstructs that exact path and
//! that an unresolvable target yields `found == false` with an empty path and
//! the canonical "not found" reason.
//!
//! This is X-RAY coverage for the path-reconstruction contract: the `found`
//! flag and reconstructed node sequence must reflect the actual ingested edge,
//! not merely a non-null shape.

use crate as m1nd_mcp;

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

/// Create a unique scratch directory under the OS temp dir for one test.
fn unique_temp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("m1nd_why_{tag}_{}_{nanos}", std::process::id()));
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
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    assert!(
        edge_count > 2,
        "code ingest must leave structural edges in the live graph, got edge_count={edge_count}"
    );
    state
}

#[test]
fn why_reconstructs_real_cross_file_import_path() {
    let root = unique_temp_dir("found");
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
        result.get("found").and_then(|value| value.as_bool()),
        Some(true),
        "why must find the ingested cross-file edge, got: {result}"
    );

    let paths = result
        .get("paths")
        .and_then(|value| value.as_array())
        .expect("paths array present");
    assert!(
        !paths.is_empty(),
        "a found connection must yield at least one path, got: {result}"
    );

    let first = &paths[0];
    let hops = first
        .get("hops")
        .and_then(|value| value.as_u64())
        .expect("hops present on first path");
    assert!(
        hops >= 1,
        "reconstructed path must traverse at least one edge, got hops={hops}"
    );

    let nodes = first
        .get("nodes")
        .and_then(|value| value.as_array())
        .expect("nodes present on first path");
    assert!(
        nodes.len() >= 2,
        "a >=1-hop path must list at least source and target labels, got: {nodes:?}"
    );

    let first_label = nodes
        .first()
        .and_then(|value| value.as_str())
        .expect("first node label is a string");
    let last_label = nodes
        .last()
        .and_then(|value| value.as_str())
        .expect("last node label is a string");
    // The graph labels the source node by its relative path ("src/main.rs"),
    // so match the suffix rather than a bare file name.
    assert!(
        first_label.ends_with("main.rs"),
        "path must start at the source node, got: {nodes:?}"
    );
    assert_eq!(
        last_label, "Helper",
        "path must end at the target node label, got: {nodes:?}"
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn why_reports_not_found_for_unresolvable_target() {
    let root = unique_temp_dir("missing");
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

    // `why` signals "not found" via an empty paths list + a canonical reason
    // string (there is no boolean `found` field on this response).
    let paths = result
        .get("paths")
        .and_then(|value| value.as_array())
        .expect("paths array present");
    assert!(
        paths.is_empty(),
        "an unresolvable target must yield no paths, got: {paths:?}"
    );

    assert_eq!(
        result.get("reason").and_then(|value| value.as_str()),
        Some("One or both nodes not found"),
        "an unresolvable node must surface the canonical reason, got: {result}"
    );

    fs::remove_dir_all(&root).ok();
}
