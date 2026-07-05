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
use crate::server::{dispatch_tool, handle_mcp_method};
use crate::session::SessionState;
use crate::util::now_ms;

/// Per-spec MCP session header name (case-insensitive on the wire).
const MCP_SESSION_HEADER: &str = "mcp-session-id";
/// Hop-2 caller-root header the `--attach` bridge stamps (TWO-TIER-BRAIN-PRD
/// §9.5.4). Absent → the caller is unknown (legacy bridge / direct HTTP).
const CALLER_ROOT_HEADER: &str = "m1nd-caller-root";

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

/// The canonical "is this broadcast event a shared-graph mutation?" boundary.
///
/// Returns the relay event name (`memorize`, `ingest`, `apply_batch`, …) when the
/// event means the shared graph actually changed, and `None` for everything we
/// deliberately suppress (read tool results, unrelated event types, mutations that
/// did not actually succeed). This is the ONE mutation-detection predicate, shared
/// by two renderings: the MCP `graph_changed_notification` (JSON-RPC frame for
/// attached agents) and the browser `/api/events` `graph_changed` relay
/// (`http_server::browser_graph_changed_event` — the #233 pure-reader gap fix).
pub(crate) fn graph_mutation_event_name(event: &SseEvent) -> Option<&str> {
    match event.event_type.as_str() {
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
            Some(tool)
        }
        // Apply-batch handoff / progress are mutation-only by construction.
        "apply_batch_handoff" | "apply_batch_progress" => Some(
            event
                .data
                .get("tool")
                .and_then(|v| v.as_str())
                .unwrap_or("apply_batch"),
        ),
        // A tool that timed out: relay only if it was a mutation tool (a slow
        // read timing out is not a graph change another agent must act on).
        "tool_timeout" => {
            let tool = event.data.get("tool").and_then(|v| v.as_str())?;
            if !GRAPH_MUTATION_TOOLS.contains(&bare_tool_name(tool)) {
                return None;
            }
            Some(tool)
        }
        // Everything else (health pings, read results, UI-only events) is noise.
        _ => None,
    }
}

