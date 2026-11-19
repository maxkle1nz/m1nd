//! A default `seek` row carries what an agent acts on; diagnostics are opt-in.
//!
//! Default rows keep the id, label, type, score, a short intent line, the file
//! path and line span, the attached `symbols`, any excerpt and memory
//! provenance. `detail: true` returns the full row exactly as it was before
//! the split: `connections`, `heuristic_signals`, `score_breakdown` and the
//! full `intent_summary`. Everything outside the rows' diagnostic fields (row
//! ids, order, scores, counts, budget, sufficiency, trust envelope) is the same
//! with or without `detail`.
use crate::layer_handlers::handle_seek;
use crate::protocol::layers::SeekInput;
use crate::server::McpConfig;
use crate::session::SessionState;
use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::{Graph, NodeProvenanceInput};
use m1nd_core::types::{EdgeDirection, FiniteF32, NodeId, NodeType};
use serde_json::{json, Value};

const NARRATIVE: &str =
    "Where does validation check credentials before granting account session access?";
const IDENTIFIER: &str = "check_billing_account_access";
const SHORT: &str = "account access";

/// The fields a default row may carry.
const LEAN_FIELDS: [&str; 13] = [
    "authored_ms_ago",
    "excerpt",
    "file_path",
    "intent_summary",
    "label",
    "line_end",
    "line_start",
    "node_id",
    "origin_brain",
    "score",
    "source_agent",
    "symbols",
    "type",
];

/// The fields only `detail: true` carries.
const DETAIL_ONLY_FIELDS: [&str; 3] = ["connections", "heuristic_signals", "score_breakdown"];

/// Upper bound, in characters, of a default row's intent line.
const INTENT_LINE_MAX_CHARS: usize = 120;

fn add(
    graph: &mut Graph,
    external: &str,
    label: &str,
    kind: NodeType,
    tags: &[&str],
    path: &str,
    lines: (u32, u32),
) -> NodeId {
    let node = graph
        .add_node(external, label, kind, tags, 0.0, 0.0)
        .expect("add node");
    graph.set_node_provenance(
        node,
        NodeProvenanceInput {
            source_path: Some(path),
            line_start: Some(lines.0),
            line_end: Some(lines.1),
            excerpt: None,
            namespace: None,
            canonical: true,
        },
    );
    node
}

fn link(graph: &mut Graph, source: NodeId, target: NodeId, relation: &str) {
    graph
        .add_edge(
            source,
            target,
            relation,
            FiniteF32::new(0.8),
            EdgeDirection::Forward,
            false,
            FiniteF32::new(0.5),
        )
        .expect("add edge");
}

/// The domain each service file handles; it names that file's functions, so
/// node-granularity answers list one row per service instead of collapsing
/// same-name functions into one family.
const DOMAINS: [&str; 24] = [
    "billing", "ledger", "invoice", "payout", "refund", "wallet", "tenant", "profile", "device",
    "token", "webhook", "export", "import", "report", "audit", "quota", "plan", "seat", "role",
    "team", "inbox", "upload", "search", "notify",
];

/// Twenty-four services, each a file with three functions. Files and functions
/// carry edge-resolution tags, files import their neighbours and functions call
/// across files, so every row has connections, signals and a long summary.
fn diet_graph() -> Graph {
    let mut graph = Graph::new();
    let mut files = Vec::new();
    let mut validators = Vec::new();
    for (service, domain) in DOMAINS.iter().enumerate() {
        let next = (service + 1) % DOMAINS.len();
        let next_domain = DOMAINS[next];
        let path = format!("src/{domain}/service.rs");
        let module_tag = format!("rust:module:{domain}");
        let ambiguous = format!(
            "m1nd:edge:ambiguous:file::src/{next_domain}/service.rs::fn::validate_{next_domain}_credentials"
        );
        let file = add(
            &mut graph,
            &format!("file::{path}"),
            &path,
            NodeType::File,
            &[
                "rust",
                &module_tag,
                "m1nd:edge:unresolved",
                "m1nd:edge:ambiguous",
                &ambiguous,
            ],
            &path,
            (1, 300),
        );
        let validate_name = format!("validate_{domain}_credentials");
        let validate = add(
            &mut graph,
            &format!("file::{path}::fn::{validate_name}"),
            &validate_name,
            NodeType::Function,
            &["rust", &module_tag, &ambiguous],
            &path,
            (12, 40),
        );
        let grant_name = format!("grant_{domain}_session_access");
        let grant = add(
            &mut graph,
            &format!("file::{path}::fn::{grant_name}"),
            &grant_name,
            NodeType::Function,
            &["rust", &module_tag],
            &path,
            (44, 80),
        );
        let check_name = format!("check_{domain}_account_access");
        let check = add(
            &mut graph,
            &format!("file::{path}::fn::{check_name}"),
            &check_name,
            NodeType::Function,
            &[
                "rust",
                &module_tag,
                "rust:visibility:pub",
                "rust:async",
                "rust:attr:instrument",
                "rust:returns:result",
                "rust:doc:checks-the-account-before-any-session-is-granted",
                "m1nd:edge:unresolved",
            ],
            &path,
            (84, 120),
        );
        for function in [validate, grant, check] {
            link(&mut graph, file, function, "contains");
        }
        link(&mut graph, validate, grant, "calls");
        link(&mut graph, check, validate, "calls");
        files.push(file);
        validators.push(validate);
    }
    for service in 0..DOMAINS.len() {
        let next = (service + 1) % DOMAINS.len();
        link(&mut graph, files[service], files[next], "imports");
        link(&mut graph, files[service], validators[next], "imports");
        link(&mut graph, validators[service], validators[next], "calls");
    }
    graph
}

