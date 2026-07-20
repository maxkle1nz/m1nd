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

use crate as m1nd_mcp;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use m1nd_control::{ActionId, AuthorityVariant, Effect, Ingress};
use m1nd_mcp::brain_runtime::BrainSessionCell;
use tokio::sync::broadcast;
use tower::ServiceExt;

use m1nd_mcp::http_server::{build_router, AppState, SseEvent};
use m1nd_mcp::mcp_http::new_mcp_session_registry;
use m1nd_mcp::project_brains::{ProjectBrainRegistry, DEFAULT_WARM_BRAIN_CAP};
use m1nd_mcp::runtime_jobs::{
    RuntimeJobAuthorityBindingV1, RuntimeJobBindingV1, RuntimeJobFailure, RuntimeJobRequestV1,
    RuntimeJobState, RuntimeJobSuccess, RuntimeJobWait, RUNTIME_JOB_AUTHORITY_SCHEMA,
    RUNTIME_JOB_BINDING_SCHEMA,
};
use m1nd_mcp::server::{tool_schemas, McpConfig, McpServer};
use m1nd_mcp::session::SessionState;

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

fn mk_app_with_setup(runtime: &Path, setup: impl FnOnce(&mut SessionState)) -> Arc<AppState> {
    std::fs::create_dir_all(runtime).unwrap();
    let config = McpConfig {
        graph_source: runtime.join("graph_snapshot.json"),
        plasticity_state: runtime.join("plasticity_state.json"),
        runtime_dir: Some(runtime.to_path_buf()),
        registry_dir: Some(runtime.join("registry")),
        ..Default::default()
    };
    let server = McpServer::new(config).expect("boot owner");
    let mut session_state = server.into_session_state();
    setup(&mut session_state);
    let session = Arc::new(BrainSessionCell::new(session_state));
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
        ui_authority: Arc::new(m1nd_mcp::ui_attestation::UiBundleAttestor::default()),
        mission_service: None,
        external_mutation_service: None,
        authority_service: None,
        autonomy_owner: None,
    })
}

fn mk_app(runtime: &Path) -> Arc<AppState> {
    mk_app_with_setup(runtime, |_| {})
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis()
        .try_into()
        .expect("milliseconds fit")
}

/// Test-only fixture mutation through the same hosted-brain actor used by
/// production dispatch. This deliberately models a legacy warm session whose
/// workspace points at its store without reopening raw mutable SessionState.
fn set_hosted_workspace_root(
    registry: &ProjectBrainRegistry,
    project_root: &Path,
    workspace_root: PathBuf,
) {
    let project_root = project_root.to_string_lossy().to_string();
    let revision = registry
        .read_runtime_snapshot(&project_root, |_state| Ok::<_, RuntimeJobFailure>(()))
        .expect("read hosted brain version through actor")
        .version
        .revision;
    let job_id = "file-view-legacy-workspace-fixture";
    let request = RuntimeJobRequestV1 {
        job_id: job_id.to_string(),
        idempotency_key: format!("idem-{job_id}"),
        binding: RuntimeJobBindingV1 {
            schema: RUNTIME_JOB_BINDING_SCHEMA.to_string(),
            organism_id: "organism-file-view-test".to_string(),
            brain_id: registry.brain_id_for(&project_root),
            mission_id: "mission-file-view-test".to_string(),
            agent_id: "agent-file-view-test".to_string(),
            action: ActionId::new("graph.background-mutation").expect("fixture action"),
            ingress: Ingress::BackgroundJob,
            effects: BTreeSet::from([Effect::GraphMutation, Effect::RuntimeStoreWrite]),
            authority: RuntimeJobAuthorityBindingV1 {
                schema: RUNTIME_JOB_AUTHORITY_SCHEMA.to_string(),
                decision_id: "decision-file-view-fixture".to_string(),
                authority_variant: AuthorityVariant::Policy,
                authority_epoch: 1,
                autonomy_epoch: 0,
                capability_id: None,
                authorization_digest: "a".repeat(64),
            },
        },
        snapshot_revision: revision,
        deadline_unix_ms: now_ms() + 10_000,
    };
    let submitted = registry
        .submit_runtime_job(
            &project_root,
            request,
            |_state| Ok::<_, RuntimeJobFailure>(()),
            move |context, _snapshot| {
                context.checkpoint()?;
                Ok::<_, RuntimeJobFailure>(workspace_root)
            },
            |state, workspace_root| {
                state.workspace_root = Some(workspace_root.to_string_lossy().to_string());
                state.bump_graph_generation();
                Ok(RuntimeJobSuccess::new(
                    "fixture_applied",
                    "legacy store-dir workspace applied through actor",
                ))
            },
        )
        .expect("submit hosted fixture mutation");
    let jobs = registry
        .runtime_job_registry()
        .expect("runtime job registry");
    match jobs
        .wait_terminal(&submitted, Duration::from_secs(5))
        .expect("wait for hosted fixture mutation")
    {
        RuntimeJobWait::Terminal(job) => assert_eq!(
            job.state,
            RuntimeJobState::Succeeded,
            "hosted fixture mutation must succeed: {job:?}"
        ),
        RuntimeJobWait::ObservableNonTerminal(job) => {
            panic!("hosted fixture mutation did not finish: {job:?}")
        }
    }
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

/// Bootstrap a hosted project brain for `root` (the one-call `ingest project_root`).
async fn bootstrap_brain(app: &Arc<AppState>, root: &Path) {
    // Fixture setup crosses the registry's canonical bootstrap implementation;
    // the public generic ingest route correctly remains A2 fail-closed.
    let (_brain, ingest, _reused) = app
        .project_brains
        .bootstrap(
            &root.to_string_lossy(),
            &serde_json::json!({
                "path": root.to_string_lossy(),
                "project_root": root.to_string_lossy(),
                "agent_id":"setup"
            }),
        )
        .expect("bootstrap hosted fixture through owner registry");
    assert!(
        ingest["node_count"].as_u64().unwrap_or(0) > 0,
        "hosted fixture bootstrap must ingest: {ingest}"
    );
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

    let app = mk_app_with_setup(&runtime, |s| {
        s.workspace_root = Some(store.to_string_lossy().to_string());
        s.ingest_roots = vec![repo.to_string_lossy().to_string()];
    });

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
    let hosted = app
        .project_brains
        .read_runtime_snapshot(&brain_ref, |state| {
            Ok::<_, RuntimeJobFailure>((state.runtime_root.clone(), state.ingest_roots.clone()))
        })
        .expect("read bootstrapped brain through actor")
        .value;
    assert!(
        !hosted.1.is_empty(),
        "the bootstrapped brain must keep the repo in ingest_roots"
    );
    set_hosted_workspace_root(&app.project_brains, &repo, hosted.0);

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
