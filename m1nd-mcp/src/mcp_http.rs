// === m1nd-mcp Streamable-HTTP MCP transport (axum) ===
//
// Wave 4, Slice 1: a compliant `POST /mcp` Streamable-HTTP MCP endpoint that
// handles `initialize` + `tools/list` + `tools/call`, returning JSON.
//
// Wave 4, Slice 2: the server→client SSE stream (`GET /mcp`, a real
// `text/event-stream` per the Streamable-HTTP MCP spec) and session
// termination (`DELETE /mcp`). The GET stream is how an attached agent learns
// that ANOTHER agent changed the shared graph — the start of real
// server→agent push. It is deliberately LOW-NOISE: only mutation-class
// broadcast events (the ones that mean "the shared graph changed") are
// relayed as `notifications/m1nd/graph_changed`; an agent never sees an echo
// of its own (or anyone's) read-only tool results.
//
// This transport binds to the SAME shared `Arc<Mutex<SessionState>>` that the
// HTTP server already owns (via `AppState.session`), so a future `--attach`
// client sees the live graph. Tool execution runs under the same
// lock + spawn_blocking + timeout discipline as the REST `handle_tool_call`.
//
// Feature-gated behind "serve".

#![cfg(feature = "serve")]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Bytes,
    http::{HeaderMap, StatusCode},
    response::{sse, IntoResponse, Response, Sse},
};
use futures::stream::StreamExt;
use parking_lot::Mutex;

use crate::http_server::{AppState, SseEvent};
use crate::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use crate::server::handle_mcp_method;

/// Per-spec MCP session header name (case-insensitive on the wire).
const MCP_SESSION_HEADER: &str = "mcp-session-id";

/// Per-tool execution timeout for the HTTP MCP transport (mirrors the REST
/// `TOOL_TIMEOUT_SECS` discipline in `http_server`).
const MCP_TOOL_TIMEOUT_SECS: u64 = 120;

/// Namespaced JSON-RPC method for the one notification this stream emits.
/// Clearly scoped under `m1nd/` so it never collides with a spec method.
const GRAPH_CHANGED_METHOD: &str = "notifications/m1nd/graph_changed";

/// Keepalive interval for the `GET /mcp` SSE stream so proxies / idle clients
/// don't drop a quiet connection.
const MCP_SSE_KEEPALIVE_SECS: u64 = 15;

/// Tools whose successful execution mutates the shared graph / plasticity /
/// disk state in a way ANOTHER attached agent needs to know about. This mirrors
/// the `READ_ONLY_DENIED_TOOLS` set in `server.rs` (the canonical "mutation"
/// boundary) — kept local here so the notification relay's intent is explicit
/// and the relay stays decoupled from the read-only gate's internals.
///
/// LOW-NOISE: a `tool_result` broadcast is relayed ONLY when its `tool` is in
/// this set. Read/analysis tool results (the overwhelming majority of traffic,
/// and the echoes of an agent's own reads) are never pushed.
const GRAPH_MUTATION_TOOLS: &[&str] = &[
    "ingest",
    "apply",
    "apply_batch",
    "edit_commit",
    "memorize",
    "learn",
    "daemon_start",
    "auto_ingest_start",
];

/// Normalize an optional `m1nd.`/`m1nd_` tool prefix, matching the same idiom
/// `server.rs::read_only_denied` uses, so `apply`, `m1nd_apply` and `m1nd.apply`
/// all resolve to the same bare name.
fn bare_tool_name(tool: &str) -> &str {
    tool.strip_prefix("m1nd.")
        .or_else(|| tool.strip_prefix("m1nd_"))
        .unwrap_or(tool)
}

