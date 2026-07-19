//! Round-trip coverage for the BINARY graph snapshot (`snapshot_bin`).
//!
//! `snapshot_bin::{save_graph, load_graph}` is the compact binary persistence
//! path used in production by `m1nd-mcp` (persist_handlers), yet it had ZERO test
//! coverage — a silent corruption in the binary format would ship undetected.
//! This proves a graph survives save -> load with nodes, edges, tags, and
//! provenance intact. (Surfaced by the X-RAY proof-coverage pass.)

use m1nd_core::graph::{Graph, NodeProvenanceInput};
use m1nd_core::snapshot_bin;
use m1nd_core::types::{EdgeDirection, EdgeIdx, FiniteF32, NodeId, NodeType};

fn edge_slot(graph: &Graph, source: NodeId, target: NodeId) -> usize {
    graph
        .csr
        .out_range(source)
        .find(|&slot| graph.csr.targets[slot] == target)
        .expect("edge slot")
}

fn sample_graph() -> Graph {
    let mut g = Graph::new();
    let node_a = g
        .add_node(
            "file::a.rs::fn::alpha",
            "alpha",
            NodeType::Function,
            &["rust", "rust:visibility:pub", "xray:state:bedrock"],
            1718000000.0,
            0.5,
        )
        .unwrap();
    let node_b = g
        .add_node(
            "file::a.rs::fn::beta",
            "beta",
            NodeType::Function,
            &["rust"],
            1718000001.0,
            0.0,
        )
        .unwrap();
    g.set_node_provenance(
        node_a,
        NodeProvenanceInput {
            source_path: Some("a.rs"),
            line_start: Some(3),
            line_end: Some(9),
            ..Default::default()
        },
    );
    g.add_edge(
        node_a,
        node_b,
        "calls",
        FiniteF32::new(1.0),
        EdgeDirection::Forward,
        false,
        FiniteF32::new(0.0),
    )
    .unwrap();
    g.finalize().unwrap();
    g
}

