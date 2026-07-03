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

1. Orient with `north(task)` — the in-session front door. One round-trip returns
   binding trust, task context (focus nodes + anchors), prior cross-session
   memory, a sufficiency signal, one `next_move`, and `honest_gaps`. If it
   returns `needs_ingest`, `ingest` the repo, then `north` again. `north`
   composes `trust_selftest` + `orient` + `boot_memory` + `focus`.
2. If trust is degraded or retrieval is blocked/empty unexpectedly, drop to
   `recovery_playbook`. Treat `wrong_workspace_binding` as a binding/scope
   problem, not as stale graph truth. If the host cannot be rebound immediately,
   use an isolated `m1nd agent orient --repo /path/to/intended/repo --mode short
   --json` before raw shell search.
3. Classify partial bindings: `nested_workspace_binding` means subtree-only
   truth, and `file_level_binding` means docs/PRD/L1GHT truth only.
4. Follow the `next_move`, or use `search`, `seek`, or `activate` for focused
   discovery. Obey the calibrated verdict: `act`/`reverify`/`abstain` (abstain =
   STOP, not a weak yes; the gate is armed by `calibrate_predict` once, else caps
   at `reverify`); `why` carries a `closure` verdict (`blocked` = the path rests
   on an unresolved edge); `seek` carries a `trust_envelope` + sufficiency signal;
   `trust_band: insufficient_evidence` means NO evidence, not medium risk.
5. Read runtime envelopes before trusting empty results.
6. Verify final truth with source files, tests, compiler/runtime output, and
   focused probes.
7. Use `impact`, `validate_plan`, and `surgical_context_v2` before risky edits
   or reviews.
8. Close warmer than you found it: `memorize` every durable finding (with
   `confidence` and repo-relative `evidence` paths), then leave one
   field-telemetry signal — `learn(correct|wrong|partial)`, or one JSON line in
   `~/.m1nd/field-reports.jsonl` when m1nd itself misbehaves (local-only; never
   fix m1nd mid-mission — report). Record tool calls, recovery paths, files
   inspected, and commands run.

That loop is what `m1nd-trained` means in benchmark artifacts. It is part of
the agent pack contract. The installed skills (`m1nd-first`, `m1nd-operator`,
and the portable `m1nd-universal-agent-pack`) all teach this same OMEGA loop —
`north`-first orientation, calibrated verdicts, and memorize-at-close — so every
door an agent enters by teaches one doctrine.

When the live runtime exposes Mission Control, broad reviews, bug hunts, and
risky refactors can make the trained loop explicit with mission tools:
`mission_start`, `mission_event`, `mission_next`, `mission_verify`,
`mission_handoff`, and `mission_close`. `mission_start` creates a repo-scoped
route and budget; `mission_event` records direct or graph evidence with an event
id; `mission_next` gives one move plus `do_not` guardrails; `mission_verify`
rejects graph-only claims unless a claim references direct evidence; and
`mission_handoff`/`mission_close` emit resumable packets with verified claims,
rejected claims, gaps, event digests, and non-claims. This is an operating-loop
contract, not proof that the host was rebound, the graph was repaired, or
retrieval became correct.

For `bug_hunt`, follow any `direct_sweep` move before closing. It is a direct
negative-space pass over public contracts/docs, boundary values, error paths,
async/concurrency behavior, and helper/exported APIs.

For tiny repos, narrow reviews, or localized bug hunts, agents should use the
short-audit route instead of turning m1nd into a long investigation. Establish
trust, run one bounded recovery/ingest pass if needed, make one or two cheap
orientation calls, then switch to direct source reads, git diff, focused
runtime probes, tests, or compiler output. Record whether m1nd acted as
`short_audit_orientation` or whether it became `recovery_overhead`.

For local helper use, prefer the first-class agent CLI. It starts an isolated
runtime, binds `M1ND_WORKSPACE_ROOT` to the repo, keeps worktree artifacts out
of the inspected project, and returns a single JSON envelope:

```bash
m1nd agent first-minute \
  --repo /path/to/project \
  --query "understand this system" \
  --json

m1nd agent next \
  --repo /path/to/project \
  --query "focused subsystem or bug surface" \
  --json

m1nd agent orient \
  --repo /path/to/project \
  --query "focused subsystem or bug surface" \
  --mode short \
  --json
```

The outer JSON schema is `m1nd-agent-cli-v0`. `m1nd agent first-minute` is the
host-neutral CLI escape hatch for first contact when the live MCP session is
stale, wrong-bound, or not loaded yet — not the in-session front door (`north`
is). For broad
"understand/audit/map this repo" tasks in that out-of-session case it scopes,
trusts, ingests when needed, returns anchors, and emits `do_not` guardrails
before handing the agent to direct proof. `agent next`/`agent auto` emits an
inner `m1nd-agent-action-envelope-v0` so an agent can choose the first safe move
without memorizing the full tool matrix. `agent context` is anchor-first; use it
after a concrete file, path, or identifier is known, not as the first narrative
question. `agent orient` is intentionally not a final proof surface; it is a
bounded orientation envelope that tells the agent to switch to direct
source/runtime proof. The older
`skills/m1nd-operator/scripts/probe_m1nd.py short-audit` route remains available
for compatibility when a host has not installed the npm CLI.

