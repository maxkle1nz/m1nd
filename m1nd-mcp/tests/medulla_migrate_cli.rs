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

/// A loopback port that is CLOSED right now: bind an ephemeral port, read it, then
/// drop the listener so the port frees again. Passed to the binary via
/// `M1ND_MEDULLA_GUARD_PORT` so the owner-alive guard sees no listener and the
/// migration runs — these tests use fully synthetic runtime dirs and must not be
/// gated on (or race) the developer's real served owner on the default port.
fn closed_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind ephemeral")
        .local_addr()
        .unwrap()
        .port()
}

/// Run the binary with the owner-alive guard pointed at an explicit port.
fn run_with_guard_port(
    args: &[&str],
    runtime_dir: &Path,
    guard_port: u16,
) -> (i32, String, String) {
    let out = Command::new(BIN)
        .args(args)
        .arg("--runtime-dir")
        .arg(runtime_dir)
        .arg("--no-gui")
        .env("M1ND_REGISTRY_DIR", runtime_dir.join("registry"))
        .env("M1ND_NO_GUI", "1")
        .env("M1ND_MEDULLA_GUARD_PORT", guard_port.to_string())
        .output()
        .expect("spawn m1nd-mcp");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// Run the binary with the given args + a synthetic runtime dir. Returns
/// (status_code, stdout, stderr). Isolated: a private registry dir + no GUI so it
/// never touches the developer's real runtime, and the owner-alive guard is pointed
/// at a closed port so `apply`/`rollback` are not gated on a live owner.
fn run(args: &[&str], runtime_dir: &Path) -> (i32, String, String) {
    run_with_guard_port(args, runtime_dir, closed_port())
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

/// Seed a runtime whose AMBIENT session binding resolves to `ambient_repo`: the
/// owner's `ingest_roots.json` points at that repo, so a boot picks it up as the
/// bound project. This reproduces the field condition (2026-07-05) where a second
/// agent on another host had bound the owner to an unrelated repo, and the migrate
/// derived its destination brain from that ambient binding — moving m1nd's legacy
/// memories into the wrong brain's store (a MED-INV-1 cross-brain contamination).
fn seed_runtime_bound_to(runtime_dir: &Path, ambient_repo: &Path) {
    let medulla = runtime_dir.join("agent-memory");
    std::fs::create_dir_all(&medulla).expect("mk medulla store");
    std::fs::create_dir_all(ambient_repo).expect("mk ambient repo dir");
    // The owner boots its bound project root from ingest_roots.json next to the
    // graph (session::load_ingest_roots). Point it at the ambient repo.
    std::fs::write(
        runtime_dir.join("ingest_roots.json"),
        serde_json::to_string_pretty(&vec![ambient_repo.to_string_lossy().to_string()]).unwrap(),
    )
    .expect("write ingest_roots.json");
    // One code-anchored repo fact to move + one doctrine claim that stays.
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
}

/// Recursively collect the `.light.md` file names living under a `project-brains/`
/// tree, so the test can prove WHERE the moved claim landed without recomputing
/// the private fingerprint-hashing scheme.
fn light_files_under(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(light_files_under(&p));
            } else if p
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".light.md"))
            {
                out.push(p.to_string_lossy().to_string());
            }
        }
    }
    out
}

