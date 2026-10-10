# Evidence and Benchmarks

m1nd has several kinds of evidence. Keep them separate when evaluating the product.

## First: does it help on your repository?

Use [Quick Start](tutorials/quickstart.md) for a read-only trial. Pick a real navigation question, check the returned source locations, and record whether the graph helped you reach a verified answer. Include setup, ingestion, model loading, and follow-up reads when measuring an end-to-end workflow.

This refresh adds no new speed, token-savings, or adoption measurement.

## Engine microbenchmarks

The Criterion suites under `m1nd-core` measure specific operations on their declared fixtures. A microsecond graph traversal is not a microsecond MCP request, model response, cold start, or full-codebase analysis. Hardware, feature flags, graph size, cache state, and revision affect results.

See the [benchmark methodology](https://github.com/maxkle1nz/m1nd/blob/main/docs/benchmarks/README.md) and the benchmark source before reproducing a number.

## Historical workflow captures

Dated research includes a March 2026 warm-graph corpus that measured token proxies, false starts, guided follow-through, and recovery loops. These are self-authored scenarios from a specific revision and corpus. A token proxy is not measured API billing, and a warm graph excludes cold setup cost. The result cannot establish universal savings or independent superiority to grep.

Historical results remain in their dated records; they are not promises for the current release. Do not describe a historical 0.18-second scenario as “full code analysis end-to-end.”

## Regression tests

Tests establish bounded contracts: filter behavior, persistence, refusal semantics, startup identity, and similar invariants. Passing those tests does not establish retrieval quality on an unseen repository or business ROI. The [test portfolio](https://github.com/maxkle1nz/m1nd/blob/main/docs/TEST-PORTFOLIO.md) documents selection and integration coverage.

The recent Retrobuilder shared-fixture work reduces repeated test preparation. It is not a runtime query-speed optimization or a new product benchmark.

## Owner-run evaluations

Some evaluations deliberately separate public preparation from sealed labels and owner-only execution. A staged harness is `NOT_RUN`; an unmeasured result is `NOT_PROVEN`. This guide does not expose sealed inputs or imply that a ceremony has happened. Follow the public methodology and preserve the required independence.

## What a useful performance report includes

- Exact source revision or published version and enabled features
- OS, hardware, corpus shape, and graph size
- Cold versus warm state, including ingestion and model downloads
- Command, scope, and number of repetitions
- Failures, empty answers, abstentions, and follow-up verification work
- Raw public evidence and the limits of the comparison

Use [GitHub issues](https://github.com/maxkle1nz/m1nd/issues) for a reproducible public report. Remove secrets and private source before sharing.
