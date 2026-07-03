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

use m1nd_mcp::attach_client::{forward_with_reinit, post_and_demux, AttachSession};

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
    let child = Command::new(BIN)
        .arg("--serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--no-gui")
        // Hermetic runtime: never bind the real one, never open a browser.
        .env("M1ND_RUNTIME_DIR", tmp.join("runtime"))
        .env("M1ND_REGISTRY_DIR", tmp.join("registry"))
        .env("M1ND_GRAPH_SOURCE", tmp.join("graph.snapshot"))
        .env("M1ND_PLASTICITY_STATE", tmp.join("plasticity.json"))
        .env("M1ND_NO_GUI", "1")
        .spawn()
        .expect("spawn --serve owner");
    child
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
async fn wait_and_initialize(client: &reqwest::Client, endpoint: &str) -> AttachSession {
    let init = initialize_payload();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let mut session = AttachSession::default();
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

#[tokio::test(flavor = "multi_thread")]
async fn bridge_survives_owner_restart_transparently() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");
    let endpoint = format!("{base_url}/mcp");
    let client = reqwest::Client::builder().build().expect("reqwest client");

    // --- 1. Owner up; bridge initializes and captures the session. ---
    let mut owner = spawn_owner(port, tmp.path());
    let mut session = wait_and_initialize(&client, &endpoint).await;
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
            let fresh = AttachSession::default();
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