/// Decide whether a broadcast `SseEvent` represents a shared-graph change worth
/// pushing to an attached agent, and if so, build the minimal JSON-RPC
/// notification frame to carry on the SSE `data:` line.
///
/// Returns `None` for everything we deliberately suppress (read tool results,
/// unrelated event types, mutations that did not actually succeed).
fn graph_changed_notification(event: &SseEvent) -> Option<serde_json::Value> {
    let relay_event_name: &str = match event.event_type.as_str() {
        // A finished tool call. Relay only mutation tools, and only when the
        // call actually succeeded (a failed mutation changed nothing).
        "tool_result" => {
            let tool = event.data.get("tool").and_then(|v| v.as_str())?;
            if !GRAPH_MUTATION_TOOLS.contains(&bare_tool_name(tool)) {
                return None;
            }
            // `success` may be absent on older frames; treat absent as success
            // (the event only exists because the tool returned), but an explicit
            // `false` means no mutation landed → suppress.
            if event.data.get("success").and_then(|v| v.as_bool()) == Some(false) {
                return None;
            }
            tool
        }
        // Apply-batch handoff / progress are mutation-only by construction.
        "apply_batch_handoff" | "apply_batch_progress" => event
            .data
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("apply_batch"),
        // A tool that timed out: relay only if it was a mutation tool (a slow
        // read timing out is not a graph change another agent must act on).
        "tool_timeout" => {
            let tool = event.data.get("tool").and_then(|v| v.as_str())?;
            if !GRAPH_MUTATION_TOOLS.contains(&bare_tool_name(tool)) {
                return None;
            }
            tool
        }
        // Everything else (health pings, read results, UI-only events) is noise.
        _ => return None,
    };

    // Minimal, non-echoing detail: enough for the receiving agent to know WHAT
    // changed and re-orient, without replaying the full result payload.
    let mut detail = serde_json::Map::new();
    if let Some(agent_id) = event.data.get("agent_id") {
        detail.insert("agent_id".into(), agent_id.clone());
    }
    if let Some(source) = event.data.get("source") {
        detail.insert("source".into(), source.clone());
    }
    if let Some(batch_id) = event.data.get("batch_id") {
        detail.insert("batch_id".into(), batch_id.clone());
    }
    if let Some(ts) = event.data.get("timestamp_ms") {
        detail.insert("timestamp_ms".into(), ts.clone());
    }
    detail.insert("kind".into(), serde_json::json!(event.event_type));

    Some(serde_json::json!({
        "jsonrpc": "2.0",
        "method": GRAPH_CHANGED_METHOD,
        "params": {
            "event": relay_event_name,
            "detail": serde_json::Value::Object(detail),
        },
    }))
}

/// An MCP *wire* session (Streamable-HTTP transport session).
///
/// This is distinct from:
///   - the instance lease (`SessionState.instance`), and
///   - the per-agent sessions tracked inside `SessionState.sessions`.
///
/// It exists to satisfy the MCP Streamable-HTTP `Mcp-Session-Id` handshake.
/// Room to grow (last-event-id for resumable SSE, etc.) lands in later slices.
#[derive(Clone, Debug)]
pub struct McpTransportSession {
    /// Negotiated protocol version echoed back at `initialize`.
    pub protocol_version: String,
    /// When the session was created (ms since epoch).
    pub created_ms: u64,
    /// Last time we saw a request on this session (ms since epoch).
    pub last_seen_ms: u64,
}

/// Registry of live MCP wire sessions, keyed by opaque session id.
pub type McpSessionRegistry = Arc<Mutex<HashMap<String, McpTransportSession>>>;

/// Build a fresh, empty MCP session registry.
pub fn new_mcp_session_registry() -> McpSessionRegistry {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Current timestamp in milliseconds since epoch.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Monotonic counter so two ids minted in the same millisecond differ.
static MCP_SESSION_SEQ: AtomicU64 = AtomicU64::new(0);

/// Generate an opaque, URL-safe, visible-ASCII MCP session id (128-bit hex).
///
/// No new crate dependency: we combine two `DefaultHasher` digests seeded with
/// process id, wall-clock time, and a per-process atomic sequence — the same
/// idiom already used by `instance_registry::generate_instance_id`. The result
/// is a 32-char lowercase-hex string, which is valid per the MCP spec
/// (visible ASCII, no whitespace).
pub fn generate_mcp_session_id() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let seq = MCP_SESSION_SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let t = now_ms();

    let mut hi = DefaultHasher::new();
    pid.hash(&mut hi);
    t.hash(&mut hi);
    seq.hash(&mut hi);
    "m1nd-mcp-hi".hash(&mut hi);

    let mut lo = DefaultHasher::new();
    seq.hash(&mut lo);
    t.hash(&mut lo);
    pid.hash(&mut lo);
    "m1nd-mcp-lo".hash(&mut lo);

    format!("{:016x}{:016x}", hi.finish(), lo.finish())
}

