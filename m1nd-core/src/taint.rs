// === m1nd-core/src/taint.rs ===
// @m1nd:temponizer:WIRING — composes FlowEngine + EpidemicEngine
// @m1nd:emca:pattern — EXECUTE(compose) → MEASURE(cargo test) → CALIBRATE(api mismatch) → ADJUST(fix signatures)
// @m1nd:primitives — flow::FlowEngine, epidemic::EpidemicEngine, graph::Graph
//
// RB-02 — Graph Fuzzing / Taint Propagation
//
// Composes FlowEngine (particle propagation) + EpidemicEngine (SIR infection)
// into a unified taint analysis engine.  Inject tainted data at entry points,
// track how it propagates through the graph, and detect which security
// boundaries (validation, auth, sanitization) it crosses or misses.

use crate::epidemic::{EpidemicConfig, EpidemicDirection, EpidemicEngine, EpidemicResult};
use crate::error::{M1ndError, M1ndResult};
use crate::flow::{FlowConfig, FlowEngine, FlowSimulationResult};
use crate::graph::Graph;
use crate::types::{FiniteF32, NodeId};
use serde::Serialize;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// What kind of taint to trace.
#[derive(Clone, Debug)]
pub enum TaintType {
    /// User input data (XSS, SQL injection, command injection)
    UserInput,
    /// Sensitive data (PII, credentials, tokens)
    SensitiveData,
    /// Custom pattern set
    Custom { boundary_patterns: Vec<String> },
}

/// Configuration for taint analysis.
#[derive(Clone, Debug)]
pub struct TaintConfig {
    /// Maximum BFS/propagation depth.
    pub max_depth: u32,
    /// Number of flow particles per entry point.
    pub num_particles: u32,
    /// Epidemic simulation iterations.
    pub epidemic_iterations: u32,
    /// Minimum infection probability for a node to be flagged.
    pub min_probability: f32,
    /// Type of taint to trace (determines boundary patterns).
    pub taint_type: TaintType,
}

impl Default for TaintConfig {
    fn default() -> Self {
        Self {
            max_depth: 15,
            num_particles: 4,
            epidemic_iterations: 50,
            min_probability: 0.01,
            taint_type: TaintType::UserInput,
        }
    }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// A security boundary node (validation, auth, sanitization) that taint
/// either hit (good) or missed (bad).
#[derive(Clone, Debug, Serialize)]
pub struct BoundaryCheck {
    /// Node external ID.
    pub node_id: String,
    /// Node label.
    pub label: String,
    /// Type of boundary detected.
    pub boundary_type: String,
    /// Whether taint reached this node (true = boundary was hit).
    pub taint_reached: bool,
    /// Infection probability from epidemic simulation.
    pub infection_probability: f32,
}

/// A path where taint flows from entry to a terminus without hitting a boundary.
#[derive(Clone, Debug, Serialize)]
pub struct TaintLeak {
    /// Entry point where taint was injected.
    pub entry_node: String,
    /// Terminal node where taint arrived without boundary check.
    pub exit_node: String,
    /// Infection probability at exit.
    pub probability: f32,
    /// Transmission path from epidemic result.
    pub path: Vec<String>,
}

/// Complete result of a taint analysis.
#[derive(Clone, Debug, Serialize)]
pub struct TaintResult {
    /// Boundaries that were reached by taint (security checks that fire).
    pub boundary_hits: Vec<BoundaryCheck>,
    /// Boundaries that taint bypassed (security checks that were missed).
    pub boundary_misses: Vec<BoundaryCheck>,
    /// Paths where taint leaked through without any boundary check.
    pub leaks: Vec<TaintLeak>,
    /// Flow simulation results (turbulence = concurrent taint paths).
    pub flow_result: FlowSimulationResult,
    /// Epidemic simulation results (probability distribution).
    pub epidemic_result: EpidemicResult,
    /// Overall risk score in [0.0, 1.0].
    pub risk_score: f32,
    /// Summary statistics.
    pub summary: TaintSummary,
}

/// Aggregate statistics for taint analysis.
#[derive(Clone, Debug, Serialize)]
pub struct TaintSummary {
    pub entry_points: usize,
    pub total_nodes_reached: usize,
    pub boundary_hits: usize,
    pub boundary_misses: usize,
    pub leaks_found: usize,
    pub max_infection_probability: f32,
    pub elapsed_ms: f64,
}

// ---------------------------------------------------------------------------
// Default boundary patterns for each taint type
// ---------------------------------------------------------------------------

/// Standard patterns that indicate security boundaries in code.
fn default_boundary_patterns(taint_type: &TaintType) -> Vec<&'static str> {
    match taint_type {
        TaintType::UserInput => vec![
            "validate",
            "sanitize",
            "escape",
            "encode",
            "clean",
            "filter",
            "check_input",
            "verify",
            "parse_input",
            "whitelist",
            "allowlist",
            "blocklist",
            "blacklist",
            "csrf",
            "xss",
            "sql_injection",
            "injection",
        ],
        TaintType::SensitiveData => vec![
            "auth",
            "authenticate",
            "authorize",
            "verify_token",
            "check_permission",
            "check_role",
            "is_admin",
            "encrypt",
            "decrypt",
            "hash",
            "hmac",
            "sign",
            "mask",
            "redact",
            "anonymize",
            "obfuscate",
            "access_control",
            "permission",
            "credential",
        ],
        TaintType::Custom { boundary_patterns } => {
            // We return borrowed strs — for Custom, we handle separately
            let _ = boundary_patterns;
            vec![] // handled in is_boundary_node
        }
    }
}

