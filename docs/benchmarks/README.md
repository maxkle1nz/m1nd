# Benchmark Harness

This directory holds reproducible benchmark inputs for `m1nd`.

## Agent Reliability Rounds

Use this when you want a blinded, agent-first comparison between lanes with
`m1nd` available and lanes without `m1nd`.

There are three round families:

- `agent_reliability_round.py`: host/session recovery, stale binding, transport, and runtime trust
- `real_world_agent_round.py`: normal coding-agent work on external repositories
- `bug_hunt_round.py`: blinded seeded-defect recall scoring for real repo audits

For product usefulness claims, prefer the real-world round. The host-recovery
round is still valuable, but it only measures whether agents can tell when the
tooling environment is confused.

Create a round:

```bash
python3 scripts/benchmark/agent_reliability_round.py init \
  --out-dir docs/benchmarks/agent-rounds/round-001 \
  --repo . \
  --round-id round-001 \
  --json
```

The command writes:

- `round.json` with the 7-lane protocol
- `lane-prompts/*.md` for 3 `m1nd_available` lanes, 3 `no_m1nd` lanes, and 1 adjudicator
- `lane-results/*.json` templates for each lane to fill after work

Score the completed lane results:

```bash
python3 scripts/benchmark/agent_reliability_round.py score \
  --runs-dir docs/benchmarks/agent-rounds/round-001/lane-results \
  --output docs/benchmarks/agent-rounds/round-001/report.json \
  --round-id round-001 \
  --json
```

This report is intentionally conservative. It records success rates, median
scores, recovery follow-through, false starts, failure taxonomy, and agent
testimony, but `public_claim_worthy` remains `false` for a single round. Repeat
comparable rounds before turning any result into public copy.

Each task result also separates the kind of evidence gathered:

- `requires_live_proof`: the task cannot be fully proved from docs/code alone
- `proof_mode`: `live`, `static`, `route_only`, `mixed`, or `unreported`
- `live_state_verified`: the lane actually observed the live host/runtime/session state
- `evidence_origin`: source classes such as `m1nd_probe`, `direct_files`, or `test_output`
- `raw_event_evidence`: auditable event lines or transcript references when available

This distinction matters for recovery benchmarks. A control lane may correctly
describe the `Transport closed` route from source files, but that is still not
the same evidence as watching a live transport fail and recover. The report
therefore includes `live_required_verified_rate` and `live_proof_gap_count` so
static route knowledge cannot masquerade as runtime proof.

The report keeps `structurally_comparable_primary_arms` separate from
`live_proof_comparable_primary_arms`. A round can have the right number of lanes
and tasks while still being non-comparable for live-proof claims. In that case
`comparable_primary_arms` is `false`, and `comparability_blockers` explains why.

If a lane result still looks like an untouched template, the report lists it in
`template_like_lanes` and marks the primary arms as not comparable. This prevents
empty scaffolds from being mistaken for evidence.

The scorer only consumes JSON files whose `schema` is
`m1nd-agent-reliability-lane-result-v0`. Other JSON files in `lane-results/`
are listed under `ignored_result_files` so stray runtime artifacts cannot crash
or silently pollute the round.

## Real-World Agent Rounds

Use this when the question is whether `m1nd` helps with everyday code work in
unknown repos: audit, localization, flow explanation, bug triage, safe change
planning, small patches, seeded bug fixes, bounded refactors, code review, and
docs drift.

Fetch external fixture repos into the ignored local fixture directory:

```bash
python3 scripts/benchmark/real_world_agent_round.py fetch-fixtures \
  --fixtures-dir .m1nd-benchmark-fixtures/real-world \
  --json
```

Create a round:

```bash
python3 scripts/benchmark/real_world_agent_round.py init \
  --out-dir docs/benchmarks/real-world-rounds/round-001 \
  --fixtures-dir .m1nd-benchmark-fixtures/real-world \
  --round-id round-001 \
  --json
```

The round includes deterministic task payloads, a parent/adjudicator-only
`operator-only/answer-key.json`, and supplied benchmark payload artifacts such
as review diffs. Do not hand the answer key to primary lanes.

It also writes `operator-only/fixture-lock.json`, capturing the local fixture
HEAD commits used for the round. `prepare-lane-fixtures` checks lane checkout
HEADs against that lock and reports `ok=false` on mismatch.

It also creates `event-streams/*.jsonl`. The harness writes the first
`lane_assigned` event; agents append their own events with
`event_source="agent"`. A lane without agent-authored events remains useful for
setup smoke, but not for a clean benchmark claim.

Prepare isolated lane fixture checkouts:

```bash
python3 scripts/benchmark/real_world_agent_round.py prepare-lane-fixtures \
  --round-file docs/benchmarks/real-world-rounds/round-001/round.json \
  --write docs/benchmarks/real-world-rounds/round-001/lane-workspaces.json \
  --json
```

