# presences — the control room sees the team (`m1nd-presence-v0`)

ORGANISM-INSIDE **P1** (askGOD verdict 2026-07-13 APPROVE, binding changes 1–3 —
`docs/voice/ASKGOD-VERDICT-P1.md`; PRD §3.3 `docs/ORGANISM-INSIDE-PRD.md`). The
organism's agents were invisible to the organism itself; P1 makes a live SESSION
a visible presence — who is working, on what, since when — and derives a
COLLISION warning before two mutating hands cost hours of recovery. Home:
`m1nd-mcp/src/presence.rs` (the module), the beat in `session.rs::track_agent`,
the observed-mutation stamp in `server.rs::dispatch_tool`, the 8th cockpit slot
in `cockpit.rs`, and the north collision gap in `server.rs::handle_north`.

## Shape

A durable sidecar `<registry_root>/presences/<prs_id>.json`, molded on
`instance_registry`. `prs_<12hex>` is stable per `(agent_id, brain)` — the beat
UPSERTS one file, never N.

```
PresenceRecord (m1nd-presence-v0)
  presence_id  prs_<12hex> = stable_presence_id(agent_id, brain)
  agent_id · brain (= workspace_root, the collision key) · caller_root?
  kind? · theme? · worktree? · working_set[]     — DECLARED (session_handshake)
  task_ref?                                        — MEASURED (agent's mission_start charter)
  mutation { observed_at_ms?, declared_intent? }   — the two honest levels (verdict c)
  first_seen_ms · last_beat_ms · query_count · ttl_ms
```

## The beat (projection) + the store (sidecar) — a hybrid

- **Beat = projection.** A THROTTLED hook inside `track_agent` (the single choke
  point all four dispatch seams funnel through — REST, stdio, mcp_http, stdio
  side-loop). At most one disk write per `PRESENCE_BEAT_THROTTLE_MS` (5s) per
  session; a changed signal (a declaration or an observed mutation) forces the
  next beat so a collision surfaces promptly. Wrapped in `vigil_fail_open` — a
  broken sidecar write can NEVER break a tool call.
- **Store = sidecar.** In-memory projection dies with LRU brain eviction and
  `/api/health` only sees the root session; the sidecar survives. `is_stale`
  (last beat older than `PRESENCE_TTL_MS`, 2 min) filters at READ, so a dead
  presence DISAPPEARS rather than lying; `gc_stale` reclaims orphan files at boot
  (`session.rs`, beside `spawn_boot_gc`) after an owner restart.
- **Enrichment.** `brain`/`caller_root` are the session's own binding (never
  claimed); `task_ref` is measured from the agent's own open `mission_start`
  charter (`latest_open_mission_for`); the DECLARED fields ride NEW optional
  `session_handshake` fields; the OBSERVED mutation level is stamped in
  `dispatch_tool` off the single `read_only_denied` classifier.

## Collision — DERIVED at read, never stored

The verdict's exact predicate (`presence::collision_between`): **SAME brain AND
(same caller_root/worktree OR declared working-set overlap) AND BOTH with a
mutation signal.** Same-brain alone NEVER warns — three executors in isolated
worktrees on one brain is the NORMAL burst shape (the 2026-07-06 incident was two
hands in ONE worktree; caller_root equality is the measurable signal). Two views:
`collisions_for_agent` (per session, for north's per-agent gap) and
`collisions_in` (every pair, for the cockpit + `/api/health`).

## Surfaces

| Surface | What | Where |
|---|---|---|
| cockpit slot 8 `presences` | ONE root line (collision warning rides the LABEL, no new schema field) + a capped in-place drill (`PRESENCE_DRILL_CAP` = 6 rows: agent · theme · where · age · mutating), scope labeled "this brain" | `cockpit.rs` |
| north collision gap | ONE line on the EXISTING `honest_gaps` mechanism, present only on a real collision, derived per-agent so it lands on BOTH colliding sessions' packets | `server.rs::handle_north` |
| `/api/health` | owner-wide roster + collisions (data source for the Hall strip, m1nd-ui lane) | `http_server.rs::handle_health` |

Budget re-pinned + MECHANICALLY enforced by `cockpit_budget_holds_with_the_eighth_slot`
(`chars/4`, worst-case): cockpit root ~574 tokens, presences-drill ~567, both ≤800.

## Laws

- **Witness tissue.** A presence verifies nothing, gates nothing, lands nothing
  (PRD laws 5, 10). It is advisory telemetry — the organism warns, the human /
  orchestrator decides (the same posture as reception).
- **Measured facts only** (G1). Counts come from the registry; nothing is
  invented. The roster never renders a presence the registry did not serve
  (INV-10 applied to sessions).
- **Fail-open for voice.** Every read surface drops the roster whole on an
  unreadable registry — north/cockpit/health never break over presence.

## The written limitation (the inverse-TTL lie)

**presence == activity VISIBLE TO m1nd.** An executor compiling for 20 minutes
makes no m1nd calls, so its beat lapses and it expires from the roster — a live
agent can read as absent. The roster answers *"who is talking to m1nd, on what,
since when"*, never *"who is alive"*. It never invents a heartbeat.

## Gaps / deferred (honest)

- **The Hall strip is a separate lane** (m1nd-ui) and is P1-gate-material; the
  server exposes the data (`/api/health`), the UI renders it.
- **task_ref is best-effort** — measured only from an OPEN (`status:"active"`)
  mission-control card under the runtime root; absent otherwise (honest).
- **The h4nd tray team-view is OUT** (queued in the h4nd house, per the verdict).
