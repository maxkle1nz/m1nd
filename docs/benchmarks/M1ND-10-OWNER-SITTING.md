# M1ND-10 — the owner's one-sitting completion script

> Written 2026-07-30 against `main` (post #473/#483). The owner said *"I will mint
> everything"* — this document is the order to do it in, with each step's REAL state
> as of this writing: **RUNS** (verified against the code), **REFUSES HONESTLY**
> (staged, answers `NOT_RUN` until its wiring lands), or **OWNER-AUTHORED** (no code
> can produce it). Agent-side simulation of any owner step is prohibited
> (`G9-CUSTODY-CEREMONY.md` §0) — this script stages, it never performs.

## The dependency truth, in one paragraph

The gates are one provenance chain, not a list. G9 (custody) mints the production
owner authority; G8 (signed release) ships the release the manifest binds — and,
since 2026-07-31, the entitled ceremony *bundle* G9's enclave steps require, which a
command-line binary could never be (Step 1); G7 (LIVE) can only prove once
manifest coherence stops being DRIFT, which needs the G9/G8 authorities to exist
(`organism_manifest.rs` holds G1-truth at DRIFT until then — measured live when the
owner's first G7 run answered `NOT_PROVEN: manifest coherence is DRIFT`); G10 closes
on top. The formal benchmark (G6) is gated by the same custody floor: its runner
refuses formal mode without a pinned production authority assembly. One sitting,
two tracks, one root: **everything converges on G9.**

## Step 0 — what the owner authors with no dependencies (do this first, any day)

**Metric spec v2** (`G6-FORMAL-CEREMONY.md` §8 item 1). The checked-in
`m1nd10-g6-metric-spec-v1.json` carries the thresholds; the v2 artifact re-mints
them under schema `m1nd10-g6-metric-spec-v2` with a calibration gate, an
outcome-blind ratification, and an authority receipt digest. Both the runner
(`validate_metric_spec_for_runner`) and the scorer (`_validate_spec`) refuse v1.
This is pure owner authorship — nothing blocks it, and nothing else can produce it.

## Step 1 — cut the release tag (G8's material half)

The release pipeline is ready: Developer ID signing + notarization (#433). Run
`/release-parity` first (crates.io ↔ npm), then tag. G8 the *gate* also wants the
manifest to bind this release — that binding is the same manifest work G7 waits on;
the tag itself is not blocked.

**Correction, measured 2026-07-30 — the tagged `m1nd-mcp` binary is NOT entitled,
and cannot be.** #469 signed the release with the `keychain-access-groups`
entitlement; the v1.6.0 run then failed, because that entitlement is *restricted*
and AMFI SIGKILLs a raw executable that claims one without an embedded provisioning
profile — which a non-bundled binary has nowhere to hold (Apple, TN3137). The
ordinary binary now ships unentitled and stays that way.

**And the resolution, 2026-07-31 — the tag now produces a SECOND artifact that is
entitled, once you supply one file.** Road A is ratified
(`G9-PLATFORM-DECISION.md`) and built: the release wraps the same binary bytes in
`m1nd-custody-ceremony.app`, embeds your provisioning profile, signs the bundle
WITH the entitlement, notarizes, staples, proves it launches, and publishes
`m1nd-custody-ceremony-macos-<arch>.zip` as an artifact of the release run.

**Do this once, before the tag** (it is the whole of the machine's remaining ask):

1. In the Apple Developer portal, generate a **macOS** provisioning profile of type
   **Developer ID** for the App ID whose suffix is the access group named in
   `build/m1nd-mcp.entitlements.plist` — a team wildcard App ID already covers it,
   so this is a profile to generate and download, not a capability to request. It
   must not be a *development* profile: those are scoped to enrolled devices, and
   the release refuses one by name because the artifact is launch-proven on a
   machine that is not yours.
2. `base64 -i <the .provisionprofile>` into the repository secret
   **`APPLE_CUSTODY_PROFILE_BASE64`**, next to the `APPLE_CERT_*` / `APPLE_API_*`
   secrets the signing step already uses.
3. Tag. With the secret absent the release still succeeds and simply warns that no
   ceremony artifact was published; with it present, the release fails loudly rather
   than publish a bundle that would not launch — including if the profile is expired
   or within 30 days of expiring, which it prints along with the date it read.

The profile expires (Apple issues them for about a year) and the bundle stops
launching when it does — the release run prints the expiry as a notice. Re-tagging
with a fresh profile is what renews it.

## Step 2 — the G9 custody ceremony (`G9-CUSTODY-CEREMONY.md` is the full runbook)

The CLI door exists on `main` (#473): `--custody-ceremony <verb>` dispatches before
any config loads, on a private-field ingress no agent path can construct.

**Run them from the ceremony bundle, not from `m1nd-mcp` on `PATH`.** Unzip the
release run's `m1nd-custody-ceremony-macos-<arch>.zip` and drive the same CLI at
`m1nd-custody-ceremony.app/Contents/MacOS/m1nd-mcp --custody-ceremony <verb>`. Same
executable, same verbs, same refusals — the only difference is the signature around
it, which is what makes the keychain answer instead of refusing. The plain binary
still refuses at P4 by name, and that refusal is correct.

Verb state today, from `custody_ceremony.rs::run_custody_ceremony`:

| Verb | State today | What it does |
|---|---|---|
| `preflight` | **RUNS** — reports every prerequisite, exit 0 only when ready | run it first, at the protected root; its P4 line stays `UNPROVEN` on every build by construction — this report never touches the keychain, so only the next verb can tell you whether the profile really authorized the entitlement (see Step 1) |
| `provision-seats` | **RUNS** — mints the four verifier seats + the sealing seat in the enclave-backed store, stages each record; re-provisioning refuses (`custody_ceremony_seats_already_staged`) | mint the seats; on an unentitled binary the keychain's own refusal surfaces, named |
| `owner-seat` | **RUNS, owner-only** — mints the biometric seat with the biometry-gated access control (Touch ID fires at key USE, by the ACL); an unattended invocation is still refused *as unattended* on every platform, before the platform floor | the biometric seat — irreducibly the owner's finger |
| `seal` | **RUNS** — only over a complete staged ceremony (4 seats + sealing seat + owner seat), binds the owner's independence spec and constitution digest, writes the sealed receipt, consumes the staging | seal seats + lineage + spec digests into the ceremony receipt |
| `assemble` | **RUNS on macOS** — consumes the sealed ceremony through the same `open_ceremony_root` that `seal` writes (coherence by construction); fails closed without one | emit the pinned production authority assembly the G6 runner demands |

**The door↔floor wiring landed (#498).** Every verb now reaches the real enclave
floor; two owner-held inputs joined the CLI (`--custody-independence-spec`,
`--custody-constitution-digest` — the ceremony READS the owner's spec, it never
builds one). What remains true: real execution needs the owner, the Mac, and an
entitled artifact — which the release **now produces**, as the ceremony bundle, once
the profile secret is set (Step 1). Every missing precondition still refuses closed
with its own name, and no agent path can perform any of it.

**Before the first verb, seal the spec (P9).** The independence spec is written by
hand, and every custody verb refuses one whose declared digest is not the digest of
its own core — so the draft is sealed once, first:

```bash
m1nd-mcp --seal-independence-spec independence-spec.draft.json > independence-spec.json
```

Offline, one file in, the sealed document out, exit 0 — it touches no enclave, no
keychain and no ceremony state, so it is not one of the four owner-only verbs and
runs anywhere. Whatever placeholder the draft's `independence_spec_digest` carried
is overwritten; a draft that breaks a structural floor (not four voting seats, a
quorum outside the three-of-four floor, fewer than three distinct failure domains,
a voting proposer/executor or sentinel) refuses by name with exit 1 instead. Then
pass the sealed file as `--custody-independence-spec` to `provision-seats` and
`seal`.

## Step 3 — G6 formal blind run (`G6-FORMAL-CEREMONY.md`)

One command, every path owner-held. After steps 0–2, the remaining refusals are:
the **authority provider executable** (machine-side, not yet built), the frozen
candidate + baseline binaries (owner pins them), and the ratified baseline run.
`scripts/benchmark/g6_formal_preflight.sh` names what is missing every time — last
staged measure: 22 PASS / 0 FAIL / 15 OWNER_INPUT_MISSING, exit 3
`READY_PUBLIC_ONLY`.

**Provider status (measured 2026-07-30):** the wire contract is fully derived from
the runner — the provider is copied alone into a deny-default sandbox
(`sandbox-exec` / `bwrap --unshare-all`), invoked with **no arguments and a
scrubbed env**, speaks one canonical-JSON request per process over stdin/stdout,
and is digest-pinned before and after every call. What is NOT derivable from any
consumer channel is its **identity**: three preflight fields
(`provider_kind`, `production_authority_assembly`, `assembly_id`) appear in no
request, the sandbox denies every owner-held path, and the assembly's
`self_digest` covers the provider's own digest — so the provider cannot embed
what is computed from it. A binary config-trailer is ruled out by measurement (it
voids the macOS signature; the shipped binary carries no entitlement to void, per
Step 1's correction, but an invalid signature is disqualifying on its own). The working
direction, pending the owner's veto: **keychain-resident provider identity**,
filed by the custody ceremony itself — the only channel that survives both the
sandbox (mach-lookup is allowed) and codesigning, and it keeps the signed binary
singular. The provider then ships as a mode of `m1nd-mcp`, built after the
ceremony-verb wiring lands.

## Step 4 — G7 LIVE, again (`G7-LIVE-CEREMONY.md`)

The same four commands the owner already ran once. The first run's receipt honestly
said `manifest_not_coherent`; after G9+G8 exist and the manifest binds them, the
same ceremony can answer PROVEN. Read the digests live via
`scripts/m1nd10_g7_live_expectations.py` — never from a document (this file
included); the doc-cited digest was already stale once while the build's refusal
caught it.

## Step 5 — G10

Autonomy epoch on top of a proven chain. Its authority does not exist yet by
design; it is the last mint, after everything above is receipts.

## The honest scoreboard for this sitting

**Chain gates proven: 0 of 4** (G9 · G8-as-gate · G7 · G10). No ceremony has run;
nothing below moves that number.

- **Done and provable:** the independence spec is authored and **sealed** — its
  digest is the owner's own, computed by `--seal-independence-spec`, and the ceremony
  refuses a spec whose digest does not match. v1.6.1 is published, signed, notarized,
  and its macOS binary launches — G8's *material* half. Step 0's metric spec v2 can be
  authored at any time, though its `authority_receipt_digest` stays empty until G9's
  `assemble` mints one.
- **Step 2 is no longer platform-blocked, and its owner file is in place.** The wall
  was real — a restricted entitlement on a command-line binary is SIGKILLed by the
  kernel — and Road A went through it: the release builds and proves the entitled
  ceremony bundle, and the ordinary binary stays unentitled. The macOS Developer ID
  profile was generated for the App ID the entitlement's access group names, its
  certificate matched against the one the published release actually signs with, and
  `APPLE_CUSTODY_PROFILE_BASE64` is set. What remains is a tag.
- **Unproven until that tag:** that a Developer-ID-signed bundle with a real profile
  satisfies AMFI for this entitlement. Only the negative is measured today. The
  release's own launch check decides it, before anything is published — a failure
  there is the answer arriving, not the plan collapsing.
- **Blocked behind step 2:** step 3's authority provider (also waiting on its own
  identity-channel decision) · step 4 (needs the manifest to bind G9+G8) · step 5.
