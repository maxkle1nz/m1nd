//! F2.5c — the mission engine (§5b/§5c/§5d): the ONLY spawner. It takes an accepted
//! spawn, opens (or extends) the mission chain, creates an ISOLATED git worktree,
//! runs the pinned agent command in it under a timeout, runs the gate, and emits the
//! phase letters through the owner. The hard laws it keeps:
//!
//! - **Worktree-per-mission, always (§5b/§5d).** The workspace must be a git repo and
//!   the worktree add must succeed — else a `failed` letter with the reason and
//!   NOTHING runs. The agent never runs outside an isolated worktree.
//! - **NEVER `landed` (§1d / §5c the landed-law).** The engine emits exactly
//!   `judging → executing → (merge_wait | failed)`. A green gate is `merge_wait` with
//!   a COMPLETE `receipt_candidate`; the import is a human act (F2.5d). No path here
//!   constructs a `landed` letter — the never-lands proof.
//! - **No host paths in the letter (§1f).** The worktree path + host notes go to the
//!   owner-runtime-local side record ([`m1nd_mcp::mission_local`]), never the letter.
//! - **Never silence (§5d).** A kill/timeout/gate-fail is a `failed` letter with the
//!   reason (carried in the verdict gist so the tray renders it), never a swallow.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use m1nd_mcp::mission_letter::{
    CandidateScope, Capability, GateEvidence, MissionLetter, Phase, ReceiptCandidate, Seat,
    Verdict, VerdictDecision, MISSION_LETTER_SCHEMA,
};
use m1nd_mcp::mission_local::{MissionLocalRecord, MissionLocalStore};
use m1nd_mcp::system_blocks::{ReceiptEvidence, ReceiptType};

use crate::config::{RunnerDef, RunnersConfig};
use crate::owner::OwnerClient;

/// The packet file name written into the worktree and handed to the agent (§5b).
const PACKET_FILE: &str = "m1nd-mission-packet.md";

/// The `POST /run` request (§B) the owner forwards. `brain` is the workspace
/// project_root (the git repo to run in AND the `?brain=` routing); `mission_id` is
/// present only for a compose-opened chain (§5b).
#[derive(Debug, Clone, Deserialize)]
pub struct RunRequest {
    pub runner_id: String,
    pub packet_markdown: String,
    pub block_id: String,
    pub brain_ref: String,
    #[serde(default)]
    pub brain: Option<String>,
    #[serde(default)]
    pub mission_id: Option<String>,
}

/// The sync `/run` refusals surfaced as HTTP status + a keyword (§B.1/§B.2). The
/// secret 401 is handled at the transport layer (the header check), not here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunRefusal {
    /// The `runner_id` is not pinned in `runners.toml` (§B.1) → 403 `unpinned_runner`.
    UnpinnedRunner { runner_id: String },
    /// The workspace is absent or not under the runner's allowlist (§B.2) → 403.
    WorkspaceNotAllowed {
        runner_id: String,
        workspace: String,
    },
}

impl RunRefusal {
    pub fn status(&self) -> u16 {
        403
    }
    pub fn keyword(&self) -> &'static str {
        match self {
            RunRefusal::UnpinnedRunner { .. } => "unpinned_runner",
            RunRefusal::WorkspaceNotAllowed { .. } => "workspace_not_allowed",
        }
    }
    pub fn detail(&self) -> String {
        match self {
            RunRefusal::UnpinnedRunner { runner_id } => format!(
                "runner '{runner_id}' is not pinned in runners.toml — announce proves liveness, it never grants a capability (§5a)"
            ),
            RunRefusal::WorkspaceNotAllowed {
                runner_id,
                workspace,
            } => format!(
                "runner '{runner_id}' may not run in workspace '{workspace}' — it is not under the runner's workspace_allowlist (§5a)"
            ),
        }
    }
}

/// The sync `/run` gate (§B.1/§B.2): the runner must be pinned, and its workspace
/// must be under the runner's allowlist. Returns the pinned [`RunnerDef`] on pass.
pub fn validate_run<'a>(
    cfg: &'a RunnersConfig,
    req: &RunRequest,
) -> Result<&'a RunnerDef, RunRefusal> {
    let runner =
        crate::config::find(cfg, &req.runner_id).ok_or_else(|| RunRefusal::UnpinnedRunner {
            runner_id: req.runner_id.clone(),
        })?;
    let workspace = req
        .brain
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| RunRefusal::WorkspaceNotAllowed {
            runner_id: req.runner_id.clone(),
            workspace: "<none>".to_string(),
        })?;
    if !crate::config::workspace_allowed(runner, workspace) {
        return Err(RunRefusal::WorkspaceNotAllowed {
            runner_id: req.runner_id.clone(),
            workspace: workspace.to_string(),
        });
    }
    Ok(runner)
}

