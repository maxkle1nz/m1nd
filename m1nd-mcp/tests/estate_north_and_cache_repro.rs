//! Estate spike reproduction (2026-08-22): `tools/list` flaky, `north` hangs
//! for over 150 seconds, and the embedding cache re-embeds every node on
//! every boot ("cache 0 reused / N new"). This drives the REAL binary over
//! stdio, using the same trusted-library fixture-ingest seam
//! `persist_runtime_root.rs` already uses (never the birth ceremony, never
//! the public fail-closed `ingest` MCP verb), so it never needs a human-run
//! `m1nd init --birth`.
//!
//! Every RPC read carries a bounded deadline, so a real hang FAILS the test
//! (panic) inside that deadline instead of wedging CI for 150s+.
#![cfg(all(unix, feature = "embed"))]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_m1nd-mcp");

/// Generous but bounded — real work should land in low single-digit seconds;
/// the field symptom was ">150s / killed", so anything under 30s here is an
/// honest pass and anything that trips it is a genuine hang, not CI noise.
const RPC_DEADLINE: Duration = Duration::from_secs(30);

/// A small but non-trivial fixture: enough distinct symbols that the semantic
/// engine has real text to embed (the spike's target repo embedded 204 nodes).
fn write_fixture_repo(root: &Path) {
    std::fs::create_dir_all(root.join("src")).expect("mk src");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"estatefix\"\nversion = \"0.0.0\"\n",
    )
    .expect("write Cargo.toml");
    let mut lib_rs = String::from("pub mod helper;\n");
    for i in 0..24 {
        let module = format!("mod_{i}");
        lib_rs.push_str(&format!("pub mod {module};\n"));
        std::fs::write(
            root.join(format!("src/{module}.rs")),
            format!(
                "/// Computes a derived value for widget {i}.\npub fn compute_{i}(x: i64) -> i64 {{ x * {i} + helper::help() }}\n\npub struct Widget{i} {{ pub value: i64 }}\n\nimpl Widget{i} {{\n    pub fn new(value: i64) -> Self {{ Self {{ value }} }}\n    pub fn scaled(&self) -> i64 {{ compute_{i}(self.value) }}\n}}\n"
            ),
        )
        .expect("write module");
    }
    std::fs::write(
        root.join("src/lib.rs"),
        format!("{lib_rs}pub fn top() -> i64 {{ helper::help() + 1 }}\n"),
    )
    .expect("write lib.rs");
    std::fs::write(
        root.join("src/helper.rs"),
        "pub fn help() -> i64 { 41 }\npub struct Helper { pub v: i64 }\n",
    )
    .expect("write helper.rs");
}

fn ingest_fixture(repo: &Path) -> m1nd_core::graph::Graph {
    let (graph, _) = m1nd_ingest::Ingestor::new(m1nd_ingest::IngestConfig {
        root: repo.to_path_buf(),
        parallelism: 1,
        ..m1nd_ingest::IngestConfig::default()
    })
    .ingest()
    .expect("trusted fixture ingest");
    graph
}

struct Owner {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
    stderr_log: PathBuf,
}

