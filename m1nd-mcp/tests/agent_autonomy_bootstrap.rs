//! Agent autonomy 1A: an explicitly launcher-scoped stdio owner prepares its
//! derived graph before the first public retrieval call.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{
    mpsc::{self, Receiver},
    Mutex, OnceLock,
};
use std::thread::JoinHandle;
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_m1nd-mcp");
const AGENT: &str = "agent-autonomy-bootstrap-probe";
const UNIQUE_SYMBOL: &str = "virgin_workspace_signal_9f4d";

/// Each request can trigger a full local embedding pass in a child process.
/// Serializing that bounded external resource keeps this integration target
/// deterministic under libtest's default parallel runner.
fn owner_request_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn write_fixture(root: &Path, symbol: &str) {
    std::fs::create_dir_all(root.join("src")).expect("create fixture src");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"repo-alpha\"\nversion = \"0.0.0\"\n",
    )
    .expect("write fixture manifest");
    std::fs::write(
        root.join("src/lib.rs"),
        format!("pub fn {symbol}() -> u64 {{ 42 }}\n"),
    )
    .expect("write fixture source");
}

fn write_memory_fixture(root: &Path, symbol: &str, value: u64) {
    write_fixture(root, symbol);
    let mut source = format!("pub fn {symbol}() -> u64 {{ {value} }}\n");
    for index in 0..32 {
        source.push_str(&format!(
            "pub fn stable_memory_helper_{index}() -> u64 {{ {index} }}\n"
        ));
    }
    std::fs::write(root.join("src/lib.rs"), source).expect("write memory-rich fixture source");
}

fn source_snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    ["Cargo.toml", "src/lib.rs"]
        .into_iter()
        .map(|relative| {
            (
                relative.to_string(),
                std::fs::read(root.join(relative)).expect("read fixture source"),
            )
        })
        .collect()
}

struct Owner {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: Receiver<String>,
    stdout_reader: Option<JoinHandle<()>>,
    next_id: i64,
}

impl Owner {
    fn spawn(workspace: &Path, runtime: &Path) -> Self {
        Self::spawn_with_grant(workspace, runtime, workspace)
    }

    fn spawn_with_grant(current_dir: &Path, runtime: &Path, granted_root: &Path) -> Self {
        Self::spawn_with_overrides(current_dir, runtime, granted_root, &[])
    }

    fn spawn_with_overrides(
        current_dir: &Path,
        runtime: &Path,
        granted_root: &Path,
        overrides: &[(&str, &Path)],
    ) -> Self {
        // Bootstrap performs embedding before the first MCP reply. Hold the same
        // integration-only gate through spawn and initialize so parallel libtest
        // workers cannot saturate that bounded resource before request() runs.
        let _bootstrap_guard = owner_request_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut command = base_owner_command(current_dir);
        confine_owner_command(&mut command, runtime, granted_root);
        for (name, value) in overrides {
            command.env(name, value);
        }
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
        let initialized = owner.request_unlocked(
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
        let _request_guard = owner_request_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.request_unlocked(method, params)
    }

    fn request_unlocked(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        self.next_id += 1;
        let id = self.next_id;
        writeln!(
            self.stdin.as_mut().expect("owner stdin"),
            "{}",
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params,
            })
        )
        .expect("write MCP request");
        self.stdin
            .as_mut()
            .expect("owner stdin")
            .flush()
            .expect("flush MCP request");

        loop {
            // A cold owner embeds before initialize replies; unlike libtest,
            // nextest runs this suite in separate processes and the shared CI
            // runner can spend over 30 seconds on that bounded startup. Keep a
            // finite deadline, but do not misreport a slow bootstrap as a hang.
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
            "positive tool call {name} returned an MCP error: {reply}"
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

#[test]
fn stdio_unknown_freshness_summary_is_honest() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path().join("repo-alpha");
    let runtime = temp.path().join("private-runtime");
    write_fixture(&workspace, UNIQUE_SYMBOL);
    let before = source_snapshot(&workspace);

    let known =
        std::fs::canonicalize(workspace.join("src/lib.rs")).expect("canonical indexed source");
    let unknown = workspace.join("src/not-indexed.rs");
    assert!(!unknown.exists(), "unknown fixture must not exist");

    let mut owner = Owner::spawn(&workspace, &runtime);
    let only_unknown = owner.call(
        "am_i_stale",
        serde_json::json!({
            "agent_id": AGENT,
            "files": [unknown.to_string_lossy()]
        }),
    );
    assert_eq!(
        only_unknown["checked"],
        serde_json::json!(1),
        "{only_unknown}"
    );
    assert_eq!(
        only_unknown["stale"],
        serde_json::json!([]),
        "{only_unknown}"
    );
    assert_eq!(
        only_unknown["fresh"],
        serde_json::json!([]),
        "{only_unknown}"
    );
    assert_eq!(
        only_unknown["unknown"],
        serde_json::json!([unknown.to_string_lossy()]),
        "{only_unknown}"
    );
    let summary = only_unknown["summary"]
        .as_str()
        .unwrap_or_else(|| panic!("summary must be a string: {only_unknown}"));
    assert!(summary.contains("unknown"), "{only_unknown}");
    assert!(!summary.contains("are fresh"), "{only_unknown}");
    assert!(!summary.contains("nothing changed"), "{only_unknown}");

    let mixed = owner.call(
        "am_i_stale",
        serde_json::json!({
            "agent_id": AGENT,
            "files": [known.to_string_lossy(), unknown.to_string_lossy()]
        }),
    );
    assert_eq!(mixed["checked"], serde_json::json!(2), "{mixed}");
    assert_eq!(mixed["stale"], serde_json::json!([]), "{mixed}");
    assert_eq!(
        mixed["fresh"],
        serde_json::json!([known.to_string_lossy()]),
        "{mixed}"
    );
    assert_eq!(
        mixed["unknown"],
        serde_json::json!([unknown.to_string_lossy()]),
        "{mixed}"
    );
    assert!(
        mixed["summary"].as_str().unwrap().contains("unknown"),
        "{mixed}"
    );
    assert!(
        !mixed["summary"].as_str().unwrap().contains("are fresh"),
        "{mixed}"
    );

    let all_fresh = owner.call(
        "am_i_stale",
        serde_json::json!({
            "agent_id": AGENT,
            "files": [known.to_string_lossy()]
        }),
    );
    assert_eq!(all_fresh["checked"], serde_json::json!(1), "{all_fresh}");
    assert_eq!(
        all_fresh["fresh"],
        serde_json::json!([known.to_string_lossy()])
    );
    assert_eq!(all_fresh["unknown"], serde_json::json!([]));
    assert!(
        all_fresh["summary"].as_str().unwrap().contains("are fresh"),
        "{all_fresh}"
    );

    owner.shutdown();
    assert_eq!(source_snapshot(&workspace), before, "source tree changed");
}

#[test]
fn launcher_grant_is_the_direct_stdio_refresh_caller_identity() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path().join("repo-alpha");
    let runtime = temp.path().join("private-runtime");
    let old_symbol = "stdio_refresh_old_signal_a91f";
    let new_symbol = "stdio_refresh_new_signal_b72e";
    write_fixture(&workspace, old_symbol);

