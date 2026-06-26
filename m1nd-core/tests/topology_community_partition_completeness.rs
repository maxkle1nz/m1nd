//! Locks the partition-completeness invariant of `topology::CommunityDetector`
//! (surfaced by the X-RAY coverage sweep): every node gets a community, two
//! dense clusters joined by a single bridge land in distinct communities,
//! modularity stays finite and >= 0, per-community node counts sum to
//! `num_nodes`, and degenerate (single-node / empty) graphs never panic.

use m1nd_core::error::M1ndError;
use m1nd_core::graph::Graph;
use m1nd_core::topology::CommunityDetector;
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeId, NodeType};

/// Add a plain node and return its id, panicking on the (impossible-here)
/// duplicate-id path so a regression there is loud.
fn node(graph: &mut Graph, external_id: &str) -> NodeId {
    graph
        .add_node(external_id, external_id, NodeType::Function, &[], 0.0, 0.0)
        .expect("add_node should succeed for a fresh external id")
}

/// Add an undirected-weighted edge with a fixed positive weight.
fn link(graph: &mut Graph, source: NodeId, target: NodeId) {
    graph
        .add_edge(
            source,
            target,
            "calls",
            FiniteF32::new(1.0),
            EdgeDirection::Bidirectional,
            false,
            FiniteF32::ZERO,
        )
        .expect("add_edge should succeed between existing nodes");
}

/// Build two dense 4-cliques (alpha + beta clusters) joined by ONE bridge edge.
/// Returns the finalized graph plus the two cluster id-vectors.
fn two_clusters_one_bridge() -> (Graph, Vec<NodeId>, Vec<NodeId>) {
    let mut graph = Graph::new();

    let alpha: Vec<NodeId> = (0..4)
        .map(|i| node(&mut graph, &format!("alpha-{i}")))
        .collect();
    let beta: Vec<NodeId> = (0..4)
        .map(|i| node(&mut graph, &format!("beta-{i}")))
        .collect();

    // Dense intra-cluster edges (every pair) for both clusters.
    for cluster in [&alpha, &beta] {
        for a in 0..cluster.len() {
            for b in (a + 1)..cluster.len() {
                link(&mut graph, cluster[a], cluster[b]);
            }
        }
    }

    // Exactly one bridge connecting the two clusters.
    link(&mut graph, alpha[0], beta[0]);

    graph.finalize().expect("finalize should succeed");
    (graph, alpha, beta)
}

#[test]
fn detect_partitions_every_node_and_separates_clusters() {
    let (graph, alpha, beta) = two_clusters_one_bridge();
    let detector = CommunityDetector::with_defaults();

    let result = detector
        .detect(&graph)
        .expect("detect should succeed on a non-empty graph");

    // Partition completeness: one assignment slot per node.
    assert_eq!(
        result.assignments.len(),
        graph.num_nodes() as usize,
        "detect must assign a community to every node",
    );

    // Community ids are renumbered contiguously 0..num_communities.
    for assignment in &result.assignments {
        assert!(
            assignment.0 < result.num_communities,
            "every assignment must reference a contiguous community id",
        );
    }
    assert!(
        result.num_communities >= 2,
        "two dense clusters joined by one bridge must yield at least two communities",
    );

    // Each cluster must be internally homogeneous...
    let alpha_comm = result.assignments[alpha[0].as_usize()];
    for n in &alpha {
        assert_eq!(
            result.assignments[n.as_usize()],
            alpha_comm,
            "all alpha-cluster nodes must share one community",
        );
    }
    let beta_comm = result.assignments[beta[0].as_usize()];
    for n in &beta {
        assert_eq!(
            result.assignments[n.as_usize()],
            beta_comm,
            "all beta-cluster nodes must share one community",
        );
    }

    // ...and the two clusters must land in DISTINCT communities.
    assert_ne!(
        alpha_comm, beta_comm,
        "the bridged dense clusters must be detected as separate communities",
    );

    // Modularity must be finite and non-negative for a clearly modular graph.
    let modularity = result.modularity.get();
    assert!(
        modularity.is_finite(),
        "modularity Q must be finite, got {modularity}",
    );
    assert!(
        modularity >= 0.0,
        "modularity Q must be >= 0 for a clearly clustered graph, got {modularity}",
    );
}

#[test]
fn community_stats_node_counts_sum_to_num_nodes() {
    let (graph, _alpha, _beta) = two_clusters_one_bridge();
    let detector = CommunityDetector::with_defaults();
    let result = detector.detect(&graph).expect("detect should succeed");

    let stats = CommunityDetector::community_stats(&graph, &result);

    assert_eq!(
        stats.len(),
        result.num_communities as usize,
        "community_stats must report one entry per community",
    );

    let total_nodes: u32 = stats.iter().map(|s| s.node_count).sum();
    assert_eq!(
        total_nodes,
        graph.num_nodes(),
        "per-community node counts must sum to num_nodes (partition completeness)",
    );

    // Density is a ratio in [0, 1] and finite for every community.
    for s in &stats {
        let density = s.density.get();
        assert!(
            density.is_finite() && (0.0..=1.0).contains(&density),
            "density must be a finite ratio in [0,1], got {density}",
        );
    }
}

#[test]
fn single_node_graph_does_not_panic() {
    let mut graph = Graph::new();
    let _solo = node(&mut graph, "solo");
    graph.finalize().expect("finalize should succeed");

    let detector = CommunityDetector::with_defaults();
    // A single isolated node has no edges (two_m == 0): detect returns Ok with
    // each node in its own community rather than panicking.
    let result = detector
        .detect(&graph)
        .expect("single-node graph must not error");

    assert_eq!(
        result.assignments.len(),
        1,
        "single-node graph must still assign exactly one community slot",
    );
    assert!(
        result.modularity.get().is_finite(),
        "single-node modularity must be finite",
    );

    let stats = CommunityDetector::community_stats(&graph, &result);
    let total: u32 = stats.iter().map(|s| s.node_count).sum();
    assert_eq!(
        total, 1,
        "stats for a single-node graph must count one node"
    );
}

#[test]
fn empty_graph_returns_err_not_panic() {
    let mut graph = Graph::new();
    graph
        .finalize()
        .expect("finalize of empty graph should succeed");

    let detector = CommunityDetector::with_defaults();
    let err = detector
        .detect(&graph)
        .expect_err("empty graph must return Err, not a result or panic");

    assert!(
        matches!(err, M1ndError::EmptyGraph),
        "empty-graph detect must return M1ndError::EmptyGraph, got {err:?}",
    );
}
