// === m1nd-mcp `--attach` stdio↔HTTP bridge ===
//
// Wave 4, Slice 3 — THE deliverable.
//
// `m1nd-mcp --attach <base_url>` is a thin bridge: to the host (Claude Code) it
// is a standard stdio MCP server; to a running `m1nd-mcp --serve` owner it is a
// standard Streamable-HTTP MCP client. It loads NO graph, builds NO engines, and
// takes NO lease. Two such clients pointed at one `--serve` owner SHARE that
// owner's single live graph: agent A's mutation is visible to agent B with no
// reload — because both ultimately drive the SAME `Arc<Mutex<SessionState>>`
// inside the owner process (via `POST /mcp`).
//
// STDOUT FRAMING FIDELITY is the single biggest risk: if this bridge's stdout
// deviates even slightly from the MCP stdio wire format the host silently fails
// to initialize. MITIGATION (load-bearing): we reuse the EXACT framing
// primitives the embedded stdio server uses — `read_request_payload` /
// `write_response` / `TransportMode` from `server.rs` — verbatim, and keep ALL
// diagnostics on stderr. Nothing but JSON-RPC frames ever touches stdout.
//
// Feature-gated behind "serve" (it needs the `reqwest` HTTP client, which lives
// under that feature).

#![cfg(feature = "serve")]

use std::io::{BufReader, Write};

use tokio::sync::mpsc;

use crate::protocol::{JsonRpcError, JsonRpcResponse};
use crate::server::{read_request_payload, TransportMode};

/// Per-spec MCP session header name.
const MCP_SESSION_HEADER: &str = "mcp-session-id";
/// Per-spec negotiated protocol version header.
const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";

/// One framed JSON-RPC message destined for stdout, carrying the `TransportMode`
/// it must be framed in. Pushed by BOTH the request/response loop and the push
/// relay; drained by a SINGLE writer task so frames can never interleave.
type StdoutFrame = (serde_json::Value, TransportMode);

/// Handle to the single serialized stdout writer. Cloneable: every producer (the
/// request/response loop, the SSE push relay) holds a clone and pushes whole
/// frames; the one writer task owns `stdout` and is the only thing that touches
/// it, guaranteeing byte-level framing fidelity with no cross-frame interleave.
#[derive(Clone)]
struct StdoutSink {
    tx: mpsc::UnboundedSender<StdoutFrame>,
}

impl StdoutSink {
    /// Push a typed `JsonRpcResponse` (bridge-generated error/response) to the
    /// serialized writer in `mode`.
    fn emit(&self, resp: &JsonRpcResponse, mode: TransportMode) {
        match serde_json::to_value(resp) {
            Ok(v) => self.send(v, mode),
            Err(e) => eprintln!("[m1nd-mcp][attach] failed to serialize response frame: {e}"),
        }
    }

    /// Push an arbitrary JSON value (an owner-originated frame, forwarded as-is to
    /// preserve its exact `result`/`error`/notification shape) to the serialized
    /// writer in `mode`.
    fn emit_value(&self, value: serde_json::Value, mode: TransportMode) {
        self.send(value, mode);
    }

    /// Non-blocking, callable from sync OR async context (unbounded channel). The
    /// only failure mode is a closed receiver (writer task gone / stdout dead),
    /// which we log to stderr — there is nowhere left to deliver the frame.
    fn send(&self, value: serde_json::Value, mode: TransportMode) {
        if self.tx.send((value, mode)).is_err() {
            eprintln!("[m1nd-mcp][attach] stdout writer gone; dropping outbound frame");
        }
    }
}

/// The ONE task that owns `stdout`. It drains `rx` in arrival order and frames
/// each message with the EXACT logic the embedded stdio server uses
/// (`Content-Length` for `Framed`, newline for `Line`). Because it is the sole
/// writer, two producers can never interleave a notification inside a response.
async fn run_stdout_writer(mut rx: mpsc::UnboundedReceiver<StdoutFrame>) {
    while let Some((value, mode)) = rx.recv().await {
        let json = serde_json::to_string(&value).unwrap_or_default();
        let stdout = std::io::stdout();
        let mut writer = stdout.lock();
        let write_res = match mode {
            TransportMode::Framed => {
                write!(writer, "Content-Length: {}\r\n\r\n{}", json.len(), json)
                    .and_then(|_| writer.flush())
            }
            TransportMode::Line => writeln!(writer, "{}", json).and_then(|_| writer.flush()),
        };
        if write_res.is_err() {
            eprintln!("[m1nd-mcp][attach] stdout closed while writing frame");
            break;
        }
    }
}

