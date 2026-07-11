//! F12 — the `/curate` curation lane (HUMAN-VIEW-V2-F12-TECH §2). Mirror of `/name`.
//!
//! A SYNCHRONOUS propose call — deliberately NOT a mission: no worktree, no gate, no
//! mission letters, and the daemon NEVER writes a store. The hand-runner PROPOSES a
//! batch of `candidate_edit` ops AS DATA; the OWNER validates, sanitizes (o5) and
//! applies them itself (§3). The daemon resolves its PINNED `hand-runner` (capability
//! from `runners.toml`, never from announce), runs the pinned command ONCE with the
//! curation packet on stdin, under a per-mission timeout, and expects on stdout ONE
//! JSON document:
//!
//! ```json
//! { "schema": "m1nd-curation-proposal-v0", "ops": [ CandidateEditOp… ], "report": "…" }
//! ```
//!
//! Daemon-side hygiene (mirror of `/name`): the proposal is shape-validated HERE
//! before the wire — the `ops` are STRONG-parsed into the owner's typed
//! [`m1nd_mcp::candidate_edit::EditOp`], so a malformed op is an honest per-mission
//! failure, NEVER a partial apply. The owner re-parses + applies anyway (defense in
//! depth; the trust boundary is the LLM, not the loopback).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use m1nd_mcp::candidate_edit::EditOp;
use m1nd_mcp::mission_letter::Capability;
use m1nd_mcp::runnerd_owner::secret_matches;

use crate::config::{self, RunnerDef, RunnersConfig, PACKET_FILE_TOKEN};

/// The proposal schema tag the hand-runner MUST stamp on its stdout document (§2).
pub const CURATION_PROPOSAL_SCHEMA: &str = "m1nd-curation-proposal-v0";

/// The `POST /curate` request. `runner_id` is OPTIONAL (announce carries no
/// capability, §5a, so the owner cannot know which announced id is the hand one —
/// absent, the daemon resolves its first pinned hand-runner itself). `packet` is the
/// self-contained curation payload piped to the runner's stdin VERBATIM — opaque to
/// the daemon, exactly like `/name`'s per-block packet (it carries the embedded
/// instruction + skeleton id + OCC store_version + the owner-composed block views).
#[derive(Debug, Clone, Deserialize)]
pub struct CurateRequest {
    #[serde(default)]
    pub runner_id: Option<String>,
    pub packet: Value,
}

/// The honest `/curate` refusals (mirror of [`crate::naming::NameRefusal`], for the
/// hand capability). The secret 401 is handled by [`handle_curate_request`] directly
/// (bare, like `/run` and `/name`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurateRefusal {
    /// The named `runner_id` is not pinned in `runners.toml` → 403.
    UnpinnedRunner { runner_id: String },
    /// The named `runner_id` is pinned but is NOT a hand-runner → 403.
    NotAHandRunner { runner_id: String },
    /// No `runner_id` given and no hand-runner is pinned at all → 403.
    NoHandRunner,
}

impl CurateRefusal {
    pub fn status(&self) -> u16 {
        403
    }
    pub fn keyword(&self) -> &'static str {
        match self {
            CurateRefusal::UnpinnedRunner { .. } => "unpinned_runner",
            CurateRefusal::NotAHandRunner { .. } => "not_a_hand_runner",
            CurateRefusal::NoHandRunner => "no_hand_runner",
        }
    }
    pub fn detail(&self) -> String {
        match self {
            CurateRefusal::UnpinnedRunner { runner_id } => format!(
                "runner '{runner_id}' is not pinned in runners.toml — announce proves liveness, it never grants a capability (§5a)"
            ),
            CurateRefusal::NotAHandRunner { runner_id } => format!(
                "runner '{runner_id}' is pinned but is not a hand-runner — /curate only speaks to the hand-runner capability"
            ),
            CurateRefusal::NoHandRunner => {
                "no hand-runner is pinned in runners.toml — pin one (capability = \"hand-runner\") to serve /curate".to_string()
            }
        }
    }
}

