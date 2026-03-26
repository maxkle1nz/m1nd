// STRESS TEST: push all 5 RETROBUILDER modules to breaking point.
//
// Usage: cargo test -p m1nd-core --test retrobuilder_stress -- --nocapture
//
// Tests: adversarial inputs, scale limits, edge cases, performance.

use m1nd_ingest::{IngestConfig, Ingestor};
use m1nd_core::taint::{TaintConfig, TaintEngine, TaintType};
use m1nd_core::twins::{TwinConfig, find_twins};
use m1nd_core::refactor::{RefactorConfig, plan_refactoring};
use m1nd_core::runtime_overlay::{RuntimeOverlay, OtelBatch, OtelSpan};
use m1nd_core::git_history::{GitDepth, parse_git_history, inject_git_history};
use m1nd_core::temporal::CoChangeMatrix;
use m1nd_core::graph::Graph;
use m1nd_core::types::{NodeId, NodeType, EdgeDirection, FiniteF32};
use std::path::PathBuf;
use std::collections::HashMap;
use std::time::Instant;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Helpers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn ingest_m1nd() -> Graph {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf();
    let config = IngestConfig { root, ..Default::default() };
    let (graph, stats) = Ingestor::new(config).ingest().unwrap();
    eprintln!("[STRESS] Ingested: {} nodes, {} edges", graph.num_nodes(), graph.num_edges());
    graph
}

/// Build a dense graph with N nodes where every node connects to every other.
fn build_fully_connected(n: usize) -> Graph {
    let mut g = Graph::new();
    for i in 0..n {
        g.add_node(
            &format!("node_{i}"),
            &format!("function_{i}"),
            NodeType::Function,
            &["stress"],
            0.0,
            0.5,
        ).unwrap();
    }
    for i in 0..n {
        for j in 0..n {
            if i != j {
                let _ = g.add_edge(
                    NodeId::new(i as u32),
                    NodeId::new(j as u32),
                    "calls",
                    FiniteF32::new(0.5),
                    EdgeDirection::Forward,
                    false,
                    FiniteF32::new(0.3),
                );
            }
        }
    }
    g.finalize().unwrap();
    g
}

/// Build a long chain: 0 → 1 → 2 → ... → N-1
fn build_chain(n: usize) -> Graph {
    let mut g = Graph::new();
    for i in 0..n {
        g.add_node(
            &format!("chain_{i}"),
            &format!("step_{i}"),
            NodeType::Function,
            &["chain"],
            0.0,
            0.1,
        ).unwrap();
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
        ).unwrap();
    }
    g.finalize().unwrap();
    g
}

/// Build a star graph: hub → spoke_0, hub → spoke_1, ...
fn build_star(spokes: usize) -> Graph {
    let mut g = Graph::new();
    g.add_node("hub", "central_hub", NodeType::Function, &["hub"], 0.0, 1.0).unwrap();
    for i in 0..spokes {
        g.add_node(
            &format!("spoke_{i}"),
            &format!("handler_{i}"),
            NodeType::Function,
            &["spoke"],
            0.0,
            0.1,
        ).unwrap();
        g.add_edge(
            NodeId::new(0),
            NodeId::new((i + 1) as u32),
            "calls",
            FiniteF32::new(0.7),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.3),
        ).unwrap();
    }
    g.finalize().unwrap();
    g
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// STRESS: RB-02 Taint
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn stress_taint_fully_connected_50() {
    // Fully connected graph = worst case for flow + epidemic
    let g = build_fully_connected(50);
    let t = Instant::now();
    let config = TaintConfig::default();
    let result = TaintEngine::analyze(&g, &[NodeId::new(0)], &config).unwrap();
    let elapsed = t.elapsed();
    eprintln!("[STRESS TAINT] Fully connected 50 nodes: risk={:.3}, reached={}, elapsed={:.1}ms",
        result.risk_score, result.summary.total_nodes_reached, elapsed.as_secs_f64() * 1000.0);
    assert!(elapsed.as_secs() < 30, "Should complete within 30s, took {:?}", elapsed);
    assert!(result.risk_score >= 0.0 && result.risk_score <= 1.0);
}

