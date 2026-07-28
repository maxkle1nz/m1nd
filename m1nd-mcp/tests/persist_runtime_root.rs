//! Field-triage batch B — persist targets must resolve against the runtime
//! root, never against the process cwd.
//!
//! THE BUG (root-caused in `~/.m1nd/field-reports.jsonl`, L27/L29): the
//! launchd-spawned `--serve` owner (`com.local.m1nd-serve`) has NO
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
//!
//! THE SECOND TEST IN THIS FILE generalizes that subprocess seam into the
//! LIFECYCLE GATE — `boot → serve → mutate → clean shutdown → boot again →
//! still serves` — because the boot-path defects of 2026-07-27 all needed a
//! second boot over prior state to exist at all. It lives here rather than in a
//! file of its own so it reuses this harness instead of growing a second one.
//! Its own long comment block, below, carries the reasoning and the declared
//! wave-2 boundary.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
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

/// Ingest the fixture crate through the trusted in-process library. The public
/// generic MCP `ingest` route is fail-closed (graph replacement requires the
/// typed G2/G3 authority consumer), so seeding goes through the library that
/// production ingest itself uses.
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

/// A live stdio JSON-RPC connection to a spawned owner process.
struct Owner {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
    /// Where this owner's stderr was redirected, when the caller asked for it.
    /// An owner that dies mid-boot says why THERE and nowhere else, so every
    /// panic below quotes it.
    stderr_log: Option<PathBuf>,
}

impl Owner {
    /// Spawn the binary in stdio mode with a specific cwd + a RELATIVE graph
    /// source, mirroring the launchd invocation (no WorkingDirectory ⇒ cwd=/,
    /// `--runtime-dir` set, default relative `./graph_snapshot.json`).
    fn spawn(cwd: &Path, runtime_dir: &Path) -> Owner {
        Owner::start(cwd, runtime_dir, None)
    }

    /// The same spawn, keeping the owner's stderr in a file the caller can read
    /// after the process is gone. A FILE and not a pipe on purpose: nothing here
    /// drains the child's stderr while it runs, and a full pipe buffer would
    /// wedge the owner mid-boot instead of failing an assertion.
    fn spawn_logging_stderr(cwd: &Path, runtime_dir: &Path, stderr_log: &Path) -> Owner {
        Owner::start(cwd, runtime_dir, Some(stderr_log))
    }

