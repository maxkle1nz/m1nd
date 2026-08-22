// Integration test: run all 5 RETROBUILDER modules against the real m1nd codebase graph.
//
// Usage: cargo test -p m1nd-core --test retrobuilder_real -- --nocapture

use m1nd_core::git_history::{inject_git_history, parse_git_history, GitDepth};
use m1nd_core::refactor::{plan_refactoring, RefactorConfig};
use m1nd_core::runtime_overlay::{OtelBatch, OtelSpan, RuntimeOverlay};
use m1nd_core::taint::{TaintConfig, TaintEngine};
use m1nd_core::temporal::CoChangeMatrix;
use m1nd_core::twins::{find_twins, TwinConfig};
use m1nd_core::types::NodeId;
use m1nd_ingest::{IngestConfig, Ingestor};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Copies `source` into a fresh private temp directory (skipping `target` and
/// `node_modules`, which the ingest walker never visits anyway and which can
/// run to gigabytes). `Ingestor::ingest()` walks the whole tree once to
/// extract and once more at COMPLETE-time to revalidate that no source
/// changed mid-ingest (`m1nd_ingest::CodeIngestBundleV1::revalidate_sources`)
/// — for a ~1000-file workspace that second walk lands minutes after the
/// first. Ingesting the LIVE working tree in place means any edit landing
/// anywhere in that multi-minute window (a human, another agent, a sibling
/// test) is real drift and correctly trips `FullReindexRequired` — the fence
/// is doing its job, but a long-running stress test has no business being
/// hostage to everything else touching the repo while it runs. Ingesting a
/// private copy instead makes the source set provably immutable for the
/// whole run, without touching the guard itself: genuine drift inside the
/// copy (a real bug in the ingest pipeline) still fails it exactly the same
/// way.
fn snapshot_repo_root(source: &Path) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("snapshot tempdir");
    copy_tree(source, dir.path());
    dir
}

fn copy_tree(src: &Path, dst: &Path) {
    for entry in std::fs::read_dir(src).expect("read_dir") {
        let entry = entry.expect("dir entry");
        let file_type = entry.file_type().expect("file_type");
        let name = entry.file_name();
        if file_type.is_dir() && (name == "target" || name == "node_modules") {
            continue;
        }
        let dst_path = dst.join(&name);
        if file_type.is_dir() {
            std::fs::create_dir(&dst_path).expect("create_dir");
            copy_tree(&entry.path(), &dst_path);
        } else if file_type.is_file() {
            std::fs::copy(entry.path(), &dst_path).expect("copy file");
        }
        // Symlinks are skipped: the walker never follows them either
        // (`follow_links(false)`), so they carry nothing ingestable.
    }
}

fn ingest_m1nd() -> m1nd_core::graph::Graph {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    eprintln!("[REAL TEST] Ingesting m1nd from: {:?}", root);
    let snapshot = snapshot_repo_root(&root);

    let config = IngestConfig {
        root: snapshot.path().to_path_buf(),
        ..Default::default()
    };
    let ingestor = Ingestor::new(config);
    let (graph, stats) = ingestor.ingest().unwrap();

    eprintln!(
        "[REAL TEST] Ingest complete: {} nodes, {} edges, {} files parsed in {:.1}ms",
        graph.num_nodes(),
        graph.num_edges(),
        stats.files_parsed,
        stats.elapsed_ms
    );

    assert!(
        graph.num_nodes() > 100,
        "Real codebase should have >100 nodes, got {}",
        graph.num_nodes()
    );
    assert!(
        graph.num_edges() > 100,
        "Real codebase should have >100 edges, got {}",
        graph.num_edges()
    );

    graph
}

#[test]
fn rb01_git_history_real() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let depth = GitDepth::Days(30);
    let commits = parse_git_history(&root, depth);

    match commits {
        Ok(ref c) => {
            eprintln!("[RB-01] ✅ Parsed {} commits from git history", c.len());
            for commit in c.iter().take(3) {
                eprintln!(
                    "[RB-01]   {} by {} — {} files",
                    &commit.hash[..8],
                    commit.author,
                    commit.files.len()
                );
            }

            // Now inject into a real graph
            let graph = ingest_m1nd();
            let mut co_change = CoChangeMatrix::bootstrap(&graph, 10_000).unwrap();
            let result = inject_git_history(&graph, &mut co_change, c).unwrap();

            eprintln!(
                "[RB-01] ✅ Injected: {} pairs, {} ghost edges",
                result.co_change_pairs_injected, result.ghost_edges_found
            );
        }
        Err(e) => {
            eprintln!(
                "[RB-01] ⚠ Git history failed (may not be a git repo): {:?}",
                e
            );
        }
    }
}

