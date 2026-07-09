//! MEDULLA ladder R8 / slice M7b — the per-project mailbox, end-to-end.
//! Drives the REAL routing + read surface: an owner whose bound graph is `repo-a`
//! (ingested through the wire, exactly as a live session binds), then the box
//! read path the `GET /api/mailbox` handler wraps (`resolve_brain` → repo-side
//! box). The pure distribution/fate/sweep logic is unit-tested in
//! `mailbox::tests`; this file pins the CROSSING + the §4A.9 selector reuse.
//!
//! THE LAWS UNDER TEST (MEDULLA-PRD §9.2, HUMAN-LAYER §4A.11, ORGANISM §C2.2):
//!   - distribution files every letter into exactly ONE box (repo variants → that
//!     repo's box; projectless → the medulla box); idempotent.
//!   - MED-INV-10: no repo-bearing letter EVER lands in the medulla box.
//!   - `GET /api/mailbox` reads ONLY the resolved brain's repo-side box with the
//!     `served_brain` echo (INV-17); `medulla` selector → the medulla box.
//!   - fates derived; external excluded from the "abertas" count.
//!   - `inbox_sweep` unions spool ∪ boxes (each letter once) and names unreachable.
//!   - an unknown brain selector is an honest miss (never a filesystem read).
//!
//! NEUTRAL FIXTURES ONLY — `repo-a` / `repo-b` / temp dirs. No project names, no
//! personal paths in this file (the LOCAL live boxes the distributor writes DO
//! carry project names — that is fine; boxes are local per-repo files).
#![cfg(feature = "serve")]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Bytes;
use axum::http::HeaderMap;
use parking_lot::Mutex;
use serde_json::Value;
use tokio::sync::broadcast;

use m1nd_mcp::http_server::{resolve_brain, AppState, SseEvent};
use m1nd_mcp::mcp_http::{handle_mcp_post, new_mcp_session_registry};
use m1nd_mcp::project_brains::ProjectBrainRegistry;
use m1nd_mcp::server::{tool_schemas, McpConfig, McpServer};

// --- fixture helpers --------------------------------------------------------

fn write_repo(root: &Path, tag: &str) {
    std::fs::create_dir_all(root.join("src")).expect("mk repo src");
    std::fs::write(
        root.join("src/lib.rs"),
        format!("pub fn {tag}_probe() -> i64 {{ 42 }}\npub struct {tag}Thing;\n"),
    )
    .expect("lib.rs");
    std::fs::write(
        root.join("Cargo.toml"),
        format!("[package]\nname=\"{tag}\"\n"),
    )
    .expect("toml");
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "m1nd-mailbox-it-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("mk scratch");
    dir
}

struct Owner {
    app: Arc<AppState>,
    runtime: PathBuf,
}

