// === crates/m1nd-core/src/snapshot_bin.rs ===
// Compact binary snapshot (bincode) with atomic write. Mirrors snapshot.rs JSON logic.

use crate::error::{M1ndError, M1ndResult};
use crate::graph::{Graph, NodeProvenanceInput, ResolvedNodeProvenance};
use crate::snapshot::SNAPSHOT_VERSION;
use crate::types::*;
use std::io::{BufWriter, Cursor, Write};
use std::path::Path;

/// The ONLY encoding this format has ever been written with: fixed-width
/// little-endian integers and `u64` length prefixes — bincode 1's behaviour,
/// which bincode 2 preserves under `legacy()` and NOT under `standard()`.
///
/// This is not a style choice. Every snapshot already on disk was written this
/// way, and bincode is not self-describing, so a decode under the wrong
/// configuration does not fail — it returns different values. Measured on a real
/// 88528-byte snapshot: `standard()` reads `version=4, nodes=0, edges=0` from 3
/// bytes and hands back an EMPTY graph with no error. Never change this to
/// `standard()`; a format change needs a `SNAPSHOT_VERSION` bump and a reader
/// for the old bytes. `tests/snapshot_bin_continuity.rs` holds frozen fixtures
/// that fail if it ever moves.
const SNAPSHOT_BIN_CONFIG: bincode::config::Configuration<
    bincode::config::LittleEndian,
    bincode::config::Fixint,
> = bincode::config::legacy();

/// Decode one whole-file payload, refusing a partial read.
///
/// bincode stops at the end of the value and reports how far it got; trailing
/// bytes are silently ignored. For a file that is supposed to BE exactly one
/// snapshot, a short read is the signature of a misencoded or truncated file —
/// the empty-graph misread above consumed 3 of 88528 bytes and looked like
/// success. Demanding full consumption turns that class into a refusal.
fn decode_whole_file<T: serde::de::DeserializeOwned>(data: &[u8], what: &str) -> M1ndResult<T> {
    let (value, consumed) = bincode::serde::decode_from_slice::<T, _>(data, SNAPSHOT_BIN_CONFIG)
        .map_err(|error| M1ndError::PersistenceFailed(error.to_string()))?;
    if consumed != data.len() {
        return Err(M1ndError::CorruptState {
            reason: format!(
                "binary graph snapshot {what} decoded only {consumed} of {} bytes",
                data.len()
            ),
        });
    }
    Ok(value)
}

