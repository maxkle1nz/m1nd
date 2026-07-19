//! Field-triage #5 — the `--attach` bridge must survive an owner (`--serve`)
//! restart transparently.
//!
//! THE BUG (reported three times in `~/.m1nd/field-reports.jsonl`): when the
//! `--serve` owner restarts (a NORMAL event — e.g. a launchd kickstart on a
//! version upgrade), every live `--attach` bridge keeps its now-stale
//! `Mcp-Session-Id`. The next forwarded call hits the fresh owner, which has no
//! such session, so the owner answers `MCP error -32001: Unknown or expired
//! Mcp-Session-Id; re-initialize`. The bridge used to forward that error straight
//! to the host and the session was dead until the host reconnected.
//!
//! THE FIX (proved here red→green): on a `-32001` from the owner, the bridge
//! transparently re-runs `initialize` — REPLAYING the retained original host
//! initialize params (so clientInfo/capabilities/protocolVersion are preserved) —
//! captures the fresh `Mcp-Session-Id`, re-sends `notifications/initialized`, and
//! retries the original request ONCE under the new session. The host sees a clean
//! result as if nothing happened.
//!
//! We spawn a REAL owner as a child process (only a spawned process can prove the
//! restart path), drive [`attach_client::forward_with_reinit`] the way the bridge
//! loop does, kill+respawn the owner on the SAME port, and assert the second call
//! succeeds and preserves the replayed clientInfo. This whole test requires the
//! `serve` feature (owner HTTP transport + reqwest client).
#![cfg(feature = "serve")]

use std::net::TcpListener;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use m1nd_mcp::attach_client::{forward_with_reinit, post_and_demux, AttachSession, PostOutcome};

/// Path to the compiled binary under test (Cargo sets `CARGO_BIN_EXE_<name>`).
const BIN: &str = env!("CARGO_BIN_EXE_m1nd-mcp");

/// The distinctive clientInfo the "host" sends at initialize. The whole point of
/// the fix is that a transparent re-init REPLAYS this — so after an owner restart
/// the owner must still see THIS exact clientInfo, proving params were retained.
const CLIENT_NAME: &str = "attach-reinit-probe";

/// Grab a free TCP port by binding :0 and reading back the assigned port, then
/// dropping the listener so the owner child can bind it. (Classic ephemeral-port
/// handshake — mirrors how the e2e harness picks a port.)
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("read local addr").port()
}

/// Spawn a `--serve` owner on `port` with an isolated runtime + graph under
/// `tmp`, so the test never touches the developer's real runtime. Waits until the
/// owner answers an `initialize` before returning.
fn spawn_owner(port: u16, tmp: &std::path::Path) -> Child {
    let runtime = tmp.join("runtime");
    let child = Command::new(BIN)
        .arg("--serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--no-gui")
        // Hermetic runtime: never bind the real one, never open a browser.
        .env("M1ND_RUNTIME_DIR", &runtime)
        .env("M1ND_REGISTRY_DIR", tmp.join("registry"))
        .env("M1ND_GRAPH_SOURCE", runtime.join("graph.snapshot"))
        .env("M1ND_PLASTICITY_STATE", runtime.join("plasticity.json"))
        .env("M1ND_NO_GUI", "1")
        .spawn()
        .expect("spawn --serve owner");
    child
}

fn wait_for_owner_token(tmp: &std::path::Path) -> String {
    let token_path = tmp
        .join("runtime")
        .join(m1nd_mcp::http_security::HTTP_AUTH_TOKEN_FILE_NAME);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(token) = m1nd_mcp::http_security::read_existing_bearer_token(&token_path) {
            return token;
        }
        assert!(
            Instant::now() < deadline,
            "owner never created its HTTP bearer token within 30s"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Build the initialize payload the host would send, carrying the distinctive
/// clientInfo we later assert survived the replay.
fn initialize_payload() -> String {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": CLIENT_NAME, "version": "9.9.9" }
        }
    })
    .to_string()
}

