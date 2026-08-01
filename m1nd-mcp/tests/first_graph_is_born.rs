//! THE FIRST-VALUE PATH — a virgin repo must be able to end up with a graph.
//!
//! THE DEFECT this file was born for, measured on 1.6.2 in a new repo with an
//! empty runtime, and reproduced here end to end:
//!
//!   1. An agent could not ingest, even on an empty graph. A plain
//!      `m1nd-mcp --stdio` calling `ingest` with only `agent_id` was refused
//!      `generic_action_authority_required: semantic_action=graph.ingest.replace
//!      authority_floor=POSITIVE_SOVEREIGN` — correct policy (the first graph is
//!      the human's gesture, `docs/GENESIS-INGEST-CONSUMERS-SPEC.md` §2) told in
//!      a way that named no way out.
//!   2. The human's ceremony reported success and delivered nothing. `m1nd init
//!      --birth <repo>` exited 0 printing a node count, and the very next stdio
//!      session in that repo still served **0 nodes**: the ceremony minted a
//!      project-brain sidecar under `<runtime>/project-brains/…`, which only the
//!      served owner's HTTP routing reaches, while the graph a plain stdio owner
//!      serves — the runtime's own — stayed empty.
//!
//! Together those two are the whole product having no first-value path: neither
//! actor in the room could produce a populated graph by any route.
//!
//! WHY THIS IS A SUBPROCESS TEST AND NOT A UNIT TEST. The defect is that a PATH
//! is dead, and only a path test can prove a path. Three of its links exist
//! nowhere else: the `--birth` CLI ingress is the ONLY construction site of a
//! `HumanOrigin` (an in-process call cannot even build the stamp), the refusal in
//! step 1 is produced by the transport's floor gate, and the durability of the
//! result is decided by a SECOND boot reconciling its checkpoint — the exact
//! seam where a graph written behind `CURRENT` gets reverted on the next boot
//! (`legacy_snapshot_adoption`'s founding incident). Boot #1 here exists to
//! publish that empty `CURRENT`, so the ceremony has something to be reverted by.
//!
//! Unix-only for the same reason as its sibling `persist_runtime_root.rs`: the
//! harness drives the real binary through POSIX process plumbing. The decision
//! logic itself (which door a ceremony takes) is covered cross-platform by the
//! unit tests in `brain_birth.rs`.
#![cfg(unix)]

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

/// Path to the compiled binary under test (Cargo sets `CARGO_BIN_EXE_<name>`).
const BIN: &str = env!("CARGO_BIN_EXE_m1nd-mcp");

/// The agent that stands in the repo before the human has run anything.
const AGENT: &str = "first-value-probe";

/// The command every refusal on this path must name. An agent that is refused
/// and not told the way out concludes the product cannot be used — which is
/// exactly what happened in the field.
const THE_DOOR: &str = "m1nd init --birth";

/// A tiny two-file Rust crate: small, deterministic, and enough for the code
/// extractor to produce a real graph.
fn write_fixture_repo(root: &Path) {
    std::fs::create_dir_all(root.join("src")).expect("mk src");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"repo-alpha\"\nversion = \"0.0.0\"\n",
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

/// A live stdio JSON-RPC connection to a spawned owner process — the same shape
/// `persist_runtime_root.rs` drives, kept minimal here.
struct Owner {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
}

impl Owner {
    /// Spawn the binary exactly as a solo user's MCP host does: stdio, no GUI,
    /// a runtime dir inside their own repo, cwd = the repo.
    fn spawn(repo: &Path, runtime_dir: &Path) -> Owner {
        let mut child = Command::new(BIN)
            .arg("--stdio")
            .arg("--no-gui")
            .current_dir(repo)
            .env("M1ND_RUNTIME_DIR", runtime_dir)
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

    fn request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        self.next_id += 1;
        let id = self.next_id;
        let line = serde_json::json!({
            "jsonrpc": "2.0", "id": id, "method": method, "params": params
        })
        .to_string();
        self.stdin
            .write_all(line.as_bytes())
            .expect("write request");
        self.stdin.write_all(b"\n").expect("write newline");
        self.stdin.flush().expect("flush request");

        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for reply id={id}"
            );
            let mut buffer = String::new();
            let read = self.stdout.read_line(&mut buffer).expect("read reply line");
            assert!(
                read != 0,
                "owner stdout closed before reply id={id} — it died instead of answering"
            );
            let Ok(value) = serde_json::from_str::<serde_json::Value>(buffer.trim()) else {
                continue;
            };
            if value.get("id").and_then(|value| value.as_i64()) == Some(id) {
                return value;
            }
        }
    }

    fn initialize(&mut self) {
        let reply = self.request(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "first-value-probe", "version": "1.0" }
            }),
        );
        assert!(
            reply.get("result").is_some(),
            "initialize must succeed, got {reply}"
        );
    }

    /// Call a tool and return the structured payload (or the raw result).
    fn call(&mut self, tool: &str, arguments: serde_json::Value) -> serde_json::Value {
        let reply = self.request(
            "tools/call",
            serde_json::json!({ "name": tool, "arguments": arguments }),
        );
        let result = reply
            .get("result")
            .unwrap_or_else(|| panic!("tool {tool} returned no result: {reply}"))
            .clone();
        if let Some(structured) = result.get("structuredContent") {
            return structured.clone();
        }
        if let Some(text) = result
            .get("content")
            .and_then(|content| content.as_array())
            .and_then(|items| items.first())
            .and_then(|item| item.get("text"))
            .and_then(|text| text.as_str())
        {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) {
                return parsed;
            }
            return serde_json::json!({ "text": text });
        }
        result
    }

    fn node_count(&mut self) -> u64 {
        let health = self.call("health", serde_json::json!({ "agent_id": AGENT }));
        health
            .get("node_count")
            .and_then(|count| count.as_u64())
            .unwrap_or_else(|| panic!("no node_count in health body: {health}"))
    }

    fn shutdown(mut self) {
        drop(self.stdin);
        let status = self.child.wait().expect("wait for graceful owner shutdown");
        assert!(status.success(), "owner shutdown failed with {status}");
    }
}

