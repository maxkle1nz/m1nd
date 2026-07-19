//! MEDULLA ladder R3 / slice M5b — pull-only tier recall (the read side of the
//! medulla). Drives the REAL Streamable-HTTP routing (`handle_mcp_post`) in-process
//! for recall traffic. Sovereign fixture birth uses the owner's actor because the
//! generic mutation door is authority-frozen, never bypassed.
//!
//! THE LAWS UNDER TEST (MEDULLA-PRD §3.2, §5, §10):
//!   - MED-INV-1 (the no-leak law): a claim from brain Y NEVER surfaces in brain X's
//!     DEFAULT beat unless its tier is medulla (promoted / doctrine-born). Pull, not
//!     push, made mechanical.
//!   - `tier:"all-brains"` DOES surface Y's claim in X's beat, labeled origin_brain=Y
//!     — the explicit cross-project inspection, one argument away, never ambient.
//!   - the default beat = project + medulla only; a medulla (doctrine-born) claim
//!     surfaces cross-brain by default, tier-labeled.
//!   - the `all-brains` fan-out warm-boots through the R15 eviction gate: a wide
//!     fan-out can never pin more than the warm-brain cap (§C9.1).
//!   - provenance-in-recall: every folded row carries origin_brain (MED-INV-4).
//!
//! RED before M5b: a `seek`/`north` had no tier fan-out, so `all-brains` could not
//! surface Y's claim (no cross-brain path existed); and the medulla feed was not
//! composed into a routed project brain's default beat. This file pins both the
//! never-leak direction AND the must-surface-on-demand direction.
#![cfg(feature = "serve")]

use crate as m1nd_mcp;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Bytes;
use axum::http::HeaderMap;
use m1nd_mcp::brain_runtime::BrainSessionCell;
use serde_json::Value;
use tokio::sync::broadcast;

use m1nd_mcp::http_server::{AppState, SseEvent};
use m1nd_mcp::mcp_http::{handle_mcp_post, new_mcp_session_registry};
use m1nd_mcp::project_brains::ProjectBrainRegistry;
use m1nd_mcp::server::{tool_schemas, McpConfig, McpServer};

// ---------------------------------------------------------------------------
// Fixture repos — tiny, deterministic, DISTINCT sentinels per repo. Neutral
// names only (no other-project names, no personal paths).
// ---------------------------------------------------------------------------

fn write_repo(root: &Path, tag: &str) {
    std::fs::create_dir_all(root.join("src")).expect("mk repo src");
    std::fs::write(
        root.join("src/lib.rs"),
        format!("pub fn {tag}_probe() -> i64 {{ 42 }}\npub struct {tag}Widget {{ pub v: i64 }}\n"),
    )
    .expect("write lib.rs");
    std::fs::write(root.join("Cargo.toml"), "[package]\nname=\"fx\"\n").expect("write toml");
}

struct Owner {
    app: Arc<AppState>,
}

fn mk_owner_with_cap(runtime: &Path, cap: usize) -> Owner {
    std::fs::create_dir_all(runtime).expect("mk runtime");
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
        .unwrap_or(Value::Array(vec![]));
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
    async fn post(
        &self,
        session: Option<&str>,
        caller_root: Option<&Path>,
        body: Value,
    ) -> (Value, Option<String>) {
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
        let parsed = serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null);
        (parsed, minted)
    }

    async fn init_session(&self, caller_root: &Path) -> String {
        let (_b, minted) = self
            .post(
                None,
                Some(caller_root),
                serde_json::json!({
                    "jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                        "clientInfo": {"name": "m5b-probe", "version": "0"}}
                }),
            )
            .await;
        minted.expect("initialize mints a session id")
    }

    /// tools/call → the tool's parsed JSON payload (content[0].text).
    async fn tool(&self, sid: &str, caller_root: &Path, name: &str, args: Value) -> Value {
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

    /// Bootstrap a project brain through the production owner core; returns its
    /// sticky fixture sid.
    async fn bootstrap(&self, root: &Path, agent: &str) -> String {
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
        assert!(
            boot["ingest"]["node_count"].as_u64().unwrap_or(0) > 0,
            "bootstrap must ingest nodes: {boot}"
        );
        sid
    }

    /// Seed the bound "dev" graph through the owner actor.
    async fn ingest_bound(&self, root: &Path) -> String {
        let sid = self.init_session(root).await;
        let input: m1nd_mcp::protocol::IngestInput = serde_json::from_value(serde_json::json!({
            "path": root.to_string_lossy(),
            "agent_id": "setup"
        }))
        .expect("owner ingest input");
        let ing = self
            .app
            .project_brains
            .execute_target_m1nd(
                Arc::clone(&self.app.session),
                None,
                true,
                true,
                move |state| m1nd_mcp::tools::handle_ingest(state, input),
            )
            .expect("bound ingest actor");
        assert!(
            ing["node_count"].as_u64().unwrap_or(0) > 0,
            "bound ingest must produce nodes: {ing}"
        );
        sid
    }

    /// memorize a sentinel claim in the store the session is routed to.
    async fn memorize(&self, sid: &str, caller_root: &Path, agent: &str, label: &str, text: &str) {
        let out = self
            .tool(
                sid,
                caller_root,
                "memorize",
                serde_json::json!({
                    "agent_id": agent,
                    "node_label": label,
                    "claims": [{"label": label, "text": text, "confidence": "high"}]
                }),
            )
            .await;
        assert!(
            out["refused"].is_null(),
            "memorize for {label} must not be refused: {out}"
        );
        assert!(
            out["ingested"].as_bool().unwrap_or(false)
                || out["claims_written"].as_u64().unwrap_or(0) > 0,
            "memorize for {label} must write a claim: {out}"
        );
    }

    fn warm_len(&self) -> usize {
        self.app.project_brains.warm_len()
    }
}

