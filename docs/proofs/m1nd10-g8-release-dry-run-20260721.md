# M1ND-10 — G8 release ceremony: armed and dry-rehearsed — 2026-07-21

> Status: **DRY RUN**. Nothing in this document publishes anything. No tag was cut, no registry
> was mutated, no workflow was dispatched, the served owner and port 1338 were never touched. This
> is the "G8 dry ceremony" named as the next mechanical step in
> `docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md#7`. It maps the exact ceremony, records what was
> rehearsed locally with numbers, classifies every real-release blocker, proposes the coordinated
> 1.5.0 bump (not applied), and gives the owner the exact trigger.
>
> Repo state at authoring: `origin/main` HEAD `3bd8ab6c` (advanced from `9e8a4d7c` during the
> session via two ratified G9 custody-docs commits), working tree clean, in sync with origin.

---

## 1. The exact ceremony (`.github/workflows/release.yml`, 15 jobs)

**Trigger.** `on: push: tags: v*` (canonical) or `workflow_dispatch` **against an existing `v*`
tag ref** — `tag-guard` hard-refuses any non-tag ref, so a plain dispatch on `main` fails
immediately (release.yml:44-50). The tag must point at the exact `origin/main` HEAD.

The DAG (who runs what, what each demands):

1. **`tag-guard`** — binds identity: `GITHUB_REF` must be `refs/tags/v<ver>`; `package.json`
   version == `<ver>`; `cargo metadata --locked` versions of `m1nd-core`/`m1nd-ingest`/`m1nd-mcp`
   == `<ver>`; `HEAD == tag commit == origin/main head`. Then runs the candidate **source guard**
   (`scripts/m1nd10_candidate_source_guard.py`), **gitleaks over the full history** (pinned
   8.30.1 + sha256), and the **public nonexistence probe**: GitHub release, npm
   `@maxkle1nz/m1nd@<ver>`, and each of the four crates must return 404 (any 200 → refuse; any
   non-404 → `NOT_PROVEN` → refuse). Outputs the four crate versions.
2. **`ui-artifact`** (needs tag-guard) — `npm ci && npm test && npm run lint:soft && npm run build`
   in `m1nd-ui/`, seals the tree with `scripts/m1nd10_ui_bundle.py create/verify`, emits the UI
   bundle sha256 (the sole UI input for every downstream job).
3. **`npm-artifact`** (needs tag-guard) — `npm pack --json`; asserts exactly one tarball named
   `@maxkle1nz/m1nd@<ver>`.
4. **`release-gate`** (needs tag-guard, ui-artifact) — verifies the sealed UI; verifies the
   **frozen ratified contract hashes** for `docs/M1ND-10-PRD.md` + `docs/M1ND-10-UML.md`; runs the
   full Rust gate `--locked` (check/test/clippy `-D warnings`/fmt/`build --release`); host
   `npm test` + `m1nd:pack-check` + `m1nd:pack-routing-check`; `python3 -m unittest discover -s
   tests -p 'test_*.py'`; and the **cross-language canonical vectors** (python and node must agree).
5. **`crate-artifact`** (needs tag-guard, ui-artifact, release-gate) — stages the sealed UI into
   `m1nd-mcp/`, `cargo package --locked` the four-crate overlay, `inspect`s each `.crate`.
6. **`build`** (needs tag-guard, ui-artifact, release-gate) — matrix build of `m1nd-mcp` for the
   four targets: `linux-x86_64`, `macos-x86_64` (macos-15-intel), `macos-aarch64`, `windows-x86_64`.
7. **`artifact-smoke`** (needs tag-guard, ui-artifact, build) — matrix; installs the archive,
   proves archive member == raw runtime by sha256, runs `m1nd10_release_artifact_smoke.py`.
8. **`candidate-assembly`** (needs 1,2,3,4,5,6,7) — downloads everything, SPDX SBOM, builds the
   non-circular updater/rollback inputs (`prepare-canonical-operational`), **assembles
   `CANDIDATE.json` + `GATE-RECEIPT.json` + `ROLLBACK.json`** (`assemble`), verifies, writes
   `SHA256SUMS`. Credential-less.
9. **`candidate`** (needs candidate-assembly; `id-token: write`, `attestations: write`) — GitHub
   **Sigstore** build-provenance + SBOM attestations, then **`cosign sign-blob`** keyless over
   every file → `*.sigstore.json`. This is where OIDC first appears; `CANDIDATE.json` + its
   signature is the **release_candidate_digest** that G9 references.
10. **`verified-update-smoke`** (needs tag-guard, candidate; matrix) — `cosign` install, then
    `node scripts/m1nd10_update_rollback_smoke.js` against the signed candidate on each target.
    **See §4c — this job is blocked as currently wired.**
