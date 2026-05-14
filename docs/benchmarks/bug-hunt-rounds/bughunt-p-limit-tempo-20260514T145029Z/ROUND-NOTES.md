# Bug Hunt Round Notes: bughunt-p-limit-tempo-20260514T145029Z

Status: internal product learning, not public benchmark copy.

## Result

- `m1nd-temponizer-full`: 6/10 seeded bugs found (60.0%); per-lane counts `[4, 2]`.
- `m1nd-temponizer`: 9/10 seeded bugs found (90.0%); per-lane counts `[5, 4]`.
- `m1nd-trained`: 9/10 seeded bugs found (90.0%); per-lane counts `[4, 5]`.
- `direct`: 7/10 seeded bugs found (70.0%); per-lane counts `[3, 4]`.

## Interpretation

This round separates three conditions:

- `m1nd-temponizer-full`: m1nd trained loop plus the explicit Temponizer formula
  (`Tc = alpha(phi) * Tp`) and per-phase recalibration notes.
- `m1nd-temponizer`: m1nd trained loop plus temporal calibration and `Te` notes.
- `m1nd-trained`: m1nd trained loop without explicit Tempo/TEMPONIZER framing.
- `direct`: no m1nd tools or helper scripts.

The primary signal is that the compact m1nd modes outperformed direct controls
on seeded recall in this `p-limit` fixture. `m1nd-trained` and the lighter
`m1nd-temponizer` prompt both reached 90.0%, while direct controls reached
70.0%.

The surprise is that `m1nd-temponizer-full` underperformed at 60.0%. Read this
honestly: this is not evidence that temporal recalibration is bad. It is
evidence that the full-spec prompt, as written, was too heavy for a bug-hunt
lane. One lane avoided live m1nd because the current helper path could write
sidecar state outside its allowed artifacts; the other lane used m1nd but became
too conservative and omitted lower-confidence defects it had partially noticed.

Product lesson: Tempo belongs inside m1nd as compact operating physics, not as a
large reporting burden. Agents need the formula, the real constraints, and a few
`Tc/Te` recalibration points around meaningful decisions. They do not need to
turn every step into paperwork.

## Seeded Bugs

- `options-object-default-rejects-on-clear`
- `reject-on-clear-falsy-non-boolean-accepted`
- `map-non-array-iterable-index-lost`
- `limit-function-drops-arguments`
- `infinite-concurrency-rejected`

## Product Observations

- `m1nd` lanes again needed a cold-graph ingest path before full trust.
- `m1nd` helper/probe flows can still materialize graph sidecar files in lane
  workspaces; this should be moved to a controlled state dir for cleaner
  benchmarks.
- `m1nd-temponizer` lanes produced better `Te` evidence and clearer decisions
  around focused probes versus full test suites.
- `m1nd-temponizer-full` produced richer telemetry but lower recall, which
  points to prompt-weight and state-placement friction, not to a stable product
  conclusion.
- Direct controls were strong but missed more cross-surface contract issues,
  especially iterable index or Infinity support.
- The scorer now records timestamp-derived lane timing when event streams carry
  parseable `created_at`/`timestamp` fields. Treat those timing numbers as
  rough internal telemetry until event capture is stricter.

## Caveats

- This is one internal round on one fixture repo.
- Extra findings were preserved but not independently judged.
- This report measures seeded recall, not total bug discovery quality.
- The `p-limit` runtime dependency was stubbed locally after npm registry
  timeout so lanes could run focused behavior probes without network dependence.
- The lane count is small: two lanes per condition.

## Next Product Actions

- Add first-good-finding time and total wall-clock duration to the scorer.
- Use `probe_m1nd.py --no-worktree-artifacts` in benchmark lanes so probe state
  does not appear in the target worktree.
- Use the new `m1nd-temponizer-compact` harness mode next: full formula,
  compact logging, and no incentive to stop searching prematurely.
- Repeat this exact four-arm setup on `click-python-cli`.
- Measure Tempo separately on efficiency metrics, not only seeded recall.
