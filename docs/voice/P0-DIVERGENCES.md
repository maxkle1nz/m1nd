# ORGANISM-INSIDE P0 "WEAR THE WIRE" — divergences and honest residue

> Companion to `docs/ORGANISM-INSIDE-PRD.md` (the arc) and the P0 doctrine landed
> this PR on the agent surfaces. First-time friction wearing the wire is the GOLD
> of P0 — recorded here verbatim instead of silently absorbed. Nothing below was
> forced past the grammar; where the grammar taught, the doctrine follows the
> grammar, not the spec's shorthand.

1. **The progress verb is `mission_event`, NOT `mission_post` (the spec's word).**
   The P0 spec named the executor/progress verb `mission_post`. The REAL grammar
   for the burst card the orchestrator opens with `mission_start` — a
   MISSION-CONTROL mission (`msn_…`, `m1nd-mission-control-state-v1`,
   `m1nd-mcp/src/mission_handlers.rs`) — records progress with `mission_event`
   and closes with `mission_close`. `mission_post` is a DIFFERENT rail entirely:
   the mission-LETTER board (`m1nd-mission-letter-v0`, phases
   `judging|executing|gate|review|merge_wait|landed|failed`, dispatch
   `server.rs:4815`), whose `landed` is a human `receipt_import`. The two share
   the word "carta" but not the verb, the id space, the schema, or the store. The
   doctrine (M1ND_INSTRUCTIONS §4 + the three skills) and the P0 dogfood both use
   `mission_event`/`mission_close` — the real grammar — and cross-reference the
   letter board's human-only landing law so the reader never blurs them.

2. **Two mission systems, one word — the board the confession measures is not the
   board P0 dogfoods.** The PRD's "mission board" (the one that saw NONE of the
   8-PR day, read via `GET /api/mailbox?kind=mission`) is the mission-LETTER
   board. The P0 dogfood card, opened by the orchestrator via
   `POST /api/tools/mission_start`, is a mission-CONTROL mission persisted at
   `<runtime_root>/mission-control/<msn_id>.json` — verified live: the card
   `msn_1783893555531_claudeguardianp0we` is NOT present on
   `/api/mailbox?kind=mission` (the letter board returned its `missions` key with
   the P0 id absent). So P0 makes the ORCHESTRATION burst visible on the
   mission-control rail; fully curing the confession's specific board metric
   (letters per executor mission) is a mission-LETTER concern the later phases
   (P2's charter → `mission_post` chain) own. The doctrine names the
   mission-control card as the burst's TRAIL and points at the letter board for
   the map-coloring landing — the honest division, not a merge.

3. **A mission-control card is SINGLE-AGENT (`ensure_agent`) — "executors post to
   the orchestrator's card" cannot be literal cross-agent posting.**
   `handle_mission_event` and `handle_mission_close` both call
   `ensure_agent(&mission, &agent_id)` (`mission_handlers.rs:556`), which refuses
   any caller whose `agent_id` ≠ the card's own with the teaching message
   `mission <id> belongs to agent_id <A>; got <B>`. So the P0 rule "executors post
   progress on the orchestrator's card" is encoded honestly in the doctrine as:
   the burst posts UNDER THE ORCHESTRATOR'S id — executors report back and the
   orchestrator posts — rather than each executor writing to a card it does not
   own. This is a real design property (the mission-control card is single-writer
   by construction), not a bug; recording it so the next wire-wearer expects it.

## Dogfood record — closing this very card (`msn_1783893555531_claudeguardianp0we`)

P0 ate its own dog food: this mission was closed BY the wire it documents, over the
REST loopback against the live served owner (`:1338`), as a SEAT HAND (agent_id
`claude-guardian` — the card's own).

- **`POST /api/tools/mission_event`** → accepted, verbatim return:
  `schema: m1nd-mission-event-v1`, `event_id: evt_1`, `evidence_class: "direct"`,
  `event_count: 1`, `budget_consumed: 0.0625` — the honest outcome (docs versioned,
  PATHOS amendment, doctrine on 4 surfaces, gates green) is now recorded on the card.
- **`POST /api/tools/mission_close`** → accepted, verbatim return:
  `schema: m1nd-mission-proof-packet-v1`, the card is now `phase: "closed"`,
  `status: "closed"`, `event_count: 1`, `handoff_count: 0`,
  `next_action: "No verified claims to persist…"` (this docs mission recorded no
  `mission_verify` claims — its proof is the gates + the commits, not mission-control
  claims).

**The finding worth keeping: a mission-control close IS a seat-hand gesture — no owner
gate.** `mission_close` refused NOTHING; its only door is `ensure_agent` (the caller's
`agent_id` must equal the card's), which the guardian seat satisfies. This is the honest
asymmetry the doctrine now teaches: closing a mission-control TRAIL is a seat gesture,
while LANDING a receipt that colors the map stays human-only (`receipt_import`'s
human-origin gate, #353). Both laws held in one dogfood — the trail closed by the hand,
the map untouched. No grammar refusal occurred; the only divergences were the three
spec-vs-grammar items above, recorded, never forced.