Use the adjacent agent commands for common recovery flows:

```bash
m1nd agent scope --repo /path/to/project --json
m1nd agent trust --repo /path/to/project --ensure-ingest --json
m1nd agent auto --repo /path/to/project --from "Transport closed" --json
m1nd agent recover --repo /path/to/project --from wrong_workspace_binding --json
m1nd agent doctor --repo /path/to/project --json
```

Validate the packaged routing doctrine with:

```bash
m1nd pack-routing-check --json
```

This gate checks that the distributed skills and docs still teach the intended
split between session companions, `m1nd agent next`, m1nd MCP tools, and direct
proof. It is text-contract validation, not proof that any live host or companion
is installed.

## Session Memory Companions

Some agent hosts may also expose a session-memory companion such as DEXT3R.
That layer is useful for continuity, not for final code truth.

Use a companion to recover the session north star, prior decisions, open loops,
handoff context, or a scoped `m1nd flash` attached to the active project. Before
using it, confirm that the companion session is bound to the same repo or
project root as the task.

Do not treat companion global search, session summaries, or flash output as
proof that code behavior is correct. If the companion reports missing scope,
wrong project, unavailable flash, or global-only candidates, record it as
`companion_orientation_only` and continue with the m1nd agent CLI:

```bash
m1nd agent next --repo /path/to/project --query "current task" --json
```

The intended split is:

```text
session companion -> continuity and prior decisions
m1nd agent next    -> first safe repo move
m1nd MCP tools     -> structural graph/docs/impact/mission context
direct proof       -> source, tests, compiler/runtime output, logs, CI, smoke
```

For broader or harder work, escalate from the compact loop to the full-spec
operating layer at
`skills/m1nd-operator/references/full-spec-agent-os.md`. It is the route table
for the whole m1nd/L1GHT surface: architecture maps, bug hunts, docs drift,
multi-repo federation, perspectives/trails, locks, monitoring, and deep risk
tools. Agents should treat it as a decision router, not as mandatory paperwork.

For deep architecture/risk work, the compact packs should surface the
RETROBUILDER family directly instead of relying on the agent to remember the
full reference: `ghost_edges` for hidden historical co-change, `taint_trace` for
trust-boundary or sensitive flow, `twins` for structural duplication,
`refactor_plan` for extraction communities, and `runtime_overlay` for span/log
heat mapped onto graph nodes. `m1nd agent first-minute`, `next`, `orient`, and
`context` may emit these as `capability_suggestions`. They are graph
orientation and hypothesis tools; direct source reads, tests, compiler/runtime
output, logs, or focused probes remain final truth.

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
workspace env explicitly. `plan`/`apply` now also cover the ambient layer:
per-host SessionStart-family hooks (`SessionStart`/`agentSpawn`/`TaskStart`,
routed through the `m1nd-north-shim` command) and per-host doctrine files for
the TIER-A and TIER-B hosts.

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

For benchmark lanes or narrow write scopes, prefer the agent CLI:

```bash
m1nd agent trust --repo /path/to/project --ensure-ingest --json
```

This keeps runtime state in the isolated runtime directory while setting
`M1ND_WORKSPACE_ROOT` to the inspected repo. If `--repo` is omitted, the CLI
uses the caller directory. It avoids accidental
`graph_snapshot.json`, `ingest_roots.json`, or `plasticity_state.json` files in
the audited repo. The CLI prefers the managed runtime at `~/.m1nd/bin/m1nd-mcp`
before a potentially stale `m1nd-mcp` on `PATH`.

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

## Front Door For Every Host

In a live MCP session, call `north(task)` first — one round-trip for trust, task
context, prior memory, sufficiency, one `next_move`, and `honest_gaps`. If it
returns `needs_ingest`, `ingest` the target repo or docs, then `north` again.
Then use `search`, `glob`, `seek`, `activate`, `trace`, `impact`,
`validate_plan`, and `surgical_context_v2` before broad manual search — obeying
the calibrated verdicts (`act`/`reverify`/`abstain`, `closure`, `trust_envelope`,
`insufficient_evidence`).

Drop to the trust-only recovery sub-checks when the binding looks degraded:

0. If the host returns `Transport closed`, restart/rebind the host MCP client
   first. The transport died before m1nd could run a recovery tool.
1. Call `trust_selftest`.
2. If unavailable, call `session_handshake`.
3. If only `health` is visible, inspect `tool_surface_contract`.
4. If trust is not full, follow `recovery_playbook`.

The doctrine is portable because the MCP tool surface is portable.
