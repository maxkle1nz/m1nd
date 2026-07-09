// === m1nd-mcp HTTP server (axum) ===
//
// Embedded web UI server. Feature-gated behind "serve".
// Provides REST API for all 52 MCP tools + graph visualization endpoints.
// Uses the same dispatch_tool() free function as the stdio JSON-RPC transport.

#![allow(clippy::duplicated_attributes)]
#![cfg(feature = "serve")]

use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, StatusCode, Uri},
    response::{sse, IntoResponse, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::StreamExt;
use parking_lot::Mutex;
use rust_embed::Embed;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};

use crate::http_types::SubgraphQuery;
use crate::instance_registry::{
    delete_instance_state, list_instances, spawn_heartbeat, InstanceRegistryEntry,
};
use crate::server::{all_tool_schemas, dispatch_tool, tool_schemas, McpConfig};
use crate::session::{ApplyBatchProgressSink, SessionState};
use crate::util::now_ms;

// ---------------------------------------------------------------------------
// Event log: append-only JSON lines file for cross-process SSE (Option B)
// ---------------------------------------------------------------------------

/// Write an SSE event as a JSON line to the event log file.
/// Thread-safe: opens file with append mode on each write.
fn append_event_to_log(path: &std::path::Path, event: &SseEvent) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        if let Ok(line) = serde_json::to_string(event) {
            let _ = writeln!(f, "{}", line);
        }
    }
}

fn emit_followup_events(
    event_tx: &broadcast::Sender<SseEvent>,
    event_log_path: Option<&std::path::PathBuf>,
    tool_name: &str,
    source: &str,
    agent_id: &str,
    output: &serde_json::Value,
) {
    if tool_name != "apply_batch" {
        return;
    }

    let Some(progress_events) = output.get("progress_events").and_then(|v| v.as_array()) else {
        return;
    };

    for progress_event in progress_events {
        let sse_event = SseEvent {
            event_type: "apply_batch_progress".to_string(),
            data: serde_json::json!({
                "tool": tool_name,
                "source": source,
                "agent_id": agent_id,
                "batch_id": progress_event.get("batch_id").cloned().unwrap_or(serde_json::Value::Null),
                "progress": progress_event,
                "timestamp_ms": now_ms(),
            }),
        };
        let _ = event_tx.send(sse_event.clone());
        if let Some(log_path) = event_log_path {
            append_event_to_log(log_path, &sse_event);
        }
    }

    emit_apply_batch_handoff(event_tx, event_log_path, source, agent_id, output);
}

fn tool_result_summary(tool_name: &str, output: &serde_json::Value) -> serde_json::Value {
    if tool_name != "apply_batch" {
        return truncate_json(output, 500);
    }

    serde_json::json!({
        "batch_id": output.get("batch_id").cloned().unwrap_or(serde_json::Value::Null),
        "proof_state": output.get("proof_state").cloned().unwrap_or(serde_json::Value::Null),
        "active_phase": output.get("active_phase").cloned().unwrap_or(serde_json::Value::Null),
        "progress_pct": output.get("progress_pct").cloned().unwrap_or(serde_json::Value::Null),
        "next_suggested_tool": output.get("next_suggested_tool").cloned().unwrap_or(serde_json::Value::Null),
        "next_suggested_target": output.get("next_suggested_target").cloned().unwrap_or(serde_json::Value::Null),
        "next_step_hint": output.get("next_step_hint").cloned().unwrap_or(serde_json::Value::Null),
        "verification_verdict": output
            .get("verification")
            .and_then(|value| value.get("verdict"))
            .cloned()
            .unwrap_or(serde_json::Value::Null),
        "progress_event_count": output
            .get("progress_events")
            .and_then(|value| value.as_array())
            .map(|value| value.len())
            .unwrap_or(0),
    })
}

fn emit_apply_batch_handoff(
    event_tx: &broadcast::Sender<SseEvent>,
    event_log_path: Option<&std::path::PathBuf>,
    source: &str,
    agent_id: &str,
    output: &serde_json::Value,
) {
    let batch_id = output
        .get("batch_id")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let proof_state = output
        .get("proof_state")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let next_suggested_tool = output
        .get("next_suggested_tool")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let next_suggested_target = output
        .get("next_suggested_target")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let next_step_hint = output
        .get("next_step_hint")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    if batch_id.is_null()
        && proof_state.is_null()
        && next_suggested_tool.is_null()
        && next_suggested_target.is_null()
        && next_step_hint.is_null()
    {
        return;
    }

    let sse_event = SseEvent {
        event_type: "apply_batch_handoff".to_string(),
        data: serde_json::json!({
            "tool": "apply_batch",
            "source": source,
            "agent_id": agent_id,
            "batch_id": batch_id,
            "proof_state": proof_state,
            "next_suggested_tool": next_suggested_tool,
            "next_suggested_target": next_suggested_target,
            "next_step_hint": next_step_hint,
            "timestamp_ms": now_ms(),
        }),
    };
    let _ = event_tx.send(sse_event.clone());
    if let Some(log_path) = event_log_path {
        append_event_to_log(log_path, &sse_event);
    }
}

fn apply_batch_progress_sink(
    event_tx: broadcast::Sender<SseEvent>,
    event_log_path: Option<std::path::PathBuf>,
    source: String,
    agent_id: String,
) -> ApplyBatchProgressSink {
    Arc::new(move |progress_event| {
        let sse_event = SseEvent {
            event_type: "apply_batch_progress".to_string(),
            data: serde_json::json!({
                "tool": "apply_batch",
                "source": source,
                "agent_id": agent_id,
                "batch_id": progress_event.batch_id,
                "progress": progress_event,
                "timestamp_ms": now_ms(),
            }),
        };
        let _ = event_tx.send(sse_event.clone());
        if let Some(ref log_path) = event_log_path {
            append_event_to_log(log_path, &sse_event);
        }
    })
}

/// Watch an event log file and broadcast new events via SSE.
/// Polls every 100ms for new lines appended to the file.
async fn watch_event_log(path: std::path::PathBuf, tx: broadcast::Sender<SseEvent>) {
    use tokio::io::AsyncBufReadExt;

    // Wait for file to exist
    loop {
        if path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let file = match tokio::fs::File::open(&path).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[m1nd-mcp] Failed to open event log for watching: {}", e);
            return;
        }
    };

    // Seek to end — only read NEW events
    let mut reader = tokio::io::BufReader::new(file);
    // Read and discard existing content
    {
        let mut discard = String::new();
        loop {
            discard.clear();
            match reader.read_line(&mut discard).await {
                Ok(0) => break, // EOF
                Ok(_) => continue,
                Err(_) => break,
            }
        }
    }

    eprintln!(
        "[m1nd-mcp] Watching event log: {} (tailing new events)",
        path.display()
    );

    // Now poll for new lines
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                // No new data — poll interval
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Ok(_) => {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    if let Ok(event) = serde_json::from_str::<SseEvent>(trimmed) {
                        let _ = tx.send(event);
                    }
                }
            }
            Err(e) => {
                eprintln!("[m1nd-mcp] Event log read error: {}", e);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Embedded UI assets (rust-embed)
// ---------------------------------------------------------------------------

#[derive(Embed)]
#[folder = "../m1nd-ui/dist/"]
#[prefix = ""]
struct UiAssets;

// ---------------------------------------------------------------------------
// SSE event type
// ---------------------------------------------------------------------------

/// SSE event emitted after tool execution.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SseEvent {
    pub event_type: String,
    pub data: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

/// Shared state for axum handlers.
pub struct AppState {
    pub session: Arc<Mutex<SessionState>>,
    pub tool_schemas_cache: serde_json::Value,
    pub event_tx: broadcast::Sender<SseEvent>,
    /// Optional event log path for cross-process SSE (Option B).
    pub event_log_path: Option<std::path::PathBuf>,
    pub registry_dir: Option<std::path::PathBuf>,
    /// Registry of live Streamable-HTTP MCP wire sessions (Wave 4, Slice 1).
    /// Distinct from the instance lease and from `SessionState.sessions`.
    pub mcp_sessions: crate::mcp_http::McpSessionRegistry,
    /// Two-Tier Brain (interim): owner-hosted per-project brains, routed by the
    /// hop-2 caller root. The bound `session` above stays exactly the dev/single
    /// graph it always was; project brains live BESIDE it, never inside it.
    pub project_brains: Arc<crate::project_brains::ProjectBrainRegistry>,
}

// ---------------------------------------------------------------------------
// Tool execution timeout
// ---------------------------------------------------------------------------

const TOOL_TIMEOUT_SECS: u64 = 120; // 2 min — ingest de pastas grandes (clawd/memory ~106 files) precisa

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Spawn the HTTP server in background, sharing an existing SessionState.
/// Used by stdio mode to also serve the GUI without blocking the stdio loop.
/// Returns the tokio JoinHandle for the server task.
pub fn spawn_background(
    session: Arc<Mutex<SessionState>>,
    port: u16,
) -> tokio::task::JoinHandle<()> {
    // Build tool schemas cache
    let schemas_full = tool_schemas();
    let tool_schemas_cache = schemas_full
        .get("tools")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));

    // SSE broadcast channel
    let (event_tx, _) = broadcast::channel::<SseEvent>(64);
    let (registry_root, runtime_root) = {
        let guard = session.lock();
        (guard.instance.registry_root(), guard.runtime_root.clone())
    };

    // AppState
    let project_brains = Arc::new(crate::project_brains::ProjectBrainRegistry::new(
        runtime_root.join(crate::project_brains::PROJECT_BRAINS_DIR),
        Some(registry_root.clone()),
    ));
    let app_state = Arc::new(AppState {
        session,
        tool_schemas_cache,
        event_tx,
        event_log_path: None,
        registry_dir: Some(registry_root),
        mcp_sessions: crate::mcp_http::new_mcp_session_registry(),
        project_brains,
    });
    {
        let session = app_state.session.lock();
        let _ = session
            .instance
            .set_running_endpoint("127.0.0.1".into(), port);
        // The served owner IS the medulla — stamp its on-disk registry entry so a
        // sibling owner listing it reads the honest kind (the self-listing path
        // stamps it too, but only THIS process can label its own entry on disk).
        if session.is_medulla_store() {
            let _ = session.instance.set_brain_kind("medulla");
        }
    }
    let _heartbeat = {
        let session = app_state.session.lock();
        spawn_heartbeat(session.instance.clone())
    };

    // Router (embedded UI, not dev mode)
    let router = build_router(app_state.clone(), false);

    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", port)
        .parse()
        .expect("valid socket addr");

    tokio::spawn(async move {
        match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => {
                eprintln!("[m1nd-mcp] m1nd GUI: http://localhost:{}", port);
                // Auto-open browser after short delay
                let open_port = port;
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(400)).await;
                    let url = format!("http://localhost:{}", open_port);
                    let _ = open_browser(&url);
                });
                // Serve until process exits (no graceful shutdown needed — stdio owns lifecycle)
                let _ = axum::serve(listener, router).await;
                let mut session = app_state.session.lock();
                let _ = session.persist();
                let _ = session.instance.release();
            }
            Err(e) => {
                eprintln!(
                    "[m1nd-mcp] Background HTTP server failed to bind to {}: {} (GUI unavailable)",
                    addr, e
                );
                let session = app_state.session.lock();
                let _ = session.instance.release();
            }
        }
    })
}

