//! Response compression (gzip) is wired into the HTTP router.
//!
//! The `/api/graph/snapshot` route serves tens of MB of node/edge JSON and the
//! served UI bundle is hundreds of KB — both dominate the wire cost. A
//! `tower_http::compression::CompressionLayer` on the router negotiates gzip from
//! the client's `Accept-Encoding`. These tests prove the layer is live on the
//! heavy route AND that it stays honest: no `Accept-Encoding` ⇒ identity (never a
//! blanket re-encode), and the negotiated case carries `content-encoding: gzip`.
#![cfg(feature = "serve")]

use crate as m1nd_mcp;

use std::path::Path;
use std::sync::Arc;

use m1nd_mcp::brain_runtime::BrainSessionCell;
use tokio::sync::broadcast;
use tower::ServiceExt;

use m1nd_mcp::http_server::{build_router, AppState, SseEvent};
use m1nd_mcp::mcp_http::new_mcp_session_registry;
use m1nd_mcp::project_brains::ProjectBrainRegistry;
use m1nd_mcp::server::{tool_schemas, McpConfig, McpServer};

fn mk_app(runtime: &Path) -> Arc<AppState> {
    std::fs::create_dir_all(runtime).unwrap();
    let config = McpConfig {
        graph_source: runtime.join("graph_snapshot.json"),
        plasticity_state: runtime.join("plasticity_state.json"),
        runtime_dir: Some(runtime.to_path_buf()),
        registry_dir: Some(runtime.join("registry")),
        ..Default::default()
    };
    let server = McpServer::new(config).expect("boot owner");
    let session = Arc::new(BrainSessionCell::new(server.into_session_state()));
    let (event_tx, _rx) = broadcast::channel::<SseEvent>(64);
    let tool_schemas_cache = tool_schemas()
        .get("tools")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    let project_brains = Arc::new(ProjectBrainRegistry::with_capacity(
        runtime.join("project-brains"),
        Some(runtime.join("registry")),
        4,
    ));
    Arc::new(AppState {
        session,
        tool_schemas_cache,
        event_tx,
        event_log_path: None,
        registry_dir: Some(runtime.join("registry")),
        mcp_sessions: new_mcp_session_registry(),
        project_brains,
        runnerd: Arc::new(m1nd_mcp::runnerd_owner::RunnerdRegistry::default()),
        ui_authority: Arc::new(m1nd_mcp::ui_attestation::UiBundleAttestor::default()),
        mission_service: None,
        external_mutation_service: None,
        authority_service: None,
        autonomy_owner: None,
    })
}

/// Drive `GET /api/graph/snapshot` through the REAL router with a caller-chosen
/// `Accept-Encoding`, returning `(status, content_encoding_header)`.
async fn snapshot_content_encoding(
    app: &Arc<AppState>,
    accept_encoding: Option<&str>,
) -> (u16, Option<String>) {
    let router = build_router(app.clone(), false);
    let mut builder = axum::http::Request::builder()
        .method("GET")
        .uri("/api/graph/snapshot");
    if let Some(value) = accept_encoding {
        builder = builder.header(axum::http::header::ACCEPT_ENCODING, value);
    }
    let req = builder.body(axum::body::Body::empty()).unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let content_encoding = resp
        .headers()
        .get(axum::http::header::CONTENT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    (status, content_encoding)
}

/// A client that advertises gzip gets a gzip-encoded snapshot: the compression
/// layer is live on the heaviest read route.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn graph_snapshot_is_gzip_encoded_when_accepted() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = tmp.path().join("runtime");
    let app = mk_app(&runtime);

    let (status, content_encoding) = snapshot_content_encoding(&app, Some("gzip")).await;
    assert_eq!(status, 200, "snapshot route should serve the bound owner");
    assert_eq!(
        content_encoding.as_deref(),
        Some("gzip"),
        "a gzip-capable client must receive a gzip-encoded snapshot",
    );
}

/// A client that advertises NO encoding gets the identity body — compression is
/// negotiated, never forced (so plain `curl` and non-gzip clients still work).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn graph_snapshot_is_identity_without_accept_encoding() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = tmp.path().join("runtime");
    let app = mk_app(&runtime);

    let (status, content_encoding) = snapshot_content_encoding(&app, None).await;
    assert_eq!(status, 200, "snapshot route should serve the bound owner");
    assert_eq!(
        content_encoding, None,
        "without Accept-Encoding the snapshot must be sent identity (uncompressed)",
    );
}
