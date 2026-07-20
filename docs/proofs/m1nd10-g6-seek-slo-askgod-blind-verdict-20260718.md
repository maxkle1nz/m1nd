# M1ND-10 G6 — outcome-blind `seek` p95 ratification

Date: 2026-07-18

## Blind boundary

Before any scorer invocation, the independent askGOD/Fable reviewer received the
R2 contract, benchmark call shape, existing normative latency budgets, and the
proposed `seek p95 <= 500 ms`. The prompt explicitly prohibited reading:

- `docs/benchmarks/m1nd10-g6-held-out-v1/runner-results/`;
- `docs/benchmarks/m1nd10-g6-current-report.json`;
- any scored report or computed current/baseline p95.

The reviewer attested that it honored that boundary. Workspace `git status
--porcelain=v1` was identical before and after the read-only dispatch.

## Exact verdict

`VERDICT: CHANGE`  
`CONFIDENCE: alta`

The reviewer judged `500 ms` architecturally sound and outcome-blind, citing:

- `docs/M1ND-10-PRD.md:232` — both latency SLOs are mandatory;
- `docs/TWO-TIER-BRAIN-PRD.md:402,420,440` — pre-existing warm seek beat has a
  one-second hard ceiling;
- `docs/TWO-TIER-BRAIN-PRD.md:414,439,601` — composed warm north has a two-second
  p95 gate;
- `README.md:301` — the approximately 0.7 ms small-graph observation is expressly
  not a guarantee;
- `m1nd-mcp/tests/retrieval_battery.rs:352-359` — ten seconds is a safety budget,
  not an interactive product SLO;
- `scripts/benchmark/m1nd10_g6_blind_runner.py:401-468` — the measured surface is
  the complete warm localhost MCP HTTP call and includes fresh session overhead.

It required three changes before scoring:

1. reject fabricated/error-fallback latency and abstention evidence;
2. record why 500 ms is selected instead of the one-second hard ceiling;
3. flip the metric spec to fully ratified only after those changes.

It also required the one sealed run to be final for the same revision/digest,
preventing rerun-until-pass behavior.

## Changes satisfying the verdict

- The runner now aborts on a tool error instead of manufacturing a near-zero
  latency and an abstention, and successful rows carry explicit
  `north_executed`/`seek_executed` evidence.
- The scorer requires error-free blinded-run metadata, zero error fallbacks,
  zero actions, blinded/unscored execution, and latencies above the former
  sentinel floor. It rejects explicit unexecuted-call markers.
- `m1nd10-g6-metric-spec-v1.json` ratifies `seek_p95=500`, states that fresh
  session overhead is included, and fixes one sealed run per exact system
  revision/binary digest. A changed candidate may run again only while retaining
  prior failed or unproven evidence.

## Proof boundary

This ratifies the decision boundary only. It does not assert that the measured
current candidate passes, does not prove cold ingest, arbitrary graph scaling,
multi-tenant load, or three-OS performance. Those claims require their own
scored and release-candidate evidence.
