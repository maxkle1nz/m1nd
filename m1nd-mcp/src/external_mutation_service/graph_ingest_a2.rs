//! Governed A2 admission for full-root code ingestion.
//!
//! This module owns only graph-ingest domain facts. Authority linearization,
//! leasing, journaling, actor dispatch, and checkpointing remain owned by the
//! external-mutation service and the existing owner runtime.

use std::collections::BTreeSet;
use std::fs::File;
#[cfg(unix)]
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use m1nd_ingest::ownership::{CodeOwnershipManifestV1, OwnershipCoverageV1};
use m1nd_ingest::{IngestConfig, Ingestor};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::external_mutation_journal::{
    ExternalMutationJournalEntryV1, ExternalMutationJournalPhaseV1,
};
use crate::session::SessionState;

pub(super) const GRAPH_INGEST_A2_PAYLOAD_SCHEMA: &str = "m1nd-graph-ingest-a2-semantic-payload-v1";
pub(super) const GRAPH_INGEST_A2_PAYLOAD_DIGEST_DOMAIN: &str =
    "m1nd-graph-ingest-a2-semantic-payload-v1";
pub(super) const GRAPH_INGEST_A2_OUTCOME_DIGEST_DOMAIN: &str = "m1nd-graph-ingest-a2-outcome-v1";
pub(super) const GRAPH_INGEST_A2_RECOVERY_KIND: &str = "graph_ingest_a2";
const GRAPH_INGEST_A2_CANDIDATE_ARTIFACT_SCHEMA: &str =
    "m1nd-graph-ingest-a2-candidate-artifact-v1";
const GRAPH_INGEST_A2_CANDIDATE_ARTIFACT_REF_SCHEMA: &str =
    "m1nd-graph-ingest-a2-candidate-artifact-ref-v1";
const GRAPH_INGEST_A2_CANDIDATE_ARTIFACT_ID_DOMAIN: &str =
    "m1nd-graph-ingest-a2-candidate-artifact-id-v1";
const GRAPH_INGEST_A2_CANDIDATE_OWNERSHIP_DIGEST_DOMAIN: &str =
    "m1nd-graph-ingest-a2-candidate-ownership-v1";
const GRAPH_INGEST_A2_CANDIDATE_STATS_DIGEST_DOMAIN: &str =
    "m1nd-graph-ingest-a2-candidate-stats-v1";
const GRAPH_INGEST_A2_CANDIDATE_DIR: &str = "graph-ingest-candidates-v1";
const GRAPH_INGEST_A2_CANDIDATE_MAGIC: &[u8] = b"m1nd-graph-ingest-a2-candidate-artifact-v1\n";
// A corrupt WAL reference must never be able to drive an unbounded allocation
// before its content digest is checked. 512 MiB is intentionally far above the
// currently exercised graph snapshots while still being a hard owner limit.
const GRAPH_INGEST_A2_MAX_CANDIDATE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GraphIngestA2ModeV1 {
    Replace,
    MergeExisting,
}

