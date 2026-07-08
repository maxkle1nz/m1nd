# PATHOS — m1nd agent handoff

> Read this first. Single source of truth for any chat / subagent / parallel
> session working on m1nd, so we don't re-derive state or contradict each other.
> Last checkpoint: 2026-07-08 (**checkpoint 12 — HUMAN VIEW v2 SPECIFIED AND F0 BEGUN**).
> Checkpoint 12, condensed: the owner's vision — a visual operating system for repos (Build Map front door, SystemBlock ontology, typed receipts, mission layer) — was taken through the full spec-genesis loop in one arc: foundation → oracle verdict (18 changes folded) → authored package (PRD + screen book + UML with THIRTEEN state machines) → oracle review (15 fixes) → pre-F0 gap hunt → signed F0 technical amendment → landed as PR #310 with code pre-work (`unprovable` reaches the UI trust union; `runtime_overlay` joins the read-only deny list). F0a slice 1 then landed the SystemBlock contract in code (`m1nd-mcp/src/system_blocks.rs`: serde types, anti-poison receipt evidence, path-first membership as a type invariant, seed loader/validator with the repo-relative guard, 10/10 tests) and F0b produced `docs/system-blocks/m1nd.seed.v0.json` — the twelve-block skeleton, every base path verified, validated by the slice-1 loader and **ratified v1** (ratifier recorded honestly as owner-delegated). Next: F0a slice 2 (sidecar store + the six verbs + evidence execution-fields to Option per the review note), then the Build Map read-only (F1). The four HUMAN-VIEW-V2-* docs are the binding contract; implementation invents nothing.
> Previous: 2026-07-05 (checkpoint 11 — THE LADDER IS CLIMBED, the close-out burst).
> The entire ORGANISM ladder R0–R17 is landed on `main` and live on the served owner: medulla
> storage/tiers/promotion (R2/R3/R4), delegation (R6/R7), the per-project mailbox + UI (R8/R9),
> Pre-Flight (R10), reconnect-rebind (R13), per-brain counters (R14), eviction (R15), the soul
> (R16), seek-rerank conformance (R17) — and R11's deliverable, `docs/CASE-INTELLIGENCE-PRD.md`,
> shipped as the last rung's PRD (its implementation slices S0–S4 are the next construction seed).
> The close-out burst also wired the M5a migration CLI (`--medulla-migrate plan|apply|rollback`),
> made `restart --binary` honest (loud dry-run + codesign + kickstart), fixed the mailbox roster's
> duplicate-basename misrouting to the honest abstain, ran the first real `--inbox-sweep`
> (idempotence proven live), and brought `skills/` to the serve-attach/medulla/delegation/soul era.
> Prior checkpoints (10 → 7) are summarized in **Prior Eras** below; full text in git history.

## North Star
m1nd = operational intelligence for coding agents. The bar: genuinely BEAT plain
`rg`/Read in the inner loop, measured honestly — not tie, not "feels useful".
Run a continuous, chained improvement engine: measure (battery) → fix+test the
real defect → checkpoint → seed the next cycle. Never sugarcoat results.

**The arc now:** the verifiable trust substrate (answer + map + trust receipt) is the released,
live FLOOR. On top of it, six PRDs describe one organism — a per-project brain, an antifragile
shared memory, a native and verified handoff soul, a human-legible tree, a two-tier routing
backend, and a delegation layer. The design era spent its final stretch making those six cohere
into ONE constitution with ONE build order. The work ahead is CONSTRUCTION: climb the ladder,
rung by rung, each slice proof-grown and degrading to UNPROVABLE rather than a fake green — the
same bar, applied to building the organism outward.

## Current State (2026-07-05, checkpoint 10 — the design era closes, the construction era opens)

### The design era CLOSED — six PRDs on `main`, one organism
The blueprints are complete and, as of this checkpoint, reconciled into a single constitution.
Each PRD, one line:

