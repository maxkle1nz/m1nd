//! Two-Tier Brain (interim variant) — per-project brains hosted INSIDE the served
//! owner, one-call bootstrap, silent caller_root routing.
//!
//! THE FRICTION (field-reported, the mission's RED): the served owner is bound to
//! ONE graph (the m1nd dev graph). An agent whose session is rooted in ANOTHER
//! repo (project-b, project-d — `field-reports.jsonl` 2026-07-03/04) either gets the
//! WRONG graph's answers, or — if it follows today's reception option and calls
//! `ingest path=<its repo>` — it REPLACES the bound graph: the owner's single
//! `SessionState` is clobbered and the m1nd dev brain is destroyed for everyone.
//! Max's verdict: "enquanto não resolvermos isso o sistema não tá funcional".
//!
//! THE FIX (TWO-TIER-BRAIN-PRD §9.5.5 interim, owner-hosted variant): the owner
//! manages MULTIPLE graphs — the bound one (untouched) plus N per-project brains
//! stored under `<owner runtime_root>/project-brains/<fingerprint(root)>/`. ONE
//! call (`ingest` with `project_root=<repo>`) creates + ingests + binds + orients
//! a project brain; thereafter the caller's resolved root (hop-2
//! `M1nd-Caller-Root`) routes to that brain SILENTLY (TT-INV-12), including brand
//! NEW sessions from the same root — zero wasted calls.
//!
//! These tests drive the REAL Streamable-HTTP MCP handler (`handle_mcp_post`)
//! in-process — the exact seam attach bridges hit — and are RED before the
//! implementation:
//!   (1) bootstrap isolation: `ingest {path: B, project_root: B}` from root B must
//!       create brain B and leave the bound graph EXACTLY as it was (node counts +
//!       ingest_roots + no B-files in bound focus). Pre-fix: the call replaces the
//!       bound graph (the clobber, pinned).
//!   (2) same-session stickiness: after bootstrap, the SAME wire session's calls
//!       answer from brain B.
//!   (3) new-session silent routing: a NEW wire session whose caller_root = B
//!       binds brain B with NO reception block (silence is legal on match).
//!   (4) bound callers keep the dev graph exactly as before.
//!   (5) persistence: a fresh owner process (same runtime dir) warm-boots brain B
//!       from its store snapshot on first touch.
//!   (6) the reception mismatch block's `ingest_your_repo` option carries the REAL
//!       one-call invocation (`project_root=`), not a roadmap promise.
#![cfg(feature = "serve")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Bytes;
use axum::http::HeaderMap;
use parking_lot::Mutex;
use tokio::sync::broadcast;

use m1nd_mcp::http_server::{AppState, SseEvent};
use m1nd_mcp::mcp_http::{handle_mcp_post, new_mcp_session_registry};
use m1nd_mcp::project_brains::ProjectBrainRegistry;
use m1nd_mcp::server::{tool_schemas, McpConfig, McpServer};

// ---------------------------------------------------------------------------
// Fixture repos — tiny, deterministic, DISTINCT sentinels per repo.
// ---------------------------------------------------------------------------

/// The bound "dev" repo: 3 files / several fns so its node_count is clearly
/// distinct from the project repo's.
fn write_bound_repo(root: &Path) {
    std::fs::create_dir_all(root.join("src")).expect("mk src");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"boundgraph\"\nversion = \"0.0.0\"\n",
    )
    .expect("Cargo.toml");
    std::fs::write(
        root.join("src/gravity.rs"),
        "pub fn bound_gravity_anchor() -> i64 { 9 }\npub fn bound_gravity_pull() -> i64 { 8 }\n",
    )
    .expect("gravity.rs");
    std::fs::write(
        root.join("src/tides.rs"),
        "pub fn bound_tide_rise() -> i64 { 7 }\npub struct BoundTideTable { pub v: i64 }\n",
    )
    .expect("tides.rs");
    std::fs::write(
        root.join("src/lib.rs"),
        "pub mod gravity;\npub mod tides;\npub fn bound_root_fn() -> i64 { 1 }\n",
    )
    .expect("lib.rs");
}

/// The OTHER project ("repo B" — the project-b stand-in): 1 file, unique sentinel.
fn write_project_b_repo(root: &Path) {
    std::fs::create_dir_all(root.join("src")).expect("mk src");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"projectb\"\nversion = \"0.0.0\"\n",
    )
    .expect("Cargo.toml");
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn project_b_probe() -> i64 { 42 }\npub struct ProjectBOrbit { pub v: i64 }\n",
    )
    .expect("lib.rs");
}

// ---------------------------------------------------------------------------
// Owner harness — a real AppState around a real SessionState, driven through
// the real `handle_mcp_post` (the wire seam attach bridges use).
// ---------------------------------------------------------------------------

