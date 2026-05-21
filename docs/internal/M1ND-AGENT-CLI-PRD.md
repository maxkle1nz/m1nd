# PRD: m1nd Agent CLI v0.10

Status: implemented boundary, pending full release hardening
Date: 2026-05-19
Owner lens: Jimi, build agent and m1nd guardian

## Summary

`m1nd agent ...` is the host-neutral operating layer for coding agents. It gives
an agent a deterministic JSON cockpit outside the live MCP host: scope
alignment, runtime identity, trust state, recovery plan, bounded orientation,
context capsule, and direct-proof handoff.

The core product move is simple:

```text
do not make every agent memorize the whole m1nd tool matrix;
give every agent one CLI entrypoint that can decide the first safe move.
```

## Boundary

The v0.10 boundary is local and agent-first:

- `m1nd agent scope` classifies ambient scope and proves the CLI runtime will be
  repo-bound.
- `m1nd agent trust` runs `trust_selftest`, optional ingest, and
  `session_handshake` in one isolated runtime.
- `m1nd agent orient` replaces the old short-audit helper for small audits and
  bug hunts.
- `m1nd agent recover` turns known failures into exact recovery commands.
- `m1nd agent context` creates a compact `surgical_context_v2` capsule from a
  query or file path.
- `m1nd agent handoff` emits a small resumable handoff packet before full
  durable mission state.
- `m1nd agent doctor` combines package doctor, host readiness, update status,
  pack state, and scope alignment.

Every command returns `schema: "m1nd-agent-cli-v0"` and includes:

- `repo`
- `agent_id`
- `runtime`
- `scope_alignment`
- `graph_state` when available
- `trust` when available
- `calls`
- `results`
- `next_actions`
- `non_claims`

## Agent Contract

Defaults are intentionally biased toward agents, not humans:

- Non-TTY output is JSON.
- Runtime is isolated by default.
- `M1ND_WORKSPACE_ROOT` is always set to the requested repo.
- Worktree graph/plasticity artifacts are redirected to a temp runtime dir.
- `orient --mode short` always returns `switch_to_direct_proof=true`.
- Recovery output is a plan, not a claim that recovery happened.

The CLI reports ambient scope separately from the agent runtime scope. This lets
it say: "your host was bound to the wrong repo, but this isolated CLI call is
correctly bound to the requested repo."

## Non-Claims

`m1nd agent ...` does not:

- refresh an already-open MCP host or cached tool list;
- prove semantic retrieval correctness;
- replace direct source reads, tests, compilers, logs, or runtime probes;
- mutate tracked repository code;
- guarantee every possible host is configured;
- become a production unattended agent orchestrator.

## Success Criteria

The boundary is accepted when:

- unit tests cover scope classes, cold trust, blocked retrieval, recovery
  recipes, context path escape, and JSON envelope shape;
- `agent orient --mode short` performs only one orientation call after
  trust/ingest/handshake;
- real smoke can run against the local m1nd checkout;
- docs teach the CLI as the preferred sidecar entrypoint while keeping the old
  Python probe as compatibility;
- no command writes graph or plasticity files into the inspected repo by
  default.

## Future Work

v0.11 should add a real durable handoff store and richer repo-map capsule.
v0.12 should expose Mission Control through `m1nd agent mission ...`.
v0.13 should add benchmark automation around CLI-vs-host-vs-direct lanes.
