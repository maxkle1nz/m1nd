use super::*;
use crate::server::{dispatch_tool, McpConfig};
use m1nd_core::{domain::DomainConfig, graph::Graph, types::NodeType};
use serde_json::{json, Value};

fn state(root: &Path) -> SessionState {
    let runtime = root.join("runtime");
    std::fs::create_dir_all(&runtime).unwrap();
    let config = McpConfig {
        graph_source: runtime.join("graph.json"),
        plasticity_state: runtime.join("plasticity.json"),
        runtime_dir: Some(runtime),
        ..Default::default()
    };
    SessionState::initialize(Graph::default(), &config, DomainConfig::code()).unwrap()
}

fn search(state: &mut SessionState, mode: &str) -> Value {
    dispatch_tool(
        state,
        "search",
        &json!({
            "agent_id":"identity-test", "query":"unique_marker", "mode":mode,
            "auto_ingest":false
        }),
    )
    .unwrap()
}

fn prove_single_file_workflow(canonical_root: bool) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let file = source.join("token_validator.rs");
    std::fs::write(&file, "// unique_marker\npub struct TokenValidator;\n").unwrap();
    let root = if canonical_root {
        file.canonicalize().unwrap()
    } else {
        file
    };
    let mut state = state(dir.path());
    dispatch_tool(
        &mut state,
        "ingest",
        &json!({
            "agent_id":"identity-test", "path":root, "adapter":"code", "mode":"merge"
        }),
    )
    .unwrap();
    let canonical = "file::token_validator.rs";
    assert!(state.graph.read().resolve_id(canonical).is_some());
    for mode in ["literal", "regex"] {
        let result = search(&mut state, mode);
        let rows = result["results"].as_array().unwrap();
        assert!(!rows.is_empty(), "{result}");
        for row in rows {
            assert_eq!(row["node_id"], canonical, "{result}");
            assert_eq!(row["graph_linked"], true, "{result}");
            assert!(state
                .graph
                .read()
                .resolve_id(row["node_id"].as_str().unwrap())
                .is_some());
        }
        let created = dispatch_tool(
            &mut state,
            "lock_create",
            &json!({
                "agent_id":"identity-test", "scope":"node", "root_nodes":[rows[0]["node_id"]]
            }),
        )
        .unwrap();
        let arguments = json!({"agent_id":"identity-test", "lock_id":created["lock_id"]});
        let diff = dispatch_tool(&mut state, "lock_diff", &arguments).unwrap();
        assert_eq!(diff["diff"]["no_changes"], true);
        assert_eq!(
            dispatch_tool(&mut state, "lock_release", &arguments).unwrap()["released"],
            true
        );
    }
}

#[test]
fn single_file_search_identity_supports_literal_regex_and_lock_workflow() {
    prove_single_file_workflow(false);
}

#[test]
fn single_file_search_identity_accepts_native_canonical_root() {
    // Windows canonicalize adds its verbatim prefix; the returned graph ID must
    // remain portable and relative, not inherit that filesystem spelling.
    prove_single_file_workflow(true);
}

fn file_root_state(dir: &Path, roots: &[PathBuf]) -> SessionState {
    let mut state = state(dir);
    state.workspace_root = None;
    state.ingest_roots = roots
        .iter()
        .map(|root| root.to_string_lossy().into_owned())
        .collect();
    state
        .graph
        .write()
        .add_node("file::same.rs", "same.rs", NodeType::File, &[], 0.0, 0.0)
        .unwrap();
    state
}

#[test]
fn single_file_search_identity_refuses_ambiguous_basename_roots() {
    let dir = tempfile::tempdir().unwrap();
    let roots: Vec<_> = ["a", "b"]
        .into_iter()
        .map(|name| {
            let root = dir.path().join(name);
            std::fs::create_dir(&root).unwrap();
            let file = root.join("same.rs");
            std::fs::write(&file, format!("// unique_marker_{name}\n")).unwrap();
            file
        })
        .collect();
    let mut state = file_root_state(dir.path(), &roots);
    // Also exercise disk fallback within an independently authorized workspace.
    state.workspace_root = Some(dir.path().to_string_lossy().into_owned());
    let result = search(&mut state, "literal");
    assert!(
        result["results"].as_array().unwrap().is_empty(),
        "ambiguous graph identity must not choose or relabel either root: {result}"
    );
}

#[test]
fn single_file_search_identity_deduplicates_canonical_root_aliases() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("same.rs");
    std::fs::write(&file, "// unique_marker\n").unwrap();
    let mut state = file_root_state(dir.path(), &[file.clone(), file.canonicalize().unwrap()]);
    let result = search(&mut state, "literal");
    let rows = result["results"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{result}");
    assert_eq!(rows[0]["node_id"], "file::same.rs");
    assert_eq!(rows[0]["graph_linked"], true);
}

#[test]
fn single_file_search_identity_never_relativizes_to_empty() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("same.rs");
    std::fs::write(&file, "// unique_marker\n").unwrap();
    let roots = vec![file.to_string_lossy().into_owned()];
    assert_ne!(
        relativize_against_ingest_roots_slice(&roots, &file).as_deref(),
        Some("")
    );
}

#[test]
fn single_file_search_identity_does_not_authorize_parent_or_sibling() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("same.rs");
    std::fs::write(&file, "// allowed_marker\n").unwrap();
    let sibling = dir.path().join("sibling.rs");
    std::fs::write(&sibling, "// unique_marker\n").unwrap();
    let mut state = file_root_state(dir.path(), &[file]);
    for scope in [
        sibling.to_string_lossy().into_owned(),
        dir.path().to_string_lossy().into_owned(),
        "../sibling.rs".into(),
    ] {
        let outside = dispatch_tool(
            &mut state,
            "search",
            &json!({
                "agent_id":"identity-test", "query":"unique_marker", "mode":"literal",
                "scope":scope, "auto_ingest":false
            }),
        );
        if let Ok(result) = outside {
            assert!(
                result["results"].as_array().unwrap().is_empty(),
                "single-file root must not expose its sibling: {result}"
            );
        }
    }
}
