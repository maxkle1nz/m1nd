//! File-group seek answers carry the file's best symbols (additive contract).
//!
//! A narrative `seek` returns one row per source file (`result_granularity:
//! "file_group"`). The row's own span is the whole file, so each of the first
//! ten rows may also carry `symbols`: up to three eligible symbols from that
//! file as `{node_id, line_start, line_end}`, in the order of the node-level
//! relevance rank seek computed before grouping. The field is omitted when empty
//! and never changes the rows themselves.
use crate::layer_handlers::{handle_focus, handle_seek};
use crate::protocol::layers::SeekInput;
use crate::server::{dispatch_tool, McpConfig};
use crate::session::SessionState;
use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::{Graph, NodeProvenanceInput};
use m1nd_core::types::{FiniteF32, NodeType, SemanticWeights};
use serde_json::{json, Value};

const NARRATIVE: &str =
    "Where does validation check credentials before granting account session access?";

fn add_node(
    graph: &mut Graph,
    external: &str,
    label: &str,
    kind: NodeType,
    source_path: &str,
    lines: (u32, u32),
) {
    let node = graph
        .add_node(external, label, kind, &[], 0.0, 0.0)
        .expect("add node");
    graph.set_node_provenance(
        node,
        NodeProvenanceInput {
            source_path: Some(source_path),
            line_start: Some(lines.0),
            line_end: Some(lines.1),
            excerpt: None,
            namespace: None,
            canonical: true,
        },
    );
}

fn add_tagged_file(graph: &mut Graph, path: &str, label: &str, tags: &[&str], last_line: u32) {
    let node = graph
        .add_node(
            &format!("file::{path}"),
            label,
            NodeType::File,
            tags,
            0.0,
            0.0,
        )
        .expect("add node");
    graph.set_node_provenance(
        node,
        NodeProvenanceInput {
            source_path: Some(path),
            line_start: Some(1),
            line_end: Some(last_line),
            excerpt: None,
            namespace: None,
            canonical: true,
        },
    );
}

fn add_file(graph: &mut Graph, path: &str, label: &str, last_line: u32) {
    add_node(
        graph,
        &format!("file::{path}"),
        label,
        NodeType::File,
        path,
        (1, last_line),
    );
}

fn add_fn(graph: &mut Graph, path: &str, name: &str, lines: (u32, u32)) {
    add_node(
        graph,
        &format!("file::{path}::fn::{name}"),
        name,
        NodeType::Function,
        path,
        lines,
    );
}

fn session(root: &std::path::Path, mut graph: Graph) -> SessionState {
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

/// One file with four relevant symbols plus an unrelated one, a second
/// unrelated source file, and a documentation file with no symbols at all.
fn guard_state(root: &std::path::Path) -> SessionState {
    let mut graph = Graph::new();
    let guard = "src/auth/session_guard.rs";
    add_tagged_file(
        &mut graph,
        guard,
        "session_guard.rs",
        &[
            "rust",
            "m1nd:edge:unresolved",
            "m1nd:edge:ambiguous",
            "m1nd:edge:ambiguous:file::src/billing/invoice.rs::fn::compute_invoice_total",
        ],
        240,
    );
    add_fn(&mut graph, guard, "validate_credentials", (12, 40));
    add_fn(&mut graph, guard, "grant_session_access", (44, 80));
    add_fn(&mut graph, guard, "check_account_access", (84, 120));
    add_fn(&mut graph, guard, "session_credentials_cache", (124, 150));
    add_fn(&mut graph, guard, "format_audit_line", (160, 170));
    let invoice = "src/billing/invoice.rs";
    add_file(&mut graph, invoice, "invoice.rs", 90);
    add_fn(&mut graph, invoice, "compute_invoice_total", (10, 40));
    add_node(
        &mut graph,
        "file::docs/session-access.md",
        "session access credentials guide",
        NodeType::File,
        "docs/session-access.md",
        (1, 30),
    );
    session(root, graph)
}

fn seek_input(query: &str, scope: Option<&str>, node_types: &[&str]) -> SeekInput {
    serde_json::from_value(json!({
        "agent_id": "file-group-symbols",
        "query": query,
        "scope": scope,
        "node_types": node_types,
        "top_k": 10,
        "min_score": 0.0,
        "graph_rerank": false,
        "conformance_aware": false
    }))
    .expect("seek input")
}

/// `seek_input` with `detail: true`: the full rows, diagnostics included.
fn detail_input(query: &str, token_budget: Option<usize>) -> SeekInput {
    let mut args = json!({
        "agent_id": "file-group-symbols",
        "query": query,
        "top_k": 10,
        "min_score": 0.0,
        "graph_rerank": false,
        "conformance_aware": false,
        "detail": true
    });
    if let Some(budget) = token_budget {
        args["token_budget"] = json!(budget);
    }
    serde_json::from_value(args).expect("seek input")
}

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

fn row<'a>(output: &'a Value, node_id: &str) -> &'a Value {
    rows(output)
        .iter()
        .find(|row| row["node_id"] == node_id)
        .unwrap_or_else(|| panic!("{node_id} missing from {output:#}"))
}

