//! FIX 3 (graph-core sheet §Gaps): PageRank staleness after incremental
//! mutation, and querying a non-finalized graph.
//!
//! (a) `compute_pagerank` runs ONLY inside `finalize`, so a post-finalize
//!     `add_node`/`add_edge` leaves the stored PageRank stale. A `pagerank_dirty`
//!     flag now records this; PageRank consumers degrade gracefully instead of
//!     re-ranking on stale/zero values.
//! (b) The query pipeline slices `csr.offsets` (`out_range`/`in_range`), which
//!     index an EMPTY offsets array until `finalize()` builds the CSR. The query
//!     paths now guard on `finalized` and return an honest empty result rather
//!     than risk indexing an unbuilt CSR / returning silently-wrong structure.

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_core::query::{QueryConfig, QueryOrchestrator};
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};

fn add_edge(g: &mut Graph, s: m1nd_core::types::NodeId, t: m1nd_core::types::NodeId) {
    g.add_edge(
        s,
        t,
        "calls",
        FiniteF32::new(1.0),
        EdgeDirection::Forward,
        false,
        FiniteF32::new(1.0),
    )
    .expect("add_edge");
}

fn build_finalized_star() -> Graph {
    let mut g = Graph::new();
    let hub = g
        .add_node("hub", "hub", NodeType::Function, &[], 0.0, 1.0)
        .expect("hub");
    for i in 0..4 {
        let leaf = g
            .add_node(
                &format!("leaf{i}"),
                &format!("leaf{i}"),
                NodeType::Function,
                &[],
                0.0,
                1.0,
            )
            .expect("leaf");
        add_edge(&mut g, hub, leaf);
    }
    g.finalize().expect("finalize");
    g
}

// ── (a) PageRank staleness flag ──────────────────────────────────────────────

#[test]
fn fresh_finalized_graph_is_not_pagerank_dirty() {
    let g = build_finalized_star();
    assert!(g.finalized, "precondition: finalized");
    assert!(
        !g.pagerank_dirty,
        "a freshly finalized graph has fresh PageRank (dirty=false)"
    );
    assert!(g.pagerank_computed, "finalize computed PageRank");
}

#[test]
fn incremental_add_node_marks_pagerank_dirty() {
    let mut g = build_finalized_star();
    assert!(!g.pagerank_dirty);

    // Post-finalize mutation: PageRank is now stale relative to the topology.
    let _ = g
        .add_node("late", "late", NodeType::Function, &[], 0.0, 1.0)
        .expect("late node");
    assert!(
        g.pagerank_dirty,
        "add_node after finalize must mark PageRank stale"
    );
    assert!(!g.finalized, "add_node also un-finalizes");
}

#[test]
fn incremental_add_edge_marks_pagerank_dirty() {
    let mut g = build_finalized_star();
    // Two isolated nodes then finalize, so add_edge below is a real post-finalize
    // topology change.
    let a = g
        .add_node("a", "a", NodeType::Function, &[], 0.0, 1.0)
        .expect("a");
    let b = g
        .add_node("b", "b", NodeType::Function, &[], 0.0, 1.0)
        .expect("b");
    g.finalize().expect("re-finalize");
    assert!(!g.pagerank_dirty, "re-finalize clears dirty");

    add_edge(&mut g, a, b);
    assert!(
        g.pagerank_dirty,
        "add_edge after finalize must mark PageRank stale"
    );
}

#[test]
fn refinalize_clears_pagerank_dirty() {
    let mut g = build_finalized_star();
    let _ = g
        .add_node("late", "late", NodeType::Function, &[], 0.0, 1.0)
        .expect("late node");
    assert!(g.pagerank_dirty, "precondition: dirty after add");

    g.finalize().expect("re-finalize");
    assert!(
        !g.pagerank_dirty,
        "re-finalize recomputes PageRank and clears the stale flag"
    );
    assert!(g.finalized);
}

// ── (b) Finalize guard on the query paths ────────────────────────────────────

#[test]
fn query_on_non_finalized_graph_is_honest_empty_not_panic() {
    // A populated but NEVER finalized graph → empty CSR. The query must refuse
    // honestly (empty result), never panic on an unbuilt CSR nor return garbage.
    let mut g = Graph::new();
    let hub = g
        .add_node("hub", "hub", NodeType::Function, &[], 0.0, 1.0)
        .expect("hub");
    let leaf = g
        .add_node("leaf", "leaf", NodeType::Function, &[], 0.0, 1.0)
        .expect("leaf");
    add_edge(&mut g, hub, leaf);
    assert!(!g.finalized, "precondition: NOT finalized");

    let orch = QueryOrchestrator::build(&g).expect("build orchestrator");
    let config = QueryConfig {
        query: "hub".to_string(),
        agent_id: "test-agent".to_string(),
        include_ghost_edges: true,
        include_structural_holes: true,
        ..Default::default()
    };

    // read-only path
    let ro = orch
        .query_readonly(&g, &config, &DomainConfig::code())
        .expect("query_readonly must not error on a non-finalized graph");
    assert!(
        ro.activation.activated.is_empty(),
        "non-finalized query_readonly must return an empty activation set"
    );
    assert!(ro.ghost_edges.is_empty() && ro.structural_holes.is_empty());

    // read-write path
    let mut g2 = Graph::new();
    let h2 = g2
        .add_node("hub", "hub", NodeType::Function, &[], 0.0, 1.0)
        .expect("hub");
    let l2 = g2
        .add_node("leaf", "leaf", NodeType::Function, &[], 0.0, 1.0)
        .expect("leaf");
    add_edge(&mut g2, h2, l2);
    let mut orch2 = QueryOrchestrator::build(&g2).expect("build orchestrator");
    let rw = orch2
        .query(&mut g2, &config, &DomainConfig::code())
        .expect("query must not error on a non-finalized graph");
    assert!(
        rw.activation.activated.is_empty(),
        "non-finalized query must return an empty activation set"
    );
}

#[test]
fn finalized_graph_still_queries_normally() {
    // Guard must NOT break the normal finalized path: a finalized graph still
    // returns real activation for a matching query.
    let mut g = build_finalized_star();
    let mut orch = QueryOrchestrator::build(&g).expect("build orchestrator");
    let config = QueryConfig {
        query: "hub".to_string(),
        agent_id: "test-agent".to_string(),
        ..Default::default()
    };
    let res = orch
        .query(&mut g, &config, &DomainConfig::code())
        .expect("finalized query ok");
    assert!(
        !res.activation.activated.is_empty(),
        "a finalized graph with a matching seed must still activate nodes"
    );
}
