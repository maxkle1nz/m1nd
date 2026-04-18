# Multi-Instance Command Center for `m1nd`

Date: `2026-04-11`

## Goal

Allow `m1nd` to run across many projects at once without state collisions, while giving the user one clear UI surface to:

- see every running instance
- understand when there is a conflict
- open or explore an instance
- persist state
- restart an instance
- stop an instance
- delete an instance state directory

This should work whether the user has:

- one project and one instance
- many unrelated projects
- multiple worktrees from the same repository
- duplicate launches of the same project

---

## Current state, grounded in code

The current `m1nd` runtime is already close to the right substrate, but it is still organized as one session state per process.

### 1. One `SessionState` per running server process

`m1nd-mcp/src/session.rs`

- `SessionState` is explicitly described as:
  - "Server session state. Owns the graph and all engine instances."
  - "Single instance shared across all agent connections."
- it owns:
  - `graph`
  - `orchestrator`
  - `temporal`
  - `plasticity`
  - perspective/lock state
  - boot memory
  - daemon state
  - daemon alerts
  - auto-ingest state
  - document cache

This means:

- many agents can share one running `m1nd`
- but one `m1nd` process still corresponds to one active runtime state

### 2. HTTP handlers share one session through `Arc<Mutex<SessionState>>`

`m1nd-mcp/src/http_server.rs`

- `AppState` contains:
  - `session: Arc<Mutex<SessionState>>`
- `spawn_background()` and `run()` both create an `Arc<Mutex<SessionState>>`
- all `/api/health`, `/api/graph/stats`, `/api/graph/snapshot`, and tool routes operate on that single session

This means:

- each HTTP surface is currently an instance-local UI
- there is no built-in global registry of other `m1nd` processes

### 3. The runtime already persists a rich state set under `runtime_root`

`m1nd-mcp/src/session.rs`

`SessionState` persists or loads:

- `graph_path`
- `plasticity_path`
- `runtime_root`
- `boot_memory_path`
- `daemon_state_path`
- `daemon_alerts_path`
- auto-ingest state
- document cache
- ingest roots

and `persist()` writes them in a defined order.

This is the key enabling fact:

> `m1nd` already has a strong per-runtime state model.

What it lacks is:

- a stable registry above those runtimes
- ownership / lease rules for shared state dirs
- a user-facing multi-instance control plane

### 4. Server startup still assumes one config -> one runtime

`m1nd-mcp/src/server.rs`

- `McpServer::new(config)` loads:
  - `graph_source`
  - `plasticity_state`
- then initializes `SessionState`
- then `into_session_state()` passes that single runtime into HTTP / stdio

So the configuration model is already instance-shaped, but not registry-shaped.

---

## Product decision

`m1nd` should support **many isolated instances** and **one user-visible command center**.

The correct mental model is:

- **Instance**: one running `m1nd` process with one isolated `SessionState`
- **Workspace**: the project or worktree that instance is serving
- **State directory**: the persisted graph/plasticity/runtime sidecars owned by that instance
- **Command Center**: the UI that aggregates and controls all instances

---

## Recommendation

Build this in two layers:

### Layer 1: isolated runtime instances

Every `m1nd` process should have:

- a stable `instance_id`
- a canonical `workspace_root`
- a canonical `runtime_root`
- a conflict-safe ownership lease over that `runtime_root`

### Layer 2: global instance registry + command center

All running instances should self-register into a single global registry.

Any one `m1nd` HTTP UI can then render:

- all known instances
- their health
- their conflicts
- their actions

without requiring one giant monolithic shared `SessionState`.

This is the most robust path because it preserves what already works:

- per-process `SessionState`
- per-runtime persistence
- local-first operation

while adding the missing coordination layer.

---

## Instance model

Each instance gets a durable identity record:

```json
{
  "instance_id": "inst_01JXYZ...",
  "workspace_root": "/abs/path/to/project",
  "git_root": "/abs/path/to/repo",
  "worktree_root": "/abs/path/to/worktree",
  "runtime_root": "/abs/path/to/runtime",
  "graph_source": "/abs/path/to/runtime/graph.json",
  "plasticity_state": "/abs/path/to/runtime/plasticity.json",
  "bind": "127.0.0.1",
  "port": 3737,
  "pid": 12345,
  "started_at_ms": 0,
  "last_heartbeat_ms": 0,
  "mode": "read_write",
  "status": "running"
}
```

### Identity rules