impl GraphIngestA2ModeV1 {
    pub(super) const fn semantic_action(self) -> &'static str {
        match self {
            Self::Replace => "graph.ingest.replace",
            Self::MergeExisting => "graph.ingest.merge_existing",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphIngestA2ParentV1 {
    pub operation_id: String,
    pub lease_id: String,
    pub reservation_id: String,
    pub operation_object_digest: String,
    pub semantic_payload_digest: String,
    pub outcome_digest: String,
    pub published_result_digest: String,
}

impl GraphIngestA2ParentV1 {
    pub(super) fn validate_shape(&self) -> Result<(), GraphIngestA2Error> {
        if self.operation_id.trim().is_empty()
            || self.lease_id.trim().is_empty()
            || self.reservation_id.trim().is_empty()
            || !is_digest(&self.operation_object_digest)
            || !is_digest(&self.semantic_payload_digest)
            || !is_digest(&self.outcome_digest)
            || !is_digest(&self.published_result_digest)
        {
            return Err(GraphIngestA2Error::new(
                "graph_ingest_parent_binding_invalid",
                "merge_existing requires a complete digest-bound source.edit parent link",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphIngestA2InputV1 {
    /// Owner-issued stateless preview binding. It commits the execute request
    /// to one MCP session, ingress context, selected actor, and operation
    /// object without making the client compute any digest.
    pub preview_id: String,
    pub root: String,
    pub expected_graph_generation: u64,
    pub expected_source_projection_digest: String,
    #[serde(default)]
    pub include_dotfiles: bool,
    #[serde(default)]
    pub dotfile_patterns: Vec<String>,
    pub parent: Option<GraphIngestA2ParentV1>,
}

impl GraphIngestA2InputV1 {
    pub(super) fn validate_shape(
        &self,
        mode: GraphIngestA2ModeV1,
    ) -> Result<(), GraphIngestA2Error> {
        if !is_digest(&self.preview_id)
            || self.root.trim().is_empty()
            || !is_digest(&self.expected_source_projection_digest)
        {
            return Err(GraphIngestA2Error::new(
                "graph_ingest_request_invalid",
                "owner preview id, full-root path, and exact current source-projection digest are required",
            ));
        }
        if self
            .dotfile_patterns
            .iter()
            .any(|pattern| !is_safe_relative_discovery_pattern(pattern))
        {
            return Err(GraphIngestA2Error::new(
                "graph_ingest_discovery_controls_invalid",
                "dotfile patterns must be non-empty trimmed relative patterns",
            ));
        }
        let unique = self.dotfile_patterns.iter().collect::<BTreeSet<&String>>();
        if unique.len() != self.dotfile_patterns.len() {
            return Err(GraphIngestA2Error::new(
                "graph_ingest_discovery_controls_invalid",
                "dotfile patterns must be unique",
            ));
        }
        match (mode, self.parent.as_ref()) {
            (GraphIngestA2ModeV1::Replace, None) => Ok(()),
            (GraphIngestA2ModeV1::Replace, Some(_)) => Err(GraphIngestA2Error::new(
                "graph_ingest_replace_parent_forbidden",
                "sovereign replace establishes a baseline and cannot claim a source.edit parent",
            )),
            (GraphIngestA2ModeV1::MergeExisting, Some(parent)) => parent.validate_shape(),
            (GraphIngestA2ModeV1::MergeExisting, None) => Err(GraphIngestA2Error::new(
                "graph_ingest_parent_required",
                "merge_existing is admitted only as a typed child of source.edit.commit",
            )),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GraphIngestA2SemanticPayloadV1 {
    pub schema: String,
    pub mode: GraphIngestA2ModeV1,
    pub root_identity: String,
    pub expected_graph_generation: u64,
    pub expected_source_projection_digest: String,
    pub include_dotfiles: bool,
    pub dotfile_patterns: Vec<String>,
    pub parent: Option<GraphIngestA2ParentV1>,
    pub candidate_ownership_digest: String,
    pub candidate_source_projection_digest: String,
    pub candidate_pipeline_digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GraphIngestA2RecoveryPayloadV1 {
    pub kind: String,
    pub preview_id: String,
    pub semantic_payload: GraphIngestA2SemanticPayloadV1,
    pub ownership_manifest: CodeOwnershipManifestV1,
    pub operation_object_digest: String,
    pub outcome_digest: String,
    pub reconciliation_brain_id: String,
    pub authority_subject_id: String,
    pub candidate_artifact: GraphIngestA2CandidateArtifactRefV1,
    pub forward_complete: bool,
}

pub(super) struct InspectedGraphIngestA2V1 {
    pub preview_id: String,
    pub semantic_payload: GraphIngestA2SemanticPayloadV1,
    pub semantic_payload_digest: String,
    pub ownership_manifest: CodeOwnershipManifestV1,
    pub reconciliation_brain_id: String,
    pub authority_subject_id: String,
    candidate: m1nd_ingest::CodeIngestBundleV1,
}

#[derive(Debug)]
pub(super) struct GraphIngestA2InspectionSnapshotV1 {
    preview_id: String,
    mode: GraphIngestA2ModeV1,
    root_identity: String,
    expected_graph_generation: u64,
    expected_source_projection_digest: String,
    include_dotfiles: bool,
    dotfile_patterns: Vec<String>,
    parent: Option<GraphIngestA2ParentV1>,
    baseline: Option<CodeOwnershipManifestV1>,
    reconciliation_brain_id: String,
    authority_subject_id: String,
}

impl GraphIngestA2InspectionSnapshotV1 {
    pub(super) const fn expected_graph_generation(&self) -> u64 {
        self.expected_graph_generation
    }
}

#[derive(Clone, Debug)]
pub(super) struct StagedGraphIngestA2V1 {
    pub preview_id: String,
    pub semantic_payload: GraphIngestA2SemanticPayloadV1,
    pub ownership_manifest: CodeOwnershipManifestV1,
    pub operation_object_digest: String,
    pub outcome_digest: String,
    pub reconciliation_brain_id: String,
    pub authority_subject_id: String,
    pub candidate_artifact: GraphIngestA2CandidateArtifactRefV1,
    artifact_root: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GraphIngestA2CandidateArtifactRefV1 {
    pub schema: String,
    pub relative_path: String,
    pub artifact_sha256: String,
    pub graph_snapshot_sha256: String,
    pub ownership_manifest_digest: String,
    pub stats_digest: String,
    pub byte_len: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphIngestA2CandidateArtifactV1 {
    schema: String,
    operation_object_digest: String,
    graph_snapshot_sha256: String,
    ownership_manifest_digest: String,
    stats_digest: String,
    ownership_manifest: CodeOwnershipManifestV1,
    stats: GraphIngestA2CandidateStatsV1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphIngestA2CandidateStatsV1 {
    files_scanned: u64,
    files_parsed: u64,
    files_skipped_binary: u64,
    files_skipped_encoding: u64,
    nodes_created: u64,
    edges_created: u64,
    references_resolved: u64,
    references_unresolved: u64,
    references_ambiguous: u64,
    label_collisions: u64,
    elapsed_ms: f64,
    commit_groups: Vec<Vec<String>>,
    discovered_files: Vec<GraphIngestA2DiscoveredFileV1>,
    file_inventory: Vec<crate::session::FileInventoryEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphIngestA2DiscoveredFileV1 {
    path: PathBuf,
    relative_path: String,
    extension: Option<String>,
    size_bytes: u64,
    last_modified: f64,
    commit_count: u32,
    last_commit_time: f64,
}

pub(super) struct DurableGraphIngestCandidateV1 {
    pub(super) bundle: m1nd_ingest::CodeIngestBundleV1,
    pub(super) file_inventory: Vec<crate::session::FileInventoryEntry>,
}

impl StagedGraphIngestA2V1 {
    pub(super) fn recovery_payload(&self) -> serde_json::Value {
        serde_json::to_value(GraphIngestA2RecoveryPayloadV1 {
            kind: GRAPH_INGEST_A2_RECOVERY_KIND.to_string(),
            preview_id: self.preview_id.clone(),
            semantic_payload: self.semantic_payload.clone(),
            ownership_manifest: self.ownership_manifest.clone(),
            operation_object_digest: self.operation_object_digest.clone(),
            outcome_digest: self.outcome_digest.clone(),
            reconciliation_brain_id: self.reconciliation_brain_id.clone(),
            authority_subject_id: self.authority_subject_id.clone(),
            candidate_artifact: self.candidate_artifact.clone(),
            forward_complete: true,
        })
        .expect("graph-ingest recovery payload is serializable")
    }

    pub(super) fn load_durable_candidate(
        &self,
        require_live_sources: bool,
    ) -> Result<DurableGraphIngestCandidateV1, GraphIngestA2Error> {
        let candidate = load_candidate_artifact(
            &self.artifact_root,
            &self.candidate_artifact,
            &self.operation_object_digest,
            &self.ownership_manifest,
        )?;
        if require_live_sources {
            candidate.bundle.revalidate_sources().map_err(|error| {
                GraphIngestA2Error::new("graph_ingest_candidate_drift", error.to_string())
            })?;
        }
        Ok(candidate)
    }

    pub(super) fn revalidate_actor_preimage(
        &self,
        state: &SessionState,
        require_original_preimage: bool,
    ) -> Result<(), GraphIngestA2Error> {
        let selected_root = selected_code_root(state).ok_or_else(|| {
            GraphIngestA2Error::new(
                "graph_ingest_selected_root_missing",
                "selected brain has no bound code root",
            )
        })?;
        if selected_root != self.semantic_payload.root_identity {
            return Err(GraphIngestA2Error::new(
                "graph_ingest_root_mismatch",
                "selected brain root changed after A2 admission",
            ));
        }
        if require_original_preimage {
            if state.graph_generation != self.semantic_payload.expected_graph_generation {
                return Err(GraphIngestA2Error::new(
                    "graph_ingest_stale_generation",
                    "graph generation changed before actor READY",
                ));
            }
            let observed = m1nd_ingest::ownership::source_projection_digest(&state.graph.read())
                .map_err(|error| {
                    GraphIngestA2Error::new(
                        "graph_ingest_live_projection_invalid",
                        error.to_string(),
                    )
                })?;
            if observed != self.semantic_payload.expected_source_projection_digest {
                return Err(GraphIngestA2Error::new(
                    "graph_ingest_stale_projection",
                    "source projection changed before actor READY",
                ));
            }
        }
        Ok(())
    }
}

impl GraphIngestA2CandidateStatsV1 {
    fn capture(bundle: &m1nd_ingest::CodeIngestBundleV1) -> Result<Self, GraphIngestA2Error> {
        let file_inventory = crate::tools::build_file_inventory_entries(
            &bundle.graph,
            &bundle.stats.discovered_files,
        );
        if file_inventory.len() != bundle.ownership.source_digests.len()
            || file_inventory.iter().any(|entry| entry.sha256.is_none())
        {
            return Err(GraphIngestA2Error::new(
                "graph_ingest_candidate_stats_incomplete",
                "candidate inventory must cover every source with immutable content bytes",
            ));
        }
        let discovered_files = bundle
            .stats
            .discovered_files
            .iter()
            .map(|file| GraphIngestA2DiscoveredFileV1 {
                path: file.path.clone(),
                relative_path: file.relative_path.clone(),
                extension: file.extension.clone(),
                size_bytes: file.size_bytes,
                last_modified: file.last_modified,
                commit_count: file.commit_count,
                last_commit_time: file.last_commit_time,
            })
            .collect();
        Ok(Self {
            files_scanned: bundle.stats.files_scanned,
            files_parsed: bundle.stats.files_parsed,
            files_skipped_binary: bundle.stats.files_skipped_binary,
            files_skipped_encoding: bundle.stats.files_skipped_encoding,
            nodes_created: bundle.stats.nodes_created,
            edges_created: bundle.stats.edges_created,
            references_resolved: bundle.stats.references_resolved,
            references_unresolved: bundle.stats.references_unresolved,
            references_ambiguous: bundle.stats.references_ambiguous,
            label_collisions: bundle.stats.label_collisions,
            elapsed_ms: bundle.stats.elapsed_ms,
            commit_groups: bundle.stats.commit_groups.clone(),
            discovered_files,
            file_inventory,
        })
    }

    fn into_runtime_parts(
        self,
    ) -> (
        m1nd_ingest::IngestStats,
        Vec<crate::session::FileInventoryEntry>,
    ) {
        let discovered_files = self
            .discovered_files
            .into_iter()
            .map(|file| m1nd_ingest::walker::DiscoveredFile {
                path: file.path,
                relative_path: file.relative_path,
                extension: file.extension,
                size_bytes: file.size_bytes,
                last_modified: file.last_modified,
                commit_count: file.commit_count,
                last_commit_time: file.last_commit_time,
            })
            .collect();
        (
            m1nd_ingest::IngestStats {
                files_scanned: self.files_scanned,
                files_parsed: self.files_parsed,
                files_skipped_binary: self.files_skipped_binary,
                files_skipped_encoding: self.files_skipped_encoding,
                nodes_created: self.nodes_created,
                edges_created: self.edges_created,
                references_resolved: self.references_resolved,
                references_unresolved: self.references_unresolved,
                references_ambiguous: self.references_ambiguous,
                label_collisions: self.label_collisions,
                elapsed_ms: self.elapsed_ms,
                commit_groups: self.commit_groups,
                discovered_files,
            },
            self.file_inventory,
        )
    }
}

fn persist_candidate_artifact(
    artifact_root: &Path,
    reservation_id: &str,
    operation_object_digest: &str,
    bundle: m1nd_ingest::CodeIngestBundleV1,
) -> Result<GraphIngestA2CandidateArtifactRefV1, GraphIngestA2Error> {
    bundle.require_complete().map_err(|error| {
        GraphIngestA2Error::new("graph_ingest_incomplete_candidate", error.to_string())
    })?;
    bundle.revalidate_sources().map_err(|error| {
        GraphIngestA2Error::new("graph_ingest_candidate_drift", error.to_string())
    })?;
    let stats = GraphIngestA2CandidateStatsV1::capture(&bundle)?;
    let stats_digest =
        m1nd_control::digest_canonical(GRAPH_INGEST_A2_CANDIDATE_STATS_DIGEST_DOMAIN, &stats)
            .map_err(|error| {
                GraphIngestA2Error::new("graph_ingest_digest_failed", error.to_string())
            })?;
    let ownership_manifest_digest = m1nd_control::digest_canonical(
        GRAPH_INGEST_A2_CANDIDATE_OWNERSHIP_DIGEST_DOMAIN,
        &bundle.ownership,
    )
    .map_err(|error| GraphIngestA2Error::new("graph_ingest_digest_failed", error.to_string()))?;
    let graph_snapshot =
        m1nd_core::snapshot::encode_graph_json(&bundle.graph).map_err(|error| {
            GraphIngestA2Error::new("graph_ingest_candidate_encode_failed", error.to_string())
        })?;
    let graph_snapshot_sha256 = sha256_bytes(&graph_snapshot);
    let header = GraphIngestA2CandidateArtifactV1 {
        schema: GRAPH_INGEST_A2_CANDIDATE_ARTIFACT_SCHEMA.to_string(),
        operation_object_digest: operation_object_digest.to_string(),
        graph_snapshot_sha256: graph_snapshot_sha256.clone(),
        ownership_manifest_digest: ownership_manifest_digest.clone(),
        stats_digest: stats_digest.clone(),
        ownership_manifest: bundle.ownership,
        stats,
    };
    let header_bytes = serde_json::to_vec(&header).map_err(|error| {
        GraphIngestA2Error::new("graph_ingest_candidate_encode_failed", error.to_string())
    })?;
    let header_len = u64::try_from(header_bytes.len()).map_err(|_| {
        GraphIngestA2Error::new(
            "graph_ingest_candidate_too_large",
            "candidate artifact header length overflowed u64",
        )
    })?;
    let mut bytes = Vec::with_capacity(
        GRAPH_INGEST_A2_CANDIDATE_MAGIC.len() + 8 + header_bytes.len() + graph_snapshot.len(),
    );
    bytes.extend_from_slice(GRAPH_INGEST_A2_CANDIDATE_MAGIC);
    bytes.extend_from_slice(&header_len.to_be_bytes());
    bytes.extend_from_slice(&header_bytes);
    bytes.extend_from_slice(&graph_snapshot);
    let artifact_sha256 = sha256_bytes(&bytes);
    let artifact_id = m1nd_control::digest_canonical(
        GRAPH_INGEST_A2_CANDIDATE_ARTIFACT_ID_DOMAIN,
        &(
            operation_object_digest,
            reservation_id,
            graph_snapshot_sha256.as_str(),
            ownership_manifest_digest.as_str(),
            stats_digest.as_str(),
        ),
    )
    .map_err(|error| GraphIngestA2Error::new("graph_ingest_digest_failed", error.to_string()))?;
    let relative_path = format!("{GRAPH_INGEST_A2_CANDIDATE_DIR}/{artifact_id}.candidate");
    let artifact_dir = ensure_candidate_artifact_dir(artifact_root)?;
    let final_path = artifact_root.join(&relative_path);
    #[cfg(windows)]
    let existing = match read_candidate_artifact_file(
        artifact_root,
        Path::new(&relative_path),
        bytes.len() as u64,
    ) {
        Ok(existing) => Some(existing),
        Err(error) if error.code == "graph_ingest_candidate_artifact_missing" => None,
        Err(error) => return Err(error),
    };
    #[cfg(not(windows))]
    let existing = if final_path.symlink_metadata().is_ok() {
        Some(read_candidate_artifact_file(
            artifact_root,
            Path::new(&relative_path),
            bytes.len() as u64,
        )?)
    } else {
        None
    };
    if let Some(existing) = existing {
        if existing != bytes {
            return Err(GraphIngestA2Error::new(
                "graph_ingest_candidate_artifact_collision",
                "content-addressed candidate path already contains different bytes",
            ));
        }
    } else {
        #[cfg(windows)]
        let windows_anchor = WindowsCandidateArtifactAnchor::open(artifact_root)?;
        let mut staged_path = None;
        for attempt in 0..128u16 {
            let stage_name = format!(".{artifact_id}.{}.{}.tmp", std::process::id(), attempt);
            let candidate = artifact_dir.join(&stage_name);
            #[cfg(unix)]
            let open_result = {
                let mut options = OpenOptions::new();
                options.create_new(true).write(true);
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW);
                options.mode(0o600);
                options.open(&candidate)
            };
            #[cfg(windows)]
            let open_result = crate::windows_durable_fs::create_relative_new_no_follow(
                &windows_anchor.directory,
                std::ffi::OsStr::new(&stage_name),
            );
            #[cfg(not(any(unix, windows)))]
            let open_result: std::io::Result<File> = Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "durable candidate staging is unavailable on this platform",
            ));
            match open_result {
                Ok(mut file) => {
                    file.write_all(&bytes)
                        .and_then(|()| file.sync_all())
                        .map_err(|error| {
                            GraphIngestA2Error::new(
                                "graph_ingest_candidate_artifact_write_failed",
                                error.to_string(),
                            )
                        })?;
                    staged_path = Some(candidate);
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(GraphIngestA2Error::new(
                        if cfg!(any(unix, windows)) {
                            "graph_ingest_candidate_artifact_write_failed"
                        } else {
                            "graph_ingest_candidate_artifact_nofollow_unavailable"
                        },
                        error.to_string(),
                    ))
                }
            }
        }
        let staged_path = staged_path.ok_or_else(|| {
            GraphIngestA2Error::new(
                "graph_ingest_candidate_artifact_collision",
                "could not allocate a unique candidate staging file",
            )
        })?;
        #[cfg(unix)]
        {
            std::fs::rename(&staged_path, &final_path).map_err(|error| {
                GraphIngestA2Error::new(
                    "graph_ingest_candidate_artifact_publish_failed",
                    error.to_string(),
                )
            })?;
            // Durability order is load-bearing: the artifact file is fsync'd
            // above, then its directory entry is fsync'd here, and only after
            // this function returns may the caller append+fsync PREPARED.
            File::open(&artifact_dir)
                .and_then(|dir| dir.sync_all())
                .map_err(|error| {
                    GraphIngestA2Error::new(
                        "graph_ingest_candidate_artifact_sync_failed",
                        error.to_string(),
                    )
                })?;
        }
        #[cfg(windows)]
        {
            // The staged file was flushed through its held handle. Windows has
            // no documented directory-fsync equivalent, so the reviewed
            // write-through namespace primitive is the publication barrier.
            crate::windows_durable_fs::move_new_write_through(&staged_path, &final_path).map_err(
                |error| {
                    GraphIngestA2Error::new(
                        "graph_ingest_candidate_artifact_publish_failed",
                        error.to_string(),
                    )
                },
            )?;
            let published = read_candidate_artifact_file(
                artifact_root,
                Path::new(&relative_path),
                bytes.len() as u64,
            )?;
            if published != bytes {
                return Err(GraphIngestA2Error::new(
                    "graph_ingest_candidate_artifact_corrupt",
                    "write-through publication did not preserve the sealed candidate bytes",
                ));
            }
        }
    }
    Ok(GraphIngestA2CandidateArtifactRefV1 {
        schema: GRAPH_INGEST_A2_CANDIDATE_ARTIFACT_REF_SCHEMA.to_string(),
        relative_path,
        artifact_sha256,
        graph_snapshot_sha256,
        ownership_manifest_digest,
        stats_digest,
        byte_len: bytes.len() as u64,
    })
}

fn load_candidate_artifact(
    artifact_root: &Path,
    reference: &GraphIngestA2CandidateArtifactRefV1,
    operation_object_digest: &str,
    expected_ownership: &CodeOwnershipManifestV1,
) -> Result<DurableGraphIngestCandidateV1, GraphIngestA2Error> {
    let path = constrained_candidate_artifact_path(artifact_root, reference)?;
    let relative = path.strip_prefix(artifact_root).map_err(|_| {
        GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_reference_invalid",
            "candidate artifact escaped its trusted owner root",
        )
    })?;
    let bytes = read_candidate_artifact_file(artifact_root, relative, reference.byte_len)?;
    if bytes.len() as u64 != reference.byte_len || sha256_bytes(&bytes) != reference.artifact_sha256
    {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_corrupt",
            "candidate artifact length or content digest differs from the sealed WAL reference",
        ));
    }
    let header_start = GRAPH_INGEST_A2_CANDIDATE_MAGIC.len() + 8;
    if !bytes.starts_with(GRAPH_INGEST_A2_CANDIDATE_MAGIC) || bytes.len() < header_start {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_corrupt",
            "candidate artifact framing is invalid",
        ));
    }
    let mut header_len_bytes = [0u8; 8];
    header_len_bytes.copy_from_slice(&bytes[GRAPH_INGEST_A2_CANDIDATE_MAGIC.len()..header_start]);
    let header_len = usize::try_from(u64::from_be_bytes(header_len_bytes)).map_err(|_| {
        GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_corrupt",
            "candidate artifact header length overflowed usize",
        )
    })?;
    let graph_start = header_start.checked_add(header_len).ok_or_else(|| {
        GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_corrupt",
            "candidate artifact header length overflow",
        )
    })?;
    if graph_start >= bytes.len() {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_corrupt",
            "candidate artifact contains no graph snapshot bytes",
        ));
    }
    let header: GraphIngestA2CandidateArtifactV1 =
        serde_json::from_slice(&bytes[header_start..graph_start]).map_err(|error| {
            GraphIngestA2Error::new("graph_ingest_candidate_artifact_corrupt", error.to_string())
        })?;
    let ownership_manifest_digest = m1nd_control::digest_canonical(
        GRAPH_INGEST_A2_CANDIDATE_OWNERSHIP_DIGEST_DOMAIN,
        &header.ownership_manifest,
    )
    .map_err(|error| GraphIngestA2Error::new("graph_ingest_digest_failed", error.to_string()))?;
    let stats_digest = m1nd_control::digest_canonical(
        GRAPH_INGEST_A2_CANDIDATE_STATS_DIGEST_DOMAIN,
        &header.stats,
    )
    .map_err(|error| GraphIngestA2Error::new("graph_ingest_digest_failed", error.to_string()))?;
    let graph_snapshot = &bytes[graph_start..];
    let graph_snapshot_sha256 = sha256_bytes(graph_snapshot);
    let expected_ownership_manifest_digest = m1nd_control::digest_canonical(
        GRAPH_INGEST_A2_CANDIDATE_OWNERSHIP_DIGEST_DOMAIN,
        expected_ownership,
    )
    .map_err(|error| GraphIngestA2Error::new("graph_ingest_digest_failed", error.to_string()))?;
    if header.schema != GRAPH_INGEST_A2_CANDIDATE_ARTIFACT_SCHEMA
        || header.operation_object_digest != operation_object_digest
        || ownership_manifest_digest != expected_ownership_manifest_digest
        || header.graph_snapshot_sha256 != graph_snapshot_sha256
        || header.ownership_manifest_digest != ownership_manifest_digest
        || header.stats_digest != stats_digest
        || reference.graph_snapshot_sha256 != graph_snapshot_sha256
        || reference.ownership_manifest_digest != ownership_manifest_digest
        || reference.stats_digest != stats_digest
    {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_binding_mismatch",
            "candidate graph, ownership, or stats differ from the sealed WAL bindings",
        ));
    }
    let graph = m1nd_core::snapshot::decode_graph_json(graph_snapshot).map_err(|error| {
        GraphIngestA2Error::new("graph_ingest_candidate_artifact_corrupt", error.to_string())
    })?;
    let observed_projection =
        m1nd_ingest::ownership::source_projection_digest(&graph).map_err(|error| {
            GraphIngestA2Error::new("graph_ingest_candidate_artifact_corrupt", error.to_string())
        })?;
    if observed_projection != expected_ownership.source_projection_digest {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_binding_mismatch",
            "decoded graph projection differs from the sealed ownership manifest",
        ));
    }
    let (stats, file_inventory) = header.stats.into_runtime_parts();
    let bundle = m1nd_ingest::CodeIngestBundleV1 {
        schema: m1nd_ingest::ownership::CODE_INGEST_BUNDLE_SCHEMA.to_string(),
        graph,
        stats,
        ownership: header.ownership_manifest,
    };
    require_sealed_candidate_complete(&bundle)?;
    Ok(DurableGraphIngestCandidateV1 {
        bundle,
        file_inventory,
    })
}

