// === Battery: A1 — moved-node identity across re-ingest (the deepest edge) ===
//
// After the transplant re-ingests, the moved symbol's node is RECREATED under a
// NEW external_id (a fn node id is `file::<path>::fn::<name>` — path-dependent),
// so the OLD id orphans. The PRD (§5.A1, verdict-widened) names THREE classes of
// node-addressed state and asks which FOLLOW the symbol and which orphan:
//
//   (a) L1GHT memory evidence  — `[𝔻 evidence: …]` grounded_in edges
//   (b) xray paint / tags      — node tags applied through xray_retag
//   (c) antibody patterns       — structural bug patterns
//
// PROVEN here, per class:
//   (a) evidence binds to the FILE node (`file::<path>`), never the symbol node;
//       both endpoint files persist across a transplant, so it never orphans.
//   (c) antibodies are STRUCTURAL (matched by node_type/tags/label, re-evaluated
//       each scan), so a moved symbol is re-matched in its NEW home automatically.
//   (b) xray tags live ON the graph node, MIXED with structural ingest tags and
//       with no marker separating paint from structure — the re-ingest deletes the
//       node and the painted tags orphan. A clean auto-carry needs owner-side
//       machinery (a stable node id across re-ingest, or a paint-tag registry), so
//       the verb records `state_left_behind[]` instead of silently orphaning, and
//       the IDEAL full-follow is a declared #[ignore]d RED.

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::protocol::IngestInput;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use serde_json::json;
use std::path::Path;

// ===========================================================================
// Shared fixture (mirrors tests/transplant_battery.rs)
// ===========================================================================

mod common;

fn make_state(root: &Path) -> SessionState {
    common::disable_proof_gate_for_logic_tests();
    let config = McpConfig {
        graph_source: root.join("graph_snapshot.json"),
        plasticity_state: root.join("plasticity_state.json"),
        runtime_dir: Some(root.to_path_buf()),
        ..McpConfig::default()
    };
    let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
        .expect("init session");
    state.ingest_roots = vec![root.to_string_lossy().to_string()];
    state
}

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, content).unwrap();
}

const ALPHA: &str = r#"//! Alpha: the transplant SOURCE file.

/// Doc comment that must TRAVEL with the item (trivia-ownership law).
pub fn move_me(x: u32) -> u32 {
    let base = private_helper(x);
    shared_helper(base) + 1
}

fn private_helper(x: u32) -> u32 {
    x * 2
}

fn shared_helper(x: u32) -> u32 {
    x + 10
}

pub fn stay_here(x: u32) -> u32 {
    shared_helper(x)
}
"#;

const BETA: &str = r#"//! Beta: the transplant DESTINATION file.

pub fn existing_resident(x: u32) -> u32 {
    x - 1
}
"#;

const GAMMA: &str = r#"//! Gamma: an external REFERENCER of the moved symbol.

use crate::alpha::move_me;

pub fn call_it() -> u32 {
    move_me(21)
}
"#;

fn ingest_input(path: String, adapter: &str, mode: &str) -> IngestInput {
    IngestInput {
        path,
        agent_id: "identity".into(),
        incremental: false,
        adapter: adapter.into(),
        mode: mode.into(),
        namespace: None,
        include_dotfiles: false,
        dotfile_patterns: vec![],
        project_root: None,
    }
}

fn seed_fixture(state: &mut SessionState, root: &Path) {
    write(
        &root.join("Cargo.toml"),
        "[package]\nname = \"fixture-transplant\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(
        &root.join("src/lib.rs"),
        "pub mod alpha;\npub mod beta;\npub mod gamma;\n",
    );
    write(&root.join("src/alpha.rs"), ALPHA);
    write(&root.join("src/beta.rs"), BETA);
    write(&root.join("src/gamma.rs"), GAMMA);

    let out = m1nd_mcp::tools::handle_ingest(
        state,
        ingest_input(root.to_string_lossy().to_string(), "code", "replace"),
    )
    .expect("fixture ingest");
    assert!(
        out["node_count"].as_u64().unwrap_or(0) >= 5,
        "populated graph"
    );
}

fn transplant_params(root: &Path) -> serde_json::Value {
    json!({
        "agent_id": "identity",
        "symbol": "move_me",
        "source_file": root.join("src/alpha.rs").to_string_lossy(),
        "dest_file": root.join("src/beta.rs").to_string_lossy(),
    })
}

