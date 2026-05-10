# Agent Packs

`m1nd` ships an installable agent doctrine, not only an MCP binary.

The goal is universal: Codex, Claude, Gemini, Antigravity, Cursor, Cline, Roo,
Continue, OpenCode, and other MCP-capable hosts should all be able to load the
same operating model.

## What Is Included

- `skills/m1nd-first/SKILL.md` — the short first-layer doctrine.
- `skills/m1nd-operator/SKILL.md` — the deep operator manual.
- `skills/m1nd-operator/references/` — routing, tool-family, runtime-refresh,
  and `L1GHT` references.
- `skills/m1nd-universal-agent-pack.md` — portable rules for hosts without a
  native skill directory.
- `npm/bin/m1nd.js` — the npm-facing installer CLI.

## Install From A Source Checkout

```bash
npm install -g .
m1nd doctor
```

For Codex:

```bash
m1nd install-skills codex
```

For a project-local generic pack:

```bash
m1nd install-skills generic --project /path/to/project
```

For portable host files:

```bash
m1nd install-skills claude --project /path/to/project
m1nd install-skills gemini --project /path/to/project
m1nd install-skills antigravity --project /path/to/project
```

Those commands write into `/path/to/project/.m1nd/agent-pack/`. Point the host
at the generated rule file or paste it into the host custom-instructions
surface.

The portable pack includes recovery language for dead MCP transports. If a host
reports `Transport closed`, treat it as a host binding death, not as stale graph
state. Relaunch/rebind the MCP client or open a fresh session before calling
`doctor`, `recovery_playbook`, or `ingest`.

If the host is stale because it is still launching an older native binary, use
the self-update surface first:

```bash
m1nd update check --channel beta
m1nd update status --channel beta
m1nd update plan --channel beta
m1nd update apply --channel beta --yes
m1nd update verify --repo /path/to/m1nd --transport stdio
```

`check`, `status`, and `plan` are read-only. `status` is the compact cockpit for
agents: use it when you need one JSON object for package/runtime/PATH/agent-pack
readiness, visible runtime processes, and host-rebind caveats. `apply` mutates
only with `--yes`, updates the
npm package when the selected channel is ahead, installs the native runtime from
a GitHub Release binary when available, falls back to Cargo when needed,
refreshes the agent pack when allowed, and records any runtime backup for
rollback. It still cannot refresh a client's cached MCP tool list by itself;
restart or rebind the host session afterward.

During live multi-agent work, add `--no-kill` if the goal is only to update the
managed binary while keeping current host sessions alive:

```bash
m1nd update apply --channel beta --yes --no-kill
```

`m1nd restart --source /path/to/m1nd --yes` remains available as the lower-level
source-checkout repair helper for development builds.

## MCP Config Snippets

Codex:

```bash
m1nd mcp-config codex
```

Generic JSON:

```bash
m1nd mcp-config generic
```

The npm package installs doctrine, config helpers, diagnostics, and the
self-update CLI. The native runtime is still `m1nd-mcp`; use `m1nd update` for
the safe channel-aware path, or build it separately when developing from source.

From this source checkout:

```bash
cargo build --release -p m1nd-mcp
```

Then point your host at:

```text
target/release/m1nd-mcp
```

When the host supports environment variables, set:

```text
M1ND_WORKSPACE_ROOT=/path/to/project
```

That is the host-neutral workspace contract. Host-specific variables from
Claude Code, Antigravity, Gemini, Cursor, Windsurf, VS Code, and shells are
recognized as aliases, but `M1ND_WORKSPACE_ROOT` is the preferred signal.

On Windows, the native binary is `m1nd-mcp.exe`. The npm installer resolves it
in this order: `M1ND_MCP_BINARY`, `M1ND_MCP_BIN`, the managed m1nd binary path,
then `PATH`. The managed Windows path is:

```text
%USERPROFILE%\.m1nd\bin\m1nd-mcp.exe
```

Generate Windows-safe MCP snippets the same way:

```bash
m1nd mcp-config generic --binary "C:\\Users\\you\\.m1nd\\bin\\m1nd-mcp.exe"
```

The universal Windows lane is `m1nd-core` + `m1nd-ingest` + `m1nd-mcp`. The
`m1nd-openclaw` native fast path uses Unix sockets today, so Windows hosts
should use plain MCP until a Windows-native fast lane is introduced.

## Trust Loop For Every Host

0. If the host returns `Transport closed`, restart/rebind the host MCP client
   first. The transport died before m1nd could run a recovery tool.
1. Call `trust_selftest`.
2. If unavailable, call `session_handshake`.
3. If only `health` is visible, inspect `tool_surface_contract`.
4. If trust is not full, follow `recovery_playbook`.
5. Ingest the target repo or docs.
6. Use `search`, `glob`, `seek`, `activate`, `audit`, `trace`, `impact`,
   `validate_plan`, and `surgical_context_v2` before broad manual search.

The doctrine is portable because the MCP tool surface is portable.
