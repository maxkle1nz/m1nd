# m1nd Agent Runtime Contract v0

Date: 2026-05-12

## Summary

`m1nd-agent-runtime-contract-v0` is the first shared runtime envelope for
agent-facing retrieval and orientation tools. Its purpose is to remove ambiguous
empty results for agents: before deciding that retrieval failed, the agent can
inspect the active binary, runtime root, workspace binding, graph identity, and
recovery payload in one predictable object.

This is agent-first UX. Humans can read it, but it is designed so an agent can
route itself without asking for prose.

## Current Surfaces

The envelope is now attached as `agent_runtime_contract` on:

- `activate`
- `seek`
- `search`
- `panoramic`

These are the first high-friction surfaces because they are the tools where
`results: []` or `modules: []` can mean several different things.

## Schema Shape

Core fields:

- `schema`: `m1nd-agent-runtime-contract-v0`
- `status`: `ok`, `triaging`, or `blocked`
- `proof_state`: the tool-local proof state
- `trust_mode`: `full_trust`, `needs_ingest`, `wrong_workspace_binding`, or
  `retrieval_needs_recovery`
- `observed`: tool, candidate count, and optional error text
- `session_identity`: agent id, tool, process id, runtime root, current binary,
  and binary version
- `workspace_binding`: requested scope, active workspace root, ingest roots, and
  mismatch details when present
- `graph_identity`: node/edge counts, graph generation, cache generation,
  plasticity generation, ingest root count, graph path, and graph path existence
- `next_suggested_tool`: usually `recovery_playbook` when recovery is needed
- `recovery`: machine-readable payload to pass directly to `recovery_playbook`
- `non_claims`: what this envelope does not prove or mutate

## Routing Rules

- `full_trust`: proceed with normal m1nd-first interpretation, while still using
  compiler/tests/logs for runtime truth.
- `wrong_workspace_binding`: do not treat the graph as stale. Rebind the host
  with `M1ND_WORKSPACE_ROOT`, intentionally ingest the requested workspace on
  the same binding, or use federation if the task is truly cross-repo.
- `needs_ingest`: the active graph has no nodes. Ingest the intended workspace
  on the same binding before retrieval.
- `retrieval_needs_recovery`: the graph exists, but this call returned blocked
  or zero candidates. Pass `recovery.arguments` to `recovery_playbook` before
  falling back to shell search.

## Non-Claims

- The envelope does not repair MCP host bindings.
- The envelope does not ingest or mutate the graph.
- The envelope does not prove semantic retrieval correctness.
- The envelope does not replace compiler, test, log, or direct file truth.

## Why This Exists

Before this contract, each tool explained failure differently. Some returned
`proof_state`, some returned `graph_state`, some returned `recovery`, and some
could make an empty result look like a successful answer. The new envelope makes
the operational state explicit enough for agents to decide whether to proceed,
recover, rebind, ingest, federate, or fall back.
