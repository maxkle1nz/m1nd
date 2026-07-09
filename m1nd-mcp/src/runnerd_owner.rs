//! HUMAN VIEW v2 — F2.5c: the OWNER's runnerd surface (§5a announce/liveness, §4b
//! spawn proxy). The runner daemon ([`m1nd-runnerd`]) is the ONLY spawner (§5d);
//! this module is the owner's small counterpart:
//!
//! - the in-memory LIVENESS registry fed by `POST /api/runnerd/announce` (§5a) and
//!   read at `GET /api/runnerd/status`;
//! - the `mission_spawn` PROXY (§4b) that forwards a compose's spawn request
//!   owner→runnerd, keeping the shared secret OWNER-SIDE so the browser never sees
//!   it (the amendment's signed decision: the browser has no secret, so the spawn
//!   travels through the owner).
//!
//! ## The laws this module keeps (§5a, §5d)
//!
//! - **Announce proves LIVENESS ONLY.** It can NEVER create or widen a capability —
//!   capabilities are pinned in the runner daemon's OWN local `runners.toml`
//!   (owner-side pins, §5a), never here. This registry maps `runner_id → (port,
//!   last_seen)`; it holds no capability and grants none.
//! - **The shared secret authenticates announce + signs the proxy.** The runner
//!   daemon creates `<runtime_root>/runnerd.secret` (`0600`) on first boot; the
//!   owner READS the same file. An announce or a proxy without the matching secret
//!   is refused (§5a loopback + shared local secret; the same-UID threat is
//!   declared out of scope, §5d).
//! - **The owner NEVER spawns.** `mission_spawn` is a thin HTTP forward to the
//!   runner daemon's `/run`; the owner starts no worktree and no process itself.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use m1nd_core::error::M1ndError;

/// The shared-secret file the runner daemon creates (`0600`) in the runtime root
/// and the owner READS to authenticate announce + sign the spawn proxy (§5a).
pub const RUNNERD_SECRET_FILE: &str = "runnerd.secret";

/// The HTTP header carrying the shared secret on every runnerd request (§5a).
pub const RUNNERD_SECRET_HEADER: &str = "x-runnerd-secret";

/// One registered runner's LIVENESS (§5a) — the loopback port it serves and when
/// it last announced. Never a capability: the pins live in the runner's own config.
#[derive(Debug, Clone, Serialize)]
pub struct RunnerEntry {
    pub port: u16,
    pub last_seen_ms: u64,
}

/// The owner's in-memory runnerd liveness registry (§5a) — process-global owner
/// state (it lives on `AppState` beside `mcp_sessions`), keyed by `runner_id`.
/// Announce writes it, status reads it, the spawn proxy resolves a runner's port
/// from it. It is LIVENESS, never authority: an entry proves a daemon is up, not
/// that a capability exists.
#[derive(Debug, Default)]
pub struct RunnerdRegistry {
    runners: Mutex<BTreeMap<String, RunnerEntry>>,
}

impl RunnerdRegistry {
    /// Register (or refresh) the liveness of every `runner_id` a daemon announces
    /// at `port`, stamping `now_ms`. Re-announce updates the port + last_seen.
    pub fn register(&self, runner_ids: &[String], port: u16, now_ms: u64) {
        let mut g = self.runners.lock();
        for id in runner_ids {
            g.insert(
                id.clone(),
                RunnerEntry {
                    port,
                    last_seen_ms: now_ms,
                },
            );
        }
    }

    /// The loopback port a live `runner_id` serves, or `None` when no daemon has
    /// announced it (the spawn proxy refuses that honestly).
    pub fn port_for(&self, runner_id: &str) -> Option<u16> {
        self.runners.lock().get(runner_id).map(|e| e.port)
    }

    /// The `GET /api/runnerd/status` body (§5a read): every live runner with its
    /// port + last_seen, deterministically ordered (BTreeMap). The UI reads this to
    /// UN-disable the spawn radio and list the pinned-live runner ids.
    pub fn status_json(&self) -> Value {
        let g = self.runners.lock();
        let runners: Vec<Value> = g
            .iter()
            .map(|(id, e)| {
                json!({
                    "runner_id": id,
                    "port": e.port,
                    "last_seen_ms": e.last_seen_ms,
                })
            })
            .collect();
        json!({
            "runners": runners,
            "count": g.len(),
        })
    }
}

/// The path of the shared secret file inside an owner runtime root.
pub fn secret_path(runtime_root: &Path) -> PathBuf {
    runtime_root.join(RUNNERD_SECRET_FILE)
}