fn symbol_ids(row: &Value) -> Vec<String> {
    row.get("symbols")
        .and_then(Value::as_array)
        .map(|symbols| {
            symbols
                .iter()
                .map(|symbol| symbol["node_id"].as_str().expect("id").to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The chars/4 estimate seek's budget packer applies to one result row,
/// including the attached symbols' text.
fn row_token_estimate(row: &Value) -> u64 {
    let text = |value: &Value| value.as_str().map_or(0, str::len);
    let mut chars = text(&row["label"])
        + text(&row["type"])
        + text(&row["intent_summary"])
        + text(&row["node_id"])
        + text(&row["file_path"])
        + text(&row["excerpt"]);
    for connection in row["connections"].as_array().into_iter().flatten() {
        chars += text(&connection["label"]) + text(&connection["relation"]);
    }
    for symbol in row["symbols"].as_array().into_iter().flatten() {
        chars += text(&symbol["node_id"]);
    }
    chars.div_ceil(4) as u64
}

#[test]
fn narrative_seek_attaches_the_files_best_symbols_with_real_spans() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = guard_state(temp.path());
    let output = seek_json(&mut state, seek_input(NARRATIVE, None, &[]));

    assert_eq!(output["result_granularity"], "file_group", "{output:#}");
    let first = &rows(&output)[0];
    assert_eq!(first["node_id"], "file::src/auth/session_guard.rs");
    assert_eq!(
        (first["line_start"].as_u64(), first["line_end"].as_u64()),
        (Some(1), Some(240))
    );

    // The row describes the file; graph edge-resolution diagnostics are not
    // part of that description.
    assert_eq!(first["intent_summary"], "session_guard.rs (file) [rust]");

    let symbols = first["symbols"]
        .as_array()
        .unwrap_or_else(|| panic!("the first file group carries no symbols: {first:#}"));
    for symbol in symbols {
        let mut keys: Vec<&str> = symbol
            .as_object()
            .expect("symbol object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["line_end", "line_start", "node_id"],
            "a symbol carries its id and span, nothing else: {symbol:#}"
        );
        let (start, end) = (
            symbol["line_start"].as_u64().expect("line_start"),
            symbol["line_end"].as_u64().expect("line_end"),
        );
        assert!(
            start > 1 && end < 240 && start < end,
            "real span: {symbol:#}"
        );
    }
    // At most three, best first by node-level relevance; the unrelated
    // `format_audit_line` ranks fifth and is cut.
    assert_eq!(
        symbol_ids(first),
        [
            "check_account_access",
            "validate_credentials",
            "session_credentials_cache"
        ]
        .map(|name| format!("file::src/auth/session_guard.rs::fn::{name}"))
    );

    // A file with no eligible symbols omits the field instead of sending [].
    let docs = row(&output, "file::docs/session-access.md");
    assert!(docs.get("symbols").is_none(), "{docs:#}");
}

/// Scope and eligibility (#568) apply to attached symbols exactly as to rows. A
/// symbol whose id sits in `zone_b` while its provenance names `zone_a/auth_a.rs`
/// is grouped by provenance — the key the file ranking uses — but is attached
/// only when it is eligible under the caller's scope.
#[test]
fn attached_symbols_respect_scope_and_eligibility_with_conflicting_provenance() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut graph = Graph::new();
    add_file(&mut graph, "zone_a/auth_a.rs", "auth_a.rs", 200);
    add_fn(
        &mut graph,
        "zone_a/auth_a.rs",
        "validate_credentials",
        (10, 40),
    );
    add_node(
        &mut graph,
        "file::zone_b/auth_b.rs::fn::grant_session_access",
        "grant_session_access",
        NodeType::Function,
        "zone_a/auth_a.rs",
        (50, 80),
    );
    add_file(&mut graph, "zone_b/auth_b.rs", "auth_b.rs", 150);
    add_fn(
        &mut graph,
        "zone_b/auth_b.rs",
        "check_account_credentials",
        (20, 60),
    );
    let mut state = session(temp.path(), graph);
    let conflicting = "file::zone_b/auth_b.rs::fn::grant_session_access";

    let unscoped = seek_json(&mut state, seek_input(NARRATIVE, None, &[]));
    assert_eq!(unscoped["result_granularity"], "file_group");
    let zone_a = symbol_ids(row(&unscoped, "file::zone_a/auth_a.rs"));
    assert!(
        zone_a.contains(&"file::zone_a/auth_a.rs::fn::validate_credentials".to_string())
            && zone_a.contains(&conflicting.to_string()),
        "unscoped, both symbols grouped under zone_a/auth_a.rs by provenance: {zone_a:?}"
    );
    assert_eq!(
        symbol_ids(row(&unscoped, "file::zone_b/auth_b.rs")),
        vec!["file::zone_b/auth_b.rs::fn::check_account_credentials"]
    );

    let scoped_a = seek_json(&mut state, seek_input(NARRATIVE, Some("zone_a"), &[]));
    assert_eq!(rows(&scoped_a).len(), 1, "{scoped_a:#}");
    assert_eq!(
        symbol_ids(row(&scoped_a, "file::zone_a/auth_a.rs")),
        vec!["file::zone_a/auth_a.rs::fn::validate_credentials"],
        "a symbol outside the scope is never attached"
    );

    let scoped_b = seek_json(&mut state, seek_input(NARRATIVE, Some("zone_b"), &[]));
    assert_eq!(rows(&scoped_b).len(), 1, "{scoped_b:#}");
    assert_eq!(
        symbol_ids(row(&scoped_b, "file::zone_b/auth_b.rs")),
        vec!["file::zone_b/auth_b.rs::fn::check_account_credentials"],
        "a symbol is attached only to the file its provenance names"
    );
}

#[test]
fn file_only_node_types_attach_no_symbols() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = guard_state(temp.path());
    let output = seek_json(&mut state, seek_input(NARRATIVE, None, &["file"]));
    assert_eq!(output["result_granularity"], "file_group");
    assert!(!rows(&output).is_empty(), "{output:#}");
    for row in rows(&output) {
        assert_eq!(row["type"], "file");
        assert!(row.get("symbols").is_none(), "{row:#}");
    }
}

/// A file holding two overloads that share a label, tags and file, so their
/// relevance ties exactly. `first` is inserted (and indexed) first.
fn overload_state(root: &std::path::Path, first: (&str, u32), second: (&str, u32)) -> SessionState {
    let mut graph = Graph::new();
    let router = "src/router.rs";
    add_file(&mut graph, router, "router.rs", 300);
    for (suffix, line) in [first, second] {
        add_node(
            &mut graph,
            &format!("file::src/router.rs::fn::handle_request{suffix}"),
            "handle_request",
            NodeType::Function,
            router,
            (line, line + 40),
        );
    }
    let mut state = session(root, graph);
    state.orchestrator.semantic.weights = SemanticWeights {
        ngram: FiniteF32::ZERO,
        cooccurrence: FiniteF32::ZERO,
        synonym: FiniteF32::ZERO,
    };
    state
}

#[test]
fn attached_symbols_break_ties_by_node_index_and_repeat_exactly() {
    let temp = tempfile::tempdir().expect("tempdir");
    let query =
        "Where does the router handle each incoming request before dispatching the response?";
    let plain = "file::src/router.rs::fn::handle_request";
    let second = "file::src/router.rs::fn::handle_request#2";

    let mut state = overload_state(&temp.path().join("a"), ("", 20), ("#2", 120));
    let first_call = seek_json(&mut state, seek_input(query, None, &[]));
    let second_call = seek_json(&mut state, seek_input(query, None, &[]));
    assert_eq!(
        first_call["result_granularity"], "file_group",
        "{first_call:#}"
    );
    assert_eq!(
        symbol_ids(row(&first_call, "file::src/router.rs")),
        vec![plain, second]
    );
    assert_eq!(first_call, second_call, "identical across calls");

    // Swapping insertion order swaps the symbols. Had the two scores differed,
    // the better one would lead in both graphs, so this proves the tie is real
    // and is broken by node index alone.
    let mut swapped = overload_state(&temp.path().join("b"), ("#2", 120), ("", 20));
    let output = seek_json(&mut swapped, seek_input(query, None, &[]));
    assert_eq!(
        symbol_ids(row(&output, "file::src/router.rs")),
        vec![second, plain]
    );
}

#[test]
fn graph_diagnostic_tags_share_their_prefixes_and_stay_out_of_file_summaries() {
    use m1nd_ingest::resolve::{ambiguous_edge_tag, EDGE_AMBIGUOUS_TAG, EDGE_UNRESOLVED_TAG};
    let ambiguous_target = ambiguous_edge_tag("file::src/a.rs::fn::b");
    for tag in [
        EDGE_UNRESOLVED_TAG,
        EDGE_AMBIGUOUS_TAG,
        ambiguous_target.as_str(),
        m1nd_ingest::INGEST_CONTRIBUTION_DROPPED_TAG,
        // A future diagnostic of either family needs no change here to stay out.
        "m1nd:edge:shadowed",
        "m1nd:ingest:truncated",
    ] {
        assert!(super::l2_is_graph_diagnostic_tag(tag), "{tag}");
    }
    for tag in [
        "rust",
        "typescript",
        "m1nd:ui",
        "edge:unresolved",
        "ingest:x",
    ] {
        assert!(!super::l2_is_graph_diagnostic_tag(tag), "{tag}");
    }
}

/// With `detail`, a node row keeps every tag in its summary; the default row's
/// short intent line leaves the edge-resolution tags out.
#[test]
fn node_granularity_keeps_the_full_intent_summary() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = guard_state(temp.path());
    let output = seek_json(&mut state, detail_input("session_guard.rs", None));
    assert_eq!(output["result_granularity"], "node", "{output:#}");
    let row = row(&output, "file::src/auth/session_guard.rs");
    assert_eq!(
        row["intent_summary"],
        "session_guard.rs (file) [rust, m1nd:edge:unresolved, m1nd:edge:ambiguous, \
         m1nd:edge:ambiguous:file::src/billing/invoice.rs::fn::compute_invoice_total]"
    );
    assert!(row.get("symbols").is_none(), "{row:#}");

    let lean = seek_json(&mut state, seek_input("session_guard.rs", None, &[]));
    assert_eq!(
        self::row(&lean, "file::src/auth/session_guard.rs")["intent_summary"],
        "session_guard.rs (file) [rust]"
    );
}

