//! F11-b — the `/name` naming lane (HUMAN-VIEW-V2-F11-TECH §2a).
//!
//! A SYNCHRONOUS per-block naming call — deliberately NOT a mission: no worktree,
//! no gate, no mission letters, and the daemon NEVER writes a store (naming is a
//! response the owner sanitizes and applies; the store write stays owner-side).
//! The daemon resolves its PINNED naming-runner (capability from `runners.toml`,
//! never from announce), runs the pinned command once per block with the naming
//! packet on stdin, bounded per-block timeout, bounded parallelism, and answers
//! per block `{block_id, ok, name?, purpose?, error?}` — partial is normal.
//!
//! Output hygiene: every runner line is parsed + sanitized HERE via the owner's
//! own [`m1nd_mcp::naming_runner::parse_and_sanitize_line`] (o5) before it rides
//! the wire — and the owner re-sanitizes on receipt anyway (defense in depth).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use m1nd_mcp::mission_letter::Capability;
use m1nd_mcp::naming_runner::NameCallResult;
use m1nd_mcp::runnerd_owner::secret_matches;

use crate::config::{self, RunnerDef, RunnersConfig, PACKET_FILE_TOKEN};

/// How many naming commands run concurrently (§2a "batch-parallel", bounded).
pub const NAMING_PARALLELISM: usize = 4;

/// The `POST /name` request. `runner_id` is OPTIONAL: announce carries no
/// capability (§5a), so the owner cannot know which announced id is the naming
/// one — absent, the daemon resolves its first pinned naming-runner itself.
#[derive(Debug, Clone, Deserialize)]
pub struct NameRequest {
    #[serde(default)]
    pub runner_id: Option<String>,
    #[serde(default)]
    pub blocks: Vec<NameBlock>,
}

/// One block to name: its id and its naming packet (member paths + dominant kinds
/// + top symbols — opaque to the daemon, piped to the runner's stdin verbatim).
#[derive(Debug, Clone, Deserialize)]
pub struct NameBlock {
    pub block_id: String,
    pub packet: Value,
}

/// The honest `/name` refusals (mirrors [`crate::mission::RunRefusal`]'s shape).
/// The secret 401 is handled by [`handle_name_request`] directly (bare, like /run).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameRefusal {
    /// The named `runner_id` is not pinned in `runners.toml` → 403.
    UnpinnedRunner { runner_id: String },
    /// The named `runner_id` is pinned but is NOT a naming-runner → 403.
    NotANamingRunner { runner_id: String },
    /// No `runner_id` given and no naming-runner is pinned at all → 403.
    NoNamingRunner,
}

impl NameRefusal {
    pub fn status(&self) -> u16 {
        403
    }
    pub fn keyword(&self) -> &'static str {
        match self {
            NameRefusal::UnpinnedRunner { .. } => "unpinned_runner",
            NameRefusal::NotANamingRunner { .. } => "not_a_naming_runner",
            NameRefusal::NoNamingRunner => "no_naming_runner",
        }
    }
    pub fn detail(&self) -> String {
        match self {
            NameRefusal::UnpinnedRunner { runner_id } => format!(
                "runner '{runner_id}' is not pinned in runners.toml — announce proves liveness, it never grants a capability (§5a)"
            ),
            NameRefusal::NotANamingRunner { runner_id } => format!(
                "runner '{runner_id}' is pinned but is not a naming-runner — /name only speaks to the naming capability"
            ),
            NameRefusal::NoNamingRunner => {
                "no naming-runner is pinned in runners.toml — pin one (capability = \"naming-runner\") to serve /name".to_string()
            }
        }
    }
}

/// Resolve the pinned naming-runner for a `/name` call. A named id must be pinned
/// AND carry the naming capability; an absent id resolves to the FIRST pinned
/// naming-runner (config order — deterministic), or refuses honestly.
pub fn resolve_naming_runner<'a>(
    cfg: &'a RunnersConfig,
    runner_id: Option<&str>,
) -> Result<&'a RunnerDef, NameRefusal> {
    match runner_id {
        Some(id) => {
            let runner = config::find(cfg, id).ok_or_else(|| NameRefusal::UnpinnedRunner {
                runner_id: id.to_string(),
            })?;
            if runner.parsed_capability() != Capability::NamingRunner {
                return Err(NameRefusal::NotANamingRunner {
                    runner_id: id.to_string(),
                });
            }
            Ok(runner)
        }
        None => cfg
            .runners
            .iter()
            .find(|r| r.parsed_capability() == Capability::NamingRunner)
            .ok_or(NameRefusal::NoNamingRunner),
    }
}

