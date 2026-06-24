# Agent Tasknotes

Operational notes from real agent usage of `m1nd`.

This file is not a polished roadmap. It is a running capture surface for
moments where an agent used `m1nd`, did not get the exact answer it needed,
and had to compensate.

The rule:

- if an agent reaches for shell/tools outside `m1nd` because the current
  surface could not answer the real task directly, add a short note here
- keep notes concrete and falsifiable
- prefer one note per friction point
- when a note is resolved by code, move it to the resolved section with the
  commit/PR reference

## Open Notes

### 2026-06-24 — OPTIONAL real local embeddings for `seek` (first cut, OFF by default)

- Context: `seek`'s "semantic" slot was character-trigram TF-IDF over labels —
  it cannot match intent queries whose words never appear in the node label.
- First cut: a cargo feature `embed` (OFF by default; `m1nd-core/embed`,
  forwarded by `m1nd-mcp/embed`) wires FAST STATIC embeddings via `model2vec-rs`
  (`potion-base-8M`, ~29 MB, MIT, L2-normalized; output dim probed at load).
  Model chosen by deep research as the leanest all-rounder serving BOTH code and
  l1ght prose in one space; `M1ND_EMBED_MODEL` overrides (e.g. potion-code-16M for
  code-max, potion-multilingual-128M for non-English). `SemanticEngine::build`
  computes a per-node embedding side-map over `label + provenance.excerpt`;
  `seek` embeds the query once and blends `sem = 0.7*cosine + 0.3*legacy_trigram`
  for Phase-1 survivors. `embeddings_used` in the response is now TRUTHFUL.
- HONESTY: these are STATIC embeddings — a real upgrade over label-only
  trigrams, but NOT transformer-grade (no attention/contextualization). The
  ONNX/bge tier is the path for maximal quality. No string should imply
  transformer semantics.
- Deferred: embedding AST body text (not just the
  excerpt); and the ONNX/transformer quality tier (runner-up: jina-v2-base-code
  pure-Rust via candle, the right upgrade once nodes carry real bodies + an ANN
  index). The model is gitignored under `m1nd-core/assets/`; it is
  downloaded-on-first-use from Hugging Face (`DEFAULT_HF_REPO`) or pointed at a
  vendored copy via `M1ND_EMBED_MODEL` — no model blob is committed.

### 2026-05-05 — scope normalization failures should prefill doctor recovery too

- Context: `seek`, `search`, and `activate` now attach `graph_state` and a
  ready `doctor` recovery payload when retrieval returns `blocked` or zero
  actionable candidates.
- Remaining friction: hard parameter errors, such as ambiguous auto-ingest
  scopes, still return normal invalid-param recovery text rather than a
  structured `doctor` payload.
- Desired behavior:
  - invalid-param retrieval errors include compact graph/session breadcrumbs
    when the active graph state is relevant
  - scope/path normalization failures point at `doctor` with prefilled observed
    fields

## Resolved Notes

### 2026-06-24 — embeddings now persist in a content-addressed cache (was: recomputed every build)

- Context: with `embed` on, `SemanticEngine::build` recomputed every node's
  static embedding on every boot and re-ingest — a model load + N encodes each
  time, the main cost of keeping `embed` warm.
- Resolution: a new `embed_cache.rs` sidecar (`EmbeddingCache`, bincode, atomic
  temp+rename) keyed by a STABLE FNV-1a hash of `(model_id, label+excerpt)`.
  `SemanticEngine::build_with_cache(graph, weights, cache_path, persist)` loads
  the warm cache and reuses any vector whose `(model, text)` is unchanged,
  re-embedding only changed/new nodes, then persists EXACTLY the current graph's
  entries (self-pruning — no stale / cross-repo growth). The in-memory map stays
  keyed by `NodeId`, so the `seek` path is untouched. The cache lives next to the
  snapshot in `runtime_dir`, derived in `SessionState::initialize` and refreshed
  by `rebuild_engines`. Advisory: any version/model/dim mismatch or corruption →
  full recompute (never a wrong vector). `model_id` folds the weights-file byte
  length, so an in-place model swap invalidates the cache. Read-only attachers
  REUSE the cache but never write it (`persist=false`) — one writer per file,
  honoring the read-only contract. All OFF by default behind `embed`.
- Still deferred: NodeStorage SoA embedding fields + an ANN index over the
  persisted vectors (the next unlock — vector search over the whole graph).

### 2026-05-06 — agents need one startup verdict, not stitched diagnostics

