//! F5-RANKING — minified browser assets must not pollute the rankings.
//!
//! FIELD MEASUREMENT (askGOD F5 verdict, 2026-07-24) on a 103k-node brain: the
//! top of `seek`/`north`/`panoramic` was occupied by single-letter functions
//! from minified JS bundles (`…::fn::s` with `blast_backward=16690`, `…::fn::h`),
//! burying real code. Minifiers rename every symbol to 1–2 characters and route
//! every call site through a handful of helpers, so those helpers collect an
//! enormous in-degree and win PageRank outright — a *lexical* artifact of the
//! build tool, never a statement about the codebase.
//!
//! This battery reproduces BOTH shapes the field showed, in one synthetic
//! corpus, and drives reads through the REAL Streamable-HTTP seam
//! (`handle_mcp_post`) — the exact door the attach bridges hit:
//!   * a genuinely minified asset (`assets/app.js`: one enormous line, symbols
//!     renamed to single letters) — the *content* case;
//!   * named build output (`*.min.js`, `*.js.map`, `*.min.css`) — the
//!     *filename* case;
//!   * an ORDINARY hand-written `src/widget.ts` whose 1-letter helpers `s`/`h`
//!     are called by 40 wrappers — the exact `spawnHistoryStore.ts::fn::s`
//!     shape from the report. Nothing about this file is minified, so only a
//!     ranking-side demote can keep it out of the way.
//!
//! Cases (each PRINTS its measurement, so the truth is in numbers):
//!   N1  discovery      — `*.min.js` bundles and `*.js.map` sourcemaps are
//!                        build output; they contribute NO nodes at all.
//!   N2  pagerank top   — `session_handshake.graph_intelligence.top_pagerank`
//!                        (and `orient`'s attention backbone) must not spend its
//!                        five slots on 1-letter minifier symbols.
//!   N3  seek           — a literal query for a real symbol returns it at
//!                        rank <= 5 with ZERO 1-letter labels in the top-10.
#![cfg(feature = "serve")]

use crate as m1nd_mcp;

use std::path::Path;
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
// Fixture corpus — neutral names only (no other-project names, no personal
// paths). Deterministic: the same bytes every run.
// ---------------------------------------------------------------------------

/// The real code the agent is actually looking for.
fn write_real_source(root: &Path) {
    std::fs::create_dir_all(root.join("src")).expect("mk src");
    std::fs::write(
        root.join("src/handler.py"),
        "\"\"\"Tool dispatch for the fixture service.\"\"\"\n\n\n\
         def handle_function_call(request, registry):\n    \
             \"\"\"Route one tool invocation to the registry entry that owns it.\"\"\"\n    \
             target = registry.lookup(request.name)\n    \
             return target.invoke(request.arguments)\n\n\n\
         def build_call_registry(entries):\n    \
             \"\"\"Assemble the registry consumed by handle_function_call.\"\"\"\n    \
             return {entry.name: entry for entry in entries}\n",
    )
    .expect("write handler.py");
    std::fs::write(
        root.join("src/registry.py"),
        "def lookup(name):\n    return name\n\n\ndef invoke(arguments):\n    return arguments\n",
    )
    .expect("write registry.py");
}

/// A minified-shaped bundle: ONE enormous line, single-letter definitions, and a
/// dense fan-in that turns `a`/`s`/`h` into false hubs — the exact shape measured
/// in the field. Written as a plain `.js` (NOT `*.min.js`) so it exercises the
/// CONTENT heuristic, not the filename rule.
fn minified_bundle_bytes() -> String {
    let mut line = String::new();
    for c in b'a'..=b'z' {
        let name = c as char;
        line.push_str(&format!("function {name}(e,t){{return e+t}}"));
    }
    for i in 0..400 {
        line.push_str(&format!(
            "function wrapper{i}(e){{return a(e,s(e,h(e,{i})))}}"
        ));
    }
    line
}

fn write_bundle_assets(root: &Path) {
    std::fs::create_dir_all(root.join("assets")).expect("mk assets");
    std::fs::write(root.join("assets/app.js"), minified_bundle_bytes()).expect("write app.js");
    // Named build artifacts: skipped outright at discovery.
    std::fs::write(
        root.join("assets/vendor.min.js"),
        "function q(e){return e}function z(e){return q(e)}",
    )
    .expect("write vendor.min.js");
    std::fs::write(
        root.join("assets/vendor.min.js.map"),
        "{\"version\":3,\"sources\":[\"vendor.js\"],\"names\":[\"q\",\"z\"],\"mappings\":\"AAAA\"}",
    )
    .expect("write sourcemap");
    std::fs::write(
        root.join("assets/theme.min.css"),
        ".a{color:#fff}.b{color:#000}",
    )
    .expect("write theme.min.css");
}