/// The whole `/name` request handling, transport-free (testable without axum):
/// secret → resolve → run. Returns `(http_status, body)`:
/// - wrong/missing secret → `(401, {})` — bare, exactly like `/run` (§5a);
/// - resolution refusal → `(403, {error, detail})` — the honest keyword;
/// - accepted → `(200, {runner_id, results})` — per-block outcomes, partial normal.
pub async fn handle_name_request(
    cfg: &RunnersConfig,
    expected_secret: &str,
    provided_secret: &str,
    req: &NameRequest,
    cwd: &Path,
) -> (u16, Value) {
    if !secret_matches(expected_secret, provided_secret) {
        return (401, json!({}));
    }
    let runner = match resolve_naming_runner(cfg, req.runner_id.as_deref()) {
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
    let results = run_naming(&runner, &req.blocks, cwd).await;
    (
        200,
        json!({
            "runner_id": runner.id,
            "results": results,
        }),
    )
}

/// Run the pinned naming command once per block — bounded parallelism
/// ([`NAMING_PARALLELISM`]), per-block timeout (`naming_timeout_secs`), per-block
/// honest errors. One bad block never poisons its batch (§2a).
pub async fn run_naming(
    runner: &RunnerDef,
    blocks: &[NameBlock],
    cwd: &Path,
) -> Vec<NameCallResult> {
    let mut results = Vec::with_capacity(blocks.len());
    for chunk in blocks.chunks(NAMING_PARALLELISM) {
        let mut handles = Vec::with_capacity(chunk.len());
        for block in chunk {
            let runner = runner.clone();
            let block = block.clone();
            let cwd = cwd.to_path_buf();
            handles.push(tokio::spawn(async move {
                name_one_block(&runner, &block, &cwd).await
            }));
        }
        for (handle, block) in handles.into_iter().zip(chunk) {
            results.push(handle.await.unwrap_or_else(|e| NameCallResult {
                block_id: block.block_id.clone(),
                ok: false,
                name: None,
                purpose: None,
                error: Some(format!("naming task panicked: {e}")),
            }));
        }
    }
    results
}

/// Name ONE block: pipe the packet (one JSON line) to the pinned command's stdin,
/// wait under the per-block timeout, take the LAST non-empty stdout line, and
/// parse + sanitize it (o5). Every failure is an honest per-block error. When the
/// pinned command carries the `{packet_file}` token, the packet is ALSO written to
/// a temp file and the token substituted (both contract shapes work).
async fn name_one_block(runner: &RunnerDef, block: &NameBlock, cwd: &Path) -> NameCallResult {
    let fail = |error: String| NameCallResult {
        block_id: block.block_id.clone(),
        ok: false,
        name: None,
        purpose: None,
        error: Some(error),
    };

    let packet_line = match serde_json::to_string(&block.packet) {
        Ok(s) => format!("{s}\n"),
        Err(e) => return fail(format!("packet does not serialize: {e}")),
    };

    // Optional {packet_file} support: write the packet to a temp file and splice.
    let uses_token = runner.command.iter().any(|a| a.contains(PACKET_FILE_TOKEN));
    let temp_packet: Option<PathBuf> = if uses_token {
        let safe_id: String = block
            .block_id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let path =
            std::env::temp_dir().join(format!("m1nd-naming-{}-{safe_id}.json", std::process::id()));
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

    let outcome = run_naming_cmd(cwd, &argv, &packet_line, runner.naming_timeout_secs).await;
    if let Some(path) = temp_packet {
        let _ = std::fs::remove_file(path);
    }

    match outcome {
        NamingCmd::SpawnError(e) => fail(format!("naming runner spawn failed: {e}")),
        NamingCmd::TimedOut => fail(format!(
            "naming runner timed out after {}s — killed",
            runner.naming_timeout_secs
        )),
        NamingCmd::Exited {
            status,
            stdout,
            stderr,
        } => {
            if status != Some(0) {
                return fail(format!(
                    "naming runner exited with status {}: {}",
                    status.map_or_else(|| "?".to_string(), |c| c.to_string()),
                    excerpt(&stderr)
                ));
            }
            let Some(line) = stdout.lines().rev().find(|l| !l.trim().is_empty()) else {
                return fail("naming runner printed no output".to_string());
            };
            match m1nd_mcp::naming_runner::parse_and_sanitize_line(line) {
                Ok(clean) => NameCallResult {
                    block_id: block.block_id.clone(),
                    ok: true,
                    name: Some(clean.name),
                    purpose: Some(clean.purpose),
                    error: None,
                },
                Err(reason) => fail(reason),
            }
        }
    }
}

/// A short, single-line stderr excerpt for an honest per-block error (never the
/// whole log on the wire).
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

/// The outcome of one naming command run.
enum NamingCmd {
    SpawnError(String),
    TimedOut,
    Exited {
        status: Option<i32>,
        stdout: String,
        stderr: String,
    },
}

/// Run `argv` in `cwd` with `stdin_payload` piped in, under a wall-clock timeout.
/// stdout and stderr are captured SEPARATELY (the naming line is stdout-only);
/// `kill_on_drop` guarantees a timed-out child is killed.
async fn run_naming_cmd(
    cwd: &Path,
    argv: &[String],
    stdin_payload: &str,
    timeout_secs: u64,
) -> NamingCmd {
    if argv.is_empty() {
        return NamingCmd::SpawnError("empty command".to_string());
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
        Err(e) => return NamingCmd::SpawnError(e.to_string()),
    };
    // Write the packet line and CLOSE stdin (drop) so a well-behaved runner sees
    // EOF; a write failure is reported by the runner's own exit/parse outcome.
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let _ = stdin.write_all(stdin_payload.as_bytes()).await;
        drop(stdin);
    }
    match tokio::time::timeout(Duration::from_secs(timeout_secs), child.wait_with_output()).await {
        Ok(Ok(out)) => NamingCmd::Exited {
            status: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        },
        Ok(Err(e)) => NamingCmd::SpawnError(format!("wait failed: {e}")),
        Err(_) => NamingCmd::TimedOut,
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
            naming_timeout_secs: 1,
        }
    }

    fn cfg_with(runners: Vec<RunnerDef>) -> RunnersConfig {
        RunnersConfig { runners }
    }

    fn block(id: &str) -> NameBlock {
        NameBlock {
            block_id: id.to_string(),
            packet: serde_json::json!({
                "block_id": id,
                "member_paths": ["src/a.rs"],
                "dominant_kinds": ["functions"],
            }),
        }
    }

    // --- resolution refusals (portable, no exec) -------------------------------

    #[test]
    fn resolve_naming_runner_refusals_and_default_pick() {
        let cfg = cfg_with(vec![
            def("build-1", "build-runner", vec!["agent", "{packet_file}"]),
            def("namer-1", "naming-runner", vec!["namer"]),
            def("namer-2", "naming-runner", vec!["namer2"]),
        ]);

        // A named non-naming runner is an honest refusal, never a silent downgrade.
        let err = resolve_naming_runner(&cfg, Some("build-1")).expect_err("build is not naming");
        assert_eq!(err.keyword(), "not_a_naming_runner");
        assert_eq!(err.status(), 403);

        // An unpinned id refuses with the pin law's keyword.
        let err = resolve_naming_runner(&cfg, Some("ghost")).expect_err("unpinned");
        assert_eq!(err.keyword(), "unpinned_runner");

        // Absent id → the FIRST pinned naming-runner (deterministic config order).
        let picked = resolve_naming_runner(&cfg, None).expect("resolves the pinned namer");
        assert_eq!(picked.id, "namer-1");

        // Named naming runner resolves to itself.
        let picked = resolve_naming_runner(&cfg, Some("namer-2")).expect("named namer");
        assert_eq!(picked.id, "namer-2");

        // No naming runner pinned at all → honest no_naming_runner.
        let only_build = cfg_with(vec![def(
            "build-1",
            "build-runner",
            vec!["agent", "{packet_file}"],
        )]);
        let err = resolve_naming_runner(&only_build, None).expect_err("nothing to resolve");
        assert_eq!(err.keyword(), "no_naming_runner");
    }

    // --- the transport-free /name handling: 401 bare, 403 keyword --------------

    #[tokio::test]
    async fn handle_name_request_refuses_wrong_secret_with_bare_401() {
        let cfg = cfg_with(vec![def("namer-1", "naming-runner", vec!["namer"])]);
        let req = NameRequest {
            runner_id: None,
            blocks: vec![block("sb_a")],
        };
        let dir = std::env::temp_dir();
        let (status, body) = handle_name_request(&cfg, "right", "wrong", &req, &dir).await;
        assert_eq!(status, 401);
        assert_eq!(body, serde_json::json!({}), "the 401 is bare (§5a)");
        let (status, _) = handle_name_request(&cfg, "right", "", &req, &dir).await;
        assert_eq!(status, 401, "a missing secret is the same bare 401");
    }

    #[tokio::test]
    async fn handle_name_request_refuses_non_naming_runner_honestly() {
        let cfg = cfg_with(vec![def(
            "build-1",
            "build-runner",
            vec!["agent", "{packet_file}"],
        )]);
        let req = NameRequest {
            runner_id: Some("build-1".to_string()),
            blocks: vec![block("sb_a")],
        };
        let dir = std::env::temp_dir();
        let (status, body) = handle_name_request(&cfg, "s", "s", &req, &dir).await;
        assert_eq!(status, 403);
        assert_eq!(body["error"], "not_a_naming_runner");
        assert!(
            body["detail"].as_str().unwrap_or("").contains("build-1"),
            "the refusal names the runner: {body}"
        );
    }

    // --- the naming engine against a canned script runner (never a real LLM) ---

    /// The canned fake runner: reads the packet line from stdin and answers per
    /// block_id — a clean JSON line, a hang past the timeout, or garbage.
    #[cfg(unix)]
    fn sh_runner(script: &str) -> RunnerDef {
        def("namer-sh", "naming-runner", vec!["/bin/sh", "-c", script])
    }

    #[cfg(unix)]
    const CANNED: &str = r#"read line
case "$line" in
  *sb_ok*)   printf '%s\n' '{"name":"Clean Name","purpose":"A clean one-line purpose."}' ;;
  *sb_slow*) sleep 3 ;;
  *)         echo "this is not a naming json line" ;;
