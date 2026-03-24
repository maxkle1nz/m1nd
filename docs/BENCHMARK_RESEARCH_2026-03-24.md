# m1nd Benchmark Research

Date: 2026-03-24
Repo: `/Users/cosmophonix/SISTEMA/.codex-tmp/m1nd-standalone`
Status: working research log, not marketing copy

## Purpose

This document captures the benchmark work run on `m1nd` so far, with two goals:

1. measure where `m1nd` actually reduces agent context burn and investigation time
2. separate strong public claims from mixed or noisy cases

The benchmark is intentionally conservative. A result only counts as a strong `m1nd` win when:

- the task is structurally real, not a toy grep case
- the answer quality is equal or better than the manual pass
- the `m1nd` flow is not hiding cost behind giant payloads
- the method is honest about `cold graph` versus `warm graph`

## Measurement Model

The benchmark uses a token proxy, not provider-native billing telemetry.

### Token proxy

For each pass, we estimate:

- `chars_surfaced`: approximate number of characters surfaced to the agent
- `token_proxy = ceil(chars_surfaced / 4)`

This is a practical estimate of how much context the agent had to consume.

### Time metrics

We distinguish between:

- `cold_graph_time`: includes ingest or first-time graph setup
- `warm_graph_time`: graph already exists; measures analysis only
- `time_to_first_good_answer`: when the agent has enough evidence to act confidently
- `time_to_full_proof`: when the agent has enough evidence to justify the answer publicly

### Quality metrics

Each scenario records:

- answer quality: low / medium / high / very high
- benchmark worthiness: yes / maybe / no
- notes on whether `m1nd` won on compactness, speed, certainty, continuity, or safety

## Phase 1: Initial A/B Battery

Five scenarios were run first to calibrate the method.

### Phase 1 summary

| Scenario | Manual token proxy | m1nd token proxy | Outcome |
|---|---:|---:|---|
| Control text lookup (`actions/checkout@v5`) | 1316 | 75 | Huge compression, but not meaningful product differentiation |
| Stacktrace triage (`json_adapter`) | 13750 | 5500 | Strong structural win |
| Missing guard / verification boundary | 11272 | 4000 | Strong structural win |
| Multi-file edit prep (`boot_memory`, warm graph) | 12063 | 10250 | Mild win |
| Blast radius (`search scope normalization`, warm graph) | 10280 | 166462 | Loss due to huge payload |

### Phase 1 conclusion

The first battery showed the main truth clearly:

- `m1nd` helps a lot when the task is structural
- `m1nd` is not automatically more compact in every scenario
- some tool flows can become too verbose if the payload is not controlled
- exact-text lookups should not be used as headline proof of value

## Phase 2: Ten Structural Benchmarks

The second round focused on scenarios where connected context should matter.

### Strong public benchmarks

These are the scenarios that came out strong enough to justify public use in docs or release material.

| Scenario | Manual token proxy | m1nd warm token proxy | Savings | Why it matters |
|---|---:|---:|---:|---|
| Investigation continuity with `trail_resume` | 12861 | 800 | 93.8% | Avoids rediscovery cost across steps |
| Structural claim proof around `apply_batch -> RISKY` | 17250 | 1050 | 93.9% | Excellent snap-to-proof workflow |
| Layer / architecture inspection | 4525 | 1125 | 75.1% | Architecture picture fast and compact |
| Stacktrace / root-cause triage | 4025 | 800 | 80.1% | Fast narrowing to the right file/neighborhood |
| Semantic retrieval of intent | 7453 | 3000 | 59.7% | Good for intent lookup when plain grep is awkward |
| Missing guard / missing connection | 4396 | 4000 | 9.0% | Small token win, but useful structural focus |
| Hotspot explanation | 6377 | 3875 | 39.2% | Better explanation with honest limitations |

### Aggregate of strong benchmarks

Across the seven strongest warm-graph scenarios:

- manual total token proxy: `56,887`
- `m1nd` total token proxy: `14,650`
- aggregate estimated savings: `74.2%`

This is the most defensible current headline number from real scenarios.

### Mixed or internal-only cases