/// Pure network-exposure decision (no I/O, no exit) so it is unit-testable in
/// both directions. Returns `Err(one_line_error)` when the process MUST refuse to
/// start, `Ok(warning)` otherwise — `Some(warning)` when the bind is remote and
/// allowed (a strong exposure warning to print), `None` when it is loopback.
///
/// The rule: a bind that does NOT resolve to a loopback address exposes graph
/// mutation to the network, and there is no authentication yet — so it is refused
/// unless `allow_remote` is set. This is stricter than a literal `== "0.0.0.0"`
/// check: `0.0.0.0`, `::`, and any concrete LAN IP (e.g. `192.168.1.10`) are all
/// non-loopback and all gated. A hostname that does not parse as an IP is treated
/// as potentially remote (fail-closed) and requires the flag too.
fn remote_bind_verdict(bind: &str, allow_remote: bool) -> Result<Option<String>, String> {
    use std::net::IpAddr;

    let is_loopback = bind
        .trim()
        .parse::<IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false);

    if is_loopback {
        return Ok(None);
    }

    if allow_remote {
        Ok(Some(format!(
            "[m1nd-mcp] WARNING: Binding to a non-loopback address ({bind}) exposes the server to the network. \
             No authentication is configured — anyone who can reach this address can read AND MUTATE the graph. \
             Proceeding only because --allow-remote was given."
        )))
    } else {
        Err(format!(
            "[m1nd-mcp] REFUSING to bind to non-loopback address {bind}: this exposes graph mutation to the \
             network and no authentication is configured. Re-run with --allow-remote to opt in explicitly, or \
             bind to a loopback address (the default 127.0.0.1). Token auth is not yet implemented."
        ))
    }
}

/// Start the HTTP server (and optionally stdio).
#[allow(clippy::too_many_arguments)]
pub async fn run(
    config: McpConfig,
    port: u16,
    bind: String,
    allow_remote: bool,
    dev_mode: bool,
    auto_open: bool,
    also_stdio: bool,
    event_log: Option<String>,
    watch_events: Option<String>,
) {
    // Network-exposure gate: a non-loopback bind is REFUSED unless --allow-remote
    // is set (no auth yet — see `remote_bind_verdict`). A refusal exits before any
    // graph load, engine build, or lease is taken.
    match remote_bind_verdict(&bind, allow_remote) {
        Ok(Some(warning)) => eprintln!("{warning}"),
        Ok(None) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    }

    // 1. Create McpServer to load graph + build engines
    let server = match crate::server::McpServer::new(config.clone()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[m1nd-mcp] Failed to create server: {}", e);
            std::process::exit(1);
        }
    };

    // 2. Extract SessionState, wrap in Arc<Mutex> for shared access
    let session_state = server.into_session_state();
    let owner_runtime_root = session_state.runtime_root.clone();
    let session = Arc::new(Mutex::new(session_state));

    // 3. Cache tool schemas (static, computed once)
    let schemas_full = tool_schemas();
    let tool_schemas_cache = schemas_full
        .get("tools")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));

    // 4. Create SSE broadcast channel (64 event buffer)
    let (event_tx, _) = broadcast::channel::<SseEvent>(64);

    // 5. Resolve event log path (used by both Option A stdio and Option B)
    let event_log_path = event_log.map(std::path::PathBuf::from);

    // 6. Build shared AppState
    let project_brains = Arc::new(crate::project_brains::ProjectBrainRegistry::new(
        owner_runtime_root.join(crate::project_brains::PROJECT_BRAINS_DIR),
        config.registry_dir.clone(),
    ));
    let app_state = Arc::new(AppState {
        session: session.clone(),
        tool_schemas_cache,
        event_tx: event_tx.clone(),
        event_log_path: event_log_path.clone(),
        registry_dir: config.registry_dir.clone(),
        mcp_sessions: crate::mcp_http::new_mcp_session_registry(),
        project_brains,
    });
    {
        let session = app_state.session.lock();
        let _ = session.instance.set_running_endpoint(bind.clone(), port);
        // The served owner IS the medulla — stamp its on-disk registry entry so a
        // sibling owner listing it reads the honest kind (the self-listing path
        // stamps it too, but only THIS process can label its own entry on disk).
        if session.is_medulla_store() {
            let _ = session.instance.set_brain_kind("medulla");
        }
    }
    let _heartbeat = {
        let session = app_state.session.lock();
        spawn_heartbeat(session.instance.clone())
    };

    // 6b. If --watch-events is specified, spawn the event log watcher
    if let Some(ref watch_path) = watch_events {
        let path = std::path::PathBuf::from(watch_path);
        let tx = event_tx.clone();
        tokio::spawn(watch_event_log(path, tx));
    }

    // 7. Build router
    let router = build_router(app_state, dev_mode);

    // 8. Optionally spawn stdio JSON-RPC alongside HTTP
    if also_stdio {
        let stdio_session = session.clone();
        let stdio_event_tx = event_tx.clone();
        let stdio_event_log = event_log_path.clone();
        tokio::task::spawn_blocking(move || {
            eprintln!("[m1nd-mcp] Stdio JSON-RPC also active (--stdio). SSE cross-process bridge enabled.");
            // Run a minimal stdio loop sharing the same session state
            let stdin = std::io::stdin();
            let stdout = std::io::stdout();
            let mut reader = stdin.lock();
            let mut writer = stdout.lock();

            use std::io::{BufRead, Write};
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    _ => {}
                }
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                // Attempt parse as JSON-RPC tool call
                if let Ok(req) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if req.get("method").and_then(|m| m.as_str()) == Some("tools/call") {
                        let tool_name = req
                            .pointer("/params/name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let arguments = req
                            .pointer("/params/arguments")
                            .cloned()
                            .unwrap_or(serde_json::json!({}));
                        let result = {
                            let mut s = stdio_session.lock();
                            if tool_name == "apply_batch" {
                                s.apply_batch_progress_sink = Some(apply_batch_progress_sink(
                                    stdio_event_tx.clone(),
                                    stdio_event_log.clone(),
                                    "stdio".to_string(),
                                    arguments
                                        .get("agent_id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown")
                                        .to_string(),
                                ));
                            }
                            if let Some(agent_id) =
                                arguments.get("agent_id").and_then(|v| v.as_str())
                            {
                                s.track_agent(agent_id);
                            }
                            let result = dispatch_tool(&mut s, tool_name, &arguments);
                            s.apply_batch_progress_sink = None;
                            result
                        };

                        // Broadcast SSE event for cross-process visibility (Option A)
                        let sse_event = SseEvent {
                            event_type: "tool_result".to_string(),
                            data: serde_json::json!({
                                "tool": tool_name,
                                "source": "stdio",
                                "agent_id": arguments.get("agent_id").and_then(|v| v.as_str()).unwrap_or("unknown"),
                                "success": result.is_ok(),
                                "result_preview": match &result {
                                    Ok(v) => truncate_json(v, 500),
                                    Err(e) => serde_json::json!({"error": e.to_string()}),
                                },
                                "timestamp_ms": std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis() as u64)
                                    .unwrap_or(0),
                            }),
                        };
                        let _ = stdio_event_tx.send(sse_event.clone());

                        // Also write to event log file if configured (Option B)
                        if let Some(ref log_path) = stdio_event_log {
                            append_event_to_log(log_path, &sse_event);
                        }
                        let resp = match result {
                            Ok(output) => serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": req.get("id").cloned().unwrap_or(serde_json::Value::Null),
                                "result": { "content": [{ "type": "text", "text": serde_json::to_string(&output).unwrap_or_default() }] }
                            }),
                            Err(e) => serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": req.get("id").cloned().unwrap_or(serde_json::Value::Null),
                                "error": { "code": -32603, "message": e.to_string() }
                            }),
                        };
                        let _ = writeln!(
                            writer,
                            "{}",
                            serde_json::to_string(&resp).unwrap_or_default()
                        );
                        let _ = writer.flush();
                    } else if req.get("method").and_then(|m| m.as_str()) == Some("tools/list") {
                        let schemas = tool_schemas();
                        let resp = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": req.get("id").cloned().unwrap_or(serde_json::Value::Null),
                            "result": schemas
                        });
                        let _ = writeln!(
                            writer,
                            "{}",
                            serde_json::to_string(&resp).unwrap_or_default()
                        );
                        let _ = writer.flush();
                    }
                }
            }
        });
    }

    // 8. Bind and serve
    let addr: std::net::SocketAddr = format!("{}:{}", bind, port).parse().unwrap_or_else(|_| {
        eprintln!("[m1nd-mcp] Invalid bind address: {}:{}", bind, port);
        std::process::exit(1);
    });

    eprintln!("[m1nd-mcp] HTTP server listening on http://{}", addr);

    // 9. Auto-open browser
    if auto_open {
        let url = format!("http://localhost:{}", port);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let _ = open_browser(&url);
        });
    }

    // 10. Graceful shutdown on SIGINT
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            {
                let session = session.lock();
                let _ = session.instance.release();
            }
            eprintln!("[m1nd-mcp] Failed to bind to {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    let shutdown_session = session.clone();
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            eprintln!("[m1nd-mcp] SIGINT received, shutting down...");
            // Persist state on shutdown
            let mut s = shutdown_session.lock();
            if let Err(e) = s.persist() {
                eprintln!("[m1nd-mcp] Failed to persist state on shutdown: {}", e);
            }
            let _ = s.instance.release();
            eprintln!("[m1nd-mcp] State persisted. Goodbye.");
        })
        .await
        .expect("HTTP server failed");
}

