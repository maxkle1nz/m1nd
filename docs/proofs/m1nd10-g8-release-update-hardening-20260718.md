# M1ND-10 G8 — verified release/update hardening proof (2026-07-18)

## Verdict

`IMPLEMENTED_LOCAL`; publication and hosted proof remain gated.

The G8 source path now refuses unauthenticated GitHub raw-runtime installation,
binds the selected runtime to a signed release candidate before any managed-target
effect, and makes rollback a digest-fenced closed phase machine. The release
workflow signs the candidate before exercising the public updater and requires
four post-sign updater receipts before promotion.

No live M1ND installation, tag, GitHub release, registry, commit, push, or host
binding was changed by this lane. No Cargo command was executed in this lane.

## Bound surfaces

- `.github/workflows/release.yml`
- `npm/lib/cli.js`
- `npm/test/cli.test.js`
- `scripts/m1nd10_release_candidate.py`
- `scripts/m1nd10_update_rollback_smoke.js`
- `tests/test_m1nd10_release_candidate.py`
- `README.md`
- `docs/PATHOS.md`
- `docs/wiki/src/changelog.md`
- this proof

The other dirty-worktree surfaces are outside G8 and were preserved.

## Threat closed: raw release asset without authentication

Before this change, the updater could install a raw GitHub release asset without
cryptographically binding those bytes to the repository release workflow.

The verified GitHub-release path now, in order:

1. resolves an exact semantic version, tag, mapped platform target, and asset;
2. obtains `CANDIDATE.json`, `CANDIDATE.json.sigstore.json`, and the mapped raw
   runtime from that exact release-tag URL (or an explicitly disclosed test-only
   local transport);
3. requires `cosign verify-blob` over the candidate with the exact certificate
   identity
   `https://github.com/maxkle1nz/m1nd/.github/workflows/release.yml@refs/tags/v<version>`
   and issuer `https://token.actions.githubusercontent.com`;
4. validates candidate schema, candidate id, version, tag, full commit, platform
   target, raw asset name, SHA-256, and byte size;
5. re-hashes the staged raw runtime and requires exact artifact/binding equality;
6. only then creates a backup, writes a journal, or replaces the managed target.

Missing `cosign`, candidate, bundle, raw bytes, wrong identity/issuer, schema,
version, tag, target, asset, digest, size, or candidate id refuses the install.
The legacy single-file raw-asset test bypass has zero remaining references.
Tests may replace only transport (`M1ND_TEST_RELEASE_DIR`) and the
verifier executable (`M1ND_TEST_COSIGN_PATH`); every proof discloses both and
states that such a fixture is not live GitHub/Sigstore evidence.

The ambient Cargo-registry fallback has been removed from the updater. A
production update now refuses `runtime-release-unavailable` unless it can bind
an exact signed GitHub candidate; it does not execute `cargo install`. Automatic
npm-package mutation is also fail-closed until a candidate-bound multi-surface
transaction and rollback rehearsal exist. Legacy Cargo-like journals remain
refused rather than being interpreted as verified updates.

## Threat closed: stale rollback overwriting newer runtime bytes

The v0 local journal now has exactly three accepted phases:
`prepared`, `installed`, and `rolled_back`. Unknown or phase-less legacy journals
are refused with no target, backup, or journal mutation.

| Journal phase | Current target digest | Result |
|---|---|---|
| `prepared` | pre-update digest | close journal as rolled back; target untouched |
| `prepared` | candidate digest | crash recovery restores exact backup/removes first install |
| `installed` | candidate digest | normal rollback restores exact backup/removes first install |
| `installed` | pre-update digest | interrupted-rollback recovery closes journal; target untouched |
| `rolled_back` | pre-update digest | idempotent no-op; journal byte-identical |
| any known phase | any other digest | refuse stale overwrite; all bytes unchanged |
| any unknown/legacy phase | any digest | refuse; all bytes unchanged |

Before replacement, the updater records and fsyncs the backup, atomically writes
`prepared`, and re-hashes the current target immediately before the atomic runtime
rename. After replacement it verifies the installed candidate digest before
atomically advancing to `installed`. Rollback validates the journal, requested
target, phase, current target digest, and backup digest before restoration.

`requires_host_rebind` is true only when apply or rollback actually replaced or
removed runtime bytes. Signature failures, stale-digest refusals, idempotent
rollback, and journal-only crash recovery report false.

## Release promotion topology

The release workflow now:

- guards exact `v<package-version>` tag/commit identity and frozen contracts;
- builds one archive plus one updater-facing raw runtime per supported target;
- executes the archived bytes and proves archive/raw digest equality;
- assembles a canonical content-addressed `CANDIDATE.json` binding all four
  runtimes, artifact-smoke receipts, and SPDX SBOM;
