// === crates/m1nd-mcp/src/server.rs ===

use crate::layer_handlers;
use crate::personality;
use crate::protocol::layers;
use crate::protocol::*;
use crate::report_handlers;
use crate::search_handlers;
use crate::session::SessionState;
use crate::surgical_handlers;
use crate::tools;
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

**Session Start**: `health` → `drift` (recover what changed since last session) → \
`ingest` (if graph is empty or stale). This gives you codebase-aware context.

**Research**: `ingest` → `activate(query)` → `why(source, target)` → `missing(topic)` → \
`learn(feedback)`. Use `seek` for keyword search, `scan` for broad discovery, \
`trace` for dependency chains, `timeline` for temporal ordering.

**Code Change**: `impact(node)` (blast radius) → `predict(node)` (co-change likelihood) → \
`counterfactual(nodes)` (simulate removal) → `warmup(task_description)` (prime graph). \
Use `differential` to compare two subgraphs. Use `hypothesize` to test what-if scenarios.

**Deep Analysis**: `resonate(query)` for standing-wave harmonic patterns. \
`fingerprint(nodes)` for duplicate/equivalence detection. `diverge(node)` for \
exploring unexpected connections. `federate` to query across graph namespaces.

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
";

#[derive(Clone, Copy, Debug)]
enum TransportMode {
    Framed,
    Line,
}