/// Session state the bridge captures at `initialize` and replays on every
/// subsequent request, so the owner routes all of this client's traffic to the
/// one shared session.
#[derive(Clone, Default)]
struct AttachSession {
    /// `Mcp-Session-Id` minted by the owner at `initialize`.
    mcp_session_id: Option<String>,
    /// `result.protocolVersion` negotiated at `initialize`.
    protocol_version: Option<String>,
}

/// Run the `--attach` bridge against `base_url` (e.g. `http://127.0.0.1:1337`).
///
/// Loop: read one JSON-RPC frame from stdin (preserving its detected
/// `TransportMode`), POST it to `<base_url>/mcp`, and relay the response frame to
/// stdout in the SAME mode. Exits cleanly on stdin EOF.
pub async fn run_attach_client(base_url: String) {
    let endpoint = format!("{}/mcp", base_url.trim_end_matches('/'));
    eprintln!(
        "[m1nd-mcp][attach] bridging stdio MCP host ↔ {} (no graph / no lease loaded)",
        endpoint
    );

    let client = match reqwest::Client::builder().build() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[m1nd-mcp][attach] failed to build HTTP client: {}", e);
            std::process::exit(1);
        }
    };

    let mut session = AttachSession::default();

    // --- SINGLE serialized stdout writer (load-bearing for framing fidelity). ---
    // There are now TWO producers of stdout frames: the request/response loop
    // below AND the long-lived push relay (spawned once the session id is known).
    // Both push WHOLE frames into this unbounded channel; one dedicated writer
    // task owns `stdout` and emits them in arrival order, so a relay notification
    // can NEVER land in the middle of a response frame. Nothing else writes stdout.
    let (stdout_tx, stdout_rx) = mpsc::unbounded_channel::<StdoutFrame>();
    let sink = StdoutSink { tx: stdout_tx };
    let writer_handle = tokio::spawn(run_stdout_writer(stdout_rx));

    // The push relay is spawned lazily exactly once, right after the first
    // `initialize` captures the Mcp-Session-Id (the owner needs it to route the
    // server→client SSE stream). Guarded so a re-`initialize` never double-spawns.
    let mut relay_spawned = false;
    let mut relay_handle: Option<tokio::task::JoinHandle<()>> = None;

    // Single persistent stdin reader on a dedicated blocking thread. ONE
    // `BufReader` lives for the whole session — a fresh reader per frame would
    // discard already-buffered bytes (read-ahead) and silently drop the next
    // request. Frames are pushed to the async loop over a bounded channel; this
    // mirrors the embedded stdio server's reader-thread pattern in `server.rs`.
    let (frame_tx, mut frame_rx) =
        tokio::sync::mpsc::channel::<(String, TransportMode)>(64);
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut reader = BufReader::new(stdin.lock());
        loop {
            match read_request_payload(&mut reader) {
                Ok(Some(frame)) => {
                    if frame_tx.blocking_send(frame).is_err() {
                        break; // async side gone
                    }
                }
                Ok(None) => break, // EOF
                Err(e) => {
                    eprintln!("[m1nd-mcp][attach] stdin read error: {}", e);
                    break;
                }
            }
        }
        // Drop `frame_tx` → channel closes → async loop sees `None` → clean exit.
    });

    loop {
        // --- Receive one inbound frame from the stdin reader thread. ---
        let (payload, mode) = match frame_rx.recv().await {
            Some(frame) => frame,
            // Channel closed → stdin EOF (or read error already logged) → exit.
            None => {
                eprintln!("[m1nd-mcp][attach] stdin EOF; exiting");
                break;
            }
        };

        let trimmed = payload.trim();
        if trimmed.is_empty() {
            continue;
        }

        // --- Classify: requests (have a non-null `id`) vs notifications. ---
        let parsed: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                // Malformed JSON from the host → reply with a parse error so the
                // host sees a clean frame rather than a dropped message.
                let err = jsonrpc_error(serde_json::Value::Null, -32700, format!("Parse error: {}", e));
                sink.emit(&err, mode);
                continue;
            }
        };

        let req_id = parsed.get("id").cloned();
        let is_request = req_id.as_ref().is_some_and(|v| !v.is_null());
        let method = parsed.get("method").and_then(|m| m.as_str()).map(str::to_owned);
        let is_initialize = method.as_deref() == Some("initialize");

        // --- Build the POST with the MCP-mandated headers. ---
        let mut builder = client
            .post(&endpoint)
            .header(reqwest::header::ACCEPT, "application/json, text/event-stream")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(payload.clone());

        // After init, every request/notification carries the captured session id
        // and negotiated protocol version (so the owner routes to the shared
        // session and not a fresh one).
        if let Some(sid) = &session.mcp_session_id {
            builder = builder.header(MCP_SESSION_HEADER, sid.clone());
        }
        if let Some(pv) = &session.protocol_version {
            builder = builder.header(MCP_PROTOCOL_VERSION_HEADER, pv.clone());
        }

        let response = match builder.send().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[m1nd-mcp][attach] HTTP send error: {}", e);
                // Only a request expects a reply; surface a clean JSON-RPC error
                // to the host so it never hangs. Notifications get nothing.
                if is_request {
                    let id = req_id.clone().unwrap_or(serde_json::Value::Null);
                    let err = jsonrpc_error(
                        id,
                        -32002,
                        format!("attach bridge: failed to reach m1nd owner at {}: {}", endpoint, e),
                    );
                    sink.emit(&err, mode);
                }
                continue;
            }
        };

        let status = response.status();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();

        // On `initialize`, capture the minted session id BEFORE consuming the body.
        if is_initialize {
            if let Some(sid) = response
                .headers()
                .get(MCP_SESSION_HEADER)
                .and_then(|v| v.to_str().ok())
            {
                session.mcp_session_id = Some(sid.to_string());
                eprintln!("[m1nd-mcp][attach] captured Mcp-Session-Id={}", sid);

                // Lazily spawn the server→client push relay exactly once: the
                // owner can only route the `GET /mcp` SSE stream once it knows the
                // session id, which we just captured. Re-`initialize` won't
                // double-spawn thanks to `relay_spawned`.
                if !relay_spawned {
                    relay_spawned = true;
                    let relay = run_push_relay(
                        client.clone(),
                        endpoint.clone(),
                        sid.to_string(),
                        session.protocol_version.clone(),
                        sink.clone(),
                        mode,
                    );
                    relay_handle = Some(tokio::spawn(relay));
                }
            } else {
                eprintln!(
                    "[m1nd-mcp][attach] WARNING: initialize response had no Mcp-Session-Id header"
                );
            }
        }

        // --- Notifications/responses (no id): owner replies 202, nothing to stdout. ---
        if !is_request {
            if status != reqwest::StatusCode::ACCEPTED && !status.is_success() {
                eprintln!(
                    "[m1nd-mcp][attach] notification POST returned {} (expected 202)",
                    status
                );
            }
            continue;
        }

        let id_for_error = req_id.clone().unwrap_or(serde_json::Value::Null);
        let body = match response.text().await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("[m1nd-mcp][attach] failed to read response body: {}", e);
                let err = jsonrpc_error(
                    id_for_error,
                    -32003,
                    format!("attach bridge: failed reading owner response: {}", e),
                );
                sink.emit(&err, mode);
                continue;
            }
        };

        // --- Demux by content-type. ---
        let response_value: Option<serde_json::Value> = if content_type.contains("text/event-stream")
        {
            // SSE: extract the JSON-RPC response frame whose `id` matches the
            // request; relay any interim server→client notifications to stdout.
            extract_sse_response(&body, req_id.as_ref(), mode, &sink)
        } else {
            // application/json (slice-1's path): the body IS the JSON-RPC response.
            match serde_json::from_str::<serde_json::Value>(&body) {
                Ok(v) => Some(v),
                Err(e) => {
                    eprintln!(
                        "[m1nd-mcp][attach] owner returned {} with non-JSON body ({}): {}",
                        status, e, body
                    );
                    None
                }
            }
        };

        match response_value {
            Some(v) => {
                // On a successful `initialize` response, capture the negotiated
                // protocol version for subsequent requests.
                if is_initialize {
                    if let Some(pv) = v
                        .get("result")
                        .and_then(|r| r.get("protocolVersion"))
                        .and_then(|p| p.as_str())
                    {
                        session.protocol_version = Some(pv.to_string());
                        eprintln!("[m1nd-mcp][attach] negotiated protocolVersion={}", pv);
                    }
                }
                sink.emit_value(v, mode);
            }
            None => {
                // We got a request but couldn't surface a usable response frame.
                // Emit a clean JSON-RPC error so the host never hangs.
                let err = jsonrpc_error(
                    id_for_error,
                    -32004,
                    format!(
                        "attach bridge: owner returned {} but no matching JSON-RPC response frame",
                        status
                    ),
                );
                sink.emit(&err, mode);
            }
        }
    }

    // The loop exited on stdin EOF (the host closed its end): the bridge is done.
    // Tear down the long-lived push relay (Wave 4 slice 4) — it would otherwise
    // keep the process alive on its SSE GET forever — then drain the stdout
    // writer so any in-flight frames flush before we return.
    if let Some(handle) = relay_handle {
        handle.abort();
        let _ = handle.await; // swallow the JoinError from the abort
    }
    // Drop the producing sink so the writer task's channel closes and it exits.
    drop(sink);
    let _ = writer_handle.await;
}