/// Decide whether a broadcast `SseEvent` represents a shared-graph change worth
/// pushing to an attached agent, and if so, build the minimal JSON-RPC
/// notification frame to carry on the SSE `data:` line.
///
/// Returns `None` for everything we deliberately suppress (read tool results,
/// unrelated event types, mutations that did not actually succeed).
fn graph_changed_notification(event: &SseEvent) -> Option<serde_json::Value> {
    let relay_event_name: &str = graph_mutation_event_name(event)?;

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

/// Does this broadcast event originate from wire session `viewer`? True only when
/// the event carries an [`ORIGIN_SESSION_FIELD`] equal to `viewer`. Used by the
/// GET/SSE relay to suppress a client's own mutation (field-triage L21) — an event
/// with no origin stamp (older/other producers) is NOT anyone's own, so it is never
/// suppressed and relays to everyone exactly as before.
fn event_origin_is(event: &SseEvent, viewer: &str) -> bool {
    event
        .data
        .get(ORIGIN_SESSION_FIELD)
        .and_then(|v| v.as_str())
        == Some(viewer)
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
    /// The caller's resolved root from the hop-2 `M1nd-Caller-Root` header
    /// (TWO-TIER-BRAIN-PRD §9.5.4). `None` = the bridge/caller sent no header
    /// (legacy bridge / direct HTTP) → the owner treats first contact as unknown.
    /// Read at `initialize` and refreshed per-request if the header is present.
    pub caller_root: Option<String>,
    /// Two-Tier Brain (interim): the canonicalized project root of the
    /// per-project brain this wire session is bound to. Set by the one-call
    /// bootstrap or by the first caller_root auto-match; sticky for the session's
    /// lifetime (§9.5.2 "never re-ask" — mid-session cwd travel is deliberately
    /// NOT re-detected, the scope-guard backstop covers it). `None` = the session
    /// rides the owner's bound graph, exactly as before this feature.
    pub bound_project_root: Option<String>,
}

/// Registry of live MCP wire sessions, keyed by opaque session id.
pub type McpSessionRegistry = Arc<Mutex<HashMap<String, McpTransportSession>>>;

/// Build a fresh, empty MCP session registry.
pub fn new_mcp_session_registry() -> McpSessionRegistry {
    Arc::new(Mutex::new(HashMap::new()))
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
///
/// `caller_root` is this call's resolved hop-2 `M1nd-Caller-Root` (§9.5.4): it is
/// stamped onto the shared `SessionState` on EVERY call (set to the passed value
/// each time, so a later call without the header cannot inherit a stale root)
/// before dispatch, feeding First-Contact Reception's mismatch verdict.
async fn run_mcp_method(
    app: Arc<AppState>,
    request: JsonRpcRequest,
    caller_root: Option<String>,
) -> JsonRpcResponse {
    let id = request.id.clone();
    let result = tokio::time::timeout(
        Duration::from_secs(MCP_TOOL_TIMEOUT_SECS),
        tokio::task::spawn_blocking(move || {
            let mut session = app.session.lock();
            session.caller_root = caller_root;
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
                message: format!("Tool execution exceeded {}s timeout", MCP_TOOL_TIMEOUT_SECS),
                data: None,
            }),
        },
    }
}

// ---------------------------------------------------------------------------
// Two-Tier Brain (interim) — per-call brain routing + the one-call bootstrap.
// ---------------------------------------------------------------------------

/// When this request is the one-call bootstrap (`tools/call` on `ingest` with a
/// non-empty `project_root`), return that root. Everything else → `None`.
fn bootstrap_project_root(request: &JsonRpcRequest) -> Option<String> {
    if request.method != "tools/call" {
        return None;
    }
    let name = request.params.get("name")?.as_str()?;
    if bare_tool_name(name) != "ingest" {
        return None;
    }
    let root = request
        .params
        .get("arguments")?
        .get("project_root")?
        .as_str()?
        .trim();
    if root.is_empty() {
        None
    } else {
        Some(root.to_string())
    }
}

/// When this request is a `promote` call, parse its arguments into a
/// [`PromoteInput`](crate::promote_handlers::PromoteInput). `promote` is an
/// OWNER-LEVEL cross-store verb (reads a project brain, writes the medulla), so it
/// is handled at the routing seam — not by a single-store tool handler. Returns
/// `None` for every other request.
fn promote_request(request: &JsonRpcRequest) -> Option<crate::promote_handlers::PromoteInput> {
    if request.method != "tools/call" {
        return None;
    }
    let name = request.params.get("name")?.as_str()?;
    if bare_tool_name(name) != "promote" {
        return None;
    }
    let args = request.params.get("arguments")?;
    Some(crate::promote_handlers::PromoteInput {
        agent_id: args
            .get("agent_id")
            .and_then(|v| v.as_str())
            .unwrap_or("promote")
            .to_string(),
        brain: args
            .get("brain")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        claim: args
            .get("claim")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        reason: args
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

/// Execute a `promote` verb (MEDULLA-PRD §7). Resolves the SOURCE brain's store
/// dir + the MEDULLA store dir (the bound owner's `agent-memory`), then runs the
/// pure [`promote_claim`](crate::promote_handlers::promote_claim) logic across
/// them. A single tool handler holds one `&mut SessionState` and cannot reach two
/// stores, so — exactly like the one-call bootstrap — the crossing lives here.
fn run_promote(
    app: &Arc<AppState>,
    request: &JsonRpcRequest,
    input: &crate::promote_handlers::PromoteInput,
) -> JsonRpcResponse {
    let id = request.id.clone();

    if input.brain.trim().is_empty() || input.claim.trim().is_empty() {
        return tool_error_response(
            id,
            "promote requires a non-empty `brain` (the source project root) and `claim` (the slug to promote)".into(),
        );
    }

    // The MEDULLA store: the bound owner's own agent-memory dir + its runtime root
    // (where the medulla's .locks/.history live). This is the shared doctrine store
    // every session's default beat reads.
    let (medulla_runtime_root, medulla_store_dir, brain_is_bound) = {
        let session = app.session.lock();
        let rr = session.runtime_root.clone();
        let store = rr.join("agent-memory");
        // A brain the bound owner covers IS the medulla — promoting from it to
        // itself is a no-op the caller should not attempt.
        let covered = session.covers_root(&input.brain);
        (rr, store, covered)
    };

    // Resolve the SOURCE brain's store dir. Two shapes:
    //  - a hosted PROJECT brain → its store dir under project-brains/;
    //  - the bound owner (medulla) itself → refused (can't promote medulla→medulla).
    if brain_is_bound {
        return tool_error_response(
            id,
            format!(
                "brain '{}' is the owner's bound graph (the medulla itself) — a claim there is \
                 already doctrine; there is nothing to promote UP to.",
                input.brain
            ),
        );
    }
    let canonical = crate::project_brains::ProjectBrainRegistry::canonical_key(&input.brain);
    if !app.project_brains.knows(&canonical) {
        return tool_error_response(
            id,
            format!(
                "no project brain for '{}' — bootstrap it first (ingest with project_root=...), \
                 or check the path. Promotion reads a real, hosted source store.",
                input.brain
            ),
        );
    }
    let source_store_dir = app
        .project_brains
        .store_dir_for(&canonical)
        .join("agent-memory");

    match crate::promote_handlers::promote_claim(
        input,
        &source_store_dir,
        &medulla_store_dir,
        &medulla_runtime_root,
    ) {
        Ok(outcome) => {
            // Re-ingest the medulla copy so it is immediately recallable in the
            // default beat (the R3 tier=medulla path reads the bound owner's graph).
            {
                let mut session = app.session.lock();
                let ingest = crate::protocol::core::IngestInput {
                    path: outcome.medulla_path.to_string_lossy().to_string(),
                    agent_id: input.agent_id.clone(),
                    incremental: false,
                    adapter: "light".into(),
                    mode: "merge".into(),
                    namespace: Some("light".into()),
                    include_dotfiles: false,
                    dotfile_patterns: vec![],
                    project_root: None,
                };
                // Best-effort: a failed re-ingest never loses the durable file (it
                // is on disk + will load next boot); it only delays recall.
                let _ = crate::tools::handle_ingest(&mut session, ingest);
            }
            let payload = crate::promote_handlers::promote_response(input, &outcome);
            tool_result_response(id, &payload)
        }
        Err(e) => tool_error_response(id, e.to_string()),
    }
}

/// Wrap a successful tool payload as an MCP `tools/call` result — the exact
/// shape `handle_mcp_method` emits, so bootstrap responses are indistinguishable
/// from any other tool result on the wire.
fn tool_result_response(id: serde_json::Value, payload: &serde_json::Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: Some(serde_json::json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(payload).unwrap_or_default(),
            }]
        })),
        error: None,
    }
}

/// Wrap a tool-level failure as MCP `isError` content (spec: tool execution
/// errors are content, not JSON-RPC protocol errors) — mirrors the
/// `dispatch_tool` Err arm in `handle_mcp_method`.
fn tool_error_response(id: serde_json::Value, message: String) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0".into(),
        id,
        result: Some(serde_json::json!({
            "content": [{ "type": "text", "text": format!("Error: {}", message) }],
            "isError": true
        })),
        error: None,
    }
}

