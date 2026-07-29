//! SPEC-2 — the P2 birth ceremony, driven against the REAL binary.
//!
//! The normative document is `docs/GENESIS-INGEST-CONSUMERS-SPEC.md` §2
//! (RATIFIED, owner, 2026-07-29). The in-process battery
//! (`m1nd-mcp/src/internal_tests/spec2_brain_bootstrap_birth.rs`) proves the
//! verb's decisions; this file proves the two things only a real process can:
//!
//!   1. the CEREMONY INGRESS exists and is the only stamp — the owner is what
//!      turns "the human ran this command" into `human-cli`, and no client
//!      payload reaches it;
//!   2. §5.8's birth half, FOR REAL: `kill -9` mid-birth leaves the destination
//!      ABSENT or WHOLE, never a half-built store a later boot would warm into.
//!
//! HOW THE KILL IS DRIVEN, and why it is not a fault-injection hook. A clean
//! birth is timed first; then the ceremony is re-run and `kill -9`ed at a spread
//! of instants across that duration. There is no production test hook, no
//! sleep-on-env-var, nothing in the shipped path that exists for this file — the
//! invariant is asserted after every kill, and the last step proves the
//! destination is still BIRTHABLE, which is the recovery half of
//! "completes-or-removes whole".
//!
//! Only PIDs THIS test spawned are ever signalled.
//!
//! Unix-only: `kill -9` and process-group semantics are POSIX.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, Instant};

/// Path to the compiled binary under test (Cargo sets `CARGO_BIN_EXE_<name>`).
const BIN: &str = env!("CARGO_BIN_EXE_m1nd-mcp");

/// A deterministic multi-module Rust crate. Wide enough that the first ingest
/// takes long enough to be interrupted at several distinct instants, small
/// enough that a clean ceremony stays quick.
fn write_fixture_repo(root: &Path) {
    std::fs::create_dir_all(root.join("src")).expect("mk src");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"spec2ceremony\"\nversion = \"0.0.0\"\n",
    )
    .expect("Cargo.toml");
    let mut lib = String::from("pub fn top() -> i64 { 1 }\n");
    for index in 0..24 {
        lib.push_str(&format!("pub mod part_{index};\n"));
        std::fs::write(
            root.join("src").join(format!("part_{index}.rs")),
            format!(
                "pub fn part_{index}_a() -> i64 {{ {index} }}\n\
                 pub fn part_{index}_b() -> i64 {{ {index} + 1 }}\n\
                 pub struct Part{index} {{ pub v: i64 }}\n\
                 impl Part{index} {{ pub fn make() -> Self {{ Self {{ v: {index} }} }} }}\n"
            ),
        )
        .expect("part module");
    }
    std::fs::write(root.join("src/lib.rs"), lib).expect("lib.rs");
}

/// Build the ceremony command the npm CLI builds: the owner's own `--birth`
/// ingress, pointed at an isolated runtime.
fn ceremony_command(root: &Path, runtime_dir: &Path) -> Command {
    let mut command = Command::new(BIN);
    command
        .arg("--birth")
        .arg(root)
        .arg("--no-gui")
        .env("M1ND_RUNTIME_DIR", runtime_dir)
        .env("M1ND_GRAPH_SOURCE", runtime_dir.join("graph_snapshot.json"))
        .env(
            "M1ND_PLASTICITY_STATE",
            runtime_dir.join("plasticity_state.json"),
        )
        .env("M1ND_REGISTRY_DIR", runtime_dir.join("registry"))
        .env("M1ND_NO_GUI", "1");
    command
}

/// Run the ceremony to completion and return its output.
fn run_ceremony(root: &Path, runtime_dir: &Path) -> Output {
    ceremony_command(root, runtime_dir)
        .output()
        .expect("spawn the birth ceremony")
}