These scenarios are still valuable for product learning, but should not become public benchmark claims yet.

| Scenario | Manual token proxy | m1nd warm token proxy | Interpretation |
|---|---:|---:|---|
| Blast radius for search scope normalization | 10280 | 164843 | Better certainty, much worse payload compactness |
| Multi-file edit prep for `normalized_scope` | 10710 | 27203 | Better pre-edit confidence, worse token footprint |
| Failure-to-fix workflow | 8587 | 2250 | Numerically positive, but manual path still better for this failure class |

## What the Benchmarks Say

### Where `m1nd` clearly wins

`m1nd` is strongest when the agent needs one or more of the following:

- connected context across multiple files
- structural proof rather than raw text hits
- investigation continuity across steps
- architecture or layer reconstruction
- focus narrowing before a risky edit
- stacktrace-to-suspect mapping

### Where `m1nd` does not automatically win

`m1nd` is not automatically the best choice for:

- exact-text lookups
- very small local questions that are already obvious from one file
- compile-time or fixture errors where direct file reading is already enough
- flows that surface overly large context blobs from otherwise useful tools

### Product truth

The honest product truth is:

`m1nd` does not replace grep, compiler output, tests, or git history. It reduces repo reconstruction cost when the problem is structural and the graph is already warm.`

## Economic Model

Any money estimate needs to be transparent about assumptions.

### Per-task token savings

For a benchmarked task:

- `tokens_saved = manual_token_proxy - m1nd_token_proxy`
- `token_savings_pct = tokens_saved / manual_token_proxy`

Example from the strong benchmark aggregate:

- `tokens_saved = 56,887 - 14,650 = 42,237`
- `token_savings_pct = 42,237 / 56,887 = 74.2%`

### Per-task cost savings

If the provider charges `P` dollars per 1M input tokens, then:

- `cost_saved_per_task = tokens_saved / 1,000,000 * P`

Sensitivity examples for the aggregate strong benchmark result (`42,237` tokens saved):

| Price per 1M input tokens | Saved per 7-task batch |
|---|---:|
| $1 | $0.042 |
| $5 | $0.211 |
| $15 | $0.634 |
| $30 | $1.267 |

### Monthly cost savings

Let:

- `S = tokens_saved_per_task`
- `P = dollars_per_1M_input_tokens`
- `N = tasks_per_day`
- `D = workdays_per_month`

Then:

- `monthly_savings = S / 1,000,000 * P * N * D`

Example using the strong aggregate average:

- average tokens saved per strong task: `42,237 / 7 = 6,034`

If an agent performs 200 structurally similar tasks per workday over 22 workdays:

- monthly token savings: `6,034 * 200 * 22 = 26,549,600`

Estimated monthly dollar savings:

| Price per 1M input tokens | Monthly savings |
|---|---:|
| $1 | $26.55 |
| $5 | $132.75 |
| $15 | $398.24 |
| $30 | $796.49 |

This is input-only. Real blended cost can be higher if reduced context also reduces output churn, retries, and second-pass correction work.

### Time savings model

Let:

- `T_manual = average time to first good answer without m1nd`
- `T_m1nd = average warm-graph time to first good answer with m1nd`
- `delta_T = T_manual - T_m1nd`
- `N = structurally similar tasks per day`

Then:

- `time_saved_per_day = delta_T * N`

If `delta_T = 20 seconds` on average and `N = 200` tasks/day:

- `time_saved_per_day = 4,000 seconds = 66.7 minutes`
- `time_saved_per_month` over 22 workdays = `24.4 hours`

This is why the next benchmark phase should focus on time-to-solution, not just token proxy.

## Recommended Next Benchmark Phase

The next round should be warm-graph only and answer four questions:

1. How much faster does an agent reach the first good answer with `m1nd` already warm?
2. Does `m1nd` change the working style of the agent in a measurable way?
3. How often does `m1nd` improve answer quality or confidence, even when token savings are modest?
4. How much of the real economic gain comes from fewer retries, fewer wrong-file reads, and less rediscovery?

### Warm-graph benchmark protocol

For each scenario, measure both manual and `m1nd` warm passes on:

- time to first good answer
- time to full proof
- number of files opened
- number of repeated file reads
- token proxy
- confidence of answer
- whether the answer changed the eventual plan or edit set

### Working-style metrics

Track:

- `files_opened`
- `repeat_reads`
- `search_iterations`
- `time_to_first_good_answer`
- `time_to_full_proof`
- `plan_changes_after_initial_answer`
- `tests_identified_before_edit`
- `false_start_count`

These metrics may explain value better than token count alone.

## Additional Work Worth Doing

Beyond more A/B tests, the best follow-up work would be:

1. build a benchmark harness that logs tool calls, surfaced chars, and timestamps automatically
2. define a standard warm-graph corpus so runs are reproducible
3. separate benchmark suites by task class: triage, architecture, edit-prep, continuity, proof
4. add one benchmark section to the README and keep the rest in research docs
5. measure not just cost saved, but bad edits avoided and retries avoided

## Current Recommendation

Do not claim that `m1nd` saves tokens in every scenario.

Do claim that, in warm-graph structural tasks, the current measured evidence supports:

- strong savings in continuity and structural proof scenarios
- large savings across the best real benchmark set: about `74%`
- meaningful speed and confidence benefits even in scenarios where token savings are mixed

## Phase 3: Warm-Graph Workflow Benchmarks

The third round measured how `m1nd` changes the way an agent works once the graph is already warm.

This round focused on:

- `time_to_first_good_answer`
- `time_to_full_proof`
- files opened
- repeat reads
- search iterations
- whether `m1nd` changed the eventual answer or patch plan
- next patch candidates suggested by the scenario itself

### Warm-graph summary

| Scenario | Manual first good answer | Warm `m1nd` first good answer | Manual full proof | Warm `m1nd` full proof | Main outcome |
|---|---:|---:|---:|---:|---|
| Stacktrace/root-cause triage (`json_adapter`) | ~1.0s | ~0.4s | ~9.2s | not reached with `m1nd` alone | `m1nd` wins on fast localization, manual still wins on historical proof |
| Multi-file edit prep (`normalized_scope`) | 0.012s | 0.007s | 0.020s | 0.009s | `m1nd` wins on certainty and completeness, loses on compactness |
| Missing guard / structural absence (`apply_batch`) | 163ms | ~287ms | 204ms | ~653ms | manual wins on narrow proof, `m1nd` wins on structural triage |
| Continuity (`boot_memory`) | 0.025s | 24.022s | 0.055s | 55.161s | continuity value is real, but current payload and resume flow are too expensive |
| Warm semantic retrieval (`normalize_dispatch_tool_name`) | 15.315ms | 0.742ms | 15.613ms | 1.153ms | very strong warm-graph speed win |

### Phase 3 takeaway

The warm-graph round changed the benchmark story in an important way:

- `m1nd` does not always win on wall-clock time in a small repo when the seam is already easy to grep
- `m1nd` does often change the working style of the agent by reducing rediscovery, reducing query reformulation, and increasing plan completeness
- the current best warm-graph wins are:
  - semantic retrieval for code intent
  - fast structural localization
  - fuller patch planning before edit
- the current weak points are:
  - proof paths that depend on git/change history
  - continuity flows that still surface too much payload
  - structural tools that triage correctly but do not yet collapse quickly into a compact proof set

### How `m1nd` changes the agent workflow

Across the warm-graph scenarios, the behavioral difference was consistent:

- manual loops were smaller when the target was already easy to localize
- `m1nd` loops used fewer ad hoc grep reformulations
- `m1nd` surfaced more connected context before edit, which improved patch planning
- `m1nd` reduced rereads in continuity scenarios, even when total cost was still too high
- `m1nd` gave the agent more confidence about tests, neighboring files, and protocol surfaces

This suggests the main value is not just token savings. It is also:

- lower uncertainty before editing
- fewer missed neighboring files
- fewer rediscovery loops across steps
- better structural grounding for the same question

## Phase 3 Patch Candidates

The warm-graph round produced a useful shortlist of concrete next-patch opportunities.

### Progress update on this benchmark branch

The benchmark loop has already produced direct product changes on
`codex/benchmark-research-and-timeline-p1`.

Implemented so far:

- `timeline` now canonicalizes equivalent file identities before history lookup
- `apply_batch` verification now exposes `heuristics_surface_ref` and preserves explanation fallback
- `validate_plan` suppresses common manifest/artifact noise
- `surgical_context_v2` now prefers code-bearing neighbors over docs when slots are tight
- `seek` now handles long natural-language prompts more robustly
- `surgical_context_v2` now has an opt-in `proof_focused` mode for smaller connected proof sets
- `trail_save` now auto-derives structural boosts from visited nodes, hypotheses, and conclusions
- `trail_resume` now reactivates that derived structural memory without requiring explicit manual boosts
- `trail_resume` now returns `reactivated_node_ids` and `resume_hints` so the next useful move is explicit
- `trail_resume` now supports compact output limits for reactivated-node previews and resume hints
- `trail_resume` now returns `next_focus_node_id`, `next_open_question`, and `next_suggested_tool`
- temporal resume questions can now route toward `timeline`
- literal search now demotes fixture-like hardcoded identity noise in continuity-style queries
- a first benchmark harness now exists under `scripts/benchmark/run_benchmark.py` with scenario and run JSON support
- the harness now records workflow metadata such as `false_start_count`, `tests_identified_before_edit`, and `workflow_notes`
- aggregate summaries now track search-iteration, repeat-read, and false-start deltas

Current implication:

- the benchmark work is no longer only observational
- the repo now contains concrete patches aimed at reducing warm-graph payload and retrieval reformulation
- continuity is now less dependent on perfect caller-side bookkeeping
- continuity can now propose the next continuation seam instead of only restoring graph state
- continuity lookup should surface fewer false seams from test fixtures and mock paths
- future warm-graph runs can now be recorded in a repeatable JSON format instead of notes-only
- the next benchmark pass should explicitly measure `proof_focused` against the previous `surgical_context_v2` behavior

### First harness-backed mini-battery

The repository now includes four recorded runs plus one summary JSON under
`docs/benchmarks/runs/`.

Current harness-backed comparison set:

- `warm_semantic_retrieval_dispatch`
- `warm_continuity_boot_memory`
- `warm_proof_focused_edit_prep`
- `warm_structural_proof_apply_batch`
- `warm_impact_blast_radius_follow_up`
- `warm_continuity_actionable_resume`
- `warm_continuity_temporal_resume`
- `warm_trace_root_cause_triage`

Current aggregate from those recorded runs:

- manual token proxy: `6451`
- warm `m1nd` token proxy: `1786`
- aggregate token savings: `72.31%`
- manual first good answer total: `286.615ms`
- warm `m1nd` first good answer total: `206.556ms`
- manual search iterations: `10`
- warm `m1nd` search iterations: `2`
- manual repeat reads: `14`
- warm `m1nd` repeat reads: `7`
- manual false starts: `3`
- warm `m1nd` false starts: `0`
- warm `m1nd` guidance-followed count: `7`

Interpretation:

- the harness now confirms meaningful context compression across retrieval, continuity, edit-prep, and structural proof
- the recorded time values are still corpus-specific, but the aggregate warm-graph pass now also beats manual on first-good-answer time
- `proof_focused` edit prep is currently the strongest harness-backed compactness win in the corpus
- the new actionable continuity scenario is a strong workflow win: fewer searches, fewer rereads, and no false start before the next concrete move
- the new temporal continuity scenario is the first harness case that proves `trail_resume` guidance can be followed directly into `timeline`
- the refreshed `warm_continuity_boot_memory` scenario now also behaves like a strong continuity win once the guided resume flow is followed
- the earlier semantic retrieval outlier was a benchmark unit mismatch, not a product slowdown
- semantic retrieval now joins the guided-flow set too: `seek` can hand off directly into `view` on the winning file
- blast-radius follow-up now joins it as well: `impact` can point straight at the strongest downstream seam instead of leaving the dependent choice implicit
- `validate_plan` proof hints cut a whole step out of the `apply_batch` proof flow, which helped flip the aggregate timing result
- guidance-followed is now measurable in retrieval, continuity, structural-proof, edit-prep, triage, and blast-radius flows
- `trace` is now part of that same guided workflow story: suspect selection plus immediate follow-up on the right file
- the next useful benchmark step is to tighten mixed proof flows and remove the remaining synthetic timing noise

### New blast-radius result: `impact` now guides the first follow-up

The blast-radius scenario is now part of the harness corpus rather than living
only as product intuition.

| Scenario | Manual token proxy | Warm `m1nd` token proxy | Savings | Workflow effect |
|---|---:|---:|---:|---|
| Impact-guided blast-radius follow-up | 650 | 295 | 54.62% | Search iterations drop from `1` to `0`; repeat reads drop from `2` to `1`; guidance followed `1/1` |

Interpretation:

- `impact` no longer only returns a blast set; it now suggests the first downstream target worth opening
- that makes blast-radius inspection part of the same measurable guided workflow family as `seek`, `trace`, `trail_resume`, and `validate_plan`

### New triage result: `trace` now guides the next move

The trace/root-cause scenario is now part of the harness corpus instead of
living only in the earlier narrative rounds.

| Scenario | Manual token proxy | Warm `m1nd` token proxy | Savings | Workflow effect |
|---|---:|---:|---:|---|
| Trace-guided root-cause triage | 610 | 170 | 72.13% | Search iterations drop from `1` to `0`; false starts drop from `1` to `0`; guidance followed `1/1` |

Interpretation:

- `trace` no longer only ranks suspects; it now hands the agent the next concrete move
- that makes stacktrace triage part of the same measurable guidance surface as continuity and proof workflows

### New continuity result: actionable resume hints

The newest recorded scenario tests whether `trail_resume` can reopen the next
useful question instead of merely restoring latent graph state.

| Scenario | Manual token proxy | Warm `m1nd` token proxy | Savings | Workflow effect |
|---|---:|---:|---:|---|
| Actionable continuity resume | 1340 | 145 | 89.18% | Search iterations drop from `2` to `0`; repeat reads drop from `3` to `1`; false starts drop from `1` to `0` |

Interpretation:

- this is the first continuity scenario in the harness that looks like a public-quality `m1nd` win
- the main benefit is reduced rediscovery, not a broad universal speed claim
- the compact resume limits make that win cheaper in payload without changing the shape of the workflow
- the returned `next_focus_node_id`, `resume_hints`, and `next_suggested_tool` make the continuation seam explicit instead of forcing a fresh search loop
- this makes continuity a more defensible `m1nd` value surface for docs and release notes

### New continuity result: temporal resume routed into `timeline`

The newest follow-up scenario tests a narrower but important claim: when the
carried-forward question is temporal, can `trail_resume` route the agent
directly into the right proof tool instead of forcing another discovery loop?

| Scenario | Manual token proxy | Warm `m1nd` token proxy | Savings | Workflow effect |
|---|---:|---:|---:|---|
| Temporal continuity resume | 440 | 175 | 60.23% | Search iterations drop from `2` to `0`; false starts drop from `1` to `0`; guidance followed `1/1` |

Interpretation:

- this is the first harness-backed proof that `trail_resume` can route directly into `timeline`
- the main win is not just compression; it is eliminating the intermediate rediscovery step
- this makes `next_suggested_tool` measurable instead of anecdotal inside the benchmark corpus

### Updated continuity result: boot-memory resume now follows guided focus

The older boot-memory continuity scenario was rerun after the `trail_resume`
guidance changes instead of following the pre-guidance search-heavy flow.

| Scenario | Manual token proxy | Warm `m1nd` token proxy | Savings | Workflow effect |
|---|---:|---:|---:|---|
| Boot-memory continuity resume | 1043 | 148 | 85.81% | Search iterations drop from `1` to `0`; repeat reads drop from `2` to `1`; guidance followed `1/1` |

Interpretation:

- the earlier heavy continuity outlier was mostly a stale benchmark path, not the current product behavior
- guided resume now reopens the session persistence seam directly instead of forcing search plus surgical expansion
- continuity is no longer the main drag in the harness once the updated resume flow is used

### Updated structural-proof result: `validate_plan` now carries a compact proof hint

The `apply_batch` proof scenario was rerun after adding a compact `proof_hint`
to heuristic hotspots in `validate_plan`.

| Scenario | Manual token proxy | Warm `m1nd` token proxy | Savings | Workflow effect |
|---|---:|---:|---:|---|
| Structural proof around `apply_batch` | 693 | 115 | 83.41% | Removes the immediate `heuristics_surface` follow-up and cuts repeat reads from `1` to `0` |

Interpretation:

- `validate_plan` now carries enough compact proof to justify the first risky verdict without an immediate second call
- `validate_plan` now also emits explicit next-step guidance, so the proof flow can be measured as a guided one-step handoff instead of an implicit manual decision
- this did not fully beat manual in the single-scenario first-answer time, but it materially improved compactness and helped flip the aggregate warm-graph timing result

### Priority 1

1. Fix `timeline` history fidelity for recently changed files.
   Why: root-cause triage could localize the right file fast, but `timeline` did not surface the recent hardening commit that actually explained the bug.
   Likely files: `m1nd-mcp/src/layer_handlers.rs`

2. Canonicalize file identities across `search`, `activate`, and `timeline`.
   Why: the same file surfaced under multiple external-id shapes, which likely hurts ranking clarity and historical lookup.
   Likely files: `m1nd-mcp/src/search_handlers.rs`, `m1nd-mcp/src/layer_handlers.rs`

3. Add `heuristics_surface_ref` parity to `VerificationImpact` in `apply_batch`.
   Why: verification returns inline heuristic summary, but not the same explorable reference affordance that similar plan/report flows already expose.
   Likely files: `m1nd-mcp/src/protocol/surgical.rs`, `m1nd-mcp/src/surgical_handlers.rs`

4. Stop dropping hotspot explanation when `node_id` cannot be resolved after write.
   Why: this causes the explanation to disappear exactly when the graph resolution path is weak.
   Likely files: `m1nd-mcp/src/surgical_handlers.rs`

### Priority 2

1. Make `validate_plan` less noisy for API/protocol edit scenarios.
   Why: it keeps surfacing unrelated `Cargo.toml` files and marginal artifacts, which lowers trust.
   Likely files: `m1nd-mcp/src/layer_handlers.rs`

2. Make `surgical_context_v2` support a tighter proof-focused or patch-focused mode.
   Why: it finds the right neighborhood, but often with more payload than needed.
   Likely files: `m1nd-mcp/src/surgical_handlers.rs`, `m1nd-mcp/src/protocol/surgical.rs`

3. Improve `seek` ranking for long natural-language prompts.
   Why: short code-shaped prompts worked much better than natural phrasing.
   Likely files: `m1nd-core` query/ranking path, `m1nd-mcp/src/search_handlers.rs`

4. Bias `seek` more strongly toward exact semantic anchors when terms like `alias`, `canonical`, `status`, or `dispatch` appear together.
   Why: warm semantic retrieval is promising, but still prompt-sensitive.
   Likely files: `m1nd-core`, `m1nd-mcp/src/search_handlers.rs`

### Priority 3

1. Validate `trail_resume` ranking and usefulness in longer real investigations.
   Why: structural reactivation and actionable hints now exist, but the next-focus and next-tool guidance still need broader validation outside the starter corpus.
   Likely files: `m1nd-mcp/src/layer_handlers.rs`, `m1nd-mcp/src/protocol/layers.rs`

2. Continue tightening guidance quality and latency in mixed proof workflows.
   Why: continuity has improved materially, and the remaining timing gap now comes more from proof-heavy mixed flows than from guided continuity.
   Likely files: `m1nd-mcp/src/server.rs`, `m1nd-mcp/src/layer_handlers.rs`

3. Reduce noisy fixture-style matches inside literal search.
   Why: continuity queries over `persist_boot_memory` surfaced hardcoded fixture-like paths that polluted the real answer path.
   Likely files: `m1nd-mcp/src/search_handlers.rs`
