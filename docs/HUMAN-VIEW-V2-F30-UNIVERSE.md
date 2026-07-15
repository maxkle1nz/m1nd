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
  **Navigation substrate for v1 was state-zoom** — the existing `useState` Surface
  mechanism. The URL router (deep links, a real back stack) that this section deferred
  **has since landed** as a THIN sync over that same Surface machine — see the amendment
  "**§ The hash router**" below. The state machine is unchanged; the router only mirrors
  it into `window.location.hash` (zero server change — the SPA is rust-embed).
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
  "owner": { "alerts_pending": 1 },   // null + a "note" when the owner is busy (see §3a)
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

### 3a · Vitals never block the panorama (server read resilience, 2026-07-15)

**LAW: vitals never block the panorama.** `/api/universe` is a SIDECAR-ONLY read; it must
never queue behind graph work. Two owner-scope facts (`alerts_pending`, and the registry
root the presence roster reads) live on the bound `SessionState`, behind the session lock —
the SAME lock the gardener tick holds across a re-ingest + `rebuild_engines` for minutes on a
post-kickstart warm. Taking that lock on the read's first line made the WHOLE panorama queue
behind the tick: it read as a deadlock (CPU 0%, 15-20s timeouts, the SPA falling back to the
old doctrine on every boot), but was only transient contention.

So `universe_body` **`try_lock`s** the session for the owner vitals:
- **lock free** → the real values (the registry root + the unacked `alerts_pending` count) —
  byte-identical to the prior behavior;
- **lock busy** → the panorama is served WITHOUT the session-live vitals, an HONEST omission:
  `owner` becomes `{ "alerts_pending": null, "note": "owner busy — vitals omitted" }`,
  `totals.pending` folds the missing count as zero (never inflated), and the presence roster
  degrades to the immutable boot registry hint (`AppState.registry_dir`, captured once at
  boot) or an empty roster — the established fail-open posture. Never a stall, never a
  fabricated zero.

The disk-sourced worlds (manifests, mission boxes, SystemBlock stores) need no session lock,
so the panorama's spine is always served. RED-first proof (`universe_endpoint.rs`
`universe_never_queues_behind_a_held_session_lock`): a background thread holds the session
lock for 2s; the read must still answer 200 within a 500ms ceiling carrying the honest
omission — it took 2.001s on the pre-fix code, well under 500ms after. Client side,
`buildLandingItems` treats the null vital as NO owner chip (an omitted count never coerces
into a gesture).

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

## The hash router — deep links and a real back (amendment, 2026-07-15)

The future slice §1 deferred landed on `feat/url-router` (PR 1 of the Pista-A pair; the
live-map arc is a SEPARATE PR). It is a **hash router** — zero server change (the SPA is
rust-embed from one `index.html`, so a `#` fragment never reaches the server) — and a
THIN URL⇄state sync over the existing Surface machine (`m1nd-ui/src/lib/router.ts`, pure
+ DOM-free). It invents no state; App.tsx routes every transition through one
`navigate()`.

- **One writer (R4).** A single `navigate()` wraps `setSurface` + `setViewedBrain` +
  `setMapTargetBlock` (+ the transient hall-alerts flag) and is the ONLY code that writes
  history — sprinkled `pushState` is banned. All ~10 call-sites (the TopBar wordmark/map,
  world open, tray open-block, hall/owner entry, the landing decider, the ESC ladder,
  `HallView` exit, the palette, `landAndOrient`) and `popstate` flow through it. The
  landing decider writes with `replace` (a baseline entry so the FIRST Back works, no
  spurious push); user navigations `push`; `popstate` syncs state with no write.
- **The route scheme** (derived from the real transitions): `#/universe`, `#/hall`,
  `#/tree`, `#/map` for the bound brain (`?block=sb_x` rides the map), and
  `#/world/<key>/tree` / `#/world/<key>/map?block=sb_x` for a hosted world. `<key>` is the
  world **basename** (below). `#/tree` vs `#/world/<key>/tree` IS the bound-vs-hosted
  `viewedBrain` distinction.
