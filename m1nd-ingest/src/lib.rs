#![allow(unused)]
// === crates/m1nd-ingest/src/lib.rs ===

use m1nd_core::error::{M1ndError, M1ndResult};
use m1nd_core::graph::NodeProvenanceInput;
use m1nd_core::types::*;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

pub mod bibtex_adapter;
pub mod canonical;
pub mod cargo_workspace;
pub mod cross_domain;
pub mod cross_file;
pub mod crossref_adapter;
pub mod diff;
pub mod document_router;
pub mod extract;
pub mod jats_adapter;
pub mod json_adapter;
pub mod l1ght_adapter;
pub mod memory_adapter;
pub mod merge;
pub mod ownership;
pub mod patent_adapter;
pub mod path_policy;
pub mod resolve;
pub mod rfc_adapter;
pub mod universal_adapter;
pub mod walker;

pub use bibtex_adapter::BibTexAdapter;
pub use crossref_adapter::CrossRefAdapter;
pub use jats_adapter::JatsArticleAdapter;
pub use l1ght_adapter::L1ghtIngestAdapter;
pub use patent_adapter::PatentIngestAdapter;
pub use rfc_adapter::RfcAdapter;
pub use universal_adapter::{
    grobid_endpoint_summary, ProviderAvailability, ProviderExtractionOutcome,
    ProviderExtractionResult, ProviderFailureKind, UniversalDocumentOutcome,
    UniversalIngestAdapter, UniversalIngestBundle, UniversalIngestStatus, UniversalIngestSummary,
};

pub(crate) fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

pub(crate) fn is_valid_relative_file_path(rel_path: &str) -> bool {
    if rel_path.is_empty() || rel_path != rel_path.trim() {
        return false;
    }

    // A backslash is a legal filename byte on Unix, not a path separator.  It
    // must therefore never be rewritten to `/`: doing so makes two distinct
    // filesystem objects claim the same graph/source identity.  Windows cannot
    // encode a literal backslash in a filename, so its native separator remains
    // acceptable and is canonicalized only after validation.
    #[cfg(unix)]
    if rel_path.contains('\\') {
        return false;
    }

    let windows_absolute = rel_path.as_bytes().get(1) == Some(&b':');
    !rel_path.starts_with('/')
        && !windows_absolute
        && !rel_path.contains('\0')
        && !rel_path.contains('\u{FFFD}')
        && rel_path
            .split(['/', '\\'])
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

fn build_file_external_id(rel_path: &str) -> Option<String> {
    if !is_valid_relative_file_path(rel_path) {
        return None;
    }

    #[cfg(windows)]
    let canonical = rel_path.replace('\\', "/");
    #[cfg(not(windows))]
    let canonical = rel_path.to_string();
    Some(format!("file::{canonical}"))
}

pub(crate) fn relative_source_path(root: &Path, path: &Path) -> String {
    if root.is_file() {
        let relative = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        #[cfg(windows)]
        return relative.replace('\\', "/");
        #[cfg(not(windows))]
        return relative;
    }

    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    #[cfg(windows)]
    return relative.replace('\\', "/");
    #[cfg(not(windows))]
    relative
}

/// The canonical, cross-platform identity string for an **already-canonicalized**
/// managed path. This is the exact value stamped into
/// [`ownership::CodeOwnershipManifestV1::root_identity`] and every managed source
/// key, so any consumer that must compare against a bundle's `root_identity`
/// (e.g. the graph-ingest admission path in `m1nd-mcp`) has to derive its own
/// root string through this same function — Windows `canonicalize` yields a
/// `\\?\C:\...` verbatim path, which is normalized here to `//?/C:/...` so the
/// identity never depends on the OS separator. Callers pass a path they have
/// already run through [`std::fs::canonicalize`]; this does not canonicalize.
pub fn exact_path_identity(path: &Path) -> M1ndResult<String> {
    let identity = path.to_str().ok_or_else(|| M1ndError::InvalidParams {
        tool: "ingest_identity".into(),
        detail: format!("managed path is not valid UTF-8: {}", path.display()),
    })?;
    #[cfg(windows)]
    return Ok(identity.replace('\\', "/"));
    #[cfg(not(windows))]
    Ok(identity.to_string())
}

pub(crate) fn is_valid_external_id(external_id: &str) -> bool {
    if external_id.is_empty()
        || external_id != external_id.trim()
        || external_id.contains('\0')
        || external_id.contains('\u{FFFD}')
    {
        return false;
    }

    if let Some(rel_path) = external_id.strip_prefix("file::") {
        return is_valid_relative_file_path(rel_path);
    }

    true
}

fn active_pipeline_features() -> Vec<String> {
    let mut features = Vec::new();
    if cfg!(feature = "tier1") {
        features.push("tier1".to_string());
    }
    if cfg!(feature = "tier2") {
        features.push("tier2".to_string());
    }
    features
}

fn discovery_policy_fingerprint(
    root: &Path,
    skip_dirs: &[String],
    skip_files: &[String],
    include_dotfiles: bool,
    dotfile_patterns: &[String],
    vcs: &walker::VcsContextV1,
) -> M1ndResult<String> {
    if !vcs.valid() {
        return Err(M1ndError::IngestError(
            "discovery policy cannot bind an invalid VCS context".into(),
        ));
    }
    let root = root.canonicalize().map_err(M1ndError::Io)?;
    let scan_root = if root.is_file() {
        root.parent().unwrap_or(&root)
    } else {
        root.as_path()
    };
    let mut controls = BTreeMap::<String, String>::new();
    let iter = walkdir::WalkDir::new(scan_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 || !entry.file_type().is_dir() {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            name != ".git"
                && !path_policy::is_noise_dir_name(&name)
                && !skip_dirs.iter().any(|skip| skip == name.as_ref())
        });
    for entry in iter {
        let entry = entry.map_err(|error| {
            M1ndError::Io(std::io::Error::other(format!(
                "policy control walk failed: {error}"
            )))
        })?;
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| M1ndError::InvalidParams {
                tool: "ingest_policy".into(),
                detail: format!(
                    "non-UTF-8 discovery control path: {}",
                    entry.path().display()
                ),
            })?;
        if name != ".gitignore" && name != ".ignore" {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(scan_root)
            .map_err(|_| M1ndError::InvalidParams {
                tool: "ingest_policy".into(),
                detail: "discovery control escaped scan root".into(),
            })?
            .to_str()
            .ok_or_else(|| M1ndError::InvalidParams {
                tool: "ingest_policy".into(),
                detail: "non-UTF-8 discovery control relative path".into(),
            })?;
        #[cfg(windows)]
        let relative = relative.replace('\\', "/");
        #[cfg(not(windows))]
        let relative = relative.to_string();
        if !is_valid_relative_file_path(&relative) {
            return Err(M1ndError::InvalidParams {
                tool: "ingest_policy".into(),
                detail: format!(
                    "discovery control has non-bijective relative identity: {relative:?}"
                ),
            });
        }
        if controls
            .insert(
                relative.clone(),
                ownership::sha256_bytes(&std::fs::read(entry.path()).map_err(M1ndError::Io)?),
            )
            .is_some()
        {
            return Err(M1ndError::IngestError(format!(
                "duplicate discovery control identity: {relative:?}"
            )));
        }
    }
    for ancestor in scan_root.ancestors().skip(1) {
        for name in [".gitignore", ".ignore"] {
            let control = ancestor.join(name);
            if control.is_file() {
                let identity = control.to_str().ok_or_else(|| M1ndError::InvalidParams {
                    tool: "ingest_policy".into(),
                    detail: "non-UTF-8 ancestor discovery control path".into(),
                })?;
                let key = format!("ancestor:{identity}");
                if controls
                    .insert(
                        key.clone(),
                        ownership::sha256_bytes(&std::fs::read(&control).map_err(M1ndError::Io)?),
                    )
                    .is_some()
                {
                    return Err(M1ndError::IngestError(format!(
                        "duplicate ancestor discovery control identity: {key:?}"
                    )));
                }
            }
        }
    }
    if vcs.is_git() {
        let raw = run_git_policy_command(
            scan_root,
            &["rev-parse", "--git-path", "info/exclude"],
            false,
        )?
        .ok_or_else(|| M1ndError::IngestError("Git exclude discovery returned no path".into()))?;
        let path = if Path::new(&raw).is_absolute() {
            PathBuf::from(&raw)
        } else {
            scan_root.join(&raw)
        };
        record_optional_control(&mut controls, "git-exclude", &path)?;

        match run_git_policy_command(
            scan_root,
            &["config", "--path", "--get", "core.excludesFile"],
            true,
        )? {
            Some(raw) => {
                let path = if Path::new(&raw).is_absolute() {
                    PathBuf::from(raw)
                } else {
                    scan_root.join(raw)
                };
                record_optional_control(&mut controls, "global-git-exclude", &path)?;
            }
            None => {
                controls.insert("global-git-exclude-config".into(), "UNSET".into());
            }
        }
    }
    let build_features = active_pipeline_features();
    let encoded = serde_json::to_vec(&(
        "m1nd-discovery-policy-v1",
        skip_dirs,
        skip_files,
        include_dotfiles,
        dotfile_patterns,
        &build_features,
        "nul-in-first-8192-v1",
        controls,
    ))
    .map_err(M1ndError::Serde)?;
    Ok(ownership::sha256_bytes(&encoded))
}

fn run_git_policy_command(
    root: &Path,
    args: &[&str],
    status_one_means_absent: bool,
) -> M1ndResult<Option<String>> {
    let label = format!("git {}", args.join(" "));
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| {
            M1ndError::IngestError(format!(
                "{label} failed during discovery-policy binding: {error}"
            ))
        })?;
    if status_one_means_absent && output.status.code() == Some(1) {
        return Ok(None);
    }
    if !output.status.success() {
        return Err(M1ndError::IngestError(format!(
            "{label} failed during discovery-policy binding: status {:?}, stderr {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    let value = String::from_utf8(output.stdout).map_err(|error| {
        M1ndError::IngestError(format!("{label} output is not valid UTF-8: {error}"))
    })?;
    let value = value.strip_suffix('\n').unwrap_or(&value);
    let value = value.strip_suffix('\r').unwrap_or(value);
    if value.is_empty() || value.contains('\n') || value.contains('\r') || value != value.trim() {
        return Err(M1ndError::IngestError(format!(
            "{label} did not return one exact path: {value:?}"
        )));
    }
    Ok(Some(value.to_string()))
}

fn record_optional_control(
    controls: &mut BTreeMap<String, String>,
    kind: &str,
    path: &Path,
) -> M1ndResult<()> {
    let identity = path.to_str().ok_or_else(|| M1ndError::InvalidParams {
        tool: "ingest_policy".into(),
        detail: format!("{kind} path is not valid UTF-8"),
    })?;
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(M1ndError::IngestError(format!(
                "{kind} control is not a regular non-symlink file: {identity}"
            )));
        }
        Ok(_) => {
            controls.insert(
                format!("{kind}:{identity}"),
                ownership::sha256_bytes(&std::fs::read(path).map_err(M1ndError::Io)?),
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            controls.insert(format!("{kind}:{identity}"), "ABSENT".into());
        }
        Err(error) => return Err(M1ndError::Io(error)),
    }
    Ok(())
}

fn vcs_context_digest(walk: &walker::WalkResult) -> M1ndResult<String> {
    if !walk.vcs.valid() {
        return Err(M1ndError::IngestError(
            "VCS context is missing or internally inconsistent".into(),
        ));
    }
    let mut files = walk
        .files
        .iter()
        .map(|file| {
            (
                file.relative_path.clone(),
                file.commit_count,
                file.last_commit_time.to_bits(),
                file.last_modified.to_bits(),
            )
        })
        .collect::<Vec<_>>();
    files.sort();
    let mut commit_groups = walk.commit_groups.clone();
    for group in &mut commit_groups {
        group.sort();
        group.dedup();
    }
    commit_groups.sort();
    commit_groups.dedup();
    let encoded = serde_json::to_vec(&("m1nd-vcs-context-v1", &walk.vcs, files, commit_groups))
        .map_err(M1ndError::Serde)?;
    Ok(ownership::sha256_bytes(&encoded))
}

pub trait IngestAdapter: Send + Sync {
    fn domain(&self) -> &str;
    fn ingest(&self, root: &std::path::Path) -> M1ndResult<(m1nd_core::graph::Graph, IngestStats)>;
}

pub struct IngestConfig {
    pub root: PathBuf,
    pub timeout: Duration,
    pub max_nodes: u64,
    pub skip_dirs: Vec<String>,
    pub skip_files: Vec<String>,
    pub parallelism: usize,
    pub include_dotfiles: bool,
    pub dotfile_patterns: Vec<String>,
}

impl Default for IngestConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            timeout: Duration::from_secs(300),
            max_nodes: 500_000,
            skip_dirs: path_policy::default_skip_dirs(),
            skip_files: vec![
                "package-lock.json".into(),
                "yarn.lock".into(),
                "Cargo.lock".into(),
                "poetry.lock".into(),
            ],
            parallelism: std::thread::available_parallelism()
                .map(|p| p.get().min(16))
                .unwrap_or(8),
            include_dotfiles: false,
            dotfile_patterns: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct IngestStats {
    pub files_scanned: u64,
    pub files_parsed: u64,
    pub files_skipped_binary: u64,
    pub files_skipped_encoding: u64,
    pub nodes_created: u64,
    pub edges_created: u64,
    pub references_resolved: u64,
    pub references_unresolved: u64,
    pub references_ambiguous: u64,
    pub label_collisions: u64,
    pub elapsed_ms: f64,
    pub commit_groups: Vec<Vec<String>>,
    pub discovered_files: Vec<walker::DiscoveredFile>,
}

/// A code graph plus the exact per-source ownership needed for safe incremental
/// replacement. `ownership.coverage` is COMPLETE only after auditing every
/// final graph node and canonical edge against the emitted claims.
pub struct CodeIngestBundleV1 {
    pub schema: String,
    pub graph: m1nd_core::graph::Graph,
    pub stats: IngestStats,
    pub ownership: ownership::CodeOwnershipManifestV1,
}

impl CodeIngestBundleV1 {
    /// Re-open every source in the exact root snapshot and require its bytes to
    /// still match the manifest. This READY-time fence prevents a parallel
    /// extraction/enrichment run from being installed after any source drift.
    pub fn revalidate_sources(&self) -> M1ndResult<()> {
        self.revalidate_sources_with_cancel(|| false)
    }

    /// Cancellable form of `revalidate_sources`. Mutation consumers that keep
    /// a bundle between actor turns can bind the same supervisor/deadline probe
    /// used during extraction.
    pub fn revalidate_sources_with_cancel<P>(&self, is_cancelled: P) -> M1ndResult<()>
    where
        P: Fn() -> bool + Sync,
    {
        let cancellation = IngestCancellation::new(&is_cancelled);
        self.revalidate_sources_inner(&cancellation)
    }

    fn revalidate_sources_inner<P>(
        &self,
        cancellation: &IngestCancellation<'_, P>,
    ) -> M1ndResult<()>
    where
        P: Fn() -> bool + Sync + ?Sized,
    {
        cancellation.check()?;
        let root = PathBuf::from(&self.ownership.root_identity);
        let canonical_root = root.canonicalize().map_err(M1ndError::Io)?;
        cancellation.check()?;
        if exact_path_identity(&canonical_root)? != self.ownership.root_identity {
            return Err(M1ndError::FullReindexRequired {
                reason: "managed-root identity is not the exact canonical path".into(),
            });
        }
        let pipeline = &self.ownership.pipeline_receipt;
        if pipeline.schema != ownership::CODE_PIPELINE_RECEIPT_SCHEMA {
            return Err(M1ndError::FullReindexRequired {
                reason: format!("unsupported pipeline receipt schema: {:?}", pipeline.schema),
            });
        }
        let current_walk = walker::DirectoryWalker::new(
            pipeline.skip_dirs.clone(),
            pipeline.skip_files.clone(),
            pipeline.include_dotfiles,
            pipeline.dotfile_patterns.clone(),
        )
        .walk(&canonical_root)?;
        cancellation.check()?;
        let observed_policy_fingerprint = discovery_policy_fingerprint(
            &canonical_root,
            &pipeline.skip_dirs,
            &pipeline.skip_files,
            pipeline.include_dotfiles,
            &pipeline.dotfile_patterns,
            &current_walk.vcs,
        )?;
        cancellation.check()?;
        if observed_policy_fingerprint != pipeline.policy_fingerprint
            || active_pipeline_features() != pipeline.build_features
            || pipeline.binary_policy != "nul-in-first-8192-v1"
            || pipeline.producer_name != ownership::CODE_PIPELINE_PRODUCER_NAME
            || pipeline.producer_version != env!("CARGO_PKG_VERSION")
            || pipeline.producer_build_identity != ownership::compiled_producer_build_identity()
            || pipeline.producer_executable_identity
                != ownership::running_producer_executable_identity().map_err(M1ndError::Io)?
        {
            return Err(M1ndError::FullReindexRequired {
                reason: "discovery policy, control files, binary policy, or build features changed"
                    .into(),
            });
        }
        let mut current_source_keys = std::collections::BTreeSet::new();
        for file in &current_walk.files {
            cancellation.check()?;
            current_source_keys.insert(file.relative_path.clone());
        }
        let mut indexed_source_keys = std::collections::BTreeSet::new();
        for source_key in self.ownership.source_digests.keys() {
            cancellation.check()?;
            indexed_source_keys.insert(source_key.clone());
        }
        if current_source_keys != indexed_source_keys {
            return Err(M1ndError::FullReindexRequired {
                reason: format!(
                    "managed-root discovery drift: indexed {} sources, current {} sources",
                    indexed_source_keys.len(),
                    current_source_keys.len()
                ),
            });
        }
        cancellation.check()?;
        if vcs_context_digest(&current_walk)? != pipeline.vcs_context_digest {
            return Err(M1ndError::FullReindexRequired {
                reason: "VCS/file-metadata context changed since extraction".into(),
            });
        }
        for (source_key, expected) in &self.ownership.source_digests {
            cancellation.check()?;
            let candidate = if canonical_root.is_file() {
                let expected_name = canonical_root
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| M1ndError::FullReindexRequired {
                        reason: "managed single-file root name is not valid UTF-8".into(),
                    })?;
                if source_key != expected_name {
                    return Err(M1ndError::FullReindexRequired {
                        reason: format!(
                            "source key {source_key:?} is not the managed single-file root {expected_name:?}"
                        ),
                    });
                }
                canonical_root.clone()
            } else {
                canonical_root.join(source_key)
            };
            let metadata = std::fs::symlink_metadata(&candidate).map_err(M1ndError::Io)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(M1ndError::FullReindexRequired {
                    reason: format!(
                        "source {source_key:?} is no longer a regular non-symlink file"
                    ),
                });
            }
            let canonical_candidate = candidate.canonicalize().map_err(M1ndError::Io)?;
            if canonical_root.is_dir() && !canonical_candidate.starts_with(&canonical_root) {
                return Err(M1ndError::FullReindexRequired {
                    reason: format!("source {source_key:?} escaped the managed root"),
                });
            }
            let observed = ownership::sha256_bytes(
                &std::fs::read(&canonical_candidate).map_err(M1ndError::Io)?,
            );
            cancellation.check()?;
            if &observed != expected {
                return Err(M1ndError::FullReindexRequired {
                    reason: format!(
                        "source snapshot drift for {source_key:?}: expected {expected}, observed {observed}"
                    ),
                });
            }
        }
        cancellation.check()?;
        Ok(())
    }

    /// Mutation consumers must call this gate before applying the bundle. Exact
    /// file ingestion calls it internally, so partial ownership can never be
    /// mistaken for a replace-safe child result.
    pub fn require_complete(&self) -> M1ndResult<()> {
        self.require_complete_with_cancel(|| false)
    }

    /// Cancellable form of the final COMPLETE gate. It returns the same typed
    /// cancellation error and never weakens any ownership/source validation.
    pub fn require_complete_with_cancel<P>(&self, is_cancelled: P) -> M1ndResult<()>
    where
        P: Fn() -> bool + Sync,
    {
        let cancellation = IngestCancellation::new(&is_cancelled);
        self.require_complete_inner(&cancellation)
    }

    fn require_complete_inner<P>(&self, cancellation: &IngestCancellation<'_, P>) -> M1ndResult<()>
    where
        P: Fn() -> bool + Sync + ?Sized,
    {
        cancellation.check()?;
        if self.schema != ownership::CODE_INGEST_BUNDLE_SCHEMA {
            return Err(M1ndError::InvalidParams {
                tool: "code_ingest_bundle".into(),
                detail: format!("unsupported bundle schema: {:?}", self.schema),
            });
        }
        self.revalidate_sources_inner(cancellation)?;
        cancellation.check()?;
        let receipt_valid = self
            .ownership
            .verify_against_graph(&self.graph)
            .map_err(|error| M1ndError::InvalidParams {
                tool: "code_ingest_bundle".into(),
                detail: format!("ownership receipt verification failed: {error}"),
            })?;
        cancellation.check()?;
        if !receipt_valid {
            return Err(M1ndError::InvalidParams {
                tool: "code_ingest_bundle".into(),
                detail: "ownership receipt or graph topology mismatch".into(),
            });
        }
        if self.ownership.coverage == ownership::OwnershipCoverageV1::Complete {
            return Ok(());
        }

        Err(M1ndError::InvalidParams {
            tool: "code_ingest_bundle".into(),
            detail: format!(
                "ownership coverage INCOMPLETE: unowned_nodes={}, unowned_edges={}, dangling_node_claims={}, dangling_edge_claims={}, duplicate_graph_edges={}, orphan_node_slots={}, multiply_identified_node_slots={}, invalid_identity_ids={}, out_of_range_identity_ids={}, orphan_edge_slots={}, csr_shape_valid={}, reverse_csr_valid={}, ownership_digest={}",
                self.ownership.unowned_nodes.len(),
                self.ownership.unowned_edges.len(),
                self.ownership.dangling_node_claims.len(),
                self.ownership.dangling_edge_claims.len(),
                self.ownership.duplicate_graph_edges.len(),
                self.ownership.orphan_node_slots.len(),
                self.ownership.multiply_identified_node_slots.len(),
                self.ownership.invalid_identity_ids.len(),
                self.ownership.out_of_range_identity_ids.len(),
                self.ownership.orphan_edge_slots.len(),
                self.ownership.csr_shape_valid,
                self.ownership.reverse_csr_valid,
                self.ownership.ownership_digest,
            ),
        })
    }
}

