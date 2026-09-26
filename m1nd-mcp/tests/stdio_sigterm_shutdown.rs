//! SIGTERM during cold bootstrap cancels before ownership, or checkpoints before release.
#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use m1nd_mcp::checkpoint_store::{
    CheckpointAuthorityValidationReceiptV1, CheckpointAuthorityValidator, CheckpointManifestV1,
    GRAPH_SNAPSHOT_LOGICAL_NAME,
};

struct TestCheckpointValidator;

impl CheckpointAuthorityValidator for TestCheckpointValidator {
    fn validate(
        &self,
        manifest: &CheckpointManifestV1,
        refs_digest: &str,
    ) -> Result<CheckpointAuthorityValidationReceiptV1, String> {
        CheckpointAuthorityValidationReceiptV1::verified(
            "stdio-sigterm-test",
            &manifest.checkpoint_id,
            refs_digest,
            "0".repeat(64),
            1,
        )
        .map_err(|error| error.to_string())
    }
}

const BIN: &str = env!("CARGO_BIN_EXE_m1nd-mcp");

#[test]
fn stdio_sigterm_checkpoints_and_releases_owner() {
    exercise_sigterm_after("Server ready", false, false, false);
}

#[test]
fn stdio_sigterm_during_empty_startup_never_leaves_an_owner() {
    exercise_sigterm_after("[m1nd] Domain:", false, false, false);
}

#[test]
fn stdio_sigterm_interrupts_populated_graph_cold_bootstrap() {
    exercise_sigterm_after("Loaded graph snapshot: 8192 nodes", true, true, false);
}

#[test]
fn stdio_sigterm_while_model_loader_is_blocked_returns_without_a_lease() {
    exercise_sigterm_after("Loaded graph snapshot: 8192 nodes", true, true, true);
}

#[test]
fn stdio_sigterm_after_populated_graph_ready_checkpoints_and_releases_owner() {
    exercise_sigterm_after("Server ready", true, false, false);
}

