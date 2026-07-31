# AGENTS.md — working guide for any coding agent on m1nd

Vendor-neutral instructions for autonomous agents (Jules, Codex, Claude Code, Cursor, …)
working on this repository. Read this first; it is the contract.

**m1nd** is a neuro-symbolic code-graph engine in **Rust** (workspace, resolver 2, six
members): `m1nd-core` (in-memory engine) · `m1nd-ingest` (extractors / write side) ·
`m1nd-mcp` (the served MCP owner + every verb) · `m1nd-control` · `m1nd-runnerd` (the
write/execution lane — the only spawner) · `m1nd-openclaw`. Outside the Rust workspace:
`m1nd-ui` (the served web UI, Vite/React), `m1nd-demo`, and an npm wrapper in `npm/`. Philosophy: agent-first, proof-grown,
local-first, calibrated honesty (`absent`/`abstain`/`insufficient_evidence` are real answers).

This is a **PUBLIC** repository. Everything you commit is published.

## The gates (must pass — these ARE the CI)

Run these before you consider any change done. The blocking gate is **ubuntu + macOS**:

```bash
cargo test --workspace --all-targets
cargo test --workspace --doc                            # the sentinels --all-targets skips
cargo clippy --workspace --all-targets -- -D warnings   # warnings fail the build
cargo fmt --check
```

The second line is not a duplicate of the first: `--all-targets` **excludes** doctests by
construction, and every doctest in this workspace is a `compile_fail` sentinel guarding the
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

**Windows is REQUIRED again (since 2026-07-29):** `windows-latest` runs in the same
`rust-gates` matrix as ubuntu/macos and its red blocks merge. It spent 2026-07-23→29 as an
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

**`m1nd-ui/dist` is tracked on purpose** (`.gitignore:21` — rust-embed compiles it into every
`m1nd-mcp` binary), so the build output above is *part of your change*: commit it. `ui-gates`
rebuilds from a clean checkout and fails if `dist` comes out different. That gate exists because
nothing else could see the drift: the build digest, the runtime digest and the `ui_bundle`
authority all hash the same committed tree, so a bundle five commits behind its own source still
reported COHERENT (2026-07-29, mailbox letter `84fde5e4da2e`). The build is byte-reproducible —
three consecutive host builds and a `linux/amd64` `node:22` container emit an identical tree — so
a red there is drift, not nondeterminism.

