---
name: m1nd-first
description: Use when investigating a repository, searching for implementation, reviewing changes, working from specs/docs, or preparing a risky code change in an environment where m1nd is available. This doctrine makes m1nd the first investigative layer before grep, glob, or manual file reads, except when the task is pure compiler/runtime truth or the exact file and lines are already known.
---

# m1nd-first

This is a doctrine, not a manual.

## Rules

- Start with `m1nd`.
- Before `rg`, shell globbing, or manual file reads, first ask whether `m1nd` can answer or narrow the task directly.
- Prefer the cheapest `m1nd` surface that preserves truth:
  - exact text -> `search`
  - path pattern -> `glob`
  - known purpose, unknown location -> `seek`
  - topic, subsystem, or connected neighborhood -> `activate`
  - unfamiliar repo orientation -> `audit`
  - stacktrace or runtime error text -> `trace`
- For docs/specs/knowledge:
  - authored as `L1GHT` -> `ingest` with `adapter: "light"`
  - ordinary docs/wiki/PDF/office docs -> `ingest` with `adapter: "universal"` or `adapter: "auto"`
- Before risky edits or change reviews, pass through `impact`, `validate_plan`, and usually `surgical_context_v2`.
- Keep `agent_id` stable across one investigation unless intentionally splitting roles.

## Skip Conditions

Skip the `m1nd` first pass only when:

- the user already gave the exact file and exact lines
- the question is compiler, test, or runtime truth rather than structure
- the task is a trivial local file action with no structural uncertainty

## Fallback

If `m1nd` does not answer enough, then fall back to shell search, direct file reads, compiler output, tests, logs, and debugger data.

If a local helper/probe fails before MCP initialization with
`runtime_root ... is already owned by instance`, classify it as a sidecar
runtime lock collision, not stale graph truth. Use the current
`probe_m1nd.py`, which isolates `--runtime-dir` by default, pass a unique
explicit `--runtime-dir`, or collapse dependent checks into one
`probe_m1nd.py run` call. Do not tell the user that m1nd retrieval is broken
until a fresh isolated probe or the repo-local smoke harness also fails.

If a tool call fails with `Transport closed`, classify it as a host MCP binding
failure before m1nd can run. Do not call `doctor`, `recovery_playbook`, or
`ingest` through that dead binding. Verify the binary with a local smoke, kill
stale `m1nd-mcp --stdio` processes if you own the host, and restart/rebind the
agent host or open a fresh thread so the MCP client launches a new transport.
After rebind, run `trust_selftest` or `session_handshake` before trusting
retrieval.

If the host appears to be launching an old native runtime, use the external CLI
self-update path:

```bash
m1nd update check --channel beta
m1nd update status --channel beta
m1nd update plan --channel beta
m1nd update apply --channel beta --yes
m1nd hosts status --host all --project /path/to/project --json
m1nd hosts plan --host all --project /path/to/project --json
```

Use `--no-kill` in live multi-agent sessions when you only want to update the
managed binary and rebind one selected host. `m1nd update` does not ingest,
repair graph contents, choose a workspace, or refresh an already-open client's
cached MCP tool list. `m1nd restart --source /path/to/m1nd --yes` remains the
lower-level source-checkout repair path for development builds.

When the question is host-specific, use `m1nd hosts status` before mutating
anything. It is read-only and reports agent-pack files, likely MCP config
wiring, runtime/PATH alignment, workspace hints, and `host_rebind_proven=false`
per supported host.
If host config points to an absolute current managed runtime, a stale
`m1nd-mcp` on `PATH` is only a shadow warning, not proof that the host is
stale. If the host launches `PATH` or config is unknown, treat a stale `PATH`
runtime as actionable. Then use `m1nd hosts plan` and rebind or open a fresh
host session; do not claim the client's cached tool list refreshed by itself.
If it reports `attention`, call `m1nd hosts plan` for the exact per-host
install, MCP-config, `M1ND_WORKSPACE_ROOT`, rebind, and verification recipe.

For local m1nd repo work, prefer the cheap trust selftest path before a full smoke:

```bash
python3 scripts/mcp_agent_smoke.py --repo . --handshake-only --json
```

When the live MCP surface exposes `trust_selftest`, call that tool first:

```json
{"agent_id":"codex-m1nd"}
```

Treat its `verdict` as the session routing decision before relying on
retrieval. If the verdict is not `full_trust`, follow the embedded
`recovery_playbook` or call `recovery_playbook` with the same evidence before
guessing the next move. The selftest is diagnostic-only: no ingest, repair,
host refresh, graph mutation, or retrieval probe happens automatically.

