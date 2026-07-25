// === Stress battery + compiler oracle for the `transplant` verb ===
//
// The canonical contract lives in tests/transplant_battery.rs. This file adds the
// adversarial cases the PRD needs evidence for (precise refusals, self-use,
// qualified-path rewrites, multiple referencers, idempotence, travelling
// attributes) plus a COMPILER ORACLE: after the canonical transplant, the mutated
// fixture crate must `cargo check` clean — the strongest possible proof that a
// graph-addressed move produced VALID Rust, not just text that matches asserts.

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use std::path::Path;
use std::process::Command;

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
    let out = m1nd_mcp::tools::handle_ingest(
        state,
        m1nd_mcp::protocol::IngestInput {
            path: root.to_string_lossy().to_string(),
            agent_id: "stress".to_string(),
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
    let nodes = out.get("node_count").and_then(|v| v.as_u64()).unwrap_or(0);
    assert!(nodes >= 3, "fixture graph must populate, got {nodes}");
}

fn cargo_toml() -> &'static str {
    "[package]\nname = \"fixture-transplant\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
}

fn params(root: &Path, symbol: &str, src: &str, dest: &str) -> serde_json::Value {
    serde_json::json!({
        "agent_id": "stress",
        "symbol": symbol,
        "source_file": root.join(src).to_string_lossy(),
        "dest_file": root.join(dest).to_string_lossy(),
    })
}

/// Run `cargo check` on the fixture crate with a target dir INSIDE the tempdir
/// (no lock contention with the outer test's target). Returns Ok(()) on a clean
/// compile, Err(stderr) on failure, or Err("cargo-unavailable") when cargo cannot
/// be spawned in this sandbox (the caller then skips honestly).
fn cargo_check(root: &Path) -> Result<(), String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let manifest = root.join("Cargo.toml");
    let target = root.join("_check_target");
    let out = Command::new(cargo)
        .args(["check", "--quiet", "--manifest-path"])
        .arg(&manifest)
        .env("CARGO_TARGET_DIR", &target)
        .output();
    match out {
        Err(e) => Err(format!("cargo-unavailable: {e}")),
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
    }
}

// ---------------------------------------------------------------------------
// Canonical fixture (same shape as the battery)
// ---------------------------------------------------------------------------

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

fn seed_canonical(state: &mut SessionState, root: &Path) {
    write(&root.join("Cargo.toml"), cargo_toml());
    write(
        &root.join("src/lib.rs"),
        "pub mod alpha;\npub mod beta;\npub mod gamma;\n",
    );
    write(&root.join("src/alpha.rs"), ALPHA);
    write(&root.join("src/beta.rs"), BETA);
    write(&root.join("src/gamma.rs"), GAMMA);
    ingest(state, root);
}

// ===========================================================================
// GATE 2 — the compiler oracle
// ===========================================================================

#[test]
fn oracle_canonical_transplant_produces_a_compiling_crate() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);

    // Sanity: the fixture compiles BEFORE the move (guards a broken fixture).
    match cargo_check(root) {
        Ok(()) => {}
        Err(e) if e.starts_with("cargo-unavailable") => {
            eprintln!("SKIP oracle: {e}");
            return;
        }
        Err(e) => panic!("fixture must compile before the transplant:\n{e}"),
    }

    let t0 = std::time::Instant::now();
    dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("canonical transplant succeeds");
    eprintln!(
        "ORACLE transplant elapsed_ms = {:.2}",
        t0.elapsed().as_secs_f64() * 1000.0
    );

    // THE ORACLE: the mutated crate must still compile.
    cargo_check(root).expect("crate must COMPILE after the graph-addressed transplant");
}

// ===========================================================================
// (a) symbol not found -> precise error, zero writes
// ===========================================================================

#[test]
fn stress_symbol_not_found_is_precise_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);

    let err = dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "ghost_symbol", "src/alpha.rs", "src/beta.rs"),
    )
    .expect_err("a missing symbol must be refused");
    let msg = format!("{err:?}").to_lowercase();
    assert!(
        msg.contains("not found"),
        "must be a precise not-found error: {msg}"
    );
    // The refusal should list the source's real symbols to help the caller.
    assert!(
        msg.contains("move_me"),
        "should list the file's symbols: {msg}"
    );

    assert_eq!(
        std::fs::read_to_string(root.join("src/alpha.rs")).unwrap(),
        ALPHA
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/beta.rs")).unwrap(),
        BETA
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/gamma.rs")).unwrap(),
        GAMMA
    );
}