/// A parsed inbound JSON-RPC message. We need to distinguish requests (have an
/// `id`) from notifications/responses (no `id`) without forcing the strict
/// `JsonRpcRequest` shape, so we keep the raw value too.
enum ParsedMessage {
    /// A request with an `id` and a `method` — expects a JSON-RPC response.
    Request(JsonRpcRequest),
    /// A notification or response (no `id`, or no `method`) — gets `202 Accepted`.
    NotificationOrResponse,
}

/// Build a JSON-RPC error response as an axum `Response` with the given HTTP
/// status. `id` is echoed (or `null` when unknown).
fn jsonrpc_error_response(
    status: StatusCode,
    id: serde_json::Value,
    code: i32,
    message: impl Into<String>,
) -> Response {
    let body = JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.into(),
            data: None,
        }),
    };
    let json = serde_json::to_string(&body).unwrap_or_default();
    (
        status,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        json,
    )
        .into_response()
}

/// Serialize a `JsonRpcResponse` into an `application/json` axum response,
/// optionally attaching the `Mcp-Session-Id` header.
fn jsonrpc_ok_response(resp: &JsonRpcResponse, session_id: Option<&str>) -> Response {
    let json = serde_json::to_string(resp).unwrap_or_default();
    let mut response = (
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        json,
    )
        .into_response();
    if let Some(sid) = session_id {
        if let Ok(value) = axum::http::HeaderValue::from_str(sid) {
            response.headers_mut().insert("mcp-session-id", value);
        }
    }
    response
}

/// Run an MCP request against the shared session under the same
/// lock + spawn_blocking + timeout discipline as the REST `handle_tool_call`.
///
/// The `parking_lot::Mutex` lock is acquired *inside* `spawn_blocking`, so it is
/// never held across an `.await`.
async fn run_mcp_method(app: Arc<AppState>, request: JsonRpcRequest) -> JsonRpcResponse {
    let id = request.id.clone();
    let result = tokio::time::timeout(
        Duration::from_secs(MCP_TOOL_TIMEOUT_SECS),
        tokio::task::spawn_blocking(move || {
            let mut session = app.session.lock();
            handle_mcp_method(&mut session, &request)
        }),
    )
    .await;

    match result {
        Ok(Ok(resp)) => resp,
        Ok(Err(_join_err)) => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32603,
                message: "Internal error: tool task panicked".into(),
                data: None,
            }),
        },
        Err(_elapsed) => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32000,
                message: format!(
                    "Tool execution exceeded {}s timeout",
                    MCP_TOOL_TIMEOUT_SECS
                ),
                data: None,
            }),
        },
    }
}

/// Parse the request body into a JSON-RPC message classification.
fn parse_message(body: &Bytes) -> Result<ParsedMessage, String> {
    let value: serde_json::Value =
        serde_json::from_slice(body).map_err(|e| format!("Invalid JSON: {}", e))?;

    // Batches are not part of the 2025-06-18 single-message flow we implement in
    // Slice 1; reject explicitly rather than silently mishandling.
    if value.is_array() {
        return Err("JSON-RPC batches are not supported".into());
    }

    let has_id = value.get("id").is_some_and(|v| !v.is_null());
    let has_method = value.get("method").and_then(|v| v.as_str()).is_some();

    if has_id && has_method {
        let req: JsonRpcRequest = serde_json::from_value(value)
            .map_err(|e| format!("Malformed JSON-RPC request: {}", e))?;
        Ok(ParsedMessage::Request(req))
    } else {
        // No id (notification) or no method (response) → 202 Accepted.
        Ok(ParsedMessage::NotificationOrResponse)
    }
}

