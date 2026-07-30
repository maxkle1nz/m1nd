// === m1nd-mcp `--attach` stdio↔HTTP bridge ===
//
// Wave 4, Slice 3 — THE deliverable.
//
// `m1nd-mcp --attach <base_url>` is a thin bridge: to the host (Claude Code) it
// is a standard stdio MCP server; to a running `m1nd-mcp --serve` owner it is a
// standard Streamable-HTTP MCP client. It loads NO graph, builds NO engines, and
// takes NO lease. Two such clients pointed at one `--serve` owner SHARE that
// owner's single live graph: agent A's mutation is visible to agent B with no
// reload — because both ultimately drive the SAME shared `BrainSessionCell`
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
/// Net-new hop-2 header carrying the bridge's resolved caller root to the owner
/// (TWO-TIER-BRAIN-PRD §9.5.4). Absent → the owner treats the caller as unknown
/// (legacy bridge / direct HTTP) — the serde-default posture applied to the wire.
const CALLER_ROOT_HEADER: &str = "m1nd-caller-root";

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
///
/// `initialize_payload` is the load-bearing addition for field-triage #5: we
/// RETAIN the host's original `initialize` frame verbatim so that, when the owner
/// restarts and expires our session, we can re-run `initialize` REPLAYING the
/// exact clientInfo/capabilities/protocolVersion the host negotiated — a
/// transparent re-init the host never sees.
#[derive(Clone, Default)]
pub struct AttachSession {
    /// Owner-local HTTP bearer credential. It authenticates the transport only;
    /// sovereign authority is still evaluated separately by the owner.
    pub bearer_token: Option<String>,
    /// `Mcp-Session-Id` minted by the owner at `initialize`.
    pub mcp_session_id: Option<String>,
    /// `result.protocolVersion` negotiated at `initialize`.
    pub protocol_version: Option<String>,
    /// The host's original `initialize` request frame, retained verbatim so a
    /// transparent re-init can replay its params (see field-triage #5).
    pub initialize_payload: Option<String>,
    /// The bridge's resolved caller root, computed ONCE at attach start and
    /// stamped as `M1nd-Caller-Root` on every forwarded request so the owner can
    /// tell first contact honestly (TWO-TIER-BRAIN-PRD §9.5.4). `None` only if no
    /// env candidate and no cwd could be resolved — the owner then sees unknown.
    pub caller_root: Option<String>,
}

impl AttachSession {
    /// Absorb an `initialize` round-trip: record the freshly minted
    /// `Mcp-Session-Id` (from the response header), the negotiated
    /// `result.protocolVersion` (from the response body), and retain the exact
    /// request frame that produced them so it can be replayed on a later re-init.
    /// Mirrors the capture the bridge loop performs on the host's first
    /// `initialize`; reused by [`reinitialize`] and by the integration test.
    pub fn capture_initialize(
        &mut self,
        session_id_header: &Option<String>,
        response_value: &serde_json::Value,
        request_payload: &str,
    ) {
        if let Some(sid) = session_id_header {
            self.mcp_session_id = Some(sid.clone());
        }
        if let Some(pv) = response_value
            .get("result")
            .and_then(|r| r.get("protocolVersion"))
            .and_then(|p| p.as_str())
        {
            self.protocol_version = Some(pv.to_string());
        }
        self.initialize_payload = Some(request_payload.to_string());
    }
}

/// JSON-RPC error code the owner returns when a forwarded request carries an
/// `Mcp-Session-Id` it no longer knows (e.g. after an owner restart). Per the MCP
/// spec the owner also answers HTTP 404; we treat BOTH shapes as the
/// session-expired signal (see [`SESSION_EXPIRED_STATUS`] and
/// [`PostOutcome::signals_session_expired`]) so the re-init trigger does not
/// depend on the owner always delivering a parseable JSON-RPC frame.
const SESSION_EXPIRED_CODE: i64 = -32001;

/// HTTP status the owner returns for an unknown/expired `Mcp-Session-Id` (per the
/// MCP Streamable-HTTP spec: "re-initialize"). The owner's `POST /mcp` currently
/// pairs this with a JSON body carrying the `-32001` frame, but its `GET /mcp`
/// (SSE relay) answers 404 with a PLAIN-TEXT body and NO JSON-RPC frame — and an
/// intermediary/proxy could strip the body on either path. Keying re-init on this
/// status too (not only the frame) makes recovery robust to every unknown-session
/// SHAPE the transport can present. Locked against owner drift by
/// `tests/attach_reinit.rs::owner_unknown_session_wire_shape_is_recoverable`.
const SESSION_EXPIRED_STATUS: u16 = 404;

