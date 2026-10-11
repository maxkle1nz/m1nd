# Calibration qualification and runtime containment

## Current source behavior

A stored calibration row is a diagnostic record, not sufficient evidence to
permit `act`. Every existing v1 `CalibrationRow` is unqualified for runtime
use, including a structurally valid row with finite metrics, nonzero samples,
and a recent timestamp. Its qualification reason is
`legacy_evidence_unqualified`.

- `predict` retains candidates, scores, and ranking, but its verdicts abstain.
- The seek trust envelope retains its factors and weighted score. With a
  computable score it stays at `reverify`; missing or unusable factor evidence
  can still produce `unprovable`. It cannot reach `act` from a v1 row.
- Row presence does not arm the prediction gate or set a current envelope's
  calibrated state. Current envelopes emit no calibration receipt from these
  unqualified rows.
- `calibrate_predict` and `calibrate_envelope` remain diagnostic handlers.
  When an authorized internal path invokes them, they can compute and persist
  the historical row format; running them does not qualify that row or enable
  `act`. No public calibration repair call can lift this containment.

These limits concern the prediction and seek-envelope calibration paths. They
do not remove retrieval results or replace direct source, compiler, test, and
runtime verification.

## Compatibility boundaries

The v1 persistence schema and structural validation remain unchanged. Valid
historical rows still load and round-trip; invalid rows do not become valid
because qualification is now checked separately. Load does not delete,
rewrite, or silently promote historical data.

Historical seek-receipt digest validation keeps its existing format and digest
semantics. That compatibility lets a historical record be checked; it does not
qualify the evidence for a current decision or cause a current receipt to be
emitted. A valid digest is an integrity check, not a statistical guarantee.

The clock repair remains in place: defect-ledger decay consumes Unix seconds,
while `CalibrationRow::calibrated_at_ms` and historical receipt timestamps use
Unix milliseconds. See [learning and calibration time units](LEARNING-TIME-UNITS.md).

Authority floors are unchanged. The generic public `learn`,
`calibrate_predict`, and `calibrate_envelope` routes remain policy-disabled;
diagnostic source handlers are not new public consumers.

## What this does not establish

The stored `tau`, `target_alpha`, `measured_precision`, `coverage`, and `n`
retain their historical representation. They are not evidence that a live
prediction satisfies the selected error budget. A timestamp, a successful
handler call, a persistence round-trip, or a receipt digest cannot establish
that claim.

The subsequent [learning event separation](LEARNING-EVENT-SEMANTICS.md) corrects
generic `learn`: `correct`/`wrong` is relevance-only, `partial` refuses without
mutation, and feedback no longer creates defect, false-alarm, co-change, or
automatic-antibody evidence. That separation does not qualify calibration.

No newly authorized verified-defect ingress is introduced. The source audit
found generic `learn` was the sole live MCP defect-ledger writer; the direct
core recording methods remain, while other direct call sites were fixtures.
An authorized verified adjudication producer, historical-label audit,
independently held-out evaluation, threshold qualification, and a qualified
calibration format still require separate design and evidence. Historical
records remain readable without being certified as clean. No precision,
coverage, accuracy, or statistical error bound is claimed.
