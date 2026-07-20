//! Per-brain Open — the REST `?brain=` selector (HUMAN-LAYER-PRD §4A.9, slice 2H).
//!
//! Founder question, verbatim: *"por que project-b não consigo dar Open?"*
//!
//! The browser surface was bound-graph-only: `/api/graph/*` and `/api/tools/*`
//! carried no brain selector, so a hosted project brain (the project-b card) could be
//! LISTED but not ENTERED. This slice adds the `?brain=<project_root>` selector
//! that reuses the wire's routing (`ProjectBrainRegistry`, #260) so the Hall can
//! open ANY project brain in-tab.
//!
//! These tests drive the ONE resolution the HTTP handlers wrap
//! (`http_server::resolve_brain`) against a REAL two-brain owner built through the
//! same wire seam the two-tier tests use — the same altitude `instances_listing`
//! is tested at, no HTTP client needed. The contract, point by point:
//!
//!   RED (today's gap, now GREEN): a hosted root resolves to the HOSTED brain, not
//!        the bound one — the snapshot?brain=<hosted> ≠ bound snapshot.
//!   (1) two-brain owner: resolve(hosted) ≠ resolve(bound); served_brain echoes B.
//!   (2) unknown root → honest error naming the miss, nothing created on disk.
//!   (3) absent param = bound graph, byte-compatible (same session, bound echo).
//!   (4) warm-boot via REST: a fresh owner (dormant store on disk) resolves
//!       `?brain=B` by lazily loading the store — no prior routed call needed.
//!   (5) tools/* respect the selector — the SAME resolution serves the tool route,
//!       so a seek on brain B would see B's nodes only (INV-15/16 cross-check: the
//!       resolved session's graph is B's, not the bound one's).
//!   (6) the disk-union bug fix: after a restart, `instances_listing` lists the
//!       hosted brain from its manifest with ZERO routed calls (counts from disk).
#![cfg(feature = "serve")]

use crate as m1nd_mcp;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use m1nd_mcp::brain_runtime::BrainSessionCell;
use tokio::sync::broadcast;
use tower::ServiceExt;

use m1nd_mcp::http_server::{build_router, instances_listing, resolve_brain, AppState, SseEvent};
use m1nd_mcp::mcp_http::new_mcp_session_registry;
use m1nd_mcp::project_brains::ProjectBrainRegistry;
use m1nd_mcp::server::{tool_schemas, McpConfig, McpServer};

// ---------------------------------------------------------------------------
// Fixtures — a bound "dev" repo and a hosted project repo, distinct basenames
// and DISTINCT graph sizes so "snapshot differs" is provable by node count.
// ---------------------------------------------------------------------------

fn write_bound_repo(root: &Path) {
    std::fs::create_dir_all(root.join("src")).expect("mk src");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"boundgraph\"\nversion = \"0.0.0\"\n",
    )
    .expect("Cargo.toml");
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn bound_root_fn() -> i64 { 1 }\npub struct BoundThing { pub v: i64 }\n",
    )
    .expect("lib.rs");
}

/// The hosted repo is deliberately BIGGER (more symbols) than the bound one, so a
/// mismatched payload is caught by a bare node-count compare — no name collision.
fn write_project_repo(root: &Path) {
    std::fs::create_dir_all(root.join("src")).expect("mk src");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"projectb\"\nversion = \"0.0.0\"\n",
    )
    .expect("Cargo.toml");
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn probe_a() -> i64 { 1 }\n\
         pub fn probe_b() -> i64 { 2 }\n\
         pub fn probe_c() -> i64 { 3 }\n\
         pub struct ProbeOne { pub v: i64 }\n\
         pub struct ProbeTwo { pub v: i64 }\n\
         pub struct ProbeThree { pub v: i64 }\n\
         pub enum ProbeKind { A, B, C }\n",
    )
    .expect("lib.rs");
}

// ---------------------------------------------------------------------------
// Owner harness — a real AppState, driven through the real wire handler.
// ---------------------------------------------------------------------------

struct Owner {
    app: Arc<AppState>,
}

