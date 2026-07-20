//! ORGANISM ladder R6 — the delegation layer (`delegate` / `debrief`, Slices 1-2).
//! Drives the REAL Streamable-HTTP routing (`handle_mcp_post`) in-process for
//! Ordinary `delegate` reads and pins the public fail-closed boundary for the
//! elevated `debrief` mutation. Until an exact typed G2 consumer exists,
//! debrief success semantics are exercised only through the owner-internal
//! domain dispatcher; this file does not claim a public typed debrief path.
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

use crate as m1nd_mcp;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::http::HeaderMap;
use m1nd_control::{ActionId, AuthorityVariant, Effect, Ingress};
use m1nd_mcp::brain_runtime::BrainSessionCell;
use serde_json::{json, Value};
use tokio::sync::broadcast;

use m1nd_mcp::delegation_handlers::binding_workspace_root;
use m1nd_mcp::http_server::{AppState, SseEvent};
use m1nd_mcp::mcp_http::{handle_mcp_post, new_mcp_session_registry};
use m1nd_mcp::project_brains::ProjectBrainRegistry;
use m1nd_mcp::runtime_jobs::{
    RuntimeJobAuthorityBindingV1, RuntimeJobBindingV1, RuntimeJobFailure, RuntimeJobRequestV1,
    RuntimeJobState, RuntimeJobSuccess, RuntimeJobWait, RUNTIME_JOB_AUTHORITY_SCHEMA,
    RUNTIME_JOB_BINDING_SCHEMA,
};
use m1nd_mcp::server::{tool_schemas, McpConfig, McpServer};

const ORCH: &str = "claude:main:orchestrator";
const SUB: &str = "codex:sub:maker";
static NEXT_FIXTURE_JOB: AtomicU64 = AtomicU64::new(1);

// ---------------------------------------------------------------------------
// Fixture repo — a small, deterministic Rust repo with clearly-labeled symbols
// so activation is non-empty and file paths resolve.
// ---------------------------------------------------------------------------

fn write_repo_a(root: &Path) {
    std::fs::create_dir_all(root.join("src")).expect("mk src");
    std::fs::write(root.join("Cargo.toml"), "[package]\nname=\"repo-a\"\n").expect("toml");
    std::fs::write(
        root.join("src/lib.rs"),
        "pub mod handler;\npub mod router;\n",
    )
    .expect("lib.rs");
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

fn mk_owner_with_bound_ingest(runtime: &Path, bound_root: Option<&Path>) -> Owner {
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
    if let Some(root) = bound_root {
        session_state.caller_root = Some(root.to_string_lossy().to_string());
        let ingest = m1nd_mcp::server::dispatch_tool(
            &mut session_state,
            "ingest",
            &json!({"path": root.to_string_lossy(), "agent_id": "setup"}),
        )
        .expect("preseed bound graph before actor startup");
        assert!(
            ingest["node_count"].as_u64().unwrap_or(0) > 0,
            "bound ingest must produce nodes: {ingest}"
        );
    }
    let session = Arc::new(BrainSessionCell::new(session_state));
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
            ui_authority: Arc::new(m1nd_mcp::ui_attestation::UiBundleAttestor::default()),
            mission_service: None,
            external_mutation_service: None,
            authority_service: None,
            autonomy_owner: None,
        }),
        runtime: runtime.to_path_buf(),
    }
}

