# m1nd Full-Spec Agent Operating Layer

This is the full-spec operating layer for agents. Use it when the work is broad,
risky, multi-repo, proof-sensitive, long-running, document-bound, or when the
user explicitly says to use the full m1nd/L1GHT system.

The compact packs teach the first move. This file teaches the whole game.

## Prime Contract

m1nd is not "semantic search with extra tools." m1nd is an agent operating
layer over a codebase graph, document graph, runtime sidecars, recovery state,
agent memory, and proof envelopes.

The agent should never ask "should I use m1nd or files?" as a binary. The
correct model is:

1. Use m1nd to establish trust, scope, structure, routes, impact, memory, and
   recovery state.
2. Use direct files, tests, compiler/runtime output, logs, and focused probes
   for final execution truth.
3. Feed confirmed or rejected graph results back with `learn` when the result
   materially shaped the investigation.

Non-claims:

- m1nd does not replace compiler, tests, debugger, local file truth, or human
  judgment.
- m1nd does not prove a host has rebound just because a binary was updated.
- m1nd does not repair graph contents unless an ingest/update command actually
  ran.
- Empty retrieval is not absence proof until workspace, graph, runtime, and
  recovery envelopes agree.

## Source Of Truth Hierarchy

Use this hierarchy when exact tool shape matters:

1. `tools/list`: live MCP names and JSON schemas for the active runtime.
2. `help`: operational guidance for one tool, one stage, or one error.
3. `trust_selftest` / `session_handshake`: live binding, graph, host, and
   workspace truth.
4. `skills/m1nd-operator/references/tool-families.md`: compact family map.
5. This full-spec layer: cross-tool combinations and decision recipes.
6. Direct source, tests, compiler/runtime output: final behavioral truth.

If this file and the live runtime disagree, trust the live runtime for callable
schemas and this file for operating strategy.

## Start Every Serious Session

Use one stable `agent_id` for one investigation.

1. `north(task)` FIRST, before reading or editing anything — the in-session
   front door. One round-trip returns binding trust (`trust_mode`; repair
   travels with it when degraded), task context (focus + PageRank anchors),
   prior cross-session memory (each claim with real age + author), a sufficiency
   signal, one `next_move`, and `honest_gaps`. It composes `trust_selftest` +
   `orient` + `boot_memory` + `focus`. Heed `reception`:
   `reception.match == "caller_root_mismatch"` means the bound graph does NOT
   cover your current repo — do not trust retrieval for it; read
   `reception.options[]`. ONE call sets you up: `ingest` with
   `project_root=<your repo root>` creates a per-project brain inside the
   served owner, ingests your repo, binds your session, and returns its north
   packet — thereafter every call from your root routes to YOUR brain
   automatically. Absent/null = your root matches the brain serving you.
2. If `north` returns `needs_ingest` (empty/unbound graph), `ingest` the
   intended repo, then `north` again. `needs_ingest` is a REAL answer.
3. Act on the verdicts (see "Verdict Discipline" below); then use retrieval or
   risk tools as evidence.

Degraded/recovery front door only: `trust_selftest` / `session_handshake` (with
`scope`) / `recovery_playbook` when `north` degrades, trust is not full, or
retrieval returns `blocked`/empty unexpectedly — compare binding fingerprints
and follow the playbook before trusting retrieval.

For local helper use:

```bash
m1nd agent orient \
  --repo /path/to/repo \
  --query "focused subsystem or bug surface" \
  --mode short \
  --json
```

Use `--repo` whenever the process is launched from a director repo but the
inspected repo is somewhere else. The older `probe_m1nd.py` helper remains
available for custom multi-tool sequences.

## Verdict Discipline

Retrieval and prediction return a calibrated verdict; obey it, do not override.

