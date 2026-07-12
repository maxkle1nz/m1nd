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
    /// Slice 3 reconcile state — the sha256 of this block's effective resolved
    /// membership (the ordered set of real files it claims). `None` until the first
    /// reconcile writes the honest baseline (no bump). A later reconcile whose
    /// fingerprint differs from a present one bumps `boundary_version` (the boundary
    /// moved). `serde(default)` so a Slice-1/2 seed or store loads clean.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub membership_fingerprint: Option<String>,
    /// Slice 3 reconcile cache — the ordered effective membership the fingerprint
    /// was taken over. The `added`/`removed` diff of the next reconcile is computed
    /// against this (the resolution cache foreseen by F0-TECH §2). Never part of the
    /// seed's byte-stable roundtrip (skipped when empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resolved_members: Vec<String>,
    /// Slice 3 archive state — the block's state BEFORE it was archived, so a
    /// restore returns it to its real prior state (`ratified`/`candidate`) instead
    /// of fabricating one. `Some` only while archived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_archive_state: Option<SystemBlockState>,
    /// F0c-a candidate scores (HUMAN-VIEW-V2-F0C-TECH §1/§3). Present ONLY on a block
    /// a `skeleton_candidate` scan proposed — it carries the component confidence
    /// (`graph_cohesion`, `directory_support`, `coverage_ratio`, …) and `named_by`,
    /// so a provisional heuristic label never masquerades as a final one. `None` on
    /// every ratified/hand-authored block. `serde(default, skip_serializing_if)` so
    /// the real seed and every era-prior store parse and roundtrip byte-stable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_meta: Option<CandidateMeta>,
}

/// F0c-a candidate confidence — COMPONENTS, not a single vibe score (objection 8).
/// Attached to a proposed block so the review UI can sort low-support blocks first
/// and mark provisional names. Every field is honest: `graph_cohesion` is `None`
/// (never faked) when the block saw fewer than the declared edge floor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateMeta {
    /// Who named the block: the naming-runner, or the offline heuristic.
    pub named_by: NamedBy,
    /// True while the label is a provisional heuristic — the UI renders it muted
    /// ("unnamed — needs you") and it cannot be ratified without an owner touch (§5).
    pub needs_owner_naming: bool,
    /// Fraction of the block's edges that stay INSIDE the block (internal / touching).
    /// `None` when `edge_sample_size` is below the declared floor — a docs/no-edge
    /// block does not fabricate cohesion (§3b).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_cohesion: Option<f64>,
    /// How many edges touched the block — the honest denominator behind
    /// `graph_cohesion` (and the reason it may be `None`).
    pub edge_sample_size: usize,
    /// How directory-aligned the block is: members-under-its-dir / repo-files-under-its-dir.
    pub directory_support: f64,
    /// Members backed by a real graph node / total members (a docs file with no node
    /// lowers this honestly).
    pub coverage_ratio: f64,
    /// How many members carry `role:"shared"` (a multi-owner seam surfaced, §2a).
    pub shared_member_count: usize,
}

/// Who named a candidate block (§3a). The naming-runner is opt-in (F2.5c); the
/// heuristic always works offline and marks the label provisional; `Owner` is the
/// strongest label — a human touch through the F11 screen (`candidate_edit rename`),
/// which clears `needs_owner_naming` and passes the ratify provenance gate (o6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamedBy {
    Owner,
    Runner,
    Heuristic,
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
    /// Receipt execution timestamps are not a coherent captured-artifact
    /// window. Nothing applied.
    ReceiptTemporalIncoherence {
        field: String,
        reason: String,
    },
    /// A delete was attempted without `force:true` (F0a §8). Deleting drops the block
    /// and all its receipts permanently — the honest refusal points at archive.
    DeleteRequiresForce {
        block_id: String,
    },
    /// A `skeleton_candidate` transaction (F0c-a §1) was called with an
    /// `expected_store_version` that does not match the store's presence: a store
    /// exists but no OCC key was given (would clobber), or a key was given with no
    /// store to key against. Nothing is applied.
    InvalidCandidateTransaction {
        detail: String,
    },
    /// F11-a: an edit op was attempted on a `ratified` skeleton. Editing a signed
    /// boundary is a different ceremony (the deferred revision-promotion flow); the
    /// candidate verbs refuse it (§1a). Nothing is applied.
    SkeletonNotCandidate,
    /// F11-a: the preflight-on-a-clone batch (`candidate_edit`) failed at op
    /// `op_index` (o1). The WHOLE batch is rejected and NOTHING is persisted — a
    /// partial apply under OCC is less safe than none.
    CandidateEdit {
        op_index: usize,
        reason: String,
    },
    /// F11-a ratify provenance gate (o6): a block still carries an untouched
    /// heuristic name (`needs_owner_naming == true`) and cannot be ratified until a
    /// human names it (or accepts the runner name). Nothing is applied.
    NeedsOwnerNaming {
        block_id: String,
    },
    /// F11-a advisory lease (o4): a `candidate_lease acquire`/`refresh`/`release`
    /// was refused because a DIFFERENT agent holds a still-live lease. The lease is
    /// advisory — this refuses only the lease bookkeeping, never an edit.
    LeaseHeld {
        held_by: String,
        until: String,
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
            SeedError::ReceiptTemporalIncoherence { field, reason } => write!(
                f,
                "receipt timestamp `{field}` is incoherent: {reason} — receipts must be composed from captured artifacts (nothing was applied)"
            ),
            SeedError::DeleteRequiresForce { block_id } => write!(
                f,
                "refusing to delete block `{block_id}` without force:true — a delete drops the block and all its receipts permanently; archive it instead (system_blocks_archive) to keep the history, or pass force:true to really delete (nothing was applied)"
            ),
            SeedError::InvalidCandidateTransaction { detail } => {
                write!(f, "invalid skeleton_candidate transaction: {detail} (nothing was applied)")
            }
            SeedError::SkeletonNotCandidate => write!(
                f,
                "skeleton_not_candidate: this skeleton is ratified — editing a signed boundary is a separate ceremony; the candidate verbs only edit a candidate skeleton (nothing was applied)"
            ),
            SeedError::CandidateEdit { op_index, reason } => write!(
                f,
                "candidate_edit rejected at op {op_index}: {reason} — the whole batch was preflighted on a clone and NOTHING was applied (o1)"
            ),
            SeedError::NeedsOwnerNaming { block_id } => write!(
                f,
                "needs_owner_naming: block '{block_id}' has an untouched heuristic name — name it (or accept the runner name) before ratifying"
            ),
            SeedError::LeaseHeld { held_by, until } => write!(
                f,
                "lease_held: the candidate curation lease is held by '{held_by}' until {until} — it is advisory (it never blocks an edit); wait for it to expire or coordinate (nothing was applied)"
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

fn is_zero_usize(value: &usize) -> bool {
    *value == 0
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
    /// Slice 3 reconcile output — the REAL unmapped: repo files claimed by NO block
    /// (F7: unmapped is never hidden). Materialized capped at [`UNMAPPED_FILES_CAP`]
    /// so the store stays bounded; the honest full count lives in `unmapped_total`.
    /// `serde(default)` + skip-empty so a Slice-1/2 store loads and roundtrips clean.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unmapped_files: Vec<String>,
    /// The honest TOTAL number of unmapped files, even when `unmapped_files` was
    /// capped. Zero until the first reconcile; skipped from JSON while zero so a
    /// pre-Slice-3 store is byte-identical.
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub unmapped_total: usize,
    /// F0c-a side-by-side candidate (HUMAN-VIEW-V2-F0C-TECH §4a). On a `ratified`
    /// skeleton, a `skeleton_candidate` scan writes ONLY this — the live blocks, their
    /// receipts, fingerprints and reconcile state are untouched, mechanically. The
    /// Edit-Names-&-Boundaries flow diffs it later (promotion is out of F0c). Boxed so
    /// the common no-revision store stays small; `serde(default, skip_serializing_if)`
    /// so an era-prior store parses and roundtrips byte-stable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_revision: Option<Box<SeedFile>>,
    /// F11-a advisory curation lease (§4bis o4). The agent that currently holds the
    /// soft lease on this candidate; the F11 screen surfaces it ("a hand is curating")
    /// but it NEVER blocks the owner. `None` = free. Set/cleared only by
    /// `candidate_lease`; `candidate_edit` never requires a held lease. `serde(default,
    /// skip_serializing_if)` so an era-prior store (no lease) loads + roundtrips
    /// byte-stable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curating_by: Option<String>,
    /// The RFC3339-UTC instant the current lease EXPIRES. An expired lease
    /// (`curating_until < now`) is reclaimable by anyone — no dead-agent trap (o4).
    /// `None` whenever `curating_by` is `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curating_until: Option<String>,
}

/// What the F0c-a `skeleton_candidate` transaction did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SkeletonCandidateTransactionState {
    CreatedCandidateStore,
    ReplacedCandidateStore,
    WroteCandidateRevision,
}

impl SkeletonCandidateTransactionState {
    pub fn as_str(self) -> &'static str {
        match self {
            SkeletonCandidateTransactionState::CreatedCandidateStore => "created_candidate_store",
            SkeletonCandidateTransactionState::ReplacedCandidateStore => "replaced_candidate_store",
            SkeletonCandidateTransactionState::WroteCandidateRevision => "wrote_candidate_revision",
        }
    }
}

/// F0c-a transaction summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkeletonCandidateSummary {
    pub transaction_state: SkeletonCandidateTransactionState,
    pub store_version: u64,
    pub block_count: usize,
    pub candidate_revision_written: bool,
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
            // Reconcile output — empty until the first `reconcile_store` pass.
            unmapped_files: Vec::new(),
            unmapped_total: 0,
            // No pending candidate revision on a freshly imported/created store.
            candidate_revision: None,
            // A fresh store carries no advisory curation lease.
            curating_by: None,
            curating_until: None,
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
        // The F11-a provenance gate (o6): a block still carrying an untouched
        // heuristic name (`needs_owner_naming == true`) cannot be ratified until a
        // human names it (or accepts the runner name). Runner-named + owner-named
        // blocks (needs_owner_naming == false) ratify normally, so "Ratify all" over
        // a fully runner-named map is legal — the friction law holds without weakening
        // the Ratification law. Checked BEFORE any mutation so a gated batch touches
        // nothing.
        for id in &targets {
            if let Some(block) = self
                .blocks
                .iter()
                .find(|b| b.block_id.as_str() == id.as_str())
            {
                if block
                    .candidate_meta
                    .as_ref()
                    .is_some_and(|m| m.needs_owner_naming)
                {
                    return Err(SeedError::NeedsOwnerNaming {
                        block_id: id.clone(),
                    });
                }
            }
        }
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
    /// 4. the evidence obeys the anti-poison contract ([`validate_receipt_evidence`]);
    /// 5. captured execution timestamps are ordered, not future-dated at
    ///    import, and span no more than 24 hours.
    ///
    /// On success the receipt is appended and `store_version` is bumped by one.
    pub fn import_receipt(
        &mut self,
        expected_store_version: u64,
        block_id: &str,
        receipt: Receipt,
    ) -> Result<(), SeedError> {
        self.import_receipt_at(
            expected_store_version,
            block_id,
            receipt,
            crate::util::now_ms(),
        )
    }

    fn import_receipt_at(
        &mut self,
        expected_store_version: u64,
        block_id: &str,
        receipt: Receipt,
        imported_at_ms: u64,
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
        validate_receipt_window(&receipt, imported_at_ms)?;
        block.receipts.push(receipt);
        self.store_version += 1;
        Ok(())
    }
}

const MAX_RECEIPT_WINDOW_MS: u64 = 24 * 60 * 60 * 1000;

