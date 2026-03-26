// === m1nd-core/src/twins.rs ===
// @m1nd:temponizer:WIRING — new algorithm but bounded scope (signature + cosine)
// @m1nd:emca:pattern — EXECUTE(signatures) → MEASURE(isomorphic test) → ADJUST(field names)
// @m1nd:primitives — topology (degree), cosine_similarity (custom)
//
// RB-03 — Structural Twins: topological isomorphism detection.
//
// Computes a structural "fingerprint" for each node based on its graph
// neighbourhood (in-degree, out-degree, edge types, neighbor type distribution)
// and discovers pairs/clusters of nodes with highly similar signatures.
//
// Unlike semantic similarity, this is purely topological — two functions with
// completely different names/purposes but identical structural roles (e.g. two
// retry wrappers, two CRUD handlers) will be matched.

use crate::error::{M1ndError, M1ndResult};
use crate::graph::Graph;
use crate::types::{FiniteF32, NodeId, NodeType};
use serde::Serialize;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for structural twin detection.
#[derive(Clone, Debug)]
pub struct TwinConfig {
    /// Minimum cosine similarity to consider a pair as twins [0.0, 1.0].
    pub similarity_threshold: f32,
    /// Maximum twins to return.
    pub top_k: usize,
    /// Which node types to compare (empty = all).
    pub node_types: Vec<NodeType>,
    /// File path prefix filter (empty = all).
    pub scope: Option<String>,
    /// Whether to include edge type distribution in signature.
    pub use_edge_types: bool,
}