11. **`release-verification`** (needs tag-guard, candidate, verified-update-smoke) — re-verifies
    candidate bytes + `SHA256SUMS` + `cosign verify-blob` (cert identity pinned to
    `release.yml@<ref>`, issuer `token.actions.githubusercontent.com`) + update receipts + npm
    binding.
12. **`crate-publish-verification`** (needs tag-guard, candidate, release-verification) — builds
    the **exact crates.io request bodies without registry credentials**, refuses foreign existing
    bytes, writes `CRATES-PUBLISH-PLAN.json`, and compiles every extracted crate against the others.
13. **`release`** (needs 1,9,10,11,12; **`environment: release`**, `contents: write`) — creates
    the immutable-byte **GitHub Release** (`softprops/action-gh-release`). **Mutation.**
14. **`publish`** (needs tag-guard, candidate, release, crate-publish-verification;
    **`environment: release`**, `permissions: {}`) — PUTs the four sealed bodies to
    `https://crates.io/api/v1/crates/new` in order core→control→ingest→mcp, using
    `secrets.CARGO_REGISTRY_TOKEN`, re-checking nonexistence/idempotency per crate. **Mutation.**
15. **`publish-npm`** (needs tag-guard, candidate, release, release-verification;
    **`environment: release`**, `id-token: write`) — `npm publish` the exact candidate tarball
    with `--provenance` using `secrets.NPM_TOKEN`. **Mutation.**

The three mutation jobs (13, 14, 15) are the only ones gated behind `environment: release` and are
the only ones that touch a registry.

---

## 2. What was rehearsed locally, with numbers (all GREEN except the one blocker)

Environment: node v22.22.3, python 3.14.6, npm 10.9.8, `actionlint` present; `cosign` **absent**.

| Rehearsal | Command | Result |
|---|---|---|
| Targeted release tests | `unittest tests.test_m1nd10_release_{authority,candidate,contract,artifact_smoke}` | **37** tests OK (4+16+12+5) |
| Crates.io + UI tests | `unittest tests.test_m1nd10_{crates_io_upload,ui_bundle}` | **9** tests OK (6+3) |
| Full release-gate suite | `python3 -m unittest discover -s tests -p 'test_*.py'` | **184** tests OK, 68.9s |
| Host CLI suite | `npm test` (`node --test npm/test/cli.test.js`) | PASS ~8.0s; 93 update/rollback/harness refs |
| npm pack | `npm pack --dry-run --json` | `@maxkle1nz/m1nd@1.4.0`, 27 files, unpacked 624630 B, packed 189535 B |
| Canonical vectors (py) | `m1nd10_release_candidate.py verify-canonical-vectors` | `STRUCTURALLY_VALID_NOT_CRYPTOGRAPHICALLY_VERIFIED` |
| Canonical vectors (node) | `verifyCanonicalReleaseVectors(...)` | `ok=true`, same status → **cross-language parity holds** |
| Host pack gates | `npm run m1nd:pack-check` / `pack-routing-check` | `m1nd agent pack ok` / `... routing ok` |
| Frozen contract hashes | sha256 of PRD + UML vs release-gate constants | both **OK** at HEAD `3bd8ab6c` |
| Workflow lint | `actionlint release.yml ci.yml` | clean (exit 0) |
| Rust format gate | `cargo fmt --all --check` | exit 0 |
| Signed update/rollback smoke | `node scripts/m1nd10_update_rollback_smoke.js …` | **REFUSED** — see §4c |

Not re-run locally (CI is the oracle; heavy and/or credentialed): the full `cargo
check/test/clippy/build --locked --release` matrix, the four-target native `build` +
`artifact-smoke`, Sigstore signing, and `candidate-assembly`'s `assemble` (there is **no dry/
synthetic mode** — `assemble` requires the real four-target artifacts + npm tgz + four crates +
smoke receipts; parser at `m1nd10_release_candidate.py:1330-1341`). The G9 `seal-canonical-*`
commands do expose a `--fixture-only` seam, but that is G9 canonical convergence, not the ordinary
G8 candidate, and still requires a real `--provenance-signature`.

---

## 3. The release_candidate_digest and where G8 sits

