//! Behavioral CRUD lifecycle for the perspective management tools
//! (`perspective_start` -> `perspective_list` -> `perspective_close`) driven
//! through the real `dispatch_tool` harness against a populated graph.
//!
//! These assertions are value-bound, not vanity: the perspective created from a
//! known ingested node must become observable in `perspective_list` (same id,
//! same resolved focus, with the seeded `start` navigation event counted), and
//! must disappear after `perspective_close`. The contract `proof_state`
//! reacts to whether the agent has zero perspectives. This locks X-RAY's
//! stateful navigation surface: create/list/close must reflect the create/close
//! inputs rather than merely returning well-shaped JSON.

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

/// Minimal L1GHT/1 document declaring a single entity, mirroring the
/// `light_doc` helper used by `tests/test_auto_ingest.rs`.
fn light_doc(entity: &str, doi: &str) -> String {
    format!(
        r#"---
Protocol: L1GHT/1
Node: {entity}
State: active
Color: amber
Glyph: *
Completeness: draft
Proof: working
Depends on:
- {doi}
Next:
- validate
---
## Contract
[⍂ entity: {entity}]
[⟁ depends_on: {doi}]
[𝔻 evidence: ready]
"#
    )
}

fn first_search_node_id(state: &mut SessionState, query: &str) -> String {
    call(
        state,
        "search",
        json!({"agent_id": "tester", "query": query, "mode": "literal"}),
    )
    .get("results")
    .and_then(|value| value.as_array())
    .and_then(|results| results.first())
    .and_then(|entry| entry.get("node_id"))
    .and_then(|value| value.as_str())
    .unwrap_or_default()
    .to_string()
}

#[test]
fn perspective_start_list_close_lifecycle_reflects_inputs() {
    let temp = std::env::temp_dir().join(format!(
        "m1nd_perspective_lifecycle_{}_{}",
        std::process::id(),
        now_nanos()
    ));
    let docs_root = temp.join("docs");
    fs::create_dir_all(&docs_root).expect("create docs root");
    let file = docs_root.join("alpha.light.md");
    fs::write(&file, light_doc("AlphaNode", "10.1000/shared")).expect("write light doc");

    let mut state = build_state(&temp);

    // Populate the graph: the light entity must be ingested and searchable
    // before any read tool can resolve a focus node.
    call(
        &mut state,
        "ingest",
        json!({
            "agent_id": "tester",
            "path": file.to_string_lossy().to_string(),
            "adapter": "light",
            "mode": "merge"
        }),
    );

    // The ingested entity is materialized as an external node id whose tail is
    // the case-folded label. We bind every later assertion to this concrete id.
    let ingested_node_id = first_search_node_id(&mut state, "AlphaNode");
    assert!(
        ingested_node_id.contains("alphanode"),
        "ingest+search should yield the AlphaNode external id, got `{}`",
        ingested_node_id
    );

    // ---- perspective_start: focus must resolve to the ingested node --------
    // The query is matched (substring, case-sensitive) against external ids,
    // so we seed with the case-folded label the ingester actually wrote.
    let start = call(
        &mut state,
        "perspective_start",
        json!({"agent_id": "tester", "query": "alphanode"}),
    );

    let perspective_id = start
        .get("perspective_id")
        .and_then(|value| value.as_str())
        .expect("perspective_id present")
        .to_string();
    assert!(
        !perspective_id.is_empty(),
        "perspective_start must return a non-empty perspective_id"
    );
    assert_eq!(
        start.get("focus_node").and_then(|value| value.as_str()),
        Some(ingested_node_id.as_str()),
        "focus_node must resolve to the ingested AlphaNode external id, not null"
    );

    // ---- perspective_list: the created perspective is observable -----------
    let list = call(
        &mut state,
        "perspective_list",
        json!({"agent_id": "tester"}),
    );
    let perspectives = list
        .get("perspectives")
        .and_then(|value| value.as_array())
        .expect("perspectives array present");

    let listed = perspectives
        .iter()
        .filter(|entry| {
            entry.get("perspective_id").and_then(|value| value.as_str())
                == Some(perspective_id.as_str())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        listed.len(),
        1,
        "perspective_list must contain EXACTLY the created perspective_id `{}`, got {:?}",
        perspective_id,
        perspectives
    );

    let summary = listed[0];
    assert_eq!(
        summary.get("focus_node").and_then(|value| value.as_str()),
        Some(ingested_node_id.as_str()),
        "listed perspective must carry the same resolved focus as start"
    );
    assert!(
        summary
            .get("nav_event_count")
            .and_then(|value| value.as_u64())
            .unwrap_or(0)
            >= 1,
        "the seeded `start` navigation event must be counted (nav_event_count >= 1)"
    );

    // ---- perspective_close: closed==true ----------------------------------
    let close = call(
        &mut state,
        "perspective_close",
        json!({"agent_id": "tester", "perspective_id": perspective_id.clone()}),
    );
    assert_eq!(
        close.get("closed").and_then(|value| value.as_bool()),
        Some(true),
        "perspective_close must report closed==true for the existing perspective"
    );

    // ---- perspective_list after close: agent has zero perspectives ---------
    let list_after = call(
        &mut state,
        "perspective_list",
        json!({"agent_id": "tester"}),
    );
    let remaining = list_after
        .get("perspectives")
        .and_then(|value| value.as_array())
        .expect("perspectives array present after close");
    assert!(
        remaining.is_empty(),
        "perspective_list must be EMPTY after closing the only perspective, got {:?}",
        remaining
    );
    assert_eq!(
        list_after
            .get("proof_state")
            .and_then(|value| value.as_str()),
        Some("blocked"),
        "with zero perspectives the contract must surface proof_state==blocked"
    );

    // ---- contract reacts to a genuinely unknown agent ---------------------
    let unknown = call(
        &mut state,
        "perspective_list",
        json!({"agent_id": "agent-with-no-perspectives"}),
    );
    assert!(
        unknown
            .get("perspectives")
            .and_then(|value| value.as_array())
            .map(|value| value.is_empty())
            .unwrap_or(false),
        "unknown agent_id must yield an empty perspectives array"
    );
    assert_eq!(
        unknown.get("proof_state").and_then(|value| value.as_str()),
        Some("blocked"),
        "unknown agent_id (zero perspectives) must surface proof_state==blocked"
    );

    let _ = fs::remove_dir_all(&temp);
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}
