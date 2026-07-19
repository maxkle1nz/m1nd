# M1ND-10 R9/G8 — canonical release contract and updater bridge

Date: 2026-07-18  
Scope: `ReleaseCandidateManifestV1`, `GateReceiptV1`, independent review,
structural convergence, cross-language canonical JSON, and the verified updater
bridge.  This receipt does not promote or publish a release.

## Verdict

`PASS` for the local, non-Cargo structural implementation and negative tests.

`NOT_RUN` for Rust compilation/tests in this lane, hosted GitHub Actions, live
Sigstore/OIDC verification, registry publication, and a real G0–G10 candidate.

`NOT_PROVEN` for cryptographic authentication of the opaque signatures and for
M1ND-10 release promotion.  Every successful structural validator reports
`STRUCTURALLY_VALID_NOT_CRYPTOGRAPHICALLY_VERIFIED`.

No candidate was synthesized from the dirty working tree.  No tag, commit,
push, GitHub Release, crates.io publication, or npm publication occurred.

## Authority and compatibility boundary

The source authority was read-only:

- `m1nd-control/src/canonical.rs` defines UTF-8 canonical JSON with recursively
  sorted object keys, no trailing newline, and the domain-separated SHA-256
  framing `m1nd-domain-separated-sha256-v1\0 || u64be(domain_len) || domain ||
  u64be(payload_len) || payload`.
- `m1nd-control/src/release.rs` defines the exact candidate, G0–G10 receipt,
  finding, independent-review, and structural convergence laws.
- `m1nd-control/src/lib.rs` makes `OpaqueSignature` structural only.  Validation
  therefore enforces the exact Rust law—non-empty bytes—not a made-up crypto
  claim.

`m1nd-control/src/release.rs` was not edited.

The existing updater-facing schema `m1nd-release-candidate-v1` remains accepted
for ordinary releases.  It is not relabeled as the canonical M1ND-10 contract.
The updater now also accepts `m1nd-release-candidate-manifest-v1`, but only when:

1. external cosign verification of `CANDIDATE.json` succeeds;
2. the domain-separated `candidate_digest` recomputes exactly;
3. the candidate binds the raw SHA-256 of canonical
   `RELEASE-COMPATIBILITY.json` through both
   `compatibility_manifest_digest` and the pinned artifact key
   `release_compatibility_manifest_v1`;
4. `repo_commits.m1nd`, version, tag, target, asset name, byte size, and runtime
   SHA-256 all match the planned update; and
5. the pinned `release_rollback_plan_v1` artifact key equals
   `rollback_plan_digest`.

This dual-reader sequence avoids publishing a CLI that rejects every release
which necessarily predates the first canonical candidate.

## Canonical implementation

`scripts/m1nd10_release_contract.py` ports the Rust structural laws without
using Cargo.  It provides:

- lossless integer-only JSON, duplicate-object-member refusal, and canonical
  UTF-8/no-newline bytes;
- domain-separated digests;
- exact/unknown-field-refusing candidate, gate, finding, and review validators;
- candidate, G0–G10 gate, and independent-review sealers;
- exact convergence: each of G0 through G10 exactly once, every receipt bound
  to the same candidate, every verdict `PASS`, independent review `PASS`, and
  no open P0/P1;
- an explicitly new JSON extension,
  `m1nd-release-evidence-set-json-extension-v1`, because Rust
  `ReleaseEvidenceSetV1` is memory-only and has no serde wire contract.

The enum wire values are pinned explicitly:

- gate IDs: `G0` … `G10`;
- gate verdicts: `PASS`, `FAIL`, `NOT_RUN`, `NOT_PROVEN`;
- finding statuses: `OPEN`, `CLOSED`;
- finding severities: `P0`, `P1`, `P2`, `P3`, `Info`—not `INFO`;
- active modes: `HUMAN_GATED`, `POLICY_AUTONOMOUS`, `FULL_AUTONOMY`.

Python emits fixture signatures only with the loud prefix
`NOT_CRYPTOGRAPHIC:`.  Production-mode builders refuse that prefix.  The base
validators still follow Rust exactly and accept any non-empty opaque signature;
the prefix is an emission policy, not a fabricated Rust validation law.

The G8 sealer refuses `PASS` from updater smokes alone.  It requires digest
entries for tool-catalog parity, first-minute host benchmark, capability matrix,
and the G8 ADR.  This is deliberately a builder-level completeness law above
the source-identical Rust structural validator.

## Cross-language vectors

`tests/fixtures/M1ND10-CANONICAL-VECTORS.json` is synthetic and fixture-only.  It
contains:

- canonical UTF-8, escapes, null/boolean, negative integer, and no-newline
  cases;
- `u64` values above JavaScript's `2^53` safe-integer boundary;
- UTF-8 key ordering that distinguishes Rust byte ordering from JavaScript's
  default UTF-16 sort;
- decimal/exponent floats, duplicate object members, and unpaired-surrogate
  refusal cases;
- pinned operational artifact key names;
- one exact canonical candidate, eleven exact candidate-bound receipts G0–G10,
  one independent review, and one convergent evidence-set JSON extension.