// ---------------------------------------------------------------------------
// Core engine
// ---------------------------------------------------------------------------

/// Taint analysis engine composing flow + epidemic simulations.
pub struct TaintEngine;

impl TaintEngine {
    /// Run taint analysis from the given entry points.
    ///
    /// 1. Runs `FlowEngine` to discover reachable paths and turbulence.
    /// 2. Runs `EpidemicEngine` to compute infection probabilities.
    /// 3. Cross-references results against security boundary nodes.
    /// 4. Identifies leaks: paths where taint reaches sinks without boundaries.
    pub fn analyze(
        graph: &Graph,
        entry_node_ids: &[NodeId],
        config: &TaintConfig,
    ) -> M1ndResult<TaintResult> {
        let start = std::time::Instant::now();

        if entry_node_ids.is_empty() {
            return Err(M1ndError::NoEntryPoints);
        }

        // --- Phase 1: Flow simulation ---
        let flow_config = FlowConfig {
            max_depth: config.max_depth.min(255) as u8,
            ..FlowConfig::with_defaults()
        };
        let flow_engine = FlowEngine::new();
        let flow_result =
            flow_engine.simulate(graph, entry_node_ids, config.num_particles, &flow_config)?;

        // --- Phase 2: Epidemic simulation ---
        let epidemic_config = EpidemicConfig {
            iterations: config.epidemic_iterations,
            infection_rate: None,
            recovery_rate: 0.0,
            top_k: 200,
            direction: EpidemicDirection::Forward,
            // Taint analysis *wants* full propagation — disable burnout protection
            burnout_threshold: 1.0,
            // Promote only high-probability spreaders to prevent noise
            promotion_threshold: 0.5,
        };
        let epidemic_engine = EpidemicEngine::new();
        let epidemic_result = epidemic_engine.simulate(
            graph,
            entry_node_ids,
            &[], // no recovered nodes
            &epidemic_config,
        )?;

        // --- Phase 3: Identify security boundaries ---
        let boundary_patterns = default_boundary_patterns(&config.taint_type);
        let custom_patterns: Vec<String> = if let TaintType::Custom {
            boundary_patterns: cp,
        } = &config.taint_type
        {
            cp.clone()
        } else {
            vec![]
        };

        // Build reverse map: NodeId -> external_id string
        let n = graph.num_nodes() as usize;
        let mut node_to_ext: Vec<String> = vec![String::new(); n];
        for (interned, node_id) in &graph.id_to_node {
            let idx = node_id.as_usize();
            if idx < n {
                node_to_ext[idx] = graph.strings.resolve(*interned).to_string();
            }
        }

        // Build set of infected node external IDs for quick lookup
        let infected_nodes: HashSet<String> = epidemic_result
            .predictions
            .iter()
            .filter(|p| p.infection_probability >= config.min_probability)
            .map(|p| p.node_id.clone())
            .collect();

        // Scan all nodes for boundary patterns
        let mut boundary_hits = Vec::new();
        let mut boundary_misses = Vec::new();

        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            let label = graph.strings.resolve(graph.nodes.label[i]).to_lowercase();
            let ext_id = &node_to_ext[i];

            let boundary_type = detect_boundary_type(&label, &boundary_patterns, &custom_patterns);
            if let Some(btype) = boundary_type {
                let taint_reached = infected_nodes.contains(ext_id);
                let prob = epidemic_result
                    .predictions
                    .iter()
                    .find(|p| p.node_id == *ext_id)
                    .map(|p| p.infection_probability)
                    .unwrap_or(0.0);

                let check = BoundaryCheck {
                    node_id: ext_id.to_string(),
                    label: graph.strings.resolve(graph.nodes.label[i]).to_string(),
                    boundary_type: btype,
                    taint_reached,
                    infection_probability: prob,
                };

                if taint_reached {
                    boundary_hits.push(check);
                } else {
                    boundary_misses.push(check);
                }
            }
        }

