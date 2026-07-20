//! Owner-side composition of the M1ND-10 G1 truth spine.
//!
//! The manifest is a read-only projection. Every value comes from an existing
//! authority (VCS, running binary, graph snapshot, SystemBlock store, embedded
//! UI, or a later control-plane store). Missing authorities remain unavailable;
//! this module never fills a gap with a persuasive-looking default.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use m1nd_control::{
    digest_canonical, digest_domain_bytes, ArchitectureFact, AuthorityFact, AuthorityFreshness,
    AuthorityStatus, AutonomyFact, CapabilitiesFact, GraphFact, ManifestVerification,
    OpaqueSignature, OrganismManifestV1, ReleaseProvenanceFact, RuntimeFact, SchemasFact,
    SourceFact, UiFact, ARCHITECTURE_AUTHORITY_ID, GRAPH_AUTHORITY_ID, ORGANISM_MANIFEST_SCHEMA,
    RELEASE_AUTHORITY_ID, RUNTIME_BINARY_AUTHORITY_ID, SOURCE_AUTHORITY_ID, UI_BUNDLE_AUTHORITY_ID,
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::autonomy_manifest::AutonomyManifestProjectionV1;
use crate::session::{SessionState, BINARY_VERSION};
use crate::system_blocks::{SeedSkeletonState, SystemBlockStore};
use crate::ui_attestation::{UiBundleAttestor, UiBundleObservation};

pub const MANIFEST_RESPONSE_SCHEMA: &str = "m1nd-organism-manifest-response-v1";
const ROOT_FINGERPRINT_DOMAIN: &str = "m1nd-project-root-fingerprint-v1";
const SKELETON_DIGEST_DOMAIN: &str = "m1nd-system-block-skeleton-v1";

/// The endpoint response keeps verification alongside, rather than inserting
/// non-schema fields into `OrganismManifestV1`.
#[derive(Clone, Debug, Serialize)]
pub struct ManifestResponseV1 {
    pub schema: &'static str,
    pub manifest: OrganismManifestV1,
    pub verification: ManifestVerification,
}

/// A point-in-time copy of every fact needed by the pure composer.
///
/// This split makes drift and failure modes testable without booting a server.
#[derive(Clone, Debug)]
pub struct ManifestSourceSnapshot {
    pub observed_at: u64,
    pub organism_id: String,
    pub repo_id: String,
    pub brain_id: String,
    pub project_root_fingerprint: String,
    pub source_commit: String,
    pub source_dirty: bool,
    pub source_version: String,
    pub owner_id: String,
    pub binary_version: String,
    pub binary_sha256: String,
    pub binary_build_source_commit: String,
    pub binary_build_source_dirty: bool,
    pub started_at: u64,
    pub graph_generation: u64,
    pub graph_snapshot_sha256: String,
    pub node_count: u64,
    pub edge_count: u64,
    pub architecture_store_version: Option<u64>,
    pub skeleton_digest: String,
    pub ratification_state: String,
    pub ui_bundle_version: String,
    pub ui_bundle_sha256: String,
    pub ui_mode: String,
    pub ui_status: AuthorityStatus,
    pub ui_freshness: AuthorityFreshness,
}

/// Cheap copy taken while the owner session mutex is held. It contains paths
/// and in-memory counters only; hashing and VCS subprocesses happen later.
#[derive(Clone, Debug)]
pub struct ManifestCaptureSeed {
    pub observed_at: u64,
    pub project_root_candidate: Option<PathBuf>,
    pub runtime_root: PathBuf,
    pub graph_path: PathBuf,
    pub owner_id: String,
    pub started_at: u64,
    pub graph_generation: u64,
    pub last_persist_offset_ns: Option<u128>,
    pub node_count: u64,
    pub edge_count: u64,
}

/// Clone-only half of manifest capture. It is safe to obtain through a short
/// brain actor read; graph counting and filesystem identity probes happen only
/// after the SessionState guard has been released.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ManifestCaptureParts {
    observed_at: u64,
    ingest_roots: Vec<String>,
    workspace_root: Option<String>,
    runtime_root: PathBuf,
    graph_path: PathBuf,
    owner_id: String,
    started_at: u64,
    graph_generation: u64,
    last_persist_offset_ns: Option<u128>,
    node_count: u64,
    edge_count: u64,
}

