# Tray + Command Center Grounding for `m1nd`

Date: `2026-04-11`

This note answers three questions:

1. What is possible today in the current `m1nd` architecture?
2. What should the `m1nd` tray + command center product look like?
3. Can `GitNexus` serve as a donor for UI/UX patterns?

---

## Short answer

### Yes, a tray-based `m1nd` control surface is realistic

But not as a pure web-page feature.

The clean product shape is:

- **many isolated `m1nd` instances**
- **one global registry**
- **one command center UI**
- **one lightweight native tray shell above that**

### Yes, `GitNexus` is a good UI/UX donor

But only at the level of:

- information architecture
- interaction patterns
- surface choreography

Not as a direct code donor.

The license in their README is:

- `PolyForm Noncommercial`

That makes direct code borrowing a bad fit for `m1nd`'s current open/commercial story.

So:

- **borrow ideas**
- **do not lift implementation**

---

## What is possible today

Grounded in the current `m1nd` codebase:

### Already exists

`m1nd-mcp` already has:

- a local HTTP server
- embedded web UI support
- a shared runtime state
- health endpoints
- graph stats endpoints
- daemon, alerts, document cache, boot memory, and auto-ingest state persisted on disk

Relevant files:

- `m1nd-mcp/src/http_server.rs`
- `m1nd-mcp/src/session.rs`
- `m1nd-mcp/src/server.rs`

### What that means

Today, we can build:

- a **web-based command center**
- instance listing
- status and conflict visibility
- save-state actions
- open/explore actions

without changing the core graph engine.

### What does not exist yet

Today, the codebase does **not** have:

- a native tray app
- a global multi-process instance registry
- runtime-root lease ownership
- cross-instance lifecycle control

So:

- a command center is feasible now
- a tray shell is feasible next
- robust multi-instance control needs one architectural layer first

---

## Current architecture constraints

### 1. Each process owns one `SessionState`

`m1nd-mcp/src/session.rs`

`SessionState` is explicitly documented as:

- one server session state
- one graph + engine bundle
- shared across agent connections

That is good for:

- one process serving many agents

That is not yet enough for:

- many processes coordinated safely

### 2. HTTP UI is process-local

`m1nd-mcp/src/http_server.rs`

The HTTP server wraps one session in:

- `Arc<Mutex<SessionState>>`

Meaning:

- each HTTP UI sees one process
- there is no built-in “all instances everywhere” view yet

### 3. Persistence is already strong

`m1nd-mcp/src/session.rs`

The runtime already persists:

- graph snapshot
- plasticity state
- boot memory
- daemon state
- daemon alerts
- auto-ingest
- document cache
- ingest roots

This is the good news:

> the instance boundary already exists implicitly through `runtime_root`

We mainly need to formalize it.

---

## The right product model

The right mental model is not:

- one giant shared `m1nd`

It is:

- **many `m1nd` instances**
- **one command center**
- **one tray shell**

### Instance

One running process with:

- one graph
- one `runtime_root`
- one `workspace_root`
- one `port`
- one `pid`
- one state lease

### Command center

A global UI that aggregates:

- all running instances
- all conflicts
- all runtime roots
- all saves / alerts / statuses

### Tray shell

A native menubar/tray app that exposes:

- quick status
- quick actions
- launch / attach / open command center

---

## Tray feasibility

### What is realistic right now

There are three implementation paths.

### Option A: tray shell + current HTTP UI

Build a very small native shell whose job is only:

- sit in tray
- show instance count / conflicts
- open command center in browser or native webview
- send restart / save / stop actions

Pros:

- fastest path
- keeps current web UI investment
- easiest to ship

Cons:

- tray is a wrapper, not a full native desktop product

### Option B: tray shell + embedded webview app

Use a desktop shell that embeds the command center directly.

Pros:

- best UX
- feels like a real app
- settings, registry, and instances all live together

Cons:

- bigger packaging/runtime surface

### Option C: native-only tray menus first

Use a tray with nested menus and no rich app window at first.

Pros:

- very fast to prototype

Cons:

- not enough surface for “instances, conflicts, settings, explore”
- will feel cramped almost immediately

### Recommendation

For `m1nd`, the best path is:

1. build the registry
2. build the command center web UI
3. add a thin tray shell that opens and controls it

That gives us the tray experience without designing the whole product inside tray menus.

---

## What the tray should show

At minimum:

- `m1nd` running / not running
- number of active instances
- number of conflicts
- number of alerts

Actions:

- `Open Command Center`
- `Start New Instance`
- `Open Last Project`
- `Save All`
- `Restart Conflicted Instance`
- `Quit`

Optional rich submenu:

- recent projects
- currently running instances
- stale lock warnings

---

## What the command center should show

This is the main product surface.

### Top strip