impl Owner {
    fn spawn(cwd: &Path, runtime_dir: &Path, stderr_log: &Path) -> Owner {
        let stderr = Stdio::from(std::fs::File::create(stderr_log).expect("create stderr log"));
        let mut child = Command::new(BIN)
            .arg("--no-gui")
            .current_dir(cwd)
            .env("M1ND_RUNTIME_DIR", runtime_dir)
            .env("M1ND_GRAPH_SOURCE", "./graph_snapshot.json")
            .env("M1ND_PLASTICITY_STATE", "./plasticity_state.json")
            .env("M1ND_REGISTRY_DIR", runtime_dir.join("registry"))
            .env("M1ND_NO_GUI", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr)
            .spawn()
            .expect("spawn m1nd-mcp stdio owner");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        let mut owner = Owner {
            child,
            stdin,
            stdout,
            next_id: 0,
            stderr_log: stderr_log.to_path_buf(),
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

    /// Bounded read: panics (test failure, not a CI wedge) if no reply with
    /// this id arrives inside `RPC_DEADLINE`. This is the harness-level
    /// equivalent of the estate probes' `select()`-based deadline.
    fn read_reply(&mut self, id: i64, deadline: Duration) -> serde_json::Value {
        let start = Instant::now();
        loop {
            if start.elapsed() >= deadline {
                panic!(
                    "TIMED OUT after {:?} waiting for reply id={id}. stderr so far:\n{}",
                    start.elapsed(),
                    self.owner_stderr()
                );
            }
            let mut line = String::new();
            let n = self.stdout.read_line(&mut line).expect("read reply line");
            if n == 0 {
                panic!(
                    "owner stdout closed before reply id={id}. stderr:\n{}",
                    self.owner_stderr()
                );
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
                "clientInfo": { "name": "estate-repro", "version": "1.0" }
            }
        });
        self.send(&req.to_string());
        let reply = self.read_reply(id, RPC_DEADLINE);
        assert!(
            reply.get("result").is_some(),
            "initialize must succeed, got {reply}"
        );
    }

    /// Times a raw JSON-RPC method call (not just tools/call) and returns
    /// (wall time, reply).
    fn timed_call(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> (Duration, serde_json::Value) {
        self.next_id += 1;
        let id = self.next_id;
        let req =
            serde_json::json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let t0 = Instant::now();
        self.send(&req.to_string());
        let reply = self.read_reply(id, RPC_DEADLINE);
        (t0.elapsed(), reply)
    }

    fn call(&mut self, tool: &str, args: serde_json::Value) -> (Duration, serde_json::Value) {
        let (elapsed, reply) = self.timed_call(
            "tools/call",
            serde_json::json!({ "name": tool, "arguments": args }),
        );
        let result = reply
            .get("result")
            .unwrap_or_else(|| panic!("tool {tool} returned no result: {reply}"))
            .clone();
        (elapsed, result)
    }

    fn owner_stderr(&self) -> String {
        std::fs::read_to_string(&self.stderr_log).unwrap_or_default()
    }

    fn shutdown(mut self) -> String {
        let stderr_log = self.stderr_log.clone();
        drop(self.stdin);
        let status = self.child.wait().expect("wait for graceful owner shutdown");
        let stderr = std::fs::read_to_string(&stderr_log).unwrap_or_default();
        assert!(
            status.success(),
            "owner shutdown failed with {status}. stderr:\n{stderr}"
        );
        stderr
    }
}

/// Bug #1 + #2: `tools/list` and `north` must both answer inside a bounded
/// deadline against a real, non-trivial graph. The field symptom was
/// `tools/list` answering once in 0s and then hanging on 2 of 3 runs, and
/// `north` never answering at all inside 150s.
#[test]
fn tools_list_and_north_answer_inside_deadline() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime_dir = tmp.path().join("runtime");
    std::fs::create_dir_all(&runtime_dir).expect("mk runtime dir");
    let repo = tmp.path().join("repo");
    write_fixture_repo(&repo);
    let cwd = tmp.path().join("cwd");
    std::fs::create_dir_all(&cwd).expect("mk owner cwd");

    let graph = ingest_fixture(&repo);
    let seeded_nodes = u64::from(graph.num_nodes());
    assert!(
        seeded_nodes >= 20,
        "fixture should yield a real graph, got {seeded_nodes}"
    );
    m1nd_core::snapshot::save_graph(&graph, &runtime_dir.join("graph_snapshot.json"))
        .expect("seed graph snapshot");

    let stderr_log = tmp.path().join("boot.stderr");
    let mut owner = Owner::spawn(&cwd, &runtime_dir, &stderr_log);

    // Run tools/list three times in a row, exactly like the estate probe did
    // ("responded once in 0s, hung on 2 of 3 runs") — a single green call is
    // not proof against flakiness; repeated calls are.
    for attempt in 1..=3 {
        let (elapsed, reply) = owner.timed_call("tools/list", serde_json::json!({}));
        assert!(
            reply.get("result").is_some(),
            "tools/list attempt {attempt} failed: {reply}"
        );
        assert!(
            elapsed < RPC_DEADLINE,
            "tools/list attempt {attempt} took {elapsed:?}, exceeding the {RPC_DEADLINE:?} deadline"
        );
        eprintln!("tools/list attempt {attempt}: {elapsed:?}");
    }

    let (elapsed, north) = owner.call(
        "north",
        serde_json::json!({ "agent_id": "estate-repro", "task": "orient on the fixture crate" }),
    );
    eprintln!("north: {elapsed:?}");
    assert!(
        elapsed < RPC_DEADLINE,
        "north took {elapsed:?}, exceeding the {RPC_DEADLINE:?} deadline (field symptom: >150s, killed)"
    );
    assert!(
        north.get("content").is_some() || north.get("structuredContent").is_some(),
        "north must return a real payload, got {north}"
    );

    owner.shutdown();
}

/// Bug #3: the embedding cache must be reused on a warm second boot over the
/// SAME runtime root and the SAME (unchanged) graph — not re-embed every node
/// ("cache 0 reused / N new" on every boot, ~8s+ overhead before the hang even
/// starts).
#[test]
fn embedding_cache_is_reused_on_warm_second_boot() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime_dir = tmp.path().join("runtime");
    std::fs::create_dir_all(&runtime_dir).expect("mk runtime dir");
    let repo = tmp.path().join("repo");
    write_fixture_repo(&repo);
    let cwd = tmp.path().join("cwd");
    std::fs::create_dir_all(&cwd).expect("mk owner cwd");