/// Verify that the generation/counts and persisted bytes used for the graph
/// authority all belonged to one stable owner snapshot. The caller captures a
/// seed before hashing the graph file and another after hashing it. Any owner-
/// mediated graph mutation or persist changes at least one of these fields, so
/// the endpoint can refuse instead of publishing a hybrid authority fact.
pub fn ensure_graph_authority_basis_stable(
    before: &ManifestCaptureSeed,
    after: &ManifestCaptureSeed,
) -> Result<(), String> {
    let persisted_during_observation =
        before.last_persist_offset_ns != after.last_persist_offset_ns;
    if before.graph_path == after.graph_path
        && before.graph_generation == after.graph_generation
        && before.node_count == after.node_count
        && before.edge_count == after.edge_count
        && !persisted_during_observation
    {
        return Ok(());
    }

    Err(format!(
        "graph authority changed while its snapshot digest was observed \
         (generation {} -> {}, nodes {} -> {}, edges {} -> {}, \
         graph_path_changed={}, persisted_during_observation={})",
        before.graph_generation,
        after.graph_generation,
        before.node_count,
        after.node_count,
        before.edge_count,
        after.edge_count,
        before.graph_path != after.graph_path,
        persisted_during_observation,
    ))
}

pub fn capture_seed(session: &SessionState, observed_at: u64) -> ManifestCaptureSeed {
    finish_capture_seed(capture_parts(session, observed_at))
}

pub fn capture_parts(session: &SessionState, observed_at: u64) -> ManifestCaptureParts {
    let instance = session.instance.summary();
    let graph = session.graph.read();
    ManifestCaptureParts {
        observed_at,
        ingest_roots: session.ingest_roots.clone(),
        workspace_root: session.workspace_root.clone(),
        runtime_root: session.runtime_root.clone(),
        graph_path: session.graph_path.clone(),
        owner_id: instance.instance_id,
        started_at: instance.started_at_ms,
        graph_generation: session.graph_generation,
        last_persist_offset_ns: session
            .last_persist_time
            .and_then(|persisted| persisted.checked_duration_since(session.start_time))
            .map(|duration| duration.as_nanos()),
        node_count: graph.num_nodes() as u64,
        edge_count: graph.num_edges() as u64,
    }
}

pub fn finish_capture_seed(parts: ManifestCaptureParts) -> ManifestCaptureSeed {
    let project_root_candidate = parts
        .ingest_roots
        .iter()
        .find(|root| !crate::session::is_memory_sidecar(root) && Path::new(root.as_str()).is_dir())
        .cloned()
        .or_else(|| {
            parts.workspace_root.as_ref().and_then(|workspace| {
                (!crate::session::is_memory_sidecar(workspace)).then(|| workspace.clone())
            })
        })
        .or_else(|| parts.ingest_roots.first().cloned())
        .or(parts.workspace_root);
    ManifestCaptureSeed {
        observed_at: parts.observed_at,
        project_root_candidate: project_root_candidate.map(PathBuf::from),
        runtime_root: parts.runtime_root,
        graph_path: parts.graph_path,
        owner_id: parts.owner_id,
        started_at: parts.started_at,
        graph_generation: parts.graph_generation,
        last_persist_offset_ns: parts.last_persist_offset_ns,
        node_count: parts.node_count,
        edge_count: parts.edge_count,
    }
}

/// Observe disk/VCS/build facts after the owner mutex has been released.
/// Potentially absent facts become empty and are classified explicitly later.
pub fn observe(seed: ManifestCaptureSeed) -> Result<ManifestSourceSnapshot, String> {
    let ui = UiBundleAttestor::default().observe()?;
    Ok(observe_with_ui(seed, ui))
}