/// Open browser (cross-platform).
fn open_browser(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", url])
            .spawn()?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Truncate a JSON value to at most `max_chars` when serialized.
/// Returns the original value if small enough, otherwise a truncated string.
fn truncate_json(value: &serde_json::Value, max_chars: usize) -> serde_json::Value {
    let s = serde_json::to_string(value).unwrap_or_default();
    if s.len() <= max_chars {
        value.clone()
    } else {
        serde_json::Value::String(format!("{}...(truncated)", &s[..max_chars]))
    }
}

/// Build a JSON error payload for tool execution timeouts.
fn timeout_error_payload(timeout_secs: u64) -> serde_json::Value {
    serde_json::json!({
        "error_type": "timeout",
        "timeout_secs": timeout_secs,
        "hint": format!(
            "Tool execution exceeded {}s. Try narrowing scope or using incremental mode.",
            timeout_secs
        ),
    })
}

/// Build a JSON error payload from a M1ndError.
fn tool_error_payload(e: &m1nd_core::error::M1ndError) -> serde_json::Value {
    serde_json::json!({
        "error": "tool_error",
        "message": e.to_string(),
    })
}

// ---------------------------------------------------------------------------
// Router construction
// ---------------------------------------------------------------------------

/// Build the axum router with all routes.
pub fn build_router(state: Arc<AppState>, dev_mode: bool) -> Router {
    let api = Router::new()
        .route("/api/health", get(handle_health))
        .route("/api/instance/self", get(handle_instance_self))
        .route("/api/instances", get(handle_instances))
        .route("/api/instance/save", post(handle_instance_save))
        .route(
            "/api/instances/{instance_id}/save",
            post(handle_instance_save_target),
        )
        .route(
            "/api/instances/{instance_id}/delete-state",
            post(handle_instance_delete_state),
        )
        .route("/api/tools", get(handle_list_tools))
        .route("/api/tools/{*tool_name}", post(handle_tool_call))
        // Streamable-HTTP MCP transport. POST = client→server requests (Slice 1);
        // GET = server→client SSE push, DELETE = session termination (Slice 2).
        .route(
            "/mcp",
            post(crate::mcp_http::handle_mcp_post)
                .get(crate::mcp_http::handle_mcp_get)
                .delete(crate::mcp_http::handle_mcp_delete),
        )
        .route("/api/graph/stats", get(handle_graph_stats))
        .route("/api/graph/subgraph", get(handle_subgraph))
        .route("/api/graph/snapshot", get(handle_graph_snapshot))
        // HUMAN-VIEW-V2 F2 Show Code viewer — a PURE READ of a repo-relative member
        // file under the selected brain's workspace root (anti-escape + byte cap).
        // Read-only: it never mutates, so it rides the same read surface as graph/*.
        .route("/api/file", get(handle_file_view))
        // MEDULLA-PRD §9.2 (slice M7b) — the mailbox read surface. `?brain=` reuses
        // the §4A.9 selector (registered roots only, served_brain echo); the cross-
        // box triage sweep is CLI/REST-only, OFF the MCP surface (§C6.2).
        .route("/api/mailbox", get(handle_mailbox))
        .route("/api/inbox_sweep", get(handle_inbox_sweep))
        .route("/api/events", get(handle_sse))
        .with_state(state.clone())
        .layer(DefaultBodyLimit::max(1_048_576)); // 1MB body limit (FM-A-004)

    if dev_mode {
        let ui_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../m1nd-ui/dist");
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);
        api.fallback_service(tower_http::services::ServeDir::new(ui_dir))
            .layer(cors)
    } else {
        api.fallback(serve_embedded_ui)
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn handle_health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let state = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let session = state.session.lock();
        let graph = session.graph.read();
        let node_count = graph.num_nodes() as usize;
        let edge_count = graph.num_edges();
        drop(graph);
        serde_json::json!({
            "status": if node_count > 0 { "ok" } else { "empty" },
            "uptime_secs": session.uptime_seconds(),
            "node_count": node_count,
            "edge_count": edge_count,
            "queries_processed": session.queries_processed,
            "agent_sessions": session.session_summary(),
            "domain": session.domain.name.as_str(),
            "graph_generation": session.graph_generation,
            "plasticity_generation": session.plasticity_generation,
            "binding_fingerprint": session.binding_fingerprint(),
            "tool_surface_contract": {
                "schema": "m1nd-tool-surface-contract-v0",
                "full_registry_tool_count": crate::server::all_tool_schemas()
                    .get("tools")
                    .and_then(|tools| tools.as_array())
                    .map(|tools| tools.len())
                    .unwrap_or(0),
                "advertised_tool_count": crate::server::tool_schemas()
                    .get("tools")
                    .and_then(|tools| tools.as_array())
                    .map(|tools| tools.len())
                    .unwrap_or(0),
                "tool_tier": crate::server::active_tool_tier(),
                "required_agent_trust_tools": crate::tools::AGENT_TRUST_REQUIRED_TOOLS,
                "required_host_visible_tools": crate::tools::HOST_BINDING_REQUIRED_TOOLS,
                "minimum_safe_tool_count": crate::tools::HOST_BINDING_REQUIRED_TOOLS.len(),
                "recovery_tool": "recovery_playbook",
                "diagnostic_tool": "doctor"
            },
            "host_binding_alignment": {
                "schema": "m1nd-host-binding-alignment-v0",
                "status": "needs_client_surface_comparison",
                "rule": "Compare the host-visible m1nd tool names and count against tool_surface_contract. If trust_selftest, session_handshake, or recovery_playbook is missing, treat this host binding as degraded_host_tool_surface even when health responds.",
                "current_runtime_has_graph": node_count > 0 && edge_count > 0,
                "next_action": "Call trust_selftest with observed_tool_count and available_tools when visible; otherwise use session_handshake, local repo smoke, or refresh the MCP host binding.",
                "non_claims": [
                    "health cannot see which subset of tools the client host injected",
                    "health does not rebind the host or refresh tool schemas automatically"
                ]
            }
        })
    })
    .await
    .expect("spawn_blocking panicked");

    (StatusCode::OK, Json(result))
}

async fn handle_instance_self(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let state = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let session = state.session.lock();
        session.instance_self_summary()
    })
    .await
    .expect("spawn_blocking panicked");

    (StatusCode::OK, Json(result))
}

async fn handle_instances(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let state = state.clone();
    let result = tokio::task::spawn_blocking(move || instances_listing(&state))
        .await
        .expect("spawn_blocking panicked");

    (StatusCode::OK, Json(result))
}