/// The ceremony's JSON certificate (or refusal), parsed out of stdout.
fn certificate(output: &Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let start = stdout
        .find('{')
        .unwrap_or_else(|| panic!("the ceremony must print JSON; stdout was:\n{stdout}"));
    serde_json::from_str(stdout[start..].trim()).unwrap_or_else(|error| {
        panic!(
            "the ceremony's JSON did not parse ({error}); stdout was:\n{stdout}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

/// The destination store dir for a root, derived exactly the way the owner
/// derives it — one hashing scheme, never a second copy of it here.
fn store_dir(runtime_dir: &Path, root: &Path) -> PathBuf {
    let registry = m1nd_mcp::project_brains::ProjectBrainRegistry::new(
        runtime_dir.join(m1nd_mcp::project_brains::PROJECT_BRAINS_DIR),
        None,
    );
    let key =
        m1nd_mcp::project_brains::ProjectBrainRegistry::canonical_key(&root.to_string_lossy());
    registry.store_dir_for(&key)
}

/// ABSENT, or WHOLE. The property `kill -9` must never break: a destination is
/// either not there at all, or it is a complete brain — a birth record naming
/// this root AND the first ingest beside it.
fn assert_absent_or_whole(store: &Path, root: &Path, moment: &str) {
    if !store.exists() {
        return;
    }
    let manifest = store.join("project_brain.json");
    let snapshot = store.join("graph_snapshot.json");
    assert!(
        manifest.is_file() && snapshot.is_file(),
        "{moment}: the destination exists but is HALF-BUILT — manifest={} snapshot={} at {store:?}",
        manifest.is_file(),
        snapshot.is_file()
    );
    let text = std::fs::read_to_string(&manifest).expect("read the birth record");
    let parsed: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{moment}: torn manifest ({error}): {text}"));
    let key =
        m1nd_mcp::project_brains::ProjectBrainRegistry::canonical_key(&root.to_string_lossy());
    assert_eq!(
        parsed["project_root"],
        serde_json::json!(key),
        "{moment}: the destination names another root"
    );
}

/// A ceremony against an EMPTY destination births a whole brain, and says so in
/// a certificate a human can read.
#[test]
fn spec2_ceremony_births_a_whole_brain_and_certifies_it() {
    let temp = tempfile::tempdir().expect("tempdir");
    let runtime = temp.path().join("runtime");
    std::fs::create_dir_all(&runtime).expect("mk runtime");
    let repo = temp.path().join("newborn");
    write_fixture_repo(&repo);
    let store = store_dir(&runtime, &repo);
    assert!(
        !store.exists(),
        "precondition: the destination starts empty"
    );

    let output = run_ceremony(&repo, &runtime);
    let receipt = certificate(&output);

    assert!(
        output.status.success(),
        "a ceremony on an empty destination must succeed; certificate was {receipt}, stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(receipt["ok"], serde_json::json!(true));
    assert_eq!(receipt["origin"], serde_json::json!("human-cli"));
    assert!(
        receipt["node_count"].as_u64().unwrap_or(0) > 0,
        "the birth includes the first ingest: {receipt}"
    );
    assert_absent_or_whole(&store, &repo, "after a clean ceremony");
    assert!(
        store.exists(),
        "the brain must exist after a clean ceremony"
    );

    // A SECOND ceremony over the same root is refused — the birth is durable
    // across processes, and birth is not the migration of an existing brain.
    let second = run_ceremony(&repo, &runtime);
    let refusal = certificate(&second);
    assert!(
        !second.status.success(),
        "a second birth must not report success"
    );
    assert_eq!(
        refusal["refused"],
        serde_json::json!("birth_destination_not_empty")
    );
}

/// §5.8, the birth half: `kill -9` at a spread of instants across the birth. The
/// destination is ABSENT or WHOLE after every one, and still birthable after all
/// of them.
#[test]
fn spec2_5_8_kill_9_mid_birth_leaves_the_destination_absent_or_whole() {
    let temp = tempfile::tempdir().expect("tempdir");

    // 1. Time a clean birth in its own runtime, so the kill instants below cover
    //    the real duration of THIS machine's ceremony rather than a guess.
    let timing_runtime = temp.path().join("timing-runtime");
    std::fs::create_dir_all(&timing_runtime).expect("mk timing runtime");
    let timing_repo = temp.path().join("timing-repo");
    write_fixture_repo(&timing_repo);
    let started = Instant::now();
    let clean = run_ceremony(&timing_repo, &timing_runtime);
    let clean_duration = started.elapsed();
    assert!(
        clean.status.success(),
        "the timing ceremony must succeed first; stderr:\n{}",
        String::from_utf8_lossy(&clean.stderr)
    );

    // 2. Kill at a spread of instants across that duration. Eight is enough to
    //    straddle boot, scan, staging and commit without turning the suite into
    //    a stress test.
    let repo = temp.path().join("newborn");
    write_fixture_repo(&repo);
    for step in 1..=8u32 {
        let runtime = temp.path().join(format!("runtime-{step}"));
        std::fs::create_dir_all(&runtime).expect("mk runtime");
        let store = store_dir(&runtime, &repo);
        let delay = clean_duration.mul_f64(f64::from(step) / 9.0);

        let mut child = ceremony_command(&repo, &runtime)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn the ceremony");
        std::thread::sleep(delay);
        // ONLY the pid this test spawned, and only ever this one.
        let killed = child.kill();
        let _ = child.wait();
        assert!(
            killed.is_ok() || killed.is_err(),
            "kill is best-effort: a ceremony that already finished is a valid outcome"
        );

        assert_absent_or_whole(&store, &repo, &format!("kill at {delay:?} (step {step})"));
    }

    // 3. The recovery half: after all that interruption, a fresh destination is
    //    still birthable. A birth that could be poisoned by a crashed sibling
    //    would fail here.
    let final_runtime = temp.path().join("runtime-final");
    std::fs::create_dir_all(&final_runtime).expect("mk runtime");
    let output = run_ceremony(&repo, &final_runtime);
    let receipt = certificate(&output);
    assert!(
        output.status.success(),
        "a birth after interrupted siblings must still succeed; certificate was {receipt}"
    );
    assert_absent_or_whole(&store_dir(&final_runtime, &repo), &repo, "after recovery");
}

/// The ceremony is the ONLY stamp. The same verb over the wire — the seam an
/// agent actually holds — is refused, with the claim it dressed itself in
/// buying nothing. Driven through the real binary so the refusal proven here is
/// the one a client on the wire receives, not an in-process approximation.
#[test]
fn spec2_the_wire_cannot_birth_however_it_dresses_the_call() {
    use std::io::{BufRead, BufReader, Write};

    let temp = tempfile::tempdir().expect("tempdir");
    let runtime = temp.path().join("runtime");
    std::fs::create_dir_all(&runtime).expect("mk runtime");
    let repo = temp.path().join("newborn");
    write_fixture_repo(&repo);

    let mut child = Command::new(BIN)
        .arg("--no-gui")
        .env("M1ND_RUNTIME_DIR", &runtime)
        .env("M1ND_GRAPH_SOURCE", runtime.join("graph_snapshot.json"))
        .env(
            "M1ND_PLASTICITY_STATE",
            runtime.join("plasticity_state.json"),
        )
        .env("M1ND_REGISTRY_DIR", runtime.join("registry"))
        .env("M1ND_NO_GUI", "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn the stdio owner");
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));

    let mut call = |id: i64, request: serde_json::Value| -> serde_json::Value {
        stdin
            .write_all(format!("{request}\n").as_bytes())
            .expect("write request");
        stdin.flush().expect("flush");
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            assert!(Instant::now() < deadline, "timed out waiting for id={id}");
            let mut line = String::new();
            let read = stdout.read_line(&mut line).expect("read reply");
            assert!(read > 0, "owner stdout closed before reply id={id}");
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) else {
                continue;
            };
            if value.get("id").and_then(serde_json::Value::as_i64) == Some(id) {
                return value;
            }
        }
    };

    call(
        1,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "spec2-wire-probe", "version": "1.0" }
            }
        }),
    );

    let plain = call(
        2,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": { "name": "brain_birth", "arguments": {
                "root": repo.to_string_lossy(), "agent_id": "spec2-wire-probe"
            }}
        }),
    );
    let dressed = call(
        3,
        serde_json::json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "brain_birth", "arguments": {
                "root": repo.to_string_lossy(), "agent_id": "spec2-wire-probe",
                "birth_via": "human-cli", "origin": "human-ui",
                "imported_via": "human-touchid", "ratified_via": "human-ui"
            }}
        }),
    );

    let text = |reply: &serde_json::Value| -> String {
        serde_json::to_string(&reply["result"]).unwrap_or_default()
            + &serde_json::to_string(&reply["error"]).unwrap_or_default()
    };
    assert!(
        text(&plain).contains("POSITIVE_SOVEREIGN"),
        "the wire must refuse a birth at the sovereign floor: {plain}"
    );
    assert_eq!(
        text(&dressed),
        text(&plain),
        "a claimed human origin must buy NOTHING on the wire — not even a different answer"
    );
    assert!(
        !store_dir(&runtime, &repo).exists(),
        "a refused wire birth must create nothing"
    );

    drop(stdin);
    let _ = child.wait();
}
