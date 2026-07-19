# M1ND-10 candidate freeze — owner authorization — 2026-07-20

## Authorization

In the guardian session of 2026-07-19/20 the owner (Max Kle1nz) explicitly authorized, in
sequence: approval and ratification of the public-path migration plan ("aprovo o plano e
ratifico"), full advance authorization for the night's ladder including the candidate freeze
("autorizo tudo agora"), and finally push and merge ("autorizo push merge tudo"). This receipt
records that authority for the exact ceremony below; it was written before the freeze commit so
the candidate carries its own authorization.

## What this authorizes

1. **Freeze one immutable candidate** from the reviewed working tree: a real commit on local
   branch `candidate/m1nd10-20260720`, authored by the owner identity, containing exactly the
   `git add -A` projection that the hardened candidate-source guard passes with zero violations.
2. **Re-run the gate matrix bound to that candidate digest** (workspace Rust tests, strict
   clippy, fmt, release build, Python suites, npm/UI, actionlint, guard in exact-commit mode,
   frozen-hash checks).
3. **Push** the candidate branch to `origin`, open a **pull request** to `main`, and **merge**
   it once the required 3-OS CI (including the guard security gate and pinned Gitleaks) is green.

## Review binding

The independent re-review `APPROVE`/alta/`REQUIRED_CHANGES: NONE`
(`docs/proofs/m1nd10-candidate-source-boundary-askgod-rereview-20260720.md`) binds the source
state of safeguard snapshot `07698f86`. Every change between that snapshot and the freeze commit
is documentation only: the preserved verdict, the plan wording correction the oracle itself
required, the checkpoint-27 handoff/PATHOS records, and this receipt. No guard, test, workflow,
or policy file changed after the review, so the review binding holds for the frozen tree.

## What this does not authorize

No npm/crates publication, no GitHub release or tag ceremony, no installation over the served
owner, no autonomy activation, no operator-only access, no PRD/UML edit, and no rewrite of
public Git history (the re-review's risk 1 — the retired benchmark files remaining in public
history — stays an open, separate owner decision). `HUMAN_GATED` remains the active mode.