/// Resolve the pinned hand-runner for a `/curate` call. A named id must be pinned AND
/// carry the hand capability; an absent id resolves to the FIRST pinned hand-runner
/// (config order — deterministic), or refuses honestly.
pub fn resolve_hand_runner<'a>(
    cfg: &'a RunnersConfig,
    runner_id: Option<&str>,
) -> Result<&'a RunnerDef, CurateRefusal> {
    match runner_id {
        Some(id) => {
            let runner = config::find(cfg, id).ok_or_else(|| CurateRefusal::UnpinnedRunner {
                runner_id: id.to_string(),
            })?;
            if runner.parsed_capability() != Capability::HandRunner {
                return Err(CurateRefusal::NotAHandRunner {
                    runner_id: id.to_string(),
                });
            }
            Ok(runner)
        }
        None => cfg
            .runners
            .iter()
            .find(|r| r.parsed_capability() == Capability::HandRunner)
            .ok_or(CurateRefusal::NoHandRunner),
    }
}

/// The outcome of one curation call — either a shape-validated proposal or an honest
/// whole-mission failure (never partial). `proposal` carries the ORIGINAL JSON
/// document (schema + ops + report); the owner re-parses + applies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurationRun {
    pub ok: bool,
    pub proposal: Option<Value>,
    pub error: Option<String>,
}

/// The whole `/curate` request handling, transport-free (testable without axum):
/// secret → resolve → run. Returns `(http_status, body)`:
/// - wrong/missing secret → `(401, {})` — bare, exactly like `/run` and `/name` (§5a);
/// - resolution refusal → `(403, {error, detail})` — the honest keyword;
/// - accepted → `(200, {runner_id, ok, proposal?, error?})` — the mission ran; `ok`
///   marks a shape-valid proposal, `ok:false` an honest whole-mission failure.
pub async fn handle_curate_request(
    cfg: &RunnersConfig,
    expected_secret: &str,
    provided_secret: &str,
    req: &CurateRequest,
    cwd: &Path,
) -> (u16, Value) {
    if !secret_matches(expected_secret, provided_secret) {
        return (401, json!({}));
    }
    let runner = match resolve_hand_runner(cfg, req.runner_id.as_deref()) {
        Ok(r) => r.clone(),
        Err(refusal) => {
            return (
                refusal.status(),
                json!({
                    "error": refusal.keyword(),
                    "detail": refusal.detail(),
                }),
            )
        }
    };
    let run = run_curation(&runner, &req.packet, cwd).await;
    (
        200,
        json!({
            "runner_id": runner.id,
            "ok": run.ok,
            "proposal": run.proposal,
            "error": run.error,
        }),
    )
}

/// Run the pinned hand-runner ONCE: pipe the packet (one JSON line) to its stdin,
/// wait under `curation_timeout_secs`, and parse+shape-validate the whole stdout as
/// ONE curation proposal. Every failure is an honest whole-mission failure — a
/// malformed proposal NEVER becomes a partial apply. When the pinned command carries
/// the `{packet_file}` token, the packet is ALSO written to a temp file and the token
/// substituted (both contract shapes work, mirroring `/name`).
pub async fn run_curation(runner: &RunnerDef, packet: &Value, cwd: &Path) -> CurationRun {
    let fail = |error: String| CurationRun {
        ok: false,
        proposal: None,
        error: Some(error),
    };

    let packet_line = match serde_json::to_string(packet) {
        Ok(s) => format!("{s}\n"),
        Err(e) => return fail(format!("packet does not serialize: {e}")),
    };

    // Optional {packet_file} support: write the packet to a temp file and splice.
    let uses_token = runner.command.iter().any(|a| a.contains(PACKET_FILE_TOKEN));
    let temp_packet: Option<PathBuf> = if uses_token {
        let path = std::env::temp_dir().join(format!("m1nd-curation-{}.json", std::process::id()));
        if let Err(e) = std::fs::write(&path, packet_line.as_bytes()) {
            return fail(format!("cannot write the packet temp file: {e}"));
        }
        Some(path)
    } else {
        None
    };
    let argv: Vec<String> = match &temp_packet {
        Some(path) => {
            let p = path.to_string_lossy().to_string();
            runner
                .command
                .iter()
                .map(|a| a.replace(PACKET_FILE_TOKEN, &p))
                .collect()
        }
        None => runner.command.clone(),
    };

    let outcome = run_curation_cmd(cwd, &argv, &packet_line, runner.curation_timeout_secs).await;
    if let Some(path) = temp_packet {
        let _ = std::fs::remove_file(path);
    }

    match outcome {
        CurationCmd::SpawnError(e) => fail(format!("hand-runner spawn failed: {e}")),
        CurationCmd::TimedOut => fail(format!(
            "hand-runner timed out after {}s — killed",
            runner.curation_timeout_secs
        )),
        CurationCmd::Exited {
            status,
            stdout,
            stderr,
        } => {
            if status != Some(0) {
                return fail(format!(
                    "hand-runner exited with status {}: {}",
                    status.map_or_else(|| "?".to_string(), |c| c.to_string()),
                    excerpt(&stderr)
                ));
            }
            match parse_curation_proposal(&stdout) {
                Ok(proposal) => CurationRun {
                    ok: true,
                    proposal: Some(proposal),
                    error: None,
                },
                Err(reason) => fail(reason),
            }
        }
    }
}

