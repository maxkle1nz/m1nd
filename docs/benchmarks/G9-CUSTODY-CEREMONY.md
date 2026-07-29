# M1ND-10 — G9 Secure Enclave custody ceremony — the owner's runbook

> **Status: STAGED, NOT_RUN.** Every provider this ceremony needs is implemented, tested and
> merged on `main`. What does not exist is the *wiring* — no production code path constructs any
> of it, and there is no CLI surface to drive it. This document measures that gap exactly and
> writes down the ceremony as it WILL be, so the owner can run it once the wiring lands.
>
> Amendment: `docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md` §5 (G9-A1, Path B, ratified
> 2026-07-21). Implementation proof: `docs/proofs/m1nd10-g9-pathb-enclave-floor-implementation-20260721.md`.
> Prerequisite record: `docs/proofs/m1nd10-g9-ceremony-prerequisites-landed-20260721.md`.

## 0. Agent-side simulation is PROHIBITED

**No agent may perform, simulate, stub, mock, fake, or "dry-run" any step of this ceremony, and no
agent may provision any enclave key, touch biometrics, or synthesize any artifact this document
describes as an output.** The ceremony's entire evidentiary value is that a human owner, present at
a specific machine, proved possession of hardware that no software path can stand in for. An agent
that produces a `custody-ceremony.sealed.json`, a seat public key, or a "ceremony ran" claim by any
means other than the owner executing these steps has destroyed the artifact it was imitating.

The code already encodes half of this rule: `provision_agent_enclave_seat`
(`m1nd-mcp/src/enclave_authority.rs:187`) refuses the biometric human seat fail-closed with
`EnclaveError::HumanSeatProvisioningRefused` (`:192`, defined `:1107`). That refusal is a floor, not
a ceiling — the prohibition above covers the whole ceremony, including the four unattended verifier
seats, which an agent must also not provision.

Permitted agent work: writing the wiring code in §4, and running the software-mock unit tests that
already prove the logical contract in-process (13 tests, `enclave_authority::tests`).

## 1. Prerequisites

