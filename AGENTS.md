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
