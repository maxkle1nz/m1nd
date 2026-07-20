# M1ND-10 public-path migration plan — 2026-07-19

Status: governed migration plan awaiting owner review. This document authorizes nothing by
itself. It is the required-change-4 deliverable of the candidate-source remediation
(`docs/M1ND-10-HANDOFF-20260719.md` §9) and precedes any scrub, retirement, canon exception,
or re-review.

## 1. Input and measurement

The hardened candidate-source guard now scans public blob content in both inspection modes and
refuses personal absolute paths generically (macOS `/Users/<name>/`, Linux `/home/<name>/`,
Windows `C:\Users\<name>\`; documented placeholders such as `<name>` and `<repo-root>` do not
match by construction). Measured on 2026-07-19:

| Projection | Paths | `personal_path_content` | Other reasons |
|---|---|---|---|
| worktree (`git add -A` model) | 1413 | **265** | 0 |
| `HEAD` exact commit (`b59a1c2`) | 1344 | 262 | 66 `generated_cache`, 14 `operator_private_artifact` (already deleted in the worktree) |

The worktree projection is the future-candidate surface; its 265 files are the migration scope.
The earlier username-only census (509 occurrences / 143 files) is a subset: the generic gate
also catches other personal-home usernames in examples and transcripts, which is intended.

## 2. Classes and actions

| Class | Files | Action |
|---|---|---|
| C1 — historical benchmark rounds under `docs/benchmarks/` | 246 | **Retire** (remove from the future candidate tree) |
| C2 — Rust source/tests in `m1nd-mcp/src/` | 7 | **Scrub** surgically; re-run crate tests |
| C3 — operational docs (`docs/AGENT-PACKS.md`, `docs/IDE-INTEGRATIONS.md`, `docs/deployment.md`, `docs/voice/M1ND-VOICE-DESIGN.md`, `docs/voice/CARDPERSIST-DIVERGENCES.md`, `.github/wiki/Getting-Started.md`) | 6 | **Scrub** to placeholders |
| C4 — dated proof documents (`docs/proofs/m1nd10-g2-askgod-preflight-20260718.md`, `docs/proofs/m1nd10-g2-g3-authority-bridge-20260718.md`, `docs/proofs/m1nd10-g2-g3-authority-bridge-askgod-preflight-20260718.md`) | 3 | **Redact** with an explicit post-hoc note |
| C5 — executable fixtures (`scripts/benchmark/bug_hunt_round.py`, `npm/test/cli.test.js`) | 2 | **Scrub**; re-run their suites |
| C6 — frozen canon (`docs/M1ND-10-PRD.md`, three occurrences) | 1 | **Digest-bound exception**, owner-ratified |

Class rationale and rules:

- **C1 retire, never rewrite.** These are pre-M1ND-10 evidence transcripts (bug-hunt and
  real-world rounds). Rewriting historical evidence in place would silently invalidate it;
  retiring it from the public candidate preserves honesty. Correction (2026-07-20, from the
  independent re-review): these files are already published in the public Git history of
  origin/main — retirement protects future candidates only and does not unpublish anything;
  rewriting public history is a separate owner decision outside this plan. This extends the
  retirement already begun in this worktree
  (legacy `operator-only` answer keys are already deleted). Their `operator-only` siblings are
  never opened during retirement.
- **C2 scrub with semantics preserved.** The occurrences live in strings/examples inside
  `audit_handlers.rs`, `cockpit.rs`, `internal_tests/hall_brains_listing.rs`, `main.rs`,
  `mission_letter.rs`, `presence.rs`, `session.rs`. Replace with neutral placeholders or
  fixture-relative paths; `cargo test -p m1nd-mcp` (touched lanes) plus workspace clippy/fmt
  must stay green.
- **C3/C5 scrub to documented placeholders** (`<name>`, `<repo-root>`, `<user>`), keeping each
  document's meaning; C5 additionally re-runs its own test suite. `npm/test/cli.test.js` is
  currently also being edited by a concurrent session — coordinate before touching.
- **C4 redact like the checkpoint-26 precedent**: the public Fugu verdict already replaced a
  machine-local path with `<repo-root>` and recorded that the redaction changes no finding. The
  three G2/G2-G3 proofs get the same treatment: value replaced, an explicit redaction note
  appended, no result altered.
- **C6 is the only canon touch and it is not an edit.** The frozen PRD hash is a ratified
  invariant cited by every receipt; editing it in place is forbidden. Proposed mechanism: a
  narrowly defined exception in the guard bound to the PRD's exact SHA-256
  (`00658cd88ce9dc5866f9b1fc6b9fbe594923e32fb900bde5bbc7740894c25c38`). If the file changes by
  one byte, the exception dies and the gate refuses again. This is not a path allowlist: it is
  one ratified, digest-pinned document. Alternative (owner's call): a formally amended PRD with
  a new ratified hash — heavier, touches every receipt that cites the current hash, and is not
  recommended for a documentary-only concern.
- **No blanket allowlist anywhere**, per the review verdict and handoff §9.

## 3. Execution order

1. C1 retirement (mechanical `git rm` of the 246 historical files from the future tree).
2. C2 + C3 + C5 scrubs, each followed by its focused suite (crate tests, doc no-leak, npm test).
3. C4 redactions with notes.
4. C6 exception implemented in the guard **only after explicit owner ratification**, with an
   adversarial test proving the exception dies on any PRD byte change.
5. Re-run the checkpoint-26 focused gates: guard worktree projection (target: zero
   `personal_path_content`, or exactly the ratified C6 state), guard unit tests, candidate-only
   Gitleaks, Ruff, actionlint, `git diff --check`, frozen hashes, affected aggregate lanes.
6. Security pass and fresh independent read-only review of the complete corrected diff
   (handoff §9 step 8). Only `APPROVE` restores `LOCAL_PROVEN` for the boundary.

## 4. Verification commands

```bash
python3 scripts/m1nd10_candidate_source_guard.py --repo . --worktree-projection
python3 -m unittest tests.test_m1nd10_candidate_source_guard tests.test_m1nd10_ci_security_contract
shasum -a 256 docs/M1ND-10-PRD.md docs/M1ND-10-UML.md
```

## 5. What requires the owner

1. Approve this plan (in particular: retiring the 246 historical benchmark files from the
   public candidate).
2. Ratify the C6 digest-bound PRD exception, or order the amendment path instead.
3. Nothing here authorizes commit, push, publication, installation, activation, or contact
   with the served owner; those authorities remain separate.
