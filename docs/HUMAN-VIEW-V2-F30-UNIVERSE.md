# HUMAN VIEW v2 — F30 technical amendment: the Universe landing surface, `/api/universe`, and the Landing

Status: **authored post-verdict** (askGOD CHANGE, alta, 2026-07-14 — all six required
changes applied in the authored amendment; pattern F2.5e: verdict + amendment in the
same burst). Author seat: Fable (owner's standing order for vision/PRD authorship).
This file is the buildable technical form of that amendment — it stitches the entry
door (§4A.1 of the ratified placement doctrine, HUMAN-LAYER-PRD) to one new read-only
aggregate endpoint and one new SPA surface. Nothing else in the ratified contract moves.

Family: sits beside `HUMAN-VIEW-V2-F25-TECH.md` (the mission tray + runner daemon),
`HUMAN-VIEW-V2-F12-TECH.md`, `HUMAN-VIEW-V2-F0C-TECH.md`. Reuses P1 presences
(`ASKGOD-VERDICT-P1.md`), the mission-letter board (F2.5 §1), and the SystemBlock store
(F0c) — it INVENTS no new state and no new write verb.

---

## What changes and why

The owner asked for a real Home — "an operating-system panel, not a dashboard; a
panorama of projects; ONE place to manage every receipt scattered here and there."
This amendment changes the SPA's entry door and adds one read-only aggregate endpoint.

## 1 · The Universe (new landing surface)

- New `Surface` variant `universe` in the SPA state machine (`m1nd-ui/src/App.tsx`).
  **Navigation substrate for v1 is state-zoom** — the existing `useState` Surface
  mechanism. A URL router (deep links, a real back stack) is its OWN future slice; v1
  makes no routing promises. E2E language says "universe surface renders", never
  "route /".
- Entry rule (amends §4A.1): when the owner serves **≥1 project brain**, the SPA lands
  on `universe`. With **ZERO** project brains the first-run Threshold (onboarding,
  HUMAN-LAYER-PRD §4A.2, INV-11/INV-12) behaves exactly as today — untouched, including
  the Build-Map front door for a bound-brain skeleton. The served brain's map remains
  one click away (world → map room).
- Each world is rendered from REAL measures only (see §3). Every visual property is a
  measurement: size ∝ `node_count` (log scale); light ∝ freshness of `updated_ms`
  (stale light is DATA, shown honestly WITH its age — the manifest may lag the live
  store and the UI says so); orbiting satellites = live presences; an amber dashed ring
  = pending human gestures for that world.
- The L0 header is a client-composed serif sentence stating UNIVERSE FACTS (world count,
  awake count, pending-gesture total). **No cross-brain pulse and no cross-brain vitals
  aggregation** — the pulse signature stays per-brain by ratified law (`human_view.rs`:
  "PER-BRAIN — never a cross-brain total"). Per-world cards may carry their own
  per-brain mini-pulse. The universe FACTS are counts of worlds/awake/pending — never
  a summed vital.
- The Hall remains the presence ROOM (per-brain/owner detail). At L0 the Universe owns
  the roster panorama; the Hall is where you walk in.
- v1 renders EXISTING project brains only. "Unlit worlds" (known repos without a brain,
  click-to-ingest with the overlap-guard refusals surfaced verbatim) are v1.1, sourced
  from the instance registry's seen roots — not invented here.

## 2 · The Landing (unified human-gesture queue)

- Name: **the Landing** — continuous with the existing product language (`landing_bell`,
  "missions await the human landing"). The first-run Threshold keeps its name and
  semantics; the collision the oracle flagged is resolved by NOT reusing that word.
- One list, every world: merge_wait receipt stamps, block-candidate ratifies, owner
  alert acks. Each item carries a world chip (alerts carry an `owner`-scope chip instead
  — daemon alerts live on the owner, not a brain), a one-line human explanation, and a
  gesture affordance.
- **Reads are aggregated; writes stay per-type.** Clicking an item NAVIGATES to the
  existing flow (tray card, map ratify) — no inline confirms, no new write verbs, origin
  gates untouched (`RATIFY_HUMAN_ORIGIN`, `RECEIPT_IMPORT_HUMAN_ORIGINS` stay the only
  doors, `system_blocks_handlers.rs`). Batch "ratify all" remains the h4nd G slice, out
  of scope.
- **Where each item lands (client; "honest doors" burst, 2026-07-14).** A world stamp/ratify
  opens that world's room (its map/tray). The **owner alert** item opens the **Hall and lands
  on its owner-alerts panel** — a drawer the Hall now carries, listing the bound session's
  unacked `daemon_alerts` with a per-alert acknowledge + an "acknowledge all" (behind a simple
  confirm). Before this, the owner item opened a Hall that showed the alerts *nowhere*; now it
  has a real destination. Ack calls `alerts_ack` on the **bound session (no `?brain=`)** — the
  same stock this endpoint's `owner.alerts_pending` counts (a `?brain=` selector would ack a
  project brain's own alerts, a different stock). Cross-brain unification of alerts stays out
  of scope. A world item that somehow carries **no root** renders **disabled** ("world root
  unknown — refresh") rather than silently routing to the Hall (the wrong room).
- Naming law for counters: the cockpit `bell` keeps its exact semantics (merge_wait
  heads only). The Landing badge counts all queued gesture types and is labelled
  "await your hand" — it is **never** called a bell in UI copy. Two numbers, two names,
  no visual lie.

## 3 · `GET /api/universe` (read-only aggregate)

Server-side, **sidecar-only**. Every source is read from disk (or an already-resident
in-process value) WITHOUT hydrating any project brain — the routing layer's
`resolve`/`bootstrap` is never called.

Sources:
- `project_brain.json` manifests via `ProjectBrainRegistry::disk_roster()` (node/edge
  counts + `updated_ms` — the consecrated cheap source; parsing graph snapshots on a
  list call stays banned);
- the P1 presences sidecar dir via `presence::list_live` (owner-wide roster, grouped
  per world by its `brain` root — a cross-brain read of the dir, a small declared
  contract extension);
- the mission-letter board `<project_root>/.m1nd/inbox.jsonl` via `mailbox::read_letters`
  → `mission_letter::heads_by_mission` for merge_wait heads;
- the SystemBlock store `<store_dir>/system_blocks.json` via `SystemBlockStore::load`
  for candidate blocks awaiting ratification;
- the OWNER's own `SessionState::daemon_alerts` (owner-scope) for the alert acks — read
  from the already-resident bound session, never a project brain.

**HARD LAW, executable:** serving `/api/universe` never inserts into
`ProjectBrainRegistry.brains`. Proven by a RED-first test (`universe_endpoint.rs`): the
warm map is byte-identical before/after (`warm_len()` unchanged, the served brain
`warm_counts().is_none()`), and the fixture includes an EVICTED/dormant brain served
purely from its manifest.

Freshness fields come only from persisted measures: manifest `updated_ms`, presence
`last_beat_ms`, letter head phase. Nothing invented; the UI shows the age, not a
fabricated "live" state.

### Wire shape (`m1nd-universe-v0`)

```json
{
  "schema": "m1nd-universe-v0",
  "worlds": [
    {
      "key": "<canonical project root>",
      "root": "<project root the brain maps>",
      "name": "<repo basename>",
      "node_count": 1234,          // omitted if the manifest has no count
      "edge_count": 5678,          // omitted if the manifest has no count
      "updated_ms": 1784000000000, // omitted if the manifest records neither updated nor created
      "awake": true,               // ≥1 live presence on this world
      "presences": [ /* P1 wire_entry shape, this world only */ ],
      "pending": { "stamps": 2, "ratifies": 5 },
      "letters": { "merge_wait": 2, "total": 7 }
    }
  ],
  "owner": { "alerts_pending": 1 },
  "totals": { "worlds": 3, "awake": 1, "pending": 8 }
}
```

- `pending.stamps` = merge_wait head count (the receipts awaiting the human's hand — to
  land, or to archive as superseded, both from the SAME tray card).
- `pending.ratifies` = SystemBlock candidate count (state `candidate`).
- `totals.pending` = Σ per-world (stamps + ratifies) + `owner.alerts_pending`.

### What the shape does NOT carry, and why (honesty over a fabricated field)

- **`pending.archives` is omitted.** Archival is not a distinct persisted queue: the
  `archived` mission phase (F2.5e) is an ALTERNATIVE terminal gesture on a *merge_wait*
  receipt — the same head already counted under `stamps`. There is no cheap sidecar
  signal for "receipts pending archival" (it is a human judgment, "set this superseded
  one aside"). Rather than fabricate a count, the field is omitted; the archive gesture
  still reaches the human from the tray card a stamp item navigates to. A dedicated
  archive queue, if ever wanted, is a future slice.
- **Per-brain daemon alerts are omitted.** The daemon's alerts live on a brain's own
  `SessionState`; reading a project brain's would require hydrating it (banned). Only the
  OWNER's alerts (the bound session, already resident) are surfaced, owner-scope — the
  `owner` chip, never a world chip.

### Client read resilience (`useUniverse`; honest doors, 2026-07-14)

The panorama poll degrades HONESTLY, and the three cases are now distinct (they used to be
one — any failure became a ready, empty sky, silencing real errors):
- a **404** (a pre-F30 owner) stays a settled degrade to the empty-ready sky — the entry rule
  falls through to the prior doctrine untouched;
- a **real (non-404) blip after a good read** keeps the last-good sky lit and flies a discreet
  "read failed — retrying" note (a transient error never blanks a populated Universe);
- the **first non-404 failure** is an honest `error` state that does NOT decide the landing —
  the SPA waits and retries rather than reporting "zero worlds" and landing on tree/Threshold.
  A genuinely down owner is already gated to `loading` elsewhere, so this never hangs in
  practice (the owner's `/api/universe` fail-opens to 200, so a persistent non-404 is
  essentially impossible while health is `ok`).

## Out of scope (declared)

Atrium (L1 per-world home) = v2. Unlit worlds + ingest-click = v1.1. URL router = its
own slice. Batch ratify = h4nd G. Aggregate pulse = would need its own law; not proposed.
A separate `archives` queue and per-brain alerts = future, per the honesty notes above.
Cross-brain unification of daemon alerts (surfacing project brains' alerts in one owner panel)
stays out of scope — the owner-alerts panel is the bound session's stock only.

## Validation

RED→GREEN on the anti-hydration law (`m1nd-mcp/tests/universe_endpoint.rs`); vitest on
the new components; Playwright e2e against a fully-mocked owner with fixture brains
(touching the live owner at :1338 / `~/.m1nd` from tests is prohibited by AGENTS.md — the
e2e mocks every `/api/*` route in-page). Final proof is the owner driving the real flow
in a browser (the orchestrator's step, not this burst's).
