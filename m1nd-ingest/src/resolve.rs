// === crates/m1nd-ingest/src/resolve.rs ===

use m1nd_core::error::M1ndResult;
use m1nd_core::graph::Graph;
use m1nd_core::types::*;

// ---------------------------------------------------------------------------
// ReferenceResolver — resolve ref:: edges to actual nodes
// FM-ING-008 fix: multi-value index + proximity disambiguation (not dict overwrite).
// Replaces: ingest.py CodebaseIngestor._resolve_references()
// ---------------------------------------------------------------------------

/// Resolves unresolved references (e.g., "ref::Config") to actual graph nodes.
/// FM-ING-008 fix: when multiple nodes match a label, uses proximity
/// disambiguation (same file > same directory > same module) instead of
/// silently shadowing with dict overwrite.
pub struct ReferenceResolver;

/// Resolution outcome for a single reference.
#[derive(Clone, Debug)]
pub struct ResolvedReference {
    pub source: NodeId,
    pub target: NodeId,
    pub relation: InternedStr,
    pub confidence: FiniteF32,
}

/// Summary of resolution results.
#[derive(Clone, Debug)]
pub struct ResolutionStats {
    pub resolved: u64,
    pub unresolved: u64,
    pub ambiguous: u64,
}

impl ReferenceResolver {
    /// Resolve all unresolved references in the graph.
    /// Uses multi-value label index + proximity disambiguation (FM-ING-008).
    /// Task #8: import_hint (4th tuple element) carries the import path for
    /// module-aware disambiguation (e.g., "from foo.bar import Baz" hints "foo/bar").
    /// Replaces: ingest.py CodebaseIngestor._resolve_references()
    pub fn resolve(
        graph: &mut Graph,
        unresolved: &[(String, String, String)], // (source_id, target_label, relation)
    ) -> M1ndResult<ResolutionStats> {
        // Upgrade: also accept optional import hints via resolve_with_hints
        Self::resolve_with_hints(graph, unresolved, &[])
    }

