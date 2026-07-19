# M1ND-10 G9 — Durable Autonomy Runtime Proof

Date: 2026-07-18  
Scope: `m1nd-control` constitutional autonomy execution substrate  
Status: **focused software-test proof PASS; production-live autonomy NOT PROVEN**

## Scope and authority boundary

This receipt covers:

- `m1nd-control/src/autonomy_runtime.rs` — new durable runtime;
- `m1nd-control/src/autonomy.rs` — the narrow quorum correction from an
  effective 4-of-4 implementation to the frozen PRD's true 3-of-4 rule;
- `m1nd-control/src/lib.rs` — module export only.

It does not cover a production protected-root backend, production signature
keys, G2 `AuthorityRuntime`/`AuthorityWAL` assembly, h4nd, owner/pool wiring, or
live activation. No key was created, imported, provisioned, or activated. No
commit or push was performed.

The frozen product documents were not edited. Their final SHA-256 values are:

- `docs/M1ND-10-PRD.md`: `00658cd88ce9dc5866f9b1fc6b9fbe594923e32fb900bde5bbc7740894c25c38`
- `docs/M1ND-10-UML.md`: `8a8a5fe9b9d2a4fc62c419e160e8dc2dcb4115f58d98f3f15a2d5031881dd32b`

## DONE

- Added an explicit assurance boundary. Production configuration requires
  `ProtectedProduction`; the bundled deterministic backend and verifier are
  named and typed `SoftwareTestOnlyNotProduction` and are refused by a
  production configuration.
- Added injected `ProtectedAutonomyRootBackend` and
  `AutonomyArtifactVerifier` traits. The crate contains no production key,
  signer, HSM/Secure-Enclave adapter, or ordinary-file backend presented as
  protected production storage.
- Added a canonical, hash-chained two-phase autonomy journal. A transition
  publishes `PREPARE + fsync`, advances the protected root to `PREPARED` by
  compare-and-swap, publishes `COMMIT + fsync`, then advances the protected
  root to `COMMITTED`. The protected projection binds generation, state,
  constitution/autonomy epochs, grants, intent root, RED outbox and latch
  watermarks.
- Added fail-closed recovery for exact dangling prepares, protected PREPARED
  forward completion, committed-root equality, corrupt/torn journal data,
  absent or rolled-back protected roots, concurrent writers, symlinks, and
  ambiguous durable-transition poisoning.
- Added a content-addressed `IntentCoreStore` for sovereign and safety intents.
  Canonical bytes are size-bounded, fsynced to a temporary object, atomically
  renamed, directory-fsynced, indexed, and bound into the protected state root.
  Resolution rechecks canonical bytes, content address, embedded digest, and
  index/root bindings.
- Added authoritative `HUMAN_GATED` bootstrap only. Shadow/canary evidence for
  A0 through A5 is durable but cannot change active mode, grants, or epochs.
- Added explicit mode activation. `AutonomyActivationReceiptV1` must bind the
  prior authoritative epoch/mode, prior authority decision, exact candidate,
  exact G9 evidence set, target constitution/epoch, and target grants. It is
  replay-protected and cannot be emitted by its own promoted subject, proposer,
  or executor. There is no timer or evidence path that autoactivates a mode.
- Implemented true 3-of-4 quorum over a frozen membership of exactly four
  seats. Three or four submitted `APPROVE` votes may authorize only when the
  approving votes span at least three failure domains. One seat may be absent.
  Any submitted `DISSENT` or `ABSTAIN` vetoes and escalates. Membership aliases,
  proposer/executor overlap, two approvals, and two-domain approval sets fail
  closed.
- Added per-vote verifier dispatch over canonical unsigned vote material. The
  runtime verifies every submitted vote before accepting the quorum decision,
  then verifies the decision and one-shot capability separately.
- Threaded the kernel-pinned identity/key/binary/policy digest explicitly into
  verifier requests for sentinel verdicts/outbox records, safety capabilities,
  safety-kernel decisions, and RED latch receipts. The verifier no longer has
  to infer the expected pin from a free-form subject label.
- Added durable one-shot autonomy capability consumption and replay refusal.
- Added the RED execution lane: signed monotonic outbox persistence, immediate
  positive-authority freeze, owner-side latch acknowledgement, fsynced safety
  intent, negative-only safety decision/capability validation, terminal outbox
  and latch bindings, protected latch/outbox watermarks, epoch bump, grant
  removal, and durable `HUMAN_GATED + FROZEN` terminal state.
