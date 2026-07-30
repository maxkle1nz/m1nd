// === B1: the proof gate must cover the DERIVED referencers, not just src+dest ===
//
// M1ND_PROOF_GATE refuses a write tool unless every file it will TOUCH was driven
// to proof_state==ready_to_edit this session. A transplant also writes the DERIVED
// referencer files — paths the caller never named. Before B1 the armed gate saw
// only source+dest, so a referencer could be rewritten with no permit. This proves:
// with source+dest proven but the referencer NOT, the transplant is refused naming
// the referencer; once every touched file is proven, it proceeds.
//
// SINGLE test in this suite ON PURPOSE: M1ND_PROOF_GATE is a process-global env
// var, so nothing else may run alongside it — the exclusive lease this test takes
// is what enforces that now the suite shares the crate's test process.

use crate::server::{dispatch_tool, McpConfig};
use crate::session::SessionState;
use crate::transplant_common_internal_tests as common;
use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
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
    crate::tools::handle_ingest(
        state,
        crate::protocol::IngestInput {
            path: root.to_string_lossy().to_string(),
            agent_id: "surgeon".to_string(),
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

fn params(root: &Path, symbol: &str, src: &str, dest: &str) -> serde_json::Value {
    serde_json::json!({
        "agent_id": "surgeon",
        "symbol": symbol,
        "source_file": root.join(src).to_string_lossy(),
        "dest_file": root.join(dest).to_string_lossy(),
    })
}

const ALPHA: &str = r#"//! Alpha: source.

pub fn move_me(x: u32) -> u32 {
    x + 1
}
"#;

const BETA: &str = r#"//! Beta: destination.

pub fn existing_resident(x: u32) -> u32 {
    x - 1
}
"#;

const GAMMA: &str = r#"//! Gamma: an external referencer — a DERIVED file the caller never names.

use crate::alpha::move_me;

pub fn call_it() -> u32 {
    move_me(21)
}
"#;

#[test]
fn b1_proof_gate_covers_derived_referencers() {
    let _armed = common::arm_proof_gate_exclusively();
    std::env::set_var("M1ND_PROOF_GATE", "1");

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
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
    ingest(&mut state, root);

    let source = root.join("src/alpha.rs").to_string_lossy().to_string();
    let dest = root.join("src/beta.rs").to_string_lossy().to_string();
    let referencer = root.join("src/gamma.rs").to_string_lossy().to_string();

    // Prove ONLY the caller-named paths (source + dest), NOT the derived referencer.
    // (The era made `note_proof_ready` fallible — binding a generation/digest/TTL
    // mark can fail; arming must succeed for the battery to be meaningful.)
    state
        .note_proof_ready("surgeon", &source, "test")
        .expect("arm source proof mark");
    state
        .note_proof_ready("surgeon", &dest, "test")
        .expect("arm dest proof mark");

    let gamma_before = std::fs::read_to_string(&referencer).unwrap();

    let err = dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect_err("the armed gate must refuse while the DERIVED referencer is unproven");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("PROOF_GATE"),
        "must be a proof-gate refusal: {msg}"
    );
    assert!(
        msg.to_lowercase().contains("gamma"),
        "the refusal must name the unproven DERIVED referencer (gamma): {msg}"
    );
    // The refusal is a preflight refusal: the referencer is byte-identical.
    assert_eq!(
        std::fs::read_to_string(&referencer).unwrap(),
        gamma_before,
        "a gate refusal must not touch the derived referencer"
    );

    // Now prove the referencer too — every touched file has a permit, so it proceeds.
    state
        .note_proof_ready("surgeon", &referencer, "test")
        .expect("arm referencer proof mark");
    dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("with every touched file proven, the transplant proceeds");

    let gamma_after = std::fs::read_to_string(&referencer).unwrap();
    assert_ne!(
        gamma_before, gamma_after,
        "the referencer was rewritten once fully proven"
    );
    assert!(
        gamma_after.contains("beta::move_me"),
        "the referencer now points at the new home:\n{gamma_after}"
    );

    std::env::remove_var("M1ND_PROOF_GATE");
}