This step injects deterministic seeded artifacts into isolated lane workspaces.
For the first v2 task set, Click receives
`tests/test_m1nd_seeded_callable_type.py` so every primary lane fixes the same
callable-instance custom type bug.

Score the completed lane results:

```bash
python3 scripts/benchmark/real_world_agent_round.py score \
  --runs-dir docs/benchmarks/real-world-rounds/round-001/lane-results \
  --output docs/benchmarks/real-world-rounds/round-001/report.json \
  --round-id round-001 \
  --json
```

Build an operator-only judge packet:

```bash
python3 scripts/benchmark/real_world_agent_round.py judge-input \
  --round-file docs/benchmarks/real-world-rounds/round-001/round.json \
  --lane-results-dir docs/benchmarks/real-world-rounds/round-001/lane-results \
  --answer-key docs/benchmarks/real-world-rounds/round-001/operator-only/answer-key.json \
  --output docs/benchmarks/real-world-rounds/round-001/operator-only/judge-input.json \
  --json
```

The judge packet uses `m1nd-real-world-agent-judge-input-v0` and includes the
task matrix, answer key, primary lane summaries, event summaries, and empty
adjudication templates.

This round uses `m1nd-real-world-agent-round-v0`,
`m1nd-real-world-agent-lane-result-v0`, and
`m1nd-real-world-agent-report-v0`. The optional event stream schema is
`m1nd-real-world-agent-event-v0`. The judge lane can fill top-level
`adjudications[]` in its lane result so the report can separate self-score from
adjudicated score. `public_claim_worthy` remains `false` until results are
repeated across pinned repo snapshots, all primary lanes have event evidence,
and patch/review tasks are adjudicated.

## Bug-Hunt Rounds

Use this when the question is whether agents find more real defects when m1nd is
available and taught correctly. A bug-hunt round gives agents an ordinary repo
audit task while the operator keeps a private answer key of seeded defects.

Instruction modes:

- `m1nd-full-spec`: the agent receives the full m1nd operating layer and should
  choose the best tool combination for the situation. This is for testing
  whether a complete route table helps broad/hard work or slows narrow recall.
- `m1nd-temponizer-full`: the agent receives the trained-agent loop plus the
  explicit Temponizer formula and per-phase recalibration notes. This is useful
  for testing prompt integration, but may be too heavy for recall tasks.
- `m1nd-temponizer-compact`: the agent receives the trained-agent loop plus the
  full Temponizer formula in compact form: calculate corrected agent time around
  major branch decisions, record `Te` where it matters, and keep moving.
- `m1nd-temponizer`: the earlier lighter Tempo mode with phase/time awareness
  and `Te` notes, but without the full formula.
- `m1nd-trained`: the agent receives the default trained-agent loop from the
  m1nd pack: trust check, recovery before absence, scoped orientation,
  retrieval-envelope reading, direct proof, and evidence logging.
- `m1nd-basic`: the agent knows m1nd is available but does not receive the full
  operating loop.
- `direct`: the agent does not use m1nd and relies on direct repo tools.

The first accepted round showed that `m1nd-trained` was the meaningful product
condition. Do not collapse it into "m1nd installed" when designing future
rounds. The first Tempo comparison showed that prompt weight matters too:
Temponizer should be compact operating physics, not an audit-form tax.

Create a round scaffold:

```bash
python3 scripts/benchmark/bug_hunt_round.py init \
  --out-dir docs/benchmarks/bug-hunt-rounds/round-001 \
  --round-id round-001 \
  --repo your-fixture-repo \
  --source-repo .m1nd-benchmark-fixtures/bug-hunt/your-fixture-source \
  --seeded-repo .m1nd-benchmark-fixtures/bug-hunt/round-001/your-fixture-seeded \
  --seeded-bug-count 5 \
  --json
```

The init command writes lane prompts, lane result templates, event streams,
`round.json`, and an operator-only answer-key template. It does not plant bugs
or prepare workspaces; the operator must do that before dispatching agents.

Score a completed round:

```bash
python3 scripts/benchmark/bug_hunt_round.py score \
  --round-file docs/benchmarks/bug-hunt-rounds/round-001/round.json \
  --answer-key docs/benchmarks/bug-hunt-rounds/round-001/operator-only/answer-key.json \
  --lane-results-dir docs/benchmarks/bug-hunt-rounds/round-001/lane-results \
  --output docs/benchmarks/bug-hunt-rounds/round-001/report.json \
  --notes docs/benchmarks/bug-hunt-rounds/round-001/ROUND-NOTES.md \
  --json
```

The scorer reports seeded recall by instruction mode and preserves extra
findings as `extra_unadjudicated_findings_count`. It does not treat extras as
false positives unless a separate judge validates them. `public_claim_worthy`
stays `false` for a single internal round.