    let first = Owner::spawn(&workspace, &runtime);
    first.shutdown();
    std::fs::write(
        workspace.join("src/lib.rs"),
        format!("pub fn {new_symbol}() -> u64 {{ 42 }}\n"),
    )
    .expect("edit fixture without changing launcher identity");

    let mut second = Owner::spawn(&workspace, &runtime);
    let refreshed = second.call(
        "ingest",
        serde_json::json!({
            "agent_id": AGENT,
            "path": std::fs::canonicalize(&workspace).expect("canonical workspace"),
            "mode": "refresh",
            "adapter": "code"
        }),
    );
    assert_eq!(refreshed["ok"], serde_json::json!(true), "{refreshed}");
    assert_eq!(
        refreshed["action"],
        serde_json::json!("graph.ingest.refresh_declared_root"),
        "{refreshed}"
    );
    assert!(refreshed.get("refused").is_none(), "{refreshed}");
    second.shutdown();

    let snapshot = std::fs::read_to_string(runtime.join("graph_snapshot.json"))
        .expect("read refreshed snapshot");
    assert!(
        snapshot.contains(new_symbol),
        "refreshed snapshot: {snapshot}"
    );
    assert!(
        !snapshot.contains(old_symbol),
        "refreshed snapshot: {snapshot}"
    );
    let roots: Vec<String> = serde_json::from_slice(
        &std::fs::read(runtime.join("ingest_roots.json")).expect("read roots"),
    )
    .expect("decode roots");
    assert_eq!(
        roots,
        vec![std::fs::canonicalize(&workspace)
            .expect("canonical root")
            .to_string_lossy()
            .to_string()]
    );
}

fn base_owner_command(current_dir: &Path) -> Command {
    let mut command = Command::new(BIN);
    command
        .arg("--stdio")
        .arg("--no-gui")
        .current_dir(current_dir);
    command
}

fn confine_owner_command(command: &mut Command, runtime: &Path, granted_root: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let runtime_was_missing = !runtime.exists();
    let home = runtime.join("home");
    let temp = runtime.join("tmp");
    std::fs::create_dir_all(&home).expect("fixture home");
    std::fs::create_dir_all(&temp).expect("fixture temp");
    let metadata = std::fs::symlink_metadata(runtime).expect("fixture runtime metadata");
    // Several positive fixtures pre-create their runtime with the process's
    // default 0755 umask. Normalize only that fixture artifact: deliberate
    // 0777 and symlink refusal cases must remain hostile for the launcher.
    if metadata.file_type().is_dir()
        && (runtime_was_missing || metadata.permissions().mode() & 0o777 == 0o755)
    {
        std::fs::set_permissions(runtime, std::fs::Permissions::from_mode(0o700))
            .expect("make fixture runtime owner-private");
    }
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
        "HOME",
        "TMPDIR",
        "TMP",
        "TEMP",
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

fn owner_command(current_dir: &Path, runtime: &Path, granted_root: &Path) -> Command {
    let mut command = base_owner_command(current_dir);
    confine_owner_command(&mut command, runtime, granted_root);
    command
}

fn attempt_start(current_dir: &Path, runtime: &Path, granted_root: &Path) -> std::process::Output {
    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": AGENT, "version": "1.0" }
        }
    });
    let mut child = owner_command(current_dir, runtime, granted_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mismatched owner");
    writeln!(
        child.stdin.as_mut().expect("mismatched owner stdin"),
        "{initialize}"
    )
    .expect("write initialize to mismatched owner");
    drop(child.stdin.take());
    child.wait_with_output().expect("wait for mismatched owner")
}

fn establish_empty_baseline(current_dir: &Path, runtime: &Path) -> std::process::Output {
    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": AGENT, "version": "1.0" }
        }
    });
    let mut command = owner_command(current_dir, runtime, current_dir);
    command.env_remove("M1ND_WORKSPACE_ROOT");
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn empty baseline owner");
    writeln!(
        child.stdin.as_mut().expect("baseline stdin"),
        "{initialize}"
    )
    .expect("write baseline initialize");
    drop(child.stdin.take());
    child.wait_with_output().expect("wait for baseline owner")
}

#[test]
fn launcher_grant_without_runtime_refuses_before_initialize_or_source_mutation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path().join("repo-alpha");
    let sandbox = temp.path().join("sandbox");
    write_fixture(&workspace, UNIQUE_SYMBOL);
    let before = source_snapshot(&workspace);

    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": AGENT, "version": "1.0" }
        }
    });
    let mut command = base_owner_command(&workspace);
    confine_owner_command(&mut command, &sandbox, &workspace);
    command
        .env_remove("M1ND_RUNTIME_DIR")
        .env_remove("M1ND_REGISTRY_DIR")
        .env("M1ND_GRAPH_SOURCE", workspace.join("forbidden-graph.json"))
        .env(
            "M1ND_PLASTICITY_STATE",
            workspace.join("forbidden-plasticity.json"),
        );
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn owner without runtime");
    writeln!(child.stdin.as_mut().expect("stdin"), "{initialize}").expect("write initialize");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("wait for refusal");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "missing runtime must refuse");
    assert!(
        stderr.contains("launcher_workspace_requires_explicit_runtime"),
        "unexpected refusal: {stderr}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("\"result\""),
        "refused owner must not answer initialize"
    );
    assert_eq!(source_snapshot(&workspace), before, "source tree changed");
    assert!(!workspace.join("forbidden-graph.json").exists());
    assert!(!workspace.join("forbidden-plasticity.json").exists());
}