/// Run the human's ceremony through the REAL CLI ingress — the only door that
/// can stamp `human-cli`. Returns its parsed JSON answer and its exit code.
fn run_birth_ceremony(repo: &Path, runtime_dir: &Path) -> (serde_json::Value, i32) {
    let output = Command::new(BIN)
        .arg("--birth")
        .arg(repo)
        .current_dir(repo)
        .env("M1ND_RUNTIME_DIR", runtime_dir)
        .env("M1ND_REGISTRY_DIR", runtime_dir.join("registry"))
        .env("M1ND_NO_GUI", "1")
        .output()
        .expect("run the birth ceremony");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let payload =
        serde_json::from_str::<serde_json::Value>(stdout.trim()).unwrap_or_else(|error| {
            panic!(
                "the ceremony must answer in JSON on stdout ({error}). stdout was:\n{stdout}\n\
             stderr was:\n{}",
                String::from_utf8_lossy(&output.stderr)
            )
        });
    (payload, output.status.code().unwrap_or(-1))
}

#[test]
fn a_virgin_repo_ends_up_with_a_populated_graph() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo-alpha");
    write_fixture_repo(&repo);
    // The solo topology from the README's own quickstart: the runtime lives
    // inside the repo it maps.
    let runtime_dir = repo.join(".m1nd");
    std::fs::create_dir_all(&runtime_dir).expect("mk runtime dir");

    // ── 1 · FIRST CONTACT: the agent stands in the repo and tries to ingest ──
    // It must be refused (the first graph is the human's gesture) — and the
    // refusal must NAME THE DOOR. On the defect it named none, so the agent
    // concluded the product could not be used.
    let mut agent = Owner::spawn(&repo, &runtime_dir);
    assert_eq!(
        agent.node_count(),
        0,
        "a virgin runtime must start with an empty graph, or this test is not \
         measuring first contact"
    );
    let refused = agent.call("ingest", serde_json::json!({ "agent_id": AGENT }));
    let refused_text = serde_json::to_string(&refused).expect("encode refusal");
    assert!(
        refused_text.contains("generic_action_authority_required"),
        "the agent's own ingest must stay refused — minting a brain is the \
         human's gesture and this test must never be the thing that opens it. \
         Got: {refused_text}"
    );
    assert!(
        refused_text.contains(THE_DOOR),
        "the refusal must name the way out ({THE_DOOR}). A refusal that is \
         correct and names no door is half a refusal: the agent that measured \
         this defect hit four of them and concluded the product was unusable. \
         Got: {refused_text}"
    );
    // Boot #1 shuts down cleanly, publishing an EMPTY `CURRENT`. That is not
    // scaffolding: it is what a graph written behind the checkpoint gets
    // reverted BY on the next boot.
    agent.shutdown();

    // ── 2 · THE HUMAN'S CEREMONY: it must ingest for real ────────────────────
    let (payload, exit_code) = run_birth_ceremony(&repo, &runtime_dir);
    let node_count = payload
        .get("node_count")
        .and_then(|count| count.as_u64())
        .unwrap_or(0);
    assert_eq!(
        payload.get("ok").and_then(|ok| ok.as_bool()),
        Some(true),
        "the ceremony must succeed on a virgin repo: {payload}"
    );
    assert_eq!(
        exit_code, 0,
        "a successful ceremony exits 0; got {exit_code} for {payload}"
    );
    assert!(
        node_count > 0,
        "the ceremony must never report success over an empty graph — that is \
         the exact dishonesty this test exists for: {payload}"
    );

    // ── 3 · THE PROOF: the next session in that repo READS the graph ─────────
    // This is the assertion the whole file is for. On the defect the ceremony
    // filled a project-brain sidecar that only the served owner's HTTP routing
    // reaches, and this boot served 0 nodes.
    let mut after = Owner::spawn(&repo, &runtime_dir);
    let served = after.node_count();
    assert!(
        served >= node_count,
        "the session that follows the ceremony must SERVE the graph the \
         ceremony reported: the ceremony said {node_count} nodes and this boot \
         serves {served}. Serving fewer is the defect in full — a ceremony that \
         succeeds loudly and leaves the agent with an empty brain."
    );

    // …and the orientation packet must stop asking for an ingest nobody can run.
    let north = after.call(
        "north",
        serde_json::json!({ "agent_id": AGENT, "task": "map this repo" }),
    );
    assert_ne!(
        north.get("needs").and_then(|needs| needs.as_str()),
        Some("needs_ingest"),
        "after the ceremony the orientation packet must stop reporting \
         needs_ingest — the graph is right there: {north}"
    );
    after.shutdown();
}