    /// Resolve with optional import-path hints for module-aware disambiguation.
    /// `import_hints` maps (source_id, target_label) -> import_path so that
    /// e.g. "from foo.bar import Baz" prefers the Baz node under foo/bar.
    pub fn resolve_with_hints(
        graph: &mut Graph,
        unresolved: &[(String, String, String)], // (source_id, target_label, relation)
        import_hints: &[(String, String, String)], // (source_id, target_label, import_path)
    ) -> M1ndResult<ResolutionStats> {
        let label_index = Self::build_label_index(graph);
        let mut stats = ResolutionStats {
            resolved: 0,
            unresolved: 0,
            ambiguous: 0,
        };

        // Build a quick lookup for import hints: (source, target_label) -> import_path
        let hint_map: std::collections::HashMap<(&str, &str), &str> = import_hints
            .iter()
            .map(|(s, t, p)| ((s.as_str(), t.as_str()), p.as_str()))
            .collect();

        for (source_id, target_label, relation) in unresolved {
            // Look up source node
            let source = match graph.resolve_id(source_id) {
                Some(id) => id,
                None => {
                    stats.unresolved += 1;
                    continue;
                }
            };

            // Strip "ref::" prefix if present
            let clean_label = target_label.strip_prefix("ref::").unwrap_or(target_label);

            // Extract the last segment from import path for matching
            // e.g., "m1nd_core::graph::Graph" -> "Graph"
            let last_segment = clean_label.rsplit("::").next().unwrap_or(clean_label);

            // Check for import path hint (Task #8)
            let import_hint = hint_map
                .get(&(source_id.as_str(), target_label.as_str()))
                .copied();

            // Look up by label in the graph's string interner
            let label_interned = match graph.strings.lookup(last_segment) {
                Some(id) => id,
                None => {
                    // Try matching by suffix (e.g., "Config" matches "module::Config")
                    let mut found = Vec::new();
                    for i in 0..graph.num_nodes() as usize {
                        let node_label = graph.strings.resolve(graph.nodes.label[i]);
                        if node_label == last_segment
                            || node_label == clean_label
                            || clean_label.ends_with(node_label)
                        {
                            found.push(NodeId::new(i as u32));
                        }
                    }
                    if found.is_empty() {
                        stats.unresolved += 1;
                        continue;
                    }
                    if found.len() > 1 {
                        stats.ambiguous += 1;
                    }
                    // Use first match (or disambiguate if multiple)
                    let target = if found.len() == 1 {
                        found[0]
                    } else if let Some(hint) = import_hint {
                        Self::disambiguate_with_hint(graph, source, &found, hint).unwrap_or_else(
                            || Self::disambiguate(graph, source, &found).unwrap_or(found[0]),
                        )
                    } else {
                        Self::disambiguate(graph, source, &found).unwrap_or(found[0])
                    };

                    // Add edge
                    let rel = relation.as_str();
                    let _ = graph.add_edge(
                        source,
                        target,
                        rel,
                        FiniteF32::new(0.5),
                        EdgeDirection::Forward,
                        false,
                        FiniteF32::new(0.4),
                    );
                    stats.resolved += 1;
                    continue;
                }
            };

            // Found by exact interned match (using last segment)
            if let Some(candidates) = label_index.get(&label_interned) {
                if candidates.is_empty() {
                    stats.unresolved += 1;
                    continue;
                }
                if candidates.len() > 1 {
                    stats.ambiguous += 1;
                }

                let target = if candidates.len() == 1 {
                    candidates[0]
                } else if let Some(hint) = import_hint {
                    Self::disambiguate_with_hint(graph, source, candidates, hint).unwrap_or_else(
                        || Self::disambiguate(graph, source, candidates).unwrap_or(candidates[0]),
                    )
                } else {
                    Self::disambiguate(graph, source, candidates).unwrap_or(candidates[0])
                };

                let rel = relation.as_str();
                let _ = graph.add_edge(
                    source,
                    target,
                    rel,
                    FiniteF32::new(0.5),
                    EdgeDirection::Forward,
                    false,
                    FiniteF32::ZERO,
                );
                stats.resolved += 1;
            } else {
                stats.unresolved += 1;
            }
        }

        Ok(stats)
    }

    /// Build label-to-nodes index (multi-value, not single-value).
    /// FM-ING-008 fix: returns Vec of candidates, not single overwrite.
    fn build_label_index(graph: &Graph) -> std::collections::HashMap<InternedStr, Vec<NodeId>> {
        let mut index: std::collections::HashMap<InternedStr, Vec<NodeId>> =
            std::collections::HashMap::new();
        for i in 0..graph.num_nodes() as usize {
            let label = graph.nodes.label[i];
            index.entry(label).or_default().push(NodeId::new(i as u32));
        }
        index
    }

    /// Disambiguate among multiple candidates using proximity.
    /// Priority: same file > same directory > same module > first match.
    fn disambiguate(graph: &Graph, source: NodeId, candidates: &[NodeId]) -> Option<NodeId> {
        if candidates.is_empty() {
            return None;
        }

        // Get source's external ID to compute proximity
        let source_ext_id = Self::find_external_id(graph, source)?;

        // Score each candidate by proximity to source
        let mut best = candidates[0];
        let mut best_score = 0u32;

        for &candidate in candidates {
            if let Some(cand_ext_id) = Self::find_external_id(graph, candidate) {
                let score = Self::proximity_score(&source_ext_id, &cand_ext_id);
                if score > best_score {
                    best_score = score;
                    best = candidate;
                }
            }
        }

        Some(best)
    }

    /// Find external ID string for a node.
    fn find_external_id(graph: &Graph, node: NodeId) -> Option<String> {
        for (interned, &nid) in &graph.id_to_node {
            if nid == node {
                return Some(graph.strings.resolve(*interned).to_string());
            }
        }
        None
    }