    let graph = ingest_fixture(&repo);
    let seeded_nodes = u64::from(graph.num_nodes());
    m1nd_core::snapshot::save_graph(&graph, &runtime_dir.join("graph_snapshot.json"))
        .expect("seed graph snapshot");

    // Boot #1: cold. Everything is a miss by definition.
    let boot1_stderr = tmp.path().join("boot1.stderr");
    let owner1 = Owner::spawn(&cwd, &runtime_dir, &boot1_stderr);
    let log1 = owner1.shutdown();
    eprintln!("=== boot1 stderr ===\n{log1}");
    assert!(
        log1.contains("[m1nd embed]"),
        "boot1 must attempt to build embeddings (embed feature on): {log1}"
    );

    let cache_path = runtime_dir.join("embeddings_cache.bin");
    assert!(
        cache_path.exists(),
        "boot1 must leave the embedding cache on disk at {} after its own \
         clean shutdown; it did not — every subsequent boot has nothing to \
         warm-boot from. stderr:\n{log1}",
        cache_path.display()
    );

    // Boot #2: SAME runtime root, SAME unchanged graph. This must be a warm
    // reuse — the field's exact defect is that this instead re-embeds all N.
    let boot2_stderr = tmp.path().join("boot2.stderr");
    let owner2 = Owner::spawn(&cwd, &runtime_dir, &boot2_stderr);
    let log2 = owner2.shutdown();
    eprintln!("=== boot2 stderr ===\n{log2}");

    let cache_line = log2
        .lines()
        .find(|l| l.contains("[m1nd embed] cache"))
        .unwrap_or_else(|| panic!("boot2 stderr has no cache accounting line: {log2}"));
    eprintln!("boot2 cache line: {cache_line}");

    // Parse "cache {hits} reused / {misses} new of {n} nodes"
    let hits: u64 = cache_line
        .split_whitespace()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("could not parse hits from: {cache_line}"));
    let misses: u64 = cache_line
        .split_whitespace()
        .nth(6)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("could not parse misses from: {cache_line}"));

    assert!(
        hits > 0 && misses == 0,
        "warm second boot over an UNCHANGED graph must reuse every embedding: \
         got {hits} reused / {misses} new (of {seeded_nodes} nodes) — the field \
         symptom was '0 reused / N new' on every boot. Full boot2 stderr:\n{log2}"
    );
}
