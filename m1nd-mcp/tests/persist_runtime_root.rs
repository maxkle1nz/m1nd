//! Field-triage batch B — persist targets must resolve against the runtime
//! root, never against the process cwd.
//!
//! THE BUG (root-caused in `~/.m1nd/field-reports.jsonl`, L27/L29): the
//! launchd-spawned `--serve` owner (`com.kle1nz.m1nd-serve`) has NO
//! `WorkingDirectory` key, so it runs with `cwd=/` — a sealed, read-only
//! volume. The default `graph_source` / `plasticity_state` are RELATIVE
//! (`./graph_snapshot.json`), so every persist resolved against `/` and failed
//! with `Read-only file system (os error 30)`:
//!
//!   [m1nd] WARNING: ingest roots persist failed: Read-only file system (os error 30)
//!   [m1nd] auto-persist after ingest failed: I/O error: Read-only file system (os error 30)
//!   [m1nd] No graph snapshot found, starting fresh
//!
//! The medulla therefore re-ingested the whole repo on every boot and warm-boot
//! never worked (`graph_path_exists: false`). Absolute-anchored writes (the
//! embedding cache, boot memory, daemon state — all `runtime_root.join(...)`)
//! kept working; only the cwd-relative ones failed.
//!
//! THE FIX (proved here red→green): a relative persist target is anchored on the
//! configured `runtime_dir` (`main.rs::anchor_persist_target`), so the snapshot
//! and its `ingest_roots.json` neighbor land under the always-writable runtime
//! dir regardless of cwd. This test reproduces the EXACT launchd condition (it
//! spawns the real binary with `cwd=/` and a relative graph source) and proves
//! two things.
//!
//! First: after a trusted offline fixture seed is loaded and the real owner
//! performs its graceful shutdown checkpoint, `<runtime>/graph_snapshot.json`
//! and `<runtime>/ingest_roots.json` exist (on the original bug the writes hit
//! `/` and fail). Second: a SECOND process on a DIFFERENT cwd WARM-BOOTS from
//! that snapshot — it loads the graph (node_count preserved) without any
//! external mutation request.
//!
//! Driving the real binary over stdio JSON-RPC is the faithful seam: the fix
//! lives in the binary's config resolution (`load_config_from_cli`), which only
//! the process entry point exercises — an in-process `SessionState` test would
//! bypass it entirely.
//!
//! Unix-only: the bug is a launchd sealed-volume `cwd=/` condition and the
//! reproduction pins cwd to `/`, whose semantics (and read-only-ness) are
//! POSIX-specific. The portable path-resolution contract itself
//! (`anchor_persist_target`) is covered by cross-platform unit tests in
//! `m1nd-mcp/src/main.rs`.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

/// Path to the compiled binary under test (Cargo sets `CARGO_BIN_EXE_<name>`).
const BIN: &str = env!("CARGO_BIN_EXE_m1nd-mcp");

/// A read-only, always-present directory that stands in for the sealed launchd
/// cwd. On macOS/Linux `/` is not writable by a normal user, which is exactly
/// the condition that made relative persists fail with os error 30.
const READ_ONLY_CWD: &str = "/";

/// A tiny two-file Rust crate to ingest, written under the runtime's sibling
/// source dir. Small + deterministic so node_count is stable across boots.
fn write_fixture_repo(root: &Path) {
    std::fs::create_dir_all(root.join("src")).expect("mk src");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"persistfix\"\nversion = \"0.0.0\"\n",
    )
    .expect("write Cargo.toml");
    std::fs::write(
        root.join("src/lib.rs"),
        "pub mod helper;\npub fn top() -> i64 { helper::help() + 1 }\n",
    )
    .expect("write lib.rs");
    std::fs::write(
        root.join("src/helper.rs"),
        "pub fn help() -> i64 { 41 }\npub struct Helper { pub v: i64 }\n",
    )
    .expect("write helper.rs");
}