#[test]
fn stress_taint_chain_500() {
    // Long chain = deep propagation
    let g = build_chain(500);
    let t = Instant::now();
    let config = TaintConfig { max_depth: 100, ..TaintConfig::default() };
    let result = TaintEngine::analyze(&g, &[NodeId::new(0)], &config).unwrap();
    let elapsed = t.elapsed();
    eprintln!("[STRESS TAINT] Chain 500 nodes: risk={:.3}, reached={}, elapsed={:.1}ms",
        result.risk_score, result.summary.total_nodes_reached, elapsed.as_secs_f64() * 1000.0);
    assert!(elapsed.as_secs() < 10);
    assert!(result.summary.total_nodes_reached > 1, "Should propagate along chain");
}

#[test]
fn stress_taint_all_entry_points() {
    // Use EVERY node as entry point on real graph
    let g = ingest_m1nd();
    let n = g.num_nodes() as usize;
    let entries: Vec<NodeId> = (0..n.min(100)).map(|i| NodeId::new(i as u32)).collect();
    let t = Instant::now();
    let config = TaintConfig { max_depth: 5, num_particles: 1, epidemic_iterations: 10, ..TaintConfig::default() };
    let result = TaintEngine::analyze(&g, &entries, &config).unwrap();
    let elapsed = t.elapsed();
    eprintln!("[STRESS TAINT] 100 entry points on real graph: risk={:.3}, reached={}, leaks={}, elapsed={:.1}ms",
        result.risk_score, result.summary.total_nodes_reached, result.summary.leaks_found,
        elapsed.as_secs_f64() * 1000.0);
    assert!(elapsed.as_secs() < 30);
}

#[test]
fn stress_taint_sensitive_data_mode() {
    let g = ingest_m1nd();
    let config = TaintConfig {
        taint_type: TaintType::SensitiveData,
        ..TaintConfig::default()
    };
    let result = TaintEngine::analyze(&g, &[NodeId::new(0)], &config).unwrap();
    eprintln!("[STRESS TAINT] SensitiveData mode: hits={}, misses={}, risk={:.3}",
        result.summary.boundary_hits, result.summary.boundary_misses, result.risk_score);
}