fn mk_owner_with_bound_root(runtime: &Path, bound_root: &Path, ingest_bound: bool) -> Owner {
    std::fs::create_dir_all(runtime).expect("mk runtime");
    let config = McpConfig {
        graph_source: runtime.join("graph_snapshot.json"),
        plasticity_state: runtime.join("plasticity_state.json"),
        runtime_dir: Some(runtime.to_path_buf()),
        registry_dir: Some(runtime.join("registry")),
        ..Default::default()
    };
    let server = McpServer::new(config).expect("boot owner");
    let mut session_state = server.into_session_state();
    let canonical_bound = ProjectBrainRegistry::canonical_key(&bound_root.to_string_lossy());
    session_state.workspace_root = Some(canonical_bound.clone());
    session_state.caller_root = Some(canonical_bound.clone());
    if ingest_bound {
        let ingest = m1nd_mcp::server::dispatch_tool(
            &mut session_state,
            "ingest",
            &serde_json::json!({"path": bound_root.to_string_lossy(), "agent_id": "setup"}),
        )
        .expect("preseed bound graph before actor startup");
        assert!(
            ingest["node_count"].as_u64().unwrap_or(0) > 0,
            "bound fixture ingest must produce nodes: {ingest}"
        );
        // `ingest` records the caller spelling of its path. On macOS a tempfile
        // under `/var` canonicalizes to `/private/var`; a real owner restart must
        // keep one canonical bound identity instead of making the fixture change
        // actor ids merely because this setup call restored the alias spelling.
        session_state.workspace_root = Some(canonical_bound.clone());
        session_state.caller_root = Some(canonical_bound);
    }
    let session = Arc::new(BrainSessionCell::new(session_state));
    let (event_tx, _rx) = broadcast::channel::<SseEvent>(64);
    let tool_schemas_cache = tool_schemas()
        .get("tools")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    let project_brains = Arc::new(ProjectBrainRegistry::new(
        runtime.join("project-brains"),
        Some(runtime.join("registry")),
    ));
    Owner {
        app: Arc::new(AppState {
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
        }),
    }
}

impl Owner {
    /// Drive the real REST tool route so test dispatch crosses the selected
    /// brain's actor instead of borrowing SessionState through a compatibility
    /// guard. `None` selects the bound brain; `Some(root)` exercises `?brain=`.
    async fn rest_tool(
        &self,
        brain: Option<&Path>,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let query = brain
            .map(|root| format!("?brain={}", pct(&root.to_string_lossy())))
            .unwrap_or_default();
        let request = axum::http::Request::builder()
            .method("POST")
            .uri(format!("/api/tools/{name}{query}"))
            .header("content-type", "application/json")
            .body(axum::body::Body::from(args.to_string()))
            .expect("REST tool request");
        let response = build_router(self.app.clone(), false)
            .oneshot(request)
            .await
            .map_err(|error| error.to_string())?;
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(|error| error.to_string())?;
        let body: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!("REST tool {name} returned {status}: {body}"));
        }
        Ok(body["result"].clone())
    }

    async fn graph_node_count(&self, brain: Option<&Path>) -> usize {
        let query = brain
            .map(|root| format!("?brain={}", pct(&root.to_string_lossy())))
            .unwrap_or_default();
        let request = axum::http::Request::builder()
            .method("GET")
            .uri(format!("/api/graph/stats{query}"))
            .body(axum::body::Body::empty())
            .expect("graph stats request");
        let response = build_router(self.app.clone(), false)
            .oneshot(request)
            .await
            .expect("graph stats route");
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("graph stats body");
        let body: serde_json::Value =
            serde_json::from_slice(&bytes).expect("graph stats JSON response");
        assert!(status.is_success(), "graph stats returned {status}: {body}");
        body["node_count"].as_u64().expect("graph stats node_count") as usize
    }
}

fn pct(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' | '/' => {
                character.to_string()
            }
            _ => format!("%{:02X}", character as u32),
        })
        .collect()
}

fn canon(p: &Path) -> String {
    p.canonicalize()
        .unwrap_or_else(|_| p.to_path_buf())
        .to_string_lossy()
        .to_string()
}

