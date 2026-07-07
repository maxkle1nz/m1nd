# Human View v2 — the screen book

**Every screen and sub-screen, as structural ASCII, in the ARTKIT v2.0.0 design language.**

> Binding: the ARTKIT sheet (palette · typography · block anatomy · proof beads · connection styles) is the
> design system; these wireframes bind STRUCTURE, information placement, states and microcopy. Each screen
> traces to its PRD requirements (F-refs). Copy law applies to every string: no "proven/done/correct" —
> scoped, auditable claims only.
> Tokens referenced by name: `porcelain #F7F4EE · warm-paper #FCFAF6 · graphite #242421 · muted #5F5D57 ·
> hairline #D8D1C6 · socket-blue #3D6F8E · proof-green #5F9E75 · warn-amber #C49A45 · error-clay #B9675D ·
> stale-lilac #8A7BA8`. Type: Inter (UI) · JetBrains Mono (paths, ids, metrics).

Legend used in every wireframe below:

```
(S)[…] socket (left=input, right=output)     ◉ proof ring (state+freshness)    ● bead ok  ◐ bead partial
○ bead empty  ✕ bead failed  ◇ bead stale    ▣ domain tag   ▸ chevron/expand   ⠿ drag handle
━━ dependency (required)   ╌╌ optional   ┄┄ test-coverage   ━✕━ broken   ══ selected route
●─ proof bead ON WIRE (edge evidence)
```

---

## 0. The block — ARTKIT anatomy (reference for every screen)

```
        ┌───────────────────────────────────────────┐
        │ ▣ AUTH                              ☆  ⋯ │  ← domain tag (12/600 caps) · pin · menu
        │ Auth Service                              │  ← block label — Inter 13/600, graphite
        │ repo/auth-service · go · v1.4.2           │  ← mono metadata — JetBrains 12/400, muted
   (S)──┤                                           ├──(S)
   (S)──┤   [Login] [Session] [Users] [Roles]       ├──(S)   ← sub-block chips, per-chip state dot
        │                                           │
        │  C 92%   T 86%   D 4   ! 1        ◉ fresh │  ← metric beads (mono) · proof ring
        │  2026-05-19 10:24                    5m   │  ← timestamp · freshness
        └───────────────────────────────────────────┘
   Card fill: warm-paper · border: hairline (state-tinted) · selected = double hairline + socket-blue accent
   States: default / hover (raised) / selected / disabled / stale (ring ◇ + "stale 2h" chip)
   Fill tint by rollup (PRD §5): green=evidence-backed · amber=needs evidence · red=broken/drifting
   purple=unknown ("not scanned yet" NEUTRAL) · blue=runtime signal (only with ingested spans)
```

---

## 1. BUILD MAP — the front door (F1–F7)

### 1.1 Main state