- signs the candidate and release files with GitHub OIDC/cosign bundles;
- after signing, runs the public apply/rollback/stale-refusal smoke on Linux x64,
  macOS x64, macOS arm64, and Windows x64 with real `cosign` on `PATH`;
- verifies the exact four updater receipts against the signed candidate before
  the GitHub Release job can promote bytes.

Post-sign updater receipts remain CI artifacts only. They gate promotion but are
deliberately outside the already-signed candidate and published release file set,
avoiding a self-referential candidate. `workflow_dispatch` from a branch is
deliberately rejected by the exact-tag guard.

## Negative and recovery batteries

The npm battery proves refusal with no target, journal, or backup effect for:

- tampered raw runtime;
- tampered candidate bytes;
- wrong certificate identity or OIDC issuer;
- missing candidate, bundle, or cosign executable;
- signed wrong schema, version, tag, target policy, or asset name.

It also proves current-target drift refusal in every accepted phase, unknown and
phase-less legacy refusal, tampered-backup refusal, prepared-update recovery,
interrupted-rollback recovery, first-install removal, byte-identical idempotent
rollback, deterministic verified-candidate journal digests, and explicit legacy
Cargo-journal refusal. The Python battery proves canonical Python/Node candidate-id
agreement, exact archive/raw/receipt binding, candidate file-set refusal, and
post-sign receipt binding/tamper refusal.

## Mechanical proof

| Gate | Result |
|---|---|
| `actionlint .github/workflows/release.yml` | `PASS` |
| `npm test` | `PASS` — 1 suite, 1/1 |
| `python3 -m unittest tests.test_m1nd10_release_candidate -v` | `PASS` — 11/11 |
| `node --check npm/lib/cli.js` | `PASS` |
| `node --check scripts/m1nd10_update_rollback_smoke.js` | `PASS` |
| `python3 -m py_compile scripts/m1nd10_release_candidate.py tests/test_m1nd10_release_candidate.py` | `PASS` |
| `npm run m1nd:pack-check` | `PASS` |
| `npm run m1nd:pack-routing-check` | `PASS` |
| `npm pack --dry-run --json` | `PASS` — 25 files |
| restricted tracked/untracked `git diff --check` | `PASS` |
| legacy bypass reference check | `PASS` — zero references |

The fake cosign executable belongs only to negative/contract unit fixtures. It
does not prove Fulcio/Rekor, GitHub OIDC issuance, or a live release download.

## Independent review

The preflight askGOD review returned `CHANGE` with high confidence. Its five
requirements were applied: remove the raw-asset bypass, define deterministic
removed Cargo fallback/legacy-journal treatment, disclose both test seams, document the operator
contract, and keep post-sign updater receipts CI-only rather than candidate
self-inputs.

The final askGOD `review/full` inspected the real tracked diff plus each untracked
G8 addition and returned `APPROVE` with high confidence and
`REQUIRED_CHANGES: NONE`. It independently re-ran the npm, Python, Node,
`actionlint`, and agent-pack gates. Its residual risks match the boundary above:
runtime version display matching remains secondary to candidate digest binding;
a post-`prepared` crash may intentionally leave a recoverable journal/backup;
macOS execution awaits the hosted smoke; test seams ship but are always disclosed;
and tag mutability remains repository authority.

The oracle's pre/post global `git status` differed only by the concurrent creation
of `m1nd-mcp/src/autonomy_manifest.rs`, confirmed by the parent lane as its G9
work. No G8 surface changed during the read-only review.

## Frozen-contract guard

- `docs/M1ND-10-PRD.md`:
  `00658cd88ce9dc5866f9b1fc6b9fbe594923e32fb900bde5bbc7740894c25c38`
- `docs/M1ND-10-UML.md`:
  `8a8a5fe9b9d2a4fc62c419e160e8dc2dcb4115f58d98f3f15a2d5031881dd32b`

Both hashes were re-measured unchanged after implementation.

## Honest boundary

`PROVEN_LOCAL`: JavaScript/Python contracts and batteries, negative no-effects
fixtures, journal recovery/refusal behavior, canonical candidate agreement,
workflow static validity, npm pack/routing, documentation, frozen hashes, and
restricted diff hygiene.

`NOT_RUN` / `NOT_PROVEN`: GitHub OIDC issuance, real Sigstore services, live
GitHub release download, all four hosted updater jobs, immutable tag promotion,
crates.io/npm publication, live installation replacement, and MCP host rebind.
Those claims remain blocked until a real tag workflow succeeds.

## Local diagnostic incident (preserved, not acted on)

An early superseded harness copied a signed macOS system executable into temporary
paths. Six orphan diagnostic processes remain in `UE` state with PPID 1:
`45474`, `48159`, `50747`, `52603`, `56468`, and `60132`. They were inspected
read-only and not signaled or killed by this lane. The corrected harness creates
its own disposable POSIX fixture instead. Clearing the six old processes may
require an OS reboot; they are not M1ND release/runtime processes.
