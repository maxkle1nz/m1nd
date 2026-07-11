//! Human View v2 F12 — the curation-runner engine (HUMAN-VIEW-V2-F12-TECH §2/§3).
//!
//! The propose-apply lane, generalized from the one-name naming lane to a whole
//! curation. The OWNER composes the block VIEW the hand needs (ids, names, purposes,
//! members, confidence, seams, unmapped), sends it to an announced runner daemon's
//! `POST /curate`, and the pinned hand-runner PROPOSES a batch of `candidate_edit`
//! ops AS DATA. On receiving the proposal the OWNER, in one motion, validates the
//! schema → sanitizes every rename/purpose (o5, seat `runner`, via `candidate_edit`)
//! → acquires the advisory lease as the curating hand → applies the WHOLE batch
//! through the existing engine under the caller's OCC key → releases the lease →
//! posts the summary letter into the mission chain. The agent never holds a write
//! surface — not REST, not MCP, not a file (§1). This module is the OWNER-side
//! engine, deliberately transport-thin and content-paranoid, mirroring
//! [`crate::naming_runner`]:
//!
//! - **The packet is a VIEW, never file bodies** ([`CurationPacket`]) — the same
//!   member/kind/symbol data the naming packet carries, plus each block's confidence
//!   components and seam members, plus the unmapped residue sample.
//! - **The proposal is HOSTILE input.** The daemon shape-validates it; the owner
//!   RE-parses the ops into typed [`EditOp`] and applies them with the RUNNER seat,
//!   so the o5 sanitizer (`candidate_edit::apply_rename`) governs every rename — a
//!   hostile name/purpose kills the WHOLE batch (o1 preflight: nothing persists).
//! - **All-or-nothing, never partial.** A malformed proposal, an o5 violation, or an
//!   OCC conflict applies NOTHING (the batch preflights on a clone).
//!
//! The daemon side (the `/curate` endpoint, the per-mission timeout, the strong ops
//! parse) lives in `m1nd-runnerd::curation`.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use m1nd_core::error::M1ndError;

use crate::candidate_edit::{EditOp, EditSeat};
use crate::mission_letter::{
    self, Capability, MissionLetter, Phase, Seat, Verdict, VerdictDecision, MISSION_LETTER_SCHEMA,
};
use crate::skeleton_scan::SkeletonGraphNode;
use crate::system_blocks::{
    candidate_edit_in_dir, candidate_lease_in_dir, LeaseAction, SeedError, SystemBlock,
    SystemBlockStore, DEFAULT_LEASE_TTL_SECS,
};

/// The proposal schema tag the hand-runner stamps (§2) — shared with the daemon's
/// `m1nd-runnerd::curation::CURATION_PROPOSAL_SCHEMA`.
pub const CURATION_PROPOSAL_SCHEMA: &str = "m1nd-curation-proposal-v0";

/// The report gist a summary letter carries is capped so a hand cannot post an
/// unbounded blob into the mission chain — a paragraph fits; a dump truncates. The
/// FULL report always rides the tool RESULT (the screen shows it uncapped).
pub const REPORT_GIST_CAP: usize = 2000;

/// The instruction serialized INTO every curation packet (§2) — the 3-sentence
/// proposal contract, the mold of [`crate::naming_runner::PACKET_INSTRUCTION`]. A
/// generic LLM-backed hand works out of the box; a non-LLM runner may ignore it.
pub const CURATION_PACKET_INSTRUCTION: &str = "Curate this CANDIDATE skeleton by PROPOSING a batch of candidate_edit ops as DATA — merge thin blocks, name the provisional ones (a short plain-text name <= 40 chars + a one-line purpose <= 120), resolve the seams, and assign the unmapped residue that clearly belongs somewhere; you NEVER write the store and you can NEVER ratify — the owner validates, sanitizes and applies your proposal under the store_version below (OCC), and the human ratifies the result. Reply with EXACTLY one JSON document and nothing else: {\"schema\":\"m1nd-curation-proposal-v0\",\"ops\":[...],\"report\":\"...\"} where each op is one of {\"op\":\"rename\",\"block_id\":\"...\",\"name\":\"...\",\"purpose\":\"...\"} | {\"op\":\"merge\",\"into\":\"...\",\"block_ids\":[...]} | {\"op\":\"split\",\"block_id\":\"...\",\"by\":{\"paths\":[[...]]}} | {\"op\":\"move_member\",\"path\":\"...\",\"from\":\"...\",\"to\":\"...\"} | {\"op\":\"resolve_seam\",\"path\":\"...\",\"resolution\":\"both|primary:<block_id>\"} | {\"op\":\"assign_unmapped\",\"path\":\"...\",\"block_id\":\"...\"}. `report` is one honest paragraph: what you merged/named/resolved and what you deliberately left — no markdown, no URLs, no paths, no secrets, no extra keys, no surrounding text.";

// ===========================================================================
// The curation packet (§2) — the VIEW the owner composes for the hand.
// ===========================================================================