- **`docs/ORGANISM-PRD.md` — THE CONSTITUTION (the capstone).** One spine (the north packet),
  four grammars (the trust ladder · the belief lifecycle · provenance/no-leak · attention), one
  ritual (pre-orient → act → capture), and **THE LADDER (§C10): a single cross-PRD build order,
  rungs R0–R17.** It was adversarially verified (a critic pass whose corrections are folded back
  as amendments, §C11) — the constitution wins ties between the other five PRDs and carries the
  pointer that says so. This is the last blueprint of the design era; everything after it climbs.
- **`docs/MEDULLA-PRD.md` — antifragile memory across per-project brains.** The memory state
  machine (per-brain storage · `Origin-Brain` labels · tier recall with no cross-brain leak ·
  promotion into a shared doctrine tier), designed to get stronger under churn rather than drift.
- **`docs/SOUL-PRD.md` — PATHOS native, verified, curated. [R16 S0 + S1 substrate SHIPPED
  2026-07-05]** This very handoff is now a first-class m1nd type: `soul_check` parses THIS file into
  anchored claims and returns a freshness receipt (last run on cp10: 13 fresh · 14 stale · 61
  declared); `soul_read` is the explicit pull; `soul_update` (a `memorize` mode with `Soul-Source`
  provenance) + the §C8.4 curator seat check (grader ≠ author) are the curator's substrate. The
  automated curator sweep + the north-packet soul beat (S2) + the skill call-through (S3) are the
  honest residue. The soul rode LAST on the ladder as designed.
- **`docs/HUMAN-LAYER-PRD.md` — the human face.** The Hall (projects area) · the Living Tree
  (memory-decorated filetree) · the mailbox · the precision system (iconography, lenses, honest
  search) · the Pre-Flight card — an agent's memory made legible to a human.
- **`docs/TWO-TIER-BRAIN-PRD.md` — per-project brains + reception + cwd routing.** Each repo gets
  its own brain inside one served owner; reception tells a caller honestly when it is wearing the
  wrong brain; cwd routes each call to the right one.
- **`docs/NEXTGEN-AGENT-PRD.md` §O.12 — the delegation layer.** A parent hands a child a grounded
  packet and reads back a debrief; a parent that cannot ground the child honestly declines
  (delegation-abstain). OMEGA's reach extended from one agent to a tree of agents.

### The construction era OPENED — R0/R1/R5 shipped (#275)
The first three ladder rungs landed together — small, live-defect fixes that make the flagship
packet honest and lean before the medulla state machine builds on top of it:

- **R0 — packet honesty (MED-INV-6 false-absence fix).** A `north` beat over a **non-empty**
  memory store used to emit "No durable memory yet" whenever recall found nothing for the task —
  a false absence (reproduced live: ~20 claims on disk, `memory: []`, and still the empty-store
  line). Now `SessionState::light_memory_count()` reads the ground-truth `.light.md` count, the
  packet stamps `memory_exists = n`, and the false line fires ONLY when the store is truly empty;
  over a non-empty store the gap honestly says the store holds claims that did not match this task.
- **R1 — the packet diet (Budget Law).** The binding blew its token budget two ways, both live:
  the `ingest_roots` array was serialized twice byte-identically, and the memorize write-path
  minted a per-file ingest root for every memory sidecar. Fix: `graph_runtime_summary` carries only
  `ingest_root_count` (the full array lives once, in the fingerprint), and a `.light.md` written
  into the `agent-memory` store collapses to the single store-dir root. **Measured: the packet is
  battery-pinned at ~1,419 tokens** (budget ≤2k), with CI failing on dup-arrays / sidecar-roots /
  >2k growth.
