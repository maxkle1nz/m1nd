//! RETRIEVAL BATTERY — the measured reproduction of the m1nd "reads fail to
//! find" field pains (owner-authorized 2026-07-12/13). Doctrine: a battery case
//! BEFORE any fix. Each case is NUMBERED, drives the REAL Streamable-HTTP seam
//! (`handle_mcp_post`) in-process — the exact door the attach bridges hit — and
//! PRINTS its measurement (score / rank / latency) so the truth is in numbers.
//!
//! The honest diagnosis under test: memory WRITING works; memory READING (recall
//! by meaning) fails to surface what was written. Sources (field-reports.jsonl):
//!   C1  write-then-seek        — 2026-07-12T00:19 codex-reson (false_absence)
//!   C2  medulla recall         — 2026-07-12T00:15 claude-orchestrator
//!   C3  cross_verify honesty   — 2026-07-12T11:28 codex-game (missing_from_graph)
//!   C4  north memory beat       — chronic "N claims, none surfaced"
//!   C5  federate clock          — 2026-07-12T18:11 / 18:28 codex-redsky (hang)
//!   C6  baseline latency        — reference numbers for the test brain
//!
//! GREEN cases become permanent regressions. RED cases are marked
//! `#[ignore = "the fix target: <case>"]` with the measured number in the
//! comment — the CI stays green while the truth stays recorded.
#![cfg(feature = "serve")]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use axum::body::Bytes;
use axum::http::HeaderMap;
use parking_lot::Mutex;
use serde_json::Value;
use tokio::sync::broadcast;

use m1nd_mcp::http_server::{AppState, SseEvent};
use m1nd_mcp::mcp_http::{handle_mcp_post, new_mcp_session_registry};
use m1nd_mcp::project_brains::ProjectBrainRegistry;
use m1nd_mcp::server::{tool_schemas, McpConfig, McpServer};

// ---------------------------------------------------------------------------
// Fixture repos — tiny, deterministic, DISTINCT sentinels. Neutral names only
// (no other-project names, no personal paths) — the no-leak rule.
// ---------------------------------------------------------------------------

fn write_repo(root: &Path, tag: &str) {
    std::fs::create_dir_all(root.join("src")).expect("mk repo src");
    std::fs::write(
        root.join("src/lib.rs"),
        format!(
            "/// {tag} widget module: a small deterministic fixture.\n\
             pub fn {tag}_probe() -> i64 {{ 42 }}\n\
             pub struct {tag}Widget {{ pub v: i64 }}\n\
             pub fn {tag}_scale(w: &{tag}Widget, k: i64) -> i64 {{ w.v * k }}\n"
        ),
    )
    .expect("write lib.rs");
    std::fs::write(root.join("Cargo.toml"), "[package]\nname=\"fx\"\n").expect("write toml");
}