/// Recovery validates only facts sealed into the immutable artifact. Calling
/// `CodeIngestBundleV1::require_complete` here would intentionally reopen the
/// live source tree and make a post-COMMIT edit block deterministic forward
/// recovery—the exact ambiguity the artifact is meant to remove.
fn require_sealed_candidate_complete(
    bundle: &m1nd_ingest::CodeIngestBundleV1,
) -> Result<(), GraphIngestA2Error> {
    if bundle.schema != m1nd_ingest::ownership::CODE_INGEST_BUNDLE_SCHEMA
        || bundle.ownership.coverage != OwnershipCoverageV1::Complete
    {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_corrupt",
            "sealed candidate is not one COMPLETE code ingest bundle",
        ));
    }
    let valid = bundle
        .ownership
        .verify_against_graph(&bundle.graph)
        .map_err(|error| {
            GraphIngestA2Error::new(
                "graph_ingest_candidate_artifact_corrupt",
                format!("sealed ownership verification failed: {error}"),
            )
        })?;
    if !valid {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_corrupt",
            "sealed ownership receipt does not match the decoded graph topology",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn ensure_candidate_artifact_dir(artifact_root: &Path) -> Result<PathBuf, GraphIngestA2Error> {
    let dir = artifact_root.join(GRAPH_INGEST_A2_CANDIDATE_DIR);
    if !artifact_root.exists() {
        std::fs::create_dir_all(artifact_root).map_err(|error| {
            GraphIngestA2Error::new(
                "graph_ingest_candidate_artifact_directory_failed",
                error.to_string(),
            )
        })?;
    }
    let root_metadata = artifact_root.symlink_metadata().map_err(|error| {
        GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_directory_failed",
            error.to_string(),
        )
    })?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_directory_invalid",
            "configured artifact root must be a real non-symlink directory",
        ));
    }
    match dir.symlink_metadata() {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(dir),
        Ok(_) => Err(GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_directory_invalid",
            "candidate artifact directory is not a real directory",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir(&dir).map_err(|error| {
                GraphIngestA2Error::new(
                    "graph_ingest_candidate_artifact_directory_failed",
                    error.to_string(),
                )
            })?;
            File::open(artifact_root)
                .and_then(|parent| parent.sync_all())
                .map_err(|error| {
                    GraphIngestA2Error::new(
                        "graph_ingest_candidate_artifact_sync_failed",
                        error.to_string(),
                    )
                })?;
            Ok(dir)
        }
        Err(error) => Err(GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_directory_failed",
            error.to_string(),
        )),
    }
}

