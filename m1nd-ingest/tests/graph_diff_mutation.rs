//! GraphDiff must mutate retained topology without discarding unrelated state.

use m1nd_core::graph::Graph;
use m1nd_core::types::{EdgeDirection, EdgeIdx, FiniteF32, NodeId, NodeType};
use m1nd_ingest::diff::{DiffAction, GraphDiff};
use m1nd_ingest::extract::{ExtractedEdge, ExtractedNode};

fn diff(actions: Vec<DiffAction>) -> GraphDiff {
    GraphDiff {
        actions,
        nodes_added: 0,
        nodes_removed: 0,
        nodes_modified: 0,
        edges_added: 0,
        edges_removed: 0,
    }
}

fn remove(source: &str, target: &str, relation: &str) -> DiffAction {
    DiffAction::RemoveEdge {
        source_id: source.into(),
        target_id: target.into(),
        relation: relation.into(),
    }
}

fn node(graph: &mut Graph, id: &str) -> NodeId {
    graph
        .add_node(id, id, NodeType::Function, &["stable"], 0.0, 0.5)
        .unwrap()
}

fn edge(graph: &mut Graph, source: NodeId, target: NodeId, relation: &str, bidi: bool) {
    graph
        .add_edge(
            source,
            target,
            relation,
            FiniteF32::new(0.4),
            if bidi {
                EdgeDirection::Bidirectional
            } else {
                EdgeDirection::Forward
            },
            true,
            FiniteF32::new(0.7),
        )
        .unwrap();
}

// Include every edge column and all plasticity columns; weights use exact bits.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Row(
    u32,
    u32,
    String,
    bool,
    bool,
    u32,
    [u32; 3],
    u16,
    u16,
    bool,
    bool,
    u32,
);

fn rows(graph: &Graph) -> Vec<Row> {
    let state = |source: NodeId,
                 target: NodeId,
                 relation,
                 direction,
                 inhibitory,
                 causal: FiniteF32,
                 weight: FiniteF32,
                 slot: usize| {
        let p = &graph.edge_plasticity;
        Row(
            source.0,
            target.0,
            graph.strings.resolve(relation).into(),
            direction == EdgeDirection::Bidirectional,
            inhibitory,
            causal.get().to_bits(),
            [
                weight.get().to_bits(),
                p.original_weight[slot].get().to_bits(),
                p.current_weight[slot].get().to_bits(),
            ],
            p.strengthen_count[slot],
            p.weaken_count[slot],
            p.ltp_applied[slot],
            p.ltd_applied[slot],
            p.last_used_query[slot],
        )
    };
    let mut rows = Vec::new();
    for source in 0..graph.num_nodes() {
        for slot in graph.csr.out_range(NodeId::new(source)) {
            rows.push(state(
                NodeId::new(source),
                graph.csr.targets[slot],
                graph.csr.relations[slot],
                graph.csr.directions[slot],
                graph.csr.inhibitory[slot],
                graph.csr.causal_strengths[slot],
                graph.csr.read_weight(EdgeIdx::new(slot as u32)),
                slot,
            ));
        }
    }
    for (i, edge) in graph.csr.pending_edges.iter().enumerate() {
        let row = state(
            edge.source,
            edge.target,
            edge.relation,
            edge.direction,
            edge.inhibitory,
            edge.causal_strength,
            edge.weight,
            graph.num_edges() + i,
        );
        rows.push(row.clone());
        if edge.direction == EdgeDirection::Bidirectional {
            let mut reverse = row;
            std::mem::swap(&mut reverse.0, &mut reverse.1);
            rows.push(reverse);
        }
    }
    rows.sort();
    rows
}