/// Generate a repo of a controlled size: `files` source files, each with
/// `per_file` distinct functions plus a struct. Returns nothing; the ingested
/// node count is measured at bootstrap time and reported by the case.
fn gen_sized_repo(root: &Path, tag: &str, files: usize, per_file: usize) {
    std::fs::create_dir_all(root.join("src")).expect("mk repo src");
    std::fs::write(root.join("Cargo.toml"), "[package]\nname=\"fx\"\n").expect("write toml");
    let mut lib = String::from("// generated fixture crate\n");
    for f in 0..files {
        lib.push_str(&format!("pub mod {tag}_mod_{f};\n"));
        let mut m = String::new();
        m.push_str(&format!("pub struct {tag}S{f} {{ pub v: i64 }}\n"));
        for k in 0..per_file {
            m.push_str(&format!(
                "pub fn {tag}_f{f}_{k}(x: i64) -> i64 {{ x + {f} * {k} }}\n"
            ));
        }
        std::fs::write(root.join("src").join(format!("{tag}_mod_{f}.rs")), m).expect("write mod");
    }
    std::fs::write(root.join("src/lib.rs"), lib).expect("write lib.rs");
}

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
                        "clientInfo": {"name": "battery", "version": "0"}}
                }),
            )
            .await;
        minted.expect("initialize mints a session id")
    }

    /// A session with NO caller-root header — routes to the owner's own medulla
    /// store (the doctrine store), the seat a promoted/doctrine-born law lives in.
    async fn init_session_medulla(&self) -> String {
        let (_b, minted) = self
            .post(
                None,
                None,
                serde_json::json!({
                    "jsonrpc": "2.0", "id": 1, "method": "initialize",
                    "params": {"protocolVersion": "2025-06-18", "capabilities": {},
                        "clientInfo": {"name": "battery", "version": "0"}}
                }),
            )
            .await;
        minted.expect("initialize mints a session id")
    }

    /// tools/call with NO caller-root header (medulla-store calls).
    async fn tool_medulla(&self, sid: &str, name: &str, args: Value) -> Value {
        let (body, _) = self
            .post(
                Some(sid),
                None,
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

    /// Bootstrap a project brain from `root` (one-call ingest); returns its sid
    /// and the ingested node count.
    async fn bootstrap(&self, root: &Path, agent: &str) -> (String, u64) {
        let sid = self.init_session(root).await;
        let boot = self
            .tool(
                &sid,
                root,
                "ingest",
                serde_json::json!({
                    "path": root.to_string_lossy(),
                    "project_root": root.to_string_lossy(),
                    "agent_id": agent
                }),
            )
            .await;
        let n = boot["ingest"]["node_count"]
            .as_u64()
            .or_else(|| boot["node_count"].as_u64())
            .unwrap_or(0);
        assert!(n > 0, "bootstrap must ingest nodes: {boot}");
        (sid, n)
    }

    /// memorize a sentinel claim in the store the session is routed to. Returns
    /// the raw memorize response for measurement.
    async fn memorize(
        &self,
        sid: &str,
        caller_root: &Path,
        agent: &str,
        label: &str,
        text: &str,
    ) -> Value {
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
        out
    }
}

// --- measurement helpers ----------------------------------------------------

fn seek_results(seek: &Value) -> Vec<Value> {
    seek["results"].as_array().cloned().unwrap_or_default()
}

/// Rank (1-based) and score of the first result whose node_id or label contains
/// `needle`. None when the needle never appears in the result set.
fn rank_of(results: &[Value], needle: &str) -> Option<(usize, f64)> {
    results.iter().enumerate().find_map(|(i, r)| {
        let id = r["node_id"].as_str().unwrap_or("");
        let label = r["label"].as_str().unwrap_or("");
        if id.contains(needle) || label.contains(needle) {
            Some((i + 1, r["score"].as_f64().unwrap_or(0.0)))
        } else {
            None
        }
    })
}

/// The memory feed of a north packet.
fn north_memory(north: &Value) -> Vec<Value> {
    north["memory"].as_array().cloned().unwrap_or_default()
}

// ===========================================================================
// C6 — BASELINE latency (GREEN, permanent). Reference numbers for the test
//      brain: a plain keyword seek and a north beat must return promptly.
// ===========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c6_baseline_latency() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner(&tmp.path().join("runtime"));
    let root = tmp.path().join("repo");
    write_repo(&root, "Baseline");
    let (sid, n) = owner.bootstrap(&root, "c6").await;

    let t0 = Instant::now();
    let seek = owner
        .tool(
            &sid,
            &root,
            "seek",
            serde_json::json!({"agent_id":"c6","query":"probe widget scale"}),
        )
        .await;
    let seek_ms = t0.elapsed().as_secs_f64() * 1000.0;

    let t1 = Instant::now();
    let north = owner
        .tool(
            &sid,
            &root,
            "north",
            serde_json::json!({"agent_id":"c6","task":"understand the widget scale path"}),
        )
        .await;
    let north_ms = t1.elapsed().as_secs_f64() * 1000.0;

    let hits = seek_results(&seek).len();
    let emb = seek["embeddings_used"].as_bool().unwrap_or(false);
    eprintln!(
        "C6 MEASURE: graph_nodes={n} seek_hits={hits} embeddings_used={emb} seek_ms={seek_ms:.1} north_ms={north_ms:.1} north_mem_rows={}",
        north_memory(&north).len()
    );

    assert!(
        hits > 0,
        "C6: baseline keyword seek must find code nodes: {seek}"
    );
    assert!(
        seek_ms < 10_000.0,
        "C6: seek latency {seek_ms:.1}ms exceeded 10s budget"
    );
    assert!(
        north_ms < 10_000.0,
        "C6: north latency {north_ms:.1}ms exceeded 10s budget"
    );
}

