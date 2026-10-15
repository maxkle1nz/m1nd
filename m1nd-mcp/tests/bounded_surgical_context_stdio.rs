//! Walk the real dispatcher/actor: a handler-only test cannot see checkpoint
//! rollback clearing process-owned coverage after a tool refusal.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::thread::JoinHandle;
use std::time::Duration;

struct Owner {
    child: Child,
    stdin: ChildStdin,
    replies: Receiver<String>,
    reader: Option<JoinHandle<()>>,
    id: u64,
}

impl Owner {
    fn spawn(repo: &Path, runtime: &Path) -> Self {
        std::fs::create_dir_all(runtime.join("no-model")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let mut command = Command::new(env!("CARGO_BIN_EXE_m1nd-mcp"));
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("M1ND_") {
                command.env_remove(key);
            }
        }
        let mut child = command
            .args(["--stdio", "--no-gui", "--runtime-dir"])
            .arg(runtime)
            .current_dir(runtime)
            .env("M1ND_WORKSPACE_ROOT", repo)
            .env("M1ND_REGISTRY_DIR", runtime.join("registry"))
            .env("M1ND_EMBED_MODEL", runtime.join("no-model"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, replies) = channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let mut owner = Self {
            child,
            stdin,
            replies,
            reader: Some(reader),
            id: 0,
        };
        let initialized = owner.request("initialize", json!({}));
        assert!(initialized.get("result").is_some(), "{initialized}");
        owner
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        writeln!(
            self.stdin,
            "{}",
            json!({"jsonrpc":"2.0","id":self.id,"method":method,"params":params})
        )
        .unwrap();
        self.stdin.flush().unwrap();
        loop {
            let line = self
                .replies
                .recv_timeout(Duration::from_secs(45))
                .expect("owner must answer");
            let reply: Value = serde_json::from_str(&line).unwrap();
            if reply["id"] == self.id {
                return reply;
            }
        }
    }

    fn call(&mut self, tool: &str, mut arguments: Value) -> Value {
        arguments["agent_id"] = json!("bounded-wire");
        self.request("tools/call", json!({"name":tool,"arguments":arguments}))
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn payload(reply: &Value) -> Value {
    serde_json::from_str(
        reply["result"]["content"][0]["text"]
            .as_str()
            .expect("text reply"),
    )
    .expect("JSON tool payload")
}

fn visits(owner: &mut Owner) -> Value {
    let snapshot = payload(&owner.call("coverage_session", json!({})));
    json!({"visited":snapshot["visited"],"visited_nodes":snapshot["visited_nodes"],"tools_used":snapshot["tools_used"]})
}

#[test]
fn bounded_refusals_preserve_previous_visits_through_real_stdio_actor() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo-alpha");
    let runtime = temp.path().join("private-runtime");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(
        repo.join("Cargo.toml"),
        "[package]\nname=\"repo-alpha\"\nversion=\"0.0.0\"\n",
    )
    .unwrap();
    let source = format!(
        "pub fn core() {{}}\n{}",
        "// quoted \\\" source 🧠\n".repeat(200)
    );
    std::fs::write(repo.join("src/lib.rs"), &source).unwrap();
    let mut owner = Owner::spawn(&repo, &runtime);
    let first = owner.call(
        "surgical_context",
        json!({"file_path":"src/lib.rs","max_output_bytes":1800}),
    );
    assert_ne!(first["result"]["isError"], json!(true), "{first}");
    let page = payload(&first);
    assert!(page["context_page"]["continuation"].is_object());
    let mut before = visits(&mut owner);
    assert!(before["visited"].as_u64().unwrap() > 0);
    assert!(before["visited_nodes"].as_u64().unwrap() > 0);
    let cursor = page["context_page"]["continuation"].clone();
    let requests = [
        json!({"file_path":"src/lib.rs","max_output_bytes":0}),
        json!({"file_path":"src/lib.rs","continuation":cursor}),
        json!({"file_path":"src/lib.rs","max_output_bytes":1800,"continuation":{"version":99,"context_sha256":"invalid","graph_generation":0,"plasticity_generation":0,"source_byte_offset":0,"item_offset":0}}),
        json!({"file_path":"src/lib.rs","max_output_bytes":-1}),
        json!({"file_path":temp.path().join("outside.rs"),"max_output_bytes":1800}),
    ];
    for request in requests {
        let refusal = owner.call("surgical_context", request);
        assert_eq!(refusal["result"]["isError"], json!(true), "{refusal}");
        assert_eq!(
            visits(&mut owner),
            before,
            "a refusal must restore the previous coverage, not an empty session"
        );
    }
    // A refusal must also retain the generations to which a valid cursor is bound.
    let recovered = owner.call(
        "surgical_context",
        json!({"file_path":"src/lib.rs","max_output_bytes":100_000,"continuation":cursor}),
    );
    assert_ne!(
        recovered["result"]["isError"],
        json!(true),
        "same valid cursor must recover with a larger budget: {recovered}"
    );
    before = visits(&mut owner);
    std::fs::write(repo.join("src/lib.rs"), format!("{source}// changed\n")).unwrap();
    let refusal = owner.call(
        "surgical_context",
        json!({"file_path":"src/lib.rs","max_output_bytes":1800,"continuation":cursor}),
    );
    assert_eq!(refusal["result"]["isError"], json!(true));
    assert_eq!(visits(&mut owner), before);
    let fresh = owner.call(
        "surgical_context",
        json!({"file_path":"src/lib.rs","max_output_bytes":1800}),
    );
    assert_ne!(
        fresh["result"]["isError"],
        json!(true),
        "later valid read must remain available"
    );
}

#[test]
fn oversized_metadata_refusal_can_resume_same_cursor_with_larger_budget() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo-alpha");
    let runtime = temp.path().join("private-runtime");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(
        repo.join("Cargo.toml"),
        "[package]\nname=\"repo-alpha\"\nversion=\"0.0.0\"\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("src/lib.rs"),
        format!("pub fn {}() {{}}\n", "function_name".repeat(300)),
    )
    .unwrap();
    let mut owner = Owner::spawn(&repo, &runtime);
    let mut cursor = Value::Null;
    let mut saw_refusal = false;
    for _ in 0..100 {
        let before = visits(&mut owner);
        let mut args = json!({"file_path":"src/lib.rs","max_output_bytes":1800});
        if !cursor.is_null() {
            args["continuation"] = cursor.clone();
        }
        let reply = owner.call("surgical_context", args);
        if reply["result"]["isError"] == json!(true) {
            assert_eq!(visits(&mut owner), before);
            assert!(reply["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("complete metadata item"));
            let resumed = owner.call(
                "surgical_context",
                json!({"file_path":"src/lib.rs","max_output_bytes":100_000,"continuation":cursor}),
            );
            assert_ne!(
                resumed["result"]["isError"],
                json!(true),
                "valid cursor must survive the refusal: {resumed}"
            );
            let final_page = payload(&resumed);
            assert!(final_page["file_contents"].as_str().unwrap().is_empty());
            assert!(
                final_page["context_page"]["source"]["byte_start"]
                    .as_u64()
                    .unwrap()
                    > 0
            );
            assert_reassembled_source_guidance(&final_page);
            saw_refusal = true;
            break;
        }
        cursor = payload(&reply)["context_page"]["continuation"].clone();
        assert!(
            !cursor.is_null(),
            "the oversized metadata cannot fit a small page"
        );
    }
    assert!(
        saw_refusal,
        "an indivisible oversized metadata item must refuse"
    );
}

fn assert_continuation_guidance(result: &Value) {
    assert!(result["context_page"]["continuation"].is_object());
    let suggestions = result["_m1nd"]["suggest_next"].as_array().unwrap();
    assert!(suggestions
        .iter()
        .any(|s| s.as_str().unwrap().contains("surgical_context")
            && s.as_str().unwrap().contains("continuation")));
    for suggestion in suggestions {
        let text = suggestion.as_str().unwrap();
        for write_tool in ["edit_preview", "edit_commit", "apply_batch", "apply("] {
            assert!(
                !text.contains(write_tool),
                "a partial page must not recommend a write: {text}"
            );
        }
    }
}

fn assert_reassembled_source_guidance(result: &Value) {
    assert!(result["context_page"]["continuation"].is_null());
    let suggestions = result["_m1nd"]["suggest_next"].as_array().unwrap();
    let first = suggestions[0].as_str().unwrap();
    assert!(
        first.contains("reassemble") && first.contains("all") && first.contains("sha256"),
        "final page must require verified reassembly: {result}"
    );
    assert!(suggestions
        .iter()
        .any(|s| s.as_str().unwrap().contains("edit_preview")
            && s.as_str().unwrap().contains("reassembled")));
}

#[test]
fn surgical_page_envelopes_distinguish_partial_final_and_unbounded_source() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo-alpha");
    let runtime = temp.path().join("private-runtime");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(
        repo.join("Cargo.toml"),
        "[package]\nname=\"repo-alpha\"\nversion=\"0.0.0\"\n",
    )
    .unwrap();
    let source = format!(
        "pub fn core() {{}}\n{}",
        "// quoted source λ 🧠\n".repeat(200)
    );
    std::fs::write(repo.join("src/lib.rs"), &source).unwrap();
    let mut owner = Owner::spawn(&repo, &runtime);
    let mut cursor = Value::Null;
    let mut reconstructed = String::new();
    let mut final_page = None;
    for _ in 0..100 {
        let mut args = json!({"file_path":"src/lib.rs","max_output_bytes":2048});
        if !cursor.is_null() {
            args["continuation"] = cursor;
        }
        let reply = owner.call("surgical_context", args);
        assert_ne!(reply["result"]["isError"], json!(true), "{reply}");
        let result = payload(&reply);
        assert!(result["context_page"]["serialized_bytes"].as_u64().unwrap() <= 2048);
        assert_eq!(
            result["context_page"]["source"]["byte_start"]
                .as_u64()
                .unwrap() as usize,
            reconstructed.len()
        );
        reconstructed.push_str(result["file_contents"].as_str().unwrap());
        cursor = result["context_page"]["continuation"].clone();
        if cursor.is_null() {
            assert!(
                result["context_page"]["source"]["byte_start"]
                    .as_u64()
                    .unwrap()
                    > 0
            );
            assert_reassembled_source_guidance(&result);
            final_page = Some(result);
            break;
        }
        assert_continuation_guidance(&result);
    }
    assert!(final_page.is_some(), "continuation must terminate");
    assert_eq!(reconstructed, source);
    let unbounded = payload(&owner.call("surgical_context", json!({"file_path":"src/lib.rs"})));
    assert_eq!(unbounded["file_contents"], source);
    assert!(unbounded.get("context_page").is_none());
    assert_eq!(
        unbounded["_m1nd"]["suggest_next"],
        json!([
            "validate_plan(files) to ground the coupled edit surface",
            "edit_preview(file, content) to preview changes before writing",
            "apply_batch(edits) for multiple files after proof"
        ])
    );
}