/// RED (field bug 2026-07-05): `--medulla-migrate apply` WITHOUT an explicit
/// destination must refuse and exit non-zero. Deriving the destination brain from
/// the ambient session binding silently moved m1nd's legacy memories into another
/// repo's brain. `apply` must never touch the store without an explicit target.
#[test]
fn apply_without_explicit_project_root_is_refused() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime_dir = tmp.path().join("runtime");
    let ambient_repo = tmp.path().join("repo-alpha");
    seed_runtime_bound_to(&runtime_dir, &ambient_repo);

    let (code, stdout, stderr) = run(&["--medulla-migrate", "apply"], &runtime_dir);
    assert_ne!(
        code, 0,
        "apply with no --migrate-project-root must exit non-zero; stdout:\n{stdout}"
    );
    assert!(
        stderr.contains("--migrate-project-root"),
        "the refusal must name the required flag; stderr:\n{stderr}"
    );

    // It refused BEFORE mutating: no backup dir, no claim moved anywhere.
    let medulla = runtime_dir.join("agent-memory");
    let backups: Vec<_> = std::fs::read_dir(&medulla)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".m5a-backup-"))
        .collect();
    assert!(backups.is_empty(), "a refused apply writes no backup dir");
    assert!(
        medulla.join("sliceship.light.md").exists(),
        "a refused apply leaves the medulla store untouched"
    );
    let moved = light_files_under(&runtime_dir.join("project-brains"));
    assert!(
        moved.is_empty(),
        "a refused apply moves nothing into any project brain, found: {moved:?}"
    );
}

/// RED (the heart of the bug): `apply` must derive its destination brain from the
/// EXPLICIT `--migrate-project-root`, IGNORING the ambient session binding. Here
/// the owner is bound to `repo-alpha` (ingest_roots.json) but we migrate to
/// `repo-beta`. The repo fact must land in repo-beta's brain and stamp
/// `Origin-Brain: <repo-beta>` — never repo-alpha's.
#[test]
fn apply_uses_explicit_target_and_ignores_ambient_binding() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime_dir = tmp.path().join("runtime");
    let ambient_repo = tmp.path().join("repo-alpha"); // the WRONG ambient binding
    let target_repo = tmp.path().join("repo-beta"); // the explicit destination
    seed_runtime_bound_to(&runtime_dir, &ambient_repo);
    std::fs::create_dir_all(&target_repo).expect("mk target repo dir");

    let (code, stdout, stderr) = run(
        &[
            "--medulla-migrate",
            "apply",
            "--migrate-project-root",
            &target_repo.to_string_lossy(),
        ],
        &runtime_dir,
    );
    assert_eq!(
        code, 0,
        "apply with an explicit target must succeed; stderr:\n{stderr}\nstdout:\n{stdout}"
    );

    let out: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("stdout must be JSON, got {e}:\n{stdout}"));
    let project_origin = out["project_origin"].as_str().unwrap_or_default();
    assert_eq!(
        project_origin,
        target_repo.to_string_lossy(),
        "project_origin must be the EXPLICIT target, not the ambient binding"
    );
    assert!(
        !project_origin.contains("repo-alpha"),
        "the ambient binding (repo-alpha) must NOT be the destination; got {project_origin}"
    );

    // The moved claim landed in repo-beta's brain, and its Origin-Brain stamp is
    // repo-beta — proving the ambient binding was ignored end to end.
    let moved = light_files_under(&runtime_dir.join("project-brains"));
    let sliceship: Vec<_> = moved
        .iter()
        .filter(|p| p.ends_with("sliceship.light.md"))
        .collect();
    assert_eq!(
        sliceship.len(),
        1,
        "the repo fact moved into exactly one project brain, found: {moved:?}"
    );
    let moved_text = std::fs::read_to_string(sliceship[0]).unwrap();
    assert!(
        moved_text.contains(&format!("Origin-Brain: {}", target_repo.to_string_lossy())),
        "the moved claim is stamped with the explicit target origin, got:\n{moved_text}"
    );
    assert!(
        !moved_text.contains("Origin-Brain: ") || !moved_text.contains("repo-alpha"),
        "the moved claim must NOT be stamped with the ambient binding"
    );
}

/// Read a store dir's `project_brain.json` manifest and return its `project_root`
/// field, mirroring exactly what the routing layer's `manifest_matches` /
/// `resolve` require to MOUNT a brain (a store without this manifest, or whose
/// `project_root` does not equal the resolved key, is an unmountable orphan).
/// `None` = no manifest / unreadable / no `project_root` — the orphan condition.
fn manifest_project_root(store_dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(store_dir.join("project_brain.json")).ok()?;
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()?
        .get("project_root")?
        .as_str()
        .map(|s| s.to_string())
}