// ===========================================================================
// (b) symbol exists only in a DIFFERENT file -> error naming where it lives
// ===========================================================================

#[test]
fn stress_symbol_in_different_file_names_where_it_lives() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);

    // existing_resident lives in beta.rs, not alpha.rs.
    let err = dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "existing_resident", "src/alpha.rs", "src/gamma.rs"),
    )
    .expect_err("a symbol defined elsewhere must be refused");
    let msg = format!("{err:?}").to_lowercase();
    assert!(
        msg.contains("beta.rs"),
        "must name where the symbol lives: {msg}"
    );

    assert_eq!(
        std::fs::read_to_string(root.join("src/alpha.rs")).unwrap(),
        ALPHA
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/beta.rs")).unwrap(),
        BETA
    );
}

// ===========================================================================
// (c) dest_file == source_file -> refused
// ===========================================================================

#[test]
fn stress_same_source_and_dest_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);

    let err = dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/alpha.rs"),
    )
    .expect_err("same source and dest must be refused");
    let msg = format!("{err:?}").to_lowercase();
    assert!(
        msg.contains("same file"),
        "must refuse a same-file move: {msg}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/alpha.rs")).unwrap(),
        ALPHA
    );
}

// ===========================================================================
// (d) dest_file does not exist -> refuse honestly (documented PRD decision)
// ===========================================================================

#[test]
fn stress_missing_dest_file_is_refused_honestly() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);

    let err = dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/does_not_exist.rs"),
    )
    .expect_err("a non-existent destination must be refused (PRD §7.3)");
    let msg = format!("{err:?}").to_lowercase();
    assert!(
        msg.contains("does not exist") || msg.contains("cannot be read"),
        "the refusal must be honest about the missing dest: {msg}"
    );
    // Source untouched; the new file must NOT have been created.
    assert_eq!(
        std::fs::read_to_string(root.join("src/alpha.rs")).unwrap(),
        ALPHA
    );
    assert!(!root.join("src/does_not_exist.rs").exists());
}

// ===========================================================================
// (e) self-use: the SOURCE file itself calls the moved fn -> back-import
// ===========================================================================

const ALPHA_SELFUSE: &str = r#"//! Alpha: source that keeps calling the moved fn.

pub fn move_me(x: u32) -> u32 {
    x + 1
}

pub fn source_caller(x: u32) -> u32 {
    move_me(x) + 100
}
"#;

#[test]
fn stress_self_use_back_imports_and_source_compiles() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(&root.join("src/lib.rs"), "pub mod alpha;\npub mod beta;\n");
    write(&root.join("src/alpha.rs"), ALPHA_SELFUSE);
    write(&root.join("src/beta.rs"), BETA);
    ingest(&mut state, root);

    let out = dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("self-use transplant succeeds");
    assert_eq!(
        out.get("source_back_imported").and_then(|v| v.as_bool()),
        Some(true)
    );

    let alpha = std::fs::read_to_string(root.join("src/alpha.rs")).unwrap();
    assert!(
        !alpha.contains("fn move_me"),
        "the item left the source:\n{alpha}"
    );
    assert!(
        alpha.contains("use crate::beta::move_me"),
        "the source back-imports the moved fn so its own caller still resolves:\n{alpha}"
    );

    // The strongest proof: the mutated source actually compiles.
    match cargo_check(root) {
        Ok(()) => {}
        Err(e) if e.starts_with("cargo-unavailable") => eprintln!("SKIP self-use compile: {e}"),
        Err(e) => panic!("self-use source must compile after the move:\n{e}"),
    }
}

// ===========================================================================
// (f) referencer with a QUALIFIED path (no `use`) -> rewritten
// ===========================================================================

const GAMMA_QUALIFIED: &str = r#"//! Gamma: references the symbol by a fully-qualified path, no `use`.

pub fn call_it() -> u32 {
    crate::alpha::move_me(21)
}
"#;