/// Boot an owner whose runtime root is `<base>/.m1nd/runtimes/x` — shaped so the
/// derived spool (`spool_path_for_runtime`) resolves INSIDE the scratch, keeping
/// the test hermetic (never the real `~/.m1nd`).
fn mk_owner(base: &Path) -> Owner {
    let runtime = base.join(".m1nd").join("runtimes").join("x");
    std::fs::create_dir_all(&runtime).unwrap();
    let config = McpConfig {
        graph_source: runtime.join("graph_snapshot.json"),
        plasticity_state: runtime.join("plasticity_state.json"),
        runtime_dir: Some(runtime.clone()),
        registry_dir: Some(runtime.join("registry")),
        ..Default::default()
    };
    let server = McpServer::new(config).expect("boot owner");
    let session = Arc::new(Mutex::new(server.into_session_state()));
    let (event_tx, _rx) = broadcast::channel::<SseEvent>(64);
    let tool_schemas_cache = tool_schemas()
        .get("tools")
        .cloned()
        .unwrap_or(Value::Array(vec![]));
    let project_brains = Arc::new(ProjectBrainRegistry::with_capacity(
        runtime.join("project-brains"),
        Some(runtime.join("registry")),
        4,
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
        runtime,
    }
}

impl Owner {
    async fn post(
        &self,
        sid: Option<&str>,
        caller: Option<&Path>,
        body: Value,
    ) -> (Value, Option<String>) {
        let mut headers = HeaderMap::new();
        if let Some(s) = sid {
            headers.insert("mcp-session-id", s.parse().unwrap());
        }
        if let Some(r) = caller {
            headers.insert("m1nd-caller-root", r.to_string_lossy().parse().unwrap());
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
            .unwrap();
        (
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            minted,
        )
    }

    async fn init(&self, caller: &Path) -> String {
        let (_b, minted) = self
            .post(
                None,
                Some(caller),
                serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize",
                    "params":{"protocolVersion":"2025-06-18","capabilities":{},
                        "clientInfo":{"name":"m7b","version":"0"}}}),
            )
            .await;
        minted.expect("initialize mints a session id")
    }

    async fn tool(&self, sid: &str, caller: &Path, name: &str, args: Value) -> Value {
        let (body, _) = self
            .post(
                Some(sid),
                Some(caller),
                serde_json::json!({"jsonrpc":"2.0","id":7,"method":"tools/call",
                    "params":{"name":name,"arguments":args}}),
            )
            .await;
        let text = body["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("tool {name} no text: {body}"));
        serde_json::from_str(text).unwrap_or_else(|e| panic!("tool {name} not JSON ({e}): {text}"))
    }

    /// Bind this owner's bound graph to `repo` by ingesting it (the real bind).
    async fn bind_to(&self, repo: &Path) {
        let sid = self.init(repo).await;
        let ingest = self
            .tool(
                &sid,
                repo,
                "ingest",
                serde_json::json!({"path": repo.to_string_lossy(), "agent_id":"setup"}),
            )
            .await;
        assert!(
            ingest["node_count"].as_u64().unwrap_or(0) > 0,
            "bind ingest must produce nodes: {ingest}"
        );
    }
}

/// The box read path the `GET /api/mailbox` handler wraps: resolve the brain (the
/// §4A.9 selector) then read its repo-side box (or the medulla box for `medulla`).
fn mailbox_get(owner: &Owner, brain: Option<&str>, foreign: &BTreeSet<String>) -> Value {
    if brain
        .map(|b| b.eq_ignore_ascii_case("medulla"))
        .unwrap_or(false)
    {
        let box_path = m1nd_mcp::mailbox::medulla_box_path(&owner.runtime);
        let view = m1nd_mcp::mailbox::read_box(&box_path, foreign).unwrap();
        let mut v = serde_json::to_value(view).unwrap();
        v.as_object_mut().unwrap().insert(
            "served_brain".into(),
            serde_json::json!({"project_root":"medulla","display_name":"medulla"}),
        );
        return v;
    }
    let (session, served) = resolve_brain(&owner.app, brain).unwrap();
    let repo_root = session.lock().project_root_display().unwrap();
    let box_path = Path::new(&repo_root).join(m1nd_mcp::mailbox::BOX_REL_PATH);
    let view = m1nd_mcp::mailbox::read_box(&box_path, foreign).unwrap();
    let mut v = serde_json::to_value(view).unwrap();
    v.as_object_mut()
        .unwrap()
        .insert("served_brain".into(), served);
    v
}

// --- the tests --------------------------------------------------------------

#[tokio::test]
async fn distribution_med_inv_10_and_idempotence() {
    let base = scratch("distrib");
    let owner = mk_owner(&base);
    let repo_a = base.join("repo-a");
    let repo_b = base.join("repo-b");
    write_repo(&repo_a, "a");
    write_repo(&repo_b, "b");

    let spool = m1nd_mcp::mailbox::spool_path_for_runtime(&owner.runtime);
    std::fs::create_dir_all(spool.parent().unwrap()).unwrap();
    let lines = [
        serde_json::json!({"ts":"2026-07-05T10:00:00Z","agent":"a","repo":"repo-a","tool":"seek","class":"bug","what":"a1"}).to_string(),
        serde_json::json!({"ts":"2026-07-05T11:00:00Z","agent":"a","repo":"repo-a-soul","tool":"seek","class":"friction","what":"a2 worktree"}).to_string(),
        serde_json::json!({"ts":"2026-07-05T12:00:00Z","agent":"a","repo":"repo-b","tool":"seek","class":"win","what":"b1"}).to_string(),
        serde_json::json!({"ts":"2026-07-05T13:00:00Z","agent":"a","repo":"all","tool":"seek","class":"friction","what":"projectless"}).to_string(),
    ];
    std::fs::write(&spool, lines.join("\n") + "\n").unwrap();

    let mut known = std::collections::BTreeMap::new();
    known.insert("repo-a".to_string(), repo_a.clone());
    known.insert("repo-b".to_string(), repo_b.clone());

    let receipt = m1nd_mcp::mailbox::distribute(&spool, &owner.runtime, "repo-a", &known).unwrap();
    assert_eq!(receipt.to_project, 3, "2 repo-a variants + 1 repo-b");
    assert_eq!(receipt.to_medulla, 1, "the projectless letter only");

    // MED-INV-10: the medulla box holds ONLY the projectless letter.
    let med = m1nd_mcp::mailbox::medulla_box_path(&owner.runtime);
    let med_letters = m1nd_mcp::mailbox::read_letters(&med).unwrap();
    assert_eq!(med_letters.len(), 1);
    assert_eq!(med_letters[0].what, "projectless");

    // repo-a's box: only its own two, never repo-b's.
    let a_letters = m1nd_mcp::mailbox::read_letters(&repo_a.join(".m1nd/inbox.jsonl")).unwrap();
    assert_eq!(a_letters.len(), 2);
    assert!(
        !a_letters.iter().any(|l| l.what == "b1"),
        "no repo-b leak into box-a"
    );

    // consent-deferred birth: a .gitignore covers inbox.jsonl.
    let gi = std::fs::read_to_string(repo_a.join(".m1nd/.gitignore")).unwrap();
    assert!(gi.contains("inbox.jsonl"));

    // idempotence: a second distribution appends nothing.
    let again = m1nd_mcp::mailbox::distribute(&spool, &owner.runtime, "repo-a", &known).unwrap();
    assert_eq!(again.appended, 0);
}

#[tokio::test]
async fn api_mailbox_scopes_to_brain_and_echoes_served_brain() {
    let base = scratch("apiget");
    let owner = mk_owner(&base);
    let repo_a = base.join("repo-a");
    write_repo(&repo_a, "a");
    owner.bind_to(&repo_a).await;

    // Seed repo-a's box with two letters; the read must return exactly these.
    let box_a = repo_a.join(".m1nd/inbox.jsonl");
    std::fs::create_dir_all(box_a.parent().unwrap()).unwrap();
    std::fs::write(
        &box_a,
        [
            serde_json::json!({"ts":"2026-07-05T10:00:00Z","agent":"a","repo":"repo-a","tool":"seek","class":"bug","what":"open bug"}).to_string(),
            serde_json::json!({"ts":"2026-07-05T11:00:00Z","agent":"a","repo":"repo-a","tool":"seek","class":"win","what":"a win"}).to_string(),
        ]
        .join("\n")
            + "\n",
    )
    .unwrap();

    let v = mailbox_get(&owner, None, &BTreeSet::new());
    let letters = v["letters"].as_array().unwrap();
    assert_eq!(letters.len(), 2, "only repo-a's letters (INV-17)");
    let served_root = v["served_brain"]["project_root"].as_str().unwrap();
    assert!(
        Path::new(served_root)
            .canonicalize()
            .map(|p| p == repo_a.canonicalize().unwrap())
            .unwrap_or(false),
        "served_brain echoes the bound repo-a root (got {served_root})"
    );
    // Two open letters (wet_ink), zero external → abertas = 2.
    assert_eq!(v["counts"]["wet_ink"].as_u64().unwrap(), 2);
    assert_eq!(v["counts"]["external"].as_u64().unwrap(), 0);
}

#[tokio::test]
async fn api_mailbox_medulla_selector_reads_the_medulla_box() {
    let base = scratch("apimed");
    let owner = mk_owner(&base);
    let med = m1nd_mcp::mailbox::medulla_box_path(&owner.runtime);
    std::fs::write(
        &med,
        serde_json::json!({"ts":"2026-07-05T10:00:00Z","agent":"ctx","repo":"all","tool":"context7","class":"friction","what":"transversal report"}).to_string()
            + "\n",
    )
    .unwrap();

    let foreign: BTreeSet<String> = ["context7"].iter().map(|s| s.to_string()).collect();
    let v = mailbox_get(&owner, Some("medulla"), &foreign);
    let letters = v["letters"].as_array().unwrap();
    assert_eq!(letters.len(), 1);
    assert_eq!(letters[0]["what"].as_str().unwrap(), "transversal report");
    // The Context7 letter is external → visible but never counted (wet_ink = 0).
    assert_eq!(v["counts"]["external"].as_u64().unwrap(), 1);
    assert_eq!(v["counts"]["wet_ink"].as_u64().unwrap(), 0);
    assert_eq!(v["counts"]["in_flight"].as_u64().unwrap(), 0);
}

#[tokio::test]
async fn unknown_brain_selector_is_an_honest_miss() {
    let base = scratch("apimiss");
    let owner = mk_owner(&base);
    let err = resolve_brain(&owner.app, Some("/path/to/nonexistent-repo"));
    assert!(
        err.is_err(),
        "unknown root is refused, never a filesystem read"
    );
}

#[tokio::test]
async fn sweep_unions_spool_and_boxes_naming_unreachable() {
    let base = scratch("sweep");
    let owner = mk_owner(&base);
    let repo_a = base.join("repo-a");
    write_repo(&repo_a, "a");
    std::fs::create_dir_all(repo_a.join(".m1nd")).unwrap();

    let spool = m1nd_mcp::mailbox::spool_path_for_runtime(&owner.runtime);
    std::fs::create_dir_all(spool.parent().unwrap()).unwrap();
    let shared = serde_json::json!({"ts":"2026-07-05T10:00:00Z","agent":"a","repo":"repo-a","tool":"seek","class":"bug","what":"shared"}).to_string();
    let spool_only = serde_json::json!({"ts":"2026-07-05T11:00:00Z","agent":"a","repo":"all","tool":"seek","class":"friction","what":"spool only"}).to_string();
    std::fs::write(&spool, [shared.clone(), spool_only].join("\n") + "\n").unwrap();
    let box_only = serde_json::json!({"ts":"2026-07-05T12:00:00Z","agent":"o","repo":"repo-a","tool":"seek","class":"win","what":"box only"}).to_string();
    std::fs::write(
        repo_a.join(".m1nd/inbox.jsonl"),
        [shared, box_only].join("\n") + "\n",
    )
    .unwrap();

    let boxes = vec![
        m1nd_mcp::mailbox::KnownBox {
            label: "repo-a".into(),
            path: repo_a.join(".m1nd/inbox.jsonl"),
            reachable: true,
        },
        m1nd_mcp::mailbox::KnownBox {
            label: "repo-gone".into(),
            path: base.join("gone/.m1nd/inbox.jsonl"),
            reachable: false,
        },
    ];
    let sweep = m1nd_mcp::mailbox::inbox_sweep(&spool, &boxes, &BTreeSet::new()).unwrap();
    assert_eq!(sweep.total, 3, "shared counted once → 3 distinct");
    assert_eq!(sweep.unreachable, vec!["repo-gone".to_string()]);
}
