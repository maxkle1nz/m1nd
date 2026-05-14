# m1nd Universal Agent Pack

Use this pack in any coding-agent host that can read project rules, custom
instructions, memory, or prompt files.

## Prime Directive

Use `m1nd` as the first investigative layer before raw grep, filesystem
globbing, or manual file reads when the task depends on repository structure,
docs, impact, continuity, or risky change preparation.

Skip the first pass only when the user gave exact file/line truth, when compiler
or runtime output is the only source of truth, or when the task is a trivial
local file action.

## Startup Trust Loop

0. If a tool call fails with `Transport closed`, stop treating it as graph
   staleness. The MCP transport died before m1nd could run. Verify the binary
   with a local smoke, restart/rebind the host MCP client or open a fresh
   thread, then continue with this loop.
1. Call `trust_selftest` if the host exposes it.
2. If unavailable, call `session_handshake`.
3. If only `health` is exposed, inspect `tool_surface_contract` and
   `host_binding_alignment`.
4. If trust is not full, call or follow `recovery_playbook`.
5. Only then rely on retrieval surfaces such as `search`, `seek`, or `activate`.

When the task names a concrete repo, worktree, or absolute path, pass it as
`scope` to `session_handshake`, `trust_selftest`, `recovery_playbook`, `doctor`,
or `validate_plan`. If the response reports
`context_guard.wrong_workspace_binding=true` or trust mode
`wrong_workspace_binding`, the host is bound to a different workspace than the
one you asked about. Rebind with `M1ND_WORKSPACE_ROOT`, intentionally ingest the
requested workspace on the same binding, or use explicit federation for real
cross-repo work. Do not call this graph staleness.

`trust_selftest` and `recovery_playbook` are diagnostic. They do not ingest,
repair, refresh the host, or mutate the graph.

## Default Trained Investigation Loop

For unfamiliar repo work, audits, bug hunts, reviews, and risky changes, use
this loop by default:

1. Establish trust with `trust_selftest`, or `session_handshake` with the
   intended repo/workspace as `scope`.
2. If trust is not full, follow `recovery_playbook` before interpreting empty
   retrieval. `wrong_workspace_binding` means rebind, intentional ingest, or
   real federation; it is not stale graph proof.
3. Orient with `audit` for unfamiliar repos, or route focused questions through
   `search`, `seek`, or `activate`.
4. Read the runtime envelope on retrieval responses. Empty result arrays are
   not final truth until workspace binding, graph identity, and recovery state
   are coherent.
5. Verify with direct source, tests, compiler/runtime output, and focused
   probes. `m1nd` narrows and connects; execution truth still comes from the
   repo.
6. Before edits or reviews, run `impact`, `validate_plan`, and usually
   `surgical_context_v2`.
7. Record what happened: m1nd calls, recovery path, files inspected, commands
   run, and fallback reasons.

This is the trained-agent behavior to preserve across hosts: m1nd is graph plus
operating doctrine, not graph alone.

## Full-Spec Escalation

For broad audits, hard bug hunts, multi-repo systems, docs/L1GHT work,
long-running investigations, security/risk review, or when the user asks for
the full m1nd system, load the full operating layer:

`skills/m1nd-operator/references/full-spec-agent-os.md`

Treat it as a route table, not a checklist. The compact pack gets you moving;
the full-spec layer tells you which m1nd/L1GHT tool combination to use for each
situation.

## Tool Routing

- Exact text -> `search`
- Path pattern -> `glob`
- Known purpose, unknown location -> `seek`
- Topic, subsystem, or neighborhood -> `activate`
- Unfamiliar repo -> `audit`
- Stacktrace or runtime error text -> `trace`
- Risky change -> `impact`, `predict`, `validate_plan`, then usually
  `surgical_context_v2`
- Docs/specs -> `ingest` with `adapter="universal"` or `adapter="light"`, then
  document binding/drift tools

## Recovery Rules

`Transport closed` is a host transport failure, not a m1nd proof-state. Do not
call `doctor`, `recovery_playbook`, or `ingest` through that dead binding. A
fresh MCP transport must be launched first.

