//! Locks the read-only query surface of `query::QueryOrchestrator`: that
//! `query_readonly` leaves a shared `Graph` byte-for-byte untouched (its whole
//! reason to exist for concurrent readers) and that `detect_ghost_edges` /
//! `detect_structural_holes` only ever name in-graph `NodeId`s and stay
//! panic-free on an empty activation set. Surfaced by the X-RAY coverage sweep.
//!
//! Why it matters:
//! * `query_readonly` is the path a second, read-only m1nd process uses against
//!   a graph other readers depend on — a stray mutation would corrupt them.
//! * the two detectors return `NodeId`s that callers index straight back into
//!   the graph; an out-of-range id would be an out-of-bounds bug downstream.

use m1nd_core::activation::{ActivatedNode, ActivationResult};
use m1nd_core::graph::Graph;
use m1nd_core::query::{QueryConfig, QueryOrchestrator};
use m1nd_core::types::{EdgeDirection, FiniteF32, Generation, NodeId, NodeType};

/// Build a small connected graph (a star plus a couple of leaves) and finalize
/// it so the CSR is populated. Returns the finalized graph.
fn build_small_graph() -> Graph {
    let mut graph = Graph::new();

    // A hub plus four spokes — enough degree to give the detectors something to
    // chew on without depending on the heavier ingest pipeline.
    let hub = graph
        .add_node("hub", "hub", NodeType::Function, &["core"], 0.0, 1.0)
        .expect("add hub");
    let alpha = graph
        .add_node("alpha", "alpha", NodeType::Function, &["core"], 0.0, 1.0)
        .expect("add alpha");
    let beta = graph
        .add_node("beta", "beta", NodeType::Function, &["core"], 0.0, 1.0)
        .expect("add beta");
    let gamma = graph
        .add_node("gamma", "gamma", NodeType::Function, &["core"], 0.0, 1.0)
        .expect("add gamma");
    let delta = graph
        .add_node("delta", "delta", NodeType::Function, &["core"], 0.0, 1.0)
        .expect("add delta");

    for target in [alpha, beta, gamma, delta] {
        graph
            .add_edge(
                hub,
                target,
                "calls",
                FiniteF32::new(1.0),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(1.0),
            )
            .expect("add edge from hub");
    }
    // One extra edge to give a non-hub node out-degree too.
    graph
        .add_edge(
            alpha,
            beta,
            "calls",
            FiniteF32::new(1.0),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(1.0),
        )
        .expect("add edge alpha->beta");

    graph.finalize().expect("finalize graph");
    graph
}

/// Snapshot the mutation-visible state of a graph: the monotonic generation
/// counter (bumped on every structural mutation), node/edge counts, and the
/// live edge weights (what plasticity would rewrite).
fn graph_fingerprint(graph: &Graph) -> (Generation, u32, usize, Vec<FiniteF32>) {
    (
        graph.generation,
        graph.num_nodes(),
        graph.num_edges(),
        graph.edge_plasticity.current_weight.clone(),
    )
}

/// Hand-build an activation result over real node ids so the detectors get a
/// deterministic, two-dimension activation set without running the full engine.
fn activation_over(nodes: &[(NodeId, [f32; 4])]) -> ActivationResult {
    let activated = nodes
        .iter()
        .map(|(node, dims)| {
            let dimensions = [
                FiniteF32::new(dims[0]),
                FiniteF32::new(dims[1]),
                FiniteF32::new(dims[2]),
                FiniteF32::new(dims[3]),
            ];
            let active_dimension_count = dims.iter().filter(|d| **d > 0.01).count() as u8;
            // Combined score: just the max contributing dimension, kept in (0,1].
            let activation = FiniteF32::new(dims.iter().cloned().fold(0.0_f32, f32::max));
            ActivatedNode {
                node: *node,
                activation,
                dimensions,
                active_dimension_count,
            }
        })
        .collect();

    ActivationResult {
        activated,
        seeds: Vec::new(),
        elapsed_ns: 0,
        xlr_fallback_used: false,
    }
}

