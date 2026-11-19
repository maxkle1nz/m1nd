//! Freshness at first contact: a NEW owner process that loads a persisted graph
//! built at an older commit must say so on `north` and `health`, name the exact
//! refresh call, and go quiet once that call has run.
//!
//! The scenario is the field one: the graph was born at commit C1, the code moved
//! on to C2 (a function slid more than ten lines down), and a fresh session opened
//! on the stale snapshot. Without the note, a stale `line_start` makes a true
//! citation look false.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_m1nd-mcp");
const AGENT: &str = "graph-freshness-probe";
const TARGET: &str = "freshness_probe_target";
/// The drift note's byte budget (criteria F3).
const NOTE_BYTE_BUDGET: usize = 300;

struct Owner {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: Receiver<String>,
    stdout_reader: Option<JoinHandle<()>>,
    next_id: i64,
}

impl Owner {
    fn spawn(workspace: &Path, runtime: &Path) -> Self {
        let mut command = Command::new(BIN);
        command
            .arg("--stdio")
            .arg("--no-gui")
            .current_dir(workspace);
        confine(&mut command, runtime, workspace);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn checkout binary");
        let stdin = child.stdin.take().expect("owner stdin");
        let stdout = child.stdout.take().expect("owner stdout");
        let (stdout_tx, stdout_rx) = mpsc::channel();
        let stdout_reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if stdout_tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut owner = Self {
            child,
            stdin: Some(stdin),
            stdout: stdout_rx,
            stdout_reader: Some(stdout_reader),
            next_id: 0,
        };
        let initialized = owner.request(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": AGENT, "version": "1.0" }
            }),
        );
        assert!(
            initialized.get("result").is_some(),
            "initialize must succeed: {initialized}"
        );
        owner
    }

    fn request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        self.next_id += 1;
        let id = self.next_id;
        let stdin = self.stdin.as_mut().expect("owner stdin");
        writeln!(
            stdin,
            "{}",
            serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
        )
        .expect("write MCP request");
        stdin.flush().expect("flush MCP request");
        loop {
            let line = self
                .stdout
                .recv_timeout(Duration::from_secs(120))
                .unwrap_or_else(|error| panic!("owner did not reply to request {id}: {error}"));
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if value.get("id").and_then(serde_json::Value::as_i64) == Some(id) {
                return value;
            }
        }
    }

    fn call(&mut self, name: &str, arguments: serde_json::Value) -> serde_json::Value {
        let reply = self.request(
            "tools/call",
            serde_json::json!({ "name": name, "arguments": arguments }),
        );
        assert_ne!(
            reply["result"]["isError"].as_bool(),
            Some(true),
            "tool call {name} returned an MCP error: {reply}"
        );
        reply
            .get("result")
            .unwrap_or_else(|| panic!("tool {name} returned no result: {reply}"))
            .get("structuredContent")
            .cloned()
            .or_else(|| {
                reply["result"]["content"]
                    .as_array()
                    .and_then(|items| items.first())
                    .and_then(|item| item["text"].as_str())
                    .and_then(|text| serde_json::from_str(text).ok())
            })
            .unwrap_or_else(|| reply["result"].clone())
    }

    fn shutdown(mut self) {
        drop(self.stdin.take());
        let status = self.child.wait().expect("wait for owner shutdown");
        if let Some(reader) = self.stdout_reader.take() {
            reader.join().expect("join owner stdout reader");
        }
        assert!(status.success(), "owner shutdown failed: {status}");
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        drop(self.stdin.take());
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(reader) = self.stdout_reader.take() {
            let _ = reader.join();
        }
    }
}

