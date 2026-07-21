# M1ND-10 — Amendment G9-A1 ratification — 2026-07-21

## What was ratified

In the guardian session of 2026-07-21 the owner (Max Kle1nz) was presented the custody decision
document (`docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md`) — Path A (full multi-device custody),
Path B (Secure Enclave single-host floor under an explicit amendment), the rejected plain-software
path, the per-gate unlock table, the amendment text with its declared limits, and the explicit
recommendation of Path B — and ratified Path B verbally.

Amendment G9-A1 is therefore ACTIVE: for the first activation era, the production authority
assembly's custody floor is the Secure Enclave single-host floor exactly as worded in the decision
document, with its declared non-claims (no multi-host custody, no hardware anti-rollback under
physical attack, no root-compromise resistance). Every G9/G10 receipt minted under this floor
carries `custody_floor: "secure-enclave-single-host-v1"`. Path A remains the named successor era.

## What this authorizes and what it does not

Authorizes: specifying and implementing the concrete Path-B providers (enclave signer wiring,
protected-root provisioning ceremony, logical quorum seats with the owner's biometric seat) and
taking them through the program's own rite (askGOD verdict before the BIG implementation,
independent review after). Does NOT by itself authorize: activation of any autonomy mode, release
publication, installation over the served owner, or any G-gate promotion — each keeps its own
gate and receipt discipline.

## Lineage

- Frontier finding: `docs/proofs/m1nd10-g7-live-run-and-gate-dependency-20260720.md`
- Enclave adapter precedent: `docs/proofs/m1nd10-g2-p256-secure-enclave-adapter-20260718.md`
- Decision document: `docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md`
