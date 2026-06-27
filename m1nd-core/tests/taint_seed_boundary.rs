//! Regression: a security boundary that is itself an entry (seed) node must be
//! reported as a HIT with maximal taint, not as a missed boundary.
//!
//! `TaintEngine::analyze` derived `taint_reached`/`infection_probability` purely
//! from the epidemic predictions. But the epidemic excludes the seed nodes from
//! its predictions (and truncates to top_k), so a boundary node that coincides
//! with an injection point (e.g. a `validate_*` function called directly with
//! user input) was pushed into `boundary_misses` with probability 0.0. That
//! falsely reports a security boundary as bypassed when taint is in fact
//! maximally present at it, and inflates the risk score.
//!
//! A seed carries maximal taint by definition, so a boundary that is a seed is a
//! HIT with probability 1.0. (Surfaced by the X-RAY adversarial bug hunt; proven
//! failing-then-passing.)

use m1nd_core::graph::Graph;
use m1nd_core::taint::{TaintConfig, TaintEngine};
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeId, NodeType};

fn add(g: &mut Graph, ext: &str, label: &str) {
    g.add_node(ext, label, NodeType::Function, &["svc"], 0.0, 0.1)
        .unwrap();
}

#[test]
fn boundary_that_is_an_entry_seed_is_a_hit_not_a_miss() {
    // Node 0's label matches the UserInput boundary pattern "validate" AND it is
    // the entry/seed (the injection point). Node 1 is a plain downstream node.
    let mut g = Graph::new();
    add(&mut g, "svc::validate_request", "validate_request");
    add(&mut g, "svc::handler", "handler");
    g.add_edge(
        NodeId::new(0),
        NodeId::new(1),
        "calls",
        FiniteF32::new(0.9),
        EdgeDirection::Forward,
        false,
        FiniteF32::new(0.5),
    )
    .unwrap();
    g.finalize().unwrap();

    // Default taint_type is UserInput, whose patterns include "validate".
    let config = TaintConfig::default();
    let result = TaintEngine::analyze(&g, &[NodeId::new(0)], &config).unwrap();

    let in_misses = result
        .boundary_misses
        .iter()
        .any(|b| b.label == "validate_request");
    assert!(
        !in_misses,
        "a boundary that is the seed must NOT be reported as a miss; misses={:?}",
        result.boundary_misses
    );

    let hit = result
        .boundary_hits
        .iter()
        .find(|b| b.label == "validate_request")
        .expect("the seed boundary must be reported as a HIT");
    assert!(
        hit.taint_reached,
        "the seed boundary must have taint_reached == true"
    );
    assert!(
        hit.infection_probability >= 0.99,
        "the seed carries maximal taint, expected probability ~1.0, got {}",
        hit.infection_probability
    );

    // The summary must not contradict the boundary hits: if a boundary is
    // reported at probability 1.0, the summary maximum cannot be lower.
    assert!(
        result.summary.max_infection_probability >= hit.infection_probability,
        "summary.max_infection_probability ({}) must not be below a reported \
         boundary hit ({})",
        result.summary.max_infection_probability,
        hit.infection_probability
    );
}
