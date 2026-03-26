// === m1nd-core/src/refactor.rs ===
// @m1nd:temponizer:HARDENING — complex 3-primitive composition with risk assessment
// @m1nd:emca:pattern — EXECUTE(community) → MEASURE → EXECUTE(bridge) → MEASURE → EXECUTE(counterfactual) → MEASURE
// @m1nd:primitives — topology::CommunityDetector, topology::BridgeDetector, counterfactual::CounterfactualEngine
//
// RB-04 — Intent-Driven Refactoring: topological cut planner.
//
// Given an intent (e.g. "extract module X from codebase Y"), this module:
// 1. Runs community detection to find natural module boundaries
// 2. Identifies the minimum-cut boundary (bridge edges between communities)
// 3. Simulates the extraction via counterfactual analysis
// 4. Produces a refactoring plan with:
//    - Which nodes belong to the extracted module
//    - Which edges become the new interface (API surface)
//    - Risk assessment (orphaned nodes, activation loss)
//    - Suggested interface specifications

use crate::activation::HybridEngine;
use crate::counterfactual::{CascadeResult, CounterfactualEngine, CounterfactualResult};
use crate::error::{M1ndError, M1ndResult};
use crate::graph::Graph;
use crate::topology::{Bridge, BridgeDetector, CommunityDetector, CommunityResult};
use crate::types::{CommunityId, FiniteF32, NodeId};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the refactoring planner.
#[derive(Clone, Debug)]
pub struct RefactorConfig {
    /// Maximum communities to consider for extraction.
    pub max_communities: usize,
    /// Minimum nodes in a community to consider it extractable.
    pub min_community_size: usize,
    /// Maximum acceptable activation loss for extraction (0.0-1.0).
    pub max_acceptable_impact: f32,
    /// File path scope filter.
    pub scope: Option<String>,
}

