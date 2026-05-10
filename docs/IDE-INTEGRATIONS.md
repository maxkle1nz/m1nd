# IDE & Client Integration Matrix

This page tracks where major coding clients connect MCP servers, and how `m1nd`
fits into each one.

The high-level rule is simple:

- use `m1nd-mcp` for universal compatibility
- use the native hot daemon + thin proxy/client where the host supports a faster local lane

## Integration matrix

| Client | Integration surface | Config location / entrypoint | What to point at |
|---|---|---|---|
| Claude Code | MCP config file | `.claude/mcp.json` or `claude_mcp.json` | `m1nd-mcp` |
| Codex | MCP config in TOML | `~/.codex/config.toml` | `m1nd-mcp` |
| Cursor | MCP config file | `.cursor/mcp.json` (project) or `~/.cursor/mcp.json` | `m1nd-mcp` or a thin native proxy |
| Windsurf | MCP config file | Windsurf MCP settings / config JSON | `m1nd-mcp` or a thin native proxy |
| GitHub Copilot coding agent | Repository MCP config UI | repository settings UI for MCP servers | `m1nd-mcp` or an editor-facing native proxy command |
| Zed | Assistant MCP config | Zed assistant MCP configuration | `m1nd-mcp` |
| Continue | MCP / config layer | Continue MCP configuration | `m1nd-mcp` |
| Cline / Roo Code / OpenCode | MCP-compatible command config | client-specific MCP server config | `m1nd-mcp` |
| Antigravity | `mcp_config.json` | workspace-local `mcp_config.json` | native proxy recommended |

## Why some hosts should use a proxy

Most hosts are perfectly fine with:

```json
{
  "mcpServers": {
    "m1nd": {
      "command": "/path/to/m1nd-mcp",
      "env": {
        "M1ND_GRAPH_SOURCE": "/tmp/m1nd-graph.json",
        "M1ND_PLASTICITY_STATE": "/tmp/m1nd-plasticity.json",
        "M1ND_WORKSPACE_ROOT": "/path/to/your/project"
      }
    }
  }
}
```

On Windows the command should point at `m1nd-mcp.exe`, for example:

```json
{
  "mcpServers": {
    "m1nd": {
      "command": "C:\\Users\\you\\.m1nd\\bin\\m1nd-mcp.exe",
      "args": ["--stdio", "--no-gui"]
    }
  }
}
```

Codex uses the same idea, but in TOML:

```toml
[mcp_servers.m1nd]
command = "/path/to/m1nd-mcp"
args = ["--stdio", "--no-gui"]

[mcp_servers.m1nd.env]
M1ND_GRAPH_SOURCE = "/tmp/m1nd-graph.json"
M1ND_PLASTICITY_STATE = "/tmp/m1nd-plasticity.json"
M1ND_WORKSPACE_ROOT = "/path/to/your/project"
```

But some editor hosts still pay too much overhead if they cold-start a stdio MCP
server for every interaction.

For those, the better shape is:

```text
host -> thin proxy/client -> hot native daemon -> SessionState
```

That preserves host compatibility while avoiding cold process startup and graph reload
cost on every request.

## Native fast path strategy

### Universal lane

- `m1nd-mcp`
- stdio or HTTP/UI
- best for broad compatibility

### Native lane

- `m1nd-openclaw`
- persistent Unix socket bridge
- best for local runtimes that can benefit from a hot graph
- Unix/macOS/Linux lane today; use plain `m1nd-mcp` on Windows

### Thin adapters

Adapters should be very small:

- translate the host's command/stdio shape
- forward to the hot native daemon
- preserve host expectations

Examples:

- OpenClaw → native shell/bridge adapter
- Antigravity → `m1nd-antigravity-proxy.py`
- Cursor / Windsurf → project-local MCP config pointing at a native proxy if desired

## Practical recommendations

### Set the workspace root when possible

Use `M1ND_WORKSPACE_ROOT` as the portable contract for Codex, Claude Code,
Antigravity, Gemini, Cursor, Windsurf, VS Code, and generic MCP clients. Point
it at the real repository or workspace, not at a host-managed runtime folder.

Host-specific hints such as `CLAUDE_PROJECT_DIR` or
`ANTIGRAVITY_WORKSPACE_ROOT` are recognized as compatibility aliases, but
`M1ND_WORKSPACE_ROOT` is the clearest cross-host signal.

If a tool call fails with `Transport closed`, the MCP pipe died before m1nd
could run. Restart/rebind the host MCP client or open a fresh session, then
call `trust_selftest` or `session_handshake` before retrieval.

If the host is launching an old native runtime, use the external self-update
surface:

```bash
m1nd update check --channel beta
m1nd update plan --channel beta
m1nd update apply --channel beta --yes
```

For live multi-agent sessions, add `--no-kill` to update the managed binary
without stopping current host processes, then restart/rebind only the target
host. `m1nd restart --source /path/to/m1nd --yes` remains the lower-level
source-checkout repair helper for development builds.

### Use plain MCP when:

- the host already keeps the MCP server alive
- the repo is small
- setup simplicity matters more than absolute latency

### Use the native daemon lane when:

- the host repeatedly cold-starts the server
- the graph is large
- you want graph-first navigation to feel immediate

## Current m1nd-native components

- `m1nd-openclaw` — hot daemon
- `m1nd-openclaw-client` — CLI client
- `scripts/macos/m1nd-openclaw-bridge.sh` — daemon wrapper
- `scripts/macos/m1nd-openclaw-call.sh` — call wrapper
- `scripts/macos/m1nd-antigravity-proxy.py` — editor-facing stdio proxy

## Notes

- Antigravity integration is grounded from the local product schema that recognizes
  `mcp_config.json`.
- Codex, Cursor, Windsurf, Claude Code, GitHub Copilot coding agent, Zed, and Continue
  all fit the same conceptual model: host config chooses the command, env, and working directory.
