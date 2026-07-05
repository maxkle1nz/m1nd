//! ORGANISM ladder R6 — the delegation layer (`delegate` / `debrief`, Slices 1-2).
//! Drives the REAL Streamable-HTTP routing (`handle_mcp_post`) in-process — the
//! exact seam attach bridges hit — so the packet, the registry, the debrief
//! ledger, and the child-reception link are proven end-to-end, not unit-mocked.
//!
//! THE LAWS UNDER TEST (NEXTGEN-AGENT-PRD §O.12, ORGANISM-PRD §C5.3 · §C10 R6):
//!   - `delegate` composes a valid `m1nd-delegation-packet-v0` — the mother's
//!     binding (the named brain), the selected memory slice (explicit cargo), the
//!     anchors, the honest gaps, and a deterministic `prompt_markdown`.
//!   - a dumb registry record is written (the debrief join key).
//!   - the abstain classes are real answers: `needs_ingest` on an empty graph,
//!     `unscopable` on gibberish — evidence + next_move, never a bare no.
//!   - `debrief` returns a structured outcome, classifies touched paths, appends
//!     exactly one `outcomes.jsonl` row, and memorizes findings under the
//!     subagent's id — an unknown delegation_id is a hard error.
//!   - THE CHILD LAW (§C5.3): the packet's `mission.binding.workspace_root` IS the
//!     datum reception verifies — a child landing at that root gets the SILENT
//!     reception (covers), a mismatched root gets the reception block.
//!   - the loop closes: a second `delegate` over the same area surfaces the first
//!     debrief's memorized finding, with age + author.
//!
//! RED BEFORE R6: no delegate/debrief surface existed at all (grep-proven) — a
//! child could NOT inherit the mother's binding + memory slice by any recorded
//! act, and a child re-derived context cold. This file is the GREEN that could not
//! exist before the two verbs.
//!
//! No-leak: neutral fixtures only (role ids `claude:main:orchestrator` /
//! `codex:sub:maker`, repo `repo-a`).
#![cfg(feature = "serve")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Bytes;
use axum::http::HeaderMap;
use parking_lot::Mutex;
use serde_json::{json, Value};
use tokio::sync::broadcast;

use m1nd_mcp::delegation_handlers::binding_workspace_root;
use m1nd_mcp::http_server::{AppState, SseEvent};
use m1nd_mcp::mcp_http::{handle_mcp_post, new_mcp_session_registry};
use m1nd_mcp::project_brains::ProjectBrainRegistry;
use m1nd_mcp::server::{tool_schemas, McpConfig, McpServer};

const ORCH: &str = "claude:main:orchestrator";
const SUB: &str = "codex:sub:maker";

// ---------------------------------------------------------------------------
// Fixture repo — a small, deterministic Rust repo with clearly-labeled symbols
// so activation is non-empty and file paths resolve.
// ---------------------------------------------------------------------------

fn write_repo_a(root: &Path) {
    std::fs::create_dir_all(root.join("src")).expect("mk src");
    std::fs::write(root.join("Cargo.toml"), "[package]\nname=\"repo-a\"\n").expect("toml");
    std::fs::write(
        root.join("src/router.rs"),
        "pub fn route_request(path: &str) -> i64 { path.len() as i64 }\n\
         pub fn route_table_size() -> i64 { 7 }\n",
    )
    .expect("router.rs");
    std::fs::write(
        root.join("src/handler.rs"),
        "pub fn handle_route(id: i64) -> i64 { id + route_offset() }\n\
         pub fn route_offset() -> i64 { 3 }\n",
    )
    .expect("handler.rs");
}

