//! Behavioral coverage for the `type_trace` MCP tool (dispatched in
//! `server.rs`, handled by `handle_type_trace` in `layer_handlers.rs`).
//!
//! Locks the tiered type resolution and cross-file usage tracing: a known
//! two-file Rust project (`Helper` defined in `helper.rs`, used in `main.rs`)
//! is ingested via the real `ingest` tool, then traced. The asserts pin the
//! resolved struct identity and the use-site that ties back to `main.rs`,
//! plus the handler's `None` branch for an unknown target.
//!
//! This is X-RAY coverage for the type_trace surface: it fails if resolution
//! stops picking the struct node, if cross-file BFS loses the main.rs
//! use-site, or if the empty-target contract regresses.

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

/// Write the known two-file fixture project and ingest it with the code
/// adapter so the graph holds a real `Helper` struct plus its use-site.
fn ingest_helper_fixture(state: &mut SessionState, proj: &Path) {
    fs::create_dir_all(proj).unwrap();
    fs::write(
        proj.join("helper.rs"),
        "pub struct Helper {\n    pub value: i32,\n}\n\nimpl Helper {\n    pub fn new() -> Helper {\n        Helper { value: 0 }\n    }\n}\n",
    )
    .unwrap();
    fs::write(
        proj.join("main.rs"),
        "mod helper;\nuse helper::Helper;\n\nfn run() -> Helper {\n    let h: Helper = Helper::new();\n    h\n}\n\nfn main() {\n    let _ = run();\n}\n",
    )
    .unwrap();

    call(
        state,
        "ingest",
        json!({
            "agent_id": "tester",
            "path": proj.to_string_lossy().to_string(),
            "adapter": "code",
            "mode": "replace",
        }),
    );
}

#[test]
fn type_trace_resolves_helper_struct_and_ties_usage_to_main() {
    let temp = tempfile::tempdir().unwrap();
    let proj = temp.path().join("proj");
    let mut state = build_state(temp.path());
    ingest_helper_fixture(&mut state, &proj);

    let out = call(
        &mut state,
        "type_trace",
        json!({
            "agent_id": "tester",
            "target": "Helper",
            "direction": "both",
            "max_hops": 4,
        }),
    );

    // Tiered resolution must land on the real struct node, not a function /
    // module / file that merely mentions "Helper".
    assert_eq!(
        out["target_label"].as_str(),
        Some("Helper"),
        "tiered resolution should resolve the Helper struct label, got {:?}",
        out["target_label"]
    );
    assert_eq!(
        out["target_type"].as_str(),
        Some("struct"),
        "resolved node must be the struct definition, got {:?}",
        out["target_type"]
    );

    // BFS over the known fixture must surface at least one usage.
    let total_usages = out["total_usages"].as_u64().unwrap_or(0);
    assert!(
        total_usages >= 1,
        "tracing Helper across two files should find at least one usage, got {}",
        total_usages
    );

    // At least one usage entry must tie back to main.rs — the file that
    // actually uses Helper (import + return type + construction).
    let usages = out["usages"].as_array().expect("usages array");
    let main_use_site = usages.iter().any(|usage| {
        let file_ties = usage
            .get("file_path")
            .and_then(|value| value.as_str())
            .map(|path| path.ends_with("main.rs"))
            .unwrap_or(false);
        let node_ties = usage
            .get("node_id")
            .and_then(|value| value.as_str())
            .map(|node_id| node_id.contains("main.rs"))
            .unwrap_or(false);
        file_ties || node_ties
    });
    assert!(
        main_use_site,
        "expected a usage entry whose file_path/node_id ties back to main.rs, got {:?}",
        usages
    );
}

#[test]
fn type_trace_unknown_target_returns_empty_none_branch() {
    let temp = tempfile::tempdir().unwrap();
    let proj = temp.path().join("proj");
    let mut state = build_state(temp.path());
    // Populate the graph so the empty result reflects "target not found",
    // not "graph empty" (the handler errors with EmptyGraph on an empty graph).
    ingest_helper_fixture(&mut state, &proj);

    let out = call(
        &mut state,
        "type_trace",
        json!({
            "agent_id": "tester",
            "target": "NoSuchType",
        }),
    );

    // Handler's None branch: empty labels and zero usages.
    assert_eq!(
        out["target_label"].as_str(),
        Some(""),
        "unknown target must yield an empty target_label, got {:?}",
        out["target_label"]
    );
    assert_eq!(
        out["target_type"].as_str(),
        Some(""),
        "unknown target must yield an empty target_type, got {:?}",
        out["target_type"]
    );
    assert_eq!(
        out["total_usages"].as_u64(),
        Some(0),
        "unknown target must report zero usages, got {:?}",
        out["total_usages"]
    );
    // Echoes the requested target back even on the miss path.
    assert_eq!(
        out["target"].as_str(),
        Some("NoSuchType"),
        "miss path should echo the requested target, got {:?}",
        out["target"]
    );
}
