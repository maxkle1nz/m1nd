// === m1nd-mcp HTTP server (axum) ===
//
// Embedded web UI server. Feature-gated behind "serve".
// Provides REST API for all 52 MCP tools + graph visualization endpoints.
// Uses the same dispatch_tool() free function as the stdio JSON-RPC transport.

#![allow(clippy::duplicated_attributes)]
#![cfg(feature = "serve")]

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderMap, StatusCode, Uri},
    response::{sse, IntoResponse, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::StreamExt;
use rust_embed::Embed;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};

use crate::brain_runtime::BrainSessionCell;
use crate::http_types::SubgraphQuery;
use crate::instance_registry::{
    delete_instance_state, list_instances, spawn_heartbeat, InstanceRegistryEntry,
};
use crate::protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
use crate::server::{
    all_tool_schemas, dispatch_generic_tool, dispatch_tool, enforce_generic_action_policy,
    handle_mcp_method_transactional, mcp_tool_error_response, read_request_payload, tool_schemas,
    write_response, McpConfig, TransportMode,
};
use crate::session::{ApplyBatchProgressSink, ScanProgressSink, SessionState};
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

/// The `skeleton_candidate` scan-phase sink (docs/uml/scan-loading.md slice 2) —
/// the exact `apply_batch_progress_sink` shape, one event type over. The phase
/// event's own fields (`phase`, the counts) flatten under `data`, joined by the
/// same `{tool, source, agent_id, timestamp_ms}` envelope every SSE event carries.
/// `event_tx.send` failing (no live `/api/events` subscriber) is IGNORED — the
/// emit is fail-open, so narration can never break the scan.
fn scan_progress_sink(
    event_tx: broadcast::Sender<SseEvent>,
    event_log_path: Option<std::path::PathBuf>,
    source: String,
    agent_id: String,
) -> ScanProgressSink {
    Arc::new(move |event| {
        let mut data = serde_json::to_value(event).unwrap_or_else(|_| serde_json::json!({}));
        if let Some(obj) = data.as_object_mut() {
            obj.insert("tool".into(), serde_json::json!("skeleton_candidate"));
            obj.insert("source".into(), serde_json::json!(source));
            obj.insert("agent_id".into(), serde_json::json!(agent_id));
            obj.insert("timestamp_ms".into(), serde_json::json!(now_ms()));
        }
        let sse_event = SseEvent {
            event_type: "scan_progress".to_string(),
            data,
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

#[cfg(m1nd_packaged_ui)]
#[derive(Embed)]
#[folder = "ui-dist/"]
#[prefix = ""]
struct UiAssets;

#[cfg(not(m1nd_packaged_ui))]
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
    pub session: Arc<BrainSessionCell>,
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
    /// F2.5c (§5a): the in-memory runnerd LIVENESS registry — `runner_id → (port,
    /// last_seen)`, fed by `POST /api/runnerd/announce`, read by `/api/runnerd/status`
    /// and the `mission_spawn` proxy. Liveness only; it grants no capability.
    pub runnerd: Arc<crate::runnerd_owner::RunnerdRegistry>,
    /// Exact UI source shared by the HTTP fallback and organism manifest.
    pub ui_authority: Arc<crate::ui_attestation::UiBundleAttestor>,
    /// G3 real-wire facade. `None` is the production fail-closed posture until
    /// a canonical MissionService config and sovereign G2 provider are installed.
    pub mission_service:
        Option<Arc<crate::mission_service_transport::MissionServiceTransportFacade>>,
    /// Closed typed consumer for elevated non-mission mutations.  Absence is a
    /// fail-closed NOT_INSTALLED posture; lease headers never authorize the
    /// generic dispatcher.
    pub external_mutation_service:
        Option<Arc<crate::external_mutation_service::ExternalMutationServiceV1>>,
    /// Distinct G2 issuance ingress. It mints one-shot leases; it never executes
    /// the target MissionService operation itself.
    pub authority_service: Option<Arc<crate::authority_transport::OwnerAuthorityServiceV1>>,
    /// Same protected G9 owner installed into AuthorityRuntime.  This handle is
    /// read-only on the manifest path; absence means autonomous authority is
    /// explicitly unavailable rather than inferred from compiled support.
    pub autonomy_owner: Option<Arc<dyn crate::autonomy_manifest::AutonomyAdmissionOwner>>,
}

// ---------------------------------------------------------------------------
// Tool execution slow-operation threshold
// ---------------------------------------------------------------------------

const TOOL_SLOW_SECS: u64 = 120;

fn boot_session_fence_error(
    error: crate::brain_runtime::BrainRuntimeError,
) -> crate::owner_security_config::OwnerAuthorityAssemblyError {
    crate::owner_security_config::OwnerAuthorityAssemblyError::ExternalMutation(
        crate::external_mutation_service::ExternalMutationError::refused(
            "bound_brain_actor_already_active_during_http_boot",
            error.to_string(),
        ),
    )
}

fn boot_http_lifecycle_error(
    code: &'static str,
    detail: impl Into<String>,
) -> crate::owner_security_config::OwnerAuthorityAssemblyError {
    crate::owner_security_config::OwnerAuthorityAssemblyError::ExternalMutation(
        crate::external_mutation_service::ExternalMutationError::refused(code, detail),
    )
}

/// Cooperative lifecycle handle for the stdio-owned HTTP sidecar.
///
/// `abort` is intentionally *not* Tokio's forced task abort: it only closes the
/// graceful-shutdown channel. Dropping the handle does the same, so neither API
/// can skip endpoint withdrawal or heartbeat join while the Tokio runtime lives.
#[must_use = "dropping this handle requests graceful sidecar shutdown"]
pub struct BackgroundHttpHandle {
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<
        tokio::task::JoinHandle<
            Result<
                Vec<crate::checkpoint_store::CheckpointAckV1>,
                crate::owner_security_config::OwnerAuthorityAssemblyError,
            >,
        >,
    >,
    local_addr: std::net::SocketAddr,
}

impl BackgroundHttpHandle {
    fn new(
        shutdown: tokio::sync::oneshot::Sender<()>,
        task: tokio::task::JoinHandle<
            Result<
                Vec<crate::checkpoint_store::CheckpointAckV1>,
                crate::owner_security_config::OwnerAuthorityAssemblyError,
            >,
        >,
        local_addr: std::net::SocketAddr,
    ) -> Self {
        Self {
            shutdown: Some(shutdown),
            task: Some(task),
            local_addr,
        }
    }

    /// The bound address is a readiness receipt: construction returns only
    /// after bind, security setup, and durable endpoint publication succeed.
    pub fn local_addr(&self) -> std::net::SocketAddr {
        self.local_addr
    }

    fn request_shutdown(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }

    /// Compatibility spelling for callers that previously aborted a raw
    /// `JoinHandle`. This now requests graceful shutdown and never cancels the
    /// cleanup future.
    pub fn abort(&mut self) {
        self.request_shutdown();
    }

    /// Request graceful shutdown and wait until endpoint withdrawal and
    /// heartbeat termination have both completed. The returned ACKs prove every
    /// sidecar-owned actor checkpointed and stopped; the shared bound owner lease
    /// remains with the caller.
    pub async fn shutdown(
        mut self,
    ) -> Result<
        Vec<crate::checkpoint_store::CheckpointAckV1>,
        crate::owner_security_config::OwnerAuthorityAssemblyError,
    > {
        self.request_shutdown();
        match self.task.take() {
            Some(task) => task.await.map_err(|error| {
                boot_http_lifecycle_error("owner_background_task_join_failed", error.to_string())
            })?,
            None => Ok(Vec::new()),
        }
    }

    /// Wait for a naturally terminating sidecar without initiating shutdown.
    pub async fn join(
        mut self,
    ) -> Result<
        Vec<crate::checkpoint_store::CheckpointAckV1>,
        crate::owner_security_config::OwnerAuthorityAssemblyError,
    > {
        match self.task.take() {
            Some(task) => task.await.map_err(|error| {
                boot_http_lifecycle_error("owner_background_task_join_failed", error.to_string())
            })?,
            None => Ok(Vec::new()),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.task
            .as_ref()
            .is_none_or(tokio::task::JoinHandle::is_finished)
    }
}

impl Drop for BackgroundHttpHandle {
    fn drop(&mut self) {
        self.request_shutdown();
    }
}

fn background_endpoint_failure(
    code: &'static str,
    error: m1nd_core::error::M1ndError,
    cleanup: Result<(), m1nd_core::error::M1ndError>,
) -> crate::runtime_jobs::RuntimeJobFailure {
    let detail = match cleanup {
        Ok(()) => error.to_string(),
        Err(cleanup_error) => format!(
            "{error}; endpoint withdrawal after publication failure also failed: {cleanup_error}"
        ),
    };
    crate::runtime_jobs::RuntimeJobFailure::new(code, detail)
}

fn boot_endpoint_publication_error(
    code: &'static str,
    error: m1nd_core::error::M1ndError,
    cleanup: Result<(), m1nd_core::error::M1ndError>,
) -> crate::owner_security_config::OwnerAuthorityAssemblyError {
    let detail = match cleanup {
        Ok(()) => error.to_string(),
        Err(cleanup_error) => format!(
            "{error}; endpoint withdrawal after publication failure also failed: {cleanup_error}"
        ),
    };
    boot_http_lifecycle_error(code, detail)
}

fn publish_background_endpoint(
    app_state: &Arc<AppState>,
    bind: String,
    port: u16,
    owner_is_medulla: bool,
) -> m1nd_core::error::M1ndResult<crate::instance_registry::InstanceHeartbeatPermit> {
    app_state.project_brains.execute_target_runtime(
        app_state.session.clone(),
        None,
        true,
        false,
        move |session| {
            if let Err(error) = session.instance.set_running_endpoint(bind, port) {
                let cleanup = session.instance.clear_running_endpoint();
                return Err(background_endpoint_failure(
                    "owner_endpoint_publication_failed",
                    error,
                    cleanup,
                ));
            }
            if owner_is_medulla {
                if let Err(error) = session.instance.set_brain_kind("medulla") {
                    let cleanup = session.instance.clear_running_endpoint();
                    return Err(background_endpoint_failure(
                        "owner_brain_kind_publication_failed",
                        error,
                        cleanup,
                    ));
                }
            }
            Ok(session.instance.heartbeat_permit())
        },
    )
}

fn withdraw_background_endpoint(app_state: &Arc<AppState>) -> m1nd_core::error::M1ndResult<()> {
    app_state.project_brains.execute_target_m1nd(
        app_state.session.clone(),
        None,
        true,
        false,
        move |session| {
            session.instance.clear_running_endpoint()?;
            Ok(())
        },
    )
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Spawn the HTTP server in background, sharing an existing SessionState.
/// Used by stdio mode to also serve the GUI without blocking the stdio loop.
/// Returns only after bind, boundary security, and endpoint publication have
/// succeeded. The cooperative handle's drop/abort path still requests drain;
/// callers that require cleanup proof must await [`BackgroundHttpHandle::shutdown`].
pub async fn spawn_background(
    session: Arc<BrainSessionCell>,
    port: u16,
) -> Result<BackgroundHttpHandle, crate::owner_security_config::OwnerAuthorityAssemblyError> {
    spawn_background_with_owner_authority(
        session,
        port,
        None,
        crate::owner_security_config::OwnerAuthorityBootRequirementV1::OptionalNotInstalled,
    )
    .await
}

/// Production-injectable background boot seam. The G2 issuance service and G3
/// consumer are installed atomically while AppState is still uniquely owned;
/// a required-but-missing assembly returns before endpoint publication,
/// heartbeat creation, router construction, or socket bind.
pub async fn spawn_background_with_owner_authority(
    session: Arc<BrainSessionCell>,
    port: u16,
    authority_assembly: Option<&crate::owner_security_config::OwnerAuthorityAssemblyV1>,
    authority_requirement: crate::owner_security_config::OwnerAuthorityBootRequirementV1,
) -> Result<BackgroundHttpHandle, crate::owner_security_config::OwnerAuthorityAssemblyError> {
    // Build tool schemas cache
    let schemas_full = tool_schemas();
    let tool_schemas_cache = schemas_full
        .get("tools")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));

    // SSE broadcast channel
    let (event_tx, _) = broadcast::channel::<SseEvent>(64);
    let (registry_root, runtime_root, owner_is_medulla) = {
        let guard = session.read().map_err(boot_session_fence_error)?;
        (
            guard.instance.registry_root(),
            guard.runtime_root.clone(),
            guard.is_medulla_store(),
        )
    };

    // AppState
    let runnerd = Arc::new(crate::runnerd_owner::RunnerdRegistry::default());
    // F11-b: thread the naming facts (the announce registry + the OWNER runtime
    // root, where runnerd.secret lives) into the bound session and every project
    // brain this owner boots, so a `skeleton_candidate` scan can reach the live
    // naming-runner from ANY hosted brain.
    let naming_handle = crate::runnerd_owner::NamingRunnerHandle {
        registry: runnerd.clone(),
        owner_runtime_root: runtime_root.clone(),
    };
    session
        .lock_mut_before_actor()
        .map_err(boot_session_fence_error)?
        .runnerd_naming = Some(naming_handle.clone());
    let project_brains = Arc::new(
        crate::project_brains::ProjectBrainRegistry::new(
            runtime_root.join(crate::project_brains::PROJECT_BRAINS_DIR),
            Some(registry_root.clone()),
        )
        .with_runnerd_naming(naming_handle),
    );
    let mut app_state = AppState {
        session,
        tool_schemas_cache,
        event_tx,
        event_log_path: None,
        registry_dir: Some(registry_root),
        mcp_sessions: crate::mcp_http::new_mcp_session_registry(),
        project_brains,
        runnerd,
        ui_authority: Arc::new(crate::ui_attestation::UiBundleAttestor::default()),
        mission_service: None,
        external_mutation_service: None,
        authority_service: None,
        autonomy_owner: None,
    };
    crate::owner_security_config::install_owner_authority_for_http_boot_v1(
        &mut app_state,
        authority_assembly,
        authority_requirement,
    )?;

    let app_state = Arc::new(app_state);
    let addr: std::net::SocketAddr = format!("127.0.0.1:{}", port)
        .parse()
        .expect("valid socket addr");
    // Readiness is a construction invariant: a returned handle proves bind,
    // listener identity, boundary security, and durable endpoint publication.
    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|error| {
        boot_http_lifecycle_error("owner_background_bind_failed", error.to_string())
    })?;
    let effective_addr = listener.local_addr().map_err(|error| {
        boot_http_lifecycle_error(
            "owner_background_listener_identity_failed",
            error.to_string(),
        )
    })?;
    let http_security = crate::http_security::LocalHttpSecurity::load_or_create(
        &runtime_root,
        effective_addr.port(),
    )
    .map(Arc::new)
    .map_err(|error| {
        boot_http_lifecycle_error("owner_background_security_failed", error.to_string())
    })?;
    let browser_bootstrap_url = http_security.browser_bootstrap_url();
    let router = crate::http_security::secure_local_router(
        build_router(app_state.clone(), false),
        http_security,
    );
    let heartbeat_permit = publish_background_endpoint(
        &app_state,
        effective_addr.ip().to_string(),
        effective_addr.port(),
        owner_is_medulla,
    )
    .map_err(|error| {
        boot_http_lifecycle_error(
            "owner_background_endpoint_publication_failed",
            error.to_string(),
        )
    })?;
    let heartbeat = spawn_heartbeat(heartbeat_permit);

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        eprintln!("[m1nd-mcp] m1nd GUI: http://{effective_addr}");
        let browser_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(400)).await;
            let _ = open_browser(&browser_bootstrap_url);
        });

        let serve_result = axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                let _ = (&mut shutdown_rx).await;
            })
            .await;

        browser_task.abort();
        let _ = browser_task.await;

        // A drained sidecar remains a live owner until its endpoint has been
        // durably withdrawn. Keep its weak heartbeat active while retrying so a
        // transient checkpoint/actor fence cannot create either a stale-dead
        // owner or a falsely completed shutdown handle.
        let mut cleanup_backoff = Duration::from_millis(50);
        loop {
            let cleanup_state = app_state.clone();
            let cleanup =
                tokio::task::spawn_blocking(move || withdraw_background_endpoint(&cleanup_state))
                    .await;
            match cleanup {
                Ok(Ok(())) => break,
                Ok(Err(error)) => eprintln!(
                    "[m1nd-mcp] Background endpoint withdrawal failed after drain; heartbeat retained and cleanup will retry: {error}"
                ),
                Err(error) => eprintln!(
                    "[m1nd-mcp] Background endpoint withdrawal task failed after drain; heartbeat retained and cleanup will retry: {error}"
                ),
            }
            tokio::time::sleep(cleanup_backoff).await;
            cleanup_backoff = cleanup_backoff
                .saturating_mul(2)
                .min(Duration::from_secs(2));
        }

        // Endpoint withdrawal is not actor shutdown. Stop every actor owned by
        // this sidecar and require the final checkpoint ACK before declaring the
        // background lifecycle complete. The shared bound lease remains with the
        // caller and is intentionally not released here.
        let shutdown_registry = Arc::clone(&app_state.project_brains);
        let checkpoint_acks = match tokio::task::spawn_blocking(move || {
            shutdown_registry.shutdown(Duration::from_secs(5))
        })
        .await
        {
            Ok(Ok(acks)) => acks,
            Ok(Err(error)) => {
                drop(heartbeat);
                return Err(boot_http_lifecycle_error(
                    "owner_background_final_checkpoint_not_acked",
                    error.to_string(),
                ));
            }
            Err(error) => {
                drop(heartbeat);
                return Err(boot_http_lifecycle_error(
                    "owner_background_shutdown_task_failed",
                    error.to_string(),
                ));
            }
        };
        heartbeat.abort();
        let _ = heartbeat.await;

        if let Err(error) = serve_result {
            return Err(boot_http_lifecycle_error(
                "owner_background_serve_failed",
                error.to_string(),
            ));
        }
        Ok(checkpoint_acks)
    });
    Ok(BackgroundHttpHandle::new(shutdown_tx, task, effective_addr))
}

/// Pure network-exposure decision (no I/O, no exit) so it is unit-testable in
/// both directions. Returns `Err(one_line_error)` when the process MUST refuse to
/// start and `Ok(())` only when the bind is loopback.
///
/// The rule: a bind that does NOT resolve to a loopback address exposes graph
/// mutation to the network. Because authenticated TLS remote transport is not
/// implemented, every such bind is refused even when the legacy
/// `--allow-remote` flag is present. This is stricter than a literal
/// `== "0.0.0.0"` check: `0.0.0.0`, `::`, concrete LAN IPs, and hostnames that do
/// not parse as loopback all fail closed.
fn remote_bind_verdict(bind: &str, _allow_remote: bool) -> Result<(), String> {
    use std::net::IpAddr;

    let is_loopback = bind
        .trim()
        .parse::<IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false);

    if is_loopback {
        return Ok(());
    }

    Err(format!(
        "[m1nd-mcp] REFUSING to bind to non-loopback address {bind}: authenticated remote transport with \
         TLS, authentication, and scoped authorization is not implemented. --allow-remote cannot override \
         this fail-closed gate; bind to a loopback address (the default 127.0.0.1)."
    ))
}

/// Start the HTTP server (and optionally stdio).
#[allow(clippy::too_many_arguments)]
pub async fn run(
    config: McpConfig,
    port: u16,
    bind: String,
    allow_remote: bool,
    dev_mode: bool,
    ui_dir: Option<String>,
    auto_open: bool,
    also_stdio: bool,
    event_log: Option<String>,
    watch_events: Option<String>,
) {
    if let Err(error) = run_with_owner_authority(
        config,
        port,
        bind,
        allow_remote,
        dev_mode,
        ui_dir,
        auto_open,
        also_stdio,
        event_log,
        watch_events,
        None,
        crate::owner_security_config::OwnerAuthorityBootRequirementV1::OptionalNotInstalled,
    )
    .await
    {
        eprintln!("[m1nd-mcp] HTTP owner lifecycle refused: {error}");
        std::process::exit(2);
    }
}