/// The external_id of the (first) node whose label equals `label`, read from the
/// live graph — the id a fn node is addressed by (`file::<path>::fn::<name>`).
fn external_id_of_label(state: &SessionState, label: &str) -> Option<String> {
    let graph = state.graph.read();
    let n = graph.num_nodes() as usize;
    let mut idx_to_ext = vec![String::new(); n];
    for (&interned, &nid) in &graph.id_to_node {
        let idx = nid.as_usize();
        if idx < n {
            idx_to_ext[idx] = graph.strings.resolve(interned).to_string();
        }
    }
    for (i, ext) in idx_to_ext.iter().enumerate() {
        if graph.strings.resolve(graph.nodes.label[i]) == label && !ext.is_empty() {
            return Some(ext.clone());
        }
    }
    None
}

/// True when a `Function` node labelled `label` whose provenance file ends with
/// `file_suffix` carries `tag`. File-precise on purpose: after a transplant the
/// re-ingest may LEAVE the moved symbol's old node lingering, so "does the tag live
/// on the symbol in its NEW home?" must pin the destination file, not just the name.
fn label_in_file_has_tag(state: &SessionState, label: &str, file_suffix: &str, tag: &str) -> bool {
    let graph = state.graph.read();
    let n = graph.num_nodes() as usize;
    for i in 0..n {
        if graph.nodes.node_type[i] != m1nd_core::types::NodeType::Function {
            continue;
        }
        if graph.strings.resolve(graph.nodes.label[i]) != label {
            continue;
        }
        let file = graph.nodes.provenance[i]
            .source_path
            .and_then(|s| graph.strings.try_resolve(s))
            .unwrap_or("");
        if !file.ends_with(file_suffix) {
            continue;
        }
        let nid = m1nd_core::types::NodeId::new(i as u32);
        if graph.node_tags(nid).contains(&tag) {
            return true;
        }
    }
    false
}

// ===========================================================================
// Class (a) — L1GHT memory evidence is FILE-addressed → transplant-safe.
// ===========================================================================

#[test]
fn a1_light_evidence_survives_transplant_because_it_is_file_addressed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_fixture(&mut state, root);

    // A L1GHT memory citing the SOURCE file as evidence.
    write(
        &root.join("notes.md"),
        "---\nProtocol: L1GHT/1.0\nNode: AlphaNotes\n---\n\n## Alpha\n\nThe [⍂ entity: Mover] moves things.\n[𝔻 confidence: 0.8]\n[𝔻 evidence: src/alpha.rs]\n",
    );
    let light = m1nd_mcp::tools::handle_ingest(
        &mut state,
        ingest_input(
            root.join("notes.md").to_string_lossy().to_string(),
            "light",
            "merge",
        ),
    )
    .expect("light ingest");
    assert!(
        light["light_evidence_resolved"].as_u64().unwrap_or(0) >= 1,
        "evidence resolves to the file node before the move"
    );

    let evidence_edge_present = |state: &SessionState| -> bool {
        let graph = state.graph.read();
        let Some(code_node) = graph.resolve_id("file::src/alpha.rs") else {
            return false;
        };
        let Some(grounded) = graph.strings.lookup("grounded_in") else {
            return false;
        };
        graph
            .csr
            .targets
            .iter()
            .zip(graph.csr.relations.iter())
            .any(|(&tgt, &rel)| tgt == code_node && rel == grounded)
    };
    assert!(
        evidence_edge_present(&state),
        "grounded_in edge present before"
    );

    // Move the symbol. alpha.rs LOSES `move_me` but PERSISTS as a file.
    dispatch_tool(&mut state, "transplant", &transplant_params(root)).expect("transplant");

    // The evidence still resolves: it was bound to the FILE node, which persists —
    // L1GHT evidence never binds to a symbol node, so a symbol move cannot orphan it.
    assert!(
        evidence_edge_present(&state),
        "grounded_in edge to file::src/alpha.rs must survive the transplant (file-addressed)"
    );
}

// ===========================================================================
// Class (c) — antibody patterns are STRUCTURAL → follow the moved symbol.
// ===========================================================================

#[test]
fn a1_antibody_pattern_follows_moved_symbol_structurally() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_fixture(&mut state, root);

    dispatch_tool(
        &mut state,
        "antibody_create",
        &json!({
            "agent_id": "identity",
            "action": "create",
            "name": "MoveMePattern",
            "severity": "warning",
            "pattern": {
                "nodes": [
                    { "role": "anchor", "node_type": "function", "label_contains": "move_me" }
                ],
                "edges": []
            }
        }),
    )
    .expect("antibody_create");

    let scan_hits = |state: &mut SessionState| -> usize {
        let out = dispatch_tool(state, "antibody_scan", &json!({"agent_id": "identity"}))
            .expect("antibody_scan");
        out["matches"].as_array().map(|a| a.len()).unwrap_or(0)
    };

    let before = scan_hits(&mut state);
    assert!(before >= 1, "the pattern matches move_me before the move");

    dispatch_tool(&mut state, "transplant", &transplant_params(root)).expect("transplant");

    // The structural pattern re-matches the moved symbol in its NEW home — no
    // node-id rebinding needed, the constraints (function + label move_me) hold.
    let after = scan_hits(&mut state);
    assert!(
        after >= 1,
        "the structural antibody must still match move_me after it moved to beta"
    );
}