#[test]
fn launcher_runtime_parent_component_refuses_before_initialize_or_mutation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path().join("repo-alpha");
    let sandbox = temp.path().join("sandbox");
    let runtime = sandbox
        .join("lexical-segment")
        .join("..")
        .join("escaped-runtime");
    write_fixture(&workspace, UNIQUE_SYMBOL);
    let before = source_snapshot(&workspace);

    let safe_runtime = sandbox.join("safe-runtime");
    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": AGENT, "version": "1.0" }
        }
    });
    let mut command = owner_command(&workspace, &safe_runtime, &workspace);
    command.env("M1ND_RUNTIME_DIR", &runtime);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn owner with lexical runtime");
    writeln!(child.stdin.as_mut().expect("stdin"), "{initialize}").expect("write initialize");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("wait for refusal");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "runtime with '..' must refuse");
    assert!(
        stderr.contains("launcher_workspace_runtime_parent_component"),
        "unexpected refusal: {stderr}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("\"result\""),
        "refused owner must not answer initialize"
    );
    assert_eq!(source_snapshot(&workspace), before, "source tree changed");
    assert!(
        !sandbox.join("escaped-runtime").exists(),
        "runtime containing '..' was created after refusal"
    );
}

#[cfg(unix)]
#[test]
fn launcher_rejects_a_shared_runtime_before_a_snapshot_temp_symlink_can_be_written() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path().join("repo-alpha");
    let runtime = temp.path().join("shared-runtime");
    let protected = temp.path().join("must-not-be-overwritten.json");
    write_fixture(&workspace, UNIQUE_SYMBOL);
    let before = source_snapshot(&workspace);
    std::fs::create_dir_all(&runtime).expect("runtime");
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o777))
        .expect("make runtime shared");
    std::fs::write(&protected, b"intact-outside-runtime").expect("protected fixture");
    symlink(&protected, runtime.join("graph_snapshot.tmp")).expect("plant temp symlink");

    let output = attempt_start(&workspace, &runtime, &workspace);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "a shared runtime must refuse before initialize: stdout={} stderr={stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        stderr.contains("launcher_workspace_runtime_not_private"),
        "unexpected refusal: {stderr}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("\"result\""),
        "refused owner must not answer initialize"
    );
    assert_eq!(
        std::fs::read(&protected).expect("read protected fixture"),
        b"intact-outside-runtime",
        "a shared runtime must not permit the graph temp symlink to escape"
    );
    assert_eq!(source_snapshot(&workspace), before, "source tree changed");
}

#[cfg(unix)]
#[test]
fn launcher_rejects_a_runtime_beneath_a_shared_parent_before_a_snapshot_temp_symlink_can_be_written(
) {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path().join("repo-alpha");
    let shared_parent = temp.path().join("shared-parent");
    let runtime = shared_parent.join("private-runtime");
    let protected = temp.path().join("must-not-be-overwritten.json");
    write_fixture(&workspace, UNIQUE_SYMBOL);
    let before = source_snapshot(&workspace);
    std::fs::create_dir_all(&runtime).expect("runtime");
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700))
        .expect("make runtime owner-private");
    std::fs::set_permissions(&shared_parent, std::fs::Permissions::from_mode(0o777))
        .expect("make runtime parent shared");
    std::fs::write(&protected, b"intact-outside-runtime").expect("protected fixture");
    symlink(&protected, runtime.join("graph_snapshot.tmp")).expect("plant temp symlink");

    let output = attempt_start(&workspace, &runtime, &workspace);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        !output.status.success(),
        "a runtime below a shared parent must refuse before initialize: stdout={} stderr={stderr}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        stderr.contains("launcher_workspace_runtime_not_private"),
        "unexpected refusal: {stderr}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("\"result\""),
        "refused owner must not answer initialize"
    );
    assert_eq!(
        std::fs::read(&protected).expect("read protected fixture"),
        b"intact-outside-runtime",
        "a shared runtime parent must not permit the graph temp symlink to escape"
    );
    assert_eq!(source_snapshot(&workspace), before, "source tree changed");
}

#[derive(Debug, Eq, PartialEq)]
struct BrainIdentity {
    node_count: u64,
    graph_path: String,
    plasticity_path: String,
    ingest_roots: Vec<String>,
}

fn canonical_text(path: &Path) -> String {
    std::fs::canonicalize(path)
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", path.display()))
        .to_string_lossy()
        .to_string()
}

fn persisted_roots(runtime: &Path) -> Vec<String> {
    serde_json::from_slice(
        &std::fs::read(runtime.join("ingest_roots.json")).expect("read persisted roots"),
    )
    .expect("decode persisted roots")
}

fn assert_memory_retrievable(
    owner: &mut Owner,
    label: &str,
    claim_text: &str,
) -> serde_json::Value {
    let found = owner.call(
        "search",
        serde_json::json!({
            "agent_id": AGENT,
            "query": claim_text,
            "mode": "semantic",
            "top_k": 20
        }),
    );
    let hits = found
        .get("results")
        .and_then(serde_json::Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| entry.to_string().contains(label))
                .count()
        })
        .unwrap_or(0);
    assert!(
        hits > 0,
        "semantic results, excluding the echoed query, must contain the memorized claim {label}: {found}"
    );
    found
}

fn assert_symbol_is_retrievable(owner: &mut Owner, runtime: &Path) -> BrainIdentity {
    let north = owner.call(
        "north",
        serde_json::json!({ "agent_id": AGENT, "task": UNIQUE_SYMBOL }),
    );
    assert_ne!(
        north.get("needs").and_then(serde_json::Value::as_str),
        Some("needs_ingest"),
        "first public north must observe the prepared graph: {north}"
    );

    let search = owner.call(
        "search",
        serde_json::json!({ "agent_id": AGENT, "query": UNIQUE_SYMBOL }),
    );
    let encoded = serde_json::to_string(&search).expect("encode search result");
    assert!(
        encoded.contains(&format!("file::src/lib.rs::fn::{UNIQUE_SYMBOL}"))
            && encoded.contains("src/lib.rs"),
        "real retrieval must find the fixture symbol and file: {search}"
    );

    let health = owner.call("health", serde_json::json!({ "agent_id": AGENT }));
    let node_count = health["node_count"]
        .as_u64()
        .unwrap_or_else(|| panic!("health must report node_count: {health}"));
    let fingerprint = &health["binding_fingerprint"];
    let graph_path = fingerprint["graph_path"]
        .as_str()
        .unwrap_or_else(|| panic!("health must report graph_path: {health}"))
        .to_string();
    let plasticity_path = fingerprint["plasticity_path"]
        .as_str()
        .unwrap_or_else(|| panic!("health must report plasticity_path: {health}"))
        .to_string();
    let canonical_runtime = std::fs::canonicalize(runtime).expect("canonical runtime");
    let canonical_graph = std::fs::canonicalize(&graph_path).expect("canonical graph path");
    let canonical_plasticity =
        std::fs::canonicalize(&plasticity_path).expect("canonical plasticity path");
    assert!(
        canonical_graph.starts_with(&canonical_runtime),
        "graph path escaped the fixture runtime: {graph_path}"
    );
    assert!(
        canonical_plasticity.starts_with(&canonical_runtime),
        "plasticity path escaped the fixture runtime: {plasticity_path}"
    );
    BrainIdentity {
        node_count,
        graph_path,
        plasticity_path,
        ingest_roots: fingerprint["ingest_roots"]
            .as_array()
            .unwrap_or_else(|| panic!("health must report ingest_roots: {health}"))
            .iter()
            .map(|root| {
                root.as_str()
                    .unwrap_or_else(|| panic!("ingest root must be a string: {health}"))
                    .to_string()
            })
            .collect(),
    }
}