#[test]
fn stress_qualified_path_referencer_is_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(
        &root.join("src/lib.rs"),
        "pub mod alpha;\npub mod beta;\npub mod gamma;\n",
    );
    write(&root.join("src/alpha.rs"), ALPHA);
    write(&root.join("src/beta.rs"), BETA);
    write(&root.join("src/gamma.rs"), GAMMA_QUALIFIED);
    ingest(&mut state, root);

    let out = dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("transplant with a qualified-path referencer succeeds");
    // Report which layer found the referencer (PRD data): graph vs textual.
    eprintln!(
        "QUALIFIED referencer_source = {}",
        out.get("referencer_source")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
    );

    let gamma = std::fs::read_to_string(root.join("src/gamma.rs")).unwrap();
    assert!(
        gamma.contains("crate::beta::move_me"),
        "qualified path re-pointed:\n{gamma}"
    );
    assert!(
        !gamma.contains("alpha::move_me"),
        "old qualified path gone:\n{gamma}"
    );
    match cargo_check(root) {
        Ok(()) => {}
        Err(e) if e.starts_with("cargo-unavailable") => eprintln!("SKIP qualified compile: {e}"),
        Err(e) => panic!("qualified-path crate must compile after the move:\n{e}"),
    }
}

// ===========================================================================
// (g) multiple referencing files -> all rewritten
// ===========================================================================

#[test]
fn stress_multiple_referencers_all_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(
        &root.join("src/lib.rs"),
        "pub mod alpha;\npub mod beta;\npub mod refa;\npub mod refb;\n",
    );
    write(&root.join("src/alpha.rs"), ALPHA);
    write(&root.join("src/beta.rs"), BETA);
    write(
        &root.join("src/refa.rs"),
        "//! Ref A.\n\nuse crate::alpha::move_me;\n\npub fn a() -> u32 {\n    move_me(1)\n}\n",
    );
    write(
        &root.join("src/refb.rs"),
        "//! Ref B.\n\npub fn b() -> u32 {\n    crate::alpha::move_me(2)\n}\n",
    );
    ingest(&mut state, root);

    let out = dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("multi-referencer transplant succeeds");
    let files = out
        .get("referencing_files")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    assert!(
        files >= 2,
        "both referencing files must be discovered, got {files}"
    );

    let refa = std::fs::read_to_string(root.join("src/refa.rs")).unwrap();
    let refb = std::fs::read_to_string(root.join("src/refb.rs")).unwrap();
    assert!(
        refa.contains("beta::move_me") && !refa.contains("alpha::move_me"),
        "refa:\n{refa}"
    );
    assert!(
        refb.contains("beta::move_me") && !refb.contains("alpha::move_me"),
        "refb:\n{refb}"
    );
    match cargo_check(root) {
        Ok(()) => {}
        Err(e) if e.starts_with("cargo-unavailable") => eprintln!("SKIP multi compile: {e}"),
        Err(e) => panic!("multi-referencer crate must compile after the move:\n{e}"),
    }
}

// ===========================================================================
// (h) idempotence: the same transplant twice -> second is a precise error
// ===========================================================================

#[test]
fn stress_idempotence_second_transplant_is_a_precise_error() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    seed_canonical(&mut state, root);

    dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("first transplant succeeds");

    let alpha_after = std::fs::read_to_string(root.join("src/alpha.rs")).unwrap();
    let beta_after = std::fs::read_to_string(root.join("src/beta.rs")).unwrap();

    let err = dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect_err("the second transplant must be a precise error, not a re-move");
    let msg = format!("{err:?}").to_lowercase();
    assert!(
        msg.contains("beta.rs") || msg.contains("not in source") || msg.contains("already"),
        "the second attempt must explain the symbol already moved: {msg}"
    );
    // Zero writes on the second (failed) attempt.
    assert_eq!(
        std::fs::read_to_string(root.join("src/alpha.rs")).unwrap(),
        alpha_after
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/beta.rs")).unwrap(),
        beta_after
    );
}

// ===========================================================================
// (i) travelling attributes: #[inline] + multi-line doc travel with the item
// ===========================================================================

const ALPHA_ATTR: &str = r#"//! Alpha: attributed item.

