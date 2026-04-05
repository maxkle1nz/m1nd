# API Reference

All 78 MCP tools, grouped by category. Each tool is callable via JSON-RPC stdio as `m1nd_<tool_name>`.

All tools require an `agent_id` string parameter (use any stable identifier — your editor session ID, agent name, etc.).

Jump to:
- [Foundation](#foundation-13-tools)
- [Perspective Navigation](#perspective-navigation-12-tools)
- [Lock System](#lock-system-5-tools)
- [Superpowers](#superpowers-13-tools)
- [Superpowers Extended](#superpowers-extended-9-tools)
- [RETROBUILDER](#retrobuilder-5-tools)
- [Surgical](#surgical-4-tools)
- [Search & Efficiency](#v050--search--efficiency-5-tools)
- [Audit & Session Ergonomics](#audit--session-ergonomics-7-tools)

---

## RETROBUILDER (5 tools)

These tools are implemented in the live registry and exposed through `tool_schemas()`:

- `m1nd_ghost_edges` — temporal co-change ghost edges from git history
- `m1nd_taint_trace` — taint propagation over graph structure
- `m1nd_twins` — structural equivalence / near-equivalence discovery
- `m1nd_refactor_plan` — graph-native refactoring proposals
- `m1nd_runtime_overlay` — runtime heat and error overlays from OTel spans

## Audit & Session Ergonomics (7 tools)

These tools reduce orchestration overhead for real agent sessions:

- `m1nd_batch_view` — multi-file read surface with stable delimiters and summaries
- `m1nd_scan_all` — run all structural patterns in one call
- `m1nd_cross_verify` — graph vs disk verification (`existence`, `loc`, `hash`)
- `m1nd_coverage_session` — what this agent has visited so far
- `m1nd_external_references` — explicit paths outside ingest roots
- `m1nd_federate_auto` — turn external path or manifest/workspace evidence into repo candidates and optional federation
- `m1nd_audit` — profile-aware one-call audit

---

## Foundation (13 tools)

Core graph operations: ingest, query, learn, and navigate.

---

### `m1nd_ingest`

Parse a codebase, markdown docs, or JSON domain graph into the semantic graph.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `path` | string | yes | Filesystem path to ingest |
| `adapter` | string | no | `"code"` (default), `"memory"`, or `"json"` |
| `namespace` | string | no | Node ID namespace prefix (default: none, auto-set to `"memory"` for memory adapter) |
| `mode` | string | no | `"replace"` (default, clears graph) or `"merge"` (overlay) |
| `incremental` | bool | no | Only re-ingest changed files (default: false) |

**Output:**

```json
{
  "mode": "replace",
  "adapter": "code",
  "namespace": null,
  "files_scanned": 335,
  "files_parsed": 335,
  "nodes_created": 9767,
  "edges_created": 26557,
  "elapsed_ms": 910,
  "node_count": 9767,
  "edge_count": 26557
}
```

**Benchmark:** 910ms / 335 files (code) · 138ms / 82 docs (memory adapter)

**Example — ingest code:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_ingest","arguments":{
  "agent_id":"dev","path":"/your/project","incremental":false
}}}
```

**Example — ingest docs + merge with existing code graph:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_ingest","arguments":{
  "agent_id":"dev","path":"/your/docs","adapter":"memory","namespace":"docs","mode":"merge"
}}}
// After merge: activate() returns both code files AND relevant docs in one pass
```

**Example — ingest domain-agnostic JSON:**
```jsonc
// /your/domain.json:
// {"nodes": [{"id":"svc::auth","label":"AuthService","type":"module"}],
//  "edges": [{"source":"svc::auth","target":"svc::user","relation":"calls","weight":0.8}]}
{"method":"tools/call","params":{"name":"m1nd_ingest","arguments":{
  "agent_id":"dev","path":"/your/domain.json","adapter":"json"
}}}
```

---

### `m1nd_activate`

Spreading activation query — fires signal into the graph and returns a 4D-scored activation pattern.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `query` | string | yes | Natural language query |
| `top_k` | int | no | Number of results (default: 20) |
| `dimensions` | array | no | Activation dimensions (default: all 4: `["structural","semantic","temporal","causal"]`) |
| `xlr` | bool | no | Enable XLR noise cancellation (default: true) |
| `include_ghost_edges` | bool | no | Include ghost edge detection (default: true) |
| `include_structural_holes` | bool | no | Include structural hole detection (default: false) |

**Output:**

```json
{
  "query": "session handling",
  "seeds": [
    {"node_id": "file::session_pool.py", "label": "session_pool.py", "relevance": 0.91}
  ],
  "activated": [
    {
      "node_id": "file::session_pool.py",
      "label": "session_pool.py",
      "type": "File",
      "activation": 0.89,
      "dimensions": {
        "structural": 0.92,
        "semantic": 0.95,
        "temporal": 0.78,
        "causal": 0.71
      },
      "pagerank": 0.635,
      "tags": [],
      "provenance": {
        "source_path": "backend/session_pool.py",
        "line_start": 1,
        "line_end": 312,
        "namespace": null,
        "canonical": true
      }
    }
  ],
  "ghost_edges": [
    {"source": "file::session_pool.py", "target": "file::healing_manager.py",
     "shared_dimensions": ["structural","temporal"], "strength": 0.34}
  ],
  "structural_holes": [],
  "plasticity": {
    "edges_strengthened": 0,
    "edges_decayed": 0,
    "ltp_events": 0,
    "priming_nodes": 0
  },
  "elapsed_ms": 45.2
}
```

Ghost edges are inferred undocumented connections — invisible to grep.

**Benchmark:** 1.36µs (bench, 1K nodes) · 31–77ms (production, 9,767 nodes)

**Scoring:** 4 dimensions, default weights `[structural=0.35, semantic=0.25, temporal=0.15, causal=0.25]`. 3D resonance match = 1.3x bonus. 4D = 1.5x bonus. Hebbian plasticity shifts weights based on `learn()` feedback.

---

### `m1nd_impact`

Blast radius of a code change — BFS-propagated signal strength from a source node.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `node_id` | string | yes | Starting node (e.g. `"file::payment.py"`) |
| `direction` | string | no | `"forward"` (default), `"backward"`, or `"both"` |
| `include_causal_chains` | bool | no | Include causal chain detection (default: true) |

**Output:**

```json
{
  "source": "file::chat_handler.py",
  "source_label": "chat_handler.py",
  "direction": "forward",
  "blast_radius": [
    {"node_id": "file::session_pool.py", "label": "session_pool.py", "type": "File", "signal_strength": 0.72, "hop_distance": 1},
    {"node_id": "file::whatsapp_manager.py", "label": "whatsapp_manager.py", "type": "File", "signal_strength": 0.51, "hop_distance": 2}
  ],
  "total_energy": 0.635,
  "max_hops_reached": 3,
  "causal_chains": [
    {"path": ["file::chat_handler.py", "file::session_pool.py"], "relations": ["calls"], "cumulative_strength": 0.72}
  ]
}
```

**Benchmark:** 543ns (bench) · 5–52ms (production)

**Note:** There is no `depth` parameter. Use `direction` to control traversal direction.

**Example:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_impact","arguments":{
  "agent_id":"dev","node_id":"file::chat_handler.py"
}}}
```

---

### `m1nd_why`

Shortest path between two nodes — understand how A depends on B.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `source` | string | yes | Source node ID |
| `target` | string | yes | Target node ID |
| `max_hops` | int | no | Maximum hops (default: 6) |

**Output:**

```json
{
  "path": ["file::worker_pool.py", "file::process_manager.py::fn::cancel", "file::whatsapp_manager.py"],
  "hops": 2,
  "edge_types": ["calls", "calls"],
  "path_weight": 0.61
}
```

**Benchmark:** 5–6ms

**Example:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_why","arguments":{
  "agent_id":"dev",
  "source":"file::worker_pool.py",
  "target":"file::whatsapp_manager.py"
}}}
// → 2-hop dependency via cancel function. Invisible to grep.
```

---

### `m1nd_learn`

Hebbian feedback — tell the graph which results were correct or wrong. Edge weights strengthen or weaken accordingly.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `query` | string | yes | Original query this feedback relates to |
| `feedback` | string | yes | `"correct"`, `"wrong"`, or `"partial"` |
| `node_ids` | array | yes | Node IDs that were used/discarded |
| `strength` | float | no | Feedback strength for edge adjustment (default: 0.2) |

**Output:**

```json
{"edges_strengthened": 740, "ltp_applied": true}
```

**Benchmark:** <1ms

**When to call:** After every `activate` where you used the results. After every `hypothesize` where you confirmed a verdict. This is what makes the graph smarter over time.

---

### `m1nd_drift`

What changed in the graph since your last session — structural delta for context recovery.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `since` | string | no | Timestamp or `"last_session"` (default: `"last_session"`) |
| `include_weight_drift` | bool | no | Include edge weight drift analysis (default: true) |

**Output:**

```json
{
  "nodes_added": 12,
  "nodes_removed": 3,
  "edges_strengthened": 47,
  "edges_weakened": 8,
  "top_changed": ["file::worker_pool.py", "file::session_pool.py"]
}
```

**Benchmark:** 23ms

**When to call:** At session start, to recover context from the previous session without re-reading files.

---

### `m1nd_health`

Server diagnostics — verify the MCP server is alive and the graph is loaded.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |

**Output:**

```json
{
  "status": "ok",
  "node_count": 9767,
  "edge_count": 26557,
  "queries_processed": 142,
  "uptime_seconds": 342,
  "memory_usage_bytes": 48234496,
  "plasticity_state": "active",
  "last_persist_time": "2026-03-14T19:00:00Z",
  "active_sessions": []
}
```

**Benchmark:** <1ms

---

### `m1nd_seek`

Find code by natural language intent — more targeted than `activate` for specific lookups.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `query` | string | yes | Natural language description of what to find |
| `top_k` | int | no | Maximum results (default: 20) |
| `scope` | string | no | File path prefix to limit search scope |
| `node_types` | array | no | Filter by node type: `"function"`, `"class"`, `"struct"`, `"module"`, `"file"` |
| `min_score` | float | no | Minimum combined score (default: 0.1) |
| `graph_rerank` | bool | no | Run graph re-ranking on embedding candidates (default: true) |

**Benchmark:** 10–15ms

---

### `m1nd_scan`

Run structural pattern scanners. Use a predefined pattern ID or a custom pattern string.

**Predefined patterns:** `"error_handling"`, `"resource_cleanup"`, `"api_surface"`, `"state_mutation"`, `"concurrency"`, `"auth_boundary"`, `"test_coverage"`, `"dependency_injection"`

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `pattern` | string | yes | Pattern ID or custom ast-grep pattern string |
| `scope` | string | no | File path prefix to limit scan scope |
| `severity_min` | float | no | Minimum severity threshold [0.0, 1.0] (default: 0.3) |
| `graph_validate` | bool | no | Validate findings against graph edges (default: true) |
| `limit` | int | no | Maximum findings to return (default: 50) |

**Note:** `scan` takes a single `pattern` (not a `patterns` array). Run once per pattern, or use the predefined IDs for the 8 built-in scanners.

**Benchmark:** 3–5ms per pattern · 38ms for all 8 (production, 335 files)

---

### `m1nd_timeline`

Temporal evolution of a node — change history, co-change partners, velocity, stability.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `node` | string | yes | Node external_id (e.g. `"file::backend/chat_handler.py"`) |
| `depth` | string | no | Time depth: `"7d"`, `"30d"`, `"90d"`, `"all"` (default: `"30d"`) |
| `include_co_changes` | bool | no | Include co-changed files with coupling scores (default: true) |
| `include_churn` | bool | no | Include lines added/deleted churn data (default: true) |
| `top_k` | int | no | Max co-change partners (default: 10) |

**Note:** The input parameter is `node` (not `node_id`).

**Benchmark:** ~ms

---

### `m1nd_diverge`

Structural drift analysis — compare graph state against a baseline reference.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `baseline` | string | yes | Baseline reference: ISO date (`"2026-03-01"`), git ref (SHA/tag), or `"last_session"` |
| `scope` | string | no | File path glob to limit scope |
| `include_coupling_changes` | bool | no | Include coupling matrix delta (default: true) |
| `include_anomalies` | bool | no | Detect anomalies (test deficits, velocity spikes) (default: true) |

**Note:** `diverge` takes a single `baseline` parameter (not `ref_a`/`ref_b`). It compares current graph state against one baseline point.

**Benchmark:** Varies with git history size

---

### `m1nd_warmup`

Prime the graph for an upcoming task — pre-activates seed nodes to improve subsequent query relevance.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `task_description` | string | yes | Description of upcoming work |
| `boost_strength` | float | no | Priming boost strength (default: 0.15) |

**Note:** The input parameter is `task_description` (not `task`).

**Output:**

```json
{"seeds_primed": 50, "elapsed_ms": 89}
```

**Benchmark:** 82–89ms

**When to call:** Before a focused work session (e.g., "refactor payment flow"). Subsequent `activate` and `impact` calls return better results.

---

### `m1nd_federate`

Unify multiple repositories into one graph — cross-repo blast radius and dependency analysis.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `repos` | array | yes | List of `{name, path, adapter}` objects |
| `detect_cross_repo_edges` | bool | no | Auto-detect cross-repo edges (default: true) |
| `incremental` | bool | no | Only re-ingest repos that changed (default: false) |

**Repo object fields:** `name` (string, required — namespace prefix), `path` (string, required — absolute path), `adapter` (string, optional, default: `"code"`)

**Note:** The repo object field is `name` (not `label`).

**Output:**

```json
{
  "repos_ingested": [
    {"name": "backend", "path": "/app/backend", "node_count": 9767, "edge_count": 26557, "from_cache": false, "ingest_ms": 910}
  ],
  "total_nodes": 11217,
  "total_edges": 18203,
  "cross_repo_edges": [],
  "cross_repo_edge_count": 0,
  "incremental": false,
  "skipped_repos": [],
  "elapsed_ms": 1300
}
```

**Benchmark:** 1.3s for 2 repos (11,217 nodes, 18,203 cross-repo edges)

**Example:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_federate","arguments":{
  "agent_id":"dev",
  "repos":[
    {"name":"backend","path":"/app/backend"},
    {"name":"frontend","path":"/app/frontend"}
  ]
}}}
// → activate("API contract") returns both backend handlers AND frontend consumers
```

---

### `m1nd_federate_auto`

Discover candidate sibling repositories from explicit external path references or local manifest/workspace hints and optionally execute federation in one step.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `scope` | string | no | Limit discovery to a file path prefix |
| `current_repo_name` | string | no | Optional namespace override for the current workspace |
| `max_repos` | integer | no | Maximum discovered repos to include (default: 8) |
| `detect_cross_repo_edges` | bool | no | Whether `execute=true` should auto-detect cross-repo edges (default: true) |
| `execute` | bool | no | If true, immediately run `federate` with the current repo plus discovered candidates |

**Output:**

```json
{
  "current_repo": {"namespace": "m1nd", "repo_root": "/repo/m1nd"},
  "discovered_repos": [
    {
      "namespace": "runtime",
      "repo_root": "/repo/runtime",
      "marker": ".git",
      "confidence": "high",
      "evidence_types": ["markdown_link"],
      "source_nodes": ["file::docs/architecture.md"],
      "source_files": ["/repo/m1nd/docs/architecture.md"],
      "sampled_paths": ["/repo/runtime/docs/ARCH.md"],
      "suggested_action": "run federate_auto with execute=true or pass suggested_repos into federate"
    }
  ],
  "suggested_repos": [
    {"name": "runtime", "path": "/repo/runtime", "adapter": "code"}
  ],
  "skipped_paths": [],
  "executed": false,
  "federate_result": null,
  "elapsed_ms": 42.0
}
```

**Example:**

```jsonc
{"method":"tools/call","params":{"name":"m1nd_federate_auto","arguments":{
  "agent_id":"dev",
  "scope":"docs",
  "execute":false
}}}
```

---

## Perspective Navigation (12 tools)

Navigate the graph like a filesystem. Open a perspective anchored to a node, follow routes, peek at source code, branch explorations, compare findings between agents.

---

### `m1nd_perspective_start`

Open a perspective anchored to a node. Perspectives are stateful navigation sessions.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `query` | string | yes | Starting query |
| `anchor_node` | string | no | Pin perspective to a specific node |
| `lens` | object | no | Custom ranking: `{dimensions, route_families, xlr, top_k, namespaces, node_types}` |

**Output:**

```json
{
  "perspective_id": "persp-4f2a",
  "mode": "Local",
  "anchor_node": null,
  "focus_node": "file::auth.py",
  "routes": [...],
  "total_routes": 9,
  "page": 1,
  "total_pages": 2,
  "route_set_version": 1741982400000,
  "cache_generation": 42,
  "suggested": "inspect R01"
}
```

Modes: `"Local"` (query-driven) or `"Anchored"` (pinned to `anchor_node`). Anchored mode degrades to local after 8 hops. Routes paginate at 6/page.

---

### `m1nd_perspective_routes`

List available navigation routes from the current focus node.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `perspective_id` | string | yes | Perspective to query |
| `page` | int | no | Page number, 1-based (default: 1) |
| `page_size` | int | no | Routes per page, clamped to [1, 10] (default: 6) |
| `route_set_version` | int | no | Version from previous response for staleness check |

**Output:** Array of routes with type (Structural, Causal, Hole, etc.), target node, and relevance score.

---

### `m1nd_perspective_follow`

Move the perspective focus to a route target.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `perspective_id` | string | yes | Perspective to navigate |
| `route_id` | string | no | Stable content-addressed route ID |
| `route_index` | int | no | 1-based page-local route index |
| `route_set_version` | int | no | Version from previous response |

Provide either `route_id` or `route_index`.

---

### `m1nd_perspective_back`

Navigate backward to the previous focus node.

**Inputs:** `agent_id`, `perspective_id`

---

### `m1nd_perspective_peek`

Read source code at the currently focused node (returns file content at the node's location).

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `perspective_id` | string | yes | Perspective to peek |
| `route_id` | string | no | Route ID to peek |
| `route_index` | int | no | Route index to peek |
| `route_set_version` | int | no | Version check |

**Output:**
```json
{
  "node_id": "file::worker_pool.py::fn::submit",
  "source": "async def submit(self, task: Task) -> str:\n    ...",
  "file_path": "worker_pool.py",
  "start_line": 42,
  "end_line": 67
}
```

---

### `m1nd_perspective_inspect`

Deep metadata + 5-factor score breakdown for a route's target node.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `perspective_id` | string | yes | Perspective |
| `route_id` | string | no | Stable route ID |
| `route_index` | int | no | 1-based page-local route index |
| `route_set_version` | int | no | Version check |

**Output:** PageRank, in-degree, out-degree, temporal score, causal score, per-dimension breakdown.

---

### `m1nd_perspective_suggest`

AI navigation recommendation — the graph suggests which route to follow next.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `perspective_id` | string | yes | Perspective |
| `route_set_version` | int | no | Version from previous response |

**Note:** `suggest` does not take a `goal` parameter.

---

### `m1nd_perspective_affinity`

Check route relevance to the current investigation.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `perspective_id` | string | yes | Perspective |
| `route_id` | string | no | Route ID |
| `route_index` | int | no | Route index |
| `route_set_version` | int | no | Version check |

**Note:** `affinity` does not take a `goal` string parameter. It checks affinity of a specific route.

**Output:** `{"affinity": 0.74, "reason": "shares 3 causal ancestors with current focus"}`

---

### `m1nd_perspective_branch`

Fork an independent perspective copy for parallel exploration. Two agents can investigate the same starting point independently, then compare findings.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `perspective_id` | string | yes | Source perspective |
| `branch_name` | string | no | Optional branch name |

**Output:** `{"new_perspective_id": "persp-9b1c"}`

---

### `m1nd_perspective_compare`

Diff two perspectives — shared nodes, unique nodes, divergent findings.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `perspective_id_a` | string | yes | First perspective |
| `perspective_id_b` | string | yes | Second perspective |
| `dimensions` | array | no | Dimensions to compare (empty = all) |

**Output:**
```json
{
  "shared_nodes": ["file::auth.py", "file::session_pool.py"],
  "unique_to_a": ["file::middleware.py"],
  "unique_to_b": ["file::jwt_utils.py"],
  "convergence_score": 0.62
}
```

---

### `m1nd_perspective_list`

All active perspectives for this agent, with memory usage.

**Inputs:** `agent_id`

---

### `m1nd_perspective_close`

Release perspective state and free memory.

**Inputs:** `agent_id`, `perspective_id`

---

## Lock System (5 tools)

Pin a subgraph region and watch for changes. Useful for teams working in parallel — detect structural conflicts before they become merge conflicts.

`m1nd_lock_diff` runs in **0.08µs** — essentially free.

---

### `m1nd_lock_create`

Snapshot a subgraph region.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `scope` | string | yes | Lock scope: `"node"`, `"subgraph"`, `"query_neighborhood"`, `"path"` |
| `root_nodes` | array | yes | Root nodes for the lock region |
| `radius` | int | no | BFS radius for `subgraph` scope (1–4) |
| `query` | string | no | Query for `query_neighborhood` scope |
| `path_nodes` | array | no | Ordered nodes for `path` scope |

**Note:** `lock.create` does NOT take a `center` parameter. Use `scope` + `root_nodes` instead of `center` + `radius`.

**Output:**
```json
{"lock_id": "lock-7c3d", "scope": "subgraph", "baseline_nodes": 1639, "baseline_edges": 707, "graph_generation": 42, "created_at_ms": 1741982400000}
```

**Benchmark:** 24ms

---

### `m1nd_lock_watch`

Register a change strategy on a lock.

**Inputs:** `agent_id`, `lock_id`, `strategy`

Strategy options: `"manual"`, `"on_ingest"`, `"on_learn"`. Note: `"OnAnyChange"` and `"Periodic"` are NOT valid — they do not exist.

---

### `m1nd_lock_diff`

Compare current graph state against the locked baseline. Returns changes since the lock was created.

**Inputs:** `agent_id`, `lock_id`

**Output:**
```json
{
  "diff": {
    "changed": false,
    "nodes_added": 0,
    "nodes_removed": 0,
    "edges_changed": 0
  },
  "watcher_events_drained": 0
}
```

**Benchmark:** 0.08µs (fast-path when `graph_generation` unchanged)

**Example:**
```jsonc
// Lock a region before starting work
{"name":"m1nd_lock_create","arguments":{"agent_id":"dev","scope":"subgraph","root_nodes":["file::chat_handler.py"],"radius":2}}
// → lock_id: "lock-7c3d", baseline_nodes: 1639

