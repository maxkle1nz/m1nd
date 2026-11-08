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
    /// Match span attributes["code.function"] against labels (case-insensitive exact).
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

// A necessary-only byte-presence filter for substring matching. Folding the
// high bit permits collisions; those candidates still require `contains`.
fn substring_byte_signature(value: &str) -> u128 {
    let mut signature = 0u128;
    for byte in value.bytes() {
        signature |= 1u128 << (byte & 0x7f);
    }
    signature
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

        // Only label-based strategies need a lowercase label lookup.
        let mut label_to_idx: HashMap<String, Vec<usize>> = HashMap::new();
        if self.config.mapping_strategy != MappingStrategy::ExactId {
            for i in 0..n {
                let label = graph.strings.resolve(graph.nodes.label[i]).to_lowercase();
                label_to_idx.entry(label).or_default().push(i);
            }
        }

        // Build signatures once, lazily, in label-map order. Only the last
        // fallback length's eligible indices are retained alongside that table.
        let mut substring_labels: Option<Vec<(&String, &Vec<usize>, u128)>> = None;
        let mut substring_length = None;
        let mut substring_candidates: Vec<usize> = Vec::new();

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
                                let name_signature = substring_byte_signature(&name_lower);
                                let labels = substring_labels.get_or_insert_with(|| {
                                    label_to_idx
                                        .iter()
                                        .filter(|(label, _)| label.len() >= MIN_LEN)
                                        .map(|(label, indices)| {
                                            (label, indices, substring_byte_signature(label))
                                        })
                                        .collect()
                                });
                                if substring_length != Some(name_lower.len()) {
                                    substring_candidates.clear();
                                    substring_candidates.extend(
                                        labels.iter().enumerate().filter_map(
                                            |(index, &(label, _, _))| {
                                                let shorter = label.len().min(name_lower.len());
                                                let longer = label.len().max(name_lower.len());
                                                (shorter * 2 >= longer).then_some(index)
                                            },
                                        ),
                                    );
                                    substring_length = Some(name_lower.len());
                                }
                                for &index in &substring_candidates {
                                    let (label, indices, label_signature) = labels[index];
                                    let (shorter, longer) = if label.len() <= name_lower.len() {
                                        (label_signature, name_signature)
                                    } else {
                                        (name_signature, label_signature)
                                    };
                                    if shorter & !longer != 0 {
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
                MappingStrategy::ExactId => graph
                    .resolve_id(&span.name)
                    .map(NodeId::as_usize)
                    .filter(|&idx| idx < n)
                    .map(|idx| vec![idx])
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
        let mut node_to_ext: Vec<&str> = vec![""; n];
        for (interned, node_id) in &graph.id_to_node {
            let idx = node_id.as_usize();
            if idx < n {
                node_to_ext[idx] = graph.strings.resolve(*interned);
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
                        node_to_ext[idx].to_string()
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

    #[test]
    fn label_match_length_candidates_preserve_mapping_and_batch_state() {
        let labels = [
            "abcd",
            "abcdefgh",
            "abcdefghij",
            "abc",
            "",
            "abcd",
            "xyabcd",
            "AbCd",
            "İAB",
            "four",
            "ÜBER",
            "éé",
            "é",
        ];
        let mut graph = Graph::new();
        for (index, label) in labels.iter().enumerate() {
            graph
                .add_node(
                    &format!("id::{index}"),
                    label,
                    NodeType::Function,
                    &[],
                    0.0,
                    0.0,
                )
                .unwrap();
        }
        graph.finalize().unwrap();
        // Explicit mapping expectations cover exact-match precedence, duplicates,
        // both substring directions, 50% length boundaries and Unicode byte length.
        let cases: &[(&str, &[usize])] = &[
            ("abcd", &[0, 5, 7]),
            ("ABCD", &[0, 5, 7]),
            ("abcdef", &[0, 1, 2, 5, 7]),
            ("xyabcde", &[0, 5, 6, 7]),
            ("abcdxx", &[0, 5, 7]),
            ("abcdefghx", &[1]),
            ("xxfourxx", &[9]),
            ("xxfourxxx", &[]),
            ("qfourq", &[9]),
            ("rfourr", &[9]),
            ("short", &[]),
            ("abc", &[3]),
            ("ab", &[]),
            ("", &[4]),
            ("xxi\u{307}ab", &[8]),
            ("xüber", &[10]),
            ("xééx", &[11]),
            ("xéx", &[]),
            ("ÜBER", &[10]),
        ];
        let make_span = |name: &str| OtelSpan {
            name: name.into(),
            duration_us: 700,
            count: 3,
            is_error: true,
            attributes: HashMap::new(),
            parent: None,
        };
        for &(name, expected) in cases {
            let mut overlay = RuntimeOverlay::with_defaults();
            let result = overlay
                .ingest(
                    &graph,
                    &OtelBatch {
                        spans: vec![make_span(name)],
                        timestamp: 0.0,
                        service_name: String::new(),
                    },
                )
                .unwrap();
            let actual: Vec<_> = (0..labels.len())
                .filter(|&index| overlay.get_heat(index) > 0.0)
                .collect();
            assert_eq!(actual, expected, "span {name:?}");
            assert_eq!(result.spans_processed, 1);
            assert_eq!(result.spans_mapped, usize::from(!expected.is_empty()));
            assert_eq!(result.spans_unmapped, usize::from(expected.is_empty()));
            for &index in expected {
                let state = &overlay.node_data[&index];
                assert_eq!(
                    (
                        state.invocations,
                        state.errors,
                        state.total_duration_us,
                        state.max_duration_us
                    ),
                    (3, 3, 2100, 700),
                    "span {name:?}, node {index}"
                );
            }
        }
        // One ingest exercises reuse across different names of the same length.
        let batch = OtelBatch {
            spans: cases.iter().map(|(name, _)| make_span(name)).collect(),
            timestamp: 0.0,
            service_name: String::new(),
        };
        let mut overlay = RuntimeOverlay::new(OverlayConfig {
            decay_factor: 0.5,
            ..OverlayConfig::default()
        });
        let first = overlay.ingest(&graph, &batch).unwrap();
        let mapped = cases
            .iter()
            .filter(|(_, expected)| !expected.is_empty())
            .count();
        assert_eq!(
            (first.spans_mapped, first.spans_unmapped),
            (mapped, cases.len() - mapped)
        );
        let first_heat: Vec<_> = (0..labels.len())
            .map(|index| overlay.get_heat(index))
            .collect();
        overlay
            .ingest(
                &graph,
                &OtelBatch {
                    spans: vec![],
                    timestamp: 1.0,
                    service_name: String::new(),
                },
            )
            .unwrap();
        for (index, old_heat) in first_heat.into_iter().enumerate() {
            assert_eq!(overlay.get_heat(index), old_heat * 0.5, "node {index}");
            let matches = cases
                .iter()
                .filter(|(_, expected)| expected.contains(&index))
                .count() as u64;
            if matches > 0 {
                let state = &overlay.node_data[&index];
                assert_eq!(
                    (
                        state.invocations,
                        state.errors,
                        state.total_duration_us,
                        state.max_duration_us
                    ),
                    (3 * matches, 3 * matches, 2100 * matches, 700)
                );
            }
        }
    }

    #[test]
    fn label_match_byte_prefilter_keeps_both_directions_and_rejects_false_positives() {
        let cases = [
            // Either containment direction must survive the necessary filter.
            ("xyabcd", "abcd", true),
            ("abcd", "xyabcd", true),
            ("préparer", "prépa", true),
            ("über", "xxÜBER", true),
            // Presence is not byte order, multiplicity, or a substring proof.
            ("abcd", "dcba", false),
            ("aaab", "aabb", false),
            ("éà", "àé", false),
            // U+1861 is UTF-8 E1 A1 A1, folding to the same bits as "a!".
            ("a!a!", "\u{1861}\u{1861}", false),
            // Keep minimum byte lengths and the existing half-length boundary.
            ("é", "éé", false),
            ("abcd", "xxabcdxx", true),
            ("abcd", "xxabcdxxx", false),
        ];
        for (label, name, expected) in cases {
            let mut graph = Graph::new();
            graph
                .add_node("id::target", label, NodeType::Function, &[], 0.0, 0.0)
                .unwrap();
            graph.finalize().unwrap();
            let mut overlay = RuntimeOverlay::with_defaults();
            let result = overlay
                .ingest(
                    &graph,
                    &OtelBatch {
                        spans: vec![OtelSpan {
                            name: name.into(),
                            duration_us: 700,
                            count: 3,
                            is_error: true,
                            attributes: HashMap::new(),
                            parent: None,
                        }],
                        timestamp: 0.0,
                        service_name: String::new(),
                    },
                )
                .unwrap();
            assert_eq!(result.spans_processed, 1, "{label:?}, {name:?}");
            assert_eq!(
                result.spans_mapped,
                usize::from(expected),
                "{label:?}, {name:?}"
            );
            assert_eq!(
                result.spans_unmapped,
                usize::from(!expected),
                "{label:?}, {name:?}"
            );
            assert_eq!(overlay.get_heat(0) > 0.0, expected, "{label:?}, {name:?}");
            assert_eq!(overlay.node_data.len(), usize::from(expected));
            if expected {
                let state = &overlay.node_data[&0];
                assert_eq!(
                    (
                        state.invocations,
                        state.errors,
                        state.total_duration_us,
                        state.max_duration_us
                    ),
                    (3, 3, 2100, 700)
                );
            }
        }
    }

    #[test]
    fn other_overlay_strategies_and_duration_filter_keep_their_contract() {
        let graph = build_test_graph();
        let cases = [
            (
                MappingStrategy::CodeAttribute,
                "ignored",
                Some("HANDLE_REQUEST"),
                0,
                true,
            ),
            (
                MappingStrategy::CodeAttribute,
                "handle_request",
                None,
                0,
                false,
            ),
            (
                MappingStrategy::CodeAttribute,
                "ignored",
                Some("func::handle_request"),
                0,
                false,
            ),
            (
                MappingStrategy::CodeAttribute,
                "ignored",
                Some("handle"),
                0,
                false,
            ),
            (
                MappingStrategy::ExactId,
                "func::handle_request",
                None,
                0,
                true,
            ),
            (
                MappingStrategy::ExactId,
                "FUNC::handle_request",
                None,
                0,
                false,
            ),
            (
                MappingStrategy::LabelMatch,
                "handle_request",
                None,
                1000,
                false,
            ),
        ];
        for (strategy, name, attribute, min_duration_us, mapped) in cases {
            let mut attributes = HashMap::new();
            if let Some(value) = attribute {
                attributes.insert("code.function".into(), value.into());
            }
            let mut overlay = RuntimeOverlay::new(OverlayConfig {
                mapping_strategy: strategy,
                min_duration_us,
                ..OverlayConfig::default()
            });
            let result = overlay
                .ingest(
                    &graph,
                    &OtelBatch {
                        spans: vec![OtelSpan {
                            name: name.into(),
                            duration_us: 700,
                            count: 3,
                            is_error: true,
                            attributes,
                            parent: None,
                        }],
                        timestamp: 0.0,
                        service_name: String::new(),
                    },
                )
                .unwrap();
            assert_eq!(result.spans_processed, 1);
            assert_eq!(result.spans_mapped, usize::from(mapped));
            assert_eq!(
                result.spans_unmapped,
                usize::from(!mapped && min_duration_us == 0)
            );
            assert_eq!(overlay.get_heat(0) > 0.0, mapped);
            assert_eq!(overlay.node_data.len(), usize::from(mapped));
        }
    }

    #[test]
    fn exact_id_lookup_keeps_bounds_aliases_and_removed_id_semantics() {
        let mut graph = build_test_graph();
        let removed_id = graph.strings.lookup("func::handle_request").unwrap();
        graph.id_to_node.remove(&removed_id);
        let alias = graph.strings.get_or_intern("alias::process_data");
        graph.id_to_node.insert(alias, NodeId::new(1));
        let out_of_bounds = graph.strings.get_or_intern("stale::outside_graph");
        graph
            .id_to_node
            .insert(out_of_bounds, NodeId::new(graph.num_nodes() + 5));

        // Output aliases still use the last entry in this graph map's visit order.
        let expected_output_id = graph
            .id_to_node
            .iter()
            .filter(|(_, node_id)| node_id.as_usize() == 1)
            .map(|(interned, _)| graph.strings.resolve(*interned))
            .last()
            .unwrap()
            .to_string();
        let cases: &[(MappingStrategy, &str, &[usize])] = &[
            (MappingStrategy::ExactId, "func::handle_request", &[]),
            (MappingStrategy::LabelMatch, "handle_request", &[0]),
            (MappingStrategy::ExactId, "func::process_data", &[1]),
            (MappingStrategy::ExactId, "alias::process_data", &[1]),
            (MappingStrategy::ExactId, "ALIAS::process_data", &[]),
            (MappingStrategy::ExactId, "stale::outside_graph", &[]),
        ];
        for (strategy, name, expected) in cases {
            let mut overlay = RuntimeOverlay::new(OverlayConfig {
                mapping_strategy: strategy.clone(),
                ..OverlayConfig::default()
            });
            let result = overlay
                .ingest(
                    &graph,
                    &OtelBatch {
                        spans: vec![OtelSpan {
                            name: (*name).into(),
                            duration_us: 700,
                            count: 3,
                            is_error: true,
                            attributes: HashMap::new(),
                            parent: None,
                        }],
                        timestamp: 0.0,
                        service_name: String::new(),
                    },
                )
                .unwrap();
            let actual: Vec<_> = (0..graph.num_nodes() as usize)
                .filter(|&index| overlay.get_heat(index) > 0.0)
                .collect();
            assert_eq!(
                actual.as_slice(),
                *expected,
                "strategy {strategy:?}, span {name:?}"
            );
            assert_eq!(result.spans_mapped, usize::from(!expected.is_empty()));
            assert_eq!(result.spans_unmapped, usize::from(expected.is_empty()));
            if *expected == [0] {
                assert_eq!(result.hot_nodes[0].node_id, "");
            } else if *expected == [1] {
                assert_eq!(result.hot_nodes[0].node_id, expected_output_id);
            }
        }
    }

    #[test]
    fn hot_node_limit_preserves_existing_map_order_for_equal_heat() {
        let mut graph = Graph::new();
        let mut overlay = RuntimeOverlay::with_defaults();
        for index in 0..150 {
            graph
                .add_node(
                    &format!("id::{index}"),
                    &format!("label_{index}"),
                    NodeType::Function,
                    &[],
                    0.0,
                    0.0,
                )
                .unwrap();
            overlay.node_data.insert(
                index,
                NodeRuntimeData {
                    heat: 1.0,
                    invocations: 1,
                    ..NodeRuntimeData::default()
                },
            );
        }
        graph.finalize().unwrap();
        // All heats tie. The existing stable sort retains this visit order,
        // including which 100 nodes survive truncation; no ID tiebreak is added.
        let expected_ids: Vec<_> = overlay
            .node_data
            .iter()
            .take(100)
            .map(|(&index, _)| format!("id::{index}"))
            .collect();
        let result = overlay
            .ingest(
                &graph,
                &OtelBatch {
                    spans: vec![],
                    timestamp: 0.0,
                    service_name: String::new(),
                },
            )
            .unwrap();
        let actual_ids: Vec<_> = result
            .hot_nodes
            .iter()
            .map(|node| node.node_id.clone())
            .collect();
        assert_eq!(actual_ids, expected_ids);
        assert_eq!(result.hot_nodes.len(), 100);
        assert_eq!(overlay.node_data.len(), 150);
    }
}
