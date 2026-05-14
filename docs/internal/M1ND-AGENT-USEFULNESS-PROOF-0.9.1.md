# m1nd 0.9.1 - Agent Usefulness Proof

Status: internal build north
Date: 2026-05-13

## Purpose

The 0.9 beta line proved installation, host binding, runtime update, and
recovery surfaces. The next north is usefulness: measure whether m1nd makes
agents work better on real repo tasks.

This phase should answer:

- does m1nd reduce time to good context?
- does m1nd reduce false starts and repeated discovery?
- does m1nd improve recovery from wrong workspace, stale runtime, and dead transport?
- does m1nd make proof boundaries clearer for agents?

## First Contract

`scripts/benchmark/agent_reliability_round.py` defines the first structured
round contract:

- `m1nd-agent-reliability-round-v0`
- `m1nd-agent-reliability-lane-result-v0`
- `m1nd-agent-reliability-report-v0`

The round shape is:

- 3 blinded `m1nd_available` lanes
- 3 blinded `no_m1nd` control lanes
- 1 adjudication lane
- 7 hard tasks focused on orientation, recovery, structural edit prep, root
  cause triage, and continuity

## Real-World Usefulness Contract

The host-recovery contract is not enough to answer whether m1nd is better for
normal code work. `scripts/benchmark/real_world_agent_round.py` defines the
next contract:

- `m1nd-real-world-agent-round-v0`
- `m1nd-real-world-agent-lane-result-v0`
- `m1nd-real-world-agent-report-v0`

This round uses external fixture repositories and everyday agent tasks:

- architecture audit
- feature localization
- end-to-end flow explanation
- bug symptom triage
- safe change planning
- small feature patch
- seeded bug fix
- bounded refactor planning
- code review
- docs/spec drift check

This is the round family that should drive future product usefulness claims.
The host-recovery round remains useful, but it measures whether agents can
detect environment confusion, not whether m1nd improves general coding work.

## Commands

Create a round:

```bash
python3 scripts/benchmark/agent_reliability_round.py init \
  --out-dir docs/benchmarks/agent-rounds/round-001 \
  --repo . \
  --round-id round-001 \
  --json
```

Score completed lane results:

```bash
python3 scripts/benchmark/agent_reliability_round.py score \
  --runs-dir docs/benchmarks/agent-rounds/round-001/lane-results \
  --output docs/benchmarks/agent-rounds/round-001/report.json \
  --round-id round-001 \
  --json
```

## What Counts As A Win

A useful m1nd win is not just "found a file." It is:

- correct repo and workspace orientation sooner
- honest recovery when the graph, host, or runtime is stale
- fewer false starts
- fewer repeat reads and broad context dumps
- clearer proof state and next action
- lower claim overreach

## First Round Lesson

The first live round must not be read as a headline performance claim. The
important early signal is epistemic: m1nd can expose live workspace/runtime
state, recovery payloads, and host-binding failure modes that static source
inspection can only infer.

The benchmark now records this distinction explicitly:

- `requires_live_proof`
- `proof_mode`
- `live_state_verified`
- `evidence_origin`
- `live_required_verified_rate`
- `live_proof_gap_count`

This prevents a route-only answer from scoring like a live runtime proof. It
also gives the next product loop a sharper target: make live host state easy for
agents to verify, compare, and recover across Codex, Claude, Cursor, Gemini,
Antigravity, and other MCP hosts.

## Non-Claims

- no public performance claim from one round
- no claim that m1nd replaces tests, compiler output, git history, rg, or direct file truth
- no claim that agent testimony alone is sufficient evidence
- no claim that warm-graph results equal cold-start behavior
- no claim that every host/runtime/session is fixed by benchmark success

## Next Step

Run the first real 7-lane round on a live repo task, preserve raw lane result
JSON, then turn the report into a patch queue. The benchmark should drive product
work, not marketing decoration.
