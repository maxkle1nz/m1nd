# Changelog

All notable changes to m1nd are documented here. This project uses [Semantic Versioning](https://semver.org/).

## [0.6.2] — 2026-03-26

### Added

#### Universal Knowledge Connectome

The ingestion pipeline now supports scholarly and standards documents alongside code:

- **Patent adapter** (`patent`): USPTO/EPO XML with claims, citations, and assignee extraction
- **JATS adapter** (`article`): PubMed NLM / JATS Z39.96 scientific article XML
- **BibTeX adapter** (`bibtex`/`bib`): BibTeX bibliography file ingestion
- **RFC adapter** (`rfc`): IETF RFC XML v3 with section-level granularity
- **CrossRef adapter** (`crossref`/`doi`): CrossRef API JSON for DOI metadata

#### Document Router (Auto-Detection)

`DocumentRouter` auto-detects file format by extension and content heuristics.
Used via `m1nd.ingest(adapter="auto")`.

#### Cross-Domain Resolution — 6 Bridge Strategies

`CrossDomainResolver` merges outputs from multiple adapters and discovers connections:

| Bridge | Weight | Discovery |
|--------|--------|-----------|
| `same_as` | 1.0 | Shared DOI/PMID identity |
| `cross_cites` | 0.95 | Cross-domain citation targets |
| `same_orcid` | 0.95 | Researcher identity via ORCID |
| `same_author` | 0.7 | Name matching across namespaces |
| `shared_keyword` | 0.6 | Topic clustering via keyword/subject tags (≤20 cap) |
| `citation_chain` | 0.5 | Transitive A→B→C bridging |

All bridges are cross-domain only — same namespace never bridges.

### Changed

- README hero stats added to all 7 translated READMEs
- `.gitignore` updated to exclude `.DS_Store` and `*.bak` files
- Architecture docs updated with all new adapters, router, and bridges

### Removed

- `.DS_Store` and `antibodies.json.bak` removed from repository

### Stats

- 10 ingestion adapters registered in MCP CLI
- 131+ tests passing
- 3/3 domains validated bridging via shared DOIs (RFC × CrossRef × BibTeX)

---

## [0.6.1] — 2026-03-25

### Fixed

#### Release and Publish Alignment

This patch release aligns the public release surfaces after the `v0.6.0` rollout.

- workspace crates now include the missing crates.io metadata needed for clean publish
- internal workspace dependencies now use explicit published-version constraints
- the release workflow now skips crates.io publish cleanly when `CARGO_REGISTRY_TOKEN` is missing instead of failing the full release

## [0.6.0] — 2026-03-25

### Added

#### Guided Proof State Across Core Agent Flows

Several high-value tools now surface `proof_state` plus explicit handoff guidance so an agent can tell whether it is still triaging, actively proving, or ready to move into edit preparation.

- `seek`, `trace`, `impact`, `timeline`, `hypothesize`, `validate_plan`, and `surgical_context_v2` now participate in a shared proof-state model
- guided outputs now include `next_suggested_tool`, `next_suggested_target`, and `next_step_hint` across the main structural triage and edit-prep paths
- `trail_resume` now behaves more like continuity orchestration than bookmark restore, returning compact resume hints, next-focus guidance, and tool-aware follow-up

#### `apply_batch` Progress, Correlation, and Handoff Signals

`apply_batch` has been upgraded from a “wait until the batch finishes” write surface into an observable execution flow with stable correlation and final handoff data.

- final outputs now expose `batch_id` for correlating progress and final result
- progress reporting now includes coarse lifecycle fields such as `active_phase`, `completed_phase_count`, `phase_count`, `remaining_phase_count`, `progress_pct`, and `next_phase`
- `phases` now act as a structured execution timeline across `validate`, `write`, `reingest`, `verify`, and `done`
- `progress_events` now provide a streaming-friendly event log for the same lifecycle
- live `apply_batch_progress` SSE emission now happens during execution in serve mode
- the final `batch_completed` event now carries the batch’s `proof_state` and next-step guidance

#### Benchmark Harness Expansion

The benchmark system has been extended so progress UX, guidance, and repair loops can be measured as first-class product behavior, not only token proxy.

- benchmark runs now record `execution_origin` and `source_ref`
- long-running flows can distinguish `live`, `replay`, and `snapshot` progress delivery
- the harness records progress event counts, proof-state transitions, recovery loops, and guidance-followed behavior
- the current aggregate warm-graph corpus stands at `10518 -> 5182`, or `50.73%` savings, while also reducing `false_starts` from `14` to `0`

### Changed

#### Help and Docs Are More Agent-Operational

- help entries now include `WHEN TO USE`, `AVOID WHEN`, benchmark-aware guidance, composed workflows, and proof-state handoff cues
- common tool failures are now framed as repair loops with hint/example/next-step guidance
- README, examples, benchmark docs, and public landing surfaces now describe the current guided runtime more accurately

---

## [0.5.1] — 2026-03-24

Two-phase transactional editing for LLM agents. Preview before you write.

### New Tools

- **`edit_preview`**: Build an in-memory preview of a single-file edit without touching disk. Returns preview handle, source snapshot (hash, bytes, line count), unified diff, and validation report (empty content check, noop detection, ready-to-commit flag).
- **`edit_commit`**: Commit a previously created preview to disk. Three safety guards:
  - **Confirm guard**: `confirm` must be explicitly set to `true` (default `false`).
  - **TTL**: Previews expire after 5 minutes. Garbage-collected on access.
  - **Source hash verification**: Re-reads the file at commit time; rejects if it changed since preview.

### Technical Details

- `EditPreviewState` stored in `SessionState` with agent isolation (agent_id must match).
- Preview handles are single-use — consumed on successful commit.
- Delegates to existing `handle_apply()` path for actual disk write + graph re-ingest.
- 7 new integration tests covering: happy path, nonexistent file, commit, TTL expiry, source tampering, confirm guard, invalid handle.
- Help system updated with ToolDoc entries and suggest_next chains.

### Stats

- 77 MCP tools total on the current audit branch (was 71 before the audit/session additions).
- 342 tests all passing.

---

## [0.1.0] — Initial Release

The first public release of m1nd: a graph-grounded code intelligence engine with Hebbian plasticity, spreading activation, and 43 MCP tools. Built in Rust.

### Core Engine (m1nd-core)

- **Compressed Sparse Row (CSR) graph** with forward and reverse adjacency
- **PageRank computation** on ingest
- **4-dimensional spreading activation**: structural, semantic, temporal, causal
- **Hebbian plasticity**: Long-Term Potentiation (LTP), Long-Term Depression (LTD), homeostatic normalization
- **XLR differential processing**: noise cancellation inspired by balanced audio cables
- **Hypothesis engine**: claim testing with Bayesian confidence on graph paths
- **Counterfactual engine**: module removal simulation with cascade analysis
- **Structural hole detection**: topology-based gap analysis
- **Resonance analysis**: standing wave computation for structural hub identification
- **Fingerprint engine**: activation fingerprinting for structural twin detection
- **Trail system**: investigation state persistence, resume, and multi-trail merge with conflict detection
- **Lock system**: subgraph pinning with sub-microsecond diff (0.08us)
- **Temporal engine**: co-change history, velocity scoring, decay functions
- **Domain configurations**: code, music, memory, generic presets with tuned decay half-lives

### Ingest Layer (m1nd-ingest)

- **Language extractors**: Python, Rust, TypeScript/JavaScript, Go, Java
- **Generic fallback extractor**: heuristic-based for unsupported languages
- **JSON adapter**: structured data ingestion
- **Memory adapter**: text corpus ingestion
- **Reference resolver**: cross-file import and call resolution
- **Incremental ingest**: re-process only changed files
- **Multi-repo federation**: unified graph with automatic cross-repo edge detection

### MCP Server (m1nd-mcp)

- **43 MCP tools** across 7 layers:
  - Foundation (13): activate, impact, missing, why, learn, drift, health, seek, scan, timeline, diverge, warmup, federate
  - Perspective Navigation (12): start, routes, follow, back, peek, inspect, suggest, affinity, branch, compare, list, close
  - Lock System (5): create, watch, diff, rebase, release
  - Superpowers (13): hypothesize, counterfactual, predict, fingerprint, resonate, trace, validate_plan, differential, trail.save, trail.resume, trail.merge, trail.list, seek
- **JSON-RPC over stdio**: compatible with MCP protocol version 2024-11-05
- **Dual transport**: framed (Content-Length headers) and line-delimited JSON-RPC
- **Auto-persistence**: configurable interval (default: every 50 queries) + on shutdown
- **Multi-agent support**: agent ID tracking, perspective isolation, shared graph
- **Tool name normalization**: underscores automatically converted to dots (e.g., `m1nd_activate` -> `m1nd.activate`)

### Performance (measured on 335-file Python backend, ~52K lines)

- Full ingest: 910ms (9,767 nodes, 26,557 edges)
- Spreading activation: 31-77ms
- Blast radius: 5-52ms
- Counterfactual: 3ms
- Hypothesis testing: 58ms (25,015 paths)
- Lock diff: 0.08us
- Trail merge: 1.2ms
- Memory footprint: ~50MB typical

### Known Limitations

- Semantic scoring uses trigram matching, not neural embeddings (planned for v0.2)
- No tree-sitter integration yet (planned for v0.2)
- 6 languages with dedicated extractors; others use generic fallback
- Graph is fully in-memory; very large codebases (400K+ files) need ~80MB
- No dataflow or taint analysis (out of scope; use dedicated SAST tools)

---

## Planned: v0.2.0

- Tree-sitter integration for 64+ language support
- Optional embedding-based semantic scoring
- Graph partitioning for very large codebases
- Community detection algorithms
- Performance optimizations for 100K+ node graphs
- MCP Streamable HTTP transport (in addition to stdio)

---

## Planned: v0.3.0

- Distributed graph (multi-machine federation)
- Real-time file watcher integration
- Plugin system for custom extractors and tools
- Graph visualization export (DOT, D3.js, Mermaid)
- Metrics and observability (Prometheus, OpenTelemetry)