/// The memory feed of a north packet.
fn north_memory(north: &Value) -> Vec<Value> {
    north["memory"].as_array().cloned().unwrap_or_default()
}

/// True when any row mentions `needle` — checks BOTH `claim` (north memory rows)
/// and `label` (seek result rows), so the same helper works across surfaces.
fn any_claim_mentions(rows: &[Value], needle: &str) -> bool {
    rows.iter().any(|r| {
        let claim = r["claim"].as_str().unwrap_or("");
        let label = r["label"].as_str().unwrap_or("");
        claim.contains(needle) || label.contains(needle)
    })
}

/// The seek results array.
fn seek_results(seek: &Value) -> Vec<Value> {
    seek["results"].as_array().cloned().unwrap_or_default()
}

// ===========================================================================
// (1) THE LEAK INVARIANT — a project claim NEVER leaks into another brain's
//     default beat; but IS reachable on demand via all-brains, labeled by origin.
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn med_inv_1_project_claim_never_leaks_but_all_brains_surfaces_it() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner_with_cap(&tmp.path().join("runtime"), 4);

    // Two project brains: X and Y (each its own store, its own graph).
    let root_x = tmp.path().join("repo-x");
    let root_y = tmp.path().join("repo-y");
    write_repo(&root_x, "X");
    write_repo(&root_y, "Y");
    let sid_x = owner.bootstrap(&root_x, "agent-x").await;
    let sid_y = owner.bootstrap(&root_y, "agent-y").await;

    // Seed a DISTINCT sentinel claim in brain Y only.
    let sentinel = "YellowbrickSentinelClaim";
    owner
        .memorize(
            &sid_y,
            &root_y,
            "agent-y",
            sentinel,
            "a private finding in brain Y",
        )
        .await;

    // --- THE NO-LEAK ASSERTION: brain X's DEFAULT beat NEVER carries Y's claim. ---
    let north_x = owner
        .tool(
            &sid_x,
            &root_x,
            "north",
            serde_json::json!({"agent_id": "agent-x", "task": format!("find {sentinel}")}),
        )
        .await;
    let mem_x = north_memory(&north_x);
    assert!(
        !any_claim_mentions(&mem_x, sentinel),
        "MED-INV-1 VIOLATED: brain Y's claim leaked into brain X's default north beat: {mem_x:?}"
    );

    // seek's default beat must not leak it either.
    let seek_x = owner
        .tool(
            &sid_x,
            &root_x,
            "seek",
            serde_json::json!({"agent_id": "agent-x", "query": sentinel}),
        )
        .await;
    assert!(
        !any_claim_mentions(&seek_results(&seek_x), sentinel),
        "MED-INV-1 VIOLATED: brain Y's claim leaked into brain X's default seek: {}",
        seek_x
    );

    // --- THE ON-DEMAND ASSERTION: tier=all-brains DOES surface Y's claim in X's
    //     beat, labeled origin_brain = Y. ---
    let north_all = owner
        .tool(
            &sid_x,
            &root_x,
            "north",
            serde_json::json!({
                "agent_id": "agent-x",
                "task": format!("find {sentinel}"),
                "tier": "all-brains"
            }),
        )
        .await;
    let mem_all = north_memory(&north_all);
    let hit = mem_all
        .iter()
        .find(|r| {
            r["claim"]
                .as_str()
                .map(|c| c.contains(sentinel))
                .unwrap_or(false)
        })
        .unwrap_or_else(|| {
            panic!("tier=all-brains MUST surface brain Y's claim in X's beat, got: {mem_all:?}")
        });
    let origin = hit["origin_brain"].as_str().unwrap_or_default();
    assert!(
        origin.contains("repo-y"),
        "the all-brains hit must be LABELED with its origin brain (repo-y), got origin_brain={origin:?} in {hit}"
    );
    assert_eq!(
        north_all["tier"].as_str(),
        Some("all-brains"),
        "the packet must honestly label its beat width"
    );
}

