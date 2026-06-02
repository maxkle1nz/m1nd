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

## Trained Agent Loop

For unfamiliar repo work, audits, bug hunts, reviews, and risky changes, use
this loop by default:

1. Establish trust: `trust_selftest`, or `session_handshake` with the requested
   repo/workspace as `scope`.
2. If trust is not full, follow `recovery_playbook` before interpreting empty
   results. `wrong_workspace_binding` means rebind, intentional ingest, or real
   federation; it is not stale graph proof.
3. Orient with `audit` for unfamiliar repos or `search`/`seek`/`activate` for
   focused questions.
4. Read the compact runtime envelope on retrieval responses. Empty results are
   not final truth until workspace, graph identity, and recovery state are
   coherent.
5. Verify with direct source, tests, compiler/runtime output, and focused
   probes. m1nd narrows and connects; execution truth still comes from the
   repo.
6. Before edits or reviews, run `impact`, `validate_plan`, and usually
   `surgical_context_v2`.
7. Record what happened: m1nd calls, recovery path, files inspected, commands
   run, and any fallback reason.

This is the `m1nd-trained` behavior measured in internal bug-hunt rounds: graph
plus operating doctrine, not graph alone.

## Scope Binding Taxonomy

When trust or retrieval looks wrong, classify the scope relationship before
calling more search tools:

- `full_repo_binding`: active workspace/ingest root equals the repo being
  investigated, or the requested scope is inside that root. Use m1nd normally,
  then prove final truth with source/tests.
- `wrong_workspace_binding`: active workspace is an unrelated repo. Rebind with
  `M1ND_WORKSPACE_ROOT=/target/repo`, intentionally ingest the target repo, use
  federation only for true cross-repo tasks, or run an isolated probe.
- `nested_workspace_binding`: active workspace/root is a subdirectory of the
  requested repo. Treat m1nd as partial truth for that subtree only. Rebind or
  ingest the repo root before making repo-wide claims.
- `file_level_binding`: ingest roots are individual docs, PRDs, or L1GHT files
  inside the repo. Use them as document context only; they do not prove codebase
  coverage or implementation truth.

Do not loop on `seek`/`activate` when the binding is nested or file-level. Either
upgrade the binding to the intended repo root, or explicitly switch to direct
files/tests and record `m1nd_usage_mode=partial_scope_orientation`.

## Mission Control v0

When the live runtime exposes `mission_start`, use Mission Control for broad
reviews, bug hunts, risky refactors, or any task where agents tend to loop,
over-trust graph evidence, or lose phase discipline.

Minimal mission loop:

1. `mission_start` with the intended repo, task, mode, budget, and risk.
2. Record meaningful actions with `mission_event` when the tool exists, or feed
   the latest action back as `last_event` to `mission_next`.
3. Take the starter move or ask `mission_next` for exactly one next move.
4. Obey `do_not` unless you record a dissent event with a concrete reason.
5. Use `mission_verify` before turning a conclusion into final output.
6. Use `mission_handoff` when another agent or future session may resume.
7. Use `mission_close` to produce a proof packet with verified claims, rejected
   claims, gaps, event digest, budget use, and non-claims.

Important evidence rule: a direct event does not prove every later claim. A
claim should reference the direct proof explicitly, for example
`event:evt_1`, `file_read:path:line`, `test_run:name`, or `runtime_probe:id`.

Mission Control does not replace source reads, tests, compiler/runtime output,
or host recovery. Its most important behavior is
`switch_to_direct_proof`: after graph orientation, it can tell you to stop
calling `seek`/`activate` and prove the claim directly.

Bug-hunt calibration: a verified finding does not mean the hunt is done. If
`mission_next` asks for a `direct_sweep`, do one negative-space sweep over
public contracts/docs, boundary values, error paths, async/concurrency behavior,
and helper/exported APIs, then record the event as `coverage_sweep`,
`boundary_sweep`, or `edge_case_sweep` before closing.

## Short-Audit Route

Use the short-audit route when the repo or suspected surface is small,
localized, and likely to be proven faster by direct source reads or runtime
probes than by extended graph navigation.

1. Establish trust with `trust_selftest`, or `session_handshake` scoped to the
   intended repo.
2. If you are unsure which m1nd move fits, ask the host-neutral router first:

   ```bash
   m1nd agent first-minute \
     --repo /path/to/repo \
     --query "understand this system" \
     --json

   m1nd agent next \
     --repo /path/to/repo \
     --query "focused subsystem or bug surface" \
     --json
   ```

   Use `first-minute` for first contact with a brand-new repo or broad "understand/audit/map this"
   request. It returns anchors, `do_not` guardrails, and the direct-proof
   handoff without requiring the agent to read this skill first.

   It returns an `m1nd-agent-action-envelope-v0`; follow the emitted command or
   recovery path instead of spending calls guessing the tool family.
3. If trust is not full, do one bounded recovery/ingest pass. Prefer the
   host-neutral agent CLI so trust, ingest when needed, one cheap orientation
   query, and the direct-proof handoff happen in one isolated MCP process:

   ```bash
   m1nd agent orient \
     --repo /path/to/repo \
     --query "focused subsystem or bug surface" \
     --mode short \
     --json
   ```
4. Run at most one or two orientation calls such as `audit`, `search`, `seek`,
   or `activate`.
