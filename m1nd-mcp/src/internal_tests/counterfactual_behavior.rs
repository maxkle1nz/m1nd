//! Behavioral coverage for the `counterfactual` MCP tool (X-RAY safety net).
//!
//! These tests lock the two branches of `handle_counterfactual`
//! (`m1nd-mcp/src/tools.rs`): the early-return error path taken when no input
//! id resolves, and the success path taken when an id resolves to a real graph
//! node. The branch is selected purely by whether the input id resolves, so the
//! assertions check input-derived facts — the echoed `node_ids`/`removed_nodes`,
//! and that removing a file node surfaces a non-empty downstream cascade over its
//! outgoing `contains` children (struct/impl/method) — rather than mere shape.
//!
//! Each `tests/*.rs` file is its own crate, so the `build_state`/`call` harness
//! helpers are replicated here on purpose instead of imported from another test
//! file.

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

/// Create a unique temp dir for one test and return it.
fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("m1nd_cf_{tag}_{nanos}_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

/// Ingest a two-file Rust crate where `main.rs` imports a `Helper` struct from
/// `helper.rs`, mirroring the `why`-style fixture. Returns the source root so it
/// can be cleaned up by the caller.
fn ingest_helper_crate(state: &mut SessionState) -> PathBuf {
    let code_root = unique_dir("src");
    write(
        &code_root.join("Cargo.toml"),
        "[package]\nname = \"cfprobe\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    );
    write(
        &code_root.join("src/helper.rs"),
        "pub struct Helper {\n    pub value: i32,\n}\n\nimpl Helper {\n    pub fn new() -> Self {\n        Helper { value: 0 }\n    }\n}\n",
    );
    write(
        &code_root.join("src/main.rs"),
        "mod helper;\nuse helper::Helper;\n\nfn main() {\n    let h = Helper::new();\n    println!(\"{}\", h.value);\n}\n",
    );

    call(
        state,
        "ingest",
        json!({
            "agent_id": "tester",
            "path": code_root.to_string_lossy().to_string(),
            "adapter": "code",
            "mode": "replace",
        }),
    );
    code_root
}

/// node_ids that resolve to nothing must take the early-return error path:
/// `error == "No valid node IDs found"` and the input ids are echoed back. This
/// branch is selected purely because the id fails to resolve in the graph.
#[test]
fn counterfactual_unresolved_ids_return_error_and_echo_input() {
    let runtime = unique_dir("err_rt");
    let mut state = build_state(&runtime);
    // Populate the graph so this is the resolve-failure path, not an empty-graph
    // accident — the ghost id below still must not resolve.
    let code_root = ingest_helper_crate(&mut state);

    let ghost = "file::definitely-not-a-real-node-ghost.rs";
    let output = call(
        &mut state,
        "counterfactual",
        json!({ "agent_id": "tester", "node_ids": [ghost] }),
    );

    assert_eq!(
        output.get("error").and_then(|value| value.as_str()),
        Some("No valid node IDs found"),
        "unresolved ids must hit the early-return error branch, got: {output}"
    );
    let echoed: Vec<String> = output
        .get("node_ids")
        .and_then(|value| value.as_array())
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        echoed,
        vec![ghost.to_string()],
        "error path must echo back the exact input node_ids"
    );

    fs::remove_dir_all(&runtime).ok();
    fs::remove_dir_all(&code_root).ok();
}

/// Removing the `helper.rs` file that `main.rs` depends on must take the success
/// path: `total_impact` / `pct_activation_lost` are present, `removed_nodes`
/// echoes the input, and the cascade follows the file node's outgoing
/// `contains` children (the `Helper` struct/impl/method), so
/// `cascade.total_affected >= 1`.
#[test]
fn counterfactual_removing_depended_on_node_reports_downstream_cascade() {
    let runtime = unique_dir("ok_rt");
    let mut state = build_state(&runtime);
    let code_root = ingest_helper_crate(&mut state);

    // The file-level id format is stable (`file::<relpath>`); confirm it
    // resolved by checking search returned it before driving counterfactual.
    let target = "file::src/helper.rs";
    let search = call(
        &mut state,
        "search",
        json!({ "agent_id": "tester", "query": "Helper", "mode": "literal" }),
    );
    let resolved = search
        .get("results")
        .and_then(|value| value.as_array())
        .map(|results| {
            results
                .iter()
                .any(|entry| entry.get("node_id").and_then(|value| value.as_str()) == Some(target))
        })
        .unwrap_or(false);
    assert!(
        resolved,
        "expected ingested graph to contain {target}, search returned: {search}"
    );

    let output = call(
        &mut state,
        "counterfactual",
        json!({
            "agent_id": "tester",
            "node_ids": [target],
            "include_cascade": true,
        }),
    );

    assert!(
        output.get("error").is_none(),
        "resolved id must take the success branch, got: {output}"
    );

    let removed: Vec<String> = output
        .get("removed_nodes")
        .and_then(|value| value.as_array())
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        removed,
        vec![target.to_string()],
        "success path must echo the removed node back in removed_nodes"
    );

    assert!(
        output
            .get("total_impact")
            .and_then(|value| value.as_f64())
            .is_some(),
        "success path must report total_impact, got: {output}"
    );
    assert!(
        output
            .get("pct_activation_lost")
            .and_then(|value| value.as_f64())
            .is_some(),
        "success path must report pct_activation_lost, got: {output}"
    );

    let total_affected = output
        .get("cascade")
        .and_then(|value| value.get("total_affected"))
        .and_then(|value| value.as_u64())
        .unwrap_or(0);
    assert!(
        total_affected >= 1,
        "removing helper.rs must surface a downstream cascade over its contained \
         symbols, got total_affected={total_affected} in: {output}"
    );

    fs::remove_dir_all(&runtime).ok();
    fs::remove_dir_all(&code_root).ok();
}
