// === A2: two-phase transplant — transplant_preview → transplant_commit ===
//
// The 7 mandatory cases of the house two-phase contract (tests/test_edit_preview.rs)
// TRANSPOSED to the transplant verb, per the ratified PRD §4.2/§5.A2:
//   1. preview_happy        — preview computes EVERYTHING (per-file plan + candidate
//                             receipt), returns a TTL'd preview_id, writes NOTHING
//   2. preview_missing_dest — transplant's honest transposition of "nonexistent
//                             file": a missing dest is a REFUSAL (creation is out of
//                             scope), nothing staged, nothing written
//   3. commit_happy         — commit(confirm=true) applies the staged plan
//                             atomically; the handle is consumed
//   4. commit_ttl_expired   — a preview older than 5min is gone; the error teaches
//   5. commit_stale         — a TOUCHED file (the DERIVED referencer — the caller
//                             never named it) changed after preview → refusal names
//                             the file, ZERO writes (the TOCTOU half of A5)
//   6. commit_confirm_false — confirm must be explicit; the refusal teaches and
//                             does NOT consume the handle
//   7. commit_bogus_id      — unknown preview_id; the error teaches recovery

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use std::path::Path;

// ---------------------------------------------------------------------------
// Shared infra (mirrors tests/transplant_battery.rs)
// ---------------------------------------------------------------------------

mod common;

fn make_state(root: &Path) -> SessionState {
    common::disable_proof_gate_for_logic_tests();
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
            agent_id: "two-phase".to_string(),
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
        "agent_id": "two-phase",
        "symbol": "move_me",
        "source_file": root.join("src/alpha.rs").to_string_lossy(),
        "dest_file": root.join("src/beta.rs").to_string_lossy(),
    })
}

fn commit_params(preview_id: &str, confirm: bool) -> serde_json::Value {
    serde_json::json!({
        "agent_id": "two-phase",
        "preview_id": preview_id,
        "confirm": confirm,
    })
}

fn snapshot(root: &Path) -> Vec<(String, String)> {
    ["src/alpha.rs", "src/beta.rs", "src/gamma.rs", "src/lib.rs"]
        .iter()
        .map(|rel| {
            (
                rel.to_string(),
                std::fs::read_to_string(root.join(rel)).unwrap(),
            )
        })
        .collect()
}

// ===========================================================================
// 1. preview happy path — computes everything, writes NOTHING
// ===========================================================================

#[test]
fn two_phase_preview_happy_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);
    let before = snapshot(root);

    let out = dispatch_tool(&mut state, "transplant_preview", &preview_params(root))
        .expect("transplant_preview must exist and succeed on the canonical scenario");

    // A TTL'd handle the commit can redeem.
    let pid = out
        .get("preview_id")
        .and_then(|v| v.as_str())
        .expect("preview returns a preview_id");
    assert!(
        pid.starts_with("transplant_preview_"),
        "preview_id names its verb: {pid}"
    );

    // The full per-file plan: source + dest + the DERIVED referencer, each with a
    // base hash (the TOCTOU anchor) and a diff summary.
    let files = out
        .get("files")
        .and_then(|v| v.as_array())
        .expect("preview lists the planned files");
    assert_eq!(
        files.len(),
        3,
        "source + dest + derived referencer: {files:?}"
    );
    let paths: Vec<&str> = files
        .iter()
        .filter_map(|f| f.get("file_path").and_then(|v| v.as_str()))
        .collect();
    assert!(
        paths.iter().any(|p| p.ends_with("gamma.rs")),
        "the DERIVED referencer is part of the plan: {paths:?}"
    );
    for f in files {
        let hash = f.get("base_hash").and_then(|v| v.as_str()).unwrap_or("");
        assert!(
            !hash.is_empty(),
            "each planned file carries a base hash: {f}"
        );
        assert!(
            f.get("lines_added").is_some() && f.get("lines_removed").is_some(),
            "each planned file carries a diff summary: {f}"
        );
    }

    // The candidate receipt — the same honest shape the commit will finalize.
    let candidate = out
        .get("candidate")
        .expect("preview carries the candidate receipt");
    assert_eq!(
        candidate.get("moved_symbol").and_then(|v| v.as_str()),
        Some("move_me")
    );
    assert_eq!(
        candidate
            .get("files_changed")
            .and_then(|v| v.as_array())
            .map(|a| a.len()),
        Some(3)
    );

    // THE LAW: a preview writes NOTHING.
    assert_eq!(
        snapshot(root),
        before,
        "preview must not touch a single byte"
    );
}

