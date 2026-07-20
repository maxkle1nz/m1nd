//! Round-trip + invariant coverage for plasticity-state persistence.
//!
//! `snapshot::{save_plasticity_state, load_plasticity_state}` is the production
//! autosave path for learned synaptic weights (m1nd-mcp session.rs +
//! persist_handlers.rs). It had ZERO test coverage — a silent regression here
//! would make the engine quietly forget everything it learned across restarts.
//!
//! This locks two contracts the implementation explicitly claims:
//!   * the full `SynapticState` survives save -> load unchanged;
//!   * FM-PL-001 — a non-finite `current_weight` is sanitized to
//!     `original_weight` at the save boundary (so a poisoned weight can never be
//!     written to disk and re-loaded as NaN).
//!
//! (Surfaced by the X-RAY proof-coverage pass.)

use m1nd_core::graph::Graph;
use m1nd_core::plasticity::{PlasticityConfig, PlasticityEngine, SynapticState};
use m1nd_core::snapshot::{load_plasticity_state, save_plasticity_state};
use m1nd_core::types::{EdgeDirection, EdgeIdx, FiniteF32, NodeId, NodeType};

fn state(source: &str, target: &str, current: f32) -> SynapticState {
    SynapticState {
        source_label: source.to_string(),
        target_label: target.to_string(),
        relation: "calls".to_string(),
        direction: Some(0),
        inhibitory: Some(false),
        original_weight: 0.5,
        current_weight: current,
        strengthen_count: 3,
        weaken_count: 1,
        ltp_applied: true,
        ltd_applied: false,
        last_used_query: 17,
    }
}

fn parallel_graph() -> Graph {
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
            "calls",
            FiniteF32::new(0.5),
            EdgeDirection::Forward,
            false,
            FiniteF32::ZERO,
        )
        .unwrap();
    graph
        .add_edge(
            alpha,
            beta,
            "calls",
            FiniteF32::new(0.7),
            EdgeDirection::Forward,
            true,
            FiniteF32::ZERO,
        )
        .unwrap();
    graph.finalize().unwrap();
    graph
}

fn edge_slot(graph: &Graph, source: NodeId, target: NodeId, inhibitory: bool) -> usize {
    graph
        .csr
        .out_range(source)
        .find(|&slot| graph.csr.targets[slot] == target && graph.csr.inhibitory[slot] == inhibitory)
        .expect("edge slot")
}