#[cfg(windows)]
fn ensure_candidate_artifact_dir(artifact_root: &Path) -> Result<PathBuf, GraphIngestA2Error> {
    if !artifact_root.exists() {
        std::fs::create_dir_all(artifact_root).map_err(|error| {
            GraphIngestA2Error::new(
                "graph_ingest_candidate_artifact_directory_failed",
                error.to_string(),
            )
        })?;
    }
    let root = crate::windows_durable_fs::open_directory_no_follow(artifact_root)
        .map_err(map_windows_artifact_root_error)?;
    let component = std::ffi::OsStr::new(GRAPH_INGEST_A2_CANDIDATE_DIR);
    match crate::windows_durable_fs::open_relative_directory_no_follow(&root, component) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            crate::windows_durable_fs::create_relative_directory_no_follow(&root, component)
                .map_err(|error| {
                    GraphIngestA2Error::new(
                        "graph_ingest_candidate_artifact_directory_failed",
                        error.to_string(),
                    )
                })?;
        }
        Err(error) => {
            return Err(GraphIngestA2Error::new(
                "graph_ingest_candidate_artifact_directory_invalid",
                error.to_string(),
            ));
        }
    }
    Ok(artifact_root.join(GRAPH_INGEST_A2_CANDIDATE_DIR))
}

#[cfg(not(any(unix, windows)))]
fn ensure_candidate_artifact_dir(_artifact_root: &Path) -> Result<PathBuf, GraphIngestA2Error> {
    Err(GraphIngestA2Error::new(
        "graph_ingest_candidate_artifact_nofollow_unavailable",
        "this platform has no installed durable no-follow artifact directory backend",
    ))
}

#[cfg(windows)]
struct WindowsCandidateArtifactAnchor {
    _root: File,
    directory: File,
}

#[cfg(windows)]
impl WindowsCandidateArtifactAnchor {
    fn open(artifact_root: &Path) -> Result<Self, GraphIngestA2Error> {
        let root = crate::windows_durable_fs::open_directory_no_follow(artifact_root)
            .map_err(map_windows_artifact_root_error)?;
        let directory = crate::windows_durable_fs::open_relative_directory_no_follow(
            &root,
            std::ffi::OsStr::new(GRAPH_INGEST_A2_CANDIDATE_DIR),
        )
        .map_err(|error| {
            GraphIngestA2Error::new(
                "graph_ingest_candidate_artifact_directory_invalid",
                error.to_string(),
            )
        })?;
        // Both handles carry stable by-handle identities and omit delete
        // sharing. Keep them alive together so every child open remains rooted
        // in the same namespace objects for the whole operation.
        crate::windows_durable_fs::handle_identity(&root).map_err(|error| {
            GraphIngestA2Error::new(
                "graph_ingest_candidate_artifact_directory_invalid",
                error.to_string(),
            )
        })?;
        crate::windows_durable_fs::handle_identity(&directory).map_err(|error| {
            GraphIngestA2Error::new(
                "graph_ingest_candidate_artifact_directory_invalid",
                error.to_string(),
            )
        })?;
        Ok(Self {
            _root: root,
            directory,
        })
    }
}

#[cfg(windows)]
fn map_windows_artifact_root_error(error: std::io::Error) -> GraphIngestA2Error {
    GraphIngestA2Error::new(
        "graph_ingest_candidate_artifact_directory_invalid",
        error.to_string(),
    )
}

fn constrained_candidate_artifact_path(
    artifact_root: &Path,
    reference: &GraphIngestA2CandidateArtifactRefV1,
) -> Result<PathBuf, GraphIngestA2Error> {
    let relative = Path::new(&reference.relative_path);
    let file_name = relative
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let artifact_id = file_name.strip_suffix(".candidate").unwrap_or_default();
    if reference.schema != GRAPH_INGEST_A2_CANDIDATE_ARTIFACT_REF_SCHEMA
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
        || relative.components().count() != 2
        || relative.parent() != Some(Path::new(GRAPH_INGEST_A2_CANDIDATE_DIR))
        || !is_digest(artifact_id)
        || !is_digest(&reference.artifact_sha256)
        || !is_digest(&reference.graph_snapshot_sha256)
        || !is_digest(&reference.ownership_manifest_digest)
        || !is_digest(&reference.stats_digest)
        || reference.byte_len == 0
    {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_reference_invalid",
            "candidate artifact reference is not one constrained content-addressed child",
        ));
    }
    #[cfg(unix)]
    {
        let dir = artifact_root.join(GRAPH_INGEST_A2_CANDIDATE_DIR);
        let metadata = dir.symlink_metadata().map_err(|error| {
            GraphIngestA2Error::new("graph_ingest_candidate_artifact_missing", error.to_string())
        })?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(GraphIngestA2Error::new(
                "graph_ingest_candidate_artifact_directory_invalid",
                "candidate artifact parent is not a real directory",
            ));
        }
    }
    #[cfg(windows)]
    WindowsCandidateArtifactAnchor::open(artifact_root)?;
    #[cfg(not(any(unix, windows)))]
    return Err(GraphIngestA2Error::new(
        "graph_ingest_candidate_artifact_nofollow_unavailable",
        "this platform has no installed anchored no-follow artifact reader",
    ));
    Ok(artifact_root.join(relative))
}

