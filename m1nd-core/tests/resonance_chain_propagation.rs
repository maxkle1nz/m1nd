//! Regression: a standing wave must propagate along a linear chain past the
//! second hop.
//!
//! `StandingWavePropagator::propagate` classified ANY node with out-degree 1 (at
//! hops > 0) as a dead-end and reflected the wave, even when that single edge led
//! forward to a brand-new node. On a linear chain 0->1->2->...->N the pulse
//! reached node 1 and then bounced between 0 and 1 forever — nodes 2..N never
//! received amplitude, so the wave died at the second hop on ANY chain-like
//! topology (the common case in real code graphs).
//!
//! A node is a true dead-end only when it has no outgoing edge, or its single
//! outgoing edge points back to where the pulse came from. This locks forward
//! transmission along a chain while leaving genuine dead-end reflection intact.
//!
//! (Surfaced by the X-RAY adversarial bug hunt; proven failing-then-passing.)

use m1nd_core::graph::Graph;
use m1nd_core::resonance::StandingWavePropagator;
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeId, NodeType, PosF32};

/// Directed chain 0 -> 1 -> 2 -> ... -> (n-1), each node a single forward edge.
fn linear_chain(n: usize) -> Graph {
    let mut g = Graph::new();
    for i in 0..n {
        g.add_node(
            &format!("chain_{i}"),
            &format!("step_{i}"),
            NodeType::Function,
            &["chain"],
            0.0,
            0.1,
        )
        .unwrap();
    }
    for i in 0..(n - 1) {
        g.add_edge(
            NodeId::new(i as u32),
            NodeId::new((i + 1) as u32),
            "calls",
            FiniteF32::new(0.9),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.5),
        )
        .unwrap();
    }
    g.finalize().unwrap();
    g
}

#[test]
fn wave_propagates_along_a_linear_chain_past_the_second_hop() {
    let g = linear_chain(5);
    let propagator = StandingWavePropagator::new(10, FiniteF32::new(0.001), 100_000);
    let seeds = vec![(NodeId::new(0), FiniteF32::ONE)];

    let result = propagator
        .propagate(
            &g,
            &seeds,
            PosF32::new(1.0).unwrap(),
            PosF32::new(4.0).unwrap(),
        )
        .unwrap();

    let amp = |i: usize| result.accumulators[i].amplitude().get().abs();

    // Node 1 (hop 1) is reached even by the buggy code; the regression is that
    // everything past it stayed at zero.
    assert!(amp(1) > 0.0, "node 1 must receive the wave, got {}", amp(1));

    // The real lock: the wave must travel DOWN the chain, not die at hop 2.
    assert!(
        amp(2) > 0.0,
        "node 2 (hop 2) must receive amplitude — the wave must not reflect at a \
         forward-only node; got {}",
        amp(2)
    );
    assert!(
        amp(3) > 0.0,
        "node 3 (hop 3) must receive amplitude as the wave travels the chain; got {}",
        amp(3)
    );
    assert!(
        amp(4) > 0.0,
        "node 4 (chain end) must receive amplitude; got {}",
        amp(4)
    );
}

#[test]
fn terminal_node_with_no_outgoing_edge_still_reflects() {
    // Guards against over-correcting: a genuine dead-end (out-degree 0) must
    // still reflect, so the seed's energy is not silently lost. Node 1 has no
    // outgoing edge; its reflection feeds amplitude back to node 0.
    let g = linear_chain(2);
    let propagator = StandingWavePropagator::new(6, FiniteF32::new(0.001), 100_000);
    let seeds = vec![(NodeId::new(0), FiniteF32::ONE)];

    let result = propagator
        .propagate(
            &g,
            &seeds,
            PosF32::new(1.0).unwrap(),
            PosF32::new(4.0).unwrap(),
        )
        .unwrap();

    assert!(
        result.accumulators[1].amplitude().get().abs() > 0.0,
        "the terminal node must receive the forward pulse"
    );
    assert!(
        result.pulses_processed > 0,
        "reflection at the true dead-end must keep the wave alive"
    );
}

#[test]
fn self_loop_node_reflects_and_does_not_run_away() {
    // A node whose single outgoing edge loops back to itself leads nowhere new,
    // so it is treated as a dead-end and reflects. This must terminate well
    // inside the hop budget — never spin self-pulses up to pulse_budget.
    let mut g = Graph::new();
    for i in 0..2 {
        g.add_node(
            &format!("n_{i}"),
            &format!("n_{i}"),
            NodeType::Function,
            &["loop"],
            0.0,
            0.1,
        )
        .unwrap();
    }
    // 0 -> 1, and 1 -> 1 (self-loop is node 1's only outgoing edge).
    g.add_edge(
        NodeId::new(0),
        NodeId::new(1),
        "calls",
        FiniteF32::new(0.9),
        EdgeDirection::Forward,
        false,
        FiniteF32::new(0.5),
    )
    .unwrap();
    g.add_edge(
        NodeId::new(1),
        NodeId::new(1),
        "calls",
        FiniteF32::new(0.9),
        EdgeDirection::Forward,
        false,
        FiniteF32::new(0.5),
    )
    .unwrap();
    g.finalize().unwrap();

    let budget = 100_000;
    let propagator = StandingWavePropagator::new(8, FiniteF32::new(0.001), budget);
    let seeds = vec![(NodeId::new(0), FiniteF32::ONE)];
    let result = propagator
        .propagate(
            &g,
            &seeds,
            PosF32::new(1.0).unwrap(),
            PosF32::new(4.0).unwrap(),
        )
        .unwrap();

    assert!(
        result.accumulators[1].amplitude().get().abs() > 0.0,
        "the self-loop node still receives the forward pulse"
    );
    assert!(
        result.pulses_processed < 1_000,
        "a self-loop must be hop-bounded, not run away to the budget; got {}",
        result.pulses_processed
    );
}
