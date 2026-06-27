//! X-RAY write-path proof: in-place bulk tag mutation (`xray.retag`) is correct,
//! idempotent, and survives a snapshot round-trip — the guarantee the commit
//! path relies on. Tags are a cold-path node column, so mutation must not need a
//! graph rebuild and must persist through save -> load.

use m1nd_core::graph::Graph;
use m1nd_core::snapshot::{load_graph, save_graph};
use m1nd_core::types::NodeType;

fn sample_graph() -> Graph {
    let mut g = Graph::new();
    g.add_node(
        "file::a.rs::fn::foo",
        "foo",
        NodeType::Function,
        &["rust", "rust:visibility:private"],
        0.0,
        0.0,
    )
    .unwrap();
    g.add_node(
        "file::a.rs::fn::bar",
        "bar",
        NodeType::Function,
        &["rust", "rust:visibility:pub"],
        0.0,
        0.0,
    )
    .unwrap();
    g.finalize().unwrap();
    g
}

#[test]
fn add_tags_is_idempotent_and_resolves() {
    let mut g = sample_graph();
    let n = g.resolve_id("file::a.rs::fn::foo").unwrap();

    // first add mutates one tag, second add is a no-op (idempotent)
    assert_eq!(g.add_node_tags(n, &["xray:overgrowth-candidate"]), 1);
    assert_eq!(g.add_node_tags(n, &["xray:overgrowth-candidate"]), 0);

    let tags = g.node_tags(n);
    assert!(tags.contains(&"xray:overgrowth-candidate"));
    assert!(tags.contains(&"rust:visibility:private"));
}

#[test]
fn remove_tags_counts_only_present() {
    let mut g = sample_graph();
    let n = g.resolve_id("file::a.rs::fn::bar").unwrap();

    // one present, one never existed -> exactly one removed
    assert_eq!(
        g.remove_node_tags(n, &["rust:visibility:pub", "never-existed"]),
        1
    );
    assert!(!g.node_tags(n).contains(&"rust:visibility:pub"));
}

#[test]
fn set_tags_replaces_whole_set() {
    let mut g = sample_graph();
    let n = g.resolve_id("file::a.rs::fn::foo").unwrap();

    assert_eq!(g.set_node_tags(n, &["only", "these"]), 2);
    let mut tags = g.node_tags(n);
    tags.sort();
    assert_eq!(tags, vec!["only", "these"]);
}

#[test]
fn out_of_range_node_is_a_safe_noop() {
    let mut g = sample_graph();
    let ghost = m1nd_core::types::NodeId::new(9999);
    assert_eq!(g.add_node_tags(ghost, &["x"]), 0);
    assert_eq!(g.remove_node_tags(ghost, &["x"]), 0);
    assert_eq!(g.set_node_tags(ghost, &["x"]), 0);
    assert!(g.node_tags(ghost).is_empty());
}

#[test]
fn bulk_retag_survives_snapshot_round_trip() {
    let dir = std::env::temp_dir().join("xray_tag_mutation_roundtrip");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("roundtrip.json");

    let mut g = sample_graph();
    // bulk: tag BOTH nodes with a proof-state tag, then persist
    for ext in ["file::a.rs::fn::foo", "file::a.rs::fn::bar"] {
        let n = g.resolve_id(ext).unwrap();
        g.add_node_tags(n, &["xray:bedrock"]);
    }
    save_graph(&g, &path).unwrap();

    let reloaded = load_graph(&path).unwrap();
    let n = reloaded.resolve_id("file::a.rs::fn::foo").unwrap();
    assert!(
        reloaded.node_tags(n).contains(&"xray:bedrock"),
        "mutated tag must survive save -> load (xray.retag commit guarantee)"
    );

    std::fs::remove_dir_all(&dir).ok();
}