/// Production-injectable foreground boot seam. This is the only foreground
/// path that can advertise G2/G3: it accepts a preflighted assembly by
/// ownership and installs both services before AppState becomes shared and
/// before endpoint publication, heartbeat creation, router construction, or
/// socket bind. `Required` with no valid assembly returns an error at that
/// boundary.
#[allow(clippy::too_many_arguments)]
pub async fn run_with_owner_authority(
    config: McpConfig,
    port: u16,
    bind: String,
    allow_remote: bool,
    dev_mode: bool,
    ui_dir: Option<String>,
    auto_open: bool,
    also_stdio: bool,
    event_log: Option<String>,
    watch_events: Option<String>,
    authority_assembly: Option<crate::owner_security_config::OwnerAuthorityAssemblyV1>,
    authority_requirement: crate::owner_security_config::OwnerAuthorityBootRequirementV1,
) -> Result<(), crate::owner_security_config::OwnerAuthorityAssemblyError> {
    // Network-exposure gate: every non-loopback bind is refused because there is
    // no authenticated TLS remote transport yet. The legacy --allow-remote flag
    // cannot bypass this gate. A refusal exits before any graph load, engine build,
    // or lease is taken. This injectable seam returns a typed refusal; only the
    // CLI wrapper decides the process exit code.
    remote_bind_verdict(&bind, allow_remote)
        .map_err(|error| boot_http_lifecycle_error("owner_remote_bind_refused", error))?;

    // 1. Create McpServer to load graph + build engines
    let server = crate::server::McpServer::new(config.clone()).map_err(|error| {
        boot_http_lifecycle_error("owner_http_session_boot_failed", error.to_string())
    })?;

    // 2. Extract SessionState into the actor-compatible shared cell.
    let session_state = server.into_session_state();
    let owner_runtime_root = session_state.runtime_root.clone();
    let owner_is_medulla = session_state.is_medulla_store();
    let session = Arc::new(BrainSessionCell::new(session_state));

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
    let runnerd = Arc::new(crate::runnerd_owner::RunnerdRegistry::default());
    // F11-b: thread the naming facts into the bound session + every project brain
    // (see the background-server construction above for the law).
    let naming_handle = crate::runnerd_owner::NamingRunnerHandle {
        registry: runnerd.clone(),
        owner_runtime_root: owner_runtime_root.clone(),
    };
    session
        .lock_mut_before_actor()
        .map_err(boot_session_fence_error)?
        .runnerd_naming = Some(naming_handle.clone());
    let project_brains = Arc::new(
        crate::project_brains::ProjectBrainRegistry::new(
            owner_runtime_root.join(crate::project_brains::PROJECT_BRAINS_DIR),
            config.registry_dir.clone(),
        )
        .with_runnerd_naming(naming_handle),
    );
    let ui_authority = Arc::new(crate::ui_attestation::UiBundleAttestor::for_http(
        dev_mode,
        ui_dir.map(std::path::PathBuf::from),
    ));
    let filesystem_ui = ui_authority.serve_dir().is_some();
    let mut app_state = AppState {
        session: session.clone(),
        tool_schemas_cache,
        event_tx: event_tx.clone(),
        event_log_path: event_log_path.clone(),
        registry_dir: config.registry_dir.clone(),
        mcp_sessions: crate::mcp_http::new_mcp_session_registry(),
        project_brains,
        runnerd,
        ui_authority,
        mission_service: None,
        external_mutation_service: None,
        authority_service: None,
        autonomy_owner: None,
    };
    crate::owner_security_config::install_owner_authority_for_http_boot_v1(
        &mut app_state,
        authority_assembly.as_ref(),
        authority_requirement,
    )?;

    // Authority is fully installed before even attempting the socket bind. The
    // listener must then exist before discovery can claim a live endpoint.
    let requested_addr: std::net::SocketAddr =
        format!("{}:{}", bind, port).parse().map_err(|error| {
            boot_http_lifecycle_error(
                "owner_http_bind_address_invalid",
                format!("invalid bind address {bind}:{port}: {error}"),
            )
        })?;
    let listener = tokio::net::TcpListener::bind(requested_addr)
        .await
        .map_err(|error| {
            boot_http_lifecycle_error(
                "owner_http_bind_failed",
                format!("failed to bind {requested_addr}: {error}"),
            )
        })?;
    let effective_addr = listener.local_addr().map_err(|error| {
        boot_http_lifecycle_error(
            "owner_http_listener_identity_failed",
            format!("failed to read the bound listener address: {error}"),
        )
    })?;

    // Refuse every HTTP security failure before the process advertises the now
    // bound endpoint or starts refreshing liveness. The effective port matters
    // when callers intentionally request port 0.
    let http_security = match crate::http_security::LocalHttpSecurity::load_or_create(
        &owner_runtime_root,
        effective_addr.port(),
    ) {
        Ok(security) => Arc::new(security),
        Err(error) => {
            return Err(boot_http_lifecycle_error(
                "owner_http_security_boot_failed",
                error.to_string(),
            ));
        }
    };
    let browser_bootstrap_url = http_security.browser_bootstrap_url();
    eprintln!(
        "[m1nd-mcp] HTTP bearer token file: {}",
        http_security.token_path().display()
    );

    let heartbeat_permit = {
        let mut owner = app_state
            .session
            .lock_mut_before_actor()
            .map_err(boot_session_fence_error)?;
        owner
            .instance
            .set_running_endpoint(effective_addr.ip().to_string(), effective_addr.port())
            .map_err(|error| {
                let cleanup = owner.instance.clear_running_endpoint();
                boot_endpoint_publication_error("owner_endpoint_publication_failed", error, cleanup)
            })?;
        // The served owner IS the medulla — stamp its on-disk registry entry so a
        // sibling owner listing it reads the honest kind (the self-listing path
        // stamps it too, but only THIS process can label its own entry on disk).
        if owner_is_medulla {
            owner.instance.set_brain_kind("medulla").map_err(|error| {
                let cleanup = owner.instance.clear_running_endpoint();
                boot_endpoint_publication_error(
                    "owner_brain_kind_publication_failed",
                    error,
                    cleanup,
                )
            })?;
        }
        owner.instance.heartbeat_permit()
    };
    let heartbeat = spawn_heartbeat(heartbeat_permit);
    let app_state = Arc::new(app_state);

    // 6b. If --watch-events is specified, spawn the event log watcher
    if let Some(ref watch_path) = watch_events {
        let path = std::path::PathBuf::from(watch_path);
        let tx = event_tx.clone();
        tokio::spawn(watch_event_log(path, tx));
    }

    // 7. Build router
    let shutdown_project_brains = app_state.project_brains.clone();
    let router = crate::http_security::secure_local_router(
        build_router(app_state, filesystem_ui),
        http_security,
    );

    // 8. Optionally run stdio JSON-RPC alongside HTTP. The raw stdin reader owns
    // no brain/session authority; it only feeds a bounded line queue. The
    // dispatcher owns every authoritative Arc and is cooperatively stopped and
    // joined after Axum drains, so a blocked stdin read cannot retain the lease
    // or overlap the final checkpoint.
    let stdio_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stdio_task = if also_stdio {
        let (request_tx, request_rx) = std::sync::mpsc::sync_channel::<(String, TransportMode)>(32);
        std::thread::Builder::new()
            .name("m1nd-http-stdio-reader".to_string())
            .spawn(move || {
                let stdin = std::io::stdin();
                let mut reader = stdin.lock();
                while let Ok(Some(request)) = read_request_payload(&mut reader) {
                    if request_tx.send(request).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| {
                boot_http_lifecycle_error(
                    "owner_combined_stdio_reader_start_failed",
                    error.to_string(),
                )
            })?;

        // Stdout can block forever under pipe backpressure. Keep it on a
        // no-authority worker fed by a bounded non-blocking queue; a full/broken
        // output path closes dispatcher admission instead of retaining the
        // brain lease in the writer thread.
        let (response_tx, response_rx) =
            std::sync::mpsc::sync_channel::<(JsonRpcResponse, TransportMode)>(32);
        let writer_stop = Arc::clone(&stdio_stop);
        std::thread::Builder::new()
            .name("m1nd-http-stdio-writer".to_string())
            .spawn(move || {
                let stdout = std::io::stdout();
                let mut writer = stdout.lock();
                while let Ok((response, mode)) = response_rx.recv() {
                    if write_response(&mut writer, &response, mode).is_err() {
                        writer_stop.store(true, std::sync::atomic::Ordering::Release);
                        break;
                    }
                }
            })
            .map_err(|error| {
                boot_http_lifecycle_error(
                    "owner_combined_stdio_writer_start_failed",
                    error.to_string(),
                )
            })?;

        let stdio_session = session.clone();
        let stdio_dispatch_registry = shutdown_project_brains.clone();
        let stdio_event_tx = event_tx.clone();
        let stdio_event_log = event_log_path.clone();
        let dispatcher_stop = Arc::clone(&stdio_stop);
        Some(tokio::task::spawn_blocking(move || {
            eprintln!("[m1nd-mcp] Stdio JSON-RPC also active (--stdio). SSE cross-process bridge enabled.");
            loop {
                if dispatcher_stop.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                let (payload, transport_mode) =
                    match request_rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(request) => request,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                if dispatcher_stop.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                let trimmed = payload.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let raw_request = match serde_json::from_str::<serde_json::Value>(trimmed) {
                    Ok(raw) => raw,
                    Err(error) => {
                        let response = JsonRpcResponse {
                            jsonrpc: "2.0".into(),
                            id: serde_json::Value::Null,
                            result: None,
                            error: Some(JsonRpcError {
                                code: -32700,
                                message: format!("Parse error: {error}"),
                                data: None,
                            }),
                        };
                        if response_tx.try_send((response, transport_mode)).is_err() {
                            dispatcher_stop.store(true, std::sync::atomic::Ordering::Release);
                            break;
                        }
                        continue;
                    }
                };

                // MCP notifications intentionally produce no response.
                if raw_request.get("id").is_none() {
                    continue;
                }
                let request = match serde_json::from_value::<JsonRpcRequest>(raw_request) {
                    Ok(request) => request,
                    Err(error) => {
                        let response = JsonRpcResponse {
                            jsonrpc: "2.0".into(),
                            id: serde_json::Value::Null,
                            result: None,
                            error: Some(JsonRpcError {
                                code: -32700,
                                message: format!("Parse error: {error}"),
                                data: None,
                            }),
                        };
                        if response_tx.try_send((response, transport_mode)).is_err() {
                            dispatcher_stop.store(true, std::sync::atomic::Ordering::Release);
                            break;
                        }
                        continue;
                    }
                };

                let is_tool_call = request.method == "tools/call";
                let tool_name = request
                    .params
                    .get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or("")
                    .to_string();
                let arguments = request
                    .params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));
                let mutating =
                    is_tool_call && crate::server::read_only_denied(&tool_name, &arguments);
                let request_id = request.id.clone();
                let actor_request = request.clone();
                let dispatch_arguments = arguments.clone();
                let dispatch_tool_name = tool_name.clone();
                let progress_tx = stdio_event_tx.clone();
                let progress_log = stdio_event_log.clone();

                // Decide before acquiring/routing a runtime and before installing
                // progress sinks or tracking presence. The transactional handler
                // repeats this pure check inside the actor as defense in depth.
                let dispatch = if is_tool_call {
                    enforce_generic_action_policy(&tool_name, &arguments)
                } else {
                    Ok(())
                }
                .and_then(|()| {
                    stdio_dispatch_registry.execute_target_m1nd(
                        stdio_session.clone(),
                        None,
                        true,
                        mutating,
                        move |session| {
                            if dispatch_tool_name == "apply_batch" {
                                session.apply_batch_progress_sink =
                                    Some(apply_batch_progress_sink(
                                        progress_tx.clone(),
                                        progress_log.clone(),
                                        "stdio".to_string(),
                                        dispatch_arguments
                                            .get("agent_id")
                                            .and_then(|value| value.as_str())
                                            .unwrap_or("unknown")
                                            .to_string(),
                                    ));
                            }
                            if dispatch_tool_name == "skeleton_candidate" {
                                session.scan_progress_sink = Some(scan_progress_sink(
                                    progress_tx,
                                    progress_log,
                                    "stdio".to_string(),
                                    dispatch_arguments
                                        .get("agent_id")
                                        .and_then(|value| value.as_str())
                                        .unwrap_or("unknown")
                                        .to_string(),
                                ));
                            }
                            let result =
                                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                    handle_mcp_method_transactional(session, &actor_request)
                                }));
                            session.apply_batch_progress_sink = None;
                            session.scan_progress_sink = None;
                            match result {
                                Ok(result) => result,
                                Err(payload) => std::panic::resume_unwind(payload),
                            }
                        },
                    )
                });

                let response = match dispatch {
                    Ok(response) => response,
                    Err(error) if is_tool_call => {
                        mcp_tool_error_response(request_id, error.to_string())
                    }
                    Err(error) => JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        id: request_id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32603,
                            message: error.to_string(),
                            data: None,
                        }),
                    },
                };

                if is_tool_call {
                    let success = response
                        .result
                        .as_ref()
                        .and_then(|result| result.get("isError"))
                        .and_then(|value| value.as_bool())
                        != Some(true);
                    let response_preview = serde_json::to_value(&response)
                        .map(|value| truncate_json(&value, 500))
                        .unwrap_or(serde_json::Value::Null);
                    let sse_event = SseEvent {
                        event_type: "tool_result".to_string(),
                        data: serde_json::json!({
                            "tool": tool_name,
                            "source": "stdio",
                            "agent_id": arguments.get("agent_id").and_then(|value| value.as_str()).unwrap_or("unknown"),
                            "success": success,
                            "result_preview": response_preview,
                            "timestamp_ms": now_ms(),
                        }),
                    };
                    let _ = stdio_event_tx.send(sse_event.clone());
                    if let Some(ref log_path) = stdio_event_log {
                        append_event_to_log(log_path, &sse_event);
                    }
                }

                if response_tx.try_send((response, transport_mode)).is_err() {
                    dispatcher_stop.store(true, std::sync::atomic::Ordering::Release);
                    break;
                }
            }
        }))
    } else {
        None
    };

    eprintln!(
        "[m1nd-mcp] HTTP server listening on http://{}",
        effective_addr
    );

    // 9. Auto-open browser
    if auto_open {
        let url = browser_bootstrap_url;
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let _ = open_browser(&url);
        });
    }

    // 10. SIGINT only tells Axum to stop accepting and drain existing
    // connections. Final checkpoint/actor stop and lifecycle release happen
    // strictly after `serve` returns, so no live handler can overlap a successor.
    let serve_result = axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            match owner_shutdown_signal().await {
                Ok(signal) => eprintln!(
                    "[m1nd-mcp] {signal} received; draining HTTP connections before checkpoint..."
                ),
                Err(error) => eprintln!(
                    "[m1nd-mcp] shutdown signal watcher failed; draining HTTP connections fail-closed: {error}"
                ),
            }
        })
        .await;

    stdio_stop.store(true, std::sync::atomic::Ordering::Release);

    // The listener is already drained: withdraw discovery before any possibly
    // slow checkpoint/join work. Keep the weak owner heartbeat alive and retry
    // through the actor until the registry no longer advertises a dead socket.
    let mut endpoint_backoff = Duration::from_millis(50);
    loop {
        let endpoint_registry = Arc::clone(&shutdown_project_brains);
        let endpoint_session = Arc::clone(&session);
        let withdrawal = tokio::task::spawn_blocking(move || {
            endpoint_registry.execute_target_m1nd(endpoint_session, None, true, false, |owner| {
                owner.instance.clear_running_endpoint()
            })
        })
        .await;
        match withdrawal {
            Ok(Ok(())) => break,
            Ok(Err(error)) => eprintln!(
                "[m1nd-mcp] Foreground endpoint withdrawal failed after HTTP drain; heartbeat retained and cleanup will retry: {error}"
            ),
            Err(error) => eprintln!(
                "[m1nd-mcp] Foreground endpoint withdrawal task failed after HTTP drain; heartbeat retained and cleanup will retry: {error}"
            ),
        }
        tokio::time::sleep(endpoint_backoff).await;
        endpoint_backoff = endpoint_backoff
            .saturating_mul(2)
            .min(Duration::from_secs(2));
    }

    if let Some(stdio_task) = stdio_task {
        if let Err(error) = stdio_task.await {
            eprintln!(
                "[m1nd-mcp] Combined stdio dispatcher did not join after HTTP drain; owner remains fail-closed and its heartbeat is retained: {error}"
            );
            drop(heartbeat);
            return Err(boot_http_lifecycle_error(
                "owner_combined_stdio_dispatcher_join_failed",
                error.to_string(),
            ));
        }
    }

    let checkpoint_acks = match shutdown_project_brains.shutdown(Duration::from_secs(5)) {
        Ok(acks) => acks,
        Err(error) => {
            eprintln!(
                "[m1nd-mcp] Final checkpoint was not ACKed after HTTP drain; owner remains fail-closed and its heartbeat is retained: {error}"
            );
            // Dropping a Tokio JoinHandle detaches rather than cancels it. The
            // heartbeat permit is weak, so it remains live exactly as long as the
            // actor/session recovery owner and exits when that owner drops.
            drop(heartbeat);
            return Err(boot_http_lifecycle_error(
                "owner_final_checkpoint_not_acked",
                error.to_string(),
            ));
        }
    };

    // Prove the stopped actor has returned SessionState before stopping its
    // heartbeat. Keep a spare weak permit so an unexpected re-fence can retain
    // liveness without exposing the unique lifecycle handle.
    let retained_heartbeat_permit = {
        let owner = session.lock_mut_before_actor().map_err(|error| {
            boot_http_lifecycle_error(
                "owner_session_not_returned_after_checkpoint_ack",
                error.to_string(),
            )
        })?;
        owner.instance.heartbeat_permit()
    };

    heartbeat.abort();
    let _ = heartbeat.await;

    let mut release_backoff = Duration::from_millis(50);
    loop {
        let release_result = match session.lock_mut_before_actor() {
            Ok(mut owner) => owner.instance.release(),
            Err(error) => {
                let retained = spawn_heartbeat(retained_heartbeat_permit);
                // Retain liveness for the still-owned session without returning
                // a lifecycle capability to the caller.
                drop(retained);
                return Err(boot_http_lifecycle_error(
                    "owner_session_refenced_before_release",
                    error.to_string(),
                ));
            }
        };
        match release_result {
            Ok(()) => break,
            Err(error) => eprintln!(
                "[m1nd-mcp] Owner release failed after checkpoint ACK; lifetime guard retained and cleanup will retry: {error}"
            ),
        }
        tokio::time::sleep(release_backoff).await;
        release_backoff = release_backoff
            .saturating_mul(2)
            .min(Duration::from_secs(2));
    }

    eprintln!(
        "[m1nd-mcp] Shutdown sequence complete after {} checkpoint ACK(s).",
        checkpoint_acks.len()
    );
    if let Err(error) = serve_result {
        return Err(boot_http_lifecycle_error(
            "owner_http_serve_failed",
            error.to_string(),
        ));
    }
    Ok(())
}

#[cfg(unix)]
async fn owner_shutdown_signal() -> Result<&'static str, String> {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|error| format!("could not install SIGTERM watcher: {error}"))?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => {
            result.map(|()| "SIGINT").map_err(|error| error.to_string())
        }
        signal = terminate.recv() => {
            signal.map(|()| "SIGTERM").ok_or_else(|| "SIGTERM watcher closed".to_string())
        }
    }
}

#[cfg(not(unix))]
async fn owner_shutdown_signal() -> Result<&'static str, String> {
    tokio::signal::ctrl_c()
        .await
        .map(|()| "CTRL_C")
        .map_err(|error| error.to_string())
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
    if s.chars().count() <= max_chars {
        value.clone()
    } else {
        let prefix = s.chars().take(max_chars).collect::<String>();
        serde_json::Value::String(format!("{prefix}...(truncated)"))
    }
}

/// Wait for a blocking task without ever detaching it after the slow threshold.
///
/// `spawn_blocking` tasks cannot be cancelled once running. Returning a timeout
/// while dropping their join handle would let a caller observe failure and then
/// see a late mutation. The boolean reports only that the task crossed the
/// observability threshold; the result is always the task's terminal result.
async fn await_blocking_completion<T: Send + 'static>(
    mut task: tokio::task::JoinHandle<T>,
    slow_after: Duration,
) -> (bool, Result<T, tokio::task::JoinError>) {
    tokio::select! {
        result = &mut task => (false, result),
        _ = tokio::time::sleep(slow_after) => (true, task.await),
    }
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
pub fn build_router(state: Arc<AppState>, filesystem_ui: bool) -> Router {
    let api = Router::new()
        .route("/api/health", get(handle_health))
        .route("/api/manifest", get(handle_manifest))
        .route("/api/presences", get(handle_presences))
        // HUMAN-VIEW-V2 F30 — the Universe panorama's read-only aggregate. SIDECAR-
        // ONLY: manifests + presence dir + each world's box/store + the owner's own
        // alerts; it NEVER hydrates a brain (executable HARD LAW, tests/universe_endpoint.rs).
        .route("/api/universe", get(handle_universe))
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
        .route("/api/authority/authorize", post(handle_authority_authorize))
        .route(
            "/api/authority/session/challenge",
            post(handle_authority_session_challenge),
        )
        .route(
            "/api/authority/session/authenticate",
            post(handle_authority_session_authenticate),
        )
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
        // HUMAN-VIEW-V2-F2.5c §5a — the runner daemon's LIVENESS surface. `announce`
        // is a shared-secret-guarded write to the in-memory registry (liveness only,
        // never a capability grant); `status` is a pure read the UI uses to un-disable
        // the spawn radio and list the pinned-live runners.
        .route("/api/runnerd/announce", post(handle_runnerd_announce))
        .route("/api/runnerd/status", get(handle_runnerd_status))
        .route("/api/events", get(handle_sse))
        .with_state(state.clone())
        .layer(DefaultBodyLimit::max(1_048_576)); // 1MB body limit (FM-A-004)

    let router = if let Some(ui_dir) = state.ui_authority.serve_dir() {
        debug_assert!(filesystem_ui, "UI attestor/router mode mismatch");
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);
        api.fallback_service(tower_http::services::ServeDir::new(ui_dir))
            .layer(cors)
    } else {
        debug_assert!(!filesystem_ui, "UI attestor/router mode mismatch");
        api.fallback(serve_embedded_ui)
    };

    // Response compression (gzip). The big read surfaces — `/api/graph/snapshot`
    // (tens of MB of node/edge JSON) and the served JS bundle — dominate the wire
    // cost; gzip shrinks that JSON by roughly an order of magnitude. Outermost so it
    // wraps every route including the UI fallback. `CompressionLayer::new()` uses
    // tower-http's `DefaultPredicate`, which excludes `text/event-stream`, so the
    // `/api/events` SSE stream is never buffered or compressed (streaming stays live).
    // Only `compression-gzip` is enabled, so no brotli/zstd crate enters the graph.
    router.layer(CompressionLayer::new())
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum ManifestReadError {
    UnknownBrain(m1nd_core::error::M1ndError),
    SnapshotUnstable(String),
    Unavailable(String),
}

fn manifest_read_response(
    result: Result<crate::organism_manifest::ManifestResponseV1, ManifestReadError>,
) -> axum::response::Response {
    match result {
        Ok(response) => Json(response).into_response(),
        Err(ManifestReadError::UnknownBrain(error)) => {
            let mut payload = tool_error_payload(&error);
            payload["error"] = serde_json::json!("unknown_brain");
            (StatusCode::NOT_FOUND, Json(payload)).into_response()
        }
        Err(ManifestReadError::SnapshotUnstable(detail)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "manifest_snapshot_unstable",
                "detail": detail,
            })),
        )
            .into_response(),
        Err(ManifestReadError::Unavailable(detail)) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "manifest_unavailable",
                "detail": detail,
            })),
        )
            .into_response(),
    }
}

async fn handle_manifest(
    State(state): State<Arc<AppState>>,
    Query(brain): Query<BrainQuery>,
) -> axum::response::Response {
    match tokio::task::spawn_blocking(move || {
        let (target, served_brain) = resolve_brain(&state, brain.brain.as_deref())
            .map_err(ManifestReadError::UnknownBrain)?;
        let selected_project_root = served_brain
            .get("project_root")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let bound = Arc::ptr_eq(&target, &state.session);

        // Copy only cheap in-memory facts under the selected brain's owner mutex.
        // VCS reads and file hashing happen after release so manifest polling
        // cannot stall unrelated graph work behind a binary/snapshot read.
        let seed = state
            .project_brains
            .read_target_runtime_snapshot(
                target.clone(),
                selected_project_root.as_deref(),
                bound,
                |session| {
                    Ok(crate::organism_manifest::capture_parts(
                        session,
                        crate::util::now_ms(),
                    ))
                },
            )
            .map(|snapshot| crate::organism_manifest::finish_capture_seed(snapshot.value))
            .map_err(|error| ManifestReadError::Unavailable(error.to_string()))?;
        let ui = if state.ui_authority.observes_filesystem() {
            state.ui_authority.observe()
        } else {
            let identity = embedded_ui_identity().map_err(ManifestReadError::SnapshotUnstable)?;
            state.ui_authority.observe_embedded_identity(identity)
        }
        .map_err(ManifestReadError::SnapshotUnstable)?;
        let observed = crate::organism_manifest::observe_with_ui(seed.clone(), ui);

        // Post-hash OCC: generation/counts and the persisted graph bytes must all
        // come from one stable observation window. Refuse a hybrid projection if
        // the selected owner mutated or persisted its graph while it was hashed.
        let after = state
            .project_brains
            .read_target_runtime_snapshot(
                target,
                selected_project_root.as_deref(),
                bound,
                move |session| {
                    Ok(crate::organism_manifest::capture_parts(
                        session,
                        seed.observed_at,
                    ))
                },
            )
            .map(|snapshot| crate::organism_manifest::finish_capture_seed(snapshot.value))
            .map_err(|error| ManifestReadError::Unavailable(error.to_string()))?;
        crate::organism_manifest::ensure_graph_authority_basis_stable(&seed, &after)
            .map_err(ManifestReadError::SnapshotUnstable)?;

        let autonomy = state
            .autonomy_owner
            .as_ref()
            .map(|owner| owner.read_projection(seed.observed_at))
            .transpose()
            .map_err(|error| ManifestReadError::Unavailable(error.to_string()))?;
        crate::organism_manifest::compose_with_autonomy(observed, autonomy)
            .map_err(ManifestReadError::Unavailable)
    })
    .await
    {
        Ok(result) => manifest_read_response(result),
        Err(join_error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "manifest_worker_failed",
                "detail": join_error.to_string(),
            })),
        )
            .into_response(),
    }
}