/// The `/api/instances` body — the Hall's single brains surface, now PROJECT-
/// named (HUMAN-LAYER-PRD §4A.3, promoting #260's "REST/GUI bound-only"
/// residue). Every registry entry is enriched with two honest fields BEFORE it
/// reaches the Hall:
///   - `display_name` — the repo basename ("m1nd", "project-b"), the card's
///     name. NEVER the runtime dir ("claude") nor its `agent-memory` sidecar.
///   - `project_root` — the repo the brain maps, the card's path.
///
/// Resolution per entry, at the source of the lie:
///   - the bound/self brain → `SessionState::project_root_display` (skips the
///     agent-memory sidecar + `.light.md` memory files to reach the real repo);
///   - a hosted `brain_kind:"project"` entry → its store manifest's
///     `project_root` (its `workspace_root` is the fingerprint store dir, which
///     is exactly the hash that leaked into the Hall);
///   - a sibling owner → its own `workspace_root` basename (best effort; a
///     foreign owner's manifest is not ours to read).
///
/// The raw runtime fields (`workspace_root`, `runtime_root`, `pid`, …) stay on
/// each entry, demoted to the receipt drawer — nothing is deleted, the headline
/// is just no longer plumbing. Extracted as a pure fn so the enriched shape is
/// unit-testable without an HTTP driver.
pub fn instances_listing(state: &AppState) -> serde_json::Value {
    let instances = match list_instances(state.registry_dir.as_deref()) {
        Ok(instances) => instances,
        Err(error) => {
            return serde_json::json!({ "instances": [], "error": error.to_string() });
        }
    };

    // The owner's own runtime root + its real project identity: the one entry
    // that is "self" is named from the live session, not from its sidecar dir.
    // Canonicalize the runtime root so the self-match survives macOS's
    // `/var` → `/private/var` aliasing (the registry stores the canonical form,
    // the live session carries the raw form — a bare string compare misses).
    let canon_root = |s: &str| {
        std::path::Path::new(s)
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| s.to_string())
    };
    // The owner's own brain also carries the per-brain aliveness the R14 partition
    // (TWO-TIER §9.5.1) puts on its card: its OWN attached-session + query counts,
    // read from its own SessionState in the same lock. These are the bound brain's
    // own numbers — no longer conflated with the project brains' sessions. The
    // owner-WIDE total (sum across all hosted brains) stays only on the owner's
    // receipt (`/api/instances/self`, `/api/health`), labeled owner-wide there.
    let (
        self_runtime_root,
        self_project_root,
        self_display_name,
        self_is_medulla,
        self_attached_sessions,
        self_query_count,
        self_calibration_armed,
    ) = {
        let session = state.session.lock();
        (
            canon_root(&session.runtime_root.to_string_lossy()),
            session.project_root_display(),
            session.display_name(),
            session.is_medulla_store(),
            session.sessions.len() as u64,
            session.queries_processed,
            session.calibration_armed(),
        )
    };
    // The owner it serves IS the medulla (MEDULLA-PRD §4.1: the tier is the
    // directory): its runtime_root holds the promoted/doctrine store, not a
    // per-project brain. On the Hall its card must say so — `medulla`, never the
    // basename of whatever workspace happened to bind this runtime last (the field
    // bug where the medulla card wore the last-bound project's name). The honest
    // name for a medulla self-entry is the literal `medulla`; its real repo path
    // stays on the entry (`project_root`) for the receipt drawer.
    let self_display_name = if self_is_medulla {
        Some("medulla".to_string())
    } else {
        self_display_name
    };
    let store_base = state.project_brains.base_dir().to_path_buf();

    let enriched: Vec<serde_json::Value> = instances
        .into_iter()
        .map(|entry| {
            let mut value = serde_json::to_value(&entry).unwrap_or(serde_json::Value::Null);
            // Per-entry enrichment: the PROJECT name/root, plus (for hosted
            // project brains) recorded counts + freshness, since a project brain
            // lives in-process and has no instance "running"/"lock" status.
            let is_project = entry.brain_kind.as_deref() == Some("project");
            let mut project_counts: Option<(u64, u64)> = None;
            let mut project_updated_ms: Option<u64> = None;
            // The R14 per-brain partition (§9.5.1): each entry's OWN aliveness —
            // `(attached_sessions, query_count, calibration_armed)`. `None` = this
            // brain has no live SessionState right now (a dormant project brain),
            // so the live counters render ABSENT, never a fabricated 0 (TT-INV-2).
            let mut brain_aliveness: Option<(u64, u64, bool)> = None;
            // True when THIS entry is the owner's own served brain AND that brain is
            // the medulla — so the card gets stamped `brain_kind:"medulla"` below,
            // the one honest label for the doctrine-tier root (never a project name).
            let mut is_self_medulla = false;

            let (display_name, project_root) = if canon_root(&entry.runtime_root)
                == self_runtime_root
            {
                // The bound/dev graph this owner serves — its OWN counters.
                brain_aliveness = Some((
                    self_attached_sessions,
                    self_query_count,
                    self_calibration_armed,
                ));
                is_self_medulla = self_is_medulla;
                (self_display_name.clone(), self_project_root.clone())
            } else if is_project {
                // A hosted project brain: its workspace_root IS its store dir;
                // the manifest there names the real repo it maps AND records
                // its last-known size (the cheap dormant-count source).
                let store_path = std::path::Path::new(&entry.workspace_root);
                let facts =
                    crate::project_brains::store_facts_for_store(store_path).or_else(|| {
                        // Defensive: if the entry's store path drifted, try
                        // resolving under our own store base by basename.
                        store_path
                            .file_name()
                            .map(|name| store_base.join(name))
                            .and_then(|p| crate::project_brains::store_facts_for_store(&p))
                    });
                match facts {
                    Some(f) => {
                        // Warm brain counts win (live truth); else the
                        // manifest's recorded counts (honest for a dormant
                        // store); else absent — never a fabricated zero.
                        project_counts = state.project_brains.warm_counts(&f.project_root).or(
                            match (f.node_count, f.edge_count) {
                                (Some(n), Some(e)) => Some((n, e)),
                                _ => None,
                            },
                        );
                        // R14 partition (§9.5.1): this brain's OWN aliveness,
                        // from its warm SessionState. A dormant brain (no warm
                        // state) leaves this None → live counters absent-honest.
                        brain_aliveness = state.project_brains.warm_session_stats(&f.project_root);
                        project_updated_ms = f.updated_ms;
                        let name = crate::session::basename_of(&f.project_root);
                        (Some(name), Some(f.project_root))
                    }
                    // Manifest unreadable → honest fallback to the store basename
                    // rather than invent a name.
                    None => (
                        Some(crate::session::basename_of(&entry.workspace_root)),
                        Some(entry.workspace_root.clone()),
                    ),
                }
            } else {
                // A sibling owner: name it by its own workspace basename.
                (
                    Some(crate::session::basename_of(&entry.workspace_root)),
                    Some(entry.workspace_root.clone()),
                )
            };
            // MEDULLA-PRD §9.2 (slice M7b), the D3 face count: `mailbox_open_count`
            // = the repo-side box's `wet_ink + in_flight`. Rendered ONLY when the
            // box FILE exists on disk (a repo with no box yields absent, never a
            // fabricated zero — INV-10 discipline). Reading a small JSONL per card
            // is cheap and skipped entirely when the file is missing.
            let mailbox_open_count: Option<usize> = project_root.as_deref().and_then(|root| {
                let box_path = std::path::Path::new(root).join(crate::mailbox::BOX_REL_PATH);
                if box_path.is_file() {
                    crate::mailbox::mailbox_open_count(&box_path, &foreign_tool_markers()).ok()
                } else {
                    None
                }
            });
            if let Some(map) = value.as_object_mut() {
                // The served owner IS the medulla: stamp the honest kind so the Hall
                // renders a `medulla` card, not the classic bound/dev graph (whose
                // brain_kind is the serde-default None). The UI keys on this; a
                // sibling owner's on-disk entry carries it too (stamped at serve).
                if is_self_medulla {
                    map.insert(
                        "brain_kind".into(),
                        serde_json::Value::String("medulla".into()),
                    );
                }
                map.insert(
                    "display_name".into(),
                    match display_name {
                        Some(n) => serde_json::Value::String(n),
                        None => serde_json::Value::Null,
                    },
                );
                map.insert(
                    "project_root".into(),
                    match project_root {
                        Some(r) => serde_json::Value::String(r),
                        None => serde_json::Value::Null,
                    },
                );
                map.insert(
                    "mailbox_open_count".into(),
                    match mailbox_open_count {
                        Some(n) => serde_json::json!(n),
                        None => serde_json::Value::Null,
                    },
                );
                // R14 per-brain partition (§9.5.1): the entry's OWN aliveness.
                // Present only when a live SessionState backs this brain (the bound
                // brain always; a warm project brain). Absent (null) for a dormant
                // project brain — no live wire sessions to count (TT-INV-2), never 0.
                match brain_aliveness {
                    Some((sessions, queries, calibrated)) => {
                        map.insert("attached_sessions".into(), serde_json::json!(sessions));
                        map.insert("query_count".into(), serde_json::json!(queries));
                        map.insert("calibration_armed".into(), serde_json::json!(calibrated));
                    }
                    None => {
                        map.insert("attached_sessions".into(), serde_json::Value::Null);
                        map.insert("query_count".into(), serde_json::Value::Null);
                        map.insert("calibration_armed".into(), serde_json::Value::Null);
                    }
                }
                if is_project {
                    // Project-brain semantics: counts from the store/warm brain
                    // (absent-honest, never 0), freshness from the manifest, and
                    // NO instance process-status — a project brain has no
                    // "running" state and no lock, so those never render on its
                    // card (the UI keys on brain_kind).
                    map.insert(
                        "node_count".into(),
                        project_counts
                            .map(|(n, _)| serde_json::json!(n))
                            .unwrap_or(serde_json::Value::Null),
                    );
                    map.insert(
                        "edge_count".into(),
                        project_counts
                            .map(|(_, e)| serde_json::json!(e))
                            .unwrap_or(serde_json::Value::Null),
                    );
                    if let Some(ms) = project_updated_ms {
                        map.insert("last_activity_ms".into(), serde_json::json!(ms));
                    }
                }
            }
            value
        })
        .collect();

    let mut enriched = enriched;

    // ── Cold disk union (the "hosted brain vanishes after restart" fix) ──────────
    // The instance registry only re-lists a project brain once a routed call
    // warm-boots it, so after every owner restart a dormant project brain is
    // absent from the Hall until touched (field-proven reincidence: "project-b
    // sumiu"). But a brain that exists on disk IS a brain the Hall must show and
    // `?brain=` can open — listing is a cheap manifest read, never a warm-boot.
    // Union the disk roster in: any store manifest whose canonical root is not
    // already represented (bound root or an already-listed project entry) becomes
    // a synthesized card. The warm/registry entry always wins a collision (it
    // carries live status + a real instance_id); disk fills only the gaps.
    let self_project_key = self_project_root
        .as_deref()
        .map(crate::project_brains::ProjectBrainRegistry::canonical_key);
    let mut present_roots: std::collections::HashSet<String> = enriched
        .iter()
        .filter_map(|e| e.get("project_root").and_then(|v| v.as_str()))
        .map(crate::project_brains::ProjectBrainRegistry::canonical_key)
        .collect();
    if let Some(ref k) = self_project_key {
        present_roots.insert(k.clone());
    }
    for (root_key, facts, store_dir) in state.project_brains.disk_roster() {
        if present_roots.contains(&root_key) {
            continue; // warm/registry entry already represents this brain
        }
        present_roots.insert(root_key.clone());
        // Live counts win if the brain happens to be warm; else the manifest's
        // recorded counts (honest for a dormant store); else absent — never 0.
        let counts = state.project_brains.warm_counts(&root_key).or(
            match (facts.node_count, facts.edge_count) {
                (Some(n), Some(e)) => Some((n, e)),
                _ => None,
            },
        );
        // A stable synthetic id from the store dir name (the fingerprint) — a
        // dormant brain has no live instance lease, but the Hall needs a React key
        // and the receipt a handle; it never collides with a real instance_id.
        let synthetic_id = store_dir
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| format!("project-brain:{n}"))
            .unwrap_or_else(|| format!("project-brain:{root_key}"));
        let name = crate::session::basename_of(&facts.project_root);
        let mut card = serde_json::json!({
            "instance_id": synthetic_id,
            "brain_kind": "project",
            "display_name": name,
            "project_root": facts.project_root,
            // Its store dir is its workspace_root (mirrors a warm project entry).
            "workspace_root": store_dir.to_string_lossy(),
            "node_count": counts.map(|(n, _)| n),
            "edge_count": counts.map(|(_, e)| e),
            // Dormant-on-disk: no live process, no lock, no conflicts. The card
            // keys on brain_kind and shows manifest freshness, never "not running".
            "conflicts": serde_json::Value::Array(vec![]),
            "stale": false,
            "dormant": true,
        });
        if let (Some(obj), Some(ms)) = (card.as_object_mut(), facts.updated_ms) {
            obj.insert("last_activity_ms".into(), serde_json::json!(ms));
            obj.insert("last_heartbeat_ms".into(), serde_json::json!(ms));
        }
        enriched.push(card);
    }

    // Bound-first (§4A.3): the graph THIS owner serves is the home — float it to
    // the top, then keep every other brain in the registry's freshest-first
    // recency order (a stable partition preserves that order for the tail). The
    // bound owner heartbeats continuously, but a just-bootstrapped project brain
    // can carry a marginally fresher stamp — recency alone would bury the home.
    enriched.sort_by_key(|entry| {
        let is_self = entry
            .get("runtime_root")
            .and_then(|v| v.as_str())
            .map(|r| canon_root(r) == self_runtime_root)
            .unwrap_or(false);
        // false→0 sorts before true→1; we want self FIRST, so invert.
        u8::from(!is_self)
    });

    serde_json::json!({ "instances": enriched })
}

async fn handle_instance_save(State(state): State<Arc<AppState>>) -> axum::response::Response {
    let state = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut session = state.session.lock();
        session.persist()?;
        Ok::<serde_json::Value, m1nd_core::error::M1ndError>(session.instance_self_summary())
    })
    .await
    .expect("spawn_blocking panicked");

    match result {
        Ok(output) => (
            StatusCode::OK,
            Json(serde_json::json!({ "result": output })),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(tool_error_payload(&error)),
        )
            .into_response(),
    }
}

fn instance_base_url(entry: &InstanceRegistryEntry) -> Option<String> {
    // Single source of truth for the bind/port → base-URL rule (0.0.0.0 → 127.0.0.1),
    // shared with the `--attach auto` discovery client.
    crate::instance_registry::entry_base_url(entry)
}

async fn handle_instance_save_target(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> axum::response::Response {
    let is_self = {
        let session = state.session.lock();
        session.instance.summary().instance_id == instance_id
    };

    if is_self {
        return handle_instance_save(State(state)).await;
    }

    let instances = match list_instances(state.registry_dir.as_deref()) {
        Ok(instances) => instances,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(tool_error_payload(&error)),
            )
                .into_response()
        }
    };

    let Some(entry) = instances
        .into_iter()
        .find(|entry| entry.instance_id == instance_id)
    else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "instance_not_found",
                "detail": format!("no registered instance {}", instance_id),
            })),
        )
            .into_response();
    };

    let Some(base_url) = instance_base_url(&entry) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "instance_unreachable",
                "detail": format!("instance {} has no HTTP endpoint", instance_id),
            })),
        )
            .into_response();
    };

    let upstream = match reqwest::Client::new()
        .post(format!("{}/api/instance/save", base_url))
        .json(&serde_json::json!({}))
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({
                    "error": "instance_save_forward_failed",
                    "detail": error.to_string(),
                })),
            )
                .into_response()
        }
    };

    let status = upstream.status();
    let payload = upstream
        .json::<serde_json::Value>()
        .await
        .unwrap_or_else(|error| {
            serde_json::json!({
                "error": "instance_save_forward_invalid_response",
                "detail": error.to_string(),
            })
        });

    let returned_instance_id = payload
        .get("result")
        .and_then(|value| value.get("instance"))
        .and_then(|value| value.get("instance_id"))
        .and_then(|value| value.as_str());
    if status.is_success() && returned_instance_id != Some(instance_id.as_str()) {
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({
                "error": "instance_save_forward_identity_mismatch",
                "detail": format!(
                    "forwarded save for {} reached unexpected instance {:?}",
                    instance_id, returned_instance_id
                ),
            })),
        )
            .into_response();
    }

    (status, Json(payload)).into_response()
}

