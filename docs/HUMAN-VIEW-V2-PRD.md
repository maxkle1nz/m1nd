# Human View v2 — PRD

**The visual operating system for repos.**
Code → visible architecture → actionable blocks → missions for agents.

> Status: DRAFT for owner ratification · 2026-07-07
> Supersedes: the MASSIF v1 PRD (absorbed — see §14) and **explicitly revokes** the Human-Layer PRD decision that made the Living Tree the front door with the map as drill-down. In v2 the **Build Map is the front door**; the Living Tree remains as the deterministic navigation surface one click away. This revocation is deliberate and owner-directed.
> Sisters: `HUMAN-VIEW-V2-SCREENS.md` (every screen and sub-screen, ASCII, in the owner's ARTKIT design language) · `HUMAN-VIEW-V2-UML.md` (systems and subsystems). The ARTKIT v2.0.0 sheet (palette, typography, block anatomy, proof beads, connection styles) is the binding design system.

---

## 1. Thesis

The engine already computes what no other tool computes: per-part structural evidence, blast radius, drift candidates, freshness, delegation packets, outcome ledgers. All of it is invisible to humans. v2 renders that truth as a **map of living blocks** a non-engineer can read, point at, and command — while agents keep consuming the same truth through verbs. **One source of truth, two projections.** The human never has to explain the repo to an agent again: they point at a block.

Not a node-link graph. Not a file tree. Blocks with human names — Auth, Payments, Emails — each carrying its evidence state, its connections, its receipts, and a direct line to agents.

## 2. Users

1. **The owner-operator** — reads system state at a glance, ratifies skeletons, routes missions.
2. **Vibecoders** — people who build software by directing AI agents and do not read code line-by-line. The map answers: *what exists, what is missing, what has evidence, what is broken, who is working on what.*
3. **Agents** — consume the same blocks through verbs and packets; never through pixels.

## 3. The ontology: `SystemBlock` (the new contract — nothing renders without it)

The oracle's central correction, adopted in full: the existing organs (delegate, surgical_context, impact, layers, X-RAY, mailbox) are node/file/packet-centric. v2 requires a first-class product ontology, defined **before** any UI:

```
SystemBlock {
  block_id            stable, versioned
  name                human name ("Auth") — ratified, never auto-asserted
  purpose             one sentence in operator language
  membership          many-to-many: file/node ids → blocks; a file MAY belong to several blocks
  membership_source   ratified | proposed(confidence) | manual
  ratifier + version  who approved this boundary, when; boundaries version like code
  sockets             declared inputs/outputs (internal blocks AND external services — Stripe,
                      a mail provider, queues, cron are sockets/blocks too)
  receipts[]          typed receipts (see §4) — the block's evidence, with auditable denominators
  states              derived: evidence rollup (§5), freshness, runtime signal, agent activity
  unmapped_residue    files inside the boundary claimed by no block — first-class, always visible
  links               real node_ids in the graph (the block is a projection, never a copy)
}
```

**Ratification law:** auto-clustering (the Skeleton engine, §6) only ever produces a **candidate skeleton** — names suggested, boundary diffs, confidence, unmapped files, multi-owner seams. A candidate becomes the Build Map **only after human ratification** (the Edit Names & Boundaries screen). Ratified skeletons drift-detect: new files with no block, blocks whose files vanished, broken sockets, names that no longer match reality — surfaced as `stale ratification`, never silently re-clustered.

## 3.1 Concurrency & the transactional law (the store's first invariant)

The SystemBlock store is touched concurrently: a scan re-attaches members while the owner ratifies a boundary; a receipt expires while a debrief lands a pin; span ingestion recolors runtime while a recipe edits sockets. Without concurrency control the map can show a **ratified block with fresh receipts against a stale boundary**, or promote an agent output into **evidence for the wrong contract version** — the worst failure for an evidence tool. Law:
- Every `SystemBlock`, its **boundary**, its **contract**, and each **receipt** carry a version.
- Every mutation (ratify, re-scan attach, recipe edit, receipt earn/expire, debrief→pin, span recolor, delete/archive) is an **optimistic-concurrency transaction** keyed on the version it read. A stale write is **rejected and reloaded**, never silently applied (see the delete/archive machine, UML §9.11, and the OCC `Conflict` state).
- A receipt is **bound to the `(block_id, boundary_version, contract_version)` it was earned against**. If any of those advanced, the receipt is re-scoped or marked stale — **never counted for the new version**.
- Pins/outcomes carry the `contract_version` they targeted; a debrief **cannot become evidence for a version it never saw**.

## 4. Receipt taxonomy (before any "6/6" appears on screen)

A receipt is a **typed, attributable, expirable** piece of evidence attached to a block:

| Type | Emitted by | Example | Expires |
|---|---|---|---|
| `test` | test run over the block's members | "login flow — 14 tests green" | on member change (stale) |
| `structural` | X-RAY paint | "test-exercised or grounded" | on re-paint |
| `runtime` | OTel span ingestion | "P95 210ms · 0 errors · 24h window" | window end |
| `review` | an agent or human review mission | "webhook signature verified" | on member change |
| `handoff` | soul_check (PATHOS claims) | "claims fresh as of scan" | per soul rules |
| `spec` | ULM / Block Recipe contract | "business rules attached" | on contract edit |

Counters like **"Receipts 6/6"** always mean: *of the N receipt types this block declares it needs (its contract), M are present and fresh.* The denominator is the block's own declared contract — auditable, never a vibe. **"Proved" is banned as a generic badge.** The UI says `Receipts 6/6`, `Evidence-backed`, `Covered by login-flow receipt` — scoped claims with denominators.

## 5. The state grammar and the rollup policy

Per-node states come from the persisted `xray:state:*` tags already carried by the snapshot the UI fetches (zero new backend for reading). Block-level color is a **written policy, not a color average**:

| Color | Meaning | Fires when |
|---|---|---|
| **Green** | evidence-backed | every declared receipt present & fresh AND no member is erosion-candidate/broken |
| **Amber** | needs evidence | exists and wired, but declared receipts missing/stale (`unproven` members, expired receipts) |
| **Red** | broken / drifting | failing receipt, erosion-candidate member with ratified manifest rule, broken socket, or dead code block |
| **Purple** | unknown | the engine abstains: unmapped residue dominant, no scan yet, insufficient evidence — "not scanned yet" is NEUTRAL and normal, never rendered as amber |
| **Blue** | runtime signal | ONLY when OTel spans were ingested and mapped; absence of telemetry renders as "no runtime signal", never as healthy/cold |

Rollup edge rules (explicit): truncated membership never rolls up (request full membership or show `≥ N`); mixed blocks show **proportion**, not worst-of; the block ring (ARTKIT Proof Ring) carries freshness; borders carry wired/holes separately from fill.

**Runtime signal is read-only at render.** The blue signal is read from a persisted, read-only runtime overlay artifact — the render NEVER calls the span-ingesting overlay verb (which mutates/persists boosts). Ingestion is a separate, explicit operator step; the map only reads its last persisted result. No spans persisted ⇒ `no runtime signal`, never a fabricated blue.

## 6. The two paths onto the map

**Brownfield — LIGHT Skeleton** (screen: pipeline): `Scan repo → Cluster purpose → Name blocks → Attach files → Read receipts → Render map`. Clustering uses the graph (communities, directories, semantics); **naming is agent work** — a **minimal one-shot naming runner** proposes names/purposes (with a manual-naming fallback if it is unavailable); the full policy-gated runner fleet (§7) is not required for F0c — it arrives in F2.5; the pipeline outputs a **candidate** with confidences and unmapped residue; the human ratifies. The engine orchestrates its own analysis — passivity ends here, on day one.

**Greenfield — ULM Generator** (screen: intent → blueprint): Project Intent + Audience + Core Flows → a product blueprint of blocks (Product Core, Auth, Data, Workflows, UI Surfaces, **Proof Gates**) with Blueprint Readiness per axis → **Send to Build Map** as `Planned` blocks carrying their Visual Contract (sockets, needed receipts). Both paths converge on the same map: scanned blocks beside planned ones.

## 7. Orchestration — the leap out of passivity (in the MVP)

The runner layer is **pluggable by capability**: each role is filled by whatever local agent the operator configures — a **build-runner** (one-shot code), a **naming-runner** (a fast, cheap model), a **loop-runner** (a deterministic-gated loop engine), a **hand-runner** (a superior implementation + verifier), a **review-runner** (a judge). These roles are slated to become **first-class m1nd runners** over time (today they are filled by the operator's own configured local agents). The engine already has the bottom half: `delegate` composes packets and writes a registry; `debrief` classifies what was touched; the outcomes ledger and the mailbox fates exist. v2 adds the thin missing middle:

```
MissionPacket (block-scoped delegate packet, one shape for all modes)
   mode: clipboard   → rendered as Markdown, copied; universal day-1 bridge (works with ANY agent)
   mode: direct      → delivered to the target's inbox (the mailbox that already exists)
   mode: spawn       → handed to a RUNNER that launches a real agent
Runner (thin adapter per bridge)
   build-runner   a one-shot build agent — MVP's first validated cycle (simplest)
   naming-runner  a fast, cheap model — the naming/research lane (skeleton naming, research)
   loop-runner    long-loop missions with a deterministic gate — wave 2 (needs heartbeat UI)
   hand-runner    key-moment implementations — wave 2
Return path: agent output → debrief → outcomes ledger → PIN on the block
```

**Policy layer (non-negotiable, from birth):**
- Every agent card declares **capabilities** (can edit / can test / can read m1nd / can receive packets) and its **workspace truth** (bound root; `wrong workspace` is rendered — it is the engine's reception/caller_root state as UI).
- Every spawn mission runs in an **isolated worktree** (the operator's worktree-per-agent doctrine, now product law).
- Agents **propose**; nothing auto-applies. Landing stays with the human/CI. The "Apply" button on a pin opens the diff, never merges silently.
- Audit trail = the delegate registry + outcomes ledger (already persisted).
- **Routing Rules** (screen: Clients & Agents): work-type → default runner by capability (Build→build-runner, Review→review-runner, Research→naming-runner, Stress→loop-runner). A capability role is filled by whatever connected agent the operator assigns; rules route, and the human can always override per-mission.

**Pins** are projections of the outcomes ledger + mailbox fates: `running` (heartbeat), `needs reply`, `output landed (unverified)`, `debriefed`, `failed`. A pin becomes a **receipt** only when its outcome passes the block's declared receipt rules — answers are not evidence by default.

## 8. Functional requirements (per surface)

**Build Map (front door)**
- F1. Renders ratified SystemBlocks as ARTKIT block cards (domain tag, name, repo/path mono, sockets, metric beads, proof ring, timestamp) on a stable layout (same block → same place across scans).
- F2. Block fill = rollup policy (§5); border = wired/holes; ring = freshness; beads = C/T/D/! metrics.
- F3. Connections render as typed wires (dependency, optional, test-coverage, broken) with **proof beads on wires** — edge evidence is first-class.
- F4. Sub-blocks render as chips inside the card (Login, Session, Users, Roles) with per-chip state dots.
- F5. Filters by state; System Health sidebar with counts; search over blocks/receipts.
- F6. First-run: 100% candidate/unratified state is a first-class screen (call to ratify, never an error). Read-only attach renders scan/spawn controls disabled with honest copy.
- F7. Unmapped residue is always visible (an "Unmapped" tray) — the map never pretends full coverage.

**Show Code (modal, per block)**
- F8. Tabs Files / Tests / Receipts / Impact. Files grouped **by purpose** with per-group state dots; real code viewer; a human explanation line per file; USED BY / IMPACTS chips (from the graph).
- F9. Health panel: files/tests/receipts counters with auditable denominators; type/lint/build/security checks; coverage; freshness; change risk + blast radius (impact verb).
- F10. Actions: Open in editor, Copy context, Ask agent.

**Ratification (Edit Names & Boundaries)**
- F11. Candidate skeleton review: proposed names (editable), boundary diffs, per-block confidence, unmapped files, multi-owner seams flagged. Approve = ratify (versioned); partial approval allowed. Drift alerts reopen this screen scoped to the drifted block.

**Ask Agent / Copy Packet**
- F12. From any block/sub-block: compose MissionPacket with toggles (screenshot, block details, likely files, receipts state, impact overview); preview exactly what will be sent; mode selector clipboard/direct/spawn (spawn only for agents whose capabilities+workspace allow it).
- F13. The packet is block-scoped and declares its effects (registry write on delegate reuse is admitted in the UI, or the read-only compositor is used for clipboard mode).

**Block Recipe (Planned blocks)**
- F14. New block = contract: name, purpose, expected inputs/outputs (typed sockets), needed receipts, suggested agent. Contract completeness is enforced: **cannot Send to Agent until every socket is defined** (the mockup's "0/5 connected" gate).

**Clients & Agents + Routing**
- F15. Agent cards with connection state, workspace truth, capability chips, inbox. Routing Rules table. Packet mode toggle. Locally-configured runners (build / loop / review / hand roles) are first-class agents alongside connected external tools.

**ULM Generator**
- F16. Intent/Audience/Flows → blueprint canvas → readiness panel → Generate Blueprint → Send to Build Map (Planned blocks with contracts). The five agent roles (Architect, Researcher, Builder, Reviewer, Stress Tester) render as pipeline progress.

**Pins / Mission tracking**
- F17. Pins dock on blocks; statuses from §7; Apply opens diff; View report opens debrief; pins never auto-become receipts.

## 9. Non-goals (v2)

- No 3D/WebGPU floor; no Voronoi; no auto-apply of agent output; no trust-envelope in the resting visual channel (hover/phase-2); no synthetic "runtime health" without spans; no silent re-clustering of ratified skeletons; no generic "Proved" badge anywhere.

## 10. Phasing (oracle order + the owner's leap, reconciled)

| Phase | Delivers | Proof gate |
|---|---|---|
| **F0a** | SystemBlock contract + receipt taxonomy (docs + Rust types + store) | contract review; types compile; fixtures |
| **F0b** | Seed skeleton of the m1nd repo itself, **ratified by the owner** (manual/assisted) | owner ratifies with <20% manual correction |
| **F0c** | Skeleton engine (candidate pipeline) with **naming-runner** | candidate quality vs seed; unmapped residue honest |
| **F1** | Build Map read-only (ARTKIT), real data, all degraded states | vibecoder legibility test; no green without receipts |
| **F2** | Show Code + Copy Packet (clipboard) | packet solves a real task in a cold agent |
| **F2.5** | **Spawn cycle with build-runner + naming-runner** (policy layer live) | UI-launched mission returns as pin + debrief |
| **F3** | Block Recipe (Planned blocks, contracts) + ULM Generator | contract gate blocks incomplete sends |
| **F4** | Routing Rules + loop-runner / hand-runner + Mission heartbeats | routed mission with live progress + audit trail |

## 11. Validation

(a) Owner recognizes the m1nd seed skeleton with <20% correction; (b) a Copy Packet pasted into a cold agent resolves a real task with zero repo explanation; (c) the Build Map with real data is legible by a non-programmer; (d) no green state exists on screen without a present, fresh receipt behind it; (e) a spawn mission launched from the UI lands as a pin with an auditable trail; (f) colorblind pass (states never hue-only); (g) performance: map interactive at 1k blocks (cache + LOD documented).

## 12. Risks (owned, from the oracle — mitigation in-design)

Ontological authority (who says it's "Auth" → ratification + versioning); many-to-many membership (modeled from birth); external dependencies as sockets/blocks; overclaim amplified by a beautiful map (receipt denominators + copy law); receipts as empty gamification (typed taxonomy, declared denominators); runtime false-absence ("no runtime signal"); pins ≠ receipts; stale ratification (drift detection); security of spawn (capabilities + worktrees + propose-only + audit); performance at scale; planned blocks as imaginary architecture (contract gate); killing the Living Tree's determinism (it stays, one click away).

## 13. Copy law (public-repo reputation gate)

Operator language everywhere; every claim scoped and auditable. Banned: "proven", "done", "correct" as states. Required: "evidence-backed", "receipts N/M", "candidate drift", "not scanned yet", "no runtime signal". Public demo surfaces hide local paths, workspace names, agent internals; the operator surface shows them, marked internal.

## 14. What this supersedes

MASSIF v1 (PRD/UML/screens, commits f472214+fd24dae) is **absorbed**: its honest-state rules (unpainted, manifest_source, candidate≠violation, no truncated rollups, read-only disabled states, colorblind redundancy) carry into §5/§8 verbatim; its isometric-maquette rendering direction is **discarded** (owner verdict); its scope is widened from "a proof-topography lens" to the visual OS described here. The Human-Layer PRD front-door decision is revoked per the header note.

## 15. Review compliance map

**17 of the 18** required changes from the first 2026-07-07 verdict are addressed **in these documents**; **#11 is a code pre-work item** (the `toolTypes.ts` `TrustEnvelope.verdict` `unprovable` union + its `verdictLabel`/`VerdictChip` consumers + a regression guard) landed **separately before F0a**, not claimed done here. A **second review pass (2026-07-07)** further hardened the package: copy-law swept (no `proven`/`done` as states), the receipt-counter contradiction fixed (required vs optional axes), the state-machine set completed to **thirteen** with finite retries and no dead-end states (UML §9), the runtime read-only source made explicit (§5), and the store's **transactional law added (§3.1)**. Map of the first 18: 1→header+§14 · 2→§3 · 3→§10 F0a/F0b · 4→§3 ratification law + §6 · 5→§10 order · 6→§10 F0b–F2 gates · 7→§14 carry-ins · 8→§4 badge ban · 9→§4 taxonomy · 10→§5 tags source + F6 read-only · 11→pre-work: fix `toolTypes.ts` TrustEnvelope union (add `unprovable`) · 12→§5 rollup policy · 13→§5 blue row · 14→F13 effects declaration · 15→§7 pins protocol + F17 · 16→F14 contract gate · 17→§13 · 18→§11.
