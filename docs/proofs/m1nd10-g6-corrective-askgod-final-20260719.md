# M1ND-10 G6 corrective askGOD final re-review — 2026-07-19

This is the verbatim verdict contract returned by a read-only Fugu re-review over
the public G6 corrective source. The reviewer ran with an isolated temporary
`CODEX_HOME` whose MCP inventory was empty, did not contact the installed owner,
did not run builds/tests, and did not inspect operator-only material. The
working-tree status digest before and after the review was
`172ecf44b39e1931a89f3548ce075c2fb4024ade4d24c058250250312c3915d1`.

```text
VERDICT: APPROVE
CONFIDENCE: alta
EVIDENCE:
- docs/proofs/m1nd10-g6-corrective-askgod-review-20260719.md:34-38 — the recorded CHANGE verdict's four REQUIRED_CHANGES: (1) scorer must self-derive formal completeness, (2) stop trusting eligibility/summary flags except as cross-checked values, (3) add a proof-thin declared-eligibility test, (4) add six fail-closed forgery tests; this re-review verifies only their closure.
- scripts/benchmark/m1nd10_g6_retrieval.py:15-26 — scorer imports are stdlib-only; it does not import m1nd10_g6_blind_runner or its validator (RC1, Q1).
- scripts/benchmark/m1nd10_g6_retrieval.py:1027-1037 — `_validate_formal_run_proof` re-derives eligibility from raw rows; its docstring states runner declarations are comparison values only and it never accepts a summary flag as the source of completeness (RC1, Q1).
- scripts/benchmark/m1nd10_g6_retrieval.py:1043-1045 — absent/open `formal_preflights` returns `False` with a blocker, so a proof-thin artifact cannot self-derive completeness (Q6).
- scripts/benchmark/m1nd10_g6_retrieval.py:1146 — each owner readiness proof must carry `binary_digest == expected_binary_digest` (Q2).
- scripts/benchmark/m1nd10_g6_retrieval.py:2012 — `expected_binary_digest = _sha256_path(args.current_binary)`, i.e. the hash of the real candidate binary file, not a trusted metadata string (Q2).
- scripts/benchmark/m1nd10_g6_retrieval.py:1167-1168,1238-1239 — source_revision and file_set_digest are bound to the sealed corpus manifest in both owner topology and governed ingest (Q3).
- scripts/benchmark/m1nd10_g6_retrieval.py:1183 — the topology's nested cleanup must equal the standalone cleanup record for that repo (lifecycle binding, Q3).
- scripts/benchmark/m1nd10_g6_retrieval.py:1251-1264 — topology/cleanup/ingest repo sets must equal the corpus repo set with matching cardinality, and governed mutation count must equal the repo count (Q3).
- scripts/benchmark/m1nd10_g6_retrieval.py:1113-1121 — cleanup requires same-session lifetime, session-delete, process-group termination, and completion flags (Q4).
- scripts/benchmark/m1nd10_g6_retrieval.py:1215-1244 — every governed-ingest authority receipt is re-derived (Ed25519/core/assembly/signature/clock/lifecycle/epoch/key_id/algorithm) and bound to owner/session/revision/file-set/lease/reconciliation (Q4).
- scripts/benchmark/m1nd10_g6_retrieval.py:985-1024,1270-1277 — `_validate_source_verification` recomputes expected file/byte/line/root sets, and post-ingest source must byte-equal the sealed pre-ingest proof (Q4).
- scripts/benchmark/m1nd10_g6_retrieval.py:911-958 — `_validate_path_topology` independently derives disjointness, absoluteness, symlink-freedom, and canonical-POSIX roots (Q4).
- scripts/benchmark/m1nd10_g6_retrieval.py:1053-1062 — blind-boundary proof must be coherent with top-level blind metadata (Q4).
- scripts/benchmark/m1nd10_g6_retrieval.py:1279-1297 — `derived_complete` conjoins blind proof, path proof, source live/post, exact repo set, topology bindings, cleanup, and authority receipts — all required (Q4).
- scripts/benchmark/m1nd10_g6_retrieval.py:1301-1358 — declared `formal_preflights.complete`/`status`, the seven per-stage summaries, `score_eligible`, `diagnostic_only`, and `proof_state` are each flagged when they differ from scorer-derived proof (RC2, Q1).
- scripts/benchmark/m1nd10_g6_retrieval.py:1417-1449 — `_index_results` calls the derivation, adds every proof error as a blocker, and adds a further blocker when `formal_complete` is false (double-gated enforcement).
- scripts/benchmark/m1nd10_g6_retrieval.py:1856-1862,473-481 — any blocker forces `_not_proven`, i.e. status `NOT_PROVEN`, claimable false (Q6).
- scripts/benchmark/m1nd10_g6_blind_runner.py:99-239 — runner closed-field contracts; a text-level set comparison shows the scorer's shared field sets are byte-identical and the scorer adds two stricter sets, so its re-derivation is independent and at least as strict.
- scripts/benchmark/m1nd10_g6_blind_runner.py:4334-4560 — runner-side `_validate_run_proof_metadata` mirrors the same derivation the scorer now performs independently, confirming the scorer duplicated (not delegated to) the validator.
- tests/test_m1nd10_g6_retrieval.py:697-718 — proof-thin, resealed (self-digested) artifact declaring `score_eligible:true` with no `formal_preflights` returns NOT_PROVEN (RC3, Q6 — no bypass).
- tests/test_m1nd10_g6_retrieval.py:720-789 — six fail-closed tests: forged cleanup, foreign readiness binary, missing receipt signature, post-ingest source mismatch, absent blind boundary, broken/overlapping path topology (RC4, Q5).
- tests/test_m1nd10_g6_retrieval.py:229-430,592-600 — `formal_run_metadata` builds a fully-proven fixture and `test_complete_v2_evidence_can_pass` proves non-vacuity: a correct artifact still PASSes, so the gate is not trivially closed.
RATIONALE: All four REQUIRED_CHANGES from the recorded CHANGE verdict are correctly closed on the public source. The scorer now owns a self-contained `_validate_formal_run_proof` that imports no runner code and re-derives `derived_complete` from per-owner readiness (bound to the SHA-256 of the real candidate binary), every governed-ingest authority receipt, the corpus-exact repository set with source-revision/file-set and lifecycle bindings, cleanup/session/process-group evidence, pre/post source equality, the blind-boundary proof, and path topology; it treats `score_eligible`, `diagnostic_only`, `proof_state`, `formal_preflights.complete`, and each per-stage summary only as values cross-checked against that derived proof, adding a blocker on any divergence. Enforcement is double-gated in `_index_results` and any blocker yields NOT_PROVEN. The seven new adversarial tests (proof-thin declared eligibility plus six forged/foreign/missing/mismatch/absent/broken cases) each fail closed while the fully-proven fixture still PASSes. A self-digested proof-thin artifact cannot reach PASS by declarations alone: absent proof rows drive the derived conjunction false and the resealed test confirms NOT_PROVEN, so acceptance question 6 has no bypass.
REQUIRED_CHANGES:
1. NONE
RISKS_MISSED: The scorer trusts the receipt's boolean crypto flags (e.g. `signature_verified:true`) as proof rows rather than re-running Ed25519 verification itself; that cryptographic check lives in the separate offline Rust verifier (authorization_receipt_verifier.rs), so scorer-level trust in those booleans is a by-design layering assumption. Correctness is proven only against the hand-built `formal_run_metadata` fixture and the reported 85/85 Python test pass — no formal blind run, live owner, or operator-label access occurred, and I did not personally execute tests. The scorer and runner duplicate their closed field sets rather than sharing one module, so contract drift must be kept in sync manually (a mismatched set fails closed). None of these is an open REQUIRED_CHANGE; per the stated scope this verdict only closes the corrective re-review and readiness to attempt a later formal blind run on an immutable candidate — it does not make G6 PASS or authorize that run now.
```

This APPROVE closes only the corrective re-review. The formal 220-task blind run,
live owner proof, release, publication, installation, and activation remain
`NOT_RUN` / `NOT_PROVEN`.