#[test]
fn query_readonly_does_not_mutate_the_graph() {
    let graph = build_small_graph();
    let orchestrator = QueryOrchestrator::build(&graph).expect("build orchestrator");

    let before = graph_fingerprint(&graph);

    let config = QueryConfig {
        query: "hub".to_string(),
        agent_id: "test-agent".to_string(),
        include_ghost_edges: true,
        include_structural_holes: true,
        ..Default::default()
    };

    // `query_readonly` takes `&Graph`; the borrow checker already forbids a
    // structural write, but we additionally pin the observable state so a future
    // change that smuggles in interior mutation (atomics on edge weights, the
    // generation counter) is caught.
    let result = orchestrator
        .query_readonly(&graph, &config)
        .expect("query_readonly succeeds");

    let after = graph_fingerprint(&graph);
    assert_eq!(
        before, after,
        "query_readonly must not mutate generation, counts, or edge weights"
    );

    // The read-only path deliberately zeroes plasticity (Step 8 skipped).
    assert_eq!(
        result.plasticity.edges_strengthened, 0,
        "read-only query must not strengthen edges"
    );
    assert_eq!(
        result.plasticity.edges_decayed, 0,
        "read-only query must not decay edges"
    );

    // Every NodeId named in the result must be a real in-graph node.
    let n = graph.num_nodes();
    for node in &result.activation.activated {
        assert!(
            node.node.0 < n,
            "activated node id {} out of range (n={n})",
            node.node.0
        );
    }
    for ghost in &result.ghost_edges {
        assert!(
            ghost.source.0 < n && ghost.target.0 < n,
            "ghost edge references out-of-range node(s)"
        );
    }
    for hole in &result.structural_holes {
        assert!(
            hole.node.0 < n,
            "structural hole references out-of-range node {}",
            hole.node.0
        );
    }
}

#[test]
fn detectors_reference_only_in_graph_node_ids() {
    let graph = build_small_graph();
    let orchestrator = QueryOrchestrator::build(&graph).expect("build orchestrator");
    let n = graph.num_nodes();

    // alpha(1) and gamma(3) share two dimensions and are NOT directly connected
    // in our graph (only hub->gamma and alpha->beta exist), so they are a prime
    // ghost-edge candidate. Give the hub's neighbours strong activation while
    // leaving the hub cold to also provoke a structural hole.
    let alpha = graph.resolve_id("alpha").expect("alpha exists");
    let beta = graph.resolve_id("beta").expect("beta exists");
    let gamma = graph.resolve_id("gamma").expect("gamma exists");
    let delta = graph.resolve_id("delta").expect("delta exists");

    let activation = activation_over(&[
        (alpha, [0.9, 0.9, 0.0, 0.0]),
        (beta, [0.8, 0.8, 0.0, 0.0]),
        (gamma, [0.7, 0.7, 0.0, 0.0]),
        (delta, [0.6, 0.6, 0.0, 0.0]),
    ]);

    let ghosts = orchestrator
        .detect_ghost_edges(&graph, &activation)
        .expect("detect_ghost_edges");
    for ghost in &ghosts {
        assert!(
            ghost.source.0 < n && ghost.target.0 < n,
            "ghost edge endpoints must be valid node ids"
        );
        assert_ne!(
            ghost.source, ghost.target,
            "a ghost edge must connect two distinct nodes"
        );
        assert!(
            ghost.shared_dimensions.len() >= 2,
            "ghost edge must share at least two dimensions by construction"
        );
        assert!(
            ghost.strength.get() >= 0.0 && ghost.strength.get() <= 1.0,
            "ghost strength must be clamped to [0,1]"
        );
    }

    let holes = orchestrator
        .detect_structural_holes(&graph, &activation, FiniteF32::new(0.3))
        .expect("detect_structural_holes");
    for hole in &holes {
        assert!(
            hole.node.0 < n,
            "structural hole node id {} out of range (n={n})",
            hole.node.0
        );
        // A hole is, by definition, a node that was itself inactive — the four
        // activated nodes above must therefore never appear as holes.
        assert!(
            ![alpha, beta, gamma, delta].contains(&hole.node),
            "an activated node must not be reported as a structural hole"
        );
    }
}

#[test]
fn empty_activation_yields_empty_detector_output_without_panic() {
    let graph = build_small_graph();
    let orchestrator = QueryOrchestrator::build(&graph).expect("build orchestrator");

    let empty = ActivationResult {
        activated: Vec::new(),
        seeds: Vec::new(),
        elapsed_ns: 0,
        xlr_fallback_used: false,
    };

    let ghosts = orchestrator
        .detect_ghost_edges(&graph, &empty)
        .expect("detect_ghost_edges on empty activation");
    assert!(
        ghosts.is_empty(),
        "no activated nodes can produce no ghost edges"
    );

    let holes = orchestrator
        .detect_structural_holes(&graph, &empty, FiniteF32::new(0.3))
        .expect("detect_structural_holes on empty activation");
    assert!(
        holes.is_empty(),
        "with zero activation no node has activated neighbours, so no holes"
    );
}