/// An owner whose bound graph is `bound-repo`, plus a hosted `project-repo` brain
/// bootstrapped through the one-call `ingest {project_root}` path.
async fn owner_with_two_brains(tmp: &Path) -> (Owner, PathBuf, PathBuf) {
    let bound_repo = tmp.join("bound-repo");
    write_bound_repo(&bound_repo);
    let owner = mk_owner_with_bound_root(&tmp.join("runtime"), &bound_repo, true);

    let project_repo = tmp.join("project-repo");
    write_project_repo(&project_repo);
    let (_brain, ingest, _reused) = owner
        .app
        .project_brains
        .bootstrap(
            &project_repo.to_string_lossy(),
            &serde_json::json!({
                "path": project_repo.to_string_lossy(),
                "project_root": project_repo.to_string_lossy(),
                "agent_id": "project-b-agent"
            }),
        )
        .expect("preseed project brain through owner registry bootstrap");
    assert!(
        ingest["node_count"].as_u64().unwrap_or(0) > 0,
        "hosted fixture bootstrap must ingest: {ingest}"
    );

    (owner, bound_repo, project_repo)
}

// ---------------------------------------------------------------------------
// (1) The selector resolves per brain — hosted ≠ bound, echo names B.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn brain_selector_resolves_hosted_distinct_from_bound() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, bound_repo, project_repo) = owner_with_two_brains(tmp.path()).await;

    // Absent param → the bound graph (byte-compatible).
    let (_bound_sess, bound_echo) = resolve_brain(&owner.app, None).expect("bound resolves");
    let bound_n = owner.graph_node_count(None).await;

    // ?brain=<project_root> → the HOSTED brain, a DIFFERENT graph.
    let (_hosted_sess, hosted_echo) =
        resolve_brain(&owner.app, Some(&project_repo.to_string_lossy())).expect("hosted resolves");
    let hosted_n = owner.graph_node_count(Some(&project_repo)).await;

    // THE RED→GREEN CORE: the hosted snapshot is not the bound snapshot. Before
    // this slice, ?brain= was ignored and both would be the bound count.
    assert_ne!(
        bound_n, hosted_n,
        "snapshot?brain=<hosted> must differ from the bound snapshot \
         (bound={bound_n}, hosted={hosted_n})"
    );
    assert!(
        hosted_n > bound_n,
        "the hosted repo is bigger by construction: bound={bound_n} hosted={hosted_n}"
    );

    // The served_brain echo names the brain that actually answered (INV-15 hinge).
    assert_eq!(
        Path::new(bound_echo["project_root"].as_str().unwrap_or(""))
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        canon(&bound_repo),
        "the bound echo names the bound repo: {bound_echo}"
    );
    assert_eq!(
        Path::new(hosted_echo["project_root"].as_str().unwrap_or(""))
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        canon(&project_repo),
        "the hosted echo names the hosted repo: {hosted_echo}"
    );
    assert_eq!(
        hosted_echo["display_name"].as_str(),
        Some("project-repo"),
        "the hosted echo's display_name is the project basename: {hosted_echo}"
    );
}

// ---------------------------------------------------------------------------
// (2) Unknown root → honest error, NOTHING created on disk.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_brain_root_refused_and_creates_nothing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, _bound, _proj) = owner_with_two_brains(tmp.path()).await;

    let bogus = tmp.path().join("no-such-repo");
    let before: Vec<_> = std::fs::read_dir(tmp.path().join("runtime").join("project-brains"))
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();

    // `expect_err` needs the Ok type to be Debug (SessionState is not), so match.
    let msg = match resolve_brain(&owner.app, Some(&bogus.to_string_lossy())) {
        Ok(_) => panic!("an unknown root must be refused, not resolved"),
        Err(e) => e.to_string(),
    };
    assert!(
        msg.contains("no-such-repo") || msg.contains(&bogus.to_string_lossy().to_string()),
        "the error must NAME the missing root: {msg}"
    );
    assert!(
        msg.contains("Hall lists what exists") || msg.contains("consent"),
        "the error must teach that browsing never creates a brain: {msg}"
    );

    // Nothing was created: the project-brains dir is unchanged, no store for the
    // bogus root, and the bogus dir itself was never read into existence.
    let after: Vec<_> = std::fs::read_dir(tmp.path().join("runtime").join("project-brains"))
        .map(|rd| rd.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    assert_eq!(
        before.len(),
        after.len(),
        "resolving an unknown root must create NO new store: before={before:?} after={after:?}"
    );
    assert!(
        !owner.app.project_brains.knows(&bogus.to_string_lossy()),
        "the registry must still not know the bogus root"
    );
}

