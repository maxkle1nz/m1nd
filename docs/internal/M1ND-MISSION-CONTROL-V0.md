# m1nd Mission Control v0

Status: implementation boundary, not a public performance claim
Date: 2026-05-16

## Purpose

Mission Control v0 turns the trained-agent loop into runtime state. It exists
because real agents do not only need better retrieval; they need a bounded route,
phase discipline, direct-evidence checks, and an honest close packet.

The core product lesson from benchmark work is:

- graph plus operating doctrine beats graph alone on structural bug hunts;
- tiny/localized tasks can lose time when graph exploration keeps going after it
  has done enough;
- agents need a machine-readable way to switch from orientation to direct proof.

## Contract

The v0 MCP surface is four tools:

- `mission_start`: create a repo-scoped mission with route, budget envelope,
  starter moves, and non-claims.
- `mission_next`: append an event and return exactly one next move plus `do_not`
  guardrails.
- `mission_verify`: classify a claim as `verified_for_mission` only when direct
  evidence is present.
- `mission_close`: emit a proof packet with verified claims, rejected claims,
  event count, tools observed, gaps, budget consumption, and non-claims.

Mission state is persisted under:

```text
<runtime_root>/mission-control/<mission_id>.json
```

The persisted state schema is:

```text
m1nd-mission-control-state-v0
```

The proof packet schema is:

```text
m1nd-mission-proof-packet-v0
```

## Evidence Rules

`mission_verify` is intentionally conservative. It accepts direct evidence
classes such as:

- `file_read`
- `read_file`
- `view`
- `test_run`
- `run_test`
- `compiler`
- `runtime_probe`
- `rg`
- `grep`

It rejects graph-only or inferred evidence such as `seek`, `activate`,
`audit`, or a plain claim with no direct source/runtime/test reference.

For `bug_hunt` missions, a verified claim is not enough to close by default.
After one or more verified findings, `mission_next` requires one direct
negative-space sweep before `mission_close`: public contracts/docs, boundary
values, error paths, async or concurrency semantics, and helper/exported APIs
not covered by current claims. Record that action as `coverage_sweep`,
`boundary_sweep`, `edge_case_sweep`, `negative_space_sweep`,
`public_contract_sweep`, or `followup_sweep`.

## Non-Claims

Mission Control v0 does not claim:

- graph contents are correct;
- active host MCP tool caches were refreshed;
- ingest roots were repaired;
- semantic retrieval was fixed;
- source reads, tests, compiler output, runtime probes, or human review are
  replaced;
- autonomous multi-agent orchestration is solved.

## Use

Use Mission Control for broad reviews, bug hunts, risky refactors, release
checks, and proof-sensitive investigations. Do not use it for trivial exact-file
edits or pure compiler/runtime truth where the next step is already obvious.

The intended loop:

1. `mission_start`
2. one starter move
3. `mission_next` with the last event
4. direct proof when `do_not` blocks more graph calls
5. `mission_verify`
6. for `bug_hunt`, one final direct coverage sweep when `mission_next` asks
7. `mission_close`

## Next Proof

The next honest proof is a benchmark condition:

```text
m1nd-mission-control
```

It should be compared against `m1nd-trained`, `m1nd-short-audit`, and `direct`
on seeded bug-hunt rounds. The target is not "more tools used"; the target is
equal or better recall with fewer repeated graph calls and more verified direct
evidence in final reports.