- **R5 — separator-agnostic `display_name` (Windows CI honesty triage).** `basename_of()` assumed
  `/` separators, so the brain name misfired on Windows backslash paths — the chronic red Windows
  CI test. Now it splits on both `/` and `\` (trailing-sep, UNC, mixed, POSIX all covered): a gate
  described as blocking now blocks, and Windows CI is green.

Each rung shipped RED-first (a failing test that pins the defect) → GREEN, with the doc pass in the
same PR.

### The ladder is the build order (ORGANISM §C10, R0–R17)
An implementer reads §C10 alone and knows what to build next. The spine of the order, past the
shipped R0/R1:

> **R2** (M5a — storage split + `Origin-Brain` + migration + brainless-root refusal) → **R15**
> (the eviction gate: LRU + persist-on-evict; a HARD pre-condition for the next rung's
> `all-brains` half) + **R3** (M5b — `tier` recall + no-leak proven + `all-brains`) → **R4** (M6 —
> the `promote` verb with its provenance riders) → delegation (R6 `delegate`/`debrief`) → mailbox
> (R8/R9 boxes + view) → **R10** the Pre-Flight Card → **R16** the SOUL PRD + slices, LAST, bound
> by the constitution's seven soul constraints.

The two integration points every rung composes over are the **write door** (§C4) and the **packet
spine** (§C1). No rung lands without its battery case first (RED), its doc pass, and the landing
gate. R5 (Windows) and R17 (a conformance-boost rerank that lets X-RAY steer attention) sit off
the critical path.

### Runtime reality
The served owner warm-boots multiple **per-project brains** inside one process. Per-brain **Open**
works end-to-end (a hosted project's tree opens by name, not by plumbing path). **Reception is
honest** — a caller in repo X wearing repo Y's brain is flagged, not silently served. The **Hall**
renders every brain the owner holds as a named project, freshest-first, with absent-honest counts.
Activating a new UI or a new binary needs a served-owner restart (the dist is rust-embedded; the
binary is warm-booted) — note it honestly at each cut.

## Operating Doctrine
Proof-grown: measure before claiming; verify work yourself (re-run the battery / a probe), never
trust a report. Battery-gate risky core changes. Fix AND test every defect (RED-first: a failing
test that pins the defect before the fix). Commit+push always (PR → CI → merge). Never bypass branch
protection (admin-merge is blocked by design). Land deep changes with a tight, source-grounded spec
+ a battery gate; verify on the REAL diff. Update this file at big checkpoints.

**The ladder is doctrine now:** the next rung is whatever §C10 says is next — read the constitution's
build order before opening a new front, and climb it in dependency order (R15 is a hard
pre-condition for R3's `all-brains`; the soul rides last). Divergence ripples out through §C11-style
amendments to the constitution, never by silent contradiction.

**Universal field-telemetry doctrine.** Every agent, every repo, is a sensor. When m1nd misbehaves
during ANY mission — even on another repo — the agent REPORTS, it does not fix: append one JSON line
to the machine-global mailbox `~/.m1nd/field-reports.jsonl`
(`{ts,agent,repo,tool,class:"bug|honesty|friction|win",what,expected,snippet}`) and keep working.
Report-never-fix mid-mission is the rule. The `honesty` class is the most valuable — it is
calibration ground truth (m1nd overclaimed and was wrong). When retrieval was simply right/wrong,
prefer the built-in `learn` verb (correct/wrong/partial). Triage closes the loop: every improvement
session STARTS by sweeping the mailbox (+ `seek` for field memories), and a confirmed field bug
becomes a battery case/test BEFORE the fix. The mailbox is local-only — m1nd never phones home.

**Agent-docs gate (CI, PR-only):** `scripts/agent_docs_gate.py` + the `agent-docs-gate` job FAIL any
PR that changes an agent-workflow surface (the MCP `M1ND_INSTRUCTIONS` string / tool schemas / verb
dispatch, `protocol/`, `help_guidance.rs`, `universal_docs.rs`, `skills/`, or the npm host installer)
without ALSO updating agent-facing docs in the same PR (`skills/`, `docs/` incl. the wiki, `README.md`,
`CONTRIBUTING.md`, or a root `CLAUDE.md`/`AGENTS.md`). It arms only on those surfaces (anti-cry-wolf);
an instructions-only edit self-satisfies; the `agent-docs-exempt` label skips it for genuine
no-behavioral-change refactors.

## Access Map
- Battery harness: `scratchpad/m1nd_battery.py` — **TRACKED in-repo** (protected by the `.gitignore`
  negation `!scratchpad/m1nd_battery.py`, so it survives scratchpad clears). Fresh ingest +
  ground-truth PASS/FAIL + `rg` head-to-head; the m1nd suite runs green with zero grep losses.
  (Prior throwaway probes `impact_probe.py`/`edge_proof.py`/`focus_smoke.py` and the
  `M1ND_BATTERY_REPORT.md`/`battery_FINAL.txt` reports were scratchpad-cleared — pruned here by the
  R16 curator pass; the tracked battery is the durable one.)
- **The soul is verified now (R16 · `soul_check`):** run `soul_check` (or `soul_read`) against THIS
  file — it parses PATHOS into anchored claims and returns the freshness receipt (N fresh · M stale ·
  K priced @sha). The pathos skill is the AUTHORING guide; m1nd is the ENGINE.
- Build: `cargo build -p m1nd-mcp --bin m1nd-mcp` → `./target/debug/m1nd-mcp`.
- **The constitution + the build order:** `docs/ORGANISM-PRD.md` (§C10 is THE ladder; §C11 the
  amendment ledger). The five other PRDs: `MEDULLA-PRD.md`, `SOUL-PRD.md`, `HUMAN-LAYER-PRD.md`,
  `TWO-TIER-BRAIN-PRD.md`, `NEXTGEN-AGENT-PRD.md` (§O.12 delegation, §O.10 the OMEGA floor roadmap).
- Runtime PRDs: `docs/X360-RUNTIME-PRD.md`, `docs/FOCUS-RUNTIME-PRD.md`. Ambient layer per host:
  `docs/HOST-INTEGRATION-MATRIX.md`.
- git identity = Max Kle1nz <kleinz@cosmophonix.com>.

## Known Problems (honest, product-level)
- **The medulla ladder R2→R4 (M5a → M5b → M6) is BUILT — CODE-LANDED, live HELD.** M5a (per-brain
  storage + `Origin-Brain` labels + reversible migration + brainless-root refusal), M5b (`tier` recall
  + the no-leak invariant proven + `all-brains` through the eviction gate), and M6 (the `promote` verb
  + the C8.2 origin-qualified-evidence rider + the C8.3 verified-only gate + demotion) have all shipped
  to `main` with RED-first proofs. **The LIVE owner at `:1338` has NOT been migrated/restarted** — the
  code lands and is scratch-proven per slice, but serving the new storage + `promote` verb on the live
  owner needs a maintainer rebuild/kickstart (held deliberately). Next real build: R6 (delegation —
  `delegate`/`debrief`).
- **Per-brain session-counter partition is PENDING (ladder R14, §9.5.1).** In one served owner,
  session/query counters are not yet partitioned per brain, so aliveness counts can bleed across
  brains in the Hall. Backend work budgeted, not done.
- **The `seek` rerank centrality-vs-semantic balance was corrected (pre-R17).** Previously a
  high-PageRank node could out-rank a more semantically-relevant hit — the `graph_activation * 0.2`
  centrality prior was added ungated, so a near-zero-relevance hub could ride pure centrality to the
  top. Fixed by gating the activation term with the node's own relevance (`max(sem, keyword,
  trigram)`): centrality stays a full-strength co-ranker/tie-breaker for relevant nodes but can no
  longer swamp the semantic signal for irrelevant ones. This lands the balance FIRST, before R17
  adds `conformance_boost` to the same rerank — so X-RAY steers attention on top of a correct base.
- **`x.method()` receiver-type inference — the #1 remaining GRAPH gap.** A bare `x.method()` on a
  local/field receiver carries no qualifier, so same-name ties fall to proximity / `candidates[0]`.
  Qualified calls (`Type::method()`, `module::func()`) and cross-file proximity are solved;
  receiver-type inference (track `let x: T` / field types / fn return types) is the dedicated harder
  cycle. Method-call edges exist for Rust but not TS/Java/Go/Python.
- **`why`-closure UNRESOLVED node-granularity.** The `unresolved` closure tag still over-fires at
  node granularity (the ambiguous tag was fixed to edge granularity; unresolved was not): a clean
  path leaving a node that drops any outbound ref (e.g. a std/external call) still reads `blocked`.
  It needs a design decision — a dropped ref has no target node to key an edge-specific tag against.
- **`predict`'s strength model is COARSE.** Calibrated against m1nd's own history it tops out around
  ~28% act-band precision at ~15% coverage. The calibrator is honest — `act` is structurally withheld
  until the number clears a risk budget — but the underlying strength model (`0.1·N` in neighbor
  count) needs a real upgrade before `predict` can `act` at useful coverage.
- **The poisoned-oracle threat model is OPEN.** A poisoned eval set or co-change corpus makes the
  calibrator certify a wrong verdict with confidence — "who calibrates the calibrator?". Logged as
  unsolved; eval-set integrity is a prerequisite before any verb defaults on.
- **PATHOS auto-refresh push-back to `main` is BLOCKED by branch protection.** The pattern is installed
  and proven up to the last hop: the Action regenerates the auto sections and commits on the runner,
  but the direct push is rejected (required status checks; `GITHUB_TOKEN` has no bypass). The workflow
  fail-softs (warning + step summary, never a perpetually red run) and prefers a `PATHOS_REFRESH_TOKEN`
  secret if present. Unblock = a maintainer call (a fine-grained admin PAT in `PATHOS_REFRESH_TOKEN`,
  or a required-checks bypass for `github-actions`). Until then the auto sections refresh only when a
  PR carries a regenerated copy.
- **Multi-session hygiene.** A served owner holds the live brain and sibling worktrees may hold
  parallel work — `git fetch` before acting, confirm `git branch --show-current` before commit, do
  feature work in an isolated worktree with the shared `CARGO_TARGET_DIR`
  (`$HOME/.m1nd-build-cache/target`), and `git worktree remove` it when done.

## Proof Standard
Done = `cargo test --workspace` green + clippy `-D warnings` + `cargo fmt` clean + the BATTERY
(`scratchpad/m1nd_battery.py`, tracked) green on the m1nd suite (zero grep losses) showing the
targeted tool improved with a concrete example, zero regression. CI green on 3 OSes before merge.
**For UI/human-layer slices:** INV component tests against REAL captured envelopes
(`m1nd-ui/src/__fixtures__/`) + the violet-lint (violet reserved for abstain/unknown) + the icon-lint
+ a live dogfood against a `--serve` of m1nd's own graph. **For OMEGA/prediction verbs,
calibration-gated JOINS battery-gated:** battery tests prove the code does what it says (consistency);
the calibrator proves the verdict is right often enough to act on (correctness-at-coverage). A verb
earns `act` as an allowed output ONLY when measured precision-at-coverage clears the stated risk
budget — until then `act` is structurally withheld and the verb emits `reverify`/`abstain`/`unprovable`.
Recalibration, not retraining: the number is re-measured against ground truth, never asserted in a
README. Engine cadence: each rung lands in a worktree-isolated slice with a source-grounded spec +
battery gate → verify on the REAL diff → PR/merge → the UNIVERSAL DOC GATE (docs/wiki/README/PATHOS
current, agent surfaces updated in the SAME PR) → seed the next rung.

## Next Agent Prompt / next seeds

**→ THE ERA IS CONSTRUCTION.** The design era is closed; do not write another vision. Read the
**ORGANISM constitution's §C10 ladder** — it is the single cross-PRD build order, and an implementer
reads that chapter alone and knows what to build next. R0/R1/R5 shipped (#275); **R15 → R2 → R3 → R4
are now SHIPPED (code-landed, live held)** — the whole medulla memory spine (storage split, tier
recall, promotion) is real code with RED-first proofs. **Climb from R6 onward, in dependency order,
RED-first per rung:**

1. ~~**R2 — M5a: the medulla storage split**~~ **[SHIPPED #279]** Per-brain storage + `Origin-Brain`
   labels + reversible migration + brainless-root refusal. The long pole; every later memory rung
   stands on it.
2. ~~**R15 — the eviction gate**~~ **[SHIPPED #277]** LRU + persist-on-evict in the owner. The hard
   pre-condition for R3's `all-brains` half.
3. ~~**R3 — M5b: `tier` recall + no-leak proven + `all-brains`**~~ **[SHIPPED #280]** The leak
   permutation matrix, medulla-doctrine-surfaces-cross-brain, `all-brains` through the eviction gate.
4. ~~**R4 — M6: the `promote` verb**~~ **[SHIPPED]** The audited crossing + the C8.2 origin-qualified
   evidence rider + the C8.3 verified-only gate + demotion, agent-workflow surfaces in the SAME PR.
   **Next:** delegation (R6 `delegate`/`debrief`), then the packet memory slice (R7), the mailbox
   (R8/R9), the Pre-Flight Card (R10), and — LAST — the SOUL PRD + slices (R16), bound by the
   constitution's seven soul constraints.

**Doctrine pointers (carry verbatim into every spawned agent):** the **UNIVERSAL FIELD-TELEMETRY
DOCTRINE** (every agent/repo is a sensor → REPORT to `~/.m1nd/field-reports.jsonl`, never fix
mid-mission; `honesty` class is calibration ground truth; a triage session STARTS by sweeping the
mailbox, and a confirmed bug becomes a battery case BEFORE the fix); the **UNIVERSAL DOC GATE incl.
agent surfaces** (docs/wiki/README/PATHOS current before "done"; any change to HOW agents work
updates the agent-read surfaces in the SAME PR — the agent-docs CI gate enforces this); the **DISK
HYGIENE rule** (shared `CARGO_TARGET_DIR` + worktree sweeps).

---

**↓ THE FLOOR (still-true reference): m1nd-OMEGA, `docs/NEXTGEN-AGENT-PRD.md` §O.10** — the verifiable
trust substrate, released and live. Moves 0 (conformal calibration harness) + 1 (the Trust-Gated
Envelope) are DONE and RELEASED (v1.2.0/1.2.1); the honest Move-2 reframe shipped. Read §O.1–O.11 for
the vision (answer + map + trust receipt), the calibration keystone (consistency ≠ correctness), the
baked-in critic corrections, and the open poisoned-oracle risk. **Move 2 (Solvency & Stop Gate)
remains a DESIGN task, not a build task — it is roadmap-only and un-grounded:** m1nd has no token
ledger, so a solvency arbiter would need a real budget signal wired or built net-new before it could
mean anything, and its `file:line` anchors must be re-verified against current `main` first. It is NOT
the active north — the ladder is. Return to deepening the substrate only if construction ever needs it.

## Do Not Do
- Don't edit/build m1nd source while a battery/subagent is building on the shared worktree (corrupts
  its measurement). Don't admin-merge / bypass branch protection. Don't claim a rung works without a
  battery re-run on the REAL diff. Don't delete unmerged branches without patch-id proof. Don't open a
  new front off-ladder — the constitution's §C10 order is the north; diverge only via a §C11 amendment.

## Open Questions
- Should auto-freshness default-on (watcher per ingest) or opt-in? (decide with a battery staleness
  scenario.)
- Does the `impact` symbol-first ranking want to differ by direction (reverse=callers vs
  forward=dependencies)?
- Where does the `last_used`/reinforce-on-use signal come from (a `learn`-style feedback on recall? an
  auto-stamp on `activate` touch?) — the blocker on the memory subsystem's reinforce/consolidate moves.

## Prior Eras (summary — full text in git history)
- **Checkpoint 9 (2026-07-03/04) — the construction era opens.** Three PRDs made official (HUMAN-LAYER,
  §O.12 delegation, TWO-TIER-BRAIN); the first human surfaces SHIPPED (Living Tree, the Hall, the tree
  precision system, per-brain Open); reception degraded-mode shipped; the field-report mailbox swept to
  empty in ~a day (each report a battery case before its fix). Releases: v1.3.0 (the shell reaches every
  host — 22-host recipes, MCP-Registry manifest), v1.3.1/1.3.2 (discoverability + the launch funnel).
- **Checkpoints 8 / 8.1 (2026-07-02/03) — the first OMEGA-era releases.** v1.2.0 (OMEGA Move 0
  calibration + Move 1 Envelope + the honest Move-2 reframe + `north` pre-orient + memory moves #1–#6)
  and v1.2.1 (the compounding fix — `north` composes L1GHT agent-memory recall — plus field-triage
  fixes) cut, published, and rebuilt into the served runtime. The universal field-telemetry doctrine
  established here.
- **Checkpoint 7 (2026-07-01) — memory roadmap #3–#6 + the pre-flight A/B.** Age-staleness,
  per-type decay, supersession-on-rewrite+flock, recency-capped auto-load all shipped. The first A/B
  proved `north` pre-orient HELPS orientation and does no harm, but found compounding architecturally
  blocked in process-per-hook — the insight that the ambient loop's real prerequisite is `--serve`/`--attach`.

---

<!-- ────────────────────────────────────────────────────────────────────────
  AUTO-GENERATED SECTIONS — do NOT hand-edit between the anchors below.
  Everything ABOVE this line is hand-curated and never touched by automation.
  The auto-changelog (git-cliff over Conventional Commits) and auto-overview are
  regenerated on every push to `main` by .github/workflows/pathos-autorefresh.yml
  and committed back as Max Kle1nz with [skip ci].
──────────────────────────────────────────────────────────────────────── -->

## Auto — changelog (since the last `vX.Y.Z` tag)

<!-- BEGIN:auto-changelog -->
### Unreleased

**Chores & infra**

- Stage the refreshed PATHOS per-path — a missing pathspec made git add atomically stage nothing (#237)
- Agent-docs gate — agent-workflow changes require agent-facing doc updates (or explicit exempt) (#229)

**Docs**

- Checkpoint 9 — the construction era opens (3 PRDs, Living Tree, mailbox swept) + auto-refresh installed (#236)
- The shell is the product — README re-spined around the operating loop (#228)
- TWO-TIER-BRAIN-PRD — per-project brains + shared medulla (official, proof-grown) (#227)
- The 5 launch plates (SOFT PROOF, maintainer-approved) (#223)
- §O.12 — the Delegation Layer (packet, debrief, delegation-abstain) (#224)
- HUMAN-LAYER-PRD — the Living Tree, post-it memory, Pre-Flight hero (vision → spec) (#222)

**Features**

- Living Tree slice 0 — the tree, post-its, trust dots (read-only) (#232)

**Fixes**

- Write-tool responses return real envelopes through the bridge (field-triage L21) (#235)
- Remove the opt-in savings/report unmeasured-claims surface (brand gate G1.5) (#234)
- Marker fragments excluded from recall/anchor slots (field-triage batch A) (#231)
- Re-init covers all unknown-session shapes + restart-survival proof (field-triage batch C) (#233)
- All persist targets resolve against runtime_root, never cwd (field-triage batch B) (#230)
- L1GHT recall robust on mixed graphs — memory beat scoped to light provenance (field-triage #6) (#226)
- Bridge transparently re-initializes on owner restart (-32001) (field-triage #5) (#225)
- Remove the unmeasured savings envelope — an uncalibrated claim is the confident guess (brand gate G1) (#221)
<!-- END:auto-changelog -->

## Auto — repo overview

<!-- BEGIN:auto-overview -->
- **Repo:** `m1nd`
- **Branch:** `main`
- **Last commit:** 2026-07-03
- **Commits since `v1.2.1`:** 17
<!-- END:auto-overview -->
