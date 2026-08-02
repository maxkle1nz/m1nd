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
//! in-process for read/routing traffic. Sovereign birth fixtures enter through
//! the owner's actor and production bootstrap core: generic mutation ingress is
//! deliberately authority-frozen until its exact typed consumer exists.
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
//!   (6) reception remains honest while the public bootstrap consumer is absent:
//!       it reports `brain_bootstrap_consumer_not_installed` and carries no call.
#![cfg(feature = "serve")]

use crate as m1nd_mcp;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Bytes;
use axum::http::HeaderMap;
use m1nd_mcp::brain_runtime::BrainSessionCell;
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

/// A distinct scratch repo #`n` with a unique sentinel fn, so each bootstrapped
/// brain has an identifiable graph the warm-boot can be asserted against. Node
/// counts scale a little with `n` so no two are accidentally identical.
fn write_numbered_repo(root: &Path, n: usize) {
    std::fs::create_dir_all(root.join("src")).expect("mk src");
    std::fs::write(
        root.join("Cargo.toml"),
        format!("[package]\nname = \"scratch{n}\"\nversion = \"0.0.0\"\n"),
    )
    .expect("Cargo.toml");
    // n+1 unique fns so counts differ per repo and are non-trivial.
    let mut body = String::new();
    for i in 0..=n {
        body.push_str(&format!(
            "pub fn scratch_{n}_probe_{i}() -> i64 {{ {i} }}\n"
        ));
    }
    std::fs::write(root.join("src/lib.rs"), body).expect("lib.rs");
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
    mk_owner_with_cap(runtime, m1nd_mcp::project_brains::DEFAULT_WARM_BRAIN_CAP)
}

