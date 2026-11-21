# AGENTS.md — working guide for any coding agent on m1nd

Vendor-neutral instructions for autonomous agents (Jules, Codex, Claude Code, Cursor, …)
working on this repository. Read this first; it is the contract.

**m1nd** is a neuro-symbolic code-graph engine in **Rust** (workspace, resolver 2, five
members): `m1nd-core` (in-memory engine) · `m1nd-ingest` (extractors / write side) ·
`m1nd-mcp` (the served MCP owner + every verb) · `m1nd-control` · `m1nd-runnerd` (the
write/execution lane — the only spawner). Outside the Rust workspace:
`m1nd-ui` (the served web UI, Vite/React), `m1nd-demo`, and an npm wrapper in `npm/`. Philosophy: agent-first, proof-grown,
local-first, calibrated honesty (`absent`/`abstain`/`insufficient_evidence` are real answers).

This is a **PUBLIC** repository. Everything you commit is published.

## The gates — verified change scope, then full integration proof

For each local edit, run the exact affected regression test first. Before
sharing a PR, run the smallest real proof for the behavior; Rust changes also
run `bash scripts/lightning_check.sh`. CI independently derives a strict v1
scope from the PR event's base/head SHAs and `git diff --name-status -z -M` at
the checked-out candidate. Renames include both paths; deletion, empty,
malformed, unavailable or unknown diffs select full coverage.

Known PR changes run only their affected area gates. Scoped Rust runs LIGHTNING
plus affected crates and their explicit dependents on Linux and Windows. UI,
host/npm, native cache and Python gates run only when their flag is selected.
Source guard, complete-history gitleaks and frozen-contract/docs coupling always
run. Cargo audits run for dependency/full coverage; UI audit and dependency
review run for UI dependency changes or full coverage. `Test` passes only when
every selected job succeeds and every unselected job is truly skipped; its pass
line names the scope and candidate SHA. A draft may receive that scoped result,
but GitHub still forbids merging a draft.

`main`, merge-group candidates, nightly and manual dispatch always select the
three-OS full Rust proof. Whole CI/config/Cargo/build-script/security-policy
changes also select full. Retrobuilder source/test paths select that same full
lane: real-repository ingestion exceeded the scoped nextest job budget. See
`docs/TEST-PORTFOLIO.md` for the path mapping and the preserved history of the
previous draft/ready policy.

For final review, release, or a full-selected change, run these before claiming
local full proof. The CI full gate covers **ubuntu + macOS + Windows**:

```bash
cargo nextest run --workspace --all-targets             # the LOCAL canonical runner (one process per test)
cargo test --workspace --doc                            # the sentinels nextest never runs
cargo clippy --workspace --all-targets -- -D warnings   # warnings fail the build
cargo fmt --check
```

nextest is the canonical runner ON YOUR MACHINE (adopted 2026-08-02, measured:
same suite 641–935s under `cargo test`, 377s under nextest, 1643/1643 —
including the two deadlock-timeout tests that expire under `cargo test`'s
shared-process load). Retries are 0 by policy — a flake goes red. Plain
`cargo test` still works when you need `--nocapture` on one test.

CI's final required legs still run `cargo test`, deliberately: the same measurement
taken ON THE GATE said no — ubuntu went 62→83 min under nextest (the
nested-Cargo process storm on the weakest-I/O runner) while windows improved
70→48. The observational `nextest-shadow` runs only on explicit manual
dispatch; required legs change only after controlled runner-side comparisons
reproduce the same greens and reds at an acceptable wall. Local green under
nextest + final gate green under
`cargo test` are BOTH required truths until then — do not "fix" one by
trusting the other.