| # | Prerequisite | State today | Where |
|---|---|---|---|
| P1 | Apple Silicon / T2 Mac with a Secure Enclave, owner physically present | owner's machine | — |
| P2 | Touch ID enrolled for the owner | owner's machine | — |
| P3 | macOS build target — the module is `#[cfg(target_os = "macos")]` and absent by construction elsewhere, so the assembly stays NOT_INSTALLED and fails closed rather than falling back to software | **satisfied** | `m1nd-mcp/src/lib.rs:36-37` |
| P4 | Binary codesigned with a **`KeychainAccessGroups` entitlement**. Secure Enclave keys can only be made permanent in the data-protection keychain; an unsigned or unentitled binary cannot persist *or* resolve the key, so `provision`/`open`/`sign` all fail closed | **MISSING — blocking** (see §5 R1) | `m1nd-mcp/src/enclave_authority.rs:1149-1157`, `:1425` |
| P5 | A `0700`, non-symlink, device/inode-pinned protected root directory for the sealed slots | code ready, root not provisioned | `SealedProtectedRootV1::open` — `m1nd-mcp/src/enclave_authority.rs:441` |
| P6 | Custody dependency pins intact — `security-framework =3.7.0` (feature `OSX_10_15`), `security-framework-sys =2.17.0`, `core-foundation =0.10.1`. **These are custody surface; never bump them opportunistically** | **satisfied** | `m1nd-mcp/Cargo.toml:112-114` |
| P7 | Crypto stack coherent after the RustCrypto sweep (#464) | **satisfied, measured** (see §6) | — |
| P8 | A CLI or callable ceremony surface to drive the steps below | **MISSING — blocking** (see §5 R2) | — |

## 2. The step list as it WILL be

Each step names what already exists (with `file:line`) and what is missing. All paths are relative
to the repo root. `enclave_authority.rs` means `m1nd-mcp/src/enclave_authority.rs`.

### Phase A — provision the four unattended verifier seats (owner, on the entitled binary)

1. **Construct the production key store, one per seat class.**
   `SecurityFrameworkEnclaveKeyStore::new(label_prefix, subject_id, EnclaveAccessControlV1::PrivateKeyUsageNonExportable)`.
   - EXISTS: `enclave_authority.rs:1183` (struct), `:1190` (ctor), `:1389` (`SecureEnclaveKeyStoreV1` impl).
   - MISSING: any caller. Zero references outside `enclave_authority.rs` itself.

2. **Provision each of the four seats** with a distinct `key_id` and `failure_domain`, spanning at
   least three distinct domains, each permit carrying its `bound_context_digest` (sealed later as
   seat lineage). Provision is never open-or-create: an existing item under the same
   `kSecAttrLabel` fails closed, and the created key's real token/type/size are read back via
   `SecKeyCopyAttributes` and attested against the framework's own constants.
   - EXISTS: `provision_agent_enclave_seat` `enclave_authority.rs:187`;
     `EnclaveProvisioningPermitV1` `:136`; `EnclaveKeyAttestationV1::canonical` `:101`;
     duplicate guard via `resolve_persisted_key` `:1247`.
   - Counts are law: `IMMUTABLE_VERIFIER_SEATS = 4` (`m1nd-control/src/autonomy.rs:54`),
     `IMMUTABLE_FAILURE_DOMAINS = 3` (`:56`).
   - MISSING: the driver that loops the four permits and captures the public keys.

3. **`kSecAccessControl` conformance check.** Confirm each provisioned key really carries the
   intended access-control semantics. The flag values are hand-rolled (`1 << 30` private-key usage,
   `1 << 0` user presence) and the protection class is pinned to
   `AccessibleWhenUnlockedThisDeviceOnly`. **This run is the only thing that proves them** —
   `SecKeyCopyAttributes` does not read the access-control back.
   - EXISTS: `access_control_flags` `enclave_authority.rs:1207-1233`.
   - MISSING: the check itself. It is an owner-observed conformance step, not code.

### Phase B — the owner's biometric seat (owner only, irreducibly)

4. **Provision the owner's biometric seat** with
   `EnclaveAccessControlV1::UserPresenceBiometricNonExportable` (`enclave_authority.rs:78`).
   This is the `owner_signature` authority that remains present even under `AgentQuorum`. It is
   **never** a voting quorum seat, and the ceremony receipt refuses a receipt where the owner key
   also appears among the voting seats (`:980`).
   - EXISTS: the store supports the class; `provision_agent_enclave_seat` **refuses** it by design
     (`:191-193`) — so this step needs a *separate, owner-only* entry point.
   - MISSING: that owner-only entry point. This is the one surface that must never be reachable by
     an agent path.

### Phase C — open, attest, seal

5. **Open and re-attest every seat before it signs anything.**
   `SecureEnclaveSigner::open_attested(store, key_id, &EnclaveKeyAttestationV1::canonical(access_control))` —
   token/type/size drift refuses fail-closed.
   - EXISTS: `enclave_authority.rs:214`; drift refusal proven by
     `reattestation_refuses_token_type_or_size_drift`.
   - MISSING: caller.

6. **Build the `IndependenceSpecV1`** (four seats, three failure domains) and the
   **`EnclaveCustodyCeremonyReceiptV1`**: `custody_floor`, the attestation distinction, the four
   seat public keys + lineage digests, the owner seat key, and the spec/constitution digests. Both
   `validate()` and `bind_independence_spec(&spec)` must pass — the seats are sealed BEFORE any
   quorum vote is counted.
   - EXISTS: receipt `enclave_authority.rs:907`, `validate` `:925`, `bind_independence_spec` `:993`,
     `CustodyAttestationDistinctionV1::secure_enclave_single_host` `:876`;
     `IndependenceSpecV1` `m1nd-control/src/autonomy.rs:356`.
   - MISSING: caller.

7. **Enclave-seal into the protected root**, then re-open and read back to confirm.
   `SealedProtectedRootV1::open(root, context_digest, signer, verification_key)?.seal_custody_ceremony(&receipt)`,
   then `read_custody_ceremony()`.
   - EXISTS: `open` `enclave_authority.rs:441`, `seal_custody_ceremony` `:1035`,
     `read_custody_ceremony` `:1051`.
   - MISSING: caller.

8. **Retire the live proof key** from the Keychain and record the seat public keys, the digests and
   the sealed receipt path.
   - MISSING entirely: this is an owner operational step with no code behind it.

## 3. The receipts this ceremony emits

| Receipt | Schema / constant | Where it lands | State |
|---|---|---|---|
| `EnclaveCustodyCeremonyReceiptV1` | `m1nd-enclave-custody-ceremony-receipt-v1` (`enclave_authority.rs:860`) | enclave-sealed slot `<protected-root>/custody-ceremony.sealed.json` (`:862`), seal domain `m1nd-enclave-custody-ceremony-v1` (`:861`) | never minted |
| Seat public keys ×4 + owner seat key | 65-byte uncompressed SEC1 P-256, lowercase hex (`:896`, validator `:1069`) | recorded by the owner; sealed inside the ceremony receipt | never minted |
| `GateReceiptCoreV1.custody_floor` | value `secure-enclave-single-host-v1` (`m1nd-control/src/release.rs:25`), closed set `RATIFIED_CUSTODY_FLOORS` (`:33`) | every G0–G10 gate receipt | field **wired and validated**; no real receipt minted under a real ceremony |
| `AutonomyActivationReceiptCoreV1.custody_floor` | same closed set, validated in `validate_transition` | every autonomy activation receipt | field **wired and validated**; never minted live |

The `custody_floor` closed set is enforced in all three canonical mirrors the CI runs (Rust
`require_ratified_custody_floor` `m1nd-control/src/release.rs:719`, plus the Python and Node
canon verifiers); a smuggled `"software"` value is refused by a negative test in each.

## 4. The wiring gap, measured

The honest headline: **the G9 custody floor is a fully-implemented, fully-tested, entirely unwired
island.** Both ends of the wiring are absent.

| Symbol | References outside `enclave_authority.rs` |
|---|---|
| `SecurityFrameworkEnclaveKeyStore` | **0** |
| `provision_agent_enclave_seat` | **0** |
| `SecureEnclaveSigner` | **0** |
| `SealedProtectedRootV1` | **0** |
| `seal_custody_ceremony` / `read_custody_ceremony` | **0** |
| `bind_independence_spec` | **0** |
| `EnclaveBackedWalRecordCrypto` | **0** |
| `SecureEnclaveProtectedEpochBackend` | **0** |
| `SecureEnclaveOwnerSecurityConfigRootBackend` | **0** |
| `SecureEnclaveJournalHeadBackend` | **0** |

And the consumer end, `assemble_production_owner_authority_v1`
(`m1nd-mcp/src/owner_security_config.rs:676`), has **three callers, all inside `#[cfg(test)]`**
(that block starts at `:1147`; the calls are at `:1418`, `:1500`, `:1523`). Nothing in production
assembles the production owner authority.

The decision document said this in advance —
`docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md` §7: *"implement the concrete providers (enclave
signer wiring … into `assemble_production_owner_authority_v1`, protected-root provisioning
ceremony, quorum seat minting)"*. The providers were built. The wiring named in the same sentence
was not.

