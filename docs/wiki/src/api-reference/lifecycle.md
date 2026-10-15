# Lifecycle, Document & Lock Tools

This page covers the in-session front door, graph ingestion, document runtime operations, health monitoring, plan validation, persistence, and subgraph locking with change detection.

---

<a id="m1ndnorth"></a>

## `north`
The in-session front door. Call `north(task)` **before reading or editing
anything**. In one round-trip it composes binding trust, task context (focus
nodes + PageRank anchors), prior cross-session memory (each claim with its real
age and author — absent, never faked, when unknown), a sufficiency signal, one
`next_move`, and `honest_gaps` (what m1nd does not yet know).

The packet also carries **`memory_exists`** — the ground-truth count of durable
L1GHT claims in the on-disk store. It exists so a beat that surfaces no
task-relevant memory (`memory: []`) never lies about an **empty store**: when the
store holds claims that simply did not match this task, `memory_exists` is `> 0`
and `honest_gaps` says the store has memory that did not match — the false
"no durable memory yet" line is emitted **only** when the store is truly empty.

**Packet budget.** The binding block is **fixed-cost**: it may not grow with the
brain, because `north` runs on every session and every task. `binding.graph_state`
carries only the `ingest_root_count`, never a copy of the array.
`binding.fingerprint` carries at most the **first 10** ingest roots (the array is
ordered oldest → newest, so the head is the identity-bearing prefix and stays
stable across writes, which is what makes cross-seam fingerprint comparison
meaningful) and always states the real total in `ingest_root_count`. Truncation
is **declared, never silent**: `ingest_roots_truncated` (bool, always present),
`ingest_roots_omitted` (how many entries are not shown), and
`ingest_roots_full_surface` — the pointer to the surface that serves the whole
array, [`doctor`](#m1nddoctor) under `runtime_state.ingest_roots` (`null` when
nothing was omitted). Measured on a live owner 2026-07-24: 380 roots cost 25,907
bytes (~6.5k tokens) on **every** `north` before the head-truncation; the same
fingerprint now serializes in under 2,000 bytes. Durable memory sidecars in the
`agent-memory` store also collapse into the single store-directory root rather
than minting one ingest root per `.light.md` file, so the packet stays within its
2,000-token MCP budget as the memory store grows.

On first contact the packet may also carry a `reception` field (First-Contact
Reception, degraded mode): `reception.match == "caller_root_mismatch"` means the
bound graph does **not** cover your current repo (its root ≠ your resolved
caller root) — do not trust retrieval for this repo; read `reception.options[]`.
The public brain-bootstrap consumer is **not installed**: `ingest` does not
accept `project_root` (the parameter is absent from the published schema), and
cross-root bootstrap is POSITIVE_SOVEREIGN — it fails closed with
`brain_bootstrap_consumer_not_installed`. Continue only against the bound graph
with the mismatch warning intact, or reconnect to an owner that already hosts
the intended repo. It is `null` when your root matches the brain serving you (silent
bind is legal only on a match) or when the caller root is unknown (direct-HTTP
callers).

`north` composes `trust_selftest` + `orient` + `boot_memory` + `focus`. Reach
for those pieces directly only when you need just one.

When a SystemBlock store exists, `north` also carries its coherence vital sign
through `honest_gaps`. A mismatch names both the serving brain's expected slug
and the skeleton's found slug as a **signal only**; it never blocks reads or
writes. No store means no coherence line.

The `trust_selftest` / `session_handshake` / `recovery_playbook` trio is the
degraded/recovery lane — use it when trust looks off, retrieval is blocked, or a
binding is stale — not the default front door.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `agent_id` | `string` | Yes | -- | Calling agent identifier. |
| `task` | `string` | No | -- | Natural-language description of what you are about to do. Focuses task context and the `next_move`. |

### When to Use

- **Session start** -- the first call before any read or edit, to pre-orient
- **Task switch** -- re-orient when the goal changes
- **After `needs_ingest`** -- follow the packet's `next_move`, then call `north`
  again. `needs_ingest` is a real answer for an empty or unbound graph, not a
  failure. A repo with no brain yet is the HUMAN's one-time
  `m1nd init --birth <repo>` (offer it, never run it); a stale declared root is
  `ingest {mode:"refresh"}`, which is yours

### Related Tools

