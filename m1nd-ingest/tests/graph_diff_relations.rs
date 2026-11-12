use m1nd_core::graph::Graph;
use m1nd_core::types::{EdgeDirection, EdgeIdx, FiniteF32, NodeId, NodeType};
use m1nd_ingest::diff::{DiffAction, GraphDiff};
use m1nd_ingest::extract::{ExtractedEdge, ExtractedNode};

fn nodes() -> Vec<ExtractedNode> {
    ["a", "b", "c"]
        .into_iter()
        .map(|id| ExtractedNode {
            id: id.into(),
            label: id.into(),
            node_type: NodeType::Function,
            tags: vec![],
            line: 1,
            end_line: 1,
        })
        .collect()
}

fn edge(source: &str, target: &str, relation: &str, weight: f32) -> ExtractedEdge {
    ExtractedEdge {
        source: source.into(),
        target: target.into(),
        relation: relation.into(),
        weight,
    }
}

fn build(edges: &[ExtractedEdge], finalized: bool) -> Graph {
    let mut graph = Graph::new();
    for n in nodes() {
        graph
            .add_node(&n.id, &n.label, n.node_type, &[], 1.0, 0.5)
            .unwrap();
    }
    for e in edges {
        graph
            .add_edge(
                graph.resolve_id(&e.source).unwrap(),
                graph.resolve_id(&e.target).unwrap(),
                &e.relation,
                FiniteF32::new(e.weight),
                EdgeDirection::Forward,
                false,
                FiniteF32::ZERO,
            )
            .unwrap();
    }
    if finalized {
        graph.finalize().unwrap();
    }
    graph
}

fn semantic_edges(graph: &Graph) -> Vec<(String, String, String, u32)> {
    let mut ids = vec![String::new(); graph.num_nodes() as usize];
    for (&id, &node) in &graph.id_to_node {
        ids[node.as_usize()] = graph.strings.resolve(id).into();
    }
    let mut edges = Vec::new();
    for source in 0..graph.num_nodes() {
        for j in graph.csr.out_range(NodeId::new(source)) {
            edges.push((
                ids[source as usize].clone(),
                ids[graph.csr.targets[j].as_usize()].clone(),
                graph.strings.resolve(graph.csr.relations[j]).into(),
                graph
                    .csr
                    .read_weight(EdgeIdx::new(j as u32))
                    .get()
                    .to_bits(),
            ));
        }
    }
    edges.sort();
    edges
}

fn one(action: DiffAction) -> GraphDiff {
    GraphDiff {
        actions: vec![action],
        nodes_added: 0,
        nodes_removed: 0,
        nodes_modified: 0,
        edges_added: 0,
        edges_removed: 0,
    }
}

fn snapshot_bytes(graph: &Graph) -> Vec<u8> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "m1nd-relation-bytes-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("graph.bin");
    m1nd_core::snapshot_bin::save_graph(graph, &path).unwrap();
    let bytes = std::fs::read(path).unwrap();
    std::fs::remove_dir_all(directory).unwrap();
    bytes
}

#[test]
fn removed_relation_matches_rebuild_and_keeps_parallel_relation_and_zero_weight_edge() {
    let old = vec![
        edge("a", "b", "calls", 0.7),
        edge("a", "b", "imports", 0.0),
        edge("b", "a", "calls", 0.8),
    ];
    let new = old[1..].to_vec();
    let mut graph = build(&old, true);
    let generation = graph.generation;
    let delta = GraphDiff::compute(&nodes(), &old, &nodes(), &new);
    assert_eq!(delta.apply(&mut graph).unwrap(), 1);
    assert_ne!(graph.generation, generation);
    graph.finalize().unwrap();
    assert_eq!(semantic_edges(&graph), semantic_edges(&build(&new, true)));
    assert_eq!(graph.csr.in_range(graph.resolve_id("b").unwrap()).len(), 1);
}

#[test]
fn weight_change_uses_relation_instead_of_first_endpoint_match() {
    let old = vec![edge("a", "b", "imports", 0.2), edge("a", "b", "calls", 0.7)];
    let new = vec![old[0].clone(), edge("a", "b", "calls", 0.0)];
    let mut graph = build(&old, true);
    GraphDiff::compute(&nodes(), &old, &nodes(), &new)
        .apply(&mut graph)
        .unwrap();
    graph.finalize().unwrap();
    assert_eq!(semantic_edges(&graph), semantic_edges(&build(&new, true)));
}

