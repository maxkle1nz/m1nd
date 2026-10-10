//! Process-local fixture preparation; every caller owns a complete independent graph.
//!
//! Keep the seed private: even `&Graph` exposes atomic edge-weight mutation. No
//! snapshot is used because persistence intentionally omits transient graph state.
//! There is no disk cache or reuse across checkouts, binaries, or nextest processes.

use m1nd_core::graph::{CsrGraph, EdgePlasticity, Graph, NodeStorage, StringInterner};
use m1nd_core::types::InternedStr;
use std::ffi::OsStr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

pub struct GraphFixture {
    seed: OnceLock<Graph>,
}

impl GraphFixture {
    pub const fn new() -> Self {
        Self {
            seed: OnceLock::new(),
        }
    }

    pub fn graph(&self, initialize: impl FnOnce() -> Graph) -> Graph {
        self.graph_for_mode(
            std::env::var_os("NEXTEST_EXECUTION_MODE").as_deref(),
            initialize,
        )
    }

    pub fn graph_for_mode(
        &self,
        execution_mode: Option<&OsStr>,
        initialize: impl FnOnce() -> Graph,
    ) -> Graph {
        // https://nexte.st/docs/configuration/env-vars/#environment-variables-nextest-sets
        // A nextest process has no sibling case to reuse a seed. Preserve the
        // original direct ingest: no extra graph allocation or retained seed.
        if execution_mode == Some(OsStr::new("process-per-test")) {
            return initialize();
        }
        let seed = self.seed.get_or_init(|| {
            let started = Instant::now();
            let graph = initialize();
            eprintln!("[RETRO FIXTURE] Seed preparation: {:?}", started.elapsed());
            graph
        });
        let started = Instant::now();
        let graph = copy_graph(seed);
        eprintln!("[RETRO FIXTURE] Independent copy: {:?}", started.elapsed());
        graph
    }
}

/// Exact storage copy, rather than re-adding nodes/edges or re-finalizing them.
/// Exhaustive struct literals make new graph fields a compile error here, so
/// fixture state cannot silently disappear as the engine evolves.
fn copy_graph(source: &Graph) -> Graph {
    // Preserve every interned handle, including strings not referenced by a node.
    let mut strings = StringInterner::with_capacity(source.strings.len());
    for index in 0..source.strings.len() {
        let handle = InternedStr(index as u32);
        assert_eq!(
            strings.get_or_intern(source.strings.resolve(handle)),
            handle
        );
    }

    Graph {
        nodes: NodeStorage {
            count: source.nodes.count,
            activation: source.nodes.activation.clone(),
            pagerank: source.nodes.pagerank.clone(),
            plasticity: source.nodes.plasticity.clone(),
            label: source.nodes.label.clone(),
            node_type: source.nodes.node_type.clone(),
            tags: source.nodes.tags.clone(),
            last_modified: source.nodes.last_modified.clone(),
            change_frequency: source.nodes.change_frequency.clone(),
            provenance: source.nodes.provenance.clone(),
        },
        csr: CsrGraph {
            offsets: source.csr.offsets.clone(),
            targets: source.csr.targets.clone(),
            weights: source
                .csr
                .weights
                .iter()
                .map(|weight| AtomicU32::new(weight.load(Ordering::Relaxed)))
                .collect(),
            inhibitory: source.csr.inhibitory.clone(),
            relations: source.csr.relations.clone(),
            directions: source.csr.directions.clone(),
            causal_strengths: source.csr.causal_strengths.clone(),
            rev_offsets: source.csr.rev_offsets.clone(),
            rev_sources: source.csr.rev_sources.clone(),
            rev_edge_idx: source.csr.rev_edge_idx.clone(),
            pending_edges: source.csr.pending_edges.clone(),
        },
        edge_plasticity: EdgePlasticity {
            original_weight: source.edge_plasticity.original_weight.clone(),
            current_weight: source.edge_plasticity.current_weight.clone(),
            strengthen_count: source.edge_plasticity.strengthen_count.clone(),
            weaken_count: source.edge_plasticity.weaken_count.clone(),
            ltp_applied: source.edge_plasticity.ltp_applied.clone(),
            ltd_applied: source.edge_plasticity.ltd_applied.clone(),
            last_used_query: source.edge_plasticity.last_used_query.clone(),
        },
        strings,
        id_to_node: source.id_to_node.clone(),
        generation: source.generation,
        pagerank_computed: source.pagerank_computed,
        pagerank_dirty: source.pagerank_dirty,
        finalized: source.finalized,
    }
}
