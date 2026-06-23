// === crates/m1nd-mcp/src/server.rs ===

use crate::auto_ingest;
use crate::help_guidance;
use crate::instance_registry::InstanceHandle;
use crate::layer_handlers;
use crate::mission_handlers;
use crate::personality;
use crate::protocol::layers;
use crate::protocol::*;
use crate::report_handlers;
use crate::search_handlers;
use crate::session::SessionState;
use crate::surgical_handlers;
use crate::tools;
use crate::universal_docs;
use m1nd_core::domain::DomainConfig;
use m1nd_core::error::{M1ndError, M1ndResult};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// MCP protocol instructions — injected into initialize response so agents
// automatically understand how to use m1nd effectively.
// ---------------------------------------------------------------------------
const M1ND_INSTRUCTIONS: &str = "\
m1nd is a neuro-symbolic code graph engine. It ingests codebases into a weighted \
graph and provides spreading-activation queries, impact analysis, prediction, and \
stateful perspective navigation. All tool calls require an `agent_id` parameter.

## WORKFLOWS

**Session Start**: `trust_selftest` → `recovery_playbook` if trust is not full \
or retrieval looks blocked → `ingest` if needed → `seek`/`audit`. Use `session_handshake` \
for cheaper host-surface classification and `doctor` when the playbook asks for \
deeper diagnosis of a degraded host surface, empty graph, or stale-looking binding. \
This gives you codebase-aware context.

**Research**: `ingest` → `activate(query)` → `why(source, target)` → `missing(topic)` → \
`learn(feedback)` → `memorize` any durable finding. Use `seek` for keyword search, \
`scan` for broad discovery, `trace` for dependency chains, `timeline` for temporal ordering.

**Code Change**: `impact(node)` (blast radius) → `predict(node)` (co-change likelihood; \
run `ghost_edges` first to load the git co-change matrix, else predict has only \
structural fallback) → `counterfactual(nodes)` (simulate removal) → \
`warmup(task_description)` (prime graph) → `memorize` the decision and why (with \
`evidence` paths). Use `differential` to compare two subgraphs. `hypothesize` to test what-ifs.

**Deep Analysis**: `fingerprint(nodes)` for duplicate/equivalence detection. \
`diverge(baseline)` detects structural drift between a baseline (ISO date, git ref, or \
last_session) and the current graph. `federate` to query across graph namespaces.

**Memory (compounding, cross-session)**: when you conclude something durable — a \
decision, a verified finding, an undecided design point, why code is the way it is — \
persist it with `memorize`. Pass structured claims with `confidence` and, crucially, \
`evidence` paths to the code that backs each claim. `memorize` writes a graph-native \
`.light.md`, ingests it, and anchors every evidence path to the real code node, so the \
knowledge lives in the same activation space as code and surfaces in `seek`/`activate`. \
It auto-loads on every future session start (reported in `session_handshake.agent_memory`). \
Later, `cross_verify(check:[\"evidence_freshness\"])` flags any claim whose cited code has \
changed — so the memory tells you when it has gone stale instead of misleading you. \
Closing a mission? Pass `write_light_memory:true` to `mission_close` to persist its \
verified claims the same way in one step.

## PERSPECTIVE SYSTEM (stateful navigation)

Perspectives are named, agent-scoped navigation sessions through the graph. \
Flow: `perspective_start(name, seed_nodes)` → `perspective_follow(node)` (move focus) → \
`perspective_branch(name)` (fork exploration) → `perspective_back` (undo last move) → \
`perspective_close`. Use `perspective_inspect` to see current state, `perspective_peek` \
to look at a node without moving, `perspective_list` for all open perspectives, \
`perspective_compare` to diff two perspectives, `perspective_suggest` for next-step \
recommendations, `perspective_routes` for paths between nodes, `perspective_affinity` \
for related-node scoring.

## CONCURRENCY & STATE

`lock_create` / `lock_release` — advisory locks for multi-agent coordination on graph \
regions. `lock_watch` monitors lock state. `lock_diff` shows changes within a lock scope. \
`lock_rebase` replays external changes into a locked region. \
`trail_save` / `trail_list` / `trail_resume` / `trail_merge` — persist and restore \
exploration trails across sessions. `validate_plan` checks a proposed multi-step plan \
for structural soundness.

## CRITICAL PATTERNS

1. **Always call `learn` after using `activate` results.** Feedback (correct/wrong/partial) \
trains the graph weights via Hebbian learning. Skipping this degrades future queries.
2. **Use `ingest` at session start** if the graph has zero nodes or the codebase changed.
3. **Use `drift` to recover context** between sessions — it shows weight changes since \
a baseline timestamp.
4. **`warmup` before focused work** — primes activation patterns for a specific task, \
making subsequent queries faster and more relevant.
5. **Never call `activate` without `agent_id`** — multi-agent isolation depends on it.
6. **Prefer `impact` over `activate` for code changes** — impact gives directional \
blast-radius analysis; activate gives associative exploration.
7. **Graph persists automatically** every 50 queries and on shutdown. Use `trail_save` \
for explicit exploration checkpoints.
8. **Use `boot_memory` for small canonical doctrine/state** that should persist quickly \
and stay hot in runtime memory without polluting trails or transcripts.
9. **If `tools/list` is missing recovery tools such as `ingest`, call `doctor` with \
`observed_tool=\"tools/list\"`, `observed_tool_count`, `available_tools`, and \
`missing_tools`. Treat the host surface as degraded until it is rebound; use direct \
repo reads for final truth when m1nd cannot re-ingest from the current session.**
10. **Persist durable conclusions with `memorize`** (or `mission_close write_light_memory:true`) \
before ending work or a mission. Knowledge with `evidence` paths anchors to code, auto-loads \
next session, and self-flags as stale via `cross_verify(check:[\"evidence_freshness\"])` when \
that code changes. This is how findings compound across sessions instead of being lost.
";

/// Stdio MCP framing mode auto-detected on the inbound stream. The matching
/// outbound write MUST use the same mode so the host's framing assumptions hold.
/// Exposed `pub` so the `--attach` stdio↔HTTP bridge reuses the exact same
/// framing primitives instead of hand-rolling a divergent encoder.
#[derive(Clone, Copy, Debug)]
pub enum TransportMode {
    Framed,
    Line,
}

/// Read one JSON-RPC payload from `reader`, auto-detecting Content-Length framing
/// vs newline framing, and report which mode was seen so the response can be
/// written back in the same framing. `Ok(None)` signals EOF. Reused verbatim by
/// the `--attach` bridge — see `attach_client.rs`.
pub fn read_request_payload<R: BufRead>(
    reader: &mut R,
) -> std::io::Result<Option<(String, TransportMode)>> {
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(None);
        }

        let first_non_ws = buffer
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace());
        let starts_framed = matches!(first_non_ws, Some(byte) if byte != b'{' && byte != b'[');
        if starts_framed {
            let mut content_length: Option<usize> = None;
            loop {
                let mut header_line = String::new();
                let bytes = reader.read_line(&mut header_line)?;
                if bytes == 0 {
                    return Ok(None);
                }
                let trimmed = header_line.trim_end_matches(['\r', '\n']);
                if trimmed.is_empty() {
                    break;
                }
                if let Some((name, value)) = trimmed.split_once(':') {
                    if name.trim().eq_ignore_ascii_case("Content-Length") {
                        content_length = value.trim().parse::<usize>().ok();
                    }
                }
            }

            let length = content_length.ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Missing Content-Length header",
                )
            })?;
            let mut body = vec![0_u8; length];
            reader.read_exact(&mut body)?;
            let payload = String::from_utf8(body)
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
            return Ok(Some((payload, TransportMode::Framed)));
        }

        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return Ok(Some((trimmed.to_owned(), TransportMode::Line)));
    }
}

/// Write a `JsonRpcResponse` to `writer` in the given framing mode (the one
/// detected by [`read_request_payload`] for the matching request). Reused
/// verbatim by the `--attach` bridge so its stdout framing is byte-identical to
/// the embedded stdio server's.
pub fn write_response<W: Write>(
    writer: &mut W,
    response: &JsonRpcResponse,
    mode: TransportMode,
) -> std::io::Result<()> {
    let json = serde_json::to_string(response).unwrap_or_default();
    match mode {
        TransportMode::Framed => {
            write!(writer, "Content-Length: {}\r\n\r\n{}", json.len(), json)?;
        }
        TransportMode::Line => {
            writeln!(writer, "{}", json)?;
        }
    }
    writer.flush()
}

// ---------------------------------------------------------------------------
// McpConfig — server configuration
// Replaces: 03-MCP Section 1.2 initialization config
// ---------------------------------------------------------------------------

/// MCP server configuration.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct McpConfig {
    pub graph_source: PathBuf,
    pub plasticity_state: PathBuf,
    #[serde(default)]
    pub runtime_dir: Option<PathBuf>,
    #[serde(default)]
    pub registry_dir: Option<PathBuf>,
    pub auto_persist_interval: u32,
    pub learning_rate: f32,
    pub decay_rate: f32,
    pub xlr_enabled: bool,
    pub max_concurrent_reads: usize,
    pub write_queue_size: usize,
    /// Domain name: "code" (default), "music", or "generic".
    /// Controls temporal decay half-lives and relation types.
    #[serde(default)]
    pub domain: Option<String>,
    /// Attach read-only: the session loads the snapshot and serves queries but
    /// never persists to disk and never holds an exclusive lease. Mutation tools
    /// are disabled. Set via `--read-only` CLI flag or `M1ND_READ_ONLY=1`.
    #[serde(default)]
    pub read_only: bool,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            graph_source: PathBuf::from("./graph_snapshot.json"),
            plasticity_state: PathBuf::from("./plasticity_state.json"),
            runtime_dir: None,
            registry_dir: None,
            auto_persist_interval: 50,
            learning_rate: 0.08,
            decay_rate: 0.005,
            xlr_enabled: true,
            max_concurrent_reads: 32,
            write_queue_size: 64,
            domain: None,
            read_only: false,
        }
    }
}

// ---------------------------------------------------------------------------
// McpServer — JSON-RPC stdio server
// Replaces: 03-MCP Section 1.1 deployment model
// ---------------------------------------------------------------------------

/// MCP server over JSON-RPC stdio. Single process, shared PropertyGraph.
/// Replaces: 03-MCP server architecture
pub struct McpServer {
    config: McpConfig,
    state: SessionState,
    daemon_runtime: Option<DaemonRuntimeControl>,
}

#[derive(Debug)]
enum ServerEvent {
    Request(String, TransportMode),
    StdinClosed,
    WatchNotice,
    WatchError(String),
}

struct LiveDaemonWatcher {
    _watcher: RecommendedWatcher,
    dropped_counter: Arc<AtomicU64>,
}

struct DaemonRuntimeControl {
    event_tx: mpsc::SyncSender<ServerEvent>,
    watcher: Option<LiveDaemonWatcher>,
}

// ---------------------------------------------------------------------------
// Tool tier gate
// ---------------------------------------------------------------------------

/// The curated ESSENTIAL tool set (~25 tools) advertised by default.
///
/// These are the high-frequency tools agents need for orientation, trust, and
/// everyday graph queries. All other tools are "advanced" and are hidden from
/// `tools/list` unless `M1ND_TOOL_TIER=full` is set — they remain fully
/// callable via `tools/call` dispatch at all times.
///
/// To expose everything, set `M1ND_TOOL_TIER=full` in the MCP environment.
pub const ESSENTIAL_TOOLS: &[&str] = &[
    "trust_selftest",
    "session_handshake",
    "orient",
    "am_i_stale",
    "recovery_playbook",
    "health",
    "doctor",
    "help",
    "ingest",
    "audit",
    "search",
    "seek",
    "activate",
    "learn",
    "glob",
    "view",
    "batch_view",
    "impact",
    "why",
    "trace",
    "predict",
    "validate_plan",
    "surgical_context_v2",
    "cross_verify",
    "mission_start",
    "mission_next",
    "mission_close",
    "persist",
    "memorize",
];

/// Returns the active tool tier based on the `M1ND_TOOL_TIER` env var.
///
/// - Unset or `essential` (case-insensitive) → `"essential"` (curated ~25)
/// - `full` (case-insensitive) → `"full"` (all 102 tools)
/// - Any unrecognized value → defaults to `"essential"`
pub fn active_tool_tier() -> &'static str {
    match std::env::var("M1ND_TOOL_TIER")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "full" => "full",
        _ => "essential",
    }
}

/// Whether the additive `_m1nd` response envelope is attached to tool results.
///
/// Gated by `M1ND_RESPONSE_ENVELOPE`, default ON. Only an explicit `"0"` or
/// `"false"` (case-insensitive) disables it; any other value (or unset) is ON.
pub fn response_envelope_enabled() -> bool {
    match std::env::var("M1ND_RESPONSE_ENVELOPE") {
        Ok(v) => {
            let v = v.trim().to_ascii_lowercase();
            v != "0" && v != "false"
        }
        Err(_) => true,
    }
}

/// Whether the M1ND_PROOF_GATE write guard is active.
///
/// Opt-in safety flag (default OFF), mirroring the `M1ND_READ_ONLY` parsing:
/// any value other than `"0"`/`"false"`/empty turns it ON; unset is OFF. When
/// ON, code-writing tools (`apply`/`apply_batch`/`edit_commit`) are refused at
/// dispatch unless the agent has already driven each target to
/// `proof_state == "ready_to_edit"` (via `surgical_context_v2`) this session.
pub fn proof_gate_enabled() -> bool {
    std::env::var("M1ND_PROOF_GATE")
        .map(|v| v != "0" && v != "false" && !v.is_empty())
        .unwrap_or(false)
}

/// Returns ALL registered MCP tool schemas regardless of tier.
/// Use this when you always need the full 102-tool registry (e.g., health
/// contract counts, internal tests that verify advanced tool registration).
pub fn all_tool_schemas() -> serde_json::Value {
    all_tool_schemas_inner()
}

/// Returns the tier-gated tool list for `tools/list` advertisement.
///
/// With `M1ND_TOOL_TIER=full` → returns all tools.
/// Otherwise → returns only the ESSENTIAL_TOOLS curated set.
/// Hidden tools remain callable via `tools/call` dispatch (handlers untouched).
pub fn tool_schemas() -> serde_json::Value {
    tool_schemas_for_tier(active_tool_tier())
}

/// Returns the tool list for the given tier string.
/// Used internally and by tests to avoid env-var races.
/// `tier`: "full" → all tools; anything else → essential set only.
pub fn tool_schemas_for_tier(tier: &str) -> serde_json::Value {
    let all = all_tool_schemas_inner();
    if tier.eq_ignore_ascii_case("full") {
        return all;
    }
    // Filter to essential set only
    let essential_set: std::collections::HashSet<&str> = ESSENTIAL_TOOLS.iter().copied().collect();
    let filtered: Vec<serde_json::Value> = all["tools"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|tool| {
            tool.get("name")
                .and_then(|n| n.as_str())
                .map(|name| essential_set.contains(name))
                .unwrap_or(false)
        })
        .collect();
    serde_json::json!({ "tools": filtered })
}