// ===========================================================================
// 2. preview of a missing dest — honest refusal, nothing staged
// ===========================================================================

#[test]
fn two_phase_preview_missing_dest_is_refused_and_stages_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);
    let before = snapshot(root);

    let params = serde_json::json!({
        "agent_id": "two-phase",
        "symbol": "move_me",
        "source_file": root.join("src/alpha.rs").to_string_lossy(),
        "dest_file": root.join("src/ghost.rs").to_string_lossy(),
    });
    let err = dispatch_tool(&mut state, "transplant_preview", &params)
        .expect_err("a missing dest must refuse at preview time too");
    let msg = format!("{err:?}").to_lowercase();
    assert!(
        msg.contains("does not exist") || msg.contains("cannot be read"),
        "the preview refusal teaches like the one-shot verb: {msg}"
    );

    assert!(
        state.transplant_previews.is_empty(),
        "a refused preview must stage nothing"
    );
    assert_eq!(snapshot(root), before);
    assert!(!root.join("src/ghost.rs").exists());
}

// ===========================================================================
// 3. commit happy path — the staged plan lands atomically, handle consumed
// ===========================================================================

#[test]
fn two_phase_commit_happy_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);

    let preview = dispatch_tool(&mut state, "transplant_preview", &preview_params(root))
        .expect("preview succeeds");
    let pid = preview["preview_id"].as_str().unwrap().to_string();

    let out = dispatch_tool(&mut state, "transplant_commit", &commit_params(&pid, true))
        .expect("commit with confirm=true succeeds");
    assert_eq!(
        out.get("preview_id").and_then(|v| v.as_str()),
        Some(pid.as_str())
    );
    let receipt = out
        .get("receipt")
        .expect("commit returns the finalized receipt");
    assert_eq!(
        receipt.get("moved_symbol").and_then(|v| v.as_str()),
        Some("move_me")
    );

    // The move actually landed.
    let alpha = std::fs::read_to_string(root.join("src/alpha.rs")).unwrap();
    let beta = std::fs::read_to_string(root.join("src/beta.rs")).unwrap();
    let gamma = std::fs::read_to_string(root.join("src/gamma.rs")).unwrap();
    assert!(
        !alpha.contains("fn move_me"),
        "source lost the item:\n{alpha}"
    );
    assert!(beta.contains("fn move_me"), "dest gained the item:\n{beta}");
    assert!(
        gamma.contains("beta::move_me") && !gamma.contains("alpha::move_me"),
        "the derived referencer re-pointed:\n{gamma}"
    );

    // Handle consumed — the second commit must fail precisely.
    assert!(!state.transplant_previews.contains_key(&pid));
    let err = dispatch_tool(&mut state, "transplant_commit", &commit_params(&pid, true))
        .expect_err("a consumed handle cannot commit twice");
    assert!(format!("{err:?}").contains("not found"));
}

// ===========================================================================
// 4. TTL expired — the handle dies at 5 minutes and the error teaches
// ===========================================================================