// Check if another agent changed the region (runs in 0.08µs)
{"name":"m1nd_lock_diff","arguments":{"agent_id":"dev","lock_id":"lock-7c3d"}}
// → {"diff": {"changed": false}} — safe to proceed
```

---

### `m1nd_lock_rebase`

Advance the lock baseline to the current graph state (after reviewing and accepting changes).

**Inputs:** `agent_id`, `lock_id`

**Benchmark:** 22ms

---

### `m1nd_lock_release`

Free lock state and release memory.

**Inputs:** `agent_id`, `lock_id`

**Benchmark:** ~0ms

---

## Superpowers (13 tools)

Advanced reasoning: hypothesis testing, counterfactual simulation, structural hole detection, standing wave analysis, investigation trails, and more.

---

### `m1nd_hypothesize`

Test a claim against graph structure using Bayesian path scoring. **89% accuracy validated on 10 live claims against a production codebase.**

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `claim` | string | yes | Natural language claim to test |
| `max_hops` | int | no | Max BFS hops for evidence search (default: 5) |
| `include_ghost_edges` | bool | no | Include ghost edges as weak evidence (default: true) |
| `include_partial_flow` | bool | no | Include partial flow when full path not found (default: true) |
| `path_budget` | int | no | Budget cap for all-paths enumeration (default: 1000) |

**Output:**
```json
{
  "claim": "worker_pool depends on whatsapp_manager at runtime",
  "claim_type": "depends_on",
  "subject_nodes": ["file::worker_pool.py"],
  "object_nodes": ["file::whatsapp_manager.py"],
  "verdict": "likely_true",
  "confidence": 0.72,
  "supporting_evidence": [
    {
      "type": "path_found",
      "description": "2-hop path via cancel function",
      "likelihood_factor": 0.72,
      "nodes": ["file::worker_pool.py","file::process_manager.py::fn::cancel","file::whatsapp_manager.py"],
      "relations": ["calls","calls"],
      "path_weight": 0.61
    }
  ],
  "contradicting_evidence": [],
  "paths_explored": 25015,
  "elapsed_ms": 42.1
}
```

Verdicts: `"likely_true"` (>0.8 confidence), `"likely_false"` (<0.2), or `"inconclusive"`.

**Benchmark:** 28–58ms (25,015 paths explored)

**Example:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_hypothesize","arguments":{
  "agent_id":"dev","claim":"worker_pool depends on whatsapp_manager at runtime"
}}}
// → likely_true, 72% confidence, 2-hop path via cancel function
```

