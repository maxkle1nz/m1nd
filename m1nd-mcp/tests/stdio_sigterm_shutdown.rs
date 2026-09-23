//! A launcher SIGTERM must use the same persist-before-release path as SIGINT.
#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_m1nd-mcp");

#[test]
fn stdio_sigterm_checkpoints_and_releases_owner() {
    exercise_sigterm_after("Server ready");
}

#[test]
fn stdio_sigterm_during_startup_still_checkpoints_and_releases_owner() {
    exercise_sigterm_after("[m1nd] Domain:");
}

fn exercise_sigterm_after(trigger: &str) {
    let temporary = tempfile::tempdir().expect("temporary runtime");
    let runtime = temporary.path().join("runtime");
    let home = temporary.path().join("home");
    let temp = temporary.path().join("tmp");
    std::fs::create_dir_all(&runtime).expect("runtime directory");
    std::fs::create_dir_all(&home).expect("home directory");
    std::fs::create_dir_all(&temp).expect("temp directory");

    let mut child = Command::new(BIN)
        .args(["--stdio", "--no-gui"])
        .current_dir(temporary.path())
        .env_remove("M1ND_WORKSPACE_ROOT")
        .env_remove("M1ND_PROJECT_ROOT")
        .env_remove("M1ND_REPO_ROOT")
        .env_remove("WORKSPACE_ROOT")
        .env_remove("PROJECT_ROOT")
        .env_remove("REPO_ROOT")
        .env_remove("CLAUDE_PROJECT_DIR")
        .env_remove("CLAUDE_WORKSPACE_ROOT")
        .env_remove("ANTHROPIC_WORKSPACE_ROOT")
        .env_remove("ANTIGRAVITY_WORKSPACE_ROOT")
        .env_remove("ANTIGRAVITY_PROJECT_ROOT")
        .env_remove("GEMINI_WORKSPACE_ROOT")
        .env_remove("GEMINI_PROJECT_ROOT")
        .env_remove("CURSOR_WORKSPACE_ROOT")
        .env_remove("CURSOR_PROJECT_ROOT")
        .env_remove("WINDSURF_WORKSPACE_ROOT")
        .env_remove("WINDSURF_PROJECT_ROOT")
        .env_remove("VSCODE_WORKSPACE")
        .env_remove("VSCODE_CWD")
        .env_remove("INIT_CWD")
        .env_remove("PWD")
        .env_remove("OLDPWD")
        .env_remove("M1ND_GRAPH_SOURCE")
        .env_remove("GRAPH_SNAPSHOT_PATH")
        .env_remove("M1ND_PLASTICITY_STATE")
        .env_remove("PLASTICITY_STATE_PATH")
        .env_remove("M1ND_READ_ONLY")
        .env("M1ND_RUNTIME_DIR", &runtime)
        .env("M1ND_REGISTRY_DIR", runtime.join("registry"))
        .env("M1ND_NO_GUI", "1")
        .env("HOME", &home)
        .env("TMPDIR", &temp)
        .env("TMP", &temp)
        .env("TEMP", &temp)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn stdio owner");

    let stderr = child.stderr.take().expect("owner stderr");
    let (lines_tx, lines_rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut observed = Vec::new();
        for line in BufReader::new(stderr).lines() {
            let Ok(line) = line else { break };
            observed.push(line.clone());
            let _ = lines_tx.send(line);
        }
        observed
    });

    let ready_deadline = Instant::now() + Duration::from_secs(30);
    let mut trigger_seen = false;
    while Instant::now() < ready_deadline {
        match lines_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(line) if line.contains(trigger) => {
                trigger_seen = true;
                break;
            }
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    if !trigger_seen {
        let _ = child.kill();
        let _ = child.wait();
        let observed = reader.join().expect("join stderr reader");
        panic!("stdio owner never emitted startup trigger {trigger:?}: {observed:#?}");
    }

    let kill_status = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("send SIGTERM");
    assert!(kill_status.success(), "kill -TERM must succeed");

    let exit_deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll owner") {
            break status;
        }
        if Instant::now() >= exit_deadline {
            let _ = child.kill();
            let _ = child.wait();
            let observed = reader.join().expect("join stderr reader");
            panic!("stdio owner did not stop after SIGTERM: {observed:#?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let observed = reader.join().expect("join stderr reader");
    let stderr = observed.join("\n");

    assert!(
        status.success(),
        "SIGTERM must return through the cooperative shutdown path, got {status}; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("SIGTERM received"),
        "signal receipt must be explicit; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("actor checkpoint ACK(s); owner released. Goodbye."),
        "persist-before-release receipt must be emitted; stderr:\n{stderr}"
    );
}