/// The one-call bootstrap (TWO-TIER-BRAIN interim; the reception option made
/// real): create (or warm-resolve) the per-project brain for `project_root`,
/// ingest the caller's repo into it, bind THIS wire session to it (sticky), and
/// return the new brain's north packet in the SAME response — total friction =
/// one call. The owner's bound graph is never touched.
///
/// Runs inside the caller's `spawn_blocking` context (ingest + engine build are
/// CPU-bound).
fn run_bootstrap(
    app: &Arc<AppState>,
    request: &JsonRpcRequest,
    project_root: &str,
    session_id: Option<&str>,
) -> JsonRpcResponse {
    let arguments = request
        .params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

    // Guard: a root the BOUND brain already covers needs no project brain — and
    // silently shadowing the dev graph would be worse than refusing. One honest
    // error, one next action.
    let bound_covers = { app.session.lock().covers_root(project_root) };
    if bound_covers {
        return tool_error_response(
            request.id.clone(),
            format!(
                "project_root {project_root} is already covered by this owner's bound graph — \
                 you are home; call verbs directly (bootstrap refused so the bound brain is \
                 never shadowed by a duplicate)"
            ),
        );
    }

    match app.project_brains.bootstrap(project_root, &arguments) {
        Err(e) => tool_error_response(
            request.id.clone(),
            format!("one-call bootstrap of {project_root} failed: {e}"),
        ),
        Ok((brain, ingest_result, reused)) => {
            let key = crate::project_brains::ProjectBrainRegistry::canonical_key(project_root);

            // Sticky: this wire session now belongs to the new brain (§9.5.2).
            if let Some(sid) = session_id {
                if let Some(s) = app.mcp_sessions.lock().get_mut(sid) {
                    s.bound_project_root = Some(key.clone());
                }
            }

            // Orient in the same response — north-grade, from the NEW brain.
            let agent_id = arguments
                .get("agent_id")
                .and_then(|v| v.as_str())
                .unwrap_or("bootstrap")
                .to_string();
            let north = {
                let mut state = brain.lock();
                state.caller_root = Some(key.clone());
                crate::server::dispatch_tool(
                    &mut state,
                    "north",
                    &serde_json::json!({
                        "agent_id": agent_id,
                        "task": format!(
                            "first orientation of the {key} project brain right after its one-call bootstrap"
                        ),
                    }),
                )
                .unwrap_or_else(|e| {
                    serde_json::json!({
                        "error": format!("north after bootstrap failed: {e}"),
                    })
                })
            };

            let packet = serde_json::json!({
                "schema": "m1nd-project-brain-bootstrap-v0",
                "project_root": key,
                "store_dir": app.project_brains.store_dir_for(&key),
                "reused_existing_brain": reused,
                "ingest": ingest_result,
                "north": north,
                "routing": "this wire session is now bound to your project brain; any call — this session or a brand NEW session — whose resolved caller root is this repo routes here automatically, silent on match (TT-INV-12)",
            });
            tool_result_response(request.id.clone(), &packet)
        }
    }
}

/// Route a post-`initialize` MCP request to the brain that owns the caller, then
/// dispatch it — the Two-Tier routing seam. Precedence, per call:
///
///   1. one-call bootstrap (`ingest` + `project_root`) → create/resolve brain,
///      bind session, respond with the bootstrap packet;
///   2. session sticky choice (`bound_project_root`) → that brain (a vanished
///      brain falls through to the bound graph honestly);
///   3. resolved caller_root: under the bound graph's roots → bound (TT-INV-12
///      silence, exactly today's behavior); else a known project brain (live or
///      warm-bootable store) → that brain, silently, and the session goes sticky;
///   4. default → the bound graph (whose reception verdict flags true unknowns).
///
/// Same lock + spawn_blocking + timeout discipline as `run_mcp_method`; no lock
/// is ever held across `.await` and no two session locks are held at once.
async fn route_and_run(
    app: Arc<AppState>,
    request: JsonRpcRequest,
    caller_root: Option<String>,
    session_id: String,
) -> JsonRpcResponse {
    let id = request.id.clone();
    let result = tokio::time::timeout(
        Duration::from_secs(MCP_TOOL_TIMEOUT_SECS),
        tokio::task::spawn_blocking(move || {
            // 1. The one-call bootstrap.
            if let Some(project_root) = bootstrap_project_root(&request) {
                return run_bootstrap(&app, &request, &project_root, Some(&session_id));
            }

            // 1b. The `promote` verb (MEDULLA-PRD §7) — an owner-level cross-store
            //     crossing (read a project brain, write the medulla). Like the
            //     bootstrap, it runs at the seam, before per-session routing.
            if let Some(promote) = promote_request(&request) {
                return run_promote(&app, &request, &promote);
            }

            // 2. Sticky per-session choice.
            let sticky = {
                let sessions = app.mcp_sessions.lock();
                sessions
                    .get(&session_id)
                    .and_then(|s| s.bound_project_root.clone())
            };
            if let Some(root) = sticky {
                if let Some(brain) = app.project_brains.resolve(&root) {
                    // Served by a PROJECT brain → its default beat is project + medulla,
                    // and it can fan out to `all-brains` (MEDULLA-PRD §5); compose runs
                    // AROUND the primary dispatch, holding one lock at a time.
                    return serve_and_compose(&app, brain, &request, caller_root.clone(), true);
                }
                // Brain store vanished mid-session — fall through to the bound
                // graph, whose reception will say so honestly.
            }

            // 3. Automatic recognition by resolved caller root.
            if let Some(root) = caller_root.as_deref() {
                let bound_covers = { app.session.lock().covers_root(root) };
                if !bound_covers {
                    if let Some(brain) = app.project_brains.resolve(root) {
                        let key = crate::project_brains::ProjectBrainRegistry::canonical_key(root);
                        if let Some(s) = app.mcp_sessions.lock().get_mut(&session_id) {
                            s.bound_project_root = Some(key);
                        }
                        return serve_and_compose(&app, brain, &request, caller_root.clone(), true);
                    }
                }
            }

            // 4. Default: the bound graph (reception flags true unknowns there). This
            // IS the medulla store today — its own beat is already project+medulla by
            // identity; `all-brains` still fans out to the hosted project brains.
            //
            // RECONNECT-REBIND (§C5.4, ladder R13): the owner-default reception says
            // "this graph does NOT cover your repo" and, before this rung, suggested
            // `ingest project_root=<caller_root>` — the host cwd. After an MCP
            // reconnect that cwd collapses to the host launch dir, an ANCESTOR of the
            // real repo (letter#49). If the disk roster holds exactly ONE known brain
            // related to the caller by ancestry, we rewrite the mismatch reception to
            // name THAT brain — the existing brain is preferred over the owner graph.
            // The roster consult happens only on the mismatch path (step 4), never on
            // a match (steps 2/3 already routed + returned), so TT-INV-12 silence is
            // untouched; 0 or >1 related brains leave the reception exactly as today.
            let response = serve_and_compose(
                &app,
                app.session.clone(),
                &request,
                caller_root.clone(),
                false,
            );
            enrich_reception_with_roster(response, &app, caller_root.as_deref())
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
                message: format!("Tool execution exceeded {}s timeout", MCP_TOOL_TIMEOUT_SECS),
                data: None,
            }),
        },
    }
}

