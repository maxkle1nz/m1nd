//! Behavioral lock for the query-memory priming heuristic in `plasticity.rs`.
//!
//! `QueryMemory::get_priming_signal` / `top_node_frequencies` (re-exported by
//! `PlasticityEngine::get_priming` / `top_node_access_frequencies`) are the
//! cheap "what has this agent been paying attention to" signal that the MCP
//! tool layer (`m1nd-mcp/.../tools.rs`) relies on to bias activation toward
//! frequently co-activated nodes. The math is subtle (seed-sharing gate,
//! self-exclusion, max-normalization, 0.01 floor, 50/`n` caps) and had no
//! direct behavioral coverage, so a quiet regression would silently degrade
//! priming without failing any other test.
//!
//! This locks the contracts the implementation explicitly claims:
//!
//! * a node that co-activates with a seed is boosted; a never-seen node is not;
//! * every emitted signal is finite and bounded to `(0.01, 1.0]`;
//! * `top_node_frequencies(n)` returns at most `n` entries, sorted by
//!   frequency descending, and only nodes that were actually accessed;
//! * empty seeds and no-shared-seed queries yield no priming.
//!
//! (Surfaced by the X-RAY proof-coverage pass.)

use m1nd_core::plasticity::{QueryMemory, QueryRecord};
use m1nd_core::types::{FiniteF32, NodeId};

/// Build a `QueryRecord` from raw u32 ids; timestamp is irrelevant to priming.
fn record(query_text: &str, seeds: &[u32], activated: &[u32]) -> QueryRecord {
    QueryRecord {
        query_text: query_text.to_string(),
        seeds: seeds.iter().copied().map(NodeId::new).collect(),
        activated_nodes: activated.iter().copied().map(NodeId::new).collect(),
        timestamp: 0.0,
    }
}

/// Co-activated neighbours are boosted; never-seen nodes are absent entirely.
#[test]
fn priming_boosts_co_activated_more_than_never_seen() {
    let mut memory = QueryMemory::new(16, 64);

    // Seed `alpha` (id 1) repeatedly co-activates `gamma` (id 3) and, less
    // often, `delta` (id 4). Node `omega` (id 50) never appears anywhere.
    memory.record(record("q1", &[1], &[1, 3, 4]));
    memory.record(record("q2", &[1], &[1, 3]));
    memory.record(record("q3", &[1], &[1, 3]));

    let signal = memory.get_priming_signal(&[NodeId::new(1)], FiniteF32::new(0.5));

    let score_of = |id: u32| {
        signal
            .iter()
            .find(|(n, _)| *n == NodeId::new(id))
            .map(|(_, s)| s.get())
    };

    let gamma = score_of(3).expect("frequently co-activated node must be primed");
    let delta = score_of(4).expect("occasionally co-activated node must be primed");

    // The most frequent co-activation outranks the rarer one.
    assert!(
        gamma > delta,
        "frequently co-activated node should outrank rarer one: gamma={gamma} delta={delta}"
    );

    // A node never observed in any query gets no priming at all.
    assert!(
        score_of(50).is_none(),
        "never-seen node must not appear in the priming signal"
    );

    // The seed itself is excluded from its own priming signal.
    assert!(
        score_of(1).is_none(),
        "seed node must be excluded from its own priming signal"
    );
}

/// Every emitted signal is finite and bounded to `(0.01, 1.0]`, and the top
/// entry equals the requested boost strength after max-normalization.
#[test]
fn priming_signals_are_finite_and_bounded() {
    let mut memory = QueryMemory::new(16, 64);
    memory.record(record("q1", &[1], &[1, 3, 4, 5]));
    memory.record(record("q2", &[1], &[1, 3, 4]));
    memory.record(record("q3", &[1], &[1, 3]));

    let boost = FiniteF32::new(0.7);
    let signal = memory.get_priming_signal(&[NodeId::new(1)], boost);

    assert!(
        !signal.is_empty(),
        "shared-seed query with co-activations must produce a non-empty signal"
    );

    for (node, score) in &signal {
        let value = score.get();
        assert!(
            value.is_finite(),
            "priming score for {node:?} must be finite, got {value}"
        );
        // Implementation filters scores <= 0.01 and caps at 1.0.
        assert!(
            value > 0.01 && value <= 1.0,
            "priming score for {node:?} must lie in (0.01, 1.0], got {value}"
        );
    }

    // Results are sorted by score descending, and the max-normalized top score
    // equals the boost strength (the most-co-activated node hits the ceiling).
    let top = signal[0].1.get();
    assert!(
        (top - boost.get()).abs() < 1e-6,
        "max-normalized top priming score should equal boost strength {}, got {top}",
        boost.get()
    );
    for window in signal.windows(2) {
        assert!(
            window[0].1 >= window[1].1,
            "priming signal must be sorted by score descending"
        );
    }
}

