# PATHOS — m1nd agent handoff

> Read this first. Single source of truth for any chat / subagent / parallel
> session working on m1nd, so we don't re-derive state or contradict each other.
> Last checkpoint: 2026-06-28 ~10:15 CEST (checkpoint 2 — 8 cycles merged, battery
> hardened 12 → 20 @ 20/20, node-id + cross-file resolution-correctness fixes).

## North Star
m1nd = operational intelligence for coding agents. The bar: genuinely BEAT plain
`rg`/Read in the inner loop, measured honestly — not tie, not "feels useful".
Run a continuous, chained improvement engine: measure (battery) → fix+test the
real defect → checkpoint → seed the next cycle. Never sugarcoat results.

## Current State (2026-06-28, checkpoint 2)
- main has v1.0 + `focus` (attention runtime) + conformance-aware seek + a full
  call-graph + resolution-correctness arc (8 cycles merged this session). Repo is
  branch-clean.
- **Honest battery** (`scratchpad/m1nd_battery.py`, fresh ingest + ground-truth
  PASS/FAIL + `rg` head-to-head): hardened 12 → **20 cases @ 20/20**, 0
  grep-losses. Now covers cross-file binding CORRECTNESS, not just retrieval.
- **Call-graph + node-id arc (merged #161-#163, #165, #166, #168, #169):**
  - #161/#165/#166 — Rust+TS function→function `calls` edges (enclosing-fn
    tracking, free calls, lowercase-receiver method calls).
  - #162/#163 — `impact` ranks code symbols above containers; test fns tagged +
    de-prioritized so production callers surface.
  - #168/#169 — function node ids get a `#N` disambiguator on same-name-in-file
    collisions (Rust, then TS/Java/Go/Python via shared `unique_node_id`). Fixed
    ~6.3% of functions being silently dropped from the graph (add_node returns
    DuplicateNode → loader drops the sibling).
- **Resolution correctness (merged #167, #170):**
  - #167 — `scan.total_matches_validated` counted survivors, not the display
    `limit` (it was fabricating the raw-vs-validated delta).
  - #170 — `proximity_score` splits ids on `/` too + a SAME_FILE_BONUS, so
    `calls` resolve same-file > same-dir > cross-crate (fixed ~104 mis-bound
    `resolve` edges; battery 17/20 → 20/20).
- **HONESTY NOTE:** an earlier "12/12" rested partly on a LOOSE `impact_propagate`
  proxy (`expect="propagate"` matched a mis-bound sibling `propagate#N`). The
  node-id fix is real (proven by node count), but that battery assertion was weak;
  it's been re-aligned to an honest bar and real cross-file correctness now lives
  in the rigorous `xfile_*` cases. Hardening the harness is what caught this —
  trust the 20-case battery, not the old headline number.

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
- Battery is now 20 cases (JSON key `records`, each row has `m1nd_pass`); per-case
  `check=lambda res,q,c:` hook + `has_direct_calls_edge` helper enable structural
  cross-file assertions via the live client.

## Known Problems
- **Same-name resolution still needs qualifier/type info — the #1 remaining gap.**
  `proximity` now handles same-file + (same-dir vs cross-crate), but it CANNOT
  break: (a) same-DIRECTORY same-name ties (the 4 `propagate` impls in
  activation.rs; `plan_refactoring`'s `detect` binds to temporal.rs not
  topology.rs), and (b) cross-crate calls whose correct target is in ANOTHER crate
  (`graph.strings.resolve` → m1nd-core, not the same-dir resolve.rs). Both need the
  extractor to preserve the call qualifier (`crate::topology::Detector`) or the
  receiver type — the type-inference problem; design a dedicated cycle.
- `generic.rs` extractor still has the same id-collision (others fixed in #169).
- Method-call EDGES exist for Rust (#166) but not TS/Java/Go/Python.
- `scan.mitigated` status is ~unreachable at default `severity_min=0.3` (7/8
  patterns have base×0.4 < 0.3) — a tuning/product question, not a code bug.
- `#[cfg(all(test, …))]` compound predicates aren't tagged via the module path.
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
1. **Qualifier/type-aware resolution** (the hard, high-value fix): preserve the
   call qualifier/receiver in the extractor so same-dir same-name + cross-crate
   ties resolve correctly (would flip `propagate`, `detect`, cross-crate
   `resolve`). Design-first with a safety valve; battery-gate with a new same-dir
   tie case (e.g. `plan_refactoring`'s `detect` → topology.rs not temporal.rs).
2. Keep HARDENING the harness on under-measured tools (trace/scan/xray/perception)
   then hunt+fix the next real defect — the proven loop: harden → measure →
   fix+test. (A frente is doing this now as of this checkpoint.)
3. `generic.rs` id-collision (mirror `unique_node_id`); method-call edges for
   TS/Java/Go/Python; auto-freshness (opt-out, robust, tested).
Each cycle: measure → fix+test → update this file → seed the next. Run parallel
worktree-isolated Opus frentes on non-overlapping surfaces when aggressive (only
ONE frente edits the shared battery at a time).

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
