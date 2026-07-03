# PATHOS — m1nd agent handoff

> Read this first. Single source of truth for any chat / subagent / parallel
> session working on m1nd, so we don't re-derive state or contradict each other.
> Last checkpoint: 2026-07-03 (**checkpoint 8.1 — v1.2.1 CUT: compounding live in the runtime**).
> v1.2.1 tagged on the merge commit, published to crates.io + npm, GitHub Release cut, and
> Max's live `:1338` owner rebuilt on the 1.2.1 binary — carrying the four field-triage fixes
> (#211 `north`×L1GHT recall = THE compounding fix; #212 temp-graph tempdir; #218 numeric
> `confidence`; #219 closure cry-wolf killed, ambiguous-blocked 9/11 → 0/11). The field-report
> mailbox now stands at **5 reports / 4 resolved / 1 residue** (the unresolved-tag granularity
> follow-up remains open). Prior checkpoint 8 below.
> Last checkpoint: 2026-07-02 (**checkpoint 8 — v1.2.0 CUT: the first OMEGA-era release**).
> Tagged `v1.2.0`, published to crates.io + npm, GitHub Release cut, and Max's live
> `:1338` owner upgraded to the 1.2.0 binary. The era content shipped since v1.1.0:
> OMEGA Moves 0 (calibration) + 1 (Envelope) + the Move-2 honest reframe; `north`
> pre-orient; memory moves #1-#6; the root-`.gitignore` fix (#194); smoothed-Jaccard
> co-change (#206, calibration-proven **+3pts** over raw counts); binary version+sha
> honesty (#205, `M1ND_EXPECTED_*`/`M1ND_STRICT_VERSION`); battery grown to **36/36**
> (#204); and the **agent-native MCP `initialize` instructions** (#208) — the loop now
> ships in the wire every host reads. This checkpoint also establishes the **UNIVERSAL
> FIELD-TELEMETRY DOCTRINE** (see below) with **4 seeded reports awaiting first triage**.
> Prior: checkpoint 7 — memory moves #3-#6 SHIPPED + the pre-flight A/B returned its
> first REAL verdict (north HELPS orientation 3/3 vs 0/3; compounding memory an HONEST
> NULL, blocked by process-per-hook → prerequisite is `--serve`/`--attach`, §Ω+1.3b).

## North Star
m1nd = operational intelligence for coding agents. The bar: genuinely BEAT plain
`rg`/Read in the inner loop, measured honestly — not tie, not "feels useful".
Run a continuous, chained improvement engine: measure (battery) → fix+test the
real defect → checkpoint → seed the next cycle. Never sugarcoat results.

## Current State (2026-07-03, checkpoint 8.1 — v1.2.1 CUT, compounding live)

**v1.2.1 IS RELEASED — and the compounding is now live in Max's runtime.** Tag `v1.2.1` on
the merge commit, `release.yml` published `m1nd-core` / `m1nd-ingest` / `m1nd-mcp` to crates.io
+ `@maxkle1nz/m1nd` to npm, a GitHub Release was cut, and Max's live `:1338` owner was rebuilt
on the tagged binary (`--version` → `1.2.1 (<sha>)`). The patch carries the four field-triage
fixes merged since the 1.2.0 tag: **#211 `north` composes L1GHT agent-memory recall — THE
compounding fix** (memorize once, recall through the front door thereafter); #212 temp-graph
sentinel → real tempdir; #218 numeric `confidence` coerced instead of rejected; **#219 closure
ambiguity tag fires only on genuine ties — the cry-wolf killed** (ambiguous-blocked 9/11 → 0/11,
honesty guard proven). Battery grew to **37/37**. `m1nd-openclaw` stays `0.1.0`.
**Field-report mailbox: 5 reports / 4 resolved / 1 residue** — the four fixes above closed their
reports; the residue is a follow-up on unresolved-tag granularity (still open). The live north
memory-recall proof (agent-memory surfacing through `north` on the running :1338 binary) was
captured at cut time — this checkpoint is where compounding stopped being an A/B hypothesis and
became a live runtime property.

**Human layer — Living Tree Slice 0 SHIPPED (2026-07-03, `feat/living-tree-slice0`).** The
first human surface landed: the served UI (`m1nd-ui`, `127.0.0.1:1337`) now opens on the
**Living Tree** — the familiar filetree EVOLVED, assembled from `/api/graph/snapshot` (never a
second fs view), decorated with calm trust dots (`insufficient_evidence` → iris violet), the
read-only **post-it** system (memories pinned via `grounded_in`, author + age absent-never-faked,
four states: fresh / aging / stale-flipped / violet-unknown), directory memory-count roll-ups,
the hover blast whisper (floor language), and the node drawer (rung 1, action-language verdict) —
with honest `needs_ingest`/degraded cold states. The force-directed map is demoted (not the front
door). This slice also retired the cyberpunk theme for the **SOFT PROOF** tokens and shipped the
CI-able **violet-quarantine lint** (violet is reserved for abstain/unknown, nothing else). The
INV honesty suite (23 tests) is fed by REAL captured envelopes (`m1nd-ui/src/__fixtures__/`),
dogfooded live against a `--serve` of m1nd's own graph; `cargo build -p m1nd-mcp` still compiles
the new embedded dist. Verdict/state renderer is read-only — humans read memory, agents write it.
Slices 1–3 (Pre-Flight Card, Honesty HUD + Change Preview, Project Brain) stay spec'd in
`docs/HUMAN-LAYER-PRD.md`. UI-only change; no engine/agent-surface behavior moved.

**Brand gates (pending 1.2.2):** **G1 (#221) CLOSED** — the per-response unmeasured `savings`/`tokens_saved`/`gaia` envelope was removed. **G1.5 (#234) CLOSED** (founder decision 2026-07-03, mailbox L16) — the opt-in `savings`/`report` unmeasured-claims surface is killed: `savings` removed entirely (tool + handler + `Savings*` types + `SavingsTracker`/`GlobalSavingsState` accounting; nothing writes `savings_state.json` anymore), `report` stripped of every tokens-saved/CO2 field but keeps its honest counts + heuristic hotspots — completing the beta.7 de-advertisement.

---

**v1.2.0 IS RELEASED.** Tag `v1.2.0` on the merge commit, `release.yml` published
`m1nd-core` / `m1nd-ingest` / `m1nd-mcp` to crates.io + `@maxkle1nz/m1nd` to npm (the
v1.1.0 precedent), a GitHub Release was cut with real era notes, and Max's live `:1338`
owner was rebuilt on the tagged binary (`--version` → `1.2.0 (<sha>)`). `m1nd-openclaw`
stays `0.1.0` (versioned independently).

**What the era shipped (v1.1.0 → v1.2.0), the honest ledger:**
- **OMEGA Move 0 — the calibration harness (keystone).** Conformal precision-at-coverage
  calibrator; `calibrate_predict` date-splits the repo's OWN git history; `predict` emits
  `act | reverify | abstain`, uncalibrated ⇒ honestly `abstain`. First real number on m1nd's
  own history: τ=0.60, coverage 14.6%, act-band precision 28.3% — the calibrator's JOB was to
  surface that the strength model is coarse, and it did.
- **OMEGA Move 1 — the Trust-Gated Envelope (#195).** Answer + map + trust verdict composed
  from `trust_selftest` × `cross_verify` × `am_i_stale` × `why`-closure × `mission_verify`.
- **OMEGA Move 2 — the HONEST REFRAME (#203).** The token-economics claim was DROPPED (m1nd
  has no token ledger and cannot source consumption); `stop_gate` stays **roadmap-only** — it
  is NOT referenced by the shipped instructions.
- **`north` pre-orient.** The composing front door (trust + focus/anchors + prior memory +
  sufficiency + one `next_move` + `honest_gaps`; `needs_ingest` is a real answer).
- **Memory moves #1-#6 DONE** — `Created`/`Source-Agent` provenance, authored-age + source on
  recall, `aged_out` age-staleness (#198), `activate_temporal` per-type decay fix (#199),
  supersession-on-rewrite + per-slug flock (#200), recency-capped auto-load (#201).
- **Root-`.gitignore` directory-entry fix (#194)** — ingest now honours root `.gitignore`
  directory entries (folded in with the cross-repo stress test).
- **Smoothed-Jaccard co-change (#206)** — `ghost_edges`/`predict` normalize coupling instead
  of raw co-commit counts; **calibration-proven +3 points** over raw.
- **Binary version-honesty (#205)** — `--version` = `1.2.0 (<sha>)`; `M1ND_EXPECTED_VERSION`/
  `M1ND_EXPECTED_SHA` (+ `M1ND_STRICT_VERSION`) detect and (strict) refuse a drifted binary.
- **Battery grown to 36/36 (#204)** — the OMEGA/memory surface (closure, trust_band,
  calibration, envelope, north, provenance/supersession/aged_out) is now covered; the m1nd
  suite passes 36/36 on the release binary.
- **Agent-native MCP `initialize` instructions (#208)** — the `M1ND_INSTRUCTIONS` string every
  host receives is now the operating loop itself: **pre-orient (`north`-first) → act on
  calibrated verdicts → post-capture (`memorize` + one field-telemetry signal)**. Every
  referenced verb was verified against the live dispatcher; roadmap-only `stop_gate` is NOT
  referenced. Replaces the old flat WORKFLOWS catalogue.

### THE UNIVERSAL FIELD-TELEMETRY DOCTRINE (new this checkpoint)
**Every agent, every repo, is a sensor.** When m1nd misbehaves during ANY mission — even on
another repo — the agent REPORTS, it does not fix: append **one JSON line** to the single
machine-global mailbox `~/.m1nd/field-reports.jsonl`
(`{ts,agent,repo,tool,class:"bug|honesty|friction|win",what,expected,snippet}`) and keep
working. **Report-never-fix mid-mission** is the rule — never detour into m1nd surgery while
on task; note the workaround, report, move on. The **`honesty` class is the most valuable**:
it is calibration ground truth (m1nd overclaimed — said fresh/closed/act and was wrong). When
retrieval was simply right/wrong, prefer the built-in `learn` verb (correct/wrong/partial).
**Triage closes the loop: every m1nd improvement session STARTS by sweeping the mailbox
(+ `seek` for field memories); a confirmed field bug becomes a battery case/test BEFORE the
fix.** The mailbox is **local-only — m1nd never phones home**; the roadmap `m1nd feedback`
verb (opt-in, redacted, one-click GitHub issue) is the ONLY path upstream, and always the
human's explicit call.

**Seeded reports triage status** (the improvement session sweeps these first):
1. **closure cry-wolf (AMBIGUOUS portion) = FIXED (field-triage #4).** The `m1nd:edge:ambiguous`
   tag fired on nearly every load-bearing path because it (a) tagged whenever a same-name fallback
   had >1 candidates even when proximity/qualifier picked a DECISIVE winner, and (b) was read
   node-level, so any node with ONE ambiguous edge poisoned EVERY clean path through it. Fix: tag
   only GENUINE coin-flips (decisive binds no longer tag), and read a targeted
   `m1nd:edge:ambiguous:<target>` tag PER-EDGE. Measured on the m1nd repo itself: ambiguous-blocked
   dropped **9/11 → 0/11** connected pairs; the honesty guard (a true 2-way tie still yields
   `blocked`+`ambiguous`) is proven end-to-end. NOTE: a SEPARATE node-granularity over-fire remains
   for the `unresolved` tag (explicitly out of scope — unresolved semantics unchanged); see Known
   Problems → "closure UNRESOLVED node-granularity".
2. **`memorize` unanchored on a live runtime** — evidence paths didn't anchor to code nodes
   against the running owner (friction/bug on the live `:1338` runtime).
3. **the `temp` artifact bug** — a stray `temp` file dropped in cwd by a battery/tool path
   (bug; the battery already `rm -f temp`s around it, but the source drop is unfixed).
4. **stale instructions = FIXED** — the old flat WORKFLOWS instructions string; closed by #208
   this checkpoint (kept as a `win`/closed marker for the triage sweep).
5. **north marker-fragment slot waste = FIXED (field-triage batch A, mailbox L28).** north's
   memory beat AND its anchor/focus surfaces spent slots on L1GHT MARKER FRAGMENTS — the
   annotation nodes the l1ght_adapter mints per marker line (`𝔻 confidence: …`, `𝔻 evidence: …`,
   `⟁ depends_on: …`, `⍂/⍐/⍌` declarations). Live founder SessionStart hook evidence: 2/5 memory
   slots + 4/4 anchor slots wasted on `𝔻 confidence: …` rows instead of real claims/code. Root
   cause: marker nodes inherit their file's `source_agent`/`authored_ms_ago` prov-tags, so the
   memory recall's provenance test kept them; and on a memory-heavy graph they rank into the
   PageRank/activation windows. Fix: a shared `is_marker_fragment(node_id, label)` — STRUCTURAL
   discriminator (the `::tag::` node-id segment the adapter stamps only on marker nodes; a leading
   marker glyph as the id-less fallback) — excludes markers from all four north surfaces
   (`memory`, `anchors`, `focus_nodes`, and the `memory_nearby` claim resolution). Markers stay
   in the graph (they are data); they just never take a slot. Red→green proven live and in-tree
   (a memorized note's markers leaked pre-fix, gone post-fix, real claim still recalls); battery
   case `north_recalls_memorized_claim` extended to assert no memory/anchor row is a marker.

### Runtime carry-over (post-upgrade honesty)
Max's live owner was upgraded to the 1.2.0 binary, so **any calibration rows scaled against the
OLD binary are now stale — re-run `calibrate_predict` once per ingested repo** on the live
runtime to refresh them (done at upgrade if the graph had an ingested repo; noted honestly if
the graph was empty).

## Current State (2026-07-01, checkpoint 7 — memory roadmap #3-#6 shipped + the pre-flight A/B returned its first real verdict)
- **MEMORY roadmap (PRD Subsystem D) — moves #3-#6 ALL SHIPPED + orchestrator-verified;
  so #1-#6 are DONE.** Four new moves landed this session, each TDD-proven and verified on
  the REAL diff (not the report):
  - **#198 — age-staleness (`aged_out`).** `cross_verify` evidence_freshness now emits an
    `aged_out` reason ORTHOGONAL to the code-sha `evidence_changed` signal: a memory older
    than `RECENCY_HALF_LIFE_HOURS` with no recent use flags stale even when its cited code is
    byte-identical (the "frozen-code reads fresh forever" gap). Missing-`Created` is EXEMPT.
  - **#199 — the `activate_temporal` decay fix.** `activate_temporal` now reads `DomainConfig`
    per-type half-lives, so the dead half-life table is LIVE — Module/Class decay slower than
    File. TDD-proven the table was genuinely dead before (the decay kernels existed but never
    touched stored memories; now they do).
  - **#200 — supersession-on-rewrite (invalidate-and-keep).** A same-slug rewrite no longer
    silently overwrites: it copies the prior `.light.md` to `agent-memory/.history/`
    (`State:outdated`) and writes the new one with a `Supersedes:` header — gated so a WEAKER
    claim can't clobber a stronger one. Serialized by a **per-slug `libc::flock` lock (no new
    dep)** — directly closing the multi-session-worktree-drift concurrency risk the critic
    flagged. The 2-thread concurrency flock test passes. This is the graph-native
    `[SHIPPED/histórico]`.
  - **#201 — recency-capped auto-load.** `reload_agent_memory` can now cap the auto-load by
    recency (default **unlimited = no-op**, opt-in via `M1ND_MEMORY_LOAD_CAP`); missing-`Created`
    is EXEMPT from eviction; a drop is observable. Kills the "every claim ever re-enters context
    every boot" unbounded growth, without changing default behavior.
- **THE PRE-FLIGHT A/B EXPERIMENT returned its first REAL verdict (isolated, honest).** An
  isolated A/B tested whether the `north` pre-flight packet (injected via a Claude Code hook)
  helps a real headless `claude -p` agent — full write-up folded into the PRD **§Ω+1.3b**.
  Isolation held (Max's global config untouched). Verdict:
  - **The wire EXISTS in headless.** Hooks FIRE in `claude -p` (proven with a canary); `north`
    is delivered to the model VERBATIM; ~110ms/fire; fail-open (never blocks a turn).
  - **HELPS orientation.** Arm B (north injected) opened the CORRECT file first — `config.py`,
    the `pr=1.00` anchor north pointed at — in **3/3 runs vs 0/3 for control**. Directional
    first-move retargeting is real.
  - **Does NOT confuse or hinder.** No wrong turns; when north suggested a tool the agent
    lacked, it IGNORED it without derailing (advisory, not over-obeyed). Mild cost: ~1.7 more
    tool-calls (more reading).
  - **HONEST NULL #1: both arms succeeded 3/3** — the task was too easy to show north
    *rescuing* a run. A harder "rescue" task is still untested.
  - **HONEST NULL #2 (load-bearing): COMPOUNDING MEMORY is architecturally BLOCKED.** north's
    graph is in-process + each hook fire is a separate short-lived process → a post-capture
    `memorize` writes to a graph the NEXT fire never reloads, and each fire re-ingests the repo
    (72ms here; would DOMINATE latency at scale). **The compounding loop CANNOT close in the
    process-per-hook setup.** → **THE INSIGHT:** the ambient loop's real prerequisite is NOT
    the hooks, it is the **`--serve`/`--attach` mode m1nd already has (#157/#158)** — a
    persistent live graph the hook ATTACHES to instead of re-ingesting. That kills per-fire
    ingest latency AND enables cross-fire compounding. **Recommendation: do NOT install hooks
    in the live env yet — wire the hook to a served m1nd FIRST, then re-test compounding +
    latency-at-scale + a harder rescue task.** This is now the FIRST ambient milestone.
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
- **v1.1.0 WAS the released base (superseded by checkpoint 8 → v1.2.0 is now
  released).** At checkpoint 7 main carried 1.1.0 (core/ingest/mcp + npm; openclaw
  0.1.0), with the OMEGA/memory era UNRELEASED on top. **Checkpoint 8 cut v1.2.0** —
  the tag + crates.io/npm publish + GitHub Release landed; see the checkpoint-8 Current
  State above. **v1.2.0 = Move 0 (calibration) + Move 1 (Envelope) + the era ledger.**
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
- **Memory track — moves #1-#6 now DONE (checkpoint-4 #1-#2 → checkpoint-7 #3-#6):**
  Subsystem D moves #1-#2 SHIPPED at ckpt 4 (`Created`+`Source-Agent` stamped #187, surfaced
  as authored-age + source-agent on recall #189); moves **#3-#6 SHIPPED this session** (age-
  staleness #198, `activate_temporal` per-type decay #199, supersession-on-rewrite+flock #200,
  recency-capped auto-load #201 — detailed above). **Remaining: #7 reinforce-on-use is BLOCKED**
  (no `last_used`/`memory_used` signal exists — needs a design decision), **#8 daemon
  reflect/consolidate is the largest** (leans on #7).
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
**Agent-docs gate (CI, PR-only):** `scripts/agent_docs_gate.py` + the `agent-docs-gate`
job in `ci.yml` FAIL any PR that changes an agent-workflow surface (the MCP
`M1ND_INSTRUCTIONS` string / tool schemas / verb dispatch in `server.rs`+`tools.rs`,
`protocol/`, `help_guidance.rs`, `universal_docs.rs`, `skills/`, or the npm host
installer under `npm/`) without ALSO updating agent-facing docs in the same PR
(`skills/`, `docs/` incl. the wiki, `README.md`, `CONTRIBUTING.md`, or a future
root `CLAUDE.md`/`AGENTS.md`). Anti-cry-wolf: it ARMS only on those surfaces —
unrelated core internals never trip it. Escape hatches: an instructions-only edit
self-satisfies, and the PR label **`agent-docs-exempt`** skips it for genuine
no-behavioral-change refactors. It reports (not required-check yet); Max can promote
it to blocking in branch-protection settings. Born from PR #216 (installed skills
taught a stale era for ~2 weeks). Portable via the `SURFACE_PATHS`/`DOC_PATHS`
knobs at the top of the script.

## Access Map
- Battery harness: `scratchpad/m1nd_battery.py` — **now TRACKED in-repo** (#183;
  protected by the `.gitignore` negation `!scratchpad/m1nd_battery.py` at line 52, so
  it survives scratchpad clears). Fresh ingest + ground-truth PASS/FAIL + `rg`
  head-to-head; the m1nd suite is now **36 cases, green at 36/36** on the release binary
  (grown to cover the OMEGA/memory surface, #204). Probes: `impact_probe.py`, `edge_proof.py`.
  Reports: `M1ND_BATTERY_REPORT.md`, `battery_FINAL.txt`.
- MCP stdio client pattern: `scratchpad/focus_smoke.py` (Content-Length JSON-RPC).
- Build: `cargo build -p m1nd-mcp --bin m1nd-mcp` → `./target/debug/m1nd-mcp`.
- 360 vision: `docs/X360-RUNTIME-PRD.md`. Focus runtime: `docs/FOCUS-RUNTIME-PRD.md`.
- git identity = Max Kle1nz <kleinz@cosmophonix.com>. **gh GOTCHA: the active
  account silently flips to `velvetside` mid-session → `gh pr create` fails with
  "must be a collaborator". Run `gh auth switch --user maxkle1nz` before EVERY
  push/PR.**
- Battery is now 36 cases (JSON key `records`, each row has `m1nd_pass`; summary carries
  `m1nd_pass`/`m1nd_pass_rate`); per-case `check=lambda res,q,c:` hook + `has_direct_calls_edge`
  helper enable structural cross-file assertions via the live client.

## Known Problems
- **Checkpoint-8 carry-over (open into the v1.2.x cycle — sweep the field-report mailbox
  first, then these):**
  - **`why`-closure CRY-WOLF — AMBIGUOUS portion = FIXED (field-triage #4, PR pending).** The
    `m1nd:edge:ambiguous` over-fire is closed: the tag now fires only on a genuine coin-flip
    (decisive proximity/qualifier binds no longer tag) and is read edge-specifically via a targeted
    `m1nd:edge:ambiguous:<target>` tag. Ambiguous-blocked dropped 9/11 → 0/11 on the m1nd repo;
    battery case `closure_verdict_wellformed_blocked` updated to assert (well-formed contract) +
    (no `ambiguous` dangling on the clean handle_seek→pack_to_budget path) + (honesty guard: a real
    tie STILL blocks). Binding behavior itself is UNCHANGED — only the tag precision/read granularity.
  - **`why`-closure UNRESOLVED node-granularity (NEW, discovered during field-triage #4; OPEN).**
    The `m1nd:edge:unresolved` tag has the SAME node-granularity over-fire the ambiguous tag had:
    it is set per-source-node whenever a node drops ANY outbound ref (e.g. a call to a std/external
    fn), and `closure_reason_for_edge` reads it node-level — so a clean path leaving such a node
    (e.g. handle_seek→pack_to_budget, a unique-name target) still reads `blocked` on `unresolved`.
    Left UNTOUCHED by triage #4 because "EDGE_UNRESOLVED_TAG semantics stay unchanged" was explicit
    scope, AND it needs a DESIGN DECISION (a dropped ref has no target node to key an edge-specific
    tag against; and the contract test `why_reports_blocked_when_path_rests_on_dangling_edge`
    encodes the current intent). Measured residue: 8/11 connected pairs still blocked, now all on
    `unresolved`. Follow-up task spawned.
  - **`memorize` unanchored on the live runtime (seeded field report #2).** Evidence paths
    didn't anchor to code nodes against the running `:1338` owner — friction/bug on the live
    runtime specifically. Reproduce against a served-attach graph, then fix.
  - **the `temp` artifact drop (seeded field report #3).** A stray `temp` file lands in cwd via
    a battery/tool path; the battery works around it (`rm -f temp`) but the SOURCE drop is
    unfixed. Trace which tool writes `temp` and stop it at source.
  - **Memory roadmap remainder: #7 reinforce-on-use BLOCKED, #8 daemon reflect/consolidate
    PENDING.** #7 needs a `last_used`/`memory_used` signal that does not yet exist (a design
    decision — feedback verb on recall? auto-stamp on `activate` touch?); #8 is the largest and
    leans on #7. Both carry forward unchanged.
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
- **The ambient loop needs `--serve`/`--attach` BEFORE any hook install (proven by the A/B,
  §Ω+1.3b).** The first pre-flight A/B measured it: in the **process-per-hook + in-process-graph**
  setup, (a) each hook fire re-ingests the whole repo (72ms on this small repo; would DOMINATE
  latency on a large one), and (b) **compounding cannot work** — each fire is a separate
  short-lived process, so a `Stop:memorize` writes to a graph the next fire never reloads. Both
  are architectural, not tunable. The fix is known: the hook must ATTACH to a persistent
  `--serve` graph (m1nd already ships this at #157/#158), which kills the per-fire ingest AND
  enables cross-fire compounding. **Until the served-attach variant is stood up and re-tested,
  the compounding beat is unvalidated.** Also still untested: a **harder "rescue" task** —
  the A/B's task was too easy (both arms 3/3), so it proved pre-orient HELPS orientation + does
  no harm, but NOT that north rescues a run that would otherwise fail.
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
- **Memory roadmap: #1-#6 DONE, but #7 is BLOCKED and #8 is the largest remaining.**
  Moves #3-#6 shipped this session (#198-#201, all TDD-proven). **#7 reinforce-on-use is
  BLOCKED — there is NO `last_used`/`memory_used` signal in the system** (nothing records
  that a recalled memory was actually USED), so reinforcement has no input to strengthen on;
  this needs a **design decision** (where does the use-signal come from — a `learn`-style
  feedback verb on recall? an auto-stamp on `activate` touch?) before it can be built.
  **#8 daemon reflect/consolidate is the largest remaining move and LEANS ON #7** (it
  consolidates by usage/reinforcement), so #8 is effectively gated behind the #7 decision.
  The cross-cutting concurrency-lock risk is now RESOLVED for supersession (#200's per-slug
  `flock`); missing-`Created` eviction-exemption is honored (#201); audit-observability holds
  (#201 drop is observable) — see PRD § D.
- Method-call EDGES exist for Rust (#166) but not TS/Java/Go/Python.
- **Agent-memory subsystem gaps (G1-G6, `docs/NEXTGEN-AGENT-PRD.md` § Subsystem D) — MOSTLY
  CLOSED as of checkpoint 7; residue is the `last_used`/reinforce gap.** Fixed this era:
  `.light.md` now carries `Created`/`Source-Agent` (#187/#189, move #1-#2); staleness is no
  longer code-sha-only — the `aged_out` age/disuse signal ships (#198); auto-load is now
  recency-CAPPABLE, not unbounded (#201); re-memorizing a slug no longer silently overwrites —
  it invalidates-and-keeps with supersession history + a per-slug flock (#200); and the decay
  kernels that "existed but didn't touch stored memories" now DO (`activate_temporal` reads
  per-type half-lives, #199). **Residue: `last_used` is still ABSENT** — no signal records
  that a recalled memory was used, which is exactly what blocks move #7 (reinforce-on-use);
  and there is still **no cross-agent sharing/attribution** (source is stamped but not shared).
- `scan.mitigated` is now visible at default `severity_min` (#172) — note this
  CHANGED default output (callers see `mitigated` findings; `false_positive` still
  suppressed). Revertible if undesired.
- `#[cfg(all(test, …))]` compound predicates aren't tagged via the module path.
- ~~i18n READMEs (7 langs) + wiki lag the v1.1.0 README~~ RESOLVED (PR #216):
  i18n regenerated at 1.2.0, and the era-coherence sweep brought wiki, skills,
  and agent packs to the OMEGA loop (north-first → verdicts → memorize-at-close
  → field-reports) — every agent door now teaches the same doctrine as the MCP
  `initialize` instructions (#208).
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
- **Move 2 — Solvency & Stop Gate (`solvency`) — STILL NEEDS A RE-GROUND before building.**
  Arbiter over `focus` sufficiency + `coverage_session` + a **real token-budget signal** (the
  §O.3 #4 unit-mismatch fix — `budget_consumed`/`relevance_clearing_total`/`coverage_session`
  do NOT subtract; wire a true token budget or build it net-new) + `am_i_stale`. **Re-verify
  the `file:line` anchors against current `main` before coding** (the PRD anchors carry a
  known v1.1.0 re-ground caveat; symbol is the contract, line is a hint). Then Moves 3–9
  (§O.10), each calibration-gated, composer-over-shipping-tools, degrading to UNPROVABLE not
  a fake green.

**→ FRESH STRATEGIC FRONT — Ω+1, THE AMBIENT LOOP (`docs/NEXTGEN-AGENT-PRD.md § Ω+1`).**
The next chapter after OMEGA: m1nd as the ambient nervous system of the agent loop
(`pre-orient → act → post-capture → compound`), wired into the hook lifecycle. The PRD
section is written, critic-corrected (four load-bearing corrections in § Ω+1.3), and now
**EMPIRICALLY GROUNDED by the first A/B (§Ω+1.3b)** — pre-orient HELPS orientation + does no
harm, but compounding is architecturally BLOCKED in the naïve hook setup.
- **THE NEXT AMBIENT MILESTONE = the serve/attach experiment.** The A/B proved the compounding
  beat cannot close in process-per-hook + in-process-graph (each fire re-ingests + can't see
  the prior fire's `memorize`). So the next move is: **stand up a served m1nd (`--serve`,
  #157/#158), attach the hook to it instead of re-ingesting per fire, and re-run the A/B to
  validate (a) cross-fire compounding, (b) latency-at-scale on a large repo, (c) a harder
  RESCUE task** (the last A/B was too easy — both arms 3/3). Only after that validation does
  the live wiring earn a green-light.
- **DO NOT install hooks in Max's live env without his green-light AND the serve/attach
  validation.** Installing hooks into the live Claude Code / Codex environment CHANGES Max's
  workflow (a decision, not an autonomous ship), and the naïve shim pays re-ingest latency +
  can't compound — the served-attach variant is the prerequisite.
The reuse-first Wave roadmap (§ Ω+1.4) is now re-anchored on serve/attach at Wave 0, then
ORGANIZE-first (Waves 1–3 nearly free), rewired keystone (Wave 4: `Stop → cross_verify →
memorize` directly), swarm last.

**AUTONOMOUS OVERNIGHT MANDATE (2026-06-30):** run the OMEGA roadmap (PRD §O.10) to
completion — tested + cross-repo stress-tested — UNATTENDED; stop only when complete.
Honor the universal doc gate at each move (docs/PATHOS current before "done").

**MEMORY track (Subsystem D) — moves #1-#6 DONE, #7 BLOCKED, #8 largest.** #1-#2
(`Created`/`Source-Agent` #187/#189) + **#3-#6 SHIPPED this session** (age-staleness #198,
`activate_temporal` decay fix #199, supersession-on-rewrite+flock #200, recency-capped
auto-load #201). **What's left NEEDS DECISIONS, not just execution:**
- **#7 reinforce-on-use is BLOCKED on a design decision** — there is no `last_used`/
  `memory_used` signal in the system, so reinforcement has no input. First move: DECIDE where
  the use-signal comes from (a `learn`-style feedback on recall? an auto-stamp on `activate`
  touch?), then build. Don't ship a reinforce that strengthens on nothing.
- **#8 daemon reflect/consolidate is the largest move and leans on #7** — gated behind the #7
  decision. (Full detail + gaps G1-G6 + critic corrections in the PRD § D.)
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
