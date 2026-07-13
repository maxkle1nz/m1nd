# askGOD verdict — "archive a superseded receipt" — 2026-07-13

VERDICT: APPROVE · CONFIDENCE alta (arquitetura+soberania) · 6 binding changes.

## The shape ruled
- **An 8th TERMINAL phase `archived`** on the mission letter; archiving = posting a
  seq+1 letter EXTENDING the merge_wait head. Minimal reuse: the bell drops itself
  (server.rs:4056 filters `phase == MergeWait`), history is free (append-only mailbox +
  walk_head keeps the stale receipt with its boundary_version forever), OCC is free
  (head-CAS → a race with an Import in another tab yields stale_head, nothing appended),
  who/when = the mailbox envelope `agent` + `updated_at` (zero new field).
- Rejected: a `superseded_at` field (N places to lie), reusing `Failed` (dishonest — the
  gate WAS green — and `failed` is pinned loud at the top), presentation-only dismiss
  (doesn't drop the server-side bell).

## The 6 binding changes
1. **Human-only gate is MANDATORY** in `handle_mission_post`, armed when `phase ==
   archived`: origin token in the INPUT (never in the letter schema), closed allow-list
   `"human-ui"`, refuse `human_gesture_required` with nothing appended — literal mirror
   of system_blocks_handlers.rs:448/528 + a forged-origin test. (The dossier left this a
   question; the answer is YES — same class of gate. The danger is not the map, which
   stays honest — it's ATTENTION-BURIAL: an agent silencing its own unproven work. This
   would be the product's first SILENT-burial verb; `failed` at least pins loud.)
2. **Phase name is `archived`, NOT `superseded`** — `superseded` already means
   `MissionHead.superseded_count` in the same module (mission_letter.rs:525). UI copy uses
   the word in the right place: "Archive — superseded by newer boundary".
3. **`validate()` must refuse `receipt.imported == true`** on an archived letter (landed-
   in-disguise is a lie) and inherit identity+gate as landed does (missions.ts:691-708).
4. **First transition rule of the board, narrow:** `archived` only extends a `merge_wait`
   head (checked in post_mission_letter, which already holds the head for the CAS).
   Registered as the board's FIRST transition rule — deliberate.
5. **TS mirror in the SAME PR:** Phase/PHASES/PHASE_META/PHASE_ACCENT — MissionCard.tsx:124
   crashes on an unknown phase (`PHASE_META[phase].glyph`); render tests for the new phase.
6. **Archive toast handles `stale_head`** (reload + "state moved"): landErrorToast doesn't
   recognize it today (missions.ts:586-607) → generic error without reload; the
   archive×import two-tab race would confuse the owner.

## Honesty / scope
- The CONFIRM fetches a FRESH snapshot at click (landCandidate step 1) and SHOWS the live
  comparison ("proved at boundary v1 — the block is at v3"); if the receipt is still
  importable, it says so aloud ("still importable — archive anyway?"). No frozen "current
  boundary" field (Budget Law + schema discipline).
- Staleness suggestion is DERIVED at read (the presences-collision precedent), never
  materialized. Never a daemon auto-archive; NO "archive all stale" in v1 (bulk buries N
  at once — the F11 precedent puts bulk after the proven unit gesture). No un-archive in
  v1 (the chain allows structural revival later — left undesigned, registered).

## Risks the dossier missed
- Pre-existing `failed` hole: any agent can already silence the bell posting `failed`
  seq+1 (no transition/writer guard) — the tray mitigates by pinning loud. This arc need
  not fix it, but the new phase must NOT inherit that looseness (→ change 1).
- Downgrade skew: an old binary drops the unparseable `archived` letter → head regresses
  to merge_wait → the bell RE-RINGS on an archived mission (trust wound "the system forgot
  my gesture"). No corruption; register as known in the amendment.
- Live proof is in production (the 3 real receipts on :1338) — ephemeral owner FIRST,
  production after, restart declared (dist is rust-embedded).
- Discard button next to a landing button invites misclick — the two-boundary confirm is
  also the ergonomic guard, not just honesty.
- `executing` ghost letters (dead runner) are the sibling problem this does NOT solve
  (transition restricted to merge_wait) — decide deliberately later, do not stretch.