/// Parse + shape-validate the hand-runner's stdout as ONE curation proposal (§2). The
/// whole trimmed stdout must be a single JSON document with the right `schema` tag, a
/// non-empty `report`, and an `ops` array that STRONG-parses into typed
/// [`EditOp`] — any deviation is an honest whole-mission failure (never a partial
/// apply). Returns the ORIGINAL document on success (the owner re-parses + applies).
pub fn parse_curation_proposal(stdout: &str) -> Result<Value, String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Err("the hand-runner printed no output".to_string());
    }
    let doc: Value = serde_json::from_str(trimmed)
        .map_err(|e| format!("the curation proposal is not one JSON document: {e}"))?;

    let schema = doc
        .get("schema")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if schema != CURATION_PROPOSAL_SCHEMA {
        return Err(format!(
            "curation proposal schema mismatch: expected {CURATION_PROPOSAL_SCHEMA}, got '{schema}'"
        ));
    }

    match doc.get("report").and_then(|v| v.as_str()) {
        Some(r) if !r.trim().is_empty() => {}
        _ => return Err(
            "the curation proposal carries no non-empty `report` — the hand must say what it did"
                .to_string(),
        ),
    }

    let ops_value = doc.get("ops").cloned().unwrap_or(Value::Null);
    if !ops_value.is_array() {
        return Err("the curation proposal's `ops` must be a JSON array".to_string());
    }
    // STRONG parse: the ops must deserialize into the owner's typed candidate_edit
    // ops. A malformed op fails the WHOLE mission here — never a partial apply.
    let _typed: Vec<EditOp> = serde_json::from_value(ops_value).map_err(|e| {
        format!("the curation proposal's `ops` are not valid candidate_edit ops: {e}")
    })?;

    Ok(doc)
}

/// A short, single-line stderr excerpt for an honest failure (never the whole log on
/// the wire) — mirrors `/name`.
fn excerpt(stderr: &str) -> String {
    let line = stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let capped: String = line.chars().take(200).collect();
    if capped.is_empty() {
        "(no stderr)".to_string()
    } else {
        capped
    }
}

/// The outcome of one curation command run.
enum CurationCmd {
    SpawnError(String),
    TimedOut,
    Exited {
        status: Option<i32>,
        stdout: String,
        stderr: String,
    },
}

