# M1ND-10 public-path migration — owner ratification — 2026-07-20

## What was ratified

In the guardian working session of 2026-07-19/20 the owner (Max Kle1nz) reviewed
`docs/M1ND-10-PUBLIC-PATH-MIGRATION-PLAN-20260719.md` — produced from the hardened
candidate-source guard's content-gate inventory (worktree projection: 1413 paths, 265
`personal_path_content` violations, zero other classes) — and declared approval directly in the
session: the plan is approved and the canon exception is ratified.

That declaration covers exactly the two decisions the plan reserved for the owner:

1. **The migration plan is approved**, including retiring the 246 historical benchmark files
   under `docs/benchmarks/` from the future public candidate (evidence is retired, never
   rewritten), scrubbing the 7 `m1nd-mcp` Rust source/test files, 6 operational documents, and
   2 executable fixtures to documented neutral placeholders, and redacting the 3 dated G2/G2-G3
   proof documents with explicit post-hoc notes.
2. **The C6 frozen-canon exception is ratified**: the candidate-source guard may carry one
   narrowly defined content-gate exception for `docs/M1ND-10-PRD.md`, bound to that file's exact
   ratified SHA-256 (`00658cd88ce9dc5866f9b1fc6b9fbe594923e32fb900bde5bbc7740894c25c38`). The
   exception dies if the file changes by one byte. The PRD itself is not edited; its frozen hash
   remains an invariant. This is a digest-pinned single-document exception, not a path allowlist.

## What this does not authorize

No commit, push, tag, publication, installation, activation, key rotation, served-owner contact,
PRD/UML edit, or gate weakening is authorized by this receipt. The migration execution must still
pass the checkpoint-26 focused gates, a security pass, and a fresh independent read-only review
before the candidate-source boundary can return to `LOCAL_PROVEN` and any candidate freeze can be
considered.

## Bindings

- Plan: `docs/M1ND-10-PUBLIC-PATH-MIGRATION-PLAN-20260719.md`
- Hardened guard under review: `scripts/m1nd10_candidate_source_guard.py` plus its test suites
  (21 focused tests green at ratification time; frozen PRD/UML hashes verified intact).
- Review lineage: checkpoint-26 verdict
  `docs/proofs/m1nd10-candidate-source-boundary-askgod-review-20260719.md` (`CHANGE`, high),
  which this migration and the subsequent re-review answer.
