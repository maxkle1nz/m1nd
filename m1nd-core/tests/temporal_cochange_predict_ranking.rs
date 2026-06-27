//! Locks the ranking/exclusion invariant of `temporal::CoChangeMatrix::predict`
//! (surfaced by the X-RAY coverage sweep): predictions are sorted by coupling
//! strength descending, capped at `k`, never include the queried node itself,
//! and an un-coupled node returns an empty Vec instead of panicking.

use m1nd_core::builder::GraphBuilder;
use m1nd_core::temporal::CoChangeMatrix;
use m1nd_core::types::{NodeId, NodeType};

/// Build a graph of isolated file nodes (no edges) so `bootstrap` yields empty
/// rows. This isolates the prediction logic from the structural BFS seed, making
/// the learned co-change observations the sole source of predictions.
fn isolated_graph(node_ids: &[&str]) -> (m1nd_core::graph::Graph, Vec<NodeId>) {
    let mut builder = GraphBuilder::new();
    let mut ids = Vec::new();
    for ext in node_ids {
        let nid = builder
            .add_node(ext, ext, NodeType::File, &[])
            .expect("add_node");
        ids.push(nid);
    }
    let graph = builder.finalize().expect("finalize graph");
    (graph, ids)
}

#[test]
fn predict_is_sorted_descending_excludes_self_and_caps_at_k() {
    let (graph, ids) = isolated_graph(&[
        "file::alpha.rs",
        "file::beta.rs",
        "file::gamma.rs",
        "file::delta.rs",
    ]);
    let (alpha, beta, gamma, delta) = (ids[0], ids[1], ids[2], ids[3]);

    let mut matrix = CoChangeMatrix::bootstrap(&graph, 500_000).expect("bootstrap");

    // No edges -> bootstrap learned nothing structural for these nodes.
    assert!(
        matrix.predict(alpha, 10).is_empty(),
        "fresh bootstrap on an edgeless graph must yield no co-change partners"
    );

    // Drive deterministic couplings from alpha. record_co_change uses a 0.1 base
    // strength and strengthens existing pairs by +0.1, so repeating an
    // observation makes that partner rank higher.
    // gamma observed 3x (strongest), beta 2x, delta 1x (weakest).
    for _ in 0..3 {
        matrix
            .record_co_change(alpha, gamma, 0.0)
            .expect("record alpha->gamma");
    }
    for _ in 0..2 {
        matrix
            .record_co_change(alpha, beta, 0.0)
            .expect("record alpha->beta");
    }
    matrix
        .record_co_change(alpha, delta, 0.0)
        .expect("record alpha->delta");

    let ranked = matrix.predict(alpha, 10);
    assert_eq!(
        ranked.len(),
        3,
        "all three distinct partners should be returned when k exceeds the row size"
    );

    // Invariant 1: sorted by strength descending.
    for pair in ranked.windows(2) {
        assert!(
            pair[0].strength >= pair[1].strength,
            "predict output must be sorted by coupling strength descending"
        );
    }

    // Invariant 2: ranking matches observation frequency (gamma > beta > delta).
    assert_eq!(
        ranked[0].target, gamma,
        "the most-frequently co-changed partner must rank first"
    );
    assert_eq!(
        ranked[2].target, delta,
        "the least-frequently co-changed partner must rank last"
    );

    // Invariant 3: the queried node does not appear among its own predictions.
    // NOTE: this holds because the observations never record a self co-change
    // (alpha->alpha); `predict` itself does not filter self, so this asserts the
    // realistic-data behavior, not a defensive guard in the engine.
    assert!(
        ranked.iter().all(|entry| entry.target != alpha),
        "predict should not surface the queried node as its own co-change partner"
    );

    // Invariant 4: top_k truncates the result.
    let top1 = matrix.predict(alpha, 1);
    assert_eq!(top1.len(), 1, "predict must return at most k entries");
    assert_eq!(
        top1[0].target, gamma,
        "the single returned entry must be the strongest partner"
    );
}

#[test]
fn predict_on_never_cochanged_node_returns_empty_without_panic() {
    let (graph, ids) = isolated_graph(&["file::alpha.rs", "file::beta.rs"]);
    let (alpha, beta) = (ids[0], ids[1]);

    let mut matrix = CoChangeMatrix::bootstrap(&graph, 500_000).expect("bootstrap");
    matrix
        .record_co_change(alpha, beta, 0.0)
        .expect("record alpha->beta");

    // beta was only ever a *target*, never a source: its row is empty.
    assert!(
        matrix.predict(beta, 10).is_empty(),
        "a node that was never a co-change source must predict nothing"
    );

    // An out-of-bounds NodeId must return empty rather than panic.
    let out_of_bounds = NodeId::new(9_999);
    assert!(
        matrix.predict(out_of_bounds, 10).is_empty(),
        "predicting on an out-of-range node must return empty, not panic"
    );

    // top_k of zero is a degenerate but valid request: it returns nothing.
    assert!(
        matrix.predict(alpha, 0).is_empty(),
        "predict with k=0 must return an empty Vec"
    );
}

#[test]
fn populate_from_commit_groups_resolves_ids_and_feeds_predictions() {
    let (graph, ids) = isolated_graph(&["file::alpha.rs", "file::beta.rs", "file::gamma.rs"]);
    let (alpha, beta, gamma) = (ids[0], ids[1], ids[2]);

    let mut matrix = CoChangeMatrix::bootstrap(&graph, 500_000).expect("bootstrap");

    // Two commits: the first couples all three files, the second couples
    // alpha+beta again. The group form accepts bare paths (the "file::" prefix
    // is added internally) and resolves them to NodeIds via the graph.
    let commit_groups = vec![
        vec![
            "alpha.rs".to_string(),
            "beta.rs".to_string(),
            "gamma.rs".to_string(),
        ],
        vec!["alpha.rs".to_string(), "beta.rs".to_string()],
    ];
    matrix
        .populate_from_commit_groups(&graph, &commit_groups)
        .expect("populate_from_commit_groups");

    assert!(
        matrix.num_entries() > 0,
        "populating from commit groups must create co-change entries"
    );

    let ranked = matrix.predict(alpha, 10);
    let targets: Vec<NodeId> = ranked.iter().map(|entry| entry.target).collect();
    assert!(
        targets.contains(&beta) && targets.contains(&gamma),
        "alpha's predictions must include both files it co-changed with"
    );
    assert!(
        ranked.iter().all(|entry| entry.target != alpha),
        "self-exclusion must hold for commit-group-derived predictions"
    );

    // beta co-changed with alpha twice but gamma only once, so beta should
    // rank alpha above gamma in beta's row.
    let beta_ranked = matrix.predict(beta, 10);
    assert!(
        !beta_ranked.is_empty(),
        "beta participated in commits and must have predictions"
    );
    let beta_top = beta_ranked[0].target;
    assert_eq!(
        beta_top, alpha,
        "the partner co-changed most often must rank first for beta"
    );
}
