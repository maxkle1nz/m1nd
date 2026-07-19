# M1ND-10 G6 corrective askGOD review — 2026-07-19

This is the verbatim read-only Fugu verdict over the current public G6 corrective
source. The reviewer did not run builds or tests and did not inspect operator-only
material. The working-tree status digest before and after the review was
`172ecf44b39e1931a89f3548ce075c2fb4024ade4d24c058250250312c3915d1`.

```text
VERDICT: CHANGE
CONFIDENCE: alta
EVIDENCE:
- AGENTS.md:75-82 — confirms `docs/PATHOS.md`, `docs/ORGANISM-PRD.md`, and `CLAUDE.md` are canonical context for non-trivial repository work.
- CLAUDE.md:8-16 — confirms canonical CI gates exist, but this review stayed read-only and did not rerun them.
- docs/PATHOS.md:88-110 — confirms G6 is source-implemented and locally proven only; independent review and the formal blind run remain `NOT_PROVEN` / not cumulative PASS.
- docs/PATHOS.md:1063-1066 — confirms the continuation order requires a fresh independent verdict on the actual corrective diff before the formal blind benchmark.
- docs/M1ND-10-HANDOFF-20260719.md:423-438 — states the corrective map and specifically claims the scorer independently re-derives formal completeness from readiness, exact binary, authority receipts, repo coherence, cleanup, source recheck, blind boundary, and path topology.
- m1nd-mcp/src/cli.rs:21-26 — confirms `--verify-authorization-receipt` is an exclusive one-shot verifier mode intended not to boot owner/port/runtime state.
- m1nd-mcp/src/main.rs:731-744 — confirms verifier dispatch happens early, before attach/serve/runtime owner machinery.
- m1nd-mcp/src/authorization_receipt_verifier.rs:128-139 — confirms the verifier recomputes the canonical receipt digest and rejects mismatches.
- m1nd-mcp/src/authorization_receipt_verifier.rs:141-154 — confirms half-open clock/lifetime validation.
- m1nd-mcp/src/authorization_receipt_verifier.rs:156-185 — confirms active key lifecycle validation.
- m1nd-mcp/src/authorization_receipt_verifier.rs:187-205 — confirms Ed25519 signature verification gates `Verified`.
- m1nd-mcp/src/authorization_receipt_verifier.rs:213-263 — confirms bounded stdin, closed refusal proof, and exit `0` only for verified receipts.
- scripts/benchmark/m1nd10_g6_blind_runner.py:4334-4560 — confirms runner-side validation independently derives formal completeness from formal preflights, owner topology/readiness, cleanup, governed ingest receipts, source recheck, blind boundary, path topology, and binary binding.
- scripts/benchmark/m1nd10_g6_retrieval.py:763-793 — refutes scorer independence: scorer run metadata validation checks schema/lane/errors/actions/blind markers/declared `score_eligible`/declared `diagnostic_only`/run id/raw verdict counts, not formal proof evidence.
- scripts/benchmark/m1nd10_g6_retrieval.py:885-971 — confirms scorer validates calibration summaries and rows, but still not formal owner topology, cleanup, authority receipt, source recheck, blind-boundary, or path topology evidence.
- scripts/benchmark/m1nd10_g6_retrieval.py:1123-1193 — confirms `evaluate()` builds blockers from spec/corpus/result indexes, baseline receipt, and run ledger; it does not call or reproduce the runner’s formal-proof derivation.
- tests/test_m1nd10_g6_retrieval.py:268-294 — confirms accepted scorer fixture supplies minimal run metadata with `score_eligible: True` and no formal proof fields.
- tests/test_m1nd10_g6_retrieval.py:413-422 — confirms that minimal evidence is expected to `PASS`.
- tests/test_m1nd10_g6_retrieval.py:493-517 — confirms scorer tests reject missing/false eligibility markers, but not forged positive eligibility with absent formal proof.

RATIONALE: I confirmed access to the workspace and essential public files and inspected the real dirty-tree sources without running builds/tests or inspecting operator-only material. The verifier and runner-side correction are substantially hardened on the inspected source, but property 9 is false: the scorer does not independently derive formal completeness. A self-digested result can still pass scorer-side validation by declaring `score_eligible: true` and presenting calibrated-looking measurements, without scorer-validated owner readiness, exact candidate binary binding, authority receipt proof, cleanup/process-group evidence, source recheck equality, blind-boundary proof, or path topology. That leaves the scorer-audit defect open, so the formal blind benchmark precondition cannot be accepted.

REQUIRED_CHANGES:
1. Move, duplicate, or share the formal proof validator into `scripts/benchmark/m1nd10_g6_retrieval.py`, so the scorer itself rejects artifacts unless it independently derives formal completeness from per-owner readiness, exact candidate binary binding, each governed-ingest authority receipt, repo-set coherence, cleanup/session/process-group evidence, source recheck equality, blind-boundary proof, and path topology.
2. Stop trusting declared `score_eligible`, `diagnostic_only`, `formal_preflights.complete`, or equivalent summary flags except as values checked against scorer-derived proof.
3. Add scorer adversarial tests where a self-digested/minimal metadata artifact declares eligibility but lacks formal proof fields; it must return `NOT_PROVEN`.
4. Add scorer adversarial tests for forged cleanup true, foreign readiness/binary, missing authority receipt proof, mismatched post-ingest source recheck, absent blind-boundary proof, and broken path topology; each must fail closed.

RISKS_MISSED: The dossier treats runner-side formal validation as sufficient for the scorer-audit requirement. The missed risk is a proof-thin but internally self-consistent scorer input that bypasses runner-side validation and reaches `PASS` through declared eligibility plus calibrated-looking measurements.
```

This verdict blocks the formal blind run. It is not a release verdict or a G6 PASS.
