// === m1nd-mcp/src/engine_ops.rs ===
// Theme 7: Route Synthesis Engine API Layer.
// Read-only wrappers around engine operations for perspective synthesis.
// These take &SessionState (immutable), do NOT increment queries_processed,
// do NOT trigger plasticity side effects, hold a single graph read lock.

use crate::result_shaping::{dedupe_ranked, RankedResult};
use crate::session::SessionState;
use m1nd_core::activation::{
    ActivatedNode as CoreActivatedNode, ActivationEngine, ActivationResult,
};
use m1nd_core::error::M1ndResult;
use m1nd_core::seed::SeedFinder;
use m1nd_core::types::{FiniteF32, NodeId, PropagationConfig};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Config types
// ---------------------------------------------------------------------------

/// Configuration for read-only activate.
#[derive(Clone, Debug)]
pub struct ActivateConfig {
    pub top_k: usize,
    pub dimensions: Vec<String>,
    pub xlr: bool,
    pub include_ghost_edges: bool,
    pub include_structural_holes: bool,
}

impl Default for ActivateConfig {
    fn default() -> Self {
        Self {
            top_k: 8, // perspective default, not activate's 20
            dimensions: vec![
                "structural".into(),
                "semantic".into(),
                "temporal".into(),
                "causal".into(),
            ],
            xlr: true,
            include_ghost_edges: true,
            include_structural_holes: true,
        }
    }
}

#[derive(Clone, Copy, Default)]
struct ReadOnlyScore {
    structural: f32,
    semantic: f32,
    xlr: f32,
}

fn node_label(graph: &m1nd_core::graph::Graph, node: NodeId) -> String {
    let idx = node.as_usize();
    if idx < graph.num_nodes() as usize {
        graph.strings.resolve(graph.nodes.label[idx]).to_string()
    } else {
        format!("node_{}", idx)
    }
}

fn node_type_string(graph: &m1nd_core::graph::Graph, node: NodeId) -> String {
    let idx = node.as_usize();
    if idx < graph.num_nodes() as usize {
        format!("{:?}", graph.nodes.node_type[idx])
    } else {
        "Unknown".into()
    }
}

/// Direction for impact analysis.
#[derive(Clone, Debug)]
pub enum ImpactDirection {
    Forward,
    Reverse,
    Both,
}

// ---------------------------------------------------------------------------
// Result types (lightweight, perspective-internal)
// ---------------------------------------------------------------------------

/// Activated node result for perspective synthesis.
#[derive(Clone, Debug)]
pub struct ActivatedNode {
    pub node_id: String,
    pub label: String,
    pub node_type: String,
    pub activation: f32,
    pub pagerank: f32,
    pub source_path: Option<String>,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
}

impl RankedResult for ActivatedNode {
    fn score(&self) -> f32 {
        self.activation
    }

    fn specificity(&self) -> f32 {
        let mut score = match self.node_type.as_str() {
            "Function" => 2.0,
            "Struct" | "Type" | "Enum" => 1.9,
            "Module" => 1.1,
            "File" => 0.6,
            "Directory" => 0.1,
            _ => 0.4,
        };

        let label_lower = self.label.trim().to_lowercase();
        if label_lower.starts_with("impl ") {
            score += 3.0;
        }

        let source_path_lower = self.source_path.as_deref().unwrap_or("").to_lowercase();
        if source_path_lower.contains("/src/") || source_path_lower.contains("/tests/") {
            score += 0.5;
        }
        if source_path_lower.contains("/docs/")
            || source_path_lower.contains("/wiki/")
            || source_path_lower.contains("readme")
            || source_path_lower.contains("changelog")
            || source_path_lower.contains("tutorial")
        {
            score -= 0.8;
        }
        if source_path_lower.contains("cargo.toml") {
            score -= 1.2;
        }

        score
    }