/// Internal: the complete static tool registry (all 102 tools). Never filtered.
fn all_tool_schemas_inner() -> serde_json::Value {
    serde_json::json!({
        "tools": [
            {
                "name": "orient",
                "description": "Boot into a task in one call. Give your free-form task and get your STARTING CONTEXT pre-packed: the focus nodes the task activates (ranked), prior memorized conclusions nearby, the global PageRank attention backbone, coverage so far, and the concrete first calls to make. Call this FIRST when you receive a task instead of doing exploratory reads. Read-only safe.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "task": { "type": "string", "description": "Free-form description of the task you are about to start. The graph spread-activates on this text to find your starting context." },
                        "top_k": { "type": "integer", "default": 8, "description": "How many focus nodes to return (ranked by activation from the task)" },
                        "scope": { "type": "string", "description": "Optional scope hint to bound orientation" }
                    },
                    "required": ["agent_id", "task"]
                }
            },
            {
                "name": "am_i_stale",
                "description": "Self-awareness check a long-running agent should reach for OFTEN: which files in your working set changed on disk SINCE m1nd ingested them, so you know to re-read before acting. You can't see the filesystem change under you (the user edits, another agent edits, a build runs); this gives you that perception. Pass `files` and/or `nodes` to check specific targets, or pass NEITHER and m1nd checks every file you've touched this session. Returns stale (changed|missing), fresh, and unknown (never-ingested) paths. Read-only safe.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "files": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional explicit file paths to check. If omitted (and no `nodes`), defaults to the files you've visited this session."
                        },
                        "nodes": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional node ids to check; each is resolved to its backing file path."
                        }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "activate",
                "description": "Spreading activation query across the connectome",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query for spreading activation" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "top_k": { "type": "integer", "default": 20, "description": "Number of top results to return" },
                        "dimensions": {
                            "type": "array",
                            "items": { "type": "string", "enum": ["structural", "semantic", "temporal", "causal"] },
                            "default": ["structural", "semantic", "temporal", "causal"],
                            "description": "Activation dimensions to include"
                        },
                        "xlr": { "type": "boolean", "default": true, "description": "Enable XLR noise cancellation" },
                        "include_ghost_edges": { "type": "boolean", "default": true, "description": "Include ghost edge detection" },
                        "include_structural_holes": { "type": "boolean", "default": false, "description": "Include structural hole detection" },
                        "token_budget": { "type": "integer", "minimum": 1, "description": "Optional approx context-token budget. m1nd keeps the highest-activation nodes that fit, drops the rest, and returns a 'budget' block (estimate = chars/4, not exact tokenization)" }
                    },
                    "required": ["query", "agent_id"]
                }
            },
            {
                "name": "impact",
                "description": "Impact radius / blast analysis for a node",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "node_id": { "type": "string", "description": "Target node identifier" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "direction": {
                            "type": "string",
                            "enum": ["forward", "reverse", "both"],
                            "default": "forward",
                            "description": "Propagation direction for impact analysis"
                        },
                        "include_causal_chains": { "type": "boolean", "default": true, "description": "Include causal chain detection" }
                    },
                    "required": ["node_id", "agent_id"]
                }
            },
            {
                "name": "missing",
                "description": "Detect structural holes and missing connections",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query to find structural holes around" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "min_sibling_activation": { "type": "number", "default": 0.3, "description": "Minimum sibling activation threshold" }
                    },
                    "required": ["query", "agent_id"]
                }
            },
            {
                "name": "why",
                "description": "Path explanation between two nodes",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "source": { "type": "string", "description": "Source node identifier" },
                        "target": { "type": "string", "description": "Target node identifier" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "max_hops": { "type": "integer", "default": 6, "description": "Maximum hops in path search" }
                    },
                    "required": ["source", "target", "agent_id"]
                }
            },
            {
                "name": "warmup",
                "description": "Task-based warmup and priming",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "task_description": { "type": "string", "description": "Description of the task to warm up for" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "boost_strength": { "type": "number", "default": 0.15, "description": "Priming boost strength" }
                    },
                    "required": ["task_description", "agent_id"]
                }
            },
            {
                "name": "counterfactual",
                "description": "What-if node removal simulation",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "node_ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Node identifiers to simulate removal of"
                        },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "include_cascade": { "type": "boolean", "default": true, "description": "Include cascade analysis" }
                    },
                    "required": ["node_ids", "agent_id"]
                }
            },
            {
                "name": "predict",
                "description": "Co-change prediction for a modified node",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "changed_node": { "type": "string", "description": "Node identifier that was changed" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "top_k": { "type": "integer", "default": 10, "description": "Number of top predictions to return" },
                        "include_velocity": { "type": "boolean", "default": true, "description": "Include velocity scoring" }
                    },
                    "required": ["changed_node", "agent_id"]
                }
            },
            {
                "name": "fingerprint",
                "description": "Activation fingerprint and equivalence detection",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "target_node": { "type": "string", "description": "Optional target node to find equivalents for" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "similarity_threshold": { "type": "number", "default": 0.85, "description": "Cosine similarity threshold for equivalence" },
                        "probe_queries": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Optional probe queries for fingerprinting"
                        }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "drift",
                "description": "Weight and structural drift analysis",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "since": { "type": "string", "default": "last_session", "description": "Baseline reference point for drift comparison" },
                        "include_weight_drift": { "type": "boolean", "default": true, "description": "Include edge weight drift analysis" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "learn",
                "description": "Explicit feedback-based edge adjustment",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Original query this feedback relates to" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "feedback": {
                            "type": "string",
                            "enum": ["correct", "wrong", "partial"],
                            "description": "Feedback type: correct, wrong, or partial"
                        },
                        "node_ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Node identifiers to apply feedback to"
                        },
                        "strength": { "type": "number", "default": 0.2, "description": "Feedback strength for edge adjustment" }
                    },
                    "required": ["query", "agent_id", "feedback", "node_ids"]
                }
            },
            {
                "name": "ingest",
                "description": "Ingest or re-ingest a codebase, descriptor, or memory corpus",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Filesystem path to the source root or memory corpus" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "incremental": { "type": "boolean", "default": false, "description": "Incremental ingest (code adapter only)" },
                        "adapter": {
                            "type": "string",
                            "default": "code",
                            "enum": ["code", "json", "memory", "light", "patent", "article", "bibtex", "rfc", "crossref", "auto"],
                            "description": "Adapter to use for parsing the input corpus"
                        },
                        "mode": {
                            "type": "string",
                            "default": "replace",
                            "enum": ["replace", "merge"],
                            "description": "Replace the active graph or merge the ingest into it"
                        },
                        "namespace": {
                            "type": "string",
                            "description": "Optional namespace tag for memory/non-code nodes"
                        },
                        "include_dotfiles": {
                            "type": "boolean",
                            "default": false,
                            "description": "Include selected dotfiles and hidden config directories during ingest"
                        },
                        "dotfile_patterns": {
                            "type": "array",
                            "items": { "type": "string" },
                            "default": [],
                            "description": "Allowed dotfile patterns when include_dotfiles=true (for example '.codex/**')"
                        }
                    },
                    "required": ["path", "agent_id"]
                }
            },
            {
                "name": "document_resolve",
                "description": "Resolve a canonical universal-document artifact by source path or node id",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "path": { "type": "string", "description": "Original source path or canonical markdown path" },
                        "node_id": { "type": "string", "description": "Graph node id emitted from universal ingest" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "document_provider_health",
                "description": "Report availability and install hints for universal document providers",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "document_bindings",
                "description": "Resolve deterministic document-to-code bindings for a universal document",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "path": { "type": "string", "description": "Original source path or canonical markdown path" },
                        "node_id": { "type": "string", "description": "Graph node id emitted from universal ingest" },
                        "top_k": { "type": "integer", "default": 10, "description": "Maximum bindings to return" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "document_drift",
                "description": "Analyze stale, missing, or ambiguous document/code bindings for a universal document",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "path": { "type": "string", "description": "Original source path or canonical markdown path" },
                        "node_id": { "type": "string", "description": "Graph node id emitted from universal ingest" },
                        "scope": { "type": "string", "description": "Optional drift scope hint" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "auto_ingest_start",
                "description": "Start local-first document auto-ingest watchers for supported l1ght-family formats",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "roots": { "type": "array", "items": { "type": "string" }, "description": "Filesystem roots to watch recursively" },
                        "formats": {
                            "type": "array",
                            "items": { "type": "string", "enum": ["universal", "light", "article", "bibtex", "crossref", "rfc", "patent"] },
                            "default": ["universal", "light", "article", "bibtex", "crossref", "rfc", "patent"],
                            "description": "Supported document formats to auto-ingest"
                        },
                        "debounce_ms": { "type": "integer", "default": 200, "description": "Minimum quiet period before a change is eligible for ingestion" },
                        "namespace": { "type": "string", "description": "Optional namespace for non-code document nodes" }
                    },
                    "required": ["agent_id", "roots"]
                }
            },
            {
                "name": "auto_ingest_stop",
                "description": "Stop active document auto-ingest watchers and persist manifest state",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "auto_ingest_status",
                "description": "Report current auto-ingest runtime state, counters, manifest size, and queue depth",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "auto_ingest_tick",
                "description": "Drain queued document changes immediately and apply them to the active graph",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "health",
                "description": "Server health and statistics",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "session_handshake",
                "description": "Cheap session trust handshake before relying on m1nd retrieval",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "observed_tool_count": { "type": "integer", "description": "Optional tools/list count seen by the host client" },
                        "available_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional tool names exposed by the host client" },
                        "missing_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional required tool names missing from the host client surface" },
                        "scope": { "type": "string", "description": "Optional absolute or repo-relative scope/path to validate against the active workspace binding" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "trust_selftest",
                "description": "One-call diagnostic verdict for m1nd host binding, graph, and recovery trust",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "observed_tool_count": { "type": "integer", "description": "Optional tools/list count seen by the host client" },
                        "available_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional tool names exposed by the host client" },
                        "missing_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional required tool names missing from the host client surface" },
                        "observed_tool": { "type": "string", "description": "Optional tool that produced a suspicious result" },
                        "observed_proof_state": { "type": "string", "description": "Optional proof_state from the suspicious result" },
                        "observed_candidates": { "type": "integer", "description": "Optional candidate count from retrieval" },
                        "scope": { "type": "string", "description": "Optional repo or scope path associated with the incident" },
                        "error_text": { "type": "string", "description": "Optional error text or host message" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "recovery_playbook",
                "description": "Deterministic recovery playbook for degraded bindings, empty graphs, and stale-looking sessions",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "trust_mode": { "type": "string", "description": "Optional prior handshake trust mode to preserve in the diagnostic trail" },
                        "observed_tool": { "type": "string", "description": "Optional tool that produced a suspicious result" },
                        "observed_proof_state": { "type": "string", "description": "Optional proof_state from the suspicious result" },
                        "observed_candidates": { "type": "integer", "description": "Optional candidate count from retrieval" },
                        "observed_tool_count": { "type": "integer", "description": "Optional tools/list count seen by the host client" },
                        "available_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional tool names exposed by the host client" },
                        "missing_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional required tool names missing from the host client surface" },
                        "scope": { "type": "string", "description": "Optional repo or scope path associated with the incident" },
                        "error_text": { "type": "string", "description": "Optional error text or host message" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "doctor",
                "description": "Diagnose active graph, runtime, session, and stale binding symptoms",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "observed_tool": { "type": "string", "description": "Optional tool that produced a suspicious result" },
                        "observed_proof_state": { "type": "string", "description": "Optional proof_state from the suspicious result" },
                        "observed_candidates": { "type": "integer", "description": "Optional candidate count from retrieval" },
                        "observed_tool_count": { "type": "integer", "description": "Optional tools/list count seen by the host client" },
                        "available_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional tool names exposed by the host client" },
                        "missing_tools": { "type": "array", "items": { "type": "string" }, "description": "Optional required tool names missing from the host client surface" },
                        "scope": { "type": "string", "description": "Optional scope/path used by the suspicious call" },
                        "error_text": { "type": "string", "description": "Optional error text or host message" }
                    },
                    "required": ["agent_id"]
                }
            },
            // --- Perspective MCP tools (12-PERSPECTIVE-SYNTHESIS) ---
            {
                "name": "perspective_start",
                "description": "Enter a perspective: creates a navigable route surface from a query",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "query": { "type": "string", "description": "Seed query for route synthesis" },
                        "anchor_node": { "type": "string", "description": "Optional: anchor to a specific node (activates anchored mode)" },
                        "lens": { "type": "object", "description": "Optional: starting lens configuration" }
                    },
                    "required": ["agent_id", "query"]
                }
            },
            {
                "name": "perspective_routes",
                "description": "Browse the current route set with pagination",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" },
                        "page": { "type": "integer", "default": 1, "description": "Page number (1-based)" },
                        "page_size": { "type": "integer", "default": 6, "description": "Routes per page (clamped to 1-10)" },
                        "route_set_version": { "type": "integer", "description": "Version from previous response for staleness check" }
                    },
                    "required": ["agent_id", "perspective_id"]
                }
            },
            {
                "name": "perspective_inspect",
                "description": "Expand a route with fuller path, metrics, provenance, and affinity",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" },
                        "route_id": { "type": "string", "description": "Stable content-addressed route ID" },
                        "route_index": { "type": "integer", "description": "1-based page-local position" },
                        "route_set_version": { "type": "integer" }
                    },
                    "required": ["agent_id", "perspective_id", "route_set_version"]
                }
            },
            {
                "name": "perspective_peek",
                "description": "Extract a small relevant code/doc slice from a route target",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" },
                        "route_id": { "type": "string" },
                        "route_index": { "type": "integer" },
                        "route_set_version": { "type": "integer" }
                    },
                    "required": ["agent_id", "perspective_id", "route_set_version"]
                }
            },
            {
                "name": "perspective_follow",
                "description": "Follow a route: move focus to target, synthesize new routes",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" },
                        "route_id": { "type": "string" },
                        "route_index": { "type": "integer" },
                        "route_set_version": { "type": "integer" }
                    },
                    "required": ["agent_id", "perspective_id", "route_set_version"]
                }
            },
            {
                "name": "perspective_suggest",
                "description": "Get the next best move suggestion based on navigation history",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" },
                        "route_set_version": { "type": "integer" }
                    },
                    "required": ["agent_id", "perspective_id", "route_set_version"]
                }
            },
            {
                "name": "perspective_affinity",
                "description": "Discover probable connections a route target might have",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" },
                        "route_id": { "type": "string" },
                        "route_index": { "type": "integer" },
                        "route_set_version": { "type": "integer" }
                    },
                    "required": ["agent_id", "perspective_id", "route_set_version"]
                }
            },
            {
                "name": "perspective_branch",
                "description": "Fork the current navigation state into a new branch",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" },
                        "branch_name": { "type": "string", "description": "Optional branch name" }
                    },
                    "required": ["agent_id", "perspective_id"]
                }
            },
            {
                "name": "perspective_back",
                "description": "Navigate back to previous focus, restoring checkpoint state",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" }
                    },
                    "required": ["agent_id", "perspective_id"]
                }
            },
            {
                "name": "perspective_compare",
                "description": "Compare two perspectives on shared/unique nodes and dimension deltas",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id_a": { "type": "string" },
                        "perspective_id_b": { "type": "string" },
                        "dimensions": { "type": "array", "items": { "type": "string" }, "description": "Dimensions to compare (empty = all)" }
                    },
                    "required": ["agent_id", "perspective_id_a", "perspective_id_b"]
                }
            },
            {
                "name": "perspective_list",
                "description": "List all perspectives for an agent",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "perspective_close",
                "description": "Close a perspective and release associated locks",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string" },
                        "perspective_id": { "type": "string" }
                    },
                    "required": ["agent_id", "perspective_id"]
                }
            },
            // =================================================================
            // L2: Semantic Search
            // =================================================================
            {
                "name": "seek",
                "description": "Intent-aware semantic code search — find code by PURPOSE, not text pattern",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Natural language description of what the agent is looking for" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "top_k": { "type": "integer", "default": 20, "description": "Maximum results to return" },
                        "scope": { "type": "string", "description": "File path prefix to limit search scope" },
                        "node_types": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Filter by node type: function, class, struct, module, file" },
                        "min_score": { "type": "number", "default": 0.1, "description": "Minimum combined score threshold" },
                        "graph_rerank": { "type": "boolean", "default": true, "description": "Whether to run graph re-ranking on embedding candidates" },
                        "token_budget": { "type": "integer", "minimum": 1, "description": "Optional approx context-token budget. m1nd keeps the highest graph-importance hits that fit, drops the rest, and returns a 'budget' block (estimate = chars/4, not exact tokenization)" }
                    },
                    "required": ["query", "agent_id"]
                }
            },
            {
                "name": "scan",
                "description": "Keyword/label pattern scan over graph nodes with curated anti-pattern sets, severity, and optional graph-edge context (graph_validate populates graph_context when edges are present; commonly empty for sparse graphs)",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Pattern ID (error_handling, resource_cleanup, api_surface, state_mutation, concurrency, auth_boundary, test_coverage, dependency_injection) or custom pattern" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix to limit scan scope" },
                        "severity_min": { "type": "number", "default": 0.3, "description": "Minimum severity threshold [0.0, 1.0]" },
                        "graph_validate": { "type": "boolean", "default": true, "description": "Whether to validate findings against graph edges" },
                        "limit": { "type": "integer", "default": 50, "description": "Maximum findings to return" }
                    },
                    "required": ["pattern", "agent_id"]
                }
            },
            // =================================================================
            // L3: Temporal Intelligence
            // =================================================================
            {
                "name": "timeline",
                "description": "Git-based temporal history for a node — changes, co-changes, velocity, stability",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "node": { "type": "string", "description": "Node external_id (e.g. file::backend/chat_handler.py)" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "depth": { "type": "string", "default": "30d", "description": "Time depth: 7d, 30d, 90d, all" },
                        "include_co_changes": { "type": "boolean", "default": true, "description": "Include co-changed files with coupling scores" },
                        "include_churn": { "type": "boolean", "default": true, "description": "Include lines added/deleted churn data" },
                        "top_k": { "type": "integer", "default": 10, "description": "Max co-change partners to return" }
                    },
                    "required": ["node", "agent_id"]
                }
            },
            {
                "name": "diverge",
                "description": "Detect structural drift between a baseline and current graph state",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "baseline": { "type": "string", "description": "Baseline reference: ISO date, git ref, or last_session" },
                        "scope": { "type": "string", "description": "File path glob to limit scope" },
                        "include_coupling_changes": { "type": "boolean", "default": true, "description": "Include coupling matrix delta" },
                        "include_anomalies": { "type": "boolean", "default": true, "description": "Detect anomalies (test deficits, velocity spikes)" }
                    },
                    "required": ["agent_id", "baseline"]
                }
            },
            // =================================================================
            // L4: Investigation Memory
            // =================================================================
            {
                "name": "trail_save",
                "description": "Persist current investigation state — nodes visited, hypotheses, conclusions",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "label": { "type": "string", "description": "Human-readable label for this investigation" },
                        "hypotheses": { "type": "array", "items": { "type": "object" }, "default": [], "description": "Hypotheses formed during investigation" },
                        "conclusions": { "type": "array", "items": { "type": "object" }, "default": [], "description": "Conclusions reached" },
                        "open_questions": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Open questions remaining" },
                        "tags": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Tags for organization and search" },
                        "summary": { "type": "string", "description": "Optional summary (auto-generated if omitted)" },
                        "visited_nodes": { "type": "array", "items": { "type": "object" }, "default": [], "description": "Explicitly list visited nodes with annotations" },
                        "activation_boosts": { "type": "object", "default": {}, "description": "Map of node_external_id -> boost weight [0.0, 1.0]" }
                    },
                    "required": ["agent_id", "label"]
                }
            },
            {
                "name": "trail_resume",
                "description": "Restore a saved investigation — re-inject activation boosts, detect staleness",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "trail_id": { "type": "string", "description": "Trail ID to resume" },
                        "force": { "type": "boolean", "default": false, "description": "Resume even if trail is stale (>50% missing nodes)" }
                    },
                    "required": ["agent_id", "trail_id"]
                }
            },
            {
                "name": "trail_merge",
                "description": "Combine two or more investigation trails — discover cross-connections",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "trail_ids": { "type": "array", "items": { "type": "string" }, "description": "Two or more trail IDs to merge" },
                        "label": { "type": "string", "description": "Label for the merged trail (auto-generated if omitted)" }
                    },
                    "required": ["agent_id", "trail_ids"]
                }
            },
            {
                "name": "trail_list",
                "description": "List saved investigation trails with optional filters",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "filter_agent_id": { "type": "string", "description": "Filter to a specific agent's trails" },
                        "filter_status": { "type": "string", "description": "Filter by status: active, saved, archived, stale, merged" },
                        "filter_tags": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Filter by tags (any match)" }
                    },
                    "required": ["agent_id"]
                }
            },
            // =================================================================
            // L5: Hypothesis Engine
            // =================================================================
            {
                "name": "hypothesize",
                "description": "Test a structural claim about the codebase — graph-based hypothesis testing",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "claim": { "type": "string", "description": "Natural language claim (e.g. 'chat_handler never validates session tokens')" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "max_hops": { "type": "integer", "default": 5, "description": "Max BFS hops for evidence search" },
                        "include_ghost_edges": { "type": "boolean", "default": true, "description": "Include ghost edges as weak evidence" },
                        "include_partial_flow": { "type": "boolean", "default": true, "description": "Include partial flow when full path not found" },
                        "path_budget": { "type": "integer", "default": 1000, "description": "Budget cap for all-paths enumeration" }
                    },
                    "required": ["claim", "agent_id"]
                }
            },
            {
                "name": "differential",
                "description": "Focused structural diff between two graph snapshots",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "snapshot_a": { "type": "string", "description": "Path to snapshot A, or 'current'" },
                        "snapshot_b": { "type": "string", "description": "Path to snapshot B, or 'current'" },
                        "question": { "type": "string", "description": "Focus question to narrow the diff output" },
                        "focus_nodes": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Limit diff to neighborhood of specific nodes" }
                    },
                    "required": ["agent_id", "snapshot_a", "snapshot_b"]
                }
            },
            // =================================================================
            // L6: Execution Feedback
            // =================================================================
            {
                "name": "trace",
                "description": "Map runtime errors to structural root causes via stacktrace analysis",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "error_text": { "type": "string", "description": "Full error output (stacktrace + error message)" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "language": { "type": "string", "description": "Language hint: python, rust, typescript, javascript, go (auto-detected if omitted)" },
                        "window_hours": { "type": "number", "default": 24.0, "description": "Temporal window (hours) for co-change suspect scan" },
                        "top_k": { "type": "integer", "default": 10, "description": "Max suspects to return" }
                    },
                    "required": ["error_text", "agent_id"]
                }
            },
            {
                "name": "validate_plan",
                "description": "Validate a modification plan against the code graph — detect gaps and risk",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "actions": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "action_type": { "type": "string", "description": "modify, create, delete, rename, or test" },
                                    "file_path": { "type": "string", "description": "Relative file path" },
                                    "description": { "type": "string" },
                                    "depends_on": { "type": "array", "items": { "type": "string" }, "default": [] }
                                },
                                "required": ["action_type", "file_path"]
                            },
                            "description": "Ordered list of planned actions"
                        },
                        "include_test_impact": { "type": "boolean", "default": true, "description": "Analyze test coverage for modified files" },
                        "include_risk_score": { "type": "boolean", "default": true, "description": "Compute composite risk score" },
                        "scope": { "type": "string", "description": "Optional repo or scope path for multi-repo binding diagnostics" }
                    },
                    "required": ["agent_id", "actions"]
                }
            },
            // =================================================================
            // L7: Multi-Repository Federation
            // =================================================================
            {
                "name": "federate",
                "description": "Ingest multiple repos into a unified federated graph with cross-repo edge detection",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "repos": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "name": { "type": "string", "description": "Repository name (namespace prefix)" },
                                    "path": { "type": "string", "description": "Absolute path to repository root" },
                                    "adapter": { "type": "string", "default": "code", "description": "Ingest adapter override" }
                                },
                                "required": ["name", "path"]
                            },
                            "description": "List of repositories to federate"
                        },
                        "detect_cross_repo_edges": { "type": "boolean", "default": true, "description": "Auto-detect cross-repo edges" },
                        "incremental": { "type": "boolean", "default": false, "description": "Only re-ingest repos that changed" }
                    },
                    "required": ["agent_id", "repos"]
                }
            },
            // =================================================================
            // Superpowers: Antibody / Flow / Epidemic / Tremor / Trust / Layers
            // =================================================================
            {
                "name": "antibody_scan",
                "description": "Scan code graph against stored bug antibodies (immune memory patterns). Returns matches where known bug patterns recur.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "default": "all", "description": "\"all\" = entire graph, \"changed\" = nodes since last scan" },
                        "antibody_ids": { "type": "array", "items": { "type": "string" }, "description": "Optional: only scan specific antibodies" },
                        "max_matches": { "type": "integer", "default": 50, "description": "Maximum matches to return" },
                        "min_severity": { "type": "string", "default": "info", "description": "Minimum severity: info, warning, critical" },
                        "similarity_threshold": { "type": "number", "default": 0.7, "description": "Fuzzy match threshold for label matching (0.0-1.0)" },
                        "match_mode": { "type": "string", "default": "substring", "description": "Label match mode: exact, substring, regex" },
                        "max_matches_per_antibody": { "type": "integer", "default": 50, "description": "Maximum matches per individual antibody" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "antibody_list",
                "description": "List all stored bug antibodies with metadata, match history, and specificity scores.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "include_disabled": { "type": "boolean", "default": false, "description": "Include disabled antibodies" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "antibody_create",
                "description": "Create, disable, enable, or delete a bug antibody pattern.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "action": { "type": "string", "default": "create", "description": "Action: create, disable, enable, delete" },
                        "antibody_id": { "type": "string", "description": "Required for disable/enable/delete" },
                        "name": { "type": "string", "description": "Antibody name (for create)" },
                        "description": { "type": "string", "description": "What this pattern detects" },
                        "severity": { "type": "string", "default": "warning", "description": "info, warning, critical" },
                        "pattern": { "type": "object", "description": "Pattern definition with nodes/edges/negative_edges" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "flow_simulate",
                "description": "Simulate concurrent execution flow. Detects race conditions via particle collision on shared mutable state without synchronization.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "entry_nodes": { "type": "array", "items": { "type": "string" }, "description": "Starting nodes. Auto-discovered if empty." },
                        "num_particles": { "type": "integer", "default": 2, "description": "Particles per entry point" },
                        "lock_patterns": { "type": "array", "items": { "type": "string" }, "description": "Regex patterns for lock/mutex detection" },
                        "read_only_patterns": { "type": "array", "items": { "type": "string" }, "description": "Regex patterns for read-only operations" },
                        "max_depth": { "type": "integer", "default": 15, "description": "Maximum BFS depth" },
                        "turbulence_threshold": { "type": "number", "default": 0.5, "description": "Minimum score to report" },
                        "include_paths": { "type": "boolean", "default": true, "description": "Include particle paths in output" },
                        "max_total_steps": { "type": "integer", "default": 50000, "description": "Global step budget across all particles" },
                        "scope_filter": { "type": "string", "description": "Substring filter to limit which nodes particles can enter" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "epidemic",
                "description": "Predict bug propagation via SIR epidemiological model. Given known buggy modules, predicts which neighbors are most likely to harbor undiscovered bugs.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "infected_nodes": { "type": "array", "items": { "type": "string" }, "description": "Known buggy node IDs" },
                        "recovered_nodes": { "type": "array", "items": { "type": "string" }, "description": "Already-fixed node IDs" },
                        "infection_rate": { "type": "number", "description": "Uniform infection rate. If omitted, derived from edge weights." },
                        "recovery_rate": { "type": "number", "default": 0, "description": "SIR recovery rate" },
                        "iterations": { "type": "integer", "default": 50, "description": "Simulation iterations" },
                        "direction": { "type": "string", "default": "both", "description": "Propagation direction: forward, backward, both" },
                        "top_k": { "type": "integer", "default": 20, "description": "Max predictions to return" },
                        "auto_calibrate": { "type": "boolean", "default": true, "description": "Auto-adjust infection_rate based on graph density" },
                        "scope": { "type": "string", "default": "all", "description": "Filter predictions: files, functions, all" },
                        "min_probability": { "type": "number", "default": 0.001, "description": "Filter out predictions below this probability" }
                    },
                    "required": ["agent_id", "infected_nodes"]
                }
            },
            {
                "name": "tremor",
                "description": "Detect code tremors: modules with accelerating change frequency (second derivative). Earthquake precursor analogy for imminent bugs.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "window": { "type": "string", "default": "30d", "description": "Time window: 7d, 30d, 90d, all" },
                        "threshold": { "type": "number", "default": 0.1, "description": "Minimum magnitude to report" },
                        "top_k": { "type": "integer", "default": 20, "description": "Max results" },
                        "node_filter": { "type": "string", "description": "Filter to nodes matching this prefix" },
                        "include_history": { "type": "boolean", "default": false, "description": "Include observation history" },
                        "min_observations": { "type": "integer", "default": 3, "description": "Minimum data points to compute tremor" },
                        "sensitivity": { "type": "number", "default": 1.0, "description": "Multiplier on acceleration threshold (higher = more sensitive)" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "trust",
                "description": "Per-module trust scores from defect history. Actuarial risk assessment: more confirmed bugs = lower trust = higher risk.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "default": "file", "description": "Node type scope: file, function, class, all" },
                        "min_history": { "type": "integer", "default": 1, "description": "Minimum learn events for inclusion" },
                        "top_k": { "type": "integer", "default": 20, "description": "Max results" },
                        "node_filter": { "type": "string", "description": "Filter to nodes matching this prefix" },
                        "sort_by": { "type": "string", "default": "trust_asc", "description": "Sort: trust_asc, trust_desc, defects_desc, recency" },
                        "decay_half_life_days": { "type": "number", "default": 30.0, "description": "How fast old defects lose weight (days)" },
                        "risk_cap": { "type": "number", "default": 3.0, "description": "Maximum risk multiplier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "layers",
                "description": "Auto-detect architectural layers from graph topology. Returns layer assignments plus dependency violations (edges going against expected flow).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "max_layers": { "type": "integer", "default": 8, "description": "Maximum layers to detect" },
                        "include_violations": { "type": "boolean", "default": true, "description": "Include violation analysis" },
                        "min_nodes_per_layer": { "type": "integer", "default": 2, "description": "Minimum nodes for a layer to be reported" },
                        "node_types": { "type": "array", "items": { "type": "string" }, "description": "Filter by node types" },
                        "naming_strategy": { "type": "string", "default": "auto", "description": "Layer naming: auto, path_prefix, pagerank" },
                        "exclude_tests": { "type": "boolean", "default": false, "description": "Exclude test files from layer detection" },
                        "violation_limit": { "type": "integer", "default": 100, "description": "Maximum violations to return" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "layer_inspect",
                "description": "Inspect a specific architectural layer: nodes, connections, violations, and health metrics.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "level": { "type": "integer", "description": "Layer level to inspect" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "include_edges": { "type": "boolean", "default": true, "description": "Include inter-layer edges" },
                        "top_k": { "type": "integer", "default": 50, "description": "Max nodes to return per layer" }
                    },
                    "required": ["agent_id", "level"]
                }
            },
            // =================================================================
            // RETROBUILDER modules — temporal edges, taint, twins, refactors,
            // and runtime overlays
            // =================================================================
            {
                "name": "ghost_edges",
                "description": "Parse git history and surface temporal co-change ghost edges between files that move together without explicit static dependencies.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "depth": { "type": "string", "default": "30d", "description": "Git history window: 7d, 30d, 90d, all" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "top_k": { "type": "integer", "default": 50, "description": "Maximum ghost edges to return" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "taint_trace",
                "description": "Inject taint at entry points and trace propagation through the graph to detect missed validation, auth, or sanitization boundaries.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "entry_nodes": { "type": "array", "items": { "type": "string" }, "description": "Entry point node IDs to inject taint" },
                        "taint_type": { "type": "string", "default": "user_input", "description": "Taint type: user_input, sensitive_data, or custom" },
                        "boundary_patterns": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Custom boundary patterns when taint_type=custom" },
                        "max_depth": { "type": "integer", "default": 15, "description": "Maximum propagation depth" },
                        "min_probability": { "type": "number", "default": 0.01, "description": "Minimum propagation probability to report" }
                    },
                    "required": ["agent_id", "entry_nodes"]
                }
            },
            {
                "name": "twins",
                "description": "Find structurally similar or identical nodes via topological signature similarity.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "similarity_threshold": { "type": "number", "default": 0.80, "description": "Minimum cosine similarity threshold" },
                        "top_k": { "type": "integer", "default": 50, "description": "Maximum twin pairs to return" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "node_types": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Optional node type filter" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "refactor_plan",
                "description": "Propose graph-native refactoring communities and extraction candidates for a scoped region of the codebase.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "max_communities": { "type": "integer", "default": 10, "description": "Maximum communities to consider" },
                        "min_community_size": { "type": "integer", "default": 3, "description": "Minimum nodes for an extractable community" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "runtime_overlay",
                "description": "Overlay OpenTelemetry span activity onto the graph to paint runtime heat, latency, and error signals onto nodes.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "spans": { "type": "array", "items": { "type": "object" }, "description": "OTel spans to ingest" },
                        "service_name": { "type": "string", "default": "", "description": "Optional service name for scoping" },
                        "mapping_strategy": { "type": "string", "default": "label_match", "description": "Mapping strategy: label_match, code_attribute, exact_id" },
                        "boost_strength": { "type": "number", "default": 0.15, "description": "Activation boost strength" }
                    },
                    "required": ["agent_id", "spans"]
                }
            },
            // =================================================================
            // Surgical: context + apply
            // =================================================================
            {
                "name": "heuristics_surface",
                "description": "Return an explicit explainability surface for a code target, showing why heuristics ranked it as risky or important.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "node_id": { "type": "string", "description": "Graph node ID to inspect" },
                        "file_path": { "type": "string", "description": "Absolute or workspace-relative path to inspect" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "surgical_context",
                "description": "Return full context for surgical LLM editing: file contents, symbols, and graph neighbourhood (callers, callees, tests). Use before apply.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string", "description": "Absolute or workspace-relative path to the file being edited" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "symbol": { "type": "string", "description": "Optional: narrow context to a specific symbol (function/struct/class name)" },
                        "radius": { "type": "integer", "default": 1, "description": "BFS radius for graph neighbourhood (1 or 2)" },
                        "include_tests": { "type": "boolean", "default": true, "description": "Include test files in the neighbourhood" }
                    },
                    "required": ["file_path", "agent_id"]
                }
            },
            {
                "name": "apply",
                "description": "Write LLM-edited code back to a file and trigger incremental re-ingest so the graph stays coherent. Always call surgical_context first.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string", "description": "Absolute or workspace-relative path of the file to overwrite" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "new_content": { "type": "string", "description": "New file contents (full replacement, UTF-8)" },
                        "description": { "type": "string", "description": "Human-readable description of the edit" },
                        "reingest": { "type": "boolean", "default": true, "description": "Re-ingest the file after writing (recommended)" }
                    },
                    "required": ["file_path", "agent_id", "new_content"]
                }
            },
            // =================================================================
            // View: lightweight file reader
            // =================================================================
            {
                "name": "view",
                "description": "Fast file reader with line numbers. Replaces View/cat/head/tail. No graph traversal — just reads the file. Auto-ingests if not in graph. Use for quick file inspection before surgical_context.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string", "description": "Absolute or workspace-relative path to the file" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "offset": { "type": "integer", "default": 0, "description": "Start line (0-based)" },
                        "limit": { "type": "integer", "description": "Max lines to return (default: all)" },
                        "auto_ingest": { "type": "boolean", "default": true, "description": "Auto-ingest file into graph if not present" },
                        "max_output_chars": { "type": "integer", "description": "Optional cap for returned characters after line-number formatting" }
                    },
                    "required": ["file_path", "agent_id"]
                }
            },
            {
                "name": "batch_view",
                "description": "Read multiple files or glob patterns in one call with stable delimiters, optional summaries, and auto-ingest.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "files": { "type": "array", "items": { "type": "string" }, "description": "File paths and/or glob-like patterns to expand" },
                        "max_lines_per_file": { "type": "integer", "default": 100, "description": "Maximum lines to return per file" },
                        "summary_mode": { "type": "boolean", "default": true, "description": "Add an inline summary for each returned file" },
                        "auto_ingest": { "type": "boolean", "default": true, "description": "Auto-ingest discovered files before reading" },
                        "max_output_chars": { "type": "integer", "description": "Optional cap for the concatenated response body" }
                    },
                    "required": ["agent_id", "files"]
                }
            },
            // =================================================================
            // Surgical V2: context_v2 + apply_batch
            // =================================================================
            {
                "name": "surgical_context_v2",
                "description": "Get full surgical context for a file PLUS source code of connected files (callers, callees, tests). Returns a complete workspace snapshot in one call. Superset of surgical_context.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "file_path": { "type": "string", "description": "Absolute or workspace-relative path to the primary file" },
                        "symbol": { "type": "string", "description": "Optional: narrow context to a specific symbol (function/struct/class name)" },
                        "include_tests": { "type": "boolean", "default": true, "description": "Include test files in the neighbourhood" },
                        "radius": { "type": "integer", "default": 1, "description": "BFS radius for graph neighbourhood (1 or 2)" },
                        "max_connected_files": { "type": "integer", "default": 5, "description": "Maximum number of connected files to include source for" },
                        "max_lines_per_file": { "type": "integer", "default": 60, "description": "Maximum lines per connected file (primary file is unbounded)" }
                    },
                    "required": ["agent_id", "file_path"]
                }
            },
            {
                "name": "apply_batch",
                "description": "Atomically write multiple files and trigger a single bulk re-ingest. Use after surgical_context_v2 when editing a file and its callers/tests together. All-or-nothing by default.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "edits": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "file_path": { "type": "string", "description": "Absolute or workspace-relative path of the file to write" },
                                    "new_content": { "type": "string", "description": "New file contents (full replacement, UTF-8)" },
                                    "description": { "type": "string", "description": "Optional human-readable label for this edit" }
                                },
                                "required": ["file_path", "new_content"]
                            },
                            "description": "List of file edits to apply"
                        },
                        "atomic": { "type": "boolean", "default": true, "description": "All-or-nothing: if any file fails, none are written" },
                        "reingest": { "type": "boolean", "default": true, "description": "Re-ingest all modified files after writing" },
                        "verify": { "type": "boolean", "default": false, "description": "Run post-write verification after the batch finishes" }
                    },
                    "required": ["agent_id", "edits"]
                }
            },
            {
                "name": "edit_preview",
                "description": "Build an in-memory preview of a single-file full-replacement edit. Returns a preview handle, source snapshot, diff, and validation report. Does not touch disk.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "file_path": { "type": "string", "description": "Absolute or workspace-relative path of the file to preview" },
                        "new_content": { "type": "string", "description": "Candidate file contents (full replacement, UTF-8)" },
                        "description": { "type": "string", "description": "Optional human-readable description of the preview" }
                    },
                    "required": ["agent_id", "file_path", "new_content"]
                }
            },
            {
                "name": "edit_commit",
                "description": "Commit a previously created edit_preview handle after re-checking source freshness. Persists atomically through the existing apply path.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "preview_id": { "type": "string", "description": "Preview handle returned by edit_preview" },
                        "confirm": { "type": "boolean", "default": false, "description": "Must be true to confirm the commit. Safety guard against accidental writes." },
                        "reingest": { "type": "boolean", "default": true, "description": "Re-ingest the modified file after commit" }
                    },
                    "required": ["agent_id", "preview_id", "confirm"]
                }
            },
            // =================================================================
            // v0.4.0: search, help, report, panoramic, savings
            // =================================================================
            {
                "name": "search",
                "description": "Unified code search: literal, regex (with multiline), or semantic. Searches graph node labels AND file contents on disk. Supports invert (grep -v), count-only (grep -c), multiline regex (rg -U), and filename pattern filtering (grep --include). v0.5.0: regex mode now searches file contents (not just node IDs).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "query": { "type": "string", "description": "Search query string" },
                        "mode": {
                            "type": "string",
                            "enum": ["literal", "regex", "semantic"],
                            "default": "literal",
                            "description": "Search mode: literal (substring), regex (pattern), semantic (graph-aware)"
                        },
                        "scope": { "type": "string", "description": "File path prefix filter" },
                        "top_k": { "type": "integer", "default": 50, "description": "Max results (1-500)" },
                        "context_lines": { "type": "integer", "default": 2, "description": "Lines of context before/after match (0-10)" },
                        "case_sensitive": { "type": "boolean", "default": false, "description": "Case-sensitive matching" },
                        "invert": { "type": "boolean", "default": false, "description": "Return lines that DON'T match (grep -v)" },
                        "count_only": { "type": "boolean", "default": false, "description": "Return just the count, no results (grep -c)" },
                        "multiline": { "type": "boolean", "default": false, "description": "Enable multiline regex: dot matches newline (rg -U). Only for regex mode." },
                        "auto_ingest": { "type": "boolean", "default": false, "description": "Auto-ingest exactly one resolved scope path outside current ingest roots before searching; ambiguous scopes return an error that lists candidate paths in detail" },
                        "filename_pattern": { "type": "string", "description": "Glob pattern to filter filenames (e.g. '*.rs', 'test_*.py')" },
                        "max_output_chars": { "type": "integer", "description": "Optional cap for total returned characters across serialized matches" },
                        "token_budget": { "type": "integer", "minimum": 1, "description": "Optional approx context-token budget. m1nd keeps the highest-ranked rows that fit, drops the rest, and returns a 'budget' block (estimate = chars/4, not exact tokenization; ignored when count_only)" }
                    },
                    "required": ["agent_id", "query"]
                }
            },
            {
                "name": "glob",
                "description": "Graph-aware file glob: find files in the ingested graph by glob pattern. Zero I/O — pure graph query. Replaces find/glob for indexed codebases.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "pattern": { "type": "string", "description": "Glob pattern (e.g. '**/*.rs', 'src/**/mod.rs', '*.toml')" },
                        "scope": { "type": "string", "description": "Root directory prefix to narrow scope" },
                        "top_k": { "type": "integer", "default": 200, "description": "Max results (1-10000)" },
                        "sort": {
                            "type": "string",
                            "enum": ["path", "activation"],
                            "default": "path",
                            "description": "Sort order: path (alphabetical) or activation (most connected first)"
                        }
                    },
                    "required": ["agent_id", "pattern"]
                }
            },
            {
                "name": "scan_all",
                "description": "Run all structural scan patterns in one call and return grouped findings by pattern.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "severity_min": { "type": "number", "default": 0.3, "description": "Minimum severity threshold across all patterns" },
                        "graph_validate": { "type": "boolean", "default": true, "description": "Whether to validate findings against graph edges" },
                        "limit_per_pattern": { "type": "integer", "default": 50, "description": "Maximum findings per pattern" },
                        "patterns": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Optional subset of patterns to run; empty means all built-ins" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "cross_verify",
                "description": "Compare graph state against disk truth: missing files, LOC drift, hash mismatches, and evidence_freshness (flags memorized L1GHT claims whose cited code changed). Empty check = run all.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "check": { "type": "array", "items": { "type": "string", "enum": ["existence", "loc", "hash", "evidence_freshness"] }, "default": [], "description": "Checks to run (empty = all): existence, loc, hash, evidence_freshness. evidence_freshness reports stale_evidence — memorize claims whose grounded_in code changed since ingest." },
                        "include_dotfiles": { "type": "boolean", "default": false, "description": "Include selected dotfiles while verifying disk state" },
                        "dotfile_patterns": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Allowed dotfile patterns when include_dotfiles=true" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "coverage_session",
                "description": "Report what the current agent session has and has not visited across files, nodes, and tool usage.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "external_references",
                "description": "Scan graph-tracked files for explicit references to paths outside the current ingest roots.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "federate_auto",
                "description": "Discover candidate external repositories from the current workspace and optionally federate them in one step.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix to limit discovery sources" },
                        "current_repo_name": { "type": "string", "description": "Optional namespace override for the current workspace inside the federated graph" },
                        "max_repos": { "type": "integer", "default": 8, "description": "Maximum discovered external repos to include" },
                        "detect_cross_repo_edges": { "type": "boolean", "default": true, "description": "Whether a follow-up federate execution should auto-detect cross-repo edges" },
                        "execute": { "type": "boolean", "default": false, "description": "When true, immediately run federate with the current repo plus discovered candidates" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "help",
                "description": "Context-aware help for m1nd tools. Returns tool doctrine, route guidance, workflow sequences, or recovery guidance for agents.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "tool_name": { "type": "string", "description": "Specific tool name for detailed help (omit for overview routing)" },
                        "mode": {
                            "type": "string",
                            "enum": ["overview", "tool", "route", "recovery", "workflow"],
                            "description": "Help mode: overview, tool, route, recovery, or workflow"
                        },
                        "intent": { "type": "string", "description": "Short statement of what the agent is trying to do" },
                        "stage": {
                            "type": "string",
                            "enum": ["orient", "find", "ground", "diagnose", "plan", "edit", "review", "operate", "handoff"],
                            "description": "Current working stage for the agent"
                        },
                        "path": { "type": "string", "description": "Current path or target in focus when known" },
                        "error_text": { "type": "string", "description": "Observed error text, stacktrace, or failure summary" },
                        "recent_tools": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Tools already used in the current flow"
                        },
                        "max_suggestions": { "type": "integer", "default": 3, "description": "Maximum ranked suggestions to return in route or recovery mode" },
                        "render": {
                            "type": "string",
                            "enum": ["full", "compact", "none"],
                            "default": "full",
                            "description": "Render mode for formatted help text"
                        }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "mission_start",
                "description": "Start a bounded agent mission with route, budget envelope, starter moves, and non-claims.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "repo": { "type": "string", "description": "Absolute or host-resolved repository path this mission is scoped to" },
                        "task": { "type": "string", "description": "Mission task in plain language" },
                        "mode": {
                            "type": "string",
                            "enum": ["bug_hunt", "review", "refactor", "docs_drift", "architecture", "release"],
                            "default": "review",
                            "description": "Mission mode"
                        },
                        "budget": {
                            "type": "string",
                            "enum": ["short", "normal", "deep"],
                            "default": "normal",
                            "description": "Mission budget envelope"
                        },
                        "risk": {
                            "type": "string",
                            "enum": ["low", "medium", "high"],
                            "default": "medium",
                            "description": "Risk level for routing"
                        },
                        "parent_mission_id": { "type": "string", "description": "Optional parent mission id for handoff or sub-mission tracking" }
                    },
                    "required": ["agent_id", "repo", "task"]
                }
            },
            {
                "name": "mission_next",
                "description": "Append the latest mission event and return exactly one recommended next move plus do-not guardrails.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "mission_id": { "type": "string", "description": "Mission id returned by mission_start" },
                        "last_event": {
                            "type": "object",
                            "description": "Optional event from the action just taken, such as graph_query, file_read, test_run, or dissent"
                        }
                    },
                    "required": ["agent_id", "mission_id"]
                }
            },
            {
                "name": "mission_event",
                "description": "Record one observed mission action with evidence class, event id, and local digest.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "mission_id": { "type": "string", "description": "Mission id returned by mission_start" },
                        "event": {
                            "type": ["object", "string"],
                            "description": "Observed action, such as file_read, test_run, graph_query, dissent, or coverage_sweep"
                        },
                        "payload": {
                            "description": "Optional structured evidence payload for string-style events"
                        },
                        "outcome": {
                            "type": "string",
                            "description": "Optional observed outcome, such as hypothesis_supported or inconclusive"
                        },
                        "agent_confidence": {
                            "type": "number",
                            "description": "Optional caller confidence captured as telemetry, not proof"
                        }
                    },
                    "required": ["agent_id", "mission_id", "event"]
                }
            },
            {
                "name": "mission_verify",
                "description": "Verify whether a mission claim has enough direct evidence; graph-only evidence is rejected.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "mission_id": { "type": "string", "description": "Mission id returned by mission_start" },
                        "claim": { "type": "string", "description": "Candidate conclusion to validate" },
                        "evidence_refs": {
                            "type": "array",
                            "items": { "type": "string" },
                            "default": [],
                            "description": "Evidence references such as file_read:path:line, test_run:name, compiler:error, or runtime_probe:id"
                        },
                        "confidence": { "type": "number", "description": "Optional agent confidence before verification" }
                    },
                    "required": ["agent_id", "mission_id", "claim"]
                }
            },
            {
                "name": "mission_handoff",
                "description": "Serialize a resumable mission handoff with verified claims, open hypotheses, dead paths, graph anchors, and next move.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "mission_id": { "type": "string", "description": "Mission id returned by mission_start" },
                        "summary": { "type": "string", "description": "Optional handoff summary" },
                        "recipient_agent_id": { "type": "string", "description": "Optional recipient agent id" },
                        "include_events": { "type": "boolean", "default": false, "description": "Include full event stream in the handoff packet" }
                    },
                    "required": ["agent_id", "mission_id"]
                }
            },
            {
                "name": "mission_close",
                "description": "Close a mission with a proof packet containing verified claims, rejected claims, events, gaps, and non-claims.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "mission_id": { "type": "string", "description": "Mission id returned by mission_start" },
                        "summary": { "type": "string", "description": "Optional concise mission summary" },
                        "non_claims": {
                            "type": "array",
                            "items": { "type": "string" },
                            "default": [],
                            "description": "Extra non-claims to preserve in the proof packet"
                        },
                        "gaps": {
                            "type": "array",
                            "items": { "type": "string" },
                            "default": [],
                            "description": "Known remaining gaps"
                        },
                        "write_light_memory": {
                            "type": "boolean",
                            "default": false,
                            "description": "If true, persist the mission's verified claims as L1GHT memory (.light.md, anchored to code, auto-loads next session). Path returned under light_memory."
                        }
                    },
                    "required": ["agent_id", "mission_id"]
                }
            },
            {
                "name": "report",
                "description": "Session intelligence report: queries, bugs, graph evolution, and estimated savings.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "max_output_chars": { "type": "integer", "description": "Optional cap for markdown summary size" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "audit",
                "description": "Profile-aware one-call audit for topology, scans, verification, git state, and recommendations.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "path": { "type": "string", "description": "Root path to audit" },
                        "profile": { "type": "string", "default": "auto", "description": "Audit profile: auto, quick, coordination, production, security, migration" },
                        "depth": { "type": "string", "default": "full", "description": "Audit depth: quick, surface, full" },
                        "cross_verify": { "type": "boolean", "default": true, "description": "Compare graph vs filesystem state" },
                        "include_git": { "type": "boolean", "default": true, "description": "Include git state and recent history" },
                        "include_config": { "type": "boolean", "default": false, "description": "Include selected dotfiles/config directories" },
                        "scan_patterns": { "type": "string", "default": "all", "description": "Scan selection: all, default, or a comma-separated list" },
                        "external_refs": { "type": "boolean", "default": true, "description": "Discover explicit external path references" },
                        "report_format": { "type": "string", "default": "markdown", "description": "Output format: markdown or json" },
                        "max_output_chars": { "type": "integer", "description": "Optional cap for returned narrative/report size" }
                    },
                    "required": ["agent_id", "path"]
                }
            },
            {
                "name": "daemon_start",
                "description": "Start persisted daemon state and store watched paths for continuous structural monitoring.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "watch_paths": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Paths the daemon should treat as watched roots" },
                        "poll_interval_ms": { "type": "integer", "default": 500, "description": "Fallback polling interval in milliseconds" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "daemon_stop",
                "description": "Stop persisted daemon state without deleting alerts or runtime state.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "daemon_status",
                "description": "Report daemon state, watched paths, alert counts, and generation counters.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "daemon_tick",
                "description": "Poll watched roots once, incrementally re-ingest changed files, and surface drift alerts for deleted files.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "max_files": { "type": "integer", "default": 32, "description": "Maximum changed files to process in one tick" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "alerts_list",
                "description": "List persisted daemon/proactive alerts.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "include_acked": { "type": "boolean", "default": false, "description": "Include acknowledged alerts" },
                        "limit": { "type": "integer", "default": 50, "description": "Maximum number of alerts to return" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "alerts_ack",
                "description": "Acknowledge one or more daemon/proactive alerts.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "alert_ids": { "type": "array", "items": { "type": "string" }, "description": "Alert IDs to acknowledge" }
                    },
                    "required": ["agent_id", "alert_ids"]
                }
            },
            {
                "name": "panoramic",
                "description": "Panoramic graph health overview: per-module risk scores combining blast radius, centrality, and churn signals.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix filter" },
                        "top_n": { "type": "integer", "default": 50, "description": "Max modules to return (1-1000)" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "persist",
                "description": "Persist/load graph and plasticity state; supports binary snapshots",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "action": { "type": "string", "enum": ["save", "load", "checkpoint", "status"], "description": "Action to perform" },
                        "format": { "type": "string", "enum": ["json", "bin"], "default": "json", "description": "Snapshot format" },
                        "path": { "type": "string", "description": "Override snapshot path (optional)" }
                    },
                    "required": ["agent_id", "action"]
                }
            },
            {
                "name": "boot_memory",
                "description": "Persist a small canonical boot/state memory on disk and keep it hot in runtime cache",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "action": { "type": "string", "enum": ["set", "get", "list", "delete", "status"], "description": "Action to perform" },
                        "key": { "type": "string", "description": "Canonical boot memory key" },
                        "value": { "description": "JSON value to persist for the boot memory entry" },
                        "tags": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Optional tags for organization" },
                        "source_refs": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Optional source references backing this boot memory" }
                    },
                    "required": ["agent_id", "action"]
                }
            },
            // =================================================================
            // v0.7.0: Diagnostic tools — metrics, type_trace, diagram
            // =================================================================
            {
                "name": "metrics",
                "description": "Structural codebase metrics: LOC, child counts, degree, PageRank per file/function/struct. Supports scope filtering and sorting.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "node_types": { "type": "array", "items": { "type": "string" }, "default": ["file"], "description": "Filter by node type: file, function, class, struct, module" },
                        "top_k": { "type": "integer", "default": 50, "description": "Maximum results to return" },
                        "sort": { "type": "string", "default": "loc_desc", "description": "Sort order: loc_desc, complexity_desc, name_asc" }
                    },
                    "required": ["agent_id"]
                }
            },
            {
                "name": "type_trace",
                "description": "Cross-file type usage tracing. BFS from a type/struct/enum node to find all usage sites across the codebase. Supports forward, reverse, and bidirectional tracing.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "target": { "type": "string", "description": "Type name or external_id to trace" },
                        "direction": { "type": "string", "default": "forward", "description": "BFS direction: forward, reverse, both" },
                        "max_hops": { "type": "integer", "default": 4, "description": "Maximum BFS hops" },
                        "top_k": { "type": "integer", "default": 50, "description": "Maximum results" },
                        "group_by_file": { "type": "boolean", "default": true, "description": "Group results by file" }
                    },
                    "required": ["agent_id", "target"]
                }
            },
            {
                "name": "diagram",
                "description": "Generate a visual graph diagram in Mermaid or DOT format. Centers on a node/query or shows top-N by PageRank. Supports scope, type filtering, and layout options.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "center": { "type": "string", "description": "Seed query or node_id to center the diagram on" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "format": { "type": "string", "default": "mermaid", "description": "Output format: mermaid or dot" },
                        "max_nodes": { "type": "integer", "default": 30, "description": "Maximum nodes in diagram" },
                        "depth": { "type": "integer", "default": 2, "description": "Max BFS depth from center" },
                        "node_types": { "type": "array", "items": { "type": "string" }, "description": "Filter by node types" },
                        "show_relations": { "type": "boolean", "default": true, "description": "Show edge labels" },
                        "show_pagerank": { "type": "boolean", "default": false, "description": "Show PageRank in node labels" },
                        "direction": { "type": "string", "default": "TD", "description": "Layout direction: TD (top-down) or LR (left-right)" }
                    },
                    "required": ["agent_id"]
                }
            },
            // =================================================================
            // v0.8.0: memorize — first L1GHT writer; agent durable memory
            // =================================================================
            {
                "name": "memorize",
                "description": "Write structured knowledge claims as a valid .light.md (L1GHT protocol) file, then ingest it so evidence markers bridge to real code nodes. Returns path + ingest counts. The first tool that generates L1GHT markdown rather than only parsing it.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "node_label": { "type": "string", "description": "Entity name — becomes the Node: frontmatter header and # title" },
                        "title": { "type": "string", "description": "Section heading (## <title>); defaults to node_label" },
                        "state": { "type": "string", "description": "State: frontmatter value (default 'authored')" },
                        "claims": {
                            "type": "array",
                            "description": "Knowledge claims to encode as L1GHT markers",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "label": { "type": "string", "description": "Entity name for the marker" },
                                    "text": { "type": "string", "description": "Prose line above the marker (defaults to label)" },
                                    "kind": { "type": "string", "enum": ["entity", "state", "event"], "default": "entity", "description": "Claim kind — controls the glyph (⍂/⍐/⍌)" },
                                    "confidence": { "type": "string", "description": "Confidence value or word, e.g. '0.7' or 'high'" },
                                    "ambiguity": { "type": "string", "description": "Ambiguity descriptor" },
                                    "evidence": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Repo-relative code paths (one [𝔻 evidence:] per path)" },
                                    "depends_on": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Dependency labels (one [⟁ depends_on:] per entry)" }
                                },
                                "required": ["label"]
                            }
                        },
                        "output_path": { "type": "string", "description": "Override output file path; default <runtime_root>/agent-memory/<slug>.light.md" },
                        "namespace": { "type": "string", "description": "Graph namespace for ingest (default 'light')" },
                        "ingest_after": { "type": "boolean", "default": true, "description": "Run ingest after writing (default true)" },
                        "mode": { "type": "string", "default": "merge", "description": "Ingest merge mode: 'merge' (default) or 'replace'" }
                    },
                    "required": ["agent_id", "node_label", "claims"]
                }
            }
        ]
    })
}

