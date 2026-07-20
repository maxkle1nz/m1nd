// === Battery: A5-remainder — transplant × background re-ingest (auto-ingest) ===
//
// The PRD (§5.A5) asks whether a transplant racing a background auto-ingest can be
// corrupted. Exploration finding (recorded for the promotion report):
//   * auto-ingest NEVER writes .rs source — it reads source INTO the graph and
//     writes canonical doc artifacts; so it cannot cause the DISK drift the A2
//     two-phase hash-check guards against.
//   * in the owner's single-threaded dispatch, ticks are SEQUENTIAL with tool calls
//     (maybe_tick_auto_ingest runs around dispatch, never concurrently mid-verb).
// So the only race that can matter is a GRAPH mutation landing between a transplant
// preview and its commit. This battery drives that deterministically via a full
// re-ingest (the exact operation auto-ingest schedules) and proves the staged plan
// is DISK-anchored: its computed contents + base hashes are captured at preview and
// re-validated against DISK at commit, so a graph rebuild between the two phases —
// which re-creates every node under a new id — is HARMLESS.
//
// The complementary direction (a real DISK change between the phases is REFUSED) is
// already proven by tests/transplant_two_phase.rs::
// two_phase_commit_refuses_when_a_derived_referencer_drifted.

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use std::path::Path;

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

fn ingest(state: &mut SessionState, root: &Path) {
    m1nd_mcp::tools::handle_ingest(
        state,
        m1nd_mcp::protocol::IngestInput {
            path: root.to_string_lossy().to_string(),
            agent_id: "race".to_string(),
            mode: "merge".to_string(),
            incremental: false,
            adapter: "code".to_string(),
            namespace: None,
            include_dotfiles: false,
            dotfile_patterns: Vec::new(),
            project_root: None,
        },
    )
    .expect("ingest");
}

const ALPHA: &str = r#"//! Alpha: the transplant SOURCE file.

/// Doc comment that must TRAVEL with the item (trivia-ownership law).
pub fn move_me(x: u32) -> u32 {
    x + 1
}

pub fn stay_here(x: u32) -> u32 {
    x + 2
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

fn seed_canonical(state: &mut SessionState, root: &Path) {
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
    ingest(state, root);
}

fn preview_params(root: &Path) -> serde_json::Value {
    serde_json::json!({
        "agent_id": "race",
        "symbol": "move_me",
        "source_file": root.join("src/alpha.rs").to_string_lossy(),
        "dest_file": root.join("src/beta.rs").to_string_lossy(),
    })
}

#[test]
fn a5_graph_reingest_between_preview_and_commit_is_harmless() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);

    // 1. Stage the plan: preview captures the computed contents + per-file base hash.
    let preview = dispatch_tool(&mut state, "transplant_preview", &preview_params(root))
        .expect("preview stages the plan");
    let pid = preview
        .get("preview_id")
        .and_then(|v| v.as_str())
        .expect("preview_id")
        .to_string();

    // 2. A background re-ingest lands BETWEEN the phases — the exact graph effect an
    //    auto-ingest tick produces (every node re-created under a new id). The .rs
    //    files on DISK are untouched (a re-ingest reads source; it never writes it).
    ingest(&mut state, root);

    // 3. Commit still succeeds and lands the move: the staged plan is disk-anchored,
    //    not graph-anchored, so the graph churn could not invalidate it.
    let commit = dispatch_tool(
        &mut state,
        "transplant_commit",
        &serde_json::json!({ "agent_id": "race", "preview_id": pid, "confirm": true }),
    )
    .expect("commit lands despite the concurrent graph re-ingest");
    assert!(
        commit.get("receipt").is_some(),
        "commit returns the finalized receipt"
    );

    let alpha = std::fs::read_to_string(root.join("src/alpha.rs")).unwrap();
    let beta = std::fs::read_to_string(root.join("src/beta.rs")).unwrap();
    let gamma = std::fs::read_to_string(root.join("src/gamma.rs")).unwrap();
    assert!(
        beta.contains("fn move_me"),
        "the symbol reached the dest:\n{beta}"
    );
    assert!(
        !alpha.contains("fn move_me"),
        "the source lost the symbol:\n{alpha}"
    );
    assert!(
        gamma.contains("beta::move_me") || gamma.contains("crate::beta"),
        "the referencer was re-pointed by the staged plan:\n{gamma}"
    );
}

#[test]
fn a5_reingest_after_preview_does_not_drop_the_staged_handle() {
    // A re-ingest touches the graph and its own runtime artifacts, never the staged
    // transplant handle. The handle must remain redeemable after arbitrary graph work.
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);

    let preview =
        dispatch_tool(&mut state, "transplant_preview", &preview_params(root)).expect("preview");
    let pid = preview["preview_id"].as_str().unwrap().to_string();

    // Two more re-ingests (repeated background churn) — the handle survives all of it.
    ingest(&mut state, root);
    ingest(&mut state, root);

    let commit = dispatch_tool(
        &mut state,
        "transplant_commit",
        &serde_json::json!({ "agent_id": "race", "preview_id": pid, "confirm": true }),
    )
    .expect("the staged handle is still redeemable after repeated re-ingests");
    assert!(commit.get("receipt").is_some());
}