/// Read the shared runnerd secret from the runtime root (§5a). `None` when absent
/// or empty — no daemon has booted here, so announce/proxy cannot be authenticated.
pub fn read_secret(runtime_root: &Path) -> Option<String> {
    std::fs::read_to_string(secret_path(runtime_root))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Constant-time-ish equality for the shared secret. The secrets are short random
/// hex of equal length by construction; a length check plus a byte fold avoids the
/// trivial early-return timing tell (the declared threat model is the network +
/// other users, not a same-UID timing attacker — §5d — so this is belt, not moat).
pub fn secret_matches(expected: &str, got: &str) -> bool {
    let a = expected.as_bytes();
    let b = got.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// ===========================================================================
// Announce (§5a) — liveness only.
// ===========================================================================

/// The `POST /api/runnerd/announce` body (§5a): the runner ids a booting daemon
/// serves, the port it serves them on, and a per-boot challenge the owner echoes
/// back (a liveness round-trip). Announce NEVER carries a capability.
#[derive(Debug, Clone, Deserialize)]
pub struct AnnounceInput {
    pub runner_ids: Vec<String>,
    pub port: u16,
    pub boot_challenge: String,
}

/// The outcome of authenticating + applying an announce. `Unauthorized` maps to a
/// 401 with NO detailed body (§5a: refuse without leaking why); `Registered` maps
/// to a 200 echoing the challenge.
#[derive(Debug)]
pub enum AnnounceOutcome {
    /// No secret on disk, or a mismatch → 401, bare.
    Unauthorized,
    /// Registered; carries the echo body.
    Registered(Value),
}

/// Authenticate + apply an announce (§5a). `provided_secret` is the request's
/// `x-runnerd-secret` header; it must equal the on-disk `<runtime_root>/runnerd.secret`.
/// On success every `runner_id` is registered live at `input.port` and the
/// `boot_challenge` is echoed (the liveness proof). Pure but for the disk read of
/// the secret + the registry write — testable with a temp runtime root.
pub fn apply_announce(
    registry: &RunnerdRegistry,
    runtime_root: &Path,
    provided_secret: Option<&str>,
    input: &AnnounceInput,
    now_ms: u64,
) -> AnnounceOutcome {
    let Some(expected) = read_secret(runtime_root) else {
        return AnnounceOutcome::Unauthorized;
    };
    let ok = provided_secret
        .map(|got| secret_matches(&expected, got))
        .unwrap_or(false);
    if !ok {
        return AnnounceOutcome::Unauthorized;
    }
    registry.register(&input.runner_ids, input.port, now_ms);
    AnnounceOutcome::Registered(json!({
        "ok": true,
        // The liveness echo (§5a per-boot challenge): the daemon proves the owner
        // it reached is the real one by seeing its own challenge returned.
        "echo": input.boot_challenge,
        "registered": input.runner_ids,
    }))
}

// ===========================================================================
// mission_spawn proxy (§4b) — owner→runnerd, the browser never sees the secret.
// ===========================================================================

/// The `mission_spawn` verb input (§4b). Deliberately carries NO secret and NO
/// port — the owner resolves both. `brain_ref` is the letter's reference string
/// (§1f); the workspace/routing project_root rides the `?brain=` selector and is
/// resolved by the handler, not trusted from the browser.
#[derive(Debug, Clone, Deserialize)]
pub struct SpawnInput {
    pub runner_id: String,
    pub packet_markdown: String,
    pub block_id: String,
    pub brain_ref: String,
}

/// A resolved runnerd request — everything the owner needs to POST `/run` on the
/// daemon (§4b). The secret lives here (owner-side), never on the wire to the browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnTarget {
    pub url: String,
    pub secret: String,
    pub body: Value,
}

/// Resolve a `mission_spawn` into the concrete runnerd `/run` request (§4b) — the
/// PURE heart of the proxy, testable without a network:
///
/// 1. the `runner_id` must be LIVE (a daemon announced it) — else an honest refusal
///    naming the missing daemon;
/// 2. the shared secret must be on disk (a daemon booted here) — else honest refusal;
/// 3. the workspace project_root must resolve (the `?brain=` target has a code root)
///    — a memory-only brain cannot be spawned into (§5b needs a git worktree).
///
/// The built body forwards exactly the fields the daemon's `/run` speaks; the owner
/// adds the resolved `brain` (the workspace + the `?brain=` routing the daemon
/// echoes when it posts letters). The secret is returned SEPARATELY (it becomes the
/// `x-runnerd-secret` header), never placed in the body.
pub fn resolve_spawn_target(
    registry: &RunnerdRegistry,
    runtime_root: &Path,
    input: &SpawnInput,
    workspace_root: Option<&str>,
) -> Result<SpawnTarget, M1ndError> {
    let deny = |detail: String| M1ndError::InvalidParams {
        tool: "mission_spawn".to_string(),
        detail,
    };

    let port = registry.port_for(&input.runner_id).ok_or_else(|| {
        deny(format!(
            "no live runner '{}' — no runner daemon connected (start m1nd-runnerd, or the runner is not pinned/announced)",
            input.runner_id
        ))
    })?;

    let secret = read_secret(runtime_root).ok_or_else(|| {
        deny("no runnerd.secret in the runtime root — no runner daemon has booted here".to_string())
    })?;

    let workspace = workspace_root
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            deny(
                "no workspace root for this brain — spawn needs a code repo (a memory-only brain has no git worktree to run in)"
                    .to_string(),
            )
        })?;

    Ok(SpawnTarget {
        url: format!("http://127.0.0.1:{port}/run"),
        secret,
        body: json!({
            "runner_id": input.runner_id,
            "packet_markdown": input.packet_markdown,
            "block_id": input.block_id,
            "brain_ref": input.brain_ref,
            // The workspace + the `?brain=` routing the daemon echoes when it posts
            // letters (§5b: "o runnerd é cliente do owner, com ?brain= do alvo").
            "brain": workspace,
        }),
    })
}