The first accepted bug-hunt round is:

- `docs/benchmarks/bug-hunt-rounds/bughunt-humanize-20260514T021500Z/report.json`

## Runner

Use:

```bash
python3 scripts/benchmark/run_benchmark.py \
  --scenario docs/benchmarks/scenarios/semantic_retrieval_dispatch.json \
  --mode m1nd_warm \
  --events docs/benchmarks/events/sample-semantic-retrieval.json \
  --time-to-first-good-answer-ms 0.742 \
  --time-to-full-proof-ms 1.153 \
  --answer-quality high \
  --false-start-count 0 \
  --tests-identified-before-edit 0 \
  --workflow-notes "one-shot retrieval, no query reformulation" \
  --public-claim-worthy \
  --output docs/benchmarks/runs/semantic-retrieval-dispatch.m1nd_warm.json
```

Summarize a corpus with:

```bash
python3 scripts/benchmark/summarize_benchmarks.py \
  --runs-dir docs/benchmarks/runs \
  --output docs/benchmarks/runs/summary.json
```

Optional sensitivity inputs:

- `--input-price-per-1m 5`
- `--time-value-per-hour-usd 100`

Optional provenance inputs:

- `--execution-origin live|replay|snapshot`
- `--source-ref docs/benchmarks/events/warm-structural-proof-apply-batch.json`

## Event format

The `--events` file is a JSON array. Each item can contain:

- `tool_name`
- `query`
- `target`
- `elapsed_ms`
- `payload_chars`
- `opened_files`
- `surfaced_files`
- `notes`
- `reactivated_node_ids`
- `resume_hints`
- `next_focus_node_id`
- `next_open_question`
- `next_suggested_tool`
- `next_suggested_target`
- `next_step_hint`
- `what_is_missing`
- `proof_hint`
- `proof_state`
- `batch_id`
- `next_tool_used`
- `status_message`
- `phases`
- `progress_events`
- `progress_delivery`

If `payload_chars` is omitted, the runner derives a conservative char count
from the strings present in the event.

Run-level metadata can also record:

- `false_start_count`
- `tests_identified_before_edit`
- `workflow_notes`
- `execution_origin`
- `source_ref`
- `recovery_events`
- `recovery_followed`

When a scenario covers auto-correctible errors, also record whether the run used:

- returned hint text
- returned example shape
- returned next-step guidance
- tool reroute vs same-tool retry

The harness now surfaces this explicitly as:

- `recovery_events`
- `recovery_followed`

For guided runtime scenarios, also capture when a tool names the missing proof
or inspection step explicitly. The harness tracks this as:

- `missing_signals`
- `missing_resolved`

Use `missing_resolved` when the very next move closes the gap the runtime named,
for example opening the file that `activate`, `search`, or `glob` said still
needed confirmation.

Use `recovery_followed` when the agent actually takes the hinted retry path
instead of falling back to a fresh discovery sweep.

For continuity scenarios, capture whether the run only restored context or also
surfaced the next move. The actionable-resume scenarios are meant to benchmark
"resume and continue" behavior, not bookmark restore alone.

For error-recovery scenarios, treat the win as "shorter repair loop" rather than
"fewer total errors". The benchmark should capture whether the tool taught the
agent how to recover without falling back to a fresh grep/read sweep.

For proof and planning scenarios, use the same guidance fields when a tool now
suggests the next surface directly. This lets the corpus measure when one tool
collapses both explanation and next-step routing into a single step.

Where the tool supports it, benchmark events may also include `proof_state`.
Current states:

- `blocked`
- `triaging`
- `proving`
- `ready_to_edit`

For long-running write scenarios such as `apply_batch`, benchmark the returned
`status_message`, coarse progress fields, `phases`, and `progress_events` too.
This keeps UX/progress work measurable instead of leaving it as a subjective shell/UI impression.
When available, capture `batch_id` too so live SSE progress, replay, and the
final result can be tied back to the same execution.
If the final `batch_completed` event carries `proof_state` and next-step guidance,
record that too. The event stream itself should be benchmarked as a usable handoff,
not only as a visual progress bar.

When progress is present, record how it arrived:

- `progress_delivery="live"` for events emitted during execution on the SSE bus
- `progress_delivery="replay"` for events re-emitted after the batch finished
- `progress_delivery="snapshot"` for one-shot coarse progress snapshots without an event stream

For `proof_focused_edit_prep`, treat the scenario as a compact proof handoff
into planning, not as an automatic `ready_to_edit` claim. In the current corpus
that flow ends in `proof_state="proving"`, which is the correct public reading.

For `impact_blast_radius_follow_up`, treat `proof_state="proving"` as “the seam
is strong enough to inspect next,” not as “edit immediately.” The win there is
guided downstream targeting, not skipping proof.

