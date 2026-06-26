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
use m1nd_core::snapshot::{load_graph, save_graph};
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
