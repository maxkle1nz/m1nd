//! Scope and node-type eligibility must survive every seek ranking path.
use crate::layer_handlers::handle_seek;
use crate::protocol::layers::{SeekInput, SeekOutput};
use crate::server::McpConfig;
use crate::session::SessionState;
use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::{Graph, NodeProvenanceInput};
use m1nd_core::types::{FiniteF32, NodeType, SemanticWeights};

const QUERY: &str = "needle_scope_match";

fn add_node(graph: &mut Graph, path: &str, label: &str, kind: NodeType) {
    let external = if kind == NodeType::File {
        format!("file::{path}")
    } else {
        format!("file::{path}::fn::{label}")
    };
    let node = graph
        .add_node(&external, label, kind, &[], 0.0, 0.0)
        .expect("add node");
    graph.set_node_provenance(
        node,
        NodeProvenanceInput {
            source_path: Some(path),
            line_start: Some(1),
            line_end: Some(2),
            excerpt: None,
            namespace: None,
            canonical: true,
        },
    );
}

fn state(root: &std::path::Path, mut graph: Graph) -> SessionState {
    graph.finalize().expect("finalize graph");
    let runtime = root.join("runtime");
    std::fs::create_dir_all(&runtime).expect("create runtime");
    let config = McpConfig {
        graph_source: runtime.join("graph.json"),
        plasticity_state: runtime.join("plasticity.json"),
        runtime_dir: Some(runtime),
        ..Default::default()
    };
    let mut state =
        SessionState::initialize(graph, &config, DomainConfig::code()).expect("session");
    state.ingest_roots = vec![root.to_string_lossy().into_owned()];
    state.workspace_root = Some(root.to_string_lossy().into_owned());
    #[cfg(feature = "embed")]
    {
        // Default tests exercise the no-model fallback even on machines with a model.
        state.orchestrator.semantic.embedder = None;
        state.orchestrator.semantic.embeddings.clear();
    }
    state
}

fn input(scope: Option<String>, node_types: &[&str], top_k: usize) -> SeekInput {
    serde_json::from_value(serde_json::json!({
        "agent_id": "seek-filter-test",
        "query": QUERY,
        "scope": scope,
        "node_types": node_types,
        "top_k": top_k,
        "graph_rerank": false,
        "conformance_aware": false
    }))
    .expect("seek defaults")
}

fn without_symbolic_semantics(state: &mut SessionState) {
    state.orchestrator.semantic.weights = SemanticWeights {
        ngram: FiniteF32::ZERO,
        cooccurrence: FiniteF32::ZERO,
        synonym: FiniteF32::ZERO,
    };
}

fn assert_empty(output: &SeekOutput) {
    assert!(
        output.results.is_empty(),
        "unexpected hits: {:?}",
        output.results
    );
    assert_eq!(output.relevance_clearing_total, 0);
    assert_eq!(output.sufficiency.state, "saturated");
    assert_eq!(output.sufficiency.top_score, 0.0);
    assert_eq!(output.proof_state, "blocked");
    assert!(!output.embeddings_used);
    assert!(output
        .filtering_reason
        .as_deref()
        .is_some_and(|s| s.contains("excluded all")));
}

fn assert_filter_matrix(label: &str, configure: impl FnOnce(&mut SessionState)) {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut graph = Graph::new();
    for scope in ["zone_a", "zone_b"] {
        let path = format!("{scope}/alpha.rs");
        add_node(&mut graph, &path, label, NodeType::File);
        add_node(&mut graph, &path, label, NodeType::Function);
    }
    let mut state = state(temp.path(), graph);
    configure(&mut state);
    let scopes = [
        None,
        Some("zone_a".into()),
        Some("file::zone_a".into()),
        Some(temp.path().join("zone_a").to_string_lossy().into_owned()),
        Some("missing_scope".into()),
    ];
    let filters: &[&[&str]] = &[
        &[],
        &["function"],
        &["FUNCTION"],
        &["file"],
        &["function", "file"],
        &["struct"],
        &["unknown_type"],
    ];
    for scope in scopes {
        for &types in filters {
            let output = handle_seek(&mut state, input(scope.clone(), types, 20)).expect("seek");
            let scope_count = match scope.as_deref() {
                None => 2,
                Some("missing_scope") => 0,
                _ => 1,
            };
            let type_count = if types.is_empty() {
                2
            } else {
                types
                    .iter()
                    .filter(|t| t.eq_ignore_ascii_case("function") || **t == "file")
                    .count()
            };
            let expected = scope_count * type_count;
            assert_eq!(output.result_granularity, "node");
            assert_eq!(
                output.total_candidates_scanned, expected,
                "scope={scope:?}, types={types:?}"
            );
            // Same-label nodes are deliberately deduplicated after ranking.
            assert_eq!(
                output.results.is_empty(),
                expected == 0,
                "scope={scope:?}, types={types:?}: {:?}",
                output.results
            );
            assert!(output.results.len() <= expected);
            assert_eq!(output.relevance_clearing_total, expected);
            for hit in &output.results {
                if scope.is_some() {
                    assert!(hit.node_id.starts_with("file::zone_a/"), "{}", hit.node_id);
                }
                assert!(
                    types.is_empty()
                        || types
                            .iter()
                            .any(|kind| hit.node_type.eq_ignore_ascii_case(kind))
                );
            }
            if expected == 0 {
                assert_empty(&output);
            }
        }
    }
}

#[test]
fn seek_filters_lexical_candidates() {
    assert_filter_matrix(QUERY, without_symbolic_semantics);
}

#[test]
fn seek_filters_legacy_semantic_candidates() {
    assert_filter_matrix(QUERY, |_| {});
}