- running instances
- conflicted instances
- stale locks
- active agent sessions
- unsaved state count

### Main table / card list

Each instance row:

- project/workspace name
- path
- branch/worktree
- status
- port
- pid
- graph nodes/edges
- active sessions
- alerts
- last save

Actions:

- `Open`
- `Explore`
- `Save`
- `Restart`
- `Stop`
- `Delete State`

### Detail drawer

- runtime root
- graph path
- plasticity path
- daemon state
- ingest roots
- cache generation
- last persist time
- document cache status
- auto-ingest status

### Conflict view

Badges:

- `duplicate workspace`
- `shared runtime root`
- `stale lock`
- `port reassigned`

And action suggestions:

- `Attach`
- `Fork Runtime`
- `Reclaim Lock`
- `Stop Other Instance`

---

## GitNexus as donor

## What GitNexus gets right

Grounded in their public repo and web app structure:

- `gitnexus-web/src/App.tsx`
- `gitnexus-web/src/components/Header.tsx`
- `gitnexus-web/src/components/StatusBar.tsx`
- `gitnexus-web/src/components/SettingsPanel.tsx`

The strongest reusable ideas are:

### 1. Header as command surface

Their `Header.tsx` includes:

- active project badge
- repo dropdown
- refresh / delete / re-analyze actions
- global search

For `m1nd`, this maps well to:

- active instance badge
- instance dropdown
- attach / restart / save / reclaim actions
- instance search

### 2. Status bar as always-on operational truth

Their `StatusBar.tsx` shows:

- readiness
- progress
- graph stats

For `m1nd`, this maps well to:

- registry health
- running instances
- active conflicts
- selected instance stats

### 3. Settings panel as a serious side sheet

Their `SettingsPanel.tsx` is not tiny.
It is a proper operational control surface.

That is exactly the right pattern for `m1nd` settings:

- default runtime root strategy
- conflict policy
- auto-attach behavior
- save cadence
- tray preferences
- startup behavior

### 4. Repo-centric IA

GitNexus is organized around:

- project selection
- backend connection
- graph stats
- right-side detail panels

That overall IA adapts extremely well to:

- instance selection
- registry connection/health
- runtime stats
- right-side detail drawer

## What should NOT be copied from GitNexus

### 1. Their exact visual theme

Their theme is nice, but:

- too purple-centric
- too “developer graph app”
- not obviously aligned to `m1nd`’s newer brand direction

We should borrow:

- layout logic
- panel density
- hierarchy

But style it in:

- `m1nd` colors
- `m1nd` typography
- `m1nd` visual identity

### 2. Their product model

GitNexus is:

- repo explorer + graph agent surface

`m1nd` needs:

- multi-instance operational control plane

That is not the same thing.

### 3. Their licensing assumptions

GitNexus README declares:

- `PolyForm Noncommercial`

So direct code adaptation is a poor legal/product fit.

Conclusion:

> GitNexus is a donor of **interaction patterns**, not of copy-paste implementation.

---

## Best adaptation strategy

### Borrow from GitNexus

- top header layout
- dropdown-based instance/project switching
- status bar
- settings side panel
- right-side detail panel model

### Keep from `m1nd`

- existing local HTTP architecture
- `SessionState`
- `runtime_root` persistence
- local-first stance
- command semantics

### Add for `m1nd`

- instance registry
- runtime ownership leases
- conflict classifier
- command center routes
- tray launcher

---

## Proposed visual IA for `m1nd`

### Header

Left:

- `m1nd` mark
- active instance picker

Center:

- workspace / branch / port

Right:

- search instances
- conflicts badge
- settings

### Main split

Left rail:

- instances list

Center:

- selected instance overview

Right:

- actions and detail panels

### Footer/status bar

- registry status
- last heartbeat
- total instances
- selected graph nodes/edges

This is where GitNexus is a strong donor.

---

## My recommendation

### Product recommendation

Yes:

- build the command center
- build the tray shell
- make multi-instance operation a first-class `m1nd` experience

Because if users run many projects at once, this quickly becomes a product-defining layer.

### Technical recommendation

Do it in this order:

1. instance registry
2. runtime-root ownership
3. command center API
4. command center UI
5. tray shell

### GitNexus recommendation

Use GitNexus as a donor for:

- panel layout
- instance selector behavior
- settings drawer behavior
- status bar behavior

Do not use it as:

- a direct code transplant
- a direct design system transplant

---

## Bottom line

What is possible today:

- a strong web-based command center is absolutely possible now
- a tray shell is realistic right after that
- robust conflict-safe multi-instance operation needs one new layer: a registry + lease model

What I think:

This is worth doing.

It upgrades `m1nd` from:

- powerful engine per project

to:

- something a user can actually run across many projects without fear or confusion

And that is exactly the kind of product move that makes the system feel mature.
