# m1nd Next-Gen Agent-Native Runtime

> ⚠️ **Orchestrator grounding correction (Max Kle1nz / guardian, 2026-06-29).** This PRD was synthesized by a deep-research workflow (6 dimensions; **3 completed** — concurrency / trust / context. Resolution was folded into Subsystem C as an honest `non_claim`. The **dedicated agent-MEMORY dimension failed validation and is an OPEN research gap** — extend L1GHT for multi-agent / decay / cross-agent: TODO.). The research agents grounded against the **STALE `~/m1nd` worktree (beta.8)**. The **current `main` is v1.1.0 (`~/m1nd-night`)** and DOES ship `focus` + sufficiency (`compute_sufficiency`, `handle_focus`, … — verified present, 9 matches) — so **Subsystem C EXTENDS them, it does not create them**; the appendix line "sufficiency symbols not present" is an artifact of the stale read. **Re-verify every `file:line` anchor against `~/m1nd-night` before coding — the symbol is the contract, the line is a hint.** First l00p move re-grounded: `is_pid_live` is at `instance_registry.rs:529` (not :511 — drift from the boot-GC fix #178, now merged). The designs / leaps / roadmap / importable refs below STAND; only the "current-state" facts need the v1.1.0 re-ground.

> **PRD status:** durable design doc, merged from three subsystem briefs.
> **Verification stamp (verify-before-assert, 2026-06-29):** anchors below were re-checked against the live tree at `~/m1nd`. Two corrections from the source briefs are carried as honest deltas (see *Verification deltas*), because the honesty moat forbids citing symbols that no longer exist.
> **Actual workspace version:** `m1nd-mcp` is `0.9.0-beta.8` (per `m1nd-mcp/Cargo.toml:3`), **not** "v1.1.0" as some briefs state — corrected here so no public claim inherits a wrong version.

---

# m1nd-OMEGA — the v1.2 → v2.0 era

> **Banner / codename, not a separate product (yet).** *m1nd-OMEGA* is the era spanning **v1.2.0 → v2.0** of this runtime, not a fork or a rewrite. Concretely: **v1.2.0 is cut the moment the first OMEGA verb ships** — Move 0 (the calibration harness) plus the Trust-Gated Envelope. OMEGA features then land **incrementally across 1.2.x → 1.9.x**, each one battery- and *calibration*-gated, reuse-first over what already ships. At roughly **v2.0 the era graduates** into a separately-named product. Everything below this banner is the umbrella vision; **Subsystems A–D (§3) are the first concrete OMEGA deliverables** that make it real. This section is held to the same honesty standard as the rest of the PRD: every claim is written to survive the adversarial critic, and the critic's corrections are baked in (not bolted on as caveats) — because for OMEGA, *honesty is the moat*, so the doc itself must model it.

## O.1 Thesis

m1nd-OMEGA is the **verifiable trust substrate that autonomous agents route every code decision through** — the content-addressed, file-incremental code graph m1nd already builds (fused with dataflow/taint and a non-optional honesty layer), where every answer ships with a *map* (what it reaches), a *receipt* (the re-derivable evidence behind it), and a *trust verdict* (`act` / `reverify` / `abstain` / `unprovable`). It is not a better search engine; it is the thing that lets an agent **mechanically decide how much to rely on an answer, when to stop spending, and when to refuse to act** — the properties that survive model churn, codebase drift, and oracle-poisoning over a 20-year horizon. The leap from m1nd to OMEGA is making **composition first-class**: an agent gets answer + map + trust in *one* round-trip instead of orchestrating N calls by hand, because the cost law (Σ turns × growing context) makes N round-trips the agent's dominant tax.

## O.2 The questions only m1nd answers (the egregious census, deduped)

These are questions no grep/LSP/RAG/CPG product answers today — each grounded in a tool that ships in m1nd's grounded catalog (the 119-tool census these signals are read off of). They are the raw signals OMEGA composes; OMEGA invents none of them.

**Sufficiency & economics (agent-native, no human equivalent)**
- *Do I already have ENOUGH context to act, or is there still-relevant code I haven't seen?* — `seek`/`focus` answer-free knee-test verdict (sufficient / gathering / saturated).
- *Given a token budget, what is the MINIMAL set worth loading, and exactly what did you leave out and why?* — `focus` budget-bounded set + honest `ignored` count that **can never read zero while truncation happened**.
- *In this session, which files have I actually visited vs. left as blind spots?* — `coverage_session` unvisited negative-space.

**Reality reconciliation (time + disk, categorically outside snapshot tools)**
- *Has anything in my working set changed on disk SINCE ingest, so my cached understanding is now lying?* — `am_i_stale` SHA256-vs-inventory proof.
- *Which files change TOGETHER in git history despite having NO import/call edge?* — `ghost_edges` temporal coupling.
- *Which modules are ACCELERATING in churn (second derivative) — imminent-bug precursors?* — `tremor`.
- *How structurally divergent is the repo vs a baseline ref/date/last_session?* — `diverge` (1 − Jaccard) + `drift` (Hebbian edge-weight drift).

**Transitive structure & flow (multi-hop, weighted, honest)**
- *Full transitive blast radius of a change, ranked production-first, with causal chains — and is the answer honestly complete or resting on a dropped edge?* — `impact` + `why` closure verdict.
- *If I DELETE these nodes, how much reachability is lost, and do they fail synergistically or redundantly?* — `counterfactual`.
- *What else will I be forced to change, learned from git co-change, dampened by trust/tremor?* — `predict`.
- *Does tainted data reach a sink WITHOUT crossing a validation boundary?* — `taint_trace` boundary_misses.
- *Where do concurrent paths collide on shared state without a lock?* — `flow_simulate` turbulence points.
- *Where SHOULD a connection exist but doesn't?* — `missing` structural holes (reasoning about **absence**, which grep structurally cannot).

**Learned defect history (actuarial, decaying — no static analog)**
- *Which modules are actuarially most likely buggy RIGHT NOW, by confirmed-defect density with recency half-life?* — `trust` (and it honestly emits **null, not a fake 0.5**, on empty history).
- *How risky is editing THIS file, by accumulated operational history (trust × tremor × antibody × blast)?* — `heuristics_surface`.
- *Does my planned edit reproduce a confirmed past bug pattern, including a NEGATIVE-edge "must NOT call the validator" shape?* — `antibody_scan`.

**Self-trust (a server reasoning about its own honesty)**
- *Am I bound to the m1nd I think I am — same process/binary/graph/generation, or split-brain/stale/wrong-workspace?* — `trust_selftest`/`session_handshake` binding_fingerprint.
- *Is this empty result the truth, or a binding artifact — and what exact step repairs it?* — `recovery_playbook` deterministic state machine.
- *Does the graph still match disk, and is every memorized claim's `grounded_in` evidence file still byte-identical?* — `cross_verify` evidence_freshness re-hashing.

**Architecture conformance & proof-state (the code > intended-truth diff)**
- *Does real code still obey the North Star, and which edges are eroding it?* — `xray_orient` + `xray_gate`.
- *What is each node's structural proof-state (bedrock / unproven / overgrowth / erosion), with a repo-wide proof_coverage fraction?* — `xray_paint`.
- *Which docs/specs have silently drifted from the code they describe?* — `document_drift`.
- *Is this plain-English architectural claim true, with a Bayesian verdict + contradicting evidence?* — `hypothesize`.

**Agent-process governance (proves HOW an agent investigated)**
- *Given my recorded event stream, what is the ONE next move that advances PROOF, and what am I forbidden from doing?* — `mission_next`.
- *Is THIS claim allowed to count as verified, or is my evidence merely graph-only?* — `mission_verify` (**structurally rejects graph-only evidence**).
- *Hand my whole investigation to a successor: what's proven, open, dead, and NOT to re-claim.* — `mission_handoff`/`mission_close` proof packet.
- *Which of my saved hypotheses are now stale because their supporting nodes vanished?* — `trail_resume` (auto-downgrades >50%-vanished hypotheses).

**Runtime & cross-repo (reality beyond topology)**
- *Where does real production heat (hot/slow/error OTel spans) land on the graph?* — `runtime_overlay` (honestly reports `spans_unmapped`).
- *Which OTHER repos does this codebase point at, and can I federate them with typed cross-repo edges?* — `external_references` + `federate_auto`.

The honest weak spots the census itself names — `report`'s token/CO₂ "savings" are heuristic constants, `trust`'s guidance string falsely claims `cross_verify` populates it (only `learn` does), perspective route-families are cosmetically `Structural`-only — are **load-bearing for OMEGA's credibility**: a tool that documents its own lies is the one agents trust. They stay documented and get *fixed*, never hidden.

## O.3 The combinatorial superpowers (the heart)

Independent design passes converged on the same small set of fusions. Deduped and sharpened, the durable winners cluster into **four families**, ranked by leverage. **Every composing tool ships today** — OMEGA is wiring, not a new engine.

### ★ #1 — The Trust-Gated Answer Envelope (the universal receipt)

Wrap *any* m1nd answer in a per-answer trust verdict.

- **Composes:** `trust_selftest` (right/fresh binding, not split-brain) × `cross_verify` evidence_freshness (cited file re-hashed, still matches) × `am_i_stale` (working-set hash check) × the answer's own `why`/`seek` **closure** verdict (closed vs blocked-on-dangling-edge) × `mission_verify` evidence class (direct vs graph-only).
- **New question:** *"Should I act on THIS specific answer at full confidence, or is the binding / evidence / closure / evidence-class underneath it rotten?"* → a verdict in `{act | reverify | abstain | unprovable}`, with the exact repair call named when not `act`.
- **Why it matters for an agent:** an agent calling tools 1000×/session cannot hand-audit each answer. A composable receipt lets it *mechanically* decide reliance — the property that makes a tool trusted for 20 years instead of abandoned the first time it serves a stale answer over a split-brain binding.
- **Honest novelty (critic-corrected — see §O.5):** the genuinely new thing is fusing **binding-identity + evidence-rehash + closure + evidence-class into one abstain signal over a re-derivable code graph**. It is **not** "nobody ever did this" — per-answer trust/faithfulness receipts are an active research line; OMEGA's contribution is the *self-falsifying code-graph substrate* the receipt rests on.
- **The gate is a CALIBRATED WEIGHTING, not an any-red AND-fold (critic correction baked in).** A naïve `any-red ⇒ abstain` AND over 4–5 noisy probes yields **~23% spurious abstention on a churning repo**; agents learn the envelope "cries wolf" and route around it — and the moat dies. The default policy **must** be a calibrated weighting (per-probe reliability weights + an operator risk budget) tuned against ground truth *before* it is allowed to default on. `act` is not a syntactic conjunction; it is a calibrated decision the §O.6 Move 0 harness must certify.
- **Build path:** a thin `envelope` wrapper every answer-emitting tool can opt into; the receipt is content-addressed (reuse `mission_event`'s `event_digest`) so a CI gate or future agent re-checks it. Ships **DARK** until the calibrator certifies its precision-at-coverage clears the budget.

### ★ #2 — Provable-Edit Underwriting (the pre-commit regret gate)

The most-converged-on edit-time fusion.

- **Composes:** `validate_plan` (3-hop blast radius + untested-critical gaps + proof_state) × `predict`+`ghost_edges` (git co-change files I'll *also* touch) × `tremor`+`trust` (which are accelerating / defect-prone) × `xray_gate` (would my planned imports violate the ratified North Star?) × `antibody_scan` (does my edit reproduce a confirmed bug pattern?) × `am_i_stale` (is my picture even current?).
- **New question:** *"Before I write one character: what is this edit's regret — its untested transitive dependents, the co-change tail I'll forget, the architecture rule I'd erode, the bug-shape I'd reopen, AND whether my model is already stale — fused into one go/insure/abort verdict plus the minimal co-change+test set that brings risk under my budget?"*
- **Why it matters for an agent:** this is the direct answer to the SWE-CI regression collapse. A human eyeballs the diff and ships; an autonomous agent owning code for years needs a pre-commit actuarial gate that says "don't" *with a reason it is bound to honor*. Knowing the regret *before* writing lets the agent batch co-changes into ONE pass or abstain — instead of paying for three corrective loops.
- **Honesty:** inherits each input's honesty marker, so it degrades to **UNPROVABLE** rather than a fake green light. The verdict emits `{proceed | require-human | abstain}` + the minimal-cover set. Like every OMEGA verb, it ships **calibration-gated**: `act`/`proceed` is only an allowed output once Move 0 certifies it empirically clears the regression-risk budget against a replayed-CI eval.

### ★ #3 — Stale-Belief Quarantine on Resume (memory that expires on proof)

The antidote to the frontier's named #1 trust-killer: temporal obsolescence.

- **Composes:** `boot_memory`/`reload_agent_memory`/`trail_resume` (what a past me concluded) × `cross_verify` evidence_freshness (re-hash every `grounded_in` file behind a memorized claim) × `am_i_stale` (per-file SHA drift) × `diverge(last_session)` (structural drift) × `alerts_list` (what the daemon flagged while nobody watched).
- **New question:** *"At session boot, partition everything I durably believe into STILL-TRUE / SOFT-STALE / DEAD — and re-inject only the still-true — before I touch anything. What is the minimal re-derivation set that restores correctness?"*
- **Why it matters for an agent:** a human reopens an IDE with no memory; an agent reopens *with* compounding memory that may have silently rotted, and confidently acts on a function renamed three commits ago. Stale knowledge **expires loudly with proof** (a SHA mismatch, a missing-node count), never on a guess. It operationalizes the `code > PATHOS > memory` hierarchy as a boot-time sweep.
- **Honest novelty + scoping correction (baked in — see §O.5):** bi-temporal belief quarantine is a known pattern (a port of Zep/Graphiti's invalidate-not-delete onto a *code* graph) — frame it as a port, not an invention. **Two grounding defects the critic found, corrected here:** (a) cross-session belief freshness **cannot** come from `am_i_stale` (it is empty at session boot and only sees THIS session's visited files) — it **must** come from `cross_verify` re-hashing each claim's `grounded_in`; (b) propagating staleness through the co-change matrix is **net-new logic**, not "85% exists" — `ghost_edges` exposes only **3 scalar counts**, not a caller-facing co-change matrix, so the matrix-propagation step has to be built. This directly grounds Subsystem D's Move 2/4 staleness work (§3).

### ★ #4 — Round-Trip Solvency & Stop Gate (the economics arbiter)

The decision a human never faces and an agent faces most.

- **Composes:** `focus`/`seek` sufficiency verdict + honest `ignored` tail × `coverage_session` (what I've paid to load) × a real remaining-**token**-budget signal × `mission_next` do-not list × `am_i_stale` (how much of what I hold is already invalid).
- **New question:** *"Given my remaining budget and what I've loaded, (a) is there a path to a PROVEN close or will I run dry mid-edit, and (b) is one more m1nd call worth it — 'saturated, you're paying to re-confirm what you know, STOP' — or should I re-scope / hand off NOW?"*
- **Why it matters for an agent:** this is the cost law made actionable *before* the agent commits. Stopping too early ⇒ wrong action; too late ⇒ burn the budget. An agent that detects "I'll run dry 3 round-trips short" re-scopes to a provably-completable slice instead of dying with half the call-sites converted.
- **Composition defect the critic found — corrected, not hidden (see §O.5):** the naïve formula *"diff `budget_consumed` against `relevance_clearing_total` minus `coverage_session`"* **composes incompatible units** and is wrong. `budget_consumed` is a **tool-CALL-count fraction** that requires an open mission; `relevance_clearing_total` is a **node count**; `coverage_session` is **visited-files** — they do not subtract. The Solvency Gate therefore needs a **real token-budget signal** wired in (or that signal is **net-new** and must be built), not an arithmetic of three mismatched scalars. The honesty floor still holds (`focus`'s `ignored_count` can't read zero under truncation; `mission_verify` refuses graph-only "verified"), so the gate cannot be talked into a false "enough" — but its budget arithmetic must be made unit-correct before it ships.

### Strong secondary unlocks (sharpened, lower rank)

- **#5 — Negative-Space Bug Localizer.** `missing` × `ghost_edges` × `taint_trace` boundary_misses × `antibody_scan` negative-edges × `tremor`. *"Where is the defect grep/RAG can NEVER surface because the evidence is an ABSENCE + a temporal coincidence?"* Every candidate carries WHY it's an absence (checkable).
- **#6 — Reality-Weighted Blast Radius.** `runtime_overlay` (OTel heat) × `impact`/`epidemic` × `taint_trace` × `trust`/`tremor`. *"Of everything my change could break, what does REAL traffic actually hit?"* **Honest prior art (baked in):** runtime-heat-weighted blast is **Sourcegraph + OTel** territory — OMEGA's angle is doing it over the re-derivable graph with `spans_unmapped` honesty, highest external dependency (needs a span batch fed in).
- **#7 — Counterfactual Sequencing Planner.** `counterfactual` (synergy matrix) × `epidemic` (diffusion order) × `predict`/`ghost_edges` × `refactor_plan` (min-cut safe-first seam) × `flow_simulate`. *"In what ORDER do I touch these nodes so I'm never in a broken intermediate state, and which edits are atomic vs order-free?"* The sequencing logic is real net-new work.
- **#8 — Concurrent-Swarm Collision Forecast** (the only multi-agent unlock). `predict`+`ghost_edges` × `perspective_list`/`trail_list` (other live agents' cursors) × `daemon_status`/`alerts_list`. *"Is the seam I'm about to cut temporally coupled to a file another live agent currently has under its cursor?"* Turns m1nd into the **coordination substrate for an agent swarm** — and dovetails directly with Subsystem A's multi-agent-by-default (§3).
- **#9 — Evidence-Graded Self-Handoff Receipt.** `mission_close`/`mission_handoff` × `mission_verify` (per-claim direct-vs-graph-only grade) × `cross_verify` (live freshness stamp) × `coverage_session` (blind-spot map) × `trail_save` × `xray_ledger`. *"Hand the next agent not my conclusions but re-runnable receipts, each tagged with evidence-grade + freshness + the blind spots I never touched."* Cures the SWE-CI continuity collapse and satisfies the NIST/EU-AI-Act audit-ledger requirement at once.

## O.4 The OMEGA architecture — composition as a first-class citizen

Each superpower is *several* m1nd calls an agent must orchestrate by hand, paying the cost law each round-trip. OMEGA's architectural leap is **making the composition the product**, reuse-first over what already exists.

### O.4.1 The "north" contract — every answer is a triple

Standardize one envelope that *every* answer-emitting tool returns, reusing fields that already exist (`proof_state`, `non_claims`, `closure`, `ignored`, `binding_fingerprint`, `event_digest`):

```
{
  answer:  <the existing tool output, unchanged>,
  map:     { blast_radius, co_change, reach — the structural neighborhood },
  receipt: { evidence: [file:line@sha256], closure, evidence_class,
             binding_fingerprint, event_digest, freshness_verdict },
  trust:   { verdict: act|reverify|abstain|unprovable,
             reasons[], next_repair_call }
}
```

This is a content-addressed graph wrapped in a typed-relation evidence layer with explicit honesty states. **It is not optional and not a separate call** — answer + map + trust arrive together, collapsing the human "definition → references → callers → tests → is-this-fresh" click-chain into one batchable round-trip.

### O.4.2 A signal-chaining planner (`north`), not a query DSL

A consistent SOTA finding is that **LLMs hallucinate raw graph queries** — a DSL is a hallucination trap. So OMEGA exposes **fused cognitive verbs**, not a language. Reuse the orchestrator pattern m1nd already ships in `audit` (which fans health + panoramic + layers + scan_all + cross_verify + fingerprint + trust + tremor + ghost_edges into one report) and `orient`. Add a small set of **named compositions** as new verbs — `underwrite(edit_plan)` → #2, `envelope(any_answer)` → #1, `quarantine_on_boot()` → #3, `solvency(goal, budget)` → #4, `negative_space(topic)` → #5, `swarm_collision(node)` → #8, `handoff_receipt()` → #9. Each verb fans out the existing tools **in parallel**, dedupes into one budget-bounded bundle, and returns the §O.4.1 triple. The agent never sees the chaining; it sees one verb, one receipt.

### O.4.3 Query the honesty signals, not the graph

The composable layer agents actually need is a tiny **predicate surface over the receipt**, so a CI gate or a successor agent can mechanically filter (`trust(answer) == act`, `freshness(claim) == fresh@HEAD`, `closure(path) == closed`, `evidence_class(claim) == direct`). These are already computed by `trust_selftest`/`cross_verify`/`why`/`mission_verify`; OMEGA surfaces them as a stable, content-addressed schema — the M+N collapse (LSP's lesson): M producers emit receipts, N agents/gates consume them, standardized at the cheap-to-agree level (symbols, positions, edges, provenance), never at the impossible level (a universal AST).

### O.4.4 Reuse-first discipline

Nothing above invents a new engine. `event_digest` already content-addresses; `grounded_in` edges already anchor claims; `xray_ledger` is already append-only and reversible; `binding_fingerprint` already crosses host/stdio/HTTP. **OMEGA is wiring + one envelope schema + ~9 fused verbs**, each the smallest clear composer over signals that ship today.

## O.5 Honesty about novelty (the critic's correction, stated plainly)

OMEGA earns nothing by overclaiming, so this section is explicit rather than buried. **Drop the blanket "unprecedented."** What is *genuinely* new in OMEGA is narrow and defensible:

1. **The answer + map + trust triple in ONE round-trip** — the composition itself, not any single signal.
2. **The sufficiency / solvency economics** — *"do I have enough / can I afford to finish?"* — agent-native questions with no human-tool analog.
3. **The re-derivable receipt over a CODE graph** — `file:line@sha256` anchors that resolve or don't, self-falsifying by construction.

Equally explicit, the **prior art OMEGA stands on** (and does not pretend to have invented):

- *Taint-to-sink-without-a-validation-boundary* IS **CodeQL's** core query shape.
- *Transitive blast radius* is **Glean / Sourcegraph** territory.
- *Bi-temporal belief quarantine* is a **port of Zep / Graphiti** invalidate-not-delete onto a code graph.
- *Runtime-heat-weighted blast* is **Sourcegraph + OTel**.

OMEGA's honest framing is **"first over a re-derivable, self-falsifying CODE graph,"** *not* "nobody ever did this." A vision that survives the critic is the only one worth banner-ing.

## O.6 Move 0 is the keystone: calibration before any verb earns `act`

**This is the single most important correction to the roadmap.** Structural honesty — evidence re-hash, null-not-0.5, reject-graph-only — proves self-**CONSISTENCY**, **never CORRECTNESS.** A receipt can be perfectly re-derivable and **confidently WRONG**: every probe green, every hash matching, and the underlying claim still false (e.g. a closure that looks "closed" because m1nd doesn't extract the one relation type that actually dangles). A consistency proof is not a correctness proof, and an agent that trusts `act` is trusting *correctness*.

So the roadmap is **inverted**: the first thing OMEGA ships is not a verb, it is the **harness that decides when a verb is allowed to say `act`.**

- **Move 0 = a conformal precision-at-coverage calibration harness.** Every OMEGA verb ships **DARK** and runs against a **standing eval set** — e.g. **CWE-Bench taint** (does the taint verdict match labeled vulnerabilities?), **SWE-CI regression replay** (did `underwrite`'s "proceed" actually avoid the regression the replayed CI caught?), and a **held-out co-change corpus** (did `predict`/quarantine's co-change propagation match ground-truth coupling?).
- A verb only **earns `act` as an allowed output value** once the calibrator **certifies that `act` empirically clears a stated risk budget** (a measured precision at a measured coverage, against ground truth — not an asserted one). Until then the verb can emit `reverify` / `abstain` / `unprovable`, but **`act` is structurally withheld.**
- **Reframe the whole sequencing rule:** what the rest of this PRD calls **"battery-gated"** becomes **"calibration-gated"** for OMEGA verbs. Battery tests prove the code does what it says (consistency); the calibrator proves the verdict is *right often enough to act on* (correctness-at-coverage). OMEGA needs **both**, in that order, and the calibrator is **Move 0** — it gates every verb after it.
- This is the durable-maintenance bet stated as a build step: **recalibration, not retraining.** Standing eval harnesses wired to a conformal calibrator mean claimed error rates are continuously **re-measured** against ground truth. *Calibration asserted in a README rots; calibration measured endures.* `trust_selftest` must stay real, never cosmetic.

## O.7 The honesty invariants (the non-negotiables that earn trust)

These are the floor. Break any one and the 20-year thesis dies.

1. **Re-derivability over recall.** Every claim is reconstructable on demand from primary reality (code AST + git), never served from a cached belief. Memory is an index/cache, **never** source of truth (`code > PATHOS > memory`).
2. **Abstention is a first-class, rewarded output.** OMEGA must reliably say **UNPROVABLE / I won't answer this** under an explicit risk budget — and be rewarded for it. Agents pay for a *known* precision-at-coverage over a 100%-coverage tool of unknown reliability.
3. **Structural refusal, not heuristic restraint.** `mission_verify` *structurally rejects* graph-only evidence; `focus`'s `ignored_count` *cannot* read zero while truncation happened; `trust` emits **null, not a fake 0.5**; `xray_gate` *only blocks on a RATIFIED manifest*. The honesty floor is in the data path, not a policy the composer could fudge. **But (per §O.6) structural refusal proves consistency, not correctness — calibration is the second, separate gate.**
4. **Citations are graph anchors, not prose.** OMEGA's "citations" are `file:line@sha256` nodes that resolve or don't — verifiable by construction, in a different trust class than generated text.
5. **Bi-temporal invalidation.** Knowledge carries validity intervals; superseded facts are marked invalid (re-hashed `grounded_in` mismatch), never silently overwritten.
6. **Self-localizing failure.** When OMEGA is wrong, the provenance graph localizes the bad step and supports selective invalidation of exactly the affected claims — not a full reset.
7. **Document the lies.** The census's own honest notes (`report`'s heuristic "savings," `trust`'s false guidance string, perspective's cosmetic route-families) stay documented and get *fixed*, not hidden.

## O.8 Open RISK — the poisoned-oracle threat model (NOT yet solved)

This is flagged as an **open design problem, not a solved invariant** — stating it as solved would itself violate the honesty moat.

**If OMEGA becomes the thing agents trust 100%, it is the single highest-value attack surface in the toolchain.** The dangerous case is a **poisoned graph that passes its own self-test**: a malicious co-change history that poisons `predict`/`ghost_edges`, or planted evidence that re-hashes cleanly, so the receipt is internally consistent *and wrong by construction*. `trust_selftest` proves *binding* integrity (right process / binary / graph / generation) — it does **NOT** prove the *content* of that graph wasn't adversarially seeded. The append-only `xray_ledger` records *that* a bulk write happened, not whether the writer was honest.

Concretely unaddressed today:
- A poisoned co-change corpus makes `predict`/quarantine confidently propagate a *wrong* coupling — and §O.6's calibrator only catches it if the eval set is itself uncompromised (who calibrates the calibrator?).
- A self-consistent-but-false receipt is exactly the §O.6 "consistent ≠ correct" failure, weaponized.

**Open directions (not commitments):** signing the eval/ground-truth set; independent re-derivation by a second, differently-bound instance (quorum over `binding_fingerprint`); anomaly detection on co-change history before it feeds `predict`. **`trust_selftest` alone is not sufficient** to close this, and OMEGA must not be marketed as if it were. The threat model is named here so it is designed against, never assumed away.

## O.9 20-year bets (what makes it durable like git / LSP / SQLite)

The durable-moats recipe is not "be best" — it is **format + narrow interface + openness + verifiability.**

- **Freeze the FORMAT, version the engine freely.** The durable asset is the **receipt/graph interchange format** (the §O.4.1 triple as a SCIP-style protobuf with human-readable symbol IDs), pledged backward-compatible for decades. An org's provenance-anchored memory graph must be re-readable in 20 years with zero migration.
- **Content-addressed, deterministically re-derivable.** Any party re-indexes the same commit and gets a bit-identical graph; any single edge is verifiable by its path-hash. **Trust never requires trusting the vendor.**
- **File-incremental + error-tolerant substrate.** Re-index only changed files per commit, stay valid mid-edit, answer in ~ms. `am_i_stale`/`diverge` become the live trust signal on top.
- **Narrow, composable, hallucination-proof verbs over an open protocol** (§O.4.2), never a query DSL. Permissive license + broad language coverage is *why* the niche stays adoptable.
- **Strict LLM/symbolic separation of duties.** The LLM is permanently confined to the SPEC/HEURISTIC layer; soundness *always* belongs to a deterministic certifier (graph traversal, Datalog closure, SMT, executed tests). This boundary survives model churn — swap the model **without re-earning trust**.
- **Recalibration, not retraining, as maintenance** (the §O.6 Move 0 bet, made permanent): standing eval harnesses wired to a conformal calibrator keep claimed error rates continuously re-measured.
- **Switching cost = earned, not hostage.** Lock-in comes from the deep, verified, code-anchored memory graph — but because the format is open and re-derivable, that lock-in is **earned trust.**

## O.10 Ranked OMEGA roadmap (calibration-gated, reuse-first, honest)

> **Sequencing note.** Move 0 is the keystone (§O.6) and gates `act` for every verb after it. Each later move ships **calibration-gated** (DARK until the calibrator certifies its `act`-at-coverage clears the budget), **reuse-first** (a composer over shipping tools, never a new engine), and **honest** (degrades to UNPROVABLE, never a fake green light). The "% exists" estimates from the source briefs are **deliberately dropped** here — they conflated "code present" with "calibrated and correct," which §O.6 forbids.

- **Move 0 — The calibration harness (keystone, ships FIRST).** A conformal precision-at-coverage calibrator + standing eval sets (CWE-Bench taint, SWE-CI regression replay, held-out co-change corpus). No verb earns `act` until it certifies against a stated risk budget. Everything below is gated by this. **Cutting v1.2.0 requires Move 0 + Move 1.**
- **Move 1 — The Envelope (`envelope` / the §O.4.1 triple).** Wrap existing answers in answer + map + trust by composing `trust_selftest` × `cross_verify` × `am_i_stale` × `why`-closure × `mission_verify`. **The gate is a calibrated weighting, not an any-red AND-fold** (§O.3 #1) — tuned against ground truth before defaulting on, to avoid the ~23% spurious-abstention failure. Substrate for every later move.
- **Move 2 — Solvency & Stop Gate (`solvency`).** Arbiter over `focus` sufficiency + `coverage_session` + a **real token-budget signal** (the unit-mismatch fix from §O.3 #4 — `budget_consumed`/`relevance_clearing_total`/`coverage_session` do **not** subtract; wire a true token budget or build it net-new) + `am_i_stale`. Directly attacks the cost law.
- **Move 3 — Stale-Belief Quarantine on Boot (`quarantine_on_boot`).** Boot-time fan-out of `cross_verify` (the real cross-session freshness source — **not** `am_i_stale`, §O.3 #3) × `diverge(last_session)` × `trail_resume` × `alerts_list` into a three-state partition + do-not-re-claim list. **Co-change-matrix propagation is net-new** (`ghost_edges` exposes 3 scalars, not a matrix). Grounds Subsystem D's staleness moves.
- **Move 4 — Provable-Edit Underwriting (`underwrite`).** Fuse `validate_plan` × `predict`+`ghost_edges` × `xray_gate` × `antibody_scan` × `trust`/`tremor` × `am_i_stale` into `{proceed | require-human | abstain}` + minimal-cover set. `proceed` calibration-gated against SWE-CI regression replay. The single biggest correctness win.
- **Move 5 — Evidence-Graded Self-Handoff (`handoff_receipt`).** Fold `coverage_session` blind-spots + `trail_save` contradictions + per-claim freshness into `mission_close`'s packet, threaded through `xray_ledger`. Satisfies the audit-ledger requirement; cures continuity collapse.
- **Move 6 — Negative-Space Bug Localizer (`negative_space`).** Cross-rank `missing` ∩ `ghost_edges` ∩ `taint_trace` boundary_misses ∩ `antibody_scan` negative-edges, weighted by `tremor`.
- **Move 7 — Reality-Weighted Blast Radius (`hot_blast`).** Re-rank `impact`/`epidemic` by `runtime_overlay` heat × `taint_trace` × `trust`. Ship the structural-only fallback first; light up runtime weighting when a span batch arrives (highest external dependency).
- **Move 8 — Swarm Collision Forecast (`swarm_collision`).** Intersect `ghost_edges`/`predict` coupling with `perspective_list`/`trail_list` live footprints. Lands once Subsystem A's parallel-agent usage is real.
- **Then — durability hardening (the §O.9 bets):** freeze the receipt/graph format + back-compat pledge; cut over to file-incremental re-index. At ~**v2.0** the era graduates into a separately-named product.

## O.11 Manifesto

**m1nd-OMEGA is the substrate an agent routes every code decision through and never has to second-guess.**

Other tools answer *what the code is*. OMEGA answers the questions only an agent asks: *Do I have enough to act? Can I afford to finish? Did reality move under me? Is this answer's evidence still alive, or am I about to commit against a ghost? Should I do this, ask a human, or refuse?*

It earns trust the only way trust survives twenty years — not by being confident, but by being **checkable** *and continuously re-measured against ground truth*. Every answer ships its map, its receipt, and its verdict in one round-trip. Every claim is a `file:line@sha256` an agent can re-open. Every belief expires the instant its evidence rots — loudly, on proof, never on a guess. When OMEGA doesn't know, it says **UNPROVABLE** and routes you to the call that would make it know. And because a re-derivable receipt proves consistency but not correctness, **no verb is allowed to say `act` until a standing calibrator has earned it that right** — and even then, the poisoned-oracle problem stays named and unfinished, not waved away.

It is git's content-addressing, LSP's M+N collapse, and SQLite's frozen format — pointed at the one consumer that pays per round-trip, has no working memory, and abandons any tool that burns it once. The LLM proposes; the graph, the hashes, the solvers, and the calibrator certify; and the whole thing **refuses to lie**, including about itself.

Honesty is not a feature of m1nd-OMEGA. **Honesty is the moat.** Everything else is wiring.

---

# Ω+1 — The Ambient Loop

> **The next chapter, building on OMEGA — not a replacement.** OMEGA (§O.1–O.11) makes every *answer* carry its own trust receipt. **Ω+1 makes the answer arrive whether or not the agent remembers to ask** — it wires m1nd into the agent's hook lifecycle so orientation, staleness-guarding, and memory become *ambient* rather than called. Same honesty moat, one axis further: OMEGA hardened the verb; Ω+1 hardens the *loop the verbs run in*. This section is held to the identical standard — every claim written to survive the adversarial critic, and the critic's corrections **baked in, not bolted on**. The design is presented WITH its structural defects surfaced (four are load-bearing and corrected below), because a doc that hides its own broken keystone would itself violate the moat.

## Ω+1.1 Thesis

m1nd today is a **tool you call.** The agent must remember to `orient`, remember to `am_i_stale`, remember to `memorize`. Every one of those "remember to" is a leak — and the frontier now measures exactly what leaks through it: **EvoClaw drops the best agent from >80% on isolated tasks to 38% on continuous evolution**, and the collapse point *is* the agent failing to honestly build on its own prior state. The delegation gap (AI touches ~60% of work; humans fully delegate 0–20%) has the same root: an agent that boots cold, edits stale bytes, and forgets what it proved cannot be trusted over long horizons.

The move is to stop making m1nd a node the agent *chooses* to visit and make it **the wire the agent's loop runs on**. Four beats, wired into the hook chokepoints every agent already passes through:

> **pre-orient → act → post-capture → compound**

- **Pre-orient** — before the agent acts, m1nd hands it an honest *north packet*: a trust verdict on its own binding, a ranked minimal context set, prior conclusions with age + author + staleness, and a sufficiency stop-signal. The agent never starts blind.
- **Act** — the agent works. m1nd is dark. Silence is a feature.
- **Post-capture** — after the action, m1nd folds the change back: re-ingests the delta, surfaces co-change the AST can't see, reweights trust/tremor from real test outcomes, and — the keystone — **memorizes what was proven, anchored to code.**
- **Compound** — `Stop:memorize` writes exactly what the next `SessionStart:orient` reads. The loop tightens across sessions. **Leaving m1nd stops meaning "lose a feature" and starts meaning "lose institutional memory."** That is the LSP-of-agent-ground-truth moat: not smarter per fire, but *ambient, nearly-free, and compounding.*

This is not new capability bolted on. It is **choreography of verbs that already ship** — the one primitive m1nd lacks is the wire itself. The hook is a thin `stdin-JSON → MCP-call → additionalContext/permissionDecision` shim (`session_id → agent_id`); no new engine.

## Ω+1.2 The lifecycle map

Every verb below is live in `mcp__m1nd__*`. **PRE** = orient before acting (draws from the retrieval + trust families). **POST** = capture after acting (draws from the memory-l1ght + temporal + xray families). `Stop:memorize` writes exactly what `SessionStart:orient` reads — the one loop grep/LSP/RAG structurally cannot close.

| Hook moment | m1nd verb(s) that fire | PRE / POST | Payload in → returns to agent |
|---|---|---|---|
| **SessionStart** (`startup\|resume\|clear\|compact`) | `trust_selftest` → `orient(cwd/last-goal)` → `boot_memory(get)`; on `resume`: `+am_i_stale`(coverage set) `+trail_resume` | PRE | `session_id, cwd, source` → `additionalContext`: trust_mode + binding fingerprint, focus_nodes, memory_nearby (age+author+STALE flags), PageRank anchors, coverage, first move. **`orient` is heavy (PageRank) — see Correction 2: it must NOT block on `compact`.** |
| **UserPromptSubmit** | `focus(goal, budget)` + `warmup(task)` | PRE | `prompt` → `additionalContext`: minimal `focus_set[]` (file:line + excerpt), `sufficiency` verdict (`sufficient\|gathering\|saturated`), honest `ignored{count,reason}` tail |
| **PreToolUse** (Edit/Write/MultiEdit) | `am_i_stale(file)` → on stale `surgical_context_v2` + `memory_nearby`; `xray_gate` (ratified → `deny`); `validate_plan` (blast radius) | PRE (gate) | `tool_input.file_path` → `permissionDecision` or `additionalContext` **caution (see Correction 3 — caution by default, `ask` only on a file THIS agent read this session)** |
| **PreToolUse** (Read/Grep/Glob) | `surgical_context_v2` / `seek` enrich | PRE | `tool_input` → `additionalContext`: real caller/callee/test neighborhood + risk read (never blocks) |
| **PostToolUse** (Edit/Write) | `ghost_edges` → `predict(node)` gated by calibration verdict; `daemon_tick` / incremental `ingest` re-sync | POST (fire-and-forget) | `tool_input, tool_response` → `additionalContext`: co-change files you haven't touched (only when verdict = `act`; `abstain` on an uncalibrated graph — **Correction 3**) |
| **PostToolUse** (Bash: test/build) | `learn(query, feedback: correct\|wrong\|partial, node_ids)` | POST | pass/fail from `tool_response` → side effect: trust + tremor + co-change ledger updated (silent on pass) |
| **SubagentStop** | `mission_verify` (evidence-class gate) → `mission_handoff` | POST | transcript, last_msg → typed proof packet to parent; `decision:block` on graph-only evidence. **`mission_*` only fires when a mission is genuinely open — see Correction 1.** |
| **Stop** | **`cross_verify(evidence_freshness)` → `memorize(claims, evidence)` DIRECTLY** (NOT `mission_verify`→`mission_close`) | POST (fire-and-forget) | turn conclusions → `.light.md` written, code-anchored + stale flags. **This is the rewired keystone — see Correction 1.** |
| **PreCompact** | `memorize` (rescue durable findings) + `trail_save` | POST | `trigger` → durable findings + in-flight trail flushed before forgetting |
| **SessionEnd** | `persist(save)` + `boot_memory(set)` + `alerts_ack` | POST | `reason` → side effect: snapshot learned weights |

The through-line: **PRE hooks hand a ranked, trust-verdicted north packet; POST hooks fold the change back** (re-ingest, reweight via `learn`, surface co-change via `predict`, memorize anchored to code). Every hook is an MCP round-trip, so the map is only honest once the latency budget below (Correction 2) is met.

## Ω+1.3 The four load-bearing corrections (baked in, not bolted on)

The synthesis converged on a clean design — and the adversarial critic found four structural defects in it. They are surfaced here, not footnoted, because the ambient loop is a **verification tool that fires on every action**, and such a tool dies the instant it is slow, cries wolf, or launders a guess. Each correction is enforced in the data path, not asserted in prose.

### Correction 1 — the keystone is structurally broken as a hook; rewire it (MANDATORY)

The synthesis's keystone was `Stop → mission_verify → mission_close → memorize`. **This does not compose as a hook.** Grounded in source: `handle_mission_verify` (`m1nd-mcp/src/mission_handlers.rs:200`) and `handle_mission_close` (`:309`) both open with `load_mission(state, &input.mission_id)?` — they **require a `mission_id`** from a prior `mission_start`, and `load_mission` (`:454`) **hard-errors** (`InvalidParams`, `:458`) when no such mission file exists. But `Stop` fires on **every turn end**, and **almost none** carry an open mission. Wiring the keystone as written would make m1nd throw an error on the overwhelming majority of turns.

**The composable keystone is `Stop → cross_verify(evidence_freshness) → memorize(claims, evidence)` DIRECTLY.** `memorize` (`handle_light_author`, `m1nd-mcp/src/light_author_handlers.rs`) takes **free-form structured claims with `evidence` paths and needs NO `mission_id`** (verified: its input is `{claims: [{claim, evidence[]}], …}`, no mission field). It writes a graph-native `.light.md`, ingests it, and anchors every evidence path to the real code node — exactly the compound-arc write we need, with none of the mission precondition. `cross_verify(check:["evidence_freshness"])` runs first so a born-stale claim is flagged born-stale at write time. **Reserve `mission_*` for `SubagentStop` and for the rare turn where a mission is genuinely open** — never as the default `Stop` path.

### Correction 2 — latency is asserted, not measured; make it a budget

Every PRE/POST hook is an MCP round-trip. A 12-edit refactor pays **12× `am_i_stale` + 12× `ghost_edges`→`predict`** at `PostToolUse` alone. And `orient` — which composes PageRank (the heaviest verb in the surface) — is wired to `SessionStart`, which fires on `compact` **mid-session**, potentially blocking the agent's own context-window recovery. "Sub-100ms, nearly-free" is a claim the synthesis **asserts**; nothing measures it. Requirements, enforced not asserted:

1. **A stated per-hook latency budget** — a wall-clock ceiling per hook class (PRE-gate strictest, POST-capture loosest), MEASURED against real repos, published in the receipt, not a marketing "sub-100ms."
2. **Caching keyed to `graph_generation`** for the common "nothing changed" path — the overwhelming majority of `am_i_stale` fires hit an unchanged file; those must return from cache in ~one hash compare, no graph traversal.
3. **Fire-and-forget async for all post-capture** — `Stop:memorize`, `PostToolUse:predict`, and the re-ingest tick must NOT block the agent's next turn; they run detached and surface on the *next* PRE hook.
4. **`orient` must NOT fire blocking on every `SessionStart`** — on `compact` especially, it either runs async or degrades to the cached North Packet from the last full orient. The heaviest verb never sits in the synchronous critical path.

The moment a fire adds perceptible latency per turn, the operator `--no-verify`'s it out — and disabling is sticky in the wrong direction. **m1nd's ambient fire must behave like a pre-commit formatter, not a test suite.**

### Correction 3 — `stale_guard → ask` is a wolf-cry; caution by default

The synthesis wired `am_i_stale` mismatch → `permissionDecision: ask` (a hard block). This **cries wolf.** A proven hash mismatch fires on entirely benign churn: a formatter reflowing the file, a branch switch, a sibling session's write (Max's own documented multi-session worktree drift). Block on all of those and the agent learns the guard is noise and routes around it — the SOC 65%-ignored-alerts collapse, imported into the inner loop. Corrections:

- **Stale is `additionalContext` CAUTION by default**, never a block. It tells the agent "this file changed on disk since ingest" and lets the agent decide.
- **Blocking `permissionDecision: ask` fires ONLY on a mismatch to a file THIS agent actually READ this session** — i.e., the agent is about to edit against a picture it personally holds and that has since drifted. That is the one case worth interrupting for; every other mismatch is caution.
- **The co-change / `predict` gate must auto-trigger `calibrate_predict`** or it is dead weight forever. On a fresh repo `predict` is honestly `abstain` (Move 0: uncalibrated ⇒ every verdict `abstain`), and it *never earns its way on* unless calibration runs against that repo's git history automatically. Without the auto-trigger, `cochange_nudge` ships silent and stays silent — a feature that never turns on.

### Correction 4 — the one fabrication risk lives in the auto-memorize distiller

`memorize`'s `direct`-evidence gate is right: a claim whose `evidence` paths don't resolve to a real code node is flagged `unresolved` (verified in `light_author_handlers.rs` — the handler counts `light_evidence_resolved`/`light_evidence_unresolved` and guides the agent to ingest). But the **extraction step feeding `memorize` at `Stop` is the soft spot.** If the distiller **free-LLM-summarizes the turn**, it can fabricate a memory — invent a conclusion the turn never actually reached, then persist it with authority. That is the single worst failure the ambient loop can have: a grounding tool that launders a hallucination into durable, auto-loading memory.

**The distiller must anchor claims to evidence paths (source-of-truth), not paraphrase the transcript.** It extracts claims **only** where the turn touched real code nodes (edited/read `file:line`), cites those nodes as `evidence`, and lets `memorize`'s resolve-or-flag gate reject anything that doesn't anchor. A claim with no resolvable evidence is **not written** (or written `unresolved`, never silently grounded). The rule: *memorize what the turn proved against code, never what a summarizer thinks the turn meant.*

## Ω+1.3b Empirical validation — the first A/B (real data, honest verdict)

> **This is not a thought experiment anymore.** An isolated A/B ran the `pre-orient` beat against a real headless `claude -p` agent, injecting the `north` packet via a Claude Code hook. It settles two of the four beats with measured data — and, more importantly, it PROVES the compounding beat is architecturally blocked in the naïve hook setup, which rewrites the first ambient milestone below (§Ω+1.4). Isolation held throughout: Max's global config was untouched; the experiment ran in a sandboxed hook + config.

**What the A/B tested.** Two arms on the same task against the same repo: **Arm A (control)** = the agent with no hook; **Arm B (treatment)** = the same agent with a `SessionStart`/`UserPromptSubmit` hook that injects the `north` pre-flight packet (the §Ω+1 pre-orient beat — trust verdict + ranked minimal context + prior conclusions + sufficiency) into `additionalContext`. 3 runs per arm.

**GREEN — the pre-orient beat helps orientation, and does not confuse or hinder:**
- **Hooks FIRE in `claude -p` headless.** Proven with a canary probe — the hook executes and `north` is delivered to the model **verbatim**. ~110 ms per fire. Fail-open by construction: a hook error never blocks the agent's turn. (This retires the "does the wire even exist in headless?" unknown — it does.)
- **It RETARGETS the first move, correctly.** Arm B opened the **correct file first — `config.py`, the `pr=1.00` anchor `north` pointed at — in 3/3 runs, versus 0/3 for the control.** Directional first-move retargeting is real: the north packet measurably steers the agent's opening move toward the right node.
- **It does NOT confuse or derail.** No wrong turns were induced by the packet. When `north` suggested a tool the agent didn't have, the agent **ignored the suggestion without derailing** — the packet is advisory context, not a command the model over-obeys. The only cost is mild: ~**1.7 more tool-calls** on average in Arm B (more reading, following the orientation), no correctness loss.

**HONEST NULL / caveats — what the A/B did NOT prove:**
- **The task was too easy to show `north` RESCUING a run.** BOTH arms succeeded 3/3. The A/B proves pre-orient helps *orientation* (first-move targeting) and does no harm, but it does **not** yet show the packet turning a failure into a success. That needs a **harder task** where a cold agent plausibly fails without the north packet — the missing "rescue" experiment.
- **COMPOUNDING MEMORY is an HONEST NULL — and it is ARCHITECTURALLY BLOCKED, not merely unmeasured.** This is the load-bearing finding. `north`'s graph is **in-process**, and **each hook fire is a separate short-lived process** (process-per-hook). So a post-capture `memorize` writes to a graph that the *next* fire — a fresh process — never reloads; and each fire **re-ingests the repo from scratch** (72 ms here, but this would DOMINATE latency on a large repo). In the process-per-hook + in-process-graph setup, **the `pre-orient → … → compound` loop cannot close**: there is no persistent graph across fires to compound into. This is not a bug to patch in the hook; it is the wrong substrate for the compounding beat.

**THE CORRECTED PREREQUISITE — the real first ambient milestone is serve/attach, NOT the hook.** The insight the A/B forces: the ambient loop's true prerequisite is not "write a hook," it is a **persistent, live graph the hook ATTACHES to** instead of re-ingesting per fire. m1nd **already has this** — the `--serve` / `--attach` mode shipped at #157/#158 (Subsystem A's live-owner + attach bridge). Wire the hook to a **served m1nd** and two problems dissolve at once: (i) the per-fire re-ingest latency is gone (attach is zero-graph, zero re-ingest — it shares the owner's live `Arc<RwLock<Graph>>`), and (ii) cross-fire compounding becomes possible (a `Stop:memorize` on the served graph is visible to the next `SessionStart:orient` on the *same* graph). **This is now the FIRST ambient milestone — before any hook is installed in a live environment:** stand up a served m1nd, attach the hook to it (not re-ingest), and re-run the A/B to validate (a) compounding across fires, (b) latency-at-scale on a large repo, and (c) a harder rescue task. **Recommendation: do NOT install the hook into Max's live env yet.** The naïve process-per-hook shim proves the concept (pre-orient helps, no harm) but cannot deliver the moat (compounding) and pays re-ingest latency at scale — the serve/attach validation gates the live install.

This does not weaken the §Ω+1 design; it **grounds it**. The pre-orient beat is validated (helps, no harm). The compound beat's prerequisite is now known and named: the hook must attach to a persistent graph, which is exactly Subsystem A's already-shipped `--serve`/`--attach`. The Wave roadmap below is re-anchored on it.

## Ω+1.4 Ranked reuse-first roadmap (Waves)

Mirror how OMEGA shipped: a small honest primitive, gated so it abstains until it earns the right to speak, wired then hardened. Each wave is independently shippable and leaves the loop honest. **ORGANIZE** = wire a verb that already does the work (`auto_ingest`, `daemon`, `focus`, `orient` already exist — the daemon self-wakes, the watcher captures FS events, `orient` already composes activate + memory_nearby + PageRank + coverage). **BUILD** = net-new logic.

- **Wave 0 — the shim harness ATTACHED to a served m1nd (enables everything, BUILD; EMPIRICALLY RE-ANCHORED — see §Ω+1.3b).** One tiny `stdin-JSON → m1nd-MCP → hook-output` client. No m1nd code. **The first A/B proved the shim must attach to a persistent `--serve` graph, NOT re-ingest per fire:** process-per-hook + an in-process graph re-ingests the repo every fire (dominates latency at scale) and **cannot compound** (each fire is a fresh process, so a `Stop:memorize` is invisible to the next fire). So Wave 0's real deliverable is **hook → `--attach` to a served owner** (Subsystem A's already-shipped #157/#158 bridge) — zero re-ingest, one live graph across fires. Latency-budgeted (Correction 2) from line one. **This wave gates the live install: do NOT wire hooks into Max's live env until the served-attach variant validates compounding + latency-at-scale + a harder rescue task.**
- **Wave 1 — pre-orient North Packet on SessionStart (ORGANIZE).** Wire `SessionStart → trust_selftest → orient → boot_memory`. Highest leverage-per-line because `orient` already aggregates. `trust_selftest` fires **first** — a split-brain/wrong-workspace binding injects `recovery_playbook`, not a confident lie. `orient` runs async on `compact` (Correction 2).
- **Wave 2 — the fresh-context PreToolUse enrich + stale caution (ORGANIZE).** Wire `PreToolUse(Read/Grep) → surgical_context_v2` (never blocks) and `PreToolUse(Edit) → am_i_stale`. Stale = **caution by default**, `ask` only on a file this agent read this session (Correction 3). Surface `memory_nearby` at edit-intent so prior conclusions arrive unbidden.
- **Wave 3 — the honest post-capture (ORGANIZE, fire-and-forget).** `PostToolUse(Edit) → daemon_tick`/incremental re-ingest + `PostToolUse(Bash:test) → learn(feedback)`. Silent on pass. This keeps the graph and trust model current without an agent call.
- **Wave 4 — the keystone auto-memorize, REWIRED (BUILD).** `Stop → cross_verify(evidence_freshness) → memorize(claims, evidence)` **directly** (Correction 1 — never `mission_*`). Ship the distiller thin and **evidence-anchored** (Correction 4): extract claims only where the turn touched real code nodes, cite them, let `memorize`'s resolve-or-flag gate reject the rest. Pair with `PreCompact → memorize + trail_save`. After this wave, `Stop` writes what `SessionStart` reads — the flywheel turns.
- **Wave 5 — the calibration-gated co-change nudge (ORGANIZE + auto-calibrate).** `PostToolUse(Edit) → ghost_edges → predict`. Ships **silent** on an uncalibrated graph (honest `abstain`), and **auto-triggers `calibrate_predict`** against the repo's git history (Correction 3) so it earns its way on. Becomes loud only after a measured precision-at-coverage receipt — the OMEGA discipline exactly.
- **Wave 6 — the swarm handoff gate (BUILD).** `SubagentStop → mission_verify → decision:block` on graph-only evidence — the one place `mission_*` genuinely belongs (a subagent whose whole job was a scoped mission). Collapse `PostToolBatch` drift into one digest, never twenty flags.

The ordering is deliberate: **ORGANIZE waves ship first** (nearly free, prove the wire), the **rewired keystone** (Wave 4) lands where the compound arc closes, and the **BUILD/swarm work** lands last where the honesty stakes are highest.

## Ω+1.5 Honesty invariants for ambient operation

An every-action verification tool dies four ways. Each invariant maps to a correction above, and each is enforced in the data path — the moat law: *be more conservative about your own "all clear" than about your alarms.*

1. **Never slow the loop.** Latency-budgeted, cached to `graph_generation`, incremental (re-ingest deltas, not the world), fire-and-forget on post-capture, silent when nothing changed. Sub-100ms is **MEASURED, not asserted** (Correction 2). A formatter, never a test suite.
2. **Never cry wolf.** `am_i_stale` fires **caution by default**; blocking `ask` only on a file this agent read this session. `xray_gate` hard-`deny`s **only** on a ratified manifest; unratified drift is caution. `cochange_nudge` surfaces only `act`-verdict candidates on a calibrated graph; `abstain` is suppressed; cascades collapse into one digest (Correction 3). One confidently-wrong alarm destroys the credibility of the next hundred right ones.
3. **Pre-orient must be honest about what it does NOT know.** The north packet never fakes readiness. `trust_selftest` runs first — a split-brain binding injects `recovery_playbook`, not a confident orientation. `focus` states its `ignored{count,reason}` tail. Cold nodes carry `insufficient_evidence`, never a fake 0.5. A memory with unparseable age reads **"unknown," never "now."**
4. **Post-capture must not fabricate.** The auto-memorize distiller anchors every claim to a resolvable `evidence` code path and **never free-summarizes the turn** (Correction 4). `cross_verify` at write-time flags a born-stale claim born-stale. Staleness is **represented data** — the agent is told "was true until file X changed," never silently served a superseded fact.

The meta-invariant, inherited from OMEGA: **verify-before-assert applies to m1nd itself.** A grounding tool whose ground truth has drifted is worse than no tool, because it launders wrong information with authority. `code > PATHOS > memory` is what keeps the ambient loop from becoming the most dangerous liar in the loop over 20 years.

## Ω+1.6 Manifesto

**m1nd is not a tool you call. It is the nervous system your loop runs on.**

Today an agent boots blind, gropes through a codebase it half-remembers, edits bytes that rotted while it was gone, proves something real — and then forgets it the moment the context window closes. Next session it starts over, cold, re-deriving what it already knew, confidently wrong about a file that moved underneath it. The frontier put a number on this: 80% on a task you hand it clean, **38%** on software that actually evolves. The gap is not intelligence. The gap is *continuity* — the agent cannot honestly build on its own past.

So we stop asking the agent to remember. We wire remembering into the loop itself.

Before it acts, m1nd hands it a north packet: *here is what you can trust, here are the five nodes that matter, here is what a prior agent proved here four days ago — and here is exactly what I do not know.* When it acts, m1nd goes dark; silence is the product. After it acts, m1nd folds the change back into the graph, whispers the one file git says it forgot, sharpens its own trust model on the test that just passed or failed — and when the agent has *proven* something, m1nd writes it down, **anchored to the real line of code**, so next session's first breath inhales exactly what this session exhaled.

Every piece of this already exists. The daemon already wakes on its own. The watcher already sees every file change. `orient` already composes the whole boot packet in one call. `memorize` already turns a proof into anchored memory with no mission required. What was missing was never an engine — it was **the wire.** We are building the wire.

And we build it honest, because a verification tool that cries wolf once is negative value forever. m1nd abstains before it lies. It says "unknown" before it says "now." It refuses to persist a guess as a fact, refuses to block on an architecture rule no human ratified, refuses to certify what it never read. It is more conservative about its own *all-clear* than about its alarms — because that is the only way an agent comes to *refuse to work without it*.

That is the position: not the smartest thing in the loop, but the thing the loop cannot run without. The LSP of agent-to-repo ground truth — nearly free, nearly silent, wired into every chokepoint the agent already passes through, compounding memory so that leaving means amnesia. **Pre-orient, act, post-capture, compound.** The longer it runs, the more expensive it becomes to work without it.

Close the cycle.

> **Load-bearing verbs (all live in `mcp__m1nd__*`):** `trust_selftest`, `orient`, `boot_memory`, `am_i_stale`, `trail_resume`, `focus`, `warmup`, `surgical_context_v2`, `seek`, `xray_gate`, `validate_plan`, `ghost_edges`, `predict`, `calibrate_predict`, `learn`, `mission_verify`, `mission_handoff`, `memorize`, `cross_verify`, `trail_save`, `daemon_tick`, `ingest`, `persist`, `boot_memory`, `alerts_ack`, `recovery_playbook`. **Grounding for the corrections:** `m1nd-mcp/src/mission_handlers.rs:200,309,454,458` (mission_id hard-error — Correction 1) and `m1nd-mcp/src/light_author_handlers.rs` (`handle_light_author`: free-form claims + evidence, no mission_id; resolve-or-flag gate — Corrections 1 & 4).

---

## 1. North Star (agent-first)

m1nd is the one always-reachable, always-live code-intelligence + memory runtime an autonomous coding agent talks to about a repo — and the only one that tells the agent **how much to trust what it just got, when it is blocked, and what it left out.** The next generation makes that honesty operational under the conditions agents actually run in: many concurrent sub-agents converging on **one live graph** instead of dying or drifting on private stale copies; per-node answers that say **"safe to edit / here is the de-risking move"** instead of a vacant `0.5`; and a context layer that decides **when to stop, what to keep, and what not to trust** by testing graph *closure* — never by guessing an answer. Every new signal is answer-free, carries its own evidence, and degrades to an explicit "insufficient_evidence" rather than a confident lie. We push the moat outward; we never weaken it.

---

## 2. The four weak points and the LEAP for each

| # | Weak point (measured) | Today's failure for an agent | The LEAP |
|---|---|---|---|
| **WP1** | **Operational multi-agent concurrency.** Default stdio path takes an exclusive ReadWrite lease per `runtime_root`; the 2nd..Nth concurrent agent gets `AlreadyExists` and `process::exit(1)`. No auto-fallback, no auto-attach. (Lease-leak GC is now largely fixed in tree; residual is O(N) `kill -0` subprocess sweeps.) | Fan-out sub-agents (audit/fix/verify) silently die, or — if a human forced `--read-only` — answer from a private stale snapshot with no drift signal. Both invisible to the agent. | **Multi-reader-by-default auto-join ladder.** On contention, *attach to the live owner* over the existing HTTP bridge instead of exiting; one live graph, owner's `RwLock` as the single writer. Staleness, when unavoidable, becomes an explicit `freshness` signal folded into the honesty envelope. |
| **WP2** | **Compiler-free precise resolution.** `x.method()` receiver-variable calls can't be bound to the right same-name target (no type inference). #1 retrieval-precision gap. | The agent gets a confidently-wrong same-name binding and edits the wrong node. | **Honest ambiguity instead of a silent wrong bind.** Surface the already-tracked `ambiguous` resolutions as `non_claims` ("could not bind `x.method()` to a unique target") rather than silently including a guess — turns the precision gap into a moat behavior. (Type-inference itself is a non-goal for this PRD; see §5.) |
| **WP3** | **Per-node trust/risk is vacant.** Cold-start returns `TRUST_COLD_START_DEFAULT = 0.5` / `TrustTier::Unknown`; git signals m1nd already mines are collapsed into one activation-boost scalar. | The agent's real question — "is it risky to *edit* this node?" — has no answer; worse, recency boost makes the riskiest hotspots rank *higher* with no warning. | **Action-routable per-node `risk` vector** with provenance + an empirically-binned band + an action hint + a first-class `insufficient_evidence` state — reusing git data already on the wire, never emitting a fake probability. |
| **WP4** | **The attention/sufficiency/memory moat is shallow at the agent-felt points.** Sufficiency is a scalar knee test, not a graph-closure test; no bounded working set; no head/tail positioning; cold-start trust gives "BLOCKED" no calibrated bound. | The agent over-answers on causally-incomplete context, re-pays full retrieval every turn (accumulating distractors), and gets load-bearing context buried mid-payload. | **Graph-closure sufficiency + bounded working set + head/tail positioning + calibrated abstention** — four reuse-first extensions of signals the agent already consumes via the per-call contract. |

---

## 3. Per-subsystem design

> Integration anchors are `file:line` against `~/m1nd` at the 2026-06-29 stamp. Lines drift ±a few across commits; **the named symbol is the contract**, the line is a hint.

### Subsystem A — Multi-agent-by-default (attacks WP1; folds staleness into WP4's honesty surface)

**Problem.** The DEFAULT stdio path forces exclusive ownership and dies on contention. Each `m1nd-mcp` stdio process loads its own graph and calls `acquire_with_mode(.., ReadWrite)` via `SessionState::initialize`; only one ReadWrite lease may exist per `runtime_root` (`m1nd-mcp/src/instance_registry.rs:128-179`, `AlreadyExists` at :166). The error propagates to `run_stdio_server` which calls `std::process::exit(1)` (`m1nd-mcp/src/main.rs:171,202,208`). The working multi-agent answer **already exists but is opt-in**: `--attach <url>|auto` (`m1nd-mcp/src/attach_client.rs`) is a zero-graph, zero-lease stdio↔HTTP bridge where N attachers share ONE `--serve` owner's live `Arc<RwLock<Graph>>`. Nothing makes "join the live owner" the default. ReadOnly fallback, even if wired, is a **stale private disk copy** that never sees the owner's in-memory mutations. GC of dead leases is fixed (`gc_dead_leases` at `instance_registry.rs:386`, `is_pid_live` at :511 shells out to `kill -0` per entry).

**Design.** Make **multi-reader the default** and the live owner the single writer; reuse the two mechanisms already in tree (ReadWrite/ReadOnly lease modes + the `--attach` HTTP bridge). Do **not** build a distributed graph.

- **Auto-join boot ladder** in the stdio path. On boot for a `runtime_root`:
  1. Try ReadWrite (today's behavior) — first agent wins, becomes owner-writer.
  2. On `AlreadyExists`, inspect the registry via `discover_serve_owner_base_url` (`instance_registry.rs:631`, already pure/lease-free). If the live owner publishes bind+port, **auto-attach** via the existing `run_attach_client` bridge — now this agent shares the owner's LIVE in-memory graph (correct, fresh, zero second copy).
  3. If the owner is stdio-only (no port → un-attachable, see `entry_base_url` at :603), fall back to ReadOnly **with a loud freshness contract**, not a silent stale copy.
  This promotes the existing `resolve_attach_auto` (`main.rs:68`) from manual opt-in to the automatic contention response. Gate behind `multi_agent` default-on (`M1ND_SOLO=1` opts out).
- **Owner self-promotion.** When a ReadWrite stdio owner boots under multi_agent mode, also bind a **loopback** HTTP serve surface (reuse the `serve` feature machinery in `m1nd-mcp/src/http_server.rs`) and publish bind+port via `set_running_endpoint` (`instance_registry.rs:222`). The first agent silently becomes a shared serve owner; later agents auto-attach. No human `--serve` step.
- **Freshness contract** (the staleness fix, a WP4 honesty extension). Two cheap pieces:
  (i) Publish the owner's mutation generation in its registry entry on each heartbeat (extend `InstanceRegistryEntry` at `instance_registry.rs:20`, populate in `mark_heartbeat`/`set_running_endpoint` at :222-231). A ReadOnly attacher compares loaded vs live generation and surfaces `freshness:{state:"behind", local_gen, owner_gen, behind_by}` in the `_m1nd` envelope at the **same site that already advertises read-only**. An agent reading `behind_by>0` knows to re-attach or distrust.
  (ii) Best-effort self-refresh: a behind ReadOnly attacher reloads from disk when the owner's persisted generation advances. Real-time freshness comes from the HTTP attach in step 2; this is the serve-less fallback.
- **GC scaling.** Replace per-entry `kill -0` subprocess (`instance_registry.rs:511`) with an in-process `sysinfo` process-table read — one read per sweep instead of N `fork+exec`. Identical conservative semantics (only provably-dead PIDs removed). Existing GC tests pin behavior.

**Exact m1nd integration points.**
- `m1nd-mcp/src/main.rs:171-208` `run_stdio_server` — wrap server bring-up in the ladder; on `AlreadyExists` call `discover_serve_owner_base_url` + hand off to `run_attach_client` instead of `process::exit(1)`.
- `m1nd-mcp/src/main.rs:68` `resolve_attach_auto` — reuse verbatim as the discovery step.
- `m1nd-mcp/src/instance_registry.rs:128-179` `acquire_with_mode` — surface a typed "owner-exists" outcome so `main.rs` can branch attach-vs-ReadOnly without re-reading the lease. **Do not add a third lease mode.**
- `m1nd-mcp/src/instance_registry.rs:20` `InstanceRegistryEntry` — add `#[serde(default)] cache_generation: u64`; populate on heartbeat. `#[serde(default)]` keeps legacy lease files readable.
- `m1nd-mcp/src/instance_registry.rs:511` `is_pid_live` — swap body to `sysinfo`; callers (`gc_dead_leases:386`, `list_instances:281`) untouched.
- `_m1nd` envelope (the site that injects `read_only`) — add the `freshness` block. **Verification delta:** the brief cited `server.rs:3300-3305`; the read-only envelope injection in the current tree is in `m1nd-mcp/src/session.rs` (`non_claims` block at `session.rs:640`, agent-runtime-contract path) — re-confirm the exact injection site at implementation time and attach `freshness` adjacent to `read_only`.
- `m1nd-mcp/src/http_server.rs` (`serve` feature) — reuse for owner self-promotion (loopback bind, ephemeral port). No new server.
- `McpConfig` — add `multi_agent: bool` (default true; `M1ND_SOLO=1` → false) mirroring the `read_only` flag plumbing in `main.rs:98-111`.
- `m1nd-mcp/src/cli.rs` — document auto-attach default alongside the existing `--attach` flag.

**Importable permissive refs.**
- `sysinfo` — in-process liveness — https://crates.io/crates/sysinfo — **MIT**.
- `fs4` (advisory file locking; candidate to close the lease `exists()` TOCTOU *only if a real double-acquire is observed*) — https://crates.io/crates/fs4 — **MIT OR Apache-2.0**.
- `parking_lot` (already a workspace dep — `m1nd-mcp/Cargo.toml:21`, `m1nd-core/Cargo.toml:14`; the owner's `RwLock` is the single-writer/many-reader point — **no new dep**) — https://crates.io/crates/parking_lot — **MIT OR Apache-2.0**.
- `reqwest` (already used by `attach_client.rs` under the `serve` feature — `m1nd-mcp/Cargo.toml:35`) — https://crates.io/crates/reqwest — **MIT OR Apache-2.0**.

**Phased plan.**
- **Phase 0 (GC scaling, smallest shippable, zero behavior change):** swap `is_pid_live` to `sysinfo`. Gate: existing GC tests green + a new test asserting one sweep over K planted dead entries spawns zero subprocesses. Ships independently.
- **Phase 1 (freshness signal, read-only correctness):** add `cache_generation` (`#[serde(default)]`), publish on heartbeat, emit `freshness` in the envelope for ReadOnly sessions. Gate: ReadWrite owner mutates → ReadOnly attacher's envelope reports `state=="behind"` with correct `behind_by`; legacy entries deserialize fine. Kills the silent-stale-read failure before auto-attach lands.
- **Phase 2 (auto-attach ladder — the core win):** on `AlreadyExists`, discover serve owner and hand off to `run_attach_client`; ReadOnly only when owner is stdio-only. Behind `multi_agent` default-on. Gate: boot owner A (serve), boot agent B same root, assert B does NOT exit and B's `tools/call` results match A's live graph after A ingests (proves shared LIVE graph); `M1ND_SOLO=1` reproduces today's exclusive-then-exit.
- **Phase 3 (owner self-promotion):** default ReadWrite stdio owner auto-binds loopback serve + publishes bind+port. Gate: two plain `m1nd-mcp` stdio agents back-to-back for one repo → exactly one owner + one attacher (via `list_instances` modes), both answer from one live graph. Multi-agent-by-default proven end to end.
- **Phase 4 (eventual freshness, serve-less):** ReadOnly attacher reloads snapshot when owner's persisted generation advances. Gate: `freshness.state` transitions behind→current without restart. Optional polish.

**Proof standard.** Each phase: `cargo test --workspace --all-targets` green (the real proof entry per `docs/internal/CORTEX_V04_BUILD.md:109`), clippy/fmt clean, CI green on 3 OSes before merge. Phase-specific live-client gates as above. Honesty invariant: a ReadOnly answer that is behind MUST carry the `freshness` block — a behind-but-silent answer is a test failure.

**Risks (honest).** (1) Auto-attach changes default behavior — if the owner crashes mid-session the attacher loses its backend; mitigate with `M1ND_SOLO=1` + a follow-on "respawn own ReadWrite on owner-disconnect" (not Phase 2). (2) Owner self-promotion opens a loopback port from a process that previously bound nothing — MUST bind `127.0.0.1` only (codebase already rewrites `0.0.0.0`→`127.0.0.1` in `instance_registry.rs`) and not widen the read-only deny-list. (3) The lease is still an `exists()` check → a TOCTOU window where two processes both win ReadWrite; the ladder makes a double-owner *less* catastrophic (loser attaches) but doesn't close the race — `fs4` atomic create-exclusive is the proper fix *only if a real double-acquire is observed* (verify first). (4) `sysinfo` PID-recycle race spares a truly-dead entry one cycle — harmless, conservative, matches the current `kill -0` profile. (5) Freshness must compare by `(owner instance_id, generation)`, not generation alone — a fresh owner restarting at gen 0 could falsely read as "ahead".

---

### Subsystem B — Action-routable per-node risk (attacks WP3)

**Problem.** Per-node trust is vacant for any agent without learn-history. `TrustLedger` (`m1nd-core/src/trust.rs:163`) accrues only from `learn("wrong"/"partial")` + cross_verify; with no history `compute_trust` returns `TRUST_COLD_START_DEFAULT = 0.5` (`trust.rs:14`) with `TrustTier::Unknown` (`trust.rs:68`). seek/focus then attach `trust_score: 0.5` to every result; the honesty note at `layer_handlers.rs:8034` literally says *"mean_trust 0.5 is the cold-start neutral prior, not a computed score."* Meanwhile git signals m1nd already mines are thrown away: the walker runs `git log --format='%at' --name-only` once (`m1nd-ingest/src/walker.rs:241` — confirmed `%at` only, **no author**) and collects per-file `commit_count`, `last_modified`, and `commit_groups` (`walker.rs:22,32`); ingest collapses ALL of it into one `change_frequency = commits/(commits+10)` scalar (`m1nd-ingest/src/lib.rs:277-347`) whose only use is to BOOST activation. Entropy, ownership/bus-factor, co-change fan-out are never computed; the author field is discarded.

**Design.** An agent doesn't want a probability; it wants a **routable reason** — "is it safe to edit this, and if not, what de-risks it?" Emit a per-node `risk` vector with provenance + an empirically-binned **band** + an action hint + a graceful `insufficient_evidence` state, extending the honesty moat — never a fake score.

**The vector (bootstrap tier, ZERO new data sources except one git flag):**
- `churn`: relative-churn z-score (Kamei TSE2013: relative beats absolute) — reuse the existing `VelocityScorer::score_all` z-score (`m1nd-core/src/temporal.rs:513`) over `change_frequency`. Already computed; surface it.
- `entropy`: Kamei change-scattering = Shannon entropy of the co-change distribution — compute from `commit_groups` already collected by the walker (the same input `populate_from_commit_groups` at `temporal.rs:210` consumes).
- `co_change_fanout`: support + confidence over the co-change matrix (Gerosa/Zimmermann) — reuse the existing `CoChangeMatrix::predict` (`temporal.rs:191`); formalize support=#co-change-commits, confidence=P(edit B | edit A).
- `ownership / bus_factor`: Bird-Nagappan FSE2011 Minor(<5%)/Major(≥5%)/top-owner-fraction. **The one new git field:** add `%an` to the walker's `git log` format and aggregate authors per file. `bus_factor=1` is the strongest "no one to verify with" escalation.
- `defect_history`: the existing `TrustLedger` score — now ONE component, not the whole answer.

**The output contract (the moat):** `risk: { band: "low|med|high|insufficient_evidence", components: {churn, entropy, fanout, ownership, defect_history}, evidence: [...], action_hint, non_claims: [...] }`.
- Band is **empirically binned** against the repo's own distribution (percentile of a transparent-weight composite), NEVER a raw probability — justified by the calibration negative result (Shahini 2025: JIT models are miscalibrated, post-hoc fixes inconsistent). Standing non_claim: *"reflects historical co-occurrence in THIS repo, not a calibrated probability."*
- **Action-routing (the differentiator):** dominant sub-signal → distinct agent action. High churn → "write a characterization test first"; `bus_factor=1` + high band → "no recent owner to verify with — escalate / non_claim"; high `co_change_fanout` → "pull these N coupled-but-unlinked nodes into context first" (named via `predict()`). The agent acts on the WHY.
- **Graceful degradation (hard requirement — kills the vacant-0.5 lie at the root):** a node with no git history / not in a git repo returns `band="insufficient_evidence"` + explicit non_claim, NEVER 0.5.

Transparent-weight + binned only. Learned models (graph-JIT, SZZ-labeled training) are **deferred to Tier-2** to avoid re-introducing the confident-but-vacant failure.

**Exact m1nd integration points.**
- `m1nd-ingest/src/walker.rs:241` `enrich_with_git` — change `--format='%at'` to include `%an`; aggregate per-file `authors: Vec<(String,u32)>`; add the field to `DiscoveredFile` (`walker.rs:15`). **Only new git data needed for the whole bootstrap tier.**
- `m1nd-ingest/src/lib.rs:277-347` — the per-file git map (`file_git_data`) is built here; extend it to carry entropy + ownership + author set instead of collapsing into the single `change_frequency` scalar.
- `m1nd-core/src/graph.rs` `NodeStorage` (SoA) — add ONE cold-path `risk: Vec<RiskComponents>` array mirroring the existing `change_frequency`/`provenance` pattern (push in `add_node`, default in `new()`/`with_capacity`). Cold-path → no hot-loop cost.
- `m1nd-core/src/trust.rs` — add a `RiskComponents` struct + `compose_band()` that bins the composite by repo percentile → `Band::{Low,Med,High,InsufficientEvidence}`. Keep `TrustLedger` untouched as the `defect_history` component. This file is the reuse-first home (already serialized/persisted).
- `m1nd-core/src/temporal.rs:191,513,210` — reuse `CoChangeMatrix::predict` (fanout), `VelocityScorer::score_all` (churn), `populate_from_commit_groups` input (entropy).
- `HeuristicSignals` (`m1nd-mcp/src/protocol/layers.rs:124`) — add `risk: Option<NodeRisk>` next to `trust_score`/`trust_tier`. Surfaces in seek (`layer_handlers.rs:429-431`) and focus automatically.
- `m1nd-mcp/src/layer_handlers.rs` `handle_trust` (output near :7954-8034) — emit the risk vector and **replace the cold-start 0.5/Unknown leak** (the note at :8034) with `band=insufficient_evidence` when no git history. The empty-state note becomes a real graceful-degradation path.
- seek ranking (`layer_handlers.rs:396-431`) — feed the band into the seek reason so it explains risk. Keep risk **additive/explanatory by default** (do NOT silently down-rank editing targets — an agent often WANTS the risky node); any re-rank is opt-in, mirroring the existing additive `conformance_boost` pattern.

**Importable permissive refs.**
- Kamei et al. TSE 2013 — JIT change-level metrics; relative churn beats absolute — https://posl.ait.kyushu-u.ac.jp/~kamei/publications/Kamei_TSE2013.pdf — *methodology paper; metrics computable from git (m1nd's owned input)*.
- Bird & Nagappan FSE 2011 — code ownership / bus-factor — https://doi.org/10.1145/2025113.2025119 — *methodology; computable from git blame/log*.
- Change coupling (support + confidence) — Gerosa et al.; Zimmermann & Gall — https://www.ime.usp.br/~gerosa/papers/changecoupling.pdf — *methodology; reuses existing CoChangeMatrix*.
- Shahini, Bartel, Pohl 2025 — On the calibration of JIT Defect Prediction (justifies binned band + calibration non_claim) — https://arxiv.org/abs/2504.12051 — *paper; treat any ECE range as approximate until the table is re-read*.
- `lcov` crate — pure-Rust LCOV parser (Tier-2 coverage overlay; the correct importable, NOT grcov which is MPL-2.0) — https://crates.io/crates/lcov — **MIT OR Apache-2.0**.
- `git2` / `git2-rs` — libgit2 bindings for SZZ-grade line blame (Tier-2) — https://crates.io/crates/git2 — **MIT OR Apache-2.0**.
- PyDriller `get_commits_last_modified_lines` — SZZ blame-of-removed-lines ALGORITHM reference to port (Tier-2, reference only) — https://github.com/ishepard/pydriller — **Apache-2.0**.
- SZZUnleashed — permissive SZZ implementation to mirror in Rust (Tier-2 reference) — https://github.com/wogscpar/SZZUnleashed — **MIT**.
- Graph-based JIT defect prediction (PLoS One 2023, centrality/node2vec) — https://doi.org/10.1371/journal.pone.0284077 — *paper; treat the "+152% F1" figure as unverified until the results table is read*.
- OpenSSF Scorecard — external/dependency-node supply-chain overlay (conditional; query, do NOT reimplement checks) — https://github.com/ossf/scorecard — **Apache-2.0**.
- **REJECTED:** code-maat (**GPL-3.0**, confirmed in `project.clj`) — incompatible with m1nd's permissive stack. Use the coupling/ownership *formulas* from the papers, never a line of its code.

**Phased plan.**
- **Phase 0 (battery scaffold, no behavior change):** add risk cases to the proof battery — a known hotspot file must return `band != insufficient_evidence`; a fresh/untracked file must return `insufficient_evidence`, NOT 0.5. Establishes ground truth before code.
- **Phase 1 (graceful degradation FIRST — kills the lie):** add `RiskComponents` + `Band` in `trust.rs`; replace the cold-start 0.5/Unknown in `handle_trust` with `band=insufficient_evidence` + non_claim when `total_learn_events==0 AND no git history`. Gate: `cargo test --workspace` green + the "fresh file → insufficient_evidence" battery row flips PASS. Smallest shippable honesty win, no new git data.
- **Phase 2 (churn + entropy + fanout — reuse-only):** wire `VelocityScorer` z-score, `commit_groups` entropy, `CoChangeMatrix` support/confidence into `RiskComponents`; add the cold-path `risk` array; populate in ingest; emit band from a percentile-binned composite. Gate: "hotspot → high band with churn+fanout evidence" PASS; hot-path activation benchmark unchanged; clippy/fmt clean.
- **Phase 3 (ownership/bus-factor — the one new git field):** add `%an`, aggregate authors, compute Minor/Major/top-owner-fraction + bus_factor; `bus_factor=1` → escalation hint. Gate: synthetic repo (one author on A, many on B) yields differing bus_factor; verify `git log` perf delta is negligible on a large repo.
- **Phase 4 (action-routing + seek/focus surface):** dominant-component → distinct action mapping (naming coupled nodes via `predict()`); add `risk: Option<NodeRisk>` to `HeuristicSignals`; thread into seek/focus reason (additive, opt-in re-rank). Gate: high-fanout node returns a "pull these N nodes" hint listing the actual coupled ids; seek schema test green.
- **Phase 5 (DEFERRED / Tier-2, only after 1-4 prove out):** coverage overlay via `lcov` (untested + high-churn = top edit-risk); then SZZ labeling (port PyDriller logic, `git2` blame) to turn the heuristic into a calibratable signal; graph-JIT centrality last. Each is its own battery-gated cycle; none ship as default until labels + calibration exist.

**Proof standard.** `cargo test --workspace --all-targets` green + clippy/fmt clean + the honest battery (fresh ingest + ground-truth PASS/FAIL) showing the targeted behavior with a CONCRETE example and zero regression, CI green on 3 OSes before merge. Risk-specific: (1) fresh/untracked node → `insufficient_evidence`, NEVER 0.5 (assert via live MCP client on a temp non-git dir); (2) a measurably hot file returns a non-trivial band whose `evidence` cites the actual churn/fanout components (asserted structurally, not by eyeball); (3) bus_factor synthetic test differentiates one-author vs many-author; (4) action_hint for a high-fanout node names the real coupled ids `predict()` returns. **Honesty invariant:** every band carries its component evidence + the standing calibration non_claim; a band with no evidence is a test failure. **No band may be quoted as a probability anywhere** in output or docs. Performance: risk lives in a cold-path SoA array — assert the hot activation loop benchmark is unchanged.

**Risks (honest).** (1) seek additive-vs-cut: an editing agent often WANTS the risky node → risk defaults to EXPLANATORY (annotate + action_hint), not a silent down-rank; any re-rank opt-in. (2) Young repo: few commits → noisy percentile binning → MUST fall back to `insufficient_evidence` below a min-commit threshold (the exact failure being fixed; do not reintroduce). (3) Tier-2 SBFL/graph-ML MUST NOT ship as defaults — they need runtime traces / training labels most nodes lack. (4) `%an` is identity-noisy (mailmap, multiple emails) — bus_factor is a heuristic, carries a non_claim, never a hard gate. (5) `git log` on a huge monorepo: the walker already pays this once; `%an` is free, but entropy/fanout aggregation MUST stay O(commits) and cold-path so the hot loop is untouched (benchmark-gated). **Citation hygiene:** three figures are approximate and must be re-derived from the primary table before m1nd quotes them publicly: "+152% F1" (PLoS One 2023), the ECE range (Shahini 2025), and the Scorecard check count. SZZ variant taxonomy must cite the Rosa et al. 2023 survey, not the "SZZ Unleashed" implementation paper, when describing the *taxonomy*.

---

### Subsystem C — Sufficiency / working-set / positioning / calibrated abstention (attacks WP4; resolves WP2 as a non_claim)

**Problem.** The context moat is shallow at exactly the agent-felt points. (1) **Sufficiency is a scalar knee test, not a graph-closure test** — it decides sufficient/gathering/blocked from top-score and best-dropped-candidate, never inspecting whether the returned subgraph CLOSES the goal (whether load-bearing edges dangle outside the set). This is the Sufficient Context gap (arXiv 2411.06037): a high-scoring but causally-incomplete set reads "sufficient" and the agent over-answers. (2) **No bounded working-set / eviction** — seek packs a ranked prefix to a token budget (`pack_to_budget` at `m1nd-mcp/src/result_shaping.rs:58`) and focus rides seek, but neither maintains a persistent, decayed, distractor-pruned working set across calls; the agent re-pays full retrieval every turn and accumulates distractors (Chroma Context Rot: a single distractor degrades). (3) **No head/tail positioning** where m1nd assembles context: `surgical_context_v2` (`m1nd-mcp/src/surgical_handlers.rs:2699`) emits primary file then connected files sorted by `surgical_v2_select_candidates` (`:2984`) — no Lost-in-the-Middle (TACL 2024) reordering, so the highest-value connected file can land mid-payload. (4) **Cold-start trust gives "blocked" no calibrated bound** (the WP3 vacancy seen from the context side: `proof_state: "blocked"` at `layer_handlers.rs:106-131` with no risk interval).

**Design.** Four reuse-first concept/policy ports into existing Rust (no Python deps, no new crates beyond what's already present), each gated by m1nd's own proof harness.

- **BUILD #1 — GRAPH-CLOSURE SUFFICIENCY VERDICT (keystone, superset of today's signal).** Reimplement the Sufficient Context definition natively as a cheap closure heuristic that runs AFTER ranking, where the sufficiency/`proof_state` verdict is computed. Extend that path to also receive the returned node set + the graph, so it can answer "do the returned nodes form a closed neighbourhood for the goal, or do load-bearing edges dangle outside the set?" Concretely: for the top-K returned nodes, count outgoing call/import/depends_on edges whose TARGET is NOT in the set AND IS relevance-clearing for the goal (reuse the per-node keyword/trigram scores already computed in seek). Emit a `closure: {closed_edges, open_edges, missing: [{from, to_label, relation}]}` dimension and a verdict that can now say **BLOCKED** ("the strongest match exists but N load-bearing edges leave the set — pull these to close it") distinct from saturated/gathering/sufficient. Answer-free (inspects edge presence, never answer content); keep the cheap scalar test as cascade stage 1; reserve any LLM autorater for genuinely borderline cases. **Spec only** from the paper — the reference repo `hljoren/sufficientcontext` has NO LICENSE; never vendor its prompts.
- **BUILD #2 — BOUNDED WORKING SET with H2O-style eviction over m1nd's EXISTING heat.** m1nd already has both halves of the H2O policy: an accumulated+decayed per-node score (`NodeRuntimeData.heat` + `decay_factor`, `m1nd-core/src/runtime_overlay.rs:30`) AND a heavy-hitter access counter (`top_node_access_frequencies` / `top_node_frequencies`, `m1nd-core/src/plasticity.rs:758,203` — "the cheapest attention signal, no traversal"). Add a per-agent `WorkingSet` on `SessionState` that, each focus/seek call, merges (a) the just-returned focus_set, (b) RECENT nodes (the H2O "always keep recent" half — last-N from the agent's coverage trail), and (c) HEAVY-HITTER nodes (`top_node_access_frequencies`), then evicts the rest to a bounded capacity with a MemoryOS-style heat-threshold + capacity knob. focus returns the working-set delta (kept/promoted/evicted) so the agent stops re-paying full retrieval and the set stays distractor-light. Decay reuses existing heat decay; do NOT invent a new attention metric.
- **BUILD #3 — HEAD/TAIL POSITIONING + DISTRACTOR/AMBIGUITY PRUNING in `surgical_context_v2`** (the ONE place m1nd controls assembly). Because `pack_to_budget` already returns a salience-ranked prefix, positioning is a pure presentation reorder with ZERO salience loss: after `surgical_v2_select_candidates` (`:2984`), interleave connected files so the two highest-value land at HEAD and TAIL of the connected block, weakest in the middle (Lost-in-the-Middle TACL 2024; Chroma Context Rot). For raw seek/focus (agent assembles → unenforceable) emit only an advisory `ordering_hint`. **This is also the WP2 resolution:** `resolve.rs` already tracks `ambiguous` resolutions (`m1nd-ingest/src/resolve.rs:33,113,149`) — when a connected target is an ambiguous same-name candidate, do NOT silently include the wrong one; surface it as an explicit `non_claim` ("could not bind `x.method()` to a unique target") reusing the existing non_claims channel (`m1nd-mcp/src/session.rs:640`).
- **BUILD #4 — CALIBRATED TRUST via split-conformal abstention** (fills the vacant 0.5/Unknown from the context side). Add a calibration pass in `trust.rs`: bootstrap a calibration set from m1nd's OWN history (learn events + audit outcomes + git ground truth already available via `m1nd-core/src/git_history.rs:185` `inject_git_history`) and compute a split-conformal threshold (arXiv 2405.01563) so a cold-start/low-evidence node carries a finite-sample risk bound against an operator risk budget — turning heuristic "blocked" into a calibrated abstain. Keep cold-start 0.5 as the prior but attach a conformal interval instead of a bare `Unknown`. **DROP** the unverified "+22% AUROC" attribution from any spec.

**REJECTED (per verified research):** do NOT cite IGPO (arXiv 2510.14967) as an answer-free inference-time EIG scorer — it is a training-time, ground-truth-aware RL reward. A VOI / "marginal sufficiency-gain per token" ranker for `focus()`-v2 is worth building but is a NOVEL m1nd design grounded on BUILD #1's closure verdict, not a borrowed method.

**Exact m1nd integration points.**
- The sufficiency/`proof_state` verdict path — extend to also take the returned node set + `&Graph`; add the closure dimension; keep the scalar knee test as cascade stage 1. **Verification delta:** the brief cited `compute_sufficiency` / `SUFFICIENCY_WEAK_TOP` / `saturated`/`gathering` at `layer_handlers.rs:106-158` and `struct Sufficiency` at `layers.rs:103`. In the current tree those exact symbols are **not present**; the live verdict surface is `proof_state` (`layer_handlers.rs:106-131,554,950,1074,1121`) with `verdict`/`insufficient_evidence` in `mission_handlers.rs:65,213` and a `graph_only["verdict"]=="insufficient_evidence"` test at `server.rs:5170`. **Implement against the live `proof_state`/`verdict` surface**, not the renamed/removed symbols — re-grep at implementation time and pin the cascade entry point.
- `pack_to_budget` (`result_shaping.rs:58`) — UNCHANGED; positioning consumes its ranked-prefix output downstream, so no salience is lost.
- `surgical_v2_select_candidates` (`surgical_handlers.rs:2984`) + the assembly loop (around `:2804`) — insert the head/tail interleave reorder of `scored`/connected files before emit.
- `NodeRuntimeData.heat` + `decay_factor` (`runtime_overlay.rs:30`) — reuse as the H2O accumulated/decayed score; do NOT add a second heat metric.
- `top_node_access_frequencies` / `top_node_frequencies` (`plasticity.rs:758,203`) — reuse as the heavy-hitter retainer.
- `handle_focus` (`layer_handlers.rs`) — add the WorkingSet merge/evict after `seek` returns; emit kept/promoted/evicted delta. `WorkingSet` state lives on `SessionState` (`session.rs`).
- `resolve.rs:33,113,149` `ambiguous` counters — surface ambiguous same-name binds as non_claims via the non_claims channel (`session.rs:640`).
- `trust.rs:14` `TRUST_COLD_START_DEFAULT`, `:243` `compute_trust_with_params`, report loop — add conformal calibration; ground truth from `git_history.rs:185`.
- the per-call agent contract (`session.rs:640` non_claims block) — carry the closure verdict + working-set state + conformal interval into the contract the agent already reads.
- `HeuristicSignals` (`layers.rs:124`) — closure verdict + working-set delta inherit through seek/focus.

**Importable permissive refs.**
- Sufficient Context (Joren et al., Google, ICLR 2025) — arXiv 2411.06037 — https://arxiv.org/abs/2411.06037 — *SPEC for BUILD #1; cite paper, reimplement natively. Reference impl `hljoren/sufficientcontext` has NO LICENSE — do NOT vendor.*
- H2O Heavy-Hitter Oracle (FMInference, NeurIPS 2023) — arXiv 2306.14048 — https://github.com/FMInference/H2O — **MIT** *(eviction POLICY shape; concept-import into Rust, not code)*.
- MemoryOS (BAI-LAB, EMNLP 2025) — arXiv 2506.06326 — https://github.com/BAI-LAB/MemoryOS — **Apache-2.0** *(heat-threshold + capacity-eviction knobs; LoCoMo benchmark target; concept-import, no dep)*.
- Lost in the Middle (Liu et al., TACL 2024) — https://aclanthology.org/2024.tacl-1.9/ — *paper; head/tail positioning mandate*.
- Chroma Context Rot (Chroma Research, 2025) — https://research.trychroma.com/context-rot — *vendor technical report, NOT peer-reviewed — label accurately; distractor-pruning mandate*.
- Conformal Abstention (Abbasi-Yadkori et al., DeepMind, 2024) — arXiv 2405.01563 — https://arxiv.org/abs/2405.01563 — *paper; split-conformal for BUILD #4. DROP its unverified "+22% AUROC" attribution.*
- MemOS (MemTensor, 2025) — arXiv 2505.22101 — https://github.com/MemTensor/MemOS — **Apache-2.0** *(verified via raw LICENSE, NOT MIT as some briefs state; governable-memory-handle architectural reference only)*.
- Letta / MemGPT (letta-ai) — https://github.com/letta-ai/letta — **Apache-2.0** *(agent-driven page-in/page-out pattern; concept reference for exposing working-set ops to the agent)*.

**Phased plan.**
- **Phase 1 (BUILD #1, shippable):** extend the verdict path + signals struct with the graph-closure dimension. Gate: existing "never silently sufficient" / "covers every state" invariants pass UNCHANGED; new tests assert a high-score-but-open-edge set reports BLOCKED and a closed set reports sufficient. Zero-cost-by-absence: closure omitted on empty/edgeless graph → byte-identical output.
- **Phase 2 (BUILD #3 positioning, low-risk):** head/tail interleave in `surgical_context_v2` + advisory `ordering_hint` on seek/focus. Gate: existing `surgical_context_v2` tests pass; new test asserts the two highest-edge-weight connected files occupy index 0 and N-1. Pure reorder → total_lines + selected SET invariant.
- **Phase 3 (BUILD #3 ambiguity non_claims / WP2):** wire `resolve.rs` `ambiguous` into a non_claim. Gate: fixture with two same-name methods → exactly one non_claim emitted, neither wrong target silently included.
- **Phase 4 (BUILD #2, working set):** per-agent `WorkingSet` with H2O recent+heavy-hitter retention + heat-threshold/capacity eviction over existing heat + access-frequency. Gate: bounded set never drops a recent OR heavy-hitter node; eviction respects capacity; kept/evicted delta sums correctly. Empirical target: LoCoMo-style multi-turn recall.
- **Phase 5 (BUILD #4, calibration):** split-conformal in `trust.rs` from learn/audit/git history; attach a conformal interval to cold-start nodes. Gate: deterministic test (fixed history → fixed threshold); held-out empirical abstain rate within the target risk budget. DROP unverified stats from docs.
- **Phase 6 (stretch, NOVEL):** `focus()`-v2 marginal-sufficiency-gain-per-token ranker grounded on Phase-1 closure (NOT cited to IGPO). Gate: A/B that v2 reaches `sufficient` in fewer tokens than v1 on a fixed query battery.

**Proof standard.** Every claim checkable, no invented numbers. (1) Reuse-proof: each build extends a NAMED existing fn/struct (the verdict/`proof_state` path, `NodeRuntimeData.heat`, `top_node_access_frequencies`, `surgical_v2_select_candidates`, `resolve.rs` ambiguous, `trust.rs` compute_trust) — verifiable by file:line. (2) Zero-cost-by-absence: new fields are `Option` + `skip_serializing_if`, asserted by a byte-identical test on empty-graph inputs. (3) Regression-gated: existing answer-free invariants (a truncated set is NEVER silently "sufficient"; `focus_budget_bound_is_honest`; "covers every state") pass unchanged before any new verdict ships. (4) Each phase: `cargo test -p m1nd-core/-mcp/-ingest`; closure verdict + positioning each get a NEW failing-first test (fails without the change, passes with it — TDD/bugboo discipline). (5) Empirical targets cited as what they are: LoCoMo (MemoryOS) for working-set recall; Lost-in-the-Middle (TACL paper) + Context Rot (Chroma vendor report) as the positioning mandate, labeled distinctly; Sufficient Context as the closure spec (paper cited, repo NOT vendored). (6) Conformal abstain validated by held-out empirical risk vs budget, not by the dropped "+22% AUROC".

**Risks (honest).** (1) Graph-closure can MISLABEL when the true answer needs a relation type m1nd doesn't extract (dynamic dispatch, runtime wiring) — closure looks "closed" but isn't. Mitigation: closure is ADDITIVE (only ever downgrades to BLOCKED on a DETECTED open edge; never upgrades a weak set), and dynamic gaps already surface as non_claims via WP2. (2) H2O's submodular guarantees are token-level KV-cache theory; mapping accumulated-attention to node "heat" is an ANALOGY — do NOT claim H2O's guarantees for the working set without re-derivation. Runtime `heat` is only populated when OTel traces are ingested, so for code-only graphs the access-frequency counter (`plasticity`) is the more reliable retainer — settle "is heat a good heavy-hitter proxy?" in the Phase-4 eval before trusting eviction. (3) Conformal needs labels; cold repos have little learn/audit history → early intervals are wide/honest, not tight (acceptable — wide-but-true beats a fake 0.5). (4) Positioning only binds inside `surgical_context_v2`; for seek/focus it is advisory and the agent can ignore it. (5) Cross-session/multi-agent: the per-agent `WorkingSet` MUST be keyed by `agent_id` and survive Subsystem A's concurrency model so concurrent agents get independent sets over the shared graph — dovetails with multi-agent-by-default, must not reintroduce shared mutable contention.

---

### Subsystem D — Agent-Native Memory (closes the OPEN agent-MEMORY research gap; extends WP4's honesty surface to stored claims)

> This subsystem fills the gap the orchestrator note at the top of this PRD flagged: the dedicated agent-MEMORY research dimension. Grounded by a fresh deep-research pass (6 lanes, synth + adversarial critic) against `/Users/kle1nz/m1nd-l00p` (mirror of `~/m1nd`), critic-verified GO against the live source. **The named symbol is the contract; the line is a hint — re-verify against the current tree before coding.**

**Problem.** The memory layer is a single graph-native write path that is the moat — and does NOT yet inherit the honesty machinery m1nd already has elsewhere. `memorize` (`handle_light_author`, `light_author_handlers.rs:87`) renders a `.light.md` via `render_light_markdown` (`:179-231`) and ingests it `adapter=light`, anchoring each `[𝔻 evidence:]` to a real code node — claims live in the same activation space as code. But six gaps, ALL verified in source by the critic, mean a recalled claim cannot self-describe its epistemic standing:

- **G1 — No time/use dimension on a stored claim.** `LightClaim` (`light_author_handlers.rs:32-53`) carries only label/text/kind/confidence/ambiguity/evidence/depends_on; `created_at`/`last_used`/`access_count` are absent from the writer. `render_light_markdown` writes only `Protocol/Node/State` frontmatter — no write timestamp. A memorized claim has no age, no last-recall, no use count. (Critic note: node `last_modified` is populated from **file mtime** at `walker.rs:126-127`, so a re-memorized file resets its node age on every write — you **cannot** derive a claim's authored age from the temporal kernel; a separate `Created` field is required, not a nice-to-have.)
- **G2 — Auto-load is unranked and uncapped.** `reload_agent_memory` (`tools.rs:384-413`) does `read_dir` → filter `*.light.md` → ingest ALL, every boot. No sort, no cap, no recency filter, no eviction. Every claim ever written re-enters context forever.
- **G3 — Staleness is 100% code-driven.** The ONLY staleness signal is evidence_freshness (`cross_verify`, `tools.rs:618-639`, reasons `evidence_changed`/`evidence_possibly_changed`) keyed on the cited code's sha256 changing. A claim with no evidence paths, or whose code is frozen, reads as "fresh" forever regardless of age, disuse, or a newer contradicting claim.
- **G4 — Silent overwrite-by-slug, no history.** `resolve_output_path` (`:250-257`) addresses by `node_label` slug; re-memorizing the same label silently OVERWRITES via `fs::write` (`:100`). No supersession, no belief-evolution log — `superseded_by`/`invalid_since`/`t_valid` have NO hits in the memory layer.
- **G5 — Provenance is dropped in render.** `handle_light_author` RECEIVES `agent_id` (`:58`) but `render_light_markdown` NEVER writes it. A recalled claim cannot be attributed to its author, so cross-agent recall flattens "agent B thinks X (unverified by me)" into bare fact.
- **G6 — Decay machinery exists but points only at code, and a real bug.** `activate_temporal` (`activation.rs:494-533`) computes recency `exp(-k*age)` + a frequency term — but off code-node `last_modified`, and it **HARDCODES `half_life_secs = 168h`** at `:506` instead of reading the per-`NodeType` `DomainConfig.half_lives` table that already exists (`domain.rs:26-33`, `half_life_for` at `:155`). `TrustEntry` (`trust.rs`) already implements recency-weighted decay (`RECENCY_HALF_LIFE_HOURS=720`, `RECENCY_FLOOR=0.3`) and an HONEST cold-start default (`TRUST_COLD_START_DEFAULT=0.5` = "no history yet, so 0.5, not a fake high score"). `boot_memory_handlers.rs` already has `now_ms()` (`:116`) and stamps `updated_at_ms` (`:68`). None of this is wired to memory.

**NET:** the moat (evidence anchoring + trust ledger + cold-start honesty + a real exp-decay kernel + a half-life table) is ALL present; it just doesn't touch stored memories. No new subsystem is warranted — the gaps are missing FIELDS and missing WIRING, not missing mechanisms.

**Design principles.**
- **Memory inherits the honesty moat, not just the storage.** Exactly like the cold-start trust fix (0.5 reads as "I don't know yet", not a fake high score), a recalled memory must self-describe its standing: a stale, aged-out, low-confidence, unverified-by-me, or superseded claim must READ as such at recall time — never flattened into bare fact.
- **Reuse the kernels, build no parallel store.** Every decay/recency/trust primitive needed already exists (`activate_temporal`, `domain.rs` half_lives, `trust.rs` recency-decay + cold-start, `now_ms`). Repoint them at memory; do NOT add a vector DB, a second KV store, or a new file format.
- **Extend the marker grammar, never fork the format.** New facts (`Created`, source agent, validity window, supersedes pointer) ride existing frontmatter lines or `[𝔻 …]` qualifiers — the parser already accepts arbitrary 𝔻 qualifiers, so additive fields cost zero new format and stay backward-compatible with every existing `.light.md`.
- **Invalidate-and-label, never delete.** A contradicted or shipped claim is marked (`State: outdated` + a supersedes pointer / `[SHIPPED/histórico]`) and RETAINED. This is the graph-native form of Max's existing `[SHIPPED/histórico]` doctrine and Graphiti/mem0 soft-invalidation — destroying a claim destroys honesty.
- **Evidence stays primary; age/disuse is orthogonal and additive.** Keep code-sha freshness as signal #1; add age/disuse/contradiction as INDEPENDENT signals surfaced in the SAME stale list, each labeled distinctly. A claim is stale if (code changed) OR (aged past half-life) OR (superseded) — the agent learns WHY.
- **No mandatory LLM in the hot path.** Contradiction/duplicate CANDIDATES are found deterministically (fingerprint cosine ≥0.85 + evidence-anchor overlap, both shipped). The contradiction VERDICT escalates to the host agent — never an LLM-per-ingest tax.
- **Reinforcement is honest, not gameable.** `last_used`/`use_count` bump only on retrieval-AND-use, so frequently-useful claims decay slower — mirroring Max's gradient "verified/used facts persist, speculative ones evaporate". Stability grows on USE, not on mere existence.
- **Consolidation operationalizes existing doctrine, it does not invent policy.** The "sleep-time"/reflect pass mechanizes what Max already does by hand (the `consolidate-memory` skill, PATHOS curation, update-on-change, mark-shipped) — a daemon-driven dedupe/merge/promote where the parent gets higher confidence and children are flagged `[SHIPPED/histórico]`.

**The eight moves** (each: what / reuses / net-new / honesty-angle / impact / tractability).

- **Move 1 — Stamp `Created` + `Source-Agent` in `memorize` (the keystone field move).** *What:* in `render_light_markdown` (`:185-190`) add two frontmatter lines `Created: <unix_ts>` and `Source-Agent: <agent_id>` (agent_id already in scope at `:58`), optionally also per-claim `[𝔻 created_at:]`/`[𝔻 source:]` qualifiers. *Reuses:* `now_ms()` (`boot_memory_handlers.rs:116`), `updated_at_ms` pattern (`:68`), the 𝔻-qualifier parser, agent_id at `:58`. *Net-new:* two frontmatter lines + optional two 𝔻 qualifiers + a small parser branch to read them back. *Honesty angle:* a claim missing `Created` reads as "unknown age" (honest), never fresh; `Source-Agent` makes cross-agent recall attributable. *Impact:* HIGH — the single enabling field for G1, G5, G6; everything downstream depends on it. *Tractability:* TRIVIAL — ~6 lines, no new types, no migration (old files honestly lack the field). *Ref:* in-house `now_ms` pattern; mem0 author-tag (Apache-2.0) validates `Source-Agent`.
- **Move 2 — Age/disuse staleness as an orthogonal signal in the existing stale list.** *What:* extend `cross_verify`/`am_i_stale` so a claim is stale not only when cited code's sha changes but ALSO when `(now - Created)` exceeds the claim's half-life with no recent use; surface in the SAME `stale_evidence` list, tagged `aged_out` vs `evidence_changed`. *Reuses:* the `cross_verify` return surface (`tools.rs:618-639`), `trust.rs` recency-decay constants, `Created` (Move 1). *Net-new:* one age/half-life comparison + a distinct stale-reason tag. *Honesty angle:* two independent honest staleness signals, each labeled with cause, instead of one silently-incomplete signal. *Impact:* HIGH — closes G3, the most brutal gap (frozen-code = fresh-forever). *Tractability:* EASY (depends on Move 1). *Ref:* Ebbinghaus `R=e^(-t/S)` (public domain); GenAgents recency (formula only).
- **Move 3 — Rank + cap the auto-load (forgetting as a filter on an existing loop).** *What:* in `reload_agent_memory` (`tools.rs:384-413`), before ingest-all, sort entries by `Created`/`last_used` descending and apply a budget/cap (top-N by recency, or skip past N half-lives); reuse `half_life_for` from `domain.rs`. *Reuses:* the `reload_agent_memory` loop (`:397-402`), `domain.rs` half_lives, `Created` (Move 1). *Net-new:* a sort comparator + a config-gated budget cap. *Honesty angle:* old/unused memories quietly drop OUT of always-on context (still on disk, recallable by `seek`) instead of crowding it. *Impact:* MEDIUM-HIGH — closes G2. *Tractability:* EASY; cap defaults to unlimited (no behavior change) so it ships safe. *Ref:* MemoryOS heat-based eviction (reimplement formula only); Letta RAM/disk paging (concept).
- **Move 4 — Supersession-on-rewrite: invalidate-and-keep instead of silent overwrite.** *What:* when `memorize` targets an existing slug (today `fs::write` overwrites at `:100`), first copy the prior file to `agent-memory/.history/<slug>.<ts>.light.md`, set the prior claim `State: outdated`, and record a `Supersedes:` pointer on the new one; gate supersession on the new claim being `State:verified` or ≥ confidence so a weaker agent can't clobber a stronger assessment. *Reuses:* the `State:` marker (`render_light_markdown:189`, already accepts arbitrary values), `resolve_output_path` addressing (`:250-257`), fingerprint + seek for candidate detection, the `[SHIPPED/histórico]` doctrine. *Net-new:* the invalidation lifecycle (history copy + State flip + Supersedes pointer) + the gating rule. *Honesty angle:* a superseded belief is retained and labeled; recall shows "X (superseded 2026-06 by Y)"; last-writer-wins is gated by confidence. *Impact:* HIGH — closes G4; the highest-value true gap across all lanes (Graphiti/mem0/MemClaw all converge here). *Tractability:* MEDIUM. *Ref:* Graphiti invalidate-not-delete (Apache-2.0, reimplement in Rust); mem0 soft-invalidate (Apache-2.0).
- **Move 5 — Honest recall labeling in `seek`/`federate` output.** *What:* render each memory hit as "claim (conf 0.7, authored 12d ago, source: agent-B, unverified-by-you / aged / superseded)" instead of bare fact; add age (from `Created`), author (from `Source-Agent`), and the stale/superseded flags (Moves 2/4) to the recall formatter. *Reuses:* the `seek` output formatter (already fuses embedding + graph_rerank), confidence/evidence already returned, `Created`/`Source-Agent` (Move 1), stale flags (Moves 2/4). *Net-new:* recall-side labeling of fields that now exist. *Honesty angle:* THE core principle made concrete — a stale/low-confidence/foreign/superseded memory reads as such at the moment of use. *Impact:* HIGH — where the moat becomes VISIBLE; all field work above is wasted if recall flattens it. *Tractability:* EASY-MEDIUM (depends on Moves 1/2/4). *Ref:* mem0 author-attribution-at-recall (Apache-2.0); Graphiti validity-windows-on-hits (Apache-2.0).
- **Move 6 — Fix `activate_temporal` to read `DomainConfig.half_lives` (decay correctness).** *What:* replace the hardcoded `half_life_secs = 168h` (`activation.rs:506`) with a `half_life_for(node_type)` lookup so memory nodes (and code nodes) decay by their declared half-life; add a memory `NodeType` half-life entry. *Reuses:* `domain.rs` half_lives + `half_life_for`, the `activate_temporal` kernel (`:494-533`). *Net-new:* swap the constant for the existing lookup. *Honesty angle:* decay reflects the declared retention policy instead of a silent 7-day lie; a Module-scoped memory honestly persists longer than a File-scoped one. *Impact:* MEDIUM — fixes a verified correctness bug (the table is dead) AND is the wiring prerequisite for memory-node retention scoring. *Tractability:* see Critic corrections — NOT trivial. *Ref:* in-house fix.
- **Move 7 — Per-claim stability that grows on retrieval-and-use (reinforcement).** *What:* add a per-claim stability `S` (init from confidence/importance) that lengthens the effective half-life each time the claim is retrieved AND used; bump `last_used`/`use_count` on recall-and-act. *Reuses:* the `activate_temporal` kernel (Move 6), `Created`/`last_used` qualifiers (Move 1), confidence as importance init. *Net-new:* a per-claim stability field + a retrieval-and-use bump hook. *Honesty angle:* retention honestly tracks demonstrated usefulness — a never-useful claim fades, a repeatedly-verified one persists, no artificial permanence. *Impact:* MEDIUM — operationalizes Max's "used facts persist, speculative ones evaporate" gradient; makes forgetting selective, not purely chronological. *Tractability:* see Critic corrections — BLOCKED on a missing use-signal. *Ref:* SuperMemo/FSRS spaced-repetition (formula only); GenAgents reinforcement-on-access (concept).
- **Move 8 — Daemon-driven reflect/consolidate pass (mechanize `consolidate-memory`).** *What:* fold into `daemon_tick` a periodic pass that, when the summed confidence of unreflected `.light.md` crosses a budget, clusters near-duplicate claims (fingerprint cos+Jaccard), merges them into a higher-confidence PARENT `.light.md` whose evidence array points at the children, marks children `[SHIPPED/histórico]` (`State:outdated` via Move 4), and resets the counter to prevent churn; optionally run on a stronger model since it's not latency-bound. *Reuses:* `daemon_tick`/`daemon_start`, `auto_ingest`, fingerprint clustering, `memorize` evidence-anchor shape, Move 4 supersession, the `consolidate-memory` skill as the manual reference. *Net-new:* the scheduled trigger + cluster→parent synthesis control flow + counter-reset churn guard. *Honesty angle:* the parent inherits higher confidence ONLY by citing its children as evidence (auditable); children are retained as `[SHIPPED/histórico]`, never deleted. *Impact:* HIGH (compounding). *Tractability:* MEDIUM — substrate exists; gate hard on reuse (see Critic corrections). *Ref:* Letta sleep-time consolidation (Apache-2.0 *claimed, unconfirmed*); GenAgents reflection trees (control-flow); MemoryOS promote-at-threshold + reset (Apache-2.0, reimplement); A-MEM evolution prompts (MIT *claimed, unconfirmed*).

**Ranked roadmap (keystone `Created` first).**
1. **#1 KEYSTONE — Move 1 (`Created` + `Source-Agent`).** Smallest change (~6 lines copying `now_ms()`), unblocks everything; Moves 2/3/5/7 all depend on `Created`. Ship first.
2. **#2 — Move 5 (honest recall labeling), partial.** As soon as `Created`+`Source-Agent` exist, surface "authored 12d ago, source agent-B". Highest-visibility honesty win, lowest risk; even before staleness flags, age+author labeling already makes recall honest.
3. **#3 — Move 2 (age/disuse staleness).** Add the `aged_out` flag to the existing stale list, tagged distinctly; closes the most brutal gap and feeds the #2 recall labels.
4. **#4 — Move 4 (supersession-on-rewrite).** Stop silent overwrite; invalidate-and-keep with a `Supersedes` pointer + `State:outdated`, gated on confidence. Highest-value true gap; the graph-native form of `[SHIPPED/histórico]`. Then extend #2's labels to show "superseded by Y".
5. **#5 — Move 6 (fix `activate_temporal`).** Correctness fix (dead table) + the wiring prerequisite for retention scoring. Cheap, independent, ship anytime after #1.
6. **#6 — Move 3 (rank + cap auto-load).** Recency-weighted, capped reload; default cap unlimited so it ships safe, then tune. Depends on #1 (sort key) and benefits from #5 (per-type half-life).
7. **#7 — Move 7 (per-claim stability + reinforce-on-use).** Selective forgetting; builds on #1/#5/#6. Medium effort — see the use-signal blocker.
8. **#8 — Move 8 (daemon reflect/consolidate).** Highest compounding value, largest surface; do last, on top of #4 and #7.

**Critic corrections (must-fix before building).**
- **Move 6 / `activate_temporal` is NOT trivial.** Its signature is `activate_temporal(graph, seeds, weights)` — there is NO `DomainConfig` in scope (`activation.rs:494-498`). Wiring `half_life_for` requires threading `DomainConfig` through the call signature AND its callers, and because `half_life_for` is keyed on `NodeType` the lookup must run PER-NODE inside the seed loop (`:510-525`), not swap one constant. Re-rate as EASY-MEDIUM with a signature change, and note it is a **hidden prerequisite for Moves 2/3/7**, which lean on per-type half-life.
- **Move 2 age-staleness can't be a one-boolean add.** The `stale_evidence` list it extends (`tools.rs:618-639`) runs INSIDE `finalize_ingest` on `previous_inventory` sha diffs — it only fires on a code re-ingest and has the marker node, not a parsed per-claim `Created`, in scope. Surfacing `aged_out` there needs a NEW parse of the `.light.md` `Created` frontmatter (or better, route it through the `am_i_stale` handler, which can re-read on demand). Not a one-boolean add to the existing branch.
- **Move 7 reinforce-on-use is BLOCKED on a missing use-signal.** It leans on a "retrieval-AND-use" signal the codebase does not have — `seek` returns hits but there is no `memory_used(claim_id)` ack. Without resolving this, the bump is either gameable (bump on surface) or needs a new host-agent protocol surface (net-new the design elsewhere avoids). Treat this as a BLOCKER for Move 7, not a footnote — resolve before building.

**Cross-cutting risks (from the critic).**
- **Concurrency / locking on `agent-memory/` writes (biggest unhandled correctness risk).** `reload_agent_memory` and `memorize` both `fs::write` to the same `agent-memory/` dir, and Move 4's invalidate-and-keep is a read-modify-write (copy-to-`.history` + State flip + rewrite) with NO locking. Two sessions memorizing the same slug can lose the supersession history or clobber — this collides directly with the known multi-session worktree drift. Write serialization MUST be designed in (advisory file lock / single-writer through the Subsystem A owner) before Move 4 ships.
- **Migration / backfill.** Move 3 sorts by `Created`, so every legacy `.light.md` (missing `Created`) sorts as oldest and could be the first evicted the moment the cap ships — silently dropping the entire pre-existing corpus out of always-on context. Explicit rule required: **missing-`Created` is exempt from eviction (or backfilled from file mtime as a floor)** until re-memorized.
- **Evidence-freshness must suppress on already-superseded claims.** A superseded (`State:outdated`, Move 4) claim still has evidence paths; if `cross_verify` keeps scanning it, the stale list fills with intentionally-retired claims (noise). Add a third axis: **suppress evidence-staleness on already-superseded claims** (a `State` filter).
- **Observability / audit emission (proof-grown standard).** No move yet adds a way to SEE that the forgetting (Move 3) and consolidation (Move 8) passes actually happened — what got capped out, what got merged, the confidence deltas. Under Max's verify-before-claim doctrine, these passes need an audit emission or they are unprovable. Required, not optional.

**Open questions.**
- **Bump-on-USE vs bump-on-surface (Move 7):** do we need a lightweight `memory_used(claim_id)` ack from the host, or approximate use = returned-in-top-K? The honest version needs a real use signal; a mandatory ack is friction.
- **Supersession candidate gating threshold (Move 4):** exact overlap rule (same evidence path? same slug? both?) and when to auto-supersede vs only-flag. Auto-supersede on same-slug-rewrite is safe; cross-slug contradiction should probably only flag for host judgment.
- **Bi-temporal validity windows:** Graphiti's `t_valid`/`t_invalid` (when true in the world) vs `Created`/superseded (when recorded) is richer than `created_at` alone. Worth the 4-field marker extension for "what did we believe at time X" queries, or is `Created` + `Supersedes` + `State:outdated` sufficient? Lean minimal until a concrete point-in-time query need appears.
- **Cross-agent memory sharing scope:** `federate`'s namespace-prefix-then-merge could point at per-agent `agent-memory/` dirs. Do we need the four-level scope (agent/team/tenant/restricted), or is per-claim `Source-Agent` + recall-labeling enough for Max's single-operator-many-agents reality? Defer the scope field; ship `Source-Agent` first.
- **Per-agent trust dimension:** the trust ledger (`trust.rs`) is per-MODULE defect risk. Cross-agent recall honesty arguably needs a per-AGENT trust score (is agent-B's memory reliable?) distinct from per-module. This is the one genuinely net-new field the multi-agent lane flagged — and the one place tempted to build a second trust system. **Defer** until multiple agents actually write to one `runtime_root`.
- **Auto-load cap default and policy (Move 3):** right default budget (top-N? N half-lives? byte budget?), and whether aged-out-but-superseding claims are exempt so the current truth always loads. Needs a real corpus to tune; ship cap=unlimited first.
- **Reflection trigger units (Move 8):** summed confidence of unreflected claims, count of near-duplicate clusters, or daemon cadence? And how far may the parent's confidence exceed its children's without overstating (honesty cap mirroring `trust.rs PRIOR_CAP=0.95`)?

**License notes (verify before asserting in shipped text).**
- **Confirmed Apache-2.0 (cite directly):** Graphiti (`getzep/graphiti` LICENSE) and mem0 (`mem0ai/mem0` LICENSE). Cite **"Graphiti"**, NOT bare "Zep" — Zep changed its hosted-product/SDK posture; the Apache-2.0 reusable artifact is the Graphiti engine.
- **Claimed but UNCONFIRMED in this review:** Letta (Apache-2.0), MemoryOS (Apache-2.0), A-MEM (MIT). Low risk because every move reimplements the FORMULA/PATTERN in Rust and copies no source — but the specific license tags MUST be verified before being asserted in any shipped/public doc.
- **Reuse hygiene:** every move reimplements the formula/pattern in Rust; **no source is copied** from any reference repo.

**Proof standard.** Mirrors the rest of this PRD: each move extends a NAMED existing fn/struct (verifiable by file:line) and ships under `cargo test --workspace --all-targets` green + clippy/fmt clean + CI on 3 OSes. Move-specific TDD gates: Move 1 — a memorized `.light.md` contains `Created` + `Source-Agent`, and a legacy file (no `Created`) recalls as "unknown age", never "fresh". Move 2 — a long-untouched claim with frozen code flips to `aged_out` in the stale list, tagged distinctly from `evidence_changed`. Move 4 — re-memorizing a slug retains the prior file in `.history` with `State:outdated` + a `Supersedes` pointer, and a lower-confidence write does NOT clobber a verified one. Move 3 — with cap set, old/unused claims drop from auto-load but remain `seek`-recallable; legacy missing-`Created` files are NOT evicted. Move 8 — a consolidation pass emits an audit record of merged/capped claims with confidence deltas. **Honesty invariant:** any recalled memory that is stale/aged/superseded/foreign-authored MUST read as such at recall — a flattened-to-bare-fact recall is a test failure.

---

## 4. Ranked roadmap (impact × tractability) and the first move for the l00p

Each subsystem self-rated **impact 5 / tractability 4**. The ranking below is by **first-shippable-slice value** — what de-risks the most for the least, and lands as an isolated, test-gated change an autonomous loop can verify deterministically.

| Rank | Move | Subsystem / Phase | Impact | Tract. | Why it ranks here |
|---|---|---|---|---|---|
| **1** | **GC scaling: swap `is_pid_live` to `sysinfo`** | A / Phase 0 | 4 | 5 | Smallest shippable, ZERO behavior change, pure internal swap; existing GC tests pin behavior; de-risks the leak-storm boot path. Highest tractability of any slice — **the ideal first l00p mission.** |
| **2** | **Risk graceful degradation: kill the cold-start 0.5 lie** | B / Phase 1 | 5 | 5 | Highest honesty-per-line: replaces the single most-visible vacant signal with `insufficient_evidence` + non_claim. No new git data, no hot-path touch, one battery row flips. |
| **3** | **Graph-closure sufficiency verdict (BLOCKED on open edges)** | C / Phase 1 | 5 | 4 | The keystone moat extension — turns the stop-signal from a score guess into a causal-closure test. Superset of today's signal; gated by existing invariants. |
| **4** | **Freshness signal for ReadOnly** | A / Phase 1 | 5 | 4 | Kills the silent-stale-read failure before auto-attach lands; pure honesty-envelope add; `#[serde(default)]` keeps legacy leases readable. |
| **5** | **Auto-attach ladder (multi-agent-by-default)** | A / Phase 2 | 5 | 3 | The biggest operational win, but changes default behavior + needs an integration test proving a shared LIVE graph; depends on Phase 1 freshness for safety. |
| **6** | **Head/tail positioning + ambiguity non_claim** | C / Phase 2-3 | 4 | 4 | Pure reorder (zero salience loss) + the WP2 honesty fix; low-risk, independently shippable. |
| **7** | **Risk churn/entropy/fanout (reuse-only) + bus_factor** | B / Phase 2-3 | 5 | 3 | High value but touches ingest + adds a git field; benchmark-gated for hot-path invariance. |
| **8** | **Bounded working set (H2O over existing heat)** | C / Phase 4 | 5 | 3 | Strong agent-RAM win but needs the Phase-4 eval to validate the heat-as-heavy-hitter analogy first. |
| **9** | **Calibrated conformal abstention** | C / Phase 5 + B / Phase 5 | 4 | 3 | Depends on a labeled calibration set; early intervals wide; ship after the cheaper honesty wins. |
| **10** | **Tier-2 learned models (SZZ, graph-JIT, coverage)** | B / Phase 5 | 4 | 2 | Deferred by design — must not ship as defaults until labels + calibration exist. |

### The single first move that should feed the l00p execution engine

**Rank 1 — Subsystem A, Phase 0: replace `is_pid_live`'s `kill -0` subprocess (`m1nd-mcp/src/instance_registry.rs:511`) with an in-process `sysinfo` process-table lookup.**

Why this is the correct seed for l00p:
- **Deterministic, checkable gate:** `cargo test --workspace --all-targets` (the real proof entry, `docs/internal/CORTEX_V04_BUILD.md:109`) — the existing GC tests pin exact semantics, plus one new test asserting a sweep over K planted dead entries spawns **zero** subprocesses. Exit 0 = pass; nothing ambiguous for the semantic verifier to second-guess.
- **Tightly scoped:** one function body, one new dependency (`sysinfo`, MIT, already vetted), conservative semantics unchanged. The mission decomposes cleanly: add dep → swap body → add the zero-subprocess test → run gate.
- **Zero behavior change + zero honesty surface touched:** the loop cannot accidentally weaken the moat; this is the safest possible first real l00p run.
- **Unblocks the rest of Subsystem A:** GC scaling is the foundation under owner self-promotion and the auto-attach ladder.

l00p mission framing: *"In `~/m1nd`, replace `is_pid_live` (instance_registry.rs:511) with a `sysinfo`-based in-process liveness check. Keep semantics identical (only provably-dead PIDs removed; parse errors skipped). Add `sysinfo` (MIT) to `m1nd-mcp/Cargo.toml`. Gate: `cargo test --workspace --all-targets` green + a new test asserting one GC sweep over K planted dead entries spawns zero subprocesses. Commit as Max Kle1nz."*

---

## 5. Non-goals (explicit)

1. **No distributed graph / consensus / new storage engine.** Subsystem A reuses the existing ReadWrite/ReadOnly lease modes + the `--attach` HTTP bridge; concurrency stays single-writer / many-reader over the owner's `RwLock`. No Raft, no CRDT, no second graph copy.
2. **No third lease mode.** ReadWrite and ReadOnly are sufficient; the ladder branches between them, it does not invent a new state.
3. **No full type inference / compiler frontend for WP2.** This PRD resolves the receiver-variable same-name gap by *honestly declining* (a `non_claim`), not by building type inference. Real binding via type inference is a separate, larger effort, explicitly out of scope here.
4. **No probability output for per-node risk.** Bands are empirically binned against the repo's own distribution; no calibrated probability is claimed for the bootstrap tier. (Justified by the Shahini 2025 calibration negative result.)
5. **No learned/ML risk models as defaults.** Graph-JIT, SZZ-labeled training, and SBFL/coverage are Tier-2, each its own battery-gated cycle, none default until labels + calibration exist — precisely to avoid recreating the confident-but-vacant failure being fixed.
6. **No silent re-ranking of editing targets by risk.** Risk is explanatory/additive by default; any down-rank is opt-in, mirroring the existing additive `conformance_boost` pattern. An agent often *wants* the risky node.
7. **No vendoring of non-permissive or unlicensed code.** code-maat (GPL-3.0), grcov (MPL-2.0), and `hljoren/sufficientcontext` (no license) are referenced for *formulas/specs only*, never copied. Importable code is MIT/Apache-2.0/BSD only.
8. **No new attention/heat metric.** Subsystem C reuses `NodeRuntimeData.heat` + `plasticity` access frequencies; it does not introduce a parallel attention signal.
9. **No public claim that inherits an unverified number.** The "+152% F1", ECE range, "+22% AUROC", and Scorecard check count are flagged approximate and must be re-derived from primary sources before any public m1nd output quotes them. No invented papers or licenses, ever.
10. **Not a UI/visualization effort.** This PRD is the runtime/agent-contract layer; `m1nd-ui`/`m1nd-viz` are out of scope.

---

### Verification deltas (honesty appendix)

Carried so no claim is taken on faith:
- **Version:** workspace is `0.9.0-beta.8` (`m1nd-mcp/Cargo.toml:3`), not "v1.1.0".
- **Sufficiency symbols (Subsystem C):** `compute_sufficiency`, `SUFFICIENCY_WEAK_TOP`, `struct Sufficiency`, and the verdict words `saturated`/`gathering` from the brief are **not present** in the current tree. The live verdict surface is `proof_state` (`layer_handlers.rs:106-131`, …) and `verdict`/`insufficient_evidence` (`mission_handlers.rs:65,213`; `server.rs:5170` test). Implement against the live surface; re-grep at implementation time.
- **Read-only envelope site (Subsystem A):** the brief cited `server.rs:3300-3305`; the read-only/non_claims contract injection in the current tree is in `session.rs:640`. Attach `freshness` adjacent to the actual `read_only` advertisement at the real site, confirmed at implementation time.
- **Line drift:** all other anchors verified present with ±a few lines of drift from the briefs (`main.rs:171/202/208`, `instance_registry.rs:128-179/222/511/631`, `walker.rs:15/22/32/241` confirming `%at`-only, `lib.rs:277-347`, `temporal.rs:191/210/513`, `trust.rs:14/68/163/243`, `resolve.rs:33/113/149`, `runtime_overlay.rs:30`, `plasticity.rs:203/758`, `result_shaping.rs:58`, `surgical_handlers.rs:2699/2804/2984`, `git_history.rs:185`, `layers.rs:124`).