#[test]
fn explicit_launcher_root_bootstraps_once_and_survives_restart() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path().join("repo-alpha");
    let runtime = temp.path().join("private-runtime");
    write_fixture(&workspace, UNIQUE_SYMBOL);
    std::fs::create_dir_all(&runtime).expect("create external runtime");
    assert!(!runtime.starts_with(&workspace));
    let before = source_snapshot(&workspace);

    let mut first = Owner::spawn(&workspace, &runtime);
    let first_count = assert_symbol_is_retrievable(&mut first, &runtime);
    let repeated_count = assert_symbol_is_retrievable(&mut first, &runtime);
    assert_eq!(
        repeated_count, first_count,
        "a second retrieval in one session must reuse the prepared graph"
    );
    first.shutdown();

    let mut restarted = Owner::spawn(&workspace, &runtime);
    let restarted_count = assert_symbol_is_retrievable(&mut restarted, &runtime);
    assert_eq!(
        restarted_count, first_count,
        "restart must reuse the same persisted brain"
    );
    restarted.shutdown();

    assert_eq!(source_snapshot(&workspace), before, "source tree changed");
    assert!(
        !workspace.join(".m1nd").exists(),
        "derived state must stay in the external runtime"
    );
}

#[test]
fn legitimate_grounded_memory_survives_restart_refresh_and_restart() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path().join("repo-alpha");
    let foreign = temp.path().join("repo-beta");
    let runtime = temp.path().join("private-runtime");
    let original_symbol = "memory_restart_original_signal_4a2d";
    let refreshed_symbol = "memory_restart_refreshed_signal_7c9e";
    let memory_label = "LauncherMemoryRestartFact9d31";
    let second_label = "LauncherMemoryGroundingFact6b82";
    let claim_text = "legitimate launcher memory remains readable after a clean restart";
    write_memory_fixture(&workspace, original_symbol, 42);
    write_fixture(&foreign, "foreign_memory_restart_signal_1f5b");

    let mut first = Owner::spawn(&workspace, &runtime);
    let memorized = first.call(
        "memorize",
        serde_json::json!({
            "agent_id": AGENT,
            "node_label": memory_label,
            "claims": [
                {
                    "label": memory_label,
                    "text": claim_text,
                    "confidence": "high",
                    "evidence": ["src/lib.rs"]
                },
                {
                    "label": second_label,
                    "text": "the durable claim remains grounded in the refreshed fixture",
                    "confidence": "high",
                    "evidence": ["src/lib.rs"]
                }
            ]
        }),
    );
    assert_eq!(
        memorized["ingested"],
        serde_json::json!(true),
        "{memorized}"
    );
    assert_eq!(
        memorized["light_evidence_resolved"],
        serde_json::json!(2),
        "both claims must be grounded in real code: {memorized}"
    );
    assert_eq!(
        memorized["light_evidence_unresolved"],
        serde_json::json!(0),
        "{memorized}"
    );
    let memory_path = memorized["path"]
        .as_str()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| panic!("memorize must return its managed path: {memorized}"));
    let canonical_memory_store = runtime
        .join("agent-memory")
        .canonicalize()
        .expect("owned memory store must exist");
    assert_eq!(
        memory_path.parent(),
        Some(canonical_memory_store.as_path()),
        "memorize must use this state's owned memory store"
    );
    let memory_bytes = std::fs::read_to_string(&memory_path).expect("read authored memory");
    assert!(memory_bytes.contains(memory_label), "{memory_bytes}");
    assert!(memory_bytes.contains(second_label), "{memory_bytes}");
    assert!(memory_bytes.contains(claim_text), "{memory_bytes}");
    assert!(
        memory_bytes.contains("[𝔻 evidence: src/lib.rs]"),
        "{memory_bytes}"
    );
    assert_memory_retrievable(&mut first, memory_label, claim_text);
    first.shutdown();

    let exact_roots = vec![
        canonical_text(&workspace),
        canonical_text(&runtime.join("agent-memory")),
    ];
    assert_eq!(
        persisted_roots(&runtime),
        exact_roots,
        "the persisted identity must contain exactly one real code root and its owned memory store"
    );

    let mut restarted = Owner::spawn(&workspace, &runtime);
    assert_memory_retrievable(&mut restarted, memory_label, claim_text);
    assert_eq!(persisted_roots(&runtime), exact_roots);

    write_memory_fixture(&workspace, refreshed_symbol, 84);
    let refreshed = restarted.call(
        "ingest",
        serde_json::json!({
            "agent_id": AGENT,
            "path": canonical_text(&workspace),
            "mode": "refresh",
            "adapter": "code"
        }),
    );
    assert_eq!(refreshed["ok"], serde_json::json!(true), "{refreshed}");
    assert_eq!(
        refreshed["root_set_unchanged"],
        serde_json::json!(true),
        "{refreshed}"
    );
    assert_memory_retrievable(&mut restarted, memory_label, claim_text);
    assert_eq!(persisted_roots(&runtime), exact_roots);
    restarted.shutdown();

    let mut after_refresh_restart = Owner::spawn(&workspace, &runtime);
    let refreshed_code = after_refresh_restart.call(
        "search",
        serde_json::json!({
            "agent_id": AGENT,
            "query": refreshed_symbol,
            "mode": "literal"
        }),
    );
    assert!(
        refreshed_code["results"]
            .as_array()
            .is_some_and(|results| !results.is_empty()),
        "refreshed code must survive the next restart: {refreshed_code}"
    );
    assert_memory_retrievable(&mut after_refresh_restart, memory_label, claim_text);
    assert_eq!(persisted_roots(&runtime), exact_roots);
    after_refresh_restart.shutdown();

    let graph_before_refusal = std::fs::read(runtime.join("graph_snapshot.json"))
        .expect("snapshot before foreign grant refusal");
    let memory_before_refusal = std::fs::read(&memory_path).expect("memory before refusal");
    let source_before_refusal = source_snapshot(&workspace);
    let refused = attempt_start(&foreign, &runtime, &foreign);
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(!refused.status.success(), "foreign grant must refuse");
    assert!(
        stderr.contains("launcher_workspace_conflicts_with_bound_graph"),
        "memory-bearing runtime must remain bound to its real code root: {stderr}"
    );
    assert_eq!(persisted_roots(&runtime), exact_roots);
    assert_eq!(
        std::fs::read(runtime.join("graph_snapshot.json")).unwrap(),
        graph_before_refusal,
        "foreign refusal must preserve the graph"
    );
    assert_eq!(
        std::fs::read(&memory_path).unwrap(),
        memory_before_refusal,
        "foreign refusal must preserve authored memory"
    );
    assert_eq!(
        source_snapshot(&workspace),
        source_before_refusal,
        "foreign refusal must preserve the granted source"
    );
}