/// Mint a fresh `msn_<12hex>` id (mission_letter `valid_mission_id`).
pub fn mint_mission_id() -> String {
    use rand::RngCore;
    let mut b = [0u8; 6];
    rand::rng().fill_bytes(&mut b);
    format!(
        "msn_{}",
        b.iter().map(|x| format!("{x:02x}")).collect::<String>()
    )
}

/// `sha256(bytes)` as lowercase hex — the SAME digest the mailbox/receipt taxonomy
/// uses, so a candidate is a direct hand-off at import time (§5c).
/// Lowercase hex of a byte slice.
///
/// `digest` 0.11 returns `hybrid_array::Array`, which — unlike the old
/// `generic_array::GenericArray` — does not implement `LowerHex`, so the
/// former `format!("{d:x}")` spelling no longer compiles. This is the
/// workspace's existing hex idiom, kept local so the digest call sites stay
/// dependency-free.
fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

/// The current instant as `YYYY-MM-DDTHH:MM:SSZ` (UTC) — the repo's dependency-free
/// civil-date math (mirrors `system_blocks_handlers::iso8601_from_ms`), so the tray's
/// `elapsedLabel` parses it, without pulling a datetime crate.
fn now_iso8601() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let secs = (ms / 1000) as i64;
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (h, mi, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

// ===========================================================================
// Letter builders (pure) — every runnerd letter is seat `hand` (the daemon IS the
// hand executor, §1a). NEVER a `landed` builder exists here (the never-lands law).
// ===========================================================================

/// Shared skeleton for every runnerd letter (§1). `runner_id` + `capability` name
/// the pinned lane; `seat` is always `hand`; the worktree path is NOT here (§1f).
struct LetterCtx {
    mission_id: String,
    started_at: String,
    packet_ref: String,
    block_id: String,
    brain_ref: String,
    runner_id: String,
    capability: Capability,
}

impl LetterCtx {
    fn base(&self, seq: u64, prev: Option<String>, phase: Phase) -> MissionLetter {
        MissionLetter {
            schema: MISSION_LETTER_SCHEMA.to_string(),
            mission_id: self.mission_id.clone(),
            mission_seq: seq,
            prev_letter_id: prev,
            block_id: self.block_id.clone(),
            brain_ref: self.brain_ref.clone(),
            seat: Seat::Hand,
            runner_id: Some(self.runner_id.clone()),
            capability: self.capability,
            phase,
            verdict: None,
            gate: None,
            receipt_candidate: None,
            receipt: None,
            packet_ref: Some(self.packet_ref.clone()),
            tokens_total: 0,
            started_at: self.started_at.clone(),
            updated_at: now_iso8601(),
            // A runnerd letter always names the real block its packet targets.
            synthetic: false,
        }
    }

    /// seq-1 `judging` — the daemon opening the mission (§5b, when no compose did).
    fn judging(&self, seq: u64, prev: Option<String>) -> MissionLetter {
        self.base(seq, prev, Phase::Judging)
    }

    /// `executing` — the agent is running in the worktree (§5b). No verdict (§1b).
    fn executing(&self, seq: u64, prev: Option<String>) -> MissionLetter {
        self.base(seq, prev, Phase::Executing)
    }

    /// `merge_wait` — the gate is GREEN and a COMPLETE candidate is attached (§5c).
    /// NEVER `landed`: the import is a human act (§1d).
    fn merge_wait(
        &self,
        seq: u64,
        prev: Option<String>,
        gate: GateEvidence,
        candidate: ReceiptCandidate,
    ) -> MissionLetter {
        let mut l = self.base(seq, prev, Phase::MergeWait);
        l.gate = Some(gate);
        l.receipt_candidate = Some(candidate);
        l
    }

    /// `failed` — the reason rides the verdict gist so the tray renders it (§5d never
    /// silence). A gate-related failure also carries the gate line (with its hash).
    fn failed(
        &self,
        seq: u64,
        prev: Option<String>,
        reason: impl Into<String>,
        gate: Option<GateEvidence>,
    ) -> MissionLetter {
        let mut l = self.base(seq, prev, Phase::Failed);
        l.verdict = Some(Verdict {
            decision: VerdictDecision::Reject,
            gist: reason.into(),
        });
        l.gate = gate;
        l
    }
}

/// Build the COMPLETE receipt candidate a `merge_wait` carries (§5c). Its `type` +
/// `evidence` reuse the receipt taxonomy so import is a direct hand-off; the scope
/// comes from a FRESH owner snapshot; the evidence anchor is the REAL gate-log hash.
/// `cwd` is "." (the isolated worktree, never a host path, §1f); `started_at`/
/// `ended_at` are the gate's real wall-clock window — a `test` receipt is
/// un-importable without them.
#[allow(clippy::too_many_arguments)]
fn build_candidate(
    block_id: &str,
    scope: (u32, u32),
    gate_argv: &[String],
    gate_hash: &str,
    started_at: &str,
    ended_at: &str,
    gate_log: &str,
    mission_id: &str,
) -> ReceiptCandidate {
    ReceiptCandidate {
        block_id: block_id.to_string(),
        type_: ReceiptType::Test,
        scope: CandidateScope {
            boundary_version: scope.0,
            contract_version: scope.1,
        },
        evidence: ReceiptEvidence {
            command: Some(gate_argv.join(" ")),
            // The workspace is the isolated worktree by design; "." is the honest
            // repo-relative cwd (a host path never enters the letter, §1f) — but it
            // is PRESENT, so a `test` receipt's execution identity is complete.
            cwd: Some(".".to_string()),
            exit_status: Some(0),
            started_at: Some(started_at.to_string()),
            ended_at: Some(ended_at.to_string()),
            artifact_hash: gate_hash.to_string(),
            stdout_excerpt: Some(last_lines(gate_log, 12)),
            evidence_refs: vec![format!("gate://{mission_id}")],
        },
    }
}

/// The trailing `n` lines of the gate log — an honest excerpt for the receipt
/// (the full log is what the hash covers; this is the human-readable tail).
fn last_lines(log: &str, n: usize) -> String {
    let lines: Vec<&str> = log.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

// ===========================================================================
// Process + git helpers.
// ===========================================================================

/// The outcome of running a command with a timeout.
struct CmdOutcome {
    exit: Option<i32>,
    log: String,
    timed_out: bool,
    spawn_error: Option<String>,
}

/// Run `argv` in `cwd` with a wall-clock timeout, capturing stdout+stderr combined.
/// `kill_on_drop` guarantees a timed-out child is killed (§5d kill/timeout → failed).
async fn run_argv(cwd: &Path, argv: &[String], timeout_secs: u64) -> CmdOutcome {
    if argv.is_empty() {
        return CmdOutcome {
            exit: None,
            log: String::new(),
            timed_out: false,
            spawn_error: Some("empty command".to_string()),
        };
    }
    let mut cmd = tokio::process::Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return CmdOutcome {
                exit: None,
                log: String::new(),
                timed_out: false,
                spawn_error: Some(format!("spawn failed: {e}")),
            }
        }
    };
    match tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output()).await {
        Ok(Ok(out)) => {
            let mut log = String::from_utf8_lossy(&out.stdout).into_owned();
            log.push_str(&String::from_utf8_lossy(&out.stderr));
            CmdOutcome {
                exit: out.status.code(),
                log,
                timed_out: false,
                spawn_error: None,
            }
        }
        Ok(Err(e)) => CmdOutcome {
            exit: None,
            log: String::new(),
            timed_out: false,
            spawn_error: Some(format!("wait failed: {e}")),
        },
        Err(_) => CmdOutcome {
            exit: None,
            log: String::new(),
            timed_out: true,
            spawn_error: None,
        },
    }
}