/// An ORDINARY, hand-written TypeScript module — normal line lengths, normal
/// whitespace, no build-tool signature — whose two 1-letter helpers collect the
/// whole module's fan-in. This is the `spawnHistoryStore.ts::fn::s` shape from
/// the field report: no discovery rule can exclude it, so the ranking itself has
/// to stop treating a 1-letter symbol as an important hub.
fn write_short_symbol_source(root: &Path) {
    let mut ts = String::from(
        "// Row rendering helpers for the fixture widget.\n\
         export function s(v: number): number {\n  return v * 2;\n}\n\
         export function h(v: number): number {\n  return v + 1;\n}\n",
    );
    for i in 0..40 {
        ts.push_str(&format!(
            "export function renderWidgetRow{i}(v: number): number {{\n  return s(h(v)) + {i};\n}}\n"
        ));
    }
    std::fs::write(root.join("src/widget.ts"), ts).expect("write widget.ts");
}

fn write_corpus(root: &Path) {
    write_real_source(root);
    write_short_symbol_source(root);
    write_bundle_assets(root);
}

// ---------------------------------------------------------------------------
// Owner harness (same shape as the retrieval battery).
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
    let session = Arc::new(BrainSessionCell::new(server.into_session_state()));
    let (event_tx, _rx) = broadcast::channel::<SseEvent>(64);
    let tool_schemas_cache = tool_schemas()
        .get("tools")
        .cloned()
        .unwrap_or(Value::Array(vec![]));
    let project_brains = Arc::new(ProjectBrainRegistry::with_capacity(
        runtime.join("project-brains"),
        Some(runtime.join("registry")),
        8,
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
                        "clientInfo": {"name": "f5-ranking", "version": "0"}}
                }),
            )
            .await;
        minted.expect("initialize mints a session id")
    }

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

    async fn bootstrap(&self, root: &Path, agent: &str) -> (String, u64) {
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
        let n = boot["ingest"]["node_count"]
            .as_u64()
            .or_else(|| boot["node_count"].as_u64())
            .unwrap_or(0);
        assert!(n > 0, "bootstrap must ingest nodes: {boot}");
        (sid, n)
    }
}

// --- measurement helpers ----------------------------------------------------

fn seek_results(seek: &Value) -> Vec<Value> {
    seek["results"].as_array().cloned().unwrap_or_default()
}

fn labels_of(rows: &[Value]) -> Vec<String> {
    rows.iter()
        .map(|r| r["label"].as_str().unwrap_or("").to_string())
        .collect()
}

fn ids_of(rows: &[Value]) -> Vec<String> {
    rows.iter()
        .map(|r| {
            r["node_id"]
                .as_str()
                .or_else(|| r["id"].as_str())
                .unwrap_or("")
                .to_string()
        })
        .collect()
}

/// Labels of exactly one character — the minifier signature.
fn single_letter_labels(rows: &[Value]) -> Vec<String> {
    labels_of(rows)
        .into_iter()
        .filter(|l| l.chars().count() == 1)
        .collect()
}

fn rank_of(rows: &[Value], needle: &str) -> Option<usize> {
    rows.iter().enumerate().find_map(|(i, r)| {
        let id = r["node_id"]
            .as_str()
            .or_else(|| r["id"].as_str())
            .unwrap_or("");
        let label = r["label"].as_str().unwrap_or("");
        (id.contains(needle) || label.contains(needle)).then_some(i + 1)
    })
}

