# askGOD verdict — "a voz do m1nd" (human view card) — 2026-07-12

> Seat: askGOD verdict (Fable), marcha medium. Confronted BEFORE implementation.
> Proposal: north packet gains a server-composed human-readable card ("the m1nd voice")
> that agents render in conversation when m1nd contributed structurally to a mission.
> Owner's intent: (a) the owner-view lives inside the conversation; (b) BRAND — m1nd
> as a recognizable entity in every screenshot.

VERDICT: CHANGE
CONFIDENCE: alta

## Core confirmed (with the oracle's own evidence)

The voice is ALREADY server-side today: honest_gaps, next_move and the landing-bell line
are human English sentences composed in server.rs (:3794, :3885, :3940) and relayed by
agents in the conversation language. The proposed field is the natural extension of that
pattern. English-at-source + agent translation APPROVED (whole product surface is
English-at-source; i18n/ is README-only; MCP wire carries no locale). The frame must be
served ALREADY MOUNTED by the server — agent-mounted frames would recreate the divergence
that killed alternative (a).

## The 10 required changes

1. **RENAME the field.** "owner" in this codebase = the SERVED OWNER process. `owner_view`
   reads as "the server process's view". Use `human_view` (HUMAN-LAYER-PRD language) or
   `for_human`.
2. **Budget Law (§C1.3).** Packet pinned ~1,419 tokens, ceiling 2k (PATHOS.md:159-165;
   m1nd_battery.py:498-546 north_packet_within_budget). Field costs bytes on EVERY north
   beat incl. SessionStart ambient injections. Require: tested cap (≤4 lines, ≤80
   chars/line, total char ceiling) + battery re-run with the new number recorded.
3. **Honesty under reception.** Reception is computed LAST (server.rs:3947-3951); a card
   composed before it describes the WRONG brain. Under `caller_root_mismatch` the card is
   omitted OR line 1 becomes the mismatch warning itself. Mandatory test of this shape.
   ("The brand confidently wrong in the human's conversation is worse damage than spam.")
4. **Define the needs_ingest shape.** Empty/unbound graph → "m1nd doesn't know this repo
   yet — run ingest" is a legitimate honest card. Decide explicitly, don't let it emerge.
5. **One sentence per fact.** Line 3 REUSES the exact strings already composed in
   honest_gaps (bell :3940-3942; coherence :3916-3918) — never a second wording of the
   same fact (double token cost + future divergence).
6. **Cadence ALSO in M1ND_INSTRUCTIONS as a NEGATIVE default** ("do NOT show the card
   unless…"; never consecutive messages; on state change or first orient). Instructions-only
   hosts (Gemini, matrix :91) have no skill holding the anti-spam line. agent-docs gate
   forces same-PR coupling.
7. **The ⛩ glyph exists NOWHERE in the repo** — a new permanent mark minted in passing,
   religious/geographic connotation, unstable terminal rendering. THE OWNER ratifies the
   mark explicitly; specify ASCII-safe fallback — the brand constant is the word "m1nd" in
   the signature, the glyph is optional decoration. "│" (U+2502) acceptable.
8. **Brand gate G1/G1.5 as written law of the field:** only measured facts already in the
   packet (counts, states, gap strings) — never benefit/economy claims, never uncalibrated
   adjectives. Record in spine-north.md same PR.
9. **Ladder ritual:** owner's order but off-§C10 — register as §C11-style amendment (or an
   explicit human-layer arc slice), never a silent front (PATHOS.md:355-356).
10. **Doc gate same PR:** spine-north.md + 3 skill surfaces + M1ND_INSTRUCTIONS
    (era-coherence #337). Proposed Rust tests stay, plus cases 3 and 4.

## Risks the proposer missed

- The entire Budget Law (the only risk with CI already pointing at it).
- Intra-packet duplication (same fact in two proses).
- The card lying under reception mismatch — brand damage worse than spam.
- Name collision with "served owner".
- Display opt-in does not cancel cost: field travels in EVERY ambient injection.
- The static-banner problem: lines 1-2 near-constant per repo — identical repeated cards
  read as advertising, not intelligence; needs "only on state change / never repeated in
  the same session".
- The G1 precedent: this product already removed two unmeasured-claim surfaces by brand
  law; a brand card without that written law reopens the door.

## Status

- Owner pivoted (2026-07-12) BEFORE stamping: two new Fable seats dispatched — a marketing
  director's dossier (market/positioning/the gold) and a text-artist's design of the voice
  itself (chars, states, depth mode). Implementation waits for: artist spec + owner's
  ratification of the mark (change 7) + this verdict's changes folded in.