Touching the shell chrome (`src/App.tsx`, navigation, landmarks, any control's label) also runs
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

**A gate is evidence only about the tree it ran in.** Cargo's metadata hash does not encode
the source path, so two checkouts sharing one `CARGO_TARGET_DIR` emit the same artifact name
and one can link the other's binary. Measured across parallel worktrees on 2026-07-27/28: a
checkout whose `m1nd-control/src/action_catalog.rs` held 169 entries linked a sibling's 172
and failed 47 tests with `CatalogDrift` — cured by `touch` alone, with no code change. The
red is the harmless half; a gate that PASSES on another checkout's binary makes the claim
unfalsifiable. So before running gates anywhere parallel checkouts exist, take this one's
own directory:

```bash
export CARGO_TARGET_DIR="$(scripts/cargo_target_dir.sh)"   # deterministic, per-checkout
```

Deliberately not a checked-in `.cargo/config.toml`: CI and a lone clone already build in
isolation, and no contributor should inherit this machine's build layout.

**MCP tool-schema contract:** every advertised tool's `inputSchema` MUST declare a top-level
`"type": "object"` (MCP spec). Strict clients (Claude Code) reject the ENTIRE `tools/list`
when a single tool violates it — the 2026-07-22 incident (a bare top-level `oneOf` on
`mission_service`/`external_mutation_service`) silently unregistered all 48 tools from live
sessions. The registry-wide regression test `every_tool_input_schema_is_top_level_object`
(`m1nd-mcp/src/server.rs`) enforces this; never weaken it to land a schema.

**REST route seating contract:** `POST /api/tools/{tool}` runs the F-01 generic floor gate
(`enforce_generic_action_policy`) on its way to generic dispatch. A verb that carries its OWN
interception in `handle_tool_call` — the owner→daemon proxies (`mission_spawn`,
`candidate_naming`, `curation_spawn`) and the typed G3 facades — is refused by that gate, so
seating one BEHIND it turns its REST path into dead code behind a 403 while every test stays
green (it happened twice in the field: #471, #475). Declare every specially routed verb in
`REST_ROUTE_SEATING` and give every owner proxy a probe in `OWNER_PROXY_PROBES`
(`m1nd-mcp/src/http_server.rs`): the guard
`rest_route_verb_seating_is_exhaustive_on_both_sides_of_the_floor_gate` reads the live route
source and holds both tables exactly equal to it, in both directions.

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
- **How PRs merge (owner-ratified 2026-07-24): squash is the default.** One PR = one
  conventional commit on main — the changelog is generated from commit subjects, branch noise
  (WIP commits, conflict-resolution merges) never lands individually, and any PR reverts as one
  gesture. **Merge commits are reserved** for ceremonies where the commit LINEAGE is itself the
  artifact — the M1ND-10 candidate freezes, where each preserved commit carries its own guard
  PASS (squash would destroy the proof). Auto-merge (squash) is armed on a PR once it has a
  review verdict; never arm what nobody read.
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

## The G6 blind benchmark is the owner's, not yours

The G6 knowledge-quality ceremony is staged to one command
(`scripts/benchmark/g6_formal_run.sh`, described in
`docs/benchmarks/G6-FORMAL-CEREMONY.md`) and **only the owner runs it**. Agents may
stage it, verify it, and run `--dry-run` or `g6_formal_preflight.sh`, which touch
nothing but public artifacts. Three hard rules:

- **Never open `docs/benchmarks/**/operator-only/` or `**/runner-results/`.** Those hold
  the labels and the raw measurements. The blindness is structural — do not be the
  hole in it. Hashing a sealed file against its pinned digest is fine; parsing it is not.
- **Never simulate the ceremony.** A run that did not happen is `NOT_RUN`; a verdict
  that was not measured is fraud. `NOT_PROVEN` is a legitimate, valuable outcome.
- **Never re-run to chase green.** The metric spec allows one sealed run per revision
  (`one_sealed_run_only_no_rerun_until_pass`); a `FAIL` stays in the record.

## The custody ceremony — the one surface no agent may run

`m1nd-mcp --custody-ceremony <verb>` drives the G9 Secure Enclave custody ceremony
(`docs/benchmarks/G9-CUSTODY-CEREMONY.md`, amendment G9-A1 Path B). Of its five verbs, agents
may run exactly ONE: `preflight`, which reports prerequisites and provisions nothing.

**`provision-seats`, `owner-seat`, `seal` and `assemble` are the owner's.** No agent may perform,
simulate, stub, mock or dry-run them, provision any enclave key, touch biometrics, or produce a
`custody-ceremony.sealed.json`, a seat public key, or any claim that a ceremony happened. The
ceremony's entire evidentiary value is that a human proved possession of hardware no software
path can stand in for — an agent that synthesises the artifact has destroyed the thing it
imitated. An agent that finds the ceremony un-run OFFERS the command and stops.

The code holds part of this line mechanically: the ceremony is reachable only from its own CLI
ingress (the `--birth` precedent — the ingress IS the human-origin fact), it appears in no MCP
tool and no REST route, the biometric step refuses when no human is attached (asked BEFORE the
platform question, so an unattended process is refused on every OS), and the two provisioning
entry points refuse each other's seat class fail-closed — `provision_agent_enclave_seat` refuses
the human seat, `provision_owner_biometric_seat` refuses every other one, and only the ceremony
door reaches either. `m1nd-mcp/tests/custody_ceremony_wiring.rs` holds the rest, including a guard
that fails if a simulation path is ever added and one that fails if any verb goes back to
answering from a placeholder instead of asking the platform.

The four verbs now reach the real floor, which means a run on any unentitled binary — every local
build — refuses at the keychain naming prerequisite P4. That refusal is the correct answer and
must never be worked around: `docs/benchmarks/G9-CUSTODY-CEREMONY.md` §5 R1.

**One artifact IS entitled, and it changes nothing above.** The release builds a second macOS
artifact, `m1nd-custody-ceremony.app` (the same binary bytes in an app-like bundle with the owner's
provisioning profile embedded), because a restricted entitlement on a raw executable is SIGKILLed
by the kernel — `build/README.md`. It exists so the OWNER can run the ceremony; the four verbs stay
the owner's on it exactly as on any other build, and `preflight` stays the only one an agent may
run. Agents do not build it, do not sign it, and never handle the profile: it is a repository
secret the release step reads and deletes.

**`--seal-independence-spec <PATH>` is NOT one of these verbs.** It sits next to them in the CLI
and serves the same ceremony, but it seals a DOCUMENT: it reads the owner's hand-authored
`IndependenceSpecV1`, fills `independence_spec_digest` from the digest of that spec's own core,
prints the sealed document, and exits. It opens no enclave, no keychain, no protected root and no
ceremony state, so it runs on every platform and there is nothing in it an agent is forbidden to
perform. It produces no seat, no key and no claim that a ceremony happened — the prohibition above
is untouched by it.

## Dogfood m1nd — for LOCAL agents only

If you can reach the served m1nd owner (a local process on `127.0.0.1:1338`), orient with
`north(task)` before editing and `memorize` durable findings after — ground yourself in the
graph, don't start cold. **Cloud VMs (e.g. Jules) cannot reach a local owner**, so skip this
unless m1nd is served at a reachable address.

Every agent is a sensor: if m1nd misbehaves during a mission, append one JSON line to the
field-report spool (see `CLAUDE.md`) — report, never fix mid-mission.

## The box — carve out-of-scope findings in stone (mandatory, every repo)

**The spool above IS the box.** `~/.m1nd/field-reports.jsonl` is the ONE append-only write slot
(`mailbox.rs`); a distributor routes each entry by its `repo` field into that project's box.
Writing a field report and writing a letter are the same gesture, not two — the classes are the
same set (`bug` / `honesty` / `friction` / `win`). What follows is the half that was never
stated: what the box is FOR, and the duty to read and close it.

Every repo has a box: `<repo>/.m1nd/inbox.jsonl`. It is born LOCAL behind a
consent-deferred `.gitignore` (`mailbox.rs` §C7.5); the repo's own `m1nd init` is the ONE
consent moment that flips it to committed, after which what the project knows travels with the
project. An existing `.gitignore` is never rewritten. **Committing a box publishes its letters —
in a public repo, treat that as a publishing decision, not a formality.**

The box exists for one specific case, and it is the case that bites subagents hardest: **you
hit a real defect that is NOT in your scope.** Do not fix it mid-mission (that is how a focused
change becomes an unreviewable sprawl) and do not swallow it. Write one letter — what you saw,
what you expected, the evidence — so the system's MAIN agent can fix it at the opportune
moment. Carving it in stone is the whole point: the finding outlives your session.

**Two boxes, always.** The project you are working in, and m1nd's own — m1nd is the tool every
agent here depends on, so a defect in it belongs to everyone to report and to its main agent to
fix. Read a box with `GET /api/mailbox?brain=<root>` or `m1nd-mcp --inbox-sweep`; these are
CLI/REST surfaces, never MCP tools, never in the agent loop.

**A letter nobody answers is pressure, not honesty** (`mailbox.rs`'s own words). Whoever holds a
system answers its box: closing a letter is as much the duty as writing one. Proof this matters:
on 2026-07-04 a subagent filed `Job Test(windows-latest) marks FAIL on the last 3 merges though
every cargo test inside it passes`. Nobody read the box. Twenty days later the same condition
was rediscovered from zero, after it had held the entire merge queue hostage and forced an owner
override to publish 1.5.0. The write half worked perfectly; the read half did not exist.

**Subagents get specs, not session hooks** — so a subagent spec must carry this duty verbatim,
or the population most likely to find out-of-scope defects is the one least likely to file them.

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
   `_ratify` / `_reconcile`, `mission_post`) would land in the WRONG brain. **No public
   gesture lifts this for AGENTS.** Generic cross-root `ingest` remains withdrawn
   (`project_root` is absent from the published schema; POSITIVE_SOVEREIGN), and over the
   wire `brain.bootstrap.birth` refuses every client with `human_gesture_required` — the
   stamp is the binary's own CLI flag, which no MCP or REST payload can forge. The way
   forward for a brainless repo is the HUMAN's one-time ceremony: OFFER the exact command
   `m1nd init --birth <repo>` and stop — running it is not the agent's to do. Until then,
   reconnect to an owner that already hosts the intended repo, or stay read-only with the
   mismatch warning intact — do not write.
   (The mechanical write-refusal has LANDED — every skeleton write verb
   (`skeleton_candidate`, `candidate_edit`, `system_blocks_seed_import`/`_ratify`/`_reconcile`/
   `_archive`/`_delete`, `candidate_lease` acquire) refuses under mismatch with a teaching
   `brainless_root` that names both roots; the doctrine holds regardless.)
2. **No twin brains.** Minting a brain for a root that is the PARENT, CHILD, or WORKTREE of
   an existing brain is refused with a teaching error (`overlap_parent` / `overlap_child` /
   `overlap_worktree`) that names the conflict and the two ways forward: bind to the existing
   brain, or mint a separate one anyway only when you know exactly why. It holds on BOTH
   doors — the MCP wire and REST `POST /api/tools/ingest` route through one guarded core —
   but neither way is reachable from the public MCP surface while the bootstrap consumer is
   absent (`project_root` and `allow_overlap` are not in the published `ingest` schema). A burst worktree does NOT get its own brain; bind to the main
   repo's. This stops one repo growing two brains (double ingest cost, memories fragmented).
3. **Memory writes never move your code root.** `memorize` (and any agent-memory ingest
   merge) can no longer demote a brain's `workspace_root` onto its own memory-store dir —
   the write path guards it (the #326 family, third member), and a brain found already
   flipped self-heals at the boot/load seam with an honest log line
   (`healed workspace_root: <from> -> <to>`). If you ever see a bare-REST
   `caller_root_mismatch` naming an `agent-memory` dir as the bound workspace, that is this
   disease on a pre-fix binary — rebuild/restart heals it; never hand-edit the manifest.
4. **The one ingest you CAN run is `ingest {mode:"refresh"}` — from exactly your own root.**
   `replace` and `merge` stay policy-disabled for every client; `refresh` re-scans a root the
   brain has ALREADY declared and is admitted at `SCOPED_GRANT_A2`, A2-locally, with no lease
   (`docs/GENESIS-INGEST-CONSUMERS-SPEC.md` §1, owner-ratified 2026-07-29). It is admitted by
   ACTION, never by floor: the two siblings at that same floor — `source.edit.commit` and
   `graph.ingest.merge_existing` — stay refused, and a test pins their refusal bytes.
   What it refuses, always with nothing mutated: a caller root that is not EXACTLY a declared
   root (`refresh_root_not_exact` — a subdirectory is not the root, and neither is an explicit
   REST `?brain=` selector); a path that does not resolve (`refresh_root_unresolvable`); a
   second refresh of the same root (`refresh_in_flight`); a root set that would move
   (`refresh_would_change_roots`); and — the armor that matters — a candidate holding under
   **60%** of the live graph's nodes (`refresh_would_shrink_graph`, naming both counts). That
   last one exists because the persist layer's own shrink guard is fail-open by written
   design: it backs up and writes anyway. Do NOT reach for `refresh` to repair a reception
   mismatch — it cannot create or rebind anything, and law 1 still governs. It closes the
   reflex vector, an agent acting from habit or misconfiguration; it is not a defense against
   a hostile same-UID process (that is the lease plane, still dormant).

**When you withdraw a capability, sweep the prose in the same PR.** The cross-root bootstrap
was withdrawn in the runtime (the published `ingest` schema lost `project_root` and
`allow_overlap`, and `server.rs` asserts the served instructions never name them) — but seven
prose surfaces kept teaching it, and `v1.5.0` shipped that way: quickstart, lifecycle,
changelog, README, this file, and both agent skills all sent readers to a call that fails
closed. A guard that covers only the wire is half a guard: the prose IS the interface for
every agent and every new user. `tests/test_agent_surface_bootstrap_honesty.py` now extends
the runtime's assertion to every instructional surface, so the two can no longer disagree in
silence. Withdraw a verb, add it to that list.

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