#[test]
fn plasticity_state_round_trips_all_fields() {
    let dir = std::env::temp_dir().join("m1nd_plasticity_rt");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plasticity.json");

    let original = vec![state("alpha", "beta", 0.81), state("beta", "gamma", 0.42)];
    save_plasticity_state(&original, &path).expect("save plasticity state");

    let loaded = load_plasticity_state(&path).expect("load plasticity state");
    assert_eq!(loaded.len(), original.len(), "state count must survive");

    for (a, b) in original.iter().zip(loaded.iter()) {
        assert_eq!(a.source_label, b.source_label);
        assert_eq!(a.target_label, b.target_label);
        assert_eq!(a.relation, b.relation);
        assert_eq!(a.direction, b.direction);
        assert_eq!(a.inhibitory, b.inhibitory);
        assert_eq!(a.original_weight, b.original_weight);
        assert_eq!(
            a.current_weight, b.current_weight,
            "learned weight must survive"
        );
        assert_eq!(a.strengthen_count, b.strengthen_count);
        assert_eq!(a.weaken_count, b.weaken_count);
        assert_eq!(a.ltp_applied, b.ltp_applied);
        assert_eq!(a.ltd_applied, b.ltd_applied);
        assert_eq!(a.last_used_query, b.last_used_query);
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn save_sanitizes_non_finite_current_weight_to_original() {
    // FM-PL-001: a poisoned (NaN) current_weight must never reach disk; the save
    // boundary rewrites it to original_weight so load returns a finite value.
    let dir = std::env::temp_dir().join("m1nd_plasticity_nan_firewall");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plasticity.json");

    let poisoned = vec![state("alpha", "beta", f32::NAN)];
    save_plasticity_state(&poisoned, &path).expect("save must not fail on NaN");

    let loaded = load_plasticity_state(&path).expect("load sanitized state");
    assert_eq!(loaded.len(), 1);
    assert!(
        loaded[0].current_weight.is_finite(),
        "NaN current_weight must be sanitized at the save boundary"
    );
    assert_eq!(
        loaded[0].current_weight, loaded[0].original_weight,
        "sanitized weight must fall back to original_weight"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn load_missing_plasticity_file_is_error_not_panic() {
    let missing = std::env::temp_dir().join("m1nd_plasticity_missing_xyz.json");
    let _ = std::fs::remove_file(&missing);
    assert!(
        load_plasticity_state(&missing).is_err(),
        "loading a missing plasticity file must return Err, not panic"
    );
}

#[test]
fn learned_sidecar_restart_restores_baseline_current_counters_and_last_use() {
    let source_graph = parallel_graph();
    let source_engine = PlasticityEngine::new(&source_graph, PlasticityConfig::default());
    let mut states = source_engine.export_state(&source_graph).unwrap();
    let learned = states
        .iter_mut()
        .find(|entry| entry.inhibitory == Some(false))
        .unwrap();
    learned.original_weight = 0.02;
    learned.current_weight = 0.01;
    learned.strengthen_count = 91;
    learned.weaken_count = 37;
    learned.ltp_applied = true;
    learned.ltd_applied = true;
    learned.last_used_query = 4_242;

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("learned-sidecar.json");
    save_plasticity_state(&states, &path).unwrap();
    let persisted = load_plasticity_state(&path).unwrap();

    let mut restored_graph = parallel_graph();
    let mut restored_engine = PlasticityEngine::new(&restored_graph, PlasticityConfig::default());
    assert_eq!(
        restored_engine
            .import_state(&mut restored_graph, &persisted)
            .unwrap(),
        2
    );
    let alpha = restored_graph.resolve_id("alpha").unwrap();
    let beta = restored_graph.resolve_id("beta").unwrap();
    let slot = edge_slot(&restored_graph, alpha, beta, false);
    assert_eq!(
        restored_graph.edge_plasticity.original_weight[slot].get(),
        0.02,
        "original baseline must be restored exactly, not replaced by current"
    );
    assert_eq!(
        restored_graph.edge_plasticity.current_weight[slot].get(),
        0.01,
        "persisted current weight must not be clamped during restore"
    );
    assert_eq!(
        restored_graph
            .csr
            .read_weight(EdgeIdx::new(slot as u32))
            .get(),
        0.01
    );
    assert_eq!(restored_graph.edge_plasticity.strengthen_count[slot], 91);
    assert_eq!(restored_graph.edge_plasticity.weaken_count[slot], 37);
    assert!(restored_graph.edge_plasticity.ltp_applied[slot]);
    assert!(restored_graph.edge_plasticity.ltd_applied[slot]);
    assert_eq!(restored_graph.edge_plasticity.last_used_query[slot], 4_242);
}

#[test]
fn complete_identity_disambiguates_parallel_edges() {
    let source_graph = parallel_graph();
    let source_engine = PlasticityEngine::new(&source_graph, PlasticityConfig::default());
    let mut states = source_engine.export_state(&source_graph).unwrap();
    for state in &mut states {
        if state.inhibitory == Some(false) {
            state.original_weight = 0.11;
            state.current_weight = 0.21;
            state.last_used_query = 11;
        } else {
            state.original_weight = 0.72;
            state.current_weight = 1.72;
            state.last_used_query = 72;
        }
    }

    let mut restored = parallel_graph();
    let mut engine = PlasticityEngine::new(&restored, PlasticityConfig::default());
    assert_eq!(engine.import_state(&mut restored, &states).unwrap(), 2);
    let alpha = restored.resolve_id("alpha").unwrap();
    let beta = restored.resolve_id("beta").unwrap();
    let excitatory = edge_slot(&restored, alpha, beta, false);
    let inhibitory = edge_slot(&restored, alpha, beta, true);
    assert_eq!(
        restored.edge_plasticity.current_weight[excitatory].get(),
        0.21
    );
    assert_eq!(
        restored.edge_plasticity.current_weight[inhibitory].get(),
        1.72
    );
    assert_eq!(restored.edge_plasticity.last_used_query[excitatory], 11);
    assert_eq!(restored.edge_plasticity.last_used_query[inhibitory], 72);
}

#[test]
fn legacy_triple_only_sidecar_fails_on_ambiguous_parallel_edges() {
    let mut graph = parallel_graph();
    let mut engine = PlasticityEngine::new(&graph, PlasticityConfig::default());
    let legacy = SynapticState {
        direction: None,
        inhibitory: None,
        ..state("alpha", "beta", 0.9)
    };
    let before: Vec<f32> = graph
        .edge_plasticity
        .current_weight
        .iter()
        .map(|weight| weight.get())
        .collect();
    let error = engine.import_state(&mut graph, &[legacy]).unwrap_err();
    assert!(error.to_string().contains("legacy triple-only"));
    assert_eq!(
        graph
            .edge_plasticity
            .current_weight
            .iter()
            .map(|weight| weight.get())
            .collect::<Vec<_>>(),
        before,
        "an ambiguous legacy sidecar must mutate nothing"
    );
}

#[test]
fn legacy_triple_only_sidecar_migrates_when_unique() {
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
            "calls",
            FiniteF32::new(0.5),
            EdgeDirection::Forward,
            false,
            FiniteF32::ZERO,
        )
        .unwrap();
    graph.finalize().unwrap();
    let legacy = SynapticState {
        direction: None,
        inhibitory: None,
        original_weight: 0.31,
        current_weight: 1.31,
        strengthen_count: 8,
        weaken_count: 2,
        last_used_query: 99,
        ..state("alpha", "beta", 1.31)
    };
    let mut engine = PlasticityEngine::new(&graph, PlasticityConfig::default());
    assert_eq!(engine.import_state(&mut graph, &[legacy]).unwrap(), 1);
    let slot = edge_slot(&graph, alpha, beta, false);
    assert_eq!(graph.edge_plasticity.original_weight[slot].get(), 0.31);
    assert_eq!(graph.edge_plasticity.current_weight[slot].get(), 1.31);
    assert_eq!(graph.edge_plasticity.strengthen_count[slot], 8);
    assert_eq!(graph.edge_plasticity.weaken_count[slot], 2);
    assert_eq!(graph.edge_plasticity.last_used_query[slot], 99);
}

#[test]
fn duplicate_full_keys_are_rejected_before_any_mutation() {
    let mut graph = parallel_graph();
    let engine = PlasticityEngine::new(&graph, PlasticityConfig::default());
    let exported = engine.export_state(&graph).unwrap();
    let duplicate = exported
        .iter()
        .find(|entry| entry.inhibitory == Some(false))
        .unwrap()
        .clone();
    let mut conflicting = duplicate.clone();
    conflicting.current_weight = 2.5;
    let before: Vec<f32> = graph
        .edge_plasticity
        .current_weight
        .iter()
        .map(|weight| weight.get())
        .collect();
    let mut importer = PlasticityEngine::new(&graph, PlasticityConfig::default());
    let error = importer
        .import_state(&mut graph, &[duplicate, conflicting])
        .unwrap_err();
    assert!(error.to_string().contains("duplicate full synaptic key"));
    assert_eq!(
        graph
            .edge_plasticity
            .current_weight
            .iter()
            .map(|weight| weight.get())
            .collect::<Vec<_>>(),
        before
    );
}