#[test]
fn stress_taint_custom_boundaries() {
    let g = ingest_m1nd();
    let config = TaintConfig {
        taint_type: TaintType::Custom {
            boundary_patterns: vec!["graph".to_string(), "node".to_string(), "edge".to_string()],
        },
        ..TaintConfig::default()
    };
    let result = TaintEngine::analyze(&g, &[NodeId::new(0)], &config).unwrap();
    eprintln!("[STRESS TAINT] Custom boundaries (graph/node/edge): hits={}, misses={}, risk={:.3}",
        result.summary.boundary_hits, result.summary.boundary_misses, result.risk_score);
    // With graph/node/edge as boundaries, many nodes should match
    let total_boundaries = result.summary.boundary_hits + result.summary.boundary_misses;
    assert!(total_boundaries > 10, "graph/node/edge should match many nodes, got {}", total_boundaries);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// STRESS: RB-03 Twins
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn stress_twins_all_identical() {
    // 100 nodes with identical topology — all should be twins of each other
    let g = build_star(100);
    let config = TwinConfig {
        similarity_threshold: 0.95,
        top_k: 200,
        node_types: vec![],
        scope: None,
        use_edge_types: true,
    };
    let t = Instant::now();
    let result = find_twins(&g, &config).unwrap();
    let elapsed = t.elapsed();
    eprintln!("[STRESS TWINS] Star 100 spokes: {} pairs, {} analyzed, {:.1}ms",
        result.pairs.len(), result.nodes_analyzed, elapsed.as_secs_f64() * 1000.0);
    // All spokes should be twins (same in-degree=1, out-degree=0)
    assert!(result.pairs.len() > 10, "Spokes should be highly similar, got {} pairs", result.pairs.len());
    assert!(elapsed.as_secs() < 5);
}

#[test]
fn stress_twins_large_chain() {
    // Chain: interior nodes should be twins (in=1, out=1)
    let g = build_chain(200);
    let config = TwinConfig {
        similarity_threshold: 0.9,
        top_k: 100,
        node_types: vec![],
        scope: None,
        use_edge_types: true,
    };
    let t = Instant::now();
    let result = find_twins(&g, &config).unwrap();
    let elapsed = t.elapsed();
    eprintln!("[STRESS TWINS] Chain 200: {} pairs, {} analyzed, {:.1}ms",
        result.pairs.len(), result.nodes_analyzed, elapsed.as_secs_f64() * 1000.0);
    assert!(result.pairs.len() > 0, "Interior chain nodes should be twins");
    assert!(elapsed.as_secs() < 10);
}

#[test]
fn stress_twins_real_functions_only() {
    let g = ingest_m1nd();
    let config = TwinConfig {
        similarity_threshold: 0.90,
        top_k: 50,
        node_types: vec![NodeType::Function],
        scope: None,
        use_edge_types: true,
    };
    let t = Instant::now();
    let result = find_twins(&g, &config).unwrap();
    let elapsed = t.elapsed();
    eprintln!("[STRESS TWINS] Real graph, Functions only: {} pairs, {} analyzed, {:.1}ms",
        result.pairs.len(), result.nodes_analyzed, elapsed.as_secs_f64() * 1000.0);
    for pair in result.pairs.iter().take(10) {
        eprintln!("   {:.3}: '{}' ↔ '{}'", pair.similarity, pair.node_a_label, pair.node_b_label);
    }
    assert!(elapsed.as_secs() < 30);
}

#[test]
fn stress_twins_low_threshold() {
    // Very low threshold = many matches. Tests that we don't OOM or hang.
    let g = ingest_m1nd();
    let config = TwinConfig {
        similarity_threshold: 0.50,
        top_k: 200,
        node_types: vec![],
        scope: None,
        use_edge_types: true,
    };
    let t = Instant::now();
    let result = find_twins(&g, &config).unwrap();
    let elapsed = t.elapsed();
    eprintln!("[STRESS TWINS] Threshold=0.50: {} pairs, {:.1}ms",
        result.pairs.len(), elapsed.as_secs_f64() * 1000.0);
    assert!(result.pairs.len() >= 20, "Low threshold should find many pairs");
    assert!(elapsed.as_secs() < 30);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// STRESS: RB-04 Refactor
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn stress_refactor_fully_connected() {
    // Fully connected = no natural communities. Should handle gracefully.
    let g = build_fully_connected(30);
    let config = RefactorConfig {
        min_community_size: 2,
        max_communities: 20,
        max_acceptable_impact: 0.5,
        scope: None,
    };
    let t = Instant::now();
    let result = plan_refactoring(&g, &config).unwrap();
    let elapsed = t.elapsed();
    eprintln!("[STRESS REFACTOR] Fully connected 30: modularity={:.4}, communities={}, candidates={}, {:.1}ms",
        result.graph_modularity, result.num_communities, result.candidates.len(),
        elapsed.as_secs_f64() * 1000.0);
    // Fully connected → modularity should be low (no natural separation)
    assert!(result.graph_modularity < 0.5, "Fully connected should have low modularity, got {:.4}", result.graph_modularity);
    assert!(elapsed.as_secs() < 10);
}

#[test]
fn stress_refactor_chain() {
    let g = build_chain(200);
    let config = RefactorConfig {
        min_community_size: 3,
        max_communities: 20,
        max_acceptable_impact: 0.8,
        scope: None,
    };
    let t = Instant::now();
    let result = plan_refactoring(&g, &config).unwrap();
    let elapsed = t.elapsed();
    eprintln!("[STRESS REFACTOR] Chain 200: modularity={:.4}, communities={}, candidates={}, {:.1}ms",
        result.graph_modularity, result.num_communities, result.candidates.len(),
        elapsed.as_secs_f64() * 1000.0);
    assert!(elapsed.as_secs() < 10);
}

#[test]
fn stress_refactor_real_aggressive() {
    // Very small min_community_size + many communities
    let g = ingest_m1nd();
    let config = RefactorConfig {
        min_community_size: 2,
        max_communities: 50,
        max_acceptable_impact: 0.9,
        scope: None,
    };
    let t = Instant::now();
    let result = plan_refactoring(&g, &config).unwrap();
    let elapsed = t.elapsed();
    eprintln!("[STRESS REFACTOR] Aggressive on real graph: modularity={:.4}, communities={}, candidates={}, {:.1}ms",
        result.graph_modularity, result.num_communities, result.candidates.len(),
        elapsed.as_secs_f64() * 1000.0);
    for (i, c) in result.candidates.iter().take(5).enumerate() {
        eprintln!("   #{}: {} nodes, cohesion={:.3}, coupling={:.3}, risk={} ({:.3})",
            i, c.extracted_nodes.len(), c.cohesion, c.coupling, c.risk.level, c.risk.activation_loss);
    }
    assert!(elapsed.as_secs() < 60);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// STRESS: RB-05 OTel Overlay
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn stress_otel_1000_spans() {
    let graph = ingest_m1nd();
    let n = graph.num_nodes() as usize;

    // 1000 spans — first 500 matching real labels, last 500 garbage
    let mut spans = Vec::new();
    let n = graph.num_nodes() as usize;
    for i in 0..500 {
        let idx = i % n;
        let name = graph.strings.resolve(graph.nodes.label[idx]).to_string();
        spans.push(OtelSpan {
            name,
            duration_us: 100 + (i as u64 * 10),
            count: 1 + (i as u64 % 100),
            is_error: i % 13 == 0,
            attributes: HashMap::new(),
            parent: None,
        });
    }
    for i in 500..1000 {
        spans.push(OtelSpan {
            name: format!("zzz_totally_fake_xq7k_{i}"),
            duration_us: 100 + (i as u64 * 10),
            count: 1,
            is_error: false,
            attributes: HashMap::new(),
            parent: None,
        });
    }

    let batch = OtelBatch {
        spans,
        timestamp: 1700000000.0,
        service_name: "stress-test".to_string(),
    };

    let mut overlay = RuntimeOverlay::with_defaults();
    let t = Instant::now();
    let result = overlay.ingest(&graph, &batch).unwrap();
    let elapsed = t.elapsed();

    eprintln!("[STRESS OTEL] 1000 spans: mapped={}, unmapped={}, hot={}, {:.1}ms",
        result.spans_mapped, result.spans_unmapped, result.hot_nodes.len(),
        elapsed.as_secs_f64() * 1000.0);

    assert!(result.spans_processed == 1000);
    assert!(result.spans_mapped > 0, "Should map at least some spans");
    assert!(result.spans_unmapped > 0, "Garbage spans should be unmapped");
    assert!(elapsed.as_secs() < 5);
}

#[test]
fn stress_otel_repeated_batches() {
    // Ingest 10 batches in a row — test heat decay and accumulation
    let mut graph = ingest_m1nd();
    let label0 = graph.strings.resolve(graph.nodes.label[0]).to_string();
    let label1 = graph.strings.resolve(graph.nodes.label[1]).to_string();

    let mut overlay = RuntimeOverlay::with_defaults();
    let mut map_counts = Vec::new();

    for batch_idx in 0..10 {
        let batch = OtelBatch {
            spans: vec![
                OtelSpan {
                    name: label0.clone(),
                    duration_us: 5000,
                    count: 100,
                    is_error: false,
                    attributes: HashMap::new(),
                    parent: None,
                },
                OtelSpan {
                    name: label1.clone(),
                    duration_us: 3000,
                    count: 50,
                    is_error: batch_idx % 3 == 0,
                    attributes: HashMap::new(),
                    parent: None,
                },
            ],
            timestamp: 1700000000.0 + (batch_idx as f64 * 60.0),
            service_name: "decay-test".to_string(),
        };

        let result = overlay.ingest(&graph, &batch).unwrap();
        map_counts.push((result.spans_mapped, result.hot_nodes.len()));
    }

    // Apply boosts after all batches
    let applied = overlay.apply_boosts(&mut graph, 0.1);

    eprintln!("[STRESS OTEL] 10 repeated batches:");
    for (i, (mapped, hot)) in map_counts.iter().enumerate() {
        eprintln!("   Batch {}: mapped={}, hot_nodes={}", i, mapped, hot);
    }
    eprintln!("   Final boosts applied: {}", applied);

    // Heat should accumulate across batches
    assert!(applied > 0);
}

#[test]
fn stress_otel_all_errors() {
    // Every span is an error — test error tracking at scale
    let graph = ingest_m1nd();
    let n = graph.num_nodes() as usize;

    let spans: Vec<OtelSpan> = (0..n.min(200)).map(|i| {
        OtelSpan {
            name: graph.strings.resolve(graph.nodes.label[i]).to_string(),
            duration_us: 10000,
            count: 1,
            is_error: true, // ALL errors
            attributes: HashMap::new(),
            parent: None,
        }
    }).collect();

    let batch = OtelBatch {
        spans,
        timestamp: 1700000000.0,
        service_name: "error-storm".to_string(),
    };

    let mut overlay = RuntimeOverlay::with_defaults();
    let result = overlay.ingest(&graph, &batch).unwrap();

    eprintln!("[STRESS OTEL] All-error batch: mapped={}, hot_nodes={}",
        result.spans_mapped, result.hot_nodes.len());

    let error_nodes: Vec<_> = result.hot_nodes.iter().filter(|n| n.error_count > 0).collect();
    eprintln!("[STRESS OTEL] Nodes with errors: {}/{}", error_nodes.len(), result.hot_nodes.len());

    assert!(error_nodes.len() > 0, "Error tracking should work");
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// STRESS: Cross-module integration
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[test]
fn stress_taint_then_refactor() {
    // Run taint → use leaked nodes as refactor scope hints
    let g = ingest_m1nd();

    // Taint first
    let taint_config = TaintConfig::default();
    let taint_result = TaintEngine::analyze(&g, &[NodeId::new(0)], &taint_config).unwrap();

    // Refactor
    let refactor_config = RefactorConfig {
        min_community_size: 3,
        max_communities: 10,
        max_acceptable_impact: 0.5,
        scope: None,
    };
    let refactor_result = plan_refactoring(&g, &refactor_config).unwrap();

    eprintln!("[STRESS CROSS] Taint risk={:.3}, leaks={} → Refactor modularity={:.4}, candidates={}",
        taint_result.risk_score, taint_result.summary.leaks_found,
        refactor_result.graph_modularity, refactor_result.candidates.len());

    // Both should complete without interference
    assert!(taint_result.risk_score >= 0.0);
    assert!(refactor_result.num_communities >= 1);
}

#[test]
fn stress_twins_then_otel() {
    // Find twins → boost one twin with OTel → verify divergence
    let mut g = ingest_m1nd();

    let twin_config = TwinConfig {
        similarity_threshold: 0.95,
        top_k: 10,
        node_types: vec![],
        scope: None,
        use_edge_types: true,
    };
    let twins = find_twins(&g, &twin_config).unwrap();
    eprintln!("[STRESS CROSS] Found {} twin pairs", twins.pairs.len());

    if let Some(pair) = twins.pairs.first() {
        // Boost one twin with OTel
        let batch = OtelBatch {
            spans: vec![OtelSpan {
                name: pair.node_a_label.clone(),
                duration_us: 50000,
                count: 1000,
                is_error: false,
                attributes: HashMap::new(),
                parent: None,
            }],
            timestamp: 1700000000.0,
            service_name: "twin-diverge".to_string(),
        };

        let mut overlay = RuntimeOverlay::with_defaults();
        let result = overlay.ingest(&g, &batch).unwrap();
        let applied = overlay.apply_boosts(&mut g, 0.5);

        eprintln!("[STRESS CROSS] Boosted twin '{}': mapped={}, boosts={}",
            pair.node_a_label, result.spans_mapped, applied);
    }
}
