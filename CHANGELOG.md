# Changelog

All notable changes to m1nd are documented here. This project uses [Semantic Versioning](https://semver.org/).

---

## [Unreleased]

### Fixed — the owner boot now serves the graph it loaded

- **A pre-1.5 runtime could load a full graph and serve zero nodes, on every boot.** The brain
  actor restores checkpoint `CURRENT` and rebuilds its whole session from disk when it starts.
  The 1.5 legacy adoption wrote the pre-1.5 graph into the runtime root *before any actor
  existed*, so the actor reverted it on the same boot — and the adoption journal still recorded
  `status: "adopted"`, whose own guard forbids re-adoption. **The rescue was spent without ever
  having worked**, and the symptom was silent: `Loaded graph snapshot: N nodes` followed by
  `Server ready. 0 nodes`. Adoption now runs *inside* the actor boundary and commits through the
  checkpoint, so `CURRENT` is never older than the files it describes. The journal is written
  only after the commit is acknowledged, and an adoption that did not stick is re-adoptable —
  a journal beside an empty brain is exactly the footprint of a reverted adoption. **An affected
  installation recovers on its next boot with no operator step.**
  The rejected alternative — letting an uncommitted file outrank `CURRENT` — is pinned as
  *correct* by its own test, so this cannot later be "fixed" by inverting it: a half-written
  graph must never beat a committed one.

### The graph learned to write — `transplant`

- **New verb `transplant`** (plus `transplant_preview` / `transplant_commit`). Move a top-level
  Rust `fn` between files of the SAME crate **by reference**: the caller sends a symbol and two
  paths, and the server computes the whole move from the graph — the widened item extent (doc
  comments and attributes travel), the dependency trichotomy from `calls` edges (private deps
  travel; shared deps stay, gain `pub(crate)` and a back-import), and every referencer
  re-qualified across N files — then writes atomically through `apply_batch`, re-ingests, and
  returns a receipt. Measured on a real 714-line move: **256× fewer output tokens** than the
  whole-file path (12,235 → 48).
- **The receipt is an honesty contract.** `refs_unresolved` names every reference site the verb
  refused to rewrite (grouped/nested `use`, globs), `state_left_behind` names node-addressed state
  the re-ingest orphaned, `blocks_touched` names the SystemBlocks whose ratified boundary this move
  aged, `rustfmt` says whether the computed contents were formatted before the write. A zero in
  those fields is a proven zero, never an unchecked one.
- **A refusal never touches a byte, and teaches the retry.** A destination collision names the
  occupant (over the FULL item namespace, not just `fn`); a poisonous module stem (`lib`/`main`/
  `mod`) names the invalid path it would produce; a cross-crate move names both crate roots; a
  missing destination, a nested symbol, or a repeated move each get their own precise message.
- **Two-phase path.** `transplant_preview` stages the complete multi-file plan (per-file base hash
  and line deltas) behind a 5-minute handle; `transplant_commit{confirm:true}` re-validates the
  hash of EVERY planned file — source, destination and each derived referencer — and refuses a
  stale plan instead of clobbering drift.
- **Wired into the existing laws, not around them.** Denied under a read-only attach; classified in
  the action catalog as a source write; the armed proof gate covers the DERIVED referencer set (not
  only source and destination); a protected-zone match (`ci/protected-zones.json`) refuses
  fail-closed unless the caller passes an explicit `allow_protected` reason, which the receipt
  records; the write relays as `graph_changed` so live viewers refetch.
- **Declared v1 boundaries** (`docs/TRANSPLANT-PRD.md` §7): top-level `fn` only, module = file
  stem, same crate, the destination file must already exist, grouped/nested `use` in the source
  file is reported rather than rewritten, macro-generated references are invisible to the parser,
  and a re-ingest failure AFTER the write leaves changed files with an error return.
- **Proof:** 55 tests in 12 binaries (53 active + 2 declared `#[ignore]`) — contract battery,
  stress/refusal battery, adversarial harness with a "nothing else changed" certificate,
  property-based battery with pinned regressions, a self-hosting oracle that moves a real `fn`
  inside `m1nd-core` and compiles it, plus dedicated suites for the proof gate, protected zones,
  node identity, boundary aging and concurrency.
- **Known follow-ups, declared:** full paint-tag follow across a move needs owner-side wiring
  (stable node identity or a paint-tag registry) — the ideal is pinned by an `#[ignore]`d
  acceptance test; the incremental re-ingest leaves the moved symbol's OLD node lingering in the
  graph (pre-existing, surfaced by this work); the post-commit reverse-gate (auto-rollback on an
  ERROR delta) is its own cycle.

### Ingest

- **Rust function nodes now carry their real extent.** The extractor records a `function_item`'s
  true end row from the parsed tree instead of leaving `line_end == line_start`, so graph
  provenance addresses the whole item rather than its declaration line. Measured on `m1nd-core`:
  791 of 794 function nodes gained a real extent (99.6%), snapshot size −73 bytes, ingest time
  inside the existing noise band.

---

## [1.5.0] — 2026-07-22

> **Active mode remains HUMAN_GATED.** The M1ND-10 authority/autonomy machinery ships **DORMANT**
> and **NOT_INSTALLED** — this release does not activate autonomy; formal
> G6/G7-LIVE/G8-hosted/G9-custody/G10 are not claimed.

The era that grew a visual operating system for repositories, a mission spine with a single audited
spawner, a human voice at the threshold, and — underneath, dormant and human-gated — the M1ND-10
control-plane organism. Span `v1.4.0..HEAD`: roughly 85 pull requests (#302 → #387), 368 commits,
855 files. None of it reaches installers until this version is published.

### Human View v2 — the visual OS for a repository

- **Build Map front door.** The map became the entry surface for a project brain, not a buried view.
- **SystemBlock ontology.** A typed vocabulary for the parts of a system and how they compose.
- **Typed receipts + reconciliation engine.** Every map assertion carries a typed receipt; a
  reconciliation engine diffs the drawn model against the real graph and offers a two-step,
  one-click-write Reconcile.
- **Show Code + Copy Packet.** Jump from any block to its real code; copy a grounded packet for an
  agent.
- **`skeleton_candidate`.** Map any repository into a candidate skeleton, curated before it binds.

### Mission layer

- **Mission letters (hash-chain).** Missions are letters on an append-only, hash-chained ledger.
- **`m1nd-runnerd` — the ONLY spawner.** A dedicated runner daemon executes pinned spawn missions in
  an isolated git worktree, runs the gate, and emits mission letters on the chain — it never writes
  `landed`.
- **The human landing gesture.** Landing a mission is a human act, by design.
- **Curation lane.** Candidates enter the graph through an explicit lane, never by leak.

### The Voice + the human layer

- **north landing bell.** `north` opens with a VOICE CARD when a served owner is live.
- **Voice cards + scan-loading state machine.** The scan surface is a real state machine
  (empty · loading · partial · error · ideal) rather than a spinner.
- **Presences + the Hall.** Live presences per brain; the Hall gathers owner alerts.
- **Cockpit.** An operator cockpit over the real queue.

### The Universe

- **Panorama of project brains as worlds.** Every project brain is a WORLD (mass = nodes, light =
  manifest freshness shown as age, satellites = live presences, amber ring = pending gestures).
- **Landing — one queue.** A single queue for every world's human gestures.
- **Living map + hash router.** The map subscribes to scoped graph mutations and stays live; a hash
  router adds deep links and a real Back, with brain keys as basenames (never abspaths).

### Honest front door + CI reviewer

- **Proven-vs-designed table.** The README front door separates what is proven today from what is
  designed, with calibration language downgraded to what is actually measured.
- **Ambient shim.** The npm-shipped SessionStart hook opens with the north voice card when a served
  owner is live; cwd-independence proven.
- **Retrieval battery.** A standing retrieval battery guards answer quality.

### Binding immunity + plasticity

- **Served owner pinned.** The served owner's binding is immune to foreign local runs — an HTTP
  `ingest{path}` can no longer rebind the owner.
- **Write-refusal under mismatch + mission-control persistence.** Writes fail closed under scope
  mismatch; mission-control state persists across restarts.
- **Breaking Rust embedding API.** `http_server::spawn_background` and
  `spawn_background_with_owner_authority` are now async readiness barriers that return
  `Result<BackgroundHttpHandle, OwnerAuthorityAssemblyError>`. A successful handle exposes the
  effective bound address; `BackgroundHttpHandle::shutdown().await` returns the final actor
  checkpoint ACKs. Bind, security, publication, serve, actor-shutdown, and task-join failures are no
  longer hidden behind a log-only background task.
- **Actor-only transport transactions.** stdio, Streamable HTTP, REST routing, the combined
  `--serve --stdio` mode, and OpenClaw route tool execution through the bounded per-brain actor. Tool
  errors remain callback errors until the actor restores the preimage, then become MCP `isError`
  content. Crate-external raw session/dispatch access is closed.
- **Owner lifetime and shutdown.** Read-write leases use an OS-backed lifetime lock in addition to
  the durable lease record; the actor owns an independent heartbeat worker that stays live while idle
  or inside a long command. Shutdown closes transport and runtime-job admission together, drains
  admitted work, checkpoints actors in deterministic order, withdraws endpoints, revokes stale
  heartbeat permits, and releases hosted instances only after actor stop.
- **Combined MCP protocol parity.** `--serve --stdio` supports line and `Content-Length` framing,
  `initialize`, `ping`, `tools/list`, `tools/call`, notifications, parse errors, and
  method-not-found responses with the same wire shapes as stdio-only mode. Stdout backpressure is
  isolated on a bounded no-authority writer thread.
- **No detached mutation after HTTP timeout; cooperative termination.** A slow blocking MCP command
  is awaited to its terminal actor result instead of returning a timeout while work continues in the
  background; foreground HTTP and OpenClaw handle SIGINT and SIGTERM, drain active clients/requests,
  require the final checkpoint ACK, and release ownership in order.

### Native Touch ID at the origin

- The tray was rebuilt with a native Touch ID stamp live in the menu bar — a human gesture at the
  point of origin.

### M1ND-10 organism (control plane + authority) — DORMANT / HUMAN_GATED

> Ships dormant and NOT_INSTALLED. Concrete hardware adapters are absent, the deployed owner stays
> fail-closed, and this release neither claims `FULL_AUTONOMY` nor activates any autonomous path.

- **Catalog of 169 actions + canonical identity.** A typed action catalog and a canonical organism
  identity.
- **Ed25519 / P-256 authority.** Domain-separated, independently signed authorization receipts,
  transactions, execution/review results, and a signed AuthorityWAL whose `COMMIT` is the sovereign
  mutation point.
- **MissionService + AuthorityWAL.** Typed mission consumption over a durable, crash-recoverable
  write-ahead log.
- **Per-brain actors + runtime + checkpoints.** Bounded per-brain actors, a runtime, and
  deterministic checkpoints.
- **Universal ingest + evidence spine + calibration.** Universal document ingest, an evidence spine,
  and a calibration dataset.
- **G6 blind runner + G7 LIVE orchestrator.** A blind runner (G6) and a LIVE orchestrator (G7),
  advanced to source-implemented / local-proven — not hosted-LIVE.
- **Release machinery + constitutional autonomy A0–A5.** The release ceremony and the constitutional
  autonomy ladder (A0–A5) exist in the tree, dormant and human-gated.

### G4 phase 2 — Windows

- **The dir-fsync law.** Directory-fsync durability on Windows, backed by roughly 150 curated Windows
  tests. Full Windows support remains a declared phase 2.

### Secure Enclave custody floor (G9 Path-B) — DORMANT / NOT_INSTALLED

- **Enclave adapter + custody threading.** A Secure Enclave adapter (open / sign / persistence) and
  the `custody_floor` threaded through the authority assembly; keys pinned to
  `WhenUnlockedThisDeviceOnly`.
- **Amendment G9-A1 ratified.** Path B (single-host Secure Enclave floor) is ratified with Path A
  (multi-device) as the named successor. Autonomy is **NOT** activated; the amendment authorizes no
  release or gate promotion.

### Security hardening

- **Candidate-source guard, fail-closed.** The candidate source guard refuses unexpected inputs.
- **Governed migration.** A governed migration retired 246 benchmarks from the tree.
- **Pinned gitleaks + cargo-audit.** gitleaks is pinned by binary + sha256; cargo-audit runs in CI.
- **quick-xml 0.41.** Upgraded to clear 4 RUSTSEC advisories.
- **Updater fsync fixes.** Durability fixes in the updater's write path.

### Pre-tag dogfood hardening (2026-07-22)

Found by running the new m1nd on m1nd. Every fix below was reproduced as a failing test before the
fix landed.

- **MCP tool-schema contract.** `mission_service` and `external_mutation_service` advertised a bare
  top-level `oneOf` on `inputSchema`; strict MCP clients (Claude Code) reject the ENTIRE `tools/list`
  when one tool omits the top-level `"type": "object"` — every m1nd tool silently vanished from live
  sessions. Fixed, with a registry-wide regression test.
- **Honest recovery playbook.** On an empty brain the playbook recommended the policy-refused generic
  `ingest` verb (a refusal loop); it now consults the live mutation policy and names the real
  recovery paths.
- **Legacy snapshot adoption at boot.** A one-time, journaled adoption of a pre-1.5 cwd-layout graph
  snapshot into the runtime root, so a 1.4→1.5 owner boots with its brain instead of `needs_ingest`.
- **Windows path/handle hardening.** Closed a cross-platform **security** gap where a rooted
  discovery pattern could escape the scanned root on Windows (`Path::is_absolute` is not
  OS-independent); implemented the real Windows anchored walk for the peek allow-root (was a stub
  that always refused); routed torn-journal truncation and directory removal through Windows-durable
  primitives (os error 5/32); shared registry lock files for read.

### Known gap (honest)

- **Windows source-edit transaction suite.** A pre-existing Windows path-canonicalization mismatch in
  the source-edit transaction subsystem (`fs::canonicalize` emits the `\\?\` verbatim prefix that
  stored/constructed identities lack) leaves ~22 tests red on Windows — red since before this era,
  not a regression, and gated behind DORMANT / HUMAN_GATED machinery. Tracked as a follow-up with a
  full diagnosis; it does not affect the shipped read/graph paths. Windows remains a declared phase 2.

---

## [1.4.0] — 2026-07-06

The ORGANISM release. One served owner now hosts many per-project brains, so m1nd
works in any repo without a per-repo install; memory crosses brains only by an
audited promotion, never by leak; agents can spawn and grade sub-work; and the soul
knows what it can prove. Four hardening waves — bind safety, migration data safety,
the trust engine, and cross-platform locks — landed underneath the new surface, each
with a red-first proof.

### Added

- **Per-project brains in the served owner — one-call bootstrap + silent cwd routing.**
  The one served owner now hosts multiple graphs: its bound dev graph (untouched) plus
  per-project brains stored under `<runtime_root>/project-brains/<hash>/`. From a repo the
  owner does not cover, `ingest` with `project_root=<repo root>` creates the brain, ingests
  the repo, binds the session, and returns the new brain's `north` packet in one call;
  thereafter every call from that root routes to that brain silently. Owner restarts warm-boot
  each project brain from its own store.
- **Reception — degraded mode + reconnect-rebind.** A caller outside the bound repo gets an
  explicit `caller_root_mismatch` reception block with honest options instead of silent
  wrong-graph answers, and after an MCP reconnect from a host launched above the repo, routing
  consults the on-disk brain roster and rebinds to the existing project brain.
- **The medulla — cross-brain memory with a no-leak law.** Memory is pull, not push: a recall
  beat carries the caller's own project brain plus the shared medulla (promoted/doctrine
  claims); another brain's private claim never appears ambiently. `tier` selects the recall
  scope, every row is labeled `tier` + `origin_brain`, and cross-brain fan-out runs through an
  eviction gate.
- **`promote` — the audited project→medulla crossing.** Lifts a project claim into the shared
  medulla only through a verified-only gate with an origin-qualified evidence rider; demotion
  reverses it.
- **Medulla storage split + reversible migration.** Per-brain on-disk storage, `Origin-Brain`
  labeling, a brainless-root refusal, and a plan/apply/rollback migration that requires an
  explicit destination brain.
- **`delegate` / `debrief` — the delegation layer.** Grounded spawn packets for sub-agents and
  graded returns, with calibration computed from the trust ledger.
- **`soul_check` / `soul_read` — the agentic soul, PATHOS-native and verified.** Reads the repo's
  `PATHOS.md`, mechanically checks each claim against the repo, git, and the running owner, and
  reports what it can and cannot prove.
- **Per-project mailboxes, per-brain session/query counters, LRU eviction gate, and the `seek`
  conformance rerank** (X-RAY steers ranking by intent, off by absence without a manifest).
- **`calibrate_envelope` — the seek trust envelope can now reach `act`.** A real production
  writer derives a labeled corpus from the trust ledger, measures a split-conformal τ on the
  envelope's own scale, and persists the row so a calibrated seek can emit `act`; with no labeled
  corpus it stays honestly capped at `reverify`, never a fabricated `act`.
- **Hall + human layer.** A projects area (the Hall of brains), the onboarding Threshold, a
  per-brain Open REST selector, the Pre-Flight Card, and the Mailbox view.

### Fixed

- **Security wave (wave 1).** A non-loopback HTTP bind is refused without `--allow-remote`,
  unknown `learn` feedback is rejected instead of charged as a defect, the launchd restart path
  is scoped and gated, and broad L1GHT recall no longer returns an inverted (oldest-first) order.
- **Migration data safety (wave 2).** The medulla migration data-loss cluster is closed:
  `medulla-migrate` requires an explicit destination brain, the owner-alive guard port is
  overridable, cross-project doctrine stays on the medulla even when it cites evidence, and the
  destination brain is registered after apply.
- **Trust engine (wave 3).** Both activation engines re-relax stronger later arrivals (Dijkstra
  decrease-key), proven by a cross-engine equivalence test against a brute-force oracle; a
  `pagerank_dirty` flag skips stale PageRank; and `query`/`query_readonly` guard on `finalized`
  with bounds-safe ranges, so a non-finalized-graph query returns an honest empty result instead
  of panicking. A deterministic `FakeEmbedder` exercises the embed path blobless in CI.
- **Cross-platform locks (wave 4).** Concurrent access is serialized in-process on every platform
  (the Windows advisory-lock gap closed), the auto-ingest queue drains from the server idle clock,
  and persist targets resolve against the runtime root rather than the process cwd — a
  launchd-spawned owner with `cwd=/` no longer fails every persist silently and warm-boot works.
  Proven with a red→green test that spawns the real binary under `cwd=/` and asserts a second
  process warm-boots from the persisted snapshot.

### Known gaps (honest)

- **Case intelligence (R11) and the ambient wave (R12) are DESIGN-ONLY.** Both PRDs shipped, but
  their slices are not built; the ambient hook install is a named human gate.
- **The Solvency & Stop gate (OMEGA Move 2) is roadmap-only** — there is no token ledger yet.
- **Calibration rests on one signal.** Co-change is the first and only calibrated signal; the
  document-to-code binding lanes are not yet built, and the poisoned-oracle threat model (a
  poisoned eval or co-change corpus) remains open.

---

## [1.3.2] — 2026-07-04

The launch-funnel patch — a stranger's first minute now works.

### Fixed

- **`--version` flag (#254).** `npx -y @maxkle1nz/m1nd --version` errored ("missing value")
  — a stranger's most common first command. Now prints the version.
- **Fresh installs fetched a months-old beta (#254).** A brand-new HOME received
  m1nd-mcp `0.9.0-beta.6` plus confusing channel advice; fresh installs now fetch the
  runtime matching the npm package's own version, with an honest fallback to the latest
  release.

### Added

- **README conversion pass (#256):** 30-second real-session demo GIF, badges row,
  a "60-second start", and `llms-install.md` (agent-legible install) — in all 8 languages.
- **m1nd.world launch-week hero (#255):** the shell story, registry install, honest
  proof points; stale claims removed.

---

## [1.3.1] — 2026-07-04

Discoverability patch — metadata only, no behavior change.

### Added

- **npm keywords** (`mcp`, `mcp-server`, `model-context-protocol`, `code-graph`, …) and
  **crates.io keywords + categories** on all three crates — both were shipping **empty**,
  so the published packages were invisible to registry search. Repo GitHub topics set to
  match. `glama.json` added (Glama listing claim). `server.json` synced to 1.3.1.

---

## [1.3.0] — 2026-07-04

The construction-era release: **the shell reaches every host.** One 24-hour sweep —
fourteen PRs — empties the field-triage mailbox to zero, takes the Living Tree live,
teaches `m1nd hosts` twenty-two agent hosts, and steps m1nd into the official MCP
Registry. (A `1.2.2` section was drafted here but never tagged; its content ships in
this release.)

### Added

- **`m1nd hosts` learns 22 hosts (#244).** From 5 to 22: seven TIER-A hook recipes
  (`SessionStart`/`agentSpawn`/`TaskStart` families — claude, codex, qwen, kiro, cline,
  continue, grok) plus fifteen B-tier doctrine emitters (cursor, windsurf, zed, vscode,
  gemini, antigravity, opencode, warp, trae, jetbrains, amp, goose, crush, aider, generic).
  `plan` is pure print; `apply` is idempotent and never clobbers foreign config (the codex
  duplicate-TOML incident is now a regression test); on claude, apply never writes
  `settings.json` — it prints the block for explicit pasting.
- **`m1nd-north-shim` (#244).** New fail-open bin that wraps `m1nd agent first-minute`
  and renders its envelope into the hook contract
  (`{"hookSpecificOutput":{"additionalContext":…}}`) — one stable command every
  session-start hook can call.
- **The Living Tree goes live (#242).** A shared mutation predicate now derives a
  browser `graph_changed` event on the existing `/api/events` SSE stream (closing the
  known pure-reader relay gap); the UI refetches with a calm ~500 ms debounce and falls
  back to polling. Fonts are vendored (Instrument Sans, IBM Plex Mono, Fraunces — OFL,
  ~116 KB): the UI renders fully offline, zero external hosts in `dist/`.
- **HOST-INTEGRATION-MATRIX (#241).** The canonical map of ~24 agent hosts ×
  (session-start hooks / MCP `instructions` rendering / roots / rules files), every cell
  carrying its verification label, with copy-pasteable TIER-A recipes and the honest
  spec limit: a server speaks only when called — the in-band packet is the universal floor.
- **First-Contact Reception protocol (#238).** TWO-TIER-BRAIN-PRD §9.5: on first contact
  the bridge/owner answers with where-you-are, what-exists, machine-executable options,
  a suggested default, and honest gaps — silent binding only when cwd matches (TT-INV-12).
  Field-evidenced by the Antigravity silent-bind report.
- **Two-Tier Brain PRD (#227)**, **Human-Layer PRD (#222)** + Living Tree Slice 0 (#232),
  and the **§O.12 subagent Delegation Layer (#224)** — the construction era's three official
  blueprints.
- **PATHOS auto-refresh + checkpoint 9 (#236/#237/#239).** git-cliff + GitHub Action keep
  the auto sections fresh on every main push (fail-soft under branch protection); cp9
  consolidates the era.
- **MCP Registry manifest (#243).** Root `server.json` (2025-12-11 schema) + `mcpName`
  in the npm package — the ownership proof the official registry validates.
- **agent-docs CI gate (#229)** and the **README re-spined around "the shell" (#228)**
  in all eight languages.

### Fixed

- **Warm-boot immortal graph (#230).** Relative persist targets anchor on the runtime
  root; the launchd owner stopped failing persistence (39 consecutive failures → 0) and
  now warm-boots the full graph.
- **Marker fragments excluded from recall/anchors (#231).** `::tag::` structural
  fragments no longer pollute north's memory beat or anchor slots.
- **Attach re-init covers every unknown-session shape (#233)** — including the frameless
  404 — with restart-survival proven end-to-end.
- **Attach self-echo (#235).** Write-tool responses return real envelopes through the
  bridge; `graph_changed` notifications no longer race the response into the stdout sink.
- **auto_ingest CI flake killed at the source (#240).** Watch events for existing
  directories are dropped before the queue, so `queue_depth` is an honest signal and the
  single forced tick is deterministic — proven 20/20 across three configurations.

### Removed

- **The unmeasured `savings` envelope (brand gate G1) and the opt-in `savings`/`report`
  unmeasured-claims surface (G1.5) (#234).** An uncalibrated "tokens saved" number is a
  confident guess, and it has no place in a product whose promise is calibrated trust.
  `savings` is gone entirely (dispatch arm, handler, types, tracker state); `report`
  survives stripped to its honest content — query counts, elapsed time, graph size,
  heuristic hotspots. Completes the beta.7 de-advertisement.

---

## [1.2.2] — 2026-07-03

Brand gate G1 — honesty is the product.

### Removed

- **The unmeasured `savings` envelope (brand gate G1).** Every response used to
  carry `_m1nd.savings`, `_m1nd.tokens_saved`, and `_m1nd.gaia.global_tokens_never_burned`
  — a confidently-stated "tokens saved" number with no measured basis. An uncalibrated
  claim is the confident guess, and it has no place in a product whose whole promise is
  calibrated trust. The envelope's honest neighbors (`suggest_next`, `read_only`,
  `summary`) are kept; the standalone opt-in `savings`/`report` tools are unchanged.

---

## [1.1.0] — 2026-06-28

A code-intelligence correctness release: a new attention runtime (`focus`), the
function-level call graph extended across languages, and the cross-file resolver
hardened so `impact`/`why` bind to the right same-name target. The honest battery
(m1nd vs `rg` on m1nd's own repo) went from 10/12 to **28/28 with 0 losses to
grep** across this arc.

### Added

- **`focus(goal, token_budget)` — goal-conditioned attention runtime.** Returns
  the minimal, budget-bounded working set ranked by goal salience, an honest
  `ignored` tail (every relevance-clearing node left out is counted, never
  silently dropped), and an answer-free `sufficiency` signal
  (`sufficient`/`gathering`/`saturated` via a knee test). `seek` gained the same
  `sufficiency` envelope.
- **Conformance-aware attention.** When a ratified X-RAY manifest resolves,
  `seek`/`focus` bias ranking by intent (erosion-source nodes down, proof-
  exercised "bedrock" up). Off-by-absence and byte-identical without a manifest.

### Fixed

- **Call graph at function granularity (Rust + TypeScript).** Calls are sourced
  from the enclosing function (not the file); free-function and lowercase-receiver
  method calls are followed; `impact` ranks code symbols above containers and
  production callers above test functions. `impact`/`why` no longer degrade to
  file-containment noise on code.
- **Same-name function ids no longer collide (all extractors).** Function nodes
  used a line-less `file::…::fn::<name>` id, so same-named functions in one file
  collided and ~6.3% of functions were silently dropped from the graph. Fixed for
  Rust, TypeScript, Java, Go, Python, and the generic extractor.
- **Cross-file call resolution.** Proximity prefers same-file > same-directory >
  cross-crate, and `Type::method()` calls bind to the impl owner via the call
  qualifier (qualifier-aware resolution) instead of an arbitrary same-name sibling.
- **`scan` honesty.** `total_matches_validated` counts validation survivors rather
  than the display `limit` (it was fabricating the raw-vs-validated delta), and
  documented `mitigated` findings are now visible at the default `severity_min`.

---

## [1.0.0] — 2026-06-27

First stable release.

### Added

- **Semantic recall on by default.** The shipped binary builds with the `embed`
  feature, so `seek` matches by meaning out of the box (model2vec, ~29 MB
  fetch-on-first-use, graceful fall-back to trigram if the model can't load). A
  content-addressed embedding cache is persisted alongside the graph snapshot, and
  behavioral excerpts (signature + body + doc-comments) feed the embeddings.

---

## [0.9.0-beta.8] — 2026-06-10

### Fixed

#### Graph edges now survive re-finalization (critical)

`Graph::finalize()` rebuilt the CSR index exclusively from pending edges, so any
node insertion followed by a re-finalization silently discarded every
materialized edge. In the MCP runtime this left retrieval running on a
near-empty graph — `impact`, `seek`, and activation queries saw no structural
edges even though ingest reported them created. `finalize()` is now idempotent:
existing CSR edges are rehydrated before the rebuild, with plasticity weights
preserved. Found through live agent dogfooding of beta.7; covered by new core
regression tests (`refinalize_preserves_edges`) and an end-to-end handler test
that asserts a cross-file import edge stays queryable after a memory write.

- Stack-trace frame parsing handles Windows drive-letter paths: the `C:` colon
  previously shifted the `path:line:col` split and zeroed parsed frames.
- L1GHT evidence dedup checks materialized edges, not only pending ones (the
  previous behavior implicitly relied on the finalize bug).

### Security

- Dependency advisories patched: `rustls-webpki` → 0.103.13 (high), `rand` →
  0.9.3, `postcss` → 8.5.15 in both UI trees. `npm audit` and Dependabot now
  report zero open advisories.

### Documentation

- README rewritten around the local mission runtime thesis — funnel-first,
  ~255 lines, quick start near the top, claims bounded by what tests prove.
- All seven i18n READMEs regenerated 1:1 against the new structure.

---

## [0.9.0-beta.7] — 2026-06-09

### Added

#### Language coverage — calls + cross-file import resolution

Native extractors now resolve both call edges and cross-file import edges across
the major languages, bringing the structural graph to real multi-language parity.

- Calls + cross-file import resolution for Rust, Python, JS/TS, Go, Java, C, C++,
  Kotlin, PHP, Scala, and Ruby.
- C# and Swift are calls-only; C# namespaces are intentionally not file-resolved.
- Specific mechanisms include a `RustModuleIndex` (resolve `mod`/`use` to file
  nodes), a `JavaPackageIndex`, C `#include` and Kotlin `import` resolution,
  PHP (PSR-4) and Scala (package) cross-file imports, a Ruby
  `require_relative` scanner, and AST-verified tree-sitter call extraction for
  C++, PHP, Scala, and Swift.

#### L1GHT compounding memory

The first end-to-end agent-authored memory loop that lives in the same activation
space as code and survives across sessions.

- `memorize` — the first L1GHT *writer*: agents persist structured claims with
  `confidence` and `evidence`, written as a graph-native `.light.md`.
- Evidence→code `grounded_in` anchoring resolves L1GHT `𝔻` epistemic markers to
  real code nodes.
- `cross_verify` `evidence_freshness` flags memorized claims whose cited code has
  since changed; `memory_freshness` is reported inline on code re-ingest.
- Boot auto-load of agent memory (`.light.md`), reported in
  `session_handshake.agent_memory`.
- Memory survives a `mode=replace` ingest instead of being silently lost.
- `mission_close` gained `write_light_memory` for one-step mission + memory commit.
- L1GHT confidence accepts word-form values (`low`/`medium`/`high`/`certain`).

#### Output discipline & tool tiering

- Bounded output for `impact`, `layers`, and `surgical_context_v2`.
- Env-gated tool surface tiering: an ESSENTIAL tier is advertised by default
  (`M1ND_TOOL_TIER`), with the full surface available on demand. Hidden tools
  remain callable by name regardless of tier.

#### RETROBUILDER + kickstart (from the consolidation line)

- RETROBUILDER agent-first wiring into agent routing.
- `m1nd kickstart` command.
- User-scope host detection (hosts-status fix for user-scope installs).

### Changed

#### Agent legibility & honesty

- `session_handshake` now surfaces `graph_intelligence` (top PageRank, attention
  anchors, memory counts).
- Real per-file `change_frequency` is wired from git history.
- `savings` and `resonate` were removed from the advertised tool surface (both
  remain callable by name).
- `twins` topological similarity is now gated by identifier-token overlap.
- `scan` uses word-boundary matching and returns populated `file_path`/`line`.
- Honest empty-state guidance for `predict`, `trust`, and `tremor`.
- Slim release binary: `[profile.release]` now uses `strip` + thin LTO.

### Fixed

#### Instruction/schema dead-ends and trust hardening

- Closed instruction-vs-schema dead-ends: `learn` is now advertised,
  `write_light_memory` and `evidence_freshness` appear in their schemas, and the
  `diverge` description was corrected.
- `attention_anchors` now reads the correct plasticity engine (was a dead signal).
- P1 trust hardening: `trace` absolute paths, `hypothesize` verdict, and
  `predict` co-change.

---

## [0.9.0-beta.5] — 2026-05-16

### Added

- Added `probe_m1nd.py short-audit`, a bounded helper that lets agents run a
  compact, file-backed orientation pass for real-world repo audits without
  treating it as final proof.

### Changed

- Updated the m1nd agent doctrine and benchmark guidance so m1nd-first agents
  learn the short-audit route, compare graph evidence against local truth, and
  preserve explicit non-claims around host rebinds and graph correctness.

### Fixed

- Fixed persisted ingest-root parsing on Windows by decoding JSON paths instead
  of reconstructing strings by hand.

---

## [0.9.0-beta.4] — 2026-05-12

### Added

- Added `m1nd hosts apply`, an opt-in host-local mutation surface that can
  install or refresh agent packs and write canonical MCP config snippets for
  known hosts while preserving `host_rebind_proven=false`.

### Fixed

- Scoped host runtime/config detection to the actual `m1nd` MCP config entry so
  unrelated MCP env vars no longer pollute m1nd readiness diagnostics.
- Demoted stale binaries on `PATH` to a shadow warning when the selected host
  config already points to a current managed runtime.

---

## [0.9.0-beta.3] — 2026-05-12

### Added

- Added `agent_runtime_contract` to critical retrieval/orientation responses so
  agents can distinguish wrong workspace bindings, cold graphs, and retrieval
  recovery states before interpreting empty results.
- Added `m1nd update` with read-only check/status/plan, opt-in apply, verify,
  and rollback commands for safe local self-update and host-rebind guidance.
- Added `m1nd hosts status`, a read-only host readiness contract for supported
  packaged hosts that reports agent-pack, config, runtime, workspace, and rebind
  caveats before agents mutate anything.
- Added `m1nd hosts plan` and `m1nd mcp-config --project` to produce
  host-specific rebind recipes with explicit `M1ND_WORKSPACE_ROOT`.

---

## [0.9.0-beta.2] — 2026-05-10

### Added

- Added `m1nd restart` as an external repair helper for stale MCP host
  bindings, old native runtime binaries, and `Transport closed` recovery.

### Fixed

- Aligned the Rust crate versions and `m1nd-mcp --version` output with the
  `0.9.0-beta.2` npm/package line.
- Isolated `m1nd-operator` probe runtimes by default so parallel agents do not
  collide on stale runtime locks during health checks.

---

## [0.9.0-beta.1] — 2026-05-07

### Added

- Added `m1nd restart` as an external agent repair helper for stale host
  bindings, old native runtime binaries, and dead MCP transports. It can build
  `m1nd-mcp`, install the managed binary, and optionally stop visible runtime
  processes while preserving the non-claim that host MCP clients still need a
  restart/rebind.

### Changed

- The universal agent pack and integration docs now teach host-neutral
  workspace binding with `M1ND_WORKSPACE_ROOT`.
- The agent recovery doctrine now classifies `Transport closed` as a dead MCP
  transport that needs host rebind/restart before m1nd recovery tools can run.
- Runtime discovery now accepts both `M1ND_MCP_BINARY` and `M1ND_MCP_BIN`, and
  prefers the managed `~/.m1nd/bin` runtime before older binaries on `PATH`.

---

## [0.8.0] — 2026-04-10

### Added

#### Daemon control plane + persistent structural alerts

The audit/runtime layer now graduates from one-shot inspection into a persisted daemon-era control plane:

- `daemon_start`
- `daemon_stop`
- `daemon_status`
- `daemon_tick`
- `alerts_list`
- `alerts_ack`

These tools keep daemon state and a small proactive alert queue alive under the runtime root, so structural warnings can survive past the exact write or ingest that produced them.

The daemon control plane also gained the operational behavior needed to make it useful in live agent sessions:

- opportunistic auto-ticks between ordinary tool calls
- daemon ticks during idle server time
- scheduler timing exposure in `daemon_status`
- tick metrics exposure in `daemon_status`
- adaptive backoff when watch activity is low
- native filesystem watcher wakeups
- burst coalescing before reconciliation
- Git-aware changed-set reconciliation when watched roots are repositories
- SCM-aware daemon baselines instead of a moving cursor model

#### Proactive structural insights on writes

`apply` and `apply_batch` now attach `proactive_insights` directly to write results instead of forcing the agent to remember the next structural checks.

Initial insight kinds include:

- `co_change_prediction`
- `untouched_test_companion`
- `antibody_recurrence`
- `trust_drop`
- `tremor_hotspot`
- `cross_repo_contract_risk`
- `schema_contract_drift`

When the daemon is active, the strongest write-time insights are also promoted into the persisted alert queue so they can be reviewed and acknowledged later.

#### `federate_auto` becomes a real evidence-to-federation bridge

`federate_auto` now turns external evidence into an actionable federation plan instead of just reporting raw hints.

It can:

- scan `external_references` output
- lift referenced files to repo roots via `.git` or manifest markers
- suggest stable namespace names for the current repo and sibling repos
- optionally execute `federate` directly in one call

Its discovery surface now includes:

- manifest/workspace evidence such as Cargo workspaces, `package.json` workspaces, `pnpm-workspace.yaml`, `pyproject.toml`, and `go.work`
- import/package-name matches against nearby repo identities
- contract artifacts such as `.proto` definitions, MCP tool-name surfaces, and OpenAPI/Swagger routes and schemas
- shared `/api/...` route evidence between the current workspace and nearby repos
- schema and component-name recognition for stronger contract matching
- scope/evidence-strength hardening so the bridge stays conservative

#### Universal document intelligence in the canonical engine

The universal document lane is now ported into canonical `m1nd` instead of living only in the integration repo.

This adds:

- canonical local artifact resolution for universal documents
- deterministic document-to-code bindings
- document/code drift detection
- provider health reporting
- local-first document watcher/runtime control

New MCP surfaces:

- `document_resolve`
- `document_bindings`
- `document_drift`
- `document_provider_health`
- `auto_ingest_start`
- `auto_ingest_status`
- `auto_ingest_tick`
- `auto_ingest_stop`

The universal lane also now preserves source-byte fidelity and writes a fuller canonical artifact set:

- `source.<ext>`
- `canonical.md`
- `canonical.json`
- `claims.json`
- `metadata.json`

Optional provider lanes are now surfaced operationally instead of implicitly:

- `Docling`
- `Trafilatura`
- `MarkItDown`
- `GROBID`

`auto_ingest_status` also reports provider route/fallback counts so agents can see whether rich extraction actually happened or whether the runtime fell back.

### Changed

#### The public surface is finally aligned with the live runtime

The docs and public product surfaces now match the real engine instead of the pre-document-runtime story.

- the tool matrix SSOT is now published and wired into the docs flow
- API coverage is complete for the current MCP surface
- GitHub Pages now publishes the real `wiki-build` output
- the canonical docs wave aligned README, examples, wiki pages, API docs, and the published tool matrix with the universal document runtime
- the GitHub wiki mirror and localized READMEs were synced with the canonical docs
- stale public counts from the old `63` / `77` / `78` eras were replaced with the live `93`-tool surface

#### Document runtime hardening

The universal runtime was tightened in several ways before and after the port:

- post-ingest semantic refresh is now restricted to the universal document lane
- file-root watchers use non-recursive mode when the watched root is a single file
- queue waiting now fails with explicit diagnostics instead of a silent timeout
- false `binding_ambiguous` cases were reduced when multiple relations hit the same target

Tool count: 77 → 93.

### Fixed

#### Provider-gated regression coverage for scholarly PDFs

The `GROBID` lane now has a provider-gated regression path that verifies the runtime resolves to `universal:grobid` for a minimal generated PDF when the provider environment is configured.

#### Canonical artifact correctness

- universal content hashes now track original source bytes instead of only the normalized canonical text
- canonical caches preserve reachable original source bytes instead of quietly rewriting everything into plain text
- binding/drift summaries refresh against graph generation instead of reusing stale semantic state

---

## [0.7.0] — 2026-04-05

### Added

#### Audit Mode + Session Foundations

Six new MCP tools reduce orchestration overhead in long structural sessions:

| Tool | What It Does |
|------|-------------|
| `batch_view` | Read multiple files or glob expansions in one call with stable delimiters, optional summaries, and auto-ingest |
| `scan_all` | Run all structural scan patterns in one call and return grouped findings |
| `cross_verify` | Compare graph state against current disk truth (`existence`, `loc`, `hash`) |
| `coverage_session` | Report which files/nodes the current agent has already visited |
| `external_references` | Discover explicit references to paths outside current ingest roots |
| `audit` | Profile-aware one-call audit for topology, scans, verification, git state, and recommendations |

Related contract upgrades:

- `health` now exposes git context (`branch`, `clean`, `head`, recent commits, uncommitted files)
- `ingest` now accepts `include_dotfiles` and `dotfile_patterns`
- `view`, `search`, `report`, and `audit` now support inline truncation metadata instead of forcing file-only spill paths

Tool count: 71 → 77.

#### RETROBUILDER: 5 Advanced Graph Analysis Tools

Five new MCP tools expose the RETROBUILDER core modules (RB-01 through RB-05), adding temporal analysis, security taint propagation, structural duplication detection, refactoring planning, and runtime observability to the tool surface.

| Tool | Module | What It Does |
|------|--------|-------------|
| `ghost_edges` | RB-01: 4D Git Graph | Parse git history and inject temporal co-change ghost edges — hidden coupling between files that always change together but have no static dependency |
| `taint_trace` | RB-02: Graph Fuzzing | Inject taint at entry points, track propagation through the graph, detect missed security boundaries (validation, auth, sanitization) |
| `twins` | RB-03: Structural Twins | Find structurally identical code via topological signature cosine similarity — detects duplicate retry logic, CRUD handlers, state machines |
| `refactor_plan` | RB-04: Intent-Driven Refactoring | Community detection + bridge analysis + counterfactual simulation for safe module extraction planning |
| `runtime_overlay` | RB-05: OTel Overlay | Ingest OpenTelemetry trace data to paint runtime heat (call counts, latency, error rates) onto graph nodes |

New types in `protocol/layers.rs`: `GhostEdgesInput`, `TaintTraceInput`, `TwinsInput`, `RefactorPlanInput`, `RuntimeOverlayInput`, `RuntimeOverlaySpan`.

Tool count: 63 → 68.

#### Diagnostic Tools: 3 Structural Observability Tools

Three new MCP tools provide structural observability, type-dependency tracing, and visual graph generation — moving m1nd from a passive graph engine to an active diagnostic platform.

| Tool | What It Does |
|------|-------------|
| `metrics` | Per-node structural metrics: LOC (with 3-tier fallback: provenance → child span → disk read), child counts (functions, structs, enums, classes), in/out degree, PageRank, density ratio. Supports scope filtering and sorting by LOC, complexity, or name. |
| `type_trace` | Cross-file type usage tracing via BFS from a type/struct/enum node. 4-tier target resolution (exact ID → label exact → segment match → substring) with explicit preference for type-defining nodes over impl blocks. Forward, reverse, and bidirectional tracing with file grouping. |
| `diagram` | Generate visual graph diagrams in Mermaid or DOT format. Centers on a node/query via BFS or shows top-N by PageRank. Supports scope filtering, type filtering, edge label display, PageRank annotation, and layout direction (TD/LR). |

New types in `protocol/layers.rs`: `MetricsInput`, `MetricsOutput`, `MetricsEntry`, `MetricsSummary`, `TypeTraceInput`, `TypeTraceOutput`, `TypeTraceUsage`, `TypeTraceFileGroup`, `DiagramInput`, `DiagramOutput`.

Tool count: 68 → 71.

#### Native OpenClaw fast path

`m1nd` now includes a native OpenClaw-facing bridge crate and fast path so the project can integrate with that execution fabric without giving up the MCP-first contract.

- `m1nd-openclaw` was added as an auxiliary bridge crate
- the native fast path preserves MCP compatibility instead of forking the product

### Changed

#### Public product surfaces were repositioned around the real runtime

The product story was reworked around current agent use, speed, and grounded structural navigation:

- the visual wiki became the primary documentation surface
- the landing/site flow was rebuilt around the product story instead of the old root page
- editor/client integration entrypoints were documented across the major MCP clients
- localized READMEs were refreshed to match the new public story
- README language around limits, scope, and grounded retrieval was clarified

### Fixed

#### CI and release operations were re-stabilized

- fresh rustfmt/clippy regressions on main were resolved
- the required `Test` status was restored for branch protection
- release prep and help/workflow surfaces were aligned before the `v0.7.0` cut

---
## [0.6.1] — 2026-03-25

### Fixed

#### Release and Publish Alignment

This patch release aligns the public release surfaces after the `v0.6.0` rollout.

- added missing crates.io metadata to workspace crates so publish succeeds cleanly
- added explicit published-version constraints on internal workspace dependencies
- hardened the release workflow so crates.io publish is skipped cleanly when
  `CARGO_REGISTRY_TOKEN` is not configured, instead of failing the whole release job

---

## [0.6.0] — 2026-03-25

### Added

#### Guided Proof State Across Core Agent Flows

Several high-value tools now surface `proof_state` plus explicit handoff guidance so
an agent can tell whether it is still triaging, actively proving, or ready to move
into edit preparation.

- `seek`, `trace`, `impact`, `timeline`, `hypothesize`, `validate_plan`, and
  `surgical_context_v2` now participate in a shared proof-state model
- guided outputs now include `next_suggested_tool`, `next_suggested_target`, and
  `next_step_hint` across the main structural triage and edit-prep paths
- `trail_resume` now behaves more like continuity orchestration than bookmark restore,
  returning compact resume hints, next-focus guidance, and tool-aware follow-up

#### `apply_batch` Progress, Correlation, and Handoff Signals

`apply_batch` has been upgraded from a “wait until the batch finishes” write surface
into an observable execution flow with stable correlation and final handoff data.

- final outputs now expose `batch_id` for correlating progress and final result
- progress reporting now includes coarse lifecycle fields such as `active_phase`,
  `completed_phase_count`, `phase_count`, `remaining_phase_count`, `progress_pct`,
  and `next_phase`
- `phases` now act as a structured execution timeline across `validate`, `write`,
  `reingest`, `verify`, and `done`
- `progress_events` now provide a streaming-friendly event log for the same lifecycle
- live `apply_batch_progress` SSE emission now happens during execution in serve mode
- replay and live transports now carry consistent batch correlation data
- the final `batch_completed` event now carries the batch’s `proof_state` and
  next-step guidance, so clients do not need to wait for a separate final blob to
  recover the cognitive handoff

#### Benchmark Harness Expansion

The benchmark system has been extended so progress UX and workflow guidance can be
measured as first-class product behavior, not only token proxy.

- benchmark runs now record `execution_origin` and `source_ref`
- long-running flows can now distinguish `live`, `replay`, and `snapshot` progress delivery
- the harness now records progress event counts, delivery modes, phase sequences,
  and guidance-followed behavior
- the `warm_structural_proof_apply_batch` scenario now captures live progress delivery
  explicitly instead of treating progress as an undifferentiated blob

### Changed

#### Help and Docs Are More Agent-Operational

The help surface and public docs now reflect the real working style of current m1nd,
with less catalog-style listing and more decision support.

- help entries now include `WHEN TO USE`, `AVOID WHEN`, benchmark-aware guidance,
  composed workflows, and proof-state handoff cues
- help and docs now frame common tool failures as short repair loops, with
  hint/example/next-step guidance that agents can use to self-correct
- README, examples, and benchmark docs now describe the current guided behavior of
  `apply_batch`, `proof_state`, and long-running progress updates more accurately
- benchmark truth now explicitly includes recovery-loop scenarios such as invalid
  regex retry, ambiguous scope retry, stale route refresh, and protected-write reroute
- benchmark research now documents progress observability and delivery modes as part
  of product truth, not only token savings

### Notes

- Current benchmark corpus summary shows `10518 -> 5182` token proxy on the
  aggregate warm-graph corpus, for `50.73%` savings
- The same corpus now measures more than token compression: `false_starts`,
  guided follow-through, recovery loops, progress events, and proof-state transitions
- Across the recorded corpus, `m1nd_warm` reduced `false_starts` from `14` to `0`,
  recorded `31` guided follow-throughs, and recorded `12` successful recovery loops

---

## [0.5.0] — 2026-03-16

### Added

#### `apply_batch` 5-Layer Post-Write Verification (`verify=true`)

When `apply_batch` is called with `verify: true`, every write now passes through a
five-layer verification pipeline before the tool reports success. A single `VerificationReport`
aggregates all layer outcomes and produces a final **verdict**.

**Layer A — Expanded Trivial-Return Detection**

Detects files that look syntactically valid but are semantically hollow.

- 30+ trivial-return patterns (empty body, constant return, pass/noop, single-line
  no-op closures, stub `unimplemented!()` / `todo!()` bodies)
- `has_real_logic()` heuristic: a file passes only when it contains at least one
  non-trivial expression — assignment, function call, conditional, loop, or match arm
  with a real body
- Pattern set is language-aware; Rust, Python, TypeScript, and Go each have dedicated
  pattern lists

**Layer B — Post-Write Compilation Check**

After the file is written to disk, Layer B runs the relevant compiler/checker in a
subprocess and captures stdout + stderr.

| Language | Command |
|----------|---------|
| Rust | `cargo check --message-format=short` |
| Go | `go build ./...` |
| Python | `python -c "import ast; ast.parse(open('<file>').read())"` |
| TypeScript | `tsc --noEmit` |

- Timeout: 60 seconds per command
- Failures produce a structured `CompileError` with command, exit code, and trimmed output
- Result surfaced in `ApplyBatchOutput.compile_check`

**Layer C — BFS Blast Radius via CSR Edges**

Uses the in-memory CSR adjacency structure to compute 2-hop reachability from every
modified file node.

- Forward + backward BFS to 2 hops
- Deduplicates reachable nodes and maps each back to a file path
- Produces a `Vec<BlastRadiusEntry>` — one entry per affected file with `distance` (1 or 2)
  and the `relation` type along the path
- Surfaced in `ApplyBatchOutput.blast_radius`

**Layer D — Affected Test Execution**

After computing the blast radius, Layer D identifies test files within 2 hops and runs
them.

| Language | Command |
|----------|---------|
| Rust | `cargo test <module>` |
| Go | `go test ./...` |
| Python | `pytest <file> -x -q` |

- Per-test-run timeout: 30 seconds
- `tests_run`, `tests_passed`, `tests_failed`, and `test_output` fields added to
  `ApplyBatchOutput`
- Zero test files found = Layer D skipped (not counted as failure)

**Layer E — Anti-Pattern Detection**

Scans the new file content for patterns that indicate a semantic regression even when
the file compiles cleanly.

Detected anti-patterns:

| Pattern | Signal |
|---------|--------|
| `todo!()` / `unimplemented!()` inserted | Stub replacing real logic |
| `.unwrap()` added where none existed before | Error handling removed |
| `panic!()` / `unreachable!()` in non-test code | Crash path introduced |
| Empty `catch` / `except` block | Silent error swallowing |
| Explicit error handler replaced with no-op | Regression in error handling |

- Comparison is pre-write content vs post-write content (diff-based)
- Each detected anti-pattern produces an `AntiPatternMatch` with location and description

#### Graph-Diff Verification

`apply_batch` now snapshots the node set before writing and re-ingests after. The delta
is compared:

- **Node set shrinkage** — if the post-write graph has fewer nodes than pre-write for the
  affected files, this is flagged as a potential symbol deletion
- **Edge set regression** — significant edge count drop triggers a `RISKY` signal
- Result stored as a structured `GraphDiff` embedded in `VerificationReport`

#### New Types

| Type | Location | Purpose |
|------|----------|---------|
| `VerificationReport` | `m1nd-core/src/verify.rs` | Top-level verification result: layers A–E + graph-diff + verdict |
| `VerificationImpact` | `m1nd-core/src/verify.rs` | Aggregated impact summary: compile status, test counts, anti-patterns |
| `BlastRadiusEntry` | `m1nd-core/src/verify.rs` | Single affected-file record from Layer C BFS |
| `CompileCheckResult` | `m1nd-core/src/verify.rs` | Structured compile output: command, exit code, stderr |
| `AntiPatternMatch` | `m1nd-core/src/verify.rs` | Single anti-pattern detection hit with location |
| `GraphDiff` | `m1nd-core/src/verify.rs` | Pre/post node+edge delta from graph-diff step |
| `Verdict` | `m1nd-core/src/verify.rs` | `SAFE` / `RISKY` / `BROKEN` — final write verdict |

#### New Fields in `ApplyBatchOutput`

| Field | Type | Description |
|-------|------|-------------|
| `verification` | `Option<VerificationReport>` | Full verification report (present when `verify=true`) |
| `compile_check` | `Option<CompileCheckResult>` | Layer B compile result |
| `tests_run` | `u32` | Total test cases executed in Layer D |
| `tests_passed` | `u32` | Passing test count |
| `tests_failed` | `u32` | Failing test count |
| `test_output` | `Option<String>` | Raw test runner output (trimmed to 2 KB) |
| `blast_radius` | `Vec<BlastRadiusEntry>` | Layer C 2-hop affected files |

#### Verdict System

The `Verdict` enum drives the final `apply_batch` outcome when `verify=true`:

| Verdict | Meaning | Condition |
|---------|---------|-----------|
| `SAFE` | All layers passed; write accepted | Compiles, tests pass, no anti-patterns, graph stable |
| `RISKY` | Write accepted with warnings | Compile OK, but anti-patterns detected OR graph shrinkage OR some tests failed |
| `BROKEN` | Write rejected; file restored to pre-write content | Compile failure OR Layer A trivial-only content detected |

On `BROKEN`, the pre-write content is automatically restored and the error is surfaced
in `VerificationReport.error`.

#### 12/12 Test Accuracy — Exhaustive Hardening

The verification pipeline passed an exhaustive test suite of 12 scenarios designed to
cover every combination of layer outcomes:

1. Clean write — all layers pass → `SAFE`
2. Compile error — Layer B fails → `BROKEN` + auto-restore
3. Trivial stub replacement — Layer A triggers → `BROKEN`
4. Anti-pattern insertion — Layer E triggers → `RISKY`
5. Test regression — Layer D fails → `RISKY`
6. Graph node shrinkage — graph-diff triggers → `RISKY`
7. Multi-file batch — blast radius correct across 3 files
8. No test files in radius — Layer D skipped cleanly
9. Python AST parse failure — Layer B Python path → `BROKEN`
10. TypeScript `tsc` clean — Layer B TS path → `SAFE`
11. `.unwrap()` added where absent — Layer E Rust pattern → `RISKY`
12. Empty except block added — Layer E Python pattern → `RISKY`

All 12 scenarios produced the expected verdict with correct field population.

### Changed

#### Tool Names: All 61 Tools Use Underscores

`dispatch_tool` previously reversed dot-notation to underscore normalization selectively.
As of v0.5.0, **all 61 tools** are registered and dispatched exclusively with underscore
names. The dot-to-underscore reversal in `dispatch_tool` has been removed.

- MCP tool names: `m1nd_apply_batch`, `m1nd_surgical_context_v2`, `m1nd_antibody_scan`, etc.
- HTTP bridge endpoint paths: `/api/tools/m1nd.apply_batch` still accepted at the HTTP
  layer for backward compatibility, but the canonical name is underscore throughout
- Callers using dot notation in direct MCP calls must update to underscore names
- All 61 tool names documented in `reference_m1nd_all_tools.md` and `mcp/m1nd/README.md`

#### Crate Versions Bumped to 0.4.0

All three crates in the workspace have been bumped from 0.3.x to 0.4.0 in `Cargo.toml`:

| Crate | Previous | New |
|-------|---------|-----|
| `m1nd-core` | 0.3.x | 0.4.0 |
| `m1nd-ingest` | 0.3.x | 0.4.0 |
| `m1nd-mcp` | 0.3.x | 0.4.0 |

The version bump reflects the addition of the verification subsystem, which introduces
new public types (`VerificationReport`, `VerificationImpact`, `BlastRadiusEntry`, etc.)
into the `m1nd-core` API surface.

---

## [0.2.0] — 2026-03-14

### Added

#### 9 New MCP Tools — "Superpowers Extended"

The server now registers 52 tools (up from 43). The 9 additions form a new
**Superpowers Extended** category focused on operational intelligence:
bug immunity, execution dynamics, propagation risk, and architectural health.

| Tool | Category | What It Does |
|------|----------|-------------|
| `m1nd.antibody_scan` | Immune Memory | Scan the entire graph against all stored bug antibody patterns |
| `m1nd.antibody_list` | Immune Memory | List stored antibodies with metadata and specificity scores |
| `m1nd.antibody_create` | Immune Memory | Create, disable, enable, or delete antibody patterns |
| `m1nd.flow_simulate` | Execution Dynamics | Particle-based concurrent execution simulation |
| `m1nd.epidemic` | Propagation Risk | SIR model predicting bug spread from known-infected modules |
| `m1nd.tremor` | Change Acceleration | Second-derivative detection of accelerating change frequency |
| `m1nd.trust` | Defect History | Actuarial per-module defect density with Bayesian prior adjustment |
| `m1nd.layers` | Architecture | Automatic layer detection + dependency violation reporting |
| `m1nd.layer_inspect` | Architecture | Layer-specific node, edge, and violation inspection |

#### Bug Antibodies (`m1nd-core/src/antibody.rs`)

Immune memory system that learns structural bug patterns from confirmed defects and
automatically scans new code for recurrences.

- `Antibody` / `AntibodyPattern` / `AntibodyMatch` structs
- `PatternNode` with `match_mode`: Exact / Substring / Regex label matching
- `negative_edges` in patterns — detect structural absence (pattern must NOT have this edge)
- DFS graph matching with per-antibody timeout budget (10ms / pattern, 100ms total scan)
- `extract_antibody_from_learn()` — auto-extract patterns from `m1nd.learn` feedback
- `compute_specificity()` — reject patterns too broad to be useful (MIN_SPECIFICITY=0.15)
- `pattern_similarity()` — duplicate detection at registration time (threshold=0.9)
- Persistence: `antibodies.json` alongside graph, atomic write with `.bak` backup
- Registry capacity: 500 antibodies max
- Severity levels: Critical / High / Medium / Low

#### Flow Simulation (`m1nd-core/src/flow.rs`)

Particle-based concurrent execution analysis. Launches simulated particles from entry
points and detects where concurrent paths collide.

- `FlowEngine` with configurable `FlowConfig` (max_depth, num_particles, turbulence_threshold)
- `TurbulencePoint` — race condition hotspot with `entry_pairs` attribution and path tracking
- `ValvePoint` — lock/bottleneck detection via label pattern matching
- `FlowEngine::discover_entry_points()` — auto-discover entry nodes from graph structure
- `scope_filter` — limit simulation to a subgraph region
- Hard caps: MAX_PARTICLES=100, MAX_ACTIVE_PARTICLES=10,000 total steps
- `M1ndError::NoEntryPoints` raised when graph has no identifiable entry points
- Turbulence severity: Critical / High / Medium / Low

#### Epidemic Prediction (`m1nd-core/src/epidemic.rs`)

SIR (Susceptible-Infected-Recovered) model for predicting how a bug in one module
propagates through the dependency graph.

- `EpidemicEngine` / `EpidemicConfig` / `EpidemicResult` / `EpidemicPrediction`
- `EpidemicDirection` enum: Forward / Backward / Both propagation
- Per-edge-type transmission coupling factors: imports=0.8, calls=0.7, inherits=0.6,
  references=0.4, contains=0.3
- Union probability combination across multiple paths to the same node
- `R0` (basic reproduction number) estimate in `EpidemicSummary`
- `unreachable_components` count — modules guaranteed safe from this seed
- Burnout detection: auto-calibrates infection rate when >80% of graph would be infected
- Dense graph node promotion via configurable `promotion_threshold`
- `EpidemicPersistentState` for disk persistence across sessions
- Hard cap: MAX_ITERATIONS=500; default: 50
- `M1ndError::EpidemicBurnout` — graph too densely connected for meaningful prediction
- `M1ndError::NoValidInfectedNodes` — seed nodes not found in graph

#### Code Tremors (`m1nd-core/src/tremor.rs`)

Second-derivative acceleration detection on edge weight time series. Like seismic
tremors as earthquake precursors — accelerating change frequency predicts instability.

- `TremorRegistry` ring buffer (256 observations per node)
- `TremorObservation` — timestamped weight delta recorded on every `learn` call
- `TremorWindow` enum: Days7 / Days30 / Days90 / All
- `TremorDirection` enum: Accelerating / Decelerating / Stable
- `RiskLevel` enum: Critical / High / Medium / Low / Unknown
- Magnitude formula: `|mean_acceleration| × sqrt(edge_events)`
- Linear regression slope for trend detection
- Risk classification: Critical = magnitude>5 AND slope>0.5
- `node_filter` parameter to scope analysis to a subgraph
- Minimum observation gap: 1 second (dedup interval)
- Persistence: `tremor_state.json` alongside graph

#### Module Trust Scores (`m1nd-core/src/trust.rs`)

Actuarial per-module defect density. Records confirmed bugs, false alarms, and partial
matches per node, then computes a time-weighted trust score with Bayesian adjustment.

- `TrustLedger` — defect history store
- `TrustEntry` — per-node defect data with timestamps
- `TrustScore` with `TrustTier`: HighRisk (<0.4) / MediumRisk (<0.7) / LowRisk (>=0.7)
- `record_defect()` / `record_false_alarm()` / `record_partial()` — feedback API
- `compute_trust()` — time-weighted density: `base × (FLOOR + (1-FLOOR) × recency)`
- `RECENCY_HALF_LIFE_HOURS=720` (30-day half-life), `RECENCY_FLOOR=0.3`
- `adjust_prior()` — Bayesian prior update; handles both positive and negative claims
- `report()` — full trust report with `min_history`, `tier_filter`, `sort_by` options
- `TrustSortBy`: TrustAsc / TrustDesc / DefectsDesc / Recency
- Cold-start default: 0.5 (neutral trust until evidence accumulates)
- Persistence: `trust_state.json` alongside graph

#### Architectural Layer Detection (`m1nd-core/src/layer.rs`)

Automatically assigns modules to architectural layers using Tarjan SCC + BFS longest-path
depth. Detects upward dependencies, circular dependencies, and skip-layer violations.

- `LayerDetector` with `LayerDetectionResult`
- `ArchLayer` — detected layer with node membership and health metrics
- `LayerViolation` with `ViolationType`: UpwardDependency / CircularDependency / SkipLayer
- `ViolationSeverity`: Critical / High / Medium / Low
- `UtilityNode` with `UtilityClassification`: CrossCutting / Bridge / Orphan
- `LayerHealth` — per-layer metrics including `layer_separation_score`
- `tarjan_scc()` — iterative (non-recursive) SCC to avoid stack overflow on deep graphs
- BFS longest-path depth assignment algorithm
- Layer merging for sparse layers (min 2 nodes per layer)
- Layer naming strategies: heuristic / path_prefix / pagerank
- `exclude_tests` and `node_type_filter` parameters
- `LayerCache` — detection results cached against graph generation counter
- Hard cap: DEFAULT_MAX_LAYERS=8
- `M1ndError::LayerNotFound` when requested layer index is out of range

#### Tree-sitter Tier 1 and Tier 2 (22 languages total)

Tree-sitter integration is no longer "planned" — it shipped. The default build
(`cargo build --release`) includes all 22 languages.

**Tier 1** (`--features tier1`) — 14 languages:
C/H, C++, C#, Ruby, PHP, Swift, Kotlin, Scala, Bash/Shell, Lua, R, HTML, CSS, JSON

**Tier 2** (`--features tier2`, default) — 8 additional languages:
Elixir, Dart, Zig, Haskell, OCaml, TOML, YAML, SQL

`TreeSitterExtractor` is a universal extractor driven by `LanguageConfig` structs.
Per-language configs specify `function_kinds`, `class_kinds`, `name_field`,
`alt_name_fields`, and the `name_from_first_child` flag for complex AST layouts.

Four-layer name extraction strategy for each definition: (1) `name_field` child,
(2) `alt_name_fields` fallback, (3) recursive declarator drill for C/C++,
(4) first named child scan for languages with `name_from_first_child=true`.

#### MemoryIngestAdapter (`m1nd-ingest/src/memory_adapter.rs`)

Turns markdown and plain text files into a queryable graph. Enables using m1nd as
an AI agent memory layer.

- Parses `.md`, `.markdown`, `.txt` (single file or directory walk)
- Configurable `namespace` parameter scopes all node IDs (default: `"memory"`)
- Section parsing: H1–H6 headings → `Module` nodes tagged `memory:section`
- Bullet parsing: `- / * / +` → `Concept` / `Process` nodes
- Checkbox parsing: `- [x] / - [ ]` → `Process` nodes tagged `memory:task`
- Table row parsing: `| col | col |` → nodes from joined cell text
- Entry classification by keyword: todo/task → task, decision/decided → decision,
  mode/state → state, meeting/session → event, default → note
- Canonical source detection: `YYYY-MM-DD.md`, `memory.md`, `*-active.md`,
  `*-history.md`, files containing `briefing` → `canonical=true` in provenance
- Cross-reference extraction: file paths in entry text → `Reference` nodes with
  `references` edges
- Code block skipping: fenced blocks are excluded from entry extraction
- File timestamp from filesystem metadata → temporal scoring dimension
- Node ID scheme: `memory::<namespace>::{file,section,entry,reference}::<slug>`
- Invoked via `m1nd.ingest` with `adapter: "memory"`

#### JsonIngestAdapter (`m1nd-ingest/src/json_adapter.rs`)

Escape hatch for any domain. Describe any graph as JSON and ingest it without writing
a custom adapter.

- Accepts a single JSON file: `{"nodes": [...], "edges": [...]}`
- Node fields: `id` (required), `label`, `type` (17 supported types), `tags`
- Edge fields: `source`, `target`, `relation`, `weight`
- Auto-assigned `causal_strength` by relation type
- `contains` relation → `EdgeDirection::Bidirectional` auto-promotion
- Invoked via `m1nd.ingest` with `adapter: "json"`

#### 15 Calibration Knobs

New tools expose agent-controllable parameters for tuning behavior without recompilation:

| Tool | Key Parameters |
|------|---------------|
| `antibody_scan` | `match_mode` (Exact/Substring/Regex), `min_severity` |
| `antibody_create` | `severity`, `description`, `tags` |
| `flow_simulate` | `num_particles`, `max_depth`, `turbulence_threshold`, `scope_filter` |
| `epidemic` | `iterations`, `direction` (Forward/Backward/Both), `promotion_threshold` |
| `tremor` | `window` (Days7/Days30/Days90/All), `node_filter`, `min_magnitude` |
| `trust` | `min_history`, `tier_filter`, `sort_by`, `half_life_hours` |
| `layers` | `exclude_tests`, `node_type_filter` |

#### HTTP Server + Embedded GUI (`--features serve`)

Optional feature flag adds an axum HTTP server and embedded React UI.
Build with `cargo build --release --features serve`.

Modes:
- `m1nd-mcp --serve` — HTTP server + embedded UI on port 1337 (default)
- `m1nd-mcp --serve --stdio` — Both transports simultaneously. SSE cross-process bridge: stdio
  and HTTP share the same graph state. SSE `/api/events` endpoint streams tool results to
  browser in real time.
- `m1nd-mcp --serve --dev` — HTTP with frontend served from `m1nd-ui/dist/` on disk (supports
  Vite HMR during UI development)
- `m1nd-mcp --serve --open` — HTTP + auto-open browser on launch
- `m1nd-mcp --serve --stdio --event-log /tmp/e.jsonl` — Option A+B: in-process broadcast +
  append-to-file event log for external consumers

HTTP API endpoints:
- `GET /api/health` — server health: node/edge counts, domain, uptime, query count
- `GET /api/tools` — full tool schema list (same as MCP `tools/list`)
- `POST /api/tools/{tool_name}` — invoke any of the 52 tools via REST (30s timeout, FM-C-004)
- `GET /api/graph/stats` — node/edge counts, domain, namespaces
- `GET /api/graph/subgraph?query=<q>&top_k=<n>` — activate + return subgraph for visualization
- `GET /api/graph/snapshot` — full graph dump (nodes + edges) for external export
- `GET /api/events` — SSE stream of tool results (event_type, data, timestamp_ms)

Cross-process SSE bridge: stdio MCP clients (Claude Code, Cursor) and the browser UI can share
the same graph state via event log (`--event-log`) and watch (`--watch-events`). Each tool call
from either transport is broadcast to all SSE subscribers.

Body limit: 1MB per tool call (FM-A-004). Request timeout: 30s (FM-C-004). CORS: permissive
(disable in production). Binding to `0.0.0.0` emits a network exposure warning.

#### Other Additions

- `DomainConfig` multi-domain system — `code`, `music`, `memory`, `generic` presets,
  each with different temporal decay half-lives and co-change behavior
- `GraphBuilder` fluent API for programmatic graph construction in m1nd-core
- `M1ND_DOMAIN` env var and `domain` config file field
- Config file via CLI arg: `./m1nd-mcp config.json` (first argument, JSON)
- MCP instructions injection on `initialize` — 73-line workflow guide injected into
  the MCP handshake response so clients automatically understand usage patterns

### Fixed

- Epidemic burnout on dense graphs: auto-calibrate infection rate when >80% saturation
  rather than hard-failing
- Antibody `match_mode` now propagated correctly through recursive DFS subgraph matching
- Flow simulation enforces `max_depth` and `max_total_steps` hard caps independently
  (previously max_depth could be bypassed by particle branching)
- Tool dispatch normalization (underscore ↔ dot) now applies uniformly to all 52 tools
  including the 9 new ones; previously new tools required exact dot notation
- Lock `watch` strategy validation rejects `"periodic"` with
  `M1ndError::WatchStrategyNotSupported` instead of silently accepting and never firing
- `lock.diff` correctly drains watcher event queue before computing delta
- Peek security allowlist enforced for all perspective branches, not only the root
  perspective (previously branched perspectives bypassed the ingest-scope check)
- `GraphDiff` incremental mode counts `RemoveNode` / `RemoveEdge` actions in stats
  even though CSR does not physically remove them (clarified behavior, no silent drop)

### Changed

- README tool count updated from 43 to 52
- Default build now includes Tier 2 tree-sitter languages (`default = ["tier2"]`)
- `SNAPSHOT_VERSION` bumped to 3; `load_graph()` performs version migration on older files
- `resonate` output now includes all 5 fields: `harmonics`, `sympathetic_pairs`,
  `resonant_frequencies`, `wave_pattern`, `harmonic_groups`
- `counterfactual` output includes `synergy_factor` when >1 node removed, and
  `reachability_before` / `reachability_after` metrics
- Ingest response includes `commit_groups` in `IngestStats` (was populated but not
  surfaced in the JSON response)

---

## [0.1.0] — initial release

Foundation release: 43 MCP tools across Foundation (13), Perspective Navigation (12),
Lock System (5), and Superpowers (13) categories. Hebbian plasticity, spreading
activation, XLR noise cancellation, trail system, hypothesis engine, counterfactual
engine. Native extractors for Python, Rust, TypeScript/JavaScript, Go, Java.