- Added the explicit `FROZEN -> HUMAN_GATED/A0` recovery path required by the
  frozen UML. The recovery intent is the only sovereign intent accepted while
  terminally frozen; it is content-addressed before judgment and must bind the
  frozen state/epoch, retained terminal RED latch, last valid mode, fresh exact
  `GREEN`, remediation and rollback evidence, last-mode authority decision,
  signed recovery receipt, and exact next healthy epoch. Recovery clears grants
  and tier evidence, never activates an autonomous mode, retains incident
  history, rejects replay, and survives restart. A terminally frozen runtime
  also refuses another RED until recovery instead of growing a dead outbox.
- Kept the safety kernel's allow-list negative-only; no positive filesystem,
  graph, source, sovereign, or release effect can enter through the RED lane.

## PROVEN

All Cargo commands used `CARGO_INCREMENTAL=0`. The final post-review battery
and clippy used the isolated external target
`/Volumes/Cofre/.codex-m1nd-build-20260718`; the internal target was not grown
after the volume reached the coordinated stop floor.

### Exact quorum contract and runtime

```text
cargo test --locked -p m1nd-control quorum_ -- --nocapture
```

Result: **3 passed, 0 failed**.

This includes the exact frozen cases:

1. three signed approvals plus one absent seat passes;
2. three approvals plus a submitted dissent fails;
3. three approvals plus a submitted abstention fails;
4. only two approvals fail with `InsufficientQuorum { approvals: 2, required: 3 }`;
5. membership other than four fails the kernel floor;
6. three approvals spanning only two failure domains fail;
7. proposer/executor overlap and vote-binding drift fail;
8. the runtime dispatches all three submitted vote signatures to the injected
   verifier, rejects an invalid vote, persists one successful admission, and
   rejects capability replay.

### G9 runtime battery

```text
CARGO_TARGET_DIR=/Volumes/Cofre/.codex-m1nd-build-20260718 \
cargo test --locked -p m1nd-control autonomy_runtime::tests -- --nocapture
```

Result: **9 passed, 0 failed**.

| Test | Mechanically demonstrated |
|---|---|
| `production_configuration_refuses_software_only_backend_and_verifier` | test fixtures cannot satisfy production assurance |
| `bootstrap_is_durable_human_gated_and_prepared_recovery_forward_completes` | HUMAN_GATED bootstrap, interrupted protected commit, exact restart completion |
| `shadow_canary_a0_through_a5_never_changes_active_mode` | A0-A5 evidence remains non-authoritative; self-promotion refused |
| `exact_prior_authority_activation_is_explicit_and_survives_restart` | explicit human-prior activation to policy mode, exact candidate/evidence, restart persistence |
| `prior_authority_cannot_issue_its_own_target_grant` | self-grant/self-promotion activation refused without state change |
| `agent_quorum_three_of_four_verifies_each_vote_and_is_one_shot` | 3-of-4 over three domains, individual verifier dispatch, invalid signature refusal, one-shot replay fence, role exclusion |
| `concurrent_owner_and_journal_or_protected_root_rollback_fail_closed` | stale writer refusal, damaged journal refusal, missing protected-root anti-rollback |
| `red_outbox_latch_and_negative_transaction_are_durable_and_absolute` | RED freeze, outbox/latch chain, negative transaction, grant removal, epoch bump, retained terminal latch, refusal of another RED while frozen, last-authority recovery with fresh GREEN, HUMAN_GATED/A0 reset, replay refusal, and two restart proofs |
| `kernel_negative_allow_list_cannot_be_used_for_positive_payload_effects` | safety effect set is negative-only |

### Static gates

```text
CARGO_TARGET_DIR=/Volumes/Cofre/.codex-m1nd-build-20260718 \
cargo clippy --locked -p m1nd-control --all-targets -- -D warnings
```

Result: **PASS**.

```text
rustfmt --edition 2021 --check \
  m1nd-control/src/autonomy.rs \
  m1nd-control/src/autonomy_runtime.rs
```

Result: **PASS**.

### Full crate observation

```text
cargo test --locked -p m1nd-control --lib --tests
```

An earlier integrated observation produced **131 passed, 1 failed**. The only
failure was outside G9:

```text
action_catalog::tests::audited_inventory_count_and_ingress_coverage_are_stable
actual audited action count: 168
stale expected count: 167
```

That stale assertion has since been reconciled by its owning concurrent lane to
expect the real count `168`. G9 did not alter `action_catalog.rs`. A new full
crate run after the final recovery edit was **NOT_RUN by G9** because the final
authorized gates were the focused battery and all-target clippy on the external
target; therefore this receipt still does not independently claim a globally
green crate suite.

