# m1nd Benchmark Harness Spec

Date: 2026-03-24
Repo: `/Users/cosmophonix/SISTEMA/.codex-tmp/m1nd-standalone`

## Purpose

Manual benchmarking was enough to discover patterns, but it is too noisy for repeated product claims.

The next step should be a lightweight benchmark harness that logs:

- wall-clock timing
- tool sequence
- files surfaced
- repeat reads
- approximate surfaced chars
- token proxy
- benchmark metadata

## What the Harness Should Measure

For each pass:

- `scenario_id`
- `scenario_name`
- `mode`: `manual`, `m1nd_cold`, `m1nd_warm`
- `time_to_first_good_answer_ms`
- `time_to_full_proof_ms`
- `files_opened`
- `repeat_reads`
- `search_iterations`
- `chars_surfaced`
- `token_proxy`
- `answer_quality`
- `plan_changed`
- `notes`

For each tool event:

- timestamp
- tool name
- query or target
- elapsed time
- payload size in chars
- surfaced file ids

## Minimal Implementation Plan

### Phase 1

Add a script that accepts a structured scenario file and records events from a benchmark run.

Suggested path:
- `scripts/benchmark/run_benchmark.py`

Suggested inputs:
- scenario definition in JSON or TOML
- mode (`manual`, `m1nd_warm`)
- output file path

Suggested outputs:
- one JSON record per run
- one summary JSON for rollups

### Phase 2

Add a small scenario corpus:

- root-cause triage
- structural claim proof
- layer inspection
- edit prep
- continuity

Suggested path:
- `docs/benchmarks/scenarios/`

### Phase 3

Add one summarizer:

- aggregate token savings
- aggregate time savings
- best and worst scenarios
- outlier detection

Suggested path:
- `scripts/benchmark/summarize_benchmarks.py`

## Data Rules

- warm-graph runs must explicitly exclude ingest/setup time
- cold-graph runs must include ingest/setup time
- token proxy should be computed consistently as `ceil(chars / 4)`
- benchmark output must label whether a result is public-claim-worthy or internal-only

## Why This Matters

The harness is not just for marketing claims. It is also a product loop:

- every patch candidate should improve a benchmark
- every regression should be visible
- warm-graph behavior should be measurable, not guessed

## Suggested First Patch After Harness Exists

Use the harness to validate these patch groups:

1. `timeline` + canonical identity normalization
2. `VerificationImpact` parity fixes
3. `validate_plan` noise reduction
4. `seek` prompt robustness
5. `trail_resume` continuity strengthening
