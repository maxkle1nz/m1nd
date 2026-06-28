# PATHOS — m1nd agent handoff

> Read this first. Single source of truth for any chat / subagent / parallel
> session working on m1nd, so we don't re-derive state or contradict each other.
> Last checkpoint: 2026-06-28 ~13:30 CEST (checkpoint 3 — v1.1.0 cut: focus + the
> full call-graph/resolution arc; honest battery 12 → 28 @ 28/28, 0 grep-losses).

## North Star
m1nd = operational intelligence for coding agents. The bar: genuinely BEAT plain
`rg`/Read in the inner loop, measured honestly — not tie, not "feels useful".
Run a continuous, chained improvement engine: measure (battery) → fix+test the
real defect → checkpoint → seed the next cycle. Never sugarcoat results.

## Current State (2026-06-28, checkpoint 3 — v1.1.0)
- **v1.1.0 cut.** main (`509bcf3`) bumped to 1.1.0 (core/ingest/mcp + npm;
  openclaw stays 0.1.0). Tag `v1.1.0` + crates.io/npm publish via `release.yml`
  fire once the merge-commit CI is green (in flight at this checkpoint) — until
  the tag lands, everything below is on main but UNRELEASED on crates/npm.
- **Honest battery** (`scratchpad/m1nd_battery.py`, fresh ingest + ground-truth
  PASS/FAIL + `rg` head-to-head): hardened 12 → **28 cases @ 28/28**, 0
  grep-losses. Covers cross-file binding CORRECTNESS + under-measured tools
  (trace/scan/xray/am_i_stale), not just retrieval.
