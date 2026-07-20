// === Battery: the `transplant` verb (graph-addressed cross-file move) ===
//
// Contract tests in the house tradition of tests/test_surgical_v2.rs: they
// COMPILE now and FAIL until the verb exists — the RED half of the
// proof-grown loop. Contract source: the donors deep-study
// (~/donors-study/STUDY.md §7.1) — the 7-phase spec composed from rope
// (region widening, placeholder rename, over-provision→prune imports),
// ts-morph (dependency trichotomy, canonical phase order), Roslyn
// (dest-collision preflight) and recast (indent + trivia ownership).
//
// Canonical scenario:
//   alpha.rs — `move_me` (with doc comment) + `private_helper` (used ONLY by
//              move_me → must TRAVEL) + `shared_helper` (used by move_me AND
//              stay_here → must STAY and become pub(crate), back-imported)
//   beta.rs  — destination, with an existing resident item
//   gamma.rs — external referencer (`use crate::alpha::move_me`)
//
// NOTE on addressing: whether the final verb takes `node_id` or
// `symbol` + `source_file` is a PRD decision; what this battery pins is the
// OBSERVABLE on-disk contract. Adjust the params here when the PRD lands —
// the asserts stay.

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use std::path::Path;

// ===========================================================================
// Shared test infrastructure (mirrors tests/counterfactual_behavior.rs)
// ===========================================================================

fn make_state(root: &Path) -> SessionState {
    let config = McpConfig {
        graph_source: root.join("graph_snapshot.json"),
        plasticity_state: root.join("plasticity_state.json"),
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

// Used ONLY by move_me -> must travel with it (trichotomy: private).
fn private_helper(x: u32) -> u32 {
    x * 2
}

// Used by move_me AND stay_here -> must STAY, gain pub(crate), be back-imported.
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

/// Writes the canonical fixture crate and ingests it into the session graph.
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

    let ingest = m1nd_mcp::tools::handle_ingest(
        state,
        m1nd_mcp::protocol::IngestInput {
            path: root.to_string_lossy().to_string(),
            agent_id: "battery".to_string(),
            mode: "merge".to_string(),
            incremental: false,
            adapter: "code".to_string(),
            namespace: None,
            include_dotfiles: false,
            dotfile_patterns: Vec::new(),
            project_root: None,
        },
    )
    .expect("fixture ingest");
    let nodes = ingest
        .get("node_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    assert!(
        nodes >= 5,
        "fixture must produce a populated graph, got {nodes} nodes"
    );
}

fn transplant_params(root: &Path) -> serde_json::Value {
    serde_json::json!({
        "agent_id": "battery",
        "symbol": "move_me",
        "source_file": root.join("src/alpha.rs").to_string_lossy(),
        "dest_file": root.join("src/beta.rs").to_string_lossy(),
    })
}

// ===========================================================================
// Battery 1 — the canonical move: dependency trichotomy on disk
// ===========================================================================

#[test]
fn battery_transplant_moves_symbol_with_dependency_trichotomy() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_fixture(&mut state, root);

    // THE VERB UNDER TEST — does not exist yet: this expect IS the red.
    dispatch_tool(&mut state, "transplant", &transplant_params(root))
        .expect("the `transplant` verb must exist and succeed on the canonical scenario");

    let alpha = std::fs::read_to_string(root.join("src/alpha.rs")).unwrap();
    let beta = std::fs::read_to_string(root.join("src/beta.rs")).unwrap();
    let gamma = std::fs::read_to_string(root.join("src/gamma.rs")).unwrap();

    // Destination gained: the item, its doc comment (trivia travels), its private dep.
    assert!(
        beta.contains("fn move_me"),
        "dest must gain the item:\n{beta}"
    );
    assert!(
        beta.contains("/// Doc comment that must TRAVEL"),
        "the doc comment travels with the item (trivia-ownership law):\n{beta}"
    );
    assert!(
        beta.contains("fn private_helper"),
        "private dep travels (trichotomy):\n{beta}"
    );
    assert!(
        beta.contains("fn existing_resident"),
        "resident item untouched:\n{beta}"
    );

    // Source lost the item and the private dep…
    assert!(
        !alpha.contains("fn move_me"),
        "source must lose the item:\n{alpha}"
    );
    assert!(
        !alpha.contains("fn private_helper"),
        "private dep must not stay behind:\n{alpha}"
    );
    // …kept the shared dep with a visibility bump, and the sibling intact.
    assert!(
        alpha.contains("pub(crate) fn shared_helper") || alpha.contains("pub fn shared_helper"),
        "shared dep stays with a visibility bump:\n{alpha}"
    );
    assert!(
        alpha.contains("fn stay_here"),
        "unrelated sibling stays intact:\n{alpha}"
    );

    // External referencer re-qualified to the new home, old path gone.
    assert!(
        gamma.contains("beta::move_me") || gamma.contains("crate::beta"),
        "referencer must point at the new home:\n{gamma}"
    );
    assert!(
        !gamma.contains("alpha::move_me"),
        "old path must be gone:\n{gamma}"
    );
}

// ===========================================================================
// Battery 2 — dest-collision preflight refuses and changes NOTHING
// (the check rope lacks; the Roslyn FindTypeInSolution lesson)
// ===========================================================================

#[test]
fn battery_transplant_refuses_dest_collision_and_touches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_fixture(&mut state, root);

    // Poison the destination: it already defines a `move_me`.
    let poisoned = format!("{BETA}\npub fn move_me(x: u32) -> u32 {{\n    x\n}}\n");
    write(&root.join("src/beta.rs"), &poisoned);

    let err = dispatch_tool(&mut state, "transplant", &transplant_params(root))
        .expect_err("a dest collision must be refused");
    let msg = format!("{err:?}").to_lowercase();
    assert!(
        msg.contains("collision") || msg.contains("already define"),
        "the refusal must name the collision (not be a generic/unknown-tool error), got: {msg}"
    );

    // The anti-corruption law: a refusal leaves every file byte-identical.
    assert_eq!(
        std::fs::read_to_string(root.join("src/alpha.rs")).unwrap(),
        ALPHA
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/beta.rs")).unwrap(),
        poisoned
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/gamma.rs")).unwrap(),
        GAMMA
    );
}
