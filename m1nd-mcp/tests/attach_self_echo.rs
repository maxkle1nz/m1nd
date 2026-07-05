//! Field-triage L21 — write-tool responses must return REAL envelopes through the
//! `--attach` bridge, never a literal `null`.
//!
//! THE BUG (field-reported, mailbox L21): a write tool (`ingest`, `memorize`, …)
//! called THROUGH the `--attach` bridge succeeds on the owner (graph grows, memory
//! file is written) but the host sees a response body of literally `"null"` — it
//! cannot tell success from failure. Direct stdio (non-attach) and a direct HTTP
//! `POST /mcp` both return the proper counts envelope; only the bridge is wrong.
//!
//! THE MECHANISM (pinned by reproduction): the owner's `POST /mcp` publishes a
//! `graph_changed` mutation event onto the process-wide broadcast bus for EVERY
//! mutation — INCLUDING the one made by the caller's own wire session. The bridge's
//! push relay (`GET /mcp` SSE) forwards that `notifications/m1nd/graph_changed`
//! frame to the host's stdout, where it RACES the real tool response into the SAME
//! serialized stdout sink. A host that reads the first frame after its mutation and
//! extracts `.result` gets the id-less notification (whose `result` is absent) →
//! serialized as the literal `"null"`. Reads are immune (the relay suppresses read
//! results); the self-echo only fires for mutation tools — exactly the report.
//!
//! THE FIX: an agent must never see an echo of its OWN mutation. The owner stamps
//! each broadcast mutation event with the originating wire `mcp-session-id`, and the
//! `GET /mcp` SSE stream skips events whose origin session equals the stream's own
//! session — so the bridge's relay never forwards the caller's own mutation back.
//!
//! We spawn a REAL owner + the REAL `--attach` bridge binary (only the true wire
//! path can prove this), feed `ingest` then `memorize` on the bridge's stdin the way
//! a stdio host does, and assert every frame the host receives:
//!   (1) the `ingest` response body parses to JSON carrying non-null counts;
//!   (2) the `memorize` response body parses to JSON carrying `ok`/`claims_written`;
//!   (3) NO `notifications/m1nd/graph_changed` self-echo frame is delivered for the
//!       bridge's own mutations (the frame that, winning the race, shows as `null`).
//! RED on `main` (the self-echo frame is delivered, racing the response); GREEN with
//! the origin-session suppression. Requires the `serve` feature (owner HTTP + relay).
#![cfg(feature = "serve")]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Path to the compiled binary under test (Cargo sets `CARGO_BIN_EXE_<name>`).
const BIN: &str = env!("CARGO_BIN_EXE_m1nd-mcp");

/// Grab a free TCP port by binding :0, reading the assigned port, then dropping the
/// listener so the owner child can bind it (classic ephemeral-port handshake).
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    listener.local_addr().expect("read local addr").port()
}

/// Spawn a `--serve` owner on `port` with an isolated runtime under `tmp`, so the
/// test never touches the developer's real runtime.
fn spawn_owner(port: u16, tmp: &std::path::Path) -> Child {
    Command::new(BIN)
        .arg("--serve")
        .arg("--port")
        .arg(port.to_string())
        .arg("--no-gui")
        .env("M1ND_RUNTIME_DIR", tmp.join("runtime"))
        .env("M1ND_REGISTRY_DIR", tmp.join("registry"))
        .env("M1ND_GRAPH_SOURCE", tmp.join("graph.snapshot"))
        .env("M1ND_PLASTICITY_STATE", tmp.join("plasticity.json"))
        .env("M1ND_NO_GUI", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn --serve owner")
}

/// Block until the owner answers an `initialize` on `POST /mcp` (bounded), so we
/// don't race the child's bind. Uses a blocking reqwest client on a throwaway thread
/// via `ureq`-free std: we just retry a raw TCP+HTTP is overkill, so reuse reqwest's
/// blocking feature is unavailable here — instead we shell a tiny curl-equivalent
/// through the bridge is also overkill. Simplest robust path: use the async reqwest
/// the crate already depends on, on a one-off runtime.
fn wait_until_serving(base_url: &str) {
    let endpoint = format!("{base_url}/mcp");
    let init = init_payload();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("rt");
    rt.block_on(async {
        let client = reqwest::Client::builder().build().expect("client");
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let r = client
                .post(&endpoint)
                .header("Accept", "application/json, text/event-stream")
                .header("Content-Type", "application/json")
                .body(init.clone())
                .send()
                .await;
            if let Ok(resp) = r {
                if let Ok(body) = resp.text().await {
                    if body.contains("\"result\"") {
                        return;
                    }
                }
            }
            if Instant::now() >= deadline {
                panic!("owner never answered initialize within 30s");
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    });
}

fn init_payload() -> String {
    serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "l21-self-echo-probe", "version": "1" }
        }
    })
    .to_string()
}

