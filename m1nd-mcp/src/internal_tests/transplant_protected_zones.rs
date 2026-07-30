// === Battery: A3 — protected zones (the house Money-Zone law) ===
//
// The Money-Zone doctrine, mechanized server-side: a repo-level
// `ci/protected-zones.json` (the house convention — `{"zones":[{"glob":"…",
// "reason":"…"}]}`) declares guarded globs. At transplant preflight, if the
// source, dest, or ANY derived referencer matches a zone glob, the verb REFUSES —
// naming the zone, the matched file, and the gesture — UNLESS the caller carried
// the explicit `allow_protected:"<reason>"`. With the gesture it proceeds and
// records the crossing in the receipt. No zone file → no interference.
//
// These COMPILE now and FAIL until the gate exists: without it the guarded move
// SUCCEEDS (no refusal) and the receipt carries no `protected_zone`.

use crate::server::{dispatch_tool, McpConfig};
use crate::session::SessionState;
use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use serde_json::json;
use std::path::Path;

// ===========================================================================
// Shared fixture (mirrors transplant_battery.rs)
// ===========================================================================

use crate::transplant_common_internal_tests as common;

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

    let out = crate::tools::handle_ingest(
        state,
        crate::protocol::IngestInput {
            path: root.to_string_lossy().to_string(),
            agent_id: "zones".to_string(),
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
    assert!(
        out["node_count"].as_u64().unwrap_or(0) >= 5,
        "populated graph"
    );
}

/// Write `ci/protected-zones.json` at the repo root guarding the DEST file.
fn seed_zone_file(root: &Path) {
    write(
        &root.join("ci/protected-zones.json"),
        r#"{ "zones": [ { "glob": "**/beta.rs", "reason": "the money ledger lives here" } ] }"#,
    );
}

fn base_params(root: &Path) -> serde_json::Value {
    json!({
        "agent_id": "zones",
        "symbol": "move_me",
        "source_file": root.join("src/alpha.rs").to_string_lossy(),
        "dest_file": root.join("src/beta.rs").to_string_lossy(),
    })
}

fn read_all(root: &Path) -> (String, String, String) {
    (
        std::fs::read_to_string(root.join("src/alpha.rs")).unwrap(),
        std::fs::read_to_string(root.join("src/beta.rs")).unwrap(),
        std::fs::read_to_string(root.join("src/gamma.rs")).unwrap(),
    )
}

// ===========================================================================
// A3.1 — a guarded dest refuses WITHOUT the gesture, and touches nothing.
// ===========================================================================

#[test]
fn a3_guarded_zone_refuses_without_gesture_and_touches_nothing() {
    let _proof_gate = common::proof_gate_off_lease();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_fixture(&mut state, root);
    seed_zone_file(root);

    let before = read_all(root);

    let err = dispatch_tool(&mut state, "transplant", &base_params(root))
        .expect_err("a move into a protected zone must be refused without the gesture");
    let msg = format!("{err:?}").to_lowercase();
    assert!(
        msg.contains("protected zone") && msg.contains("beta.rs"),
        "the refusal must name the zone and the matched file, got: {msg}"
    );
    assert!(
        msg.contains("allow_protected"),
        "the refusal must teach the gesture, got: {msg}"
    );
    assert!(
        msg.contains("money ledger"),
        "the refusal must carry the zone's reason, got: {msg}"
    );

    // Byte-identity: a refusal changes not one byte.
    assert_eq!(
        read_all(root),
        before,
        "a refused transplant touches nothing"
    );
}

// ===========================================================================
// A3.2 — the same call WITH the gesture proceeds and records the crossing.
// ===========================================================================

#[test]
fn a3_guarded_zone_proceeds_with_gesture_and_records_it() {
    let _proof_gate = common::proof_gate_off_lease();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_fixture(&mut state, root);
    seed_zone_file(root);

    let mut params = base_params(root);
    params["allow_protected"] = json!("moving a pure helper; the ledger math is untouched");

    let receipt = dispatch_tool(&mut state, "transplant", &params)
        .expect("with the explicit gesture the guarded move proceeds");

    // The move actually happened.
    let (alpha, beta, _gamma) = read_all(root);
    assert!(
        beta.contains("fn move_me"),
        "the symbol moved into the dest"
    );
    assert!(!alpha.contains("fn move_me"), "the source lost the symbol");

    // The crossing is recorded in the receipt — auditable.
    let pz = receipt
        .get("protected_zone")
        .filter(|v| !v.is_null())
        .expect("the receipt records the protected-zone crossing");
    assert_eq!(pz["zone"], "**/beta.rs", "the matched zone glob");
    assert_eq!(pz["zone_reason"], "the money ledger lives here");
    assert!(
        pz["matched_file"]
            .as_str()
            .unwrap_or("")
            .contains("beta.rs"),
        "the matched file is named"
    );
    assert_eq!(
        pz["gesture"], "moving a pure helper; the ledger math is untouched",
        "the caller's reason is recorded verbatim"
    );
}

// ===========================================================================
// A3.3 — no zone file: the gate does not interfere with an ordinary move.
// ===========================================================================

#[test]
fn a3_no_zone_file_does_not_interfere() {
    let _proof_gate = common::proof_gate_off_lease();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_fixture(&mut state, root);
    // No ci/protected-zones.json.

    let receipt = dispatch_tool(&mut state, "transplant", &base_params(root))
        .expect("an ordinary move with no zone config just works");
    assert!(
        receipt
            .get("protected_zone")
            .map(|v| v.is_null())
            .unwrap_or(true),
        "no zone crossed → no protected_zone gesture recorded"
    );
    let (alpha, beta, _g) = read_all(root);
    assert!(beta.contains("fn move_me") && !alpha.contains("fn move_me"));
}