// ===========================================================================
// (2) THE MEDULLA FEED — a doctrine-born (medulla) claim SURFACES cross-brain in
//     the DEFAULT beat; that is the ONLY cross-brain thing the default beat carries.
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn medulla_doctrine_claim_surfaces_in_a_project_default_beat() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner_with_cap(&tmp.path().join("runtime"), 4);

    // The bound owner IS the medulla today. Ingest a tiny bound graph, then write a
    // doctrine claim into the medulla store (a covered/owner session).
    let bound = tmp.path().join("bound-repo");
    write_repo(&bound, "Bound");
    let sid_bound = owner.ingest_bound(&bound).await;
    let doctrine = "DoctrineProofStandardClaim";
    owner
        .memorize(
            &sid_bound,
            &bound,
            "maintainer",
            doctrine,
            "always prove before claiming",
        )
        .await;

    // A project brain X, distinct store.
    let root_x = tmp.path().join("repo-x");
    write_repo(&root_x, "X");
    let sid_x = owner.bootstrap(&root_x, "agent-x").await;

    // X's DEFAULT beat must carry the medulla doctrine claim, tier-labeled medulla.
    let north_x = owner
        .tool(
            &sid_x,
            &root_x,
            "north",
            serde_json::json!({"agent_id": "agent-x", "task": format!("recall {doctrine}")}),
        )
        .await;
    let mem_x = north_memory(&north_x);
    let hit = mem_x
        .iter()
        .find(|r| {
            r["claim"]
                .as_str()
                .map(|c| c.contains(doctrine))
                .unwrap_or(false)
        })
        .unwrap_or_else(|| {
            panic!("the medulla doctrine claim MUST surface in brain X's default beat: {mem_x:?}")
        });
    assert_eq!(
        hit["tier"].as_str(),
        Some("medulla"),
        "the doctrine claim must be labeled tier=medulla, got: {hit}"
    );
    assert_eq!(
        hit["origin_brain"].as_str(),
        Some("medulla"),
        "a doctrine-born claim's origin brain is medulla, got: {hit}"
    );
}

// ===========================================================================
// (3) DEFAULT UNCHANGED — tier absent behaves as project+medulla, and a lone brain
//     (no medulla claims, no siblings) still sees its OWN claim. Pins the default.
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn default_beat_carries_own_claim_and_labels_tier_project() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner_with_cap(&tmp.path().join("runtime"), 4);

    let root_x = tmp.path().join("repo-x");
    write_repo(&root_x, "X");
    let sid_x = owner.bootstrap(&root_x, "agent-x").await;
    let own = "MyOwnProjectClaim";
    owner
        .memorize(&sid_x, &root_x, "agent-x", own, "a fact local to brain X")
        .await;

    // Default (no tier): X's own claim surfaces, labeled tier=project, origin=X.
    let north_x = owner
        .tool(
            &sid_x,
            &root_x,
            "north",
            serde_json::json!({"agent_id": "agent-x", "task": format!("recall {own}")}),
        )
        .await;
    let mem = north_memory(&north_x);
    let hit = mem
        .iter()
        .find(|r| {
            r["claim"]
                .as_str()
                .map(|c| c.contains(own))
                .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("brain X's own claim must surface in its own beat: {mem:?}"));
    assert_eq!(
        hit["tier"].as_str(),
        Some("project"),
        "own claim is tier=project: {hit}"
    );
    assert!(
        hit["origin_brain"]
            .as_str()
            .unwrap_or_default()
            .contains("repo-x"),
        "own claim's origin brain is X: {hit}"
    );
}