impl Default for TwinConfig {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.80,
            top_k: 50,
            node_types: vec![],
            scope: None,
            use_edge_types: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// A detected structural twin pair.
#[derive(Clone, Debug, Serialize)]
pub struct TwinPair {
    /// External ID of node A.
    pub node_a_id: String,
    /// Label of node A.
    pub node_a_label: String,
    /// External ID of node B.
    pub node_b_id: String,
    /// Label of node B.
    pub node_b_label: String,
    /// Cosine similarity of structural signatures.
    pub similarity: f32,
    /// Shared structural properties.
    pub shared_properties: Vec<String>,
}

/// Structural signature for a single node.
#[derive(Clone, Debug)]
struct StructuralSignature {
    node_id: NodeId,
    ext_id: String,
    label: String,
    features: Vec<f32>,
}

/// Result of twin detection.
#[derive(Clone, Debug, Serialize)]
pub struct TwinResult {
    /// Twin pairs sorted by similarity descending.
    pub pairs: Vec<TwinPair>,
    /// Number of nodes analyzed.
    pub nodes_analyzed: usize,
    /// Number of signatures computed.
    pub signatures_computed: usize,
    /// Elapsed time in ms.
    pub elapsed_ms: f64,
}

// ---------------------------------------------------------------------------
// Feature extraction
// ---------------------------------------------------------------------------

/// Convert NodeType to a numeric discriminant (safe alternative to `as u8`
/// since NodeType has a Custom(u8) variant making it non-fieldless).
fn node_type_as_f32(nt: &NodeType) -> f32 {
    match nt {
        NodeType::File => 0.0,
        NodeType::Directory => 1.0,
        NodeType::Function => 2.0,
        NodeType::Class => 3.0,
        NodeType::Struct => 4.0,
        NodeType::Enum => 5.0,
        NodeType::Type => 6.0,
        NodeType::Module => 7.0,
        NodeType::Reference => 8.0,
        NodeType::Concept => 9.0,
        NodeType::Material => 10.0,
        NodeType::Process => 11.0,
        NodeType::Product => 12.0,
        NodeType::Supplier => 13.0,
        NodeType::Regulatory => 14.0,
        NodeType::System => 15.0,
        NodeType::Cost => 16.0,
        NodeType::Custom(v) => 17.0 + *v as f32,
    }
}

/// Number of base features (before edge type expansion).
const BASE_FEATURES: usize = 8;

/// Compute structural signature features for a node.
fn compute_signature(
    graph: &Graph,
    node: NodeId,
    n: usize,
    edge_type_vocab: &HashMap<String, usize>,
    use_edge_types: bool,
) -> Vec<f32> {
    let edge_vocab_size = if use_edge_types {
        edge_type_vocab.len()
    } else {
        0
    };
    let total_features = BASE_FEATURES + edge_vocab_size * 2; // outgoing + incoming edge types
    let mut features = vec![0.0f32; total_features];

    let idx = node.as_usize();
    if idx >= n || !graph.finalized {
        return features;
    }

    // Feature 0: out-degree
    let out_range = graph.csr.out_range(node);
    let out_degree = out_range.len() as f32;
    features[0] = out_degree;

    // Feature 1: in-degree
    let in_range = graph.csr.in_range(node);
    let in_degree = in_range.len() as f32;
    features[1] = in_degree;

    // Feature 2: degree ratio (in / (in + out))
    let total_degree = in_degree + out_degree;
    features[2] = if total_degree > 0.0 {
        in_degree / total_degree
    } else {
        0.5
    };

    // Feature 3: pagerank
    features[3] = graph.nodes.pagerank[idx].get();

    // Feature 4: node type as one-hot position
    features[4] = node_type_as_f32(&graph.nodes.node_type[idx]);

    // Feature 5: change frequency
    features[5] = graph.nodes.change_frequency[idx].get();

    // Feature 6: number of tags
    features[6] = graph.nodes.tags[idx].len() as f32;

    // Feature 7: neighbor type distribution entropy
    let mut type_counts: [f32; 8] = [0.0; 8]; // up to 8 NodeType variants
    for edge_idx in out_range.clone() {
        let target = graph.csr.targets[edge_idx];
        let tidx = target.as_usize();
        if tidx < n {
            let t = node_type_as_f32(&graph.nodes.node_type[tidx]) as usize;
            if t < type_counts.len() {
                type_counts[t] += 1.0;
            }
        }
    }
    // Shannon entropy of neighbor type distribution
    let total: f32 = type_counts.iter().sum();
    if total > 0.0 {
        let entropy: f32 = type_counts
            .iter()
            .filter(|&&c| c > 0.0)
            .map(|&c| {
                let p = c / total;
                -p * p.ln()
            })
            .sum();
        features[7] = entropy;
    }

    // Edge type distribution features (outgoing + incoming)
    if use_edge_types {
        for edge_idx in graph.csr.out_range(node) {
            let rel = graph.strings.resolve(graph.csr.relations[edge_idx]);
            if let Some(&vocab_idx) = edge_type_vocab.get(rel) {
                features[BASE_FEATURES + vocab_idx] += 1.0;
            }
        }
        for edge_idx in graph.csr.in_range(node) {
            let fwd_idx = graph.csr.rev_edge_idx[edge_idx];
            let rel = graph
                .strings
                .resolve(graph.csr.relations[fwd_idx.as_usize()]);
            if let Some(&vocab_idx) = edge_type_vocab.get(rel) {
                features[BASE_FEATURES + edge_vocab_size + vocab_idx] += 1.0;
            }
        }
    }

    features
}

/// Cosine similarity between two feature vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a < f32::EPSILON || norm_b < f32::EPSILON {
        return 0.0;
    }
    (dot / (norm_a * norm_b)).clamp(0.0, 1.0)
}