```
┌────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ m1  ▸ mind-main · main      [Map|List|Proof]   ⌕ Search blocks, receipts…  ⌘K   ◉ Proofs fresh  M │
├──────────┬─────────────────────────────────────────────────────────────────┬───────────────────────┤
│ OVERVIEW │  All systems ▾   [Evidence 4] [Needs 2] [Broken 1] [Unknown 2]  │  AUTH        ▣ AUTH   │
│ ◈ Build  │                  [Planned 1] [Runtime 1]         ⊕ ⊖ 100% ⛶    │  Auth Service         │
│   Map    │                                                                 │  Receipts 5/5 · fresh │
│ ⌥ Tree   │   ┌ ▣ AUTH ────────────┐        ┌ ▣ PAYMENTS ─────────┐        │  ─────────────────────│
│ ⑂ Graph  │   │ Auth Service       │━━●━━━━▶│ Payments API        │        │  HEALTH               │
│ ⎙ Receipts│  │ repo/auth · go     │        │ repo/payments · go  │        │  Receipts   5/5  ●    │
│ ≈ Stress │   │ [Login][Session]   │        │ [Checkout][Billing] │        │  Tests    18/18  ●    │
│ ⚉ Agents │   │ [Users][Roles]     │        │ [Subs][Invoices]    │        │  Structural  ✓ paint  │
│ ⚙ Settings│  │ C92 T86 D4  ◉fresh │        │ C74 T61 D7  ◉ 12m   │        │  Runtime  ▸ no signal │
│──────────│   └────────────────────┘        └─────────────────────┘        │  Freshness   5m  ●    │
│ SYSTEM   │            ┃ ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌●╌╌╌╌┓                     │  ─────────────────────│
│ HEALTH   │   ┌ ▣ EMAILS ──────────┐        ┌ ▣ DATABASE ─────────┐        │  CONNECTED            │
│ ● Evid. 4│   │ Email Service      │        │ Core Data           │        │  Payments   needs ev. │
│ ◐ Needs 2│   │ [Templates][Send]  │◀━━●━━━━│ [Schemas][Models]   │        │  Emails     unknown   │
│ ✕ Broken1│   │ C—  T—  D2  ◉ ?    │        │ [Migrations][Seeds] │        │  Database   runtime   │
│ ? Unkn. 2│   │ purple: not scanned│        │ C91 T88 D3  ◉fresh  │        │  ─────────────────────│
│ ▢ Plan. 1│   └────────────────────┘        └─────────────────────┘        │  RECEIPTS             │
│ ≈ Runt. 1│                                                                 │  ✓ login-flow    2m   │
│──────────│   ┌ ▣ DASHBOARD ───────┐        ┌╌▣ SUBSCRIPTIONS╌╌╌╌┐         │  ✓ session-create 2m  │
│ Unmapped │   │ User Dashboard     │        ╎ Subscriptions      ╎         │  ✓ user-roles    3m   │
│ tray: 12 │   │ [Overview][Widgets]│        ╎ PLANNED · contract ╎         │  View all receipts ↗  │
│ files ▸  │   │ C68 T54 D5  ◉ 40m  │        ╎ 3/5 · not sendable ╎         │  ─────────────────────│
│──────────│   └────────────────────┘        └╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┘         │  [  </>  Show Code  ] │
│ Last scan│                                                                 │  [ ⚉ Ask agent      ] │
│ 2m ago ⟳ │   ┄┄┄┄┄●┄┄┄ test-coverage wire (bead=receipt on edge)          │  blk_auth_01 · v3     │
└──────────┴─────────────────────────────────────────────────────────────────┴───────────────────────┘
```
Annotations: F1 block anatomy · F2 fill=rollup, ring=freshness, border=wired/holes · F3 wires typed with
proof beads ON the edge (●─) · F4 sub-chips with dots · F5 filter chips + System Health + search · F7 the
Unmapped tray is permanent (never pretend full coverage). Right drawer = ARTKIT §12. Layout is stable:
same block → same place across scans (position persisted with the ratified skeleton).
Purple EMAILS shows the honest unknown: beads em-dash (no fake zeros), ring "?", copy "not scanned".

### 1.2 First-run — candidate skeleton, nothing ratified yet (F6)

```
┌────────────────────────────────────────────────────────────────────────────────────┐
│  ◔ Candidate skeleton ready — nothing on this map is ratified yet.                 │
│                                                                                    │
│      ┌╌╌╌╌╌╌╌╌╌╌╌╌┐   ┌╌╌╌╌╌╌╌╌╌╌╌╌┐   ┌╌╌╌╌╌╌╌╌╌╌╌╌┐      all blocks dashed      │
│      ╎ auth? 82%  ╎   ╎ billing?70%╎   ╎ mail? 64%  ╎      + confidence chip      │
│      └╌╌╌╌╌╌╌╌╌╌╌╌┘   └╌╌╌╌╌╌╌╌╌╌╌╌┘   └╌╌╌╌╌╌╌╌╌╌╌╌┘                              │
│                                                                                    │
│   The engine proposed 7 blocks from 342 files (23 unmapped). Names are guesses     │
│   until you ratify them.                                                           │
│                        [ Review & ratify boundaries ]   [ View scan log ]          │
└────────────────────────────────────────────────────────────────────────────────────┘
```
A candidate map is visually distinct (all-dashed + confidence chips) — it can NEVER be mistaken for a
ratified map. Primary action leads to screen 3.

### 1.3 Degraded / empty / loading / error (F6, ARTKIT §14–16)