// ---------------------------------------------------------------------------
// (3) Absent param = bound graph, byte-compatible (same Arc, bound echo).
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn absent_param_is_the_bound_graph() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, _bound, _proj) = owner_with_two_brains(tmp.path()).await;

    let (absent_sess, _e1) = resolve_brain(&owner.app, None).expect("absent resolves");
    // An empty string is treated as absent too (a bare `?brain=` in the URL).
    let (empty_sess, _e2) = resolve_brain(&owner.app, Some("")).expect("empty resolves as bound");

    // Both must be the SAME Arc as the bound session — not a copy, THE bound graph.
    assert!(
        Arc::ptr_eq(&absent_sess, &owner.app.session),
        "absent ?brain= must resolve to the bound session itself"
    );
    assert!(
        Arc::ptr_eq(&empty_sess, &owner.app.session),
        "a bare/empty ?brain= must resolve to the bound session itself"
    );
}

// ---------------------------------------------------------------------------
// (3b) Naming the bound root explicitly also routes to the bound graph.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn naming_the_bound_root_routes_to_bound() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, bound_repo, _proj) = owner_with_two_brains(tmp.path()).await;

    let (sess, echo) =
        resolve_brain(&owner.app, Some(&bound_repo.to_string_lossy())).expect("bound-by-name");
    assert!(
        Arc::ptr_eq(&sess, &owner.app.session),
        "naming the bound root must resolve to the bound session (no double-routing)"
    );
    assert_eq!(
        Path::new(echo["project_root"].as_str().unwrap_or(""))
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        canon(&bound_repo),
        "the echo names the bound repo: {echo}"
    );
}

// ---------------------------------------------------------------------------
// (4) Warm-boot via REST — a fresh owner lazily loads the store on ?brain=B.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn warm_boots_dormant_store_on_first_selector() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = tmp.path().join("runtime");
    let (owner, bound_repo, project_repo) = owner_with_two_brains(tmp.path()).await;
    // Capture the hosted count while warm, then "restart" the owner (new AppState,
    // empty brain map) — the project brain is now DORMANT on disk.
    let (warm_sess, _e) =
        resolve_brain(&owner.app, Some(&project_repo.to_string_lossy())).expect("warm resolves");
    let warm_n = owner.graph_node_count(Some(&project_repo)).await;
    let shutdown_acks = owner
        .app
        .project_brains
        .shutdown(std::time::Duration::from_secs(3))
        .expect("graceful owner shutdown checkpoints and releases hosted brains");
    assert!(
        !shutdown_acks.is_empty(),
        "every initialized hosted actor must acknowledge the restart checkpoint"
    );
    drop(warm_sess);
    drop(owner);

    let owner2 = mk_owner_with_bound_root(&runtime, &bound_repo, false);
    // The brain is NOT in owner2's map yet. Resolving ?brain=B must warm-boot it
    // from its store — no prior routed call, lazy load on first selector.
    let (_booted_sess, echo) = resolve_brain(&owner2.app, Some(&project_repo.to_string_lossy()))
        .expect("a dormant store must warm-boot on the first REST selector");
    let booted_n = owner2.graph_node_count(Some(&project_repo)).await;
    assert_eq!(
        booted_n, warm_n,
        "the warm-booted brain must carry its full graph (warm={warm_n}, booted={booted_n})"
    );
    assert_eq!(
        Path::new(echo["project_root"].as_str().unwrap_or(""))
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        canon(&project_repo),
        "the echo still names the hosted repo after warm-boot: {echo}"
    );
}

// ---------------------------------------------------------------------------
// (5) tools/* respect the selector — the tool route dispatches on the SAME
//     resolved session, so a tool on brain B sees B's graph, not the bound one.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tool_route_dispatches_against_the_selected_brain() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, _bound_repo, project_repo) = owner_with_two_brains(tmp.path()).await;

    // The tool route resolves the brain then dispatches against THAT session. Prove
    // the resolution is the hosted graph: dispatch `health` on the resolved session
    // and confirm its node_count is the HOSTED count, not the bound count.
    let bound_health = owner
        .rest_tool(None, "health", serde_json::json!({"agent_id": "t"}))
        .await
        .expect("bound health through REST actor route");
    let hosted_health = owner
        .rest_tool(
            Some(&project_repo),
            "health",
            serde_json::json!({"agent_id": "t"}),
        )
        .await
        .expect("hosted health through REST actor route");
    let bn = bound_health["node_count"].as_u64().unwrap_or(0);
    let hn = hosted_health["node_count"].as_u64().unwrap_or(0);
    assert_ne!(
        bn, hn,
        "a tool on brain B must see B's graph (INV-16): bound={bn} hosted={hn}"
    );
    assert!(hn > bn, "hosted graph is bigger: bound={bn} hosted={hn}");
}