---

### `m1nd_counterfactual`

Simulate module removal — compute full cascade of what breaks.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `node_ids` | array | yes | Nodes to simulate removing |
| `include_cascade` | bool | no | Return full depth-by-depth breakdown (default: true) |

**Output:**
```json
{
  "cascade": [
    {"depth": 1, "affected": 23},
    {"depth": 2, "affected": 456},
    {"depth": 3, "affected": 3710}
  ],
  "total_affected": 4189,
  "orphaned_count": 0,
  "pct_activation_lost": 0.41
}
```

Multi-node removal detects synergy:
```json
{
  "synergy_factor": 1.42,
  "reachability_before": 0.91,
  "reachability_after": 0.49
}
```
`synergy_factor > 1.0` means removal is super-additive — the nodes are architecturally coupled and should be deleted together.

**Benchmark:** 3ms (single node) · 8ms (multi-node with synergy)

---

### `m1nd_missing`

Find structural holes — what *should* exist in a region of the graph but doesn't.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `query` | string | yes | Region to analyze |
| `min_sibling_activation` | float | no | Minimum sibling activation threshold (default: 0.3) |

**Output:**
```json
{
  "holes": [
    {"region": "connection lifecycle", "adjacent_nodes": 4, "description": "No dedicated connection pool abstraction"},
    {"region": "pool metrics", "adjacent_nodes": 3, "description": "No pool health monitoring"}
  ],
  "total_holes": 9
}
```

