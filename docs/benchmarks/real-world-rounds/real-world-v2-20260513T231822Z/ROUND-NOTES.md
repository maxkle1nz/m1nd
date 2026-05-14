# Real-World Agent Benchmark Round Notes

Round: `real-world-v2-20260513T231822Z`
Status: internal evidence, not public performance copy

## What Ran

- `3` primary lanes with `m1nd_available`
- `3` primary control lanes with `no_m1nd`
- `1` adjudication lane
- `10` deterministic tasks per primary lane across Click, p-limit, and human-panic
- `60` primary task results adjudicated against `operator-only/answer-key.json`

## Result Snapshot

Self-scored primary lane rollup:

- `m1nd_available`: `29/30` success, median run score `230`
- `no_m1nd`: `30/30` success, median run score `233`

Adjudicated rollup:

- `m1nd_available`: `29/30` success, median adjudicated score `23`, average adjudicated score `22.8/24`
- `no_m1nd`: `30/30` success, median adjudicated score `23`, average adjudicated score `22.6/24`
- All `60` adjudications were comparable; no exclusions.
- The single partial was `m1nd-2 / repo_architecture_audit`, which missed `src/click/parser.py`.

Interpretation: this round does not prove m1nd is generally better or worse
than control. It proves the deterministic benchmark harness can now produce a
complete, comparable, adjudicated internal result.

## What m1nd Showed

- m1nd lanes completed all patch tasks.
- m1nd lanes recorded recovery and stale-binding evidence instead of hiding it.
- m1nd lanes used more structured investigation steps, but also more search/tool
  iterations.
- When semantic retrieval was stale or blocked, good lanes fell back to exact
  files and tests, which is the correct agent behavior.

## What Controls Showed

- Strong agents using `rg`, direct file reads, and tests remain a serious
  baseline.
- Control lanes were fast and direct on small/medium fixture repos.
- m1nd must beat a competent agent workflow, not a strawman without tools.

## Harness Findings

- The judge lane originally wrote a custom schema; the scorer ignored it until
  the output was wrapped in the lane-result schema. This proved the harness
  needs clearer judge-output instructions.
- Several primary event streams contained invalid event types. This did not
  invalidate the tasks, but it reduced evidence quality and showed that event
  capture needs better agent-facing affordances.
- The scorer had a false public-claim blocker for missing adjudication even
  after complete adjudication; this round fixed that logic.

## Non-Claims

- This is one round only.
- This is not public benchmark evidence.
- This does not prove universal m1nd superiority.
- This does not prove production agent reliability.
- This does not replace compiler output, tests, direct file truth, or human
  judgment.

## Next Move

Run at least two more deterministic rounds across different repo families after
improving event capture and judge-output UX. The next useful product cuts are:

- strict event schema helper for agents
- judge result template that already uses the scorer-compatible schema
- better workspace/retrieval provenance in m1nd recovery output
- explicit separation of graph calls, shell search, file reads, and test proof
  in round reports
