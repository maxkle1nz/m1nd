# M1ND-10 — G9 Path-B Secure Enclave custody floor — implementation

Date: 2026-07-21
Amendment: G9-A1 (ratified; `docs/proofs/m1nd10-g9-a1-custody-floor-ratification-20260721.md`)
Branch: `feat/g9-pathb-enclave-floor` (blocking-order follow-up merged separately on
`feat/g9-custody-floor-threading`; see "Follow-up MERGED" below)
Gate status: **implementation slice; the owner's live custody ceremony is NOT_RUN**

## Outcome

The Path-B custody floor from the ratified amendment G9-A1 is implemented as a
proof-grown set of providers that satisfy the production owner-authority seam,
plus the honest, offline-verifiable P-256 plumbing the floor needs. Nothing here
provisions a real Secure Enclave key or activates any autonomy mode — those are
the owner's ceremony. The design injected into
`assemble_production_owner_authority_v1` (`m1nd-mcp/src/owner_security_config.rs`)
was NOT refactored; this work only adds providers that satisfy it.

## What was implemented (file:line)

### 1. Offline receipt verifier admits the P-256 algorithm
- `m1nd-mcp/src/authorization_receipt_verifier.rs:124` — receipt binding gate now
  accepts the **closed set** `{ED25519, ECDSA_P256_SHA256_X962}`.
- `:204` — signature acceptance names both `CryptographicIntegrity` variants
  explicitly (no `Ok(_)`); a future variant fails the build.
- The exact `receipt.algorithm == verification_key.algorithm` binding is preserved.

### 2. Honest P-256 assurance labels (no false attestation)
- `m1nd-mcp/src/authority_runtime.rs:1595` — new `ControlVerifiedEcdsaP256Sha256X962`
  variant on `AuthorityVerificationAssurance` (existing wire names untouched).
- `:1602` `control_assurance_for_integrity` derives the receipt/session assurance
  from the verified key's algorithm; carried on `PositiveAuthorityProofV1`.
- `m1nd-mcp/src/authority_transport.rs:86` — the serialized session-assurance
  mirror gains the same SCREAMING_SNAKE variant, and its `From` maps it.

### 3. Production signing seam (m1nd-mcp never links p256)
- `m1nd-control/src/crypto_authority.rs:1072` — `sign_authority_message` signs an
  exact framed message through an injected `AuthoritySigner` and normalizes the
  raw (possibly high-S) output to canonical low-S DER against the pinned key.
  `sign_canonical_authority_payload` now delegates to it.

### 4. Enclave module (`m1nd-mcp/src/enclave_authority.rs`, cfg macOS)
- `:58` `SECURE_ENCLAVE_CUSTODY_FLOOR_V1` — value `"secure-enclave-single-host-v1"`;
  since the threading follow-up this is re-exported from `m1nd-control` (single
  source of truth) rather than defined here.
- `:156` `SecureEnclaveKeyStoreV1` — the mockable enclave boundary; `:178`
  `provision_agent_enclave_seat` refuses the biometric human seat.
- `:196` `SecureEnclaveSigner` (impl `AuthoritySigner`) — opens + re-attests
  token/type/size before it can sign.
- `:272` `EnclaveBackedWalRecordCrypto` — signs via the control seam, verifies
  only through m1nd-control; `ProductionCryptographic`.
- `:412` `SealedProtectedRootV1` — 0700 device/inode-pinned root; enclave-sealed
  slots verified on read.
- `:612` / `:665` / `:719` — the three CAS backends
  (`ProtectedEpochBackend`, `ProtectedOwnerSecurityConfigRootBackendV1`,
  `ProtectedJournalHeadBackendV1`), all `HardwareProtectedAttested`.
- `:839` `EnclaveCustodyCeremonyReceiptV1` — carries `custody_floor`, the
  key-custody-vs-anti-rollback attestation distinction, the four distinct enclave
  verifier seats + failure domains, the owner biometric seat, and the sealed
  independence-spec/constitution digests; validated fail-closed and bound to the
  `IndependenceSpecV1` before any quorum vote.
- `:1088` `SecurityFrameworkEnclaveKeyStore` — the real adapter (see boundary).

## Mechanical proof

16 new tests, all green on macOS (`cargo test -p m1nd-mcp --lib`), plus the
crates' existing batteries:

```text
authorization_receipt_verifier: 11 passed (3 new: P-256 verify, P-256 tamper,
  algorithm outside the closed set)
authority_runtime::tests + authority_transport::tests: 32 passed
  (2 new: integrity->assurance mapping, P-256 session wire mirror)
enclave_authority::tests: 11 passed (round-trip sign/verify through control,
  re-attestation drift refusal, provisioning-never-open-or-create, agent-never-
  provisions-biometric-seat, signer/key identity mismatch, sealed epoch CAS +
  advance/skip refusal, tampered-payload seal failure, non-0700 root refusal,
  journal-head anti-rollback invariants, ceremony envelope validation, ceremony
  seal/read-back + independence-spec binding)
```

Gates green per crate touched: `cargo fmt --check`, `cargo clippy --all-targets
-D warnings`, `cargo test` (m1nd-control and m1nd-mcp).

## Exact boundary

### Proven
- P-256 receipts verify end to end offline; the closed algorithm set and the
  exhaustive integrity match are fail-closed.
- A P-256 decision is labeled P-256, never Ed25519.
- The enclave WAL crypto round-trips: the enclave's non-deterministic/high-S DER
  is normalized to canonical low-S by m1nd-control and re-verified there.
- Re-attestation refuses token/type/size drift; provisioning is never
  open-or-create; the agent never provisions the biometric human seat.
- The sealed 0700 device/inode-pinned CAS roots enforce their anti-rollback
  invariants and refuse a tampered payload and a non-0700 directory.
- The custody ceremony envelope is fail-closed on the floor, four distinct
  enclave seats, three failure domains, and an owner seat disjoint from the
  voting seats; it binds to the exact `IndependenceSpecV1`.
- `SecurityFrameworkEnclaveKeyStore` — `provision` (`SecKey::new` with
  `kSecAttrTokenIDSecureEnclave`, EC 256, `kSecAccessControl`, **persisted** via
  `Location::DataProtectionKeychain` + never-open-or-create duplicate guard), `open`
  (`SecItemCopyMatching` by `kSecAttrLabel`, scoped to the data-protection keychain
  so it sees what `provision` wrote, + `SecKeyCopyAttributes` read-back with a
  `CFGetTypeID` guard on the size), and `sign` (`SecKeyCreateSignature`,
  ECDSA-message-X962-SHA256) all compile against the real Security.framework crate
  on macOS. Runtime execution against enclave hardware/biometry — and the
  code-signing entitlement persistence requires — is the owner's ceremony (NOT_RUN).

### NOT_RUN — the owner's live ceremony, with a BLOCKING order
- No real Secure Enclave key was created, rotated, activated, or used. No
  biometric signature was produced.
- `SecurityFrameworkEnclaveKeyStore::open` / `sign`, key persistence, and the
  provision duplicate guard are now fully implemented and compile-verified on macOS
  (see "Prerequisite follow-up landed" below). What stays live-NOT_RUN is the real
  persistence-and-resolution proof: making a Secure Enclave key permanent in the
  data-protection keychain and resolving it back **requires the calling binary to be
  codesigned with a `KeychainAccessGroups` entitlement** — a HARD prerequisite the
  owner alone holds, not a runtime nicety. Only then does `provision` -> process
  restart -> `open`/`sign` (biometric) actually round-trip. The agent cannot provoke
  `SecKeyCreateSignature` under user presence — the proof is the owner's ceremony.
- **BLOCKING ORDER (G9-A1 ratification) — SATISFIED 2026-07-21.** `custody_floor`
  was fail-closed only in the ceremony receipt when this slice landed. Threading it
  into the digest-sealed `m1nd-control/src/release.rs` gate receipt and the autonomy
  activation receipt was the follow-up (the field perturbs every frozen fixture
  digest, so it landed as its own reviewed, owner-authorized change on
  `feat/g9-custody-floor-threading`, regenerating the frozen G9/G10 canonical
  vectors — 11 gate cores + the activation receipt). The ordering obligation is met:
  the threading merges before the owner's custody ceremony, so no receipt can claim
  the floor without carrying it. (Gate/review distinction, owner-ratified 2026-07-21:
  the independent adversarial review receipt is not gated on a candidate era's
  custody floor and is intentionally left byte-identical — only the gate and autonomy
  receipts carry the field.)