    fn family_key(&self) -> String {
        let label = self.label.trim();
        if let Some(rest) = label.strip_prefix("impl ") {
            if let Some((trait_part, _)) = rest.split_once(" for ") {
                return format!("impl:{}", trait_part.trim().to_lowercase());
            }
            return format!("impl:{}", rest.trim().to_lowercase());
        }

        if self
            .source_path
            .as_deref()
            .map(|path| path.to_lowercase().contains("cargo.toml"))
            .unwrap_or(false)
        {
            return format!("crate:{}", label.to_lowercase());
        }

        label.to_lowercase()
    }
}

/// Read-only activate result.
#[derive(Clone, Debug)]
pub struct ActivateResult {
    pub nodes: Vec<ActivatedNode>,
    pub ghost_edges: Vec<GhostEdge>,
    pub structural_holes: Vec<StructuralHole>,
    pub elapsed_ms: f64,
}

#[derive(Clone, Debug)]
pub struct GhostEdge {
    pub source: String,
    pub target: String,
    pub strength: f32,
}

#[derive(Clone, Debug)]
pub struct StructuralHole {
    pub node_id: String,
    pub label: String,
    pub node_type: String,
    pub reason: String,
}

/// Read-only impact result.
#[derive(Clone, Debug)]
pub struct ImpactResult {
    pub blast_radius: Vec<ImpactEntry>,
    pub total_energy: f32,
}

#[derive(Clone, Debug)]
pub struct ImpactEntry {
    pub node_id: String,
    pub label: String,
    pub signal_strength: f32,
    pub hop_distance: u8,
}

/// Read-only missing result.
#[derive(Clone, Debug)]
pub struct MissingResult {
    pub holes: Vec<StructuralHole>,
}

/// Read-only why result.
#[derive(Clone, Debug)]
pub struct WhyResult {
    pub paths: Vec<WhyPath>,
}

#[derive(Clone, Debug)]
pub struct WhyPath {
    pub nodes: Vec<String>,
    pub relations: Vec<String>,
    pub cumulative_strength: f32,
}

/// Read-only resonate result.
#[derive(Clone, Debug)]
pub struct ResonateResult {
    pub harmonics: Vec<HarmonicEntry>,
}

#[derive(Clone, Debug)]
pub struct HarmonicEntry {
    pub node_id: String,
    pub label: String,
    pub amplitude: f32,
}

// ---------------------------------------------------------------------------
// Read-only engine operations (Theme 7)
// ---------------------------------------------------------------------------