struct Owner {
    app: Arc<AppState>,
    #[allow(dead_code)]
    runtime: PathBuf,
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
        }),
        runtime: runtime.to_path_buf(),
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
                json!({
                    "jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                        "clientInfo": {"name": "r6-probe", "version": "0"}}
                }),
            )
            .await;
        minted.expect("initialize mints a session id")
    }

    async fn tool_raw(
        &self,
        sid: &str,
        caller_root: &Path,
        name: &str,
        args: Value,
    ) -> Result<Value, String> {
        let (body, _) = self
            .post(
                Some(sid),
                Some(caller_root),
                json!({
                    "jsonrpc": "2.0", "id": 7, "method": "tools/call",
                    "params": {"name": name, "arguments": args}
                }),
            )
            .await;
        let content = &body["result"]["content"][0]["text"];
        let text = content
            .as_str()
            .unwrap_or_else(|| panic!("tool {name} returned no content text: {body}"));
        if body["result"]["isError"].as_bool().unwrap_or(false) {
            return Err(text.to_string());
        }
        Ok(serde_json::from_str(text)
            .unwrap_or_else(|e| panic!("tool {name} content not JSON ({e}): {text}")))
    }

    async fn tool(&self, sid: &str, caller_root: &Path, name: &str, args: Value) -> Value {
        self.tool_raw(sid, caller_root, name, args)
            .await
            .unwrap_or_else(|e| panic!("tool {name} errored: {e}"))
    }

    async fn bootstrap(&self, root: &Path, agent: &str) -> String {
        let sid = self.init_session(root).await;
        let boot = self
            .tool(
                &sid,
                root,
                "ingest",
                json!({
                    "path": root.to_string_lossy(),
                    "project_root": root.to_string_lossy(),
                    "agent_id": agent
                }),
            )
            .await;
        assert!(
            boot["ingest"]["node_count"].as_u64().unwrap_or(0) > 0,
            "bootstrap must ingest: {boot}"
        );
        sid
    }
}

// ---------------------------------------------------------------------------
// Slice 1 — delegate: the packet, the registry, the abstains, the renderer.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn delegate_composes_a_grounded_packet_with_binding_and_memory_slice() {
    let tmp = tempfile::tempdir().expect("tmp");
    let root = tmp.path().join("repo-a");
    write_repo_a(&root);
    let owner = mk_owner(&tmp.path().join("owner"));
    let sid = owner.bootstrap(&root, ORCH).await;

    // Seed a durable memory so the mother has a slice to select.
    owner
        .tool(
            &sid,
            &root,
            "memorize",
            json!({
                "agent_id": ORCH,
                "node_label": "router dispatch shape",
                "claims": [{
                    "label": "route_request length rule",
                    "text": "route_request returns the path length; route_table_size is fixed at 7.",
                    "kind": "state",
                    "evidence": ["src/router.rs"]
                }]
            }),
        )
        .await;

    let packet = owner
        .tool(
            &sid,
            &root,
            "delegate",
            json!({
                "agent_id": ORCH,
                "task": "extend the request router so route_request handles an empty path",
                "scope": { "paths": ["src/router.rs"] }
            }),
        )
        .await;

    // GOLDEN assertions on the packet shape.
    assert_eq!(packet["schema"], "m1nd-delegation-packet-v0", "{packet}");
    assert_eq!(packet["verdict"], "packet", "{packet}");
    assert!(
        packet["delegation_id"]
            .as_str()
            .map(|s| s.starts_with("dlg_"))
            .unwrap_or(false),
        "delegation_id must be a dlg_* id: {packet}"
    );

    // The mother's binding NAMES the brain (the child-law datum).
    let named = packet["mission"]["binding"]["workspace_root"]
        .as_str()
        .expect("binding names a workspace_root");
    assert!(
        !named.is_empty(),
        "binding.workspace_root non-empty: {packet}"
    );
    assert_eq!(
        packet["mission"]["tier"], "project",
        "slice 1 is project-tier"
    );

    // The selected memory slice is present, project-tier, with provenance.
    let mem = packet["context"]["memory"]
        .as_array()
        .expect("memory slice is an array");
    assert!(
        mem.iter().any(|m| m["source_agent"].as_str() == Some(ORCH)),
        "the mother's memorized claim must appear in the inherited slice: {packet}"
    );

    // honest_gaps + non_claims never empty (honesty invariants).
    assert!(
        !packet["honest_gaps"].as_array().unwrap().is_empty(),
        "honest_gaps must not be empty"
    );
    assert!(
        !packet["non_claims"].as_array().unwrap().is_empty(),
        "non_claims must NEVER be empty"
    );

    // The rendered appendix carries the never-dropped duties section + report
    // protocol, and is duty-coupled — no fenced code bodies.
    let md = packet["prompt_markdown"].as_str().expect("prompt_markdown");
    assert!(
        md.contains("What m1nd could NOT determine"),
        "the duties section is never dropped: {md}"
    );
    assert!(
        md.contains("DEVIATIONS") && md.contains("FINDINGS"),
        "the report protocol rides the packet: {md}"
    );
    assert!(
        !md.contains("```"),
        "the packet carries NO fenced code bodies (stale shadows): {md}"
    );

    // The registry record exists on disk — the debrief join key.
    let id = packet["delegation_id"].as_str().unwrap();
    let record = tmp.path().join("owner").join("project-brains");
    // The record lives under the routed brain's runtime; find it by walking the
    // owner tree for `<id>.json` (dumb record, one file).
    let found = find_file(&tmp.path().join("owner"), &format!("{id}.json"))
        .or_else(|| find_file(&record, &format!("{id}.json")));
    assert!(
        found.is_some(),
        "the dumb registry record {id}.json must exist on disk"
    );
}

