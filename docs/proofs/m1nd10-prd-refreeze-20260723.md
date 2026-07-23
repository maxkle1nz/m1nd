# M1ND-10 — PRD re-freeze amendment — 2026-07-23

> The owner edited the frozen PRD to remove machine-local absolute paths (the no-leak rule
> applied to the public canon) and ratified re-freezing it at the new digest. This record is
> the amendment's paper trail; historical proof documents are untouched and keep the digest
> that was ratified at their date.

## What changed

- Commit `531ca750` (author: the owner, direct to main, 2026-07-23): every machine-local
  absolute path in `docs/M1ND-10-PRD.md` replaced with home shorthand (`~/…`). Zero personal
  paths remain in the public canon. No semantic contract content changed.

## Digests

| | SHA-256 |
|---|---|
| Previous frozen PRD | `00658cd88ce9dc5866f9b1fc6b9fbe594923e32fb900bde5bbc7740894c25c38` |
| **Re-frozen PRD (this amendment)** | `bf7b03c7e26ee90fe1bcad9eed4303bb9024b7dab7988251ca33834df26b81f5` |
| UML (unchanged) | `8a8a5fe9b9d2a4fc62c419e160e8dc2dcb4115f58d98f3f15a2d5031881dd32b` |

## Ratification

- The content edit itself was the owner's hand (`531ca750`).
- The re-freeze at the new digest was explicitly confirmed by the owner in the 2026-07-23
  guardian session ("confirmo") after the guardian surfaced the frozen-contract divergence
  (the release tag-guard caught it — the enforcement worked exactly as designed).

## Enforcement points updated (this amendment's commit)

- `.github/workflows/release.yml` (tag-guard `sha256sum --check`)
- `.github/workflows/ci.yml` (frozen-contracts gate)
- `scripts/m1nd10_candidate_source_guard.py` (`FROZEN_PRD_SHA256`, the digest-bound
  `personal_path_content` exception — now moot in effect, since the PRD no longer contains
  personal paths; the mechanism is kept)
- `tests/test_m1nd10_ci_security_contract.py` (the semantic assertion that exists precisely
  to force this conscious amendment step)
- `docs/M1ND-10-HANDOFF-20260719.md` (living frozen-contracts table + amendment note)

## Not touched (history keeps its own digest)

`docs/proofs/*` (ratification, gate, review receipts dated ≤2026-07-22),
`docs/benchmarks/*` result/spec JSONs, `docs/M1ND-10-PUBLIC-PATH-MIGRATION-PLAN-20260719.md`.