/// Describe the shared structural properties between two signatures.
fn describe_shared(a: &[f32], b: &[f32]) -> Vec<String> {
    let mut props = Vec::new();
    if a.len() < BASE_FEATURES || b.len() < BASE_FEATURES {
        return props;
    }
    if (a[0] - b[0]).abs() < 0.5 {
        props.push(format!("out_degree≈{:.0}", a[0]));
    }
    if (a[1] - b[1]).abs() < 0.5 {
        props.push(format!("in_degree≈{:.0}", a[1]));
    }
    if (a[4] - b[4]).abs() < 0.5 {
        props.push("same_node_type".to_string());
    }
    if (a[6] - b[6]).abs() < 0.5 {
        props.push(format!("tag_count≈{:.0}", a[6]));
    }
    props
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// Find structural twins in the graph.
pub fn find_twins(graph: &Graph, config: &TwinConfig) -> M1ndResult<TwinResult> {
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

    // Build edge type vocabulary
    let mut edge_type_vocab: HashMap<String, usize> = HashMap::new();
    if config.use_edge_types {
        for i in 0..n {
            let nid = NodeId::new(i as u32);
            for edge_idx in graph.csr.out_range(nid) {
                let rel = graph
                    .strings
                    .resolve(graph.csr.relations[edge_idx])
                    .to_string();
                let len = edge_type_vocab.len();
                edge_type_vocab.entry(rel).or_insert(len);
            }
        }
    }

    // Filter nodes by config
    let candidate_nodes: Vec<NodeId> = (0..n)
        .filter(|&i| {
            // Type filter
            if !config.node_types.is_empty()
                && !config.node_types.contains(&graph.nodes.node_type[i])
            {
                return false;
            }
            // Scope filter
            if let Some(ref scope) = config.scope {
                if !node_to_ext[i].contains(scope.as_str()) {
                    return false;
                }
            }
            true
        })
        .map(|i| NodeId::new(i as u32))
        .collect();

    // Compute signatures
    let signatures: Vec<StructuralSignature> = candidate_nodes
        .iter()
        .map(|&nid| {
            let idx = nid.as_usize();
            StructuralSignature {
                node_id: nid,
                ext_id: node_to_ext[idx].clone(),
                label: graph.strings.resolve(graph.nodes.label[idx]).to_string(),
                features: compute_signature(graph, nid, n, &edge_type_vocab, config.use_edge_types),
            }
        })
        .collect();

    let sigs_count = signatures.len();

    // Pairwise comparison (O(n²) but capped by candidate count)
    let mut pairs: Vec<TwinPair> = Vec::new();
    let budget = 100_000usize; // max comparisons
    let mut comparisons = 0usize;

    for i in 0..signatures.len() {
        if comparisons >= budget {
            break;
        }
        for j in (i + 1)..signatures.len() {
            comparisons += 1;
            if comparisons >= budget {
                break;
            }

            let sim = cosine_similarity(&signatures[i].features, &signatures[j].features);
            if sim >= config.similarity_threshold {
                pairs.push(TwinPair {
                    node_a_id: signatures[i].ext_id.clone(),
                    node_a_label: signatures[i].label.clone(),
                    node_b_id: signatures[j].ext_id.clone(),
                    node_b_label: signatures[j].label.clone(),
                    similarity: sim,
                    shared_properties: describe_shared(
                        &signatures[i].features,
                        &signatures[j].features,
                    ),
                });
            }
        }
    }

    // Sort by similarity descending and truncate
    pairs.sort_by(|a, b| {
        b.similarity
            .partial_cmp(&a.similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    pairs.truncate(config.top_k);

    Ok(TwinResult {
        pairs,
        nodes_analyzed: candidate_nodes.len(),
        signatures_computed: sigs_count,
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
    use crate::types::{EdgeDirection, FiniteF32};

    /// Build a graph with two structurally identical subgraphs:
    ///   handler_a → process_a → output_a
    ///   handler_b → process_b → output_b
    fn build_twin_graph() -> Graph {
        let mut g = Graph::new();
        // Subgraph A
        g.add_node(
            "a_h",
            "handler_a",
            NodeType::Function,
            &["handler"],
            0.0,
            0.5,
        )
        .unwrap();
        g.add_node("a_p", "process_a", NodeType::Function, &["data"], 0.0, 0.3)
            .unwrap();
        g.add_node("a_o", "output_a", NodeType::Function, &["output"], 0.0, 0.2)
            .unwrap();
        // Subgraph B (structural twin)
        g.add_node(
            "b_h",
            "handler_b",
            NodeType::Function,
            &["handler"],
            0.0,
            0.5,
        )
        .unwrap();
        g.add_node("b_p", "process_b", NodeType::Function, &["data"], 0.0, 0.3)
            .unwrap();
        g.add_node("b_o", "output_b", NodeType::Function, &["output"], 0.0, 0.2)
            .unwrap();

        // Edges A
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
        // Edges B (same structure)
        g.add_edge(
            NodeId::new(3),
            NodeId::new(4),
            "calls",
            FiniteF32::new(0.8),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.5),
        )
        .unwrap();
        g.add_edge(
            NodeId::new(4),
            NodeId::new(5),
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

    /// Build a graph with structurally different nodes.
    fn build_different_graph() -> Graph {
        let mut g = Graph::new();
        // Hub node (high out-degree)
        g.add_node("hub", "dispatcher", NodeType::Function, &["core"], 0.0, 0.9)
            .unwrap();
        // Leaf nodes (low degree)
        g.add_node(
            "leaf1",
            "handler_1",
            NodeType::Function,
            &["handler"],
            0.0,
            0.1,
        )
        .unwrap();
        g.add_node(
            "leaf2",
            "handler_2",
            NodeType::Function,
            &["handler"],
            0.0,
            0.1,
        )
        .unwrap();
        g.add_node(
            "leaf3",
            "handler_3",
            NodeType::Function,
            &["handler"],
            0.0,
            0.1,
        )
        .unwrap();

        // Hub fans out to all leaves
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
            NodeId::new(0),
            NodeId::new(2),
            "calls",
            FiniteF32::new(0.8),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.5),
        )
        .unwrap();
        g.add_edge(
            NodeId::new(0),
            NodeId::new(3),
            "calls",
            FiniteF32::new(0.8),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.5),
        )
        .unwrap();

        g.finalize().unwrap();
        g
    }

    #[test]
    fn cosine_sim_identical() {
        let a = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&a, &a) - 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_sim_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 0.001);
    }

    #[test]
    fn cosine_sim_zero_vectors() {
        let a = vec![0.0, 0.0, 0.0];
        assert!(cosine_similarity(&a, &a).abs() < 0.001);
    }

    #[test]
    fn twins_found_in_symmetric_graph() {
        let g = build_twin_graph();
        let config = TwinConfig {
            similarity_threshold: 0.85,
            ..TwinConfig::default()
        };
        let result = find_twins(&g, &config).unwrap();
        assert!(result.nodes_analyzed == 6);
        // handler_a ↔ handler_b, process_a ↔ process_b, output_a ↔ output_b
        // should all be detected as structural twins
        assert!(
            !result.pairs.is_empty(),
            "Should detect twin pairs, got none"
        );

        // Check that at least one handler pair is found
        let has_handler_twin = result
            .pairs
            .iter()
            .any(|p| (p.node_a_label.contains("handler") && p.node_b_label.contains("handler")));
        assert!(
            has_handler_twin,
            "Should detect handler_a ↔ handler_b twin, pairs: {:?}",
            result.pairs
        );
    }

    #[test]
    fn hub_not_twin_with_leaf() {
        let g = build_different_graph();
        let config = TwinConfig {
            similarity_threshold: 0.90,
            ..TwinConfig::default()
        };
        let result = find_twins(&g, &config).unwrap();
        // Hub should NOT be a twin with any leaf
        let has_hub_leaf_twin = result
            .pairs
            .iter()
            .any(|p| (p.node_a_label == "dispatcher" || p.node_b_label == "dispatcher"));
        assert!(
            !has_hub_leaf_twin,
            "Hub should not be twin with leaf at 0.9 threshold"
        );
    }

    #[test]
    fn leaves_are_twins_of_each_other() {
        let g = build_different_graph();
        let config = TwinConfig {
            similarity_threshold: 0.85,
            ..TwinConfig::default()
        };
        let result = find_twins(&g, &config).unwrap();
        // All 3 leaves should be twins of each other
        let leaf_pairs: Vec<_> = result
            .pairs
            .iter()
            .filter(|p| {
                p.node_a_label.starts_with("handler_") && p.node_b_label.starts_with("handler_")
            })
            .collect();
        assert!(
            leaf_pairs.len() >= 2,
            "Should find at least 2 leaf twin pairs, found {}",
            leaf_pairs.len()
        );
    }

    #[test]
    fn empty_graph_returns_error() {
        let g = Graph::new();
        let config = TwinConfig::default();
        assert!(find_twins(&g, &config).is_err());
    }

    #[test]
    fn scope_filter_limits_analysis() {
        let g = build_twin_graph();
        let config = TwinConfig {
            scope: Some("nonexistent_scope".to_string()),
            ..TwinConfig::default()
        };
        let result = find_twins(&g, &config).unwrap();
        assert!(
            result.pairs.is_empty(),
            "Scoped to nonexistent prefix should find no twins"
        );
    }
}