```
READ-ONLY ATTACH          EMPTY (no repo bound)        LOADING                ERROR
┌──────────────────┐      ┌──────────────────┐      ┌──────────────┐      ┌──────────────────┐
│ ⓘ read-only      │      │  ▦               │      │  ◌ Loading   │      │  ⚠ Failed to     │
│ attach: scanning │      │  No skeleton yet │      │  repository  │      │  load map        │
│ and missions     │      │  for this repo.  │      │  map… 42%    │      │  [ Retry ⟳ ]     │
│ unavailable here │      │ [ Run first scan]│      └──────────────┘      └──────────────────┘
│ (controls render │      └──────────────────┘
│ disabled, never  │      Scan disabled + honest copy when the attach is read-only.
│ dead)            │
└──────────────────┘
```

---

## 2. LIGHT SKELETON — the pipeline (brownfield path, PRD §6)

```
┌────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ LIGHT Skeleton — translate the code graph into a human system skeleton      ◉ Analysis complete    │
│                                                          [ View scan log ]  [ ⟳ Regenerate ]       │
├──────────────────────────┬──────────────────────────────────┬──────────────────────────────────────┤
│ 1 · CODE GRAPH           │ 2 · CANDIDATE SKELETON           │ 3 · SKELETON RECEIPT                 │
│  raw structure           │  proposed blocks — NOT ratified  │  scan sk_2026-07-07_1012            │
│                          │                                  │  duration 00:02:48                   │
│    ∴∵∴   auth 24f        │  ▣ Auth        24f  conf 92% ▸   │  files analyzed        342           │
│   ∵⠿⠿∵   payments 31f    │  ▣ Payments    31f  conf 74% ▸   │  clusters proposed      12           │
│    ∴∵∴   emails 12f      │  ▣ Emails      12f  conf 96% ▸   │  blocks proposed         7           │
│   ∵⠿⠿∵   db 27f          │  ▣ Database    27f  conf 91% ▸   │  files attached   313 (91%)          │
│    ∴∵∴   dashboard 18f   │  ▣ Dashboard   18f  conf 68% ▸   │  unmapped residue  23 (7%)           │
│   ∵⠿⠿∵   … 8 more        │  ▣ Tests       44f  conf 81% ▸   │  multi-owner seams       3           │
│                          │  ▣ Dead Code   23f  conf 55% ▸   │  ──────────────────────────          │
│  total files 342         │                                  │  NAMING                              │
│                          │  naming: naming-runner (fast)    │  proposed by  naming-runner (fast)   │
│                          │  [ ✎ Edit names & boundaries ]   │  cost         1 session              │
│                          │  [ ⠿ drag to merge / split ]     │  every name = a guess until          │
│                          │                                  │  ratified (v0 candidate)             │
├──────────────────────────┴──────────────────────────────────┴──────────────────────────────────────┤
│ PIPELINE  ①Scan repo ●→ ②Cluster purpose ●→ ③Name blocks(agent) ●→ ④Attach files ●→ ⑤Read          │
│           receipts ●→ ⑥Candidate map ● — every stage emits its own receipt line                    │
└─────────────────────────────────────────────────────────────────────────────────────────────────────┘
```
The naming stage is agent work through a runner (PRD §6/§7) and says so on screen — the engine
orchestrating its own analysis is visible, not hidden. Output is always a CANDIDATE (column 2 header).

---

## 3. RATIFICATION — Edit Names & Boundaries (F11) — the screen that makes the map honest

