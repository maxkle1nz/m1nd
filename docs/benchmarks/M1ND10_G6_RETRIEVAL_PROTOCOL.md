# M1ND-10 G6 Retrieval and Abstention Gate

This gate implements the R2 measurement contract without inventing evidence.
It consumes four immutable artifacts: a ratified metric spec, a blinded and
sealed corpus, exact-revision current results, and paired baseline results.

The held-out corpus must contain at least 200 uniquely identified tasks across
multiple repository-size bands and languages. Every task binds `repo_id` and
`repo_revision`; localizable tasks carry one or more adjudicated anchor IDs and
unlocalizable tasks carry an empty anchor set. Results must cover the corpus
exactly once and bind both a source revision and binary digest.

The scorer reports:

- top-5 anchor recall on localizable tasks;
- abstention recall on unlocalizable tasks;
- wrong-ground `act` authorization rate;
- paired baseline regressions with an exact two-sided sign test;
- `north` and `seek` p95 latency.

Any missing task, duplicate measurement, revision/digest omission, unresolved
SLO, unratified spec, invalid verdict, non-finite latency, or missing baseline
returns `NOT_PROVEN`. A measured threshold miss returns `FAIL`. Only complete
evidence satisfying every check returns `PASS` and `claimable=true`.

Current truth: R2's quality thresholds and the existing 2-second warm composed
`north` SLO are ratified, but no R2-compliant sealed 200-task corpus, paired
baseline run, or ratified `seek` p95 SLO exists in this checkout. Therefore the
checked-in current report is correctly `NOT_PROVEN`.

Example scorer invocation:

```bash
python3 scripts/benchmark/m1nd10_g6_retrieval.py \
  --spec docs/benchmarks/m1nd10-g6-metric-spec-v1.json \
  --cases /operator-only/m1nd10-g6-held-out-v1.json \
  --results /runs/current.json \
  --baseline /runs/baseline.json \
  --output /runs/report.json
```
