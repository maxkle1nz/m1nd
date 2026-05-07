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

## L1GHT

Use `adapter="light"` for graph-native semantic markdown. Use
`adapter="universal"` or `adapter="auto"` for ordinary docs, wiki pages, PDFs,
and office documents.