pub fn observe_with_ui(
    seed: ManifestCaptureSeed,
    ui: UiBundleObservation,
) -> ManifestSourceSnapshot {
    // Brain identity belongs to the repo the selected brain serves. Product
    // source identity belongs to the m1nd checkout that built this owner. They
    // are intentionally different roots for hosted brains: comparing a user's
    // package version/commit to the m1nd executable would manufacture drift.
    let project_root = seed
        .project_root_candidate
        .as_deref()
        .and_then(resolve_git_root)
        .or(seed.project_root_candidate.clone());
    let project_root_display = project_root
        .as_deref()
        .map(normalized_path)
        .unwrap_or_default();
    let project_root_fingerprint = if project_root_display.is_empty() {
        String::new()
    } else {
        sha256_label(&digest_domain_bytes(
            ROOT_FINGERPRINT_DOMAIN,
            project_root_display.as_bytes(),
        ))
    };

    let product_source_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(resolve_git_root);
    let (source_commit, source_dirty) = product_source_root
        .as_deref()
        .map(git_identity)
        .unwrap_or_else(|| (String::new(), true));
    let source_version = product_source_root
        .as_deref()
        .and_then(declared_source_version)
        .unwrap_or_else(|| "UNKNOWN".to_string());
    let repo_id = project_root
        .as_deref()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unknown".to_string());
    let brain_id = if project_root_fingerprint.is_empty() {
        String::new()
    } else {
        format!(
            "brain:{}",
            project_root_fingerprint
                .trim_start_matches("sha256:")
                .chars()
                .take(20)
                .collect::<String>()
        )
    };

    let binary_sha256 = binary_sha256();
    let graph_snapshot_sha256 = sha256_file(&seed.graph_path).unwrap_or_default();

    let (architecture_store_version, skeleton_digest, ratification_state) =
        match SystemBlockStore::load(&seed.runtime_root) {
            Ok(Some(store)) => {
                let digest = skeleton_digest(&store).unwrap_or_default();
                let state = match store.skeleton.state {
                    SeedSkeletonState::Candidate => "candidate",
                    SeedSkeletonState::Ratified => "ratified",
                };
                (Some(store.store_version), digest, state.to_string())
            }
            Ok(None) => (None, String::new(), "unavailable".to_string()),
            Err(_) => (None, String::new(), "degraded".to_string()),
        };

    ManifestSourceSnapshot {
        observed_at: seed.observed_at,
        organism_id: "m1nd".to_string(),
        repo_id,
        brain_id,
        project_root_fingerprint,
        source_commit,
        source_dirty,
        source_version,
        owner_id: seed.owner_id,
        binary_version: BINARY_VERSION.to_string(),
        binary_sha256,
        binary_build_source_commit: crate::session::BINARY_BUILD_SOURCE_COMMIT.to_string(),
        binary_build_source_dirty: crate::session::BINARY_BUILD_SOURCE_DIRTY == "1",
        started_at: seed.started_at,
        graph_generation: seed.graph_generation,
        graph_snapshot_sha256,
        node_count: seed.node_count,
        edge_count: seed.edge_count,
        architecture_store_version,
        skeleton_digest,
        ratification_state,
        ui_bundle_version: ui.bundle_version,
        ui_bundle_sha256: ui.bundle_sha256,
        ui_mode: ui.mode,
        ui_status: ui.status,
        ui_freshness: ui.freshness,
    }
}

/// Convenience wrapper for non-latency-sensitive callers.
pub fn capture(session: &SessionState, observed_at: u64) -> Result<ManifestSourceSnapshot, String> {
    observe(capture_seed(session, observed_at))
}

pub fn compose(snapshot: ManifestSourceSnapshot) -> Result<ManifestResponseV1, String> {
    compose_with_autonomy(snapshot, None)
}

