# MASSIF Screens

These are structural wireframes only. They bind layout, hierarchy, information placement, labels, empty states, and operator copy. Final color, texture, materiality, and illustration language come from the parallel design calibration captured on 2026-07-06; this file does not bind visual finish.

Glyph key used here:

- `◆` solid: structural evidence found, test-exercised or grounded.
- `♧` growing: orphaned over reference relations.
- `◇` unwired: used but no proof evidence signal.
- `✕` drifting: candidate drift from manifest rules.
- `░` not scanned yet: no `xray:state:*` tag in the snapshot.

## 1. Organism view

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ MASSIF · proof topography                                      manifest present │
├──────────────────────────────────────────────────────────────────────────────┤
│ Live sample captured 2026-07-06                                               │
│ scanned 199 · ◆ solid 0 · ♧ growing 195 · ◇ unwired 4 · ✕ drifting 0           │
│ proof coverage 0.0 · current snapshot paint tags: 0 of 199                    │
├──────────────────────────────────────────────────────────────────────────────┤
│ Legend  ◆ solid  ♧ growing  ◇ unwired  ✕ drifting  ░ not scanned yet          │
│                                                                              │
│  ┌──────────────────── repo root ─────────────────────────────────────────┐  │
│  │                                                                        │  │
│  │   ┌──────── data_access · L0 ────────┐   ┌──── tests · L2 ──────────┐  │  │
│  │   │ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧  │   │ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧    │  │  │
│  │   │ 15 nodes · full membership       │   │ 42 nodes · full required │  │  │
│  │   └──────────────────────────────────┘   └──────────────────────────┘  │  │
│  │                                                                        │  │
│  │   ┌──────── data_access · L1 ───────────────────────────────────────┐  │  │
│  │   │ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ◇ ◇                  │  │  │
│  │   │ 60 nodes · duplicate layer name uses level as key               │  │  │
│  │   └─────────────────────────────────────────────────────────────────┘  │  │
│  │                                                                        │  │
│  │   ┌──────── entry_points · L3 ──────────────────────────────────────┐  │  │
│  │   │ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ♧ ◇ ◇              │  │  │
│  │   │ 82 nodes · border shows wired or holes separately               │  │  │
│  │   └─────────────────────────────────────────────────────────────────┘  │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  Copy note: colors and glyphs show structural evidence state, not correctness.│
└──────────────────────────────────────────────────────────────────────────────┘
```

## 2. Semantic zoom mid-level

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ MASSIF · data_access · L1                                      zoom 64 percent │
├──────────────────────────────────────────────────────────────────────────────┤
│ Container ratio: ♧ growing 58 · ◇ unwired 2 · ◆ solid 0 · ✕ drifting 0        │
│ Border: wired       Lens: architecture layer · full membership requested      │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌──────────────────────── data_access · L1 ──────────────────────────────┐  │
│  │                                                                        │  │
│  │   ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐               │  │
│  │   │ ♧    │ │ ♧    │ │ ♧    │ │ ♧    │ │ ♧    │ │ ♧    │               │  │
│  │   │doc A │ │doc B │ │doc C │ │doc D │ │doc E │ │doc F │               │  │
│  │   └──────┘ └──────┘ └──────┘ └──────┘ └──────┘ └──────┘               │  │
│  │                                                                        │  │
│  │   ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐               │  │
│  │   │ ♧    │ │ ♧    │ │ ◇    │ │ ♧    │ │ ♧    │ │ ◇    │               │  │
│  │   │node G│ │node H│ │node I│ │node J│ │node K│ │node L│               │  │
│  │   └──────┘ └──────┘ └──────┘ └──────┘ └──────┘ └──────┘               │  │
│  │                                                                        │  │
│  │  Farther zoom hides labels. Nearer zoom opens individual evidence lines.│  │
│  └────────────────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────────────┘
```

## 3. Piece detail panel

