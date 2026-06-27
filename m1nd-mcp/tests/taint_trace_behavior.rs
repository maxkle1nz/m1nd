//! X-RAY coverage for the `taint_trace` MCP tool dispatched through the real
//! `dispatch_tool` harness.
//!
//! These tests lock the input-determined control-flow fork inside
//! `handle_taint_trace`: when NONE of the supplied `entry_nodes` resolve against
//! the ingested graph, the handler short-circuits with
//! `M1ndError::InvalidParams { tool: "taint_trace", .. }` and the `detail`
//! string echoes the unresolved ids it was given. When a genuinely-present
//! ingested file (`file::src/main.rs`) is passed, the same call succeeds and
//! returns a `summary` object plus a finite numeric `risk_score`.
//!
//! The load-bearing behavior is the resolve / no-resolve fork plus the echoed
//! bad id in the error detail — that is driven by the known input, not by
//! response shape. The success-path numeric range on `risk_score` is the weaker
//! medium-confidence check.

use m1nd_core::domain::DomainConfig;
use m1nd_core::error::M1ndError;
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

/// Create a unique scratch directory under the system temp dir for one test.
fn unique_scratch(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("m1nd_taint_trace_{tag}_{nanos}"));
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Write a tiny code tree and ingest it via the `code` adapter so the graph
/// contains a resolvable `file::src/main.rs` node.
fn ingest_fixture(state: &mut SessionState, root: &Path) {
    let src_dir = root.join("src");
    fs::create_dir_all(&src_dir).expect("create src dir");
    fs::write(
        src_dir.join("main.rs"),
        "fn main() {\n    let payload = read_request();\n    handle(payload);\n}\n\nfn read_request() -> String {\n    String::from(\"data\")\n}\n\nfn handle(input: String) {\n    let _ = input;\n}\n",
    )
    .expect("write main.rs");

    let ingest = call(
        state,
        "ingest",
        json!({
            "agent_id": "tester",
            "path": root.to_string_lossy().to_string(),
            "adapter": "code",
            "mode": "replace"
        }),
    );
    assert_eq!(
        ingest.get("adapter").and_then(|value| value.as_str()),
        Some("code"),
        "ingest fixture should run the code adapter"
    );
}

/// The no-resolve branch is genuine input-determined control flow: a phantom
/// entry id that exists in no graph forces the `InvalidParams` error path, and
/// the error `detail` must echo the exact id that failed to resolve.
#[test]
fn taint_trace_errors_when_no_entry_node_resolves() {
    let root = unique_scratch("noresolve");
    let mut state = build_state(&root);
    ingest_fixture(&mut state, &root);

    let bad_id = "file::ghost_does_not_exist.rs";
    let params = json!({
        "agent_id": "tester",
        "entry_nodes": [bad_id],
        "taint_type": "user_input",
        "max_depth": 4
    });

    let result = dispatch_tool(&mut state, "taint_trace", &params);
    let err = result.expect_err("unresolvable entry nodes must produce an error, not a value");

    match err {
        M1ndError::InvalidParams { tool, detail } => {
            assert_eq!(
                tool, "taint_trace",
                "InvalidParams should attribute the failure to the taint_trace tool"
            );
            assert!(
                detail.contains(bad_id),
                "error detail must echo the unresolved entry id; got: {detail}"
            );
        }
        other => panic!("expected M1ndError::InvalidParams, got: {other:?}"),
    }

    fs::remove_dir_all(&root).ok();
}

/// The resolve branch is the other fork of the same input-determined control
/// flow: the real ingested `file::src/main.rs` resolves, so the call must NOT
/// error and must return a `summary` object. The finite numeric `risk_score`
/// in [0, 1] is the weaker success-path check.
#[test]
fn taint_trace_succeeds_for_real_ingested_entry_node() {
    let root = unique_scratch("resolve");
    let mut state = build_state(&root);
    ingest_fixture(&mut state, &root);

    let params = json!({
        "agent_id": "tester",
        "entry_nodes": ["file::src/main.rs"],
        "taint_type": "user_input",
        "max_depth": 4
    });

    // Load-bearing: the resolvable entry must NOT take the error fork.
    let value = dispatch_tool(&mut state, "taint_trace", &params)
        .expect("resolvable entry node must not error");

    assert!(
        value.get("summary").is_some(),
        "successful taint_trace must return a summary object; got: {value}"
    );

    let risk_score = value
        .get("risk_score")
        .and_then(|score| score.as_f64())
        .expect("successful taint_trace must return a numeric risk_score");
    assert!(
        risk_score.is_finite(),
        "risk_score must be a finite f64; got: {risk_score}"
    );
    assert!(
        (0.0..=1.0).contains(&risk_score),
        "risk_score must fall within [0, 1]; got: {risk_score}"
    );

    fs::remove_dir_all(&root).ok();
}