/// Outcome of one POST to the owner's `/mcp`, demuxed to a single JSON-RPC value.
///
/// `value` is the response frame (for a request) or `None` (for a notification,
/// or when no usable frame could be extracted). `session_id_header` carries the
/// `Mcp-Session-Id` the owner minted on this response (only present on
/// `initialize`). `status` is the HTTP status, kept for diagnostics.
pub struct PostOutcome {
    /// The demuxed JSON-RPC response value, if the owner returned one.
    pub value: Option<serde_json::Value>,
    /// `Mcp-Session-Id` response header, if the owner set one (init only).
    pub session_id_header: Option<String>,
    /// HTTP status code the owner returned.
    pub status: u16,
}

impl PostOutcome {
    /// Does this outcome mean the owner no longer knows our session (restart)?
    ///
    /// True if EITHER the demuxed response frame carries the `-32001` error code
    /// OR the owner answered the session-expired HTTP status (`404`). Covering
    /// both shapes is the field-triage batch-C hardening: #225 keyed re-init only
    /// on the parseable `-32001` frame, so an unknown-session response delivered
    /// WITHOUT a JSON-RPC frame (the owner's own SSE/GET path already does this
    /// with a plain-text 404 body; a proxy could do it on POST) slipped past the
    /// trigger and the bridge failed with "no JSON-RPC response frame" instead of
    /// recovering. The status check closes that.
    pub fn signals_session_expired(&self) -> bool {
        self.status == SESSION_EXPIRED_STATUS || self.value.as_ref().is_some_and(is_session_expired)
    }
}

/// Does this response frame carry the owner's "session unknown/expired" error?
/// Keyed on the JSON-RPC error `code == -32001` so it works whether the owner
/// delivered the error as `application/json` or inside an SSE frame. This is the
/// FRAME-level check; the transport-level shape (HTTP 404 with or without a frame)
/// is handled by [`PostOutcome::signals_session_expired`].
fn is_session_expired(value: &serde_json::Value) -> bool {
    value
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_i64())
        == Some(SESSION_EXPIRED_CODE)
}

/// POST one JSON-RPC `payload` to the owner's `/mcp` `endpoint`, attaching the
/// session's `Mcp-Session-Id` + negotiated protocol version, and demux the reply
/// to a single JSON-RPC value.
///
/// This is the exact transport step the bridge loop performs, factored out so the
/// re-init retry ([`forward_with_reinit`]) and the integration test drive the
/// SAME code the loop does. When `relay` is `Some`, interim server→client SSE
/// notifications are forwarded to that sink (the loop passes its stdout sink);
/// pass `None` when no relay target exists (the test path).
///
/// Errors are transport failures (could not reach the owner / could not read the
/// body). A JSON-RPC *error frame* from the owner is NOT an `Err` — it comes back
/// as `Ok(PostOutcome { value: Some(<error frame>), .. })`, so callers can inspect
/// the code (e.g. `-32001`) and decide to re-initialize.
pub async fn post_and_demux(
    client: &reqwest::Client,
    endpoint: &str,
    session: &AttachSession,
    payload: &str,
) -> Result<PostOutcome, String> {
    post_and_demux_relayed::<fn(serde_json::Value)>(client, endpoint, session, payload, None).await
}