/// One block's confidence components (the `candidate_meta` the human weighs) — a
/// leaner mirror shipped in the packet so the hand can judge which blocks are thin.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BlockConfidence {
    pub graph_cohesion: Option<f64>,
    pub edge_sample_size: usize,
    pub directory_support: f64,
    pub coverage_ratio: f64,
    pub shared_member_count: usize,
}

/// One block's curation VIEW (§2): everything the hand needs to reason about a block
/// WITHOUT reading the store or any file body. Reuses the naming packet's member/
/// kind/symbol extraction plus the block's state, confidence, and seam members.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BlockCurationView {
    pub block_id: String,
    pub name: String,
    pub purpose: String,
    /// True while the block still carries an untouched provisional (heuristic) name.
    pub needs_owner_naming: bool,
    /// `heuristic` | `owner` | `runner` — the honest provenance of the current name.
    pub named_by: String,
    pub member_count: usize,
    /// Repo-relative member paths, capped (the honest total rides in `member_count`).
    pub member_paths: Vec<String>,
    pub dominant_kinds: Vec<String>,
    pub top_symbols: Vec<String>,
    pub confidence: BlockConfidence,
    /// The block's members that ANOTHER block also owns (shared role or multi-owner
    /// path) — the seams the hand may resolve.
    pub seam_members: Vec<String>,
}

/// The curation packet piped to the hand-runner's stdin (§2). Self-contained: the
/// embedded instruction, the OCC store_version + skeleton id the ops must anchor to,
/// the block views, and the unmapped residue sample.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CurationPacket {
    pub instruction: String,
    /// The schema the hand must stamp on its proposal (echoed so a generic runner
    /// works out of the box, mirroring the naming packet's instruction).
    pub proposal_schema: String,
    pub skeleton_id: String,
    pub store_version: u64,
    pub blocks: Vec<BlockCurationView>,
    pub unmapped_total: usize,
    /// A capped sample of the unmapped files (the honest total is `unmapped_total`).
    pub unmapped_sample: Vec<String>,
}

/// How many unmapped files the packet materializes (the honest total rides beside it).
pub const PACKET_UNMAPPED_SAMPLE_CAP: usize = 24;

/// Compose the curation packet from the store + the live graph nodes (§2). Member
/// paths / dominant kinds / top symbols come from the SAME builder the naming lane
/// uses ([`crate::skeleton_scan::naming_packet_for_store_block`]); the seam map is
/// computed once over the store so each block names its shared members honestly.
pub fn compose_curation_packet(
    store: &SystemBlockStore,
    nodes: &[SkeletonGraphNode],
) -> CurationPacket {
    // Path → number of blocks that own it (any role) — the seam signal.
    let mut owners: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for block in &store.blocks {
        for m in &block.membership {
            *owners.entry(m.path.as_str()).or_default() += 1;
        }
    }

    let blocks = store
        .blocks
        .iter()
        .map(|block| block_view(block, nodes, &owners))
        .collect();

    CurationPacket {
        instruction: CURATION_PACKET_INSTRUCTION.to_string(),
        proposal_schema: CURATION_PROPOSAL_SCHEMA.to_string(),
        skeleton_id: store.skeleton.skeleton_id.clone(),
        store_version: store.store_version,
        blocks,
        unmapped_total: store.unmapped_total,
        unmapped_sample: store
            .unmapped_files
            .iter()
            .take(PACKET_UNMAPPED_SAMPLE_CAP)
            .cloned()
            .collect(),
    }
}

fn block_view(
    block: &SystemBlock,
    nodes: &[SkeletonGraphNode],
    owners: &std::collections::HashMap<&str, usize>,
) -> BlockCurationView {
    let packet = crate::skeleton_scan::naming_packet_for_store_block(block, nodes);
    let (needs_owner_naming, named_by, confidence) = match &block.candidate_meta {
        Some(meta) => (
            meta.needs_owner_naming,
            named_by_label(&meta.named_by),
            BlockConfidence {
                graph_cohesion: meta.graph_cohesion,
                edge_sample_size: meta.edge_sample_size,
                directory_support: meta.directory_support,
                coverage_ratio: meta.coverage_ratio,
                shared_member_count: meta.shared_member_count,
            },
        ),
        None => (
            false,
            "unknown".to_string(),
            BlockConfidence {
                graph_cohesion: None,
                edge_sample_size: 0,
                directory_support: 0.0,
                coverage_ratio: 0.0,
                shared_member_count: 0,
            },
        ),
    };
    let seam_members: Vec<String> = block
        .membership
        .iter()
        .filter(|m| {
            m.role == crate::system_blocks::MembershipRole::Shared
                || owners.get(m.path.as_str()).copied().unwrap_or(0) > 1
        })
        .map(|m| m.path.clone())
        .collect();

    BlockCurationView {
        block_id: block.block_id.clone(),
        name: block.name.clone(),
        purpose: block.purpose.clone(),
        needs_owner_naming,
        named_by,
        member_count: packet.member_count,
        member_paths: packet.member_paths,
        dominant_kinds: packet.dominant_kinds,
        top_symbols: packet.top_symbols,
        confidence,
        seam_members,
    }
}