fn learn(graph: &mut Graph) {
    for slot in 0..graph.num_edges() {
        let weight = FiniteF32::new(0.8 + slot as f32 * 0.1);
        graph
            .csr
            .atomic_write_weight(EdgeIdx::new(slot as u32), weight, 10)
            .unwrap();
        let p = &mut graph.edge_plasticity;
        p.current_weight[slot] = weight;
        p.original_weight[slot] = FiniteF32::new(0.2 + slot as f32 * 0.01);
        p.strengthen_count[slot] = slot as u16 + 5;
        p.weaken_count[slot] = slot as u16 + 2;
        p.ltp_applied[slot] = slot % 2 == 0;
        p.ltd_applied[slot] = slot % 2 != 0;
        p.last_used_query[slot] = slot as u32 + 70;
    }
}

fn assert_reverse_and_lengths(graph: &Graph) {
    let n = graph.num_edges();
    let p = &graph.edge_plasticity;
    for len in [
        p.original_weight.len(),
        p.current_weight.len(),
        p.strengthen_count.len(),
        p.weaken_count.len(),
        p.ltp_applied.len(),
        p.ltd_applied.len(),
        p.last_used_query.len(),
    ] {
        assert_eq!(len, n);
    }
    let mut seen = vec![false; n];
    for target in 0..graph.num_nodes() {
        for reverse in graph.csr.in_range(NodeId::new(target)) {
            let slot = graph.csr.rev_edge_idx[reverse].as_usize();
            assert!(!seen[slot]);
            seen[slot] = true;
            assert_eq!(graph.csr.targets[slot], NodeId::new(target));
            assert!(graph
                .csr
                .out_range(graph.csr.rev_sources[reverse])
                .contains(&slot));
        }
    }
    assert!(seen.iter().all(|seen| *seen));
}

#[test]
fn remove_live_edge_preserves_parallel_relations_and_opposite_direction() {
    let mut graph = Graph::new();
    let a = node(&mut graph, "a");
    let b = node(&mut graph, "b");
    edge(&mut graph, a, b, "calls", false);
    edge(&mut graph, a, b, "calls", false);
    edge(&mut graph, a, b, "imports", false);
    edge(&mut graph, a, b, "imports", false);
    edge(&mut graph, b, a, "calls", false);
    graph.finalize().unwrap();
    learn(&mut graph);
    let expected: Vec<_> = rows(&graph)
        .into_iter()
        .filter(|row| !(row.0 == a.0 && row.1 == b.0 && row.2 == "calls"))
        .collect();
    let generation = graph.generation;
    assert_eq!(
        diff(vec![remove("a", "b", "calls")])
            .apply(&mut graph)
            .unwrap(),
        1
    );
    let completion = (graph.finalized, graph.pagerank_dirty);
    graph.finalize().unwrap();
    assert_eq!(rows(&graph), expected);
    assert_eq!(graph.generation, generation.next());
    assert_eq!(completion, (true, false));
    assert_eq!(graph.num_nodes(), 2);
    assert!(!graph.pagerank_dirty);
    assert_reverse_and_lengths(&graph);
}

#[test]
fn remove_from_csr_and_pending_preserves_asymmetric_bidirectional_learning() {
    let mut graph = Graph::new();
    let a = node(&mut graph, "a");
    let b = node(&mut graph, "b");
    edge(&mut graph, a, b, "remove", false);
    edge(&mut graph, b, a, "keep-bidi", true);
    edge(&mut graph, a, a, "keep-loop", true);
    graph.finalize().unwrap();
    learn(&mut graph);
    let c = node(&mut graph, "c");
    edge(&mut graph, a, b, "remove", true);
    edge(&mut graph, c, b, "new-bidi", true);
    edge(&mut graph, a, c, "new-forward", false);
    let pending = graph.csr.pending_edges.last_mut().unwrap();
    pending.inhibitory = false;
    pending.causal_strength = FiniteF32::new(0.2);
    for slot in graph.num_edges()..graph.edge_plasticity.original_weight.len() {
        graph.edge_plasticity.strengthen_count[slot] = 23;
        graph.edge_plasticity.weaken_count[slot] = 11;
        graph.edge_plasticity.ltp_applied[slot] = true;
        graph.edge_plasticity.ltd_applied[slot] = true;
        graph.edge_plasticity.last_used_query[slot] = 91;
    }
    let expected: Vec<_> = rows(&graph)
        .into_iter()
        .filter(|row| row.2 != "remove")
        .collect();
    diff(vec![remove("a", "b", "remove")])
        .apply(&mut graph)
        .unwrap();
    let completion = (graph.finalized, graph.pagerank_dirty);
    graph.finalize().unwrap();
    assert_eq!(rows(&graph), expected);
    assert_eq!(completion, (true, false));
    assert_reverse_and_lengths(&graph);
    let generation = graph.generation;
    graph.finalize().unwrap();
    assert_eq!(rows(&graph), expected);
    assert_eq!(graph.generation, generation);
    node(&mut graph, "d");
    graph.finalize().unwrap();
    assert_eq!(
        rows(&graph),
        expected,
        "later re-finalize must not mirror away reverse learning"
    );
}

