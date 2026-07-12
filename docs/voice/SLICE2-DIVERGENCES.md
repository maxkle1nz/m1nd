# DIVERGENCES — human_view slice 2 (feat/voice-slice2)

Honest record of every point where the slice-2 implementation deviates from, or
makes a bounded decision beneath, the judged designs (`M1ND-VOICE-ALIEN.md` §5,
`ASKGOD-VERDICT-COCKPIT.md`, `M1ND-VOICE-DESIGN.md`), per the standing order:
implement the honest subset, never invent.

## 1. The map segment reads `map <N> blocks`, not `<N> maps ratified`

The design docs illustrate the maps segment as `4 maps ratified`
(`M1ND-VOICE-DESIGN.md` §5). The slice-2 order gives the literal segment
`map <N> blocks`, and this is what ships. It is the MORE honest noun: the served
brain has ONE skeleton with N ratified SystemBlocks, not "N maps". The count is
strictly the ratified-block count; a zero omits the segment (G1). The structured
`map` field names it unambiguously (`ratified_blocks`).

## 2. The cockpit is a ROUTER in v1, not an inline executor

The verdict's amendment 5 ("receipts/hashes render INSIDE the item view") and
amendment 6 ("drill-down responses carry store_version/state_sig") admit two
readings: (a) the cockpit executes each read verb and serves its output inline;
(b) the cockpit is a navigation ROUTER (like the help overview) that presents the
argument-less call to run, whose OWN output carries the receipts. **Slice 2 ships
(b)** — the pure-router reading — because it is what "reuse the help machine"
(amendment 10) means: the help overview presents `minimal_calls`, it never
executes. The cockpit therefore never fabricates a receipt; a read drill hands
the exact argument-less call, and the verb's output renders the receipts in the
item view (as S5 already does). Serving verb output inline is a future slice.
This is a scope decision, not a law break: all ten laws hold on the router.

## 3. Missions is a POINTER (to the tray), not a served list verb

Amendment 4 lists `missions` as a root collection. There is no argument-less MCP
READ verb that lists mission letters (the box is read via the tray/HTTP surface,
and `mission_next` needs a `mission_id`). Slice 2 renders `missions` as a POINTER
to the tray (carrying a measured count label from the same box read the bell
uses) — faithful to amendment 3 (pointer = door, no verb) and to "the tray is
the door". The mission COUNT is honest (measured); the ACTION lives at the tray.

## 4. The cockpit's `state_sig` is the menu's own, not byte-identical to human_view's

The human_view `state_sig` is `trust|bell:N|coh|recv|pulse`. The cockpit
re-asserts state with its own menu-shaped key `bell:N|map:K|coh|recv|sv:V`
(amendment 6 requires "store_version/state_sig", and `sv` carries the store
version the pulse sig does not). Both re-affirm the same beat; they are not the
same string. Unifying them is a future tidy, noted in `docs/uml/cockpit.md`.

## 5. `health` folds `alerts` into one slot

Amendment 4 writes "health (doctor/alerts)". Slice 2 maps the health slot to the
`doctor` read (the richer census) and names alerts in its label/why, rather than
minting a separate `alerts_list` slot — keeping the root at the seven collections
the amendment enumerates. `alerts_list` remains a valid read the human can run
from doctor's output.

## 6. Live bell/map render is unit-pinned, not shown on the fresh-ingest probe

The budget probe ingests a throwaway temp graph with no missions and no
SystemBlock store, so the LIVE render shows the calm state
(`m1nd ╷╷╷╷╷  full trust · 9,178 nodes`) with no bell/map segment. The bell
render (`m1nd ╷╷╷│╷  …` + the verbatim bell line), the map segment
(`· map 12 blocks`), the needs_ingest pulse (`╷│╷╷╷`), and the mismatch
pulse-drop are pinned deterministically by the human_view unit tests rather than
by the live probe. No behavior is unproven — only the surface that would require
seeding missions + a store to exercise live.
