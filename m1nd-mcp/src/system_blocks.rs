//! Human View v2 F0a — SystemBlock contract seed types and validator.
//!
//! This module is deliberately data-only for slice 1: it models the ratified
//! seed contract (`m1nd-system-block-seed-v0`), validates import-time safety
//! invariants, and exports a deterministic pretty JSON form. It does not add
//! verbs, routes, UI, runner execution, or a live sidecar store.

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptEvidence {
    pub command: String,
    pub cwd: String,
    pub exit_status: i32,
    pub started_at: String,
    pub ended_at: String,
    pub artifact_hash: String,
    pub stdout_excerpt: String,
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
}

impl fmt::Display for SeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SeedError::Json(err) => write!(f, "invalid SystemBlock seed JSON: {err}"),
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
        }
    }
}

impl Error for SeedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            SeedError::Json(err) => Some(err),
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
        }
    }
    Ok(())
}

fn validate_receipt_scope(block: &SystemBlock, receipt: &Receipt) -> Result<(), SeedError> {
    validate_repo_relative_path(&receipt.evidence.cwd)?;
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

fn validate_repo_relative_path(path: &str) -> Result<(), SeedError> {
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
}