/// Boost strengths above 1.0 are clamped: no signal can exceed 1.0.
#[test]
fn priming_clamps_excessive_boost_to_one() {
    let mut memory = QueryMemory::new(8, 32);
    memory.record(record("q1", &[1], &[1, 2]));
    memory.record(record("q2", &[1], &[1, 2]));

    let signal = memory.get_priming_signal(&[NodeId::new(1)], FiniteF32::new(5.0));
    assert!(
        !signal.is_empty(),
        "expected a primed neighbour for the shared seed"
    );
    for (node, score) in &signal {
        assert!(
            score.get() <= 1.0,
            "priming score for {node:?} must be clamped to <= 1.0, got {}",
            score.get()
        );
    }
}

/// Empty seeds and queries that share no seed produce no priming.
#[test]
fn priming_requires_shared_seeds() {
    let mut memory = QueryMemory::new(8, 32);
    memory.record(record("q1", &[1], &[1, 3, 4]));

    // No seeds at all -> empty.
    assert!(
        memory
            .get_priming_signal(&[], FiniteF32::new(0.5))
            .is_empty(),
        "empty seed set must yield no priming"
    );

    // A seed that never co-occurred with any recorded seed -> empty.
    assert!(
        memory
            .get_priming_signal(&[NodeId::new(99)], FiniteF32::new(0.5))
            .is_empty(),
        "a seed sharing nothing with recorded queries must yield no priming"
    );
}

/// `top_node_frequencies(n)` returns at most `n` entries, sorted by frequency
/// descending, containing only nodes that were actually accessed.
#[test]
fn top_node_frequencies_are_capped_and_sorted() {
    let mut memory = QueryMemory::new(32, 64);

    // Node 2 appears in 4 records, node 3 in 3, node 4 in 2, node 5 in 1.
    // Node 9 is never activated.
    memory.record(record("q1", &[1], &[2, 3, 4, 5]));
    memory.record(record("q2", &[1], &[2, 3, 4]));
    memory.record(record("q3", &[1], &[2, 3]));
    memory.record(record("q4", &[1], &[2]));

    // Cap is honoured: at most `n` entries even though 4 distinct nodes exist.
    let top2 = memory.top_node_frequencies(2);
    assert_eq!(
        top2.len(),
        2,
        "top_node_frequencies(2) must return at most 2 entries, got {}",
        top2.len()
    );

    // Full ordering: descending by frequency.
    let all = memory.top_node_frequencies(100);
    for window in all.windows(2) {
        assert!(
            window[0].1 >= window[1].1,
            "frequencies must be sorted descending: {:?} before {:?}",
            window[0],
            window[1]
        );
    }

    // The two highest-frequency nodes are exactly 2 (freq 4) then 3 (freq 3).
    assert_eq!(
        top2[0],
        (NodeId::new(2), 4),
        "most-accessed node must be node 2 with frequency 4"
    );
    assert_eq!(
        top2[1],
        (NodeId::new(3), 3),
        "second-most-accessed node must be node 3 with frequency 3"
    );

    // Only accessed nodes appear; the never-activated node 9 is absent.
    assert!(
        all.iter()
            .all(|(node, freq)| *freq > 0 && *node != NodeId::new(9)),
        "top_node_frequencies must exclude nodes with zero accesses"
    );
}

/// The ring buffer evicts the oldest record at capacity and decrements its
/// frequency contribution, so stale accesses stop influencing the top list.
#[test]
fn ring_buffer_eviction_decays_old_frequencies() {
    // Capacity 2: the third record overwrites the first.
    let mut memory = QueryMemory::new(2, 32);
    memory.record(record("q1", &[1], &[7])); // node 7 accessed once
    memory.record(record("q2", &[1], &[8])); // node 8 accessed once
    memory.record(record("q3", &[1], &[8])); // overwrites q1, node 7 evicted

    let freqs = memory.top_node_frequencies(10);
    let freq_of = |id: u32| {
        freqs
            .iter()
            .find(|(n, _)| *n == NodeId::new(id))
            .map(|(_, f)| *f)
    };

    assert_eq!(
        freq_of(7),
        None,
        "evicted record's node should drop out of the frequency list"
    );
    assert_eq!(
        freq_of(8),
        Some(2),
        "node 8 was accessed in both surviving records, frequency should be 2"
    );
    assert_eq!(
        memory.len(),
        2,
        "ring buffer at capacity 2 must hold exactly 2 live records"
    );
}
