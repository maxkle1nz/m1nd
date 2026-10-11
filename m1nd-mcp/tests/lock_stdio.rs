//! Public change-tracking path: discovery, launcher-bound refresh, observation
//! lifecycle and authority refusals. No internal dispatch or human-origin stamp.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

struct Owner {
    child: Child,
    stdin: Option<ChildStdin>,
    replies: Receiver<Value>,
    next_id: u64,
}
impl Owner {
    fn spawn(root: &Path, runtime: &Path, tier: Option<&str>) -> Self {
        // Existing empty model directory disables embeddings without triggering a download.
        std::fs::create_dir_all(runtime.join("no-model-needed")).unwrap();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_m1nd-mcp"));
        cmd.args(["--stdio", "--no-gui", "--runtime-dir"])
            .arg(runtime)
            .current_dir(runtime)
            .env("M1ND_WORKSPACE_ROOT", root)
            .env("M1ND_RUNTIME_DIR", runtime)
            .env("M1ND_RUNTIME_BASE", runtime)
            .env("M1ND_GRAPH_SOURCE", runtime.join("graph_snapshot.json"))
            .env(
                "M1ND_PLASTICITY_STATE",
                runtime.join("plasticity_state.json"),
            )
            .env("M1ND_REGISTRY_DIR", runtime.join("registry"))
            .env("M1ND_EMBED_MODEL", runtime.join("no-model-needed"))
            .env_remove("M1ND_TOOL_TIER")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(tier) = tier {
            cmd.env("M1ND_TOOL_TIER", tier);
        }
        let mut child = cmd.spawn().unwrap();
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (sender, replies) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(value) = serde_json::from_str(&line) {
                    if sender.send(value).is_err() {
                        break;
                    }
                }
            }
        });
        let mut owner = Self {
            child,
            stdin,
            replies,
            next_id: 0,
        };
        let init=owner.request("initialize",json!({"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"lock-public-proof","version":"1"}}));
        assert!(init["result"]["instructions"].is_string(), "{init}");
        owner
    }
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        writeln!(
            self.stdin.as_mut().unwrap(),
            "{}",
            json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
        )
        .unwrap();
        self.stdin.as_mut().unwrap().flush().unwrap();
        let deadline = Instant::now() + Duration::from_secs(180);
        loop {
            let response = self
                .replies
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("stdio response deadline");
            if response["id"] == id {
                return response;
            }
        }
    }
    fn tool(&mut self, name: &str, mut args: Value) -> Value {
        if args.get("agent_id").is_none() {
            args["agent_id"] = json!("audit");
        }
        self.request("tools/call", json!({"name":name,"arguments":args}))
    }
    fn ok(&mut self, name: &str, args: Value) -> Value {
        let response = self.tool(name, args);
        assert!(response.get("error").is_none(), "{response}");
        assert_ne!(response["result"]["isError"], true, "{response}");
        response["result"]
            .get("structuredContent")
            .cloned()
            .unwrap_or_else(|| {
                serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap())
                    .unwrap()
            })
    }
    fn refused(&mut self, name: &str, args: Value, reason: &str) {
        let response = self.tool(name, args);
        assert_eq!(response["result"]["isError"], true, "{response}");
        assert!(response.to_string().contains(reason), "{response}");
    }
    fn shutdown(mut self) {
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "{status}");
                break;
            }
            assert!(Instant::now() < deadline, "owner did not stop");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn private_dir(path: &Path) {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path).unwrap();
}
fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn public_lock_discovery_refresh_lifecycle_and_refusals() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    private_dir(&root);
    let runtime = temp.path().join("runtime");
    private_dir(&runtime);
    let source =
        "def alpha():\n    return 1\n\ndef beta():\n    return 2\n\ndef gamma():\n    return 3\n";
    std::fs::write(root.join("app.py"), source).unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "."]);
    git(
        &root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    let mut owner = Owner::spawn(&root, &runtime, None);
    let menu = owner.request("tools/list", json!({}));
    assert_eq!(menu["result"]["tools"].as_array().unwrap().len(), 15);
    assert!(!menu["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .any(|tool| tool["name"] == "lock_create"));
    owner.shutdown();
    let mut owner = Owner::spawn(&root, &runtime, Some("investigate"));
    let menu = owner.request("tools/list", json!({}));
    let tools = menu["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 20);
    for name in [
        "lock_create",
        "lock_watch",
        "lock_diff",
        "lock_rebase",
        "lock_release",
    ] {
        assert!(tools
            .iter()
            .any(|tool| tool["name"] == name && tool["inputSchema"]["type"] == "object"));
    }
    owner.ok("help", json!({"intent":"track changes in the graph"}));
    let baseline = owner.ok(
        "lock_create",
        json!({"scope":"subgraph","root_nodes":["app.py"],"radius":1}),
    );
    assert_eq!(baseline["baseline_nodes"], 4);
    assert_eq!(baseline["baseline_edges"], 3);
    let lock = baseline["lock_id"].as_str().unwrap();
    let boundary = owner.ok(
        "lock_create",
        json!({"scope":"node","root_nodes":["file::app.py"]}),
    );
    let boundary_id = boundary["lock_id"].as_str().unwrap();
    owner.ok("lock_watch", json!({"lock_id":lock,"strategy":"on_ingest"}));
    for name in ["lock_diff", "lock_watch", "lock_rebase", "lock_release"] {
        owner.refused(
            name,
            json!({"agent_id":"other","lock_id":lock,"strategy":"manual"}),
            "ownership",
        );
    }
    owner.refused(
        "lock_create",
        json!({"scope":"query_neighborhood","root_nodes":["file::app.py"],"query":"alpha"}),
        "not supported",
    );
    owner.refused(
        "lock_create",
        json!({"scope":"node","root_nodes":[]}),
        "root_nodes",
    );
    owner.refused(
        "lock_watch",
        json!({"lock_id":lock,"strategy":"periodic"}),
        "not supported",
    );
    std::fs::write(
        root.join("app.py"),
        format!("{source}\ndef delta():\n    return 4\n"),
    )
    .unwrap();
    let refresh = owner.ok("ingest", json!({"mode":"refresh","path":root}));
    assert_eq!(refresh["ok"], true);
    let diff = owner.ok("lock_diff", json!({"lock_id":lock}));
    assert_eq!(diff["diff"]["no_changes"], false);
    assert_eq!(diff["watcher_events_drained"], 1);
    assert_eq!(
        diff["diff"]["new_nodes"],
        json!(["file::app.py::fn::delta"])
    );
    assert_eq!(diff["diff"]["new_edges"].as_array().unwrap().len(), 1);
    let d = owner.ok("lock_diff", json!({"lock_id":boundary_id}));
    assert_eq!(
        d["diff"]["boundary_edges_added"].as_array().unwrap().len(),
        1
    );
    assert_eq!(d["diff"]["no_changes"], false);
    owner.ok("lock_rebase", json!({"lock_id":lock}));
    assert_eq!(
        owner.ok("lock_diff", json!({"lock_id":lock}))["diff"]["no_changes"],
        true
    );
    let saved = owner.ok("trail_save", json!({"label":"public boundary proof"}));
    owner.refused(
        "trail_resume",
        json!({"trail_id":saved["trail_id"]}),
        "generic_action_authority_required",
    );
    owner.refused(
        "trail_merge",
        json!({"trail_ids":[saved["trail_id"],saved["trail_id"]]}),
        "generic_action_authority_required",
    );
    std::fs::write(root.join("app.py"), "# no code remains\n").unwrap();
    let rejected = owner.ok("ingest", json!({"mode":"refresh","path":root}));
    assert_eq!(rejected["ok"], false);
    assert_eq!(rejected["refused"], "refresh_would_shrink_graph");
    let unchanged = owner.ok("lock_diff", json!({"lock_id":lock}));
    assert_eq!(unchanged["diff"]["no_changes"], true);
    assert_eq!(unchanged["watcher_events_drained"], 0);
    std::fs::write(
        root.join("app.py"),
        format!("{source}\ndef delta():\n    return 4\n"),
    )
    .unwrap();
    owner.refused(
        "ingest",
        json!({"mode":"replace","path":root}),
        "generic_action_authority_required",
    );
    assert_eq!(
        owner.ok("lock_diff", json!({"lock_id":lock}))["diff"]["no_changes"],
        true
    );
    owner.ok("lock_release", json!({"lock_id":lock}));
    owner.refused("lock_diff", json!({"lock_id":lock}), "not found");
    owner.shutdown();
    let mut owner = Owner::spawn(&root, &runtime, Some("investigate"));
    owner.refused("lock_diff", json!({"lock_id":boundary_id}), "not found");
    assert_eq!(owner.ok("trail_list", json!({}))["total_count"], 1);
    owner.shutdown();
}