async fn handle_instance_delete_state(
    State(state): State<Arc<AppState>>,
    Path(instance_id): Path<String>,
) -> impl IntoResponse {
    let state = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        delete_instance_state(&instance_id, state.registry_dir.as_deref())
    })
    .await
    .expect("spawn_blocking panicked");

    match result {
        Ok(entry) => (
            StatusCode::OK,
            Json(serde_json::json!({ "deleted": entry })),
        )
            .into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, Json(tool_error_payload(&error))).into_response(),
    }
}

async fn handle_list_tools(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "tools": state.tool_schemas_cache,
            // Capability stamp (HUMAN-LAYER-PRD §4A.9.5): the Hall feature-detects
            // the REST brain selector — Open enables only when this is present, so
            // an old owner without `?brain=` routing keeps the 0T disabled posture
            // (never assumed, never version-sniffed).
            "rest_brain_selector": true,
        })),
    )
}

// ---------------------------------------------------------------------------
// Per-brain selector (HUMAN-LAYER-PRD §4A.9) — the REST brain routing that lets
// the Hall Open any project brain, not just the bound graph.
// ---------------------------------------------------------------------------

/// The `?brain=<project_root>` query parameter shared by the graph browse routes
/// and the tool route. Absent = the bound graph (today's behavior, byte-
/// compatible — the serde-default posture applied to a URL, §4A.9.2).
#[derive(Clone, Debug, serde::Deserialize)]
pub struct BrainQuery {
    /// URL-encoded absolute `project_root` of a brain the owner already holds.
    /// Absent → route to the bound graph.
    pub brain: Option<String>,
}

/// The `/api/mailbox` query (HUMAN-VIEW-V2-F2.5a §2b): the shared `?brain=`
/// selector plus an optional `?kind=mission`. Absent `kind` = today's field-report
/// caixinha, byte-for-byte; `kind=mission` returns per-mission heads instead.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct MailboxQuery {
    pub brain: Option<String>,
    pub kind: Option<String>,
}

/// The `served_brain` echo shape (§4A.9.4): the resolution `instances_listing`
/// already computes, attached to every `/api/graph/*` response so the client can
/// ASSERT what it got against what it asked for and drop mismatches (INV-15).
fn served_brain_json(
    project_root: Option<String>,
    display_name: Option<String>,
) -> serde_json::Value {
    serde_json::json!({
        "project_root": project_root,
        "display_name": display_name,
    })
}

/// The bound graph's `served_brain` echo — its real PROJECT identity (never the
/// `agent-memory` sidecar), the SAME derivation the Brain Chip and the Hall use
/// (`SessionState::project_root_display` / `display_name`).
fn bound_served_brain(session: &Arc<Mutex<SessionState>>) -> serde_json::Value {
    let s = session.lock();
    served_brain_json(s.project_root_display(), s.display_name())
}

/// Resolve the `?brain=` parameter to the session it names and that session's
/// `served_brain` echo — the ONE resolution both REST doors (graph + tools)
/// share, reusing the wire's routing verbatim (`ProjectBrainRegistry`, #260):
///
/// - **Absent** → the bound graph (today's behavior, byte-compatible).
/// - **Names the bound root** → the bound graph (canonical-path match, so the
///   macOS `/var`→`/private/var` alias and a trailing slash both resolve).
/// - **Names a known hosted brain** → that project brain, warm-booting its
///   dormant store on first touch (#230 semantics per store).
/// - **Unknown root** → `Err` with an honest tool_error naming the miss; NEVER a
///   filesystem read, NEVER an auto-create (creation stays consented, §4A.9.3).
///
/// Registered-roots-only is the security line (§4A.9.3): the param adds routing,
/// not exposure — the surface stays loopback-only (`cli.rs`).
///
/// `pub` so the per-brain-selector integration test drives the ONE resolution the
/// HTTP handlers wrap — the real routing, warm-boot, and echo, without an HTTP
/// client (the same altitude `instances_listing` is tested at).
pub fn resolve_brain(
    state: &Arc<AppState>,
    brain: Option<&str>,
) -> Result<(Arc<Mutex<SessionState>>, serde_json::Value), m1nd_core::error::M1ndError> {
    let Some(root) = brain.map(str::trim).filter(|s| !s.is_empty()) else {
        // Absent param = the bound graph, exactly as before.
        let echo = bound_served_brain(&state.session);
        return Ok((state.session.clone(), echo));
    };

    // Does the param name the BOUND graph's own root? Compare on canonical form
    // so `/private/var` aliases and trailing slashes resolve to a match — then the
    // bound session answers, with its bound echo (no double-routing).
    let requested_key = crate::project_brains::ProjectBrainRegistry::canonical_key(root);
    let bound_matches = {
        let s = state.session.lock();
        s.project_root_display()
            .map(|r| {
                crate::project_brains::ProjectBrainRegistry::canonical_key(&r) == requested_key
            })
            .unwrap_or(false)
    };
    if bound_matches {
        let echo = bound_served_brain(&state.session);
        return Ok((state.session.clone(), echo));
    }

    // Otherwise it must be a KNOWN hosted project brain (warm or dormant-on-disk).
    // `resolve` warm-boots a dormant store; `None` = this owner holds no such
    // brain → honest miss, never a filesystem probe of the raw path.
    match state.project_brains.resolve(root) {
        Some(brain_session) => {
            // The hosted brain's identity is its manifest's project_root basename
            // (the SAME name the Hall card wears) — resolved from the canonical key
            // so the echo names the repo, not the fingerprint store dir.
            let display = Some(crate::session::basename_of(&requested_key));
            let echo = served_brain_json(Some(requested_key), display);
            Ok((brain_session, echo))
        }
        None => Err(m1nd_core::error::M1ndError::InvalidParams {
            tool: "brain_selector".into(),
            detail: format!(
                "no brain for '{root}' — the Hall lists what exists. \
                 A brain is created only by consent (`ingest {{project_root}}` or `m1nd init`), \
                 never by browsing to it."
            ),
        }),
    }
}

/// Turn a `/api/graph/*` result into its HTTP response: 200 with the body, or —
/// when the `?brain=` selector named an unknown root — an honest 404-grade
/// tool_error naming the miss (§4A.9.3). NOT_FOUND is the right grade: the human
/// asked for a brain that does not exist, the same way a bad tool name 404s.
fn graph_response(
    result: Result<serde_json::Value, m1nd_core::error::M1ndError>,
) -> axum::response::Response {
    match result {
        Ok(body) => (StatusCode::OK, Json(body)).into_response(),
        Err(e) => {
            let mut payload = tool_error_payload(&e);
            payload["error"] = serde_json::json!("unknown_brain");
            (StatusCode::NOT_FOUND, Json(payload)).into_response()
        }
    }
}