- **`focus` attention runtime** (#157/#158): goal-conditioned working set + an
  honest `ignored` tail + an answer-free `sufficiency` signal; conformance-aware
  seek/focus bias ranking when an X-RAY manifest resolves.
- **Call graph (merged #161-#163, #165, #166):** Rust+TS function→function
  `calls` (enclosing-fn, free calls, lowercase-receiver methods); `impact` ranks
  code symbols above containers and production callers above test functions.
- **Node-id collision FIXED across all 6 extractors (#168/#169/#173):** ids carry
  a `#N` disambiguator on same-name-in-file collisions (shared `unique_node_id`).
  Was silently dropping ~6.3% of functions (add_node DuplicateNode → loader drop).
- **Cross-file resolution (#170/#175):** `proximity_score` prefers same-file >
  same-dir > cross-crate; `Type::method()` / `module::func()` calls bind to the
  impl owner via the call qualifier (`disambiguate_with_qualifier`) instead of an
  arbitrary same-name sibling. `impact`/`why` now bind correctly cross-file.
- **scan honesty (#167/#172):** `total_matches_validated` counts survivors (not
  the display limit); documented `mitigated` findings visible at default severity.
- **HONESTY NOTE:** an earlier "12/12" leaned on a LOOSE `impact_propagate` proxy
  (matched a mis-bound sibling). Hardening the harness caught it; the number that
  means anything is the **28-case battery with rigorous `xfile_*`/qualified
  assertions**, not the old headline.

## Operating Doctrine
Proof-grown: measure before claiming; verify subagent work yourself (re-run the
battery / a probe), never trust a report. Battery-gate risky core changes. Fix
AND test every defect. Commit+push always (PR → CI → merge as Max Kle1nz /
maxkle1nz). Never bypass branch protection (admin-merge is blocked by design).
Delegate deep changes to Opus subagents with a tight, source-grounded spec + a
battery gate; orchestrate + verify. Update this file at big checkpoints.

## Access Map
- Battery harness: `scratchpad/m1nd_battery.py` (fresh ingest + ground-truth
  PASS/FAIL + `rg` head-to-head). Probes: `impact_probe.py`, `edge_proof.py`.
  Reports: `M1ND_BATTERY_REPORT.md`, `battery_FINAL.txt`.
- MCP stdio client pattern: `scratchpad/focus_smoke.py` (Content-Length JSON-RPC).
- Build: `cargo build -p m1nd-mcp --bin m1nd-mcp` → `./target/debug/m1nd-mcp`.
- 360 vision: `docs/X360-RUNTIME-PRD.md`. Focus runtime: `docs/FOCUS-RUNTIME-PRD.md`.
- git identity = Max Kle1nz <kleinz@cosmophonix.com>. **gh GOTCHA: the active
  account silently flips to `velvetside` mid-session → `gh pr create` fails with
  "must be a collaborator". Run `gh auth switch --user maxkle1nz` before EVERY
  push/PR.**
- Battery is now 28 cases (JSON key `records`, each row has `m1nd_pass`); per-case
  `check=lambda res,q,c:` hook + `has_direct_calls_edge` helper enable structural
  cross-file assertions via the live client.

## Known Problems
- **`x.method()` receiver-variable resolution still needs type inference — the #1
  remaining gap.** #175 fixed QUALIFIED calls (`Type::method()`, `module::func()`)
  via the call qualifier, and #170 fixed cross-file proximity (same-file > same-dir
  > cross-crate). What's left: a bare `x.method()` on a local/field receiver
  carries NO qualifier in source, so same-name ties (the 4 `propagate` impls; a
  `detector.detect()` whose type only a `let` binding knows) still fall to
  proximity/`candidates[0]`. Real fix = receiver-type inference (track `let x: T`
  / field types / fn return types) — a dedicated, harder cycle. Cross-crate calls
  whose correct target is in ANOTHER crate (`graph.strings.resolve` → m1nd-core)
  are the same class.
- Method-call EDGES exist for Rust (#166) but not TS/Java/Go/Python.
- `scan.mitigated` is now visible at default `severity_min` (#172) — note this
  CHANGED default output (callers see `mitigated` findings; `false_positive` still
  suppressed). Revertible if undesired.
- `#[cfg(all(test, …))]` compound predicates aren't tagged via the module path.
- i18n READMEs (7 langs) + wiki lag the v1.1.0 README (`focus` not yet
  translated) — post-release follow-up.
- Auto-freshness: `ingest` doesn't auto-start the watcher (it exists —
  notify + `maybe_tick` — but is opt-in); seeded fix.
- Multi-session: an X-RAY/Codex guardian also touches this repo — `git fetch`
  before acting, confirm `git branch --show-current` before commit. `main` is held
  by the primary worktree (/Users/kle1nz/m1nd); do feature work in
  /Users/kle1nz/m1nd-night and run parallel frentes in isolated worktrees.

## Proof Standard
Done = `cargo test --workspace` green + clippy/fmt clean + the BATTERY shows the
targeted tool improved with a concrete example (e.g. `impact(reverse,
pack_to_budget)` ranks `handle_seek` above the `pack_to_budget_*` tests), zero
regression. CI green on 3 OSes before merge.

## Next Agent Prompt / next seeds
1. **Receiver-type inference for `x.method()`** (the hard, high-value fix): track
   local/field/return types so unqualified same-name method calls bind to the
   owner (resolves the `propagate` / `detector.detect()` ties #175 can't reach).
   Design-first with a safety valve; battery-gate with a new `x.method()` tie case.
2. **Method-call edges for TS/Java/Go/Python** (mirror Rust #166).
3. **Auto-freshness** (`ingest` auto-starts the watcher, opt-out, robust, tested).
4. Keep HARDENING the harness on still-under-measured tools (auto_ingest, coverage
   sessions) → hunt+fix the next real defect.
5. Post-release: regenerate the 7 i18n READMEs + wiki for v1.1.0 (`focus`).
Each cycle: measure → fix+test → update THIS file → seed the next. Run parallel
worktree-isolated Opus frentes on non-overlapping surfaces; go SOLO if
rate-limited (a parallel burst tripped the API rate limit this session).

## Do Not Do
- Don't edit/build m1nd source while a battery/subagent is building on the shared
  worktree (corrupts its measurement). Don't admin-merge / bypass branch
  protection. Don't claim a fix works without a battery re-run. Don't delete
  unmerged branches without patch-id proof.

## Open Questions
- Should auto-freshness default-on (watcher per ingest) or opt-in? (decide with a
  battery staleness scenario.)
- Does the impact symbol-first ranking want to differ by direction
  (reverse=callers vs forward=dependencies)?
