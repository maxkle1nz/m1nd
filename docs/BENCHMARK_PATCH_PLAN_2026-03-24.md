# m1nd Next Patch Plan

Date: 2026-03-24
Repo: `<path-to-local-m1nd-repo>`
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
- explicit `reactivated_node_ids` and `resume_hints` in `trail_resume`
- compact limits for `trail_resume` preview fields
- literal search demotion of fixture-like hardcoded identity noise
- initial benchmark harness with scenario/event/run JSON support
- aggregate summary support and workflow metadata capture for false starts, test awareness, and workflow notes

Still open after this pass:

- measure the practical effect of `proof_focused` in warm-graph edit-prep scenarios
- validate whether the new `trail_resume` hints keep winning once timings are captured less synthetically
- validate whether compact `trail_resume` stays equally useful in longer real investigations, not just the starter continuity scenario
- measure whether literal-search continuity now needs fewer reformulations in warm-graph runs
- tune ranking quality of `next_focus_node_id` and `next_suggested_tool` in longer investigations
- add optional public cost/time-value projections without hardcoding one provider assumption

Harness status update:

- run recording exists
- aggregate summarization now exists
- starter corpus now covers retrieval, continuity, edit-prep, and structural proof
- next missing step is a less synthetic corpus with repeatable event capture for continuity and semantic retrieval timing

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

Status:
- mostly implemented on this branch

What landed:
- structural boost derivation in `trail_save`
- structural node reactivation in `trail_resume`
- explicit `reactivated_node_ids` and `resume_hints`
- compact preview limits for resumed output

Residual problem:
- continuity is now structurally stronger, but the ranking quality of the resumed next step still needs more real-session validation

Acceptance criteria:
- resumed investigations keep reducing repeat searches in longer, less synthetic runs
- next-focus and next-tool suggestions stay useful beyond the starter continuity corpus

### 10. Add a continuity-oriented resume mode

Status:
- partially implemented on this branch

What landed:
- `trail_resume` now surfaces `next_focus_node_id`
- `trail_resume` now surfaces `next_open_question`
- `trail_resume` now surfaces `next_suggested_tool`
- temporal follow-ups can now route toward `timeline`

Residual problem:
- the current resume mode is actionable, but still needs ranking and latency tuning in larger real investigations

Acceptance criteria:
- a resumed investigation can answer the next open question with fewer retrieval steps
- the guidance remains compact without losing the right next seam

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

### 12. Improve `apply_batch` progress UX during long writes

Problem:
- `apply_batch` can feel opaque during larger writes because the user sees one big batch land without intermediate state
- even when the backend work is correct, the waiting experience feels confusing and untrustworthy

Likely files:
- `m1nd-mcp/src/surgical_handlers.rs`
- `m1nd-mcp/src/protocol/surgical.rs`
- any client/UI surfaces that render batch execution state

Expected impact:
- better user trust during long-running batch writes
- fewer “is it stuck?” moments
- clearer mental model of what the system is doing before verification completes

Acceptance criteria:
- long `apply_batch` executions can surface stepwise progress such as prepare, write, re-ingest, verify
- the user can see which file or phase is currently being processed
- partial progress updates do not require waiting for the full batch response to finish
- the final response still remains compact and authoritative after completion

## Suggested Execution Order

1. `timeline` fidelity + canonical identity normalization
2. `VerificationImpact` parity + fallback explanation preservation
3. `validate_plan` noise reduction
4. `surgical_context_v2` proof-focused mode
5. `seek` ranking improvements
6. `trail_resume` ranking and continuity follow-up improvements
7. `apply_batch` progress and user-facing execution feedback

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