/// Backing implementation of [`post_and_demux`]. `relay`, when set, is invoked for
/// every interim server→client notification frame found in an SSE body (id-less),
/// so the caller can forward it verbatim to the host. Generic over the relay
/// closure so the public `None` path needs no allocation and no sink type.
async fn post_and_demux_relayed<F>(
    client: &reqwest::Client,
    endpoint: &str,
    session: &AttachSession,
    payload: &str,
    mut relay: Option<F>,
) -> Result<PostOutcome, String>
where
    F: FnMut(serde_json::Value),
{
    let mut builder = client
        .post(endpoint)
        .header(
            reqwest::header::ACCEPT,
            "application/json, text/event-stream",
        )
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(payload.to_string());

    if let Some(token) = &session.bearer_token {
        builder = builder.bearer_auth(token);
    }

    if let Some(sid) = &session.mcp_session_id {
        builder = builder.header(MCP_SESSION_HEADER, sid.clone());
    }
    if let Some(pv) = &session.protocol_version {
        builder = builder.header(MCP_PROTOCOL_VERSION_HEADER, pv.clone());
    }
    // Hop-2 first-contact truth (§9.5.4): tell the owner which repo the caller is
    // rooted in. Stamped on EVERY forwarded request (the bridge cwd is fixed, but
    // this keeps the value arriving even if it was absent at initialize).
    if let Some(root) = &session.caller_root {
        builder = builder.header(CALLER_ROOT_HEADER, root.clone());
    }

    let response = builder.send().await.map_err(|e| e.to_string())?;
    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let session_id_header = response
        .headers()
        .get(MCP_SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    // A notification/response POST (no reply expected) returns 202 with an empty
    // body — surface no value.
    if status == reqwest::StatusCode::ACCEPTED {
        return Ok(PostOutcome {
            value: None,
            session_id_header,
            status: status.as_u16(),
        });
    }

    let body = response.text().await.map_err(|e| e.to_string())?;

    // Parse the request id we sent so SSE demux can pick the matching frame.
    let want_id = serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .and_then(|v| v.get("id").cloned())
        .filter(|v| !v.is_null());

    let value = if content_type.contains("text/event-stream") {
        extract_sse_response_relayed(&body, want_id.as_ref(), relay.as_mut())
    } else {
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

    Ok(PostOutcome {
        value,
        session_id_header,
        status: status.as_u16(),
    })
}

/// Transparently re-initialize a session against the owner after it expired
/// (owner restart). REPLAYS the retained host `initialize` frame so the fresh
/// session preserves the negotiated clientInfo/capabilities/protocolVersion, then
/// re-sends `notifications/initialized`. On success `session` is updated in place
/// with the new `Mcp-Session-Id` (and protocol version); returns the fresh
/// session id. Returns `Err` if there is no retained initialize frame or the
/// re-init round-trip itself fails — the caller then passes the honest error
/// through rather than looping.
pub async fn reinitialize(
    client: &reqwest::Client,
    endpoint: &str,
    session: &mut AttachSession,
) -> Result<String, String> {
    let init_payload = session
        .initialize_payload
        .clone()
        .ok_or_else(|| "no retained initialize frame to replay".to_string())?;

    // Re-run initialize with a CLEAN session (no stale id) so the owner mints a
    // brand-new one from the replayed host params.
    let clean = AttachSession {
        bearer_token: session.bearer_token.clone(),
        caller_root: session.caller_root.clone(),
        initialize_payload: Some(init_payload.clone()),
        ..AttachSession::default()
    };
    let outcome = post_and_demux(client, endpoint, &clean, &init_payload).await?;

    let value = outcome
        .value
        .ok_or_else(|| "re-init produced no initialize response frame".to_string())?;
    if is_session_expired(&value) || value.get("error").is_some() {
        return Err(format!("re-init initialize returned an error: {value}"));
    }

    session.capture_initialize(&outcome.session_id_header, &value, &init_payload);
    let new_sid = session
        .mcp_session_id
        .clone()
        .ok_or_else(|| "re-init response carried no Mcp-Session-Id".to_string())?;

    // The owner's new session needs the `initialized` notification before it will
    // accept post-init requests. Best-effort: a failure here surfaces as the retry
    // failing, which is handled honestly upstream.
    let notify = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    let _ = post_and_demux(client, endpoint, session, notify).await;

    Ok(new_sid)
}

/// Forward one JSON-RPC `payload` to the owner, transparently recovering from an
/// owner restart. If the owner signals an unknown/expired session — in ANY shape:
/// the `-32001` JSON-RPC frame OR a bare HTTP `404` with no usable body (see
/// [`PostOutcome::signals_session_expired`]) — re-initialize ONCE (replaying the
/// retained host params) and retry the original request under the fresh session,
/// returning that result to the host as if nothing happened. Guarded to a single
/// re-init attempt per call: if re-init or the retry still signals expiry, the
/// honest error is returned rather than looping. `session` is updated in place
/// across a successful re-init.
///
/// This is the field-triage #5 fix, hardened in field-triage batch-C to key on
/// the unknown-session HTTP status (not only the frame) so recovery is robust to
/// every transport shape. Exercised end-to-end (real owner restart, incl. a
/// double-restart / binary-swap cycle, red→green) by `tests/attach_reinit.rs`.
pub async fn forward_with_reinit(
    client: &reqwest::Client,
    endpoint: &str,
    session: &mut AttachSession,
    payload: &str,
) -> Result<serde_json::Value, String> {
    let outcome = post_and_demux(client, endpoint, session, payload).await?;

    // Decide expiry from the WHOLE outcome (HTTP status + optional frame), not
    // just a parsed frame — the owner may signal an unknown session at the
    // transport layer (HTTP 404) with no usable JSON-RPC body (its SSE/GET path
    // already does exactly that; a proxy could do it on POST). Reading `value`
    // first would fail with "no response frame" and never reach re-init.
    if !outcome.signals_session_expired() {
        // Not a session-expiry outcome: return the frame, or surface the honest
        // "no frame" transport error for anything else.
        return outcome
            .value
            .ok_or_else(|| "owner returned no JSON-RPC response frame".to_string());
    }

    // Preserve the exact session-expired error to pass through honestly if re-init
    // or the retry cannot recover. Synthesize one when the owner sent no frame
    // (e.g. a plain-text 404) so double-failure still yields a well-formed error.
    let want_id = serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .and_then(|v| v.get("id").cloned())
        .filter(|v| !v.is_null())
        .unwrap_or(serde_json::Value::Null);
    let expired_error = outcome.value.clone().unwrap_or_else(|| {
        serde_json::to_value(jsonrpc_error(
            want_id,
            SESSION_EXPIRED_CODE as i32,
            "Unknown or expired Mcp-Session-Id; re-initialize".to_string(),
        ))
        .unwrap_or(serde_json::Value::Null)
    });

    // Owner-side session is gone (restart). Re-initialize once, then retry.
    eprintln!("[m1nd-mcp][attach] owner session expired — re-initialized");
    match reinitialize(client, endpoint, session).await {
        Ok(_) => {
            // Single-retry guard: exactly one re-init + retry per call. Whatever
            // the retry returns is final — its real result on success, or (if it
            // STILL signals expiry / carried no frame) the preserved honest error,
            // never a second re-init loop.
            let retry = post_and_demux(client, endpoint, session, payload).await?;
            Ok(retry.value.unwrap_or(expired_error))
        }
        Err(e) => {
            eprintln!("[m1nd-mcp][attach] re-init failed ({e}); passing error through");
            // Honest passthrough of the original session-expired error.
            Ok(expired_error)
        }
    }
}

/// Resolve the caller root for the hop-2 `M1nd-Caller-Root` header
/// (TWO-TIER-BRAIN-PRD §9.5.4).
///
/// Mirrors the owner's workspace ladder but stays host-neutral: the explicit
/// `M1ND_*` pins first, then the generic workspace-env set, then the bridge's own
/// spawn cwd — which §9.5.4 documents as the hop-1 caller truth. The full
/// editor-hint list is intentionally NOT copied here; cwd already covers the
/// bridge's spawn dir. An env candidate wins only if it is a non-empty, existing
/// directory (so a stale/empty pin never masks the real cwd).
///
/// `pub` because `--attach auto`'s ingest-coverage discovery must ask about the
/// SAME root the bridge will then present. Two definitions would let a client
/// pick an owner by one root and then introduce itself with another.
pub fn resolve_caller_root() -> Option<String> {
    const CALLER_ROOT_ENV_CANDIDATES: [&str; 6] = [
        "M1ND_WORKSPACE_ROOT",
        "M1ND_PROJECT_ROOT",
        "M1ND_REPO_ROOT",
        "WORKSPACE_ROOT",
        "PROJECT_ROOT",
        "REPO_ROOT",
    ];
    for name in CALLER_ROOT_ENV_CANDIDATES {
        if let Ok(value) = std::env::var(name) {
            let trimmed = value.trim();
            if !trimmed.is_empty() && std::path::Path::new(trimmed).is_dir() {
                return Some(trimmed.to_string());
            }
        }
    }
    std::env::current_dir()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

/// Run the `--attach` bridge against `base_url` (e.g. `http://127.0.0.1:1337`).
///
/// Loop: read one JSON-RPC frame from stdin (preserving its detected
/// `TransportMode`), POST it to `<base_url>/mcp`, and relay the response frame to
/// stdout in the SAME mode. Exits cleanly on stdin EOF.
pub async fn run_attach_client(base_url: String, bearer_token: String) {
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

    // Resolve the caller root ONCE (§9.5.4 hop-2): the bridge inherits the host
    // session env at spawn, so this is the caller's truth for the session.
    let mut session = AttachSession {
        bearer_token: Some(bearer_token),
        caller_root: resolve_caller_root(),
        ..AttachSession::default()
    };
    eprintln!(
        "[m1nd-mcp][attach] caller_root={:?} (hop-2 M1nd-Caller-Root)",
        session.caller_root
    );

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
    let (frame_tx, mut frame_rx) = tokio::sync::mpsc::channel::<(String, TransportMode)>(64);
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
                let err = jsonrpc_error(
                    serde_json::Value::Null,
                    -32700,
                    format!("Parse error: {}", e),
                );
                sink.emit(&err, mode);
                continue;
            }
        };

        let req_id = parsed.get("id").cloned();
        let is_request = req_id.as_ref().is_some_and(|v| !v.is_null());
        let method = parsed
            .get("method")
            .and_then(|m| m.as_str())
            .map(str::to_owned);
        let is_initialize = method.as_deref() == Some("initialize");

        let id_for_error = req_id.clone().unwrap_or(serde_json::Value::Null);

        // ================= INITIALIZE (host's own) =================
        // The host's `initialize` cannot itself be session-expired, so it takes
        // the plain POST path. Capturing it here RETAINS the host's exact
        // clientInfo/capabilities/protocolVersion frame so a later transparent
        // re-init (owner restart) can replay it verbatim — the core of the fix.
        if is_initialize {
            let outcome = match post_and_demux(&client, &endpoint, &session, &payload).await {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("[m1nd-mcp][attach] HTTP send error on initialize: {}", e);
                    let err = jsonrpc_error(
                        id_for_error,
                        -32002,
                        format!("attach bridge: failed to reach m1nd owner at {endpoint}: {e}"),
                    );
                    sink.emit(&err, mode);
                    continue;
                }
            };
            match outcome.value {
                Some(v) => {
                    session.capture_initialize(&outcome.session_id_header, &v, &payload);
                    match &session.mcp_session_id {
                        Some(sid) => eprintln!("[m1nd-mcp][attach] captured Mcp-Session-Id={sid}"),
                        None => eprintln!(
                            "[m1nd-mcp][attach] WARNING: initialize response had no Mcp-Session-Id header"
                        ),
                    }
                    // Lazily spawn the server→client push relay exactly once, now
                    // that a session id exists (the owner routes the SSE GET by it).
                    if !relay_spawned {
                        if let Some(sid) = session.mcp_session_id.clone() {
                            relay_spawned = true;
                            relay_handle = Some(spawn_push_relay(
                                &client,
                                &endpoint,
                                sid,
                                session.protocol_version.clone(),
                                session.bearer_token.clone(),
                                &sink,
                                mode,
                            ));
                        }
                    }
                    sink.emit_value(v, mode);
                }
                None => {
                    let err = jsonrpc_error(
                        id_for_error,
                        -32004,
                        "attach bridge: owner returned no initialize response frame".to_string(),
                    );
                    sink.emit(&err, mode);
                }
            }
            continue;
        }

        // ================= NOTIFICATIONS (no id) =================
        // Owner replies 202; nothing goes to stdout. Best-effort forward — a
        // post-restart 404 here is harmless (the next request re-inits).
        if !is_request {
            match post_and_demux(&client, &endpoint, &session, &payload).await {
                Ok(o) if o.status != 202 && !(200..300).contains(&o.status) => {
                    eprintln!(
                        "[m1nd-mcp][attach] notification POST returned {} (expected 202)",
                        o.status
                    );
                }
                Ok(_) => {}
                Err(e) => eprintln!("[m1nd-mcp][attach] notification POST error: {e}"),
            }
            continue;
        }

        // ================= POST-INIT REQUESTS =================
        // Route through the transparent re-init path: if the owner restarted and
        // our session expired (-32001), re-initialize (replaying the retained host
        // params) and retry ONCE, so the host sees a clean result. We snapshot the
        // session id first; if it changed, a re-init happened and the SSE relay
        // must re-subscribe under the fresh id.
        let sid_before = session.mcp_session_id.clone();
        let result = forward_with_reinit(&client, &endpoint, &mut session, &payload).await;
        let sid_after = session.mcp_session_id.clone();

        if sid_after != sid_before {
            // A transparent re-init minted a new session. Tear down the stale relay
            // (its GET is pinned to the old, now-unknown session id) and re-spawn it
            // under the new one so `graph_changed` push notifications keep flowing.
            if let Some(handle) = relay_handle.take() {
                handle.abort();
                let _ = handle.await;
            }
            if let Some(sid) = sid_after {
                relay_handle = Some(spawn_push_relay(
                    &client,
                    &endpoint,
                    sid,
                    session.protocol_version.clone(),
                    session.bearer_token.clone(),
                    &sink,
                    mode,
                ));
                relay_spawned = true;
            }
        }

        match result {
            Ok(v) => sink.emit_value(v, mode),
            Err(e) => {
                eprintln!("[m1nd-mcp][attach] request forward error: {e}");
                let err = jsonrpc_error(
                    id_for_error,
                    -32002,
                    format!("attach bridge: failed to reach m1nd owner at {endpoint}: {e}"),
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

/// Spawn the long-lived push relay under `session_id`, returning its handle.
/// Small wrapper so both the first-`initialize` spawn and the re-subscribe after a
/// transparent re-init share one call site (they must pass the SAME arguments).
fn spawn_push_relay(
    client: &reqwest::Client,
    endpoint: &str,
    session_id: String,
    protocol_version: Option<String>,
    bearer_token: Option<String>,
    sink: &StdoutSink,
    mode: TransportMode,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run_push_relay(
        client.clone(),
        endpoint.to_string(),
        session_id,
        protocol_version,
        bearer_token,
        sink.clone(),
        mode,
    ))
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
    bearer_token: Option<String>,
    sink: StdoutSink,
    mode: TransportMode,
) {
    use std::time::Duration;

    const MAX_BACKOFF_SECS: u64 = 30;
    let mut backoff_secs: u64 = 1;

    eprintln!(
        "[m1nd-mcp][attach] push relay: subscribing to {} (SSE)",
        endpoint
    );

    loop {
        let mut builder = client
            .get(&endpoint)
            .header(reqwest::header::ACCEPT, "text/event-stream")
            .header(MCP_SESSION_HEADER, session_id.clone());
        if let Some(token) = &bearer_token {
            builder = builder.bearer_auth(token);
        }
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
    extract_sse_response_relayed(body, want_id, Some(|v| sink.emit_value(v, mode)))
}

/// Core of [`extract_sse_response`]: returns the response frame matching `want_id`
/// (or the first response-shaped frame) and invokes `relay` on every interim
/// id-less notification frame. Generic over the relay closure so the sink-free
/// callers (`post_and_demux`, tests) pay nothing; the stdout path passes a closure
/// that forwards to the serialized sink.
fn extract_sse_response_relayed<F>(
    body: &str,
    want_id: Option<&serde_json::Value>,
    mut relay: Option<F>,
) -> Option<serde_json::Value>
where
    F: FnMut(serde_json::Value),
{
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
            // Interim server→client notification → relay to the caller's sink.
            if let Some(relay) = relay.as_mut() {
                relay(value);
            }
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
    fn signals_session_expired_covers_frame_and_status_shapes() {
        // Frame carries -32001 (owner's POST shape) → expired, whatever the status.
        let frame = serde_json::json!({
            "jsonrpc": "2.0", "id": 1,
            "error": { "code": -32001, "message": "Unknown or expired Mcp-Session-Id" }
        });
        let with_frame = PostOutcome {
            value: Some(frame),
            session_id_header: None,
            status: 404,
        };
        assert!(with_frame.signals_session_expired());

        // 404 with NO frame (owner's SSE/GET shape, or a proxy stripping the body)
        // → STILL expired, driven by the status alone. This is the batch-C fix.
        let frameless = PostOutcome {
            value: None,
            session_id_header: None,
            status: 404,
        };
        assert!(frameless.signals_session_expired());

        // A 404 whose body happens to be some OTHER error is still treated as an
        // unknown-session outcome (the owner only ever 404s for that reason on
        // /mcp) — status is authoritative.
        let other_404 = PostOutcome {
            value: Some(
                serde_json::json!({"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"x"}}),
            ),
            session_id_header: None,
            status: 404,
        };
        assert!(other_404.signals_session_expired());

        // A normal 200 result must NOT be treated as expired.
        let ok = PostOutcome {
            value: Some(serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"ok":true}})),
            session_id_header: None,
            status: 200,
        };
        assert!(!ok.signals_session_expired());

        // A non-404 error frame that is NOT -32001 must NOT be treated as expired
        // (e.g. a genuine tool error under HTTP 200) — otherwise we'd re-init on
        // ordinary failures.
        let other_error = PostOutcome {
            value: Some(
                serde_json::json!({"jsonrpc":"2.0","id":1,"error":{"code":-32603,"message":"boom"}}),
            ),
            session_id_header: None,
            status: 200,
        };
        assert!(!other_error.signals_session_expired());
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
        let event =
            "data: {\"jsonrpc\":\"2.0\",\ndata: \"method\":\"notifications/x\",\"params\":{}}\n";
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
