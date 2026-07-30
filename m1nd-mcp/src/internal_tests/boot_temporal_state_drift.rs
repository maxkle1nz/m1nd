//! A co-change sidecar bound to a DIFFERENT graph must not kill the boot.
//!
//! `temporal_state_v1.json` is one sidecar among many on the startup path, and
//! every other one degrades ("continuing without it") when it no longer matches
//! the loaded graph. This one used to abort `SessionState::initialize`, which
//! takes the whole MCP server — all its tools — down with it, and the only
//! recovery was deleting the file by hand.
//!
//! The binding check itself is correct and stays: a matrix whose rows are
//! indexed by another graph's nodes would predict nonsense. What changes is the
//! consequence — the stale matrix is dropped and relearned, loudly.

use crate::server::McpConfig;
use crate::session::SessionState;
use crate::temporal_state::{save_temporal_state, TEMPORAL_STATE_FILE};
use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_core::temporal::CoChangeMatrix;
use m1nd_core::types::{NodeId, NodeType};
use std::path::Path;

fn graph_of(node_count: usize, prefix: &str) -> Graph {
    let mut graph = Graph::new();
    for index in 0..node_count {
        let id = format!("{prefix}_{index}");
        graph
            .add_node(&id, &id, NodeType::File, &[], 0.0, 0.0)
            .expect("add node");
    }
    graph.finalize().expect("finalize");
    graph
}

fn learned_matrix(graph: &Graph) -> CoChangeMatrix {
    let mut matrix = CoChangeMatrix::bootstrap(graph, 128).expect("bootstrap");
    for _ in 0..4 {
        matrix.note_node_appearance(NodeId::new(0));
        matrix.note_node_appearance(NodeId::new(1));
        matrix
            .record_co_change(NodeId::new(0), NodeId::new(1), 0.0)
            .expect("learn");
    }
    matrix
}

fn config_for(root: &Path) -> McpConfig {
    McpConfig {
        graph_source: root.join("graph_snapshot.json"),
        plasticity_state: root.join("plasticity_state.json"),
        runtime_dir: Some(root.to_path_buf()),
        registry_dir: Some(root.join("registry")),
        ..McpConfig::default()
    }
}

#[test]
fn boot_survives_a_co_change_sidecar_bound_to_another_graph() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let written_for = graph_of(3, "alpha");
    let matrix = learned_matrix(&written_for);
    save_temporal_state(
        &root.join(TEMPORAL_STATE_FILE),
        &written_for,
        &matrix,
        &matrix,
    )
    .expect("save sidecar bound to graph A");

    // Boot against a different graph: same shape, different node identities.
    let loaded = graph_of(3, "beta");
    let state = SessionState::initialize(loaded, &config_for(root), DomainConfig::code())
        .expect("a stale co-change sidecar must not abort the boot");

    assert_eq!(
        state.temporal.co_change.num_entries(),
        0,
        "the stale matrix must be dropped, not bound to the new graph"
    );
    assert_eq!(
        state.orchestrator.temporal.co_change.num_entries(),
        0,
        "the orchestrator fallback matrix must be dropped too"
    );
    assert!(
        state
            .temporal
            .co_change
            .predict(NodeId::new(0), 8)
            .is_empty(),
        "a dropped matrix must predict nothing until it is relearned"
    );
}

#[test]
fn boot_still_adopts_a_co_change_sidecar_bound_to_the_loaded_graph() {
    // The degrade must not become a shrug: a sidecar that DOES match is still
    // adopted exactly, otherwise every restart would silently forget.
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let graph = graph_of(3, "alpha");
    let matrix = learned_matrix(&graph);
    let expected = matrix.predict(NodeId::new(0), 8);
    assert!(!expected.is_empty(), "fixture must have learned something");
    save_temporal_state(&root.join(TEMPORAL_STATE_FILE), &graph, &matrix, &matrix)
        .expect("save sidecar");

    let state = SessionState::initialize(
        graph_of(3, "alpha"),
        &config_for(root),
        DomainConfig::code(),
    )
    .expect("boot");
    assert_eq!(
        state.temporal.co_change.predict(NodeId::new(0), 8),
        expected
    );
    assert_eq!(
        state
            .orchestrator
            .temporal
            .co_change
            .predict(NodeId::new(0), 8),
        expected
    );
}