### The surface that must exist

Follow the repo's own established one-shot CLI pattern — `--verify-authorization-receipt`
(`m1nd-mcp/src/cli.rs:36`, dispatched `m1nd-mcp/src/main.rs:742`), `--inbox-sweep`
(`cli.rs`, dispatched `main.rs:809`), `--medulla-migrate <mode>` (dispatched `main.rs:819`). Each is
an early mode: parse, do one bounded thing offline, print JSON, exit — never booting an owner,
opening a port, or taking a lease. The ceremony belongs in exactly that family, with one hard
addition: the biometric-seat step must be reachable only by the owner, never by an agent path.

## 5. Residual engineering, ranked by blocking-ness

- **R1 (blocking, and invisible until the ceremony fails).** No `KeychainAccessGroups` entitlement
  anywhere. The repo has no `.entitlements` file, and `release.yml` codesigns with
  `--force --timestamp --options runtime --sign` and **no `--entitlements` flag**
  (`.github/workflows/release.yml:571-572`). The entitlement exists only in prose. As shipped, the
  signed release binary **cannot** run this ceremony: provisioning cannot persist and `open` cannot
  resolve. Fix: add an entitlements plist and thread `--entitlements` through the signing step, or
  document a separate locally-signed ceremony binary.
- **R2 (blocking).** No ceremony surface at all — §4. Neither a CLI mode nor a callable driver.
- **R3 (blocking for the ladder, not for the ceremony).** `assemble_production_owner_authority_v1`
  is never called from production, so even a completed ceremony would not install the custody floor
  into a running owner.