async fn handle_health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    use std::collections::HashMap;
    use std::sync::{Mutex as StdMutex, OnceLock};

    static SNAPSHOTS: OnceLock<StdMutex<HashMap<usize, serde_json::Value>>> = OnceLock::new();
    let cache_key = Arc::as_ptr(&state.session) as usize;
    let cache = SNAPSHOTS.get_or_init(|| StdMutex::new(HashMap::new()));
    let project_brain_runtimes = state.project_brains.runtime_health_snapshots();

    // Once the bound actor exists, health never queues behind it and never
    // re-enters the raw SessionState cell. The actor publishes its own lock-free
    // health; graph counters remain the last actor-serialized snapshot and are
    // labeled CACHED rather than silently claimed fresh.
    if let Ok(Some(runtime_health)) = state.project_brains.bound_runtime_health() {
        let mut snapshot = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&cache_key)
            .cloned()
            .unwrap_or_else(empty_health_snapshot);
        if let Some(object) = snapshot.as_object_mut() {
            let healthy = runtime_health.status == "healthy" && runtime_health.accepting;
            object.insert(
                "status".into(),
                serde_json::json!(if healthy { "ok" } else { "degraded" }),
            );
            object.insert("owner_busy".into(), serde_json::json!(!healthy));
            object.insert(
                "snapshot_freshness".into(),
                serde_json::json!("CACHED_ACTOR_SAFE"),
            );
            object.insert(
                "health_non_claims".into(),
                serde_json::json!([
                    "cached health does not claim current graph counts",
                    "actor health proves admission and checkpoint status, not tool progress"
                ]),
            );
            object.insert(
                "bound_brain_runtime".into(),
                serde_json::json!(runtime_health),
            );
            object.insert(
                "project_brain_runtimes".into(),
                serde_json::json!(project_brain_runtimes),
            );
        }
        return (StatusCode::OK, Json(snapshot));
    }

    // First health read starts the bound actor and copies a capability-free DTO
    // through it on the blocking pool. Every later health read takes the
    // lock-free/cached branch above.
    let snapshot_state = Arc::clone(&state);
    let read = tokio::task::spawn_blocking(move || {
        http_target_read_snapshot(
            &snapshot_state,
            Arc::clone(&snapshot_state.session),
            None,
            true,
            |session| Ok(HttpHealthReadSnapshot::from_session(session)),
        )
    })
    .await;
    let health = match read {
        Ok(Ok(snapshot)) => snapshot,
        Ok(Err(_)) | Err(_) => {
            let mut snapshot = cache
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .get(&cache_key)
                .cloned()
                .unwrap_or_else(empty_health_snapshot);
            if let Some(object) = snapshot.as_object_mut() {
                object.insert("status".into(), serde_json::json!("degraded"));
                object.insert("owner_busy".into(), serde_json::json!(true));
                object.insert(
                    "snapshot_freshness".into(),
                    serde_json::json!("STALE_OR_UNKNOWN"),
                );
                object.insert(
                    "project_brain_runtimes".into(),
                    serde_json::json!(project_brain_runtimes),
                );
            }
            return (StatusCode::OK, Json(snapshot));
        }
    };

    let result = health_snapshot_json(health, project_brain_runtimes);
    cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(cache_key, result.clone());

    (StatusCode::OK, Json(result))
}

fn empty_health_snapshot() -> serde_json::Value {
    serde_json::json!({
        "status": "degraded",
        "uptime_secs": 0,
        "node_count": 0,
        "edge_count": 0,
        "queries_processed": 0,
        "agent_sessions": [],
        "domain": "unknown",
        "graph_generation": 0,
        "plasticity_generation": 0,
    })
}

fn health_snapshot_json(
    snapshot: HttpHealthReadSnapshot,
    project_brain_runtimes: Vec<crate::brain_runtime::BrainRuntimeHealthV1>,
) -> serde_json::Value {
    let has_graph = snapshot.node_count > 0 && snapshot.edge_count > 0;
    serde_json::json!({
        "status": if snapshot.node_count > 0 { "ok" } else { "empty" },
        "owner_busy": false,
        "snapshot_freshness": "FRESH_ACTOR_SNAPSHOT",
        "uptime_secs": snapshot.uptime_secs,
        "node_count": snapshot.node_count,
        "edge_count": snapshot.edge_count,
        "queries_processed": snapshot.queries_processed,
        "agent_sessions": snapshot.agent_sessions,
        "presences": {
            "schema": crate::presence::PRESENCE_SCHEMA,
            "scope": "owner-wide",
            "status": "DEDICATED_ENDPOINT",
            "endpoint": "/api/presences",
            "non_claim": "health performs no filesystem-backed roster scan",
        },
        "domain": snapshot.domain,
        "graph_generation": snapshot.graph_generation,
        "plasticity_generation": snapshot.plasticity_generation,
        "project_brain_runtimes": project_brain_runtimes,
        "binding_fingerprint": snapshot.binding_fingerprint,
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
            "current_runtime_has_graph": has_graph,
            "next_action": "Call trust_selftest with observed_tool_count and available_tools when visible; otherwise use session_handshake, local repo smoke, or refresh the MCP host binding.",
            "non_claims": [
                "health cannot see which subset of tools the client host injected",
                "health does not rebind the host or refresh tool schemas automatically"
            ]
        }
    })
}

/// `GET /api/presences?brain=` — the P1 presence endpoint the Hall strip reads
/// (contract: `m1nd-ui` `docs/voice/P1-UI-CONTRACT.md`; authority: the P1
/// verdict, binding changes 1–3). Envelope: `{presences, collisions, served_brain?}`.
///
/// - `brain` ABSENT ⇒ the OWNER-WIDE roster (the Hall's control-room scope):
///   every live presence across all this owner's brains, no `served_brain` echo
///   (echoing the bound brain would mislabel an owner-wide roster).
/// - `brain` PRESENT ⇒ that brain's roster, filtered by the RESOLVED session's
///   own `workspace_root` (the exact key its sessions' beats write), with the
///   §4A.9.4 `served_brain` echo; an unknown root 404s honestly (the client
///   degrades to an empty roster per the contract).
/// - Pure READ, fail-open: an unreadable registry serves an empty roster; TTL
///   filtering at read (`list_live`) means no ghost is ever rendered.
/// - `collisions` is ALWAYS present (server-authoritative, even `[]`), derived
///   at read with the P1 predicate — the same one the cockpit and north use.
async fn handle_presences(
    State(state): State<Arc<AppState>>,
    Query(brain): Query<BrainQuery>,
) -> impl IntoResponse {
    let result = tokio::task::spawn_blocking(move || {
        let now = crate::util::now_ms();
        match brain
            .brain
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            None => {
                // Owner-wide: one registry serves every brain of this owner; the
                // collision predicate is same-brain by construction, so no
                // cross-brain pair can fire.
                let registry_root = http_bound_identity_snapshot(&state)?.registry_root;
                let roster = crate::presence::list_live(&registry_root, now);
                Ok(crate::presence::wire_response(&roster, now))
            }
            Some(root) => {
                let (target, served_echo) = resolve_brain(&state, Some(root))?;
                let selected_project_root = served_echo
                    .get("project_root")
                    .and_then(|value| value.as_str())
                    .map(str::to_string);
                let identity = http_target_read_snapshot(
                    &state,
                    target.clone(),
                    selected_project_root.as_deref(),
                    Arc::ptr_eq(&target, &state.session),
                    |session| Ok(HttpBrainIdentitySnapshot::from_session(session)),
                )?;
                let registry_root = identity.registry_root;
                let brain_key = identity.workspace_root;
                // The sidecar's `brain` field IS the writing session's
                // workspace_root — filter by the resolved session's own key so
                // the scope matches exactly what its traffic wrote. An unbound
                // session has no roster to join: honest empty.
                let roster = match brain_key.as_deref() {
                    Some(key) => crate::presence::roster_for_brain(&registry_root, key, now),
                    None => Vec::new(),
                };
                let mut body = crate::presence::wire_response(&roster, now);
                body["served_brain"] = served_echo;
                Ok(body)
            }
        }
    })
    .await
    // Fail-open for voice: a panicked read serves the honest empty envelope,
    // never an error wall (the same posture the contract's client side takes).
    .unwrap_or_else(|_| Ok(serde_json::json!({ "presences": [], "collisions": [] })));

    graph_response(result)
}

/// `GET /api/universe` — the Universe panorama's read-only aggregate
/// (HUMAN-VIEW-V2 F30, `m1nd-universe-v0`). One sidecar-only read of every EXISTING
/// project brain: its manifest facts (size + freshness), its live presences (grouped
/// from the owner-wide roster), its pending human gestures (merge_wait stamps from
/// the mission box + candidate ratifies from the SystemBlock store), plus the OWNER's
/// own daemon-alert scope. Composed as a pure fn so the shape is unit-testable and
/// so the HARD LAW (never hydrates a brain) is provable without an HTTP driver.
async fn handle_universe(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let state = state.clone();
    let result = tokio::task::spawn_blocking(move || universe_body(&state))
        .await
        .unwrap_or_else(|_| {
            // Fail-open: a panicked read serves an honest-empty panorama, never a wall.
            serde_json::json!({
                "schema": "m1nd-universe-v0",
                "worlds": [],
                "owner": { "alerts_pending": 0 },
                "totals": { "worlds": 0, "awake": 0, "pending": 0 },
            })
        });
    (StatusCode::OK, Json(result))
}

/// Compose the Universe panorama body. SIDECAR-ONLY and pure: it reads project-brain
/// manifests via [`crate::project_brains::ProjectBrainRegistry::disk_roster`] (never a
/// warm-boot), the presence dir via [`crate::presence::list_live`], each world's
/// mission box + SystemBlock store, and the owner's already-resident
/// `daemon_alerts` — the routing layer's `resolve`/`bootstrap` is NEVER called, so
/// `ProjectBrainRegistry.brains` is untouched (HARD LAW, tests/universe_endpoint.rs).
pub fn universe_body(state: &AppState) -> serde_json::Value {
    let now = crate::util::now_ms();
    // VITALS NEVER BLOCK THE PANORAMA (F30 §3a), and the actor fence means a REST
    // reader must never probe the raw SessionState mutex after startup.  Both
    // inputs therefore come from immutable boot/sidecar facts: the configured
    // registry root and the authoritative daemon-alert sidecar under the owner
    // runtime root. Missing sidecar = the canonical empty state; malformed or
    // unreadable bytes are an honest omission, never a fabricated count.
    let owner_runtime_root = state
        .project_brains
        .base_dir()
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    let registry_root = state.registry_dir.clone().unwrap_or_default();
    let alerts_pending =
        match std::fs::read_to_string(owner_runtime_root.join("daemon_alerts.json")) {
            Ok(text) => serde_json::from_str::<Vec<crate::session::DaemonAlert>>(&text)
                .ok()
                .map(|alerts| alerts.iter().filter(|alert| !alert.acked).count() as u64),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(0),
            Err(_) => None,
        };
    // The owner-wide live presence roster — a cross-brain read of the presence dir,
    // grouped per world below by each sidecar's `brain` (its writing session's
    // workspace_root == the world's canonical root).
    let roster = crate::presence::list_live(&registry_root, now);
    // The COLD roster: every project brain ON DISK, from its inert manifest only.
    let disk = state.project_brains.disk_roster();

    let mut worlds: Vec<serde_json::Value> = Vec::with_capacity(disk.len());
    let mut total_awake = 0u64;
    let mut total_pending = 0u64;

    for (key, facts, store_dir) in &disk {
        let root = &facts.project_root;
        let name = crate::session::basename_of(root);

        // Satellites: this world's live presences (grouped by brain == the world root).
        let presences: Vec<serde_json::Value> = roster
            .iter()
            .filter(|p| p.brain == *root)
            .map(crate::presence::wire_entry)
            .collect();
        let awake = !presences.is_empty();
        if awake {
            total_awake += 1;
        }

        // Stamps: merge_wait heads on this world's mission box (sidecar-only, fail-open).
        let (merge_wait, mission_total) = {
            let box_path = std::path::Path::new(root).join(crate::mailbox::BOX_REL_PATH);
            match crate::mailbox::read_letters(&box_path) {
                Ok(letters) => {
                    let heads = crate::mission_letter::heads_by_mission(&letters);
                    let mw = heads
                        .values()
                        .filter(|h| h.head.phase == crate::mission_letter::Phase::MergeWait)
                        .count() as u64;
                    (mw, heads.len() as u64)
                }
                Err(_) => (0, 0),
            }
        };

        // Ratifies: candidate blocks on this world's SystemBlock store (sidecar-only).
        // `SystemBlockStore::load` reads `<store_dir>/system_blocks.json` directly; a
        // missing/unreadable store is an honest zero, never an error.
        let ratifies = match crate::system_blocks::SystemBlockStore::load(store_dir) {
            Ok(Some(store)) => store
                .blocks
                .iter()
                .filter(|b| b.state == crate::system_blocks::SystemBlockState::Candidate)
                .count() as u64,
            _ => 0,
        };

        total_pending += merge_wait + ratifies;

        let mut world = serde_json::json!({
            "key": key,
            "root": root,
            "name": name,
            "awake": awake,
            "presences": presences,
            // Reads aggregated; the WRITE for each still goes through its per-type verb.
            "pending": { "stamps": merge_wait, "ratifies": ratifies },
            "letters": { "merge_wait": merge_wait, "total": mission_total },
        });
        // Size + freshness ONLY when the manifest recorded them (a pre-counts manifest
        // omits them — honest absence, never a fabricated zero or "live" state).
        if let Some(n) = facts.node_count {
            world["node_count"] = serde_json::json!(n);
        }
        if let Some(e) = facts.edge_count {
            world["edge_count"] = serde_json::json!(e);
        }
        if let Some(u) = facts.updated_ms {
            world["updated_ms"] = serde_json::json!(u);
        }
        worlds.push(world);
    }

    // Owner alerts are a universe-wide pending gesture too (owner-scope, one bucket).
    // An unreadable sidecar is OMITTED — folded in as zero so the total is never
    // inflated by a fabricated value, and declared in the body below.
    total_pending += alerts_pending.unwrap_or(0);
    let world_count = worlds.len() as u64;

    // Owner vitals: the real unacked sidecar count, or an HONEST omission (a
    // null count + a declared note) when the sidecar cannot be verified.
    let owner = match alerts_pending {
        Some(n) => serde_json::json!({ "alerts_pending": n }),
        None => serde_json::json!({
            "alerts_pending": serde_json::Value::Null,
            "note": "owner alert sidecar unavailable — vitals omitted",
        }),
    };

    serde_json::json!({
        "schema": "m1nd-universe-v0",
        "worlds": worlds,
        "owner": owner,
        "totals": { "worlds": world_count, "awake": total_awake, "pending": total_pending },
    })
}

async fn handle_instance_self(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let state = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        state
            .project_brains
            .read_target_runtime_snapshot(state.session.clone(), None, true, |session| {
                Ok(HttpInstanceSelfSnapshot::from_session(session))
            })
            .map(|snapshot| snapshot.value.into_json())
            .unwrap_or_else(|error| {
                serde_json::json!({
                    "error": "bound_brain_snapshot_unavailable",
                    "detail": error.to_string(),
                })
            })
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
    let bound = match http_bound_identity_snapshot(state) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return serde_json::json!({ "instances": [], "error": error.to_string() });
        }
    };
    let self_runtime_root_raw = bound.runtime_root.to_string_lossy().into_owned();
    let self_ingest_roots = bound.ingest_roots;
    let self_workspace_root = bound.workspace_root;
    let self_is_medulla = bound.is_medulla;
    let self_attached_sessions = bound.attached_sessions;
    let self_query_count = bound.query_count;
    let self_calibration_armed = bound.calibration_armed;
    let self_runtime_root = canon_root(&self_runtime_root_raw);
    let self_project_root =
        project_root_display_from_inputs(&self_ingest_roots, self_workspace_root.as_deref());
    let self_display_name = self_project_root
        .as_deref()
        .map(crate::session::basename_of);
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
        state
            .project_brains
            .execute_target_m1nd(state.session.clone(), None, true, true, move |session| {
                session.persist()?;
                Ok(HttpInstanceSelfSnapshot::from_session(session))
            })
            .map(HttpInstanceSelfSnapshot::into_json)
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
    let is_self = match http_bound_identity_snapshot(&state) {
        Ok(snapshot) => snapshot.instance.instance_id == instance_id,
        Err(error) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(tool_error_payload(&error)),
            )
                .into_response();
        }
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
fn served_brain_from_identity(snapshot: &HttpBrainIdentitySnapshot) -> serde_json::Value {
    let project_root = project_root_display_from_inputs(
        &snapshot.ingest_roots,
        snapshot.workspace_root.as_deref(),
    );
    let display_name = project_root.as_deref().map(crate::session::basename_of);
    served_brain_json(project_root, display_name)
}

