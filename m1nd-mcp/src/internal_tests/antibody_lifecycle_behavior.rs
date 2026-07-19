//! Behavioral coverage for the `antibody_create` / `antibody_list` / `antibody_scan`
//! tools driven through the real `dispatch_tool` MCP surface.
//!
//! These tools form m1nd's immune-memory subsystem: an agent registers a structural
//! bug pattern (antibody), lists the registry, and scans the graph for recurrences.
//! Before this file the entire antibody tool trio had zero integration coverage in
//! `m1nd-mcp` — nothing pinned the response shapes nor the mutation accounting the
//! handlers do against `SessionState::antibodies`.
//!
//! Every assertion here tracks the exact prior mutation: the id returned by `create`
//! is the id that shows up in `list`; `disable` flips the enabled/disabled counts by
//! exactly one; `include_disabled=false` hides the just-disabled antibody; `delete`
//! drops the total to zero; and enable/disable/delete on an unknown id make
//! `dispatch_tool` return `Err`. The specificity (2 of 3 node constraints = ~0.67)
//! and `initial_matches` against a known seeded `validate_request` function node are
//! asserted against the inputs we control — not vanity not-null checks.
//!
//! This locks the antibody lifecycle contract so a refactor of the handlers or of the
//! `Antibody` serialization (the `id`/`enabled` fields surfaced in `list`) cannot
//! silently change agent-visible behavior. (X-RAY coverage: previously UNPROVABLE.)

use crate as m1nd_mcp;

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_core::types::NodeType;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use serde_json::json;
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

/// Seed one finalized `Function` node whose label contains "validate" so the example
/// antibody pattern (anchor: function, label_contains "validate") matches exactly once.
fn seed_validate_function(state: &mut SessionState) {
    {
        let mut graph = state.graph.write();
        graph
            .add_node(
                "func::validate_request",
                "validate_request",
                NodeType::Function,
                &["code"],
                1.0,
                0.1,
            )
            .unwrap();
        // A second, non-matching function node ensures the match count reflects the
        // label_contains constraint rather than "every function".
        graph
            .add_node(
                "func::render_page",
                "render_page",
                NodeType::Function,
                &["code"],
                1.0,
                0.1,
            )
            .unwrap();
        graph.finalize().unwrap();
    }
    state.rebuild_engines().unwrap();
}

/// The example pattern from the tool docs: a single anchor function node constrained by
/// type + label substring. specificity = 2 constraints / (1 node * 3) = ~0.667.
fn example_create_params() -> serde_json::Value {
    json!({
        "agent_id": "tester",
        "action": "create",
        "name": "AB1",
        "severity": "warning",
        "pattern": {
            "nodes": [
                {
                    "role": "anchor",
                    "node_type": "function",
                    "label_contains": "validate"
                }
            ],
            "edges": []
        }
    })
}

