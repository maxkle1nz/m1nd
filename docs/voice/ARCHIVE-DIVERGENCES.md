# DIVERGENCES — the archive gesture (feat/archive-superseded-receipt, F2.5e)

Honest record of what the v1 archive gesture deliberately does NOT do, and every
bounded decision made beneath the judged design (`docs/voice/ASKGOD-VERDICT-ARCHIVE.md`,
6 binding changes), per the standing order: implement the honest subset, never invent.

## 1. Out of scope v1 — registered, not built (from the verdict)

- **No auto-archive / daemon.** Nothing ever archives a stale `merge_wait` on its own.
  Staleness is DERIVED at read (the confirm's live two-boundary comparison), never
  materialized, never acted on by a background process. The gesture is always the human's
  explicit click.
- **No bulk "archive all stale".** The tray archives ONE head per gesture. Burying N
  receipts at once is the F11 precedent's danger (bulk after the proven unit gesture, not
  before); it awaits its own slice.
- **No un-archive.** `archived` is terminal in v1. The append-only chain leaves structural
  revival possible later (a future letter could supersede an archived head under a new
  rule), but that rule is left UNDESIGNED here — not stubbed, not half-built.

## 2. Not fixed — pre-existing holes the verdict named but scoped OUT

- **The `failed` hole.** Any agent can already silence the bell by posting a `failed`
  seq+1 letter (no transition rule, no writer gate) — the tray mitigates by pinning
  `failed` loud atop. This arc does NOT fix that hole (it is a separate decision). It only
  ensures the NEW phase does not inherit the looseness: `archived` is human-gated AND
  transition-restricted, exactly because it is the first SILENT-burial verb.
- **`executing` ghost letters (dead runner).** A mission stuck in `executing` because its
  runner died is the sibling problem this does NOT solve. The transition rule is restricted
  to `merge_wait → archived` on purpose; stretching it to sweep `executing` ghosts is a
  deliberate later decision, not this arc's.

## 3. Bounded decisions beneath the verdict

- **Doc tag `(1h)`, not `(1g)`.** The verdict calls the transition rule "§1g" in prose, but
  `HUMAN-VIEW-V2-F25-TECH` already uses **(1g)** for "a real letter names a real block". To
  avoid a real collision the archive rule is registered as **(1h)** across the doc and the
  code comments (`mission_letter.rs`, `missions.ts`). Same rule, non-colliding tag.
- **`stale_head` reuses the `conflict` toast kind.** `landErrorToast` gains a `stale_head`
  branch (binding change 6) that reloads with the text "the state moved — reloading". It
  reuses the existing `LandToastKind` value `conflict` (both reload) rather than minting a
  new kind — `stale_head` IS a concurrent-head conflict, and reusing the value avoids
  touching every `LandToastKind` consumer for no honesty gain. The text is distinct and
  honest; the reload behavior is what matters.
- **`stillImportable` is derived from `boundary_version` only.** The confirm's "still
  importable — archive anyway?" line fires when the block is present AND its current
  `boundary_version` equals the candidate's. It does NOT additionally re-check
  `contract_version` or membership (`resolution_hash`) — the confirm has no pre-import
  resolution hash, and the verdict's headline comparison is the boundary ("proved at v1 —
  the block is at v3"). This is a conservative, honest proxy for "the receipt_import gate
  would still pass"; the real gate (anti-poison, at import) remains the source of truth. A
  false "still importable" only ever adds a warning, never enables a bad write.
- **The archived tip carries the gate but NOT the candidate.** `archiveHead` inherits
  identity + gate (as the landed letter does) but drops `receipt_candidate` from the
  `archived` letter. The superseded receipt is not lost — it stays on the prior `merge_wait`
  letter in the append-only chain (`walk_head` keeps it with its `boundary_version`
  forever). The terminal tip only records the set-aside; re-offering the candidate on an
  archived card would be dishonest.
- **The strip glyph for `archived` is `▤`** (a monochrome filled square, "filed away"),
  chosen to sit calm and distinct beside `✓` landed / `✗` failed; the accent is a muted
  `hairline` rule + `porcelain` chip — a set-aside is deliberately the quietest state, never
  a loud one.

## 4. Live proof boundary

Proven against ephemeral owners (the integration-test pattern) and the pure UI heart
(node:test render + unit specs) — the house's ordered gate. The 3 REAL superseded receipts
on the production owner (`:1338`) are NOT archived here: landing/archiving a real receipt is
a HUMAN gesture, and the dist is Rust-embedded, so the production restart + the owner's own
archive clicks are the orchestrator's declared step, not this executor's.