#[test]
fn a_second_ceremony_on_a_born_repo_refuses_and_says_you_are_home() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo-beta");
    write_fixture_repo(&repo);
    let runtime_dir = repo.join(".m1nd");
    std::fs::create_dir_all(&runtime_dir).expect("mk runtime dir");

    let (first, first_exit) = run_birth_ceremony(&repo, &runtime_dir);
    assert_eq!(
        first.get("ok").and_then(|ok| ok.as_bool()),
        Some(true),
        "the first ceremony must succeed: {first}"
    );
    assert_eq!(first_exit, 0, "the first ceremony exits 0: {first}");

    // A second run must not mint anything, must not re-scan behind the human's
    // back, and must say plainly that this repo already has its brain.
    let (second, second_exit) = run_birth_ceremony(&repo, &runtime_dir);
    assert_eq!(
        second.get("ok").and_then(|ok| ok.as_bool()),
        Some(false),
        "a repo that already has its brain must not be born twice: {second}"
    );
    assert_eq!(
        second.get("refused").and_then(|code| code.as_str()),
        Some("birth_root_is_bound_graph"),
        "the refusal must be the 'you are home' one: {second}"
    );
    assert_eq!(
        second_exit, 1,
        "a refusal exits 1 so a script sees the difference: {second}"
    );

    // The graph the first ceremony built is untouched by the refusal.
    let mut owner = Owner::spawn(&repo, &runtime_dir);
    let served = owner.node_count();
    assert!(
        served > 0,
        "the refused second ceremony must leave the born graph intact, and this \
         boot serves {served} nodes"
    );
    owner.shutdown();
}

/// The layout where the runtime IS the repo root, not a `.m1nd` inside it.
///
/// It earns its own case because it is the one place the two "am I home?"
/// questions disagree. With the graph path directly under the repo, the empty
/// binding's `workspace_root` falls back to that parent — the repo itself — so
/// `covers_root` answers YES over a brain that holds nothing. A ceremony gated on
/// coverage would refuse "this repo already has its brain" while reporting zero
/// nodes, which is the same dishonesty this file exists to kill. The gate is the
/// node count, and this walk proves it end to end.
#[test]
fn a_runtime_at_the_repo_root_is_home_too_and_its_empty_graph_is_filled() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let repo = tmp.path().join("repo-gamma");
    write_fixture_repo(&repo);

    let (payload, exit_code) = run_birth_ceremony(&repo, &repo);
    assert_eq!(
        payload.get("ok").and_then(|ok| ok.as_bool()),
        Some(true),
        "an empty brain is an empty brain wherever its runtime sits: {payload}"
    );
    assert_eq!(exit_code, 0, "a successful ceremony exits 0: {payload}");
    assert_eq!(
        payload.get("brain").and_then(|brain| brain.as_str()),
        Some("owner_bound_graph"),
        "the runtime lives at the named root, so the graph it fills is its own: {payload}"
    );

    let mut after = Owner::spawn(&repo, &repo);
    let served = after.node_count();
    assert!(
        served > 0,
        "the session that follows must serve the graph the ceremony built, and it \
         serves {served} nodes"
    );
    after.shutdown();
}
