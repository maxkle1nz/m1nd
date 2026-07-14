//! F30 — `GET /api/universe` (the Universe landing surface's read-only aggregate).
//!
//! THE HARD LAW (RED-first, `docs/HUMAN-VIEW-V2-F30-UNIVERSE.md` §3): serving
//! `/api/universe` is SIDECAR-ONLY and must NEVER hydrate a project brain — it must
//! never insert into `ProjectBrainRegistry.brains`. The panorama is composed purely
//! from on-disk manifests + the presence dir + each world's mission box / SystemBlock
//! store, plus the OWNER's own already-resident daemon alerts. A dormant/evicted brain
//! (on disk, never warm-booted) is served entirely from its `project_brain.json`.
//!
//! RED before the route exists: the request 404s, so the world-shape assertions fail.
//! The anti-hydration assertions (`warm_len` byte-identical, `warm_counts` absent) hold
//! at RED too — the endpoint that does not exist cannot hydrate — which is exactly the
//! point: implementing the route must KEEP them holding.
#![cfg(feature = "serve")]

use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use tokio::sync::broadcast;
use tower::ServiceExt;

use m1nd_mcp::http_server::{build_router, AppState, SseEvent};
use m1nd_mcp::mcp_http::new_mcp_session_registry;
use m1nd_mcp::project_brains::ProjectBrainRegistry;
use m1nd_mcp::server::{tool_schemas, McpConfig, McpServer};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
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
    })
}

/// Write a dormant project-brain store on disk (manifest with counts + freshness),
/// exactly like a real bootstrap's `write_manifest` output — but WITHOUT ever
/// booting the brain (so it is never in the warm map). Returns the canonical root
/// (== the manifest's `project_root`, the world key the endpoint reports).
fn write_dormant_brain(
    app: &Arc<AppState>,
    project_root: &Path,
    node_count: u64,
    edge_count: u64,
    updated_ms: u64,
) -> String {
    // A real repo dir so the root canonicalizes to a stable spelling (macOS
    // /tmp -> /private/tmp) — the same key the manifest, the presence beat, and
    // the box path all agree on.
    std::fs::create_dir_all(project_root).unwrap();
    let canonical = ProjectBrainRegistry::canonical_key(&project_root.to_string_lossy());
    let store_dir = app.project_brains.store_dir_for(&canonical);
    std::fs::create_dir_all(&store_dir).unwrap();
    let manifest = serde_json::json!({
        "schema": "m1nd-project-brain-v0",
        "project_root": canonical,
        "brain_kind": "project",
        "created_ms": 1_700_000_000_000u64,
        "node_count": node_count,
        "edge_count": edge_count,
        "updated_ms": updated_ms,
    });
    // MANIFEST_FILE is "project_brain.json" (project_brains.rs).
    std::fs::write(
        store_dir.join("project_brain.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();
    canonical
}

/// Drop one live P1 presence sidecar bound to `brain` (a raw `m1nd-presence-v0`
/// record — the exact on-disk shape `presence::list_live` reads). No brain is
/// hydrated: a presence is witness tissue in the shared registry dir.
fn write_presence(registry_root: &Path, agent: &str, brain: &str, now: u64) {
    let dir = registry_root.join("presences");
    std::fs::create_dir_all(&dir).unwrap();
    // stable_presence_id is hash-based; any unique, stable file name works for read.
    let pid = format!("prs_{}_{}", agent, brain.len());
    let record = serde_json::json!({
        "schema": "m1nd-presence-v0",
        "presence_id": pid,
        "agent_id": agent,
        "brain": brain,
        "first_seen_ms": now.saturating_sub(60_000),
        "last_beat_ms": now,
        "query_count": 3u64,
        "ttl_ms": 120_000u64,
    });
    std::fs::write(
        dir.join(format!("{pid}.json")),
        serde_json::to_string_pretty(&record).unwrap(),
    )
    .unwrap();
}

/// Append one `merge_wait` mission letter to a world's box
/// (`<project_root>/.m1nd/inbox.jsonl`) — the JSONL envelope `read_letters` parses
/// and `heads_by_mission` walks into a head. A merge_wait head carries a gate.
fn write_merge_wait_letter(project_root: &str, mission_id: &str) {
    let box_path = Path::new(project_root).join(".m1nd").join("inbox.jsonl");
    std::fs::create_dir_all(box_path.parent().unwrap()).unwrap();
    let letter = serde_json::json!({
        "kind": "mission",
        "agent": "hand-1",
        "ts": "2026-07-14T10:00:00Z",
        "mission": {
            "schema": "m1nd-mission-letter-v0",
            "mission_id": mission_id,
            "mission_seq": 1,
            "prev_letter_id": null,
            "block_id": "sb_probe",
            "brain_ref": "world",
            "seat": "hand",
            "capability": "build-runner",
            "phase": "merge_wait",
            "gate": { "command": "cargo test", "exit_status": 0, "artifact_hash": "sha256:deadbeef" },
            "packet_ref": "sha256:packet",
            "tokens_total": 0,
            "started_at": "2026-07-14T09:00:00Z",
            "updated_at": "2026-07-14T10:00:00Z"
        }
    });
    let line = serde_json::to_string(&letter).unwrap();
    std::fs::write(&box_path, format!("{line}\n")).unwrap();
}

/// Drive the REAL `GET /api/universe` route through the router.
async fn get_universe(app: &Arc<AppState>) -> (u16, serde_json::Value) {
    let router = build_router(app.clone(), false);
    let req = axum::http::Request::builder()
        .method("GET")
        .uri("/api/universe")
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

fn world_by_root<'a>(body: &'a serde_json::Value, root: &str) -> Option<&'a serde_json::Value> {
    body["worlds"]
        .as_array()?
        .iter()
        .find(|w| w["root"].as_str() == Some(root))
}

// ---------------------------------------------------------------------------
// (1) THE HARD LAW — serving the panorama never hydrates a dormant brain, and
// serves it purely from its manifest (counts + freshness echoed honestly).
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn universe_serves_dormant_brain_without_hydration() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = tmp.path().join("runtime");
    let app = mk_app(&runtime);

    let repo = tmp.path().join("world-a");
    let updated = now_ms().saturating_sub(3_600_000); // an hour stale — shown, not hidden
    let root = write_dormant_brain(&app, &repo, 128, 256, updated);

    // Precondition: NOTHING warm. The manifest is on disk; the brain is dormant.
    assert_eq!(
        app.project_brains.warm_len(),
        0,
        "precondition: no project brain is hydrated"
    );

    let (status, body) = get_universe(&app).await;

    // Shape: the dormant world appears, served purely from its manifest.
    assert_eq!(status, 200, "the universe read must be 200: {body}");
    let w = world_by_root(&body, &root)
        .unwrap_or_else(|| panic!("the dormant world must appear in the panorama: {body}"));
    assert_eq!(
        w["node_count"],
        serde_json::json!(128u64),
        "manifest node_count echoed: {w}"
    );
    assert_eq!(
        w["edge_count"],
        serde_json::json!(256u64),
        "manifest edge_count echoed: {w}"
    );
    assert_eq!(
        w["updated_ms"],
        serde_json::json!(updated),
        "manifest freshness echoed verbatim (stale age shown, never a fabricated 'live'): {w}"
    );
    assert_eq!(
        w["name"],
        serde_json::json!("world-a"),
        "world name is the repo basename: {w}"
    );

    // THE HARD LAW: serving the panorama inserted NOTHING into the warm map, and did
    // NOT resolve the dormant brain (byte-identical warm map before/after).
    assert_eq!(
        app.project_brains.warm_len(),
        0,
        "serving /api/universe must NEVER hydrate a project brain (warm map unchanged)"
    );
    assert!(
        app.project_brains.warm_counts(&root).is_none(),
        "the dormant brain must not have been resolved into the warm map by the read"
    );
}

