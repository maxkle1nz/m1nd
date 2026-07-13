# m1nd — THE ORGANISM FROM INSIDE + 360 — PRD

**The immune loop · the process memory · the presences · the portfolio (vision → spec)**

> **RATIFIED by the owner 2026-07-12** — verbatim: *"MUITO BEM! ratificado bora pra frente"*.
> Authored 2026-07-12 at the Fable seat, on the owner's approved vision of the same day
> (owner's reaction to the vision, verbatim: *"isso é perfeito"*). Versioned onto `main`
> @ `c305d07`+ as a §C11-style amendment (the PATHOS block carries the era entry); the UML
> sister lives at `docs/uml/organism-inside.md` (design-stage, deliberately out of the
> code-grounded atlas until the wires land).
> Grounded in `main` @ `7b3b665` (post-#354). Every claim about existing machinery below
> was re-verified against this tree, the LIVE served owner at `:1338`, the live `poold`
> daemon, and the live field spool — on 2026-07-12. Where a number is measured it carries
> its measurement; where a figure is an engineering estimate it is written in words, never
> as a precision bar (the HUMAN-LAYER discipline, applied to this PRD too).
> Sisters: `docs/ORGANISM-PRD.md` is the constitution and wins ties; this PRD opens an
> off-§C10 front and therefore lands as a §C11-style amendment, never a silent fork.
> The pre-arc in flight (the Gardener: auto-ingest + daemon + auto-reconcile, oracle
> verdict running today) is CONTEXT, not scope — nothing here depends on its landing.

---

## Resumo executivo (pt-BR — a vista do dono)

1. **Hoje o teu guardião operou o organismo POR FORA:** 8 PRs mergeados (medido: #347–#354),
   6 executores, 2 oráculos — e o quadro de missões do m1nd não viu NADA (medido ao vivo:
   21 cartas no quadro, todas de 09–11/07, zero de hoje).
2. **Este arco liga os fios que já existem:** dor vira carta de missão sozinha (o reflexo
   imunológico), missão fechada vira sabedoria de COMO delegar (memória de processo),
   agentes viram presenças visíveis na sala de controle, e teus 8 cérebros deixam de ser silos.
3. **Nada de novo poder:** o carimbo continua TEU — um segundo de soberania por decisão;
   nenhum agente pousa, nenhum agente ratifica, jamais (a lei do gate de origem, nascida hoje).
4. **Quatro fases + uma fase zero de doutrina**, cada uma fechando com prova viva no dono
   servido — nada conta como feito sem o teu olho.
5. **Custo honesto:** a maior parte é fio, não máquina — os verbos existem; o que falta é
   ligá-los e MEDIR que a ligação vale (métricas com baseline de hoje, todas no §7).

---

## 1. Thesis — the mother-confession

**The organism's agents are invisible to the organism itself.**

On 2026-07-12 the guardian ran the largest single-day burst in the repo's history from the
OUTSIDE: **8 PRs merged** (#347–#354, verified in git), six executor agents, two oracle
seats — and the m1nd mission board registered **none of it**. Measured live the same
evening (`GET /api/mailbox?kind=mission` on the served owner): **21 mission letters, dated
2026-07-09 (5), 2026-07-10 (15), 2026-07-11 (1) — zero letters from 2026-07-12.** The
board's newest activity is the pool era; today's work — the biggest work — left no trace
on the organism's own nervous system.

The same is true of the pain rail: the 2026-07-11 integrity burst closed all four open
field reports in **under 12 hours** — but through a MANUAL sweep of
`~/.m1nd/field-reports.jsonl` (115 letters at this writing), not through anything the
organism did for itself. The pieces all exist as verbs; **what is missing is the wires.**

Four connections make the organism see, heal, learn, and span itself:

1. **The organism that heals itself** (the immune loop): a field report becomes a mission
   charter automatically → the warm pool executes → the gate proves → a receipt candidate
   waits in the tray → the HUMAN stamps. The judge seat pre-filters, advisory forever.
2. **The organism that learns to delegate** (the process memory): every closed mission
   debriefs → a distillate passes the existing C8.3/C8.4 gauntlet → the medulla accumulates
   wisdom about HOW to drive agents (which spec shapes work, which executor fails at what) —
   not just knowledge about code.
3. **Agents as presences** (the control room): sessions become visible presences in the
   Hall/cockpit/tray — who is working, on what, since when — and collisions (two mutating
   hands on one theme) surface BEFORE they cost two hours of recovery (they have cost
   exactly that: the 2026-07-06 worktree collision, the 2026-07-10 twin-brain incident —
   both discovered after the damage).
4. **Federate 360** (the portfolio organism): the owner's brains stop being silos —
   doctrine and patterns cross with provenance always visible (*"brain loja cured this pattern
   in June; this memory came from there"*), under the medulla's pull-only law, never code,
   never across client boundaries.

**What this arc is NOT** (kills, stated up front — full list in §9):

- Not a new power for agents. Landing, ratifying, and promoting-to-doctrine keep every
  existing gate; the human-origin gate born today (#353) is the arc's model, not its victim.
- Not a new mission schema. The `m1nd-mission-letter-v0` chain carries the whole arc;
  new letter fields, if any, are argued at implementation against `deny_unknown_fields`.
- Not a cloud. Everything is local-first; the spool never phones home; federation means
  the one served owner's own brains, on this machine.
- Not autonomy theater. Every phase gate is a live proof on the served owner with the
  owner's eye on it — suites green alone do not count (the owner's standing doctrine,
  2026-07-10: a function is only real when driven for real).

---

## 2. Personas — one human, many hands, one body

| Persona | Who | What this arc gives them | Sovereignty cost |
|---|---|---|---|
| **THE OWNER** | One human. Ratifies maps, stamps receipts, reads the tray. | The team becomes visible (presences), the pain heals while he sleeps (immune loop), the stamp arrives PRE-JUDGED (advisory parecer), the portfolio speaks as one. | **One second per decision** — a stamp stays one gesture; nothing in this arc adds a second gesture or removes one. |
| **THE AGENTS** | The orchestrator (plans, spawns, verifies), the executors (worktree-isolated missions), the pool hand (h4nd poold, always-on), the runners (build/naming/hand via runnerd), the judge (advisory oracle). | A board that reflects their real work; packets that carry process wisdom; presence instead of anonymity; refusals that teach instead of silent wrong-writes. | Zero new write powers. The hand still never lands; the judge still decides nothing in effect. |
| **THE ORGANISM** | The served owner (`:1338`): brains, graph, medulla, mission board, spool, tray. | Wires between its own organs: spool→board, board→medulla, sessions→Hall, brain→brain. | Budget Law holds on every packet it speaks (§4, law 3). |

---

## 3. The four connections — capability by capability

Each connection is specified the same way: **what exists** (verified at `7b3b665` /
live), **the new wire** (honest net-new), and **the data contract sketch**.

### 3.1 Connection 1 — the immune loop (the organism that heals itself)

**The full cycle:** field report → charter → spawn → pool/runner executes → gate proves →
receipt candidate → tray bell → judge parecer (advisory) → **human stamp** → landed letter.

**What exists (all verified):**

- **The pain diary.** `~/.m1nd/field-reports.jsonl` — append-only spool, 115 letters,
  shape `{ts, agent, repo, tool, class: bug|honesty|friction|win, what, expected, snippet}`
  (universal field-telemetry doctrine, PATHOS). Distribution into per-brain boxes:
  `inbox_sweep` / `resolve_box` (`m1nd-mcp/src/mailbox.rs`), fates derived at read time.
- **The mission rail.** `mission_post` (`server.rs`, F2.5a): per-mission hash chain,
  head-CAS `stale_head`, per-phase gating, the §1d landed-law (a green gate without an
  imported receipt is `merge_wait`, never `landed`), the §1f no-absolute-path guard, the
  §1g `unknown_block` refusal with the `synthetic:true` smoke escape, the #321
  `brain_mismatch` refusal. Phases (verbatim enum, `mission_letter.rs:83`):
  `judging | executing | gate | review | merge_wait | landed | failed`.
- **The spawner.** `mission_spawn` — HTTP-only owner→runnerd proxy (the browser never
  holds the secret); runnerd pins capabilities owner-side (`runners.toml`), runs the
  packet in a worktree-per-mission, composes a complete `receipt_candidate` on zero exit,
  holds NO `receipt_import` permission and never lands. Live right now: 3 runners
  announced (`build-01`, `hand-01`, `naming-01` → `:1339`, measured via
  `GET /api/runnerd/status`).
- **The warm pool.** h4nd `poold` (god-hud, read-only reference): launchd daemon, 20s
  sweep, fail-closed claims anchored on the reserved `sb_m1nd_pool_` namespace, keep-alive
  warm (measured live: sweep #3692, uptime 20h45m, cached_tokens 640), cold-lane handoff
  spool, Money-Zone `REFUSED-HUMAN`. Its 8 house laws (Sacred Law, never-lands,
  NONE-escape…) are inherited contract.
- **The judge seat.** `judge.py` — advisory parecer per `merge_wait` head
  (gpt-5.6-sol, xhigh), total NONE-escape (any engine failure → honest ABSTAIN → wire
  `CHANGE` with a gist that says "abstained"), never composes `landed` (mechanical guard),
  rides the honest `judging` phase after the live owner REFUSED a gateless `merge_wait`
  parecer (3/3 refusals in the poold log, 2026-07-11 — the wire's law held). **DISARMED
  by default** behind `H4ND_JUDGE_ENABLED=1` (`poold.py:660`, commit `c22539c`): a
  judging-phase parecer would silence the head-phase landing bells before the human
  stamps — sovereignty rings first. Calibration exercise reported 2/2 correct verdicts
  (orchestrator-reported 2026-07-12; **not yet a versioned proof file** — honest).
- **The landing.** `receipt_import` with the **human-origin gate** (#353): gate 0 —
  `imported_via` must be on the closed server-side allow-list
  (`RECEIPT_IMPORT_HUMAN_ORIGINS` = `"human-ui"` the owner's screen, `"human-touchid"` the
  h4nd tray's native prompt landed behind Touch ID); absent or off-list refuses
  `human_gesture_required`, nothing applied. Plus OCC, `stale_scope`, evidence contract,
  temporal-coherence guards — all live-fire proven.

**The new wire (honest net-new):**

1. **The charter composer** — a pure function from an eligible spool letter to a
   spawn-ready mission charter (a seq-1 `judging` letter + a composed packet). New code;
   everything it emits rides existing rails and existing refusals.
2. **The trigger** — where the composer runs. Slice decision: it runs inside the existing
   sweep cadence (the owner's daemon tick or the operator CLI, both exist) — never a new
   always-on process for v1.
3. **The caps** — anti-spam ceilings (below, and §8 R1).
4. **Smart-bells** (h4nd side, prerequisite for arming the judge): the tray/north/tray-app
   learn that an oracle verdict letter over a waiting chain still means "awaiting the
   human landing" — the bell keeps ringing on the HEAD's `merge_wait`, informed by the
   parecer instead of silenced by it. This is queued at the h4nd house; the arc's phase
   P2 declares it a dependency, honestly.

**Data contract sketch — the charter (`m1nd-immune-charter-v0`, composer output):**

```
source_report:   {report_id: sha256(raw line)[0..12], ts, agent, repo, tool, class, what, expected, snippet}
eligibility:     class ∈ {bug, honesty}                  — friction/win stay human-triaged in v1
                 AND repo resolves to a brain the owner holds (resolve_box law — never Pending)
                 AND a gate is derivable (the block's declared gate, or the repo's default
                     battery command) — NO GATE, NO CHARTER (refusal no_gate_derivable)
                 AND block resolvable via the ratified skeleton (else refusal unknown_block —
                     the #328 guard is the enforcement; the charter NEVER passes synthetic:true)
charter:
  letter:        m1nd-mission-letter-v0, seq 1, phase judging, seat oracle,
                 capability routed (loop-runner | build-runner), brain_ref = owning brain,
                 block_id = resolved block, packet_ref = the packet below
  packet:        symptom VERBATIM (what/expected/snippet — quoted, never paraphrased) +
                 the battery-case-first instruction (a confirmed field bug becomes a test
                 BEFORE the fix — existing triage doctrine, now traveling IN the packet) +
                 the gate command + the report_id as provenance
  provenance:    rides INSIDE packet markdown + the letter's packet_ref string —
                 MissionLetter is deny_unknown_fields; a dedicated `source` field is a
                 v1-schema decision argued at implementation, not assumed here
caps:            MAX_AUTO_CHARTERS_PER_SWEEP = 1 · MAX_OPEN_AUTO = 3 (open = not landed/failed)
                 dedup by report fingerprint (tool + class + hash(what)) — one report, one
                 charter, forever (a re-fire needs a NEW report)
refusals:        no_gate_derivable | unknown_block | cap_reached | duplicate_report |
                 foreign_brain | ineligible_class — each a logged, honest non-event
```

**What the loop can never do:** land (`receipt_import` human-origin gate), ratify,
silence a bell, touch a brain other than the report's own, or spend without ceiling.

### 3.2 Connection 2 — the process memory (the organism that learns to delegate)

**The cycle:** mission closes → debrief → distillate → the existing gauntlet → medulla.

**What exists (all verified):**

- **The delegation debrief.** `debrief` (`delegation_handlers.rs`) — grades a subagent's
  REAL diff against the packet it was handed: path classification
  (`in_scope | expected_change | dependent_contact | unpredicted`), worst-of conformance
  verdict, findings memorized under the subagent's id, lessons under the grader's id,
  asymmetric `learn` teaching, exactly one `outcomes.jsonl` row (stamped
  `outcome_unverified` without evidence). The calibration reducer measures the three
  §O.12.8 metrics at N ≥ 30 (hardening wave 4) — it measures, it does not yet tune.
- **The audited crossing.** `promote` (R4/M6): Origin-Brain + Origin-Claim + Promoted-By +
  Promotion-Reason riders; **C8.3** — only `State: verified` or founder-sourced claims may
  promote (declared maker findings stop one verb short of doctrine); **C8.2** — evidence
  re-anchored origin-qualified or stamped `evidence_unverifiable` (a medulla claim never
  reads fresher than it can prove); WouldDowngrade bounce; demotion never touches the
  witness.
- **The curator laws.** **C8.4** — evidence union over children's CODE paths (claims never
  cite claims), merge-and-recite (the curator may not author), confidence caps at
  max(children), and the seat law: curator output is verified by a DIFFERENT agent than
  the one that curated (grader ≠ author — `soul_check {verify_curator_report}` refuses
  grader==author).
- **The write door.** `memorize` is the sole L1GHT writer; Origin-Brain stamps; C8.5 —
  letters are witness tissue, a letter path in `evidence:` is refused at the write door.

**What does NOT exist (the honest gap):** the mission LETTER rail has no debrief. A pool
or runnerd mission ends at `landed | failed` and teaches nothing — the packet quality,
the runner's real latency, the instruction that was missing, the gate that flaked: all of
it evaporates. The proof this matters is already lived: #331 (a real CLI naming runner
measures ~50s/call while the budget assumed 20s; the naming packet carried no task
instruction and a generic LLM wandered to timeout) was learned by HUMAN forensics, twice.
The organism should have known after the first time.

**The new wire:**

1. **The mission debrief step** — at mission close (landed/failed/abandoned-expiry), a
   debrief distillate is composed and written through the EXISTING write door into the
   orchestration brain, project-private, tagged `kind:process`. Whether this is a new
   thin verb, a `debrief` mode keyed by `mission_id` instead of `delegation_id`, or an
   orchestrator duty (doctrine-only in P0) is argued at implementation against the
   constitution's future-verb rule (§C6.3: zero-new-verbs is the null hypothesis).
2. **The distillation shape** (below) — bounded, verbatim-quoting, never authored prose.
3. **The packet feedback** — `delegate` packets and charter packets gain a
   `context.process` row sourced from `kind:process` recall (the same mechanism as
   `context.memory` today) — so the NEXT spec is composed with the last mission's lesson.

**Data contract sketch — the distillate (`m1nd-mission-debrief-v0`):**

```
mission_id · outcome (landed|failed|abandoned) · capability · runner_id · wall_clock
spec_quality:   {packet_tokens, instruction_carried: bool, gate_command,
                 gate_first_try: bool, retries: n}
friction:       [verbatim strings — quoted from letters/logs, never paraphrased]
lessons:        ≤3 claims, each ≤2 sentences, evidence = CODE paths only
                (letter ids may ride the body as provenance — C8.5: never in evidence:)
sink:           memorize → orchestration brain, project-private, kind:process,
                State: authored (NEVER born verified)
promotion path: authored → verified (a later mission's measured contradiction/confirmation,
                or the owner's word) → promote (C8.3 gate) → C8.4 curator consolidation,
                seat-verified — THE GAUNTLET IS UNCHANGED; this arc adds zero shortcuts
```

**The law that keeps the medulla clean:** a distillate is DECLARED tissue at birth. It
cannot promote until verified (C8.3 — existing, untouched). The gauntlet is the filter;
the arc only feeds it.

### 3.3 Connection 3 — agents as presences (the control room)

**What exists (verified):**

- **Instance registry.** `instance_registry.rs` — PID + heartbeat lease per
  `runtime_root`, `instances/<id>.json` discoverable entries, ReadWrite/ReadOnly modes,
  GC sweep; feeds `/api/instances` (the Hall's brains surface, PROJECT-named).
- **Runnerd announce.** In-memory liveness registry `runner_id → (port, last_seen)` —
  liveness only, grants no capability. 3 live runners right now.
- **Trails.** `trail_save / trail_list / trail_resume` — per-agent investigation state
  with labels, hypotheses, staleness detection.
- **Handshake.** `session_handshake` — a per-call TRUST verb (binding fingerprint,
  degraded-host detection); it does NOT register anything durable.
- **Render surfaces.** The Hall (every brain as a project card, aliveness dots), the
  `cockpit` verb born today (#349: seven stable slots, `menu_sig`, read-only derived),
  the h4nd tray (native bell on `merge_wait`).

**What does NOT exist:** a roster of live SESSIONS. Brains have aliveness; agents do
not. Two orchestrators, six executors and a pool hand can work one evening and no
surface can answer *"who is working, on what, since when — and are two of them about to
collide?"*. The cost is paid history: the 2026-07-06 two-hands-one-worktree collision
(~2h recovery), the 2026-07-10 twin-brain incident — both post-hoc discoveries.

**The new wire:**

1. **The presence record** — a runtime sidecar in the registry-dir pattern the instance
   registry already uses (`presences/<id>.json`), TTL'd, heartbeat-refreshed, honest-absent
   on expiry. No new always-on process: the beat rides calls the agents already make.
2. **The register/beat path** — reuse-first decision to argue at implementation:
   piggyback optional presence fields on `session_handshake` (it is already the session's
   first call) + refresh on any verb via the session identity, OR a thin `presence` verb.
   The constitution's §C6.3 makes zero-new-verbs the null hypothesis.
3. **Collision derivation** — pure read-time function (never stored): two live presences
   with `intent: mutate` whose declared working sets (paths, blocks, or branch) overlap →
   a `collision` block on the read surface + a line in the north packet's honest gaps for
   BOTH sessions. Advisory, never blocking — the organism warns, the human/orchestrator
   decides (the same posture as reception).
4. **Renders** — a presence strip in the Hall, a cockpit collection slot, the tray's team
   view. All read-only, all absent-honest, all under the Budget Law.

**Data contract sketch — the presence (`m1nd-presence-v0`, sidecar):**

```
presence_id:  prs_<12hex>
agent_id · kind (orchestrator|executor|pool-hand|runner|oracle|human-ui)
brain:        the bound brain's display name (from the session's own binding — never claimed)
theme:        one line, free text ("F12 curation lane", "reader slice 1")
intent:       read | mutate
working_set:  [repo-relative paths and/or sb_ block ids] — optional, honest-absent
worktree:     branch/worktree display string — optional
started_at · last_beat · ttl_s (default: minutes-scale, expiry = honest absence)
collision:    DERIVED at read: [{with: prs_id, overlap: [paths|blocks], both_mutate: bool}]
```

**Laws:** presences are self-declared telemetry (witness tissue — they verify nothing,
gate nothing); a dead presence disappears rather than lying; the roster never renders a
presence the registry did not serve (INV-10's discipline applied to sessions).

### 3.4 Connection 4 — Federate 360 (the portfolio organism)

**What exists (verified — and what today's telemetry says about it):**

- **Graph federation.** `federate` / `federate_auto` — multi-repo ingest into ONE
  namespaced graph with cross-repo edge detection. This is a CODE-graph feature. Honest
  live telemetry from TODAY'S spool (2026-07-12 18:11/18:28): two read-only seek calls
  over a 278-node federated graph did not return after ~50s; a re-federation of two
  immutable snapshots did not return after ~60s. Graph federation exists and has open
  performance wounds — this arc does NOT build on it.
- **Memory crossing.** The medulla spine (R2→R4, shipped): per-brain stores,
  `Origin-Brain` labels on every claim, tier recall with the no-leak invariant proven,
  `all-brains` recall through the eviction gate (R3), and the ONE audited write-crossing:
  `promote` (C8.2/C8.3 riders, §3.2 above). **The pull law:** a brain's default beat =
  its own store + the medulla, nothing else.
- **The provenance vocabulary.** `Origin-Brain`, `Origin-Claim`, `Promoted-By`,
  `Promotion-Reason`, `Evidence-Unverifiable` — every field the honest cross-brain line
  needs already exists in frontmatter.

**The judgment this PRD is asked to make — what federates:**

| Tissue | Crosses? | Why |
|---|---|---|
| **Medulla doctrine** (promoted claims) | **YES — it already does.** | That is the medulla's purpose; the arc adds VISIBILITY, not a new channel. |
| **Project memories** (project-private claims) | **Pull-only, explicit, labeled.** | `tier:"all-brains"` recall exists (R3) — an explicit query, never the default beat. The arc renders its provenance; it does not widen it. |
| **Process distillates** (§3.2) | **YES, through the same gauntlet.** | "The naming runner needs the instruction inside the packet" is transversal doctrine — exactly what C8.3-verified promotion is for. |
| **Field-report patterns** | **YES as doctrine, NO as raw letters.** | A recurring cross-repo pain promotes as a claim; raw letters stay in their boxes (C8.5 — letters are witness tissue). |
| **Code receipts / SystemBlocks / skeletons** | **NEVER.** | A receipt's scope binds `(block_id, boundary_version, contract_version)` — meaningless outside its repo by construction. Skeletons are per-repo ratified maps. Nothing to share, everything to poison. |
| **Code itself** | **NEVER via memory rails.** | Cross-repo code reuse is git's job, not the medulla's. |
| **Anything across CLIENT boundaries** | **NEVER, mechanically.** | The isolation law below — future-proofed now, before a client brain ever exists. |

**The new wire:**

1. **The provenance render** — wherever a crossed claim surfaces (north memory strip,
   seek results, the human tray), the origin line renders: *"promoted from loja ·
   2026-06-xx · by <agent> — <reason>"* — every field already stored, none of it shown
   today. Absent provenance renders "origin unknown", never invented (INV-04's law).
2. **The portfolio view** — a read-only surface (a cockpit slot + a Hall lens) answering
   *"which brains hold doctrine, how fresh, what crossed recently"* — counts and lines
   from existing stores, no new store.
3. **The isolation allow-list** — a runtime-level `federation_policy` (default:
   `owner-personal` = all brains the owner holds may cross via the existing rails).
   A brain marked `isolated` (the future client case) is excluded from `all-brains`
   recall, from promotion INTO the medulla, and from the portfolio view — refusals, not
   silences. Ships default-permissive with the mechanism proven by test, so the day a
   client repo arrives the law exists before the risk does.
4. **The reuse meter** — cross-brain reuse becomes measurable: when a medulla claim with
   `Origin-Brain: A` is recalled into a session bound to brain B and survives the
   session's `learn` feedback (not marked wrong), that is ONE reuse event, counted in the
   existing outcomes/metrics pattern. No new judgment machinery — a counter over events
   that already fire.

**Data contract sketch — the crossed-claim render row (`m1nd-crossbrain-row-v0`, a
RENDER, not a store):**

```
claim slug + label + tier (medulla|project)
origin:       {brain: Origin-Brain, promoted_by, promoted_at, reason}   — verbatim frontmatter
evidence:     verified (re-anchored, C8.2 channel a) | evidence_unverifiable (channel b)
              — declared tissue never wears verified formatting (C8.1)
isolation:    present iff the serving policy excluded brains: {excluded_count} — honest
```

---

## 4. THE LAWS THAT DO NOT BEND

Numbered, constitutional-style. Each names its enforcement point. A slice that cannot
hold one of these does not land.

1. **SOVEREIGNTY — the human stamp is the only landing.** `receipt_import`'s human-origin
   gate (closed server-side allow-list, #353) and `system_blocks_ratify`'s `human-ui`
   guard stay exactly as they are. **No auto-ratify exists, ever, in any phase, under any
   flag.** The immune loop's output is a WAITING candidate, never a landed one.
2. **NEVER-LANDS.** No component this arc touches or creates composes a `landed` letter
   or calls `receipt_import`: not the charter composer, not the pool, not the judge
   (mechanical guard raises on `landed`), not the debrief step, not the presence layer.
   Grep is the guardian (the h4nd pattern); a CI check pins it.
3. **THE BUDGET LAW (constitution §C1.3).** north stays ≤2k (pinned ~1,404 tokens
   2026-07-12); cockpit stays ≤800 (~695 root / ~430 drill). Every packet line this arc
   adds (presence gap-line, process row, provenance line) is measured and re-pinned in
   the landing PR — a budget breach is a failed gate, not a footnote.
4. **G1 — MEASURED FACTS ONLY.** No surface invents a number: presence counts come from
   the registry, reuse counts from real events, immune metrics from real letters. A
   metric without a measurement renders absent (violet-unknown discipline), never
   estimated.
5. **FAIL-OPEN for voice, FAIL-CLOSED for writes.** Read surfaces (presence roster,
   portfolio view, provenance lines) never take north or the cockpit down — they drop
   whole, honestly. Write paths keep their refusal grammar and add the arc's own:
   `no_gate_derivable | cap_reached | duplicate_report | ineligible_class` on the
   charter; isolation refusals on federation. A refusal is always a logged, teaching
   non-event — never a silent skip.
6. **THE ORIGIN-GATE LAW (born 2026-07-12, #353 — now generalized).** Every landing rail
   carries a closed, server-side origin allow-list that grows ONLY in code. If a future
   slice ever proposes a native non-UI landing gesture, it argues against this law in
   writing; the default answer is no.
7. **THE MEDULLA LAW (pull, never push) — unchanged.** Crossing is `promote` (C8.2
   re-anchor + C8.3 verified-only) or an explicit `all-brains` pull; the default beat
   never widens. The gauntlet (C8.3 + C8.4 grader ≠ author) filters everything the
   process memory feeds it — this arc adds volume, zero shortcuts.
8. **PROVENANCE ALWAYS VISIBLE.** A crossed claim renders its origin line or renders
   "origin unknown" — it never renders as native. (C8.1: declared tissue never wears
   verified formatting.)
9. **CLIENT ISOLATION IS MECHANICAL, NOT DISCIPLINE.** The `isolated` policy excludes a
   brain from every crossing rail with refusals — proven by a leak-permutation test
   before any client brain exists.
10. **LETTERS ARE STATE, NOT EVIDENCE (constitution C8.5 — unchanged).** A mission letter
    never colors a block; a field report never verifies a claim; a presence never proves
    work happened. The map's colors move only by receipt.
11. **REPORT-NEVER-FIX STAYS.** The immune loop automates TRIAGE, not mid-mission
    surgery: a sensing agent still only appends its letter and keeps working. The charter
    is born LATER, at the sweep, by the composer — the reporting agent never becomes the
    fixing agent inside the same mission.
12. **LOCAL-FIRST.** No network federation, no cloud relay, no telemetry leaving the
    machine. "360" means one owner, one machine, all its brains.

---

## 5. Phases — proof-gated slices, each closing with live proof on the served owner

The order is chosen so the cheapest honesty lands first: the confession is answered
first by USING wires that exist (P0), then by building the four new ones. Each phase is
burst-sized (the house's working unit); sizes in words, never dates.

### P0 — WEAR THE WIRE (doctrine + skills; near-zero engine code)

The mother-confession's cheapest cure: the guardian's own workflow starts speaking the
rails that already exist. Every spawned executor mission gets a mission letter chain
(`mission_post`: opened at spawn, `executing`, closed `landed`-by-the-human or `failed`);
every delegation gets its `debrief`; the board becomes the day's truth. Plus board
hygiene: the 13 failed pool-era smokes get their archive lane so the board reads signal.

- **Touches:** skills/M1ND_INSTRUCTIONS/orchestrator doctrine (the agent-docs CI gate
  applies — same-PR surface updates); possibly a `failed`-fold render decision (the
  failed-never-folds law stays; hygiene = archive, never hide).
- **Gate (live, owner's eye):** one REAL burst (≥3 executors) fully visible on the
  board as it happens — letters opened, phases moving, closes honest; `mission_post`
  volume measured before/after (baseline: 0 letters on 2026-07-12's 8-PR day).
- **Size:** small.

### P1 — PRESENCES (the control room sees the team)

The presence sidecar + register/beat path (reuse-first per §3.3) + collision derivation +
renders (Hall strip, cockpit slot, tray team view) + the north honest-gap line on
collision.

- **Gate (live):** two real mutating sessions visible with themes and ages; an ARRANGED
  same-block collision surfaces on both sessions' north packets and on the Hall BEFORE
  either lands anything; TTL expiry proven (a killed session disappears within its TTL,
  never lingers as a ghost); budgets re-pinned.
- **Size:** medium.

### P2 — THE IMMUNE REFLEX (pain becomes a charter by itself)

The charter composer + eligibility screen + caps + dedup, running on the existing sweep
cadence; the judge armed as TRIAGER once h4nd's smart-bells land (declared dependency —
if smart-bells slip, P2 ships with the judge still disarmed and says so).

- **Gate (live):** ONE real report from the production spool (class bug|honesty) becomes
  a posted charter → spawned → gate green → `merge_wait` with a complete candidate → the
  judge's advisory parecer on the chain (if armed) → **the owner stamps** → `landed`,
  store bumped. Refusals proven live the same day: a `cap_reached`, a
  `no_gate_derivable`, and a `duplicate_report`, each a logged non-event. Zero
  auto-landings by construction AND by grep (law 2's CI check).
- **Size:** medium-large (the composer is real code; the rails are rented).

### P3 — THE PROCESS MEMORY (missions leave wisdom)

The mission debrief step at close + the distillate shape + `kind:process` write-through +
the packet feedback row (`context.process` in delegate/charter packets). Promotion stays
manual through the untouched gauntlet.

- **Gate (live + measured):** after a real week of missions, a packet composed WITH
  process rows is A/B'd against one without on the same task class (the cold-agent
  packet-validation pattern, reused): the process-fed packet must not lose, and at least
  one lesson (e.g. the #331 instruction-in-packet class) must demonstrably ride a packet
  instead of a human memory. One distillate walked through verified → promote → C8.4
  seat-verified consolidation, end to end, owner watching.
- **Size:** medium.

### P4 — FEDERATE 360 (the portfolio speaks as one)

The provenance render + the portfolio view (cockpit slot + Hall lens) + the isolation
allow-list mechanism (default-permissive, test-proven) + the reuse meter.

- **Gate (live + measured):** a claim promoted from brain A (real, e.g. a process lesson)
  surfaces in a session bound to brain B with its full origin line rendered; ONE reuse
  event counted by the meter from a real session; the leak-permutation battery extended
  with the `isolated` policy case (an isolated brain's claims reach NOTHING, refusals
  fire); the federated-graph performance wounds from today's spool are explicitly NOT
  claimed fixed by this phase (separate triage — this phase builds on the medulla rails,
  not the federated graph).
- **Size:** medium.

**Cross-phase:** every phase carries the universal doc gate (docs/wiki/README/PATHOS +
agent surfaces in the same PR — the agent-docs CI gate arms on these), lands RED-first
where a defect is being pinned, and re-pins every budget it touches.

---

## 6. What already exists vs what is new wire — the honest ledger, one table

| Piece | Status at `7b3b665` | This arc |
|---|---|---|
| Field spool + sweep + boxes/fates | **EXISTS, live** (115 letters) | rents it (P2 reads, never re-shapes) |
| Mission letter chain + gates (§1b–§1g) | **EXISTS, live-fire proven** | rents it (P0 uses, P2 posts into it) |
| `mission_spawn` proxy + runnerd + worktree-per-mission | **EXISTS, 3 live runners** | rents it |
| Warm pool daemon (h4nd poold) | **EXISTS, live 20h+ uptime** | rents it (read-only reference; its laws inherited) |
| Judge seat (advisory, NONE-escape) | **EXISTS, DISARMED** (flag + smart-bells dependency) | arms it in P2, advisory forever |
| `receipt_import` human-origin gate | **EXISTS (born today, #353)** | the arc's model law (law 6) |
| Charter composer + caps + eligibility | **NEW** | P2's real code |
| `debrief` (delegation layer) + outcomes + calibration reducer | **EXISTS** | rented in P0; pattern-source for P3 |
| Mission-close debrief + distillate + `kind:process` | **NEW** | P3's real code |
| `promote` C8.2/C8.3 + C8.4 curator seat law | **EXISTS, shipped R4** | untouched gauntlet — the arc only feeds it |
| Instance registry + Hall + cockpit + tray | **EXISTS** (cockpit born today, #349) | render host for P1/P4 |
| Presence records + collision derivation | **NEW** | P1's real code |
| `session_handshake` / trails | **EXIST** (trust verb / investigation state) | candidate carriers for the presence beat (reuse argued at implementation) |
| Medulla tiers + `all-brains` recall + no-leak battery | **EXISTS, shipped R2–R4** | rented in P4 |
| `federate`/`federate_auto` (graph) | **EXISTS — with live friction reports from today** | explicitly NOT the base of P4; wounds triaged separately |
| Provenance render + portfolio view + reuse meter | **NEW** | P4's real code |
| Isolation allow-list (`federation_policy`) | **NEW (mechanism), default-permissive** | P4, test-proven before any client exists |
| Smart-bells (verdict-over-waiting) | **NEW, h4nd side, queued there** | P2 dependency, declared |

---

## 7. Metrics — honest, baselined today (2026-07-12)

Every metric names its baseline measurement. A metric that cannot be measured is not
promised.

| Metric | Baseline (measured today) | Direction the arc must prove |
|---|---|---|
| Mission letters per real executor mission | **0 / 8-PR day** (board: 21 letters, none from today) | → every spawned mission posts (P0 gate) |
| Pain → receipt wall-clock | **< 12h, via MANUAL sweep** (2026-07-11 integrity burst) | → median measured with ZERO human triage steps for eligible classes (P2) |
| % eligible spool reports auto-chartered without a human | **0%** (composer does not exist) | → measured %, with refusal counts beside it (never hidden) |
| Auto-charter spam | n/a | → open-auto count NEVER exceeds 3; cap refusals logged (P2 gate proves one live) |
| Collisions surfaced before damage | **0 — two known incidents, both post-hoc** (2026-07-06, 2026-07-10) | → an arranged collision surfaces pre-landing (P1 gate); real ones counted thereafter |
| Ghost presences | n/a | → TTL expiry proven; roster never renders an expired presence |
| % closed missions leaving a distillate | **0%** (no debrief on the letter rail) | → measured %; distillate quality gated by the A/B (P3 gate) |
| Process lessons riding packets | **0** (the #331 lesson lives in humans and PATHOS) | → ≥1 demonstrably in a real packet (P3 gate) |
| Cross-brain reuse events (provenance-rendered, learn-surviving) | **unmeasured / no meter** | → meter exists and counts ≥1 real event (P4 gate) |
| Medulla leak permutations incl. `isolated` policy | no-leak battery EXISTS; isolation case absent | → extended battery green (P4 gate) |
| Judge triage record | **disarmed; 3/3 owner refusals on 2026-07-11 (fixed); calibration 2/2 reported, unversioned** | → a versioned calibration ledger BEFORE default-arming; ABSTAIN rate visible; human-override rate tracked |
| north / cockpit budgets | **~1,404 / ~695+~430 tokens** | → re-pinned ≤2k / ≤800 after every phase that touches a packet |

---

## 8. Risks — each with its mitigation named

- **R1 — auto-spawn spam / runaway cost.** *Mitigation:* the caps are law (1 charter per
  sweep, ≤3 open autos, dedup-forever by report fingerprint), classes restricted to
  `bug|honesty` in v1, refusals logged and counted (§7). The ceiling is enforced in the
  composer, proven by a live `cap_reached` at the P2 gate.
- **R2 — debrief garbage polluting the medulla.** *Mitigation:* distillates are born
  DECLARED (`State: authored`), the C8.3 verified-only gate blocks promotion, C8.4's
  grader ≠ author seat check verifies consolidation, confidence caps at max(children).
  The gauntlet already exists and is untouched — the arc cannot weaken it without
  violating law 7.
- **R3 — federate leaking context between brains (someday: clients).** *Mitigation:*
  provenance always visible (law 8), crossing rails unchanged (pull-only + audited
  promote), the `isolated` policy mechanical and battery-proven BEFORE any client brain
  exists (law 9). Raw letters and receipts never cross by design (§3.4 table).
- **R4 — the judge biasing the human (a confident-wrong APPROVE).** *Mitigation:*
  advisory forever (law: the parecer is STATE; the wire has no ABSTAIN so abstains map to
  CHANGE with a gist that SAYS "abstained" — never a fabricated decision); disarmed until
  smart-bells; a versioned calibration ledger before default-arming (§7); the
  poisoned-oracle threat model stays OPEN and is inherited honestly from PATHOS — the
  judge never graduates past advisory while it is open.
- **R5 — presence spam / stale ghosts / roster lies.** *Mitigation:* TTL + heartbeat,
  honest expiry, self-declared-witness law (presences gate nothing), the INV-10
  discipline (render only what the registry served).
- **R6 — the board drowning in noise (13 failed smokes already).** *Mitigation:* P0's
  hygiene lane (archive, never hide — failed-never-folds holds); auto-charters are
  visually provenance-marked so a human scans human-born vs immune-born at a glance.
- **R7 — the confession's real cause was doctrine, not machinery — and machinery gets
  built anyway.** *Mitigation:* P0 is doctrine-first and MEASURES the wire-wearing before
  any new engine code lands; if P0's gate alone closes most of the visibility gap, P1+
  proceed for the parts doctrine cannot do (collisions, auto-charter, process rows) with
  the measurement in hand.
- **R8 — scope creep into the federated GRAPH's open wounds.** *Mitigation:* today's two
  friction reports (seek >50s, re-federate >60s on a 278-node federated graph) are
  triaged on the normal field rail, explicitly OUT of P4's promises (§5 P4 gate).

---

## 9. OUT OF SCOPE — named kills

- **Auto-ratify / auto-land: NEVER.** Not behind a flag, not for smokes, not for the
  immune loop's own fixes. (Laws 1, 2, 6.)
- **Cloud / network federation: NO.** No remote owners, no cross-machine sync, no
  telemetry export. Local-first is constitutional.
- **A new mission schema: NO.** `m1nd-mission-letter-v0` carries the arc; field additions
  are per-slice arguments against `deny_unknown_fields`, defaulting to "ride the packet".
- **A new always-on daemon: NO (v1).** The composer rides existing sweep cadences; the
  presence beat rides existing calls. (The Gardener pre-arc owns the daemon question.)
- **The judge as a gate: NO.** It never blocks, never lands, never silences a bell.
  Graduating it past advisory would be a constitutional amendment with the
  poisoned-oracle problem solved first — not this arc.
- **Replacing the delegation layer: NO.** `delegate`/`debrief` stay; the letter rail and
  the process memory COMPOSE with them.
- **Cross-brain code/receipt/skeleton sharing: NO.** (§3.4 judgment table.)
- **Autonomy KPIs ("N fixes shipped without humans") as success:** NO — the arc's
  success metric is sovereignty held (one second per decision) WITH the loop closed, not
  human-elimination.

---

## Appendix A — verified anchor index (at `7b3b665` + live probes, 2026-07-12)

| Fact | Anchor |
|---|---|
| Mission letter phases (7) verbatim | `m1nd-mcp/src/mission_letter.rs:83` |
| `mission_post` gates (§1b–§1g incl. `unknown_block`, `brain_mismatch`, head-CAS) | `server.rs` tool schema "mission_post" (~:2893) |
| `mission_spawn` HTTP-only proxy, never lands | `server.rs` (~:2905 + dispatch refusal ~:4797) |
| `receipt_import` human-origin gate (closed allow-list) | `m1nd-mcp/src/system_blocks_handlers.rs:499-536` · PR #353 |
| `debrief` grading contract (delegation layer) | `server.rs` (~:672) · `delegation_handlers.rs` · `docs/uml/delegation.md` |
| `promote` + C8.2/C8.3 riders | `server.rs` (~:2537) · `docs/ORGANISM-PRD.md` §C8.2/§C8.3 (SHIPPED) |
| C8.4 curator laws + grader ≠ author | `docs/ORGANISM-PRD.md` §C8.4 · `soul_check {verify_curator_report}` |
| C8.5 letters-never-evidence | `docs/ORGANISM-PRD.md` §C8.5 |
| Instance registry (lease + heartbeat + GC) | `m1nd-mcp/src/instance_registry.rs` |
| Runnerd announce registry (liveness only) | `m1nd-mcp/src/http_server.rs` (~:334) |
| Live runners: build-01, hand-01, naming-01 → :1339 | `GET /api/runnerd/status` (live probe 2026-07-12) |
| Board truth: 21 letters — 5×07-09, 15×07-10, 1×07-11, 0×07-12; 13 failed / 4 landed / 3 merge_wait / 1 judging | `GET /api/mailbox?kind=mission` (live probe 2026-07-12) |
| 8 PRs merged today | `git log --merges --since=2026-07-12` → #347–#354 |
| Field spool shape + 115 letters + today's federate friction pair | `~/.m1nd/field-reports.jsonl` (live read) |
| poold live: sweep #3692, 20h45m, judged_total=0, warm cached_tokens=640 | `~/.m1nd/pool/poold.log` (live read) |
| Judge disarmed behind `H4ND_JUDGE_ENABLED=1`; honest `judging` phase; owner refused gateless parecer 3/3 | `god-hud/h4nd-pool/poold.py:660` · `judge.py` · commit `c22539c` · poold.log 2026-07-11 |
| h4nd house laws (Sacred Law, never-lands, NONE-escape) | `god-hud/docs/PATHOS.md` (Doutrina) |
| Medulla no-leak + `all-brains` + eviction gate | PATHOS cp (R2–R4, R15 SHIPPED) · `docs/uml/medulla.md` |
| north ~1,404 ≤2k · cockpit ~695/~430 ≤800 | PATHOS checkpoint 18 (battery-pinned 2026-07-12) |
| Mailbox fates + sweep + resolve_box ownership law | `m1nd-mcp/src/mailbox.rs` · `docs/uml/mailbox.md` |
| Collision history (post-hoc discoveries) | PATHOS cp14 (two-hands-one-worktree) · cp16 (twin-brain guardrail #332/#333) |

*Authored at the Fable seat, 2026-07-12, for the owner's stamp. Where this draft and
reality diverge tomorrow, trust reality, amend this draft, and log a letter.*