/// Long-lived server→client push relay (Wave 4 slice 4).
///
/// Issues `GET {endpoint}` with `Accept: text/event-stream` plus the captured
/// `mcp-session-id` (and `mcp-protocol-version` if negotiated), then streams the
/// owner's SSE body and forwards every JSON-RPC NOTIFICATION (a frame with no
/// `id`, e.g. `notifications/m1nd/graph_changed`) to the host through the SAME
/// serialized `StdoutSink` the request/response loop uses — so an attached agent
/// learns that ANOTHER agent mutated the shared graph without polling.
///
/// Robustness: parsing is incremental (event boundary = blank line); SSE comment
/// / keep-alive lines (`:` prefix) are skipped and never JSON-parsed; id-bearing
/// response frames are ignored here (they belong to the request/response loop, so
/// the relay can never race a real response). On stream error/EOF the relay logs
/// to stderr and retries with bounded exponential backoff — it NEVER crashes the
/// bridge or writes anything but well-formed notification frames to stdout.
async fn run_push_relay(
    client: reqwest::Client,
    endpoint: String,
    session_id: String,
    protocol_version: Option<String>,
    sink: StdoutSink,
    mode: TransportMode,
) {
    use std::time::Duration;

    const MAX_BACKOFF_SECS: u64 = 30;
    let mut backoff_secs: u64 = 1;

    eprintln!("[m1nd-mcp][attach] push relay: subscribing to {} (SSE)", endpoint);

    loop {
        let mut builder = client
            .get(&endpoint)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .header(MCP_SESSION_HEADER, session_id.clone());
        if let Some(pv) = &protocol_version {
            builder = builder.header(MCP_PROTOCOL_VERSION_HEADER, pv.clone());
        }

        match builder.send().await {
            Ok(resp) if resp.status().is_success() => {
                // Connected: reset backoff and stream until the body ends/errors.
                backoff_secs = 1;
                if let Err(e) = stream_relay_body(resp, &sink, mode).await {
                    eprintln!("[m1nd-mcp][attach] push relay stream ended: {e}");
                } else {
                    eprintln!("[m1nd-mcp][attach] push relay stream closed by owner");
                }
            }
            Ok(resp) => {
                eprintln!(
                    "[m1nd-mcp][attach] push relay GET returned {} (not subscribing)",
                    resp.status()
                );
            }
            Err(e) => {
                eprintln!("[m1nd-mcp][attach] push relay GET failed: {e}");
            }
        }

        // Bounded exponential backoff before reconnecting.
        eprintln!(
            "[m1nd-mcp][attach] push relay reconnecting in {}s",
            backoff_secs
        );
        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(MAX_BACKOFF_SECS);
    }
}