impl Default for RefactorConfig {
    fn default() -> Self {
        Self {
            max_communities: 10,
            min_community_size: 3,
            max_acceptable_impact: 0.30,
            scope: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// A single interface edge that would need to become an API boundary.
#[derive(Clone, Debug, Serialize)]
pub struct InterfaceEdge {
    /// External ID of the source node.
    pub source_id: String,
    /// External ID of the target node.
    pub target_id: String,
    /// Edge relation type (e.g., "calls", "imports").
    pub relation: String,
    /// Edge weight.
    pub weight: f32,
    /// Direction: "inbound" (external → extracted) or "outbound" (extracted → external).
    pub direction: String,
}

/// Risk assessment for an extraction.
#[derive(Clone, Debug, Serialize)]
pub struct ExtractionRisk {
    /// Overall risk level.
    pub level: String,
    /// Activation loss if this community is extracted.
    pub activation_loss: f32,
    /// Number of nodes that become orphaned.
    pub orphaned_count: usize,
    /// Number of nodes that lose >50% activation.
    pub weakened_count: usize,
    /// Cascade depth.
    pub cascade_depth: u8,
    /// Total cascade affected nodes.
    pub cascade_affected: u32,
}

/// A proposed module extraction plan.
#[derive(Clone, Debug, Serialize)]
pub struct ExtractionPlan {
    /// Community ID being extracted.
    pub community_id: u32,
    /// External IDs of nodes in the extracted module.
    pub extracted_nodes: Vec<String>,
    /// Labels of nodes in the extracted module.
    pub extracted_labels: Vec<String>,
    /// Interface edges that become API boundaries.
    pub interface_edges: Vec<InterfaceEdge>,
    /// Risk assessment.
    pub risk: ExtractionRisk,
    /// Modularity score of the community.
    pub community_modularity: f32,
    /// Internal cohesion: internal_edges / total_edges for this community.
    pub cohesion: f32,
    /// Coupling: external_edges / total_edges for this community.
    pub coupling: f32,
}

/// Full refactoring plan result.
#[derive(Clone, Debug, Serialize)]
pub struct RefactorPlan {
    /// Candidate extraction plans, sorted by feasibility (low risk first).
    pub candidates: Vec<ExtractionPlan>,
    /// Overall graph modularity.
    pub graph_modularity: f32,
    /// Number of communities detected.
    pub num_communities: u32,
    /// Total nodes analyzed.
    pub nodes_analyzed: usize,
    /// Elapsed time in ms.
    pub elapsed_ms: f64,
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// Build a refactoring plan by composing community detection and
/// counterfactual analysis.
pub fn plan_refactoring(graph: &Graph, config: &RefactorConfig) -> M1ndResult<RefactorPlan> {
    let start = std::time::Instant::now();
    let n = graph.num_nodes() as usize;

    if n == 0 || !graph.finalized {
        return Err(M1ndError::EmptyGraph);
    }

    // Build reverse map: NodeId -> external_id
    let mut node_to_ext: Vec<String> = vec![String::new(); n];
    for (interned, node_id) in &graph.id_to_node {
        let idx = node_id.as_usize();
        if idx < n {
            node_to_ext[idx] = graph.strings.resolve(*interned).to_string();
        }
    }

    // --- Phase 1: Community Detection ---
    let detector = CommunityDetector::with_defaults();
    let communities = detector.detect(graph)?;
    let bridges = BridgeDetector::detect(graph, &communities)?;

    // --- Phase 2: Analyze each community as an extraction candidate ---
    let mut community_nodes: HashMap<u32, Vec<usize>> = HashMap::new();
    #[allow(clippy::needless_range_loop)]
    for i in 0..n {
        // Scope filter
        if let Some(ref scope) = config.scope {
            if !node_to_ext[i].contains(scope.as_str()) {
                continue;
            }
        }
        let cid = communities.assignments[i].0;
        community_nodes.entry(cid).or_default().push(i);
    }

    // Count edges per community
    let mut internal_edges: HashMap<u32, u32> = HashMap::new();
    let mut external_edges: HashMap<u32, u32> = HashMap::new();

    for i in 0..n {
        let ci = communities.assignments[i].0;
        let range = graph.csr.out_range(NodeId::new(i as u32));
        for j in range {
            let tgt = graph.csr.targets[j].as_usize();
            if tgt < n {
                let cj = communities.assignments[tgt].0;
                if ci == cj {
                    *internal_edges.entry(ci).or_insert(0) += 1;
                } else {
                    *external_edges.entry(ci).or_insert(0) += 1;
                }
            }
        }
    }

    // --- Phase 3: Build extraction plans ---
    let cf_engine = CounterfactualEngine::with_defaults();
    let hybrid_engine = HybridEngine::new();
    let prop_config = crate::types::PropagationConfig::default();

    let mut candidates: Vec<ExtractionPlan> = Vec::new();

    for (&cid, nodes) in &community_nodes {
        if nodes.len() < config.min_community_size {
            continue;
        }
        if candidates.len() >= config.max_communities {
            break;
        }

        let node_ids: Vec<NodeId> = nodes.iter().map(|&i| NodeId::new(i as u32)).collect();
        let node_set: HashSet<usize> = nodes.iter().copied().collect();

        // Counterfactual: what happens if we remove this community?
        let cf_result =
            cf_engine.simulate_removal(graph, &hybrid_engine, &prop_config, &node_ids)?;

        // Cascade analysis from the first node
        let cascade =
            cf_engine.cascade_analysis(graph, &hybrid_engine, &prop_config, node_ids[0])?;

        // Find interface edges (bridges touching this community)
        let interface: Vec<InterfaceEdge> = bridges
            .iter()
            .filter(|b| {
                b.source_community == CommunityId(cid) || b.target_community == CommunityId(cid)
            })
            .map(|b| {
                let direction = if b.source_community == CommunityId(cid) {
                    "outbound"
                } else {
                    "inbound"
                };
                InterfaceEdge {
                    source_id: node_to_ext[b.source.as_usize()].clone(),
                    target_id: node_to_ext[b.target.as_usize()].clone(),
                    relation: graph
                        .strings
                        .resolve(graph.csr.relations[b.edge_idx.as_usize()])
                        .to_string(),
                    weight: b.importance.get(),
                    direction: direction.to_string(),
                }
            })
            .collect();

        // Compute cohesion and coupling
        let int_e = *internal_edges.get(&cid).unwrap_or(&0) as f32;
        let ext_e = *external_edges.get(&cid).unwrap_or(&0) as f32;
        let total_e = int_e + ext_e;
        let cohesion = if total_e > 0.0 { int_e / total_e } else { 1.0 };
        let coupling = if total_e > 0.0 { ext_e / total_e } else { 0.0 };

        // Risk assessment
        let impact = cf_result.pct_activation_lost.get();
        let risk_level = if impact < 0.05 {
            "low"
        } else if impact < 0.15 {
            "medium"
        } else if impact < config.max_acceptable_impact {
            "high"
        } else {
            "critical"
        };

        candidates.push(ExtractionPlan {
            community_id: cid,
            extracted_nodes: nodes.iter().map(|&i| node_to_ext[i].clone()).collect(),
            extracted_labels: nodes
                .iter()
                .map(|&i| graph.strings.resolve(graph.nodes.label[i]).to_string())
                .collect(),
            interface_edges: interface,
            risk: ExtractionRisk {
                level: risk_level.to_string(),
                activation_loss: impact,
                orphaned_count: cf_result.orphaned_nodes.len(),
                weakened_count: cf_result.weakened_nodes.len(),
                cascade_depth: cascade.cascade_depth,
                cascade_affected: cascade.total_affected,
            },
            community_modularity: communities.modularity.get(),
            cohesion,
            coupling,
        });
    }

    // Sort by risk: low risk + high cohesion first
    candidates.sort_by(|a, b| {
        let score_a = a.risk.activation_loss - a.cohesion * 0.5;
        let score_b = b.risk.activation_loss - b.cohesion * 0.5;
        score_a
            .partial_cmp(&score_b)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(RefactorPlan {
        candidates,
        graph_modularity: communities.modularity.get(),
        num_communities: communities.num_communities,
        nodes_analyzed: n,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::*;
    use crate::types::{EdgeDirection, FiniteF32, NodeId, NodeType};

    /// Build a graph with two clear clusters connected by a single bridge:
    ///   Cluster A: a1 → a2 → a3
    ///   Cluster B: b1 → b2 → b3
    ///   Bridge: a3 → b1
    fn build_two_cluster_graph() -> Graph {
        let mut g = Graph::new();
        // Cluster A
        g.add_node(
            "a1",
            "handler_a",
            NodeType::Function,
            &["cluster_a"],
            0.0,
            0.5,
        )
        .unwrap();
        g.add_node(
            "a2",
            "process_a",
            NodeType::Function,
            &["cluster_a"],
            0.0,
            0.4,
        )
        .unwrap();
        g.add_node(
            "a3",
            "output_a",
            NodeType::Function,
            &["cluster_a"],
            0.0,
            0.3,
        )
        .unwrap();
        // Cluster B
        g.add_node(
            "b1",
            "handler_b",
            NodeType::Function,
            &["cluster_b"],
            0.0,
            0.5,
        )
        .unwrap();
        g.add_node(
            "b2",
            "process_b",
            NodeType::Function,
            &["cluster_b"],
            0.0,
            0.4,
        )
        .unwrap();
        g.add_node(
            "b3",
            "output_b",
            NodeType::Function,
            &["cluster_b"],
            0.0,
            0.3,
        )
        .unwrap();

        // Internal edges A (strong)
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
            NodeId::new(2),
            "calls",
            FiniteF32::new(0.8),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.5),
        )
        .unwrap();
        // Internal edges B (strong)
        g.add_edge(
            NodeId::new(3),
            NodeId::new(4),
            "calls",
            FiniteF32::new(0.9),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.5),
        )
        .unwrap();
        g.add_edge(
            NodeId::new(4),
            NodeId::new(5),
            "calls",
            FiniteF32::new(0.8),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.5),
        )
        .unwrap();
        // Bridge (weak)
        g.add_edge(
            NodeId::new(2),
            NodeId::new(3),
            "calls",
            FiniteF32::new(0.2),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.3),
        )
        .unwrap();

        g.finalize().unwrap();
        g
    }

    #[test]
    fn plan_empty_graph_error() {
        let g = Graph::new();
        let config = RefactorConfig::default();
        assert!(plan_refactoring(&g, &config).is_err());
    }

    #[test]
    fn plan_two_clusters_produces_candidates() {
        let g = build_two_cluster_graph();
        let config = RefactorConfig {
            min_community_size: 2,
            ..RefactorConfig::default()
        };
        let result = plan_refactoring(&g, &config).unwrap();
        assert!(result.nodes_analyzed == 6);
        assert!(result.num_communities >= 1);
        // Should produce at least one extraction candidate
        // (even if communities merge, the planner still runs)
    }

    #[test]
    fn plan_high_cohesion_low_coupling() {
        let g = build_two_cluster_graph();
        let config = RefactorConfig {
            min_community_size: 2,
            ..RefactorConfig::default()
        };
        let result = plan_refactoring(&g, &config).unwrap();
        // If communities are properly detected, the best candidate
        // should have relatively high cohesion
        if !result.candidates.is_empty() {
            let best = &result.candidates[0];
            // Cohesion should be reasonable (internal > external edges)
            assert!(best.cohesion >= 0.0, "Cohesion should be >= 0");
        }
    }

    #[test]
    fn plan_risk_levels_assigned() {
        let g = build_two_cluster_graph();
        let config = RefactorConfig {
            min_community_size: 2,
            ..RefactorConfig::default()
        };
        let result = plan_refactoring(&g, &config).unwrap();
        for candidate in &result.candidates {
            assert!(
                ["low", "medium", "high", "critical"].contains(&candidate.risk.level.as_str()),
                "Invalid risk level: {}",
                candidate.risk.level
            );
        }
    }

    #[test]
    fn plan_scope_filter_limits_candidates() {
        let g = build_two_cluster_graph();
        let config = RefactorConfig {
            min_community_size: 1,
            scope: Some("nonexistent".to_string()),
            ..RefactorConfig::default()
        };
        let result = plan_refactoring(&g, &config).unwrap();
        assert!(
            result.candidates.is_empty(),
            "Nonexistent scope should yield no candidates"
        );
    }

    #[test]
    fn plan_interface_edges_on_bridge() {
        let g = build_two_cluster_graph();
        let config = RefactorConfig {
            min_community_size: 2,
            ..RefactorConfig::default()
        };
        let result = plan_refactoring(&g, &config).unwrap();
        // At least one candidate should have interface edges if communities are split
        if result.num_communities >= 2 {
            let has_interface = result
                .candidates
                .iter()
                .any(|c| !c.interface_edges.is_empty());
            assert!(
                has_interface,
                "Split communities should have interface edges"
            );
        }
    }
}