#[test]
fn only_the_exact_owned_nonsymlink_memory_store_is_auxiliary_identity() {
    let temp = tempfile::tempdir().expect("tempdir");
    let granted = temp.path().join("repo-alpha");
    let source_runtime = temp.path().join("source-runtime");
    write_fixture(&granted, UNIQUE_SYMBOL);
    let mut source = Owner::spawn(&granted, &source_runtime);
    assert_symbol_is_retrievable(&mut source, &source_runtime);
    source.shutdown();
    let source_graph =
        std::fs::read(source_runtime.join("graph_snapshot.json")).expect("source graph");
    let source_before_refusals = source_snapshot(&granted);

    let cases = [
        "foreign-directory-named-agent-memory",
        "symlinked-owned-store-escape",
        "sidecar-only-identity",
    ];
    for case in cases {
        let runtime = temp.path().join(case).join("runtime");
        std::fs::create_dir_all(&runtime).expect("case runtime");
        std::fs::write(runtime.join("graph_snapshot.json"), &source_graph).expect("case graph");
        let owned_store = runtime.join("agent-memory");
        let roots = match case {
            "foreign-directory-named-agent-memory" => {
                let foreign_store = temp.path().join(case).join("foreign/agent-memory");
                std::fs::create_dir_all(&foreign_store).expect("foreign named store");
                vec![canonical_text(&granted), canonical_text(&foreign_store)]
            }
            "symlinked-owned-store-escape" => {
                let escaped_store = temp.path().join(case).join("escaped-store");
                std::fs::create_dir_all(&escaped_store).expect("escaped store");
                symlink(&escaped_store, &owned_store).expect("symlink owned store outside runtime");
                vec![
                    canonical_text(&granted),
                    owned_store.to_string_lossy().to_string(),
                ]
            }
            "sidecar-only-identity" => {
                std::fs::create_dir_all(&owned_store).expect("owned store");
                vec![canonical_text(&owned_store)]
            }
            _ => unreachable!(),
        };
        std::fs::write(
            runtime.join("ingest_roots.json"),
            serde_json::to_vec(&roots).expect("encode roots"),
        )
        .expect("write roots");
        let graph_before = std::fs::read(runtime.join("graph_snapshot.json")).unwrap();

        let refused = attempt_start(&granted, &runtime, &granted);
        let stderr = String::from_utf8_lossy(&refused.stderr);
        assert!(!refused.status.success(), "{case} must refuse");
        assert!(
            stderr.contains("launcher_workspace_conflicts_with_bound_graph"),
            "{case} must fail closed at launcher identity: {stderr}"
        );
        assert_eq!(persisted_roots(&runtime), roots, "{case} changed roots");
        assert_eq!(
            std::fs::read(runtime.join("graph_snapshot.json")).unwrap(),
            graph_before,
            "{case} changed graph"
        );
        assert_eq!(
            source_snapshot(&granted),
            source_before_refusals,
            "{case} changed source"
        );
    }
}

#[test]
fn prompt_or_tool_text_cannot_authorize_a_sibling_workspace() {
    let temp = tempfile::tempdir().expect("tempdir");
    let granted = temp.path().join("repo-alpha");
    let sibling = temp.path().join("repo-beta");
    let runtime = temp.path().join("private-runtime");
    write_fixture(&granted, UNIQUE_SYMBOL);
    write_fixture(&sibling, "sibling_only_signal_7c2a");
    std::fs::create_dir_all(&runtime).expect("create external runtime");

    let mut owner = Owner::spawn(&granted, &runtime);
    let north = owner.call(
        "north",
        serde_json::json!({
            "agent_id": AGENT,
            "task": format!("inspect {} sibling_only_signal_7c2a", sibling.display())
        }),
    );
    let north_code = serde_json::to_string(&north["code"]).expect("encode north code");
    assert!(
        !north_code.contains("sibling_only_signal_7c2a"),
        "prompt text must not extend the launcher's workspace grant: {north}"
    );
    let sibling_search = owner.call(
        "search",
        serde_json::json!({
            "agent_id": AGENT,
            "query": "sibling_only_signal_7c2a",
            "scope": sibling
        }),
    );
    assert!(
        !serde_json::to_string(&sibling_search)
            .expect("encode sibling search")
            .contains("file::src/lib.rs::fn::sibling_only_signal_7c2a"),
        "tool scope/path text must not ingest the sibling: {sibling_search}"
    );
    assert_symbol_is_retrievable(&mut owner, &runtime);
    owner.shutdown();
    assert!(!sibling.join(".m1nd").exists());
}

#[test]
fn read_only_source_is_retrievable_with_external_runtime() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path().join("repo-read-only");
    let runtime = temp.path().join("private-runtime");
    write_fixture(&workspace, UNIQUE_SYMBOL);
    std::fs::create_dir_all(&runtime).expect("create external runtime");
    let before = source_snapshot(&workspace);

    std::fs::set_permissions(
        workspace.join("Cargo.toml"),
        std::fs::Permissions::from_mode(0o444),
    )
    .expect("make manifest read-only");
    std::fs::set_permissions(
        workspace.join("src/lib.rs"),
        std::fs::Permissions::from_mode(0o444),
    )
    .expect("make source read-only");
    std::fs::set_permissions(
        workspace.join("src"),
        std::fs::Permissions::from_mode(0o555),
    )
    .expect("make src dir read-only");
    std::fs::set_permissions(&workspace, std::fs::Permissions::from_mode(0o555))
        .expect("make workspace read-only");

    let mut owner = Owner::spawn(&workspace, &runtime);
    assert!(assert_symbol_is_retrievable(&mut owner, &runtime).node_count > 0);
    owner.shutdown();
    assert_eq!(source_snapshot(&workspace), before);
    assert!(!workspace.join(".m1nd").exists());

    std::fs::set_permissions(&workspace, std::fs::Permissions::from_mode(0o755))
        .expect("restore workspace permissions");
    std::fs::set_permissions(
        workspace.join("src"),
        std::fs::Permissions::from_mode(0o755),
    )
    .expect("restore src permissions");
}

