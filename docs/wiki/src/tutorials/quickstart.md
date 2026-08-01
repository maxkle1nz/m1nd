# Quick Start

Five minutes from zero to your first useful graph query.

## Prerequisites

- **Rust toolchain**: current stable recommended if you build from source
- **A codebase**: Python, Rust, TypeScript/JavaScript, Go, and Java have the strongest handling; additional languages are available through tree-sitter and fallback extraction
- **An MCP client**: Claude Code, Codex, Cursor, Windsurf, Zed, Cline, Continue, or any client that can connect to an MCP server over stdio

## Install

Choose one path:

### 1. Build from source

```bash
git clone https://github.com/maxkle1nz/m1nd.git
cd m1nd
cargo build --release
```

The binary will be at:

```bash
./target/release/m1nd-mcp
```

If you want it on your PATH:

```bash
cp ./target/release/m1nd-mcp /usr/local/bin/
```

### 2. Install from crates.io

```bash
cargo install m1nd-mcp
```

### 3. Download a release binary

The current release workflow publishes these artifact names:

- `m1nd-mcp-linux-x86_64`
- `m1nd-mcp-macos-x86_64`
- `m1nd-mcp-macos-aarch64`

If you use release binaries, download them from the latest GitHub release page instead of relying on hardcoded tarball names.

## Verify the binary

```bash
m1nd-mcp --help
```

If the binary is healthy, you should see the CLI help for the MCP server.

## Configure your MCP client

m1nd is an MCP server over stdio. Your client starts the binary and calls tools through MCP.

### Claude Code

Add this to your Claude Code MCP config.

The exact file path depends on how you run Claude Code, but the current repo README uses this shape:

```json
{
  "mcpServers": {
    "m1nd": {
      "command": "/path/to/m1nd-mcp",
      "env": {
        "M1ND_GRAPH_SOURCE": "/tmp/m1nd-graph.json",
        "M1ND_PLASTICITY_STATE": "/tmp/m1nd-plasticity.json"
      }
    }
  }
}
```

### Codex

Codex reads MCP servers from `~/.codex/config.toml`.

Use:

```toml
[mcp_servers.m1nd]
command = "/path/to/m1nd-mcp"
args = ["--stdio", "--no-gui"]

[mcp_servers.m1nd.env]
M1ND_GRAPH_SOURCE = "/tmp/m1nd-graph.json"
M1ND_PLASTICITY_STATE = "/tmp/m1nd-plasticity.json"
```

Verify with:

```bash
codex mcp list
```

If the entry is enabled, restart Codex if needed and make the first call through the MCP tool surface.

### Cursor

In MCP Servers, use the same binary + env setup:

```json
{
  "m1nd": {
    "command": "/path/to/m1nd-mcp",
    "env": {
      "M1ND_GRAPH_SOURCE": "/tmp/m1nd-graph.json",
      "M1ND_PLASTICITY_STATE": "/tmp/m1nd-plasticity.json"
    }
  }
}
```

### Antigravity

Antigravity supports a workspace-local `mcp_config.json` shape.

Use:

```json
{
  "mcpServers": {
    "m1nd": {
      "command": "python3",
      "args": ["/path/to/m1nd-antigravity-proxy.py"],
      "cwd": "/path/to/workspace",
      "env": {
        "M1ND_OPENCLAW_SOCKET": "/tmp/m1nd-openclaw.sock",
        "M1ND_GRAPH_SOURCE": "/tmp/m1nd-graph.json",
        "M1ND_PLASTICITY_STATE": "/tmp/m1nd-plasticity.json"
      }
    }
  }
}
```

This keeps the host-facing contract stdio-compatible while routing into the hot native daemon.

### Other MCP clients

The pattern is the same:

- point the client at `m1nd-mcp`
- optionally set graph/plasticity persistence env vars
- then call tools through MCP

## Persistence variables

| Variable | Purpose | If omitted |
|----------|---------|------------|
| `M1ND_GRAPH_SOURCE` | Persist the graph snapshot | graph is memory-only |
| `M1ND_PLASTICITY_STATE` | Persist learned edge weights | learning is memory-only |

Recommendation: set both if you want continuity across restarts.

## Important note on tool names

The live MCP registry exposes bare tool names like:

- `ingest`
- `activate`
- `search`
- `impact`

Some clients or docs may show transport-prefixed aliases. Treat that as presentation sugar. The canonical live registry names are the bare tool names shown by `tools/list`.

## First run

Once your client is configured, the first thing to do is ingest a project.

### Step 1: ingest a repo

```jsonc
{
  "method": "tools/call",
  "params": {
    "name": "ingest",
    "arguments": {
      "agent_id": "dev",
      "path": "/path/to/your/project"
    }
  }
}
```

Example response shape:

```json
{
  "files_processed": 335,
  "nodes_created": 9767,
  "edges_created": 26557,
  "languages": {
    "python": 335
  },
  "elapsed_ms": 910
}
```

What happened:

- files were parsed
- structural nodes and edges were created
- references were resolved
- the graph was finalized for querying

### Step 2: pre-orient with `north` (the in-session front door)

Once the graph is ingested, call `north(task)` **before reading or editing
anything**. It is the in-session front door: one round-trip that composes
binding trust, task context, prior cross-session memory, a sufficiency signal,
one `next_move`, and `honest_gaps` (what m1nd does not yet know). Heed
`reception` when present: `reception.match == "caller_root_mismatch"` means the
bound graph does NOT cover your current repo — do not trust retrieval for it;
read `reception.options[]`. The public brain-bootstrap consumer is **not
installed**: `ingest` does not accept `project_root`, and cross-root bootstrap
fails closed with `brain_bootstrap_consumer_not_installed`. Continue only
against the bound graph with the mismatch warning intact, or reconnect to an
owner that already hosts the intended repo. Absent/null `reception` = your root
matches the brain serving you.

```jsonc
{
  "method": "tools/call",
  "params": {
    "name": "north",
    "arguments": {
      "agent_id": "dev",
      "task": "harden authentication token validation"
    }
  }
}
```

`north` composes `trust_selftest` + `orient` + `boot_memory` + `focus` — reach
for those pieces directly only when you need just one. If `north` returns
`needs_ingest` (empty or unbound graph), that is a real answer, not a failure —
and it is not yours to fix with `ingest`, which is refused on every transport.
Read the packet's `next_move`: a repo with no brain yet needs the HUMAN's
one-time ceremony `m1nd init --birth <repo>` (offer the command and stop), while
a brain that already declares your root you refresh yourself with
`ingest {mode:"refresh", path: <your root>}`. Then call `north` again.

If trust looks off — retrieval blocked, `wrong_workspace_binding`, or a
`Transport closed` error — drop to the degraded/recovery path with
`trust_selftest`, `session_handshake`, and `recovery_playbook`. Those are the
recovery lane, not the default front door.

```jsonc
{
  "method": "tools/call",
  "params": {
    "name": "trust_selftest",
    "arguments": {
      "agent_id": "dev"
    }
  }
}
```

`trust_selftest` returns a single `verdict` (`full_trust`, `needs_ingest`,
`orientation_only`, `degraded_host_tool_surface`, `wrong_workspace_binding`, or
`stale_binding_suspected`) plus a binding fingerprint, graph state, an embedded
`session_handshake`, and an optional recovery playbook. It is diagnostic-only:
it does not ingest, repair, refresh host bindings, mutate files, or run
retrieval probes. If the verdict or trust mode is not `full_trust`, ask m1nd for
the deterministic recovery path with `recovery_playbook`, which returns ordered
steps without performing the repair for you.

### Step 3: check server health

```jsonc
{
  "method": "tools/call",
  "params": {
    "name": "health",
    "arguments": {
      "agent_id": "dev"
    }
  }
}
```

Current response shape in the repo:

```json
{
  "status": "ok",
  "node_count": 9767,
  "edge_count": 26557,
  "queries_processed": 1,
  "uptime_seconds": 12.4,
  "memory_usage_bytes": 0,
  "plasticity_state": "0 edges tracked",
  "last_persist_time": null,
  "active_sessions": [
    {
      "agent_id": "dev",
      "query_count": 1
    }
  ]
}
```

If a later retrieval call looks stale, call `doctor` before falling back to
manual file search. `seek`, `search`, and `activate` now include a ready
`recovery.arguments` payload when they return `blocked` or zero actionable
candidates; use that payload directly when present.

```jsonc
{
  "method": "tools/call",
  "params": {
    "name": "doctor",
    "arguments": {
      "agent_id": "dev",
      "observed_tool": "seek",
      "observed_proof_state": "blocked",
      "observed_candidates": 0
    }
  }
}
```

It reports whether the active graph is empty, populated but filtered, or likely
split across host bindings/transports.

Also check `tools/list` early. If the host client exposes m1nd but hides
recovery tools such as `ingest`, call `doctor` with the observed surface before
trusting retrieval:

```jsonc
{
  "method": "tools/call",
  "params": {
    "name": "doctor",
    "arguments": {
      "agent_id": "dev",
      "observed_tool": "tools/list",
      "observed_proof_state": "blocked",
      "observed_tool_count": 3,
      "available_tools": ["seek", "audit", "doctor"],
      "missing_tools": ["ingest"]
    }
  }
}
```

When `ingest` is unavailable in the current host surface, use m1nd as
orientation only and cross-check final answers against local files until the
MCP binding is refreshed.

For local repo validation, the smoke harness now has a cheap diagnostic mode:

```bash
python3 scripts/mcp_agent_smoke.py --repo . --handshake-only --json
```

That mode calls `trust_selftest` and `session_handshake` when the binary exposes
them, and falls back to the local harness implementation for older builds. Add
`--handshake-probe` only when the task depends on retrieval trust.

### Step 4: run a first structural audit

`north` is the front door for in-session orientation; `audit` is the deeper
one-call structural pass over topology, scans, and verification. It requires the
repo root path:

```jsonc
{
  "method": "tools/call",
  "params": {
    "name": "audit",
    "arguments": {
      "agent_id": "dev",
      "path": "/path/to/your/project",
      "profile": "auto"
    }
  }
}
```

Use it when you want:

- top-level module shape
- likely risk seams
- git/filesystem verification
- a first recommendation for where to inspect next

### Step 5: ingest a document root with the universal lane

If your investigation spans specs, notes, wiki pages, or office/PDF artifacts, ingest them into the same graph instead of keeping them outside the runtime:

```jsonc
{
  "method": "tools/call",
  "params": {
    "name": "ingest",
    "arguments": {
      "agent_id": "dev",
      "path": "/path/to/your/docs",
      "adapter": "universal",
      "mode": "merge"
    }
  }
}
```

Then resolve the canonical artifact set:

```jsonc
{
  "method": "tools/call",
  "params": {
    "name": "document_resolve",
    "arguments": {
      "agent_id": "dev",
      "path": "docs/specs/auth.md"
    }
  }
}
```

This is the shortest path from “there is a doc” to “the agent can reason over the canonical local copy, bind it to code, and check drift later.”

If you see `node_count: 0`, your ingest path was wrong or the ingest did not run.

### Step 6: ask the graph something real

```jsonc
{
  "method": "tools/call",
  "params": {
    "name": "activate",
    "arguments": {
      "agent_id": "dev",
      "query": "authentication",
      "top_k": 5
    }
  }
}
```

Example response shape:

```json
{
  "query": "authentication",
  "activated": [
    {
      "node_id": "file::auth.py",
      "score": 0.89
    },
    {
      "node_id": "file::middleware.py",
      "score": 0.72
    },
    {
      "node_id": "file::session.py",
      "score": 0.61
    }
  ],
  "ghost_edges": [
    {
      "from": "file::auth.py",
      "to": "file::rate_limiter.py",
      "confidence": 0.34
    }
  ],
  "elapsed_ms": 31,
  "proof_state": "triaging"
}
```

What happened:

- seed nodes were selected from the query
- spreading activation propagated across graph structure
- the result was ranked by graph signal, not just text matching

## What to do next

After the first `activate`, these are the most useful next steps:

- `search` for exact text or regex
- `seek` for intent-based retrieval
- `impact` before touching a central file
- `surgical_context_v2` before multi-file edits
- `validate_plan` when you already know the files you want to touch

### Read the verdicts, don't override them

Retrieval and prediction return a calibrated verdict — obey it instead of
guessing past it:

- **`act` / `reverify` / `abstain`** on retrieval and prediction. `abstain`
  means uncalibrated or insufficient evidence: a STOP, not a weak yes. The
  prediction gate is armed per-repo by running `calibrate_predict` once, and the
  seek trust envelope by running `calibrate_envelope` (from the ledger's learn
  outcomes); until each is armed its verdict caps at `reverify`.
- **`why` carries a `closure` verdict** — `blocked` means the path rests on an
  unresolved edge; verify that edge before relying on the path.
- **`seek` carries a `trust_envelope` + a sufficiency stop-signal** —
  `sufficient` means stop gathering; `gathering` / `saturated` mean widen or
  refine.
- **`trust_band: insufficient_evidence` means NO evidence**, not medium risk —
  the honest cold-start answer.

### Before you finish, leave the graph warmer

Call `memorize` on every durable finding (a decision, a verified fact, why code
is the way it is, an open design point). Pass structured claims with
`confidence` and repo-relative `evidence` paths so each claim anchors to the
real code node and self-flags stale when that code changes. Closing a mission?
`mission_close(write_light_memory:true)` persists verified claims in one step.

## Troubleshooting

### “No graph snapshot found, starting fresh”

Normal on first run. The graph is empty until you call `ingest`.

### Ingest returns 0 files

Check the path and confirm it points at a real project root.

### My client cannot see the tools

Check:

- the `m1nd-mcp` path
- execute permissions on the binary
- the MCP client logs

### Learned state disappears between restarts

Set:

- `M1ND_GRAPH_SOURCE`
- `M1ND_PLASTICITY_STATE`

### I want persistent low-latency operation for a large codebase

See:

- [Deployment](../../deployment.md)

That doc covers the persistent server + stdio proxy pattern for near-zero startup overhead.