fn session(root: &std::path::Path) -> SessionState {
    let mut graph = diet_graph();
    graph.finalize().expect("finalize graph");
    let runtime = root.join("runtime");
    std::fs::create_dir_all(&runtime).expect("create runtime");
    let config = McpConfig {
        graph_source: runtime.join("graph.json"),
        plasticity_state: runtime.join("plasticity.json"),
        registry_dir: Some(runtime.join("registry")),
        runtime_dir: Some(runtime),
        ..Default::default()
    };
    let mut state =
        SessionState::initialize(graph, &config, DomainConfig::code()).expect("session");
    state.ingest_roots = vec![root.to_string_lossy().into_owned()];
    state.workspace_root = Some(root.to_string_lossy().into_owned());
    #[cfg(feature = "embed")]
    {
        // Pin the no-model fallback so the ranking is identical on every machine.
        state.orchestrator.semantic.embedder = None;
        state.orchestrator.semantic.embeddings.clear();
    }
    state
}

fn seek_input(query: &str, detail: Option<bool>, token_budget: Option<usize>) -> SeekInput {
    let mut args = json!({
        "agent_id": "seek-lean-rows",
        "query": query,
        "top_k": 20,
        "min_score": 0.0
    });
    if let Some(detail) = detail {
        args["detail"] = json!(detail);
    }
    if let Some(budget) = token_budget {
        args["token_budget"] = json!(budget);
    }
    serde_json::from_value(args).expect("seek input")
}

/// The answer as the wire carries it, without the wall-clock field.
fn seek_json(state: &mut SessionState, input: SeekInput) -> Value {
    let output = handle_seek(state, input).expect("seek");
    let mut value = serde_json::to_value(output).expect("serialize seek");
    value
        .as_object_mut()
        .expect("seek object")
        .remove("elapsed_ms");
    value
}

fn rows(output: &Value) -> &Vec<Value> {
    output["results"].as_array().expect("results array")
}

fn keys(row: &Value) -> Vec<&str> {
    row.as_object()
        .expect("row object")
        .keys()
        .map(String::as_str)
        .collect()
}

fn ids(output: &Value) -> Vec<&str> {
    rows(output)
        .iter()
        .map(|row| row["node_id"].as_str().expect("node_id"))
        .collect()
}

/// The three queries cover both granularities and both row kinds.
fn cases() -> [(&'static str, &'static str, &'static str); 3] {
    [
        ("narrative", NARRATIVE, "file_group"),
        ("identifier", IDENTIFIER, "node"),
        ("short", SHORT, "node"),
    ]
}

#[test]
fn default_rows_carry_only_what_an_agent_acts_on() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = session(temp.path());
    for (case, query, granularity) in cases() {
        for explicit in [None, Some(false)] {
            let output = seek_json(&mut state, seek_input(query, explicit, None));
            assert_eq!(output["result_granularity"], granularity, "{case}");
            assert!(rows(&output).len() >= 5, "{case}: {output:#}");
            for row in rows(&output) {
                for key in keys(row) {
                    assert!(
                        LEAN_FIELDS.contains(&key),
                        "{case}: default row carries `{key}`: {row:#}"
                    );
                }
                for key in ["node_id", "label", "type", "score", "intent_summary"] {
                    assert!(row.get(key).is_some(), "{case}: `{key}` missing: {row:#}");
                }
                let intent = row["intent_summary"].as_str().expect("intent line");
                assert!(
                    intent.chars().count() <= INTENT_LINE_MAX_CHARS,
                    "{case}: intent line over {INTENT_LINE_MAX_CHARS} chars: {intent:?}"
                );
                assert!(
                    !intent.contains("m1nd:edge:"),
                    "{case}: edge diagnostics in the intent line: {intent:?}"
                );
            }
        }
    }
}

