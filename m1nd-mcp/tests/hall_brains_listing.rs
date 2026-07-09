//! The Hall's brains list — `/api/brains` shows PROJECTS, not plumbing.
//!
//! THE SEAM (Max's screenshot verdict, 2026-07-04): the Hall listed the OLD
//! instance registry (one card, runtime name "claude"), while the NEW per-project
//! brains #260 ships (e.g. the project-b brain) were invisible — #260's declared
//! residue was "REST/GUI bound-only". Verdict: the Hall must show PROJECTS
//! ("m1nd", "project-b"), never plumbing ("claude", "agent-memory").
//!
//! This pins the promoted REST surface (`http_server::brains_listing`, the pure
//! body of `GET /api/brains`) against a REAL owner driven through the SAME wire
//! seam (`handle_mcp_post`) the two-tier tests use:
//!   (A) BOTH brains are listed — the bound dev graph AND the hosted project brain.
//!   (B) display_name is the PROJECT basename, for both — never the runtime dir
//!       name, never "agent-memory".
//!   (C) bound-first ordering (the graph this owner serves leads the list).
//!   (D) the naming guard: no listed display_name equals a runtime dir name
//!       ("claude") or "agent-memory" while a project_root exists (the Brain Chip
//!       law, server side).
#![cfg(feature = "serve")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Bytes;
use axum::http::HeaderMap;
use parking_lot::Mutex;
use tokio::sync::broadcast;

use m1nd_mcp::http_server::{instances_listing, AppState, SseEvent};
use m1nd_mcp::mcp_http::{handle_mcp_post, new_mcp_session_registry};
use m1nd_mcp::project_brains::ProjectBrainRegistry;
use m1nd_mcp::server::{tool_schemas, McpConfig, McpServer};

// ---------------------------------------------------------------------------
// Fixtures — a bound "dev" repo and a hosted project repo, distinct basenames.
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

fn write_project_repo(root: &Path) {
    std::fs::create_dir_all(root.join("src")).expect("mk src");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"projectb\"\nversion = \"0.0.0\"\n",
    )
    .expect("Cargo.toml");
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn project_b_probe() -> i64 { 42 }\n",
    )
    .expect("lib.rs");
}

// ---------------------------------------------------------------------------
// Owner harness — a real AppState, driven through the real wire handler.
// ---------------------------------------------------------------------------

struct Owner {
    app: Arc<AppState>,
}

fn mk_owner(runtime: &Path) -> Owner {
    std::fs::create_dir_all(runtime).expect("mk runtime");
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
        }),
    }
}

impl Owner {
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
                        "clientInfo": {"name": "hall-brains-probe", "version": "0"}
                    }
                }),
            )
            .await;
        minted.expect("initialize must mint a session id")
    }

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
}

/// Canonical string form (macOS /tmp → /private/tmp), for path comparisons.
fn canon(p: &Path) -> String {
    p.canonicalize()
        .unwrap_or_else(|_| p.to_path_buf())
        .to_string_lossy()
        .to_string()
}

/// The repo basename of a filesystem root, separator-agnostic — the SAME thing
/// production's `session::basename_of` computes for `display_name`. Splits on
/// BOTH '/' and '\\' (tolerating trailing separators) so a Windows backslash
/// `project_root` ("C:\\...\\project-repo") yields "project-repo" here exactly as
/// it does in the production listing, instead of the whole path. This is why the
/// prior fix (#275 fixed production `basename_of`) did not un-red Windows CI: the
/// naming-guard test recomputed the expected basename with '/'-only splitting,
/// so on Windows `expected` was the entire path and the assertion failed.
fn repo_basename(root: &str) -> &str {
    let is_sep = |c: char| c == '/' || c == '\\';
    root.trim_end_matches(is_sep)
        .rsplit(is_sep)
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(root)
}