/// Like [`mk_owner`] but with an explicit warm-brain cap — the eviction-gate
/// (§C9.1) proof pins the bound small so a handful of scratch brains force
/// eviction.
fn mk_owner_with_cap(runtime: &Path, cap: usize) -> Owner {
    std::fs::create_dir_all(runtime).expect("mk runtime");
    let config = McpConfig {
        graph_source: runtime.join("graph_snapshot.json"),
        plasticity_state: runtime.join("plasticity_state.json"),
        runtime_dir: Some(runtime.to_path_buf()),
        registry_dir: Some(runtime.join("registry")),
        ..Default::default()
    };
    let server = McpServer::new(config.clone()).expect("boot owner");
    let session = Arc::new(BrainSessionCell::new(server.into_session_state()));
    let (event_tx, _rx) = broadcast::channel::<SseEvent>(64);
    let tool_schemas_cache = tool_schemas()
        .get("tools")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    let project_brains = Arc::new(ProjectBrainRegistry::with_capacity(
        runtime.join("project-brains"),
        Some(runtime.join("registry")),
        cap,
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

    /// How many project brains are hydrated in the warm map RIGHT NOW.
    fn warm_len(&self) -> usize {
        self.app.project_brains.warm_len()
    }

    /// Seed the bound graph through the owner actor. This is fixture authority,
    /// not a bypass or relaxation of the generic mutation policy.
    fn ingest_bound_actor(&self, root: &Path, agent: &str) -> serde_json::Value {
        let input: m1nd_mcp::protocol::IngestInput = serde_json::from_value(serde_json::json!({
            "path": root.to_string_lossy(),
            "agent_id": agent
        }))
        .expect("owner ingest input");
        self.app
            .project_brains
            .execute_target_m1nd(
                Arc::clone(&self.app.session),
                None,
                true,
                true,
                move |state| m1nd_mcp::tools::handle_ingest(state, input),
            )
            .expect("bound ingest actor")
    }

    /// Run the production bootstrap core through the owner actor, then apply the
    /// same sticky wire binding the approved transport seam owns.
    async fn bootstrap_actor(&self, root: &Path, agent: &str) -> (String, serde_json::Value) {
        let sid = self.init_session(root).await;
        let project_root = root.to_string_lossy().to_string();
        let arguments = serde_json::json!({
            "path": project_root,
            "project_root": project_root,
            "agent_id": agent
        });
        let (key, boot) =
            m1nd_mcp::mcp_http::run_bootstrap_core(self.app.as_ref(), &project_root, &arguments)
                .expect("owner bootstrap actor");
        self.app
            .mcp_sessions
            .lock()
            .get_mut(&sid)
            .expect("fixture wire session")
            .bound_project_root = Some(key);
        (sid, boot)
    }

    /// Bootstrap a project fixture and return its ingested node_count.
    async fn bootstrap(&self, root: &Path, agent: &str) -> u64 {
        let (_sid, boot) = self.bootstrap_actor(root, agent).await;
        boot["ingest"]["node_count"]
            .as_u64()
            .unwrap_or_else(|| panic!("bootstrap without node_count: {boot}"))
    }
}

/// The `binding.fingerprint` block from a `north` packet.
fn fingerprint(north: &serde_json::Value) -> &serde_json::Value {
    &north["binding"]["fingerprint"]
}

/// The binding's ingest roots as seen from a north packet. The fingerprint block
/// is budget-capped (Budget Law §C1.3.4) and carries only a head once a brain
/// accumulates roots, so this helper REFUSES a truncated block: the negative
/// assertions below ("root X must not be here") would silently pass on a partial
/// list. These fixtures bootstrap one root per brain, far under the cap; a future
/// fixture that trips this must read `doctor -> runtime_state.ingest_roots`.
fn ingest_roots_of(north: &serde_json::Value) -> Vec<String> {
    let fp = fingerprint(north);
    assert_ne!(
        fp["ingest_roots_truncated"],
        serde_json::json!(true),
        "fingerprint roots are truncated ({} of {} shown) — read the full array \
         from doctor instead of asserting over a head",
        fp["ingest_roots"].as_array().map(|a| a.len()).unwrap_or(0),
        fp["ingest_root_count"],
    );
    fp["ingest_roots"]
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

/// Canonicalized string form of a path string (for comparing a reception field's
/// stored root against a fixture path across `/tmp` → `/private/tmp` aliases).
fn canon_str(s: &str) -> String {
    Path::new(s)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(s))
        .to_string_lossy()
        .to_string()
}

/// Common setup: owner + bound repo ingested as the "dev graph".
/// Returns (owner, bound_repo, bound node_count).
async fn owner_with_bound_graph(tmp: &Path) -> (Owner, PathBuf, u64) {
    let bound_repo = tmp.join("bound-repo");
    write_bound_repo(&bound_repo);
    let owner = mk_owner(&tmp.join("runtime"));

    // Seed the bound repo through the owner's actor. Generic ingest remains
    // sovereign-frozen; subsequent observations still cross the real wire.
    let sid = owner.init_session(&bound_repo).await;
    let ingest = owner.ingest_bound_actor(&bound_repo, "setup");
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

    // Exercise the production bootstrap core under owner authority. The generic
    // wire door remains frozen; this test proves isolation and sticky routing.
    let (sid_b, boot) = owner.bootstrap_actor(&project_b, "project-b-agent").await;

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
    let (_sid_boot, boot) = owner.bootstrap_actor(&project_b, "bootstrapper").await;
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

    let (_sid, boot) = owner.bootstrap_actor(&project_b, "bootstrapper").await;
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
// (6) — reception reports the closed consumer, never an unreachable call.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reception_reports_the_closed_bootstrap_consumer_without_a_call() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, _bound_repo, _n0) = owner_with_bound_graph(tmp.path()).await;

    // A root with NO brain anywhere: reception must fire, but the internal
    // owner-only bootstrap must not leak as a public repair.
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
    let bootstrap_option = options
        .iter()
        .find(|o| o["action"] == "bootstrap_unavailable")
        .expect("reception must name the closed bootstrap state");
    assert_eq!(
        bootstrap_option["code"],
        "brain_bootstrap_consumer_not_installed"
    );
    assert!(bootstrap_option.get("call").is_none());
    assert!(!reception.to_string().contains("project_root"));
}

// ---------------------------------------------------------------------------
// (6b) — a caller_root_mismatch is answered FAST, with the human voice intact.
// ---------------------------------------------------------------------------

/// The field kill (2026-08-02, Paco's probes): a north from a root the bound
/// graph does not cover cost ≈47–55s on a 21k graph (activate ≈44s of it) only
/// to return "this graph does not cover your repo" — a fact the reception knows
/// from the caller_root header in microseconds. north now returns under
/// mismatch BEFORE trust_selftest and orient/activate. The earlier attempt at
/// this dropped the human_view voice card; this proves it survives — the card
/// carries no statistics under a mismatch, so it needs no expensive step.
///
/// This harness's store serves medulla (brainless), so it exercises the
/// COMPLEMENT: the fast path must NOT fire for a served-medulla caller (its
/// doctrine beat is legitimate), and the full mismatch packet — with its
/// project_brain_absent gap and human_view — must still compose. Together with
/// the human_view unit test in server.rs (which drives a non-medulla mismatch),
/// both sides of the guard are pinned.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_served_medulla_mismatch_keeps_its_full_beat_not_the_fast_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, _bound_repo, _n0) = owner_with_bound_graph(tmp.path()).await;

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

    assert_eq!(
        north["reception"]["match"], "caller_root_mismatch",
        "the mismatch is still declared: {north}"
    );
    // The served-medulla case is NOT the fast path: it keeps the full beat.
    assert_ne!(
        north["proof_state"], "reception_mismatch",
        "a served-medulla mismatch keeps its full beat, not the bare fast path: {north}"
    );
    assert!(
        north["human_view"].is_object(),
        "the human voice card must be present on every mismatch: {north}"
    );
    let gaps = north["honest_gaps"].to_string();
    assert!(
        gaps.contains("project_brain_absent"),
        "the served-medulla mismatch names project_brain_absent: {north}"
    );
}

// ---------------------------------------------------------------------------
// (7) — THE EVICTION GATE (§C9.1, ladder R15). The interim owner-hosted topology
// recreates the blast radius two-tier was built to kill: one owner holds N warm
// brains, so an unbounded map + one crash loses N brains' state. Law: the warm
// map is LRU-bounded, and a brain persists-then-drops on eviction so a later call
// warm-boots it back identical. Battery case: bootstrap cap+1 brains → the map
// never exceeds the cap → `kill -9` the owner → EVERY brain warm-boots from its
// own snapshot with no data loss → the bound dev graph never evicts.
//
// RED before this rung: the map is unbounded (no eviction — `warm_len` grows to
// cap+1) OR (had eviction been a naive drop) the evicted brain would lose its
// unpersisted state and warm-boot smaller than it was ingested.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn eviction_gate_bounds_the_map_and_persists_on_evict_surviving_kill9() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = tmp.path().join("runtime");
    const CAP: usize = 2;

    // Owner bound to a dev graph, with a tiny warm-brain cap so a few scratch
    // brains force the gate.
    let bound_repo = tmp.path().join("bound-repo");
    write_bound_repo(&bound_repo);
    let owner = mk_owner_with_cap(&runtime, CAP);
    let sid_bound = owner.init_session(&bound_repo).await;
    let bound_ingest = owner.ingest_bound_actor(&bound_repo, "dev");
    let bound_nodes = bound_ingest["node_count"].as_u64().unwrap_or(0);
    assert!(bound_nodes > 0, "bound graph must ingest: {bound_ingest}");

    // Bootstrap CAP+1 distinct project brains. Bootstrap auto-persists each
    // brain's snapshot immediately (#230), so what we prove here is the map BOUND
    // and that EVERY brain — the evicted ones included — survives a hard kill and
    // warm-boots from its own store. (The persist-on-evict path for state mutated
    // AFTER a brain's last persist is proven at the unit level in
    // `project_brains.rs::eviction_persists_unpersisted_state`.)
    let n_brains = CAP + 1;
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut ingested_nodes: Vec<u64> = Vec::new();
    for i in 0..n_brains {
        let root = tmp.path().join(format!("scratch-{i}"));
        write_numbered_repo(&root, i + 1);
        let nodes = owner.bootstrap(&root, &format!("agent-{i}")).await;
        assert!(nodes > 0, "scratch brain {i} must ingest nodes");
        // THE BOUND, checked on every insert: the warm map NEVER exceeds the cap.
        // Pre-fix (unbounded map) this reaches CAP+1 and fails here.
        assert!(
            owner.warm_len() <= CAP,
            "warm map exceeded the cap after bootstrapping brain {i}: {} > {CAP} \
             (the eviction gate did not arm)",
            owner.warm_len()
        );
        roots.push(root);
        ingested_nodes.push(nodes);
    }
    // With cap+1 brains bootstrapped and a cap of CAP, at least one must have been
    // evicted — so the map sits AT the cap, not below.
    assert_eq!(
        owner.warm_len(),
        CAP,
        "after bootstrapping cap+1 brains the warm map must sit at the cap"
    );

    // The bound dev graph is NOT a project brain — it lives on AppState::session,
    // never in the evictable map — so it must answer unchanged after all the
    // churn (it can never be the eviction victim).
    assert_eq!(
        owner.node_count(&sid_bound, &bound_repo).await,
        bound_nodes,
        "the bound dev graph must never be evicted by project-brain churn"
    );

    // KILL -9: drop the whole owner (no graceful shutdown, no final flush). The
    // only state that survives is what persist-on-evict + bootstrap's immediate
    // persist already wrote to each store.
    drop(owner);

    // A brand-new owner over the SAME runtime dir (fresh empty map). EVERY brain —
    // the evicted ones included — must warm-boot from its own snapshot with its
    // full ingested graph intact (node counts match). A brain that was evicted
    // WITHOUT persist-on-evict would warm-boot fresh/empty and fail here.
    let owner2 = mk_owner_with_cap(&runtime, CAP);
    for (i, root) in roots.iter().enumerate() {
        let sid = owner2.init_session(root).await;
        let north = owner2
            .tool(
                &sid,
                root,
                "north",
                serde_json::json!({"agent_id": "post-kill", "task": "orient"}),
            )
            .await;
        assert!(
            north["reception"].is_null(),
            "post-kill, root {i} must bind its own brain silently: {}",
            north["reception"]
        );
        let warm_nodes = fingerprint(&north)["node_count"].as_u64().unwrap_or(0);
        assert_eq!(
            warm_nodes, ingested_nodes[i],
            "brain {i} lost state across kill-9: warm-booted {warm_nodes} nodes, \
             ingested {} — its store was not preserved through eviction",
            ingested_nodes[i]
        );
    }
}

