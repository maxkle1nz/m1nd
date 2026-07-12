# askGOD verdict — the navigable cockpit (m1nd menu) — 2026-07-12

> Seat: askGOD verdict (Fable), marcha medium, 9 files read. Confronted BEFORE implementation.
> Proposal judged: human_view v2 gains a navigable menu (3 layers: served contract, text
> grammar on the spine, rich widget via sendPrompt), read-only sovereignty law.

VERDICT: CHANGE
CONFIDENCE: alta

## What the repo had already answered (the oracle's gifts)

- **The read-only classification ALREADY EXISTS**: `READ_ONLY_DENIED_TOOLS`
  (server.rs:2862-2930) is the single mechanical source. A "closed list in the contract"
  would mint a SECOND source of truth — the real containment hole is divergence between
  them. The menu must DERIVE from the deny list (deny grows → menu shrinks alone), with a
  Rust test asserting `entries.verb ∩ READ_ONLY_DENIED_TOOLS = ∅` + the `persist`
  status-only rule (:2948-2954).
- **`learn` and `mission_post` are not grey** — the deny list already classifies them as
  writes (:2870, :2922). No exceptions in v1.
- **The menu-entry FORM already exists in the binary**: `help_guidance.rs` OverviewCard
  {title, why, tool, arguments, reject_tool, reject_reason} (:64-118). Reuse the help
  machinery; a parallel catalog would repeat the era-coherence sin inside the binary.
- **m1nd already HAS a rich human menu**: the served UI (:1338). The chat menu is a second
  navigable surface over the same state → decision-staleness risk (human deciding from a
  stale menu; sig protects reference identity, not decision freshness).

## The 10 required changes

1. Architecture = DEDICATED READ-ONLY VERB (`cockpit` or a human mode of `help`) — never a
   north field (north is a single pinned packet; R2 is agent-rendered by ratified
   definition — the dossier's "menu as served R2" was an internal contradiction), never
   agent-composed. Pure-composer pattern (spine-north). Fail-open does not apply — if it
   breaks, it breaks alone, never north with it.
2. Read-only law anchored on the single source (derivation + Rust test), no hand-kept list.
3. Pointer entries (tray/ratify/import) carry NO executable verb field — only the door text
   ("open the tray — the stamp lives there"). Nothing to execute even by mistake.
4. Root = argument-less reads only: bell/tray, maps+freshness, missions, health
   (doctor/alerts), trust, recent-memories (fixed-N projection, never open seek), drift.
   impact/why/trace leave the root — they become drill-down actions from a SELECTED item
   (context supplies the target) or stay out of v1.
5. Max depth 3 (root → collection → item); receipts/hashes render INSIDE the item view
   (as S5 already does); "0" reserved = back/re-serve root.
6. Stable slots for the permanent trio (condition changes the LABEL, not the slot);
   menu_sig mandatory; drill-down responses carry and re-assert `store_version`/`state_sig`
   — diverged → honest "state moved" before answering.
7. Widget: button payload = the SAME short reference a human would type (number + menu_sig),
   never free command text, never strings interpolated from graph/repo content
   (prompt-injection channel). Never-lands inherited verbatim: no button names a write verb.
   Widget leaving prototype → uiproof + two-skins law.
8. Trigger strictly on-request in v1 ("?" / explicit ask). Cut "landing moment" as a menu
   auto-serve — at landing the S5 card speaks and the door is the tray, not a navigation
   menu. Negative default verbatim in M1ND_INSTRUCTIONS + 3 skills, same PR.
9. The verb gets its OWN budget, battery-pinned (north's pin does not cover it); number
   recorded in the PR.
10. ONE §C11 ritual covering human_view v1 + menu as one arc (never two silent fronts);
    SEQUENCED delivery: v1 (card) lands first, menu is the next slice of the same arc.

## Risks the proposer missed (owner-relevant)

- sendPrompt clicks fabricate a message with USER AUTHORITY — a click behind a write verb
  would be laundered consent; graph-derived labels as click payload are a prompt-injection
  channel. (This kills "approve inside the chat widget" at the law level, independent of
  the CSP sandbox that also blocks it.)
- The R2 contradiction; the missing own-budget; the second-source-of-truth trap; the
  argument-carrying verbs breaking the numeric grammar.

## Status

- human_view v1 (card) still awaits the owner's stamp on the provador (7 forms, 2 poles;
  artist recommends PULSO). Cockpit verb is slice 2 of the same arc after v1 lands.
- Owner's newest intent folded in: "read everything inside the widget" = yes (template-fed
  reads); "approve inside the widget" = NO by law (laundered consent) — lowest legitimate
  friction is deep-link to the tray card; native in-widget approval belongs to the served
  UI/tray-app, where the click travels browser→server without a model in between.
- Template-fed widget pattern (STATE JSON + versioned template, agent pastes both) aligns:
  the served `cockpit` verb IS the JSON source; the template lives versioned in the repo.