/// A live stdio JSON-RPC connection to a spawned owner process.
struct Owner {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl Owner {
    /// Spawn the binary in stdio mode with a specific cwd + a RELATIVE graph
    /// source, mirroring the launchd invocation (no WorkingDirectory ⇒ cwd=/,
    /// `--runtime-dir` set, default relative `./graph_snapshot.json`).
    fn spawn(cwd: &Path, runtime_dir: &Path) -> Owner {
        let mut child = Command::new(BIN)
            .arg("--no-gui")
            .current_dir(cwd)
            .env("M1ND_RUNTIME_DIR", runtime_dir)
            // RELATIVE on purpose — this is the default the launchd owner runs
            // with. Pre-fix it resolves against `cwd`; post-fix against runtime.
            .env("M1ND_GRAPH_SOURCE", "./graph_snapshot.json")
            .env("M1ND_PLASTICITY_STATE", "./plasticity_state.json")
            // Deterministic + isolated: never touch the developer's real runtime.
            .env("M1ND_REGISTRY_DIR", runtime_dir.join("registry"))
            .env("M1ND_NO_GUI", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn m1nd-mcp stdio owner");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        let mut owner = Owner {
            child,
            stdin,
            stdout,
            next_id: 0,
        };
        owner.initialize();
        owner
    }

    fn send(&mut self, line: &str) {
        self.stdin
            .write_all(line.as_bytes())
            .expect("write request");
        self.stdin.write_all(b"\n").expect("write newline");
        self.stdin.flush().expect("flush request");
    }

    /// Read newline-delimited JSON responses until one carries our `id` (the
    /// server may interleave nothing else in stdio Line mode, but be robust).
    fn read_reply(&mut self, id: i64) -> serde_json::Value {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if Instant::now() >= deadline {
                panic!("timed out waiting for reply id={id}");
            }
            let mut line = String::new();
            let n = self.stdout.read_line(&mut line).expect("read reply line");
            if n == 0 {
                panic!("owner stdout closed before reply id={id}");
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed) else {
                continue;
            };
            if v.get("id").and_then(|x| x.as_i64()) == Some(id) {
                return v;
            }
        }
    }

    fn initialize(&mut self) {
        self.next_id += 1;
        let id = self.next_id;
        let req = serde_json::json!({
            "jsonrpc": "2.0", "id": id, "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "persist-runtime-root-probe", "version": "1.0" }
            }
        });
        self.send(&req.to_string());
        let reply = self.read_reply(id);
        assert!(
            reply.get("result").is_some(),
            "initialize must succeed, got {reply}"
        );
    }

    /// Call a tool, returning the parsed `result` payload (structuredContent when
    /// present, else the raw result).
    fn call(&mut self, tool: &str, args: serde_json::Value) -> serde_json::Value {
        self.next_id += 1;
        let id = self.next_id;
        let req = serde_json::json!({
            "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": tool, "arguments": args }
        });
        self.send(&req.to_string());
        let reply = self.read_reply(id);
        let result = reply
            .get("result")
            .unwrap_or_else(|| panic!("tool {tool} returned no result: {reply}"))
            .clone();
        // MCP wraps tool output; the structured payload lives under
        // structuredContent, or the first content text is JSON we can parse.
        if let Some(sc) = result.get("structuredContent") {
            return sc.clone();
        }
        if let Some(text) = result
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|item| item.get("text"))
            .and_then(|t| t.as_str())
        {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) {
                return parsed;
            }
        }
        result
    }

    /// Read the live graph node_count from the `health` tool. `health` carries a
    /// top-level `node_count`; the binding fingerprint mirrors it (and its
    /// `graph_path` / `graph_path_exists` are exactly the field-report signals).
    fn node_count(&mut self) -> u64 {
        let health = self.call("health", serde_json::json!({ "agent_id": "probe" }));
        health
            .get("node_count")
            .and_then(|n| n.as_u64())
            .unwrap_or_else(|| panic!("no node_count in health body: {health}"))
    }

    fn shutdown(mut self) {
        // Closing the transport is the stdio owner's cooperative shutdown seam.
        // It must run the real persist-before-release lifecycle; killing the
        // child would bypass the behavior this integration test exists to prove.
        drop(self.stdin);
        let status = self.child.wait().expect("wait for graceful owner shutdown");
        assert!(status.success(), "owner shutdown failed with {status}");
    }
}

