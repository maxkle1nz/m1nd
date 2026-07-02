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

    // Drive deterministic couplings from alpha, simulating six co-change
    // events (commits): 3x {alpha,gamma}, 2x {alpha,beta}, 1x {alpha,delta}.
    // Each event notes both participants' appearances (the smoothed-Jaccard
    // marginal counts), then records the pair. More joint observations mean
    // a higher smoothed-Jaccard strength, so gamma (3x) ranks above beta
    // (2x) above delta (1x).
    for _ in 0..3 {
        matrix.note_node_appearance(alpha);
        matrix.note_node_appearance(gamma);
        matrix
            .record_co_change(alpha, gamma, 0.0)
            .expect("record alpha->gamma");
    }
    for _ in 0..2 {
        matrix.note_node_appearance(alpha);
        matrix.note_node_appearance(beta);
        matrix
            .record_co_change(alpha, beta, 0.0)
            .expect("record alpha->beta");
    }
    matrix.note_node_appearance(alpha);
    matrix.note_node_appearance(delta);
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

// ── Smoothed-Jaccard strength: deterministic oracles (hand-computed) ──
//
// strength = co / (count(A) + count(B) − co + 2), the additive smoothing
// keeping one-off coincidences from saturating at 1.0 while the union term
// punishes promiscuous files.

#[test]
fn smoothed_jaccard_punishes_promiscuity_and_damps_one_offs() {
    let (graph, ids) = isolated_graph(&[
        "file::core.rs",
        "file::exclusive.rs",
        "file::promiscuous.rs",
        "file::noise1.rs",
        "file::noise2.rs",
        "file::noise3.rs",
        "file::lonely1.rs",
        "file::lonely2.rs",
    ]);
    let (core, exclusive, promiscuous) = (ids[0], ids[1], ids[2]);
    let (lonely1, lonely2) = (ids[6], ids[7]);

    let mut matrix = CoChangeMatrix::bootstrap(&graph, 500_000).expect("bootstrap");

    // exclusive: 2 commits, both with core. promiscuous: 5 commits, 2 with
    // core and 3 with noise files. lonely1+lonely2: one joint commit only.
    let groups: Vec<Vec<String>> = vec![
        vec!["core.rs".into(), "exclusive.rs".into()],
        vec!["core.rs".into(), "exclusive.rs".into()],
        vec!["core.rs".into(), "promiscuous.rs".into()],
        vec!["core.rs".into(), "promiscuous.rs".into()],
        vec!["promiscuous.rs".into(), "noise1.rs".into()],
        vec!["promiscuous.rs".into(), "noise2.rs".into()],
        vec!["promiscuous.rs".into(), "noise3.rs".into()],
        vec!["lonely1.rs".into(), "lonely2.rs".into()],
    ];
    matrix
        .populate_from_commit_groups(&graph, &groups)
        .expect("populate");

    // core appears in 4 commits, exclusive in 2, promiscuous in 5.
    // J(core, exclusive)   = 2 / (4 + 2 − 2 + 2) = 2/6  EXACT
    // J(core, promiscuous) = 2 / (4 + 5 − 2 + 2) = 2/9  EXACT
    let ranked = matrix.predict(core, 10);
    let strength_of = |target| {
        ranked
            .iter()
            .find(|e| e.target == target)
            .map(|e| e.strength.get())
    };
    let s_exclusive = strength_of(exclusive).expect("exclusive must be predicted");
    let s_promiscuous = strength_of(promiscuous).expect("promiscuous must be predicted");
    assert!(
        (s_exclusive - 2.0 / 6.0).abs() < 1e-6,
        "J(core,exclusive) must be 2/6, got {s_exclusive}"
    );
    assert!(
        (s_promiscuous - 2.0 / 9.0).abs() < 1e-6,
        "J(core,promiscuous) must be 2/9, got {s_promiscuous}"
    );
    assert!(
        s_exclusive > s_promiscuous,
        "equal joint counts: the partner with the bigger union must score lower"
    );

    // One-off pair must NOT saturate at 1.0 (the raw-Jaccard failure mode the
    // smoothing exists to prevent): J = 1 / (1 + 1 − 1 + 2) = 1/3 EXACT.
    let lonely_ranked = matrix.predict(lonely1, 10);
    assert_eq!(lonely_ranked.len(), 1);
    assert_eq!(lonely_ranked[0].target, lonely2);
    assert!(
        (lonely_ranked[0].strength.get() - 1.0 / 3.0).abs() < 1e-6,
        "one-off pair must score 1/3, got {}",
        lonely_ranked[0].strength.get()
    );
    // The raw support is carried for consumers that filter on it.
    assert_eq!(lonely_ranked[0].co_count, 1);
}

#[test]
fn direct_record_without_appearances_stays_bounded() {
    // Callers that record pairs without noting appearances (the legacy direct
    // path) must still get a valid, bounded strength: the union saturates to
    // the joint count, so strength = co / (co + 2) — never 1.0, never a
    // degenerate denominator.
    let (graph, ids) = isolated_graph(&["file::a.rs", "file::b.rs"]);
    let (a, b) = (ids[0], ids[1]);

    let mut matrix = CoChangeMatrix::bootstrap(&graph, 500_000).expect("bootstrap");
    for _ in 0..3 {
        matrix.record_co_change(a, b, 0.0).expect("record a->b");
    }

    let ranked = matrix.predict(a, 10);
    assert_eq!(ranked.len(), 1);
    // 3 / (3 + 2) = 0.6 EXACT.
    assert!(
        (ranked[0].strength.get() - 0.6).abs() < 1e-6,
        "count-only evidence must score co/(co+2), got {}",
        ranked[0].strength.get()
    );
    assert_eq!(ranked[0].co_count, 3);
}