#[test]
fn populated_runtime_refuses_a_different_launcher_root_without_replacement() {
    let temp = tempfile::tempdir().expect("tempdir");
    let original = temp.path().join("repo-alpha");
    let conflicting = temp.path().join("repo-beta");
    let runtime = temp.path().join("private-runtime");
    write_fixture(&original, UNIQUE_SYMBOL);
    write_fixture(&conflicting, "conflicting_signal_1b6e");
    std::fs::create_dir_all(&runtime).expect("create external runtime");

    let mut first = Owner::spawn(&original, &runtime);
    let original_count = assert_symbol_is_retrievable(&mut first, &runtime);
    first.shutdown();

    let refused = attempt_start(&conflicting, &runtime, &conflicting);
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        stderr.contains("launcher_workspace_conflicts_with_bound_graph"),
        "conflicting launch must fail with the structured refusal code: {stderr}"
    );
    assert!(
        !refused.status.success(),
        "startup refusal must exit nonzero"
    );
    assert!(
        !String::from_utf8_lossy(&refused.stdout).contains("\"result\""),
        "a conflicting owner must not answer initialize as ready"
    );

    let mut original_again = Owner::spawn(&original, &runtime);
    assert_eq!(
        assert_symbol_is_retrievable(&mut original_again, &runtime),
        original_count,
        "the refusal must leave the original brain unchanged"
    );
    let conflict_search = original_again.call(
        "search",
        serde_json::json!({ "agent_id": AGENT, "query": "conflicting_signal_1b6e" }),
    );
    assert!(
        !serde_json::to_string(&conflict_search)
            .expect("encode conflict search")
            .contains("file::src/lib.rs::fn::conflicting_signal_1b6e"),
        "the conflicting root must not replace or merge into the bound graph"
    );
    original_again.shutdown();
}

#[test]
fn populated_mixed_persisted_roots_refuse_grant_and_preserve_identity() {
    let temp = tempfile::tempdir().expect("tempdir");
    let granted = temp.path().join("repo-alpha");
    let foreign = temp.path().join("repo-beta");
    let source_runtime = temp.path().join("source-runtime");
    let runtime = temp.path().join("m1nd-runtimes/owner");
    write_fixture(&granted, "alpha_only_marker_5b7a");
    write_fixture(&foreign, "beta_only_marker_0f42");
    std::fs::create_dir_all(&source_runtime).expect("source runtime");
    std::fs::create_dir_all(&runtime).expect("runtime");

    let mut beta = Owner::spawn(&foreign, &source_runtime);
    let beta_search = beta.call(
        "search",
        serde_json::json!({ "agent_id": AGENT, "query": "beta_only_marker_0f42" }),
    );
    assert!(serde_json::to_string(&beta_search)
        .expect("encode beta search")
        .contains("file::src/lib.rs::fn::beta_only_marker_0f42"));
    beta.shutdown();
    std::fs::copy(
        source_runtime.join("graph_snapshot.json"),
        runtime.join("graph_snapshot.json"),
    )
    .expect("copy populated graph without checkpoint");
    let persisted_roots = serde_json::to_vec(&vec![
        std::fs::canonicalize(&granted)
            .expect("canonical granted root")
            .to_string_lossy()
            .to_string(),
        std::fs::canonicalize(&foreign)
            .expect("canonical foreign root")
            .to_string_lossy()
            .to_string(),
    ])
    .expect("encode mixed roots");
    std::fs::write(runtime.join("ingest_roots.json"), &persisted_roots)
        .expect("write mixed persisted roots");
    let graph_before = std::fs::read(runtime.join("graph_snapshot.json")).unwrap();

    let refused = attempt_start(&granted, &runtime, &granted);
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(!refused.status.success(), "mixed identity must refuse");
    assert!(
        stderr.contains("launcher_workspace_conflicts_with_bound_graph"),
        "mixed populated identity must fail closed: {stderr}"
    );
    assert!(!String::from_utf8_lossy(&refused.stdout).contains("\"result\""));
    assert_eq!(
        std::fs::read(runtime.join("graph_snapshot.json")).unwrap(),
        graph_before,
        "refusal must preserve the populated foreign snapshot"
    );
    let roots_after: Vec<String> =
        serde_json::from_slice(&std::fs::read(runtime.join("ingest_roots.json")).unwrap())
            .expect("decode roots after refusal");
    let roots_before: Vec<String> =
        serde_json::from_slice(&persisted_roots).expect("decode fixture roots");
    assert_eq!(
        roots_after, roots_before,
        "refusal must preserve the complete logical root identity"
    );
}

#[test]
fn empty_mixed_persisted_roots_refuse_grant_without_replacement() {
    let temp = tempfile::tempdir().expect("tempdir");
    let baseline_root = temp.path().join("empty-baseline");
    let granted = temp.path().join("repo-alpha");
    let foreign = temp.path().join("repo-beta");
    let source_runtime = temp.path().join("source-runtime");
    let runtime = temp.path().join("m1nd-runtimes/owner");
    std::fs::create_dir_all(&baseline_root).expect("baseline root");
    write_fixture(&granted, "alpha_replace_risk_81e2");
    write_fixture(&foreign, "beta_identity_marker_72bc");
    std::fs::create_dir_all(&source_runtime).expect("source runtime");
    std::fs::create_dir_all(&runtime).expect("runtime");

    let baseline = establish_empty_baseline(&baseline_root, &source_runtime);
    assert!(baseline.status.success(), "empty baseline must start");
    std::fs::copy(
        source_runtime.join("graph_snapshot.json"),
        runtime.join("graph_snapshot.json"),
    )
    .expect("copy empty graph without checkpoint");
    let persisted_roots = serde_json::to_vec(&vec![
        std::fs::canonicalize(&granted)
            .expect("canonical granted root")
            .to_string_lossy()
            .to_string(),
        std::fs::canonicalize(&foreign)
            .expect("canonical foreign root")
            .to_string_lossy()
            .to_string(),
    ])
    .expect("encode mixed roots");
    std::fs::write(runtime.join("ingest_roots.json"), &persisted_roots)
        .expect("write mixed persisted roots");
    let graph_before = std::fs::read(runtime.join("graph_snapshot.json")).unwrap();

    let refused = attempt_start(&granted, &runtime, &granted);
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(!refused.status.success(), "mixed identity must refuse");
    assert!(
        stderr.contains("launcher_workspace_conflicts_with_bound_graph"),
        "mixed empty identity must fail closed before replace: {stderr}"
    );
    assert!(!String::from_utf8_lossy(&refused.stdout).contains("\"result\""));
    assert_eq!(
        std::fs::read(runtime.join("graph_snapshot.json")).unwrap(),
        graph_before,
        "refusal must preserve the empty snapshot"
    );
    let roots_after: Vec<String> =
        serde_json::from_slice(&std::fs::read(runtime.join("ingest_roots.json")).unwrap())
            .expect("decode roots after refusal");
    let roots_before: Vec<String> =
        serde_json::from_slice(&persisted_roots).expect("decode fixture roots");
    assert_eq!(
        roots_after, roots_before,
        "refusal must not shrink the logical root identity"
    );
}

