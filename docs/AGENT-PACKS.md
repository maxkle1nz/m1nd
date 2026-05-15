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
- `skills/m1nd-operator/references/full-spec-agent-os.md` — the full-spec
  operating layer with tool-combination recipes for broad/risky work.
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

## Default Trained-Agent Loop

Internal bug-hunt evidence showed the important distinction: `m1nd` works best
when the agent receives the operating loop, not merely a graph endpoint. The
portable pack therefore teaches every host this default sequence:

1. Establish trust with `trust_selftest`, or `session_handshake` scoped to the
   intended repo.
2. Follow `recovery_playbook` before interpreting blocked or empty retrieval.
3. Treat `wrong_workspace_binding` as a binding/scope problem, not as stale
   graph truth.
4. Orient with `audit`, then use `search`, `seek`, or `activate` for focused
   discovery.
5. Read runtime envelopes before trusting empty results.
6. Verify final truth with source files, tests, compiler/runtime output, and
   focused probes.
7. Use `impact`, `validate_plan`, and `surgical_context_v2` before risky edits
   or reviews.
8. Record tool calls, recovery paths, files inspected, commands run, and
   fallback reasons.

That loop is what `m1nd-trained` means in benchmark artifacts. It is part of
the agent pack contract.

For tiny repos, narrow reviews, or localized bug hunts, agents should use the
short-audit route instead of turning m1nd into a long investigation. Establish
trust, run one bounded recovery/ingest pass if needed, make one or two cheap
orientation calls, then switch to direct source reads, git diff, focused
runtime probes, tests, or compiler output. Record whether m1nd acted as
`short_audit_orientation` or whether it became `recovery_overhead`.

For local helper use, run the dedicated short-audit command so trust,
ingest-if-needed, one orientation query, and the direct-proof handoff stay in
one MCP process:

```bash
python3 /path/to/skills/m1nd-operator/scripts/probe_m1nd.py \
  --no-worktree-artifacts \
  --workspace-root /path/to/project \
  short-audit \
  --agent-id lane-short-audit \
  --repo /path/to/project \
  --query "focused subsystem or bug surface" \
  --tool search
```

The JSON schema is `m1nd-short-audit-helper-v0`. It is intentionally not a
final proof surface; it is a bounded orientation envelope that tells the agent
to switch to direct source/runtime proof.

For broader or harder work, escalate from the compact loop to the full-spec
operating layer at
`skills/m1nd-operator/references/full-spec-agent-os.md`. It is the route table
for the whole m1nd/L1GHT surface: architecture maps, bug hunts, docs drift,
multi-repo federation, perspectives/trails, locks, monitoring, and deep risk
tools. Agents should treat it as a decision router, not as mandatory paperwork.

If the host is stale because it is still launching an older native binary, use
the self-update surface first:

```bash
m1nd update check --channel beta
m1nd update status --channel beta
m1nd update plan --channel beta
m1nd update apply --channel beta --yes
m1nd update verify --repo /path/to/m1nd --transport stdio
m1nd hosts status --host all --project /path/to/project --json
m1nd hosts plan --host all --project /path/to/project --json
m1nd hosts apply --host all --project /path/to/project --yes --json
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

Use `m1nd hosts status` when the question is not only "is the install current?"
but "is this agent host ready?" It reports agent-pack presence, likely MCP
config wiring, runtime alignment, workspace hints, and
`host_rebind_proven=false` per supported host. It is read-only and does not edit
host config.
If host config selects an absolute current managed runtime, a stale
`m1nd-mcp` on `PATH` is a shadow warning only, not proof the host is stale. If
the host launches `PATH` or the config target is unknown, stale `PATH` is
actionable. Confirm with `hosts status` first, use `hosts plan` for the exact
rebind recipe, and do not claim a client's cached MCP tool list refreshed until
that fresh host session is actually running.

Use `m1nd hosts plan` when the status is red or workspace binding is unclear.
It emits per-host install, MCP snippet, `M1ND_WORKSPACE_ROOT`, rebind, and
verification recipes without mutating host files. The generated
`m1nd mcp-config <host> --project /path/to/project` snippets include the
workspace env explicitly.

Use `m1nd hosts apply` only as the local mutating follow-through after
`hosts status` or `hosts plan`. Without `--yes` it remains a dry-run preview.
With `--yes`, it can install or refresh agent-pack files and write canonical
MCP config snippets for known hosts. It still does not prove rebind, refresh an
already-open host's cached tool list, repair graph state, or remove the manual
config step for generic hosts.

During live multi-agent work, add `--no-kill` if the goal is only to update the
managed binary while keeping current host sessions alive:

```bash
m1nd update apply --channel beta --yes --no-kill
```

`m1nd restart --source /path/to/m1nd --yes` remains available as the lower-level
source-checkout repair helper for development builds.

For benchmark lanes or narrow write scopes, use the operator helper with
`--no-worktree-artifacts`:

```bash
python3 /path/to/skills/m1nd-operator/scripts/probe_m1nd.py \
  --no-worktree-artifacts \
  --workspace-root /path/to/project \
  run '[{"name":"ingest","arguments":{"agent_id":"lane","path":"/path/to/project"}}]'
```

This keeps probe runtime state in the isolated runtime directory while
setting `M1ND_WORKSPACE_ROOT` to the inspected repo. If `--workspace-root` is
omitted, the helper uses the caller directory. It avoids accidental
`graph_snapshot.json`, `ingest_roots.json`, or `plasticity_state.json` files in
the audited repo. The helper prefers the managed runtime at
`~/.m1nd/bin/m1nd-mcp` before a potentially stale `m1nd-mcp` on `PATH`.

## MCP Config Snippets

Codex:

```bash
m1nd mcp-config codex --project /path/to/project
```

Generic JSON:

```bash
m1nd mcp-config generic --project /path/to/project
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
Prefer it in host config so the binding does not drift to `OLDPWD` or another
ambient workspace hint.

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
