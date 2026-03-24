# Benchmark Harness

This directory holds reproducible benchmark inputs for `m1nd`.

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

For continuity scenarios, capture whether the run only restored context or also
surfaced the next move. The actionable-resume scenarios are meant to benchmark
"resume and continue" behavior, not bookmark restore alone.

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

These are warm-graph oriented starter scenarios for rerunning the benchmark
work captured in `docs/BENCHMARK_RESEARCH_2026-03-24.md`.

In particular:

- `continuity_boot_memory.json` captures the older, heavier continuity flow
- `continuity_actionable_resume.json` captures compact resume plus next-step guidance
- `continuity_temporal_resume.json` captures compact resume that routes directly into `timeline`
- `impact_blast_radius_follow_up.json` captures `impact` plus guided follow-up into the strongest downstream seam, with `proof_state` showing when blast analysis has moved from triage into proof
- `hypothesize_structural_claim_follow_up.json` captures `hypothesize` plus guided follow-up into the strongest proof target
- `semantic_retrieval_dispatch.json` captures `seek` plus guided follow-up into the winning file, with `proof_state` showing when retrieval has already moved from loose localization into file-level proof
- `trace_root_cause_triage.json` captures trace-driven suspect selection plus guided follow-up into the right file
- `structural_proof_apply_batch.json` now also captures compact proof hints from `validate_plan` plus measurable `apply_batch` progress metadata such as `progress_pct`, detailed `progress_events`, and the post-batch handoff into the next proof surface
- `structural_proof_apply_batch.json` currently marks `apply_batch` progress as `live`, which reflects the current serve-mode behavior rather than the older replay-only contract
- `proof_focused_edit_prep.json` captures `surgical_context_v2` as a guided handoff into edit prep rather than a context blob alone
