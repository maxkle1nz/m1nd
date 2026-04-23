# API Reference Overview

m1nd exposes the live MCP tool surface over JSON-RPC 2.0 via stdio. Every tool requires `agent_id` as a parameter. The exported MCP schema uses underscore-based canonical names such as `trail_resume`, `perspective_start`, and `apply_batch`. Use `tools/list` for the exact count in your current build.

The grouped pages below still keep historical prefixed anchors for stable links, but executable `tools/call` examples use the canonical bare names returned by `tools/list`.

Several tools now do more than return raw results. On the main structural flows you should expect some combination of:

- `proof_state`
- `next_suggested_tool`
- `next_suggested_target`
- `next_step_hint`

That matters for how you integrate m1nd into an agent loop: treat many responses as workflow guidance, not just data blobs.

## Tool Index

### Core Activation

| Tool | Description |
|------|-------------|
| [`activate`](activation.md#m1ndactivate) | Spreading activation query across the graph |
| [`warmup`](activation.md#m1ndwarmup) | Task-based warmup and priming |
| [`resonate`](activation.md#m1ndresonate) | Resonance analysis: harmonics, sympathetic pairs, and resonant frequencies |

### Analysis

| Tool | Description |
|------|-------------|
| [`impact`](analysis.md#m1ndimpact) | Impact radius / blast analysis for a node |
| [`predict`](analysis.md#m1ndpredict) | Co-change prediction for a modified node |
| [`counterfactual`](analysis.md#m1ndcounterfactual) | What-if node removal simulation |
| [`fingerprint`](analysis.md#m1ndfingerprint) | Activation fingerprint and equivalence detection |
| [`hypothesize`](analysis.md#m1ndhypothesize) | Graph-based hypothesis testing against structural claims |
| [`differential`](analysis.md#m1nddifferential) | Focused structural diff between two graph snapshots |
| [`diverge`](analysis.md#m1nddiverge) | Structural drift between a baseline and current graph state |

### Memory, Trails, and Learning

| Tool | Description |
|------|-------------|
| [`learn`](memory.md#m1ndlearn) | Explicit feedback-based edge adjustment |
| [`drift`](memory.md#m1nddrift) | Weight and structural drift analysis |
| [`why`](memory.md#m1ndwhy) | Path explanation between two nodes |
| [`trail_save`](memory.md#m1ndtrailsave) | Persist current investigation state |
| [`trail_resume`](memory.md#m1ndtrailresume) | Restore a saved investigation with next-step guidance |
| [`trail_list`](memory.md#m1ndtraillist) | List saved investigation trails |
| [`trail_merge`](memory.md#m1ndtrailmerge) | Combine two or more investigation trails |

### Exploration

| Tool | Description |
|------|-------------|
| [`seek`](exploration.md#m1ndseek) | Intent-aware semantic code search |
| [`scan`](exploration.md#m1ndscan) | Pattern-aware structural code analysis |
| [`missing`](exploration.md#m1ndmissing) | Detect structural holes and missing connections |
| [`trace`](exploration.md#m1ndtrace) | Map runtime errors to structural root causes |
| [`timeline`](exploration.md#m1ndtimeline) | Git-based temporal history for a node |
| [`federate`](exploration.md#m1ndfederate) | Multi-repository federated graph ingestion |
| [`federate_auto`](exploration.md#m1ndfederate_auto) | Discover repo candidates from external path evidence and optionally federate them |

### Perspectives

| Tool | Description |
|------|-------------|
| [`perspective_start`](perspectives.md#m1ndperspectivestart) | Enter a perspective: navigable route surface from a query |
| [`perspective_routes`](perspectives.md#m1ndperspectiveroutes) | Browse the current route set with pagination |
| [`perspective_inspect`](perspectives.md#m1ndperspectiveinspect) | Expand a route with metrics, provenance, and affinity |
| [`perspective_peek`](perspectives.md#m1ndperspectivepeek) | Extract a code/doc slice from a route target |
| [`perspective_follow`](perspectives.md#m1ndperspectivefollow) | Follow a route: move focus to target, synthesize new routes |
| [`perspective_suggest`](perspectives.md#m1ndperspectivesuggest) | Get the next best move suggestion |
| [`perspective_affinity`](perspectives.md#m1ndperspectiveaffinity) | Discover probable connections a route target might have |
| [`perspective_branch`](perspectives.md#m1ndperspectivebranch) | Fork navigation state into a new branch |
| [`perspective_back`](perspectives.md#m1ndperspectiveback) | Navigate back to previous focus |
| [`perspective_compare`](perspectives.md#m1ndperspectivecompare) | Compare two perspectives on shared/unique nodes |
| [`perspective_list`](perspectives.md#m1ndperspectivelist) | List all perspectives for an agent |
| [`perspective_close`](perspectives.md#m1ndperspectiveclose) | Close a perspective and release associated locks |

### Lifecycle, Search, and Surgical

| Tool | Description |
|------|-------------|
| [`ingest`](lifecycle.md#m1ndingest) | Ingest or re-ingest a codebase, descriptor, or memory corpus |
| [`document_resolve`](lifecycle.md#m1nddocument_resolve) | Resolve canonical local artifacts for a universal document |
| [`document_provider_health`](lifecycle.md#m1nddocument_provider_health) | Report optional document provider availability and install hints |
| [`document_bindings`](lifecycle.md#m1nddocument_bindings) | Show deterministic document-to-code bindings |
| [`document_drift`](lifecycle.md#m1nddocument_drift) | Detect stale, missing, or ambiguous document/code links |
| [`auto_ingest_start`](lifecycle.md#m1ndauto_ingest_start) | Start local-first document auto-ingest watchers |
| [`auto_ingest_status`](lifecycle.md#m1ndauto_ingest_status) | Inspect the document auto-ingest runtime and counters |
| [`auto_ingest_tick`](lifecycle.md#m1ndauto_ingest_tick) | Drain queued document changes immediately |
| [`auto_ingest_stop`](lifecycle.md#m1ndauto_ingest_stop) | Stop document watchers and persist manifest state |
| [`health`](lifecycle.md#m1ndhealth) | Server health and statistics |
| [`search`](lifecycle.md#m1ndsearch) | Literal, regex, or semantic-graph-aware content search |
| [`glob`](lifecycle.md#m1ndglob) | Graph-aware file globbing |
| [`view`](lifecycle.md#m1ndview) | Fast line-numbered file inspection |
| [`validate_plan`](lifecycle.md#m1ndvalidate_plan) | Validate a modification plan against the code graph |
| [`surgical_context_v2`](lifecycle.md#m1ndsurgicalcontextv2) | Pull connected edit context with proof-oriented options |
| [`apply_batch`](lifecycle.md#m1ndapplybatch) | Atomic multi-file write with progress, verification, and handoff |

The grouped reference pages below still organize the surface by area, but the current best operational map is in `help`, the README tool table, and the live `tools/list` response.

---

## Common Parameters

Every tool requires the `agent_id` parameter:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `agent_id` | `string` | Yes | Identifier for the calling agent. Used for session tracking, perspective ownership, lock ownership, and multi-agent coordination. |

Agent IDs are free-form strings. Convention: use the agent's name or role identifier (e.g. `"jimi"`, `"hacker-auth"`, `"forge-build"`).

## JSON-RPC Request Format

m1nd uses the [MCP protocol](https://modelcontextprotocol.io/) over JSON-RPC 2.0 via stdio. All tool calls use the `tools/call` method.

### Request structure

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
        "name": "activate",
    "arguments": {
      "agent_id": "jimi",
      "query": "session management"
    }
  }
}
```

### Successful response

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{ ... pretty-printed JSON result ... }"
      }
    ]
  }
}
```

The `text` field contains the tool-specific output as a pretty-printed JSON string. Parse it to get the structured result.

### Error response (tool execution error)

Tool execution errors are returned as MCP `isError` content, **not** as JSON-RPC errors. Many current errors are recovery-oriented and may include fields such as `hint`, `workflow_hint`, `example`, or `suggested_next_step`.

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{ \"error\": \"...\", \"hint\": \"...\", \"suggested_next_step\": \"...\" }"
      }
    ],
    "isError": true
  }
}
```

### Error response (protocol error)

JSON-RPC protocol errors use standard error codes:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32601,
    "message": "Method not found: bad_method"
  }
}
```

## Error Codes

| Code | Meaning |
|------|---------|
| `-32700` | Parse error -- invalid JSON |
| `-32601` | Method not found -- unknown JSON-RPC method |
| `-32603` | Internal error -- server-level failure |

Tool-specific errors (node not found, perspective not found, lock ownership violation, etc.) are returned via `isError: true` in the content, not as JSON-RPC errors.

## Transport

m1nd supports two stdio transport modes, auto-detected per message:

- **Framed**: `Content-Length: N\r\n\r\n{json}` -- standard MCP/LSP framing. Used by Claude Code, Cursor, and most MCP clients.
- **Line**: `{json}\n` -- one JSON object per line. Used by simple scripts and testing.

The server auto-detects which mode each incoming message uses and responds in the same mode.

## Tool Name Normalization

The exported MCP schema uses underscore-based canonical names, but the server still accepts legacy transport aliases when possible:

- `activate` is canonical
- transport-prefixed aliases are accepted where normalization applies

If you are generating tool calls from an MCP client, prefer the canonical schema names returned by `tools/list`.

## Protocol Handshake

Before calling tools, MCP clients perform a handshake:

```json
{"jsonrpc":"2.0","id":0,"method":"initialize","params":{}}
```

Response:

```json
{
  "jsonrpc": "2.0",
  "id": 0,
  "result": {
    "protocolVersion": "2024-11-05",
    "serverInfo": { "name": "m1nd-mcp", "version": "0.8.0" },
    "capabilities": { "tools": {} }
  }
}
```

Then list available tools:

```json
{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}
```

This returns the full schema for the live tool surface with `inputSchema` for each entry. Treat `tools/list` as the source of truth for the exact count in your current build.
