# m1nd Routing Playbooks

This file is the operational map: which `m1nd` tools to choose for each kind of job, and in what order.

## Decision Rules

- Start with `north(task)` — the in-session front door — unless the task is
  obviously outside m1nd's lane. It composes trust + orient + boot_memory +
  focus in one round-trip; `needs_ingest` -> `ingest` -> `north` again.
  `trust_selftest`/`session_handshake`/`recovery_playbook` are the
  degraded/recovery route, not the default first move.
- Obey verdicts: retrieval/prediction return `act`/`reverify`/`abstain`
  (`abstain` = stop, not weak-yes; gate caps at `reverify` until
  `calibrate_predict` runs once); `why` carries `closure` (`blocked` = unresolved
  edge); `seek` carries `trust_envelope` + sufficiency; `insufficient_evidence` =
  NO evidence, not medium risk.
- Use `search` when the user already knows the string or regex.
- Use `glob` when the user knows the path shape but not the contents.
- Use `view` when the file is already known and bounded reading is enough.
- Use `seek` when the user knows what the code does but not where it lives.
- Use `activate` when the user needs a subsystem or neighborhood map.
- Use `perspective_*` only when navigation state is worth the overhead.
- Use `impact`, `validate_plan`, and the surgical tools before risky edits.
- Use `trace` when you already have failure text.
- Use `document_*` when docs, wiki pages, PDFs, or specs must stay grounded in code.
- Use daemon, alerts, trails, and locks when the session is long-lived or multi-agent.

## Workflow 1: First Contact With An Unfamiliar Repo

1. `north(task)` — one round-trip trust + context + prior memory + sufficiency +
   `next_move`
2. `ingest` then `north` again if step 1 returned `needs_ingest`
3. `audit` for a wider high-level sweep when `north`'s anchors are not enough
4. `search`, `seek`, or `activate`
5. `batch_view` or `view`
6. `coverage_session` if the investigation is getting wide

`north` is the fastest orientation and carries the anchors most first questions
need. Reach for `audit` for a wider sweep, `activate` when the next question is
subsystem-shaped, and `seek` when it is intent-shaped.

## Workflow 2: Find The Code That Does X

1. `seek`
2. `view` or `batch_view`
3. `activate` if the best result needs neighborhood expansion
4. `learn` after the result was actually useful

Use this instead of `activate` when you know the job and want a specific implementation surface, not a cluster.

## Workflow 3: Explore A Subsystem Or Topic

1. `warmup` if the session is focused on one area for a while
2. `activate`
3. `why` for specific dependencies
4. `resonate` when you need coherent clusters or refactor boundaries
5. `learn` after useful results

Use `include_structural_holes` in `activate` or call `missing` separately if the question smells like absence rather than presence.

## Workflow 4: Triage A Bug From Error Output

1. `trace`
2. `impact` on the strongest suspect if the fix may be risky
3. `why` for a specific suspect-to-effect chain
4. `validate_plan`
5. `surgical_context_v2`
6. local edit path or `edit_preview` / `edit_commit`
7. `predict`

Use `trace` instead of manually chasing top stack frames when the runtime output is already available.

## Workflow 5: Plan A Risky Change

1. `seek` or `activate` to find the entry surface
2. `impact`
3. `validate_plan`
4. `surgical_context_v2`
5. optional `heuristics_surface` for why a file ranks as risky
6. optional `edit_preview` for two-phase commit
7. optional `apply_batch` when an external m1nd write path is actually desired
8. `predict`

Interpretation:

- `impact` tells you what the root file touches
- `validate_plan` tells you what your proposed change list is missing
- `surgical_context_v2` gives connected source context in one shot
- `predict` is the post-edit follow-through check

## Workflow 6: Review Whether A Plan Or PR Is Incomplete

1. `validate_plan`
2. `impact` for any central modified file
3. `predict` for changed nodes that still look under-covered
4. `scan` or `scan_all` if structural quality or safety patterns matter
5. `heuristics_surface` when you need a reasoned explanation of why a file is risky

This is the review-time route when the important question is not "what changed?" but "what is still missing?"

## Workflow 7: Implement From A Spec, Wiki, Or PDF

1. choose the doc lane:
   - authored semantic markdown -> `ingest` with `adapter: "light"`
   - ordinary docs -> `ingest` with `adapter: "universal"` or `adapter: "auto"`
2. for `universal`, use `document_provider_health` if richer extraction may matter
3. for `universal`, use `document_resolve`
4. for `universal`, use `document_bindings`
5. for `universal`, use `document_drift`
6. for both lanes, use `search`, `seek`, or `activate` on the merged graph
7. `impact` or `validate_plan`
8. `surgical_context_v2`

Use this when a document must be connected to code, not merely quoted or summarized. Use `light` when the spec itself is graph-native. Use `universal` when the source must first be canonicalized.

## Workflow 8: Multi-Repo Questions

1. `federate` when the repo list is already known
2. `federate_auto` when only import/path/contract evidence suggests sibling repos
3. `activate`, `seek`, `impact`, or `trace` on the federated graph

Use federation before pretending the current repo is the whole truth.

## Workflow 9: Multi-Agent Exploration

1. Give each agent a stable `agent_id`
2. Use `perspective_start` per agent when route-state navigation is useful
3. Use `perspective_branch` for parallel exploration
4. Use `perspective_compare` to diff findings
5. Use `trail_save` and `trail_merge` for handoff and convergence
6. Use `lock_create`, `lock_watch`, `lock_diff`, `lock_rebase`, `lock_release` when concurrent edits or baselines matter

Rules:

- graph learning is shared
- perspectives are isolated per agent
- trails are shareable
- locks are for coordination, not for solo reading

## Workflow 10: Long-Lived Monitoring

1. `daemon_start`
2. `daemon_status`
3. `daemon_tick` when an explicit reconciliation pass is needed
4. `alerts_list`
5. `alerts_ack`
6. `persist`

This is the right lane when the task is ongoing drift detection, not one-shot analysis.

## Workflow 11: Session Resume Or Handoff

1. `trail_list`
2. `trail_resume`
3. `drift` if the graph may have changed
4. `warmup` if you only need gentle re-priming
5. `boot_memory` for tiny durable facts that should stay hot

Use `trail_resume` for real investigation continuity. Use `boot_memory` only for small canonical facts, not full investigative state.

## Workflow 12: Deep Structural Or Risk Analysis

Choose among these when basic search/impact is not enough:

- `hypothesize`: test a structural claim
- `counterfactual`: simulate node or module removal
- `differential`: compare two graph snapshots
- `diverge`: compare against a baseline and surface drift
- `fingerprint` or `twins`: find structural equivalence
- `resonate`: find reinforcing clusters
- `layers` and `layer_inspect`: inspect architectural layering and violations
- `ghost_edges`: use temporal co-change edges
- `runtime_overlay`: bring in runtime heat and errors
- `taint_trace`: follow taint propagation
- `flow_simulate`: reason about concurrency turbulence
- `epidemic`, `trust`, `tremor`, `antibody_*`: risk and defect-propagation surfaces

## What Not To Force Through m1nd

- Exact text already known: use `search` or `rg`
- One known file with a simple question: use `view` or direct read
- Compiler or test failure truth: use the compiler or test runner
- Runtime behavior truth: use logs, traces, debugger, or profiler
- Final local file mutation inside Codex: usually use `apply_patch`
