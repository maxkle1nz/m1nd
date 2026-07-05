# MCP Server (m1nd-mcp)

m1nd-mcp is the transport and session layer. It exposes m1nd-core and m1nd-ingest through the live MCP tool surface over JSON-RPC stdio, manages the shared graph lifecycle, handles multi-agent sessions, and turns graph results into a more agent-operational runtime with proof-state, next-step guidance, recovery-oriented errors, observable batch execution, and a local-first document runtime. Use `tools/list` for the exact count in your current build.

Source: `m1nd-mcp/src/`

## Module Map

| Module | Purpose |
|--------|---------|
| `main.rs` | Binary entry point, config loading, tokio runtime, SIGINT handling |
| `server.rs` | `McpServer`, JSON-RPC transport (framed + line), tool schema registry |
| `session.rs` | `SessionState`, engine lifecycle, auto-persist, perspective/lock management |
| `tools.rs` | Tool handler implementations for the exported MCP surface |
| `auto_ingest.rs` | Document watcher runtime, persisted manifest, queue/tick orchestration |
| `universal_docs.rs` | Canonical document artifacts, provider health, resolve/bindings/drift surfaces |
| `engine_ops.rs` | Read-only engine wrappers for perspective synthesis |
| `protocol/auto_ingest.rs` | Request/response types for document and auto-ingest tools |
| `perspective/` | Perspective branching, lock state, watcher events |
| `layer_handlers.rs` | Layer-based tool dispatch |

## Transport Layer

### Dual Transport Mode

m1nd-mcp accepts two JSON-RPC transport formats on stdin, auto-detected per message:

**Framed mode** (HTTP-style headers):
```
Content-Length: 142\r\n
\r\n
{"jsonrpc":"2.0","method":"tools/call","params":{"name":"activate","arguments":{"query":"chat","agent_id":"agent-1"}},"id":1}
```

**Line mode** (raw JSON):
```
{"jsonrpc":"2.0","method":"tools/call","params":{"name":"activate","arguments":{"query":"chat","agent_id":"agent-1"}},"id":1}
```

Detection is based on the first non-whitespace byte: if it is `{` or `[`, the message is treated as line-mode JSON. Otherwise, it is parsed as framed with `Content-Length` headers. Responses are written in the same mode as the incoming request.

```rust
fn read_request_payload<R: BufRead>(
    reader: &mut R,
) -> std::io::Result<Option<(String, TransportMode)>> {
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() { return Ok(None); }
        let first_non_ws = buffer.iter().copied()
            .find(|byte| !byte.is_ascii_whitespace());
        let starts_framed = matches!(first_non_ws, Some(byte) if byte != b'{' && byte != b'[');
        // ...
    }
}
```

This dual-mode support allows m1nd to work with both Claude Code (which uses framed headers) and other MCP clients that send raw line JSON.

### Request/Response Format

Requests follow the JSON-RPC 2.0 specification with MCP conventions:

```json
{
    "jsonrpc": "2.0",
    "method": "tools/call",
    "params": {
        "name": "activate",
        "arguments": {
            "query": "chat handler",
            "agent_id": "agent-1",
            "top_k": 20,
            "dimensions": ["structural", "semantic", "temporal", "causal"],
            "xlr": true
        }
    },
    "id": 1
}
```

Responses:

```json
{
    "jsonrpc": "2.0",
    "result": {
        "content": [{
            "type": "text",
            "text": "..."
        }]
    },
    "id": 1
}
```

The `tools/list` method returns the live tool schemas with full `inputSchema` per MCP spec, enabling auto-discovery by any MCP client. Use it as the source of truth for exact names and count in your current build.

### HTTP Serve Surface & Live Events

Under the `serve` feature (`--serve`, default port `127.0.0.1:1337`), `m1nd-mcp` also exposes an HTTP/axum surface (`m1nd-mcp/src/http_server.rs`) that serves the embedded web UI and a REST/SSE API sharing the SAME `SessionState` as the stdio transport:

- `POST /api/tools/{tool}` — universal tool dispatch (same `dispatch_tool` free function the stdio transport uses). Accepts the optional `?brain=<project_root>` **per-brain selector** (see below).
- `GET /api/graph/snapshot` · `subgraph` · `stats` — graph read endpoints. Each accepts the optional `?brain=<project_root>` selector and echoes `served_brain` (see below).
- **Per-brain selector `?brain=<project_root>` (HUMAN-LAYER-PRD §4A.9, slice 2H).** Every `/api/graph/*` read and `POST /api/tools/*` call takes an optional, URL-encoded absolute `project_root`. Resolution reuses the wire's routing verbatim (`http_server::resolve_brain` → `project_brains.rs`, #260):
  - **absent** → the bound graph (today's behavior, byte-compatible — the serde-default posture on a URL);
  - **names the bound root** → the bound graph (canonical-path match, so the macOS `/var`→`/private/var` alias and a trailing slash resolve);
  - **names a known hosted brain** → that project brain, **warm-booting its dormant store on first touch**;
  - **unknown root** → an honest `404` tool_error naming the miss (`"no brain for <root> — the Hall lists what exists"`), NEVER a filesystem read, NEVER an auto-create (creation stays consented: `ingest {project_root}` or `m1nd init`). The surface stays loopback-only — the param adds routing, not exposure.

  Every `/api/graph/*` response carries a **`served_brain: {project_root, display_name}` echo** — the brain that actually answered — so a browser can ASSERT it against what it asked for and drop a mismatch (INV-15: never render one brain's nodes under another's chip). `GET /api/tools` carries a **`rest_brain_selector: true` capability stamp** so the UI feature-detects Open (never assumed, never version-sniffed).
- `GET /api/tools` — the tool schema list + the `rest_brain_selector` capability stamp (above).
- `GET /api/instances` — **every brain this owner holds**, PROJECT-named (the Hall's brains surface, HUMAN-LAYER-PRD §4A.3). Lists the bound dev graph AND every owner-hosted per-project brain (Two-Tier interim, `project_brains.rs`) — **warm brains from the in-memory map UNIONED with a cold DISK roster**: `ProjectBrainRegistry::disk_roster()` scans `<runtime_root>/project-brains/*/project_brain.json` manifests and surfaces every dormant brain with ZERO routed calls (display_name from the root basename, counts/freshness from the manifest — listing ≠ warm-boot; the warm map wins duplicates). This fixes the field bug where a hosted brain vanished from the Hall after an owner restart until a routed call warm-booted it. Each entry is a registry `InstanceRegistryEntry` enriched server-side (`http_server::instances_listing`) with two honest fields:
  - `display_name` — the **repo basename** ("m1nd", "project-b"), the brain's human name. NEVER the runtime dir ("claude") nor its `agent-memory` sidecar. Resolved from `SessionState::project_root_display` for the bound brain (its primary code ingest root, skipping `.light.md` memory files + the `agent-memory` dir) and from the store's `project_brain.json` manifest for a hosted `brain_kind:"project"` brain (whose raw `workspace_root` is the fingerprint store dir).
  - `project_root` — the repo the brain maps (the card's path).

  The bound brain floats first; the rest keep the registry's freshest-heartbeat order. The raw runtime fields (`workspace_root`, `runtime_root`, `pid`, `brain_kind`, …) stay on each entry, demoted to the receipt drawer. Each project entry also carries **`mailbox_open_count`** — the D3 face count (MEDULLA-PRD §9.2, slice M7b): the repo's box `wet_ink + in_flight`, or `null` when the box file does not exist (absent-honest, never a fabricated zero).
- **The Mailbox (MEDULLA-PRD §9.2, slice M7b).** Field-report letters are the ONE append-only spool (`~/.m1nd/field-reports.jsonl`); a local distribution files each into exactly one **box** — a project letter → its repo-side `<repo>/.m1nd/inbox.jsonl` (travels with git; a consent-deferred `.gitignore` keeps it uncommitted until `m1nd init`), a genuinely projectless letter → the medulla box (`<runtime_root>/inbox.jsonl`). A letter naming a project NEVER lands in the medulla (MED-INV-10); a letter whose repo dir is absent waits `pending_distribution`. Letter id = `sha256(raw line bytes)[0..12]` (stable, machine-independent → git-traveled boxes dedup). Fates are DERIVED from the reply graph (`answers[]`) at read time — `wet_ink`/`in_flight`/`fired_clay`/`external`; the "abertas" count = `wet_ink + in_flight` (`external` visible, never counted). Two REST doors + one CLI:
  - `GET /api/mailbox?brain=<project_root>` — ONE brain's box only (the §4A.9 `resolve_brain` selector reused verbatim: registered roots only, `served_brain` echo), with derived fates + `counts`. `?brain=medulla` reads the medulla box. Absent param → the bound brain's box.
  - `GET /api/inbox_sweep` — the cross-box triage view: the spool ∪ every known box, de-duplicated by content id (each letter once), with any unreachable box NAMED. Deliberately has NO `?brain=` selector (the team keeps seeing the conjunto).
  - `m1nd-mcp --inbox-sweep` — the CLI one-shot: distribute (idempotent, LOCAL, safe to re-run) then print the sweep as JSON. `--no-distribute` sweeps existing boxes read-only. Both `inbox_sweep` surfaces are CLI/REST-only, **OFF the MCP tool surface** (ORGANISM §C6.2 — their consumers are triage + the Hall, never the in-loop agent).
- `GET /api/instance/self` — the bound brain's own envelope (`SessionState::instance_self_summary`): `instance` + `graph_state` + session counters, now also carrying top-level `display_name` + `project_root` (the Brain Chip's name source, so the chip reads "m1nd", not the `agent-memory` sidecar that `graph_state.workspace_root` still carries).
- `POST /api/instances/{id}/save` · `/delete-state` — persist / clean a listed brain's runtime state (the delete is live-guarded and refuses a running instance).
- `GET /api/events` — a **Server-Sent Events** stream. Browsers (the served UI) subscribe here with `EventSource`.

**SSE event classes on `/api/events`.** Every broadcast event carries a named `event:` line. The classes are:

| Event | Meaning |
|---|---|
| `activation` | A spreading-activation run fired. |
| `learn` | A `learn` feedback landed. |
| `ingest` | An ingest completed (`nodes_added` / `edges_added`). |
| `persist` | The graph persisted (a `generation` bump). |
| `graph_changed` | **The shared graph actually mutated** — an agent ran `memorize` / `ingest` / `edit_commit` / `apply` / `learn`. Carries `{ event, agent_id?, source?, batch_id?, timestamp_ms?, brain_root? }`. The optional **`brain_root`** (§4A.9.6, additive — absent on a bound/legacy mutation and on a pre-2H owner) names WHICH brain mutated when the mutating call used `?brain=`; a viewer refetches when it names ITS brain OR carries no field (the honest over-refetch on old owners). |

`graph_changed` is the browser (pure-reader) counterpart of the MCP transport's `notifications/m1nd/graph_changed` notification (`m1nd-mcp/src/mcp_http.rs`). Both renderings share ONE mutation-detection predicate — `mcp_http::graph_mutation_event_name` — so a read-only tool result never masquerades as a graph change on either surface. On the browser side `http_server::browser_graph_changed_event` derives the `graph_changed` event from a mutation event and the SSE handler emits it alongside the raw event. The served Living Tree listens for `graph_changed` to refresh its snapshot in place (debounced ~500 ms; a low-frequency `/api/graph/stats` poll is the graceful fallback when SSE is unavailable).

A separate Streamable-HTTP MCP transport (`POST`/`GET`/`DELETE /mcp`, `mcp_http.rs`) serves attached agents (`--attach`); its `GET /mcp` stream carries the same mutation signal as `notifications/m1nd/graph_changed`.

## Server Lifecycle

```mermaid
sequenceDiagram
    participant Main as main.rs
    participant Server as McpServer
    participant Session as SessionState
    participant Tokio as tokio runtime

    Main->>Main: load_config()
    Main->>Server: McpServer::new(config)
    Server->>Session: SessionState::initialize(graph, config, domain)
    Session->>Session: Build all engines (orchestrator, temporal, counterfactual, topology, resonance, plasticity)
    Main->>Server: server.start()
    Main->>Tokio: spawn_blocking(server.serve())

    loop Until EOF or SIGINT
        Tokio->>Server: read JSON-RPC request
        Server->>Server: dispatch to tool handler
        Server->>Session: execute tool logic
        Session->>Session: auto-persist check
        Server->>Tokio: write JSON-RPC response
    end

    Tokio->>Server: server.shutdown()
    Server->>Session: final persist
```

### Configuration

Configuration is resolved in priority order: CLI argument (JSON file path) > environment variables > defaults.

```rust
pub struct McpConfig {
    pub graph_source: PathBuf,          // default: "./graph_snapshot.json"
    pub plasticity_state: PathBuf,      // default: "./plasticity_state.json"
    pub auto_persist_interval: u32,     // default: 50 (queries between persists)
    pub learning_rate: f32,             // default: 0.08
    pub decay_rate: f32,                // default: 0.005
    pub xlr_enabled: bool,             // default: true
    pub max_concurrent_reads: usize,    // default: 32
    pub write_queue_size: usize,        // default: 64
    pub domain: Option<String>,         // default: None ("code")
}
```

Environment variables: `M1ND_GRAPH_SOURCE`, `M1ND_PLASTICITY_STATE`, `M1ND_XLR_ENABLED`.

### Startup Sequence

1. `load_config()`: Resolve config from CLI args, env vars, or defaults.
2. `McpServer::new(config)`: Load graph snapshot from disk (or create empty graph). Load plasticity state. Initialize `SessionState` with all engines.
3. `server.start()`: Prepare the server for serving (no-op currently, reserved for future setup).
4. `tokio::task::spawn_blocking(server.serve())`: The serve loop does synchronous stdio I/O in a blocking task.
5. `tokio::select!` waits for either SIGINT (`ctrl_c()`) or serve loop completion.

### Shutdown

On SIGINT or stdin EOF:

1. `server.shutdown()`: Final persist of graph and plasticity state.
2. Process exits.

The atomic write pattern (temp file + rename) ensures that even if shutdown is interrupted, the previous snapshot remains intact.

## Tool Registration and Dispatch

### Schema Registry

`tool_schemas()` returns a JSON array of the live tool definitions with full `inputSchema` objects. Each tool specifies:

- `name`: Canonical live registry name (e.g., `activate`)
- `description`: Human-readable purpose
- `inputSchema`: JSON Schema with `properties`, `required`, `type`, defaults

Some clients may display transport-prefixed compatibility aliases, but the live registry returned by `tools/list` uses the bare tool names.

The current surface now includes a document runtime in addition to the code graph runtime:

- `document_resolve`
- `document_provider_health`
- `document_bindings`
- `document_drift`
- `auto_ingest_start`
- `auto_ingest_stop`
- `auto_ingest_status`
- `auto_ingest_tick`

Example schema entry:

```json
{
    "name": "activate",
    "description": "Spreading activation query across the graph",
    "inputSchema": {
        "type": "object",
        "properties": {
            "query": { "type": "string" },
            "agent_id": { "type": "string" },
            "top_k": { "type": "integer", "default": 20 },
            "dimensions": {
                "type": "array",
                "items": { "type": "string", "enum": ["structural", "semantic", "temporal", "causal"] },
                "default": ["structural", "semantic", "temporal", "causal"]
            },
            "xlr": { "type": "boolean", "default": true }
        },
        "required": ["query", "agent_id"]
    }
}
```

### Tool Categories

The live tool surface is organized into functional groups. Use `tools/list` for the exhaustive registry in your current build.

**Core Query Tools**:

| Tool | Purpose | Key Parameters |
|------|---------|----------------|
| `activate` | Spreading activation query | `query`, `top_k`, `dimensions`, `xlr` |
| `impact` | Blast radius analysis | `node_id`, `direction` (forward/reverse/both) |
| `missing` | Structural hole detection | `query`, `min_sibling_activation` |
| `why` | Path explanation between nodes | `source`, `target`, `max_hops` |
| `warmup` | Task-based priming | `task_description`, `boost_strength` |
| `counterfactual` | Node removal simulation | `node_ids`, `include_cascade` |
| `predict` | Co-change prediction | `changed_node`, `top_k`, `include_velocity` |
| `fingerprint` | Equivalence detection | `target_node` |
| `drift` | Weight changes since baseline | `since` |
| `learn` | Hebbian feedback | `feedback` (correct/wrong) |
| `resonate` | Standing wave analysis | `query`, `frequencies`, `num_harmonics` |
| `seek` | Seed-level node lookup | `query` |
| `scan` | Full graph summary | (none) |

**Graph Mutation Tools**:

| Tool | Purpose |
|------|---------|
| `ingest` | Ingest codebase into graph |
| `health` | Server diagnostics |
| `timeline` | Temporal event timeline |

**Perspective Tools**:

| Tool | Purpose |
|------|---------|
| `perspective_start` | Open a named perspective branch |
| `perspective_close` | Close a perspective |
| `perspective_list` | List open perspectives for an agent |
| `perspective_inspect` | View perspective state and cached results |
| `perspective_compare` | Diff two perspectives |
| `perspective_branch` | Fork a perspective |
| `perspective_suggest` | Generate suggestions from perspective context |
| `perspective_back` | Undo last perspective operation |
| `perspective_peek` | Read source file content from within perspective |
| `perspective_follow` | Follow links from perspective results |
| `perspective_routes` | View cached activation routes |
| `perspective_affinity` | Cross-perspective affinity analysis |

**Lock Tools**:

| Tool | Purpose |
|------|---------|
| `lock_create` | Create a baseline snapshot for change tracking |
| `lock_diff` | Diff current state against lock baseline |
| `lock_rebase` | Update lock baseline to current state |
| `lock_release` | Release a lock |
| `lock_watch` | Watch for changes against lock baseline |

**Trail Tools**:

| Tool | Purpose |
|------|---------|
| `trail_save` | Save current exploration trail |
| `trail_list` | List saved trails |
| `trail_resume` | Resume a saved trail |
| `trail_merge` | Merge trails |

**Topology Tools**:

| Tool | Purpose |
|------|---------|
| `federate` | Cross-graph federation |
| `diverge` | Divergence analysis between graph regions |
| `differential` | Differential activation (compare two queries) |
| `hypothesize` | Generate hypotheses from graph structure |
| `validate_plan` | Validate an implementation plan against graph |

### Dispatch

All tools require an `agent_id` parameter. The serve loop matches the tool name from the JSON-RPC request and dispatches to the corresponding handler in `tools.rs`. The handler extracts parameters from the `arguments` JSON object, acquires the appropriate lock on `SessionState`, executes the operation, and returns the result as a JSON-RPC response.

## Session Management

### SessionState

`SessionState` is the central state object. It owns the graph, all engines, and all session metadata:

```rust
pub struct SessionState {
    pub graph: SharedGraph,                    // Arc<RwLock<Graph>>
    pub domain: DomainConfig,
    pub orchestrator: QueryOrchestrator,       // activation + XLR + semantic
    pub temporal: TemporalEngine,
    pub counterfactual: CounterfactualEngine,
    pub topology: TopologyAnalyzer,
    pub resonance: ResonanceEngine,
    pub plasticity: PlasticityEngine,

    pub queries_processed: u64,
    pub auto_persist_interval: u32,            // default: 50
    pub start_time: Instant,
    pub last_persist_time: Option<Instant>,
    pub graph_path: PathBuf,
    pub plasticity_path: PathBuf,
    pub sessions: HashMap<String, AgentSession>,

    // Perspective/lock state
    pub graph_generation: u64,
    pub plasticity_generation: u64,
    pub cache_generation: u64,
    pub perspectives: HashMap<(String, String), PerspectiveState>,
    pub locks: HashMap<String, LockState>,
    pub perspective_counter: HashMap<String, u64>,
    pub lock_counter: HashMap<String, u64>,
    pub pending_watcher_events: Vec<WatcherEvent>,
    pub perspective_limits: PerspectiveLimits,
    pub peek_security: PeekSecurityConfig,
    pub ingest_roots: Vec<String>,
}
```

### SharedGraph

`SharedGraph = Arc<parking_lot::RwLock<Graph>>` provides concurrent access:

- **Reads** (activation, impact, predict, etc.): Acquire read lock. Multiple concurrent reads allowed.
- **Writes** (ingest, learn): Acquire write lock. Exclusive access. Blocks all readers.

`parking_lot::RwLock` is used instead of `std::sync::RwLock` for two reasons:
1. **Writer starvation prevention**: parking_lot uses a fair queue, so plasticity writes do not starve behind continuous read queries.
2. **Performance**: parking_lot's implementation is faster for the read-heavy, write-rare access pattern of m1nd.

### Engine Rebuild

After ingestion replaces the graph, all engines must be rebuilt because they hold indexes derived from the old graph:

```rust
pub fn rebuild_engines(&mut self) -> M1ndResult<()> {
    {
        let graph = self.graph.read();
        self.orchestrator = QueryOrchestrator::build(&graph)?;
        self.temporal = TemporalEngine::build(&graph)?;
        self.plasticity = PlasticityEngine::new(
            &graph, PlasticityConfig::default(),
        );
    }
    self.invalidate_all_perspectives();
    self.mark_all_lock_baselines_stale();
    self.graph_generation += 1;
    self.cache_generation = self.cache_generation.max(self.graph_generation);
    Ok(())
}
```

The rebuild also invalidates all perspective and lock state, bumping generation counters so that stale caches are detected.

### Auto-Persist

Every `auto_persist_interval` queries (default 50), the session persists state to disk:

1. **Graph first**: `save_graph()` writes the CSR graph to JSON via atomic temp-file-then-rename.
2. **Plasticity second**: `export_state()` extracts per-edge `SynapticState`, then `save_plasticity_state()` writes to JSON.
3. If graph save fails, plasticity save is skipped (prevents inconsistent state).
4. If plasticity save fails after graph succeeds, a warning is logged but the server continues.

```rust
pub fn persist(&mut self) -> M1ndResult<()> {
    let graph = self.graph.read();
    m1nd_core::snapshot::save_graph(&graph, &self.graph_path)?;
    match self.plasticity.export_state(&graph) {
        Ok(states) => {
            if let Err(e) = save_plasticity_state(&states, &self.plasticity_path) {
                eprintln!("[m1nd] WARNING: plasticity persist failed: {}", e);
            }
        }
        Err(e) => eprintln!("[m1nd] WARNING: plasticity export failed: {}", e),
    }
    self.last_persist_time = Some(Instant::now());
    Ok(())
}
```

## Multi-Agent Support

### Agent Sessions

Each unique `agent_id` gets an `AgentSession`:

```rust
pub struct AgentSession {
    pub agent_id: String,
    pub first_seen: Instant,
    pub last_seen: Instant,
    pub query_count: u64,
}
```

Sessions are created on first query and updated on each subsequent query. The `last_seen` timestamp enables timeout-based cleanup.

### Agent Isolation

All agents share one graph (writes are immediately visible), but isolation is provided through:

- **Perspectives**: Per-agent branching views with independent route caches. A perspective opened by Agent A is invisible to Agent B.
- **Locks**: Per-agent change tracking baselines. Each agent can create independent locks to track changes from their perspective.
- **Query Memory**: The plasticity engine's ring buffer is global (shared learning), but perspective-level route caches are per-agent.

### Generation Counters

Three generation counters detect stale state:

| Counter | Bumped By | Purpose |
|---------|-----------|---------|
| `graph_generation` | Ingest, rebuild_engines | Detects stale engine indexes |
| `plasticity_generation` | Learn | Detects stale plasticity state |
| `cache_generation` | max(graph_gen, plasticity_gen) | Unified staleness for perspective caches |

When a perspective's cached results were computed at a different generation than the current `cache_generation`, the perspective is marked stale and results are recomputed on next access.

## Perspective System

Perspectives are per-agent named branches that cache activation results and enable comparative analysis.

### Lifecycle

```mermaid
stateDiagram-v2
    [*] --> Open: perspective_start
    Open --> Active: first query (activate/impact/etc)
    Active --> Active: additional queries
    Active --> Branched: perspective_branch
    Active --> Stale: graph mutation (ingest/learn)
    Stale --> Active: re-query (auto-refresh)
    Active --> [*]: perspective_close
    Open --> [*]: perspective_close
```

### Perspective IDs

Generated as `persp_{agent_prefix}_{counter:03}`. Each agent has an independent monotonic counter. Example: Agent "agent-1" creates perspectives `persp_agent1_001`, `persp_agent1_002`, etc.

### Resource Limits

`PerspectiveLimits` caps resource usage:

- Maximum open perspectives per agent
- Maximum total perspectives across all agents
- Maximum locks per agent

### Peek Security

`perspective_peek` allows reading source file content from within a perspective context. Security restrictions:

- Files must be within an `ingest_roots` allow-list (populated during ingest).
- Path traversal (`..`) is blocked.
- Symlinks outside the allow-list are rejected.

## Lock System

Locks capture a baseline snapshot of graph state for change tracking.

### Lock Lifecycle

```mermaid
stateDiagram-v2
    [*] --> Created: lock_create
    Created --> Active: baseline captured
    Active --> Diffed: lock_diff (non-destructive)
    Diffed --> Active: continue tracking
    Active --> Rebased: lock_rebase (new baseline)
    Rebased --> Active: continue tracking
    Active --> Stale: graph mutation
    Stale --> Rebased: lock_rebase
    Active --> [*]: lock_release
```

### Operations

- **lock_create**: Captures current graph state (generation counter + weight snapshot for tracked edges).
- **lock_diff**: Compares current state against baseline. Reports weight changes, new/removed edges. Non-destructive.
- **lock_rebase**: Updates baseline to current state. Clears staleness flag.
- **lock_release**: Frees the lock.
- **lock_watch**: Returns pending watcher events (changes that occurred since last check).

When a graph mutation occurs (ingest, learn), all locks are marked `baseline_stale = true`. `lock_diff` reports this staleness and suggests `lock_rebase`.

## Engine Operations (Perspective Synthesis)

`engine_ops.rs` provides read-only wrappers around engine operations for use within perspective synthesis. These wrappers operate under a `SynthesisBudget`:

- **Max calls**: 8 engine calls per synthesis operation.
- **Wall-clock timeout**: 500ms total.

This prevents perspective synthesis from monopolizing the server. Each wrapper (`activate_readonly`, `impact_readonly`, etc.) checks budget before executing and returns a budget-exhausted error if limits are exceeded.

## Concurrency Model

```mermaid
flowchart TD
    STDIN["stdin (JSON-RPC)"]
    TOKIO["tokio::task::spawn_blocking"]
    SERVE["McpServer.serve()<br/>(synchronous loop)"]
    RW["SharedGraph<br/>Arc&lt;parking_lot::RwLock&lt;Graph&gt;&gt;"]
    READ["Read Lock<br/>(activate, impact, predict, ...)"]
    WRITE["Write Lock<br/>(ingest, graph swap)"]
    PERSIST["Auto-Persist<br/>(write lock on graph)"]

    STDIN --> TOKIO
    TOKIO --> SERVE
    SERVE --> READ
    SERVE --> WRITE
    SERVE --> PERSIST
    READ --> RW
    WRITE --> RW
    PERSIST --> RW
```

The serve loop is single-threaded (synchronous stdio I/O), but graph access is concurrent-safe through `SharedGraph`. Within a single request:

- Read operations acquire a read lock (shared, non-blocking with other reads).
- Write operations acquire a write lock (exclusive, blocks all other access).
- Plasticity weight updates use atomic CAS (no lock needed for individual weight writes).

The tokio runtime is used solely for the `select!` between SIGINT and serve loop completion. All actual tool processing happens synchronously within `spawn_blocking`.