/// RED (LEVA 3-PREP, orphan-brain — field report 2026-07-05T22:31): after a
/// successful `apply` the destination project brain must be MOUNTABLE — its store
/// dir must carry a `project_brain.json` manifest whose `project_root` equals the
/// `--migrate-project-root`, plus `brain_kind: "project"`. Today `apply` writes the
/// `.light.md` files into `project-brains/<fp>/agent-memory/` but never registers
/// the brain (no manifest), so `resolve`/`knows` return None and the owner cannot
/// mount the moved memories — de-facto data loss at the routing layer.
#[test]
fn apply_registers_the_destination_brain_so_it_is_mountable() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime_dir = tmp.path().join("runtime");
    let target_repo = tmp.path().join("repo-beta");
    seed_runtime_bound_to(&runtime_dir, &tmp.path().join("repo-alpha"));
    std::fs::create_dir_all(&target_repo).expect("mk target repo dir");

    let (code, stdout, stderr) = run(
        &[
            "--medulla-migrate",
            "apply",
            "--migrate-project-root",
            &target_repo.to_string_lossy(),
        ],
        &runtime_dir,
    );
    assert_eq!(
        code, 0,
        "apply with an explicit target must succeed; stderr:\n{stderr}\nstdout:\n{stdout}"
    );

    // The moved claim landed under some project-brains/<fp>/agent-memory/ dir. Its
    // store dir is that agent-memory's PARENT — where the manifest must live.
    let moved = light_files_under(&runtime_dir.join("project-brains"));
    let sliceship: Vec<_> = moved
        .iter()
        .filter(|p| p.ends_with("sliceship.light.md"))
        .collect();
    assert_eq!(
        sliceship.len(),
        1,
        "the repo fact moved into exactly one project brain, found: {moved:?}"
    );
    let store_dir = Path::new(sliceship[0])
        .parent()
        .and_then(|agent_memory| agent_memory.parent())
        .expect("store dir = agent-memory's parent")
        .to_path_buf();

    // THE MOUNT CONTRACT: a `project_brain.json` naming the target repo must exist.
    // Without it the store is an orphan `resolve`/`knows` can never return.
    let canonical_target = std::fs::canonicalize(&target_repo)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| target_repo.to_string_lossy().to_string());
    let manifest_root = manifest_project_root(&store_dir).unwrap_or_else(|| {
        panic!(
            "orphan store: no mountable project_brain.json in {}; contents: {:?}",
            store_dir.display(),
            std::fs::read_dir(&store_dir)
                .map(|e| e.flatten().map(|d| d.file_name()).collect::<Vec<_>>())
                .unwrap_or_default()
        )
    });
    assert_eq!(
        manifest_root, canonical_target,
        "the manifest's project_root must equal the --migrate-project-root so resolve() mounts it"
    );

    // brain_kind: "project" — the shared phonebook tells this brain from the medulla.
    let manifest_text =
        std::fs::read_to_string(store_dir.join("project_brain.json")).expect("read manifest");
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text).expect("manifest json");
    assert_eq!(
        manifest["brain_kind"], "project",
        "the registered brain is stamped brain_kind: project, got:\n{manifest_text}"
    );
}

