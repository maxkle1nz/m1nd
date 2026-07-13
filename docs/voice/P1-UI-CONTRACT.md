# P1-UI-CONTRACT — the presence endpoint the Hall strip reads

**Lane:** P1-UI (the Hall presence strip, `m1nd-ui`). **Status:** built against this
MOCKED contract (the house pattern — the UI suite mocks the API; real wiring is the
burst close). **Authority:** `ASKGOD-VERDICT-P1.md` (binding changes 1–3) +
`ORGANISM-INSIDE-PRD.md` §P1. This file DECLARES the shape the P1-UI expects so the
P1-server lane can serve it verbatim. If the server must diverge, record it in
`P1-UI-DIVERGENCES.md` and reconcile at the burst close.

The wire types live in `m1nd-ui/src/types.ts` (`PresenceEntry`, `PresenceMutation`,
`PresenceCollision`, `PresenceResponse`); the predicate + age/liveness logic live in
`m1nd-ui/src/lib/presence.ts`. Those two files are the machine-readable half of this
contract.

---

## The endpoint

```
GET /api/presences?brain=<project_root>
```

- **`brain` ABSENT ⇒ the OWNER-WIDE roster** — every agent visible to this owner
  across all its brains. This is the scope the **Hall strip** uses (the control-room
  view: the owner sees the whole team). *(This resolves the verdict's flagged
  "cockpit presence scope must be decided and LABELED" for the Hall surface: the Hall
  is owner-wide.)*
- **`brain` PRESENT ⇒ scoped to that brain's roster.** Reserved for a per-brain view;
  the Hall does not use it today.
- Pure **READ** — safe under a read-only attach. `?brain=` is the standard §4A.9
  selector (URL-encoded absolute root), same as every other REST read.
- A **pre-P1 owner has no route** (404). The client treats 404/absence as an **empty
  roster**, never an error wall (vigil-fail-open) — see *Degradation* below.

## The response envelope

```jsonc
{
  "presences": [ PresenceEntry, ... ],   // required (may be [])
  "collisions": [ PresenceCollision ],   // OPTIONAL — server-derived at read
  "served_brain": { "project_root": "...", "display_name": "..." }  // optional §4A.9.4 echo
}
```

### `PresenceEntry`

| field | type | meaning |
|---|---|---|
| `agent_id` | `string` | the agent's own id — its `agent_id` on every m1nd call / its charter. **WHO.** |
| `root` | `string` | the brain/root this agent is bound to (the served brain root). **WHERE.** |
| `caller_root` | `string \| null?` | the agent's resolved `caller_root`/worktree when it differs from `root`. The **measurable collision signal** (binding change 2). Absent/null when it equals the root. |
| `first_seen_ms` | `number` | first time m1nd saw this agent (ms epoch). |
| `last_seen_ms` | `number` | last time m1nd saw this agent (ms epoch). **SINCE WHEN** — the age source, always rendered. |
| `query_count` | `number` | how many verbs m1nd has seen from this agent this presence. |
| `mutation` | `PresenceMutation` | the mutation signal, two honest levels (below). |
| `task_ref` | `string \| null?` | the task line measured from the agent's OWN `mission_start` charter (never a free declaration). **ON WHAT.** Absent when the agent opened no letter. |

### `PresenceMutation` (verdict binding change 1c — two honest levels)

| field | type | meaning |
|---|---|---|
| `observed_at_ms` | `number \| null?` | the agent dispatched a verb classified mutating (`read_only_denied`), timestamped. The **strong** signal (rendered as a filled dot). |
| `declared_intent` | `string \| null?` | a handshake-declared intent — **declared cloth** (rendered as a hollow ring; weaker: a claim, not an observation). |

Both absent ⇒ a quiet read-only presence (no dot). *m1nd does not see git; nothing
beyond these two levels is claimable.*

### `PresenceCollision` (verdict binding changes 1d + 2 — DERIVED AT READ, never materialized)

| field | type | meaning |
|---|---|---|
| `brain_root` | `string` | the shared brain root. |
| `caller_root` | `string \| null?` | the shared worktree when THAT is the trigger (the strong arm). |
| `agent_ids` | `string[]` | the colliding agents (≥2 distinct ids). |
| `reason` | `'same_worktree' \| 'declared_overlap'` | which arm of the predicate fired. |

**The collision predicate (the law the server must implement):**

> same brain **AND** (same `caller_root`/worktree **OR** declared working-set overlap)
> **AND** BOTH agents carry a mutation signal.

- **Same-brain-alone NEVER warns.** N executors in ISOLATED worktrees on one brain is
  the normal burst shape (`AGENTS.md`), not a collision.
- The **measurable arm** is `caller_root` equality — two hands in ONE worktree is the
  2026-07-06 incident shape. The client can derive this arm itself
  (`deriveCollisions` in `lib/presence.ts`, unit-tested) and DOES so as a fallback.
- The **`declared_overlap`** arm needs a declared working set the UI does not invent —
  it is **server-derived only**; the strip renders it when the server sends it.

## Collision authority + degradation

- **`collisions` present (even `[]`) ⇒ the server is authoritative.** The client
  renders exactly what the server derived and does NOT re-derive.
- **`collisions` absent ⇒ the client degrades** to `deriveCollisions(presences)` (the
  measurable `same_worktree` arm only). This is the pre-P1-owner fallback; the
  honest-degradation pattern used across the Hall (`served_brain`, the REST selector).
- **404 / network miss ⇒ empty roster**, no error wall. A genuine non-404 error is
  surfaced honestly in the strip but never breaks the Hall.

## Honesty invariants the server must uphold

1. **TTL-filtered at read** — the roster only contains presences alive-within-TTL
   (minutes scale, verdict 1b). **Expired presences are ABSENT** (GC'd at read and at
   boot). The strip renders NO ghost. *(Metric: "Ghost presences → TTL expiry
   proven".)*
2. **No invented heartbeats** — `last_seen_ms` is a real sighting; the beat is a
   throttled projection off real traffic, never a synthetic ping.
3. **The age is the truth, not "online"** — the strip renders the age from
   `last_seen_ms` always; there is no binary online/offline. The UI writes the
   limitation on its own surface ("presence = activity visible to m1nd — a busy agent
   that hasn't called in stays invisible"); the server need not.
4. **No free-text leak** — `task_ref` comes from the charter; `declared_intent` is the
   handshake intent. Neither should carry secrets, and neither ever lands in a commit
   (no-leak law: the fixtures here use neutral names only).

## What the P1-UI renders from this (for the server's mental model)

- A **strip** at the top of the Hall: `Working now · N`, the limitation caption, an
  amber collision notice per `PresenceCollision` (calm, inline — never a modal or a
  block), then a chip per `PresenceEntry` (who · where · age · mutation dot · task).
- Empty roster ⇒ "No agents visible to m1nd right now."

## Open alignment points for the burst close

- The envelope is `{ presences, collisions?, served_brain? }`. If the server prefers a
  **bare array** `PresenceEntry[]` + a separate collisions channel, note it in
  `P1-UI-DIVERGENCES.md`; the client's `resolveCollisions` already tolerates an absent
  `collisions` field, but a bare top-level array would need a one-line client adapter.
- Confirm the **owner-wide** semantics of an absent `brain` (the Hall depends on it).
- Confirm `caller_root` is the agent's real resolved worktree root (so the measurable
  collision arm is trustworthy).
