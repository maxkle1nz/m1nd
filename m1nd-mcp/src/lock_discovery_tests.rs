//! Discovery contracts for the opt-in graph change-tracking slice.
use super::{all_tool_schemas, core_menu_tool_names, tool_schemas_for_tier, INVESTIGATE_TOOLS};
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn names(tier: &str) -> BTreeSet<String> {
    tool_schemas_for_tier(tier)["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn investigate_is_exactly_core_plus_five_without_changing_default() {
    let core: BTreeSet<String> = core_menu_tool_names()
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(core.len(), 15);
    for tier in ["", "core", "essential", "unknown"] {
        assert_eq!(names(tier), core);
    }
    let mut expected = core;
    expected.extend(INVESTIGATE_TOOLS.iter().map(|name| (*name).to_owned()));
    assert_eq!(expected.len(), 20);
    assert_eq!(names("investigate"), expected);
    assert_eq!(names("INVESTIGATE"), expected);
    for required in crate::tools::HOST_BINDING_REQUIRED_TOOLS {
        assert!(expected.contains(required));
    }
}

#[test]
fn graph_tracking_schemas_are_honest_and_discoverable() {
    let registry = all_tool_schemas();
    let tools = registry["tools"].as_array().unwrap();
    for name in INVESTIGATE_TOOLS {
        let matches: Vec<&Value> = tools.iter().filter(|tool| tool["name"] == *name).collect();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["inputSchema"]["type"], "object");
        assert!(names("full").contains(*name));
        assert!(!names("core").contains(*name));
        let entry = crate::help_guidance::catalog_entry(name).expect("full help entry");
        assert!(!entry.one_liner.contains("POLICY-DISABLED"));
        assert!(crate::action_routes::MCP_TOOL_ROUTE_NAMES.contains(name));
    }
    let create = tools
        .iter()
        .find(|tool| tool["name"] == "lock_create")
        .unwrap();
    assert_eq!(
        create["inputSchema"]["properties"]["scope"]["enum"],
        json!(["node", "subgraph", "path"])
    );
    assert_eq!(
        create["inputSchema"]["properties"]["root_nodes"]["minItems"],
        1
    );
    assert!(create["inputSchema"]["properties"].get("query").is_none());
    assert!(
        create["inputSchema"]["properties"]["radius"]
            .get("default")
            .is_none(),
        "a global default would generate invalid node/path calls"
    );
    let watch = tools
        .iter()
        .find(|tool| tool["name"] == "lock_watch")
        .unwrap();
    assert_eq!(
        watch["inputSchema"]["properties"]["strategy"]["enum"],
        json!(["manual", "on_ingest", "on_learn"])
    );
}

#[test]
fn graph_tracking_help_routes_to_live_ids_before_baselines() {
    let input = serde_json::from_value(
        json!({"agent_id":"investigator", "intent":"track changes in the graph"}),
    )
    .unwrap();
    let guidance = crate::help_guidance::build_route_resolution(&input)
        .guidance
        .unwrap();
    assert_eq!(guidance.minimal_calls[0].tool, "seek");
    assert_eq!(
        guidance.minimal_calls[0].arguments["agent_id"],
        "investigator"
    );
    assert!(guidance
        .recommended_sequence
        .iter()
        .any(|step| step.tool == "lock_diff"));
    assert!(guidance.why.contains("not filesystem locks"));
    assert!(guidance.recovery_steps[0].contains("Help alone does not register"));
}