// =========================================================================
// MEDULLA M5b — pull-only tier recall (the read side of the medulla)
// =========================================================================
//
// The routing layer is the ONLY place that can compose ACROSS stores: a tool
// handler holds a single `&mut SessionState` and structurally cannot read a
// sibling brain. `serve_and_compose` dispatches the tool on the primary (routed)
// brain, then — for the memory-recall tools only — folds in the other stores the
// tier selector names, each row labeled with its `origin_brain` (MEDULLA-PRD §6).
//
// THE LEAK INVARIANT (MED-INV-1), made mechanical here: a brain X default beat
// composes exactly X's own store + the medulla — no third store is ever read
// unless the caller passes `tier:"all-brains"`. So a claim from brain Y can reach
// X's default beat only if it lives in the medulla (promoted / doctrine-born).
// Pull, never push.
//
// Lock discipline (route_and_run's contract): NEVER hold two session locks at
// once. Each store is locked, queried, and released before the next is touched;
// the primary lock is dropped before any sibling is read.

/// Tools whose payloads carry a durable-memory feed that tier recall composes.
/// `seek` folds into `results`; `north` into `memory`; `boot_memory` (list) into
/// `entries`; `delegate` folds the medulla doctrine feed into its NESTED
/// `context.memory` slice (M7 · ORGANISM R7) so a delegation packet's inherited
/// memory carries doctrine beside project fact, each row tier/origin-labeled. Every
/// other tool is served verbatim by the primary brain.
fn is_tier_recall_tool(tool: &str) -> bool {
    matches!(
        bare_tool_name(tool),
        "seek" | "north" | "boot_memory" | "delegate"
    )
}

/// The memory tier a caller asked for (MEDULLA-PRD §5.2). Absent / unknown →
/// the default beat (`project + medulla`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum MemoryTier {
    /// This brain's own store only.
    Project,
    /// The medulla (promoted/doctrine) store only.
    Medulla,
    /// Default: this brain's store + the medulla.
    ProjectAndMedulla,
    /// The explicit cross-project fan-out — every hosted store, labeled by origin.
    AllBrains,
}