fn read_candidate_artifact_file(
    artifact_root: &Path,
    relative: &Path,
    expected_len: u64,
) -> Result<Vec<u8>, GraphIngestA2Error> {
    if expected_len == 0 || expected_len > GRAPH_INGEST_A2_MAX_CANDIDATE_BYTES {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_too_large",
            format!(
                "sealed candidate length {expected_len} is outside the owner limit 1..={GRAPH_INGEST_A2_MAX_CANDIDATE_BYTES}"
            ),
        ));
    }
    let file_name = relative
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            GraphIngestA2Error::new(
                "graph_ingest_candidate_artifact_reference_invalid",
                "candidate artifact file name is not valid UTF-8",
            )
        })?;
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
        || relative.components().count() != 2
        || relative.parent() != Some(Path::new(GRAPH_INGEST_A2_CANDIDATE_DIR))
        || file_name.contains('/')
        || file_name.contains('\\')
        || file_name.contains(':')
    {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_reference_invalid",
            "candidate artifact is not one direct child of the sealed directory",
        ));
    }

    let file = open_candidate_artifact_handle(artifact_root, file_name)?;
    let metadata = file.metadata().map_err(|error| {
        GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_read_failed",
            error.to_string(),
        )
    })?;
    if !metadata.is_file() || metadata.len() != expected_len {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_not_regular",
            format!(
                "opened candidate must be one regular file of sealed length {expected_len}; observed {}",
                metadata.len()
            ),
        ));
    }
    let capacity = usize::try_from(expected_len).map_err(|_| {
        GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_too_large",
            "sealed candidate length cannot fit this platform",
        )
    })?;
    #[cfg(windows)]
    let identity_before = crate::windows_durable_fs::handle_identity(&file).map_err(|error| {
        GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_read_failed",
            error.to_string(),
        )
    })?;
    let mut bytes = Vec::with_capacity(capacity);
    (&file)
        .take(expected_len.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| {
            GraphIngestA2Error::new(
                "graph_ingest_candidate_artifact_read_failed",
                error.to_string(),
            )
        })?;
    if bytes.len() as u64 != expected_len {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_corrupt",
            "candidate bytes changed while the already-open handle was read",
        ));
    }
    #[cfg(windows)]
    {
        let identity_after =
            crate::windows_durable_fs::handle_identity(&file).map_err(|error| {
                GraphIngestA2Error::new(
                    "graph_ingest_candidate_artifact_read_failed",
                    error.to_string(),
                )
            })?;
        let length_after = file.metadata().map_err(|error| {
            GraphIngestA2Error::new(
                "graph_ingest_candidate_artifact_read_failed",
                error.to_string(),
            )
        })?;
        if identity_after != identity_before || length_after.len() != expected_len {
            return Err(GraphIngestA2Error::new(
                "graph_ingest_candidate_artifact_corrupt",
                "candidate handle identity or length changed during the sealed read",
            ));
        }
    }
    Ok(bytes)
}

#[cfg(unix)]
fn open_candidate_artifact_handle(
    artifact_root: &Path,
    file_name: &str,
) -> Result<File, GraphIngestA2Error> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::OpenOptionsExt;

    // The configured artifact root is the trust anchor. O_NOFOLLOW binds its
    // final component; the candidate directory and file are then opened with
    // openat from already-open directory handles, so path replacement cannot
    // redirect the read to a symlink target.
    let mut root_options = OpenOptions::new();
    root_options
        .read(true)
        .custom_flags(libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW);
    let root = root_options.open(artifact_root).map_err(|error| {
        GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_directory_invalid",
            error.to_string(),
        )
    })?;
    let candidate_dir = CString::new(GRAPH_INGEST_A2_CANDIDATE_DIR).expect("static directory");
    // SAFETY: both C strings are NUL-free and each returned descriptor is
    // immediately owned by exactly one File.
    let dir_fd = unsafe {
        libc::openat(
            root.as_raw_fd(),
            candidate_dir.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if dir_fd < 0 {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_directory_invalid",
            std::io::Error::last_os_error().to_string(),
        ));
    }
    // SAFETY: successful openat returned a new owned descriptor.
    let dir = unsafe { File::from_raw_fd(dir_fd) };
    let file_name = CString::new(file_name.as_bytes()).map_err(|_| {
        GraphIngestA2Error::new(
            "graph_ingest_candidate_artifact_reference_invalid",
            "candidate file name contains NUL",
        )
    })?;
    // SAFETY: file_name is NUL-terminated and dir remains open for this call.
    let file_fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            file_name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if file_fd < 0 {
        let error = std::io::Error::last_os_error();
        let code = if error.raw_os_error() == Some(libc::ELOOP) {
            "graph_ingest_candidate_artifact_not_regular"
        } else if error.kind() == std::io::ErrorKind::NotFound {
            "graph_ingest_candidate_artifact_missing"
        } else {
            "graph_ingest_candidate_artifact_read_failed"
        };
        return Err(GraphIngestA2Error::new(code, error.to_string()));
    }
    // SAFETY: successful openat returned a new owned descriptor.
    Ok(unsafe { File::from_raw_fd(file_fd) })
}

#[cfg(windows)]
fn open_candidate_artifact_handle(
    artifact_root: &Path,
    file_name: &str,
) -> Result<File, GraphIngestA2Error> {
    let anchor = WindowsCandidateArtifactAnchor::open(artifact_root)?;
    crate::windows_durable_fs::open_relative_read_no_follow(
        &anchor.directory,
        std::ffi::OsStr::new(file_name),
    )
    .map_err(|error| {
        let code = if error.kind() == std::io::ErrorKind::InvalidInput {
            "graph_ingest_candidate_artifact_not_regular"
        } else if error.kind() == std::io::ErrorKind::NotFound {
            "graph_ingest_candidate_artifact_missing"
        } else {
            "graph_ingest_candidate_artifact_read_failed"
        };
        GraphIngestA2Error::new(code, error.to_string())
    })
}

#[cfg(not(any(unix, windows)))]
fn open_candidate_artifact_handle(
    _artifact_root: &Path,
    _file_name: &str,
) -> Result<File, GraphIngestA2Error> {
    // Do not emulate no-follow with metadata-then-open: Windows reparse points
    // can be swapped in that gap. Until this backend owns an anchored
    // CreateFileW(FILE_FLAG_OPEN_REPARSE_POINT) chain, recovery is unavailable
    // rather than silently weakening the immutable-candidate guarantee.
    Err(GraphIngestA2Error::new(
        "graph_ingest_candidate_artifact_nofollow_unavailable",
        "this platform has no installed anchored no-follow artifact reader",
    ))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    super::hex_lower_bytes(&Sha256::digest(bytes))
}

pub(super) fn capture_inspection_snapshot(
    state: &SessionState,
    input: &GraphIngestA2InputV1,
    mode: GraphIngestA2ModeV1,
    entries: &[ExternalMutationJournalEntryV1],
    brain_id: &str,
    reconciliation_brain_id: String,
    authority_subject_id: String,
) -> Result<GraphIngestA2InspectionSnapshotV1, GraphIngestA2Error> {
    input.validate_shape(mode)?;
    let root_identity = canonical_root(&input.root)?;
    let selected_root = selected_code_root(state).ok_or_else(|| {
        GraphIngestA2Error::new(
            "graph_ingest_selected_root_missing",
            "selected brain has no bound code root",
        )
    })?;
    if root_identity != selected_root {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_root_mismatch",
            format!(
                "request root {root_identity:?} differs from selected brain root {selected_root:?}"
            ),
        ));
    }
    if state.graph_generation != input.expected_graph_generation {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_stale_generation",
            format!(
                "expected graph generation {}, observed {}",
                input.expected_graph_generation, state.graph_generation
            ),
        ));
    }
    let observed_projection = m1nd_ingest::ownership::source_projection_digest(&state.graph.read())
        .map_err(|error| {
            GraphIngestA2Error::new("graph_ingest_live_projection_invalid", error.to_string())
        })?;
    if observed_projection != input.expected_source_projection_digest {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_stale_projection",
            "live source projection differs from the authority-bound preimage",
        ));
    }

    let baseline = if mode == GraphIngestA2ModeV1::MergeExisting {
        let parent = input.parent.as_ref().expect("shape gate requires parent");
        verify_parent(entries, parent, brain_id)?;
        let baseline = latest_replace_baseline(entries, brain_id, &root_identity)?;
        verify_merge_live_baseline(state, &baseline, &root_identity)?;
        Some(baseline)
    } else {
        None
    };

    Ok(GraphIngestA2InspectionSnapshotV1 {
        preview_id: input.preview_id.clone(),
        mode,
        root_identity,
        expected_graph_generation: input.expected_graph_generation,
        expected_source_projection_digest: input.expected_source_projection_digest.clone(),
        include_dotfiles: input.include_dotfiles,
        dotfile_patterns: input.dotfile_patterns.clone(),
        parent: input.parent.clone(),
        baseline,
        reconciliation_brain_id,
        authority_subject_id,
    })
}

pub(super) fn complete_inspection_off_actor(
    snapshot: GraphIngestA2InspectionSnapshotV1,
) -> Result<InspectedGraphIngestA2V1, GraphIngestA2Error> {
    complete_inspection_off_actor_with_cancel(snapshot, || false)
}

pub(super) fn complete_inspection_off_actor_with_cancel<Cancelled>(
    snapshot: GraphIngestA2InspectionSnapshotV1,
    is_cancelled: Cancelled,
) -> Result<InspectedGraphIngestA2V1, GraphIngestA2Error>
where
    Cancelled: Fn() -> bool + Sync,
{
    let bundle = build_complete_bundle_with_cancel(
        &snapshot.root_identity,
        snapshot.include_dotfiles,
        &snapshot.dotfile_patterns,
        is_cancelled,
    )?;
    if bundle.ownership.root_identity != snapshot.root_identity
        || bundle.ownership.exact_source_key.is_some()
        || bundle.ownership.base_ownership_digest.is_some()
    {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_full_root_required",
            "A2 accepts only a full-root code bundle; exact/incremental bundles are forbidden",
        ));
    }

    if let Some(baseline) = snapshot.baseline.as_ref() {
        verify_merge_candidate_baseline(baseline, &bundle.ownership)?;
    }

    let semantic_payload = GraphIngestA2SemanticPayloadV1 {
        schema: GRAPH_INGEST_A2_PAYLOAD_SCHEMA.to_string(),
        mode: snapshot.mode,
        root_identity: snapshot.root_identity,
        expected_graph_generation: snapshot.expected_graph_generation,
        expected_source_projection_digest: snapshot.expected_source_projection_digest,
        include_dotfiles: snapshot.include_dotfiles,
        dotfile_patterns: snapshot.dotfile_patterns,
        parent: snapshot.parent,
        candidate_ownership_digest: bundle.ownership.ownership_digest.clone(),
        candidate_source_projection_digest: bundle.ownership.source_projection_digest.clone(),
        candidate_pipeline_digest: bundle.ownership.pipeline_digest.clone(),
    };
    let semantic_payload_digest =
        m1nd_control::digest_canonical(GRAPH_INGEST_A2_PAYLOAD_DIGEST_DOMAIN, &semantic_payload)
            .map_err(|error| {
                GraphIngestA2Error::new("graph_ingest_digest_failed", error.to_string())
            })?;
    Ok(InspectedGraphIngestA2V1 {
        preview_id: snapshot.preview_id,
        semantic_payload,
        semantic_payload_digest,
        ownership_manifest: bundle.ownership.clone(),
        reconciliation_brain_id: snapshot.reconciliation_brain_id,
        authority_subject_id: snapshot.authority_subject_id,
        candidate: bundle,
    })
}

