# M1ND-10 candidate source boundary — 2026-07-19

## Verdict

> **Superseded by independent review:** this document records the original local preflight and its
> passing commands. The later isolated Fugu review returned `CHANGE`/high confidence after
> reproducing case, credential/key, opaque-archive, and public-content bypasses. Therefore the
> source implementation and worktree projection rows below are historical test results, not a
> current fail-closed approval. See
> `docs/proofs/m1nd10-candidate-source-boundary-askgod-review-20260719.md`.

| Boundary | Verdict | Meaning |
|---|---|---|
| Source implementation | `CHANGE_REQUIRED` | Exact-tree architecture is wired, but enumerated path policy and public-content enforcement require correction. |
| Uncommitted worktree projection | `HISTORICAL_PASS` | The original 1,410-path projection passed the incomplete policy; it is not proof of the corrected boundary. |
| Immutable candidate | `NOT_PROVEN` | No candidate commit/tree object has been created or reviewed. |
| Hosted enforcement | `NOT_RUN` | The new CI/release jobs have not executed on GitHub. |
| Publication, install, activation | `NOT_RUN` | No external mutation was authorized or attempted. |

This receipt proves a local preflight boundary. It is not a release receipt, a blind-benchmark
result, or G10.

## Threat closed

The old source tree allowed a broad add or release commit to carry material that invalidated a
blind benchmark or polluted a public candidate:

- `operator-only` labels, answer keys, author reviews, and judge inputs;
- benchmark runner results;
- label-generating corpus builders and their oracle tests;
- local runner configuration or secret files;
- private-key file types;
- `node_modules`, Python/tool caches, `.l00p` execution state, TypeScript build state, logs, and
  stale generated wiki output;
- symlinks, gitlinks/submodules, non-regular Git entries, and blobs larger than 8 MiB.

`scripts/m1nd10_candidate_source_guard.py` now derives the exact tree from a commit with
`git ls-tree`, or models the current `git add -A` path projection without changing the real index.
It fails closed on those classes and emits a machine-readable verdict. The release tag guard and
the required CI security job invoke it against the exact `${GITHUB_SHA}`.

The M1ND-10 public corpus boundary remains deliberately narrower: public queries, manifests,
digests, and schemas are versionable; labels, outcomes, independent-review internals, and the
source that reconstructs labels stay operator-private. Existing private M1ND-10 files were not
opened, deleted, or copied during this work.

## Candidate hygiene changes

- `.gitignore` now protects root `node_modules`, all benchmark `operator-only` and
  `runner-results` trees, the three M1ND-10 label builders and their three oracle tests, `.l00p`,
  logs, and existing cache families.
- Fourteen legacy tracked operator artifacts were removed from the future tree without reading
  their content. They remain recoverable from Git history and are retired as held-out evidence.
- `.github/.DS_Store`, `.l00p-run.log`, both tracked `tsconfig.tsbuildinfo` files, and four tracked
  `.l00p` execution files were removed as generated/private state.
- The stale `docs/wiki-build` copy (58 tracked files, 4.3 MiB) was removed. The canonical mdBook
  source remains under `docs/wiki`; its configured build output is repository-root `wiki-build`.
- Thirteen Gitleaks generic-key false positives were verified as deterministic fixtures and
  annotated individually with `gitleaks:allow`; no broad rule was disabled.
- Gitleaks Action v2.3.9 is pinned by full action commit
  `ff98106e4c7b2bc287b24eaf42907196329070c7`, with scanner version 8.30.1 pinned in both CI and
  release.

Removing files from the future tree does not rewrite existing Git history. The retired legacy
answer keys must never be reused as held-out evidence. History rewriting or remote mutation was
not authorized and was not attempted.

## Local evidence

| Check | Result |
|---|---|
| Guard against old `HEAD` `b59a1c2a1454a83164dfb4d5640c6b005154d1ee` | Expected `REFUSED`: 80 violations in 1,344 tracked paths |
| Guard against current worktree projection | `PASS`: 1,410 paths, 0 violations, 8 MiB/blob ceiling |
| Gitleaks 8.30.1 over candidate-only temporary projection | `PASS`: 25.76 MB scanned, 0 findings, exit 0, under 2 s |
| Candidate-source and CI security contracts | `PASS`: 18 tests |
| Candidate-public Python regression | `PASS`: 142 repository + 60 benchmark tests |
| Rust regression | `PASS`: `m1nd-control` 134/134; `m1nd-mcp` 1,399 PASS/15 ignored |
| Ruff on new guard/tests | `PASS` |
| Cargo format | `PASS` |
| actionlint on CI/release | `PASS` |
| `git diff --check` | `PASS` |
| Frozen PRD hash | `00658cd88ce9dc5866f9b1fc6b9fbe594923e32fb900bde5bbc7740894c25c38` |
| Frozen UML hash | `8a8a5fe9b9d2a4fc62c419e160e8dc2dcb4115f58d98f3f15a2d5031881dd32b` |

The temporary scan copy was removed automatically. The served owner and port 1338 were not
contacted. The real Git index was not modified; nothing was staged, committed, pushed, tagged,
published, installed, or activated.

## Remaining gate

The next evidence step requires explicit Git authority: select and review the intended scope,
create one immutable commit/tree, rerun the source guard and Gitleaks on that exact identity, then
repeat the full aggregate matrix. Only that exact candidate may enter formal G4/G6/G7/G8/G9/G10
proof. Until then, `IMMUTABLE_CANDIDATE`, hosted enforcement, release custody, LIVE, activation,
and G10 remain `NOT_PROVEN`/`NOT_RUN`.
