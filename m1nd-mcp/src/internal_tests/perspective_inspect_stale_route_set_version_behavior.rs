//! Behavior lock for the optimistic-concurrency contract shared by
//! `perspective_start` and `perspective_inspect` in m1nd-mcp.
//!
//! `perspective_start` mints a `route_set_version` (call it V) and, when the
//! focus node has at least one graph edge, returns a non-empty `routes[]`.
//! `perspective_inspect` then enforces optimistic concurrency: a caller that
//! supplies a `route_set_version` that does NOT match the stored V is rejected
//! with an error (`route_set_stale_error`), and the rejection fires before any
//! route lookup. A caller that supplies the matching V together with
//! `route_index=1` is accepted and the inspected route reflects the route set
//! created by `perspective_start` (its `route_id` equals `routes[0].route_id`).
//!
//! This is the X-RAY coverage that keeps stale clients from acting on a route
//! page the engine has already superseded, while keeping the happy path honest:
//! the inspected route is a function of the freshly created route set, not an
//! arbitrary non-null shape.

use crate as m1nd_mcp;

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeId, NodeType};
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use serde_json::json;
use std::path::Path;

/// Build an isolated session rooted at `root` (replicates the shared harness;
/// `tests/*.rs` are separate crates so helpers cannot be imported across files).
fn build_state(root: &Path) -> SessionState {
    let config = McpConfig {
        graph_source: root.join("graph_snapshot.json"),
        plasticity_state: root.join("plasticity_state.json"),
        runtime_dir: Some(root.to_path_buf()),
        ..McpConfig::default()
    };
    SessionState::initialize(Graph::new(), &config, DomainConfig::code()).expect("init session")
}

/// Dispatch a tool that is expected to succeed.
fn call(state: &mut SessionState, tool: &str, params: serde_json::Value) -> serde_json::Value {
    dispatch_tool(state, tool, &params).expect("tool call")
}

/// Seed the graph with two nodes joined by one directed edge so that
/// `synthesize_routes` can build at least one route from the focus node.
/// Mirrors the add_node + add_edge + finalize pattern used elsewhere, then
/// rebuilds engines so the new graph is live for tool dispatch.
fn seed_graph_with_edge(state: &mut SessionState, focus_external_id: &str) {
    {
        let mut graph = state.graph.write();
        let focus = graph
            .add_node(
                focus_external_id,
                "AlphaNode",
                NodeType::File,
                &["code"],
                1.0,
                0.1,
            )
            .expect("add focus node");
        let neighbor = graph
            .add_node(
                "file::src/beta_node.rs",
                "BetaNode",
                NodeType::File,
                &["code"],
                1.0,
                0.1,
            )
            .expect("add neighbor node");
        graph
            .add_edge(
                focus,
                neighbor,
                "calls",
                FiniteF32::new(0.8),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.5),
            )
            .expect("add edge");
        // Sanity: ids are the first two nodes in an otherwise empty graph.
        assert_eq!(focus, NodeId::new(0));
        assert_eq!(neighbor, NodeId::new(1));
        graph.finalize().expect("finalize graph");
    }
    state.rebuild_engines().expect("rebuild engines");
}

#[test]
fn perspective_inspect_rejects_stale_version_and_accepts_matching_version() {
    let temp = std::env::temp_dir().join("m1nd_perspective_stale_version_behavior");
    let _ = std::fs::remove_dir_all(&temp);
    std::fs::create_dir_all(&temp).expect("create temp dir");

    let focus_external_id = "file::src/alpha_node.rs";
    let mut state = build_state(&temp);
    seed_graph_with_edge(&mut state, focus_external_id);

    // perspective_start: anchor directly on the focus node so route synthesis
    // is deterministic and the single edge yields >= 1 route.
    let start = call(
        &mut state,
        "perspective_start",
        json!({
            "agent_id": "tester",
            "query": "AlphaNode",
            "anchor_node": focus_external_id,
        }),
    );

    let perspective_id = start
        .get("perspective_id")
        .and_then(|value| value.as_str())
        .expect("perspective_id in start output")
        .to_string();

    let version = start
        .get("route_set_version")
        .and_then(|value| value.as_u64())
        .expect("route_set_version in start output");

    let routes = start
        .get("routes")
        .and_then(|value| value.as_array())
        .expect("routes array in start output");

    assert!(
        !routes.is_empty(),
        "a focus node with one outgoing edge must yield at least one route; got total_routes={:?}",
        start.get("total_routes")
    );

    let first_route_id = routes[0]
        .get("route_id")
        .and_then(|value| value.as_str())
        .expect("route_id on first route")
        .to_string();

    // --- Stale path: a deliberately wrong version is rejected. ---
    // `route_set_version` is wall-clock-derived (>> 1), so 1 is guaranteed to
    // mismatch the stored version. The error is a pure function of the version
    // mismatch and fires before any route lookup.
    let stale_version = 1u64;
    assert_ne!(
        stale_version, version,
        "test precondition: stale version must differ from the minted version"
    );

    let stale_result = dispatch_tool(
        &mut state,
        "perspective_inspect",
        &json!({
            "agent_id": "tester",
            "perspective_id": perspective_id,
            "route_index": 1,
            "route_set_version": stale_version,
        }),
    );
    assert!(
        stale_result.is_err(),
        "perspective_inspect with a stale route_set_version must be rejected, got Ok: {:?}",
        stale_result.ok()
    );
    let stale_error = stale_result.unwrap_err().to_string();
    assert!(
        stale_error.contains("stale") && stale_error.contains("route_set_version"),
        "stale rejection must explain the route_set_version staleness; got: {}",
        stale_error
    );
    assert!(
        stale_error.contains(&stale_version.to_string()),
        "stale error must echo the rejected version {}; got: {}",
        stale_version,
        stale_error
    );

    // --- Happy path: the matching version + route_index=1 is accepted and the
    // inspected route reflects the route set produced by perspective_start. ---
    let ok = call(
        &mut state,
        "perspective_inspect",
        json!({
            "agent_id": "tester",
            "perspective_id": perspective_id,
            "route_index": 1,
            "route_set_version": version,
        }),
    );

    let inspected_route_id = ok
        .get("route_id")
        .and_then(|value| value.as_str())
        .expect("route_id in inspect output");
    assert_eq!(
        inspected_route_id, first_route_id,
        "inspecting route_index=1 with the matching version must return the first \
         route minted by perspective_start"
    );
    assert_eq!(
        ok.get("route_index").and_then(|value| value.as_u64()),
        Some(1),
        "inspected route_index must be the 1-based index that was requested"
    );

    let _ = std::fs::remove_dir_all(&temp);
}
