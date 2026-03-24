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
- `next_tool_used`

If `payload_chars` is omitted, the runner derives a conservative char count
from the strings present in the event.

Run-level metadata can also record:

- `false_start_count`
- `tests_identified_before_edit`
- `workflow_notes`

For continuity scenarios, capture whether the run only restored context or also
surfaced the next move. The actionable-resume scenarios are meant to benchmark
"resume and continue" behavior, not bookmark restore alone.

For proof and planning scenarios, use the same guidance fields when a tool now
suggests the next surface directly. This lets the corpus measure when one tool
collapses both explanation and next-step routing into a single step.

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
- `impact_blast_radius_follow_up.json` captures `impact` plus guided follow-up into the strongest downstream seam
- `hypothesize_structural_claim_follow_up.json` captures `hypothesize` plus guided follow-up into the strongest proof target
- `semantic_retrieval_dispatch.json` captures `seek` plus guided follow-up into the winning file
- `trace_root_cause_triage.json` captures trace-driven suspect selection plus guided follow-up into the right file
