# Real-World Agent Benchmark Protocol

Date: 2026-05-13
Status: internal protocol, not a result claim

## Purpose

The host-recovery round answered a narrow question: can agents notice stale
workspace, runtime, graph, or transport state?

This protocol answers the bigger product question: does `m1nd` help agents do
normal code work better on real repositories?

The benchmark should simulate work that coding agents do every day:

- understand an unfamiliar repo
- locate where a feature lives
- explain a real flow
- triage a bug symptom
- plan a safe change
- implement a small patch
- fix a seeded bug
- prepare a bounded refactor
- review a diff
- check docs/spec drift

## Fixture Repositories

The starter fixture set intentionally spans languages and repo shapes:

- `click-python-cli`: mature Python CLI library
- `p-limit-node`: compact TypeScript utility
- `human-panic-rust-cli`: Rust crate with docs, examples, and tests

Clone fixtures into an ignored local directory:

```bash
python3 scripts/benchmark/real_world_agent_round.py fetch-fixtures \
  --fixtures-dir .m1nd-benchmark-fixtures/real-world \
  --json
```

The cloned repositories are benchmark fixtures. Do not commit them into `m1nd`.

## Round Setup

Create the round:

```bash
python3 scripts/benchmark/real_world_agent_round.py init \
  --out-dir docs/benchmarks/real-world-rounds/round-001 \
  --fixtures-dir .m1nd-benchmark-fixtures/real-world \
  --round-id round-001 \
  --json
```

The command writes:

- `round.json`
- `operator-only/answer-key.json`
- `operator-only/fixture-lock.json`
- `benchmark-payloads/*`
- `event-streams/*.jsonl`
- `lane-result-template.json`
- `lane-prompts/*.md`
- `lane-results/*.json`

`round.json` and lane prompts contain only public task payloads.
`operator-only/answer-key.json` is for the parent and adjudicator; do not give
it to primary lanes.

`operator-only/fixture-lock.json` captures the local fixture HEAD commits at
round creation. It is a benchmark comparability guard, not proof that upstream
repositories are immutable forever.

Prepare isolated lane workspaces before running agents:

```bash
python3 scripts/benchmark/real_world_agent_round.py prepare-lane-fixtures \
  --round-file docs/benchmarks/real-world-rounds/round-001/round.json \
  --write docs/benchmarks/real-world-rounds/round-001/lane-workspaces.json \
  --json
```

Each lane gets its own fixture checkout under
`.m1nd-benchmark-fixtures/real-world-lanes/<round-id>/<lane-id>/`. Patch tasks
must use those lane-specific checkouts, not the shared fixture repos.

`prepare-lane-fixtures` also writes deterministic seeded artifacts into the
lane workspaces. In v2, Click receives
`tests/test_m1nd_seeded_callable_type.py`, which proves the callable-instance
custom type bug before any agent patch.

When a fixture lock has a commit for a repo, `prepare-lane-fixtures` verifies
that each prepared lane checkout has the same HEAD. A mismatch makes the
prepare payload `ok=false`.

The round uses seven lanes:

- `3` lanes with `m1nd_available`
- `3` control lanes with `no_m1nd`
- `1` adjudication lane

The same ten-task battery is assigned to every primary lane.

Every lane must append raw investigation events to its own JSONL stream under
`event-streams/`. Use `event_source="agent"` for agent-authored events. The
harness writes only the initial `lane_assigned` event, so a lane with zero
agent-authored events is still treated as missing event capture.

## Task Battery

Each task now has a deterministic repo, payload, and answer key:

1. `repo_architecture_audit`: Click public exports, decorators, core, parser, testing.
2. `feature_location`: p-limit `rejectOnClear` and `clearQueue`.
3. `flow_explanation`: human-panic release-mode `setup_panic!()` flow.
4. `bug_symptom_triage`: Click callable-instance custom type symptom.
5. `safe_change_plan`: p-limit `clearQueue()` return-count plan.
6. `small_feature_patch`: human-panic `Metadata::name` / `Metadata::version`.
7. `seeded_bug_fix`: Click seeded callable-instance regression test.
8. `bounded_refactor_plan`: p-limit queue scheduling/draining cluster.
9. `code_review_diff`: supplied human-panic support-output diff.
10. `docs_drift_check`: Click lazy-loading docs claim versus implementation.

Patch tasks must stay local to fixture repos. Do not push, publish, or commit
fixture changes.

## Scoring

Each task is scored from `0` to `4` on:

- `orientation`
- `localization`
- `causal_understanding`
- `proof`
- `efficiency`
- `outcome`

Score a task as success only when the lane reaches the correct files/modules or
patch boundary and preserves missing proof honestly.

Score a task as partial when the answer is usable but noisy, over-broad, weakly
proved, or missing an important test/link.

Score a task as failed when the lane anchors on the wrong subsystem, invents
proof, or produces a bad patch/review.

Mark a task invalidated when the fixture repo, task setup, or environment is not
comparable across arms.

## Score A Completed Round

```bash
python3 scripts/benchmark/real_world_agent_round.py score \
  --runs-dir docs/benchmarks/real-world-rounds/round-001/lane-results \
  --output docs/benchmarks/real-world-rounds/round-001/report.json \
  --round-id round-001 \
  --json
```

The scorer now reports:

- whether event logs exist for all primary lanes
- whether all primary lanes contain agent-authored event evidence
- per-arm median agent event count
- invalid event counts
- optional adjudicated scores from the judge lane

The judge lane may fill top-level `adjudications[]` in its lane result. Each
item should name `primary_lane_id`, `task_id`, `adjudicated_final_state`,
`adjudicated_scores`, `comparability_class`, `exclusion_reason`, `notes`, and
`evidence`.

Create a judge packet from completed primary lane results:

```bash
python3 scripts/benchmark/real_world_agent_round.py judge-input \
  --round-file docs/benchmarks/real-world-rounds/round-001/round.json \
  --lane-results-dir docs/benchmarks/real-world-rounds/round-001/lane-results \
  --answer-key docs/benchmarks/real-world-rounds/round-001/operator-only/answer-key.json \
  --output docs/benchmarks/real-world-rounds/round-001/operator-only/judge-input.json \
  --json
```

The judge packet includes the task matrix, operator answer key, primary lane
summaries, event summaries, and empty adjudication templates for every primary
lane/task.

## What This Should Prove

This round is allowed to show:

- fewer false starts
- faster time to good context
- fewer irrelevant files opened
- better localization of implementation and tests
- better blast-radius reasoning
- better review findings
- better docs/code drift detection
- better patch scope

This round must not claim:

- universal superiority from one repo set
- production-grade benchmark certainty
- that m1nd replaces direct file truth or tests
- that agent testimony alone is evidence
- that a plan-only task proves patch quality

## Deterministic V2 Boundary

The current harness now fixes payloads, writes an answer key, supplies a review
diff, and seeds at least one real regression test into isolated lane workspaces.
It also writes a fixture lock and can generate judge input for adjudication.

Still missing before any public performance claim:

- raw tool/event transcript capture
- repeated rounds across more repo families