fn named_by_label(named_by: &crate::system_blocks::NamedBy) -> String {
    match named_by {
        crate::system_blocks::NamedBy::Heuristic => "heuristic",
        crate::system_blocks::NamedBy::Owner => "owner",
        crate::system_blocks::NamedBy::Runner => "runner",
    }
    .to_string()
}

// ===========================================================================
// The loopback `/curate` client — owner → runner daemon (mirror of /name's).
// ===========================================================================

/// The daemon's `/curate` answer (owner-side): the resolved runner id, whether the
/// mission produced a shape-valid proposal, and either the proposal or the honest
/// whole-mission error.
#[derive(Debug, Clone, PartialEq)]
pub struct CurationDaemonResult {
    pub runner_id: String,
    pub ok: bool,
    pub proposal: Option<Value>,
    pub error: Option<String>,
}

/// The owner-side wait budget for one `/curate` call. A curation is one synchronous
/// propose over a whole candidate — a real CLI hand takes minutes (process startup +
/// reasoning), and the daemon's `curation_timeout_secs` bounds it (default 300, up to
/// the operator's cap). The owner waits generously PAST the daemon's own timeout so
/// the daemon's honest `ok:false` returns instead of the socket being cut mid-read.
fn curation_wait_budget() -> Duration {
    Duration::from_secs(900)
}

/// POST the curation packet to an announced daemon's `/curate` and return its answer.
/// `runner_id: None` lets the DAEMON resolve its pinned hand-runner (announce carries
/// no capability, §5a). A non-200 answer or transport failure is an `Err` carrying
/// the daemon's honest keyword; the caller degrades to the `no_hand_runner` refusal.
pub fn call_curate_endpoint(
    port: u16,
    secret: &str,
    packet: &CurationPacket,
    timeout: Duration,
) -> Result<CurationDaemonResult, String> {
    use std::io::{Read, Write};

    let body = serde_json::to_string(&json!({ "packet": packet })).map_err(|e| e.to_string())?;

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let connect_timeout = timeout.min(Duration::from_secs(5));
    let mut stream = std::net::TcpStream::connect_timeout(&addr, connect_timeout)
        .map_err(|e| format!("connect to the runner daemon on port {port} failed: {e}"))?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(connect_timeout));

    let request = format!(
        "POST /curate HTTP/1.1\r\nhost: 127.0.0.1:{port}\r\n{}: {secret}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        crate::runnerd_owner::RUNNERD_SECRET_HEADER,
        body.len(),
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("write to the runner daemon failed: {e}"))?;

    let mut raw = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&chunk[..n]),
            Err(e) => {
                if raw.is_empty() {
                    return Err(format!("read from the runner daemon failed: {e}"));
                }
                break;
            }
        }
    }

    parse_curate_response(&raw)
}

/// Parse the daemon's raw HTTP response: status line + a JSON body. Split out for
/// direct testing (mirror of `naming_runner::parse_name_response`).
fn parse_curate_response(raw: &[u8]) -> Result<CurationDaemonResult, String> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| "malformed HTTP response from the runner daemon".to_string())?;
    let head = String::from_utf8_lossy(&raw[..split]);
    let status: u16 = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "malformed HTTP status line from the runner daemon".to_string())?;
    let mut body = &raw[split + 4..];
    for line in head.lines().skip(1) {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                if let Ok(len) = v.trim().parse::<usize>() {
                    if len <= body.len() {
                        body = &body[..len];
                    }
                }
            }
        }
    }
    if status == 401 {
        return Err("unauthorized (401): the runner daemon refused the shared secret".to_string());
    }
    let value: Value = if body.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(body)
            .map_err(|e| format!("invalid JSON from the runner daemon (status {status}): {e}"))?
    };
    if status != 200 {
        let keyword = value
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("runner_error");
        let detail = value
            .get("detail")
            .and_then(|v| v.as_str())
            .unwrap_or("the runner daemon refused the curation call");
        return Err(format!("{keyword}: {detail}"));
    }
    Ok(CurationDaemonResult {
        runner_id: value
            .get("runner_id")
            .and_then(|v| v.as_str())
            .unwrap_or("hand")
            .to_string(),
        ok: value.get("ok").and_then(|v| v.as_bool()).unwrap_or(false),
        proposal: value.get("proposal").filter(|v| !v.is_null()).cloned(),
        error: value
            .get("error")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
}

// ===========================================================================
// The curation_spawn transaction core (§3) — validate → apply → post summary.
// ===========================================================================

/// What a `curation_spawn` call did — the wire result the screen reads.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CurationSpawnOutcome {
    /// True iff the hand's batch was applied (an empty proposal is applied trivially).
    pub applied: bool,
    /// How many ops the hand proposed (0 = an honest "nothing to change" proposal).
    pub ops_count: usize,
    /// The store version AFTER the call (bumped once iff any op applied).
    pub store_version: u64,
    /// The hand's honest report paragraph (uncapped — the screen shows it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
    /// The mission the summary letter landed in (the tray watches it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mission_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mission_seq: Option<u64>,
    /// The honest whole-call refusal when nothing was applied (`no_hand_runner`,
    /// `proposal_malformed`, `batch_refused`) — the screen disables/tells why. An OCC
    /// conflict is a hard error (a stale view is never silently applied), not a refusal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
}

