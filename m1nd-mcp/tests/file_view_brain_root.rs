//! Regression: the Show Code file viewer (`GET /api/file`) must read a member
//! file from the served brain's CODE root, never its raw `workspace_root` — which
//! for a hosted/memory brain is the STORE dir where memory sidecars live, not the
//! repo. A brain whose store is absent or whose workspace points at the sidecar
//! dir must still serve the file that exists on disk (block metadata is
//! enrichment, never a precondition for reading source).
//!
//! THE BUG (field-reported from the UI, 2026-07-12): opening README.md in a block
//! of a freshly-created project brain returned
//! `invalid params for file_view: system-block store I/O error: No such file or
//! directory (os error 2)`. `read_repo_relative_file` was handed the brain's
//! `workspace_root` (a store/agent-memory dir with no README), so the very first
//! `canonicalize` of `<store>/README.md` failed — the same class #326 fixed for
//! `skeleton_candidate`/`reconcile` (resolve `code_root_path()`), on the file_view
//! surface that migration missed.
//!
//! RED before the fix: cases (1) and (2) 404 with the store-I/O error string.
#![cfg(feature = "serve")]

use std::path::Path;
use std::sync::Arc;

use axum::body::Bytes;
use axum::http::HeaderMap;
use parking_lot::Mutex;
use tokio::sync::broadcast;
use tower::ServiceExt;

use m1nd_mcp::http_server::{build_router, resolve_brain, AppState, SseEvent};
use m1nd_mcp::mcp_http::{handle_mcp_post, new_mcp_session_registry};
use m1nd_mcp::project_brains::{ProjectBrainRegistry, DEFAULT_WARM_BRAIN_CAP};
use m1nd_mcp::server::{tool_schemas, McpConfig, McpServer};

// ---------------------------------------------------------------------------
// Fixtures + harness (the http_server integration pattern: a real AppState, the
// real MCP wire for bootstrap, the real router for the route under test).
// ---------------------------------------------------------------------------

/// A tiny repo with a README at its root and one source file.
fn write_repo(root: &Path) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("README.md"), "# game\n\nreal repo readme\n").unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn probe() -> i64 { 7 }\n").unwrap();
}

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
    let session = Arc::new(Mutex::new(server.into_session_state()));
    let (event_tx, _rx) = broadcast::channel::<SseEvent>(64);
    let tool_schemas_cache = tool_schemas()
        .get("tools")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    let project_brains = Arc::new(ProjectBrainRegistry::with_capacity(
        runtime.join("project-brains"),
        Some(runtime.join("registry")),
        DEFAULT_WARM_BRAIN_CAP,
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
    })
}

/// A minimal percent-encoder for the query string (paths in these tests use only
/// safe chars plus `/`, so this keeps the URL faithful without a new dependency).
fn pct(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' | '/' => c.to_string(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}

/// Drive the REAL `GET /api/file` route (optionally with `?brain=`).
async fn get_file(
    app: &Arc<AppState>,
    path: &str,
    brain: Option<&str>,
) -> (u16, serde_json::Value) {
    let router = build_router(app.clone(), false);
    let uri = match brain {
        Some(b) => format!("/api/file?path={}&brain={}", pct(path), pct(b)),
        None => format!("/api/file?path={}", pct(path)),
    };
    let req = axum::http::Request::builder()
        .method("GET")
        .uri(uri)
        .body(axum::body::Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let status = resp.status().as_u16();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body =
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap_or(serde_json::Value::Null);
    (status, body)
}

async fn post_mcp(
    app: &Arc<AppState>,
    session: Option<&str>,
    caller_root: Option<&Path>,
    body: serde_json::Value,
) -> (serde_json::Value, Option<String>) {
    let mut headers = HeaderMap::new();
    if let Some(sid) = session {
        headers.insert("mcp-session-id", sid.parse().unwrap());
    }
    if let Some(root) = caller_root {
        headers.insert("m1nd-caller-root", root.to_string_lossy().parse().unwrap());
    }
    let resp = handle_mcp_post(
        axum::extract::State(app.clone()),
        headers,
        Bytes::from(body.to_string()),
    )
    .await;
    let minted = resp
        .headers()
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let parsed =
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap_or(serde_json::Value::Null);
    (parsed, minted)
}

/// Bootstrap a hosted project brain for `root` (the one-call `ingest project_root`).
async fn bootstrap_brain(app: &Arc<AppState>, root: &Path) {
    let (_b, minted) = post_mcp(
        app,
        None,
        Some(root),
        serde_json::json!({
            "jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}
        }),
    )
    .await;
    let sid = minted.unwrap();
    let (_b, _) = post_mcp(
        app,
        Some(&sid),
        Some(root),
        serde_json::json!({
            "jsonrpc":"2.0","id":7,"method":"tools/call",
            "params":{"name":"ingest","arguments":{
                "path": root.to_string_lossy(),
                "project_root": root.to_string_lossy(),
                "agent_id":"setup"
            }}
        }),
    )
    .await;
}

// ---------------------------------------------------------------------------
// (1) THE REPRO — a served brain whose raw workspace_root is a store/sidecar dir
// (no README) with the real repo in ingest_roots. This is the exact live shape
// (the memory-store workspace #326 documented). file_view must serve the repo's
// README, not 404 on the store dir. RED pre-fix: the store-I/O NotFound.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_view_reads_code_root_when_workspace_is_the_store_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = tmp.path().join("runtime");
    let store = runtime.join("agent-memory");
    std::fs::create_dir_all(&store).unwrap();
    std::fs::write(store.join("note.light.md"), "# a durable memory\n").unwrap();
    let repo = tmp.path().join("game");
    write_repo(&repo);

    let app = mk_app(&runtime);
    {
        let mut s = app.session.lock();
        s.workspace_root = Some(store.to_string_lossy().to_string());
        s.ingest_roots = vec![repo.to_string_lossy().to_string()];
    }

    let (status, body) = get_file(&app, "README.md", None).await;
    assert_eq!(
        status, 200,
        "file_view must resolve the repo (code root), not the store dir — got {status}: {body}"
    );
    assert!(
        body["content"]
            .as_str()
            .unwrap_or_default()
            .contains("real repo readme"),
        "must return the REAL repo README, not a store artifact: {body}"
    );
    // And it must NOT be the store-I/O NotFound the bug produced.
    assert_ne!(
        body["error"], "tool_error",
        "no tool_error expected: {body}"
    );
}

// ---------------------------------------------------------------------------
// (2) THE SERVED-BRAIN PATH — a hosted project brain (?brain=<root>) whose warm
// session wears the store-dir workspace (the live incident's shape: an older
// runtime that never re-homed workspace_root onto the code root). Reading through
// the §4A.9 selector must still serve the repo file. RED pre-fix: store-I/O 404.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_view_via_brain_selector_reads_code_root_not_store_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = tmp.path().join("runtime");
    let repo = tmp.path().join("game");
    write_repo(&repo);

    let app = mk_app(&runtime);
    bootstrap_brain(&app, &repo).await;

    // Force the warm brain into the store-dir workspace shape (what the live,
    // un-re-homed runtime carries): workspace_root = its store dir, repo still in
    // ingest_roots. code_root_path() must recover the repo.
    let brain_ref = repo.to_string_lossy().to_string();
    {
        let (session, _echo) = resolve_brain(&app, Some(&brain_ref)).expect("brain resolves");
        let store_dir = { session.lock().runtime_root.clone() };
        let mut s = session.lock();
        s.workspace_root = Some(store_dir.to_string_lossy().to_string());
        assert!(
            !s.ingest_roots.is_empty(),
            "the bootstrapped brain must keep the repo in ingest_roots"
        );
    }

    let (status, body) = get_file(&app, "README.md", Some(&brain_ref)).await;
    assert_eq!(
        status, 200,
        "served-brain file_view must read the code root through the selector — got {status}: {body}"
    );
    assert!(
        body["content"]
            .as_str()
            .unwrap_or_default()
            .contains("real repo readme"),
        "served brain must return the repo README: {body}"
    );
}

