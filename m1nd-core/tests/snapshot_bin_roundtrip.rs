//! Round-trip coverage for the BINARY graph snapshot (`snapshot_bin`).
//!
//! `snapshot_bin::{save_graph, load_graph}` is the compact binary persistence
//! path used in production by `m1nd-mcp` (persist_handlers), yet it had ZERO test
//! coverage — a silent corruption in the binary format would ship undetected.
//! This proves a graph survives save -> load with nodes, edges, tags, and
//! provenance intact. (Surfaced by the X-RAY proof-coverage pass.)

use m1nd_core::graph::{Graph, NodeProvenanceInput};
use m1nd_core::snapshot_bin;
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};

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
fn binary_load_missing_file_is_error_not_panic() {
    let missing = std::env::temp_dir().join("m1nd_snapshot_bin_missing_xyz.bin");
    let _ = std::fs::remove_file(&missing);
    assert!(
        snapshot_bin::load_graph(&missing).is_err(),
        "loading a missing binary snapshot must return Err, not panic"
    );
}