5. Switch to direct files, `rg`, git diff, tests, compiler/runtime output, and
   focused probes.
6. Record whether this was `short_audit_orientation` or `recovery_overhead`.

The short-audit route is still m1nd-first. It just caps graph/recovery spend so
tiny localized tasks do not turn into tool-operation exercises.

`m1nd agent context` is anchor-first. Do not call it on broad narrative queries
such as "trace chat flow" or "understand this repo" unless you already have a
concrete `--anchor <file>` or intentionally pass `--allow-discovery`. Context
capsules support proof; they are not the first orientation move.

## RETROBUILDER Escalation

When a task asks for deep architecture quality, hidden coupling, taint/security
paths, duplication, refactor seams, or runtime heat, check whether the agent CLI
returned `capability_suggestions.family_id=retrobuilder`. If it did, use only
the matching tools, then stop and prove directly:

- hidden co-change or files that "move together" -> `ghost_edges`, then
  `timeline` or `impact`
- untrusted input, sensitive data, auth, privacy, or trust-boundary review ->
  `taint_trace`, then `trust`, `type_trace`, or `validate_plan`
- duplicate structures, near-equivalent modules, cleanup, extraction, or
  spaghetti -> `twins`, then `refactor_plan`
- runtime heat, OpenTelemetry spans, logs, latency, production failures, or hot
  paths -> `runtime_overlay`, then `trace` or `impact`
- broad architecture/risk audit -> `layers` plus the relevant RETROBUILDER
  tools: `ghost_edges`, `taint_trace`, `twins`, `refactor_plan`,
  `runtime_overlay`

RETROBUILDER output is graph orientation. It can identify strong hypotheses,
hidden neighbors, and proof targets, but it does not prove bugs or runtime
behavior without direct source reads, tests, compiler/runtime output, logs, or
focused probes.

## Session Companion Bridge

If the host also exposes a session-memory companion such as DEXT3R, use it only
for continuity: north star, prior decisions, open loops, handoff context, and a
scoped `m1nd flash` summary when available.

If the host exposes only the companion wrapper and no direct `m1nd` MCP tools,
classify the situation as `missing_m1nd_host_tool_surface`, not as unhealthy
graph truth. First try the host-neutral CLI path below; fall back to raw
`rg`/file reads only if the CLI path is unavailable or its recovery output says
to use direct local truth.

Do not use a companion digest or global memory search as code truth. Before
using companion output, verify that the session is bound to the intended repo or
project root. If the companion reports missing scope, wrong project, unavailable
flash, or global candidate search, treat that output as orientation only and
return to:

```bash
m1nd agent next --repo /path/to/repo --query "current task" --json
```

Then prove final claims with direct source reads, tests, compiler/runtime
output, logs, or focused probes. In short: session companions preserve why the
work matters; `m1nd agent` chooses the next repo move; direct proof decides what
is true.

## Skip Conditions

Skip the `m1nd` first pass only when:

- the user already gave the exact file and exact lines
- the question is compiler, test, or runtime truth rather than structure
- the task is a trivial local file action with no structural uncertainty

## Fallback

If `m1nd` does not answer enough, then fall back to shell search, direct file reads, compiler output, tests, logs, and debugger data.

If a local helper/probe fails before MCP initialization with
`runtime_root ... is already owned by instance`, classify it as a sidecar
runtime lock collision, not stale graph truth. Prefer `m1nd agent trust` or
`m1nd agent orient`, which isolate runtime state by default. If the npm CLI is
not installed, use the current `probe_m1nd.py`, pass a unique explicit
`--runtime-dir`, or collapse dependent checks into one `probe_m1nd.py run`
call. Do not tell the user that m1nd retrieval is broken until a fresh isolated
probe or the repo-local smoke harness also fails.
In benchmark lanes or repos with narrow write scopes, add
`m1nd agent ... --repo /path/to/repo --json` so graph/runtime metadata stays in
the isolated runtime directory instead of the target worktree. The CLI sets
`M1ND_WORKSPACE_ROOT` for the requested repo.

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
m1nd hosts apply --host all --project /path/to/project --yes --json
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
Use `m1nd hosts apply` only for the local mutation step after `status` or
`plan`. Without `--yes` it is still a dry-run preview. With `--yes`, it can
install or refresh agent-pack files and write canonical MCP config snippets for
known hosts, but it does not prove rebind, refresh cached tool lists, repair
graph state, or automate generic-host config paths.

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
If the open host cannot be rebound during the current turn, do not abandon m1nd
as a layer. Use an isolated local probe bound to the requested workspace, then
switch to direct source/test proof:

```bash
m1nd agent orient \
  --repo /path/to/requested/repo \
  --query "focused subsystem or bug surface" \
  --mode short \
  --json
```

Record this as `m1nd_usage_mode=isolated_probe_after_wrong_workspace_binding`,
not as "m1nd unavailable". Fall back to raw `rg`/manual reads only if the probe
cannot ingest/retrieve or the recovery playbook says to use local truth.
If `wrong_workspace_binding` is reported but the active root or ingest roots
are actually inside the requested repo, treat this as `nested_workspace_binding`
or `file_level_binding`, not as a dead m1nd core. The graph may be useful for
that sub-scope, but it cannot support repo-wide claims until the binding is
upgraded.

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