// ===========================================================================
// C1 — WRITE-THEN-SEEK. memorize a distinct claim, then recall it in the SAME
//      runtime by (a) an exact label token and (b) a paraphrase (meaning only,
//      zero token overlap). Then reload the runtime and recall again.
// ===========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c1_write_then_seek() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = tmp.path().join("runtime");
    let root = tmp.path().join("repo");
    write_repo(&root, "Cadence");

    let sentinel = "OrbitalCadenceCovenant";
    let body = "the release train waits for a green three-operating-system verification matrix before any publish is allowed";

    // Distinct paraphrase: shares MEANING with `body`, shares NO token with the
    // sentinel label and (deliberately) none of the body's rarest words.
    let paraphrase = "when is it safe to ship a build across every platform";

    let (found_exact, found_para, found_reload);
    let (rank_exact, score_exact);
    let (rank_para, score_para, emb_para);
    {
        let owner = mk_owner(&runtime);
        let (sid, _n) = owner.bootstrap(&root, "c1").await;
        let mem = owner.memorize(&sid, &root, "c1", sentinel, body).await;
        assert!(
            mem["ingested"].as_bool().unwrap_or(false),
            "C1: memorize must ingest the claim: {mem}"
        );

        // (a) exact label token.
        let s_exact = owner
            .tool(
                &sid,
                &root,
                "seek",
                serde_json::json!({"agent_id":"c1","query":sentinel}),
            )
            .await;
        let re = rank_of(&seek_results(&s_exact), sentinel);
        found_exact = re.is_some();
        (rank_exact, score_exact) = re.map(|(r, s)| (r as i64, s)).unwrap_or((-1, 0.0));
        // Dump the claim node's searchable shape (report 105 asks: is the BODY
        // searchable, or only the label?).
        if let Some(hit) = seek_results(&s_exact).into_iter().find(|r| {
            r["node_id"].as_str().unwrap_or("").contains("orbital")
                || r["label"].as_str().unwrap_or("").contains(sentinel)
        }) {
            eprintln!(
                "C1 PROBE-SHAPE: node_type={} label={:?} excerpt={:?}",
                hit["node_type"], hit["label"], hit["excerpt"]
            );
        }
        // (a2) report-105 faithful: an EXACT distinctive term that lives in the
        // claim BODY but NOT the label ("verification matrix").
        let s_body = owner
            .tool(
                &sid,
                &root,
                "seek",
                serde_json::json!({"agent_id":"c1","query":"verification matrix"}),
            )
            .await;
        let rb = rank_of(&seek_results(&s_body), "orbital").or_else(|| {
            seek_results(&s_body).iter().enumerate().find_map(|(i, r)| {
                r["label"]
                    .as_str()
                    .filter(|l| l.contains("verification"))
                    .map(|_| (i + 1, r["score"].as_f64().unwrap_or(0.0)))
            })
        });
        eprintln!(
            "C1 PROBE-BODY: exact body-term 'verification matrix' -> found={} rank/score={:?}",
            rb.is_some(),
            rb
        );

        // (b) paraphrase — pure meaning.
        let s_para = owner
            .tool(
                &sid,
                &root,
                "seek",
                serde_json::json!({"agent_id":"c1","query":paraphrase}),
            )
            .await;
        let rp = rank_of(&seek_results(&s_para), sentinel);
        found_para = rp.is_some();
        (rank_para, score_para) = rp.map(|(r, s)| (r as i64, s)).unwrap_or((-1, 0.0));
        emb_para = s_para["embeddings_used"].as_bool().unwrap_or(false);
    }

    // Reload: a brand-new owner on the SAME runtime dir (agent memory auto-loads
    // at boot). Recall the exact token — persistence across a runtime restart.
    {
        let owner = mk_owner(&runtime);
        let sid = owner.init_session(&root).await;
        let s = owner
            .tool(
                &sid,
                &root,
                "seek",
                serde_json::json!({"agent_id":"c1","query":sentinel}),
            )
            .await;
        found_reload = rank_of(&seek_results(&s), sentinel).is_some();
    }

    eprintln!(
        "C1 MEASURE: exact[found={found_exact} rank={rank_exact} score={score_exact:.3}] \
         paraphrase[found={found_para} rank={rank_para} score={score_para:.3} emb={emb_para}] \
         reload_exact_found={found_reload}"
    );

    // The permanent floor: a memorized claim MUST be recallable by an exact
    // distinctive token, in-session AND across a reload. (The paraphrase /
    // meaning direction is asserted separately — that is where the pain lives.)
    assert!(
        found_exact,
        "C1: exact-token recall of a just-memorized claim FAILED"
    );
    assert!(
        found_reload,
        "C1: exact-token recall FAILED after runtime reload"
    );
}

