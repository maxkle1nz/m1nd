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
        "M1ND_PLASTICITY_STATE": "/tmp/m1nd-plasticity.json"
      }
    }
  }
}
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
- Cursor, Windsurf, Claude Code, GitHub Copilot coding agent, Zed, and Continue
  all fit the same conceptual model: host config chooses the command, env, and working directory.
