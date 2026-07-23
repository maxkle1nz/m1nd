# AGENTS.md — working guide for any coding agent on m1nd

Vendor-neutral instructions for autonomous agents (Jules, Codex, Claude Code, Cursor, …)
working on this repository. Read this first; it is the contract.

**m1nd** is a neuro-symbolic code-graph engine in **Rust** (workspace, resolver 2):
`m1nd-core` (in-memory engine) · `m1nd-ingest` (extractors / write side) · `m1nd-mcp`
(the served MCP owner + every verb) · `m1nd-openclaw`. Plus `m1nd-ui` (the served web
UI, Vite/React) and an npm wrapper in `npm/`. Philosophy: agent-first, proof-grown,
local-first, calibrated honesty (`absent`/`abstain`/`insufficient_evidence` are real answers).

This is a **PUBLIC** repository. Everything you commit is published.

## The gates (must pass — these ARE the CI)

Run these before you consider any change done. The blocking gate is **ubuntu + macOS**:

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings   # warnings fail the build
cargo fmt --check
cargo build --release --workspace
```

**Windows is ADVISORY (phase-2, since 2026-07-23):** the `rust-gates-windows` job runs the
identical gate and reports its own honest check, but it is NOT in the required `Test` aggregator
— a red Windows leg (the ~22 diagnosed source-edit path-canon tests, tracked phase-2 debt) does
**not** block merge. This restored auto-merge (the whole queue was hostage to admin overrides).
Still write cross-platform-correct code (the fs/path contract below is real); when Windows goes
green, re-add `windows-latest` to the matrix and delete the advisory job.

UI changes (`m1nd-ui/`) additionally:

```bash
cd m1nd-ui && npm ci && npm test && npm run build && npm run lint:soft
```

**Cross-platform fs & path contract (Windows is a first-class CI OS):**
- Never `set_len`/truncate on an append-mode handle — on Windows it lacks `FILE_WRITE_DATA`
  (os error 5). Route tail-truncation through `windows_durable_fs::truncate_no_follow`.
- Never hold long-lived lock files opened with `share_mode(0)` — read-only tree snapshots then
  die with sharing violations (os error 32). Share reads (`FILE_SHARE_READ`); write access stays
  unshared so single-owner collision detection is unchanged.
- Never screen operator-supplied paths with `Path::is_absolute` alone — `"/x"`, `"\x"` and
  `"C:\x"` are not absolute under the other OS's semantics. Use the shared helpers
  (`is_safe_relative_discovery_pattern`, `m1nd_ingest::exact_path_identity`) so security screens
  and identity stamps agree on every OS (2026-07-22 incident: a rooted discovery pattern escaped
  the scanned root on Windows).

Always run `cargo fmt` and `cargo clippy --workspace -- -D warnings` before finishing.
If a test flakes under parallel build-cache contention (e.g. `retrobuilder_real`), re-run
it in isolation (`cargo test -p m1nd-core --test retrobuilder_real`) before concluding.

**MCP tool-schema contract:** every advertised tool's `inputSchema` MUST declare a top-level
`"type": "object"` (MCP spec). Strict clients (Claude Code) reject the ENTIRE `tools/list`
when a single tool violates it — the 2026-07-22 incident (a bare top-level `oneOf` on
`mission_service`/`external_mutation_service`) silently unregistered all 48 tools from live
sessions. The registry-wide regression test `every_tool_input_schema_is_top_level_object`
(`m1nd-mcp/src/server.rs`) enforces this; never weaken it to land a schema.

## Git identity — ABSOLUTE

- Author every commit as **`Max Kle1nz <kleinz@cosmophonix.com>`**. Never as a bot, never as
  the agent, never as "Claude" or any `noreply@…` address. No `Co-Authored-By` bot trailers.
- Commit subjects state **public intent** (what/why), not process. No AI-tell language, no
  unverifiable claims, no marketing superlatives. Conventional Commits (`fix:`, `feat:`,
  `docs:`, `chore:`) — the changelog is generated from them.
- Commit messages in **English** (this repo).
- If your platform sets a bot PR author it cannot override, say so in the PR body so the
  maintainer can reconcile authorship before merge.

## No-leak — reputation rule (public repo)

Never write, in code / tests / docs / commit messages / PR bodies:
- Personal filesystem paths (`/Users/<name>/…`, `/private/tmp/…`, home-dir absolutes).
- Other project or client names, or personal machine/service labels.
- Runtime secrets, tokens, or internal development scaffolding.

In tests and fixtures use **neutral names** (`repo-alpha`, `project-b`, `com.example.*`,
`tempfile` tmpdirs). Never touch the maintainer's live runtime at `~/.m1nd` or the served
owner on port `1338` — all tests use temp dirs.

## How work lands — bursts, not PR-per-fix

- **Local commits are cheap and atomic** (one per proven logical unit) — they do NOT trigger CI.
- **The expensive round is push → PR → CI → merge.** Accumulate local commits for one
  theme/session and land **one PR** that covers the batch — CI runs once per burst, not per fix.
- For **async cloud agents** (Jules) each task becomes its own PR: scope tasks to
  **independent, self-contained units** (a bug fix, a hygiene pass, a doc), not to pieces of a
  themed batch that wants accumulation.
- **Async-agent close-out (the platform-bot rule):** platform-authored bot commits are never
  merged to main. The maintainer loop closes the work: cherry-pick the agent's diff re-authored
  as the maintainer, complete any gate the agent missed (docs coupling above all), then land.
  State the groundwork's provenance honestly in the commit body — never claim an authorship
  the platform did not produce.
- The universal **documentation gate**: a behaviour/API/architecture change updates the repo's
  `docs/`, wiki, `README`, and `docs/PATHOS.md` **in the same PR** — a feature is not done until
  the docs reflect it.

## Where the real truth lives (read before non-trivial work)

- **`docs/PATHOS.md`** — the canonical handoff: north star, current state, doctrine, next moves.
  Read this first.
- **`docs/UML-ORGANISM.md`** + `docs/uml/` — the structural atlas: every system as
  code-grounded UML, plus a ranked ledger of known open gaps.
- **`docs/ORGANISM-PRD.md`** — the constitution (the spine, the four grammars, the build ladder).
- **`CLAUDE.md`** — the repo's canonical build/gate/automation notes (also read by Claude Code).

## Dogfood m1nd — for LOCAL agents only

If you can reach the served m1nd owner (a local process on `127.0.0.1:1338`), orient with
`north(task)` before editing and `memorize` durable findings after — ground yourself in the
graph, don't start cold. **Cloud VMs (e.g. Jules) cannot reach a local owner**, so skip this
unless m1nd is served at a reachable address.

Every agent is a sensor: if m1nd misbehaves during a mission, append one JSON line to the
field-report spool (see `CLAUDE.md`) — report, never fix mid-mission.

## Wear the wire — the cards, the voice, the presences (when m1nd is served)

When you can reach the served owner, the organism should SEE the work, SPEAK to the human, and
SHOW the team. All three rails are advisory — none is a gate, none lands truth (only a human
`receipt_import` colors the map).

**The cards (WEAR THE WIRE).** When you ORCHESTRATE a burst — dispatching ≥2 executors, or
landing a BIG change — open a mission-control card so the work is on the board, not off-book.
The grammar (learn it once, don't spend four tries on it):

- `mission_start {agent_id, repo, task, mode, budget, risk}` opens the card. The enums are
  CLOSED — an off-list value is refused with the allowed set: `mode ∈ bug_hunt | review |
  refactor | docs_drift | architecture | release`, `budget ∈ short | normal | deep`,
  `risk ∈ low | medium | high`.
- Progress = **`mission_event`**. NOT `mission_post` — that is a DIFFERENT rail (the
  mission-LETTER board, `m1nd-mission-letter-v0`, whose `landed` is a human `receipt_import`
  and whose terminal `archived` is the human's set-aside of a superseded receipt, F2.5e),
  not mission progress. The two share the word "card" but not the verb, the id space, or the store.
- Close = **`mission_close`** — a seat gesture, its only door is `ensure_agent` (no owner sign-off;
  landing a receipt stays human-only).
- A card is **SINGLE-WRITER**: `ensure_agent` refuses any caller whose `agent_id` ≠ the card's own
  (`mission <id> belongs to agent_id <A>; got <B>`). So you post only to a card you opened under
  YOUR `agent_id` — the orchestrator holds the umbrella burst card (executors report back to it,
  they never post into it); an executor that runs its own scoped mission opens its OWN card under
  its OWN id. Negative default: a card is for a real burst, never a trivial one-file touch. The
  card is a TRAIL, never a gate.
- Over REST, always pass an explicit `?brain=<root>` selector.

**The voice (render law → M1ND_INSTRUCTIONS §7).** `north` carries `human_view`
(`m1nd-human-view-v0`): a server-composed ≤4-line card — the m1nd voice for the HUMAN in the
conversation, its `pulse` row (`trust · graph · focus · bell · coherence`) the served brain's own
vital signs, plus a `map <N> blocks` fact. You RENDER it verbatim under the negative-default
cadence in **§7 (`## 7. THE M1ND VOICE`) of the initialize instructions** — never re-compose it,
never invent a statistic; under `caller_root_mismatch` the card IS the warning and the pulse drops
whole. The human's on-request navigable menu is the read-only **`cockpit`** verb (a sibling of
`north`). §7 is the single source for the render law — this file points at it, it does not restate it.

