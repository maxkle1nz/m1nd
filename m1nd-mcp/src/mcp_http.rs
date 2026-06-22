// === m1nd-mcp Streamable-HTTP MCP transport (axum) ===
//
// Wave 4, Slice 1: a compliant `POST /mcp` Streamable-HTTP MCP endpoint that
// handles `initialize` + `tools/list` + `tools/call`, returning JSON
// (no SSE / GET / DELETE yet — those land in Slice 2).
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
    response::{IntoResponse, Response},
};
use parking_lot::Mutex;

use crate::http_server::AppState;
use crate::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use crate::server::handle_mcp_method;

/// Per-spec MCP session header name (case-insensitive on the wire).
const MCP_SESSION_HEADER: &str = "mcp-session-id";

/// Per-tool execution timeout for the HTTP MCP transport (mirrors the REST
/// `TOOL_TIMEOUT_SECS` discipline in `http_server`).
const MCP_TOOL_TIMEOUT_SECS: u64 = 120;

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
    let response = run_mcp_method(app, request).await;
    jsonrpc_ok_response(&response, None)
}
