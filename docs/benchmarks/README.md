# Benchmark Harness

This directory holds reproducible benchmark inputs for `m1nd`.

## Runner

Use:

```bash
python3 scripts/benchmark/run_benchmark.py \
  --scenario docs/benchmarks/scenarios/semantic_retrieval_dispatch.json \
  --mode m1nd_warm \
  --events docs/benchmarks/events/sample-semantic-retrieval.json \
  --time-to-first-good-answer-ms 740 \
  --time-to-full-proof-ms 1153 \
  --answer-quality high \
  --public-claim-worthy \
  --output docs/benchmarks/runs/semantic-retrieval-dispatch.m1nd_warm.json
```

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

If `payload_chars` is omitted, the runner derives a conservative char count
from the strings present in the event.

## Current scenario corpus

- `semantic_retrieval_dispatch.json`
- `continuity_boot_memory.json`

These are warm-graph oriented starter scenarios for rerunning the benchmark
work captured in `docs/BENCHMARK_RESEARCH_2026-03-24.md`.
