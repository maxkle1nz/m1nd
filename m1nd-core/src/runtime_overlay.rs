// === m1nd-core/src/runtime_overlay.rs ===
// @m1nd:temponizer:HARDENING — new ingestion pipeline + node matching + heat decay
// @m1nd:emca:pattern — EXECUTE(ingest) → MEASURE(map test) → CALIBRATE(LabelMatch bug) → ADJUST(ratio guard)
// @m1nd:primitives — graph::Graph (activation arrays, SoA NodeStorage)
//
// RB-05 — OpenTelemetry Overlay: runtime heat ingestion.
//
// Ingests OpenTelemetry-format trace/span data and maps it onto graph nodes,
// creating a runtime heat overlay that boosts activation scoring for
// runtime-hot code paths. This bridges the gap between static structure
// and actual runtime behavior.

use crate::error::{M1ndError, M1ndResult};
use crate::graph::Graph;
use crate::types::{FiniteF32, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for runtime overlay ingestion.
#[derive(Clone, Debug)]
pub struct OverlayConfig {
    /// How to map span names to graph nodes.
    pub mapping_strategy: MappingStrategy,
    /// Decay factor for old observations [0.0, 1.0].
    /// Lower values = faster decay of old runtime data.
    pub decay_factor: f32,
    /// Maximum heat score (cap to prevent runaway values).
    pub max_heat: f32,
    /// Minimum span duration (µs) to consider significant.
    pub min_duration_us: u64,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            mapping_strategy: MappingStrategy::LabelMatch,
            decay_factor: 0.9,
            max_heat: 10.0,
            min_duration_us: 0,
        }
    }
}

/// Strategy for mapping OTel span names to graph nodes.
#[derive(Clone, Debug, PartialEq)]
pub enum MappingStrategy {
    /// Match span name against node labels (case-insensitive substring).
    LabelMatch,
    /// Match span attributes["code.function"] → node external_id.
    CodeAttribute,
    /// Exact match against external_id.
    ExactId,
}

// ---------------------------------------------------------------------------
// Input types (OpenTelemetry-compatible)
// ---------------------------------------------------------------------------

/// A single span from an OpenTelemetry trace.
#[derive(Clone, Debug, Deserialize)]
pub struct OtelSpan {
    /// Span name (usually the function/operation name).
    pub name: String,
    /// Duration in microseconds.
    pub duration_us: u64,
    /// Number of times this span was observed.
    #[serde(default = "default_count")]
    pub count: u64,
    /// Optional error flag.
    #[serde(default)]
    pub is_error: bool,
    /// Optional attributes for code-level mapping.
    #[serde(default)]
    pub attributes: HashMap<String, String>,
    /// Parent span name (for call-chain reconstruction).
    pub parent: Option<String>,
}

fn default_count() -> u64 {
    1
}

/// A batch of OTel spans to ingest.
#[derive(Clone, Debug, Deserialize)]
pub struct OtelBatch {
    /// Spans to ingest.
    pub spans: Vec<OtelSpan>,
    /// Timestamp of the batch (Unix seconds).
    #[serde(default)]
    pub timestamp: f64,
    /// Service name for scoping.
    #[serde(default)]
    pub service_name: String,
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Per-node runtime heat data.
#[derive(Clone, Debug, Serialize)]
pub struct NodeHeat {
    /// Graph node external ID.
    pub node_id: String,
    /// Node label.
    pub label: String,
    /// Accumulated heat score.
    pub heat: f32,
    /// Total invocation count from traces.
    pub invocation_count: u64,
    /// Total error count from traces.
    pub error_count: u64,
    /// Average duration in µs.
    pub avg_duration_us: f64,
    /// P99 duration (approximated as max observed).
    pub max_duration_us: u64,
}

/// Result of runtime overlay ingestion.
#[derive(Clone, Debug, Serialize)]
pub struct OverlayResult {
    /// Number of spans processed.
    pub spans_processed: usize,
    /// Number of spans mapped to graph nodes.
    pub spans_mapped: usize,
    /// Number of spans that couldn't be mapped.
    pub spans_unmapped: usize,
    /// Per-node heat data (sorted by heat descending).
    pub hot_nodes: Vec<NodeHeat>,
    /// Activation boosts applied.
    pub boosts_applied: usize,
    /// Elapsed time in ms.
    pub elapsed_ms: f64,
}

// ---------------------------------------------------------------------------
// Runtime overlay store
// ---------------------------------------------------------------------------

/// Accumulated runtime data for a single node.
#[derive(Clone, Debug, Default)]
struct NodeRuntimeData {
    heat: f32,
    invocations: u64,
    errors: u64,
    total_duration_us: u64,
    max_duration_us: u64,
}

/// The runtime overlay engine. Maintains runtime heat state across
/// multiple ingestion batches.
#[derive(Clone, Debug)]
pub struct RuntimeOverlay {
    /// Per-node runtime data, keyed by NodeId index.
    node_data: HashMap<usize, NodeRuntimeData>,
    /// Configuration.
    config: OverlayConfig,
    /// Number of batches ingested.
    batches_ingested: u32,
}

impl RuntimeOverlay {
    /// Create a new runtime overlay engine.
    pub fn new(config: OverlayConfig) -> Self {
        Self {
            node_data: HashMap::new(),
            config,
            batches_ingested: 0,
        }
    }