// ---------------------------------------------------------------------------
// (6) The disk-union fix — after a restart, the Hall lists the hosted brain from
//     its manifest with ZERO routed calls (the "project-b vanished" reincidence).
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hall_lists_dormant_project_brain_from_disk_after_restart() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = tmp.path().join("runtime");
    let (owner, bound_repo, project_repo) = owner_with_two_brains(tmp.path()).await;

    // Warm listing has the hosted brain (it was just bootstrapped, still warm).
    let warm = instances_listing(&owner.app);
    let warm_hosted = warm["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["brain_kind"] == "project")
        .expect("warm listing has the project brain")
        .clone();
    let warm_n = warm_hosted["node_count"].as_u64();
    drop(owner);

    // "Restart" the owner: brand-new AppState, EMPTY brain map. Faithful to a real
    // process restart, the project brain's instance-lease entry does NOT survive
    // (its owning process died; the phonebook entry it wrote is gone) — only its
    // durable `project_brain.json` manifest remains on disk. Delete the project's
    // registry entry to model that: now ONLY the disk manifest can surface it.
    // Before the fix, the Hall would show only the bound brain here (the field-
    // proven "project-b sumiu" reincidence: post-kickstart instances=['m1nd']).
    let instances_dir = runtime.join("registry").join("instances");
    if let Ok(rd) = std::fs::read_dir(&instances_dir) {
        for e in rd.flatten() {
            let txt = std::fs::read_to_string(e.path()).unwrap_or_default();
            let v: serde_json::Value =
                serde_json::from_str(&txt).unwrap_or(serde_json::Value::Null);
            if v["brain_kind"] == "project" {
                std::fs::remove_file(e.path()).expect("drop the dead project lease entry");
            }
        }
    }
    let owner2 = mk_owner_with_bound_root(&runtime, &bound_repo, false);
    let cold = instances_listing(&owner2.app);
    let cold_list = cold["instances"].as_array().expect("instances array");

    let hosted = cold_list
        .iter()
        .find(|b| {
            b["project_root"]
                .as_str()
                .map(|r| {
                    Path::new(r)
                        .canonicalize()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default()
                        == canon(&project_repo)
                })
                .unwrap_or(false)
        })
        .unwrap_or_else(|| {
            panic!(
                "a dormant project brain must be listed from disk with zero routed calls: {cold}"
            )
        });

    assert_eq!(
        hosted["brain_kind"], "project",
        "the disk-listed brain is a project brain: {hosted}"
    );
    assert_eq!(
        hosted["display_name"].as_str(),
        Some("project-repo"),
        "the disk-listed brain wears its project basename, not a fingerprint: {hosted}"
    );
    assert_eq!(
        hosted["node_count"].as_u64(),
        warm_n,
        "the disk-listed brain reports its manifest-recorded counts (no warm-boot): {hosted}"
    );
    assert!(
        hosted["last_activity_ms"].as_u64().is_some(),
        "a disk-listed brain carries a freshness stamp from its manifest: {hosted}"
    );

    // And it is OPENABLE: resolving ?brain=<its root> against the fresh owner
    // warm-boots it — a listable brain is an openable brain.
    let (_sess, _echo) = resolve_brain(&owner2.app, Some(&project_repo.to_string_lossy()))
        .expect("a disk-listed brain must be openable via ?brain=");
    assert!(
        owner2.graph_node_count(Some(&project_repo)).await > 0,
        "the opened brain has its graph after warm-boot"
    );
}