fn validate_receipt_window(receipt: &Receipt, imported_at_ms: u64) -> Result<(), SeedError> {
    let Some(started_at) = receipt.evidence.started_at.as_deref() else {
        return Ok(());
    };
    let Some(ended_at) = receipt.evidence.ended_at.as_deref() else {
        return Ok(());
    };
    let started_at_ms = parse_captured_timestamp("started_at", started_at)?;
    let ended_at_ms = parse_captured_timestamp("ended_at", ended_at)?;

    if started_at_ms >= ended_at_ms {
        return Err(SeedError::ReceiptTemporalIncoherence {
            field: "started_at".to_string(),
            reason: "must be earlier than `ended_at`".to_string(),
        });
    }
    if started_at_ms > imported_at_ms {
        return Err(SeedError::ReceiptTemporalIncoherence {
            field: "started_at".to_string(),
            reason: "cannot be in the future relative to receipt import time".to_string(),
        });
    }
    if ended_at_ms > imported_at_ms {
        return Err(SeedError::ReceiptTemporalIncoherence {
            field: "ended_at".to_string(),
            reason: "cannot be in the future relative to receipt import time".to_string(),
        });
    }
    if ended_at_ms - started_at_ms > MAX_RECEIPT_WINDOW_MS {
        return Err(SeedError::ReceiptTemporalIncoherence {
            field: "ended_at".to_string(),
            reason: "execution window cannot exceed 24 hours from `started_at`".to_string(),
        });
    }
    Ok(())
}

fn parse_captured_timestamp(field: &str, value: &str) -> Result<u64, SeedError> {
    let bytes = value.as_bytes();
    let invalid = || SeedError::ReceiptTemporalIncoherence {
        field: field.to_string(),
        reason: "must use the captured runner timestamp shape `YYYY-MM-DDTHH:MM:SSZ`".to_string(),
    };
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return Err(invalid());
    }
    let number = |start: usize, end: usize| -> Result<i64, SeedError> {
        std::str::from_utf8(&bytes[start..end])
            .ok()
            .and_then(|part| part.parse::<i64>().ok())
            .ok_or_else(&invalid)
    };
    let year = number(0, 4)?;
    let month = number(5, 7)?;
    let day = number(8, 10)?;
    let hour = number(11, 13)?;
    let minute = number(14, 16)?;
    let second = number(17, 19)?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err(invalid()),
    };
    if day < 1 || day > month_days || hour > 23 || minute > 59 || second > 59 {
        return Err(invalid());
    }
    let days = days_from_civil(year, month, day);
    if days < 0 {
        return Err(invalid());
    }
    let seconds = days
        .checked_mul(86_400)
        .and_then(|base| base.checked_add(hour * 3_600 + minute * 60 + second))
        .ok_or_else(&invalid)?;
    u64::try_from(seconds)
        .ok()
        .and_then(|seconds| seconds.checked_mul(1000))
        .ok_or_else(invalid)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
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

// ===========================================================================
// F11-a — the `candidate_edit` transaction wrapper + the advisory `candidate_lease`.
//
// `candidate_edit` is preflight-on-a-clone (o1): the pure engine
// ([`crate::candidate_edit::apply_edits`]) validates the WHOLE batch against a
// working copy; on any failure the caller persists NOTHING. Only on full success
// does this wrapper save once + bump `store_version` once. The lease is a soft,
// ADVISORY serialization aid (o4): the owner is the single point of serialization,
// so acquire is an atomic compare-and-set and an expired lease is reclaimable by
// anyone (no dead-agent trap). The lease NEVER blocks an edit and never bumps
// `store_version` — it is bookkeeping orthogonal to the OCC edit stream, so it can
// never invalidate a pending edit's OCC key.
// ===========================================================================

/// The default advisory-lease TTL when a caller omits `ttl_secs` (15 minutes) — a
/// live curation turn, short enough that a dead agent's lease self-heals soon.
pub const DEFAULT_LEASE_TTL_SECS: u64 = 900;

/// A `candidate_lease` action (o4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseAction {
    /// Compare-and-set: grant iff the lease is free, expired, or already this agent's.
    Acquire,
    /// Extend the TTL — only for the agent that currently holds it.
    Refresh,
    /// Clear the lease — only for the agent that currently holds it (or a free no-op).
    Release,
}

impl LeaseAction {
    /// Parse the wire string. Unknown values are a caller error.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "acquire" => Ok(Self::Acquire),
            "refresh" => Ok(Self::Refresh),
            "release" => Ok(Self::Release),
            other => Err(format!(
                "action must be \"acquire\", \"refresh\", or \"release\", got \"{other}\""
            )),
        }
    }
}

/// What a `candidate_lease` call did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LeaseSummary {
    /// `acquired` | `refreshed` | `released` | `already_free`.
    pub state: &'static str,
    /// True iff the persisted lease fields actually changed (drives the save).
    pub changed: bool,
    /// The lease holder AFTER the call (`None` = free).
    pub curating_by: Option<String>,
    /// When the lease expires AFTER the call (`None` = free).
    pub curating_until: Option<String>,
}

impl SystemBlockStore {
    /// Whether a live (unexpired) lease is currently held. A lease with a missing or
    /// past `curating_until` is NOT live — it is reclaimable (o4: no dead-agent trap).
    /// Comparison is lexical over fixed-width RFC3339-UTC stamps (as elsewhere).
    /// `pub(crate)`: the gardener's auto-reconcile YIELDS voluntarily to a live
    /// lease (the lease is advisory by ratified law — it never blocks; WE cede).
    pub(crate) fn lease_is_live(&self, now_iso: &str) -> bool {
        self.curating_by.is_some()
            && self
                .curating_until
                .as_deref()
                .is_some_and(|until| until > now_iso)
    }

    /// Apply an advisory-lease action (o4). Pure compare-and-set on the lease fields
    /// against `now_iso`; `until_iso` is the caller-computed expiry (`now + ttl`).
    /// NEVER bumps `store_version` — the lease is advisory bookkeeping, orthogonal to
    /// the OCC edit stream, so it can never block the owner or invalidate a pending
    /// edit's OCC key. A refuse (`LeaseHeld`) leaves the lease untouched.
    pub fn apply_lease(
        &mut self,
        action: LeaseAction,
        agent_id: &str,
        now_iso: &str,
        until_iso: &str,
    ) -> Result<LeaseSummary, SeedError> {
        let holder = self.curating_by.clone();
        let live = self.lease_is_live(now_iso);
        let mine = holder.as_deref() == Some(agent_id);
        match action {
            LeaseAction::Acquire => {
                // Grant iff free, expired, or already ours (idempotent re-acquire).
                if !live || mine {
                    let changed = self.curating_by.as_deref() != Some(agent_id)
                        || self.curating_until.as_deref() != Some(until_iso);
                    self.curating_by = Some(agent_id.to_string());
                    self.curating_until = Some(until_iso.to_string());
                    Ok(self.lease_summary("acquired", changed))
                } else {
                    Err(SeedError::LeaseHeld {
                        held_by: holder.unwrap_or_default(),
                        until: self.curating_until.clone().unwrap_or_default(),
                    })
                }
            }
            LeaseAction::Refresh => {
                // Only the recorded holder may extend; anyone else must acquire.
                if mine {
                    let changed = self.curating_until.as_deref() != Some(until_iso);
                    self.curating_until = Some(until_iso.to_string());
                    Ok(self.lease_summary("refreshed", changed))
                } else {
                    Err(SeedError::LeaseHeld {
                        held_by: holder.unwrap_or_default(),
                        until: self.curating_until.clone().unwrap_or_default(),
                    })
                }
            }
            LeaseAction::Release => {
                if mine {
                    self.curating_by = None;
                    self.curating_until = None;
                    Ok(self.lease_summary("released", true))
                } else if holder.is_none() {
                    // Already free — an idempotent no-op, never a refusal.
                    Ok(self.lease_summary("already_free", false))
                } else {
                    Err(SeedError::LeaseHeld {
                        held_by: holder.unwrap_or_default(),
                        until: self.curating_until.clone().unwrap_or_default(),
                    })
                }
            }
        }
    }

    fn lease_summary(&self, state: &'static str, changed: bool) -> LeaseSummary {
        LeaseSummary {
            state,
            changed,
            curating_by: self.curating_by.clone(),
            curating_until: self.curating_until.clone(),
        }
    }
}

/// Apply a `candidate_lease` action against the store in a brain dir: load ->
/// [`SystemBlockStore::apply_lease`] -> save (only when the lease actually changed).
/// A missing store is a hard [`SeedError::NoStore`]; a `LeaseHeld` refusal leaves the
/// disk intact. Never bumps `store_version`.
pub fn candidate_lease_in_dir(
    dir: &Path,
    action: LeaseAction,
    agent_id: &str,
    now_iso: &str,
    until_iso: &str,
) -> Result<(SystemBlockStore, LeaseSummary), SeedError> {
    let mut store = SystemBlockStore::load(dir)?.ok_or(SeedError::NoStore)?;
    let summary = store.apply_lease(action, agent_id, now_iso, until_iso)?;
    if summary.changed {
        store.save(dir)?;
    }
    Ok((store, summary))
}

/// Apply a `candidate_edit` batch against the store in a brain dir (F11-a §B). The
/// full transaction law, in order — nothing is persisted unless every gate passes:
/// 1. OCC — `expected_store_version` matches, else [`SeedError::Conflict`];
/// 2. candidate-only (§1a) — a `ratified` skeleton refuses every op with
///    [`SeedError::SkeletonNotCandidate`];
/// 3. preflight-on-a-clone (o1) — [`crate::candidate_edit::apply_edits`] validates
///    the WHOLE batch against a working copy; the FIRST failure returns its op index
///    ([`SeedError::CandidateEdit`]) and NOTHING is persisted.
///
/// On full success the edited store is saved ONCE and `store_version` is bumped
/// ONCE. The advisory lease is never consulted — `candidate_edit` NEVER requires a
/// held lease (a dead agent must not block the owner, o4).
pub fn candidate_edit_in_dir(
    dir: &Path,
    expected_store_version: u64,
    ops: &[crate::candidate_edit::EditOp],
    seat: crate::candidate_edit::EditSeat,
) -> Result<SystemBlockStore, SeedError> {
    let store = SystemBlockStore::load(dir)?.ok_or(SeedError::NoStore)?;
    if expected_store_version != store.store_version {
        return Err(SeedError::Conflict {
            expected: expected_store_version,
            actual: store.store_version,
        });
    }
    if store.skeleton.state == SeedSkeletonState::Ratified {
        return Err(SeedError::SkeletonNotCandidate);
    }
    // Preflight-on-a-clone: the engine mutates a working copy and returns it only on
    // total success. Any op or final-invariant failure aborts with its op index and
    // leaves the on-disk store byte-for-byte intact.
    let mut edited = crate::candidate_edit::apply_edits(&store, ops, seat).map_err(|e| {
        SeedError::CandidateEdit {
            op_index: e.op_index,
            reason: e.reason,
        }
    })?;
    // Full success -> one persist, one bump.
    edited.store_version = store.store_version + 1;
    edited.save(dir)?;
    Ok(edited)
}

/// Clear every field a candidate did not earn itself (F0c-a §4b/§4c). This is
/// applied even to internally generated candidates so future callers cannot
/// accidentally smuggle live receipts/fingerprints into a proposed skeleton.
fn sanitize_candidate_seed(mut seed: SeedFile) -> Result<SeedFile, SeedError> {
    seed.skeleton.state = SeedSkeletonState::Candidate;
    seed.skeleton.ratification = SeedRatification {
        method: String::new(),
        ratifier: String::new(),
        ratified_at: String::new(),
        commit: String::new(),
    };
    for block in &mut seed.blocks {
        block.state = SystemBlockState::Candidate;
        block.membership_source = MembershipSource::Proposed;
        block.receipts.clear();
        block.membership_fingerprint = None;
        block.resolved_members.clear();
        block.pre_archive_state = None;
        block.unmapped_residue.clear();
    }
    validate_seed(&seed)?;
    Ok(seed)
}