fn exercise_sigterm_after(
    trigger: &str,
    populated: bool,
    cancel_during_boot: bool,
    block_model: bool,
) {
    let temporary = tempfile::tempdir().expect("temporary runtime");
    let runtime = temporary.path().join("runtime");
    let home = temporary.path().join("home");
    let temp = temporary.path().join("tmp");
    std::fs::create_dir_all(&runtime).expect("runtime directory");
    std::fs::create_dir_all(&home).expect("home directory");
    std::fs::create_dir_all(&temp).expect("temp directory");

    let source_snapshot = if populated {
        // A real, finalized snapshot: the signal lands after load, while the
        // cold semantic engine still has thousands of nodes to initialize.
        // The empty local model directory refuses immediately without a network
        // fetch, so the test measures lifecycle work rather than HF availability.
        let mut graph = m1nd_core::graph::Graph::with_capacity(8192, 8191);
        for i in 0..8192 {
            let node = graph
                .add_node(
                    &format!("repo-alpha/src/file-{i}.rs"),
                    &format!("file-{i}"),
                    m1nd_core::types::NodeType::File,
                    &[],
                    0.0,
                    0.0,
                )
                .expect("seed node");
            if i > 0 {
                graph
                    .add_edge(
                        m1nd_core::types::NodeId::new(i - 1),
                        node,
                        "references",
                        m1nd_core::types::FiniteF32::ONE,
                        m1nd_core::types::EdgeDirection::Forward,
                        false,
                        m1nd_core::types::FiniteF32::ONE,
                    )
                    .expect("seed edge");
            }
        }
        graph.finalize().expect("finalize seed");
        m1nd_core::snapshot::save_graph(&graph, &runtime.join("graph_snapshot.json"))
            .expect("seed snapshot");
        Some(std::fs::read(runtime.join("graph_snapshot.json")).expect("read seed snapshot"))
    } else {
        None
    };

    // The FIFO is the upstream model loader's tokenizer file: opening the
    // writer proves the real binary is inside from_pretrained's read. Never
    // release it until the SIGTERM child has exited; no timing guess or network.
    let model_dir = temporary.path().join("model");
    let fifo = model_dir.join("tokenizer.json");
    if block_model {
        std::fs::create_dir(&model_dir).expect("model fixture directory");
        std::fs::write(model_dir.join("config.json"), "{}").expect("config fixture");
        std::fs::write(model_dir.join("model.safetensors"), b"").expect("weights fixture");
        let name = std::ffi::CString::new(fifo.as_os_str().as_encoded_bytes()).expect("fifo name");
        assert_eq!(
            unsafe { libc::mkfifo(name.as_ptr(), 0o600) },
            0,
            "mkfifo fixture"
        );
    }

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
        .env(
            "M1ND_EMBED_MODEL",
            if block_model { &model_dir } else { &home },
        )
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

    let mut held_model = None;
    if block_model {
        let (opened_tx, opened_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let model_fifo = fifo.clone();
        let holder = std::thread::spawn(move || {
            let writer = std::fs::OpenOptions::new()
                .write(true)
                .open(&model_fifo)
                .expect("open tokenizer FIFO while model reads");
            opened_tx.send(()).expect("report active model read");
            release_rx
                .recv()
                .expect("release model read after child exit");
            drop(writer);
        });
        if opened_rx.recv_timeout(Duration::from_secs(15)).is_err() {
            let _ = child.kill();
            let _ = child.wait();
            // Unblock the fixture writer even if the child never opened the FIFO.
            let _reader = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&fifo)
                .expect("unblock fixture writer");
            release_tx.send(()).expect("release fixture writer");
            holder.join().expect("join fixture writer");
            let observed = reader.join().expect("join stderr reader");
            panic!("stdio owner never entered model load: {observed:#?}");
        }
        held_model = Some((release_tx, holder));
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
            if let Some((release_tx, holder)) = held_model.take() {
                release_tx
                    .send(())
                    .expect("release fixture writer after kill");
                holder.join().expect("join fixture writer after kill");
            }
            let observed = reader.join().expect("join stderr reader");
            panic!("stdio owner did not stop after SIGTERM: {observed:#?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    if let Some((release_tx, holder)) = held_model {
        release_tx.send(()).expect("release fixture writer");
        holder.join().expect("join fixture writer");
    }
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
    let cancelled_before_owner =
        stderr.contains("Startup cancelled before serving; no owner remains.");
    if cancel_during_boot {
        assert!(
            cancelled_before_owner,
            "signal during cold bootstrap must cancel before lease; stderr:\n{stderr}"
        );
    }
    if cancelled_before_owner {
        assert!(
            !stderr.contains("Server ready"),
            "cold boot must not finish before acknowledging SIGTERM; stderr:\n{stderr}"
        );
    } else {
        assert!(
            stderr.contains("actor checkpoint ACK(s); owner released. Goodbye."),
            "persist-before-release receipt must be emitted; stderr:\n{stderr}"
        );
    }
    if populated {
        let graph = m1nd_core::snapshot::load_graph(&runtime.join("graph_snapshot.json"))
            .expect("populated snapshot remains readable after SIGTERM");
        assert_eq!((graph.num_nodes(), graph.num_edges()), (8192, 8191));
    }
    if cancelled_before_owner {
        if populated {
            assert_eq!(
                std::fs::read(runtime.join("graph_snapshot.json")).expect("source snapshot exists"),
                source_snapshot.expect("populated seed"),
                "cancellation before lease must not replace the source snapshot"
            );
        }
        assert!(
            !runtime.join("checkpoint-store/CURRENT").exists(),
            "no checkpoint may be published before an actor exists"
        );
    } else if populated {
        let store =
            m1nd_mcp::checkpoint_store::CheckpointStore::open(runtime.join("checkpoint-store"))
                .expect("open checkpoint after child exit");
        let pointer = store
            .current_pointer()
            .expect("valid CURRENT after shutdown");
        assert!(!pointer.current_checkpoint_id.is_empty());
        let loaded = store
            .load_current(&TestCheckpointValidator)
            .expect("checkpoint inventory and content digests validate");
        let graph = m1nd_core::snapshot::decode_graph_json(
            &loaded
                .read_file(GRAPH_SNAPSHOT_LOGICAL_NAME)
                .expect("checkpoint graph blob validates"),
        )
        .expect("checkpoint graph decodes");
        assert_eq!((graph.num_nodes(), graph.num_edges()), (8192, 8191));
    }
    for subdir in ["leases", "instances"] {
        let dir = runtime.join("registry").join(subdir);
        if cancelled_before_owner {
            assert!(
                !dir.exists(),
                "cancelled before registry acquisition: {subdir}"
            );
        } else {
            assert!(dir.is_dir(), "owner must have registered in {subdir}");
        }
        if dir.exists() {
            assert_eq!(
                std::fs::read_dir(&dir)
                    .expect("read registry directory")
                    .filter_map(Result::ok)
                    .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
                    .count(),
                0,
                "{subdir} must be empty after child exit; stderr:\n{stderr}"
            );
        }
    }
}