- **`act` / `reverify` / `abstain`** on retrieval and prediction. `abstain` =
  uncalibrated OR insufficient evidence: a STOP, not a weak yes. The prediction
  gate is armed by `calibrate_predict` and the seek trust envelope by
  `calibrate_envelope` (from the ledger's learn outcomes); until each is armed its
  verdict caps at `reverify`, never `act`.
- **`why` carries a `closure` verdict** — `blocked` means the path rests on an
  unresolved (guessed/dropped) edge; verify that edge before relying on the path.
- **`seek` carries a `trust_envelope` + a sufficiency stop-signal** —
  `sufficient` = stop gathering; `gathering`/`saturated` = widen or refine.
- **`trust_band: insufficient_evidence` = NO evidence, not medium risk** — the
  honest cold-start answer, distinct from low/medium/high risk.

## Tool Families At A Glance

Foundation:

- `ingest`, `health`, `trust_selftest`, `session_handshake`,
  `recovery_playbook`, `doctor`, `help`, `report`, `savings`

Search and orientation:

- `search`, `glob`, `view`, `batch_view`, `audit`, `coverage_session`,
  `cross_verify`, `panoramic`, `metrics`, `diagram`

Intent and structure:

- `seek`, `activate`, `warmup`, `resonate`, `why`, `missing`, `fingerprint`,
  `scan`, `scan_all`, `timeline`, `trace`

Change and proof:

- `impact`, `predict`, `validate_plan`, `heuristics_surface`, `hypothesize`,
  `counterfactual`, `differential`, `diverge`

Memory and learning:

- `learn`, `drift`, `trail_save`, `trail_resume`, `trail_list`,
  `trail_merge`, `persist`, `boot_memory`

Mission control:

- `mission_start`, `mission_event`, `mission_next`, `mission_verify`,
  `mission_handoff`, `mission_close`

Stateful navigation:

- `perspective_start`, `perspective_routes`, `perspective_inspect`,
  `perspective_peek`, `perspective_follow`, `perspective_suggest`,
  `perspective_affinity`, `perspective_branch`, `perspective_back`,
  `perspective_compare`, `perspective_list`, `perspective_close`

Docs and L1GHT:

- `ingest` with `adapter="light"`, `ingest` with `adapter="universal"` or
  `adapter="auto"`, `document_resolve`, `document_provider_health`,
  `document_bindings`, `document_drift`, `auto_ingest_start`,
  `auto_ingest_status`, `auto_ingest_tick`, `auto_ingest_stop`

Surgical/write lane:

- `surgical_context`, `surgical_context_v2`, `edit_preview`, `edit_commit`,
  `apply`, `apply_batch`

Multi-repo:

- `federate`, `federate_auto`, `external_references`

Coordination and monitoring:

- `lock_create`, `lock_watch`, `lock_diff`, `lock_rebase`, `lock_release`,
  `daemon_start`, `daemon_status`, `daemon_tick`, `daemon_stop`,
  `alerts_list`, `alerts_ack`

Deep risk and architecture:

- `antibody_scan`, `antibody_list`, `antibody_create`, `flow_simulate`,
  `epidemic`, `tremor`, `trust`, `layers`, `layer_inspect`, `ghost_edges`,
  `taint_trace`, `twins`, `refactor_plan`, `runtime_overlay`, `type_trace`

## Decision Router

Tiny localized audit:

- `north(task)` (front door); `needs_ingest` -> `ingest` -> `north`. Drop to
  `trust_selftest`/scoped `session_handshake` only if trust looks off
- one bounded recovery/ingest pass if needed
- one or two cheap orientation calls when `north`'s anchors are not enough:
  `search`, `seek`, or `activate` (or `audit` for a wider sweep)
- then stop graph exploration (obey the verdicts) and prove with direct files,
  git diff, tests, compiler/runtime output, and focused probes
- record `short_audit_orientation` or `recovery_overhead`

Broad review, bug hunt, refactor, or proof-sensitive investigation:

- start with `mission_start` so repo, task, mode, budget, risk, and non-claims
  are explicit
- record meaningful actions with `mission_event` when available, especially
  direct reads, tests, runtime probes, coverage sweeps, and dissent
- use `mission_next` after meaningful events so the runtime can stop loops and
  issue `do_not` guardrails
- when `mission_next` switches to direct proof, stop graph calls unless a
  dissent event justifies the deviation
- call `mission_verify` before finalizing any material claim
- call `mission_handoff` before pausing, delegating, or switching agents
- close with `mission_close` so the result carries verified claims, rejected
  claims, gaps, event digest, and non-claims

Evidence rule: claims must reference their direct proof explicitly. A prior
`mission_event` only proves a claim when the claim cites that `event_id`, for
example `event:evt_1`, or gives a direct `file_read`, `test_run`, compiler, or
runtime probe ref.

Exact text:

- `search`
- then `view` or `batch_view`
- then `learn` if the path was useful

Path or file shape:

- `glob`
- then `view`
- then `cross_verify` if graph/disk truth matters

Unknown repo:

- `north(task)` -> `ingest` (only on `needs_ingest`) -> `north` -> `audit` ->
  `panoramic`
- add `layers` for architecture
- add `diagram` when the user needs a map

Known purpose, unknown file:

- `seek`
- if broad, `activate`
- if result is blocked/empty, read the runtime envelope and run
  `recovery_playbook` before falling back

Connected subsystem:

- `warmup` -> `activate` -> `why` / `missing`
- then `batch_view`

Bug symptom or stacktrace:

- `trace`
- then `search` exact error terms
- then `impact` on the suspect file
- prove with focused runtime/test reproduction

Risky edit:

- `search` or `activate` to locate
- `impact`
- `predict`
- `validate_plan`
- `surgical_context_v2`
- direct edit/test proof
- `learn` on useful or wrong graph routes

Code review:

- `audit` if unfamiliar
- `impact` on changed surfaces
- `validate_plan` for missing tests/docs/contracts
- `heuristics_surface` for suspicious hotspots
- direct diff review and tests

Docs/spec drift:

- `ingest` docs with `adapter="light"` for L1GHT or `adapter="universal"` for
  ordinary docs
- `document_bindings`
- `document_drift`
- `search` / `view` for direct code proof

Architecture map:

- `audit`
- `layers`
- `layer_inspect`
- `diagram`
- `panoramic`
- `metrics`

Security or data-flow suspicion:

- `search` exact sinks/sources
- `taint_trace`
- `trust`
- `ghost_edges`
- `impact`
- direct security/test proof

Concurrency or propagation suspicion:

- `flow_simulate`
- `epidemic`
- `tremor`
- `ghost_edges`
- then direct tests/logs

Long investigation:

- `perspective_start`
- `perspective_routes`
- `perspective_follow` / `perspective_peek`
- `trail_save`
- `trail_resume` later

Multi-agent work:

- stable per-agent `agent_id`
- `lock_create` for shared risky region
- `lock_watch` while others work
- `lock_diff` before integration
- `trail_merge` for handoff synthesis

Multi-repo work:

- `session_handshake(scope=target_repo)` first
- if the active workspace is wrong, do not call it stale graph
- `federate_auto` for likely siblings
- `federate` for known repo set
- `external_references` for outside-root evidence

Runtime/host confusion:

- `trust_selftest`
- `session_handshake`
- `recovery_playbook`
- `doctor`
- external CLI: `m1nd doctor`, `m1nd update status`, `m1nd hosts status`,
  `m1nd hosts plan`
- rebind host before claiming refreshed tools

## High-Value Combination Recipes

### First Contact Audit

Use when entering a repo cold.

```text
north(task)                       # front door; needs_ingest -> ingest -> north
ingest(path)                      # only if north returned needs_ingest
audit(path/profile=auto)          # wider sweep beyond north's anchors
panoramic()
layers()
diagram(format=mermaid)
coverage_session()
```

Deliverable: map of major surfaces, likely hotspots, proof gaps, and next
investigation routes.

### Feature Localization

Use when the user asks "where is X implemented?"

```text
search(exact nouns)
seek(purpose)
activate(topic)
why(source,target) when relationships matter
batch_view(top files)
learn(correct|partial|wrong)
```

Deliverable: exact files/functions, route rationale, and confidence boundary.

### Bug Hunt

Use when searching for defects without editing.

```text
north(task)                       # front door; needs_ingest -> ingest -> north
audit()
panoramic()
search(edge-case terms)
trace(error text if present)
heuristics_surface(hot files)
impact(suspect)
direct probes/tests
```

Deliverable: source-backed findings with reproduction/test ideas. Do not
over-claim extra findings without judge/proof.

### Risky Patch Prep

Use before touching shared behavior.

```text
seek/activate(target behavior)
impact(target node)
predict(target node)
validate_plan(actions)
surgical_context_v2(target file)
edit locally
tests/compiler/runtime proof
learn(feedback)
```

Deliverable: narrow patch, touched files, tests run, missing risks.

### Documentation Truth Loop

Use when docs must be living code truth.

```text
ingest(adapter="light"|"universal", path=doc_or_docs_root)
document_resolve()
document_bindings()
document_drift()
search/view code bindings
update docs or code
cross_verify()
```

Deliverable: claim-to-code binding, drift report, and corrected truth.

### L1GHT Authoring Loop

Use when creating docs that should think like code.

```text
write L1GHT with claims/entities/bindings
ingest(adapter="light")
document_bindings()
document_drift()
search/activate around bound entities
cross_verify()
```

Deliverable: graph-native doc whose claims can be navigated and checked.

### Multi-Agent Integration

Use when several agents work on one repo.

```text
lock_create(scope/root_nodes)
assign stable agent_id per lane
perspective_start for each lane if exploratory
lock_watch()
lock_diff() before integration
trail_save per lane
trail_merge()
validate_plan(final integration)
```

Deliverable: non-overlapping work, conflict surface, integration risk.

### Cross-Repo System Map

Use when two or more repos define one product/system.

```text
session_handshake(scope=repo_a)
ingest(repo_a)
federate_auto()
federate([repo_a, repo_b, ...]) when known
external_references()
activate(system topic)
impact(boundary node)
diagram()
```

Deliverable: cross-boundary map with explicit non-claims about missing repos or
un-ingested systems.

### Recovery From Bad m1nd State

Use when agents report blocked, empty, stale, wrong workspace, or transport
failure.

```text
Transport closed -> restart/rebind host first
trust_selftest(scope)
session_handshake(scope)
recovery_playbook(recovery.arguments)
doctor(observed_tool, observed_tool_count, missing_tools)
m1nd hosts status/plan outside MCP if host binding may be stale
ingest only after binding/scope is clear
```

Deliverable: classify the failure: dead transport, wrong workspace, empty
graph, stale binary, degraded tool surface, or real retrieval miss.

## L1GHT Operating Rules

Use L1GHT when a document needs graph-native truth:

- specs
- contracts
- architectural decisions
- proof notes
- docs that should bind to implementation
- long-lived agent handoff knowledge

Use `adapter="light"` for authored L1GHT. Use `adapter="universal"` or
`adapter="auto"` for PDFs, DOCX, HTML, Markdown, wiki pages, and ordinary docs.

After ingest, do not stop at "doc was read." Run:

- `document_resolve` to find canonical artifacts
- `document_bindings` to map claims/entities to code
- `document_drift` to expose stale or missing bindings

## Evidence Discipline

Every serious m1nd-assisted answer should preserve:

- tool calls used
- graph/trust state when relevant
- files inspected
- commands/tests run
- recovery path if any
- fallback reason if m1nd was insufficient
- non-claims

For benchmark lanes, write events as you go. A final answer without event
evidence is weaker than a messy but timestamped investigation.

## Post-Capture — Leave The Graph Warmer

Before ending, close the loop:

- `memorize` every durable finding (a decision, a verified fact, why code is the
  way it is, an open design point) with `confidence` and repo-relative
  `evidence` paths, so it lives in the same activation space as code and
  self-flags stale via `cross_verify(check:["evidence_freshness"])`. Closing a
  mission? `mission_close(write_light_memory:true)` persists verified claims in
  one step.
- Then ONE field-telemetry signal, keep working (report, never detour): when a
  retrieval was right/wrong, `learn(correct|wrong|partial)`; when m1nd ITSELF
  misbehaves (bug, friction, or honesty miss — claimed fresh/closed/act and was
  wrong), append ONE JSON line to `~/.m1nd/field-reports.jsonl`
  (`{ts,agent,repo,tool,class:"bug|honesty|friction|win",what,expected,snippet}`)
  — LOCAL-ONLY, never phones home. NEVER fix m1nd mid-mission; work around, note
  the workaround, report.

## Anti-Patterns

Do not:

- treat blocked or empty retrieval as proof of absence
- run broad direct file reads before trust/scope is established
- call a wrong-workspace binding "stale graph" without context guard evidence
- assume updated binaries refreshed an already-open MCP host
- use `activate` for exact strings that belong to `search`
- use `seek` for path patterns that belong to `glob`
- open perspective state for a tiny one-shot lookup
- use m1nd write tools when the host's native patch tool is safer and expected
- claim L1GHT drift is solved before `document_drift`
- skip tests because graph impact looked small

## Full-Spec Benchmark Mode

`m1nd-full-spec` means the agent receives this operating layer and should route
the task through the best tool combination, not only the compact trained loop.

It should be compared separately from:

- `m1nd-basic`: graph available, no doctrine
- `m1nd-mission-control`: trained loop expressed as mission state, next-move
  guardrails, claim verification, and proof packets
- `m1nd-short-audit`: bounded orientation plus direct proof for tiny/localized tasks
- `m1nd-trained`: compact doctrine
- `m1nd-temponizer-compact`: compact doctrine plus temporal recalibration
- `direct`: no m1nd

Expected hypothesis:

- full spec may improve broad/systemic tasks, architecture audits, docs drift,
  multi-repo work, and hard recovery
- full spec may slow narrow bug-recall tasks if the agent treats the manual as
  paperwork instead of a router

The agent should read it as a route table, not a checklist.