**Benchmark:** 44–67ms

**Use for:** Gap analysis before writing specs. Finding orphaned specs in memory adapter graphs. Pre-code review.

---

### `m1nd_resonate`

Standing wave analysis — find structural hubs where signal reinforces across multiple dimensions simultaneously.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `query` | string | no | Search query to find seed nodes (alternative to `node_id`) |
| `node_id` | string | no | Specific node to use as seed (alternative to `query`) |
| `top_k` | int | no | Number of results (default: 20) |

At least one of `query` or `node_id` must be provided.

**Output:** Nodes with highest resonance score + sympathetic pairs + resonant frequency breakdown.

**Benchmark:** 37–52ms (production) · 8.17µs (bench)

---

### `m1nd_fingerprint`

Find structural twins — nodes with identical or near-identical topology.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `target_node` | string | no | Node to find equivalents for (if omitted, scans all pairs) |
| `similarity_threshold` | float | no | Cosine similarity threshold (default: 0.85) |
| `probe_queries` | array | no | Optional probe queries for targeted matching |

**Note:** The parameter is `target_node` (not `node_id`).

**Output:** Pairs with similarity score. Use for duplicate detection, consolidation candidates, pattern matching.

**Benchmark:** 1–107ms

---

### `m1nd_trace`

Map a stacktrace to root cause suspects — ranks files by suspiciousness × centrality.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `error_text` | string | yes | Full error output (stacktrace + error message) |
| `language` | string | no | Language hint: `"python"`, `"rust"`, `"typescript"`, `"javascript"`, `"go"` (auto-detected if omitted) |
| `window_hours` | float | no | Temporal window for co-change suspect scan (default: 24.0) |
| `top_k` | int | no | Max suspects to return (default: 10) |

**Note:** The parameter is `error_text` (not `stacktrace`).

**Output:**
```json
{
  "language_detected": "python",
  "error_type": "AttributeError",
  "error_message": "...",
  "frames_parsed": 5,
  "frames_mapped": 4,
  "suspects": [
    {"node_id": "file::worker_pool.py", "label": "worker_pool.py", "type": "File",
     "suspiciousness": 0.89,
     "signals": {"trace_depth_score": 0.9, "recency_score": 0.8, "centrality_score": 0.72},
     "related_callers": []}
  ],
  "co_change_suspects": [],
  "causal_chain": ["file::worker_pool.py"],
  "fix_scope": {"files_to_inspect": ["worker_pool.py"], "estimated_blast_radius": 47, "risk_level": "high"},
  "unmapped_frames": [],
  "elapsed_ms": 4.2
}
```

**Benchmark:** 3.5–5.8ms (5 frames → 4 suspects ranked)

---

### `m1nd_validate_plan`

Pre-flight risk assessment for a set of planned changes — blast radius, gap count, risk score.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `actions` | array | yes | Ordered list of planned actions |
| `include_test_impact` | bool | no | Analyze test coverage for modified files (default: true) |
| `include_risk_score` | bool | no | Compute composite risk score (default: true) |

**Action object fields:** `action_type` (required: `"modify"`, `"create"`, `"delete"`, `"rename"`, or `"test"`), `file_path` (required), `description` (optional), `depends_on` (optional array of file_paths)

**Note:** The parameter is `actions` (not `files`). Each action has `action_type` + `file_path`, not just filenames.

**Output:**
```json
{
  "actions_analyzed": 7,
  "actions_resolved": 6,
  "actions_unresolved": 1,
  "gaps": [],
  "risk_score": 0.70,
  "risk_level": "high",
  "test_coverage": {"modified_files": 6, "tested_files": 4, "untested_files": ["billing.py"], "coverage_ratio": 0.67},
  "suggested_additions": [],
  "blast_radius_total": 43152,
  "elapsed_ms": 10.1
}
```

**Benchmark:** 0.5–10ms · 10ms for 7 files (43,152 blast radius)

---

### `m1nd_predict`

Co-change prediction — given a file you just changed, which other files likely need changes too.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `changed_node` | string | yes | Node ID that was changed (e.g. `"file::billing.py"`) |
| `top_k` | int | no | Number of predictions (default: 10) |
| `include_velocity` | bool | no | Include velocity scoring (default: true) |

**Output:**
```json
{
  "predictions": [
    {"node": "file::billing.py", "probability": 0.84},
    {"node": "file::invoice.py", "probability": 0.71}
  ]
}
```

**Benchmark:** <1ms

---

### `m1nd_trail_save`

Persist investigation state — hypotheses, conclusions, open questions, visited nodes, and activation boosts.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `label` | string | yes | Human-readable label |
| `hypotheses` | array | no | List of `{statement, confidence, supporting_nodes, contradicting_nodes}` objects |
| `conclusions` | array | no | List of `{statement, confidence, from_hypotheses, supporting_nodes}` objects |
| `open_questions` | array | no | List of open question strings |
| `tags` | array | no | Tags for organization and search |
| `summary` | string | no | Free-form summary (auto-generated if omitted) |
| `visited_nodes` | array | no | Explicitly listed visited nodes with annotations |
| `activation_boosts` | object | no | Map of `node_external_id → boost_weight [0.0, 1.0]` |

**Note:** There is no `notes` parameter — use `summary` instead.

**Output:** `{"trail_id": "trail-abc123", "nodes_saved": 12, "hypotheses_saved": 2, "conclusions_saved": 1, "open_questions_saved": 0}`

**Benchmark:** ~0ms

---

### `m1nd_trail_resume`

Restore exact investigation context — nodes, weights, hypotheses — from a saved trail.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `trail_id` | string | yes | Trail ID to resume |
| `force` | bool | no | Resume even if trail is stale (>50% missing nodes) (default: false) |

**Output:**
```json
{
  "trail_id": "trail-abc123",
  "label": "race condition investigation",
  "stale": false,
  "generations_behind": 0,
  "missing_nodes": [],
  "nodes_reactivated": 47,
  "hypotheses_downgraded": [],
  "trail": {...},
  "elapsed_ms": 0.2
}
```

Stale nodes = nodes that were deleted since the trail was saved.

**Benchmark:** 0.2ms

---

### `m1nd_trail_merge`

Combine two or more agents' independent investigations. Auto-detects where they converged and flags conflicts on shared hypotheses.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `trail_ids` | array | yes | Two or more trail IDs to merge |
| `label` | string | no | Label for merged trail (auto-generated if omitted) |

**Note:** The parameter is `trail_ids` (an array). There are no separate `trail_id_a`/`trail_id_b` parameters. Supports merging 2+ trails simultaneously.

