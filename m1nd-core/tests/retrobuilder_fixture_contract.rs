//! The shared preparation must never turn independent algorithm tests into
//! shared mutable state, or silently normalize away a graph's learned state.
#[path = "support/retrobuilder_fixture.rs"]
mod retrobuilder_fixture;

use m1nd_core::graph::{Graph, NodeProvenanceInput};
use m1nd_core::types::{EdgeDirection, FiniteF32, InternedStr, NodeId, NodeType};
use rayon::prelude::*;
use retrobuilder_fixture::GraphFixture;
use std::sync::atomic::{AtomicUsize, Ordering};

fn seed(finalized: bool) -> Graph {
    let mut graph = Graph::new();
    // An unreferenced string and a spilled SmallVec catch lossy reconstruction.
    graph.strings.get_or_intern("unreferenced");
    for index in 0..2 {
        graph
            .add_node(
                &format!("node-{index}"),
                &format!("label-{index}"),
                NodeType::Function,
                &["a", "b", "c", "d", "e", "f", "g"],
                1234.5 + index as f64,
                0.3,
            )
            .unwrap();
        graph.set_node_provenance(
            NodeId::new(index),
            NodeProvenanceInput {
                source_path: Some("src/example.rs"),
                line_start: Some(3),
                line_end: Some(8),
                excerpt: Some("fn example() {}"),
                namespace: Some("example"),
                canonical: true,
            },
        );
    }
    graph
        .add_edge(
            NodeId::new(0),
            NodeId::new(1),
            "calls",
            FiniteF32::new(0.22),
            EdgeDirection::Bidirectional,
            true,
            FiniteF32::new(0.6),
        )
        .unwrap();
    if finalized {
        graph.finalize().unwrap();
        // Deliberately distinct CSR and learning weights: copying must preserve
        // bytes, not reconcile the two stores or reset the learning counters.
        graph.csr.weights[0].store(0.91f32.to_bits(), Ordering::Relaxed);
        graph.edge_plasticity.current_weight[0] = FiniteF32::new(0.77);
        graph.edge_plasticity.strengthen_count[0] = 9;
        graph.edge_plasticity.weaken_count[0] = 4;
        graph.edge_plasticity.ltp_applied[0] = true;
        graph.edge_plasticity.ltd_applied[0] = true;
        graph.edge_plasticity.last_used_query[0] = 123;
    }
    graph.nodes.activation[0] = [FiniteF32::new(0.42); 4];
    graph.nodes.pagerank[0] = FiniteF32::new(0.17);
    graph.nodes.plasticity[0].incoming_weight_sum = FiniteF32::new(0.63);
    graph.nodes.plasticity[0].ceiling = FiniteF32::new(3.5);
    graph.pagerank_dirty = true;
    graph
}