/// The pure-ish F0c-a store transaction wrapper: load -> state-machine -> atomic
/// save. It deliberately DOES NOT use seed-import semantics (`force`/version
/// reset/operator import). Accepted mutations bump the existing OCC counter by one,
/// while a first absent-store candidate creates v1.
pub fn skeleton_candidate_in_dir(
    dir: &Path,
    candidate_seed: SeedFile,
    expected_store_version: Option<u64>,
) -> Result<(SystemBlockStore, SkeletonCandidateSummary), SeedError> {
    let candidate_seed = sanitize_candidate_seed(candidate_seed)?;
    let current = SystemBlockStore::load(dir)?;
    match (current, expected_store_version) {
        (None, None) => {
            let store = SystemBlockStore::from_seed(candidate_seed);
            store.save(dir)?;
            let summary = SkeletonCandidateSummary {
                transaction_state: SkeletonCandidateTransactionState::CreatedCandidateStore,
                store_version: store.store_version,
                block_count: store.blocks.len(),
                candidate_revision_written: false,
            };
            Ok((store, summary))
        }
        (None, Some(_)) => Err(SeedError::InvalidCandidateTransaction {
            detail: "store is absent; expected_store_version must be null to create candidate v1"
                .to_string(),
        }),
        (Some(_), None) => Err(SeedError::InvalidCandidateTransaction {
            detail: "store already exists; expected_store_version is required for OCC".to_string(),
        }),
        (Some(mut store), Some(expected)) => {
            if expected != store.store_version {
                return Err(SeedError::Conflict {
                    expected,
                    actual: store.store_version,
                });
            }
            match store.skeleton.state {
                SeedSkeletonState::Candidate => {
                    let next_version = store.store_version.saturating_add(1);
                    let mut replacement = SystemBlockStore::from_seed(candidate_seed);
                    replacement.store_version = next_version;
                    // Herança-zero at store level: no reconcile cache and no pending revision.
                    replacement.unmapped_files.clear();
                    replacement.unmapped_total = 0;
                    replacement.candidate_revision = None;
                    replacement.save(dir)?;
                    let summary = SkeletonCandidateSummary {
                        transaction_state:
                            SkeletonCandidateTransactionState::ReplacedCandidateStore,
                        store_version: replacement.store_version,
                        block_count: replacement.blocks.len(),
                        candidate_revision_written: false,
                    };
                    Ok((replacement, summary))
                }
                SeedSkeletonState::Ratified => {
                    store.candidate_revision = Some(Box::new(candidate_seed));
                    store.store_version = store.store_version.saturating_add(1);
                    store.save(dir)?;
                    let summary = SkeletonCandidateSummary {
                        transaction_state:
                            SkeletonCandidateTransactionState::WroteCandidateRevision,
                        store_version: store.store_version,
                        block_count: store.blocks.len(),
                        candidate_revision_written: true,
                    };
                    Ok((store, summary))
                }
            }
        }
    }
}

// ===========================================================================
// Slice 3 — the reconciliation engine (the "architectural git status").
//
// The skeleton reacts to files entering/leaving the repo WITHOUT lying:
// - each block's declared membership (exact paths + globs) is resolved against a
//   real file list into an effective member set and a deterministic fingerprint
//   (HUMAN-VIEW-V2-F0-TECH §2);
// - when a block's resolved set changes, its `boundary_version` bumps — which, by
//   the EXISTING rollup law (PRD §5) and the EXISTING `stale_scope` gate
//   (import_receipt), makes every receipt earned against the older boundary stale
//   BY SCOPE, with no new staleness code (see `reconcile_boundary_bump_...` tests);
// - files claimed by NO block are surfaced as the real unmapped (PRD F7);
// - archive/delete let the owner retire a block honestly (§8).
// The engine (`reconcile_store`, `receipt_recompute`) is PURE — the file list is
// injected; the OCC/persistence wrappers (`*_in_dir`) own version + disk.
// ===========================================================================

use std::collections::BTreeSet;

/// How many unmapped paths the store MATERIALIZES. Unmapped is never hidden (F7),
/// but the store stays bounded — `unmapped_total` always carries the honest full
/// count even when the stored sample is capped at this limit.
pub const UNMAPPED_FILES_CAP: usize = 500;

/// Whether a membership `path` is a glob pattern (claims a SET of files) rather than
/// an exact path. Mirrors the metacharacters `glob::Pattern` recognizes. `pub(crate)`
/// so the F11-c store-block packet builder matches members the same way.
pub(crate) fn is_glob_pattern(path: &str) -> bool {
    path.contains(['*', '?', '['])
}

/// The deterministic fingerprint of a block's resolved membership: `sha256:` + the
/// hex digest of the SORTED, newline-joined member set. Two reconciles that resolve
/// the identical set of real files produce the identical fingerprint; any add or
/// remove flips it. (`sha2` is already a direct dependency — see mailbox.rs.)
fn membership_fingerprint(sorted_members: &[String]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for m in sorted_members {
        hasher.update(m.as_bytes());
        hasher.update(b"\n");
    }
    format!("sha256:{:x}", hasher.finalize())
}

/// Resolve a block's EFFECTIVE membership against a real file list. Exact-path
/// members that exist join the resolved set; exact-path members that are ABSENT are
/// reported `missing`; glob members expand to every matching path (matched on the
/// full repo-relative path with `glob::Pattern`'s default options, where `*`/`**`
/// cross `/` — the "claim this subtree" semantics the seed globs intend). Returns
/// `(resolved, missing)`, both sorted + deduped (via `BTreeSet`) for determinism.
fn resolve_block_membership(
    block: &SystemBlock,
    file_list: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut resolved: BTreeSet<String> = BTreeSet::new();
    let mut missing: BTreeSet<String> = BTreeSet::new();
    for entry in &block.membership {
        if is_glob_pattern(&entry.path) {
            // A malformed pattern claims nothing (it also cannot match a literal).
            if let Ok(pat) = glob::Pattern::new(&entry.path) {
                for f in file_list {
                    if pat.matches(f) {
                        resolved.insert(f.clone());
                    }
                }
            }
        } else if file_list.iter().any(|f| f == &entry.path) {
            resolved.insert(entry.path.clone());
        } else {
            missing.insert(entry.path.clone());
        }
    }
    (
        resolved.into_iter().collect(),
        missing.into_iter().collect(),
    )
}

/// A block's outcome in one reconcile pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconcileOutcome {
    /// First reconcile of this block — the ratified boundary fingerprint is recorded
    /// (no `boundary_version` bump). The ratified fronteira IS this set (honest
    /// baseline).
    Baseline,
    /// The resolved membership changed vs the recorded fingerprint — `boundary_version`
    /// was bumped by one.
    Bumped,
    /// The resolved membership is byte-identical to the recorded fingerprint.
    Unchanged,
}

/// Per-block reconcile detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BlockReconcile {
    pub block_id: String,
    pub outcome: ReconcileOutcome,
    /// The block's `boundary_version` AFTER this pass.
    pub boundary_version: u32,
    /// How many real files the block now resolves to.
    pub resolved_count: usize,
    /// Files that entered the block's resolved set (only on `Bumped`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<String>,
    /// Files that left the block's resolved set (only on `Bumped`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
    /// Declared EXACT members that are absent from the file list (an honest
    /// "declared but gone", reported on any outcome).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
}

/// The whole-store reconcile report (ESCOPO A). Serializable so a verb can embed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReconcileReport {
    /// True iff this reconcile changed PERSISTED state — a baseline fingerprint
    /// write, a boundary bump, or a change in the unmapped set. A no-op reconcile is
    /// `false` and costs NO `store_version` bump: reconcile is idempotent.
    pub dirty: bool,
    /// Every block's outcome, in store order.
    pub blocks: Vec<BlockReconcile>,
    /// Convenience: the ids of blocks whose `boundary_version` bumped this pass.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bumped_block_ids: Vec<String>,
    /// The honest TOTAL count of files claimed by no block (never capped).
    pub unmapped_total: usize,
    /// How many unmapped paths were materialized into the store (≤ the cap).
    pub unmapped_materialized: usize,
    /// The honest staleness note — present iff at least one boundary bumped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// The pure reconciliation engine (ESCOPO A). Deterministic over `(store,
/// file_list)`; performs NO I/O and does NOT touch `store_version` or OCC (the verb
/// wrapper owns those). For each block it resolves the effective membership, records
/// the fingerprint (baseline) or bumps `boundary_version` (change), refreshes the
/// `resolved_members` cache, and accumulates the real unmapped into the store. The
/// returned [`ReconcileReport`] carries `dirty` so the OCC wrapper knows whether to
/// bump + persist.
pub fn reconcile_store(store: &mut SystemBlockStore, file_list: &[String]) -> ReconcileReport {
    let mut blocks_report = Vec::with_capacity(store.blocks.len());
    let mut bumped_block_ids = Vec::new();
    let mut claimed: BTreeSet<String> = BTreeSet::new();
    let mut dirty = false;
    let mut any_bump = false;

    for block in store.blocks.iter_mut() {
        let (resolved, missing) = resolve_block_membership(block, file_list);
        for r in &resolved {
            claimed.insert(r.clone());
        }
        let new_fp = membership_fingerprint(&resolved);

        let (outcome, added, removed) = match &block.membership_fingerprint {
            None => {
                // Baseline: record the ratified fronteira; no boundary bump.
                block.membership_fingerprint = Some(new_fp);
                block.resolved_members = resolved.clone();
                dirty = true;
                (ReconcileOutcome::Baseline, Vec::new(), Vec::new())
            }
            Some(prev_fp) if prev_fp == &new_fp => {
                (ReconcileOutcome::Unchanged, Vec::new(), Vec::new())
            }
            Some(_) => {
                // The fronteira moved — diff against the cache, then bump + refresh.
                let old: BTreeSet<&String> = block.resolved_members.iter().collect();
                let new: BTreeSet<&String> = resolved.iter().collect();
                let added: Vec<String> = new.difference(&old).map(|s| (*s).clone()).collect();
                let removed: Vec<String> = old.difference(&new).map(|s| (*s).clone()).collect();
                block.boundary_version = block.boundary_version.saturating_add(1);
                block.membership_fingerprint = Some(new_fp);
                block.resolved_members = resolved.clone();
                bumped_block_ids.push(block.block_id.clone());
                dirty = true;
                any_bump = true;
                (ReconcileOutcome::Bumped, added, removed)
            }
        };

        blocks_report.push(BlockReconcile {
            block_id: block.block_id.clone(),
            outcome,
            boundary_version: block.boundary_version,
            resolved_count: resolved.len(),
            added,
            removed,
            missing,
        });
    }

    // The REAL unmapped: files claimed by NO block (F7 — never hidden). Sorted +
    // deduped for a deterministic store; the total is honest even when capped.
    let unmapped: Vec<String> = file_list
        .iter()
        .filter(|f| !claimed.contains(*f))
        .cloned()
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect();
    let unmapped_total = unmapped.len();
    let materialized: Vec<String> = unmapped.into_iter().take(UNMAPPED_FILES_CAP).collect();
    if store.unmapped_files != materialized || store.unmapped_total != unmapped_total {
        dirty = true;
    }
    store.unmapped_files = materialized;
    store.unmapped_total = unmapped_total;

    let note = if any_bump {
        Some(
            "receipts previously earned against older boundaries are now stale by scope"
                .to_string(),
        )
    } else {
        None
    };

    ReconcileReport {
        dirty,
        blocks: blocks_report,
        bumped_block_ids,
        unmapped_total,
        unmapped_materialized: store.unmapped_files.len(),
        note,
    }
}

/// Archive/restore mode for [`SystemBlockStore::set_archive`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveMode {
    Archive,
    Restore,
}

impl ArchiveMode {
    fn as_str(self) -> &'static str {
        match self {
            ArchiveMode::Archive => "archive",
            ArchiveMode::Restore => "restore",
        }
    }
}

/// What an archive/restore transaction changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArchiveSummary {
    pub mode: String,
    /// Blocks whose state actually flipped (already-in-target-state blocks are a
    /// silent idempotent no-op, not listed).
    pub changed_block_ids: Vec<String>,
    pub store_version: u64,
}

/// What a delete transaction removed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeleteSummary {
    pub deleted_block_id: String,
    /// How many receipts died with the block (the honest cost of a delete).
    pub receipts_removed: usize,
    pub store_version: u64,
}

/// Per-receipt freshness in a [`RecomputeReport`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReceiptStatus {
    pub block_id: String,
    pub receipt_index: usize,
    pub receipt_type: String,
    pub fresh: bool,
    /// When stale, the FIRST failing reason: `block` | `boundary` | `contract` |
    /// `expired`. Absent when fresh.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The receipt-recompute report (ESCOPO C.2) — a pure READ; history is never
/// deleted, the report IS the truth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecomputeReport {
    pub block_count: usize,
    pub receipt_count: usize,
    pub fresh_count: usize,
    pub stale_count: usize,
    pub receipts: Vec<ReceiptStatus>,
}