pub(super) fn owner_derived_input(
    state: &SessionState,
    preview_id: String,
    include_dotfiles: bool,
    dotfile_patterns: Vec<String>,
    parent: Option<GraphIngestA2ParentV1>,
) -> Result<GraphIngestA2InputV1, GraphIngestA2Error> {
    let root = selected_code_root(state).ok_or_else(|| {
        GraphIngestA2Error::new(
            "graph_ingest_selected_root_missing",
            "selected brain has no bound code root",
        )
    })?;
    let expected_source_projection_digest =
        m1nd_ingest::ownership::source_projection_digest(&state.graph.read()).map_err(|error| {
            GraphIngestA2Error::new("graph_ingest_live_projection_invalid", error.to_string())
        })?;
    Ok(GraphIngestA2InputV1 {
        preview_id,
        root,
        expected_graph_generation: state.graph_generation,
        expected_source_projection_digest,
        include_dotfiles,
        dotfile_patterns,
        parent,
    })
}

pub(super) fn stage(
    inspected: InspectedGraphIngestA2V1,
    operation_object_digest: &str,
    artifact_root: &Path,
    reservation_id: &str,
) -> Result<StagedGraphIngestA2V1, GraphIngestA2Error> {
    let artifact_root = absolute_artifact_root(artifact_root)?;
    let outcome_digest = m1nd_control::digest_canonical(
        GRAPH_INGEST_A2_OUTCOME_DIGEST_DOMAIN,
        &(
            operation_object_digest,
            inspected.semantic_payload.mode,
            inspected.semantic_payload.root_identity.as_str(),
            inspected.ownership_manifest.ownership_digest.as_str(),
            inspected
                .ownership_manifest
                .source_projection_digest
                .as_str(),
            inspected.semantic_payload.parent.as_ref(),
        ),
    )
    .map_err(|error| GraphIngestA2Error::new("graph_ingest_digest_failed", error.to_string()))?;
    let candidate_artifact = persist_candidate_artifact(
        &artifact_root,
        reservation_id,
        operation_object_digest,
        inspected.candidate,
    )?;
    Ok(StagedGraphIngestA2V1 {
        preview_id: inspected.preview_id,
        semantic_payload: inspected.semantic_payload,
        ownership_manifest: inspected.ownership_manifest,
        operation_object_digest: operation_object_digest.to_string(),
        outcome_digest,
        reconciliation_brain_id: inspected.reconciliation_brain_id,
        authority_subject_id: inspected.authority_subject_id,
        candidate_artifact,
        artifact_root,
    })
}

pub(super) fn from_recovery(
    recovery: GraphIngestA2RecoveryPayloadV1,
    entry: &ExternalMutationJournalEntryV1,
    artifact_root: &Path,
) -> Result<StagedGraphIngestA2V1, GraphIngestA2Error> {
    let artifact_root = absolute_artifact_root(artifact_root)?;
    if recovery.kind != GRAPH_INGEST_A2_RECOVERY_KIND
        || !recovery.forward_complete
        || !is_digest(&recovery.preview_id)
        || recovery.semantic_payload.schema != GRAPH_INGEST_A2_PAYLOAD_SCHEMA
        || recovery.semantic_payload.mode.semantic_action() != entry.prepare.semantic_action
        || recovery.operation_object_digest != entry.prepare.operation_object_digest
        || recovery.outcome_digest != entry.outcome_digest.as_deref().unwrap_or_default()
        || recovery.ownership_manifest.ownership_digest
            != recovery.semantic_payload.candidate_ownership_digest
        || recovery.ownership_manifest.source_projection_digest
            != recovery.semantic_payload.candidate_source_projection_digest
        || recovery.ownership_manifest.pipeline_digest
            != recovery.semantic_payload.candidate_pipeline_digest
        || recovery.candidate_artifact.schema != GRAPH_INGEST_A2_CANDIDATE_ARTIFACT_REF_SCHEMA
    {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_recovery_binding_mismatch",
            "durable A2 recovery payload differs from the committed journal bindings",
        ));
    }
    let staged = StagedGraphIngestA2V1 {
        preview_id: recovery.preview_id,
        semantic_payload: recovery.semantic_payload,
        ownership_manifest: recovery.ownership_manifest,
        operation_object_digest: recovery.operation_object_digest,
        outcome_digest: recovery.outcome_digest,
        reconciliation_brain_id: recovery.reconciliation_brain_id,
        authority_subject_id: recovery.authority_subject_id,
        candidate_artifact: recovery.candidate_artifact,
        artifact_root,
    };
    // Boot recovery validates the exact immutable bytes before any actor job is
    // admitted. The actor handshake loads them again immediately before use;
    // this first check makes missing/corrupt WAL dependencies a boot barrier.
    staged.load_durable_candidate(false)?;
    Ok(staged)
}

fn absolute_artifact_root(artifact_root: &Path) -> Result<PathBuf, GraphIngestA2Error> {
    if artifact_root.is_absolute() {
        return Ok(artifact_root.to_path_buf());
    }
    std::env::current_dir()
        .map(|current| current.join(artifact_root))
        .map_err(|error| {
            GraphIngestA2Error::new(
                "graph_ingest_candidate_artifact_directory_failed",
                error.to_string(),
            )
        })
}

pub(super) fn request_matches_entry(
    input: &GraphIngestA2InputV1,
    mode: GraphIngestA2ModeV1,
    entry: &ExternalMutationJournalEntryV1,
) -> Result<bool, GraphIngestA2Error> {
    let recovery: GraphIngestA2RecoveryPayloadV1 =
        serde_json::from_value(entry.prepare.recovery_payload.clone()).map_err(|error| {
            GraphIngestA2Error::new("graph_ingest_recovery_payload_invalid", error.to_string())
        })?;
    let root_identity = canonical_root(&input.root)?;
    Ok(recovery.kind == GRAPH_INGEST_A2_RECOVERY_KIND
        && recovery.semantic_payload.mode == mode
        && recovery.preview_id == input.preview_id
        && recovery.semantic_payload.root_identity == root_identity
        && recovery.semantic_payload.expected_graph_generation == input.expected_graph_generation
        && recovery.semantic_payload.expected_source_projection_digest
            == input.expected_source_projection_digest
        && recovery.semantic_payload.include_dotfiles == input.include_dotfiles
        && recovery.semantic_payload.dotfile_patterns == input.dotfile_patterns
        && recovery.semantic_payload.parent == input.parent)
}

pub(super) fn selected_actor_id(state: &SessionState) -> String {
    let identity = state
        .workspace_root
        .clone()
        .or_else(|| state.ingest_roots.first().cloned())
        .unwrap_or_else(|| state.runtime_root.to_string_lossy().into_owned());
    crate::brain_runtime::project_brain_id(&format!("bound:{identity}"))
}

fn build_complete_bundle(
    root_identity: &str,
    include_dotfiles: bool,
    dotfile_patterns: &[String],
) -> Result<m1nd_ingest::CodeIngestBundleV1, GraphIngestA2Error> {
    build_complete_bundle_with_cancel(root_identity, include_dotfiles, dotfile_patterns, || false)
}

fn build_complete_bundle_with_cancel<Cancelled>(
    root_identity: &str,
    include_dotfiles: bool,
    dotfile_patterns: &[String],
    is_cancelled: Cancelled,
) -> Result<m1nd_ingest::CodeIngestBundleV1, GraphIngestA2Error>
where
    Cancelled: Fn() -> bool + Sync,
{
    let bundle = Ingestor::new(IngestConfig {
        root: PathBuf::from(root_identity),
        include_dotfiles,
        dotfile_patterns: dotfile_patterns.to_vec(),
        ..IngestConfig::default()
    })
    .ingest_bundle_with_cancel(&is_cancelled)
    .map_err(|error| match error {
        m1nd_core::error::M1ndError::IngestionCancelled => GraphIngestA2Error::new(
            "graph_ingest_scan_cancelled",
            "owner runtime cancelled graph candidate ingestion",
        ),
        error => GraphIngestA2Error::new("graph_ingest_candidate_failed", error.to_string()),
    })?;
    bundle
        .require_complete_with_cancel(&is_cancelled)
        .map_err(|error| match error {
            m1nd_core::error::M1ndError::IngestionCancelled => GraphIngestA2Error::new(
                "graph_ingest_scan_cancelled",
                "owner runtime cancelled graph candidate completion validation",
            ),
            error => {
                GraphIngestA2Error::new("graph_ingest_incomplete_candidate", error.to_string())
            }
        })?;
    Ok(bundle)
}

