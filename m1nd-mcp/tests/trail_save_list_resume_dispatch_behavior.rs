//! Behavioral coverage for the `trail_save` -> `trail_list` -> `trail_resume`
//! tool chain driven through `dispatch_tool` (the real MCP entry point).
//!
//! The existing in-module unit tests in `layer_handlers.rs` cover the save/resume
//! internals by calling the handlers directly with hand-built nodes; they do NOT
//! exercise the `dispatch_tool` JSON path, the `trail_list` tag/status filtering,
//! or the on-disk save -> list -> resume roundtrip across freshly dispatched
//! calls against a graph populated by a real `ingest`. This X-RAY-style coverage
//! locks the dispatch contract: trail_id naming, save counts (including the
//! supporting-node upsert), tag-filter fidelity in trail_list, and node
//! resolution vs. a bogus node during resume.

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use serde_json::json;
use std::fs;
use std::path::Path;

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

fn u64_at(value: &serde_json::Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(|inner| inner.as_u64())
        .unwrap_or_else(|| panic!("expected numeric field {key:?} in {value}"))
}

/// Ingest a tiny two-file Rust project so that the trail's visited /
/// supporting node external ids resolve against a real populated graph.
/// Node id shape confirmed against the live `code` adapter:
///   `file::src/helper.rs::struct::Helper` and `file::src/main.rs`.
fn ingest_two_file_project(state: &mut SessionState, project: &Path) {
    fs::create_dir_all(project.join("src")).expect("create src dir");
    fs::write(
        project.join("src/helper.rs"),
        "pub struct Helper {\n    pub seed: u32,\n}\n\nimpl Helper {\n    pub fn make() -> Helper {\n        Helper { seed: 1 }\n    }\n}\n",
    )
    .expect("write helper.rs");
    fs::write(
        project.join("src/main.rs"),
        "mod helper;\n\nfn main() {\n    let _ = helper::Helper::make();\n}\n",
    )
    .expect("write main.rs");

    let ingest = call(
        state,
        "ingest",
        json!({
            "agent_id": "tester",
            "path": project.to_string_lossy().to_string(),
            "adapter": "code",
            "mode": "replace"
        }),
    );
    let node_count = ingest
        .get("nodes")
        .and_then(|inner| inner.as_u64())
        .or_else(|| ingest.get("node_count").and_then(|inner| inner.as_u64()));
    if let Some(count) = node_count {
        assert!(count > 0, "code ingest should populate the graph: {ingest}");
    }
}