/// Only the first ten rows carry symbols: the rows an agent reads first, and a
/// bounded packet however large `top_k` is.
#[test]
fn only_the_first_ten_file_groups_carry_symbols() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut graph = Graph::new();
    for file in 0..14 {
        let path = format!("src/auth/guard_{file:02}.rs");
        add_file(&mut graph, &path, &format!("guard_{file:02}.rs"), 100);
        add_fn(&mut graph, &path, "validate_credentials", (10, 30));
    }
    let mut state = session(temp.path(), graph);
    let mut input = seek_input(NARRATIVE, None, &[]);
    input.top_k = 14;
    let output = seek_json(&mut state, input);
    assert_eq!(output["result_granularity"], "file_group");
    assert_eq!(rows(&output).len(), 14, "{output:#}");
    for (position, row) in rows(&output).iter().enumerate() {
        assert_eq!(
            row.get("symbols").is_some(),
            position < 10,
            "row {position}: {row:#}"
        );
    }
}

#[test]
fn token_budget_counts_attached_symbols_in_seek_and_focus() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = guard_state(temp.path());

    // The budget is measured on the full (`detail`) rows, so the default rows
    // report the same budget block and keep the same rows.
    let output = seek_json(&mut state, detail_input(NARRATIVE, Some(100_000)));
    assert!(
        rows(&output)[0].get("symbols").is_some(),
        "budgeted seek keeps the symbols: {output:#}"
    );
    let expected: u64 = rows(&output).iter().map(row_token_estimate).sum();
    assert_eq!(output["budget"]["estimated_used_tokens"], expected);
    let mut input = seek_input(NARRATIVE, None, &[]);
    input.token_budget = Some(100_000);
    let lean = seek_json(&mut state, input);
    assert_eq!(lean["budget"], output["budget"]);

    let focus = handle_focus(
        &mut state,
        serde_json::from_value(json!({
            "goal": NARRATIVE,
            "agent_id": "file-group-symbols",
            "token_budget": 100_000,
            "min_score": 0.0
        }))
        .expect("focus input"),
    )
    .expect("focus");
    let focus = serde_json::to_value(focus).expect("serialize focus");
    let focus_set = focus["focus_set"].as_array().expect("focus_set");
    assert!(
        focus_set[0].get("symbols").is_some(),
        "focus inherits the symbols: {focus:#}"
    );
    let expected: u64 = focus_set.iter().map(row_token_estimate).sum();
    assert_eq!(focus["budget"]["estimated_used_tokens"], expected);
}

