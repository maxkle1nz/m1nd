# PATHOS — m1nd agent handoff

> Read this first. Single source of truth for any chat / subagent / parallel
> session working on m1nd, so we don't re-derive state or contradict each other.
> Last checkpoint: 2026-06-30 (checkpoint 4 — NEXTGEN roadmap moves #1-#3 shipped:
> GC once-per-sweep + cold-start trust band + graph-closure verdict; battery
> re-armed in-repo @ 28/28; the memory frontier, Subsystem D, is now active).

## North Star
m1nd = operational intelligence for coding agents. The bar: genuinely BEAT plain
`rg`/Read in the inner loop, measured honestly — not tie, not "feels useful".
Run a continuous, chained improvement engine: measure (battery) → fix+test the
real defect → checkpoint → seed the next cycle. Never sugarcoat results.

## Current State (2026-06-30, checkpoint 4 — v1.1.0 + NEXTGEN moves #1-#3)
- **v1.1.0 is the released base.** main carries 1.1.0 (core/ingest/mcp + npm;
  openclaw stays 0.1.0); the tag + crates.io/npm publish landed at checkpoint 3.
  Everything below this is on top of v1.1.0 on main, UNRELEASED on crates/npm
  (no version bump tonight — all five merges are perf/fix/feat/docs, no new tag).
- **NEXTGEN roadmap code moves #1-#3 SHIPPED tonight (all merged to main):**
  - **Move #1 — GC reads the OS process table ONCE per sweep (#181, `perf`).**
    `is_pid_live` no longer re-reads the process table per registry entry; a single
    `LivePids::snapshot` (`m1nd-mcp/src/instance_registry.rs`, via `sysinfo` 0.39,
    no subprocess) is built once per sweep and reused. Gated, zero behavior change.
    Built on the #178 boot-time dead-lease GC.
  - **Move #2 — killed the cold-start `0.5`/`Unknown` trust LIE (#182, `fix`).**
    On the no-evidence path the bare numeric trust is now ABSENT; every agent-facing
    surface carries `trust_band: "insufficient_evidence"` instead of a fake-confident
    0.5/Unknown. Symbol: `m1nd_core::trust::trust_band` (`m1nd-core/src/trust.rs:78`).
    Highest honesty-per-line move.
  - **Move #3 — graph-closure verdict on `why` (#185, `feat`).** `why` now emits
    `closure: {state: closed|blocked, dangling_edges, why}` so an answer resting on
    an unresolved/ambiguous edge READS as blocked. Provenance tags
    (`m1nd:edge:ambiguous` / `m1nd:edge:unresolved`) are recorded ADDITIVELY at the
    `resolve.rs` fallback/drop sites (the binding itself is unchanged). Logic in
    `closure_reason_for_source` (`m1nd-mcp/src/tools.rs:1688`). Keystone moat extension.
