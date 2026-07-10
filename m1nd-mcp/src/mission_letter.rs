//! HUMAN VIEW v2 — F2.5a: the mission letter contract and its safety laws
//! (HUMAN-VIEW-V2-F25-TECH §1, §2, §6-F2.5a).
//!
//! The write mode's unit of state is a **mission letter**: a JSON document
//! describing one mission's live state, emitted by whoever runs the mission and
//! consumed by the tray. This module owns the frozen `m1nd-mission-letter-v0`
//! shape, the per-phase field gating (incl. the §1d `landed` law), the §1e head
//! CAS (mission letters form a content-hash chain), and the append path that
//! reuses the mailbox transport (§2). It NEVER touches the SystemBlockStore — a
//! mission letter is STATE, not evidence (§1c): color only ever changes through
//! `receipt_import`, unchanged.
//!
//! ## The laws this module enforces (the anti-lie core)
//!
//! - **(1b/1d) Per-phase gating + the landed law.** A letter in `executing`
//!   carries no verdict; `merge_wait` requires a `gate`; **`landed` is RESERVED
//!   for "the receipt is confirmed in the store"** — it demands
//!   `receipt.imported == true` with a real `store_version`. A zero-exit gate
//!   without an imported receipt is `merge_wait` ("gate green — receipt not
//!   landed"), never `landed`.
//! - **(1e) Ordering is causal, not clock-based.** Each mission's letters form a
//!   hash chain: `mission_seq` increments by 1 and `prev_letter_id` names the
//!   prior letter's content id (the same `sha256[0..12]` the mailbox uses). A
//!   letter whose `prev_letter_id` does not match the current head is REJECTED at
//!   post time (`stale_head`, CAS). Append-only storage is preserved; the CAS
//!   guards the head pointer, not the log.
//! - **(1f) No absolute paths in the public contract.** `brain_ref` is a
//!   reference string (display name / repo_id), never a path; the guard here
//!   refuses an absolute-looking `brain_ref`. Host-local detail (worktree paths)
//!   lives in the owner-runtime-local side record ([`crate::mission_local`]),
//!   never in this schema.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::mailbox::{self, Letter};
// Reuse the receipt taxonomy from the SystemBlock store where it fits (§6-F2.5a:
// "reuse the receipt types where they fit, without forcing the coupling"). The
// candidate's `type` and evidence anchor ARE the receipt's — reusing them keeps a
// candidate directly convertible to a real receipt at import time (F2.5d). Only
// the scope differs (a candidate has no resolution_hash / block_id-in-scope yet),
// so that one field stays local ([`CandidateScope`]).
use crate::system_blocks::{ReceiptEvidence, ReceiptType};

/// The frozen mission-letter schema tag (§1).
pub const MISSION_LETTER_SCHEMA: &str = "m1nd-mission-letter-v0";

/// The mailbox `kind` a mission letter files under (§2a). A `Letter` with this
/// `kind` and a `mission` payload is a mission letter; every other letter is a
/// field report and is untouched by this module.
pub const KIND_MISSION: &str = "mission";

// ===========================================================================
// The §1 enums — roles, capabilities, phases, verdicts. `deny_unknown_fields`
// is applied to structs; the enums are closed by construction.
// ===========================================================================

/// The seat naming a role, never a vendor (§1a). Voice/model/vendor is the
/// operator's private runner configuration and NEVER appears in the letter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Seat {
    Oracle,
    Hand,
}

/// The five MVP capabilities (§1). Role names, kebab-cased on the wire
/// (`build-runner`, `naming-runner`, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Capability {
    BuildRunner,
    NamingRunner,
    LoopRunner,
    HandRunner,
    ReviewRunner,
}

/// The seven-state phase enum (§1), verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Judging,
    Executing,
    Gate,
    Review,
    MergeWait,
    Landed,
    Failed,
}

impl Phase {
    /// The lowercase wire string (for honest error detail).
    fn as_str(self) -> &'static str {
        match self {
            Phase::Judging => "judging",
            Phase::Executing => "executing",
            Phase::Gate => "gate",
            Phase::Review => "review",
            Phase::MergeWait => "merge_wait",
            Phase::Landed => "landed",
            Phase::Failed => "failed",
        }
    }
}

/// A verdict's decision (§1). Uppercase on the wire (`APPROVE|CHANGE|REJECT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum VerdictDecision {
    Approve,
    Change,
    Reject,
}

/// The oracle's verdict (§1) — a decision plus a one-line gist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verdict {
    pub decision: VerdictDecision,
    pub gist: String,
}

/// The gate evidence line (§1) — the verbatim command, its exit status, and the
/// hash of the full log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateEvidence {
    pub command: String,
    pub exit_status: i32,
    pub artifact_hash: String,
}

/// A candidate receipt's scope (§1). Leaner than [`crate::system_blocks::ReceiptScope`]:
/// the block_id and resolution_hash are bound at import time (F2.5d), so a
/// candidate carries only the two version anchors the emitter can see now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateScope {
    pub boundary_version: u32,
    pub contract_version: u32,
}