/// The launcher-grant contract: an explicit workspace, a private runtime, and no
/// ambient root that could leak in from the test process.
fn confine(command: &mut Command, runtime: &Path, granted_root: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let home = runtime.join("home");
    let temp = runtime.join("tmp");
    std::fs::create_dir_all(&home).expect("fixture home");
    std::fs::create_dir_all(&temp).expect("fixture temp");
    std::fs::set_permissions(runtime, std::fs::Permissions::from_mode(0o700))
        .expect("make fixture runtime owner-private");
    for name in [
        "M1ND_WORKSPACE_ROOT",
        "M1ND_PROJECT_ROOT",
        "M1ND_REPO_ROOT",
        "WORKSPACE_ROOT",
        "PROJECT_ROOT",
        "REPO_ROOT",
        "CLAUDE_PROJECT_DIR",
        "CLAUDE_WORKSPACE_ROOT",
        "ANTHROPIC_WORKSPACE_ROOT",
        "ANTIGRAVITY_WORKSPACE_ROOT",
        "ANTIGRAVITY_PROJECT_ROOT",
        "GEMINI_WORKSPACE_ROOT",
        "GEMINI_PROJECT_ROOT",
        "CURSOR_WORKSPACE_ROOT",
        "CURSOR_PROJECT_ROOT",
        "WINDSURF_WORKSPACE_ROOT",
        "WINDSURF_PROJECT_ROOT",
        "VSCODE_WORKSPACE",
        "VSCODE_CWD",
        "INIT_CWD",
        "PWD",
        "OLDPWD",
        "M1ND_RUNTIME_DIR",
        "M1ND_REGISTRY_DIR",
        "M1ND_GRAPH_SOURCE",
        "GRAPH_SNAPSHOT_PATH",
        "M1ND_PLASTICITY_STATE",
        "PLASTICITY_STATE_PATH",
        "M1ND_READ_ONLY",
        "M1ND_NO_GUI",
        "M1ND_TOOL_TIER",
        "HOME",
        "TMPDIR",
        "TMP",
        "TEMP",
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
    ] {
        command.env_remove(name);
    }
    command
        .env("M1ND_WORKSPACE_ROOT", granted_root)
        .env("M1ND_RUNTIME_DIR", runtime)
        .env("M1ND_REGISTRY_DIR", runtime.join("registry"))
        .env("M1ND_NO_GUI", "1")
        .env("HOME", home)
        .env("TMPDIR", &temp)
        .env("TMP", &temp)
        .env("TEMP", temp);
}

fn git_available() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

/// Run git in `dir` with no user or system configuration in play, so a signing
/// or hook setting on the machine running the suite cannot change the fixture.
fn git(dir: &Path, git_home: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=freshness-fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "init.defaultBranch=main",
        ])
        .args(args)
        .current_dir(dir)
        .env("HOME", git_home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The fixture crate. `padding` filler functions sit ABOVE the target, so the
/// target's first line moves by exactly that many lines between commits.
/// Returns the 1-based line the target starts on.
fn write_crate(root: &Path, padding: usize) -> u64 {
    std::fs::create_dir_all(root.join("src")).expect("create fixture src");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"repo-alpha\"\nversion = \"0.0.0\"\n",
    )
    .expect("write fixture manifest");
    let mut source = String::new();
    for index in 0..padding {
        source.push_str(&format!("pub fn filler_{index}() -> u64 {{ {index} }}\n"));
    }
    let target_line = padding as u64 + 1;
    source.push_str(&format!("pub fn {TARGET}() -> u64 {{\n    42\n}}\n"));
    std::fs::write(root.join("src/lib.rs"), source).expect("write fixture source");
    target_line
}

fn canonical_text(path: &Path) -> String {
    std::fs::canonicalize(path)
        .expect("canonical path")
        .to_string_lossy()
        .to_string()
}