**The presences (the control room sees the team).** All traffic becomes a visible presence:
registration rides the verbs you already call (the `track_agent` beat, TTL by last-seen —
*presence = activity visible to m1nd*); a dead session disappears rather than lingering as a ghost.
A **collision** is derived at read, never stored: two live presences on the **same brain**, **both
carrying a mutation signal**, whose declared working sets **overlap** → an advisory line on BOTH
sessions' `north` packets. It warns; it never blocks. **When you see a collision, STOP and
coordinate — do not force the write** (the same posture as reception). Contract: `m1nd-presence-v0`
(`docs/ORGANISM-INSIDE-PRD.md` §3.3, `m1nd-mcp/src/presence.rs`); the Hall renders the live roster (`m1nd-ui`).

**The Universe (the human's L0 panorama).** The served UI's landing surface for an owner with ≥1
project brain is the **Universe** — a per-world panorama fed by one read-only aggregate,
`GET /api/universe` (`m1nd-universe-v0`). It is **sidecar-only**: it reads project-brain manifests,
the presence dir, each world's mission-letter box + SystemBlock store, and the owner's own daemon
alerts — and it NEVER hydrates a brain (an executable RED-first law: the warm map is byte-identical
before/after). Its unified gesture queue, **the Landing**, aggregates reads (merge_wait stamps,
candidate ratifies, owner alerts) but every write still goes through the existing per-type verb.
Not an agent surface — agents keep using `north`/`seek`/the cards. Contract: `docs/HUMAN-VIEW-V2-F30-UNIVERSE.md`.

## The write laws — reception governs WRITES (read before any m1nd write)

One owner (`:1338`) hosts many per-project brains and routes each request to the brain that
covers the caller's repo. Three laws keep one agent's work out of another repo's brain. Ignore
them and you corrupt shared state — a real incident: a foreign skeleton once overwrote a
bound brain because the writer never checked which brain it was talking to.

1. **No WRITE under a reception mismatch.** A m1nd response may carry a `reception` block;
   `reception.match == "caller_root_mismatch"` means the brain currently serving you does
   NOT cover your repo. A read under mismatch is a warning (don't trust retrieval for this
   repo). A **write** under mismatch is prohibited by doctrine — every write verb
   (`memorize`, `skeleton_candidate`, `candidate_edit`, `system_blocks_seed_import` /
   `_ratify` / `_reconcile`, `mission_post`) would land in the WRONG brain. The one correct
   gesture BEFORE any write: `ingest project_root=<your repo root>` — a single call
   creates/resolves YOUR brain, ingests it, binds the session, and returns its north in the
   same response; from then on your root routes to your brain automatically. Then write.
   (The mechanical write-refusal has LANDED — every skeleton write verb
   (`skeleton_candidate`, `candidate_edit`, `system_blocks_seed_import`/`_ratify`/`_reconcile`/
   `_archive`/`_delete`, `candidate_lease` acquire) refuses under mismatch with a teaching
   `brainless_root` that names both roots; the doctrine holds regardless.)
2. **No twin brains.** Minting a brain for a root that is the PARENT, CHILD, or WORKTREE of
   an existing brain is refused with a teaching error (`overlap_parent` / `overlap_child` /
   `overlap_worktree`) that names the conflict and the two ways forward: bind to the existing
   brain (`ingest project_root=<existing>`), or pass `allow_overlap:true` only when you know
   exactly why. It holds on BOTH doors — the MCP wire and REST `POST /api/tools/ingest` route
   through one guarded core. A burst worktree does NOT get its own brain; bind to the main
   repo's. This stops one repo growing two brains (double ingest cost, memories fragmented).
3. **Memory writes never move your code root.** `memorize` (and any agent-memory ingest
   merge) can no longer demote a brain's `workspace_root` onto its own memory-store dir —
   the write path guards it (the #326 family, third member), and a brain found already
   flipped self-heals at the boot/load seam with an honest log line
   (`healed workspace_root: <from> -> <to>`). If you ever see a bare-REST
   `caller_root_mismatch` naming an `agent-memory` dir as the bound workspace, that is this
   disease on a pre-fix binary — rebuild/restart heals it; never hand-edit the manifest.

Editing the block map (the skeleton) is one atomic verb, `candidate_edit`, and it refuses on
a ratified skeleton (candidate-only). **Ratifying a skeleton is a human-only gesture — no
agent ratifies, ever.** The hand proposes; the human signs.

**The same human gate now guards `receipt_import`.** Landing a receipt (the OTHER human write
that bumps `store_version`) requires an `imported_via` origin token, validated server-side
against a CLOSED allow-list (today only `"human-ui"`, the value the owner's screen stamps). An
absent, empty, or off-list origin is refused `human_gesture_required` and nothing is applied.
Stated plainly: until the sovereign-stamp arc's step 0, `receipt_import` carried **no origin
check at all** — only `ratify` did — so an agent could land evidence by simply calling the verb.
Both human writes now carry the mirror. A new origin is a code change plus a test here, never a
trusted client string; and, like ratify, the token is forgeable on an unauthenticated loopback,
so it closes the cheap reflex vector, not a same-UID process (cryptographic elevation is a later
step of the arc).

**F2.5e adds the THIRD human write: archiving a superseded receipt.** The mission-LETTER board
gained a terminal `archived` phase — the human sets a stale `merge_wait` receipt aside (posting a
seq+1 `archived` letter that supersedes the head), which drops the landing bell. Because this is
the board's first SILENT-burial verb (`failed` at least pins loud atop the tray, an `archived`
head is quiet), `mission_post` of an `archived` letter requires the SAME human origin token —
`archived_via:"human-ui"`, the closed allow-list, refused `human_gesture_required` with nothing
appended. Two engine laws back it on the pure post path (both seams): an `archived` letter may
never carry `receipt.imported==true` (never a landing in disguise), and it may only supersede a
`merge_wait` head — the board's FIRST transition rule (`invalid_transition` otherwise). **An agent
never archives; the owner's screen does** — an agent must never silence its own unproven work. Full
rule: `HUMAN-VIEW-V2-F25-TECH` §1h + § archived.

**Two more write-door guards (2026-07-15 field hardening).** (1) A `mission_post` letter carrying a
`receipt_candidate` is now checked against the LIVE block at post time: a candidate whose
`scope.boundary_version` no longer matches the block is refused `stale_scope` naming both versions —
dead evidence is declared at the door instead of surviving as an orphan letter the tray would offer
for a one-click import that `receipt_import` then rejects. (2) The owner's graph persist gained a
catastrophic-shrink guard: when an incoming graph holds under 20% of the nodes in the existing
on-disk snapshot, the prior snapshot is renamed to `<path>.bak-<unix_ts>` before it is overwritten
(fail-open — a legitimate shrink is never blocked, the large snapshot is never lost in silence).
This is defense in depth behind the #370 binding fix, after a snapshot was overwritten 10573→704.