/// A complete receipt candidate (§1, §5c) — everything a human/agent needs to run
/// `receipt_import` from the tray with one click. Runnerd holds NO import
/// permission in the MVP; the candidate is a PROPOSAL, never an applied receipt.
/// Its `type` and `evidence` reuse the SystemBlock receipt taxonomy so the import
/// is a direct hand-off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptCandidate {
    pub block_id: String,
    #[serde(rename = "type")]
    pub type_: ReceiptType,
    pub scope: CandidateScope,
    pub evidence: ReceiptEvidence,
}

/// The receipt anchor a `landed` letter carries (§1, §1d) — the confirmation that
/// `receipt_import` succeeded and the `store_version` it produced. `imported`
/// must be `true` and `store_version` real for a letter to validate as `landed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptAnchor {
    pub imported: bool,
    pub store_version: u64,
}

/// The mission letter — the one shape every seat speaks (§1). `deny_unknown_fields`
/// as the house does; per-phase gating is semantic (see [`validate`]), not
/// schema-shaped. NO absolute-path field exists here (§1f).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionLetter {
    pub schema: String,
    pub mission_id: String,
    pub mission_seq: u64,
    /// The content id of the previous letter in this mission's chain (§1e), or
    /// `None`/absent for `mission_seq == 1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_letter_id: Option<String>,
    pub block_id: String,
    /// The brain's registered display name / repo_id — NEVER an absolute path (§1f).
    pub brain_ref: String,
    pub seat: Seat,
    /// The owner-side pinned runner id, or `None` for non-runner emitters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_id: Option<String>,
    pub capability: Capability,
    pub phase: Phase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<Verdict>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<GateEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_candidate: Option<ReceiptCandidate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<ReceiptAnchor>,
    /// The hash of the MissionPacket markdown that opened the mission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packet_ref: Option<String>,
    #[serde(default)]
    pub tokens_total: u64,
    pub started_at: String,
    pub updated_at: String,
    /// `true` marks a legitimately-synthetic letter (a smoke test / warm-pool
    /// probe) whose `block_id` is NOT expected to exist in the store — the
    /// mission_post block guard skips validation for it. Default `false`: a real
    /// letter must name a real block (the field-hardening the hand agent proposed
    /// after mission_post silently accepted a non-existent block).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub synthetic: bool,
}

// ===========================================================================
// Typed errors — every honest refusal carries its keyword in Display so the
// handler surfaces `stale_head` / `invalid_phase` verbatim (§2c).
// ===========================================================================

/// The honest refusals of the mission-letter contract.
#[derive(Debug)]
pub enum MissionLetterError {
    /// The `schema` tag is not `m1nd-mission-letter-v0`.
    SchemaMismatch { found: String },
    /// `mission_id` is not `msn_<12hex>`.
    InvalidMissionId { got: String },
    /// `mission_seq` is below 1 (a chain starts at 1).
    InvalidSeq { got: u64 },
    /// A public-contract field carried an absolute path (§1f) — brain_ref must be
    /// a reference string, never a path.
    AbsolutePathInContract { field: String, value: String },
    /// Per-phase field gating failed (§1b): the wrong fields for the phase.
    InvalidPhase { phase: String, detail: String },
    /// The §1d landed law: `landed` demands `receipt.imported == true` with a real
    /// `store_version` (a zero-exit gate without an imported receipt is
    /// `merge_wait`, never `landed`).
    LandedWithoutReceipt { detail: String },
    /// A `receipt_candidate` (or `gate`) evidence anchor was empty (§5c/§3 anti-poison).
    IncompleteEvidence { part: String, missing: String },
    /// The §1e head CAS: the letter does not extend the current head. Nothing is
    /// appended (append-only log preserved; only the head pointer is guarded).
    StaleHead {
        expected_head: Option<String>,
        got_prev: Option<String>,
        detail: String,
    },
    /// Reading or appending the mailbox box failed (I/O or a corrupt line).
    Storage { detail: String },
}

impl std::fmt::Display for MissionLetterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MissionLetterError::SchemaMismatch { found } => write!(
                f,
                "schema mismatch: expected {MISSION_LETTER_SCHEMA}, found {found}"
            ),
            MissionLetterError::InvalidMissionId { got } => {
                write!(f, "invalid mission_id '{got}': expected msn_<12hex>")
            }
            MissionLetterError::InvalidSeq { got } => {
                write!(f, "invalid mission_seq {got}: a chain starts at 1")
            }
            MissionLetterError::AbsolutePathInContract { field, value } => write!(
                f,
                "the public contract carries no absolute paths (§1f): field `{field}` = '{value}' looks like a path — brain_ref is a reference string, host paths live in the local side record"
            ),
            MissionLetterError::InvalidPhase { phase, detail } => {
                write!(f, "invalid_phase: `{phase}` {detail}")
            }
            MissionLetterError::LandedWithoutReceipt { detail } => write!(
                f,
                "landed_law: `landed` requires receipt.imported==true with a real store_version — {detail} (a zero-exit gate without an imported receipt is merge_wait, never landed)"
            ),
            MissionLetterError::IncompleteEvidence { part, missing } => write!(
                f,
                "incomplete_evidence: {part} is missing `{missing}` — evidence a tool cannot point at is not evidence"
            ),
            MissionLetterError::StaleHead {
                expected_head,
                got_prev,
                detail,
            } => write!(
                f,
                "stale_head: {detail} (expected_head={}, got prev_letter_id={})",
                expected_head.as_deref().unwrap_or("<none>"),
                got_prev.as_deref().unwrap_or("<none>")
            ),
            MissionLetterError::Storage { detail } => {
                write!(f, "mission mailbox I/O error: {detail}")
            }
        }
    }
}