fn assert_declares_drift(
    surface: &str,
    packet: &serde_json::Value,
    indexed: &str,
    head: &str,
    root: &str,
) -> serde_json::Value {
    let note = packet
        .get("graph_drift")
        .unwrap_or_else(|| panic!("{surface} must declare the drift on a stale graph: {packet}"));
    assert_eq!(
        note["indexed"].as_str(),
        Some(&indexed[..12]),
        "{surface}: {note}"
    );
    assert_eq!(
        note["head"].as_str(),
        Some(&head[..12]),
        "{surface}: {note}"
    );
    assert_eq!(note["behind"].as_u64(), Some(1), "{surface}: {note}");
    assert!(
        note["files_changed"]
            .as_u64()
            .is_some_and(|count| count >= 1),
        "{surface}: {note}"
    );
    assert_eq!(
        note["refresh"],
        serde_json::json!({ "tool": "ingest", "mode": "refresh", "path": root }),
        "{surface} must name the exact refresh call: {note}"
    );
    let bytes = serde_json::to_vec(note).expect("encode note").len();
    assert!(
        bytes <= NOTE_BYTE_BUDGET,
        "{surface}: the drift note is {bytes} bytes, over the {NOTE_BYTE_BUDGET}-byte budget: {note}"
    );
    note.clone()
}

#[test]
fn a_new_process_on_a_graph_behind_head_declares_drift_until_the_named_refresh() {
    if !git_available() {
        eprintln!("skipping: git is not on PATH");
        return;
    }
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path().join("repo-alpha");
    let runtime = temp.path().join("private-runtime");
    let git_home = temp.path().join("git-home");
    std::fs::create_dir_all(&git_home).expect("git home");

    // C1: the commit the graph is born at.
    write_crate(&workspace, 0);
    git(&workspace, &git_home, &["init", "-q"]);
    git(&workspace, &git_home, &["add", "-A"]);
    git(&workspace, &git_home, &["commit", "-q", "-m", "c1"]);
    let c1 = git(&workspace, &git_home, &["rev-parse", "HEAD"]);

    let first = Owner::spawn(&workspace, &runtime);
    first.shutdown();

    // C2: the target slides twelve lines down; the persisted graph still says C1.
    let moved_line = write_crate(&workspace, 12);
    git(&workspace, &git_home, &["commit", "-q", "-am", "c2"]);
    let c2 = git(&workspace, &git_home, &["rev-parse", "HEAD"]);
    assert_ne!(c1, c2);
    let root = canonical_text(&workspace);

    // (a) A NEW process over the persisted graph declares the drift on both surfaces.
    let mut second = Owner::spawn(&workspace, &runtime);
    let north = second.call(
        "north",
        serde_json::json!({ "agent_id": AGENT, "task": format!("where is {TARGET} defined") }),
    );
    let health = second.call("health", serde_json::json!({ "agent_id": AGENT }));
    let note = assert_declares_drift("north", &north, &c1, &c2, &root);
    assert_declares_drift("health", &health, &c1, &c2, &root);

    // (b) Running exactly the call the note names clears it, and retrieval answers
    // with the function's CURRENT line.
    let refreshed = second.call(
        "ingest",
        serde_json::json!({
            "agent_id": AGENT,
            "mode": note["refresh"]["mode"],
            "path": note["refresh"]["path"],
        }),
    );
    assert_eq!(refreshed["ok"], serde_json::json!(true), "{refreshed}");

    let north = second.call(
        "north",
        serde_json::json!({ "agent_id": AGENT, "task": format!("where is {TARGET} defined") }),
    );
    let health = second.call("health", serde_json::json!({ "agent_id": AGENT }));
    assert!(
        north.get("graph_drift").is_none(),
        "north must be silent once the graph matches HEAD: {north}"
    );
    assert!(
        health.get("graph_drift").is_none(),
        "health must be silent once the graph matches HEAD: {health}"
    );

    let seek = second.call(
        "seek",
        serde_json::json!({ "agent_id": AGENT, "query": TARGET, "top_k": 5 }),
    );
    let hit = seek["results"]
        .as_array()
        .and_then(|results| {
            results.iter().find(|result| {
                result["node_id"]
                    .as_str()
                    .is_some_and(|id| id.ends_with(&format!("fn::{TARGET}")))
            })
        })
        .unwrap_or_else(|| panic!("seek must find {TARGET}: {seek}"))
        .clone();
    assert_eq!(
        hit["line_start"].as_u64(),
        Some(moved_line),
        "seek must return the function's current line after the refresh: {hit}"
    );

    second.shutdown();
}
