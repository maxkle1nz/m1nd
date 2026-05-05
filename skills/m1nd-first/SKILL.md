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

If `trust_selftest` is not exposed but `session_handshake` is, call the cheaper
sub-check:

```json
{"agent_id":"codex-m1nd"}
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