/// Read the `Mcp-Session-Id` request header, if present.
fn session_id_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get(MCP_SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// `POST /mcp` — the Streamable-HTTP MCP request handler.
///
/// Slice 1 scope: `initialize` mints a session and returns the result with the
/// `Mcp-Session-Id` response header; subsequent requests must carry that header.
/// Returns plain `application/json` (no SSE streaming yet).
pub async fn handle_mcp_post(
    axum::extract::State(app): axum::extract::State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let incoming_session = session_id_from_headers(&headers);

    // 1. Parse + classify the message.
    let parsed = match parse_message(&body) {
        Ok(p) => p,
        Err(msg) => {
            return jsonrpc_error_response(
                StatusCode::BAD_REQUEST,
                serde_json::Value::Null,
                -32700,
                msg,
            );
        }
    };

    let request = match parsed {
        // Notifications/responses get 202 Accepted with empty body. If they
        // carry a known session, bump last_seen.
        ParsedMessage::NotificationOrResponse => {
            if let Some(sid) = &incoming_session {
                let mut reg = app.mcp_sessions.lock();
                if let Some(s) = reg.get_mut(sid) {
                    s.last_seen_ms = now_ms();
                }
            }
            return StatusCode::ACCEPTED.into_response();
        }
        ParsedMessage::Request(req) => req,
    };

    // 2. `initialize` — mint a new wire session, run, return with session header.
    if request.method == "initialize" {
        let session_id = generate_mcp_session_id();
        let now = now_ms();

        let response = run_mcp_method(app.clone(), request).await;

        // Record the negotiated protocol version from the result we just built.
        let protocol_version = response
            .result
            .as_ref()
            .and_then(|r| r.get("protocolVersion"))
            .and_then(|v| v.as_str())
            .unwrap_or(crate::server::MCP_PROTOCOL_VERSION)
            .to_string();

        {
            let mut reg = app.mcp_sessions.lock();
            reg.insert(
                session_id.clone(),
                McpTransportSession {
                    protocol_version,
                    created_ms: now,
                    last_seen_ms: now,
                },
            );
        }

        return jsonrpc_ok_response(&response, Some(&session_id));
    }

    // 3. Post-init request — require + validate the session header.
    let session_id = match incoming_session {
        None => {
            return jsonrpc_error_response(
                StatusCode::BAD_REQUEST,
                request.id.clone(),
                -32600,
                "Missing Mcp-Session-Id header",
            );
        }
        Some(sid) => sid,
    };

    {
        let mut reg = app.mcp_sessions.lock();
        match reg.get_mut(&session_id) {
            // Unknown session → 404 signals the client to re-initialize (per spec).
            None => {
                return jsonrpc_error_response(
                    StatusCode::NOT_FOUND,
                    request.id.clone(),
                    -32001,
                    "Unknown or expired Mcp-Session-Id; re-initialize",
                );
            }
            Some(s) => {
                s.last_seen_ms = now_ms();
            }
        }
    }

    // 4. Known session → run the method against the shared graph.
    //
    // Capture the tool name + agent_id BEFORE `run_mcp_method` consumes the
    // request, so that after a successful mutation we can publish a `tool_result`
    // SseEvent onto the broadcast bus. This is the producer side of the
    // server→client push relay: `handle_mcp_get` subscribes to the same bus and
    // forwards `notifications/m1nd/graph_changed` to every attached client. Reads
    // and failed calls publish nothing (`graph_changed_notification` filters to
    // GRAPH_MUTATION_TOOLS + success).
    let mutation_meta = mutation_event_meta(&request);
    let response = run_mcp_method(app.clone(), request).await;
    if let Some((tool, agent_id)) = mutation_meta {
        publish_graph_mutation_event(&app, &tool, agent_id.as_deref(), response.error.is_none());
    }
    jsonrpc_ok_response(&response, None)
}

/// Extract `(tool_name, agent_id)` from a `tools/call` request whose tool is a
/// graph-mutation tool; returns `None` for any other method or a non-mutation
/// tool, so we never publish noise.
fn mutation_event_meta(request: &JsonRpcRequest) -> Option<(String, Option<String>)> {
    if request.method != "tools/call" {
        return None;
    }
    let tool = request.params.get("name").and_then(|v| v.as_str())?;
    if !GRAPH_MUTATION_TOOLS.contains(&bare_tool_name(tool)) {
        return None;
    }
    let agent_id = request
        .params
        .get("arguments")
        .and_then(|a| a.get("agent_id"))
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    Some((tool.to_string(), agent_id))
}

/// Publish a `tool_result` SseEvent for a finished mutation onto the broadcast
/// bus. The shape mirrors the stdio server's `tool_result` event so the shared
/// [`graph_changed_notification`] relay logic forwards it identically. A failed
/// call carries `success:false` and is suppressed downstream.
fn publish_graph_mutation_event(
    app: &Arc<AppState>,
    tool: &str,
    agent_id: Option<&str>,
    success: bool,
) {
    let sse_event = SseEvent {
        event_type: "tool_result".to_string(),
        data: serde_json::json!({
            "tool": tool,
            "source": "mcp_http",
            "agent_id": agent_id,
            "success": success,
            "timestamp_ms": now_ms(),
        }),
    };
    // Best-effort: a send error only means there are no subscribers right now.
    let _ = app.event_tx.send(sse_event);
}

/// Validate the `Mcp-Session-Id` header against the live registry, bumping
/// `last_seen_ms` on success. Mirrors the slice-1 POST validation, factored out
/// so `GET` and `DELETE` share one source of truth.
///
/// Errors are plain-text axum responses with the correct status:
///   - missing header → `400 Bad Request`
///   - unknown / expired id → `404 Not Found` (signals "re-initialize")
///
/// The `parking_lot` lock is held only for the brief get-and-touch; it is never
/// carried across an `.await` (critical for the long-lived SSE stream).
// The `Err` is an axum `Response` (the natural rejection type for these handlers);
// boxing it would only push the allocation onto every caller's happy path.
#[allow(clippy::result_large_err)]
fn validate_session(app: &Arc<AppState>, headers: &HeaderMap) -> Result<String, Response> {
    let session_id = match session_id_from_headers(headers) {
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                "Missing Mcp-Session-Id header",
            )
                .into_response());
        }
        Some(sid) => sid,
    };

    {
        let mut reg = app.mcp_sessions.lock();
        match reg.get_mut(&session_id) {
            None => {
                return Err((
                    StatusCode::NOT_FOUND,
                    "Unknown or expired Mcp-Session-Id; re-initialize",
                )
                    .into_response());
            }
            Some(s) => {
                s.last_seen_ms = now_ms();
            }
        }
    }

    Ok(session_id)
}