#[test]
fn default_intent_line_keeps_the_summary_shape_and_marks_a_cut() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = session(temp.path());
    let output = seek_json(&mut state, seek_input(IDENTIFIER, None, None));
    let check = rows(&output)
        .iter()
        .find(|row| row["label"] == IDENTIFIER)
        .unwrap_or_else(|| panic!("no {IDENTIFIER} row: {output:#}"));
    let intent = check["intent_summary"].as_str().expect("intent line");
    // `label (type) [tags]` with the edge tags left out; this one is longer
    // than the cap, so it is cut on a character boundary and marked.
    assert!(
        intent.starts_with("check_billing_account_access (function) [rust, rust:module:billing, "),
        "{intent:?}"
    );
    assert_eq!(intent.chars().count(), INTENT_LINE_MAX_CHARS, "{intent:?}");
    assert!(intent.ends_with('…'), "{intent:?}");

    // A short summary is not cut.
    let output = seek_json(
        &mut state,
        seek_input("validate_billing_credentials", None, None),
    );
    let validate = rows(&output)
        .iter()
        .find(|row| row["label"] == "validate_billing_credentials")
        .unwrap_or_else(|| panic!("no validate_billing_credentials row: {output:#}"));
    assert_eq!(
        validate["intent_summary"],
        "validate_billing_credentials (function) [rust, rust:module:billing]"
    );
}

/// `detail: true` returns the rows exactly as seek returned them before the
/// diagnostics moved behind the flag. Each golden line holds one case's rows
/// as the earlier build serialized them from this fixture.
#[test]
fn detail_restores_the_full_rows_byte_for_byte() {
    let golden: Vec<Value> = include_str!("seek_lean_rows.detail_golden.jsonl")
        .lines()
        .map(|line| serde_json::from_str(line).expect("golden line"))
        .collect();
    assert_eq!(golden.len(), cases().len());
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = session(temp.path());
    for (case, query, _) in cases() {
        let expected = golden
            .iter()
            .find(|line| line["case"] == case)
            .unwrap_or_else(|| panic!("no golden line for {case}"));
        let output = seek_json(&mut state, seek_input(query, Some(true), None));
        assert!(!rows(&output).is_empty(), "{case}");
        assert_eq!(
            serde_json::to_string(&output["results"]).expect("rows"),
            serde_json::to_string(&expected["results"]).expect("golden rows"),
            "{case}: detail rows differ from the golden"
        );
        for row in rows(&output) {
            for key in DETAIL_ONLY_FIELDS {
                assert!(row.get(key).is_some(), "{case}: `{key}` missing: {row:#}");
            }
        }
    }
}

/// `detail` changes the rows' diagnostic fields and nothing else: the same
/// ids in the same order with the same scores and symbols, and an identical
/// envelope (counts, budget, sufficiency, trust envelope, next step).
#[test]
fn detail_changes_the_rows_never_the_ranking() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = session(temp.path());
    for (case, query, _) in cases() {
        for budget in [None, Some(400), Some(100_000)] {
            let mut lean = seek_json(&mut state, seek_input(query, None, budget));
            let mut full = seek_json(&mut state, seek_input(query, Some(true), budget));
            assert_eq!(ids(&lean), ids(&full), "{case} budget={budget:?}");
            for (lean_row, full_row) in rows(&lean).iter().zip(rows(&full)) {
                for key in keys(lean_row) {
                    if key != "intent_summary" {
                        assert_eq!(
                            lean_row[key], full_row[key],
                            "{case} budget={budget:?}: `{key}` differs"
                        );
                    }
                }
                let mut dropped: Vec<&str> = keys(full_row)
                    .into_iter()
                    .filter(|key| lean_row.get(*key).is_none())
                    .collect();
                dropped.sort_unstable();
                assert_eq!(dropped, DETAIL_ONLY_FIELDS, "{case} budget={budget:?}");
            }
            if budget == Some(400) {
                assert!(
                    lean["budget"]["dropped"].as_u64().unwrap_or(0) > 0,
                    "{case}: the small budget should drop rows: {lean:#}"
                );
            }
            lean.as_object_mut().expect("object").remove("results");
            full.as_object_mut().expect("object").remove("results");
            assert_eq!(lean, full, "{case} budget={budget:?}: envelope differs");
        }
    }
}

/// The default narrative answer on this fixture stays under a byte budget;
/// the same answer with `detail: true` does not.
#[test]
fn default_narrative_answer_fits_its_byte_budget() {
    const RESULTS_BUDGET_BYTES: usize = 9_000;
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = session(temp.path());
    let lean = seek_json(&mut state, seek_input(NARRATIVE, None, None));
    let full = seek_json(&mut state, seek_input(NARRATIVE, Some(true), None));
    assert_eq!(rows(&lean).len(), 20, "{lean:#}");
    let lean_bytes = serde_json::to_string(&lean["results"]).expect("rows").len();
    let full_bytes = serde_json::to_string(&full["results"]).expect("rows").len();
    assert!(
        lean_bytes <= RESULTS_BUDGET_BYTES,
        "default rows take {lean_bytes} bytes (budget {RESULTS_BUDGET_BYTES}, detail {full_bytes})"
    );
    assert!(
        full_bytes > RESULTS_BUDGET_BYTES,
        "detail rows: {full_bytes} bytes"
    );
}
