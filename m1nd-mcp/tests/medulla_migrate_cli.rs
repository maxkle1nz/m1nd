//! MEDULLA slice M5a — the CLI wiring for the storage-split migration.
//!
//! `medulla_migration.rs` ships `MedullaMigration::{plan, apply, rollback}` fully
//! built + battery-proven, but with ZERO callers outside its own tests (the
//! CODE-LAND-ONLY posture — the module builds and proves the migration, it never
//! ran it). This test drives the operator seam that makes it reachable:
//!
//!   m1nd-mcp --medulla-migrate plan --runtime-dir <runtime>
//!
//! `plan` is the pure dry-run (§11 M5a default): it enumerates + classifies the
//! medulla `agent-memory/` store and prints the plan JSON, mutating NOTHING. The
//! test spawns the REAL binary against a synthetic runtime fixture (never the
//! developer's live `~/.m1nd`) and asserts the plan keys + the count-conservation
//! gate — the faithful seam, because the flag lives in the process entry point
//! (`cli.rs` + `main.rs`), which only the binary exercises.
//!
//! RED on `main`: the flag does not exist, so clap exits non-zero with an
//! "unexpected argument" error and prints no plan JSON. GREEN with the wiring:
//! stdout is the `m1nd-medulla-migrate-v0` plan payload.

use std::path::Path;
use std::process::Command;

/// Path to the compiled binary under test (Cargo sets `CARGO_BIN_EXE_<name>`).
const BIN: &str = env!("CARGO_BIN_EXE_m1nd-mcp");

/// A minimal `.light.md` with the given frontmatter + body (mirrors the module's
/// own fixture shape).
fn light_doc(node: &str, source_agent: &str, body: &str) -> String {
    format!(
        "---\nProtocol: L1GHT/1.0\nNode: {node}\nState: authored\nCreated: 1700000000000\nSource-Agent: {source_agent}\n---\n\n# {node}\n\n## {node}\n\n{body}\n"
    )
}

/// Run the binary with the given args + a synthetic runtime dir. Returns
/// (status_code, stdout, stderr). Isolated: a private registry dir + no GUI so it
/// never touches the developer's real runtime.
fn run(args: &[&str], runtime_dir: &Path) -> (i32, String, String) {
    let out = Command::new(BIN)
        .args(args)
        .arg("--runtime-dir")
        .arg(runtime_dir)
        .arg("--no-gui")
        .env("M1ND_REGISTRY_DIR", runtime_dir.join("registry"))
        .env("M1ND_NO_GUI", "1")
        .output()
        .expect("spawn m1nd-mcp");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn medulla_migrate_plan_prints_the_dry_run_plan_and_mutates_nothing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime_dir = tmp.path().join("runtime");
    let medulla = runtime_dir.join("agent-memory");
    std::fs::create_dir_all(&medulla).expect("mk medulla store");

    // A code-anchored repo fact (→ Project) and a doctrine claim (→ Medulla),
    // neither carrying an Origin-Brain line (the RED framing of M5a).
    std::fs::write(
        medulla.join("sliceship.light.md"),
        light_doc(
            "SliceShip",
            "closer",
            "shipped.\n\n[⍂ entity: SliceShip]\n[𝔻 evidence: m1nd-mcp/src/server.rs]\n",
        ),
    )
    .expect("write repo-fact claim");
    std::fs::write(
        medulla.join("doctrine.light.md"),
        light_doc(
            "Doctrine",
            "orchestrator",
            "The maintainer prefers pt-BR replies always.\n\n[⍂ entity: Doctrine]\n",
        ),
    )
    .expect("write doctrine claim");

    // Snapshot the store bytes BEFORE the plan runs — plan must be pure-read.
    let before = std::fs::read_to_string(medulla.join("sliceship.light.md")).unwrap();

    let (code, stdout, stderr) = run(&["--medulla-migrate", "plan"], &runtime_dir);
    assert_eq!(
        code, 0,
        "`--medulla-migrate plan` must exit 0; stderr:\n{stderr}\nstdout:\n{stdout}"
    );

    let plan: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout must be the plan JSON, got parse error {e}:\n{stdout}"));

    assert_eq!(
        plan["schema"], "m1nd-medulla-migrate-v0",
        "the migrate payload carries its schema tag"
    );
    assert_eq!(plan["mode"], "plan", "mode echoes the requested subcommand");
    let p = &plan["plan"];
    assert_eq!(p["baseline_count"], 2, "two live claims in the store");
    assert_eq!(
        p["project_count"], 1,
        "the code-anchored fact routes to project"
    );
    assert_eq!(
        p["medulla_count"], 1,
        "the doctrine claim stays on the medulla"
    );
    assert_eq!(
        p["count_conserved"], true,
        "the count-conservation gate holds (baseline == project + medulla)"
    );
    assert!(
        p["claims"].as_array().map(|a| a.len()) == Some(2),
        "one row per claim, got: {}",
        p["claims"]
    );

    // PURE-READ: the store is byte-identical after the plan.
    let after = std::fs::read_to_string(medulla.join("sliceship.light.md")).unwrap();
    assert_eq!(
        before, after,
        "plan is a dry-run — it must not mutate the store"
    );
    // apply's backup dirs never get created by a plan.
    let backups: Vec<_> = std::fs::read_dir(&medulla)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".m5a-backup-"))
        .collect();
    assert!(backups.is_empty(), "plan writes no backup dir");
}

#[test]
fn medulla_migrate_requires_an_explicit_subcommand() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime_dir = tmp.path().join("runtime");
    std::fs::create_dir_all(runtime_dir.join("agent-memory")).expect("mk store");

    // No value → clap error (the flag takes a required value; no default).
    let (code, _stdout, _stderr) = run(&["--medulla-migrate"], &runtime_dir);
    assert_ne!(
        code, 0,
        "a bare --medulla-migrate with no value must be a usage error"
    );

    // An unknown value → rejected (only plan|apply|rollback are legal).
    let (code2, _o2, _e2) = run(&["--medulla-migrate", "frobnicate"], &runtime_dir);
    assert_ne!(code2, 0, "an unknown subcommand value must be rejected");
}