Use:

- `workspace_root` as the primary logical identity for the project being served
- `runtime_root` as the primary state identity
- `instance_id` as the process identity

### Why this matters

Two launches can be:

- the same workspace and same runtime root
- the same workspace and different runtime roots
- different worktrees under the same git root
- different projects entirely

Those cases must not be treated the same.

---

## Conflict model

We should explicitly classify conflicts.

### No conflict

- different `workspace_root`
- different `runtime_root`

### Soft duplication

- same `workspace_root`
- different `runtime_root`

Meaning:

- technically safe
- but likely redundant
- UI should warn: "duplicate workspace"

### Hard state conflict

- same `runtime_root`
- two live processes both trying to own it as read-write

Meaning:

- not safe
- only one process may hold the write lease

### Port conflict

- requested port already occupied

Meaning:

- recoverable
- allocate a new port and register it

### Stale lock

- lock file exists
- pid is gone or heartbeat is stale

Meaning:

- UI should offer reclaim / restart / clear stale lock

---

## Runtime ownership rules

Every `runtime_root` should have an ownership lease record, but in Phase 1 that
lease should live in the **global registry**, not inside the repo/runtime
directory itself.

Example Phase 1 lease path:

`~/.m1nd/registry/leases/<runtime-root-fingerprint>.json`

Contents:

```json
{
  "instance_id": "inst_...",
  "pid": 12345,
  "hostname": "machine",
  "workspace_root": "/abs/project",
  "runtime_root": "/abs/runtime",
  "mode": "read_write",
  "started_at_ms": 0,
  "last_heartbeat_ms": 0
}
```

### Lease behavior

On startup:

1. if no lock exists -> claim it
2. if a healthy live owner exists:
   - if same workspace + same runtime root:
     - default to attach/open existing instance in UI
   - else:
     - block write ownership
3. if stale owner:
   - reclaim

### Modes

- `read_write`
- `read_only`
- `stale`

Read-only should be allowed for diagnostics or duplicate observers, but only one writer may own the state dir.

---

## Default path strategy

The current config already supports custom `graph_source`, `plasticity_state`, and `runtime_root`.

To make many instances safe by default:

### Current problem

If the user points multiple launches at the same generic temp paths, collisions are easy.

### New default

Generate runtime roots under a namespaced home:

```text
~/.m1nd/instances/<workspace_fingerprint>/
```

Inside:

```text
graph.json
plasticity.json
boot_memory_state.json
daemon_state.json
daemon_alerts.json
document_cache.json
ingest_roots.json
```

### Workspace fingerprint

Use a stable digest of:

- canonical worktree root
- git root
- maybe branch/worktree label when present

Important:

Two worktrees from the same git root should default to **different** instance fingerprints if their filesystem roots differ.

That avoids cross-worktree contamination.

---

## Global registry

Add a global registry, for example:

```text
~/.m1nd/registry/instances/
  inst_abc.json
  inst_def.json
```

Each running instance writes one heartbeat-updated file.

Why directory-per-instance instead of one giant JSON file:

- simpler atomicity
- easier stale cleanup
- less lock contention
- better crash recovery

### Registry entry fields

- identity
- workspace info
- runtime paths
- port / bind
- pid
- status
- heartbeat
- graph counts
- active agent sessions
- conflict flags
- current alerts count

### Registry refresh cadence

- heartbeat every `2s` or `5s`
- on state transitions:
  - startup
  - shutdown
  - persist
  - conflict
  - restart

---

## Command center UI

The UI should show all instances in one place.

### Core views

### 1. Instance list

Card per instance:

- project name
- workspace path
- git branch / worktree
- status
- pid
- port
- last heartbeat
- node count / edge count
- alert count
- active agent session count

### 2. Conflict panel

Badges:

- `shared runtime root`
- `duplicate workspace`
- `stale lock`
- `port reassigned`

### 3. Instance detail drawer

Show:

- runtime paths
- graph source
- plasticity state
- daemon state
- ingest roots
- persistence timestamps
- active sessions

### 4. Actions row

Per instance:

- `Open`
- `Explore`
- `Save state`
- `Restart`
- `Stop`
- `Delete state`
- `Reveal runtime dir`

---

## UX behavior

### Open

Open the instance-local UI in browser.

### Explore

Jump into graph stats / subgraph / health of that instance.

### Save state

Call the existing persist path explicitly.

This should serialize:

- graph
- plasticity
- boot memory
- daemon state
- alerts
- auto-ingest
- document cache

### Restart

Needs lifecycle management.

Short-term:

- graceful stop + relaunch if started by supervisor

Long-term:

- managed by a dedicated launcher/daemon

### Stop

Graceful process stop and registry removal.

### Delete state

Delete the `runtime_root` only when:

- process is stopped
- or force confirmed

### Conflict recovery

Buttons:

- `Attach to running instance`
- `Clone state into new runtime root`
- `Reclaim stale lock`

---

## API proposal

Extend the existing HTTP surface with:

### Self endpoints

- `/api/instance/self`
- `/api/instance/save`
- `/api/instance/stop`
- `/api/instance/restart`

### Registry endpoints

- `/api/instances`
- `/api/instances/:id`
- `/api/instances/:id/conflicts`
- `/api/instances/:id/delete-state`

### Why split self vs registry

Because one instance should be able to:

- describe itself locally
- while the command center aggregates many instances globally

---

## Process model recommendation

There are two viable ways to control restart/stop.

### Option A: self-registering instances only

Pros:

- simpler
- lowest implementation cost

Cons:

- restart/stop of another process is awkward
- cross-process control becomes platform-sensitive

### Option B: add a lightweight supervisor (`m1ndd`)

Pros:

- clean lifecycle management
- easier restart/stop/delete-state
- central place for registry truth

Cons:

- more moving parts

### Recommendation

Phase it:

1. ship self-registering instances + read-only command center
2. add supervisor-backed control actions after the registry stabilizes

That gets us value quickly without overbuilding the first version.

---

## Suggested implementation plan

### Phase 1: isolate instances safely

Files:

- `m1nd-mcp/src/session.rs`
- `m1nd-mcp/src/server.rs`
- `m1nd-mcp/src/config.rs` or equivalent config surface
- `m1nd-mcp/src/instance_registry.rs` (new)

Deliver:

- instance identity
- runtime-root ownership lease (stored in the global registry in Phase 1)
- namespaced default runtime roots
- registry heartbeat files

### Phase 2: expose command center API

Files:

- `m1nd-mcp/src/http_server.rs`
- `m1nd-mcp/src/instance_registry.rs`

Deliver:

- list instances
- inspect instance
- conflict diagnostics
- save-state action

### Phase 3: UI

Files:

- existing embedded UI surface in `m1nd-mcp`
- or dedicated React page in `m1nd-demo` for product simulation first

Deliver:

- instance table/cards
- conflict panel
- actions row
- detail drawer

### Phase 4: lifecycle control

Files:

- supervisor or launcher surface

Deliver:

- restart
- stop
- stale lock reclaim
- safe delete-state

---

## Suggested UI model

### Top summary strip

- running instances
- conflicted instances
- stale locks
- total active sessions

### Main table columns

- name
- workspace
- branch
- status
- graph
- sessions
- alerts
- last save
- actions

### Detail drawer sections

- runtime paths
- state ownership
- graph/runtime summary
- persistence health
- daemon + auto-ingest
- recent alerts

---

## Critical product rules

### Rule 1

One writable owner per `runtime_root`.

### Rule 2

Multiple instances across different projects must be zero-conflict by default.

### Rule 3

Multiple worktrees of the same repo must isolate by worktree path unless the user explicitly chooses shared state.

### Rule 4

The UI must surface conflict type explicitly, not just "error".

### Rule 5

`Save state`, `Restart`, and `Delete state` must operate on the instance, not on a guessed workspace.

---

## Validation against the current graph

The current plan touches the right core surfaces:

- `m1nd-mcp/src/session.rs`
- `m1nd-mcp/src/http_server.rs`
- `m1nd-mcp/src/server.rs`
- `m1nd-demo/src/App.tsx`
- `m1nd-demo/src/pages/Landing.tsx`

`m1nd.validate_plan` classified the overall plan as `medium` risk with no structural gaps, while recommending explicit tests for:

- `session`
- `http_server`
- `server`
- landing/app routing

That is consistent with the code shape.

---

## Bottom line

`m1nd` does not need one giant shared runtime to support many open projects.

It needs:

- isolated instance state
- deterministic runtime-root ownership
- a global registry
- a command center UI above those instances

That is the clean path to:

- many open projects
- no graph/plasticity collisions
- visible conflicts
- explicit restart/save/delete controls
- and a user experience that makes multi-instance operation understandable instead of mysterious
