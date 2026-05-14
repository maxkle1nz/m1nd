# Real-World Agent Round Notes

Round: `real-world-20260513T005733Z`
Status: primary lanes and adjudication complete
Fixture directory: `.m1nd-benchmark-fixtures/real-world`

## Purpose

This is the first real-world usefulness round. It is intentionally different
from the host-recovery round: the goal is to compare everyday coding-agent work,
not whether an agent can diagnose a stale MCP session.

## Fixture Repos

- `click-python-cli`: Python CLI library
- `p-limit-node`: TypeScript utility
- `human-panic-rust-cli`: Rust crate

The fixture repos were cloned locally into the ignored benchmark fixture
directory. They are not part of the `m1nd` source tree and should not be
committed.

## Task Battery

The round covers ten normal agent activities:

1. repo architecture audit
2. feature localization
3. end-to-end flow explanation
4. bug symptom triage
5. safe change plan
6. small feature patch
7. seeded bug fix
8. bounded refactor plan
9. code review of a diff
10. docs/spec drift check

## Current State

The six primary lanes completed:

- `m1nd-1`
- `m1nd-2`
- `m1nd-3`
- `control-1`
- `control-2`
- `control-3`

The generated report is structurally comparable across primary arms, but it is
not public-claim-worthy. The adjudication lane reviewed correctness and evidence
quality and found that only a subset of tasks were anchored enough for a clean
comparison.

## Operator Interventions

This round contains two important limitations:

1. The parent accidentally sent a timebox instruction while lanes were still
   working, then corrected the instruction and told every lane to continue in
   full-depth mode. Treat the round as useful internal evidence, not a clean
   no-intervention benchmark.
2. `m1nd-3` used a `0-5` score scale while the harness requires integer `0-4`
   scores. The parent mechanically clamped `31` fields above `4` down to `4`
   and recorded `parent_normalization` in `m1nd-3.json`. No notes, evidence, or
   final states were changed.

Several lanes also noted that `seeded_bug_fix` and `code_review_diff` were less
controlled than ideal because the harness did not yet generate a deterministic
seeded bug or supplied diff. That is a benchmark-design gap, not a product
claim.

## Next Step

Build the next benchmark cut with pinned fixture commits, deterministic seeded
bugs, supplied review diffs, fixed task targets, raw event transcript capture,
and automatic result normalization warnings before agent lanes start.


## Adjudicated Snapshot

After scoring all lane artifacts:

- `m1nd_available`: success rate `0.9667`, median run score `218`, median files opened `34`, median search iterations `29`, median tests/commands `26`
- `no_m1nd`: success rate `0.9333`, median run score `229`, median files opened `38`, median search iterations `14`, median tests/commands `18`
- `adjudication`: success rate `0.3`, median run score `153`

Primary lane self-scores are not enough for a public claim. The adjudicator
found only three tasks meaningfully comparable as a shared benchmark target:

- `repo_architecture_audit`
- `flow_explanation`
- `bounded_refactor_plan`

The other tasks were useful work, but underanchored for scientific comparison:
feature target, bug symptom, change request, seeded bug, supplied diff, and docs
claim selection were not deterministic enough across lanes.

Working product read: m1nd lanes did very well and completed all patch tasks,
but the control lanes were also strong. This round supports improving the
benchmark design more than it supports a marketing claim.

## Non-Claims

- This round does not yet prove m1nd is better or worse.
- This round does not replace pinned-commit benchmark fixtures.
- This round does not replace raw event transcript capture.
- This round does not replace deterministic seeded bugs or supplied diffs.
