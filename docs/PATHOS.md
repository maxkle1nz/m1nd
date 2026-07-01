# PATHOS — m1nd agent handoff

> Read this first. Single source of truth for any chat / subagent / parallel
> session working on m1nd, so we don't re-derive state or contradict each other.
> Last checkpoint: 2026-07-01 (checkpoint 6 — OMEGA Move 1 (Trust-Gated Envelope,
> #195) SHIPPED + verified (calibrated weighting not AND, ships dark, 14 tests incl.
> the anti-AND); the cross-repo stress test proved m1nd generalizes across TS/Py/Rust
> and surfaced the gitignore fix #194; and the emerging **Ω+1 ambient-loop**
> direction — m1nd as the ambient nervous system of the agent loop — is now the fresh
> strategic front, folded into the PRD (`docs/NEXTGEN-AGENT-PRD.md § Ω+1`),
> critic-corrected. Prior: checkpoint 5 — OMEGA banner (#191) + Move 0 calibration
> harness (#192, first real number: act-band precision 28.3% on m1nd's own history).

## North Star
m1nd = operational intelligence for coding agents. The bar: genuinely BEAT plain
`rg`/Read in the inner loop, measured honestly — not tie, not "feels useful".
Run a continuous, chained improvement engine: measure (battery) → fix+test the
real defect → checkpoint → seed the next cycle. Never sugarcoat results.

## Current State (2026-07-01, checkpoint 6 — OMEGA Move 1 shipped, Ω+1 ambient loop is the fresh front)
- **OMEGA Move 1 — the Trust-Gated Envelope — SHIPPED + verified (PR #195).** Every
  answer can now ship as the §O.4.1 triple (**answer + map + trust verdict**). The gate
  is a **CALIBRATED WEIGHTING, not an any-red AND-fold** (§O.3 #1) — per-probe reliability
  weights + a risk budget, tuned before defaulting on, to avoid the ~23% spurious-abstention
  failure. **Ships DARK** until the calibrator certifies its precision-at-coverage. **14
  tests** incl. an explicit **anti-AND** test proving a single red probe does NOT force
  `abstain`. Verified on the REAL diff (not the report). **v1.2.0 = Move 0 + Move 1 —
  both now landed.**
- **Cross-repo stress test PASSED — m1nd generalizes beyond its own history.** The OMEGA
  verbs were run against real foreign repos across **TS / Python / Rust**, not just m1nd's
  own git history — the precondition before any verb defaults on. It surfaced and forced a
  **`.gitignore` fix (#194)** (a battery/harness artifact that would have mis-scoped ingest
  on foreign trees). Generalization is now evidenced, not assumed.
- **Ω+1 — the AMBIENT LOOP — is the emerging strategic front (new PRD section).**
  `docs/NEXTGEN-AGENT-PRD.md § Ω+1` frames the next chapter after OMEGA: m1nd stops being a
  **tool-you-call** and becomes **the wire the loop runs on** — `pre-orient → act →
  post-capture → compound`, wired into the Claude Code / Codex hook lifecycle
  (SessionStart/UserPromptSubmit/PreToolUse/PostToolUse/SubagentStop/Stop/PreCompact/
  SessionEnd) via a thin `stdin-JSON → MCP → additionalContext/permissionDecision` shim.
  Grounded in the frontier (EvoClaw: best agent >80% isolated → **38% continuous**; the
  collapse point IS failing to build on prior state). The moat shifts: leaving m1nd stops
  meaning "lose a feature" and starts meaning **"lose institutional memory."** Four critic
  corrections are BAKED IN (see Known Problems) — the doc models the honesty moat, does not
  present the design as flawless.
- **v1.1.0 is the released base.** main carries 1.1.0 (core/ingest/mcp + npm;
  openclaw stays 0.1.0); the tag + crates.io/npm publish landed at checkpoint 3.
  Everything below this is on top of v1.1.0 on main, UNRELEASED on crates/npm
  (no new tag — these merges are docs/feat, no version bump). **v1.2.0 = Move 0
  (calibration) + Move 1 (Envelope) — BOTH DONE as of checkpoint 6.**
- **m1nd-OMEGA is now the BANNER for the v1.2 → v2.0 era (PR #191, `docs`).**
  `docs/NEXTGEN-AGENT-PRD.md` §O.1–O.11 frames the vision: a **verifiable trust
  substrate** where every answer ships as a triple — **answer + map + trust verdict**
  (a re-derivable receipt over a code graph) — so agents mechanically decide reliance /
  when to stop / when to refuse. The critic's corrections are BAKED IN, not bolted on:
  - **Calibration is the keystone — consistency ≠ correctness.** Battery tests prove
    the code does what it says; the calibrator proves the verdict is *right often
    enough to act on*. OMEGA needs both, calibrator second (§O.6).
  - **The receipt is a CALIBRATED WEIGHTING, not an any-red AND-fold** (§O.3 #1).
    A naïve `any-red ⇒ abstain` over noisy probes ≈ 23% spurious abstention; agents
    learn to route around it and the moat dies. The gate must be a calibrated
    weighting tuned against ground truth before it defaults on.
  - **Honest novelty (§O.5):** taint = CodeQL, blast = Glean/Sourcegraph, quarantine =
    a Zep/Graphiti bi-temporal port — OMEGA's TRUE novelty is **answer + map + trust in
    one round-trip**, plus sufficiency/solvency economics, over a re-derivable receipt
    on a code graph. Framed as a port where it is one, not an invention.
  - **The poisoned-oracle threat model is an OPEN risk (§O.7):** a poisoned eval/
    co-change corpus makes the calibrator certify a wrong verdict — "who calibrates the
    calibrator?" — explicitly logged as unsolved, not papered over.
- **OMEGA Move 0 SHIPPED (PR #192, `570cb23`, `feat`) — a conformal calibration
  harness, and the FIRST real measured number.**
  - `m1nd-core/src/calibration.rs`: a `CalibrationTable` (clones the trust-ledger
    persistence pattern: atomic temp+rename, empty-on-absent) + `conformal_quantile`
    (hand-rolled split-conformal τ at risk α) + a `verdict_for` binner.
  - `calibrate_predict` harness: **date-splits the repo's OWN git history** — train-only
    `CoChangeMatrix` → score held-out commits → precision-at-coverage curve + conformal
    τ → persisted to `calibration_state.json`.
  - `predict` now emits a calibrated verdict **`act | reverify | abstain`** gated on
    that τ (`m1nd-mcp/src/tools.rs:2177`); uncalibrated ⇒ EVERY verdict is honestly
    `abstain`, never a fake-high `act`.
  - **THE FIRST REAL NUMBER (m1nd's own history):** 563 commits → 360 train / 203
    held-out, **9,825 scored predictions**, at α=0.10 → **τ=0.60, coverage 14.6%,
    act-band precision 28.3%.** HONEST: precision tops ~28% because predict's strength
    model is coarse (`0.1·N`) — calibration's JOB is to SURFACE that the model is weak,
    and it did. **Honesty invariant: never quote a band as a probability; uncalibrated
    ⇒ abstain.**
- **Battery 28/28, tracked in-repo (#183) — with a grounded TIE ANALYSIS.** The
  canonical capability battery (`scratchpad/m1nd_battery.py`) is committed and
  `.gitignore`-negation-protected (`!scratchpad/m1nd_battery.py`, line 52). Re-run:
  **28/28 on the m1nd suite, 16 wins / 12 ties / 0 grep-losses, embeddings active.**
  Grounded analysis of the win/tie line: it is dominated by an **`rg_lines>8` scoring
  artifact** — twin scan cases (identical capability, opposite verdict purely by grep
  pattern volume). Of the 12 ties: only ~2-3 are convertible by a REAL capability
  (canonical-definition-over-synonym semantic ranking), ~5 are the battery
  UNDER-crediting structural tools (trace/scan/am_i_stale/xray_orient measured against
  a meaningless grep), ~4 are honest permanent ties. The headline is real, but the
  win/tie ratio is partly a measurement artifact, not pure capability.
- **HONESTY NOTE (preserved):** an earlier "12/12" leaned on a LOOSE
  `impact_propagate` proxy (matched a mis-bound sibling). Hardening the harness caught
  it; the number that means anything is the **28-case battery with rigorous
  `xfile_*`/qualified assertions**, not the old headline.
- **Memory track moved while OMEGA was framed (checkpoint-4 → now):** Subsystem D
  moves #1-#2 SHIPPED — `Created`+`Source-Agent` stamped on every memorized claim
  (#187) and surfaced as authored-age + source-agent on recall (#189). Moves #3-#8
  (staleness/eviction/decay/consolidation) remain PENDING.
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
- **Ω+1 ambient-loop OPEN RISKS (the four critic corrections, baked into the PRD § Ω+1.3
  — this design is NOT shipped, it is a direction awaiting Max's green-light on hook
  install).**
  - **Hook latency must be MEASURED, not asserted.** Every PRE/POST hook is an MCP
    round-trip; a 12-edit refactor pays 12× `am_i_stale` + 12× `ghost_edges`→`predict`,
    and `orient` (PageRank, heaviest verb) must NOT fire blocking on `SessionStart`
    (esp. `compact`). Needs a per-hook latency budget, caching keyed to `graph_generation`
    for the "nothing changed" path, and fire-and-forget async on post-capture. "Sub-100ms"
    is a claim until benchmarked.
  - **The keystone is `Stop → cross_verify → memorize` DIRECTLY, NOT `mission_*`.** The
    synthesis's `Stop → mission_verify → mission_close → memorize` is structurally broken as
    a hook: `mission_verify`/`mission_close` require a `mission_id` and hard-error without
    one (`mission_handlers.rs:200,309,454,458`), but `Stop` fires on every turn end (almost
    none with an open mission). `memorize` takes free-form claims + evidence and needs no
    `mission_id` (`light_author_handlers.rs`), so it is the composable keystone. `mission_*`
    is reserved for `SubagentStop` / genuinely-open missions.
  - **Auto-memorize fabrication risk (the one honesty soft spot).** The distiller feeding
    `Stop:memorize` must anchor claims to resolvable `evidence` code paths, NOT
    free-LLM-summarize the turn (which could fabricate a memory and persist it with
    authority). `memorize`'s resolve-or-flag gate rejects unresolved evidence; the
    extraction step is where the guard must hold.
  - **Calibration auto-trigger is MISSING.** `predict`/co-change is honestly `abstain` on an
    uncalibrated graph (Move 0) and never earns its way on unless `calibrate_predict` runs
    against the repo's git history automatically. Without the auto-trigger, the co-change
    nudge ships silent and stays silent forever. Also: `am_i_stale → ask` must be CAUTION by
    default (block only on a file THIS agent read this session) — a hard `ask` on every hash
    mismatch cries wolf on formatter/branch-switch/sibling-session churn (our own documented
    multi-session worktree drift).
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
- **`predict`'s signal is COARSE — calibration just measured how coarse (Move 0).**
  Predict's strength model is `0.1·N` (linear in co-change neighbor count); calibrated
  against m1nd's own history it tops out at **~28.3% act-band precision** (τ=0.60,
  14.6% coverage). The calibrator is honest — `act` is structurally withheld until the
  number clears a risk budget — but the underlying strength model needs a real upgrade
  (proper conformal score / learned weighting) before `predict` can `act` at useful
  coverage. Calibration's job was to surface this; it did.
- **Battery `rg_lines>8` scoring artifact (5 ties under-credit structural tools).**
  The win/tie verdict partly tracks grep PATTERN VOLUME, not capability — twin scan
  cases get opposite verdicts purely by how many lines `rg` printed, and trace/scan/
  am_i_stale/xray_orient lose "ties" to a meaningless grep. Fix = score by ANSWER
  correctness, not match-line count. (Of 12 ties: ~5 are this artifact, ~2-3 are real
  convertible capability, ~4 honest permanent ties.)
- **Poisoned-oracle threat model is OPEN (OMEGA §O.7).** A poisoned eval set or
  co-change corpus makes Move 0's calibrator certify a WRONG verdict with confidence —
  "who calibrates the calibrator?". A self-consistent-but-false receipt is the
  "consistent ≠ correct" failure weaponized. Logged as unsolved; eval-set integrity is
  a hard prerequisite before any verb defaults on.
- **OMEGA memory roadmap moves #3-#8 still PENDING** (staleness/eviction/decay/
  supersession/consolidation; cross-cutting concurrency-lock + missing-`Created`
  eviction-exemption + audit-observability risks must be designed in — see PRD § D).
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
**For OMEGA verbs, "calibration-gated" now JOINS battery-gated (§O.6).** Battery
tests prove the code does what it says (consistency); the calibrator proves the
verdict is right often enough to act on (correctness-at-coverage). A verb earns `act`
as an allowed output ONLY when measured precision-at-coverage clears the stated risk
budget — until then `act` is structurally withheld and the verb emits
`reverify`/`abstain`/`unprovable`. Both gates, in that order. Recalibration, not
retraining: the number is re-measured against ground truth, never asserted in a README.
Engine cadence (proof-grown): delegate each move to a worktree-isolated Opus subagent
with a tight source-grounded spec + battery gate → the orchestrator verifies the gate
+ battery itself on the REAL diff (targeted re-run, never trust the report) →
commit/PR/auto-merge as Max → the UNIVERSAL DOC GATE (docs/wiki/README/PATHOS current
before "done" — now a standing CLAUDE.md rule) → seed the next move.

## Next Agent Prompt / next seeds

**→ ACTIVE NORTH IS m1nd-OMEGA: `docs/NEXTGEN-AGENT-PRD.md` §O.10 (the ranked OMEGA
roadmap, calibration-gated, reuse-first, honest).** Read §O.1–O.11 first — the vision
(answer + map + trust receipt), the calibration keystone, the critic corrections baked
in, and the open poisoned-oracle risk. The verb plan: each OMEGA verb is a thin
composer over shipping tools (a fan-out + dedupe over `audit`/`orient`-style chaining),
ships DARK, and earns `act` only once Move 0's calibrator certifies it.

**OMEGA roadmap (§O.10) — Move 0 + Move 1 DONE, Move 2 next; Ω+1 is the fresh front:**
- **Move 0 — conformal calibration harness (keystone).** ✅ **DONE (PR #192).** First
  real number measured (predict: 28.3% act-precision @ 14.6% coverage). Gates `act`
  for every verb after it.
- **Move 1 — the Trust-Gated Envelope (`envelope` / the §O.4.1 triple).** ✅ **DONE +
  VERIFIED (PR #195).** answer + map + trust in one round-trip. **Gate = CALIBRATED
  WEIGHTING, not an any-red AND-fold** (§O.3 #1); ships DARK until the calibrator
  certifies it; **14 tests incl. the anti-AND**. Cross-repo stress test PASSED (TS/Py/Rust)
  and surfaced the gitignore fix **#194**. **v1.2.0 = Move 0 + Move 1, both landed.**
- **Move 2 — NEXT: Solvency & Stop Gate (`solvency`).** Arbiter over `focus` sufficiency
  + `coverage_session` + a **real token-budget signal** (the §O.3 #4 unit-mismatch fix —
  `budget_consumed`/`relevance_clearing_total`/`coverage_session` do NOT subtract; wire a
  true token budget or build it net-new) + `am_i_stale`. Then Moves 3–9 (§O.10), each
  calibration-gated, composer-over-shipping-tools, degrading to UNPROVABLE not a fake green.

**→ FRESH STRATEGIC FRONT — Ω+1, THE AMBIENT LOOP (`docs/NEXTGEN-AGENT-PRD.md § Ω+1`).**
The next chapter after OMEGA: m1nd as the ambient nervous system of the agent loop
(`pre-orient → act → post-capture → compound`), wired into the hook lifecycle. The PRD
section is written and critic-corrected (four load-bearing corrections in § Ω+1.3). **It
AWAITS Max's green-light before any wiring — installing hooks into the live Claude Code /
Codex environment CHANGES Max's workflow, so it is a decision, not an autonomous ship.**
The reuse-first Wave roadmap (§ Ω+1.4) is ordered ORGANIZE-first (Waves 1–3 nearly free),
rewired keystone (Wave 4: `Stop → cross_verify → memorize` directly), swarm last.

**AUTONOMOUS OVERNIGHT MANDATE (2026-06-30):** run the OMEGA roadmap (PRD §O.10) to
completion — tested + cross-repo stress-tested — UNATTENDED; stop only when complete.
Honor the universal doc gate at each move (docs/PATHOS current before "done").

**MEMORY track (Subsystem D) — moves #1-#2 SHIPPED, #3-#8 PENDING.** `Created`+
`Source-Agent` are now stamped (#187) and surfaced on recall as authored-age +
source-agent (#189). Remaining order: age/disuse staleness, supersession-on-rewrite,
the `activate_temporal` decay fix, ranked+capped auto-load, reinforce-on-use, daemon
consolidate (full detail + gaps G1-G6 + critic corrections in the PRD § D).
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