Python and Node independently recompute every canonical case, candidate/gate/
review digest and identifier, and the convergence law.  Node uses a small
integer-only parser rather than lossy `JSON.parse`, so the path remains valid on
the package's Node `>=18` support floor.

Compiled Rust parity is intentionally `NOT_RUN` until the coordinated Cargo
gate is released.  The vectors are derived from the read-only Rust source, but
are not falsely labeled as compiled Rust proof.

## Release-lane separation and first canonical candidate

The existing `.github/workflows/release.yml` remains the ordinary, already
proven G8 publication lane.  It does not claim G0–G10 convergence and it is not
blocked waiting for evidence producers which do not yet exist.

The same build now prepares, before candidate sealing, non-circular canonical
operational inputs from the exact G8 bytes:

- `RELEASE-COMPATIBILITY.json`;
- `M1ND10-ROLLBACK.json` (no candidate digest, so no self-reference);
- `CANONICAL-OPERATIONAL-DIGESTS.json`, with
  `release_artifact:<filename>` for every current release input,
  `release_asset:<asset>` for updater-facing bytes, and the two pinned
  compatibility/rollback keys.

Preparation requires exactly one SPDX SBOM.  The compatibility and rollback
documents independently pin the same version, commit, source ref, target set,
asset names, positive `u64` byte sizes, and runtime SHA-256 values; sealing and
verification refuse non-canonical bytes or any cross-document drift.

Those supplemental files are copied into the ordinary release set only after
the legacy candidate is assembled.  They then enter the same SHA256SUMS,
attestation glob, per-file cosign loop, and immutable release upload.  They do
not silently change the identity of the legacy candidate.

The first canonical M1ND-10 candidate must follow this separate ceremony:

1. a clean tagged G8 build produces exact artifacts and the signed operational
   inputs above;
2. an authorized producer supplies `m1nd-release-candidate-core-input-v1` with
   declared `HUMAN_RATIFIED` or `GOVERNANCE_QUORUM` authority and an authority
   receipt digest; the declaration is not itself authentication;
3. the candidate builder refuses missing fields, placeholders, `NOT_PROVEN`,
   digest drift, commit drift, or circular compatibility/rollback inputs;
4. the sealed canonical candidate receives external cryptographic provenance;
5. independent gate producers emit complete G0–G10 receipts for that exact
   candidate; an independent reviewer emits the IAR receipt;
6. an external crypto layer verifies every real signature/key lifecycle;
7. structural convergence runs; only a separate, owner-visible M1ND-10
   promotion authority may publish the M1ND-10 mark.

Until steps 2–7 exist with real evidence, ordinary releases continue and
M1ND-10 promotion remains `NOT_PROVEN`.  This is intentional fail-closed
separation, not a silent registry outage.

## Local gates executed

| Gate | Result |
|---|---|
| Python compile for both release scripts | `PASS` |
| Canonical Python contract + negative tests | `PASS` (12) |
| Legacy G8 Python regression tests | `PASS` (11) |
| Node syntax check | `PASS` |
| npm CLI tests, including legacy/canonical updater and tamper refusal | `PASS` |
| `actionlint .github/workflows/release.yml` | `PASS` |
| Python/Node checked-in vector recomputation | `PASS` |
| Cargo/Rust compiled parity | `NOT_RUN` (coordination hold) |
| Hosted release workflow | `NOT_RUN` |
| Real G0–G10/IAR evidence + crypto verification | `NOT_PROVEN` |

## Independent preflight

askGOD preflight returned `CHANGE` with high confidence.  All required changes
were applied:

- ordinary releases, crates.io, and npm were not frozen behind unavailable
  G0–G10 producers;
- the updater kept legacy compatibility through first canonical release;
- `NOT_CRYPTOGRAPHIC:` became fixture emission policy only;
- exact enum values, UTF-8/no-newline canonicalization, integer-only numbers,
  large `u64`, and evidence-set extension status were pinned;
- G8 updater smokes alone cannot produce a G8 `PASS` receipt;
- compatibility/rollback artifact names enter the control, checksum, signing,
  and vector discipline.

Final askGOD review used the Fugu fallback because the preferred Fable route
reported insufficient credit.  The first broad Fugu run was interrupted after
repeated context-compaction loops; it surfaced duplicate-object-member
ambiguity, which was fixed and regression-tested.  A fresh, bounded review of
the stable diff returned `APPROVE` with high confidence and
`REQUIRED_CHANGES: NONE`.  Its residual risks match this receipt: compiled Rust
parity, hosted Actions, live Sigstore/OIDC, registry publication, and real
G0-G10/IAR evidence remain `NOT_RUN` or `NOT_PROVEN`.

Frozen contract hashes were rechecked without editing the files:

- `docs/M1ND-10-PRD.md`:
  `00658cd88ce9dc5866f9b1fc6b9fbe594923e32fb900bde5bbc7740894c25c38`
- `docs/M1ND-10-UML.md`:
  `8a8a5fe9b9d2a4fc62c419e160e8dc2dcb4115f58d98f3f15a2d5031881dd32b`