#[test]
fn antibody_create_then_list_then_disable_then_delete_tracks_each_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = build_state(temp.path());
    seed_validate_function(&mut state);

    // --- create -------------------------------------------------------------
    let created = call(&mut state, "antibody_create", example_create_params());

    let antibody_id = created
        .get("antibody_id")
        .and_then(|value| value.as_str())
        .expect("create returns antibody_id")
        .to_string();
    assert!(
        antibody_id.starts_with("ab-"),
        "antibody id should use the ab- prefix, got {antibody_id}"
    );

    let specificity = created
        .get("specificity")
        .and_then(|value| value.as_f64())
        .expect("create returns specificity");
    assert!(
        (specificity - 0.666_667).abs() < 0.01,
        "2 of 3 node constraints should yield specificity ~0.67, got {specificity}"
    );

    let initial_matches = created
        .get("initial_matches")
        .and_then(|value| value.as_u64())
        .expect("create returns initial_matches");
    assert_eq!(
        initial_matches, 1,
        "exactly one seeded function label contains 'validate'"
    );

    // --- list after create --------------------------------------------------
    let listed = call(
        &mut state,
        "antibody_list",
        json!({ "agent_id": "tester", "include_disabled": true }),
    );
    assert_eq!(
        listed.get("total").and_then(|value| value.as_u64()),
        Some(1),
        "exactly one antibody exists after a single create"
    );
    assert_eq!(
        listed.get("enabled").and_then(|value| value.as_u64()),
        Some(1),
        "the freshly created antibody is enabled"
    );
    assert_eq!(
        listed.get("disabled").and_then(|value| value.as_u64()),
        Some(0),
        "nothing is disabled yet"
    );
    let listed_id = listed
        .get("antibodies")
        .and_then(|value| value.as_array())
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get("id"))
        .and_then(|value| value.as_str())
        .expect("listed antibody exposes its id");
    assert_eq!(
        listed_id, antibody_id,
        "the listed antibody is the one create returned"
    );

    // --- disable ------------------------------------------------------------
    let disabled = call(
        &mut state,
        "antibody_create",
        json!({
            "agent_id": "tester",
            "action": "disable",
            "antibody_id": antibody_id
        }),
    );
    assert_eq!(
        disabled.get("success").and_then(|value| value.as_bool()),
        Some(true),
        "disable on an existing id succeeds"
    );

    // include_disabled=true still sees it, now counted as disabled.
    let listed_all = call(
        &mut state,
        "antibody_list",
        json!({ "agent_id": "tester", "include_disabled": true }),
    );
    assert_eq!(
        listed_all.get("enabled").and_then(|value| value.as_u64()),
        Some(0),
        "after disable nothing is enabled"
    );
    assert_eq!(
        listed_all.get("disabled").and_then(|value| value.as_u64()),
        Some(1),
        "after disable exactly one is disabled"
    );

    // include_disabled=false hides the disabled antibody entirely.
    let listed_enabled_only = call(
        &mut state,
        "antibody_list",
        json!({ "agent_id": "tester", "include_disabled": false }),
    );
    let visible = listed_enabled_only
        .get("antibodies")
        .and_then(|value| value.as_array())
        .expect("antibodies array present");
    assert!(
        visible.is_empty(),
        "include_disabled=false hides the disabled antibody, got {visible:?}"
    );

    // --- delete -------------------------------------------------------------
    let deleted = call(
        &mut state,
        "antibody_create",
        json!({
            "agent_id": "tester",
            "action": "delete",
            "antibody_id": antibody_id
        }),
    );
    assert_eq!(
        deleted.get("success").and_then(|value| value.as_bool()),
        Some(true),
        "delete on an existing id succeeds"
    );

    let listed_after_delete = call(
        &mut state,
        "antibody_list",
        json!({ "agent_id": "tester", "include_disabled": true }),
    );
    assert_eq!(
        listed_after_delete
            .get("total")
            .and_then(|value| value.as_u64()),
        Some(0),
        "registry is empty after delete"
    );
}

#[test]
fn antibody_mutations_on_unknown_id_return_err() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = build_state(temp.path());
    seed_validate_function(&mut state);

    // No .expect here: we assert dispatch_tool itself errors (AntibodyNotFound).
    for action in ["enable", "disable", "delete"] {
        let result = dispatch_tool(
            &mut state,
            "antibody_create",
            &json!({
                "agent_id": "tester",
                "action": action,
                "antibody_id": "does-not-exist"
            }),
        );
        assert!(
            result.is_err(),
            "{action} on a non-existent antibody id should return Err, got {result:?}"
        );
    }
}

#[test]
fn antibody_scan_finds_the_created_pattern_in_the_seeded_graph() {
    let temp = tempfile::tempdir().unwrap();
    let mut state = build_state(temp.path());
    seed_validate_function(&mut state);

    call(&mut state, "antibody_create", example_create_params());

    let scan = call(&mut state, "antibody_scan", json!({ "agent_id": "tester" }));

    assert_eq!(
        scan.get("antibodies_checked")
            .and_then(|value| value.as_u64()),
        Some(1),
        "the one enabled antibody is checked during scan"
    );

    let matches = scan
        .get("matches")
        .and_then(|value| value.as_array())
        .expect("scan returns a matches array");
    assert_eq!(
        matches.len(),
        1,
        "scan finds exactly the seeded validate_request function, got {matches:?}"
    );
}