impl std::error::Error for MissionLetterError {}

type Result<T> = std::result::Result<T, MissionLetterError>;

// ===========================================================================
// Validation — the §1 schema + phase gating + the landed law.
// ===========================================================================

/// Whether a string looks like an absolute host path (§1f guard). A leading `/`,
/// a leading `~`, or a Windows drive prefix (`C:\` / `C:/`) is a path; a bare
/// name or an `org/repo` reference is not.
fn looks_absolute(s: &str) -> bool {
    let t = s.trim();
    if t.starts_with('/') || t.starts_with("~") || t.starts_with("\\\\") {
        return true;
    }
    let bytes = t.as_bytes();
    // Windows drive: `X:\` or `X:/`.
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

/// `mission_id` must be `msn_` followed by exactly 12 lowercase-hex characters.
fn valid_mission_id(id: &str) -> bool {
    match id.strip_prefix("msn_") {
        Some(hex) => hex.len() == 12 && hex.chars().all(|c| c.is_ascii_hexdigit()),
        None => false,
    }
}

/// Validate the evidence anchor reused from the receipt taxonomy (§3 anti-poison):
/// `artifact_hash` and `evidence_refs` are present and non-empty. Mirrors the
/// SystemBlock store's `validate_receipt_evidence` universal-anchor rule without
/// coupling to its `Receipt`-shaped signature.
fn validate_evidence_anchor(part: &str, ev: &ReceiptEvidence) -> Result<()> {
    if ev.artifact_hash.trim().is_empty() {
        return Err(MissionLetterError::IncompleteEvidence {
            part: part.to_string(),
            missing: "artifact_hash".to_string(),
        });
    }
    if ev.evidence_refs.is_empty() {
        return Err(MissionLetterError::IncompleteEvidence {
            part: part.to_string(),
            missing: "evidence_refs".to_string(),
        });
    }
    Ok(())
}

/// Validate a mission letter against the §1 contract: schema tag, id shape, seq
/// floor, the §1f absolute-path guard, per-phase field gating (§1b), the §1d
/// landed law, and (when present) the gate/candidate evidence anchors. Pure — no
/// I/O, no store access.
pub fn validate(letter: &MissionLetter) -> Result<()> {
    if letter.schema != MISSION_LETTER_SCHEMA {
        return Err(MissionLetterError::SchemaMismatch {
            found: letter.schema.clone(),
        });
    }
    if !valid_mission_id(&letter.mission_id) {
        return Err(MissionLetterError::InvalidMissionId {
            got: letter.mission_id.clone(),
        });
    }
    if letter.mission_seq < 1 {
        return Err(MissionLetterError::InvalidSeq {
            got: letter.mission_seq,
        });
    }
    // §1f: the one field that used to be a path (`brain_root` → `brain_ref`) must
    // never carry one.
    if looks_absolute(&letter.brain_ref) {
        return Err(MissionLetterError::AbsolutePathInContract {
            field: "brain_ref".to_string(),
            value: letter.brain_ref.clone(),
        });
    }

    // Per-phase gating (§1b) + the landed law (§1d).
    match letter.phase {
        // `executing` carries no verdict (the verdict belongs to the judging seat);
        // an `executing` letter WITHOUT a verdict is fine (falls to `_`).
        Phase::Executing if letter.verdict.is_some() => {
            return Err(MissionLetterError::InvalidPhase {
                phase: "executing".to_string(),
                detail: "carries no verdict (the verdict belongs to the judging seat)".to_string(),
            });
        }
        Phase::MergeWait => {
            let gate = letter
                .gate
                .as_ref()
                .ok_or(MissionLetterError::InvalidPhase {
                    phase: "merge_wait".to_string(),
                    detail: "requires a gate".to_string(),
                })?;
            if gate.command.trim().is_empty() || gate.artifact_hash.trim().is_empty() {
                return Err(MissionLetterError::IncompleteEvidence {
                    part: "gate".to_string(),
                    missing: if gate.command.trim().is_empty() {
                        "command".to_string()
                    } else {
                        "artifact_hash".to_string()
                    },
                });
            }
        }
        Phase::Landed => match &letter.receipt {
            Some(r) if r.imported && r.store_version >= 1 => {}
            Some(r) if !r.imported => {
                return Err(MissionLetterError::LandedWithoutReceipt {
                    detail: "receipt.imported is false".to_string(),
                })
            }
            Some(_) => {
                return Err(MissionLetterError::LandedWithoutReceipt {
                    detail: "receipt.store_version is not a real (>=1) version".to_string(),
                })
            }
            None => {
                return Err(MissionLetterError::LandedWithoutReceipt {
                    detail: "no receipt anchor is present".to_string(),
                })
            }
        },
        _ => {}
    }

    // A gate line present in ANY phase must carry its anchor (evidence, not a claim).
    if let Some(gate) = &letter.gate {
        if gate.artifact_hash.trim().is_empty() {
            return Err(MissionLetterError::IncompleteEvidence {
                part: "gate".to_string(),
                missing: "artifact_hash".to_string(),
            });
        }
    }
    // A receipt candidate, when present, must be complete (§5c) — an incomplete
    // candidate is a lie the tray would offer for one-click import.
    if let Some(candidate) = &letter.receipt_candidate {
        validate_evidence_anchor("receipt_candidate.evidence", &candidate.evidence)?;
    }

    Ok(())
}

// ===========================================================================
// The head CAS (§1e) — the letters of one mission form a hash chain.
// ===========================================================================

/// A reference to a mission chain's current head — just what the CAS needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadRef {
    /// The head letter's content id (`sha256[0..12]` of its mailbox line).
    pub letter_id: String,
    /// The head letter's `mission_seq`.
    pub mission_seq: u64,
}