    /// Create with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(OverlayConfig::default())
    }

    /// Ingest an OTel batch and map spans to graph nodes.
    pub fn ingest(&mut self, graph: &Graph, batch: &OtelBatch) -> M1ndResult<OverlayResult> {
        let start = std::time::Instant::now();
        let n = graph.num_nodes() as usize;

        if n == 0 {
            return Err(M1ndError::EmptyGraph);
        }

        // Decay existing heat values
        if self.batches_ingested > 0 {
            for data in self.node_data.values_mut() {
                data.heat *= self.config.decay_factor;
            }
        }

        // Build label lookup: label → NodeId index
        let mut label_to_idx: HashMap<String, Vec<usize>> = HashMap::new();
        let mut ext_to_idx: HashMap<String, usize> = HashMap::new();

        for (interned, node_id) in &graph.id_to_node {
            let idx = node_id.as_usize();
            if idx < n {
                let ext_id = graph.strings.resolve(*interned).to_string();
                ext_to_idx.insert(ext_id, idx);
            }
        }

        for i in 0..n {
            let label = graph.strings.resolve(graph.nodes.label[i]).to_lowercase();
            label_to_idx.entry(label).or_default().push(i);
        }

        let mut spans_mapped = 0usize;
        let mut spans_unmapped = 0usize;

        for span in &batch.spans {
            // Skip spans below minimum duration
            if span.duration_us < self.config.min_duration_us {
                continue;
            }

            // Map span to graph node(s)
            let matched_indices = match self.config.mapping_strategy {
                MappingStrategy::LabelMatch => {
                    let name_lower = span.name.to_lowercase();
                    match label_to_idx.get(&name_lower) {
                        Some(indices) => indices.clone(),
                        None => {
                            // Substring match with quality guard:
                            // - Both strings must be ≥ MIN_LEN for substring to fire
                            // - The shorter string must be ≥ 50% of the longer string
                            //   to prevent "span" from matching "nonexistent_span_500"
                            const MIN_LEN: usize = 4;
                            let mut matches = Vec::new();
                            if name_lower.len() >= MIN_LEN {
                                for (label, indices) in &label_to_idx {
                                    if label.len() < MIN_LEN {
                                        continue;
                                    }
                                    let shorter = label.len().min(name_lower.len());
                                    let longer = label.len().max(name_lower.len());
                                    // Ratio guard: prevent "span" (4) matching "nonexistent_span_500" (22)
                                    if shorter * 2 < longer {
                                        continue;
                                    }
                                    if label.contains(name_lower.as_str())
                                        || name_lower.contains(label.as_str())
                                    {
                                        matches.extend_from_slice(indices);
                                    }
                                }
                            }
                            matches
                        }
                    }
                }
                MappingStrategy::CodeAttribute => {
                    if let Some(func_name) = span.attributes.get("code.function") {
                        let func_lower = func_name.to_lowercase();
                        label_to_idx.get(&func_lower).cloned().unwrap_or_default()
                    } else {
                        Vec::new()
                    }
                }
                MappingStrategy::ExactId => ext_to_idx
                    .get(&span.name)
                    .map(|&idx| vec![idx])
                    .unwrap_or_default(),
            };

            if matched_indices.is_empty() {
                spans_unmapped += 1;
                continue;
            }

            spans_mapped += 1;

            // Accumulate heat data
            let heat_increment = (span.count as f32).ln_1p()
                * (1.0 + (span.duration_us as f64 / 1_000_000.0) as f32);

            for &idx in &matched_indices {
                let data = self.node_data.entry(idx).or_default();
                data.heat = (data.heat + heat_increment).min(self.config.max_heat);
                data.invocations += span.count;
                data.total_duration_us += span.duration_us * span.count;
                data.max_duration_us = data.max_duration_us.max(span.duration_us);
                if span.is_error {
                    data.errors += span.count;
                }
            }
        }

        self.batches_ingested += 1;

        // Build reverse map for output
        let mut node_to_ext: Vec<String> = vec![String::new(); n];
        for (interned, node_id) in &graph.id_to_node {
            let idx = node_id.as_usize();
            if idx < n {
                node_to_ext[idx] = graph.strings.resolve(*interned).to_string();
            }
        }

        // Collect hot nodes
        let mut hot_nodes: Vec<NodeHeat> = self
            .node_data
            .iter()
            .filter(|(_, data)| data.heat > 0.01)
            .map(|(&idx, data)| {
                let avg_dur = if data.invocations > 0 {
                    data.total_duration_us as f64 / data.invocations as f64
                } else {
                    0.0
                };
                NodeHeat {
                    node_id: if idx < n {
                        node_to_ext[idx].clone()
                    } else {
                        String::new()
                    },
                    label: if idx < n {
                        graph.strings.resolve(graph.nodes.label[idx]).to_string()
                    } else {
                        String::new()
                    },
                    heat: data.heat,
                    invocation_count: data.invocations,
                    error_count: data.errors,
                    avg_duration_us: avg_dur,
                    max_duration_us: data.max_duration_us,
                }
            })
            .collect();

        hot_nodes.sort_by(|a, b| {
            b.heat
                .partial_cmp(&a.heat)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hot_nodes.truncate(100);

        Ok(OverlayResult {
            spans_processed: batch.spans.len(),
            spans_mapped,
            spans_unmapped,
            hot_nodes,
            boosts_applied: spans_mapped,
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        })
    }

    /// Apply runtime heat as activation boosts to the graph.
    /// Hot nodes get their structural activation dimension boosted
    /// proportionally to their runtime heat.
    pub fn apply_boosts(&self, graph: &mut Graph, boost_strength: f32) -> usize {
        let n = graph.num_nodes() as usize;
        let mut applied = 0usize;

        for (&idx, data) in &self.node_data {
            if idx >= n || data.heat < 0.01 {
                continue;
            }
            // Boost the structural activation dimension (index 0)
            let current = graph.nodes.activation[idx][0].get();
            let boost = data.heat * boost_strength;
            let new_val = (current + boost).min(1.0);
            graph.nodes.activation[idx][0] = FiniteF32::new(new_val);
            applied += 1;
        }

        applied
    }

    /// Get current heat for a node by index.
    pub fn get_heat(&self, node_idx: usize) -> f32 {
        self.node_data.get(&node_idx).map(|d| d.heat).unwrap_or(0.0)
    }

    /// Get number of batches ingested.
    pub fn batches_ingested(&self) -> u32 {
        self.batches_ingested
    }

    /// Reset all runtime data.
    pub fn reset(&mut self) {
        self.node_data.clear();
        self.batches_ingested = 0;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::*;
    use crate::types::{EdgeDirection, FiniteF32, NodeId, NodeType};

    fn build_test_graph() -> Graph {
        let mut g = Graph::new();
        g.add_node(
            "func::handle_request",
            "handle_request",
            NodeType::Function,
            &["handler"],
            0.0,
            0.5,
        )
        .unwrap();
        g.add_node(
            "func::process_data",
            "process_data",
            NodeType::Function,
            &["data"],
            0.0,
            0.3,
        )
        .unwrap();
        g.add_node(
            "func::send_response",
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

    fn build_test_batch() -> OtelBatch {
        OtelBatch {
            spans: vec![
                OtelSpan {
                    name: "handle_request".to_string(),
                    duration_us: 5000,
                    count: 100,
                    is_error: false,
                    attributes: HashMap::new(),
                    parent: None,
                },
                OtelSpan {
                    name: "process_data".to_string(),
                    duration_us: 3000,
                    count: 95,
                    is_error: false,
                    attributes: HashMap::new(),
                    parent: Some("handle_request".to_string()),
                },
                OtelSpan {
                    name: "send_response".to_string(),
                    duration_us: 1000,
                    count: 90,
                    is_error: true,
                    attributes: HashMap::new(),
                    parent: Some("process_data".to_string()),
                },
            ],
            timestamp: 1700000000.0,
            service_name: "test-service".to_string(),
        }
    }

    #[test]
    fn empty_graph_returns_error() {
        let g = Graph::new();
        let mut overlay = RuntimeOverlay::with_defaults();
        let batch = build_test_batch();
        assert!(overlay.ingest(&g, &batch).is_err());
    }

    #[test]
    fn ingest_maps_spans_to_nodes() {
        let g = build_test_graph();
        let mut overlay = RuntimeOverlay::with_defaults();
        let batch = build_test_batch();

        let result = overlay.ingest(&g, &batch).unwrap();
        assert_eq!(result.spans_processed, 3);
        assert!(result.spans_mapped > 0, "Should map at least some spans");
    }

    #[test]
    fn hot_nodes_sorted_by_heat() {
        let g = build_test_graph();
        let mut overlay = RuntimeOverlay::with_defaults();
        let batch = build_test_batch();

        let result = overlay.ingest(&g, &batch).unwrap();
        for window in result.hot_nodes.windows(2) {
            assert!(
                window[0].heat >= window[1].heat,
                "Hot nodes should be sorted by heat desc"
            );
        }
    }

    #[test]
    fn error_spans_tracked() {
        let g = build_test_graph();
        let mut overlay = RuntimeOverlay::with_defaults();
        let batch = build_test_batch();

        let result = overlay.ingest(&g, &batch).unwrap();
        // send_response has is_error=true
        let error_node = result.hot_nodes.iter().find(|n| n.label == "send_response");
        if let Some(node) = error_node {
            assert!(node.error_count > 0, "Error spans should be tracked");
        }
    }

    #[test]
    fn decay_reduces_old_heat() {
        let g = build_test_graph();
        let config = OverlayConfig {
            decay_factor: 0.5,
            ..OverlayConfig::default()
        };
        let mut overlay = RuntimeOverlay::new(config);

        let batch = build_test_batch();
        overlay.ingest(&g, &batch).unwrap();
        let heat_after_first = overlay.get_heat(0);

        // Ingest empty batch to trigger decay
        let empty_batch = OtelBatch {
            spans: vec![],
            timestamp: 0.0,
            service_name: String::new(),
        };
        overlay.ingest(&g, &empty_batch).unwrap();
        let heat_after_decay = overlay.get_heat(0);

        assert!(
            heat_after_decay < heat_after_first,
            "Decay should reduce heat"
        );
        // With decay_factor=0.5, heat should be roughly halved
        let ratio = heat_after_decay / heat_after_first;
        assert!(
            ratio < 0.6,
            "Heat should decay by ~50%, got ratio {}",
            ratio
        );
    }

    #[test]
    fn apply_boosts_modifies_activation() {
        let mut g = build_test_graph();
        let mut overlay = RuntimeOverlay::with_defaults();
        let batch = build_test_batch();
        overlay.ingest(&g, &batch).unwrap();

        let activation_before = g.nodes.activation[0][0].get();
        let applied = overlay.apply_boosts(&mut g, 0.1);
        let activation_after = g.nodes.activation[0][0].get();

        assert!(applied > 0, "Should apply at least one boost");
        assert!(
            activation_after >= activation_before,
            "Activation should not decrease after boost"
        );
    }

    #[test]
    fn unmapped_spans_counted() {
        let g = build_test_graph();
        let mut overlay = RuntimeOverlay::with_defaults();
        let batch = OtelBatch {
            spans: vec![OtelSpan {
                name: "nonexistent_function".to_string(),
                duration_us: 1000,
                count: 10,
                is_error: false,
                attributes: HashMap::new(),
                parent: None,
            }],
            timestamp: 0.0,
            service_name: String::new(),
        };

        let result = overlay.ingest(&g, &batch).unwrap();
        assert_eq!(result.spans_unmapped, 1, "Should count unmapped spans");
    }

    #[test]
    fn exact_id_mapping_strategy() {
        let g = build_test_graph();
        let config = OverlayConfig {
            mapping_strategy: MappingStrategy::ExactId,
            ..OverlayConfig::default()
        };
        let mut overlay = RuntimeOverlay::new(config);

        let batch = OtelBatch {
            spans: vec![OtelSpan {
                name: "func::handle_request".to_string(), // matches external ID
                duration_us: 5000,
                count: 50,
                is_error: false,
                attributes: HashMap::new(),
                parent: None,
            }],
            timestamp: 0.0,
            service_name: String::new(),
        };

        let result = overlay.ingest(&g, &batch).unwrap();
        assert_eq!(result.spans_mapped, 1, "ExactId should match external ID");
    }

    #[test]
    fn min_duration_filter() {
        let g = build_test_graph();
        let config = OverlayConfig {
            min_duration_us: 10_000, // 10ms minimum
            ..OverlayConfig::default()
        };
        let mut overlay = RuntimeOverlay::new(config);
        let batch = build_test_batch(); // All spans are < 10ms

        let result = overlay.ingest(&g, &batch).unwrap();
        // All spans should be filtered out by the minimum duration
        assert!(
            result.hot_nodes.is_empty(),
            "Spans below min_duration should be filtered"
        );
    }
}
