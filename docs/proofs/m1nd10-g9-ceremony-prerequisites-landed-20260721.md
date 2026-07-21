# M1ND-10 — G9 ceremony prerequisites landed — 2026-07-21

> Both declared blocking prerequisites of the owner's Secure Enclave custody ceremony are merged
> into public main. What remains before the ceremony is a small hardening pass and the ceremony
> itself (owner's codesigned binary + Touch ID). No code architecture remains.

## What landed

| PR | Merge | What | Fable seat verdict |
|---|---|---|---|
| #386 | `14d1df67` | FU1 — Secure Enclave `open`-by-label, biometric `sign`, real attribute read-back, never-open-or-create, **key persistence** | review `CHANGE`/alta → fix → re-review `APPROVE`/alta (NONE) |
| #387 | `bb8ac27d` | FU2 — `custody_floor` threaded into gate (G0-G10) + autonomy activation receipts, canon regenerated | review `APPROVE`/alta (NONE), independent digest recomputation |

Both reviews were run through the Fable seat as a `model:fable` subagent (the main session's Fable
is bounced to Opus by the platform safeguard on this security work; the subagent route reaches the
Fable seat). The FU1 re-review verified the persistence fix against the vendored
`security-framework 3.7.0` source (`key.rs:417` — `kSecAttrIsPermanent` is emitted exactly when
`set_location` is called). The FU2 review independently recomputed all 11 regenerated
`receipt_digest`s and confirmed they match, and that the closed-set validation bites in all three
canonical mirrors (Rust/Python/Node).

## The custody-floor merge conflict resolution (honest record)

FU2 branched before FU1 merged; both touched `enclave_authority.rs`. The merge conflicted only on
narrative (the `BLOCKING ORDER` code comment and the proof-doc status prose) — no reviewed logic.
Resolution combined the true post-both-merges state: open/sign/persistence **implemented**
(FU1) and `custody_floor` threading **SATISFIED** (FU2). Verified in the merged tree:
`m1nd-control` 137/137, Python contract 14/14, Node canon verifier pass, frozen PRD/UML hashes
intact (`00658cd8…`, `8a8a5fe9…`).

## Owner interpretive decision (ratified this session)

The G9-A1 ratification says "every G9/G10 receipt carries `custody_floor`." The Independent
Adversarial Review Receipt (IAR) is intentionally left **byte-identical** — the owner ratified
(2026-07-21) that the IAR is a review receipt (records a verdict), not a gate/activation receipt,
so "G9/G10 receipt" covers the gate and activation receipts only. This is declared in the schema
rustdoc and the FU2 proof doc.

## Pre-ceremony hardening items (named by the FU1 re-review) — APPLIED 2026-07-21

All three landed (the FU1 re-review's exact named recommendations; enclave tests 13/13, clippy
`-D warnings` and fmt green on macOS):

1. **Protection class.** The now-persisting Secure Enclave key should use
   `AccessibleWhenUnlockedThisDeviceOnly` (Apple's guidance for SE keys), not the crate default
   `WhenUnlocked`. Real impact ~null (the key is hardware-non-exportable), but it is the correct
   hardening; a one-line change in `provision`'s `create_with_protection`.
2. **Public-key label note.** `to_dictionary` also sets `kSecAttrIsPermanent=true` on the public
   key, so a public-key item may persist under the same label. The custody query is immune (private
   key-class filter); document that any future label-based maintenance must filter by class.
3. **Feature sentinel comment.** If a future refactor drops the `Location::DataProtectionKeychain`
   use but keeps `ignore_legacy_keychains`, the `OSX_10_15` feature would become a silent query
   no-op. Today compile-coupled; add a sentinel comment if that code migrates.

## Future-era note (FU2 review risk 1)

`RATIFIED_CUSTODY_FLOORS` is a singleton today. A successor Path-A era that ADDS (rather than
replaces) a second floor would let the builder accept any set member via core-input (era mixing
within the allowlist; the receipt→era bind is by string, not by ceremony-receipt digest). Inocuous
now; decide at the first Path-A succession.

## What remains before G9 is live

- The three hardening items above (small pass).
- The **owner's ceremony**: a codesigned, `KeychainAccessGroups`-entitled binary running the real
  round-trip — `provision` → process restart → `open`/`sign` under Touch ID. This is the only proof
  the adapter code cannot self-verify; it is the owner's, by hardware and biometry.
- Then the converging chain the ceremony unblocks: G8 signed release → G7 `COHERENT` manifest →
  G6 formal blind → G9 receipts → activation → G10-as-amended.