struct Owner {
    app: Arc<AppState>,
}

/// Build an owner whose bound graph lives under `runtime` (graph_snapshot.json
/// etc.), exactly like `--serve` boots: `McpServer::new` warm-boots the snapshot
/// when present, else starts fresh.
fn mk_owner(runtime: &Path) -> Owner {
    std::fs::create_dir_all(runtime).expect("mk runtime");
    let config = McpConfig {
        graph_source: runtime.join("graph_snapshot.json"),
        plasticity_state: runtime.join("plasticity_state.json"),
        runtime_dir: Some(runtime.to_path_buf()),
        registry_dir: Some(runtime.join("registry")),
        ..Default::default()
    };
    let server = McpServer::new(config.clone()).expect("boot owner");
    let session = Arc::new(Mutex::new(server.into_session_state()));
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
        }),
    }
}

impl Owner {
    /// POST one JSON-RPC message to `/mcp`; returns (parsed body, minted session id).
    async fn post(
        &self,
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
            axum::extract::State(self.app.clone()),
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
            .expect("read body");
        let parsed =
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap_or(serde_json::Value::Null);
        (parsed, minted)
    }

    /// `initialize` a wire session whose resolved caller root is `caller_root`.
    async fn init_session(&self, caller_root: &Path) -> String {
        let (_body, minted) = self
            .post(
                None,
                Some(caller_root),
                serde_json::json!({
                    "jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": {
                        "protocolVersion": "2025-06-18",
                        "capabilities": {},
                        "clientInfo": {"name": "two-tier-probe", "version": "0"}
                    }
                }),
            )
            .await;
        minted.expect("initialize must mint a session id")
    }