The second line is not a duplicate of the first: nextest runs **no doctests at all** (and
the old `cargo test --all-targets` excluded them too), and every doctest in this workspace
is a `compile_fail` sentinel guarding the
candidate boundary — 13 of them, all in `m1nd-mcp`, each pinning a symbol that must not be
reachable from outside its crate. Until 2026-07-30 nothing in CI executed them, which is how
two rotted silently through the transplant era and surfaced in a release audit rather than a
PR (#505). If you widen a visibility, this is the line that tells you. It costs ~10s on top of
the run above (0.12s of it is the tests; the rest is rustdoc over artifacts already built).

There is deliberately no standalone `cargo check` (clippy type-checks every target on its way
to linting — a separate pass was a redundant full compile), and the `--release` workspace
build runs on **main pushes only**, not on PRs (2026-07-24: it proved nothing tests+clippy
don't and cost ~half of every 70-minute CI round; the signed release pipeline rebuilds
`--release` at tag time regardless). A release-profile-only breakage is caught on the main
push — if you suspect one, run `cargo build --release --workspace` locally before merging.

**Windows is REQUIRED again (since 2026-07-29):** `windows-latest` runs in the full
matrix with ubuntu/macos and in scoped Rust with Linux; its red blocks the selected
proof. It spent 2026-07-23→29 as an
advisory job while the phase-2 debt was diagnosed and paid (#435–#440, #444); the flip back was
made against a fully green advisory run on main, and the scaffold is deleted. The fs/path
contract below is load-bearing — the whole debt family was path identity and cfg-gated code the
Unix legs are structurally blind to, so a green local run on macOS proves nothing about your
`#[cfg(windows)]` branches: mirror-probe them (flip `cfg(unix)`↔`cfg(windows)` locally) when
you touch one.

UI changes (`m1nd-ui/`) additionally:

```bash
cd m1nd-ui && npm ci && npm test && npm run lint && npm run lint:soft && npm run build
```

`npm run lint` is eslint; `lint:soft` is the semantic/icon pair. Both are CI steps as of the
ESLint 10 migration — eslint was absent from `ui-gates` until then, which is exactly how it sat
broken under green PRs (an override pinned `brace-expansion@5` beneath the CJS `minimatch@3` that
eslint 9 pulled, and every run died with `TypeError: expand is not a function`). Warnings do not
fail the gate; errors do.

**`m1nd-ui/dist` is tracked on purpose** (`.gitignore:20` — rust-embed compiles it into every
`m1nd-mcp` binary), so the build output above is *part of your change*: commit it. `ui-gates`
rebuilds from a clean checkout and fails if `dist` comes out different. That gate exists because
nothing else could see the drift: the build digest, the runtime digest and the `ui_bundle`
authority all hash the same committed tree, so a bundle five commits behind its own source still
reported COHERENT (2026-07-29, mailbox letter `84fde5e4da2e`). The build is byte-reproducible —
three consecutive host builds and a `linux/amd64` `node:22` container emit an identical tree — so
a red there is drift, not nondeterminism.

Touching the shell chrome (`m1nd-ui/src/App.tsx`, navigation, landmarks, any control's label) also runs
the two browser lanes, which CI keeps **separate on purpose** — G7 counts them one by one:

```bash
cd m1nd-ui && npm run test:e2e && npm run test:e2e:a11y
```

`test:e2e` is the fixture suite (`e2e/`); `test:e2e:a11y` is the accessibility smoke
(`e2e-a11y/`, own config, own CI step) — landmarks, accessible names, the `aria-current="page"`
door, keyboard reach. Never fold a new accessibility assertion into `e2e/`: merged into the
fixture suite it stops being a separate proof and closes nothing
(`docs/benchmarks/G7-LIVE-CEREMONY.md` §5).

**Cross-platform fs & path contract (Windows is a first-class CI OS):**
- Never `set_len`/truncate on an append-mode handle — on Windows it lacks `FILE_WRITE_DATA`
  (os error 5). Route tail-truncation through `windows_durable_fs::truncate_no_follow`.
- Never hold long-lived lock files opened with `share_mode(0)` — read-only tree snapshots then
  die with sharing violations (os error 32). Share reads (`FILE_SHARE_READ`); write access stays
  unshared so single-owner collision detection is unchanged.
- Never rename or remove a directory tree while something inside it is still open. Unix moves a
  tree out from under live handles; Windows refuses with the same os error 32. A live brain holds
  a checkpoint-store directory handle, writer lock and leases under its own store dir, so quiesce
  it first (`ProjectBrainRegistry::shutdown` — pause, checkpoint+ACK, stop the actor, release the
  instance, drop the cell) and move the bytes only once nobody holds them. A live `ReadDir` counts
  too: scope it so its handle is closed before the removal.
- Never screen operator-supplied paths with `Path::is_absolute` alone — `"/x"`, `"\x"` and
  `"C:\x"` are not absolute under the other OS's semantics. Use the shared helpers
  (`is_safe_relative_discovery_pattern`, `m1nd_ingest::exact_path_identity`) so security screens
  and identity stamps agree on every OS (2026-07-22 incident: a rooted discovery pattern escaped
  the scanned root on Windows).

Always run `cargo fmt` and `cargo clippy --workspace -- -D warnings` before finishing.
If a test flakes under parallel build-cache contention (e.g. `retrobuilder_real`), re-run
it in isolation (`cargo test -p m1nd-core --test retrobuilder_real`) before concluding.

**Brain-actor hand-off is a signal, never a poll.** A brain session's single-writer fence
(`actor_active`) is lowered by the outgoing owner's detached shutdown thread. The bound bind waits
on `BrainSessionCell::wait_for_actor_release` rather than polling or retrying — a refusal from the
bound bind is cached in `bound_runtime` for the life of the registry, so never answer `brain session
already has an active actor owner` with sleeps, retry budgets, or `#[ignore]`.

**A gate is evidence only about the tree it ran in.** Cargo's metadata hash does not encode the
source path, so parallel checkouts sharing one `CARGO_TARGET_DIR` can link each other's binaries.
Before running gates anywhere parallel checkouts exist, take this checkout's own target directory:

```bash
export CARGO_TARGET_DIR="$(scripts/cargo_target_dir.sh)"   # deterministic, per-checkout
```

## Runtime surface invariants

- **MCP tool-schema contract:** every advertised tool's `inputSchema` MUST declare a top-level
  `"type": "object"` (MCP spec). Strict clients reject the entire `tools/list` when one tool omits
  it. `every_tool_input_schema_is_top_level_object` (`m1nd-mcp/src/server.rs`) enforces this; never
  weaken it.
- **The menu is not the registry:** `tools/list` serves a **15-verb core menu** — `CORE_TOOLS` (12:
  `north`, `memorize`, `boot_memory`, `ingest`, `seek`, `search`, `view`, `surgical_context`,
  `impact`, `health`, `trust_selftest`, `session_handshake`) ∪ `HOST_BINDING_REQUIRED_TOOLS`
  (adding `recovery_playbook`, `doctor`, `help`) — out of **147 registered tools** (`all_tool_schemas`
  / `MCP_TOOL_ROUTE_NAMES`).
  - The filter lives at one seam (`tool_schemas_for_tier`). Registry parity and `POLICY-DISABLED`
    annotations apply to `all_tool_schemas` before filtering.
  - `core_menu_tool_names()` is derived, never a second hand-typed list. Never shrink
    `HOST_BINDING_REQUIRED_TOOLS` to pass a menu test; `const _: () = assert!(CORE_MENU_MAX < 30)`
    pins the menu bound.
  - `help` catalogs the full 147-verb registry at every tier (`help_guidance::schema_tools` reads
    `all_tool_schemas`). Battery: `server::tests::core_menu_*` and
    `help_sees_every_verb_regardless_of_tier`.
- **The session card — pull, never push:** every session pays for `M1ND_INSTRUCTIONS` and the
  default `tools/list` before its first call: ≤ 10,000 characters together (about 2,500 tokens).
  The instructions stay an operating card (≤ 3,000 characters) and the menu serves
  `CORE_MENU_CARDS` with prose-free input schemas that name each verb's required parameters and
  its `CORE_MENU_PARAMS` (≤ 7,000 characters, ≤ 1,200 per tool), pinned on the real binary by
  `m1nd-mcp/tests/session_card_budget.rs`. New doctrine goes into a `help {topic}` card
  (`m1nd-mcp/src/help_topics.rs`) and new parameters and their prose into the registry, which
  `help(tool_name)` serves; never back into the push surface.
- **Answers are lean by default:** `north` answers a one-word `state` and one `path:lines symbol`
  line per focus node (≤ 1,500 bytes), and `search` answers lean rows packed under 4 KB with the
  cut declared. `detail: true` returns the full answer, and an abnormal `north` (mismatch,
  `needs_ingest`, degraded trust, `graph_drift`, a collision) returns it unasked. A caller that
  reads a full-packet field (`binding`, `context`, `honest_gaps`, `human_view`, `code`) passes
  `detail: true`, as the UI and the north shim do.
- **REST route seating:** `POST /api/tools/{tool}` runs the generic floor gate
  (`enforce_generic_action_policy`) before generic dispatch. Every verb with custom interception in
  `handle_tool_call` (owner→daemon proxies `mission_spawn`, `candidate_naming`, `curation_spawn` and
  typed G3 facades) must be declared in `REST_ROUTE_SEATING` with probes in `OWNER_PROXY_PROBES`
  (`m1nd-mcp/src/http_server.rs`), enforced by
  `rest_route_verb_seating_is_exhaustive_on_both_sides_of_the_floor_gate`.

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

- **Local commits are cheap and atomic** (one per proven logical unit) — they do not trigger CI.
  Accumulate commits for one theme/session into **one PR**.
- **Async cloud agents** (e.g. Jules) should take independent, self-contained units. Platform-authored
  bot commits are never merged to `main`; the maintainer cherry-picks and re-authors them, stating
  provenance honestly in the commit body.
- **Squash is the default merge strategy** (one PR = one conventional commit on `main`). A train
  (see "Develop m1nd with m1nd") keeps its per-change commits instead. Merge commits are reserved
  for ceremonies whose commit lineage is itself the artifact. Never arm
  auto-merge on an unreviewed PR.
- **Documentation coupling:** behavior, API, or architecture changes update `docs/` (including
  `docs/wiki/`), `README.md`, `CONTRIBUTING.md`, or `skills/` in the same PR (enforced by
  `scripts/agent_docs_gate.py`).

## Where the real truth lives

- **`docs/PATHOS.md`** — canonical handoff: north star, current state, doctrine, next moves.
- **`docs/uml/`** — code-grounded architecture sheets and open gaps.
- **`CONTRIBUTING.md`** and **`docs/TEST-PORTFOLIO.md`** — build commands, feature flags, and CI
  scope mapping.

## The G6 blind benchmark is the owner's, not yours

The G6 knowledge-quality ceremony (`scripts/benchmark/g6_formal_run.sh`,
`docs/benchmarks/G6-FORMAL-CEREMONY.md`) is **owner-only**. Agents may stage it, verify it, and run
`--dry-run` or `scripts/benchmark/g6_formal_preflight.sh` (public artifacts only):

- **Never open `docs/benchmarks/**/operator-only/` or `**/runner-results/`.** Hashing a sealed file
  against its pinned digest is fine; parsing it is not.
- **Never simulate the ceremony.** An unrun ceremony is `NOT_RUN`; `NOT_PROVEN` is a valid outcome.
- **Never re-run to chase green.** The metric spec allows one sealed run per revision
  (`one_sealed_run_only_no_rerun_until_pass`); a `FAIL` stays in the record.

## The custody ceremony — the one surface no agent may run

`m1nd-mcp --custody-ceremony <verb>` drives the G9 Secure Enclave custody ceremony
(`docs/benchmarks/G9-CUSTODY-CEREMONY.md`). Of its five verbs, agents may run **only `preflight`**.

- **`provision-seats`, `owner-seat`, `seal`, and `assemble` are owner-only.** No agent may run,
  simulate, stub, mock, or dry-run them, provision any enclave key, touch biometrics, or synthesize
  a `custody-ceremony.sealed.json` or seat public key. Offer the command and stop.
- The ceremony is CLI-only (no MCP tool or REST route), refuses unattended invocations before
  platform checks, and enforces seat-class separation in `m1nd-mcp/tests/custody_ceremony_wiring.rs`.
  Unentitled local builds refuse at keychain prerequisite P4 — never work around that refusal.
- The entitled macOS release bundle (`m1nd-custody-ceremony.app`, see `build/README.md`) is also
  owner-only; agents never build, sign, or handle its provisioning profile.
- **Exception:** `--seal-independence-spec <PATH>` is a document-only digest command (opens no
  enclave, keychain, or ceremony state) and is safe on every platform.

## Dogfood m1nd and the field-report box

If a local served owner is reachable, orient with `north(task)` before editing and `memorize`
durable findings after. Cloud VMs cannot reach a local owner — skip this there.

- **Per-verb telemetry (`m1nd-mcp/src/verb_usage.rs`):** `<runtime>/verb_usage_state.json` records
  only verb name, three counters (`answered`, `refused_at_authority_floor`, `refused_at_dispatch`),
  and first/last seen. Unrouted names collapse into `<unrouted>`; caller text (`agent_id`,
  arguments, queries, paths) must never reach the file
  (`verb_usage_persisted_shape_holds_no_free_text`).
- **Out-of-scope defects go in the box, and every task ends with one letter:** do not fix
  unrelated defects mid-mission and do not swallow them. Append one JSON line to the spool at
  `~/.m1nd/field-reports.jsonl` (`m1nd-mcp/src/mailbox.rs`), routed by `repo` into
  `<repo>/.m1nd/inbox.jsonl`. Every task, defect or not, closes with one letter: `class`
  `win` | `friction` | `bug` | `honesty`, the `tool`, `what` you asked and what came back, and what
  you `expected`. Never secrets, tokens, or credentials. Read boxes via `GET /api/mailbox?brain=<root>` or
  `m1nd-mcp --inbox-sweep` (CLI/REST only, never MCP). Answering and closing letters is part of
  owning a repo; committing `<repo>/.m1nd/inbox.jsonl` in a public repo is a publishing decision.
  Carry this duty verbatim in subagent specs.

## Develop m1nd with m1nd

The fastest way to learn what agents need from m1nd is to be one. When a served owner or a
launcher-granted runtime covers your checkout:

- Orient with `north(task)`, find code with `seek`, and ask `impact` (`direction: "reverse"`, on a
  function, never a whole file) before you change a seam. Heed `graph_drift`: run its `refresh`
  call or read lines as of `indexed`. Open every span before you trust it.
- Before you call a change done, read `impact`'s `co_change` for every file you touched and answer
  for each partner outside your diff: the manifest, skill, or doc that usually moves with it.
- Say what helped and what did not in the letter that ends the task (above). The evidence behind
  each practice is in `docs/AGENT-BEST-PRACTICES.md`.

**Changes ship in trains.** Each change gets its own branch: failing tests first, then its local
gates and its proof (a bracketed regression run, a measured cost). Proven changes stack onto one
train branch, which gets one full local gate run, one PR, and one hosted CI run, and merges keeping
its per-change commits, so each change stays one reviewable conventional commit on `main`.

## Wear the wire (when m1nd is served)

All three rails are advisory — none is a gate and none lands truth (only human `receipt_import`
colors the map):

- **Mission cards:** for multi-executor bursts or large changes, open a card with
  `mission_start {agent_id, repo, task, mode, budget, risk}` (`mode ∈ bug_hunt | review | refactor |
  docs_drift | architecture | release`, `budget ∈ short | normal | deep`, `risk ∈ low | medium |
  high`), post progress with **`mission_event`** (not `mission_post`, which is the separate
  mission-letter board), and close with `mission_close`. Cards are single-writer (`ensure_agent`
  enforces matching `agent_id`). Over REST, pass `?brain=<root>`.
- **The voice:** render the `human_view` card of the full `north` packet verbatim under the cadence in
  `M1ND_INSTRUCTIONS` (full card law: `help {topic: "voice"}`) — never re-compose it or invent
  statistics. `cockpit` is the on-request read-only navigable menu.
- **Presences & Universe:** activity registers via `m1nd-presence-v0` (`m1nd-mcp/src/presence.rs`).
  Overlapping mutation signals on the same brain emit an advisory collision line on `north` — stop
  and coordinate rather than forcing writes. `GET /api/universe` is sidecar-only (never hydrates a
  brain) and is a human UI surface, not an agent surface.

## The write laws — reception governs WRITES

One served owner hosts many per-project brains and routes calls by caller root. Follow these laws
before any m1nd write:

1. **No write under `caller_root_mismatch`.** When `reception.match == "caller_root_mismatch"`,
   reads are a warning and all write verbs are prohibited (skeleton writes refuse with
   `brainless_root`). Generic cross-root `ingest` is withdrawn (`project_root` and `allow_overlap`
   are absent from the published schema), and `brain.bootstrap.birth` refuses over MCP/REST with
   `human_gesture_required`. For a brainless repo on a shared owner, offer
   `m1nd init --birth <repo>` and stop.
2. **No twin brains.** Minting a brain for a parent, child, or worktree of an existing brain refuses
   (`overlap_parent` / `overlap_child` / `overlap_worktree`). A git worktree binds to its main
   repository's brain.
3. **Memory writes never move `workspace_root`.** `memorize` and memory-ingest merges cannot demote
   a brain's `workspace_root` onto `agent-memory`; boot self-heals pre-fix manifests with
   `healed workspace_root: <from> -> <to>`. Never hand-edit manifests.
4. **Only `ingest {mode: "refresh"}` is agent-callable — from the exact declared root.** `replace`
   and `merge` remain policy-disabled for every client. `refresh` is admitted by action at
   `SCOPED_GRANT_A2` without a lease, and refuses non-exact roots (`refresh_root_not_exact`),
   unresolvable paths, concurrent refresh (`refresh_in_flight`), root-set changes, and candidates
   below 60% of live node count (`refresh_would_shrink_graph`).
5. **Brainless repo startup doors.** On direct stdio, a launcher's explicit `M1ND_WORKSPACE_ROOT`
   (or `launcher_workspace_root`) with a private writable runtime prepares an empty bound graph
   automatically and installs exact-root refresh identity (see `docs/AGENT-AUTONOMY.md`). Outside
   that contract, the human terminal command `m1nd init --birth <repo>` creates either an
   `owner_bound_graph` (when the runtime lives inside the repo) or a hosted `project_brain`
   (reached via `--attach auto`), refusing if scan yields zero nodes (`birth_produced_empty_graph`).
6. **Withdraw a capability in prose in the same PR.** `tests/test_agent_surface_bootstrap_honesty.py`
   enforces that agent-facing prose matches runtime withdrawals; extend it whenever a verb or
   parameter is retired.
7. **Human-only writes and write-door guards.**
   - `candidate_edit` is atomic and candidate-only; **ratifying** a skeleton is human-only.
   - `receipt_import` requires a server-allowlisted `imported_via` origin (`"human-ui"` or
     `"human-touchid"`); agents never land receipts.
   - Archiving a superseded `merge_wait` letter via `mission_post` requires `archived_via: "human-ui"`
     and refuses non-`merge_wait` heads (`invalid_transition`); agents never archive.
   - `mission_post` with a `receipt_candidate` refuses mismatched `scope.boundary_version`
     (`stale_scope`). Graph persistence backs up snapshots under 20% shrink to `<path>.bak-<unix_ts>`
     before overwriting.