// ---------------------------------------------------------------------------
// (3) A normal hosted brain (workspace_root == code root) keeps working — the fix
// must not regress the healthy path.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_view_healthy_project_brain_still_reads_the_file() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = tmp.path().join("runtime");
    let repo = tmp.path().join("game");
    write_repo(&repo);

    let app = mk_app(&runtime);
    bootstrap_brain(&app, &repo).await;

    let brain_ref = repo.to_string_lossy().to_string();
    let (status, body) = get_file(&app, "README.md", Some(&brain_ref)).await;
    assert_eq!(
        status, 200,
        "healthy brain must still serve the file: {body}"
    );
    assert!(
        body["content"]
            .as_str()
            .unwrap_or_default()
            .contains("real repo readme"),
        "healthy brain content: {body}"
    );
}

// ---------------------------------------------------------------------------
// (4) SECURITY UNCHANGED — path traversal is still refused (400), never a read of
// a file outside the repo, even under the code-root resolution.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_view_still_refuses_path_traversal() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = tmp.path().join("runtime");
    let repo = tmp.path().join("game");
    write_repo(&repo);
    // A secret one level ABOVE the repo the traversal would try to reach.
    std::fs::write(tmp.path().join("secret.txt"), "TOP SECRET\n").unwrap();

    let app = mk_app(&runtime);
    bootstrap_brain(&app, &repo).await;
    let brain_ref = repo.to_string_lossy().to_string();

    let (status, body) = get_file(&app, "../secret.txt", Some(&brain_ref)).await;
    assert_eq!(
        status, 400,
        "a `..` escape must be a 400 client error, never a read: {body}"
    );
    assert!(
        !body["content"]
            .as_str()
            .unwrap_or_default()
            .contains("TOP SECRET"),
        "traversal must never leak an out-of-repo file: {body}"
    );
}

// ---------------------------------------------------------------------------
// (5) UNKNOWN BRAIN — an honest miss (404), never a filesystem probe of the raw
// `?brain=` path.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_view_unknown_brain_is_an_honest_miss() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = tmp.path().join("runtime");
    let repo = tmp.path().join("game");
    write_repo(&repo);

    let app = mk_app(&runtime);
    let ghost = tmp.path().join("no-such-brain");
    std::fs::create_dir_all(&ghost).unwrap();
    std::fs::write(ghost.join("README.md"), "should never be read\n").unwrap();

    let (status, body) = get_file(&app, "README.md", Some(&ghost.to_string_lossy())).await;
    assert_eq!(status, 404, "an unknown brain must 404: {body}");
    assert!(
        !body["content"]
            .as_str()
            .unwrap_or_default()
            .contains("should never be read"),
        "an unknown ?brain= must never be probed as a filesystem path: {body}"
    );
}
