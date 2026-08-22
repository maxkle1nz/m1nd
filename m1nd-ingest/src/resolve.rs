// === crates/m1nd-ingest/src/resolve.rs ===

use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_core::graph::Graph;
use m1nd_core::types::*;

use crate::ownership::{
    graph_has_edge, OwnedEdgeClaimV1, OwnershipDeltaV1, ResolutionDecisionV1, ResolutionOutcomeV1,
};

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

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct OwnedUnresolvedReferenceV1 {
    pub source_key: String,
    pub source_id: String,
    pub target_label: String,
    pub relation: String,
}

#[derive(Clone, Debug)]
pub struct OwnedResolutionStatsV1 {
    pub summary: ResolutionStats,
    pub ownership: OwnershipDeltaV1,
    pub decisions: Vec<ResolutionDecisionV1>,
    pub input_count: u64,
    pub hint_count: u64,
}

/// Node tag marking a source node that has at least one outgoing edge which was
/// resolved via a genuine coin-flip among same-name candidates (a guess no
/// qualifier/hint/proximity signal could decide). Provenance only — the edge IS
/// created; the tag lets `why` flag a path that rests on it. Shared so the read
/// side (`why` closure verdict) uses the same literal.
///
/// This BARE tag is node-level ("this node has SOME ambiguous outbound edge").
/// It is intentionally coarse and drives the `ResolutionStats.ambiguous` count.
/// For a PER-PATH honest verdict the resolver ALSO emits a TARGETED variant,
/// `m1nd:edge:ambiguous:<target_external_id>` (see [`ambiguous_edge_tag`]), that
/// names the specific ambiguous edge. `why` reads the targeted tag so a CLEAN
/// edge leaving a node that happens to have an unrelated ambiguous edge is NOT
/// falsely reported blocked — killing the closure cry-wolf.
pub const EDGE_AMBIGUOUS_TAG: &str = "m1nd:edge:ambiguous";

/// Build the TARGETED ambiguity tag for an edge whose binding to `target_ext_id`
/// was a genuine coin-flip: `m1nd:edge:ambiguous:<target_external_id>`. Placed on
/// the SOURCE node so the read side can tell WHICH outgoing edge was the guess
/// (the bare [`EDGE_AMBIGUOUS_TAG`] only says the node has one somewhere). Kept
/// here so the tagger and the `why` reader share one literal scheme.
pub fn ambiguous_edge_tag(target_ext_id: &str) -> String {
    format!("{EDGE_AMBIGUOUS_TAG}:{target_ext_id}")
}

/// Read side: does `source` carry the TARGETED ambiguity tag for the specific
/// edge to `target_ext_id`? True iff THAT edge was a genuine coin-flip at ingest.
/// Used by `why` to make the closure verdict edge-specific instead of blaming a
/// clean edge for an unrelated ambiguous sibling on the same source node.
pub fn source_has_ambiguous_edge_to(graph: &Graph, source: NodeId, target_ext_id: &str) -> bool {
    let needle = ambiguous_edge_tag(target_ext_id);
    graph.node_tags(source).iter().any(|t| *t == needle)
}

