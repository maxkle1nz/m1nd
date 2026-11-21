//! A default `search` answer carries what an agent acts on, inside 4 KB.
//!
//! Default rows keep `node_id`, `type`, `file_path`, `line_number` and
//! `matched_line`; they are packed best-first and any cut is declared.
//! `detail: true` returns the full rows (context lines, `graph_linked`,
//! `label`) and the next-step diagnostics with the `agent_runtime_contract`.
//! The ranking is the same either way, so the default rows are a prefix of the
//! detailed ones.
use crate::server::{dispatch_tool, McpConfig};
use crate::session::SessionState;
use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_core::types::NodeType;
use serde_json::{json, Value};

/// Bytes a default answer may take as the MCP text serves it.
const SEARCH_DEFAULT_BUDGET_BYTES: usize = 4_096;

/// The fields a default row may carry.
const LEAN_ROW_FIELDS: [&str; 9] = [
    "context_after",
    "context_before",
    "file_path",
    "label",
    "line_number",
    "matched_line",
    "node_id",
    "score",
    "type",
];

/// Response fields only `detail: true` carries on a healthy graph.
const DETAIL_ONLY_RESPONSE_FIELDS: [&str; 6] = [
    "agent_runtime_contract",
    "confidence",
    "next_suggested_target",
    "next_suggested_tool",
    "what_is_missing",
    "why_this_next_step",
];

/// A populated session over a repository where one token matches 120 lines
/// across three files: far more rows than a 4 KB answer can carry.
fn build_state(root: &std::path::Path) -> SessionState {
    let runtime_dir = root.join("runtime");
    std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
    let src = root.join("src");
    std::fs::create_dir_all(&src).expect("src dir");
    let mut graph = Graph::new();
    for name in ["alpha", "beta", "gamma"] {
        let mut body = String::new();
        for line in 0..40 {
            body.push_str(&format!(
                "pub fn {name}_step_{line}() -> u32 {{ needle_token_{line}() + {line} }}\n"
            ));
        }
        std::fs::write(src.join(format!("{name}.rs")), body).expect("write source");
        graph
            .add_node(
                &format!("file::src/{name}.rs"),
                &format!("{name}.rs"),
                NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("add file node");
    }
    graph.finalize().expect("finalize graph");
    let config = McpConfig {
        graph_source: runtime_dir.join("graph.json"),
        plasticity_state: runtime_dir.join("plasticity.json"),
        registry_dir: Some(runtime_dir.join("registry")),
        runtime_dir: Some(runtime_dir),
        ..Default::default()
    };
    let mut state = SessionState::initialize(graph, &config, DomainConfig::code()).expect("init");
    state.ingest_roots = vec![root.to_string_lossy().to_string()];
    state.workspace_root = Some(root.to_string_lossy().to_string());
    state
}

fn search(state: &mut SessionState, extra: Value) -> Value {
    let mut arguments = json!({"agent_id": "lean", "query": "needle_token"});
    for (key, value) in extra.as_object().expect("object").iter() {
        arguments[key] = value.clone();
    }
    dispatch_tool(state, "search", &arguments).expect("search answers")
}

/// The bytes an agent receives: the MCP text content of the answer.
fn served_bytes(answer: &Value) -> usize {
    serde_json::to_string_pretty(answer)
        .expect("serialize")
        .len()
}

fn row_keys(answer: &Value) -> Vec<(String, String, u64)> {
    answer["results"]
        .as_array()
        .expect("results")
        .iter()
        .map(|row| {
            (
                row["node_id"].as_str().unwrap_or_default().to_string(),
                row["file_path"].as_str().unwrap_or_default().to_string(),
                row["line_number"].as_u64().unwrap_or_default(),
            )
        })
        .collect()
}

#[test]
fn a_default_search_answer_fits_four_kilobytes_and_declares_its_cut() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_state(temp.path());

    let answer = search(&mut state, json!({}));
    let bytes = served_bytes(&answer);
    eprintln!(
        "SEARCH_DEFAULT bytes={bytes} rows={} total_matches={}",
        answer["results"].as_array().map_or(0, Vec::len),
        answer["total_matches"]
    );
    assert!(
        bytes <= SEARCH_DEFAULT_BUDGET_BYTES,
        "a default search answer is {bytes} bytes; the budget is {SEARCH_DEFAULT_BUDGET_BYTES}"
    );
    assert!(
        answer["total_matches"].as_u64().unwrap_or(0) >= 120,
        "{answer}"
    );
    assert_eq!(answer["truncated"], true, "the cut must be declared");
    let kept = answer["results"].as_array().expect("results").len();
    let omitted = answer["rows_omitted"].as_u64().expect("rows_omitted") as usize;
    assert!(kept >= 5, "a default answer still carries rows, got {kept}");
    assert_eq!(kept + omitted, 50, "kept + omitted account for top_k rows");
    assert!(
        answer["inline_summary"]
            .as_str()
            .unwrap_or_default()
            .contains("detail: true"),
        "the summary names the way to the rest: {answer}"
    );
    for row in answer["results"].as_array().expect("results") {
        let keys: Vec<&str> = row
            .as_object()
            .expect("row")
            .keys()
            .map(String::as_str)
            .collect();
        for key in &keys {
            assert!(
                LEAN_ROW_FIELDS.contains(key),
                "a default row carries `{key}`: {row}"
            );
        }
        assert!(row.get("graph_linked").is_none(), "{row}");
        assert!(row.get("context_before").is_none(), "{row}");
    }
    for field in DETAIL_ONLY_RESPONSE_FIELDS {
        assert!(
            answer.get(field).is_none(),
            "a default answer on a healthy graph carries `{field}`"
        );
    }
}