- **Deep-link beats the landing rule (placement precedence).** The hash SEEDS the initial
  surface BEFORE the landing rule runs — the landing gate is `surface == null`
  (App.tsx), so seeding from the hash makes the precedence natural. A bound route seeds at
  once; a world route waits for the worlds/brains reads to settle, then resolves, while the
  landing effect STANDS DOWN (a `deepLinkPending` guard). **No hash → the landing decision
  is byte-identical to before** (the router only adds the `replaceState` baseline).
- **`landAndOrient` is suppressed under a hash.** A deep link does NOT pass through the
  3-beat orientation, and there is no race between bootstrap and the router — `landAndOrient`
  runs only on a fresh bootstrap (the Threshold path), never on a deep-linked load.
- **The brain key is a basename, never the absolute root (R3, no-leak law).** AGENTS.md
  forbids personal paths in the public repo, and the e2e crystallizes URLs into public
  specs. The world's `key`/`root` fields are BOTH the canonical absolute path (they leak);
  `instance_id` is NOT stable either — `generate_instance_id` (instance_registry.rs) hashes
  pid + clock + a seq nonce, ephemeral by construction across restarts. The **basename** is
  stable (a pure function of the path) and non-leaking. Serialize = basename of the viewed
  root; resolve = match the basename against the worlds panorama (its `name` is the server
  basename) then the Hall registry. An **unresolvable key** (brain evicted, or a basename
  collision — two worlds share a basename → ambiguous, never a guess) falls back to the
  normal landing rule; a `popstate` to an evicted world falls back to the universe. It
  **NEVER strands the human in an empty map.**
- **The addressable boundary (half the design): transients stay OUT of the URL.**
  Addressable = durable location only — the surface, which brain it views, and the
  tray-seeded map block (`?block=`). NOT addressable: the ingest modal, the Cmd+K palette,
  the 3-beat orientation, `hallOpenAlerts` (how you ENTERED the Hall — the Hall itself is
  `#/hall`, but the auto-open-alerts sub-state is transient), and the Build Map's own
  ad-hoc card selection (clicking a block card is exploration — only the tray-target
  `mapTargetBlock` is the address; a Back clears `?block=` from the URL while the live
  panel, still closeable via its ✕, is transient — the boundary, not a break). `threshold`
  (first-run onboarding) is not addressable — it serializes to a neutral home hash and the
  landing rule re-derives it.
- **R5 — the brain-swap 'loading' is BY DESIGN.** A Back that swaps the viewed brain
  (world → universe → a different world) shows the map's honest 'loading', because
  `nextReadStatus` (useBuildMap.ts) does not hold last-good across a brain change. Intended
  (the map is expected to change; it never lies with a stale snapshot) — declared here and
  in an e2e comment so it never reads as a false regression.
- **Proof.** 18 router unit tests (`router.test.ts`: parse/serialize every route, the
  deep-link precedence, the evicted/collision fallback) + 4 Playwright flows
  (`e2e/url-router.spec.ts`: deep-link map+block beats landing, real Back world↔universe,
  tray `?block=` Back, evicted-key fallback) + the 23 existing e2e green; tsc + vite build +
  `node --test` (635) + eslint/violet/icon clean; embedded dist rebuilt; no personal path in
  any URL or spec (neutral fixtures only).

## Out of scope (declared)

Atrium (L1 per-world home) = v2. Unlit worlds + ingest-click = v1.1. URL router =
**LANDED** (its own slice — see "§ The hash router" above). Batch ratify = h4nd G.
Aggregate pulse = would need its own law; not proposed.
A separate `archives` queue and per-brain alerts = future, per the honesty notes above.
Cross-brain unification of daemon alerts (surfacing project brains' alerts in one owner panel)
stays out of scope — the owner-alerts panel is the bound session's stock only.

## Validation

RED→GREEN on the anti-hydration law (`m1nd-mcp/tests/universe_endpoint.rs`); vitest on
the new components; Playwright e2e against a fully-mocked owner with fixture brains
(touching the live owner at :1338 / `~/.m1nd` from tests is prohibited by AGENTS.md — the
e2e mocks every `/api/*` route in-page). Final proof is the owner driving the real flow
in a browser (the orchestrator's step, not this burst's).
