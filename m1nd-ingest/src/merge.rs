use m1nd_core::error::M1ndResult;
use m1nd_core::graph::{Graph, NodeProvenanceInput};
use m1nd_core::types::{EdgeDirection, EdgeIdx, NodeId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct EdgeKey {
    source: String,
    target: String,
    relation: String,
    direction: u8,
    inhibitory: bool,
}

#[derive(Clone, Debug)]
struct EdgeRecord {
    key: EdgeKey,
    weight: f32,
    causal_strength: f32,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ClaimedEdgeKey {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub direction: u8,
    pub inhibitory: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SourceClaims {
    pub source_hint: Option<String>,
    pub node_ids: Vec<String>,
    pub edges: Vec<ClaimedEdgeKey>,
}

fn is_valid_relative_file_path(rel_path: &str) -> bool {
    crate::is_valid_relative_file_path(rel_path)
}

fn is_valid_external_id(external_id: &str) -> bool {
    let trimmed = external_id.trim();
    if trimmed.is_empty() {
        return false;
    }

    if let Some(rel_path) = trimmed.strip_prefix("file::") {
        return is_valid_relative_file_path(rel_path);
    }

    true
}

fn node_external_ids(graph: &Graph) -> Vec<String> {
    let mut ids = vec![String::new(); graph.num_nodes() as usize];
    for (interned, &node_id) in &graph.id_to_node {
        let idx = node_id.as_usize();
        if idx < ids.len() {
            ids[idx] = graph.strings.resolve(*interned).to_string();
        }
    }
    ids
}

fn canonical_edge_key(
    source: &str,
    target: &str,
    relation: &str,
    direction: EdgeDirection,
    inhibitory: bool,
) -> EdgeKey {
    if direction == EdgeDirection::Bidirectional && source > target {
        EdgeKey {
            source: target.to_string(),
            target: source.to_string(),
            relation: relation.to_string(),
            direction: 1,
            inhibitory,
        }
    } else {
        EdgeKey {
            source: source.to_string(),
            target: target.to_string(),
            relation: relation.to_string(),
            direction: if direction == EdgeDirection::Bidirectional {
                1
            } else {
                0
            },
            inhibitory,
        }
    }
}

fn edge_key_to_claimed(key: &EdgeKey) -> ClaimedEdgeKey {
    ClaimedEdgeKey {
        source: key.source.clone(),
        target: key.target.clone(),
        relation: key.relation.clone(),
        direction: key.direction,
        inhibitory: key.inhibitory,
    }
}

fn claimed_to_edge_key(key: &ClaimedEdgeKey) -> EdgeKey {
    EdgeKey {
        source: key.source.clone(),
        target: key.target.clone(),
        relation: key.relation.clone(),
        direction: key.direction,
        inhibitory: key.inhibitory,
    }
}

fn merge_tags(existing: &[String], incoming: &[String]) -> Vec<String> {
    let mut merged = Vec::with_capacity(existing.len() + incoming.len());
    let mut seen = HashSet::new();
    for tag in existing.iter().chain(incoming.iter()) {
        if seen.insert(tag.clone()) {
            merged.push(tag.clone());
        }
    }
    merged
}

fn collect_edges(graph: &Graph) -> (Vec<EdgeRecord>, u64) {
    let node_ids = node_external_ids(graph);
    let mut out = Vec::new();
    let mut skipped_invalid_edges = 0u64;

    for src in 0..graph.num_nodes() as usize {
        if !is_valid_external_id(&node_ids[src]) {
            continue;
        }

        for edge_idx in graph.csr.out_range(NodeId::new(src as u32)) {
            let target = graph.csr.targets[edge_idx].as_usize();
            let direction = graph.csr.directions[edge_idx];
            if direction == EdgeDirection::Bidirectional && src > target {
                continue;
            }

            if !is_valid_external_id(&node_ids[target]) {
                skipped_invalid_edges += 1;
                continue;
            }

            let relation = graph
                .strings
                .resolve(graph.csr.relations[edge_idx])
                .to_string();
            out.push(EdgeRecord {
                key: canonical_edge_key(
                    &node_ids[src],
                    &node_ids[target],
                    &relation,
                    direction,
                    graph.csr.inhibitory[edge_idx],
                ),
                weight: graph.csr.read_weight(EdgeIdx::new(edge_idx as u32)).get(),
                causal_strength: graph.csr.causal_strengths[edge_idx].get(),
            });
        }
    }

    (out, skipped_invalid_edges)
}

pub fn collect_source_claims(graph: &Graph) -> SourceClaims {
    let node_ids = node_external_ids(graph);
    let mut claims = SourceClaims::default();
    let mut claimed_nodes = HashSet::new();
    let mut claimed_edges = HashSet::new();

    for (idx, external_id) in node_ids.iter().enumerate() {
        if !is_valid_external_id(external_id) {
            continue;
        }

        let provenance = graph.resolve_node_provenance(NodeId::new(idx as u32));
        if provenance.canonical {
            if claims.source_hint.is_none() {
                claims.source_hint = provenance.source_path.clone();
            }
            if claimed_nodes.insert(external_id.clone()) {
                claims.node_ids.push(external_id.clone());
            }
        }
    }

    let (edges, _) = collect_edges(graph);
    for record in edges {
        let claimed = edge_key_to_claimed(&record.key);
        if claimed_edges.insert(claimed.clone()) {
            claims.edges.push(claimed);
        }
    }

    claims
}

pub fn prune_source_claims(
    base: &Graph,
    target_source: &str,
    claims_by_source: &HashMap<String, SourceClaims>,
) -> M1ndResult<Graph> {
    let Some(target_claims) = claims_by_source.get(target_source) else {
        return merge_graphs(&Graph::new(), base);
    };

    let mut node_claim_counts: HashMap<&str, usize> = HashMap::new();
    let mut edge_claim_counts: HashMap<ClaimedEdgeKey, usize> = HashMap::new();

    for (source, claims) in claims_by_source {
        if source == target_source {
            continue;
        }
        for node_id in &claims.node_ids {
            *node_claim_counts.entry(node_id.as_str()).or_insert(0) += 1;
        }
        for edge in &claims.edges {
            *edge_claim_counts.entry(edge.clone()).or_insert(0) += 1;
        }
    }

    let removed_node_ids: HashSet<String> = target_claims
        .node_ids
        .iter()
        .filter(|node_id| !node_claim_counts.contains_key(node_id.as_str()))
        .cloned()
        .collect();
    let removed_edge_keys: HashSet<EdgeKey> = target_claims
        .edges
        .iter()
        .filter(|edge| !edge_claim_counts.contains_key(*edge))
        .map(claimed_to_edge_key)
        .collect();

    let base_ids = node_external_ids(base);
    let mut pruned = Graph::with_capacity(base.num_nodes() as usize, base.num_edges());
    let mut retained_node_ids = HashSet::new();

    for (idx, external_id) in base_ids.iter().enumerate() {
        if !is_valid_external_id(external_id) || removed_node_ids.contains(external_id) {
            continue;
        }

        let label = base.strings.resolve(base.nodes.label[idx]).to_string();
        let tags: Vec<String> = base.nodes.tags[idx]
            .iter()
            .map(|&tag| base.strings.resolve(tag).to_string())
            .collect();
        let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
        let node_id = pruned.add_node(
            external_id,
            &label,
            base.nodes.node_type[idx],
            &tag_refs,
            base.nodes.last_modified[idx],
            base.nodes.change_frequency[idx].get(),
        )?;
        let provenance = base.resolve_node_provenance(NodeId::new(idx as u32));
        pruned.set_node_provenance(
            node_id,
            NodeProvenanceInput {
                source_path: provenance.source_path.as_deref(),
                line_start: provenance.line_start,
                line_end: provenance.line_end,
                excerpt: provenance.excerpt.as_deref(),
                namespace: provenance.namespace.as_deref(),
                canonical: provenance.canonical,
            },
        );
        retained_node_ids.insert(external_id.clone());
    }

    for src in 0..base.num_nodes() as usize {
        let src_ext = &base_ids[src];
        if !retained_node_ids.contains(src_ext) {
            continue;
        }

        for edge_idx in base.csr.out_range(NodeId::new(src as u32)) {
            let target = base.csr.targets[edge_idx].as_usize();
            let tgt_ext = &base_ids[target];
            if !retained_node_ids.contains(tgt_ext) {
                continue;
            }

            let direction = base.csr.directions[edge_idx];
            if direction == EdgeDirection::Bidirectional && src > target {
                continue;
            }

            let relation = base
                .strings
                .resolve(base.csr.relations[edge_idx])
                .to_string();
            let edge_key = canonical_edge_key(
                src_ext,
                tgt_ext,
                &relation,
                direction,
                base.csr.inhibitory[edge_idx],
            );
            if removed_edge_keys.contains(&edge_key) {
                continue;
            }

            let source = pruned
                .resolve_id(src_ext)
                .expect("retained source node must exist");
            let target = pruned
                .resolve_id(tgt_ext)
                .expect("retained target node must exist");
            pruned.add_edge(
                source,
                target,
                &relation,
                base.csr.read_weight(EdgeIdx::new(edge_idx as u32)),
                direction,
                base.csr.inhibitory[edge_idx],
                base.csr.causal_strengths[edge_idx],
            )?;
        }
    }

    if pruned.num_nodes() > 0 {
        pruned.finalize()?;
    }

    Ok(pruned)
}

pub fn merge_graphs(base: &Graph, overlay: &Graph) -> M1ndResult<Graph> {
    let base_ids = node_external_ids(base);
    let overlay_ids = node_external_ids(overlay);
    let mut merged = Graph::with_capacity(
        base.num_nodes() as usize + overlay.num_nodes() as usize,
        base.num_edges() + overlay.num_edges(),
    );

    for graph in [base, overlay] {
        let external_ids = if std::ptr::eq(graph, base) {
            &base_ids
        } else {
            &overlay_ids
        };

        #[allow(clippy::needless_range_loop)]
        for idx in 0..graph.num_nodes() as usize {
            let external_id = &external_ids[idx];
            if !is_valid_external_id(external_id) {
                eprintln!(
                    "[m1nd-ingest] WARNING: skipping invalid external_id during merge: {:?}",
                    external_id
                );
                continue;
            }

            let label = graph.strings.resolve(graph.nodes.label[idx]).to_string();
            let tags: Vec<String> = graph.nodes.tags[idx]
                .iter()
                .map(|&tag| graph.strings.resolve(tag).to_string())
                .collect();

            if let Some(existing) = merged.resolve_id(external_id) {
                let existing_idx = existing.as_usize();
                let current_tags: Vec<String> = merged.nodes.tags[existing_idx]
                    .iter()
                    .map(|&tag| merged.strings.resolve(tag).to_string())
                    .collect();
                let merged_tags = merge_tags(&current_tags, &tags);
                merged.nodes.tags[existing_idx] = merged_tags
                    .iter()
                    .map(|tag| merged.strings.get_or_intern(tag))
                    .collect();
                merged.nodes.last_modified[existing_idx] =
                    merged.nodes.last_modified[existing_idx].max(graph.nodes.last_modified[idx]);
                let provenance = graph.resolve_node_provenance(NodeId::new(idx as u32));
                merged.merge_node_provenance(
                    existing,
                    NodeProvenanceInput {
                        source_path: provenance.source_path.as_deref(),
                        line_start: provenance.line_start,
                        line_end: provenance.line_end,
                        excerpt: provenance.excerpt.as_deref(),
                        namespace: provenance.namespace.as_deref(),
                        canonical: provenance.canonical,
                    },
                );
                continue;
            }

            let tag_refs: Vec<&str> = tags.iter().map(String::as_str).collect();
            let node_id = merged.add_node(
                external_id,
                &label,
                graph.nodes.node_type[idx],
                &tag_refs,
                graph.nodes.last_modified[idx],
                graph.nodes.change_frequency[idx].get(),
            )?;
            let provenance = graph.resolve_node_provenance(NodeId::new(idx as u32));
            merged.set_node_provenance(
                node_id,
                NodeProvenanceInput {
                    source_path: provenance.source_path.as_deref(),
                    line_start: provenance.line_start,
                    line_end: provenance.line_end,
                    excerpt: provenance.excerpt.as_deref(),
                    namespace: provenance.namespace.as_deref(),
                    canonical: provenance.canonical,
                },
            );
        }
    }

    // Parallel edges — two edges between the same pair carrying the same
    // relation, direction and inhibitory flag — are legal graph content, not
    // noise to fold away. `Graph::add_edge` admits them; the snapshot round trip
    // writes and reads back one row per CSR slot (`edge_slot_queues` /
    // `pop_unconsumed_slot` exist for exactly that); and plasticity binds the
    // rows a clean shutdown persisted to those slots POSITIONALLY (#442). A
    // merge that collapsed a key's occurrences onto one edge would erase real
    // structure and leave the sidecar's rows pointing at slots it just deleted —
    // reintroducing, from the writer, the ambiguity #442 hardened the reader
    // against.
    //
    // So the merge is positional too: the Nth occurrence of a key in the base
    // pairs with the Nth in the overlay, and the side with more occurrences sets
    // the multiplicity. Re-ingesting the same source therefore still cannot
    // double an edge (max(1, 1) == 1), which is what the old fold bought.
    let mut edge_records: HashMap<EdgeKey, Vec<EdgeRecord>> = HashMap::new();
    let (base_edges, skipped_base_edges) = collect_edges(base);
    let (overlay_edges, skipped_overlay_edges) = collect_edges(overlay);

    for record in base_edges {
        edge_records
            .entry(record.key.clone())
            .or_default()
            .push(record);
    }

    let mut overlay_occurrences: HashMap<EdgeKey, usize> = HashMap::new();
    for record in overlay_edges {
        let occurrence = *overlay_occurrences
            .entry(record.key.clone())
            .and_modify(|seen| *seen += 1)
            .or_insert(0);
        let slots = edge_records.entry(record.key.clone()).or_default();
        match slots.get_mut(occurrence) {
            Some(existing) => {
                existing.weight = existing.weight.max(record.weight);
                existing.causal_strength = existing.causal_strength.max(record.causal_strength);
            }
            None => slots.push(record),
        }
    }

    for record in edge_records.values().flatten() {
        let source = merged.resolve_id(&record.key.source).unwrap();
        let target = merged.resolve_id(&record.key.target).unwrap();
        merged.add_edge(
            source,
            target,
            &record.key.relation,
            m1nd_core::types::FiniteF32::new(record.weight),
            if record.key.direction == 1 {
                EdgeDirection::Bidirectional
            } else {
                EdgeDirection::Forward
            },
            record.key.inhibitory,
            m1nd_core::types::FiniteF32::new(record.causal_strength),
        )?;
    }

    if merged.num_nodes() > 0 {
        merged.finalize()?;
    }

    if skipped_base_edges > 0 || skipped_overlay_edges > 0 {
        eprintln!(
            "[m1nd-ingest] merge hygiene summary: skipped {} invalid base edges, {} invalid overlay edges",
            skipped_base_edges, skipped_overlay_edges
        );
    }

    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use m1nd_core::types::{FiniteF32, NodeType};

    #[test]
    fn merge_graphs_preserves_base_and_overlay_nodes() {
        let mut base = Graph::with_capacity(2, 1);
        let a = base
            .add_node(
                "file::alpha.rs",
                "alpha.rs",
                NodeType::File,
                &["code"],
                10.0,
                0.3,
            )
            .unwrap();
        let b = base
            .add_node(
                "file::beta.rs",
                "beta.rs",
                NodeType::File,
                &["code"],
                20.0,
                0.3,
            )
            .unwrap();
        base.add_edge(
            a,
            b,
            "references",
            FiniteF32::new(0.7),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.3),
        )
        .unwrap();
        base.finalize().unwrap();

        let mut overlay = Graph::with_capacity(2, 1);
        let note = overlay
            .add_node(
                "memory::memory::file::daily-note",
                "daily-note",
                NodeType::File,
                &["memory"],
                30.0,
                0.6,
            )
            .unwrap();
        let state = overlay
            .add_node(
                "memory::memory::entry::batman-mode",
                "Batman mode",
                NodeType::Concept,
                &["memory", "memory:state"],
                30.0,
                0.5,
            )
            .unwrap();
        overlay
            .add_edge(
                note,
                state,
                "contains",
                FiniteF32::ONE,
                EdgeDirection::Bidirectional,
                false,
                FiniteF32::new(0.8),
            )
            .unwrap();
        overlay.finalize().unwrap();

        let merged = merge_graphs(&base, &overlay).unwrap();

        assert!(merged.resolve_id("file::alpha.rs").is_some());
        assert!(merged
            .resolve_id("memory::memory::entry::batman-mode")
            .is_some());
        assert!(merged.num_edges() >= 2);
    }

    /// Build a graph whose one file node `contains` one struct node over
    /// `twins` edges that share their COMPLETE synaptic key — the shape a
    /// clean shutdown turns into `twins` plasticity rows under one key (#442).
    fn graph_with_parallel_contains(twins: usize) -> Graph {
        let mut graph = Graph::with_capacity(2, twins);
        let file = graph
            .add_node(
                "file::Cargo.toml",
                "Cargo.toml",
                NodeType::File,
                &["code"],
                10.0,
                0.3,
            )
            .unwrap();
        let table = graph
            .add_node(
                "file::Cargo.toml::struct::package",
                "package",
                NodeType::Struct,
                &["code"],
                10.0,
                0.3,
            )
            .unwrap();
        for _ in 0..twins {
            graph
                .add_edge(
                    file,
                    table,
                    "contains",
                    FiniteF32::new(0.8),
                    EdgeDirection::Forward,
                    false,
                    FiniteF32::new(0.4),
                )
                .unwrap();
        }
        graph.finalize().unwrap();
        graph
    }

    /// The merge must preserve parallel edges — and must still not breed them.
    ///
    /// RED before the positional merge: the fold keyed on `EdgeKey` collapsed
    /// every occurrence of a key onto one edge, so the merge behind `memorize`
    /// silently deleted a slot the snapshot and the plasticity sidecar both
    /// address by position (#442). The three legs pin the whole rule, because
    /// preserving the pair is only correct if re-ingesting a source still
    /// cannot double its edges.
    #[test]
    fn merge_graphs_preserves_parallel_edges_without_breeding_them() {
        let twinned = graph_with_parallel_contains(2);
        let single = graph_with_parallel_contains(1);

        assert_eq!(
            merge_graphs(&twinned, &twinned).unwrap().num_edges(),
            2,
            "a parallel pair re-ingested from the same source must stay a pair"
        );
        assert_eq!(
            merge_graphs(&twinned, &single).unwrap().num_edges(),
            2,
            "an overlay carrying one edge must not erase the base's second slot"
        );
        assert_eq!(
            merge_graphs(&single, &twinned).unwrap().num_edges(),
            2,
            "an overlay carrying a pair must bring its second slot into the merge"
        );
        assert_eq!(
            merge_graphs(&single, &single).unwrap().num_edges(),
            1,
            "one edge merged with itself must stay one edge — the merge must \
             never breed a parallel edge out of a plain re-ingest"
        );
    }

    #[test]
    fn merge_graphs_skips_invalid_external_ids_and_edges() {
        let mut base = Graph::with_capacity(1, 0);
        base.add_node(
            "file::alpha.rs",
            "alpha.rs",
            NodeType::File,
            &["code"],
            10.0,
            0.3,
        )
        .unwrap();
        base.finalize().unwrap();

        let mut overlay = Graph::with_capacity(2, 1);
        let invalid = overlay
            .add_node("", "broken.rs", NodeType::File, &["code"], 20.0, 0.3)
            .unwrap();
        let valid = overlay
            .add_node(
                "file::beta.rs",
                "beta.rs",
                NodeType::File,
                &["code"],
                20.0,
                0.3,
            )
            .unwrap();
        overlay
            .add_edge(
                invalid,
                valid,
                "references",
                FiniteF32::new(0.6),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.2),
            )
            .unwrap();
        overlay.finalize().unwrap();

        let merged = merge_graphs(&base, &overlay).unwrap();

        assert!(merged.resolve_id("").is_none());
        assert!(merged.resolve_id("file::alpha.rs").is_some());
        assert!(merged.resolve_id("file::beta.rs").is_some());
        assert_eq!(merged.num_nodes(), 2);
        assert_eq!(merged.num_edges(), 0);
    }
}
