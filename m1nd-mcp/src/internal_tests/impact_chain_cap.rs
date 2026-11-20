//! `impact` keeps its causal chains inside an agent-sized answer.
//!
//! The chain detector explores up to its operation budget and, on a real
//! repository, returns thousands of chains: measured on an 18k-node brain,
//! `impact` answered 3.6-3.8 MB, two thirds of it `causal_chains`, while the
//! blast radius it exists for was 28 KB. These tests pin the ceiling:
//!
//! * by default only the strongest few chains come back, with the total count
//!   and an explicit truncation marker, and the whole answer stays small;
//! * the returned chains are exactly the first chains of the uncapped list, in
//!   the same order;
//! * an opt-in limit restores the full list unchanged;
//! * the blast radius, its order, scores and energy do not move.

use crate as m1nd_mcp;

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};
use m1nd_mcp::server::{dispatch_tool, enforce_generic_action_policy, McpConfig};
use m1nd_mcp::session::SessionState;
use serde_json::{json, Value};
use std::path::Path;

const AGENT: &str = "impact-chain-cap-probe";
const HUB: &str = "file::src/hub.rs";
/// The default answer ceiling the cap must hold on a chain-rich target.
const ANSWER_BYTE_BUDGET: usize = 60_000;
const DEFAULT_CHAIN_CAP: usize = 20;
const LAYER_WIDTH: usize = 24;

/// A layered call fan-out: the hub calls every node of layer 1, each of which
/// calls every node of layer 2, and so on. The detector finds far more chains
/// than its operation budget lets it keep.
fn chain_rich_state(runtime: &Path) -> SessionState {
    std::fs::create_dir_all(runtime).expect("runtime dir");
    let mut graph = Graph::new();
    let hub = graph
        .add_node(HUB, "src/hub.rs", NodeType::File, &[], 0.0, 0.0)
        .expect("hub");
    let mut previous = vec![hub];
    for layer in 1..=3 {
        let mut current = Vec::new();
        for index in 0..LAYER_WIDTH {
            let label =
                format!("a_function_with_a_rather_descriptive_name_layer{layer}_{index:02}");
            let id = format!("file::src/layer{layer}.rs::fn::{label}");
            current.push(
                graph
                    .add_node(&id, &label, NodeType::Function, &[], 0.0, 0.0)
                    .expect("node"),
            );
        }
        for &source in &previous {
            for &target in &current {
                graph
                    .add_edge(
                        source,
                        target,
                        "calls",
                        FiniteF32::new(0.9),
                        EdgeDirection::Forward,
                        false,
                        FiniteF32::new(0.95),
                    )
                    .expect("edge");
            }
        }
        previous = current;
    }
    graph.finalize().expect("finalize");
    let config = McpConfig {
        graph_source: runtime.join("graph_snapshot.json"),
        plasticity_state: runtime.join("plasticity_state.json"),
        runtime_dir: Some(runtime.to_path_buf()),
        registry_dir: Some(runtime.join("registry")),
        ..McpConfig::default()
    };
    SessionState::initialize(graph, &config, DomainConfig::code()).expect("init session")
}

fn impact(state: &mut SessionState, extra: Value) -> Value {
    let mut params = json!({ "node_id": HUB, "agent_id": AGENT });
    for (key, value) in extra.as_object().expect("object").iter() {
        params[key] = value.clone();
    }
    enforce_generic_action_policy("impact", &params).expect("impact is an ordinary read");
    dispatch_tool(state, "impact", &params).expect("impact")
}

/// Every chain the detector finds, mapped exactly as `impact` renders one —
/// the uncapped list an older binary returned.
fn full_chains(state: &SessionState) -> Vec<Value> {
    let graph = state.graph.read();
    let hub = graph.resolve_id(HUB).expect("hub node");
    let label = |node: m1nd_core::types::NodeId| {
        graph
            .strings
            .resolve(graph.nodes.label[node.as_usize()])
            .to_string()
    };
    state
        .temporal
        .chain_detector
        .detect(&graph, hub)
        .expect("detect")
        .iter()
        .map(|chain| {
            json!({
                "path": chain.path.iter().map(|&node| label(node)).collect::<Vec<_>>(),
                "relations": chain
                    .relations
                    .iter()
                    .map(|&relation| graph.strings.resolve(relation).to_string())
                    .collect::<Vec<_>>(),
                "cumulative_strength": chain.cumulative_strength.get(),
            })
        })
        .collect()
}

fn chains(output: &Value) -> Vec<Value> {
    output["causal_chains"]
        .as_array()
        .unwrap_or_else(|| panic!("impact carried no causal_chains: {output}"))
        .clone()
}

#[test]
fn default_impact_keeps_the_strongest_chains_inside_the_answer_budget() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = chain_rich_state(&temp.path().join("runtime"));
    let full = full_chains(&state);
    assert!(
        full.len() > 1_000,
        "the fixture must be chain-rich, got {}",
        full.len()
    );

    let output = impact(&mut state, json!({}));
    let returned = chains(&output);
    assert_eq!(returned.len(), DEFAULT_CHAIN_CAP, "{output}");
    assert_eq!(
        returned,
        full[..DEFAULT_CHAIN_CAP].to_vec(),
        "the cap keeps the first chains of the uncapped list, in order"
    );
    assert_eq!(output["total_causal_chains"], json!(full.len()));
    assert_eq!(output["causal_chains_truncated"], json!(true));
    let bytes = serde_json::to_vec(&output).expect("encode").len();
    assert!(
        bytes <= ANSWER_BYTE_BUDGET,
        "default impact answered {bytes} bytes, over {ANSWER_BYTE_BUDGET}"
    );
}

#[test]
fn an_opt_in_limit_restores_the_full_chain_list() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = chain_rich_state(&temp.path().join("runtime"));
    let full = full_chains(&state);

    let output = impact(&mut state, json!({ "max_causal_chains": full.len() }));
    assert_eq!(
        chains(&output),
        full,
        "the opt-in limit returns every chain"
    );
    assert_eq!(output["total_causal_chains"], json!(full.len()));
    assert_eq!(output["causal_chains_truncated"], json!(false));
    assert!(
        serde_json::to_vec(&output).expect("encode").len() > ANSWER_BYTE_BUDGET,
        "the uncapped answer is the oversized one the cap exists for"
    );

    // Any limit in between keeps the same prefix.
    let some = impact(&mut state, json!({ "max_causal_chains": 57 }));
    assert_eq!(chains(&some), full[..57].to_vec());
    assert_eq!(some["causal_chains_truncated"], json!(true));
}

#[test]
fn the_cap_leaves_the_blast_radius_untouched() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = chain_rich_state(&temp.path().join("runtime"));
    let capped = impact(&mut state, json!({}));
    let uncapped = impact(&mut state, json!({ "max_causal_chains": 1_000_000 }));
    let without = impact(&mut state, json!({ "include_causal_chains": false }));
    for key in [
        "blast_radius",
        "total_energy",
        "max_hops_reached",
        "total_blast_nodes",
        "truncated",
        "source",
        "source_label",
    ] {
        assert_eq!(capped[key], uncapped[key], "{key} moved with the chain cap");
        assert_eq!(capped[key], without[key], "{key} moved with chains off");
    }
    assert!(!capped["blast_radius"].as_array().expect("array").is_empty());

    assert_eq!(chains(&without), Vec::<Value>::new());
    assert_eq!(without["total_causal_chains"], json!(0));
    assert_eq!(without["causal_chains_truncated"], json!(false));
}