// ===========================================================================
// Class (b) — xray tags are node-addressed → orphan on re-ingest. The verb
// records state_left_behind[] rather than silently dropping them.
// ===========================================================================

#[test]
fn a1_xray_tags_orphan_and_are_reported_in_state_left_behind() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_fixture(&mut state, root);

    // Paint a distinctive tag on move_me through the real xray_retag verb.
    let move_me_id = external_id_of_label(&state, "move_me").expect("move_me node present");
    let retag = dispatch_tool(
        &mut state,
        "xray_retag",
        &json!({
            "selector": { "path_prefix": move_me_id },
            "op": "add",
            "tags": ["xray:reviewed"],
            "mode": "commit"
        }),
    )
    .expect("xray_retag");
    assert!(
        retag["counts"]["applied"].as_u64().unwrap_or(0) >= 1,
        "the paint applied to at least move_me"
    );
    assert!(
        label_in_file_has_tag(&state, "move_me", "alpha.rs", "xray:reviewed"),
        "move_me carries the painted tag in alpha before the move"
    );

    let receipt =
        dispatch_tool(&mut state, "transplant", &transplant_params(root)).expect("transplant");

    // The verb NEVER silently orphans: it records the node-addressed state the move
    // left behind, naming the symbol and the orphaned tag.
    let left = receipt
        .get("state_left_behind")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let move_me_entry = left.iter().find(|e| e["symbol"] == "move_me");
    assert!(
        move_me_entry.is_some(),
        "state_left_behind must carry a move_me entry, got {left:?}"
    );
    let entry = move_me_entry.unwrap();
    let detail: Vec<String> = entry["detail"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        detail.iter().any(|t| t == "xray:reviewed"),
        "the orphaned painted tag must be named in the receipt, got {detail:?}"
    );
    assert_eq!(entry["kind"], "xray_tags", "the orphaned state is classed");
    assert!(
        entry["old_node_id"]
            .as_str()
            .is_some_and(|s| s.contains("alpha")),
        "the old node id names the source location"
    );
    assert!(
        entry["new_node_id"]
            .as_str()
            .is_some_and(|s| s.contains("beta")),
        "the new node id names the destination location"
    );
}

/// The IDEAL A1 outcome for xray tags: the painted tag FULLY FOLLOWS to the moved
/// symbol's new home. It cannot be reached cleanly in the lab — xray tags share the
/// node's tag set with structural ingest tags and carry no paint marker, so a
/// faithful auto-carry needs OWNER-SIDE wiring (a stable node identity preserved
/// across re-ingest — the OpenRewrite id the PRD notes is unimplemented — or a
/// paint-tag registry). Until then the verb reports `state_left_behind[]` (proven
/// GREEN above). Kept as a compiling, #[ignore]d RED so the ideal is never lost.
#[test]
#[ignore = "A1 class-b IDEAL: full tag-follow needs owner-side wiring (stable node id across re-ingest or a paint-tag registry); the lab records state_left_behind[] instead"]
fn a1_xray_tags_full_follow_ideal_needs_owner_wiring() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_fixture(&mut state, root);

    let move_me_id = external_id_of_label(&state, "move_me").expect("move_me node present");
    dispatch_tool(
        &mut state,
        "xray_retag",
        &json!({
            "selector": { "path_prefix": move_me_id },
            "op": "add",
            "tags": ["xray:reviewed"],
            "mode": "commit"
        }),
    )
    .expect("xray_retag");

    dispatch_tool(&mut state, "transplant", &transplant_params(root)).expect("transplant");

    // The IDEAL: move_me carries its painted tag in its NEW home (beta.rs
    // specifically — the re-ingest may leave the old alpha node lingering, which is
    // NOT the tag following the symbol). This fails today: beta's fresh node has no
    // paint.
    assert!(
        label_in_file_has_tag(&state, "move_me", "beta.rs", "xray:reviewed"),
        "IDEAL (owner-wired): the painted tag follows the moved symbol to its beta node"
    );
}