/// Consume a streaming SSE response chunk-by-chunk, splitting it into events on
/// blank lines and forwarding each id-less notification frame to the sink. Holds
/// a rolling buffer so a frame split across TCP chunks is reassembled correctly.
async fn stream_relay_body(
    resp: reqwest::Response,
    sink: &StdoutSink,
    mode: TransportMode,
) -> Result<(), String> {
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();

    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        buf.push_str(&String::from_utf8_lossy(&chunk));

        // Drain every complete event (terminated by a blank line) from the buffer.
        // SSE event separator is "\n\n"; tolerate "\r\n\r\n" too.
        loop {
            let sep = find_event_boundary(&buf);
            let Some((end, sep_len)) = sep else { break };
            let event: String = buf.drain(..end + sep_len).collect();
            relay_one_event(&event, sink, mode);
        }
    }
    Ok(())
}

/// Find the byte offset + length of the first SSE event boundary (a blank line)
/// in `buf`, handling both `\n\n` and `\r\n\r\n`.
fn find_event_boundary(buf: &str) -> Option<(usize, usize)> {
    if let Some(idx) = buf.find("\r\n\r\n") {
        return Some((idx, 4));
    }
    if let Some(idx) = buf.find("\n\n") {
        return Some((idx, 2));
    }
    None
}