// ---------------------------------------------------------------------------
// (2) THE AGGREGATION — presences group per world, merge_wait letters become
// stamps, and the totals sum honestly across worlds + the owner.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn universe_aggregates_presences_stamps_and_totals_per_world() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = tmp.path().join("runtime");
    let app = mk_app(&runtime);
    let now = now_ms();

    let repo_a = tmp.path().join("world-a");
    let repo_b = tmp.path().join("world-b");
    let root_a = write_dormant_brain(&app, &repo_a, 100, 200, now);
    let root_b = write_dormant_brain(&app, &repo_b, 10, 20, now);

    // A live presence on world-a only; a merge_wait receipt on world-a only.
    let registry_root = { app.session.lock().instance.registry_root() };
    write_presence(&registry_root, "atlas", &root_a, now);
    write_merge_wait_letter(&root_a, "msn_0000000000a1");

    let (status, body) = get_universe(&app).await;
    assert_eq!(status, 200, "200: {body}");

    let a = world_by_root(&body, &root_a).expect("world-a present");
    let b = world_by_root(&body, &root_b).expect("world-b present");

    // world-a: one satellite, awake, one stamp.
    assert_eq!(
        a["presences"].as_array().map(|v| v.len()),
        Some(1),
        "world-a carries its one live presence (grouped by brain): {a}"
    );
    assert_eq!(a["awake"], serde_json::json!(true), "world-a is awake: {a}");
    assert_eq!(
        a["pending"]["stamps"],
        serde_json::json!(1u64),
        "one merge_wait stamp on world-a: {a}"
    );
    assert_eq!(
        a["letters"]["merge_wait"],
        serde_json::json!(1u64),
        "letters.merge_wait echoes the head: {a}"
    );

    // world-b: no presence, not awake, zero pending — an honest zero, never a ghost.
    assert_eq!(
        b["presences"].as_array().map(|v| v.len()),
        Some(0),
        "world-b has no live presence: {b}"
    );
    assert_eq!(
        b["awake"],
        serde_json::json!(false),
        "world-b is not awake: {b}"
    );
    assert_eq!(
        b["pending"]["stamps"],
        serde_json::json!(0u64),
        "world-b has no stamp: {b}"
    );

    // totals: 2 worlds, 1 awake, 1 pending gesture across the universe.
    assert_eq!(
        body["totals"]["worlds"],
        serde_json::json!(2u64),
        "two worlds: {body}"
    );
    assert_eq!(
        body["totals"]["awake"],
        serde_json::json!(1u64),
        "one awake: {body}"
    );
    assert_eq!(
        body["totals"]["pending"],
        serde_json::json!(1u64),
        "one pending gesture universe-wide: {body}"
    );

    // owner scope is present (alerts live on the owner, never a world) — zero here.
    assert_eq!(
        body["owner"]["alerts_pending"],
        serde_json::json!(0u64),
        "owner alert scope present and honest-zero: {body}"
    );

    // The HARD LAW still holds under the fuller read.
    assert_eq!(
        app.project_brains.warm_len(),
        0,
        "no hydration under aggregation"
    );
}