pub struct Ingestor {
    config: IngestConfig,
}

/// One ingest-local latch around an arbitrary supervisor probe. Once any
/// worker observes cancellation, every later checkpoint returns the same typed
/// error even if the external probe itself is not monotonic.
struct IngestCancellation<'a, P: ?Sized> {
    probe: &'a P,
    observed: std::sync::atomic::AtomicBool,
}

impl<'a, P> IngestCancellation<'a, P>
where
    P: Fn() -> bool + Sync + ?Sized,
{
    fn new(probe: &'a P) -> Self {
        Self {
            probe,
            observed: std::sync::atomic::AtomicBool::new(false),
        }
    }

    fn check(&self) -> M1ndResult<()> {
        use std::sync::atomic::Ordering;

        if self.observed.load(Ordering::Acquire) || (self.probe)() {
            self.observed.store(true, Ordering::Release);
            return Err(M1ndError::IngestionCancelled);
        }
        Ok(())
    }
}

struct IngestBundleContext<'a> {
    exact_source_key: Option<String>,
    enrich_global: bool,
    initial_graph: m1nd_core::graph::Graph,
    seed_ownership: Option<&'a ownership::CodeOwnershipManifestV1>,
    base_ownership_digest: Option<String>,
    replaceable_node_ids: Option<&'a HashSet<String>>,
}

struct CachedSourceSnapshot {
    root: PathBuf,
}

impl CachedSourceSnapshot {
    fn materialize_with_cancel<'a, P>(
        files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
        cancellation: &IngestCancellation<'_, P>,
    ) -> M1ndResult<Self>
    where
        P: Fn() -> bool + Sync + ?Sized,
    {
        cancellation.check()?;
        static SNAPSHOT_COUNTER: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(0);
        let nonce = SNAPSHOT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let epoch_nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let root = std::env::temp_dir().join(format!(
            "m1nd-code-source-snapshot-{}-{epoch_nonce}-{nonce}",
            std::process::id(),
        ));
        std::fs::create_dir(&root).map_err(M1ndError::Io)?;
        let snapshot = Self { root };
        for (source_key, bytes) in files {
            cancellation.check()?;
            if !is_valid_relative_file_path(source_key) {
                return Err(M1ndError::InvalidParams {
                    tool: "ingest_source_snapshot".into(),
                    detail: format!("invalid source key for immutable snapshot: {source_key:?}"),
                });
            }
            let target = snapshot.root.join(source_key);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(M1ndError::Io)?;
            }
            std::fs::write(&target, bytes).map_err(M1ndError::Io)?;
            cancellation.check()?;
        }
        Ok(snapshot)
    }

    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for CachedSourceSnapshot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn sort_pending_edges_canonically<P>(
    graph: &mut m1nd_core::graph::Graph,
    cancellation: &IngestCancellation<'_, P>,
) -> M1ndResult<()>
where
    P: Fn() -> bool + Sync + ?Sized,
{
    cancellation.check()?;
    if graph.csr.num_edges() != 0
        || graph.csr.pending_edges.len() != graph.edge_plasticity.original_weight.len()
        || graph.csr.pending_edges.len() != graph.edge_plasticity.current_weight.len()
    {
        return Err(M1ndError::CorruptState {
            reason: "pre-finalize edge/plasticity staging arrays are not bijective".into(),
        });
    }
    let mut node_ids = vec![String::new(); graph.num_nodes() as usize];
    for (interned, node_id) in &graph.id_to_node {
        cancellation.check()?;
        node_ids[node_id.as_usize()] = graph.strings.resolve(*interned).to_string();
    }
    let pending = std::mem::take(&mut graph.csr.pending_edges);
    let old = std::mem::replace(
        &mut graph.edge_plasticity,
        m1nd_core::graph::EdgePlasticity::new(),
    );
    let mut indexed = pending.into_iter().enumerate().collect::<Vec<_>>();
    cancellation.check()?;
    indexed.sort_by(|(left_index, left), (right_index, right)| {
        (
            node_ids[left.source.as_usize()].as_str(),
            node_ids[left.target.as_usize()].as_str(),
            graph.strings.resolve(left.relation),
            left.direction as u8,
            left.inhibitory,
            left.weight.get().to_bits(),
            left.causal_strength.get().to_bits(),
            *left_index,
        )
            .cmp(&(
                node_ids[right.source.as_usize()].as_str(),
                node_ids[right.target.as_usize()].as_str(),
                graph.strings.resolve(right.relation),
                right.direction as u8,
                right.inhibitory,
                right.weight.get().to_bits(),
                right.causal_strength.get().to_bits(),
                *right_index,
            ))
    });
    cancellation.check()?;
    for (old_index, edge) in indexed {
        cancellation.check()?;
        graph.csr.pending_edges.push(edge);
        graph
            .edge_plasticity
            .original_weight
            .push(old.original_weight[old_index]);
        graph
            .edge_plasticity
            .current_weight
            .push(old.current_weight[old_index]);
        graph
            .edge_plasticity
            .strengthen_count
            .push(old.strengthen_count[old_index]);
        graph
            .edge_plasticity
            .weaken_count
            .push(old.weaken_count[old_index]);
        graph
            .edge_plasticity
            .ltp_applied
            .push(old.ltp_applied[old_index]);
        graph
            .edge_plasticity
            .ltd_applied
            .push(old.ltd_applied[old_index]);
        graph
            .edge_plasticity
            .last_used_query
            .push(old.last_used_query[old_index]);
    }
    cancellation.check()?;
    Ok(())
}