When `seek`, `search`, `activate`, or `panoramic` includes
`agent_runtime_contract`, read that envelope before interpreting results. It is
the agent-facing runtime identity contract: `trust_mode` tells whether this is
`full_trust`, `needs_ingest`, `wrong_workspace_binding`, or
`retrieval_needs_recovery`; `workspace_binding` shows whether the requested
scope belongs to the active workspace; `graph_identity` shows the exact graph
generation and counts; and `recovery.arguments` is the payload to pass directly
to `recovery_playbook`. Empty result arrays are not final truth until the
contract says the runtime/workspace/graph identity is coherent.

If the verdict is `needs_ingest`, or `graph_state.node_count` is `0` while
`ingest` is available, treat it as a recoverable cold graph, not as a reason to
abandon m1nd. Call `ingest` on the same MCP binding with the absolute path of
the intended repo/workspace, never a managed runtime/session path such as
`~/.codex/m1nd-runtimes/...`, `~/.claude/m1nd-runtimes/...`, an Antigravity
agent runtime, or a generic `mcp-runtimes`/`agent-runtimes` folder. Host
integrations should prefer `M1ND_WORKSPACE_ROOT`; m1nd also recognizes common
workspace hints from Claude Code, Antigravity, Gemini, Cursor, Windsurf, VS
Code, and shell/package-manager env vars. Then rerun `session_handshake` and
one cheap retrieval. Fall back to direct files only when ingest is unavailable,
ingest fails, or a post-ingest retrieval still reports `blocked` and
`recovery_playbook`/`doctor` confirms stale binding or degraded host surface.

If `trust_selftest`, `session_handshake`, `recovery_playbook`, `doctor`,
`validate_plan`, or a retrieval response includes
`context_guard.wrong_workspace_binding=true`, do not classify the repo graph as
stale. The active binding is pointed at one workspace while the call asked about
another. Follow the embedded `recovery_playbook` payload, rebind the host with
`M1ND_WORKSPACE_ROOT` set to `requested_workspace_hint`, or explicitly ingest
that workspace on the same binding if the switch is intentional. Use
`federate_auto`/`federate` only when the task is genuinely cross-repo.

If `trust_selftest` is not exposed but `session_handshake` is, call the cheaper
sub-check:

```json
{"agent_id":"codex-m1nd"}
```

When the task names a target repo or absolute path, include it as `scope`:

```json
{"agent_id":"codex-m1nd","scope":"/path/to/intended/repo"}
```

Treat its `trust_mode` as the session routing decision before relying on
retrieval. If the mode is not `full_trust`, call `recovery_playbook` before
guessing the next move. The playbook returns ordered recovery steps and a
binding fingerprint without ingesting, repairing, or probing automatically.

Use `--handshake-probe` only when retrieval trust itself matters. The plain
selftest/handshake path should stay cheap: no ingest, no repair, and no
retrieval probe by default. The repo-local smoke harness calls `trust_selftest`
and `session_handshake` when available and falls back to its built-in handshake
for older binaries.

If the host exposes `health` but not `trust_selftest`, `session_handshake`, or
`recovery_playbook`, read `health.tool_surface_contract` and
`health.host_binding_alignment`.
That is enough to classify the binding as partial/degraded and switch to local
smokes or direct file truth until the host refreshes its tool surface.

If `m1nd` is visible but the host tool surface is missing recovery tools such as
`ingest`, treat it as `degraded_host_tool_surface`, not as a normal graph
failure. Use whatever m1nd can still provide for orientation, but verify final
truth against local files until the MCP binding is refreshed. If
`recovery_playbook` is available, call it with the tool surface:

```json
{"agent_id":"codex-m1nd","observed_tool":"tools/list","observed_proof_state":"blocked","observed_tool_count":3,"available_tools":["seek","audit","doctor"],"missing_tools":["ingest"]}
```

If an `ingest` call appears to succeed but a follow-up retrieval call such as
`seek`, `search`, or `activate` returns `blocked`, zero candidates, or an empty
graph unexpectedly, do not assume the codebase is unindexed. Treat it as a
possible host-binding/session-continuity problem. If `recovery_playbook` is
available, use `recovery.arguments` from the retrieval response when present.
If the response does not include a recovery payload, call `recovery_playbook`
with the suspicious output first. Let the playbook decide when to call
`doctor`:

```json
{"agent_id":"codex-m1nd","observed_tool":"seek","observed_proof_state":"blocked","observed_candidates":0}
```

If the repo has a local m1nd checkout, verify the real runtime with its stdio
and HTTP smoke harness before deciding:

```bash
python3 scripts/mcp_agent_smoke.py --repo . --json
python3 scripts/mcp_agent_smoke.py --repo . --transport http --json
```

## Deep Manual

If the task needs detailed routing, `L1GHT` semantics, document-lane choice, multi-agent coordination, or refresh procedures, consult:

- the companion `m1nd-operator` skill installed with this pack, usually at
  `m1nd-operator/SKILL.md` in the same skills root.