/// Compose one manifest from the ordinary owner authorities and, when a G9
/// owner is installed, the exact protected autonomy observation captured for
/// the same timestamp.  The projection is validated before any of its facts
/// enter the manifest; absence stays explicit and cannot be confused with a
/// HUMAN_GATED authority record.
pub fn compose_with_autonomy(
    snapshot: ManifestSourceSnapshot,
    autonomy_projection: Option<AutonomyManifestProjectionV1>,
) -> Result<ManifestResponseV1, String> {
    if let Some(projection) = autonomy_projection.as_ref() {
        projection.validate().map_err(|error| error.to_string())?;
        if projection.observed_at != snapshot.observed_at {
            return Err(format!(
                "autonomy projection timestamp {} differs from manifest observation {}",
                projection.observed_at, snapshot.observed_at
            ));
        }
    }
    let source_status = if snapshot.source_commit.is_empty() {
        AuthorityStatus::Unavailable
    } else if snapshot.source_dirty {
        AuthorityStatus::Drift
    } else {
        AuthorityStatus::Available
    };
    let runtime_status = if snapshot.binary_sha256.is_empty() {
        AuthorityStatus::Unavailable
    } else if snapshot.binary_build_source_commit.is_empty()
        || snapshot.binary_build_source_commit == "unknown"
    {
        AuthorityStatus::Degraded
    } else if snapshot.binary_build_source_dirty
        || snapshot.binary_build_source_commit != snapshot.source_commit
    {
        AuthorityStatus::Drift
    } else {
        AuthorityStatus::Available
    };
    let graph_status = availability(&snapshot.graph_snapshot_sha256);
    let architecture_status =
        if snapshot.architecture_store_version.is_some() && !snapshot.skeleton_digest.is_empty() {
            AuthorityStatus::Available
        } else if snapshot.ratification_state == "degraded" {
            AuthorityStatus::Degraded
        } else {
            AuthorityStatus::Unavailable
        };
    let mut authorities = BTreeMap::new();
    authorities.insert(
        SOURCE_AUTHORITY_ID.to_string(),
        authority(
            &snapshot.source_version,
            &snapshot.source_commit,
            snapshot.observed_at,
            source_status,
        ),
    );
    authorities.insert(
        RUNTIME_BINARY_AUTHORITY_ID.to_string(),
        authority(
            &snapshot.binary_build_source_commit,
            &snapshot.binary_sha256,
            snapshot.observed_at,
            runtime_status,
        ),
    );
    authorities.insert(
        GRAPH_AUTHORITY_ID.to_string(),
        authority(
            &snapshot.graph_generation.to_string(),
            &snapshot.graph_snapshot_sha256,
            snapshot.observed_at,
            graph_status,
        ),
    );
    authorities.insert(
        ARCHITECTURE_AUTHORITY_ID.to_string(),
        authority(
            &snapshot
                .architecture_store_version
                .unwrap_or_default()
                .to_string(),
            &snapshot.skeleton_digest,
            snapshot.observed_at,
            architecture_status,
        ),
    );
    authorities.insert(
        UI_BUNDLE_AUTHORITY_ID.to_string(),
        authority_with_freshness(
            &snapshot.ui_bundle_version,
            &snapshot.ui_bundle_sha256,
            snapshot.observed_at,
            snapshot.ui_status,
            snapshot.ui_freshness,
        ),
    );
    authorities.insert(
        RELEASE_AUTHORITY_ID.to_string(),
        AuthorityFact::unavailable(snapshot.observed_at),
    );

    // Authorities introduced by later cumulative gates are mapped now and stay
    // unavailable until their actual store exists. This is preferable to hiding
    // them or re-stamping a cached value as fresh.
    for authority_id in [
        "authority_journal",
        "autonomy_epoch",
        "constitution",
        "intent_core_store",
        "l1ght",
        "mission_letters",
        "presence",
        "runnerd_registry",
        "sentinel_outbox",
    ] {
        authorities.insert(
            authority_id.to_string(),
            AuthorityFact::unavailable(snapshot.observed_at),
        );
    }

    if let Some(projection) = autonomy_projection.as_ref() {
        for (authority_id, authority_fact) in &projection.authorities {
            authorities.insert(authority_id.clone(), authority_fact.clone());
        }
    }

    let mut supported_modes = BTreeSet::new();
    supported_modes.insert("HUMAN_GATED".to_string());

    let mut manifest = OrganismManifestV1 {
        schema: ORGANISM_MANIFEST_SCHEMA.to_string(),
        organism_id: snapshot.organism_id,
        repo_id: snapshot.repo_id,
        brain_id: snapshot.brain_id,
        project_root_fingerprint: snapshot.project_root_fingerprint,
        source: SourceFact {
            commit: snapshot.source_commit,
            dirty: snapshot.source_dirty,
            version: snapshot.source_version,
        },
        runtime: RuntimeFact {
            owner_id: snapshot.owner_id,
            binary_version: snapshot.binary_version,
            binary_sha256: snapshot.binary_sha256,
            started_at: snapshot.started_at,
        },
        graph: GraphFact {
            generation: snapshot.graph_generation,
            snapshot_sha256: snapshot.graph_snapshot_sha256,
            node_count: snapshot.node_count,
            edge_count: snapshot.edge_count,
        },
        architecture: ArchitectureFact {
            store_version: snapshot.architecture_store_version.unwrap_or_default(),
            skeleton_digest: snapshot.skeleton_digest,
            ratification_state: snapshot.ratification_state,
        },
        ui: UiFact {
            bundle_version: snapshot.ui_bundle_version,
            bundle_sha256: snapshot.ui_bundle_sha256,
            mode: snapshot.ui_mode,
        },
        capabilities: CapabilitiesFact {
            policy_version: "UNAVAILABLE".to_string(),
            enabled_effects: BTreeSet::new(),
        },
        autonomy: autonomy_projection
            .map(|projection| projection.autonomy)
            .unwrap_or_else(|| AutonomyFact {
                supported_modes,
                mechanically_proven_modes: BTreeSet::new(),
                active_mode: "UNKNOWN".to_string(),
                activation_receipt_id: String::new(),
                constitution_digest: String::new(),
                constitution_epoch: 0,
                safety_kernel_digest: String::new(),
                autonomy_epoch: 0,
                grants_digest: String::new(),
                quorum_policy_digest: String::new(),
                max_effective_tier_projection: "NONE".to_string(),
                issuance_frozen: true,
                sentinel_safety_state: "UNKNOWN".to_string(),
            }),
        schemas: SchemasFact {
            mission: crate::mission_letter::MISSION_LETTER_SCHEMA.to_string(),
            receipt: "m1nd-system-block-receipt-v0".to_string(),
            checkpoint: "UNAVAILABLE".to_string(),
            light: "m1nd-light-claim-v0".to_string(),
            system_blocks: crate::system_blocks::SYSTEM_BLOCK_STORE_SCHEMA.to_string(),
        },
        authorities,
        release_provenance: ReleaseProvenanceFact {
            release_candidate_digest: String::new(),
            signature: OpaqueSignature::new(String::new()),
        },
        generated_at: snapshot.observed_at,
        manifest_sha256: String::new(),
    };
    manifest.seal().map_err(|err| err.to_string())?;
    let verification = manifest.verify().map_err(|err| err.to_string())?;
    Ok(ManifestResponseV1 {
        schema: MANIFEST_RESPONSE_SCHEMA,
        manifest,
        verification,
    })
}