// ---------------------------------------------------------------------------
// (8) — RECONNECT-REBIND (§C5.4, ladder R13, field letter#49). After an MCP
// reconnect the wire session is minted fresh (`bound_project_root: None`) and the
// bridge stamps `M1nd-Caller-Root` = the HOST CWD — which, when the host was
// launched from a dir ABOVE the repo (the classic `~` launch), is an ANCESTOR of
// the repo, not the repo. So `caller_root` has no brain of its own and the bound
// graph does not cover it. Reception may name the existing descendant brain, but
// must not manufacture a public warm-rebind call while its typed consumer is absent.
//
// Law: a rebind after a session-id change re-runs first-contact classification
// with the disk roster consulted, so an EXISTING project brain is preferred over
// the owner graph. When exactly one known brain relates to the caller_root by
// ancestry, reception names THAT brain's root (`known_brain`) and keeps bootstrap
// closed. Ambiguity is honest: 0 or >1 related
// brains → the plain unknown-repo reception, unchanged.
//
// The prior defect suggested the host cwd and carried no `known_brain` — the
// existing brain was invisible to the front desk.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnect_reception_prefers_the_existing_brain_over_the_host_cwd() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, _bound_repo, _n0) = owner_with_bound_graph(tmp.path()).await;

    // The host is launched from a workspace dir; the real repo lives one level in
    // (`<ws>/project-b`). The reconnect's caller_root will be `<ws>`, an ancestor
    // of the brain root — the field shape of letter#49.
    let workspace = tmp.path().join("workspace");
    let project_b = workspace.join("project-b");
    write_project_b_repo(&project_b);

    // Bootstrap brain B from a session correctly rooted at the repo (the day it was
    // ingested). This leaves a project brain ON DISK for `<ws>/project-b`.
    let (_sid_boot, boot) = owner.bootstrap_actor(&project_b, "bootstrapper").await;
    assert_eq!(boot["schema"], "m1nd-project-brain-bootstrap-v0");

    // THE RECONNECT. A brand-new wire session whose caller_root is the HOST CWD
    // (`<ws>`, the ancestor) — the bind to brain B is gone, the bridge re-stamped
    // the launch dir, not the repo.
    let sid_reconnect = owner.init_session(&workspace).await;
    let north = owner
        .tool(
            &sid_reconnect,
            &workspace,
            "north",
            serde_json::json!({"agent_id": "reconnected", "task": "orient"}),
        )
        .await;

    // It is NOT a silent match (the caller_root is the ancestor, not the repo), so
    // reception fires — but it must POINT AT brain B, never the host cwd.
    let reception = &north["reception"];
    assert_eq!(
        reception["match"], "caller_root_mismatch",
        "the reconnect from an ancestor cwd is still a mismatch (fires reception): {north}"
    );
    assert_eq!(
        reception["known_brain"].as_str().map(canon_str),
        Some(canon(&project_b)),
        "reconnect reception must name the EXISTING brain under the caller_root, \
         not fall to the owner graph blind: {reception}"
    );
    let bootstrap_option = reception["options"]
        .as_array()
        .and_then(|opts| opts.iter().find(|o| o["action"] == "bootstrap_unavailable"))
        .expect("closed bootstrap option");
    assert_eq!(
        bootstrap_option["code"],
        "brain_bootstrap_consumer_not_installed"
    );
    assert!(bootstrap_option.get("call").is_none());
    assert!(
        bootstrap_option["note"]
            .as_str()
            .unwrap_or_default()
            .contains(&canon(&project_b)),
        "known-brain reception should name the exact root without an executable call: {reception}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reconnect_unknown_root_keeps_the_plain_reception_and_a_match_stays_silent() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, _bound_repo, _n0) = owner_with_bound_graph(tmp.path()).await;

    let workspace = tmp.path().join("workspace");
    let project_b = workspace.join("project-b");
    write_project_b_repo(&project_b);
    let (_sid_boot, boot) = owner.bootstrap_actor(&project_b, "bootstrapper").await;
    assert_eq!(boot["schema"], "m1nd-project-brain-bootstrap-v0");

    // (a) A caller root with NO related brain anywhere on its ancestry — the honest
    //     unknown-repo reception has no `known_brain` and keeps bootstrap closed.
    let stranger = tmp.path().join("stranger-elsewhere");
    std::fs::create_dir_all(&stranger).expect("mk stranger");
    let sid_stranger = owner.init_session(&stranger).await;
    let north_stranger = owner
        .tool(
            &sid_stranger,
            &stranger,
            "north",
            serde_json::json!({"agent_id": "stranger", "task": "orient"}),
        )
        .await;
    let reception = &north_stranger["reception"];
    assert_eq!(
        reception["match"], "caller_root_mismatch",
        "an unrelated root still gets the honest mismatch block: {north_stranger}"
    );
    assert!(
        reception.get("known_brain").is_none() || reception["known_brain"].is_null(),
        "no related brain → no known_brain hint (honest absence): {reception}"
    );
    let stranger_option = reception["options"]
        .as_array()
        .and_then(|opts| opts.iter().find(|o| o["action"] == "bootstrap_unavailable"))
        .expect("closed bootstrap option");
    assert_eq!(
        stranger_option["code"],
        "brain_bootstrap_consumer_not_installed"
    );
    assert!(stranger_option.get("call").is_none());

    // (b) TT-INV-12 preserved: a caller rooted EXACTLY at brain B still binds
    //     silently — the roster consult must never turn a matched root into a packet.
    let sid_match = owner.init_session(&project_b).await;
    let north_match = owner
        .tool(
            &sid_match,
            &project_b,
            "north",
            serde_json::json!({"agent_id": "matched", "task": "project_b_probe"}),
        )
        .await;
    assert!(
        north_match["reception"].is_null(),
        "a matched caller_root must stay silent even with the roster consulted (TT-INV-12): {}",
        north_match["reception"]
    );
}
