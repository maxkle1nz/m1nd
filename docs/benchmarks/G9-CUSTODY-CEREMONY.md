# M1ND-10 — G9 Secure Enclave custody ceremony — the owner's runbook

> **Status: STAGED, NOT_RUN.** Every provider this ceremony needs is implemented, tested and
> merged on `main`, and since the door (#473) and the verb wiring the five verbs now REACH those
> providers rather than reporting a placeholder. What has still never happened is the ceremony:
> no Secure Enclave key has been minted, no Touch ID prompt has been answered, and no
> `custody-ceremony.sealed.json` exists. This document writes down the ceremony as it WILL be,
> and §4/§5 record which parts of the gap it measured are closed and which are not.
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
prove the logical contract in-process — 14 in `enclave_authority::tests` and 14 in
`custody_ceremony::tests`, all against temp directories and a software P-256 key store. The two
suites drive the SAME fake (`enclave_authority::test_support::MockEnclaveKeyStore`): a second fake
for the door would be a second contract, and the door could then be proven against a boundary the
floor does not have.

Wiring the verbs does not soften this section. It moves the refusals from a hand-written answer to
the platform's own: an unentitled binary now fails at the keychain and the failure is reported as
prerequisite P4, which is the honest form of "this did not run".

## 1. Prerequisites

| # | Prerequisite | State today | Where |
|---|---|---|---|
| P1 | Apple Silicon / T2 Mac with a Secure Enclave, owner physically present | owner's machine | — |
| P2 | Touch ID enrolled for the owner | owner's machine | — |
| P3 | macOS build target — the module is `#[cfg(target_os = "macos")]` and absent by construction elsewhere, so the assembly stays NOT_INSTALLED and fails closed rather than falling back to software | **satisfied** | `m1nd-mcp/src/lib.rs:36-37` |
| P4 | Binary codesigned with a **`KeychainAccessGroups` entitlement**. Secure Enclave keys can only be made permanent in the data-protection keychain; an unsigned or unentitled binary cannot persist *or* resolve the key, so `provision`/`open`/`sign` all fail closed | **UNSATISFIED until the owner supplies one file — then the release ships the entitled artifact itself.** #469 threaded `--entitlements` onto the raw binary and AMFI SIGKILLed the v1.6.0 product at launch: `keychain-access-groups` is *restricted*, and a raw executable has nowhere to embed the provisioning profile that would authorize it (measured 2026-07-30, run `30556058443`; TN3137 § Implementation differences). Road A is ratified (`G9-PLATFORM-DECISION.md`), so the release now builds a **second** macOS artifact — `m1nd-custody-ceremony.app`, the same binary bytes inside an app-like bundle with the profile at `Contents/embedded.provisionprofile`, signed WITH the entitlement, notarized, stapled, and proven to launch before publication. The ordinary `m1nd-mcp` stays unentitled and keeps refusing one. **The owner's remaining act is one-time and off-machine:** generate a macOS **Developer ID** provisioning profile for the App ID whose suffix is the access group in `build/m1nd-mcp.entitlements.plist`, and paste it base64-encoded into the repository secret `APPLE_CUSTODY_PROFILE_BASE64`. Without that secret the release publishes NO ceremony artifact and says so loudly; local builds keep failing closed here, naming P4 | `m1nd-mcp/src/enclave_authority.rs:1197-1222`, `:1489`, `build/README.md`, `.github/workflows/release.yml:539-935` |
| P5 | A `0700`, non-symlink, device/inode-pinned protected root directory for the sealed slots | code ready, root not provisioned | `SealedProtectedRootV1::open` — `m1nd-mcp/src/enclave_authority.rs:441` |
| P6 | Custody dependency pins intact — `security-framework =3.7.0` (feature `OSX_10_15`), `security-framework-sys =2.17.0`, `core-foundation =0.10.1`. **These are custody surface; never bump them opportunistically** | **satisfied** | `m1nd-mcp/Cargo.toml:112-114` |
| P7 | Crypto stack coherent after the RustCrypto sweep (#464) | **satisfied, measured** (see §6) | — |
| P8 | A CLI or callable ceremony surface to drive the steps below | **satisfied** — `m1nd-mcp --custody-ceremony <verb>`, all five verbs reaching the floor | `m1nd-mcp/src/custody_ceremony.rs`, dispatched `m1nd-mcp/src/main.rs:789` |
| P9 | The owner's `IndependenceSpecV1` (JSON) — the ceremony provisions and seals the seats IT names and invents none. Required by `provision-seats` and `seal`. The owner writes the four voting seats by hand and then seals the file with `m1nd-mcp --seal-independence-spec <draft> > independence-spec.json`, which fills `independence_spec_digest` from the digest of the spec's own core (whatever placeholder the draft carried is overwritten) and refuses by name if the draft breaks a structural floor — without it every custody verb refuses a hand-authored spec at the digest check | owner-held | `--custody-independence-spec`, sealed by `--seal-independence-spec` (`m1nd-mcp/src/seal_independence_spec.rs`) |
| P10 | The owner's constitution digest (lowercase sha-256 hex), recorded in the receipt and never computed here | owner-held | `--custody-constitution-digest` |

## 2. The step list as it WILL be

Each step names what already exists (with `file:line`) and what is missing. All paths are relative
to the repo root. `enclave_authority.rs` means `m1nd-mcp/src/enclave_authority.rs`, and
`custody_ceremony.rs` means `m1nd-mcp/src/custody_ceremony.rs` — the door, which is where every
WIRED line below points.

**Where the owner runs these.** Not from the ordinary `m1nd-mcp` on `PATH` — that one is
deliberately unentitled and refuses at the keychain, naming P4. The verbs are run from the
ceremony bundle published by the release run for the tag, `m1nd-custody-ceremony-macos-<arch>.zip`,
unzipped anywhere the owner likes:
`m1nd-custody-ceremony.app/Contents/MacOS/m1nd-mcp --custody-ceremony <verb>`. It is the same
executable and the same CLI — nothing about the verbs, the ingress or the refusals changes; only
the signature around it does. Prerequisite P4 above names the one-time step that makes the release
produce it.

The ceremony reads the seats it provisions out of the owner's `IndependenceSpecV1` (P9) and never
invents one. That is not convenience: a ceremony that fabricated its own seats would make step 6's
`bind_independence_spec` a tautology, binding to what it had just made up.

### Phase A — provision the four unattended verifier seats (owner, on the entitled binary)

1. **Construct the production key store, one per seat class.**
   `SecurityFrameworkEnclaveKeyStore::new(label_prefix, subject_id, EnclaveAccessControlV1::PrivateKeyUsageNonExportable)`.
   - EXISTS: `enclave_authority.rs:1183` (struct), `:1190` (ctor), `:1389` (`SecureEnclaveKeyStoreV1` impl).
   - WIRED: `custody_store` `custody_ceremony.rs:718`, one store per seat class, built by the
     `provision-seats`, `owner-seat` and `seal` verbs.

2. **Provision each of the four seats** with a distinct `key_id` and `failure_domain`, spanning at
   least three distinct domains, each permit carrying its `bound_context_digest` (sealed later as
   seat lineage). Provision is never open-or-create: an existing item under the same
   `kSecAttrLabel` fails closed, and the created key's real token/type/size are read back via
   `SecKeyCopyAttributes` and attested against the framework's own constants. The MIRROR case — an
   ABSENT label on a fresh keychain, the only time provisioning is legitimate — must PROCEED to
   create. Apple answers that no-match query with `errSecItemNotFound`, which `security-framework`
   surfaces as `Err`; the guard originally misread that error as a fatal open and aborted, so on a
   clean keychain no seat could ever be minted. Found live in the owner's first real G9 ceremony
   against the entitled `m1nd-custody-ceremony.app` bundle (v1.6.2) — the software fake returns
   `Ok(None)` on absent and never reproduced it — and fixed in `resolve_persisted_key` by routing
   the search error through `classify_keychain_search_error`, which maps only `errSecItemNotFound`
   to absent and keeps every other OSStatus fatal (a new signed release is required to carry the
   fix onto an entitled bundle).
   - EXISTS: `provision_agent_enclave_seat` `enclave_authority.rs:187`;
     `EnclaveProvisioningPermitV1` `:136`; `EnclaveKeyAttestationV1::canonical` `:101`;
     duplicate guard via `resolve_persisted_key` `:1247`.
   - Counts are law: `IMMUTABLE_VERIFIER_SEATS = 4` (`m1nd-control/src/autonomy.rs:54`),
     `IMMUTABLE_FAILURE_DOMAINS = 3` (`:56`).
   - WIRED: `provision_seats_into_store` `custody_ceremony.rs:866` loops the spec's four permits,
     captures each public key, and stages them. The counts are checked against the spec BEFORE the
     first key is minted (`require_a_usable_independence_spec` `:811`), because discovering a bad
     spec afterwards would leave orphaned keys only a re-provisioning ceremony can clear. A fifth
     unattended key — the ceremony's own sealing seat, `custody-sealing-seat-v1` — is minted in the
     same step; it casts no vote and is staged separately from the four.
   - Idempotence is a REFUSAL, matching the floor's never-open-or-create law: a second run answers
     `custody_ceremony_seats_already_staged` rather than re-staging over live keys.

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
   - LANDED: `provision_owner_biometric_seat` `enclave_authority.rs:219`, the exact mirror — it
     refuses every NON-biometric class as its sibling refuses the biometric one, so the two are
     exhaustive over the seat classes and neither can mint the other's seat. Driven only by
     `provision_owner_seat_into_store` `custody_ceremony.rs:937`, behind the CLI ingress and the
     owner-presence gate, which refuses an unattended process on every target before the platform
     question is even asked.
   - Still the owner's, irreducibly: minting the key is not USING it. The Touch ID prompt is raised
     by the key's own `kSecAccessControl` when it signs, which is hardware the code cannot stand in
     for.

### Phase C — open, attest, seal

5. **Open and re-attest every seat before it signs anything.**
   `SecureEnclaveSigner::open_attested(store, key_id, &EnclaveKeyAttestationV1::canonical(access_control))` —
   token/type/size drift refuses fail-closed.
   - EXISTS: `enclave_authority.rs:214`; drift refusal proven by
     `reattestation_refuses_token_type_or_size_drift`.
   - WIRED: `open_ceremony_root` `custody_ceremony.rs:994` opens and re-attests the sealing seat.
     The four verifier seats are re-attested at provisioning time by `provision_agent_enclave_seat`
     and are not opened again by the ceremony: it seals their public halves, it never signs with
     them. The quorum wiring that will use them is R4, still open.

6. **Build the `IndependenceSpecV1`** (four seats, three failure domains) and the
   **`EnclaveCustodyCeremonyReceiptV1`**: `custody_floor`, the attestation distinction, the four
   seat public keys + lineage digests, the owner seat key, and the spec/constitution digests. Both
   `validate()` and `bind_independence_spec(&spec)` must pass — the seats are sealed BEFORE any
   quorum vote is counted.
   - EXISTS: receipt `enclave_authority.rs:907`, `validate` `:925`, `bind_independence_spec` `:993`,
     `CustodyAttestationDistinctionV1::secure_enclave_single_host` `:876`;
     `IndependenceSpecV1` `m1nd-control/src/autonomy.rs:356`.
   - WIRED: `seal_with_store` `custody_ceremony.rs:1034` builds the receipt from the staged seats
     and runs BOTH `validate()` and `bind_independence_spec(&spec)` before anything is written. The
     spec is the owner's (P9), not one this step composes. One check the receipt schema cannot make
     is made here: the staged seats carry the lineage digest of the spec they were provisioned
     under, and presenting a DIFFERENT spec at seal is refused even when its seat set matches.

7. **Enclave-seal into the protected root**, then re-open and read back to confirm.
   `SealedProtectedRootV1::open(root, context_digest, signer, verification_key)?.seal_custody_ceremony(&receipt)`,
   then `read_custody_ceremony()`.
   - EXISTS: `open` `enclave_authority.rs:441`, `seal_custody_ceremony` `:1035`,
     `read_custody_ceremony` `:1051`.
   - WIRED: `seal_with_store` seals and immediately reads back, refusing if what returns is not
     what was sealed. The root is opened by `open_ceremony_root`, the SAME function `assemble`
     calls, so the sealing key, root binding and context digest cannot drift between the step that
     writes the ceremony and the step that consumes it. On success the staging file is removed: a
     completed ceremony leaves only the sealed receipt behind.

8. **Retire the live proof key** from the Keychain and record the seat public keys, the digests and
   the sealed receipt path.
   - PARTLY WIRED: every verb prints its result as one closed JSON object — the staged seat rows
     with their public keys, and on seal the full receipt plus the sealed slot path — so the record
     the owner keeps is the command's own output, not a transcription.
   - MISSING: retiring the key. That remains an owner operational step with no code behind it, and
     deliberately so: deleting custody material is not something this binary should be able to do.

## 3. The receipts this ceremony emits

| Receipt | Schema / constant | Where it lands | State |
|---|---|---|---|
| `EnclaveCustodyCeremonyReceiptV1` | `m1nd-enclave-custody-ceremony-receipt-v1` (`enclave_authority.rs:860`) | enclave-sealed slot `<protected-root>/custody-ceremony.sealed.json` (`:862`), seal domain `m1nd-enclave-custody-ceremony-v1` (`:861`) | **never minted.** The path that mints it is wired (`seal`, §2 steps 6-7) and printed verbatim by the verb |
| Seat public keys ×4 + owner seat key | 65-byte uncompressed SEC1 P-256, lowercase hex (`:896`, validator `:1069`) | staged by `provision-seats`/`owner-seat` into `<protected-root>/custody-seats.staged.json`, sealed into the receipt, printed by each verb | **never minted.** The staging file is a work-in-progress record under its own schema (`m1nd-custody-ceremony-staged-v1`) and is consumed on a successful seal |
| `GateReceiptCoreV1.custody_floor` | value `secure-enclave-single-host-v1` (`m1nd-control/src/release.rs:25`), closed set `RATIFIED_CUSTODY_FLOORS` (`:33`) | every G0–G10 gate receipt | field **wired and validated**; no real receipt minted under a real ceremony |
| `AutonomyActivationReceiptCoreV1.custody_floor` | same closed set, validated in `validate_transition` | every autonomy activation receipt | field **wired and validated**; never minted live |

The `custody_floor` closed set is enforced in all three canonical mirrors the CI runs (Rust
`require_ratified_custody_floor` `m1nd-control/src/release.rs:719`, plus the Python and Node
canon verifiers); a smuggled `"software"` value is refused by a negative test in each.

## 4. The wiring gap, measured — and closed

The headline this section carried, *"the G9 custody floor is a fully-implemented, fully-tested,
entirely unwired island"*, is no longer true. Every symbol below was measured at **0** references
outside `enclave_authority.rs` when this runbook was written; the same measurement today:

| Symbol | References outside `enclave_authority.rs` | Then | Now |
|---|---|---|---|
| `SecurityFrameworkEnclaveKeyStore` | `custody_ceremony.rs` (`custody_store`) | 0 | wired |
| `provision_agent_enclave_seat` | `custody_ceremony.rs` (Phase A, four seats + the sealing seat) | 0 | wired |
| `provision_owner_biometric_seat` | `custody_ceremony.rs` (Phase B) | did not exist | wired |
| `SecureEnclaveSigner` | `custody_ceremony.rs` (`open_ceremony_root`) | 0 | wired |
| `SealedProtectedRootV1` | `custody_ceremony.rs` (ceremony root + the three sub-roots) | 0 | wired |
| `seal_custody_ceremony` / `read_custody_ceremony` | `custody_ceremony.rs` (`seal` writes, `assemble` reads) | 0 | wired |
| `bind_independence_spec` | `custody_ceremony.rs` (`seal`, before anything is written) | 0 | wired |
| `EnclaveBackedWalRecordCrypto` | `custody_ceremony.rs` (`assemble`) | 0 | wired |
| `SecureEnclaveProtectedEpochBackend` | `custody_ceremony.rs` (`assemble`) | 0 | wired |
| `SecureEnclaveOwnerSecurityConfigRootBackend` | `custody_ceremony.rs` (`assemble`) | 0 | wired |
| `SecureEnclaveJournalHeadBackend` | `custody_ceremony.rs` (`assemble`) | 0 | wired |

The measurement is now a test, not a table anyone has to remember to re-run:
`the_ceremony_verbs_reference_the_floor_s_provisioning_and_sealing_primitives`
(`m1nd-mcp/tests/custody_ceremony_wiring.rs`) fails if any of them goes back to zero, and it also
fails if anything OTHER than the ceremony door references them.

The consumer end, `assemble_production_owner_authority_v1`
(`m1nd-mcp/src/owner_security_config.rs:676`), had **three callers, all inside `#[cfg(test)]`**.
The `assemble` verb is now its production caller, pinned by
`the_production_authority_assembly_has_a_non_test_caller` in the same battery. What that verb does
NOT do is install the assembly into a running owner — see §5 R3.

The decision document said this in advance —
`docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md` §7: *"implement the concrete providers (enclave
signer wiring … into `assemble_production_owner_authority_v1`, protected-root provisioning
ceremony, quorum seat minting)"*. The providers were built first; the wiring named in the same
sentence followed. **Quorum seat minting is the third clause of that sentence and remains open** —
the seats are minted and sealed, but nothing yet cross-checks a sealed seat key against the
verification-key registry the quorum verifier resolves (R4).

### The surface, as built

It follows the repo's own one-shot CLI pattern — `--verify-authorization-receipt`
(`m1nd-mcp/src/cli.rs:36`, dispatched `m1nd-mcp/src/main.rs:742`), `--inbox-sweep`, and
`--medulla-migrate <mode>`. `--custody-ceremony <verb>` is an early mode: parse, do one bounded
thing offline, print one closed JSON object, exit — never booting an owner, opening a port, or
taking a lease. The hard addition the gap named is in place: the biometric-seat step refuses an
unattended process BEFORE the platform question is asked, on every target, and it is minted by an
entry point no agent-side function can reach.

Two properties are worth stating because they are load-bearing rather than incidental:

- **The store is injected, not global.** Each verb builds the production
  `SecurityFrameworkEnclaveKeyStore` and hands it to the function that does the work — the same
  narrow `SecureEnclaveKeyStoreV1` boundary the floor declares for exactly this reason. That is
  what lets the battery prove the custody path with a software key; it is not a bypass, because the
  verbs pass the real store and nothing outside the crate can call the inner functions.
- **`seal` and `assemble` open the ceremony root through ONE function.** `open_ceremony_root`
  derives the sealing key, the root binding and the context digest once, so "assemble consumes what
  seal wrote" holds by construction instead of by two code paths agreeing.

## 5. Residual engineering, ranked by blocking-ness

- **R1 (the machine half is BUILT; one owner file remains — reopened 2026-07-30 by
  measurement, road ratified and implemented 2026-07-31).** The
  original entry said the signed release binary was structurally incapable of running this
  ceremony because `release.yml` passed no `--entitlements`. #469 added
  `build/m1nd-mcp.entitlements.plist` and threaded the flag through — and that is where the
  reasoning was wrong. `keychain-access-groups` is a **restricted** entitlement: AMFI honours it
  only when an embedded provisioning profile authorizes it, and a raw Mach-O executable has
  nowhere to embed one (a profile lives at `Contents/embedded.provisionprofile` inside a bundle).
  Apple states it in TN3137 § Implementation differences — those entitlements "must be authorized
  by a provisioning profile. Your program needs an app-like bundle structure in which to embed
  that profile. This is standard for app and app extensions but not for command-line tools."
  The consequence was measured, not inferred: release run `30556058443` (tag `v1.6.0`) signed,
  notarized (`Accepted`) and verified both macOS binaries with the entitlement provably on the
  bytes, and the **installed-artifact smoke then failed on both macOS legs** with `--version`
  dying on `SIGKILL`. The exact artifact reproduces it off-CI (exit 137); the kernel's reason is
  "Code has restricted entitlements, but the validation of its code signature failed", with amfid
  reporting `-413 "No matching profile found"`. Same bytes re-signed without the entitlement run.
  A minimal `.app` wrapper without an `embedded.provisionprofile` is killed identically.
  **So the ordinary runtime ships unentitled** (`.github/workflows/release.yml:539-634`: no
  `--entitlements`, a refusal if any restricted entitlement appears on the output, and a launch
  check on the signed bytes). Every raw binary — shipped or local — therefore refuses at P4,
  surfaced as `custody_ceremony_keychain_entitlement_missing` rather than an opaque OSStatus, which
  is the honest answer and not a regression.
  **The decision was made and the machine half is built.** The owner ratified Road A on
  2026-07-31 (`G9-PLATFORM-DECISION.md`): the ceremony surface ships inside an app-like bundle,
  Apple's own workaround for this case (*Signing a daemon with a restricted entitlement*). The
  release now produces `m1nd-custody-ceremony.app` per macOS target — the SAME binary bytes, the
  profile at `Contents/embedded.provisionprofile`, signed WITH the entitlement, notarized,
  stapled, launch-proven on the runner before and after packaging, and published as
  `m1nd-custody-ceremony-macos-<arch>.zip` outside the signed candidate byte set. The ordinary
  runtime is untouched and stays unentitled.
  **What is left is one owner file and one owner run.** The profile cannot be minted by this
  pipeline: the owner generates a macOS **Developer ID** provisioning profile for the App ID whose
  suffix is the access group in `build/m1nd-mcp.entitlements.plist` and puts it, base64-encoded,
  in the repository secret `APPLE_CUSTODY_PROFILE_BASE64`. Absent it the release publishes no
  ceremony artifact and warns; present it, the release refuses to publish one whose profile is
  expired (or within 30 days of it), device-scoped, non-macOS, or issued for an App ID the derived
  bundle identifier does not match. **Still unproven, and only a tagged release can prove it:**
  that a Developer-ID-signed bundle with a real profile satisfies AMFI for this entitlement — the
  negative was measured (a bundle without a profile dies exactly like the raw binary), the
  positive is decided by the launch check on the first such release. And the persistence proof — a
  real key persisted and resolved across a process restart — still happens only in the owner's run.
- **R2 (CLOSED).** The ceremony surface exists and all five verbs reach the floor — §4. The one
  thing no surface can supply is the owner's hand.
- **R3 (open — blocking for the ladder, not for the ceremony).** `assemble` is now a production
  caller of `assemble_production_owner_authority_v1`, but it is a ONE-SHOT: it assembles the
  authority from the sealed ceremony, prints the pinned manifest, drops the assembly and exits.
  Nothing installs that assembly into a running owner, so a completed ceremony still does not put
  the custody floor under the served process. That handoff is the next mechanical step.
- **R4 (correctness, named by the original proof and still open).** Quorum wiring:
  `VerifierSeatV1` carries no public key, so `bind_independence_spec` binds by
  (principal, key_id, failure_domain) only — it does **not** force the sealed seat public key to
  equal the verification-key registry entry the quorum verifier resolves. The future quorum wiring
  must cross-check sealed-pubkey == registered-pubkey. What the wiring added is adjacent and does
  not close it: the seats' public keys are sealed into the receipt, the staged seats carry the
  lineage digest of the spec they were provisioned under (a spec swapped between provisioning and
  sealing is refused), and the sealing key resolved from the keychain must equal the one the
  ceremony staged. None of that reaches the registry.
- **R5 (owner-observed, cannot be closed by code — and the wiring did not try).** `kSecAccessControl`
  semantics are never read back. The hand-rolled flags (`1 << 30`, `1 << 0`) are proven only by the
  owner's live conformance run — step 3. The ceremony sets the flags for the class it is minting and
  claims nothing about whether the persisted key really carries them, because there is no API to
  ask. Minting the owner's biometric seat therefore raises no Touch ID prompt: the prompt belongs to
  USING the key, and nothing in the ceremony signs with it.
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

That block is a dated snapshot and is left as measured. The counts moved with the wiring: the
floor's suite is 14 (the owner-only entry point's mirror refusal), and the door's own suite is 14
more (`cargo test -p m1nd-mcp --lib custody_ceremony`).

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