fn require_same_candidate(
    observed: &CodeOwnershipManifestV1,
    expected: &CodeOwnershipManifestV1,
) -> Result<(), GraphIngestA2Error> {
    if observed.coverage != OwnershipCoverageV1::Complete
        || observed.root_identity != expected.root_identity
        || observed.exact_source_key.is_some()
        || observed.ownership_digest != expected.ownership_digest
        || observed.source_projection_digest != expected.source_projection_digest
        || observed.pipeline_digest != expected.pipeline_digest
    {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_candidate_drift",
            "full-root candidate changed after authority inspection",
        ));
    }
    Ok(())
}

fn verify_parent(
    entries: &[ExternalMutationJournalEntryV1],
    parent: &GraphIngestA2ParentV1,
    brain_id: &str,
) -> Result<(), GraphIngestA2Error> {
    let entry = entries
        .iter()
        .find(|entry| entry.operation_id == parent.operation_id)
        .ok_or_else(|| {
            GraphIngestA2Error::new(
                "graph_ingest_parent_not_found",
                "source.edit parent is absent from the protected external journal",
            )
        })?;
    let exact = entry.phase == ExternalMutationJournalPhaseV1::Published
        && entry.prepare.semantic_action == "source.edit.commit"
        && entry.prepare.actor_brain_id == brain_id
        && entry.lease_id == parent.lease_id
        && entry.reservation_id == parent.reservation_id
        && entry.prepare.operation_object_digest == parent.operation_object_digest
        && entry.prepare.payload_digest == parent.semantic_payload_digest
        && entry.outcome_digest.as_deref() == Some(parent.outcome_digest.as_str())
        && entry.published_result_digest.as_deref()
            == Some(parent.published_result_digest.as_str())
        && entry
            .published_result
            .as_ref()
            .is_some_and(|result| result.graph_resync_required);
    if !exact {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_parent_binding_mismatch",
            "merge_existing parent is not the exact PUBLISHED source.edit operation on this brain",
        ));
    }
    let latest = entries
        .iter()
        .filter(|candidate| {
            candidate.phase == ExternalMutationJournalPhaseV1::Published
                && candidate.prepare.semantic_action == "source.edit.commit"
                && candidate.prepare.actor_brain_id == brain_id
        })
        .max_by_key(|candidate| (candidate.updated_at, candidate.operation_id.as_str()));
    if latest.map(|candidate| candidate.operation_id.as_str()) != Some(parent.operation_id.as_str())
    {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_parent_superseded",
            "merge_existing must bind the latest PUBLISHED source.edit child obligation",
        ));
    }
    Ok(())
}

fn latest_replace_baseline(
    entries: &[ExternalMutationJournalEntryV1],
    brain_id: &str,
    root_identity: &str,
) -> Result<CodeOwnershipManifestV1, GraphIngestA2Error> {
    entries
        .iter()
        .filter(|entry| {
            entry.phase == ExternalMutationJournalPhaseV1::Published
                && entry.prepare.semantic_action == "graph.ingest.replace"
                && entry.prepare.actor_brain_id == brain_id
        })
        .filter_map(|entry| {
            let recovery: GraphIngestA2RecoveryPayloadV1 =
                serde_json::from_value(entry.prepare.recovery_payload.clone()).ok()?;
            (recovery.kind == GRAPH_INGEST_A2_RECOVERY_KIND
                && recovery.semantic_payload.root_identity == root_identity)
                .then_some((entry.updated_at, entry.operation_id.as_str(), recovery))
        })
        .max_by_key(|(updated_at, operation_id, _)| (*updated_at, *operation_id))
        .map(|(_, _, recovery)| recovery.ownership_manifest)
        .ok_or_else(|| {
            GraphIngestA2Error::new(
                "graph_ingest_baseline_missing",
                "merge_existing requires a prior PUBLISHED sovereign replace baseline",
            )
        })
}

fn verify_merge_live_baseline(
    state: &SessionState,
    baseline: &CodeOwnershipManifestV1,
    root_identity: &str,
) -> Result<(), GraphIngestA2Error> {
    if baseline.coverage != OwnershipCoverageV1::Complete
        || baseline.root_identity != root_identity
        || baseline.exact_source_key.is_some()
        || baseline.base_ownership_digest.is_some()
        || !baseline.verify_receipt().map_err(|error| {
            GraphIngestA2Error::new("graph_ingest_baseline_invalid", error.to_string())
        })?
    {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_baseline_untrustworthy",
            "replace baseline is not an exact full-root COMPLETE ownership receipt",
        ));
    }

    let owned = baseline
        .claims_by_source
        .values()
        .flat_map(|claims| claims.node_ids.iter().cloned())
        .collect::<BTreeSet<_>>();
    let graph = state.graph.read();
    let foreign = graph
        .id_to_node
        .iter()
        .filter_map(|(external_id, &node_id)| {
            let external_id = graph.strings.resolve(*external_id).to_string();
            let provenance = graph.resolve_node_provenance(node_id);
            let source_owned = external_id.starts_with("file::")
                || provenance.source_path.as_deref().is_some_and(|path| {
                    let path = Path::new(path);
                    path.is_absolute() && path.starts_with(root_identity)
                });
            (source_owned && !owned.contains(&external_id)).then_some(external_id)
        })
        .take(8)
        .collect::<Vec<_>>();
    if !foreign.is_empty() {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_foreign_nodes",
            format!("live code projection contains foreign nodes: {foreign:?}"),
        ));
    }
    Ok(())
}

fn verify_merge_candidate_baseline(
    baseline: &CodeOwnershipManifestV1,
    candidate: &CodeOwnershipManifestV1,
) -> Result<(), GraphIngestA2Error> {
    let base_pipeline = &baseline.pipeline_receipt;
    let next_pipeline = &candidate.pipeline_receipt;
    if base_pipeline.include_dotfiles != next_pipeline.include_dotfiles
        || base_pipeline.dotfile_patterns != next_pipeline.dotfile_patterns
        || base_pipeline.skip_dirs != next_pipeline.skip_dirs
        || base_pipeline.skip_files != next_pipeline.skip_files
        || base_pipeline.policy_fingerprint != next_pipeline.policy_fingerprint
        || base_pipeline.build_features != next_pipeline.build_features
        || base_pipeline.binary_policy != next_pipeline.binary_policy
        || base_pipeline.producer_name != next_pipeline.producer_name
        || base_pipeline.producer_version != next_pipeline.producer_version
        || base_pipeline.producer_build_identity != next_pipeline.producer_build_identity
        || base_pipeline.producer_executable_identity != next_pipeline.producer_executable_identity
    {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_discovery_controls_changed",
            "merge_existing refuses changed discovery/build controls; sovereign replace is required",
        ));
    }

    Ok(())
}

fn selected_code_root(state: &SessionState) -> Option<String> {
    state
        .workspace_root
        .as_deref()
        .or_else(|| state.ingest_roots.first().map(String::as_str))
        .and_then(|root| canonical_root(root).ok())
}

fn canonical_root(root: &str) -> Result<String, GraphIngestA2Error> {
    let canonical = PathBuf::from(root).canonicalize().map_err(|error| {
        GraphIngestA2Error::new("graph_ingest_root_unavailable", error.to_string())
    })?;
    if !canonical.is_dir() {
        return Err(GraphIngestA2Error::new(
            "graph_ingest_full_root_required",
            "A2 code ingestion requires a canonical directory root",
        ));
    }
    // The identity MUST match `CodeOwnershipManifestV1.root_identity` byte-for-byte
    // or a full-root bundle looks foreign (on Windows `canonicalize` yields
    // `\\?\C:\...`, which ingest normalizes to `//?/C:/...`). Reuse ingest's own
    // normalizer instead of re-deriving it here so the two can never drift.
    m1nd_ingest::exact_path_identity(&canonical).map_err(|error| {
        GraphIngestA2Error::new("graph_ingest_root_unavailable", error.to_string())
    })
}

fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// A discovery dotfile pattern must be a non-empty, whitespace-trimmed, RELATIVE
/// pattern. `Path::is_absolute` alone is platform-dependent: on Windows a
/// leading-separator (`/x`, `\x`) or drive-relative root (`C:\x`) is NOT reported
/// as absolute, so an operator-supplied pattern could escape the scanned root on
/// Windows while being refused on POSIX. This rejects the leading-separator and
/// drive-letter forms on every OS, mirroring the separator/drive rules
/// `m1nd_ingest::is_valid_relative_file_path` already enforces for source keys.
fn is_safe_relative_discovery_pattern(pattern: &str) -> bool {
    !pattern.is_empty()
        && pattern == pattern.trim()
        && !pattern.starts_with('/')
        && !pattern.starts_with('\\')
        && pattern.as_bytes().get(1) != Some(&b':')
        && !Path::new(pattern).is_absolute()
        && !pattern
            .split(['/', '\\'])
            .any(|segment| segment == "." || segment == "..")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GraphIngestA2Error {
    pub code: &'static str,
    pub detail: String,
}

impl GraphIngestA2Error {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(byte: char) -> String {
        std::iter::repeat_n(byte, 64).collect()
    }

    fn complete_parent() -> GraphIngestA2ParentV1 {
        GraphIngestA2ParentV1 {
            operation_id: "source-operation".to_string(),
            lease_id: "source-lease".to_string(),
            reservation_id: "source-reservation".to_string(),
            operation_object_digest: digest('a'),
            semantic_payload_digest: digest('b'),
            outcome_digest: digest('c'),
            published_result_digest: digest('d'),
        }
    }

    fn input(parent: Option<GraphIngestA2ParentV1>) -> GraphIngestA2InputV1 {
        GraphIngestA2InputV1 {
            preview_id: digest('f'),
            root: "/project".to_string(),
            expected_graph_generation: 7,
            expected_source_projection_digest: digest('e'),
            include_dotfiles: false,
            dotfile_patterns: Vec::new(),
            parent,
        }
    }

    #[test]
    fn a2_wire_shape_keeps_replace_sovereign_and_merge_causally_bound() {
        input(None)
            .validate_shape(GraphIngestA2ModeV1::Replace)
            .expect("replace without parent");
        assert_eq!(
            input(Some(complete_parent()))
                .validate_shape(GraphIngestA2ModeV1::Replace)
                .expect_err("replace parent must refuse")
                .code,
            "graph_ingest_replace_parent_forbidden"
        );
        assert_eq!(
            input(None)
                .validate_shape(GraphIngestA2ModeV1::MergeExisting)
                .expect_err("merge parent is mandatory")
                .code,
            "graph_ingest_parent_required"
        );
        input(Some(complete_parent()))
            .validate_shape(GraphIngestA2ModeV1::MergeExisting)
            .expect("merge with complete parent");

        let mut incomplete = complete_parent();
        incomplete.outcome_digest.clear();
        assert_eq!(
            input(Some(incomplete))
                .validate_shape(GraphIngestA2ModeV1::MergeExisting)
                .expect_err("partial parent must refuse")
                .code,
            "graph_ingest_parent_binding_invalid"
        );
    }

    #[test]
    fn a2_discovery_controls_are_trimmed_relative_and_unique() {
        for patterns in [
            vec!["".to_string()],
            vec![" .env".to_string()],
            vec!["/absolute".to_string()],
            // Windows-rooted forms that `Path::is_absolute` does NOT classify as
            // absolute on POSIX: a leading backslash, a drive-letter root, and a
            // verbatim prefix must all be refused on every OS so an operator
            // pattern can never escape the scanned root.
            vec!["\\rooted".to_string()],
            vec!["C:\\drive".to_string()],
            vec!["\\\\?\\C:\\verbatim".to_string()],
            // Dot segments must be refused on every OS (mirror of
            // `is_valid_relative_file_path`): a future consumer that joins a
            // pattern to the root must never inherit a traversal vector.
            vec!["..".to_string()],
            vec!["../escape".to_string()],
            vec!["a/../b".to_string()],
            vec!["a\\..\\b".to_string()],
            vec![".".to_string()],
            vec![".env".to_string(), ".env".to_string()],
        ] {
            let mut request = input(None);
            request.dotfile_patterns = patterns;
            assert_eq!(
                request
                    .validate_shape(GraphIngestA2ModeV1::Replace)
                    .expect_err("unsafe discovery controls must refuse")
                    .code,
                "graph_ingest_discovery_controls_invalid"
            );
        }
    }

    #[test]
    fn canonical_root_identity_matches_ingested_bundle_root_identity() {
        // Admission compares `bundle.ownership.root_identity` against the
        // request's `canonical_root(...)`. That only holds cross-platform when
        // canonical_root normalizes the path exactly like the ingest crate: on
        // Windows `canonicalize` returns `\\?\C:\...`, which ingest stamps as
        // `//?/C:/...`. Before the fix canonical_root kept the backslashes, so
        // every full-root A2 operation was misread as foreign and refused with
        // `graph_ingest_full_root_required`.
        let temp = tempfile::tempdir().expect("A2 identity root");
        std::fs::write(temp.path().join("lib.rs"), "pub fn a2() {}\n").expect("A2 source");
        let raw_root = temp
            .path()
            .canonicalize()
            .expect("canonical root")
            .to_string_lossy()
            .into_owned();

        let identity = canonical_root(&raw_root).expect("canonical identity");
        let bundle = build_complete_bundle(&identity, false, &[]).expect("full-root bundle");
        assert_eq!(
            identity, bundle.ownership.root_identity,
            "canonical_root must match the identity ingest stamps into the bundle"
        );
    }

    #[test]
    fn incomplete_or_incremental_candidate_can_never_pass_revalidation() {
        let temp = tempfile::tempdir().expect("A2 candidate root");
        std::fs::write(temp.path().join("lib.rs"), "pub fn a2() {}\n")
            .expect("A2 candidate source");
        let root = temp.path().canonicalize().expect("canonical root");
        let root = root.to_string_lossy().into_owned();
        let bundle = build_complete_bundle(&root, false, &[]).expect("complete candidate");
        assert_eq!(bundle.ownership.coverage, OwnershipCoverageV1::Complete);

        let mut incomplete = bundle.ownership.clone();
        incomplete.coverage = OwnershipCoverageV1::Incomplete;
        assert_eq!(
            require_same_candidate(&incomplete, &bundle.ownership)
                .expect_err("incomplete candidate must refuse")
                .code,
            "graph_ingest_candidate_drift"
        );

        let mut exact = bundle.ownership.clone();
        exact.exact_source_key = Some("lib.rs".to_string());
        assert_eq!(
            require_same_candidate(&exact, &bundle.ownership)
                .expect_err("exact-file candidate must refuse")
                .code,
            "graph_ingest_candidate_drift"
        );
    }

    #[cfg(unix)]
    #[test]
    fn candidate_reader_refuses_swapped_symlink_and_oversize_before_allocation() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("artifact root");
        let candidate_dir = temp.path().join(GRAPH_INGEST_A2_CANDIDATE_DIR);
        std::fs::create_dir(&candidate_dir).expect("candidate directory");
        let file_name = format!("{}.candidate", digest('a'));
        let relative = Path::new(GRAPH_INGEST_A2_CANDIDATE_DIR).join(&file_name);
        let outside = temp.path().join("outside-bytes");
        std::fs::write(&outside, b"evil").expect("outside bytes");
        symlink(&outside, candidate_dir.join(&file_name)).expect("swapped symlink");
        let error = read_candidate_artifact_file(temp.path(), &relative, 4)
            .expect_err("O_NOFOLLOW must refuse a swapped symlink");
        assert_eq!(error.code, "graph_ingest_candidate_artifact_not_regular");

        let oversized_name = format!("{}.candidate", digest('b'));
        let oversized_relative = Path::new(GRAPH_INGEST_A2_CANDIDATE_DIR).join(&oversized_name);
        File::create(candidate_dir.join(&oversized_name))
            .and_then(|file| file.set_len(GRAPH_INGEST_A2_MAX_CANDIDATE_BYTES + 1))
            .expect("sparse oversized candidate");
        let error = read_candidate_artifact_file(
            temp.path(),
            &oversized_relative,
            GRAPH_INGEST_A2_MAX_CANDIDATE_BYTES + 1,
        )
        .expect_err("oversized WAL reference must fail before allocation");
        assert_eq!(error.code, "graph_ingest_candidate_artifact_too_large");
    }

    #[cfg(windows)]
    #[test]
    fn candidate_reader_uses_anchored_windows_handles_and_refuses_unsafe_components() {
        let temp = tempfile::tempdir().expect("artifact root");
        let candidate_dir = temp.path().join(GRAPH_INGEST_A2_CANDIDATE_DIR);
        std::fs::create_dir(&candidate_dir).expect("candidate directory");
        let file_name = format!("{}.candidate", digest('a'));
        let relative = Path::new(GRAPH_INGEST_A2_CANDIDATE_DIR).join(&file_name);
        std::fs::write(candidate_dir.join(&file_name), b"safe").expect("candidate bytes");
        assert_eq!(
            read_candidate_artifact_file(temp.path(), &relative, 4).expect("anchored read"),
            b"safe"
        );

        let traversal = Path::new(GRAPH_INGEST_A2_CANDIDATE_DIR)
            .join("..")
            .join(&file_name);
        let error = read_candidate_artifact_file(temp.path(), &traversal, 4)
            .expect_err("parent traversal must be rejected before open");
        assert_eq!(
            error.code,
            "graph_ingest_candidate_artifact_reference_invalid"
        );

        let root =
            crate::windows_durable_fs::open_directory_no_follow(temp.path()).expect("root handle");
        let error = crate::windows_durable_fs::open_relative_directory_no_follow(
            &root,
            std::ffi::OsStr::new(".."),
        )
        .expect_err("anchored helper must reject parent components");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[cfg(windows)]
    #[test]
    fn candidate_reader_refuses_windows_reparse_file_when_symlinks_are_available() {
        use std::os::windows::fs::symlink_file;

        let temp = tempfile::tempdir().expect("artifact root");
        let candidate_dir = temp.path().join(GRAPH_INGEST_A2_CANDIDATE_DIR);
        std::fs::create_dir(&candidate_dir).expect("candidate directory");
        let outside = temp.path().join("outside-bytes");
        std::fs::write(&outside, b"evil").expect("outside bytes");
        let file_name = format!("{}.candidate", digest('b'));
        match symlink_file(&outside, candidate_dir.join(&file_name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
            Err(error) => panic!("create Windows reparse fixture: {error}"),
        }
        let relative = Path::new(GRAPH_INGEST_A2_CANDIDATE_DIR).join(file_name);
        let error = read_candidate_artifact_file(temp.path(), &relative, 4)
            .expect_err("FILE_OPEN_REPARSE_POINT must refuse the link itself");
        assert_eq!(error.code, "graph_ingest_candidate_artifact_not_regular");
    }

    #[cfg(not(any(unix, windows)))]
    #[test]
    fn candidate_reader_fails_closed_without_anchored_nofollow_backend() {
        let relative =
            Path::new(GRAPH_INGEST_A2_CANDIDATE_DIR).join(format!("{}.candidate", digest('a')));
        let error = read_candidate_artifact_file(Path::new("."), &relative, 1)
            .expect_err("unsupported no-follow assurance must fail closed");
        assert_eq!(
            error.code,
            "graph_ingest_candidate_artifact_nofollow_unavailable"
        );
    }
}