// ---------------------------------------------------------------------------
// Read-only attach: mutating-tool deny-list
// ---------------------------------------------------------------------------

/// Tools that mutate graph/plasticity/disk state and must be refused when the
/// session is attached read-only. Read-only/analysis tools are NOT listed here
/// and continue to work normally. `persist` is handled specially (see
/// [`read_only_denied`]) because its `status` action is read-only.
const READ_ONLY_DENIED_TOOLS: &[&str] = &[
    "ingest",
    "apply",
    "apply_batch",
    "edit_commit",
    "memorize",
    "learn",
    "daemon_start",
    "auto_ingest_start",
];

/// Returns true if `tool_name` must be refused in read-only attach mode.
///
/// Normalizes the optional `m1nd.`/`m1nd_` prefix first so `apply`, `m1nd_apply`
/// and `m1nd.apply` are all caught. `persist` is allowed only for its read-only
/// `action == "status"`; every other persist action (`save`/`checkpoint`/`load`)
/// writes graph/disk state and is denied. `edit_preview` is intentionally
/// allowed: it stages an in-memory preview and never writes to disk; only
/// `edit_commit` performs the write.
fn read_only_denied(tool_name: &str, params: &serde_json::Value) -> bool {
    let bare = tool_name
        .strip_prefix("m1nd.")
        .or_else(|| tool_name.strip_prefix("m1nd_"))
        .unwrap_or(tool_name);
    if READ_ONLY_DENIED_TOOLS.contains(&bare) {
        return true;
    }
    if bare == "persist" {
        let action = params
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("status");
        return !action.eq_ignore_ascii_case("status");
    }
    false
}

/// Real code-writing tools the M1ND_PROOF_GATE guards. These are exactly the
/// tools that perform an on-disk write of agent-supplied content. `edit_preview`
/// is deliberately excluded: it only stages an in-memory preview and never
/// writes — same stance as the read-only gate.
const PROOF_GATED_WRITE_TOOLS: &[&str] = &["apply", "apply_batch", "edit_commit"];

/// Returns the normalized (prefix-stripped) tool name if `tool_name` is a
/// proof-gated code-writing tool, else `None`. Mirrors the prefix handling in
/// [`read_only_denied`].
fn proof_gated_write_tool(tool_name: &str) -> Option<&'static str> {
    let bare = tool_name
        .strip_prefix("m1nd.")
        .or_else(|| tool_name.strip_prefix("m1nd_"))
        .unwrap_or(tool_name);
    PROOF_GATED_WRITE_TOOLS
        .iter()
        .copied()
        .find(|&t| t == bare)
}