#[test]
fn repo_basename_is_separator_agnostic() {
    // POSIX separators (Unix CI / the macOS dev box).
    assert_eq!(
        repo_basename("/private/tmp/xyz/project-repo"),
        "project-repo"
    );
    assert_eq!(
        repo_basename("/private/tmp/xyz/project-repo/"),
        "project-repo"
    );
    // Windows backslash separators — the chronic red CI case. On a Unix box the
    // std::path::Path basename of a backslash string is the whole string (there
    // is no '\\' separator off-Windows), so the guard MUST split on '\\' itself.
    // This pins the exact input shape that broke #279/#280 without needing a
    // Windows runner to observe the failure.
    assert_eq!(
        repo_basename(r"C:\Users\RUNNER~1\AppData\Local\Temp\xyz\project-repo"),
        "project-repo",
        "a Windows project_root must yield the repo basename, not the whole path"
    );
    assert_eq!(
        repo_basename(r"C:\Users\RUNNER~1\AppData\Local\Temp\xyz\project-repo\"),
        "project-repo",
        "trailing backslash tolerated"
    );
    // Mixed separators (a POSIX-rooted temp with a backslash tail, or vice versa).
    assert_eq!(
        repo_basename(r"/private/tmp/xyz\project-repo"),
        "project-repo"
    );
}

/// An owner whose bound graph is `bound-repo`, plus a hosted `project-repo`
/// brain bootstrapped through the one-call `ingest {project_root}` path.
async fn owner_with_two_brains(tmp: &Path) -> (Owner, PathBuf, PathBuf) {
    let bound_repo = tmp.join("bound-repo");
    write_bound_repo(&bound_repo);
    let owner = mk_owner(&tmp.join("runtime"));

    // Bound dev graph (the classic single-brain flow).
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

    // Hosted project brain via the one-call bootstrap (leaves the bound graph intact).
    let project_repo = tmp.join("project-repo");
    write_project_repo(&project_repo);
    let sid_p = owner.init_session(&project_repo).await;
    let boot = owner
        .tool(
            &sid_p,
            &project_repo,
            "ingest",
            serde_json::json!({
                "path": project_repo.to_string_lossy(),
                "project_root": project_repo.to_string_lossy(),
                "agent_id": "project-b-agent"
            }),
        )
        .await;
    assert_eq!(
        boot["schema"], "m1nd-project-brain-bootstrap-v0",
        "the project brain must be born through the one-call bootstrap: {boot}"
    );

    (owner, bound_repo, project_repo)
}

/// The enriched instance entries the Hall renders.
fn brains(listing: &serde_json::Value) -> &Vec<serde_json::Value> {
    listing["instances"]
        .as_array()
        .expect("instances_listing must return an `instances` array")
}

/// The owner's own served brain — the MEDULLA (the doctrine-tier root the owner
/// serves). Post-fix it is stamped `brain_kind:"medulla"` (never the serde-default
/// None, which was the pre-fix lie that let it wear a bound workspace's name).
fn bound_of(list: &[serde_json::Value]) -> &serde_json::Value {
    list.iter()
        .find(|b| b["brain_kind"] == "medulla")
        .expect("the owner's own (brain_kind: \"medulla\") entry must be listed")
}

/// The hosted project entry: `brain_kind == "project"`.
fn project_of(list: &[serde_json::Value]) -> &serde_json::Value {
    list.iter()
        .find(|b| b["brain_kind"] == "project")
        .expect("a project (brain_kind: \"project\") entry must be listed")
}

// ---------------------------------------------------------------------------
// (A) BOTH brains listed.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lists_both_the_bound_and_the_hosted_project_brain() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, bound_repo, project_repo) = owner_with_two_brains(tmp.path()).await;

    let listing = instances_listing(&owner.app);
    let list = brains(&listing);

    // Both project roots (canonicalized) must be present — the bound repo AND the
    // hosted project repo, each as an entry's enriched `project_root`.
    let roots: Vec<String> = list
        .iter()
        .filter_map(|b| b["project_root"].as_str())
        .map(|r| {
            Path::new(r)
                .canonicalize()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| r.to_string())
        })
        .collect();
    assert!(
        roots.contains(&canon(&bound_repo)),
        "the bound repo must appear as an entry's project_root: {roots:?}"
    );
    assert!(
        roots.contains(&canon(&project_repo)),
        "the hosted project repo must appear as an entry's project_root: {roots:?}"
    );

    // The hosted project brain has its own entry, distinct from the bound one.
    let hosted = project_of(list);
    assert!(
        Path::new(hosted["project_root"].as_str().unwrap_or(""))
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
            == canon(&project_repo),
        "the project entry's project_root must be the hosted repo: {hosted}"
    );
}

