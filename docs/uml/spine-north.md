# The Spine — north packet (pre-orient)

`north(task)` is a pure read-only COMPOSER: it fans out four already-shipped handlers
(trust_selftest -> orient -> boot_memory + L1GHT recall -> focus) into ONE honest
pre-orient packet, passing every honesty signal straight through and never fabricating
context on an empty/unbound graph.

## Class

```mermaid
classDiagram
    class handle_north {
        <<composer, server.rs:3167-3610>>
        +agent_id: String
        +task: String
        +top_k: u8 (clamp 1..=50, default 8)
        +scope: Option~String~
        +compose() NorthPacket
        %% NO graph traversal of its own — pure fan-out
    }

    class NorthPacket {
        <<m1nd-north-packet-v0, server.rs:3585-3609>>
        +task
        +binding: BindingSlice
        +context: Option~ContextSlice~
        +memory: Vec~MemoryRow~
        +memory_exists: usize
        +sufficiency: Option~Sufficiency~
        +next_move: String
        +honest_gaps: Vec~String~
        +reception: Option~Value~
        +needs: Option~String~
        +recovery_playbook: Option~Value~
        +landing_bell: Option~Value~ (present iff merge_wait>0)
        +map: Option~Value~ (present iff a SystemBlock store exists: ratified_blocks+coherence)
        +human_view: Option~HumanView~ (present iff composed)
        +proof_state: "triaging"
        +non_claims: [4 disclaimers]
    }

    class HumanView {
        <<m1nd-human-view-v0, human_view.rs>>
        +schema: "m1nd-human-view-v0"
        +state: clean|bell|coherence|mismatch|needs_ingest
        +state_sig: "trust|bell:N|coh:ok·sick|recv:match·mismatch|pulse:╷╷╷╷╷"
        +lines: Vec~String~ (mounted, wordmark + PULSE + gutter, <=4, <=80 cols)
        %% composed AFTER reception — pure, fail-open; pulse dropped under mismatch
    }

    class BindingSlice {
        <<from trust_selftest, server.rs:3205-3249>>
        +trust_mode
        +binding_fingerprint
        +graph_populated: bool
        +graph_state
        +ok: bool
        +recovery_playbook (only when !full trust)
    }

    class MemoryRow {
        <<dual feed, server.rs:3251-3463>>
        +claim: String
        +age_ms: Option~u64~
        +source_agent: Option~String~
        +stale: bool
        +tier: "project" | "medulla"
        +origin_brain: Option~String~
        +node_id: Option~String~
        +kind: "kv" | "light"
    }

    class ContextSlice {
        <<from orient, server.rs:3472-3549>>
        +focus_nodes (marker-filtered)
        +anchors (PageRank)
        +coverage
        +memory_nearby
        +suggested_first_calls
    }

    class Sufficiency {
        <<layers.rs:111-126>>
        +state: sufficient|gathering|saturated
        +top_score
        +captured
        +why
    }

    class handle_trust_selftest {
        <<producer, tools.rs:3654-3788>>
        +verdict_machine()
        +recovery attached only when !ok||suspicious
    }
    class handle_orient {
        <<server.rs:2998-3142>>
        +activate + marker filter
    }
    class handle_boot_memory {
        <<boot_memory_handlers.rs:21-96>>
        +action=list -> KV rows
    }
    class handle_focus {
        <<layer_handlers.rs:962-1042>>
        +token_budget=2000, top_k=60
        +sufficiency (answer-free)
    }
    class handle_seek_light {
        <<scope=light::, layer_handlers.rs:190+>>
        +code excluded before scoring
    }
    class result_shaping {
        <<result_shaping.rs>>
        +pack_to_budget() keep>=1, <=budget
        +estimate_tokens_from_chars() chars/4
        +dedupe_ranked() TOTAL-ORDER key
    }
    class serve_and_compose {
        <<routing wrapper, mcp_http.rs:838+>>
        +folds medulla/all-brains into north.memory
        +append_memory_rows() dedupe by node_id
        %% HTTP/attach transport ONLY
    }

    handle_north --> BindingSlice : lifts
    handle_north --> ContextSlice : lifts (only if populated)
    handle_north --> Sufficiency : lifts
    handle_north --> NorthPacket : assembles
    NorthPacket *-- BindingSlice
    NorthPacket *-- ContextSlice
    NorthPacket *-- "0..*" MemoryRow
    NorthPacket *-- Sufficiency
    handle_north ..> handle_trust_selftest : calls once
    handle_north ..> handle_orient : calls (populated only)
    handle_north ..> handle_boot_memory : action=list
    handle_north ..> handle_focus : budget 2000
    handle_north ..> handle_seek_light : L1GHT recall
    handle_focus ..> result_shaping : budget law
    serve_and_compose ..> handle_north : wraps (tier fold)
```

