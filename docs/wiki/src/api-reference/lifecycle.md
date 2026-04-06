# Lifecycle & Lock Tools

Eight tools for graph ingestion, health monitoring, plan validation, and subgraph locking with change detection.

---

## `m1nd.ingest`

Ingest or re-ingest a codebase, descriptor, or memory corpus into the graph. This is the primary way to load data into m1nd. Supports three adapters (`code`, `json`, `memory`) and two modes (`replace`, `merge`).

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `path` | `string` | Yes | -- | Filesystem path to the source root or memory corpus. |
| `agent_id` | `string` | Yes | -- | Calling agent identifier. |
| `incremental` | `boolean` | No | `false` | Incremental ingest (code adapter only). Only re-processes files that changed since the last ingest. |
| `adapter` | `string` | No | `"code"` | Adapter to use for parsing. Values: `"code"` (source code -- Python, Rust, TypeScript, etc.), `"json"` (graph snapshot JSON), `"memory"` (markdown memory corpus). |
| `mode` | `string` | No | `"replace"` | How to handle the existing graph. Values: `"replace"` (clear and rebuild), `"merge"` (add new nodes/edges into existing graph). |
| `namespace` | `string` | No | -- | Optional namespace tag for non-code nodes. Used by `memory` and `json` adapters to prefix node external_ids. |

### Example Request

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "m1nd.ingest",
    "arguments": {
      "agent_id": "jimi",
      "path": "/Users/cosmophonix/clawd/roomanizer-os/backend",
      "adapter": "code",
      "mode": "replace",
      "incremental": false
    }
  }
}
```

### Example Response

```json
{
  "files_processed": 335,
  "nodes_created": 9767,
  "edges_created": 26557,
  "languages": { "python": 335 },
  "elapsed_ms": 910.0
}
```

### Adapters

| Adapter | Input | Node Types | Edge Types |
|---------|-------|------------|------------|
| `code` | Source code directory | file, class, function, struct, module | imports, calls, registers, configures, tests, inherits |
| `json` | Graph snapshot JSON | (preserved from snapshot) | (preserved from snapshot) |
| `memory` | Markdown files | document, concept, entity | references, relates_to |

### Mode Behavior

| Mode | Behavior |
|------|----------|
| `replace` | Clears the existing graph, ingests fresh, finalizes (PageRank + CSR). All perspectives and locks are invalidated. |
| `merge` | Adds new nodes and edges into the existing graph. Existing nodes are updated if they share the same external_id. Graph is re-finalized after merge. |

### When to Use

- **Session start** -- ingest the codebase if the graph is empty or stale
- **After code changes** -- re-ingest incrementally to update the graph
- **Multi-source** -- merge a memory corpus into a code graph for cross-domain queries
- **Federation preparation** -- use `m1nd.federate` instead for multi-repo ingestion

### Side Effects

- **replace mode**: clears all graph state, invalidates all perspectives and locks, marks lock baselines as stale
- **merge mode**: adds to graph, increments graph generation, triggers watcher events on affected locks

### Related Tools

- [`m1nd.health`](#m1ndhealth) -- check graph status before deciding to ingest
- [`m1nd.drift`](memory.md#m1nddrift) -- see what changed since last session
- [`m1nd.federate`](exploration.md#m1ndfederate) -- multi-repo ingestion

---

## `m1nd.health`

Server health and statistics. Returns node/edge counts, query count, uptime, memory usage, plasticity state, and active sessions.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `agent_id` | `string` | Yes | -- | Calling agent identifier. |

### Example Request

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "m1nd.health",
    "arguments": {
      "agent_id": "jimi"
    }
  }
}
```

### Example Response

```json
{
  "status": "healthy",
  "node_count": 9767,
  "edge_count": 26557,
  "queries_processed": 142,
  "uptime_seconds": 3600.5,
  "memory_usage_bytes": 52428800,
  "plasticity_state": "active",
  "last_persist_time": "2026-03-13T10:30:00Z",
  "active_sessions": [
    { "agent_id": "jimi", "last_active": "2026-03-13T11:25:00Z" }
  ]
}
```

### When to Use

