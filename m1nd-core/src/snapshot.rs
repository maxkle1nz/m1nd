// === crates/m1nd-core/src/snapshot.rs ===

use crate::error::{M1ndError, M1ndResult};
use crate::graph::{Graph, NodeProvenanceInput, ResolvedNodeProvenance};
use crate::plasticity::SynapticState;
use crate::types::*;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------------
// Snapshot — JSON graph persistence
// FM-PL-008 fix: atomic write (write to temp, rename).
// ---------------------------------------------------------------------------

/// Graph snapshot format version.
pub const SNAPSHOT_VERSION: u32 = 4;
const LEGACY_SNAPSHOT_VERSION: u32 = 3;

// ---------------------------------------------------------------------------
// Serialization types
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize)]
struct GraphSnapshotV4 {
    version: u32,
    nodes: Vec<NodeSnapshot>,
    edges: Vec<EdgeSnapshotV4>,
}

#[derive(serde::Deserialize)]
struct GraphSnapshotV3 {
    version: u32,
    nodes: Vec<NodeSnapshot>,
    edges: Vec<EdgeSnapshotV3>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct NodeSnapshot {
    external_id: String,
    label: String,
    node_type: u8,
    tags: Vec<String>,
    last_modified: f64,
    change_frequency: f32,
    #[serde(default, skip_serializing_if = "node_provenance_snapshot_is_empty")]
    provenance: NodeProvenanceSnapshot,
}

#[derive(Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct NodeProvenanceSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    line_start: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    line_end: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    excerpt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    namespace: Option<String>,
    #[serde(default)]
    canonical: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct EdgeSnapshotV4 {
    source_id: String,
    target_id: String,
    relation: String,
    original_weight: f32,
    current_weight: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reverse_original_weight: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reverse_current_weight: Option<f32>,
    direction: u8, // 0=Forward, 1=Bidirectional
    inhibitory: bool,
    causal_strength: f32,
}

#[derive(serde::Deserialize)]
struct EdgeSnapshotV3 {
    source_id: String,
    target_id: String,
    relation: String,
    weight: f32,
    direction: u8,
    inhibitory: bool,
    causal_strength: f32,
}

impl From<EdgeSnapshotV3> for EdgeSnapshotV4 {
    fn from(value: EdgeSnapshotV3) -> Self {
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

// ---------------------------------------------------------------------------
// NodeType to/from u8 helpers
// ---------------------------------------------------------------------------

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

fn node_provenance_snapshot_is_empty(value: &NodeProvenanceSnapshot) -> bool {
    value == &NodeProvenanceSnapshot::default()
}

fn snapshot_from_provenance(value: ResolvedNodeProvenance) -> NodeProvenanceSnapshot {
    NodeProvenanceSnapshot {
        source_path: value.source_path,
        line_start: value.line_start,
        line_end: value.line_end,
        excerpt: value.excerpt,
        namespace: value.namespace,
        canonical: value.canonical,
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
) -> M1ndResult<Vec<EdgeSnapshotV4>> {
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

        edges.push(EdgeSnapshotV4 {
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

fn validate_v4_edge(edge: &EdgeSnapshotV4) -> M1ndResult<()> {
    if edge.direction > 1 {
        return Err(M1ndError::CorruptState {
            reason: format!("unknown edge direction {}", edge.direction),
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
                "non-finite persisted edge state for {} -> {}",
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
            reason: "forward edge unexpectedly contains reverse-slot state".into(),
        }),
        (1, _, _) => Err(M1ndError::CorruptState {
            reason: "bidirectional edge is missing one or both reverse-slot weights".into(),
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

fn graph_from_snapshot_v4(snapshot: GraphSnapshotV4) -> M1ndResult<Graph> {
    if snapshot.version != SNAPSHOT_VERSION {
        return Err(M1ndError::CorruptState {
            reason: format!(
                "unsupported JSON graph snapshot version {}",
                snapshot.version
            ),
        });
    }
    if snapshot.nodes.is_empty() {
        if snapshot.edges.is_empty() {
            let mut graph = Graph::new();
            graph.finalize()?;
            return Ok(graph);
        }
        return Err(M1ndError::CorruptState {
            reason: "graph snapshot has edges but no nodes".into(),
        });
    }

    for edge in &snapshot.edges {
        validate_v4_edge(edge)?;
    }

    let mut graph = Graph::with_capacity(snapshot.nodes.len(), snapshot.edges.len());
    for node in &snapshot.nodes {
        if !node.last_modified.is_finite() || !node.change_frequency.is_finite() {
            return Err(M1ndError::CorruptState {
                reason: format!("non-finite node state for {}", node.external_id),
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
                reason: format!("snapshot edge source {} is missing", edge.source_id),
            })?;
        let target = graph
            .resolve_id(&edge.target_id)
            .ok_or_else(|| M1ndError::CorruptState {
                reason: format!("snapshot edge target {} is missing", edge.target_id),
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
                reason: format!("snapshot edge source {} disappeared", edge.source_id),
            })?;
        let target = graph
            .resolve_id(&edge.target_id)
            .ok_or_else(|| M1ndError::CorruptState {
                reason: format!("snapshot edge target {} disappeared", edge.target_id),
            })?;
        let relation =
            graph
                .strings
                .lookup(&edge.relation)
                .ok_or_else(|| M1ndError::CorruptState {
                    reason: format!("snapshot edge relation {} disappeared", edge.relation),
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
                    "persisted edge {} -> {} ({}) has no CSR slot",
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
                        "persisted bidirectional edge {} -> {} has no reverse CSR slot",
                        edge.source_id, edge.target_id
                    ),
                })?;
            consumed[reverse_slot] = true;
            restore_edge_slot(
                &mut graph,
                reverse_slot,
                edge.reverse_original_weight
                    .expect("v4 bidirectional edge was prevalidated"),
                edge.reverse_current_weight
                    .expect("v4 bidirectional edge was prevalidated"),
            )?;
        }
    }

    Ok(graph)
}

// ---------------------------------------------------------------------------
// Graph save/load
// ---------------------------------------------------------------------------

// Catastrophic-shrink guard (defense in depth). Incident 2026-07-15: the owner's
// `graph_snapshot.json` was overwritten from 10573 nodes to 704 with NO backup
// (the foreign-ingest root cause was closed by #370; this is the second line of
// defense). A canonical snapshot must never be replaced by a catastrophically
// smaller one in silence — the large snapshot is preserved as a timestamped
// `.bak-<unix_ts>` sibling first. Fail-open: the write always proceeds (a
// legitimate shrink is not blocked), the big graph is simply never lost silently.

/// The existing on-disk snapshot must hold at least this many nodes for the guard
/// to engage — below it, shrinking is ordinary churn on a small/fresh brain.
const SHRINK_GUARD_FLOOR: usize = 100;

/// Count the nodes in an on-disk snapshot cheaply, without reconstructing the
/// graph: only the `nodes` array length is read (its elements and the `edges`
/// array are skipped). Returns `None` if the file is absent or unreadable — the
/// guard then does nothing (best-effort, never blocks a persist).
fn snapshot_node_count_on_disk(path: &Path) -> Option<usize> {
    #[derive(serde::Deserialize)]
    struct NodeCountPeek {
        #[serde(default)]
        nodes: Vec<serde::de::IgnoredAny>,
    }
    let bytes = std::fs::read(path).ok()?;
    let peek: NodeCountPeek = serde_json::from_slice(&bytes).ok()?;
    Some(peek.nodes.len())
}

/// If `path` already holds a non-trivial snapshot and `new_node_count` is under
/// 20% of it, rename the existing file to `<path>.bak-<unix_ts>` before it is
/// overwritten. Returns the backup path when one was made. Fail-open: any I/O
/// hiccup (unreadable prior file, rename failure) skips the backup and lets the
/// write proceed — the guard never blocks a legitimate persist.
fn backup_if_catastrophic_shrink(path: &Path, new_node_count: usize) -> Option<PathBuf> {
    let existing = snapshot_node_count_on_disk(path)?;
    if existing < SHRINK_GUARD_FLOOR {
        return None;
    }
    // Catastrophic := new < 20% of existing, i.e. `new * 5 < existing`.
    if new_node_count.saturating_mul(5) >= existing {
        return None;
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut bak = path.as_os_str().to_owned();
    bak.push(format!(".bak-{ts}"));
    let bak = PathBuf::from(bak);
    match std::fs::rename(path, &bak) {
        Ok(()) => {
            eprintln!(
                "[m1nd] WARNING: snapshot {} holds {existing} nodes but the incoming graph has \
                 {new_node_count} (< 20%) — backed up the prior snapshot to {} before overwriting",
                path.display(),
                bak.display()
            );
            Some(bak)
        }
        Err(_) => None,
    }
}

/// Encode a complete graph as strict snapshot-v4 JSON without touching the
/// filesystem. Actor/checkpoint code uses this to stage an immutable candidate
/// before any canonical path is replaced.
pub fn encode_graph_json(graph: &Graph) -> M1ndResult<Vec<u8>> {
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

    // Serialize nodes
    let mut nodes = Vec::with_capacity(n);
    #[allow(clippy::needless_range_loop)]
    for i in 0..n {
        let label = graph.strings.resolve(graph.nodes.label[i]).to_string();
        let tags: Vec<String> = graph.nodes.tags[i]
            .iter()
            .map(|&t| graph.strings.resolve(t).to_string())
            .collect();
        nodes.push(NodeSnapshot {
            external_id: node_to_ext_id[i].clone(),
            label,
            node_type: node_type_to_u8(graph.nodes.node_type[i]),
            tags,
            last_modified: graph.nodes.last_modified[i],
            change_frequency: graph.nodes.change_frequency[i].get(),
            provenance: snapshot_from_provenance(
                graph.resolve_node_provenance(NodeId::new(i as u32)),
            ),
        });
    }

    // A bidirectional logical edge owns two independently learned CSR slots.
    // Persist them together instead of silently collapsing both to one weight.
    let edges = collect_edge_snapshots_v4(graph, &node_to_ext_id)?;

    let snapshot = GraphSnapshotV4 {
        version: SNAPSHOT_VERSION,
        nodes,
        edges,
    };
    serde_json::to_vec(&snapshot).map_err(M1ndError::Serde)
}

/// Save full graph to JSON snapshot. Atomic write: temp file + rename (FM-PL-008).
/// Serializes all nodes and edges so the graph can be fully reconstructed on load.
pub fn save_graph(graph: &Graph, path: &Path) -> M1ndResult<()> {
    let json = encode_graph_json(graph)?;

    // Defense in depth: never silently overwrite a large snapshot with a tiny one.
    let _ = backup_if_catastrophic_shrink(path, graph.num_nodes() as usize);

    // FM-PL-008: atomic write via temp file + rename
    let temp_path = path.with_extension("tmp");
    {
        let file = std::fs::File::create(&temp_path)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(&json)?;
        writer.flush()?;
    }
    std::fs::rename(&temp_path, path)?;

    Ok(())
}

/// Decode strict snapshot-v4 JSON, with the explicit v3 compatibility path,
/// without reading or writing a filesystem path.
pub fn decode_graph_json(data: &[u8]) -> M1ndResult<Graph> {
    #[derive(serde::Deserialize)]
    struct VersionPeek {
        version: u32,
    }
    let version = serde_json::from_slice::<VersionPeek>(data)
        .map_err(M1ndError::Serde)?
        .version;

    let snapshot = match version {
        SNAPSHOT_VERSION => {
            serde_json::from_slice::<GraphSnapshotV4>(data).map_err(M1ndError::Serde)?
        }
        LEGACY_SNAPSHOT_VERSION => {
            let legacy =
                serde_json::from_slice::<GraphSnapshotV3>(data).map_err(M1ndError::Serde)?;
            if legacy.version != LEGACY_SNAPSHOT_VERSION {
                return Err(M1ndError::CorruptState {
                    reason: format!(
                        "legacy JSON graph snapshot reports version {}",
                        legacy.version
                    ),
                });
            }
            GraphSnapshotV4 {
                version: SNAPSHOT_VERSION,
                nodes: legacy.nodes,
                edges: legacy.edges.into_iter().map(Into::into).collect(),
            }
        }
        other => {
            return Err(M1ndError::CorruptState {
                reason: format!("unsupported JSON graph snapshot version {other}"),
            });
        }
    };
    graph_from_snapshot_v4(snapshot)
}

/// Load full graph from JSON snapshot. Reconstructs the complete graph
/// with all nodes, edges, CSR, and PageRank.
pub fn load_graph(path: &Path) -> M1ndResult<Graph> {
    let data = std::fs::read(path)?;
    decode_graph_json(&data)
}

// ---------------------------------------------------------------------------
// Plasticity state save/load
// ---------------------------------------------------------------------------

/// Save plasticity state to JSON. Atomic write (FM-PL-008).
/// FM-PL-001 NaN firewall: non-finite weights replaced with originals at export.
pub fn save_plasticity_state(states: &[SynapticState], path: &Path) -> M1ndResult<()> {
    // FM-PL-001: NaN firewall at export boundary
    let safe_states: Vec<SynapticState> = states
        .iter()
        .map(|s| {
            let mut safe = s.clone();
            if !safe.current_weight.is_finite() {
                safe.current_weight = safe.original_weight;
            }
            safe
        })
        .collect();

    let json = serde_json::to_string_pretty(&safe_states).map_err(M1ndError::Serde)?;

    // FM-PL-008: atomic write
    let temp_path = path.with_extension("tmp");
    {
        let file = std::fs::File::create(&temp_path)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(json.as_bytes())?;
        writer.flush()?;
    }
    std::fs::rename(&temp_path, path)?;

    Ok(())
}

/// Load plasticity state from JSON.
/// FM-PL-007 fix: schema validation + error recovery.
pub fn load_plasticity_state(path: &Path) -> M1ndResult<Vec<SynapticState>> {
    let data = std::fs::read_to_string(path)?;
    let states: Vec<SynapticState> = serde_json::from_str(&data).map_err(M1ndError::Serde)?;

    // FM-PL-007: validate each entry
    for state in &states {
        if !state.original_weight.is_finite() || !state.current_weight.is_finite() {
            return Err(M1ndError::CorruptState {
                reason: format!(
                    "Non-finite weight in state: {}->{}",
                    state.source_label, state.target_label
                ),
            });
        }
    }

    Ok(states)
}

// ---------------------------------------------------------------------------
// Co-change matrix save/load
// ---------------------------------------------------------------------------

static DURABLE_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn durable_atomic_write(path: &Path, bytes: &[u8]) -> M1ndResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| M1ndError::PersistenceFailed("state path has no UTF-8 filename".into()))?;
    let sequence = DURABLE_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp_path = parent.join(format!(
        ".{file_name}.tmp-{}-{sequence}",
        std::process::id()
    ));
    let result = (|| -> M1ndResult<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp_path, path)?;
        // Windows refuses fsync on directory handles (ACCESS_DENIED); the
        // renamed entry is made durable there by write-through semantics.
        #[cfg(not(windows))]
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

/// Save the complete, graph-bound co-change matrix. The write is durable and
/// atomic: a crash can expose either the previous valid file or the complete
/// new file, never a truncated JSON document.
pub fn save_co_change_matrix(
    matrix: &crate::temporal::CoChangeMatrix,
    graph: &Graph,
    path: &Path,
) -> M1ndResult<()> {
    let state = matrix.export_state(graph)?;
    let bytes = serde_json::to_vec_pretty(&state)?;
    durable_atomic_write(path, &bytes)
}

/// Load a complete matrix. Unknown versions, digest failures and graph drift
/// are errors; restore never falls back to an empty/bootstrap matrix.
pub fn load_co_change_matrix(
    graph: &Graph,
    path: &Path,
) -> M1ndResult<crate::temporal::CoChangeMatrix> {
    let bytes = std::fs::read(path)?;
    let state: crate::temporal::CoChangeMatrixStateV1 = serde_json::from_slice(&bytes)?;
    crate::temporal::CoChangeMatrix::from_state(graph, state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temporal::{CoChangeMatrix, CoChangeMatrixStateV1};

    fn graph_with_nodes(n: usize) -> Graph {
        let mut g = Graph::new();
        for i in 0..n {
            let id = format!("node_{i}");
            g.add_node(&id, &id, NodeType::File, &[], 0.0, 0.0)
                .expect("add node");
        }
        g
    }

    fn temporal_graph() -> Graph {
        let mut graph = graph_with_nodes(3);
        graph.finalize().expect("finalize temporal graph");
        graph
    }

    #[test]
    fn empty_v4_roundtrip_rebuilds_finalized_csr_storage() {
        let mut graph = Graph::new();
        graph.finalize().expect("finalize empty graph");
        let encoded = encode_graph_json(&graph).expect("encode empty graph");
        let decoded = decode_graph_json(&encoded).expect("decode empty graph");

        assert!(decoded.finalized);
        assert_eq!(decoded.csr.offsets, vec![0]);
        assert_eq!(decoded.csr.rev_offsets, vec![0]);
        assert!(decoded.csr.targets.is_empty());
        assert!(decoded.edge_plasticity.original_weight.is_empty());
    }

    #[test]
    fn v4_roundtrip_preserves_large_fractional_timestamps_bit_exactly() {
        // Without serde_json's `float_roundtrip` parser, this real epoch-scale
        // value has been observed to move by one ULP across checkpoint JSON
        // (`...5382137` -> `...5382135`). Source-projection OCC binds the raw
        // timestamp bits, so a successful checkpoint must preserve them.
        let timestamp = 1_784_446_071.538_213_7_f64;
        let mut graph = graph_with_nodes(1);
        graph.nodes.last_modified[0] = timestamp;
        graph.finalize().expect("finalize timestamp graph");

        let encoded = encode_graph_json(&graph).expect("encode timestamp graph");
        let decoded = decode_graph_json(&encoded).expect("decode timestamp graph");

        assert_eq!(
            decoded.nodes.last_modified[0].to_bits(),
            timestamp.to_bits(),
            "checkpoint JSON must not alter an authority-visible source timestamp"
        );
    }

    #[test]
    fn co_change_roundtrip_preserves_learned_evidence_exactly() {
        let graph = temporal_graph();
        let source = graph.resolve_id("node_0").expect("source");
        let target = graph.resolve_id("node_2").expect("target");
        let mut matrix = CoChangeMatrix::bootstrap(&graph, 128).expect("bootstrap");
        for _ in 0..3 {
            matrix.note_node_appearance(source);
            matrix.note_node_appearance(target);
            matrix
                .record_co_change(source, target, 0.0)
                .expect("record");
        }

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("co_change_state.json");
        save_co_change_matrix(&matrix, &graph, &path).expect("durable save");
        let restored = load_co_change_matrix(&graph, &path).expect("strict restore");

        assert_eq!(restored.num_entries(), matrix.num_entries());
        assert_eq!(restored.predict(source, 10), matrix.predict(source, 10));
        assert!(
            std::fs::read_dir(dir.path())
                .expect("read dir")
                .all(|entry| !entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .contains(".tmp-")),
            "a completed atomic write must not leave staging files"
        );
    }

    #[test]
    fn co_change_restore_rejects_corruption_version_and_graph_drift() {
        let graph = temporal_graph();
        let matrix = CoChangeMatrix::bootstrap(&graph, 128).expect("bootstrap");
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("co_change_state.json");
        save_co_change_matrix(&matrix, &graph, &path).expect("save");

        let pristine = std::fs::read(&path).expect("read pristine");
        let mut state: CoChangeMatrixStateV1 =
            serde_json::from_slice(&pristine).expect("decode state");
        state.total_entries = state.total_entries.saturating_add(1);
        std::fs::write(&path, serde_json::to_vec(&state).expect("encode")).expect("tamper");
        assert!(
            load_co_change_matrix(&graph, &path).is_err(),
            "digest drift must fail"
        );

        let mut state: CoChangeMatrixStateV1 =
            serde_json::from_slice(&pristine).expect("decode version state");
        state.version += 1;
        std::fs::write(&path, serde_json::to_vec(&state).expect("encode")).expect("tamper");
        assert!(
            load_co_change_matrix(&graph, &path).is_err(),
            "unknown version must fail"
        );

        std::fs::write(&path, b"{\"schema\":").expect("truncate");
        assert!(
            load_co_change_matrix(&graph, &path).is_err(),
            "truncation must fail"
        );

        std::fs::write(&path, pristine).expect("restore pristine");
        let mut different_graph = graph_with_nodes(4);
        different_graph
            .finalize()
            .expect("finalize different graph");
        assert!(
            load_co_change_matrix(&different_graph, &path).is_err(),
            "state bound to a different graph must fail, never partially restore"
        );
    }

    fn bak_siblings(dir: &Path, stem: &str) -> Vec<std::path::PathBuf> {
        std::fs::read_dir(dir)
            .expect("read dir")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(stem) && n.contains(".bak-"))
            })
            .collect()
    }

    #[test]
    fn owner_persist_refuses_or_backs_up_on_catastrophic_node_shrink() {
        // Defense in depth (incident 2026-07-15: graph_snapshot.json overwritten
        // 10573 -> 704 nodes with no backup). Overwriting a large canonical
        // snapshot with a catastrophically smaller graph must never lose the big
        // one in silence — the prior snapshot is preserved as a timestamped backup.
        let base =
            std::env::temp_dir().join(format!("m1nd_snapshot_shrink_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);

        // Scenario A — catastrophic shrink (500 -> 10, well under 20%): backup made.
        let dir_a = base.join("a");
        std::fs::create_dir_all(&dir_a).expect("mk a");
        let path_a = dir_a.join("graph_snapshot.json");
        save_graph(&graph_with_nodes(500), &path_a).expect("save big");
        save_graph(&graph_with_nodes(10), &path_a).expect("save tiny (fail-open)");

        let baks = bak_siblings(&dir_a, "graph_snapshot.json");
        assert_eq!(
            baks.len(),
            1,
            "a catastrophic shrink must back up the prior large snapshot before overwriting"
        );
        let backed = load_graph(&baks[0]).expect("the backup is a valid snapshot");
        assert_eq!(
            backed.num_nodes(),
            500,
            "the backup preserves the large graph, not the tiny replacement"
        );
        // The write still happened (fail-open): the live snapshot is the tiny one.
        assert_eq!(load_graph(&path_a).expect("live loads").num_nodes(), 10);

        // Scenario B — a moderate shrink (500 -> 300, above 20%) is normal churn:
        // no backup, the write proceeds untouched.
        let dir_b = base.join("b");
        std::fs::create_dir_all(&dir_b).expect("mk b");
        let path_b = dir_b.join("graph_snapshot.json");
        save_graph(&graph_with_nodes(500), &path_b).expect("save big");
        save_graph(&graph_with_nodes(300), &path_b).expect("save moderate");
        assert!(
            bak_siblings(&dir_b, "graph_snapshot.json").is_empty(),
            "a non-catastrophic shrink must not spawn a backup"
        );

        let _ = std::fs::remove_dir_all(&base);
    }
}