## Sequence — north compose fan-out

```mermaid
sequenceDiagram
    participant C as Caller (agent)
    participant R as serve_and_compose (mcp_http, HTTP only)
    participant N as handle_north (server.rs:3167)
    participant T as trust_selftest (tools.rs:3654)
    participant B as boot_memory (list)
    participant S as seek light::
    participant O as orient (server.rs:2998)
    participant F as focus (layer_handlers:962)

    C->>R: tools/call north {agent_id, task, top_k?, scope?, tier?}
    Note over R: HTTP/attach: resolve caller_root -> brain (may set reception)
    R->>N: dispatch north on primary brain
    N->>N: validate agent_id + non-empty task, clamp top_k 1..=50
    N->>T: handle_trust_selftest()
    T-->>N: verdict, fingerprint, graph_populated, graph_state, recovery?
    Note over N: needs_ingest = verdict==needs_ingest OR (!populated && verdict in {needs_ingest,orientation_only})
    N->>B: boot_memory action=list
    B-->>N: KV rows -> {claim, age_ms|absent, source_agent, stale, tier}

    alt graph populated
        N->>S: seek scope=light:: (top_k 24, code excluded)
        S-->>N: L1GHT hits -> filter provenance/non-marker, dedupe by node_id, cap 5
        N->>O: handle_orient (activate)
        O-->>N: focus_nodes, anchors, coverage, memory_nearby, suggested_first_calls
        N->>F: handle_focus (budget 2000, top_k 60)
        F-->>N: sufficiency (sufficient|gathering|saturated)
        Note over N: next_move = orient top suggested_first_call
    else empty / unbound
        Note over N: context=null, sufficiency=null,<br/>next_move="run ingest then north again",<br/>push honest_gap
    end

    N->>N: memory_exists = light_memory_count() on-disk, reception_verdict() LAST
    N-->>R: m1nd-north-packet-v0
    opt HTTP tier-recall
        R->>R: fold medulla (+all-brains) rows into north.memory, dedupe by node_id, label tier/origin
    end
    R-->>C: north packet
```

## State/Flow — needs_ingest decision + memory_exists honesty

```mermaid
stateDiagram-v2
    [*] --> Validate
    Validate --> Binding : agent_id + task ok
    Validate --> [*] : InvalidParams (empty task)

    Binding --> NeedsIngest : verdict==needs_ingest OR (!graph_populated)
    Binding --> Populated : graph_populated && full/degraded trust

    NeedsIngest --> Assemble : context=null, sufficiency=null,<br/>needs="needs_ingest", recovery rides packet

    Populated --> RecallLight : boot KV + L1GHT seek(light)
    RecallLight --> HasMatch : L1GHT hit found
    RecallLight --> RecallMiss : empty beat
    RecallMiss --> MemExistsCheck : light_memory_count()
    MemExistsCheck --> HonestGap : count>0 -> "store HAS claims that didn't match" (MED-INV-6)
    MemExistsCheck --> EmptyStore : count==0 -> "no durable memory yet"
    HasMatch --> Orient
    HonestGap --> Orient
    EmptyStore --> Orient
    Orient --> Focus : focus_nodes activated
    Focus --> Assemble

    Assemble --> [*] : return m1nd-north-packet-v0 (reception computed last)
```

## Invariantes