/// Parse one SSE event block, concatenating its `data:` lines, and forward it to
/// stdout iff it is a JSON-RPC notification (no/null `id`). Comment/keep-alive
/// lines (leading `:`) and id-bearing response frames are ignored.
fn relay_one_event(event: &str, sink: &StdoutSink, mode: TransportMode) {
    let mut data_lines: Vec<String> = Vec::new();
    for line in event.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() || line.starts_with(':') {
            // Blank line or SSE comment / keep-alive — never JSON-parse.
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
        }
        // `id:`, `event:`, and other fields are not needed for forwarding.
    }
    if data_lines.is_empty() {
        return;
    }
    let payload = data_lines.join("\n");
    let value: serde_json::Value = match serde_json::from_str(&payload) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[m1nd-mcp][attach] push relay: skipping non-JSON SSE data ({e})");
            return;
        }
    };
    // Forward ONLY id-less notifications, so a relayed frame can never collide
    // with a real request/response in the loop.
    let has_id = value.get("id").is_some_and(|v| !v.is_null());
    if has_id {
        return;
    }
    sink.emit_value(value, mode);
}

/// Build a JSON-RPC error response frame.
fn jsonrpc_error(id: serde_json::Value, code: i32, message: String) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message,
            data: None,
        }),
    }
}

/// Parse a `text/event-stream` body, relaying any interim server→client
/// notification frames (no `id`) to stdout via the serialized `sink` and
/// returning the response frame whose `id` matches `want_id` (if found). Falls
/// back to the first response-shaped frame (has `result` or `error`) when no id
/// matches.
fn extract_sse_response(
    body: &str,
    want_id: Option<&serde_json::Value>,
    mode: TransportMode,
    sink: &StdoutSink,
) -> Option<serde_json::Value> {
    let mut matched: Option<serde_json::Value> = None;
    let mut first_response: Option<serde_json::Value> = None;

    for raw in sse_data_payloads(body) {
        let value: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let has_id = value.get("id").is_some_and(|v| !v.is_null());
        let is_response = value.get("result").is_some() || value.get("error").is_some();

        if is_response && has_id {
            if let Some(want) = want_id {
                if value.get("id") == Some(want) && matched.is_none() {
                    matched = Some(value.clone());
                    continue;
                }
            }
            if first_response.is_none() {
                first_response = Some(value);
            }
        } else if !has_id {
            // Interim server→client notification → relay to stdout.
            sink.emit_value(value, mode);
        }
    }

    matched.or(first_response)
}

