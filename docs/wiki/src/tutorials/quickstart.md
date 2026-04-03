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

Some clients or docs may show `m1nd.ingest`-style names. Treat that as presentation sugar. The canonical live registry names are the bare tool names shown by `tools/list`.

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

### Step 2: check server health

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

If you see `node_count: 0`, your ingest path was wrong or the ingest did not run.

### Step 3: ask the graph something real

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
