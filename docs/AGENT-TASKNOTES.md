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

### 2026-04-05 — `federate_auto` now covers explicit paths, manifests, imports, route-level API matches, and basic contract artifacts, but deeper schema/service discovery is still missing

- Context: `federate_auto` now bridges explicit path evidence, manifest/workspace
  signals, package/import identity matches, shared `/api/...` routes, and basic
  contract artifacts into repo candidates, namespaces, and optional one-shot federation
- Friction: repos that are only implied by richer schemas, operation groups,
  or indirect service contracts still need manual enumeration
- Desired behavior:
  - richer OpenAPI/schema discovery beyond simple `operationId` or route matching
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
