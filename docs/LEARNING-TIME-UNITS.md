# Learning and calibration time units

The envelope calibration path consumes defect history recorded by `learn`.
`TrustEntry` defect timestamps and `TrustLedger::compute_trust` use Unix seconds;
`CalibrationRow::calibrated_at_ms` and seek calibration receipts use Unix
milliseconds.

The calibration core accepts one integer millisecond timestamp, converts it to
fractional seconds for trust decay, and preserves the original integer in the
row. A fresh defect therefore keeps its recent-defect weighting; the default
30-day half-life is measured in days, not accidentally in milliseconds treated
as seconds. The existing synthetic corpus also records seconds, matching the
real producer.

## Regression contract

- A fixed corpus with three defects and one false alarm per node produces
  trust-band reliability 0.2 immediately, 0.5 after 30 days, and 0.8 after
  120 days. At 30 days its recency factor is 0.5 and trust score is 0.5125.
- An actual `learn` handler event followed by envelope calibration retains the
  fresh-defect band.
- Calibration retains exact integer epoch milliseconds, including subsecond
  digits, in the row; persistence and reload retain that same row.

These are clock and persistence regressions, not quality benchmarks.

## Compatibility and limits

The persisted ledger and calibration formats are unchanged. Existing rows are
not rewritten on load. A subsequent authorized calibration recomputes from the
recorded ledger using the corrected clock; this change does not add a public
calibration or learning consumer or relax an authority floor.

The feedback vocabulary, label interpretation, threshold algorithm, and `act`
gating are unchanged. Correct clock units do not establish that relevance
feedback is a verified defect, that the corpus is independently held out, or
that a calibration threshold satisfies a statistical error budget. Those are
separate pending quality repairs; continue checking code and executed evidence.