- **Prerequisite follow-up landed.** Implemented and compile-verified on macOS:
  - **Key persistence (the review's central fix).** `provision` now sets
    `Location::DataProtectionKeychain`; without a location the created key was
    EPHEMERAL (`kSecAttrIsPermanent` is only emitted when a location is set), so it
    never reached the Keychain and `open`/`sign` could never resolve it. Secure
    Enclave keys can only be made permanent in the data-protection keychain, and
    `resolve_persisted_key` queries the SAME scope (`ignore_legacy_keychains` /
    `kSecUseDataProtectionKeychain`) so creation and lookup agree. This needs the
    `OSX_10_15` feature on `security-framework` (an empty cfg-only feature — no new
    crate, no lockfile move) and a codesigned, entitled binary at runtime.
  - **Open-by-label** (`SecItemCopyMatching` via `ItemSearchOptions`) — custody is
    keyed by `kSecAttrLabel`, not `kSecAttrApplicationTag` (the high-level
    key-creation surface exposes no application tag); any item sharing the label
    makes provision AND open fail closed.
  - The real `SecKeyCopyAttributes` read-back of token/type/size (Secure Enclave
    residency + EC P-256 proven by `CFEqual` against the framework's constants,
    with a `CFGetTypeID` guard before the size is read as a `CFNumber`).
  - `sign` through `SecKeyCreateSignature`, the never-open-or-create duplicate
    guard, and `provision` attesting the created key by the same read-back.
  - The `SecurityFrameworkEnclaveKeyStore::new` constructor gained a seat-class
    `access_control` argument (a store is bound to one seat class; it refuses a
    permit for another). On non-macOS the whole module is absent by construction, so
    the production assembly stays NOT_INSTALLED / fail-closed.

## Owner's documented live one-shot proof (you run this, not the agent)

### On the owner's codesigned, entitled binary (no biometry for the verifier seats)
Note: because provisioning now PERSISTS into the data-protection keychain, even the
unattended verifier seats require the codesigned binary with a `KeychainAccessGroups`
entitlement — there is no ephemeral-key shortcut anymore. The agent's mock proves the
logical contract; these steps run on the owner's signed binary.
1. For each of the four verifier seats, provision an unattended enclave key:
   `provision_agent_enclave_seat(&SecurityFrameworkEnclaveKeyStore::new(prefix,
   subject, EnclaveAccessControlV1::PrivateKeyUsageNonExportable),
   &permit_for_seat)` — distinct `key_id`/`failure_domain`, at least three
   distinct domains, each permit carrying its `bound_context_digest` (sealed later
   as seat lineage). Capture each 65-byte SEC1 public key. Provision persists the
   key, reads its real token/type/size back, and refuses a label already present.
2. **kSecAccessControl conformance check.** Confirm each provisioned key actually
   carries the intended access-control semantics (private-key usage; user
   presence for the biometric seat). The flag values are hand-rolled (`1<<30`,
   `1<<0`), so this run is what proves them.

### Prerequisite follow-up, then the ceremony (the owner's alone)
3. The open/sign follow-up (persistence + open-by-label + attribute read-back +
   biometric sign + duplicate guard) has landed, and the `custody_floor` threading
   (blocking order above) has landed too — both ceremony prerequisites are met.
4. Provision the owner's biometric seat (Touch ID / user presence) —
   `owner_signature`, never a voting seat.
5. Open + re-attest each seat: `SecureEnclaveSigner::open_attested(store, key_id,
   &EnclaveKeyAttestationV1::canonical(access_control))`; token/type/size drift
   refuses.
6. Build the `IndependenceSpecV1` (four seats, three failure domains) and the
   `EnclaveCustodyCeremonyReceiptV1` (custody_floor, attestation distinction, four
   seat public keys + lineage, owner seat key, spec/constitution digests).
   `validate()` and `bind_independence_spec(&spec)` must both pass.
7. Enclave-seal into the 0700 root:
   `SealedProtectedRootV1::open(root, context_digest, signer, verification_key)?
   .seal_custody_ceremony(&receipt)`; re-open and `read_custody_ceremony()` to
   confirm.
8. Retire the live proof key from the Keychain.

Record the seat public keys, the digests, and the sealed receipt path.

## Honest risks and named follow-ups
- `provision`/`open` now read the key's real token/type/size back via
  `SecKeyCopyAttributes` and prove Secure Enclave residency + EC P-256 by
  `CFEqual` against the framework's own constants. What is NOT read back is the
  `kSecAccessControl` (presence) semantics — the hand-rolled flag values
  (`1<<30`, `1<<0`) and the persisted key's Keychain visibility are proven only by
  the owner's live conformance run. This is compile-verified on macOS, not run
  against hardware in CI.
- Persistence has a HARD runtime prerequisite: the data-protection keychain that
  Secure Enclave keys must live in requires the calling binary to be codesigned
  with a `KeychainAccessGroups` entitlement. An unsigned/unentitled binary cannot
  persist or resolve the key, so `provision`/`open`/`sign` fail closed. The
  provision→restart→open round-trip is therefore proven only on the owner's signed
  binary; the mock proves the logical contract in-process.
- Sealed-slot anti-replay is filesystem-strength plus a root-path + context
  binding sealed into each record (a slot cannot be replayed into another root
  sealed by the same key); it is NOT hardware anti-rollback. Single-host limits
  are the amendment's declared non-claims, carried on the ceremony receipt.
- The `custody_floor` field now reaches the gate receipt and the autonomy
  activation receipt (see "Follow-up MERGED" below); the independent review
  receipt stays byte-identical by design. Every receipt that names the floor is
  fail-closed on the closed ratified set.
- **Quorum wiring follow-up.** The ceremony receipt seals the four seat public
  keys and binds them to the `IndependenceSpecV1` by (principal, key,
  failure-domain). `VerifierSeatV1` carries no public key, so the binding does
  not yet force equality between the sealed seat key and the verification-key
  registry entry the quorum verifier resolves. The future quorum wiring must
  cross-check sealed-pubkey == registered-pubkey.

## Follow-up MERGED (2026-07-21): custody_floor threaded into the receipts

The blocking-order follow-up above is **SATISFIED** on branch
`feat/g9-custody-floor-threading`. `custody_floor` now rides the digest-sealed
receipts, not just the ceremony receipt:

- `m1nd-control::release::GateReceiptCoreV1` and
  `m1nd-control::autonomy::AutonomyActivationReceiptCoreV1` carry `custody_floor`,
  validated fail-closed against the closed `RATIFIED_CUSTODY_FLOORS` set
  (`{secure-enclave-single-host-v1}` today) — the exact-set precedent from
  `authorization_receipt_verifier`. The value is drawn from the ratified constant
  / ceremony receipt, never request payload. It is era-scoped: a successor Path-A
  era will carry a different value.
- The closed set is validated in all three canonical mirrors the CI runs: Rust
  (`require_ratified_custody_floor`), Python
  (`m1nd10_release_contract.validate_gate_core`), and Node
  (`validateCanonicalGateReceipt`). A smuggled `"software"` floor is refused by a
  negative test in each. `SECURE_ENCLAVE_CUSTODY_FLOOR_V1` has a single source of
  truth in `m1nd-control`; `enclave_authority` re-exports it.

**Schema disposition (owner-ratified 2026-07-21).** The field joins
`m1nd-gate-receipt-v1` (and `m1nd-autonomy-activation-receipt-v1`) **without a
version bump** — the schema stays v1 while its field set grows. Receipts minted
before this floor existed are historical/void; the pipeline never re-consumes
them (an activation candidate's gates are re-minted under the floor). The frozen
canon `tests/fixtures/M1ND10-CANONICAL-VECTORS.json` was therefore **regenerated,
not migrated**, via the canonical builder: the diff is restricted to
11×`+custody_floor`, 11× recomputed `receipt_digest`, 11× `receipt_id`; the
candidate block (`candidate_digest 52544a51…`), the independent review receipt,
the canonical/refusal cases, and the operational manifests stay byte-identical.
The three canonical verifiers pass and the fixture re-derivation test is green.

## Lineage
- Decision: `docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md`
- Ratification: `docs/proofs/m1nd10-g9-a1-custody-floor-ratification-20260721.md`
- Threading follow-up: branch `feat/g9-custody-floor-threading` (this update)
- Enclave adapter precedent: `docs/proofs/m1nd10-g2-p256-secure-enclave-adapter-20260718.md`