    /// Disambiguate among multiple candidates using an import path hint.
    /// If a candidate's external ID contains path segments matching the import hint,
    /// prefer that candidate. E.g., import hint "foo.bar" matches candidate
    /// "file::foo/bar.py::class::Baz".
    fn disambiguate_with_hint(
        graph: &Graph,
        _source: NodeId,
        candidates: &[NodeId],
        import_hint: &str,
    ) -> Option<NodeId> {
        if candidates.is_empty() || import_hint.is_empty() {
            return None;
        }

        // Normalize the import hint: "foo.bar" -> ["foo", "bar"] and also "foo/bar"
        let hint_parts: Vec<&str> = import_hint.split('.').collect();
        let hint_as_path = hint_parts.join("/");
        let hint_as_colons = hint_parts.join("::");

        let mut best: Option<NodeId> = None;
        let mut best_score = 0u32;

        for &candidate in candidates {
            if let Some(cand_ext_id) = Self::find_external_id(graph, candidate) {
                let mut score = 0u32;
                // Check if candidate's ID contains the import path segments
                if cand_ext_id.contains(&hint_as_path) {
                    score += 200;
                }
                if cand_ext_id.contains(&hint_as_colons) {
                    score += 200;
                }
                // Partial match: check individual segments
                for part in &hint_parts {
                    if cand_ext_id.contains(part) {
                        score += 10;
                    }
                }
                if score > best_score {
                    best_score = score;
                    best = Some(candidate);
                }
            }
        }

        // Only return if we actually found a match via the hint
        if best_score > 0 {
            best
        } else {
            None
        }
    }