// ---------------------------------------------------------------------------
// (B) display_name is the PROJECT basename, for both brains.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn display_name_is_the_project_basename_never_plumbing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, _bound_repo, _project_repo) = owner_with_two_brains(tmp.path()).await;

    let listing = instances_listing(&owner.app);
    let list = brains(&listing);

    // The owner's own served brain IS the medulla — its card says `medulla`, NOT
    // the basename of the workspace that bound last (the last-bound-project leak).
    assert_eq!(
        bound_of(list)["display_name"].as_str(),
        Some("medulla"),
        "the owner's own brain is the medulla — its name is `medulla`, not the bound workspace basename: {}",
        bound_of(list)
    );
    assert_eq!(
        project_of(list)["display_name"].as_str(),
        Some("project-repo"),
        "the hosted brain's name is its project basename, not the fingerprint store dir: {}",
        project_of(list)
    );
}

// ---------------------------------------------------------------------------
// (B2) the owner's own card is the MEDULLA, honestly (the last-bound-name leak).
//
// Field bug (2026-07-06): after the memory migration the Hall's owner card wore
// the last-bound project's name + `brain_kind:None` — the medulla's identity had
// stuck to the last workspace that bound this runtime. The owner it serves IS the
// medulla (its runtime_root holds the promoted/doctrine store, not a per-project
// brain), so its card must say `medulla` and carry `brain_kind:"medulla"`, with
// its real repo path demoted to `project_root` (the receipt), never the headline.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owner_card_is_the_medulla_not_the_last_bound_workspace() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, bound_repo, _project_repo) = owner_with_two_brains(tmp.path()).await;

    let listing = instances_listing(&owner.app);
    let list = brains(&listing);

    // The owner's own served brain is stamped the medulla kind (never the
    // serde-default None the classic bound graph carried).
    let medulla = bound_of(list);
    assert_eq!(
        medulla["brain_kind"].as_str(),
        Some("medulla"),
        "the owner's own served brain must be stamped brain_kind:\"medulla\": {medulla}"
    );

    // Its card name is the literal `medulla` — NOT the basename of the workspace
    // that bound last (the last-bound-project leak the fix kills).
    assert_eq!(
        medulla["display_name"].as_str(),
        Some("medulla"),
        "the medulla card must read `medulla`, not the bound workspace basename: {medulla}"
    );
    assert_ne!(
        medulla["display_name"].as_str(),
        Some("bound-repo"),
        "the medulla must NOT wear the last-bound workspace's name: {medulla}"
    );

    // The real repo path is not lost — it stays on the entry for the receipt.
    let root = medulla["project_root"].as_str().unwrap_or("");
    assert_eq!(
        Path::new(root)
            .canonicalize()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        canon(&bound_repo),
        "the medulla's real repo path is demoted to project_root, never dropped: {medulla}"
    );
}

// ---------------------------------------------------------------------------
// (C) bound-first ordering — the live owner (freshest heartbeat) leads.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bound_brain_leads_the_list() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, _bound_repo, _project_repo) = owner_with_two_brains(tmp.path()).await;

    let listing = instances_listing(&owner.app);
    let list = brains(&listing);
    // list_instances sorts freshest-heartbeat first; the live owner is the
    // freshest, so its own served brain — the medulla — leads the Hall.
    assert_eq!(
        list[0]["brain_kind"].as_str(),
        Some("medulla"),
        "the brain this owner serves (the medulla) must lead the Hall list: {listing}"
    );
}

