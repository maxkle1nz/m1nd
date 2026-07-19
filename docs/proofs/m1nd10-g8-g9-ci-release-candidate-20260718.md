# M1ND-10 G8/G9 CI and release-candidate checkpoint

Date: 2026-07-18  
Scope: CI/release control plane only; this is not a G8, G9, or G10 PASS receipt  
Frozen contracts changed: none

## Implemented

- CI expresses the complete Rust `check/test/clippy/fmt/release-build` gate on
  Ubuntu, macOS, and Windows, with `--locked`, all targets, denied warnings, and
  one aggregate required status.
- The npm host/update/rollback suite runs on the same three operating systems.
  UI unit/static/build/fixture-browser, Python proof harness, dependency audit,
  immutable-contract hashes, and documentation coupling are separate gates.
- Every referenced GitHub Action is pinned to an exact commit. Dependabot owns
  weekly GitHub Actions, Cargo, root npm, and UI npm update proposals.
- Release accepts only an existing `v*` tag whose version exactly matches both
  the npm wrapper and `m1nd-mcp`, at the exact tagged commit.
- Four runtime targets are built once: Linux x86-64, macOS x86-64, macOS
  AArch64, and Windows x86-64. No downstream job rebuilds those runtime bytes.
- Each extracted archive passes a real binary smoke: version/source identity,
  non-loopback refusal, authenticated health and manifest, unauthenticated API
  refusal, and stdio-to-HTTP attach initialization. Its receipt is included in
  the candidate digest.
- Candidate assembly refuses a missing target, a missing/non-PASS smoke
  receipt, wrong tag/version/commit, symlink/empty artifact, changed bytes, or
  incomplete SBOM. It emits a content-addressed manifest, gate receipt, explicit
  non-automatic rollback manifest, and portable SHA-256 inventory.
- The exact candidate files receive SPDX JSON, GitHub Sigstore build and SBOM
  attestations, and portable cosign keyless signature bundles before promotion.
  npm publication requests registry provenance. Publication credentials are
  mandatory and GitHub/npm/crates publication occurs only after candidate
  sealing and GitHub release promotion.

## Mechanical evidence run locally

- `actionlint .github/workflows/ci.yml .github/workflows/release.yml` — PASS.
- YAML parsing for CI, release, and Dependabot configuration — PASS.
- `python3 -m unittest tests.test_m1nd10_release_candidate -v` — 5 passed.
- `python3 -m unittest tests.test_m1nd10_release_artifact_smoke -v` — 3 passed.
- Current macOS AArch64 debug-binary smoke — PASS for all five assertions. This
  validates the harness, not a promoted release artifact.
- Root npm CLI/update/rollback suite — 1 file passed; agent pack check and
  routing check PASS; `npm pack --dry-run` produced 25 publish entries.
- UI `npm audit --audit-level=high --json` — zero vulnerabilities at every
  severity across 340 dependencies.
- Root Python proof discovery — 43 passed before the release-smoke unit module
  was added; the added module passes separately as 3/3.
- `git diff --check` over the checkpoint files — PASS.

## Explicitly not proven by this checkpoint

- No GitHub workflow was dispatched and no tag, package, release, commit, push,
  or PR was created. Linux/macOS Intel/Windows runner execution remains
  `NOT_RUN`; GitHub/Sigstore/npm/crates external publication remains `NOT_RUN`.
- The manifest is intentionally an intermediate release-control artifact. It
  does not yet bind the final h4nd shell/bundle, poold/reviewer/runner versions,
  cross-repo compatibility receipt, complete SafetyKernel/governance state, or
  all G0-G10 receipts required by `ReleaseCandidateManifestV1`.
- Fixture Playwright is still not live-browser proof. Candidate-installed live
  browser, real poold warm/cold spawn, update/rollback rehearsal, and cross-repo
  rollback order remain open.
- Pinned workflow source plus local validation is not supply-chain execution
  proof. The signatures, attestations, SBOM, and promotion identity become
  evidence only when the tag workflow runs and their exact bytes verify.
- This checkpoint does not activate `POLICY_AUTONOMOUS` or `FULL_AUTONOMY` and
  cannot issue an `AutonomyActivationReceiptV1`.