#[test]
fn persisted_roots_require_every_entry_to_resolve_and_allow_duplicates() {
    let temp = tempfile::tempdir().expect("tempdir");
    let granted = temp.path().join("repo-alpha");
    let source_runtime = temp.path().join("source-runtime");
    let duplicate_runtime = temp.path().join("duplicate-runtime");
    let unresolved_runtime = temp.path().join("unresolved-runtime");
    write_fixture(&granted, UNIQUE_SYMBOL);
    std::fs::create_dir_all(&source_runtime).expect("source runtime");
    std::fs::create_dir_all(&duplicate_runtime).expect("duplicate runtime");
    std::fs::create_dir_all(&unresolved_runtime).expect("unresolved runtime");

    let mut first = Owner::spawn(&granted, &source_runtime);
    let identity = assert_symbol_is_retrievable(&mut first, &source_runtime);
    first.shutdown();
    let canonical_grant = std::fs::canonicalize(&granted)
        .expect("canonical granted root")
        .to_string_lossy()
        .to_string();
    let duplicate_roots = serde_json::to_vec(&vec![canonical_grant.clone(), canonical_grant])
        .expect("encode duplicate roots");
    std::fs::copy(
        source_runtime.join("graph_snapshot.json"),
        duplicate_runtime.join("graph_snapshot.json"),
    )
    .expect("copy graph for duplicate roots");
    std::fs::write(
        duplicate_runtime.join("ingest_roots.json"),
        &duplicate_roots,
    )
    .expect("write duplicate roots");
    let mut duplicate = Owner::spawn(&granted, &duplicate_runtime);
    assert_eq!(
        assert_symbol_is_retrievable(&mut duplicate, &duplicate_runtime).node_count,
        identity.node_count,
        "duplicate valid roots remain the same exact identity"
    );
    duplicate.shutdown();

    let unresolved_roots = serde_json::to_vec(&vec![
        std::fs::canonicalize(&granted)
            .expect("canonical granted root")
            .to_string_lossy()
            .to_string(),
        temp.path()
            .join("missing-root")
            .to_string_lossy()
            .to_string(),
    ])
    .expect("encode unresolved roots");
    std::fs::copy(
        source_runtime.join("graph_snapshot.json"),
        unresolved_runtime.join("graph_snapshot.json"),
    )
    .expect("copy graph for unresolved roots");
    std::fs::write(
        unresolved_runtime.join("ingest_roots.json"),
        &unresolved_roots,
    )
    .expect("write unresolved roots");
    let graph_before = std::fs::read(unresolved_runtime.join("graph_snapshot.json")).unwrap();
    let refused = attempt_start(&granted, &unresolved_runtime, &granted);
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        !refused.status.success(),
        "unresolvable extra root must refuse"
    );
    assert!(
        stderr.contains("launcher_workspace_conflicts_with_bound_graph"),
        "unresolvable identity must fail closed: {stderr}"
    );
    assert!(!String::from_utf8_lossy(&refused.stdout).contains("\"result\""));
    assert_eq!(
        std::fs::read(unresolved_runtime.join("graph_snapshot.json")).unwrap(),
        graph_before
    );
    let roots_after: Vec<String> = serde_json::from_slice(
        &std::fs::read(unresolved_runtime.join("ingest_roots.json")).unwrap(),
    )
    .expect("decode unresolved roots after refusal");
    let roots_before: Vec<String> =
        serde_json::from_slice(&unresolved_roots).expect("decode unresolved fixture roots");
    assert_eq!(roots_after, roots_before);
}

#[test]
fn populated_snapshot_without_persisted_identity_refuses_a_new_grant() {
    let temp = tempfile::tempdir().expect("tempdir");
    let original = temp.path().join("repo-alpha");
    let conflicting = temp.path().join("repo-beta");
    let source_runtime = temp.path().join("source-runtime");
    let legacy_runtime = temp.path().join("m1nd-runtimes/owner");
    write_fixture(&original, "alpha_only_marker_4d1c");
    write_fixture(&conflicting, "beta_only_marker_88e3");
    std::fs::create_dir_all(&source_runtime).expect("source runtime");
    std::fs::create_dir_all(&legacy_runtime).expect("legacy runtime");

    let mut alpha = Owner::spawn(&original, &source_runtime);
    let _ = alpha.call(
        "search",
        serde_json::json!({ "agent_id": AGENT, "query": "alpha_only_marker_4d1c" }),
    );
    alpha.shutdown();
    std::fs::copy(
        source_runtime.join("graph_snapshot.json"),
        legacy_runtime.join("graph_snapshot.json"),
    )
    .expect("copy legacy graph without identity sidecars");
    let graph_before = std::fs::read(legacy_runtime.join("graph_snapshot.json"))
        .expect("read copied legacy graph");

    let refused = attempt_start(&conflicting, &legacy_runtime, &conflicting);
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        !refused.status.success(),
        "unbound populated graph must refuse"
    );
    assert!(
        stderr.contains("launcher_workspace_conflicts_with_bound_graph"),
        "populated graph without persisted identity must fail closed: {stderr}"
    );
    assert!(
        !String::from_utf8_lossy(&refused.stdout).contains("\"result\""),
        "refused runtime must not answer initialize"
    );
    assert_eq!(
        std::fs::read(legacy_runtime.join("graph_snapshot.json")).unwrap(),
        graph_before,
        "refusal must not replace the populated snapshot"
    );
}