#[test]
fn pending_only_removal_and_bidirectional_reverse_endpoint_match() {
    let mut graph = Graph::new();
    let a = node(&mut graph, "a");
    let b = node(&mut graph, "b");
    edge(&mut graph, b, a, "remove", true);
    edge(&mut graph, a, b, "keep", true);
    edge(&mut graph, b, a, "keep", false);
    let expected: Vec<_> = rows(&graph)
        .into_iter()
        .filter(|row| row.2 != "remove")
        .collect();
    diff(vec![remove("a", "b", "remove")])
        .apply(&mut graph)
        .unwrap();
    graph.finalize().unwrap();
    assert_eq!(rows(&graph), expected);
    assert_reverse_and_lengths(&graph);
}

#[test]
fn absent_removal_is_a_complete_noop_for_finalized_and_pending_graphs() {
    let mut graph = Graph::new();
    let a = node(&mut graph, "a");
    let b = node(&mut graph, "b");
    edge(&mut graph, a, b, "calls", false);
    graph.finalize().unwrap();
    for pending in [false, true] {
        if pending {
            edge(&mut graph, b, a, "pending", false);
        }
        let before = rows(&graph);
        let metadata = (
            graph.generation,
            graph.finalized,
            graph.pagerank_dirty,
            graph.pagerank_computed,
            graph.strings.len(),
            graph.csr.offsets.clone(),
            graph.csr.pending_edges.len(),
        );
        diff(vec![
            remove("a", "b", "absent"),
            remove("missing", "b", "calls"),
            remove("b", "a", "calls"),
        ])
        .apply(&mut graph)
        .unwrap();
        assert_eq!(rows(&graph), before);
        assert_eq!(
            (
                graph.generation,
                graph.finalized,
                graph.pagerank_dirty,
                graph.pagerank_computed,
                graph.strings.len(),
                graph.csr.offsets.clone(),
                graph.csr.pending_edges.len()
            ),
            metadata
        );
    }
}

#[test]
fn refinalization_preserves_each_learned_bidirectional_slot_including_self_loops() {
    let mut graph = Graph::new();
    let a = node(&mut graph, "a");
    let b = node(&mut graph, "b");
    edge(&mut graph, b, a, "bidi", true);
    edge(&mut graph, a, a, "loop", true);
    graph.finalize().unwrap();
    learn(&mut graph);
    let expected = rows(&graph);
    node(&mut graph, "c");
    graph.finalize().unwrap();
    assert_eq!(rows(&graph), expected);
    assert_reverse_and_lengths(&graph);
}