/// Read-only activate. No query counting, no plasticity side effects.
/// Holds a single graph read lock for the entire call.
///
/// Performance budget for route synthesis: max 8 underlying calls total.
/// This counts as 1 call.
pub fn activate_readonly(
    state: &SessionState,
    query: &str,
    config: ActivateConfig,
) -> M1ndResult<ActivateResult> {
    let start = std::time::Instant::now();
    let graph = state.graph.read();
    let top_k = config.top_k.max(1);
    let seed_budget = top_k.saturating_mul(5).max(top_k);
    let seeds = SeedFinder::find_seeds(&graph, query, seed_budget)?;

    if seeds.is_empty() {
        return Ok(ActivateResult {
            nodes: Vec::new(),
            ghost_edges: Vec::new(),
            structural_holes: Vec::new(),
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        });
    }

    let use_structural =
        config.dimensions.is_empty() || config.dimensions.iter().any(|d| d == "structural");
    let use_semantic =
        config.dimensions.is_empty() || config.dimensions.iter().any(|d| d == "semantic");
    let use_xlr = config.xlr && use_structural;

    let mut scores: HashMap<NodeId, ReadOnlyScore> = HashMap::new();

    if use_structural {
        let structural =
            state
                .orchestrator
                .engine
                .propagate(&graph, &seeds, &PropagationConfig::default())?;
        for (node, score) in structural.scores {
            scores.entry(node).or_default().structural = score.get();
        }
    }

    if use_semantic {
        for (node, score) in state
            .orchestrator
            .semantic
            .query(&graph, query, seed_budget)?
        {
            scores.entry(node).or_default().semantic = score.get();
        }
    }

    let mut xlr_fallback_used = false;
    if use_xlr {
        let xlr = state
            .orchestrator
            .xlr
            .query(&graph, &seeds, &PropagationConfig::default())?;
        xlr_fallback_used = xlr.fallback_to_hot_only;
        for (node, score) in xlr.activations {
            scores.entry(node).or_default().xlr = score.get();
        }
    }

    let mut activated: Vec<CoreActivatedNode> = scores
        .into_iter()
        .filter_map(|(node, score)| {
            let weights = [
                if use_structural { 0.60 } else { 0.0 },
                if use_semantic { 0.30 } else { 0.0 },
                0.0,
                if use_xlr { 0.10 } else { 0.0 },
            ];
            let weight_sum: f32 = weights.iter().sum();
            if weight_sum <= 0.0 {
                return None;
            }

            let activation = (score.structural * weights[0]
                + score.semantic * weights[1]
                + score.xlr * weights[3])
                / weight_sum;

            if activation <= 0.0 {
                return None;
            }

            let dimensions = [
                FiniteF32::new(score.structural),
                FiniteF32::new(score.semantic),
                FiniteF32::ZERO,
                FiniteF32::new(score.xlr),
            ];
            let active_dimension_count =
                dimensions.iter().filter(|dim| dim.get() > 0.01).count() as u8;

            Some(CoreActivatedNode {
                node,
                activation: FiniteF32::new(activation.min(1.0)),
                dimensions,
                active_dimension_count,
            })
        })
        .collect();

    activated.sort_by(|a, b| b.activation.cmp(&a.activation));
    activated.truncate(top_k);

    let activation = ActivationResult {
        activated,
        seeds: seeds.clone(),
        elapsed_ns: start.elapsed().as_nanos() as u64,
        xlr_fallback_used,
    };

    let ghost_edges = if config.include_ghost_edges {
        state.orchestrator.detect_ghost_edges(&graph, &activation)?
    } else {
        Vec::new()
    };

    let structural_holes = if config.include_structural_holes {
        state
            .orchestrator
            .detect_structural_holes(&graph, &activation, FiniteF32::new(0.3))?
    } else {
        Vec::new()
    };

    let nodes = activation
        .activated
        .iter()
        .map(|node| {
            let idx = node.node.as_usize();
            let provenance = graph.resolve_node_provenance(node.node);
            ActivatedNode {
                node_id: node_label(&graph, node.node),
                label: node_label(&graph, node.node),
                node_type: node_type_string(&graph, node.node),
                activation: node.activation.get(),
                pagerank: if idx < graph.nodes.pagerank.len() {
                    graph.nodes.pagerank[idx].get()
                } else {
                    0.0
                },
                source_path: provenance.source_path,
                line_start: provenance.line_start,
                line_end: provenance.line_end,
            }
        })
        .collect();

    let nodes = dedupe_ranked(nodes, top_k);

    Ok(ActivateResult {
        nodes,
        ghost_edges: ghost_edges
            .into_iter()
            .map(|edge| GhostEdge {
                source: node_label(&graph, edge.source),
                target: node_label(&graph, edge.target),
                strength: edge.strength.get(),
            })
            .collect(),
        structural_holes: structural_holes
            .into_iter()
            .map(|hole| StructuralHole {
                node_id: node_label(&graph, hole.node),
                label: node_label(&graph, hole.node),
                node_type: "structural_hole".into(),
                reason: hole.reason,
            })
            .collect(),
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

/// Read-only impact. No side effects.
pub fn impact_readonly(
    state: &SessionState,
    node: &str,
    direction: ImpactDirection,
) -> M1ndResult<ImpactResult> {
    todo!("impact_readonly: extract from tools::handle_impact")
}

/// Read-only missing. No side effects.
pub fn missing_readonly(state: &SessionState, query: &str) -> M1ndResult<MissingResult> {
    todo!("missing_readonly: extract from tools::handle_missing")
}

/// Read-only why. No side effects.
pub fn why_readonly(state: &SessionState, from: &str, to: &str) -> M1ndResult<WhyResult> {
    todo!("why_readonly: extract from tools::handle_why")
}

/// Read-only resonate. Only called if remaining budget allows after first 8 calls.
pub fn resonate_readonly(state: &SessionState, query: &str) -> M1ndResult<ResonateResult> {
    todo!("resonate_readonly: extract from tools::handle_resonate")
}

// ---------------------------------------------------------------------------
// Budget tracking for route synthesis
// ---------------------------------------------------------------------------

/// Track call budget during route synthesis.
/// Max 8 calls (1 activate + 3 impact + 3 why + 1 missing).
/// Wall-clock timeout: 500ms.
pub struct SynthesisBudget {
    pub max_calls: u32,
    pub calls_used: u32,
    pub timeout_ms: f64,
    pub start_time: std::time::Instant,
}

impl Default for SynthesisBudget {
    fn default() -> Self {
        Self::new()
    }
}

impl SynthesisBudget {
    pub fn new() -> Self {
        Self {
            max_calls: 8,
            calls_used: 0,
            timeout_ms: 500.0,
            start_time: std::time::Instant::now(),
        }
    }

    /// Check if another call is within budget.
    pub fn can_call(&self) -> bool {
        self.calls_used < self.max_calls && self.elapsed_ms() < self.timeout_ms
    }

    /// Record a call.
    pub fn record_call(&mut self) {
        self.calls_used += 1;
    }

    /// Elapsed time in ms.
    pub fn elapsed_ms(&self) -> f64 {
        self.start_time.elapsed().as_secs_f64() * 1000.0
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::McpConfig;
    use crate::session::SessionState;
    use m1nd_core::builder::GraphBuilder;
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::types::NodeType;
    use std::path::PathBuf;

    #[test]
    fn budget_starts_with_capacity() {
        let budget = SynthesisBudget::new();
        assert!(budget.can_call());
        assert_eq!(budget.calls_used, 0);
        assert_eq!(budget.max_calls, 8);
    }

    #[test]
    fn budget_exhausts() {
        let mut budget = SynthesisBudget::new();
        for _ in 0..8 {
            assert!(budget.can_call());
            budget.record_call();
        }
        assert!(!budget.can_call());
    }

    #[test]
    fn activate_config_defaults() {
        let config = ActivateConfig::default();
        assert_eq!(config.top_k, 8); // perspective-specific
        assert_eq!(config.dimensions.len(), 4);
    }

    #[test]
    fn activate_readonly_keeps_live_state_unchanged() {
        let mut builder = GraphBuilder::new();
        builder
            .add_node("file::alpha", "Alpha", NodeType::Function, &["alpha"])
            .unwrap();
        builder
            .add_node("file::beta", "Beta", NodeType::Function, &["beta"])
            .unwrap();
        let graph = builder.finalize().unwrap();

        let unique = format!(
            "m1nd-readonly-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time must be after UNIX_EPOCH")
                .as_nanos()
        );
        let base = std::env::temp_dir().join(unique);
        let config = McpConfig {
            graph_source: PathBuf::from(base.join("graph.json")),
            plasticity_state: PathBuf::from(base.join("plasticity.json")),
            ..McpConfig::default()
        };

        let state = SessionState::initialize(graph, &config, DomainConfig::code()).unwrap();
        let before_generation = state.graph.read().generation;
        let before_queries = state.queries_processed;

        let result = activate_readonly(&state, "Alpha", ActivateConfig::default()).unwrap();

        assert!(
            !result.nodes.is_empty(),
            "read-only activate should return nodes"
        );
        assert_eq!(state.queries_processed, before_queries);
        assert_eq!(state.graph.read().generation, before_generation);
    }
}
