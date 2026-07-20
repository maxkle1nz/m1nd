# M1ND-10 candidate delta — askGOD medium review — 2026-07-20

## Binding and outcome

| Field | Value |
|---|---|
| Review mode | askGOD medium, read-only, Fable seat (isolated Explore agent) |
| Scope | delta `70598733` → `272e799f` only (first-CI-collision fixes) |
| Verdict | `CHANGE`, alta |
| Boundary binding | explicitly preserved: the delta touches no guard, guard test, workflow, or proof policy, so the boundary re-review `APPROVE` (rereview-20260720) remains valid |

## What the oracle confirmed

The `.gitignore` single-file whitelist re-includes exactly one named public fixture (content
verified clean; the guard is gitignore-independent by construction); the generalization-score
module-level `SkipTest` is honest (the corpus is absent from CI by the boundary's own design —
the previous state was a lying collection error); the `cfg(unix)` gate replaces a compile error
with an explicit runtime refusal — fail-closed, not concealment.

## The finding that required change

quick-xml 0.38+ reports every `&name;`/`&#N;` inside text as a separate `Event::GeneralRef`
(verified in the 0.41 source: `resolve_char_ref` is the caller's responsibility; predefined
entities included). The three XML adapters captured only `Event::Text`, so referenced characters
were silently dropped from extracted fields — "AT&T" would extract as "ATT", "p &lt; 0.05" as
"p  0.05": silent data corruption invisible to the green suite because no fixture carried
entities inside captured fields.

## Required changes and their closure (same day)

1. `Event::GeneralRef` arms mirroring the exact capture conditions in
   `m1nd-ingest/src/rfc_adapter.rs`, `patent_adapter.rs`, and `jats_adapter.rs`, resolving
   numeric char refs, mapping the five predefined entities, and keeping unknown entities
   verbatim (never a silent drop) via a shared `append_general_ref` helper.
2. One fixture per adapter asserting resolved text ("AT&T", "&lt;", `&#x2264;` → `≤`) inside a
   captured field, so the regression can never return invisibly. Suite: 302 passed / 0 failed /
   6 ignored; clippy `-D warnings` and fmt green.

## Registered residual risks (from the verdict)

The 0.36→0.41 jump also tightens well-formedness and EOL normalization (dirty real-world XML the
old parser accepted may now error — not covered by fixtures); the operator-local skip covers only
`FileNotFoundError` (a corrupted-but-present corpus still hard-errors, which is fail-closed and
correct); RUSTSEC closure is authoritative only via CI's cargo-audit on the exact commit.