## Current scenario corpus

- `semantic_retrieval_dispatch.json`
- `continuity_boot_memory.json`
- `proof_focused_edit_prep.json`
- `structural_proof_apply_batch.json`
- `continuity_actionable_resume.json`
- `continuity_temporal_resume.json`
- `impact_blast_radius_follow_up.json`
- `hypothesize_structural_claim_follow_up.json`
- `trace_root_cause_triage.json`
- `search_invalid_regex_recovery.json`
- `search_ambiguous_scope_recovery.json`
- `perspective_stale_route_recovery.json`
- `perspective_guided_navigation.json`
- `edit_preview_source_modified_recovery.json`
- `apply_batch_path_safety_recovery.json`
- `apply_batch_protected_state_recovery.json`
- `lock_diff_guided_follow_up.json`
- `activate_guided_entrypoint.json`
- `search_guided_file_follow_up.json`
- `help_guided_tool_handoff.json`

These are warm-graph oriented starter scenarios for rerunning the benchmark
work captured in `docs/benchmarks/BENCHMARK_RESEARCH_2026-03-24.md`.

In particular:

- `continuity_boot_memory.json` captures the older, heavier continuity flow
- `continuity_actionable_resume.json` captures compact resume plus next-step guidance
- `continuity_temporal_resume.json` captures compact resume that routes directly into `timeline`
- `impact_blast_radius_follow_up.json` captures `impact` plus guided follow-up into the strongest downstream seam, with `proof_state` showing when blast analysis has moved from triage into proof
- `hypothesize_structural_claim_follow_up.json` captures `hypothesize` plus guided follow-up into the strongest proof target
- `semantic_retrieval_dispatch.json` captures `seek` plus guided follow-up into the winning file, with `proof_state` showing when retrieval has already moved from loose localization into file-level proof
- `trace_root_cause_triage.json` captures trace-driven suspect selection plus guided follow-up into the right file
- `search_invalid_regex_recovery.json` captures a concrete repair loop where `search` rejects an invalid regex, suggests `literal` mode, and the agent retries successfully without falling back to shell grep
- `search_ambiguous_scope_recovery.json` captures a scope-repair loop where `search` rejects an ambiguous `auto_ingest` scope, teaches the agent to retry with an explicit path, and avoids manual file guessing
- `perspective_stale_route_recovery.json` captures a stateful recovery loop where a stale `route_set_version` points the agent to `perspective_routes`, then back into a fresh `perspective_follow` without restarting the investigation
- `perspective_guided_navigation.json` captures the new runtime contract on `perspective_start` and `perspective_inspect`, where navigation can move straight into inspect and peek without an extra manual route-list detour
- `edit_preview_source_modified_recovery.json` captures a write-safety recovery loop where `edit_commit` rejects a stale preview, teaches the agent to rerun `edit_preview`, and keeps the edit flow safe without forcing a blind write
- `apply_batch_path_safety_recovery.json` captures a long-running write-safety recovery loop where `apply_batch` rejects an out-of-root target, teaches the agent to retry inside the ingested workspace, and preserves progress plus handoff on the successful retry
- `apply_batch_protected_state_recovery.json` captures a runtime-safety recovery loop where `apply_batch` rejects a protected `graph/plasticity` state file, redirects the agent back to a source file, and preserves the proof handoff after the safe retry
- `lock_diff_guided_follow_up.json` captures `lock_create` and `lock_diff` as a guided chain, where the lock layer can now point directly at the changed file to inspect next instead of leaving the agent to guess
- `activate_guided_entrypoint.json` captures `activate` as a proper runtime entrypoint that can hand the agent straight into the strongest file instead of forcing a follow-up search pass
- `activate_guided_entrypoint.json` now also captures `what_is_missing` as the explicit confirmation step the runtime still needs before activation can escalate
- `search_guided_file_follow_up.json` captures `search` as a guided retrieval surface that can route directly into the winning file-level follow-up, and now also records the missing confirmation step that the next file open resolves
- `help_guided_tool_handoff.json` captures `help` as workflow selection, where a tool page can now hand the agent into the next downstream operation instead of stopping at documentation alone, while still naming the downstream step that remains undone
- `structural_proof_apply_batch.json` now also captures compact proof hints from `validate_plan` plus measurable `apply_batch` progress metadata such as `progress_pct`, detailed `progress_events`, and the post-batch handoff into the next proof surface
- `structural_proof_apply_batch.json` currently marks `apply_batch` progress as `live`, which reflects the current serve-mode behavior rather than the older replay-only contract
- `proof_focused_edit_prep.json` captures `surgical_context_v2` as a guided handoff into edit prep rather than a context blob alone

If you add a new public benchmark claim, keep this section in lockstep with the actual files present in `docs/benchmarks/scenarios`.