/// Doc line one of the moved item.
/// Doc line two of the moved item.
#[inline]
pub fn move_me(x: u32) -> u32 {
    x + 1
}

pub fn stay_here() -> u32 {
    0
}
"#;

#[test]
fn stress_attributes_and_multiline_doc_travel() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(&root.join("src/lib.rs"), "pub mod alpha;\npub mod beta;\n");
    write(&root.join("src/alpha.rs"), ALPHA_ATTR);
    write(&root.join("src/beta.rs"), BETA);
    ingest(&mut state, root);

    dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("attributed transplant succeeds");

    let alpha = std::fs::read_to_string(root.join("src/alpha.rs")).unwrap();
    let beta = std::fs::read_to_string(root.join("src/beta.rs")).unwrap();
    assert!(beta.contains("#[inline]"), "the attribute travels:\n{beta}");
    assert!(
        beta.contains("/// Doc line one"),
        "first doc line travels:\n{beta}"
    );
    assert!(
        beta.contains("/// Doc line two"),
        "second doc line travels:\n{beta}"
    );
    assert!(
        !alpha.contains("#[inline]"),
        "the attribute left the source:\n{alpha}"
    );
    match cargo_check(root) {
        Ok(()) => {}
        Err(e) if e.starts_with("cargo-unavailable") => eprintln!("SKIP attr compile: {e}"),
        Err(e) => panic!("attributed crate must compile after the move:\n{e}"),
    }
}

// ===========================================================================
// (j) A7 — IMPOSED boundary: poisonous module stems (lib/main/mod)
// The module name is the file stem; lib/main/mod would make the verb synthesize
// an invalid `crate::<stem>::…` path (the crate root is NOT a module named `lib`).
// Today the move proceeds and produces a success receipt over a broken build —
// the "ideal-falso". A7 makes this state UNREACHABLE: refuse, teach, write nothing.
// ===========================================================================

#[test]
fn stress_a7_poisonous_module_stem_is_refused_and_teaches() {
    for stem in ["lib", "main", "mod"] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut state = make_state(root);
        write(&root.join("Cargo.toml"), cargo_toml());
        // A clean crate wiring, plus a source file whose stem is poisonous.
        write(&root.join("src/lib.rs"), "pub mod beta;\n");
        let src_rel = format!("src/{stem}.rs");
        write(
            &root.join(&src_rel),
            "//! poisonous-stem source.\n\npub fn move_me(x: u32) -> u32 {\n    x + 1\n}\n",
        );
        write(&root.join("src/beta.rs"), BETA);
        ingest(&mut state, root);

        let before_src = std::fs::read_to_string(root.join(&src_rel)).unwrap();
        let before_dst = std::fs::read_to_string(root.join("src/beta.rs")).unwrap();

        let err = dispatch_tool(
            &mut state,
            "transplant",
            &params(root, "move_me", &src_rel, "src/beta.rs"),
        )
        .expect_err("a poisonous module stem must be refused (A7)");
        let msg = format!("{err:?}").to_lowercase();
        // The error must TEACH: name the invalid module path it would have synthesized.
        assert!(
            msg.contains(&format!("crate::{stem}")),
            "the refusal must name the invalid `crate::{stem}::…` path it avoided: {msg}"
        );
        assert!(
            msg.contains("stem"),
            "the refusal must name the poisonous stem class: {msg}"
        );

        // Byte-identity: a refusal changes nothing.
        assert_eq!(
            std::fs::read_to_string(root.join(&src_rel)).unwrap(),
            before_src,
            "source must be byte-identical after a stem refusal ({stem})"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("src/beta.rs")).unwrap(),
            before_dst,
            "dest must be byte-identical after a stem refusal ({stem})"
        );
    }
}

