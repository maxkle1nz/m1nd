# Changelog

All notable changes to m1nd are documented here. This project uses [Semantic Versioning](https://semver.org/).

---

## [Unreleased]

The next release train starts here.

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