/// RED (field bug 2026-07-05, idempotency): running `apply` a SECOND time over an
/// already-migrated store (every claim moved or stamped, nothing left to do) must
/// report `already_migrated` with `moved: 0` and `count_conserved != false`,
/// WITHOUT writing a fresh (empty) backup or degrading the store. In the field the
/// second invocation reported `count_conserved: false` and created an empty backup.
#[test]
fn apply_is_idempotent_on_an_already_migrated_store() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime_dir = tmp.path().join("runtime");
    let target_repo = tmp.path().join("repo-beta");
    seed_runtime_bound_to(&runtime_dir, &tmp.path().join("repo-alpha"));
    std::fs::create_dir_all(&target_repo).expect("mk target repo dir");
    let medulla = runtime_dir.join("agent-memory");

    let args = [
        "--medulla-migrate",
        "apply",
        "--migrate-project-root",
        target_repo.to_str().unwrap(),
    ];

    // First apply: migrates (moves the repo fact, stamps the doctrine, backs up).
    let (code1, stdout1, stderr1) = run(&args, &runtime_dir);
    assert_eq!(code1, 0, "first apply must succeed; stderr:\n{stderr1}");
    let out1: serde_json::Value = serde_json::from_str(stdout1.trim()).expect("json1");
    assert_eq!(
        out1["plan"]["already_migrated"], false,
        "the first apply actually migrates, got:\n{stdout1}"
    );
    // Clear the (legitimate) first backup so the assertion below isolates the
    // SECOND run's behavior.
    for e in std::fs::read_dir(&medulla).unwrap().flatten() {
        if e.file_name().to_string_lossy().starts_with(".m5a-backup-") {
            std::fs::remove_dir_all(e.path()).unwrap();
        }
    }

    // Second apply over the now-migrated store: nothing to move, nothing to stamp.
    let (code2, stdout2, stderr2) = run(&args, &runtime_dir);
    assert_eq!(
        code2, 0,
        "second apply over an already-migrated store must exit 0; stderr:\n{stderr2}\nstdout:\n{stdout2}"
    );
    let out2: serde_json::Value = serde_json::from_str(stdout2.trim())
        .unwrap_or_else(|e| panic!("stdout must be JSON, got {e}:\n{stdout2}"));
    assert_eq!(
        out2["plan"]["already_migrated"], true,
        "a second apply with nothing to do reports already_migrated; got:\n{stdout2}"
    );
    assert_eq!(
        out2["plan"]["moved_to_project"], 0,
        "already-migrated apply moved zero claims"
    );
    assert_ne!(
        out2["plan"]["count_conserved"], false,
        "already-migrated apply must not report count_conserved:false (the field regression)"
    );

    // The essential guard: the SECOND run wrote NO fresh backup dir.
    let backups: Vec<_> = std::fs::read_dir(&medulla)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".m5a-backup-"))
        .collect();
    assert!(
        backups.is_empty(),
        "an already-migrated apply writes no backup dir, found: {backups:?}"
    );
}

/// RED (owner-alive guard, end to end): with a live listener on the guard port,
/// `apply` must refuse ("stop the served owner first") and exit non-zero WITHOUT
/// mutating the store — the offline migration must never race a live served owner.
#[test]
fn apply_refuses_while_a_listener_is_up_on_the_guard_port() {
    use std::net::TcpListener;
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime_dir = tmp.path().join("runtime");
    let target_repo = tmp.path().join("repo-beta");
    seed_runtime_bound_to(&runtime_dir, &tmp.path().join("repo-alpha"));
    std::fs::create_dir_all(&target_repo).expect("mk target repo dir");
    let medulla = runtime_dir.join("agent-memory");

    // Stand in for a live served owner on an ephemeral port and point the guard
    // at it (the child holds the socket open for the duration of the call).
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stand-in owner");
    let port = listener.local_addr().unwrap().port();

    let (code, stdout, stderr) = run_with_guard_port(
        &[
            "--medulla-migrate",
            "apply",
            "--migrate-project-root",
            &target_repo.to_string_lossy(),
        ],
        &runtime_dir,
        port,
    );
    assert_ne!(
        code, 0,
        "apply must refuse while a listener is up; stdout:\n{stdout}"
    );
    assert!(
        stderr
            .to_ascii_lowercase()
            .contains("stop the served owner"),
        "the refusal tells the maintainer to stop the served owner; stderr:\n{stderr}"
    );
    // Refused before mutating: no backup, source claim intact.
    let backups: Vec<_> = std::fs::read_dir(&medulla)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".m5a-backup-"))
        .collect();
    assert!(backups.is_empty(), "a guard-refused apply writes no backup");
    assert!(
        medulla.join("sliceship.light.md").exists(),
        "a guard-refused apply leaves the medulla store untouched"
    );
}