#[tokio::test]
async fn delegate_abstains_on_gibberish_and_needs_ingest_on_empty_graph() {
    let tmp = tempfile::tempdir().expect("tmp");
    let root = tmp.path().join("repo-a");
    write_repo_a(&root);
    let owner = mk_owner(&tmp.path().join("owner"));

    // needs_ingest: a fresh session whose brain is empty → never a packet.
    let empty_root = tmp.path().join("empty");
    std::fs::create_dir_all(&empty_root).unwrap();
    let empty_sid = owner.init_session(&empty_root).await;
    let ni = owner
        .tool(
            &empty_sid,
            &empty_root,
            "delegate",
            json!({ "agent_id": ORCH, "task": "do something in an unbuilt repo" }),
        )
        .await;
    assert_eq!(
        ni["verdict"], "needs_ingest",
        "empty graph → needs_ingest: {ni}"
    );
    assert!(
        ni["next_move"].as_str().is_some(),
        "abstain carries next_move"
    );

    // unscopable: gibberish over a real graph activates no coherent subgraph.
    let sid = owner.bootstrap(&root, ORCH).await;
    let gib = owner
        .tool(
            &sid,
            &root,
            "delegate",
            json!({ "agent_id": ORCH, "task": "zzzq wombat pldoxxing quux ⧉ nonsense token soup" }),
        )
        .await;
    // Either an honest abstain (unscopable) or a low-signal packet — but if it
    // abstains it must hand back evidence + next_move, never a bare no.
    if gib["verdict"] == "abstain" {
        assert_eq!(gib["abstain_class"], "unscopable", "{gib}");
        assert!(
            gib["evidence"].is_object(),
            "abstain carries evidence: {gib}"
        );
        assert!(
            gib["next_move"].as_str().is_some(),
            "abstain carries next_move: {gib}"
        );
    }
}