**Output:**
```json
{
  "merged_trail_id": "trail-merged-xyz",
  "label": "merged investigation",
  "source_trails": ["trail-a", "trail-b"],
  "nodes_merged": 23,
  "hypotheses_merged": 5,
  "conflicts": [
    {"hypothesis_a": "...", "hypothesis_b": "...", "resolution": "unresolved", "score_delta": 0.12}
  ],
  "connections_discovered": [
    {"type": "shared_node", "detail": "file::worker_pool.py appears in both trails"}
  ],
  "elapsed_ms": 1.2
}
```

**Benchmark:** 1.2ms (5 hypotheses, 3 conflicts detected)

---

### `m1nd_trail_list`

Browse all saved investigations with optional filters.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `filter_agent_id` | string | no | Filter to a specific agent's trails |
| `filter_status` | string | no | Filter by status: `"active"`, `"saved"`, `"archived"`, `"stale"`, `"merged"` |
| `filter_tags` | array | no | Filter by tags (any match) |

**Output:** Array of `{trail_id, label, status, created_at_ms, hypothesis_count, conclusion_count, node_count}`.

**Benchmark:** ~0ms

---

### `m1nd_differential`

Focused structural diff between two graph snapshots — surface what changed structurally between two points in time.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `snapshot_a` | string | yes | Path to snapshot A, or `"current"` |
| `snapshot_b` | string | yes | Path to snapshot B, or `"current"` |
| `question` | string | no | Focus question to narrow the diff output |
| `focus_nodes` | array | no | Limit diff to the neighborhood of specific node IDs |

**Output:**
```json
{
  "snapshot_a": "...",
  "snapshot_b": "current",
  "new_edges": [],
  "removed_edges": [],
  "weight_changes": [
    {"source": "file::auth.py", "target": "file::session.py", "relation": "calls", "old_weight": 0.4, "new_weight": 0.8, "delta": 0.4}
  ],
  "new_nodes": ["file::new_module.py"],
  "removed_nodes": [],
  "coupling_deltas": [],
  "summary": "14 nodes added, 3 removed, 47 edges changed",
  "elapsed_ms": 8.1
}
```

**Benchmark:** ~ms (varies with snapshot size)

---

## Superpowers Extended (9 tools)

Immune memory, concurrent flow simulation, epidemic propagation, tremor detection, trust scoring, and architectural layer analysis.

---

### `m1nd_antibody_scan`

Scan the graph against all stored bug antibody patterns (known bug shapes). Returns matches with confidence and bindings.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `scope` | string | no | `"all"` (default) or `"changed"` (only recently changed nodes) |
| `antibody_ids` | array | no | Only scan specific antibodies (default: all) |
| `max_matches` | int | no | Cap on total results (default: 50) |
| `min_severity` | string | no | `"info"` (default), `"warning"`, or `"critical"` |
| `similarity_threshold` | float | no | Minimum match confidence (default: 0.7) |
| `match_mode` | string | no | Label match mode: `"exact"`, `"substring"` (default), `"regex"` |
| `max_matches_per_antibody` | int | no | Cap per antibody (default: 50) |

**Note:** `min_severity` default is `"info"`. Severity scale is `"info"`, `"warning"`, `"critical"` — not `"low"`, `"medium"`, `"high"`.

**Output:**
```json
{
  "matches": [
    {
      "antibody_id": "ab-0012",
      "label": "TOCTOU is_alive check",
      "severity": "high",
      "confidence": 0.84,
      "matched_at": "file::worker_pool.py::fn::submit",
      "bindings": [
        {"pattern_node": "check", "matched_node": "file::worker_pool.py::fn::is_alive"},
        {"pattern_node": "use",   "matched_node": "file::worker_pool.py::fn::send"}
      ]
    }
  ],
  "total_scanned": 47,
  "total_matches": 2,
  "elapsed_ms": 45
}
```

**Benchmark:** 2.68ms (50 patterns, bench) · <100ms full registry scan (10ms budget per pattern)

**When to call:** After every `ingest` to auto-check for recurring bug classes.

---

### `m1nd_antibody_list`

List all stored antibodies with match history.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `include_disabled` | bool | no | Include disabled antibodies (default: false) |

**Output:** Array of `{antibody_id, label, severity, match_count, last_matched}`.

**Benchmark:** ~0ms

---

### `m1nd_antibody_create`

Create, disable, enable, or delete an antibody pattern.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `action` | string | no | `"create"` (default), `"disable"`, `"enable"`, `"delete"` |
| `name` | string | create only | Human-readable name |
| `description` | string | no | What bug this catches |
| `severity` | string | no | `"info"`, `"warning"` (default), `"critical"` |
| `pattern` | object | create only | `{nodes, edges, negative_edges}` |
| `antibody_id` | string | non-create | Target antibody for action |

**Pattern node schema:** `{role (string), node_type? (string), required_tags? (array), label_contains? (string)}`

**Pattern edge schema:** `{source_idx (int), target_idx (int), relation? (string)}`

**Important:** Pattern nodes use `role` (not `id`). Pattern edges use `source_idx`/`target_idx` (integer indices into the nodes array, not string references). There is no `label_pattern` or `match_mode` field on nodes — use `label_contains`.

**Pattern schema:**
```json
{
  "nodes": [
    {"role": "check", "label_contains": "is_alive"},
    {"role": "use",   "label_contains": "send"}
  ],
  "edges": [
    {"source_idx": 0, "target_idx": 1, "relation": "calls"}
  ],
  "negative_edges": [
    {"source_idx": 0, "target_idx": 2, "relation": "calls"}
  ]
}
```

`negative_edges` = structural **absence** detection. The pattern matches ONLY when the negative edge's target is NOT present. This is what grep cannot do.