fn bound_served_brain(state: &AppState) -> m1nd_core::error::M1ndResult<serde_json::Value> {
    http_bound_identity_snapshot(state).map(|snapshot| served_brain_from_identity(&snapshot))
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
/// Crate-private so the internal per-brain-selector suite can drive the ONE
/// resolution the HTTP handlers wrap without exporting the raw session cell as
/// a library capability.
pub(crate) fn resolve_brain(
    state: &Arc<AppState>,
    brain: Option<&str>,
) -> Result<(Arc<BrainSessionCell>, serde_json::Value), m1nd_core::error::M1ndError> {
    let Some(root) = brain.map(str::trim).filter(|s| !s.is_empty()) else {
        // Absent param = the bound graph, exactly as before.
        let echo = bound_served_brain(state)?;
        return Ok((state.session.clone(), echo));
    };

    // Does the param name the BOUND graph's own root? Compare on canonical form
    // so `/private/var` aliases and trailing slashes resolve to a match — then the
    // bound session answers, with its bound echo (no double-routing).
    let requested_key = crate::project_brains::ProjectBrainRegistry::canonical_key(root);
    let bound = http_bound_identity_snapshot(state)?;
    let bound_matches =
        project_root_display_from_inputs(&bound.ingest_roots, bound.workspace_root.as_deref())
            .map(|bound_root| {
                crate::project_brains::ProjectBrainRegistry::canonical_key(&bound_root)
                    == requested_key
            })
            .unwrap_or(false);
    if bound_matches {
        let echo = served_brain_from_identity(&bound);
        return Ok((state.session.clone(), echo));
    }

    // Otherwise it must be a KNOWN hosted project brain (warm or dormant-on-disk).
    // `resolve` warm-boots a dormant store; `None` = this owner holds no such
    // brain → honest miss, never a filesystem probe of the raw path.
    match state.project_brains.try_resolve(root)? {
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

/// Immutable request-time facts copied out of a brain under a short actor/session
/// read. Filesystem inspection, graph analysis, and runner traffic must consume
/// this value only after the `SessionState` guard has been released.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct HttpBrainReadSnapshot {
    runtime_root: std::path::PathBuf,
    graph_json: Vec<u8>,
    domain_name: String,
    ingest_roots: Vec<String>,
    workspace_root: Option<String>,
}

/// Small, capability-free identity/aliveness projection used by REST routing
/// and owner sidecar surfaces.  It crosses the brain actor as serialized data;
/// no `SessionState`, graph guard, registry handle, or runner capability can
/// escape the single-writer boundary.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct HttpBrainIdentitySnapshot {
    runtime_root: std::path::PathBuf,
    registry_root: std::path::PathBuf,
    ingest_roots: Vec<String>,
    workspace_root: Option<String>,
    read_only: bool,
    instance: InstanceRegistryEntry,
    is_medulla: bool,
    attached_sessions: u64,
    query_count: u64,
    calibration_armed: bool,
}

impl HttpBrainIdentitySnapshot {
    fn from_session(session: &SessionState) -> Self {
        Self {
            runtime_root: session.runtime_root.clone(),
            registry_root: session.instance.registry_root(),
            ingest_roots: session.ingest_roots.clone(),
            workspace_root: session.workspace_root.clone(),
            read_only: session.read_only,
            instance: session.instance.summary(),
            is_medulla: session.is_medulla_store(),
            attached_sessions: session.sessions.len() as u64,
            query_count: session.queries_processed,
            calibration_armed: session.calibration_armed(),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct HttpHealthReadSnapshot {
    uptime_secs: f64,
    node_count: usize,
    edge_count: usize,
    queries_processed: u64,
    agent_sessions: Vec<serde_json::Value>,
    domain: String,
    graph_generation: u64,
    plasticity_generation: u64,
    binding_fingerprint: serde_json::Value,
}

impl HttpHealthReadSnapshot {
    fn from_session(session: &SessionState) -> Self {
        let (node_count, edge_count) = {
            let graph = session.graph.read();
            (graph.num_nodes() as usize, graph.num_edges())
        };
        Self {
            uptime_secs: session.uptime_seconds(),
            node_count,
            edge_count,
            queries_processed: session.queries_processed,
            agent_sessions: session.session_summary(),
            domain: session.domain.name.clone(),
            graph_generation: session.graph_generation,
            plasticity_generation: session.plasticity_generation,
            binding_fingerprint: session.binding_fingerprint(),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct HttpInstanceSelfSnapshot {
    instance: InstanceRegistryEntry,
    node_count: u32,
    edge_count: u64,
    finalized: bool,
    graph_generation: u64,
    plasticity_generation: u64,
    cache_generation: u64,
    ingest_roots: Vec<String>,
    workspace_root: Option<String>,
    workspace_root_source: Option<String>,
    runtime_root: std::path::PathBuf,
    graph_path: std::path::PathBuf,
    active_agent_sessions: usize,
    queries_processed: u64,
    last_persist_secs_ago: Option<f64>,
}

impl HttpInstanceSelfSnapshot {
    fn from_session(session: &SessionState) -> Self {
        let graph = session.graph.read();
        Self {
            instance: session.instance.summary(),
            node_count: graph.num_nodes(),
            edge_count: graph.num_edges() as u64,
            finalized: graph.finalized,
            graph_generation: session.graph_generation,
            plasticity_generation: session.plasticity_generation,
            cache_generation: session.cache_generation,
            ingest_roots: session.ingest_roots.clone(),
            workspace_root: session.workspace_root.clone(),
            workspace_root_source: session.workspace_root_source.clone(),
            runtime_root: session.runtime_root.clone(),
            graph_path: session.graph_path.clone(),
            active_agent_sessions: session.sessions.len(),
            queries_processed: session.queries_processed,
            last_persist_secs_ago: session
                .last_persist_time
                .map(|time| time.elapsed().as_secs_f64()),
        }
    }

    fn into_json(self) -> serde_json::Value {
        let graph_state = serde_json::json!({
            "node_count": self.node_count,
            "edge_count": self.edge_count,
            "finalized": self.finalized,
            "graph_generation": self.graph_generation,
            "plasticity_generation": self.plasticity_generation,
            "cache_generation": self.cache_generation,
            "ingest_root_count": self.ingest_roots.len(),
            "workspace_root": self.workspace_root.clone(),
            "workspace_root_source": self.workspace_root_source,
            "runtime_root": self.runtime_root,
            "graph_path": self.graph_path.clone(),
            "graph_path_exists": self.graph_path.exists(),
        });
        let project_root =
            project_root_display_from_inputs(&self.ingest_roots, self.workspace_root.as_deref());
        let display_name = project_root.as_deref().map(crate::session::basename_of);
        serde_json::json!({
            "instance": self.instance,
            "graph_state": graph_state,
            "active_agent_sessions": self.active_agent_sessions,
            "queries_processed": self.queries_processed,
            "last_persist_secs_ago": self.last_persist_secs_ago,
            "display_name": display_name,
            "project_root": project_root,
        })
    }
}

impl HttpBrainReadSnapshot {
    fn from_session(session: &SessionState) -> m1nd_core::error::M1ndResult<Self> {
        let graph_json = m1nd_core::snapshot::encode_graph_json(&session.graph.read())?;
        Ok(Self {
            runtime_root: session.runtime_root.clone(),
            graph_json,
            domain_name: session.domain.name.clone(),
            ingest_roots: session.ingest_roots.clone(),
            workspace_root: session.workspace_root.clone(),
        })
    }

    fn decode_graph(&self) -> m1nd_core::error::M1ndResult<m1nd_core::graph::Graph> {
        m1nd_core::snapshot::decode_graph_json(&self.graph_json)
    }

    fn domain(&self) -> m1nd_core::domain::DomainConfig {
        match self.domain_name.as_str() {
            "music" => m1nd_core::domain::DomainConfig::music(),
            "memory" => m1nd_core::domain::DomainConfig::memory(),
            "generic" => m1nd_core::domain::DomainConfig::generic(),
            _ => m1nd_core::domain::DomainConfig::code(),
        }
    }

    /// Mirrors `SessionState::project_root_display`, but deliberately performs
    /// the `is_dir` probes after the session/actor snapshot has been released.
    fn project_root_display(&self) -> Option<String> {
        project_root_display_from_inputs(&self.ingest_roots, self.workspace_root.as_deref())
    }

    fn display_name(&self) -> Option<String> {
        self.project_root_display()
            .map(|root| crate::session::basename_of(&root))
    }

    fn code_root_path(&self) -> Option<String> {
        for root in &self.ingest_roots {
            if !crate::session::is_memory_sidecar(root) && std::path::Path::new(root).is_dir() {
                return Some(root.clone());
            }
        }
        self.workspace_root.as_deref().and_then(|workspace| {
            (!crate::session::is_memory_sidecar(workspace)
                && std::path::Path::new(workspace).join(".git").exists())
            .then(|| workspace.to_string())
        })
    }
}

fn project_root_display_from_inputs(
    ingest_roots: &[String],
    workspace_root: Option<&str>,
) -> Option<String> {
    for root in ingest_roots {
        if !crate::session::is_memory_sidecar(root) && std::path::Path::new(root).is_dir() {
            return Some(root.clone());
        }
    }
    if let Some(workspace) = workspace_root {
        if !crate::session::is_memory_sidecar(workspace) {
            return Some(workspace.to_string());
        }
    }
    ingest_roots
        .first()
        .cloned()
        .or_else(|| workspace_root.map(str::to_string))
}

/// Resolve a read snapshot through the hosted brain actor when the selected
/// brain is project-owned; the bound owner still uses a deliberately short
/// clone-only critical section. The returned value contains no session guard,
/// making it impossible for downstream filesystem/network/analysis code to
/// accidentally extend the global mutex lifetime.
fn http_brain_read_snapshot(
    state: &Arc<AppState>,
    target: &Arc<BrainSessionCell>,
    selected_project_root: Option<String>,
) -> m1nd_core::error::M1ndResult<HttpBrainReadSnapshot> {
    http_target_read_snapshot(
        state,
        target.clone(),
        selected_project_root.as_deref(),
        Arc::ptr_eq(target, &state.session),
        |session| {
            HttpBrainReadSnapshot::from_session(session).map_err(|error| {
                crate::runtime_jobs::RuntimeJobFailure::new(
                    "brain_snapshot_encode_failed",
                    error.to_string(),
                )
            })
        },
    )
}

/// The only REST-side read door into a brain after actor activation.  `S` must
/// survive a serde round trip, which deliberately prevents callers from
/// returning live capabilities hidden inside `SessionState`.
fn http_target_read_snapshot<S, Read>(
    state: &AppState,
    target: Arc<BrainSessionCell>,
    selected_project_root: Option<&str>,
    bound: bool,
    read: Read,
) -> m1nd_core::error::M1ndResult<S>
where
    S: serde::Serialize + serde::de::DeserializeOwned + Send + 'static,
    Read:
        FnOnce(&SessionState) -> Result<S, crate::runtime_jobs::RuntimeJobFailure> + Send + 'static,
{
    state
        .project_brains
        .read_target_runtime_snapshot(target, selected_project_root, bound, read)
        .map(|snapshot| snapshot.value)
}

fn http_bound_identity_snapshot(
    state: &AppState,
) -> m1nd_core::error::M1ndResult<HttpBrainIdentitySnapshot> {
    http_target_read_snapshot(state, Arc::clone(&state.session), None, true, |session| {
        Ok(HttpBrainIdentitySnapshot::from_session(session))
    })
}

async fn http_bound_identity_snapshot_async(
    state: &Arc<AppState>,
) -> m1nd_core::error::M1ndResult<HttpBrainIdentitySnapshot> {
    let state = Arc::clone(state);
    tokio::task::spawn_blocking(move || http_bound_identity_snapshot(&state))
        .await
        .map_err(|error| {
            m1nd_core::error::M1ndError::PersistenceFailed(format!(
                "bound brain snapshot worker failed: {error}"
            ))
        })?
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

/// Strip the optional `m1nd.`/`m1nd_` prefix from a tool name (mirrors the
/// dispatch-side normalization in `server::read_only_denied`), so `mission_spawn`,
/// `m1nd.mission_spawn` and `m1nd_mission_spawn` all resolve to the bare id.
fn bare_tool_name(tool_name: &str) -> &str {
    tool_name
        .strip_prefix("m1nd.")
        .or_else(|| tool_name.strip_prefix("m1nd_"))
        .unwrap_or(tool_name)
}

/// `mission_spawn` (§4b) — the owner→runnerd proxy. Refuses under a read-only attach
/// (it is a WRITE that launches a mission), resolves the live runner + the shared
/// secret + the workspace project_root, then FORWARDS the compose's spawn request to
/// the daemon's loopback `/run` with the secret in the `x-runnerd-secret` header (the
/// browser never sees it). The daemon's acceptance (`{mission_id, accepted}`) or its
/// honest refusal is relayed verbatim. The owner itself spawns nothing (§5d).
async fn handle_mission_spawn(
    state: &Arc<AppState>,
    served_echo: &serde_json::Value,
    body: serde_json::Value,
) -> axum::response::Response {
    // Read-only attach: the proxy is a write (it starts a mission), so refuse it
    // exactly as the dispatch gate refuses `mission_post` (§2c / the deny-list).
    let read_only = match http_bound_identity_snapshot_async(state).await {
        Ok(snapshot) => snapshot.read_only,
        Err(error) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(tool_error_payload(&error)),
            )
                .into_response();
        }
    };
    if read_only {
        let e = m1nd_core::error::M1ndError::InvalidParams {
            tool: "mission_spawn".to_string(),
            detail: "m1nd is attached read-only (--read-only); mission_spawn launches a mission (a write) and is disabled. Detach or run a read-write instance to spawn."
                .to_string(),
        };
        return (StatusCode::BAD_REQUEST, Json(tool_error_payload(&e))).into_response();
    }

    let input: crate::runnerd_owner::SpawnInput = match serde_json::from_value(body) {
        Ok(v) => v,
        Err(e) => {
            let err = m1nd_core::error::M1ndError::InvalidParams {
                tool: "mission_spawn".to_string(),
                detail: format!("invalid mission_spawn input: {e}"),
            };
            return (StatusCode::BAD_REQUEST, Json(tool_error_payload(&err))).into_response();
        }
    };

    // The workspace/routing project_root comes from the RESOLVED `?brain=` echo, not
    // from the browser body (the owner decides which repo a runner may touch, §5a).
    let workspace_root = served_echo
        .get("project_root")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let runtime_root = owner_runtime_root(state);

    let target = match crate::runnerd_owner::resolve_spawn_target(
        &state.runnerd,
        &runtime_root,
        &input,
        workspace_root.as_deref(),
    ) {
        Ok(t) => t,
        Err(e) => return (StatusCode::BAD_REQUEST, Json(tool_error_payload(&e))).into_response(),
    };

    // Forward to the daemon's loopback `/run` with the secret out-of-band (header).
    let client = reqwest::Client::new();
    let sent = client
        .post(&target.url)
        .header(crate::runnerd_owner::RUNNERD_SECRET_HEADER, &target.secret)
        .json(&target.body)
        .send()
        .await;

    let result = match sent {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let val = resp
                .json::<serde_json::Value>()
                .await
                .unwrap_or_else(|_| serde_json::json!({}));
            crate::runnerd_owner::map_runnerd_response(status, &val)
        }
        Err(e) => Err(m1nd_core::error::M1ndError::InvalidParams {
            tool: "mission_spawn".to_string(),
            detail: format!(
                "could not reach the runner daemon at {}: {e} (is m1nd-runnerd still up?)",
                target.url
            ),
        }),
    };

    match result {
        // The `{result}` envelope the UI client unwraps (like mission_post).
        Ok(inner) => (StatusCode::OK, Json(serde_json::json!({ "result": inner }))).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(tool_error_payload(&e))).into_response(),
    }
}

/// `POST /api/tools/candidate_naming` (F11-c §2b) — the in-screen "Name with
/// runner" path, HTTP-only like `mission_spawn` (the browser never holds the
/// shared secret; the owner reads it and signs the `/name` forward). Scoped to the
/// RESOLVED brain: its store supplies the target blocks, its graph supplies the
/// packet kinds/symbols. The heavy part (`name_candidate_blocks`: the blocking
/// loopback call + the `candidate_edit` apply under the caller's OCC key, runner
/// seat) runs on the blocking pool. Honest surfaces: read-only refuses; a stale
/// OCC key conflicts BEFORE any runner is invoked; no live naming-runner returns
/// the `no_naming_runner` refusal inside a 200 result (the screen disables the
/// button with the why).
async fn handle_candidate_naming(
    state: &Arc<AppState>,
    target_session: &Arc<BrainSessionCell>,
    selected_project_root: Option<String>,
    body: serde_json::Value,
) -> axum::response::Response {
    const TOOL: &str = "candidate_naming";
    let deny = |detail: String| m1nd_core::error::M1ndError::InvalidParams {
        tool: TOOL.to_string(),
        detail,
    };

    // Read-only attach: the naming apply is a write — refuse exactly like the
    // dispatch gate would (the verb is on the deny-list).
    let bound_read_only = match http_bound_identity_snapshot_async(state).await {
        Ok(snapshot) => snapshot.read_only,
        Err(error) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(tool_error_payload(&error)),
            )
                .into_response();
        }
    };
    if bound_read_only {
        let e = deny(
            "m1nd is attached read-only (--read-only); candidate_naming applies names through candidate_edit (a write) and is disabled. Detach or run a read-write instance."
                .to_string(),
        );
        return (StatusCode::BAD_REQUEST, Json(tool_error_payload(&e))).into_response();
    }

    #[derive(serde::Deserialize)]
    struct CandidateNamingBody {
        expected_store_version: u64,
        #[serde(default)]
        block_ids: Option<Vec<String>>,
    }
    let input: CandidateNamingBody = match serde_json::from_value(body) {
        Ok(v) => v,
        Err(e) => {
            let e = deny(format!("invalid candidate_naming input: {e}"));
            return (StatusCode::BAD_REQUEST, Json(tool_error_payload(&e))).into_response();
        }
    };

    let snapshot = match http_brain_read_snapshot(state, target_session, selected_project_root) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            let e = deny(err.to_string());
            return (StatusCode::BAD_REQUEST, Json(tool_error_payload(&e))).into_response();
        }
    };
    let naming_handle = Some(crate::runnerd_owner::NamingRunnerHandle {
        registry: Arc::clone(&state.runnerd),
        owner_runtime_root: owner_runtime_root(state),
    });

    // Store I/O and graph packet analysis consume only immutable copied facts.
    // They run on the blocking pool and cannot retain a SessionState guard.
    let block_ids = input.block_ids.clone();
    let prepared = tokio::task::spawn_blocking(move || {
        let graph = snapshot.decode_graph().map_err(|error| error.to_string())?;
        let dir = snapshot.runtime_root;
        let handle = naming_handle;
        let store = match crate::system_blocks::SystemBlockStore::load(&dir) {
            Ok(Some(s)) => s,
            Ok(None) => {
                return Err(
                    "no system-block store here yet — scan or import a seed before naming"
                        .to_string(),
                )
            }
            Err(err) => return Err(err.to_string()),
        };
        let targets = crate::naming_runner::select_naming_targets(&store, block_ids.as_deref())
            .map_err(|err| err.to_string())?;
        let nodes = crate::skeleton_scan::graph_nodes_for_naming(&graph);
        let packets: Vec<crate::naming_runner::BlockNamingPacket> = targets
            .iter()
            .map(|b| crate::skeleton_scan::naming_packet_for_store_block(b, &nodes))
            .collect();
        Ok::<_, String>((dir, handle, store.store_version, packets))
    })
    .await;
    let (dir, handle, store_version, packets) = match prepared {
        Ok(Ok(prepared)) => prepared,
        Ok(Err(detail)) => {
            let e = deny(detail);
            return (StatusCode::BAD_REQUEST, Json(tool_error_payload(&e))).into_response();
        }
        Err(join_err) => {
            let e = deny(format!("candidate_naming preparation failed: {join_err}"));
            return (StatusCode::BAD_REQUEST, Json(tool_error_payload(&e))).into_response();
        }
    };

    // No announce surface on this owner → the honest refusal shape (never an
    // exception: the screen reads it and says why the button is off).
    let Some(handle) = handle else {
        return (
            StatusCode::OK,
            Json(serde_json::json!({ "result": {
                "store_version": store_version,
                "named": [],
                "fell_back": [],
                "refusal": "no_naming_runner: this owner has no runner-daemon announce surface",
            }})),
        )
            .into_response();
    };

    // The blocking loopback call + the candidate_edit apply, off the async worker.
    let expected = input.expected_store_version;
    let joined = tokio::task::spawn_blocking(move || {
        crate::naming_runner::name_candidate_blocks(&handle, &dir, expected, &packets)
    })
    .await;

    match joined {
        Ok(Ok(outcome)) => (
            StatusCode::OK,
            Json(serde_json::json!({ "result": outcome })),
        )
            .into_response(),
        Ok(Err(err)) => {
            let e = deny(err.to_string());
            (StatusCode::BAD_REQUEST, Json(tool_error_payload(&e))).into_response()
        }
        Err(join_err) => {
            let e = deny(format!("candidate_naming task failed: {join_err}"));
            (StatusCode::BAD_REQUEST, Json(tool_error_payload(&e))).into_response()
        }
    }
}

/// `POST /api/tools/curation_spawn` (F12 §3) — the propose-apply curation lane,
/// HTTP-only like `candidate_naming` and `mission_spawn` (the browser never holds the
/// shared secret; the owner reads it and signs the `/curate` forward). Scoped to the
/// RESOLVED brain: its store + graph compose the block-view packet the hand-runner
/// reasons over, and the summary letter lands in ITS mission box. The heavy part
/// (`curate_candidate`: the blocking loopback call + the `candidate_edit` apply under
/// the caller's OCC key, runner seat, o5 + o1, + the lease + the mission letters)
/// runs on the blocking pool. Honest surfaces: read-only refuses; a stale OCC key
/// conflicts BEFORE any runner is invoked; no live hand-runner returns the
/// `no_hand_runner` refusal inside a 200 result (the screen falls back to DIRECT).
async fn handle_curation_spawn(
    state: &Arc<AppState>,
    target_session: &Arc<BrainSessionCell>,
    selected_project_root: Option<String>,
    body: serde_json::Value,
) -> axum::response::Response {
    const TOOL: &str = "curation_spawn";
    let deny = |detail: String| m1nd_core::error::M1ndError::InvalidParams {
        tool: TOOL.to_string(),
        detail,
    };

    // Read-only attach: the curation apply is a write — refuse exactly like the
    // dispatch gate would (the verb is HTTP-only, so it self-gates here).
    let bound_read_only = match http_bound_identity_snapshot_async(state).await {
        Ok(snapshot) => snapshot.read_only,
        Err(error) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(tool_error_payload(&error)),
            )
                .into_response();
        }
    };
    if bound_read_only {
        let e = deny(
            "m1nd is attached read-only (--read-only); curation_spawn applies a curation through candidate_edit (a write) and is disabled. Detach or run a read-write instance."
                .to_string(),
        );
        return (StatusCode::BAD_REQUEST, Json(tool_error_payload(&e))).into_response();
    }

    #[derive(serde::Deserialize)]
    struct CurationSpawnBody {
        expected_store_version: u64,
    }
    let input: CurationSpawnBody = match serde_json::from_value(body) {
        Ok(v) => v,
        Err(e) => {
            let e = deny(format!("invalid curation_spawn input: {e}"));
            return (StatusCode::BAD_REQUEST, Json(tool_error_payload(&e))).into_response();
        }
    };

    let snapshot = match http_brain_read_snapshot(state, target_session, selected_project_root) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            let e = deny(err.to_string());
            return (StatusCode::BAD_REQUEST, Json(tool_error_payload(&e))).into_response();
        }
    };
    let naming_handle = Some(crate::runnerd_owner::NamingRunnerHandle {
        registry: Arc::clone(&state.runnerd),
        owner_runtime_root: owner_runtime_root(state),
    });

    // Filesystem identity probes, store I/O, and graph packet analysis all run
    // after the clone-only session/actor snapshot and on the blocking pool.
    let prepared = tokio::task::spawn_blocking(move || {
        let graph = snapshot.decode_graph().map_err(|error| error.to_string())?;
        let dir = snapshot.runtime_root.clone();
        let handle = naming_handle;
        let store = match crate::system_blocks::SystemBlockStore::load(&dir) {
            Ok(Some(s)) => s,
            Ok(None) => {
                return Err(
                    "no system-block store here yet — scan or import a seed before curating"
                        .to_string(),
                )
            }
            Err(err) => return Err(err.to_string()),
        };
        // The mission box for THIS brain (mirror of mission_letter_handlers): the
        // repo-side box when the brain has a code root, else the medulla box.
        let box_path = match snapshot.project_root_display() {
            Some(root) => std::path::Path::new(&root).join(crate::mailbox::BOX_REL_PATH),
            None => crate::mailbox::medulla_box_path(&snapshot.runtime_root),
        };
        // The letters' brain_ref = the brain's display name (basename of its root —
        // the §1f reference, never a path), falling back to the skeleton's repo id.
        let brain_ref = snapshot
            .display_name()
            .unwrap_or_else(|| store.skeleton.skeleton_id.clone());
        let nodes = crate::skeleton_scan::graph_nodes_for_naming(&graph);
        let packet = crate::curation_runner::compose_curation_packet(&store, &nodes);
        Ok::<_, String>((dir, handle, box_path, brain_ref, packet))
    })
    .await;
    let (dir, handle, box_path, brain_ref, packet) = match prepared {
        Ok(Ok(prepared)) => prepared,
        Ok(Err(detail)) => {
            let e = deny(detail);
            return (StatusCode::BAD_REQUEST, Json(tool_error_payload(&e))).into_response();
        }
        Err(join_err) => {
            let e = deny(format!("curation_spawn preparation failed: {join_err}"));
            return (StatusCode::BAD_REQUEST, Json(tool_error_payload(&e))).into_response();
        }
    };

    // No announce surface on this owner → the honest refusal shape (never an
    // exception: the screen reads it and falls back to the DIRECT clipboard path).
    let Some(handle) = handle else {
        return (
            StatusCode::OK,
            Json(serde_json::json!({ "result": {
                "applied": false,
                "ops_count": 0,
                "store_version": packet.store_version,
                "refusal": "no_hand_runner: this owner has no runner-daemon announce surface",
            }})),
        )
            .into_response();
    };

    // The blocking loopback call + the candidate_edit apply + the letters, off the
    // async worker.
    let expected = input.expected_store_version;
    let joined = tokio::task::spawn_blocking(move || {
        crate::curation_runner::curate_candidate(
            &handle, &dir, &box_path, &brain_ref, expected, &packet,
        )
    })
    .await;

    match joined {
        Ok(Ok(outcome)) => (
            StatusCode::OK,
            Json(serde_json::json!({ "result": outcome })),
        )
            .into_response(),
        Ok(Err(err)) => (StatusCode::BAD_REQUEST, Json(tool_error_payload(&err))).into_response(),
        Err(join_err) => {
            let e = deny(format!("curation_spawn task failed: {join_err}"));
            (StatusCode::BAD_REQUEST, Json(tool_error_payload(&e))).into_response()
        }
    }
}