fn authority(
    revision: &str,
    digest: &str,
    observed_at: u64,
    status: AuthorityStatus,
) -> AuthorityFact {
    if matches!(
        status,
        AuthorityStatus::Unavailable | AuthorityStatus::Unknown
    ) {
        return AuthorityFact::unavailable(observed_at);
    }
    AuthorityFact {
        revision: revision.to_string(),
        digest: digest.to_string(),
        observed_at,
        freshness: AuthorityFreshness::Fresh,
        status,
    }
}

fn authority_with_freshness(
    revision: &str,
    digest: &str,
    observed_at: u64,
    status: AuthorityStatus,
    freshness: AuthorityFreshness,
) -> AuthorityFact {
    if matches!(
        status,
        AuthorityStatus::Unavailable | AuthorityStatus::Unknown
    ) {
        return AuthorityFact::unavailable(observed_at);
    }
    AuthorityFact {
        revision: revision.to_string(),
        digest: digest.to_string(),
        observed_at,
        freshness,
        status,
    }
}

fn availability(digest: &str) -> AuthorityStatus {
    if digest.is_empty() {
        AuthorityStatus::Unavailable
    } else {
        AuthorityStatus::Available
    }
}

fn resolve_git_root(candidate: &Path) -> Option<PathBuf> {
    let working_dir = if candidate.is_file() {
        candidate.parent().unwrap_or(candidate)
    } else {
        candidate
    };
    git_output(working_dir, &["rev-parse", "--show-toplevel"]).map(PathBuf::from)
}

fn git_identity(root: &Path) -> (String, bool) {
    let head = git_output(root, &["rev-parse", "HEAD"]).unwrap_or_default();
    let status = git_output(root, &["status", "--porcelain"]);
    let dirty = status
        .as_deref()
        .map(str::is_empty)
        .map(|clean| !clean)
        .unwrap_or(true);
    (head, dirty)
}

fn declared_source_version(root: &Path) -> Option<String> {
    let crate_manifest = root.join("m1nd-mcp/Cargo.toml");
    if let Ok(raw) = std::fs::read_to_string(crate_manifest) {
        let mut in_package = false;
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                in_package = trimmed == "[package]";
                continue;
            }
            if in_package {
                let Some((key, value)) = trimmed.split_once('=') else {
                    continue;
                };
                if key.trim() == "version" {
                    let value = value.trim().trim_matches('"');
                    return (!value.is_empty()).then(|| value.to_string());
                }
            }
        }
    }

    let package_json = root.join("package.json");
    std::fs::read(package_json)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| value.get("version")?.as_str().map(str::to_owned))
        .filter(|version| !version.is_empty())
}

fn git_output(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty() || args == ["status", "--porcelain"]).then_some(value)
}

fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(sha256_label(&format!("{:x}", hasher.finalize())))
}

fn binary_sha256() -> String {
    static DIGEST: OnceLock<String> = OnceLock::new();
    DIGEST
        .get_or_init(|| {
            std::env::current_exe()
                .ok()
                .and_then(|path| sha256_file(&path).ok())
                .unwrap_or_default()
        })
        .clone()
}