#[test]
fn empty_snapshot_with_incompatible_persisted_identity_is_not_reassigned() {
    let temp = tempfile::tempdir().expect("tempdir");
    let original = temp.path().join("repo-alpha");
    let conflicting = temp.path().join("repo-beta");
    let source_runtime = temp.path().join("source-runtime");
    let legacy_runtime = temp.path().join("m1nd-runtimes/owner");
    std::fs::create_dir_all(&original).expect("original root");
    std::fs::create_dir_all(&conflicting).expect("conflicting root");
    std::fs::create_dir_all(&source_runtime).expect("source runtime");
    std::fs::create_dir_all(&legacy_runtime).expect("legacy runtime");

    let baseline = establish_empty_baseline(&original, &source_runtime);
    assert!(
        baseline.status.success(),
        "empty source baseline must start"
    );
    std::fs::copy(
        source_runtime.join("graph_snapshot.json"),
        legacy_runtime.join("graph_snapshot.json"),
    )
    .expect("copy empty legacy graph");
    let persisted_roots = serde_json::to_vec(&vec![std::fs::canonicalize(&original)
        .expect("canonical original")
        .to_string_lossy()
        .to_string()])
    .expect("encode roots");
    std::fs::write(legacy_runtime.join("ingest_roots.json"), &persisted_roots)
        .expect("write persisted identity");
    let graph_before = std::fs::read(legacy_runtime.join("graph_snapshot.json")).unwrap();

    let refused = attempt_start(&conflicting, &legacy_runtime, &conflicting);
    let stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        !refused.status.success(),
        "incompatible identity must refuse"
    );
    assert!(
        stderr.contains("launcher_workspace_conflicts_with_bound_graph"),
        "empty graph must not erase persisted identity: {stderr}"
    );
    assert!(!String::from_utf8_lossy(&refused.stdout).contains("\"result\""));
    assert_eq!(
        std::fs::read(legacy_runtime.join("graph_snapshot.json")).unwrap(),
        graph_before
    );
    let roots_after: Vec<String> =
        serde_json::from_slice(&std::fs::read(legacy_runtime.join("ingest_roots.json")).unwrap())
            .expect("decode roots after refusal");
    let roots_before: Vec<String> =
        serde_json::from_slice(&persisted_roots).expect("decode fixture roots");
    assert_eq!(roots_after, roots_before);
}

#[test]
fn invalid_and_empty_launcher_roots_exit_nonzero_and_empty_ingest_rolls_back() {
    let temp = tempfile::tempdir().expect("tempdir");
    let current_dir = temp.path().join("current");
    let empty = temp.path().join("empty-repo");
    let invalid_runtime = temp.path().join("invalid-runtime");
    let runtime = temp.path().join("m1nd-runtimes/owner");
    std::fs::create_dir_all(&current_dir).expect("current dir");
    std::fs::create_dir_all(&empty).expect("empty repo");
    std::fs::create_dir_all(&invalid_runtime).expect("invalid runtime");
    std::fs::create_dir_all(&runtime).expect("runtime");

    let missing = attempt_start(&current_dir, &invalid_runtime, &temp.path().join("missing"));
    assert!(
        !missing.status.success(),
        "invalid root refusal must exit nonzero"
    );
    assert!(String::from_utf8_lossy(&missing.stderr).contains("launcher_workspace_unresolvable"));

    let baseline = establish_empty_baseline(&empty, &runtime);
    assert!(
        baseline.status.success(),
        "empty baseline must start cleanly"
    );
    assert!(String::from_utf8_lossy(&baseline.stdout).contains("\"result\""));
    let baseline_roots = std::fs::read(runtime.join("ingest_roots.json")).ok();
    let baseline_graph = std::fs::read(runtime.join("graph_snapshot.json")).ok();
    let refused = attempt_start(&empty, &runtime, &empty);
    assert!(
        !refused.status.success(),
        "empty ingest refusal must exit nonzero"
    );
    let refused_stderr = String::from_utf8_lossy(&refused.stderr);
    assert!(
        refused_stderr.contains("launcher_workspace_produced_empty_graph"),
        "empty ingest must return its domain refusal: {refused_stderr}"
    );
    assert_eq!(
        std::fs::read(runtime.join("ingest_roots.json")).ok(),
        baseline_roots,
        "rejected empty ingest must not commit a new root"
    );
    assert_eq!(
        std::fs::read(runtime.join("graph_snapshot.json")).ok(),
        baseline_graph,
        "rejected empty ingest must not commit a new graph"
    );

    write_fixture(&empty, UNIQUE_SYMBOL);
    let mut recovered = Owner::spawn(&empty, &runtime);
    assert_symbol_is_retrievable(&mut recovered, &runtime);
    recovered.shutdown();
}

#[test]
fn hostile_persistence_overrides_are_ignored_and_sentinels_stay_intact() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = temp.path().join("repo-alpha");
    let runtime = temp.path().join("private-runtime");
    let hostile_graph = temp.path().join("hostile-graph.json");
    let hostile_plasticity = temp.path().join("hostile-plasticity.json");
    write_fixture(&workspace, UNIQUE_SYMBOL);
    std::fs::create_dir_all(&runtime).expect("runtime");
    std::fs::write(&hostile_graph, b"hostile graph sentinel\n").expect("graph sentinel");
    std::fs::write(&hostile_plasticity, b"hostile plasticity sentinel\n")
        .expect("plasticity sentinel");
    std::fs::set_permissions(&hostile_graph, std::fs::Permissions::from_mode(0o444))
        .expect("graph sentinel mode");
    std::fs::set_permissions(&hostile_plasticity, std::fs::Permissions::from_mode(0o444))
        .expect("plasticity sentinel mode");
    let graph_before = std::fs::read(&hostile_graph).expect("graph before");
    let plasticity_before = std::fs::read(&hostile_plasticity).expect("plasticity before");

    let mut owner = Owner::spawn_with_overrides(
        &workspace,
        &runtime,
        &workspace,
        &[
            ("M1ND_GRAPH_SOURCE", &hostile_graph),
            ("GRAPH_SNAPSHOT_PATH", &hostile_graph),
            ("M1ND_PLASTICITY_STATE", &hostile_plasticity),
            ("PLASTICITY_STATE_PATH", &hostile_plasticity),
        ],
    );
    assert_symbol_is_retrievable(&mut owner, &runtime);
    owner.shutdown();

    assert_eq!(std::fs::read(&hostile_graph).unwrap(), graph_before);
    assert_eq!(
        std::fs::read(&hostile_plasticity).unwrap(),
        plasticity_before
    );
    assert_eq!(
        std::fs::metadata(&hostile_graph)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o444
    );
    assert_eq!(
        std::fs::metadata(&hostile_plasticity)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o444
    );
}
