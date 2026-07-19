//! Round-trip + error coverage for the JSON graph snapshot (`snapshot`).
//!
//! `snapshot::{save_graph, load_graph}` is the JSON persistence path production
//! actually defaults to (`graph_snapshot.json`, written by the m1nd daemon/ingest),
//! yet the JSON `save_graph`/`load_graph` pair had ZERO test coverage — only the
//! compact binary path (`snapshot_bin`) and plasticity state were locked. A silent
//! regression in the JSON format would corrupt the graph the engine reloads on
//! every restart.
//!
//! This locks the contracts the implementation claims:
//!
//! * a graph (nodes + edges + tags + provenance) survives save -> load with
//!   `num_nodes`/`num_edges`/labels/tags/provenance/edge-weights semantically
//!   intact;
//! * `load_graph` on a missing path returns `Err` (not panic);
//! * `load_graph` on malformed JSON returns `Err` (not panic).
//!
//! (Surfaced by the X-RAY proof-coverage pass.)

use m1nd_core::graph::{Graph, NodeProvenanceInput};
use m1nd_core::snapshot::{decode_graph_json, encode_graph_json, load_graph, save_graph};
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeId, NodeType};

/// Build a small but representative graph: two function nodes, distinctive tags,
/// provenance on one node, and a weighted directed edge between them.
fn sample_graph() -> Graph {
    let mut graph = Graph::new();
    let alpha = graph
        .add_node(
            "file::a.rs::fn::alpha",
            "alpha",
            NodeType::Function,
            &["rust", "rust:visibility:pub", "xray:state:bedrock"],
            1_718_000_000.0,
            0.5,
        )
        .unwrap();
    let beta = graph
        .add_node(
            "file::a.rs::fn::beta",
            "beta",
            NodeType::Struct,
            &["rust"],
            1_718_000_001.0,
            0.0,
        )
        .unwrap();
    graph.set_node_provenance(
        alpha,
        NodeProvenanceInput {
            source_path: Some("a.rs"),
            line_start: Some(3),
            line_end: Some(9),
            ..Default::default()
        },
    );
    graph
        .add_edge(
            alpha,
            beta,
            "calls",
            FiniteF32::new(0.75),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.25),
        )
        .unwrap();
    graph.finalize().unwrap();
    graph
}

/// Read the weight of the (single) outgoing edge of `node`, going through the same
/// public CSR surface `snapshot::save_graph` itself serializes from.
fn first_out_edge_weight(graph: &Graph, node: NodeId) -> f32 {
    let range = graph.csr.out_range(node);
    let idx = range.start;
    graph
        .csr
        .read_weight(m1nd_core::types::EdgeIdx::new(idx as u32))
        .get()
}

fn edge_slot(graph: &Graph, source: NodeId, target: NodeId) -> usize {
    graph
        .csr
        .out_range(source)
        .find(|&slot| graph.csr.targets[slot] == target)
        .expect("edge slot")
}

#[test]
fn json_snapshot_round_trips_nodes_edges_tags_provenance_weights() {
    let dir = std::env::temp_dir().join("m1nd_snapshot_json_rt");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("graph_snapshot.json");

    let original = sample_graph();
    save_graph(&original, &path).expect("json save_graph");
    assert!(
        std::fs::metadata(&path)
            .map(|m| m.len() > 0)
            .unwrap_or(false),
        "json snapshot file should be written and non-empty"
    );

    let loaded = load_graph(&path).expect("json load_graph");

    assert_eq!(
        loaded.num_nodes(),
        original.num_nodes(),
        "node count must survive the JSON round-trip"
    );
    assert_eq!(
        loaded.num_edges(),
        original.num_edges(),
        "edge count must survive the JSON round-trip"
    );

    // Node identity + label + tags must survive.
    let alpha = loaded
        .resolve_id("file::a.rs::fn::alpha")
        .expect("node external_id must survive");
    let alpha_label = loaded.strings.resolve(loaded.nodes.label[alpha.as_usize()]);
    assert_eq!(alpha_label, "alpha", "node label must survive");

    let tags = loaded.node_tags(alpha);
    assert!(
        tags.contains(&"xray:state:bedrock"),
        "tags must survive: {tags:?}"
    );
    assert!(
        tags.contains(&"rust:visibility:pub"),
        "all tags must survive: {tags:?}"
    );

    // Provenance must survive.
    let prov = loaded.resolve_node_provenance(alpha);
    assert_eq!(
        prov.line_start,
        Some(3),
        "provenance line_start must survive"
    );
    assert_eq!(prov.line_end, Some(9), "provenance line_end must survive");

    // Second node, with a distinct NodeType, must survive.
    let beta = loaded
        .resolve_id("file::a.rs::fn::beta")
        .expect("second node must survive");
    assert_eq!(
        loaded.nodes.node_type[beta.as_usize()],
        NodeType::Struct,
        "node_type must survive the JSON round-trip"
    );

    // Edge weight must survive semantically (load goes through add_edge + finalize).
    let weight = first_out_edge_weight(&loaded, alpha);
    assert!(
        (weight - 0.75).abs() < 1e-6,
        "edge weight must survive the JSON round-trip, got {weight}"
    );
}

