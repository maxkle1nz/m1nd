//! Proves the DomainConfig per-NodeType half-life table is now LIVE inside
//! `activation::activate_temporal`. Before Move 5 the temporal kernel hardcoded
//! a single 168h (7-day) half-life for every node, so the declared retention
//! schedule in `DomainConfig` (`half_life_for`) was dead code: a File node and a
//! Module node decayed identically. This locks two properties:
//!
//! * DIFFERENT TYPES DECAY DIFFERENTLY — a Module (720h half-life in the code
//!   profile) retains more recency than a File (168h) at the same age; the table
//!   is genuinely consumed.
//! * NO REGRESSION FOR THE DEFAULT — a File (168h == the old hardcoded constant)
//!   still produces the exact recency the old `168.0 * 3600.0` formula gave, so
//!   the fix is a no-op for any type whose declared half-life is already 7 days.

use m1nd_core::activation::activate_temporal;
use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_core::types::{FiniteF32, NodeId, NodeType, TemporalWeights};

/// Age (in seconds) applied to every seed node in these tests: 14 days. Large
/// enough that the half-life difference between File (168h) and Module (720h)
/// produces a clearly separated recency, small enough to stay well away from
/// float underflow.
const AGE_SECS: f64 = 14.0 * 24.0 * 3600.0;

/// Recency-only weights (frequency zeroed) so the assertions read the pure decay
/// term without the frequency contribution muddying the score.
fn recency_only_weights() -> TemporalWeights {
    TemporalWeights {
        recency: FiniteF32::new(1.0),
        frequency: FiniteF32::new(0.0),
    }
}

/// Build a graph whose nodes all sit at `now - AGE_SECS`, with `change_frequency`
/// zeroed so only the recency term survives the recency-only weights. Node types
/// are supplied by the caller; returns the graph and the `NodeId`s in order.
fn graph_with_types(types: &[(&str, NodeType)]) -> (Graph, Vec<NodeId>) {
    let mut graph = Graph::new();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let last_mod = now - AGE_SECS;
    let mut ids = Vec::new();
    for (id, node_type) in types {
        let node = graph
            .add_node(id, id, *node_type, &[], last_mod, 0.0)
            .expect("add node");
        ids.push(node);
    }
    graph.finalize().expect("finalize");
    (graph, ids)
}

/// Look up the temporal score `activate_temporal` produced for `node`.
fn score_for(graph: &Graph, node: NodeId, domain: &DomainConfig) -> f32 {
    let seeds = vec![(node, FiniteF32::new(1.0))];
    let result = activate_temporal(graph, &seeds, &recency_only_weights(), domain)
        .expect("activate_temporal");
    result
        .scores
        .iter()
        .find(|(n, _)| *n == node)
        .map(|(_, s)| s.get())
        .expect("node scored")
}

#[test]
fn longer_half_life_type_decays_slower_than_shorter() {
    // In the code profile: File = 168h, Module = 720h. Same age -> the Module
    // (longer half-life) must retain strictly MORE recency than the File. If the
    // table were still dead (both 168h), the two would be equal.
    let domain = DomainConfig::code();
    let (graph, ids) = graph_with_types(&[("f", NodeType::File), ("m", NodeType::Module)]);

    let file_score = score_for(&graph, ids[0], &domain);
    let module_score = score_for(&graph, ids[1], &domain);

    assert!(
        module_score > file_score,
        "Module (720h half-life) must decay slower than File (168h): \
         module={module_score} file={file_score}"
    );
}

#[test]
fn default_half_life_type_matches_old_hardcoded_constant() {
    // File carries a 168h half-life in the code profile — exactly the constant
    // the old kernel hardcoded for EVERY node. So a File must still produce the
    // recency the old `168.0 * 3600.0` formula gave: this proves the fix is a
    // no-op for the default and does not regress ranking for 7-day types.
    let domain = DomainConfig::code();
    let (graph, ids) = graph_with_types(&[("f", NodeType::File)]);

    let got = score_for(&graph, ids[0], &domain);

    // Re-derive the pre-fix recency for the same 14-day age.
    let old_half_life_secs = 168.0 * 3600.0_f64;
    let k = std::f64::consts::LN_2 / old_half_life_secs;
    let expected = (-k * AGE_SECS).exp() as f32;

    assert!(
        (got - expected).abs() < 1e-6,
        "File (168h) recency must equal the old hardcoded-constant recency: \
         got={got} expected={expected}"
    );
}