- [`ingest`](#m1ndingest) -- `mode:"refresh"` after `needs_ingest` on a root this brain already declares; a repo with no brain at all needs the human's `m1nd init --birth <repo>`
- [`trust_selftest`](#m1ndtrust_selftest) -- the degraded/recovery entry when trust looks off
- [`seek`](exploration.md#m1ndseek) -- first retrieval pass after orientation

---

<a id="m1ndingest"></a>

## `ingest`

The engine supports several ingestion modes and adapters, but the public tool is constrained by authority. **Default/`replace` and `merge` are policy-disabled for clients.** A schema describing a mode is not permission to call it. Do not use this tool to bootstrap a missing graph, attach another repository, or merge documents.

For first setup, use [Quick Start](../tutorials/quickstart.md). A person prepares a shared/served brain in a terminal. The current source-preview isolated path uses an explicit launcher workspace grant before serving tools; it is not a generic ingest exception.

### Refresh an already declared exact root

```json
{"method":"tools/call","params":{"name":"ingest","arguments":{"agent_id":"dev","path":"/absolute/path/to/already-declared-repo","mode":"refresh"}}}
```

`refresh` requires the caller's exact canonical root to already be declared by the selected brain. It refuses unresolved paths, subdirectory/root mismatches, overlapping refreshes, changed root sets, and a candidate graph below the shrink guard. It does not mint or rebind a brain. The graph mutation can be expensive; read its result instead of assuming the current graph is fresh.

Inspect the installed schema for supported adapter and payload fields. Document-engine capabilities include code, structured data, memory, and universal/document adapters, but their existence does not expose public `merge` authority. For existing document nodes, see the read-oriented document tools below.

Related: [`health`](#m1ndhealth), [`doctor`](#m1nddoctor), and [agent autonomy](https://github.com/maxkle1nz/m1nd/blob/main/docs/AGENT-AUTONOMY.md).

---

<a id="m1nddocument_resolve"></a>

## `document_resolve`
Resolve the canonical local artifact set for a universally ingested document by source path or universal node id.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | `string` | Yes | Calling agent identifier. |
| `path` | `string` | No | Original source path or canonical markdown path. |
| `node_id` | `string` | No | Universal graph node id for the document. |

### Example Request

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "document_resolve",
    "arguments": {
      "agent_id": "agent-1",
      "path": "docs/specs/auth.md"
    }
  }
}
```

### Example Response

```json
{
  "source_path": "docs/specs/auth.md",
  "canonical_markdown_path": "/tmp/m1nd-runtime/l1ght-cache/sources/abcd/canonical.md",
  "canonical_json_path": "/tmp/m1nd-runtime/l1ght-cache/sources/abcd/canonical.json",
  "claims_path": "/tmp/m1nd-runtime/l1ght-cache/sources/abcd/claims.json",
  "producer": "universal:internal",
  "section_count": 4,
  "claim_count": 3,
  "binding_count": 2
}
```

### When to Use

- when an agent needs the durable local artifact path
- when a doc has already been ingested and you want its canonical projection
- before opening `canonical.md` or `claims.json` directly

### Related Tools

- [`document_bindings`](#m1nddocument_bindings)
- [`document_drift`](#m1nddocument_drift)

---

<a id="m1nddocument_provider_health"></a>

## `document_provider_health`
Report availability, mode, detail, and install hints for optional universal-document providers.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | `string` | Yes | Calling agent identifier. |

### Example Response

```json
{
  "python": "python3",
  "providers": [
    { "name": "docling", "available": true, "mode": "broad-spectrum canonicalizer" },
    { "name": "grobid", "available": false, "mode": "scholarly pdf lane", "install_hint": "Set M1ND_GROBID_URL to a reachable GROBID service." }
  ]
}
```

### When to Use

- before assuming richer HTML/PDF/office extraction exists
- during environment setup
- when a provider-backed lane seems to be falling back unexpectedly

### Related Tools

- [`ingest`](#m1ndingest)
- [`auto_ingest_status`](#m1ndauto_ingest_status)

---

<a id="m1nddocument_bindings"></a>

## `document_bindings`
Resolve deterministic document-to-code bindings for a universal document.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | `string` | Yes | Calling agent identifier. |
| `path` | `string` | No | Original source path or canonical markdown path. |
| `node_id` | `string` | No | Universal graph node id for the document. |
| `top_k` | `integer` | No | Maximum bindings to return. |

### Example Response

```json
{
  "source_path": "docs/specs/auth.md",
  "bindings": [
    {
      "target_node_id": "file::src/auth/session.rs",
      "target_label": "SessionPool",
      "relation": "mentions_symbol",
      "score": 0.92,
      "confidence": "parsed",
      "reason": "exact label match"
    }
  ]
}
```

### When to Use

- when the question is “which code implements this doc?”
- when preparing an implementation map from a spec, paper, or note
- before editing code to match a document

### Related Tools

- [`document_resolve`](#m1nddocument_resolve)
- [`document_drift`](#m1nddocument_drift)

---

<a id="m1nddocument_drift"></a>

## `document_drift`
Analyze stale, missing, or ambiguous document/code bindings for a universal document.

### Example Response

```json
{
  "source_path": "docs/specs/auth.md",
  "summary": {
    "total_findings": 1,
    "stale_bindings": 1,
    "missing_targets": 0,
    "ambiguous_targets": 0,
    "unbacked_claims": 0,
    "code_change_unreflected": 1
  }
}
```

### When to Use

- after refactors or repo moves
- when document claims may no longer be backed by current code
- when a spec feels “probably stale” and you want a grounded first pass

### Related Tools

- [`document_bindings`](#m1nddocument_bindings)
- [`auto_ingest_status`](#m1ndauto_ingest_status)

---

<a id="m1ndauto_ingest_start"></a>

## `auto_ingest_start`
Start local-first document watchers for one or more roots and supported document families.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | `string` | Yes | Calling agent identifier. |
| `roots` | `string[]` | Yes | Filesystem roots to watch recursively. |
| `formats` | `string[]` | No | Supported document formats to auto-ingest. |
| `debounce_ms` | `integer` | No | Minimum quiet period before a change is eligible for ingestion. |
| `namespace` | `string` | No | Optional namespace for non-code document nodes. |

### Example Request

```json
{
  "jsonrpc": "2.0",
  "id": 6,
  "method": "tools/call",
  "params": {
    "name": "auto_ingest_start",
    "arguments": {
      "agent_id": "agent-1",
      "roots": ["/project/docs", "/project/wiki"],
      "formats": ["universal", "light"],
      "debounce_ms": 200
    }
  }
}
```

<a id="m1ndauto_ingest_status"></a>

## `auto_ingest_status`
Inspect the current auto-ingest runtime, queue depth, semantic counts, provider status, and provider route/fallback counts.

### Example Response

```json
{
  "running": true,
  "queue_depth": 0,
  "semantic_document_count": 12,
  "semantic_claim_count": 34,
  "drift_document_count": 1,
  "provider_status": { "docling": true, "trafilatura": true, "grobid": false }
}
```

<a id="m1ndauto_ingest_tick"></a>

## `auto_ingest_tick`
Drain queued document changes immediately and apply them to the active graph.

### Example Response

```json
{
  "ingested_paths": ["/project/docs/specs/auth.md"],
  "removed_paths": [],
  "skipped_paths": [],
  "errored_paths": []
}
```

<a id="m1ndauto_ingest_stop"></a>

## `auto_ingest_stop`
Stop active document watchers and persist the manifest state.

### Related Tools

- [`auto_ingest_start`](#m1ndauto_ingest_start)
- [`auto_ingest_status`](#m1ndauto_ingest_status)
- [`document_resolve`](#m1nddocument_resolve)

---

<a id="m1ndhealth"></a>

## `health`
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
    "name": "health",
    "arguments": {
      "agent_id": "agent-1"
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
    { "agent_id": "agent-1", "last_active": "2026-03-13T11:25:00Z" }
  ]
}
```

### When to Use

- **Session start** -- verify the server is alive and the graph is loaded (prefer `north` to pre-orient; `health` is the raw liveness check)
- **Monitoring** -- periodic health checks in long sessions
- **Debugging** -- check memory usage and query counts

### Related Tools

- [`north`](#m1ndnorth) -- composed in-session front door for orientation
- [`ingest`](#m1ndingest) -- load data if the graph is empty
- [`trust_selftest`](#m1ndtrust_selftest) -- one-call startup verdict for the degraded/recovery lane
- [`session_handshake`](#m1ndsession_handshake) -- classify whether this binding is trustworthy
- [`recovery_playbook`](#m1ndrecovery_playbook) -- return ordered recovery steps
- [`doctor`](#m1nddoctor) -- explain empty graphs, blocked retrieval, and binding/session continuity
- [`drift`](memory.md#m1nddrift) -- check what changed since last session

---

<a id="m1ndtrust_selftest"></a>

## `trust_selftest`
Returns a one-call trust verdict for agents. It composes the current binding
fingerprint, graph state, host-visible tool evidence, `session_handshake`, and
an optional `recovery_playbook`.

This is the degraded/recovery entry, not the default front door — reach for it
when trust looks off (blocked retrieval, `wrong_workspace_binding`, a
`Transport closed` error) or when you only need the trust layer. For normal
in-session orientation, call [`north`](#m1ndnorth), which composes this check
with `orient`, `boot_memory`, and `focus`.

The tool is diagnostic-only: it does not ingest, repair, run shell commands,
mutate files, refresh host bindings, or probe retrieval automatically.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `agent_id` | `string` | Yes | -- | Calling agent identifier. |
| `observed_tool_count` | `integer` | No | -- | Tool count returned by the host client's `tools/list`. |
| `available_tools` | `array<string>` | No | `[]` | Tool names exposed by the host client. |
| `missing_tools` | `array<string>` | No | `[]` | Required tool names missing from the host client surface. |
| `observed_tool` | `string` | No | -- | Tool that produced a suspicious result. |
| `observed_proof_state` | `string` | No | -- | Observed proof state, such as `blocked`. |
| `observed_candidates` | `integer` | No | -- | Candidate count from suspicious retrieval. |
| `scope` | `string` | No | -- | Repo or scope path associated with the incident. |
| `error_text` | `string` | No | -- | Error text or host message. |

### Example Request

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "trust_selftest",
    "arguments": {
      "agent_id": "agent-1"
    }
  }
}
```

### Example Response

```json
{
  "schema": "m1nd-trust-selftest-v0",
  "ok": true,
  "status": "ok",
  "verdict": "full_trust",
  "next_action": "proceed_with_m1nd_first",
  "binding_fingerprint": {
    "schema": "m1nd-binding-fingerprint-v0",
    "process_id": 12345,
    "node_count": 9767
  },
  "checks": {
    "graph_populated": true,
    "host_surface_complete": true,
    "stale_binding_suspected": false,
    "recovery_playbook_attached": false
  },
  "recovery_playbook": null
}
```

### Verdicts

- `full_trust` -- graph has content and the required recovery surface is present.
- `needs_ingest` -- graph is empty; the playbook names who repairs it (the human's `m1nd init --birth <repo>` for a repo with no brain, `ingest {mode:"refresh"}` for a declared root).
- `orientation_only` -- graph is empty and the current host surface cannot ingest.
- `degraded_host_tool_surface` -- the host is hiding required recovery tools.
- `wrong_workspace_binding` -- the requested `scope` points at a different
  workspace than the active binding. Rebind with `M1ND_WORKSPACE_ROOT`, ingest
  the requested workspace on the same binding, or use explicit federation for
  intentional cross-repo work.
- `stale_binding_suspected` -- populated graph plus blocked/zero-candidate evidence suggests host/session split-brain.

### Related Tools

- [`session_handshake`](#m1ndsession_handshake) -- cheaper sub-check used by the selftest
- [`recovery_playbook`](#m1ndrecovery_playbook) -- attached when the verdict needs action
- [`doctor`](#m1nddoctor) -- deeper runtime diagnosis when recovery asks for it
- [`ingest`](#m1ndingest) -- `mode:"refresh"` only, and only after `needs_ingest` on a declared root

---

<a id="m1ndsession_handshake"></a>

## `session_handshake`
Classifies whether the current MCP binding is trustworthy before an agent leans
on retrieval. The tool is diagnostic-only: it does not ingest, repair, run shell
commands, or execute a retrieval probe.

Pass the host's `tools/list` evidence when available. That lets m1nd distinguish
a genuinely empty graph from a host surface that is hiding recovery tools such
as `ingest`.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `agent_id` | `string` | Yes | -- | Calling agent identifier. |
| `observed_tool_count` | `integer` | No | -- | Tool count returned by the host client's `tools/list`. |
| `available_tools` | `array<string>` | No | `[]` | Tool names exposed by the host client. |
| `missing_tools` | `array<string>` | No | `[]` | Required tool names missing from the host client surface. |
| `scope` | `string` | No | -- | Absolute or repo-relative scope/path to validate against the active workspace binding. |

### Example Request

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "session_handshake",
    "arguments": {
      "agent_id": "agent-1"
    }
  }
}
```

### Example Response

```json
{
  "schema": "m1nd-session-handshake-v0",
  "trust_mode": "full_trust",
  "can_ingest": true,
  "can_retrieve": true,
  "can_recover": true,
  "next_action": "continue with m1nd-first retrieval; use compiler/tests for runtime truth",
  "binding_fingerprint": {
    "schema": "m1nd-binding-fingerprint-v0",
    "process_id": 12345,
    "runtime_root": "/tmp/m1nd-runtime",
    "graph_path": "/tmp/m1nd-runtime/graph.json",
    "graph_generation": 1,
    "node_count": 9767
  },
  "tool_surface": {
    "status": "ok",
    "degraded_host_tool_surface": false
  },
  "used_probe": false,
  "probe": null
}
```

### Trust Modes

- `full_trust` -- graph has content and the required recovery surface is present.
- `needs_ingest` -- graph is empty; the playbook names who repairs it (the human's `m1nd init --birth <repo>` for a repo with no brain, `ingest {mode:"refresh"}` for a declared root).
- `orientation_only` -- graph is empty and the current host surface cannot ingest.
- `degraded_host_tool_surface` -- the host is hiding required recovery tools.
- `wrong_workspace_binding` -- the requested scope belongs to a different
  workspace than the active binding. Follow `context_guard` and
  `doctor_recovery` before retrieval.

### Related Tools

- [`trust_selftest`](#m1ndtrust_selftest) -- preferred one-call startup verdict
- [`recovery_playbook`](#m1ndrecovery_playbook) -- what to do when trust mode is not full
- [`ingest`](#m1ndingest) -- `mode:"refresh"` reloads a declared root after `needs_ingest`
- [`doctor`](#m1nddoctor) -- inspect the recovery payload under suspicion
- [`seek`](exploration.md#m1ndseek) -- first retrieval pass after `full_trust`

---

<a id="m1ndrecovery_playbook"></a>

## `recovery_playbook`
Returns deterministic next steps for a degraded host surface, empty graph,
orientation-only binding, or stale-looking retrieval. The tool is
diagnostic-only: it does not ingest, repair, run shell commands, mutate files,
or probe retrieval.

Use it after `session_handshake` reports anything other than `full_trust`, or
after a retrieval response reports `blocked` or zero candidates when the graph
should be populated.

If the incident involves an absolute path or another repository, pass it as
`scope`. The playbook returns `trust_mode=wrong_workspace_binding` and
`next_action=select_or_bind_workspace` when the active graph is healthy but the
call targeted a different workspace.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `agent_id` | `string` | Yes | -- | Calling agent identifier. |
| `trust_mode` | `string` | No | -- | Prior handshake trust mode to preserve in the diagnostic trail. |
| `observed_tool` | `string` | No | -- | Tool that produced a suspicious result. |
| `observed_proof_state` | `string` | No | -- | Observed proof state, such as `blocked`. |
| `observed_candidates` | `integer` | No | -- | Candidate count from the suspicious retrieval. |
| `observed_tool_count` | `integer` | No | -- | Tool count returned by the host client's `tools/list`. |
| `available_tools` | `array<string>` | No | `[]` | Tool names exposed by the host client. |
| `missing_tools` | `array<string>` | No | `[]` | Required tool names missing from the host client surface. |
| `scope` | `string` | No | -- | Repo or scope path associated with the incident. |
| `error_text` | `string` | No | -- | Error text or host message. |

### Example Request

```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "tools/call",
  "params": {
    "name": "recovery_playbook",
    "arguments": {
      "agent_id": "agent-1",
      "observed_tool": "seek",
      "observed_proof_state": "blocked",
      "observed_candidates": 0
    }
  }
}
```

### Example Response

```json
{
  "schema": "m1nd-recovery-playbook-v0",
  "status": "warn",
  "trust_mode": "stale_binding_suspected",
  "recovery_goal": "Prove whether host, binary, runtime, or graph identity drift is causing split-brain retrieval.",
  "next_action": "call_doctor",
  "steps": [
    {
      "id": "call_doctor",
      "tool": "doctor",
      "action": "Call doctor with the blocked or zero-candidate observation."
    },
    {
      "id": "compare_binding_fingerprint",
      "action": "Compare this binding_fingerprint with the host, repo-local stdio, and repo-local HTTP handshake outputs."
    }
  ],
  "non_claims": [
    "No automatic repair was performed.",
    "No ingest or graph mutation was performed."
  ]
}
```

### Related Tools

- [`session_handshake`](#m1ndsession_handshake) -- classify the binding first
- [`doctor`](#m1nddoctor) -- inspect active graph/session/runtime clues
- [`ingest`](#m1ndingest) -- run only when the playbook says the graph needs it

---

<a id="m1nddoctor"></a>

## `doctor`
Diagnoses active graph, runtime, session, and stale-binding symptoms. Use it
when an agent just ingested a repo but retrieval returns `blocked`, zero
candidates, or a graph that feels empty.

`seek`, `search`, and `activate` include a ready `recovery` payload when they
return `blocked` or zero actionable candidates. An agent can pass
`recovery.arguments` directly to `doctor`.

Use the same tool for degraded host surfaces. If `tools/list` is missing
required recovery tools such as `ingest`, pass the observed count, available
tool names, and missing tool names to `doctor`. It will flag
`diagnostics.degraded_host_tool_surface` and explain whether the current host
binding should be treated as orientation-only.

If the suspicious call included a path from another repo, pass it as `scope`.
`doctor` will surface `context_guard.wrong_workspace_binding=true` and suggest
rebinding with `M1ND_WORKSPACE_ROOT`, same-binding ingest, or explicit
federation instead of diagnosing a stale graph.

`doctor` is also the surface that serves the **complete** `ingest_roots` array,
under `runtime_state.ingest_roots`. The `north` binding fingerprint is
budget-capped and carries only a head plus the count (see
[Packet budget](#m1ndnorth)); when you need every root, ask `doctor`.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `agent_id` | `string` | Yes | -- | Calling agent identifier. |
| `observed_tool` | `string` | No | -- | Tool that returned a suspicious result. |
| `observed_proof_state` | `string` | No | -- | Observed proof state, such as `blocked`. |
| `observed_candidates` | `integer` | No | -- | Candidate count from the suspicious retrieval. |
| `observed_tool_count` | `integer` | No | -- | Tool count returned by the host client's `tools/list`. |
| `available_tools` | `array<string>` | No | `[]` | Tool names exposed by the host client. |
| `missing_tools` | `array<string>` | No | `[]` | Required tool names missing from the host client surface. |
| `scope` | `string` | No | -- | Scope/path used by the suspicious call. |
| `error_text` | `string` | No | -- | Error text or host message. |

### Example Request

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "doctor",
    "arguments": {
      "agent_id": "agent-1",
      "observed_tool": "seek",
      "observed_proof_state": "blocked",
      "observed_candidates": 0
    }
  }
}
```

### Degraded Tool Surface Request

```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "tools/call",
  "params": {
    "name": "doctor",
    "arguments": {
      "agent_id": "agent-1",
      "observed_tool": "tools/list",
      "observed_proof_state": "blocked",
      "observed_tool_count": 3,
      "available_tools": ["seek", "audit", "doctor"],
      "missing_tools": ["ingest"]
    }
  }
}
```

### Example Response

```json
{
  "schema": "m1nd-doctor-v0",
  "status": "warn",
  "diagnostics": {
    "graph_has_nodes": true,
    "agent_session_known": true,
    "stale_binding_suspected": true,
    "wrong_workspace_binding": false
  },
  "warnings": [
    "seek reported blocked/zero-candidate retrieval while the active graph is populated"
  ],
  "next_actions": [
    "verify the same binding with stdio and HTTP smokes before declaring the graph stale"
  ]
}
```

### When to Use

- **After suspicious retrieval** -- distinguish empty graph from stale binding
- **After incomplete `tools/list`** -- identify a degraded host tool surface
- **After transport changes** -- confirm HTTP and stdio see the same session
- **Before shell fallback** -- give the agent a precise recovery path

### Related Tools

- [`health`](#m1ndhealth) -- basic server health and graph counts
- [`ingest`](#m1ndingest) -- refresh graph state
- [`help`](exploration.md#m1ndhelp) -- tool guidance and examples

---

<a id="m1ndvalidate_plan"></a>

## `validate_plan`
Validate a proposed modification plan against the code graph. Detects gaps (affected files missing from the plan), risk level, test coverage, and suggested additions. Designed to be called before implementing a plan.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `agent_id` | `string` | Yes | -- | Calling agent identifier. |
| `actions` | `object[]` | Yes | -- | Ordered list of planned actions. Each object has: `action_type` (string, required -- `"modify"`, `"create"`, `"delete"`, `"rename"`, `"test"`), `file_path` (string, required -- relative path), `description` (string, optional), `depends_on` (string[], optional -- other file_paths this action depends on). |
| `include_test_impact` | `boolean` | No | `true` | Analyze test coverage for modified files. |
| `include_risk_score` | `boolean` | No | `true` | Compute composite risk score. |
| `scope` | `string` | No | -- | Optional repo or scope path for multi-repo binding diagnostics. |

### Example Request

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "validate_plan",
    "arguments": {
      "agent_id": "agent-1",
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

- [`impact`](analysis.md#m1ndimpact) -- blast radius for a single node
- [`predict`](analysis.md#m1ndpredict) -- co-change prediction for a single node
- [`trace`](exploration.md#m1ndtrace) -- validate a fix plan for a specific error

---

<a id="m1ndlockcreate"></a>

## `lock_create`

**Unreleased source-build restoration.** These five graph-tracking tools are in
full registry/help discovery and the opt-in `M1ND_TOOL_TIER=investigate` menu.
The released 1.6.3 menu does not include this restoration. Restart/reconnect the
host after changing its server environment. Default core remains unchanged.

Capture an ephemeral graph baseline; this is not a filesystem lock, reservation,
source-content diff, or mutual exclusion. All calls use the same `agent_id` and
running session. Copy live canonical node IDs from `seek`/`search`, then save the
returned `lock_id`. Baselines do not survive restart.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | `string` | Yes | Calling agent identifier; owns the baseline. |
| `scope` | `string` | Yes | `node`, `subgraph`, or `path`. |
| `root_nodes` | `string[]` | Yes | Nonempty exact canonical IDs or unique exact labels. Unknown/ambiguous IDs refuse. |
| `radius` | `integer` | No | Subgraph only: outgoing BFS depth 1–4, default 2. |
| `path_nodes` | `string[]` | No | Path only: additional explicit nodes. Does not calculate or validate connectivity. |

`query_neighborhood` and `query` refuse. `node` uses the explicit roots;
`subgraph` expands them; `path` combines roots and optional path nodes. The
snapshot observes scoped nodes and every incident edge, including incoming and
outgoing boundary edges. `baseline_edges` counts directed CSR arcs, so a bidirectional logical edge counts
twice. Parallel arcs use deterministic weight-sorted multiset pairing, not
persistent edge-instance attribution. Treat edge keys as opaque JSON tuples.
Output includes `lock_id`, scope, baseline counts, generation and creation time.
See the [beginner and advanced recipe](../../../GRAPH-CHANGE-TRACKING.md).

<a id="m1ndlockwatch"></a>

## `lock_watch`

Required: `agent_id`, returned `lock_id`, and `strategy`.
`manual` buffers no notifications; `on_ingest` and `on_learn` buffer coalesced
trigger notifications. They do not run a background scheduler, watch source
files, or authorize graph updates. `periodic` refuses. In particular, configuring
`on_learn` does not make an authority-gated `learn` call executable.
Output includes the strategy and the previous strategy when one was replaced.

<a id="m1ndlockdiff"></a>

## `lock_diff`

Required: `agent_id` and returned `lock_id`.
Compares the current scoped graph against the baseline. Reports canonical node
additions/removals, internal and boundary edge additions/removals, weight changes,
and generation staleness. Weight changes require an absolute delta greater than
0.001; `no_changes` means no detected change above that tolerance.
`diff_truncated` flags capped added-node, added-internal-edge and
added-boundary-edge previews only. `no_changes` is computed before those caps. It drains pending watcher notifications but does not
advance the baseline. `watcher_events_drained` counts coalesced notification
batches, not mutations. An empty diff is not proof that source text is unchanged:
source changes only appear here after an independently authorized graph update.

<a id="m1ndlockrebase"></a>

## `lock_rebase`

Required: `agent_id` and returned `lock_id`.
After reviewing the diff, replace the comparison point with the current scoped
graph. Watch configuration is preserved. Output includes previous/new generations,
baseline counts and `watcher_preserved`. Rebasing discards the old comparison
point; it does not grant permission to modify files or ingest roots. If the
entire observed scope has vanished, rebase refuses and preserves the old baseline
for inspecting removals or releasing it.

<a id="m1ndlockrelease"></a>

## `lock_release`

Required: `agent_id` and returned `lock_id`.
Discard the baseline, watcher and queued notifications. Output includes the ID
and `released`. All five operations retain existing authority checks; ownership
by agent ID is not authentication against a caller choosing another agent ID.

---

<a id="m1nddaemon_start"></a>

## `daemon_start`
Arm the per-brain code daemon: persist the watched roots (empty = the brain's own ingest roots) and initialize the daemon counters. Arming is a per-brain opt-in — the durable state lives in the brain's own store dir, survives owner restarts AND LRU eviction (the daemon re-arms on the next warm-boot/resolve), and the factory default is OFF.

**Freshness is honest, not continuous.** On a served (HTTP) owner ticks advance BY TRAFFIC — any non-recall verb touching the brain runs a due tick ("fresh when seen") — or via an explicit `daemon_tick`. Only a stdio owner additionally holds a live watch-event consumer and an idle clock. No surface claims a free-running monitor; `watch_backend` reports `native_fs` only while a real notify watcher exists.

Bursts coalesce: a branch checkout becomes ONE detection whose full changed set enters a persisted backlog, drained a bounded slice per tick (`max_files`) — no file is lost to truncation. After a burst settles (45 s quiet window, pushed by every activity tick), the daemon auto-reconciles the RATIFIED system-blocks store: it yields voluntarily to a live `candidate_lease`, keys the write on a fresh `store_version`, and gives OCC exactly one retry before raising an `auto_reconcile_conflict` alert.

### Parameters

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `agent_id` | `string` | Yes | -- | Calling agent identifier. |
| `watch_paths` | `string[]` | No | current ingest roots | Paths the daemon should watch (the per-brain default is the brain's own ingest roots). |
| `poll_interval_ms` | `integer` | No | `500` | Traffic/idle tick due-interval in milliseconds. |

### When to Use

- Opting a brain into standing freshness (per-brain, durable, default OFF)
- Before relying on daemon alerts or `daemon_tick`
- Before background/idle reconciliation should run

### Related Tools

- [`daemon_status`](#m1nddaemon_status)
- [`daemon_tick`](#m1nddaemon_tick)
- [`alerts_list`](#m1ndalerts_list)

---

<a id="m1nddaemon_stop"></a>

## `daemon_stop`
Stop the daemon control plane without deleting persisted alert history.

### Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | `string` | Yes | Calling agent identifier. |

### When to Use

- End of a daemon-backed session
- Before shutting down a host that should not keep reconciling

### Related Tools

- [`daemon_start`](#m1nddaemon_start)
- [`daemon_status`](#m1nddaemon_status)

---

<a id="m1nddaemon_status"></a>

## `daemon_status`
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

- [`daemon_start`](#m1nddaemon_start)
- [`daemon_tick`](#m1nddaemon_tick)
- [`alerts_list`](#m1ndalerts_list)

---

<a id="m1nddaemon_tick"></a>

## `daemon_tick`
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

- [`daemon_status`](#m1nddaemon_status)
- [`alerts_list`](#m1ndalerts_list)
- [`cross_verify`](../api-reference/exploration.md#m1ndcross_verify)

---

<a id="m1ndalerts_list"></a>

## `alerts_list`
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

- [`alerts_ack`](#m1ndalerts_ack)
- [`daemon_status`](#m1nddaemon_status)

---

<a id="m1ndalerts_ack"></a>

## `alerts_ack`
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

- [`alerts_list`](#m1ndalerts_list)

---

<a id="m1ndedit_preview"></a>

## `edit_preview`
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

- [`edit_commit`](#m1ndedit_commit)
- [`apply`](../api-reference/lifecycle.md)

---

<a id="m1ndedit_commit"></a>

## `edit_commit`
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

- [`edit_preview`](#m1ndedit_preview)
- [`apply`](../api-reference/lifecycle.md)

---

## Withdrawn transplant APIs

`transplant`, `transplant_preview`, and `transplant_commit` are absent from the
current source registry and dispatch. Old preview handles cannot be redeemed.
Use the host's edit tools, then verify the change with retrieval, impact, compiler
and tests. Other m1nd source writers retain their existing authority requirements.
See [the first slice contract](../../../FIRST-SIMPLIFICATION-SLICE.md).

---

<a id="m1ndpersist"></a>

## `persist`
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

- [`health`](#m1ndhealth)
- [`boot_memory`](#m1ndboot_memory)

---

<a id="m1ndboot_memory"></a>

## `boot_memory`
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

- [`persist`](#m1ndpersist)
- [`trail_save`](memory.md#m1ndtrailsave)

---

<a id="m1ndheuristics_surface"></a>

## `heuristics_surface`
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

- [`validate_plan`](#m1ndvalidate_plan)
- [`apply_batch`](../api-reference/lifecycle.md)
- [`daemon_tick`](#m1nddaemon_tick)

---

<a id="m1ndaudit"></a>

## `audit`
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

- [`batch_view`](../api-reference/exploration.md#m1ndbatch_view)
- [`cross_verify`](../api-reference/exploration.md#m1ndcross_verify)
- [`coverage_session`](../api-reference/exploration.md#m1ndcoverage_session)