**Example — teach m1nd a new bug shape:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_antibody_create","arguments":{
  "agent_id":"dev",
  "action":"create",
  "name":"CancelledError swallowed",
  "severity":"warning",
  "pattern":{
    "nodes":[
      {"role":"handler","label_contains":"CancelledError"},
      {"role":"body",   "label_contains":"pass"}
    ],
    "edges":[{"source_idx":0,"target_idx":1,"relation":"contains"}],
    "negative_edges":[{"source_idx":0,"target_idx":2,"relation":"contains"}]
  }
}}}
// → antibody_id: "ab-0031", specificity: 0.68
```

**Benchmark:** ~0ms

---

### `m1nd_flow_simulate`

Concurrent execution flow simulation — particles travel the graph in parallel, turbulence points are shared mutable state collisions (race conditions).

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `entry_nodes` | array | no | Starting nodes (auto-discovered if empty) |
| `num_particles` | int | no | Particles per entry point (default: 2, max: 100) |
| `lock_patterns` | array | no | Regex patterns for lock/mutex detection |
| `read_only_patterns` | array | no | Regex patterns for read-only operations |
| `max_depth` | int | no | Maximum BFS depth (default: 15) |
| `turbulence_threshold` | float | no | Minimum score to report (default: 0.5) |
| `include_paths` | bool | no | Include particle paths in output (default: true) |
| `max_total_steps` | int | no | Global step budget across all particles (default: 50000) |
| `scope_filter` | string | no | Substring filter to limit which nodes particles can enter |

**Output:**
```json
{
  "turbulence_points": [
    {
      "node": "file::session_pool.py::fn::_registry",
      "severity": "critical",
      "collision_count": 6,
      "entry_pairs": [
        ["file::chat_handler.py::fn::handle_message", "file::ws_relay.py::fn::broadcast"]
      ]
    }
  ],
  "valve_points": [
    {"node": "file::worker_pool.py::fn::acquire", "particle_throughput": 0.12, "label": "lock bottleneck"}
  ],
  "summary": {
    "turbulence_count": 1,
    "valve_count": 1,
    "steps_total": 312
  },
  "elapsed_ms": 92
}
```

`turbulence` = race condition hotspot. `valve` = lock contention bottleneck.

**Benchmark:** 552µs (4 particles, bench) · 92ms (production)

---

### `m1nd_epidemic`

SIR epidemiological bug propagation — given known-buggy modules, predict which neighbors are most likely to harbor undiscovered bugs.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `infected_nodes` | array | yes | Known-buggy node IDs |
| `recovered_nodes` | array | no | Already-fixed node IDs (default: empty) |
| `infection_rate` | float | no | Uniform infection rate (if omitted, derived from edge weights) |
| `recovery_rate` | float | no | SIR recovery rate (default: 0) |
| `iterations` | int | no | SIR simulation steps (default: 50, max: 500) |
| `direction` | string | no | `"forward"`, `"backward"`, or `"both"` (default: `"both"`) |
| `top_k` | int | no | Predictions to return (default: 20) |
| `auto_calibrate` | bool | no | Auto-tune infection rates (default: true) |
| `scope` | string | no | Filter predictions: `"files"`, `"functions"`, `"all"` (default: `"all"`) |
| `min_probability` | float | no | Minimum infection probability threshold (default: 0.001) |

**Output:**
```json
{
  "predictions": [
    {"node": "file::process_manager.py", "probability": 0.81, "state": "infected"},
    {"node": "file::chat_handler.py",    "probability": 0.67, "state": "infected"}
  ],
  "summary": {
    "R0": 2.3,
    "peak_infected": 12,
    "final_infected": 8
  },
  "elapsed_ms": 38
}
```

`R0 > 1.0` = bug pattern is spreading through the codebase.

Transmission rates by edge type: `imports=0.8`, `calls=0.7`, `inherits=0.6`, `refs=0.4`, `contains=0.3`.

**Benchmark:** 110µs (50 iterations, bench) · 38ms (production)

---

### `m1nd_tremor`

Change frequency acceleration detection — identifies modules with *accelerating* change frequency. Acceleration precedes bugs, not just high churn.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `window` | string | no | `"7d"`, `"30d"` (default), `"90d"`, `"all"` |
| `threshold` | float | no | Minimum magnitude to report (default: 0.1) |
| `top_k` | int | no | Top accelerating modules (default: 20) |
| `node_filter` | string | no | Filter to nodes matching this prefix |
| `include_history` | bool | no | Return observation history (default: false) |
| `min_observations` | int | no | Minimum data points to compute tremor (default: 3) |
| `sensitivity` | float | no | Multiplier on acceleration threshold (default: 1.0) |

**Note:** `window` uses lowercase values (`"7d"`, `"30d"`, `"90d"`, `"all"`), not PascalCase (`"Days7"` etc.). Path filtering uses `node_filter`, not a variant of `threshold`.

**Output:**
```json
{
  "alerts": [
    {
      "node": "file::stormender_v2_runtime.py",
      "direction": "Accelerating",
      "magnitude": 8.4,
      "risk": "Critical",
      "recent_slope": 0.72,
      "history": [0.3, 0.5, 0.9, 1.4, 2.1]
    }
  ],
  "stable_count": 142,
  "decelerating_count": 7
}
```

Risk tiers: `Critical` (magnitude > 5 AND slope > 0.5), `High`, `Medium`.

**Benchmark:** 236µs (500 nodes, bench) · 14ms (production)

---

### `m1nd_trust`

Per-module actuarial trust scores from defect history. More confirmed bugs = lower trust = higher risk weighting in activation queries.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `scope` | string | no | Node type: `"file"` (default), `"function"`, `"class"`, `"all"` |
| `top_k` | int | no | Modules to return (default: 20) |
| `node_filter` | string | no | Filter to nodes matching this prefix |
| `sort_by` | string | no | `"trust_asc"` (default), `"trust_desc"`, `"defects_desc"`, `"recency"` |
| `min_history` | int | no | Minimum defect events required (default: 1) |
| `decay_half_life_days` | float | no | Recency decay in days (default: 30.0) |
| `risk_cap` | float | no | Maximum risk multiplier cap (default: 3.0) |

**Note:** `sort_by` uses snake_case (`"trust_asc"`, `"defects_desc"`, `"recency"`), not PascalCase. `scope` filters by node type; use `node_filter` for path prefix filtering.

**Output:**
```json
{
  "nodes": [
    {
      "node": "file::worker_pool.py",
      "trust_score": 0.23,
      "tier": "HighRisk",
      "defect_count": 7,
      "last_defect_hours_ago": 48
    }
  ],
  "summary": {
    "high_risk_count": 3,
    "medium_risk_count": 11,
    "avg_trust": 0.71
  }
}
```

Tiers: `HighRisk` (< 0.4), `MediumRisk` (< 0.7), `LowRisk` (>= 0.7).

**Benchmark:** 70µs (500 nodes, bench) · 6ms (production)

**Keep scores current:** call `m1nd_learn` with `feedback="correct"` and `node_ids=[buggy_file]` after confirming a bug.

---

### `m1nd_layers`

Auto-detect architectural layers from graph topology. Uses BFS longest-path depth assignment + Tarjan SCC for circular groups. Reports dependency violations.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `scope` | string | no | Path prefix to filter |
| `max_layers` | int | no | Maximum layers to detect (default: 8) |
| `include_violations` | bool | no | Report upward/circular/skip-layer violations (default: true) |
| `min_nodes_per_layer` | int | no | Minimum nodes for a layer to be reported (default: 2) |
| `node_types` | array | no | Filter by node types |
| `naming_strategy` | string | no | `"auto"` (default), `"path_prefix"`, `"pagerank"` |
| `exclude_tests` | bool | no | Exclude test files from layer detection (default: false) |
| `violation_limit` | int | no | Maximum violations to return (default: 100) |

**Note:** `naming_strategy` default is `"auto"` (not `"heuristic"`).

**Output:**
```json
{
  "layers": [
    {"level": 0, "name": "entry",    "nodes": ["file::main.py"],          "node_count": 4},
    {"level": 1, "name": "routes",   "nodes": ["file::chat_routes.py"],   "node_count": 23},
    {"level": 2, "name": "handlers", "nodes": ["file::chat_handler.py"],  "node_count": 31},
    {"level": 3, "name": "services", "nodes": ["file::session_pool.py"],  "node_count": 18},
    {"level": 4, "name": "core",     "nodes": ["file::config.py"],        "node_count": 9}
  ],
  "violations": [
    {
      "from": "file::config.py",
      "to":   "file::chat_routes.py",
      "type": "UpwardDependency",
      "severity": "high"
    }
  ],
  "total_violations": 1,
  "layer_count": 5
}
```

Violation types: `UpwardDependency`, `CircularDependency`, `SkipLayer`.

**Benchmark:** 862µs (500 nodes, bench) · 71ms (production)

---

### `m1nd_layer_inspect`

Inspect a specific architectural layer: nodes, inter-layer connections, health metrics.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `level` | int | yes | Layer level to inspect |
| `scope` | string | no | Path prefix to filter |
| `include_edges` | bool | no | Return inter-layer edges (default: true) |
| `top_k` | int | no | Max nodes per layer (default: 50) |

**Output:**
```json
{
  "level": 2,
  "name": "handlers",
  "nodes": [
    {"node": "file::chat_handler.py", "pagerank": 0.51, "outgoing_violations": 0}
  ],
  "health": {
    "layer_separation_score": 0.81,
    "violation_count": 1,
    "avg_internal_coupling": 0.23,
    "avg_external_coupling": 0.61
  },
  "edges_to_upper_layers": [
    {"from": "file::whatsapp_manager.py", "to": "file::chat_routes.py", "violation": true}
  ]
}
```

`layer_separation_score`: 1.0 = perfectly layered, 0.0 = spaghetti.

**Benchmark:** ~22ms

**Workflow:** `layers()` to detect violations → `layer_inspect(level=N)` to triage the offending layer.

---

## Surgical (4 tools)

Precision tools for reading and writing individual code nodes with full context awareness. v0.3.0 added `surgical_context_v2` and `apply_batch`.

---

### `m1nd_surgical_context`

Return complete context for a file in one call: full source, symbol table, callers, callees, and test coverage neighbours. Use before `m1nd_apply` for single-file edits.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `file_path` | string | yes | Absolute or workspace-relative path to the file |
| `symbol` | string | no | Narrow context to a specific symbol (function/struct/class name) |
| `radius` | int | no | BFS radius for graph neighbourhood (default: 1) |
| `include_tests` | bool | no | Include test files in neighbourhood (default: true) |

**Note:** The parameter is `file_path` (not `node_id`).

**Output:**

```json
{
  "file_path": "/abs/path/backend/chat_handler.py",
  "file_contents": "# full file source...",
  "line_count": 201,
  "node_id": "file::backend/chat_handler.py",
  "symbols": [
    {"name": "handle_message", "type": "function", "line_start": 42, "line_end": 89}
  ],
  "focused_symbol": null,
  "callers": [
    {"node_id": "file::backend/ws_relay.py", "label": "ws_relay.py", "file_path": "backend/ws_relay.py", "relation": "calls", "edge_weight": 0.81}
  ],
  "callees": [
    {"node_id": "file::backend/session_pool.py", "label": "session_pool.py", "file_path": "backend/session_pool.py", "relation": "calls", "edge_weight": 0.72}
  ],
  "tests": [
    {"node_id": "file::tests/test_chat.py", "label": "test_chat.py", "file_path": "tests/test_chat.py", "relation": "tests", "edge_weight": 0.9}
  ],
  "elapsed_ms": 12
}
```

---

### `m1nd_apply`

Write LLM-edited code back to a file and trigger incremental re-ingest so the graph stays coherent. Always call `m1nd_surgical_context` or `m1nd_surgical_context_v2` first to get the current file contents.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `file_path` | string | yes | Absolute or workspace-relative path of the file to overwrite |
| `new_content` | string | yes | New file contents (full replacement, UTF-8) |
| `description` | string | no | Human-readable description of the edit (logged) |
| `reingest` | bool | no | Re-ingest the file after writing (default: true) |

**Note:** The parameter is `new_content` (not `content`). Path can be absolute or workspace-relative.

**Output:**

```json
{
  "file_path": "/abs/path/to/file.py",
  "bytes_written": 4821,
  "lines_added": 12,
  "lines_removed": 3,
  "reingested": true,
  "updated_node_ids": ["file::backend/worker_pool.py", "file::backend/worker_pool.py::fn::submit"],
  "elapsed_ms": 145
}
```

**Example:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_apply","arguments":{
  "agent_id":"dev",
  "file_path":"/app/backend/worker_pool.py",
  "new_content":"# edited file contents...",
  "description":"Fix race condition in submit()",
  "reingest":true
}}}
```