/// Spawn a reader thread that pushes each newline-framed stdout line from the bridge
/// onto an mpsc channel, so the test can pull frames with a timeout.
fn spawn_line_reader(stdout: ChildStdout) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let trimmed = line.trim_end().to_string();
                    if !trimmed.is_empty() && tx.send(trimmed).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    rx
}

/// Pull the next frame within `timeout`, or `None` if none arrives.
fn next_frame(rx: &mpsc::Receiver<String>, timeout: Duration) -> Option<String> {
    rx.recv_timeout(timeout).ok()
}

/// Collect every frame that arrives within `window` (a quiet-period drain).
fn drain_frames(rx: &mpsc::Receiver<String>, window: Duration) -> Vec<String> {
    let mut out = Vec::new();
    let deadline = Instant::now() + window;
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(frame) => out.push(frame),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    out
}

/// Write one newline-framed JSON-RPC frame to the bridge's stdin.
fn send(stdin: &mut std::process::ChildStdin, value: serde_json::Value) {
    let line = format!("{}\n", serde_json::to_string(&value).unwrap());
    stdin.write_all(line.as_bytes()).expect("write stdin");
    stdin.flush().expect("flush stdin");
}

/// Is this frame a `graph_changed` self-echo notification (the racing `null`)?
fn is_graph_changed(frame: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(frame)
        .ok()
        .and_then(|v| v.get("method").and_then(|m| m.as_str()).map(str::to_owned))
        .as_deref()
        == Some("notifications/m1nd/graph_changed")
}

/// Extract the tool-result text from a `tools/call` JSON-RPC response frame with the
/// given `id`, parse it as JSON, and return that object — or `None` if the frame is
/// not the matching response or its text is not JSON.
fn tool_result_json(frame: &str, id: i64) -> Option<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(frame).ok()?;
    if v.get("id").and_then(|i| i.as_i64()) != Some(id) {
        return None;
    }
    let text = v
        .get("result")?
        .get("content")?
        .get(0)?
        .get("text")?
        .as_str()?;
    serde_json::from_str::<serde_json::Value>(text).ok()
}