/// Run `git <args>` in `dir`, returning `(success, combined_output)`.
async fn git(dir: &Path, args: &[&str]) -> (bool, String) {
    match tokio::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .await
    {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            (o.status.success(), s)
        }
        Err(e) => (false, format!("git spawn error: {e}")),
    }
}

async fn is_git_repo(workspace: &Path) -> bool {
    git(workspace, &["rev-parse", "--is-inside-work-tree"])
        .await
        .0
}

// ===========================================================================
// The engine — the ONLY spawner.
// ===========================================================================

/// Where the engine creates worktrees + writes the owner-runtime-local side record.
#[derive(Debug, Clone)]
pub struct EngineOpts {
    /// The base dir under which `msn_<id>` worktrees are created (outside the repo).
    pub worktree_base: PathBuf,
    /// The owner runtime root — the side record ([`m1nd_mcp::mission_local`]) lives here.
    pub runtime_root: PathBuf,
}

/// Post a letter, logging + returning `None` on an owner refusal (never fabricate a
/// success — §5d). The returned id is the next letter's `prev_letter_id`.
async fn post<C: OwnerClient>(
    client: &C,
    brain: Option<&str>,
    agent_id: &str,
    letter: &MissionLetter,
) -> Option<String> {
    match client.post_letter(brain, agent_id, letter).await {
        Ok(id) => Some(id),
        Err(e) => {
            eprintln!(
                "[m1nd-runnerd] owner refused a {:?} letter for {}: {e}",
                letter.phase, letter.mission_id
            );
            None
        }
    }
}