---

### `m1nd_surgical_context_v2`

Superset of `m1nd_surgical_context` — returns the target file's full contents AND source excerpts of all connected files (callers, callees, tests) in one call. Eliminates the need to read multiple files separately. Use before `m1nd_apply_batch` when editing a file and its callers/tests together.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `file_path` | string | yes | Absolute or workspace-relative path to the target file |
| `symbol` | string | no | Narrow context to a specific symbol (function/struct/class name) |
| `radius` | int | no | BFS radius for graph neighbourhood (default: 1) |
| `include_tests` | bool | no | Include test files in the neighbourhood (default: true) |
| `max_connected_files` | int | no | Maximum connected files to include source for (default: 5) |
| `max_lines_per_file` | int | no | Maximum lines per connected file excerpt (default: 60) |

**Note:** The parameter is `file_path` (not `node_id`). The primary file is returned in full; connected files are capped at `max_lines_per_file`.

**Output:**

```json
{
  "file_path": "/abs/path/backend/worker_pool.py",
  "file_contents": "# full file source...",
  "line_count": 312,
  "node_id": "file::backend/worker_pool.py",
  "symbols": [
    {"name": "submit", "type": "function", "line_start": 42, "line_end": 67}
  ],
  "focused_symbol": null,
  "connected_files": [
    {
      "node_id": "file::backend/chat_handler.py",
      "label": "chat_handler.py",
      "file_path": "/abs/path/backend/chat_handler.py",
      "relation_type": "caller",
      "edge_weight": 0.81,
      "source_excerpt": "# first 60 lines...",
      "excerpt_lines": 60,
      "truncated": true
    }
  ],
  "total_lines": 372,
  "elapsed_ms": 18.4
}
```

`total_lines` = sum of all lines returned (primary file + all excerpts). Use to manage context budget.

**When to use:** Before any multi-file edit. One call replaces 3–6 separate file reads. The `connected_files` give you callers' context so you can update call sites in the same `apply_batch` call.

**Example:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_surgical_context_v2","arguments":{
  "agent_id":"dev",
  "file_path":"/app/backend/worker_pool.py",
  "max_connected_files":5,
  "max_lines_per_file":80
}}}
```

**HTTP bridge:**
```bash
curl -s http://localhost:1337/api/tools/m1nd_surgical_context_v2 \
  -H 'Content-Type: application/json' \
  -d '{"agent_id":"dev","file_path":"/app/backend/worker_pool.py"}'
```

---

### `m1nd_apply_batch`

Atomically write multiple files and trigger a single bulk re-ingest. All-or-nothing by default — if any file fails, all writes are rolled back. Optionally runs post-write verification (5-layer analysis: graph diff, anti-pattern detection, BFS blast radius, test execution, compile check). **12/12 accuracy validated.**

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `edits` | array | yes | List of `{file_path, new_content, description?}` objects |
| `atomic` | bool | no | Rollback all on partial failure (default: true) |
| `reingest` | bool | no | Re-ingest all modified files after writing (default: true) |
| `verify` | bool | no | Run post-write verification (default: false) |

**Edit object fields:** `file_path` (required), `new_content` (required — full replacement UTF-8), `description` (optional string for the apply log)

**Note:** The edit field is `new_content` (not `content`). Use `atomic=true` (default) to guarantee all-or-nothing.

**Output (verify=false):**

```json
{
  "all_succeeded": true,
  "files_written": 3,
  "files_total": 3,
  "results": [
    {
      "file_path": "/abs/path/worker_pool.py",
      "success": true,
      "diff": "@@ -42,6 +42,8 @@...",
      "lines_added": 8,
      "lines_removed": 2
    }
  ],
  "reingested": true,
  "total_bytes_written": 14632,
  "elapsed_ms": 287
}
```

**Output (verify=true) — adds `VerificationReport`:**

```json
{
  "all_succeeded": true,
  "files_written": 3,
  "results": [...],
  "verification": {
    "verdict": "SAFE",
    "high_impact_files": [],
    "antibodies_triggered": [],
    "layer_violations": [],
    "total_affected_nodes": 12,
    "blast_radius": [
      {"file_path": "worker_pool.py", "reachable_files": 3, "risk": "low", "top_affected": ["chat_handler.py"]}
    ],
    "tests_run": 47,
    "tests_passed": 47,
    "tests_failed": 0,
    "compile_check": "ok",
    "verify_elapsed_ms": 1840
  },
  "elapsed_ms": 2127
}
```

**Verdicts:** `SAFE` (no issues), `RISKY` (warnings or medium blast radius), `BROKEN` (compile failure or test failures).

**Verification layers:**
- Layer A: graph-diff (pre vs post node sets)
- Layer B: anti-pattern detection (todo!() removal, unwrap, error handling patterns)
- Layer C: real BFS blast radius per file (2-hop reachability via CSR edges)
- Layer D: affected test execution (cargo test / pytest / go test)
- Layer E: compilation check (first project type detected)

**Example — multi-file atomic edit:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_apply_batch","arguments":{
  "agent_id":"dev",
  "edits":[
    {"file_path":"/app/backend/worker_pool.py","new_content":"...","description":"Fix race condition"},
    {"file_path":"/app/backend/chat_handler.py","new_content":"...","description":"Update call site"}
  ],
  "atomic":true,
  "verify":true
}}}
```

**HTTP bridge:**
```bash
curl -s http://localhost:1337/api/tools/m1nd_apply_batch \
  -H 'Content-Type: application/json' \
  -d '{"agent_id":"dev","edits":[{"file_path":"/app/f.py","new_content":"..."}],"verify":true}'
```

---

## v0.5.0 — Search & Efficiency (5 tools)

New in v0.5.0: unified search across graph and source files, self-documenting help, panoramic risk analysis, and token economy tracking.

---

### `m1nd_search`

Unified literal/regex/semantic search — graph-aware grep replacement. Searches both node labels in the graph and file contents on disk, returning results with line context.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `query` | string | yes | Search term, regex pattern, or natural language query |
| `mode` | string | no | `"literal"` (default), `"regex"`, or `"semantic"` |
| `scope` | string | no | File path prefix to limit search scope |
| `top_k` | int | no | Maximum results (default: 50, max: 500) |
| `case_sensitive` | bool | no | Case-sensitive matching for literal/regex (default: false) |
| `context_lines` | int | no | Lines of context around each match (default: 2, max: 10) |

**Output:**

```json
{
  "query": "async def.*cancel",
  "mode": "regex",
  "results": [
    {
      "node_id": "file::backend/worker_pool.py",
      "label": "backend/worker_pool.py",
      "type": "FileContent",
      "file_path": "/abs/path/backend/worker_pool.py",
      "line_number": 142,
      "matched_line": "    async def cancel_task(self, task_id: str) -> bool:",
      "context_before": ["    # Cancellation handler", ""],
      "context_after": ["        \"\"\"Cancel a running task by ID.\"\"\"", "        if task_id not in self._tasks:"],
      "graph_linked": true
    }
  ],
  "total_matches": 7,
  "scope_applied": false,
  "elapsed_ms": 12.4
}
```