#[cfg(feature = "embed")]
fn inject_embeddings(state: &mut SessionState) {
    let graph = state.graph.read();
    state.orchestrator.semantic = m1nd_core::semantic::SemanticEngine::with_injected_embedder(
        &graph,
        SemanticWeights::default(),
        std::sync::Arc::new(m1nd_core::embed::FakeEmbedder::with_anchor(
            64,
            &[QUERY, "qz9wb"],
            1.0,
        )),
        None,
        false,
    )
    .expect("injected embeddings");
}

#[cfg(feature = "embed")]
#[test]
fn seek_filters_static_embedding_candidates() {
    assert_filter_matrix("qz9wb", |state| {
        inject_embeddings(state);
        without_symbolic_semantics(state);
    });
}

#[cfg(feature = "embed")]
#[test]
fn seek_filters_hybrid_candidates() {
    assert_filter_matrix(QUERY, inject_embeddings);
}

#[test]
fn seek_filters_before_semantic_overfetch_can_starve_eligible_nodes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut graph = Graph::new();
    add_node(
        &mut graph,
        "scope_a/adapter.rs",
        "needle_scope_match_adapter",
        NodeType::Function,
    );
    for i in 0..40 {
        add_node(
            &mut graph,
            &format!("scope_b/decoy_{i}.rs"),
            QUERY,
            NodeType::Function,
        );
    }
    let mut state = state(temp.path(), graph);
    let narrow = handle_seek(&mut state, input(Some("scope_a".into()), &["function"], 1))
        .expect("narrow seek");
    let wide = handle_seek(&mut state, input(Some("scope_a".into()), &["function"], 20))
        .expect("wide seek");
    assert_eq!(narrow.results[0].node_id, wide.results[0].node_id);
    assert!(
        (narrow.results[0].score - wide.results[0].score).abs() < 1e-6,
        "excluded nodes consumed semantic overfetch: narrow={}, wide={}",
        narrow.results[0].score,
        wide.results[0].score
    );
    assert_eq!(narrow.results.len(), 1);
    assert_eq!(wide.results.len(), 1);
    assert_eq!(narrow.total_candidates_scanned, 1);
    assert_eq!(narrow.relevance_clearing_total, 1);
}

#[test]
fn seek_filters_reproduce_two_file_scope_and_missing_type() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut graph = Graph::new();
    add_node(
        &mut graph,
        "scope_a/local.rs",
        "local_guard",
        NodeType::Function,
    );
    add_node(&mut graph, "scope_b/needle.rs", QUERY, NodeType::Function);
    add_node(&mut graph, "scope_b/needle.rs", "needle.rs", NodeType::File);
    let mut state = state(temp.path(), graph);
    let scoped = handle_seek(&mut state, input(Some("scope_a".into()), &["function"], 20))
        .expect("scoped seek");
    assert_eq!(scoped.total_candidates_scanned, 1);
    assert!(
        scoped
            .results
            .iter()
            .all(|hit| hit.node_id.starts_with("file::scope_a/") && hit.node_type == "function"),
        "{:?}",
        scoped.results
    );
    let no_struct = handle_seek(&mut state, input(None, &["struct"], 20)).expect("type seek");
    assert_eq!(no_struct.total_candidates_scanned, 0);
    assert_empty(&no_struct);
}

#[test]
fn seek_filters_file_group_representatives_by_scope_and_type() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut graph = Graph::new();
    for (scope, file) in [("zone_a", "auth_a.rs"), ("zone_b", "auth_b.rs")] {
        let path = format!("{scope}/{file}");
        add_node(&mut graph, &path, file, NodeType::File);
        add_node(
            &mut graph,
            &path,
            "validate_credentials",
            NodeType::Function,
        );
    }
    let mut state = state(temp.path(), graph);
    for scope in [None, Some("zone_a"), Some("missing_scope")] {
        let mut request = input(scope.map(str::to_owned), &["file"], 20);
        request.query =
            "where does validation check credentials before granting account session access".into();
        let output = handle_seek(&mut state, request).expect("file-group seek");
        assert_eq!(output.result_granularity, "file_group");
        let expected = match scope {
            None => 2,
            Some("zone_a") => 1,
            _ => 0,
        };
        assert_eq!(output.results.len(), expected);
        assert_eq!(output.total_candidates_scanned, expected);
        assert_eq!(output.relevance_clearing_total, expected);
        for hit in &output.results {
            assert_eq!(hit.node_type, "file");
            if scope.is_some() {
                assert_eq!(hit.node_id, "file::zone_a/auth_a.rs");
            }
        }
        if expected == 0 {
            assert_empty(&output);
        }
    }
}

#[test]
fn seek_filters_file_group_representatives_even_with_conflicting_provenance() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut graph = Graph::new();
    add_node(
        &mut graph,
        "zone_b/auth.rs",
        "validate_credentials",
        NodeType::File,
    );
    graph.set_node_provenance(
        m1nd_core::types::NodeId::new(0),
        NodeProvenanceInput {
            source_path: Some("zone_a/auth.rs"),
            line_start: Some(1),
            line_end: Some(2),
            excerpt: None,
            namespace: None,
            canonical: true,
        },
    );
    let mut state = state(temp.path(), graph);
    let mut request = input(Some("zone_a".into()), &["file"], 20);
    request.query =
        "where does validation check credentials before granting account session access".into();
    let output = handle_seek(&mut state, request).expect("file-group seek");
    assert_eq!(output.result_granularity, "file_group");
    assert_eq!(output.total_candidates_scanned, 0);
    assert_empty(&output);
}