/// The first reason a receipt is stale against its block + `now`, or `None` if
/// fresh. A receipt is FRESH iff its scope still binds to the block's CURRENT
/// `(block_id, boundary_version, contract_version)` AND it has not expired.
fn receipt_stale_reason(block: &SystemBlock, receipt: &Receipt, now: &str) -> Option<String> {
    if receipt.scope.block_id != block.block_id {
        return Some("block".to_string());
    }
    if receipt.scope.boundary_version != block.boundary_version {
        return Some("boundary".to_string());
    }
    if receipt.scope.contract_version != block.contract_version {
        return Some("contract".to_string());
    }
    if let Some(expires_on) = &receipt.validity.expires_on {
        // RFC3339 UTC stamps (fixed-width, `Z`) compare lexicographically. Best-effort
        // for a non-canonical stamp — a same-shape compare is correct.
        if expires_on.as_str() < now {
            return Some("expired".to_string());
        }
    }
    None
}

/// Recompute receipt freshness (ESCOPO C.2). PURE READ — never mutates; the report
/// IS the truth (`stale` is derived, not materialized: history stays intact). A
/// named `block_id` that is absent is a hard [`SeedError::BlockNotFound`] (no silent
/// skip); `None` recomputes every block.
pub fn receipt_recompute(
    store: &SystemBlockStore,
    block_id: Option<&str>,
    now: &str,
) -> Result<RecomputeReport, SeedError> {
    if let Some(id) = block_id {
        if !store.blocks.iter().any(|b| b.block_id.as_str() == id) {
            return Err(SeedError::BlockNotFound {
                block_id: id.to_string(),
            });
        }
    }
    let mut receipts = Vec::new();
    let mut block_count = 0usize;
    for block in &store.blocks {
        if let Some(id) = block_id {
            if block.block_id.as_str() != id {
                continue;
            }
        }
        block_count += 1;
        for (i, r) in block.receipts.iter().enumerate() {
            let reason = receipt_stale_reason(block, r, now);
            receipts.push(ReceiptStatus {
                block_id: block.block_id.clone(),
                receipt_index: i,
                receipt_type: receipt_type_str(r.type_).to_string(),
                fresh: reason.is_none(),
                reason,
            });
        }
    }
    let receipt_count = receipts.len();
    let fresh_count = receipts.iter().filter(|s| s.fresh).count();
    Ok(RecomputeReport {
        block_count,
        receipt_count,
        fresh_count,
        stale_count: receipt_count - fresh_count,
        receipts,
    })
}

impl SystemBlockStore {
    /// Reconcile the store against a real file list (ESCOPO C.1). OCC-checked (PRD
    /// §3.1): a stale `expected` is rejected with [`SeedError::Conflict`] and NOTHING
    /// is mutated. The WHOLE reconcile is one atomic mutation — on any change,
    /// `store_version` is bumped exactly ONCE; a no-op reconcile leaves the version
    /// intact (idempotent).
    pub fn reconcile(
        &mut self,
        expected_store_version: u64,
        file_list: &[String],
    ) -> Result<ReconcileReport, SeedError> {
        if expected_store_version != self.store_version {
            return Err(SeedError::Conflict {
                expected: expected_store_version,
                actual: self.store_version,
            });
        }
        let report = reconcile_store(self, file_list);
        if report.dirty {
            self.store_version += 1;
        }
        Ok(report)
    }

    /// Archive or restore blocks (ESCOPO C.3). OCC-checked; an unknown id fails
    /// BEFORE any mutation ([`SeedError::BlockNotFound`]). Archive records the block's
    /// prior state in `pre_archive_state` then flips it to `Archived`; restore returns
    /// it to that REAL prior state (falling back to `Restored` only if none was
    /// recorded). Already-in-target-state blocks are a silent idempotent no-op. Only a
    /// real change bumps `store_version`.
    pub fn set_archive(
        &mut self,
        expected_store_version: u64,
        block_ids: &[String],
        mode: ArchiveMode,
    ) -> Result<ArchiveSummary, SeedError> {
        if expected_store_version != self.store_version {
            return Err(SeedError::Conflict {
                expected: expected_store_version,
                actual: self.store_version,
            });
        }
        for id in block_ids {
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
        let mut changed = Vec::new();
        for block in self.blocks.iter_mut() {
            if !block_ids
                .iter()
                .any(|id| id.as_str() == block.block_id.as_str())
            {
                continue;
            }
            match mode {
                ArchiveMode::Archive => {
                    if block.state != SystemBlockState::Archived {
                        block.pre_archive_state = Some(block.state);
                        block.state = SystemBlockState::Archived;
                        changed.push(block.block_id.clone());
                    }
                }
                ArchiveMode::Restore => {
                    if block.state == SystemBlockState::Archived {
                        block.state = block
                            .pre_archive_state
                            .take()
                            .unwrap_or(SystemBlockState::Restored);
                        changed.push(block.block_id.clone());
                    }
                }
            }
        }
        if !changed.is_empty() {
            self.store_version += 1;
        }
        Ok(ArchiveSummary {
            mode: mode.as_str().to_string(),
            changed_block_ids: changed,
            store_version: self.store_version,
        })
    }

    /// Delete a block from the store FOR REAL (ESCOPO C.4). OCC-checked. `force` is
    /// mandatory — without it the block exists but the call refuses with
    /// [`SeedError::DeleteRequiresForce`] (suggesting archive). With `force`, the block
    /// and all its receipts are removed and `store_version` bumps. An unknown id is a
    /// hard [`SeedError::BlockNotFound`].
    pub fn delete_block(
        &mut self,
        expected_store_version: u64,
        block_id: &str,
        force: bool,
    ) -> Result<DeleteSummary, SeedError> {
        if expected_store_version != self.store_version {
            return Err(SeedError::Conflict {
                expected: expected_store_version,
                actual: self.store_version,
            });
        }
        let idx = self
            .blocks
            .iter()
            .position(|b| b.block_id.as_str() == block_id)
            .ok_or_else(|| SeedError::BlockNotFound {
                block_id: block_id.to_string(),
            })?;
        if !force {
            return Err(SeedError::DeleteRequiresForce {
                block_id: block_id.to_string(),
            });
        }
        let removed = self.blocks.remove(idx);
        let receipts_removed = removed.receipts.len();
        self.store_version += 1;
        Ok(DeleteSummary {
            deleted_block_id: block_id.to_string(),
            receipts_removed,
            store_version: self.store_version,
        })
    }

    /// The honest "active" block count — blocks NOT archived. The backend only MARKS
    /// archived state (never deletes data); the UI rollup excludes archived blocks
    /// from active counts by filtering on exactly this (PRD §5 / F0-TECH §8).
    pub fn active_block_count(&self) -> usize {
        self.blocks
            .iter()
            .filter(|b| b.state != SystemBlockState::Archived)
            .count()
    }
}

/// Reconcile against the store in a brain dir: load -> [`SystemBlockStore::reconcile`]
/// -> save. Saved ONLY when the reconcile changed something (idempotent no-op leaves
/// the disk byte-for-byte intact); an OCC conflict likewise leaves it untouched. A
/// missing store is a hard [`SeedError::NoStore`].
pub fn reconcile_in_dir(
    dir: &Path,
    expected_store_version: u64,
    file_list: &[String],
) -> Result<(SystemBlockStore, ReconcileReport), SeedError> {
    let mut store = SystemBlockStore::load(dir)?.ok_or(SeedError::NoStore)?;
    let report = store.reconcile(expected_store_version, file_list)?;
    if report.dirty {
        store.save(dir)?;
    }
    Ok((store, report))
}

/// Archive/restore against the store in a brain dir: load ->
/// [`SystemBlockStore::set_archive`] -> save (only on a real change). A missing store
/// is a hard [`SeedError::NoStore`]; a conflict / unknown id leaves the disk intact.
pub fn archive_in_dir(
    dir: &Path,
    expected_store_version: u64,
    block_ids: &[String],
    mode: ArchiveMode,
) -> Result<(SystemBlockStore, ArchiveSummary), SeedError> {
    let mut store = SystemBlockStore::load(dir)?.ok_or(SeedError::NoStore)?;
    let summary = store.set_archive(expected_store_version, block_ids, mode)?;
    if !summary.changed_block_ids.is_empty() {
        store.save(dir)?;
    }
    Ok((store, summary))
}

/// Delete against the store in a brain dir: load -> [`SystemBlockStore::delete_block`]
/// -> save. Saved only on success (a `force`-less refusal or conflict returns before
/// the save, leaving the disk intact). A missing store is a hard
/// [`SeedError::NoStore`].
pub fn delete_in_dir(
    dir: &Path,
    expected_store_version: u64,
    block_id: &str,
    force: bool,
) -> Result<(SystemBlockStore, DeleteSummary), SeedError> {
    let mut store = SystemBlockStore::load(dir)?.ok_or(SeedError::NoStore)?;
    let summary = store.delete_block(expected_store_version, block_id, force)?;
    store.save(dir)?;
    Ok((store, summary))
}

/// Recompute receipt freshness against the store in a brain dir (ESCOPO C.2). A pure
/// READ: load -> [`receipt_recompute`], never saves. A missing store is a hard
/// [`SeedError::NoStore`].
pub fn recompute_in_dir(
    dir: &Path,
    block_id: Option<&str>,
    now: &str,
) -> Result<RecomputeReport, SeedError> {
    let store = SystemBlockStore::load(dir)?.ok_or(SeedError::NoStore)?;
    receipt_recompute(&store, block_id, now)
}

/// The repo file list (ESCOPO B) — the source of truth a reconcile resolves against.
/// Prefers git (`git ls-files -z --cached --others --exclude-standard`): tracked +
/// untracked files, honoring `.gitignore`, paths repo-relative with `/`. If git is
/// unavailable or `root` is not a repo, falls back to a filesystem walk that skips
/// `.git/`, `target/`, `node_modules/`, and hidden directories (a minimal sane deny;
/// git is the canonical source, so the fallback is best-effort). The result is sorted
/// + deduped for a deterministic reconcile.
pub fn repo_file_list(root: &Path) -> Result<Vec<String>, SeedError> {
    if let Some(list) = git_file_list(root) {
        return Ok(list);
    }
    walk_file_list(root)
}

/// Ask git for the working set. `-z` (NUL-separated) sidesteps path quoting. Returns
/// `None` (so the caller falls back) when git is missing or `root` is not a repo.
fn git_file_list(root: &Path) -> Option<Vec<String>> {
    let output = std::process::Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let mut list: Vec<String> = output
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).replace('\\', "/"))
        .collect();
    list.sort();
    list.dedup();
    Some(list)
}