// ---------------------------------------------------------------------------
// (D) the naming guard — no plumbing name leaks while a project_root exists.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_display_name_is_a_runtime_or_agent_memory_name() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, _bound_repo, _project_repo) = owner_with_two_brains(tmp.path()).await;

    let listing = instances_listing(&owner.app);
    for b in brains(&listing) {
        let name = b["display_name"].as_str().unwrap_or("");
        let has_root = b["project_root"]
            .as_str()
            .map(|s| !s.is_empty())
            .unwrap_or(false);
        if has_root {
            assert_ne!(name, "agent-memory", "display_name leaked the sidecar: {b}");
            assert_ne!(name, "claude", "display_name leaked the runtime dir: {b}");
            assert_ne!(
                name, "runtimes",
                "display_name leaked a runtime path part: {b}"
            );
            assert!(
                !name.is_empty(),
                "a brain with a project_root must have a name: {b}"
            );
            // Max's screenshot: a project card wore "68c5ce186f6efcd2" — the
            // fingerprint store-dir hash leaking as identity. A display_name may
            // NEVER be a bare 16-char (or longer) hex fingerprint.
            let is_hex_fingerprint =
                name.len() >= 16 && name.chars().all(|c| c.is_ascii_hexdigit());
            assert!(
                !is_hex_fingerprint,
                "display_name is a fingerprint store-dir hash, not a project name: {b}"
            );
            // The medulla is the ONE brain whose name is intentionally NOT its
            // project_root basename: it is the doctrine-tier root and presents as
            // the literal `medulla`, with its real repo path demoted to the receipt.
            if b["brain_kind"] == "medulla" {
                assert_eq!(
                    name, "medulla",
                    "the medulla presents as `medulla`, not its bound workspace basename: {b}"
                );
                continue;
            }
            // Every OTHER brain's name must actually be the basename of its
            // project_root. Compute `expected` separator-agnostically (the SAME rule
            // production uses for display_name) so a Windows backslash project_root
            // yields the repo basename here too — otherwise this guard reds Windows
            // CI while production is correct (the trap #279 fell into).
            let root = b["project_root"].as_str().unwrap();
            let expected = repo_basename(root);
            assert_eq!(
                name, expected,
                "display_name must be the project_root basename: {b}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// (E) project-brain counts — real counts, not "not running" (Max's screenshot).
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn warm_project_brain_reports_real_counts() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, _bound_repo, _project_repo) = owner_with_two_brains(tmp.path()).await;

    let listing = instances_listing(&owner.app);
    let hosted = project_of(brains(&listing));
    // A project brain carries its OWN counts on the entry — never absent for a
    // freshly-ingested brain, never a fabricated 0. (The card must not say
    // "not running": that instance language does not apply to a project brain.)
    assert!(
        hosted["node_count"].as_u64().unwrap_or(0) > 0,
        "a warm project brain must report real node counts on its entry: {hosted}"
    );
    assert!(
        hosted["edge_count"].as_u64().unwrap_or(0) > 0,
        "a warm project brain must report real edge counts on its entry: {hosted}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dormant_project_brain_reports_manifest_counts_after_restart() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = tmp.path().join("runtime");
    let (owner, _bound, _proj) = owner_with_two_brains(tmp.path()).await;
    // Capture the warm counts, then "restart" the owner (fresh registry object,
    // empty brain map) — the project brain is now DORMANT on disk, not warm.
    let warm = instances_listing(&owner.app);
    let warm_hosted = project_of(brains(&warm)).clone();
    let n = warm_hosted["node_count"].as_u64().unwrap();
    let e = warm_hosted["edge_count"].as_u64().unwrap();
    drop(owner);

    let owner2 = mk_owner(&runtime);
    let listing = instances_listing(&owner2.app);
    let hosted = project_of(brains(&listing));
    // Dormant → the counts come from the store manifest (recorded at bootstrap),
    // NOT from a warm graph and NEVER "counts unknown — not running".
    assert_eq!(
        hosted["node_count"].as_u64(),
        Some(n),
        "a dormant project brain must report its manifest-recorded node count: {hosted}"
    );
    assert_eq!(
        hosted["edge_count"].as_u64(),
        Some(e),
        "a dormant project brain must report its manifest-recorded edge count: {hosted}"
    );
    assert!(
        hosted["last_activity_ms"].as_u64().is_some(),
        "a project brain must carry a freshness stamp: {hosted}"
    );
}

// ---------------------------------------------------------------------------
// (F) per-brain session/query PARTITION (ladder R14 / TWO-TIER §9.5.1).
//
// The honesty gap (field-report letter#51): the Hall's G4 aliveness line and the
// instances listing wore the OWNER-GLOBAL session/query counters — sessions on
// OTHER hosted brains inflated a card. R14 partitions on the session's bound
// brain: each brain's entry reports ITS OWN attached-sessions + query-count, and
// the owner-wide total stays only on the owner's own receipt, labeled owner-wide.
//
// The partition is provable because each brain is a full SessionState and routed
// calls dispatch against the brain that owns the caller (mcp_http::route_and_run):
// a tool call carrying `agent_id` records a session (track_agent) on THAT brain,
// and its `queries_processed` counts only that brain's queries. We drive the two
// brains to DISTINCT, divergent counts and assert no card wears the global sum.
// ---------------------------------------------------------------------------

/// The per-brain attached-session count the R14 partition puts on each entry.
fn attached_sessions(entry: &serde_json::Value) -> Option<u64> {
    entry["attached_sessions"].as_u64()
}

/// The per-brain query count the R14 partition puts on each entry.
fn query_count(entry: &serde_json::Value) -> Option<u64> {
    entry["query_count"].as_u64()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn per_brain_counters_partition_on_the_bound_brain_not_owner_global() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (owner, bound_repo, project_repo) = owner_with_two_brains(tmp.path()).await;

    // Drive DIVERGENT traffic so the partition is unmistakable:
    //   bound brain: a SECOND distinct agent hits it (agent "setup" already did
    //     the ingest) → 2 distinct sessions on the bound brain;
    //   project brain: no new agents → its 1 bootstrap session ("project-b-agent")
    //     stands alone.
    // If the counters were owner-global, EITHER card would read the same inflated
    // sum (3 sessions); the partition makes them 2 and 1.
    let sid_extra = owner.init_session(&bound_repo).await;
    owner
        .tool(
            &sid_extra,
            &bound_repo,
            "north",
            serde_json::json!({"task": "bump the bound brain", "agent_id": "bound-second-agent"}),
        )
        .await;

    let listing = instances_listing(&owner.app);
    let list = brains(&listing);
    let bound = bound_of(list);
    let project = project_of(list);

    // ── The partition: each card wears its OWN attached-session count. ──────────
    assert_eq!(
        attached_sessions(bound),
        Some(2),
        "the bound brain's entry must report ONLY its own attached sessions \
         (setup + bound-second-agent = 2), never the owner-global sum: {bound}"
    );
    assert_eq!(
        attached_sessions(project),
        Some(1),
        "the project brain's entry must report ONLY its own attached sessions \
         (project-b-agent = 1), never inflated by the bound brain's sessions: {project}"
    );

    // ── The query count is per-brain too — each card wears its OWN queries. ─────
    // Read each brain's TRUE own `queries_processed` straight from its SessionState
    // and assert the card echoes exactly that. The owner-global counter (a naive
    // implementation) would put the SUM on both cards; the partition puts each
    // brain's own number on its own card.
    let bound_own_q = { owner.app.session.lock().queries_processed };
    let project_key = ProjectBrainRegistry::canonical_key(&project_repo.to_string_lossy());
    let project_own_q = owner
        .app
        .project_brains
        .warm_session_stats(&project_key)
        .expect("the project brain is warm")
        .1;
    let bound_q = query_count(bound).expect("bound entry must carry a per-brain query_count");
    let project_q = query_count(project).expect("project entry must carry a per-brain query_count");
    assert_eq!(
        bound_q, bound_own_q,
        "the bound card's query_count must be the bound brain's OWN queries \
         ({bound_own_q}), not the owner-global sum: {bound}"
    );
    assert_eq!(
        project_q, project_own_q,
        "the project card's query_count must be the project brain's OWN queries \
         ({project_own_q}), never inflated by the bound brain's traffic: {project}"
    );

    // ── The owner-wide total is NOT gone — it lives on the owner's own receipt. ──
    // The bound entry IS the owner's own brain; its per-brain session count IS the
    // owner's own SessionState session count (2), correctly attributed — never the
    // cross-brain sum (which would be 3, folding in the project brain's session).
    let owner_wide_sessions = { owner.app.session.lock().sessions.len() as u64 };
    assert_eq!(
        attached_sessions(bound),
        Some(owner_wide_sessions),
        "the bound/self brain's per-brain count IS the owner's own SessionState \
         session count (2), correctly attributed — not the cross-brain sum: {bound}"
    );
    // The cross-brain sum (bound + project sessions) is what the OLD global counter
    // would have shown on EVERY card. Prove no card wears it.
    let cross_brain_sum = owner_wide_sessions + attached_sessions(project).unwrap();
    assert_ne!(
        attached_sessions(bound),
        Some(cross_brain_sum),
        "the bound card must NOT wear the cross-brain session sum (the honesty gap): {bound}"
    );
    assert_ne!(
        attached_sessions(project),
        Some(cross_brain_sum),
        "the project card must NOT wear the cross-brain session sum (the honesty gap): {project}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dormant_project_brain_omits_live_session_counters_absent_honest() {
    // A brain warm-booted only for listing (no routed session) has no live wire
    // sessions — the per-brain live counters are ABSENT, never a fabricated 0
    // (TT-INV-2). We prove the absent-honest posture on a dormant brain: after a
    // restart the project brain is on disk with no warm SessionState, so its live
    // attached-session count is absent.
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = tmp.path().join("runtime");
    let (owner, _bound, _proj) = owner_with_two_brains(tmp.path()).await;
    drop(owner);

    let owner2 = mk_owner(&runtime);
    let listing = instances_listing(&owner2.app);
    let hosted = project_of(brains(&listing));
    assert!(
        hosted["attached_sessions"].is_null(),
        "a dormant project brain has no live wire sessions — attached_sessions must \
         be absent (null), never a fabricated 0: {hosted}"
    );
}

// ---------------------------------------------------------------------------
// Fixture capture — REAL enriched `/api/instances` output for the UI tests.
//
// Run explicitly to (re)generate the UI fixture from a scratch owner shaped
// like the live :1338 owner (a "m1nd" dev repo + a "project-b" project
// brain), so the Hall's render-proof fixtures are real captured API output, not
// hand-written JSON:
//   cargo test -p m1nd-mcp --features serve --test hall_brains_listing \
//     -- --ignored capture_brains_fixture --nocapture
//
// It writes `brains.captured.json` (raw scratch capture, git-ignored via the
// dir's own hygiene). The COMMITTED `instances.json` — consumed by
// hall-render.test.tsx — keeps this captured SHAPE + the enriched fields
// (display_name, project_root, brain_kind, counts, ordering) but with the
// scratch tmp roots remapped to the real deployment roots they stand in for
// (the tmpdir base is machine-specific and noisy): bound → /Users/kle1nz/m1nd
// (runtime /Users/kle1nz/.m1nd/runtimes/claude, workspace .../agent-memory —
// the exact leak the fix must survive), project →
// /path/to/project-b.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "fixture capture; run explicitly to regenerate the UI __fixtures__ brains capture"]
async fn capture_brains_fixture() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Shape the scratch owner like the real :1338 owner: a "m1nd" dev repo and a
    // "project-b" project brain — so the captured names are the real ones.
    let bound_repo = tmp.path().join("m1nd");
    write_bound_repo(&bound_repo);
    let owner = mk_owner(&tmp.path().join("runtime"));
    let sid = owner.init_session(&bound_repo).await;
    owner
        .tool(
            &sid,
            &bound_repo,
            "ingest",
            serde_json::json!({"path": bound_repo.to_string_lossy(), "agent_id": "setup"}),
        )
        .await;

    let cherry = tmp.path().join("project-b");
    write_project_repo(&cherry);
    let sid_c = owner.init_session(&cherry).await;
    owner
        .tool(
            &sid_c,
            &cherry,
            "ingest",
            serde_json::json!({
                "path": cherry.to_string_lossy(),
                "project_root": cherry.to_string_lossy(),
                "agent_id": "project-b-agent"
            }),
        )
        .await;

    let listing = instances_listing(&owner.app);
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("m1nd-ui")
        .join("src")
        .join("__fixtures__");
    std::fs::create_dir_all(&fixture_dir).expect("mk fixtures dir");
    let out = fixture_dir.join("brains.captured.json");
    std::fs::write(&out, serde_json::to_string_pretty(&listing).unwrap()).expect("write fixture");
    println!("captured enriched /api/instances → {}", out.display());
    println!("{}", serde_json::to_string_pretty(&listing).unwrap());
}