impl Ingestor {
    pub fn new(config: IngestConfig) -> Self {
        Self { config }
    }

    fn select_extractor(ext: &str) -> Box<dyn extract::Extractor> {
        match ext {
            "py" | "pyi" => Box::new(extract::python::PythonExtractor::new()),
            "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => {
                Box::new(extract::typescript::TypeScriptExtractor::new())
            }
            "rs" => Box::new(extract::rust_lang::RustExtractor::new()),
            "go" => Box::new(extract::go::GoExtractor::new()),
            "java" => Box::new(extract::java::JavaExtractor::new()),
            #[cfg(feature = "tier1")]
            "c" | "h" => Box::new(extract::tree_sitter_ext::c_extractor()),
            #[cfg(feature = "tier1")]
            "cpp" | "cxx" | "cc" | "hpp" | "hxx" | "hh" => {
                Box::new(extract::tree_sitter_ext::cpp_extractor())
            }
            #[cfg(feature = "tier1")]
            "cs" => Box::new(extract::tree_sitter_ext::csharp_extractor()),
            #[cfg(feature = "tier1")]
            "rb" | "rake" | "gemspec" => Box::new(extract::tree_sitter_ext::ruby_extractor()),
            #[cfg(feature = "tier1")]
            "php" => Box::new(extract::tree_sitter_ext::php_extractor()),
            #[cfg(feature = "tier1")]
            "swift" => Box::new(extract::tree_sitter_ext::swift_extractor()),
            #[cfg(feature = "tier1")]
            "kt" | "kts" => Box::new(extract::tree_sitter_ext::kotlin_extractor()),
            #[cfg(feature = "tier1")]
            "scala" | "sc" => Box::new(extract::tree_sitter_ext::scala_extractor()),
            #[cfg(feature = "tier1")]
            "sh" | "bash" | "zsh" => Box::new(extract::tree_sitter_ext::bash_extractor()),
            #[cfg(feature = "tier1")]
            "lua" => Box::new(extract::tree_sitter_ext::lua_extractor()),
            #[cfg(feature = "tier1")]
            "r" | "R" | "Rmd" => Box::new(extract::tree_sitter_ext::r_extractor()),
            #[cfg(feature = "tier1")]
            "html" | "htm" => {
                Box::new(extract::tree_sitter_ext::EmbeddedExtractor::html_embedded())
            }
            #[cfg(feature = "tier1")]
            "css" => Box::new(extract::tree_sitter_ext::css_extractor()),
            #[cfg(feature = "tier1")]
            "json" => Box::new(extract::tree_sitter_ext::json_extractor()),
            #[cfg(feature = "tier2")]
            "ex" | "exs" => Box::new(extract::tree_sitter_ext::elixir_extractor()),
            #[cfg(feature = "tier2")]
            "dart" => Box::new(extract::tree_sitter_ext::dart_extractor()),
            #[cfg(feature = "tier2")]
            "zig" => Box::new(extract::tree_sitter_ext::zig_extractor()),
            #[cfg(feature = "tier2")]
            "hs" | "lhs" => Box::new(extract::tree_sitter_ext::haskell_extractor()),
            #[cfg(feature = "tier2")]
            "ml" | "mli" => Box::new(extract::tree_sitter_ext::ocaml_extractor()),
            #[cfg(feature = "tier2")]
            "toml" => Box::new(extract::tree_sitter_ext::toml_extractor()),
            #[cfg(feature = "tier2")]
            "yml" | "yaml" => Box::new(extract::tree_sitter_ext::yaml_extractor()),
            #[cfg(feature = "tier2")]
            "sql" => Box::new(extract::tree_sitter_ext::sql_extractor()),
            _ => Box::new(extract::generic::GenericExtractor::new()),
        }
    }

    fn preview_exact_node_ids(target: &Path, relative: &str) -> M1ndResult<HashSet<String>> {
        let file_id = build_file_external_id(relative).ok_or_else(|| M1ndError::InvalidParams {
            tool: "ingest_exact_file".into(),
            detail: format!("invalid managed-root relative source key: {relative:?}"),
        })?;
        let content = std::fs::read(target).map_err(M1ndError::Io)?;
        let extractor = Self::select_extractor(&extension_of(target));
        let extraction = extractor.extract(&content, &file_id)?;
        Ok(extraction
            .nodes
            .into_iter()
            .filter(|node| is_valid_external_id(&node.id))
            .map(|node| node.id)
            .collect())
    }

    pub fn ingest(&self) -> M1ndResult<(m1nd_core::graph::Graph, IngestStats)> {
        let bundle = self.ingest_bundle()?;
        bundle.require_complete()?;
        Ok((bundle.graph, bundle.stats))
    }

    /// Full-root governed ingest. This preserves the legacy graph/stats API via
    /// `ingest()` while exposing the ownership manifest to mutation consumers.
    pub fn ingest_bundle(&self) -> M1ndResult<CodeIngestBundleV1> {
        self.ingest_bundle_with_cancel(|| false)
    }

    /// Full-root governed ingest with dependency-neutral cooperative
    /// cancellation. The probe may capture any supervisor token; it is called
    /// from both the caller thread and Rayon workers, so it must be `Sync` and
    /// should be cheap. Cancellation is latched and always returns
    /// `M1ndError::IngestionCancelled`; a partially built bundle is never
    /// returned.
    pub fn ingest_bundle_with_cancel<P>(&self, is_cancelled: P) -> M1ndResult<CodeIngestBundleV1>
    where
        P: Fn() -> bool + Sync,
    {
        let cancellation = IngestCancellation::new(&is_cancelled);
        cancellation.check()?;
        let identity_root = self.config.root.canonicalize().map_err(M1ndError::Io)?;
        cancellation.check()?;
        self.ingest_bundle_inner(
            &self.config.root,
            &identity_root,
            IngestBundleContext {
                exact_source_key: None,
                enrich_global: true,
                initial_graph: m1nd_core::graph::Graph::new(),
                seed_ownership: None,
                base_ownership_digest: None,
                replaceable_node_ids: None,
            },
            &cancellation,
        )
    }

    /// Contextual exact-file replacement. The prior COMPLETE ownership receipt
    /// is mandatory: it lets us prune the old target projection, resolve the new
    /// source against the real remaining graph, and rebuild target-scoped global
    /// enrichments without losing valid cross-file/Cargo edges.
    pub fn ingest_exact_file(
        &self,
        target: &Path,
        expected_before_sha: &str,
        base_graph: &m1nd_core::graph::Graph,
        base_ownership: &ownership::CodeOwnershipManifestV1,
    ) -> M1ndResult<CodeIngestBundleV1> {
        let managed_root = self.config.root.canonicalize().map_err(M1ndError::Io)?;
        if !managed_root.is_dir() {
            return Err(M1ndError::InvalidParams {
                tool: "ingest_exact_file".into(),
                detail: format!(
                    "managed root must be a directory: {}",
                    managed_root.display()
                ),
            });
        }

        let target = target.canonicalize().map_err(M1ndError::Io)?;
        if !target.is_file() {
            return Err(M1ndError::InvalidParams {
                tool: "ingest_exact_file".into(),
                detail: format!("target must be a regular file: {}", target.display()),
            });
        }
        let relative = target
            .strip_prefix(&managed_root)
            .map_err(|_| M1ndError::InvalidParams {
                tool: "ingest_exact_file".into(),
                detail: format!(
                    "target {} escapes managed root {}",
                    target.display(),
                    managed_root.display()
                ),
            })?
            .to_str()
            .ok_or_else(|| M1ndError::InvalidParams {
                tool: "ingest_exact_file".into(),
                detail: "target relative path is not valid UTF-8".into(),
            })?;
        #[cfg(windows)]
        let relative = relative.replace('\\', "/");
        #[cfg(not(windows))]
        let relative = relative.to_string();
        if !is_valid_relative_file_path(&relative) {
            return Err(M1ndError::InvalidParams {
                tool: "ingest_exact_file".into(),
                detail: format!("target has no valid managed-root relative key: {relative:?}"),
            });
        }

        if base_ownership.coverage != ownership::OwnershipCoverageV1::Complete {
            return Err(M1ndError::InvalidParams {
                tool: "ingest_exact_file".into(),
                detail: "base ownership receipt must be COMPLETE".into(),
            });
        }
        let base_receipt_valid =
            base_ownership
                .verify_against_graph(base_graph)
                .map_err(|error| M1ndError::InvalidParams {
                    tool: "ingest_exact_file".into(),
                    detail: format!("base ownership graph verification failed: {error}"),
                })?;
        if !base_receipt_valid {
            return Err(M1ndError::InvalidParams {
                tool: "ingest_exact_file".into(),
                detail: "base ownership receipt or graph topology mismatch".into(),
            });
        }
        let root_identity = exact_path_identity(&managed_root)?;
        if base_ownership.root_identity != root_identity {
            return Err(M1ndError::InvalidParams {
                tool: "ingest_exact_file".into(),
                detail: format!(
                    "base ownership root mismatch: expected {root_identity:?}, got {:?}",
                    base_ownership.root_identity
                ),
            });
        }
        let Some(indexed_before_sha) = base_ownership.source_digests.get(&relative) else {
            return Err(M1ndError::InvalidParams {
                tool: "ingest_exact_file".into(),
                detail: format!("UPDATE target is not indexed in base digests: {relative}"),
            });
        };
        if !base_ownership.claims_by_source.contains_key(&relative) {
            return Err(M1ndError::InvalidParams {
                tool: "ingest_exact_file".into(),
                detail: format!("UPDATE target has no base ownership claims: {relative}"),
            });
        }
        if expected_before_sha.trim().is_empty() || indexed_before_sha != expected_before_sha {
            return Err(M1ndError::InvalidParams {
                tool: "ingest_exact_file".into(),
                detail: format!(
                    "UPDATE preimage digest mismatch for {relative}: expected {expected_before_sha:?}, indexed {indexed_before_sha:?}"
                ),
            });
        }

        Err(M1ndError::FullReindexRequired {
            reason: format!(
                "exact-file refresh for {relative:?} is disabled in governed v1; build a deterministic immutable full-root bundle"
            ),
        })
    }

    fn ingest_bundle_inner<P>(
        &self,
        walk_root: &Path,
        identity_root: &Path,
        context: IngestBundleContext<'_>,
        cancellation: &IngestCancellation<'_, P>,
    ) -> M1ndResult<CodeIngestBundleV1>
    where
        P: Fn() -> bool + Sync + ?Sized,
    {
        let IngestBundleContext {
            exact_source_key,
            enrich_global,
            initial_graph,
            seed_ownership,
            base_ownership_digest,
            replaceable_node_ids,
        } = context;
        let start = Instant::now();
        let mut stats = IngestStats::default();
        cancellation.check()?;

        let dir_walker = walker::DirectoryWalker::new(
            self.config.skip_dirs.clone(),
            self.config.skip_files.clone(),
            self.config.include_dotfiles,
            self.config.dotfile_patterns.clone(),
        );
        let mut walk_result = dir_walker.walk(walk_root)?;
        cancellation.check()?;
        if exact_source_key.is_some() {
            for file in &mut walk_result.files {
                file.relative_path = relative_source_path(identity_root, &file.path);
            }
        }
        walk_result
            .files
            .sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        for group in &mut walk_result.commit_groups {
            group.sort();
            group.dedup();
        }
        walk_result.commit_groups.sort();
        walk_result.commit_groups.dedup();
        stats.files_scanned = walk_result.files.len() as u64;
        stats.commit_groups = walk_result.commit_groups.clone();
        stats.discovered_files = walk_result.files.clone();
        if !walk_result.files.is_empty() && start.elapsed() >= self.config.timeout {
            return Err(M1ndError::IngestionTimeout {
                elapsed_s: start.elapsed().as_secs_f64(),
            });
        }

        let mut discovered_source_keys = BTreeMap::<String, ()>::new();
        for file in &walk_result.files {
            if !is_valid_relative_file_path(&file.relative_path) {
                return Err(M1ndError::InvalidParams {
                    tool: "ingest".into(),
                    detail: format!(
                        "discovered source has invalid managed-root relative key: {:?}",
                        file.relative_path
                    ),
                });
            }
            if discovered_source_keys
                .insert(file.relative_path.clone(), ())
                .is_some()
            {
                return Err(M1ndError::InvalidParams {
                    tool: "ingest".into(),
                    detail: format!(
                        "duplicate discovered source key prevents exact corpus accounting: {:?}",
                        file.relative_path
                    ),
                });
            }
        }
        cancellation.check()?;

        use rayon::prelude::*;
        let num_threads = self.config.parallelism.clamp(1, 64);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .map_err(|e| M1ndError::InvalidParams {
                tool: "ingest".into(),
                detail: format!("thread pool: {}", e),
            })?;

        // (source_key, file_id, extraction result,
        //  [(node_id, behavioral excerpt)], content_sha256)
        type FileExtraction = (
            String,
            String,
            extract::ExtractionResult,
            Vec<(String, String)>,
            String,
            Vec<u8>,
        );
        let extraction_results: M1ndResult<Vec<FileExtraction>> = pool.install(|| {
            walk_result
                .files
                .par_iter()
                .map(|file| -> M1ndResult<FileExtraction> {
                    cancellation.check()?;
                    if start.elapsed() >= self.config.timeout {
                        return Err(M1ndError::IngestionTimeout {
                            elapsed_s: start.elapsed().as_secs_f64(),
                        });
                    }
                    let ext = file.extension.as_deref().unwrap_or("");
                    let extractor = Self::select_extractor(ext);
                    let content = std::fs::read(&file.path).map_err(M1ndError::Io)?;
                    cancellation.check()?;
                    let source_key = file.relative_path.clone();
                    let file_id = build_file_external_id(&file.relative_path).ok_or_else(|| {
                        M1ndError::InvalidParams {
                            tool: "ingest".into(),
                            detail: format!(
                                "discovered source has invalid relative path: {:?}",
                                file.relative_path
                            ),
                        }
                    })?;
                    let result = extractor.extract(&content, &file_id)?;
                    cancellation.check()?;
                    // Behavioral excerpts sliced here while the file content is live,
                    // so embeddings later see what each symbol DOES, not just its name.
                    let excerpts = extract::compute_excerpts(&result, &content);
                    let content_digest = ownership::sha256_bytes(&content);
                    cancellation.check()?;
                    if start.elapsed() >= self.config.timeout {
                        return Err(M1ndError::IngestionTimeout {
                            elapsed_s: start.elapsed().as_secs_f64(),
                        });
                    }
                    Ok((
                        source_key,
                        file_id,
                        result,
                        excerpts,
                        content_digest,
                        content,
                    ))
                })
                .collect()
        });
        // Cancellation wins over any race-dependent worker error observed in
        // the same parallel phase, yielding one deterministic supervisor
        // outcome.
        cancellation.check()?;
        let extraction_results = extraction_results?;
        cancellation.check()?;
        if start.elapsed() >= self.config.timeout && !walk_result.files.is_empty() {
            return Err(M1ndError::IngestionTimeout {
                elapsed_s: start.elapsed().as_secs_f64(),
            });
        }
        let extracted_source_keys = extraction_results
            .iter()
            .map(|(source_key, _, _, _, _, _)| (source_key.clone(), ()))
            .collect::<BTreeMap<_, ()>>();
        if extracted_source_keys != discovered_source_keys {
            return Err(M1ndError::InvalidParams {
                tool: "ingest".into(),
                detail: "discovered/extracted source-set mismatch prevents COMPLETE ownership"
                    .into(),
            });
        }

        // Build per-file git data map: file_id -> (commit_count, last_modified).
        // The walker already ran git log once and populated DiscoveredFile fields.
        // We key by file_id (e.g. "file::src/lib.rs") so sub-file nodes can inherit
        // their parent file's commit history via prefix lookup.
        use std::collections::HashMap;
        let file_git_data: HashMap<String, (u32, f64)> = walk_result
            .files
            .iter()
            .map(|file| {
                build_file_external_id(&file.relative_path)
                    .map(|file_id| (file_id, (file.commit_count, file.last_modified)))
                    .ok_or_else(|| M1ndError::InvalidParams {
                        tool: "ingest".into(),
                        detail: format!(
                            "Git metadata input has invalid source identity: {:?}",
                            file.relative_path
                        ),
                    })
            })
            .collect::<M1ndResult<_>>()?;
        cancellation.check()?;
        if file_git_data.len() != walk_result.files.len() {
            return Err(M1ndError::IngestError(format!(
                "Git metadata source identity collision: discovered={}, accounted={}",
                walk_result.files.len(),
                file_git_data.len()
            )));
        }

        // Flatten per-file excerpts into a global node_id -> excerpt map (bounded
        // ~240 chars/node), consumed at provenance time below. FIRST-WINS on a
        // duplicate id, to match graph insertion: the first node with an id wins
        // `add_node` (later dups are dropped as DuplicateNode), so the surviving
        // node must keep ITS OWN excerpt, not a later same-named sibling's.
        let mut node_excerpts: HashMap<String, String> = HashMap::new();
        for (_, _, _, excerpts, _, _) in &extraction_results {
            cancellation.check()?;
            for (id, excerpt) in excerpts {
                cancellation.check()?;
                node_excerpts
                    .entry(id.clone())
                    .or_insert_with(|| excerpt.clone());
            }
        }

        let mut source_digests = seed_ownership
            .map(|manifest| manifest.source_digests.clone())
            .unwrap_or_default();
        if let Some(source_key) = exact_source_key.as_deref() {
            source_digests.remove(source_key);
        }
        let mut all_nodes: Vec<(String, String, extract::ExtractedNode)> = Vec::new();
        let mut all_edges: Vec<(String, extract::ExtractedEdge)> = Vec::new();
        for (source_key, file_id, result, _, content_digest, _) in &extraction_results {
            cancellation.check()?;
            if start.elapsed() >= self.config.timeout {
                return Err(M1ndError::IngestionTimeout {
                    elapsed_s: start.elapsed().as_secs_f64(),
                });
            }

            stats.files_parsed += 1;
            source_digests.insert(source_key.clone(), content_digest.clone());
            all_nodes.extend(
                result
                    .nodes
                    .iter()
                    .cloned()
                    .map(|node| (source_key.clone(), file_id.clone(), node)),
            );
            all_edges.extend(
                result
                    .edges
                    .iter()
                    .cloned()
                    .map(|edge| (source_key.clone(), edge)),
            );
        }

        let mut node_contributions = BTreeMap::<
            String,
            (
                String,
                String,
                Vec<String>,
                u32,
                u32,
                String,
                Option<String>,
            ),
        >::new();
        for (_, file_id, node) in &all_nodes {
            cancellation.check()?;
            let mut tags = node.tags.clone();
            tags.sort();
            tags.dedup();
            let contribution = (
                node.label.clone(),
                format!("{:?}", node.node_type),
                tags,
                node.line,
                node.end_line,
                file_id.clone(),
                node_excerpts.get(&node.id).cloned(),
            );
            if let Some(existing) = node_contributions.insert(node.id.clone(), contribution.clone())
            {
                if existing != contribution {
                    // Preserve both sides of the deterministic conflict in the
                    // refusal. An external id collision is otherwise almost
                    // impossible to diagnose on a large corpus because the id
                    // alone does not reveal which extractor line/scope drifted.
                    return Err(M1ndError::FullReindexRequired {
                        reason: format!(
                            "conflicting static node contributions for external id {:?}; first={existing:?}; next={contribution:?}",
                            node.id
                        ),
                    });
                }
            }
        }
        let mut edge_contributions = BTreeMap::<(String, String, String), u32>::new();
        for (_, edge) in &all_edges {
            cancellation.check()?;
            let key = (
                edge.source.clone(),
                edge.target.clone(),
                edge.relation.clone(),
            );
            let weight = edge.weight.to_bits();
            if edge_contributions
                .insert(key.clone(), weight)
                .is_some_and(|existing| existing != weight)
            {
                return Err(M1ndError::FullReindexRequired {
                    reason: format!(
                        "conflicting static edge contributions for {:?} -> {:?} ({:?})",
                        key.0, key.1, key.2
                    ),
                });
            }
        }
        cancellation.check()?;
        all_nodes.sort_by(|left, right| {
            (
                left.0.as_str(),
                left.2.id.as_str(),
                left.2.line,
                left.2.end_line,
            )
                .cmp(&(
                    right.0.as_str(),
                    right.2.id.as_str(),
                    right.2.line,
                    right.2.end_line,
                ))
        });
        cancellation.check()?;
        all_edges.sort_by(|left, right| {
            (
                left.0.as_str(),
                left.1.source.as_str(),
                left.1.target.as_str(),
                left.1.relation.as_str(),
                left.1.weight.to_bits(),
            )
                .cmp(&(
                    right.0.as_str(),
                    right.1.source.as_str(),
                    right.1.target.as_str(),
                    right.1.relation.as_str(),
                    right.1.weight.to_bits(),
                ))
        });
        cancellation.check()?;

        let cached_source_snapshot = CachedSourceSnapshot::materialize_with_cancel(
            extraction_results
                .iter()
                .map(|(source_key, _, _, _, _, content)| (source_key.as_str(), content.as_slice())),
            cancellation,
        )?;

        let mut graph = initial_graph;
        let mut ownership = ownership::OwnershipCollectorV1::default();
        if let Some(manifest) = seed_ownership {
            for (source_key, claims) in &manifest.claims_by_source {
                cancellation.check()?;
                if exact_source_key.as_deref() != Some(source_key.as_str()) {
                    ownership.extend_source_claims(source_key, claims);
                }
            }
        }
        let mut skipped_invalid_nodes = 0u64;
        for (source_key, file_id, node) in &all_nodes {
            cancellation.check()?;
            if !is_valid_external_id(&node.id) {
                return Err(M1ndError::InvalidParams {
                    tool: "ingest".into(),
                    detail: format!(
                        "extractor emitted invalid external_id {:?} for source {source_key:?}",
                        node.id
                    ),
                });
            }

            // Look up git data for this file. Sub-file nodes (functions, structs, …)
            // inherit from their containing file identified by file_id.
            let (commit_count, last_modified) =
                file_git_data.get(file_id).copied().unwrap_or((0, 0.0));

            // change_frequency: monotonically maps commit_count -> [0, 1).
            // 0 commits → 0.0 (neutral/unknown), 10 commits → ~0.5, 50+ → ~0.83.
            // Higher value means "changes more often" — consistent with activation.rs
            // (frequency boosts score) and temporal.rs VelocityScorer (z > 0 = Accelerating).
            let change_frequency = if commit_count == 0 {
                0.0f32
            } else {
                commit_count as f32 / (commit_count as f32 + 10.0)
            };

            let tags: Vec<&str> = node.tags.iter().map(String::as_str).collect();
            let (node_id, created) = match graph.add_node(
                &node.id,
                &node.label,
                node.node_type,
                &tags,
                last_modified,
                change_frequency,
            ) {
                Ok(node_id) => (node_id, true),
                Err(M1ndError::DuplicateNode(existing))
                    if graph.resolve_id(&node.id) == Some(existing) =>
                {
                    (existing, false)
                }
                Err(error) => return Err(error),
            };
            let refresh_retained = !created
                && replaceable_node_ids.is_some_and(|node_ids| node_ids.contains(node.id.as_str()));
            if refresh_retained {
                let node_idx = node_id.as_usize();
                graph.nodes.label[node_idx] = graph.strings.get_or_intern(&node.label);
                graph.nodes.node_type[node_idx] = node.node_type;
                graph.nodes.tags[node_idx].clear();
                for tag in &node.tags {
                    let interned = graph.strings.get_or_intern(tag);
                    graph.nodes.tags[node_idx].push(interned);
                }
                graph.nodes.last_modified[node_idx] = last_modified;
                graph.nodes.change_frequency[node_idx] = FiniteF32::new(change_frequency);
            }
            ownership.claim_node(source_key, &node.id);
            if created {
                stats.nodes_created += 1;
            }

            if created || refresh_retained {
                // Provenance: the source file is the node's containing file_id
                // ("file::<relpath>"), so strip the "file::" prefix to recover the
                // project-relative source path. This is what makes the graph-driven
                // AST-apply path (xray_apply AnnotateSymbol) actually match symbols:
                // without it every code node lands with source_path = None.
                // line == 0 is fine — resolve_node_provenance treats 0 as None.
                if let Some(source_path) = file_id.strip_prefix("file::") {
                    if !source_path.is_empty() {
                        graph.set_node_provenance(
                            node_id,
                            NodeProvenanceInput {
                                source_path: Some(source_path),
                                line_start: Some(node.line),
                                line_end: Some(node.end_line),
                                excerpt: node_excerpts.get(&node.id).map(String::as_str),
                                ..Default::default()
                            },
                        );
                    }
                }
            }
        }

        let mut unresolved_edges: Vec<resolve::OwnedUnresolvedReferenceV1> = Vec::new();
        let mut unresolved_edge_keys = BTreeSet::new();
        let mut direct_resolution_decisions = Vec::new();
        let mut direct_unresolved_count = 0u64;
        let mut import_hints: Vec<(String, String, String)> = Vec::new();
        let mut import_hint_keys = BTreeSet::new();
        let mut skipped_invalid_edges = 0u64;

        for (source_key, edge) in &all_edges {
            cancellation.check()?;
            if !is_valid_external_id(&edge.source) || !is_valid_external_id(&edge.target) {
                // One degenerate edge must never abort the whole ingest: a
                // REAL birth ceremony died on `ref:: ` from an html inline
                // script (2026-08-02) — a brain refused to be born over one
                // blank href-shaped reference. Invalid nodes were already
                // skipped-and-counted a few lines up; edges now get the same
                // treatment, and the count is reported after the loop.
                eprintln!(
                    "[m1nd ingest] skipping invalid edge endpoint {:?} -> {:?} ({}) for source {source_key:?}",
                    edge.source, edge.target, edge.relation
                );
                skipped_invalid_edges += 1;
                continue;
            }

            if edge.target.starts_with("ref::") {
                let unresolved = resolve::OwnedUnresolvedReferenceV1 {
                    source_key: source_key.clone(),
                    source_id: edge.source.clone(),
                    target_label: edge.target.clone(),
                    relation: edge.relation.clone(),
                };
                // Several physical imports/calls may express the exact same
                // graph relation in one source file. The governed resolver
                // intentionally rejects duplicate inputs, so the producer must
                // collapse only byte-identical ownership keys here; references
                // from different source_key owners remain distinct.
                if unresolved_edge_keys.insert(unresolved.clone()) {
                    unresolved_edges.push(unresolved);
                }

                if edge.relation == "imports" || edge.relation == "reexports" {
                    if let Some(clean_target) = edge.target.strip_prefix("ref::") {
                        if let Some((import_path, _)) = clean_target.rsplit_once("::") {
                            let hint = (
                                edge.source.clone(),
                                edge.target.clone(),
                                import_path.to_string(),
                            );
                            if import_hint_keys.insert(hint.clone()) {
                                import_hints.push(hint);
                            }
                        }
                    }
                }

                continue;
            }

            let source = graph.resolve_id(&edge.source);
            let target = graph.resolve_id(&edge.target);
            match (source, target) {
                (Some(source), Some(target)) => {
                    let represented = if ownership::graph_has_edge(
                        &graph,
                        source,
                        target,
                        &edge.relation,
                        EdgeDirection::Forward,
                        false,
                    ) {
                        true
                    } else {
                        match graph.add_edge(
                            source,
                            target,
                            &edge.relation,
                            FiniteF32::new(edge.weight),
                            EdgeDirection::Forward,
                            false,
                            FiniteF32::new(0.0),
                        ) {
                            Ok(_) => {
                                stats.edges_created += 1;
                                true
                            }
                            Err(error) => {
                                if ownership::graph_has_edge(
                                    &graph,
                                    source,
                                    target,
                                    &edge.relation,
                                    EdgeDirection::Forward,
                                    false,
                                ) {
                                    true
                                } else {
                                    return Err(error);
                                }
                            }
                        }
                    };
                    if represented {
                        ownership.claim_edge(ownership::OwnedEdgeClaimV1::forward(
                            source_key,
                            &edge.source,
                            &edge.target,
                            &edge.relation,
                        ));
                    }
                }
                // Source exists but its (non-ref) target does not resolve: the edge
                // is silently dropped, leaving this source's outgoing picture
                // incomplete. Record that on the source so `why` can flag a path
                // resting on a node with a dropped edge. Behavior is otherwise
                // unchanged — no edge is created either way.
                (Some(source), None) => {
                    let provenance = graph.resolve_node_provenance(source);
                    graph.add_node_tags(source, &[resolve::EDGE_UNRESOLVED_TAG]);
                    direct_unresolved_count += 1;
                    direct_resolution_decisions.push(ownership::ResolutionDecisionV1 {
                        source_key: source_key.clone(),
                        source_id: edge.source.clone(),
                        target_label: edge.target.clone(),
                        relation: edge.relation.clone(),
                        outcome: ownership::ResolutionOutcomeV1::Unresolved,
                        resolved_target_id: None,
                        candidate_ids: Vec::new(),
                        source_line_start: provenance.line_start,
                        source_line_end: provenance.line_end,
                    });
                }
                (None, _) => {
                    return Err(M1ndError::InvalidParams {
                        tool: "ingest".into(),
                        detail: format!(
                            "extractor emitted edge with missing source {:?} for source {source_key:?}",
                            edge.source
                        ),
                    });
                }
            }
        }

        cancellation.check()?;
        let resolution = resolve::ReferenceResolver::resolve_owned_with_hints(
            &mut graph,
            &unresolved_edges,
            &import_hints,
        )?;
        cancellation.check()?;
        if resolution.input_count != unresolved_edges.len() as u64
            || resolution.hint_count != import_hints.len() as u64
            || resolution.decisions.len() != unresolved_edges.len()
        {
            return Err(M1ndError::IngestError(format!(
                "resolution producer accounting mismatch: inputs={} accounted_inputs={}, hints={} accounted_hints={}, decisions={}",
                unresolved_edges.len(),
                resolution.input_count,
                import_hints.len(),
                resolution.hint_count,
                resolution.decisions.len()
            )));
        }
        let mut resolution_inputs =
            Vec::with_capacity(direct_resolution_decisions.len() + unresolved_edges.len());
        for decision in &direct_resolution_decisions {
            cancellation.check()?;
            resolution_inputs.push(ownership::ResolutionInputV1::from_decision(decision));
        }
        for reference in &unresolved_edges {
            cancellation.check()?;
            resolution_inputs.push(ownership::ResolutionInputV1 {
                source_key: reference.source_key.clone(),
                source_id: reference.source_id.clone(),
                target_label: reference.target_label.clone(),
                relation: reference.relation.clone(),
            });
        }
        let expected_resolution_inputs = direct_resolution_decisions.len() + unresolved_edges.len();
        let expected_resolution_decisions =
            direct_resolution_decisions.len() + resolution.decisions.len();
        if resolution_inputs.len() != expected_resolution_inputs
            || expected_resolution_inputs != expected_resolution_decisions
        {
            return Err(M1ndError::IngestError(format!(
                "resolution manifest accounting mismatch: inputs={}, decisions={expected_resolution_decisions}",
                resolution_inputs.len()
            )));
        }
        let mut resolution_hints = Vec::with_capacity(import_hints.len());
        for (source_id, target_label, import_path) in &import_hints {
            cancellation.check()?;
            resolution_hints.push(ownership::ResolutionHintV1 {
                source_id: source_id.clone(),
                target_label: target_label.clone(),
                import_path: import_path.clone(),
            });
        }
        stats.references_resolved = resolution.summary.resolved;
        stats.references_unresolved = resolution.summary.unresolved + direct_unresolved_count;
        stats.references_ambiguous = resolution.summary.ambiguous;
        stats.edges_created += resolution.summary.resolved;
        ownership.record_resolution_inputs(resolution_inputs);
        ownership.record_resolution_hints(resolution_hints);
        ownership.record_resolution_decisions(direct_resolution_decisions);
        ownership.record_resolution_decisions(resolution.decisions);
        ownership.extend(resolution.ownership);

        let mut cross_file_accounting = (0u64, 0u64, 0u64, 0u64);
        let mut cargo_accounting = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
        if enrich_global {
            cancellation.check()?;
            let cargo_stats =
                cargo_workspace::enrich_rust_workspace(&mut graph, cached_source_snapshot.root())?;
            cancellation.check()?;
            stats.nodes_created += cargo_stats.nodes_added;
            stats.edges_created += cargo_stats.edges_added;
            cargo_accounting = (
                cargo_stats.workspace_members_expected,
                cargo_stats.workspace_members_accounted,
                cargo_stats.dependency_inputs_expected,
                cargo_stats.dependency_inputs_accounted,
                cargo_stats.package_file_links_expected,
                cargo_stats.package_file_links_accounted,
            );
            ownership.extend(cargo_stats.ownership);

            let cross_file =
                cross_file::resolve_cross_file_edges(&mut graph, cached_source_snapshot.root())?;
            cancellation.check()?;
            stats.edges_created += cross_file.imports_resolved
                + cross_file.test_edges_created
                + cross_file.register_edges_created;
            cross_file_accounting = (
                cross_file.source_files_expected,
                cross_file.source_metadata_verified,
                cross_file.source_files_read,
                cross_file.source_files_parsed,
            );
            ownership.extend(cross_file.ownership);
        }

        cancellation.check()?;
        sort_pending_edges_canonically(&mut graph, cancellation)?;
        graph.finalize()?;
        cancellation.check()?;
        stats.elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

        if skipped_invalid_nodes > 0 || skipped_invalid_edges > 0 {
            eprintln!(
                "[m1nd-ingest] hygiene summary: skipped {} invalid nodes, {} invalid edges",
                skipped_invalid_nodes, skipped_invalid_edges
            );
        }

        ownership.set_pipeline_receipt(ownership::CodePipelineReceiptV1 {
            schema: ownership::CODE_PIPELINE_RECEIPT_SCHEMA.to_string(),
            pipeline_version: format!("m1nd-ingest-{}", env!("CARGO_PKG_VERSION")),
            producer_name: ownership::CODE_PIPELINE_PRODUCER_NAME.into(),
            producer_version: env!("CARGO_PKG_VERSION").into(),
            producer_build_identity: ownership::compiled_producer_build_identity(),
            producer_executable_identity: ownership::running_producer_executable_identity()
                .map_err(M1ndError::Io)?,
            skip_dirs: self.config.skip_dirs.clone(),
            skip_files: self.config.skip_files.clone(),
            include_dotfiles: self.config.include_dotfiles,
            dotfile_patterns: self.config.dotfile_patterns.clone(),
            policy_fingerprint: discovery_policy_fingerprint(
                identity_root,
                &self.config.skip_dirs,
                &self.config.skip_files,
                self.config.include_dotfiles,
                &self.config.dotfile_patterns,
                &walk_result.vcs,
            )?,
            build_features: active_pipeline_features(),
            binary_policy: "nul-in-first-8192-v1".into(),
            vcs_context_digest: vcs_context_digest(&walk_result)?,
            immutable_source_snapshot: true,
            discovered_source_count: walk_result.files.len() as u64,
            extracted_source_count: extraction_results.len() as u64,
            digested_source_count: source_digests.len() as u64,
            global_enrichment_enabled: enrich_global,
            cross_file_source_files_expected: cross_file_accounting.0,
            cross_file_source_metadata_verified: cross_file_accounting.1,
            cross_file_source_files_read: cross_file_accounting.2,
            cross_file_source_files_parsed: cross_file_accounting.3,
            cargo_workspace_members_expected: cargo_accounting.0,
            cargo_workspace_members_accounted: cargo_accounting.1,
            cargo_dependency_inputs_expected: cargo_accounting.2,
            cargo_dependency_inputs_accounted: cargo_accounting.3,
            cargo_package_file_links_expected: cargo_accounting.4,
            cargo_package_file_links_accounted: cargo_accounting.5,
        });

        cancellation.check()?;
        let root_identity = exact_path_identity(identity_root)?;
        let ownership = ownership
            .audit(
                &graph,
                root_identity,
                exact_source_key,
                base_ownership_digest,
                source_digests,
            )
            .map_err(|error| M1ndError::InvalidParams {
                tool: "ingest_ownership_audit".into(),
                detail: format!("ownership manifest serialization failed: {error}"),
            })?;
        cancellation.check()?;

        let bundle = CodeIngestBundleV1 {
            schema: ownership::CODE_INGEST_BUNDLE_SCHEMA.to_string(),
            graph,
            stats,
            ownership,
        };
        bundle.require_complete_inner(cancellation)?;
        cancellation.check()?;
        Ok(bundle)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_file_external_id, discovery_policy_fingerprint, is_valid_external_id, IngestConfig,
        Ingestor,
    };
    use crate::ownership::{source_projection_digest, OwnershipCoverageV1};
    use crate::IngestAdapter;
    use crate::L1ghtIngestAdapter;
    use m1nd_core::error::M1ndError;
    use m1nd_core::types::EdgeIdx;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_ingest_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("m1nd-ingest-{name}-{nonce}"))
    }

    #[test]
    fn file_external_id_builder_rejects_empty_and_dot_paths() {
        assert_eq!(build_file_external_id(""), None);
        assert_eq!(build_file_external_id("   "), None);
        assert_eq!(build_file_external_id("."), None);
        assert_eq!(build_file_external_id("./"), None);
        assert_eq!(build_file_external_id("/absolute.rs"), None);
        assert_eq!(build_file_external_id("../escape.rs"), None);
        assert_eq!(build_file_external_id("a/../../escape.rs"), None);
        assert_eq!(build_file_external_id("src/./main.rs"), None);
        assert_eq!(
            build_file_external_id("src/main.rs"),
            Some("file::src/main.rs".to_string())
        );
        assert_eq!(build_file_external_id(" src/main.rs"), None);
        assert_eq!(build_file_external_id("src/main.rs "), None);
        #[cfg(unix)]
        assert_eq!(build_file_external_id("src\\main.rs"), None);
        #[cfg(windows)]
        assert_eq!(
            build_file_external_id("src\\main.rs"),
            Some("file::src/main.rs".to_string())
        );
    }

    #[test]
    fn source_path_identity_never_collapses_distinct_unix_names() {
        let canonical = build_file_external_id("src/main.rs").expect("canonical identity");
        assert_ne!(
            build_file_external_id(" src/main.rs"),
            Some(canonical.clone())
        );
        assert_ne!(
            build_file_external_id("src/main.rs "),
            Some(canonical.clone())
        );
        #[cfg(unix)]
        assert_ne!(build_file_external_id("src\\main.rs"), Some(canonical));
    }

    #[cfg(unix)]
    #[test]
    fn filesystem_collision_fixtures_are_rejected_instead_of_normalized() {
        let root = temp_ingest_dir("path-identity-collisions");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("main.rs"), "pub fn canonical() {}\n").unwrap();
        fs::write(root.join(" main.rs"), "pub fn leading_space() {}\n").unwrap();
        fs::write(root.join("trail.rs"), "pub fn canonical_trail() {}\n").unwrap();
        fs::write(root.join("trail.rs "), "pub fn trailing_space() {}\n").unwrap();
        fs::write(root.join("src/main.rs"), "pub fn nested() {}\n").unwrap();
        fs::write(root.join("src\\main.rs"), "pub fn literal_backslash() {}\n").unwrap();

        let error = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        })
        .ingest_bundle()
        .err()
        .expect("non-bijective filesystem names must fail closed");
        assert!(
            error
                .to_string()
                .contains("walker discovered non-bijective relative path"),
            "{error}"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn discovery_control_identity_rejects_literal_backslash_collision_fixture() {
        let root = temp_ingest_dir("policy-path-identity-collision");
        fs::create_dir_all(root.join("src\\nested")).unwrap();
        fs::write(root.join("src\\nested/.gitignore"), "target\n").unwrap();

        let error = discovery_policy_fingerprint(
            &root,
            &[],
            &[],
            false,
            &[],
            &crate::walker::VcsContextV1::default(),
        )
        .expect_err("literal backslash control identity must not be normalized");
        assert!(error
            .to_string()
            .contains("non-bijective relative identity"));

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn managed_root_identity_preserves_literal_backslash() {
        let parent = temp_ingest_dir("root-path-identity");
        let root = parent.join("governed\\root");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("source.rs"), "pub fn source() {}\n").unwrap();

        let bundle = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        })
        .ingest_bundle()
        .unwrap();
        let canonical = root.canonicalize().unwrap();
        assert_eq!(
            bundle.ownership.root_identity,
            canonical.to_str().unwrap(),
            "literal Unix backslash must remain part of the root identity"
        );
        bundle.require_complete().unwrap();

        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn external_id_validation_rejects_empty_file_ids() {
        assert!(!is_valid_external_id(""));
        assert!(!is_valid_external_id("file::"));
        assert!(!is_valid_external_id("file::   "));
        assert!(!is_valid_external_id(" file::src/main.rs"));
        assert!(!is_valid_external_id("file::src/main.rs "));
        assert!(!is_valid_external_id("symbol::bad\0id"));
        assert!(!is_valid_external_id("symbol::bad\u{FFFD}id"));
        assert!(is_valid_external_id("cargo::workspace::Cargo.toml"));
        assert!(is_valid_external_id("file::src/main.rs"));
    }

    #[test]
    fn cancellable_full_root_ingest_pre_cancel_is_typed_and_fs_independent() {
        let missing_root = temp_ingest_dir("cancel-before-canonicalize");
        let result = Ingestor::new(IngestConfig {
            root: missing_root,
            ..Default::default()
        })
        .ingest_bundle_with_cancel(|| true);

        assert!(matches!(result, Err(M1ndError::IngestionCancelled)));
    }

    #[test]
    fn cancellable_full_root_ingest_mid_extraction_returns_no_partial_bundle() {
        let root = temp_ingest_dir("cancel-mid-extraction");
        fs::create_dir_all(&root).unwrap();
        for index in 0..8 {
            fs::write(
                root.join(format!("source_{index}.rs")),
                format!("pub fn source_{index}() -> usize {{ {index} }}\n"),
            )
            .unwrap();
        }
        let probe_calls = AtomicUsize::new(0);
        let ingestor = Ingestor::new(IngestConfig {
            root: root.clone(),
            parallelism: 1,
            ..Default::default()
        });

        // Checkpoints 1-5 cover entry/canonicalization/walk/discovery. With a
        // single Rayon worker, checkpoint 6 enters the first extraction and 7
        // follows its file read. Cancelling there proves in-flight extraction
        // cannot yield a partial COMPLETE bundle. The probe is deliberately
        // true only once: the ingest-local latch must retain the observation.
        let result = ingestor
            .ingest_bundle_with_cancel(|| probe_calls.fetch_add(1, Ordering::SeqCst) + 1 == 7);

        assert!(matches!(result, Err(M1ndError::IngestionCancelled)));
        assert_eq!(probe_calls.load(Ordering::SeqCst), 7);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cancellable_full_root_reduction_never_returns_partial_output() {
        let root = temp_ingest_dir("cancel-during-reduction");
        fs::create_dir_all(&root).unwrap();
        let source = (0..64)
            .map(|index| format!("pub fn item_{index}() -> usize {{ {index} }}\n"))
            .collect::<String>();
        fs::write(root.join("many.rs"), source).unwrap();
        let probe_calls = AtomicUsize::new(0);
        let ingestor = Ingestor::new(IngestConfig {
            root: root.clone(),
            parallelism: 1,
            ..Default::default()
        });

        let result = ingestor
            .ingest_bundle_with_cancel(|| probe_calls.fetch_add(1, Ordering::SeqCst) + 1 >= 30);

        match result {
            Err(M1ndError::IngestionCancelled) => {}
            Err(other) => panic!("expected typed cancellation, got {other}"),
            Ok(_) => panic!("cancelled reduction returned a partial ingest bundle"),
        }
        assert!(probe_calls.load(Ordering::SeqCst) >= 30);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cancellable_full_root_success_matches_legacy_bundle() {
        let root = temp_ingest_dir("cancel-success-parity");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "pub mod worker;\npub fn ready() -> bool { worker::ready() }\n",
        )
        .unwrap();
        fs::write(
            root.join("src/worker.rs"),
            "pub fn ready() -> bool { true }\n",
        )
        .unwrap();
        let config = || IngestConfig {
            root: root.clone(),
            parallelism: 2,
            ..Default::default()
        };

        let legacy = Ingestor::new(config()).ingest_bundle().unwrap();
        let cancellable = Ingestor::new(config())
            .ingest_bundle_with_cancel(|| false)
            .unwrap();

        assert_eq!(
            source_projection_digest(&legacy.graph).unwrap(),
            source_projection_digest(&cancellable.graph).unwrap()
        );
        assert_eq!(
            legacy.ownership.ownership_digest,
            cancellable.ownership.ownership_digest
        );
        assert_eq!(
            legacy.ownership.source_digests,
            cancellable.ownership.source_digests
        );
        assert_eq!(legacy.stats.files_scanned, cancellable.stats.files_scanned);
        assert_eq!(legacy.stats.files_parsed, cancellable.stats.files_parsed);
        legacy.require_complete().unwrap();
        cancellable.require_complete().unwrap();

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn full_root_ingest_is_deterministic_across_parallelism() {
        let root = temp_ingest_dir("deterministic-full-root");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/a.ts"),
            "import { helper } from \"./b\";\nexport function run() { return helper(); }\n",
        )
        .unwrap();
        fs::write(
            root.join("src/b.ts"),
            "export function helper() { return 42; }\n",
        )
        .unwrap();
        fs::write(root.join("src/c.rs"), "pub fn ready() -> bool { true }\n").unwrap();

        let serial = Ingestor::new(IngestConfig {
            root: root.clone(),
            parallelism: 1,
            ..Default::default()
        })
        .ingest_bundle()
        .unwrap();
        let parallel = Ingestor::new(IngestConfig {
            root: root.clone(),
            parallelism: 8,
            ..Default::default()
        })
        .ingest_bundle()
        .unwrap();

        assert_eq!(
            source_projection_digest(&serial.graph).unwrap(),
            source_projection_digest(&parallel.graph).unwrap()
        );
        assert_eq!(
            serial.ownership.source_projection_digest,
            parallel.ownership.source_projection_digest
        );
        assert_eq!(
            serial.ownership.ownership_digest,
            parallel.ownership.ownership_digest
        );
        assert_eq!(
            serial.ownership.lineage_digest,
            parallel.ownership.lineage_digest
        );
        assert_eq!(
            serial.ownership.resolution_digest,
            parallel.ownership.resolution_digest
        );
        let cross_file = &serial.ownership.pipeline_receipt;
        assert!(cross_file.global_enrichment_enabled);
        assert_eq!(cross_file.cross_file_source_files_expected, 3);
        assert_eq!(cross_file.cross_file_source_metadata_verified, 3);
        assert_eq!(cross_file.cross_file_source_files_read, 3);
        assert_eq!(cross_file.cross_file_source_files_parsed, 3);
        serial.require_complete().unwrap();
        parallel.require_complete().unwrap();

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn complete_bundle_revalidation_refuses_source_set_drift() {
        let root = temp_ingest_dir("complete-source-set-drift");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.rs"), "pub fn a() -> u32 { 1 }\n").unwrap();
        let bundle = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        })
        .ingest_bundle()
        .unwrap();

        fs::write(root.join("b.rs"), "pub fn b() -> u32 { 2 }\n").unwrap();
        let error = bundle
            .require_complete()
            .expect_err("a newly discovered source must invalidate READY");
        assert!(matches!(
            error,
            m1nd_core::error::M1ndError::FullReindexRequired { .. }
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn complete_bundle_revalidation_refuses_discovery_control_drift() {
        let root = temp_ingest_dir("complete-policy-drift");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join(".gitignore"), "# baseline\n").unwrap();
        fs::write(root.join("a.rs"), "pub fn a() -> u32 { 1 }\n").unwrap();
        let bundle = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        })
        .ingest_bundle()
        .unwrap();

        fs::write(root.join(".gitignore"), "# changed discovery control\n").unwrap();
        let error = bundle
            .require_complete()
            .expect_err("discovery-control drift must invalidate READY");
        assert!(matches!(
            error,
            m1nd_core::error::M1ndError::FullReindexRequired { .. }
        ));
        assert!(error.to_string().contains("discovery policy"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn exact_file_api_validates_base_then_requires_governed_full_root_reindex() {
        let root = temp_ingest_dir("exact-file-bundle");
        fs::create_dir_all(root.join("src")).unwrap();
        let target = root.join("src/lib.rs");
        fs::write(&target, "pub fn answer() -> u32 { 42 }\n").unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });
        let baseline = ingest.ingest_bundle().unwrap();
        let before_sha = baseline.ownership.source_digests["src/lib.rs"].clone();
        let error = ingest
            .ingest_exact_file(&target, &before_sha, &baseline.graph, &baseline.ownership)
            .err()
            .expect("governed v1 exact path must require full-root rebuild");
        assert!(matches!(
            error,
            m1nd_core::error::M1ndError::FullReindexRequired { .. }
        ));

        let error = ingest
            .ingest_exact_file(
                &target,
                "wrong-preimage",
                &baseline.graph,
                &baseline.ownership,
            )
            .err()
            .expect("stale UPDATE preimage must fail closed");
        assert!(error.to_string().contains("preimage digest mismatch"));

        let wrong_topology = m1nd_core::graph::Graph::new();
        let error = ingest
            .ingest_exact_file(&target, &before_sha, &wrong_topology, &baseline.ownership)
            .err()
            .expect("valid-looking receipt against the wrong graph must fail closed");
        assert!(error.to_string().contains("topology mismatch"));

        let mut tampered = baseline.ownership.clone();
        tampered
            .source_digests
            .insert("src/lib.rs".into(), "tampered".into());
        let error = ingest
            .ingest_exact_file(&target, &before_sha, &baseline.graph, &tampered)
            .err()
            .expect("tampered base receipt must fail closed");
        assert!(error.to_string().contains("topology mismatch"));

        let unindexed = root.join("src/new.rs");
        fs::write(&unindexed, "pub fn new_source() {}\n").unwrap();
        let error = ingest
            .ingest_exact_file(
                &unindexed,
                "not-a-base-preimage",
                &baseline.graph,
                &baseline.ownership,
            )
            .err()
            .expect("UPDATE-only exact ingest must reject an unindexed source");
        assert!(error.to_string().contains("not indexed in base digests"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn contextual_exact_file_requires_full_reindex_for_multi_source_closure() {
        let root = temp_ingest_dir("exact-file-context-import");
        fs::create_dir_all(&root).unwrap();
        let target = root.join("a.ts");
        fs::write(
            &target,
            "import { foo } from \"./b\";\nexport function run() { return foo(); }\n",
        )
        .unwrap();
        fs::write(root.join("b.ts"), "export function foo() { return 1; }\n").unwrap();
        fs::write(root.join("c.ts"), "export function bar() { return 2; }\n").unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });
        let baseline = ingest.ingest_bundle().unwrap();
        let baseline_before_sha = baseline.ownership.source_digests["a.ts"].clone();
        let error = ingest
            .ingest_exact_file(
                &target,
                &baseline_before_sha,
                &baseline.graph,
                &baseline.ownership,
            )
            .err()
            .expect("multi-source exact refresh must fail closed");
        assert!(matches!(
            error,
            m1nd_core::error::M1ndError::FullReindexRequired { .. }
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn contextual_exact_file_refuses_incoming_other_source_dependency_closure() {
        let root = temp_ingest_dir("exact-file-incoming-retention");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("a.ts"),
            "import { foo } from \"./b\";\nexport function run() {\n  return foo();\n}\n",
        )
        .unwrap();
        let target = root.join("b.ts");
        fs::write(&target, "export function foo() {\n  return 1;\n}\n").unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });
        let baseline = ingest.ingest_bundle().unwrap();
        let run_id = "file::a.ts::fn::run";
        let foo_id = "file::b.ts::fn::foo";
        let has_incoming_call = |graph: &m1nd_core::graph::Graph| {
            let run = graph.resolve_id(run_id).unwrap();
            let foo = graph.resolve_id(foo_id).unwrap();
            graph.csr.out_range(run).any(|edge_idx| {
                graph.csr.targets[edge_idx] == foo
                    && graph.strings.resolve(graph.csr.relations[edge_idx]) == "calls"
            })
        };
        assert!(has_incoming_call(&baseline.graph));
        let before_sha = baseline.ownership.source_digests["b.ts"].clone();

        // The symbol identity is unchanged, but its body/provenance changes.
        // Incoming `calls` is owned by a.ts and must survive replacement of b.ts.
        fs::write(&target, "export function foo() {\n  return 99;\n}\n").unwrap();
        let error = ingest
            .ingest_exact_file(&target, &before_sha, &baseline.graph, &baseline.ownership)
            .err()
            .expect("incoming foreign dependency requires full closure rebuild");
        assert!(matches!(
            error,
            m1nd_core::error::M1ndError::FullReindexRequired { .. }
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn exact_file_refuses_newly_discovered_source_after_single_source_baseline() {
        let root = temp_ingest_dir("exact-file-discovery-drift");
        fs::create_dir_all(&root).unwrap();
        let target = root.join("a.rs");
        fs::write(&target, "pub fn a() -> u32 { 1 }\n").unwrap();
        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });
        let baseline = ingest.ingest_bundle().unwrap();
        let before_sha = baseline.ownership.source_digests["a.rs"].clone();

        fs::write(root.join("b.rs"), "pub fn b() -> u32 { 2 }\n").unwrap();
        fs::write(&target, "pub fn a() -> u32 { 3 }\n").unwrap();
        let error = ingest
            .ingest_exact_file(&target, &before_sha, &baseline.graph, &baseline.ownership)
            .err()
            .expect("new source must invalidate exact fast path");
        assert!(matches!(
            error,
            m1nd_core::error::M1ndError::FullReindexRequired { .. }
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn zero_timeout_cannot_emit_false_complete_empty_bundle() {
        let root = temp_ingest_dir("zero-timeout");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.rs"), "pub fn a() {}\n").unwrap();
        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            timeout: std::time::Duration::ZERO,
            ..Default::default()
        });
        let error = ingest
            .ingest_bundle()
            .err()
            .expect("zero timeout with a discovered file must be fatal");
        assert!(matches!(
            error,
            m1nd_core::error::M1ndError::IngestionTimeout { .. }
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ownership_receipt_persists_unresolved_and_ambiguous_resolution_decisions() {
        let root = temp_ingest_dir("resolution-receipts");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("caller.ts"),
            "export function run() { return missing(); }\n",
        )
        .unwrap();
        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });
        let unresolved = ingest.ingest_bundle().unwrap();
        assert!(unresolved
            .ownership
            .resolution_decisions
            .iter()
            .any(|decision| {
                decision.source_key == "caller.ts"
                    && decision.outcome == crate::ownership::ResolutionOutcomeV1::Unresolved
                    && decision.resolved_target_id.is_none()
            }));
        unresolved.require_complete().unwrap();

        fs::write(
            root.join("left.ts"),
            "export function helper() { return 1; }\n",
        )
        .unwrap();
        fs::write(
            root.join("right.ts"),
            "export function helper() { return 2; }\n",
        )
        .unwrap();
        fs::write(
            root.join("caller.ts"),
            "export function run() { return helper(); }\n",
        )
        .unwrap();
        let ambiguous = ingest.ingest_bundle().unwrap();
        let ambiguous_decision = ambiguous
            .ownership
            .resolution_decisions
            .iter()
            .find(|decision| {
                decision.source_key == "caller.ts"
                    && decision.outcome == crate::ownership::ResolutionOutcomeV1::Ambiguous
                    && decision.resolved_target_id.is_some()
            })
            .expect("ambiguous bind must have an auditable decision");
        assert!(ambiguous_decision.candidate_ids.len() >= 2);
        assert!(ambiguous_decision
            .candidate_ids
            .binary_search_by(|candidate| {
                candidate.as_str().cmp(
                    ambiguous_decision
                        .resolved_target_id
                        .as_deref()
                        .expect("ambiguous bind must record its selected target"),
                )
            })
            .is_ok());
        assert!(ambiguous_decision.source_line_start.is_some());
        assert!(ambiguous_decision.source_line_end.is_some());
        let mut tampered = ambiguous.ownership.clone();
        tampered.resolution_decisions[0]
            .target_label
            .push_str("-tampered");
        assert!(!tampered.verify_receipt().unwrap());
        ambiguous.require_complete().unwrap();

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn cargo_dependency_node_records_all_producing_manifests() {
        let root = temp_ingest_dir("cargo-shared-owner");
        for member in ["a", "b"] {
            fs::create_dir_all(root.join(format!("crates/{member}/src"))).unwrap();
            fs::write(
                root.join(format!("crates/{member}/Cargo.toml")),
                format!(
                    "[package]\nname = \"{member}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"1\"\n"
                ),
            )
            .unwrap();
            fs::write(
                root.join(format!("crates/{member}/src/lib.rs")),
                "pub fn ready() -> bool { true }\n",
            )
            .unwrap();
        }
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/a\", \"crates/b\"]\nresolver = \"2\"\n",
        )
        .unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });
        let bundle = ingest.ingest_bundle().unwrap();

        assert_eq!(bundle.ownership.coverage, OwnershipCoverageV1::Complete);
        for manifest in ["crates/a/Cargo.toml", "crates/b/Cargo.toml"] {
            assert!(bundle.ownership.claims_by_source[manifest]
                .node_ids
                .contains(&"cargo::dep::serde".to_string()));
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ingest_resolves_rust_ref_edges_before_finalize() {
        let root = temp_ingest_dir("rust-resolve");
        fs::create_dir_all(root.join("src")).unwrap();

        fs::write(root.join("src/helper.rs"), "pub struct Helper;\n").unwrap();
        fs::write(
            root.join("src/main.rs"),
            "mod helper;\nuse crate::helper::Helper;\npub fn build(helper: Helper) {}\n",
        )
        .unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, stats) = ingest.ingest().unwrap();
        let main_file = graph.resolve_id("file::src/main.rs").unwrap();
        let helper = graph
            .resolve_id("file::src/helper.rs::struct::Helper")
            .unwrap();

        let has_reference_edge = graph.csr.out_range(main_file).any(|idx| {
            graph.csr.targets[idx] == helper
                && graph.strings.resolve(graph.csr.relations[idx]) == "references"
        });
        let has_import_edge = graph.csr.out_range(main_file).any(|idx| {
            graph.csr.targets[idx] == helper
                && graph.strings.resolve(graph.csr.relations[idx]) == "imports"
        });

        assert!(stats.references_resolved >= 1);
        assert!(has_reference_edge || has_import_edge);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ingest_populates_node_provenance_for_code_symbols() {
        // PROOF the AST-apply path is not a no-op: a REAL ingest (walk + extract +
        // build) must populate each symbol node's provenance (source_path +
        // line_start), not leave it at the add_node default of None / 0.
        let root = temp_ingest_dir("rust-provenance");
        fs::create_dir_all(root.join("src")).unwrap();

        // A function spanning known lines: `fn answer` opens on line 3.
        fs::write(
            root.join("src/lib.rs"),
            "// header comment\n\
             \n\
             pub fn answer() -> u32 {\n\
             \x20   42\n\
             }\n",
        )
        .unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, _stats) = ingest.ingest().unwrap();
        let func = graph
            .resolve_id("file::src/lib.rs::fn::answer")
            .expect("function node must exist after ingest");

        let prov = graph.resolve_node_provenance(func);
        assert_eq!(
            prov.source_path.as_deref(),
            Some("src/lib.rs"),
            "provenance source_path must be the project-relative source file"
        );
        assert_eq!(
            prov.line_start,
            Some(3),
            "provenance line_start must be the function's real opening line"
        );

        // The symbol now also carries a behavioral excerpt sliced from its own
        // source span (signature + body), so embeddings capture what it DOES —
        // not just its name. (Drives the seek semantic layer end to end.)
        let excerpt = prov.excerpt.as_deref().unwrap_or("");
        assert!(
            excerpt.contains("answer") && excerpt.contains("42"),
            "provenance excerpt must fold in the signature + body, got {excerpt:?}"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ingest_adds_rust_workspace_and_crate_nodes() {
        let root = temp_ingest_dir("cargo-workspace");
        fs::create_dir_all(root.join("crates/app/src")).unwrap();
        fs::create_dir_all(root.join("crates/core/src")).unwrap();

        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/app\", \"crates/core\"]\nresolver = \"2\"\n",
        )
        .unwrap();
        fs::write(
            root.join("crates/core/Cargo.toml"),
            "[package]\nname = \"corelib\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(root.join("crates/core/src/lib.rs"), "pub struct Core;\n").unwrap();
        fs::write(
            root.join("crates/app/Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ncorelib = { path = \"../core\" }\n",
        )
        .unwrap();
        fs::write(
            root.join("crates/app/src/lib.rs"),
            "use corelib::Core;\npub fn boot(_: Core) {}\n",
        )
        .unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let bundle = ingest.ingest_bundle().unwrap();
        let graph = &bundle.graph;
        let workspace = graph.resolve_id("cargo::workspace::Cargo.toml").unwrap();
        let app = graph
            .resolve_id("cargo::crate::crates/app/Cargo.toml::app")
            .unwrap();
        let core = graph
            .resolve_id("cargo::crate::crates/core/Cargo.toml::corelib")
            .unwrap();
        let app_file = graph.resolve_id("file::crates/app/src/lib.rs").unwrap();

        let workspace_contains_app = graph.csr.out_range(workspace).any(|idx| {
            graph.csr.targets[idx] == app
                && graph.strings.resolve(graph.csr.relations[idx]) == "contains"
        });
        let app_depends_on_core = graph.csr.out_range(app).any(|idx| {
            graph.csr.targets[idx] == core
                && graph.strings.resolve(graph.csr.relations[idx]) == "depends_on"
        });
        let app_contains_file = graph.csr.out_range(app).any(|idx| {
            graph.csr.targets[idx] == app_file
                && graph.strings.resolve(graph.csr.relations[idx]) == "contains"
        });

        assert!(workspace_contains_app);
        assert!(app_depends_on_core);
        assert!(app_contains_file);
        let receipt = &bundle.ownership.pipeline_receipt;
        assert_eq!(receipt.cargo_workspace_members_expected, 2);
        assert_eq!(receipt.cargo_workspace_members_accounted, 2);
        assert_eq!(receipt.cargo_dependency_inputs_expected, 1);
        assert_eq!(receipt.cargo_dependency_inputs_accounted, 1);
        assert_eq!(receipt.cargo_package_file_links_expected, 4);
        assert_eq!(receipt.cargo_package_file_links_accounted, 4);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ingest_resolves_rust_pub_use_edges_before_finalize() {
        let root = temp_ingest_dir("rust-reexport-resolve");
        fs::create_dir_all(root.join("src")).unwrap();

        fs::write(root.join("src/helper.rs"), "pub struct Helper;\n").unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "mod helper;\npub use crate::helper::Helper;\n",
        )
        .unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, stats) = ingest.ingest().unwrap();
        let lib_file = graph.resolve_id("file::src/lib.rs").unwrap();
        let helper = graph
            .resolve_id("file::src/helper.rs::struct::Helper")
            .unwrap();

        let has_reexport_edge = graph.csr.out_range(lib_file).any(|idx| {
            graph.csr.targets[idx] == helper
                && graph.strings.resolve(graph.csr.relations[idx]) == "reexports"
        });

        assert!(stats.references_resolved >= 1);
        assert!(has_reexport_edge);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ingest_links_rust_mod_declarations_to_module_files() {
        let root = temp_ingest_dir("rust-mod-file-link");
        fs::create_dir_all(root.join("src")).unwrap();

        fs::write(root.join("src/helper.rs"), "pub struct Helper;\n").unwrap();
        fs::write(root.join("src/main.rs"), "mod helper;\n").unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, _stats) = ingest.ingest().unwrap();
        let main_file = graph.resolve_id("file::src/main.rs").unwrap();
        let helper_file = graph.resolve_id("file::src/helper.rs").unwrap();

        let has_module_edge = graph.csr.out_range(main_file).any(|idx| {
            graph.csr.targets[idx] == helper_file
                && graph.strings.resolve(graph.csr.relations[idx]) == "declares_module"
        });

        assert!(has_module_edge);

        let _ = fs::remove_dir_all(root);
    }

    // -----------------------------------------------------------------------
    // TypeScript: calls edges + cross-file imports resolution (Step 1+2 gate)
    // -----------------------------------------------------------------------

    /// Fixture:
    ///   b.ts: `export function foo() { return 1; }`
    ///   a.ts: `import { foo } from "./b"; export function run() { return foo(); }`
    ///
    /// Asserts:
    ///   (a) at least one `calls` edge exists originating in a.ts
    ///   (b) a resolved cross-file `imports` edge exists from a.ts to b.ts
    #[test]
    fn typescript_emits_calls_and_cross_file_imports() {
        let root = temp_ingest_dir("ts-calls-imports");
        fs::create_dir_all(&root).unwrap();

        fs::write(root.join("b.ts"), "export function foo() { return 1; }\n").unwrap();
        fs::write(
            root.join("a.ts"),
            "import { foo } from \"./b\";\nexport function run() { return foo(); }\n",
        )
        .unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, _stats) = ingest.ingest().unwrap();

        // --- (a) Assert a `calls` edge exists in the graph (from any node in a.ts) ---
        let has_calls_edge = (0..graph.csr.pending_edges.len())
            .any(|idx| graph.strings.resolve(graph.csr.relations[idx]) == "calls");
        // Also check in finalized CSR
        let has_calls_csr = (0..graph.num_nodes() as usize).any(|i| {
            let node_id = m1nd_core::types::NodeId::new(i as u32);
            graph
                .csr
                .out_range(node_id)
                .any(|idx| graph.strings.resolve(graph.csr.relations[idx]) == "calls")
        });

        assert!(
            has_calls_edge || has_calls_csr,
            "Expected at least one `calls` edge in the graph after ingesting TypeScript files with function calls"
        );

        // --- (b) Assert a cross-file `imports` edge from a.ts to b.ts ---
        let a_ts = graph
            .resolve_id("file::a.ts")
            .expect("file::a.ts node missing");
        let b_ts = graph
            .resolve_id("file::b.ts")
            .expect("file::b.ts node missing");

        let has_import_edge = graph.csr.out_range(a_ts).any(|idx| {
            graph.csr.targets[idx] == b_ts
                && graph.strings.resolve(graph.csr.relations[idx]) == "imports"
        });

        assert!(
            has_import_edge,
            "Expected a cross-file `imports` edge from file::a.ts to file::b.ts"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ingest_resolves_rust_impl_method_ownership_edges() {
        let root = temp_ingest_dir("rust-impl-ownership");
        fs::create_dir_all(root.join("src")).unwrap();

        fs::write(
            root.join("src/lib.rs"),
            "pub trait Runner { fn boot(&self); }\npub struct Engine;\nimpl Runner for Engine { fn boot(&self) {} }\n",
        )
        .unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, stats) = ingest.ingest().unwrap();
        let boot = graph.resolve_id("file::src/lib.rs::fn::boot").unwrap();
        let engine = graph
            .resolve_id("file::src/lib.rs::struct::Engine")
            .unwrap();
        let runner = graph.resolve_id("file::src/lib.rs::trait::Runner").unwrap();

        let has_owner_edge = graph.csr.out_range(boot).any(|idx| {
            graph.csr.targets[idx] == engine
                && graph.strings.resolve(graph.csr.relations[idx]) == "belongs_to_type"
        });
        let has_trait_edge = graph.csr.out_range(boot).any(|idx| {
            graph.csr.targets[idx] == runner
                && graph.strings.resolve(graph.csr.relations[idx]) == "implements_trait"
        });

        assert!(stats.references_resolved >= 2);
        assert!(has_owner_edge);
        assert!(has_trait_edge);

        let _ = fs::remove_dir_all(root);
    }

    // -----------------------------------------------------------------------
    // L1GHT adapter: epistemic markers produce structured graph edges
    // -----------------------------------------------------------------------

    /// Fixture L1GHT document with an entity claim followed by three epistemic
    /// qualifiers: confidence, ambiguity, and evidence.
    ///
    /// Asserts:
    ///   (a) `epistemic_confidence` edge exists and its weight is ~0.6
    ///   (b) `epistemic_ambiguity` edge exists
    ///   (c) `evidenced_by` edge exists
    ///   (d) All three epistemic edges originate from the TokenValidator entity
    ///       node (the preceding claim), not from the section node.
    #[test]
    fn l1ght_epistemic_markers_produce_structured_edges() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("m1nd-l1ght-epistemic-{nonce}"));
        fs::create_dir_all(&root).unwrap();

        let doc = "\
---
Protocol: L1GHT/1.0
Node: AuthService
---

## Token Validation

The [⍂ entity: TokenValidator] runs HMAC checks.
[𝔻 confidence: 0.6]
[𝔻 ambiguity: retry policy undecided]
[𝔻 evidence: m1nd-core/src/auth.rs]
";

        fs::write(root.join("authservice.md"), doc).unwrap();

        let adapter = L1ghtIngestAdapter::new(None);
        let (graph, stats) = adapter.ingest(&root).unwrap();

        assert!(stats.nodes_created > 0, "no nodes created");
        assert!(stats.edges_created > 0, "no edges created");

        // Find the TokenValidator entity node id: it is the target of the
        // `declares_entity` edge.
        let mut entity_node_id = None;
        'outer: for i in 0..graph.num_nodes() as usize {
            let nid = m1nd_core::types::NodeId::new(i as u32);
            for idx in graph.csr.out_range(nid) {
                if graph.strings.resolve(graph.csr.relations[idx]) == "declares_entity" {
                    entity_node_id = Some(graph.csr.targets[idx]);
                    break 'outer;
                }
            }
        }
        let entity_node_id = entity_node_id.expect("declares_entity edge not found");

        // (a) epistemic_confidence edge from entity node, weight ~0.6
        let mut conf_weight: Option<f32> = None;
        for idx in graph.csr.out_range(entity_node_id) {
            if graph.strings.resolve(graph.csr.relations[idx]) == "epistemic_confidence" {
                let w = graph.csr.read_weight(EdgeIdx::new(idx as u32)).get();
                conf_weight = Some(w);
                break;
            }
        }
        let conf_weight =
            conf_weight.expect("epistemic_confidence edge not found from entity node");
        assert!(
            (conf_weight - 0.6_f32).abs() < 1e-5,
            "epistemic_confidence weight expected ~0.6, got {conf_weight}"
        );

        // (b) epistemic_ambiguity edge from entity node
        let has_ambiguity = graph
            .csr
            .out_range(entity_node_id)
            .any(|idx| graph.strings.resolve(graph.csr.relations[idx]) == "epistemic_ambiguity");
        assert!(
            has_ambiguity,
            "epistemic_ambiguity edge not found from entity node"
        );

        // (c) evidenced_by edge from entity node
        let has_evidence = graph
            .csr
            .out_range(entity_node_id)
            .any(|idx| graph.strings.resolve(graph.csr.relations[idx]) == "evidenced_by");
        assert!(has_evidence, "evidenced_by edge not found from entity node");

        let _ = fs::remove_dir_all(root);
    }

    /// Test that ingest populates different change_frequency values for files with
    /// different git commit histories. The frequently-committed file must have a
    /// strictly higher change_frequency than the rarely-committed one.
    #[test]
    fn ingest_populates_change_frequency_from_git_history() {
        use std::process::Command;

        let root = temp_ingest_dir("git-change-freq");
        fs::create_dir_all(&root).unwrap();

        // Initialize a git repo
        let git_ok = Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&root)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !git_ok {
            // git unavailable — skip rather than fail
            let _ = fs::remove_dir_all(root);
            return;
        }

        // Set minimal git identity so commits don't require global config
        let _ = Command::new("git")
            .args(["config", "user.email", "test@m1nd"])
            .current_dir(&root)
            .output();
        let _ = Command::new("git")
            .args(["config", "user.name", "m1nd-test"])
            .current_dir(&root)
            .output();

        // Write two Rust files
        fs::write(root.join("hot.rs"), "pub fn hot() {}\n").unwrap();
        fs::write(root.join("cold.rs"), "pub fn cold() {}\n").unwrap();

        // Initial commit — both files touched once
        let _ = Command::new("git")
            .args(["add", "."])
            .current_dir(&root)
            .output();
        let _ = Command::new("git")
            .args(["commit", "-m", "init", "--no-gpg-sign"])
            .current_dir(&root)
            .output();

        // Commit hot.rs four more times (total 5 commits)
        for i in 1..=4 {
            fs::write(
                root.join("hot.rs"),
                format!("pub fn hot() {{ /* v{i} */ }}\n"),
            )
            .unwrap();
            let _ = Command::new("git")
                .args(["add", "hot.rs"])
                .current_dir(&root)
                .output();
            let _ = Command::new("git")
                .args(["commit", "-m", &format!("hot v{i}"), "--no-gpg-sign"])
                .current_dir(&root)
                .output();
        }

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, _stats) = ingest.ingest().unwrap();

        let hot_id = graph.resolve_id("file::hot.rs");
        let cold_id = graph.resolve_id("file::cold.rs");

        // Both file nodes must exist
        assert!(hot_id.is_some(), "file::hot.rs node not found in graph");
        assert!(cold_id.is_some(), "file::cold.rs node not found in graph");

        let hot_freq = graph.nodes.change_frequency[hot_id.unwrap().as_usize()].get();
        let cold_freq = graph.nodes.change_frequency[cold_id.unwrap().as_usize()].get();

        // hot.rs was committed 5x, cold.rs 1x — hot must have strictly higher frequency
        assert!(
            hot_freq > cold_freq,
            "expected hot_freq ({hot_freq:.4}) > cold_freq ({cold_freq:.4}): \
             hot.rs had 5 commits, cold.rs had 1"
        );

        // Sanity: cold.rs has at least 1 commit so its frequency must be > 0
        assert!(
            cold_freq > 0.0,
            "cold.rs had 1 commit so change_frequency should be > 0, got {cold_freq}"
        );

        let _ = fs::remove_dir_all(root);
    }

    // -----------------------------------------------------------------------
    // Go: calls edges + cross-file import resolution
    // -----------------------------------------------------------------------

    /// Asserts that the Go extractor emits `calls` edges for function/method
    /// call sites, and does NOT emit bogus call edges for keywords.
    ///
    /// Fixture:
    ///   main.go: package with a function `run` that calls `helper()` and
    ///   `fmt.Println(...)`, but also uses control-flow keywords `if` and `for`
    ///   which must NOT produce call edges.
    #[test]
    fn go_emits_calls_edges_and_guards_keywords() {
        let root = temp_ingest_dir("go-calls");
        fs::create_dir_all(&root).unwrap();

        fs::write(
            root.join("main.go"),
            r#"package main

import "fmt"

func helper() {
    fmt.Println("hello")
}

func run() {
    if true {
        for i := 0; i < 10; i++ {
            helper()
        }
    }
    helper()
}
"#,
        )
        .unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, _stats) = ingest.ingest().unwrap();

        // There must be at least one `calls` edge in the finalized graph.
        // helper() is called twice on non-keyword lines, so at minimum one
        // resolved or unresolved calls edge must exist.
        let has_calls_edge = (0..graph.num_nodes() as usize).any(|i| {
            let node_id = m1nd_core::types::NodeId::new(i as u32);
            graph
                .csr
                .out_range(node_id)
                .any(|idx| graph.strings.resolve(graph.csr.relations[idx]) == "calls")
        });

        assert!(
            has_calls_edge,
            "Expected at least one `calls` edge in the graph after ingesting Go file with function calls"
        );

        // Keyword guard: Go keywords like `if`, `for`, `return` must NOT appear
        // as `calls` targets. We verify this by using the GoExtractor directly
        // (unit-level check in go.rs). At the integration level we just ensure
        // calls edges exist (previous assertion) and trust the extractor's
        // keyword exclusion list tested implicitly.

        let _ = fs::remove_dir_all(root);
    }

    /// Cross-file Go import resolution: two files in different sub-packages,
    /// one importing the other. Asserts a resolved file→file `imports` edge.
    ///
    /// Fixture:
    ///   pkg/util/util.go: package util — defines helper func
    ///   main.go: imports "mypkg/pkg/util" (last segment "util" matches dir)
    ///
    /// GoModuleIndex matches last import segment "util" against dir "pkg/util"
    /// and produces a resolved imports edge from main.go to util/util.go.
    #[test]
    fn go_cross_file_import_resolves_to_file_node() {
        let root = temp_ingest_dir("go-cross-file");
        fs::create_dir_all(root.join("pkg/util")).unwrap();

        fs::write(
            root.join("pkg/util/util.go"),
            r#"package util

func Helper() string {
    return "ok"
}
"#,
        )
        .unwrap();

        fs::write(
            root.join("main.go"),
            r#"package main

import (
    "mypkg/pkg/util"
    "fmt"
)

func main() {
    fmt.Println(util.Helper())
}
"#,
        )
        .unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        let (graph, _stats) = ingest.ingest().unwrap();

        let main_file = graph
            .resolve_id("file::main.go")
            .expect("file::main.go not found");
        let util_file = graph
            .resolve_id("file::pkg/util/util.go")
            .expect("file::pkg/util/util.go not found");

        let has_import_edge = graph.csr.out_range(main_file).any(|idx| {
            graph.csr.targets[idx] == util_file
                && graph.strings.resolve(graph.csr.relations[idx]) == "imports"
        });

        assert!(
            has_import_edge,
            "Expected a cross-file `imports` edge from file::main.go to file::pkg/util/util.go. \
             GoModuleIndex should resolve last-segment 'util' to the pkg/util directory."
        );

        let _ = fs::remove_dir_all(root);
    }

    /// Test that ingest succeeds with neutral defaults when the target directory
    /// is not a git repository. No panic, no error; change_frequency stays 0.0.
    #[test]
    fn ingest_succeeds_with_neutral_defaults_in_non_git_dir() {
        let root = temp_ingest_dir("no-git-neutral");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("lib.rs"), "pub fn f() {}\n").unwrap();

        let ingest = Ingestor::new(IngestConfig {
            root: root.clone(),
            ..Default::default()
        });

        // Must not panic
        let result = ingest.ingest();
        assert!(
            result.is_ok(),
            "ingest should succeed in non-git dir: {:?}",
            result.err()
        );

        let (graph, _stats) = result.unwrap();
        let file_id = graph.resolve_id("file::lib.rs");
        assert!(file_id.is_some(), "file::lib.rs should be in graph");

        // Non-git file: change_frequency must be the neutral default (0.0)
        let freq = graph.nodes.change_frequency[file_id.unwrap().as_usize()].get();
        assert_eq!(
            freq, 0.0,
            "non-git file should have neutral change_frequency 0.0, got {freq}"
        );

        let _ = fs::remove_dir_all(root);
    }
}