/// Run one accepted mission to a terminal letter (§5b/§5c). Opens or extends the
/// chain, creates the isolated worktree (§5d — else `failed`, nothing runs), spawns
/// the pinned agent under a timeout, runs the gate, and emits `merge_wait` (green,
/// with a complete candidate) or `failed` (never silence). Writes the worktree path
/// to the side record and cleans a clean worktree. NEVER emits `landed`.
pub async fn run_mission<C: OwnerClient>(
    client: &C,
    runner: &RunnerDef,
    req: &RunRequest,
    mission_id: &str,
    opts: &EngineOpts,
) {
    let brain = req.brain.as_deref();
    let agent_id = format!("runnerd:{}", runner.id);
    let ctx = LetterCtx {
        mission_id: mission_id.to_string(),
        started_at: now_iso8601(),
        packet_ref: format!("sha256:{}", sha256_hex(req.packet_markdown.as_bytes())),
        block_id: req.block_id.clone(),
        brain_ref: req.brain_ref.clone(),
        runner_id: runner.id.clone(),
        capability: runner.parsed_capability(),
    };

    // --- open or extend the chain ---------------------------------------------
    // A compose-opened mission already has seq-1 `judging` (§5b): read the head and
    // extend it. Otherwise the daemon opens seq-1 `judging` itself.
    let (mut seq, mut prev): (u64, Option<String>) = match req.mission_id.as_deref() {
        Some(_) => match client.fetch_head(brain, mission_id).await {
            Some(head) => (head.seq, Some(head.letter_id)),
            None => {
                // Named a mission with no chain yet — open seq-1 ourselves.
                match post(client, brain, &agent_id, &ctx.judging(1, None)).await {
                    Some(id) => (1, Some(id)),
                    None => return,
                }
            }
        },
        None => match post(client, brain, &agent_id, &ctx.judging(1, None)).await {
            Some(id) => (1, Some(id)),
            None => return,
        },
    };

    let workspace = match brain {
        Some(w) => PathBuf::from(w),
        None => {
            // Should never happen (validate_run required a workspace), but never run
            // without one — emit a failed letter honestly.
            let _ = post(
                client,
                brain,
                &agent_id,
                &ctx.failed(seq + 1, prev.take(), "no workspace root", None),
            )
            .await;
            return;
        }
    };

    // --- worktree-per-mission, always (§5b/§5d) -------------------------------
    let worktree = opts.worktree_base.join(mission_id);
    if !is_git_repo(&workspace).await {
        let _ = post(
            client,
            brain,
            &agent_id,
            &ctx.failed(
                seq + 1,
                prev.take(),
                format!(
                    "workspace '{}' is not a git repo — a mission NEVER runs outside an isolated worktree (§5d)",
                    workspace.display()
                ),
                None,
            ),
        )
        .await;
        return;
    }
    let worktree_str = worktree.to_string_lossy().to_string();
    let (added, add_log) = git(&workspace, &["worktree", "add", "--detach", &worktree_str]).await;
    if !added {
        let _ = post(
            client,
            brain,
            &agent_id,
            &ctx.failed(
                seq + 1,
                prev.take(),
                format!("git worktree add failed: {}", add_log.trim()),
                None,
            ),
        )
        .await;
        return;
    }

    // The worktree path is host-local detail — record it in the side record, NEVER
    // the letter (§1f). Written now so a crash mid-mission still leaves the trail.
    record_side(opts, mission_id, &worktree_str, &runner.id, false);

    // --- executing (§5b) ------------------------------------------------------
    seq += 1;
    prev = match post(client, brain, &agent_id, &ctx.executing(seq, prev.take())).await {
        Some(id) => Some(id),
        None => {
            cleanup(&workspace, &worktree, opts, mission_id, &runner.id).await;
            return;
        }
    };

    // Write the packet INTO the worktree and splice its path into the command (§5b).
    let packet_path = worktree.join(PACKET_FILE);
    if let Err(e) = std::fs::write(&packet_path, &req.packet_markdown) {
        seq += 1;
        let _ = post(
            client,
            brain,
            &agent_id,
            &ctx.failed(
                seq,
                prev.take(),
                format!("could not write the packet into the worktree: {e}"),
                None,
            ),
        )
        .await;
        cleanup(&workspace, &worktree, opts, mission_id, &runner.id).await;
        return;
    }
    let packet_abs = packet_path.to_string_lossy().to_string();
    let argv: Vec<String> = runner
        .command
        .iter()
        .map(|a| a.replace(crate::config::PACKET_FILE_TOKEN, &packet_abs))
        .collect();

    // --- run the agent under the timeout --------------------------------------
    let agent = run_argv(&worktree, &argv, runner.timeout_secs).await;
    if agent.timed_out {
        seq += 1;
        let _ = post(
            client,
            brain,
            &agent_id,
            &ctx.failed(
                seq,
                prev.take(),
                format!(
                    "agent timed out after {}s — killed (§5d)",
                    runner.timeout_secs
                ),
                None,
            ),
        )
        .await;
        cleanup(&workspace, &worktree, opts, mission_id, &runner.id).await;
        return;
    }
    if let Some(err) = agent.spawn_error {
        seq += 1;
        let _ = post(
            client,
            brain,
            &agent_id,
            &ctx.failed(
                seq,
                prev.take(),
                format!("agent could not run: {err}"),
                None,
            ),
        )
        .await;
        cleanup(&workspace, &worktree, opts, mission_id, &runner.id).await;
        return;
    }

    // The packet was the daemon's transient input — remove it before assessing the
    // agent's own changes, so cleanup reflects the AGENT's work, not our artifact.
    let _ = std::fs::remove_file(&packet_path);

    // --- the gate + the candidate (§5c) ---------------------------------------
    // The gate's real wall-clock window — a `test` receipt's execution identity
    // (cwd/started_at/ended_at) is MANDATORY at import, so the runner records it
    // here rather than leaving it None (the bug that made the first spawned
    // candidate un-importable). The window is the runner's own honest clock.
    let gate_started = now_iso8601();
    let gate = run_argv(&worktree, &runner.gate_command, runner.timeout_secs).await;
    let gate_ended = now_iso8601();
    let gate_hash = format!("sha256:{}", sha256_hex(gate.log.as_bytes()));
    let gate_cmdline = runner.gate_command.join(" ");

    seq += 1;
    if gate.exit == Some(0) {
        // Green gate → merge_wait with a COMPLETE candidate. The scope comes from a
        // FRESH owner snapshot; a miss falls back to (1,1), declared honestly.
        let scope = client
            .fetch_block_scope(brain, &req.block_id)
            .await
            .unwrap_or((1, 1));
        let gate_ev = GateEvidence {
            command: gate_cmdline.clone(),
            exit_status: 0,
            artifact_hash: gate_hash.clone(),
        };
        let candidate = build_candidate(
            &req.block_id,
            scope,
            &runner.gate_command,
            &gate_hash,
            &gate_started,
            &gate_ended,
            &gate.log,
            mission_id,
        );
        let _ = post(
            client,
            brain,
            &agent_id,
            &ctx.merge_wait(seq, prev.take(), gate_ev, candidate),
        )
        .await;
    } else {
        // Gate red (or it could not run) → failed, carrying the gate line + reason.
        let reason = if gate.timed_out {
            format!(
                "gate `{gate_cmdline}` timed out after {}s",
                runner.timeout_secs
            )
        } else if let Some(err) = &gate.spawn_error {
            format!("gate `{gate_cmdline}` could not run: {err}")
        } else {
            format!(
                "gate `{gate_cmdline}` exited {} — not green, receipt not landed",
                gate.exit
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "killed".to_string())
            )
        };
        // A gate that actually RAN carries its evidence line (with the real hash);
        // one that never spawned has no exit to show.
        let gate_ev = if gate.spawn_error.is_none() {
            Some(GateEvidence {
                command: gate_cmdline,
                exit_status: gate.exit.unwrap_or(-1),
                artifact_hash: gate_hash,
            })
        } else {
            None
        };
        let _ = post(
            client,
            brain,
            &agent_id,
            &ctx.failed(seq, prev.take(), reason, gate_ev),
        )
        .await;
    }

    cleanup(&workspace, &worktree, opts, mission_id, &runner.id).await;
}

