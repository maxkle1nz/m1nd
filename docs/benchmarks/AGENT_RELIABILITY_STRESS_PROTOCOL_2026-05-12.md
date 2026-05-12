# Agent Reliability Stress Protocol

Date: 2026-05-12
Repo: `/Users/kle1nz/m1nd`
Status: internal protocol, not a result claim

## Purpose

Define a compact, repeatable stress protocol for agent-first reliability testing.
The goal is to compare blinded lanes with `m1nd` available versus blinded lanes
without `m1nd`, on hard tasks that punish false confidence, wrong workspace
assumptions, stale runtime state, and recovery quality.

This protocol is for internal product learning first:

- measure whether `m1nd` reduces false starts and rediscovery cost
- measure whether `m1nd` improves time to good context and recovery follow-through
- separate strong evidence from mixed or internal-only outcomes

## Run Design

Each benchmark round uses `7` agent lanes:

- `3` blinded `m1nd_available` lanes
- `3` blinded `no_m1nd` control lanes
- `1` blinded adjudication lane for disputed, failed, or noisy cases

Blinding rules:

- do not tell agents the benchmark hypothesis
- do not tell agents which outcome would count as a product win
- present the task as normal operator work with the same success target
- hide prior lane outcomes until the round is closed

Environment rules:

- same model family, reasoning tier, and base system prompt across all lanes
- same task brief, same allowed time budget, same artifact budget
- same repo snapshot for all same-repo tasks
- randomize task order across rounds to reduce order effects
- explicitly label `warm` versus `cold` graph conditions
- record host/runtime anomalies instead of smoothing them away

Recommended round shape:

1. assign one task to all `6` primary lanes
2. let each lane run once without human rescue
3. send one failed or disputed outcome to the adjudication lane
4. score primary outcomes first, adjudication outcome second

## Task Battery

Use hard tasks that force orientation, recovery, and proof rather than toy lookup.
Each task should have a written answer key, expected evidence shape, and a clear
stop condition.

Minimum battery for the first round:

1. multi-repo orientation: correct repo, file, or subsystem must be identified before action
2. wrong workspace binding: lane starts in the wrong workspace and must diagnose it
3. transport closed / dead runtime: lane must recover or reroute without inventing success
4. stale `PATH` or stale binary route: lane must identify the runtime/tool mismatch
5. structural edit prep: enough context must be gathered to name the safe edit target and likely proof steps
6. root-cause triage: isolate the most likely fault file or boundary from a realistic symptom
7. continuity resume: continue a partially completed investigation without restarting from zero

Task design rules:

- at least `4` tasks should be recovery-heavy
- at least `2` tasks should require multi-file proof, not one-file lookup
- at least `1` task should involve cross-repo or wrong-repo ambiguity
- exact-text toy tasks are not headline evidence

## Primary Metrics

Use repo-native benchmark language where possible.

For each run, record:

- `mode`: `manual`, `m1nd_cold`, `m1nd_warm`, or `no_m1nd`
- `time_to_first_good_answer_ms`
- `time_to_full_proof_ms`
- `false_start_count`
- `files_opened`
- `repeat_reads`
- `search_iterations`
- `chars_surfaced`
- `token_proxy = ceil(chars_surfaced / 4)`
- `answer_quality`
- `recovery_events`
- `recovery_followed`
- `missing_signals`
- `missing_resolved`
- `proof_state` when available
- `workflow_notes`

Agent-first reliability adds these run-level fields:

- `time_to_good_context_ms`: first moment the lane has the right repo/workspace/tool direction
- `wrong_workspace_detected`: yes / no
- `wrong_workspace_recovered`: yes / no
- `transport_failure_detected`: yes / no
- `transport_failure_recovered`: yes / no
- `stale_path_detected`: yes / no
- `stale_path_recovered`: yes / no
- `claim_overreach`: none / mild / severe
- `agent_testimony`: short post-run self-report on confidence, confusion, and perceived recovery quality
- `final_state`: success / partial / failed / invalidated

## Evidence Fields

Every scored run should keep enough evidence to audit the score later.

Required evidence:

