# AGENTS.md — working guide for any coding agent on m1nd

Vendor-neutral instructions for autonomous agents (Jules, Codex, Claude Code, Cursor, …)
working on this repository. Read this first; it is the contract.

**m1nd** is a neuro-symbolic code-graph engine in **Rust** (workspace, resolver 2):
`m1nd-core` (in-memory engine) · `m1nd-ingest` (extractors / write side) · `m1nd-mcp`
(the served MCP owner + every verb) · `m1nd-openclaw`. Plus `m1nd-ui` (the served web
UI, Vite/React) and an npm wrapper in `npm/`. Philosophy: agent-first, proof-grown,
local-first, calibrated honesty (`absent`/`abstain`/`insufficient_evidence` are real answers).

This is a **PUBLIC** repository. Everything you commit is published.

## The gates (must pass — these ARE the CI, on ubuntu · macos · windows)

Run these before you consider any change done. CI blocks on all of them across three OSes:

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings   # warnings fail the build
cargo fmt --check
cargo build --release --workspace
```

UI changes (`m1nd-ui/`) additionally:

```bash
cd m1nd-ui && npm ci && npm test && npm run build && npm run lint:soft
```

Always run `cargo fmt` and `cargo clippy --workspace -- -D warnings` before finishing.
If a test flakes under parallel build-cache contention (e.g. `retrobuilder_real`), re-run
it in isolation (`cargo test -p m1nd-core --test retrobuilder_real`) before concluding.

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

## The two write laws — reception governs WRITES (read before any m1nd write)

One owner (`:1338`) hosts many per-project brains and routes each request to the brain that
covers the caller's repo. Two laws keep one agent's work out of another repo's brain. Ignore
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
   (The mechanical write-refusal is landing; the doctrine holds now.)
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