#[test]
fn pending_relation_removal_and_weight_change_match_rebuild() {
    let old = vec![edge("a", "b", "calls", 0.7), edge("a", "b", "imports", 0.4)];
    let new = vec![edge("a", "b", "imports", 0.0)];
    let mut graph = build(&old, false);
    GraphDiff::compute(&nodes(), &old, &nodes(), &new)
        .apply(&mut graph)
        .unwrap();
    graph.finalize().unwrap();
    assert_eq!(semantic_edges(&graph), semantic_edges(&build(&new, true)));
}

#[test]
fn removed_relation_is_idempotent_and_keeps_unrelated_duplicate_learning_state() {
    let old = vec![
        edge("a", "b", "calls", 0.7),
        edge("a", "b", "imports", 0.2),
        edge("a", "b", "imports", 0.4),
    ];
    let mut graph = build(&old, true);
    graph.edge_plasticity.strengthen_count[1] = 3;
    graph.edge_plasticity.strengthen_count[2] = 7;
    let delta = one(DiffAction::RemoveEdge {
        source_id: "a".into(),
        target_id: "b".into(),
        relation: "calls".into(),
    });
    assert_eq!(delta.apply(&mut graph).unwrap(), 1);
    graph.finalize().unwrap();
    let counts = graph.edge_plasticity.strengthen_count.clone();
    assert_eq!(counts, vec![3, 7]);
    let generation = graph.generation;
    assert_eq!(delta.apply(&mut graph).unwrap(), 0);
    assert_eq!(graph.generation, generation);
    graph.finalize().unwrap();
    assert_eq!(graph.edge_plasticity.strengthen_count, counts);
    assert_eq!(
        semantic_edges(&graph),
        semantic_edges(&build(&old[1..], true))
    );
}

#[test]
fn finalized_and_pending_edits_preserve_live_weights_and_learning_histories() {
    let old = vec![edge("a", "b", "calls", 0.7), edge("a", "b", "imports", 0.4)];
    let mut graph = build(&old, true);
    graph
        .csr
        .atomic_write_weight(EdgeIdx::new(1), FiniteF32::new(0.9), 10)
        .unwrap();
    graph.edge_plasticity.current_weight[1] = FiniteF32::new(0.9);
    graph.edge_plasticity.weaken_count[1] = 6;
    graph
        .add_edge(
            graph.resolve_id("b").unwrap(),
            graph.resolve_id("c").unwrap(),
            "calls",
            FiniteF32::new(0.8),
            EdgeDirection::Forward,
            false,
            FiniteF32::ZERO,
        )
        .unwrap();
    one(DiffAction::ModifyEdgeWeight {
        source_id: "b".into(),
        target_id: "c".into(),
        relation: "calls".into(),
        new_weight: 0.0,
    })
    .apply(&mut graph)
    .unwrap();
    one(DiffAction::RemoveEdge {
        source_id: "a".into(),
        target_id: "b".into(),
        relation: "calls".into(),
    })
    .apply(&mut graph)
    .unwrap();
    assert!(!graph.finalized);
    assert!(graph.pagerank_dirty);
    graph.finalize().unwrap();
    assert_eq!(
        semantic_edges(&graph),
        semantic_edges(&build(
            &[edge("a", "b", "imports", 0.9), edge("b", "c", "calls", 0.0)],
            true
        ))
    );
    assert_eq!(
        graph.edge_plasticity.original_weight[0],
        FiniteF32::new(0.4)
    );
    assert_eq!(graph.edge_plasticity.current_weight[0], FiniteF32::new(0.9));
    assert_eq!(graph.edge_plasticity.weaken_count[0], 6);
}

#[test]
fn bidirectional_removal_uses_either_endpoint_order_and_keeps_other_relations() {
    let mut graph = build(&[], true);
    let a = graph.resolve_id("a").unwrap();
    let b = graph.resolve_id("b").unwrap();
    graph
        .add_edge(
            a,
            b,
            "calls",
            FiniteF32::new(0.7),
            EdgeDirection::Bidirectional,
            false,
            FiniteF32::ZERO,
        )
        .unwrap();
    graph
        .add_edge(
            a,
            b,
            "imports",
            FiniteF32::new(0.4),
            EdgeDirection::Bidirectional,
            true,
            FiniteF32::new(0.3),
        )
        .unwrap();
    graph.finalize().unwrap();
    assert_eq!(graph.csr.num_edges(), 4);
    assert_eq!(
        one(DiffAction::RemoveEdge {
            source_id: "b".into(),
            target_id: "a".into(),
            relation: "calls".into()
        })
        .apply(&mut graph)
        .unwrap(),
        1
    );
    graph.finalize().unwrap();
    assert_eq!(graph.csr.num_edges(), 2);
    assert!(graph.csr.inhibitory.iter().all(|&value| value));
    assert!(graph
        .csr
        .causal_strengths
        .iter()
        .all(|&value| value == FiniteF32::new(0.3)));
    for relation in &graph.csr.relations {
        assert_eq!(graph.strings.resolve(*relation), "imports");
    }
    graph.finalize().unwrap();
    assert_eq!(graph.csr.num_edges(), 2);
}