/// The §1e compare-and-swap: a candidate letter may only be appended if it
/// extends the current head. `current_head == None` (a fresh mission) accepts
/// exactly `mission_seq == 1` with `prev_letter_id == None`; a present head
/// accepts exactly `mission_seq == head.seq + 1` with `prev_letter_id ==
/// head.letter_id`. Any other case is a `stale_head` — nothing is appended. Pure.
pub fn validate_against_head(letter: &MissionLetter, current_head: Option<&HeadRef>) -> Result<()> {
    match current_head {
        None => {
            if letter.mission_seq != 1 {
                return Err(MissionLetterError::StaleHead {
                    expected_head: None,
                    got_prev: letter.prev_letter_id.clone(),
                    detail: format!(
                        "no chain exists yet for this mission — the first letter must be seq 1, got seq {}",
                        letter.mission_seq
                    ),
                });
            }
            if letter.prev_letter_id.is_some() {
                return Err(MissionLetterError::StaleHead {
                    expected_head: None,
                    got_prev: letter.prev_letter_id.clone(),
                    detail: "seq 1 must carry a null prev_letter_id".to_string(),
                });
            }
            Ok(())
        }
        Some(head) => {
            let want_seq = head.mission_seq + 1;
            let prev_ok = letter.prev_letter_id.as_deref() == Some(head.letter_id.as_str());
            if letter.mission_seq != want_seq || !prev_ok {
                return Err(MissionLetterError::StaleHead {
                    expected_head: Some(head.letter_id.clone()),
                    got_prev: letter.prev_letter_id.clone(),
                    detail: format!(
                        "the head is seq {} ({}); the next letter must be seq {want_seq} with prev_letter_id={}",
                        head.mission_seq, head.letter_id, head.letter_id
                    ),
                });
            }
            Ok(())
        }
    }
}

// ===========================================================================
// Head computation — reading a mission's chain out of the mailbox box.
// ===========================================================================

/// One entry of a mission's chain: a stored letter's content id + its payload.
struct ChainEntry {
    letter_id: String,
    mission: MissionLetter,
}

/// The head of one mission's chain, for the tray read (§2b).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MissionHead {
    pub mission_id: String,
    /// The head letter's content id.
    pub head_letter_id: String,
    /// The head mission letter (the current state).
    pub head: MissionLetter,
    /// How many letters for this mission are NOT the head (§2b honest count).
    pub superseded_count: usize,
}

/// Collect the mission-letter chain entries from a box's parsed letters,
/// optionally filtered to one `mission_id`.
fn collect_chain(letters: &[Letter], only_mission: Option<&str>) -> Vec<ChainEntry> {
    letters
        .iter()
        .filter_map(|l| {
            if l.kind.as_deref() != Some(KIND_MISSION) {
                return None;
            }
            let mission = l.mission.as_ref()?;
            if let Some(mid) = only_mission {
                if mission.mission_id != mid {
                    return None;
                }
            }
            Some(ChainEntry {
                letter_id: l.id.clone(),
                mission: mission.clone(),
            })
        })
        .collect()
}

/// Walk a mission's chain from its `seq 1` root and return the tip. Follows
/// `prev_letter_id` links; capped at `entries.len()` hops so a hand-corrupted box
/// can never loop forever. A chain with no clean root (never produced by the CAS)
/// falls back to the highest-`mission_seq` entry, so a read never crashes.
fn walk_head(entries: &[ChainEntry]) -> Option<&ChainEntry> {
    if entries.is_empty() {
        return None;
    }
    let root = entries
        .iter()
        .find(|e| e.mission.prev_letter_id.is_none())
        .or_else(|| entries.iter().max_by_key(|e| e.mission.mission_seq))?;
    let mut cur = root;
    for _ in 0..entries.len() {
        match entries
            .iter()
            .find(|e| e.mission.prev_letter_id.as_deref() == Some(cur.letter_id.as_str()))
        {
            Some(next) => cur = next,
            None => break,
        }
    }
    Some(cur)
}