fn sha256_label(raw: &str) -> String {
    format!("sha256:{raw}")
}

fn skeleton_digest(store: &SystemBlockStore) -> Result<String, String> {
    let mut projection = serde_json::to_value(store).map_err(|err| err.to_string())?;
    let object = projection
        .as_object_mut()
        .ok_or_else(|| "SystemBlockStore did not serialize as an object".to_string())?;
    object.remove("schema");
    object.remove("store_version");
    object.remove("candidate_revision");
    object.remove("curating_by");
    object.remove("curating_until");
    if let Some(blocks) = object.get_mut("blocks").and_then(Value::as_array_mut) {
        for block in blocks {
            if let Some(block) = block.as_object_mut() {
                block.remove("receipts");
            }
        }
    }
    digest_canonical(SKELETON_DIGEST_DOMAIN, &projection)
        .map(|digest| sha256_label(&digest))
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy_manifest::{
        AUTHORITY_JOURNAL_ID, AUTONOMY_EPOCH_AUTHORITY_ID, CONSTITUTION_AUTHORITY_ID,
        INTENT_CORE_STORE_AUTHORITY_ID, SENTINEL_OUTBOX_AUTHORITY_ID,
    };
    use m1nd_control::{ManifestCoherence, ManifestIssueKind};

    fn digest(byte: char) -> String {
        std::iter::repeat_n(byte, 64).collect()
    }

    fn autonomy_projection(observed_at: u64) -> AutonomyManifestProjectionV1 {
        let authority = |revision: &str, digest: String| AuthorityFact {
            revision: revision.to_string(),
            digest,
            observed_at,
            freshness: AuthorityFreshness::Fresh,
            status: AuthorityStatus::Available,
        };
        AutonomyManifestProjectionV1 {
            schema: crate::autonomy_manifest::AUTONOMY_MANIFEST_PROJECTION_SCHEMA.to_string(),
            organism_id: "organism-1".to_string(),
            repo_id: "repo-1".to_string(),
            brain_id: "brain-1".to_string(),
            observed_at,
            state_generation: 1,
            state_digest: digest('a'),
            protected_root_digest: digest('b'),
            journal_sequence: 2,
            journal_record_digest: digest('c'),
            intent_store_root_digest: digest('d'),
            intent_count: 0,
            autonomy: AutonomyFact {
                supported_modes: BTreeSet::from(["HUMAN_GATED".to_string()]),
                mechanically_proven_modes: BTreeSet::new(),
                active_mode: "HUMAN_GATED".to_string(),
                activation_receipt_id: String::new(),
                constitution_digest: digest('e'),
                constitution_epoch: 0,
                safety_kernel_digest: digest('f'),
                autonomy_epoch: 0,
                grants_digest: digest('1'),
                quorum_policy_digest: digest('2'),
                max_effective_tier_projection: "NONE".to_string(),
                issuance_frozen: true,
                sentinel_safety_state: "FROZEN".to_string(),
            },
            authorities: BTreeMap::from([
                (
                    AUTHORITY_JOURNAL_ID.to_string(),
                    authority("2", digest('c')),
                ),
                (
                    AUTONOMY_EPOCH_AUTHORITY_ID.to_string(),
                    authority("0", digest('3')),
                ),
                (
                    CONSTITUTION_AUTHORITY_ID.to_string(),
                    authority("0", digest('e')),
                ),
                (
                    INTENT_CORE_STORE_AUTHORITY_ID.to_string(),
                    authority("0", digest('d')),
                ),
                (
                    SENTINEL_OUTBOX_AUTHORITY_ID.to_string(),
                    authority("0", digest('a')),
                ),
            ]),
        }
    }

    fn snapshot() -> ManifestSourceSnapshot {
        ManifestSourceSnapshot {
            observed_at: 42,
            organism_id: "m1nd".into(),
            repo_id: "m1nd".into(),
            brain_id: "brain:test".into(),
            project_root_fingerprint: "sha256:root".into(),
            source_commit: "abc123".into(),
            source_dirty: false,
            source_version: "1.4.0".into(),
            owner_id: "owner:test".into(),
            binary_version: "1.4.0".into(),
            binary_sha256: "sha256:binary".into(),
            binary_build_source_commit: "abc123".into(),
            binary_build_source_dirty: false,
            started_at: 1,
            graph_generation: 7,
            graph_snapshot_sha256: "sha256:graph".into(),
            node_count: 10,
            edge_count: 20,
            architecture_store_version: Some(3),
            skeleton_digest: "sha256:skeleton".into(),
            ratification_state: "ratified".into(),
            ui_bundle_version: "1.4.0".into(),
            ui_bundle_sha256: "sha256:ui".into(),
            ui_mode: "embedded".into(),
            ui_status: AuthorityStatus::Available,
            ui_freshness: AuthorityFreshness::Fresh,
        }
    }

    #[test]
    fn composer_seals_the_exact_manifest_and_keeps_later_authorities_unknown() {
        let response = compose(snapshot()).unwrap();
        assert_eq!(response.schema, MANIFEST_RESPONSE_SCHEMA);
        assert_eq!(
            response.manifest.compute_manifest_sha256().unwrap(),
            response.manifest.manifest_sha256
        );
        // G1 truth stays DRIFT until the release/autonomy authorities exist;
        // the missing later authority must still be classified explicitly.
        assert_eq!(response.verification.coherence, ManifestCoherence::Drift);
        assert!(response.verification.issues.iter().any(|issue| {
            issue.kind == ManifestIssueKind::Unknown
                && issue.authority_id.as_deref() == Some("autonomy_epoch")
        }));
    }

    #[test]
    fn protected_autonomy_projection_replaces_only_its_authoritative_facts() {
        let response = compose_with_autonomy(snapshot(), Some(autonomy_projection(42))).unwrap();
        assert_eq!(response.manifest.autonomy.active_mode, "HUMAN_GATED");
        assert_eq!(
            response.manifest.authorities[AUTONOMY_EPOCH_AUTHORITY_ID].status,
            AuthorityStatus::Available
        );
        assert_eq!(
            response.manifest.authorities["mission_letters"].status,
            AuthorityStatus::Unavailable
        );
        assert_eq!(
            response.manifest.compute_manifest_sha256().unwrap(),
            response.manifest.manifest_sha256
        );
    }

    #[test]
    fn autonomy_projection_from_another_observation_window_is_refused() {
        let error = compose_with_autonomy(snapshot(), Some(autonomy_projection(41)))
            .expect_err("mixed-time authority facts must fail closed");
        assert!(error.contains("timestamp 41"));
    }

    #[test]
    fn source_binary_bundle_version_drift_is_visible() {
        let mut input = snapshot();
        input.ui_bundle_version = "0.1.0".into();
        let response = compose(input).unwrap();
        assert_eq!(response.verification.coherence, ManifestCoherence::Drift);
        assert!(response.verification.issues.iter().any(|issue| issue
            .detail
            .contains("source/binary/bundle versions diverge")));
    }

    #[test]
    fn stale_same_version_binary_commit_is_visible() {
        let mut input = snapshot();
        input.binary_build_source_commit = "different-commit".into();
        let response = compose(input).unwrap();
        assert_eq!(response.verification.coherence, ManifestCoherence::Drift);
        assert!(response
            .verification
            .issues
            .iter()
            .any(|issue| issue.detail.contains("differs from projected revision")));
    }

    #[test]
    fn dirty_build_is_visible_even_when_commit_and_version_match() {
        let mut input = snapshot();
        input.binary_build_source_dirty = true;
        let response = compose(input).unwrap();
        assert_eq!(response.verification.coherence, ManifestCoherence::Drift);
        assert!(response
            .verification
            .issues
            .iter()
            .any(|issue| issue.detail.contains("authority reports drift")));
    }

    #[test]
    fn placeholder_ui_authority_is_never_fresh() {
        let mut input = snapshot();
        input.ui_mode = "placeholder".into();
        input.ui_status = AuthorityStatus::Degraded;
        input.ui_freshness = AuthorityFreshness::Unknown;
        let response = compose(input).unwrap();
        let ui = &response.manifest.authorities[UI_BUNDLE_AUTHORITY_ID];
        assert_eq!(ui.status, AuthorityStatus::Degraded);
        assert_eq!(ui.freshness, AuthorityFreshness::Unknown);
    }

    #[test]
    fn filesystem_ui_drift_carries_the_runtime_digest_not_the_build_digest() {
        let mut input = snapshot();
        input.ui_mode = "external_ui_dir".into();
        input.ui_bundle_sha256 = "sha256:served-now".into();
        input.ui_status = AuthorityStatus::Drift;
        let response = compose(input).unwrap();
        let ui = &response.manifest.authorities[UI_BUNDLE_AUTHORITY_ID];
        assert_eq!(ui.status, AuthorityStatus::Drift);
        assert_eq!(ui.digest, "sha256:served-now");
        assert_eq!(response.verification.coherence, ManifestCoherence::Drift);
    }

    #[test]
    fn dirty_source_cannot_verify_as_coherent() {
        let mut input = snapshot();
        input.source_dirty = true;
        let response = compose(input).unwrap();
        assert_eq!(response.verification.coherence, ManifestCoherence::Drift);
        assert_eq!(
            response.manifest.authorities[SOURCE_AUTHORITY_ID].status,
            AuthorityStatus::Drift
        );
    }

    #[test]
    fn unavailable_projection_never_restamps_old_revision_or_digest() {
        let mut input = snapshot();
        input.graph_snapshot_sha256.clear();
        let response = compose(input).unwrap();
        let graph = &response.manifest.authorities[GRAPH_AUTHORITY_ID];
        assert_eq!(graph.status, AuthorityStatus::Unavailable);
        assert!(graph.revision.is_empty());
        assert!(graph.digest.is_empty());
    }

    #[test]
    fn file_hash_is_content_addressed() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("fact");
        std::fs::write(&file, b"one").unwrap();
        let one = sha256_file(&file).unwrap();
        std::fs::write(&file, b"two").unwrap();
        let two = sha256_file(&file).unwrap();
        assert_ne!(one, two);
        assert!(one.starts_with("sha256:"));
    }

    #[test]
    fn empty_git_status_is_a_valid_clean_observation() {
        let dir = tempfile::tempdir().unwrap();
        Command::new("git")
            .current_dir(dir.path())
            .args(["init", "-q"])
            .status()
            .unwrap();
        Command::new("git")
            .current_dir(dir.path())
            .args(["config", "user.email", "manifest@test.invalid"])
            .status()
            .unwrap();
        Command::new("git")
            .current_dir(dir.path())
            .args(["config", "user.name", "Manifest Test"])
            .status()
            .unwrap();
        std::fs::write(dir.path().join("tracked"), b"x").unwrap();
        Command::new("git")
            .current_dir(dir.path())
            .args(["add", "tracked"])
            .status()
            .unwrap();
        Command::new("git")
            .current_dir(dir.path())
            .args(["commit", "-qm", "fixture"])
            .status()
            .unwrap();
        let (head, dirty) = git_identity(dir.path());
        assert_eq!(head.len(), 40);
        assert!(!dirty);
    }

    #[test]
    fn source_version_is_read_from_the_source_authority_not_the_running_binary() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("m1nd-mcp")).unwrap();
        std::fs::write(
            dir.path().join("m1nd-mcp/Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"9.8.7\"\n\n[dependencies]\n",
        )
        .unwrap();
        assert_eq!(
            declared_source_version(dir.path()).as_deref(),
            Some("9.8.7")
        );
    }

    #[test]
    fn hosted_brain_identity_cannot_replace_m1nd_product_source_identity() {
        let dir = tempfile::tempdir().unwrap();
        let hosted = dir.path().join("hosted-project");
        std::fs::create_dir_all(&hosted).unwrap();
        let seed = ManifestCaptureSeed {
            observed_at: 42,
            project_root_candidate: Some(hosted.clone()),
            runtime_root: dir.path().join("runtime"),
            graph_path: dir.path().join("missing-graph-snapshot"),
            owner_id: "owner:test".into(),
            started_at: 1,
            graph_generation: 0,
            last_persist_offset_ns: None,
            node_count: 0,
            edge_count: 0,
        };
        let ui = UiBundleObservation {
            bundle_version: "1.4.0".into(),
            bundle_sha256: "sha256:ui".into(),
            mode: "fixture".into(),
            status: AuthorityStatus::Available,
            freshness: AuthorityFreshness::Fresh,
        };

        let observed = observe_with_ui(seed, ui);
        let product_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(resolve_git_root)
            .expect("the repository test runs from the M1nd product checkout");
        let (expected_commit, expected_dirty) = git_identity(&product_root);

        assert_eq!(observed.repo_id, "hosted-project");
        assert_eq!(observed.source_commit, expected_commit);
        assert_eq!(observed.source_dirty, expected_dirty);
        assert_eq!(
            observed.source_version,
            declared_source_version(&product_root).expect("M1nd source version")
        );
        assert_ne!(
            observed.repo_id, observed.source_version,
            "a hosted brain remains a brain/repo authority, never product source"
        );
    }
}
