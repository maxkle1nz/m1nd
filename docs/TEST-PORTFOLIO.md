# The test portfolio — what each family proves, and in which lane

> The suite is a PORTFOLIO OF PROOFS, not a count of asserts. This manifest is
> the single place that says what each family promises, which lane runs it,
> and what its budget is. It was demanded by the suite-audit verdict
> (2026-08-02, askGOD CHANGE) and ratified with one standing rule: **reduction
> yes, pruning by count never** — a case leaves the portfolio only after the
> safe order below proves the promise is covered elsewhere.
>
> Derived from main at `94fff76f` (2026-08-02) — never from a stale tree. The
> audit's own numbers came from a checkout 41 commits behind and are recorded
> as direction, not denominators. Re-derive on this file's next revision.

## 0. Denominators (main `94fff76f`, measured 2026-08-02)

| Surface | Count |
|---|---|
| Rust lexical `#[test]`/`#[tokio::test]` | **2,604** in 234 files |
| — by crate | m1nd-mcp 1,770 · core 316 · ingest 318 · control 164 · runnerd 34 · openclaw 2 |
| — by location | inline `src/` 2,064 (79.3%) · `src/internal_tests/` 227 · `tests/` 313 |
| Integration-test targets | 47 |
| `#[ignore]` | 30 (four mixed natures — retag pending) |
| Doctests (all `compile_fail` sentinels) | 13 executed (16 lexical mentions; 3 are comments) |
| Python proof harnesses | 156 tests / 17 files (root `tests/`) |
| UI units | 656 declarations · browser fixture 33 · a11y 4 |
| Executed, lib suite (nextest, dev box) | 1,643 passed / 18 skipped / 376.7s |
| Executed, workspace (nextest, dev box) | 2,644 passed / 25 skipped / 966s |

Wall-clock across runners, same suite, step `Test every target` (2026-08-02):

| Leg | `cargo test` | nextest | Verdict |
|---|---|---|---|
| ubuntu (4 vCPU) | 62 min | 83 min | nested-Cargo process storm — **cargo test stays** |
| macos | 53 min | 52 min | flat |
| windows | 70 min | 48 min | nextest wins — awaits shadow evidence |
| dev box (M-series) | 641–935 s | **377 s** | **nextest canonical locally** |

## 1. The lanes (policy cut, 2026-09-25)

| Lane | When | Rust coverage | Decision |
|---|---|---|---|
| **per-edit** | local, before sharing an iteration | exact changed behavior/regression test; `scripts/lightning_check.sh` once per coherent batch, plus affected host tests | feedback only, never a release receipt |
| **draft PR** | every push while the PR is draft | lightning on Linux **and Windows**, workspace Clippy, doctests, lean feature check, formatting (40-minute job cap) | other CI gates still run; required `Test` deliberately stays red even when draft feedback passes |
| **final PR** | `ready_for_review` and subsequent pushes while ready | original `cargo test --locked --workspace --all-targets`, doctests, Clippy, fmt, lean edge on Linux, macOS **and Windows** (120-minute cap per leg) | required `Test` may pass only if every cumulative gate passes on this head |
| **merge queue / main** | queued ref / push to main | same full three-OS suite; release workspace build on main | protects the combined commit; no draft shortcut |
| **shadow** | manually dispatched workflow only | `cargo nextest --profile shadow` on the hosted runner | timing experiment, never a merge gate; no more automatic 80+-minute shadow on each main push |

Mark a PR ready only after its focused regressions are stable. Move it back
to draft before another burst of edits. `ready_for_review` triggers the full
run **without needing a new commit**. A draft's `Test` check is intentionally
failing, not secretly skipped or green; GitHub already forbids merging drafts,
and the required check fails closed as a second guard. The full suite remains
the proof boundary for merge and release, not a command to rerun per edit.