/// The current head ref of one mission's chain in a box (for the CAS pre-read).
pub fn head_ref_for(letters: &[Letter], mission_id: &str) -> Option<HeadRef> {
    let entries = collect_chain(letters, Some(mission_id));
    walk_head(&entries).map(|e| HeadRef {
        letter_id: e.letter_id.clone(),
        mission_seq: e.mission.mission_seq,
    })
}

/// Compute the head + honest superseded count for EVERY mission in a box's
/// letters (§2b). Keyed by `mission_id` (BTreeMap → deterministic order).
pub fn heads_by_mission(letters: &[Letter]) -> BTreeMap<String, MissionHead> {
    let all = collect_chain(letters, None);
    let mut by_mission: BTreeMap<String, Vec<ChainEntry>> = BTreeMap::new();
    for e in all {
        by_mission
            .entry(e.mission.mission_id.clone())
            .or_default()
            .push(e);
    }
    let mut out = BTreeMap::new();
    for (mission_id, entries) in by_mission {
        if let Some(head) = walk_head(&entries) {
            out.insert(
                mission_id.clone(),
                MissionHead {
                    mission_id,
                    head_letter_id: head.letter_id.clone(),
                    head: head.mission.clone(),
                    superseded_count: entries.len().saturating_sub(1),
                },
            );
        }
    }
    out
}

// ===========================================================================
// The mailbox envelope + the post path (§2a/§2c) — reuse the mailbox transport.
// ===========================================================================

/// The mailbox line a mission letter serializes to (§2a). A dedicated struct with
/// a fixed field order so the content id is deterministic regardless of any
/// serde_json key-ordering feature; it round-trips through [`crate::mailbox::parse_letter`]
/// into a `Letter` with `kind = Some("mission")` and the mission payload (legacy
/// fields default to empty — a mission letter is not a field report).
#[derive(Serialize)]
struct MissionEnvelope<'a> {
    kind: &'a str,
    agent: &'a str,
    ts: &'a str,
    mission: &'a MissionLetter,
}

/// Build the canonical mailbox JSONL line for a mission letter. Deterministic:
/// the same `(agent_id, letter)` yields byte-identical bytes, so replay dedups by
/// content id (§2c idempotence).
fn envelope_line(agent_id: &str, letter: &MissionLetter) -> Result<String> {
    let env = MissionEnvelope {
        kind: KIND_MISSION,
        agent: agent_id,
        ts: &letter.updated_at,
        mission: letter,
    };
    serde_json::to_string(&env).map_err(|e| MissionLetterError::Storage {
        detail: e.to_string(),
    })
}

/// The outcome of a `mission_post` (§2c).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PostOutcome {
    /// The appended (or deduped) letter's content id — the value the emitter sets
    /// as the NEXT letter's `prev_letter_id`.
    pub letter_id: String,
    pub mission_id: String,
    pub mission_seq: u64,
    /// True when the identical letter was already present (idempotent replay, no
    /// append); false when it was newly appended.
    pub deduped: bool,
}

/// Post a mission letter into a mailbox box (§2c): validate the §1 contract, then
/// — inside the same read of the box — dedup by content id (idempotent replay),
/// compute the mission's current head, run the §1e head CAS, and only then append.
/// A stale head returns `stale_head` and NOTHING is appended (the log is
/// untouched). This is the whole verb's engine; the MCP handler is a thin wrapper
/// that resolves `box_path` from the session and maps the error onto the MCP
/// surface. It NEVER opens the SystemBlockStore (§1c — a letter is state, not
/// evidence).
pub fn post_mission_letter(
    box_path: &std::path::Path,
    agent_id: &str,
    letter: &MissionLetter,
) -> Result<PostOutcome> {
    validate(letter)?;

    let line = envelope_line(agent_id, letter)?;
    let id = mailbox::letter_id(&line);

    let existing = mailbox::read_letters(box_path).map_err(|e| MissionLetterError::Storage {
        detail: e.to_string(),
    })?;

    // Idempotent replay: an identical letter (same content id) is already filed —
    // return it, append nothing (§2c content-hash dedup).
    if existing.iter().any(|l| l.id == id) {
        return Ok(PostOutcome {
            letter_id: id,
            mission_id: letter.mission_id.clone(),
            mission_seq: letter.mission_seq,
            deduped: true,
        });
    }

    // The §1e head CAS against THIS mission's current head.
    let head = head_ref_for(&existing, &letter.mission_id);
    validate_against_head(letter, head.as_ref())?;

    // Extend the log (append-only; dedup guards a concurrent identical write).
    let (appended_id, appended) =
        mailbox::append_raw_line_deduped(box_path, &line).map_err(|e| {
            MissionLetterError::Storage {
                detail: e.to_string(),
            }
        })?;
    debug_assert_eq!(appended_id, id, "content id is deterministic");

    Ok(PostOutcome {
        letter_id: appended_id,
        mission_id: letter.mission_id.clone(),
        mission_seq: letter.mission_seq,
        deduped: !appended,
    })
}

