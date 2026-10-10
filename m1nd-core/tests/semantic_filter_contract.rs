//! Eligibility must be applied before semantic candidate limits, not after them.
use m1nd_core::graph::Graph;
use m1nd_core::semantic::{CharNgramIndex, CoOccurrenceIndex, SemanticEngine, SynonymExpander};
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeId, NodeType, SemanticWeights};

fn engine(graph: &Graph, weights: SemanticWeights) -> SemanticEngine {
    SemanticEngine {
        ngram: CharNgramIndex::build(graph, 3).expect("ngram index"),
        cooccurrence: CoOccurrenceIndex::build(graph, 10, 20, 4).expect("cooccurrence index"),
        synonym: SynonymExpander::build(vec![vec!["fetch", "retrieve"]]).expect("synonyms"),
        weights,
        #[cfg(feature = "embed")]
        embeddings: Default::default(),
        #[cfg(feature = "embed")]
        embedder: None,
    }
}

fn add(graph: &mut Graph, label: &str) -> NodeId {
    let id = format!("node::{}", graph.num_nodes());
    graph
        .add_node(&id, label, NodeType::Function, &[], 0.0, 0.0)
        .expect("node")
}

#[test]
fn semantic_filtered_query_keeps_ngram_budget_for_eligible_nodes() {
    let mut graph = Graph::new();
    for _ in 0..30 {
        add(&mut graph, "needle_scope_match");
    }
    let eligible = add(&mut graph, "needle_scope_match_adapter");
    graph.finalize().expect("finalize");
    let engine = engine(&graph, SemanticWeights::default());
    let hits = engine
        .query_filtered(&graph, "needle_scope_match", 1, |id| id == eligible)
        .expect("query");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, eligible);
    assert!(engine
        .query_filtered(&graph, "needle_scope_match", 1, |_| false)
        .expect("empty query")
        .is_empty());
}

#[test]
fn semantic_filtered_query_applies_synonym_eligibility_before_limit() {
    let mut graph = Graph::new();
    for _ in 0..30 {
        add(&mut graph, "retrieve");
    }
    let eligible = add(&mut graph, "retrieve");
    graph.finalize().expect("finalize");
    let engine = engine(
        &graph,
        SemanticWeights {
            ngram: FiniteF32::ZERO,
            cooccurrence: FiniteF32::ZERO,
            synonym: FiniteF32::ONE,
        },
    );
    let hits = engine
        .query_filtered(&graph, "fetch", 1, |id| id == eligible)
        .expect("query");
    assert_eq!(hits, vec![(eligible, FiniteF32::ONE)]);
}

#[test]
fn semantic_filtered_query_keeps_cooccurrence_candidates_eligible() {
    let mut graph = Graph::new();
    let seed = add(&mut graph, "needle_scope_match");
    let target = add(&mut graph, "qz9wb");
    for label in ["outside_a", "outside_b", "outside_c", "outside_d"] {
        let outside = add(&mut graph, label);
        for node in [seed, target] {
            graph
                .add_edge(
                    node,
                    outside,
                    "calls",
                    FiniteF32::ONE,
                    EdgeDirection::Bidirectional,
                    false,
                    FiniteF32::ONE,
                )
                .expect("edge");
        }
    }
    graph.finalize().expect("finalize");
    let engine = engine(
        &graph,
        SemanticWeights {
            ngram: FiniteF32::ZERO,
            cooccurrence: FiniteF32::ONE,
            synonym: FiniteF32::ZERO,
        },
    );
    // Select a positive candidate beyond the unfiltered 3x overfetch limit.
    // Filtering only after that limit would lose this eligible result.
    let cooccurrence = engine
        .cooccurrence
        .query_top_k(seed, graph.num_nodes() as usize);
    assert!(
        cooccurrence.len() > 3,
        "fixture needs more than three positive neighbors"
    );
    let target = cooccurrence[3].0;
    let hits = engine
        .query_filtered(&graph, "needle_scope_match", 1, |id| {
            id == seed || id == target
        })
        .expect("query");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].0, target);
    assert!(hits[0].1.get() > 0.01);
}

#[test]
fn semantic_unfiltered_wrapper_accepts_all_eligible_nodes() {
    let mut graph = Graph::new();
    for label in ["fetch", "retrieve", "fetch_remote", "unrelated"] {
        add(&mut graph, label);
    }
    graph.finalize().expect("finalize");
    let engine = engine(&graph, SemanticWeights::default());
    let mut original = engine.query(&graph, "fetch", 20).expect("query");
    let mut filtered = engine
        .query_filtered(&graph, "fetch", 20, |_| true)
        .expect("filtered query");
    original.sort_by_key(|(node, _)| node.0);
    filtered.sort_by_key(|(node, _)| node.0);
    assert_eq!(original.len(), filtered.len());
    for ((original_node, original_score), (filtered_node, filtered_score)) in
        original.into_iter().zip(filtered)
    {
        assert_eq!(original_node, filtered_node);
        assert!((original_score.get() - filtered_score.get()).abs() < 1e-6);
    }
}