```
┌────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ Ratify skeleton — candidate v0 → ratified v1              7 blocks · 3 seams · 23 unmapped        │
├──────────────────────────────────────────────┬─────────────────────────────────────────────────────┤
│ BLOCKS (edit name · merge · split)           │ SELECTED: auth?  conf 82%                           │
│                                              │                                                     │
│ ▸ ▣ [Auth_________]  24f  conf 92%  ✓ ok     │  purpose (editable)                                 │
│ ▸ ▣ [Payments_____]  31f  conf 74%  ✓ ok     │  [ Handles login, sessions, users and access ]      │
│ ▾ ▣ [auth?________]  18f  conf 82%  ⚠ seam   │                                                     │
│     members:                                 │  BOUNDARY DIFF (proposal vs directories)            │
│       auth/middleware.go        ● certain    │   + auth/middleware.go      (graph pulls it in)     │
│       auth/session.go           ● certain    │   − billing/stripe_hook.go  (graph pushes it out)   │
│       billing/stripe_hook.go    ◐ SEAM ⚠     │                                                     │
│         also claimed by: Payments            │  SEAM: billing/stripe_hook.go belongs to BOTH       │
│       dashboard/auth_widget.tsx ◐ SEAM ⚠     │  Auth and Payments (membership is many-to-many).    │
│ ▸ ▣ [Emails_______]  12f  conf 96%  ✓ ok     │  ( ) keep in both   (•) primary: Payments           │
│                                              │  ( ) primary: Auth                                  │
│ UNMAPPED RESIDUE (23 files) ▸                │                                                     │
│   scripts/…  ci/…  assets/…                  │  [ Split block ] [ Merge into… ] [ Reset proposal ] │
│   [ assign to block ▾ ] [ leave unmapped ]   │                                                     │
├──────────────────────────────────────────────┴─────────────────────────────────────────────────────┤
│  Ratifying signs boundaries as v1 in this brain. Agents and scans will respect them; drift will    │
│  reopen this screen — scoped to what drifted, never a silent re-cluster.                           │
│                                   [ Ratify 7 blocks → v1 ]   [ Ratify selected only ]   [ Later ]  │
└─────────────────────────────────────────────────────────────────────────────────────────────────────┘
```
Sub-state — DRIFT ALERT (reopens scoped):
```
┌──────────────────────────────────────────────────────────────────────┐
│ ⚠ Ratification drift — Auth (v1, ratified 12d ago)                   │
│   3 new files match no block · 1 socket broken (Emails renamed)      │
│   [ Review Auth boundary ]                [ Snooze 7d ]              │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 4. SHOW CODE — the modal (F8–F10)

### 4.1 Files tab

```
┌────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ Build Map ▸ Auth ▸ Show Code                                    [Open in editor ↗][Copy context ⧉] │
│ ┌─[Files]──[Tests]──[Receipts]──[Impact]─────────────────────────────────────────────────────────┐ │
│ │ FILES BY PURPOSE (28)     │  login.page.tsx        react·ts ⧉ │ AUTH HEALTH                    │ │
│ │ ▾ Login screen        ● 3 │  1 import { LoginForm } from '…'  │  Files        12  ●            │ │
│ │   login.page.tsx      ●   │  2 import { redirect } from '…'   │  Tests     18/18  ●            │ │
│ │   LoginForm.tsx           │  4 export default function        │  Receipts    5/5  ●            │ │
│ │   login.schema.ts         │      LoginPage() {                │  Type safety      ●            │ │
│ │ ▸ Session keeper      ● 4 │  5   return (                     │  Lint             ●            │ │
│ │ ▸ Password reset      ◐ 4 │  6   <div className="min-h-…">    │  Build            ●            │ │
│ │ ▸ Protected pages     ● 3 │  9   <LoginForm />                │  Security     ⚠ 1 ▸            │ │
│ │ ▸ Tests              ● 14 │ 13 }                              │  Coverage      87% ▬▬▬▬▬       │ │
│ │                           │ ────────────────────────────────  │  Recent change  2h ago         │ │
│ │ [Show all 28 files]       │ ABOUT — Login page entry. Renders │  Last receipt   2h ago ●       │ │
│ │                           │ the form, redirects after success.│  Change risk   Medium          │ │
│ │                           │ USED BY [Login screen][Protected] │  Blast radius  2 systems ▸     │ │
│ │                           │ IMPACTS [Session keeper][Dashbrd] │  [ ⚉ Ask agent ]               │ │
│ └───────────────────────────┴───────────────────────────────────┴────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────────────────────────────┘
```
Health counters carry auditable denominators (Receipts 5/5 = of the block's declared required contract; optional axes like runtime are shown but never counted).
Security ⚠ expands to the finding — a check, not a vibe. ABOUT is one human sentence per file.

### 4.2 Receipts tab (the taxonomy on screen, PRD §4)

```
│ RECEIPTS — 5 of 5 required · all fresh                       contract: auth.receipts v1 ▸ │
│  ✓ test        login-flow           14 tests green            2m ago   fresh              │
│  ✓ test        session-create        4 tests green            2m ago   fresh              │
│  ✓ structural  x-ray paint          test-exercised/grounded   1h ago   fresh              │
│  ✓ review      webhook-signatures   agent: claude-reviewer    8m ago   fresh              │
│  ✓ spec        business-rules       attached by owner         3d ago   fresh              │
│  ┈ optional    runtime auth-latency no spans yet — NOT required, NOT counted in 5/5        │
│  A required receipt expires when its members change; expired shows ◇ stale and must be     │
│  re-earned. An OPTIONAL axis with no signal is neutral — never counted for or against.     │
```

### 4.3 Impact tab

```
│ IMPACT — change simulation for Auth                                                       │
│  blast radius: 2 systems (Payments, Dashboard) · 14 nodes · truncated? no                 │
│  Auth ━━▶ Payments (session tokens) ●─ receipt on wire: token-contract ✓                  │
│  Auth ━━▶ Dashboard (identity)      ○─ no edge receipt — the wire itself needs evidence   │
│  [ Show causal chains ▸ ]                                                                 │
```

---

## 5. ASK AGENT — packet compose (F12–F13)

```
┌────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ Ask agent · from Auth ▸ Password reset                                              ✕              │
├──────────────────────────────────────────────────────┬─────────────────────────────────────────────┤
│ What should change?                                  │ PACKET PREVIEW (exactly what is sent)       │
│ ┌──────────────────────────────────────────────────┐ │  path        Auth ▸ Password reset          │
│ │ Make password reset send the email and add a     │ │  state       needs evidence (amber)         │
│ │ test.                                            │ │  likely files (6)                           │
│ └──────────────────────────────────────────────────┘ │   forgot.page.tsx · reset.page.tsx          │
│ INCLUDE                                              │   reset.service.ts · email.service.ts +2    │
│  [●] Selected block details    [●] Likely files      │  receipts    4/6 · missing: test, review    │
│  [●] Receipts & evidence state [●] Impact overview   │  impact      touches Emails (1 wire)        │
│  [○] Screenshot of current view                      │  effects     writes a delegate record       │
│ MODE                                                 │              (auditable join for debrief)   │
│  (•) clipboard — paste anywhere (universal)          │─────────────────────────────────────────────│
│  ( ) direct — deliver to agent inbox                 │  [ ⧉ Copy packet (Markdown) ]               │
│  ( ) spawn — launch now via runner…                  │                                             │
│      agent: [ Codex ▾ ]  workspace: isolated ✓       │  clipboard: no side effects                 │
│      policy: propose-only · worktree per mission     │  spawn: creates mission + isolated worktree │
└──────────────────────────────────────────────────────┴─────────────────────────────────────────────┘
```
The packet declares its effects honestly (delegate registry write). Spawn is only offered when the
selected agent's capabilities + workspace truth allow it; policy line is always visible.

### 5.1 Spawn confirmation (the safety gesture)

```
┌──────────────────────────────────────────────────────────────┐
│ Launch mission?                                              │
│  agent      Codex (can edit · can test)                      │
│  workspace  isolated worktree (auto-created)                 │
│  scope      Auth ▸ Password reset (6 likely files)           │
│  applies?   NO — the agent proposes; you land the diff       │
│  audit      delegate record + outcomes ledger                │
│            [ Launch mission ]        [ Back ]                │
└──────────────────────────────────────────────────────────────┘
```

---

## 6. BLOCK RECIPE — planned blocks as contracts (F14)

```
┌────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ New block · Subscriptions                                                    state: PLANNED ▢      │
├───────────────────────────────┬───────────────────────────────────┬────────────────────────────────┤
│ RECIPE                        │        (canvas: dashed block      │ VISUAL CONTRACT                │
│ name    [Subscriptions_____]  │         among real ones)          │ how this block fits            │
│ purpose [Manage customer     ]│                                   │                                │
│         [subscription        ]│   ┌╌▣ SUBSCRIPTIONS╌╌╌╌╌╌┐        │ ● Connect to Auth   connected  │
│         [lifecycle events.   ]│   ╎                      ╎        │   needs: Users, Session        │
│                               │   ╎  define the contract ╎        │ ◐ Read Payments     pending    │
│ EXPECTED INPUTS               │   ╎  before any agent    ╎        │   needs: Payment, Invoice      │
│  User     from Auth     obj   │   ╎  can build it        ╎        │ ◐ Send Emails       pending    │
│  Payment  from Payments obj   │   └╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┘        │ ◐ Declare tests     pending    │
│  Plan     from Pricing  obj   │                                   │ ◐ Attach spec       pending    │
│  [+ add input]                │                                   │   receipt: business-rules      │
│ EXPECTED OUTPUTS              │                                   │────────────────────────────────│
│  Subscription obj · Invoice   │                                   │ contract 1/5 · NOT SENDABLE    │
│  obj · Status event  [+ add]  │                                   │ every socket must be defined   │
│ NEEDED RECEIPTS               │                                   │ before an agent can build this │
│  [business-rules][data-model] │                                   │────────────────────────────────│
│  [api-spec]        [+ add]    │                                   │ [Save blueprint]               │
│ SUGGESTED AGENT               │                                   │ [Send to agent →] (disabled)   │
│  [ subscription-architect ▾ ] │                                   │                                │
└───────────────────────────────┴───────────────────────────────────┴────────────────────────────────┘
```
The gate is mechanical: Send stays disabled until the contract is complete (the oracle's F14 rule —
planned blocks can never become imaginary architecture handed loosely to an agent).

---

## 7. ULM GENERATOR — greenfield path (F16)

```
┌────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ ULM Generator — design the system before the code                                                  │
├─────────────────────────┬────────────────────────────────────────────┬─────────────────────────────┤
│ PROJECT INTENT          │        (blueprint canvas)                  │ BLUEPRINT READINESS         │
│ [Describe the problem…] │        ┌─▣ AUTH ────────┐                  │  Purpose      ● ready       │
│                  0/500  │        │ SignUp/SignIn  │                  │  Flows        ● ready       │
│ AUDIENCE                │        │ Roles · Session│                  │  Data         ◐ in progress │
│ [Primary users…]  0/300 │        └───────┬────────┘                  │  Risks        ◐ in progress │
│ CORE FLOWS              │   ┌─▣ DATA─┐ ┌─▣ PRODUCT CORE─┐ ┌─▣ WORK─┐ │  Agents       ● ready       │
│ [Key user flows…] 0/500 │   │Entities│─│ Value prop     │─│ FLOWS  │ │  Tests        ▢ planned     │
│                         │   │Storage │ │ Capabilities   │ │Journeys│ │  Receipts     ✕ not started │
│ AGENT ROLES (pipeline)  │   └────────┘ │ Metrics        │ └────────┘ │─────────────────────────────│
│ Architect   ●●●○○       │        ┌─▣ UI┴SURFACES─┐ ┌─▣ PROOF GATES┐ │ [ ▦ Generate blueprint ]    │
│ Researcher  ●●◐○○       │        │Screens·Comps  │ │ Assumptions  │ │ [ ↗ Send to Build Map ]     │
│ Builder     ●●○○○       │        │Nav·States     │ │ Criteria     │ │   creates PLANNED blocks    │
│ Reviewer    ●●●●○       │        └───────────────┘ │ Evidence req.│ │   carrying their contracts  │
│ StressTester◐◐○○○       │                          └──────────────┘ │                             │
└─────────────────────────┴────────────────────────────────────────────┴─────────────────────────────┘
```
Proof Gates are a first-class blueprint block (the proof-grown philosophy present at conception).
Send to Build Map emits `Planned` blocks whose contracts arrive pre-filled from the blueprint.

---

## 8. CLIENTS & AGENTS + ROUTING (F15)

```
┌────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ Clients & Agents                                                        [+ Add client or agent]    │
├──────────────────────────────────────────────────────────────────┬─────────────────────────────────┤
│ ┌ Codex ──────────────┐ ┌ Claude Code ─────────┐ ┌ Gemini CLI ─┐ │ ROUTING RULES                   │
│ │ ● connected         │ │ ◐ wrong workspace ⚠  │ │ ● connected │ │  Build      → [ Codex      ▾ ]  │
│ │ ws: mind-main       │ │ ws: other-repo       │ │ ws: mind-…  │ │  Review     → [ Claude Code▾ ]  │
│ │ [edit][test][m1nd]  │ │ [edit][test][m1nd]   │ │ [edit][…]   │ │  Research   → [ Gemini CLI ▾ ]  │
│ │ [packets] ✉         │ │ [packets] ✉          │ │             │ │  Stress     → [ loop-runner▾ ]  │
│ └─────────────────────┘ └──────────────────────┘ └─────────────┘ │  Review     → [ review-run.▾ ]  │
│ ┌ naming-runner ──────┐ ┌ loop-runner ─────────┐ ┌ hand-runner ┐ │  rules route by CAPABILITY;     │
│ │ ● connected         │ │ ● connected          │ │ ● connected │ │  any mission overrides per-send │
│ │ fast cheap model    │ │ gated loop engine    │ │ verified hand│ │─────────────────────────────────│
│ └─────────────────────┘ └──────────────────────┘ └─────────────┘ │ WORKSPACE TRUTH                 │
│                                                                  │  repo  mind-main · bound ✓      │
│  "wrong workspace" = the engine's reception/caller_root state    │  last scan 2m · inbox 3 packets │
│  rendered as UI — the agent will be offered a rebind packet.     │  packet mode  [clipboard|direct │
│                                                                  │                |spawn]          │
└──────────────────────────────────────────────────────────────────┴─────────────────────────────────┘
```

---

## 9. PINS & MISSIONS (F17)

```
PIN STATES (dock on the block edge)                MISSION DRAWER (click a pin)
 ⟳ running     — heartbeat + %                     ┌───────────────────────────────────────┐
 ✉ needs reply — agent asked something             │ Mission m_0142 · Codex · Auth ▸ Reset │
 ◐ output landed (unverified) — output landed, no debrief   │ status ⟳ running 42% · started 2m ago │
 ● debriefed  — touched paths classified    │ workspace wt/auth-reset-m0142 (isolated)│
 ✕ failed      — with the failure line             │ scope 6 files · policy propose-only   │
                                                   │ live: "Creating reset.service test…"  │
┌ ▣ AUTH ────────────┐                             │ [ View diff ] [ Cancel ] [ Open log ] │
│ Auth Service   ⟳42%│  ← pin on block             │ on finish → debrief → outcome ledger  │
│ …                  │                             │ a pin becomes a RECEIPT only if the   │
└────────────────────┘                             │ outcome passes the block's rules      │
                                                   └───────────────────────────────────────┘
APPLY FLOW: [View diff] opens the proposed change · the human lands it (or CI does) ·
debrief classifies touched paths vs scope · out-of-scope touches are flagged loudly.
```

---

## 10. Small components

```
CONTEXT MENU (right-click block)       TOOLTIP (hover)                    LEGEND / ONBOARDING
┌────────────────────┐                ┌─────────────────────────┐   ● evidence-backed — receipts
│ Ask agent          │                │ Auth Service            │     present & fresh
│ Show code          │                │ repo/auth-service       │   ◐ needs evidence — exists,
│ Show impact        │                │ "Reads identity from    │     receipts missing/stale
│ Copy packet        │                │  IdP"                   │   ✕ broken/drifting — failing
│ Add pin            │                │ owner platform · 2h ago │     receipt or ratified-rule hit
│ ─────────────────  │                │ tests 86% · receipts 5/5│   ? unknown — not scanned yet
│ Show dependencies  │                └─────────────────────────┘     (neutral, not a warning)
│ Copy path · id     │                                              ≈ runtime — only with real
│ Mute alerts        │                SEARCH+FILTERS                  spans ("no signal" otherwise)
│ Delete block       │                ⌕ blocks/receipts/files      ▢ planned — a contract, not code
└────────────────────┘                [Domain: Auth ✕][Fresh][Owner…]
```

---

## Screen ↔ requirement traceability

| Screen | PRD refs |
|---|---|
| 1.1–1.3 Build Map + states | F1–F7, §5 rollup, §13 copy law |
| 2 LIGHT Skeleton | §6 brownfield, §7 naming-runner, §3 candidate law |
| 3 Ratification + drift | F11, §3 ratification/versioning/drift |
| 4 Show Code (4 tabs) | F8–F10, §4 taxonomy on screen, edge receipts |
| 5 Ask Agent + spawn confirm | F12–F13, §7 modes/policy |
| 6 Block Recipe | F14 contract gate |
| 7 ULM Generator | F16, §6 greenfield |
| 8 Clients & Routing | F15, §7 routing/workspace truth |
| 9 Pins & Missions | F17, §7 pins protocol |
| 10 Components | ARTKIT §9–12, legend = §5 grammar in operator language |
