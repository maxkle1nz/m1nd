//! Cross-engine equivalence + fixpoint correctness for the two structural
//! activation engines (WavefrontEngine, HeapEngine).
//!
//! REGRESSION (graph-core sheet §Gaps, "HeapEngine re-expansion"): once a node
//! entered the visited-set, a later STRONGER arrival updated its dense
//! activation but never re-enqueued it, so the stronger value never propagated
//! ONWARD to that node's children — an order-dependent loss. Both engines shared
//! the flaw (Heap via its Bloom no-re-push, Wavefront via its `visited` guard).
//!
//! The fix re-relaxes an already-visited node when a new arrival exceeds the
//! stored activation by `REEXPANSION_MARGIN` (Dijkstra decrease-key). This test
//! pins the intended contract: on a topology where a node is popped/expanded
//! with a WEAK value and only later strengthened, BOTH engines must now converge
//! to the true propagation FIXPOINT (an independent brute-force relaxation), so
//! the strengthened value reaches downstream leaves. Before the fix, the leaf
//! keeps the stale weak value and diverges from the fixpoint.

use m1nd_core::activation::{ActivationEngine, HeapEngine, WavefrontEngine};
use m1nd_core::graph::Graph;
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeId, NodeType, PropagationConfig};

fn add_edge(g: &mut Graph, s: NodeId, t: NodeId, w: f32) {
    g.add_edge(
        s,
        t,
        "flows",
        FiniteF32::new(w),
        EdgeDirection::Forward,
        false,
        FiniteF32::new(0.5),
    )
    .expect("add_edge");
}