- **Battery re-armed and TRACKED in-repo (#183, `chore`).** The canonical capability
  battery (`scratchpad/m1nd_battery.py`, 34 cases = 28 m1nd + 6 ts) had silently
  VANISHED from disk (it was gitignored and died with the cleared scratchpads). It is
  now committed and protected by a `.gitignore` negation (`!scratchpad/m1nd_battery.py`,
  line 52) so it can't vanish again. Re-run is green: **28/28 on the m1nd suite,
  16 wins / 12 ties / 0 grep-losses, embeddings active.**
- **NEXTGEN PRD now carries the MEMORY frontier (#184, `docs`).** `docs/NEXTGEN-AGENT-PRD.md`
  gained **Subsystem D — Agent-Native Memory**: the deep-research roadmap (8 ranked,
  honesty-preserving, reuse-first moves, critic-approved GO) that fills the PRD's
  former open agent-memory gap. This is now the active research frontier.
- **HONESTY NOTE (preserved):** an earlier "12/12" leaned on a LOOSE
  `impact_propagate` proxy (matched a mis-bound sibling). Hardening the harness caught
  it; the number that means anything is the **28-case battery with rigorous
  `xfile_*`/qualified assertions**, not the old headline.
- **Still true from checkpoint 3 (the v1.1.0 arc):** `focus` attention runtime
  (#157/#158: goal-conditioned working set + honest `ignored` tail + answer-free
  `sufficiency`); Rust+TS function→function `calls` graph (#161-#163/#165/#166;
  `impact` ranks symbols > containers, production > test); node-id collision fixed
  across all 6 extractors (#168/#169/#173, `unique_node_id` `#N` disambiguator);
  cross-file resolution (#170/#175: `proximity_score` same-file > same-dir >
  cross-crate; qualified `Type::method()`/`module::func()` bind to the impl owner);
  scan honesty (#167/#172: `total_matches_validated` = survivors; `mitigated` visible).

## Operating Doctrine
Proof-grown: measure before claiming; verify subagent work yourself (re-run the
battery / a probe), never trust a report. Battery-gate risky core changes. Fix
AND test every defect. Commit+push always (PR → CI → merge as Max Kle1nz /
maxkle1nz). Never bypass branch protection (admin-merge is blocked by design).
Delegate deep changes to Opus subagents with a tight, source-grounded spec + a
battery gate; orchestrate + verify. Update this file at big checkpoints.

## Access Map
- Battery harness: `scratchpad/m1nd_battery.py` — **now TRACKED in-repo** (#183;
  protected by the `.gitignore` negation `!scratchpad/m1nd_battery.py` at line 52, so
  it survives scratchpad clears). Fresh ingest + ground-truth PASS/FAIL + `rg`
  head-to-head; 34 cases (28 m1nd + 6 ts), m1nd suite green at 28/28. Probes:
  `impact_probe.py`, `edge_proof.py`. Reports: `M1ND_BATTERY_REPORT.md`, `battery_FINAL.txt`.
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
- **Agent-memory subsystem gaps (verified in source by the critic — full detail in
  `docs/NEXTGEN-AGENT-PRD.md` § Subsystem D, gaps G1-G6).** A recalled memory cannot
  yet self-describe its epistemic standing: a memorized `.light.md` carries NO
  `created_at`/`last_used`/`source_agent` (the keystone field lands with move #1, in
  flight); staleness is **code-sha-only** (`cross_verify` keyed on cited code's hash —
  frozen-code reads "fresh forever", no age/disuse/contradiction signal); auto-load
  (`reload_agent_memory`) is **unranked and uncapped** (every claim ever written
  re-enters context every boot); re-memorizing a slug **silently overwrites** with no
  supersession history; and there is **no cross-agent sharing/attribution**. The decay
  kernels (`activate_temporal`, `domain.rs` half_lives, `trust.rs` recency-decay) all
  exist but don't touch stored memories.
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
Done = `cargo test --workspace` green + clippy `-D warnings` + `cargo fmt` clean +
the BATTERY (`scratchpad/m1nd_battery.py`, tracked) at **28/28** on the m1nd suite
showing the targeted tool improved with a concrete example (e.g. `impact(reverse,
pack_to_budget)` ranks `handle_seek` above the `pack_to_budget_*` tests), zero
regression. CI green on 3 OSes before merge.
Engine cadence (proof-grown): delegate deep changes to a worktree-isolated Opus
subagent with a tight source-grounded spec + battery gate → verify the REAL diff
yourself (targeted re-run, never trust the report) → PR with auto-merge as Max.

## Next Agent Prompt / next seeds

**→ NORTH IS `docs/NEXTGEN-AGENT-PRD.md` (#179, + Subsystem D #184)** — a deep-research
synthesis attacking m1nd's measured weak points agent-first, with a ranked
impact×tractability roadmap + permissive importable refs (sysinfo/fs4/lcov/git2 +
H2O/MemoryOS/Letta concepts). Read it first. (It carries an orchestrator grounding
correction: the research grounded on the STALE `~/m1nd`/beta.8 — re-verify every
file:line anchor against THIS tree, which is v1.1.0; the symbol is the contract.)

**Core (engine) roadmap — moves #1/#2/#3 DONE tonight:**
1. ~~GC scaling `is_pid_live` → `sysinfo` (A/P0)~~ **DONE #181.**
2. ~~Kill the cold-start `0.5`/Unknown trust lie → `insufficient_evidence` band (B/P1)~~ **DONE #182.**
3. ~~Graph-closure verdict, BLOCKED on dangling load-bearing edges (C/P1)~~ **DONE #185.**
4. **NEXT — Freshness signal for ReadOnly (A/P1).** → 5. auto-attach ladder = multi-agent-by-default (A/P2).
6. head/tail positioning + ambiguity non_claim, WP2 (C) · 7. risk churn/fanout/bus_factor (B) · 8. bounded working-set over existing heat (C) · 9. conformal abstention (C+B) · 10. Tier-2 learned models (deferred).

**MEMORY track — now ACTIVE (Subsystem D, #184). Keystone = move #1 IN FLIGHT.**
The agent-MEMORY research gap is closed by Subsystem D's 8 ranked moves (full detail +
gaps G1-G6 + critic corrections in the PRD). Order: keystone `Created`+`Source-Agent`
field first, then honest recall labeling, age/disuse staleness, supersession-on-rewrite,
the `activate_temporal` decay fix, ranked+capped auto-load, reinforce-on-use, daemon
consolidate.
- **Move #1 (keystone) IN FLIGHT on branch `l00p/mem-created-at`:** stamp `Created`
  (`unix_ts`) + `Source-Agent` (`agent_id`) frontmatter in `memorize`'s
  `render_light_markdown`, reusing `now_ms()` (`boot_memory_handlers.rs:116`); a legacy
  file with no `Created` must recall as "unknown age", never "fresh". ~6 lines, no
  migration. Unblocks moves 2/3/5/7.
- **Critic's cross-cutting memory risks (must design in before the related move ships):**
  - **Concurrency / locking on `agent-memory/` writes** — biggest unhandled correctness
    risk. `reload_agent_memory` + `memorize` both `fs::write` the same dir, and move #4's
    invalidate-and-keep is a read-modify-write with NO lock; two sessions on one slug can
    clobber. Collides directly with our known multi-session worktree drift — design write
    serialization (advisory file lock) before supersession ships.
  - **Missing-`Created` backfill before eviction** — move #3 sorts by `Created`, so every
    legacy `.light.md` sorts oldest and would be evicted first the moment a cap ships.
    Rule: missing-`Created` is EXEMPT from eviction (or floored to file mtime) until
    re-memorized.
  - **Observability for the forgetting/consolidation passes** — under verify-before-claim,
    moves #3/#8 need an audit emission (what got capped/merged + confidence deltas) or
    they are unprovable.

Each cycle: measure → fix+test → update THIS file → seed the next. Run parallel
worktree-isolated Opus frentes on non-overlapping surfaces; go SOLO if rate-limited
(parallel bursts tripped the API rate limit this session — serialize then).

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