fn read_request_payload<R: BufRead>(
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

fn write_response<W: Write>(
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
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            graph_source: PathBuf::from("./graph_snapshot.json"),
            plasticity_state: PathBuf::from("./plasticity_state.json"),
            runtime_dir: None,
            auto_persist_interval: 50,
            learning_rate: 0.08,
            decay_rate: 0.005,
            xlr_enabled: true,
            max_concurrent_reads: 32,
            write_queue_size: 64,
            domain: None,
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

/// List of all registered MCP tool schemas with full inputSchema per MCP spec.
/// Public so the HTTP server can cache and serve it.
pub fn tool_schemas() -> serde_json::Value {
    serde_json::json!({
        "tools": [
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
                        "include_structural_holes": { "type": "boolean", "default": false, "description": "Include structural hole detection" }
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
                "name": "resonate",
                "description": "Resonance analysis: harmonics, sympathetic pairs, and resonant frequencies",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query to find seed nodes for resonance analysis" },
                        "node_id": { "type": "string", "description": "Specific node identifier to use as seed (alternative to query)" },
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "top_k": { "type": "integer", "default": 20, "description": "Number of top resonance results to return" }
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
                        "graph_rerank": { "type": "boolean", "default": true, "description": "Whether to run graph re-ranking on embedding candidates" }
                    },
                    "required": ["query", "agent_id"]
                }
            },
            {
                "name": "scan",
                "description": "Pattern-aware structural code analysis with graph-validated findings",
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
                        "include_risk_score": { "type": "boolean", "default": true, "description": "Compute composite risk score" }
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
                "description": "Return full context for surgical LLM editing: file contents, symbols, and graph neighbourhood (callers, callees, tests). Use before m1nd.apply.",
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
                "description": "Write LLM-edited code back to a file and trigger incremental re-ingest so the graph stays coherent. Always call m1nd.surgical_context first.",
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
                "description": "Get full surgical context for a file PLUS source code of connected files (callers, callees, tests). Returns a complete workspace snapshot in one call. Superset of m1nd.surgical_context.",
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
                "description": "Atomically write multiple files and trigger a single bulk re-ingest. Use after m1nd.surgical_context_v2 when editing a file and its callers/tests together. All-or-nothing by default.",
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
                        "reingest": { "type": "boolean", "default": true, "description": "Re-ingest all modified files after writing" }
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
                        "max_output_chars": { "type": "integer", "description": "Optional cap for total returned characters across serialized matches" }
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
                "description": "Compare graph state against disk truth: missing files, LOC drift, and hash mismatches.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "scope": { "type": "string", "description": "File path prefix to limit scope" },
                        "check": { "type": "array", "items": { "type": "string" }, "default": [], "description": "Checks to run: existence, loc, hash" },
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
                "description": "Get help text for m1nd tools. Returns overview or detailed help for a specific tool with visual identity.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" },
                        "tool_name": { "type": "string", "description": "Specific tool name for detailed help (omit for overview)" }
                    },
                    "required": ["agent_id"]
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
                "name": "savings",
                "description": "Estimated token and cost savings from using m1nd. Shows current session and global totals with Gaia counter.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "agent_id": { "type": "string", "description": "Calling agent identifier" }
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
            }
        ]
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

    // Extract agent_id for tracking
    let agent_id = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
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

    let result = match normalized.as_str() {
        name if name.starts_with("perspective_") => dispatch_perspective_tool(state, name, params),
        name if name.starts_with("lock_") => dispatch_lock_tool(state, name, params),
        _ => dispatch_core_tool(state, &normalized, params),
    };

    // Post-dispatch: track savings + log query + add _m1nd metadata
    if let Ok(ref value) = result {
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        let result_count = value
            .get("results")
            .and_then(|v| v.as_array())
            .map_or(0, |a| a.len());

        // Track savings (skip meta tools)
        if !matches!(
            normalized.as_str(),
            "health" | "help" | "savings" | "report"
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

fn should_autotick_daemon(tool_name: &str) -> bool {
    !matches!(
        tool_name,
        "daemon_start"
            | "daemon_stop"
            | "daemon_status"
            | "daemon_tick"
            | "alerts_list"
            | "alerts_ack"
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
    state.daemon_state.last_tick_trigger = Some(trigger.to_string());
    let _ = crate::daemon_handlers::handle_daemon_tick(
        state,
        layers::DaemonTickInput {
            agent_id: "daemon".into(),
            max_files: 32,
        },
    );
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
                self.state.daemon_state.watch_backend = "native_fs".into();
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
                    self.state.daemon_state.last_watch_event_ms = Some(now_ms());
                    loop {
                        match rx.try_recv() {
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
                            Err(mpsc::TryRecvError::Empty)
                            | Err(mpsc::TryRecvError::Disconnected) => break,
                        }
                    }
                    self.state.daemon_state.watch_events_seen = self
                        .state
                        .daemon_state
                        .watch_events_seen
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
        eprintln!("[m1nd-mcp] State persisted. Goodbye.");
        Ok(())
    }

    /// Dispatch a single JSON-RPC request to the appropriate tool handler.
    fn dispatch(&mut self, request: &JsonRpcRequest) -> M1ndResult<JsonRpcResponse> {
        let method = request.method.as_str();

        // Handle MCP protocol methods
        match method {
            "initialize" => Ok(JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: request.id.clone(),
                result: Some(serde_json::json!({
                    "protocolVersion": "2024-11-05",
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
            }),
            "notifications/initialized" => {
                // No response needed for notifications, but we return one since caller expects it
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: request.id.clone(),
                    result: Some(serde_json::Value::Null),
                    error: None,
                })
            }
            "tools/list" => Ok(JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: request.id.clone(),
                result: Some(tool_schemas()),
                error: None,
            }),
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
                    self.state.track_agent(agent_id);
                    if self.state.daemon_state.active
                        && should_autotick_daemon(tool_name)
                        && self.state.daemon_state.last_tick_ms.is_some_and(|last| {
                            now_ms().saturating_sub(last)
                                >= self.state.daemon_state.poll_interval_ms
                        })
                    {
                        run_daemon_tick(&mut self.state, "traffic");
                    }
                }

                // MCP spec: tool execution errors -> isError content, not JSON-RPC errors
                match self.dispatch_tool_call(tool_name, &arguments) {
                    Ok(result) => {
                        if matches!(tool_name, "daemon_start" | "daemon_stop") {
                            self.refresh_daemon_watcher();
                        }
                        Ok(JsonRpcResponse {
                            jsonrpc: "2.0".into(),
                            id: request.id.clone(),
                            result: Some(serde_json::json!({
                                "content": [{
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&result).unwrap_or_default(),
                                }]
                            })),
                            error: None,
                        })
                    }
                    Err(e) => Ok(JsonRpcResponse {
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
                    }),
                }
            }
            _ => {
                // Method not found — JSON-RPC protocol error
                Ok(JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: request.id.clone(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32601,
                        message: format!("Method not found: {}", method),
                        data: None,
                    }),
                })
            }
        }
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
        background_tick_if_due, daemon_wait_duration_ms, should_autotick_daemon, tool_schemas,
        DaemonRuntimeControl, McpServer,
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

    #[test]
    fn tool_schemas_expose_new_audit_surface_and_retrobuilder_tools() {
        let schema = tool_schemas();
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
            "daemon_start",
            "daemon_stop",
            "daemon_status",
            "daemon_tick",
            "alerts_list",
            "alerts_ack",
        ] {
            assert!(
                names.iter().any(|name| name == expected),
                "tool_schemas should expose {expected}"
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
}