    /// tools/call returning the tool's parsed JSON payload (content[0].text).
    async fn tool(
        &self,
        sid: &str,
        caller_root: &Path,
        name: &str,
        args: serde_json::Value,
    ) -> serde_json::Value {
        let (body, _) = self
            .post(
                Some(sid),
                Some(caller_root),
                serde_json::json!({
                    "jsonrpc": "2.0", "id": 7, "method": "tools/call",
                    "params": {"name": name, "arguments": args}
                }),
            )
            .await;
        let text = body["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("tool {name} returned no content text: {body}"));
        serde_json::from_str(text)
            .unwrap_or_else(|e| panic!("tool {name} content is not JSON ({e}): {text}"))
    }

    /// node_count as seen by a given session/root (via `health`).
    async fn node_count(&self, sid: &str, caller_root: &Path) -> u64 {
        let health = self
            .tool(
                sid,
                caller_root,
                "health",
                serde_json::json!({"agent_id": "probe"}),
            )
            .await;
        health["node_count"]
            .as_u64()
            .unwrap_or_else(|| panic!("health without node_count: {health}"))
    }
}

/// The `binding.fingerprint` block from a `north` packet.
fn fingerprint(north: &serde_json::Value) -> &serde_json::Value {
    &north["binding"]["fingerprint"]
}

fn ingest_roots_of(north: &serde_json::Value) -> Vec<String> {
    fingerprint(north)["ingest_roots"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Canonicalized string form (macOS /tmp → /private/tmp), for path comparisons.
fn canon(p: &Path) -> String {
    p.canonicalize()
        .unwrap_or_else(|_| p.to_path_buf())
        .to_string_lossy()
        .to_string()
}

/// Path equality across alias forms (`/var/...` vs `/private/var/...`): the
/// graph may store whichever spelling the caller used; identity is what counts.
fn same_path(a: &str, b: &Path) -> bool {
    let ca = Path::new(a)
        .canonicalize()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| a.to_string());
    ca == canon(b)
}

/// Common setup: owner + bound repo ingested as the "dev graph".
/// Returns (owner, bound_repo, bound node_count).
async fn owner_with_bound_graph(tmp: &Path) -> (Owner, PathBuf, u64) {
    let bound_repo = tmp.join("bound-repo");
    write_bound_repo(&bound_repo);
    let owner = mk_owner(&tmp.join("runtime"));

    // Ingest the bound repo as the dev graph (a caller AT that root — the classic
    // single-brain flow, unchanged).
    let sid = owner.init_session(&bound_repo).await;
    let ingest = owner
        .tool(
            &sid,
            &bound_repo,
            "ingest",
            serde_json::json!({"path": bound_repo.to_string_lossy(), "agent_id": "setup"}),
        )
        .await;
    assert!(
        ingest["node_count"].as_u64().unwrap_or(0) > 0,
        "bound ingest must produce nodes: {ingest}"
    );
    let n0 = owner.node_count(&sid, &bound_repo).await;
    (owner, bound_repo, n0)
}

// ---------------------------------------------------------------------------
// (1) + (2) — one-call bootstrap: isolation + same-session stickiness.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bootstrap_creates_project_brain_without_touching_bound_graph() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, bound_repo, n0) = owner_with_bound_graph(tmp.path()).await;

    let project_b = tmp.path().join("project-b");
    write_project_b_repo(&project_b);

    // A session rooted in project B (the project-b situation).
    let sid_b = owner.init_session(&project_b).await;

    // THE ONE CALL. Pre-fix this is the clobber: `project_root` is inert and the
    // plain ingest REPLACES the bound dev graph for everyone.
    let boot = owner
        .tool(
            &sid_b,
            &project_b,
            "ingest",
            serde_json::json!({
                "path": project_b.to_string_lossy(),
                "project_root": project_b.to_string_lossy(),
                "agent_id": "project-b-agent"
            }),
        )
        .await;

    // The bootstrap envelope: schema + the ingest counts + a north-grade packet of
    // the NEW brain, all in the SAME response (total friction = ONE call).
    assert_eq!(
        boot["schema"], "m1nd-project-brain-bootstrap-v0",
        "one-call bootstrap must return the bootstrap envelope, got: {boot}"
    );
    assert!(
        boot["ingest"]["node_count"].as_u64().unwrap_or(0) > 0,
        "bootstrap must carry the project ingest counts: {boot}"
    );
    let north_b = &boot["north"];
    assert_eq!(
        north_b["schema"], "m1nd-north-packet-v0",
        "bootstrap must orient the caller in the same response: {boot}"
    );
    let roots_b = ingest_roots_of(north_b);
    assert!(
        roots_b.iter().any(|r| same_path(r, &project_b)),
        "the new brain's ingest_roots must be project B, got {roots_b:?}"
    );

    // ISOLATION — the m1nd/dev graph is never replaced or polluted. A bound-side
    // session still sees the EXACT pre-bootstrap graph.
    let sid_bound = owner.init_session(&bound_repo).await;
    let n_after = owner.node_count(&sid_bound, &bound_repo).await;
    assert_eq!(
        n_after, n0,
        "bound dev graph must be untouched by a project bootstrap (was {n0}, now {n_after})"
    );
    let north_bound = owner
        .tool(
            &sid_bound,
            &bound_repo,
            "north",
            serde_json::json!({"agent_id": "dev", "task": "bound_gravity_anchor"}),
        )
        .await;
    let roots_bound = ingest_roots_of(&north_bound);
    assert!(
        roots_bound.iter().any(|r| same_path(r, &bound_repo)),
        "bound ingest_roots must still be the bound repo, got {roots_bound:?}"
    );
    assert!(
        !roots_bound.iter().any(|r| same_path(r, &project_b)),
        "project B must NOT leak into the bound graph's roots: {roots_bound:?}"
    );

    // STICKINESS — the SAME session's next calls answer from brain B, silently.
    let north_same = owner
        .tool(
            &sid_b,
            &project_b,
            "north",
            serde_json::json!({"agent_id": "project-b-agent", "task": "project_b_probe"}),
        )
        .await;
    assert!(
        north_same["reception"].is_null(),
        "a caller on its OWN brain must get silence, not a reception block (TT-INV-12): {}",
        north_same["reception"]
    );
    let ws = fingerprint(&north_same)["workspace_root"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert_eq!(
        ws,
        canon(&project_b),
        "same-session calls after bootstrap must be served by brain B"
    );
    // The B brain literally cannot contain bound files — no focus node may point
    // at a bound fixture path.
    if let Some(focus) = north_same["context"]["focus_nodes"].as_array() {
        for node in focus {
            let path = node["path"].as_str().unwrap_or_default();
            assert!(
                !path.contains("gravity.rs") && !path.contains("tides.rs"),
                "brain B focus must never surface bound-repo files: {path}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// (3) + (4) — NEW sessions: silent auto-routing by caller_root; bound callers
// keep the dev graph.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn new_session_from_project_root_binds_silently() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, bound_repo, n0) = owner_with_bound_graph(tmp.path()).await;

    let project_b = tmp.path().join("project-b");
    write_project_b_repo(&project_b);

    // Bootstrap once, from some session.
    let sid_boot = owner.init_session(&project_b).await;
    let boot = owner
        .tool(
            &sid_boot,
            &project_b,
            "ingest",
            serde_json::json!({
                "path": project_b.to_string_lossy(),
                "project_root": project_b.to_string_lossy(),
                "agent_id": "bootstrapper"
            }),
        )
        .await;
    assert_eq!(boot["schema"], "m1nd-project-brain-bootstrap-v0");

    // A BRAND NEW session from the same root: zero setup calls, zero reception —
    // the first verb is already served by brain B (the friction math: after = 0
    // wasted calls, forever).
    let sid_new = owner.init_session(&project_b).await;
    let north = owner
        .tool(
            &sid_new,
            &project_b,
            "north",
            serde_json::json!({"agent_id": "fresh-session", "task": "project_b_probe"}),
        )
        .await;
    assert!(
        north["reception"].is_null(),
        "a new session whose caller_root has a project brain must bind SILENTLY \
         (no reception block), got: {}",
        north["reception"]
    );
    assert_eq!(
        fingerprint(&north)["workspace_root"].as_str().unwrap_or(""),
        canon(&project_b),
        "new session must be served by brain B automatically"
    );

    // Bound-side sessions are exactly as before.
    let sid_dev = owner.init_session(&bound_repo).await;
    let north_dev = owner
        .tool(
            &sid_dev,
            &bound_repo,
            "north",
            serde_json::json!({"agent_id": "dev", "task": "bound_gravity_anchor"}),
        )
        .await;
    assert!(
        north_dev["reception"].is_null(),
        "bound-root callers keep silent match: {}",
        north_dev["reception"]
    );
    assert_eq!(
        owner.node_count(&sid_dev, &bound_repo).await,
        n0,
        "bound graph node_count unchanged"
    );
}

// ---------------------------------------------------------------------------
// (5) — persistence: a restarted owner warm-boots brain B from its store.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn project_brain_warm_boots_after_owner_restart() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = tmp.path().join("runtime");
    let (owner, _bound_repo, _n0) = owner_with_bound_graph(tmp.path()).await;

    let project_b = tmp.path().join("project-b");
    write_project_b_repo(&project_b);

    let sid = owner.init_session(&project_b).await;
    let boot = owner
        .tool(
            &sid,
            &project_b,
            "ingest",
            serde_json::json!({
                "path": project_b.to_string_lossy(),
                "project_root": project_b.to_string_lossy(),
                "agent_id": "bootstrapper"
            }),
        )
        .await;
    let n_b = boot["ingest"]["node_count"].as_u64().unwrap_or(0);
    assert!(n_b > 0, "bootstrap ingest must count nodes: {boot}");
    drop(owner);

    // "Restart": a brand-new owner over the SAME runtime dir (fresh registry
    // object, empty brain map). First touch from root B must warm-boot the brain
    // from its persisted store — #230 semantics per store.
    let owner2 = mk_owner(&runtime);
    let sid2 = owner2.init_session(&project_b).await;
    let north = owner2
        .tool(
            &sid2,
            &project_b,
            "north",
            serde_json::json!({"agent_id": "morning-session", "task": "project_b_probe"}),
        )
        .await;
    assert!(
        north["reception"].is_null(),
        "after restart, a caller from root B must still bind brain B silently: {}",
        north["reception"]
    );
    assert_eq!(
        fingerprint(&north)["workspace_root"].as_str().unwrap_or(""),
        canon(&project_b),
        "restarted owner must warm-boot brain B for its root"
    );
    let warm_nodes = fingerprint(&north)["node_count"].as_u64().unwrap_or(0);
    assert!(
        warm_nodes >= n_b,
        "warm boot must preserve brain B's graph ({warm_nodes} < {n_b} means it \
         started fresh — the persistence gate)"
    );
}

// ---------------------------------------------------------------------------
// (6) — the reception option is the REAL call, not a roadmap promise.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reception_option_carries_the_real_bootstrap_call() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, _bound_repo, _n0) = owner_with_bound_graph(tmp.path()).await;

    // A root with NO brain anywhere: reception must fire AND its
    // `ingest_your_repo` option must be the real one-call bootstrap.
    let stranger = tmp.path().join("stranger-repo");
    std::fs::create_dir_all(&stranger).expect("mk stranger");

    let sid = owner.init_session(&stranger).await;
    let north = owner
        .tool(
            &sid,
            &stranger,
            "north",
            serde_json::json!({"agent_id": "stranger", "task": "orient"}),
        )
        .await;
    let reception = &north["reception"];
    assert_eq!(
        reception["match"], "caller_root_mismatch",
        "an unknown root must still get the honest mismatch block: {north}"
    );
    let options = reception["options"]
        .as_array()
        .expect("reception.options must be an array");
    let ingest_option = options
        .iter()
        .find(|o| o["action"] == "ingest_your_repo")
        .expect("reception must keep the ingest_your_repo option");
    let call = ingest_option["call"].as_str().unwrap_or_default();
    assert!(
        call.contains("project_root="),
        "the reception option must carry the REAL one-call bootstrap invocation \
         (`project_root=<root>`), not a roadmap note; got: {call}"
    );
    assert!(
        !call.contains("separate runtime today"),
        "the roadmap-era wording must be gone once the call is real: {call}"
    );
}