#[test]
fn write_tools_return_real_envelopes_through_the_bridge() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let port = free_port();
    let base_url = format!("http://127.0.0.1:{port}");

    // A tiny hermetic repo for `ingest` to chew on.
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).expect("repo dir");
    std::fs::write(
        repo.join("lib.rs"),
        "pub fn add(a: i32, b: i32) -> i32 { a + b }\npub fn mul(a: i32, b: i32) -> i32 { a * b }\n",
    )
    .expect("write lib.rs");

    // 1. Owner up.
    let mut owner = spawn_owner(port, tmp.path());
    wait_until_serving(&base_url);

    // 2. The REAL `--attach` bridge, stdio host on one side, owner on the other.
    //    Pin the bridge's caller root to the ingested repo (via M1ND_WORKSPACE_ROOT)
    //    so its hop-2 header names a root the bound graph COVERS after the ingest
    //    below — otherwise the bridge's own cwd is a foreign brainless root and the
    //    memorize is correctly refused (MEDULLA M5a S2), which this test is not
    //    exercising.
    let mut bridge = Command::new(BIN)
        .arg("--attach")
        .arg(&base_url)
        .env("M1ND_WORKSPACE_ROOT", &repo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn --attach bridge");
    let mut stdin = bridge.stdin.take().expect("bridge stdin");
    let rx = spawn_line_reader(bridge.stdout.take().expect("bridge stdout"));

    // --- initialize + initialized handshake through the bridge. ---
    send(&mut stdin, serde_json::from_str(&init_payload()).unwrap());
    let init_resp = next_frame(&rx, Duration::from_secs(15)).expect("initialize response");
    assert!(
        init_resp.contains("\"result\""),
        "bridge initialize should return a result frame, got: {init_resp}"
    );
    send(
        &mut stdin,
        serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    );
    // Let the push relay's SSE GET subscribe before we mutate (so a self-echo, if the
    // bug is present, actually has a live subscriber to be delivered on).
    std::thread::sleep(Duration::from_millis(800));

    // --- INGEST (mutation) through the bridge. ---
    send(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": "ingest", "arguments": {
                "agent_id": "l21", "path": repo.to_string_lossy() } }
        }),
    );
    let ingest_frames = drain_frames(&rx, Duration::from_secs(8));

    // --- MEMORIZE (mutation) through the bridge. ---
    send(
        &mut stdin,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "memorize", "arguments": {
                "agent_id": "l21", "node_label": "l21-fact",
                "claims": [ { "label": "the bridge returns real envelopes", "confidence": 0.9 } ] } }
        }),
    );
    let memorize_frames = drain_frames(&rx, Duration::from_secs(8));

    // Tear down before asserting (so a panic never leaks children).
    let _ = stdin.write_all(b"");
    drop(stdin);
    let _ = bridge.kill();
    let _ = bridge.wait();
    let _ = owner.kill();
    let _ = owner.wait();

    // ---- (1) the ingest response carries REAL, non-null counts. ----
    let ingest_body = ingest_frames
        .iter()
        .find_map(|f| tool_result_json(f, 2))
        .unwrap_or_else(|| {
            panic!(
                "ingest must return a JSON tool-result body (not null); frames seen: {ingest_frames:?}"
            )
        });
    assert!(
        ingest_body
            .get("node_count")
            .and_then(|v| v.as_u64())
            .is_some(),
        "ingest body must carry a non-null node_count; got: {ingest_body}"
    );
    assert!(
        ingest_body
            .get("edges_created")
            .and_then(|v| v.as_u64())
            .is_some(),
        "ingest body must carry edges_created; got: {ingest_body}"
    );

    // ---- (2) the memorize response carries ok / claims_written. ----
    let memorize_body = memorize_frames
        .iter()
        .find_map(|f| tool_result_json(f, 3))
        .unwrap_or_else(|| {
            panic!(
                "memorize must return a JSON tool-result body (not null); frames seen: {memorize_frames:?}"
            )
        });
    assert_eq!(
        memorize_body.get("ok").and_then(|v| v.as_bool()),
        Some(true),
        "memorize body must carry ok:true; got: {memorize_body}"
    );
    assert!(
        memorize_body
            .get("claims_written")
            .and_then(|v| v.as_u64())
            .is_some(),
        "memorize body must carry claims_written; got: {memorize_body}"
    );

    // ---- (3) THE ASSERTION. No `graph_changed` SELF-ECHO reached the host. This is
    // the racing frame that, delivered before the response, a host reads as `null`.
    // RED on main (the owner echoes the caller's own mutation to its own relay);
    // GREEN once the owner suppresses same-session events. ----
    let self_echoes: Vec<&String> = ingest_frames
        .iter()
        .chain(memorize_frames.iter())
        .filter(|f| is_graph_changed(f))
        .collect();
    assert!(
        self_echoes.is_empty(),
        "the bridge must NOT forward a graph_changed self-echo for the caller's own \
         mutation (this frame races the response and shows to the host as null); \
         saw {} such frame(s): {:?}",
        self_echoes.len(),
        self_echoes
    );
}