    /// Split an external id into proximity segments, treating BOTH `::` and the
    /// path separator `/` as boundaries. Ids look like
    /// `file::m1nd-core/src/graph.rs::fn::resolve`; splitting only on `::` left
    /// the whole directory path (`m1nd-core/src/graph.rs`) as one segment, so two
    /// candidates in the SAME directory but different files scored identically to
    /// a candidate in a different crate — the same-directory preference promised
    /// by `proximity_score` was unreachable. Splitting on `/` too makes each
    /// directory component comparable, restoring that preference.
    fn proximity_segments(id: &str) -> Vec<&str> {
        id.split("::")
            .flat_map(|seg| seg.split('/'))
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Compute proximity score between two external IDs.
    /// Higher = closer. The count of matching leading segments is itself a
    /// depth-agnostic proximity measure: candidates sharing more of the
    /// `file::<crate>/<dirs…>/<file>` prefix are closer, with the deepest
    /// directory components dominating. A `SAME_FILE_BONUS` guarantees a
    /// candidate in the SAME file always outranks a same-directory one, at any
    /// path depth (shallow ids like `file::x.rs::fn::y` and deep ids like
    /// `file::crate/src/m.rs::fn::y` both behave correctly). Only the ORDERING
    /// matters — the sole caller, `disambiguate`, compares scores with `>`.
    fn proximity_score(source_id: &str, candidate_id: &str) -> u32 {
        let src_parts = Self::proximity_segments(source_id);
        let cand_parts = Self::proximity_segments(candidate_id);

        // Matching leading segments: file::, crate, src, dir…, file, kind, name.
        let mut matching = 0u32;
        for (a, b) in src_parts.iter().zip(cand_parts.iter()) {
            if a == b {
                matching += 1;
            } else {
                break;
            }
        }

        // Same file iff everything up to and including the filename matches, i.e.
        // the parts agree except for the trailing `[kind, name]` (last two). This
        // dominates directory proximity so an in-file definition always wins.
        const SAME_FILE_BONUS: u32 = 1000;
        let src_path_len = src_parts.len().saturating_sub(2);
        let cand_path_len = cand_parts.len().saturating_sub(2);
        let same_file = src_path_len > 0
            && src_path_len == cand_path_len
            && (matching as usize) >= src_path_len;

        matching + if same_file { SAME_FILE_BONUS } else { 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use m1nd_core::types::NodeType;

    fn fn_node(graph: &mut Graph, ext_id: &str, label: &str) -> NodeId {
        graph
            .add_node(ext_id, label, NodeType::Function, &[], 0.0, 0.0)
            .expect("add_node")
    }

    // The edge target a `calls` ref resolved to, found by scanning pending edges.
    fn calls_target(graph: &Graph, source: NodeId) -> Option<NodeId> {
        graph
            .csr
            .pending_edges
            .iter()
            .find(|e| e.source == source && graph.strings.resolve(e.relation) == "calls")
            .map(|e| e.target)
    }

    /// proximity_score must prefer a SAME-DIRECTORY candidate over a candidate in
    /// a different crate. With the old `::`-only split these tied (both 10) and the
    /// directory preference promised by the function was unreachable; the `/`-split
    /// makes the same-directory candidate score strictly higher.
    #[test]
    fn proximity_prefers_same_directory_over_cross_crate() {
        let caller = "file::crate_a/src/walker.rs::fn::walk";
        let same_dir = "file::crate_a/src/policy.rs::fn::helper";
        let cross_crate = "file::crate_b/src/other.rs::fn::helper";
        let s_same = ReferenceResolver::proximity_score(caller, same_dir);
        let s_cross = ReferenceResolver::proximity_score(caller, cross_crate);
        assert!(
            s_same > s_cross,
            "same-dir ({s_same}) must outrank cross-crate ({s_cross})"
        );
    }

    /// A same-FILE candidate must dominate a same-directory one at any path depth
    /// (guards the SAME_FILE_BONUS for both deep and shallow ids).
    #[test]
    fn proximity_same_file_dominates_same_directory() {
        let caller = "file::crate_a/src/m.rs::fn::a";
        let same_file = "file::crate_a/src/m.rs::fn::b";
        let same_dir = "file::crate_a/src/n.rs::fn::b";
        assert!(
            ReferenceResolver::proximity_score(caller, same_file)
                > ReferenceResolver::proximity_score(caller, same_dir)
        );
        // Shallow ids (no crate/src path) must behave the same way.
        let c2 = "file::x.rs::fn::a";
        let same_file2 = "file::x.rs::fn::b";
        let diff_file2 = "file::y.rs::fn::b";
        assert!(
            ReferenceResolver::proximity_score(c2, same_file2)
                > ReferenceResolver::proximity_score(c2, diff_file2)
        );
    }

    /// End-to-end regression for the cross-file `calls` mis-binding: a caller in
    /// `crate_a/src/walker.rs` calling `helper`, which is defined in BOTH
    /// `crate_a/src/policy.rs` (same directory — the correct target) and
    /// `crate_b/src/other.rs` (a different crate — wrong). The WRONG candidate is
    /// inserted FIRST so a pure `candidates[0]` tie-break would pick it; the
    /// proximity fix must instead bind to the same-directory definition.
    #[test]
    fn calls_edge_binds_same_directory_not_cross_crate() {
        let mut graph = Graph::new();
        let caller = fn_node(&mut graph, "file::crate_a/src/walker.rs::fn::walk", "walk");
        // Insert the WRONG (cross-crate) candidate before the correct same-dir one.
        let wrong = fn_node(
            &mut graph,
            "file::crate_b/src/other.rs::fn::helper",
            "helper",
        );
        let correct = fn_node(
            &mut graph,
            "file::crate_a/src/policy.rs::fn::helper",
            "helper",
        );

        let unresolved = vec![(
            "file::crate_a/src/walker.rs::fn::walk".to_string(),
            "ref::helper".to_string(),
            "calls".to_string(),
        )];
        let stats = ReferenceResolver::resolve(&mut graph, &unresolved).expect("resolve");
        assert_eq!(stats.resolved, 1, "the calls ref must resolve");

        let bound = calls_target(&graph, caller).expect("a calls edge from walk");
        assert_eq!(
            bound, correct,
            "calls edge must bind to the same-directory helper (crate_a/src/policy.rs), not the cross-crate one"
        );
        assert_ne!(
            bound, wrong,
            "must NOT bind to crate_b/src/other.rs::helper"
        );
    }
}