/// Run `argv` in `cwd` with `stdin_payload` piped in, under a wall-clock timeout.
/// stdout and stderr are captured SEPARATELY (the proposal is stdout-only);
/// `kill_on_drop` guarantees a timed-out child is killed. Mirror of `/name`'s heart.
async fn run_curation_cmd(
    cwd: &Path,
    argv: &[String],
    stdin_payload: &str,
    timeout_secs: u64,
) -> CurationCmd {
    if argv.is_empty() {
        return CurationCmd::SpawnError("empty command".to_string());
    }
    let mut cmd = tokio::process::Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return CurationCmd::SpawnError(e.to_string()),
    };
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let _ = stdin.write_all(stdin_payload.as_bytes()).await;
        drop(stdin);
    }
    match tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output()).await {
        Ok(Ok(out)) => CurationCmd::Exited {
            status: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        },
        Ok(Err(e)) => CurationCmd::SpawnError(format!("wait failed: {e}")),
        Err(_) => CurationCmd::TimedOut,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(id: &str, capability: &str, command: Vec<&str>) -> RunnerDef {
        RunnerDef {
            id: id.to_string(),
            capability: capability.to_string(),
            command: command.into_iter().map(String::from).collect(),
            gate_command: Vec::new(),
            workspace_allowlist: Vec::new(),
            timeout_secs: crate::config::DEFAULT_TIMEOUT_SECS,
            naming_timeout_secs: crate::config::DEFAULT_NAMING_TIMEOUT_SECS,
            // A short curation timeout keeps the hang test fast.
            curation_timeout_secs: 1,
        }
    }

    fn cfg_with(runners: Vec<RunnerDef>) -> RunnersConfig {
        RunnersConfig { runners }
    }

    fn packet() -> Value {
        json!({
            "instruction": "propose candidate_edit ops",
            "skeleton_id": "sk_demo",
            "store_version": 7,
            "blocks": [ { "block_id": "sb_a", "name": "A" } ],
        })
    }

    // --- resolution refusals (portable, no exec) -------------------------------

    #[test]
    fn resolve_hand_runner_refusals_and_default_pick() {
        let cfg = cfg_with(vec![
            def("namer-1", "naming-runner", vec!["namer"]),
            def("hand-1", "hand-runner", vec!["hand"]),
            def("hand-2", "hand-runner", vec!["hand2"]),
        ]);

        // A named non-hand runner is an honest refusal, never a silent downgrade.
        let err = resolve_hand_runner(&cfg, Some("namer-1")).expect_err("naming is not hand");
        assert_eq!(err.keyword(), "not_a_hand_runner");
        assert_eq!(err.status(), 403);

        // An unpinned id refuses with the pin law's keyword.
        let err = resolve_hand_runner(&cfg, Some("ghost")).expect_err("unpinned");
        assert_eq!(err.keyword(), "unpinned_runner");

        // Absent id → the FIRST pinned hand-runner (deterministic config order).
        let picked = resolve_hand_runner(&cfg, None).expect("resolves the pinned hand");
        assert_eq!(picked.id, "hand-1");

        // Named hand runner resolves to itself.
        let picked = resolve_hand_runner(&cfg, Some("hand-2")).expect("named hand");
        assert_eq!(picked.id, "hand-2");

        // No hand runner pinned at all → honest no_hand_runner.
        let only_namer = cfg_with(vec![def("namer-1", "naming-runner", vec!["namer"])]);
        let err = resolve_hand_runner(&only_namer, None).expect_err("nothing to resolve");
        assert_eq!(err.keyword(), "no_hand_runner");
    }

    // --- the transport-free /curate handling: 401 bare, 403 keyword ------------

    #[tokio::test]
    async fn handle_curate_refuses_wrong_secret_with_bare_401() {
        let cfg = cfg_with(vec![def("hand-1", "hand-runner", vec!["hand"])]);
        let req = CurateRequest {
            runner_id: None,
            packet: packet(),
        };
        let dir = std::env::temp_dir();
        let (status, body) = handle_curate_request(&cfg, "right", "wrong", &req, &dir).await;
        assert_eq!(status, 401);
        assert_eq!(body, json!({}), "the 401 is bare (§5a)");
        let (status, _) = handle_curate_request(&cfg, "right", "", &req, &dir).await;
        assert_eq!(status, 401, "a missing secret is the same bare 401");
    }

    #[tokio::test]
    async fn handle_curate_refuses_non_hand_runner_honestly() {
        let cfg = cfg_with(vec![def("namer-1", "naming-runner", vec!["namer"])]);
        let req = CurateRequest {
            runner_id: Some("namer-1".to_string()),
            packet: packet(),
        };
        let dir = std::env::temp_dir();
        let (status, body) = handle_curate_request(&cfg, "s", "s", &req, &dir).await;
        assert_eq!(status, 403);
        assert_eq!(body["error"], "not_a_hand_runner");
        assert!(
            body["detail"].as_str().unwrap_or("").contains("namer-1"),
            "the refusal names the runner: {body}"
        );
    }

    // --- proposal shape validation (pure, no exec) -----------------------------

    #[test]
    fn parse_curation_proposal_accepts_a_valid_document_and_strong_parses_ops() {
        let doc = r#"{
            "schema": "m1nd-curation-proposal-v0",
            "ops": [
                {"op":"rename","block_id":"sb_a","name":"Auth","purpose":"Owns login."},
                {"op":"merge","into":"sb_a","block_ids":["sb_b"]}
            ],
            "report": "Named the auth block and merged the thin one."
        }"#;
        let parsed = parse_curation_proposal(doc).expect("a valid proposal parses");
        assert_eq!(parsed["schema"], CURATION_PROPOSAL_SCHEMA);
        assert_eq!(parsed["ops"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn parse_curation_proposal_rejects_every_malformed_class() {
        // Not JSON at all.
        assert!(parse_curation_proposal("not json")
            .unwrap_err()
            .contains("not one JSON document"));
        // Empty output.
        assert!(parse_curation_proposal("   ")
            .unwrap_err()
            .contains("printed no output"));
        // Wrong schema tag.
        let bad_schema = r#"{"schema":"wrong","ops":[],"report":"x"}"#;
        assert!(parse_curation_proposal(bad_schema)
            .unwrap_err()
            .contains("schema mismatch"));
        // Missing/empty report.
        let no_report = r#"{"schema":"m1nd-curation-proposal-v0","ops":[]}"#;
        assert!(parse_curation_proposal(no_report)
            .unwrap_err()
            .contains("report"));
        // ops not an array.
        let ops_obj = r#"{"schema":"m1nd-curation-proposal-v0","ops":{},"report":"x"}"#;
        assert!(parse_curation_proposal(ops_obj)
            .unwrap_err()
            .contains("must be a JSON array"));
        // An op with an unknown `op` tag fails the STRONG parse (whole mission).
        let bad_op = r#"{"schema":"m1nd-curation-proposal-v0","ops":[{"op":"nuke","block_id":"x"}],"report":"x"}"#;
        assert!(parse_curation_proposal(bad_op)
            .unwrap_err()
            .contains("not valid candidate_edit ops"));
    }

    // --- the curation engine against a canned script runner (never a real LLM) -

    #[cfg(unix)]
    fn sh_runner(script: &str) -> RunnerDef {
        def("hand-sh", "hand-runner", vec!["/bin/sh", "-c", script])
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_curation_returns_a_valid_proposal() {
        // The canned hand: read the packet line, emit a valid proposal document.
        let runner = sh_runner(
            r#"read line; printf '%s\n' '{"schema":"m1nd-curation-proposal-v0","ops":[{"op":"rename","block_id":"sb_a","name":"Auth","purpose":"Owns login."}],"report":"named one block"}'"#,
        );
        let run = run_curation(&runner, &packet(), &std::env::temp_dir()).await;
        assert!(run.ok, "a valid proposal is ok: {run:?}");
        let proposal = run.proposal.expect("carries the proposal");
        assert_eq!(proposal["ops"].as_array().unwrap().len(), 1);
        assert_eq!(proposal["report"], "named one block");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_curation_malformed_output_is_an_honest_whole_mission_failure() {
        let runner = sh_runner(r#"read line; echo "this is not a proposal""#);
        let run = run_curation(&runner, &packet(), &std::env::temp_dir()).await;
        assert!(!run.ok, "malformed output fails honestly");
        assert!(
            run.error.as_deref().unwrap_or("").contains("JSON document"),
            "honest parse error: {run:?}"
        );
        assert!(run.proposal.is_none(), "no partial proposal on failure");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_curation_hostile_op_dies_at_the_strong_parse() {
        // A well-formed document whose ops carry an unknown verb: the STRONG parse
        // kills the WHOLE mission daemon-side (never a partial apply).
        let runner = sh_runner(
            r#"read line; printf '%s\n' '{"schema":"m1nd-curation-proposal-v0","ops":[{"op":"delete_everything"}],"report":"tried"}'"#,
        );
        let run = run_curation(&runner, &packet(), &std::env::temp_dir()).await;
        assert!(!run.ok);
        assert!(run
            .error
            .as_deref()
            .unwrap_or("")
            .contains("not valid candidate_edit ops"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_curation_times_out_honestly() {
        let runner = sh_runner("sleep 3"); // curation_timeout_secs = 1 (from def)
        let run = run_curation(&runner, &packet(), &std::env::temp_dir()).await;
        assert!(!run.ok);
        assert!(run.error.as_deref().unwrap_or("").contains("timed out"));
    }

    #[tokio::test]
    async fn run_curation_spawn_failure_is_honest() {
        let runner = def(
            "hand-ghost",
            "hand-runner",
            vec!["m1nd-no-such-hand-binary-xyz"],
        );
        let run = run_curation(&runner, &packet(), &std::env::temp_dir()).await;
        assert!(!run.ok);
        assert!(run.error.as_deref().unwrap_or("").contains("spawn failed"));
    }
}
