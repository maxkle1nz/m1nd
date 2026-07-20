#![cfg(feature = "serve")]

use crate as m1nd_mcp;

use std::path::Path;
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use m1nd_control::{AuthorityFreshness, AuthorityStatus};
use m1nd_mcp::brain_runtime::BrainSessionCell;
use m1nd_mcp::http_server::{build_router, AppState, SseEvent};
use m1nd_mcp::mcp_http::new_mcp_session_registry;
use m1nd_mcp::project_brains::ProjectBrainRegistry;
use m1nd_mcp::server::{tool_schemas, McpConfig, McpServer};
use m1nd_mcp::ui_attestation::{
    UiBundleAttestor, UI_MODE_DEBUG_FILESYSTEM, UI_MODE_DEVELOPMENT_DIST, UI_MODE_EMBEDDED,
    UI_MODE_EXTERNAL_DIR,
};
use m1nd_mcp::ui_bundle_support::{
    stable_ui_tree_identity_with_hook, ui_tree_identity, StableUiTreeError, UI_PLACEHOLDER_MARKER,
};
use tokio::sync::broadcast;
use tower::ServiceExt;

fn app(runtime: &Path, ui_dir: &Path) -> Arc<AppState> {
    std::fs::create_dir_all(runtime).unwrap();
    let config = McpConfig {
        graph_source: runtime.join("graph_snapshot.json"),
        plasticity_state: runtime.join("plasticity_state.json"),
        runtime_dir: Some(runtime.to_path_buf()),
        registry_dir: Some(runtime.join("registry")),
        ..Default::default()
    };
    let session = Arc::new(BrainSessionCell::new(
        McpServer::new(config)
            .expect("boot owner")
            .into_session_state(),
    ));
    let (event_tx, _rx) = broadcast::channel::<SseEvent>(16);
    let tool_schemas_cache = tool_schemas()
        .get("tools")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    Arc::new(AppState {
        session,
        tool_schemas_cache,
        event_tx,
        event_log_path: None,
        registry_dir: Some(runtime.join("registry")),
        mcp_sessions: new_mcp_session_registry(),
        project_brains: Arc::new(ProjectBrainRegistry::new(
            runtime.join("project-brains"),
            Some(runtime.join("registry")),
        )),
        runnerd: Arc::new(m1nd_mcp::runnerd_owner::RunnerdRegistry::default()),
        ui_authority: Arc::new(UiBundleAttestor::for_http(
            false,
            Some(ui_dir.to_path_buf()),
        )),
        mission_service: None,
        external_mutation_service: None,
        authority_service: None,
        autonomy_owner: None,
    })
}

async fn get(router: &axum::Router, uri: &str) -> (StatusCode, Vec<u8>) {
    let response = router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    (status, body)
}

#[tokio::test]
async fn external_ui_manifest_tracks_the_exact_tree_the_router_serves() {
    let temp = tempfile::tempdir().unwrap();
    let ui_dir = temp.path().join("external-dist");
    std::fs::create_dir_all(&ui_dir).unwrap();
    std::fs::write(temp.path().join("package.json"), r#"{"version":"9.9.9"}"#).unwrap();
    std::fs::write(ui_dir.join("index.html"), b"served-v1").unwrap();
    let app = app(&temp.path().join("runtime"), &ui_dir);
    let router = build_router(app, true);

    let (status, served) = get(&router, "/index.html").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(served, b"served-v1");

    let (status, body) = get(&router, "/api/manifest").await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let first: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let first_identity = ui_tree_identity(&ui_dir).unwrap();
    let first_digest = format!("sha256:{}", first_identity.sha256);
    assert_eq!(first["manifest"]["ui"]["mode"], UI_MODE_EXTERNAL_DIR);
    assert_eq!(first["manifest"]["ui"]["bundle_sha256"], first_digest);
    assert_eq!(
        first["manifest"]["authorities"]["ui_bundle"]["digest"],
        first_digest
    );
    assert_eq!(
        first["manifest"]["authorities"]["ui_bundle"]["status"],
        "DRIFT"
    );

    std::fs::write(ui_dir.join("index.html"), b"served-v2-changed").unwrap();
    let (status, served) = get(&router, "/index.html").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(served, b"served-v2-changed");

    let (status, body) = get(&router, "/api/manifest").await;
    assert_eq!(status, StatusCode::OK, "{}", String::from_utf8_lossy(&body));
    let second: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let second_identity = ui_tree_identity(&ui_dir).unwrap();
    let second_digest = format!("sha256:{}", second_identity.sha256);
    assert_ne!(second_digest, first_digest);
    assert_eq!(second["manifest"]["ui"]["bundle_sha256"], second_digest);
    assert_eq!(
        second["manifest"]["authorities"]["ui_bundle"]["digest"],
        second_digest
    );
    assert_eq!(
        second["manifest"]["authorities"]["ui_bundle"]["status"],
        "DRIFT"
    );
}

#[test]
fn placeholder_tree_is_never_a_fresh_ui_authority() {
    let temp = tempfile::tempdir().unwrap();
    let ui_dir = temp.path().join("dist");
    std::fs::create_dir_all(&ui_dir).unwrap();
    std::fs::write(ui_dir.join("index.html"), UI_PLACEHOLDER_MARKER).unwrap();
    let observation = UiBundleAttestor::for_http(false, Some(ui_dir))
        .observe()
        .unwrap();
    assert_eq!(observation.status, AuthorityStatus::Degraded);
    assert_eq!(observation.freshness, AuthorityFreshness::Unknown);
    assert!(!observation.bundle_sha256.is_empty());
}

#[test]
fn a_tree_change_between_digest_passes_is_explicit_instability() {
    let temp = tempfile::tempdir().unwrap();
    let index = temp.path().join("index.html");
    std::fs::write(&index, b"before").unwrap();
    let error = stable_ui_tree_identity_with_hook(temp.path(), || {
        std::fs::write(&index, b"after").unwrap();
    })
    .expect_err("two different served trees cannot yield one fresh digest");
    assert!(matches!(error, StableUiTreeError::Unstable { .. }));
}

#[test]
fn serving_mode_and_attestation_source_are_bound_together() {
    let default = UiBundleAttestor::for_http(false, None);
    if cfg!(debug_assertions) {
        assert_eq!(default.mode(), UI_MODE_DEBUG_FILESYSTEM);
        assert!(default.observes_filesystem());
    } else {
        assert_eq!(default.mode(), UI_MODE_EMBEDDED);
        assert!(!default.observes_filesystem());
    }
    assert!(default.serve_dir().is_none());

    let dev = UiBundleAttestor::for_http(true, None);
    assert_eq!(dev.mode(), UI_MODE_DEVELOPMENT_DIST);
    assert!(dev.observes_filesystem());
    assert!(dev.serve_dir().is_some());

    let external = UiBundleAttestor::for_http(false, Some(Path::new("/tmp/ui").to_path_buf()));
    assert_eq!(external.mode(), UI_MODE_EXTERNAL_DIR);
    assert!(external.observes_filesystem());
    assert_eq!(external.serve_dir().as_deref(), Some(Path::new("/tmp/ui")));
}