async fn handle_tool_call(
    State(state): State<Arc<AppState>>,
    Path(tool_name): Path<String>,
    Query(brain): Query<BrainQuery>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    // §4A.9.1: the tool route carries the SAME selector as the graph routes —
    // Reading the Tree's lenses/filters/meaning-search (seek, layers, tremor,
    // trust, impact) are all /api/tools calls, so they ride this param and answer
    // from the named brain. Resolve up-front; an unknown root 404s honestly.
    let (target_session, served_echo) = match resolve_brain(&state, brain.brain.as_deref()) {
        Ok(pair) => pair,
        Err(e) => return graph_response(Err(e)),
    };
    // §4A.9.6: brain-scope the mutation event. When the caller selected a brain,
    // stamp its root on the `graph_changed` relay so a viewer only refetches for
    // ITS brain (a bound/absent call leaves it None → the honest over-refetch on
    // old readers still fires). Only meaningful when the param was explicit.
    let brain_root_for_event: Option<String> = if brain.brain.is_some() {
        served_echo
            .get("project_root")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    } else {
        None
    };
    let event_tx = state.event_tx.clone();
    let event_log_path = state.event_log_path.clone();
    let tool_for_event = tool_name.clone();
    let agent_id_for_event = body
        .get("agent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let tool = tool_name.clone();
    let progress_event_tx = event_tx.clone();
    let progress_event_log_path = event_log_path.clone();
    let progress_agent_id = agent_id_for_event.clone();

    // Wrap in timeout (FM-C-004: 30s per tool)
    let result = tokio::time::timeout(
        Duration::from_secs(TOOL_TIMEOUT_SECS),
        tokio::task::spawn_blocking(move || {
            // §4A.9: dispatch against the SELECTED brain (bound when absent) — the
            // resolution already validated the root, so this is the same brain the
            // graph routes would serve for this selector.
            let mut session = target_session.lock();
            if tool == "apply_batch" {
                session.apply_batch_progress_sink = Some(apply_batch_progress_sink(
                    progress_event_tx.clone(),
                    progress_event_log_path.clone(),
                    "http".to_string(),
                    progress_agent_id.clone(),
                ));
            }
            if let Some(agent_id) = body.get("agent_id").and_then(|v| v.as_str()) {
                session.track_agent(agent_id);
            }
            let result = dispatch_tool(&mut session, &tool, &body);
            session.apply_batch_progress_sink = None;
            result
        }),
    )
    .await;

    match result {
        Err(_elapsed) => {
            // Broadcast timeout event
            let sse_event = SseEvent {
                event_type: "tool_timeout".to_string(),
                data: serde_json::json!({
                    "tool": tool_for_event,
                    "source": "http",
                    "agent_id": agent_id_for_event,
                    "timeout_secs": TOOL_TIMEOUT_SECS,
                    "timestamp_ms": now_ms(),
                }),
            };
            let _ = event_tx.send(sse_event.clone());
            if let Some(ref log_path) = event_log_path {
                append_event_to_log(log_path, &sse_event);
            }

            (
                StatusCode::GATEWAY_TIMEOUT,
                Json(timeout_error_payload(TOOL_TIMEOUT_SECS)),
            )
                .into_response()
        }
        Ok(inner) => {
            let inner = inner.expect("spawn_blocking panicked");

            // Broadcast SSE event for the tool result
            let mut result_data = serde_json::json!({
                "tool": tool_for_event,
                "source": "http",
                "agent_id": agent_id_for_event,
                "success": inner.is_ok(),
                "result_preview": match &inner {
                    Ok(v) => tool_result_summary(&tool_for_event, v),
                    Err(e) => serde_json::json!({"error": e.to_string()}),
                },
                "timestamp_ms": now_ms(),
            });
            // §4A.9.6: name WHICH brain mutated (additive; absent for bound/old).
            if let (Some(obj), Some(root)) = (result_data.as_object_mut(), &brain_root_for_event) {
                obj.insert("brain_root".into(), serde_json::json!(root));
            }
            let sse_event = SseEvent {
                event_type: "tool_result".to_string(),
                data: result_data,
            };
            let _ = event_tx.send(sse_event.clone());
            if let Some(ref log_path) = event_log_path {
                append_event_to_log(log_path, &sse_event);
            }
            if let Ok(output) = &inner {
                if tool_for_event == "apply_batch" {
                    emit_apply_batch_handoff(
                        &event_tx,
                        event_log_path.as_ref(),
                        "http",
                        &agent_id_for_event,
                        output,
                    );
                }
            }
            match inner {
                Ok(output) => (
                    StatusCode::OK,
                    Json(serde_json::json!({ "result": output })),
                )
                    .into_response(),
                Err(e) => {
                    let (status, error_type) = match &e {
                        m1nd_core::error::M1ndError::UnknownTool { .. } => {
                            (StatusCode::NOT_FOUND, "unknown_tool")
                        }
                        m1nd_core::error::M1ndError::InvalidParams { .. } => {
                            (StatusCode::BAD_REQUEST, "invalid_params")
                        }
                        m1nd_core::error::M1ndError::Serde(_) => {
                            (StatusCode::BAD_REQUEST, "invalid_json")
                        }
                        _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
                    };
                    let mut payload = tool_error_payload(&e);
                    payload["error"] = serde_json::json!(error_type);
                    (status, Json(payload)).into_response()
                }
            }
        }
    }
}

async fn handle_subgraph(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SubgraphQuery>,
) -> impl IntoResponse {
    let state = state.clone();
    let top_k = params.clamped_top_k(); // Cap at 100 (FM-FE-001)
    let query = params.query.clone();
    let brain_param = params.brain.clone();

    // §4A.9: resolve the target brain up-front — an unknown root 404s before any
    // work; a known one (bound when absent) hands back its session + echo. The
    // echo rides the response meta so the client can assert it (INV-15).
    let (target, served_brain) = match resolve_brain(&state, brain_param.as_deref()) {
        Ok(pair) => pair,
        Err(e) => return graph_response(Err(e)),
    };

    let result: serde_json::Value = tokio::task::spawn_blocking(move || {
        let start = std::time::Instant::now();
        let mut session = target.lock();

        // 1. Run activate internally to get top-K nodes
        let activate_params = serde_json::json!({
            "query": query,
            "agent_id": "gui-subgraph",
            "top_k": top_k,
            "include_ghost_edges": true,
            "include_structural_holes": false,
        });
        let activate_result = dispatch_tool(&mut session, "m1nd_activate", &activate_params);

        match activate_result {
            Err(e) => {
                // Return empty subgraph on activate failure
                serde_json::json!({
                    "nodes": [],
                    "edges": [],
                    "meta": {
                        "total_nodes": 0,
                        "rendered_nodes": 0,
                        "query": query,
                        "elapsed_ms": start.elapsed().as_millis() as u64,
                        "error": e.to_string(),
                    }
                })
            }
            Ok(output) => {
                let graph = session.graph.read();
                let n = graph.num_nodes() as usize;

                // Build reverse map: NodeId -> external_id
                let mut node_to_ext: Vec<String> = vec![String::new(); n];
                for (interned, &nid) in &graph.id_to_node {
                    let idx = nid.as_usize();
                    if idx < n {
                        node_to_ext[idx] = graph.strings.resolve(*interned).to_string();
                    }
                }

                // 2. Extract activated node IDs from activate result
                let activated = output
                    .get("activated")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();

                let total_nodes = activated.len();

                // Collect top_k node external IDs and resolve to NodeIds
                let mut top_node_ids: Vec<m1nd_core::types::NodeId> = Vec::new();
                let mut top_ext_ids: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                let mut subgraph_nodes: Vec<serde_json::Value> = Vec::new();

                for node_val in activated.iter().take(top_k) {
                    let ext_id = node_val
                        .get("node_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if ext_id.is_empty() {
                        continue;
                    }
                    if let Some(nid) = graph.resolve_id(ext_id) {
                        let idx = nid.as_usize();
                        if idx < n {
                            top_node_ids.push(nid);
                            top_ext_ids.insert(ext_id.to_string());

                            let label = graph.strings.resolve(graph.nodes.label[idx]).to_string();
                            let node_type_val = node_type_to_u8(graph.nodes.node_type[idx]);
                            let activation = node_val
                                .get("activation")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0) as f32;
                            let tags: Vec<String> = graph.nodes.tags[idx]
                                .iter()
                                .map(|&t| graph.strings.resolve(t).to_string())
                                .collect();
                            let provenance = graph.resolve_node_provenance(nid);
                            let pagerank = if idx < graph.nodes.pagerank.len() {
                                graph.nodes.pagerank[idx].get()
                            } else {
                                0.0
                            };

                            subgraph_nodes.push(serde_json::json!({
                                "id": ext_id,
                                "label": label,
                                "node_type": node_type_val,
                                "activation": activation,
                                "tags": tags,
                                "source_path": provenance.source_path,
                                "pagerank": pagerank,
                            }));
                        }
                    }
                }

                // 3. Collect edges between top-K nodes
                let mut subgraph_edges: Vec<serde_json::Value> = Vec::new();
                for &nid in &top_node_ids {
                    if !graph.finalized {
                        continue;
                    }
                    let range = graph.csr.out_range(nid);
                    for j in range {
                        let tgt = graph.csr.targets[j];
                        let tgt_idx = tgt.as_usize();
                        if tgt_idx < n && top_ext_ids.contains(&node_to_ext[tgt_idx]) {
                            let src_ext = &node_to_ext[nid.as_usize()];
                            let tgt_ext = &node_to_ext[tgt_idx];
                            let weight = graph
                                .csr
                                .read_weight(m1nd_core::types::EdgeIdx::new(j as u32))
                                .get();
                            let relation =
                                graph.strings.resolve(graph.csr.relations[j]).to_string();
                            subgraph_edges.push(serde_json::json!({
                                "source": src_ext,
                                "target": tgt_ext,
                                "weight": weight,
                                "relation": relation,
                            }));
                        }
                    }
                }

                // 4. Also add ghost edges from activate output
                if let Some(ghost_edges) = output.get("ghost_edges").and_then(|v| v.as_array()) {
                    for ge in ghost_edges {
                        let src = ge.get("source").and_then(|v| v.as_str()).unwrap_or("");
                        let tgt = ge.get("target").and_then(|v| v.as_str()).unwrap_or("");
                        if top_ext_ids.contains(src) && top_ext_ids.contains(tgt) {
                            let strength =
                                ge.get("strength").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            subgraph_edges.push(serde_json::json!({
                                "source": src,
                                "target": tgt,
                                "weight": strength,
                                "relation": "ghost",
                            }));
                        }
                    }
                }

                let rendered = subgraph_nodes.len();
                serde_json::json!({
                    "nodes": subgraph_nodes,
                    "edges": subgraph_edges,
                    "meta": {
                        "total_nodes": total_nodes,
                        "rendered_nodes": rendered,
                        "query": query,
                        "elapsed_ms": start.elapsed().as_millis() as u64,
                    }
                })
            }
        }
    })
    .await
    .expect("spawn_blocking panicked");

    // §4A.9.4: attach the served_brain echo at the top level (same place as
    // stats/snapshot) so every graph door speaks the same INV-15 language.
    let mut result = result;
    if let Some(obj) = result.as_object_mut() {
        obj.insert("served_brain".into(), served_brain);
    }

    (StatusCode::OK, Json(result)).into_response()
}

async fn handle_graph_stats(
    State(state): State<Arc<AppState>>,
    Query(brain): Query<BrainQuery>,
) -> impl IntoResponse {
    let state = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        // §4A.9: route to the named brain (bound when absent), echo served_brain.
        let (session, served_brain) = resolve_brain(&state, brain.brain.as_deref())?;
        let session = session.lock();
        let graph = session.graph.read();
        Ok::<_, m1nd_core::error::M1ndError>(serde_json::json!({
            "node_count": graph.num_nodes(),
            "edge_count": graph.num_edges(),
            "domain": session.domain.name.as_str(),
            "namespaces": serde_json::Value::Array(vec![]),
            "memory_estimate_bytes": 0_usize,
            "served_brain": served_brain,
        }))
    })
    .await
    .expect("spawn_blocking panicked");

    graph_response(result)
}

async fn handle_graph_snapshot(
    State(state): State<Arc<AppState>>,
    Query(brain): Query<BrainQuery>,
) -> impl IntoResponse {
    let state = state.clone();
    let result: Result<serde_json::Value, m1nd_core::error::M1ndError> =
        tokio::task::spawn_blocking(move || {
        // §4A.9: route to the named brain (bound when absent), echo served_brain.
        let (session, served_brain) = resolve_brain(&state, brain.brain.as_deref())?;
        let session = session.lock();
        let graph = session.graph.read();
        let n = graph.num_nodes() as usize;

        // Build reverse map: NodeId -> external_id
        let mut node_to_ext: Vec<String> = vec![String::new(); n];
        for (interned, &nid) in &graph.id_to_node {
            let idx = nid.as_usize();
            if idx < n {
                node_to_ext[idx] = graph.strings.resolve(*interned).to_string();
            }
        }

        // Serialize nodes
        let mut nodes = Vec::with_capacity(n);
        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            let label = graph.strings.resolve(graph.nodes.label[i]).to_string();
            let tags: Vec<String> = graph.nodes.tags[i]
                .iter()
                .map(|&t| graph.strings.resolve(t).to_string())
                .collect();
            let provenance = graph.resolve_node_provenance(m1nd_core::types::NodeId::new(i as u32));
            nodes.push(serde_json::json!({
                "external_id": node_to_ext[i],
                "label": label,
                "node_type": node_type_to_u8(graph.nodes.node_type[i]),
                "tags": tags,
                "last_modified": graph.nodes.last_modified[i],
                "change_frequency": graph.nodes.change_frequency[i].get(),
                "provenance": {
                    "source_path": provenance.source_path,
                    "line_start": provenance.line_start,
                    "line_end": provenance.line_end,
                    "namespace": provenance.namespace,
                    "canonical": provenance.canonical,
                },
            }));
        }

        // Serialize edges from CSR
        let mut edges = Vec::new();
        if graph.finalized {
            for src in 0..n {
                let range = graph.csr.out_range(m1nd_core::types::NodeId::new(src as u32));
                for j in range {
                    let tgt = graph.csr.targets[j].as_usize();
                    let dir = graph.csr.directions[j];
                    // For bidirectional edges, only save canonical direction
                    if dir == m1nd_core::types::EdgeDirection::Bidirectional && src > tgt {
                        continue;
                    }
                    let relation = graph.strings.resolve(graph.csr.relations[j]).to_string();
                    let weight = graph.csr.read_weight(m1nd_core::types::EdgeIdx::new(j as u32)).get();
                    edges.push(serde_json::json!({
                        "source_id": node_to_ext[src],
                        "target_id": node_to_ext[tgt],
                        "relation": relation,
                        "weight": weight,
                        "direction": if dir == m1nd_core::types::EdgeDirection::Bidirectional { 1 } else { 0 },
                        "inhibitory": graph.csr.inhibitory[j],
                        "causal_strength": graph.csr.causal_strengths[j].get(),
                    }));
                }
            }
        }

        Ok(serde_json::json!({
            "version": 1,
            "nodes": nodes,
            "edges": edges,
            // §4A.9.4: the served_brain echo makes INV-15 testable — the client
            // asserts this against what it asked for and drops mismatches.
            "served_brain": served_brain,
        }))
    })
    .await
    .expect("spawn_blocking panicked");

    graph_response(result)
}