/// Extract the concatenated `data:` payloads from an SSE body, one logical frame
/// per event (events are separated by a blank line; a frame may span multiple
/// `data:` lines per the SSE spec).
fn sse_data_payloads(body: &str) -> Vec<String> {
    let mut frames = Vec::new();
    let mut current: Vec<String> = Vec::new();

    for line in body.lines() {
        if line.is_empty() {
            if !current.is_empty() {
                frames.push(current.join("\n"));
                current.clear();
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            // Per spec, a single leading space after the colon is stripped.
            current.push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
        }
        // `id:`, `event:`, `:`-comments and other fields are ignored here.
    }
    if !current.is_empty() {
        frames.push(current.join("\n"));
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `StdoutSink` plus the receiver end so tests can inspect every
    /// frame the sink was asked to emit (instead of writing real stdout).
    fn test_sink() -> (StdoutSink, mpsc::UnboundedReceiver<StdoutFrame>) {
        let (tx, rx) = mpsc::unbounded_channel::<StdoutFrame>();
        (StdoutSink { tx }, rx)
    }

    fn drain(rx: &mut mpsc::UnboundedReceiver<StdoutFrame>) -> Vec<serde_json::Value> {
        let mut out = Vec::new();
        while let Ok((v, _)) = rx.try_recv() {
            out.push(v);
        }
        out
    }

    #[test]
    fn sse_single_data_frame_is_extracted() {
        let body = "id: 0\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n\n";
        let frames = sse_data_payloads(body);
        assert_eq!(frames.len(), 1);
        assert!(frames[0].contains("\"ok\":true"));
    }

    #[test]
    fn sse_multiline_data_frame_is_joined() {
        let body = "data: {\"jsonrpc\":\"2.0\",\ndata: \"id\":1,\"result\":{}}\n\n";
        let frames = sse_data_payloads(body);
        assert_eq!(frames.len(), 1);
        // The two data lines join with a newline; the JSON re-parses.
        let v: serde_json::Value = serde_json::from_str(&frames[0]).expect("rejoined JSON parses");
        assert_eq!(v["id"], 1);
    }

    #[test]
    fn extract_picks_response_with_matching_id() {
        // A notification frame followed by the real response frame.
        let body = "data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/x\",\"params\":{}}\n\n\
                    data: {\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{\"v\":42}}\n\n";
        let want = serde_json::json!(7);
        let (sink, mut rx) = test_sink();
        let got = extract_sse_response(body, Some(&want), TransportMode::Line, &sink)
            .expect("matching response found");
        assert_eq!(got["id"], 7);
        assert_eq!(got["result"]["v"], 42);
        // The interim notification was relayed to the sink.
        let relayed = drain(&mut rx);
        assert_eq!(relayed.len(), 1);
        assert_eq!(relayed[0]["method"], "notifications/x");
    }

    #[test]
    fn extract_falls_back_to_first_response_when_no_id_match() {
        let body = "data: {\"jsonrpc\":\"2.0\",\"id\":99,\"result\":{\"v\":1}}\n\n";
        let want = serde_json::json!(1);
        let (sink, _rx) = test_sink();
        let got = extract_sse_response(body, Some(&want), TransportMode::Line, &sink)
            .expect("falls back to first response");
        assert_eq!(got["id"], 99);
    }

    #[test]
    fn jsonrpc_error_has_expected_shape() {
        let err = jsonrpc_error(serde_json::json!(5), -32002, "boom".into());
        assert_eq!(err.jsonrpc, "2.0");
        assert_eq!(err.id, serde_json::json!(5));
        assert!(err.result.is_none());
        let e = err.error.expect("error present");
        assert_eq!(e.code, -32002);
        assert_eq!(e.message, "boom");
    }

    #[test]
    fn relay_forwards_graph_changed_notification() {
        let (sink, mut rx) = test_sink();
        let event = "id: 3\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/m1nd/graph_changed\",\"params\":{\"event\":\"memorize\"}}\n";
        relay_one_event(event, &sink, TransportMode::Line);
        let out = drain(&mut rx);
        assert_eq!(out.len(), 1, "the notification should be forwarded");
        assert_eq!(out[0]["method"], "notifications/m1nd/graph_changed");
        assert!(out[0].get("id").is_none(), "must stay a notification");
        assert_eq!(out[0]["params"]["event"], "memorize");
    }

    #[test]
    fn relay_skips_keepalive_comment() {
        let (sink, mut rx) = test_sink();
        // A bare SSE comment / keep-alive line must never be JSON-parsed/forwarded.
        relay_one_event(":\n", &sink, TransportMode::Line);
        assert!(drain(&mut rx).is_empty());
    }

    #[test]
    fn relay_skips_id_bearing_response() {
        let (sink, mut rx) = test_sink();
        // An id-bearing response belongs to the request/response loop, not the relay.
        let event = "data: {\"jsonrpc\":\"2.0\",\"id\":12,\"result\":{\"ok\":true}}\n";
        relay_one_event(event, &sink, TransportMode::Line);
        assert!(drain(&mut rx).is_empty());
    }

    #[test]
    fn relay_joins_multiline_data() {
        let (sink, mut rx) = test_sink();
        let event = "data: {\"jsonrpc\":\"2.0\",\ndata: \"method\":\"notifications/x\",\"params\":{}}\n";
        relay_one_event(event, &sink, TransportMode::Line);
        let out = drain(&mut rx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["method"], "notifications/x");
    }

    #[test]
    fn event_boundary_handles_lf_and_crlf() {
        assert_eq!(find_event_boundary("data: a\n\nrest"), Some((7, 2)));
        assert_eq!(find_event_boundary("data: a\r\n\r\nrest"), Some((7, 4)));
        assert_eq!(find_event_boundary("data: a\n"), None);
    }
}
