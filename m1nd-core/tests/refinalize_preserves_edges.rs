//! Regression: `Graph::finalize()` must be non-destructive across re-finalization.
//!
//! Before the fix, `finalize()` rebuilt the CSR purely from `pending_edges`. After the
//! first finalize that staging list is empty, so a later `add_node()` (which flips
//! `finalized = false` without staging any edge) followed by another `finalize()` wiped
//! every edge already materialized in the CSR. This is the standing pipeline bug behind
//! the live "edges_created=11279 but edge_count=2" symptom.

use m1nd_core::graph::Graph;
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};

fn add_edge(g: &mut Graph, s: m1nd_core::types::NodeId, t: m1nd_core::types::NodeId, rel: &str) {
    g.add_edge(
        s,
        t,
        rel,
        FiniteF32::new(1.0),
        EdgeDirection::Forward,
        false,
        FiniteF32::new(0.5),
    )
    .expect("add_edge");
}

#[test]
fn re_finalize_after_add_node_preserves_existing_csr_edges() {
    let mut g = Graph::new();
    let a = g
        .add_node("file::a.rs", "a", NodeType::File, &[], 0.0, 0.0)
        .unwrap();
    let b = g
        .add_node("file::b.rs", "b", NodeType::File, &[], 0.0, 0.0)
        .unwrap();
    add_edge(&mut g, a, b, "imports");
    g.finalize().unwrap();
    assert_eq!(g.num_edges(), 1, "baseline edge after first finalize");

    // Add a node post-finalize (flips finalized=false, pending_edges stays empty) and
    // re-finalize. The pre-existing CSR edge MUST survive.
    let _c = g
        .add_node("file::c.rs", "c", NodeType::File, &[], 0.0, 0.0)
        .unwrap();
    assert!(!g.finalized, "add_node should mark graph dirty");
    g.finalize().unwrap();

    assert_eq!(
        g.num_edges(),
        1,
        "re-finalize after add_node must NOT drop the existing CSR edge"
    );
    // The neighbor relationship must remain queryable.
    let has_edge = g.csr.out_range(a).any(|i| g.csr.targets[i] == b);
    assert!(
        has_edge,
        "a -> b edge must still be queryable after re-finalize"
    );
}

#[test]
fn re_finalize_merges_existing_csr_edges_with_new_pending_edges() {
    let mut g = Graph::new();
    let a = g
        .add_node("file::a.rs", "a", NodeType::File, &[], 0.0, 0.0)
        .unwrap();
    let b = g
        .add_node("file::b.rs", "b", NodeType::File, &[], 0.0, 0.0)
        .unwrap();
    add_edge(&mut g, a, b, "imports");
    g.finalize().unwrap();
    assert_eq!(g.num_edges(), 1);

    // Add a node AND a new edge, then re-finalize: both the old and new edge survive.
    let c = g
        .add_node("file::c.rs", "c", NodeType::File, &[], 0.0, 0.0)
        .unwrap();
    add_edge(&mut g, b, c, "calls");
    g.finalize().unwrap();

    assert_eq!(
        g.num_edges(),
        2,
        "re-finalize must keep the old CSR edge and the freshly staged edge"
    );
    assert!(g.csr.out_range(a).any(|i| g.csr.targets[i] == b));
    assert!(g.csr.out_range(b).any(|i| g.csr.targets[i] == c));
}

#[test]
fn repeated_finalize_is_stable_at_scale() {
    let mut g = Graph::new();
    let nodes: Vec<_> = (0..200)
        .map(|i| {
            g.add_node(
                &format!("file::n{i}.rs"),
                "n",
                NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .unwrap()
        })
        .collect();
    // Chain edges n0->n1->...->n199 (199 edges).
    for w in nodes.windows(2) {
        add_edge(&mut g, w[0], w[1], "imports");
    }
    g.finalize().unwrap();
    let baseline = g.num_edges();
    assert_eq!(baseline, 199);

    // Three rounds of add-node + re-finalize must not erode the edge set.
    for round in 0..3 {
        g.add_node(
            &format!("file::extra{round}.rs"),
            "x",
            NodeType::File,
            &[],
            0.0,
            0.0,
        )
        .unwrap();
        g.finalize().unwrap();
        assert_eq!(
            g.num_edges(),
            baseline,
            "edge count must stay exact ({baseline}) after re-finalize round {round}"
        );
    }
}