// ===========================================================================
// C1b — the DECIDING NUMBER. Measures the RAW static-embedding cosine between a
//       claim body and a spectrum of queries, so the paraphrase miss in C1 is
//       attributed correctly: a mistuned recall floor (surgical) vs. a static
//       model that cannot bridge the paraphrase at all (redesign, not surgical).
//       SEMANTIC_RECALL_FLOOR = 0.40 (m1nd-mcp/src/layer_handlers.rs:73).
// ===========================================================================
#[cfg(feature = "embed")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c1b_embedding_cosine_probe() {
    use m1nd_core::embed::{cosine, Embedder, Model2VecEmbedder};
    let embedder = match Model2VecEmbedder::from_default() {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "C1b MEASURE: embedder unavailable ({e}); semantic recall runs in trigram fallback"
            );
            return;
        }
    };
    let body = "the release train waits for a green three-operating-system verification matrix before any publish is allowed";
    let bv = embedder.embed(body);
    let probes = [
        ("identity", body),
        (
            "near_body",
            "release waits for a green three OS verification matrix before publish",
        ),
        (
            "topical",
            "gate the deploy on a passing multi-OS CI matrix before release",
        ),
        (
            "paraphrase",
            "when is it safe to ship a build across every platform",
        ),
        (
            "unrelated",
            "the cat sat quietly on the warm windowsill at dawn",
        ),
    ];
    eprint!("C1b MEASURE: floor=0.40 cosines[");
    for (name, q) in probes {
        let c = cosine(&bv, &embedder.embed(q));
        eprint!("{name}={c:.3} ");
    }
    eprintln!("]");
    // Sanity floor only: identity must be ~1.0 — proves the embedder is live and
    // deterministic. The interpretive numbers above are the deliverable.
    let identity = cosine(&bv, &bv);
    assert!(
        identity > 0.99,
        "C1b: identity cosine must be ~1.0, got {identity:.3}"
    );
}

// ===========================================================================
// C2 — MEDULLA RECALL. A law written into the owner's medulla store (the
//      doctrine seat) must be recallable from that store by (a) exact token and
//      (b) a natural-language question. Repro of 2026-07-12T00:15.
// ===========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c2_medulla_recall() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner(&tmp.path().join("runtime"));
    let sid = owner.init_session_medulla().await;

    let sentinel = "AbstentionEnginePrerogative";
    let body = "an engine may abstain and return insufficient_evidence rather than guess when calibrated trust is unknown";
    let mem = owner
        .tool_medulla(
            sid.as_str(),
            "memorize",
            serde_json::json!({
                "agent_id": "c2",
                "node_label": sentinel,
                "claims": [{"label": sentinel, "text": body, "confidence": "high"}]
            }),
        )
        .await;
    let wrote = mem["refused"].is_null()
        && (mem["ingested"].as_bool().unwrap_or(false)
            || mem["claims_written"].as_u64().unwrap_or(0) > 0);

    let s_exact = owner
        .tool_medulla(
            sid.as_str(),
            "seek",
            serde_json::json!({"agent_id":"c2","query":sentinel,"tier":"medulla"}),
        )
        .await;
    let re = rank_of(&seek_results(&s_exact), sentinel);

    let s_nl = owner
        .tool_medulla(
            sid.as_str(),
            "seek",
            serde_json::json!({"agent_id":"c2","query":"may an engine abstain instead of guessing","tier":"medulla"}),
        )
        .await;
    let rn = rank_of(&seek_results(&s_nl), sentinel);

    eprintln!(
        "C2 MEASURE: wrote={wrote} refused={:?} exact[found={} rank/score={:?}] nl_question[found={} rank/score={:?}]",
        mem["refused"], re.is_some(), re, rn.is_some(), rn
    );

    // Permanent floor: a medulla law must be recallable by an exact token from
    // its own store. (The natural-language direction is measured, not gated —
    // that is the pain surface.)
    assert!(wrote, "C2: memorize into medulla was refused/empty: {mem}");
    assert!(
        re.is_some(),
        "C2: exact-token medulla recall FAILED: {s_exact}"
    );
}

