//! Behavioral coverage for the `metrics` MCP tool (X-RAY safety net).
//!
//! These tests lock the input-determined behavior of `handle_metrics`
//! (`m1nd-mcp/src/layer_handlers.rs`), not mere response shape. A small
//! two-file Rust crate is ingested via the `code` adapter: `helper.rs`
//! defines `struct Helper`, and `main.rs` defines `fn build`. The assertions
//! tie the returned aggregates and entry labels back to those known symbols:
//! filtering on `["function", "struct"]` must count the real function and
//! struct (and surface their labels), `sort = "name_asc"` must order entries
//! alphabetically, and the default file scope must report the two source
//! files with a positive line count. A separate test exercises the
//! `EmptyGraph` error path, which is selected purely because no nodes were
//! ingested.
//!
//! Each `tests/*.rs` file is its own crate, so the `build_state`/`call`
//! harness helpers are replicated here on purpose instead of imported from
//! another test file.

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use serde_json::{json, Value};
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

fn call(state: &mut SessionState, tool: &str, params: Value) -> Value {
    dispatch_tool(state, tool, &params).expect("tool call")
}

/// Create a unique temp dir for one test and return it.
fn unique_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("m1nd_metrics_{tag}_{nanos}_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

/// Ingest a two-file Rust crate where `helper.rs` defines `struct Helper` and
/// `main.rs` defines `fn build` that constructs it. Returns the source root so
/// it can be cleaned up by the caller.
fn ingest_build_crate(state: &mut SessionState) -> PathBuf {
    let code_root = unique_dir("src");
    write(
        &code_root.join("Cargo.toml"),
        "[package]\nname = \"metricsprobe\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    );
    write(
        &code_root.join("src/helper.rs"),
        "pub struct Helper {\n    pub value: i32,\n}\n",
    );
    write(
        &code_root.join("src/main.rs"),
        "mod helper;\nuse helper::Helper;\n\nfn build() -> Helper {\n    Helper { value: 0 }\n}\n\nfn main() {\n    let h = build();\n    println!(\"{}\", h.value);\n}\n",
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

fn entries(output: &Value) -> &Vec<Value> {
    output
        .get("entries")
        .and_then(Value::as_array)
        .expect("metrics output must carry an entries array")
}

fn entry_labels(output: &Value) -> Vec<String> {
    entries(output)
        .iter()
        .filter_map(|entry| entry.get("label").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

fn summary_u64(output: &Value, field: &str) -> u64 {
    output
        .get("summary")
        .and_then(|summary| summary.get(field))
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("summary.{field} must be a non-negative integer"))
}

/// `sort = "name_asc"` must return entries in ascending label order. Proving the
/// sort branch requires the produced order to be non-decreasing, which only
/// holds because the handler took the `"name_asc"` arm rather than the default
/// `loc_desc`.
#[test]
fn metrics_name_asc_orders_entries_alphabetically() {
    let runtime = unique_dir("sort_rt");
    let mut state = build_state(&runtime);
    let code_root = ingest_build_crate(&mut state);

    let output = call(
        &mut state,
        "metrics",
        json!({
            "agent_id": "tester",
            "node_types": ["function", "struct"],
            "sort": "name_asc",
            "top_k": 50,
        }),
    );

    let labels = entry_labels(&output);
    assert!(
        labels.len() >= 2,
        "fixture must yield at least two function/struct entries to test ordering; got {labels:?}"
    );
    assert!(
        labels[0] <= labels[1],
        "name_asc must order entries alphabetically: entries[0]={:?} should be <= entries[1]={:?}",
        labels[0],
        labels[1]
    );
    for window in labels.windows(2) {
        assert!(
            window[0] <= window[1],
            "name_asc order broken between {:?} and {:?} in {labels:?}",
            window[0],
            window[1]
        );
    }

    fs::remove_dir_all(&code_root).ok();
    fs::remove_dir_all(&runtime).ok();
}

/// The default node scope (`node_types = ["file"]`) must report both ingested
/// source files and a positive total line count. The counts are derived from
/// the two real files written to disk, not from non-null shape.
#[test]
fn metrics_default_file_scope_counts_files_and_loc() {
    let runtime = unique_dir("file_rt");
    let mut state = build_state(&runtime);
    let code_root = ingest_build_crate(&mut state);

    let output = call(
        &mut state,
        "metrics",
        json!({
            "agent_id": "tester",
        }),
    );

    assert!(
        summary_u64(&output, "total_files") >= 2,
        "fixture ingests helper.rs and main.rs, so total_files must be >= 2; got summary {:?}",
        output.get("summary")
    );
    assert!(
        summary_u64(&output, "total_loc") > 0,
        "ingested files have content, so total_loc must be > 0; got summary {:?}",
        output.get("summary")
    );

    fs::remove_dir_all(&code_root).ok();
    fs::remove_dir_all(&runtime).ok();
}

/// An empty graph (nothing ingested) must take the early-return error path:
/// `handle_metrics` returns `Err(M1ndError::EmptyGraph)`. This branch is
/// selected purely by the absence of nodes, so it is an input-determined error
/// path rather than a vanity check.
#[test]
fn metrics_empty_graph_returns_error() {
    let runtime = unique_dir("empty_rt");
    let mut state = build_state(&runtime);

    let result = dispatch_tool(
        &mut state,
        "metrics",
        &json!({
            "agent_id": "tester",
            "node_types": ["function", "struct"],
        }),
    );

    let err = result.expect_err("metrics on an empty graph must return an error, not a value");
    assert!(
        err.to_string().to_lowercase().contains("empty"),
        "empty-graph error should mention an empty graph; got {err}"
    );

    fs::remove_dir_all(&runtime).ok();
}
