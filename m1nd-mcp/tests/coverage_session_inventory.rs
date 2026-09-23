//! Coverage must reflect the live workspace even before another audit populates
//! the optional in-memory inventory cache.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_m1nd-mcp");
const AGENT: &str = "coverage-inventory-probe";

struct Owner {
    child: Child,
    stdin: ChildStdin,
    stdout: Receiver<String>,
    next_id: u64,
}

impl Drop for Owner {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Owner {
    fn request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        self.next_id += 1;
        let id = self.next_id;
        writeln!(
            self.stdin,
            "{}",
            serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params,
            })
        )
        .expect("write MCP request");
        self.stdin.flush().expect("flush MCP request");

        loop {
            let line = self
                .stdout
                .recv_timeout(Duration::from_secs(30))
                .unwrap_or_else(|error| panic!("owner did not reply to request {id}: {error}"));
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if value.get("id").and_then(serde_json::Value::as_u64) == Some(id) {
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
            "tool {name} failed: {reply}"
        );
        reply["result"]
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
}

fn spawn_owner(workspace: &Path, runtime: &Path) -> Owner {
    use std::os::unix::fs::PermissionsExt;

    std::fs::create_dir_all(runtime).expect("fixture runtime");
    std::fs::set_permissions(runtime, std::fs::Permissions::from_mode(0o700))
        .expect("owner-private fixture runtime");
    let home = runtime.join("home");
    let temp = runtime.join("tmp");
    std::fs::create_dir_all(&home).expect("fixture home");
    std::fs::create_dir_all(&temp).expect("fixture temp");
    let disabled_embed_model = runtime.join("offline-empty-model");
    std::fs::create_dir_all(&disabled_embed_model).expect("empty embed-model sentinel");
    let mut command = Command::new(BIN);
    command
        .args(["--stdio", "--no-gui"])
        .current_dir(workspace)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("M1ND_WORKSPACE_ROOT", workspace)
        .env("M1ND_RUNTIME_DIR", runtime)
        .env("M1ND_REGISTRY_DIR", runtime.join("registry"))
        .env("M1ND_EMBED_MODEL", &disabled_embed_model)
        .env("M1ND_TEST_EMBED_MODEL", &disabled_embed_model)
        .env("M1ND_NO_GUI", "1")
        .env("HOME", home)
        .env("TMPDIR", &temp)
        .env("TMP", &temp)
        .env("TEMP", temp)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut child = command.spawn().expect("spawn isolated owner");
    let stdin = child.stdin.take().expect("owner stdin");
    let stdout = child.stdout.take().expect("owner stdout");
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut owner = Owner {
        child,
        stdin,
        stdout: rx,
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
        "initialize: {initialized}"
    );
    owner
}

#[test]
fn coverage_session_builds_inventory_for_a_fresh_workspace() {
    let temporary = tempfile::tempdir().expect("temporary fixture");
    let workspace = temporary.path().join("workspace");
    let runtime = temporary.path().join("runtime");
    std::fs::create_dir_all(workspace.join("src")).expect("fixture src");
    std::fs::write(
        workspace.join("Cargo.toml"),
        "[package]\nname = \"coverage-fixture\"\nversion = \"0.0.0\"\n",
    )
    .expect("fixture manifest");
    std::fs::write(
        workspace.join("src/lib.rs"),
        "pub fn covered() -> bool { true }\n",
    )
    .expect("fixture source");

    // The first owner bootstraps and persists a graph. The regression appears
    // only after a later owner loads that snapshot: the graph is populated but
    // the optional audit inventory cache starts empty.
    drop(spawn_owner(&workspace, &runtime));
    let mut owner = spawn_owner(&workspace, &runtime);
    let coverage = owner.call("coverage_session", serde_json::json!({ "agent_id": AGENT }));

    let total_files = coverage["total_files"]
        .as_u64()
        .unwrap_or_else(|| panic!("coverage total_files missing: {coverage}"));
    assert!(
        total_files >= 2,
        "fresh workspace files must be counted without a prior cross_verify call: {coverage}"
    );
    let unvisited = coverage["unvisited"]
        .as_array()
        .unwrap_or_else(|| panic!("coverage unvisited missing: {coverage}"));
    assert!(
        unvisited
            .iter()
            .filter_map(serde_json::Value::as_str)
            .any(|path| path.ends_with("Cargo.toml")),
        "manifest must appear as unvisited: {coverage}"
    );
    assert!(
        unvisited
            .iter()
            .filter_map(serde_json::Value::as_str)
            .any(|path| path.ends_with("src/lib.rs")),
        "source file must appear as unvisited: {coverage}"
    );
}