/// Independent oracle: iterate scatter-max relaxation to a FIXPOINT with NO
/// visited-set (a node is re-relaxed as long as any incoming arrival strictly
/// improves it). Excitatory-only, so this is monotone and converges. This is the
/// ground truth the engines must match — it re-derives the true activation of
/// every node reachable within `max_depth` hops.
fn fixpoint_oracle(
    graph: &Graph,
    seeds: &[(NodeId, FiniteF32)],
    config: &PropagationConfig,
) -> Vec<f32> {
    let n = graph.num_nodes() as usize;
    let threshold = config.threshold.get();
    let decay = config.decay.get();
    let mut act = vec![0.0f32; n];
    for &(node, s) in seeds {
        let idx = node.as_usize();
        if idx < n {
            act[idx] = act[idx].max(s.get().min(config.saturation_cap.get()));
        }
    }
    // Bellman-style relaxation: at most n*max_depth sweeps guarantees the fixpoint
    // for excitatory propagation (each sweep pushes the frontier one more hop).
    let sweeps = (n * (config.max_depth as usize).max(1)).max(1);
    for _ in 0..sweeps {
        let mut changed = false;
        for src in 0..n {
            let src_act = act[src];
            if src_act < threshold {
                continue;
            }
            let range = graph.csr.out_range(NodeId::new(src as u32));
            for j in range {
                if graph.csr.inhibitory[j] {
                    continue;
                }
                let tgt = graph.csr.targets[j].as_usize();
                if tgt >= n {
                    continue;
                }
                let w = graph
                    .csr
                    .read_weight(m1nd_core::types::EdgeIdx::new(j as u32))
                    .get();
                let signal = src_act * w * decay;
                if signal > threshold && signal > act[tgt] {
                    act[tgt] = signal;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    act
}

fn index_scores(n: usize, r: &m1nd_core::activation::DimensionResult) -> Vec<f32> {
    let mut v = vec![0.0f32; n];
    for (id, s) in &r.scores {
        v[id.as_usize()] = s.get();
    }
    v
}

/// A "weak-first, strong-later" graph: the seed reaches relay nodes over WEAK
/// edges (so each relay is discovered/expanded early at a low value), and the
/// SAME relays over a STRONG detour through a hub whose activation is LOWER than
/// the relays' first value (so the hub — and thus the strong arrival — is
/// processed AFTER the relays have already expanded to their leaves). Node count
/// is pushed into the Bloom-collision regime.
fn build_reexpansion_graph(relays: usize) -> (Graph, Vec<(NodeId, FiniteF32)>) {
    let mut g = Graph::new();
    let seed = g
        .add_node("n::seed", "seed", NodeType::Function, &[], 0.0, 0.0)
        .expect("seed");
    // hub gets a LOW activation from the seed, so it pops/expands only AFTER the
    // relays (which get a higher first value) have already fanned out to leaves.
    let hub = g
        .add_node("n::hub", "hub", NodeType::Function, &[], 0.0, 0.0)
        .expect("hub");

    let mut relay_ids = Vec::with_capacity(relays);
    let mut leaf_ids = Vec::with_capacity(relays);
    for i in 0..relays {
        relay_ids.push(
            g.add_node(
                &format!("n::relay{i}"),
                &format!("relay{i}"),
                NodeType::Function,
                &[],
                0.0,
                0.0,
            )
            .expect("relay"),
        );
        leaf_ids.push(
            g.add_node(
                &format!("n::leaf{i}"),
                &format!("leaf{i}"),
                NodeType::Function,
                &[],
                0.0,
                0.0,
            )
            .expect("leaf"),
        );
    }

    // seed -> hub with a MODERATE weight; seed -> relay with a slightly HIGHER
    // effective value so relays pop before hub.
    add_edge(&mut g, seed, hub, 0.85);
    for (&r, &l) in relay_ids.iter().zip(leaf_ids.iter()) {
        add_edge(&mut g, seed, r, 0.95); // relay first value > hub's value
        add_edge(&mut g, hub, r, 3.0); // hub -> relay: STRONG later arrival
        add_edge(&mut g, r, l, 1.0); // relay -> leaf carries value onward
    }

    g.finalize().expect("finalize");
    let seeds = vec![(seed, FiniteF32::new(1.0))];
    (g, seeds)
}

#[test]
fn heap_and_wavefront_converge_to_fixpoint_on_weak_then_strong() {
    let (graph, seeds) = build_reexpansion_graph(600);
    let config = PropagationConfig {
        max_depth: 6,
        ..PropagationConfig::default()
    };

    let n = graph.num_nodes() as usize;
    let oracle = fixpoint_oracle(&graph, &seeds, &config);
    let wave = index_scores(
        n,
        &WavefrontEngine::new()
            .propagate(&graph, &seeds, &config)
            .expect("wavefront"),
    );
    let heap = index_scores(
        n,
        &HeapEngine::new()
            .propagate(&graph, &seeds, &config)
            .expect("heap"),
    );

    const EPS: f32 = 1e-4;
    let mut heap_vs_oracle = Vec::new();
    let mut wave_vs_oracle = Vec::new();
    let mut heap_vs_wave = Vec::new();
    for i in 0..n {
        if (heap[i] - oracle[i]).abs() > EPS {
            heap_vs_oracle.push((i, oracle[i], heap[i]));
        }
        if (wave[i] - oracle[i]).abs() > EPS {
            wave_vs_oracle.push((i, oracle[i], wave[i]));
        }
        if (heap[i] - wave[i]).abs() > EPS {
            heap_vs_wave.push((i, wave[i], heap[i]));
        }
    }

    // The heart of the regression: without re-relaxation the leaves keep the
    // stale WEAK relay value and diverge from the true fixpoint.
    assert!(
        heap_vs_oracle.is_empty(),
        "HeapEngine must match the fixpoint within {EPS}; {} node(s) diverged (oracle, heap). First: {:?}",
        heap_vs_oracle.len(),
        &heap_vs_oracle[..heap_vs_oracle.len().min(6)]
    );
    assert!(
        wave_vs_oracle.is_empty(),
        "WavefrontEngine must match the fixpoint within {EPS}; {} node(s) diverged (oracle, wave). First: {:?}",
        wave_vs_oracle.len(),
        &wave_vs_oracle[..wave_vs_oracle.len().min(6)]
    );
    assert!(
        heap_vs_wave.is_empty(),
        "the two engines must agree within {EPS}; {} node(s) diverged (wave, heap). First: {:?}",
        heap_vs_wave.len(),
        &heap_vs_wave[..heap_vs_wave.len().min(6)]
    );
}

/// Sanity precondition: the topology actually EXERCISES a strengthening (some
/// leaf's true fixpoint value is strictly greater than the weak-only value it
/// would have if the strong arrival never propagated). Guards against the test
/// silently degrading into a no-op if the topology stops triggering the bug.
#[test]
fn reexpansion_topology_actually_strengthens_a_leaf() {
    let (graph, seeds) = build_reexpansion_graph(64);
    let config = PropagationConfig {
        max_depth: 6,
        ..PropagationConfig::default()
    };
    let n = graph.num_nodes() as usize;
    let oracle = fixpoint_oracle(&graph, &seeds, &config);

    // The weak-only leaf value: seed(1.0) -> relay(0.95) -> leaf, one strong hop
    // never applied. decay applied twice. If the strong arrival matters, the
    // fixpoint leaf value must exceed this.
    let decay = config.decay.get();
    let weak_leaf = 1.0 * 0.95 * decay * 1.0 * decay;

    // relay0 = node index 2, leaf0 = index 3.
    let leaf0 = graph.resolve_id("n::leaf0").expect("leaf0");
    let leaf_fixpoint = oracle[leaf0.as_usize()];
    assert!(
        leaf_fixpoint > weak_leaf + 1e-3,
        "topology must strengthen the leaf (fixpoint {leaf_fixpoint} should exceed weak-only {weak_leaf})",
    );
    let _ = n;
}
