# M1ND-10 — the owner's one-sitting completion script

> Written 2026-07-30 against `main` (post #473/#483). The owner said *"I will mint
> everything"* — this document is the order to do it in, with each step's REAL state
> as of this writing: **RUNS** (verified against the code), **REFUSES HONESTLY**
> (staged, answers `NOT_RUN` until its wiring lands), or **OWNER-AUTHORED** (no code
> can produce it). Agent-side simulation of any owner step is prohibited
> (`G9-CUSTODY-CEREMONY.md` §0) — this script stages, it never performs.

## The dependency truth, in one paragraph

The gates are one provenance chain, not a list. G9 (custody) mints the production
owner authority; G8 (signed release) ships the entitled binary that G9's enclave
steps require *and* the release the manifest binds; G7 (LIVE) can only prove once
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

The release pipeline is ready: Developer ID signing + notarization (#433) and the
custody entitlement on the shipped bytes (#469 — before it, the notarized binary was
structurally incapable of the enclave ceremony; the release step now proves the
entitlement landed by reading the signed output). Tagging produces the **entitled
binary** the G9 steps below must run on. Run `/release-parity` first (crates.io ↔
npm), then tag. G8 the *gate* also wants the manifest to bind this release — that
binding is the same manifest work G7 waits on; the tag itself is not blocked.

## Step 2 — the G9 custody ceremony (`G9-CUSTODY-CEREMONY.md` is the full runbook)

The CLI door exists on `main` (#473): `--custody-ceremony <verb>` dispatches before
any config loads, on a private-field ingress no agent path can construct. Verb
state today, from `custody_ceremony.rs::run_custody_ceremony`:

| Verb | State today | What it does |
|---|---|---|
| `preflight` | **RUNS** — reports every prerequisite, exit 0 only when ready | run it first, on the entitled binary, at the protected root |
| `provision-seats` | **RUNS** — mints the four verifier seats + the sealing seat in the enclave-backed store, stages each record; re-provisioning refuses (`custody_ceremony_seats_already_staged`) | mint the seats; on an unentitled binary the keychain's own refusal surfaces, named |
| `owner-seat` | **RUNS, owner-only** — mints the biometric seat with the biometry-gated access control (Touch ID fires at key USE, by the ACL); an unattended invocation is still refused *as unattended* on every platform, before the platform floor | the biometric seat — irreducibly the owner's finger |
| `seal` | **RUNS** — only over a complete staged ceremony (4 seats + sealing seat + owner seat), binds the owner's independence spec and constitution digest, writes the sealed receipt, consumes the staging | seal seats + lineage + spec digests into the ceremony receipt |
| `assemble` | **RUNS on macOS** — consumes the sealed ceremony through the same `open_ceremony_root` that `seal` writes (coherence by construction); fails closed without one | emit the pinned production authority assembly the G6 runner demands |

**The door↔floor wiring landed (#498).** Every verb now reaches the real enclave
floor; two owner-held inputs joined the CLI (`--custody-independence-spec`,
`--custody-constitution-digest` — the ceremony READS the owner's spec, it never
builds one). What remains true: real execution needs the owner, the entitled
binary, and the Mac — every missing precondition refuses closed with its own
name, and no agent path can perform any of it.

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
voids the macOS signature and with it the keychain entitlement). The working
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

- Owner can do TODAY: step 0 (metric spec v2) · step 1 (tag) · **step 2 whole**
  (all five verbs wired since #498 — the ceremony itself now runs to the sealed
  receipt and the assembly, on the entitled binary, by the owner's hand).
- Blocked on one named machine-side item: step 3's authority provider executable —
  itself waiting on the owner's identity-channel decision (keychain-resident,
  recorded above).
- Blocked on the chain itself: step 4 (needs 2+1's manifest binding) · step 5.
