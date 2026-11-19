//! Semantic scores, and which nodes survive a top-k cut, must not depend on
//! per-process hash order. Every `HashMap` gets its own random hash keys, so
//! engines built independently from the same graph stand in for two processes.
use m1nd_core::graph::Graph;
use m1nd_core::semantic::{CharNgramIndex, CoOccurrenceIndex, SemanticEngine, SynonymExpander};
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeId, NodeType, SemanticWeights};

const TIED_LABEL: &str = "record_tick_metrics";
const TIED: u32 = 40;

fn engine(graph: &Graph) -> SemanticEngine {
    SemanticEngine {
        ngram: CharNgramIndex::build(graph, 3).expect("ngram index"),
        cooccurrence: CoOccurrenceIndex::build(graph, 10, 20, 4).expect("cooccurrence index"),
        synonym: SynonymExpander::build(vec![vec!["record", "store"]]).expect("synonyms"),
        weights: SemanticWeights::default(),
        #[cfg(feature = "embed")]
        embeddings: Default::default(),
        #[cfg(feature = "embed")]
        embedder: None,
    }
}

/// Forty unconnected nodes share one label (exact score ties); thirty more form
/// a call chain and have long, distinct labels whose scores are sums of many
/// trigram terms.
fn graph() -> Graph {
    let mut graph = Graph::new();
    let mut previous: Option<NodeId> = None;
    for i in 0..TIED + 30 {
        let label = if i < TIED {
            TIED_LABEL.to_string()
        } else {
            format!("server_metrics_record_tick_timings_and_intent_counts_variant_{i}")
        };
        let node = graph
            .add_node(
                &format!("node::{i}"),
                &label,
                NodeType::Function,
                &[],
                0.0,
                0.0,
            )
            .expect("node");
        if let Some(previous) = previous.filter(|_| i > TIED) {
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

fn bits(hits: &[(NodeId, FiniteF32)]) -> Vec<(u32, u32)> {
    hits.iter()
        .map(|(node, score)| (node.0, score.get().to_bits()))
        .collect()
}

#[test]
fn semantic_query_is_identical_across_independently_built_engines() {
    let graph = graph();
    let query = "which code records per tick timings and intent counts into server metrics";
    let reference = bits(&engine(&graph).query(&graph, query, 12).expect("query"));
    assert!(!reference.is_empty());
    for _ in 0..8 {
        let other = bits(&engine(&graph).query(&graph, query, 12).expect("query"));
        assert_eq!(other, reference, "same graph, same query, different answer");
    }
}

#[test]
fn equal_semantic_scores_keep_ascending_node_order_through_the_cut() {
    let graph = graph();
    for _ in 0..8 {
        let hits = engine(&graph).query(&graph, TIED_LABEL, 5).expect("query");
        let nodes: Vec<u32> = hits.iter().map(|(node, _)| node.0).collect();
        assert_eq!(
            nodes,
            vec![0, 1, 2, 3, 4],
            "five of forty equal scores must be the first five nodes: {hits:?}"
        );
        assert!(hits.windows(2).all(|pair| pair[0].1 == pair[1].1));
    }
}