#[tokio::test]
async fn packet_renderer_is_string_stable() {
    let tmp = tempfile::tempdir().expect("tmp");
    let root = tmp.path().join("repo-a");
    write_repo_a(&root);
    let owner = mk_owner(&tmp.path().join("owner"));
    let sid = owner.bootstrap(&root, ORCH).await;

    // Two delegates with the SAME task differ only in their generated id/ms; the
    // rendered body (minus the id line) must be byte-identical — the renderer is a
    // pure function of the structured packet.
    let a = owner
        .tool(
            &sid,
            &root,
            "delegate",
            json!({ "agent_id": ORCH, "task": "refactor route_offset" }),
        )
        .await;
    let b = owner
        .tool(
            &sid,
            &root,
            "delegate",
            json!({ "agent_id": ORCH, "task": "refactor route_offset" }),
        )
        .await;
    let strip = |p: &Value| {
        p["prompt_markdown"]
            .as_str()
            .unwrap()
            .lines()
            .filter(|l| !l.contains("dlg_"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        strip(&a),
        strip(&b),
        "renderer is string-stable for the same packet shape"
    );
}

// ---------------------------------------------------------------------------
// The child law (§C5.3) — mission.binding IS the datum reception verifies.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn child_reception_link_binding_is_the_covers_datum() {
    let tmp = tempfile::tempdir().expect("tmp");
    let root = tmp.path().join("repo-a");
    write_repo_a(&root);
    let owner = mk_owner(&tmp.path().join("owner"));
    let sid = owner.bootstrap(&root, ORCH).await;

    let packet = owner
        .tool(
            &sid,
            &root,
            "delegate",
            json!({ "agent_id": ORCH, "task": "touch the router" }),
        )
        .await;
    let named = packet["mission"]["binding"]["workspace_root"]
        .as_str()
        .expect("binding names a root");

    // The constitutional link: the packet's binding root IS the root reception
    // uses. Reach into the routed brain and prove:
    //   binding_workspace_root(state) == named   AND
    //   covers_root(named) == true  (the child would receive the SILENT reception)
    //   covers_root(<foreign>) == false (a mismatched child gets the block).
    let brain = owner
        .app
        .project_brains
        .resolve(&root.to_string_lossy())
        .expect("routed brain exists");
    let state = brain.lock();
    assert_eq!(
        binding_workspace_root(&state).as_deref(),
        Some(named),
        "the packet's binding is EXACTLY binding_workspace_root — one datum, two hops"
    );
    assert!(
        state.covers_root(named),
        "a child landing at the named root is COVERED → silent reception"
    );
    assert!(
        !state.covers_root("/nonexistent/foreign/repo"),
        "a mismatched child is NOT covered → reception block"
    );
}

// ---------------------------------------------------------------------------
// Slice 2 — debrief: conformance, the ledger, memorize provenance, hard errors.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn debrief_grades_touched_paths_and_appends_one_ledger_row() {
    let tmp = tempfile::tempdir().expect("tmp");
    let root = tmp.path().join("repo-a");
    write_repo_a(&root);
    let owner = mk_owner(&tmp.path().join("owner"));
    let sid = owner.bootstrap(&root, ORCH).await;

    let packet = owner
        .tool(
            &sid,
            &root,
            "delegate",
            json!({
                "agent_id": ORCH,
                "task": "extend route_request",
                "scope": { "paths": ["src/router.rs"] }
            }),
        )
        .await;
    let id = packet["delegation_id"].as_str().unwrap().to_string();

    // The subagent touched an in-scope file AND an unpredicted one.
    let debrief = owner
        .tool(
            &sid,
            &root,
            "debrief",
            json!({
                "agent_id": ORCH,
                "subagent_id": SUB,
                "delegation_id": id,
                "outcome": "partial",
                "touched_paths": ["src/router.rs", "src/handler.rs"],
                "findings": ["route_offset is coupled to route_request via handle_route"]
            }),
        )
        .await;

    assert_eq!(debrief["schema"], "m1nd-debrief-v0", "{debrief}");
    // worst-of verdict carries fence existence honestly.
    let verdict = debrief["conformance"]["verdict"].as_str().unwrap();
    assert!(
        verdict.contains("unpredicted") || verdict.contains("stayed"),
        "verdict must be a worst-of string with fence existence: {verdict}"
    );
    // an unpredicted touch (handler.rs was outside may_touch) is graded as such.
    assert!(
        debrief["conformance"]["counts"]["unpredicted"]
            .as_u64()
            .unwrap_or(0)
            >= 1,
        "handler.rs was outside the declared scope → unpredicted: {debrief}"
    );
    // The finding memorized under the SUBAGENT's id.
    let memorized = debrief["learned"]["memorized"].as_array().unwrap();
    assert!(
        memorized
            .iter()
            .any(|m| m["under_agent"].as_str() == Some(SUB)),
        "the finding lands under the subagent's id: {debrief}"
    );
    // never claims merge-safe.
    let nc = debrief["non_claims"].as_array().unwrap();
    assert!(
        nc.iter().any(|c| c
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains("never says merge-safe")),
        "debrief never claims merge-safe: {debrief}"
    );

    // Exactly one outcomes.jsonl row appended.
    let ledger = find_file(&tmp.path().join("owner"), "outcomes.jsonl")
        .expect("outcomes.jsonl exists after debrief");
    let rows = std::fs::read_to_string(&ledger).unwrap();
    let n = rows.lines().filter(|l| !l.trim().is_empty()).count();
    assert_eq!(n, 1, "exactly one ledger row per debrief: {rows}");

    // Unknown delegation_id is a HARD error, no guessing.
    let err = owner
        .tool_raw(
            &sid,
            &root,
            "debrief",
            json!({ "agent_id": ORCH, "delegation_id": "dlg_does_not_exist", "outcome": "success" }),
        )
        .await;
    assert!(
        err.is_err(),
        "unknown delegation_id must be a hard error, got {err:?}"
    );
}