esac"#;

    #[cfg(unix)]
    #[tokio::test]
    async fn run_naming_partial_ok_timeout_and_garbage_fall_per_block() {
        let runner = sh_runner(CANNED); // naming_timeout_secs = 1 (from def)
        let blocks = vec![block("sb_ok"), block("sb_slow"), block("sb_garbage")];
        let results = run_naming(&runner, &blocks, &std::env::temp_dir()).await;
        assert_eq!(results.len(), 3);

        let ok = &results[0];
        assert!(ok.ok, "the clean block names: {ok:?}");
        assert_eq!(ok.name.as_deref(), Some("Clean Name"));
        assert_eq!(ok.purpose.as_deref(), Some("A clean one-line purpose."));

        let slow = &results[1];
        assert!(!slow.ok, "the hung block times out per block");
        assert!(
            slow.error.as_deref().unwrap_or("").contains("timed out"),
            "honest timeout error: {slow:?}"
        );

        let garbage = &results[2];
        assert!(!garbage.ok, "garbage output falls back honestly");
        assert!(
            garbage
                .error
                .as_deref()
                .unwrap_or("")
                .contains("invalid naming JSON line"),
            "honest parse error: {garbage:?}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_naming_hostile_output_is_sanitized_daemon_side() {
        // The runner answers valid JSON with hostile content — the o5 sanitizer
        // refuses it BEFORE it rides the wire.
        let runner =
            sh_runner(r#"read line; printf '%s\n' '{"name":"<script>x</script>","purpose":"p"}'"#);
        let results = run_naming(&runner, &[block("sb_a")], &std::env::temp_dir()).await;
        assert!(!results[0].ok);
        assert!(
            results[0].error.as_deref().unwrap_or("").contains("HTML"),
            "the sanitizer's class surfaces: {:?}",
            results[0]
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn run_naming_substitutes_the_packet_file_token_when_pinned() {
        // A pinned command carrying {packet_file} gets the packet as a real file
        // too (both contract shapes work) — this runner reads the FILE, not stdin.
        let runner = def(
            "namer-file",
            "naming-runner",
            vec![
                "/bin/sh",
                "-c",
                r#"grep -q sb_file "{packet_file}" && printf '%s\n' '{"name":"From File","purpose":"Read the packet file."}'"#,
            ],
        );
        let results = run_naming(&runner, &[block("sb_file")], &std::env::temp_dir()).await;
        assert!(results[0].ok, "token-shaped runner works: {:?}", results[0]);
        assert_eq!(results[0].name.as_deref(), Some("From File"));
    }

    #[tokio::test]
    async fn run_naming_spawn_failure_is_an_honest_per_block_error() {
        // A nonexistent binary — portable (no shell involved).
        let runner = def(
            "namer-ghost",
            "naming-runner",
            vec!["m1nd-no-such-naming-binary-xyz"],
        );
        let results = run_naming(&runner, &[block("sb_a")], &std::env::temp_dir()).await;
        assert!(!results[0].ok);
        assert!(
            results[0]
                .error
                .as_deref()
                .unwrap_or("")
                .contains("spawn failed"),
            "honest spawn error: {:?}",
            results[0]
        );
    }
}
