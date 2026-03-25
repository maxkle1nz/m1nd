# Benchmarks

For the raw research artifacts and patch backlog that produced the current public numbers, see:

- `docs/BENCHMARK_RESEARCH_2026-03-24.md`
- `docs/BENCHMARK_PATCH_PLAN_2026-03-24.md`
- `docs/BENCHMARK_HARNESS_SPEC_2026-03-24.md`

This page is the short product-truth layer for the current benchmark corpus.

## What We Measure Now

The current benchmark system is not only about token proxy. It also tracks whether m1nd improves workflow behavior:

- token proxy / context churn
- `false_starts`
- guided follow-through
- recovery loops
- proof-state progression
- progress observability on long-running writes

That matters because some of m1nd’s strongest wins are continuity, repair, and execution clarity rather than raw compression in every single scenario.

## Current Warm-Graph Corpus

The current recorded aggregate warm-graph corpus shows:

| Metric | Manual | `m1nd_warm` | Result |
|--------|--------|-------------|--------|
| Aggregate token proxy | `10518` | `5182` | `50.73%` reduction |
| False starts | `14` | `0` | m1nd eliminates the recorded false starts |
| Guided follow-throughs | `0` | `31` | guided next-step behavior is being followed in real runs |
| Successful recovery loops | `0` | `12` | repair loops are closing instead of restarting from scratch |

These are the public numbers reflected in the README and landing. They are the benchmark truth to mirror across docs.

## Representative Engine Timings

The underlying engine remains fast on the measured production backend (~335 files, ~52K lines, 9,767 nodes, 26,557 edges):

| Operation | Time | Notes |
|-----------|------|-------|
| Full ingest | ~910ms | Walk + extract + resolve + finalize |
| Activate query | ~31ms | Four-dimensional ranking |
| Impact analysis | ~5ms | Blast-radius path |
| Trace analysis | ~3.5ms | Stacktrace to suspects |
| Trail resume | ~0.2ms | Continuity restore + hints |
| Apply batch | ~165ms | Atomic multi-file write before deeper verification |

These timings are useful, but they are no longer the whole story. Current m1nd is also measured on guided behavior and recovery quality.

## Where m1nd Wins

m1nd wins most clearly when the task is structural, stateful, or risky:

- stacktrace triage with `trace`
- blast-radius analysis with `impact`
- continuity restoration with `trail_resume`
- edit preparation with `surgical_context_v2` and `validate_plan`
- long-running writes with `apply_batch`
- repair loops after invalid regex, stale route sets, stale trails, protected writes, and stale edit previews

## Where Plain Tools Still Win

m1nd is not the headline tool for:

- exact text search
- one-file lookup when you already know the file
- compiler truth
- runtime logs and debugger work

Use `rg`, the compiler, the test runner, and logs when execution truth is the question. Use m1nd when navigation and connected structure are the bottleneck.

## Why The Corpus Matters

The benchmark corpus is now part of product development, not just a marketing appendix.

Recent runtime and UX improvements were driven directly by measured benchmark pain:

- `proof_state` and next-step guidance on core flows
- more actionable `trail_resume`
- better `seek` handling for natural-language prompts
- reduced `validate_plan` noise
- more useful `surgical_context_v2`
- observable `apply_batch` progress and SSE handoff
- recovery-oriented error payloads for invalid or stale tool calls

## Reproducibility

To inspect the current benchmark system:

```bash
git clone https://github.com/maxkle1nz/m1nd.git
cd m1nd
cargo build --release --workspace
python3 scripts/benchmark/run_benchmark.py --help
python3 scripts/benchmark/summarize_benchmarks.py --help
```

The versioned scenarios, events, and run outputs live under `docs/benchmarks/`.

### Memory

| Component | Size |
|-----------|------|
| Graph (9,767 nodes, 26,557 edges) | ~2MB |
| Plasticity state | ~500KB |
| Perspective state (per active perspective) | ~100KB |
| Lock baselines (per lock) | ~200KB |
| Trail storage (per saved trail) | ~50KB |
| JSON-RPC server overhead | ~5MB |
| **Typical total** | **~50MB** |

Memory scales linearly with graph size. A 100K-node graph would use approximately 20MB for the graph alone, with similar overhead for the server.

### CPU

m1nd is single-threaded for graph operations (no lock contention, deterministic results). Ingest uses Rayon for parallel file parsing. During query serving, CPU usage is negligible between queries and spikes briefly during activation (31-77ms of computation).

On an Apple M2, the server at idle uses <0.1% CPU. During a burst of queries, it peaks at ~5% of a single core.

## Scaling Characteristics

### Ingest Time vs Codebase Size

Ingest scales linearly with file count. Reference resolution is roughly O(n log n) where n is the number of cross-file references.

| Files | Estimated Ingest Time | Estimated Nodes |
|-------|-----------------------|-----------------|
| 100 | ~270ms | ~3,000 |
| 335 | 910ms (measured) | 9,767 (measured) |
| 1,000 | ~2.7s | ~29,000 |
| 10,000 | ~27s | ~290,000 |
| 100,000 | ~4.5min | ~2,900,000 |

### Activation Time vs Graph Size

Spreading activation is bounded by the number of edges traversed, which depends on graph density and query specificity rather than total graph size. Activation in a 100K-node graph is estimated at 100-200ms.

### Persistence Time vs State Size

JSON serialization scales linearly with state size. A 10K-node graph persists in under 100ms. A 100K-node graph would take approximately 1 second.

## Reproducibility

To reproduce these benchmarks:

```bash
git clone https://github.com/cosmophonix/m1nd.git
cd m1nd
cargo build --release

# Start the server
./target/release/m1nd-mcp
```

Then send the JSON-RPC calls from the [Examples](../EXAMPLES.md) document against your own codebase. Times will vary based on:

- Hardware (CPU speed, memory bandwidth)
- Codebase size and language
- Graph density (codebases with many cross-references produce denser graphs)
- Plasticity state (learned weights affect activation propagation paths)

Report your benchmarks via GitHub Issues with the `benchmark` label.