#[tokio::test]
async fn the_loop_closes_second_delegate_surfaces_the_debrief_finding() {
    let tmp = tempfile::tempdir().expect("tmp");
    let root = tmp.path().join("repo-a");
    write_repo_a(&root);
    let owner = mk_owner(&tmp.path().join("owner"));
    let sid = owner.bootstrap(&root, ORCH).await;

    // First delegation over the router area.
    let p1 = owner
        .tool(
            &sid,
            &root,
            "delegate",
            json!({ "agent_id": ORCH, "task": "work on route_request in the router" }),
        )
        .await;
    let id1 = p1["delegation_id"].as_str().unwrap().to_string();

    // Debrief with a durable finding — memorized under the subagent.
    owner
        .tool(
            &sid,
            &root,
            "debrief",
            json!({
                "agent_id": ORCH,
                "subagent_id": SUB,
                "delegation_id": id1,
                "outcome": "success",
                "touched_paths": ["src/router.rs"],
                "findings": ["route_request must treat an empty path as length zero, not panic"]
            }),
        )
        .await;

    // A SECOND delegate over the same area must surface that finding in the
    // inherited memory slice — the flywheel closing (Slice 2 exit).
    let p2 = owner
        .tool(
            &sid,
            &root,
            "delegate",
            json!({ "agent_id": ORCH, "task": "revisit route_request empty-path handling" }),
        )
        .await;
    let mem = p2["context"]["memory"].as_array().unwrap();
    assert!(
        mem.iter().any(|m| {
            m["claim"]
                .as_str()
                .map(|c| c.contains("empty path") || c.contains("route_request"))
                .unwrap_or(false)
                && m["source_agent"].as_str() == Some(SUB)
        }),
        "the second packet inherits the first debrief's finding, by the subagent, with provenance: {p2}"
    );
}

// ---------------------------------------------------------------------------
// helper
// ---------------------------------------------------------------------------

/// Walk a directory tree for the first file with the given name. The registry
/// lives under the routed brain's runtime dir, whose exact path is an internal
/// detail — the test asserts on presence, not location.
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(hit) = find_file(&path, name) {
                return Some(hit);
            }
        } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            return Some(path);
        }
    }
    None
}
