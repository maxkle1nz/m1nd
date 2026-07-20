use crate::merge::{ClaimedEdgeKey, SourceClaims};
use m1nd_core::graph::Graph;
use m1nd_core::types::{EdgeDirection, EdgeIdx, NodeId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub const CODE_INGEST_BUNDLE_SCHEMA: &str = "m1nd-code-ingest-bundle-v1";
pub const CODE_OWNERSHIP_MANIFEST_SCHEMA: &str = "m1nd-code-ownership-manifest-v1";
pub const CODE_OWNERSHIP_DIGEST_DOMAIN: &str = "m1nd-code-ownership-v1";
pub const CODE_LINEAGE_DIGEST_DOMAIN: &str = "m1nd-code-ingest-lineage-v1";
pub const CODE_SOURCE_PROJECTION_DIGEST_DOMAIN: &str = "m1nd-code-source-projection-v1";
pub const LIVE_GRAPH_CONTENT_DIGEST_DOMAIN: &str = "m1nd-live-graph-content-v1";
pub const CODE_RESOLUTION_DIGEST_DOMAIN: &str = "m1nd-code-resolution-decisions-v1";
pub const CODE_RESOLUTION_INPUT_DIGEST_DOMAIN: &str = "m1nd-code-resolution-inputs-v1";
pub const CODE_RESOLUTION_HINT_DIGEST_DOMAIN: &str = "m1nd-code-resolution-hints-v1";
pub const CODE_PIPELINE_RECEIPT_SCHEMA: &str = "m1nd-code-pipeline-receipt-v1";
pub const CODE_PIPELINE_DIGEST_DOMAIN: &str = "m1nd-code-pipeline-receipt-v1";
pub const CODE_PIPELINE_PRODUCER_NAME: &str = "m1nd-ingest";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodePipelineReceiptV1 {
    pub schema: String,
    pub pipeline_version: String,
    pub producer_name: String,
    pub producer_version: String,
    pub producer_build_identity: String,
    /// SHA-256 of the exact running executable that produced the receipt. The
    /// source/config identity above catches semantic source drift; this field
    /// additionally binds dependency/compiler/linker output as executed.
    pub producer_executable_identity: String,
    pub skip_dirs: Vec<String>,
    pub skip_files: Vec<String>,
    pub include_dotfiles: bool,
    pub dotfile_patterns: Vec<String>,
    pub policy_fingerprint: String,
    pub build_features: Vec<String>,
    pub binary_policy: String,
    pub vcs_context_digest: String,
    pub immutable_source_snapshot: bool,
    pub discovered_source_count: u64,
    pub extracted_source_count: u64,
    pub digested_source_count: u64,
    /// Global enrichment is deliberately disabled for exact-file replacement.
    /// Persisting the mode prevents a zeroed cross-file receipt from being
    /// mistaken for a completed full-root scan.
    pub global_enrichment_enabled: bool,
    pub cross_file_source_files_expected: u64,
    pub cross_file_source_metadata_verified: u64,
    pub cross_file_source_files_read: u64,
    pub cross_file_source_files_parsed: u64,
    pub cargo_workspace_members_expected: u64,
    pub cargo_workspace_members_accounted: u64,
    pub cargo_dependency_inputs_expected: u64,
    pub cargo_dependency_inputs_accounted: u64,
    pub cargo_package_file_links_expected: u64,
    pub cargo_package_file_links_accounted: u64,
}

impl CodePipelineReceiptV1 {
    fn valid_for_sources(&self, source_digests: &BTreeMap<String, String>) -> bool {
        let source_count = source_digests.len();
        let expected_cross_file_sources = source_digests
            .keys()
            .filter(|source| crate::cross_file::is_scanned_source_path(source))
            .count() as u64;
        let cross_file_counts_equal = self.cross_file_source_files_expected
            == self.cross_file_source_metadata_verified
            && self.cross_file_source_files_expected == self.cross_file_source_files_read
            && self.cross_file_source_files_expected == self.cross_file_source_files_parsed;
        let cross_file_mode_valid = if self.global_enrichment_enabled {
            self.cross_file_source_files_expected == expected_cross_file_sources
                && cross_file_counts_equal
        } else {
            self.cross_file_source_files_expected == 0
                && self.cross_file_source_metadata_verified == 0
                && self.cross_file_source_files_read == 0
                && self.cross_file_source_files_parsed == 0
        };
        let cargo_counts_equal = self.cargo_workspace_members_expected
            == self.cargo_workspace_members_accounted
            && self.cargo_dependency_inputs_expected == self.cargo_dependency_inputs_accounted
            && self.cargo_package_file_links_expected == self.cargo_package_file_links_accounted;
        let cargo_mode_valid = if self.global_enrichment_enabled {
            cargo_counts_equal
        } else {
            self.cargo_workspace_members_expected == 0
                && self.cargo_workspace_members_accounted == 0
                && self.cargo_dependency_inputs_expected == 0
                && self.cargo_dependency_inputs_accounted == 0
                && self.cargo_package_file_links_expected == 0
                && self.cargo_package_file_links_accounted == 0
        };

        self.schema == CODE_PIPELINE_RECEIPT_SCHEMA
            && !self.pipeline_version.trim().is_empty()
            && self.producer_name == CODE_PIPELINE_PRODUCER_NAME
            && self.producer_version == env!("CARGO_PKG_VERSION")
            && self.producer_build_identity == compiled_producer_build_identity()
            && running_producer_executable_identity()
                .is_ok_and(|identity| identity == self.producer_executable_identity)
            && self.policy_fingerprint.len() == 64
            && !self.binary_policy.trim().is_empty()
            && self.vcs_context_digest.len() == 64
            && self.immutable_source_snapshot
            && self.discovered_source_count == source_count as u64
            && self.extracted_source_count == source_count as u64
            && self.digested_source_count == source_count as u64
            && cross_file_mode_valid
            && cargo_mode_valid
    }

    fn missing() -> Self {
        Self {
            schema: CODE_PIPELINE_RECEIPT_SCHEMA.into(),
            pipeline_version: String::new(),
            producer_name: String::new(),
            producer_version: String::new(),
            producer_build_identity: String::new(),
            producer_executable_identity: String::new(),
            skip_dirs: Vec::new(),
            skip_files: Vec::new(),
            include_dotfiles: false,
            dotfile_patterns: Vec::new(),
            policy_fingerprint: String::new(),
            build_features: Vec::new(),
            binary_policy: "nul-in-first-8192-v1".into(),
            vcs_context_digest: sha256_bytes(b"ownership-unit-vcs-v1"),
            immutable_source_snapshot: false,
            discovered_source_count: 0,
            extracted_source_count: 0,
            digested_source_count: 0,
            global_enrichment_enabled: false,
            cross_file_source_files_expected: 0,
            cross_file_source_metadata_verified: 0,
            cross_file_source_files_read: 0,
            cross_file_source_files_parsed: 0,
            cargo_workspace_members_expected: 0,
            cargo_workspace_members_accounted: 0,
            cargo_dependency_inputs_expected: 0,
            cargo_dependency_inputs_accounted: 0,
            cargo_package_file_links_expected: 0,
            cargo_package_file_links_accounted: 0,
        }
    }

    #[cfg(test)]
    fn test_default(source_count: usize) -> Self {
        Self {
            pipeline_version: "ownership-unit-v1".into(),
            producer_name: CODE_PIPELINE_PRODUCER_NAME.into(),
            producer_version: env!("CARGO_PKG_VERSION").into(),
            producer_build_identity: compiled_producer_build_identity(),
            producer_executable_identity: running_producer_executable_identity()
                .expect("test executable identity"),
            policy_fingerprint: sha256_bytes(b"ownership-unit-policy-v1"),
            immutable_source_snapshot: true,
            discovered_source_count: source_count as u64,
            extracted_source_count: source_count as u64,
            digested_source_count: source_count as u64,
            ..Self::missing()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OwnershipCoverageV1 {
    Complete,
    Incomplete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResolutionOutcomeV1 {
    Resolved,
    Unresolved,
    Ambiguous,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionDecisionV1 {
    pub source_key: String,
    pub source_id: String,
    pub target_label: String,
    pub relation: String,
    pub outcome: ResolutionOutcomeV1,
    pub resolved_target_id: Option<String>,
    pub candidate_ids: Vec<String>,
    pub source_line_start: Option<u32>,
    pub source_line_end: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionInputV1 {
    pub source_key: String,
    pub source_id: String,
    pub target_label: String,
    pub relation: String,
}

impl ResolutionInputV1 {
    pub fn from_decision(decision: &ResolutionDecisionV1) -> Self {
        Self {
            source_key: decision.source_key.clone(),
            source_id: decision.source_id.clone(),
            target_label: decision.target_label.clone(),
            relation: decision.relation.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResolutionHintV1 {
    pub source_id: String,
    pub target_label: String,
    pub import_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnedEdgeClaimV1 {
    pub source_key: String,
    pub source: String,
    pub target: String,
    pub relation: String,
    pub direction: u8,
    pub inhibitory: bool,
}

impl OwnedEdgeClaimV1 {
    pub fn forward(
        source_key: impl Into<String>,
        source: impl Into<String>,
        target: impl Into<String>,
        relation: impl Into<String>,
    ) -> Self {
        Self {
            source_key: source_key.into(),
            source: source.into(),
            target: target.into(),
            relation: relation.into(),
            direction: 0,
            inhibitory: false,
        }
    }

    pub fn claimed_key(&self) -> ClaimedEdgeKey {
        canonical_claimed_edge(
            &self.source,
            &self.target,
            &self.relation,
            self.direction,
            self.inhibitory,
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnershipDeltaV1 {
    pub nodes: Vec<(String, String)>,
    pub edges: Vec<OwnedEdgeClaimV1>,
}

impl OwnershipDeltaV1 {
    pub fn claim_node(&mut self, source_key: impl Into<String>, external_id: impl Into<String>) {
        self.nodes.push((source_key.into(), external_id.into()));
    }

    pub fn claim_edge(&mut self, edge: OwnedEdgeClaimV1) {
        self.edges.push(edge);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeOwnershipManifestV1 {
    pub schema: String,
    pub root_identity: String,
    pub exact_source_key: Option<String>,
    /// Ownership receipt of the base graph used for contextual exact-file
    /// resolution. None for a full-root baseline ingest.
    pub base_ownership_digest: Option<String>,
    pub source_digests: BTreeMap<String, String>,
    pub claims_by_source: BTreeMap<String, SourceClaims>,
    /// Canonical digest of the finalized graph projection, including node
    /// attributes/provenance and edge attributes/weights. Topology alone is
    /// insufficient to bind a replace-safe ownership receipt.
    pub source_projection_digest: String,
    pub graph_finalized: bool,
    pub pending_edge_count: u64,
    pub bidirectional_mirrors_valid: bool,
    pub csr_shape_valid: bool,
    pub reverse_csr_valid: bool,
    /// Every node slot must have exactly one valid external identity. These
    /// lists make storage corruption visible instead of letting projection
    /// helpers silently skip anonymous or multiply-named slots.
    pub orphan_node_slots: Vec<u32>,
    pub multiply_identified_node_slots: Vec<u32>,
    pub invalid_identity_ids: Vec<String>,
    pub out_of_range_identity_ids: Vec<String>,
    /// CSR edge slots whose source or target lacks an exact node identity.
    pub orphan_edge_slots: Vec<u64>,
    pub resolution_inputs: Vec<ResolutionInputV1>,
    pub resolution_input_digest: String,
    pub resolution_hints: Vec<ResolutionHintV1>,
    pub resolution_hint_digest: String,
    pub resolution_decisions: Vec<ResolutionDecisionV1>,
    pub resolution_digest: String,
    pub pipeline_receipt: CodePipelineReceiptV1,
    pub pipeline_digest: String,
    pub coverage: OwnershipCoverageV1,
    pub unowned_nodes: Vec<String>,
    pub unowned_edges: Vec<ClaimedEdgeKey>,
    pub dangling_node_claims: Vec<String>,
    pub dangling_edge_claims: Vec<ClaimedEdgeKey>,
    /// Repeated canonical keys would make a set-valued claim cover multiple
    /// graph edge instances, so they block a bijective COMPLETE verdict.
    pub duplicate_graph_edges: Vec<ClaimedEdgeKey>,
    pub lineage_digest: String,
    pub ownership_digest: String,
}

impl CodeOwnershipManifestV1 {
    fn resolution_decisions_valid(&self) -> bool {
        resolution_decisions_valid_for(
            &self.resolution_inputs,
            &self.resolution_hints,
            &self.resolution_decisions,
            &self.source_digests,
            &self.claims_by_source,
        )
    }

    fn compute_ownership_digest(&self) -> Result<String, serde_json::Error> {
        #[derive(Serialize)]
        struct DigestView<'a> {
            domain: &'static str,
            root_identity: &'a str,
            exact_source_key: &'a Option<String>,
            base_ownership_digest: &'a Option<String>,
            source_digests: &'a BTreeMap<String, String>,
            claims_by_source: &'a BTreeMap<String, SourceClaims>,
            source_projection_digest: &'a str,
            graph_finalized: bool,
            pending_edge_count: u64,
            bidirectional_mirrors_valid: bool,
            csr_shape_valid: bool,
            reverse_csr_valid: bool,
            orphan_node_slots: &'a [u32],
            multiply_identified_node_slots: &'a [u32],
            invalid_identity_ids: &'a [String],
            out_of_range_identity_ids: &'a [String],
            orphan_edge_slots: &'a [u64],
            resolution_inputs: &'a [ResolutionInputV1],
            resolution_input_digest: &'a str,
            resolution_hints: &'a [ResolutionHintV1],
            resolution_hint_digest: &'a str,
            resolution_decisions: &'a [ResolutionDecisionV1],
            resolution_digest: &'a str,
            pipeline_receipt: &'a CodePipelineReceiptV1,
            pipeline_digest: &'a str,
            coverage: OwnershipCoverageV1,
            unowned_nodes: &'a [String],
            unowned_edges: &'a [ClaimedEdgeKey],
            dangling_node_claims: &'a [String],
            dangling_edge_claims: &'a [ClaimedEdgeKey],
            duplicate_graph_edges: &'a [ClaimedEdgeKey],
        }
        digest_json(&DigestView {
            domain: CODE_OWNERSHIP_DIGEST_DOMAIN,
            root_identity: &self.root_identity,
            exact_source_key: &self.exact_source_key,
            base_ownership_digest: &self.base_ownership_digest,
            source_digests: &self.source_digests,
            claims_by_source: &self.claims_by_source,
            source_projection_digest: &self.source_projection_digest,
            graph_finalized: self.graph_finalized,
            pending_edge_count: self.pending_edge_count,
            bidirectional_mirrors_valid: self.bidirectional_mirrors_valid,
            csr_shape_valid: self.csr_shape_valid,
            reverse_csr_valid: self.reverse_csr_valid,
            orphan_node_slots: &self.orphan_node_slots,
            multiply_identified_node_slots: &self.multiply_identified_node_slots,
            invalid_identity_ids: &self.invalid_identity_ids,
            out_of_range_identity_ids: &self.out_of_range_identity_ids,
            orphan_edge_slots: &self.orphan_edge_slots,
            resolution_inputs: &self.resolution_inputs,
            resolution_input_digest: &self.resolution_input_digest,
            resolution_hints: &self.resolution_hints,
            resolution_hint_digest: &self.resolution_hint_digest,
            resolution_decisions: &self.resolution_decisions,
            resolution_digest: &self.resolution_digest,
            pipeline_receipt: &self.pipeline_receipt,
            pipeline_digest: &self.pipeline_digest,
            coverage: self.coverage,
            unowned_nodes: &self.unowned_nodes,
            unowned_edges: &self.unowned_edges,
            dangling_node_claims: &self.dangling_node_claims,
            dangling_edge_claims: &self.dangling_edge_claims,
            duplicate_graph_edges: &self.duplicate_graph_edges,
        })
    }

    /// Cryptographically re-evaluate both receipts before trusting a persisted
    /// manifest as incremental-replacement context.
    pub fn verify_receipt(&self) -> Result<bool, serde_json::Error> {
        if self.schema != CODE_OWNERSHIP_MANIFEST_SCHEMA
            || self.root_identity.trim().is_empty()
            || self.exact_source_key.is_some() != self.base_ownership_digest.is_some()
            || !self.graph_finalized
            || self.pending_edge_count != 0
            || !self.bidirectional_mirrors_valid
            || !self.csr_shape_valid
            || !self.reverse_csr_valid
            || !self.orphan_node_slots.is_empty()
            || !self.multiply_identified_node_slots.is_empty()
            || !self.invalid_identity_ids.is_empty()
            || !self.out_of_range_identity_ids.is_empty()
            || !self.orphan_edge_slots.is_empty()
            || !self
                .pipeline_receipt
                .valid_for_sources(&self.source_digests)
        {
            return Ok(false);
        }

        let digest_sources = self.source_digests.keys().collect::<BTreeSet<_>>();
        let claim_sources = self.claims_by_source.keys().collect::<BTreeSet<_>>();
        if digest_sources != claim_sources
            || self
                .source_digests
                .keys()
                .any(|source| !crate::is_valid_relative_file_path(source))
            || self.claims_by_source.iter().any(|(source, claims)| {
                claims.source_hint.as_deref() != Some(source.as_str())
                    || claims.node_ids.windows(2).any(|pair| pair[0] >= pair[1])
                    || claims
                        .node_ids
                        .iter()
                        .any(|node| !crate::is_valid_external_id(node))
                    || !claimed_edges_strictly_sorted(&claims.edges)
                    || claims.edges.iter().any(|edge| {
                        !crate::is_valid_external_id(&edge.source)
                            || !crate::is_valid_external_id(&edge.target)
                            || edge.relation.is_empty()
                            || edge.relation != edge.relation.trim()
                            || edge.direction > 1
                            || (edge.direction == 1 && edge.source > edge.target)
                    })
            })
            || self
                .exact_source_key
                .as_ref()
                .is_some_and(|source| !self.source_digests.contains_key(source))
        {
            return Ok(false);
        }
        if !self.resolution_decisions_valid() {
            return Ok(false);
        }
        let resolution_input_digest =
            digest_json(&(CODE_RESOLUTION_INPUT_DIGEST_DOMAIN, &self.resolution_inputs))?;
        if resolution_input_digest != self.resolution_input_digest {
            return Ok(false);
        }
        let resolution_hint_digest =
            digest_json(&(CODE_RESOLUTION_HINT_DIGEST_DOMAIN, &self.resolution_hints))?;
        if resolution_hint_digest != self.resolution_hint_digest {
            return Ok(false);
        }
        let resolution_digest =
            digest_json(&(CODE_RESOLUTION_DIGEST_DOMAIN, &self.resolution_decisions))?;
        if resolution_digest != self.resolution_digest {
            return Ok(false);
        }
        let pipeline_digest = digest_json(&(CODE_PIPELINE_DIGEST_DOMAIN, &self.pipeline_receipt))?;
        if pipeline_digest != self.pipeline_digest {
            return Ok(false);
        }

        let lineage_digest = digest_json(&(
            CODE_LINEAGE_DIGEST_DOMAIN,
            self.root_identity.as_str(),
            self.exact_source_key.as_deref(),
            self.base_ownership_digest.as_deref(),
            &self.source_digests,
        ))?;
        let ownership_digest = self.compute_ownership_digest()?;
        Ok(lineage_digest == self.lineage_digest && ownership_digest == self.ownership_digest)
    }

    /// Validate the receipt against the actual graph topology. A self-consistent
    /// hash is not an authority signature and can be resealed around false
    /// COMPLETE claims; this re-audit reconstructs the producer relation and
    /// requires the same digest to emerge from the supplied graph.
    pub fn verify_against_graph(&self, graph: &Graph) -> Result<bool, serde_json::Error> {
        let slots = graph_slot_audit(graph);
        if !self.verify_receipt()?
            || !graph.finalized
            || !graph.csr.pending_edges.is_empty()
            || !slots.csr_shape_valid
            || !reverse_csr_valid(graph)
            || !bidirectional_mirrors_valid(graph)
            || !slots.orphan_node_slots.is_empty()
            || !slots.multiply_identified_node_slots.is_empty()
            || !slots.invalid_identity_ids.is_empty()
            || !slots.out_of_range_identity_ids.is_empty()
            || !slots.orphan_edge_slots.is_empty()
            || source_projection_digest(graph)? != self.source_projection_digest
        {
            return Ok(false);
        }

        let mut collector = OwnershipCollectorV1::default();
        for (source_key, claims) in &self.claims_by_source {
            collector.extend_source_claims(source_key, claims);
        }
        collector.record_resolution_inputs(self.resolution_inputs.clone());
        collector.record_resolution_hints(self.resolution_hints.clone());
        collector.record_resolution_decisions(self.resolution_decisions.clone());
        collector.set_pipeline_receipt(self.pipeline_receipt.clone());
        let audited = collector.audit(
            graph,
            self.root_identity.clone(),
            self.exact_source_key.clone(),
            self.base_ownership_digest.clone(),
            self.source_digests.clone(),
        )?;
        Ok(audited.coverage == OwnershipCoverageV1::Complete
            && audited.lineage_digest == self.lineage_digest
            && audited.ownership_digest == self.ownership_digest)
    }
}

fn resolution_decisions_valid_for(
    inputs: &[ResolutionInputV1],
    hints: &[ResolutionHintV1],
    decisions: &[ResolutionDecisionV1],
    source_digests: &BTreeMap<String, String>,
    claims_by_source: &BTreeMap<String, SourceClaims>,
) -> bool {
    if inputs.windows(2).any(|pair| pair[0] >= pair[1])
        || hints.windows(2).any(|pair| pair[0] >= pair[1])
        || decisions.windows(2).any(|pair| pair[0] >= pair[1])
        || inputs.len() != decisions.len()
    {
        return false;
    }
    if inputs.iter().any(|input| {
        !crate::is_valid_relative_file_path(&input.source_key)
            || input.source_id.is_empty()
            || input.source_id != input.source_id.trim()
            || input.target_label.is_empty()
            || input.target_label != input.target_label.trim()
            || input.relation.is_empty()
            || input.relation != input.relation.trim()
    }) {
        return false;
    }
    let decision_inputs = decisions
        .iter()
        .map(ResolutionInputV1::from_decision)
        .collect::<Vec<_>>();
    if decision_inputs != inputs {
        return false;
    }
    if hints.iter().any(|hint| {
        hint.source_id.is_empty()
            || hint.source_id != hint.source_id.trim()
            || hint.target_label.is_empty()
            || hint.target_label != hint.target_label.trim()
            || hint.import_path.is_empty()
            || hint.import_path != hint.import_path.trim()
            || !inputs.iter().any(|input| {
                input.source_id == hint.source_id && input.target_label == hint.target_label
            })
    }) || hints.windows(2).any(|pair| {
        pair[0].source_id == pair[1].source_id && pair[0].target_label == pair[1].target_label
    }) {
        return false;
    }

    decisions.iter().all(|decision| {
        if !source_digests.contains_key(&decision.source_key)
            || decision.source_line_start.is_some() != decision.source_line_end.is_some()
            || decision
                .candidate_ids
                .windows(2)
                .any(|pair| pair[0] >= pair[1])
            || decision
                .candidate_ids
                .iter()
                .any(|candidate| candidate.is_empty() || candidate != candidate.trim())
            || decision
                .source_line_start
                .zip(decision.source_line_end)
                .is_some_and(|(start, end)| start > end)
        {
            return false;
        }
        let Some(claims) = claims_by_source.get(&decision.source_key) else {
            return false;
        };
        if claims.node_ids.binary_search(&decision.source_id).is_err() {
            return false;
        }
        match decision.outcome {
            ResolutionOutcomeV1::Unresolved => {
                decision.resolved_target_id.is_none() && decision.candidate_ids.is_empty()
            }
            ResolutionOutcomeV1::Resolved | ResolutionOutcomeV1::Ambiguous => {
                let Some(target) = decision.resolved_target_id.as_deref() else {
                    return false;
                };
                let candidate_shape_valid = decision
                    .candidate_ids
                    .binary_search_by(|candidate| candidate.as_str().cmp(target))
                    .is_ok()
                    && if decision.outcome == ResolutionOutcomeV1::Ambiguous {
                        decision.candidate_ids.len() >= 2
                    } else {
                        !decision.candidate_ids.is_empty()
                    };
                candidate_shape_valid
                    && claims.edges.iter().any(|edge| {
                        edge.source == decision.source_id
                            && edge.target == target
                            && edge.relation == decision.relation
                            && edge.direction == 0
                            && !edge.inhibitory
                    })
            }
        }
    })
}

#[derive(Default)]
pub struct OwnershipCollectorV1 {
    sources: BTreeSet<String>,
    node_owners: HashMap<String, BTreeSet<String>>,
    edge_owners: HashMap<ClaimedEdgeKey, BTreeSet<String>>,
    resolution_inputs: Vec<ResolutionInputV1>,
    resolution_hints: Vec<ResolutionHintV1>,
    resolution_decisions: Vec<ResolutionDecisionV1>,
    pipeline_receipt: Option<CodePipelineReceiptV1>,
}

impl OwnershipCollectorV1 {
    pub fn register_source(&mut self, source_key: &str) {
        if !source_key.trim().is_empty() {
            self.sources.insert(source_key.to_string());
        }
    }

    pub fn claim_node(&mut self, source_key: &str, external_id: &str) {
        if source_key.trim().is_empty() || external_id.trim().is_empty() {
            return;
        }
        self.register_source(source_key);
        self.node_owners
            .entry(external_id.to_string())
            .or_default()
            .insert(source_key.to_string());
    }

    pub fn claim_edge(&mut self, claim: OwnedEdgeClaimV1) {
        if claim.source_key.trim().is_empty()
            || claim.source.trim().is_empty()
            || claim.target.trim().is_empty()
            || claim.relation.trim().is_empty()
        {
            return;
        }
        self.register_source(&claim.source_key);
        self.edge_owners
            .entry(claim.claimed_key())
            .or_default()
            .insert(claim.source_key);
    }

    pub fn extend(&mut self, delta: OwnershipDeltaV1) {
        for (source_key, external_id) in delta.nodes {
            self.claim_node(&source_key, &external_id);
        }
        for edge in delta.edges {
            self.claim_edge(edge);
        }
    }

    pub fn extend_source_claims(&mut self, source_key: &str, claims: &SourceClaims) {
        self.register_source(source_key);
        for external_id in &claims.node_ids {
            self.claim_node(source_key, external_id);
        }
        for edge in &claims.edges {
            self.claim_edge(OwnedEdgeClaimV1 {
                source_key: source_key.to_string(),
                source: edge.source.clone(),
                target: edge.target.clone(),
                relation: edge.relation.clone(),
                direction: edge.direction,
                inhibitory: edge.inhibitory,
            });
        }
    }

    pub fn record_resolution_decisions(
        &mut self,
        decisions: impl IntoIterator<Item = ResolutionDecisionV1>,
    ) {
        self.resolution_decisions.extend(decisions);
    }

    pub fn record_resolution_inputs(
        &mut self,
        inputs: impl IntoIterator<Item = ResolutionInputV1>,
    ) {
        self.resolution_inputs.extend(inputs);
    }

    pub fn record_resolution_hints(&mut self, hints: impl IntoIterator<Item = ResolutionHintV1>) {
        self.resolution_hints.extend(hints);
    }

    pub fn set_pipeline_receipt(&mut self, receipt: CodePipelineReceiptV1) {
        self.pipeline_receipt = Some(receipt);
    }

    pub fn audit(
        mut self,
        graph: &Graph,
        root_identity: String,
        exact_source_key: Option<String>,
        base_ownership_digest: Option<String>,
        source_digests: BTreeMap<String, String>,
    ) -> Result<CodeOwnershipManifestV1, serde_json::Error> {
        for source_key in source_digests.keys() {
            self.register_source(source_key);
        }
        let GraphSlotAuditV1 {
            node_ids,
            orphan_node_slots,
            multiply_identified_node_slots,
            invalid_identity_ids,
            out_of_range_identity_ids,
            orphan_edge_slots,
            csr_shape_valid,
        } = graph_slot_audit(graph);
        let graph_nodes = node_ids
            .into_iter()
            .filter(|external_id| !external_id.is_empty())
            .collect::<BTreeSet<_>>();
        let graph_edge_list = graph_edge_keys(graph);
        let mut graph_edge_counts = HashMap::<ClaimedEdgeKey, usize>::new();
        for edge in &graph_edge_list {
            *graph_edge_counts.entry(edge.clone()).or_default() += 1;
        }
        let mut duplicate_graph_edges = graph_edge_counts
            .iter()
            .filter(|(_, count)| **count > 1)
            .map(|(edge, _)| edge.clone())
            .collect::<Vec<_>>();
        sort_edge_keys(&mut duplicate_graph_edges);
        let graph_edges = graph_edge_list.into_iter().collect::<HashSet<_>>();
        let claimed_nodes = self.node_owners.keys().cloned().collect::<BTreeSet<_>>();
        let claimed_edges = self.edge_owners.keys().cloned().collect::<HashSet<_>>();

        let unowned_nodes = graph_nodes
            .difference(&claimed_nodes)
            .cloned()
            .collect::<Vec<_>>();
        let dangling_node_claims = claimed_nodes
            .difference(&graph_nodes)
            .cloned()
            .collect::<Vec<_>>();
        let mut unowned_edges = graph_edges
            .difference(&claimed_edges)
            .cloned()
            .collect::<Vec<_>>();
        let mut dangling_edge_claims = claimed_edges
            .difference(&graph_edges)
            .cloned()
            .collect::<Vec<_>>();
        sort_edge_keys(&mut unowned_edges);
        sort_edge_keys(&mut dangling_edge_claims);

        let mut claims_by_source = self
            .sources
            .into_iter()
            .map(|source| {
                (
                    source.clone(),
                    SourceClaims {
                        source_hint: Some(source),
                        ..SourceClaims::default()
                    },
                )
            })
            .collect::<BTreeMap<String, SourceClaims>>();
        for (external_id, owners) in self.node_owners {
            for owner in owners {
                let claims =
                    claims_by_source
                        .entry(owner.clone())
                        .or_insert_with(|| SourceClaims {
                            source_hint: Some(owner),
                            ..SourceClaims::default()
                        });
                claims.node_ids.push(external_id.clone());
            }
        }
        for (edge, owners) in self.edge_owners {
            for owner in owners {
                let claims =
                    claims_by_source
                        .entry(owner.clone())
                        .or_insert_with(|| SourceClaims {
                            source_hint: Some(owner),
                            ..SourceClaims::default()
                        });
                claims.edges.push(edge.clone());
            }
        }
        for claims in claims_by_source.values_mut() {
            claims.node_ids.sort();
            claims.node_ids.dedup();
            sort_edge_keys(&mut claims.edges);
            claims.edges.dedup();
        }

        let graph_finalized = graph.finalized;
        let pending_edge_count = graph.csr.pending_edges.len() as u64;
        let bidirectional_mirrors_valid = csr_shape_valid && bidirectional_mirrors_valid(graph);
        let reverse_csr_valid = csr_shape_valid && reverse_csr_valid(graph);
        let source_projection_digest = source_projection_digest(graph)?;
        self.resolution_inputs.sort();
        let resolution_inputs = self.resolution_inputs;
        let resolution_input_digest =
            digest_json(&(CODE_RESOLUTION_INPUT_DIGEST_DOMAIN, &resolution_inputs))?;
        self.resolution_hints.sort();
        let resolution_hints = self.resolution_hints;
        let resolution_hint_digest =
            digest_json(&(CODE_RESOLUTION_HINT_DIGEST_DOMAIN, &resolution_hints))?;
        self.resolution_decisions.sort();
        let resolution_decisions = self.resolution_decisions;
        let resolution_digest =
            digest_json(&(CODE_RESOLUTION_DIGEST_DOMAIN, &resolution_decisions))?;
        let resolution_decisions_valid = resolution_decisions_valid_for(
            &resolution_inputs,
            &resolution_hints,
            &resolution_decisions,
            &source_digests,
            &claims_by_source,
        );
        let pipeline_receipt = self
            .pipeline_receipt
            .unwrap_or_else(CodePipelineReceiptV1::missing);
        let pipeline_digest = digest_json(&(CODE_PIPELINE_DIGEST_DOMAIN, &pipeline_receipt))?;
        let claim_sources = claims_by_source.keys().collect::<BTreeSet<_>>();
        let digest_sources = source_digests.keys().collect::<BTreeSet<_>>();
        let coverage = if graph_finalized
            && pending_edge_count == 0
            && bidirectional_mirrors_valid
            && csr_shape_valid
            && reverse_csr_valid
            && orphan_node_slots.is_empty()
            && multiply_identified_node_slots.is_empty()
            && invalid_identity_ids.is_empty()
            && out_of_range_identity_ids.is_empty()
            && orphan_edge_slots.is_empty()
            && claim_sources == digest_sources
            && resolution_decisions_valid
            && pipeline_receipt.valid_for_sources(&source_digests)
            && unowned_nodes.is_empty()
            && unowned_edges.is_empty()
            && dangling_node_claims.is_empty()
            && dangling_edge_claims.is_empty()
            && duplicate_graph_edges.is_empty()
        {
            OwnershipCoverageV1::Complete
        } else {
            OwnershipCoverageV1::Incomplete
        };
        let lineage_digest = digest_json(&(
            CODE_LINEAGE_DIGEST_DOMAIN,
            root_identity.as_str(),
            exact_source_key.as_deref(),
            base_ownership_digest.as_deref(),
            &source_digests,
        ))?;
        let mut manifest = CodeOwnershipManifestV1 {
            schema: CODE_OWNERSHIP_MANIFEST_SCHEMA.to_string(),
            root_identity,
            exact_source_key,
            base_ownership_digest,
            source_digests,
            claims_by_source,
            source_projection_digest,
            graph_finalized,
            pending_edge_count,
            bidirectional_mirrors_valid,
            csr_shape_valid,
            reverse_csr_valid,
            orphan_node_slots,
            multiply_identified_node_slots,
            invalid_identity_ids,
            out_of_range_identity_ids,
            orphan_edge_slots,
            resolution_inputs,
            resolution_input_digest,
            resolution_hints,
            resolution_hint_digest,
            resolution_decisions,
            resolution_digest,
            pipeline_receipt,
            pipeline_digest,
            coverage,
            unowned_nodes,
            unowned_edges,
            dangling_node_claims,
            dangling_edge_claims,
            duplicate_graph_edges,
            lineage_digest,
            ownership_digest: String::new(),
        };
        manifest.ownership_digest = manifest.compute_ownership_digest()?;
        Ok(manifest)
    }
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Identity of the exact executable image acting as the ingest producer. The
/// value is cached because the running image is immutable for this process and
/// ownership verification can occur repeatedly on large graphs.
pub fn running_producer_executable_identity() -> std::io::Result<String> {
    static IDENTITY: std::sync::OnceLock<Result<String, String>> = std::sync::OnceLock::new();
    match IDENTITY.get_or_init(|| {
        let executable = std::env::current_exe()
            .map_err(|error| format!("resolve current executable: {error}"))?
            .canonicalize()
            .map_err(|error| format!("canonicalize current executable: {error}"))?;
        let metadata = std::fs::metadata(&executable)
            .map_err(|error| format!("read current executable metadata: {error}"))?;
        if !metadata.is_file() {
            return Err(format!(
                "current executable is not a regular file: {}",
                executable.display()
            ));
        }
        let bytes = std::fs::read(&executable)
            .map_err(|error| format!("read current executable bytes: {error}"))?;
        Ok(sha256_bytes(&bytes))
    }) {
        Ok(identity) => Ok(identity.clone()),
        Err(error) => Err(std::io::Error::other(error.clone())),
    }
}

/// Exact identity of the compiled ingest producer. The receipt must not treat
/// two binaries with the same semver but different extractor/policy sources or
/// feature/target profiles as interchangeable.
pub fn compiled_producer_build_identity() -> String {
    const SOURCES: &[(&str, &[u8])] = &[
        ("Cargo.toml", include_bytes!("../Cargo.toml")),
        ("src/bibtex_adapter.rs", include_bytes!("bibtex_adapter.rs")),
        ("src/canonical.rs", include_bytes!("canonical.rs")),
        (
            "src/cargo_workspace.rs",
            include_bytes!("cargo_workspace.rs"),
        ),
        ("src/cross_domain.rs", include_bytes!("cross_domain.rs")),
        ("src/cross_file.rs", include_bytes!("cross_file.rs")),
        (
            "src/crossref_adapter.rs",
            include_bytes!("crossref_adapter.rs"),
        ),
        ("src/diff.rs", include_bytes!("diff.rs")),
        (
            "src/document_router.rs",
            include_bytes!("document_router.rs"),
        ),
        (
            "src/extract/generic.rs",
            include_bytes!("extract/generic.rs"),
        ),
        ("src/extract/go.rs", include_bytes!("extract/go.rs")),
        ("src/extract/java.rs", include_bytes!("extract/java.rs")),
        ("src/extract/mod.rs", include_bytes!("extract/mod.rs")),
        ("src/extract/python.rs", include_bytes!("extract/python.rs")),
        (
            "src/extract/rust_lang.rs",
            include_bytes!("extract/rust_lang.rs"),
        ),
        (
            "src/extract/tree_sitter_ext.rs",
            include_bytes!("extract/tree_sitter_ext.rs"),
        ),
        (
            "src/extract/typescript.rs",
            include_bytes!("extract/typescript.rs"),
        ),
        ("src/jats_adapter.rs", include_bytes!("jats_adapter.rs")),
        ("src/json_adapter.rs", include_bytes!("json_adapter.rs")),
        ("src/l1ght_adapter.rs", include_bytes!("l1ght_adapter.rs")),
        ("src/lib.rs", include_bytes!("lib.rs")),
        ("src/memory_adapter.rs", include_bytes!("memory_adapter.rs")),
        ("src/merge.rs", include_bytes!("merge.rs")),
        ("src/ownership.rs", include_bytes!("ownership.rs")),
        ("src/patent_adapter.rs", include_bytes!("patent_adapter.rs")),
        ("src/path_policy.rs", include_bytes!("path_policy.rs")),
        ("src/resolve.rs", include_bytes!("resolve.rs")),
        ("src/rfc_adapter.rs", include_bytes!("rfc_adapter.rs")),
        (
            "src/universal_adapter.rs",
            include_bytes!("universal_adapter.rs"),
        ),
        ("src/walker.rs", include_bytes!("walker.rs")),
    ];

    let configuration = format!(
        "package={};version={};tier1={};tier2={};debug={};os={};arch={};family={}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION"),
        cfg!(feature = "tier1"),
        cfg!(feature = "tier2"),
        cfg!(debug_assertions),
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::consts::FAMILY,
    );
    let mut hasher = Sha256::new();
    hasher.update(b"m1nd-ingest-compiled-producer-v1\0");
    hasher.update((configuration.len() as u64).to_le_bytes());
    hasher.update(configuration.as_bytes());
    for (path, bytes) in SOURCES {
        hasher.update((path.len() as u64).to_le_bytes());
        hasher.update(path.as_bytes());
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    format!("{:x}", hasher.finalize())
}

#[derive(Serialize)]
struct GraphContentV1 {
    domain: &'static str,
    nodes: Vec<GraphContentNodeV1>,
    edges: Vec<GraphContentEdgeV1>,
}

#[derive(Serialize)]
struct GraphContentNodeV1 {
    external_id: String,
    label: String,
    node_type: String,
    tags: Vec<String>,
    last_modified_bits: u64,
    change_frequency_bits: u32,
    provenance_source_path: Option<String>,
    provenance_line_start: Option<u32>,
    provenance_line_end: Option<u32>,
    provenance_excerpt: Option<String>,
    provenance_namespace: Option<String>,
    provenance_canonical: bool,
}

#[derive(Serialize)]
struct GraphContentEdgeV1 {
    source: String,
    target: String,
    relation: String,
    direction: u8,
    inhibitory: bool,
    weight_bits: u32,
    causal_strength_bits: u32,
}

#[derive(Serialize)]
struct LiveGraphContentV1 {
    domain: &'static str,
    source_projection_digest: String,
    generation: u64,
    finalized: bool,
    pagerank_computed: bool,
    pagerank_dirty: bool,
    nodes: Vec<LiveGraphNodeV1>,
    edges: Vec<LiveGraphEdgeV1>,
    pending_edges: Vec<LivePendingEdgeV1>,
}

#[derive(Serialize)]
struct LiveGraphNodeV1 {
    external_id: String,
    activation_bits: [u32; 4],
    pagerank_bits: u32,
    incoming_weight_sum_bits: u32,
    ceiling_bits: u32,
}

#[derive(Serialize)]
struct LiveGraphEdgeV1 {
    source: String,
    target: String,
    relation: String,
    direction: u8,
    inhibitory: bool,
    csr_weight_bits: u32,
    causal_strength_bits: u32,
    original_weight_bits: Option<u32>,
    current_weight_bits: Option<u32>,
    strengthen_count: Option<u16>,
    weaken_count: Option<u16>,
    ltp_applied: Option<bool>,
    ltd_applied: Option<bool>,
    last_used_query: Option<u32>,
}

#[derive(Serialize)]
struct LivePendingEdgeV1 {
    source: u32,
    target: u32,
    relation: String,
    direction: u8,
    inhibitory: bool,
    weight_bits: u32,
    causal_strength_bits: u32,
}

/// Canonical source-projection digest. It includes source-derived semantic
/// attributes and original edge weights, while deliberately excluding live
/// activation/PageRank/current plasticity so ordinary read-write learning does
/// not make the ownership receipt stale.
pub fn source_projection_digest(graph: &Graph) -> Result<String, serde_json::Error> {
    let slot_audit = graph_slot_audit(graph);
    if !slot_audit.csr_shape_valid {
        return Err(serde_json::Error::io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "graph CSR/plasticity storage shape is invalid",
        )));
    }
    let node_ids = slot_audit.node_ids;
    let mut nodes = Vec::with_capacity(graph.num_nodes() as usize);
    for (index, external_id) in node_ids.iter().enumerate() {
        if external_id.is_empty() {
            continue;
        }
        let node_id = NodeId::new(index as u32);
        let provenance = graph.resolve_node_provenance(node_id);
        let mut tags = graph.nodes.tags[index]
            .iter()
            .map(|tag| graph.strings.resolve(*tag).to_string())
            .collect::<Vec<_>>();
        tags.sort();
        tags.dedup();
        nodes.push(GraphContentNodeV1 {
            external_id: external_id.clone(),
            label: graph.strings.resolve(graph.nodes.label[index]).to_string(),
            node_type: format!("{:?}", graph.nodes.node_type[index]),
            tags,
            last_modified_bits: graph.nodes.last_modified[index].to_bits(),
            change_frequency_bits: graph.nodes.change_frequency[index].get().to_bits(),
            provenance_source_path: provenance.source_path,
            provenance_line_start: provenance.line_start,
            provenance_line_end: provenance.line_end,
            provenance_excerpt: provenance.excerpt,
            provenance_namespace: provenance.namespace,
            provenance_canonical: provenance.canonical,
        });
    }
    nodes.sort_by(|left, right| left.external_id.cmp(&right.external_id));

    let mut edges = Vec::with_capacity(graph.csr.num_edges());
    for source_index in 0..graph.num_nodes() as usize {
        if node_ids[source_index].is_empty() {
            continue;
        }
        for edge_index in graph.csr.out_range(NodeId::new(source_index as u32)) {
            let target_index = graph.csr.targets[edge_index].as_usize();
            if target_index >= node_ids.len() || node_ids[target_index].is_empty() {
                continue;
            }
            let direction = graph.csr.directions[edge_index];
            if direction == EdgeDirection::Bidirectional && source_index > target_index {
                continue;
            }
            let (source, target) = if direction == EdgeDirection::Bidirectional
                && node_ids[source_index] > node_ids[target_index]
            {
                (
                    node_ids[target_index].clone(),
                    node_ids[source_index].clone(),
                )
            } else {
                (
                    node_ids[source_index].clone(),
                    node_ids[target_index].clone(),
                )
            };
            edges.push(GraphContentEdgeV1 {
                source,
                target,
                relation: graph
                    .strings
                    .resolve(graph.csr.relations[edge_index])
                    .to_string(),
                direction: if direction == EdgeDirection::Bidirectional {
                    1
                } else {
                    0
                },
                inhibitory: graph.csr.inhibitory[edge_index],
                weight_bits: graph.edge_plasticity.original_weight[edge_index]
                    .get()
                    .to_bits(),
                causal_strength_bits: graph.csr.causal_strengths[edge_index].get().to_bits(),
            });
        }
    }
    edges.sort_by(|left, right| {
        (
            left.source.as_str(),
            left.target.as_str(),
            left.relation.as_str(),
            left.direction,
            left.inhibitory,
            left.weight_bits,
            left.causal_strength_bits,
        )
            .cmp(&(
                right.source.as_str(),
                right.target.as_str(),
                right.relation.as_str(),
                right.direction,
                right.inhibitory,
                right.weight_bits,
                right.causal_strength_bits,
            ))
    });

    digest_json(&GraphContentV1 {
        domain: CODE_SOURCE_PROJECTION_DIGEST_DOMAIN,
        nodes,
        edges,
    })
}

/// Digest of the complete live in-memory graph state used by checkpoint
/// evidence. Unlike the stable source-projection digest, this includes both
/// CSR slots of bidirectional edges plus current learning state.
pub fn live_graph_content_digest(graph: &Graph) -> Result<String, serde_json::Error> {
    let node_ids = graph_node_ids_by_index(graph);
    let mut nodes = Vec::with_capacity(graph.num_nodes() as usize);
    for (index, external_id) in node_ids.iter().enumerate() {
        if external_id.is_empty() {
            continue;
        }
        nodes.push(LiveGraphNodeV1 {
            external_id: external_id.clone(),
            activation_bits: graph.nodes.activation[index].map(|value| value.get().to_bits()),
            pagerank_bits: graph.nodes.pagerank[index].get().to_bits(),
            incoming_weight_sum_bits: graph.nodes.plasticity[index]
                .incoming_weight_sum
                .get()
                .to_bits(),
            ceiling_bits: graph.nodes.plasticity[index].ceiling.get().to_bits(),
        });
    }
    nodes.sort_by(|left, right| left.external_id.cmp(&right.external_id));

    let mut edges = Vec::with_capacity(graph.csr.num_edges());
    for source_index in 0..graph.num_nodes() as usize {
        for edge_index in graph.csr.out_range(NodeId::new(source_index as u32)) {
            let target_index = graph.csr.targets[edge_index].as_usize();
            let source = node_ids.get(source_index).cloned().unwrap_or_default();
            let target = node_ids.get(target_index).cloned().unwrap_or_default();
            edges.push(LiveGraphEdgeV1 {
                source,
                target,
                relation: graph
                    .strings
                    .resolve(graph.csr.relations[edge_index])
                    .to_string(),
                direction: if graph.csr.directions[edge_index] == EdgeDirection::Bidirectional {
                    1
                } else {
                    0
                },
                inhibitory: graph.csr.inhibitory[edge_index],
                csr_weight_bits: graph
                    .csr
                    .read_weight(EdgeIdx::new(edge_index as u32))
                    .get()
                    .to_bits(),
                causal_strength_bits: graph.csr.causal_strengths[edge_index].get().to_bits(),
                original_weight_bits: graph
                    .edge_plasticity
                    .original_weight
                    .get(edge_index)
                    .map(|value| value.get().to_bits()),
                current_weight_bits: graph
                    .edge_plasticity
                    .current_weight
                    .get(edge_index)
                    .map(|value| value.get().to_bits()),
                strengthen_count: graph
                    .edge_plasticity
                    .strengthen_count
                    .get(edge_index)
                    .copied(),
                weaken_count: graph.edge_plasticity.weaken_count.get(edge_index).copied(),
                ltp_applied: graph.edge_plasticity.ltp_applied.get(edge_index).copied(),
                ltd_applied: graph.edge_plasticity.ltd_applied.get(edge_index).copied(),
                last_used_query: graph
                    .edge_plasticity
                    .last_used_query
                    .get(edge_index)
                    .copied(),
            });
        }
    }
    edges.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then_with(|| left.target.cmp(&right.target))
            .then_with(|| left.relation.cmp(&right.relation))
            .then_with(|| left.direction.cmp(&right.direction))
            .then_with(|| left.inhibitory.cmp(&right.inhibitory))
            .then_with(|| left.csr_weight_bits.cmp(&right.csr_weight_bits))
            .then_with(|| left.causal_strength_bits.cmp(&right.causal_strength_bits))
            .then_with(|| left.original_weight_bits.cmp(&right.original_weight_bits))
            .then_with(|| left.current_weight_bits.cmp(&right.current_weight_bits))
            .then_with(|| left.strengthen_count.cmp(&right.strengthen_count))
            .then_with(|| left.weaken_count.cmp(&right.weaken_count))
            .then_with(|| left.ltp_applied.cmp(&right.ltp_applied))
            .then_with(|| left.ltd_applied.cmp(&right.ltd_applied))
            .then_with(|| left.last_used_query.cmp(&right.last_used_query))
    });

    let mut pending_edges = graph
        .csr
        .pending_edges
        .iter()
        .map(|edge| LivePendingEdgeV1 {
            source: edge.source.0,
            target: edge.target.0,
            relation: graph.strings.resolve(edge.relation).to_string(),
            direction: if edge.direction == EdgeDirection::Bidirectional {
                1
            } else {
                0
            },
            inhibitory: edge.inhibitory,
            weight_bits: edge.weight.get().to_bits(),
            causal_strength_bits: edge.causal_strength.get().to_bits(),
        })
        .collect::<Vec<_>>();
    pending_edges.sort_by(|left, right| {
        (
            left.source,
            left.target,
            left.relation.as_str(),
            left.direction,
            left.inhibitory,
            left.weight_bits,
            left.causal_strength_bits,
        )
            .cmp(&(
                right.source,
                right.target,
                right.relation.as_str(),
                right.direction,
                right.inhibitory,
                right.weight_bits,
                right.causal_strength_bits,
            ))
    });

    digest_json(&LiveGraphContentV1 {
        domain: LIVE_GRAPH_CONTENT_DIGEST_DOMAIN,
        source_projection_digest: source_projection_digest(graph)?,
        generation: graph.generation.0,
        finalized: graph.finalized,
        pagerank_computed: graph.pagerank_computed,
        pagerank_dirty: graph.pagerank_dirty,
        nodes,
        edges,
        pending_edges,
    })
}

/// True when the graph already contains the exact canonical edge. Ingestion
/// uses this after an insertion error so a second producing source can still
/// acquire shared ownership without pretending that an invalid edge succeeded.
pub fn graph_has_edge(
    graph: &Graph,
    source: NodeId,
    target: NodeId,
    relation: &str,
    direction: EdgeDirection,
    inhibitory: bool,
) -> bool {
    let Some(relation_id) = graph.strings.lookup(relation) else {
        return false;
    };

    let matches = |edge_source: NodeId,
                   edge_target: NodeId,
                   edge_relation,
                   edge_direction: EdgeDirection,
                   edge_inhibitory: bool| {
        edge_relation == relation_id
            && edge_direction == direction
            && edge_inhibitory == inhibitory
            && if direction == EdgeDirection::Bidirectional {
                (edge_source == source && edge_target == target)
                    || (edge_source == target && edge_target == source)
            } else {
                edge_source == source && edge_target == target
            }
    };

    if graph.csr.pending_edges.iter().any(|edge| {
        matches(
            edge.source,
            edge.target,
            edge.relation,
            edge.direction,
            edge.inhibitory,
        )
    }) {
        return true;
    }

    graph.csr.out_range(source).any(|edge_idx| {
        matches(
            source,
            graph.csr.targets[edge_idx],
            graph.csr.relations[edge_idx],
            graph.csr.directions[edge_idx],
            graph.csr.inhibitory[edge_idx],
        )
    }) || (direction == EdgeDirection::Bidirectional
        && graph.csr.out_range(target).any(|edge_idx| {
            matches(
                target,
                graph.csr.targets[edge_idx],
                graph.csr.relations[edge_idx],
                graph.csr.directions[edge_idx],
                graph.csr.inhibitory[edge_idx],
            )
        }))
}

fn bidirectional_mirrors_valid(graph: &Graph) -> bool {
    if graph.edge_plasticity.original_weight.len() < graph.csr.num_edges() {
        return false;
    }
    let mut multiplicities = BTreeMap::<(u32, u32, u32, bool, u32, u32), (usize, usize)>::new();
    for source_index in 0..graph.num_nodes() as usize {
        let source = NodeId::new(source_index as u32);
        for edge_index in graph.csr.out_range(source) {
            if graph.csr.directions[edge_index] != EdgeDirection::Bidirectional {
                continue;
            }
            let target = graph.csr.targets[edge_index];
            let key = (
                source.0.min(target.0),
                source.0.max(target.0),
                graph.csr.relations[edge_index].0,
                graph.csr.inhibitory[edge_index],
                graph.csr.causal_strengths[edge_index].get().to_bits(),
                graph.edge_plasticity.original_weight[edge_index]
                    .get()
                    .to_bits(),
            );
            let counts = multiplicities.entry(key).or_default();
            if source.0 <= target.0 {
                counts.0 += 1;
            } else {
                counts.1 += 1;
            }
        }
    }
    multiplicities
        .into_iter()
        .all(|((source, target, ..), counts)| {
            if source == target {
                counts.0 > 0 && counts.0 % 2 == 0
            } else {
                counts.0 > 0 && counts.0 == counts.1
            }
        })
}

fn reverse_csr_valid(graph: &Graph) -> bool {
    let edge_count = graph.csr.targets.len();
    let node_count = graph.num_nodes() as usize;
    let mut forward_sources = vec![usize::MAX; edge_count];
    for source in 0..node_count {
        for edge in graph.csr.out_range(NodeId::new(source as u32)) {
            if edge >= edge_count || forward_sources[edge] != usize::MAX {
                return false;
            }
            forward_sources[edge] = source;
        }
    }
    if forward_sources.contains(&usize::MAX) {
        return false;
    }

    let mut reverse_counts = vec![0u8; edge_count];
    for target in 0..node_count {
        for reverse_slot in graph.csr.in_range(NodeId::new(target as u32)) {
            let Some(edge) = graph.csr.rev_edge_idx.get(reverse_slot) else {
                return false;
            };
            let edge = edge.as_usize();
            if edge >= edge_count
                || graph.csr.targets[edge].as_usize() != target
                || graph.csr.rev_sources[reverse_slot].as_usize() != forward_sources[edge]
            {
                return false;
            }
            reverse_counts[edge] = reverse_counts[edge].saturating_add(1);
        }
    }
    reverse_counts.into_iter().all(|count| count == 1)
}

fn digest_json<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    serde_json::to_vec(value).map(|bytes| sha256_bytes(&bytes))
}

fn canonical_claimed_edge(
    source: &str,
    target: &str,
    relation: &str,
    direction: u8,
    inhibitory: bool,
) -> ClaimedEdgeKey {
    if direction == 1 && source > target {
        ClaimedEdgeKey {
            source: target.to_string(),
            target: source.to_string(),
            relation: relation.to_string(),
            direction,
            inhibitory,
        }
    } else {
        ClaimedEdgeKey {
            source: source.to_string(),
            target: target.to_string(),
            relation: relation.to_string(),
            direction,
            inhibitory,
        }
    }
}

#[derive(Default)]
struct GraphSlotAuditV1 {
    node_ids: Vec<String>,
    orphan_node_slots: Vec<u32>,
    multiply_identified_node_slots: Vec<u32>,
    invalid_identity_ids: Vec<String>,
    out_of_range_identity_ids: Vec<String>,
    orphan_edge_slots: Vec<u64>,
    csr_shape_valid: bool,
}

fn graph_slot_audit(graph: &Graph) -> GraphSlotAuditV1 {
    let node_count = graph.num_nodes() as usize;
    let mut identities_by_slot = vec![BTreeSet::<String>::new(); node_count];
    let mut invalid_identity_ids = Vec::new();
    let mut out_of_range_identity_ids = Vec::new();

    for (interned, &node) in &graph.id_to_node {
        let external_id = graph.strings.resolve(*interned).to_string();
        if node.as_usize() >= node_count {
            out_of_range_identity_ids.push(external_id);
        } else if !crate::is_valid_external_id(&external_id) {
            invalid_identity_ids.push(external_id);
        } else {
            identities_by_slot[node.as_usize()].insert(external_id);
        }
    }
    invalid_identity_ids.sort();
    invalid_identity_ids.dedup();
    out_of_range_identity_ids.sort();
    out_of_range_identity_ids.dedup();

    let mut node_ids = vec![String::new(); node_count];
    let mut orphan_node_slots = Vec::new();
    let mut multiply_identified_node_slots = Vec::new();
    for (slot, identities) in identities_by_slot.into_iter().enumerate() {
        match identities.len() {
            0 => orphan_node_slots.push(slot as u32),
            1 => node_ids[slot] = identities.into_iter().next().expect("single identity"),
            _ => multiply_identified_node_slots.push(slot as u32),
        }
    }

    let edge_count = graph.csr.targets.len();
    let offsets_valid = offsets_shape_valid(&graph.csr.offsets, node_count, edge_count);
    let reverse_offsets_valid = offsets_shape_valid(
        &graph.csr.rev_offsets,
        node_count,
        graph.csr.rev_sources.len(),
    );
    let csr_shape_valid = offsets_valid
        && reverse_offsets_valid
        && graph.csr.weights.len() == edge_count
        && graph.csr.inhibitory.len() == edge_count
        && graph.csr.relations.len() == edge_count
        && graph.csr.directions.len() == edge_count
        && graph.csr.causal_strengths.len() == edge_count
        && graph.csr.rev_sources.len() == edge_count
        && graph.csr.rev_edge_idx.len() == edge_count
        && graph
            .csr
            .targets
            .iter()
            .all(|target| target.as_usize() < node_count)
        && graph
            .csr
            .rev_sources
            .iter()
            .all(|source| source.as_usize() < node_count)
        && graph
            .csr
            .rev_edge_idx
            .iter()
            .all(|edge| edge.as_usize() < edge_count)
        && graph.edge_plasticity.original_weight.len() == edge_count
        && graph.edge_plasticity.current_weight.len() == edge_count
        && graph.edge_plasticity.strengthen_count.len() == edge_count
        && graph.edge_plasticity.weaken_count.len() == edge_count
        && graph.edge_plasticity.ltp_applied.len() == edge_count
        && graph.edge_plasticity.ltd_applied.len() == edge_count
        && graph.edge_plasticity.last_used_query.len() == edge_count;

    let mut orphan_edge_slots = Vec::new();
    if csr_shape_valid {
        for source_slot in 0..node_count {
            for edge_slot in graph.csr.out_range(NodeId::new(source_slot as u32)) {
                let target_slot = graph.csr.targets[edge_slot].as_usize();
                if node_ids[source_slot].is_empty() || node_ids[target_slot].is_empty() {
                    orphan_edge_slots.push(edge_slot as u64);
                }
            }
        }
        orphan_edge_slots.sort_unstable();
        orphan_edge_slots.dedup();
    }

    GraphSlotAuditV1 {
        node_ids,
        orphan_node_slots,
        multiply_identified_node_slots,
        invalid_identity_ids,
        out_of_range_identity_ids,
        orphan_edge_slots,
        csr_shape_valid,
    }
}

fn offsets_shape_valid(offsets: &[u64], node_count: usize, edge_count: usize) -> bool {
    offsets.len() == node_count + 1
        && offsets.first() == Some(&0)
        && offsets.last().copied() == Some(edge_count as u64)
        && offsets
            .windows(2)
            .all(|pair| pair[0] <= pair[1] && pair[1] <= edge_count as u64)
}

fn graph_node_ids_by_index(graph: &Graph) -> Vec<String> {
    graph_slot_audit(graph).node_ids
}

fn graph_edge_keys(graph: &Graph) -> Vec<ClaimedEdgeKey> {
    let slot_audit = graph_slot_audit(graph);
    if !slot_audit.csr_shape_valid {
        return Vec::new();
    }
    let node_ids = slot_audit.node_ids;
    let mut edges = Vec::new();
    for source_idx in 0..graph.num_nodes() as usize {
        let source = &node_ids[source_idx];
        if source.is_empty() {
            continue;
        }
        for edge_idx in graph.csr.out_range(NodeId::new(source_idx as u32)) {
            let target_idx = graph.csr.targets[edge_idx].as_usize();
            if target_idx >= node_ids.len() || node_ids[target_idx].is_empty() {
                continue;
            }
            let direction = graph.csr.directions[edge_idx];
            if direction == EdgeDirection::Bidirectional && source_idx > target_idx {
                continue;
            }
            edges.push(canonical_claimed_edge(
                source,
                &node_ids[target_idx],
                graph.strings.resolve(graph.csr.relations[edge_idx]),
                if direction == EdgeDirection::Bidirectional {
                    1
                } else {
                    0
                },
                graph.csr.inhibitory[edge_idx],
            ));
        }
    }
    edges
}

fn sort_edge_keys(edges: &mut [ClaimedEdgeKey]) {
    edges.sort_by(|left, right| {
        (
            left.source.as_str(),
            left.target.as_str(),
            left.relation.as_str(),
            left.direction,
            left.inhibitory,
        )
            .cmp(&(
                right.source.as_str(),
                right.target.as_str(),
                right.relation.as_str(),
                right.direction,
                right.inhibitory,
            ))
    });
}

fn claimed_edges_strictly_sorted(edges: &[ClaimedEdgeKey]) -> bool {
    edges.windows(2).all(|pair| {
        (
            pair[0].source.as_str(),
            pair[0].target.as_str(),
            pair[0].relation.as_str(),
            pair[0].direction,
            pair[0].inhibitory,
        ) < (
            pair[1].source.as_str(),
            pair[1].target.as_str(),
            pair[1].relation.as_str(),
            pair[1].direction,
            pair[1].inhibitory,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{
        digest_json, live_graph_content_digest, source_projection_digest, OwnedEdgeClaimV1,
        OwnershipCollectorV1, OwnershipCoverageV1, CODE_OWNERSHIP_DIGEST_DOMAIN,
    };
    use crate::merge::prune_source_claims;
    use m1nd_core::graph::Graph;
    use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};
    use std::collections::{BTreeMap, HashMap};

    #[test]
    fn shared_node_and_edge_claims_survive_removing_one_owner() {
        let mut graph = Graph::new();
        let source = graph
            .add_node("shared::source", "source", NodeType::File, &[], 0.0, 0.0)
            .unwrap();
        let target = graph
            .add_node(
                "shared::target",
                "target",
                NodeType::Function,
                &[],
                0.0,
                0.0,
            )
            .unwrap();
        graph
            .add_edge(
                source,
                target,
                "contains",
                FiniteF32::new(1.0),
                EdgeDirection::Forward,
                false,
                FiniteF32::ZERO,
            )
            .unwrap();
        graph.finalize().unwrap();

        let mut collector = OwnershipCollectorV1::default();
        collector.set_pipeline_receipt(super::CodePipelineReceiptV1::test_default(2));
        for owner in ["a.rs", "b.rs"] {
            collector.claim_node(owner, "shared::source");
            collector.claim_node(owner, "shared::target");
            collector.claim_edge(OwnedEdgeClaimV1::forward(
                owner,
                "shared::source",
                "shared::target",
                "contains",
            ));
        }
        let manifest = collector
            .audit(
                &graph,
                "/managed/root".into(),
                None,
                None,
                BTreeMap::from([
                    ("a.rs".into(), "digest-a".into()),
                    ("b.rs".into(), "digest-b".into()),
                ]),
            )
            .unwrap();

        assert_eq!(manifest.coverage, OwnershipCoverageV1::Complete);
        assert!(manifest.claims_by_source["a.rs"]
            .node_ids
            .contains(&"shared::source".to_string()));
        assert!(manifest.claims_by_source["b.rs"]
            .node_ids
            .contains(&"shared::source".to_string()));
        assert_eq!(manifest.claims_by_source["a.rs"].edges.len(), 1);
        assert_eq!(manifest.claims_by_source["b.rs"].edges.len(), 1);

        let mut wrong_schema = manifest.clone();
        wrong_schema.schema = "m1nd-code-ownership-manifest-v999".into();
        assert!(!wrong_schema.verify_receipt().unwrap());

        // A malicious/buggy writer can reseal a self-consistent hash around a
        // false COMPLETE manifest. Topology verification must still reject it.
        let mut forged = manifest.clone();
        for claims in forged.claims_by_source.values_mut() {
            claims.node_ids.retain(|node| node != "shared::target");
        }
        forged.coverage = OwnershipCoverageV1::Complete;
        forged.unowned_nodes.clear();
        forged.unowned_edges.clear();
        forged.dangling_node_claims.clear();
        forged.dangling_edge_claims.clear();
        forged.duplicate_graph_edges.clear();
        forged.ownership_digest = forged.compute_ownership_digest().unwrap();
        assert!(forged.verify_receipt().unwrap());
        assert!(!forged.verify_against_graph(&graph).unwrap());

        let claims: HashMap<_, _> = manifest.claims_by_source.into_iter().collect();
        let pruned = prune_source_claims(&graph, "a.rs", &claims).unwrap();
        let retained_source = pruned.resolve_id("shared::source").unwrap();
        let retained_target = pruned.resolve_id("shared::target").unwrap();
        assert!(pruned.csr.out_range(retained_source).any(|edge_idx| {
            pruned.csr.targets[edge_idx] == retained_target
                && pruned.strings.resolve(pruned.csr.relations[edge_idx]) == "contains"
        }));
    }

    fn finalized_owned_pair() -> (Graph, super::CodeOwnershipManifestV1) {
        let mut graph = Graph::new();
        let source = graph
            .add_node("a", "source", NodeType::File, &["original"], 1.0, 0.1)
            .unwrap();
        let target = graph
            .add_node("b", "target", NodeType::Function, &[], 2.0, 0.2)
            .unwrap();
        graph
            .add_edge(
                source,
                target,
                "contains",
                FiniteF32::new(0.75),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.25),
            )
            .unwrap();
        graph.finalize().unwrap();

        let mut collector = OwnershipCollectorV1::default();
        collector.set_pipeline_receipt(super::CodePipelineReceiptV1::test_default(1));
        collector.claim_node("a.rs", "a");
        collector.claim_node("a.rs", "b");
        collector.claim_edge(OwnedEdgeClaimV1::forward("a.rs", "a", "b", "contains"));
        let manifest = collector
            .audit(
                &graph,
                "/managed/root".into(),
                None,
                None,
                BTreeMap::from([("a.rs".into(), "digest-a".into())]),
            )
            .unwrap();
        assert!(manifest.verify_against_graph(&graph).unwrap());
        (graph, manifest)
    }

    #[test]
    fn orphan_node_slot_is_explicit_and_cannot_verify_complete() {
        let mut graph = Graph::new();
        let node = graph
            .add_node("node::a", "a", NodeType::File, &[], 0.0, 0.0)
            .unwrap();
        graph.finalize().unwrap();
        let identity = graph.strings.lookup("node::a").expect("interned identity");
        assert_eq!(graph.id_to_node.remove(&identity), Some(node));

        let mut collector = OwnershipCollectorV1::default();
        collector.set_pipeline_receipt(super::CodePipelineReceiptV1::test_default(1));
        collector.claim_node("a.rs", "node::a");
        let manifest = collector
            .audit(
                &graph,
                "/managed/root".into(),
                None,
                None,
                BTreeMap::from([("a.rs".into(), "digest-a".into())]),
            )
            .unwrap();

        assert_eq!(manifest.coverage, OwnershipCoverageV1::Incomplete);
        assert_eq!(manifest.orphan_node_slots, vec![node.as_usize() as u32]);
        assert!(!manifest.verify_receipt().unwrap());
    }

    #[test]
    fn multiply_identified_node_and_its_edges_are_rejected() {
        let mut graph = Graph::new();
        let source = graph
            .add_node("node::source", "source", NodeType::File, &[], 0.0, 0.0)
            .unwrap();
        let target = graph
            .add_node("node::target", "target", NodeType::Function, &[], 0.0, 0.0)
            .unwrap();
        graph
            .add_edge(
                source,
                target,
                "contains",
                FiniteF32::new(1.0),
                EdgeDirection::Forward,
                false,
                FiniteF32::ZERO,
            )
            .unwrap();
        graph.finalize().unwrap();
        let alias = graph.strings.get_or_intern("node::target-alias");
        graph.id_to_node.insert(alias, target);

        let mut collector = OwnershipCollectorV1::default();
        collector.set_pipeline_receipt(super::CodePipelineReceiptV1::test_default(1));
        collector.claim_node("a.rs", "node::source");
        collector.claim_node("a.rs", "node::target");
        collector.claim_edge(OwnedEdgeClaimV1::forward(
            "a.rs",
            "node::source",
            "node::target",
            "contains",
        ));
        let manifest = collector
            .audit(
                &graph,
                "/managed/root".into(),
                None,
                None,
                BTreeMap::from([("a.rs".into(), "digest-a".into())]),
            )
            .unwrap();

        assert_eq!(manifest.coverage, OwnershipCoverageV1::Incomplete);
        assert_eq!(
            manifest.multiply_identified_node_slots,
            vec![target.as_usize() as u32]
        );
        assert_eq!(manifest.orphan_edge_slots, vec![0]);
        assert!(!manifest.verify_receipt().unwrap());
    }

    #[test]
    fn malformed_csr_parallel_arrays_fail_without_projection_panic() {
        let (mut graph, _) = finalized_owned_pair();
        graph.csr.causal_strengths.clear();

        let error = source_projection_digest(&graph)
            .expect_err("malformed CSR storage must be rejected before indexing");
        assert!(error.to_string().contains("storage shape is invalid"));
    }

    #[test]
    fn resealed_receipt_cannot_forge_producer_or_cross_file_accounting() {
        let (_, receipt) = finalized_owned_pair();

        let mut forged_producer = receipt.clone();
        forged_producer.pipeline_receipt.producer_build_identity = "0".repeat(64);
        forged_producer.pipeline_digest = digest_json(&(
            super::CODE_PIPELINE_DIGEST_DOMAIN,
            &forged_producer.pipeline_receipt,
        ))
        .unwrap();
        forged_producer.ownership_digest = forged_producer.compute_ownership_digest().unwrap();
        assert!(!forged_producer.verify_receipt().unwrap());

        let (_, mut forged_executable) = finalized_owned_pair();
        forged_executable
            .pipeline_receipt
            .producer_executable_identity = "f".repeat(64);
        forged_executable.pipeline_digest = digest_json(&(
            super::CODE_PIPELINE_DIGEST_DOMAIN,
            &forged_executable.pipeline_receipt,
        ))
        .unwrap();
        forged_executable.ownership_digest = forged_executable.compute_ownership_digest().unwrap();
        assert!(!forged_executable.verify_receipt().unwrap());

        let mut forged_cross_file = receipt;
        forged_cross_file
            .pipeline_receipt
            .cross_file_source_files_expected = 1;
        forged_cross_file
            .pipeline_receipt
            .cross_file_source_metadata_verified = 1;
        forged_cross_file
            .pipeline_receipt
            .cross_file_source_files_read = 1;
        forged_cross_file
            .pipeline_receipt
            .cross_file_source_files_parsed = 1;
        forged_cross_file.pipeline_digest = digest_json(&(
            super::CODE_PIPELINE_DIGEST_DOMAIN,
            &forged_cross_file.pipeline_receipt,
        ))
        .unwrap();
        forged_cross_file.ownership_digest = forged_cross_file.compute_ownership_digest().unwrap();
        assert!(!forged_cross_file.verify_receipt().unwrap());

        let (_, mut forged_cargo) = finalized_owned_pair();
        forged_cargo
            .pipeline_receipt
            .cargo_dependency_inputs_expected = 1;
        forged_cargo
            .pipeline_receipt
            .cargo_dependency_inputs_accounted = 1;
        forged_cargo.pipeline_digest = digest_json(&(
            super::CODE_PIPELINE_DIGEST_DOMAIN,
            &forged_cargo.pipeline_receipt,
        ))
        .unwrap();
        forged_cargo.ownership_digest = forged_cargo.compute_ownership_digest().unwrap();
        assert!(!forged_cargo.verify_receipt().unwrap());

        let (_, mut forged_claim_identity) = finalized_owned_pair();
        forged_claim_identity
            .claims_by_source
            .get_mut("a.rs")
            .unwrap()
            .node_ids[0] = " a".into();
        forged_claim_identity.ownership_digest =
            forged_claim_identity.compute_ownership_digest().unwrap();
        assert!(!forged_claim_identity.verify_receipt().unwrap());
    }

    #[test]
    fn receipt_rejects_pending_edges_and_same_topology_content_mutations() {
        let (mut pending, receipt) = finalized_owned_pair();
        let a = pending.resolve_id("a").unwrap();
        let b = pending.resolve_id("b").unwrap();
        pending
            .add_edge(
                b,
                a,
                "calls",
                FiniteF32::new(1.0),
                EdgeDirection::Forward,
                false,
                FiniteF32::ZERO,
            )
            .unwrap();
        assert!(!pending.finalized);
        assert!(!pending.csr.pending_edges.is_empty());
        assert!(!receipt.verify_against_graph(&pending).unwrap());

        let (mut changed, receipt) = finalized_owned_pair();
        changed.nodes.label[0] = changed.strings.get_or_intern("changed-label");
        assert!(!receipt.verify_against_graph(&changed).unwrap());

        let (mut changed, receipt) = finalized_owned_pair();
        changed.nodes.node_type[0] = NodeType::Class;
        assert!(!receipt.verify_against_graph(&changed).unwrap());

        let (mut changed, receipt) = finalized_owned_pair();
        let tag = changed.strings.get_or_intern("changed-tag");
        changed.nodes.tags[0].push(tag);
        assert!(!receipt.verify_against_graph(&changed).unwrap());

        let (mut changed, receipt) = finalized_owned_pair();
        changed.set_node_provenance(
            changed.resolve_id("a").unwrap(),
            m1nd_core::graph::NodeProvenanceInput {
                source_path: Some("changed.rs"),
                line_start: Some(7),
                excerpt: Some("changed"),
                ..Default::default()
            },
        );
        assert!(!receipt.verify_against_graph(&changed).unwrap());

        let (changed, receipt) = finalized_owned_pair();
        let live_before = live_graph_content_digest(&changed).unwrap();
        changed
            .csr
            .atomic_write_weight(m1nd_core::types::EdgeIdx::new(0), FiniteF32::new(0.5), 8)
            .unwrap();
        assert!(receipt.verify_against_graph(&changed).unwrap());
        assert_ne!(live_graph_content_digest(&changed).unwrap(), live_before);

        let (mut changed, receipt) = finalized_owned_pair();
        changed.edge_plasticity.original_weight[0] = FiniteF32::new(0.5);
        assert!(!receipt.verify_against_graph(&changed).unwrap());

        let (mut changed, receipt) = finalized_owned_pair();
        changed.csr.rev_sources[0] = changed.resolve_id("b").unwrap();
        assert!(!receipt.verify_against_graph(&changed).unwrap());
    }

    #[test]
    fn bidirectional_reverse_live_weight_is_checkpoint_visible_but_projection_stable() {
        let mut graph = Graph::new();
        let a = graph
            .add_node("a", "a", NodeType::File, &[], 0.0, 0.0)
            .unwrap();
        let b = graph
            .add_node("b", "b", NodeType::File, &[], 0.0, 0.0)
            .unwrap();
        graph
            .add_edge(
                a,
                b,
                "related",
                FiniteF32::new(0.8),
                EdgeDirection::Bidirectional,
                false,
                FiniteF32::new(0.4),
            )
            .unwrap();
        graph.finalize().unwrap();
        let mut collector = OwnershipCollectorV1::default();
        collector.set_pipeline_receipt(super::CodePipelineReceiptV1::test_default(1));
        collector.claim_node("a.rs", "a");
        collector.claim_node("a.rs", "b");
        collector.claim_edge(OwnedEdgeClaimV1 {
            source_key: "a.rs".into(),
            source: "a".into(),
            target: "b".into(),
            relation: "related".into(),
            direction: 1,
            inhibitory: false,
        });
        let receipt = collector
            .audit(
                &graph,
                "/managed/root".into(),
                None,
                None,
                BTreeMap::from([("a.rs".into(), "digest-a".into())]),
            )
            .unwrap();
        assert!(receipt.verify_against_graph(&graph).unwrap());
        let stable_before = source_projection_digest(&graph).unwrap();
        let live_before = live_graph_content_digest(&graph).unwrap();

        graph
            .csr
            .atomic_write_weight(m1nd_core::types::EdgeIdx::new(1), FiniteF32::new(0.3), 8)
            .unwrap();
        assert_eq!(source_projection_digest(&graph).unwrap(), stable_before);
        assert!(receipt.verify_against_graph(&graph).unwrap());
        assert_ne!(live_graph_content_digest(&graph).unwrap(), live_before);

        graph.edge_plasticity.original_weight[1] = FiniteF32::new(0.2);
        assert!(!receipt.verify_against_graph(&graph).unwrap());

        graph.edge_plasticity.original_weight[1] = FiniteF32::new(0.8);
        let reverse = 1usize;
        graph.csr.targets.push(graph.csr.targets[reverse]);
        graph.csr.weights.push(std::sync::atomic::AtomicU32::new(
            graph
                .csr
                .read_weight(m1nd_core::types::EdgeIdx::new(reverse as u32))
                .get()
                .to_bits(),
        ));
        graph.csr.inhibitory.push(graph.csr.inhibitory[reverse]);
        graph.csr.relations.push(graph.csr.relations[reverse]);
        graph.csr.directions.push(graph.csr.directions[reverse]);
        graph
            .csr
            .causal_strengths
            .push(graph.csr.causal_strengths[reverse]);
        *graph.csr.offsets.last_mut().unwrap() += 1;
        graph
            .edge_plasticity
            .original_weight
            .push(FiniteF32::new(0.8));
        graph
            .edge_plasticity
            .current_weight
            .push(FiniteF32::new(0.8));
        graph.edge_plasticity.strengthen_count.push(0);
        graph.edge_plasticity.weaken_count.push(0);
        graph.edge_plasticity.ltp_applied.push(false);
        graph.edge_plasticity.ltd_applied.push(false);
        graph.edge_plasticity.last_used_query.push(0);
        assert!(!receipt.verify_against_graph(&graph).unwrap());
    }
}
