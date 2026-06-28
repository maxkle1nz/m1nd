# PATHOS — m1nd agent handoff

> Read this first. Single source of truth for any chat / subagent / parallel
> session working on m1nd, so we don't re-derive state or contradict each other.
> Last checkpoint: 2026-06-28 ~06:15 CEST (call-graph improvement arc complete).

## North Star
m1nd = operational intelligence for coding agents. The bar: genuinely BEAT plain
`rg`/Read in the inner loop, measured honestly — not tie, not "feels useful".
Run a continuous, chained improvement engine: measure (battery) → fix+test the
real defect → checkpoint → seed the next cycle. Never sugarcoat results.

## Current State (2026-06-28)
- main has v1.0 + `focus` (attention runtime) + conformance-aware seek + the
  call-graph improvement arc below. Repo is branch-clean (a recent hygiene pass
  took it 74 → a handful of branches).
- **Honest baseline (battery, `scripts`-less harness in scratchpad):** on a FRESH
  graph m1nd is full_trust, embeddings active, no memory-noise in impact, no
  stale. `seek`/`activate`/`focus` beat `rg`; the defect was `impact`/`why`
  (call-graph). Overall 10/12.
- **Call-graph arc (3 merged cycles):**
  - #161 — Rust extractor emits function→function `calls` edges (enclosing-fn
    tracking + free-function calls), not file-sourced UpperCamelCase-only. `why`
    paths + forward callees now work.
  - #162 — `impact` ranks code symbols (fn/struct/enum) above containers
    (file/module), so callers surface instead of their files.
  - #163 — test functions are tagged at extraction (`#[cfg(test)]` + `#[test]`)
    and de-prioritized in `impact`, so the PRODUCTION caller surfaces above test
    callers.
  - **Result: impact 0/2 → 1/2; overall 10/12 → 11/12; impact stopped losing to
    grep.** (`#163` may still be finishing CI at this checkpoint.)

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
- gh active = `maxkle1nz`; git identity = Max Kle1nz <kleinz@cosmophonix.com>.

## Known Problems
- `impact`/`why` still can't follow value-receiver METHOD calls (`x.m()`) —
  needs receiver-type inference (e.g. `propagate` returns no callers). This is
  the #1 remaining call-graph gap.
- `#[cfg(all(test, …))]` compound predicates aren't tagged via the module path.
- Long agent sessions go stale if the auto-ingest daemon isn't started — it
  exists (notify watcher + per-tool-call `maybe_tick`) but is opt-in; `ingest`
  does not auto-start it (auto-freshness is a seeded fix).
- TypeScript/Python call-graphs are weaker than Rust's (the fix was Rust-only).
- Multi-session: an X-RAY/Codex guardian also touches this repo — `git fetch`
  before acting, confirm `git branch --show-current` before commit.

## Proof Standard
Done = `cargo test --workspace` green + clippy/fmt clean + the BATTERY shows the
targeted tool improved with a concrete example (e.g. `impact(reverse,
pack_to_budget)` ranks `handle_seek` above the `pack_to_budget_*` tests), zero
regression. CI green on 3 OSes before merge.

## Next Agent Prompt / cycle 4 seed
1. Receiver-type inference for method calls (`x.m()`) → flip `propagate`
   (impact 1/2 → 2/2). Hardest; design carefully.
2. Auto-freshness: `ingest` auto-starts the watcher (reuse `start()` /
   `start_watcher()` / `maybe_tick` server.rs dispatch), opt-out + robust + tests.
3. Call-graph for TypeScript (typescript.rs), then `#[cfg(all(test,…))]`.
Each cycle: measure → fix+test → update this file → seed the next.

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
