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
- `SecurityFrameworkEnclaveKeyStore::provision` compiles against the real
  Security.framework crate on macOS (`SecKey::new` with
  `kSecAttrTokenIDSecureEnclave`, EC 256, `kSecAccessControl`).

### NOT_RUN — the owner's live ceremony
- No real Secure Enclave key was created, rotated, activated, or used. No
  biometric signature was produced.
- `SecurityFrameworkEnclaveKeyStore::open` / `sign` fail closed on purpose:
  resolving a persisted enclave key via `SecItemCopyMatching` by application tag
  and signing under biometric presence needs the hardware, biometry, and
  code-signing / keychain-access-group identity the owner alone holds. Shipping
  an unverifiable item-query would be false completeness.
- `custody_floor` is threaded and fail-closed in the **ceremony receipt**. The
  wider threading into the existing `m1nd-control/src/release.rs` gate/review
  receipts and the autonomy activation receipt is a follow-up: those cores are
  digest-sealed, so adding the field perturbs every existing fixture digest and
  must land as its own reviewed change.
- No CLI subcommand wires the four-seat provisioning + assembly yet; the building
  blocks below are the ceremony's exact steps.

## Owner's documented live one-shot proof (you run this, not the agent)

On the owner's machine, with the production code-signing / keychain identity:

1. For each of the four verifier seats, provision an unattended enclave key:
   `provision_agent_enclave_seat(&SecurityFrameworkEnclaveKeyStore::new(prefix,
   subject), &permit_for_seat)` — distinct `key_id` and `failure_domain` per seat
   (at least three distinct domains). Capture each 65-byte SEC1 public key.
2. Provision the owner's biometric seat separately (Touch ID / user presence) —
   this is `owner_signature`, never a voting seat. This step raises the biometric
   UI; it is the owner's alone.
3. Open + re-attest each seat: `SecureEnclaveSigner::open_attested(store, key_id,
   &EnclaveKeyAttestationV1::canonical(access_control))`. Any token/type/size
   drift refuses.
4. Build the `IndependenceSpecV1` (four seats, three failure domains) and the
   `EnclaveCustodyCeremonyReceiptV1` with `custody_floor`, the attestation
   distinction, the four seat public keys, the owner seat key, and the
   independence-spec + constitution digests. `receipt.validate()` and
   `receipt.bind_independence_spec(&spec)` must both pass.
5. Enclave-seal the receipt into the 0700 protected root:
   `SealedProtectedRootV1::open(root, signer, verification_key)?
   .seal_custody_ceremony(&receipt)`. Re-open and `read_custody_ceremony()` to
   confirm the seal verifies.
6. To retire the live proof key, delete it from the Keychain.

Record the seat public keys, the digests, and the sealed receipt path.

## Honest risks
- `SecurityFrameworkEnclaveKeyStore::provision` is compile-verified, not
  run-verified: the exact `kSecAccessControl` flag semantics and the persisted
  key's Keychain visibility are only proven by the owner's live run.
- Single-host limits are the amendment's declared non-claims (no multi-host
  custody, no hardware anti-rollback under physical attack, no root-compromise
  resistance) and are carried on the ceremony receipt itself.
- The `custody_floor` field does not yet reach the release/autonomy receipts;
  until it does, only ceremony receipts are fail-closed on the floor.

## Lineage
- Decision: `docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md`
- Ratification: `docs/proofs/m1nd10-g9-a1-custody-floor-ratification-20260721.md`
- Enclave adapter precedent: `docs/proofs/m1nd10-g2-p256-secure-enclave-adapter-20260718.md`