If a local helper/probe fails before initialization with
`runtime_root ... is already owned by instance`, treat it as a runtime sidecar
lock collision, not stale graph truth. Use an isolated `--runtime-dir`, use the
current `probe_m1nd.py` helper, or group dependent checks into one
`probe_m1nd.py run` process before falling back to files.
In benchmark lanes or strict write scopes, prefer `probe_m1nd.py
--no-worktree-artifacts ...` so runtime metadata stays in the isolated runtime
dir while the caller directory becomes the declared `M1ND_WORKSPACE_ROOT`.
If the command is launched from outside the inspected repo, add
`--workspace-root /path/to/repo`.
The helper prefers `~/.m1nd/bin/m1nd-mcp` before a stale `m1nd-mcp` on `PATH`.

If the host appears to be launching an old or stale native runtime, and a local
`m1nd` CLI is available outside the MCP transport, run:

```bash
m1nd update check --channel beta
m1nd update plan --channel beta
m1nd update apply --channel beta --yes
```

Then restart or rebind the host MCP client. `m1nd update` is external repair:
it does not ingest, pick the workspace, repair graph contents, or refresh a
client's cached tool list inside an already-open conversation.
If host config points to an absolute current managed runtime, a stale
`m1nd-mcp` on `PATH` is a shadow warning only. If the host launches `PATH` or
config is unknown, stale `PATH` is actionable. Verify with `m1nd hosts status`
and `m1nd hosts plan`, then rebind or open a fresh host session before claiming
the updated runtime or tool surface is active.
Use `m1nd hosts apply` only as the local mutation step after `status` or
`plan`. Without `--yes` it stays a dry-run preview. With `--yes`, it can
install or refresh agent-pack files and write canonical MCP config snippets for
known hosts, but it does not prove rebind, refresh cached tool lists, repair
graph state, or automate generic-host config paths.

In live multi-agent sessions, use `--no-kill` to update the managed binary
without stopping every active `m1nd-mcp` host. `m1nd restart --source
/path/to/m1nd --yes` remains the lower-level source-checkout repair path for
development builds.

If `seek`, `search`, or `activate` returns `blocked`, zero candidates, or an
unexpectedly empty graph after ingest, treat it as possible stale binding or
session split-brain before blaming the repo.

Pass the returned `recovery.arguments` to `recovery_playbook` when present. If
the response has no payload, call `recovery_playbook` with:

```json
{"agent_id":"agent","observed_tool":"seek","observed_proof_state":"blocked","observed_candidates":0}
```

If m1nd is visible but required tools such as `ingest`, `trust_selftest`, or
`recovery_playbook` are missing, classify the session as
`degraded_host_tool_surface`. Use m1nd only for orientation and verify final
truth with local files until the host binding refreshes.

## Change Discipline

Before risky edits:

1. Use `seek` or `activate` to find the connected surface.
2. Use `impact` for blast radius.
3. Use `validate_plan` for missing work.
4. Use `surgical_context_v2` for compact edit context.
5. Run compiler/tests/runtime checks for execution truth.

`m1nd` complements the compiler, test runner, LSP, debugger, security scanner,
and local file truth. It does not replace them.

## Continuity

Keep `agent_id` stable within one investigation. Use trails, perspectives, and
coverage sessions when work spans agents, branches, or sessions.

For host setup, prefer exporting `M1ND_WORKSPACE_ROOT` to the actual repository
or project root. Claude Code, Antigravity, Gemini, Cursor, Windsurf, VS Code,
and generic shells can expose their own workspace hints too, but
`M1ND_WORKSPACE_ROOT` is the portable contract.

When the host binding looks wrong, run the read-only CLI cockpit before
mutating anything:

```bash
m1nd hosts status --host all --project /path/to/project --json
m1nd hosts plan --host all --project /path/to/project --json
m1nd hosts apply --host all --project /path/to/project --yes --json
```

`hosts plan` emits the install, MCP-config, `M1ND_WORKSPACE_ROOT`, rebind, and
verification recipe for each supported packaged host. Prefer
`M1ND_WORKSPACE_ROOT` in host config so the binding does not fall back to
`OLDPWD` or another ambient workspace hint.

## L1GHT

Use `adapter="light"` for graph-native semantic markdown. Use
`adapter="universal"` or `adapter="auto"` for ordinary docs, wiki pages, PDFs,
and office documents.