/// Collect the raw target file path(s) a write call will touch, exactly as the
/// agent supplied them (pre-normalization). For `apply`/`edit_preview` this is
/// `params["file_path"]`; for `apply_batch` it is every `params["edits"][*]
/// ["file_path"]`; for `edit_commit` there is no path in params, so it is
/// recovered from the staged preview (`state.edit_previews[preview_id]
/// .file_path`). Returns the gathered raw targets (may be empty if params are
/// malformed — the gate treats an empty/unresolvable target set as unproven).
fn proof_gate_targets(
    bare_tool: &str,
    params: &serde_json::Value,
    state: &SessionState,
) -> Vec<String> {
    match bare_tool {
        "apply" => params
            .get("file_path")
            .and_then(|v| v.as_str())
            .map(|s| vec![s.to_string()])
            .unwrap_or_default(),
        "apply_batch" => params
            .get("edits")
            .and_then(|v| v.as_array())
            .map(|edits| {
                edits
                    .iter()
                    .filter_map(|e| e.get("file_path").and_then(|v| v.as_str()))
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default(),
        "edit_commit" => params
            .get("preview_id")
            .and_then(|v| v.as_str())
            .and_then(|pid| state.edit_previews.get(pid))
            .map(|preview| vec![preview.file_path.clone()])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Tier 3: memory at point-of-relevance (`_m1nd.memory_nearby`)
// ---------------------------------------------------------------------------

/// Tools whose results carry rankable node ids worth checking for nearby memory.
fn tool_has_memory_anchors(tool: &str) -> bool {
    let bare = tool
        .strip_prefix("m1nd.")
        .or_else(|| tool.strip_prefix("m1nd_"))
        .unwrap_or(tool);
    matches!(
        bare,
        "activate" | "seek" | "search" | "surgical_context" | "surgical_context_v2"
    )
}

/// Parse a confidence value out of a memory marker label, if present.
/// Markers authored via `memorize` embed `[𝔻 confidence: 0.9]` in the label text.
fn parse_marker_confidence(label: &str) -> Option<f64> {
    let pos = label.find("confidence:")? + "confidence:".len();
    let tail = &label[pos..];
    let num: String = tail
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    num.parse::<f64>().ok()
}

/// Build `_m1nd.memory_nearby`: for the top result node ids of a query/seek/
/// activate/surgical result, surface any memorized claim that anchors to them
/// via a `grounded_in` edge (marker → code), so the agent sees prior
/// conclusions WITHOUT issuing another query.
///
/// Best-effort and capped at 3. `evidence_fresh` is a cheap signal: true when
/// the cited code file still exists on disk (re-hashing on every query would be
/// too costly here; `audit(checks=["evidence_freshness"])` remains the
/// authoritative hash-level check). Returns `None` when the tool has no anchors
/// or nothing is found.
fn memory_nearby_for_result(
    state: &SessionState,
    tool: &str,
    result: &serde_json::Value,
) -> Option<Vec<serde_json::Value>> {
    if !tool_has_memory_anchors(tool) {
        return None;
    }

    // Collect up to a few top result node ids (labels / external ids).
    let results = result.get("results").and_then(|v| v.as_array())?;
    let top_ids: Vec<String> = results
        .iter()
        .take(3)
        .filter_map(|r| {
            r.get("node_id")
                .or_else(|| r.get("label"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();
    if top_ids.is_empty() {
        return None;
    }

    let graph = state.graph.read();
    let grounded_in = graph.strings.lookup("grounded_in")?;
    let evidenced_by_tag = graph.strings.lookup("light:evidenced_by");

    // Map each requested top id → its node index (skip ids not in the graph).
    let mut target_idx: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    for id in &top_ids {
        if let Some(nid) = graph.resolve_id(id) {
            target_idx.insert(nid.as_usize(), id.clone());
        }
    }
    if target_idx.is_empty() {
        return None;
    }

    // Walk markers: every node with an outgoing `grounded_in` edge whose target
    // is one of our top result nodes is a memory anchor for that result.
    let node_count = graph.nodes.count as usize;
    let mut out: Vec<serde_json::Value> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    'outer: for src_idx in 0..node_count {
        // Only consider memory/light markers when the tag is present in the graph.
        if let Some(tag) = evidenced_by_tag {
            let is_marker = graph
                .nodes
                .tags
                .get(src_idx)
                .is_some_and(|tags| tags.contains(&tag));
            if !is_marker {
                continue;
            }
        }
        let src_nid = m1nd_core::types::NodeId::new(src_idx as u32);
        for edge_i in graph.csr.out_range(src_nid) {
            if graph.csr.relations[edge_i] != grounded_in {
                continue;
            }
            let tgt_idx = graph.csr.targets[edge_i].as_usize();
            let Some(anchor_id) = target_idx.get(&tgt_idx) else {
                continue;
            };
            let claim = graph.strings.resolve(graph.nodes.label[src_idx]).to_string();
            if !seen.insert(claim.clone()) {
                continue;
            }
            let confidence = parse_marker_confidence(&claim);
            // Cheap freshness: does the cited code file still exist on disk?
            let tgt_ext = graph
                .id_to_node
                .iter()
                .find(|(_, &nid)| nid.as_usize() == tgt_idx)
                .map(|(interned, _)| graph.strings.resolve(*interned).to_string());
            let evidence_fresh = tgt_ext
                .as_deref()
                .and_then(|ext| state.file_inventory.get(ext))
                .map(|inv| std::path::Path::new(&inv.file_path).exists())
                .unwrap_or(true);

            let mut entry = serde_json::json!({
                "claim": claim,
                "anchored_to": anchor_id,
                "evidence_fresh": evidence_fresh,
            });
            if let Some(c) = confidence {
                entry["confidence"] = serde_json::json!(c);
            }
            out.push(entry);
            if out.len() >= 3 {
                break 'outer;
            }
        }
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

// ---------------------------------------------------------------------------
// `orient` — boot into a task in one call (agent-first cold-start aggregation)
// ---------------------------------------------------------------------------

/// Top-N nodes by PageRank as the global "attention backbone".
///
/// Shared helper factored from the `graph_intelligence` block in
/// `handle_session_handshake` (tools.rs). Returns `{node_id, label, pagerank}`
/// objects, descending by score, skipping zero scores. Empty when PageRank has
/// not been computed yet.
fn top_pagerank_anchors(graph: &m1nd_core::graph::Graph, n: usize) -> Vec<serde_json::Value> {
    if !graph.pagerank_computed || graph.nodes.pagerank.is_empty() {
        return vec![];
    }
    // NodeId → external id reverse map (only for the few nodes we return).
    let mut nid_to_ext: std::collections::HashMap<usize, String> =
        std::collections::HashMap::with_capacity(graph.id_to_node.len());
    for (interned, &nid) in &graph.id_to_node {
        nid_to_ext.insert(nid.as_usize(), graph.strings.resolve(*interned).to_string());
    }
    let count = graph.nodes.count as usize;
    let mut ranked: Vec<(f32, usize)> = (0..count)
        .filter_map(|i| {
            let pr = graph.nodes.pagerank[i].get();
            if pr > 0.0 {
                Some((pr, i))
            } else {
                None
            }
        })
        .collect();
    ranked.sort_unstable_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    ranked.truncate(n);
    ranked
        .into_iter()
        .map(|(pr, idx)| {
            let ext_id = nid_to_ext.get(&idx).cloned().unwrap_or_default();
            let label = graph
                .strings
                .try_resolve(graph.nodes.label[idx])
                .unwrap_or("")
                .to_string();
            serde_json::json!({ "node_id": ext_id, "label": label, "pagerank": pr })
        })
        .collect()
}

/// `orient` — pre-pack an agent's STARTING CONTEXT from a free-form task string.
///
/// AGGREGATION handler: it composes existing primitives rather than
/// reimplementing them.
///   * spread-activation on the task text via `handle_activate` (which uses
///     `SessionState::run_query`, so this works in `--read-only` attach too) →
///     `focus_nodes`.
///   * `memory_nearby_for_result` over the focus nodes → prior conclusions.
///   * `top_pagerank_anchors` → global attention backbone.
///   * coverage state from `state.coverage_sessions` → visited/total +
///     high-PageRank unvisited files (or null when the agent has no session).
///   * the top focus node → concrete `suggested_first_calls` (surgical_context,
///     then why) so the agent's very next move is grounded.
///
/// READ-ONLY SAFE: only queries. Not in `read_only_denied`.
fn handle_orient(
    state: &mut SessionState,
    params: &serde_json::Value,
) -> M1ndResult<serde_json::Value> {
    let agent_id = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: "orient".into(),
            detail: "orient requires an `agent_id` string".into(),
        })?
        .to_string();
    let task = params
        .get("task")
        .and_then(|v| v.as_str())
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: "orient".into(),
            detail: "orient requires a `task` string describing what the agent is about to do"
                .into(),
        })?
        .to_string();
    if task.trim().is_empty() {
        return Err(M1ndError::InvalidParams {
            tool: "orient".into(),
            detail: "orient `task` must be non-empty".into(),
        });
    }
    let top_k = params
        .get("top_k")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .unwrap_or(8)
        .clamp(1, 50);

    // 1. Spread-activate on the task text. handle_activate routes through
    //    run_query, which picks query_readonly in read-only mode — so orient is
    //    safe in --read-only attach. Reuse it wholesale; no reimplementation.
    let activate_input = ActivateInput {
        query: task.clone(),
        agent_id: agent_id.clone(),
        top_k,
        dimensions: vec![
            "structural".into(),
            "semantic".into(),
            "temporal".into(),
            "causal".into(),
        ],
        xlr: true,
        include_ghost_edges: false,
        include_structural_holes: false,
        token_budget: None,
    };
    let activate_out = tools::handle_activate(state, activate_input)?;

    // focus_nodes: compact projection of the activated nodes, ranked by activation.
    let focus_nodes: Vec<serde_json::Value> = activate_out
        .activated
        .iter()
        .take(top_k)
        .map(|a| {
            let path = a
                .provenance
                .as_ref()
                .and_then(|p| p.source_path.clone());
            serde_json::json!({
                "node_id": a.node_id,
                "label": a.label,
                "path": path,
                "pagerank": a.pagerank,
                "activation": a.activation,
                "kind": a.node_type,
            })
        })
        .collect();
    let top_focus_id = activate_out
        .activated
        .first()
        .map(|a| a.node_id.clone());

    // 2. memory_nearby: reuse memory_nearby_for_result over the focus nodes.
    //    It expects a `results` array of `{node_id|label}` — shape one from the
    //    focus nodes (capped at ~5 prior conclusions).
    let memory_nearby = {
        let pseudo_result = serde_json::json!({
            "results": activate_out
                .activated
                .iter()
                .take(5)
                .map(|a| serde_json::json!({ "node_id": a.node_id }))
                .collect::<Vec<_>>(),
        });
        memory_nearby_for_result(state, "activate", &pseudo_result).unwrap_or_default()
    };

    // 3. anchors: global PageRank attention backbone (cap 5).
    let anchors = {
        let graph = state.graph.read();
        top_pagerank_anchors(&graph, 5)
    };

    // 4. coverage: surface visited/total + a few high-PageRank unvisited files
    //    when the agent has a coverage session; otherwise null.
    let coverage = build_orient_coverage(state, &agent_id);

    // 5. suggested_first_calls: lead with surgical_context on the top focus node
    //    (grounded edit prep), then reuse suggest_next for textual guidance.
    let mut suggested_first_calls: Vec<serde_json::Value> = Vec::new();
    if let Some(ref node_id) = top_focus_id {
        suggested_first_calls.push(serde_json::json!({
            "tool": "surgical_context",
            "arguments": { "agent_id": agent_id, "node_id": node_id },
        }));
    }
    suggested_first_calls.push(serde_json::json!({
        "tool": "why",
        "arguments": { "agent_id": agent_id, "query": task },
    }));

    let summary = if let Some(first) = focus_nodes.first() {
        let label = first
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("the top focus node");
        format!(
            "Load {} first ({} focus node(s) activated for this task); then ground it with surgical_context.",
            label,
            focus_nodes.len()
        )
    } else {
        "No focus nodes activated for this task — try ingesting the relevant area or refining the task description.".to_string()
    };

    Ok(serde_json::json!({
        "task": task,
        "focus_nodes": focus_nodes,
        "memory_nearby": memory_nearby,
        "anchors": anchors,
        "coverage": coverage,
        "suggested_first_calls": suggested_first_calls,
        "proof_state": "triaging",
        "summary": summary,
    }))
}

/// Resolve a node id to its absolute on-disk file path for `am_i_stale`.
///
/// Two paths, both grounded in already-recorded state — no new traversal logic:
///   1. The node id is itself a `file::…` inventory key → return its recorded
///      `file_path` directly.
///   2. Otherwise look the node up in the graph and use its provenance
///      `source_path` (the file the node was ingested from).
///
/// Returns `None` when the node is unknown / has no file provenance.
fn resolve_node_file_path(state: &SessionState, node_id: &str) -> Option<String> {
    // Fast path: the node id is already a file inventory key.
    if let Some(entry) = state.file_inventory.get(node_id) {
        return Some(entry.file_path.clone());
    }
    // Otherwise resolve via the graph's provenance source_path.
    let graph = state.graph.read();
    let interned = graph.strings.lookup(node_id)?;
    let nid = *graph.id_to_node.get(&interned)?;
    let prov = graph.resolve_node_provenance(nid);
    prov.source_path
}

/// `am_i_stale` — tell a long-running agent which files in its working set have
/// changed on disk SINCE m1nd ingested them, so it knows to re-read before
/// acting. The perception an agent structurally lacks.
///
/// AGGREGATION handler: it composes existing primitives and reimplements
/// nothing.
///   * `state.file_inventory` is the "what m1nd last saw" baseline — each entry
///     records the absolute `file_path` and the `sha256` captured at ingest.
///   * `audit_handlers::simple_content_hash` recomputes the current on-disk hash
///     with the SAME algorithm the ingest path used, so a recomputed hash is
///     directly comparable to the stored one (shared fn — no second hasher).
///   * `state.coverage_sessions[agent_id].visited_files` is the DEFAULT working
///     set when the caller passes neither `files` nor `nodes`: "you don't even
///     have to tell me what you're holding; I'll check what you've touched".
///
/// READ-ONLY SAFE: only reads disk + inventory. Not in `read_only_denied`.
fn handle_am_i_stale(
    state: &mut SessionState,
    params: &serde_json::Value,
) -> M1ndResult<serde_json::Value> {
    let agent_id = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| M1ndError::InvalidParams {
            tool: "am_i_stale".into(),
            detail: "am_i_stale requires an `agent_id` string".into(),
        })?
        .to_string();

    let explicit_files: Vec<String> = params
        .get("files")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let explicit_nodes: Vec<String> = params
        .get("nodes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    // Decide the working set + its provenance. Each target is (path, node_id?).
    // `note` is a caller-facing explanation when a target couldn't be turned
    // into a checkable path (e.g. an unresolvable node id).
    let mut targets: Vec<(String, Option<String>)> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    let source: &str;

    if !explicit_files.is_empty() {
        source = "explicit_files";
        for path in explicit_files {
            targets.push((path, None));
        }
    } else if !explicit_nodes.is_empty() {
        source = "explicit_nodes";
        for node_id in explicit_nodes {
            match resolve_node_file_path(state, &node_id) {
                Some(path) => targets.push((path, Some(node_id))),
                None => notes.push(format!(
                    "node `{}` could not be resolved to a file path (unknown or not file-backed)",
                    node_id
                )),
            }
        }
    } else if let Some(session) = state.coverage_sessions.get(&agent_id) {
        source = "coverage_session";
        for path in &session.visited_files {
            targets.push((path.clone(), None));
        }
    } else {
        source = "empty";
    }

    // Build a quick lookup from absolute file_path → inventory entry. The
    // inventory is keyed by external_id, but visited_files / explicit files are
    // disk paths, so index by file_path (mirrors audit_handlers usage). We also
    // index by the canonicalized form so a caller passing a non-canonical path
    // (e.g. /var/… on macOS where the inventory recorded /private/var/…) still
    // matches the same baseline.
    let mut inventory_by_path: std::collections::HashMap<String, &crate::session::FileInventoryEntry> =
        std::collections::HashMap::with_capacity(state.file_inventory.len() * 2);
    for entry in state.file_inventory.values() {
        inventory_by_path.insert(entry.file_path.clone(), entry);
        if let Ok(canon) = std::fs::canonicalize(&entry.file_path) {
            inventory_by_path.insert(canon.to_string_lossy().to_string(), entry);
        }
    }

    let mut stale: Vec<serde_json::Value> = Vec::new();
    let mut fresh: Vec<String> = Vec::new();
    let mut unknown: Vec<String> = Vec::new();

    for (path, node_id) in &targets {
        let entry = inventory_by_path.get(path.as_str()).copied().or_else(|| {
            std::fs::canonicalize(path)
                .ok()
                .and_then(|c| inventory_by_path.get(c.to_string_lossy().as_ref()).copied())
        });
        let Some(entry) = entry else {
            // Never ingested → m1nd has no baseline for this path.
            unknown.push(path.clone());
            continue;
        };
        let disk_path = std::path::Path::new(path.as_str());
        if !disk_path.exists() {
            let mut item = serde_json::json!({ "path": path, "reason": "missing" });
            if let Some(nid) = node_id {
                item["node_id"] = serde_json::Value::String(nid.clone());
            }
            stale.push(item);
            continue;
        }
        let current_hash = crate::audit_handlers::simple_content_hash(disk_path);
        match (&entry.sha256, current_hash) {
            (Some(known), Some(now)) if known != &now => {
                let mut item = serde_json::json!({ "path": path, "reason": "changed" });
                if let Some(nid) = node_id {
                    item["node_id"] = serde_json::Value::String(nid.clone());
                }
                stale.push(item);
            }
            (Some(_), Some(_)) => {
                // Hash matches — fresh.
                fresh.push(path.clone());
            }
            _ => {
                // No recorded hash, or the file couldn't be re-read: we can't
                // make a confident staleness judgement, so report as unknown
                // rather than silently calling it fresh.
                unknown.push(path.clone());
            }
        }
    }

    let checked = stale.len() + fresh.len() + unknown.len();

    let summary = if source == "empty" {
        notes.push(
            "no `files`/`nodes` given and agent has no coverage session — nothing to check".into(),
        );
        format!(
            "0 files checked: agent `{}` has no coverage session and you passed no files or nodes.",
            agent_id
        )
    } else if checked == 0 {
        format!(
            "0 files checked ({}): nothing in your working set was tracked in m1nd's file inventory.",
            source
        )
    } else if stale.is_empty() {
        format!(
            "All {} file(s) checked ({}) are fresh — nothing changed on disk since ingest.",
            checked, source
        )
    } else {
        let stale_paths: Vec<&str> = stale
            .iter()
            .filter_map(|s| s.get("path").and_then(|p| p.as_str()))
            .collect();
        let preview: Vec<&str> = stale_paths.iter().take(3).copied().collect();
        let suffix = if stale_paths.len() > preview.len() {
            format!("{}, +{} more", preview.join(", "), stale_paths.len() - preview.len())
        } else {
            preview.join(", ")
        };
        let touched = if source == "coverage_session" {
            "you've touched"
        } else {
            "you're checking"
        };
        format!(
            "{} of {} files {} changed since ingest — re-read {} before editing.",
            stale.len(),
            checked,
            touched,
            suffix
        )
    };

    let mut out = serde_json::json!({
        "checked": checked,
        "stale": stale,
        "fresh": fresh,
        "unknown": unknown,
        "source": source,
        "summary": summary,
    });
    if !notes.is_empty() {
        out["notes"] = serde_json::Value::Array(
            notes.into_iter().map(serde_json::Value::String).collect(),
        );
    }
    Ok(out)
}

/// Build the `coverage` block for `orient` from `state.coverage_sessions`.
///
/// Returns `{visited, total, unvisited_high_value:[paths]}` when the agent has a
/// coverage session, otherwise `null`. `unvisited_high_value` lists up to 5 file
/// paths with the highest PageRank that the agent has not yet visited.
fn build_orient_coverage(state: &SessionState, agent_id: &str) -> serde_json::Value {
    let Some(session) = state.coverage_sessions.get(agent_id) else {
        return serde_json::Value::Null;
    };
    let graph = state.graph.read();
    let total = graph.nodes.count as usize;
    let visited = session.visited_nodes.len();

    // High-PageRank file nodes the agent has not visited yet.
    let mut unvisited: Vec<(f32, String)> = Vec::new();
    if graph.pagerank_computed && !graph.nodes.pagerank.is_empty() {
        for (interned, &nid) in &graph.id_to_node {
            let ext = graph.strings.resolve(*interned).to_string();
            if session.visited_nodes.contains(&ext) || session.visited_files.contains(&ext) {
                continue;
            }
            // Prefer file-level nodes for the "what to read next" hint.
            if !ext.starts_with("file::") {
                continue;
            }
            let idx = nid.as_usize();
            let pr = graph
                .nodes
                .pagerank
                .get(idx)
                .map(|p| p.get())
                .unwrap_or(0.0);
            if pr > 0.0 {
                let path = ext.strip_prefix("file::").unwrap_or(&ext).to_string();
                unvisited.push((pr, path));
            }
        }
        unvisited.sort_unstable_by(|a, b| {
            b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal)
        });
        unvisited.truncate(5);
    }
    serde_json::json!({
        "visited": visited,
        "total": total,
        "unvisited_high_value": unvisited.into_iter().map(|(_, p)| p).collect::<Vec<_>>(),
    })
}

// ---------------------------------------------------------------------------
// Free dispatch functions — used by both JSON-RPC stdio and HTTP API.
// Zero duplication: McpServer::dispatch_tool() delegates to these.
// ---------------------------------------------------------------------------

/// Dispatch a tool call by name. Normalizes underscores to dots.
/// Used by both JSON-RPC stdio and HTTP API -- zero duplication.
///
/// v0.4.0: wraps all responses with _m1nd metadata, tracks savings.
pub fn dispatch_tool(
    state: &mut SessionState,
    tool_name: &str,
    params: &serde_json::Value,
) -> M1ndResult<serde_json::Value> {
    let normalized = tool_name.to_string();
    let start = std::time::Instant::now();

    // Read-only attach gate: refuse mutating tools BEFORE any dispatch or
    // side-effecting tick so the writer's on-disk state can never be touched.
    if state.read_only && read_only_denied(&normalized, params) {
        return Err(M1ndError::InvalidParams {
            tool: normalized.clone(),
            detail: format!(
                "m1nd is attached read-only (--read-only); mutation tool '{}' is disabled. Detach or run a read-write instance to modify state.",
                normalized
            ),
        });
    }

    // Extract agent_id for tracking
    let agent_id = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // M1ND_PROOF_GATE: when ON, refuse a real code-writing tool BEFORE any write
    // unless every target it will touch was driven to proof_state ==
    // "ready_to_edit" this session (via surgical_context_v2). edit_preview is
    // allowed (it only stages). All targets are normalized through the same
    // scope normalizer the recorder used, so proved==edited keys compare equal.
    if proof_gate_enabled() {
        if let Some(bare_tool) = proof_gated_write_tool(&normalized) {
            let targets = proof_gate_targets(bare_tool, params, state);
            let unproven: Vec<String> = if targets.is_empty() {
                // Malformed/unresolvable target set: refuse rather than allow an
                // unverifiable write through. Surface the tool itself.
                vec![format!("<unresolved target for {bare_tool}>")]
            } else {
                targets
                    .iter()
                    .filter(|t| !state.is_proof_ready(&agent_id, t))
                    .cloned()
                    .collect()
            };
            if !unproven.is_empty() {
                let unproven_list = unproven.join(", ");
                let first = unproven.first().cloned().unwrap_or_default();
                return Err(M1ndError::InvalidParams {
                    tool: normalized.clone(),
                    detail: format!(
                        "M1ND_PROOF_GATE is on: {count} target(s) not proven ready_to_edit for agent_id='{agent}': {unproven_list}. \
Run surgical_context_v2 (agent_id='{agent}', path='{first}') for each unproven target to gather context and reach proof_state==ready_to_edit, then retry '{tool}'.",
                        count = unproven.len(),
                        agent = agent_id,
                        tool = normalized,
                    ),
                });
            }
        }
    }

    let query_preview = params
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            params
                .get("claim")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| params.get("node_id").and_then(|v| v.as_str()).unwrap_or(""))
        })
        .to_string();

    if !matches!(
        normalized.as_str(),
        "recovery_playbook"
            | "trust_selftest"
            | "mission_start"
            | "mission_event"
            | "mission_next"
            | "mission_verify"
            | "mission_handoff"
            | "mission_close"
    ) {
        auto_ingest::maybe_tick_auto_ingest(state, &normalized)?;
    }

    let result = match normalized.as_str() {
        name if name.starts_with("perspective_") => dispatch_perspective_tool(state, name, params),
        name if name.starts_with("lock_") => dispatch_lock_tool(state, name, params),
        _ => dispatch_core_tool(state, &normalized, params),
    }
    .map_err(|error| match error {
        M1ndError::Serde(detail) => M1ndError::InvalidParams {
            tool: normalized.clone(),
            detail: help_guidance::runtime_error_guidance_hint(&normalized, &detail.to_string()),
        },
        M1ndError::InvalidParams { tool, detail } => {
            let normalized_tool = tool
                .strip_prefix("m1nd.")
                .or_else(|| tool.strip_prefix("m1nd_"))
                .unwrap_or(&tool)
                .to_string();
            M1ndError::InvalidParams {
                tool: tool.clone(),
                detail: help_guidance::runtime_error_guidance_hint(&normalized_tool, &detail),
            }
        }
        other => other,
    });

    // Post-dispatch: track savings + log query + add _m1nd metadata
    let mut result = result;
    if let Ok(ref mut value) = result {
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        let result_count = value
            .get("results")
            .and_then(|v| v.as_array())
            .map_or(0, |a| a.len());

        // Track savings (skip meta tools)
        if !matches!(
            normalized.as_str(),
            "health"
                | "session_handshake"
                | "trust_selftest"
                | "recovery_playbook"
                | "doctor"
                | "help"
                | "mission_start"
                | "mission_event"
                | "mission_next"
                | "mission_verify"
                | "mission_handoff"
                | "mission_close"
                | "savings"
                | "report"
        ) {
            state.savings_tracker.record(&normalized, result_count);
            state.global_savings.total_queries += 1;
        }

        // Log query
        state.log_query(
            &normalized,
            &agent_id,
            elapsed_ms,
            result_count,
            &query_preview,
        );

        // Additive `_m1nd` response envelope (Tier 2). Gated behind
        // M1ND_RESPONSE_ENVELOPE (default ON; set to "0"/"false" to disable).
        // ADDITIVE ONLY: attaches a `_m1nd` object to JSON-object results;
        // never removes or renames existing fields. Non-object results (rare)
        // are left untouched.
        if response_envelope_enabled() && value.is_object() {
            let session_saved = state.savings_tracker.tokens_saved;
            let global_saved = state.global_savings.total_tokens_saved + session_saved;
            // Builders read the result; snapshot it once to avoid a borrow
            // conflict with the mutable insert below.
            let snapshot = value.clone();
            let mut meta =
                personality::build_m1nd_meta(&normalized, &snapshot, session_saved, global_saved);
            // Promote the headline fields the contract calls for so agents get
            // them without reaching into nested `savings`.
            let summary = personality::personality_line(&normalized, &snapshot);
            if !summary.is_empty() {
                meta["summary"] = serde_json::Value::String(summary);
            }
            meta["tokens_saved"] = serde_json::json!(session_saved);
            meta["read_only"] = serde_json::json!(state.read_only);

            // Tier 3: memory at point-of-relevance (additive, capped, best-effort).
            if let Some(nearby) = memory_nearby_for_result(state, &normalized, &snapshot) {
                if !nearby.is_empty() {
                    meta["memory_nearby"] = serde_json::Value::Array(nearby);
                }
            }

            if let Some(obj) = value.as_object_mut() {
                obj.insert("_m1nd".to_string(), meta);
            }
        }
    }

    result
}

/// Dispatch core + superpowers tools (35 tools).
fn dispatch_core_tool(
    state: &mut SessionState,
    tool_name: &str,
    params: &serde_json::Value,
) -> M1ndResult<serde_json::Value> {
    match tool_name {
        "orient" => handle_orient(state, params),
        "am_i_stale" => handle_am_i_stale(state, params),
        "activate" => {
            let input: ActivateInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = tools::handle_activate(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "impact" => {
            let input: ImpactInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = tools::handle_impact(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "missing" => {
            let input: MissingInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_missing(state, input)
        }
        "why" => {
            let input: WhyInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_why(state, input)
        }
        "warmup" => {
            let input: WarmupInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_warmup(state, input)
        }
        "counterfactual" => {
            let input: CounterfactualInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_counterfactual(state, input)
        }
        "predict" => {
            let input: PredictInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_predict(state, input)
        }
        "fingerprint" => {
            let input: FingerprintInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_fingerprint(state, input)
        }
        "drift" => {
            let input: DriftInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_drift(state, input)
        }
        "learn" => {
            let input: LearnInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_learn(state, input)
        }
        "ingest" => {
            let input: IngestInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_ingest(state, input)
        }
        "document_resolve" => {
            let input: DocumentResolveInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            universal_docs::resolve_document(state, input)
        }
        "document_provider_health" => {
            let input: DocumentProviderHealthInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            universal_docs::provider_health(input)
        }
        "document_bindings" => {
            let input: DocumentBindingsInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            universal_docs::document_bindings(state, input)
        }
        "document_drift" => {
            let input: DocumentDriftInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            universal_docs::document_drift(state, input)
        }
        "auto_ingest_start" => {
            let input: AutoIngestStartInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            auto_ingest::handle_auto_ingest_start(state, input)
        }
        "auto_ingest_stop" => {
            let input: AutoIngestStopInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            auto_ingest::handle_auto_ingest_stop(state, input)
        }
        "auto_ingest_status" => {
            let input: AutoIngestStatusInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            auto_ingest::handle_auto_ingest_status(state, input)
        }
        "auto_ingest_tick" => {
            let input: AutoIngestTickInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            auto_ingest::handle_auto_ingest_tick(state, input)
        }
        "resonate" => {
            let input: ResonateInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_resonate(state, input)
        }
        "health" => {
            let input: HealthInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = tools::handle_health(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "session_handshake" => {
            let input: SessionHandshakeInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_session_handshake(state, input)
        }
        "trust_selftest" => {
            let input: TrustSelftestInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_trust_selftest(state, input)
        }
        "recovery_playbook" => {
            let input: RecoveryPlaybookInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_recovery_playbook(state, input)
        }
        "doctor" => {
            let input: DoctorInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            tools::handle_doctor(state, input)
        }
        "mission_start" => {
            let input: layers::MissionStartInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            mission_handlers::handle_mission_start(state, input)
        }
        "mission_event" => {
            let input: layers::MissionEventInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            mission_handlers::handle_mission_event(state, input)
        }
        "mission_next" => {
            let input: layers::MissionNextInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            mission_handlers::handle_mission_next(state, input)
        }
        "mission_verify" => {
            let input: layers::MissionVerifyInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            mission_handlers::handle_mission_verify(state, input)
        }
        "mission_handoff" => {
            let input: layers::MissionHandoffInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            mission_handlers::handle_mission_handoff(state, input)
        }
        "mission_close" => {
            let input: layers::MissionCloseInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            mission_handlers::handle_mission_close(state, input)
        }
        // L2-L7: Superpowers layer tools
        "seek" => {
            let input: layers::SeekInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_seek(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "scan" => {
            let input: layers::ScanInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_scan(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "timeline" => {
            let input: layers::TimelineInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_timeline(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "diverge" => {
            let input: layers::DivergeInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_diverge(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "trail_save" => {
            let input: layers::TrailSaveInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_trail_save(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "trail_resume" => {
            let input: layers::TrailResumeInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_trail_resume(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "trail_merge" => {
            let input: layers::TrailMergeInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_trail_merge(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "trail_list" => {
            let input: layers::TrailListInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_trail_list(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "hypothesize" => {
            let input: layers::HypothesizeInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_hypothesize(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "differential" => {
            let input: layers::DifferentialInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_differential(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "trace" => {
            let input: layers::TraceInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "trace".into(),
                    detail: e.to_string(),
                })?;
            let output = layer_handlers::handle_trace(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "validate_plan" => {
            let input: layers::ValidatePlanInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_validate_plan(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "federate" => {
            let input: layers::FederateInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_federate(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "antibody_scan" => {
            let input: layers::AntibodyScanInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_antibody_scan(state, input)
        }
        "antibody_list" => {
            let input: layers::AntibodyListInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_antibody_list(state, input)
        }
        "antibody_create" => {
            let input: layers::AntibodyCreateInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_antibody_create(state, input)
        }
        "flow_simulate" => {
            let input: layers::FlowSimulateInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_flow_simulate(state, input)
        }
        "epidemic" => {
            let input: layers::EpidemicInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_epidemic(state, input)
        }
        "tremor" => {
            let input: layers::TremorInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_tremor(state, input)
        }
        "trust" => {
            let input: layers::TrustInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_trust(state, input)
        }
        "heuristics_surface" => {
            let input: surgical::HeuristicsSurfaceInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = surgical_handlers::handle_heuristics_surface(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "layers" => {
            let input: layers::LayersInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_layers(state, input)
        }
        "layer_inspect" => {
            let input: layers::LayerInspectInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_layer_inspect(state, input)
        }
        "ghost_edges" => {
            let input: layers::GhostEdgesInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_ghost_edges(state, input)
        }
        "taint_trace" => {
            let input: layers::TaintTraceInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_taint_trace(state, input)
        }
        "twins" => {
            let input: layers::TwinsInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_twins(state, input)
        }
        "refactor_plan" => {
            let input: layers::RefactorPlanInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_refactor_plan(state, input)
        }
        "runtime_overlay" => {
            let input: layers::RuntimeOverlayInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            layer_handlers::handle_runtime_overlay(state, input)
        }
        // -----------------------------------------------------------------
        // v0.4.0: search, help, panoramic, savings, report
        // -----------------------------------------------------------------
        "search" => {
            let input: layers::SearchInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = search_handlers::handle_search(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "scan_all" => {
            let input: layers::ScanAllInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::audit_handlers::handle_scan_all(state, input)
        }
        "cross_verify" => {
            let input: layers::CrossVerifyInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::audit_handlers::handle_cross_verify(state, input)
        }
        "coverage_session" => {
            let input: layers::CoverageSessionInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::audit_handlers::handle_coverage_session(state, input)
        }
        "external_references" => {
            let input: layers::ExternalReferencesInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::audit_handlers::handle_external_references(state, input)
        }
        "federate_auto" => {
            let input: layers::FederateAutoInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::audit_handlers::handle_federate_auto(state, input)
        }
        "glob" => {
            let input: layers::GlobInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = search_handlers::handle_glob(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "help" => {
            let input: layers::HelpInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = search_handlers::handle_help(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "report" => {
            let input: layers::ReportInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = report_handlers::handle_report(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "audit" => {
            let input: layers::AuditInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::audit_handlers::handle_audit(state, input)
        }
        "daemon_start" => {
            let input: layers::DaemonStartInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::daemon_handlers::handle_daemon_start(state, input)
        }
        "daemon_stop" => {
            let input: layers::DaemonStopInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::daemon_handlers::handle_daemon_stop(state, input)
        }
        "daemon_status" => {
            let input: layers::DaemonStatusInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::daemon_handlers::handle_daemon_status(state, input)
        }
        "daemon_tick" => {
            let input: layers::DaemonTickInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::daemon_handlers::handle_daemon_tick(state, input)
        }
        "alerts_list" => {
            let input: layers::AlertsListInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::daemon_handlers::handle_alerts_list(state, input)
        }
        "alerts_ack" => {
            let input: layers::AlertsAckInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::daemon_handlers::handle_alerts_ack(state, input)
        }
        "panoramic" => {
            let input: layers::PanoramicInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = report_handlers::handle_panoramic(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "savings" => {
            let input: layers::SavingsInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = report_handlers::handle_savings(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        // -----------------------------------------------------------------
        // Surgical: context + apply
        // -----------------------------------------------------------------
        "surgical_context" => {
            let input: crate::protocol::surgical::SurgicalContextInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "surgical_context".into(),
                    detail: e.to_string(),
                })?;
            let output = surgical_handlers::handle_surgical_context(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "apply" => {
            let input: crate::protocol::surgical::ApplyInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "apply".into(),
                    detail: e.to_string(),
                })?;
            let output = surgical_handlers::handle_apply(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        // -----------------------------------------------------------------
        // Surgical V2: context_v2 + apply_batch
        // -----------------------------------------------------------------
        "surgical_context_v2" => {
            let input: crate::protocol::surgical::SurgicalContextV2Input =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "surgical_context_v2".into(),
                    detail: e.to_string(),
                })?;
            let output = surgical_handlers::handle_surgical_context_v2(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "apply_batch" => {
            let input: crate::protocol::surgical::ApplyBatchInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "apply_batch".into(),
                    detail: e.to_string(),
                })?;
            let output = surgical_handlers::handle_apply_batch(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "edit_preview" => {
            let input: crate::protocol::surgical::EditPreviewInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "edit_preview".into(),
                    detail: e.to_string(),
                })?;
            let output = surgical_handlers::handle_edit_preview(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "edit_commit" => {
            let input: crate::protocol::surgical::EditCommitInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "edit_commit".into(),
                    detail: e.to_string(),
                })?;
            let output = surgical_handlers::handle_edit_commit(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        // -----------------------------------------------------------------
        // View: lightweight file reader
        // -----------------------------------------------------------------
        "view" => {
            let input: crate::protocol::surgical::ViewInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "view".into(),
                    detail: e.to_string(),
                })?;
            let output = surgical_handlers::handle_view(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "batch_view" => {
            let input: crate::protocol::surgical::BatchViewInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "batch_view".into(),
                    detail: e.to_string(),
                })?;
            let output = surgical_handlers::handle_batch_view(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "persist" => {
            let input: crate::persist_handlers::PersistInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::persist_handlers::handle_persist(state, input)
        }
        "boot_memory" => {
            let input: crate::boot_memory_handlers::BootMemoryInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::boot_memory_handlers::handle_boot_memory(state, input)
        }
        "memorize" => {
            let input: crate::light_author_handlers::LightAuthorInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            crate::light_author_handlers::handle_light_author(state, input)
        }
        // -----------------------------------------------------------------
        // v0.7.0: Diagnostic tools
        // -----------------------------------------------------------------
        "metrics" => {
            let input: layers::MetricsInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_metrics(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "type_trace" => {
            let input: layers::TypeTraceInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_type_trace(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        "diagram" => {
            let input: layers::DiagramInput =
                serde_json::from_value(params.clone()).map_err(M1ndError::Serde)?;
            let output = layer_handlers::handle_diagram(state, input)?;
            serde_json::to_value(output).map_err(M1ndError::Serde)
        }
        _ => Err(M1ndError::UnknownTool {
            name: tool_name.to_string(),
        }),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or(0)
}

/// MCP protocol version this server implements/prefers.
pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// Older MCP protocol versions we remain compatible with. If a client offers one
/// of these we echo it back (per the MCP spec's version-negotiation handshake);
/// otherwise we reply with our preferred [`MCP_PROTOCOL_VERSION`].
const MCP_SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

/// Negotiate the protocol version: honor the client's requested version when we
/// support it, otherwise fall back to our preferred version.
fn negotiate_protocol_version(requested: Option<&str>) -> &'static str {
    if let Some(req) = requested {
        if let Some(v) = MCP_SUPPORTED_PROTOCOL_VERSIONS
            .iter()
            .copied()
            .find(|v| *v == req)
        {
            return v;
        }
    }
    MCP_PROTOCOL_VERSION
}

/// Transport-agnostic MCP method dispatch.
///
/// Handles the JSON-RPC MCP protocol methods (`initialize`,
/// `notifications/initialized`, `tools/list`, `tools/call`, method-not-found)
/// against a borrowed [`SessionState`]. Used by both the stdio transport
/// (via [`McpServer::dispatch`]) and the Streamable-HTTP transport
/// (`mcp_http::handle_mcp_post`), so both bind to the same shared graph.
///
/// Note: the stdio-only live FS watcher refresh for `daemon_start`/`daemon_stop`
/// is NOT performed here — the caller (`McpServer::dispatch`) handles that after
/// this returns, since it requires `&mut McpServer`, not just `&mut SessionState`.
pub fn handle_mcp_method(state: &mut SessionState, request: &JsonRpcRequest) -> JsonRpcResponse {
    let method = request.method.as_str();

    match method {
        "initialize" => {
            let requested = request
                .params
                .get("protocolVersion")
                .and_then(|v| v.as_str());
            let protocol_version = negotiate_protocol_version(requested);
            JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: request.id.clone(),
                result: Some(serde_json::json!({
                    "protocolVersion": protocol_version,
                    "serverInfo": {
                        "name": "m1nd-mcp",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "capabilities": {
                        "tools": {},
                    },
                    "instructions": M1ND_INSTRUCTIONS,
                })),
                error: None,
            }
        }
        "notifications/initialized" => {
            // No response needed for notifications, but we return one since caller expects it
            JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: request.id.clone(),
                result: Some(serde_json::Value::Null),
                error: None,
            }
        }
        "tools/list" => JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id: request.id.clone(),
            result: Some(tool_schemas()),
            error: None,
        },
        "tools/call" => {
            // Extract tool name and arguments from params
            let tool_name = request
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let arguments = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

            // Track agent session from arguments
            if let Some(agent_id) = arguments.get("agent_id").and_then(|v| v.as_str()) {
                state.track_agent(agent_id);
                if state.daemon_state.active
                    && should_autotick_daemon(tool_name)
                    && state.daemon_state.last_tick_ms.is_some_and(|last| {
                        now_ms().saturating_sub(last) >= state.daemon_state.poll_interval_ms
                    })
                {
                    run_daemon_tick(state, "traffic");
                }
            }

            // MCP spec: tool execution errors -> isError content, not JSON-RPC errors
            match dispatch_tool(state, tool_name, &arguments) {
                Ok(result) => JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: request.id.clone(),
                    result: Some(serde_json::json!({
                        "content": [{
                            "type": "text",
                            "text": serde_json::to_string_pretty(&result).unwrap_or_default(),
                        }]
                    })),
                    error: None,
                },
                Err(e) => JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: request.id.clone(),
                    result: Some(serde_json::json!({
                        "content": [{
                            "type": "text",
                            "text": format!("Error: {}", e),
                        }],
                        "isError": true
                    })),
                    error: None,
                },
            }
        }
        _ => {
            // Method not found — JSON-RPC protocol error
            JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: request.id.clone(),
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", method),
                    data: None,
                }),
            }
        }
    }
}

fn should_autotick_daemon(tool_name: &str) -> bool {
    !matches!(
        tool_name,
        "daemon_start"
            | "daemon_stop"
            | "daemon_status"
            | "daemon_tick"
            | "alerts_list"
            | "alerts_ack"
            | "session_handshake"
            | "trust_selftest"
            | "recovery_playbook"
            | "mission_start"
            | "mission_event"
            | "mission_next"
            | "mission_verify"
            | "mission_handoff"
            | "mission_close"
    )
}

fn background_tick_if_due(state: &mut SessionState) {
    if !state.daemon_state.active || state.daemon_state.poll_interval_ms == 0 {
        return;
    }
    let due = state
        .daemon_state
        .last_tick_ms
        .is_none_or(|last| now_ms().saturating_sub(last) >= state.daemon_state.poll_interval_ms);
    if !due {
        return;
    }

    let _ = crate::daemon_handlers::handle_daemon_tick(
        state,
        layers::DaemonTickInput {
            agent_id: "daemon".into(),
            max_files: 32,
        },
    );
}

fn run_daemon_tick(state: &mut SessionState, trigger: &str) {
    if state.daemon_state.tick_in_flight {
        state.daemon_state.pending_rerun = true;
        return;
    }

    state.daemon_state.tick_in_flight = true;
    state.daemon_state.last_tick_trigger = Some(trigger.to_string());
    let _ = crate::daemon_handlers::handle_daemon_tick(
        state,
        layers::DaemonTickInput {
            agent_id: "daemon".into(),
            max_files: 32,
        },
    );
    state.daemon_state.tick_in_flight = false;

    if state.daemon_state.pending_rerun {
        state.daemon_state.pending_rerun = false;
        state.daemon_state.last_tick_trigger = Some("reconciliation".into());
        state.daemon_state.tick_in_flight = true;
        let _ = crate::daemon_handlers::handle_daemon_tick(
            state,
            layers::DaemonTickInput {
                agent_id: "daemon".into(),
                max_files: 32,
            },
        );
        state.daemon_state.tick_in_flight = false;
    }
}

fn daemon_wait_duration_ms(state: &SessionState) -> u64 {
    if !state.daemon_state.active {
        return 1000;
    }
    if state.daemon_state.poll_interval_ms == 0 {
        return 1000;
    }

    let exponent = state
        .daemon_state
        .idle_streak
        .min(state.daemon_state.max_backoff_multiplier.saturating_sub(1));
    let effective_poll_interval_ms = state
        .daemon_state
        .poll_interval_ms
        .saturating_mul(2u64.pow(exponent))
        .clamp(25, 10_000);
    let scheduler_interval_ms = if state.daemon_state.watch_backend == "native_fs" {
        effective_poll_interval_ms.max(5_000)
    } else {
        effective_poll_interval_ms
    };

    match state.daemon_state.last_tick_ms {
        Some(last_tick_ms) => {
            let elapsed = now_ms().saturating_sub(last_tick_ms);
            if elapsed >= scheduler_interval_ms {
                25
            } else {
                scheduler_interval_ms
                    .saturating_sub(elapsed)
                    .clamp(25, 1000)
            }
        }
        None => 25,
    }
}

impl LiveDaemonWatcher {
    fn start(
        watch_paths: &[String],
        event_tx: mpsc::SyncSender<ServerEvent>,
    ) -> Result<Self, String> {
        let dropped_counter = Arc::new(AtomicU64::new(0));
        let dropped_for_cb = dropped_counter.clone();
        let tx_for_cb = event_tx.clone();

        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                let event = match result {
                    Ok(_) => ServerEvent::WatchNotice,
                    Err(error) => ServerEvent::WatchError(error.to_string()),
                };
                match tx_for_cb.try_send(event) {
                    Ok(_) => {}
                    Err(mpsc::TrySendError::Full(_)) | Err(mpsc::TrySendError::Disconnected(_)) => {
                        dropped_for_cb.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
            .map_err(|error| error.to_string())?;

        for raw_path in watch_paths {
            let path = PathBuf::from(raw_path);
            let mode = if path.is_dir() {
                RecursiveMode::Recursive
            } else {
                RecursiveMode::NonRecursive
            };
            watcher
                .watch(path.as_path(), mode)
                .map_err(|error| error.to_string())?;
        }

        Ok(Self {
            _watcher: watcher,
            dropped_counter,
        })
    }
}

/// Dispatch perspective tools (12 tools).
fn dispatch_perspective_tool(
    state: &mut SessionState,
    tool_name: &str,
    params: &serde_json::Value,
) -> M1ndResult<serde_json::Value> {
    use crate::perspective_handlers;
    use crate::protocol::perspective::*;

    match tool_name {
        "perspective_start" => {
            let input: PerspectiveStartInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_start".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_start(state, input)
        }
        "perspective_routes" => {
            let input: PerspectiveRoutesInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_routes".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_routes(state, input)
        }
        "perspective_inspect" => {
            let input: PerspectiveInspectInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_inspect".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_inspect(state, input)
        }
        "perspective_peek" => {
            let input: PerspectivePeekInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_peek".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_peek(state, input)
        }
        "perspective_follow" => {
            let input: PerspectiveFollowInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_follow".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_follow(state, input)
        }
        "perspective_suggest" => {
            let input: PerspectiveSuggestInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_suggest".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_suggest(state, input)
        }
        "perspective_affinity" => {
            let input: PerspectiveAffinityInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_affinity".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_affinity(state, input)
        }
        "perspective_branch" => {
            let input: PerspectiveBranchInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_branch".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_branch(state, input)
        }
        "perspective_back" => {
            let input: PerspectiveBackInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_back".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_back(state, input)
        }
        "perspective_compare" => {
            let input: PerspectiveCompareInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_compare".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_compare(state, input)
        }
        "perspective_list" => {
            let input: PerspectiveListInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_list".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_list(state, input)
        }
        "perspective_close" => {
            let input: PerspectiveCloseInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "perspective_close".into(),
                    detail: e.to_string(),
                })?;
            perspective_handlers::handle_perspective_close(state, input)
        }
        _ => Err(M1ndError::UnknownTool {
            name: tool_name.to_string(),
        }),
    }
}

/// Dispatch lock tools (5 tools).
fn dispatch_lock_tool(
    state: &mut SessionState,
    tool_name: &str,
    params: &serde_json::Value,
) -> M1ndResult<serde_json::Value> {
    use crate::lock_handlers;
    use crate::protocol::lock::*;

    match tool_name {
        "lock_create" => {
            let input: LockCreateInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "lock_create".into(),
                    detail: e.to_string(),
                })?;
            lock_handlers::handle_lock_create(state, input)
        }
        "lock_watch" => {
            let input: LockWatchInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "lock_watch".into(),
                    detail: e.to_string(),
                })?;
            lock_handlers::handle_lock_watch(state, input)
        }
        "lock_diff" => {
            let input: LockDiffInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "lock_diff".into(),
                    detail: e.to_string(),
                })?;
            lock_handlers::handle_lock_diff(state, input)
        }
        "lock_rebase" => {
            let input: LockRebaseInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "lock_rebase".into(),
                    detail: e.to_string(),
                })?;
            lock_handlers::handle_lock_rebase(state, input)
        }
        "lock_release" => {
            let input: LockReleaseInput =
                serde_json::from_value(params.clone()).map_err(|e| M1ndError::InvalidParams {
                    tool: "lock_release".into(),
                    detail: e.to_string(),
                })?;
            lock_handlers::handle_lock_release(state, input)
        }
        _ => Err(M1ndError::UnknownTool {
            name: tool_name.to_string(),
        }),
    }
}

impl McpServer {
    fn sync_watcher_drop_counter(&mut self) {
        if let Some(runtime) = &self.daemon_runtime {
            if let Some(watcher) = &runtime.watcher {
                self.state.daemon_state.watch_events_dropped =
                    watcher.dropped_counter.load(Ordering::Relaxed);
            }
        }
    }

    fn refresh_daemon_watcher(&mut self) {
        let Some(runtime) = &mut self.daemon_runtime else {
            return;
        };

        runtime.watcher = None;
        if !self.state.daemon_state.active {
            self.state.daemon_state.watch_backend = "polling".into();
            self.state.daemon_state.watch_backend_error = None;
            let _ = self.state.persist_daemon_state();
            return;
        }

        match LiveDaemonWatcher::start(
            &self.state.daemon_state.watch_paths,
            runtime.event_tx.clone(),
        ) {
            Ok(watcher) => {
                runtime.watcher = Some(watcher);
                self.state.daemon_state.watch_backend =
                    if self.state.daemon_state.git_root.is_some() {
                        "git_native_fs".into()
                    } else {
                        "native_fs".into()
                    };
                self.state.daemon_state.watch_backend_error = None;
            }
            Err(error) => {
                self.state.daemon_state.watch_backend = "polling".into();
                self.state.daemon_state.watch_backend_error = Some(error);
            }
        }
        let _ = self.state.persist_daemon_state();
    }

    /// Create server with config. Does not start serving yet.
    ///
    /// Startup sequence:
    /// 1. Try to load graph snapshot from disk
    /// 2. If loaded, finalize (PageRank + CSR) if needed
    /// 3. Build all engines from graph
    /// 4. Try to load plasticity state and import into graph
    /// 5. Fall back gracefully to empty graph on any failure
    pub fn new(config: McpConfig) -> M1ndResult<Self> {
        // Build domain config from config.domain
        let domain_config = match config.domain.as_deref() {
            Some("music") => DomainConfig::music(),
            Some("memory") => DomainConfig::memory(),
            Some("generic") => DomainConfig::generic(),
            Some("code") | None => DomainConfig::code(),
            Some(other) => {
                eprintln!("[m1nd] Unknown domain '{}', falling back to 'code'", other);
                DomainConfig::code()
            }
        };
        eprintln!("[m1nd] Domain: {}", domain_config.name);

        // Step 1: Try to load graph snapshot
        let (mut graph, graph_loaded) = if config.graph_source.exists() {
            match m1nd_core::snapshot::load_graph(&config.graph_source) {
                Ok(g) => {
                    eprintln!(
                        "[m1nd] Loaded graph snapshot: {} nodes, {} edges",
                        g.num_nodes(),
                        g.num_edges(),
                    );
                    (g, true)
                }
                Err(e) => {
                    eprintln!(
                        "[m1nd] Failed to load graph snapshot ({}), starting fresh",
                        e,
                    );
                    (m1nd_core::graph::Graph::new(), false)
                }
            }
        } else {
            eprintln!("[m1nd] No graph snapshot found, starting fresh");
            (m1nd_core::graph::Graph::new(), false)
        };

        // Step 2: Finalize loaded graph if needed
        if graph_loaded && !graph.finalized && graph.num_nodes() > 0 {
            if let Err(e) = graph.finalize() {
                eprintln!(
                    "[m1nd] Failed to finalize loaded graph ({}), starting fresh",
                    e,
                );
                graph = m1nd_core::graph::Graph::new();
            }
        }

        // Step 3: Build all engines (handled by SessionState::initialize)
        let mut state = SessionState::initialize(graph, &config, domain_config)?;

        // Step 4: Try to load plasticity state
        if graph_loaded && config.plasticity_state.exists() {
            match m1nd_core::snapshot::load_plasticity_state(&config.plasticity_state) {
                Ok(states) => {
                    let mut g = state.graph.write();
                    match state.plasticity.import_state(&mut g, &states) {
                        Ok(_) => {
                            eprintln!(
                                "[m1nd] Loaded plasticity state: {} synaptic records",
                                states.len(),
                            );
                        }
                        Err(e) => {
                            eprintln!(
                                "[m1nd] Failed to import plasticity state ({}), continuing without it",
                                e,
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[m1nd] Failed to load plasticity state ({}), continuing without it",
                        e,
                    );
                }
            }
        }

        // Step 5: Auto-load agent-authored memory.
        // On boot, ingest <runtime_root>/agent-memory/*.light.md (adapter=light,
        // mode=merge) so knowledge the agent authored via `memorize` in prior
        // sessions is loaded automatically. Gated by M1ND_AUTO_LOAD_AGENT_MEMORY
        // (default ON; "0"/"false" disables). The result is stashed on the
        // session and surfaced verbatim in session_handshake — never hidden.
        state.agent_memory_boot = crate::tools::reload_agent_memory(&mut state);

        Ok(Self {
            config,
            state,
            daemon_runtime: None,
        })
    }

    /// Consume the McpServer and return the SessionState.
    /// Used by the HTTP server to take ownership of the session state
    /// and wrap it in Arc<Mutex<>> for shared concurrent access.
    pub fn into_session_state(self) -> SessionState {
        self.state
    }

    pub fn instance_handle(&self) -> InstanceHandle {
        self.state.instance.clone()
    }

    /// Startup sequence (03-MCP Section 1.2):
    /// 1. Load graph snapshot       (done in new())
    /// 2. Load plasticity state     (done in new())
    /// 3. Compute PageRank          (done in new() via finalize)
    /// 4. Build CSR (finalize)      (done in new() via finalize)
    /// 5. Warm up engines           (engines built in new())
    /// 6. Register MCP tools (13 tools)
    /// 7. Ready for connections
    pub fn start(&mut self) -> M1ndResult<()> {
        eprintln!(
            "[m1nd-mcp] Server ready. {} nodes, {} edges",
            self.state.graph.read().num_nodes(),
            self.state.graph.read().num_edges(),
        );

        Ok(())
    }

    /// Main event loop: read JSON-RPC from stdin, dispatch, write response to stdout.
    /// Blocks until EOF or shutdown signal.
    pub fn serve(&mut self) -> M1ndResult<()> {
        let stdout = std::io::stdout();
        let mut writer = stdout.lock();
        let (tx, rx) = mpsc::sync_channel(1024);
        self.daemon_runtime = Some(DaemonRuntimeControl {
            event_tx: tx.clone(),
            watcher: None,
        });
        self.refresh_daemon_watcher();

        thread::spawn(move || {
            let stdin = std::io::stdin();
            let mut reader = stdin.lock();
            loop {
                let next = read_request_payload(&mut reader);
                match next {
                    Ok(Some(value)) => {
                        if tx.send(ServerEvent::Request(value.0, value.1)).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        let _ = tx.send(ServerEvent::StdinClosed);
                        break;
                    }
                    Err(_) => {
                        let _ = tx.send(ServerEvent::StdinClosed);
                        break;
                    }
                }
            }
        });

        let mut pending_request: Option<(String, TransportMode)> = None;
        loop {
            self.sync_watcher_drop_counter();

            let next_event = if let Some((payload, mode)) = pending_request.take() {
                Ok(ServerEvent::Request(payload, mode))
            } else {
                rx.recv_timeout(Duration::from_millis(daemon_wait_duration_ms(&self.state)))
            };

            let (payload, transport_mode) = match next_event {
                Ok(ServerEvent::Request(payload, mode)) => (payload, mode),
                Ok(ServerEvent::StdinClosed) => break,
                Ok(ServerEvent::WatchNotice) => {
                    let mut watch_events_seen = 1u64;
                    let coalesced_at_ms = now_ms();
                    self.state.daemon_state.last_watch_event_ms = Some(coalesced_at_ms);
                    loop {
                        match rx.recv_timeout(Duration::from_millis(
                            self.state.daemon_state.coalesce_window_ms.max(1),
                        )) {
                            Ok(ServerEvent::WatchNotice) => {
                                watch_events_seen = watch_events_seen.saturating_add(1);
                            }
                            Ok(ServerEvent::WatchError(error)) => {
                                self.state.daemon_state.watch_events_dropped = self
                                    .state
                                    .daemon_state
                                    .watch_events_dropped
                                    .saturating_add(1);
                                self.state.daemon_state.watch_backend_error = Some(error);
                            }
                            Ok(ServerEvent::Request(payload, mode)) => {
                                pending_request = Some((payload, mode));
                                break;
                            }
                            Ok(ServerEvent::StdinClosed) => {
                                pending_request = None;
                                break;
                            }
                            Err(mpsc::RecvTimeoutError::Timeout)
                            | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                    }
                    self.state.daemon_state.watch_events_seen = self
                        .state
                        .daemon_state
                        .watch_events_seen
                        .saturating_add(watch_events_seen);
                    self.state.daemon_state.last_coalesced_event_ms = Some(coalesced_at_ms);
                    self.state.daemon_state.coalesced_event_count = self
                        .state
                        .daemon_state
                        .coalesced_event_count
                        .saturating_add(watch_events_seen);
                    run_daemon_tick(&mut self.state, "watch_event");
                    continue;
                }
                Ok(ServerEvent::WatchError(error)) => {
                    self.state.daemon_state.watch_events_dropped = self
                        .state
                        .daemon_state
                        .watch_events_dropped
                        .saturating_add(1);
                    self.state.daemon_state.watch_backend_error = Some(error);
                    self.state.daemon_state.last_watch_event_ms = Some(now_ms());
                    run_daemon_tick(&mut self.state, "reconciliation");
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    let trigger = if self.state.daemon_state.watch_backend == "native_fs" {
                        "reconciliation"
                    } else {
                        "idle_timeout"
                    };
                    if !self.state.daemon_state.active
                        || self.state.daemon_state.poll_interval_ms == 0
                    {
                        continue;
                    }
                    let due = self.state.daemon_state.last_tick_ms.is_none_or(|last| {
                        now_ms().saturating_sub(last) >= daemon_wait_duration_ms(&self.state)
                    });
                    if due {
                        run_daemon_tick(&mut self.state, trigger);
                    }
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };
            let trimmed = payload.trim();
            if trimmed.is_empty() {
                continue;
            }

            // MCP notifications (no "id" field) must be silently ignored per spec.
            // Check for notification before attempting full request parse.
            if let Ok(raw) = serde_json::from_str::<serde_json::Value>(trimmed) {
                if raw.get("id").is_none() {
                    // This is a notification — no response required.
                    continue;
                }
            }

            // Parse JSON-RPC request
            let request: JsonRpcRequest = match serde_json::from_str(trimmed) {
                Ok(r) => r,
                Err(e) => {
                    let err_resp = JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        id: serde_json::Value::Null,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32700,
                            message: format!("Parse error: {}", e),
                            data: None,
                        }),
                    };
                    let _ = write_response(&mut writer, &err_resp, transport_mode);
                    continue;
                }
            };

            // Dispatch and get response
            let response = self.dispatch(&request);

            let resp = match response {
                Ok(r) => r,
                Err(e) => JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: request.id.clone(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32603,
                        message: format!("{}", e),
                        data: None,
                    }),
                },
            };

            if write_response(&mut writer, &resp, transport_mode).is_err() {
                break; // stdout closed
            }
        }

        Ok(())
    }

    /// Graceful shutdown: persist state, flush writes, close connections.
    pub fn shutdown(&mut self) -> M1ndResult<()> {
        eprintln!("[m1nd-mcp] Shutting down...");
        let _ = self.state.persist();
        let _ = self.state.instance.release();
        eprintln!("[m1nd-mcp] State persisted. Goodbye.");
        Ok(())
    }

    /// Dispatch a single JSON-RPC request to the appropriate tool handler.
    ///
    /// Thin wrapper over the transport-agnostic [`handle_mcp_method`] free fn.
    /// The only stdio-specific concern kept here is refreshing the live FS
    /// watcher after a successful `daemon_start`/`daemon_stop`, which requires
    /// `&mut self` (the watcher lives on `McpServer`, not `SessionState`).
    fn dispatch(&mut self, request: &JsonRpcRequest) -> M1ndResult<JsonRpcResponse> {
        let response = handle_mcp_method(&mut self.state, request);

        // stdio-only: if this was a successful daemon_start/daemon_stop tool call,
        // rebind the live FS watcher to match the new daemon state.
        if request.method == "tools/call" {
            let tool_name = request
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if matches!(tool_name, "daemon_start" | "daemon_stop") {
                // The MCP wrapper reports tool execution errors via isError content,
                // not JSON-RPC errors, so only refresh when the call did not error.
                let is_error = response
                    .result
                    .as_ref()
                    .and_then(|r| r.get("isError"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if !is_error {
                    self.refresh_daemon_watcher();
                }
            }
        }

        Ok(response)
    }

    /// Dispatch a tool call by name. Delegates to the free dispatch_tool() function.
    fn dispatch_tool_call(
        &mut self,
        tool_name: &str,
        params: &serde_json::Value,
    ) -> M1ndResult<serde_json::Value> {
        dispatch_tool(&mut self.state, tool_name, params)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        all_tool_schemas, background_tick_if_due, daemon_wait_duration_ms, run_daemon_tick,
        should_autotick_daemon, tool_schemas, tool_schemas_for_tier, DaemonRuntimeControl,
        McpServer, ESSENTIAL_TOOLS,
    };
    use crate::server::McpConfig;
    use crate::session::SessionState;
    use m1nd_core::domain::DomainConfig;
    use m1nd_core::graph::Graph;
    use std::sync::mpsc;

    fn build_state() -> (tempfile::TempDir, SessionState) {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..McpConfig::default()
        };
        let state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");
        (temp, state)
    }

    fn build_server() -> (tempfile::TempDir, McpServer) {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..McpConfig::default()
        };
        let server = McpServer::new(config).expect("server");
        (temp, server)
    }

    fn build_state_read_only() -> (tempfile::TempDir, SessionState) {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            read_only: true,
            ..McpConfig::default()
        };
        let state = SessionState::initialize(Graph::new(), &config, DomainConfig::code())
            .expect("init session");
        (temp, state)
    }

    #[test]
    fn read_only_deny_list_is_precise() {
        use super::read_only_denied;
        let empty = serde_json::json!({});
        // Mutating tools are denied (bare and prefixed).
        for t in [
            "ingest",
            "apply",
            "apply_batch",
            "edit_commit",
            "memorize",
            "learn",
            "daemon_start",
            "auto_ingest_start",
            "m1nd_apply",
            "m1nd.ingest",
        ] {
            assert!(read_only_denied(t, &empty), "{t} should be denied");
        }
        // Read-only / analysis tools are allowed.
        for t in [
            "seek",
            "search",
            "activate",
            "why",
            "impact",
            "audit",
            "surgical_context_v2",
            "session_handshake",
            "trust_selftest",
            "doctor",
            "health",
            "view",
            "scan",
            "trace",
            "edit_preview",
        ] {
            assert!(!read_only_denied(t, &empty), "{t} should be allowed");
        }
        // persist: status is allowed; save/checkpoint/load are denied.
        assert!(!read_only_denied(
            "persist",
            &serde_json::json!({"action": "status"})
        ));
        assert!(read_only_denied(
            "persist",
            &serde_json::json!({"action": "save"})
        ));
        assert!(read_only_denied(
            "persist",
            &serde_json::json!({"action": "load"})
        ));
        // persist with no action defaults to status (allowed).
        assert!(!read_only_denied("persist", &empty));
    }

    #[test]
    fn read_only_dispatch_refuses_mutation_but_allows_query() {
        let (_temp, mut state) = build_state_read_only();
        assert!(state.read_only);

        // A mutation tool is refused with the contract error message.
        let err = super::dispatch_tool(
            &mut state,
            "ingest",
            &serde_json::json!({"agent_id": "t", "path": "/tmp/x"}),
        )
        .expect_err("ingest must be refused in read-only");
        let msg = err.to_string();
        assert!(
            msg.contains("attached read-only") && msg.contains("ingest"),
            "unexpected error: {msg}"
        );

        // A read-only tool still works (health needs no graph).
        let ok = super::dispatch_tool(&mut state, "health", &serde_json::json!({"agent_id": "t"}));
        assert!(ok.is_ok(), "health should work read-only: {ok:?}");
    }

    #[test]
    fn read_only_persist_is_a_noop() {
        let (_temp, mut state) = build_state_read_only();
        // persist() must early-return Ok without creating the graph file.
        state.persist().expect("read-only persist returns Ok");
        assert!(
            !state.graph_path.exists(),
            "read-only persist must not write the graph snapshot"
        );
        // should_persist is always false even after many queries.
        state.queries_processed = state.auto_persist_interval as u64;
        assert!(!state.should_persist());
    }

    #[test]
    fn response_envelope_attaches_additively() {
        let (_temp, mut state) = build_state();
        // seek on an empty graph returns a results-shaped object; the envelope
        // must be attached without removing existing fields.
        let out = super::dispatch_tool(
            &mut state,
            "seek",
            &serde_json::json!({"agent_id": "t", "query": "anything"}),
        )
        .expect("seek ok");
        let obj = out.as_object().expect("object result");
        assert!(obj.contains_key("_m1nd"), "_m1nd envelope must be present");
        let meta = &obj["_m1nd"];
        assert!(meta.get("suggest_next").is_some(), "suggest_next present");
        assert!(meta.get("tokens_saved").is_some(), "tokens_saved present");
        // Additive: the original results field is still there.
        assert!(obj.contains_key("results"), "results field preserved");
    }

    #[test]
    fn server_instructions_document_the_memory_habit() {
        // Host-agnostic contract: every MCP host injects M1ND_INSTRUCTIONS, so the
        // memory-authoring habit must be documented here (not in a host-specific skill).
        let s = super::M1ND_INSTRUCTIONS;
        assert!(s.contains("memorize"), "instructions must mention memorize");
        assert!(
            s.contains("evidence_freshness"),
            "instructions must mention the staleness check"
        );
        assert!(
            s.contains("write_light_memory"),
            "instructions must mention the mission_close memory option"
        );
    }

    #[test]
    fn boot_auto_loads_agent_memory_and_reports_it() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        let mem_dir = runtime_dir.join("agent-memory");
        std::fs::create_dir_all(&mem_dir).expect("mem dir");
        // A prior session's authored memory.
        std::fs::write(
            mem_dir.join("prior.light.md"),
            "---\nProtocol: L1GHT/1.0\nNode: PriorKnowledge\n---\n\n## Recall\n\nThe [⍂ entity: PriorFinding] was learned last session.\n[𝔻 confidence: high]\n",
        )
        .expect("write light memory");

        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            ..McpConfig::default()
        };
        let server = McpServer::new(config).expect("server");

        let report = server
            .state
            .agent_memory_boot
            .as_ref()
            .expect("agent_memory_boot should be Some when the dir exists with files");
        assert_eq!(
            report["loaded"], true,
            "memory should auto-load: {:?}",
            report
        );
        assert_eq!(report["file_count"], 1);
        assert!(
            report["nodes_added"].as_u64().unwrap_or(0) >= 1,
            "expected nodes added from the .light.md, got {:?}",
            report["nodes_added"]
        );
        // The prior knowledge must now be in the live graph.
        assert!(
            server.state.graph.read().num_nodes() >= 1,
            "graph should contain the loaded memory nodes"
        );
    }

    #[test]
    fn tool_schemas_expose_new_audit_surface_and_retrobuilder_tools() {
        // Use all_tool_schemas() — the full registry regardless of tier — to verify
        // that all advanced tool handlers are registered in the binary.
        // The tier gate only affects tools/list advertisement, not handler existence.
        let schema = all_tool_schemas();
        let names: Vec<String> = schema["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .filter_map(|tool| tool.get("name").and_then(|value| value.as_str()))
            .map(|value| value.to_string())
            .collect();

        for expected in [
            "ghost_edges",
            "taint_trace",
            "twins",
            "refactor_plan",
            "runtime_overlay",
            "batch_view",
            "scan_all",
            "cross_verify",
            "coverage_session",
            "external_references",
            "federate_auto",
            "audit",
            "session_handshake",
            "trust_selftest",
            "recovery_playbook",
            "doctor",
            "daemon_start",
            "daemon_stop",
            "daemon_status",
            "daemon_tick",
            "alerts_list",
            "alerts_ack",
            "mission_start",
            "mission_event",
            "mission_next",
            "mission_verify",
            "mission_handoff",
            "mission_close",
        ] {
            assert!(
                names.iter().any(|name| name == expected),
                "all_tool_schemas should expose {expected} (handler registered in binary)"
            );
        }
    }

    #[test]
    fn autotick_skips_daemon_control_tools() {
        for skipped in [
            "daemon_start",
            "daemon_stop",
            "daemon_status",
            "daemon_tick",
            "alerts_list",
            "alerts_ack",
            "session_handshake",
            "trust_selftest",
            "recovery_playbook",
            "mission_start",
            "mission_event",
            "mission_next",
            "mission_verify",
            "mission_handoff",
            "mission_close",
        ] {
            assert!(
                !should_autotick_daemon(skipped),
                "autotick should skip {skipped}"
            );
        }
        assert!(should_autotick_daemon("search"));
        assert!(should_autotick_daemon("apply"));
    }

    #[test]
    fn mission_control_records_guardrails_and_proof_packet() {
        let (_temp, mut state) = build_state();

        let start = super::dispatch_tool(
            &mut state,
            "mission_start",
            &serde_json::json!({
                "agent_id": "jimi",
                "repo": "/tmp/project",
                "task": "audit auth/session boundary",
                "mode": "review",
                "budget": "short",
                "risk": "medium"
            }),
        )
        .expect("mission_start");
        assert_eq!(start["schema"], "m1nd-mission-start-v0");
        let mission_id = start["mission_id"]
            .as_str()
            .expect("mission id")
            .to_string();
        assert!(
            state
                .runtime_root
                .join("mission-control")
                .join(format!("{mission_id}.json"))
                .exists(),
            "mission_start should persist mission state under runtime_root"
        );

        let next = super::dispatch_tool(
            &mut state,
            "mission_next",
            &serde_json::json!({
                "agent_id": "jimi",
                "mission_id": mission_id,
                "last_event": {
                    "event": "graph_query",
                    "tool": "seek",
                    "outcome": "inconclusive"
                }
            }),
        )
        .expect("mission_next");
        assert_eq!(next["schema"], "m1nd-mission-next-v0");
        assert_eq!(next["move"]["type"], "read_file");
        assert!(next["do_not"]
            .as_array()
            .expect("do_not")
            .iter()
            .any(|value| value == "seek"));

        let event = super::dispatch_tool(
            &mut state,
            "mission_event",
            &serde_json::json!({
                "agent_id": "jimi",
                "mission_id": mission_id,
                "event": "file_read",
                "payload": {
                    "path": "src/auth.rs",
                    "lines": [42, 55]
                },
                "outcome": "read direct source",
                "agent_confidence": 0.82
            }),
        )
        .expect("mission_event");
        assert_eq!(event["schema"], "m1nd-mission-event-v1");
        assert_eq!(event["event"]["event"], "file_read");
        assert_eq!(event["event"]["payload"]["path"], "src/auth.rs");
        assert_eq!(event["event"]["outcome"], "read direct source");
        assert_eq!(event["event"]["evidence_class"], "direct");
        assert!(event["event_digest"]
            .as_str()
            .expect("event digest")
            .starts_with("hash64:"));

        let graph_only = super::dispatch_tool(
            &mut state,
            "mission_verify",
            &serde_json::json!({
                "agent_id": "jimi",
                "mission_id": mission_id,
                "claim": "logout clears session",
                "evidence_refs": ["seek:auth flow"]
            }),
        )
        .expect("mission_verify graph-only");
        assert_eq!(graph_only["verdict"], "insufficient_evidence");
        assert_eq!(graph_only["evidence_grade"], "graph_only");

        let direct = super::dispatch_tool(
            &mut state,
            "mission_verify",
            &serde_json::json!({
                "agent_id": "jimi",
                "mission_id": mission_id,
                "claim": "logout route clears the session cookie",
                "evidence_refs": ["file_read:src/auth.rs:42"]
            }),
        )
        .expect("mission_verify direct");
        assert_eq!(direct["verdict"], "verified_for_mission");
        assert_eq!(direct["evidence_grade"], "direct");

        let handoff = super::dispatch_tool(
            &mut state,
            "mission_handoff",
            &serde_json::json!({
                "agent_id": "jimi",
                "mission_id": mission_id,
                "summary": "handoff after direct source proof",
                "recipient_agent_id": "reviewer"
            }),
        )
        .expect("mission_handoff");
        assert_eq!(handoff["schema"], "m1nd-mission-handoff-v1");
        assert_eq!(handoff["verified_claims"].as_array().unwrap().len(), 1);
        assert!(handoff["files_read"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "src/auth.rs"));

        let close = super::dispatch_tool(
            &mut state,
            "mission_close",
            &serde_json::json!({
                "agent_id": "jimi",
                "mission_id": mission_id,
                "summary": "checked the auth/session boundary",
                "gaps": ["did not run browser smoke"]
            }),
        )
        .expect("mission_close");
        assert_eq!(close["schema"], "m1nd-mission-proof-packet-v1");
        assert_eq!(close["verified_claims"].as_array().unwrap().len(), 1);
        assert_eq!(close["rejected_claims"].as_array().unwrap().len(), 1);
        assert_eq!(close["handoff_count"], 1);
        assert_eq!(
            close["context_guard_at_start"]["schema"],
            "m1nd-mission-context-guard-v1"
        );
        assert!(close["event_digest"]
            .as_str()
            .expect("event digest")
            .starts_with("hash64:"));
        assert!(close["non_claims"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value
                .as_str()
                .unwrap_or("")
                .contains("does not prove graph contents")));
    }

    #[test]
    fn health_exposes_host_binding_contract() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "health",
            &serde_json::json!({
                "agent_id": "jimi"
            }),
        )
        .expect("health output");

        assert_eq!(
            output["tool_surface_contract"]["schema"],
            "m1nd-tool-surface-contract-v0"
        );
        assert!(
            output["tool_surface_contract"]["required_host_visible_tools"]
                .as_array()
                .expect("required tools")
                .iter()
                .any(|tool| tool.as_str() == Some("trust_selftest")),
            "health should tell partial hosts that trust_selftest is required"
        );
        assert_eq!(
            output["host_binding_alignment"]["schema"],
            "m1nd-host-binding-alignment-v0"
        );
        assert_eq!(
            output["binding_fingerprint"]["schema"],
            "m1nd-binding-fingerprint-v0"
        );
    }

    #[test]
    fn session_handshake_marks_empty_graph_as_needing_ingest() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "session_handshake",
            &serde_json::json!({
                "agent_id": "jimi"
            }),
        )
        .expect("session handshake output");

        assert_eq!(output["schema"], "m1nd-session-handshake-v0");
        assert_eq!(output["trust_mode"], "needs_ingest");
        assert_eq!(output["can_ingest"], true);
        assert_eq!(output["tool_surface"]["degraded_host_tool_surface"], false);
        assert_eq!(
            output["doctor_recovery"]["suggested_tool"],
            "recovery_playbook"
        );
    }

    #[test]
    fn session_handshake_includes_binding_fingerprint() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "session_handshake",
            &serde_json::json!({
                "agent_id": "jimi"
            }),
        )
        .expect("session handshake output");

        assert_eq!(
            output["binding_fingerprint"]["schema"],
            "m1nd-binding-fingerprint-v0"
        );
        assert!(
            output["binding_fingerprint"]["process_id"]
                .as_u64()
                .unwrap_or_default()
                > 0
        );
        assert_eq!(
            output["binding_fingerprint"]["graph_finalized"],
            output["health"]["graph_finalized"]
        );
    }

    #[test]
    fn session_handshake_flags_degraded_host_tool_surface() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "session_handshake",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool_count": 3,
                "available_tools": ["seek", "audit", "doctor"],
                "missing_tools": ["ingest"]
            }),
        )
        .expect("session handshake output");

        assert_eq!(output["schema"], "m1nd-session-handshake-v0");
        assert_eq!(output["trust_mode"], "degraded_host_tool_surface");
        assert_eq!(output["can_ingest"], false);
        assert_eq!(output["can_recover"], false);
        assert!(
            output["tool_surface"]["missing_required_tools"]
                .as_array()
                .expect("missing tools")
                .iter()
                .any(|tool| tool.as_str() == Some("ingest")),
            "handshake should preserve the missing ingest diagnosis"
        );
        assert_eq!(
            output["doctor_recovery"]["arguments"]["observed_tool"],
            "tools/list"
        );
    }

    #[test]
    fn session_handshake_does_not_invent_missing_tools_from_count_only() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "session_handshake",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool_count": 94
            }),
        )
        .expect("session handshake output");

        assert_eq!(output["schema"], "m1nd-session-handshake-v0");
        assert_eq!(output["trust_mode"], "needs_ingest");
        assert_eq!(output["tool_surface"]["tool_count"], 94);
        assert_eq!(output["tool_surface"]["degraded_host_tool_surface"], false);
        assert!(
            output["tool_surface"]["missing_required_tools"]
                .as_array()
                .expect("missing tools")
                .is_empty(),
            "count-only evidence should not invent missing tool names"
        );
    }

    #[test]
    fn session_handshake_returns_full_trust_after_ingest() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        std::fs::write(
            repo.join("src/core.py"),
            "def session_handshake_target():\n    return 'trusted graph'\n",
        )
        .expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("ingest");

        let output = super::dispatch_tool(
            &mut state,
            "session_handshake",
            &serde_json::json!({
                "agent_id": "jimi",
                "available_tools": ["health", "trust_selftest", "recovery_playbook", "doctor", "ingest", "seek", "help", "session_handshake"]
            }),
        )
        .expect("session handshake output");

        assert_eq!(output["schema"], "m1nd-session-handshake-v0");
        assert_eq!(output["trust_mode"], "full_trust");
        assert_eq!(output["doctor_recovery"], serde_json::Value::Null);
        assert_eq!(
            output["tool_surface"]["required_tools_present"]["trust_selftest"],
            true
        );
        assert!(
            output["health"]["node_count"].as_u64().unwrap_or_default() > 0,
            "handshake should report the populated graph"
        );
    }

    #[test]
    fn trust_selftest_empty_graph_returns_needs_ingest_with_playbook() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "trust_selftest",
            &serde_json::json!({
                "agent_id": "jimi"
            }),
        )
        .expect("trust selftest output");

        assert_eq!(output["schema"], "m1nd-trust-selftest-v0");
        assert_eq!(output["ok"], false);
        assert_eq!(output["status"], "blocked");
        assert_eq!(output["verdict"], "needs_ingest");
        assert_eq!(output["checks"]["graph_populated"], false);
        assert_eq!(
            output["recovery_playbook"]["schema"],
            "m1nd-recovery-playbook-v0"
        );
        assert_eq!(
            output["session_handshake"]["schema"],
            "m1nd-session-handshake-v0"
        );
    }

    #[test]
    fn trust_selftest_prioritizes_wrong_workspace_over_empty_graph() {
        let (temp, mut state) = build_state();
        let active_repo = temp.path().join("active-repo");
        let other_repo = temp.path().join("other-repo");
        std::fs::create_dir_all(active_repo.join("src")).expect("active src");
        std::fs::create_dir_all(other_repo.join("src")).expect("other src");
        state.workspace_root = Some(active_repo.to_string_lossy().to_string());

        let output = super::dispatch_tool(
            &mut state,
            "trust_selftest",
            &serde_json::json!({
                "agent_id": "jimi",
                "scope": other_repo.join("src").to_string_lossy(),
            }),
        )
        .expect("trust selftest output");

        assert_eq!(output["schema"], "m1nd-trust-selftest-v0");
        assert_eq!(output["ok"], false);
        assert_eq!(output["status"], "blocked");
        assert_eq!(output["verdict"], "wrong_workspace_binding");
        assert_eq!(output["checks"]["needs_ingest"], false);
        assert_eq!(output["checks"]["wrong_workspace_binding"], true);
        assert_eq!(
            output["session_handshake"]["trust_mode"],
            "wrong_workspace_binding"
        );
        assert_eq!(
            output["recovery_playbook"]["trust_mode"],
            "wrong_workspace_binding"
        );
        assert_eq!(output["next_action"], "select_or_bind_workspace");
    }

    #[test]
    fn trust_selftest_flags_degraded_host_tool_surface() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "trust_selftest",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool_count": 3,
                "available_tools": ["seek", "audit", "doctor"],
                "missing_tools": ["ingest", "trust_selftest"]
            }),
        )
        .expect("trust selftest output");

        assert_eq!(output["schema"], "m1nd-trust-selftest-v0");
        assert_eq!(output["ok"], false);
        assert_eq!(output["status"], "warn");
        assert_eq!(output["verdict"], "degraded_host_tool_surface");
        assert_eq!(output["checks"]["host_surface_complete"], false);
        assert!(
            output["session_handshake"]["tool_surface"]["missing_required_tools"]
                .as_array()
                .expect("missing tools")
                .iter()
                .any(|tool| tool.as_str() == Some("trust_selftest")),
            "selftest should preserve missing trust_selftest evidence"
        );
    }

    #[test]
    fn trust_selftest_returns_full_trust_after_ingest() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        std::fs::write(
            repo.join("src/core.py"),
            "def trust_selftest_target():\n    return 'trusted graph'\n",
        )
        .expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("ingest");

        let output = super::dispatch_tool(
            &mut state,
            "trust_selftest",
            &serde_json::json!({
                "agent_id": "jimi",
                "available_tools": ["health", "trust_selftest", "recovery_playbook", "doctor", "ingest", "seek", "help", "session_handshake"]
            }),
        )
        .expect("trust selftest output");

        assert_eq!(output["schema"], "m1nd-trust-selftest-v0");
        assert_eq!(output["ok"], true);
        assert_eq!(output["status"], "ok");
        assert_eq!(output["verdict"], "full_trust");
        assert_eq!(output["next_action"], "proceed_with_m1nd_first");
        assert_eq!(output["checks"]["recovery_playbook_attached"], false);
        assert_eq!(output["recovery_playbook"], serde_json::Value::Null);
    }

    #[test]
    fn trust_selftest_flags_stale_binding_from_suspicious_retrieval() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        std::fs::write(
            repo.join("src/core.py"),
            "def trust_selftest_stale_binding_target():\n    return 'split brain?'\n",
        )
        .expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("ingest");

        let output = super::dispatch_tool(
            &mut state,
            "trust_selftest",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool": "seek",
                "observed_proof_state": "blocked",
                "observed_candidates": 0
            }),
        )
        .expect("trust selftest output");

        assert_eq!(output["schema"], "m1nd-trust-selftest-v0");
        assert_eq!(output["ok"], false);
        assert_eq!(output["status"], "warn");
        assert_eq!(output["verdict"], "stale_binding_suspected");
        assert_eq!(output["checks"]["graph_populated"], true);
        assert_eq!(output["checks"]["suspicious_retrieval_evidence"], true);
        assert_eq!(
            output["recovery_playbook"]["trust_mode"],
            "stale_binding_suspected"
        );
    }

    #[test]
    fn doctor_blocks_empty_graph_with_recovery_guidance() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "doctor",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool": "seek",
                "observed_proof_state": "blocked",
                "observed_candidates": 0
            }),
        )
        .expect("doctor output");

        assert_eq!(output["schema"], "m1nd-doctor-v0");
        assert_eq!(output["status"], "blocked");
        assert_eq!(output["diagnostics"]["graph_has_nodes"], false);
        assert_eq!(output["diagnostics"]["stale_binding_suspected"], false);
        assert!(
            output["next_actions"]
                .as_array()
                .expect("next actions")
                .iter()
                .any(|action| action.as_str().unwrap_or_default().contains("ingest")),
            "doctor should tell the agent how to recover an empty graph"
        );
    }

    #[test]
    fn doctor_flags_stale_binding_when_retrieval_blocks_on_populated_graph() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        std::fs::write(
            repo.join("src/core.py"),
            "def schema_registry():\n    return 'm1nd doctor'\n",
        )
        .expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("ingest");
        state.track_agent("jimi");

        let output = super::dispatch_tool(
            &mut state,
            "doctor",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool": "seek",
                "observed_proof_state": "blocked",
                "observed_candidates": 0
            }),
        )
        .expect("doctor output");

        assert_eq!(output["schema"], "m1nd-doctor-v0");
        assert_eq!(output["status"], "warn");
        assert_eq!(output["diagnostics"]["graph_has_nodes"], true);
        assert_eq!(output["diagnostics"]["stale_binding_suspected"], true);
        assert_eq!(output["diagnostics"]["agent_session_known"], true);
        assert!(
            output["transport_clues"]["split_brain_rule"]
                .as_str()
                .unwrap_or_default()
                .contains("host binding"),
            "doctor should name the host binding split-brain risk"
        );
    }

    #[test]
    fn doctor_flags_degraded_host_tool_surface_when_required_tools_are_missing() {
        let (_temp, mut state) = build_state();
        state.track_agent("jimi");

        let output = super::dispatch_tool(
            &mut state,
            "doctor",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool": "tools/list",
                "observed_proof_state": "blocked",
                "observed_tool_count": 3,
                "available_tools": ["seek", "audit", "doctor"],
                "missing_tools": ["ingest"]
            }),
        )
        .expect("doctor output");

        assert_eq!(output["schema"], "m1nd-doctor-v0");
        assert_eq!(output["diagnostics"]["degraded_host_tool_surface"], true);
        assert!(
            output["tool_surface"]["missing_tools"]
                .as_array()
                .expect("missing tools")
                .iter()
                .any(|tool| tool.as_str() == Some("ingest")),
            "doctor should name ingest as a missing recovery tool"
        );
        assert!(
            output["next_actions"]
                .as_array()
                .expect("next actions")
                .iter()
                .any(|action| action
                    .as_str()
                    .unwrap_or_default()
                    .contains("direct repo reads")),
            "doctor should tell the agent to fall back to file truth when ingest is unavailable"
        );
    }

    #[test]
    fn recovery_playbook_tool_schema_is_exposed() {
        let schema = tool_schemas();
        let names: Vec<String> = schema["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .filter_map(|tool| tool.get("name").and_then(|value| value.as_str()))
            .map(|value| value.to_string())
            .collect();

        assert!(
            names.iter().any(|name| name == "recovery_playbook"),
            "tool_schemas should expose recovery_playbook"
        );
    }

    #[test]
    fn recovery_playbook_empty_graph_returns_needs_ingest() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "recovery_playbook",
            &serde_json::json!({
                "agent_id": "jimi"
            }),
        )
        .expect("recovery playbook output");

        assert_eq!(output["schema"], "m1nd-recovery-playbook-v0");
        assert_eq!(output["status"], "blocked");
        assert_eq!(output["trust_mode"], "needs_ingest");
        assert!(
            output["steps"]
                .as_array()
                .expect("steps")
                .iter()
                .any(|step| step["id"] == "call_ingest" && step["tool"] == "ingest"),
            "recovery playbook should include an ingest step for an empty graph"
        );
    }

    #[test]
    fn recovery_playbook_flags_degraded_host_tool_surface() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "recovery_playbook",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool_count": 3,
                "available_tools": ["seek", "audit", "doctor"],
                "missing_tools": ["ingest"]
            }),
        )
        .expect("recovery playbook output");

        assert_eq!(output["trust_mode"], "degraded_host_tool_surface");
        assert_eq!(output["status"], "warn");
        assert!(
            output["tool_surface"]["missing_required_tools"]
                .as_array()
                .expect("missing tools")
                .iter()
                .any(|tool| tool.as_str() == Some("ingest")),
            "recovery playbook should preserve the missing ingest diagnosis"
        );
        assert!(
            output["steps"]
                .as_array()
                .expect("steps")
                .iter()
                .any(|step| step["id"] == "call_doctor" && step["tool"] == "doctor"),
            "recovery playbook should include doctor guidance when doctor is available"
        );
    }

    #[test]
    fn recovery_playbook_flags_stale_binding_on_populated_graph() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        std::fs::write(
            repo.join("src/core.py"),
            "def recovery_playbook_target():\n    return 'split brain?'\n",
        )
        .expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("ingest");
        state.track_agent("jimi");

        let output = super::dispatch_tool(
            &mut state,
            "recovery_playbook",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool": "seek",
                "observed_proof_state": "blocked",
                "observed_candidates": 0
            }),
        )
        .expect("recovery playbook output");

        assert_eq!(output["trust_mode"], "stale_binding_suspected");
        assert_eq!(output["status"], "warn");
        assert!(
            output["steps"]
                .as_array()
                .expect("steps")
                .iter()
                .any(|step| step["id"] == "call_doctor" && step["tool"] == "doctor"),
            "stale binding playbook should tell the agent to call doctor"
        );
        assert!(
            output["steps"]
                .as_array()
                .expect("steps")
                .iter()
                .any(|step| step["id"] == "compare_binding_fingerprint"),
            "stale binding playbook should compare binding fingerprints"
        );
        assert!(
            output["steps"]
                .as_array()
                .expect("steps")
                .iter()
                .any(|step| step["action"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("mcp_agent_smoke.py")),
            "stale binding playbook should include repo-local smoke commands"
        );
    }

    #[test]
    fn session_handshake_flags_wrong_workspace_binding_for_absolute_scope() {
        let (temp, mut state) = build_state();
        let active_repo = temp.path().join("active-repo");
        let other_repo = temp.path().join("other-repo");
        std::fs::create_dir_all(active_repo.join("src")).expect("active src");
        std::fs::create_dir_all(other_repo.join("src")).expect("other src");
        std::fs::write(active_repo.join("src/lib.rs"), "pub fn active() {}\n")
            .expect("active file");
        std::fs::write(other_repo.join("Cargo.toml"), "[package]\nname='other'\n")
            .expect("other manifest");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: active_repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("ingest active repo");

        let output = super::dispatch_tool(
            &mut state,
            "session_handshake",
            &serde_json::json!({
                "agent_id": "jimi",
                "scope": other_repo.join("src").to_string_lossy(),
            }),
        )
        .expect("session handshake output");

        assert_eq!(output["trust_mode"], "wrong_workspace_binding");
        assert_eq!(
            output["context_guard"]["workspace_binding_mismatch"]["code"],
            "wrong_workspace_binding"
        );
        assert_eq!(
            output["doctor_recovery"]["suggested_tool"],
            "recovery_playbook"
        );
    }

    #[test]
    fn recovery_playbook_count_only_host_evidence_does_not_invent_missing_tools() {
        let (_temp, mut state) = build_state();

        let output = super::dispatch_tool(
            &mut state,
            "recovery_playbook",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool_count": 94
            }),
        )
        .expect("recovery playbook output");

        assert_eq!(output["trust_mode"], "needs_ingest");
        assert_eq!(output["tool_surface"]["tool_count"], 94);
        assert!(
            output["tool_surface"]["missing_required_tools"]
                .as_array()
                .expect("missing tools")
                .is_empty(),
            "count-only evidence should not invent missing tool names"
        );
    }

    #[test]
    fn recovery_playbook_routes_wrong_workspace_binding_before_stale_binding() {
        let (temp, mut state) = build_state();
        let active_repo = temp.path().join("active-repo");
        let other_repo = temp.path().join("other-repo");
        std::fs::create_dir_all(active_repo.join("src")).expect("active src");
        std::fs::create_dir_all(other_repo.join("src")).expect("other src");
        std::fs::write(active_repo.join("src/lib.rs"), "pub fn active() {}\n")
            .expect("active file");
        std::fs::write(other_repo.join("package.json"), "{\"name\":\"other\"}\n")
            .expect("other package");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: active_repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("ingest active repo");

        let output = super::dispatch_tool(
            &mut state,
            "recovery_playbook",
            &serde_json::json!({
                "agent_id": "jimi",
                "observed_tool": "seek",
                "observed_proof_state": "blocked",
                "observed_candidates": 0,
                "scope": other_repo.join("src").to_string_lossy(),
            }),
        )
        .expect("recovery playbook output");

        assert_eq!(output["trust_mode"], "wrong_workspace_binding");
        assert_eq!(output["next_action"], "select_or_bind_workspace");
        assert_eq!(
            output["context_guard"]["workspace_binding_mismatch"]["requested_workspace_hint"],
            other_repo.to_string_lossy().as_ref()
        );
        assert!(output["steps"]
            .as_array()
            .expect("steps")
            .iter()
            .any(|step| step["id"] == "rebind_with_workspace_root"));
    }

    #[test]
    fn activate_blocked_response_points_to_recovery_playbook_with_graph_state() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        std::fs::write(repo.join("src/core.py"), "def core():\n    return 1\n")
            .expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("ingest");

        let output = super::dispatch_tool(
            &mut state,
            "activate",
            &serde_json::json!({
                "agent_id": "jimi",
                "query": "   ",
                "top_k": 5
            }),
        )
        .expect("activate output");

        assert_eq!(output["proof_state"], "blocked");
        assert_eq!(output["next_suggested_tool"], "recovery_playbook");
        assert!(output["graph_state"]["node_count"].as_u64().is_some());
        assert_eq!(
            output["recovery"]["suggested_tool"].as_str(),
            Some("recovery_playbook")
        );
        assert_eq!(
            output["recovery"]["arguments"]["observed_tool"].as_str(),
            Some("activate")
        );
    }

    #[test]
    fn activate_zero_results_without_blocked_proof_does_not_suggest_recovery() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        std::fs::write(repo.join("src/core.py"), "def core():\n    return 1\n")
            .expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "jimi".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("ingest");

        let output = super::dispatch_tool(
            &mut state,
            "activate",
            &serde_json::json!({
                "agent_id": "jimi",
                "query": "core",
                "top_k": 0
            }),
        )
        .expect("activate output");

        assert_eq!(output["proof_state"], "triaging");
        assert_eq!(output["activated"].as_array().expect("activated").len(), 0);
        assert_ne!(output["next_suggested_tool"], "recovery_playbook");
        assert_eq!(output["recovery"], serde_json::Value::Null);
        assert_eq!(output["agent_runtime_contract"]["trust_mode"], "full_trust");
    }

    #[test]
    fn background_tick_runs_when_daemon_is_due() {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        let file_path = repo.join("src/core.py");
        std::fs::write(&file_path, "def core():\n    return 1\n").expect("write file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "test".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("initial ingest");

        crate::daemon_handlers::handle_daemon_start(
            &mut state,
            crate::protocol::layers::DaemonStartInput {
                agent_id: "test".into(),
                watch_paths: vec![repo.to_string_lossy().to_string()],
                poll_interval_ms: 25,
            },
        )
        .expect("daemon start");

        std::fs::write(&file_path, "def core():\n    return 9\n").expect("rewrite file");
        state.daemon_state.last_tick_ms = Some(0);

        background_tick_if_due(&mut state);

        let hit = crate::search_handlers::handle_search(
            &mut state,
            crate::protocol::layers::SearchInput {
                query: "return 9".into(),
                agent_id: "test".into(),
                mode: crate::protocol::layers::SearchMode::Literal,
                scope: None,
                filename_pattern: None,
                top_k: 5,
                case_sensitive: false,
                context_lines: 0,
                invert: false,
                count_only: false,
                multiline: false,
                auto_ingest: false,
                max_output_chars: None,
                token_budget: None,
            },
        )
        .expect("search after background tick");

        assert!(
            hit.results
                .iter()
                .any(|result| { result.matched_line.contains("return 9") }),
            "background tick should refresh the graph before the next explicit tool call"
        );
    }

    #[test]
    fn daemon_wait_duration_uses_remaining_time_until_next_tick() {
        let (_temp, mut state) = build_state();
        state.daemon_state.active = true;
        state.daemon_state.poll_interval_ms = 500;
        state.daemon_state.last_tick_ms = Some(super::now_ms().saturating_sub(125));

        let wait_ms = daemon_wait_duration_ms(&state);
        assert!(
            (300..=400).contains(&wait_ms),
            "remaining wait should be close to the poll interval remainder"
        );

        state.daemon_state.last_tick_ms = Some(0);
        let overdue_wait_ms = daemon_wait_duration_ms(&state);
        assert_eq!(overdue_wait_ms, 25);
    }

    #[test]
    fn daemon_wait_duration_expands_with_idle_backoff() {
        let (_temp, mut state) = build_state();
        state.daemon_state.active = true;
        state.daemon_state.poll_interval_ms = 200;
        state.daemon_state.last_tick_ms = Some(super::now_ms());
        state.daemon_state.idle_streak = 2;
        state.daemon_state.max_backoff_multiplier = 8;

        let wait_ms = daemon_wait_duration_ms(&state);
        assert!(
            (700..=800).contains(&wait_ms),
            "idle streak should expand effective wait close to 4x the base interval"
        );
    }

    #[test]
    fn run_daemon_tick_marks_pending_rerun_when_already_in_flight() {
        let (_temp, mut state) = build_state();
        state.daemon_state.tick_in_flight = true;
        state.daemon_state.pending_rerun = false;

        run_daemon_tick(&mut state, "traffic");

        assert!(state.daemon_state.pending_rerun);
        assert!(state.daemon_state.tick_in_flight);
    }

    #[test]
    fn native_watcher_refresh_falls_back_to_polling_for_invalid_path() {
        let (_temp, mut server) = build_server();
        let (tx, _rx) = mpsc::sync_channel(8);
        server.daemon_runtime = Some(DaemonRuntimeControl {
            event_tx: tx,
            watcher: None,
        });
        server.state.daemon_state.active = true;
        server.state.daemon_state.watch_paths = vec!["/definitely/not/present".into()];

        server.refresh_daemon_watcher();

        assert_eq!(server.state.daemon_state.watch_backend, "polling");
        assert!(server.state.daemon_state.watch_backend_error.is_some());
    }

    #[test]
    fn native_watcher_refresh_uses_native_fs_for_real_root() {
        let (temp, mut server) = build_server();
        let watch_root = temp.path().join("watch-root");
        std::fs::create_dir_all(&watch_root).expect("watch-root");
        let (tx, _rx) = mpsc::sync_channel(8);
        server.daemon_runtime = Some(DaemonRuntimeControl {
            event_tx: tx,
            watcher: None,
        });
        server.state.daemon_state.active = true;
        server.state.daemon_state.watch_paths = vec![watch_root.to_string_lossy().to_string()];

        server.refresh_daemon_watcher();

        assert_eq!(server.state.daemon_state.watch_backend, "native_fs");
        assert!(server.state.daemon_state.watch_backend_error.is_none());
    }

    #[test]
    fn native_backend_uses_coarse_reconciliation_interval() {
        let (_temp, mut state) = build_state();
        state.daemon_state.active = true;
        state.daemon_state.poll_interval_ms = 200;
        state.daemon_state.watch_backend = "native_fs".into();
        state.daemon_state.last_tick_ms = Some(super::now_ms());

        let wait_ms = daemon_wait_duration_ms(&state);
        assert_eq!(wait_ms, 1000);
    }

    // -------------------------------------------------------------------------
    // Tier gate tests (Step 4)
    // These tests use tool_schemas_for_tier() directly to avoid env-var races
    // in parallel test execution. The runtime path (tool_schemas() reading
    // M1ND_TOOL_TIER) is also validated where safe to do so.
    // -------------------------------------------------------------------------

    /// The "essential" tier must advertise exactly the ESSENTIAL_TOOLS set:
    /// all required trust tools present, advanced tools absent.
    #[test]
    fn tier_gate_essential_advertises_only_essential_tools() {
        let schema = tool_schemas_for_tier("essential");
        let names: Vec<&str> = schema["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();

        // Size must equal ESSENTIAL_TOOLS
        assert_eq!(
            names.len(),
            ESSENTIAL_TOOLS.len(),
            "essential tier should advertise exactly {} tools, got {}: {:?}",
            ESSENTIAL_TOOLS.len(),
            names.len(),
            names
        );

        // All required trust tools must be present in the essential set
        for required in crate::tools::HOST_BINDING_REQUIRED_TOOLS {
            assert!(
                names.contains(&required),
                "essential tier must include required trust tool '{}'",
                required
            );
        }

        // Advanced tools that are NOT in ESSENTIAL_TOOLS must be absent
        assert!(
            !names.contains(&"resonate"),
            "advanced tool 'resonate' must be absent from essential tier (schema removed)"
        );
        assert!(
            !names.contains(&"ghost_edges"),
            "advanced tool 'ghost_edges' must be absent from essential tier"
        );
        assert!(
            !names.contains(&"twins"),
            "advanced tool 'twins' must be absent from essential tier"
        );
    }

    /// The "full" tier must advertise all registered tools (same as all_tool_schemas).
    #[test]
    fn tier_gate_full_advertises_all_tools() {
        let full_schema = all_tool_schemas();
        let gated_schema = tool_schemas_for_tier("full");

        let full_count = full_schema["tools"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);
        let gated_count = gated_schema["tools"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);

        assert_eq!(
            full_count, gated_count,
            "full tier must advertise all {} tools, got {}",
            full_count, gated_count
        );

        // Advanced tools must be present in the full tier
        let gated_names: Vec<&str> = gated_schema["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();
        // resonate schema was removed from the advertised surface (handler kept for back-compat)
        assert!(
            !gated_names.contains(&"resonate"),
            "resonate schema must be absent from full tier after surface removal"
        );
        assert!(
            gated_names.contains(&"ghost_edges"),
            "advanced tool 'ghost_edges' must be present in full tier"
        );
    }

    /// Explicit "essential" string matches what an empty/unset tier produces.
    #[test]
    fn tier_gate_essential_explicit_matches_unset() {
        // "essential" and "" (unset/default) must yield same count
        let essential_count = tool_schemas_for_tier("essential")["tools"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);
        let unset_count = tool_schemas_for_tier("")["tools"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);

        assert_eq!(
            essential_count, unset_count,
            "tier=essential must equal tier='' (unset)"
        );
        assert_eq!(
            essential_count,
            ESSENTIAL_TOOLS.len(),
            "essential count must match ESSENTIAL_TOOLS const"
        );
    }

    /// Unrecognized tier values must fall back to essential (not full).
    #[test]
    fn tier_gate_unrecognized_value_falls_back_to_essential() {
        let count = tool_schemas_for_tier("bogus_value_xyz")["tools"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0);

        assert_eq!(
            count,
            ESSENTIAL_TOOLS.len(),
            "unrecognized tier value must fall back to essential ({} tools), got {}",
            ESSENTIAL_TOOLS.len(),
            count
        );
    }

    /// all_tool_schemas() must always return the full registry regardless of tier,
    /// and the essential set must be a strict subset. This proves that hidden tools
    /// remain registered (their handlers exist) even when tier=essential.
    #[test]
    fn all_tool_schemas_always_contains_all_tools_regardless_of_tier() {
        let full = all_tool_schemas();
        let full_count = full["tools"].as_array().map(|a| a.len()).unwrap_or(0);
        let essential = tool_schemas_for_tier("essential");
        let essential_count = essential["tools"].as_array().map(|a| a.len()).unwrap_or(0);

        // Full registry is strictly larger than the essential set
        assert!(
            full_count > essential_count,
            "full registry ({}) must be larger than essential set ({})",
            full_count,
            essential_count
        );

        // A known advanced tool exists in the full registry (handler registered)
        let full_names: Vec<&str> = full["tools"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
            .collect();
        // resonate schema removed from advertised surface — handler remains but schema is gone
        assert!(
            !full_names.contains(&"resonate"),
            "'resonate' schema must be absent from all_tool_schemas after surface removal"
        );
        assert!(
            full_names.contains(&"ghost_edges"),
            "'ghost_edges' handler must remain registered even when tier=essential"
        );
        assert!(
            full_names.contains(&"daemon_start"),
            "'daemon_start' handler must remain registered even when tier=essential"
        );
    }

    /// resonate schema must be absent from ALL tiers and all_tool_schemas after surface removal.
    /// The dispatch handler is kept for back-compat but is not advertised.
    #[test]
    fn resonate_schema_absent_from_all_tiers_after_surface_removal() {
        let full = all_tool_schemas();
        let essential = tool_schemas_for_tier("essential");
        let full_gated = tool_schemas_for_tier("full");

        for (label, schema) in [
            ("all_tool_schemas", &full),
            ("essential tier", &essential),
            ("full tier", &full_gated),
        ] {
            let names: Vec<&str> = schema["tools"]
                .as_array()
                .expect("tools array")
                .iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
                .collect();
            assert!(
                !names.contains(&"resonate"),
                "'resonate' schema must be absent from {} (handler kept, schema removed)",
                label
            );
        }
    }

    // -----------------------------------------------------------------------
    // orient — agent-first cold-start aggregation tool
    // -----------------------------------------------------------------------

    /// Build a SessionState backed by a small populated, finalized graph so
    /// PageRank is computed and spread-activation has nodes to land on.
    fn build_state_populated(read_only: bool) -> (tempfile::TempDir, SessionState) {
        use m1nd_core::types::{EdgeDirection, FiniteF32, NodeType};
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            read_only,
            ..McpConfig::default()
        };

        let mut graph = Graph::new();
        // A tiny "lease" cluster so a task about leases activates something.
        let lease = graph
            .add_node(
                "file::src/lease.rs",
                "lease enforcement",
                NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("add lease node");
        let registry = graph
            .add_node(
                "file::src/registry.rs",
                "instance registry lease",
                NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("add registry node");
        let other = graph
            .add_node(
                "file::src/unrelated.rs",
                "unrelated helper",
                NodeType::File,
                &[],
                0.0,
                0.0,
            )
            .expect("add other node");
        graph
            .add_edge(
                lease,
                registry,
                "imports",
                FiniteF32::new(1.0),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.9),
            )
            .expect("edge lease->registry");
        graph
            .add_edge(
                registry,
                other,
                "imports",
                FiniteF32::new(1.0),
                EdgeDirection::Forward,
                false,
                FiniteF32::new(0.3),
            )
            .expect("edge registry->other");
        graph.finalize().expect("finalize graph");

        let state = SessionState::initialize(graph, &config, DomainConfig::code())
            .expect("init populated session");
        (temp, state)
    }

    #[test]
    fn orient_returns_focus_nodes_on_populated_graph() {
        let (_temp, mut state) = build_state_populated(false);
        let out = super::dispatch_tool(
            &mut state,
            "orient",
            &serde_json::json!({
                "agent_id": "orienter",
                "task": "lease enforcement in the instance registry",
            }),
        )
        .expect("orient should succeed on a populated graph");

        // Contract shape.
        assert_eq!(
            out["task"], "lease enforcement in the instance registry",
            "task must be echoed"
        );
        assert_eq!(out["proof_state"], "triaging");
        assert!(out["summary"].is_string(), "summary must be a string");

        let focus = out["focus_nodes"].as_array().expect("focus_nodes array");
        assert!(
            !focus.is_empty(),
            "focus_nodes must be non-empty on a populated graph"
        );
        // Each focus node carries the contract fields.
        for f in focus {
            assert!(f["node_id"].is_string(), "focus node needs node_id");
            assert!(f["label"].is_string(), "focus node needs label");
            assert!(f.get("pagerank").is_some(), "focus node needs pagerank");
            assert!(f.get("activation").is_some(), "focus node needs activation");
            assert!(f.get("kind").is_some(), "focus node needs kind");
            assert!(f.get("path").is_some(), "focus node needs path key");
        }

        // anchors are the global PageRank backbone (non-empty on a finalized graph).
        let anchors = out["anchors"].as_array().expect("anchors array");
        assert!(
            !anchors.is_empty(),
            "anchors must be non-empty once PageRank is computed"
        );
        for a in anchors {
            assert!(a["node_id"].is_string());
            assert!(a["pagerank"].is_number());
        }

        // The activation inside orient records a coverage session for this agent,
        // so coverage is populated with visited/total and a high-value shortlist.
        let cov = &out["coverage"];
        assert!(cov.is_object(), "coverage must be populated after activation");
        assert!(cov["visited"].is_number(), "coverage.visited present");
        assert_eq!(cov["total"], serde_json::json!(3), "graph has 3 nodes");
        assert!(
            cov["unvisited_high_value"].is_array(),
            "coverage.unvisited_high_value is an array"
        );

        // suggested_first_calls leads with surgical_context on the top focus node.
        let calls = out["suggested_first_calls"]
            .as_array()
            .expect("suggested_first_calls array");
        assert!(!calls.is_empty(), "must suggest at least one first call");
        assert_eq!(calls[0]["tool"], "surgical_context");
        assert!(calls[0]["arguments"]["node_id"].is_string());

        // The _m1nd envelope is attached by dispatch (additive).
        assert!(
            out.as_object().unwrap().contains_key("_m1nd"),
            "_m1nd envelope must wrap orient too"
        );
    }

    #[test]
    fn orient_works_in_read_only_mode() {
        let (_temp, mut state) = build_state_populated(true);
        assert!(state.read_only, "state must be read-only");

        // orient must NOT be caught by the mutation deny-list.
        use super::read_only_denied;
        assert!(
            !read_only_denied("orient", &serde_json::json!({})),
            "orient must be allowed in read-only mode"
        );

        // It dispatches successfully through the read-only path (query_readonly).
        let out = super::dispatch_tool(
            &mut state,
            "orient",
            &serde_json::json!({
                "agent_id": "ro-agent",
                "task": "lease enforcement",
            }),
        )
        .expect("orient must succeed in read-only attach");
        assert_eq!(out["task"], "lease enforcement");
        assert!(out["focus_nodes"].is_array());
        // read_only flag is surfaced via the envelope.
        assert_eq!(out["_m1nd"]["read_only"], serde_json::json!(true));

        // A read-only attach must never write the graph snapshot.
        assert!(
            !state.graph_path.exists(),
            "orient must not persist anything in read-only mode"
        );
    }

    /// REAL PROBE: load the repo's actual graph_snapshot.json (~5540 nodes) and
    /// run `orient` on a real task, printing the focus nodes + summary.
    ///
    /// Run with: `cargo test -p m1nd-mcp orient_real_snapshot_probe -- --nocapture`
    /// Skips gracefully (printing a note) if the snapshot is not present.
    #[test]
    fn orient_real_snapshot_probe() {
        // Locate the repo-root snapshot relative to the crate dir.
        let snapshot = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.join("graph_snapshot.json"))
            .filter(|p| p.exists());
        let Some(snapshot_path) = snapshot else {
            eprintln!("[orient_real_snapshot_probe] graph_snapshot.json not found — skipping");
            return;
        };

        let graph = m1nd_core::snapshot::load_graph(&snapshot_path)
            .expect("load real graph_snapshot.json");
        eprintln!(
            "[orient_real_snapshot_probe] loaded {} nodes from {}",
            graph.nodes.count,
            snapshot_path.display()
        );

        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            read_only: true, // attach-style: prove orient works without mutating
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(graph, &config, DomainConfig::code())
            .expect("init session from real snapshot");

        let out = super::dispatch_tool(
            &mut state,
            "orient",
            &serde_json::json!({
                "agent_id": "probe",
                "task": "read-only attach lease enforcement",
                "top_k": 8,
            }),
        )
        .expect("orient on real snapshot");

        eprintln!("\n=== orient(task=\"read-only attach lease enforcement\") on REAL graph ===");
        eprintln!("summary: {}", out["summary"].as_str().unwrap_or(""));
        eprintln!("focus_nodes:");
        for (i, f) in out["focus_nodes"]
            .as_array()
            .map(|a| a.as_slice())
            .unwrap_or(&[])
            .iter()
            .enumerate()
        {
            eprintln!(
                "  {:>2}. {:<55} act={:.4} pr={:.6} kind={} path={}",
                i + 1,
                f["label"].as_str().unwrap_or(""),
                f["activation"].as_f64().unwrap_or(0.0),
                f["pagerank"].as_f64().unwrap_or(0.0),
                f["kind"].as_str().unwrap_or(""),
                f["path"].as_str().unwrap_or("·"),
            );
        }
        eprintln!("anchors (global PageRank backbone):");
        for a in out["anchors"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
            eprintln!(
                "  - {:<55} pr={:.6}",
                a["label"].as_str().unwrap_or(""),
                a["pagerank"].as_f64().unwrap_or(0.0)
            );
        }
        eprintln!(
            "memory_nearby: {} | coverage: {}",
            out["memory_nearby"].as_array().map(|a| a.len()).unwrap_or(0),
            out["coverage"]
        );
        eprintln!("=== end probe ===\n");

        // Real data must produce a non-empty starting context.
        assert!(
            !out["focus_nodes"].as_array().unwrap().is_empty(),
            "orient must surface focus nodes on the real graph"
        );
    }

    /// REAL PROBE: load the repo's actual graph_snapshot.json (~5540 nodes) and
    /// run `seek` for a broad query twice — once unbudgeted, once with a tight
    /// `token_budget` — printing both result counts, the `budget` block, and the
    /// kept labels so the context-budget packing can be SEEN keeping the
    /// top-signal hits and dropping the rest on real data.
    ///
    /// Run with: `cargo test -p m1nd-mcp seek_token_budget_real_snapshot_probe -- --nocapture`
    /// Skips gracefully (printing a note) if the snapshot is not present.
    #[test]
    fn seek_token_budget_real_snapshot_probe() {
        let snapshot = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.join("graph_snapshot.json"))
            .filter(|p| p.exists());
        let Some(snapshot_path) = snapshot else {
            eprintln!(
                "[seek_token_budget_real_snapshot_probe] graph_snapshot.json not found — skipping"
            );
            return;
        };

        let graph = m1nd_core::snapshot::load_graph(&snapshot_path)
            .expect("load real graph_snapshot.json");
        let node_count = graph.nodes.count;

        let temp = tempfile::tempdir().expect("tempdir");
        let runtime_dir = temp.path().join("runtime");
        std::fs::create_dir_all(&runtime_dir).expect("runtime dir");
        let config = McpConfig {
            graph_source: runtime_dir.join("graph.json"),
            plasticity_state: runtime_dir.join("plasticity.json"),
            runtime_dir: Some(runtime_dir),
            read_only: true,
            ..McpConfig::default()
        };
        let mut state = SessionState::initialize(graph, &config, DomainConfig::code())
            .expect("init session from real snapshot");

        let query = "read only";
        let top_k = 40;

        // Baseline: no token_budget.
        let base = super::dispatch_tool(
            &mut state,
            "seek",
            &serde_json::json!({
                "agent_id": "probe",
                "query": query,
                "top_k": top_k,
            }),
        )
        .expect("baseline seek on real snapshot");
        let base_results = base["results"].as_array().cloned().unwrap_or_default();

        // Budgeted: tight ~300-token budget.
        let budget_tokens = 300u64;
        let budgeted = super::dispatch_tool(
            &mut state,
            "seek",
            &serde_json::json!({
                "agent_id": "probe",
                "query": query,
                "top_k": top_k,
                "token_budget": budget_tokens,
            }),
        )
        .expect("budgeted seek on real snapshot");
        let budgeted_results = budgeted["results"].as_array().cloned().unwrap_or_default();

        eprintln!(
            "\n=== seek(query=\"{}\") on REAL graph ({} nodes) ===",
            query, node_count
        );
        eprintln!("BASELINE (no token_budget): {} results", base_results.len());
        eprintln!(
            "  top labels: {:?}",
            base_results
                .iter()
                .take(8)
                .map(|r| r["label"].as_str().unwrap_or("?"))
                .collect::<Vec<_>>()
        );
        eprintln!(
            "BUDGETED (token_budget={}): {} results",
            budget_tokens,
            budgeted_results.len()
        );
        eprintln!("  budget block: {}", budgeted["budget"]);
        eprintln!("  kept labels (score / path):");
        for (i, r) in budgeted_results.iter().enumerate() {
            eprintln!(
                "    {:>2}. {:<48} score={:.4} path={}",
                i + 1,
                r["label"].as_str().unwrap_or("?"),
                r["score"].as_f64().unwrap_or(0.0),
                r["file_path"].as_str().unwrap_or("·"),
            );
        }
        eprintln!("=== end probe ===\n");

        // Baseline absent of a budget block; budgeted carries one.
        assert!(base.get("budget").is_none() || base["budget"].is_null());
        assert!(budgeted["budget"].is_object());

        // Packing must keep fewer than the (larger) baseline and keep the
        // top-ranked prefix.
        assert!(
            budgeted_results.len() <= base_results.len(),
            "budgeted seek must not return more than baseline"
        );
        if !base_results.is_empty() {
            assert!(!budgeted_results.is_empty(), "must keep at least one hit");
            assert_eq!(
                budgeted_results[0]["label"], base_results[0]["label"],
                "budgeted set must keep the same top-ranked hit"
            );
        }

        // budget accounting must be internally consistent.
        let b = &budgeted["budget"];
        let kept = b["kept"].as_u64().unwrap();
        let dropped = b["dropped"].as_u64().unwrap();
        assert_eq!(kept as usize, budgeted_results.len());
        assert_eq!(kept + dropped, base_results.len() as u64);
        assert_eq!(b["requested_tokens"].as_u64().unwrap(), budget_tokens);
    }

    // -----------------------------------------------------------------------
    // am_i_stale — agent-first on-disk staleness perception
    // -----------------------------------------------------------------------

    /// Ingest a temp repo with a single file and return (temp, state, abs_path).
    /// After ingest, `state.file_inventory` holds the recorded sha256 baseline.
    fn ingest_single_file(contents: &str) -> (tempfile::TempDir, SessionState, std::path::PathBuf) {
        let (temp, mut state) = build_state();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join("src")).expect("repo src");
        let file = repo.join("src/target.py");
        std::fs::write(&file, contents).expect("write target file");

        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: repo.to_string_lossy().to_string(),
                agent_id: "stale-agent".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("ingest single file");

        // The ingest pipeline canonicalizes paths (on macOS /var → /private/var),
        // so resolve the test's path the same way to address the file the tool
        // will actually see. Mirrors what a caller passing a real on-disk path
        // gets, and what `am_i_stale`'s own canonicalization-aware match handles.
        let file = std::fs::canonicalize(&file).unwrap_or(file);

        // Inventory must record the file with a hash baseline.
        assert!(
            state
                .file_inventory
                .values()
                .any(|e| e.file_path == file.to_string_lossy() && e.sha256.is_some()),
            "ingest must record the target file with a sha256 baseline"
        );
        (temp, state, file)
    }

    #[test]
    fn am_i_stale_detects_changed_file() {
        let (_temp, mut state, file) =
            ingest_single_file("def target():\n    return 'original'\n");

        // Rewrite the file on disk with different content (the change m1nd can't see).
        std::fs::write(&file, "def target():\n    return 'MUTATED'\n").expect("rewrite file");

        let out = super::handle_am_i_stale(
            &mut state,
            &serde_json::json!({
                "agent_id": "stale-agent",
                "files": [file.to_string_lossy()],
            }),
        )
        .expect("am_i_stale must succeed");

        assert_eq!(out["source"], "explicit_files");
        assert_eq!(out["checked"], serde_json::json!(1));
        let stale = out["stale"].as_array().expect("stale array");
        assert_eq!(stale.len(), 1, "the rewritten file must be flagged stale");
        assert_eq!(stale[0]["path"], file.to_string_lossy().as_ref());
        assert_eq!(stale[0]["reason"], "changed");
        assert!(
            out["fresh"].as_array().unwrap().is_empty(),
            "no file should be fresh after the rewrite"
        );
    }

    #[test]
    fn am_i_stale_detects_missing_file() {
        let (_temp, mut state, file) =
            ingest_single_file("def target():\n    return 'original'\n");

        // Delete the file on disk after ingest.
        std::fs::remove_file(&file).expect("delete file");

        let out = super::handle_am_i_stale(
            &mut state,
            &serde_json::json!({
                "agent_id": "stale-agent",
                "files": [file.to_string_lossy()],
            }),
        )
        .expect("am_i_stale must succeed");

        let stale = out["stale"].as_array().expect("stale array");
        assert_eq!(stale.len(), 1, "the deleted file must be flagged stale");
        assert_eq!(stale[0]["reason"], "missing");
    }

    #[test]
    fn am_i_stale_reports_fresh() {
        let (_temp, mut state, file) =
            ingest_single_file("def target():\n    return 'untouched'\n");

        // Do NOT touch the file — it must come back fresh.
        let out = super::handle_am_i_stale(
            &mut state,
            &serde_json::json!({
                "agent_id": "stale-agent",
                "files": [file.to_string_lossy()],
            }),
        )
        .expect("am_i_stale must succeed");

        assert_eq!(out["checked"], serde_json::json!(1));
        assert!(
            out["stale"].as_array().unwrap().is_empty(),
            "an untouched file must not be stale"
        );
        let fresh = out["fresh"].as_array().expect("fresh array");
        assert_eq!(fresh.len(), 1, "the untouched file must be fresh");
        assert_eq!(fresh[0], file.to_string_lossy().as_ref());
    }

    #[test]
    fn am_i_stale_defaults_to_coverage_session() {
        let (_temp, mut state, file) =
            ingest_single_file("def target():\n    return 'original'\n");

        // Record a coverage session for the agent that has visited the file,
        // then mutate the file so the default working set should flag it.
        state.note_coverage(
            "stale-agent",
            "view",
            [file.to_string_lossy().to_string()],
            std::iter::empty::<String>(),
        );
        std::fs::write(&file, "def target():\n    return 'changed-under-agent'\n")
            .expect("rewrite file");

        // No `files`/`nodes` → must default to the coverage session's visited files.
        let out = super::handle_am_i_stale(
            &mut state,
            &serde_json::json!({ "agent_id": "stale-agent" }),
        )
        .expect("am_i_stale must succeed");

        assert_eq!(
            out["source"], "coverage_session",
            "with no explicit targets it must default to the coverage session"
        );
        assert_eq!(out["checked"], serde_json::json!(1));
        let stale = out["stale"].as_array().expect("stale array");
        assert_eq!(stale.len(), 1, "the touched-then-changed file must be stale");
        assert_eq!(stale[0]["reason"], "changed");
    }

    #[test]
    fn am_i_stale_empty_when_no_targets_and_no_session() {
        let (_temp, mut state, _file) =
            ingest_single_file("def target():\n    return 'x'\n");

        // A different agent with no coverage session and no explicit targets.
        let out = super::handle_am_i_stale(
            &mut state,
            &serde_json::json!({ "agent_id": "ghost-agent" }),
        )
        .expect("am_i_stale must succeed");

        assert_eq!(out["source"], "empty");
        assert_eq!(out["checked"], serde_json::json!(0));
        assert!(out["notes"].is_array(), "empty result must carry a note");
    }

    #[test]
    fn am_i_stale_is_not_read_only_denied() {
        use super::read_only_denied;
        assert!(
            !read_only_denied("am_i_stale", &serde_json::json!({})),
            "am_i_stale only reads disk + inventory — it must be allowed in read-only attach"
        );
        // The prefix-normalized forms must also be allowed.
        assert!(!read_only_denied("m1nd_am_i_stale", &serde_json::json!({})));
        assert!(!read_only_denied("m1nd.am_i_stale", &serde_json::json!({})));
    }

    /// REAL PROBE: ingest a SMALL real directory (`m1nd-core/src`), record the
    /// inventory, mutate ONE real file's content on disk, call `am_i_stale`, and
    /// print the stale/fresh classification so we can SEE it catch the real
    /// change. Restores the file afterward so the working tree stays clean.
    ///
    /// Run with: `cargo test -p m1nd-mcp am_i_stale_real_probe -- --nocapture`
    #[test]
    fn am_i_stale_real_probe() {
        // Locate m1nd-core/src relative to the crate dir (repo-root/m1nd-core/src).
        let real_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.join("m1nd-core").join("src"))
            .filter(|p| p.is_dir());
        let Some(real_dir) = real_dir else {
            eprintln!("[am_i_stale_real_probe] m1nd-core/src not found — skipping");
            return;
        };

        let (_temp, mut state) = build_state();
        crate::tools::handle_ingest(
            &mut state,
            crate::protocol::IngestInput {
                path: real_dir.to_string_lossy().to_string(),
                agent_id: "real-probe".into(),
                mode: "replace".into(),
                incremental: false,
                adapter: "code".into(),
                namespace: None,
                include_dotfiles: false,
                dotfile_patterns: Vec::new(),
            },
        )
        .expect("ingest real m1nd-core/src");

        let inv_count = state.file_inventory.len();
        eprintln!(
            "\n=== am_i_stale REAL PROBE: ingested {} files from {} ===",
            inv_count,
            real_dir.display()
        );
        assert!(inv_count >= 3, "expected several real files in m1nd-core/src");

        // Pick three real ingested files: one to mutate, two left untouched.
        let mut paths: Vec<String> = state
            .file_inventory
            .values()
            .map(|e| e.file_path.clone())
            .collect();
        paths.sort();
        let victim = paths[0].clone();
        let untouched_a = paths.get(1).cloned().expect("a second real file");
        let untouched_b = paths.get(2).cloned().expect("a third real file");

        // Snapshot the victim's real bytes, mutate, and ALWAYS restore.
        let original_bytes = std::fs::read(&victim).expect("read victim file");
        let mut mutated = original_bytes.clone();
        mutated.extend_from_slice(b"\n// am_i_stale real probe touch\n");
        std::fs::write(&victim, &mutated).expect("mutate victim file");

        // Run the probe inside a closure so we can restore on any path.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let out = super::handle_am_i_stale(
                &mut state,
                &serde_json::json!({
                    "agent_id": "real-probe",
                    "files": [victim.clone(), untouched_a.clone(), untouched_b.clone()],
                }),
            )
            .expect("am_i_stale on real files");

            eprintln!("source : {}", out["source"]);
            eprintln!("checked: {}", out["checked"]);
            eprintln!("summary: {}", out["summary"].as_str().unwrap_or(""));
            eprintln!("STALE:");
            for s in out["stale"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
                eprintln!(
                    "  - {} [{}]",
                    s["path"].as_str().unwrap_or(""),
                    s["reason"].as_str().unwrap_or("")
                );
            }
            eprintln!("FRESH:");
            for f in out["fresh"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
                eprintln!("  - {}", f.as_str().unwrap_or(""));
            }
            eprintln!("=== end real probe ===\n");

            // The mutated file MUST be flagged changed; the others MUST be fresh.
            let stale_paths: Vec<String> = out["stale"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|s| s.get("path").and_then(|p| p.as_str()).map(String::from))
                .collect();
            assert!(
                stale_paths.contains(&victim),
                "the mutated real file must be flagged stale"
            );
            let fresh_paths: Vec<String> = out["fresh"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|f| f.as_str().map(String::from))
                .collect();
            assert!(
                fresh_paths.contains(&untouched_a) && fresh_paths.contains(&untouched_b),
                "the untouched real files must be fresh, not flagged"
            );
        }));

        // ALWAYS restore the real file so the working tree is clean.
        std::fs::write(&victim, &original_bytes).expect("restore victim file");
        let restored = std::fs::read(&victim).expect("re-read restored file");
        assert_eq!(restored, original_bytes, "victim file must be restored byte-for-byte");

        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }
}