- **R4 (correctness, named by the original proof and still open).** Quorum wiring:
  `VerifierSeatV1` carries no public key, so `bind_independence_spec` binds by
  (principal, key_id, failure_domain) only — it does **not** force the sealed seat public key to
  equal the verification-key registry entry the quorum verifier resolves. The future quorum wiring
  must cross-check sealed-pubkey == registered-pubkey.
- **R5 (owner-observed, cannot be closed by code).** `kSecAccessControl` semantics are never read
  back. The hand-rolled flags (`1 << 30`, `1 << 0`) are proven only by the owner's live
  conformance run — step 3.
- **R6 (latent, future era).** `RATIFIED_CUSTODY_FLOORS` is a singleton today. A successor Path-A
  era that ADDS rather than replaces a floor would let the builder accept any set member via
  core-input, because the receipt→era bind is by string, not by ceremony-receipt digest. Inocuous
  now; decide at the first Path-A succession.

## 6. Coherence after the RustCrypto sweep (#464) — measured 2026-07-29

Measured on `main` at `b96b191f`, in a clean per-checkout target dir. The custody code is coherent
with the post-sweep crate majors (`p256 0.14.0`, `ecdsa 0.17.0`, `ed25519-dalek 3.0.0`,
`sha2 0.11.0`):

```
cargo test -p m1nd-control
  unittests src/lib.rs        137 passed; 0 failed
  tests/crypto_authority.rs    12 passed; 0 failed
  tests/crypto_continuity.rs   12 passed; 0 failed
  tests/crypto_p256.rs          3 passed; 0 failed
  Doc-tests m1nd_control        0 passed; 0 failed
                              ---
                              164 passed; 0 failed

cargo test -p m1nd-mcp --lib enclave_authority
                               13 passed; 0 failed; 1487 filtered out
```

#464 handled the two API seams itself rather than leaving them for the custody code:
`to_sec1_point` is in use at `m1nd-control/src/crypto_authority.rs:1213` and `:1237` and at
`enclave_authority.rs:1557`/`:1972`; `normalize_s` became infallible in `ecdsa 0.17` and the new
spelling landed with a comment recording the change (`crypto_authority.rs:1277-1281`). The sweep
touched `enclave_authority.rs` directly (4 lines). **No residual crypto-rename work exists.**

## 7. What the ceremony unblocks

The converging chain, in order: G8 signed release → G7 `COHERENT` manifest → G6 formal blind →
G9 receipts → activation → G10-as-amended. None of it can close until the custody floor is real,
because `assemble_production_owner_authority_v1` fail-closes without hardware-protected providers
and software test assurance is rejected by the code itself (`ProtectedProduction` only).
