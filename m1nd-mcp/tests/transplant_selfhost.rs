// === A5 — the self-hosting oracle (the supreme test) ===
//
// Ingest a REAL crate from this very worktree (`m1nd-core`), transplant a real
// private fn between two of its modules, and prove the crate STILL COMPILES with
// `cargo check -p m1nd-core`. This is the strongest possible evidence that the
// graph-addressed move produces valid Rust on production code, not just fixtures.
//
// The chosen fn is `tremor::linear_regression_slope` — private, pure (no imports
// to carry), a single same-module caller (tremor.rs:326). Moving it out forces
// the back-import path AND the moved-symbol visibility bump (a private fn that is
// back-imported must become at least `pub(crate)` or the source stops compiling).
//
// SAFETY: this test MUTATES two real worktree files. A Drop guard restores their
// exact original bytes even on panic; the test also restores explicitly and then
// VERIFIES restoration before asserting the compile result.

use m1nd_core::domain::DomainConfig;
use m1nd_core::graph::Graph;
use m1nd_mcp::server::{dispatch_tool, McpConfig};
use m1nd_mcp::session::SessionState;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// Restores the exact original bytes of a set of files on Drop (panic-safe).
struct RestoreOnDrop {
    files: Vec<(PathBuf, String)>,
}
impl Drop for RestoreOnDrop {
    fn drop(&mut self) {
        for (p, original) in &self.files {
            let _ = std::fs::write(p, original);
        }
    }
}

fn worktree_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <wt>/m1nd-mcp ; its parent is the worktree root.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("m1nd-mcp has a parent")
        .to_path_buf()
}

fn ingest_core(state: &mut SessionState, core: &Path) {
    m1nd_mcp::tools::handle_ingest(
        state,
        m1nd_mcp::protocol::IngestInput {
            path: core.to_string_lossy().to_string(),
            agent_id: "selfhost".to_string(),
            mode: "merge".to_string(),
            incremental: false,
            adapter: "code".to_string(),
            namespace: None,
            include_dotfiles: false,
            dotfile_patterns: Vec::new(),
            project_root: None,
        },
    )
    .expect("ingest m1nd-core");
}

/// `cargo check -p m1nd-core` against the worktree, reusing the shared build cache
/// (CARGO_TARGET_DIR from the environment) so the check is incremental/fast.
fn cargo_check_core(wt: &Path) -> Result<(), String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut cmd = Command::new(cargo);
    cmd.args(["check", "-p", "m1nd-core", "--quiet", "--manifest-path"])
        .arg(wt.join("Cargo.toml"));
    // Reuse the shared target dir when the harness set one; otherwise let cargo use
    // the worktree default.
    if let Ok(target) = std::env::var("CARGO_TARGET_DIR") {
        cmd.env("CARGO_TARGET_DIR", target);
    }
    match cmd.output() {
        Err(e) => Err(format!("cargo-unavailable: {e}")),
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).to_string()),
    }
}

#[test]
fn selfhost_transplant_real_private_fn_keeps_m1nd_core_compiling() {
    let wt = worktree_root();
    let core = wt.join("m1nd-core");
    let tremor = core.join("src/tremor.rs");
    let resonance = core.join("src/resonance.rs");

    // Movability precheck: the fn must be where we expect and NOT already in dest.
    let orig_tremor = std::fs::read_to_string(&tremor).expect("read tremor.rs");
    let orig_resonance = std::fs::read_to_string(&resonance).expect("read resonance.rs");
    assert!(
        orig_tremor.contains("fn linear_regression_slope"),
        "precondition: tremor.rs must define linear_regression_slope (worktree dirty?)"
    );
    assert!(
        !orig_resonance.contains("fn linear_regression_slope"),
        "precondition: resonance.rs must NOT already define it (worktree dirty?)"
    );

    // Arm the panic-safe restore BEFORE the first mutation.
    let _guard = RestoreOnDrop {
        files: vec![
            (tremor.clone(), orig_tremor.clone()),
            (resonance.clone(), orig_resonance.clone()),
        ],
    };

    // Ingest the real crate. Graph artifacts go to a TEMPDIR — never into m1nd-core.
    let tmp = tempfile::tempdir().unwrap();
    let config = McpConfig {
        graph_source: tmp.path().join("graph_snapshot.json"),
        plasticity_state: tmp.path().join("plasticity_state.json"),
        ..McpConfig::default()
    };
    let mut state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
        .expect("init session");
    state.ingest_roots = vec![core.to_string_lossy().to_string()];

    let t_ingest = Instant::now();
    ingest_core(&mut state, &core);
    eprintln!(
        "SELFHOST ingest_ms = {:.0}",
        t_ingest.elapsed().as_secs_f64() * 1000.0
    );

    // The real move.
    let t_move = Instant::now();
    let out = dispatch_tool(
        &mut state,
        "transplant",
        &serde_json::json!({
            "agent_id": "selfhost",
            "symbol": "linear_regression_slope",
            "source_file": tremor.to_string_lossy(),
            "dest_file": resonance.to_string_lossy(),
        }),
    );
    eprintln!(
        "SELFHOST transplant_ms = {:.2}",
        t_move.elapsed().as_secs_f64() * 1000.0
    );

    // Run the compiler oracle on the MUTATED tree, capture the verdict, THEN restore.
    let (move_ok, check) = match out {
        Ok(_) => {
            let t_check = Instant::now();
            let c = cargo_check_core(&wt);
            eprintln!(
                "SELFHOST cargo_check_ms = {:.0}",
                t_check.elapsed().as_secs_f64() * 1000.0
            );
            (true, c)
        }
        Err(e) => (false, Err(format!("transplant refused: {e:?}"))),
    };

    // Explicit restoration + verification (the Drop guard is only a backstop).
    std::fs::write(&tremor, &orig_tremor).unwrap();
    std::fs::write(&resonance, &orig_resonance).unwrap();
    assert_eq!(
        std::fs::read_to_string(&tremor).unwrap(),
        orig_tremor,
        "tremor.rs must be restored byte-for-byte"
    );
    assert_eq!(
        std::fs::read_to_string(&resonance).unwrap(),
        orig_resonance,
        "resonance.rs must be restored byte-for-byte"
    );

    // Now judge the oracle.
    assert!(move_ok, "the real transplant must succeed: {check:?}");
    match check {
        Ok(()) => {}
        Err(e) if e.starts_with("cargo-unavailable") => {
            eprintln!("SKIP selfhost oracle: {e}");
        }
        Err(e) => panic!("m1nd-core must still COMPILE after moving a real private fn:\n{e}"),
    }
}
