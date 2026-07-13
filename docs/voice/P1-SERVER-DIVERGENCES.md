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

6. **The Hall's data source is `GET /api/presences` (the UI contract), not
   `/api/health`.** This lane FIRST exposed the roster on `/api/health` (beside
   `agent_sessions`); the burst close then surfaced the P1-UI lane's declared
   contract (`m1nd-ui docs/voice/P1-UI-CONTRACT.md`): a dedicated
   `GET /api/presences?brain=` returning `{presences, collisions?, served_brain?}`.
   The endpoint now exists and honors that contract: absent `brain` = OWNER-WIDE
   (the Hall's declared scope), present = that brain's roster (filtered by the
   RESOLVED session's own `workspace_root` — the exact key its beats write) + the
   §4A.9.4 `served_brain` echo, unknown root = the house 404 (the client degrades
   to an empty roster), `collisions` ALWAYS present (server-authoritative, the
   client never re-derives). The `/api/health` block REMAINS as an owner-wide
   diagnostic beside `agent_sessions` — same pure functions, one truth.

7. **Contract fields the server serves differently (all honest, none fabricated):**
   - `task_ref` — the UI contract calls it "the task line"; the server sends the
     charter's **`msn_` id** (the verdict's binding shape: "task_ref?: msn_ of
     the agent's charter"). The id is what is honestly measured from the mission
     card and is leak-safe (free task text may carry personal context; the
     no-leak law applies). The UI renders the id or adapts at the burst close.
   - `PresenceCollision` is emitted **per colliding pair** (`agent_ids` always
     2); three hands in one worktree arrive as pairs, not one merged triple —
     same information, no invented grouping.
   - On the `same_worktree` arm, `caller_root` carries the shared **measured
     caller_root path when that matched**, else the shared **declared worktree
     string** (both are the measurable arm; the value is what matched, never
     invented). A pure working-set overlap (`declared_overlap`) claims no
     location, exactly as the contract states.
   - The endpoint's owner-wide response carries **no `served_brain` echo** — the
     contract marks the field optional, and echoing the BOUND brain over an
     owner-wide roster would mislabel the scope (INV-15 posture).
