// === crates/m1nd-core/src/snapshot.rs ===

use crate::error::{M1ndError, M1ndResult};
use crate::graph::{Graph, NodeProvenanceInput, ResolvedNodeProvenance};
use crate::plasticity::SynapticState;
use crate::types::*;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Snapshot — JSON graph persistence
// FM-PL-008 fix: atomic write (write to temp, rename).
// ---------------------------------------------------------------------------

/// Graph snapshot format version.
pub const SNAPSHOT_VERSION: u32 = 3;

// ---------------------------------------------------------------------------
// Serialization types
// ---------------------------------------------------------------------------

#[derive(serde::Serialize, serde::Deserialize)]
struct GraphSnapshot {
    version: u32,
    nodes: Vec<NodeSnapshot>,
    edges: Vec<EdgeSnapshot>,
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
struct EdgeSnapshot {
    source_id: String,
    target_id: String,
    relation: String,
    weight: f32,
    direction: u8, // 0=Forward, 1=Bidirectional
    inhibitory: bool,
    causal_strength: f32,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CoChangeMetadata {
    version: u32,
    num_entries: u64,
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

/// Save full graph to JSON snapshot. Atomic write: temp file + rename (FM-PL-008).
/// Serializes all nodes and edges so the graph can be fully reconstructed on load.
pub fn save_graph(graph: &Graph, path: &Path) -> M1ndResult<()> {
    let n = graph.num_nodes() as usize;

    // Defense in depth: never silently overwrite a large snapshot with a tiny one.
    let _ = backup_if_catastrophic_shrink(path, n);

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

    // Serialize edges from CSR (deduplicate bidirectional: only save source < target)
    let mut edges = Vec::new();
    for src in 0..n {
        let range = graph.csr.out_range(NodeId::new(src as u32));
        for j in range {
            let tgt = graph.csr.targets[j].as_usize();
            let dir = graph.csr.directions[j];
            // For bidirectional edges, only save the canonical direction (source < target)
            if dir == EdgeDirection::Bidirectional && src > tgt {
                continue;
            }
            let relation = graph.strings.resolve(graph.csr.relations[j]).to_string();
            let weight = graph.csr.read_weight(EdgeIdx::new(j as u32)).get();
            edges.push(EdgeSnapshot {
                source_id: node_to_ext_id[src].clone(),
                target_id: node_to_ext_id[tgt].clone(),
                relation,
                weight,
                direction: if dir == EdgeDirection::Bidirectional {
                    1
                } else {
                    0
                },
                inhibitory: graph.csr.inhibitory[j],
                causal_strength: graph.csr.causal_strengths[j].get(),
            });
        }
    }

    let snapshot = GraphSnapshot {
        version: SNAPSHOT_VERSION,
        nodes,
        edges,
    };

    let json = serde_json::to_string(&snapshot).map_err(M1ndError::Serde)?;

    // FM-PL-008: atomic write via temp file + rename
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

/// Load full graph from JSON snapshot. Reconstructs the complete graph
/// with all nodes, edges, CSR, and PageRank.
pub fn load_graph(path: &Path) -> M1ndResult<Graph> {
    let data = std::fs::read_to_string(path)?;
    let snapshot: GraphSnapshot = serde_json::from_str(&data).map_err(M1ndError::Serde)?;

    if snapshot.nodes.is_empty() {
        return Ok(Graph::new());
    }

    let mut graph = Graph::with_capacity(snapshot.nodes.len(), snapshot.edges.len());

    // Add all nodes
    for node in &snapshot.nodes {
        let tags: Vec<&str> = node.tags.iter().map(|s| s.as_str()).collect();
        if let Ok(node_id) = graph.add_node(
            &node.external_id,
            &node.label,
            u8_to_node_type(node.node_type),
            &tags,
            node.last_modified,
            node.change_frequency,
        ) {
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
    }

    // Add all edges
    for edge in &snapshot.edges {
        if let (Some(src), Some(tgt)) = (
            graph.resolve_id(&edge.source_id),
            graph.resolve_id(&edge.target_id),
        ) {
            let direction = if edge.direction == 1 {
                EdgeDirection::Bidirectional
            } else {
                EdgeDirection::Forward
            };
            let _ = graph.add_edge(
                src,
                tgt,
                &edge.relation,
                FiniteF32::new(edge.weight),
                direction,
                edge.inhibitory,
                FiniteF32::new(edge.causal_strength),
            );
        }
    }

    // Finalize: build CSR + PageRank
    if graph.num_nodes() > 0 {
        graph.finalize()?;
    }

    Ok(graph)
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

/// Save co-change matrix metadata.
pub fn save_co_change_matrix(
    _matrix: &crate::temporal::CoChangeMatrix,
    path: &Path,
) -> M1ndResult<()> {
    let meta = CoChangeMetadata {
        version: 1,
        num_entries: _matrix.num_entries(),
    };
    let json = serde_json::to_string_pretty(&meta).map_err(M1ndError::Serde)?;

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

/// Load co-change matrix.
pub fn load_co_change_matrix(path: &Path) -> M1ndResult<crate::temporal::CoChangeMatrix> {
    let data = std::fs::read_to_string(path)?;
    let _meta: CoChangeMetadata = serde_json::from_str(&data).map_err(M1ndError::Serde)?;

    // Return empty matrix; full deserialization needs graph context
    let graph = Graph::new();
    crate::temporal::CoChangeMatrix::bootstrap(&graph, 500_000)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph_with_nodes(n: usize) -> Graph {
        let mut g = Graph::new();
        for i in 0..n {
            let id = format!("node_{i}");
            g.add_node(&id, &id, NodeType::File, &[], 0.0, 0.0)
                .expect("add node");
        }
        g
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