// ===========================================================================
// N1 — DISCOVERY. Named minified artifacts and sourcemaps are build output:
//      they must contribute NO nodes to the graph at all.
// ===========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n1_named_build_artifacts_are_not_ingested() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner(&tmp.path().join("runtime"));
    let root = tmp.path().join("repo");
    write_corpus(&root);
    let (sid, n) = owner.bootstrap(&root, "n1").await;

    // `glob` walks the graph's `file::` nodes — the honest way to ask "did this
    // file reach the graph?" through the real seam. Ask for the WHOLE asset
    // directory so the measurement prints the full inventory, not just a
    // pattern that might silently miss.
    let out = owner
        .tool(
            &sid,
            &root,
            "glob",
            serde_json::json!({"agent_id":"n1","pattern":"assets/*","top_k":100}),
        )
        .await;
    let ingested_assets: Vec<String> = out["files"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|f| f["file_path"].as_str().unwrap_or("").to_string())
        .collect();
    let leaked: Vec<&String> = ingested_assets
        .iter()
        .filter(|p| p.contains(".min.") || p.ends_with(".map"))
        .collect();

    eprintln!(
        "N1 MEASURE: graph_nodes={n} total_matches={} assets_in_graph={ingested_assets:?} build_artifacts={leaked:?}",
        out["total_matches"]
    );
    assert!(
        leaked.is_empty(),
        "N1: minified bundles / sourcemaps reached the graph: {leaked:?}"
    );
}

// ===========================================================================
// N2 — PAGERANK TOP. The minifier's `a`/`s`/`h` helpers collect the whole
//      bundle's fan-in and win PageRank outright. They must not spend the five
//      structural-importance slots that an agent reads to orient.
// ===========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n2_pagerank_top_is_not_minifier_symbols() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner(&tmp.path().join("runtime"));
    let root = tmp.path().join("repo");
    write_corpus(&root);
    let (sid, n) = owner.bootstrap(&root, "n2").await;

    let hs = owner
        .tool(
            &sid,
            &root,
            "session_handshake",
            serde_json::json!({"agent_id":"n2"}),
        )
        .await;
    let top: Vec<Value> = hs["graph_intelligence"]["top_pagerank"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let orient = owner
        .tool(
            &sid,
            &root,
            "orient",
            serde_json::json!({"agent_id":"n2","task":"route a tool call to its registry entry"}),
        )
        .await;
    let anchors: Vec<Value> = orient["anchors"]
        .as_array()
        .cloned()
        .unwrap_or_else(|| panic!("orient must return an `anchors` array: {orient}"));
    assert!(
        !anchors.is_empty(),
        "N2: orient returned an EMPTY attention backbone — the assertion below \
         would be vacuous: {orient}"
    );

    let top_noise = single_letter_labels(&top);
    let anchor_noise = single_letter_labels(&anchors);
    eprintln!(
        "N2 MEASURE: graph_nodes={n}\n  top_pagerank labels={:?} single_letter={top_noise:?}\n  attention_backbone labels={:?} single_letter={anchor_noise:?}",
        labels_of(&top),
        labels_of(&anchors)
    );

    assert!(
        top_noise.is_empty(),
        "N2: top_pagerank is polluted by {} single-letter minifier symbol(s) {top_noise:?}; ids={:?}",
        top_noise.len(),
        ids_of(&top)
    );
    assert!(
        anchor_noise.is_empty(),
        "N2: orient's attention backbone is polluted by {} single-letter minifier symbol(s) {anchor_noise:?}",
        anchor_noise.len()
    );
}

// ===========================================================================
// N3 — SEEK. A literal query for a real symbol must return it near the top,
//      with no minifier symbols in the visible window.
// ===========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn n3_literal_seek_beats_the_minified_hub() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner(&tmp.path().join("runtime"));
    let root = tmp.path().join("repo");
    write_corpus(&root);
    let (sid, n) = owner.bootstrap(&root, "n3").await;

    let seek = owner
        .tool(
            &sid,
            &root,
            "seek",
            serde_json::json!({"agent_id":"n3","query":"handle_function_call","top_k":10}),
        )
        .await;
    let rows = seek_results(&seek);
    let top10: Vec<Value> = rows.iter().take(10).cloned().collect();
    let rank = rank_of(&rows, "handle_function_call");
    let noise = single_letter_labels(&top10);

    eprintln!(
        "N3 MEASURE: graph_nodes={n} hits={} rank_of_real_symbol={rank:?} top10={:?} single_letter={noise:?}",
        rows.len(),
        labels_of(&top10)
    );

    assert!(
        matches!(rank, Some(r) if r <= 5),
        "N3: the literal symbol must rank <= 5, got {rank:?}; top10={:?}",
        labels_of(&top10)
    );
    assert!(
        noise.is_empty(),
        "N3: seek top-10 carries {} single-letter minifier symbol(s) {noise:?}",
        noise.len()
    );
}