- task id and lane id
- task prompt given to the agent
- repo or workspace path presented to the lane
- tool/event stream with timestamps
- file-open list in order
- final answer or final action summary
- adjudicator notes with pass/fail rationale

Useful optional evidence:

- first correct repo named
- first correct file or subsystem named
- first explicit recovery hint surfaced
- first explicit non-claim stated by the lane
- short post-run testimony from the lane in its own words
- whether the lane switched from wrong theory to correct theory

## Success Criteria

A run counts as `success` only when all of the following are true:

- the lane reaches the correct task target or correct recovery route
- the lane does not claim proof it did not actually gather
- the lane names missing proof when proof is still missing
- the lane avoids unnecessary restart behavior after a useful hint is available

A run counts as `partial` when:

- the lane finds the right neighborhood but not full proof
- the lane recovers late with excessive rediscovery
- the lane reaches a usable answer but with a material false start

A run counts as `failed` when:

- the lane stays in the wrong repo, wrong workspace, or wrong theory
- the lane fabricates success after transport/runtime failure
- the lane misses the safe recovery path and thrashes into fresh discovery

Mark the run `invalidated` when the environment is not comparable across arms.

## Failure Taxonomy

Classify each failed or partial run with one primary label:

- `wrong_workspace_binding`
- `wrong_repo_assumption`
- `transport_closed`
- `stale_path_or_binary`
- `tool_surface_misread`
- `false_confident_proof`
- `fresh_rediscovery_after_hint`
- `over-broad_context_dump`
- `manual_fallback_too_late`
- `correct_answer_poor_recovery`
- `host_not_comparable`

Secondary tags may be added, but only one primary failure class should drive the score.

## Scoring Rubric

Score each run on a `0-4` scale per dimension:

- `orientation`: correct repo/workspace/tool direction
- `recovery`: follows the shortest honest repair loop
- `proof`: distinguishes proved vs missing evidence
- `efficiency`: low false starts, low repeat reads, compact context
- `outcome`: reaches the correct usable end state

Dimension anchors:

- `4`: strong, direct, low-noise behavior
- `3`: correct with moderate friction
- `2`: mixed; usable but wasteful or shaky
- `1`: mostly wrong, rescued late
- `0`: failed or fabricated

Rollups:

- `run_score = sum(dimensions)` with max `20`
- `arm_median_run_score`
- `arm_median_time_to_good_context_ms`
- `arm_median_false_start_count`
- `arm_success_rate`
- `arm_recovery_followed_rate`

Do not publish a headline advantage from one round alone. Look for repeated wins
across task classes, not one dramatic outlier.

## Report Template

Use this compact template per round:

```md
# Agent Reliability Stress Report

- Date:
- Round id:
- Repo set:
- Condition:
- Model/runtime:
- Task battery:

## Arm Summary

| Arm | Lanes | Success rate | Median run score | Median time to good context | Median false starts | Notes |
|---|---:|---:|---:|---:|---:|---|
| m1nd_available | 3 |  |  |  |  |  |
| no_m1nd | 3 |  |  |  |  |  |
| adjudication | 1 |  |  |  |  |  |

## Per-Task Notes

| Task | Better arm | Why | Failure class seen | Public-claim-worthy |
|---|---|---|---|---|

## Non-Claims

- no public performance claim is made from this report unless the evidence is repeated and comparable
- no claim that `m1nd` replaces tests, `rg`, compiler output, or direct file truth
- no claim that all hosts, transports, or workspace bindings work reliably
```

## Non-Claims

Keep these attached to every round unless later evidence proves otherwise:

- no public performance claim until the result is measured, repeated, and comparable
- no claim that `m1nd` replaces tests, `rg`, compiler output, git history, or direct file reads
- no claim that `m1nd` is the best tool for exact-text lookup or tiny local questions
- no claim that all hosts, transports, workspace bindings, or binary routes work
- no claim that warm-graph results equal cold-start behavior
- no claim that subjective agent testimony alone is sufficient evidence

## Notes For Operators

- keep adjudication separate from primary scoring
- preserve raw evidence for later re-scoring
- prefer internal-only labels when the result is mixed, noisy, or environment-sensitive
- if a task can be won by trivial grep alone, it should not anchor a public reliability story
