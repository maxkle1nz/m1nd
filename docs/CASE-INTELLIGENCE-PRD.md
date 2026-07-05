# Case Intelligence — PRD

**The mailbox learns to think · fingerprints, cases, absence sentinels, claim-vs-measure, and the abandonment signal — five organs, one law: a case is an overlay, never an axis**

> **Status:** OFFICIAL — maintainer-directed design, 2026-07-05. **UNBUILT** — code-confirmed: `grep -rE 'case_id|CaseId|fingerprint|absence_sentinel|abandonment' m1nd-mcp/src m1nd-core/src` returns zero engine hits at HEAD. This is the §C10 **R11** rung of the organism ladder — the last unclimbed rung, "the homeless organ" (JOINT-K): it spans mailbox, calibration, delegation, and soul, so no earlier sub-PRD could own it. Per the ladder's own doctrine it is **PRD-first, then slices**; shipping this document closes the rung, and §9 names the implementation slices it opens.
> **Provenance:** design seat, single-seat. Every `file:line` anchor was re-verified in the working tree at `origin/main` @ `5ebec83` (the commit that landed R13, closing the ladder's code rungs). **The symbol is the contract, the line is a hint — re-anchor at implementation start** (a mailbox-resolver fix and a migration CLI are in flight in the same area).
> **Ground includes a LIVE PROBE (2026-07-05):** the first real `--inbox-sweep` ever run against the maintainer's spool (66 letters), plus a census of that corpus (§2.3). The probe both proved the substrate's invariants (spool untouched · idempotent re-run `appended: 0` · consent-deferred `.gitignore`) and produced this PRD's RED state: **recurrence is real, and the system cannot see it** (§2.4). One friction letter about the sweep itself was filed during the probe — the corpus grew by one *while being probed*, which is the mailbox working as designed.
> **Sisters, same organism:** the mailbox substrate this PRD overlays — `docs/MEDULLA-PRD.md` (§9 letters, boxes, misdelivery). The receipt shape it borrows — `docs/SOUL-PRD.md` (§6.1 the freshness receipt). The claim-vs-measure precedent it generalizes — `docs/NEXTGEN-AGENT-PRD.md` §O.12 (`outcomes.jsonl`). The constitution it obeys — `docs/ORGANISM-PRD.md` §C2.2 (the one law), §C6 (verb budget), Grammar 3 (letter fates).

---

## 1. Thesis — the founding insight

> **The problem, stated:** a letter is a witness; a case is a trial. Today the mailbox remembers every signal and understands none of them. Three letters about the same defect are three strangers standing in the same room — the grouping happens in an orchestrator's head, or nowhere.

The corpus proves this is not hypothetical. During the construction era, one defect (the pre-orient verb returning an empty memory beat over a non-empty store — the false-absence bug, later fixed as ladder R0) generated **three independent honesty-class letters from three different sessions**. The third witness's author, having no mechanical way to declare the repetition, wrote *"REINCIDENCE of the L52 class"* **in free prose inside the letter body**. The recurrence was real, was noticed, and was recorded — in the one place no machine can read it. That sentence is this PRD's founding artifact: an agent hand-simulating the organ this document designs.

Decompressed into the five organs, each mechanizing a judgment the maintainer or an orchestrator currently performs by hand:

1. **FINGERPRINT** — a stable `(tool_family + symptom_kind)` identity for a recurring problem, so "this again" becomes a computable fact instead of a prose remark. *Confusion is telemetry* made mechanical.
2. **CASES** — N letters → 1 root cause → 1 fix → linked receipts. The grouping the orchestrator does in their head at triage time, made a derived, recomputable overlay.
3. **ABSENCE SENTINELS** — a fixed case earns a standing battery assertion that the symptom **stays absent after restart**. The post-deploy *permanence diff* made mechanical: silence stops being assumed and starts being asserted.
4. **CLAIM-VS-MEASURE** — every honesty-class letter is a labeled pair (what m1nd claimed, what reality showed). Today those pairs rot as prose; audited, they are the calibration corpus the trust machinery is starving for.
5. **THE ABANDONMENT SIGNAL** — an agent that hit an error and silently stopped using m1nd is the most damning honesty datum the system can collect, and today it is invisible. Mechanized (where a session boundary is actually observable — §4.5 is honest about where that is), it becomes the corpus's one **auto-generated** calibration ground truth (the JOINT-D seam made real).

And the one law that disciplines all five (§3): **case intelligence is a grouping overlay, never a third axis.** No new stored state on letters, no fifth fate, no new grammar. Delete every case artifact and not one letter loses a byte.

---

## 2. Today, probed — the current-state truth (2026-07-05)

### 2.1 The substrate that exists (and this design composes — reuse-first)

| Mechanism | Where (verified @ `5ebec83`) | What it gives case intelligence |
|---|---|---|
| **Letter model** | `m1nd-mcp/src/mailbox.rs:62` (`Letter`), `:75` (doc classes `friction\|bug\|triage\|honesty\|win`) | The witness record: `ts`, `agent`, `repo`, `tool`, `class`, `what`, `expected`, stable content-id (sha256[0..12]), `answers[]` reply graph. Free-form `class` field — the fingerprint's `kind` follows the same open-field-closed-vocabulary pattern. |
| **Derived fates** | `mailbox.rs` `derive_fate` — `wet_ink \| in_flight \| fired_clay \| external`, receipt `disposition: fixed \| declined \| moot` | The axis cases overlay. Fates are derived from `answers[]`, never stored — the exact discipline the case overlay inherits. |
| **A closed kind vocabulary, already shipped** | `mailbox.rs:108-125` — `memory_misdelivery` kinds `leak \| false_absence \| wrong_store_write \| misattribution \| vanished` | The proof that a typed symptom taxonomy works in this codebase, and the seed of the fingerprint vocabulary (§4.1): `false_absence` is already both a misdelivery kind and the model case's root class. |
| **Counts-only rendering** | `mailbox.rs:614-624` (`SweepResult.misdelivery`), `:887-895` (`doctor` confusion block) | The honesty style: `doctor` renders confusion as **counts per week, never an uncalibrated score**. Every case-intelligence surface inherits this (CASE-INV-6). |
| **Boxes + sweep** | `mailbox.rs:160-173` (`BoxTarget`), `main.rs:227+` (`run_inbox_sweep`) | The distribution substrate (per-project boxes, medulla box, pending). Fingerprinting is a sweep-time computation — no new process, no new door. |
| **`learn`** | `tools.rs:2736` (`handle_learn`) — `correct \| wrong \| partial`, Hebbian on the graph | The measure half of claim-vs-measure at edge granularity; case receipts cite `learn` events as evidence. |
| **Trust ledger** | `m1nd-core/src/trust.rs` — `TrustEntry` defect/false-alarm/partial counts + timestamps | The per-node memory of being wrong. Cases aggregate the same events at *problem* granularity — same data, one level up. |
| **`outcomes.jsonl`** | `delegation_handlers.rs:~1419-1439` (row schema), `:1617` (`append_outcome_row`) | The shipped claim-vs-measure precedent: delegate PREDICTS a touch-set, debrief MEASURES the real one, one JSONL row per grading. §4.4 generalizes exactly this pattern. |
| **Calibration** | `m1nd-core/src/calibration.rs` — split-conformal, `CalibrationTable` keyed by signal | The consumer. Its own header admits co-change is "the first and currently only calibrated signal" — case intelligence exists to feed it labeled rows from the field. |
| **Soul receipt** | `m1nd-mcp/src/soul_handlers.rs` — one-line count-honest receipt | The receipt shape (`checked <date> @<sha> — N fresh · M stale`) that case receipts and the sentinel report reuse verbatim in spirit: dated, sha-anchored, counts only. |
| **Per-brain session counters** | R14 — `attached_sessions`, `query_count`, `calibration_armed` partitioned by `bound_project_root` | The only session substrate that exists today. §4.5's honest dependency analysis starts from what these counters can and cannot witness. |

### 2.2 What does NOT exist (grep-proven)

No fingerprint computation, no case derivation, no sentinel runner, no generalized claim audit, no abandonment detection. There is also **no engine-minted letter**: every letter in the corpus was appended by an agent. The write door for the engine speaking into its own mailbox (§4.1's escalation letter, §4.5's abandonment letter) is new — small, but new, and named in the budget (§7).

### 2.3 The corpus census — the live probe's numbers

The maintainer's spool at probe time: **66 letters**. By class: 25 friction · 15 triage · 10 honesty · 8 bug · 8 win. By tool, the leader by a wide margin is the pre-orient verb: **16 letters** name it (11 as `north`, 5 as `north (memory beat)` — note the free-text suffix variance, which is exactly why §4.1 normalizes `tool_family` before matching). The first real sweep distributed 17 letters into per-project boxes and the medulla box, proved idempotence (`appended: 0` on re-run), left the spool byte-identical — and left the **majority of the corpus pending under bare repo names**, a resolver gap caught by the probe itself, filed as a friction letter, with a fix in flight. The probe audits the prober: working as designed.

### 2.4 The RED state — recurrence is real and invisible

A naive experiment against the census: fingerprint every letter as `(tool, first-6-words-of-symptom)`. Result: **zero recurrences detected** across 66 letters. Yet the corpus demonstrably contains at least one three-witness recurrence (the false-absence trilogy — three honesty letters, three sessions, one root cause, one eventual fix at ladder R0) plus a hand-written *"REINCIDENCE"* annotation proving an agent SAW the repetition the machine could not.

The lesson is the design: **free text does not fingerprint.** Recurrence in this corpus lives at the granularity of `(tool_family + symptom_kind)` where `kind` is a closed vocabulary — the same lesson `memory_misdelivery` already encodes for its five kinds. Raw-text matching under-groups (three phrasings of one bug look like three bugs); anything fuzzier over-groups (a junk drawer). A closed vocabulary with an honest "no kind → no fingerprint" refusal is the only shape that neither lies in either direction. That refusal is CASE-INV-2.

### 2.4.1 The seams, enumerated

1. **Recurrence invisible** — the trilogy took an orchestrator's memory to connect; the third witness hand-wrote the link.
2. **Fixes prove presence-of-green, never absence-of-symptom.** R0 landed with a RED→green battery case — but nothing PERIODICALLY re-asserts "the false-absence symptom is still absent on the live owner after every restart." The permanence diff is still a human habit.
3. **Claim-vs-measure exists for exactly one verb pair** (delegate/debrief). The 10 honesty letters — each one a labeled (claim, reality) pair about `north`, envelopes, freshness — feed nothing.
4. **Abandonment is silent.** The counters (R14) can say a session queried 3 times and stopped; nothing asks *why*, and no letter is minted.

---

## 3. The one law — a case is an overlay (constitutional, adopted verbatim)

From `docs/ORGANISM-PRD.md` §C2.2:

> **Case intelligence is a grouping overlay, never a third axis:** a `case` is a set of letter ids plus one receipt (`case_id`/`fingerprint` fields), layered over fates (→ the Case-Intelligence PRD, §C10 R11).

Operationalized as four hard rules this PRD may not weaken:

1. **Member letters are never touched.** A letter gains no case field, no flag, no rewrite when it joins a case. Membership is derived: `case(fp) = { letters whose computed fingerprint == fp } ∪ { the receipt that names fp }`.
2. **The only stored case artifacts are two fields ON THE RECEIPT** — `fingerprint` (the identity) and optionally `case_id` (an alias for it) — plus the receipt's existing `answers[]` links. A receipt is already a letter; no new record type exists.
3. **Fates gain no fifth word.** A case has no fate of its own; it *renders* the fates of its members (a case whose receipt is `fired_clay·fixed` and whose sentinel is green is displayed "closed·guarded" — display vocabulary, never storage).
4. **Recomputability is the test.** Delete every fingerprint cache and every derived view: re-running the sweep over the untouched spool+boxes reconstructs every case bit-identically. If any information would be lost, that information is stored in the wrong place.

---

## 4. The five organs

### 4.1 FINGERPRINT — the stable identity of a recurring problem

**Identity:** `fingerprint = (tool_family, symptom_kind)`, rendered `fp:<tool_family>/<kind>` (e.g. `fp:north/false_absence`).

- **`tool_family`** — the letter's `tool` field, normalized: lowercase, strip any parenthesized suffix (`north (memory beat)` → `north`), trim. The census (§2.3) shows suffix variance is the dominant noise; nothing smarter is warranted or permitted (no fuzzy matching across families — CASE-INV-2).
- **`symptom_kind`** — a **closed vocabulary**, seeded from the corpus and from the shipped `memory_misdelivery` precedent:

| kind | one-line definition (corpus-grounded) |
|---|---|
| `false_absence` | said "nothing here" over a store that provably held content (the model case) |
| `stale_claim` | served a claim whose evidence had moved underneath it |
| `overclaimed_verdict` | said `act`/fresh/closed where reality was weaker (honesty-class core) |
| `wrong_route` | answered from / wrote to the wrong brain, box, or store |
| `scope_escape` | retrieval or action crossed a declared boundary |
| `silent_drop` | accepted input and lost it without an error or a receipt |
| `crash` | process/verb died (panics, codesigning rejections, transport deaths) |
| `ux_confusion` | a surface made a human or agent misread state (labels, counts, phrasing) |

- **Who assigns `kind`:** the letter author, via a new OPTIONAL `kind` field on the letter (same open-field pattern as `class`; the write transport is unchanged). The sweep may **conservatively backfill** kinds for legacy letters only where a deterministic rule fires (e.g. class `honesty` + tool `north` + "memory" and an emptiness token in `what` → `false_absence`); anything short of a rule match gets **no fingerprint**. There is deliberately no `other` bucket: a junk drawer is where taxonomies go to lie (CASE-INV-2).
- **Where it computes:** inside the existing sweep (`run_inbox_sweep`), read-only over spool+boxes; recurrence counts land in `SweepResult` beside `misdelivery` and render in `doctor` — counts only.

**Auto-escalation on the 2nd recurrence.** When a fingerprint's witness count reaches 2, the sweep mints **one** escalation letter into the affected project's box: class `triage`, `kind` mirrored from the fingerprint, `what` naming the fingerprint and its witness letter ids, authored as the engine (§4.6). Exactly once per fingerprint (dedup by fingerprint id in the escalation letter's content — the content-id dedup the boxes already enforce makes double-minting structurally impossible; CASE-INV-3). At 2, not 3: the corpus shows the second witness is where a human first says "again" — the trilogy's REINCIDENCE annotation appeared on witness #2's successor because nothing had flagged witness #2.

### 4.2 CASES — N letters → 1 root cause → 1 fix → linked receipts

A case is **born by a receipt, never by a letter count.** When someone (agent or maintainer) answers the witnesses with a receipt letter — `answers: [ids…]`, `disposition: fixed|declined|moot` — and stamps it `fingerprint: fp:<…>`, the case exists. From that moment:

- **Membership is derived** (law §3): every letter whose computed fingerprint matches, plus every letter the receipt explicitly `answers`. The explicit list handles pre-taxonomy letters that a backfill rule can't reach: the receipt's author is the human-judgment escape hatch, and their judgment is *recorded in the receipt*, not smeared onto members.
- **The case receipt speaks the soul receipt's dialect** (dated, sha-anchored, counts-honest): `case fp:north/false_absence — 3 witnesses · fixed @<sha> · sentinel: <probe>`. One line, renderable everywhere a letter renders today (doctor, the mailbox UI's fate-line), no new UI surface required in v0.
- **A case never closes letters.** Members keep their own fates; the mailbox UI already renders reply-graph state, and a case is just a lens over it. Where a display groups by case, the group label uses display vocabulary (`open` / `answered` / `closed·guarded`) computed from member fates + sentinel state, stored nowhere.

### 4.3 ABSENCE SENTINELS — the permanence diff made mechanical

**A fixed case earns a standing assertion that its symptom stays gone.**

- **Shape:** the fixing receipt MAY carry `sentinel: { probe: "<command or battery case id>", expect: "absent" }`. The probe is the same command family the repo's battery already speaks (a battery case id, or a one-line runnable probe against the served owner).
- **Runner:** `--sentinel-sweep` (same offline CLI pattern as `--inbox-sweep`): collect every sentinel from every box's receipts, run each probe, report `sentinels: N guarded · M green · K RED` — and a RED sentinel **mints a letter** into the case's box (class `bug`, kind mirrored, `what`: "sentinel regression: <fp> resurfaced"), which — because the fingerprint already has a receipt — is *witness N+1 of an existing case*, not a new orphan. Regression re-opens the story exactly where its history lives.
- **When it runs:** by hand, at battery time, and naturally after restarts (the post-restart diff — the moment silence is least trustworthy). It is a read-only prober; it never mutates stores.
- **Bound:** a sentinel exists only on a `disposition: fixed` receipt (CASE-INV-4). Declined/moot cases guard nothing — there is nothing to keep absent.

The model case, concretely: the false-absence receipt would carry `sentinel: { probe: "north over the seeded non-empty fixture asserts memory_exists ≥ 1 or a non-empty beat", expect: "absent(false_absence)" }` — the exact battery case R0 landed with, promoted from "ran once at merge" to "stands guard forever".

### 4.4 CLAIM-VS-MEASURE — the audit, generalized from the one place it already works

The delegation layer already does this end-to-end: predict (packet) → measure (debrief) → one `outcomes.jsonl` row → derived counts, calibration-gated tuning at N ≥ 30. R11 generalizes the PATTERN, not the plumbing:

- **The labeled pair already exists in the wild:** every honesty-class letter IS `(claim m1nd made, reality observed)` — the census holds 10. What's missing is structure: the `kind` field (§4.1) supplies the claim taxonomy; the fingerprint supplies grouping.
- **S3 adds one export:** the sweep renders, per `(tool_family, kind)`, the honest counts — `claimed-right / claimed-wrong / unresolved` — from honesty letters and their receipts, into a `claims` block beside `misdelivery` in `SweepResult` + `doctor`. Counts only, forever (CASE-INV-6): the delegation layer's own rule ("no quality number prints anywhere" until calibrated) binds here verbatim.
- **The calibration hand-off is a file, not a promise:** rows shaped for `CalibrationTable` (signal = the tool_family's verdict channel, label = the letter's measured truth) appended to a `field-labels.jsonl` beside `outcomes.jsonl` — the same one-row-per-event JSONL discipline. Whether any signal ever consumes them is that signal's calibration decision at its own N-threshold; this PRD only guarantees the rows stop rotting as prose.

### 4.5 THE ABANDONMENT SIGNAL — the crown, held honestly

**Definition:** an agent that received an error / wrong answer / hard abstain from m1nd and then **stopped using m1nd within the same working session** — not a complaint, a *silent walk-away*. It is the purest honesty datum: unprompted, behavioral, and negative (nobody files a letter about the tool they just stopped trusting — that is exactly why it must be auto-generated, JOINT-D).

**What m1nd can actually see (the honest dependency analysis):**

| Substrate | Can it witness abandonment? |
|---|---|
| Per-brain counters (R14: `attached_sessions`, `query_count`) | Volume only. Can show "3 calls then silence", cannot distinguish *finished the task happily* from *walked away angry*. |
| The MCP transport | Sees its own calls only. A session that keeps working via other tools after dropping m1nd is invisible from inside m1nd. |
| **The ambient hook wave (ladder R12 — Stop/session-end hooks)** | **The only honest witness.** A session-end hook can observe: m1nd was called early, an error/abstain/`learn(wrong)` occurred, zero m1nd calls followed, and the session demonstrably continued (the Stop event itself proves the session outlived the walk-away). |

**Decision:** abandonment detection is **S4 and depends on the ambient wave**. When the Stop-hook path exists, the rule is mechanical and conservative: `(error|wrong|hard-abstain observed) ∧ (zero m1nd calls afterward) ∧ (session continued to a Stop)` → mint ONE letter, class `honesty`, `kind: overclaimed_verdict` or the error's own kind, engine-authored, citing the last verb + the failing call's shape (never the conversation — m1nd sees its own traffic only). Exactly-once per session (dedup by session id in content). Until the hook exists, the organism does not pretend: a `learn(wrong)`-with-no-follow-up heuristic was considered and REJECTED as a proxy — it cannot see whether the session continued, so it would mint accusations from silence. §10 carries this as the named gap.

### 4.6 The engine-minted letter — one new write path, tightly caged

§4.1 (escalation), §4.3 (sentinel regression), and §4.5 (abandonment) all need the engine to speak into its own mailbox. One shared mechanism, three callers:

- `agent` field: `"m1nd:engine"` — a reserved authorship namespace that no real agent uses; provenance is unmistakable and auditable (the same etiquette-by-provenance promote already practices; CASE-INV-5).
- Same transport as everyone else: append to the target box (never the spool — engine letters are born *distributed*, they have no distribution question), content-id dedup applies, MED-INV-9/10 untouched.
- Engine letters never impersonate, never editorialize (template bodies with ids and counts, no adjectives), and are themselves fingerprintable — the system's own speech is subject to the same audit as everyone else's.

---

## 5. The verb surface — zero new verbs (v0 decision, Budget Law)

The soul earned two verbs because it introduced a first-class TYPE. A case is an overlay on an existing type, and the constitution's verb budget (§C6) prices every new verb against the packet. Decision: **v0 ships no new MCP verb.**

- **Compute** lives in the sweep (`--inbox-sweep` gains fingerprint/recurrence; `--sentinel-sweep` is a CLI flag, not a verb).
- **Read** lives where mailbox reading already lives: `doctor` (counts + top recurrent fingerprints + case one-liners), the mailbox HTTP route / UI fate-line (case lens, display-only), and the letters themselves.
- **Write** is the receipt author stamping two fields — `memorize`/append paths unchanged.
- If field pressure ever demands a `cases` read verb, that is an S-slice behind measured demand (letters asking for it), not a v0 guess.

---

## 6. CASE-INV — the honesty invariants (additive; TT-INV/MED-INV bind here unweakened)

1. **CASE-INV-1 (overlay-only):** cases are derived; deleting every case artifact loses zero letter bytes; recompute reconstructs bit-identically (§3.4).
2. **CASE-INV-2 (no junk drawer):** no `kind` → no fingerprint → no case membership by computation. There is no `other`. Backfill only by deterministic rule; the receipt's explicit `answers[]` is the sole judgment channel, and it is signed.
3. **CASE-INV-3 (escalation exactly-once):** one escalation letter per fingerprint, ever — enforced structurally by content-id dedup, not by memory.
4. **CASE-INV-4 (sentinels guard fixes):** a sentinel exists only on a `disposition: fixed` receipt; a RED sentinel mints a witness into the existing case, never a new orphan.
5. **CASE-INV-5 (engine provenance):** engine-minted letters author as `m1nd:engine`, template-bodied, dedup-caged, never impersonating an agent — and never minted from unobservable silence (§4.5's rejection of the proxy).
6. **CASE-INV-6 (counts only):** every case-intelligence surface renders counts and receipts; no uncalibrated score, rate-without-denominator, or health adjective, anywhere, ever.
7. **CASE-INV-7 (substrate inviolate):** the spool is never truncated (MED-INV-9); pending never re-routes to the medulla (MED-INV-10); boxes append-only with dedup. Case intelligence reads; only §4.6's caged writer writes.

---

## 7. Budget conformance — everything new, priced

| Addition | Kind | Cost |
|---|---|---|
| `kind` field on letters | optional field, open write shape | zero to existing writers |
| `fingerprint`/`case_id` + `sentinel` on receipts | optional fields on an existing letter shape | zero to non-participants |
| fingerprint/recurrence/claims in `SweepResult` + `doctor` | extension of existing render | counts only, no packet impact |
| `--sentinel-sweep` | CLI flag (offline, read-only prober + caged writer) | no verb, no packet |
| engine-minted letters | one new write path, §4.6 caged | bounded: template + dedup |
| `field-labels.jsonl` | one JSONL beside `outcomes.jsonl` | append-only rows |
| new MCP verbs | — | **none** |
| north-packet change | — | **none in v0** (a case headline in the packet is an S-slice behind the same sufficiency gating as everything else in §C1.3) |

---

## 8. The closed loop, traced end-to-end (the payoff)

The model case, replayed as if R11 had existed during the construction era:

1. Session A files a witness: `north` returned an empty memory beat over a bound, non-empty store → letter, class `honesty`, `kind: false_absence`. Fingerprint `fp:north/false_absence`, count 1. Nothing escalates — one witness is an anecdote.
2. Session B (different day, different phrasing) files witness 2. The sweep computes count 2 → **mints the escalation letter** into the project box: "fp:north/false_absence — 2 witnesses: <id>, <id>". No human memory required; the *"REINCIDENCE"* prose annotation never needs to exist.
3. Triage reads one escalation letter instead of re-deriving the link; the confirmed defect becomes a RED battery case BEFORE the fix (the repo's standing triage law, now with the witnesses pre-assembled).
4. The fix lands. The receipt answers all witnesses: `disposition: fixed`, `fingerprint: fp:north/false_absence`, `sentinel: {probe: <the RED case, now guarding>, expect: absent}`. **The case now exists** — derived, one line: `case fp:north/false_absence — 3 witnesses · fixed @<sha> · guarded`.
5. Every future `--sentinel-sweep` (notably post-restart) re-asserts absence. If the symptom resurfaces in any future rebuild, the RED sentinel mints witness 4 *into the same case* — history intact, no orphan, no re-diagnosis from zero.
6. The three honesty letters export as labeled rows to `field-labels.jsonl`; when the pre-orient verdict channel reaches its calibration N, the field corpus is sitting there, structured, waiting.
7. When the ambient wave lands, the walk-away that today costs a user silently becomes witness 0 of the next case — filed by the engine before any human notices the frustration.

One signal, one identity, one trial, one guard, one labeled row — and the next cold session inherits all of it by reading one line.

---

## 9. Slice plan — proof-grown, PRD-first honored

Each slice lands with a battery case RED on the REAL corpus (or a faithful synthetic replay of it) before the code.

- **S0 — fingerprint in the sweep (read-only).** `tool_family` normalization + the kind vocabulary + optional `kind` on letters + conservative backfill rules + recurrence counts in `SweepResult`/`doctor`. **Seed battery (RED today):** replaying the census corpus groups the false-absence trilogy under one fingerprint (today: zero groupings, §2.4); the two tool-suffix variants of the pre-orient verb normalize to one family; a kindless letter earns NO fingerprint.
- **S1 — escalation + the case overlay.** Engine-minted escalation at count 2 (exactly-once, §4.6 writer); `fingerprint`/`case_id` on receipts; case derivation + the one-line case receipt in `doctor` and the mailbox surfaces. **Battery:** corpus replay mints exactly ONE escalation for the trilogy (at witness 2, not 3); the receipt derives the full case; deleting derived artifacts + recompute is bit-identical (CASE-INV-1 proven, not asserted).
- **S2 — absence sentinels.** `sentinel` on fixed receipts; `--sentinel-sweep` runner; RED-sentinel mints a witness into the existing case. **Battery:** the model case's sentinel runs green against the fixed engine; a deliberately re-broken fixture turns it RED and the minted letter lands in the same case.
- **S3 — claim-vs-measure export.** The `claims` counts block + `field-labels.jsonl` rows from honesty letters + receipts. **Battery:** the census's honesty letters produce exactly the expected labeled rows; counts render with denominators; no score appears anywhere (a grep gate, like the UI's air-gap grep).
- **S4 — abandonment `[depends: the ambient wave — hard dependency, not a preference]`.** The Stop-hook rule (§4.5), engine-minted honesty letters, exactly-once per session. **Battery:** a synthetic session transcript (error → silence → Stop) mints one letter; the same transcript without the Stop (session died with m1nd's error as the last event) mints NOTHING — the no-accusation-from-silence rule proven.

---

## 10. Known problems & honest gaps

- **S4 is gated on infrastructure that does not exist yet** (the ambient Stop-hook wave). The crown organ ships last, and the interim proxy was rejected on honesty grounds (§4.5) — abandonment stays invisible until it can be witnessed truthfully. This is a scope admission, not a schedule promise.
- **The kind vocabulary will be wrong somewhere.** Eight kinds cover the 66-letter census; the first uncoverable letter will arrive. The governance is the taxonomy's own precedent: extending the closed vocabulary is a PRD-amendment-grade change (a new word in a grammar), never an ad-hoc string — and the pressure signal is mechanical: kindless letters accumulating in `doctor`'s counts.
- **Backfill rules can misfile history.** They are deliberately narrow (deterministic, corpus-derived); a misfiled legacy letter is correctable by a receipt's explicit `answers[]`, and the derived overlay means re-filing is always a recompute, never a migration.
- **The bare-name pending gap** (§2.3, fix in flight at PRD time) means per-project boxes under-populate until resolved; fingerprinting over spool+boxes together is unaffected, but per-box case rendering inherits whatever the resolver misses.
- **Engine speech is a new trust surface.** §4.6's cage (reserved author, templates, dedup, observable-facts-only) is designed to keep the engine's mailbox voice as auditable as any agent's — the first engine letter that editorializes or double-mints is itself a `bug`-class letter against `m1nd:engine`, and the system audits itself with its own machinery.