/// Mint a fresh `msn_<12hex>` mission id (mission_letter `valid_mission_id`). No
/// `rand` dep here — a sha256 of (pid, nanos) is unique-enough per call and keeps the
/// crate dependency-free, mirroring how the mailbox/side-record mint scratch ids.
fn mint_mission_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seed = format!("{}-{nanos}", std::process::id());
    let hex = sha256_hex(seed.as_bytes());
    format!("msn_{}", &hex[..12])
}

fn now_iso() -> String {
    crate::system_blocks_handlers::iso8601_from_ms(crate::util::now_ms())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Build one curation mission letter (§3/§4). Every curation letter is seat `oracle`
/// (the owner is the JUDGING seat over the hand's proposal), capability `hand-runner`
/// (the lane), block_id = the skeleton id (the whole-skeleton anchor the mission_post
/// guard recognizes). `runner_id` names the hand that proposed (honest provenance).
#[allow(clippy::too_many_arguments)]
fn curation_letter(
    mission_id: &str,
    seq: u64,
    prev: Option<String>,
    skeleton_id: &str,
    brain_ref: &str,
    runner_id: &str,
    packet_ref: &str,
    started_at: &str,
    verdict: Option<Verdict>,
) -> MissionLetter {
    MissionLetter {
        schema: MISSION_LETTER_SCHEMA.to_string(),
        mission_id: mission_id.to_string(),
        mission_seq: seq,
        prev_letter_id: prev,
        block_id: skeleton_id.to_string(),
        brain_ref: brain_ref.to_string(),
        seat: Seat::Oracle,
        runner_id: Some(runner_id.to_string()),
        capability: Capability::HandRunner,
        phase: Phase::Judging,
        verdict,
        gate: None,
        receipt_candidate: None,
        receipt: None,
        packet_ref: Some(packet_ref.to_string()),
        tokens_total: 0,
        started_at: started_at.to_string(),
        updated_at: now_iso(),
        synthetic: false,
    }
}

/// The whole `curation_spawn` transaction (§3). OCC pre-check BEFORE any network →
/// compose is done by the caller → open the mission chain (seq-1 `judging`) → call an
/// announced daemon's `/curate` → validate the proposal → acquire the advisory lease
/// as the hand → apply the WHOLE batch (runner seat, o5 + o1, SAME OCC) → release the
/// lease → post the summary letter (seq-2 `judging`, the report verbatim in the
/// verdict gist). Honest outcomes:
/// - stale `expected_store_version` → [`M1ndError`] carrying `conflict` BEFORE any
///   network call (nothing ran, nothing applied);
/// - no daemon / no hand-runner / transport failure → `Ok` with `refusal:no_hand_runner`;
/// - a runner failure / malformed proposal → `Ok` with `refusal:proposal_malformed`;
/// - an o5/preflight violation → `Ok` with `refusal:batch_refused` (nothing persisted);
/// - an OCC conflict AT APPLY time → [`M1ndError`] carrying `conflict` (lease released).
pub fn curate_candidate(
    handle: &crate::runnerd_owner::NamingRunnerHandle,
    dir: &Path,
    box_path: &Path,
    brain_ref: &str,
    expected_store_version: u64,
    packet: &CurationPacket,
) -> Result<CurationSpawnOutcome, M1ndError> {
    let tool_err = |detail: String| M1ndError::InvalidParams {
        tool: "curation_spawn".to_string(),
        detail,
    };
    let seed_err = |e: SeedError| tool_err(e.to_string());

    let store = SystemBlockStore::load(dir)
        .map_err(seed_err)?
        .ok_or_else(|| seed_err(SeedError::NoStore))?;
    // OCC pre-check: a stale caller conflicts BEFORE any runner is invoked.
    if expected_store_version != store.store_version {
        return Err(seed_err(SeedError::Conflict {
            expected: expected_store_version,
            actual: store.store_version,
        }));
    }
    let skeleton_id = store.skeleton.skeleton_id.clone();

    let refuse = |reason: String| {
        Ok(CurationSpawnOutcome {
            applied: false,
            ops_count: 0,
            store_version: store.store_version,
            report: None,
            mission_id: None,
            mission_seq: None,
            refusal: Some(reason),
        })
    };

    // No announce surface / no daemon booted here → the honest refusal (a legible
    // RESULT the screen reads, never an exception).
    let Some(secret) = crate::runnerd_owner::read_secret(&handle.owner_runtime_root) else {
        return refuse(
            "no_hand_runner: no runner daemon has booted here (no shared secret on disk)"
                .to_string(),
        );
    };
    let ports = handle.registry.live_ports();
    if ports.is_empty() {
        return refuse(
            "no_hand_runner: no runner daemon announced — start m1nd-runnerd with a pinned hand-runner"
                .to_string(),
        );
    }

    // Open the mission chain: seq-1 `judging` (the shape the UI DIRECT path composes).
    let mission_id = mint_mission_id();
    let started_at = now_iso();
    let packet_ref = format!(
        "sha256:{}",
        sha256_hex(serde_json::to_string(packet).unwrap_or_default().as_bytes())
    );

    // Try each announced daemon's /curate; the daemon resolves its own hand pin.
    let timeout = curation_wait_budget();
    let mut last_err = String::new();
    let mut daemon_result: Option<CurationDaemonResult> = None;
    for port in ports {
        match call_curate_endpoint(port, &secret, packet, timeout) {
            Ok(res) => {
                daemon_result = Some(res);
                break;
            }
            Err(e) => last_err = e,
        }
    }
    let Some(result) = daemon_result else {
        return refuse(format!("no_hand_runner: {last_err}"));
    };
    if !result.ok {
        let reason = result
            .error
            .unwrap_or_else(|| "the hand-runner returned no proposal".to_string());
        return refuse(format!("proposal_malformed: {reason}"));
    }
    let Some(proposal) = result.proposal else {
        return refuse(
            "proposal_malformed: the hand-runner reported ok but carried no proposal".to_string(),
        );
    };
    let runner_id = result.runner_id;
    let agent_id = format!("runnerd:{runner_id}");

    // Re-parse the ops into typed candidate_edit ops (the owner trusts no wire).
    let ops_value = proposal.get("ops").cloned().unwrap_or(Value::Null);
    let ops: Vec<EditOp> = match serde_json::from_value(ops_value) {
        Ok(v) => v,
        Err(e) => {
            return refuse(format!(
                "proposal_malformed: the ops are not valid candidate_edit ops: {e}"
            ))
        }
    };
    let report = proposal
        .get("report")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // The chain opener — posted before the apply (the mission is dispatched).
    let opener = curation_letter(
        &mission_id,
        1,
        None,
        &skeleton_id,
        brain_ref,
        &runner_id,
        &packet_ref,
        &started_at,
        None,
    );
    let opener_id = mission_letter::post_mission_letter(box_path, &agent_id, &opener)
        .map_err(|e| tool_err(e.to_string()))?
        .letter_id;

    // Acquire the advisory lease as the curating hand (o4 — advisory, best-effort:
    // it never blocks the owner and never bumps the OCC counter). Release it after
    // the apply regardless of outcome.
    let ms = crate::util::now_ms();
    let lease_now = crate::system_blocks_handlers::iso8601_from_ms(ms);
    let lease_until = crate::system_blocks_handlers::iso8601_from_ms(
        ms.saturating_add(DEFAULT_LEASE_TTL_SECS.saturating_mul(1000)),
    );
    let _ = candidate_lease_in_dir(
        dir,
        LeaseAction::Acquire,
        &agent_id,
        &lease_now,
        &lease_until,
    );
    let release = |dir: &Path, agent_id: &str| {
        let ms = crate::util::now_ms();
        let iso = crate::system_blocks_handlers::iso8601_from_ms(ms);
        let _ = candidate_lease_in_dir(dir, LeaseAction::Release, agent_id, &iso, &iso);
    };

    // Apply the WHOLE batch (runner seat → o5 governs renames; o1 preflight → an
    // invalid op persists NOTHING) under the caller's OCC key. An empty proposal is
    // an honest no-op (the hand judged the candidate already good).
    let new_version = if ops.is_empty() {
        store.store_version
    } else {
        match candidate_edit_in_dir(dir, expected_store_version, &ops, EditSeat::Runner) {
            Ok(new_store) => new_store.store_version,
            Err(SeedError::Conflict { expected, actual }) => {
                release(dir, &agent_id);
                return Err(seed_err(SeedError::Conflict { expected, actual }));
            }
            Err(SeedError::CandidateEdit { op_index, reason }) => {
                release(dir, &agent_id);
                return refuse(format!("batch_refused: op {op_index} — {reason}"));
            }
            Err(other) => {
                release(dir, &agent_id);
                return Err(seed_err(other));
            }
        }
    };
    release(dir, &agent_id);

    // The summary letter (seq-2 `judging`): the owner (oracle seat) records the
    // applied curation with the hand's report verbatim in the verdict gist (capped so
    // the letter stays bounded). APPROVE = the proposal passed o5/preflight/OCC and
    // was accepted into the candidate for the human's final ratify.
    let gist: String = report.chars().take(REPORT_GIST_CAP).collect();
    let summary = curation_letter(
        &mission_id,
        2,
        Some(opener_id),
        &skeleton_id,
        brain_ref,
        &runner_id,
        &packet_ref,
        &started_at,
        Some(Verdict {
            decision: VerdictDecision::Approve,
            gist: if gist.is_empty() {
                "curation applied".to_string()
            } else {
                gist
            },
        }),
    );
    let summary_out = mission_letter::post_mission_letter(box_path, &agent_id, &summary)
        .map_err(|e| tool_err(e.to_string()))?;

    Ok(CurationSpawnOutcome {
        applied: true,
        ops_count: ops.len(),
        store_version: new_version,
        report: Some(report),
        mission_id: Some(mission_id),
        mission_seq: Some(summary_out.mission_seq),
        refusal: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runnerd_owner::{secret_path, NamingRunnerHandle, RunnerdRegistry};
    use crate::system_blocks::{
        CandidateMeta, Layout, MembershipEntry, MembershipRole, MembershipSource, NamedBy,
        ReceiptContract, SeedFile, SeedRatification, SeedRepo, SeedSkeleton, SeedSkeletonState,
        Sockets, SystemBlock, SystemBlockKind, SystemBlockState, UnmappedDefaultAction,
        UnmappedPolicy, SYSTEM_BLOCK_SEED_SCHEMA,
    };

    fn heuristic_block(id: &str, paths: &[&str]) -> SystemBlock {
        SystemBlock {
            block_id: id.to_string(),
            name: format!("Heuristic {id}"),
            purpose: "Provisional.".to_string(),
            kind: SystemBlockKind::Scanned,
            state: SystemBlockState::Candidate,
            boundary_version: 1,
            contract_version: 1,
            membership_source: MembershipSource::Proposed,
            membership: paths
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
                named_by: NamedBy::Heuristic,
                needs_owner_naming: true,
                graph_cohesion: Some(0.5),
                edge_sample_size: 40,
                directory_support: 0.9,
                coverage_ratio: 0.9,
                shared_member_count: 0,
            }),
        }
    }

    fn seed_of(blocks: Vec<SystemBlock>) -> SeedFile {
        SeedFile {
            schema: SYSTEM_BLOCK_SEED_SCHEMA.to_string(),
            repo: SeedRepo {
                repo_id: "r".to_string(),
                root: ".".to_string(),
                source_commit: "c".to_string(),
            },
            skeleton: SeedSkeleton {
                skeleton_id: "sk_demo".to_string(),
                version: 1,
                state: SeedSkeletonState::Candidate,
                ratification: SeedRatification {
                    method: String::new(),
                    ratifier: String::new(),
                    ratified_at: String::new(),
                    commit: String::new(),
                },
            },
            blocks,
            unmapped_policy: UnmappedPolicy {
                visible: true,
                default_action: UnmappedDefaultAction::LeaveUnmappedUntilRatified,
            },
        }
    }

    /// A tempdir store + an owner runtime root with the shared secret + a registry.
    fn fixture(
        blocks: Vec<SystemBlock>,
    ) -> (tempfile::TempDir, std::path::PathBuf, NamingRunnerHandle) {
        let temp = tempfile::tempdir().expect("tempdir");
        let store_dir = temp.path().join("brain");
        std::fs::create_dir_all(&store_dir).expect("store dir");
        SystemBlockStore::from_seed(seed_of(blocks))
            .save(&store_dir)
            .expect("save");
        let owner_rt = temp.path().join("owner-rt");
        std::fs::create_dir_all(&owner_rt).expect("owner rt");
        std::fs::write(secret_path(&owner_rt), "curate-secret").expect("secret");
        let handle = NamingRunnerHandle {
            registry: std::sync::Arc::new(RunnerdRegistry::default()),
            owner_runtime_root: owner_rt,
        };
        (temp, store_dir, handle)
    }

    fn packet_for(store: &SystemBlockStore) -> CurationPacket {
        compose_curation_packet(store, &[])
    }

    /// A minimal fake daemon: accepts ONE connection, asserts the secret header,
    /// answers with the canned body. Never a real hand-runner.
    fn spawn_fake_daemon(
        status_line: &'static str,
        body: String,
        expect_secret: &'static str,
    ) -> (u16, std::thread::JoinHandle<String>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (mut sock, _) = listener.accept().expect("accept");
            let mut req = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = sock.read(&mut chunk).expect("read");
                req.extend_from_slice(&chunk[..n]);
                if let Some(pos) = req.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&req[..pos]).to_string();
                    let want: usize = head
                        .lines()
                        .find_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            k.trim()
                                .eq_ignore_ascii_case("content-length")
                                .then(|| v.trim().parse().ok())?
                        })
                        .unwrap_or(0);
                    if req.len() >= pos + 4 + want {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let request_text = String::from_utf8_lossy(&req).to_string();
            assert!(
                request_text.contains(&format!("x-runnerd-secret: {expect_secret}")),
                "the client must send the shared secret header: {request_text}"
            );
            let response = format!(
                "{status_line}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            sock.write_all(response.as_bytes()).expect("write");
            request_text
        });
        (port, handle)
    }

    fn box_path(temp: &tempfile::TempDir) -> std::path::PathBuf {
        temp.path().join("inbox.jsonl")
    }

    // --- the packet compositor -------------------------------------------------

    #[test]
    fn compose_curation_packet_carries_views_confidence_and_seams() {
        let store = SystemBlockStore::from_seed(seed_of(vec![
            heuristic_block("sb_a", &["auth/a.rs", "shared/hook.rs"]),
            heuristic_block("sb_b", &["billing/b.rs", "shared/hook.rs"]),
        ]));
        let packet = compose_curation_packet(&store, &[]);
        assert_eq!(packet.proposal_schema, CURATION_PROPOSAL_SCHEMA);
        assert_eq!(packet.skeleton_id, "sk_demo");
        assert_eq!(packet.blocks.len(), 2);
        assert!(packet.instruction.contains("m1nd-curation-proposal-v0"));
        // shared/hook.rs is owned by BOTH blocks → a seam member on each.
        let a = &packet.blocks[0];
        assert!(a.needs_owner_naming, "provisional block flagged");
        assert_eq!(a.named_by, "heuristic");
        assert!(
            a.seam_members.contains(&"shared/hook.rs".to_string()),
            "the shared member is a seam: {:?}",
            a.seam_members
        );
        assert!(
            !a.seam_members.contains(&"auth/a.rs".to_string()),
            "a sole-owned member is not a seam"
        );
        assert!(a.confidence.directory_support > 0.0, "confidence shipped");
    }

    // --- the loopback /curate client -------------------------------------------

    #[test]
    fn call_curate_endpoint_round_trips_a_proposal_and_sends_the_secret() {
        let body = json!({
            "runner_id": "hand-1",
            "ok": true,
            "proposal": {
                "schema": CURATION_PROPOSAL_SCHEMA,
                "ops": [{"op":"rename","block_id":"sb_a","name":"Auth","purpose":"Owns login."}],
                "report": "named one block",
            },
        })
        .to_string();
        let (port, daemon) = spawn_fake_daemon("HTTP/1.1 200 OK", body, "s3cr3t");
        let store = SystemBlockStore::from_seed(seed_of(vec![heuristic_block("sb_a", &["a.rs"])]));
        let res = call_curate_endpoint(port, "s3cr3t", &packet_for(&store), Duration::from_secs(5))
            .expect("call ok");
        assert!(res.ok);
        assert_eq!(res.runner_id, "hand-1");
        let request_text = daemon.join().expect("daemon thread");
        assert!(request_text.contains("POST /curate"));
        assert!(request_text.contains("\"packet\""));
    }

    #[test]
    fn call_curate_endpoint_surfaces_daemon_refusals_and_401() {
        let store = SystemBlockStore::from_seed(seed_of(vec![heuristic_block("sb_a", &["a.rs"])]));
        // 403 no_hand_runner → keyword verbatim.
        let body = json!({"error":"no_hand_runner","detail":"no hand-runner pinned"}).to_string();
        let (port, _d) = spawn_fake_daemon("HTTP/1.1 403 Forbidden", body, "s");
        let err = call_curate_endpoint(port, "s", &packet_for(&store), Duration::from_secs(5))
            .expect_err("refusal maps to Err");
        assert!(err.contains("no_hand_runner"), "got '{err}'");
        // Bare 401.
        let (port, _d) = spawn_fake_daemon("HTTP/1.1 401 Unauthorized", String::new(), "s");
        let err = call_curate_endpoint(port, "s", &packet_for(&store), Duration::from_secs(5))
            .expect_err("401 maps to Err");
        assert!(err.contains("401"), "got '{err}'");
    }

    // --- the whole transaction (fake daemon) -----------------------------------

    #[test]
    fn curate_candidate_no_daemon_is_the_no_hand_runner_refusal() {
        let (temp, dir, handle) = fixture(vec![heuristic_block("sb_a", &["a.rs"])]);
        let store = SystemBlockStore::load(&dir).unwrap().unwrap();
        let before = std::fs::read(dir.join("system_blocks.json")).expect("read before");
        let out = curate_candidate(
            &handle,
            &dir,
            &box_path(&temp),
            "repo",
            1,
            &packet_for(&store),
        )
        .expect("a refusal is a result, not an exception");
        assert!(!out.applied);
        assert_eq!(out.store_version, 1, "no version churn");
        assert!(
            out.refusal
                .as_deref()
                .unwrap_or("")
                .contains("no_hand_runner"),
            "got {:?}",
            out.refusal
        );
        let after = std::fs::read(dir.join("system_blocks.json")).expect("read after");
        assert_eq!(before, after, "the store is byte-identical");
    }

    #[test]
    fn curate_candidate_occ_conflict_before_any_network() {
        let (temp, dir, handle) = fixture(vec![heuristic_block("sb_a", &["a.rs"])]);
        let store = SystemBlockStore::load(&dir).unwrap().unwrap();
        // NO daemon registered and none is contacted: a stale OCC key conflicts FIRST.
        let err = curate_candidate(
            &handle,
            &dir,
            &box_path(&temp),
            "repo",
            99,
            &packet_for(&store),
        )
        .expect_err("a stale OCC key conflicts");
        assert!(err.to_string().contains("conflict"), "got {err}");
    }

    #[test]
    fn curate_candidate_happy_path_applies_seat_runner_and_posts_the_summary() {
        let (temp, dir, handle) = fixture(vec![
            heuristic_block("sb_a", &["a.rs"]),
            heuristic_block("sb_b", &["b.rs"]),
        ]);
        let store = SystemBlockStore::load(&dir).unwrap().unwrap();
        // The hand proposes: rename sb_a + merge sb_b into sb_a.
        let body = json!({
            "runner_id": "hand-1",
            "ok": true,
            "proposal": {
                "schema": CURATION_PROPOSAL_SCHEMA,
                "ops": [
                    {"op":"rename","block_id":"sb_a","name":"Auth","purpose":"Owns login."},
                    {"op":"merge","into":"sb_a","block_ids":["sb_b"]}
                ],
                "report": "Named the auth block and folded the thin one into it.",
            },
        })
        .to_string();
        let (port, _d) = spawn_fake_daemon("HTTP/1.1 200 OK", body, "curate-secret");
        handle.registry.register(&["hand-1".to_string()], port, 1);

        let out = curate_candidate(
            &handle,
            &dir,
            &box_path(&temp),
            "repo",
            1,
            &packet_for(&store),
        )
        .expect("the curation lands");
        assert!(out.applied, "the batch applied: {out:?}");
        assert_eq!(out.ops_count, 2);
        assert_eq!(out.store_version, 2, "one OCC bump for the whole batch");
        assert_eq!(out.mission_seq, Some(2), "the summary is seq-2 (seq+1)");
        assert!(out.report.as_deref().unwrap().contains("Named the auth"));

        // The PERSISTED store carries the RUNNER provenance on the rename.
        let store = SystemBlockStore::load(&dir).unwrap().unwrap();
        let a = store.blocks.iter().find(|b| b.block_id == "sb_a").unwrap();
        assert_eq!(a.name, "Auth");
        assert_eq!(a.candidate_meta.as_ref().unwrap().named_by, NamedBy::Runner);
        assert!(
            store.blocks.iter().all(|b| b.block_id != "sb_b"),
            "sb_b was merged away"
        );
        // The advisory lease was released after the apply.
        assert!(store.curating_by.is_none(), "the lease is released");

        // The mission chain carries the summary letter with the report verbatim.
        let letters = crate::mailbox::read_letters(&box_path(&temp)).unwrap();
        let heads = crate::mission_letter::heads_by_mission(&letters);
        let head = heads.values().next().expect("a mission chain exists");
        assert_eq!(head.head.mission_seq, 2, "head is the seq-2 summary");
        assert_eq!(head.head.block_id, "sk_demo", "anchored to the skeleton id");
        assert_eq!(head.head.seat, Seat::Oracle);
        assert_eq!(head.head.capability, Capability::HandRunner);
        let verdict = head
            .head
            .verdict
            .as_ref()
            .expect("the summary carries the report");
        assert!(verdict.gist.contains("Named the auth block"));
    }

    #[test]
    fn curate_candidate_hostile_rename_dies_at_the_o5_guard() {
        let (temp, dir, handle) = fixture(vec![heuristic_block("sb_a", &["a.rs"])]);
        let store = SystemBlockStore::load(&dir).unwrap().unwrap();
        let before = std::fs::read(dir.join("system_blocks.json")).expect("read before");
        // A hostile rename (HTML markup) — the o5 sanitizer must kill the WHOLE batch.
        let body = json!({
            "runner_id": "hand-1",
            "ok": true,
            "proposal": {
                "schema": CURATION_PROPOSAL_SCHEMA,
                "ops": [{"op":"rename","block_id":"sb_a","name":"<script>x</script>","purpose":"p"}],
                "report": "tried to inject",
            },
        })
        .to_string();
        let (port, _d) = spawn_fake_daemon("HTTP/1.1 200 OK", body, "curate-secret");
        handle.registry.register(&["hand-1".to_string()], port, 1);

        let out = curate_candidate(
            &handle,
            &dir,
            &box_path(&temp),
            "repo",
            1,
            &packet_for(&store),
        )
        .expect("a refusal is a result");
        assert!(!out.applied, "the hostile batch never applies");
        let refusal = out.refusal.expect("batch_refused");
        assert!(refusal.contains("batch_refused"), "got '{refusal}'");
        assert!(
            refusal.contains("HTML"),
            "the o5 class surfaces: '{refusal}'"
        );
        // The store is byte-identical (o1 preflight-on-a-clone) and the lease is free.
        let after = std::fs::read(dir.join("system_blocks.json")).expect("read after");
        assert_eq!(before, after, "nothing was applied");
        let store = SystemBlockStore::load(&dir).unwrap().unwrap();
        assert!(
            store.curating_by.is_none(),
            "the lease is released on refusal"
        );
    }

    #[test]
    fn curate_candidate_malformed_proposal_is_proposal_malformed() {
        let (temp, dir, handle) = fixture(vec![heuristic_block("sb_a", &["a.rs"])]);
        let store = SystemBlockStore::load(&dir).unwrap().unwrap();
        // The daemon reports an honest whole-mission failure (ok:false).
        let body = json!({
            "runner_id": "hand-1",
            "ok": false,
            "error": "the curation proposal is not one JSON document",
        })
        .to_string();
        let (port, _d) = spawn_fake_daemon("HTTP/1.1 200 OK", body, "curate-secret");
        handle.registry.register(&["hand-1".to_string()], port, 1);

        let out = curate_candidate(
            &handle,
            &dir,
            &box_path(&temp),
            "repo",
            1,
            &packet_for(&store),
        )
        .expect("a refusal is a result");
        assert!(!out.applied);
        assert!(out
            .refusal
            .as_deref()
            .unwrap_or("")
            .contains("proposal_malformed"));
    }
}