        // --- Phase 4: Detect leaks ---
        // A leak is an infected node that is NOT a boundary and has no
        // boundary in its transmission path.
        let boundary_ext_ids: HashSet<&str> = boundary_hits
            .iter()
            .chain(boundary_misses.iter())
            .map(|b| b.node_id.as_str())
            .collect();

        let mut leaks = Vec::new();
        for pred in &epidemic_result.predictions {
            if pred.infection_probability < config.min_probability {
                continue;
            }
            // Skip boundary nodes themselves
            if boundary_ext_ids.contains(pred.node_id.as_str()) {
                continue;
            }
            // Check if any boundary exists in the transmission path
            let has_boundary_in_path = pred
                .transmission_path
                .iter()
                .any(|node| boundary_ext_ids.contains(node.as_str()));

            if !has_boundary_in_path && !pred.transmission_path.is_empty() {
                // Find entry point from path
                let entry = pred.transmission_path.first().cloned().unwrap_or_default();

                leaks.push(TaintLeak {
                    entry_node: entry,
                    exit_node: pred.node_id.clone(),
                    probability: pred.infection_probability,
                    path: pred.transmission_path.clone(),
                });
            }
        }

        // Sort leaks by probability descending
        leaks.sort_by(|a, b| {
            b.probability
                .partial_cmp(&a.probability)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // --- Phase 5: Compute risk score ---
        let risk_score =
            compute_risk_score(&boundary_hits, &boundary_misses, &leaks, &epidemic_result);

        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

        let summary = TaintSummary {
            entry_points: entry_node_ids.len(),
            total_nodes_reached: epidemic_result.predictions.len() + entry_node_ids.len(),
            boundary_hits: boundary_hits.len(),
            boundary_misses: boundary_misses.len(),
            leaks_found: leaks.len(),
            max_infection_probability: epidemic_result
                .predictions
                .first()
                .map(|p| p.infection_probability)
                .unwrap_or(0.0),
            elapsed_ms,
        };

        Ok(TaintResult {
            boundary_hits,
            boundary_misses,
            leaks,
            flow_result,
            epidemic_result,
            risk_score,
            summary,
        })
    }
}

/// Detect if a node label matches any boundary pattern.
fn detect_boundary_type(
    label_lower: &str,
    default_patterns: &[&str],
    custom_patterns: &[String],
) -> Option<String> {
    for pattern in default_patterns {
        if label_lower.contains(pattern) {
            return Some(pattern.to_string());
        }
    }
    for pattern in custom_patterns {
        if label_lower.contains(&pattern.to_lowercase()) {
            return Some(pattern.clone());
        }
    }
    None
}

/// Compute a composite risk score based on boundary coverage and leaks.
fn compute_risk_score(
    hits: &[BoundaryCheck],
    misses: &[BoundaryCheck],
    leaks: &[TaintLeak],
    epidemic: &EpidemicResult,
) -> f32 {
    let total_boundaries = hits.len() + misses.len();

    // Factor 1: Boundary miss ratio (0 = all hit, 1 = all missed)
    let miss_ratio = if total_boundaries > 0 {
        misses.len() as f32 / total_boundaries as f32
    } else {
        0.5 // No boundaries found — medium risk (unknown)
    };

    // Factor 2: Leak severity (logarithmic scale)
    let leak_factor = if leaks.is_empty() {
        0.0
    } else {
        (1.0 + leaks.len() as f32).ln() / (1.0 + 100.0_f32).ln() // normalized to ~1.0 at 100 leaks
    };

    // Factor 3: Spread factor (how far the epidemic got)
    let spread_factor = if epidemic.summary.total_infected > 0 {
        let total = epidemic.summary.total_susceptible
            + epidemic.summary.total_infected
            + epidemic.summary.total_recovered;
        if total > 0 {
            epidemic.summary.total_infected as f32 / total as f32
        } else {
            0.0
        }
    } else {
        0.0
    };

    // Weighted combination
    let score = 0.4 * miss_ratio + 0.35 * leak_factor + 0.25 * spread_factor;
    score.clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::*;
    use crate::types::{EdgeDirection, FiniteF32, NodeId, NodeType};

    /// Build a simple graph: entry → process → output (no boundary)
    fn build_no_boundary_graph() -> Graph {
        let mut g = Graph::new();
        g.add_node(
            "entry",
            "handle_request",
            NodeType::Function,
            &["handler"],
            0.0,
            0.5,
        )
        .unwrap();
        g.add_node(
            "proc",
            "process_data",
            NodeType::Function,
            &["data"],
            0.0,
            0.3,
        )
        .unwrap();
        g.add_node(
            "out",
            "send_response",
            NodeType::Function,
            &["output"],
            0.0,
            0.2,
        )
        .unwrap();

        g.add_edge(
            NodeId::new(0),
            NodeId::new(1),
            "calls",
            FiniteF32::new(0.8),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.5),
        )
        .unwrap();
        g.add_edge(
            NodeId::new(1),
            NodeId::new(2),
            "calls",
            FiniteF32::new(0.7),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.4),
        )
        .unwrap();