/// Map the runner daemon's `/run` reply onto the owner's tool surface (§4b). The
/// daemon answers `200 {mission_id, accepted:true}` on acceptance, or a `4xx`
/// carrying its honest `{error, detail}` (`unpinned_runner`, `workspace_not_allowed`,
/// …). A non-2xx becomes an `InvalidParams` whose detail carries the daemon's own
/// keyword VERBATIM (never a silent success — §5d "never silence"). Pure.
pub fn map_runnerd_response(status: u16, body: &Value) -> Result<Value, M1ndError> {
    if (200..300).contains(&status) {
        // Relay the daemon's acceptance shape verbatim under the `{result}` envelope
        // convention the UI client unwraps.
        return Ok(json!({
            "mission_id": body.get("mission_id").cloned().unwrap_or(Value::Null),
            "accepted": body.get("accepted").and_then(|v| v.as_bool()).unwrap_or(true),
            "runner_id": body.get("runner_id").cloned().unwrap_or(Value::Null),
        }));
    }
    let keyword = body
        .get("error")
        .and_then(|v| v.as_str())
        .unwrap_or("runner_error");
    let detail = body
        .get("detail")
        .and_then(|v| v.as_str())
        .or_else(|| body.get("message").and_then(|v| v.as_str()))
        .unwrap_or("the runner daemon refused the spawn");
    Err(M1ndError::InvalidParams {
        tool: "mission_spawn".to_string(),
        detail: format!("{keyword}: {detail}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch {
        dir: PathBuf,
    }
    impl Scratch {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "m1nd-runnerd-owner-test-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            std::fs::create_dir_all(&dir).expect("mk scratch");
            Self { dir }
        }
        fn write_secret(&self, s: &str) {
            std::fs::write(secret_path(&self.dir), s).expect("write secret");
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn spawn_input() -> SpawnInput {
        SpawnInput {
            runner_id: "build-1".to_string(),
            packet_markdown: "# packet".to_string(),
            block_id: "sb_alpha".to_string(),
            brain_ref: "repo-a".to_string(),
        }
    }

    #[test]
    fn secret_matches_is_length_and_value_sensitive() {
        assert!(secret_matches("abcd", "abcd"));
        assert!(!secret_matches("abcd", "abce"));
        assert!(!secret_matches("abcd", "abc"));
        assert!(!secret_matches("", "x"));
    }

    #[test]
    fn announce_without_secret_file_is_unauthorized() {
        let s = Scratch::new("no-secret");
        let reg = RunnerdRegistry::default();
        let input = AnnounceInput {
            runner_ids: vec!["build-1".to_string()],
            port: 61999,
            boot_challenge: "chal".to_string(),
        };
        let out = apply_announce(&reg, &s.dir, Some("anything"), &input, 1);
        assert!(matches!(out, AnnounceOutcome::Unauthorized));
        assert!(reg.port_for("build-1").is_none(), "nothing registered");
    }

    #[test]
    fn announce_wrong_secret_is_unauthorized_correct_registers_and_echoes() {
        let s = Scratch::new("announce");
        s.write_secret("s3cr3t");
        let reg = RunnerdRegistry::default();
        let input = AnnounceInput {
            runner_ids: vec!["build-1".to_string(), "name-1".to_string()],
            port: 61999,
            boot_challenge: "chal-xyz".to_string(),
        };

        // Wrong secret → 401, nothing registered.
        assert!(matches!(
            apply_announce(&reg, &s.dir, Some("wrong"), &input, 1),
            AnnounceOutcome::Unauthorized
        ));
        assert!(reg.port_for("build-1").is_none());

        // Right secret → registered + challenge echoed.
        let out = apply_announce(&reg, &s.dir, Some("s3cr3t"), &input, 42);
        match out {
            AnnounceOutcome::Registered(body) => {
                assert_eq!(body["echo"], "chal-xyz", "the liveness challenge is echoed");
                assert_eq!(body["ok"], true);
            }
            _ => panic!("expected Registered"),
        }
        assert_eq!(reg.port_for("build-1"), Some(61999));
        assert_eq!(reg.port_for("name-1"), Some(61999));
        // Status lists both, deterministically ordered.
        let status = reg.status_json();
        assert_eq!(status["count"], 2);
        assert_eq!(status["runners"][0]["runner_id"], "build-1");
    }

    #[test]
    fn resolve_spawn_refuses_unknown_runner_missing_secret_and_no_workspace() {
        let s = Scratch::new("resolve");
        let reg = RunnerdRegistry::default();
        let input = spawn_input();

        // (a) runner not live → honest "no runner daemon connected".
        let err =
            resolve_spawn_target(&reg, &s.dir, &input, Some("/repo")).expect_err("no live runner");
        assert!(err.to_string().contains("no live runner"), "got {err}");

        // Make the runner live.
        reg.register(&["build-1".to_string()], 61999, 1);

        // (b) live runner but NO secret file → honest refusal.
        let err = resolve_spawn_target(&reg, &s.dir, &input, Some("/repo")).expect_err("no secret");
        assert!(err.to_string().contains("runnerd.secret"), "got {err}");

        // (c) secret present but no workspace root → honest refusal.
        s.write_secret("s3cr3t");
        let err = resolve_spawn_target(&reg, &s.dir, &input, None).expect_err("no workspace");
        assert!(err.to_string().contains("no workspace root"), "got {err}");
    }

    #[test]
    fn resolve_spawn_builds_the_run_request_with_the_secret_out_of_band() {
        let s = Scratch::new("resolve-ok");
        s.write_secret("s3cr3t");
        let reg = RunnerdRegistry::default();
        reg.register(&["build-1".to_string()], 61999, 1);
        let input = spawn_input();

        let target = resolve_spawn_target(&reg, &s.dir, &input, Some("/repo/root")).unwrap();
        assert_eq!(target.url, "http://127.0.0.1:61999/run");
        assert_eq!(
            target.secret, "s3cr3t",
            "the secret is out-of-band, never in the body"
        );
        assert_eq!(target.body["runner_id"], "build-1");
        assert_eq!(target.body["block_id"], "sb_alpha");
        assert_eq!(target.body["brain_ref"], "repo-a");
        assert_eq!(
            target.body["brain"], "/repo/root",
            "the workspace/routing is threaded"
        );
        // The secret NEVER appears in the forwarded body (the browser proxy law).
        assert!(!target.body.to_string().contains("s3cr3t"));
    }

    #[test]
    fn map_runnerd_response_accept_and_honest_refusals() {
        // 200 accept → relayed shape.
        let ok = map_runnerd_response(
            200,
            &json!({"mission_id": "msn_0123456789ab", "accepted": true, "runner_id": "build-1"}),
        )
        .unwrap();
        assert_eq!(ok["mission_id"], "msn_0123456789ab");
        assert_eq!(ok["accepted"], true);

        // 403 unpinned → the daemon's keyword surfaces verbatim.
        let err = map_runnerd_response(
            403,
            &json!({"error": "unpinned_runner", "detail": "runner 'x' is not pinned"}),
        )
        .expect_err("a refusal maps to an error");
        assert!(err.to_string().contains("unpinned_runner"), "got {err}");
        assert!(err.to_string().contains("not pinned"), "got {err}");
    }
}