    fn start(cwd: &Path, runtime_dir: &Path, stderr_log: Option<&Path>) -> Owner {
        let stderr = match stderr_log {
            Some(path) => {
                Stdio::from(std::fs::File::create(path).expect("create the owner stderr log"))
            }
            None => Stdio::null(),
        };
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
            stderr_log: stderr_log.map(Path::to_path_buf),
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
                panic!(
                    "owner stdout closed before reply id={id} — the process died \
                     instead of answering. Its stderr said:\n{}",
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

    /// This owner's captured stderr, or an honest note when it was not captured.
    fn owner_stderr(&self) -> String {
        match self.stderr_log.as_deref() {
            Some(path) => read_stderr(path),
            None => "<stderr not captured for this owner>".to_string(),
        }
    }

    fn shutdown(mut self) {
        // Closing the transport is the stdio owner's cooperative shutdown seam.
        // It must run the real persist-before-release lifecycle; killing the
        // child would bypass the behavior this integration test exists to prove.
        let stderr = self.owner_stderr();
        drop(self.stdin);
        let status = self.child.wait().expect("wait for graceful owner shutdown");
        assert!(
            status.success(),
            "owner shutdown failed with {status}. Its stderr said:\n{stderr}"
        );
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
    let seed_graph = ingest_fixture(&repo);
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

// ─────────────────────────────────────────────────────────────────────────────
// THE LIFECYCLE GATE — Cycle A
//
// m1nd is a continuity system: its promise is state crossing time. On
// 2026-07-27 three independent BRICKING defects were found in the boot path
// and none of the then-green unit tests caught any of them, because each one
// needs a SECOND boot over prior state to exist at all:
//
//   * the owner's brain served 0 of 5540 nodes for five days — the actor start
//     reconciled a CURRENT captured while the runtime graph was empty and
//     reverted the graph the boot had just loaded (#441);
//   * a CLEAN shutdown bricked the next boot — the plasticity sidecar a
//     shutdown writes carries two rows for a parallel edge, and the reader
//     refused its own writer's file (#442);
//   * a graph-stale co-change sidecar aborted `SessionState::initialize` and
//     took all 48 MCP tools down with it (#442).
//
// The general property behind all three had no proof:
//
//   boot → serve → mutate → clean shutdown → boot again → still serves
//
// This is that proof, over the real binary, on ONE runtime root, across four
// generations of it. Every boot is strictly SEQUENTIAL — the checkpoint store's
// `WRITER.lock` allows one writer at a time, so a parallel spawn would fail the
// lock rather than test the property.
//
// WHY FOUR BOOTS, each earning its place:
//   #0 publishes the EMPTY CURRENT that a later boot's graph must survive;
//   #1 adopts the pre-1.5 graph through the actor, serves it, makes the
//      DEBOUNCED durable write, and shuts down cleanly;
//   #2 is the second boot over prior state — it must come up AT ALL (the
//      plasticity sidecar it strictly reads carries two rows under one full
//      synaptic key), still see the debounced write, then make the CLASSIFIED
//      durable write and shut down cleanly;
//   #3 must still find what #2 memorized.
// The classified write is `memorize`, which re-ingests and therefore rewrites
// the graph; that is exactly why it lands on its own leg instead of sharing one
// with the parallel-edge sidecar it would otherwise dedupe away.
//
// THE DURABLE-WRITE SURFACE IS A DECLARED BOUNDARY, not a choice. Under the
// M1ND-10 authority floors, almost every mutating verb in
// `READ_ONLY_DENIED_TOOLS` — `antibody_create`, `learn`, `daemon_start`,
// `auto_ingest_start`, `calibrate_*`, `boot_memory` set/delete, `apply`,
// `edit_commit` — now sits at SCOPED_GRANT_A2 or above and refuses generic MCP
// dispatch outright ("generic_action_authority_required"). Of the whole list
// only `memorize` (plus the candidate/mission verbs, which belong to M1ND-10
// and are out of scope here) is still ORDINARY. So the CLASSIFIED family is
// covered through `memorize`, and the DEBOUNCED family through `alerts_ack` —
// whose own alert has to be SEEDED, because every producer of a real alert
// (`daemon_tick`, `apply`) is above the floor too.
//
// Cycle B — crash / `kill -9` with no checkpoint — is deliberately NOT here.
// It is wave 2, and until it lands the fault-injection half of this property
// stays NOT_RUN rather than silently claimed.
// ─────────────────────────────────────────────────────────────────────────────

/// The agent identity every call in the lifecycle cycle carries.
const LIFECYCLE_AGENT: &str = "lifecycle-gate-probe";

/// The label of the knowledge the CLASSIFIED durable write commits. Unique
/// enough that finding it after the restart cannot be a coincidence.
const LIFECYCLE_MEMORY_LABEL: &str = "LifecycleGateMarkerZ7";

/// The alert the DEBOUNCED durable write acknowledges. Seeded, because every
/// producer of a real alert (`daemon_tick`, `apply`) sits above the ORDINARY
/// authority floor — see the boundary note in the block above.
const LIFECYCLE_ALERT_ID: &str = "alert-lifecycle-gate-0001";

/// Lines a healthy SECOND boot must never print. Every one of them is a real
/// refusal or degrade emitted on the startup path, and every one means state a
/// clean shutdown promised to keep was dropped, reverted, or never written.
const SIDECAR_REFUSAL_SIGNATURES: &[&str] = &[
    // The co-change sidecar no longer binds to the loaded graph (#442). Fatal
    // before the fix; a degrade after it — but on a warm boot over its OWN
    // checkpoint it must not happen at all.
    "does not match the loaded graph",
    // Any sidecar degraded away on the boot path.
    "continuing without it",
    // The plasticity round trip refusing what its own writer produced (#442),
    // in either the pre-fix ("ambiguous across") or post-fix wording.
    "synaptic key",
    // The launchd bug this file was born for: a relative persist target
    // resolved against a read-only cwd.
    "persist failed",
    // A second boot that found nothing to warm-boot from.
    "No graph snapshot found",
    // The field's exact signature for #441: a boot that loads a graph and then
    // serves an empty one.
    "Server ready. 0 nodes",
    // The one-time rescue must not fire twice; the runtime is populated now.
    "adopted legacy graph snapshot",
];

/// The fixture graph plus ONE deliberate parallel edge: a second edge between
/// the same pair with the same relation, direction, inhibitory flag and causal
/// strength — sharing the COMPLETE synaptic key that plasticity persistence is
/// written against.
///
/// This is not decoration. `export_state` writes one row per CSR slot, so a
/// graph with a parallel edge makes every clean shutdown produce a sidecar
/// carrying two rows under one key: the exact file whose reader used to refuse
/// it and brick the next boot (#442). The owner's real graph carries these on
/// `contains` edges; a three-file fixture does not grow one on its own, so the
/// condition is planted here and then ASSERTED, never assumed.
fn seed_graph_with_a_parallel_edge(repo: &Path) -> m1nd_core::graph::Graph {
    let mut graph = ingest_fixture(repo);
    let source = (0..graph.num_nodes() as usize)
        .find(|&index| graph.csr.offsets[index + 1] > graph.csr.offsets[index])
        .map(|index| m1nd_core::types::NodeId::new(index as u32))
        .expect("the fixture graph must have at least one edge to twin");
    let slot = graph.csr.offsets[source.as_usize()] as usize;
    let target = graph.csr.targets[slot];
    let relation = graph
        .strings
        .try_resolve(graph.csr.relations[slot])
        .expect("resolve the twinned relation")
        .to_string();
    let direction = graph.csr.directions[slot];
    let inhibitory = graph.csr.inhibitory[slot];
    let causal_strength = graph.csr.causal_strengths[slot];
    graph
        .add_edge(
            source,
            target,
            &relation,
            m1nd_core::types::FiniteF32::new(0.5),
            direction,
            inhibitory,
            causal_strength,
        )
        .expect("add the twin edge");
    graph.finalize().expect("re-finalize with the twin edge");

    // Precondition, not decoration: the graph must actually export two rows
    // under one complete key, or this fixture silently stops covering #442.
    let rows = m1nd_core::plasticity::PlasticityEngine::new(
        &graph,
        m1nd_core::plasticity::PlasticityConfig::default(),
    )
    .export_state(&graph)
    .expect("export the seeded plasticity state");
    let mut rows_per_key: std::collections::HashMap<_, usize> = std::collections::HashMap::new();
    for row in &rows {
        *rows_per_key
            .entry((
                row.source_label.as_str(),
                row.target_label.as_str(),
                row.relation.as_str(),
                row.direction,
                row.inhibitory,
            ))
            .or_default() += 1;
    }
    let twinned = rows_per_key.values().copied().max().unwrap_or(0);
    assert!(
        twinned >= 2,
        "the seeded graph must carry a parallel edge (two rows under one full \
         synaptic key), got {twinned} row(s) — without it this gate stops \
         covering the plasticity round trip a clean shutdown depends on"
    );
    graph
}

/// Seed one unacked daemon alert into the runtime root, the way an owner that
/// shut down with an open finding leaves one behind. `daemon_alerts` is a
/// durable checkpoint sidecar and `alerts_ack` is its DEBOUNCED writer.
fn seed_unacked_alert(runtime_dir: &Path) {
    let alerts = serde_json::json!([{
        "alert_id": LIFECYCLE_ALERT_ID,
        "severity": "warning",
        "kind": "lifecycle_gate",
        "message": "seeded finding that must stay acknowledged across a restart",
        "confidence": 0.9,
        "evidence": [],
        "suggested_tool": null,
        "suggested_target": null,
        "file_path": null,
        "node_id": null,
        "created_at_ms": 1_700_000_000_000u64,
        "acked": false,
        "acked_at_ms": null
    }]);
    std::fs::write(
        runtime_dir.join("daemon_alerts.json"),
        serde_json::to_vec_pretty(&alerts).expect("encode seeded alerts"),
    )
    .expect("seed daemon alerts");
}

/// Read a captured stderr log; an owner that never wrote one reads as empty.
fn read_stderr(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn brain_serves_its_own_state_across_clean_shutdowns_and_second_boots() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime_dir = tmp.path().join("runtime");
    std::fs::create_dir_all(&runtime_dir).expect("mk runtime dir");
    let repo = tmp.path().join("repo");
    write_fixture_repo(&repo);
    seed_unacked_alert(&runtime_dir);

    // The owner's working directory across every boot — the one a restart keeps.
    let cwd = tmp.path().join("cwd");
    std::fs::create_dir_all(&cwd).expect("mk owner cwd");

    let boot0_stderr = tmp.path().join("boot0.stderr");
    let boot1_stderr = tmp.path().join("boot1.stderr");
    let boot2_stderr = tmp.path().join("boot2.stderr");
    let boot3_stderr = tmp.path().join("boot3.stderr");

    // ── Boot #0: the empty brain that publishes an EMPTY CURRENT ─────────────
    // Not scaffolding — this generation is half the defect. #441 happened
    // because a CURRENT captured while the runtime graph was still empty
    // outranked the graph the NEXT boot legitimately loaded. Without a prior
    // generation on this root there is nothing for the next boot to be reverted
    // by, and the whole class becomes invisible.
    let cold = Owner::spawn_logging_stderr(&cwd, &runtime_dir, &boot0_stderr);
    cold.shutdown();

    // The pre-1.5 graph now appears in the legacy location beside an empty
    // runtime that already has a committed generation: the field's exact
    // upgrade footprint.
    let seed_graph = seed_graph_with_a_parallel_edge(&repo);
    let seeded_nodes = u64::from(seed_graph.num_nodes());
    assert!(
        seeded_nodes >= 3,
        "fixture ingest should create several nodes, got {seeded_nodes}"
    );
    m1nd_core::snapshot::save_graph(&seed_graph, &cwd.join("graph_snapshot.json"))
        .expect("seed the legacy graph snapshot");

    // ── Boot #1: serve, make the DEBOUNCED durable write, shut down cleanly ──
    let mut owner1 = Owner::spawn_logging_stderr(&cwd, &runtime_dir, &boot1_stderr);
    let served = owner1.node_count();
    assert!(
        served >= seeded_nodes,
        "the first boot must SERVE the graph it adopted: expected at least \
         {seeded_nodes} nodes, got {served}. Serving fewer is the field's exact \
         signature — a graph the boot loaded and the actor's CURRENT \
         reconciliation then reverted."
    );

    // `alerts_ack` is NOT classified a mutation. It reaches the persist choke
    // point `persist_daemon_alerts`, which under an actor stage writes nothing
    // and only joins the staged-persist debounce. The default interval is 50
    // deferring turns, so the debounce cannot flush inside this cycle: the ack
    // is durable to NOBODY unless the clean shutdown checkpoint carries it.
    let acked = owner1.call(
        "alerts_ack",
        serde_json::json!({ "agent_id": LIFECYCLE_AGENT, "alert_ids": [LIFECYCLE_ALERT_ID] }),
    );
    assert_eq!(
        acked.get("acked").and_then(|value| value.as_u64()),
        Some(1),
        "the seeded alert must be acknowledged in memory before the shutdown: {acked}"
    );

    // The cooperative stdio seam, not a kill: closing the transport runs the
    // real persist-before-release lifecycle.
    owner1.shutdown();

    let boot1_log = read_stderr(&boot1_stderr);
    assert!(
        boot1_log.contains("adopted legacy graph snapshot"),
        "the first boot must reach its graph THROUGH the actor-boundary \
         adoption — that seam is what stops the actor's CURRENT reconciliation \
         from reverting it on the same boot. stderr was:\n{boot1_log}"
    );

    // ── Boot #2: the second boot over prior state ────────────────────────────
    // Booting AT ALL is the first assertion here. The plasticity sidecar this
    // boot is about to read strictly is the one the clean shutdown just wrote,
    // and it carries two rows under one full synaptic key (the seeded parallel
    // edge). A reader that refuses its own writer's file dies on this line.
    let mut owner2 = Owner::spawn_logging_stderr(&cwd, &runtime_dir, &boot2_stderr);
    let rebooted = owner2.node_count();
    assert!(
        rebooted >= served,
        "the second boot must serve the state the first one checkpointed: \
         booted with {rebooted} nodes but the first boot served {served}"
    );

    // The DEBOUNCED durable write survived, read back through a public verb.
    let alerts = owner2.call(
        "alerts_list",
        serde_json::json!({ "agent_id": LIFECYCLE_AGENT, "include_acked": true, "limit": 50 }),
    );
    let still_acked = alerts
        .get("alerts")
        .and_then(|list| list.as_array())
        .and_then(|list| {
            list.iter()
                .find(|entry| {
                    entry.get("alert_id").and_then(|id| id.as_str()) == Some(LIFECYCLE_ALERT_ID)
                })
                .and_then(|entry| entry.get("acked"))
                .and_then(|acked| acked.as_bool())
        });
    assert_eq!(
        still_acked,
        Some(true),
        "the DEBOUNCED durable write must survive the restart: `alerts_ack` \
         acked {LIFECYCLE_ALERT_ID} before a clean shutdown and this boot reads \
         it back unacknowledged. The staged-persist drift never reached the \
         shutdown checkpoint — a loss window a clean shutdown must not have. \
         Read back: {alerts}"
    );

    // ── Boot #2: make the CLASSIFIED durable write, shut down cleanly ────────
    // `memorize` sits in `READ_ONLY_DENIED_TOOLS`, so the actor classifies the
    // turn a mutation and publishes CURRENT on the turn it acks. It writes a
    // `.light.md` under the runtime root AND ingests it, so the commitment
    // lands in the graph itself. It is also the only mutating verb in that list
    // a plain MCP client can still reach — see the boundary note at the top.
    let memorized = owner2.call(
        "memorize",
        serde_json::json!({
            "agent_id": LIFECYCLE_AGENT,
            "node_label": LIFECYCLE_MEMORY_LABEL,
            "claims": [ { "label": LIFECYCLE_MEMORY_LABEL, "text": "knowledge that must cross a restart" } ]
        }),
    );
    assert_eq!(
        memorized.get("ingested").and_then(|value| value.as_bool()),
        Some(true),
        "memorize must write its light file AND ingest it — an un-ingested \
         memory never reaches the graph this gate follows: {memorized}"
    );
    let after_memorize = owner2.node_count();
    assert!(
        after_memorize > rebooted,
        "memorize must actually grow the graph it will be asked to remember: \
         {rebooted} -> {after_memorize}"
    );
    owner2.shutdown();

    // ── Boot #3: the classified write is still there ─────────────────────────
    let mut owner3 = Owner::spawn_logging_stderr(&cwd, &runtime_dir, &boot3_stderr);
    let final_nodes = owner3.node_count();
    assert!(
        final_nodes >= after_memorize,
        "the third boot must serve what the second one memorized: booted with \
         {final_nodes} nodes, the checkpointed graph had {after_memorize}"
    );
    let found = owner3.call(
        "search",
        serde_json::json!({
            "agent_id": LIFECYCLE_AGENT,
            "query": LIFECYCLE_MEMORY_LABEL,
            "mode": "literal",
            "top_k": 20
        }),
    );
    // Look inside `results` and nowhere else: `search` echoes the query back in
    // its own `query` field, so matching the whole payload would pass on zero
    // hits.
    let hits = found
        .get("results")
        .and_then(|results| results.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter(|entry| entry.to_string().contains(LIFECYCLE_MEMORY_LABEL))
                .count()
        })
        .unwrap_or(0);
    assert!(
        hits > 0,
        "the CLASSIFIED durable write must survive the restart: `memorize` \
         acked {LIFECYCLE_MEMORY_LABEL} before a clean shutdown and the \
         rebooted brain cannot find it. Read back: {found}"
    );
    owner3.shutdown();

    // ── ZERO sidecar refusals on every boot that had prior state to read ─────
    // The strongest observable available: the owner's own stderr, which is
    // where each of the three 2026-07-27 defects announced itself — or, for
    // #441, announced the graph it was about to throw away.
    for (label, log) in [
        ("second", read_stderr(&boot2_stderr)),
        ("third", read_stderr(&boot3_stderr)),
    ] {
        // An absence check over an empty file proves nothing. Pin that this log
        // is the real one, from a boot that actually came up, first.
        assert!(
            log.contains("Server ready."),
            "the {label} boot's captured stderr must carry its ready banner, \
             otherwise the refusal scan below is checking an empty file. Log \
             was:\n{log}"
        );
        for signature in SIDECAR_REFUSAL_SIGNATURES {
            assert!(
                !log.contains(signature),
                "the {label} boot refused or degraded a sidecar it wrote itself \
                 ({signature:?}). A warm boot over its own clean checkpoint must \
                 adopt every sidecar exactly. stderr was:\n{log}"
            );
        }
    }
}
