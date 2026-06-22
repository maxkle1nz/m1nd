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

use crate::protocol::{JsonRpcError, JsonRpcResponse};
use crate::server::{read_request_payload, write_response, TransportMode};

/// Per-spec MCP session header name.
const MCP_SESSION_HEADER: &str = "mcp-session-id";
/// Per-spec negotiated protocol version header.
const MCP_PROTOCOL_VERSION_HEADER: &str = "mcp-protocol-version";

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
                emit(&err, mode);
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
                    emit(&err, mode);
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
                emit(&err, mode);
                continue;
            }
        };

        // --- Demux by content-type. ---
        let response_value: Option<serde_json::Value> = if content_type.contains("text/event-stream")
        {
            // SSE: extract the JSON-RPC response frame whose `id` matches the
            // request; relay any interim server→client notifications to stdout.
            extract_sse_response(&body, req_id.as_ref(), mode)
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
                emit_value(&v, mode);
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
                emit(&err, mode);
            }
        }
    }

    // TODO(slice4): a long-lived `GET /mcp` push relay — subscribe to the owner's
    // server→client SSE stream and forward `notifications/m1nd/graph_changed`
    // frames to this host's stdout, so an attached agent learns that ANOTHER
    // agent mutated the shared graph without polling. The request/response bridge
    // (the must-have for shared-graph mutation visibility) is complete above;
    // the push relay is deliberately deferred.
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

/// Write a typed `JsonRpcResponse` to stdout using the embedded server's framing
/// primitive (verbatim), in the inbound `TransportMode`.
fn emit(resp: &JsonRpcResponse, mode: TransportMode) {
    let stdout = std::io::stdout();
    let mut writer = stdout.lock();
    if write_response(&mut writer, resp, mode).is_err() {
        eprintln!("[m1nd-mcp][attach] stdout closed while writing response");
    }
}

/// Write an arbitrary JSON value (a full JSON-RPC frame received from the owner)
/// to stdout in the inbound framing mode. We re-frame using the SAME logic
/// `write_response` uses (Content-Length vs newline) so stdout is byte-identical
/// to the embedded server — the value is forwarded as-is, preserving the owner's
/// exact `result`/`error` shape.
fn emit_value(value: &serde_json::Value, mode: TransportMode) {
    let json = serde_json::to_string(value).unwrap_or_default();
    let stdout = std::io::stdout();
    let mut writer = stdout.lock();
    let write_res = match mode {
        TransportMode::Framed => write!(writer, "Content-Length: {}\r\n\r\n{}", json.len(), json)
            .and_then(|_| writer.flush()),
        TransportMode::Line => writeln!(writer, "{}", json).and_then(|_| writer.flush()),
    };
    if write_res.is_err() {
        eprintln!("[m1nd-mcp][attach] stdout closed while writing frame");
    }
}

/// Parse a `text/event-stream` body, relaying any interim server→client
/// notification frames (no `id`) to stdout and returning the response frame whose
/// `id` matches `want_id` (if found). Falls back to the first response-shaped
/// frame (has `result` or `error`) when no id matches.
fn extract_sse_response(
    body: &str,
    want_id: Option<&serde_json::Value>,
    mode: TransportMode,
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
            emit_value(&value, mode);
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
        let got = extract_sse_response(body, Some(&want), TransportMode::Line)
            .expect("matching response found");
        assert_eq!(got["id"], 7);
        assert_eq!(got["result"]["v"], 42);
    }

    #[test]
    fn extract_falls_back_to_first_response_when_no_id_match() {
        let body = "data: {\"jsonrpc\":\"2.0\",\"id\":99,\"result\":{\"v\":1}}\n\n";
        let want = serde_json::json!(1);
        let got = extract_sse_response(body, Some(&want), TransportMode::Line)
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
}
