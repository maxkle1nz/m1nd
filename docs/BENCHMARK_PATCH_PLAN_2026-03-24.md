# m1nd Next Patch Plan

Date: 2026-03-24
Repo: `/Users/cosmophonix/SISTEMA/.codex-tmp/m1nd-standalone`
Source: benchmark findings from `docs/BENCHMARK_RESEARCH_2026-03-24.md`

## Goal

Turn benchmark findings into a concrete patch queue that improves:

- warm-graph time to useful answer
- payload compactness
- proof quality
- investigation continuity
- trust in planning and retrieval surfaces

This is not a release note. It is an execution backlog.

## Progress Snapshot

Implemented on `codex/benchmark-research-and-timeline-p1`:

- `timeline` path canonicalization across equivalent external-id shapes
- `apply_batch` verification parity via `heuristics_surface_ref`
- explanation fallback preservation when post-write node resolution is weak
- `validate_plan` suppression of common manifest/artifact noise
- `surgical_context_v2` prioritization of code neighbors over docs
- `seek` natural-language tokenization improvements
- `surgical_context_v2.proof_focused` for smaller connected proof bundles
- automatic structural boost derivation in `trail_save`
- structural node reactivation from derived boosts in `trail_resume`
- literal search demotion of fixture-like hardcoded identity noise

Still open after this pass:

- measure the practical effect of `proof_focused` in warm-graph edit-prep scenarios
- deepen `trail_resume` so saved investigations reopen the next useful question, not only the boosted graph state
- measure whether literal-search continuity now needs fewer reformulations in warm-graph runs

## Priority 1

### 1. Fix recent-history fidelity in `timeline`

Problem:
- `timeline` localizes the right file but can miss the recent commit that actually explains a bug or hardening change.

Why it matters:
- warm `m1nd` is already strong at fast localization
- it still loses to manual `git show` on full proof in failure triage

Likely files:
- `m1nd-mcp/src/layer_handlers.rs`
- `m1nd-mcp/src/protocol/layers.rs`

Expected impact:
- faster `time_to_full_proof` in root-cause scenarios
- less fallback to manual git archaeology

Acceptance criteria:
- recent file-changing commits surface consistently for a modified file
- a failure triage scenario can recover both localization and recent proof path through `m1nd`

### 2. Canonicalize file identities across `search`, `activate`, and `timeline`

Problem:
- the same file can surface under multiple external-id shapes
- this likely hurts ranking, dedupe, and historical lookup

Likely files:
- `m1nd-mcp/src/search_handlers.rs`
- `m1nd-mcp/src/layer_handlers.rs`

Expected impact:
- cleaner ranking
- better history joins
- fewer confusing duplicates in result sets

Acceptance criteria:
- the same file uses one canonical identity through retrieval, history, and activation flows
- search and timeline results dedupe correctly on the same file

### 3. Add `heuristics_surface_ref` parity to `VerificationImpact`

Problem:
- `apply_batch` verification can surface heuristic summaries, but not the same explorable reference affordance used by analogous plan/report flows

Likely files:
- `m1nd-mcp/src/protocol/surgical.rs`
- `m1nd-mcp/src/surgical_handlers.rs`

Expected impact:
- stronger proof and explanation path in verification flows
- better parity across related APIs

Acceptance criteria:
- verification hotspot payload includes an explorable reference when a heuristic summary exists
- tests cover parity between verification and validate-plan/report flows

### 4. Preserve explanation when `node_id` resolution fails after write

Problem:
- hotspot explanation can disappear when post-write graph resolution fails

Likely files:
- `m1nd-mcp/src/surgical_handlers.rs`

Expected impact:
- more stable verification output
- less “summary vanished” behavior in exactly the scenarios where the graph is under stress

Acceptance criteria:
- fallback explanation still exists when `node_id` is absent
- file-path-based or equivalent fallback is covered by tests

## Priority 2

### 5. Reduce `validate_plan` noise for API/protocol edits

Problem:
- `validate_plan` can pull unrelated `Cargo.toml` files and low-value artifacts into otherwise narrow patches

Likely files:
- `m1nd-mcp/src/layer_handlers.rs`