#[derive(serde::Serialize, serde::Deserialize)]
struct GraphSnapshotBinV4 {
    version: u32,
    nodes: Vec<NodeSnapshotBin>,
    edges: Vec<EdgeSnapshotBinV4>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct GraphSnapshotBinV3 {
    version: u32,
    nodes: Vec<NodeSnapshotBin>,
    edges: Vec<EdgeSnapshotBinV3>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct NodeSnapshotBin {
    external_id: String,
    label: String,
    node_type: u8,
    tags: Vec<String>,
    last_modified: f64,
    change_frequency: f32,
    provenance: NodeProvenanceBin,
}

#[derive(Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct NodeProvenanceBin {
    source_path: Option<String>,
    line_start: Option<u32>,
    line_end: Option<u32>,
    excerpt: Option<String>,
    namespace: Option<String>,
    canonical: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct EdgeSnapshotBinV4 {
    source_id: String,
    target_id: String,
    relation: String,
    original_weight: f32,
    current_weight: f32,
    reverse_original_weight: Option<f32>,
    reverse_current_weight: Option<f32>,
    direction: u8, // 0=Forward, 1=Bidirectional
    inhibitory: bool,
    causal_strength: f32,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct EdgeSnapshotBinV3 {
    source_id: String,
    target_id: String,
    relation: String,
    weight: f32,
    direction: u8,
    inhibitory: bool,
    causal_strength: f32,
}

impl From<EdgeSnapshotBinV3> for EdgeSnapshotBinV4 {
    fn from(value: EdgeSnapshotBinV3) -> Self {
        let reverse_weight = (value.direction == 1).then_some(value.weight);
        Self {
            source_id: value.source_id,
            target_id: value.target_id,
            relation: value.relation,
            original_weight: value.weight,
            current_weight: value.weight,
            reverse_original_weight: reverse_weight,
            reverse_current_weight: reverse_weight,
            direction: value.direction,
            inhibitory: value.inhibitory,
            causal_strength: value.causal_strength,
        }
    }
}

fn node_type_to_u8(nt: NodeType) -> u8 {
    match nt {
        NodeType::File => 0,
        NodeType::Directory => 1,
        NodeType::Function => 2,
        NodeType::Class => 3,
        NodeType::Struct => 4,
        NodeType::Enum => 5,
        NodeType::Type => 6,
        NodeType::Module => 7,
        NodeType::Reference => 8,
        NodeType::Concept => 9,
        NodeType::Material => 10,
        NodeType::Process => 11,
        NodeType::Product => 12,
        NodeType::Supplier => 13,
        NodeType::Regulatory => 14,
        NodeType::System => 15,
        NodeType::Cost => 16,
        NodeType::Custom(v) => 100 + v,
    }
}

fn u8_to_node_type(v: u8) -> NodeType {
    match v {
        0 => NodeType::File,
        1 => NodeType::Directory,
        2 => NodeType::Function,
        3 => NodeType::Class,
        4 => NodeType::Struct,
        5 => NodeType::Enum,
        6 => NodeType::Type,
        7 => NodeType::Module,
        8 => NodeType::Reference,
        9 => NodeType::Concept,
        10 => NodeType::Material,
        11 => NodeType::Process,
        12 => NodeType::Product,
        13 => NodeType::Supplier,
        14 => NodeType::Regulatory,
        15 => NodeType::System,
        16 => NodeType::Cost,
        v if v >= 100 => NodeType::Custom(v - 100),
        _ => NodeType::Custom(v),
    }
}

fn provenance_from_resolved(p: ResolvedNodeProvenance) -> NodeProvenanceBin {
    NodeProvenanceBin {
        source_path: p.source_path,
        line_start: p.line_start,
        line_end: p.line_end,
        excerpt: p.excerpt,
        namespace: p.namespace,
        canonical: p.canonical,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct EdgeSlotKey {
    source: u32,
    target: u32,
    relation: u32,
    direction: u8,
    inhibitory: bool,
    causal_strength_bits: u32,
}

impl EdgeSlotKey {
    fn reversed(self) -> Self {
        Self {
            source: self.target,
            target: self.source,
            ..self
        }
    }
}

fn edge_slot_sources(graph: &Graph) -> M1ndResult<Vec<NodeId>> {
    let edge_count = graph.csr.num_edges();
    let mut sources = vec![NodeId::default(); edge_count];
    let mut assigned = vec![false; edge_count];
    for source in 0..graph.num_nodes() as usize {
        for slot in graph.csr.out_range(NodeId::new(source as u32)) {
            if slot >= edge_count || assigned[slot] {
                return Err(M1ndError::CorruptState {
                    reason: "CSR edge ranges are overlapping or out of bounds".into(),
                });
            }
            sources[slot] = NodeId::new(source as u32);
            assigned[slot] = true;
        }
    }
    if assigned.iter().any(|assigned| !assigned) {
        return Err(M1ndError::CorruptState {
            reason: "CSR offsets leave one or more edge slots without a source".into(),
        });
    }
    Ok(sources)
}

fn edge_slot_key(graph: &Graph, sources: &[NodeId], slot: usize) -> M1ndResult<EdgeSlotKey> {
    if slot >= graph.csr.num_edges()
        || slot >= sources.len()
        || slot >= graph.csr.targets.len()
        || slot >= graph.csr.relations.len()
        || slot >= graph.csr.directions.len()
        || slot >= graph.csr.inhibitory.len()
        || slot >= graph.csr.causal_strengths.len()
    {
        return Err(M1ndError::CorruptState {
            reason: format!("CSR edge slot {slot} is structurally incomplete"),
        });
    }
    Ok(EdgeSlotKey {
        source: sources[slot].0,
        target: graph.csr.targets[slot].0,
        relation: graph.csr.relations[slot].0,
        direction: graph.csr.directions[slot] as u8,
        inhibitory: graph.csr.inhibitory[slot],
        causal_strength_bits: graph.csr.causal_strengths[slot].get().to_bits(),
    })
}

fn edge_slot_queues(
    graph: &Graph,
    sources: &[NodeId],
) -> M1ndResult<std::collections::HashMap<EdgeSlotKey, std::collections::VecDeque<usize>>> {
    use std::collections::{HashMap, VecDeque};

    let mut queues: HashMap<EdgeSlotKey, VecDeque<usize>> = HashMap::new();
    for slot in 0..graph.csr.num_edges() {
        queues
            .entry(edge_slot_key(graph, sources, slot)?)
            .or_default()
            .push_back(slot);
    }
    Ok(queues)
}

fn pop_unconsumed_slot(
    queues: &mut std::collections::HashMap<EdgeSlotKey, std::collections::VecDeque<usize>>,
    key: EdgeSlotKey,
    consumed: &[bool],
) -> Option<usize> {
    let queue = queues.get_mut(&key)?;
    while let Some(slot) = queue.pop_front() {
        if !consumed[slot] {
            return Some(slot);
        }
    }
    None
}

fn validate_edge_plasticity_slot(graph: &Graph, slot: usize) -> M1ndResult<()> {
    if slot >= graph.edge_plasticity.original_weight.len()
        || slot >= graph.edge_plasticity.current_weight.len()
    {
        return Err(M1ndError::CorruptState {
            reason: format!("edge plasticity slot {slot} is missing"),
        });
    }
    Ok(())
}

fn collect_edge_snapshots_v4(
    graph: &Graph,
    node_to_ext_id: &[String],
) -> M1ndResult<Vec<EdgeSnapshotBinV4>> {
    let edge_count = graph.csr.num_edges();
    let sources = edge_slot_sources(graph)?;
    let mut queues = edge_slot_queues(graph, &sources)?;
    let mut consumed = vec![false; edge_count];
    let mut edges = Vec::with_capacity(edge_count);

    for slot in 0..edge_count {
        if consumed[slot] {
            continue;
        }
        validate_edge_plasticity_slot(graph, slot)?;
        let key = edge_slot_key(graph, &sources, slot)?;
        let source = key.source as usize;
        let target = key.target as usize;
        if source >= node_to_ext_id.len() || target >= node_to_ext_id.len() {
            return Err(M1ndError::CorruptState {
                reason: format!("edge slot {slot} points outside the node table"),
            });
        }

        consumed[slot] = true;
        let (reverse_original_weight, reverse_current_weight) =
            if graph.csr.directions[slot] == EdgeDirection::Bidirectional {
                let reverse_slot = pop_unconsumed_slot(&mut queues, key.reversed(), &consumed)
                    .ok_or_else(|| M1ndError::CorruptState {
                        reason: format!(
                            "bidirectional edge slot {slot} has no exact reverse CSR mirror"
                        ),
                    })?;
                validate_edge_plasticity_slot(graph, reverse_slot)?;
                consumed[reverse_slot] = true;
                (
                    Some(graph.edge_plasticity.original_weight[reverse_slot].get()),
                    Some(
                        graph
                            .csr
                            .read_weight(EdgeIdx::new(reverse_slot as u32))
                            .get(),
                    ),
                )
            } else {
                (None, None)
            };

        edges.push(EdgeSnapshotBinV4 {
            source_id: node_to_ext_id[source].clone(),
            target_id: node_to_ext_id[target].clone(),
            relation: graph.strings.resolve(graph.csr.relations[slot]).to_string(),
            original_weight: graph.edge_plasticity.original_weight[slot].get(),
            current_weight: graph.csr.read_weight(EdgeIdx::new(slot as u32)).get(),
            reverse_original_weight,
            reverse_current_weight,
            direction: key.direction,
            inhibitory: key.inhibitory,
            causal_strength: f32::from_bits(key.causal_strength_bits),
        });
    }

    Ok(edges)
}

fn validate_v4_edge(edge: &EdgeSnapshotBinV4) -> M1ndResult<()> {
    if edge.direction > 1 {
        return Err(M1ndError::CorruptState {
            reason: format!("unknown binary edge direction {}", edge.direction),
        });
    }
    if !edge.original_weight.is_finite()
        || !edge.current_weight.is_finite()
        || !edge.causal_strength.is_finite()
        || edge
            .reverse_original_weight
            .is_some_and(|value| !value.is_finite())
        || edge
            .reverse_current_weight
            .is_some_and(|value| !value.is_finite())
    {
        return Err(M1ndError::CorruptState {
            reason: format!(
                "non-finite binary edge state for {} -> {}",
                edge.source_id, edge.target_id
            ),
        });
    }
    match (
        edge.direction,
        edge.reverse_original_weight,
        edge.reverse_current_weight,
    ) {
        (0, None, None) | (1, Some(_), Some(_)) => Ok(()),
        (0, _, _) => Err(M1ndError::CorruptState {
            reason: "forward binary edge unexpectedly contains reverse-slot state".into(),
        }),
        (1, _, _) => Err(M1ndError::CorruptState {
            reason: "bidirectional binary edge is missing reverse-slot weights".into(),
        }),
        _ => unreachable!("direction was range checked"),
    }
}

fn restore_edge_slot(
    graph: &mut Graph,
    slot: usize,
    original_weight: f32,
    current_weight: f32,
) -> M1ndResult<()> {
    validate_edge_plasticity_slot(graph, slot)?;
    graph.edge_plasticity.original_weight[slot] = FiniteF32::new(original_weight);
    graph.edge_plasticity.current_weight[slot] = FiniteF32::new(current_weight);
    graph.csr.weights[slot].store(
        current_weight.to_bits(),
        std::sync::atomic::Ordering::Release,
    );
    Ok(())
}

fn graph_from_snapshot_v4(snapshot: GraphSnapshotBinV4) -> M1ndResult<Graph> {
    if snapshot.version != SNAPSHOT_VERSION {
        return Err(M1ndError::CorruptState {
            reason: format!(
                "unsupported binary graph snapshot version {}",
                snapshot.version
            ),
        });
    }
    if snapshot.nodes.is_empty() {
        if snapshot.edges.is_empty() {
            return Ok(Graph::new());
        }
        return Err(M1ndError::CorruptState {
            reason: "binary graph snapshot has edges but no nodes".into(),
        });
    }
    for edge in &snapshot.edges {
        validate_v4_edge(edge)?;
    }

    let mut graph = Graph::with_capacity(snapshot.nodes.len(), snapshot.edges.len());
    for node in &snapshot.nodes {
        if !node.last_modified.is_finite() || !node.change_frequency.is_finite() {
            return Err(M1ndError::CorruptState {
                reason: format!("non-finite binary node state for {}", node.external_id),
            });
        }
        let tags: Vec<&str> = node.tags.iter().map(String::as_str).collect();
        let node_id = graph.add_node(
            &node.external_id,
            &node.label,
            u8_to_node_type(node.node_type),
            &tags,
            node.last_modified,
            node.change_frequency,
        )?;
        graph.set_node_provenance(
            node_id,
            NodeProvenanceInput {
                source_path: node.provenance.source_path.as_deref(),
                line_start: node.provenance.line_start,
                line_end: node.provenance.line_end,
                excerpt: node.provenance.excerpt.as_deref(),
                namespace: node.provenance.namespace.as_deref(),
                canonical: node.provenance.canonical,
            },
        );
    }

    for edge in &snapshot.edges {
        let source = graph
            .resolve_id(&edge.source_id)
            .ok_or_else(|| M1ndError::CorruptState {
                reason: format!("binary snapshot edge source {} is missing", edge.source_id),
            })?;
        let target = graph
            .resolve_id(&edge.target_id)
            .ok_or_else(|| M1ndError::CorruptState {
                reason: format!("binary snapshot edge target {} is missing", edge.target_id),
            })?;
        graph.add_edge(
            source,
            target,
            &edge.relation,
            FiniteF32::new(edge.original_weight),
            if edge.direction == 1 {
                EdgeDirection::Bidirectional
            } else {
                EdgeDirection::Forward
            },
            edge.inhibitory,
            FiniteF32::new(edge.causal_strength),
        )?;
    }
    graph.finalize()?;

    let sources = edge_slot_sources(&graph)?;
    let mut queues = edge_slot_queues(&graph, &sources)?;
    let mut consumed = vec![false; graph.csr.num_edges()];
    for edge in &snapshot.edges {
        let source = graph
            .resolve_id(&edge.source_id)
            .ok_or_else(|| M1ndError::CorruptState {
                reason: format!("binary snapshot source {} disappeared", edge.source_id),
            })?;
        let target = graph
            .resolve_id(&edge.target_id)
            .ok_or_else(|| M1ndError::CorruptState {
                reason: format!("binary snapshot target {} disappeared", edge.target_id),
            })?;
        let relation =
            graph
                .strings
                .lookup(&edge.relation)
                .ok_or_else(|| M1ndError::CorruptState {
                    reason: format!("binary snapshot relation {} disappeared", edge.relation),
                })?;
        let key = EdgeSlotKey {
            source: source.0,
            target: target.0,
            relation: relation.0,
            direction: edge.direction,
            inhibitory: edge.inhibitory,
            causal_strength_bits: edge.causal_strength.to_bits(),
        };
        let slot = pop_unconsumed_slot(&mut queues, key, &consumed).ok_or_else(|| {
            M1ndError::CorruptState {
                reason: format!(
                    "binary edge {} -> {} ({}) has no CSR slot",
                    edge.source_id, edge.target_id, edge.relation
                ),
            }
        })?;
        consumed[slot] = true;
        restore_edge_slot(&mut graph, slot, edge.original_weight, edge.current_weight)?;

        if edge.direction == 1 {
            let reverse_slot = pop_unconsumed_slot(&mut queues, key.reversed(), &consumed)
                .ok_or_else(|| M1ndError::CorruptState {
                    reason: format!(
                        "binary bidirectional edge {} -> {} has no reverse CSR slot",
                        edge.source_id, edge.target_id
                    ),
                })?;
            consumed[reverse_slot] = true;
            restore_edge_slot(
                &mut graph,
                reverse_slot,
                edge.reverse_original_weight
                    .expect("v4 binary bidirectional edge was prevalidated"),
                edge.reverse_current_weight
                    .expect("v4 binary bidirectional edge was prevalidated"),
            )?;
        }
    }
    Ok(graph)
}

/// Save full graph to compact binary snapshot. Atomic write via temp+rename.
pub fn save_graph(graph: &Graph, path: &Path) -> M1ndResult<()> {
    if !graph.csr.pending_edges.is_empty() {
        return Err(M1ndError::CorruptState {
            reason: "cannot snapshot a graph with unfinalized pending edges".into(),
        });
    }
    let n = graph.num_nodes() as usize;

    // Build reverse map: NodeId -> external_id string
    let mut node_to_ext_id = vec![String::new(); n];
    for (&interned, &node_id) in &graph.id_to_node {
        node_to_ext_id[node_id.as_usize()] = graph.strings.resolve(interned).to_string();
    }

    // Nodes
    let mut nodes = Vec::with_capacity(n);
    for (i, ext_id) in node_to_ext_id.iter().enumerate().take(n) {
        let label = graph.strings.resolve(graph.nodes.label[i]).to_string();
        let tags: Vec<String> = graph.nodes.tags[i]
            .iter()
            .map(|&t| graph.strings.resolve(t).to_string())
            .collect();
        nodes.push(NodeSnapshotBin {
            external_id: ext_id.clone(),
            label,
            node_type: node_type_to_u8(graph.nodes.node_type[i]),
            tags,
            last_modified: graph.nodes.last_modified[i],
            change_frequency: graph.nodes.change_frequency[i].get(),
            provenance: provenance_from_resolved(
                graph.resolve_node_provenance(NodeId::new(i as u32)),
            ),
        });
    }

    // A bidirectional logical edge owns two independently learned CSR slots.
    let edges = collect_edge_snapshots_v4(graph, &node_to_ext_id)?;

    let snapshot = GraphSnapshotBinV4 {
        version: SNAPSHOT_VERSION,
        nodes,
        edges,
    };

    let bytes = bincode::serde::encode_to_vec(&snapshot, SNAPSHOT_BIN_CONFIG)
        .map_err(|e| M1ndError::PersistenceFailed(e.to_string()))?;

    // Atomic write
    let temp_path = path.with_extension("tmp");
    {
        let file = std::fs::File::create(&temp_path)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(&bytes)?;
        writer.flush()?;
    }
    std::fs::rename(&temp_path, path)?;

    Ok(())
}

/// Load full graph from compact binary snapshot.
pub fn load_graph(path: &Path) -> M1ndResult<Graph> {
    let data = std::fs::read(path)?;
    // Bincode is not self-describing. Decode only the leading version field,
    // then select the exact historical struct instead of hoping a v4 decode
    // failure can be distinguished from corruption in a v3 payload.
    let mut cursor = Cursor::new(&data);
    let version: u32 = bincode::serde::decode_from_std_read(&mut cursor, SNAPSHOT_BIN_CONFIG)
        .map_err(|error| M1ndError::PersistenceFailed(error.to_string()))?;
    let snapshot = match version {
        SNAPSHOT_VERSION => decode_whole_file::<GraphSnapshotBinV4>(&data, "v4")?,
        3 => {
            let legacy = decode_whole_file::<GraphSnapshotBinV3>(&data, "v3")?;
            if legacy.version != 3 {
                return Err(M1ndError::CorruptState {
                    reason: format!(
                        "legacy binary graph snapshot reports version {}",
                        legacy.version
                    ),
                });
            }
            GraphSnapshotBinV4 {
                version: SNAPSHOT_VERSION,
                nodes: legacy.nodes,
                edges: legacy.edges.into_iter().map(Into::into).collect(),
            }
        }
        other => {
            return Err(M1ndError::CorruptState {
                reason: format!("unsupported binary graph snapshot version {other}"),
            });
        }
    };
    graph_from_snapshot_v4(snapshot)
}