#[test]
fn two_phase_commit_ttl_expired() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);
    let before = snapshot(root);

    let preview = dispatch_tool(&mut state, "transplant_preview", &preview_params(root))
        .expect("preview succeeds");
    let pid = preview["preview_id"].as_str().unwrap().to_string();

    // Backdate the staged preview by 6 minutes (mirrors test_commit_handle_expired).
    if let Some(entry) = state.transplant_previews.get_mut(&pid) {
        entry.created_at_ms = entry.created_at_ms.saturating_sub(6 * 60 * 1000);
    }

    let err = dispatch_tool(&mut state, "transplant_commit", &commit_params(&pid, true))
        .expect_err("an expired preview must be rejected");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("not found") || msg.contains("expired"),
        "error should mention not found/expired: {msg}"
    );
    assert!(
        msg.contains("Hint:") && msg.contains("transplant_preview"),
        "error should teach recovery via transplant_preview: {msg}"
    );
    assert_eq!(snapshot(root), before, "an expired commit writes nothing");
}

// ===========================================================================
// 5. stale — a TOUCHED file drifted after preview (the TOCTOU half of A5)
// ===========================================================================

#[test]
fn two_phase_commit_refuses_when_a_derived_referencer_drifted() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);

    let preview = dispatch_tool(&mut state, "transplant_preview", &preview_params(root))
        .expect("preview succeeds");
    let pid = preview["preview_id"].as_str().unwrap().to_string();

    // Drift the DERIVED referencer — a file the caller never named. Blind-applying
    // the staged plan would clobber this edit; the hash check must catch it.
    let tampered = format!("{GAMMA}\npub fn late_arrival() -> u32 {{\n    7\n}}\n");
    write(&root.join("src/gamma.rs"), &tampered);
    let before = snapshot(root);

    let err = dispatch_tool(&mut state, "transplant_commit", &commit_params(&pid, true))
        .expect_err("a drifted touched file must refuse the commit");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("source_modified"),
        "error names the stale class: {msg}"
    );
    assert!(
        msg.to_lowercase().contains("gamma"),
        "error names the drifted file: {msg}"
    );
    assert!(
        msg.contains("Hint:") && msg.contains("transplant_preview"),
        "error teaches re-preview: {msg}"
    );

    // ZERO writes: alpha/beta untouched, gamma still exactly the tampered text.
    assert_eq!(snapshot(root), before, "a stale refusal must write nothing");
}

// ===========================================================================
// 6. confirm=false — explicit confirmation is the contract; handle survives
// ===========================================================================

#[test]
fn two_phase_commit_confirm_false_is_refused_and_keeps_the_handle() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);

    let preview = dispatch_tool(&mut state, "transplant_preview", &preview_params(root))
        .expect("preview succeeds");
    let pid = preview["preview_id"].as_str().unwrap().to_string();
    let before = snapshot(root);

    let err = dispatch_tool(&mut state, "transplant_commit", &commit_params(&pid, false))
        .expect_err("confirm=false must be refused");
    let msg = format!("{err:?}");
    assert!(msg.contains("confirm"), "error mentions confirm: {msg}");
    assert!(
        msg.contains("Hint:"),
        "error teaches how to retry with confirm=true: {msg}"
    );
    assert_eq!(snapshot(root), before, "a refused confirm writes nothing");

    // The refusal must NOT consume the handle: the corrected retry lands.
    dispatch_tool(&mut state, "transplant_commit", &commit_params(&pid, true))
        .expect("the same preview_id commits once confirm is explicit");
    let beta = std::fs::read_to_string(root.join("src/beta.rs")).unwrap();
    assert!(beta.contains("fn move_me"), "the retry landed:\n{beta}");
}

// ===========================================================================
// 7. bogus preview_id — precise not-found that teaches recovery
// ===========================================================================

#[test]
fn two_phase_commit_bogus_id_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);
    let before = snapshot(root);

    let err = dispatch_tool(
        &mut state,
        "transplant_commit",
        &commit_params("transplant_preview_bogus_0000", true),
    )
    .expect_err("a bogus preview_id must fail");
    let msg = format!("{err:?}");
    assert!(msg.contains("not found"), "error mentions not found: {msg}");
    assert!(
        msg.contains("Hint:") && msg.contains("transplant_preview"),
        "error teaches minting a fresh preview: {msg}"
    );
    assert_eq!(snapshot(root), before);
}