Change-to-test rule: choose the smallest **real** test that traverses the
changed behavior first, then lightning. For graph/lease/persistence changes,
exercise a populated, isolated runtime plus checkpoint/restart and ownership
invariants; for authority/security changes, refusal **and** success paths;
for platform-specific fs/process changes, a Windows proof; for UI changes,
unit, lint, reproducible bundle, fixture browser **and** separate accessibility
smoke; for host/cache changes, real native-process integration without skips.
None of those focused tests replaces the final three-OS gate. A timeout,
missing result, skipped job, or test under the wrong candidate is NOT PASS.

Do not push another iteration merely to rerun a monolith. Keep local commits
within a logical burst; run affected tests locally; push the batch once, while
still draft; make it ready only when there are no known P0/P1 issues. If full
CI finds a defect, move back to draft while repairing it. Never turn a failing
family into `#[ignore]`, a permissive timeout, or an unrequired advisory gate
to buy a green check.

## 2. The families

`class`: invariant (a) · contract (b) · implementation (c) · overlap (d) · manual/future (e).
`owner`: **Max** — the family's promise is a product promise; **resident** — mechanics of the suite itself.

| Family (selector sketch) | Class | Promise / incident it protects | Lane | Budget | Owner |
|---|---|---|---|---|---|
| lifecycle clean+crash (`persist_runtime_root`) | a | boot→serve→mutate→shutdown/crash→boot→still-serves; caught the 3 bricking boot bugs 1,458 units missed | lightning + merge | ~12s | Max |
| checkpoint corruption / fault injection (`checkpoint_store`) | a | recovery refuses lossy/non-current payloads | lightning + merge | ~5s | Max |
| snapshot/plasticity/bin continuity (`snapshot_bin_continuity`, plasticity roundtrips) | a | bytes written by version N boot version N+1 | lightning + merge | ~10s | Max |
| cross-brain isolation + authority fail-closed | a | writes never land in the wrong brain (real incident: foreign skeleton overwrote a bound brain) | lightning + merge | fast | Max |
| registry↔dispatch parity, REST seating, MCP top-level schema | b | advertised == routed; one bad schema once wiped every tool from a strict client | lightning + merge | fast | Max |
| 13 `compile_fail` doctests | b | the candidate boundary stays shut (2 rotted unrun until #505) | lightning + merge (own leg) | 0.5s | Max |
| lean edge (`--no-default-features` check) | b | feature unification once hid a real break | lightning + merge | ~20s | Max |
| frozen contract digests + docs coupling | b | PRD/UML immutability; agents taught in the same PR | merge | fast | Max |
| windows path/fs/process contracts | b | the phase-2 debt family; Unix legs are structurally blind | merge (windows leg) | — | Max |
| birth ceremony e2e (`first_graph_is_born`) | a | a virgin repo ends up with a POPULATED, CLEAN graph in all 3 layouts (2026-08-02: the ceremony was ingesting its own runtime — 32/39 junk nodes) | merge | ~12s | Max |
| runtime-exclusion gate (`ingest_excludes_runtime_state`) | b | the anti-aging validator: every state file a real session writes must be excluded from source walks (bit on run one: `antibodies.json.bak`) | lightning + merge | ~8s | resident |
| ingest grammars matrix (`m1nd-ingest` extractors) | b | one fixture per grammar parses; full matrix on merge | lightning (representatives) + merge (full) | — | Max |
| retrobuilder real+stress (16 real-repo ingests; 390–567s EACH on dev box) | a/d | retrieval works over THIS repo's real history | merge (capped ceiling 300s×8) → candidate nightly | ~21 min total | Max |
| transplant compiler oracles (~21 nested `cargo check`) | b/d | moved code still compiles; §6 fusion candidate (shared fixture workspace) | merge → candidate nightly | heavy | Max |
| 10k-operation brain stress (`project_brain_runtime`) | a/d | concurrency under load; smoke stays, full run is deferred evidence | merge → candidate nightly | ~300s | Max |
| proptest wide (`transplant_proptest` 61s, others) | a | property coverage; case count is the dial | merge; wide count nightly | 60s+ | Max |
| golden/type-shape batteries (test_v04, surgical, perspective_golden — 69 cases) | c/d | wire-shape contracts; several assert data the test itself builds — §6 fusion candidate | merge | fast | resident |
| UI units + a11y + fixture browser | b | separate proofs by ceremony law (a11y NEVER folds into e2e) | merge | — | Max |
| npm host monolith (3,021-line serial script) | c | install/update/rollback truth; needs decomposition into real cases (§6) | merge | — | resident |
| 30 `#[ignore]` | e | four mixed natures; retag to manual/future/bench/tooling pending | none | — | resident |

## 3. Never cut — the fifteen

Ratified 2026-08-02 (owner side: Paco, from the suite-audit verdict; the
histories live in `docs/PATHOS.md`). Canonical copy in `docs/MANUAL.md` §7 —
this table is the portfolio view. Removing or weakening any of these is an
owner decision with an amendment here, never a cleanup.

lifecycle clean+crash · checkpoint corruption/fault-injection ·
snapshot/plasticity/crypto/WAL continuity · frozen-bytes compatibility ·
cross-brain + authority fail-closed · registry/dispatch parity · REST route
seating · MCP top-level schema · windows path/fs/process · the 13
`compile_fail` doctests · lean `default-features=false` · docs coupling ·
a11y as a separate proof · eslint actually executed · `m1nd-ui/dist` drift.

## 4. The lightning lane (draft feedback, never merge proof)

Ratified 2026-08-02 (Paco) with one non-negotiable design condition, built
and measured the same day. The draft-feedback policy above now puts this
selector on CI; its explicit exclusions remain visible on every local run.

- Selector pinned as `[profile.lightning]` in `.config/nextest.toml`; the
  command is `scripts/lightning_check.sh`, which adds the two proofs nextest
  cannot carry (the 13 `compile_fail` sentinels, the lean
  `no-default-features` check).
- **Measured on the dev box: 57s hot** (82 selected tests of 2,674 — the
  never-cut core is ~3% of the suite; 16.3s nextest + 0.36s doctests + lean
  check + incremental overhead). Warm-after-branch-switch: 190s. Ceiling: 180s
  hot, per ratification.
- **The design condition, verbatim in mechanism:** the script prints on EVERY
  run that it is not the merge gate and exactly what it does not prove; the
  merge gate remains the full suite on three OSes, unchanged; MANUAL §5
  carries the one-line cost statement with the pointer here.
- What it deliberately omits (all still on merge): retrobuilder over real
  history, transplant compiler oracles, 10k-op stress, wide proptests, the
  full grammar matrix, browser suites, every other OS.

## 5. Safe order (ratified; where we are)

1. ~~Week 1 observe~~ — local nextest prints per-test timing; the hosted
   `nextest-shadow` experiment is now manual, not a main-push tax.
2. **Manifest** — THIS FILE (first revision).
3. Lightning WITHOUT deleting anything — draft feedback only; final three-OS
   full suite unchanged.
4. Consolidations (§6 of the verdict): compiler-fixture workspace, shared
   retrobuilder graph fixture, sleeps→events, npm decomposition, ignore retag.
5. Controlled hosted shadow comparison before any runner promotion (MANUAL §5).
6. Mutation sampling / historical-bug reverts over families with no recorded bite.
7. Only then: delete proven redundancy, one family per PR, amendment here.

## Registry

| Date | Revision |
|---|---|
| 2026-08-02 | Manifest established at `94fff76f`, from the suite-audit verdict (CHANGE) confronted with resident measurements; lanes, families, the fifteen, and the lightning proposal recorded. Lightning NOT active. |
| 2026-09-25 | CI policy candidate: short draft feedback; full required on ready PR, merge queue and main. Shadow moved to manual. No family removed; exact candidate must pass before policy can be considered active. |