#[test]
fn trail_save_list_resume_dispatch_roundtrip_is_behaviorally_faithful() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("proj");
    let mut state = build_state(temp.path());
    ingest_two_file_project(&mut state, &project);

    // --- trail_save: counts must reflect the exact input we passed ---
    let save = call(
        &mut state,
        "trail_save",
        json!({
            "agent_id": "tester",
            "label": "Investigation A",
            "tags": ["auth"],
            "hypotheses": [{
                "statement": "H1",
                "confidence": 0.8,
                "supporting_nodes": ["file::src/helper.rs::struct::Helper"]
            }],
            "conclusions": [{ "statement": "C1", "confidence": 0.9 }],
            "open_questions": ["q1", "q2"],
            "visited_nodes": [{ "node_external_id": "file::src/main.rs", "relevance": 0.7 }]
        }),
    );

    assert_eq!(
        u64_at(&save, "hypotheses_saved"),
        1,
        "exactly one hypothesis was supplied"
    );
    assert_eq!(
        u64_at(&save, "conclusions_saved"),
        1,
        "exactly one conclusion was supplied"
    );
    assert_eq!(
        u64_at(&save, "open_questions_saved"),
        2,
        "two open questions were supplied"
    );
    // visited (file::src/main.rs) + the hypothesis supporting node
    // (file::src/helper.rs::struct::Helper) are both upserted into visited_nodes.
    assert!(
        u64_at(&save, "nodes_saved") >= 2,
        "visited node plus hypothesis supporting node should both be saved: {save}"
    );

    let trail_id = save
        .get("trail_id")
        .and_then(|inner| inner.as_str())
        .expect("trail_id present")
        .to_string();
    assert!(
        trail_id.starts_with("trail_tester_001_"),
        "first trail for agent 'tester' must be counter 001: got {trail_id}"
    );

    // --- trail_list: matching tag returns exactly this trail with faithful counts ---
    let listed = call(
        &mut state,
        "trail_list",
        json!({ "agent_id": "tester", "filter_tags": ["auth"] }),
    );
    assert_eq!(
        u64_at(&listed, "total_count"),
        1,
        "the auth tag matches the one saved trail: {listed}"
    );
    let trails = listed
        .get("trails")
        .and_then(|inner| inner.as_array())
        .expect("trails array");
    assert_eq!(trails.len(), 1, "exactly one trail entry expected");
    let entry = &trails[0];
    assert_eq!(
        entry.get("trail_id").and_then(|inner| inner.as_str()),
        Some(trail_id.as_str()),
        "listed trail id must match the saved id"
    );
    assert_eq!(
        u64_at(entry, "hypothesis_count"),
        1,
        "summary hypothesis_count mirrors the saved hypothesis"
    );
    assert_eq!(
        u64_at(entry, "open_question_count"),
        2,
        "summary open_question_count mirrors the two saved questions"
    );

    // --- trail_list: non-matching tag is a real filter, not cosmetic ---
    let empty = call(
        &mut state,
        "trail_list",
        json!({ "agent_id": "tester", "filter_tags": ["nope"] }),
    );
    assert_eq!(
        u64_at(&empty, "total_count"),
        0,
        "a tag that no trail carries must yield zero results: {empty}"
    );
    let empty_trails = empty
        .get("trails")
        .and_then(|inner| inner.as_array())
        .expect("trails array");
    assert!(
        empty_trails.is_empty(),
        "non-matching tag filter returns no trail summaries"
    );

    // --- trail_resume: known nodes resolve, no bogus id leaks into missing_nodes ---
    let resume = call(
        &mut state,
        "trail_resume",
        json!({ "agent_id": "tester", "trail_id": trail_id }),
    );
    assert_eq!(
        resume.get("label").and_then(|inner| inner.as_str()),
        Some("Investigation A"),
        "resume must restore the saved label"
    );
    let missing = resume
        .get("missing_nodes")
        .and_then(|inner| inner.as_array())
        .expect("missing_nodes array");
    let missing_ids: Vec<&str> = missing.iter().filter_map(|inner| inner.as_str()).collect();
    assert!(
        !missing_ids.contains(&"file::src/main.rs"),
        "the visited file node resolves against the ingested graph: {missing_ids:?}"
    );
    assert!(
        !missing_ids.contains(&"file::src/helper.rs::struct::Helper"),
        "the hypothesis supporting struct resolves against the ingested graph: {missing_ids:?}"
    );
}

#[test]
fn trail_resume_reports_unresolvable_node_as_missing() {
    let temp = tempfile::tempdir().expect("tempdir");
    let project = temp.path().join("proj");
    let mut state = build_state(temp.path());
    ingest_two_file_project(&mut state, &project);

    // Save a throwaway first trail so the trail under test is the second one
    // for this agent -> counter must advance from 001 to 002 (real per-agent
    // counter behavior, not a constant).
    let first = call(
        &mut state,
        "trail_save",
        json!({ "agent_id": "tester", "label": "Investigation A" }),
    );
    assert!(first
        .get("trail_id")
        .and_then(|inner| inner.as_str())
        .expect("first trail_id")
        .starts_with("trail_tester_001_"));

    // Save a trail that visits one real node and one node that does not exist
    // in the ingested graph. The bogus node MUST surface in missing_nodes,
    // proving resolution is real rather than vacuous.
    let bogus = "file::src/ghost.rs::struct::Phantom";
    let save = call(
        &mut state,
        "trail_save",
        json!({
            "agent_id": "tester",
            "label": "Investigation B",
            "tags": ["auth"],
            "visited_nodes": [
                { "node_external_id": "file::src/main.rs", "relevance": 0.7 },
                { "node_external_id": bogus, "relevance": 0.5 }
            ]
        }),
    );
    let trail_id = save
        .get("trail_id")
        .and_then(|inner| inner.as_str())
        .expect("trail_id present")
        .to_string();
    // Second trail saved by the same agent in this state -> counter 002.
    assert!(
        trail_id.starts_with("trail_tester_002_"),
        "second trail for agent 'tester' must be counter 002: got {trail_id}"
    );

    let resume = call(
        &mut state,
        "trail_resume",
        json!({ "agent_id": "tester", "trail_id": trail_id }),
    );
    let missing = resume
        .get("missing_nodes")
        .and_then(|inner| inner.as_array())
        .expect("missing_nodes array");
    let missing_ids: Vec<&str> = missing.iter().filter_map(|inner| inner.as_str()).collect();
    assert!(
        missing_ids.contains(&bogus),
        "the unresolvable node must be reported missing: {missing_ids:?}"
    );
    assert!(
        !missing_ids.contains(&"file::src/main.rs"),
        "the real node must NOT be reported missing: {missing_ids:?}"
    );
}