/// Write/refresh the owner-runtime-local side record (§1f): the worktree path + a
/// host note. `dirty` flips the note when the worktree is kept for the human.
fn record_side(opts: &EngineOpts, mission_id: &str, worktree: &str, runner_id: &str, dirty: bool) {
    let note = if dirty {
        format!("runner {runner_id}: worktree KEPT (uncommitted changes to review)")
    } else {
        format!("runner {runner_id}")
    };
    let rec = MissionLocalRecord {
        worktree: Some(worktree.to_string()),
        host_notes: Some(note),
    };
    if let Err(e) = MissionLocalStore::put(&opts.runtime_root, mission_id, rec) {
        eprintln!("[m1nd-runnerd] could not write the side record for {mission_id}: {e}");
    }
}

/// Hygiene (§B.7): remove the worktree when its working tree is CLEAN; keep it +
/// note it in the side record when it is DIRTY (the human reviews the agent's work).
async fn cleanup(
    workspace: &Path,
    worktree: &Path,
    opts: &EngineOpts,
    mission_id: &str,
    runner_id: &str,
) {
    let (ok, out) = git(worktree, &["status", "--porcelain"]).await;
    let worktree_str = worktree.to_string_lossy().to_string();
    let clean = ok && out.trim().is_empty();
    if clean {
        let (removed, log) =
            git(workspace, &["worktree", "remove", "--force", &worktree_str]).await;
        if !removed {
            eprintln!(
                "[m1nd-runnerd] worktree remove failed for {mission_id}: {}",
                log.trim()
            );
            record_side(opts, mission_id, &worktree_str, runner_id, true);
        }
    } else {
        // Dirty → keep it and record why (the hygiene rule: never delete unproven).
        record_side(opts, mission_id, &worktree_str, runner_id, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // A fake owner that records every posted letter and returns synthetic ids, so the
    // chain + the gate/candidate are provable with NO network + NO real owner.
    #[derive(Default)]
    struct FakeOwner {
        letters: Arc<Mutex<Vec<MissionLetter>>>,
        scope: Option<(u32, u32)>,
    }
    impl OwnerClient for FakeOwner {
        async fn post_letter(
            &self,
            _brain: Option<&str>,
            _agent_id: &str,
            letter: &MissionLetter,
        ) -> Result<String, String> {
            let mut g = self.letters.lock().unwrap();
            g.push(letter.clone());
            Ok(format!("id{}", g.len()))
        }
        async fn fetch_block_scope(
            &self,
            _brain: Option<&str>,
            _block_id: &str,
        ) -> Option<(u32, u32)> {
            self.scope
        }
        async fn fetch_head(
            &self,
            _brain: Option<&str>,
            _mission_id: &str,
        ) -> Option<super::super::owner::HeadInfo> {
            None
        }
    }

    fn runner(command: Vec<&str>, gate: Vec<&str>, workspace: &str) -> RunnerDef {
        RunnerDef {
            id: "build-1".to_string(),
            capability: "build-runner".to_string(),
            command: command.into_iter().map(String::from).collect(),
            gate_command: gate.into_iter().map(String::from).collect(),
            workspace_allowlist: vec![workspace.to_string()],
            timeout_secs: 60,
            naming_timeout_secs: crate::config::DEFAULT_NAMING_TIMEOUT_SECS,
            curation_timeout_secs: crate::config::DEFAULT_CURATION_TIMEOUT_SECS,
        }
    }

    fn req(brain: &str) -> RunRequest {
        RunRequest {
            runner_id: "build-1".to_string(),
            packet_markdown: "# packet\ndo the thing".to_string(),
            block_id: "sb_alpha".to_string(),
            brain_ref: "repo-a".to_string(),
            brain: Some(brain.to_string()),
            mission_id: None,
        }
    }

    fn opts(base: &Path, runtime: &Path) -> EngineOpts {
        EngineOpts {
            worktree_base: base.to_path_buf(),
            runtime_root: runtime.to_path_buf(),
        }
    }

    /// Init a real git repo with one commit (worktree add needs a HEAD).
    fn init_repo(dir: &Path) {
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .current_dir(dir)
                .args(args)
                .output()
                .expect("git");
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@t"]);
        run(&["config", "user.name", "t"]);
        std::fs::write(dir.join("README.md"), "seed").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "seed"]);
    }

    fn phases(letters: &[MissionLetter]) -> Vec<Phase> {
        letters.iter().map(|l| l.phase).collect()
    }

    /// The never-lands proof, applied to any recorded run: no letter is ever `landed`.
    fn assert_never_lands(letters: &[MissionLetter]) {
        assert!(
            letters.iter().all(|l| l.phase != Phase::Landed),
            "the runner daemon must NEVER emit a landed letter (§1d)"
        );
    }

    #[tokio::test]
    async fn non_git_workspace_fails_and_nothing_runs() {
        let base = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let ws = tempfile::tempdir().unwrap(); // NOT a git repo
        let owner = FakeOwner::default();
        let r = runner(
            vec!["sh", "-c", "echo RAN; cat {packet_file}"],
            vec!["true"],
            ws.path().to_str().unwrap(),
        );

        run_mission(
            &owner,
            &r,
            &req(ws.path().to_str().unwrap()),
            "msn_0123456789ab",
            &opts(base.path(), runtime.path()),
        )
        .await;

        let letters = owner.letters.lock().unwrap().clone();
        assert_never_lands(&letters);
        // judging then failed — NEVER executing (nothing ran outside a worktree).
        assert_eq!(phases(&letters), vec![Phase::Judging, Phase::Failed]);
        let failed = letters.last().unwrap();
        assert!(
            failed
                .verdict
                .as_ref()
                .unwrap()
                .gist
                .contains("not a git repo"),
            "the reason is carried honestly"
        );
        assert!(
            !letters.iter().any(|l| l.phase == Phase::Executing),
            "no executing letter"
        );
    }

    #[tokio::test]
    async fn happy_flow_green_gate_emits_merge_wait_with_complete_candidate() {
        let base = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let ws = tempfile::tempdir().unwrap();
        init_repo(ws.path());

        // A fake agent that only READS the packet (leaves the worktree clean) + a
        // gate with DETERMINISTIC output so we can assert the REAL log hash.
        let owner = FakeOwner {
            letters: Arc::new(Mutex::new(Vec::new())),
            scope: Some((3, 5)),
        };
        let r = runner(
            vec!["sh", "-c", "cat {packet_file} > /dev/null; echo done"],
            vec!["sh", "-c", "echo GATE_OK"],
            ws.path().to_str().unwrap(),
        );
        let mid = "msn_0123456789ab";
        run_mission(
            &owner,
            &r,
            &req(ws.path().to_str().unwrap()),
            mid,
            &opts(base.path(), runtime.path()),
        )
        .await;

        let letters = owner.letters.lock().unwrap().clone();
        assert_never_lands(&letters);
        assert_eq!(
            phases(&letters),
            vec![Phase::Judging, Phase::Executing, Phase::MergeWait]
        );

        // The chain is linked: each letter's prev is the prior's synthetic id.
        assert_eq!(letters[0].prev_letter_id, None, "seq-1 has a null prev");
        assert_eq!(letters[1].prev_letter_id.as_deref(), Some("id1"));
        assert_eq!(letters[2].prev_letter_id.as_deref(), Some("id2"));
        assert_eq!(letters[1].mission_seq, 2);
        assert_eq!(letters[2].mission_seq, 3);

        let mw = &letters[2];
        let gate = mw.gate.as_ref().expect("merge_wait carries a gate");
        assert_eq!(gate.exit_status, 0);
        // The REAL hash of the REAL gate log ("GATE_OK\n").
        let expect_hash = format!("sha256:{}", sha256_hex(b"GATE_OK\n"));
        assert_eq!(
            gate.artifact_hash, expect_hash,
            "the gate hash is the real log hash"
        );

        let cand = mw.receipt_candidate.as_ref().expect("a complete candidate");
        assert_eq!(cand.block_id, "sb_alpha");
        assert_eq!(
            cand.scope.boundary_version, 3,
            "scope from the fresh snapshot"
        );
        assert_eq!(cand.scope.contract_version, 5);
        assert_eq!(
            cand.evidence.artifact_hash, expect_hash,
            "evidence anchors the real hash"
        );
        assert!(
            !cand.evidence.evidence_refs.is_empty(),
            "evidence is pointable"
        );
        // Execution identity is COMPLETE — a `test` receipt is un-importable
        // without it (the bug this fixes); cwd is the honest "." of the isolated
        // worktree, never a host path (§1f).
        assert_eq!(
            cand.evidence.cwd.as_deref(),
            Some("."),
            "cwd present, no host path (§1f)"
        );
        assert!(cand.evidence.started_at.is_some(), "started_at present");
        assert!(cand.evidence.ended_at.is_some(), "ended_at present");
        assert!(!cand.evidence.artifact_hash.is_empty(), "hash present");
        assert!(
            !cand.evidence.started_at.as_deref().unwrap().contains('/'),
            "timestamp is not a path"
        );

        // The letter validates against the owner contract (would be accepted).
        assert!(
            m1nd_mcp::mission_letter::validate(mw).is_ok(),
            "the merge_wait is a valid letter"
        );

        // Side record holds the worktree path (never the letter).
        let rec = MissionLocalStore::get(runtime.path(), mid)
            .unwrap()
            .expect("side record");
        assert!(rec.worktree.as_deref().unwrap().contains(mid));

        // Clean worktree → removed (the packet was cleaned before the check).
        assert!(
            !base.path().join(mid).exists(),
            "a clean worktree is removed"
        );
    }

    #[tokio::test]
    async fn red_gate_emits_failed_with_the_gate_line() {
        let base = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let ws = tempfile::tempdir().unwrap();
        init_repo(ws.path());

        let owner = FakeOwner::default();
        let r = runner(
            vec!["sh", "-c", "cat {packet_file} > /dev/null"],
            vec!["sh", "-c", "echo BOOM >&2; exit 1"],
            ws.path().to_str().unwrap(),
        );
        run_mission(
            &owner,
            &r,
            &req(ws.path().to_str().unwrap()),
            "msn_0123456789ab",
            &opts(base.path(), runtime.path()),
        )
        .await;

        let letters = owner.letters.lock().unwrap().clone();
        assert_never_lands(&letters);
        assert_eq!(
            phases(&letters),
            vec![Phase::Judging, Phase::Executing, Phase::Failed]
        );
        let failed = letters.last().unwrap();
        assert_eq!(
            failed.gate.as_ref().unwrap().exit_status,
            1,
            "the gate line shows exit 1"
        );
        assert!(failed.verdict.as_ref().unwrap().gist.contains("exited 1"));
        assert!(
            m1nd_mcp::mission_letter::validate(failed).is_ok(),
            "a failed letter is valid"
        );
    }

    #[test]
    fn validate_run_pins_and_allowlists() {
        // The allowlist takes only paths the platform agrees are ABSOLUTE, and
        // `/allowed/repo` is not one on Windows — see `crate::config::abs_path`.
        // Substituted rather than `format!`ed in: the fixture carries a literal
        // `{packet_file}` token that a format string would try to expand.
        let allowed = crate::config::abs_path("/allowed/repo");
        let cfg = crate::config::parse(
            &r#"
[[runner]]
id = "build-1"
capability = "build-runner"
command = ["c", "{packet_file}"]
gate_command = ["t"]
workspace_allowlist = ["{allowed_root}"]
"#
            .replace("{allowed_root}", &allowed),
        )
        .unwrap();

        // Unpinned runner id → 403 unpinned_runner.
        let mut r = req(&allowed);
        r.runner_id = "ghost".to_string();
        let err = validate_run(&cfg, &r).expect_err("unpinned");
        assert_eq!(err.keyword(), "unpinned_runner");
        assert_eq!(err.status(), 403);

        // Workspace outside the allowlist → 403 workspace_not_allowed.
        let r = req(&crate::config::abs_path("/somewhere/else"));
        let err = validate_run(&cfg, &r).expect_err("outside allowlist");
        assert_eq!(err.keyword(), "workspace_not_allowed");

        // In-allowlist → the pinned runner.
        let r = req(&allowed);
        assert_eq!(validate_run(&cfg, &r).unwrap().id, "build-1");
    }

    #[test]
    fn mint_mission_id_is_well_formed() {
        let id = mint_mission_id();
        assert!(id.starts_with("msn_"));
        assert_eq!(id.len(), 16, "msn_ + 12 hex");
        assert!(id[4..].chars().all(|c| c.is_ascii_hexdigit()));
    }
}