- Context: `health`, `session_handshake`, and `recovery_playbook` exposed the
  right primitives, but every agent still had to remember how to compose them
  when a host binding was partial or retrieval looked stale.
- Resolution: `trust_selftest` now returns `m1nd-trust-selftest-v0` with a
  single verdict (`full_trust`, `needs_ingest`, `orientation_only`,
  `degraded_host_tool_surface`, or `stale_binding_suspected`), the binding
  fingerprint, graph state, embedded session handshake, and an optional
  recovery playbook. It is diagnostic-only: no ingest, repair, probe, or
  mutation happens automatically.
- Follow-up: agents should call `trust_selftest` first when the live MCP
  surface exposes it, fall back to `session_handshake` when it does not, and use
  `recovery_playbook` only when the verdict is not `full_trust` or retrieval
  evidence is suspicious.

### 2026-05-05 — session startup needs a cheap trust handshake

- Context: after adding `doctor` and degraded-surface diagnostics, the remaining
  risk was making every agent run a heavy smoke or repeated retrieval probes at
  the start of a session.
- Resolution: `scripts/mcp_agent_smoke.py` now has `--handshake-only`, returning
  `m1nd-session-handshake-v0` with `trust_mode`, `can_ingest`, `can_retrieve`,
  `can_recover`, tool-surface status, health summary, and next action. It calls
  `doctor` only under suspicion, and `--handshake-probe` is opt-in.
- Follow-up: the same contract is now exposed as the official MCP
  `session_handshake` tool. The smoke harness calls the tool when the live
  surface supports it, and falls back to its local implementation for older
  binaries.

### 2026-05-05 — recovery should be in-band, not chat memory

- Context: `session_handshake` could classify a session as trusted, needing
  ingest, orientation-only, or degraded, but agents still needed remembered
  operator doctrine to decide the next safe action.
- Resolution: `recovery_playbook` now returns `m1nd-recovery-playbook-v0` with
  ordered recovery steps plus `m1nd-binding-fingerprint-v0`. The playbook is
  diagnostic-only: no ingest, repair, probe, or filesystem mutation happens
  automatically.

### 2026-05-05 — host MCP surfaces can hide required recovery tools

- Context: another agent saw m1nd through a host-provided surface where `audit`
  was available but `ingest` was not exposed.
- Friction: without `ingest`, an agent cannot repair, refresh, or prove the
  active graph from inside that host session, even if other m1nd tools respond.
- Resolution: `doctor` now accepts `observed_tool_count`, `available_tools`, and
  `missing_tools` from `tools/list`, flags
  `diagnostics.degraded_host_tool_surface`, and tells the agent to treat m1nd as
  orientation-only until the MCP binding is refreshed. The smoke harness also
  emits structured `degraded_host_tool_surface` details instead of a bare
  missing-tool string.
- Follow-up: `health` now exposes `m1nd-tool-surface-contract-v0` and
  `m1nd-host-binding-alignment-v0`, so even a partial host that only exposes
  `health` can tell agents which tools should be visible and when the host is
  showing a degraded binding.

### 2026-05-05 — retrieval responses need inline active-graph breadcrumbs

- Context: the new `doctor` tool could diagnose active graph/runtime/session
  state after a stale-looking retrieval, but the retrieval response itself did
  not include enough continuity breadcrumbs for instant self-recovery.
- Resolution: `seek`, `search`, and `activate` now include a compact
  `graph_state` plus a ready `recovery.suggested_tool=recovery_playbook`
  payload when they return `blocked` or zero actionable candidates. The smoke
  harness also runs a negative seek after populated ingest to prove the
  recovery payload and playbook exist.

### 2026-05-05 — injected MCP surface can lose graph state across ingest and seek

- Context: real agent smoke from a host-provided `m1nd` MCP surface after
  re-ingesting the current repo.
- Friction: `ingest` returned a populated graph, but immediate `seek` calls
  returned `proof_state=blocked` with `total_candidates_scanned=0`, forcing the
  agent back to shell search for repo orientation.
- Cross-check: the local stdio binary on the same checkout exposed the full
  tool surface and `ingest -> seek` scanned the expected graph candidates.
- Resolution: `doctor` now distinguishes empty graph, populated graph with
  blocked/zero-candidate retrieval, missing ingest roots, unknown workspace
  root, session gaps, and host-binding split-brain clues. The repo-local smoke
  harness now proves `initialize/tools -> ingest -> seek -> help -> doctor`
  over stdio and HTTP. HTTP tool dispatch also tracks `agent_id` before calling
  the shared dispatcher, matching stdio session semantics more closely.