/// `GET /mcp` — the server→client Streamable-HTTP SSE stream.
///
/// Per the MCP spec this is a long-lived `text/event-stream` that the server
/// uses to push JSON-RPC messages to the client. Here it carries exactly one
/// kind of message — a `notifications/m1nd/graph_changed` notification — emitted
/// whenever ANOTHER agent mutates the shared graph. It is intentionally
/// low-noise (see [`graph_changed_notification`]): read-only tool results are
/// never relayed.
///
/// Each frame gets an incrementing SSE `id:` (cheap; enables future
/// `Last-Event-ID` resumability — replay itself is NOT implemented in this
/// slice). A periodic keepalive comment keeps idle connections open.
pub async fn handle_mcp_get(
    axum::extract::State(app): axum::extract::State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    // Validate before opening the stream (no lock held across `.await`).
    if let Err(resp) = validate_session(&app, &headers) {
        return resp;
    }

    let rx = app.event_tx.subscribe();
    let mut next_id: u64 = 0;
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(move |event| {
        // Synchronous mapping closure → no `.await`, so the session mutex is
        // never touched here and tool dispatch is never blocked by a slow client.
        let frame = match event {
            Ok(e) => graph_changed_notification(&e),
            // Lagged (slow consumer dropped messages) or closed → skip; the
            // keepalive and subsequent live events keep the stream useful.
            Err(_) => None,
        };
        let item = frame.and_then(|notification| {
            let id = next_id;
            next_id += 1;
            sse::Event::default()
                .id(id.to_string())
                .json_data(notification)
                .ok()
                .map(Ok::<_, std::convert::Infallible>)
        });
        async move { item }
    });

    Sse::new(stream)
        .keep_alive(
            sse::KeepAlive::new().interval(Duration::from_secs(MCP_SSE_KEEPALIVE_SECS)),
        )
        .into_response()
}