#[test]
fn detail_restores_the_full_rows_and_keeps_the_ranking() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_state(temp.path());

    let lean = search(&mut state, json!({}));
    let full = search(&mut state, json!({"detail": true}));

    assert_eq!(full["truncated"], false, "{}", full["inline_summary"]);
    assert_eq!(full["results"].as_array().expect("rows").len(), 50);
    let row = &full["results"][0];
    assert!(row["graph_linked"].is_boolean(), "{row}");
    assert!(row["label"].is_string(), "{row}");
    assert!(
        row["context_after"]
            .as_array()
            .is_some_and(|c| !c.is_empty()),
        "detail rows carry two context lines by default: {row}"
    );
    assert_eq!(
        full["agent_runtime_contract"]["schema"],
        "m1nd-agent-runtime-contract-v0"
    );
    assert!(full["why_this_next_step"].is_string(), "{full}");

    let lean_rows = row_keys(&lean);
    let full_rows = row_keys(&full);
    assert_eq!(
        full_rows[..lean_rows.len()],
        lean_rows[..],
        "the default rows must be the first rows of the detailed answer"
    );
    assert_eq!(lean["total_matches"], full["total_matches"]);
}

#[test]
fn a_default_answer_honours_the_callers_own_budget_and_context() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_state(temp.path());

    // An explicit context request is served on the lean rows too.
    let with_context = search(&mut state, json!({"context_lines": 1, "top_k": 3}));
    let row = &with_context["results"][0];
    assert!(
        row["context_after"]
            .as_array()
            .is_some_and(|c| c.len() == 1),
        "{row}"
    );
    assert!(row.get("graph_linked").is_none(), "{row}");

    // A caller that sets its own output cap is not packed a second time.
    let own_cap = search(&mut state, json!({"max_output_chars": 100_000}));
    assert_eq!(own_cap["results"].as_array().expect("rows").len(), 50);
    assert!(own_cap.get("rows_omitted").is_none(), "{own_cap}");
}

#[test]
fn a_default_answer_with_no_match_keeps_the_repair_once() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = build_state(temp.path());

    let answer = search(&mut state, json!({"query": "no_such_token_anywhere"}));
    let bytes = served_bytes(&answer);
    assert!(
        bytes <= SEARCH_DEFAULT_BUDGET_BYTES,
        "a default answer with no match is {bytes} bytes: {answer}"
    );
    assert_eq!(answer["total_matches"], 0);
    assert_eq!(
        answer["recovery"]["expected_output_schema"]
            .as_str()
            .or_else(|| answer["recovery"]["auto_action"]["expected_output_schema"].as_str()),
        Some("m1nd-recovery-playbook-v0"),
        "the repair stays on a failed retrieval: {answer}"
    );
    assert!(
        answer.get("agent_runtime_contract").is_none(),
        "the contract repeats the same recovery; it rides detail: true"
    );

    let full = search(
        &mut state,
        json!({"query": "no_such_token_anywhere", "detail": true}),
    );
    assert_eq!(
        full["agent_runtime_contract"]["recovery"], full["recovery"],
        "detail keeps the contract, which carries the same recovery"
    );
}