/// Poll `POST /mcp` with an initialize until the owner answers (bounded), so we
/// don't race the child's bind. Returns a fresh [`AttachSession`] with the
/// captured session id + negotiated protocol version + retained init payload,
/// exactly like the bridge's first-initialize path.
async fn wait_and_initialize(
    client: &reqwest::Client,
    endpoint: &str,
    tmp: &std::path::Path,
) -> AttachSession {
    let init = initialize_payload();
    let bearer_token = wait_for_owner_token(tmp);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let mut session = AttachSession {
            bearer_token: Some(bearer_token.clone()),
            ..AttachSession::default()
        };
        match post_and_demux(client, endpoint, &session, &init).await {
            Ok(outcome) => {
                if let Some(v) = outcome.value {
                    if v.get("result").is_some() {
                        // Capture like the real bridge does on initialize.
                        session.capture_initialize(&outcome.session_id_header, &v, &init);
                        assert!(
                            session.mcp_session_id.is_some(),
                            "owner initialize must mint an Mcp-Session-Id"
                        );
                        return session;
                    }
                }
            }
            Err(_) => { /* not up yet */ }
        }
        if Instant::now() >= deadline {
            panic!("owner never answered initialize within 30s");
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

/// A cheap, always-available request to forward through the bridge. `tools/list`
/// needs no graph and no agent_id, so it exercises the session-routing path
/// without depending on ingest state.
fn tools_list_payload(id: i64) -> String {
    serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": "tools/list" }).to_string()
}

/// Poll a bare `initialize` against `endpoint` (with a FRESH throwaway session, so
/// the caller's real bridge session is left untouched) until the owner answers a
/// `result`, or panic after `timeout`. Used to know a (re)spawned owner is serving
/// before driving the stale-session bridge through it — without racing the bind.
async fn wait_until_serving(
    client: &reqwest::Client,
    endpoint: &str,
    timeout: Duration,
    tmp: &std::path::Path,
) {
    let probe = initialize_payload();
    let bearer_token = wait_for_owner_token(tmp);
    let deadline = Instant::now() + timeout;
    loop {
        let fresh = AttachSession {
            bearer_token: Some(bearer_token.clone()),
            ..AttachSession::default()
        };
        if let Ok(o) = post_and_demux(client, endpoint, &fresh, &probe).await {
            if o.value.and_then(|v| v.get("result").cloned()).is_some() {
                return;
            }
        }
        if Instant::now() >= deadline {
            panic!("owner never came back to serving within {timeout:?}");
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

/// Kill an owner child, wait for exit, give the OS a beat to release the port,
/// then spawn a fresh owner on the SAME port + tmp (the launchd-kickstart /
/// binary-swap event) and block until it is serving. Returns the new child.
async fn restart_owner(
    old: &mut Child,
    client: &reqwest::Client,
    endpoint: &str,
    port: u16,
    tmp: &std::path::Path,
) -> Child {
    let _ = old.kill();
    let _ = old.wait();
    // Give the OS a moment to release the port before rebinding.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let fresh = spawn_owner(port, tmp);
    wait_until_serving(client, endpoint, Duration::from_secs(30), tmp).await;
    fresh
}

#[tokio::test(flavor = "multi_thread")]
async fn bridge_survives_owner_restart_transparently() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let endpoint = format!("{base_url}/mcp");
    let client = reqwest::Client::builder().build().expect("reqwest client");

    // --- 1. Owner up; bridge initializes and captures the session. ---
    let mut owner = spawn_owner(port, tmp.path());
    let mut session = wait_and_initialize(&client, &endpoint, tmp.path()).await;
    let first_session_id = session
        .mcp_session_id
        .clone()
        .expect("session id after initialize");

    // A call through the live session works.
    let ok = forward_with_reinit(&client, &endpoint, &mut session, &tools_list_payload(2))
        .await
        .expect("first tools/list forwards");
    assert!(
        ok.get("result").is_some(),
        "first call should return a result, got: {ok}"
    );

    // --- 2. Restart the owner on the SAME port (the launchd-kickstart event). ---
    let _ = owner.kill();
    let _ = owner.wait();
    // Give the OS a moment to release the port before rebinding.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut owner2 = spawn_owner(port, tmp.path());

    // Wait until the fresh owner is actually serving (a bare initialize answers),
    // WITHOUT mutating our bridge session — the bridge still holds the stale id.
    {
        let probe = initialize_payload();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let fresh = AttachSession {
                bearer_token: session.bearer_token.clone(),
                ..AttachSession::default()
            };
            if let Ok(o) = post_and_demux(&client, &endpoint, &fresh, &probe).await {
                if o.value.and_then(|v| v.get("result").cloned()).is_some() {
                    break;
                }
            }
            if Instant::now() >= deadline {
                let _ = owner2.kill();
                panic!("restarted owner never came back within 30s");
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    // --- 3. THE ASSERTION. The SAME bridge session (holding the STALE id) makes
    // another call. On `main` this returns the -32001 error verbatim (RED). With
    // the fix the bridge transparently re-initializes and the call succeeds (GREEN).
    let after = forward_with_reinit(&client, &endpoint, &mut session, &tools_list_payload(3)).await;

    let _ = owner2.kill();
    let _ = owner2.wait();

    let after = after.expect("forward_with_reinit must not surface a transport error");
    assert!(
        after.get("error").is_none(),
        "after owner restart the bridge must transparently re-initialize; \
         instead the host saw an error frame: {after}"
    );
    assert!(
        after.get("result").is_some(),
        "transparent re-init should yield a real result, got: {after}"
    );

    // The re-init must have minted a NEW session id (proves it actually happened,
    // not a fluke where the stale id still worked).
    let second_session_id = session
        .mcp_session_id
        .clone()
        .expect("session id after re-init");
    assert_ne!(
        first_session_id, second_session_id,
        "a transparent re-init must capture a fresh Mcp-Session-Id"
    );
}

/// Field-triage batch-C / PRD §21.14 — "sessions ride #225" is a BUDGETED claim,
/// so it needs a proof that survives MORE than one restart. The original test
/// restarts once; this drives the SAME bridge session through TWO consecutive
/// owner restarts (the second respawned after a full kill — the binary-swap /
/// launchd-kickstart shape), asserting transparent recovery each time and a fresh
/// session id after each. A regression that recovers once but not repeatedly
/// (e.g. a re-init guard that latches) fails HERE.
#[tokio::test(flavor = "multi_thread")]
async fn bridge_survives_two_restarts_including_binary_swap() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let endpoint = format!("{base_url}/mcp");
    let client = reqwest::Client::builder().build().expect("reqwest client");

    let mut owner = spawn_owner(port, tmp.path());
    let mut session = wait_and_initialize(&client, &endpoint, tmp.path()).await;
    let mut last_sid = session.mcp_session_id.clone().expect("initial session id");

    // A live call works before any restart.
    let ok = forward_with_reinit(&client, &endpoint, &mut session, &tools_list_payload(2))
        .await
        .expect("first tools/list forwards");
    assert!(ok.get("result").is_some(), "pre-restart call: {ok}");

    // Two consecutive restarts, each driven through the SAME stale bridge session.
    for cycle in 1..=2 {
        owner = restart_owner(&mut owner, &client, &endpoint, port, tmp.path()).await;

        // Distinct request id per cycle (10, 11) so a mismatched frame can't slip by.
        let id = 10 + cycle;
        let after = forward_with_reinit(&client, &endpoint, &mut session, &tools_list_payload(id))
            .await
            .unwrap_or_else(|e| panic!("cycle {cycle}: transport error: {e}"));
        assert!(
            after.get("error").is_none(),
            "cycle {cycle}: bridge must transparently re-init, saw error frame: {after}"
        );
        assert!(
            after.get("result").is_some(),
            "cycle {cycle}: re-init should yield a real result, got: {after}"
        );

        let sid = session
            .mcp_session_id
            .clone()
            .unwrap_or_else(|| panic!("cycle {cycle}: session id after re-init"));
        assert_ne!(
            last_sid, sid,
            "cycle {cycle}: each restart must mint a FRESH Mcp-Session-Id"
        );
        last_sid = sid;
    }

    let _ = owner.kill();
    let _ = owner.wait();
}

/// Field-triage batch-C — LOCK the owner's unknown-session wire SHAPE to the shape
/// the bridge's re-init trigger matches, so a future owner change that alters it
/// FAILS loudly here instead of silently re-breaking live sessions.
///
/// This is the gap the investigation surfaced: #225 keyed re-init ONLY on a
/// parseable `-32001` JSON-RPC frame. The owner's `POST /mcp` currently pairs its
/// `404` with that frame (so #225 works) — but its `GET /mcp` relay answers `404`
/// with a PLAIN-TEXT body and no frame, and a proxy could strip the body on POST.
/// The fix keys re-init on the 404 STATUS too. Here we assert:
///   (a) the real owner's POST unknown-session response is `404` carrying the
///       `-32001` frame — the shape the bridge matches; drift on EITHER trips it;
///   (b) `PostOutcome::signals_session_expired()` fires on that real outcome;
///   (c) it ALSO fires on a synthetic frame-less 404 (the plain-text / proxy
///       shape), proving the status path — not only the frame — drives recovery.
#[tokio::test(flavor = "multi_thread")]
async fn owner_unknown_session_wire_shape_is_recoverable() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let endpoint = format!("{base_url}/mcp");
    let client = reqwest::Client::builder().build().expect("reqwest client");

    let mut owner = spawn_owner(port, tmp.path());
    wait_until_serving(&client, &endpoint, Duration::from_secs(30), tmp.path()).await;

    // Drive a post-init request carrying a BOGUS session id (exactly what a stale
    // bridge holds after an owner restart) and inspect the RAW outcome.
    let bogus = AttachSession {
        bearer_token: Some(wait_for_owner_token(tmp.path())),
        mcp_session_id: Some("00000000-0000-0000-0000-deadbeefdead".to_string()),
        protocol_version: Some("2025-06-18".to_string()),
        initialize_payload: None,
        caller_root: None,
    };
    let outcome = post_and_demux(&client, &endpoint, &bogus, &tools_list_payload(2))
        .await
        .expect("owner reachable");

    let _ = owner.kill();
    let _ = owner.wait();

    // (a) The owner's unknown-session shape is EXACTLY what the bridge matches.
    assert_eq!(
        outcome.status, 404,
        "owner must answer HTTP 404 for an unknown Mcp-Session-Id (the re-init \
         trigger status); got {}",
        outcome.status
    );
    let frame = outcome
        .value
        .as_ref()
        .expect("owner's 404 currently carries a JSON-RPC frame");
    assert_eq!(
        frame
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(|c| c.as_i64()),
        Some(-32001),
        "owner's unknown-session frame must carry code -32001; got: {frame}"
    );

    // (b) The real outcome is recognized as session-expired → the bridge re-inits.
    assert!(
        outcome.signals_session_expired(),
        "the real owner unknown-session outcome must trigger re-init"
    );

    // (c) The STATUS path (not the frame) is load-bearing: a frame-less 404 — the
    // owner's own SSE/GET shape, or anything a proxy produces — still triggers.
    let frameless_404 = PostOutcome {
        value: None,
        session_id_header: None,
        status: 404,
    };
    assert!(
        frameless_404.signals_session_expired(),
        "a 404 with NO JSON-RPC frame must STILL trigger re-init (status-driven)"
    );

    // And a normal success outcome must NOT be mistaken for expiry.
    let ok_200 = PostOutcome {
        value: Some(serde_json::json!({"jsonrpc":"2.0","id":2,"result":{}})),
        session_id_header: None,
        status: 200,
    };
    assert!(
        !ok_200.signals_session_expired(),
        "a 200 result must never be treated as session-expired"
    );
}