#[test]
fn stress_a7_poisonous_dest_stem_is_refused() {
    // The boundary is symmetric: a poisonous DEST stem is refused too (you cannot
    // land a symbol into `crate::mod::…`).
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(&root.join("src/lib.rs"), "pub mod alpha;\n");
    write(&root.join("src/alpha.rs"), ALPHA);
    write(
        &root.join("src/mod.rs"),
        "//! poisonous-stem destination.\n\npub fn resident(x: u32) -> u32 {\n    x\n}\n",
    );
    ingest(&mut state, root);

    let before_src = std::fs::read_to_string(root.join("src/alpha.rs")).unwrap();
    let before_dst = std::fs::read_to_string(root.join("src/mod.rs")).unwrap();

    let err = dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/mod.rs"),
    )
    .expect_err("a poisonous dest stem must be refused (A7)");
    let msg = format!("{err:?}").to_lowercase();
    assert!(
        msg.contains("crate::mod") && msg.contains("dest"),
        "the refusal must name the poisonous dest stem: {msg}"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/alpha.rs")).unwrap(),
        before_src
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/mod.rs")).unwrap(),
        before_dst
    );
}

// ===========================================================================
// (k) A7 — IMPOSED boundary: cross-crate moves.
// source and dest must share the same crate root (nearest ancestor with a
// Cargo.toml). A cross-crate move dangles `crate::…` paths — a broken build under
// a success receipt today. A7 refuses, naming BOTH crate roots, and writes nothing.
// ===========================================================================

#[test]
fn stress_a7_cross_crate_move_is_refused_and_teaches() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    // crate A
    write(
        &root.join("crate_a/Cargo.toml"),
        "[package]\nname = \"crate-a\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(&root.join("crate_a/src/lib.rs"), "pub mod alpha;\n");
    write(
        &root.join("crate_a/src/alpha.rs"),
        "//! crate A source.\n\npub fn move_me(x: u32) -> u32 {\n    x + 1\n}\n",
    );
    // crate B
    write(
        &root.join("crate_b/Cargo.toml"),
        "[package]\nname = \"crate-b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    );
    write(&root.join("crate_b/src/lib.rs"), "pub mod beta;\n");
    write(
        &root.join("crate_b/src/beta.rs"),
        "//! crate B dest.\n\npub fn existing(x: u32) -> u32 {\n    x\n}\n",
    );
    ingest(&mut state, root);

    let call = serde_json::json!({
        "agent_id": "stress",
        "symbol": "move_me",
        "source_file": root.join("crate_a/src/alpha.rs").to_string_lossy(),
        "dest_file": root.join("crate_b/src/beta.rs").to_string_lossy(),
    });
    let before_src = std::fs::read_to_string(root.join("crate_a/src/alpha.rs")).unwrap();
    let before_dst = std::fs::read_to_string(root.join("crate_b/src/beta.rs")).unwrap();

    let err = dispatch_tool(&mut state, "transplant", &call)
        .expect_err("a cross-crate move must be refused (A7)");
    let msg = format!("{err:?}").to_lowercase();
    assert!(
        msg.contains("cross-crate") || msg.contains("crate root"),
        "the refusal must explain the crate boundary: {msg}"
    );
    // TEACH: both crate roots are named so the caller sees the two homes.
    assert!(
        msg.contains("crate_a") && msg.contains("crate_b"),
        "the refusal must name BOTH crate roots: {msg}"
    );

    assert_eq!(
        std::fs::read_to_string(root.join("crate_a/src/alpha.rs")).unwrap(),
        before_src,
        "source must be byte-identical after a cross-crate refusal"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("crate_b/src/beta.rs")).unwrap(),
        before_dst,
        "dest must be byte-identical after a cross-crate refusal"
    );
}

// ===========================================================================
// (l) A8 — full-namespace dest collision.
// The collision preflight checked only `fn <symbol>` in the dest; a homonymous
// struct/enum/trait/type/const/static/mod slips through and lands E0428 AFTER a
// success receipt. Extend the dest scan to every top-level item kind and refuse,
// naming the occupant kind. Struct homonym is the witness; byte-identity on refusal.
// ===========================================================================