// ===========================================================================
// C3 — CROSS_VERIFY HONESTY. After memorize + an explicit light merge ingest of
//      the agent-memory dir, cross_verify(existence) must NOT report the
//      `.light.md` memory files as `missing_from_graph`. Repro of
//      2026-07-12T11:28 (codex-game): light nodes exist yet are flagged missing.
// ===========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c3_cross_verify_light_not_missing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner(&tmp.path().join("runtime"));
    let root = tmp.path().join("repo");
    write_repo(&root, "Verify");
    let (sid, _n) = owner.bootstrap(&root, "c3").await;

    // Memorize two claims (each a .light.md on disk, ingested into the graph).
    let m1 = owner
        .memorize(
            &sid,
            &root,
            "c3",
            "VerifyClaimAlpha",
            "alpha durable finding",
        )
        .await;
    owner
        .memorize(&sid, &root, "c3", "VerifyClaimBeta", "beta durable finding")
        .await;

    // The agent-memory directory that holds the .light.md files.
    let light_path = m1["path"].as_str().unwrap_or_default().to_string();
    let mem_dir = Path::new(&light_path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    // Explicit light merge ingest of that dir (report 111's exact move).
    let ing = owner
        .tool(
            &sid,
            &root,
            "ingest",
            serde_json::json!({
                "path": mem_dir,
                "adapter": "light",
                "mode": "merge",
                "agent_id": "c3"
            }),
        )
        .await;
    let light_nodes = ing["node_count"].as_u64().unwrap_or(0);

    let cv = owner
        .tool(
            &sid,
            &root,
            "cross_verify",
            serde_json::json!({"agent_id":"c3","check":["existence"]}),
        )
        .await;
    let missing = cv["missing_from_graph"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let missing_light: Vec<String> = missing
        .iter()
        .filter_map(|m| {
            m["file_path"]
                .as_str()
                .or_else(|| m["external_id"].as_str())
        })
        .filter(|p| p.ends_with(".light.md") || p.contains("light"))
        .map(|s| s.to_string())
        .collect();

    eprintln!(
        "C3 MEASURE: light_nodes_ingested={light_nodes} missing_from_graph_total={} missing_light_count={} missing_light={:?}",
        missing.len(),
        missing_light.len(),
        missing_light
    );

    assert!(
        missing_light.is_empty(),
        "C3: cross_verify falsely reports {} .light.md file(s) missing_from_graph though their nodes are in the graph: {:?}",
        missing_light.len(),
        missing_light
    );
}

// ===========================================================================
// C4 — NORTH MEMORY BEAT. With several claims in the store, a north whose task
//      matches a claim's MEANING must surface at least that claim. Repro of the
//      chronic "N claims, none surfaced". North's memory feed reuses `seek`
//      scoped to `light::` (m1nd-mcp/src/server.rs ~3736).
//
// MEASURED RED (2026-07-13, pre-fix): claims_written=5 memory_exists=5
//   close_task_mem_rows=0 relevant_hit=false generic_task_mem_rows=0.
//   Probes: unscoped exact-token seek FINDS the claim (rank 1) and the scope
//   filter is fine (light::+exact-token = 4 hits); north's broad-fallback
//   recall = 0 hits, with_provenance=0. Root cause: the claim BODY prose line
//   is parsed into NO node and NO excerpt (m1nd-ingest/src/l1ght_adapter.rs
//   parse_file — only `##` headings and `[..]` markers become nodes), so a
//   task-meaning query has nothing to match: seek keyword/trigram use the
//   label only (layer_handlers.rs:330-365), CharNgramIndex indexes the label
//   only (m1nd-core/src/semantic.rs CharNgramIndex::build), and embeddings
//   embed label+excerpt (semantic.rs build_embeddings) where excerpt==label.
// ===========================================================================
#[ignore = "the fix target: C4 north memory beat — 5 claims on disk, 0 surfaced (task, generic and broad-fallback recalls all return 0 rows); the claim body prose reaches no searchable surface"]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c4_north_memory_beat_surfaces_relevant() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let owner = mk_owner(&tmp.path().join("runtime"));
    let root = tmp.path().join("repo");
    write_repo(&root, "Beat");
    let (sid, _n) = owner.bootstrap(&root, "c4").await;

    // Five distinct claims across topics.
    let claims = [
        (
            "DeployMatrixLaw",
            "publishing a release requires a green three-OS verification matrix first",
        ),
        (
            "TokenBudgetLaw",
            "each retrieval pass caps its output at a fixed token budget to stay bounded",
        ),
        (
            "SupersessionLaw",
            "a weaker memory write is refused so the stronger prior belief stays live",
        ),
        (
            "EvidenceAnchorLaw",
            "a memory claim anchors to the code file it cites so staleness is detectable",
        ),
        (
            "AbstainLaw",
            "an engine returns insufficient_evidence rather than guess when trust is unknown",
        ),
    ];
    for (label, text) in claims {
        owner.memorize(&sid, &root, "c4", label, text).await;
    }

    // Task closely matching ONE claim's meaning (DeployMatrixLaw), zero token
    // overlap with its label.
    let close = owner
        .tool(&sid, &root, "north", serde_json::json!({"agent_id":"c4","task":"is it safe to publish across operating systems"}))
        .await;
    let close_rows = north_memory(&close);
    let close_hit = close_rows.iter().any(|r| {
        let c = r["claim"].as_str().unwrap_or("");
        let l = r["label"].as_str().unwrap_or("");
        c.contains("DeployMatrix")
            || l.contains("DeployMatrix")
            || c.contains("three-OS")
            || c.contains("verification matrix")
    });

    let generic = owner
        .tool(
            &sid,
            &root,
            "north",
            serde_json::json!({"agent_id":"c4","task":"orient me in this project"}),
        )
        .await;

    let mem_exists = close["memory_exists"]
        .as_u64()
        .or_else(|| close["memory_store"]["memory_exists"].as_u64())
        .unwrap_or(0);

    // --- diagnostic probes: WHY does the beat surface nothing? ---
    // (i) unscoped exact-token seek proves the claim is retrievable in THIS store.
    let ex = owner
        .tool(
            &sid,
            &root,
            "seek",
            serde_json::json!({"agent_id":"c4","query":"DeployMatrixLaw"}),
        )
        .await;
    let ex_hit = seek_results(&ex).into_iter().find(|r| {
        r["node_id"].as_str().unwrap_or("").contains("DeployMatrix")
            || r["label"].as_str().unwrap_or("").contains("DeployMatrix")
    });
    let ex_found = ex_hit.is_some();
    let ex_node_id = ex_hit
        .as_ref()
        .map(|r| r["node_id"].as_str().unwrap_or("").to_string())
        .unwrap_or_default();
    eprintln!("C4 PROBE-ID: unscoped exact hit node_id='{ex_node_id}'");
    // (ii-a) STRONG query under the light:: scope — isolates a broken scope
    //        filter from merely-weak scoring.
    let scoped_strong = owner
        .tool(
            &sid,
            &root,
            "seek",
            serde_json::json!({"agent_id":"c4","query":"DeployMatrixLaw","scope":"light::"}),
        )
        .await;
    eprintln!(
        "C4 PROBE-SCOPE: light::+exact-token hits={} scanned={}",
        seek_results(&scoped_strong).len(),
        scoped_strong["total_candidates_scanned"]
            .as_u64()
            .unwrap_or(0)
    );
    // (ii) the `light::`-scoped recall north runs internally (broad fallback query).
    let scoped = owner
        .tool(&sid, &root, "seek", serde_json::json!({"agent_id":"c4","query":"memory decision finding note claim","scope":"light::"}))
        .await;
    let scoped_rows = seek_results(&scoped);
    let with_prov = scoped_rows
        .iter()
        .filter(|r| !r["source_agent"].is_null() || !r["authored_ms_ago"].is_null())
        .count();
    let sample_id = scoped_rows
        .first()
        .map(|r| r["node_id"].as_str().unwrap_or("").to_string())
        .unwrap_or_default();

    eprintln!(
        "C4 MEASURE: claims_written=5 memory_exists={mem_exists} close_task_mem_rows={} close_task_relevant_hit={close_hit} generic_task_mem_rows={}",
        close_rows.len(),
        north_memory(&generic).len()
    );
    eprintln!(
        "C4 PROBE: unscoped_exact_found={ex_found} | light::_scoped_hits={} with_provenance={with_prov} sample_node_id='{sample_id}'",
        scoped_rows.len()
    );

    assert!(
        close_hit,
        "C4: north memory beat surfaced no claim relevant to the task though 5 claims exist (rows={:?})",
        close_rows
    );
}

