//! The activation query behind `orient`/`north` must give the same answer for
//! the same graph and the same query in every process.
//!
//! Measured on 2026-11-21: the same `north` task over the same born graph
//! returned 10 distinct focus lists in 10 processes. Every `HashMap` gets its
//! own random hash keys, so orchestrators built independently from the same
//! graph stand in for separate processes (the same method as
//! `semantic_determinism.rs`).
use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_core::query::{QueryConfig, QueryOrchestrator};
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeId, NodeType};

/// Nodes whose labels match the query equally well, each its own seed family.
const TIED: u32 = 60;
const QUERY: &str = "lease enforce";

/// Sixty unconnected functions named `lease_enforce_<two letters>` score the
/// same against the query, more than the seed cut keeps; a short call chain
/// gives the structural dimension something to spread over.
fn graph() -> Graph {
    let mut graph = Graph::new();
    let letters: Vec<char> = ('a'..='z').collect();
    for i in 0..TIED {
        let suffix: String = [letters[(i / 26) as usize], letters[(i % 26) as usize]]
            .iter()
            .collect();
        graph
            .add_node(
                &format!("node::{i}"),
                &format!("lease_enforce_{suffix}"),
                NodeType::Function,
                &[],
                0.0,
                0.0,
            )
            .expect("tied node");
    }
    let mut previous: Option<NodeId> = None;
    for i in 0..10 {
        let node = graph
            .add_node(
                &format!("chain::{i}"),
                &format!("lease_registry_step_{i}"),
                NodeType::Function,
                &[],
                0.0,
                0.0,
            )
            .expect("chain node");
        if let Some(previous) = previous {
            graph
                .add_edge(
                    previous,
                    node,
                    "calls",
                    FiniteF32::ONE,
                    EdgeDirection::Forward,
                    false,
                    FiniteF32::new(0.5),
                )
                .expect("edge");
        }
        previous = Some(node);
    }
    graph.finalize().expect("finalize");
    graph
}

/// The seeds and the activated nodes of one query, as exact bits.
type Answer = (Vec<(u32, u32)>, Vec<(u32, u32)>);

fn answer(top_k: usize) -> Answer {
    let graph = graph();
    let orchestrator = QueryOrchestrator::build(&graph).expect("orchestrator");
    let config = QueryConfig {
        query: QUERY.into(),
        agent_id: "determinism".into(),
        top_k,
        include_ghost_edges: false,
        ..QueryConfig::default()
    };
    let result = orchestrator
        .query_readonly(&graph, &config, &DomainConfig::code())
        .expect("query");
    let seeds = result
        .activation
        .seeds
        .iter()
        .map(|(node, score)| (node.0, score.get().to_bits()))
        .collect();
    let activated = result
        .activation
        .activated
        .iter()
        .map(|a| (a.node.0, a.activation.get().to_bits()))
        .collect();
    (seeds, activated)
}

#[test]
fn query_answer_is_identical_across_independently_built_orchestrators() {
    let reference = answer(8);
    assert!(!reference.0.is_empty() && !reference.1.is_empty());
    for _ in 0..12 {
        assert_eq!(
            answer(8),
            reference,
            "same graph, same query, different seeds or activated nodes"
        );
    }
}

#[test]
fn equal_scores_keep_ascending_node_order_through_every_cut() {
    for _ in 0..6 {
        let (seeds, activated) = answer(8);
        for (what, ranked) in [("seeds", &seeds), ("activated", &activated)] {
            for pair in ranked.windows(2) {
                let (first, second) = (pair[0], pair[1]);
                let first_score = f32::from_bits(first.1);
                let second_score = f32::from_bits(second.1);
                assert!(
                    first_score > second_score
                        || (first_score == second_score && first.0 < second.0),
                    "{what} must rank by score, then ascending node id: {ranked:?}"
                );
            }
        }
    }
}