/// The `?path=<repo-relative>[&brain=<root>]` query for the Show Code viewer.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct FileViewQuery {
    /// Repo-relative path of the member file to view (read-only).
    pub path: String,
    /// The §4A.9 brain selector (absent = the bound brain).
    pub brain: Option<String>,
}

/// `GET /api/file?path=<repo-relative>[&brain=<root>]` (HUMAN-VIEW-V2 F2 Show Code).
/// A PURE READ: returns a member file's content under the selected brain's
/// workspace root, enforcing the seed's anti-absolute/anti-escape law + a byte cap
/// with honest truncation (`crate::system_blocks::read_repo_relative_file`). Never
/// mutates — safe under a read-only attach, so it is NOT in the write deny-list.
/// Path validation/escape → 400; a missing file or an unknown `?brain=` → 404.
async fn handle_file_view(
    State(state): State<Arc<AppState>>,
    Query(q): Query<FileViewQuery>,
) -> impl IntoResponse {
    let state = state.clone();
    let outcome = tokio::task::spawn_blocking(
        move || -> Result<serde_json::Value, (StatusCode, m1nd_core::error::M1ndError)> {
            // §4A.9: route to the named brain (bound when absent); an unknown root
            // 404s honestly (the same grade the graph routes give it).
            let (session, served_brain) = resolve_brain(&state, q.brain.as_deref())
                .map_err(|e| (StatusCode::NOT_FOUND, e))?;
            let root = {
                let s = session.lock();
                s.workspace_root.clone()
            }
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    m1nd_core::error::M1ndError::InvalidParams {
                        tool: "file_view".into(),
                        detail: "no workspace root is bound to this brain".into(),
                    },
                )
            })?;
            match crate::system_blocks::read_repo_relative_file(
                std::path::Path::new(&root),
                &q.path,
                crate::system_blocks::FILE_VIEW_MAX_BYTES,
            ) {
                Ok(read) => Ok(serde_json::json!({
                    "path": q.path,
                    "content": read.content,
                    "bytes": read.bytes,
                    "truncated": read.truncated,
                    "max_bytes": crate::system_blocks::FILE_VIEW_MAX_BYTES,
                    "served_brain": served_brain,
                })),
                // Absolute/escape refusals are a client error (400); a missing or
                // unreadable file is a 404 — the file the human asked for isn't there.
                Err(e @ crate::system_blocks::SeedError::AbsolutePath { .. }) => Err((
                    StatusCode::BAD_REQUEST,
                    m1nd_core::error::M1ndError::InvalidParams {
                        tool: "file_view".into(),
                        detail: e.to_string(),
                    },
                )),
                Err(e) => Err((
                    StatusCode::NOT_FOUND,
                    m1nd_core::error::M1ndError::InvalidParams {
                        tool: "file_view".into(),
                        detail: e.to_string(),
                    },
                )),
            }
        },
    )
    .await
    .expect("spawn_blocking panicked");

    match outcome {
        Ok(body) => (StatusCode::OK, Json(body)).into_response(),
        Err((code, e)) => (code, Json(tool_error_payload(&e))).into_response(),
    }
}

/// The set of tool markers judged `external` (about a tool that is NOT m1nd) for
/// fate derivation (§C2.2). A transversal tool like Context7 filed a mailbox
/// letter about itself — it is not m1nd's to close, so its letter wears `◌`.
/// Neutral, small, extendable; empty would make nothing external.
fn foreign_tool_markers() -> std::collections::BTreeSet<String> {
    ["context7", "browseros", "playwright", "semgrep"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// The project roots this owner knows (bound root + every disk-roster brain) —
/// the inputs to `boxes_from_roots`. De-duplicated by the registry's canonical key.
fn known_project_roots(state: &Arc<AppState>) -> Vec<String> {
    let mut roots: Vec<String> = Vec::new();
    if let Some(bound) = state.session.lock().project_root_display() {
        roots.push(bound);
    }
    for (_key, facts, _dir) in state.project_brains.disk_roster() {
        roots.push(facts.project_root);
    }
    roots
}

/// The bound owner's runtime root (the medulla box's home) + the worktree base
/// name (the bound project's basename, whose worktrees are `<base>-*`).
fn owner_runtime_and_base(state: &Arc<AppState>) -> (std::path::PathBuf, String) {
    let s = state.session.lock();
    let base = s
        .project_root_display()
        .as_deref()
        .map(crate::session::basename_of)
        .unwrap_or_default();
    (s.runtime_root.clone(), base)
}

/// `GET /api/mailbox?brain=<project_root>` (MEDULLA-PRD §9.2, slice M7b) — returns
/// ONLY the named brain's box letters with derived fates + counts + the
/// `served_brain` echo (the §4A.9 selector contract reused verbatim). `?brain=`
/// absent → the bound brain's box; `?brain=medulla` → the medulla box (the
/// projectless letters). The read is scoped to THIS box only, never a re-fold of
/// the spool (MED-INV-1 / INV-17).
async fn handle_mailbox(
    State(state): State<Arc<AppState>>,
    Query(q): Query<MailboxQuery>,
) -> impl IntoResponse {
    let state = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let foreign = foreign_tool_markers();
        let (runtime_root, _worktree_base) = owner_runtime_and_base(&state);

        // Resolve (served_brain, box_path) ONCE — the same box the field-report and
        // mission-head views both read, and the same box `mission_post` writes.
        //
        // The medulla box is addressed by the literal `medulla` selector; otherwise
        // the §4A.9 selector resolves the brain (registered roots only) and its box
        // is that repo's repo-side file (a memory-only brain → the medulla box).
        let is_medulla = q
            .brain
            .as_deref()
            .map(|b| b.trim().eq_ignore_ascii_case("medulla"))
            .unwrap_or(false);
        let (served_brain, box_path) = if is_medulla {
            (
                served_brain_json(Some("medulla".into()), Some("medulla".into())),
                crate::mailbox::medulla_box_path(&runtime_root),
            )
        } else {
            let (session, served_brain) = resolve_brain(&state, q.brain.as_deref())?;
            let repo_root = {
                let s = session.lock();
                s.project_root_display()
            };
            let box_path = match repo_root {
                Some(root) => std::path::Path::new(&root).join(crate::mailbox::BOX_REL_PATH),
                None => crate::mailbox::medulla_box_path(&runtime_root),
            };
            (served_brain, box_path)
        };

        // F2.5a §2b: `kind=mission` returns per-mission heads (the §1e chain) +
        // honest superseded counts. Absent `kind` = today's caixinha byte-for-byte.
        let is_mission = q
            .kind
            .as_deref()
            .map(|k| {
                k.trim()
                    .eq_ignore_ascii_case(crate::mission_letter::KIND_MISSION)
            })
            .unwrap_or(false);
        if is_mission {
            let letters = crate::mailbox::read_letters(&box_path)?;
            let heads: Vec<crate::mission_letter::MissionHead> =
                crate::mission_letter::heads_by_mission(&letters)
                    .into_values()
                    .collect();
            return Ok::<_, m1nd_core::error::M1ndError>(serde_json::json!({
                "served_brain": served_brain,
                "missions": heads,
            }));
        }

        let view = crate::mailbox::read_box(&box_path, &foreign)?;
        Ok::<_, m1nd_core::error::M1ndError>(serde_json::json!({
            "served_brain": served_brain,
            "letters": view.letters,
            "counts": {
                "wet_ink": view.counts.wet_ink,
                "in_flight": view.counts.in_flight,
                "fired_clay": view.counts.fired_clay,
                "external": view.counts.external,
                "open": view.counts.open(),
            },
        }))
    })
    .await
    .expect("spawn_blocking panicked");

    graph_response(result)
}

/// `GET /api/inbox_sweep` (MEDULLA-PRD §9.2, §C6.2 — CLI/REST only, OFF the MCP
/// surface): the triage session's whole view — spool ∪ every known box,
/// de-duplicated by content id (each letter once), with any unreachable box
/// NAMED, never silently skipped. No `?brain=` selector: the sweep is
/// deliberately cross-box (the m1nd team keeps seeing the conjunto).
async fn handle_inbox_sweep(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let state = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let foreign = foreign_tool_markers();
        let (runtime_root, worktree_base) = owner_runtime_and_base(&state);
        let spool = crate::mailbox::spool_path_for_runtime(&runtime_root);

        let roots = known_project_roots(&state);
        let (_known, mut boxes) = crate::mailbox::boxes_from_roots(&roots, &worktree_base);
        // The medulla box is a known box too (always reachable — it is owner-local).
        boxes.push(crate::mailbox::KnownBox {
            label: "medulla".into(),
            path: crate::mailbox::medulla_box_path(&runtime_root),
            reachable: true,
        });

        let sweep = crate::mailbox::inbox_sweep(&spool, &boxes, &foreign)?;
        Ok::<_, m1nd_core::error::M1ndError>(serde_json::json!({
            "schema": "m1nd-inbox-sweep-v0",
            "letters": sweep.letters,
            "total": sweep.total,
            "open": sweep.open,
            "misdelivery": sweep.misdelivery,
            "unreachable": sweep.unreachable,
        }))
    })
    .await
    .expect("spawn_blocking panicked");

    graph_response(result)
}