### 2026-04-05 — proactive write insights still lack runtime-backed mismatch signals

- Context: first proactive-insight slice on `apply` / `apply_batch`
- Friction: the system now emits useful structural guidance after writes, but it
  still does not compare those edits against runtime overlay evidence
- Desired behavior:
  - `runtime_hotspot_mismatch`
  - stronger cross-repo contract drift when federation is active
  - per-insight latency budgets in benchmarks

### 2026-04-05 — daemon can tick for delta ingest, but it still is not a background watcher

- Context: daemon control plane + explicit `daemon_tick`
- Friction: the daemon can now poll watched roots and incrementally re-ingest
  changed files, but it still needs an explicit tick call instead of a true
  background filesystem / SCM watcher
- Desired behavior:
  - autonomous background ticks
  - watchman/native notify acceleration
  - latency budgets for `single-file changed -> alert available`

### 2026-04-05 — `audit` still composes more than it understands

- Context: first implementation of `m1nd.audit`
- Friction: `audit` is already useful, but it is still mainly an orchestrator
  over existing tools, not yet a deep profile-specialized intelligence pass
- Desired behavior:
  - richer per-profile recommendations
  - stronger `coordination` semantics around docs/config/reference truth
  - stronger `production` semantics around runtime risk ranking
- Likely next step: strengthen the profile registry so `audit` changes not only
  tool selection but also grading, recommendation logic, and narrative output

### 2026-04-05 — `federate_auto` now covers explicit paths, manifests, imports, route-level API matches, contract artifacts, and schema/component names, but deeper service discovery is still missing

- Context: `federate_auto` now bridges explicit path evidence, manifest/workspace
  signals, package/import identity matches, shared `/api/...` routes, and basic
  contract artifacts into repo candidates, namespaces, and optional one-shot federation
- Friction: repos that are only implied by multi-artifact service contracts,
  behavior-level protocol flow, or deeper schema structure still need manual enumeration
- Desired behavior:
  - stronger schema discovery beyond token-level message/component/operation matches
  - future semantic linking donor lane (stack-graphs / SCIP-class ideas)

### 2026-04-05 — `coverage_session` is useful but still shallow

- Context: real MCP smoke using `search`, `batch_view`, and `audit`
- Friction: coverage answers “what did I touch?” but not “what important area
  did I still miss?” or “what should I inspect next?”
- Desired behavior:
  - importance-weighted unread files
  - estimated remaining exploration
  - per-tool coverage contribution

## Resolved Notes

### 2026-04-05 — `federate_auto(scope="docs")` was polluted by non-doc semantic signals

- Context: real `execute=true` smoke on the live `m1nd` repo
- Root cause: semantic discovery lanes ignored the requested `scope`, so
  import/API heuristics could outrank the explicit doc evidence and surface
  nearby worktrees instead of the external repos actually referenced in docs
- Resolution: semantic discovery now respects scope and candidate ranking now
  prefers stronger evidence families over weaker heuristic matches
- Landed in branch: `codex/m1nd-federate-field-hardening`

### 2026-04-05 — `cross_verify` counted symbol nodes as missing files

- Context: real MCP smoke on the `m1nd` repo returned a wildly inflated
  `stale_confidence`
- Root cause: `cross_verify` treated all `file::...` IDs as file nodes,
  including symbol IDs like `file::...::fn::...`
- Resolution: restricted graph-vs-disk comparison to `NodeType::File`
- Landed in branch: `codex/m1nd-audit-epic`
- Commit: `8b7f276`

### 2026-04-05 — `audit` replace-mode inherited stale ingest roots

- Context: sequential audit smokes across different repos
- Root cause: `replace` ingest kept older `ingest_roots`, contaminating later
  path normalization and inventory
- Resolution: replace-mode now resets active ingest roots and updates
  `workspace_root`
- Landed in branch: `codex/m1nd-audit-epic`
- Commit: `8b7f276`

### 2026-04-05 — repo-wide isolation hypotheses returned weak evidence

- Context: the field report asked for orphan enumeration instead of
  `inconclusive`
- Root cause: `hypothesize` only handled unary isolated claims after resolving a
  concrete subject node
- Resolution: added repo-wide isolated/orphan claim parsing and degree-0 file
  enumeration
- Landed in branch: `codex/m1nd-audit-epic`
- Commit: `e9d444d`