G8's `candidate` job produces `CANDIDATE.json` (schema `m1nd-release-candidate-v1`) and signs it
keyless. Its sha256, recorded in `SHA256SUMS` and bound by the `*.sigstore.json` bundle, **is** the
release_candidate_digest. Per the just-ratified custody decision, G6-formal, G7-complete, and
G8-signing all converge on G9; G8 is the step that mints the provenance the others consume
(`docs/M1ND-10-G9-CUSTODY-DECISION-20260721.md`, §4 table: "G7 complete … after G8 mints release
provenance"). The **ordinary G8 release** (this workflow) deliberately does not claim M1ND-10
convergence (release.yml:719); the **canonical G9 seal** is a separate rite that needs the
production authority assembly (Path-B providers, below).

---

## 4. Blockers to a real release, classified

### (a) Owner decision / custody (G9 / signing)
- **`environment: release` approval.** Jobs `release`/`publish`/`publish-npm` are gated behind the
  `release` GitHub Environment. Its protection rules (required reviewers) are a repo-**Settings**
  fact, **not verifiable from the tree** — the owner must confirm a human-approval reviewer is set.
- **The decision to cut the tag and authorize publication.** Owner-only, by design.
- **G9 production-authority custody.** DECIDED today: Amendment **G9-A1** ratified Path B (Secure
  Enclave single-host floor); Path A (multi-device) is the named successor
  (`docs/proofs/m1nd10-g9-a1-custody-floor-ratification-20260721.md`). The amendment explicitly
  **does not** authorize release publication or any gate promotion — each keeps its own receipt
  discipline. Path-B providers are ratified but **not yet implemented** (enclave signer wiring into
  `assemble_production_owner_authority_v1`, protected-root provisioning, quorum-seat minting).
  This gates the **G9 canonical convergence**, not the ordinary G8 GitHub/npm/crates release.

### (b) Secret / infra
- **`secrets.CARGO_REGISTRY_TOKEN`** (crates.io publish) — existence/validity is a Settings→Secrets
  fact, **NOT_PROVEN** from the tree.
- **`secrets.NPM_TOKEN`** (npm publish) — documented as configured in the repo and `~/.npmrc`
  (repo `CLAUDE.md`); its live validity is still Settings-side, treat as **PROVEN-by-doc /
  verify-before-fire**.
- **OIDC `id-token: write`** — needed by `candidate` (Sigstore) and `publish-npm` (provenance);
  the GitHub OIDC provider is default-available. Keyless `cosign` writes a public **Rekor/Fulcio**
  transparency entry — a real external side effect, which is exactly why it is **out of scope for a
  dry run** and cannot be pre-rehearsed without publishing signatures.
- **`cosign`** must be installed in every verifying/signing job (workflow uses
  `sigstore/cosign-installer`); it is absent on this dev host, which is why the signed smoke and
  `cosign verify-blob` steps are not locally reproducible.

### (c) Code to be done
- **`verified-update-smoke` is blocked as wired (reproduced locally).** The smoke drives the
  production CLI `m1nd.js update apply` with `M1ND_TEST_RELEASE_DIR` set in the child env
  (`scripts/m1nd10_update_rollback_smoke.js:161-186`). But `main` routes `update` →
  `selfUpdate` (`npm/lib/cli.js:4701-4702`), and `selfUpdate` **unconditionally refuses** ambient
  `M1ND_TEST_RELEASE_DIR`/`M1ND_TEST_COSIGN_PATH` with no CI exemption
  (`npm/lib/cli.js:4073-4086`). Local run reproduced the exact refusal:
  `unsafe self-update test overrides are not accepted by the production updater: M1ND_TEST_RELEASE_DIR`.
  Both the guard and the smoke's use of that env landed in the **same** freeze commit `70598733`
  and have **never run together** because the release workflow is NOT_RUN — a latent contradiction.
  The test seam lives only in `createSelfUpdateTestHarness()` (`cli.js:4092`, source-checkout only),
  which is what `npm test` exercises (and which passed). **Fix owed before G8 can go green:** route
  the smoke through the harness path, or add a narrowly-scoped, source-checkout-gated seam the
  production `update` command honors — proven by a failing-first test, per doctrine. Not fixed here
  (docs-only, dry-run, no gate loosening).

### (d) Ready, waiting for the trigger
- The entire credential-less spine (jobs 1-9, 11, 12) is armed and its source-side gates are proven
  green locally (§2): identity binding, source guard, gitleaks, frozen contracts, 184 python tests,
  host suite, npm pack, canonical vectors, pack gates, actionlint, fmt.
- The only content change required to arm the trigger is the coordinated **1.5.0** bump (§5).

---

## 5. Version parity and the 1.5.0 proposal (NOT applied)

Read-only parity probe (GET only, no mutation; `/release-parity` method):

| Source | Version |
|---|---|
| `m1nd-mcp/Cargo.toml` (local) | 1.4.0 |
| crates.io `m1nd-mcp` (max) | 1.4.0 |
| `package.json` | 1.4.0 |
| npm `@latest` | 1.4.0 |
| npm `@beta` | 0.9.0-beta.8 |

Verdict: local == crates.io == npm `@latest` == **1.4.0** → registries are in parity now, and the
`@latest` dist-tag correctly points at a stable (not a beta), so the historical npm-lag is **not**
present today. But **1.4.0 already exists in both registries** → releasing the unreleased era at
1.4.0 would be refused by `tag-guard`'s nonexistence probe (duplicate publication). **The bump to
1.5.0 is a hard prerequisite, not a preference.**

Coordinated edits the owner (or a follow-up session) must make — **listed, deliberately not
applied here**:

- `package.json`: `1.4.0` → `1.5.0`.
- `m1nd-core/Cargo.toml`: `1.4.0` → `1.5.0`.
- `m1nd-ingest/Cargo.toml`: `1.4.0` → `1.5.0`.
- `m1nd-mcp/Cargo.toml`: `1.4.0` → `1.5.0`.
- `Cargo.lock`: regenerate so the three workspace versions update (`cargo update -p m1nd-core -p
  m1nd-ingest -p m1nd-mcp --precise 1.5.0`, or a plain `cargo build`); `tag-guard` reads
  `cargo metadata --locked`, so the lock must match.
- `CHANGELOG.md`: retitle the populated `## [Unreleased]` section to `## [1.5.0] - 2026-07-21` and
  open a fresh `## [Unreleased]`.
- `m1nd-control/Cargo.toml`: **leave at `0.1.0`** — it did not exist at the `v1.4.0` tag
  (`git show v1.4.0:m1nd-control/Cargo.toml` → absent), so it is a brand-new crate whose `0.1.0` is
  unpublished and will pass the nonexistence probe on first publication. The gate only requires
  core/ingest/mcp to equal the release version (`tag-guard` allows `m1nd-control` to differ;
  release.yml:1013, 1121, 1390). Confirm any crates.io-published dependents pin a publishable
  `m1nd-control = "0.1.0"` rather than a path.
- `m1nd-ui/package.json` (`0.1.0`): internal input sealed into the UI bundle provenance; **no bump
  needed** for release parity.

After editing, re-run the local gate battery in §2 plus the full `cargo` gate, and re-verify the
frozen PRD/UML hashes are still OK (they are content-frozen and should be untouched by a version
bump).

---

## 6. The exact command the owner fires when authorizing

Nothing below is run in this session. When the owner authorizes the real release:

1. Land the 1.5.0 bump (§5) on `main` as `Max Kle1nz <kleinz@cosmophonix.com>`, English commit,
   and push so `origin/main` HEAD is that commit.
2. Cut the immutable tag at that exact HEAD and push it (this is the trigger):
   ```bash
   git tag -a v1.5.0 <bump-commit-sha> -m "m1nd 1.5.0"
   git push origin v1.5.0
   ```
   (`tag-guard` requires `tag == v<package.json version> == origin/main head`; do not force-push
   the tag — it must be immutable.) Alternatively, `workflow_dispatch` the Release workflow
   **selecting the `v1.5.0` tag ref**, never `main`.
3. When jobs `release`, `publish`, and `publish-npm` request review on the `release` Environment,
   **approve** them (the human custody gate). Order is enforced by `needs:`: GitHub Release first,
   then crates.io, then npm.
4. After the run, re-verify parity (`/release-parity`): crates.io max, npm `@latest`, and the tag
   must all read `1.5.0`.

**Prerequisite before step 2 is even worth firing:** resolve the §4c `verified-update-smoke`
blocker, or the run will fail at job 10 and never reach the mutation jobs.

---

## 7. Honesty ledger — NOT_RUN / NOT_PROVEN

- **NOT_RUN:** the real release workflow (tag cut, OIDC/Sigstore signing, four-target native
  build/smoke, GitHub Release, crates.io + npm publication, install-rollback on the four targets).
  Unchanged by this dry run — this document arms and rehearses only.
- **NOT_PROVEN (repo-side):** `secrets.CARGO_REGISTRY_TOKEN` presence/validity; the `release`
  Environment's required-reviewer protection; live validity of `NPM_TOKEN`. All are
  GitHub-Settings facts outside the tree.
- **NOT_PROVEN (crates.io breadth):** only `m1nd-mcp` max_version was read politely; a first
  anonymous burst was rate-limited by crates.io's data-access policy. `m1nd-core`/`m1nd-ingest`
  current versions were not individually re-read, but the workflow's own nonexistence probe is the
  authoritative gate at release time.
- **CODE-CERTAIN, CI-UNPROVEN:** the §4c smoke refusal is reproduced locally and grounded in the
  code (`cli.js:4073-4086`, no CI exemption); it is "unproven in CI" only because the workflow has
  never executed. Treat it as a real blocker.
- **GREEN and reusable:** everything in §2 marked OK — these are the standing proof that the
  source-side gates are ready for the trigger.