/// Node tag marking a source node that had at least one outgoing reference m1nd
/// could NOT resolve to any target (the edge was dropped, leaving the source's
/// outgoing picture incomplete).
pub const EDGE_UNRESOLVED_TAG: &str = "m1nd:edge:unresolved";

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
        let owned = unresolved
            .iter()
            .map(
                |(source_id, target_label, relation)| OwnedUnresolvedReferenceV1 {
                    source_key: String::new(),
                    source_id: source_id.clone(),
                    target_label: target_label.clone(),
                    relation: relation.clone(),
                },
            )
            .collect::<Vec<_>>();
        Self::resolve_owned_with_hints(graph, &owned, import_hints).map(|result| result.summary)
    }

    /// Ownership-preserving resolver used by governed code ingestion. Existing
    /// callers keep using `resolve_with_hints`; this variant additionally returns
    /// the exact edge keys produced for each extractor-supplied source key,
    /// including an already-represented shared edge.
    pub fn resolve_owned_with_hints(
        graph: &mut Graph,
        unresolved: &[OwnedUnresolvedReferenceV1],
        import_hints: &[(String, String, String)],
    ) -> M1ndResult<OwnedResolutionStatsV1> {
        let governed = unresolved
            .first()
            .is_some_and(|reference| !reference.source_key.is_empty());
        let mut input_keys = std::collections::BTreeSet::new();
        for reference in unresolved {
            if reference.source_key.is_empty() == governed
                || (governed && !crate::is_valid_relative_file_path(&reference.source_key))
                || reference.source_id.is_empty()
                || reference.source_id != reference.source_id.trim()
                || reference.target_label.is_empty()
                || reference.target_label != reference.target_label.trim()
                || reference.relation.is_empty()
                || reference.relation != reference.relation.trim()
            {
                return Err(M1ndError::InvalidParams {
                    tool: "resolve_references".into(),
                    detail: format!("invalid resolution input: {reference:?}"),
                });
            }
            if !input_keys.insert(reference.clone()) {
                return Err(M1ndError::InvalidParams {
                    tool: "resolve_references".into(),
                    detail: format!("duplicate resolution input: {reference:?}"),
                });
            }
        }

        let hint_targets = unresolved
            .iter()
            .map(|reference| {
                (
                    reference.source_id.as_str(),
                    reference.target_label.as_str(),
                )
            })
            .collect::<std::collections::BTreeSet<_>>();
        let mut hint_map = std::collections::HashMap::with_capacity(import_hints.len());
        for (source_id, target_label, import_path) in import_hints {
            if source_id.is_empty()
                || source_id != source_id.trim()
                || target_label.is_empty()
                || target_label != target_label.trim()
                || import_path.is_empty()
                || import_path != import_path.trim()
                || !hint_targets.contains(&(source_id.as_str(), target_label.as_str()))
            {
                return Err(M1ndError::InvalidParams {
                    tool: "resolve_references".into(),
                    detail: format!(
                        "invalid or orphan resolution hint: ({source_id:?}, {target_label:?}, {import_path:?})"
                    ),
                });
            }
            if hint_map
                .insert(
                    (source_id.as_str(), target_label.as_str()),
                    import_path.as_str(),
                )
                .is_some()
            {
                return Err(M1ndError::InvalidParams {
                    tool: "resolve_references".into(),
                    detail: format!(
                        "duplicate/conflicting resolution hint for ({source_id:?}, {target_label:?})"
                    ),
                });
            }
        }

        let label_index = Self::build_label_index(graph);
        let mut stats = ResolutionStats {
            resolved: 0,
            unresolved: 0,
            ambiguous: 0,
        };
        let mut ownership = OwnershipDeltaV1::default();
        let mut decisions = Vec::with_capacity(unresolved.len());
        // Memoizes the O(num_nodes) suffix-match scan below by `clean_label`. The
        // scan result depends only on the graph's (fixed, for the duration of this
        // pass) label roster and `clean_label`/`last_segment` — never on `source` —
        // so every reference that repeats the same unresolved external label (the
        // common case: many files `use`/`import` the same stdlib/third-party name)
        // reuses one scan instead of paying it again. Real corpora hit this branch
        // thousands of times with heavy label repetition, which made this the
        // dominant cost of ingesting m1nd's own ~1000-file workspace.
        let mut suffix_scan_cache: std::collections::HashMap<String, Vec<NodeId>> =
            std::collections::HashMap::new();

        for reference in unresolved {
            let source_id = &reference.source_id;
            let target_label = &reference.target_label;
            let relation = &reference.relation;
            // Look up source node
            let source = match graph.resolve_id(source_id) {
                Some(id) => id,
                None => {
                    stats.unresolved += 1;
                    decisions.push(resolution_decision(
                        graph,
                        reference,
                        ResolutionOutcomeV1::Unresolved,
                        None,
                        &[],
                    )?);
                    continue;
                }
            };

            // Strip "ref::" prefix if present
            let clean_label = target_label.strip_prefix("ref::").unwrap_or(target_label);

            // Extract the last segment from import path for matching
            // e.g., "m1nd_core::graph::Graph" -> "Graph"
            let last_segment = clean_label.rsplit("::").next().unwrap_or(clean_label);

            // Qualifier carried by a `ref::Type::method` (or `ref::a::b::method`)
            // call: the segment immediately BEFORE the last one. For
            // `TaintEngine::analyze` -> "TaintEngine"; for
            // `m1nd_core::taint::TaintEngine::analyze` -> "TaintEngine". Used to
            // pick the same-name candidate OWNED by that type/module among ties.
            // `None` for a bare `ref::name` (no `::`), so resolution is unchanged
            // for unqualified refs (back-compat).
            let qualifier = clean_label
                .strip_suffix(last_segment)
                .and_then(|head| head.strip_suffix("::"))
                .and_then(|head| head.rsplit("::").next())
                .filter(|q| !q.is_empty());

            // Check for import path hint (Task #8)
            let import_hint = hint_map
                .get(&(source_id.as_str(), target_label.as_str()))
                .copied();

            // Look up by label in the graph's string interner
            let label_interned = match graph.strings.lookup(last_segment) {
                Some(id) => id,
                None => {
                    // Try matching by suffix (e.g., "Config" matches "module::Config").
                    // Memoized by `clean_label`: the scan result is invariant across
                    // this whole pass (see `suffix_scan_cache` comment above).
                    let found = suffix_scan_cache
                        .entry(clean_label.to_string())
                        .or_insert_with(|| {
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
                            found
                        })
                        .clone();
                    if found.is_empty() {
                        stats.unresolved += 1;
                        graph.add_node_tags(source, &[EDGE_UNRESOLVED_TAG]);
                        decisions.push(resolution_decision(
                            graph,
                            reference,
                            ResolutionOutcomeV1::Unresolved,
                            None,
                            &[],
                        )?);
                        continue;
                    }
                    // Use first match (or disambiguate if multiple). A call-site
                    // qualifier (`ref::Type::method`) wins first — it pins the
                    // owner among same-name candidates; then the import hint; then
                    // proximity. `pick_candidate` also reports whether the choice
                    // was a GENUINE tie (a coin-flip) so the provenance tag fires
                    // only on real ambiguity, not merely "same name existed".
                    let (target, is_tie) = if found.len() == 1 {
                        (found[0], false)
                    } else {
                        Self::pick_candidate(graph, source, &found, qualifier, import_hint)
                    };
                    if is_tie {
                        stats.ambiguous += 1;
                        // Provenance: this source node's outgoing edge resolved via
                        // a genuine coin-flip among same-name candidates that no
                        // qualifier/hint/proximity signal could decide. The edge is
                        // still created (binding unchanged) — the tags only make the
                        // guess KNOWABLE so `why` can flag a path that rests on it.
                        // Both the bare (node-level count) and targeted (per-edge,
                        // read by `why`) tags are added.
                        Self::tag_ambiguous_edge(graph, source, target);
                    }

                    // Add edge
                    let rel = relation.as_str();
                    let owned = if graph_has_edge(
                        graph,
                        source,
                        target,
                        rel,
                        EdgeDirection::Forward,
                        false,
                    ) {
                        true
                    } else {
                        match graph.add_edge(
                            source,
                            target,
                            rel,
                            FiniteF32::new(0.5),
                            EdgeDirection::Forward,
                            false,
                            FiniteF32::new(0.4),
                        ) {
                            Ok(_) => true,
                            Err(_) => graph_has_edge(
                                graph,
                                source,
                                target,
                                rel,
                                EdgeDirection::Forward,
                                false,
                            ),
                        }
                    };
                    let target_id = Self::find_external_id(graph, target);
                    if owned && !reference.source_key.is_empty() {
                        if let Some(target_id) = target_id.clone() {
                            ownership.claim_edge(OwnedEdgeClaimV1::forward(
                                reference.source_key.clone(),
                                source_id.clone(),
                                target_id,
                                relation.clone(),
                            ));
                        }
                    }
                    decisions.push(resolution_decision(
                        graph,
                        reference,
                        if is_tie {
                            ResolutionOutcomeV1::Ambiguous
                        } else {
                            ResolutionOutcomeV1::Resolved
                        },
                        target_id,
                        &found,
                    )?);
                    stats.resolved += 1;
                    continue;
                }
            };

            // Found by exact interned match (using last segment)
            if let Some(candidates) = label_index.get(&label_interned) {
                if candidates.is_empty() {
                    stats.unresolved += 1;
                    graph.add_node_tags(source, &[EDGE_UNRESOLVED_TAG]);
                    decisions.push(resolution_decision(
                        graph,
                        reference,
                        ResolutionOutcomeV1::Unresolved,
                        None,
                        &[],
                    )?);
                    continue;
                }
                // Qualifier (`ref::Type::method`) first, then import hint, then
                // proximity — same precedence as the suffix branch above.
                // `pick_candidate` reports whether the choice was a genuine tie so
                // the provenance tag fires only on real ambiguity.
                let (target, is_tie) = if candidates.len() == 1 {
                    (candidates[0], false)
                } else {
                    Self::pick_candidate(graph, source, candidates, qualifier, import_hint)
                };
                if is_tie {
                    stats.ambiguous += 1;
                    // Provenance only — see the suffix-branch comment above. The
                    // SAME disambiguated edge is still created; the tags mark a
                    // genuine coin-flip so `why` can flag a path that rests on it.
                    Self::tag_ambiguous_edge(graph, source, target);
                }

                let rel = relation.as_str();
                let owned =
                    if graph_has_edge(graph, source, target, rel, EdgeDirection::Forward, false) {
                        true
                    } else {
                        match graph.add_edge(
                            source,
                            target,
                            rel,
                            FiniteF32::new(0.5),
                            EdgeDirection::Forward,
                            false,
                            FiniteF32::ZERO,
                        ) {
                            Ok(_) => true,
                            Err(_) => graph_has_edge(
                                graph,
                                source,
                                target,
                                rel,
                                EdgeDirection::Forward,
                                false,
                            ),
                        }
                    };
                let target_id = Self::find_external_id(graph, target);
                if owned && !reference.source_key.is_empty() {
                    if let Some(target_id) = target_id.clone() {
                        ownership.claim_edge(OwnedEdgeClaimV1::forward(
                            reference.source_key.clone(),
                            source_id.clone(),
                            target_id,
                            relation.clone(),
                        ));
                    }
                }
                decisions.push(resolution_decision(
                    graph,
                    reference,
                    if is_tie {
                        ResolutionOutcomeV1::Ambiguous
                    } else {
                        ResolutionOutcomeV1::Resolved
                    },
                    target_id,
                    candidates,
                )?);
                stats.resolved += 1;
            } else {
                stats.unresolved += 1;
                graph.add_node_tags(source, &[EDGE_UNRESOLVED_TAG]);
                decisions.push(resolution_decision(
                    graph,
                    reference,
                    ResolutionOutcomeV1::Unresolved,
                    None,
                    &[],
                )?);
            }
        }

        decisions.sort();
        if decisions.len() != unresolved.len()
            || stats.resolved + stats.unresolved != unresolved.len() as u64
            || stats.ambiguous > stats.resolved
        {
            return Err(M1ndError::IngestError(format!(
                "resolution accounting mismatch: inputs={}, decisions={}, resolved={}, unresolved={}, ambiguous={}",
                unresolved.len(),
                decisions.len(),
                stats.resolved,
                stats.unresolved,
                stats.ambiguous
            )));
        }

        Ok(OwnedResolutionStatsV1 {
            summary: stats,
            ownership,
            decisions,
            input_count: unresolved.len() as u64,
            hint_count: import_hints.len() as u64,
        })
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

    /// Record provenance for a genuinely-ambiguous edge `source -> target`: the
    /// bare node-level [`EDGE_AMBIGUOUS_TAG`] (drives the count / back-compat) AND
    /// the TARGETED `m1nd:edge:ambiguous:<target_ext_id>` tag that names THIS edge
    /// so `why` can report it per-path (not blame clean siblings). If the target
    /// has no resolvable external id (should not happen for a real bind), only the
    /// bare tag is added.
    fn tag_ambiguous_edge(graph: &mut Graph, source: NodeId, target: NodeId) {
        graph.add_node_tags(source, &[EDGE_AMBIGUOUS_TAG]);
        if let Some(target_ext_id) = Self::find_external_id(graph, target) {
            let targeted = ambiguous_edge_tag(&target_ext_id);
            graph.add_node_tags(source, &[targeted.as_str()]);
        }
    }

    /// Pick the winning candidate AND report whether the choice was a GENUINE
    /// tie (a coin-flip). Runs the same precedence as the call sites — qualifier
    /// (`ref::Type::method`) → import hint → proximity → first-match fallback —
    /// but returns `(winner, is_tie)`:
    ///   * a qualifier or import-hint match is DECISIVE → `is_tie == false`;
    ///   * proximity is decisive only when ONE candidate holds the strict best
    ///     score → `is_tie == false`;
    ///   * when nothing above decided it (≥2 candidates share the best proximity
    ///     rank, or no signal applied) the bind is a coin-flip on `candidates[0]`
    ///     → `is_tie == true`.
    ///
    /// This is what tightens the `m1nd:edge:ambiguous` provenance tag from
    /// "same-name existed" to "the resolution was actually ambiguous", killing
    /// the closure cry-wolf while still flagging real ties. Callers pass ≥2
    /// candidates; a single candidate never reaches here (handled inline).
    fn pick_candidate(
        graph: &Graph,
        source: NodeId,
        candidates: &[NodeId],
        qualifier: Option<&str>,
        import_hint: Option<&str>,
    ) -> (NodeId, bool) {
        // 1) Call-site qualifier decides the owner — decisive.
        if let Some(t) = Self::disambiguate_with_qualifier(graph, candidates, qualifier) {
            return (t, false);
        }
        // 2) Import-path hint decides the module — decisive.
        if let Some(hint) = import_hint {
            if let Some(t) = Self::disambiguate_with_hint(graph, source, candidates, hint) {
                return (t, false);
            }
        }
        // 3) Proximity: decisive only when the best score is uniquely held.
        if let Some((t, unique_best)) = Self::disambiguate_decisive(graph, source, candidates) {
            return (t, !unique_best);
        }
        // 4) No signal applied at all (e.g. source has no external id): coin-flip.
        (candidates[0], true)
    }

    /// Proximity disambiguation that also reports whether the winning score is
    /// held by a UNIQUE candidate. Returns `(best, unique_best)` where
    /// `unique_best` is true iff exactly one candidate achieves the maximum
    /// proximity score — i.e. proximity genuinely decided it. When ≥2 candidates
    /// tie for the top score (including the degenerate all-zero case where no
    /// candidate shares any prefix), `unique_best` is false and the caller treats
    /// the pick as a coin-flip. The winning node is exactly the highest-proximity
    /// candidate (`proximity_score`, same-file > same-dir > cross-crate); this
    /// only adds the strict-uniqueness (tie) signal on top of that choice.
    fn disambiguate_decisive(
        graph: &Graph,
        source: NodeId,
        candidates: &[NodeId],
    ) -> Option<(NodeId, bool)> {
        if candidates.is_empty() {
            return None;
        }
        let source_ext_id = Self::find_external_id(graph, source)?;

        // Only candidates that actually have an external id can be scored; seed
        // `best`/`best_score` on the FIRST scored candidate so an unscorable
        // `candidates[0]` never masquerades as the winner. If NONE is scorable,
        // fall back to `candidates[0]` as a non-unique (coin-flip) pick.
        let mut best: Option<NodeId> = None;
        let mut best_score = 0u32;
        let mut best_count = 0usize; // how many scored candidates hold `best_score`
        for &candidate in candidates {
            if let Some(cand_ext_id) = Self::find_external_id(graph, candidate) {
                let score = Self::proximity_score(&source_ext_id, &cand_ext_id);
                match best {
                    Some(_) if score > best_score => {
                        best_score = score;
                        best = Some(candidate);
                        best_count = 1;
                    }
                    Some(_) if score == best_score => best_count += 1,
                    Some(_) => {}
                    None => {
                        best_score = score;
                        best = Some(candidate);
                        best_count = 1;
                    }
                }
            }
        }
        match best {
            Some(b) => Some((b, best_count == 1)),
            None => Some((candidates[0], false)),
        }
    }

    /// Find the node's single external identity. Anonymous and multiply-named
    /// slots are both invalid, so neither may be collapsed to an arbitrary
    /// HashMap iteration winner.
    fn find_external_id(graph: &Graph, node: NodeId) -> Option<String> {
        let mut found = None;
        for (interned, &nid) in &graph.id_to_node {
            if nid == node {
                if found.is_some() {
                    return None;
                }
                found = Some(graph.strings.resolve(*interned).to_string());
            }
        }
        found
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

    /// Disambiguate same-name candidates using the CALL-SITE qualifier carried by
    /// a `ref::Type::method` (or `ref::module::func`). For a `Type::method(` call
    /// the strongest signal is the method's impl owner: prefer a candidate tagged
    /// `rust:impl:self:<qualifier>`. Failing that (e.g. a `module::func` path
    /// qualifier, or a non-tier1 graph with no impl tags), prefer a candidate
    /// whose external id contains the qualifier as a path/segment component. Only
    /// the LAST segment of the qualifier is matched (`a::b::Type` -> `Type`), which
    /// is what the extractor emits. Returns `None` when no candidate matches, so
    /// the caller falls back to import-hint / proximity (back-compat).
    fn disambiguate_with_qualifier(
        graph: &Graph,
        candidates: &[NodeId],
        qualifier: Option<&str>,
    ) -> Option<NodeId> {
        let qualifier = qualifier?;
        if candidates.is_empty() {
            return None;
        }

        // 1) Impl-owner match: `rust:impl:self:<qualifier>` on the candidate.
        let owner_tag = format!("rust:impl:self:{qualifier}");
        let owner_match: Vec<NodeId> = candidates
            .iter()
            .copied()
            .filter(|&c| graph.node_tags(c).iter().any(|t| *t == owner_tag))
            .collect();
        if owner_match.len() == 1 {
            return Some(owner_match[0]);
        }
        // Ambiguous owner (same type name in two files): fall through to the
        // id/path match below, which can still break the tie by module path.

        // 2) External-id / module-path match: the qualifier appears as a path
        // component of the candidate's id (covers `module::func` and disambiguates
        // a multi-file impl-owner tie by directory). Match on `::<q>` or `/<q>` /
        // `<q>/` boundaries so `taint` matches `…/taint.rs::…` but not `restaint`.
        let pool: &[NodeId] = if owner_match.len() > 1 {
            &owner_match
        } else {
            candidates
        };
        let mut id_match: Option<NodeId> = None;
        for &c in pool {
            if let Some(id) = Self::find_external_id(graph, c) {
                let hit = id.contains(&format!("::{qualifier}"))
                    || id.contains(&format!("/{qualifier}/"))
                    || id.contains(&format!("/{qualifier}."))
                    || id.contains(&format!("::{qualifier}::"));
                if hit {
                    if id_match.is_some() {
                        return None; // ambiguous — let proximity decide
                    }
                    id_match = Some(c);
                }
            }
        }
        id_match
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

fn resolution_decision(
    graph: &Graph,
    reference: &OwnedUnresolvedReferenceV1,
    outcome: ResolutionOutcomeV1,
    resolved_target_id: Option<String>,
    candidates: &[NodeId],
) -> M1ndResult<ResolutionDecisionV1> {
    let mut candidate_ids = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let candidate_id =
            ReferenceResolver::find_external_id(graph, *candidate).ok_or_else(|| {
                M1ndError::IngestError(format!(
                    "resolution candidate slot {} has no external identity",
                    candidate.as_usize()
                ))
            })?;
        candidate_ids.push(candidate_id);
    }
    candidate_ids.sort();
    if candidate_ids.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(M1ndError::IngestError(format!(
            "resolution candidates are not bijective for source {:?}, target {:?}",
            reference.source_id, reference.target_label
        )));
    }
    let provenance = graph
        .resolve_id(&reference.source_id)
        .map(|source| graph.resolve_node_provenance(source));
    Ok(ResolutionDecisionV1 {
        source_key: reference.source_key.clone(),
        source_id: reference.source_id.clone(),
        target_label: reference.target_label.clone(),
        relation: reference.relation.clone(),
        outcome,
        resolved_target_id,
        candidate_ids,
        source_line_start: provenance.as_ref().and_then(|value| value.line_start),
        source_line_end: provenance.and_then(|value| value.line_end),
    })
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

    /// A `Type::method()` call carries the qualifier in the ref string
    /// (`ref::TaintEngine::analyze`). Among same-name `analyze` candidates the
    /// resolver must bind to the one OWNED by `TaintEngine` (tagged
    /// `rust:impl:self:TaintEngine`), not to a same-name decoy — even when the
    /// decoy is inserted FIRST (so a `candidates[0]` tie-break would pick it) and
    /// sits closer to the caller by proximity (same crate). This is the qualified
    /// same-name CALL resolution the qualifier path adds; bare `ref::analyze`
    /// (no qualifier) is unaffected.
    #[test]
    fn qualified_call_binds_impl_owner_over_proximity_and_order() {
        let mut graph = Graph::new();
        let caller = fn_node(&mut graph, "file::crate_b/src/api.rs::fn::handle", "handle");
        // Decoy: same name, inserted FIRST, SAME crate as the caller (proximity
        // would prefer it). NOT owned by TaintEngine.
        let decoy = graph
            .add_node(
                "file::crate_b/src/tremor.rs::fn::analyze",
                "analyze",
                NodeType::Function,
                &["rust:impl:self:TremorEngine"],
                0.0,
                0.0,
            )
            .expect("add decoy");
        // Correct: owned by TaintEngine, in a DIFFERENT crate (proximity-disfavored).
        let correct = graph
            .add_node(
                "file::crate_a/src/taint.rs::fn::analyze",
                "analyze",
                NodeType::Function,
                &["rust:impl:self:TaintEngine"],
                0.0,
                0.0,
            )
            .expect("add correct");

        let unresolved = vec![(
            "file::crate_b/src/api.rs::fn::handle".to_string(),
            "ref::TaintEngine::analyze".to_string(),
            "calls".to_string(),
        )];
        let stats = ReferenceResolver::resolve(&mut graph, &unresolved).expect("resolve");
        assert_eq!(stats.resolved, 1, "the qualified calls ref must resolve");

        let bound = calls_target(&graph, caller).expect("a calls edge from handle");
        assert_eq!(
            bound, correct,
            "qualified `TaintEngine::analyze` must bind to the TaintEngine-owned analyze"
        );
        assert_ne!(
            bound, decoy,
            "must NOT bind to the same-name TremorEngine decoy"
        );
    }

    /// Provenance (GENUINE TIE): an ambiguous `ref::` whose same-name candidates
    /// are a real coin-flip — two `helper`s in the SAME directory, so proximity
    /// scores them identically, with NO qualifier to decide — must (a) still
    /// create the SAME edge (binding unchanged) and (b) tag the SOURCE node
    /// `m1nd:edge:ambiguous` while incrementing `ResolutionStats.ambiguous`, so
    /// the guess is knowable. This is the honesty guard for the cry-wolf fix: a
    /// true tie MUST still flag (and yield `blocked` on paths crossing it).
    #[test]
    fn genuine_tie_tags_source_and_counts_ambiguous() {
        let mut graph = Graph::new();
        let caller = fn_node(&mut graph, "file::crate_a/src/walker.rs::fn::walk", "walk");
        // Two same-name targets in the SAME directory -> identical proximity, no
        // qualifier -> genuine coin-flip.
        let _a = fn_node(&mut graph, "file::crate_a/src/one.rs::fn::helper", "helper");
        let _b = fn_node(&mut graph, "file::crate_a/src/two.rs::fn::helper", "helper");

        let unresolved = vec![(
            "file::crate_a/src/walker.rs::fn::walk".to_string(),
            "ref::helper".to_string(),
            "calls".to_string(),
        )];
        let stats = ReferenceResolver::resolve(&mut graph, &unresolved).expect("resolve");

        // Binding unchanged: an edge IS still created.
        assert_eq!(stats.resolved, 1, "the ambiguous ref must still resolve");
        assert!(
            calls_target(&graph, caller).is_some(),
            "the same edge must still be created (binding unchanged)"
        );
        // Provenance recorded — a genuine tie still flags.
        assert_eq!(
            stats.ambiguous, 1,
            "ambiguous count must increment on a true tie"
        );
        assert!(
            graph.node_tags(caller).contains(&EDGE_AMBIGUOUS_TAG),
            "source must carry the bare ambiguous provenance tag on a true tie, got {:?}",
            graph.node_tags(caller)
        );
        // And the TARGETED tag names the specific edge that was picked, so `why`
        // can flag exactly this edge per-path.
        let picked = calls_target(&graph, caller).expect("edge exists");
        let picked_ext = ReferenceResolver::find_external_id(&graph, picked).expect("ext id");
        assert!(
            source_has_ambiguous_edge_to(&graph, caller, &picked_ext),
            "source must carry the TARGETED ambiguous tag for the picked edge, got {:?}",
            graph.node_tags(caller)
        );
    }

    /// Cry-wolf killer: a source with ONE genuinely-ambiguous outbound edge must
    /// NOT taint a DIFFERENT, cleanly-resolved outbound edge. This is the exact
    /// failure the field report describes — a clean path (e.g. handle_seek ->
    /// pack_to_budget) reported `blocked` only because the source also calls some
    /// common-named fn. The targeted tag is per-edge, so `source_has_ambiguous_
    /// edge_to` is TRUE for the tied target and FALSE for the clean (unique) one.
    #[test]
    fn one_ambiguous_edge_does_not_taint_a_clean_sibling_edge() {
        let mut graph = Graph::new();
        let caller = fn_node(
            &mut graph,
            "file::crate_a/src/handler.rs::fn::handle",
            "handle",
        );
        // A genuinely-ambiguous target: two same-name `get` in the same dir.
        let _g1 = fn_node(&mut graph, "file::crate_a/src/one.rs::fn::get", "get");
        let _g2 = fn_node(&mut graph, "file::crate_a/src/two.rs::fn::get", "get");
        // A cleanly-resolvable UNIQUE target.
        let unique = fn_node(
            &mut graph,
            "file::crate_a/src/pack.rs::fn::pack_to_budget",
            "pack_to_budget",
        );

        let unresolved = vec![
            (
                "file::crate_a/src/handler.rs::fn::handle".to_string(),
                "ref::get".to_string(),
                "calls".to_string(),
            ),
            (
                "file::crate_a/src/handler.rs::fn::handle".to_string(),
                "ref::pack_to_budget".to_string(),
                "calls".to_string(),
            ),
        ];
        let stats = ReferenceResolver::resolve(&mut graph, &unresolved).expect("resolve");
        assert_eq!(stats.resolved, 2, "both refs resolve");
        assert_eq!(stats.ambiguous, 1, "only the `get` tie is ambiguous");

        // The clean unique edge must NOT be reported ambiguous …
        let unique_ext = ReferenceResolver::find_external_id(&graph, unique).expect("ext id");
        assert!(
            !source_has_ambiguous_edge_to(&graph, caller, &unique_ext),
            "the clean pack_to_budget edge must NOT carry a targeted ambiguous tag"
        );
        // … even though the SAME source node is (bare) tagged because of `get`.
        assert!(
            graph.node_tags(caller).contains(&EDGE_AMBIGUOUS_TAG),
            "the source is still bare-tagged due to its ambiguous `get` edge"
        );
    }

    /// Cry-wolf fix (a): same-name candidates in DIFFERENT directories where
    /// proximity picks a UNIQUE same-directory winner is a CONFIDENT bind, not a
    /// coin-flip — it must NOT tag `m1nd:edge:ambiguous` nor increment the count,
    /// even though a same-name candidate existed elsewhere. (Pre-fix this tagged
    /// ambiguous purely because `found.len() > 1`, which is the cry-wolf.) The
    /// edge still binds to the closer candidate.
    #[test]
    fn decisive_by_proximity_does_not_tag_ambiguous() {
        let mut graph = Graph::new();
        let caller = fn_node(&mut graph, "file::crate_a/src/walker.rs::fn::walk", "walk");
        // Same-directory winner (shares crate_a/src prefix) …
        let correct = fn_node(
            &mut graph,
            "file::crate_a/src/policy.rs::fn::helper",
            "helper",
        );
        // … vs a cross-crate same-name candidate (proximity-disfavored).
        let _far = fn_node(
            &mut graph,
            "file::crate_b/src/other.rs::fn::helper",
            "helper",
        );

        let unresolved = vec![(
            "file::crate_a/src/walker.rs::fn::walk".to_string(),
            "ref::helper".to_string(),
            "calls".to_string(),
        )];
        let stats = ReferenceResolver::resolve(&mut graph, &unresolved).expect("resolve");

        assert_eq!(stats.resolved, 1, "the ref must resolve");
        assert_eq!(
            calls_target(&graph, caller),
            Some(correct),
            "must bind to the same-directory helper"
        );
        assert_eq!(
            stats.ambiguous, 0,
            "a unique proximity winner is decisive — no ambiguous count"
        );
        assert!(
            !graph.node_tags(caller).contains(&EDGE_AMBIGUOUS_TAG),
            "decisive proximity bind must NOT carry the ambiguous tag, got {:?}",
            graph.node_tags(caller)
        );
    }

    /// Cry-wolf fix (b): a `Type::method()` call whose qualifier resolves the
    /// owner among same-name candidates is a CONFIDENT bind — it must NOT tag
    /// `m1nd:edge:ambiguous`, even with two same-name `analyze` candidates. The
    /// qualifier decided it; that is not a coin-flip.
    #[test]
    fn decisive_by_qualifier_does_not_tag_ambiguous() {
        let mut graph = Graph::new();
        let caller = fn_node(&mut graph, "file::crate_b/src/api.rs::fn::handle", "handle");
        // Owned by TaintEngine (the qualifier target) …
        let correct = graph
            .add_node(
                "file::crate_a/src/taint.rs::fn::analyze",
                "analyze",
                NodeType::Function,
                &["rust:impl:self:TaintEngine"],
                0.0,
                0.0,
            )
            .expect("add correct");
        // … vs a same-name decoy owned by a different type.
        let _decoy = graph
            .add_node(
                "file::crate_b/src/tremor.rs::fn::analyze",
                "analyze",
                NodeType::Function,
                &["rust:impl:self:TremorEngine"],
                0.0,
                0.0,
            )
            .expect("add decoy");

        let unresolved = vec![(
            "file::crate_b/src/api.rs::fn::handle".to_string(),
            "ref::TaintEngine::analyze".to_string(),
            "calls".to_string(),
        )];
        let stats = ReferenceResolver::resolve(&mut graph, &unresolved).expect("resolve");

        assert_eq!(stats.resolved, 1, "the qualified ref must resolve");
        assert_eq!(
            calls_target(&graph, caller),
            Some(correct),
            "must bind to the TaintEngine-owned analyze"
        );
        assert_eq!(
            stats.ambiguous, 0,
            "a qualifier-resolved bind is decisive — no ambiguous count"
        );
        assert!(
            !graph.node_tags(caller).contains(&EDGE_AMBIGUOUS_TAG),
            "qualifier-decided bind must NOT carry the ambiguous tag, got {:?}",
            graph.node_tags(caller)
        );
    }

    /// Provenance regression: a `ref::` that resolves to NOTHING drops the edge
    /// (binding unchanged) AND tags the source `m1nd:edge:unresolved` while
    /// incrementing `ResolutionStats.unresolved`.
    #[test]
    fn unresolved_ref_tags_source_and_counts_unresolved() {
        let mut graph = Graph::new();
        let caller = fn_node(&mut graph, "file::crate_a/src/walker.rs::fn::walk", "walk");

        let unresolved = vec![(
            "file::crate_a/src/walker.rs::fn::walk".to_string(),
            "ref::DoesNotExistAnywhere".to_string(),
            "calls".to_string(),
        )];
        let stats = ReferenceResolver::resolve(&mut graph, &unresolved).expect("resolve");

        assert_eq!(stats.resolved, 0, "an unresolvable ref must not resolve");
        assert_eq!(stats.unresolved, 1, "unresolved count must increment");
        assert!(
            calls_target(&graph, caller).is_none(),
            "no edge is created for an unresolvable ref (binding unchanged)"
        );
        assert!(
            graph.node_tags(caller).contains(&EDGE_UNRESOLVED_TAG),
            "source must carry the unresolved provenance tag, got {:?}",
            graph.node_tags(caller)
        );
    }

    /// Back-compat guard: a BARE `ref::analyze` (no qualifier) must still resolve
    /// via proximity exactly as before — the qualifier path is skipped when there
    /// is no `::` in the ref, so it cannot disturb unqualified resolution.
    #[test]
    fn bare_call_still_resolves_by_proximity() {
        let mut graph = Graph::new();
        let caller = fn_node(&mut graph, "file::crate_a/src/api.rs::fn::handle", "handle");
        let far = fn_node(&mut graph, "file::crate_b/src/other.rs::fn::run", "run");
        let near = fn_node(&mut graph, "file::crate_a/src/api.rs::fn::run", "run");

        let unresolved = vec![(
            "file::crate_a/src/api.rs::fn::handle".to_string(),
            "ref::run".to_string(),
            "calls".to_string(),
        )];
        ReferenceResolver::resolve(&mut graph, &unresolved).expect("resolve");
        let bound = calls_target(&graph, caller).expect("a calls edge from handle");
        assert_eq!(
            bound, near,
            "bare ref must bind to the same-file `run` by proximity"
        );
        assert_ne!(bound, far, "bare ref must not bind cross-crate");
    }

    fn governed_reference(source_id: &str, target_label: &str) -> OwnedUnresolvedReferenceV1 {
        OwnedUnresolvedReferenceV1 {
            source_key: "src/caller.rs".into(),
            source_id: source_id.into(),
            target_label: target_label.into(),
            relation: "calls".into(),
        }
    }

    #[test]
    fn duplicate_resolution_inputs_are_rejected_not_collapsed() {
        let mut graph = Graph::new();
        let source_id = "file::src/caller.rs::fn::caller";
        fn_node(&mut graph, source_id, "caller");
        let reference = governed_reference(source_id, "ref::missing");

        let error = ReferenceResolver::resolve_owned_with_hints(
            &mut graph,
            &[reference.clone(), reference],
            &[],
        )
        .expect_err("duplicate inputs must fail closed");
        assert!(error.to_string().contains("duplicate resolution input"));
    }

    #[test]
    fn orphan_and_duplicate_resolution_hints_are_rejected() {
        let mut graph = Graph::new();
        let source_id = "file::src/caller.rs::fn::caller";
        fn_node(&mut graph, source_id, "caller");
        let reference = governed_reference(source_id, "ref::target");

        let orphan = ReferenceResolver::resolve_owned_with_hints(
            &mut graph,
            std::slice::from_ref(&reference),
            &[(source_id.into(), "ref::other".into(), "crate::other".into())],
        )
        .expect_err("orphan hint must fail closed");
        assert!(orphan.to_string().contains("orphan resolution hint"));

        let duplicate = ReferenceResolver::resolve_owned_with_hints(
            &mut graph,
            std::slice::from_ref(&reference),
            &[
                (source_id.into(), "ref::target".into(), "crate::one".into()),
                (source_id.into(), "ref::target".into(), "crate::two".into()),
            ],
        )
        .expect_err("duplicate/conflicting hint must fail closed");
        assert!(duplicate
            .to_string()
            .contains("duplicate/conflicting resolution hint"));
    }

    #[test]
    fn every_resolution_input_has_one_decision_and_exact_counts() {
        let mut graph = Graph::new();
        let source_id = "file::src/caller.rs::fn::caller";
        fn_node(&mut graph, source_id, "caller");
        fn_node(&mut graph, "file::src/target.rs::fn::target", "target");
        let inputs = vec![
            governed_reference(source_id, "ref::target"),
            governed_reference(source_id, "ref::missing"),
        ];

        let result = ReferenceResolver::resolve_owned_with_hints(&mut graph, &inputs, &[])
            .expect("fully accounted resolution");
        assert_eq!(result.input_count, 2);
        assert_eq!(result.hint_count, 0);
        assert_eq!(result.decisions.len(), 2);
        assert_eq!(result.summary.resolved, 1);
        assert_eq!(result.summary.unresolved, 1);
        assert_eq!(result.summary.ambiguous, 0);
    }

    #[test]
    fn candidate_without_external_identity_is_fatal() {
        let mut graph = Graph::new();
        let source_id = "file::src/caller.rs::fn::caller";
        fn_node(&mut graph, source_id, "caller");
        let target_id = "file::src/target.rs::fn::target";
        fn_node(&mut graph, target_id, "target");
        let target_interned = graph.strings.lookup(target_id).expect("interned target id");
        graph.id_to_node.remove(&target_interned);

        let error = ReferenceResolver::resolve_owned_with_hints(
            &mut graph,
            &[governed_reference(source_id, "ref::target")],
            &[],
        )
        .expect_err("anonymous candidate slot must fail closed");
        assert!(error.to_string().contains("resolution candidate slot"));
    }
}