```text
┌────────────────────────────────────┬─────────────────────────────────────────┐
│ MASSIF block map                   │ Piece detail                            │
├────────────────────────────────────┼─────────────────────────────────────────┤
│                                    │ label                                   │
│   ┌──────┐ ┌──────┐ ┌──────┐      │   node I                                │
│   │ ♧    │ │ ◆    │ │ ♧    │      │ state                                   │
│   │doc H │ │node I│ │doc J │      │   ◆ solid                                │
│   └──────┘ └──────┘ └──────┘      │ exact tag                               │
│                                    │   xray:state:bedrock                    │
│                                    │ evidence                                │
│                                    │   test-exercised or grounded in evidence│
│                                    │ connectedness                           │
│                                    │   incoming refs present · holes unknown │
│                                    │ manifest_source                         │
│                                    │   manifest present                      │
│                                    │ copy note                               │
│                                    │   This block has structural evidence.   │
│                                    │   The map does not assert correctness.  │
│                                    │ actions                                 │
│                                    │   Open source · Show neighbors · Copy ID│
└────────────────────────────────────┴─────────────────────────────────────────┘
```

## 4. First-run 100 percent unpainted

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ MASSIF · proof topography                                                     │
├──────────────────────────────────────────────────────────────────────────────┤
│ Live sample captured 2026-07-06                                               │
│ scanned 199 · snapshot nodes 199 · xray state tags visible now 0              │
│ distribution after dry scan: ◆ 0 · ♧ 195 · ◇ 4 · ✕ 0 · proof coverage 0.0      │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│                         ░ ░ ░ ░ ░ ░ ░ ░ ░ ░ ░                                │
│                      ░ ░ ░ ░ ░ ░ ░ ░ ░ ░ ░ ░ ░                               │
│                   ░ ░ ░ ░ ░ ░ ░ ░ ░ ░ ░ ░ ░ ░ ░                              │
│                                                                              │
│              This graph has not been painted in the current snapshot.         │
│        Run the X-RAY paint scan to persist structural evidence tags,          │
│        then return here to see the maquette fill in place.                    │
│                                                                              │
│              [ Run X-RAY paint scan ]        [ Read what states mean ]        │
│                                                                              │
│  Copy note: not scanned yet is neutral. It is not the same as unwired.        │
└──────────────────────────────────────────────────────────────────────────────┘
```

## 5. No-manifest state

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ MASSIF · proof topography                                  no manifest active │
├──────────────────────────────────────────────────────────────────────────────┤
│ Erosion axis disabled                                                         │
│ Candidate drift needs a manifest rule. Without one, MASSIF can still show     │
│ solid, growing, unwired, and not scanned yet states, but drifting is not       │
│ interpreted.                                                                  │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌──────── repo root ──────────────────────────────────────────────────────┐  │
│  │  ◆ structural evidence blocks     ♧ orphaned blocks      ◇ unwired     │  │
│  │  ░ not scanned yet                ✕ drifting disabled                  │  │
│  └────────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
│  Detail panel copy: manifest_source: no manifest active.                     │
│  Operator note: drifting is unavailable here, not cleared.                   │
└──────────────────────────────────────────────────────────────────────────────┘
```

## 6. Legend and onboarding

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ Reading MASSIF                                                                │
├──────────────────────────────────────────────────────────────────────────────┤
│ Operator words: solid · growing · unwired · drifting · not scanned yet        │
│                                                                              │
│ MASSIF turns graph evidence into a stable place. The same block keeps the     │
│ same place across snapshots; only its state, border, glyph, and detail line   │
│ change.                                                                       │
│                                                                              │
│  ◆ solid                                                                      │
│    Test-exercised or grounded. Structural evidence exists for this node.      │
│                                                                              │
│  ♧ growing                                                                    │
│    Orphaned over reference relations. It may be unused or off-lattice.        │
│                                                                              │
│  ◇ unwired                                                                    │
│    Used by the graph, but no proof evidence signal is attached.               │
│                                                                              │
│  ✕ drifting                                                                   │
│    Candidate drift from the active manifest rules. Review the rule source.    │
│                                                                              │
│  ░ not scanned yet                                                            │
│    No persisted X-RAY state tag is present in this snapshot. Run the scan.    │
│                                                                              │
│  Borders                                                                      │
│    solid border means wired; notched border means possible holes.             │
│                                                                              │
│  Rule                                                                         │
│    The map shows structural evidence, not semantic correctness.               │
└──────────────────────────────────────────────────────────────────────────────┘
```
