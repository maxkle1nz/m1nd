//! Access-control coverage for the `lock_diff` MCP tool's `require_lock` gate.
//!
//! These tests lock the authorization branch in
//! `m1nd-mcp/src/lock_handlers.rs::require_lock`, which every lock-mutating
//! tool funnels through. A lock is created by agent `tester` over a real
//! ingested code node, then `lock_diff` is dispatched with (a) a different
//! `agent_id` than the owner and (b) a `lock_id` that was never created.
//! The first must fail with `LockOwnership` (caller != owner) and the second
//! with `LockNotFound`. Both assert the branch rejects based on the concrete
//! `agent_id` / `lock_id` input values — not merely on response shape — so a
//! regression that drops the ownership check or the existence check would turn
//! these `Err` results into `Ok` and fail the test.
//!
//! X-RAY coverage: proves the lock access-control surface is BEDROCK, not a
//! shape-only facade.

use m1nd_core::domain::DomainConfig;
use m1nd_core::error::M1ndError;
use m1nd_core::graph::Graph;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use serde_json::json;
use std::fs;
use std::path::Path;

/// Build a fresh session rooted at `root` (mirrors the shared harness; each
/// `tests/*.rs` file is its own crate, so the helper is replicated locally).
fn build_state(root: &Path) -> SessionState {
    let config = McpConfig {
        graph_source: root.join("graph_snapshot.json"),
        plasticity_state: root.join("plasticity_state.json"),
        runtime_dir: Some(root.to_path_buf()),
        ..McpConfig::default()
    };
    SessionState::initialize(Graph::new(), &config, DomainConfig::code()).expect("init session")
}

/// Panicking dispatch helper for the happy-path setup calls (ingest / search /
/// lock_create). The error-path assertions below deliberately use the raw
/// `dispatch_tool` instead so they can inspect the `Result`.
fn call(state: &mut SessionState, tool: &str, params: serde_json::Value) -> serde_json::Value {
    dispatch_tool(state, tool, &params).expect("tool call")
}

/// Resolve the first searchable node id for `query`, used as the lock root.
fn search_first_node_id(state: &mut SessionState, query: &str) -> String {
    call(
        state,
        "search",
        json!({"agent_id":"tester","query":query,"mode":"literal"}),
    )
    .get("results")
    .and_then(|value| value.as_array())
    .and_then(|value| value.first())
    .and_then(|value| value.get("node_id"))
    .and_then(|value| value.as_str())
    .unwrap_or("")
    .to_string()
}

/// Ingest a small code file, create a lock as `tester`, and return
/// `(state, lock_id)` so each test starts from a genuinely owned lock.
fn lock_owned_by_tester(root: &Path) -> (SessionState, String) {
    let src_dir = root.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let src_file = src_dir.join("token_validator.rs");
    fs::write(
        &src_file,
        "pub struct TokenValidator;\n\
         impl TokenValidator {\n\
         pub fn validate_request(&self) -> bool { true }\n\
         }\n",
    )
    .unwrap();

    let mut state = build_state(root);
    call(
        &mut state,
        "ingest",
        json!({
            "agent_id": "tester",
            "path": src_file.to_string_lossy().to_string(),
            "adapter": "code",
            "mode": "merge"
        }),
    );

    let node_id = search_first_node_id(&mut state, "TokenValidator");
    assert!(
        !node_id.is_empty(),
        "ingested code symbol must be searchable so the lock has a real root node"
    );

    let created = call(
        &mut state,
        "lock_create",
        json!({"agent_id":"tester","scope":"node","root_nodes":[node_id]}),
    );
    let lock_id = created
        .get("lock_id")
        .and_then(|value| value.as_str())
        .expect("lock_create must return a lock_id")
        .to_string();
    assert!(!lock_id.is_empty(), "created lock id must be non-empty");

    (state, lock_id)
}

#[test]
fn lock_diff_rejects_caller_that_does_not_own_the_lock() {
    let temp = std::env::temp_dir().join("m1nd_lock_diff_ownership_intruder");
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).unwrap();

    let (mut state, lock_id) = lock_owned_by_tester(&temp);

    // Same, existing lock_id — only the agent_id differs from the owner.
    let result = dispatch_tool(
        &mut state,
        "lock_diff",
        &json!({"agent_id":"intruder","lock_id":lock_id}),
    );

    assert!(
        result.is_err(),
        "lock_diff by a non-owner must be rejected, got Ok: {:?}",
        result.ok()
    );
    assert!(
        matches!(result, Err(M1ndError::LockOwnership { .. })),
        "expected LockOwnership for caller != owner, got {:?}",
        result
    );

    let _ = fs::remove_dir_all(&temp);
}

#[test]
fn lock_diff_rejects_unknown_lock_id() {
    let temp = std::env::temp_dir().join("m1nd_lock_diff_ownership_missing");
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).unwrap();

    // The owner agent calls, but for a lock_id that was never created.
    let (mut state, _real_lock_id) = lock_owned_by_tester(&temp);

    let result = dispatch_tool(
        &mut state,
        "lock_diff",
        &json!({"agent_id":"tester","lock_id":"lock-does-not-exist"}),
    );

    assert!(
        result.is_err(),
        "lock_diff for a non-existent lock must be rejected, got Ok: {:?}",
        result.ok()
    );
    assert!(
        matches!(result, Err(M1ndError::LockNotFound { .. })),
        "expected LockNotFound for an unknown lock_id, got {:?}",
        result
    );

    let _ = fs::remove_dir_all(&temp);
}