Expected impact:
- smaller payloads
- higher trust in plan output
- better pre-edit signal-to-noise ratio

Acceptance criteria:
- narrow API/protocol change scenarios prioritize directly impacted implementation, protocol, tests, and user-facing docs
- unrelated workspace files are demoted or excluded by default

### 6. Add a tighter proof-focused mode to `surgical_context_v2`

Problem:
- `surgical_context_v2` often finds the right neighborhood, but with more payload than needed

Likely files:
- `m1nd-mcp/src/surgical_handlers.rs`
- `m1nd-mcp/src/protocol/surgical.rs`

Expected impact:
- lower token footprint in warm edit-prep flows
- smaller proof sets for benchmark and interactive use

Acceptance criteria:
- a narrow mode can return primary file + strongest connected proof set without flooding the agent
- warm edit-prep token proxy drops meaningfully in the normalized-scope scenario

### 7. Improve `seek` for natural-language prompts

Problem:
- `seek` works much better for short, code-shaped prompts than for natural phrasing

Likely files:
- `m1nd-core` query/ranking path
- `m1nd-mcp/src/search_handlers.rs`

Expected impact:
- better first-shot retrieval
- less prompt reformulation by the agent

Acceptance criteria:
- long natural-language prompts retrieve the same top target as short “code-shaped” prompts in warm semantic scenarios
- benchmarked `search_iterations` decrease

### 8. Add stronger semantic bias for alias/canonical/dispatch questions

Problem:
- retrieval around alias normalization still depends too much on exact prompt wording

Likely files:
- `m1nd-core`
- `m1nd-mcp/src/search_handlers.rs`

Expected impact:
- better semantic routing to normalization helpers
- more robust warm retrieval in dispatch and alias questions

Acceptance criteria:
- `seek` reliably ranks `normalize_dispatch_tool_name` on relevant prompts without needing manual reformulation

## Priority 3

### 9. Make `trail_save` and `trail_resume` structurally stronger

Problem:
- continuity behaves more like a bookmark than a true structural resume unless explicit nodes are supplied

Likely files:
- `m1nd-mcp/src/layer_handlers.rs`
- `m1nd-mcp/src/protocol/layers.rs`

Expected impact:
- fewer rediscovery loops
- stronger continuity value in real agent sessions

Acceptance criteria:
- saved trails automatically preserve and reactivate useful structural nodes
- `trail_resume` can reduce repeat searches in multi-step investigations

### 10. Add a continuity-oriented resume mode

Problem:
- after resuming, the agent still needs to formulate multiple new searches to continue answering open questions

Likely files:
- `m1nd-mcp/src/server.rs`
- `m1nd-mcp/src/layer_handlers.rs`

Expected impact:
- better “continue from where we left off” flow
- less manual query formulation after resume

Acceptance criteria:
- a resumed investigation can answer the next open question with fewer retrieval steps

### 11. Reduce noisy literal-search fixture contamination

Problem:
- literal search can surface fixture-like or hardcoded identity noise in continuity scenarios

Likely files:
- `m1nd-mcp/src/search_handlers.rs`

Expected impact:
- cleaner continuity and seam-finding flows
- less false trust erosion from noisy matches

Acceptance criteria:
- real implementation seams outrank fixture-like artifacts for direct seam queries

## Suggested Execution Order

1. `timeline` fidelity + canonical identity normalization
2. `VerificationImpact` parity + fallback explanation preservation
3. `validate_plan` noise reduction
4. `surgical_context_v2` proof-focused mode
5. `seek` ranking improvements
6. `trail_resume` structural continuity improvements

## Suggested Benchmark Gates

After each patch group, rerun one representative benchmark:

- root-cause triage for `timeline`
- `apply_batch` explanation gap for verification parity
- normalized-scope edit prep for `validate_plan` and `surgical_context_v2`
- alias normalization retrieval for `seek`
- boot-memory continuity for `trail_resume`

No patch should claim success without improving at least one of:

- `time_to_first_good_answer`
- `time_to_full_proof`
- `token_proxy`
- `repeat_reads`
- `search_iterations`
