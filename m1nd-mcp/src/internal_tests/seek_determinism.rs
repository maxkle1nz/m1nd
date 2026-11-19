//! The same graph and query must give the same `seek` answer in every process.
//! Each session below builds its own semantic engine and file index, so their
//! hash maps iterate in different orders, as two server processes would.
use crate::layer_handlers::handle_seek;
use crate::protocol::layers::SeekInput;
use crate::server::McpConfig;
use crate::session::SessionState;
use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::{Graph, NodeProvenanceInput};
use m1nd_core::types::NodeType;
use serde_json::{json, Value};

fn add(graph: &mut Graph, external: &str, label: &str, kind: NodeType, path: &str, line: u32) {
    let node = graph
        .add_node(external, label, kind, &[], 0.0, 0.0)
        .expect("add node");
    graph.set_node_provenance(
        node,
        NodeProvenanceInput {
            source_path: Some(path),
            line_start: Some(line),
            line_end: Some(line + 9),
            excerpt: None,
            namespace: None,
            canonical: true,
        },
    );
}

/// Thirty files that each hold the same three symbol names: more equal
/// semantic scores than seek's semantic candidate window (5 x top_k) admits, so
/// the cut has to choose among ties.
fn session(root: &std::path::Path) -> SessionState {
    let mut graph = Graph::new();
    for file in 0..30 {
        let path = format!("src/metrics_{file}.rs");
        add(
            &mut graph,
            &format!("file::{path}"),
            &format!("metrics_{file}.rs"),
            NodeType::File,
            &path,
            1,
        );
        for (offset, name) in ["record_tick_metrics", "intent_counts", "server_timings"]
            .iter()
            .enumerate()
        {
            add(
                &mut graph,
                &format!("file::{path}::fn::{name}"),
                name,
                NodeType::Function,
                &path,
                10 + 20 * offset as u32,
            );
        }
    }
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
        state.orchestrator.semantic.embedder = None;
        state.orchestrator.semantic.embeddings.clear();
    }
    state
}

/// Everything ranking decides: ids, exact score bits, and attached symbols.
fn answer(state: &mut SessionState, query: &str, top_k: usize) -> (String, Vec<Value>) {
    let input: SeekInput = serde_json::from_value(json!({
        "agent_id": "seek-determinism",
        "query": query,
        "top_k": top_k,
        "min_score": 0.0,
        "conformance_aware": false
    }))
    .expect("seek input");
    let output = handle_seek(state, input).expect("seek");
    let rows = output
        .results
        .iter()
        .map(|row| {
            json!({
                "node_id": row.node_id,
                "score_bits": row.score.to_bits(),
                "symbols": serde_json::to_value(&row.symbols).expect("symbols"),
            })
        })
        .collect();
    (output.result_granularity, rows)
}

#[test]
fn seek_answers_identically_across_independently_built_sessions() {
    let queries = [
        (
            "Which production code records per tick timings and intent counts into server metrics?",
            10,
        ),
        ("record_tick_metrics", 1),
    ];
    let temp = tempfile::tempdir().expect("tempdir");
    for (case, (query, top_k)) in queries.into_iter().enumerate() {
        // Every session gets its own runtime directory, so none can reuse a
        // cache another one wrote.
        let mut reference = session(&temp.path().join(format!("case-{case}-reference")));
        let expected = answer(&mut reference, query, top_k);
        assert!(!expected.1.is_empty(), "{query}");
        for run in 0..6 {
            let mut other = session(&temp.path().join(format!("case-{case}-run-{run}")));
            assert_eq!(
                answer(&mut other, query, top_k),
                expected,
                "query {query:?} answered differently by an identical session"
            );
        }
    }
}