        g.finalize().unwrap();
        g
    }

    /// Build a graph with a validation boundary: entry → validate → process → output
    fn build_with_boundary_graph() -> Graph {
        let mut g = Graph::new();
        g.add_node(
            "entry",
            "handle_request",
            NodeType::Function,
            &["handler"],
            0.0,
            0.5,
        )
        .unwrap();
        g.add_node(
            "val",
            "validate_input",
            NodeType::Function,
            &["security"],
            0.0,
            0.4,
        )
        .unwrap();
        g.add_node(
            "proc",
            "process_data",
            NodeType::Function,
            &["data"],
            0.0,
            0.3,
        )
        .unwrap();
        g.add_node(
            "out",
            "send_response",
            NodeType::Function,
            &["output"],
            0.0,
            0.2,
        )
        .unwrap();

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
            FiniteF32::new(0.4),
        )
        .unwrap();
        g.add_edge(
            NodeId::new(2),
            NodeId::new(3),
            "calls",
            FiniteF32::new(0.7),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.4),
        )
        .unwrap();

        g.finalize().unwrap();
        g
    }

    #[test]
    fn detect_boundary_validates() {
        let patterns = vec!["validate", "sanitize", "auth"];
        let custom: Vec<String> = vec![];
        assert!(detect_boundary_type("validate_input", &patterns, &custom).is_some());
        assert!(detect_boundary_type("process_data", &patterns, &custom).is_none());
        assert!(detect_boundary_type("sanitize_html", &patterns, &custom).is_some());
    }

    #[test]
    fn risk_score_no_boundaries_is_medium() {
        let score = compute_risk_score(
            &[],
            &[],
            &[],
            &EpidemicResult {
                predictions: vec![],
                summary: crate::epidemic::EpidemicSummary {
                    total_susceptible: 10,
                    total_infected: 0,
                    total_recovered: 0,
                    peak_infection_iteration: 0,
                    r0_estimate: 0.0,
                    epidemic_extinct: true,
                },
                unreachable_components: vec![],
                warnings: vec![],
                unresolved_nodes: vec![],
                elapsed_ms: 1.0,
            },
        );
        assert!(
            (score - 0.2).abs() < 0.1,
            "No boundaries = medium risk, got {score}"
        );
    }

    #[test]
    fn taint_no_boundary_graph_analysis() {
        let g = build_no_boundary_graph();
        let config = TaintConfig::default();
        let result = TaintEngine::analyze(&g, &[NodeId::new(0)], &config).unwrap();
        // With no boundary nodes, all should pass through
        assert_eq!(result.summary.entry_points, 1);
        assert!(result.summary.total_nodes_reached > 0);
    }

    #[test]
    fn taint_with_boundary_detects_validation() {
        let g = build_with_boundary_graph();
        let config = TaintConfig::default();
        let result = TaintEngine::analyze(&g, &[NodeId::new(0)], &config).unwrap();
        // validate_input should be detected as a boundary
        let all_boundaries: Vec<_> = result
            .boundary_hits
            .iter()
            .chain(result.boundary_misses.iter())
            .collect();
        assert!(
            all_boundaries.iter().any(|b| b.label.contains("validate")),
            "Should detect validate_input as boundary, found: {:?}",
            all_boundaries
        );
    }

    #[test]
    fn taint_empty_entry_returns_error() {
        let g = build_no_boundary_graph();
        let config = TaintConfig::default();
        let result = TaintEngine::analyze(&g, &[], &config);
        assert!(result.is_err());
    }

    #[test]
    fn risk_score_bounded_zero_to_one() {
        let leak = TaintLeak {
            entry_node: "a".into(),
            exit_node: "b".into(),
            probability: 0.9,
            path: vec!["a".into(), "b".into()],
        };
        let miss = BoundaryCheck {
            node_id: "x".into(),
            label: "validate".into(),
            boundary_type: "validate".into(),
            taint_reached: false,
            infection_probability: 0.0,
        };
        let score = compute_risk_score(
            &[],
            &[miss],
            &[leak],
            &EpidemicResult {
                predictions: vec![],
                summary: crate::epidemic::EpidemicSummary {
                    total_susceptible: 5,
                    total_infected: 5,
                    total_recovered: 0,
                    peak_infection_iteration: 10,
                    r0_estimate: 2.0,
                    epidemic_extinct: false,
                },
                unreachable_components: vec![],
                warnings: vec![],
                unresolved_nodes: vec![],
                elapsed_ms: 1.0,
            },
        );
        assert!(
            (0.0..=1.0).contains(&score),
            "Risk score out of range: {score}"
        );
        assert!(
            score > 0.5,
            "All misses + leaks + high spread should be high risk, got {score}"
        );
    }
}