## Final source hashes

- `m1nd-control/src/autonomy.rs`: `078d7cda75b12f0c68d69bf5feae9574d72f81b149adc52ad0c9b93ae1575438`
- `m1nd-control/src/autonomy_runtime.rs`: `d68643b2ed4bb58eef471cae17b86871faa18f414108eedc174431a20c253100`
- `m1nd-control/src/lib.rs`: `aa22d36eb44d50b71da875506896a34f202cd3487831fefb1514c480d89a02ca`

These hashes describe integrated dirty-working-tree bytes. The `lib.rs` hash
may include concurrent shared-file work and is not an ownership claim by G9.

## Independent final review

The final integrated bytes above were submitted read-only to askGOD/Fable after
the recovery, explicit-pin, and terminal-RED corrections. Its contract result
was:

```text
VERDICT: APPROVE
CONFIDENCE: alta
REQUIRED_CHANGES:
1. NONE
```

The reviewer independently matched the frozen PRD/UML authority mapping,
verified that recovery is a durable last-authority transaction targeting only
the exact next healthy `HUMAN_GATED/A0` epoch, confirmed retained RED-latch
history and replay refusal across restart, checked explicit kernel-pin carriage,
and recomputed all three source hashes plus both frozen-document hashes. The
whole-worktree status hash was unchanged across oracle dispatch:
`c95f5af60219dc7a5fefcec6c8b421420a3aa965aa47f5f9c1998f9c2cff488d`.

Approval applies only to this receipt's software-test boundary. The review's
residual risks are preserved below instead of being treated as closure.

## NOT PROVEN

- No production protected-root backend exists in this lane. Secure Enclave,
  TPM/HSM, rollback-resistant monotonic storage, real protected time, and real
  signature/key lifecycle are **NOT PROVEN**.
- The deterministic test verifier proves dispatch and exact canonical bindings,
  not cryptographic authenticity. No production private key or key ceremony was
  used.
- A production verifier's exact signed subset for self-containing artifacts
  such as `AutonomyRecoveryReceiptV1` and signed target epochs is not specified
  or integrated here; sign-then-embed circularity remains an integration risk.
- The deterministic verifier refuses absent/malformed pins but does not yet
  include a wrong-but-well-formed 64-hex pin mismatch fixture. The runtime
  sources pins from kernel state, but production equality enforcement is not
  mechanically proven by this battery.
- G9 is not yet composed with G2 `AuthorityRuntime`, `AuthorityWAL`, owner
  authorization broker, replay ledger, or execution dispatcher as one recovery
  domain. Cross-runtime atomicity and the PRD's exact RED-versus-positive commit
  race are **NOT PROVEN** here.
- Filesystem `fsync`/rename/CAS ordering is exercised on macOS, but arbitrary
  torn-sector behavior, hostile same-UID directory replacement/TOCTOU,
  power-loss hardware testing, and Windows durability semantics are not proven.
- The RED outbox record is durable locally; real network delivery, remote
  SafetyActuator isolation, retry across processes/machines, and production
  sentinel availability are not proven.
- The four verifier seats and three failure domains are deterministic fixtures,
  not independent external agents/providers. Common-mode model/provider failure
  is not empirically proven.
- `FULL_AUTONOMY` recovery is structurally routed to the prior AgentQuorum and
  the same per-vote verifier path, but the end-to-end recovery battery exercised
  the ratified `POLICY_AUTONOMOUS -> FROZEN -> HUMAN_GATED/A0` case with prior
  human governance authority. A live/full-mode quorum recovery is not claimed.
- The implemented recovery transaction covers terminal RED freezes. A future
  non-RED freeze caused directly by identity, protected-root, or tamper failure
  has no separately defined recovery variant in this lane and therefore remains
  fail-closed rather than recoverable.
- Recovery freshness is evaluated against caller-supplied `now_ms`; protected
  time and an independently enforced recovery authorization window are not
  proven.
- No live A4/A5 promotion, `FULL_AUTONOMY` activation, autonomous constitution
  amendment, or production `AutonomyActivationReceiptV1` was performed. The
  system remains unactivated by this lane.
- Retention/garbage collection of intent objects after all terminal references
  disappear is not implemented by this lane.
- The repository-wide test suite and multi-platform release candidate are not
  claimed by this lane. The earlier action-count failure was reconciled in
  source, but G9 did not rerun the complete crate after its final recovery edit.
- No commit, push, PR, owner mutation, pool mutation, production deployment, or
  external side effect occurred.