/// `DELETE /mcp` — explicit session termination per the Streamable-HTTP spec.
///
/// Validates the `Mcp-Session-Id`; on a known session, removes it from the
/// registry and returns `200 OK` with an empty body. Missing header → `400`,
/// unknown id → `404`.
pub async fn handle_mcp_delete(
    axum::extract::State(app): axum::extract::State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    let session_id = match validate_session(&app, &headers) {
        Ok(sid) => sid,
        Err(resp) => return resp,
    };

    {
        let mut reg = app.mcp_sessions.lock();
        reg.remove(&session_id);
    }

    StatusCode::OK.into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::{tool_schemas, McpConfig};
    use crate::session::SessionState;
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::Graph;
    use tokio::sync::broadcast;

    fn ev(event_type: &str, data: serde_json::Value) -> SseEvent {
        SseEvent {
            event_type: event_type.to_string(),
            data,
        }
    }

    // ---- Low-noise relay decision -----------------------------------------

    #[test]
    fn read_tool_result_is_not_relayed() {
        // A `seek` (read) result must produce no notification — this is the
        // whole point: an agent never sees an echo of a read.
        let e = ev(
            "tool_result",
            serde_json::json!({"tool": "seek", "success": true, "agent_id": "a"}),
        );
        assert!(graph_changed_notification(&e).is_none());
    }

    #[test]
    fn mutation_tool_result_is_relayed_with_namespaced_method() {
        let e = ev(
            "tool_result",
            serde_json::json!({
                "tool": "memorize",
                "success": true,
                "agent_id": "agent-b",
                "source": "http",
                "timestamp_ms": 1234,
            }),
        );
        let frame = graph_changed_notification(&e).expect("memorize relays");
        assert_eq!(frame["jsonrpc"], "2.0");
        assert_eq!(frame["method"], "notifications/m1nd/graph_changed");
        assert_eq!(frame["params"]["event"], "memorize");
        assert_eq!(frame["params"]["detail"]["agent_id"], "agent-b");
        assert_eq!(frame["params"]["detail"]["kind"], "tool_result");
    }

    #[test]
    fn prefixed_mutation_tool_is_relayed() {
        for tool in ["m1nd.apply", "m1nd_apply", "apply"] {
            let e = ev(
                "tool_result",
                serde_json::json!({"tool": tool, "success": true}),
            );
            assert!(
                graph_changed_notification(&e).is_some(),
                "{} should relay",
                tool
            );
        }
    }

    #[test]
    fn failed_mutation_is_suppressed() {
        // A mutation that did not succeed changed nothing → no push.
        let e = ev(
            "tool_result",
            serde_json::json!({"tool": "ingest", "success": false}),
        );
        assert!(graph_changed_notification(&e).is_none());
    }

    #[test]
    fn apply_batch_handoff_and_progress_relay() {
        for et in ["apply_batch_handoff", "apply_batch_progress"] {
            let e = ev(et, serde_json::json!({"tool": "apply_batch", "batch_id": "b1"}));
            let frame = graph_changed_notification(&e).expect("relays");
            assert_eq!(frame["params"]["event"], "apply_batch");
            assert_eq!(frame["params"]["detail"]["batch_id"], "b1");
        }
    }

    #[test]
    fn read_tool_timeout_is_not_relayed_but_mutation_timeout_is() {
        let read = ev("tool_timeout", serde_json::json!({"tool": "seek"}));
        assert!(graph_changed_notification(&read).is_none());

        let mutation = ev("tool_timeout", serde_json::json!({"tool": "ingest"}));
        assert!(graph_changed_notification(&mutation).is_some());
    }

    #[test]
    fn unrelated_event_types_are_dropped() {
        for et in ["health", "heartbeat", "ui_refresh", "instance_changed"] {
            let e = ev(et, serde_json::json!({"foo": "bar"}));
            assert!(
                graph_changed_notification(&e).is_none(),
                "{} must not relay",
                et
            );
        }
    }

    // ---- Handler behavior (validation / termination) ----------------------

    fn build_app_state(root: &std::path::Path) -> Arc<AppState> {
        let runtime_dir = root.join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..Default::default()
        };
        let session = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");
        let (event_tx, _) = broadcast::channel::<SseEvent>(16);
        let tool_schemas_cache = tool_schemas()
            .get("tools")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        Arc::new(AppState {
            session: Arc::new(Mutex::new(session)),
            tool_schemas_cache,
            event_tx,
            event_log_path: None,
            registry_dir: None,
            mcp_sessions: new_mcp_session_registry(),
        })
    }

    fn header_map_with_session(sid: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(MCP_SESSION_HEADER, sid.parse().unwrap());
        h
    }

    fn seed_session(app: &Arc<AppState>) -> String {
        let sid = generate_mcp_session_id();
        let now = now_ms();
        app.mcp_sessions.lock().insert(
            sid.clone(),
            McpTransportSession {
                protocol_version: "test".into(),
                created_ms: now,
                last_seen_ms: now,
            },
        );
        sid
    }

    #[tokio::test]
    async fn get_missing_session_is_400() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = build_app_state(temp.path());
        let resp =
            handle_mcp_get(axum::extract::State(app), HeaderMap::new()).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn get_unknown_session_is_404() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = build_app_state(temp.path());
        let headers = header_map_with_session("does-not-exist");
        let resp = handle_mcp_get(axum::extract::State(app), headers).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn get_known_session_opens_event_stream() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = build_app_state(temp.path());
        let sid = seed_session(&app);
        let headers = header_map_with_session(&sid);
        let resp = handle_mcp_get(axum::extract::State(app), headers).await;
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert!(
            ct.starts_with("text/event-stream"),
            "expected SSE content-type, got {ct}"
        );
    }

    #[tokio::test]
    async fn delete_missing_session_is_400() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = build_app_state(temp.path());
        let resp =
            handle_mcp_delete(axum::extract::State(app), HeaderMap::new()).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn delete_unknown_session_is_404() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = build_app_state(temp.path());
        let headers = header_map_with_session("nope");
        let resp = handle_mcp_delete(axum::extract::State(app), headers).await;
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_removes_session_then_revalidation_is_404() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = build_app_state(temp.path());
        let sid = seed_session(&app);

        // DELETE → 200 and session gone from registry.
        let resp =
            handle_mcp_delete(axum::extract::State(app.clone()), header_map_with_session(&sid))
                .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(!app.mcp_sessions.lock().contains_key(&sid));

        // A subsequent GET with the now-dead session id → 404.
        let resp2 =
            handle_mcp_get(axum::extract::State(app.clone()), header_map_with_session(&sid)).await;
        assert_eq!(resp2.status(), StatusCode::NOT_FOUND);

        // And a POST tools/list with that session → 404 (matches the probe's
        // post-delete acceptance check).
        let body = axum::body::Bytes::from(
            serde_json::to_vec(&serde_json::json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/list"
            }))
            .unwrap(),
        );
        let resp3 = handle_mcp_post(
            axum::extract::State(app),
            header_map_with_session(&sid),
            body,
        )
        .await;
        assert_eq!(resp3.status(), StatusCode::NOT_FOUND);
    }
}