// ===========================================================================
// C5 — FEDERATE CLOCK. Federate two repos (small ~50, then medium ~300 nodes)
//      and seek the federated graph under a HARD 90s bound — measure real time
//      or document the hang. Repro of 2026-07-12T18:11 / 18:28 (codex-redsky).
// ===========================================================================
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn c5_federate_clock() {
    use std::time::Duration;

    async fn run(files: usize, per_file: usize, tag: &str) -> (bool, f64, u64, bool, f64, usize) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let owner = mk_owner(&tmp.path().join("runtime"));
        let a = tmp.path().join("repo_a");
        let b = tmp.path().join("repo_b");
        gen_sized_repo(&a, &format!("{tag}A"), files, per_file);
        gen_sized_repo(&b, &format!("{tag}B"), files, per_file);
        let sid = owner.init_session(&a).await;

        let fed_args = serde_json::json!({
            "agent_id": "c5",
            "repos": [
                {"name": "alpha", "path": a.to_string_lossy()},
                {"name": "beta", "path": b.to_string_lossy()}
            ],
            "detect_cross_repo_edges": true
        });

        let t0 = Instant::now();
        let fed = tokio::time::timeout(
            Duration::from_secs(90),
            owner.tool(&sid, &a, "federate", fed_args),
        )
        .await;
        let fed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        let (fed_ok, nodes) = match &fed {
            Ok(v) => (
                true,
                v["total_nodes"]
                    .as_u64()
                    .or_else(|| v["node_count"].as_u64())
                    .unwrap_or(0),
            ),
            Err(_) => (false, 0),
        };

        // Seek the federated graph under its own 90s bound.
        let t1 = Instant::now();
        let seek = tokio::time::timeout(
            Duration::from_secs(90),
            owner.tool(&sid, &a, "seek", serde_json::json!({"agent_id":"c5","query":"scale widget probe","graph_rerank":true,"top_k":25})),
        )
        .await;
        let seek_ms = t1.elapsed().as_secs_f64() * 1000.0;
        let (seek_ok, hits) = match &seek {
            Ok(v) => (true, seek_results(v).len()),
            Err(_) => (false, 0),
        };
        (fed_ok, fed_ms, nodes, seek_ok, seek_ms, hits)
    }

    let (s_ok, s_ms, s_nodes, ss_ok, ss_ms, s_hits) = run(2, 10, "Sm").await;
    eprintln!(
        "C5 MEASURE small: federate[ok={s_ok} ms={s_ms:.0} nodes={s_nodes}] seek[ok={ss_ok} ms={ss_ms:.0} hits={s_hits}]"
    );
    let (m_ok, m_ms, m_nodes, ms_ok, ms_ms, m_hits) = run(5, 28, "Md").await;
    eprintln!(
        "C5 MEASURE medium: federate[ok={m_ok} ms={m_ms:.0} nodes={m_nodes}] seek[ok={ms_ok} ms={ms_ms:.0} hits={m_hits}]"
    );

    assert!(
        s_ok && ss_ok,
        "C5: small federate/seek did not return within 90s (federate_ok={s_ok} seek_ok={ss_ok})"
    );
    assert!(m_ok && ms_ok, "C5: medium ({m_nodes} nodes) federate/seek did not return within 90s (federate_ok={m_ok} seek_ok={ms_ok})");
}
