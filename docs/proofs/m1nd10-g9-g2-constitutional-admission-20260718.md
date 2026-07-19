# M1ND-10 G2/G9 Constitutional Admission Proof

Date: 2026-07-18  
Observation point: dirty working tree at `b59a1c2a1454a83164dfb4d5640c6b005154d1ee`  
Status: **component/integration PASS; production activation NOT PROVEN**

## Claim boundary

This receipt covers the served-owner seam that binds G2 authorization to G9
constitutional admission. It does not claim a live owner upgrade, a hardware
key ceremony, physical atomicity across G2 and G9 stores, hosted release
behavior, or `FULL_AUTONOMY` activation.

The frozen product contracts were not edited:

- `docs/M1ND-10-PRD.md` — `00658cd88ce9dc5866f9b1fc6b9fbe594923e32fb900bde5bbc7740894c25c38`
- `docs/M1ND-10-UML.md` — `8a8a5fe9b9d2a4fc62c419e160e8dc2dcb4115f58d98f3f15a2d5031881dd32b`

## Implemented invariants

1. Generic positive dispatch accepts only the human branch. Every non-human
   authority variant is rejected before it can become executable authority.
2. Autonomous positive admission requires `PositiveSovereign`, an autonomy
   capability, exact G2 decision/capability/session/policy bindings, and exact
   G9 evidence/projection bindings for organism, repository, brain, action,
   payload, mission/head, mode, constitution, epochs, grant and protected root.
3. G9 admission runs before G2 positive authorization. A final G9 witness runs
   after authorization and before the transport can sign a receipt or issue a
   lease.
4. Missing evidence, foreign scope, witness unavailability, or post-admission
   state/root drift fails closed and freezes positive issuance plus safety
   globally.
5. G9 consume/project is serialized under one G9 store lock. The code and
   manifest explicitly retain `multi_artifact_atomicity_proven == false`; this
   is not presented as a physically atomic G2/G9 transaction.
6. Production assembly accepts and retains one autonomy owner only when its
   assurance is `ProtectedProduction`. Software-test owners are refused.
7. Bootstrap is exactly `HUMAN_GATED`, issuance-frozen and safety-frozen. Proven
   support is distinct from active authority; autonomous mode requires an
   explicit prior-authority activation receipt.

## Mechanical proof

All Rust commands used the external target
`/Volumes/Cofre/.codex-m1nd-build-20260718` with `CARGO_INCREMENTAL=0`.

```text
cargo test --locked -p m1nd-mcp --lib authority_runtime::tests --features serve
```

Result: **28 passed, 0 failed, 0 ignored**. The battery includes protected G9
synchronization, generic autonomy bypass refusal, missing/foreign evidence,
pre/post-admission drift, global freeze, exact transport issuance, replay and
restart, production positive transport, safety disjointness, prepared recovery,
rollback/corruption refusal, and concurrent replay ownership.

```text
cargo test --locked -p m1nd-mcp --lib autonomy_manifest::tests --features serve
```

Result: **4 passed, 0 failed, 0 ignored**. Bootstrap without activation remains
valid and frozen; proven support and active authority stay distinct; autonomous
mode requires a prior-authority receipt; stale/partial projection is refused.

```text
cargo test --locked -p m1nd-mcp --lib generic_dispatch --features serve
```

Result: **2 passed, 0 failed, 0 ignored**. The ordinary allow-list is exact and
the elevated floor table is exhaustive and fail-closed.

`git diff --check` also passed at this observation point.

## Independent read-only verdict

The final bounded diff was submitted to askGOD/Fugu after the exact G2/G9
bindings, generic bypass refusal, final witness, global freeze and production
owner assembly were in place.

```text
VERDICT: APPROVE
CONFIDENCE: alta
REQUIRED_CHANGES:
1. NONE
```

The reviewer independently identified the following decisive evidence:

- autonomous admission and final witness surround G2 authorization;
- generic non-human positive authority is rejected;
- the G2 and G9 tuples bind the same authority, scope, action and state;
- final witness failure freezes before transport issuance;
- production construction refuses test assurance and retains the same protected
  autonomy owner;
- bootstrap remains `HUMAN_GATED` and the one-lock G9 projection does not claim
  cross-store physical atomicity.

The verdict is a bounded source review, not runtime, hardware, release or
activation evidence.

## Current source identities

- `m1nd-mcp/src/authority_runtime.rs` — `1bc39f5ccd84c2d874178e037088f2485071f19cc57c4c78478e1a2c8034247d`
- `m1nd-mcp/src/autonomy_manifest.rs` — `7f401058dc1ffbc65bcfb1bea4be9f51f82913e432aa54305fa010a6f5751acf`
- `m1nd-mcp/src/authority_transport.rs` — `f3c43f708e76556e5f2b8c30ee9a898e018c48b8b9c0bd824c15e8e7f2af58d0`
- `m1nd-mcp/src/owner_security_config.rs` — `8544888e0413552e60b61addfa05a8b00ac12b0f91e7eb1b3bc09a511d6abf49`

These hashes identify dirty-working-tree source bytes, not a published
candidate.

## Not proven / not run

- Physical atomicity across G2 and G9 stores is **NOT PROVEN**. The safe failure
  mode burns liveness by freezing; it does not claim rollback-free distributed
  commit.
- Secure Enclave/TPM/HSM custody, protected monotonic time/root storage, real
  production signatures and a live owner key ceremony are **NOT INSTALLED**.
- Live Touch ID, external independent verifier domains, real RED delivery,
  physical power-loss recovery and hostile same-UID rename races are
  **NOT RUN / NOT PROVEN**.
- Linux and Windows execution, hosted CI, immutable tag publication, registry
  visibility and installer adoption are **NOT RUN** in this local receipt.
- No activation receipt was created or consumed. The authoritative mode remains
  `HUMAN_GATED`; `FULL_AUTONOMY` is **INACTIVE**.

