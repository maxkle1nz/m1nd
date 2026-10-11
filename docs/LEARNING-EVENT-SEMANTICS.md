# Learning event semantics

## Scope and authority

This is the internal source contract for generic learning feedback and its
existing debrief caller. Public `learn`, `debrief`, `calibrate_predict`, and
`calibrate_envelope` routes remain blocked at their existing authority floors.
This correction installs no new public consumer and does not authorize a bypass.

## Relevance feedback

- `correct` says that the supplied nodes were relevant. Positive strength
  strengthens existing graph edges in the resolved node set and its direct
  `contains` children.
- `wrong` says that the supplied nodes were irrelevant. Positive strength
  weakens existing edges in that same set. It does not declare a false alarm
  about a previously verified defect.
- `partial` is refused before mutation with
  `partial_feedback_requires_explicit_attribution`. A single unordered node list does not
  identify which results were relevant or irrelevant; splitting the list by
  position invents attribution. Explicitly attributed mixed feedback needs a
  separate contract.
- Strength must be finite and nonnegative. Zero is a no-op, including learning
  telemetry and invalidation. Unknown feedback and invalid strength are refused
  before mutation. Existing edge-weight bounds still apply.

`edges_modified` counts successfully changed arcs, not attempted writes to
already-saturated weights. Tremor telemetry sums realized post-clamp forward and
reverse edge deltas; saturated arcs emit no observation. The sum uses a wide
accumulator and is bounded only at the finite f32 storage limit, so even the
largest accepted finite strength cannot corrupt persisted telemetry. Nonzero
requests retain existing graph invalidation behavior; only zero strength is a
complete learning no-op.

Successful responses declare `feedback_semantics: "retrieval_relevance"`,
`trust_records_updated: 0`, `co_change_observations_recorded: 0`, and
`antibody_added: false`. A zero-strength response adds `status: "no_op"` with
all learning event counts zero.

These events do not append defect, false-alarm, or partial entries to
`TrustLedger`, record observed co-change or node appearances in its temporal
matrix, or automatically extract/store antibodies. Weight-change telemetry,
where emitted for actual relevance adjustment, is not evidence of a source-code
change or a confirmed defect. Supplying a defect-like query string or multiple
node IDs does not change this boundary.

## Debrief

The existing internal debrief path still classifies touched paths and captures
findings, map-miss lessons, and outcome rows under their existing evidence rules.
A contacted dependency can receive relevance-only `correct` feedback. An
unpredicted path cannot be taught with `partial`: the ambiguous teaching is
explicitly refused, rather than silently interpreted as a defect or co-change.
That refusal does not discard the debrief's separate finding/outcome capture.
The debrief outcome `partial` remains valid; it is distinct from learning
feedback `partial`. Each `taught[]` entry retains its feedback and success flag
and exposes relevance semantics plus a refusal reason when teaching fails.
Untouched dependencies are not negative examples. Path conformance and a
successful outcome remain distinct from verified code quality.

## Existing evidence and compatibility

Historical trust, temporal, antibody, and calibration stores are not deleted,
rewritten, or reclassified by this change. Direct core recording methods and
independently observed-change producers retain their existing contracts;
`TrustLedger::record_defect` remains the direct defect producer used by the
[clock regression](LEARNING-TIME-UNITS.md). The source audit found generic
`learn` was the sole live MCP defect-ledger writer before this separation;
other direct recording call sites were fixtures. No newly authorized defect
ingress or verified adjudication producer is introduced. This correction does
not certify old entries as clean or retroactively identify which arose from
relevance feedback.

All v1 calibration rows remain `legacy_evidence_unqualified` under the
[qualification containment](CALIBRATION-QUALIFICATION.md). Prediction abstains,
and seek stays at `reverify` or `unprovable` without a current calibration receipt.
Changing feedback semantics or running diagnostic calibration does not enable
`act`, establish corpus independence, or prove statistical quality.

## Regression contract

Focused checks must cover positive/negative relevance adjustment without
ledger/co-change/antibody writes; partial and invalid-input refusal before any
mutation; a zero-strength no-op; and debrief's explicit partial refusal with
finding/outcome capture retained. Historical-store round trips and direct
producer tests remain separate compatibility evidence. These checks establish
an event boundary, not a benchmark result, release, or hosted integration pass.