- **Session start** -- first tool to call; verify the server is alive and the graph is loaded
- **Monitoring** -- periodic health checks in long sessions
- **Debugging** -- check memory usage and query counts

### Related Tools

- [`m1nd.ingest`](#m1ndingest) -- load data if the graph is empty
- [`m1nd.drift`](memory.md#m1nddrift) -- check what changed since last session

---

## `m1nd.validate_plan`

Validate a proposed modification plan against the code graph. Detects gaps (affected files missing from the plan), risk level, test coverage, and suggested additions. Designed to be called before implementing a plan.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `agent_id` | `string` | Yes | -- | Calling agent identifier. |
| `actions` | `object[]` | Yes | -- | Ordered list of planned actions. Each object has: `action_type` (string, required -- `"modify"`, `"create"`, `"delete"`, `"rename"`, `"test"`), `file_path` (string, required -- relative path), `description` (string, optional), `depends_on` (string[], optional -- other file_paths this action depends on). |
| `include_test_impact` | `boolean` | No | `true` | Analyze test coverage for modified files. |
| `include_risk_score` | `boolean` | No | `true` | Compute composite risk score. |

### Example Request

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "m1nd.validate_plan",
    "arguments": {
      "agent_id": "jimi",
      "actions": [
        { "action_type": "modify", "file_path": "backend/session_pool.py", "description": "Add connection timeout" },
        { "action_type": "modify", "file_path": "backend/worker_pool.py", "description": "Update pool acquire to use timeout" },
        { "action_type": "test", "file_path": "backend/tests/test_session_pool.py" }
      ]
    }
  }
}
```

### Example Response

```json
{
  "actions_analyzed": 3,
  "actions_resolved": 3,
  "actions_unresolved": 0,
  "gaps": [
    {
      "file_path": "backend/config.py",
      "node_id": "file::config.py",
      "reason": "imported by modified file session_pool.py -- timeout config likely needed here",
      "severity": "warning",
      "signal_strength": 0.72
    },
    {
      "file_path": "backend/process_manager.py",
      "node_id": "file::process_manager.py",
      "reason": "in blast radius of worker_pool.py -- calls worker_pool.submit",
      "severity": "info",
      "signal_strength": 0.45
    }
  ],
  "risk_score": 0.52,
  "risk_level": "medium",
  "test_coverage": {
    "modified_files": 2,
    "tested_files": 1,
    "untested_files": ["backend/worker_pool.py"],
    "coverage_ratio": 0.5
  },
  "suggested_additions": [
    { "action_type": "modify", "file_path": "backend/config.py", "reason": "Timeout configuration likely needed" },
    { "action_type": "test", "file_path": "backend/tests/test_worker_pool.py", "reason": "Modified file has no test action in plan" }
  ],
  "blast_radius_total": 47,
  "elapsed_ms": 35.0
}
```

### Risk Levels

| Level | Score Range | Meaning |
|-------|------------|---------|
| `"low"` | < 0.3 | Small, well-tested change |
| `"medium"` | 0.3 -- 0.6 | Moderate scope, some gaps |
| `"high"` | 0.6 -- 0.8 | Large scope or missing tests |
| `"critical"` | >= 0.8 | Very high risk -- review carefully |

### When to Use

- **Before implementing** -- validate your plan catches all affected files
- **PR review** -- validate that a PR's changes are complete
- **Planning** -- estimate risk and scope before committing to a plan
- **Quality gate** -- reject plans with risk_score > threshold

### Related Tools

- [`m1nd.impact`](analysis.md#m1ndimpact) -- blast radius for a single node
- [`m1nd.predict`](analysis.md#m1ndpredict) -- co-change prediction for a single node
- [`m1nd.trace`](exploration.md#m1ndtrace) -- validate a fix plan for a specific error

---

## `m1nd.lock.create`

Pin a subgraph region and capture a baseline snapshot for change monitoring. Locks are used to track what changes in a region of the graph while you work. The baseline is compared against the current state when you call `lock.diff`.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `agent_id` | `string` | Yes | -- | Calling agent identifier. Lock is owned by this agent. |
| `scope` | `string` | Yes | -- | Scope type. Values: `"node"` (single nodes only), `"subgraph"` (BFS expansion from roots), `"query_neighborhood"` (nodes matching a query), `"path"` (ordered node list). |
| `root_nodes` | `string[]` | Yes | -- | Root nodes for the lock scope. Non-empty. Matched by external_id (exact), then label, then substring. |
| `radius` | `integer` | No | -- | BFS radius for `subgraph` scope. Range: 1 to 4. Required for subgraph scope. |
| `query` | `string` | No | -- | Query string for `query_neighborhood` scope. |
| `path_nodes` | `string[]` | No | -- | Ordered node list for `path` scope. |

### Scope Types

| Scope | Description |
|-------|-------------|
| `node` | Lock only the specified root nodes |
| `subgraph` | BFS expansion from root nodes up to `radius` hops |
| `query_neighborhood` | Nodes matching a query activation |
| `path` | An ordered list of nodes forming a path |

### Example Request

```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "tools/call",
  "params": {
    "name": "m1nd.lock.create",
    "arguments": {
      "agent_id": "jimi",
      "scope": "subgraph",
      "root_nodes": ["file::chat_handler.py"],
      "radius": 2
    }
  }
}
```

### Example Response

```json
{
  "lock_id": "lock_jimi_001",
  "scope": "subgraph",
  "baseline_nodes": 1639,
  "baseline_edges": 707,
  "graph_generation": 42,
  "created_at_ms": 1710300000000
}
```

### Limits

- Max locks per agent: configurable (default: 10)
- Max baseline nodes: configurable (default: 5000)
- Max baseline edges: configurable (default: 10000)
- Total memory budget shared with perspectives

### When to Use

- **Change monitoring** -- lock a region before making changes, then diff to see what changed
- **Multi-agent coordination** -- lock regions to detect when other agents' changes affect your work
- **Regression detection** -- lock a stable region and watch for unexpected changes

### Related Tools

- [`m1nd.lock.watch`](#m1ndlockwatch) -- set automatic change detection
- [`m1nd.lock.diff`](#m1ndlockdiff) -- compute what changed since baseline
- [`m1nd.lock.release`](#m1ndlockrelease) -- release the lock when done

---

## `m1nd.lock.watch`

Set a watcher strategy on a lock. Watchers determine when the lock automatically detects changes. Without a watcher, you must manually call `lock.diff`.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `agent_id` | `string` | Yes | -- | Calling agent identifier. Must own the lock. |
| `lock_id` | `string` | Yes | -- | Lock to set the watcher on. |
| `strategy` | `string` | Yes | -- | Watcher strategy. Values: `"manual"` (no automatic detection), `"on_ingest"` (detect after every ingest), `"on_learn"` (detect after every learn call). Note: `"periodic"` is not supported in V1. |

### Example Request

```json
{
  "jsonrpc": "2.0",
  "id": 5,
  "method": "tools/call",
  "params": {
    "name": "m1nd.lock.watch",
    "arguments": {
      "agent_id": "jimi",
      "lock_id": "lock_jimi_001",
      "strategy": "on_ingest"
    }
  }
}
```

### Example Response

```json
{
  "lock_id": "lock_jimi_001",
  "strategy": "on_ingest",
  "previous_strategy": null
}
```

### When to Use

- **Automatic monitoring** -- set `on_ingest` to detect changes after every code re-ingest
- **Learning feedback** -- set `on_learn` to detect when learning shifts edge weights in your region
- **Manual control** -- set `manual` to disable automatic detection

### Related Tools

- [`m1nd.lock.diff`](#m1ndlockdiff) -- manually trigger a diff (always available regardless of strategy)
- [`m1nd.lock.create`](#m1ndlockcreate) -- create the lock first

---

## `m1nd.lock.diff`

Compute what changed in a locked region since the baseline was captured. Returns new/removed nodes, new/removed edges, weight changes, and watcher event counts. Fast-path: if the graph generation has not changed, returns immediately with `no_changes: true`.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `agent_id` | `string` | Yes | -- | Calling agent identifier. Must own the lock. |
| `lock_id` | `string` | Yes | -- | Lock to diff. |

### Example Request

```json
{
  "jsonrpc": "2.0",
  "id": 6,
  "method": "tools/call",
  "params": {
    "name": "m1nd.lock.diff",
    "arguments": {
      "agent_id": "jimi",
      "lock_id": "lock_jimi_001"
    }
  }
}
```

### Example Response (changes detected)

```json
{
  "diff": {
    "lock_id": "lock_jimi_001",
    "no_changes": false,
    "new_nodes": ["file::chat_handler.py::fn::new_method"],
    "removed_nodes": [],
    "new_edges": ["chat_handler.py|new_method->stream_parser.py|calls"],
    "removed_edges": [],
    "boundary_edges_added": [],
    "boundary_edges_removed": [],
    "weight_changes": [
      { "edge_key": "chat_handler.py|stream_parser.py|imports", "old_weight": 0.5, "new_weight": 0.72 }
    ],
    "baseline_stale": false,
    "elapsed_ms": 0.08
  },
  "watcher_events_drained": 2,
  "rebase_suggested": null
}
```

### Example Response (no changes)

```json
{
  "diff": {
    "lock_id": "lock_jimi_001",
    "no_changes": true,
    "new_nodes": [],
    "removed_nodes": [],
    "new_edges": [],
    "removed_edges": [],
    "boundary_edges_added": [],
    "boundary_edges_removed": [],
    "weight_changes": [],
    "baseline_stale": false,
    "elapsed_ms": 0.001
  },
  "watcher_events_drained": 0,
  "rebase_suggested": null
}
```

### When to Use

- **After ingest** -- check if your locked region was affected
- **After learning** -- check if feedback shifted weights in your region
- **Periodic check** -- poll for changes during long sessions
- **Before committing** -- verify no unexpected changes in the region

### Related Tools

- [`m1nd.lock.rebase`](#m1ndlockrebase) -- re-capture baseline after acknowledging changes
- [`m1nd.lock.watch`](#m1ndlockwatch) -- set automatic change detection

---

## `m1nd.lock.rebase`

Re-capture the lock baseline from the current graph without releasing the lock. Use this after calling `lock.diff` and acknowledging the changes -- the new baseline becomes the reference for future diffs.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `agent_id` | `string` | Yes | -- | Calling agent identifier. Must own the lock. |
| `lock_id` | `string` | Yes | -- | Lock to rebase. |

### Example Request

```json
{
  "jsonrpc": "2.0",
  "id": 7,
  "method": "tools/call",
  "params": {
    "name": "m1nd.lock.rebase",
    "arguments": {
      "agent_id": "jimi",
      "lock_id": "lock_jimi_001"
    }
  }
}
```

### Example Response

```json
{
  "lock_id": "lock_jimi_001",
  "previous_generation": 42,
  "new_generation": 45,
  "baseline_nodes": 1645,
  "baseline_edges": 712,
  "watcher_preserved": true
}
```

### When to Use

- **After acknowledging changes** -- rebase after reviewing a diff to reset the baseline
- **After stale warning** -- when `lock.diff` returns `baseline_stale: true`, rebase to fix it
- **Periodic refresh** -- rebase periodically in long sessions to keep baselines current

### Related Tools

- [`m1nd.lock.diff`](#m1ndlockdiff) -- the diff that triggers a rebase
- [`m1nd.lock.create`](#m1ndlockcreate) -- creating a new lock is an alternative to rebasing

---

## `m1nd.lock.release`

Release a lock and free its resources. Removes the lock state, cleans up pending watcher events, and frees memory. Irreversible.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `agent_id` | `string` | Yes | -- | Calling agent identifier. Must own the lock. |
| `lock_id` | `string` | Yes | -- | Lock to release. |

### Example Request

```json
{
  "jsonrpc": "2.0",
  "id": 8,
  "method": "tools/call",
  "params": {
    "name": "m1nd.lock.release",
    "arguments": {
      "agent_id": "jimi",
      "lock_id": "lock_jimi_001"
    }
  }
}
```

### Example Response

```json
{
  "lock_id": "lock_jimi_001",
  "released": true
}
```

### When to Use

- **Done monitoring** -- release when you no longer need change detection
- **Memory pressure** -- locks consume memory proportional to baseline size
- **Session end** -- release all locks before ending a session
- **Automatic**: locks are also cascade-released when their associated perspective is closed via `perspective.close`

### Related Tools

- [`m1nd.lock.create`](#m1ndlockcreate) -- create a new lock
- [`m1nd.perspective.close`](perspectives.md#m1ndperspectiveclose) -- cascade-releases associated locks

---

## `m1nd.daemon_start`

Start the persisted daemon control plane. Stores watched roots, initializes daemon counters, and begins the long-lived structural monitoring lane.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `agent_id` | `string` | Yes | -- | Calling agent identifier. |
| `watch_paths` | `string[]` | No | current ingest roots | Paths the daemon should monitor. |
| `poll_interval_ms` | `integer` | No | `500` | Poll interval fallback in milliseconds. |

### When to Use

- Start of a long-lived agent session
- Before relying on daemon alerts or `daemon_tick`
- Before background/idle reconciliation should run

### Related Tools

- [`m1nd.daemon_status`](#m1nddaemon_status)
- [`m1nd.daemon_tick`](#m1nddaemon_tick)
- [`m1nd.alerts_list`](#m1ndalerts_list)

---

## `m1nd.daemon_stop`

Stop the daemon control plane without deleting persisted alert history.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | `string` | Yes | Calling agent identifier. |

### When to Use

- End of a daemon-backed session
- Before shutting down a host that should not keep reconciling

### Related Tools

- [`m1nd.daemon_start`](#m1nddaemon_start)
- [`m1nd.daemon_status`](#m1nddaemon_status)

---

## `m1nd.daemon_status`

Inspect daemon liveness and runtime counters. Returns watched roots, tracked files, recent tick metrics, and alert counts.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | `string` | Yes | Calling agent identifier. |

### Typical Output Fields

- `active`
- `watch_paths`
- `poll_interval_ms`
- `tracked_files`
- `tick_count`
- `last_tick_duration_ms`
- `last_tick_changed_files`
- `last_tick_deleted_files`
- `last_tick_alerts_emitted`
- `alert_count`

### When to Use

- To verify daemon startup worked
- To inspect whether reconciliation is actually happening
- To debug daemon slowness or alert silence

### Related Tools

- [`m1nd.daemon_start`](#m1nddaemon_start)
- [`m1nd.daemon_tick`](#m1nddaemon_tick)
- [`m1nd.alerts_list`](#m1ndalerts_list)

---

## `m1nd.daemon_tick`

Run one explicit daemon reconciliation pass. Polls watched roots, re-ingests changed files, detects deletions, and emits drift alerts.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `agent_id` | `string` | Yes | -- | Calling agent identifier. |
| `max_files` | `integer` | No | `32` | Maximum changed files to process in one tick. |

### Typical Output Fields

- `changed_files_detected`
- `deleted_files_detected`
- `files_reingested`
- `ingested_files[]`
- `alerts_emitted`
- `alert_ids[]`
- `tick_at_ms`

### When to Use

- To force one reconciliation before reading daemon status
- To debug watched-root drift deterministically
- To reproduce daemon ingest issues outside background ticking

### Related Tools

- [`m1nd.daemon_status`](#m1nddaemon_status)
- [`m1nd.alerts_list`](#m1ndalerts_list)
- [`m1nd.cross_verify`](../api-reference/exploration.md#m1ndcross_verify)

---

## `m1nd.alerts_list`

List persisted daemon and proactive alerts.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `agent_id` | `string` | Yes | -- | Calling agent identifier. |
| `include_acked` | `boolean` | No | `false` | Include already acknowledged alerts. |
| `limit` | `integer` | No | `50` | Maximum alerts to return. |

### When to Use

- Reviewing daemon findings after a session
- Building an alert inbox for an agent or UI

### Related Tools

- [`m1nd.alerts_ack`](#m1ndalerts_ack)
- [`m1nd.daemon_status`](#m1nddaemon_status)

---

## `m1nd.alerts_ack`

Acknowledge one or more persisted daemon/proactive alerts so they stop resurfacing in the unread queue.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | `string` | Yes | Calling agent identifier. |
| `alert_ids` | `string[]` | Yes | Alert IDs to acknowledge. |

### When to Use

- After reviewing or actioning daemon findings
- To keep the alert queue focused on new drift

### Related Tools

- [`m1nd.alerts_list`](#m1ndalerts_list)

---

## `m1nd.edit_preview`

Preview a full-file write without touching disk. Returns a diff, freshness snapshot, and validation report so the caller can inspect before committing.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | `string` | Yes | Calling agent identifier. |
| `file_path` | `string` | Yes | Absolute or workspace-relative file path. |
| `new_content` | `string` | Yes | Candidate replacement content. |
| `description` | `string` | No | Human-readable summary of the edit. |

### When to Use

- Before risky writes
- When you want a two-phase edit protocol
- When a human or another agent should inspect the diff first

### Related Tools

- [`m1nd.edit_commit`](#m1ndedit_commit)
- [`m1nd.apply`](../api-reference/lifecycle.md)

---

## `m1nd.edit_commit`

Commit a previously previewed edit after freshness re-check and explicit confirmation.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | `string` | Yes | Calling agent identifier. |
| `preview_id` | `string` | Yes | Preview handle returned by `edit_preview`. |
| `confirm` | `boolean` | Yes | Must be `true` to commit the preview. |
| `reingest` | `boolean` | No | Re-ingest the modified file after commit. |

### When to Use

- After a human/agent approves an `edit_preview`
- When stale-source protection matters more than speed

### Related Tools

- [`m1nd.edit_preview`](#m1ndedit_preview)
- [`m1nd.apply`](../api-reference/lifecycle.md)

---

## `m1nd.persist`

Force graph and sidecar persistence immediately.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | `string` | Yes | Calling agent identifier. |
| `action` | `string` | Yes | Persistence action, such as save/load semantics supported by the current implementation. |

### When to Use

- Before shutdown
- Before risky host lifecycle transitions
- When you want an explicit persistence checkpoint

### Related Tools

- [`m1nd.health`](#m1ndhealth)
- [`m1nd.boot_memory`](#m1ndboot_memory)

---

## `m1nd.boot_memory`

Persist small canonical hot-state values next to the graph without polluting larger investigation trails.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | `string` | Yes | Calling agent identifier. |
| `action` | `string` | Yes | Memory action (`set`, `get`, `list`, `delete`, etc.). |
| `key` | `string` | No | Canonical key to address. |
| `value` | `json` | No | JSON value to store. |

### When to Use

- Short doctrine/state values that should stay hot
- Session bootstrapping facts an agent should retrieve quickly

### Related Tools

- [`m1nd.persist`](#m1ndpersist)
- [`m1nd.trail_save`](memory.md#m1ndtrailsave)

---

## `m1nd.heuristics_surface`

Explain why a node or file is currently ranked as risky or important. Surfaces trust/tremor/antibody/blast-style heuristic factors in one payload.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | `string` | Yes | Calling agent identifier. |
| `node_id` | `string` | No | Graph node to inspect. |
| `file_path` | `string` | No | File path to inspect. |

### When to Use

- After `predict`, `validate_plan`, or surgical flows rank something unexpectedly high
- When an agent needs explainability before editing or escalating

### Related Tools

- [`m1nd.validate_plan`](#m1ndvalidate_plan)
- [`m1nd.apply_batch`](../api-reference/lifecycle.md)
- [`m1nd.daemon_tick`](#m1nddaemon_tick)

---

## `m1nd.audit`

Profile-aware one-call audit over topology, scans, verification, filesystem truth, and git state.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | `string` | Yes | Calling agent identifier. |
| `path` | `string` | Yes | Root path to audit. |
| `profile` | `string` | No | Audit profile such as `auto`, `quick`, `coordination`, or `production`. |
| `depth` | `string` | No | Audit depth. |
| `cross_verify` | `boolean` | No | Include graph-vs-disk verification. |
| `external_refs` | `boolean` | No | Include explicit external reference discovery. |

### When to Use

- First pass on an unfamiliar repo
- Long-running session orientation
- Pre-handoff or pre-merge structural review

### Related Tools

- [`m1nd.batch_view`](../api-reference/exploration.md#m1ndbatch_view)
- [`m1nd.cross_verify`](../api-reference/exploration.md#m1ndcross_verify)
- [`m1nd.coverage_session`](../api-reference/exploration.md#m1ndcoverage_session)