- **Pure composition**: handle_north performs NO graph traversal of its own — only fans out trust_selftest/orient/boot_memory/focus/seek and reshapes their honest outputs (server.rs:3147-3151). [verified: fn handle_north at 3167]
- **Never start cold, never fabricate**: on empty/unbound graph (needs_ingest||!graph_populated) context AND sufficiency are hard-null and needs="needs_ingest" with the repair — no orient/focus over an empty graph (server.rs:3472-3482).
- **Age honesty**: age_ms = now-timestamp only when the stamp is present and sane (ts>0 && ts<=now); absent otherwise, never faked to 'now' (boot KV 3290-3292; L1GHT 3385-3392).
- **memory_exists never lies (MED-INV-6)**: an empty memory[] beat over a non-empty on-disk store carries memory_exists>0 + a gap; recall-miss != empty-store (server.rs:3556-3577; session.rs:1016-1024 light_memory_count — verified at :1016).
- **Mixed-graph recall robustness**: L1GHT recall scoped to the `light::` id prefix so CODE nodes are structurally excluded BEFORE scoring (server.rs:3342-3352, 3421-3423).
- **Marker fragments never occupy a slot**: is_marker_fragment (::tag:: id segment OR leading glyph) excludes annotation nodes from memory rows AND orient focus/anchors (server.rs:2739-2744, 3368-3371, 3057-3061).
- **Repair travels with the diagnosis**: recovery_playbook rides the packet whenever binding is not full trust (server.rs:3244-3249; trust_selftest attaches only when !ok||suspicious tools.rs:3714-3732).
- **Read-only safe**: north is NOT in read_only_denied; every composed handler routes through read-only-safe query paths (server.rs:3164-3166).
- **Landing bell is a vital sign, never a gate**: north counts the missions whose CURRENT head letter is `merge_wait` (mission_letter::heads_by_mission over the bound brain's mailbox box — the same box the tray and mission_post speak, resolved by mission_letter_handlers::mission_box_path). Only when N>0 it pushes ONE honest_gaps line ("N mission(s) in merge_wait await the human landing — the tray is the door") and a structured `landing_bell:{merge_wait:N}` (absent, not null, when N==0 — no empty ornament). A head that later `landed`/`failed` moved off merge_wait and is never counted; historical letters never ring. The box read FAILS OPEN — an absent/unreadable box rings nothing and never takes the packet down (server.rs handle_north, right after the skeleton-coherence signal). Mirrors the skeleton_coherence mould.
- **Budget law**: pack_to_budget always keeps >=1 item even on zero/overflow budget, boundary <=budget inclusive; dedupe_ranked's comparator is a TOTAL order (single scalar key, panic-fix) (result_shaping.rs:80-106, 15-65 — verified: dedupe_ranked at :15, pack_to_budget at :80).
- **top_k clamped 1..=50, default 8**; task non-empty or InvalidParams (server.rs:3188-3199).
- **Sufficiency is answer-free**: reports sufficient|gathering|saturated from a knee test on relevance strength + what was cut, never inspecting answer content (layers.rs:100-126).
- **human_view is composed AFTER reception and never lies under it**: the voice card is assembled from data already in the packet, after `reception_verdict()` returns — under `caller_root_mismatch` the card IS the warning (the reception strings verbatim + the literal repair call) and carries ZERO statistics, because they would describe the wrong brain (human_view.rs; server.rs handle_north tail). Fail-open: the composer is pure and total; a compose miss omits the field, `north` never errors over its own voice.

## human_view — the m1nd voice (`m1nd-human-view-v0`)

The server-composed, ALREADY-MOUNTED human-readable card: the m1nd voice in the
conversation. Law source: the askGOD verdict "human view" (2026-07-12, CHANGE
with 10 amendments) + the SPINE design — both versioned under `docs/voice/`.

**Wire shape** (composed in `m1nd-mcp/src/human_view.rs`, mounted by
`handle_north` after reception):

```json
"human_view": {
  "schema": "m1nd-human-view-v0",
  "state": "clean",
  "state_sig": "full_trust|bell:0|coh:ok|recv:match|pulse:╷╷╷╷╷",
  "lines": ["m1nd ╷╷╷╷╷  full trust · 9,113 nodes · 30 memories"]
}
```

**The five states** (form follows state; priority when signals coexist:
mismatch > needs_ingest > bell > coherence):

| state | form | trigger |
|---|---|---|
| `clean` | 1 line (the whisper) | no signals — the better the world, the smaller the voice |
| `bell` | 2 lines | `landing_bell.merge_wait > 0`; line 2 = the bell gap string VERBATIM |
| `coherence` | ≤4 lines | skeleton_coherence mismatch; the sickness line VERBATIM, wrapped |
| `mismatch` | warning card | `reception.match == caller_root_mismatch`: line 1 = the reception `honest` string, then bound/yours, then `next: ingest project_root=<caller_root>` — ZERO statistics |
| `needs_ingest` | ≤3 lines | empty/unbound graph: `m1nd │ needs_ingest · 0 nodes` + the gap VERBATIM; `next_move` rides only when it fits whole |

**The laws of the field** (verdict amendment numbering):

- (1) The field is `human_view` — never `owner_view` ("owner" = the served
  owner process in this codebase).
- (2) MECHANICAL cap, tested: ≤4 lines, ≤80 chars/line (counted in chars, not
  bytes); wrap breaks at word boundaries and indents +2 inside the gutter
  (continuations start `     │   `); the spine sits fixed at column 6.
- (3) Composed AFTER reception. Under mismatch the card IS the warning and
  shows no statistic — a brand confidently wrong in the human's conversation
  is worse damage than spam.
- (4) The empty/unbound graph is a legitimate honest card (`needs_ingest`),
  never an emergent accident.
- (5) ONE SENTENCE PER FACT: signal lines reuse the exact `honest_gaps`
  strings (the bell line and the coherence line are captured at their compose
  site; the needs-ingest gap is the shared constant
  `human_view::NEEDS_INGEST_GAP`) — never a second wording of the same fact.
  A verbatim line that cannot fit the remaining budget falls WHOLE, never
  truncated (no `…`).
- (8) **Brand law G1 — the written law of this field**: every line carries
  only measured facts already in the packet (counts, states, verbatim gap
  strings). No uncalibrated adjective, no benefit or economy claim, no
  ornament. Zero-valued identity segments are OMITTED (the zero speaks only in
  `needs_ingest`, where it IS the message). Numbers render with the thousands
  separator (`9,024`), nothing else is reformatted.
- The `state_sig` is the mechanical anti-repetition key
  (`trust|bell:N|coh:ok·sick|recv:match·mismatch|pulse:╷╷╷╷╷`): equal state ⇒
  equal sig; agents never render the same sig twice in a session (cadence law
  in `M1ND_INSTRUCTIONS` §7 and the three skills). The pulse row is appended so
  a change in the graph/focus vitals (not only the four legacy tokens) flips
  the key.

**The mark is the PULSE** (amendment 7; the owner's explicit stamp 2026-07-12 —
`M1ND-VOICE-ALIEN.md` §5 variant C). The brand anchor is the WORD `m1nd`
(always lowercase) followed by a FIVE-cell pulse row: `m1nd ╷╷╷│╷  <facts>`. A
cell is calm `╷` (U+2577, narrow-guaranteed) or raised `│` (U+2502, the spine
standing up). The cell order is FIXED FOREVER (the anti-equalizer law, pinned
by `pulse_is_the_fixed_five_cell_signature`): `trust · graph · focus · bell ·
coherence` —
- **trust** rises when `trust_mode != full_trust` (calm on `needs_ingest`, where
  the graph cell owns the message);
- **graph** rises on `needs_ingest`/empty graph;
- **focus** rises when a populated graph activated no focus node;
- **bell** rises when `merge_wait > 0`;
- **coherence** rises on a skeleton-coherence mismatch/stale signal.

Read the row as an EXPRESSION (all low = calm; one stem up = look), never
cell-by-cell in the compact card. The first cell sits at column 6, exactly
under the continuation gutter's spine — the lombada is BORN from the pulse.
Under `caller_root_mismatch` the pulse is DROPPED WHOLE and the plain spine
`m1nd │ ` returns (the vitals would read the wrong brain — S3 is its own
warning). Line 1 is composed by `compose_voice_signature()` alone (the pluggable
seam). ASCII fallback is the AGENT's duty (1:1 map `╷`→`.`, `│`→`|`, `·`→`.`,
`—`→`-`, and the deep-rung proof glyphs `⊢`→`>`, `∎`→`#`; widths identical).