/// Build the browser-facing `graph_changed` SSE event from a broadcast event, or
/// `None` if the event is not a shared-graph mutation.
///
/// This is the browser (pure-reader) rendering of the SAME mutation boundary the
/// MCP relay uses (`mcp_http::graph_mutation_event_name`) — reused, never
/// duplicated. It closes the #233 "SSE pure-reader relay gap": the MCP transport
/// already turns mutation events into `notifications/m1nd/graph_changed` for
/// attached agents, but a plain browser reading `/api/events` never saw a
/// `graph_changed` class. The Living Tree subscribes to it to refresh in place
/// (PRD §5.3). The payload is minimal — `event` (which mutation) + non-echoing
/// context — the tree re-fetches the snapshot rather than trusting a diff.
fn browser_graph_changed_event(event: &SseEvent) -> Option<SseEvent> {
    let name = crate::mcp_http::graph_mutation_event_name(event)?;
    let mut detail = serde_json::Map::new();
    detail.insert("event".into(), serde_json::json!(name));
    // `brain_root` (§4A.9.6) rides along when the mutation named a brain — the
    // viewer refetches only for ITS brain (or when the field is absent, the
    // honest over-refetch on old owners). Additive: absent on bound/legacy events.
    for key in [
        "agent_id",
        "source",
        "batch_id",
        "timestamp_ms",
        "brain_root",
    ] {
        if let Some(v) = event.data.get(key) {
            detail.insert(key.into(), v.clone());
        }
    }
    Some(SseEvent {
        event_type: "graph_changed".to_string(),
        data: serde_json::Value::Object(detail),
    })
}

async fn handle_sse(
    State(state): State<Arc<AppState>>,
) -> Sse<impl futures::Stream<Item = Result<sse::Event, std::convert::Infallible>>> {
    let rx = state.event_tx.subscribe();
    let stream = tokio_stream::wrappers::BroadcastStream::new(rx).flat_map(|event| {
        // Each broadcast event is relayed raw (unchanged legacy behavior). When
        // the event is a shared-graph mutation it ALSO yields a derived
        // `graph_changed` event so a plain browser can refresh live (#233 fix).
        let frames: Vec<Result<sse::Event, std::convert::Infallible>> = match event {
            Ok(e) => {
                let mut out = Vec::with_capacity(2);
                if let Some(changed) = browser_graph_changed_event(&e) {
                    if let Ok(ev) = sse::Event::default()
                        .event(changed.event_type)
                        .json_data(changed.data)
                    {
                        out.push(Ok(ev));
                    }
                }
                if let Ok(ev) = sse::Event::default().event(e.event_type).json_data(e.data) {
                    out.push(Ok(ev));
                }
                out
            }
            Err(_) => Vec::new(),
        };
        futures::stream::iter(frames)
    });
    Sse::new(stream)
}

/// Map NodeType to u8 for JSON serialization.
fn node_type_to_u8(nt: m1nd_core::types::NodeType) -> u8 {
    use m1nd_core::types::NodeType;
    match nt {
        NodeType::File => 0,
        NodeType::Directory => 1,
        NodeType::Function => 2,
        NodeType::Class => 3,
        NodeType::Struct => 4,
        NodeType::Enum => 5,
        NodeType::Type => 6,
        NodeType::Module => 7,
        NodeType::Reference => 8,
        NodeType::Concept => 9,
        NodeType::Material => 10,
        NodeType::Process => 11,
        NodeType::Product => 12,
        NodeType::Supplier => 13,
        NodeType::Regulatory => 14,
        NodeType::System => 15,
        NodeType::Cost => 16,
        NodeType::Custom(v) => v,
    }
}

/// Serve embedded UI assets (rust-embed). SPA fallback to index.html.
async fn serve_embedded_ui(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match UiAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref().to_string())],
                content.data.into_owned(),
            )
                .into_response()
        }
        None => {
            // SPA fallback: serve index.html for client-side routing
            match UiAssets::get("index.html") {
                Some(content) => (
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "text/html".to_string())],
                    content.data.into_owned(),
                )
                    .into_response(),
                None => (StatusCode::NOT_FOUND, "UI not built").into_response(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_followup_events_replays_apply_batch_progress() {
        let (tx, mut rx) = broadcast::channel::<SseEvent>(16);
        let output = serde_json::json!({
            "batch_id": "batch-1",
            "proof_state": "proving",
            "next_suggested_tool": "heuristics_surface",
            "next_suggested_target": "src/core.py",
            "next_step_hint": "Inspect the hotspot before promotion.",
            "progress_events": [
                {
                    "batch_id": "batch-1",
                    "event_type": "phase_completed",
                    "phase": "validate",
                    "phase_index": 0,
                    "progress_pct": 20.0
                },
                {
                    "batch_id": "batch-1",
                    "event_type": "batch_completed",
                    "phase": "done",
                    "phase_index": 4,
                    "progress_pct": 100.0
                }
            ]
        });

        emit_followup_events(&tx, None, "apply_batch", "http", "tester", &output);

        let first = rx.try_recv().expect("first progress event");
        let second = rx.try_recv().expect("second progress event");
        let third = rx.try_recv().expect("handoff event");
        assert_eq!(first.event_type, "apply_batch_progress");
        assert_eq!(second.event_type, "apply_batch_progress");
        assert_eq!(third.event_type, "apply_batch_handoff");
        assert_eq!(first.data["batch_id"].as_str(), Some("batch-1"));
        assert_eq!(second.data["batch_id"].as_str(), Some("batch-1"));
        assert_eq!(third.data["batch_id"].as_str(), Some("batch-1"));
        assert_eq!(
            third.data["next_suggested_tool"].as_str(),
            Some("heuristics_surface")
        );
        assert_eq!(first.data["progress"]["phase"].as_str(), Some("validate"));
        assert_eq!(second.data["progress"]["phase"].as_str(), Some("done"));
    }

    #[test]
    fn tool_result_summary_compacts_apply_batch_for_sse_consumers() {
        let output = serde_json::json!({
            "batch_id": "batch-42",
            "proof_state": "ready_to_edit",
            "active_phase": "done",
            "progress_pct": 100.0,
            "next_step_hint": "Safe to continue.",
            "verification": {"verdict": "SAFE"},
            "progress_events": [{}, {}, {}]
        });

        let summary = tool_result_summary("apply_batch", &output);
        assert_eq!(summary["batch_id"], "batch-42");
        assert_eq!(summary["proof_state"], "ready_to_edit");
        assert_eq!(summary["verification_verdict"], "SAFE");
        assert_eq!(summary["progress_event_count"], 3);
    }

    #[test]
    fn emit_apply_batch_handoff_skips_empty_payloads() {
        let (tx, mut rx) = broadcast::channel::<SseEvent>(16);
        emit_apply_batch_handoff(&tx, None, "http", "tester", &serde_json::json!({}));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn timeout_payload_teaches_how_to_retry() {
        let payload = timeout_error_payload(30);
        assert_eq!(payload["error_type"], "timeout");
        assert!(payload["hint"].as_str().expect("hint").contains("scope"));
    }

    // ---- Browser `graph_changed` relay (#233 pure-reader gap fix) ----------

    fn sse(event_type: &str, data: serde_json::Value) -> SseEvent {
        SseEvent {
            event_type: event_type.to_string(),
            data,
        }
    }

    #[test]
    fn browser_relays_graph_changed_for_a_successful_mutation() {
        // A landed `memorize` mutates the shared graph → the browser must get a
        // `graph_changed` event so the Living Tree refreshes in place (PRD §5.3).
        let e = sse(
            "tool_result",
            serde_json::json!({
                "tool": "memorize",
                "success": true,
                "agent_id": "agent-b",
                "source": "http",
                "timestamp_ms": 1234,
            }),
        );
        let changed = browser_graph_changed_event(&e).expect("memorize relays to browser");
        assert_eq!(changed.event_type, "graph_changed");
        assert_eq!(changed.data["event"], "memorize");
        assert_eq!(changed.data["agent_id"], "agent-b");
        assert_eq!(changed.data["timestamp_ms"], 1234);
    }

    // ---- Network-exposure bind gate (SECURITY #1) --------------------------

    #[test]
    fn loopback_bind_needs_no_flag_and_no_warning() {
        // 127.0.0.1 and ::1 are loopback → allowed with no flag, no warning.
        assert_eq!(remote_bind_verdict("127.0.0.1", false), Ok(None));
        assert_eq!(remote_bind_verdict("::1", false), Ok(None));
        // Whitespace is tolerated.
        assert_eq!(remote_bind_verdict("  127.0.0.1  ", false), Ok(None));
    }

    #[test]
    fn wildcard_bind_is_refused_without_the_flag() {
        // The regression this closes: `0.0.0.0` used to bind with only a stderr
        // warning. It must now be a hard refusal (Err) unless opted in.
        let verdict = remote_bind_verdict("0.0.0.0", false);
        let msg = verdict.expect_err("0.0.0.0 without --allow-remote must refuse");
        assert!(msg.contains("REFUSING"), "got: {msg}");
        assert!(
            msg.contains("--allow-remote"),
            "must name the opt-in: {msg}"
        );
        // `::` (IPv6 wildcard) is refused the same way.
        assert!(remote_bind_verdict("::", false).is_err());
    }

    #[test]
    fn concrete_lan_ip_is_refused_without_the_flag() {
        // Stricter than a literal `== "0.0.0.0"`: any non-loopback address, incl.
        // a concrete LAN IP, is gated.
        assert!(remote_bind_verdict("192.168.1.10", false).is_err());
        assert!(remote_bind_verdict("10.0.0.5", false).is_err());
        // A non-IP hostname is fail-closed (treated as potentially remote).
        assert!(remote_bind_verdict("example.local", false).is_err());
    }

    #[test]
    fn remote_bind_is_allowed_with_the_flag_but_warns() {
        // With the explicit opt-in the bind proceeds, returning the strong
        // unauthenticated-exposure warning to print.
        let verdict = remote_bind_verdict("0.0.0.0", true);
        let warning = verdict
            .expect("--allow-remote must not refuse")
            .expect("a remote bind must carry a warning");
        assert!(warning.contains("WARNING"), "got: {warning}");
        assert!(
            warning.contains("--allow-remote"),
            "warning should note the opt-in: {warning}"
        );
        // A concrete LAN IP with the flag is likewise allowed + warned.
        assert!(remote_bind_verdict("192.168.1.10", true).unwrap().is_some());
    }

    #[test]
    fn loopback_bind_ignores_the_flag() {
        // The flag is a no-op for loopback — still no warning.
        assert_eq!(remote_bind_verdict("127.0.0.1", true), Ok(None));
    }

    #[test]
    fn browser_does_not_relay_read_or_ui_events_as_graph_changed() {
        // Reads, activations, and persists are NOT graph mutations — the browser
        // still receives them raw on their own event class, but they must NEVER
        // masquerade as `graph_changed` (that would trigger needless refetches).
        for e in [
            sse(
                "tool_result",
                serde_json::json!({"tool": "seek", "success": true}),
            ),
            sse(
                "activation",
                serde_json::json!({"agent_id": "a", "query": "x"}),
            ),
            sse("persist", serde_json::json!({"generation": 3})),
            sse(
                "tool_result",
                serde_json::json!({"tool": "ingest", "success": false}),
            ),
        ] {
            assert!(
                browser_graph_changed_event(&e).is_none(),
                "{} must not relay as graph_changed",
                e.event_type
            );
        }
    }
}