#[test]
fn consecutive_removals_share_one_generation_and_preserve_later_add_order() {
    let mut graph = Graph::new();
    let a = node(&mut graph, "a");
    let b = node(&mut graph, "b");
    edge(&mut graph, a, b, "calls", false);
    edge(&mut graph, a, b, "imports", false);
    graph.finalize().unwrap();
    let generation = graph.generation;
    diff(vec![remove("a", "b", "calls"), remove("a", "b", "imports")])
        .apply(&mut graph)
        .unwrap();
    assert_eq!(graph.generation, generation.next());
    assert!(graph.finalized);
    assert_eq!(graph.num_edges(), 0);
    assert_reverse_and_lengths(&graph);

    let add = DiffAction::AddEdge(ExtractedEdge {
        source: "a".into(),
        target: "b".into(),
        relation: "calls".into(),
        weight: 0.5,
    });
    diff(vec![add.clone(), remove("a", "b", "calls"), add])
        .apply(&mut graph)
        .unwrap();
    assert!(
        !graph.finalized,
        "the add following a removal stays pending"
    );
    graph.finalize().unwrap();
    assert_eq!(graph.num_edges(), 1);
    assert_eq!(graph.strings.resolve(graph.csr.relations[0]), "calls");
    assert_reverse_and_lengths(&graph);
}

#[test]
fn restoration_reuses_id_updates_extracted_metadata_and_preserves_node_learning() {
    let mut graph = Graph::new();
    let id = node(&mut graph, "file::src/leaf.ts::fn::leaf");
    let stable = node(&mut graph, "stable");
    edge(&mut graph, stable, id, "keep", false);
    graph.nodes.activation[id.as_usize()][0] = FiniteF32::new(0.6);
    graph.nodes.plasticity[id.as_usize()].ceiling = FiniteF32::new(1.7);
    graph.finalize().unwrap();
    let before = rows(&graph);
    let delete = diff(vec![DiffAction::RemoveNode(
        "file::src/leaf.ts::fn::leaf".into(),
    )]);
    delete.apply(&mut graph).unwrap();
    assert!(graph.node_tags(id).contains(&"__m1nd_deleted"));
    let generation = graph.generation;
    delete.apply(&mut graph).unwrap();
    assert_eq!(
        graph.generation, generation,
        "repeated soft-delete is a no-op"
    );
    graph.finalize().unwrap();
    let restored = ExtractedNode {
        id: "file::src/leaf.ts::fn::leaf".into(),
        label: "restored".into(),
        node_type: NodeType::Class,
        tags: vec!["public".into()],
        line: 17,
        end_line: 23,
    };
    diff(vec![DiffAction::AddNode(restored.clone())])
        .apply(&mut graph)
        .unwrap();
    assert_eq!(graph.resolve_id(&restored.id), Some(id));
    assert_eq!(graph.num_nodes(), 2);
    assert_eq!(graph.node_tags(id), vec!["public"]);
    let i = id.as_usize();
    assert_eq!(graph.strings.resolve(graph.nodes.label[i]), "restored");
    assert_eq!(graph.nodes.node_type[i], NodeType::Class);
    assert_eq!(graph.nodes.last_modified[i], 17.0);
    let provenance = graph.resolve_node_provenance(id);
    assert_eq!(provenance.line_start, Some(17));
    assert_eq!(provenance.line_end, Some(23));
    assert_eq!(graph.nodes.activation[i][0].get(), 0.6);
    assert_eq!(graph.nodes.plasticity[i].ceiling.get(), 1.7);
    assert_eq!(graph.generation, generation.next());
    assert!(!graph.finalized);
    assert!(graph.pagerank_dirty);
    graph.finalize().unwrap();
    assert_eq!(rows(&graph), before);
    let generation = graph.generation;
    let mut duplicate = restored;
    duplicate.label = "must not replace an active node".into();
    diff(vec![DiffAction::AddNode(duplicate)])
        .apply(&mut graph)
        .unwrap();
    assert_eq!(graph.generation, generation);
    assert!(graph.finalized);
    assert_eq!(graph.strings.resolve(graph.nodes.label[i]), "restored");
    assert_eq!(graph.node_tags(stable), vec!["stable"]);
}