**Line-1 signature**: `m1nd ╷╷╷│╷  <trust> · <N nodes> · <M memories> · map <K>
blocks` — the **map segment** is the SERVED brain's ratified SystemBlock count
(slice-2; the packet's `map` field, PER-BRAIN, never a cross-brain total),
OMITTED when zero (G1: a zero is not "the map exists"). This resolves slice-1
`DIVERGENCES.md` §1, which omitted the segment because the packet carried no
ratified-block count; slice 2 exposes it honestly from the same
`system_blocks_snapshot` read that feeds the coherence signal.

**The `map` field** rides the packet ONLY when the served brain has a
SystemBlock store (absent — not null — otherwise, mirroring `landing_bell`):
`{ "ratified_blocks": <K>, "coherence": "ok"|"mismatch" }`.

**Budget**: re-pinned with the pulse + map field mounted — the packet measures
~1,404 tokens (≤2,000 ceiling; battery `north_packet_within_budget`, fresh
ingest 9,178 nodes, 2026-07-12); the card costs ~43 tokens on a clean beat,
worst case ≤4×80 + envelope (~120 tokens). The pulse adds 5 cells (chars that
were spaces) to line 1 and the cells to `state_sig` (`…|pulse:╷╷╷│╷`); the
width cap is re-verified by the human_view tests.

## cockpit — the navigable menu (`m1nd-cockpit-v0`)

A DEDICATED read-only verb (askGOD verdict "the navigable cockpit", the 10
amendments — `docs/voice/ASKGOD-VERDICT-COCKPIT.md`), the human's ON-REQUEST
router over m1nd's read surfaces. It is a **sibling of `north`, never a field**:
if it breaks it breaks alone, never taking `north` down (fail-open does not
apply). Full contract + class/sequence: `docs/uml/cockpit.md`. In three lines:
- **Root = seven stable-slot collections** (labels move with state, slots never
  do): the tray (POINTER), the map (`system_blocks_snapshot`), missions
  (POINTER), health (`doctor`), trust (`trust`), recent-memories (`boot_memory`,
  fixed projection), drift (`drift`). Pointer entries carry NO verb (amendment 3).
- **The read-only law is DERIVED**: every routed verb is filtered at compose
  time against `server::read_only_denied`, pinned by the test
  `cockpit_read_verbs ∩ READ_ONLY_DENIED_TOOLS = ∅` (amendment 2); the `why`
  text is lifted from the one help catalog (amendment 10), never a parallel one.
- **Navigation**: `select="<slot>"` drills (depth ≤3; `"0"` re-serves root); every
  response carries `menu_sig` (the short reference a widget button carries back —
  never free text, never a write verb) + `store_version` + `state_sig`, and a
  drill says "state moved" when the caller's `seen_store_version` diverged
  (amendment 6). Own budget pinned ~695 tokens (root, ≤800 ceiling; drill ~430).

## Gaps

- **[medium] Freshest-first fallback puts undated legacy claims FIRST**: the broad L1GHT fallback does `hits.sort_by_key(|r| r.authored_ms_ago)` where `Option<u64>` sorts None < Some, so legacy memories with no Created stamp crowd out fresher stamped claims — contradicts the "surfaced most-recent" intent (server.rs:3442-3444).
- **[medium] Medulla fold is HTTP/attach-only**: the cross-store medulla/all-brains fold into north.memory happens ONLY on serve_and_compose; a plain stdio server dispatches north directly (server.rs:4095) with no sibling compose, so a stdio caller's north.memory silently lacks the medulla doctrine feed (mcp_http.rs:838-855 vs server.rs:4095). The memory-is-pull medulla law is only realized on the HTTP path.
- **[low] No aggregate packet budget**: north declares token_budget=2000 to focus and caps L1GHT at 5, but the OUTER north packet has no budget_block; composed memory+context+anchors+gaps can sum arbitrarily large (server.rs:3585-3609; budget_block never called for north).
- **[low] Two overlapping retrieval passes**: sufficiency is a SECOND focus call re-running seek over the whole graph (top_k=60), separate from orient's activate — cost scales with graph size and the two rankings can disagree (server.rs:3486-3493 vs 3518-3529).
- **[low] Uncached per-call disk walk**: memory_exists/light_memory_count does a synchronous read_dir of the agent-memory dir on every north call — O(files) in the hot pre-orient path (session.rs:1016-1024, called at server.rs:3560).
- **[low] honest_gaps are unstructured strings**: the 'not full trust' and 'no focus nodes' gaps are additive English with no machine-readable code, unlike reception/needs (server.rs:3504-3507, 3551-3555).