impl MemoryTier {
    /// Read the `tier` argument off a `tools/call` request. Absent, empty, or
    /// unrecognized → the default beat (never an error — an unknown tier must not
    /// break recall, and must never silently widen past the default, §12 risk 1).
    fn from_request(request: &JsonRpcRequest) -> MemoryTier {
        let raw = request
            .params
            .get("arguments")
            .and_then(|a| a.get("tier"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        match raw {
            "project" => MemoryTier::Project,
            "medulla" => MemoryTier::Medulla,
            "all-brains" | "all_brains" => MemoryTier::AllBrains,
            // "" | "project+medulla" | anything else → the safe default.
            _ => MemoryTier::ProjectAndMedulla,
        }
    }
}

/// Serve `request` on `primary` (the routed brain), then compose the tier-selected
/// memory feed from the other stores. `primary_is_project` is true when `primary`
/// is a per-project brain (its own beat is `project`); false when it is the bound
/// owner, which IS the medulla today (its own beat is already `project+medulla` by
/// identity). Non-recall tools and the `project` tier are served verbatim.
fn serve_and_compose(
    app: &Arc<AppState>,
    primary: Arc<Mutex<SessionState>>,
    request: &JsonRpcRequest,
    caller_root: Option<String>,
    primary_is_project: bool,
) -> JsonRpcResponse {
    // Non-recall tools, or a non-`tools/call` method: serve verbatim, one lock.
    let tool_name = if request.method == "tools/call" {
        request.params.get("name").and_then(|v| v.as_str())
    } else {
        None
    };
    let Some(tool) = tool_name.filter(|t| is_tier_recall_tool(t)) else {
        let mut state = primary.lock();
        state.caller_root = caller_root;
        return handle_mcp_method(&mut state, request);
    };
    let tier = MemoryTier::from_request(request);

    // 1. PRIMARY dispatch — the routed brain's own answer (its full envelope). We
    //    dispatch the tool directly to get the RAW payload (not the wrapped text),
    //    so composing is a structured merge, not string surgery. The lock is
    //    released before any sibling store is read (lock discipline).
    let id = request.id.clone();
    let (mut payload, primary_origin) = {
        let mut state = primary.lock();
        state.caller_root = caller_root.clone();
        let args = request
            .params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        // Mirror handle_mcp_method's per-call agent tracking (the read-only recall
        // tools do not autotick the daemon, so nothing else here diverges).
        if let Some(aid) = args.get("agent_id").and_then(|v| v.as_str()) {
            state.track_agent(aid);
        }
        let origin = state.origin_brain();
        match dispatch_tool(&mut state, bare_tool_name(tool), &args) {
            Ok(v) => (v, origin),
            // Tool-level failure: return it verbatim (no compose over an error).
            Err(e) => return tool_error_response(id, e.to_string()),
        }
    };
    // `tier:"project"` — the primary brain's own beat, nothing folded in.
    if tier == MemoryTier::Project {
        return tool_result_response(id, &payload);
    }

    // 2. Decide which sibling stores to read.
    //    - the MEDULLA feed (app.session) is added for every non-project tier, but
    //      only when the primary is NOT already the medulla (a project brain);
    //    - `all-brains` additionally fans out over every hosted project brain.
    let agent_id = request
        .params
        .get("arguments")
        .and_then(|a| a.get("agent_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("tier-recall")
        .to_string();
    let query = tier_recall_query(tool, request);

    let mut folded: Vec<serde_json::Value> = Vec::new();

    let want_medulla = matches!(
        tier,
        MemoryTier::Medulla | MemoryTier::ProjectAndMedulla | MemoryTier::AllBrains
    );
    if want_medulla && primary_is_project {
        // Read the medulla (the bound owner store). Its own claims are the shared
        // doctrine feed every project beat gets (MED-INV-1's "+ medulla").
        let medulla = app.session.clone();
        folded.extend(store_recall_rows(
            tool, &medulla, &agent_id, &query, "medulla",
        ));
    }

    // `tier:"medulla"` — ONLY the medulla feed. When the primary is a PROJECT brain,
    // drop its own project rows so only the folded medulla feed remains. When the
    // primary already IS the medulla (the bound owner), its own rows ARE the medulla
    // feed — keep them (nothing was folded, and stripping would return empty).
    if tier == MemoryTier::Medulla && primary_is_project {
        strip_memory_feed(&mut payload, tool);
    }

    if tier == MemoryTier::AllBrains {
        // THE FAN-OUT. Every project brain on disk is resolved through the registry,
        // which routes each warm-boot through the R15 eviction gate — so a wide
        // fan-out can never pin more than the warm-brain cap (§C9.1). Each store's
        // rows are labeled by its OWN origin brain (its project root). The primary
        // brain is skipped (its rows are already in `payload`); the medulla was
        // folded above when the primary is a project brain.
        let primary_key = canonical_of(&primary_origin);
        for (root, _facts, _dir) in app.project_brains.disk_roster() {
            if canonical_of(&root) == primary_key {
                continue; // already the primary's own rows
            }
            // resolve() bumps LRU + routes through insert_with_eviction: the warm
            // map stays ≤ cap no matter how many roots this fan-out touches.
            if let Some(brain) = app.project_brains.resolve(&root) {
                let origin = { brain.lock().origin_brain() };
                folded.extend(store_recall_rows(tool, &brain, &agent_id, &query, &origin));
            }
        }
    }

    // 3. Fold the sibling rows into the primary payload's memory feed, de-duped by
    //    node id so a claim surfaced twice is carried once (the primary wins).
    if !folded.is_empty() {
        append_memory_rows(&mut payload, tool, folded);
    }
    // Honest label so a reader/agent knows the beat is cross-brain and how wide.
    if let Some(obj) = payload.as_object_mut() {
        obj.insert(
            "tier".into(),
            serde_json::json!(match tier {
                MemoryTier::Project => "project",
                MemoryTier::Medulla => "medulla",
                MemoryTier::ProjectAndMedulla => "project+medulla",
                MemoryTier::AllBrains => "all-brains",
            }),
        );
    }
    // M7: the delegate packet's `prompt_markdown` is the ONE string the child reads.
    // The handler rendered it from PROJECT rows only (it holds one lock, can't reach
    // the medulla); now that the medulla doctrine rows are folded into the structured
    // `context.memory`, re-render deterministically so the labeled doctrine reaches
    // the child, not just a JSON router. Reuses the same pure renderer — no second
    // rendering path.
    if bare_tool_name(tool) == "delegate"
        && payload.get("verdict").and_then(|v| v.as_str()) == Some("packet")
    {
        let budget_tokens = request
            .params
            .get("arguments")
            .and_then(|a| a.get("budget"))
            .and_then(|b| b.get("tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(crate::delegation_handlers::DEFAULT_BUDGET_TOKENS)
            .min(crate::delegation_handlers::HARD_BUDGET_TOKENS);
        let md = crate::delegation_handlers::render_delegation_packet(&payload, budget_tokens);
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("prompt_markdown".into(), serde_json::json!(md));
        }
    }
    tool_result_response(id, &payload)
}

/// RECONNECT-REBIND (§C5.4, ladder R13). The owner-default reception is produced
/// by the bound `SessionState` (`reception_verdict`), which structurally cannot see
/// the sibling project-brain roster. This routing-seam post-step — the ONE place
/// that holds BOTH the caller_root and the registry — layers the roster onto that
/// verdict: when the response carries a `caller_root_mismatch` reception AND the
/// disk roster names exactly one known brain related to the caller by ancestry
/// (`covering_brain`), it rewrites the reception to PREFER that existing brain over
/// the host cwd:
///   - `known_brain` names the repo root the caller should land on;
///   - the `ingest_your_repo` option's `call` points at the brain root (a re-ingest
///     is a warm re-bind — it resolves the existing store, not a fresh birth), not
///     the ancestor cwd the letter#49 defect suggested;
///   - `honest` gains the reconnect hint so a reader knows a known brain was found.
///
/// A response with no reception, a matched/unknown reception, or no unique roster
/// candidate is returned VERBATIM — honest absence, never a fabricated pick. Reuses
/// the tier-recall pattern of parsing `content[0].text` → mutating → re-serializing,
/// so all three reception-bearing verbs (north / health / session_handshake) are
/// enriched through one seam without a per-tool branch.
fn enrich_reception_with_roster(
    response: JsonRpcResponse,
    app: &Arc<AppState>,
    caller_root: Option<&str>,
) -> JsonRpcResponse {
    let Some(caller_root) = caller_root else {
        return response; // unknown caller → no match to enrich (honesty by omission)
    };
    // Only a successful tool result carries a reception-bearing payload.
    let Some(result) = response.result.as_ref() else {
        return response;
    };
    let Some(text) = result
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
    else {
        return response;
    };
    let Ok(mut payload) = serde_json::from_str::<serde_json::Value>(text) else {
        return response; // non-JSON content (never a reception carrier) → verbatim
    };

    // Gate on a real mismatch reception — a match/unknown/absent one is left alone.
    let is_mismatch = payload
        .get("reception")
        .and_then(|r| r.get("match"))
        .and_then(|m| m.as_str())
        == Some("caller_root_mismatch");
    if !is_mismatch {
        return response;
    }

    // The roster consult: exactly one known brain related to the caller by ancestry.
    let Some(brain_root) = app.project_brains.covering_brain(caller_root) else {
        return response; // 0 = unknown repo, >1 = ambiguous → plain reception
    };

    // Rewrite the mismatch reception to point at the existing brain.
    if let Some(reception) = payload.get_mut("reception").and_then(|r| r.as_object_mut()) {
        reception.insert("known_brain".into(), serde_json::json!(brain_root));
        reception.insert(
            "honest".into(),
            serde_json::json!(
                "this graph does NOT cover your repo — but a known project brain covers your \
                 caller root; rebind to it instead of the host cwd (reconnect-rebind, §C5.4)"
            ),
        );
        if let Some(options) = reception.get_mut("options").and_then(|o| o.as_array_mut()) {
            for option in options.iter_mut() {
                if option.get("action").and_then(|a| a.as_str()) == Some("ingest_your_repo") {
                    if let Some(obj) = option.as_object_mut() {
                        obj.insert(
                            "call".into(),
                            serde_json::json!(format!(
                                "ingest with project_root={brain_root} — a project brain already \
                                 exists for this root; ONE call warm-resolves it and binds this \
                                 session to it (a re-bind after reconnect, not a fresh birth), \
                                 then every call from your root routes to it automatically"
                            )),
                        );
                    }
                }
            }
        }
    }

    tool_result_response(response.id, &payload)
}

/// Canonicalized form of a brain origin string, for identity comparison across
/// path alias spellings. `medulla` (and any non-path origin) passes through.
fn canonical_of(origin: &str) -> String {
    if origin == "medulla" {
        return origin.to_string();
    }
    crate::project_brains::ProjectBrainRegistry::canonical_key(origin)
}

/// The recall query for the folded feed: `seek`/`north` carry a `query`/`task`;
/// `boot_memory` list has none (a broad, most-recent recall). Kept aligned with
/// north's own light-recall query shape.
fn tier_recall_query(tool: &str, request: &JsonRpcRequest) -> String {
    let args = request.params.get("arguments");
    let field = match bare_tool_name(tool) {
        "seek" => "query",
        "north" | "delegate" => "task",
        _ => "",
    };
    args.and_then(|a| a.get(field))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty())
        // boot_memory (or an empty query) → a broad, most-recent memory recall.
        .unwrap_or_else(|| "memory decision finding note claim".to_string())
}

/// Read a sibling store's memory feed in the shape the tool expects: `seek`/`north`
/// fold L1GHT claims (via [`store_memory_rows`]); `boot_memory` folds boot-KV
/// entries (via [`store_boot_rows`]) — same shape as the primary's own `entries`.
fn store_recall_rows(
    tool: &str,
    brain: &Arc<Mutex<SessionState>>,
    agent_id: &str,
    query: &str,
    origin_brain: &str,
) -> Vec<serde_json::Value> {
    match bare_tool_name(tool) {
        "boot_memory" => store_boot_rows(brain, agent_id, origin_brain),
        _ => store_memory_rows(brain, agent_id, query, origin_brain),
    }
}

/// Read a sibling store's boot-memory KV entries (action=list), each labeled with
/// `origin_brain` + `tier`. Same `{key, value, updated_at_ms, …}` shape the
/// primary's own `entries` carry, so the fold is homogeneous.
fn store_boot_rows(
    brain: &Arc<Mutex<SessionState>>,
    agent_id: &str,
    origin_brain: &str,
) -> Vec<serde_json::Value> {
    let mut state = brain.lock();
    let this_tier = if state.is_medulla_store() {
        "medulla"
    } else {
        "project"
    };
    let list = crate::boot_memory_handlers::handle_boot_memory(
        &mut state,
        crate::boot_memory_handlers::BootMemoryInput {
            agent_id: agent_id.to_string(),
            action: "list".into(),
            key: None,
            value: None,
            tags: Vec::new(),
            source_refs: Vec::new(),
        },
    );
    list.ok()
        .and_then(|v| v.get("entries").and_then(|e| e.as_array()).cloned())
        .unwrap_or_default()
        .into_iter()
        .map(|mut entry| {
            if let Some(obj) = entry.as_object_mut() {
                obj.insert("origin_brain".into(), serde_json::json!(origin_brain));
                obj.insert("tier".into(), serde_json::json!(this_tier));
            }
            entry
        })
        .collect()
}

/// Run a LIGHT-tier recall over `brain`'s own store and return memory rows labeled
/// with `origin_brain`. Reuses `seek` scoped to the `light::` id namespace (exactly
/// north's mixed-graph-safe recall, §2.1) so code nodes never compete for the
/// window; each hit becomes a `{claim, age_ms, source_agent, origin_brain, tier,
/// node_id}` row. Empty when the store has no live L1GHT claims (honest absence).
fn store_memory_rows(
    brain: &Arc<Mutex<SessionState>>,
    agent_id: &str,
    query: &str,
    origin_brain: &str,
) -> Vec<serde_json::Value> {
    const LIGHT_RECALL_SCOPE: &str = "light::";
    let mut state = brain.lock();
    let this_tier = if state.is_medulla_store() {
        "medulla"
    } else {
        "project"
    };
    let out = crate::layer_handlers::handle_seek(
        &mut state,
        crate::protocol::layers::SeekInput {
            query: query.to_string(),
            agent_id: agent_id.to_string(),
            top_k: 24,
            scope: Some(LIGHT_RECALL_SCOPE.to_string()),
            node_types: Vec::new(),
            min_score: 0.1,
            graph_rerank: true,
            conformance_aware: true,
            token_budget: None,
        },
    );
    let now = now_ms();
    let stale_after_ms: u64 = 30 * 24 * 60 * 60 * 1000;
    let mut seen = std::collections::HashSet::new();
    out.map(|o| o.results)
        .unwrap_or_default()
        .into_iter()
        // A real memory row carries authorship provenance (a code node never does).
        .filter(|r| r.source_agent.is_some() || r.authored_ms_ago.is_some())
        .filter(|r| seen.insert(r.node_id.clone()))
        .take(5)
        .map(|r| {
            let age_ms = r.authored_ms_ago;
            let mut obj = serde_json::Map::new();
            obj.insert("kind".into(), serde_json::json!("light"));
            obj.insert("claim".into(), serde_json::json!(r.label));
            if let Some(age) = age_ms {
                obj.insert("age_ms".into(), serde_json::json!(age));
                obj.insert("stale".into(), serde_json::json!(age > stale_after_ms));
            }
            obj.insert(
                "source_agent".into(),
                r.source_agent
                    .map(serde_json::Value::String)
                    .unwrap_or(serde_json::Value::Null),
            );
            // Provenance-in-recall: prefer the claim's OWN Origin-Brain stamp; fall
            // back to the store's identity when the file predates the stamp.
            let origin = r.origin_brain.unwrap_or_else(|| origin_brain.to_string());
            obj.insert("origin_brain".into(), serde_json::json!(origin));
            obj.insert("tier".into(), serde_json::json!(this_tier));
            obj.insert("node_id".into(), serde_json::json!(r.node_id));
            serde_json::Value::Object(obj)
        })
        .collect()
}

/// Where a tool's durable-memory feed lives in its payload: a flat top-level key
/// for `seek`/`north`/`boot_memory`, or the NESTED `context.memory` slice for
/// `delegate` (M7). The path is walked (creating intermediate objects) so the same
/// fold/strip machinery serves both shapes.
fn memory_feed_path(tool: &str) -> &'static [&'static str] {
    match bare_tool_name(tool) {
        "seek" => &["results"],
        "boot_memory" => &["entries"],
        "delegate" => &["context", "memory"],
        // north and the default fall through to the flat `memory` feed.
        _ => &["memory"],
    }
}

/// Resolve (creating if absent) the mutable memory-feed array for `tool`, walking
/// the nested path from [`memory_feed_path`]. Returns `None` only when an
/// intermediate value exists but is not an object/array (a malformed payload).
fn memory_feed_mut<'a>(
    payload: &'a mut serde_json::Value,
    tool: &str,
) -> Option<&'a mut Vec<serde_json::Value>> {
    let path = memory_feed_path(tool);
    let (last, parents) = path.split_last()?;
    let mut cursor = payload;
    for key in parents {
        let obj = cursor.as_object_mut()?;
        cursor = obj
            .entry((*key).to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    }
    let obj = cursor.as_object_mut()?;
    obj.entry((*last).to_string())
        .or_insert_with(|| serde_json::Value::Array(Vec::new()))
        .as_array_mut()
}

/// Append folded sibling-store rows into the primary payload's memory feed,
/// de-duped by `node_id` (the primary's own rows already present win).
fn append_memory_rows(payload: &mut serde_json::Value, tool: &str, rows: Vec<serde_json::Value>) {
    let Some(arr) = memory_feed_mut(payload, tool) else {
        return;
    };
    // Identity is `node_id` for light rows, `key` for boot-KV rows.
    let row_id = |r: &serde_json::Value| -> Option<String> {
        r.get("node_id")
            .and_then(|v| v.as_str())
            .or_else(|| r.get("key").and_then(|v| v.as_str()))
            .map(String::from)
    };
    let mut have: std::collections::HashSet<String> = arr.iter().filter_map(row_id).collect();
    for row in rows {
        if let Some(rid) = row_id(&row) {
            if !have.insert(rid) {
                continue; // already carried by the primary or an earlier store
            }
        }
        arr.push(row);
    }
}

/// Drop the primary brain's OWN memory feed (used by `tier:"medulla"`, which wants
/// only the medulla's rows). Leaves the rest of the envelope intact. Walks the same
/// (possibly nested) feed path so it clears `delegate`'s `context.memory` too.
fn strip_memory_feed(payload: &mut serde_json::Value, tool: &str) {
    if let Some(arr) = memory_feed_mut(payload, tool) {
        arr.clear();
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

/// Read the hop-2 `M1nd-Caller-Root` request header, if present (§9.5.4). Absent
/// → `None` (caller unknown). Mirrors `session_id_from_headers`.
fn caller_root_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get(CALLER_ROOT_HEADER)
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
    // Hop-2 caller root (§9.5.4): present on every bridge-forwarded request,
    // absent for legacy bridges / direct HTTP (→ owner sees unknown).
    let incoming_caller_root = caller_root_from_headers(&headers);

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

        let response = run_mcp_method(app.clone(), request, incoming_caller_root.clone()).await;

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
                    // Remember the caller root for post-init requests that omit it.
                    caller_root: incoming_caller_root,
                    bound_project_root: None,
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

    // Resolve THIS call's caller root: the per-request header wins (the bridge
    // stamps every request); if absent we fall back to the value captured at
    // initialize, so a legacy re-init or a dropped header does not blind the owner
    // mid-session. Refreshed back onto the stored session when present (§9.5.4).
    let resolved_caller_root = {
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
                if incoming_caller_root.is_some() {
                    s.caller_root = incoming_caller_root.clone();
                }
                s.caller_root.clone()
            }
        }
    };

    // 4. Known session → run the method against the shared graph.
    //
    // Capture the tool name + agent_id BEFORE `run_mcp_method` consumes the
    // request, so that after a successful mutation we can publish a `tool_result`
    // SseEvent onto the broadcast bus. This is the producer side of the
    // server→client push relay: `handle_mcp_get` subscribes to the same bus and
    // forwards `notifications/m1nd/graph_changed` to every attached client. Reads
    // and failed calls publish nothing (`graph_changed_notification` filters to
    // GRAPH_MUTATION_TOOLS + success).
    //
    // The event is stamped with THIS request's originating wire session id so the
    // GET/SSE relay can suppress a client's own mutation (field-triage L21): the
    // push stream is a CROSS-session notifier — an agent must never see an echo of
    // its own write, which through the `--attach` bridge races the real response
    // into the host's stdout and is read as a literal `null`.
    let mutation_meta = mutation_event_meta(&request);
    let response = route_and_run(
        app.clone(),
        request,
        resolved_caller_root,
        session_id.clone(),
    )
    .await;
    if let Some((tool, agent_id)) = mutation_meta {
        publish_graph_mutation_event(
            &app,
            &tool,
            agent_id.as_deref(),
            Some(session_id.as_str()),
            response.error.is_none(),
        );
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

/// Field key under which a broadcast mutation event carries the wire
/// `mcp-session-id` of the session that CAUSED it. The GET/SSE relay reads this to
/// suppress a client's own mutation (see [`graph_changed_notification`]).
const ORIGIN_SESSION_FIELD: &str = "origin_mcp_session";

/// Publish a `tool_result` SseEvent for a finished mutation onto the broadcast
/// bus. The shape mirrors the stdio server's `tool_result` event so the shared
/// [`graph_changed_notification`] relay logic forwards it identically. A failed
/// call carries `success:false` and is suppressed downstream.
///
/// `origin_session` is the wire `mcp-session-id` that made this call; it is stamped
/// into the event so the GET/SSE stream can skip echoing the mutation back to that
/// same session (field-triage L21). It is `None` only for producers with no wire
/// session (none today on this path); such an event is relayed to everyone as
/// before.
fn publish_graph_mutation_event(
    app: &Arc<AppState>,
    tool: &str,
    agent_id: Option<&str>,
    origin_session: Option<&str>,
    success: bool,
) {
    let mut data = serde_json::json!({
        "tool": tool,
        "source": "mcp_http",
        "agent_id": agent_id,
        "success": success,
        "timestamp_ms": now_ms(),
    });
    // Stamp the originating wire session under the const key (a non-literal key can't
    // go in the `json!` body). Absent when there is no wire session.
    if let Some(obj) = data.as_object_mut() {
        obj.insert(
            ORIGIN_SESSION_FIELD.to_string(),
            match origin_session {
                Some(sid) => serde_json::Value::String(sid.to_string()),
                None => serde_json::Value::Null,
            },
        );
    }
    let sse_event = SseEvent {
        event_type: "tool_result".to_string(),
        data,
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
            return Err((StatusCode::BAD_REQUEST, "Missing Mcp-Session-Id header").into_response());
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
/// never relayed, and — enforced here (field-triage L21) — a client never sees an
/// echo of its OWN mutation: an event stamped with this stream's own wire
/// `mcp-session-id` is skipped. Without that skip the caller's own write comes
/// back through the `--attach` bridge and races the real tool response into the
/// host's stdout, where it is read as a literal `null`.
///
/// Each frame gets an incrementing SSE `id:` (cheap; enables future
/// `Last-Event-ID` resumability — replay itself is NOT implemented in this
/// slice). A periodic keepalive comment keeps idle connections open.
pub async fn handle_mcp_get(
    axum::extract::State(app): axum::extract::State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response {
    // Validate before opening the stream (no lock held across `.await`). Retain the
    // validated session id: the relay must suppress THIS session's own mutations.
    let own_session = match validate_session(&app, &headers) {
        Ok(sid) => sid,
        Err(resp) => return resp,
    };

    let rx = app.event_tx.subscribe();
    let mut next_id: u64 = 0;
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).filter_map(move |event| {
        // Synchronous mapping closure → no `.await`, so the session mutex is
        // never touched here and tool dispatch is never blocked by a slow client.
        let frame = match event {
            // Suppress a client's own mutation: an event carrying this stream's own
            // originating wire session id must NEVER be echoed back to it (L21).
            Ok(ref e) if event_origin_is(e, &own_session) => None,
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
        .keep_alive(sse::KeepAlive::new().interval(Duration::from_secs(MCP_SSE_KEEPALIVE_SECS)))
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

    // ---- Origin-session self-echo suppression (field-triage L21) -----------

    #[test]
    fn event_from_own_session_is_recognized_as_self() {
        // An event stamped with the viewer's own wire session id is the viewer's own
        // mutation → the GET/SSE relay must suppress it (this is the frame that,
        // through the --attach bridge, races the response and shows as `null`).
        let e = ev(
            "tool_result",
            serde_json::json!({
                "tool": "ingest", "success": true, "origin_mcp_session": "sess-A",
            }),
        );
        assert!(
            event_origin_is(&e, "sess-A"),
            "own session must be detected"
        );
        // But it is a genuine, relayable mutation for ANY OTHER session (agent B).
        assert!(
            !event_origin_is(&e, "sess-B"),
            "another session must NOT see it as its own"
        );
        assert!(
            graph_changed_notification(&e).is_some(),
            "the event itself is still a real graph change (relayed to others)"
        );
    }

    #[test]
    fn event_without_origin_stamp_is_never_self() {
        // No origin stamp (older/other producers) → not anyone's own → relayed to all.
        let e = ev(
            "tool_result",
            serde_json::json!({"tool": "memorize", "success": true}),
        );
        assert!(!event_origin_is(&e, "sess-A"));
        assert!(!event_origin_is(&e, ""));
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
            let e = ev(
                et,
                serde_json::json!({"tool": "apply_batch", "batch_id": "b1"}),
            );
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
        let project_brains = Arc::new(crate::project_brains::ProjectBrainRegistry::new(
            root.join("runtime")
                .join(crate::project_brains::PROJECT_BRAINS_DIR),
            None,
        ));
        Arc::new(AppState {
            session: Arc::new(Mutex::new(session)),
            tool_schemas_cache,
            event_tx,
            event_log_path: None,
            registry_dir: None,
            mcp_sessions: new_mcp_session_registry(),
            project_brains,
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
                caller_root: None,
                bound_project_root: None,
            },
        );
        sid
    }

    #[tokio::test]
    async fn get_missing_session_is_400() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = build_app_state(temp.path());
        let resp = handle_mcp_get(axum::extract::State(app), HeaderMap::new()).await;
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
        let resp = handle_mcp_delete(axum::extract::State(app), HeaderMap::new()).await;
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
        let resp = handle_mcp_delete(
            axum::extract::State(app.clone()),
            header_map_with_session(&sid),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(!app.mcp_sessions.lock().contains_key(&sid));

        // A subsequent GET with the now-dead session id → 404.
        let resp2 = handle_mcp_get(
            axum::extract::State(app.clone()),
            header_map_with_session(&sid),
        )
        .await;
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
