# DIVERGENCES — human_view slice 1 (feat/human-view-v1)

Honest record of every point where the implementation deviates from the judged
design (`docs/voice/M1ND-VOICE-DESIGN.md` + `docs/voice/ASKGOD-VERDICT-HUMAN-VIEW.md`),
per the slice's standing order: implement the honest subset, never invent.

## 1. Line 1 omits the ratified-maps segment (coordinator-ruled) — RESOLVED in slice 2

The judged signature is `m1nd │ <trust> · <N nodes> · <M memories> · <K maps
ratified>`. The north packet carries NO ratified-map count today: the
`system_blocks_snapshot` read used for the coherence signal returns
`block_count` (total blocks) and the full store, but no ratified-only count is
computed anywhere on the packet path, and inventing or deriving a new number at
compose time would violate G1's "only facts already in the packet".

**Ruling applied (orchestrator, 2026-07-12):** line 1 renders
`m1nd │ <trust> · <N nodes> · <M memories>` — the maps segment is OMITTED
(mirroring the field's own law: a segment without a value is omitted, never
zero-stuffed). Exposing a ratified-map count in the packet is a slice-2
decision.

**RESOLVED (slice 2, feat/voice-slice2, 2026-07-12):** the count is now measured
from the SAME `system_blocks_snapshot` read (no new read, no invented number):
the packet carries a `map` field (`{ratified_blocks, coherence}`, per-brain,
present iff a store exists) and line 1 gains a `map <K> blocks` segment (omitted
when zero). See `docs/voice/SLICE2-DIVERGENCES.md` for the slice-2 record.

## 2. Overflow drops whole lines — no `…` ellipsis truncation

The design's §3 anatomy sketches an overflow ladder ending in "L2's wrap
becomes an ellipsis `…` on the last permitted line". The slice's amendment-5
restatement is stricter: "a verbatim string that cannot fit the cap → the line
falls WHOLE, never truncated". The stricter law is implemented: every signal
line and the `next:` line ride whole-or-nothing; nothing is ever cut mid-string
and no `…` is minted. (L1 keeps a final mechanical `truncate(4)` guard that is
unreachable in practice — the cap law wins over everything by construction.)

## 3. S3's `next:` uses the design's example form, not `options[]` prose

The design says S3's `next:` is "the literal call from `options[]`", but the
real `reception.options[].call` string is ~200 chars of prose ("ingest with
project_root=… — ONE call: creates a per-project brain…") and can never fit
the cap. The design's own normative S3 example renders the short literal form,
which is what the composer emits: `next: ingest project_root=<caller_root>`.

## 4. Long roots can push S3/S4 below their normative line counts

With real-world long paths (e.g. a tempdir bound workspace), the `bound: … ·
yours: …` line wraps and the `next:` line may fall whole (the cap law). The
normative 3-line S3 form is byte-pinned in unit tests with short roots; the
integration test over a real tempdir asserts the laws (warning verbatim, both
roots present, zero statistics, cap) rather than exact line counts.
