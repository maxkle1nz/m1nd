//! The fixed tax every session pays before its first m1nd call.
//!
//! A host injects the server `instructions` and the `tools/list` schemas into
//! the agent's context when it connects, whether or not the agent ever calls
//! m1nd. Measured on 2026-11-20 they were 24,179 and 24,036 characters: about
//! 12k tokens before the first call, enough that a cold session with m1nd
//! connected cost more than the same session using plain text search.
//!
//! The first diet (2026-11-20) brought them to 4,951 and 9,899. A cold-agent
//! experiment the same day still measured the card at about 5.9k tokens of the
//! gap between a session with m1nd and one without, so the whole card is now
//! held to 2,500 tokens: 10,000 characters for both together.
//!
//! This drives the real binary over stdio and measures exactly what a host
//! receives on the default menu. Instructions are counted in characters. Tool
//! schemas are counted with the ruler the budget was registered with: JSON with
//! `", "` / `": "` separators and every non-ASCII character escaped to `\uXXXX`
//! (Python's default `json.dumps`), which never undercounts the compact wire.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

/// The server instructions: a short operating card, not the whole doctrine.
const INSTRUCTIONS_BUDGET_CHARS: usize = 3_000;
/// Every advertised schema together.
const TOOLS_LIST_BUDGET_CHARS: usize = 7_000;
/// The whole card a session pays before its first call: instructions and
/// `tools/list` together, about 2,500 tokens.
const SESSION_CARD_BUDGET_CHARS: usize = 10_000;
/// Any single advertised schema.
const TOOL_BUDGET_CHARS: usize = 1_200;

struct Owner {
    child: Child,
    stdin: Option<ChildStdin>,
    replies: Receiver<Value>,
    next_id: u64,
}

impl Owner {
    fn spawn(runtime: &Path) -> Self {
        // An existing empty model directory disables embeddings without a download.
        let no_model = runtime.join("no-model-needed");
        std::fs::create_dir_all(&no_model).expect("empty model dir");
        let mut child = Command::new(env!("CARGO_BIN_EXE_m1nd-mcp"))
            .args(["--stdio", "--no-gui", "--runtime-dir"])
            .arg(runtime)
            .current_dir(runtime)
            .env("M1ND_RUNTIME_DIR", runtime)
            .env("M1ND_RUNTIME_BASE", runtime)
            .env("M1ND_GRAPH_SOURCE", runtime.join("graph_snapshot.json"))
            .env(
                "M1ND_PLASTICITY_STATE",
                runtime.join("plasticity_state.json"),
            )
            .env("M1ND_REGISTRY_DIR", runtime.join("registry"))
            .env("M1ND_EMBED_MODEL", &no_model)
            .env("M1ND_NO_GUI", "1")
            // The default menu is what a newcomer's host serves.
            .env_remove("M1ND_TOOL_TIER")
            .env_remove("M1ND_WORKSPACE_ROOT")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn m1nd-mcp stdio owner");
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("owner stdout");
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
        Self {
            child,
            stdin,
            replies,
            next_id: 0,
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        let stdin = self.stdin.as_mut().expect("owner stdin");
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
        )
        .expect("write request");
        stdin.flush().expect("flush request");
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let reply = self
                .replies
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|error| panic!("owner did not answer {method}: {error}"));
            if reply["id"] == id {
                return reply;
            }
        }
    }

    fn shutdown(mut self) {
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(status) = self.child.try_wait().expect("poll owner") {
                assert!(status.success(), "owner shutdown failed with {status}");
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

/// Length of `value` rendered as Python's default `json.dumps` renders it.
fn ruler_len(value: &Value) -> usize {
    match value {
        Value::Null => 4,
        Value::Bool(true) => 4,
        Value::Bool(false) => 5,
        Value::Number(number) => number.to_string().len(),
        Value::String(text) => {
            2 + text
                .chars()
                .map(|ch| match ch {
                    '"' | '\\' | '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' => 2,
                    ' '..='~' => 1,
                    ch if (ch as u32) > 0xFFFF => 12,
                    _ => 6,
                })
                .sum::<usize>()
        }
        Value::Array(items) => {
            2 + items.iter().map(ruler_len).sum::<usize>() + 2 * items.len().saturating_sub(1)
        }
        Value::Object(map) => {
            2 + map
                .iter()
                .map(|(key, item)| ruler_len(&Value::String(key.clone())) + 2 + ruler_len(item))
                .sum::<usize>()
                + 2 * map.len().saturating_sub(1)
        }
    }
}

#[test]
fn ruler_matches_the_registered_measurement() {
    // Python prints the ASCII sample exactly as written here.
    let ascii = json!({"a": [1, true, null], "b": "x"});
    assert_eq!(
        ruler_len(&ascii),
        r#"{"a": [1, true, null], "b": "x"}"#.len()
    );
    // A BMP character becomes a six-character escape; an astral one becomes a
    // surrogate pair of two (`json.dumps("\u2014")` is 8 characters long).
    assert_eq!(ruler_len(&json!("\u{2014}")), 8);
    assert_eq!(ruler_len(&json!("\u{1d53b}")), 14);
    assert_eq!(ruler_len(&json!("a\n\"b")), 8);
}

#[test]
fn a_session_pays_a_bounded_tax_before_its_first_call() {
    let runtime = tempfile::tempdir().expect("runtime dir");
    let mut owner = Owner::spawn(runtime.path());

    let init = owner.request(
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "session-tax-probe", "version": "1"}
        }),
    );
    let instructions = init["result"]["instructions"]
        .as_str()
        .unwrap_or_else(|| panic!("initialize must carry instructions: {init}"))
        .to_string();
    let listed = owner.request("tools/list", json!({}));
    let tools = listed["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("tools/list must carry tools: {listed}"))
        .clone();
    owner.shutdown();

    let instructions_chars = instructions.chars().count();
    let total = ruler_len(&Value::Array(tools.clone()));
    let mut per_tool: Vec<(usize, String)> = tools
        .iter()
        .map(|tool| {
            (
                ruler_len(tool),
                tool["name"].as_str().unwrap_or("?").to_string(),
            )
        })
        .collect();
    per_tool.sort_by(|a, b| b.cmp(a));
    let table = per_tool
        .iter()
        .map(|(size, name)| format!("{name} {size}"))
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!(
        "SESSION_TAX instructions={instructions_chars} tools_list={total} tools={} [{table}]",
        tools.len()
    );

    assert!(
        instructions_chars <= INSTRUCTIONS_BUDGET_CHARS,
        "the server instructions are {instructions_chars} characters; the session card \
         budget is {INSTRUCTIONS_BUDGET_CHARS}. Move detail behind `help` (pull, never push)."
    );
    assert!(
        total <= TOOLS_LIST_BUDGET_CHARS,
        "tools/list is {total} characters; the budget is {TOOLS_LIST_BUDGET_CHARS}. \
         Per tool: {table}"
    );
    assert!(
        instructions_chars + total <= SESSION_CARD_BUDGET_CHARS,
        "the session card is {} characters (instructions {instructions_chars} + tools/list \
         {total}); the budget is {SESSION_CARD_BUDGET_CHARS}",
        instructions_chars + total
    );
    let over: Vec<&(usize, String)> = per_tool
        .iter()
        .filter(|(size, _)| *size > TOOL_BUDGET_CHARS)
        .collect();
    assert!(
        over.is_empty(),
        "no advertised schema may exceed {TOOL_BUDGET_CHARS} characters: {over:?}"
    );
}
