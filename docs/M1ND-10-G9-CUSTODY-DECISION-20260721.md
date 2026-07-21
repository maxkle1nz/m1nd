# M1ND-10 — G9 custody decision — 2026-07-21

> Status: decision document for the owner. Nothing here executes anything. It exists because the
> first real G7 run proved that G6-formal, G7-complete, and G8-signing all converge on one open
> question — what custody backs the production authority assembly — and that question is the
> owner's alone (`docs/proofs/m1nd10-g7-live-run-and-gate-dependency-20260720.md`).

## 1. Why this is the frontier

`assemble_production_owner_authority_v1` fail-closes without injected hardware-protected
config/epoch providers, a protected broker/WAL journal head, and a production signer. The G6
formal runner refuses without a pinned production authority assembly
(`m1nd10_g6_blind_runner.py:2654`). The isolated G7 owner honestly reports manifest `DRIFT` while
release provenance and authority are absent. Software test assurance is **rejected by the code
itself** (`ProtectedProduction` only). Until custody exists, the top of the ladder cannot close —
not for lack of code, but for lack of the foundation only the owner can pick.

## 2. What the ratified contract literally requires

Hardware-protected signers and epochs, a protected journal head, quorum evidence, sentinel and
safety actuators, shadow/canary on a release candidate, and prior-authority activation
(handoff §4.10, §G9; `OwnerSecurityConfigV1` carries public anchors only and pins through a
separate protected root). The PRD does not name a specific device class — it names properties.

## 3. The paths

**Path A — full multi-device production custody.** Dedicated custody (HSM or multi-machine
quorum), physical sentinel/actuators, attestation across hosts. Closes G9 exactly as the maximal
reading of the contract imagines. Cost: hardware acquisition + integration + new adapters +
multi-host ceremonies; weeks, not days. Unlocks `FULL_AUTONOMY` and G10 with zero amendments.

**Path B — Secure Enclave single-machine floor, ratified by amendment.** The decisive fact: the
era already built and proved the macOS Secure Enclave P-256 adapter
(`docs/proofs/m1nd10-g2-p256-secure-enclave-adapter-20260718.md`: explicit provisioning,
re-attestation of token/type/size, sign-only-through-`SecKeyCreateSignature`; 15 control-plane
tests green). Path B installs the production assembly on THIS machine with:

- production signing keys in the Secure Enclave (real consumer hardware custody — a key that
  cannot be exported, only used);
- the protected config/epoch roots on `0700` no-follow paths pinned by device/inode (already the
  runtime's law), with the journal head sealed by an enclave-signed record;
- quorum realized as N logical seats with distinct enclave keys plus the owner's Touch ID as the
  human seat (quorum-of-keys, single physical host — the honest limitation);
- sentinel as the existing launchd/watchdog machinery, declared for what it is.

What it is NOT: multi-host physical custody, hardware anti-rollback across power loss, or
protection against an attacker with root on this Mac. The amendment states these limits.

**Path C — plain software keys.** Rejected by the code by design; not offered.

## 4. What each path unlocks, per gate

| Gate | Path A | Path B |
|---|---|---|
| G6 formal blind (220 tasks) | ✓ | ✓ (assembly pinned, labels stay operator-held) |
| G7 complete (`COHERENT` manifest) | ✓ | ✓ (after G8 mints release provenance) |
| G8 signing ceremony | ✓ | ✓ (enclave-backed release authority) |
| G9 cumulative | ✓ full | ✓ with declared single-host limits |
| `FULL_AUTONOMY` activation | ✓ | ✓ within the amended scope |
| G10 | ✓ 10/10 literal | ✓ 10/10 **as amended** — the amendment is part of the record |

## 5. The amendment Path B requires (ready to ratify)

> **Amendment G9-A1 (2026-07-21).** For the first activation era, the production authority
> assembly's custody floor is defined as: Secure Enclave P-256 signing keys on the owner's
> machine (non-exportable, attested at open), protected `0700` no-follow config/epoch roots
> pinned by device identity, an enclave-sealed journal head, logical quorum seats with distinct
> enclave keys plus the owner's biometric seat, and the declared sentinel. This floor explicitly
> does NOT claim multi-host custody, hardware anti-rollback under physical attack, or root-level
> compromise resistance; those remain the target of a future Path-A era. All G9/G10 receipts
> minted under this floor carry `custody_floor: "secure-enclave-single-host-v1"`.
>
> Ratified by: ______ (owner), date ______.

## 6. Recommendation

**Path B now, Path A as the declared production-scale era later.** B is honest (the limits are
in the amendment, on every receipt), uses real hardware custody that already has a proven
adapter, unblocks the entire converging branch (G6-formal → G7-complete → G8 → G9 → activation
→ G10-as-amended) on this machine in days, and loses nothing: A remains the named successor.
The program's own constitution prefers an explicit amendment over a silent stretch — this is
exactly that mechanism used as designed.

## 7. Mechanical next steps once the owner picks

- **B:** implement the concrete providers (enclave signer wiring from the dormant h4nd adapter
  into `assemble_production_owner_authority_v1`, protected-root provisioning ceremony, quorum
  seat minting), then G8 dry ceremony → G6 formal → G7 complete → G9 receipts.
- **A:** acquisition list + adapter contracts first; everything in B remains reusable as the
  single-host seat of the quorum.
