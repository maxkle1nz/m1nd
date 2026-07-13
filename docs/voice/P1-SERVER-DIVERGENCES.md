# ORGANISM-INSIDE P1-SERVER "PRESENCES" — divergences and honest residue

> Companion to `docs/ORGANISM-INSIDE-PRD.md` (the arc) and
> `docs/voice/ASKGOD-VERDICT-P1.md` (the binding verdict). The server lane of P1
> landed all four binding changes; nothing was forced past the grammar. Recorded
> here verbatim instead of silently absorbed — the honest residue is the gold.

1. **The beat rides `track_agent` WITHOUT changing its signature.** The verdict
   put the beat "inside `track_agent`". `track_agent(agent_id)` has ~22 call
   sites; threading the tool name + a mutation flag through all of them would be
   churn and risk. Reuse-first resolution: the beat stays inside `track_agent`
   (the single choke point all four seams funnel through), and the OBSERVED
   mutation level is stamped by a SEPARATE one-line call `note_mutation_observed`
   in `dispatch_tool`, off the same `read_only_denied` classifier the verdict
   named. Same seam, same classifier, no 22-site signature change. The mutation
   lands on the next beat within one throttle cycle (a changed signal clears the
   throttle so a collision surfaces promptly).

2. **Budget re-pin: a NEW mechanical battery replaces the manual pins.** The
   pre-P1 cockpit budget (~695 root / ~430 drill) was a doc-recorded MANUAL
   measurement — no battery test existed in the tree (`north_packet_within_budget`
   is referenced in docs but is not a live test). P1 lands
   `cockpit_budget_holds_with_the_eighth_slot`: a real test measuring the
   worst-case loud root + the presences drill via the repo's own
   `estimate_tokens_from_chars` (chars/4), pretty-print basis (the conservative,
   larger number). Measured: root ~574, presences-drill ~567 — both under the
   ≤800 ceiling, now MECHANICALLY enforced, not hand-kept. The presences drill is
   the new largest drill; `PRESENCE_DRILL_CAP` (6) sizes it to fit.

3. **The historical PATHOS "seven" mentions were LEFT INTACT.** The verdict said
   "update every doc that says 'seven stable slots'". The living surfaces
   (`uml/cockpit.md`, `uml/spine-north.md`, `UML-ORGANISM.md`, the code comments,
   the tool description) now say EIGHT. But three "seven"/"~695/~430" mentions in
   `docs/PATHOS.md` sit inside DATED checkpoint-18 (2026-07-12) era-log blocks —
   they record what the cockpit was WHEN IT SHIPPED that day. Rewriting them would
   falsify the era log (the heading says "dated blocks below are the era log").
   The new 2026-07-13 P1 era block records the evolution to eight + the re-pin.
   Current-state truth is eight everywhere; history stays honest.

4. **The LIVE two-session collision gate is a burst-closing, cross-lane step —
   deliberately NOT run in this lane.** The served owner at `:1338` runs the
   pre-P1 binary and other executors are working against it; rebuilding +
   restarting shared infra mid-burst was declined. The PRD P1 gate ("two real
   mutating sessions visible; an arranged collision surfaces on both north packets
   AND the Hall") also needs the Hall (m1nd-ui lane, gate-material, separate).
   The logic is proven MECHANICALLY here (registration by traffic, TTL expiry, GC,
   throttle, fail-open, collision positive + the isolated-worktrees anti-test, the
   honest_gap derived for both agents, the cockpit 8th slot + budget). The live
   gate is the orchestrator's close, once `:1338` carries this binary and the Hall
   lands. Recorded in the mission card's gaps.

5. **`task_ref` is best-effort MEASURED, honest-absent otherwise.** It is read
   only from an OPEN (`status:"active"`) mission-control card under the runtime
   root (`mission_handlers::latest_open_mission_for`), on the throttled beat. No
   open card → no `task_ref` (never fabricated). This honors "measured from the
   charter, never free declaration" while staying total and cheap.

6. **Presences are exposed on `/api/health`, owner-wide.** The verdict named the
   cockpit slot + the north gap; the Hall (m1nd-ui lane) needs a server data
   source. Rather than a new REST route, the roster + collisions ride the existing
   `/api/health` (beside `agent_sessions`), owner-wide (collisions are same-brain
   by construction, so no cross-brain pairing leaks). The UI lane consumes it; no
   m1nd-ui/ file was touched.