fn mk_owner(runtime: &Path) -> Owner {
    mk_owner_with_bound_ingest(runtime, None)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis()
        .try_into()
        .expect("milliseconds fit")
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

    /// Exercise debrief's domain semantics without pretending the public
    /// generic MCP route is authorized. This is deliberately debrief-only: the
    /// production ingress remains fail-closed at SCOPED_GRANT_A2 until a typed
    /// G2 consumer exists.
    fn debrief_internal(&self, caller_root: &Path, args: Value) -> Result<Value, String> {
        let caller_root = ProjectBrainRegistry::canonical_key(&caller_root.to_string_lossy());
        self.app
            .project_brains
            .try_resolve(&caller_root)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("project brain did not resolve for {caller_root}"))?;
        let revision = self
            .app
            .project_brains
            .read_runtime_snapshot(&caller_root, |_state| Ok::<_, RuntimeJobFailure>(()))
            .map_err(|error| error.to_string())?
            .version
            .revision;
        let sequence = NEXT_FIXTURE_JOB.fetch_add(1, Ordering::Relaxed);
        let job_id = format!("delegation-debrief-fixture-{}-{sequence}", now_ms());
        let request = RuntimeJobRequestV1 {
            job_id: job_id.clone(),
            idempotency_key: format!("idem-{job_id}"),
            binding: RuntimeJobBindingV1 {
                schema: RUNTIME_JOB_BINDING_SCHEMA.to_string(),
                organism_id: "organism-delegation-test".to_string(),
                brain_id: self.app.project_brains.brain_id_for(&caller_root),
                mission_id: "mission-delegation-test".to_string(),
                agent_id: ORCH.to_string(),
                action: ActionId::new("graph.background-mutation").expect("fixture action id"),
                ingress: Ingress::BackgroundJob,
                effects: BTreeSet::from([Effect::GraphMutation, Effect::RuntimeStoreWrite]),
                authority: RuntimeJobAuthorityBindingV1 {
                    schema: RUNTIME_JOB_AUTHORITY_SCHEMA.to_string(),
                    decision_id: format!("decision-{job_id}"),
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
        let (result_tx, result_rx) = mpsc::sync_channel(1);
        let dispatch_root = caller_root.clone();
        let submitted = self
            .app
            .project_brains
            .submit_runtime_job(
                &caller_root,
                request,
                |_state| Ok::<_, RuntimeJobFailure>(()),
                move |context, _snapshot| {
                    context.checkpoint()?;
                    Ok::<_, RuntimeJobFailure>(args)
                },
                move |state, args| {
                    let previous_root = state.caller_root.replace(dispatch_root);
                    let dispatched = m1nd_mcp::server::dispatch_tool(state, "debrief", &args)
                        .map_err(|error| error.to_string());
                    state.caller_root = previous_root;
                    match dispatched {
                        Ok(value) => {
                            let _ = result_tx.send(Ok(value));
                            Ok(RuntimeJobSuccess::new(
                                "debrief_applied",
                                "owner-internal debrief executed through brain actor",
                            ))
                        }
                        Err(error) => {
                            let _ = result_tx.send(Err(error.clone()));
                            Err(RuntimeJobFailure::new("debrief_refused", error))
                        }
                    }
                },
            )
            .map_err(|error| error.to_string())?;
        let jobs = self
            .app
            .project_brains
            .runtime_job_registry()
            .map_err(|error| error.to_string())?;
        let terminal = jobs
            .wait_terminal(&submitted, Duration::from_secs(5))
            .map_err(|error| error.to_string())?;
        let job = match terminal {
            RuntimeJobWait::Terminal(job) => job,
            RuntimeJobWait::ObservableNonTerminal(job) => {
                return Err(format!("debrief fixture job did not finish: {job:?}"));
            }
        };
        let dispatched = result_rx
            .recv_timeout(Duration::from_secs(1))
            .map_err(|error| format!("debrief fixture result missing: {error}"))?;
        if job.state != RuntimeJobState::Succeeded {
            return dispatched.and_then(|_| {
                Err(job
                    .terminal_result
                    .map(|result| result.message)
                    .unwrap_or_else(|| "debrief fixture job failed".to_string()))
            });
        }
        dispatched
    }

    async fn bootstrap(&self, root: &Path, agent: &str) -> String {
        // Fixture setup is owner-internal: public generic `ingest` is an A2
        // mutation and must remain fail-closed without a typed G2 consumer.
        // Preseed through the registry's real bootstrap implementation, then
        // initialize the wire session so the tests still exercise automatic
        // caller-root routing through `handle_mcp_post`.
        let (_brain, boot, _reused) = self
            .app
            .project_brains
            .bootstrap(
                &root.to_string_lossy(),
                &json!({
                    "path": root.to_string_lossy(),
                    "project_root": root.to_string_lossy(),
                    "agent_id": agent
                }),
            )
            .expect("preseed project brain through the owner-internal bootstrap");
        assert!(
            boot["node_count"].as_u64().unwrap_or(0) > 0,
            "bootstrap must ingest: {boot}"
        );
        self.init_session(root).await
    }

    /// memorize a sentinel claim into the store the session is routed to.
    async fn memorize(&self, sid: &str, caller_root: &Path, agent: &str, label: &str, text: &str) {
        let out = self
            .tool(
                sid,
                caller_root,
                "memorize",
                json!({
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
    let named_for_snapshot = named.to_string();
    let (binding, covers_named, covers_foreign) = owner
        .app
        .project_brains
        .read_runtime_snapshot(&root.to_string_lossy(), move |state| {
            Ok::<_, RuntimeJobFailure>((
                binding_workspace_root(state),
                state.covers_root(&named_for_snapshot),
                state.covers_root("/nonexistent/foreign/repo"),
            ))
        })
        .expect("read child binding through routed brain actor")
        .value;
    assert_eq!(
        binding.as_deref(),
        Some(named),
        "the packet's binding is EXACTLY binding_workspace_root — one datum, two hops"
    );
    assert!(
        covers_named,
        "a child landing at the named root is COVERED → silent reception"
    );
    assert!(
        !covers_foreign,
        "a mismatched child is NOT covered → reception block"
    );
}

// ---------------------------------------------------------------------------
// Slice 2 — debrief: conformance, the ledger, memorize provenance, hard errors.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn public_debrief_is_a2_frozen_without_typed_g2_consumer() {
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
            json!({ "agent_id": ORCH, "task": "extend route_request" }),
        )
        .await;
    let id = packet["delegation_id"].as_str().unwrap().to_string();

    let error = owner
        .tool_raw(
            &sid,
            &root,
            "debrief",
            json!({
                "agent_id": ORCH,
                "subagent_id": SUB,
                "delegation_id": id,
                "outcome": "success",
                "touched_paths": ["src/router.rs"]
            }),
        )
        .await
        .expect_err("public generic debrief must fail closed without typed G2 authority");
    assert!(
        error.contains("generic_action_authority_required") && error.contains("SCOPED_GRANT_A2"),
        "the refusal must name the exact A2 authority boundary: {error}"
    );
    assert!(
        find_file(&tmp.path().join("owner"), "outcomes.jsonl").is_none(),
        "a refused public debrief must not append an outcome ledger row"
    );
}

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

    // The subagent touched an in-scope file, a known dependent, AND a path the
    // packet could not have predicted.
    let debrief = owner
        .debrief_internal(
            &root,
            json!({
                "agent_id": ORCH,
                "subagent_id": SUB,
                "delegation_id": id,
                "outcome": "partial",
                "touched_paths": [
                    "src/router.rs",
                    "src/handler.rs",
                    "scratch/untracked-note.txt"
                ],
                "findings": ["route_offset is coupled to route_request via handle_route"]
            }),
        )
        .expect("owner-internal debrief domain dispatch");

    assert_eq!(debrief["schema"], "m1nd-debrief-v0", "{debrief}");
    // worst-of verdict carries fence existence honestly.
    let verdict = debrief["conformance"]["verdict"].as_str().unwrap();
    assert!(
        verdict.contains("unpredicted") || verdict.contains("stayed"),
        "verdict must be a worst-of string with fence existence: {verdict}"
    );
    // A path absent from the graph/predicted packet is graded as unpredicted.
    assert!(
        debrief["conformance"]["counts"]["unpredicted"]
            .as_u64()
            .unwrap_or(0)
            >= 1,
        "scratch/untracked-note.txt was outside the predicted map → unpredicted: {debrief}"
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
    let err = owner.debrief_internal(
        &root,
        json!({ "agent_id": ORCH, "delegation_id": "dlg_does_not_exist", "outcome": "success" }),
    );
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
        .debrief_internal(
            &root,
            json!({
                "agent_id": ORCH,
                "subagent_id": SUB,
                "delegation_id": id1,
                "outcome": "success",
                "touched_paths": ["src/router.rs"],
                "findings": ["route_request must treat an empty path as length zero, not panic"]
            }),
        )
        .expect("owner-internal debrief domain dispatch");

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
// M7 (ORGANISM R7) — the packet memory slice carries `tier` + `origin_brain`.
//
// THE LAW (MEDULLA-PRD §8.2, §6 · NEXTGEN §O.12.4): every row of the delegation
// packet's `context.memory` is LABELED cargo — a child sees not just the claim but
// WHICH tier (project | medulla) and WHICH brain it was born in. Doctrine is
// distinguishable from project fact; inheritance is auditable, not ambient.
//
// RED before M7: the packet's memory rows carried only {claim, age_days,
// source_agent, stale} — a child could not tell a medulla doctrine claim from a
// project fact, and no origin brain was named. This two-brain fixture is the GREEN
// that could not exist before the row-labeling lands.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m7_packet_memory_rows_carry_tier_and_origin_brain_two_brain() {
    let tmp = tempfile::tempdir().expect("tmp");

    // A doctrine claim in the medulla (the bound owner IS the medulla today).
    let bound = tmp.path().join("bound-repo");
    write_repo_a(&bound);
    let owner = mk_owner_with_bound_ingest(&tmp.path().join("owner"), Some(&bound));
    let sid_bound = owner.init_session(&bound).await;
    let doctrine = "route dispatch doctrine: always verify the route table size";
    owner
        .memorize(&sid_bound, &bound, "maintainer", doctrine, doctrine)
        .await;

    // A project brain (repo-a) with its OWN project claim.
    let root = tmp.path().join("repo-a");
    write_repo_a(&root);
    let sid = owner.bootstrap(&root, ORCH).await;
    let project_claim = "route_request returns the path length; route_table_size is fixed at 7.";
    owner
        .memorize(
            &sid,
            &root,
            ORCH,
            "route_request length rule",
            project_claim,
        )
        .await;

    // The mother delegates from the project brain — its DEFAULT beat is
    // project + medulla, so the packet must carry BOTH tiers, each labeled.
    let packet = owner
        .tool(
            &sid,
            &root,
            "delegate",
            json!({
                "agent_id": ORCH,
                "task": "extend the request router so route_request handles an empty route path",
                "scope": { "paths": ["src/router.rs"] }
            }),
        )
        .await;

    let mem = packet["context"]["memory"]
        .as_array()
        .expect("memory slice is an array");

    // EVERY row is labeled: no bare {claim, age, author} rows survive.
    for row in mem {
        assert!(
            row.get("tier").and_then(|v| v.as_str()).is_some(),
            "every packet memory row must carry a `tier` label (project|medulla): {row}"
        );
        assert!(
            row.get("origin_brain").and_then(|v| v.as_str()).is_some(),
            "every packet memory row must carry an `origin_brain` label (never faked): {row}"
        );
    }

    // THE PROJECT ROW: labeled tier=project, origin_brain = the repo it was born in.
    // (The `claim` field renders the node label — the `route_request length rule`
    // claim we memorized above.)
    let project_row = mem
        .iter()
        .find(|r| {
            r["claim"]
                .as_str()
                .map(|c| c.contains("route_request length rule"))
                .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("the project claim must appear, labeled: {packet}"));
    assert_eq!(
        project_row["tier"].as_str(),
        Some("project"),
        "the project fact is tier=project: {project_row}"
    );
    assert!(
        project_row["origin_brain"]
            .as_str()
            .unwrap_or_default()
            .contains("repo-a"),
        "the project row's origin_brain names the brain it was born in (repo-a): {project_row}"
    );

    // THE MEDULLA ROW: the doctrine claim, labeled tier=medulla, origin=medulla —
    // folded into the project brain's default beat, distinguishable from the fact.
    let medulla_row = mem
        .iter()
        .find(|r| {
            r["claim"]
                .as_str()
                .map(|c| c.contains("always verify the route table size"))
                .unwrap_or(false)
        })
        .unwrap_or_else(|| {
            panic!("the medulla doctrine claim MUST surface in the packet, labeled: {packet}")
        });
    assert_eq!(
        medulla_row["tier"].as_str(),
        Some("medulla"),
        "the doctrine claim is tier=medulla: {medulla_row}"
    );
    assert_eq!(
        medulla_row["origin_brain"].as_str(),
        Some("medulla"),
        "a doctrine-born claim's origin brain is medulla: {medulla_row}"
    );

    // THE CHILD READS THE MARKDOWN: the re-rendered prompt must show the labeled
    // rows (doctrine distinguishable from project fact), not just the JSON.
    let md = packet["prompt_markdown"].as_str().expect("prompt_markdown");
    assert!(
        md.contains("[medulla]") && md.contains("[project]"),
        "the rendered packet must label memory rows by tier so the child SEES doctrine vs fact: {md}"
    );
    assert!(
        md.contains("always verify the route table size"),
        "the folded medulla doctrine must reach the child's prompt, not only the JSON: {md}"
    );
}

/// Legacy tolerance: a memory row whose claim has NO `Origin-Brain` provenance (a
/// pre-M5a `.light.md`, or any hit the graph tag never stamped) must still render —
/// falling back to the store's own identity — never a faked or absent label. The
/// project brain's own store answers with its own origin, so a lone-brain packet
/// still labels every row (unknown is honest, never a crash).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m7_lone_brain_packet_still_labels_every_row_with_its_own_origin() {
    let tmp = tempfile::tempdir().expect("tmp");
    let owner = mk_owner(&tmp.path().join("owner"));
    let root = tmp.path().join("repo-a");
    write_repo_a(&root);
    let sid = owner.bootstrap(&root, ORCH).await;
    owner
        .memorize(
            &sid,
            &root,
            ORCH,
            "route table size",
            "route_table_size is fixed at 7.",
        )
        .await;

    let packet = owner
        .tool(
            &sid,
            &root,
            "delegate",
            json!({
                "agent_id": ORCH,
                "task": "confirm route_table_size stays fixed at 7",
                "scope": { "paths": ["src/router.rs"] }
            }),
        )
        .await;

    let mem = packet["context"]["memory"]
        .as_array()
        .expect("memory slice is an array");
    assert!(
        !mem.is_empty(),
        "the lone brain's own claim must surface: {packet}"
    );
    for row in mem {
        assert_eq!(
            row["tier"].as_str(),
            Some("project"),
            "a project brain's own rows are tier=project: {row}"
        );
        assert!(
            row["origin_brain"]
                .as_str()
                .map(|o| !o.is_empty())
                .unwrap_or(false),
            "every row carries a non-empty origin_brain (its own store's identity as fallback): {row}"
        );
    }
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
