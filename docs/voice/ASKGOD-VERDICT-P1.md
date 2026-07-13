# askGOD verdict — P1 Presences + G1 (the burst) — 2026-07-13

VERDICT: APPROVE · CONFIDENCE 0.78 · required changes are BINDING.

## The architecture ruled (question by question)

(a) **Hybrid, not sidecar-pure nor projection-pure.** The BEAT is projection: a throttled
hook inside `track_agent` (session.rs:2235 — where `mark_heartbeat` already piggybacks on
traffic; all four seams pass through it). The STORE is a sidecar in the instance-registry
pattern (files in the registry dir, TTL, `is_stale` filtered at read, GC at boot —
instance_registry.rs:284-297,354,446,477) because in-memory projection dies with LRU brain
eviction (project_brains.rs:36-44) and /api/health only sees the root session
(http_server.rs:895-909). Enrichment ("where/on what") comes from OPTIONAL fields on
`session_handshake` + the agent's own `mission_start` charter (task/repo measured from the
charter, never free declaration). Zero new verbs, zero new daemons. All wrapped in
`vigil_fail_open`.

(b) **"Alive" = last_seen-by-traffic within TTL** (minutes scale), AGE always rendered.
Expired = absent at read + GC'd. Disk beat THROTTLED. No invented heartbeats.

(c) **"Mutant" in two honest levels:** OBSERVED — the session dispatched a verb classified
mutating by `read_only_denied()` (server.rs:4518), timestamped; DECLARED — handshake
intent, rendered as declared cloth. m1nd does not see git; nothing more is claimable.

(d) **Collision derived at read, never materialized.**

## Binding changes

1. Presence sidecar as above; hook in track_agent; enrichment via handshake+charter.
2. **Collision predicate:** same brain AND (same caller_root/worktree OR declared
   working-set overlap) AND both with mutation signal. Same-brain alone NEVER warns —
   3 executors in isolated worktrees on the m1nd brain is the NORMAL burst shape
   (AGENTS.md:111-117). The 2026-07-06 incident was two hands in ONE worktree —
   caller_root equality is the measurable signal.
3. **Surfaces cut:** cockpit gains the 8th collection (ONE line at root + capped drill;
   RE-PIN both budgets — ~105 tokens of root slack fit a line, not a list; update every
   doc that says "seven stable slots"). Hall strip (m1nd-ui) is IN as its own lane and is
   GATE-MATERIAL (the P1 gate requires it). h4nd tray is OUT — queued slice in the h4nd
   house. north: NO new schema field — the collision line rides the existing honest-gaps
   mechanism, present only when a collision exists (P1 gate requires it on both sessions'
   packets, PRD:466-468).
4. **G1:** the daemon tick moves INTO `dispatch_tool` mirroring the auto-ingest vigil
   (server.rs:4609-4630 precedent), condition `active && !tick_in_flight &&
   should_autotick_daemon(tool) && due` INLINE (nested internal dispatches would inflate
   pending_rerun otherwise); DELETE the pre-dispatch block in handle_mcp_method
   (server.rs:5518-5525) same PR — one seam only. This fixes THREE deaf seams at once
   (REST, stdio side-loop, mcp_http — the oracle verified all three lack ticks).
   `track_agent` stays per-seam. Tests: new RED-first driving dispatch_tool directly;
   keep the skip-list regression pinned to the REAL list (server.rs:5571-5590 —
   daemon_*/alerts_*/session_handshake/trust_selftest/recovery_playbook/mission_* — the
   dossier wrongly said recall verbs are in it; they are NOT). Measure tick wall-clock
   inside the REST 30s window in the PR.
5. **Burst shape:** each executor opens/events/closes its OWN charter under its OWN
   agent_id (ensure_agent respected by construction) — otherwise P1 has no three
   presences to show and the gate starves. The orchestrator keeps an umbrella charter
   referencing the child msn_ ids. EVERY REST call in the burst passes `?brain=`
   explicitly (the post-restart mismatch makes implicit binding untrustworthy today).
6. **The REST-without-brain field report** = separate INVESTIGATION mission (may be
   doctrine-that-was-missing, not a bug: the owner may be medulla-stamped at boot,
   http_server.rs:574). Confirm before verdicting. Not folded into G1.

## Risks flagged

spawn_blocking holds the brain lock past the REST 30s timeout (measure; maybe skip inline
tick when the entry tool is already heavy) · the inverse TTL lie (a live executor
compiling 20min vanishes from the roster — write the limitation into the surfaces:
"presence = activity visible to m1nd") · cockpit presence scope must be decided and
LABELED (owner-wide vs served-brain roster) before coding the slot · free-text
theme/working_set never in commits (no-leak; neutral fixtures) · sessions HashMap grows
without GC (sidecar must not depend on it to expire) · boot-GC must sweep stale sidecar
files after owner restart · the P1 gate requires the Hall — the UI lane is gate-material.