#[test]
fn rb02_taint_real() {
    let graph = ingest_m1nd();
    let n = graph.num_nodes() as usize;

    // Find entry point nodes (handlers, ingests, mains)
    let mut entries = Vec::new();
    for i in 0..n {
        let label = graph.strings.resolve(graph.nodes.label[i]).to_lowercase();
        if (label.contains("handle") || label.contains("ingest") || label.contains("main"))
            && entries.len() < 3
        {
            entries.push(NodeId::new(i as u32));
            eprintln!("[RB-02] Entry point: node {} = '{}'", i, label);
        }
    }

    if entries.is_empty() {
        eprintln!("[RB-02] ⚠ No entry points found — skipping taint test");
        return;
    }

    eprintln!(
        "[RB-02] Running taint from {} entry points on {} nodes",
        entries.len(),
        n
    );
    let config = TaintConfig::default();
    let result = TaintEngine::analyze(&graph, &entries, &config).unwrap();

    eprintln!("[RB-02] ✅ Taint analysis complete:");
    eprintln!("  Nodes reached: {}", result.summary.total_nodes_reached);
    eprintln!("  Boundary hits: {}", result.summary.boundary_hits);
    eprintln!("  Boundary misses: {}", result.summary.boundary_misses);
    eprintln!("  Leaks found: {}", result.summary.leaks_found);
    eprintln!("  Risk score: {:.3}", result.risk_score);
    eprintln!("  Elapsed: {:.1}ms", result.summary.elapsed_ms);

    assert!(
        result.risk_score >= 0.0 && result.risk_score <= 1.0,
        "Risk must be [0,1]"
    );
    assert!(
        result.summary.total_nodes_reached > 0,
        "Taint should reach at least some nodes"
    );
}

#[test]
fn rb03_twins_real() {
    let graph = ingest_m1nd();
    let config = TwinConfig {
        similarity_threshold: 0.85,
        top_k: 20,
        node_types: vec![],
        scope: None,
        use_edge_types: true,
        name_overlap_min: 0.0,
    };

    eprintln!(
        "[RB-03] Running twin detection on {} nodes",
        graph.num_nodes()
    );
    let result = find_twins(&graph, &config).unwrap();

    eprintln!(
        "[RB-03] ✅ Twins found: {} pairs, {} nodes analyzed",
        result.pairs.len(),
        result.nodes_analyzed
    );

    for pair in result.pairs.iter().take(10) {
        eprintln!(
            "[RB-03]   {:.3} sim: '{}' ↔ '{}'",
            pair.similarity, pair.node_a_label, pair.node_b_label
        );
    }

    assert!(result.nodes_analyzed > 0);
}

#[test]
fn rb04_refactor_real() {
    let graph = ingest_m1nd();
    let config = RefactorConfig {
        min_community_size: 3,
        max_communities: 10,
        max_acceptable_impact: 0.5,
        scope: None,
    };

    eprintln!(
        "[RB-04] Running refactor planner on {} nodes",
        graph.num_nodes()
    );
    let result = plan_refactoring(&graph, &config).unwrap();

    eprintln!(
        "[RB-04] ✅ Modularity: {:.4}, Communities: {}, Candidates: {}, Elapsed: {:.1}ms",
        result.graph_modularity,
        result.num_communities,
        result.candidates.len(),
        result.elapsed_ms
    );

    for (i, c) in result.candidates.iter().take(5).enumerate() {
        eprintln!("[RB-04]   #{}: {} nodes, cohesion={:.3}, coupling={:.3}, risk={} (loss={:.3}), {} interface edges",
            i, c.extracted_nodes.len(), c.cohesion, c.coupling, c.risk.level, c.risk.activation_loss,
            c.interface_edges.len());
    }

    assert!(
        result.num_communities >= 1,
        "Should detect at least 1 community"
    );
    assert!(result.nodes_analyzed > 100, "Should analyze all nodes");
}

#[test]
fn rb05_otel_overlay_real() {
    let mut graph = ingest_m1nd();
    let n = graph.num_nodes() as usize;

    // Build synthetic OTel spans from REAL node labels
    let mut spans = Vec::new();
    for i in 0..n.min(20) {
        let label = graph.strings.resolve(graph.nodes.label[i]).to_string();
        if !label.is_empty() {
            spans.push(OtelSpan {
                name: label.clone(),
                duration_us: 1000 + (i as u64 * 500),
                count: 10 + (i as u64),
                is_error: i % 7 == 0,
                attributes: HashMap::new(),
                parent: None,
            });
        }
    }

    let batch = OtelBatch {
        spans,
        timestamp: 1700000000.0,
        service_name: "m1nd-real-test".to_string(),
    };

    let mut overlay = RuntimeOverlay::with_defaults();
    let result = overlay.ingest(&graph, &batch).unwrap();

    eprintln!(
        "[RB-05] ✅ OTel overlay: processed={}, mapped={}, unmapped={}, hot_nodes={}",
        result.spans_processed,
        result.spans_mapped,
        result.spans_unmapped,
        result.hot_nodes.len()
    );

    for node in result.hot_nodes.iter().take(5) {
        eprintln!(
            "[RB-05]   heat={:.3} invocations={} errors={}: '{}'",
            node.heat, node.invocation_count, node.error_count, node.label
        );
    }

    let applied = overlay.apply_boosts(&mut graph, 0.1);
    eprintln!("[RB-05] Boosts applied: {}", applied);

    assert!(result.spans_processed > 0);
    assert!(
        result.spans_mapped > 0,
        "At least some spans should map to real nodes"
    );
}