fn assert_same_graph(expected: &Graph, actual: &Graph) {
    macro_rules! equal {
        ($($($field:ident).+),+ $(,)?) => {$(
            assert_eq!(expected.$($field).+, actual.$($field).+, stringify!($($field).+));
        )+};
    }
    equal!(
        nodes.count,
        nodes.activation,
        nodes.pagerank,
        nodes.label,
        nodes.node_type,
        nodes.tags,
        nodes.last_modified,
        nodes.change_frequency,
        csr.offsets,
        csr.targets,
        csr.inhibitory,
        csr.relations,
        csr.directions,
        csr.causal_strengths,
        csr.rev_offsets,
        csr.rev_sources,
        csr.rev_edge_idx,
        edge_plasticity.original_weight,
        edge_plasticity.current_weight,
        edge_plasticity.strengthen_count,
        edge_plasticity.weaken_count,
        edge_plasticity.ltp_applied,
        edge_plasticity.ltd_applied,
        edge_plasticity.last_used_query,
        id_to_node,
        generation,
        pagerank_computed,
        pagerank_dirty,
        finalized,
    );
    assert_eq!(expected.strings.len(), actual.strings.len());
    for index in 0..expected.strings.len() {
        let handle = InternedStr(index as u32);
        let value = expected.strings.resolve(handle);
        assert_eq!(actual.strings.resolve(handle), value);
        assert_eq!(actual.strings.lookup(value), Some(handle));
    }
    let node_plasticity = |graph: &Graph| {
        graph
            .nodes
            .plasticity
            .iter()
            .map(|node| (node.incoming_weight_sum, node.ceiling))
            .collect::<Vec<_>>()
    };
    assert_eq!(node_plasticity(expected), node_plasticity(actual));
    let provenance = |graph: &Graph| {
        graph
            .nodes
            .provenance
            .iter()
            .map(|node| {
                (
                    node.source_path,
                    node.line_start,
                    node.line_end,
                    node.excerpt,
                    node.namespace,
                    node.canonical,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(provenance(expected), provenance(actual));
    let weights = |graph: &Graph| {
        graph
            .csr
            .weights
            .iter()
            .map(|weight| weight.load(Ordering::Relaxed))
            .collect::<Vec<_>>()
    };
    assert_eq!(weights(expected), weights(actual));
    let pending = |graph: &Graph| {
        graph
            .csr
            .pending_edges
            .iter()
            .map(|edge| {
                (
                    edge.source,
                    edge.target,
                    edge.weight,
                    edge.inhibitory,
                    edge.relation,
                    edge.direction,
                    edge.causal_strength,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(pending(expected), pending(actual));
}

#[test]
fn copies_every_field_without_rebuilding_or_finalizing() {
    for finalized in [false, true] {
        let fixture = GraphFixture::new();
        let first = fixture.graph_for_mode(None, || seed(finalized));
        assert_same_graph(&seed(finalized), &first);
        let second = fixture.graph_for_mode(None, || panic!("must reuse the prepared seed"));
        assert_same_graph(&first, &second);
    }
}

#[test]
fn caller_mutation_cannot_change_seed_or_sibling() {
    let fixture = GraphFixture::new();
    let mut changed = fixture.graph_for_mode(None, || seed(true));
    let sibling = fixture.graph_for_mode(None, || unreachable!());
    changed.nodes.activation[0][0] = FiniteF32::ONE;
    changed.nodes.pagerank[0] = FiniteF32::ONE;
    changed.nodes.tags[0].clear();
    changed.nodes.provenance[0].line_start = 99;
    changed.nodes.plasticity[0].ceiling = FiniteF32::ONE;
    changed.csr.weights[0].store(0.01f32.to_bits(), Ordering::Relaxed);
    changed.edge_plasticity.current_weight[0] = FiniteF32::ONE;
    changed.edge_plasticity.strengthen_count[0] = 999;
    changed.edge_plasticity.weaken_count[0] = 999;
    changed.edge_plasticity.ltp_applied[0] = false;
    changed.edge_plasticity.ltd_applied[0] = false;
    changed.edge_plasticity.last_used_query[0] = 999;
    changed
        .add_node("private", "private", NodeType::Class, &[], 0.0, 0.0)
        .unwrap();
    changed
        .add_edge(
            NodeId::new(1),
            NodeId::new(2),
            "private",
            FiniteF32::ONE,
            EdgeDirection::Forward,
            false,
            FiniteF32::ONE,
        )
        .unwrap();
    changed.finalize().unwrap();
    assert_same_graph(&seed(true), &sibling);
    assert_same_graph(&sibling, &fixture.graph_for_mode(None, || unreachable!()));
    assert!(sibling.strings.lookup("private").is_none());
}

#[test]
fn rayon_callers_initialize_once_and_mutate_independent_graphs() {
    let fixture = GraphFixture::new();
    let initialized = AtomicUsize::new(0);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    pool.install(|| {
        (0..64).into_par_iter().for_each(|index| {
            let mut graph = fixture.graph_for_mode(None, || {
                initialized.fetch_add(1, Ordering::SeqCst);
                seed(true)
            });
            assert_same_graph(&seed(true), &graph);
            graph.csr.weights[0].store((index as f32).to_bits(), Ordering::Relaxed);
            graph.nodes.activation[0][0] = FiniteF32::new(index as f32);
            graph.edge_plasticity.strengthen_count[0] = index as u16;
            graph.nodes.tags[0].clear();
            assert_same_graph(
                &seed(true),
                &fixture.graph_for_mode(None, || unreachable!()),
            );
        });
    });
    assert_eq!(initialized.load(Ordering::SeqCst), 1);
    assert_same_graph(
        &seed(true),
        &fixture.graph_for_mode(None, || unreachable!()),
    );
}

#[test]
fn failed_preparation_is_not_cached_as_an_empty_success() {
    let fixture = GraphFixture::new();
    assert!(
        std::panic::catch_unwind(|| fixture.graph_for_mode(None, || panic!("ingest failed")))
            .is_err()
    );
    assert_same_graph(&seed(true), &fixture.graph_for_mode(None, || seed(true)));
}

#[test]
fn process_per_test_does_not_retain_a_seed_or_skip_initialization() {
    let fixture = GraphFixture::new();
    let initialized = AtomicUsize::new(0);
    let mode = Some(std::ffi::OsStr::new("process-per-test"));
    let first = fixture.graph_for_mode(mode, || {
        initialized.fetch_add(1, Ordering::SeqCst);
        seed(true)
    });
    let second = fixture.graph_for_mode(mode, || {
        initialized.fetch_add(1, Ordering::SeqCst);
        seed(false)
    });
    assert_eq!(initialized.load(Ordering::SeqCst), 2);
    assert_same_graph(&seed(true), &first);
    assert_same_graph(&seed(false), &second);
    // The bypass must not populate the shared-process seed either.
    let shared = fixture.graph_for_mode(None, || {
        initialized.fetch_add(1, Ordering::SeqCst);
        seed(false)
    });
    assert_eq!(initialized.load(Ordering::SeqCst), 3);
    assert_same_graph(&second, &shared);
}

#[test]
fn runtime_mode_uses_the_documented_nextest_contract() {
    let fixture = GraphFixture::new();
    let initialized = AtomicUsize::new(0);
    for _ in 0..2 {
        let graph = fixture.graph(|| {
            initialized.fetch_add(1, Ordering::SeqCst);
            seed(true)
        });
        assert_same_graph(&seed(true), &graph);
    }
    let expected = if std::env::var_os("NEXTEST_EXECUTION_MODE").as_deref()
        == Some(std::ffi::OsStr::new("process-per-test"))
    {
        2
    } else {
        1
    };
    assert_eq!(initialized.load(Ordering::SeqCst), expected);
}