/// The REST arm of the one-call bootstrap: this route's own blocking + slow-op +
/// error grammar around the seam-shared `mcp_http::run_bootstrap_core` (guard →
/// guarded mint → ingest → orient — see the interception comment in
/// [`handle_tool_call`]). REST has no wire session to sticky-bind, so the
/// packet's `routing` line states THIS seam's law: address the brain with
/// `?brain=<root>` (wire callers from that root still route automatically).
/// Errors keep the generic tool-route shape — the overlap guard's
/// `overlap_<class>` refusal (`M1ndError::InvalidParams`) comes out as HTTP 400
/// `invalid_params` carrying the guard's full teaching message.
async fn handle_rest_bootstrap(
    state: &Arc<AppState>,
    project_root: String,
    body: serde_json::Value,
) -> axum::response::Response {
    let app = state.clone();
    let root = project_root.clone();
    let (slow, joined) = await_blocking_completion(
        tokio::task::spawn_blocking(move || {
            crate::mcp_http::run_bootstrap_core(&app, &root, &body)
        }),
        Duration::from_secs(TOOL_SLOW_SECS),
    )
    .await;
    if slow {
        eprintln!(
            "[m1nd] REST bootstrap exceeded the {}s slow-operation threshold; awaiting terminal result to prevent late writes",
            TOOL_SLOW_SECS
        );
    }
    match joined.expect("spawn_blocking panicked") {
        Ok((key, mut packet)) => {
            if let Some(obj) = packet.as_object_mut() {
                obj.insert(
                    "routing".into(),
                    serde_json::Value::String(format!(
                        "REST calls address this brain with the ?brain={key} selector; MCP \
                             wire calls whose resolved caller root is this repo route to it \
                             automatically"
                    )),
                );
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({ "result": packet })),
            )
                .into_response()
        }
        Err(e) => {
            // Same class→status mapping as the generic dispatch arm below, so a
            // refusal reads the same no matter which ingest shape produced it.
            let (status, error_type) = match &e {
                m1nd_core::error::M1ndError::InvalidParams { .. } => {
                    (StatusCode::BAD_REQUEST, "invalid_params")
                }
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
            };
            let mut payload = tool_error_payload(&e);
            payload["error"] = serde_json::json!(error_type);
            (status, Json(payload)).into_response()
        }
    }
}

fn mission_service_error_status(
    error: &crate::mission_service_transport::MissionServiceTransportError,
) -> StatusCode {
    mission_service_error_status_code(error.code())
}

fn mission_service_error_status_code(code: &str) -> StatusCode {
    match code {
        "missing_authenticated_authority"
        | "missing_authorization_lease"
        | "missing_transport_session"
        | "authorization_lease_not_found" => StatusCode::UNAUTHORIZED,
        "authorization_request_binding_mismatch"
        | "authorization_reservation_binding_mismatch"
        | "authorization_operation_binding_mismatch"
        | "land_authorization_receipt_binding_mismatch"
        | "authorization_receipt_binding_mismatch"
        | "authorization_receipt_signature_invalid"
        | "outer_authority_transaction_invalid"
        | "outer_authority_transaction_identity_mismatch"
        | "outer_authority_transaction_algorithm_mismatch"
        | "outer_authority_transaction_key_inactive"
        | "outer_authority_transaction_signature_invalid"
        | "signed_artifact_key_inactive"
        | "signed_artifact_algorithm_mismatch"
        | "signed_artifact_signature_invalid" => StatusCode::FORBIDDEN,
        "stale_head"
        | "stale_store_version"
        | "state_mismatch"
        | "idempotency_conflict"
        | "authorization_lease_not_unused"
        | "authorization_reservation_not_current"
        | "authorization_state_changed_before_finalization" => StatusCode::CONFLICT,
        "legacy_direct_mutation_refused" => StatusCode::GONE,
        "mission_service_unavailable"
        | "authority_runtime_unavailable"
        | "authorization_broker_unavailable"
        | "authorization_broker_poisoned"
        | "outer_authority_transaction_verifier_not_installed"
        | "signed_artifact_verifier_not_installed"
        | "authorization_receipt_verifier_not_installed"
        | "authority_wal_crypto_required" => StatusCode::SERVICE_UNAVAILABLE,
        "mission_service_io"
        | "mission_service_corruption"
        | "authorization_broker_corruption"
        | "authorization_broker_rollback_detected"
        | "broker_symlink_refused"
        | "authority_runtime_corruption"
        | "outer_authority_transaction_canonicalization_failed"
        | "signed_artifact_canonicalization_failed"
        | "authority_wal_refused"
        | "authority_wal_commit_failed"
        | "authority_wal_witness_binding_mismatch" => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::BAD_REQUEST,
    }
}

const AUTHORITY_LEASE_HEADER: &str = "m1nd-authority-lease-id";
const TRANSPORT_SESSION_HEADER: &str = "m1nd-transport-session-id";
const CALLER_ROOT_HEADER: &str = "m1nd-caller-root";

fn mission_header(headers: &HeaderMap, name: &'static str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn mission_ingress_context_digest(
    ingress: &'static str,
    transport_session_id: Option<&str>,
    caller_root: Option<&str>,
    brain_selector: Option<&str>,
) -> Option<String> {
    let transport_session_id = transport_session_id?;
    m1nd_control::digest_canonical(
        "m1nd-mission-service-ingress-context-v1",
        &(ingress, transport_session_id, caller_root, brain_selector),
    )
    .ok()
}

fn authority_error_status(
    error: &crate::authority_transport::AuthorityTransportError,
) -> StatusCode {
    authority_error_status_code(error.code())
}

fn authority_error_status_code(code: &str) -> StatusCode {
    match code {
        "missing_transport_session"
        | "missing_authority_session"
        | "authority_session_not_found"
        | "authority_session_expired" => StatusCode::UNAUTHORIZED,
        "authority_brain_mismatch"
        | "authority_session_transport_mismatch"
        | "authority_session_context_mismatch"
        | "authority_session_key_inactive"
        | "authority_session_role_not_pinned"
        | "authority_session_role_invalid"
        | "authority_session_role_mismatch"
        | "authority_binding_mismatch"
        | "authority_policy_refused"
        | "authority_crypto_refused"
        | "authority_runtime_refused"
        | "authorization_receipt_binding_mismatch"
        | "authorization_receipt_signature_invalid"
        | "outer_authority_transaction_invalid"
        | "outer_authority_transaction_identity_mismatch"
        | "outer_authority_transaction_algorithm_mismatch"
        | "outer_authority_transaction_key_inactive"
        | "outer_authority_transaction_signature_invalid"
        | "signed_artifact_key_inactive"
        | "signed_artifact_algorithm_mismatch"
        | "signed_artifact_signature_invalid" => StatusCode::FORBIDDEN,
        "authority_session_capacity_exceeded" => StatusCode::TOO_MANY_REQUESTS,
        "duplicate_authorization_lease"
        | "authority_session_challenge_not_pending"
        | "authority_session_challenge_not_found"
        | "authority_session_challenge_expired"
        | "authority_session_challenge_consumed"
        | "authority_session_challenge_replay"
        | "authority_replay_refused"
        | "authority_issuance_frozen" => StatusCode::CONFLICT,
        "authorization_broker_unavailable"
        | "authorization_broker_poisoned"
        | "authority_runtime_unavailable"
        | "authority_verifier_unavailable"
        | "authorization_receipt_signer_not_installed"
        | "authorization_receipt_verifier_not_installed"
        | "outer_authority_transaction_verifier_not_installed"
        | "signed_artifact_verifier_not_installed"
        | "authority_wal_crypto_required" => StatusCode::SERVICE_UNAVAILABLE,
        "authority_runtime_corruption"
        | "authorization_broker_corruption"
        | "authorization_broker_rollback_detected"
        | "authority_wal_refused"
        | "authority_wal_commit_failed"
        | "authority_wal_witness_binding_mismatch"
        | "outer_authority_transaction_canonicalization_failed"
        | "signed_artifact_canonicalization_failed"
        | "authorization_receipt_signing_failed" => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::BAD_REQUEST,
    }
}

async fn handle_authority_authorize(
    State(state): State<Arc<AppState>>,
    Query(brain): Query<BrainQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    let Some(authority_service) = state.authority_service.clone() else {
        let error = crate::authority_transport::AuthorityTransportError::refused(
            "authority_service_unavailable",
            "no owner AuthorityRuntime, key registry, and durable broker are installed",
        );
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(error.to_refusal(None)),
        )
            .into_response();
    };
    let request: crate::authority_transport::AuthorityAuthorizeRequestV1 =
        match serde_json::from_slice(&body) {
            Ok(request) => request,
            Err(error) => {
                let error = crate::authority_transport::AuthorityTransportError::refused(
                    "invalid_authority_authorize_request",
                    error.to_string(),
                );
                return (StatusCode::BAD_REQUEST, Json(error.to_refusal(None))).into_response();
            }
        };
    let request_id = request.request_id.clone();
    let transport_session_id = mission_header(&headers, TRANSPORT_SESSION_HEADER);
    let caller_root = mission_header(&headers, CALLER_ROOT_HEADER);
    let ingress_context_digest = mission_ingress_context_digest(
        "REST",
        transport_session_id.as_deref(),
        caller_root.as_deref(),
        brain.brain.as_deref(),
    );
    let context = crate::mission_service_transport::MissionServiceTransportContextV1 {
        ingress: crate::mission_service_transport::MissionServiceIngressV1::Rest,
        transport_session_id,
        ingress_context_digest,
        authority_lease_id: None,
        caller_root,
        route_selector: brain.brain.clone(),
        actor_brain_id: brain.brain,
    };
    let result = tokio::task::spawn_blocking(move || {
        authority_service.authorize(&context, request, crate::util::now_ms())
    })
    .await
    .expect("authority authorize task panicked");
    match result {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => {
            let status = authority_error_status(&error);
            (status, Json(error.to_refusal(Some(&request_id)))).into_response()
        }
    }
}

async fn handle_authority_session_challenge(
    State(state): State<Arc<AppState>>,
    Query(brain): Query<BrainQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    let Some(authority_service) = state.authority_service.clone() else {
        let error = crate::authority_transport::AuthorityTransportError::refused(
            "authority_service_unavailable",
            "no owner AuthorityRuntime, key registry, and durable broker are installed",
        );
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(error.to_refusal(None)),
        )
            .into_response();
    };
    let request: crate::authority_transport::AuthoritySessionChallengeRequestV1 =
        match serde_json::from_slice(&body) {
            Ok(request) => request,
            Err(error) => {
                let error = crate::authority_transport::AuthorityTransportError::refused(
                    "invalid_authority_session_challenge_request",
                    error.to_string(),
                );
                return (StatusCode::BAD_REQUEST, Json(error.to_refusal(None))).into_response();
            }
        };
    let request_id = request.request_id.clone();
    let context = authority_rest_context(&headers, brain.brain);
    let result = tokio::task::spawn_blocking(move || {
        authority_service.issue_session_challenge(&context, request, crate::util::now_ms())
    })
    .await
    .expect("authority session challenge task panicked");
    match result {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => (
            authority_error_status(&error),
            Json(error.to_refusal(Some(&request_id))),
        )
            .into_response(),
    }
}

async fn handle_authority_session_authenticate(
    State(state): State<Arc<AppState>>,
    Query(brain): Query<BrainQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    let Some(authority_service) = state.authority_service.clone() else {
        let error = crate::authority_transport::AuthorityTransportError::refused(
            "authority_service_unavailable",
            "no owner AuthorityRuntime, key registry, and durable broker are installed",
        );
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(error.to_refusal(None)),
        )
            .into_response();
    };
    let request: crate::authority_transport::AuthoritySessionAuthenticateRequestV1 =
        match serde_json::from_slice(&body) {
            Ok(request) => request,
            Err(error) => {
                let error = crate::authority_transport::AuthorityTransportError::refused(
                    "invalid_authority_session_authenticate_request",
                    error.to_string(),
                );
                return (StatusCode::BAD_REQUEST, Json(error.to_refusal(None))).into_response();
            }
        };
    let request_id = request.request_id.clone();
    let context = authority_rest_context(&headers, brain.brain);
    let result = tokio::task::spawn_blocking(move || {
        authority_service.authenticate_session(&context, request, crate::util::now_ms())
    })
    .await
    .expect("authority session authenticate task panicked");
    match result {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => (
            authority_error_status(&error),
            Json(error.to_refusal(Some(&request_id))),
        )
            .into_response(),
    }
}

fn authority_rest_context(
    headers: &HeaderMap,
    brain_selector: Option<String>,
) -> crate::mission_service_transport::MissionServiceTransportContextV1 {
    let transport_session_id = mission_header(headers, TRANSPORT_SESSION_HEADER);
    let caller_root = mission_header(headers, CALLER_ROOT_HEADER);
    let ingress_context_digest = mission_ingress_context_digest(
        "REST",
        transport_session_id.as_deref(),
        caller_root.as_deref(),
        brain_selector.as_deref(),
    );
    crate::mission_service_transport::MissionServiceTransportContextV1 {
        ingress: crate::mission_service_transport::MissionServiceIngressV1::Rest,
        transport_session_id,
        ingress_context_digest,
        authority_lease_id: None,
        caller_root,
        route_selector: brain_selector.clone(),
        actor_brain_id: brain_selector,
    }
}

async fn handle_mission_service_call(
    state: &Arc<AppState>,
    brain_selector: Option<String>,
    headers: &HeaderMap,
    body: Bytes,
) -> axum::response::Response {
    let Some(facade) = state.mission_service.clone() else {
        let error = crate::mission_service_transport::MissionServiceTransportError::refused(
            "mission_service_unavailable",
            "no canonical MissionService config and sovereign G2 authority provider are installed",
        );
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(error.to_refusal(None)),
        )
            .into_response();
    };
    let body = body.to_vec();
    let transport_session_id = mission_header(headers, TRANSPORT_SESSION_HEADER);
    let caller_root = mission_header(headers, CALLER_ROOT_HEADER);
    let ingress_context_digest = mission_ingress_context_digest(
        "REST",
        transport_session_id.as_deref(),
        caller_root.as_deref(),
        brain_selector.as_deref(),
    );
    let context = crate::mission_service_transport::MissionServiceTransportContextV1 {
        ingress: crate::mission_service_transport::MissionServiceIngressV1::Rest,
        transport_session_id,
        ingress_context_digest,
        authority_lease_id: mission_header(headers, AUTHORITY_LEASE_HEADER),
        caller_root,
        route_selector: brain_selector.clone(),
        actor_brain_id: brain_selector,
    };
    let result = tokio::task::spawn_blocking(move || facade.dispatch_wire_json(&context, &body))
        .await
        .expect("MissionService wire task panicked");
    match result {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => {
            let status = mission_service_error_status(&error);
            (status, Json(error.to_refusal(None))).into_response()
        }
    }
}