// ===========================================================================
// Battery — the anti-lie core (§6-F2.5a): gate-zero-cannot-land, stale-head-
// rejected, letter-cannot-color, the happy chain, candidate completeness, and
// the §1f no-absolute-paths proof. NEUTRAL fixtures only.
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch dir cleaned on drop.
    struct Scratch {
        dir: std::path::PathBuf,
    }
    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "m1nd-mission-letter-test-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            std::fs::create_dir_all(&dir).expect("mk scratch");
            Self { dir }
        }
        fn path(&self, rel: &str) -> std::path::PathBuf {
            self.dir.join(rel)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn base_letter(seq: u64, phase: Phase) -> MissionLetter {
        MissionLetter {
            schema: MISSION_LETTER_SCHEMA.to_string(),
            mission_id: "msn_0123456789ab".to_string(),
            mission_seq: seq,
            prev_letter_id: None,
            block_id: "sb_alpha".to_string(),
            brain_ref: "repo-a".to_string(),
            seat: Seat::Hand,
            runner_id: None,
            capability: Capability::BuildRunner,
            phase,
            verdict: None,
            gate: None,
            receipt_candidate: None,
            receipt: None,
            packet_ref: Some("sha256:packethash".to_string()),
            tokens_total: 0,
            started_at: "2026-07-09T00:00:00Z".to_string(),
            updated_at: "2026-07-09T00:00:00Z".to_string(),
            synthetic: false,
        }
    }

    fn complete_candidate() -> ReceiptCandidate {
        ReceiptCandidate {
            block_id: "sb_alpha".to_string(),
            type_: ReceiptType::Test,
            scope: CandidateScope {
                boundary_version: 1,
                contract_version: 1,
            },
            evidence: ReceiptEvidence {
                command: Some("cargo test".to_string()),
                cwd: None,
                exit_status: Some(0),
                started_at: None,
                ended_at: None,
                artifact_hash: "sha256:loghash".to_string(),
                stdout_excerpt: None,
                evidence_refs: vec!["artifact://run/1".to_string()],
            },
        }
    }

    fn green_gate() -> GateEvidence {
        GateEvidence {
            command: "cargo test -p m1nd-mcp".to_string(),
            exit_status: 0,
            artifact_hash: "sha256:gatelog".to_string(),
        }
    }

    // --- §1 schema/id/seq validation ---------------------------------------

    #[test]
    fn schema_id_and_seq_are_validated() {
        let mut l = base_letter(1, Phase::Judging);
        assert!(validate(&l).is_ok());

        l.schema = "wrong".to_string();
        assert!(matches!(
            validate(&l),
            Err(MissionLetterError::SchemaMismatch { .. })
        ));

        let mut l = base_letter(1, Phase::Judging);
        l.mission_id = "msn_nothex".to_string();
        assert!(matches!(
            validate(&l),
            Err(MissionLetterError::InvalidMissionId { .. })
        ));

        let mut l = base_letter(0, Phase::Judging);
        assert!(matches!(
            validate(&l),
            Err(MissionLetterError::InvalidSeq { .. })
        ));
    }

    // --- the §1d LANDED LAW: gate-zero cannot land -------------------------

    #[test]
    fn gate_zero_cannot_land() {
        // A merge_wait letter carrying a GREEN gate (exit 0) but NO imported
        // receipt is VALID — that is exactly "gate green, receipt not landed".
        let mut merge = base_letter(3, Phase::MergeWait);
        merge.gate = Some(green_gate());
        merge.receipt_candidate = Some(complete_candidate());
        assert!(
            validate(&merge).is_ok(),
            "a green gate without a receipt is a valid merge_wait"
        );

        // The SAME mission trying to declare `landed` with only that green gate and
        // NO imported receipt is REJECTED by the landed law (§1d).
        let mut landed = base_letter(4, Phase::Landed);
        landed.gate = Some(green_gate());
        landed.receipt = None; // gate green, but receipt NOT imported
        let err = validate(&landed).expect_err("gate-zero must not validate as landed");
        assert!(
            matches!(err, MissionLetterError::LandedWithoutReceipt { .. }),
            "expected the landed-law error, got {err}"
        );

        // receipt present but not imported → still rejected.
        let mut landed2 = base_letter(4, Phase::Landed);
        landed2.receipt = Some(ReceiptAnchor {
            imported: false,
            store_version: 0,
        });
        assert!(matches!(
            validate(&landed2),
            Err(MissionLetterError::LandedWithoutReceipt { .. })
        ));

        // imported with a real store_version → the ONLY thing that lands.
        let mut landed_ok = base_letter(4, Phase::Landed);
        landed_ok.receipt = Some(ReceiptAnchor {
            imported: true,
            store_version: 9,
        });
        assert!(validate(&landed_ok).is_ok(), "an imported receipt lands");
    }

    // --- per-phase gating: executing has no verdict; merge_wait needs a gate -

    #[test]
    fn executing_carries_no_verdict_and_merge_wait_requires_gate() {
        let mut exec = base_letter(2, Phase::Executing);
        exec.verdict = Some(Verdict {
            decision: VerdictDecision::Approve,
            gist: "should not be here".to_string(),
        });
        assert!(matches!(
            validate(&exec),
            Err(MissionLetterError::InvalidPhase { .. })
        ));

        let merge_no_gate = base_letter(3, Phase::MergeWait);
        assert!(matches!(
            validate(&merge_no_gate),
            Err(MissionLetterError::InvalidPhase { .. })
        ));
    }

    // --- receipt candidate completeness (§5c) ------------------------------

    #[test]
    fn receipt_candidate_complete_validates_incomplete_rejected() {
        let mut ok = base_letter(3, Phase::MergeWait);
        ok.gate = Some(green_gate());
        ok.receipt_candidate = Some(complete_candidate());
        assert!(validate(&ok).is_ok());

        // Missing artifact_hash → rejected with the clear typed error.
        let mut bad = base_letter(3, Phase::MergeWait);
        bad.gate = Some(green_gate());
        let mut cand = complete_candidate();
        cand.evidence.artifact_hash = "".to_string();
        bad.receipt_candidate = Some(cand);
        let err = validate(&bad).expect_err("an empty artifact_hash is not evidence");
        assert!(
            matches!(err, MissionLetterError::IncompleteEvidence { ref missing, .. } if missing == "artifact_hash"),
            "expected incomplete_evidence(artifact_hash), got {err}"
        );

        // Missing evidence_refs → rejected too.
        let mut bad2 = base_letter(3, Phase::MergeWait);
        bad2.gate = Some(green_gate());
        let mut cand2 = complete_candidate();
        cand2.evidence.evidence_refs = vec![];
        bad2.receipt_candidate = Some(cand2);
        assert!(matches!(
            validate(&bad2),
            Err(MissionLetterError::IncompleteEvidence { .. })
        ));
    }

    // --- §1f: brain_ref may not be an absolute path ------------------------

    #[test]
    fn brain_ref_absolute_path_is_rejected() {
        for path in ["/Users/someone/repo", "~/repo", "C:\\repo"] {
            let mut l = base_letter(1, Phase::Judging);
            l.brain_ref = path.to_string();
            assert!(
                matches!(
                    validate(&l),
                    Err(MissionLetterError::AbsolutePathInContract { .. })
                ),
                "brain_ref '{path}' must be refused (§1f)"
            );
        }
        // A display name / repo_id reference is fine.
        let mut ok = base_letter(1, Phase::Judging);
        ok.brain_ref = "org/repo-a".to_string();
        assert!(validate(&ok).is_ok());
    }

    // --- §1f: no absolute path ever appears in a serialized MissionLetter ---

    #[test]
    fn no_absolute_path_in_any_mission_letter_serialization() {
        // A fully-populated letter (every optional field set) serialized to JSON
        // must contain NO absolute host path marker.
        let mut l = base_letter(4, Phase::Landed);
        l.prev_letter_id = Some("abcdef012345".to_string());
        l.runner_id = Some("runner-build-1".to_string());
        l.gate = Some(green_gate());
        l.receipt_candidate = Some(complete_candidate());
        l.receipt = Some(ReceiptAnchor {
            imported: true,
            store_version: 9,
        });
        let json = serde_json::to_string(&l).unwrap();
        assert!(!json.contains("/Users"), "no macOS home path: {json}");
        assert!(!json.contains("/home/"), "no linux home path: {json}");
        // And the envelope line the mailbox stores is equally clean.
        let line = envelope_line("agent-x", &l).unwrap();
        assert!(!line.contains("/Users") && !line.contains("/home/"));
    }

    // --- §1e head CAS (pure) -----------------------------------------------

    #[test]
    fn head_cas_seq1_none_seqn_extends_and_stale_rejected() {
        // seq 1 with no head + null prev → ok.
        let l1 = base_letter(1, Phase::Judging);
        assert!(validate_against_head(&l1, None).is_ok());

        // seq 1 with a non-null prev → stale.
        let mut l1_bad = base_letter(1, Phase::Judging);
        l1_bad.prev_letter_id = Some("deadbeef0001".to_string());
        assert!(matches!(
            validate_against_head(&l1_bad, None),
            Err(MissionLetterError::StaleHead { .. })
        ));

        // seq 2 extending head(seq1, id X) → ok.
        let head = HeadRef {
            letter_id: "aaaaaaaaaaaa".to_string(),
            mission_seq: 1,
        };
        let mut l2 = base_letter(2, Phase::Executing);
        l2.prev_letter_id = Some("aaaaaaaaaaaa".to_string());
        assert!(validate_against_head(&l2, Some(&head)).is_ok());

        // seq 2 with the WRONG prev → stale_head.
        let mut l2_bad = base_letter(2, Phase::Executing);
        l2_bad.prev_letter_id = Some("wrongprevabc".to_string());
        let err = validate_against_head(&l2_bad, Some(&head)).expect_err("wrong prev is stale");
        assert!(matches!(err, MissionLetterError::StaleHead { .. }));
        assert!(err.to_string().contains("stale_head"));
    }

    // --- the happy chain through the box: seq1→seq2→seq3, head=seq3, super=2 -

    #[test]
    fn happy_chain_head_is_seq3_with_two_superseded() {
        let s = Scratch::new("chain");
        let box_path = s.path("inbox.jsonl");

        // seq 1 — judging.
        let l1 = base_letter(1, Phase::Judging);
        let out1 = post_mission_letter(&box_path, "agent-a", &l1).unwrap();
        assert!(!out1.deduped);

        // seq 2 — executing, chained on seq 1's id.
        let mut l2 = base_letter(2, Phase::Executing);
        l2.prev_letter_id = Some(out1.letter_id.clone());
        let out2 = post_mission_letter(&box_path, "agent-a", &l2).unwrap();

        // seq 3 — merge_wait with a gate + a complete candidate.
        let mut l3 = base_letter(3, Phase::MergeWait);
        l3.prev_letter_id = Some(out2.letter_id.clone());
        l3.gate = Some(green_gate());
        l3.receipt_candidate = Some(complete_candidate());
        let out3 = post_mission_letter(&box_path, "agent-a", &l3).unwrap();

        // The read head is seq 3 with superseded_count 2.
        let letters = mailbox::read_letters(&box_path).unwrap();
        let heads = heads_by_mission(&letters);
        let head = heads.get("msn_0123456789ab").expect("mission present");
        assert_eq!(head.head.mission_seq, 3, "head is the seq-3 tip");
        assert_eq!(head.head.phase, Phase::MergeWait);
        assert_eq!(head.head_letter_id, out3.letter_id);
        assert_eq!(head.superseded_count, 2, "seq 1 and seq 2 are superseded");
    }

    // --- stale_head at post time: nothing is appended ----------------------

    #[test]
    fn stale_head_rejected_and_nothing_appended() {
        let s = Scratch::new("stale");
        let box_path = s.path("inbox.jsonl");

        let l1 = base_letter(1, Phase::Judging);
        post_mission_letter(&box_path, "agent-a", &l1).unwrap();
        let after_seq1 = std::fs::read_to_string(&box_path).unwrap();

        // A seq-2 letter with the WRONG prev (not the head) → stale_head.
        let mut l2_bad = base_letter(2, Phase::Executing);
        l2_bad.prev_letter_id = Some("deadbeefdead".to_string());
        let err = post_mission_letter(&box_path, "agent-a", &l2_bad)
            .expect_err("a wrong prev must be rejected");
        assert!(
            matches!(err, MissionLetterError::StaleHead { .. }),
            "expected stale_head, got {err}"
        );
        assert!(err.to_string().contains("stale_head"));

        // The mailbox is INTACT — the rejected letter was never appended.
        let after_reject = std::fs::read_to_string(&box_path).unwrap();
        assert_eq!(
            after_seq1, after_reject,
            "a stale-head rejection appends nothing"
        );
        assert_eq!(
            mailbox::read_letters(&box_path).unwrap().len(),
            1,
            "still exactly the seq-1 letter"
        );
    }

    // --- idempotent replay: the identical letter dedups ---------------------

    #[test]
    fn identical_replay_is_idempotent() {
        let s = Scratch::new("dedup");
        let box_path = s.path("inbox.jsonl");
        let l1 = base_letter(1, Phase::Judging);

        let first = post_mission_letter(&box_path, "agent-a", &l1).unwrap();
        assert!(!first.deduped);
        let second = post_mission_letter(&box_path, "agent-a", &l1).unwrap();
        assert!(second.deduped, "an identical replay is a dedup no-op");
        assert_eq!(first.letter_id, second.letter_id);
        assert_eq!(
            mailbox::read_letters(&box_path).unwrap().len(),
            1,
            "the replay appended nothing"
        );
    }

    // --- §1c: a mission letter NEVER colors the SystemBlockStore -------------

    #[test]
    fn letter_cannot_color_the_store() {
        use crate::system_blocks::{import_seed_into_dir, SystemBlockStore};
        let s = Scratch::new("nocolor");
        let dir = s.path("brain");
        std::fs::create_dir_all(&dir).unwrap();

        // A real store at store_version 1.
        let seed = include_str!("../../docs/system-blocks/m1nd.seed.v0.json");
        import_seed_into_dir(&dir, seed, false).expect("seed import");
        let before = SystemBlockStore::load(&dir).unwrap().unwrap().store_version;
        assert_eq!(before, 1);

        // Import a VALID landed letter (imported receipt, real store_version) into
        // the box that lives in the SAME dir.
        let box_path = dir.join("inbox.jsonl");
        let mut landed = base_letter(1, Phase::Landed);
        landed.receipt = Some(ReceiptAnchor {
            imported: true,
            store_version: 9, // the letter NAMES 9, but it does not MAKE the store 9
        });
        post_mission_letter(&box_path, "agent-a", &landed).expect("post landed");

        // The store is UNTOUCHED — a letter is state, not evidence (§1c).
        let after = SystemBlockStore::load(&dir).unwrap().unwrap().store_version;
        assert_eq!(
            before, after,
            "posting a letter must not bump the store OCC counter"
        );
        assert!(
            box_path.exists(),
            "the letter landed in the box, not the store"
        );
    }
}