// ===========================================================================
// (4) ALL-BRAINS FAN-OUT ROUTES THROUGH EVICTION — a fan-out over MORE brains than
//     the cap must never pin more than the cap in the warm map (§C9.1 · MED-INV-8).
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn all_brains_fanout_stays_within_the_eviction_cap() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Cap 2 — a fan-out over 5 project brains must never hold more than 2 warm.
    let cap = 2usize;
    let owner = mk_owner_with_cap(&tmp.path().join("runtime"), cap);

    // The caller lives in the bound owner (the medulla); it fans out over N project
    // brains. Build N distinct project brains, each with a sentinel claim.
    let bound = tmp.path().join("bound-repo");
    write_repo(&bound, "Bound");
    let sid_bound = owner.ingest_bound(&bound).await;

    let n = 5usize;
    let mut roots: Vec<PathBuf> = Vec::new();
    for i in 0..n {
        let root = tmp.path().join(format!("repo-{i}"));
        write_repo(&root, &format!("R{i}"));
        let sid = owner.bootstrap(&root, &format!("agent-{i}")).await;
        owner
            .memorize(
                &sid,
                &root,
                &format!("agent-{i}"),
                &format!("FanoutSentinel{i}"),
                "a claim in one of many brains",
            )
            .await;
        roots.push(root);
    }

    // Bootstrapping already exercised the gate; assert the precondition holds.
    assert!(
        owner.warm_len() <= cap,
        "precondition: warm map already within cap after bootstraps: {} > {cap}",
        owner.warm_len()
    );

    // THE FAN-OUT: an all-brains north from the bound owner touches every store.
    let north_all = owner
        .tool(
            &sid_bound,
            &bound,
            "north",
            serde_json::json!({
                "agent_id": "maintainer",
                "task": "FanoutSentinel survey across all brains",
                "tier": "all-brains"
            }),
        )
        .await;

    // THE EVICTION PROOF: after fanning out over N=5 brains with cap=2, the warm map
    // is STILL within the cap — the fan-out warm-booted through the eviction gate.
    assert!(
        owner.warm_len() <= cap,
        "all-brains fan-out over {n} brains pinned {} warm brains, exceeding cap {cap} — \
         the fan-out did NOT route through the R15 eviction gate",
        owner.warm_len()
    );

    // AND the fan-out actually reached across brains: at least a couple of distinct
    // sentinels surfaced, each labeled by its origin brain (not one anonymous pile).
    let mem = north_memory(&north_all);
    let labeled_sentinels = mem
        .iter()
        .filter(|r| {
            r["claim"]
                .as_str()
                .map(|c| c.contains("FanoutSentinel"))
                .unwrap_or(false)
                && r["origin_brain"].as_str().is_some()
        })
        .count();
    assert!(
        labeled_sentinels >= 2,
        "all-brains must surface sentinels from multiple brains, each origin-labeled — saw {labeled_sentinels}: {mem:?}"
    );
}

// ===========================================================================
// (5) EXPLICIT `tier:"medulla"` — from a project brain, returns ONLY the medulla
//     feed (the project's own claim is dropped); from the owner (which IS the
//     medulla) it returns the medulla's own claim, not empty. Pins the selector.
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tier_medulla_selector_returns_only_doctrine_from_a_project_brain() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner_with_cap(&tmp.path().join("runtime"), 4);

    // Doctrine in the medulla (bound owner), a private claim in project X.
    let bound = tmp.path().join("bound-repo");
    write_repo(&bound, "Bound");
    let sid_bound = owner.ingest_bound(&bound).await;
    let doctrine = "DoctrineOnlyClaim";
    owner
        .memorize(
            &sid_bound,
            &bound,
            "maintainer",
            doctrine,
            "the shared rule",
        )
        .await;

    let root_x = tmp.path().join("repo-x");
    write_repo(&root_x, "X");
    let sid_x = owner.bootstrap(&root_x, "agent-x").await;
    let private = "XPrivateOnlyClaim";
    owner
        .memorize(&sid_x, &root_x, "agent-x", private, "a fact local to X")
        .await;

    // tier=medulla from X's session: the doctrine surfaces, X's own private claim
    // does NOT (the project feed is dropped — only the medulla feed remains).
    let seek_medulla = owner
        .tool(
            &sid_x,
            &root_x,
            "seek",
            serde_json::json!({
                "agent_id": "agent-x",
                "query": "Claim",
                "tier": "medulla"
            }),
        )
        .await;
    let rows = seek_results(&seek_medulla);
    assert!(
        any_claim_mentions(&rows, doctrine),
        "tier=medulla must surface the doctrine claim: {rows:?}"
    );
    assert!(
        !any_claim_mentions(&rows, private),
        "tier=medulla must NOT surface the project's own private claim: {rows:?}"
    );

    // tier=medulla from the OWNER itself (which IS the medulla) must still return
    // its own doctrine claim — not empty (the strip-then-nothing regression guard).
    let seek_owner_medulla = owner
        .tool(
            &sid_bound,
            &bound,
            "seek",
            serde_json::json!({
                "agent_id": "maintainer",
                "query": "Claim",
                "tier": "medulla"
            }),
        )
        .await;
    assert!(
        any_claim_mentions(&seek_results(&seek_owner_medulla), doctrine),
        "tier=medulla on the owner (the medulla itself) must return its own claim, not empty: {seek_owner_medulla}"
    );
}