#[test]
fn semantic_search_rows_keep_their_shape() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = guard_state(temp.path());
    let seek = seek_json(&mut state, seek_input(NARRATIVE, None, &[]));
    let search = dispatch_tool(
        &mut state,
        "search",
        &json!({
            "agent_id": "file-group-symbols",
            "query": NARRATIVE,
            "mode": "semantic",
            "auto_ingest": false,
            "detail": true
        }),
    )
    .expect("semantic search");
    let search_rows = search["results"].as_array().expect("search rows");
    assert!(!search_rows.is_empty(), "{search:#}");
    for row in search_rows {
        let mut keys: Vec<&str> = row
            .as_object()
            .expect("row object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "context_after",
                "context_before",
                "file_path",
                "graph_linked",
                "heuristic_signals",
                "label",
                "line_number",
                "matched_line",
                "node_id",
                "score",
                "type"
            ],
            "{row:#}"
        );
    }
    let seek_ids: Vec<&Value> = rows(&seek).iter().map(|row| &row["node_id"]).collect();
    let search_ids: Vec<&Value> = search_rows.iter().map(|row| &row["node_id"]).collect();
    assert_eq!(search_ids, seek_ids);
    assert_eq!(search_rows[0]["line_number"], 1);
}

/// north's memory recall seeks inside the `light::` scope and maps each hit to
/// its own row. Neither the recall seek nor the packet may change: a narrative
/// task (file-group mode, which has no in-scope file inside `light::` and falls
/// back to the broad recall) and a short task (node mode) are both pinned.
#[test]
fn north_memory_recall_is_unchanged() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut state = guard_state(temp.path());
    let memory_dir = temp.path().join("memory");
    std::fs::create_dir_all(&memory_dir).expect("memory dir");
    std::fs::write(
        memory_dir.join("session_access.light.md"),
        "---\nProtocol: L1GHT/1.0\nNode: SessionAccessDecision\nState: verified\n\
         Created: 1700000000000\nSource-Agent: agent-mem\n---\n\n\
         # SessionAccessDecision\n\n## Session access decision note\n\n\
         Decision note: validation checks credentials before granting account session access.\n\n\
         [⍂ entity: session access credentials validation]\n[𝔻 confidence: 0.9]\n",
    )
    .expect("write memory");
    crate::tools::handle_ingest(
        &mut state,
        crate::protocol::core::IngestInput {
            path: memory_dir.to_string_lossy().to_string(),
            agent_id: "agent-mem".into(),
            incremental: false,
            adapter: "light".into(),
            mode: "merge".into(),
            namespace: Some("light".into()),
            include_dotfiles: false,
            dotfile_patterns: vec![],
            project_root: None,
        },
    )
    .expect("light ingest");
    #[cfg(feature = "embed")]
    {
        // The ingest rebuilt the semantic engine; pin the no-model fallback again.
        state.orchestrator.semantic.embedder = None;
        state.orchestrator.semantic.embeddings.clear();
    }

    let mut recall = seek_input(NARRATIVE, Some("light::"), &[]);
    recall.min_score = 0.1;
    recall.top_k = 24;
    recall.graph_rerank = true;
    recall.conformance_aware = true;
    let recall = seek_json(&mut state, recall);
    assert_eq!(recall["result_granularity"], "file_group");
    assert!(rows(&recall).is_empty(), "{recall:#}");

    let row = |id: &str, claim: &str| {
        json!({
            "claim": claim,
            "kind": "light",
            "node_id": format!("light::light::{id}"),
            "source_agent": "agent-mem",
            "stale": true,
            "tier": "medulla"
        })
    };
    let section = row(
        "section::session-access-light-md::session-access-decision-note-1",
        "Session access decision note",
    );
    let node = row(
        "meta::session-access-light-md::node",
        "SessionAccessDecision",
    );
    let expected = [
        (NARRATIVE, vec![section.clone(), node.clone()]),
        (
            "session access decision note",
            vec![
                section,
                node,
                row("file::session-access-light-md", "session_access.light.md"),
                row("meta::session-access-light-md::protocol", "L1GHT/1.0"),
                row("meta::session-access-light-md::state", "verified"),
            ],
        ),
    ];
    for (task, expected) in expected {
        let north = dispatch_tool(
            &mut state,
            "north",
            &json!({"agent_id": "northerner", "task": task, "detail": true}),
        )
        .expect("north");
        let memory: Vec<Value> = north["memory"]
            .as_array()
            .expect("memory array")
            .iter()
            .map(|entry| {
                let mut entry = entry.clone();
                // Age is relative to the wall clock; every other byte is pinned.
                entry.as_object_mut().expect("row").remove("age_ms");
                entry
            })
            .collect();
        assert_eq!(memory, expected, "task={task:?}");
    }
}