/// The no-git fallback: a depth-first filesystem walk under `root`, skipping noise
/// dirs (`.git`, `target`, `node_modules`) and hidden dirs. Paths are repo-relative
/// with `/`.
fn walk_file_list(root: &Path) -> Result<Vec<String>, SeedError> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if file_type.is_dir() {
                if name.starts_with('.') || name == "target" || name == "node_modules" {
                    continue;
                }
                stack.push(entry.path());
            } else if file_type.is_file() {
                if let Ok(rel) = entry.path().strip_prefix(root) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
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
    fn f0c_candidate_fields_are_retrocompatible_and_omitted_when_absent() {
        let seed = load_seed(fixture_seed()).expect("fixture parses");
        assert!(seed.blocks.iter().all(|b| b.candidate_meta.is_none()));
        let exported = export_seed(&seed);
        assert!(
            !exported.contains("candidate_meta"),
            "absent block candidate metadata must not serialize"
        );

        let mut store = SystemBlockStore::from_seed(seed);
        assert!(store.candidate_revision.is_none());
        let store_json = serde_json::to_string_pretty(&store).expect("store serializes");
        assert!(
            !store_json.contains("candidate_revision"),
            "absent candidate revision must not serialize"
        );
        let reparsed: SystemBlockStore = serde_json::from_str(&store_json).expect("store reloads");
        assert_eq!(reparsed, store);

        store.blocks[0].candidate_meta = Some(CandidateMeta {
            named_by: NamedBy::Heuristic,
            needs_owner_naming: true,
            graph_cohesion: None,
            edge_sample_size: 0,
            directory_support: 1.0,
            coverage_ratio: 1.0,
            shared_member_count: 0,
        });
        let with_meta = serde_json::to_string(&store).expect("with meta serializes");
        assert!(with_meta.contains("candidate_meta"));
        assert!(with_meta.contains("needs_owner_naming"));
    }

    #[test]
    fn f0c_slice2_store_without_candidate_fields_loads_clean() {
        let slice2 = r#"{
  "schema": "m1nd-system-block-store-v0",
  "store_version": 7,
  "skeleton": {
    "skeleton_id": "sk_old",
    "version": 1,
    "state": "ratified",
    "ratification": { "method": "pr_merge", "ratifier": "owner", "ratified_at": "2026-07-01T00:00:00Z", "commit": "old" }
  },
  "blocks": [
    {
      "block_id": "sb_old",
      "name": "Old",
      "purpose": "A block written before F0c.",
      "kind": "scanned",
      "state": "ratified",
      "boundary_version": 3,
      "contract_version": 1,
      "membership_source": "ratified",
      "membership": [{ "path": "src/old.rs", "role": "primary" }],
      "sockets": { "inputs": [], "outputs": [], "external": [] },
      "receipt_contract": { "version": 1, "required": [], "optional": [], "waived": [], "declared_by": null, "declared_at": null },
      "receipts": [],
      "layout": { "x": null, "y": null, "locked": false, "algorithm_seed": null, "version": 1 },
      "unmapped_residue": []
    }
  ],
  "unmapped_policy": { "visible": true, "default_action": "leave_unmapped_until_ratified" }
}"#;
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(SystemBlockStore::path_in(dir.path()), slice2).expect("write old store");
        let store = SystemBlockStore::load(dir.path())
            .expect("pre-F0c store loads")
            .expect("present");
        assert!(store.candidate_revision.is_none());
        assert!(store.blocks[0].candidate_meta.is_none());
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
    fn fabricated_receipt_timestamp_shapes_are_refused() {
        let imported_at = parse_captured_timestamp("imported_at", "2026-07-10T12:00:00Z")
            .expect("fixed import time");
        let cases = [
            (
                "2026-07-10T11:00:00Z",
                "2026-07-10T11:00:00Z",
                "started_at",
                "earlier than `ended_at`",
            ),
            (
                "2026-07-10T12:00:01Z",
                "2026-07-10T12:00:02Z",
                "started_at",
                "future relative to receipt import time",
            ),
            (
                "2026-07-09T11:59:58Z",
                "2026-07-10T12:00:00Z",
                "ended_at",
                "cannot exceed 24 hours",
            ),
        ];

        for (started_at, ended_at, field, teaching) in cases {
            let mut store = store_from_fixture();
            let mut evidence = full_exec_evidence();
            evidence.started_at = Some(started_at.to_string());
            evidence.ended_at = Some(ended_at.to_string());
            let receipt = mk_receipt(ReceiptType::Test, "sb_core", 1, 1, evidence);
            let before = serde_json::to_vec(&store).expect("store bytes");
            let err = store
                .import_receipt_at(1, "sb_core", receipt, imported_at)
                .expect_err("fabricated execution window refused");
            let detail = err.to_string();
            assert!(detail.contains(field), "field named in refusal: {detail}");
            assert!(detail.contains(teaching), "specific refusal: {detail}");
            assert!(
                detail.contains("composed from captured artifacts"),
                "teaching present: {detail}"
            );
            assert_eq!(
                serde_json::to_vec(&store).expect("store bytes after refusal"),
                before,
                "refusal leaves store byte-identical"
            );
        }
    }

    #[test]
    fn genuine_runnerd_receipt_imports_byte_identically() {
        let imported_at = parse_captured_timestamp("imported_at", "2026-07-10T12:00:00Z")
            .expect("fixed import time");
        let mut store = store_from_fixture();
        let mut receipt = mk_receipt(ReceiptType::Test, "sb_core", 1, 1, full_exec_evidence());
        receipt.emitter = ReceiptEmitter {
            kind: ReceiptEmitterKind::Runnerd,
            id: "runner-a".to_string(),
        };
        let expected = serde_json::to_vec(&receipt).expect("runnerd receipt bytes");

        store
            .import_receipt_at(1, "sb_core", receipt, imported_at)
            .expect("captured runnerd window imports");

        let imported = store.blocks[0].receipts.last().expect("receipt appended");
        assert_eq!(
            serde_json::to_vec(imported).expect("imported receipt bytes"),
            expected,
            "receipt_import preserves the runnerd-composed wire shape byte-identically"
        );
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

    #[test]
    fn skeleton_candidate_transaction_creates_replaces_and_writes_revision_with_zero_inheritance() {
        let dir = tempfile::tempdir().expect("tempdir");
        let candidate = load_seed(fixture_seed()).expect("candidate seed");

        let (created, create_summary) =
            skeleton_candidate_in_dir(dir.path(), candidate.clone(), None)
                .expect("absent store creates candidate v1");
        assert_eq!(
            create_summary.transaction_state,
            SkeletonCandidateTransactionState::CreatedCandidateStore
        );
        assert_eq!(created.store_version, 1);
        assert_eq!(created.skeleton.state, SeedSkeletonState::Candidate);

        // Contaminate the candidate store with reconcile/cache/receipt state; replace must clear it.
        let mut contaminated = created.clone();
        contaminated.unmapped_files = vec!["old/unmapped.rs".to_string()];
        contaminated.unmapped_total = 1;
        contaminated.candidate_revision = Some(Box::new(candidate.clone()));
        contaminated.blocks[0].receipts.push(mk_receipt(
            ReceiptType::Spec,
            "sb_core",
            1,
            1,
            anchor_only_evidence(),
        ));
        contaminated.blocks[0].membership_fingerprint = Some("sha256:old".to_string());
        contaminated.blocks[0].resolved_members = vec!["src/old.rs".to_string()];
        contaminated.blocks[0].pre_archive_state = Some(SystemBlockState::Candidate);
        contaminated.save(dir.path()).expect("save contaminated");

        let (replaced, replace_summary) =
            skeleton_candidate_in_dir(dir.path(), candidate.clone(), Some(1))
                .expect("candidate store replaces wholesale");
        assert_eq!(
            replace_summary.transaction_state,
            SkeletonCandidateTransactionState::ReplacedCandidateStore
        );
        assert_eq!(replaced.store_version, 2);
        assert!(replaced.candidate_revision.is_none());
        assert!(replaced.unmapped_files.is_empty());
        assert_eq!(replaced.unmapped_total, 0);
        assert!(replaced.blocks.iter().all(|b| b.receipts.is_empty()));
        assert!(replaced
            .blocks
            .iter()
            .all(|b| b.membership_fingerprint.is_none()));
        assert!(replaced
            .blocks
            .iter()
            .all(|b| b.resolved_members.is_empty()));
        assert!(replaced
            .blocks
            .iter()
            .all(|b| b.pre_archive_state.is_none()));

        // Ratified live store keeps receipts/fingerprints; candidate_revision carries none of them.
        let ratified_dir = tempfile::tempdir().expect("ratified tempdir");
        let mut live =
            SystemBlockStore::from_seed(load_seed(fixture_seed()).expect("fixture seed"));
        live.skeleton.state = SeedSkeletonState::Ratified;
        live.blocks[0].state = SystemBlockState::Ratified;
        live.blocks[0].membership_source = MembershipSource::Ratified;
        live.blocks[0].receipts.push(mk_receipt(
            ReceiptType::Spec,
            "sb_core",
            1,
            1,
            anchor_only_evidence(),
        ));
        let live_receipts_before = live.blocks[0].receipts.len();
        live.blocks[0].membership_fingerprint = Some("sha256:live".to_string());
        live.blocks[0].resolved_members = vec!["src/core.rs".to_string()];
        live.unmapped_files = vec!["live/unmapped.rs".to_string()];
        live.unmapped_total = 1;
        live.save(ratified_dir.path()).expect("save live ratified");

        let (after, revision_summary) =
            skeleton_candidate_in_dir(ratified_dir.path(), candidate, Some(1))
                .expect("ratified store gets side revision only");
        assert_eq!(
            revision_summary.transaction_state,
            SkeletonCandidateTransactionState::WroteCandidateRevision
        );
        assert_eq!(after.store_version, 2);
        assert_eq!(
            after.blocks[0].receipts.len(),
            live_receipts_before,
            "live receipts remain"
        );
        assert_eq!(
            after.blocks[0].membership_fingerprint.as_deref(),
            Some("sha256:live")
        );
        assert_eq!(after.unmapped_total, 1, "live reconcile cache remains");
        let revision = after
            .candidate_revision
            .as_ref()
            .expect("candidate revision written");
        assert!(revision.blocks.iter().all(|b| b.receipts.is_empty()));
        assert!(revision
            .blocks
            .iter()
            .all(|b| b.membership_fingerprint.is_none()));
        assert!(revision
            .blocks
            .iter()
            .all(|b| b.resolved_members.is_empty()));
    }

    #[test]
    fn skeleton_candidate_transaction_refuses_invalid_combinations_and_conflict() {
        let dir = tempfile::tempdir().expect("tempdir");
        let candidate = load_seed(fixture_seed()).expect("candidate seed");
        let err = skeleton_candidate_in_dir(dir.path(), candidate.clone(), Some(1))
            .expect_err("absent+expected is invalid");
        assert!(matches!(err, SeedError::InvalidCandidateTransaction { .. }));

        let _ = skeleton_candidate_in_dir(dir.path(), candidate.clone(), None).expect("create");
        let err = skeleton_candidate_in_dir(dir.path(), candidate.clone(), None)
            .expect_err("present+none invalid");
        assert!(matches!(err, SeedError::InvalidCandidateTransaction { .. }));
        let err = skeleton_candidate_in_dir(dir.path(), candidate, Some(99))
            .expect_err("stale OCC conflicts");
        assert!(matches!(
            err,
            SeedError::Conflict {
                expected: 99,
                actual: 1
            }
        ));
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

    // =======================================================================
    // Slice 3 — the reconciliation engine, git file source, and the verbs.
    // =======================================================================

    /// A two-block seed with GLOB membership: `sb_glob` claims `src/**` (+ an
    /// optional exact `README.md`), `sb_other` claims `other/**`. Both ratified at
    /// boundary 1 / contract 1 — the fixture for glob resolution + boundary bumps.
    fn glob_fixture_seed() -> &'static str {
        r#"{
  "schema": "m1nd-system-block-seed-v0",
  "repo": { "repo_id": "repo_g", "root": ".", "source_commit": "g" },
  "skeleton": {
    "skeleton_id": "sk_g",
    "version": 1,
    "state": "ratified",
    "ratification": { "method": "pr_merge", "ratifier": "owner", "ratified_at": "2026-07-09T00:00:00Z", "commit": "g" }
  },
  "blocks": [
    {
      "block_id": "sb_glob",
      "name": "Glob",
      "purpose": "A glob-membership block.",
      "kind": "scanned",
      "state": "ratified",
      "boundary_version": 1,
      "contract_version": 1,
      "membership_source": "ratified",
      "membership": [
        { "path": "src/**", "role": "primary" },
        { "path": "README.md", "role": "docs", "optional": true }
      ],
      "sockets": { "inputs": [], "outputs": [], "external": [] },
      "receipt_contract": { "version": 1, "required": [], "optional": [], "waived": [], "declared_by": null, "declared_at": null },
      "receipts": [],
      "layout": { "x": null, "y": null, "locked": false, "algorithm_seed": null, "version": 1 },
      "unmapped_residue": []
    },
    {
      "block_id": "sb_other",
      "name": "Other",
      "purpose": "Another glob-membership block.",
      "kind": "scanned",
      "state": "ratified",
      "boundary_version": 1,
      "contract_version": 1,
      "membership_source": "ratified",
      "membership": [{ "path": "other/**", "role": "primary" }],
      "sockets": { "inputs": [], "outputs": [], "external": [] },
      "receipt_contract": { "version": 1, "required": [], "optional": [], "waived": [], "declared_by": null, "declared_at": null },
      "receipts": [],
      "layout": { "x": null, "y": null, "locked": false, "algorithm_seed": null, "version": 1 },
      "unmapped_residue": []
    }
  ],
  "unmapped_policy": { "visible": true, "default_action": "leave_unmapped_until_ratified" }
}"#
    }

    fn glob_store() -> SystemBlockStore {
        SystemBlockStore::from_seed(load_seed(glob_fixture_seed()).expect("glob fixture parses"))
    }

    fn files(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    // --- glob resolution + missing exact detection -------------------------

    #[test]
    fn glob_resolves_new_file_and_exact_missing_is_detected() {
        let store = glob_store();
        let block = &store.blocks[0]; // sb_glob: src/** + README.md(optional exact)
        let (resolved, missing) = resolve_block_membership(
            block,
            &files(&["src/a.rs", "src/deep/b.rs", "unrelated.txt"]),
        );
        // The glob `src/**` claims both src files (crossing `/`), never `unrelated.txt`.
        assert_eq!(
            resolved,
            vec!["src/a.rs".to_string(), "src/deep/b.rs".to_string()]
        );
        // The exact `README.md` is absent from the list -> missing.
        assert_eq!(missing, vec!["README.md".to_string()]);
    }

    #[test]
    fn glob_double_star_matches_nested_test_pattern() {
        // The seed uses patterns like `m1nd-ui/src/**/*.test.ts`; prove the crate
        // matches a nested tail on the full repo-relative path.
        let pat = glob::Pattern::new("m1nd-ui/src/**/*.test.ts").expect("compiles");
        assert!(pat.matches("m1nd-ui/src/lib/foo.test.ts"));
        assert!(!pat.matches("m1nd-ui/src/lib/foo.ts"));
    }

    // --- baseline: first reconcile records fingerprint WITHOUT a bump; a second
    //     no-change reconcile is idempotent (zero boundary bumps, no version churn).

    #[test]
    fn reconcile_baseline_records_fingerprint_without_bump_and_is_idempotent() {
        let mut store = glob_store();
        assert!(store.blocks[0].membership_fingerprint.is_none());
        let file_list = files(&["src/a.rs", "other/x.rs"]);

        let r1 = reconcile_store(&mut store, &file_list);
        assert!(r1.dirty, "the baseline write is a real change");
        assert!(
            r1.bumped_block_ids.is_empty(),
            "baseline never bumps a boundary"
        );
        for b in &r1.blocks {
            assert_eq!(b.outcome, ReconcileOutcome::Baseline);
        }
        assert_eq!(
            store.blocks[0].boundary_version, 1,
            "boundary unchanged at baseline"
        );
        assert!(
            store.blocks[0].membership_fingerprint.is_some(),
            "fingerprint recorded"
        );
        assert_eq!(
            store.blocks[0].resolved_members,
            vec!["src/a.rs".to_string()]
        );

        // Second reconcile, identical files -> idempotent: nothing dirty, no bumps.
        let r2 = reconcile_store(&mut store, &file_list);
        assert!(!r2.dirty, "an unchanged reconcile is a no-op");
        assert!(r2.bumped_block_ids.is_empty());
        for b in &r2.blocks {
            assert_eq!(b.outcome, ReconcileOutcome::Unchanged);
        }
        assert_eq!(store.blocks[0].boundary_version, 1);
    }

    // --- boundary bump: adding a file inside a glob bumps ONLY that block --------

    #[test]
    fn reconcile_boundary_bump_touches_only_the_changed_block() {
        let mut store = glob_store();
        reconcile_store(&mut store, &files(&["src/a.rs", "other/x.rs"])); // baseline

        // Add src/b.rs (inside sb_glob's `src/**`); sb_other's files are unchanged.
        let report = reconcile_store(&mut store, &files(&["src/a.rs", "src/b.rs", "other/x.rs"]));
        assert!(report.dirty);
        assert_eq!(report.bumped_block_ids, vec!["sb_glob".to_string()]);
        let glob_b = report
            .blocks
            .iter()
            .find(|b| b.block_id == "sb_glob")
            .unwrap();
        let other_b = report
            .blocks
            .iter()
            .find(|b| b.block_id == "sb_other")
            .unwrap();
        assert_eq!(glob_b.outcome, ReconcileOutcome::Bumped);
        assert_eq!(glob_b.added, vec!["src/b.rs".to_string()]);
        assert!(glob_b.removed.is_empty());
        assert_eq!(glob_b.boundary_version, 2, "the changed block bumped");
        assert_eq!(other_b.outcome, ReconcileOutcome::Unchanged);
        assert_eq!(
            other_b.boundary_version, 1,
            "the untouched block did NOT bump"
        );
        assert_eq!(store.blocks[0].boundary_version, 2);
        assert_eq!(store.blocks[1].boundary_version, 1);
        assert!(
            report.note.is_some(),
            "a bump carries the honest staleness note"
        );
    }

    // --- THE ANTI-LIE CHAIN (the most important test) ---------------------------
    //  earned-fresh receipt -> new file enters the glob -> reconcile -> boundary+1
    //  -> recompute shows the receipt stale(boundary) AND a fresh import of the OLD
    //  scope is rejected (stale_scope). No new staleness code — the bump cascades
    //  through the EXISTING rollup + import_receipt gate.

    #[test]
    fn reconcile_boundary_bump_stales_earned_receipt_end_to_end() {
        let dir = tempfile::tempdir().expect("tempdir");
        glob_store().save(dir.path()).expect("seed the store");
        // store_version starts at 1.

        // Baseline reconcile against the ratified file set.
        let (store, _r) = reconcile_in_dir(dir.path(), 1, &files(&["src/a.rs"])).expect("baseline");
        assert_eq!(store.blocks[0].boundary_version, 1);
        let v = store.store_version; // bumped once by the baseline write

        // Earn a FRESH test receipt scoped to sb_glob @ boundary 1 / contract 1.
        let earned = mk_receipt(ReceiptType::Test, "sb_glob", 1, 1, full_exec_evidence());
        let store = import_receipt_in_dir(dir.path(), v, "sb_glob", earned).expect("earn receipt");
        let v = store.store_version;

        // Recompute now: the receipt is FRESH (scope still binds to boundary 1).
        let before = recompute_in_dir(dir.path(), Some("sb_glob"), "2999-01-01T00:00:00Z")
            .expect("recompute");
        assert_eq!(before.receipt_count, 1);
        assert_eq!(
            before.fresh_count, 1,
            "earned receipt is fresh before the boundary moves"
        );
        assert!(before.receipts[0].reason.is_none());

        // A NEW file enters the glob -> reconcile -> sb_glob boundary bumps 1 -> 2.
        let (store, report) =
            reconcile_in_dir(dir.path(), v, &files(&["src/a.rs", "src/b.rs"])).expect("reconcile");
        assert_eq!(report.bumped_block_ids, vec!["sb_glob".to_string()]);
        assert_eq!(store.blocks[0].boundary_version, 2);
        let v = store.store_version;

        // Recompute: the SAME earned receipt is now STALE by boundary — the bump
        // cascaded, with zero receipt-specific staleness code.
        let after = recompute_in_dir(dir.path(), Some("sb_glob"), "2999-01-01T00:00:00Z")
            .expect("recompute");
        assert_eq!(
            after.stale_count, 1,
            "the earned receipt is now stale by scope"
        );
        assert_eq!(after.receipts[0].reason.as_deref(), Some("boundary"));

        // And importing a fresh receipt carrying the OLD scope (boundary 1) is
        // rejected — the EXISTING anti-poison gate refuses evidence for a version the
        // block no longer is.
        let stale_scoped = mk_receipt(ReceiptType::Test, "sb_glob", 1, 1, full_exec_evidence());
        let err = import_receipt_in_dir(dir.path(), v, "sb_glob", stale_scoped)
            .expect_err("an old-scope receipt must be refused after the bump");
        assert!(matches!(err, SeedError::ReceiptStaleScope { .. }));
    }

    #[test]
    fn recompute_flags_expired_receipt() {
        // A receipt whose scope binds but whose `expires_on` is in the past is stale.
        let mut store = glob_store();
        let mut r = mk_receipt(ReceiptType::Spec, "sb_glob", 1, 1, anchor_only_evidence());
        r.validity.expires_on = Some("2000-01-01T00:00:00Z".to_string());
        store.blocks[0].receipts.push(r);
        let report =
            receipt_recompute(&store, Some("sb_glob"), "2026-07-09T00:00:00Z").expect("recompute");
        assert_eq!(report.receipts[0].reason.as_deref(), Some("expired"));
    }

    #[test]
    fn recompute_unknown_block_is_a_hard_error() {
        let store = glob_store();
        assert!(matches!(
            receipt_recompute(&store, Some("sb_ghost"), "2026-07-09T00:00:00Z"),
            Err(SeedError::BlockNotFound { .. })
        ));
    }

    // --- unmapped real: files outside every boundary surface; cap is honest ------

    #[test]
    fn reconcile_surfaces_real_unmapped_files() {
        let mut store = glob_store();
        // `loose.txt` and `docs/x.md` are claimed by NO block.
        let report = reconcile_store(
            &mut store,
            &files(&["src/a.rs", "other/x.rs", "loose.txt", "docs/x.md"]),
        );
        assert_eq!(report.unmapped_total, 2);
        assert_eq!(
            store.unmapped_files,
            vec!["docs/x.md".to_string(), "loose.txt".to_string()]
        );
        assert_eq!(store.unmapped_total, 2);
    }

    #[test]
    fn reconcile_unmapped_cap_is_honest() {
        let mut store = glob_store();
        // More unmapped files than the cap: total is honest, materialized is capped.
        let mut paths: Vec<String> = (0..UNMAPPED_FILES_CAP + 25)
            .map(|i| format!("loose/f{i:04}.txt"))
            .collect();
        paths.push("src/a.rs".to_string()); // one mapped file, to prove filtering
        let list: Vec<String> = paths;
        let report = reconcile_store(&mut store, &list);
        assert_eq!(
            report.unmapped_total,
            UNMAPPED_FILES_CAP + 25,
            "honest total"
        );
        assert_eq!(
            store.unmapped_files.len(),
            UNMAPPED_FILES_CAP,
            "materialized capped"
        );
        assert_eq!(report.unmapped_materialized, UNMAPPED_FILES_CAP);
    }

    // --- OCC: a stale reconcile is rejected; the store is byte-intact ------------

    #[test]
    fn reconcile_occ_conflict_leaves_store_intact() {
        let dir = tempfile::tempdir().expect("tempdir");
        glob_store().save(dir.path()).expect("save");
        let before = SystemBlockStore::load(dir.path()).unwrap().unwrap();
        let err = reconcile_in_dir(dir.path(), 99, &files(&["src/a.rs"]))
            .expect_err("stale expected must conflict");
        assert!(matches!(
            err,
            SeedError::Conflict {
                expected: 99,
                actual: 1
            }
        ));
        let after = SystemBlockStore::load(dir.path()).unwrap().unwrap();
        assert_eq!(
            after, before,
            "a rejected reconcile must not touch the store"
        );
    }

    #[test]
    fn reconcile_missing_store_is_no_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(matches!(
            reconcile_in_dir(dir.path(), 1, &files(&["src/a.rs"])),
            Err(SeedError::NoStore)
        ));
    }

    // --- archive / restore: flips + the rollup-exclusion mark; unknown/OCC -------

    #[test]
    fn archive_then_restore_flips_state_and_marks_rollup_exclusion() {
        let dir = tempfile::tempdir().expect("tempdir");
        store_from_fixture().save(dir.path()).expect("save"); // sb_core ratified, sb_api candidate
        assert_eq!(
            SystemBlockStore::load(dir.path())
                .unwrap()
                .unwrap()
                .active_block_count(),
            2
        );

        // Archive sb_core: state -> archived, prior state remembered, active count drops.
        let (store, summary) = archive_in_dir(
            dir.path(),
            1,
            &["sb_core".to_string()],
            ArchiveMode::Archive,
        )
        .expect("archive");
        assert_eq!(summary.changed_block_ids, vec!["sb_core".to_string()]);
        assert_eq!(summary.store_version, 2);
        let core = store
            .blocks
            .iter()
            .find(|b| b.block_id == "sb_core")
            .unwrap();
        assert_eq!(core.state, SystemBlockState::Archived);
        assert_eq!(core.pre_archive_state, Some(SystemBlockState::Ratified));
        assert_eq!(
            store.active_block_count(),
            1,
            "archived block excluded from active count"
        );

        // Restore: returns to the REAL prior state (ratified), not a fabricated one.
        let (store, summary) = archive_in_dir(
            dir.path(),
            2,
            &["sb_core".to_string()],
            ArchiveMode::Restore,
        )
        .expect("restore");
        assert_eq!(summary.store_version, 3);
        let core = store
            .blocks
            .iter()
            .find(|b| b.block_id == "sb_core")
            .unwrap();
        assert_eq!(
            core.state,
            SystemBlockState::Ratified,
            "restored to the true prior state"
        );
        assert_eq!(
            core.pre_archive_state, None,
            "prior state cleared on restore"
        );
        assert_eq!(store.active_block_count(), 2);
    }

    #[test]
    fn archive_unknown_block_and_occ_conflict() {
        let dir = tempfile::tempdir().expect("tempdir");
        store_from_fixture().save(dir.path()).expect("save");
        assert!(matches!(
            archive_in_dir(
                dir.path(),
                1,
                &["sb_ghost".to_string()],
                ArchiveMode::Archive
            ),
            Err(SeedError::BlockNotFound { .. })
        ));
        assert!(matches!(
            archive_in_dir(
                dir.path(),
                99,
                &["sb_core".to_string()],
                ArchiveMode::Archive
            ),
            Err(SeedError::Conflict { .. })
        ));
    }

    // --- delete: force-less refusal (intact); force removes + counts receipts ----

    #[test]
    fn delete_without_force_is_refused_and_store_intact() {
        let dir = tempfile::tempdir().expect("tempdir");
        store_from_fixture().save(dir.path()).expect("save");
        let before = SystemBlockStore::load(dir.path()).unwrap().unwrap();
        let err = delete_in_dir(dir.path(), 1, "sb_core", false)
            .expect_err("a force-less delete is refused");
        assert!(matches!(err, SeedError::DeleteRequiresForce { .. }));
        let after = SystemBlockStore::load(dir.path()).unwrap().unwrap();
        assert_eq!(after, before, "a refused delete must not touch the store");
    }

    #[test]
    fn delete_with_force_removes_block_and_reports_dead_receipts() {
        let dir = tempfile::tempdir().expect("tempdir");
        store_from_fixture().save(dir.path()).expect("save");
        // sb_core carries one receipt in the fixture.
        let (store, summary) = delete_in_dir(dir.path(), 1, "sb_core", true).expect("force delete");
        assert_eq!(summary.deleted_block_id, "sb_core");
        assert_eq!(
            summary.receipts_removed, 1,
            "the block's receipt died with it"
        );
        assert_eq!(summary.store_version, 2);
        assert!(
            store.blocks.iter().all(|b| b.block_id != "sb_core"),
            "block is gone"
        );
        assert_eq!(store.blocks.len(), 1);
    }

    #[test]
    fn delete_unknown_and_occ_conflict() {
        let dir = tempfile::tempdir().expect("tempdir");
        store_from_fixture().save(dir.path()).expect("save");
        assert!(matches!(
            delete_in_dir(dir.path(), 1, "sb_ghost", true),
            Err(SeedError::BlockNotFound { .. })
        ));
        assert!(matches!(
            delete_in_dir(dir.path(), 99, "sb_core", true),
            Err(SeedError::Conflict { .. })
        ));
    }

    // --- ESCOPO B: git as the file source, with the no-git fallback --------------

    fn run_git(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "git {args:?} failed");
    }

    #[test]
    fn repo_file_list_from_git_includes_untracked_excludes_gitignored() {
        let dir = tempfile::tempdir().expect("tempdir");
        run_git(dir.path(), &["init", "-q"]);
        std::fs::write(dir.path().join("tracked.rs"), "//\n").expect("write tracked");
        run_git(dir.path(), &["add", "tracked.rs"]);
        run_git(
            dir.path(),
            &[
                "-c",
                "user.email=t@example.com",
                "-c",
                "user.name=t",
                "commit",
                "-q",
                "-m",
                "init",
            ],
        );
        // Untracked (not ignored) -> included; gitignored -> excluded.
        std::fs::write(dir.path().join("untracked.rs"), "//\n").expect("write untracked");
        std::fs::write(dir.path().join(".gitignore"), "ignored.rs\n").expect("write gitignore");
        std::fs::write(dir.path().join("ignored.rs"), "//\n").expect("write ignored");

        let list = repo_file_list(dir.path()).expect("git file list");
        assert!(
            list.contains(&"tracked.rs".to_string()),
            "tracked present: {list:?}"
        );
        assert!(
            list.contains(&"untracked.rs".to_string()),
            "untracked present: {list:?}"
        );
        assert!(
            !list.contains(&"ignored.rs".to_string()),
            "gitignored excluded: {list:?}"
        );
    }

    #[test]
    fn repo_file_list_falls_back_to_walk_without_git() {
        // A plain tempdir (not a git repo) -> git ls-files fails -> filesystem walk.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.rs"), "//\n").expect("write a");
        std::fs::create_dir_all(dir.path().join("sub")).expect("mk sub");
        std::fs::write(dir.path().join("sub/b.rs"), "//\n").expect("write b");
        // `target/` is on the fallback deny-list.
        std::fs::create_dir_all(dir.path().join("target")).expect("mk target");
        std::fs::write(dir.path().join("target/c.rs"), "//\n").expect("write c");

        let list = repo_file_list(dir.path()).expect("walk file list");
        assert!(list.contains(&"a.rs".to_string()), "a.rs present: {list:?}");
        assert!(
            list.contains(&"sub/b.rs".to_string()),
            "nested file present: {list:?}"
        );
        assert!(
            !list.iter().any(|p| p.starts_with("target/")),
            "target/ is skipped by the fallback walk: {list:?}"
        );
    }

    // --- retro-compat: a Slice-2 store (no Slice-3 fields) loads clean -----------

    #[test]
    fn slice2_store_without_slice3_fields_loads_clean() {
        // A store exactly as Slice 2 would have written it: no membership_fingerprint,
        // no resolved_members, no pre_archive_state, no unmapped_files/total.
        let slice2 = r#"{
  "schema": "m1nd-system-block-store-v0",
  "store_version": 7,
  "skeleton": {
    "skeleton_id": "sk_old",
    "version": 1,
    "state": "ratified",
    "ratification": { "method": "pr_merge", "ratifier": "owner", "ratified_at": "2026-07-01T00:00:00Z", "commit": "old" }
  },
  "blocks": [
    {
      "block_id": "sb_old",
      "name": "Old",
      "purpose": "A block written before Slice 3.",
      "kind": "scanned",
      "state": "ratified",
      "boundary_version": 3,
      "contract_version": 1,
      "membership_source": "ratified",
      "membership": [{ "path": "src/old.rs", "role": "primary" }],
      "sockets": { "inputs": [], "outputs": [], "external": [] },
      "receipt_contract": { "version": 1, "required": [], "optional": [], "waived": [], "declared_by": null, "declared_at": null },
      "receipts": [],
      "layout": { "x": null, "y": null, "locked": false, "algorithm_seed": null, "version": 1 },
      "unmapped_residue": []
    }
  ],
  "unmapped_policy": { "visible": true, "default_action": "leave_unmapped_until_ratified" }
}"#;
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(SystemBlockStore::path_in(dir.path()), slice2).expect("write slice2 store");
        let store = SystemBlockStore::load(dir.path())
            .expect("a pre-Slice-3 store loads")
            .expect("present");
        assert_eq!(store.store_version, 7);
        assert!(
            store.blocks[0].membership_fingerprint.is_none(),
            "fingerprint defaults to None"
        );
        assert!(
            store.blocks[0].resolved_members.is_empty(),
            "resolved_members defaults empty"
        );
        assert!(
            store.blocks[0].pre_archive_state.is_none(),
            "pre_archive_state defaults None"
        );
        assert!(
            store.unmapped_files.is_empty(),
            "unmapped_files defaults empty"
        );
        assert_eq!(store.unmapped_total, 0, "unmapped_total defaults 0");

        // And it reconciles cleanly on top (baseline over its real member).
        let report = reconcile_store(&mut store.clone(), &files(&["src/old.rs"]));
        assert!(report.dirty);
    }

    #[test]
    fn fresh_store_json_omits_slice3_fields_for_byte_stability() {
        // A never-reconciled store serializes WITHOUT any Slice-3 keys, so it stays
        // byte-identical to a Slice-2 store (skip_serializing_if defaults).
        let store = glob_store();
        let json = serde_json::to_value(&store).expect("serialize");
        assert!(
            json.get("unmapped_files").is_none(),
            "no unmapped_files when empty"
        );
        assert!(
            json.get("unmapped_total").is_none(),
            "no unmapped_total when zero"
        );
        assert!(
            json["blocks"][0].get("membership_fingerprint").is_none(),
            "no fingerprint before the first reconcile"
        );
        assert!(json["blocks"][0].get("resolved_members").is_none());
        assert!(json["blocks"][0].get("pre_archive_state").is_none());
    }

    // =======================================================================
    // F11-a — candidate_edit transaction, the ratify provenance gate (o6), and
    // the advisory curation lease (o4).
    // =======================================================================

    use crate::candidate_edit::{EditOp, EditSeat};

    /// A candidate block with an exact-primary membership and a `named_by:heuristic`
    /// meta (`needs_owner_naming` per the flag).
    fn f11_block(
        id: &str,
        members: &[&str],
        needs_owner_naming: bool,
        named_by: NamedBy,
    ) -> SystemBlock {
        SystemBlock {
            block_id: id.to_string(),
            name: format!("Name {id}"),
            purpose: "p".to_string(),
            kind: SystemBlockKind::Scanned,
            state: SystemBlockState::Candidate,
            boundary_version: 1,
            contract_version: 1,
            membership_source: MembershipSource::Proposed,
            membership: members
                .iter()
                .map(|p| MembershipEntry {
                    path: p.to_string(),
                    role: MembershipRole::Primary,
                    optional: false,
                })
                .collect(),
            sockets: Sockets {
                inputs: Vec::new(),
                outputs: Vec::new(),
                external: Vec::new(),
            },
            receipt_contract: ReceiptContract {
                version: 1,
                required: Vec::new(),
                optional: Vec::new(),
                waived: Vec::new(),
                declared_by: None,
                declared_at: None,
            },
            receipts: Vec::new(),
            layout: Layout {
                x: None,
                y: None,
                locked: false,
                algorithm_seed: None,
                version: 1,
            },
            unmapped_residue: Vec::new(),
            membership_fingerprint: None,
            resolved_members: Vec::new(),
            pre_archive_state: None,
            candidate_meta: Some(CandidateMeta {
                named_by,
                needs_owner_naming,
                graph_cohesion: None,
                edge_sample_size: 0,
                directory_support: 1.0,
                coverage_ratio: 1.0,
                shared_member_count: 0,
            }),
        }
    }

    /// A candidate store (skeleton.state == candidate) with the given blocks, at v1.
    fn f11_candidate_store(blocks: Vec<SystemBlock>) -> SystemBlockStore {
        let mut store = store_from_fixture();
        store.skeleton.state = SeedSkeletonState::Candidate;
        store.blocks = blocks;
        store
    }

    #[test]
    fn candidate_edit_ratified_skeleton_refuses_every_edit_op() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store =
            f11_candidate_store(vec![f11_block("sb_a", &["a1"], true, NamedBy::Heuristic)]);
        store.skeleton.state = SeedSkeletonState::Ratified; // a signed boundary
        store.save(dir.path()).expect("save");
        let ops = vec![EditOp::Rename {
            block_id: "sb_a".to_string(),
            name: Some("Auth".to_string()),
            purpose: None,
        }];
        let err = candidate_edit_in_dir(dir.path(), 1, &ops, EditSeat::Owner)
            .expect_err("a ratified skeleton refuses every op");
        assert!(matches!(err, SeedError::SkeletonNotCandidate));
        assert!(
            err.to_string().contains("skeleton_not_candidate"),
            "honest keyword: {err}"
        );
    }

    #[test]
    fn candidate_edit_preflight_abort_persists_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        f11_candidate_store(vec![
            f11_block("sb_a", &["a1", "a2"], true, NamedBy::Heuristic),
            f11_block("sb_b", &["b1"], true, NamedBy::Heuristic),
        ])
        .save(dir.path())
        .expect("save");
        // The exact bytes on disk before the failing batch.
        let before = std::fs::read(SystemBlockStore::path_in(dir.path())).expect("read before");

        // A batch whose FIRST op is valid but SECOND op is invalid (moves a member the
        // source block does not have) — the whole batch must abort before any persist.
        let ops = vec![
            EditOp::Rename {
                block_id: "sb_a".to_string(),
                name: Some("Renamed".to_string()),
                purpose: None,
            },
            EditOp::MoveMember {
                path: "not-a-member".to_string(),
                from: "sb_a".to_string(),
                to: "sb_b".to_string(),
            },
        ];
        let err = candidate_edit_in_dir(dir.path(), 1, &ops, EditSeat::Owner)
            .expect_err("the batch aborts on the invalid middle op");
        match err {
            SeedError::CandidateEdit { op_index, .. } => {
                assert_eq!(op_index, 1, "the second op is named as the offender");
            }
            other => panic!("expected CandidateEdit, got {other:?}"),
        }
        // The store on disk is BYTE-IDENTICAL — a partial apply persisted nothing (o1).
        let after = std::fs::read(SystemBlockStore::path_in(dir.path())).expect("read after");
        assert_eq!(before, after, "a preflight abort must persist nothing");
    }

    #[test]
    fn candidate_edit_occ_conflict_leaves_store_intact() {
        let dir = tempfile::tempdir().expect("tempdir");
        f11_candidate_store(vec![f11_block("sb_a", &["a1"], false, NamedBy::Owner)])
            .save(dir.path())
            .expect("save");
        let before = SystemBlockStore::load(dir.path()).unwrap().unwrap();
        let ops = vec![EditOp::Rename {
            block_id: "sb_a".to_string(),
            name: Some("New".to_string()),
            purpose: None,
        }];
        let err = candidate_edit_in_dir(dir.path(), 99, &ops, EditSeat::Owner)
            .expect_err("a stale expected version conflicts");
        assert!(matches!(
            err,
            SeedError::Conflict {
                expected: 99,
                actual: 1
            }
        ));
        let after = SystemBlockStore::load(dir.path()).unwrap().unwrap();
        assert_eq!(after, before, "a rejected edit must not touch the store");
    }

    #[test]
    fn candidate_edit_success_persists_once_and_bumps_once() {
        let dir = tempfile::tempdir().expect("tempdir");
        f11_candidate_store(vec![f11_block("sb_a", &["a1"], true, NamedBy::Heuristic)])
            .save(dir.path())
            .expect("save");
        let ops = vec![EditOp::Rename {
            block_id: "sb_a".to_string(),
            name: Some("Auth".to_string()),
            purpose: Some("The auth boundary.".to_string()),
        }];
        let store =
            candidate_edit_in_dir(dir.path(), 1, &ops, EditSeat::Owner).expect("edit lands");
        assert_eq!(store.store_version, 2, "one accepted batch bumps once");
        // The on-disk store matches the returned one and carries the owner provenance.
        let reloaded = SystemBlockStore::load(dir.path()).unwrap().unwrap();
        assert_eq!(reloaded, store, "persisted exactly the returned store");
        let meta = reloaded.blocks[0].candidate_meta.as_ref().unwrap();
        assert_eq!(meta.named_by, NamedBy::Owner);
        assert!(!meta.needs_owner_naming);
    }

    // --- o6: the ratify provenance gate ------------------------------------

    #[test]
    fn ratify_refuses_an_untouched_heuristic_block() {
        let dir = tempfile::tempdir().expect("tempdir");
        f11_candidate_store(vec![f11_block("sb_a", &["a1"], true, NamedBy::Heuristic)])
            .save(dir.path())
            .expect("save");
        let before = SystemBlockStore::load(dir.path()).unwrap().unwrap();
        let err = ratify_in_dir(dir.path(), 1, None, "owner", "2026-07-10T00:00:00Z")
            .expect_err("an untouched heuristic block cannot be ratified");
        match err {
            SeedError::NeedsOwnerNaming { block_id } => assert_eq!(block_id, "sb_a"),
            other => panic!("expected NeedsOwnerNaming, got {other:?}"),
        }
        assert!(
            err_message(&SeedError::NeedsOwnerNaming {
                block_id: "sb_a".to_string()
            })
            .contains("needs_owner_naming"),
            "honest keyword"
        );
        // The gate is pre-mutation: nothing changed on disk.
        let after = SystemBlockStore::load(dir.path()).unwrap().unwrap();
        assert_eq!(after, before, "a gated ratify must not touch the store");
    }

    #[test]
    fn ratify_allows_runner_named() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Runner-named (needs_owner_naming == false) ratifies without an individual touch.
        f11_candidate_store(vec![f11_block("sb_a", &["a1"], false, NamedBy::Runner)])
            .save(dir.path())
            .expect("save");
        let (store, summary) = ratify_in_dir(dir.path(), 1, None, "owner", "2026-07-10T00:00:00Z")
            .expect("a runner-named block ratifies");
        assert_eq!(summary.ratified_block_ids, vec!["sb_a".to_string()]);
        assert_eq!(store.store_version, 2);
        assert_eq!(store.blocks[0].state, SystemBlockState::Ratified);
    }

    #[test]
    fn ratify_owner_named_block_passes_the_gate() {
        // An owner touch (NamedBy::Owner, needs_owner_naming cleared) ratifies too.
        let dir = tempfile::tempdir().expect("tempdir");
        f11_candidate_store(vec![f11_block("sb_a", &["a1"], false, NamedBy::Owner)])
            .save(dir.path())
            .expect("save");
        let (store, _) =
            ratify_in_dir(dir.path(), 1, None, "owner", "t").expect("owner-named ratifies");
        assert_eq!(store.blocks[0].state, SystemBlockState::Ratified);
    }

    // --- o4: the advisory curation lease -----------------------------------

    #[test]
    fn lease_acquire_is_atomic_and_expired_is_reclaimable() {
        let dir = tempfile::tempdir().expect("tempdir");
        f11_candidate_store(vec![f11_block("sb_a", &["a1"], true, NamedBy::Heuristic)])
            .save(dir.path())
            .expect("save");

        // Agent A acquires a lease valid 00:00 -> 00:15.
        let (s1, sum1) = candidate_lease_in_dir(
            dir.path(),
            LeaseAction::Acquire,
            "agentA",
            "2026-07-10T00:00:00Z",
            "2026-07-10T00:15:00Z",
        )
        .expect("agent A acquires");
        assert_eq!(sum1.state, "acquired");
        assert_eq!(s1.curating_by.as_deref(), Some("agentA"));
        assert_eq!(s1.store_version, 1, "the lease never bumps store_version");

        // Agent B is refused while A's lease is LIVE (now 00:05 < until 00:15).
        let err = candidate_lease_in_dir(
            dir.path(),
            LeaseAction::Acquire,
            "agentB",
            "2026-07-10T00:05:00Z",
            "2026-07-10T00:20:00Z",
        )
        .expect_err("a live lease is not stealable");
        match err {
            SeedError::LeaseHeld { held_by, .. } => assert_eq!(held_by, "agentA"),
            other => panic!("expected LeaseHeld, got {other:?}"),
        }

        // After expiry (now 00:30 > until 00:15) the lease is reclaimable by anyone.
        let (s3, sum3) = candidate_lease_in_dir(
            dir.path(),
            LeaseAction::Acquire,
            "agentB",
            "2026-07-10T00:30:00Z",
            "2026-07-10T00:45:00Z",
        )
        .expect("an expired lease is reclaimable — no dead-agent trap");
        assert_eq!(sum3.state, "acquired");
        assert_eq!(s3.curating_by.as_deref(), Some("agentB"));
        assert_eq!(s3.store_version, 1, "still no version churn from the lease");

        // The holder can release; a non-holder cannot.
        let held = candidate_lease_in_dir(
            dir.path(),
            LeaseAction::Release,
            "agentA",
            "2026-07-10T00:31:00Z",
            "2026-07-10T00:31:00Z",
        )
        .expect_err("only the holder releases");
        assert!(matches!(held, SeedError::LeaseHeld { .. }));
        let (s5, sum5) = candidate_lease_in_dir(
            dir.path(),
            LeaseAction::Release,
            "agentB",
            "2026-07-10T00:31:00Z",
            "2026-07-10T00:31:00Z",
        )
        .expect("the holder releases");
        assert_eq!(sum5.state, "released");
        assert!(s5.curating_by.is_none(), "the lease is free again");
    }

    #[test]
    fn lease_is_advisory_edit_never_requires_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        f11_candidate_store(vec![f11_block("sb_a", &["a1"], false, NamedBy::Owner)])
            .save(dir.path())
            .expect("save");
        // Agent A holds a LIVE lease.
        candidate_lease_in_dir(
            dir.path(),
            LeaseAction::Acquire,
            "agentA",
            "2026-07-10T00:00:00Z",
            "2026-07-10T00:15:00Z",
        )
        .expect("agent A acquires");

        // Agent B (NOT the lease holder) edits anyway — the lease is ADVISORY, so the
        // edit succeeds and the owner is never blocked.
        let ops = vec![EditOp::Rename {
            block_id: "sb_a".to_string(),
            name: Some("Edited By B".to_string()),
            purpose: None,
        }];
        let store = candidate_edit_in_dir(dir.path(), 1, &ops, EditSeat::Owner)
            .expect("the edit ignores the advisory lease");
        assert_eq!(store.blocks[0].name, "Edited By B");
        assert_eq!(store.store_version, 2);
        // The edit preserves the advisory lease untouched (it only warns).
        assert_eq!(store.curating_by.as_deref(), Some("agentA"));
    }

    #[test]
    fn f11_lease_fields_are_retrocompatible_and_omitted_when_absent() {
        // A fresh store serializes WITHOUT the lease keys (byte-stable vs an era-prior
        // store).
        let store = f11_candidate_store(vec![f11_block("sb_a", &["a1"], true, NamedBy::Heuristic)]);
        let json = serde_json::to_value(&store).expect("serialize");
        assert!(
            json.get("curating_by").is_none(),
            "no curating_by when free"
        );
        assert!(
            json.get("curating_until").is_none(),
            "no curating_until when free"
        );

        // A pre-F11 store JSON (no lease fields) loads clean with both defaulting None.
        let dir = tempfile::tempdir().expect("tempdir");
        let pre_f11 = r#"{
  "schema": "m1nd-system-block-store-v0",
  "store_version": 4,
  "skeleton": {
    "skeleton_id": "sk_old",
    "version": 1,
    "state": "candidate",
    "ratification": { "method": "", "ratifier": "", "ratified_at": "", "commit": "" }
  },
  "blocks": [
    {
      "block_id": "sb_old",
      "name": "Old",
      "purpose": "A block written before F11.",
      "kind": "scanned",
      "state": "candidate",
      "boundary_version": 1,
      "contract_version": 1,
      "membership_source": "proposed",
      "membership": [{ "path": "src/old.rs", "role": "primary" }],
      "sockets": { "inputs": [], "outputs": [], "external": [] },
      "receipt_contract": { "version": 1, "required": [], "optional": [], "waived": [], "declared_by": null, "declared_at": null },
      "receipts": [],
      "layout": { "x": null, "y": null, "locked": false, "algorithm_seed": null, "version": 1 },
      "unmapped_residue": []
    }
  ],
  "unmapped_policy": { "visible": true, "default_action": "leave_unmapped_until_ratified" }
}"#;
        std::fs::write(SystemBlockStore::path_in(dir.path()), pre_f11)
            .expect("write pre-f11 store");
        let loaded = SystemBlockStore::load(dir.path())
            .expect("a pre-F11 store loads")
            .expect("present");
        assert!(loaded.curating_by.is_none(), "curating_by defaults None");
        assert!(
            loaded.curating_until.is_none(),
            "curating_until defaults None"
        );
        // And it roundtrips byte-stable (the lease fields never appear).
        assert!(
            !serde_json::to_string(&loaded)
                .unwrap()
                .contains("curating_"),
            "no lease keys are written for a free store"
        );
    }

    /// Small helper: the Display string of a SeedError (used to assert honest keywords).
    fn err_message(err: &SeedError) -> String {
        err.to_string()
    }
}