#[test]
fn json_v4_restart_preserves_original_and_learned_weight_separately() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("learned.json");
    let mut graph = sample_graph();
    let alpha = graph.resolve_id("file::a.rs::fn::alpha").unwrap();
    let beta = graph.resolve_id("file::a.rs::fn::beta").unwrap();
    let slot = edge_slot(&graph, alpha, beta);
    graph.edge_plasticity.current_weight[slot] = FiniteF32::new(1.37);
    graph
        .csr
        .atomic_write_weight(
            m1nd_core::types::EdgeIdx::new(slot as u32),
            FiniteF32::new(1.37),
            64,
        )
        .unwrap();

    save_graph(&graph, &path).expect("save v4 learned graph");
    let restored = load_graph(&path).expect("restore v4 learned graph");
    let restored_alpha = restored.resolve_id("file::a.rs::fn::alpha").unwrap();
    let restored_beta = restored.resolve_id("file::a.rs::fn::beta").unwrap();
    let restored_slot = edge_slot(&restored, restored_alpha, restored_beta);
    assert_eq!(
        restored.edge_plasticity.original_weight[restored_slot].get(),
        0.75
    );
    assert_eq!(
        restored.edge_plasticity.current_weight[restored_slot].get(),
        1.37
    );
    assert_eq!(
        restored
            .csr
            .read_weight(m1nd_core::types::EdgeIdx::new(restored_slot as u32))
            .get(),
        1.37
    );
}

#[test]
fn detached_json_encode_decode_roundtrip_needs_no_filesystem() {
    let graph = sample_graph();
    let bytes = encode_graph_json(&graph).expect("encode detached candidate");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["version"],
        4
    );
    let restored = decode_graph_json(&bytes).expect("decode detached candidate");
    assert_eq!(restored.num_nodes(), graph.num_nodes());
    assert_eq!(restored.num_edges(), graph.num_edges());
    assert!(restored.resolve_id("file::a.rs::fn::alpha").is_some());
}

#[test]
fn json_v4_preserves_asymmetric_bidirectional_slots() {
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
        .atomic_write_weight(
            m1nd_core::types::EdgeIdx::new(forward as u32),
            FiniteF32::new(1.2),
            64,
        )
        .unwrap();
    graph
        .csr
        .atomic_write_weight(
            m1nd_core::types::EdgeIdx::new(reverse as u32),
            FiniteF32::new(0.3),
            64,
        )
        .unwrap();

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bidir.json");
    save_graph(&graph, &path).expect("save bidirectional graph");
    let restored = load_graph(&path).expect("restore bidirectional graph");
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
fn json_loader_migrates_v3_weight_as_both_original_and_current() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("legacy-v3.json");
    let legacy = serde_json::json!({
        "version": 3,
        "nodes": [
            {"external_id":"alpha","label":"alpha","node_type":2,"tags":[],"last_modified":0.0,"change_frequency":0.0},
            {"external_id":"beta","label":"beta","node_type":2,"tags":[],"last_modified":0.0,"change_frequency":0.0}
        ],
        "edges": [{
            "source_id":"alpha","target_id":"beta","relation":"calls","weight":0.66,
            "direction":0,"inhibitory":false,"causal_strength":0.1
        }]
    });
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let restored = load_graph(&path).expect("load legacy v3 JSON");
    let alpha = restored.resolve_id("alpha").unwrap();
    let beta = restored.resolve_id("beta").unwrap();
    let slot = edge_slot(&restored, alpha, beta);
    assert_eq!(restored.edge_plasticity.original_weight[slot].get(), 0.66);
    assert_eq!(restored.edge_plasticity.current_weight[slot].get(), 0.66);
}

#[test]
fn json_load_missing_file_is_error_not_panic() {
    let missing = std::env::temp_dir().join("m1nd_snapshot_json_missing_xyz.json");
    let _ = std::fs::remove_file(&missing);
    assert!(
        load_graph(&missing).is_err(),
        "loading a missing JSON snapshot must return Err, not panic"
    );
}

#[test]
fn json_load_malformed_is_error_not_panic() {
    let dir = std::env::temp_dir().join("m1nd_snapshot_json_malformed");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("garbage.json");
    std::fs::write(&path, b"{ this is not valid graph json ]]").unwrap();

    assert!(
        load_graph(&path).is_err(),
        "loading malformed JSON must return Err, not panic"
    );

    std::fs::remove_dir_all(&dir).ok();
}