#[test]
fn snapshots_after_removal_restore_only_current_topology_and_zero_weights() {
    use m1nd_core::snapshot_bin::{load_graph, save_graph};
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let directory = std::env::temp_dir().join(format!(
        "m1nd-relation-snapshot-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("graph.bin");
    let old = vec![edge("a", "b", "calls", 0.7), edge("a", "b", "imports", 0.4)];
    let new = vec![edge("a", "b", "imports", 0.0)];
    let mut graph = build(&old, true);
    GraphDiff::compute(&nodes(), &old, &nodes(), &new)
        .apply(&mut graph)
        .unwrap();
    assert!(
        save_graph(&graph, &path).is_err(),
        "unfinalized edge edits cannot be snapshotted"
    );
    graph.finalize().unwrap();
    save_graph(&graph, &path).unwrap();
    let mut restarted = load_graph(&path).unwrap();
    assert_eq!(
        semantic_edges(&restarted),
        semantic_edges(&build(&new, true))
    );
    // A subsequent epoch and restart must not resurrect the removed relation.
    restarted
        .add_edge(
            restarted.resolve_id("b").unwrap(),
            restarted.resolve_id("c").unwrap(),
            "calls",
            FiniteF32::new(0.5),
            EdgeDirection::Forward,
            false,
            FiniteF32::ZERO,
        )
        .unwrap();
    restarted.finalize().unwrap();
    save_graph(&restarted, &path).unwrap();
    assert_eq!(
        semantic_edges(&load_graph(&path).unwrap()),
        semantic_edges(&build(
            &[new[0].clone(), edge("b", "c", "calls", 0.5)],
            true
        ))
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn consecutive_unique_triple_deltas_match_full_rebuild_topology_and_pagerank() {
    let mut old = Vec::new();
    let mut graph = build(&old, true);
    for epoch in 0..8 {
        let mut new = Vec::new();
        for (i, source) in ["a", "b", "c"].iter().enumerate() {
            for (j, target) in ["a", "b", "c"].iter().enumerate() {
                for (k, relation) in ["calls", "imports"].iter().enumerate() {
                    if (i + j + k + epoch) % 3 != 0 {
                        new.push(edge(
                            source,
                            target,
                            relation,
                            ((i + j + k + epoch) % 5) as f32 / 4.0,
                        ));
                    }
                }
            }
        }
        GraphDiff::compute(&nodes(), &old, &nodes(), &new)
            .apply(&mut graph)
            .unwrap();
        graph.finalize().unwrap();
        let rebuilt = build(&new, true);
        assert_eq!(
            semantic_edges(&graph),
            semantic_edges(&rebuilt),
            "epoch {epoch}"
        );
        for i in 0..3 {
            assert_eq!(
                graph.csr.in_range(NodeId::new(i)).len(),
                rebuilt.csr.in_range(NodeId::new(i)).len()
            );
            assert!(
                (graph.nodes.pagerank[i as usize].get() - rebuilt.nodes.pagerank[i as usize].get())
                    .abs()
                    < 0.000001
            );
        }
        old = new;
    }
}

#[test]
fn unrelated_bidirectional_self_loop_is_not_duplicated_by_relation_removal() {
    let mut graph = build(&[edge("a", "b", "calls", 0.7)], true);
    let a = graph.resolve_id("a").unwrap();
    graph
        .add_edge(
            a,
            a,
            "imports",
            FiniteF32::new(0.4),
            EdgeDirection::Bidirectional,
            false,
            FiniteF32::ZERO,
        )
        .unwrap();
    graph.finalize().unwrap();
    assert_eq!(graph.csr.num_edges(), 3);
    for j in graph.csr.out_range(a) {
        if graph.csr.targets[j] == a {
            graph.edge_plasticity.strengthen_count[j] = 9;
            graph.edge_plasticity.last_used_query[j] = 42;
        }
    }
    one(DiffAction::RemoveEdge {
        source_id: "a".into(),
        target_id: "b".into(),
        relation: "calls".into(),
    })
    .apply(&mut graph)
    .unwrap();
    graph.finalize().unwrap();
    assert_eq!(
        graph.csr.num_edges(),
        2,
        "an unrelated self-loop remains one logical pair"
    );
    assert_eq!(graph.edge_plasticity.strengthen_count, vec![9, 9]);
    assert_eq!(graph.edge_plasticity.last_used_query, vec![42, 42]);
    assert_eq!(
        graph
            .set_relation_weight(a, a, "imports", FiniteF32::ZERO)
            .unwrap(),
        1
    );
    graph.finalize().unwrap();
    assert_eq!(graph.csr.num_edges(), 2);
    assert!(graph
        .csr
        .weights
        .iter()
        .enumerate()
        .all(|(j, _)| graph.csr.read_weight(EdgeIdx::new(j as u32)) == FiniteF32::ZERO));
    assert_eq!(graph.edge_plasticity.strengthen_count, vec![9, 9]);
    let generation = graph.generation;
    assert_eq!(
        graph
            .set_relation_weight(a, a, "imports", FiniteF32::ZERO)
            .unwrap(),
        0
    );
    assert_eq!(graph.generation, generation);
    assert!(graph.finalized);
}

#[test]
fn asymmetric_self_loop_refuses_before_any_batch_or_direct_helper_mutation() {
    let mut graph = build(&[edge("a", "b", "calls", 0.7)], true);
    let a = graph.resolve_id("a").unwrap();
    graph
        .add_edge(
            a,
            a,
            "imports",
            FiniteF32::new(0.4),
            EdgeDirection::Bidirectional,
            false,
            FiniteF32::ZERO,
        )
        .unwrap();
    graph.finalize().unwrap();
    let index = graph
        .csr
        .out_range(a)
        .find(|&j| graph.csr.targets[j] == a)
        .unwrap();
    graph.edge_plasticity.strengthen_count[index] = 7;
    let before_edges = semantic_edges(&graph);
    let before_counts = graph.edge_plasticity.strengthen_count.clone();
    let before_original = graph.edge_plasticity.original_weight.clone();
    let before_current = graph.edge_plasticity.current_weight.clone();
    let before_generation = graph.generation;
    let before_strings = graph.strings.len();
    let before_snapshot = snapshot_bytes(&graph);
    let engine = m1nd_core::plasticity::PlasticityEngine::new(&graph, Default::default());
    let before_learning = serde_json::to_vec(&engine.export_state(&graph).unwrap()).unwrap();
    let mut delta = one(DiffAction::RemoveEdge {
        source_id: "a".into(),
        target_id: "b".into(),
        relation: "calls".into(),
    });
    delta.actions.insert(
        0,
        DiffAction::AddNode(ExtractedNode {
            id: "must-not-be-added".into(),
            label: "new".into(),
            node_type: NodeType::Function,
            tags: vec![],
            line: 1,
            end_line: 1,
        }),
    );
    assert!(delta
        .apply(&mut graph)
        .unwrap_err()
        .to_string()
        .contains("ambiguous bidirectional self-loop"));
    assert!(graph.resolve_id("must-not-be-added").is_none());
    assert!(graph
        .set_relation_weight(a, a, "imports", FiniteF32::ZERO)
        .is_err());
    assert!(graph
        .remove_relation_edges(a, graph.resolve_id("b").unwrap(), "calls")
        .is_err());
    assert_eq!(snapshot_bytes(&graph), before_snapshot);
    assert_eq!(
        serde_json::to_vec(&engine.export_state(&graph).unwrap()).unwrap(),
        before_learning
    );
    assert_eq!(semantic_edges(&graph), before_edges);
    assert_eq!(graph.edge_plasticity.strengthen_count, before_counts);
    assert_eq!(graph.edge_plasticity.original_weight, before_original);
    assert_eq!(graph.edge_plasticity.current_weight, before_current);
    assert_eq!(graph.generation, before_generation);
    assert_eq!(graph.strings.len(), before_strings);
    assert!(graph.finalized);
    assert!(!graph.pagerank_dirty);
    assert!(graph.csr.pending_edges.is_empty());
}

#[test]
fn asymmetric_reverse_learning_refuses_unrelated_relation_edits_without_loss() {
    let mut graph = build(&[edge("b", "c", "calls", 0.7)], true);
    let a = graph.resolve_id("a").unwrap();
    let b = graph.resolve_id("b").unwrap();
    let c = graph.resolve_id("c").unwrap();
    graph
        .add_edge(
            a,
            b,
            "imports",
            FiniteF32::new(0.4),
            EdgeDirection::Bidirectional,
            false,
            FiniteF32::ZERO,
        )
        .unwrap();
    graph.finalize().unwrap();
    let reverse = graph
        .csr
        .out_range(b)
        .find(|&j| graph.csr.targets[j] == a)
        .unwrap();
    graph
        .csr
        .atomic_write_weight(EdgeIdx::new(reverse as u32), FiniteF32::new(0.9), 10)
        .unwrap();
    graph.edge_plasticity.current_weight[reverse] = FiniteF32::new(0.9);
    graph.edge_plasticity.strengthen_count[reverse] = 8;
    let before_edges = semantic_edges(&graph);
    let before_relation: Vec<_> = before_edges
        .iter()
        .filter(|row| row.2 == "imports")
        .cloned()
        .collect();
    let before_snapshot = snapshot_bytes(&graph);
    let engine = m1nd_core::plasticity::PlasticityEngine::new(&graph, Default::default());
    let before_learning = serde_json::to_vec(&engine.export_state(&graph).unwrap()).unwrap();
    let before_generation = graph.generation;
    let before_strings = graph.strings.len();
    assert_eq!(graph.remove_relation_edges(a, c, "imports").unwrap(), 0);
    assert_eq!(
        graph
            .set_relation_weight(a, c, "imports", FiniteF32::ZERO)
            .unwrap(),
        0
    );
    assert_eq!(
        graph
            .set_relation_weight(b, c, "calls", FiniteF32::new(0.7))
            .unwrap(),
        0
    );
    assert_eq!(graph.generation, before_generation);
    let mut delta = one(DiffAction::RemoveEdge {
        source_id: "b".into(),
        target_id: "c".into(),
        relation: "calls".into(),
    });
    delta.actions.insert(
        0,
        DiffAction::AddNode(ExtractedNode {
            id: "must-not-precede-relation-error".into(),
            label: "new".into(),
            node_type: NodeType::Function,
            tags: vec![],
            line: 1,
            end_line: 1,
        }),
    );
    let result = delta.apply(&mut graph);
    if result.is_ok() {
        graph.finalize().unwrap();
    }
    let relation: Vec<_> = semantic_edges(&graph)
        .into_iter()
        .filter(|row| row.2 == "imports")
        .collect();
    assert_eq!(
        relation, before_relation,
        "unrelated reverse learned weight must not be replaced by the canonical slot"
    );
    assert!(result.unwrap_err().to_string().contains("bidirectional"));
    assert!(graph
        .resolve_id("must-not-precede-relation-error")
        .is_none());
    assert!(graph.remove_relation_edges(b, c, "calls").is_err());
    assert!(graph
        .set_relation_weight(a, b, "imports", FiniteF32::ZERO)
        .is_err());
    assert_eq!(semantic_edges(&graph), before_edges);
    assert_eq!(snapshot_bytes(&graph), before_snapshot);
    assert_eq!(
        serde_json::to_vec(&engine.export_state(&graph).unwrap()).unwrap(),
        before_learning
    );
    assert_eq!(graph.generation, before_generation);
    assert_eq!(graph.strings.len(), before_strings);
    assert!(graph.finalized);
    assert!(!graph.pagerank_dirty);
}

#[test]
fn reverse_counter_only_divergence_refuses_even_when_live_weights_are_equal() {
    let mut graph = build(&[edge("b", "c", "calls", 0.7)], true);
    let a = graph.resolve_id("a").unwrap();
    let b = graph.resolve_id("b").unwrap();
    let c = graph.resolve_id("c").unwrap();
    graph
        .add_edge(
            a,
            b,
            "imports",
            FiniteF32::new(0.4),
            EdgeDirection::Bidirectional,
            false,
            FiniteF32::ZERO,
        )
        .unwrap();
    graph.finalize().unwrap();
    let reverse = graph
        .csr
        .out_range(b)
        .find(|&j| graph.csr.targets[j] == a)
        .unwrap();
    graph.edge_plasticity.strengthen_count[reverse] = 13;
    let engine = m1nd_core::plasticity::PlasticityEngine::new(&graph, Default::default());
    let before_learning = serde_json::to_vec(&engine.export_state(&graph).unwrap()).unwrap();
    let before_snapshot = snapshot_bytes(&graph);
    let generation = graph.generation;
    assert!(graph
        .remove_relation_edges(b, c, "calls")
        .unwrap_err()
        .to_string()
        .contains("bidirectional reverse state"));
    assert_eq!(snapshot_bytes(&graph), before_snapshot);
    assert_eq!(
        serde_json::to_vec(&engine.export_state(&graph).unwrap()).unwrap(),
        before_learning
    );
    assert_eq!(graph.generation, generation);
    assert!(graph.finalized);
    assert!(graph.csr.pending_edges.is_empty());
}