// ---------------------------------------------------------------------------
// (7) Mission-control PERSISTENCE on the hosted ?brain= path — the invariant the
//     field report (2026-07-13T03:20/03:30) doubted: "cartas de mission-control
//     aceitas via REST ?brain=<root> NUNCA chegam a disco — vivem em memória de um
//     runtime de instance hospedado e morrem no restart."
//
//     The claim does NOT reproduce, and this test is the standing PROOF (a
//     regression guard, not a RED→GREEN fix): `handle_mission_start` calls
//     `save_mission` BEFORE it returns the ack (mission_handlers.rs), and a hosted
//     brain's `runtime_root` IS its durable store dir (project_brains.rs
//     `boot_store`, `runtime_dir: Some(store)`). So a charter accepted on the
//     hosted path is a real file at `<store>/mission-control/<msn>.json` the instant
//     mission_start returns, and it warm-boots back after an owner restart.
//
//     Field corroboration this encodes: an unrelated hosted project brain (a
//     different repo) had its own `msn_*` cards persisted normally under its
//     `project-brains/<fp>/mission-control/` — exactly the path this test asserts.
//     Were `save_mission` ever regressed to an in-memory-only record (the imagined
//     bug), BOTH the ack-time `is_file()` and the post-restart reload would fail.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn charter_survives_owner_restart_on_the_hosted_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = tmp.path().join("runtime");
    let (owner, bound_repo, project_repo) = owner_with_two_brains(tmp.path()).await;

    // The hosted store's canonical mission-control dir — the ONE durable home a
    // charter for this root must land in ("um root = um mission-control canônico").
    let key = ProjectBrainRegistry::canonical_key(&project_repo.to_string_lossy());
    let store_mc = owner
        .app
        .project_brains
        .store_dir_for(&key)
        .join("mission-control");

    // 1. Open a charter on the HOSTED brain through the ?brain= seam (`resolve_brain`,
    //    the ONE resolution `handle_tool_call` wraps), then dispatch mission_start on
    //    that resolved session — the exact hosted path the field report names.
    let mission_id = {
        let out = owner
            .rest_tool(
                Some(&project_repo),
                "mission_start",
                serde_json::json!({
                    "agent_id": "charter-hand",
                    "repo": project_repo.to_string_lossy(),
                    "task": "prove a hosted charter survives an owner restart",
                    "mode": "bug_hunt",
                    "budget": "normal",
                    "risk": "medium"
                }),
            )
            .await
            .expect("mission_start on the hosted brain actor");
        out["mission_id"]
            .as_str()
            .expect("mission_start returns a mission_id")
            .to_string()
    };

    // ACK-TIME DURABILITY: the card is a real file the instant mission_start returned
    // (save_mission runs BEFORE the ack) — never an in-memory-only record.
    let card = store_mc.join(format!("{mission_id}.json"));
    assert!(
        card.is_file(),
        "a charter accepted on the hosted ?brain= path must be on disk at its store's \
         mission-control BEFORE the ack (looked at {card:?})"
    );

    // 2. A progress event — the lifecycle write path (mission_event → save_mission)
    //    also persists, so the card on disk carries the event across the restart.
    owner
        .rest_tool(
            Some(&project_repo),
            "mission_event",
            serde_json::json!({
                "agent_id": "charter-hand",
                "mission_id": mission_id,
                "event": "file_read",
                "payload": {"path": "src/lib.rs"}
            }),
        )
        .await
        .expect("mission_event on the hosted brain actor");

    // 3. THE RESTART: drop the owner (every warm brain evicted from memory), then
    //    boot a brand-new owner on the SAME runtime — the project brain is now
    //    DORMANT on disk, faithful to a `launchctl kickstart` of the served owner.
    drop(owner);
    let owner2 = mk_owner_with_bound_root(&runtime, &bound_repo, false);

    // 4. Re-open the hosted brain via ?brain= (warm-boot from its store) and RELOAD
    //    the charter through the real seam: a mission_event on the SAME id must load
    //    the card from disk. Had the charter "died in memory" (the field-report
    //    hypothesis), `load_mission` would fail with "could not be loaded".
    let reload = owner2
        .rest_tool(
            Some(&project_repo),
            "mission_event",
            serde_json::json!({
                "agent_id": "charter-hand",
                "mission_id": mission_id,
                "event": "file_read",
                "payload": {"path": "src/lib.rs (after restart)"}
            }),
        )
        .await
        .expect(
            "after an owner restart, the hosted charter must reload from disk — the seam must \
             not report it lost",
        );
    assert_eq!(
        reload["event_count"].as_u64(),
        Some(2),
        "the reloaded charter carries its pre-restart event (count 2 = pre + post): {reload}"
    );

    // 5. Still the SAME single canonical file — one root, one mission-control.
    assert!(
        card.is_file(),
        "the charter file survived the restart on disk at its canonical mission-control: {card:?}"
    );
}