**Modes:**
- **`literal`** — exact substring match on node labels + file contents. Fastest. Use for identifiers, strings, known symbols.
- **`regex`** — linear-time regex (Rust `regex` crate) on node labels + file contents. Supports `(?i)` for case-insensitive. Safe against catastrophic backtracking.
- **`semantic`** — delegates to the seek engine (trigram TF-IDF + graph re-ranking). Use for natural language queries: "code that validates user credentials".

**When to use `search` vs other tools:**
- `search(mode="literal")` — fastest path to a known symbol or string
- `search(mode="regex")` — structural patterns across files (e.g. `async def.*cancel`)
- `search(mode="semantic")` → same as `seek` but via unified interface
- `activate` — spreading activation with 4D scoring; best for exploration, not exact lookup

**Example — literal:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_search","arguments":{
  "agent_id":"dev","query":"session_pool","mode":"literal","context_lines":3
}}}
```

**Example — regex:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_search","arguments":{
  "agent_id":"dev","query":"async def.*cancel","mode":"regex","scope":"backend/"
}}}
```

**Example — semantic:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_search","arguments":{
  "agent_id":"dev","query":"code that validates user credentials","mode":"semantic"
}}}
```

**HTTP bridge:**
```bash
curl -s http://localhost:1337/api/tools/m1nd_search \
  -H 'Content-Type: application/json' \
  -d '{"agent_id":"dev","query":"async def","mode":"regex","top_k":20}'
```

---

### `m1nd_help`

Self-documenting tool reference with m1nd's visual identity. Returns a formatted index of all tools or detailed docs for a specific tool — including params, examples, and NEXT suggestions.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `tool_name` | string | no | Tool name to look up (`"activate"`, `"m1nd_activate"`). Omit for full index. Pass `"about"` for visual identity banner. |

**Output:**

```json
{
  "formatted": "...(ANSI-colored terminal text)...",
  "tool": "m1nd_activate",
  "found": true,
  "suggestions": []
}
```

The `formatted` field contains ANSI box-drawing and color codes using m1nd's visual identity palette:

| Glyph | Unicode | Meaning |
|-------|---------|---------|
| `⍌` | U+234C | Spreading activation signal |
| `⍐` | U+2350 | Paths through the graph |
| `⍂` | U+2342 | Structural analysis |
| `𝔻` | U+1D53B | 4D dimensional scoring |
| `⟁` | U+27C1 | Graph connections, edges |

When `tool_name` is unknown, `found: false` and `suggestions` contains the closest matching tool names.

**Example — full index:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_help","arguments":{
  "agent_id":"dev"
}}}
```

**Example — specific tool:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_help","arguments":{
  "agent_id":"dev","tool_name":"hypothesize"
}}}
```

**Example — visual identity:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_help","arguments":{
  "agent_id":"dev","tool_name":"about"
}}}
```

---

### `m1nd_panoramic`

Full module risk panorama — ranked view of every file-level node by combined risk score. Combines blast radius, centrality, and churn into one sorted list. Critical modules (risk ≥ 0.7) trigger alerts.

**Risk formula:** `combined_risk = blast_normalized × 0.5 + centrality × 0.3 + churn × 0.2`

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |
| `scope` | string | no | File path prefix to limit scope |
| `top_n` | int | no | Maximum modules to return (default: 50, max: 1000) |

**Output:**

```json
{
  "modules": [
    {
      "node_id": "file::backend/chat_handler.py",
      "label": "backend/chat_handler.py",
      "file_path": "backend/chat_handler.py",
      "blast_forward": 48,
      "blast_backward": 12,
      "centrality": 0.74,
      "combined_risk": 0.83,
      "is_critical": true
    }
  ],
  "total_modules": 285,
  "critical_alerts": [
    {
      "node_id": "file::backend/chat_handler.py",
      "label": "backend/chat_handler.py",
      "combined_risk": 0.83,
      "reason": "high combined risk (0.83): blast_fwd=48, blast_bwd=12, centrality=0.74"
    }
  ],
  "scope_applied": false,
  "elapsed_ms": 18.7
}
```

**When to use:** Start every audit session with `panoramic()` to instantly identify the highest-risk modules without reading any files. Critical alerts = where to look first.

**Example:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_panoramic","arguments":{
  "agent_id":"dev","top_n":20
}}}
```

**HTTP bridge:**
```bash
curl -s http://localhost:1337/api/tools/m1nd_panoramic \
  -H 'Content-Type: application/json' \
  -d '{"agent_id":"dev","top_n":20}'
```

---

### `m1nd_savings`

Token economy report — session and global tracking of tokens saved by using m1nd instead of grep/glob, with CO2 estimation.

**Estimation model:** Every m1nd query replaces an average of ~500 tokens of grep output + LLM re-reading. CO2 rate: 0.0002g per avoided token. Cost rate: $0.003/1K tokens saved.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity |

**Output:**

```json
{
  "session_tokens_saved": 14200,
  "global_tokens_saved": 482000,
  "global_co2_grams": 96.4,
  "cost_saved_usd": 1.446,
  "recent_sessions": [
    {
      "agent_id": "dev",
      "session_start_ms": 1741987200000,
      "queries": 28,
      "tokens_saved": 14200,
      "co2_grams": 2.84
    }
  ],
  "formatted_summary": "...(ANSI-colored efficiency report)..."
}
```

The `formatted_summary` field displays a colored terminal report with session and global totals.

**Example:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_savings","arguments":{
  "agent_id":"dev"
}}}
```

---

### `m1nd_report`

Session summary — query log, timing statistics, tokens saved, and graph state for the calling agent. Provides a markdown-formatted report ready for display or logging.

**Inputs:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | string | yes | Caller identity (report is scoped to this agent) |

**Output:**

```json
{
  "agent_id": "dev",
  "session_queries": 34,
  "session_elapsed_ms": 1240.5,
  "queries_answered": 34,
  "tokens_saved_session": 17000,
  "tokens_saved_global": 499000,
  "co2_saved_grams": 99.8,
  "recent_queries": [
    {"tool": "m1nd_activate", "query": "session handling", "elapsed_ms": 45.2, "m1nd_answered": true},
    {"tool": "m1nd_hypothesize", "query": "session_pool leaks on cancel", "elapsed_ms": 58.1, "m1nd_answered": true}
  ],
  "markdown_summary": "## m1nd Session Report\n\n| Metric | Value |\n..."
}
```

The `markdown_summary` is pre-formatted for display in chat or documentation. The `recent_queries` field shows the last 10 tool calls for this agent.

**Example:**
```jsonc
{"method":"tools/call","params":{"name":"m1nd_report","arguments":{
  "agent_id":"dev"
}}}
```

---

## Benchmark Summary

**End-to-end (production, 335–380 files, ~52K lines):**

| Operation | Time |
|-----------|------|
| Full ingest (code) | 910ms–1.3s |
| Full ingest (docs/memory) | 138ms |
| Spreading activation | 31–77ms |
| Blast radius (forward) | 5–52ms |
| Stacktrace analysis | 3.5ms |
| Plan validation | 10ms |
| Counterfactual cascade | 3ms |
| Hypothesis testing | 28–58ms |
| Pattern scan (all 8) | 38ms |
| Antibody scan | <100ms |
| Multi-repo federation | 1.3s |
| Lock diff | 0.08µs |
| Trail merge | 1.2ms |

**Criterion micro-benchmarks (isolated, 1K–500-node graphs):**

| Benchmark | Time |
|-----------|------|
| `activate` 1K nodes | 1.36 µs |
| `impact` depth=3 | 543 ns |
| `graph build` 1K nodes | 528 µs |
| `flow_simulate` 4 particles | 552 µs |
| `epidemic` SIR 50 iterations | 110 µs |
| `antibody_scan` 50 patterns | 2.68 ms |
| `tremor` detect 500 nodes | 236 µs |
| `trust` report 500 nodes | 70 µs |
| `layer_detect` 500 nodes | 862 µs |
| `resonate` 5 harmonics | 8.17 µs |

---

## Related Pages

- [Home](Home) — overview, key numbers, common workflows
- [Getting Started](Getting-Started) — installation, Claude Code setup, config, adapters
- [EXAMPLES.md](../EXAMPLES.md) — raw output from production codebase runs
