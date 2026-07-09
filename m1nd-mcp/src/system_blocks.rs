//! Human View v2 F0a — SystemBlock contract seed types, validator, and the live
//! sidecar store.
//!
//! Slice 1 modelled the ratified seed contract (`m1nd-system-block-seed-v0`),
//! validated import-time safety invariants, and exported a deterministic pretty
//! JSON form. Slice 2 adds the LIVE side: the per-project-brain sidecar store
//! (`m1nd-system-block-store-v0`, [`SystemBlockStore`]) that the F0a verbs serve,
//! its atomic load/save, the optimistic-concurrency (OCC) transaction law
//! (PRD §3.1 — every mutation is keyed on the `store_version` it read; a stale
//! write is rejected with [`SeedError::Conflict`], never silently applied), and
//! the anti-poison receipt-evidence contract (§3). It still adds no routes, UI,
//! or runner execution — the MCP verbs live in `system_blocks_handlers`.

use std::error::Error;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The only seed schema accepted by F0a slice 1.
pub const SYSTEM_BLOCK_SEED_SCHEMA: &str = "m1nd-system-block-seed-v0";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemBlock {
    pub block_id: String,
    pub name: String,
    pub purpose: String,
    pub kind: SystemBlockKind,
    pub state: SystemBlockState,
    pub boundary_version: u32,
    pub contract_version: u32,
    pub membership_source: MembershipSource,
    pub membership: Vec<MembershipEntry>,
    pub sockets: Sockets,
    pub receipt_contract: ReceiptContract,
    pub receipts: Vec<Receipt>,
    pub layout: Layout,
    pub unmapped_residue: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemBlockKind {
    Scanned,
    Planned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemBlockState {
    Candidate,
    Planned,
    Building,
    Scanned,
    Ratified,
    Drifted,
    Archived,
    Restored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipSource {
    Ratified,
    Proposed,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MembershipEntry {
    pub path: String,
    pub role: MembershipRole,
    #[serde(default, skip_serializing_if = "is_false")]
    pub optional: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipRole {
    Primary,
    Shared,
    Generated,
    Test,
    Docs,
    ExternalSocket,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptContract {
    pub version: u32,
    pub required: Vec<ReceiptRequirement>,
    pub optional: Vec<ReceiptRequirement>,
    pub waived: Vec<ReceiptRequirement>,
    pub declared_by: Option<String>,
    pub declared_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptRequirement {
    #[serde(rename = "type")]
    pub type_: ReceiptType,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stales_on: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptType {
    Test,
    Structural,
    Runtime,
    Review,
    Handoff,
    Spec,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    #[serde(rename = "type")]
    pub type_: ReceiptType,
    pub emitter: ReceiptEmitter,
    pub scope: ReceiptScope,
    pub evidence: ReceiptEvidence,
    pub validity: ReceiptValidity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptEmitter {
    pub kind: ReceiptEmitterKind,
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptEmitterKind {
    Ci,
    Runnerd,
    Verb,
    Owner,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptScope {
    pub block_id: String,
    pub boundary_version: u32,
    pub contract_version: u32,
    pub resolution_hash: String,
}

/// Receipt evidence (anti-poison, HUMAN-VIEW-V2-F0-TECH §3). The EXECUTION fields
/// are optional — a `spec`/`structural`/`review` receipt is not born from a shell
/// command — but the UNIVERSAL ANCHOR (`artifact_hash` + `evidence_refs`) is
/// mandatory and never empty: evidence a tool cannot point at is not evidence. A
/// `test` receipt is held to the stronger contract that its execution identity
/// (`command`/`cwd`/`exit_status`/`started_at`/`ended_at`) is present — see
/// [`validate_receipt_evidence`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptEvidence {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_status: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    /// The universal anchor: the sha256 of the raw artifact. NEVER empty.
    pub artifact_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout_excerpt: Option<String>,
    /// The universal anchor: artifact paths or CI run urls. NEVER empty.
    pub evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptValidity {
    pub expires_on: Option<String>,
    pub stales_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sockets {
    pub inputs: Vec<Socket>,
    pub outputs: Vec<Socket>,
    pub external: Vec<Socket>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Socket {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(rename = "class", skip_serializing_if = "Option::is_none")]
    pub class_: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub locked: bool,
    pub algorithm_seed: Option<serde_json::Value>,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionPacket {
    pub source_block: String,
    pub message: String,
    pub includes: MissionPacketIncludes,
    pub mode: MissionPacketMode,
    pub declared_effects: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionPacketIncludes {
    pub details: bool,
    pub files: bool,
    pub receipts: bool,
    pub impact: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissionPacketMode {
    Clipboard,
    Direct,
    Spawn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerRef {
    pub runner_id: String,
    pub label: String,
    pub capabilities: Vec<RunnerCapability>,
    pub workspace_truth: RunnerWorkspaceTruth,
    pub policy: MissionPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerCapability {
    CanEdit,
    CanTest,
    CanReadM1nd,
    CanReceivePackets,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerWorkspaceTruth {
    pub bound_root: Option<String>,
    pub wrong_workspace: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionPolicy {
    pub isolated_worktree: bool,
    pub propose_only: bool,
    pub screenshot_in_packet: bool,
    pub read_only_clipboard: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pub block_id: String,
    pub mission_id: String,
    pub agent: String,
    pub status: PinStatus,
    pub progress: Option<String>,
    pub outcome_ref: Option<String>,
    pub contract_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinStatus {
    Running,
    NeedsReply,
    OutputLanded,
    Debriefed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedFile {
    pub schema: String,
    pub repo: SeedRepo,
    pub skeleton: SeedSkeleton,
    pub blocks: Vec<SystemBlock>,
    pub unmapped_policy: UnmappedPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedRepo {
    pub repo_id: String,
    pub root: String,
    pub source_commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedSkeleton {
    pub skeleton_id: String,
    pub version: u32,
    pub state: SeedSkeletonState,
    pub ratification: SeedRatification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeedSkeletonState {
    Candidate,
    Ratified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedRatification {
    pub method: String,
    pub ratifier: String,
    pub ratified_at: String,
    pub commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnmappedPolicy {
    pub visible: bool,
    pub default_action: UnmappedDefaultAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnmappedDefaultAction {
    LeaveUnmappedUntilRatified,
}

/// Typed seed import/export errors.
#[derive(Debug)]
pub enum SeedError {
    Json(serde_json::Error),
    /// Filesystem error reading/writing the sidecar store.
    Io(std::io::Error),
    SchemaMismatch {
        expected: String,
        found: String,
    },
    AbsolutePath {
        path: String,
    },
    MissingField {
        field: String,
    },
    ReceiptScopeMismatch {
        block_id: String,
        receipt_block_id: String,
    },
    /// OCC conflict (PRD §3.1): the mutation was keyed on a `store_version` that is
    /// no longer current. The write is REJECTED and nothing is applied — reload and
    /// retry against the fresh version.
    Conflict {
        expected: u64,
        actual: u64,
    },
    /// A seed import found an existing store and `force` was not set. Honest refusal
    /// (`already_present`); nothing is applied.
    StoreAlreadyPresent,
    /// A store mutation was attempted where no store exists yet — import a seed first.
    NoStore,
    /// A targeted `block_id` is not present in the store (no silent skip).
    BlockNotFound {
        block_id: String,
    },
    /// A receipt was earned against a `(boundary_version, contract_version)` that is
    /// not the block's current one (`stale_scope`, PRD §3.1) — evidence is never
    /// counted for a version it did not see. Nothing is applied.
    ReceiptStaleScope {
        block_id: String,
        receipt_boundary: u32,
        receipt_contract: u32,
        block_boundary: u32,
        block_contract: u32,
    },
    /// Receipt evidence failed the anti-poison contract (§3): the universal anchor
    /// (`artifact_hash` + `evidence_refs`) was empty, or a `test` receipt was missing
    /// a required execution field.
    EvidenceIncomplete {
        receipt_type: String,
        missing: String,
    },
}

impl fmt::Display for SeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeedError::Json(err) => write!(f, "invalid SystemBlock seed JSON: {err}"),
            SeedError::Io(err) => write!(f, "system-block store I/O error: {err}"),
            SeedError::SchemaMismatch { expected, found } => {
                write!(
                    f,
                    "seed schema mismatch: expected {expected}, found {found}"
                )
            }
            SeedError::AbsolutePath { path } => write!(
                f,
                "seed paths must be repo-relative and cannot escape the repo: {path}"
            ),
            SeedError::MissingField { field } => {
                write!(f, "seed is missing required field `{field}`")
            }
            SeedError::ReceiptScopeMismatch {
                block_id,
                receipt_block_id,
            } => write!(
                f,
                "receipt scope block_id mismatch: block {block_id}, receipt {receipt_block_id}"
            ),
            SeedError::Conflict { expected, actual } => write!(
                f,
                "store version conflict: expected {expected}, actual {actual} — reload and retry (nothing was applied)"
            ),
            SeedError::StoreAlreadyPresent => write!(
                f,
                "already_present: a system-block store already exists here — pass force:true to overwrite (nothing was applied)"
            ),
            SeedError::NoStore => write!(
                f,
                "no system-block store here yet — import a seed before mutating it"
            ),
            SeedError::BlockNotFound { block_id } => {
                write!(f, "unknown block_id `{block_id}` — not in the store")
            }
            SeedError::ReceiptStaleScope {
                block_id,
                receipt_boundary,
                receipt_contract,
                block_boundary,
                block_contract,
            } => write!(
                f,
                "stale_scope: block {block_id} is at boundary {block_boundary}/contract {block_contract}, but the receipt was earned against boundary {receipt_boundary}/contract {receipt_contract} — never counted for a version it did not see (nothing was applied)"
            ),
            SeedError::EvidenceIncomplete {
                receipt_type,
                missing,
            } => write!(
                f,
                "receipt evidence incomplete for a `{receipt_type}` receipt: missing {missing}"
            ),
        }
    }
}

impl Error for SeedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SeedError::Json(err) => Some(err),
            SeedError::Io(err) => Some(err),
            _ => None,
        }
    }
}

/// Parse and validate a SystemBlock seed.
pub fn load_seed(raw: &str) -> Result<SeedFile, SeedError> {
    let seed: SeedFile = serde_json::from_str(raw).map_err(classify_json_error)?;
    if seed.schema != SYSTEM_BLOCK_SEED_SCHEMA {
        return Err(SeedError::SchemaMismatch {
            expected: SYSTEM_BLOCK_SEED_SCHEMA.to_string(),
            found: seed.schema.clone(),
        });
    }
    validate_seed(&seed)?;
    Ok(seed)
}

/// Export a deterministic, pretty JSON seed representation.
pub fn export_seed(seed: &SeedFile) -> String {
    serde_json::to_string_pretty(seed).expect("SystemBlock seed serialization cannot fail")
}

fn validate_seed(seed: &SeedFile) -> Result<(), SeedError> {
    validate_repo_relative_path(&seed.repo.root)?;
    for block in &seed.blocks {
        for member in &block.membership {
            validate_repo_relative_path(&member.path)?;
        }
        for path in &block.unmapped_residue {
            validate_repo_relative_path(path)?;
        }
        for receipt in &block.receipts {
            validate_receipt_scope(block, receipt)?;
            validate_receipt_evidence(receipt)?;
        }
    }
    Ok(())
}

fn validate_receipt_scope(block: &SystemBlock, receipt: &Receipt) -> Result<(), SeedError> {
    if receipt.scope.block_id != block.block_id
        || receipt.scope.boundary_version != block.boundary_version
        || receipt.scope.contract_version != block.contract_version
    {
        return Err(SeedError::ReceiptScopeMismatch {
            block_id: block.block_id.clone(),
            receipt_block_id: receipt.scope.block_id.clone(),
        });
    }
    Ok(())
}

/// The anti-poison receipt-evidence contract (HUMAN-VIEW-V2-F0-TECH §3). Enforced
/// at BOTH seed-load time and `receipt_import` time:
/// - the universal anchor `artifact_hash` + `evidence_refs` is present and non-empty
///   (evidence a tool cannot point at is not evidence);
/// - `cwd`, when present, obeys the repo-relative law;
/// - a `test` receipt additionally carries its full execution identity
///   (`command`/`cwd`/`exit_status`/`started_at`/`ended_at`).
pub(crate) fn validate_receipt_evidence(receipt: &Receipt) -> Result<(), SeedError> {
    let ev = &receipt.evidence;
    let type_str = receipt_type_str(receipt.type_);
    if ev.artifact_hash.trim().is_empty() {
        return Err(SeedError::EvidenceIncomplete {
            receipt_type: type_str.to_string(),
            missing: "artifact_hash".to_string(),
        });
    }
    if ev.evidence_refs.is_empty() {
        return Err(SeedError::EvidenceIncomplete {
            receipt_type: type_str.to_string(),
            missing: "evidence_refs".to_string(),
        });
    }
    if let Some(cwd) = &ev.cwd {
        validate_repo_relative_path(cwd)?;
    }
    if receipt.type_ == ReceiptType::Test {
        let mut missing: Vec<&str> = Vec::new();
        if ev.command.is_none() {
            missing.push("command");
        }
        if ev.cwd.is_none() {
            missing.push("cwd");
        }
        if ev.exit_status.is_none() {
            missing.push("exit_status");
        }
        if ev.started_at.is_none() {
            missing.push("started_at");
        }
        if ev.ended_at.is_none() {
            missing.push("ended_at");
        }
        if !missing.is_empty() {
            return Err(SeedError::EvidenceIncomplete {
                receipt_type: type_str.to_string(),
                missing: missing.join(", "),
            });
        }
    }
    Ok(())
}

/// The wire string for a receipt type (matches the `snake_case` serde rename).
fn receipt_type_str(t: ReceiptType) -> &'static str {
    match t {
        ReceiptType::Test => "test",
        ReceiptType::Structural => "structural",
        ReceiptType::Runtime => "runtime",
        ReceiptType::Review => "review",
        ReceiptType::Handoff => "handoff",
        ReceiptType::Spec => "spec",
    }
}

pub(crate) fn validate_repo_relative_path(path: &str) -> Result<(), SeedError> {
    let trimmed = path.trim();
    if trimmed.starts_with('/')
        || trimmed.starts_with('\\')
        || trimmed == "~"
        || trimmed.starts_with("~/")
        || trimmed.starts_with("~\\")
        || has_windows_drive_absolute_prefix(trimmed)
        || Path::new(trimmed).is_absolute()
        || trimmed.split(['/', '\\']).any(|segment| segment == "..")
    {
        return Err(SeedError::AbsolutePath {
            path: path.to_string(),
        });
    }
    Ok(())
}

fn has_windows_drive_absolute_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
}

/// The read-only file-content cap for the F2 Show Code viewer (HUMAN-VIEW-V2 §B).
/// A larger file is returned truncated with an honest `truncated` flag — the viewer
/// never presents a partial file as if it were whole.
pub const FILE_VIEW_MAX_BYTES: usize = 256 * 1024;

/// A read-only file read for the Show Code viewer (HUMAN-VIEW-V2 F2/F8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoFileRead {
    /// The file content, UTF-8 (lossy so a non-UTF-8 member still renders), capped
    /// at the caller's `max_bytes`.
    pub content: String,
    /// The file's true on-disk size in bytes — the honest denominator for
    /// `truncated` (the viewer can say "showing 256KB of 1.2MB").
    pub bytes: u64,
    /// True when the file was longer than `max_bytes` and `content` is a prefix.
    pub truncated: bool,
}

/// Read a repo-relative member file under `root`, READ-ONLY, for the Show Code
/// viewer (HUMAN-VIEW-V2 §B). Reuses the seed's anti-absolute/anti-escape law
/// ([`validate_repo_relative_path`]) and adds a defense-in-depth root-containment
/// check: canonicalize both sides and refuse anything resolving OUTSIDE the repo
/// (a symlink can never leak a file from beyond the root). Content is capped at
/// `max_bytes` with an honest `truncated` flag and a bounded read (a huge file
/// never loads whole into memory). A pure read — never mutates, so it is safe under
/// a read-only attach and stays OFF the write deny-list.
pub fn read_repo_relative_file(
    root: &Path,
    rel: &str,
    max_bytes: usize,
) -> Result<RepoFileRead, SeedError> {
    use std::io::Read;
    // (1) The same repo-relative law the seed paths obey (absolute/`~`/drive/`..`).
    validate_repo_relative_path(rel)?;
    // (2) Resolve real paths and refuse anything that escapes the repo root — the
    //     symlink defense the lexical check alone cannot give.
    let canon_root = root.canonicalize().map_err(SeedError::Io)?;
    let canon_full = canon_root.join(rel).canonicalize().map_err(SeedError::Io)?;
    if !canon_full.starts_with(&canon_root) {
        return Err(SeedError::AbsolutePath {
            path: rel.to_string(),
        });
    }
    // (3) Only regular files — a directory or device is not a viewable member.
    let meta = std::fs::metadata(&canon_full).map_err(SeedError::Io)?;
    if !meta.is_file() {
        return Err(SeedError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a regular file",
        )));
    }
    // (4) Bounded read: take at most `max_bytes`, keep the true size for honesty.
    let bytes = meta.len();
    let mut buf = Vec::new();
    std::fs::File::open(&canon_full)
        .map_err(SeedError::Io)?
        .take(max_bytes as u64)
        .read_to_end(&mut buf)
        .map_err(SeedError::Io)?;
    Ok(RepoFileRead {
        content: String::from_utf8_lossy(&buf).into_owned(),
        bytes,
        truncated: bytes > max_bytes as u64,
    })
}

fn classify_json_error(err: serde_json::Error) -> SeedError {
    if err.is_data() {
        if let Some(field) = missing_field_name(&err.to_string()) {
            return SeedError::MissingField { field };
        }
    }
    SeedError::Json(err)
}

fn missing_field_name(message: &str) -> Option<String> {
    let start = message.find("missing field `")? + "missing field `".len();
    let rest = &message[start..];
    let end = rest.find('`')?;
    Some(rest[..end].to_string())
}

fn is_false(value: &bool) -> bool {
    !*value
}

// ===========================================================================
// Slice 2 — the live sidecar store (`m1nd-system-block-store-v0`).
//
// Store locus (HUMAN-VIEW-V2-F0-TECH §1): a sidecar file per project brain,
// `system_blocks.json`, alongside the brain's other runtime artifacts. The seed
// (in the repo) is the reviewable form; the store (in the brain runtime dir) is
// the living form. Import = seed -> store; every accepted mutation bumps the
// global OCC counter `store_version` (PRD §3.1).
// ===========================================================================

/// The only store schema Slice 2 reads/writes.
pub const SYSTEM_BLOCK_STORE_SCHEMA: &str = "m1nd-system-block-store-v0";

/// The sidecar file name inside the brain's runtime dir.
pub const SYSTEM_BLOCK_STORE_FILE: &str = "system_blocks.json";

/// The living SystemBlock store — the sidecar form of the seed, plus the global
/// optimistic-concurrency counter `store_version`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemBlockStore {
    pub schema: String,
    /// The global OCC counter (PRD §3.1). Starts at 1 on import; every accepted
    /// mutation increments it by exactly one.
    pub store_version: u64,
    pub skeleton: SeedSkeleton,
    pub blocks: Vec<SystemBlock>,
    pub unmapped_policy: UnmappedPolicy,
}

/// What a ratify transaction changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RatifySummary {
    /// The block ids that were targeted (and are now ratified).
    pub ratified_block_ids: Vec<String>,
    /// The store version AFTER the bump.
    pub store_version: u64,
}

/// The outcome of a seed import into a brain dir.
#[derive(Debug, Clone)]
pub struct SeedImportOutcome {
    pub store: SystemBlockStore,
    /// True when an existing store was overwritten (`force`).
    pub overwritten: bool,
}

impl SystemBlockStore {
    /// The store file path inside a brain runtime dir.
    pub fn path_in(dir: &Path) -> std::path::PathBuf {
        dir.join(SYSTEM_BLOCK_STORE_FILE)
    }

    /// Build a fresh store from a validated seed. `store_version` starts at 1.
    pub fn from_seed(seed: SeedFile) -> Self {
        Self {
            schema: SYSTEM_BLOCK_STORE_SCHEMA.to_string(),
            store_version: 1,
            skeleton: seed.skeleton,
            blocks: seed.blocks,
            unmapped_policy: seed.unmapped_policy,
        }
    }

    /// Load the store from a brain dir. `None` when the sidecar does not exist yet
    /// (an honest "no skeleton" state, never an error).
    pub fn load(dir: &Path) -> Result<Option<Self>, SeedError> {
        let path = Self::path_in(dir);
        let raw = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(SeedError::Io(e)),
        };
        let store: SystemBlockStore = serde_json::from_str(&raw).map_err(classify_json_error)?;
        if store.schema != SYSTEM_BLOCK_STORE_SCHEMA {
            return Err(SeedError::SchemaMismatch {
                expected: SYSTEM_BLOCK_STORE_SCHEMA.to_string(),
                found: store.schema.clone(),
            });
        }
        Ok(Some(store))
    }

    /// Persist the store to a brain dir atomically: write a sibling temp file, then
    /// rename over the target (mirrors the repo's `save_json_atomic`), so a reader
    /// never sees a half-written store.
    pub fn save(&self, dir: &Path) -> Result<(), SeedError> {
        std::fs::create_dir_all(dir).map_err(SeedError::Io)?;
        let path = Self::path_in(dir);
        let tmp = path.with_extension("json.tmp");
        let payload = serde_json::to_vec_pretty(self).map_err(SeedError::Json)?;
        std::fs::write(&tmp, payload).map_err(SeedError::Io)?;
        std::fs::rename(&tmp, &path).map_err(SeedError::Io)?;
        Ok(())
    }

    /// Ratify blocks (F0a `system_blocks_ratify`). OCC-checked (PRD §3.1): a stale
    /// `expected` is rejected with [`SeedError::Conflict`] and NOTHING is mutated.
    /// `block_ids == None` ratifies every block; a target id absent from the store
    /// is a hard [`SeedError::BlockNotFound`] (no silent skip). On success each
    /// target flips `candidate -> ratified` (state) and `proposed -> ratified`
    /// (membership_source), the skeleton records this ratification event
    /// (method `verb`), and `store_version` is bumped by one.
    pub fn ratify(
        &mut self,
        expected_store_version: u64,
        block_ids: Option<&[String]>,
        ratifier: &str,
        ratified_at: &str,
    ) -> Result<RatifySummary, SeedError> {
        if expected_store_version != self.store_version {
            return Err(SeedError::Conflict {
                expected: expected_store_version,
                actual: self.store_version,
            });
        }
        // Resolve targets up front so an unknown id fails BEFORE any mutation.
        let targets: Vec<String> = match block_ids {
            None => self.blocks.iter().map(|b| b.block_id.clone()).collect(),
            Some(ids) => {
                for id in ids {
                    if !self
                        .blocks
                        .iter()
                        .any(|b| b.block_id.as_str() == id.as_str())
                    {
                        return Err(SeedError::BlockNotFound {
                            block_id: id.clone(),
                        });
                    }
                }
                ids.to_vec()
            }
        };
        for block in self.blocks.iter_mut() {
            if !targets
                .iter()
                .any(|id| id.as_str() == block.block_id.as_str())
            {
                continue;
            }
            if block.state == SystemBlockState::Candidate {
                block.state = SystemBlockState::Ratified;
            }
            if block.membership_source == MembershipSource::Proposed {
                block.membership_source = MembershipSource::Ratified;
            }
        }
        self.skeleton.state = SeedSkeletonState::Ratified;
        self.skeleton.ratification = SeedRatification {
            method: "verb".to_string(),
            ratifier: ratifier.to_string(),
            ratified_at: ratified_at.to_string(),
            // A verb ratification has no merge commit; the seed's PR-merge form
            // carries that. Empty is honest here rather than fabricated.
            commit: String::new(),
        };
        self.store_version += 1;
        Ok(RatifySummary {
            ratified_block_ids: targets,
            store_version: self.store_version,
        })
    }

    /// Attach an imported receipt to a block (F0a `receipt_import`). OCC-checked
    /// (PRD §3.1) and anti-poison (§3), in this order, so nothing mutates unless
    /// every gate passes:
    /// 1. `expected_store_version` matches -> else [`SeedError::Conflict`];
    /// 2. the block exists -> else [`SeedError::BlockNotFound`];
    /// 3. the receipt's `scope` binds to THIS block's CURRENT `(block_id,
    ///    boundary_version, contract_version)` -> else [`SeedError::ReceiptStaleScope`]
    ///    (evidence is never counted for a version it did not see);
    /// 4. the evidence obeys the anti-poison contract ([`validate_receipt_evidence`]).
    ///
    /// On success the receipt is appended and `store_version` is bumped by one.
    pub fn import_receipt(
        &mut self,
        expected_store_version: u64,
        block_id: &str,
        receipt: Receipt,
    ) -> Result<(), SeedError> {
        if expected_store_version != self.store_version {
            return Err(SeedError::Conflict {
                expected: expected_store_version,
                actual: self.store_version,
            });
        }
        let block = self
            .blocks
            .iter_mut()
            .find(|b| b.block_id.as_str() == block_id)
            .ok_or_else(|| SeedError::BlockNotFound {
                block_id: block_id.to_string(),
            })?;
        if receipt.scope.block_id.as_str() != block_id
            || receipt.scope.boundary_version != block.boundary_version
            || receipt.scope.contract_version != block.contract_version
        {
            return Err(SeedError::ReceiptStaleScope {
                block_id: block_id.to_string(),
                receipt_boundary: receipt.scope.boundary_version,
                receipt_contract: receipt.scope.contract_version,
                block_boundary: block.boundary_version,
                block_contract: block.contract_version,
            });
        }
        validate_receipt_evidence(&receipt)?;
        block.receipts.push(receipt);
        self.store_version += 1;
        Ok(())
    }
}

/// Import a seed into a brain dir (F0a `system_blocks_seed_import`). Refuses an
/// existing store unless `force` ([`SeedError::StoreAlreadyPresent`]). The seed is
/// fully validated (schema, repo-relative paths, receipt scope + evidence) before
/// anything is written; the fresh store carries `store_version = 1`.
pub fn import_seed_into_dir(
    dir: &Path,
    raw: &str,
    force: bool,
) -> Result<SeedImportOutcome, SeedError> {
    let seed = load_seed(raw)?;
    let existed = SystemBlockStore::load(dir)?.is_some();
    if existed && !force {
        return Err(SeedError::StoreAlreadyPresent);
    }
    let store = SystemBlockStore::from_seed(seed);
    store.save(dir)?;
    Ok(SeedImportOutcome {
        store,
        overwritten: existed,
    })
}

/// Ratify against the store in a brain dir: load -> [`SystemBlockStore::ratify`]
/// -> save. The store is saved ONLY on success, so an OCC conflict (or any gate
/// failure) leaves the on-disk store byte-for-byte intact. A missing store is a
/// hard [`SeedError::NoStore`].
pub fn ratify_in_dir(
    dir: &Path,
    expected_store_version: u64,
    block_ids: Option<&[String]>,
    ratifier: &str,
    ratified_at: &str,
) -> Result<(SystemBlockStore, RatifySummary), SeedError> {
    let mut store = SystemBlockStore::load(dir)?.ok_or(SeedError::NoStore)?;
    let summary = store.ratify(expected_store_version, block_ids, ratifier, ratified_at)?;
    store.save(dir)?;
    Ok((store, summary))
}

/// Import a receipt against the store in a brain dir: load ->
/// [`SystemBlockStore::import_receipt`] -> save. Saved ONLY on success, so a
/// conflict / stale scope / bad evidence leaves the on-disk store intact. A
/// missing store is a hard [`SeedError::NoStore`].
pub fn import_receipt_in_dir(
    dir: &Path,
    expected_store_version: u64,
    block_id: &str,
    receipt: Receipt,
) -> Result<SystemBlockStore, SeedError> {
    let mut store = SystemBlockStore::load(dir)?.ok_or(SeedError::NoStore)?;
    store.import_receipt(expected_store_version, block_id, receipt)?;
    store.save(dir)?;
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_seed() -> &'static str {
        r#"{
  "schema": "m1nd-system-block-seed-v0",
  "repo": { "repo_id": "repo_a", "root": ".", "source_commit": "abc123" },
  "skeleton": {
    "skeleton_id": "sk_repo_a_seed_2026_07",
    "version": 1,
    "state": "ratified",
    "ratification": {
      "method": "pr_merge",
      "ratifier": "owner",
      "ratified_at": "2026-07-07T00:00:00Z",
      "commit": "abc123"
    }
  },
  "blocks": [
    {
      "block_id": "sb_core",
      "name": "Core",
      "purpose": "Core graph responsibilities.",
      "kind": "scanned",
      "state": "ratified",
      "boundary_version": 1,
      "contract_version": 1,
      "membership_source": "ratified",
      "membership": [
        { "path": "src/core.rs", "role": "primary" },
        { "path": "tests/core_test.rs", "role": "test", "optional": true }
      ],
      "sockets": { "inputs": [], "outputs": [{ "to": "sb_api", "type": "api" }], "external": [] },
      "receipt_contract": {
        "version": 1,
        "required": [
          { "type": "spec", "stales_on": ["contract_change"] },
          { "type": "test", "stales_on": ["member_change"] }
        ],
        "optional": [{ "type": "review" }],
        "waived": [],
        "declared_by": "owner",
        "declared_at": "2026-07-07T00:00:00Z"
      },
      "receipts": [
        {
          "type": "test",
          "emitter": { "kind": "ci", "id": "ci-main" },
          "scope": {
            "block_id": "sb_core",
            "boundary_version": 1,
            "contract_version": 1,
            "resolution_hash": "sha256:core"
          },
          "evidence": {
            "command": "cargo test -p repo-alpha",
            "cwd": ".",
            "exit_status": 0,
            "started_at": "2026-07-07T00:00:00Z",
            "ended_at": "2026-07-07T00:01:00Z",
            "artifact_hash": "sha256:artifact",
            "stdout_excerpt": "test result: ok",
            "evidence_refs": ["artifacts/core-test.txt"]
          },
          "validity": { "expires_on": null, "stales_on": ["member_change"] }
        }
      ],
      "layout": { "x": null, "y": null, "locked": false, "algorithm_seed": null, "version": 1 },
      "unmapped_residue": []
    },
    {
      "block_id": "sb_api",
      "name": "Api",
      "purpose": "API boundary responsibilities.",
      "kind": "planned",
      "state": "candidate",
      "boundary_version": 1,
      "contract_version": 1,
      "membership_source": "manual",
      "membership": [{ "path": "src/api.rs", "role": "primary" }],
      "sockets": { "inputs": [{ "to": "sb_core", "type": "api" }], "outputs": [], "external": [{ "alias": "mail_provider", "class": "external_service" }] },
      "receipt_contract": { "version": 1, "required": [{ "type": "spec" }], "optional": [], "waived": [], "declared_by": null, "declared_at": null },
      "receipts": [],
      "layout": { "x": 10.0, "y": 20.0, "locked": false, "algorithm_seed": "api-seed", "version": 1 },
      "unmapped_residue": ["src/unmapped.rs"]
    }
  ],
  "unmapped_policy": { "visible": true, "default_action": "leave_unmapped_until_ratified" }
}"#
    }

    #[test]
    fn seed_roundtrip_is_stable() {
        let seed = load_seed(fixture_seed()).expect("fixture parses");
        let exported = export_seed(&seed);
        let reparsed = load_seed(&exported).expect("export parses");
        assert_eq!(seed, reparsed);
        assert_eq!(exported, export_seed(&reparsed));
    }

    #[test]
    fn seed_rejects_absolute_path() {
        let raw = fixture_seed().replace("src/core.rs", "/etc/absolute.rs");
        let err = load_seed(&raw).expect_err("absolute path rejected");
        assert!(matches!(err, SeedError::AbsolutePath { .. }));
    }

    #[test]
    fn seed_rejects_windows_absolute_path() {
        let raw = fixture_seed().replace("src/core.rs", "C:\\\\absolute\\\\core.rs");
        let err = load_seed(&raw).expect_err("windows absolute path rejected");
        assert!(matches!(err, SeedError::AbsolutePath { .. }));
    }

    #[test]
    fn seed_rejects_absolute_receipt_cwd() {
        let raw = fixture_seed().replace("\"cwd\": \".\"", "\"cwd\": \"/tmp/evidence\"");
        let err = load_seed(&raw).expect_err("absolute cwd rejected");
        assert!(matches!(err, SeedError::AbsolutePath { .. }));
    }

    // ── read_repo_relative_file (F2 Show Code viewer, §B) — mirrors the seed's own
    //    anti-absolute/anti-escape law, plus symlink containment + honest cap. ──────

    #[test]
    fn read_repo_file_returns_content_for_a_valid_member() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("src")).expect("mk src");
        std::fs::write(dir.path().join("src/lib.rs"), "fn main() {}\n").expect("write member");
        let read = read_repo_relative_file(dir.path(), "src/lib.rs", FILE_VIEW_MAX_BYTES)
            .expect("a valid member reads");
        assert_eq!(read.content, "fn main() {}\n");
        assert!(!read.truncated);
        assert_eq!(read.bytes, 13);
    }

    #[test]
    fn read_repo_file_rejects_absolute_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = read_repo_relative_file(dir.path(), "/etc/passwd", FILE_VIEW_MAX_BYTES)
            .expect_err("an absolute path is refused");
        assert!(matches!(err, SeedError::AbsolutePath { .. }));
    }

    #[test]
    fn read_repo_file_rejects_parent_escape() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = read_repo_relative_file(dir.path(), "../outside.rs", FILE_VIEW_MAX_BYTES)
            .expect_err("a `..` escape is refused");
        assert!(matches!(err, SeedError::AbsolutePath { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn read_repo_file_refuses_symlink_escaping_the_repo() {
        // A symlink INSIDE the repo pointing OUTSIDE it must never leak the target.
        let outside = tempfile::tempdir().expect("outside tempdir");
        std::fs::write(outside.path().join("secret.txt"), "top secret\n").expect("write secret");
        let repo = tempfile::tempdir().expect("repo tempdir");
        std::os::unix::fs::symlink(
            outside.path().join("secret.txt"),
            repo.path().join("link.txt"),
        )
        .expect("mk symlink");
        let err = read_repo_relative_file(repo.path(), "link.txt", FILE_VIEW_MAX_BYTES)
            .expect_err("a symlink escaping the repo is refused");
        assert!(matches!(err, SeedError::AbsolutePath { .. }));
    }

    #[test]
    fn read_repo_file_truncates_honestly_at_the_cap() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("big.txt"), "a".repeat(10)).expect("write big");
        let read =
            read_repo_relative_file(dir.path(), "big.txt", 4).expect("an oversized file truncates");
        assert!(read.truncated, "flagged truncated");
        assert_eq!(read.content.len(), 4, "content is capped at max_bytes");
        assert_eq!(read.bytes, 10, "the true on-disk size is reported honestly");
    }

    #[test]
    fn seed_rejects_wrong_schema() {
        let raw = fixture_seed().replace("m1nd-system-block-seed-v0", "something-else");
        let err = load_seed(&raw).expect_err("schema rejected");
        assert!(matches!(err, SeedError::SchemaMismatch { .. }));
    }

    #[test]
    fn seed_rejects_missing_required_field() {
        let raw =
            fixture_seed().replace("      \"purpose\": \"Core graph responsibilities.\",\n", "");
        let err = load_seed(&raw).expect_err("missing required field rejected");
        assert!(matches!(err, SeedError::MissingField { field } if field == "purpose"));
    }

    #[test]
    fn seed_rejects_receipt_scope_mismatch() {
        let raw = fixture_seed().replace(
            "\"contract_version\": 1,\n            \"resolution_hash\"",
            "\"contract_version\": 2,\n            \"resolution_hash\"",
        );
        let err = load_seed(&raw).expect_err("receipt scope mismatch rejected");
        assert!(matches!(err, SeedError::ReceiptScopeMismatch { .. }));
    }

    #[test]
    fn membership_is_path_first() {
        let entry = MembershipEntry {
            path: "src/core.rs".to_string(),
            role: MembershipRole::Primary,
            optional: false,
        };
        let json = serde_json::to_value(&entry).expect("serializes");
        assert_eq!(json["path"], "src/core.rs");
        assert_eq!(json["role"], "primary");
        assert!(json.get("node_id").is_none());
        assert!(json.get("external_id").is_none());
    }

    #[test]
    fn receipt_binds_to_contract_version() {
        let seed = load_seed(fixture_seed()).expect("fixture parses");
        let receipt = &seed.blocks[0].receipts[0];
        assert_eq!(receipt.scope.block_id, "sb_core");
        assert_eq!(receipt.scope.boundary_version, 1);
        assert_eq!(receipt.scope.contract_version, 1);
        assert_eq!(receipt.scope.resolution_hash, "sha256:core");
    }

    /// F0b fixture law (HUMAN-VIEW-V2-F0-TECH s10): the REAL candidate seed for
    /// this repo must parse, roundtrip stably, keep stable block ids, and leak
    /// nothing personal. This is the seed the owner ratifies.
    #[test]
    fn real_m1nd_seed_parses_roundtrips_and_leaks_nothing() {
        let raw = include_str!("../../docs/system-blocks/m1nd.seed.v0.json");
        let seed = load_seed(raw).expect("the real candidate seed must parse");
        assert_eq!(seed.schema, SYSTEM_BLOCK_SEED_SCHEMA);
        assert_eq!(
            seed.blocks.len(),
            12,
            "the candidate skeleton is twelve blocks"
        );
        // Stable, unique block ids.
        let mut ids: Vec<&str> = seed.blocks.iter().map(|b| b.block_id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 12, "block ids are unique");
        assert!(
            ids.iter().all(|i| i.starts_with("sb_m1nd_")),
            "ids carry the repo prefix"
        );
        // Roundtrip stability: export -> load reproduces the same value.
        let exported = export_seed(&seed);
        let reloaded = load_seed(&exported).expect("reload");
        assert_eq!(seed, reloaded, "load(export(seed)) is value-stable");
        // No-leak: nothing personal, no absolute paths anywhere in the raw file.
        for needle in ["/Users/", "/home/", "C:\\", "~/"] {
            assert!(!raw.contains(needle), "seed must not carry {needle}");
        }
    }

    // =======================================================================
    // Slice 2 — evidence anti-poison (§3), the store, OCC, and the verbs' cores.
    // =======================================================================

    const REAL_SEED: &str = include_str!("../../docs/system-blocks/m1nd.seed.v0.json");

    fn anchor_only_evidence() -> ReceiptEvidence {
        ReceiptEvidence {
            command: None,
            cwd: None,
            exit_status: None,
            started_at: None,
            ended_at: None,
            artifact_hash: "sha256:art".to_string(),
            stdout_excerpt: None,
            evidence_refs: vec!["artifacts/x.txt".to_string()],
        }
    }

    fn full_exec_evidence() -> ReceiptEvidence {
        ReceiptEvidence {
            command: Some("cargo test -p m1nd-core".to_string()),
            cwd: Some(".".to_string()),
            exit_status: Some(0),
            started_at: Some("2026-07-09T00:00:00Z".to_string()),
            ended_at: Some("2026-07-09T00:01:00Z".to_string()),
            artifact_hash: "sha256:art".to_string(),
            stdout_excerpt: Some("test result: ok".to_string()),
            evidence_refs: vec!["artifacts/x.txt".to_string()],
        }
    }

    fn mk_receipt(
        type_: ReceiptType,
        block_id: &str,
        boundary: u32,
        contract: u32,
        evidence: ReceiptEvidence,
    ) -> Receipt {
        Receipt {
            type_,
            emitter: ReceiptEmitter {
                kind: ReceiptEmitterKind::Ci,
                id: "ci-x".to_string(),
            },
            scope: ReceiptScope {
                block_id: block_id.to_string(),
                boundary_version: boundary,
                contract_version: contract,
                resolution_hash: "sha256:res".to_string(),
            },
            evidence,
            validity: ReceiptValidity {
                expires_on: None,
                stales_on: Vec::new(),
            },
        }
    }

    fn store_from_fixture() -> SystemBlockStore {
        SystemBlockStore::from_seed(load_seed(fixture_seed()).expect("fixture parses"))
    }

    // --- A) evidence anti-poison -------------------------------------------

    #[test]
    fn spec_receipt_without_execution_fields_passes() {
        // A `spec` receipt is not born from a command; the universal anchor is
        // enough. This is the review-note fix: execution fields are optional.
        let r = mk_receipt(ReceiptType::Spec, "sb_core", 1, 1, anchor_only_evidence());
        validate_receipt_evidence(&r).expect("spec receipt with anchor-only evidence is valid");
    }

    #[test]
    fn test_receipt_without_command_is_rejected() {
        // A `test` receipt MUST carry its execution identity (semantic validation).
        let r = mk_receipt(ReceiptType::Test, "sb_core", 1, 1, anchor_only_evidence());
        let err = validate_receipt_evidence(&r).expect_err("test receipt needs execution fields");
        match err {
            SeedError::EvidenceIncomplete {
                receipt_type,
                missing,
            } => {
                assert_eq!(receipt_type, "test");
                assert!(
                    missing.contains("command"),
                    "missing must name command: {missing}"
                );
            }
            other => panic!("expected EvidenceIncomplete, got {other:?}"),
        }
    }

    #[test]
    fn evidence_anchor_is_mandatory_for_every_type() {
        // Empty artifact_hash -> rejected even for a spec receipt.
        let mut ev = anchor_only_evidence();
        ev.artifact_hash = "   ".to_string();
        let r = mk_receipt(ReceiptType::Spec, "sb_core", 1, 1, ev);
        assert!(matches!(
            validate_receipt_evidence(&r),
            Err(SeedError::EvidenceIncomplete { ref missing, .. }) if missing == "artifact_hash"
        ));
        // Empty evidence_refs -> rejected.
        let mut ev = anchor_only_evidence();
        ev.evidence_refs.clear();
        let r = mk_receipt(ReceiptType::Review, "sb_core", 1, 1, ev);
        assert!(matches!(
            validate_receipt_evidence(&r),
            Err(SeedError::EvidenceIncomplete { ref missing, .. }) if missing == "evidence_refs"
        ));
    }

    #[test]
    fn spec_receipt_omits_execution_fields_from_json() {
        // skip_serializing_if keeps a spec receipt's JSON free of null execution keys.
        let r = mk_receipt(ReceiptType::Spec, "sb_core", 1, 1, anchor_only_evidence());
        let v = serde_json::to_value(&r).expect("serializes");
        let ev = &v["evidence"];
        assert!(ev.get("command").is_none(), "command omitted");
        assert!(ev.get("exit_status").is_none(), "exit_status omitted");
        assert_eq!(ev["artifact_hash"], "sha256:art");
        assert_eq!(ev["evidence_refs"][0], "artifacts/x.txt");
    }

    // --- B) the sidecar store: roundtrip, empty-dir None, atomic save -------

    #[test]
    fn store_roundtrip_save_load_in_tempdir() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Empty dir -> honest None (no store yet).
        assert!(SystemBlockStore::load(dir.path())
            .expect("load ok")
            .is_none());
        let store = store_from_fixture();
        store.save(dir.path()).expect("save");
        let loaded = SystemBlockStore::load(dir.path())
            .expect("load ok")
            .expect("store present");
        assert_eq!(loaded, store, "save->load is value-stable");
        assert_eq!(loaded.schema, SYSTEM_BLOCK_STORE_SCHEMA);
        assert_eq!(loaded.store_version, 1);
    }

    #[test]
    fn store_save_is_atomic_no_temp_left_behind() {
        let dir = tempfile::tempdir().expect("tempdir");
        store_from_fixture().save(dir.path()).expect("save");
        assert!(
            SystemBlockStore::path_in(dir.path()).exists(),
            "store file exists"
        );
        // The temp sibling used for the atomic rename must not survive the write.
        let tmp = SystemBlockStore::path_in(dir.path()).with_extension("json.tmp");
        assert!(!tmp.exists(), "no .json.tmp left after atomic save");
    }

    // --- OCC: a stale write is rejected and the store is left INTACT --------

    #[test]
    fn occ_conflict_rejects_ratify_and_leaves_store_intact() {
        let dir = tempfile::tempdir().expect("tempdir");
        store_from_fixture().save(dir.path()).expect("save");
        let before = SystemBlockStore::load(dir.path()).unwrap().unwrap();

        // expected != current (1) -> Conflict, nothing applied.
        let err = ratify_in_dir(dir.path(), 99, None, "owner", "2026-07-09T00:00:00Z")
            .expect_err("stale expected must conflict");
        match err {
            SeedError::Conflict { expected, actual } => {
                assert_eq!(expected, 99);
                assert_eq!(actual, 1);
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
        // On-disk store is byte-for-byte the same (reload and compare).
        let after = SystemBlockStore::load(dir.path()).unwrap().unwrap();
        assert_eq!(after, before, "a rejected write must not touch the store");
    }

    #[test]
    fn mutating_a_missing_store_is_no_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(matches!(
            ratify_in_dir(dir.path(), 1, None, "owner", "t"),
            Err(SeedError::NoStore)
        ));
    }

    // --- C) seed_import: real seed, re-import guard, force ------------------

    #[test]
    fn seed_import_real_seed_then_guard_then_force() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outcome = import_seed_into_dir(dir.path(), REAL_SEED, false).expect("first import");
        assert_eq!(
            outcome.store.blocks.len(),
            12,
            "the real skeleton is twelve blocks"
        );
        assert_eq!(
            outcome.store.store_version, 1,
            "a fresh import starts at version 1"
        );
        assert!(!outcome.overwritten, "first import overwrites nothing");

        // Snapshot equivalent: the store on disk has the twelve blocks.
        let snap = SystemBlockStore::load(dir.path()).unwrap().unwrap();
        assert_eq!(snap.blocks.len(), 12);

        // Re-import without force is an honest refusal.
        assert!(matches!(
            import_seed_into_dir(dir.path(), REAL_SEED, false),
            Err(SeedError::StoreAlreadyPresent)
        ));

        // Re-import WITH force overwrites, reporting it, and resets to version 1.
        let forced = import_seed_into_dir(dir.path(), REAL_SEED, true).expect("forced import");
        assert!(forced.overwritten, "force reports the overwrite");
        assert_eq!(forced.store.store_version, 1);
    }

    // --- ratify: flips states + bumps version; partial only flips its target -

    #[test]
    fn ratify_all_flips_states_stamps_skeleton_and_bumps_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        store_from_fixture().save(dir.path()).expect("save");
        // Fixture: sb_core is already ratified; sb_api is candidate + manual.
        let (store, summary) =
            ratify_in_dir(dir.path(), 1, None, "owner", "2026-07-09T00:00:00Z").expect("ratify");
        assert_eq!(
            store.store_version, 2,
            "an accepted write bumps the version"
        );
        assert_eq!(summary.store_version, 2);
        assert!(store
            .blocks
            .iter()
            .all(|b| b.state == SystemBlockState::Ratified));
        assert_eq!(store.skeleton.state, SeedSkeletonState::Ratified);
        assert_eq!(store.skeleton.ratification.method, "verb");
        assert_eq!(store.skeleton.ratification.ratifier, "owner");
        assert_eq!(
            store.skeleton.ratification.ratified_at,
            "2026-07-09T00:00:00Z"
        );
    }

    #[test]
    fn ratify_partial_only_flips_the_named_block() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Two candidate/proposed blocks so a partial ratify is observable.
        let mut store = store_from_fixture();
        for b in store.blocks.iter_mut() {
            b.state = SystemBlockState::Candidate;
            b.membership_source = MembershipSource::Proposed;
        }
        store.save(dir.path()).expect("save");

        let targets = vec!["sb_api".to_string()];
        let (store, summary) =
            ratify_in_dir(dir.path(), 1, Some(&targets), "owner", "t").expect("partial ratify");
        assert_eq!(summary.ratified_block_ids, vec!["sb_api".to_string()]);
        let api = store
            .blocks
            .iter()
            .find(|b| b.block_id == "sb_api")
            .unwrap();
        let core = store
            .blocks
            .iter()
            .find(|b| b.block_id == "sb_core")
            .unwrap();
        assert_eq!(api.state, SystemBlockState::Ratified, "target flipped");
        assert_eq!(api.membership_source, MembershipSource::Ratified);
        assert_eq!(
            core.state,
            SystemBlockState::Candidate,
            "non-target untouched"
        );
        assert_eq!(core.membership_source, MembershipSource::Proposed);
        assert_eq!(store.store_version, 2);
    }

    #[test]
    fn ratify_unknown_block_id_is_a_hard_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        store_from_fixture().save(dir.path()).expect("save");
        let targets = vec!["sb_ghost".to_string()];
        assert!(matches!(
            ratify_in_dir(dir.path(), 1, Some(&targets), "owner", "t"),
            Err(SeedError::BlockNotFound { .. })
        ));
    }

    // --- receipt_import: spec ok, stale scope intact, test without command --

    #[test]
    fn receipt_import_spec_ok_bumps_and_appends() {
        let dir = tempfile::tempdir().expect("tempdir");
        store_from_fixture().save(dir.path()).expect("save");
        // sb_core is at boundary 1 / contract 1 in the fixture.
        let r = mk_receipt(ReceiptType::Spec, "sb_core", 1, 1, anchor_only_evidence());
        let before = SystemBlockStore::load(dir.path()).unwrap().unwrap();
        let before_n = before
            .blocks
            .iter()
            .find(|b| b.block_id == "sb_core")
            .unwrap()
            .receipts
            .len();
        let store = import_receipt_in_dir(dir.path(), 1, "sb_core", r).expect("spec receipt lands");
        assert_eq!(
            store.store_version, 2,
            "an accepted receipt bumps the version"
        );
        let after_n = store
            .blocks
            .iter()
            .find(|b| b.block_id == "sb_core")
            .unwrap()
            .receipts
            .len();
        assert_eq!(after_n, before_n + 1, "the receipt was appended");
    }

    #[test]
    fn receipt_import_stale_scope_rejected_and_store_intact() {
        let dir = tempfile::tempdir().expect("tempdir");
        store_from_fixture().save(dir.path()).expect("save");
        let before = SystemBlockStore::load(dir.path()).unwrap().unwrap();
        // Receipt earned against contract_version 2, but sb_core is at contract 1.
        let r = mk_receipt(ReceiptType::Spec, "sb_core", 1, 2, anchor_only_evidence());
        let err = import_receipt_in_dir(dir.path(), 1, "sb_core", r)
            .expect_err("stale scope must be refused");
        assert!(matches!(err, SeedError::ReceiptStaleScope { .. }));
        let after = SystemBlockStore::load(dir.path()).unwrap().unwrap();
        assert_eq!(
            after, before,
            "a stale-scope receipt must not touch the store"
        );
    }

    #[test]
    fn receipt_import_test_without_command_rejected_and_store_intact() {
        let dir = tempfile::tempdir().expect("tempdir");
        store_from_fixture().save(dir.path()).expect("save");
        let before = SystemBlockStore::load(dir.path()).unwrap().unwrap();
        // A `test` receipt with valid anchor + scope but no execution fields.
        let r = mk_receipt(ReceiptType::Test, "sb_core", 1, 1, anchor_only_evidence());
        let err = import_receipt_in_dir(dir.path(), 1, "sb_core", r)
            .expect_err("test receipt without execution is refused");
        assert!(matches!(err, SeedError::EvidenceIncomplete { .. }));
        let after = SystemBlockStore::load(dir.path()).unwrap().unwrap();
        assert_eq!(after, before, "a rejected receipt must not touch the store");
    }

    #[test]
    fn receipt_import_test_with_full_execution_lands() {
        let dir = tempfile::tempdir().expect("tempdir");
        store_from_fixture().save(dir.path()).expect("save");
        let r = mk_receipt(ReceiptType::Test, "sb_core", 1, 1, full_exec_evidence());
        let store =
            import_receipt_in_dir(dir.path(), 1, "sb_core", r).expect("full test receipt lands");
        assert_eq!(store.store_version, 2);
    }
}