async fn handle_tool_call(
    State(state): State<Arc<AppState>>,
    Path(tool_name): Path<String>,
    Query(brain): Query<BrainQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let bare_tool = bare_tool_name(&tool_name);
    // Compatibility tombstones run before brain resolution, JSON decoding, or
    // sovereign-authority lookup. Invalid/malicious bodies cannot change the
    // refusal and cannot make the old raw write path reachable.
    if let Some(refusal) = crate::mission_service_transport::legacy_mutation_refusal(bare_tool) {
        return (StatusCode::GONE, Json(refusal)).into_response();
    }

    if bare_tool == "mission_service" {
        // The typed G3 facade owns its own brain selector and exact G2 lease
        // binding. It deliberately bypasses the generic-action gate below.
        return handle_mission_service_call(&state, brain.brain.clone(), &headers, body).await;
    }
    if bare_tool == "external_mutation_service" {
        let error = crate::external_mutation_service::ExternalMutationError::refused(
            "external_mutation_ingress_policy_disabled",
            "the first typed external-mutation slice is MCP Streamable-HTTP only; REST cannot consume this lease",
        );
        return (StatusCode::FORBIDDEN, Json(error.to_refusal(None))).into_response();
    }

    let body: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(body) => body,
        Err(error) => {
            let error = m1nd_core::error::M1ndError::InvalidParams {
                tool: tool_name.clone(),
                detail: format!("invalid JSON request body: {error}"),
            };
            return (StatusCode::BAD_REQUEST, Json(tool_error_payload(&error))).into_response();
        }
    };

    // The OWNER PROXIES are intercepted HERE, ahead of the generic floor gate —
    // the same shape the wire uses, where `mcp_http::run_mission_service_wire`
    // intercepts its typed consumers before `enforce_generic_action_policy`.
    //
    // `mission_spawn` (F2.5c §4b) is the OWNER→runnerd proxy and `candidate_naming`
    // (F11-c §2b) is the in-screen "Name with runner" path. Neither is a graph verb
    // and neither is generic dispatch: each needs owner-process state (the announce
    // registry + the shared secret the browser never holds) and its own
    // async/blocking forward to the daemon, none of which the sync `dispatch_tool`
    // sees — the same reason `mission_service` above returns before the gate.
    //
    // Both sit at SCOPED_GRANT_A2, so running the generic floor FIRST refused them
    // before the proxy built to serve them ever saw the request: the REST paths
    // behind the Human View v2 spawn and the "Name with runner" button were dead
    // code behind a 403 (project mailbox letter from `opus5-annotate`, high/bug).
    // Authority is not widened — the policy function is untouched, so every other
    // seam that consults it still refuses these two HTTP-only verbs, and each
    // handler keeps its own read-only refusal, OCC key and live-runner checks.
    if matches!(bare_tool, "mission_spawn" | "candidate_naming") {
        // Scoped to the SELECTED brain (its store, its graph, its project_root), so
        // both proxies work on any hosted brain. They resolve it themselves; the
        // gate below must keep preceding brain resolution for every generic verb.
        let (target_session, served_echo) = match resolve_brain(&state, brain.brain.as_deref()) {
            Ok(pair) => pair,
            Err(e) => return graph_response(Err(e)),
        };
        if bare_tool == "mission_spawn" {
            return handle_mission_spawn(&state, &served_echo, body).await;
        }
        let selected_project_root = served_echo
            .get("project_root")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        return handle_candidate_naming(&state, &target_session, selected_project_root, body).await;
    }

    // F-01: reject elevated generic actions before brain resolution/warm boot,
    // proxy lookup, presence tracking, freshness ticks, or handler effects.
    if let Err(error) = enforce_generic_action_policy(&tool_name, &body) {
        return (StatusCode::FORBIDDEN, Json(tool_error_payload(&error))).into_response();
    }

    // §4A.9.1: the tool route carries the SAME selector as the graph routes —
    // Reading the Tree's lenses/filters/meaning-search (seek, layers, tremor,
    // trust, impact) are all /api/tools calls, so they ride this param and answer
    // from the named brain. Authority was decided above; an unknown root now
    // 404s honestly only for an admitted ORDINARY action.
    let (target_session, served_echo) = match resolve_brain(&state, brain.brain.as_deref()) {
        Ok(pair) => pair,
        Err(e) => return graph_response(Err(e)),
    };

    // F12 (§3): `curation_spawn` is likewise HTTP-only — it needs the owner-process
    // announce registry + the shared secret (never sent to the browser) + a blocking
    // /curate forward, then applies the hand's proposal through candidate_edit (runner
    // seat, o5 + o1, OCC) and posts the mission letters. Intercepted here, scoped to
    // the RESOLVED brain (its store, its graph, its mission box).
    if bare_tool == "curation_spawn" {
        let selected_project_root = served_echo
            .get("project_root")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        return handle_curation_spawn(&state, &target_session, selected_project_root, body).await;
    }

    // Two-Tier one-call bootstrap — REST-seam parity (field hole 2026-07-10):
    // `ingest` whose body carries a non-empty `project_root` is a BOOTSTRAP
    // DIRECTIVE, not a plain graph ingest. The JSON-RPC seam has always routed it
    // through the guarded bootstrap; this route used to IGNORE the field and
    // dispatch the ingest on the RESOLVED brain — the BOUND graph when `?brain=`
    // is absent — so a bootstrap-shaped call through the REST door REPLACED the
    // owner's bound graph. Intercepted HERE — like the two proxies above — and
    // routed through the SAME seam-shared core the wire uses
    // (`mcp_http::run_bootstrap_core`), so the bound-shadow guard and the overlap
    // guard (`overlap_<class>` refusal, `allow_overlap` escape) fire identically
    // on both seams. The directive takes precedence over the `?brain=` selector,
    // exactly as it precedes per-session routing on the wire; WITHOUT
    // `project_root` the route is untouched (re-ingesting the resolved brain via
    // `?brain=` stays legitimate).
    if bare_tool == "ingest" {
        if let Some(project_root) = crate::mcp_http::bootstrap_directive(&body) {
            return handle_rest_bootstrap(&state, project_root, body).await;
        }
    }

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
    let dispatch_registry = state.project_brains.clone();
    let dispatch_project_root = served_echo
        .get("project_root")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let dispatch_is_bound = Arc::ptr_eq(&target_session, &state.session);
    let explicit_brain_selector = brain.brain.is_some();
    let caller_root_header = mission_header(&headers, CALLER_ROOT_HEADER);
    let dispatch_mutates = crate::server::read_only_denied(&tool, &body);

    // A running spawn_blocking task cannot be cancelled safely. Cross the
    // threshold as an observable slow event, then await its terminal result so
    // the HTTP response can never say "failed" while a detached worker writes.
    let (slow, joined) = await_blocking_completion(
        tokio::task::spawn_blocking(move || {
            // §4A.9: dispatch against the SELECTED brain (bound when absent) — the
            // resolution already validated the root, so this is the same brain the
            // graph routes would serve for this selector.
            dispatch_registry.execute_target_m1nd(
                target_session,
                dispatch_project_root.as_deref(),
                dispatch_is_bound,
                dispatch_mutates,
                move |session| {
                    let caller_root = session.caller_root.clone();
                    // SPEC-1g: an explicit `?brain=` selector says WHICH brain to
                    // talk to; it never says the caller legitimately inhabits that
                    // brain's root. Carried into the session so the exact-root
                    // predicate can refuse it, request-scoped and restored below
                    // exactly like `caller_root`.
                    session.explicit_brain_selector = explicit_brain_selector;
                    // SPEC-1b: the REST tools seam is one of the places a value
                    // becomes `session.caller_root`, so it canonicalizes at
                    // ingress. Scoped to the freshness door on purpose — this
                    // route has never stamped a caller root, and widening that to
                    // every verb would change reception/routing behaviour far
                    // outside SPEC-1. Reuses the external-mutation seam's own
                    // canonicalization precedent.
                    let stamped_caller_root = crate::server::refresh_caller_root_from_header(
                        &tool,
                        &body,
                        &caller_root_header,
                    );
                    if let Some(root) = stamped_caller_root {
                        session.caller_root = Some(root);
                    }
                    if explicit_brain_selector
                        && crate::server::skeleton_write_needs_root_gate(&tool, &body)
                    {
                        session.caller_root = session.workspace_root.clone();
                    }
                    if tool == "apply_batch" {
                        session.apply_batch_progress_sink = Some(apply_batch_progress_sink(
                            progress_event_tx.clone(),
                            progress_event_log_path.clone(),
                            "http".to_string(),
                            progress_agent_id.clone(),
                        ));
                    }
                    if tool == "skeleton_candidate" {
                        session.scan_progress_sink = Some(scan_progress_sink(
                            progress_event_tx.clone(),
                            progress_event_log_path.clone(),
                            "http".to_string(),
                            progress_agent_id.clone(),
                        ));
                    }
                    if crate::server::tool_tracks_agent_presence(&tool) {
                        if let Some(agent_id) = body.get("agent_id").and_then(|v| v.as_str()) {
                            session.track_agent(agent_id);
                        }
                    }
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        dispatch_generic_tool(session, &tool, &body)
                    }));
                    session.caller_root = caller_root;
                    session.explicit_brain_selector = false;
                    session.apply_batch_progress_sink = None;
                    session.scan_progress_sink = None;
                    match result {
                        Ok(result) => result,
                        Err(payload) => std::panic::resume_unwind(payload),
                    }
                },
            )
        }),
        Duration::from_secs(TOOL_SLOW_SECS),
    )
    .await;

    if slow {
        let sse_event = SseEvent {
            event_type: "tool_slow".to_string(),
            data: serde_json::json!({
                "tool": tool_for_event,
                "source": "http",
                "agent_id": agent_id_for_event,
                "slow_after_secs": TOOL_SLOW_SECS,
                "terminal_result_pending": true,
                "timestamp_ms": now_ms(),
            }),
        };
        let _ = event_tx.send(sse_event.clone());
        if let Some(ref log_path) = event_log_path {
            append_event_to_log(log_path, &sse_event);
        }
    }
    let inner = joined.expect("spawn_blocking panicked");

    // Broadcast SSE event for the terminal tool result.
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
                m1nd_core::error::M1ndError::Serde(_) => (StatusCode::BAD_REQUEST, "invalid_json"),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
            };
            let mut payload = tool_error_payload(&e);
            payload["error"] = serde_json::json!(error_type);
            (status, Json(payload)).into_response()
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

    let selected_project_root = served_brain
        .get("project_root")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let snapshot = match http_brain_read_snapshot(&state, &target, selected_project_root) {
        Ok(snapshot) => snapshot,
        Err(error) => return graph_response(Err(error)),
    };

    let result: serde_json::Value = tokio::task::spawn_blocking(move || {
        let start = std::time::Instant::now();
        let graph = match snapshot.decode_graph() {
            Ok(graph) => graph,
            Err(error) => {
                return serde_json::json!({
                    "nodes": [],
                    "edges": [],
                    "meta": {
                        "total_nodes": 0,
                        "rendered_nodes": 0,
                        "query": query,
                        "elapsed_ms": start.elapsed().as_millis() as u64,
                        "error": error.to_string(),
                    }
                });
            }
        };
        let domain = snapshot.domain();
        let orchestrator = m1nd_core::query::QueryOrchestrator::build(&graph);
        let activate_result = orchestrator.and_then(|orchestrator| {
            orchestrator.query_readonly(
                &graph,
                &m1nd_core::query::QueryConfig {
                    query: query.clone(),
                    agent_id: "gui-subgraph".to_string(),
                    top_k,
                    dimensions: vec![
                        m1nd_core::types::Dimension::Structural,
                        m1nd_core::types::Dimension::Semantic,
                        m1nd_core::types::Dimension::Temporal,
                        m1nd_core::types::Dimension::Causal,
                    ],
                    xlr_enabled: true,
                    include_ghost_edges: true,
                    include_structural_holes: false,
                    propagation: m1nd_core::types::PropagationConfig::default(),
                },
                &domain,
            )
        });

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
                let n = graph.num_nodes() as usize;

                // Build reverse map: NodeId -> external_id
                let mut node_to_ext: Vec<String> = vec![String::new(); n];
                for (interned, &nid) in &graph.id_to_node {
                    let idx = nid.as_usize();
                    if idx < n {
                        node_to_ext[idx] = graph.strings.resolve(*interned).to_string();
                    }
                }

                // Preserve the activate handler's exact ranking/deduplication
                // semantics while keeping the query read-only and lock-free at
                // the SessionState layer.
                let activated = output
                    .activation
                    .activated
                    .iter()
                    .map(|activated| {
                        let idx = activated.node.as_usize();
                        let (node_id, label, node_type, tags, provenance) = if idx < n {
                            let provenance = graph.resolve_node_provenance(activated.node);
                            let provenance = if provenance.is_empty() {
                                None
                            } else {
                                Some(crate::protocol::ProvenanceOutput {
                                    source_path: provenance.source_path,
                                    line_start: provenance.line_start,
                                    line_end: provenance.line_end,
                                    excerpt: provenance.excerpt,
                                    namespace: provenance.namespace,
                                    canonical: provenance.canonical,
                                })
                            };
                            (
                                node_to_ext[idx].clone(),
                                graph.strings.resolve(graph.nodes.label[idx]).to_string(),
                                format!("{:?}", graph.nodes.node_type[idx]),
                                graph.nodes.tags[idx]
                                    .iter()
                                    .map(|&tag| graph.strings.resolve(tag).to_string())
                                    .collect(),
                                provenance,
                            )
                        } else {
                            (
                                format!("node_{idx}"),
                                format!("node_{idx}"),
                                "Unknown".to_string(),
                                Vec::new(),
                                None,
                            )
                        };
                        crate::protocol::ActivatedNodeOutput {
                            node_id,
                            label,
                            node_type,
                            activation: activated.activation.get(),
                            dimensions: crate::protocol::DimensionsOutput {
                                structural: activated.dimensions[0].get(),
                                semantic: activated.dimensions[1].get(),
                                temporal: activated.dimensions[2].get(),
                                causal: activated.dimensions[3].get(),
                            },
                            pagerank: if idx < graph.nodes.pagerank.len() {
                                graph.nodes.pagerank[idx].get()
                            } else {
                                0.0
                            },
                            tags,
                            provenance,
                        }
                    })
                    .collect::<Vec<_>>();
                let activated = crate::result_shaping::dedupe_ranked(activated, top_k);
                let total_nodes = activated.len();

                // Collect top_k node external IDs and resolve to NodeIds
                let mut top_node_ids: Vec<m1nd_core::types::NodeId> = Vec::new();
                let mut top_ext_ids: std::collections::HashSet<String> =
                    std::collections::HashSet::new();
                let mut subgraph_nodes: Vec<serde_json::Value> = Vec::new();

                for activated in &activated {
                    let ext_id = activated.node_id.as_str();
                    if ext_id.is_empty() {
                        continue;
                    }
                    if !top_ext_ids.insert(ext_id.to_string()) {
                        continue;
                    }
                    if let Some(nid) = graph.resolve_id(ext_id) {
                        let idx = nid.as_usize();
                        if idx < n {
                            top_node_ids.push(nid);

                            let label = graph.strings.resolve(graph.nodes.label[idx]).to_string();
                            let node_type_val = node_type_to_u8(graph.nodes.node_type[idx]);
                            let activation = activated.activation;
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

                // 4. Also add ghost edges from the same read-only activation.
                for ghost in &output.ghost_edges {
                    let source_idx = ghost.source.as_usize();
                    let target_idx = ghost.target.as_usize();
                    let source = if source_idx < n {
                        graph
                            .strings
                            .resolve(graph.nodes.label[source_idx])
                            .to_string()
                    } else {
                        format!("node_{source_idx}")
                    };
                    let target = if target_idx < n {
                        graph
                            .strings
                            .resolve(graph.nodes.label[target_idx])
                            .to_string()
                    } else {
                        format!("node_{target_idx}")
                    };
                    if top_ext_ids.contains(&source) && top_ext_ids.contains(&target) {
                        subgraph_edges.push(serde_json::json!({
                            "source": source,
                            "target": target,
                            "weight": ghost.strength.get(),
                            "relation": "ghost",
                        }));
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
        let (target, served_brain) = resolve_brain(&state, brain.brain.as_deref())?;
        let selected_project_root = served_brain
            .get("project_root")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let snapshot = http_brain_read_snapshot(&state, &target, selected_project_root)?;
        let graph = snapshot.decode_graph()?;
        Ok::<_, m1nd_core::error::M1ndError>(serde_json::json!({
            "node_count": graph.num_nodes(),
            "edge_count": graph.num_edges(),
            "domain": snapshot.domain_name,
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
        let (target, served_brain) = resolve_brain(&state, brain.brain.as_deref())?;
        let selected_project_root = served_brain
            .get("project_root")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let snapshot = http_brain_read_snapshot(&state, &target, selected_project_root)?;
        let graph = snapshot.decode_graph()?;
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
/// A PURE READ: returns a member file's content under the selected brain's CODE
/// root (`code_root_path()` — never the raw workspace_root, which for a hosted
/// brain is its store dir), enforcing the seed's anti-absolute/anti-escape law + a
/// byte cap with honest truncation (`crate::system_blocks::read_repo_relative_file`).
/// Never mutates — safe under a read-only attach, so it is NOT in the write
/// deny-list. Path validation/escape → 400; a brain with no code root → 400; a
/// missing file or an unknown `?brain=` → 404.
async fn handle_file_view(
    State(state): State<Arc<AppState>>,
    Query(q): Query<FileViewQuery>,
) -> impl IntoResponse {
    let state = state.clone();
    let outcome = tokio::task::spawn_blocking(
        move || -> Result<serde_json::Value, (StatusCode, m1nd_core::error::M1ndError)> {
            // §4A.9: route to the named brain (bound when absent); an unknown root
            // 404s honestly (the same grade the graph routes give it).
            let (target, served_brain) = resolve_brain(&state, q.brain.as_deref())
                .map_err(|e| (StatusCode::NOT_FOUND, e))?;
            // Resolve the CODE root, never the raw workspace_root — a hosted/memory
            // brain's workspace_root is its STORE dir (agent-memory sidecars), so
            // reading a repo member under it 404s on the store (the field bug). This
            // is the same resolution `skeleton_candidate`/`reconcile` use (#326); a
            // brain with no code root at all is an honest refusal, never a store read.
            let selected_project_root = served_brain
                .get("project_root")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            let root = http_brain_read_snapshot(&state, &target, selected_project_root)
                .map_err(|e| (StatusCode::NOT_FOUND, e))?
                .code_root_path()
                .ok_or_else(|| {
                    (
                        StatusCode::BAD_REQUEST,
                        m1nd_core::error::M1ndError::InvalidParams {
                            tool: "file_view".into(),
                            detail: "no CODE root is bound to this brain — file_view reads \
                             the repo (a hosted brain's raw workspace is its store dir, \
                             which holds no viewable source)"
                                .into(),
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
    if let Ok(identity) = http_bound_identity_snapshot(state) {
        if let Some(bound) = project_root_display_from_inputs(
            &identity.ingest_roots,
            identity.workspace_root.as_deref(),
        ) {
            roots.push(bound);
        }
    }
    for (_key, facts, _dir) in state.project_brains.disk_roster() {
        roots.push(facts.project_root);
    }
    roots
}

fn owner_runtime_root(state: &AppState) -> std::path::PathBuf {
    state
        .project_brains
        .base_dir()
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default()
}

/// The bound owner's runtime root (the medulla box's home) + the worktree base
/// name (the bound project's basename, whose worktrees are `<base>-*`).
fn owner_runtime_and_base(state: &Arc<AppState>) -> (std::path::PathBuf, String) {
    let base = http_bound_identity_snapshot(state)
        .ok()
        .and_then(|identity| {
            project_root_display_from_inputs(
                &identity.ingest_roots,
                identity.workspace_root.as_deref(),
            )
        })
        .as_deref()
        .map(crate::session::basename_of)
        .unwrap_or_default();
    (owner_runtime_root(state), base)
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
        let runtime_root = owner_runtime_root(&state);

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
            let (target, served_brain) = resolve_brain(&state, q.brain.as_deref())?;
            let selected_project_root = served_brain
                .get("project_root")
                .and_then(|value| value.as_str())
                .map(str::to_string);
            let repo_root = http_brain_read_snapshot(&state, &target, selected_project_root)?
                .project_root_display();
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

        let roots =
            crate::mailbox::project_roots_for_runtime(&runtime_root, &known_project_roots(&state))?;
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

// ---------------------------------------------------------------------------
// F2.5c (§5a) — the runner daemon liveness surface (announce + status).
// ---------------------------------------------------------------------------

/// `POST /api/runnerd/announce` (§5a) — a booting runner daemon proves LIVENESS.
/// The request carries `{runner_ids, port, boot_challenge}` and the shared secret
/// in the `x-runnerd-secret` header; the owner authenticates it against the on-disk
/// `<runtime_root>/runnerd.secret` and, on success, registers each runner id live
/// at `port` + echoes the challenge. A missing/wrong secret is a BARE 401 (§5a:
/// refuse without leaking why). Loopback is the ambient guard: every non-loopback
/// server bind is refused even when the legacy `--allow-remote` flag is present,
/// and the secret is the real application gate — the same-UID threat is declared
/// out of scope (§5d).
///
/// Announce NEVER creates or widens a capability (§5a) — the registry holds only
/// liveness; the pins live in the daemon's own `runners.toml`.
async fn handle_runnerd_announce(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(input): Json<crate::runnerd_owner::AnnounceInput>,
) -> impl IntoResponse {
    let provided = headers
        .get(crate::runnerd_owner::RUNNERD_SECRET_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let runtime_root = owner_runtime_root(&state);
    let outcome = crate::runnerd_owner::apply_announce(
        &state.runnerd,
        &runtime_root,
        provided.as_deref(),
        &input,
        now_ms(),
    );
    match outcome {
        // Bare 401 — no detail body (§5a: never leak why the secret was refused).
        crate::runnerd_owner::AnnounceOutcome::Unauthorized => {
            StatusCode::UNAUTHORIZED.into_response()
        }
        crate::runnerd_owner::AnnounceOutcome::Registered(body) => {
            (StatusCode::OK, Json(body)).into_response()
        }
    }
}

/// `GET /api/runnerd/status` (§5a read) — the live runner registry: every
/// announced `runner_id` with its port + last_seen. A pure read (no secret): it
/// exposes only liveness, which the compose UI reads to un-disable the spawn radio
/// and list the pinned-live runners. Empty when no daemon has announced.
async fn handle_runnerd_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    (StatusCode::OK, Json(state.runnerd.status_json()))
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

/// Materialize the exact release tree exposed through `UiAssets::get`. Comparing
/// this identity with the build record closes the build.rs→rust-embed race: only
/// bytes that actually reached the binary can earn AVAILABLE/FRESH.
fn embedded_ui_identity() -> Result<crate::ui_bundle_support::UiTreeIdentity, String> {
    let mut entries = Vec::new();
    for path in UiAssets::iter() {
        let content = UiAssets::get(path.as_ref())
            .ok_or_else(|| format!("embedded UI asset disappeared during attestation: {path}"))?;
        entries.push((path.into_owned(), content.data.into_owned()));
    }
    Ok(crate::ui_bundle_support::ui_tree_identity_from_entries(
        entries,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_http_statuses_preserve_auth_overload_and_integrity_classes() {
        for code in [
            "authority_session_key_inactive",
            "authority_session_role_not_pinned",
            "authority_session_role_invalid",
            "authority_session_role_mismatch",
            "authorization_receipt_binding_mismatch",
            "authorization_receipt_signature_invalid",
            "outer_authority_transaction_identity_mismatch",
            "outer_authority_transaction_algorithm_mismatch",
            "outer_authority_transaction_signature_invalid",
            "signed_artifact_key_inactive",
            "signed_artifact_algorithm_mismatch",
            "signed_artifact_signature_invalid",
        ] {
            assert_eq!(
                authority_error_status_code(code),
                StatusCode::FORBIDDEN,
                "{code}"
            );
        }
        assert_eq!(
            authority_error_status_code("authority_session_capacity_exceeded"),
            StatusCode::TOO_MANY_REQUESTS
        );
        for code in [
            "authorization_receipt_signer_not_installed",
            "authorization_receipt_verifier_not_installed",
            "outer_authority_transaction_verifier_not_installed",
            "signed_artifact_verifier_not_installed",
        ] {
            assert_eq!(
                authority_error_status_code(code),
                StatusCode::SERVICE_UNAVAILABLE,
                "{code}"
            );
        }
        for code in [
            "authority_runtime_corruption",
            "authorization_broker_corruption",
            "authorization_broker_rollback_detected",
            "authority_wal_refused",
            "authority_wal_commit_failed",
            "outer_authority_transaction_canonicalization_failed",
            "signed_artifact_canonicalization_failed",
        ] {
            assert_eq!(
                authority_error_status_code(code),
                StatusCode::INTERNAL_SERVER_ERROR,
                "{code}"
            );
        }
    }

    #[test]
    fn mission_http_statuses_preserve_signed_artifact_and_wal_classes() {
        for code in [
            "authorization_receipt_binding_mismatch",
            "authorization_receipt_signature_invalid",
            "outer_authority_transaction_invalid",
            "outer_authority_transaction_identity_mismatch",
            "outer_authority_transaction_algorithm_mismatch",
            "outer_authority_transaction_signature_invalid",
            "signed_artifact_key_inactive",
            "signed_artifact_algorithm_mismatch",
            "signed_artifact_signature_invalid",
        ] {
            assert_eq!(
                mission_service_error_status_code(code),
                StatusCode::FORBIDDEN,
                "{code}"
            );
        }
        for code in [
            "outer_authority_transaction_verifier_not_installed",
            "signed_artifact_verifier_not_installed",
            "authorization_receipt_verifier_not_installed",
            "authority_wal_crypto_required",
        ] {
            assert_eq!(
                mission_service_error_status_code(code),
                StatusCode::SERVICE_UNAVAILABLE,
                "{code}"
            );
        }
        for code in [
            "authorization_broker_rollback_detected",
            "authority_runtime_corruption",
            "authority_wal_refused",
            "outer_authority_transaction_canonicalization_failed",
            "signed_artifact_canonicalization_failed",
        ] {
            assert_eq!(
                mission_service_error_status_code(code),
                StatusCode::INTERNAL_SERVER_ERROR,
                "{code}"
            );
        }
    }

    #[tokio::test]
    async fn required_authority_background_boot_refuses_before_endpoint_publication() {
        let temporary = tempfile::tempdir().expect("temporary runtime");
        let runtime = temporary.path().join("runtime");
        let state = crate::server::McpServer::new(McpConfig {
            graph_source: runtime.join("graph_snapshot.json"),
            plasticity_state: runtime.join("plasticity_state.json"),
            runtime_dir: Some(runtime.clone()),
            registry_dir: Some(runtime.join("registry")),
            ..Default::default()
        })
        .expect("boot owner")
        .into_session_state();
        let session = Arc::new(BrainSessionCell::new(state));
        let before = session
            .read()
            .expect("pre-actor session")
            .instance
            .summary();

        let error = match spawn_background_with_owner_authority(
            Arc::clone(&session),
            0,
            None,
            crate::owner_security_config::OwnerAuthorityBootRequirementV1::Required,
        )
        .await
        {
            Ok(_) => panic!("required authority must refuse a missing production assembly"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            crate::owner_security_config::OwnerAuthorityAssemblyError::ProductionAdapterNotInstalled
        ));
        let after = session
            .read()
            .expect("pre-actor session")
            .instance
            .summary();
        assert_eq!(
            after.bind, before.bind,
            "refusal cannot publish a bind address"
        );
        assert_eq!(after.port, before.port, "refusal cannot publish a port");
        assert_eq!(
            after.last_heartbeat_ms, before.last_heartbeat_ms,
            "refusal cannot start an owner heartbeat"
        );
        session
            .lock_mut_before_actor()
            .expect("pre-actor session")
            .instance
            .release()
            .expect("release test instance");
    }

    #[tokio::test]
    async fn background_handle_is_a_readiness_and_checkpoint_shutdown_receipt() {
        let temporary = tempfile::tempdir().expect("background runtime");
        let runtime = temporary.path().join("runtime");
        let registry_dir = runtime.join("registry");
        let state = crate::server::McpServer::new(McpConfig {
            graph_source: runtime.join("graph_snapshot.json"),
            plasticity_state: runtime.join("plasticity_state.json"),
            runtime_dir: Some(runtime.clone()),
            registry_dir: Some(registry_dir.clone()),
            ..Default::default()
        })
        .expect("boot owner")
        .into_session_state();
        let runtime_root = state.instance.summary().runtime_root;
        let session = Arc::new(BrainSessionCell::new(state));

        let handle = spawn_background(Arc::clone(&session), 0)
            .await
            .expect("ready background HTTP");
        let ready_addr = handle.local_addr();
        assert!(ready_addr.ip().is_loopback());
        assert_ne!(ready_addr.port(), 0, "port zero must resolve before return");
        let published = crate::instance_registry::list_instances(Some(&registry_dir))
            .expect("published owner")
            .into_iter()
            .find(|entry| entry.runtime_root == runtime_root && entry.mode == "read_write")
            .expect("owner entry");
        assert_eq!(published.bind.as_deref(), Some("127.0.0.1"));
        assert_eq!(published.port, Some(ready_addr.port()));

        let acks = handle
            .shutdown()
            .await
            .expect("background shutdown receipt");
        assert_eq!(acks.len(), 1, "bound actor returns one final ACK");
        let mut owner = session
            .lock_mut_before_actor()
            .expect("background shutdown returned SessionState");
        let summary = owner.instance.summary();
        assert!(summary.bind.is_none());
        assert!(summary.port.is_none());
        owner.instance.release().expect("release shared owner");
    }

    #[tokio::test]
    async fn background_bind_collision_is_an_error_without_endpoint_publication() {
        let occupied = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("occupy port");
        let port = occupied.local_addr().expect("occupied address").port();
        let temporary = tempfile::tempdir().expect("background collision runtime");
        let runtime = temporary.path().join("runtime");
        let state = crate::server::McpServer::new(McpConfig {
            graph_source: runtime.join("graph_snapshot.json"),
            plasticity_state: runtime.join("plasticity_state.json"),
            runtime_dir: Some(runtime.clone()),
            registry_dir: Some(runtime.join("registry")),
            ..Default::default()
        })
        .expect("boot owner")
        .into_session_state();
        let session = Arc::new(BrainSessionCell::new(state));

        let error = match spawn_background(Arc::clone(&session), port).await {
            Ok(_) => panic!("occupied port cannot produce a ready handle"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("owner_background_bind_failed"));
        let mut owner = session
            .lock_mut_before_actor()
            .expect("bind refusal cannot start actor");
        let summary = owner.instance.summary();
        assert!(summary.bind.is_none());
        assert!(summary.port.is_none());
        owner.instance.release().expect("release refused owner");
    }

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
    fn scan_progress_sink_emits_flat_phase_event_with_envelope() {
        let (tx, mut rx) = broadcast::channel::<SseEvent>(16);
        let sink = scan_progress_sink(tx, None, "http".to_string(), "tester".to_string());
        sink(&crate::skeleton_scan::ScanProgressEvent::naming(8, 2));

        let ev = rx.try_recv().expect("scan_progress event");
        assert_eq!(ev.event_type, "scan_progress");
        // The phase fields flatten under `data`, joined by the standard envelope.
        assert_eq!(ev.data["phase"].as_str(), Some("naming"));
        assert_eq!(ev.data["block_count"].as_u64(), Some(8));
        assert_eq!(ev.data["naming_waves"].as_u64(), Some(2));
        assert_eq!(ev.data["tool"].as_str(), Some("skeleton_candidate"));
        assert_eq!(ev.data["source"].as_str(), Some("http"));
        assert_eq!(ev.data["agent_id"].as_str(), Some("tester"));
        assert!(ev.data["timestamp_ms"].as_u64().is_some());
        // No fabricated fraction rides the wire.
        assert!(ev.data.get("progress_pct").is_none());
        assert!(ev.data.get("percent").is_none());
    }

    #[test]
    fn scan_progress_sink_is_fail_open_without_a_subscriber() {
        // Zero live receivers → broadcast::send returns Err, which the sink IGNORES
        // (`let _ = event_tx.send(..)`): emitting narration can never break a scan.
        let (tx, _) = broadcast::channel::<SseEvent>(16);
        let sink = scan_progress_sink(tx, None, "http".to_string(), "tester".to_string());
        sink(&crate::skeleton_scan::ScanProgressEvent::done(3)); // must not panic
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

    #[tokio::test]
    async fn slow_blocking_work_is_joined_instead_of_detached() {
        let completed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let completed_in_task = completed.clone();
        let task = tokio::task::spawn_blocking(move || {
            std::thread::sleep(Duration::from_millis(20));
            completed_in_task.store(true, std::sync::atomic::Ordering::SeqCst);
            42_u8
        });

        let (slow, joined) = await_blocking_completion(task, Duration::from_millis(1)).await;
        assert!(slow);
        assert_eq!(joined.expect("blocking task must join"), 42);
        assert!(completed.load(std::sync::atomic::Ordering::SeqCst));
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
        assert_eq!(remote_bind_verdict("127.0.0.1", false), Ok(()));
        assert_eq!(remote_bind_verdict("::1", false), Ok(()));
        // Whitespace is tolerated.
        assert_eq!(remote_bind_verdict("  127.0.0.1  ", false), Ok(()));
    }

    #[test]
    fn wildcard_bind_is_refused_with_or_without_the_flag() {
        // The regression this closes: `0.0.0.0` used to bind with only a stderr
        // warning. It must now be a hard refusal regardless of the legacy flag.
        let verdict = remote_bind_verdict("0.0.0.0", false);
        let msg = verdict.expect_err("0.0.0.0 without --allow-remote must refuse");
        assert!(msg.contains("REFUSING"), "got: {msg}");
        assert!(
            msg.contains("authenticated remote transport"),
            "must name the unavailable security boundary: {msg}"
        );
        assert!(
            msg.contains("--allow-remote cannot override"),
            "must make the legacy flag's fail-closed behavior explicit: {msg}"
        );
        assert!(remote_bind_verdict("0.0.0.0", true).is_err());
        // `::` (IPv6 wildcard) is refused the same way.
        assert!(remote_bind_verdict("::", false).is_err());
        assert!(remote_bind_verdict("::", true).is_err());
    }

    #[test]
    fn concrete_lan_ip_and_hostname_are_always_refused() {
        // Stricter than a literal `== "0.0.0.0"`: any non-loopback address, incl.
        // a concrete LAN IP, is gated.
        assert!(remote_bind_verdict("127.0.0.1.10", false).is_err());
        assert!(remote_bind_verdict("127.0.0.1.10", true).is_err());
        assert!(remote_bind_verdict("10.0.0.5", false).is_err());
        assert!(remote_bind_verdict("10.0.0.5", true).is_err());
        // A non-IP hostname is fail-closed (treated as potentially remote).
        assert!(remote_bind_verdict("example.local", false).is_err());
        assert!(remote_bind_verdict("example.local", true).is_err());
    }

    #[test]
    fn loopback_bind_ignores_the_flag() {
        // The flag is a no-op for loopback — still no warning.
        assert_eq!(remote_bind_verdict("127.0.0.1", true), Ok(()));
    }

    #[tokio::test]
    async fn manifest_endpoint_serves_a_sealed_read_only_projection() {
        let dir = tempfile::tempdir().expect("temp runtime");
        let app = rest_owner(dir.path());
        let response = handle_manifest(State(app), Query(BrainQuery { brain: None })).await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("manifest body");
        let payload: serde_json::Value = serde_json::from_slice(&body).expect("manifest json");
        assert_eq!(
            payload["schema"],
            crate::organism_manifest::MANIFEST_RESPONSE_SCHEMA
        );
        assert_eq!(
            payload["manifest"]["manifest_sha256"],
            payload["verification"]["computed_manifest_sha256"]
        );
        assert!(payload["manifest"]["generated_at"].as_u64().is_some());
        assert_eq!(payload["manifest"]["autonomy"]["active_mode"], "UNKNOWN");
        assert_eq!(payload["manifest"]["autonomy"]["issuance_frozen"], true);
    }

    #[tokio::test]
    async fn manifest_endpoint_routes_to_the_selected_brain_and_404s_unknown_roots() {
        let dir = tempfile::tempdir().expect("temp runtime");
        let app = rest_owner(&dir.path().join("runtime"));
        let hosted = dir.path().join("repo-hosted");
        write_repo(&hosted, "hosted");
        let hosted_key = app
            .project_brains
            .ensure_registered(&hosted.to_string_lossy())
            .expect("register hosted brain");

        let response = handle_manifest(
            State(app.clone()),
            Query(BrainQuery {
                brain: Some(hosted_key),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("hosted manifest body");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("hosted manifest json");
        assert_eq!(payload["manifest"]["repo_id"], "repo-hosted");

        let response = handle_manifest(
            State(app),
            Query(BrainQuery {
                brain: Some(dir.path().join("unknown").to_string_lossy().into_owned()),
            }),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("unknown manifest body");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("unknown manifest json");
        assert_eq!(payload["error"], "unknown_brain");
    }

    #[tokio::test]
    async fn manifest_endpoint_names_an_unstable_snapshot_without_publishing_facts() {
        let response = manifest_read_response(Err(ManifestReadError::SnapshotUnstable(
            "generation 7 -> 8".into(),
        )));
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("unstable manifest body");
        let payload: serde_json::Value =
            serde_json::from_slice(&body).expect("unstable manifest json");
        assert_eq!(payload["error"], "manifest_snapshot_unstable");
        assert_eq!(payload["detail"], "generation 7 -> 8");
        assert!(
            payload.get("manifest").is_none(),
            "an unstable observation must not publish partial facts"
        );
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

    // ------------------------------------------------------------------
    // REST one-call bootstrap parity (field hole 2026-07-10). The JSON-RPC seam
    // routed `ingest`+`project_root` through the guarded bootstrap, but THIS
    // route ignored the directive and dispatched the ingest on the resolved
    // brain — the BOUND graph when `?brain=` is absent — so a bootstrap-shaped
    // call through the REST door REPLACED the owner's bound graph (live smoke:
    // a parent-folder project_root clobbered the bound ingest_roots). These
    // tests drive the REAL `handle_tool_call` handler: the refusal, the mint
    // parity, the untouched-bound law, the escape hatch, and the no-directive
    // dispatch that must never regress.
    // ------------------------------------------------------------------

    /// A tiny but non-empty repo so a real ingest produces > 0 nodes.
    fn write_repo(root: &std::path::Path, name: &str) {
        std::fs::create_dir_all(root.join("src")).expect("mk src");
        std::fs::write(
            root.join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\n"),
        )
        .expect("Cargo.toml");
        std::fs::write(
            root.join("src/lib.rs"),
            format!("pub fn {name}_probe() -> i64 {{ 1 }}\n"),
        )
        .expect("lib.rs");
    }

    /// A real AppState around a real SessionState + project-brain registry — the
    /// same altitude the wire-seam tests build, driven through the REAL
    /// `handle_tool_call` handler below.
    fn rest_owner(runtime: &std::path::Path) -> Arc<AppState> {
        std::fs::create_dir_all(runtime).expect("mk runtime");
        let config = McpConfig {
            graph_source: runtime.join("graph_snapshot.json"),
            plasticity_state: runtime.join("plasticity_state.json"),
            runtime_dir: Some(runtime.to_path_buf()),
            // Isolated registry: these tests exercise real dispatch (which beats
            // presence sidecars + instance heartbeats) — they must never write
            // into the developer's real ~/.m1nd/registry.
            registry_dir: Some(runtime.join("registry")),
            ..Default::default()
        };
        let session = crate::server::McpServer::new(config)
            .expect("boot owner")
            .into_session_state();
        let (event_tx, _rx) = broadcast::channel::<SseEvent>(64);
        let tool_schemas_cache = tool_schemas()
            .get("tools")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        let project_brains = Arc::new(crate::project_brains::ProjectBrainRegistry::new(
            runtime.join(crate::project_brains::PROJECT_BRAINS_DIR),
            None,
        ));
        Arc::new(AppState {
            session: Arc::new(BrainSessionCell::new(session)),
            tool_schemas_cache,
            event_tx,
            event_log_path: None,
            registry_dir: None,
            mcp_sessions: crate::mcp_http::new_mcp_session_registry(),
            project_brains,
            runnerd: Arc::new(crate::runnerd_owner::RunnerdRegistry::default()),
            ui_authority: Arc::new(crate::ui_attestation::UiBundleAttestor::default()),
            mission_service: None,
            external_mutation_service: None,
            authority_service: None,
            autonomy_owner: None,
        })
    }

    async fn assert_graph_analysis_does_not_hold_session_lock<F>(
        app: &Arc<AppState>,
        request: F,
    ) -> axum::response::Response
    where
        F: std::future::Future<Output = axum::response::Response> + Send + 'static,
    {
        // Actor bootstrap itself snapshots the graph. Start it before installing
        // the deterministic read-snapshot probe.
        app.project_brains
            .execute_target_m1nd(app.session.clone(), None, true, false, |_state| Ok(()))
            .expect("prestart bound actor");
        // Pause the next actor read after SessionState checkout but before its
        // clone-only snapshot closure. This avoids escaping the actor-owned graph
        // capability and proves the storage mutex is not retained during the
        // potentially blocking graph snapshot/analysis seam.
        let (snapshot_entered_tx, snapshot_entered_rx) = std::sync::mpsc::sync_channel(0);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(0);
        app.project_brains
            .install_read_snapshot_test_hook(snapshot_entered_tx, release_rx);

        let request = tokio::spawn(request);
        // Waits FOR the seam to be reached and returns the instant it is, so the
        // budget only caps a stuck run; two seconds was measuring how long a loaded
        // runner takes to schedule the spawned request.
        snapshot_entered_rx
            .recv_timeout(Duration::from_secs(60))
            .expect("request reached actor snapshot seam");
        assert!(
            !request.is_finished(),
            "request must remain blocked at the actor snapshot probe"
        );
        assert!(
            app.project_brains
                .bound_runtime_health()
                .expect("bound runtime health")
                .is_some(),
            "request must cross the bound actor before graph analysis"
        );
        assert!(
            app.session.storage_mutex_available(),
            "SessionState checkout must release the storage mutex before graph analysis"
        );

        release_tx.send(()).expect("release graph analysis");
        tokio::time::timeout(Duration::from_secs(60), request)
            .await
            .expect("request completed after graph release")
            .expect("request task joined")
    }

    fn seed_system_block_store_for_http_runtime(app: &Arc<AppState>) {
        let mut session = app
            .session
            .lock_mut_before_actor()
            .expect("test setup precedes actor startup");
        let output = dispatch_tool(
            &mut session,
            "system_blocks_seed_import",
            &serde_json::json!({
                "agent_id": "g4-lock-proof",
                "seed_json": include_str!("../../docs/system-blocks/m1nd.seed.v0.json")
            }),
        )
        .expect("seed system block store");
        assert_eq!(output["store_version"], 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn candidate_naming_graph_analysis_releases_session_lock() {
        let temp = tempfile::tempdir().expect("temp runtime");
        let app = rest_owner(temp.path());
        seed_system_block_store_for_http_runtime(&app);
        let request_app = app.clone();
        let target = request_app.session.clone();
        let response = assert_graph_analysis_does_not_hold_session_lock(&app, async move {
            handle_candidate_naming(
                &request_app,
                &target,
                None,
                serde_json::json!({"expected_store_version": 1}),
            )
            .await
        })
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn curation_spawn_graph_analysis_releases_session_lock() {
        let temp = tempfile::tempdir().expect("temp runtime");
        let app = rest_owner(temp.path());
        seed_system_block_store_for_http_runtime(&app);
        let request_app = app.clone();
        let target = request_app.session.clone();
        let response = assert_graph_analysis_does_not_hold_session_lock(&app, async move {
            handle_curation_spawn(
                &request_app,
                &target,
                None,
                serde_json::json!({"expected_store_version": 1}),
            )
            .await
        })
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn subgraph_analysis_releases_session_lock() {
        let temp = tempfile::tempdir().expect("temp runtime");
        let app = rest_owner(temp.path());
        let request_app = app.clone();
        let response = assert_graph_analysis_does_not_hold_session_lock(&app, async move {
            handle_subgraph(
                State(request_app),
                Query(SubgraphQuery {
                    query: "runtime isolation".to_string(),
                    top_k: 20,
                    depth: 2,
                    brain: None,
                }),
            )
            .await
            .into_response()
        })
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn health_returns_cached_degraded_snapshot_while_owner_is_busy() {
        const SAMPLE_COUNT: usize = 601;
        const SAMPLE_INTERVAL: Duration = Duration::from_millis(50);
        const STALL_FLOOR: Duration = Duration::from_secs(30);
        const STALL_WATCHDOG: Duration = Duration::from_secs(45);

        let temp = tempfile::tempdir().expect("temp runtime");
        let app = rest_owner(temp.path());

        let fresh = handle_health(State(app.clone())).await.into_response();
        assert_eq!(fresh.status(), StatusCode::OK);
        let fresh_bytes = axum::body::to_bytes(fresh.into_body(), usize::MAX)
            .await
            .expect("fresh health body");
        let fresh_json: serde_json::Value =
            serde_json::from_slice(&fresh_bytes).expect("fresh health json");
        assert_eq!(fresh_json["project_brain_runtimes"], serde_json::json!([]));

        let held_session = app.session.clone();
        let held_registry = Arc::clone(&app.project_brains);
        let (acquired_tx, acquired_rx) = std::sync::mpsc::sync_channel(0);
        let (sampled_tx, sampled_rx) = std::sync::mpsc::sync_channel(0);
        let (stall_elapsed_tx, stall_elapsed_rx) = std::sync::mpsc::sync_channel(1);
        let holder = std::thread::spawn(move || {
            held_registry
                .execute_target_runtime(held_session, None, true, false, move |_state| {
                    let stall_started = std::time::Instant::now();
                    acquired_tx.send(()).expect("signal acquired actor turn");
                    let _ = sampled_rx.recv_timeout(STALL_WATCHDOG);
                    std::thread::sleep(STALL_FLOOR.saturating_sub(stall_started.elapsed()));
                    stall_elapsed_tx
                        .send(stall_started.elapsed())
                        .expect("report measured actor stall");
                    Ok(())
                })
                .expect("held actor turn");
        });
        acquired_rx.recv().expect("owner actor turn acquired");
        let mut latencies = Vec::with_capacity(SAMPLE_COUNT);
        for sample_index in 0..SAMPLE_COUNT {
            let started = std::time::Instant::now();
            let busy = handle_health(State(app.clone())).await.into_response();
            assert_eq!(busy.status(), StatusCode::OK);
            let busy_bytes = axum::body::to_bytes(busy.into_body(), usize::MAX)
                .await
                .expect("busy health body");
            let busy_json: serde_json::Value =
                serde_json::from_slice(&busy_bytes).expect("busy health json");
            latencies.push(started.elapsed());
            assert_eq!(busy_json["status"], "ok");
            assert_eq!(busy_json["owner_busy"], false);
            assert_eq!(busy_json["snapshot_freshness"], "CACHED_ACTOR_SAFE");
            assert_eq!(busy_json["node_count"], fresh_json["node_count"]);
            assert_eq!(busy_json["edge_count"], fresh_json["edge_count"]);
            assert!(busy_json["bound_brain_runtime"].is_object());
            if sample_index + 1 < SAMPLE_COUNT {
                tokio::time::sleep(SAMPLE_INTERVAL).await;
            }
        }
        assert!(app
            .project_brains
            .bound_runtime_health()
            .expect("bound runtime health")
            .is_some());
        sampled_tx
            .send(())
            .expect("signal health sampling complete");
        holder.join().expect("owner holder joined");
        let measured_stall = stall_elapsed_rx.recv().expect("measured stall duration");
        assert!(
            measured_stall >= STALL_FLOOR,
            "owner stall lasted only {measured_stall:?}"
        );
        assert_eq!(latencies.len(), SAMPLE_COUNT);
        latencies.sort_unstable();
        let p99_rank = (latencies.len() * 99).div_ceil(100);
        let p99 = latencies[p99_rank - 1];
        let max = *latencies.last().expect("health latency samples");
        let threshold_violations = latencies
            .iter()
            .filter(|latency| **latency >= Duration::from_millis(100))
            .count();
        // DO NOT widen these two to appease a slow runner: unlike every other
        // wall-clock budget in this suite, 100ms here is a PRODUCT contract, frozen
        // in docs/M1ND-10-PRD.md ("/health p99 abaixo de 100 ms durante uma operação
        // de 30 s", and the same row in its acceptance matrix). The bound is the
        // guarantee, not the test's tolerance, so a red here is either a real
        // regression or evidence that the runner class cannot host the SLO — a
        // decision for the PRD, not for this assertion.
        assert!(
            p99 < Duration::from_millis(100),
            "busy health p99 was {p99:?} across {SAMPLE_COUNT} samples during a measured {measured_stall:?} owner stall"
        );
        assert_eq!(
            threshold_violations, 0,
            "busy health had {threshold_violations} samples at or above 100ms (max={max:?})"
        );
        eprintln!(
            "R6 cached health proof: samples={SAMPLE_COUNT}, interval={SAMPLE_INTERVAL:?}, measured_stall={measured_stall:?}, p99={p99:?}, max={max:?}, threshold_violations={threshold_violations}, threshold=100ms, watchdog={STALL_WATCHDOG:?}"
        );
    }

    /// Drive the REAL REST `ingest` route; return (status, parsed JSON payload).
    async fn call_ingest(
        app: &Arc<AppState>,
        brain: Option<String>,
        body: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        let resp = handle_tool_call(
            State(app.clone()),
            Path("ingest".to_string()),
            Query(BrainQuery { brain }),
            HeaderMap::new(),
            Bytes::from(serde_json::to_vec(&body).expect("serialize REST test body")),
        )
        .await
        .into_response();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("read body");
        let payload =
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap_or(serde_json::Value::Null);
        (status, payload)
    }

    async fn call_tool(
        app: &Arc<AppState>,
        tool: &str,
        brain: Option<String>,
        body: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        let resp = handle_tool_call(
            State(app.clone()),
            Path(tool.to_string()),
            Query(BrainQuery { brain }),
            HeaderMap::new(),
            Bytes::from(serde_json::to_vec(&body).expect("serialize REST test body")),
        )
        .await
        .into_response();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("read body");
        let payload =
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap_or(serde_json::Value::Null);
        (status, payload)
    }

    #[tokio::test]
    async fn rest_generic_policy_precedes_brain_resolution_and_special_handlers() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = rest_owner(temp.path());
        let missing_brain = Some(
            temp.path()
                .join("brain-that-does-not-exist")
                .to_string_lossy()
                .to_string(),
        );

        for name in [
            "system_blocks_ratify",
            "m1nd.system_blocks_ratify",
            "m1nd_system_blocks_ratify",
        ] {
            let (status, payload) = call_tool(
                &app,
                name,
                missing_brain.clone(),
                serde_json::json!({
                    "agent_id": "attacker",
                    "expected_store_version": 1,
                    "ratifier": "attacker",
                    "ratified_via": "human-ui"
                }),
            )
            .await;
            let rendered = payload.to_string();
            assert_eq!(status, StatusCode::FORBIDDEN, "{rendered}");
            assert!(
                rendered.contains("generic_action_authority_required")
                    && rendered.contains("POSITIVE_SOVEREIGN")
                    && !rendered.contains("unknown_brain"),
                "policy must refuse before resolving a selected brain: {rendered}"
            );
        }

        let (promote_status, promote_payload) = call_tool(
            &app,
            "promote",
            missing_brain,
            serde_json::json!({
                "agent_id": "attacker",
                "brain": temp.path().join("claimed-source").to_string_lossy(),
                "claim": "self-authored-verified",
                "reason": "must not reach promote_claim"
            }),
        )
        .await;
        let rendered = promote_payload.to_string();
        assert_eq!(promote_status, StatusCode::FORBIDDEN, "{rendered}");
        assert!(
            rendered.contains("generic_action_authority_required")
                && rendered.contains("POSITIVE_SOVEREIGN"),
            "REST promote must fail before any promotion handler: {rendered}"
        );

        let (ingest_status, ingest_payload) = call_ingest(
            &app,
            None,
            serde_json::json!({
                "path": temp.path().join("attacker-repo").to_string_lossy(),
                "agent_id": "attacker"
            }),
        )
        .await;
        let rendered = ingest_payload.to_string();
        assert_eq!(ingest_status, StatusCode::FORBIDDEN, "{rendered}");
        assert!(
            rendered.contains("generic_action_authority_required")
                && rendered.contains("POSITIVE_SOVEREIGN"),
            "REST ingest must fail before repository access: {rendered}"
        );
    }

    #[tokio::test]
    async fn rest_typed_mission_service_retains_its_own_transport_boundary() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = rest_owner(temp.path());
        let (status, payload) = call_tool(
            &app,
            "mission_service",
            None,
            serde_json::json!({"action": "execution_started"}),
        )
        .await;
        let rendered = payload.to_string();
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{rendered}");
        assert!(
            rendered.contains("mission_service_unavailable")
                && !rendered.contains("generic_action_authority_required"),
            "typed mission service must not be collapsed into generic policy: {rendered}"
        );
    }

    /// Field defect (project mailbox letter from `opus5-annotate`, high/bug): the
    /// generic floor gate ran BEFORE the owner-proxy interceptions, so the two
    /// REST paths built FOR the Human View v2 spawn and the in-screen "Name with
    /// runner" flow were dead code behind a 403 — both verbs sit at
    /// `SCOPED_GRANT_A2`, so the floor refused before the proxy that exists to
    /// serve them ever saw the request. The proxies are NOT generic dispatch (they
    /// need owner-process state the generic dispatcher does not have), exactly like
    /// `mission_service` above, so they are intercepted AHEAD of the gate — the
    /// same shape `mcp_http::run_mission_service_wire` uses on the wire.
    ///
    /// The proof is the HANDLER's own honest refusal: with no runner daemon
    /// announced and no system-block store in the fixture runtime, each proxy
    /// answers with its own domain refusal instead of the authority floor.
    #[tokio::test]
    async fn rest_owner_proxies_are_intercepted_ahead_of_the_generic_floor_gate() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = rest_owner(temp.path());

        for name in ["mission_spawn", "m1nd.mission_spawn", "m1nd_mission_spawn"] {
            let (status, payload) = call_tool(
                &app,
                name,
                None,
                serde_json::json!({
                    "runner_id": "runner-that-never-announced",
                    "packet_markdown": "# packet",
                    "block_id": "blk-1",
                    "brain_ref": "bound"
                }),
            )
            .await;
            let rendered = payload.to_string();
            assert!(
                !rendered.contains("generic_action_authority_required"),
                "{name} must reach the owner proxy, not the generic floor gate: {status} {rendered}"
            );
            assert_eq!(status, StatusCode::BAD_REQUEST, "{rendered}");
            assert!(
                rendered.contains("no live runner 'runner-that-never-announced'"),
                "{name} must answer with the proxy's own honest refusal: {rendered}"
            );
        }

        for name in [
            "candidate_naming",
            "m1nd.candidate_naming",
            "m1nd_candidate_naming",
        ] {
            let (status, payload) = call_tool(
                &app,
                name,
                None,
                serde_json::json!({ "expected_store_version": 1 }),
            )
            .await;
            let rendered = payload.to_string();
            assert!(
                !rendered.contains("generic_action_authority_required"),
                "{name} must reach the owner proxy, not the generic floor gate: {status} {rendered}"
            );
            assert_eq!(status, StatusCode::BAD_REQUEST, "{rendered}");
            assert!(
                rendered.contains("no system-block store here yet"),
                "{name} must answer with the proxy's own honest refusal: {rendered}"
            );
        }
    }

    /// No widening: the interception is keyed to the two verbs that HAVE an owner
    /// proxy, never to their authority floor. `edit_commit` (`source.edit.commit`)
    /// sits at the same `SCOPED_GRANT_A2` — the REST-seam twin of the spec1 pin
    /// `spec1_5_9_scoped_grant_a2_siblings_keep_todays_refusal_bytes`. Its refusal
    /// bytes are pinned here verbatim so admitting the proxies cannot silently
    /// admit the floor.
    #[tokio::test]
    async fn rest_scoped_grant_a2_siblings_keep_todays_refusal_bytes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = rest_owner(temp.path());

        let (status, payload) = call_tool(
            &app,
            "edit_commit",
            None,
            serde_json::json!({ "agent_id": "attacker", "edit_id": "e-1" }),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{payload}");
        assert_eq!(
            payload["message"].as_str().unwrap_or_default(),
            "invalid params for edit_commit: generic_action_authority_required: \
             semantic_action=source.edit.commit authority_floor=SCOPED_GRANT_A2 \
             cannot use generic REST/MCP dispatch; no exact typed G2/G3 lease \
             consumer is installed for this action",
            "REST-seam A2 sibling refusal bytes moved: {payload}"
        );

        // And the policy function itself is untouched: the two proxy verbs stay
        // refused for every OTHER seam that consults it (the MCP wire, where they
        // are HTTP-only by design — the browser never holds the runner secret).
        for (tool, params) in [
            (
                "mission_spawn",
                serde_json::json!({
                    "runner_id": "r-1",
                    "packet_markdown": "# p",
                    "block_id": "b-1",
                    "brain_ref": "bound"
                }),
            ),
            (
                "candidate_naming",
                serde_json::json!({ "expected_store_version": 1 }),
            ),
        ] {
            let refusal = crate::server::enforce_generic_action_policy(tool, &params)
                .expect_err("the generic policy must still refuse the HTTP-only proxies")
                .to_string();
            assert!(
                refusal.contains("generic_action_authority_required")
                    && refusal.contains("SCOPED_GRANT_A2"),
                "{tool}: {refusal}"
            );
        }
    }

    fn bound_node_count(app: &Arc<AppState>) -> u32 {
        app.project_brains
            .read_target_runtime_snapshot(Arc::clone(&app.session), None, true, |state| {
                Ok(state.graph.read().num_nodes())
            })
            .expect("bound node-count snapshot")
            .value
    }

    /// Seed the bound repo through the owner-internal ingest handler. Public
    /// REST mutation is sovereign-frozen until a typed G2 consumer exists; the
    /// ignored acceptance cases below retain the old REST routing assertions
    /// for reactivation after that consumer lands.
    async fn ingest_bound(app: &Arc<AppState>, bound: &std::path::Path) -> u32 {
        let input: crate::protocol::IngestInput = serde_json::from_value(serde_json::json!({
            "path": bound.to_string_lossy(),
            "agent_id": "owner-test-setup"
        }))
        .expect("owner ingest input");
        let payload = app
            .project_brains
            .execute_target_m1nd(Arc::clone(&app.session), None, true, true, move |session| {
                crate::tools::handle_ingest(session, input)
            })
            .expect("bound ingest actor");
        assert!(
            payload["node_count"].as_u64().unwrap_or(0) > 0,
            "bound ingest must count nodes: {payload}"
        );
        bound_node_count(app)
    }

    #[tokio::test]
    #[ignore = "requires the future exact typed G2 generic mutation consumer"]
    async fn explicit_rest_brain_selector_bypasses_implicit_root_write_gate() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bound = tmp.path().join("m1nd");
        let caller = tmp.path().join("repo-beta");
        write_repo(&bound, "m1nd");
        std::fs::create_dir_all(&caller).expect("caller root");
        let app = rest_owner(&tmp.path().join("runtime"));
        ingest_bound(&app, &bound).await;
        let caller_root = caller.to_string_lossy().to_string();
        let caller_for_actor = caller_root.clone();
        app.project_brains
            .execute_target_runtime(
                Arc::clone(&app.session),
                None,
                true,
                false,
                move |session| {
                    session.caller_root = Some(caller_for_actor);
                    Ok(())
                },
            )
            .expect("seed actor caller root");

        let (status, payload) = call_tool(
            &app,
            "system_blocks_seed_import",
            Some(bound.to_string_lossy().to_string()),
            serde_json::json!({
                "agent_id": "t",
                "seed_json": include_str!("../../docs/system-blocks/m1nd.seed.v0.json")
            }),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::OK,
            "explicit selector must proceed: {payload}"
        );
        assert_ne!(payload["result"]["refused"], "brainless_root");
        let observed_caller = app
            .project_brains
            .read_target_runtime_snapshot(Arc::clone(&app.session), None, true, |session| {
                Ok(session.caller_root.clone())
            })
            .expect("caller-root actor snapshot")
            .value;
        assert_eq!(
            observed_caller.as_deref(),
            Some(caller_root.as_str()),
            "request-scoped selector bypass must restore caller_root"
        );
    }

    /// (1) THE TEST THAT WOULD HAVE CAUGHT THE HOLE. A REST ingest whose
    /// project_root is the PARENT of a repo with an existing project brain must
    /// be REFUSED with the guard's overlap_parent message — and the owner's
    /// bound graph must be EXACTLY as it was (pre-fix, this call dispatched a
    /// plain ingest into the bound graph and replaced its ingest_roots).
    #[tokio::test]
    #[ignore = "requires the future exact typed G2 generic mutation consumer"]
    async fn rest_ingest_parent_project_root_refuses_and_leaves_bound_untouched() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bound = tmp.path().join("bound-repo");
        write_repo(&bound, "boundrepo");
        let app = rest_owner(&tmp.path().join("runtime"));
        let n0 = ingest_bound(&app, &bound).await;

        // An existing project brain for `<tmp>/ws/repo`; the caller opens at the
        // PARENT `<tmp>/ws` (the mother-folder trap).
        let parent = tmp.path().join("ws");
        let child = parent.join("repo");
        std::fs::create_dir_all(&child).expect("mk child");
        let child_key = app
            .project_brains
            .ensure_registered(&child.to_string_lossy())
            .expect("register child brain");

        let (status, payload) = call_ingest(
            &app,
            None,
            serde_json::json!({
                "path": parent.to_string_lossy(),
                "project_root": parent.to_string_lossy(),
                "agent_id": "smoke"
            }),
        )
        .await;

        assert_eq!(
            status,
            StatusCode::BAD_REQUEST,
            "an overlapping REST bootstrap must be an honest 400, got: {payload}"
        );
        assert_eq!(payload["error"], "invalid_params", "class: {payload}");
        let msg = payload["message"].as_str().unwrap_or_default();
        assert!(
            msg.contains("overlap_parent"),
            "the refusal must name the parent class: {msg}"
        );
        assert!(
            msg.contains(&child_key),
            "the refusal must name the conflicting child root: {msg}"
        );
        assert!(
            msg.contains("allow_overlap"),
            "the refusal must teach the escape hatch: {msg}"
        );

        // THE INCIDENT PIN: the bound graph is EXACTLY as it was — same node
        // count, still covering its own repo, never the parent.
        assert_eq!(
            bound_node_count(&app),
            n0,
            "the refused REST bootstrap must not touch the bound graph"
        );
        assert!(
            app.project_brains
                .bound_covers_root(Arc::clone(&app.session), &bound.to_string_lossy())
                .expect("bound coverage snapshot"),
            "the bound graph must still cover its own repo"
        );
        assert!(
            !app.project_brains
                .bound_covers_root(Arc::clone(&app.session), &parent.to_string_lossy())
                .expect("parent coverage snapshot"),
            "the parent root must never have entered the bound ingest_roots"
        );
        assert!(
            !app.project_brains.knows(&parent.to_string_lossy()),
            "no parent brain may exist after the refusal"
        );
    }

    /// (2) PARITY: a disjoint project_root through the REST door mints a real
    /// project brain — the same envelope the JSON-RPC seam returns — and the
    /// bound graph stays isolated. Afterwards, a plain re-ingest of that brain
    /// via `?brain=` (NO project_root) still dispatches normally — the
    /// legitimate re-ingest path must not regress.
    #[tokio::test]
    #[ignore = "requires the future exact typed G2 generic mutation consumer"]
    async fn rest_ingest_disjoint_project_root_bootstraps_a_brain() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bound = tmp.path().join("bound-repo");
        write_repo(&bound, "boundrepo");
        let app = rest_owner(&tmp.path().join("runtime"));
        let n0 = ingest_bound(&app, &bound).await;

        let repo = tmp.path().join("project-y");
        write_repo(&repo, "projecty");
        let (status, payload) = call_ingest(
            &app,
            None,
            serde_json::json!({
                "path": repo.to_string_lossy(),
                "project_root": repo.to_string_lossy(),
                "agent_id": "boot"
            }),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "a disjoint bootstrap must pass: {payload}"
        );
        let packet = &payload["result"];
        assert_eq!(
            packet["schema"], "m1nd-project-brain-bootstrap-v0",
            "REST bootstrap must return the SAME envelope the wire seam does: {payload}"
        );
        assert!(
            packet["ingest"]["node_count"].as_u64().unwrap_or(0) > 0,
            "the bootstrap must carry the project ingest counts: {payload}"
        );
        assert_eq!(
            packet["north"]["schema"], "m1nd-north-packet-v0",
            "the bootstrap must orient the caller in the same response: {payload}"
        );
        assert!(
            app.project_brains.knows(&repo.to_string_lossy()),
            "the registry must know the new brain"
        );
        assert_eq!(
            bound_node_count(&app),
            n0,
            "the bound graph must be untouched by a REST bootstrap"
        );

        // Legitimate re-ingest of the hosted brain via ?brain= (no directive):
        // the classic dispatch, NOT a bootstrap — must stay exactly as before.
        let key =
            crate::project_brains::ProjectBrainRegistry::canonical_key(&repo.to_string_lossy());
        let (status, payload) = call_ingest(
            &app,
            Some(key),
            serde_json::json!({"path": repo.to_string_lossy(), "agent_id": "re"}),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "?brain= re-ingest must pass: {payload}"
        );
        assert!(
            payload["result"]["node_count"].as_u64().unwrap_or(0) > 0,
            "the ?brain= re-ingest is a plain ingest result: {payload}"
        );
        assert!(
            payload["result"]["schema"] != "m1nd-project-brain-bootstrap-v0",
            "a directive-less re-ingest must NOT wear the bootstrap envelope: {payload}"
        );
    }

    /// (3) NO DIRECTIVE, NO CHANGE: a REST ingest WITHOUT project_root keeps the
    /// classic dispatch on the resolved brain (bound when `?brain=` is absent) —
    /// no bootstrap envelope, no project brain minted.
    #[tokio::test]
    #[ignore = "requires the future exact typed G2 generic mutation consumer"]
    async fn rest_ingest_without_project_root_dispatches_on_resolved_brain() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bound = tmp.path().join("bound-repo");
        write_repo(&bound, "boundrepo");
        let app = rest_owner(&tmp.path().join("runtime"));

        let (status, payload) = call_ingest(
            &app,
            None,
            serde_json::json!({"path": bound.to_string_lossy(), "agent_id": "dev"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "plain ingest must pass: {payload}");
        assert!(
            payload["result"]["node_count"].as_u64().unwrap_or(0) > 0,
            "plain ingest must answer with the classic ingest result: {payload}"
        );
        assert!(
            payload["result"]["schema"] != "m1nd-project-brain-bootstrap-v0",
            "a directive-less ingest must never wear the bootstrap envelope: {payload}"
        );
        assert!(
            app.project_brains
                .bound_covers_root(Arc::clone(&app.session), &bound.to_string_lossy())
                .expect("bound coverage snapshot"),
            "the classic ingest must land on the BOUND graph"
        );
        assert_eq!(
            app.project_brains.warm_len(),
            0,
            "no project brain may be minted without the directive"
        );
    }

    /// (4) THE ESCAPE HATCH ON THIS SEAM: allow_overlap:true through the REST
    /// door mints over a detected overlap, exactly like the wire.
    #[tokio::test]
    #[ignore = "requires the future exact typed G2 generic mutation consumer"]
    async fn rest_ingest_allow_overlap_true_mints_anyway() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bound = tmp.path().join("bound-repo");
        write_repo(&bound, "boundrepo");
        let app = rest_owner(&tmp.path().join("runtime"));
        ingest_bound(&app, &bound).await;

        let parent = tmp.path().join("ws");
        write_repo(&parent, "wsparent");
        let child = parent.join("repo");
        std::fs::create_dir_all(&child).expect("mk child");
        app.project_brains
            .ensure_registered(&child.to_string_lossy())
            .expect("register child brain");

        let (status, payload) = call_ingest(
            &app,
            None,
            serde_json::json!({
                "path": parent.to_string_lossy(),
                "project_root": parent.to_string_lossy(),
                "allow_overlap": true,
                "agent_id": "deliberate"
            }),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::OK,
            "allow_overlap:true must mint through the REST door: {payload}"
        );
        assert_eq!(
            payload["result"]["schema"], "m1nd-project-brain-bootstrap-v0",
            "the deliberate mint wears the bootstrap envelope: {payload}"
        );
        assert_eq!(payload["result"]["reused_existing_brain"], false);
        assert!(
            app.project_brains.knows(&parent.to_string_lossy()),
            "the deliberate overlap brain must exist"
        );
    }

    // -----------------------------------------------------------------------
    // P1 — GET /api/presences (the Hall strip's contract endpoint)
    // -----------------------------------------------------------------------

    /// Drive the REAL `/api/presences` handler; return (status, parsed payload).
    async fn call_presences(
        app: &Arc<AppState>,
        brain: Option<String>,
    ) -> (StatusCode, serde_json::Value) {
        let resp = handle_presences(State(app.clone()), Query(BrainQuery { brain }))
            .await
            .into_response();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .expect("read body");
        let payload =
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap_or(serde_json::Value::Null);
        (status, payload)
    }

    /// Seed one presence sidecar straight into the owner's (isolated) registry —
    /// the same file the throttled beat writes. Neutral fixture names only.
    fn seed_presence(
        registry: &std::path::Path,
        agent: &str,
        brain: &str,
        caller_root: Option<&str>,
        mutate: bool,
        last_beat_ms: u64,
    ) {
        let record = crate::presence::PresenceRecord {
            schema: crate::presence::PRESENCE_SCHEMA.to_string(),
            presence_id: crate::presence::stable_presence_id(agent, brain),
            agent_id: agent.to_string(),
            brain: brain.to_string(),
            caller_root: caller_root.map(str::to_string),
            kind: None,
            theme: None,
            worktree: None,
            working_set: Vec::new(),
            task_ref: None,
            mutation: crate::presence::MutationSignal {
                observed_at_ms: mutate.then_some(last_beat_ms),
                declared_intent: None,
            },
            first_seen_ms: last_beat_ms,
            last_beat_ms,
            query_count: 1,
            ttl_ms: crate::presence::PRESENCE_TTL_MS,
        };
        crate::presence::write_presence(registry, &record).expect("seed presence");
    }

    /// The contract's scope semantics: absent `brain` ⇒ the OWNER-WIDE roster
    /// (no served_brain echo); present ⇒ that brain's roster with the §4A.9.4
    /// echo; an unknown root ⇒ an honest 404 (the client degrades to empty);
    /// an expired presence is ABSENT from both scopes (no ghost).
    #[tokio::test]
    async fn presences_endpoint_owner_wide_scoped_and_404() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bound = tmp.path().join("repo-alpha");
        write_repo(&bound, "repoalpha");
        let app = rest_owner(&tmp.path().join("runtime"));
        ingest_bound(&app, &bound).await;

        let identity = http_bound_identity_snapshot(&app).expect("bound identity snapshot");
        let registry = identity.registry_root;
        let bound_key = identity.workspace_root.expect("bound workspace_root");
        let now = crate::util::now_ms();
        // One presence on the bound brain, one on a foreign brain, one GHOST
        // (expired) on the bound brain.
        seed_presence(&registry, "exec-alpha", &bound_key, None, false, now);
        seed_presence(&registry, "exec-beta", "/wt/other-brain", None, false, now);
        seed_presence(
            &registry,
            "exec-ghost",
            &bound_key,
            None,
            false,
            now.saturating_sub(crate::presence::PRESENCE_TTL_MS + 60_000),
        );

        // Owner-wide: both live agents, never the ghost, no served_brain echo.
        let (status, body) = call_presences(&app, None).await;
        assert_eq!(status, StatusCode::OK);
        let agents: Vec<&str> = body["presences"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["agent_id"].as_str().unwrap())
            .collect();
        assert!(
            agents.contains(&"exec-alpha"),
            "owner-wide sees the bound brain: {body}"
        );
        assert!(
            agents.contains(&"exec-beta"),
            "owner-wide sees every brain: {body}"
        );
        assert!(
            !agents.contains(&"exec-ghost"),
            "an expired presence is absent: {body}"
        );
        assert!(
            body.get("served_brain").is_none(),
            "owner-wide carries no served_brain echo: {body}"
        );
        assert!(body["collisions"].is_array(), "collisions always present");

        // Scoped to the bound brain: only its roster + the echo.
        let (status, body) = call_presences(&app, Some(bound.to_string_lossy().to_string())).await;
        assert_eq!(status, StatusCode::OK);
        let scoped: Vec<&str> = body["presences"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["agent_id"].as_str().unwrap())
            .collect();
        assert_eq!(
            scoped,
            vec!["exec-alpha"],
            "scoped roster is this brain only: {body}"
        );
        assert!(
            body["served_brain"]["project_root"].is_string(),
            "scoped response echoes served_brain: {body}"
        );

        // Unknown brain: honest 404 (the client degrades to an empty roster).
        let (status, body) = call_presences(&app, Some("/nowhere/unknown-brain".to_string())).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "unknown brain 404s: {body}");
        assert_eq!(body["error"], "unknown_brain");
    }

    /// The contract's collision + wire shape on the endpoint: two mutating hands
    /// sharing one caller_root produce ONE `same_worktree` collision naming both
    /// agents; and an empty owner serves the honest empty envelope.
    #[tokio::test]
    async fn presences_endpoint_collision_and_honest_empty() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let app = rest_owner(&tmp.path().join("runtime"));
        let registry = http_bound_identity_snapshot(&app)
            .expect("bound identity snapshot")
            .registry_root;

        // Honest empty FIRST (nothing seeded).
        let (status, body) = call_presences(&app, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["presences"], serde_json::json!([]));
        assert_eq!(body["collisions"], serde_json::json!([]));

        // Two mutating hands in ONE worktree on one brain → the collision.
        let now = crate::util::now_ms();
        seed_presence(
            &registry,
            "hand-a",
            "/wt/one-brain",
            Some("/wt/shared"),
            true,
            now,
        );
        seed_presence(
            &registry,
            "hand-b",
            "/wt/one-brain",
            Some("/wt/shared"),
            true,
            now,
        );
        // And the NORMAL shape beside it: a third mutating hand, same brain,
        // its OWN worktree — never part of the warning.
        seed_presence(
            &registry,
            "hand-c",
            "/wt/one-brain",
            Some("/wt/isolated"),
            true,
            now,
        );

        let (status, body) = call_presences(&app, None).await;
        assert_eq!(status, StatusCode::OK);
        let collisions = body["collisions"].as_array().unwrap();
        assert_eq!(collisions.len(), 1, "exactly one colliding pair: {body}");
        assert_eq!(collisions[0]["reason"], "same_worktree");
        assert_eq!(collisions[0]["brain_root"], "/wt/one-brain");
        assert_eq!(collisions[0]["caller_root"], "/wt/shared");
        let ids = collisions[0]["agent_ids"].as_array().unwrap();
        assert!(ids.contains(&serde_json::json!("hand-a")));
        assert!(ids.contains(&serde_json::json!("hand-b")));
        assert!(
            !ids.contains(&serde_json::json!("hand-c")),
            "the isolated worktree hand never joins the warning: {body}"
        );
        // Entry wire shape: caller_root present when it differs from root.
        let a = body["presences"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["agent_id"] == "hand-a")
            .unwrap();
        assert_eq!(a["root"], "/wt/one-brain");
        assert_eq!(a["caller_root"], "/wt/shared");
        assert!(a["mutation"]["observed_at_ms"].is_number());
    }
}
