use std::path::PathBuf;

use m1nd_mcp::organism_manifest::{ensure_graph_authority_basis_stable, ManifestCaptureSeed};

fn seed() -> ManifestCaptureSeed {
    ManifestCaptureSeed {
        observed_at: 42,
        project_root_candidate: Some(PathBuf::from("/repo")),
        runtime_root: PathBuf::from("/runtime"),
        graph_path: PathBuf::from("/runtime/graph.json"),
        owner_id: "owner:test".into(),
        started_at: 1,
        graph_generation: 7,
        last_persist_offset_ns: None,
        node_count: 10,
        edge_count: 20,
    }
}

#[test]
fn graph_authority_occ_accepts_an_unchanged_observation_window() {
    let before = seed();
    assert!(ensure_graph_authority_basis_stable(&before, &before).is_ok());
}

#[test]
fn graph_authority_occ_refuses_generation_count_or_persist_drift() {
    let before = seed();
    let mut mutated = before.clone();
    mutated.graph_generation += 1;
    mutated.node_count += 1;
    let detail = ensure_graph_authority_basis_stable(&before, &mutated)
        .expect_err("a graph mutation during hashing must refuse the manifest");
    assert!(detail.contains("generation 7 -> 8"), "got: {detail}");
    assert!(detail.contains("nodes 10 -> 11"), "got: {detail}");

    let mut persisted = before.clone();
    persisted.last_persist_offset_ns = Some(1);
    let detail = ensure_graph_authority_basis_stable(&before, &persisted)
        .expect_err("a graph persist during hashing must refuse the manifest");
    assert!(
        detail.contains("persisted_during_observation=true"),
        "got: {detail}"
    );
}