#[test]
fn stress_a8_dest_struct_homonym_is_refused_naming_the_kind() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(&root.join("src/lib.rs"), "pub mod alpha;\npub mod beta;\n");
    write(&root.join("src/alpha.rs"), ALPHA);
    // beta already defines a STRUCT named move_me — a full-namespace collision the
    // fn-only preflight misses; moving `fn move_me` here is a duplicate definition.
    let beta_poisoned = "//! Beta: destination with a struct homonym.\n\npub struct move_me {\n    pub x: u32,\n}\n\npub fn existing_resident(x: u32) -> u32 {\n    x - 1\n}\n";
    write(&root.join("src/beta.rs"), beta_poisoned);
    ingest(&mut state, root);

    let before_alpha = std::fs::read_to_string(root.join("src/alpha.rs")).unwrap();
    let before_beta = std::fs::read_to_string(root.join("src/beta.rs")).unwrap();

    let err = dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect_err("a full-namespace dest collision must be refused (A8)");
    let msg = format!("{err:?}").to_lowercase();
    assert!(
        msg.contains("collision") || msg.contains("already define"),
        "must be a collision refusal: {msg}"
    );
    // The error must NAME the occupant kind so the caller knows what to move/rename.
    assert!(
        msg.contains("struct"),
        "the refusal must name the occupant kind (struct): {msg}"
    );

    // Byte-identity: a refusal changes nothing.
    assert_eq!(
        std::fs::read_to_string(root.join("src/alpha.rs")).unwrap(),
        before_alpha,
        "source must be byte-identical after an A8 refusal"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("src/beta.rs")).unwrap(),
        before_beta,
        "dest must be byte-identical after an A8 refusal"
    );
}

// ===========================================================================
// (m) §7.7 — rustfmt on touched files.
// The oracle is `cargo check`, never fmt: in a repo with a fmt gate the verb's
// assembled output could reprove CI with no warning in the receipt. After a
// successful transplant every touched file must end rustfmt-clean; when rustfmt
// is unavailable the receipt must SAY so instead of silently skipping.
// ===========================================================================

/// `rustfmt --edition 2021 --check` on one file: Ok(()) when already clean,
/// Err("rustfmt-unavailable: …") when the binary cannot be spawned (the caller
/// then skips honestly), Err(diff) when the file needs formatting.
fn rustfmt_check(path: &Path) -> Result<(), String> {
    let out = Command::new("rustfmt")
        .args(["--edition", "2021", "--check"])
        .arg(path)
        .output();
    match out {
        Err(e) => Err(format!("rustfmt-unavailable: {e}")),
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        )),
    }
}

#[test]
fn stress_fmt_touched_files_end_rustfmt_clean() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut state = make_state(root);
    write(&root.join("Cargo.toml"), cargo_toml());
    write(&root.join("src/lib.rs"), "pub mod alpha;\npub mod beta;\n");
    // The moved item carries DELIBERATELY non-rustfmt spacing — the verb's
    // assembled dest content preserves it verbatim today.
    write(
        &root.join("src/alpha.rs"),
        "//! Alpha: non-fmt source.\n\npub fn move_me(x:u32)->u32 { x+1 }\n\npub fn stay_here() -> u32 {\n    0\n}\n",
    );
    write(&root.join("src/beta.rs"), BETA);
    ingest(&mut state, root);

    let out = dispatch_tool(
        &mut state,
        "transplant",
        &params(root, "move_me", "src/alpha.rs", "src/beta.rs"),
    )
    .expect("non-fmt transplant succeeds");

    let files: Vec<String> = out
        .get("files_changed")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    assert!(!files.is_empty(), "receipt must list the touched files");
    let fmt_receipt = out
        .get("rustfmt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Honest skip: no rustfmt in this sandbox → the RECEIPT must carry the note.
    if let Err(e) = rustfmt_check(Path::new(&files[0])) {
        if e.starts_with("rustfmt-unavailable") {
            eprintln!("SKIP fmt-check (no rustfmt): {e}");
            assert!(
                fmt_receipt.contains("unavailable"),
                "without rustfmt the receipt must note it, got: {fmt_receipt:?}"
            );
            return;
        }
    }

    for f in &files {
        if let Err(diff) = rustfmt_check(Path::new(f)) {
            panic!("touched file {f} must end rustfmt-clean after the verb (§7.7):\n{diff}");
        }
    }
    assert_eq!(
        fmt_receipt, "applied",
        "receipt must report the formatting pass"
    );
    // The formatted result still compiles (fmt never trades away the real oracle).
    match cargo_check(root) {
        Ok(()) => {}
        Err(e) if e.starts_with("cargo-unavailable") => eprintln!("SKIP fmt compile: {e}"),
        Err(e) => panic!("formatted crate must compile after the move:\n{e}"),
    }
}
