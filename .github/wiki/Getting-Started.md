# Getting Started

Get m1nd running and fire your first graph query in under 30 seconds.

---

## Prerequisites

- Rust toolchain (`cargo`) — install via [rustup.rs](https://rustup.rs)
- An MCP-compatible client: Claude Code, Cursor, Windsurf, Zed, or any MCP client

---

## Installation

### Option 1 — Build from source (recommended)

```bash
git clone https://github.com/cosmophonix/m1nd.git
cd m1nd
cargo build --release
```

The binary is at `./target/release/m1nd-mcp`. ~8MB, no runtime dependencies.

**With full language support (27+ languages):**

```bash
cargo build --release --features tier1,tier2
```

Default build includes Python, Rust, TypeScript/JavaScript, Go, Java plus a generic fallback. `--features tier1` adds C, C++, C#, Ruby, PHP, Swift, Kotlin, Scala, Bash, Lua, R, HTML, CSS, JSON. `--features tier2` adds Elixir, Dart, Zig, Haskell, OCaml, TOML, YAML, SQL.

### Option 2 — From crates.io (library use)

```toml
[dependencies]
m1nd-core = "0.4"
```

---

## First Query in 3 Steps

m1nd runs as a JSON-RPC stdio server. Any MCP client sends tool calls to it.

**Step 1 — Ingest your codebase**

```jsonc
{"method":"tools/call","params":{"name":"m1nd.ingest","arguments":{
  "path":"/your/project",
  "agent_id":"dev"
}}}
```

Response (910ms for 335 files):
```json
{
  "files_processed": 335,
  "nodes_created": 9767,
  "edges_created": 26557,
  "languages": {"python": 335},
  "elapsed_ms": 910
}
```

**Step 2 — Ask "what's related to authentication?"**

```jsonc
{"method":"tools/call","params":{"name":"m1nd.activate","arguments":{
  "query":"authentication",
  "agent_id":"dev"
}}}
```

Response (31ms):
```json
{
  "activated": [
    {"node_id": "file::auth.py", "score": 0.89},
    {"node_id": "file::middleware.py", "score": 0.78},
    {"node_id": "file::session_pool.py", "score": 0.61}
  ],
  "ghost_edges": [
    {"from": "file::auth.py", "to": "file::jwt_utils.py", "confidence": 0.42}
  ]
}
```

Ghost edges are undocumented connections inferred from co-change history — invisible to grep.

**Step 3 — Tell the graph what was useful**

```jsonc
{"method":"tools/call","params":{"name":"m1nd.learn","arguments":{
  "feedback":"correct",
  "node_ids":["file::auth.py","file::middleware.py"],
  "agent_id":"dev"
}}}
```

Response (<1ms):
```json
{"edges_strengthened": 740, "ltp_applied": true}
```

740 edges strengthen via Hebbian LTP. Next query converges faster.

---

## Add to Claude Code

Edit your Claude Code MCP config (`.claude/mcp.json` or `claude_mcp.json`):

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

Replace `/path/to/m1nd-mcp` with the absolute path to your built binary.

Once added, all 61 m1nd tools appear in Claude Code as `mcp__m1nd__*` tools.

**Recommended session pattern in Claude Code:**

```
Session start:
  mcp__m1nd__health        → verify server alive
  mcp__m1nd__drift         → check what changed since last session
  mcp__m1nd__ingest        → load codebase (or verify graph is current)

During work:
  mcp__m1nd__activate      → find what's relevant BEFORE Grep/Glob
  mcp__m1nd__impact        → check blast radius BEFORE editing
  mcp__m1nd__learn         → feed back what was correct/wrong

Session end:
  mcp__m1nd__trail.save    → persist investigation state
```

---

## Add to Cursor / Windsurf / Zed

Any MCP-compatible editor uses the same config pattern:

```json
{
  "mcpServers": {
    "m1nd": {
      "command": "/path/to/m1nd-mcp",
      "args": [],
      "env": {
        "M1ND_GRAPH_SOURCE": "/home/user/.m1nd/graph.json"
      }
    }
  }
}
```

## IDE integration matrix

Different hosts expose different config entrypoints:

- Claude Code → `.claude/mcp.json`
- Cursor → `.cursor/mcp.json`
- Windsurf → MCP config UI / JSON surface
- GitHub Copilot coding agent → repository MCP config UI
- Zed → assistant MCP config
- Antigravity → workspace-local `mcp_config.json`

For the full matrix and native-daemon strategy, see [`docs/IDE-INTEGRATIONS.md`](../docs/IDE-INTEGRATIONS.md).

---

## Config File

Pass a JSON config file as the first CLI argument to override defaults at startup:

```bash
./target/release/m1nd-mcp /path/to/config.json
```

```json
{
  "graph_source": "/path/to/graph.json",
  "plasticity_state": "/path/to/plasticity.json",
  "domain": "code",
  "xlr_enabled": true,
  "auto_persist_interval": 50,
  "max_perspectives_per_agent": 10,
  "max_locks_per_agent": 10
}
```

All fields are optional. CLI config takes precedence over environment variables.

---

## Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------| 
| `M1ND_GRAPH_SOURCE` | Path to persist graph state (JSON) | In-memory only |
| `M1ND_PLASTICITY_STATE` | Path to persist plasticity weights | In-memory only |
| `M1ND_XLR_ENABLED` | Enable/disable XLR noise cancellation | `true` |

When `M1ND_GRAPH_SOURCE` is set, additional state files are written alongside it automatically:

| File | Contents |
|------|---------|
| `antibodies.json` | Bug antibody pattern registry |
| `tremor_state.json` | Change acceleration observation history |
| `trust_state.json` | Per-module defect history ledger |
| `graph_snapshot.json` | Human-readable graph snapshot |
| `plasticity_state.json` | Hebbian weights |

**Persistence recommendation:** Set both `M1ND_GRAPH_SOURCE` and `M1ND_PLASTICITY_STATE` to the same directory. The graph and plasticity weights persist across sessions — so the graph continues learning.

---

## Domain Presets

The `domain` config field tunes temporal decay half-lives and recognized relation types:

| Domain | Temporal half-lives | Use for |
|--------|--------------------|---|
| `code` (default) | File=7d, Function=14d, Module=30d | Software codebases |
| `memory` | Tuned for knowledge decay | Agent session memory, notes, PRDs |
| `music` | Music-specific relations, no git co-change | Signal chains, patch graphs |
| `generic` | Flat decay | Any custom domain |

---

## Ingest Adapters

The `ingest` tool supports three adapters:

**Code (default)** — parses source files, resolves cross-file edges, enriches with git history:
```jsonc
{"name":"m1nd.ingest","arguments":{"path":"/your/project","agent_id":"dev"}}
```

**Memory / Markdown** — ingest `.md`, `.txt`, `.markdown` files into the graph:
```jsonc
{"name":"m1nd.ingest","arguments":{
  "path":"/your/docs",
  "adapter":"memory",
  "namespace":"docs",
  "mode":"merge",
  "agent_id":"dev"
}}
```
Headings become `Module` nodes. Bullet entries become `Process`/`Concept` nodes. Cross-references produce `Reference` edges. After merge, `activate()` queries span both code and docs in one pass.

**JSON (domain-agnostic)** — describe any graph in JSON:
```jsonc
{"name":"m1nd.ingest","arguments":{
  "path":"/your/domain.json",
  "adapter":"json",
  "agent_id":"dev"
}}
```

---

## Merge vs Replace

The `mode` parameter controls how ingested nodes merge with the existing graph:

- `"replace"` (default) — clears the existing graph and ingests fresh
- `"merge"` — overlays new nodes onto existing graph (tag union, weight max-wins)

Use `"merge"` when combining code + docs, or when doing incremental re-ingestion after a small change.

---

## Verified Writes

`apply_batch` supports a `verify` flag that re-reads each file immediately after writing and compares the on-disk content against the intended content. This catches silent failures — encoding issues, partial writes, permission races — before they propagate downstream.

**Example — write two files and verify both landed:**

```jsonc
{"method":"tools/call","params":{"name":"m1nd.apply_batch","arguments":{
  "agent_id": "dev",
  "files": [
    {
      "file_path": "src/auth.py",
      "new_content": "# updated auth module\n...",
      "description": "Add token refresh logic"
    },
    {
      "file_path": "src/middleware.py",
      "new_content": "# updated middleware\n...",
      "description": "Wire token refresh into middleware"
    }
  ],
  "verify": true
}}}
```

Response:
```json
{
  "results": [
    {"file_path": "src/auth.py", "written": true, "verified": true, "elapsed_ms": 12},
    {"file_path": "src/middleware.py", "written": true, "verified": true, "elapsed_ms": 9}
  ],
  "all_verified": true,
  "failed": []
}
```

If verification fails for any file, that entry appears in `"failed"` with the diff between expected and actual content.

> **Note:** `verify: true` adds ~200–400ms overhead per batch but catches silent failures before they propagate. Recommended for any multi-file write where correctness matters. See [API Reference — apply_batch](API-Reference#surgical-4-tools) for the full schema.

**Accuracy:** 12/12 in production testing — every write that m1nd verifies passes exactly, and every silent failure is detected.

---

## Node ID Reference

m1nd assigns deterministic IDs during ingest. These are used in `activate`, `impact`, `why`, and other targeted tools:

```
Code nodes:
  file::<relative/path.py>
  file::<relative/path.py>::class::<ClassName>
  file::<relative/path.py>::fn::<function_name>
  file::<relative/path.py>::struct::<StructName>
  file::<relative/path.py>::enum::<EnumName>

Memory nodes:
  memory::<namespace>::file::<file-slug>
  memory::<namespace>::section::<file-slug>::<heading-slug>-<n>
  memory::<namespace>::entry::<file-slug>::<line-no>::<entry-slug>
  memory::<namespace>::reference::<referenced-path-slug>

JSON nodes:
  <user-defined>  (whatever id you set in the JSON descriptor)
```

---

## When NOT to Use m1nd

- **You need neural semantic search.** V1 uses trigram matching, not embeddings. "Find code that *means* authentication but never uses the word" won't work yet.
- **Your language isn't supported.** Default build: Python, Rust, TypeScript/JS, Go, Java + generic fallback. Full 28-language support requires `--features tier1,tier2`.
- **400K+ files.** The graph is in-memory. ~2MB for 10K nodes, so 400K files ~= ~80MB. It works but isn't the primary optimization target.
- **You need dataflow or taint analysis.** m1nd tracks structural and co-change relationships, not data flow through variables.

---

---

## Configure Your Agent

m1nd is designed to replace grep, glob, and blind file reads for AI agents. Add these instructions to your agent's system prompt so it uses m1nd as its primary code navigation tool.

### System prompt snippet (copy-paste ready)

```
You have m1nd available via MCP. Use it BEFORE grep, glob, or file reads:
- m1nd.search(mode="literal") replaces grep — finds exact strings with graph context
- m1nd.activate replaces glob — finds related code by meaning, not filename
- m1nd.surgical_context_v2 replaces Read — returns source + all connected files in one call
- m1nd.impact replaces manual dependency checking — shows blast radius before edits
- m1nd.apply replaces Edit — writes code and auto-updates the graph
- m1nd.apply_batch with verify=true — write multiple files and verify each one landed
- m1nd.help() — call when unsure which tool to use
```

### For Claude Code users

Add to your project's `CLAUDE.md`:

```markdown
## Code Intelligence
m1nd is your primary code navigation tool. Use it before grep/glob/Read.
Key tools: search (grep replacement), activate (find related), surgical_context_v2 (full context),
impact (blast radius), apply (edit + re-ingest), apply_batch (multi-file + verify), help (when confused).
```

### For Cursor users

Add to your `.cursorrules`:

```
When exploring code, use m1nd MCP tools instead of grep:
- m1nd.search for finding code
- m1nd.activate for understanding relationships
- m1nd.impact before making changes
- m1nd.apply_batch with verify=true for multi-file writes
```

### For any MCP client

Any MCP-compatible tool (Windsurf, Zed, Cline, Roo Code, Continue, OpenCode, Amazon Q) uses the same config pattern. Add the system prompt instructions above to your agent's configuration. Once the MCP server is connected, all m1nd tools appear automatically.

### Why this matters

AI agents waste 80% of their context window navigating code with grep and file reads. m1nd answers the same questions in microseconds at zero token cost. In our testing, switching from grep to m1nd reduced token usage by 80% and found 8 bugs that grep could never find — because they existed in the *absence* of code, not in its presence.

### Recommended agent workflow

```
1. Session start:
   m1nd.health          → verify server alive
   m1nd.ingest           → load codebase into graph
   m1nd.drift            → check what changed since last session

2. Before any code exploration:
   m1nd.search / m1nd.activate  → find relevant code (replaces grep/glob)
   m1nd.surgical_context_v2     → get full context (replaces Read)

3. Before any edit:
   m1nd.impact           → check blast radius
   m1nd.validate_plan    → assess risk

4. After edits:
   m1nd.apply / m1nd.apply_batch(verify=true)  → write + re-ingest + verify
   m1nd.predict           → check co-change predictions
   m1nd.learn             → feed back what was correct/wrong

5. Session end:
   m1nd.trail.save        → persist investigation state
```

---

## Next Steps

- [API Reference](API-Reference) — all 61 tools with schemas, examples, and benchmarks
- [Home](Home) — overview, key numbers, common workflows
- [EXAMPLES.md](../EXAMPLES.md) — raw output from a production codebase