#[test]
fn binary_snapshot_round_trips_nodes_edges_tags_provenance() {
    let dir = std::env::temp_dir().join("m1nd_snapshot_bin_rt");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("graph.bin");

    let g = sample_graph();
    snapshot_bin::save_graph(&g, &path).expect("binary save");
    assert!(
        std::fs::metadata(&path)
            .map(|m| m.len() > 0)
            .unwrap_or(false),
        "binary snapshot file should be written and non-empty"
    );

    let r = snapshot_bin::load_graph(&path).expect("binary load");

    assert_eq!(r.num_nodes(), g.num_nodes(), "node count must survive");
    assert_eq!(r.num_edges(), g.num_edges(), "edge count must survive");

    let alpha = r
        .resolve_id("file::a.rs::fn::alpha")
        .expect("node external_id must survive");
    let tags = r.node_tags(alpha);
    assert!(
        tags.contains(&"xray:state:bedrock"),
        "tags must survive: {tags:?}"
    );
    assert!(tags.contains(&"rust:visibility:pub"));

    let prov = r.resolve_node_provenance(alpha);
    assert_eq!(
        prov.line_start,
        Some(3),
        "provenance line_start must survive"
    );
    assert_eq!(prov.line_end, Some(9), "provenance line_end must survive");

    assert!(
        r.resolve_id("file::a.rs::fn::beta").is_some(),
        "second node must survive"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn binary_v4_restart_preserves_original_and_learned_weight_separately() {
    let mut graph = sample_graph();
    let alpha = graph.resolve_id("file::a.rs::fn::alpha").unwrap();
    let beta = graph.resolve_id("file::a.rs::fn::beta").unwrap();
    let slot = edge_slot(&graph, alpha, beta);
    graph.edge_plasticity.current_weight[slot] = FiniteF32::new(1.7);
    graph
        .csr
        .atomic_write_weight(EdgeIdx::new(slot as u32), FiniteF32::new(1.7), 64)
        .unwrap();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("learned.bin");
    snapshot_bin::save_graph(&graph, &path).unwrap();
    let restored = snapshot_bin::load_graph(&path).unwrap();
    let restored_alpha = restored.resolve_id("file::a.rs::fn::alpha").unwrap();
    let restored_beta = restored.resolve_id("file::a.rs::fn::beta").unwrap();
    let restored_slot = edge_slot(&restored, restored_alpha, restored_beta);
    assert_eq!(
        restored.edge_plasticity.original_weight[restored_slot].get(),
        1.0
    );
    assert_eq!(
        restored.edge_plasticity.current_weight[restored_slot].get(),
        1.7
    );
}

#[test]
fn binary_v4_preserves_asymmetric_bidirectional_slots() {
    let mut graph = Graph::new();
    let alpha = graph
        .add_node("alpha", "alpha", NodeType::Function, &[], 0.0, 0.0)
        .unwrap();
    let beta = graph
        .add_node("beta", "beta", NodeType::Function, &[], 0.0, 0.0)
        .unwrap();
    graph
        .add_edge(
            alpha,
            beta,
            "related",
            FiniteF32::new(0.4),
            EdgeDirection::Bidirectional,
            false,
            FiniteF32::new(0.2),
        )
        .unwrap();
    graph.finalize().unwrap();
    let forward = edge_slot(&graph, alpha, beta);
    let reverse = edge_slot(&graph, beta, alpha);
    graph.edge_plasticity.original_weight[forward] = FiniteF32::new(0.4);
    graph.edge_plasticity.original_weight[reverse] = FiniteF32::new(0.6);
    graph.edge_plasticity.current_weight[forward] = FiniteF32::new(1.2);
    graph.edge_plasticity.current_weight[reverse] = FiniteF32::new(0.3);
    graph
        .csr
        .atomic_write_weight(EdgeIdx::new(forward as u32), FiniteF32::new(1.2), 64)
        .unwrap();
    graph
        .csr
        .atomic_write_weight(EdgeIdx::new(reverse as u32), FiniteF32::new(0.3), 64)
        .unwrap();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bidir.bin");
    snapshot_bin::save_graph(&graph, &path).unwrap();
    let restored = snapshot_bin::load_graph(&path).unwrap();
    let restored_alpha = restored.resolve_id("alpha").unwrap();
    let restored_beta = restored.resolve_id("beta").unwrap();
    let restored_forward = edge_slot(&restored, restored_alpha, restored_beta);
    let restored_reverse = edge_slot(&restored, restored_beta, restored_alpha);
    assert_eq!(
        restored.edge_plasticity.original_weight[restored_forward].get(),
        0.4
    );
    assert_eq!(
        restored.edge_plasticity.original_weight[restored_reverse].get(),
        0.6
    );
    assert_eq!(
        restored.edge_plasticity.current_weight[restored_forward].get(),
        1.2
    );
    assert_eq!(
        restored.edge_plasticity.current_weight[restored_reverse].get(),
        0.3
    );
}

#[test]
fn binary_loader_uses_explicit_v3_layout_fallback() {
    #[derive(serde::Serialize)]
    struct LegacyGraph {
        version: u32,
        nodes: Vec<LegacyNode>,
        edges: Vec<LegacyEdge>,
    }
    #[derive(serde::Serialize)]
    struct LegacyNode {
        external_id: String,
        label: String,
        node_type: u8,
        tags: Vec<String>,
        last_modified: f64,
        change_frequency: f32,
        provenance: LegacyProvenance,
    }
    #[derive(Default, serde::Serialize)]
    struct LegacyProvenance {
        source_path: Option<String>,
        line_start: Option<u32>,
        line_end: Option<u32>,
        excerpt: Option<String>,
        namespace: Option<String>,
        canonical: bool,
    }
    #[derive(serde::Serialize)]
    struct LegacyEdge {
        source_id: String,
        target_id: String,
        relation: String,
        weight: f32,
        direction: u8,
        inhibitory: bool,
        causal_strength: f32,
    }
    let node = |id: &str| LegacyNode {
        external_id: id.into(),
        label: id.into(),
        node_type: 2,
        tags: vec![],
        last_modified: 0.0,
        change_frequency: 0.0,
        provenance: LegacyProvenance::default(),
    };
    let legacy = LegacyGraph {
        version: 3,
        nodes: vec![node("alpha"), node("beta")],
        edges: vec![LegacyEdge {
            source_id: "alpha".into(),
            target_id: "beta".into(),
            relation: "calls".into(),
            weight: 0.66,
            direction: 0,
            inhibitory: false,
            causal_strength: 0.1,
        }],
    };
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("legacy-v3.bin");
    std::fs::write(&path, bincode::serialize(&legacy).unwrap()).unwrap();
    let restored = snapshot_bin::load_graph(&path).expect("load explicit v3 layout");
    let alpha = restored.resolve_id("alpha").unwrap();
    let beta = restored.resolve_id("beta").unwrap();
    let slot = edge_slot(&restored, alpha, beta);
    assert_eq!(restored.edge_plasticity.original_weight[slot].get(), 0.66);
    assert_eq!(restored.edge_plasticity.current_weight[slot].get(), 0.66);
}

#[test]
fn binary_load_missing_file_is_error_not_panic() {
    let missing = std::env::temp_dir().join("m1nd_snapshot_bin_missing_xyz.bin");
    let _ = std::fs::remove_file(&missing);
    assert!(
        snapshot_bin::load_graph(&missing).is_err(),
        "loading a missing binary snapshot must return Err, not panic"
    );
}