#[test]
fn persist_lands_under_runtime_root_and_second_process_warm_boots() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime_dir = tmp.path().join("runtime");
    std::fs::create_dir_all(&runtime_dir).expect("mk runtime dir");
    let repo = tmp.path().join("repo");
    write_fixture_repo(&repo);

    // The snapshot + ingest_roots MUST land here (under the runtime), never in
    // the read-only cwd. Pre-fix they resolve to `/graph_snapshot.json` etc.
    let snapshot = runtime_dir.join("graph_snapshot.json");
    let ingest_roots = runtime_dir.join("ingest_roots.json");

    // Seed a non-empty graph through the trusted in-process ingest library. The
    // public generic MCP `ingest` route is intentionally fail-closed now that
    // graph replacement requires the exact typed G2/G3 authority consumer; this
    // path-resolution test must not weaken or bypass that production contract.
    let (seed_graph, _) = m1nd_ingest::Ingestor::new(m1nd_ingest::IngestConfig {
        root: repo.clone(),
        parallelism: 1,
        ..m1nd_ingest::IngestConfig::default()
    })
    .ingest()
    .expect("trusted fixture ingest");
    let seeded_nodes = u64::from(seed_graph.num_nodes());
    assert!(
        seeded_nodes >= 3,
        "fixture ingest should create several nodes, got {seeded_nodes}"
    );
    m1nd_core::snapshot::save_graph(&seed_graph, &snapshot).expect("seed graph snapshot");

    // --- Boot #1: cwd = "/" (the sealed launchd volume). Load + checkpoint. ---
    let mut owner = Owner::spawn(Path::new(READ_ONLY_CWD), &runtime_dir);
    let ingested_nodes = owner.node_count();
    assert!(
        ingested_nodes >= seeded_nodes,
        "owner must load the trusted seed: expected at least {seeded_nodes} nodes, got {ingested_nodes}"
    );

    // Remove the seed after it has been loaded. Only the owner's real graceful
    // shutdown checkpoint can recreate it, so the existence assertion below is
    // a write proof rather than a pre-seeded tautology.
    std::fs::remove_file(&snapshot).expect("remove loaded seed before checkpoint");
    assert!(!snapshot.exists(), "checkpoint target must start absent");
    owner.shutdown();

    // THE CORE ASSERTIONS (RED on main — these files never appear because the
    // writes hit the read-only `/`; GREEN with the fix — they land under runtime).
    assert!(
        snapshot.exists(),
        "graph snapshot must be persisted under the runtime root at {}; \
         on the bug it silently failed against cwd=/ (os error 30)",
        snapshot.display()
    );
    assert!(
        ingest_roots.exists(),
        "ingest_roots.json must be persisted next to the snapshot under the \
         runtime root at {} (this is the exact 'ingest roots persist failed' line)",
        ingest_roots.display()
    );

    // --- Boot #2: a DIFFERENT cwd (the tempdir itself, which is writable) but the
    // SAME runtime. If persist had leaked into cwd, this fresh cwd would NOT see
    // it and would re-ingest from zero. A true warm boot loads the snapshot. ---
    let mut owner2 = Owner::spawn(tmp.path(), &runtime_dir);
    let warm_nodes = owner2.node_count();
    owner2.shutdown();

    assert!(
        warm_nodes >= ingested_nodes,
        "second process (different cwd, same runtime) must WARM-BOOT from the \
         snapshot with node_count preserved: booted with {warm_nodes} nodes but \
         the persisted graph had {ingested_nodes}. A lower count means it \
         re-ingested / started fresh — the warm boot the fix restores."
    );
}
