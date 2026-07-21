# M1ND-10 — G9 Path-B Secure Enclave custody floor — implementation

Date: 2026-07-21
Amendment: G9-A1 (ratified; `docs/proofs/m1nd10-g9-a1-custody-floor-ratification-20260721.md`)
Branch: `feat/g9-pathb-enclave-floor`
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
- `:58` `SECURE_ENCLAVE_CUSTODY_FLOOR_V1 = "secure-enclave-single-host-v1"`.
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
  `kSecAttrTokenIDSecureEnclave`, EC 256, `kSecAccessControl` + never-open-or-create
  duplicate guard), `open` (`SecItemCopyMatching` by application tag +
  `SecKeyCopyAttributes` read-back), and `sign` (`SecKeyCreateSignature`,
  ECDSA-message-X962-SHA256) all compile against the real Security.framework crate
  on macOS. Runtime execution against enclave hardware/biometry is the owner's
  ceremony (NOT_RUN).

### NOT_RUN — the owner's live ceremony, with a BLOCKING order
- No real Secure Enclave key was created, rotated, activated, or used. No
  biometric signature was produced.
- `SecurityFrameworkEnclaveKeyStore::open` / `sign` and the provision duplicate
  guard are now fully implemented and compile-verified on macOS (see
  "Prerequisite follow-up landed" below). What stays live-NOT_RUN is their
  execution against real hardware: a persisted key resolved out of the Keychain
  and a biometric signature need the Secure Enclave, biometry, and code-signing /
  keychain-access-group identity the owner alone holds. The agent cannot provoke
  `SecKeyCreateSignature` under user presence — the proof is the owner's ceremony.
- **BLOCKING ORDER (G9-A1 ratification) — STILL UNMET; blocked on frozen canon.**
  `custody_floor` is fail-closed only in the ceremony receipt today. Threading it
  into the `m1nd-control/src/release.rs` gate/review receipts was investigated this
  session (2026-07-21) and found to collide with **ratified, frozen cross-language
  canon** that the ceremony follow-up scope explicitly fences off. The G0–G10 gate
  receipts (including G9/G10) are pinned with fixed `receipt_digest`s inside
  `tests/fixtures/M1ND10-CANONICAL-VECTORS.json`, and the gate-receipt schema is
  dual-implemented in Python (`scripts/m1nd10_release_contract.py`,
  `GATE_CORE_FIELDS` — a closed 16-field allowlist enforced by `_exact_keys`, with
  a `digest_canonical` that must match Rust byte-for-byte). Adding a `custody_floor`
  field to `GateReceiptCoreV1` therefore requires, in one owner-authorized change:
  (a) the Rust struct + fail-closed validation, (b) the Python allowlist +
  validator, and (c) regenerating the frozen G9/G10 canonical vectors (new digests).
  Both the checked-in `test_m1nd10_release_contract.py` and the release CI gate
  (`.github/workflows/release.yml` `verify-canonical-vectors`) verify these vectors.
  Per the ceremony follow-up's own boundary ("if a vector is frozen/ratified — the
  PRD/UML hash or `M1ND10-CANONICAL-VECTORS.json` — stop and report; do not edit
  frozen canon without an order"), this threading was **not forced**. It must land
  as its own reviewed, owner-authorized change and **must merge BEFORE** the custody
  ceremony and before any G9/G10 receipt is minted under this floor — so no receipt
  can claim the floor without carrying it. This ordering is a ratification
  obligation, not an optimization; it remains a genuine blocker.
- **Prerequisite follow-up landed.** Open-by-tag (`SecItemCopyMatching` via the
  high-level `ItemSearchOptions`), the real `SecKeyCopyAttributes` read-back of
  token/type/size (proving Secure Enclave residency and EC P-256 by `CFEqual`
  against the framework's own constants — attesting the KEY, not the request),
  `sign` through `SecKeyCreateSignature` under biometric presence, and the
  never-open-or-create duplicate guard are implemented and compile-verified on
  macOS. `provision` now attests the created key by the same read-back rather than
  hard-coding token/type. The `SecurityFrameworkEnclaveKeyStore::new` constructor
  gained a seat-class `access_control` argument (a store is bound to one seat
  class; it refuses a permit for another). On non-macOS the whole module is absent
  by construction, so the production assembly stays NOT_INSTALLED / fail-closed.

## Owner's documented live one-shot proof (you run this, not the agent)

### Runs today (agent building blocks; no biometry, no persisted-key resolution)
1. For each of the four verifier seats, provision an unattended enclave key:
   `provision_agent_enclave_seat(&SecurityFrameworkEnclaveKeyStore::new(prefix,
   subject, EnclaveAccessControlV1::PrivateKeyUsageNonExportable),
   &permit_for_seat)` — distinct `key_id`/`failure_domain`, at least three
   distinct domains, each permit carrying its `bound_context_digest` (sealed later
   as seat lineage). Capture each 65-byte SEC1 public key. Provision now reads the
   created key's real token/type/size back, and refuses a tag already present.
2. **kSecAccessControl conformance check.** Confirm each provisioned key actually
   carries the intended access-control semantics (private-key usage; user
   presence for the biometric seat). The flag values are hand-rolled (`1<<30`,
   `1<<0`), so this run is what proves them.

### Prerequisite follow-up, then the ceremony (the owner's alone)
3. The open/sign follow-up (open-by-tag + attribute read-back + biometric sign +
   duplicate guard) has landed. The `custody_floor` threading (blocking order
   above) must also land before the ceremony.
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
- Sealed-slot anti-replay is filesystem-strength plus a root-path + context
  binding sealed into each record (a slot cannot be replayed into another root
  sealed by the same key); it is NOT hardware anti-rollback. Single-host limits
  are the amendment's declared non-claims, carried on the ceremony receipt.
- The `custody_floor` field does not yet reach the release/autonomy receipts;
  only ceremony receipts are fail-closed on the floor. This threading is BLOCKED
  on frozen canon (see the BLOCKING ORDER above): the gate receipts are ratified
  cross-language vectors + a Python-mirrored closed schema, so it needs an
  owner-authorized change to regenerate them. The autonomy activation receipt
  (`m1nd-control/src/autonomy.rs`, Rust-only, regenerable) is not frozen, but
  threading it alone would neither satisfy the "every G9/G10 receipt" order nor
  leave a coherent schema (half the receipts migrated), so it was not done in
  isolation; it belongs in the same owner-authorized change as the gate receipts.
- **Quorum wiring follow-up.** The ceremony receipt seals the four seat public
  keys and binds them to the `IndependenceSpecV1` by (principal, key,
  failure-domain). `VerifierSeatV1` carries no public key, so the binding does
  not yet force equality between the sealed seat key and the verification-key
  registry entry the quorum verifier resolves. The future quorum wiring must
  cross-check sealed-pubkey == registered-pubkey.

## Lineage
- Decision: `docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md`
- Ratification: `docs/proofs/m1nd10-g9-a1-custody-floor-ratification-20260721.md`
- Enclave adapter precedent: `docs/proofs/m1nd10-g2-p256-secure-enclave-adapter-20260718.md`
