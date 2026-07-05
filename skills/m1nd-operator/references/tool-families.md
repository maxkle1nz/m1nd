# m1nd Tool Families

This is the compact capability inventory for the current `m1nd` shape. Use the live runtime, not this file, when exact counts or schemas matter.

## Foundation And Search

- `ingest`: load or refresh the graph from code, JSON, memory, or universal docs.
- `health`: confirm server and graph state.
- `search`: exact text, regex, or graph-aware semantic search with line context.
- `glob`: path-pattern search over indexed files.
- `view`: bounded line-numbered file read.
- `batch_view`: stable multi-file read surface.
- `help`: self-documenting tool reference and recovery guidance.
- `audit`: one-call orientation for unfamiliar repos.
- `coverage_session`: what this agent has already visited.
- `cross_verify`: graph-vs-disk verification.
- `external_references`: explicit paths outside ingest roots.
- `report`: session summary.
- `metrics`: structural metrics per file/function/module.
- `type_trace`: follow a type across the graph.
- `diagram`: generate Mermaid or DOT graph slices.
- `panoramic`: ranked panorama of file-level risk across the repo.

## Retrieval By Intent, Topic, Or Pattern

- `seek`: find code by intent or purpose.
- `activate`: find a neighborhood around a topic via spreading activation.
- `warmup`: prime the graph for a task.
- `resonate`: find mutually reinforcing clusters.
- `why`: shortest path and dependency explanation between two nodes.
- `missing`: surface structural holes or absent abstractions.
- `scan`: run one structural pattern family.
- `scan_all`: run all structural patterns in one call.
- `timeline`: temporal history, churn, velocity, stability, and co-change shape.

> `seek` semantic recall: the server ships with the `embed` feature ON by
> default, so `seek` matches by MEANING — embeddings over each symbol's
> doc-comment + signature + body — surfacing a node whose intent matches even
> with ZERO shared tokens (e.g. a query "secure channel" reaching `fn qz9wb`
> documented as "TLS encrypted tunnel"). The model (~29 MB) is fetched on first
> use and cached; if it can't load, `seek` falls back to trigram matching. The
> response's `embeddings_used` tells you whether semantic recall actually fired
> (`false` = trigram/keyword fallback — still useful, just lexical). These are
> fast STATIC embeddings (model2vec) — a real upgrade over trigrams but NOT
> transformer-grade. When `seek` returns no results, `filtering_reason` states
> why (empty graph / scope-or-type filtered everything / nothing cleared the
> relevance threshold) — adjust the query or scope instead of re-running blind.

## Change-Risk And Plan Validation

- `impact`: blast radius from a node.
- `predict`: likely co-change partners after an edit.
- `validate_plan`: pre-flight risk, gaps, missing tests, and suggested additions.
- `heuristics_surface`: explain why a file or node ranked risky or important.
- `hypothesize`: test a structural claim.
- `counterfactual`: simulate node or module removal.
- `differential`: compare two graph snapshots.
- `diverge`: detect structural drift vs a baseline (ISO date / git ref / last_session).

Note on `predict`: co-change predictions need the git co-change matrix — run `ghost_edges` once before `predict`, otherwise it returns only structural fallback (the response carries a `note` saying so). `trust` and `tremor` are honest-empty until populated: `trust` from `learn`/`cross_verify` defect feedback (NOT churn), `tremor` from `ghost_edges`/daemon velocity observations.

## Language Coverage (what graph reasoning can trust)

`impact`/`why`/`predict`/`trace`/`taint_trace` depend on the extractor emitting real `calls` edges and resolved cross-file `imports`. Both are verified for: **Rust, Python, JS/TS, Go, Java, C, C++, Kotlin, PHP, Scala** (full parity). **Ruby**: cross-file `require_relative` resolves; calls pending. **C#, Swift**: calls only (C# namespaces don't map 1:1 to files). Anything else falls back to the generic extractor (`contains` only) — on those, prefer `search`/`grep` over `impact`/`why`. Unresolvable imports (external gems/packages/stdlib/system headers) are left unresolved, never guessed.

## Memory, Learning, And Continuity

- `learn`: reinforce or weaken paths based on feedback.
- `drift`: see what changed since the last session or baseline.
- `trail_save`: persist an investigation.
- `trail_resume`: restore an investigation and re-inject boosts.
- `trail_list`: browse saved investigations.
- `trail_merge`: merge investigations across agents or sessions.
- `persist`: force graph-sidecar persistence.
- `boot_memory`: store tiny canonical hot-state values.

Rules:

- `learn` is how the graph adapts.
- `trail_*` is for full investigative continuity.
- `boot_memory` is for tiny durable facts, not full reasoning trails.

## Agent Memory (Durable Cross-Session Knowledge)

- `memorize`: author durable agent memory as a graph-native L1GHT `.light.md`. Accepts structured claims with `confidence`, `ambiguity`, and `evidence` (repo-relative code paths). Writes to `<runtime_root>/agent-memory/`, ingests immediately (adapter: light, mode: merge by default), and anchors each evidence path to the real code node via a `grounded_in` edge. Ingest target code BEFORE calling `memorize` so evidence resolves. A `memorize` is ALWAYS project-private (born in the routed brain, stamped `Origin-Brain`).
- `promote {brain, claim, reason}` (MEDULLA M6): the AUDITED crossing — copy a VERIFIED project-private claim UP into the shared `medulla` (the doctrine tier every session's default beat reads). Stamps the full chain (`Origin-Brain`, `Origin-Claim`, `Promoted-By`, `Promotion-Reason`); the project original stays in place stamped `Promoted-To` (elevate, never move). Gates inside the verb: only `State: verified` (or `Source-Agent: human:maintainer`) may promote (C8.3); a secret/conflict-marker is refused at the hygiene floor; evidence is origin-qualified (`<origin_root>#<path>`) so freshness delegates back to the home brain, else the claim is marked `evidence_unverifiable` (C8.2 — never reads fresher than it can prove); a weaker re-promotion bounces `WouldDowngrade`. It is an ORCHESTRATOR act (makers propose, orchestrator executes); any id may call it but `Promoted-By` audits every promotion. Demote via `learn wrong` on the MEDULLA copy or a `moved_to:` superseding medulla `memorize` — never touches the witness. Served at the routed HTTP door.
- `delegate {agent_id, task, scope?{paths,seeds}, budget?}` (ORGANISM R6 + R7/M7): the READ-ONLY spawn packet — composes the RETRIEVAL half of a subagent's spec in one call (`m1nd-delegation-packet-v0`): the mother's `binding` (the NAMED brain the child must land on), the selected memory slice as LABELED cargo (M7) — each `context.memory` row carries `tier` (`project` | `medulla`) + `origin_brain` beside age + author, so the child inherits exactly what you chose AND can tell doctrine from project fact (auditable). The slice is the DEFAULT beat (your project brain's task-relevant claims + the medulla doctrine the domain touches, folded by the routing seam), never `all-brains`, never another project's private claim (MED-INV-1). Plus ranked anchors, a delegate-time staleness header, known static dependents, a proof-command heuristic, and an explicit "what m1nd could NOT determine → your duties" section closing with the report protocol. Renders `prompt_markdown` you APPEND to your brief (memory lines read `- [tier] claim — origin · author, age`; appendix: your text wins on what-to-do, the file on what-is, the packet outranks assumption only — NOT a fence). The packet's `mission.binding.workspace_root` IS the datum reception verifies (`M1nd-Caller-Root` ↔ `covers_root`), so the child VERIFIES it landed (silent on match) rather than choosing — the child law (§C5.3). Abstains honestly with evidence + `next_move`: `needs_ingest` (empty graph), `unscopable` (task activated no coherent subgraph — uncalibrated gate), `seeds_unresolvable` (every seed failed). No predict/trust/tremor/xray enrichment yet — each omission is a `non_claims` line. Writes one dumb registry record (the debrief join key). Read-only — stays OFF `READ_ONLY_DENIED_TOOLS`.
- `debrief {agent_id, delegation_id, outcome, subagent_id?, diff|touched_paths?, findings?, evidence?}` (ORGANISM R6): the graded return — the ONLY mutation in the delegation layer, through `memorize`/`learn` ONLY. Loads the record (unknown id → hard error), classifies each touched path (`in_scope | expected_change | dependent_contact | unpredicted`) with a worst-of verdict that always carries fence existence (`"stayed — no ratified boundaries existed"`), memorizes the subagent's `findings[]` under the SUBAGENT's id and any map-miss lessons under YOURS (clean runs memorize nothing — no filler), teaches asymmetrically (`unpredicted → learn(partial)`, contacted dependent → `learn(correct)`, untouched dependents never punished), and appends one `outcomes.jsonl` row stamped `outcome_unverified` unless you attach `evidence {cmd, exit_status}`. Conformance grades PATHS, never code quality — it NEVER says merge-safe. Two gradings kept apart: conformance grades the subagent-vs-map; the map-grade grades m1nd-vs-reality. Every debrief visibly deposits memory the next `delegate` will surface — skipping it wastes knowledge. On `READ_ONLY_DENIED_TOOLS` (it mutates). The subagent itself holds NO m1nd tool — it participates via the `[m1nd dlg_…]` report line (DEVIATIONS + FINDINGS) you feed back.
- `cross_verify(check: ["evidence_freshness"])`: re-hashes each `grounded_in` code target vs the hash recorded at ingest; returns `stale_evidence[]` + `stale_evidence_count` naming which memorized claims cite changed code. (`check` is an array; other values: `existence`, `loc`, `hash`.)
- `mission_close(write_light_memory: true)`: one-step path — closes the mission AND persists its verified claims as L1GHT memory; `light_memory` in the response gives the file path.
- Boot auto-load: on session start, m1nd auto-ingests all `<runtime_root>/agent-memory/*.light.md` (gated by `M1ND_AUTO_LOAD_AGENT_MEMORY`, default ON). Past findings are available without explicit re-ingest. Reported in `session_handshake.agent_memory`.

Rules:

- `memorize` is for durable, cross-session findings — decisions, verified facts, design rationale.
- `trail_*` is for full investigative continuity within and across sessions.
- `boot_memory` is for tiny hot-state values, not structured reasoning.
- Use `ingest mode: merge` (not `replace`) when re-ingesting code to preserve agent memory nodes and `grounded_in` edges.

## Mission Control And Proof Packets

- `mission_start`: create a repo-scoped mission with route, budget envelope,
  starter moves, and non-claims.
- `mission_event`: record one observed action with event id, evidence class,
  budget update, and event digest.
- `mission_next`: append the last event and get exactly one next move plus
  `do_not` guardrails.
- `mission_verify`: reject claims that only have graph/inferred evidence and
  require direct source, test, compiler/runtime, or probe evidence.
- `mission_handoff`: serialize resumable context, verified claims, open
  hypotheses, dead paths, graph anchors, and next move.
- `mission_close`: emit a proof packet with verified claims, rejected claims,
  tools observed, event digest, gaps, budget consumption, and non-claims.

Use this family when the problem is not "which file?" but "how should the
agent stay on mission and know when to stop using the graph?" It does not
repair host bindings, refresh cached MCP tool lists, or prove graph contents.

## Stateful Navigation

- `perspective_start`: create a navigable route surface from a query.
- `perspective_routes`: paginate route options.
- `perspective_inspect`: expand a route with score breakdown and provenance.
- `perspective_peek`: preview code or doc content at a route target.
- `perspective_follow`: move focus to the route target.
- `perspective_suggest`: ask the graph which route to follow next.
- `perspective_affinity`: inspect probable affinities for the current route target.
- `perspective_branch`: fork a perspective for parallel exploration.
- `perspective_back`: go back one navigation step.
- `perspective_compare`: diff two perspectives.
- `perspective_list`: list active perspectives for the agent.
- `perspective_close`: release perspective state.

Use this family only when navigation state is worth maintaining.

## Docs, Wiki, PDF, And Universal Document Runtime

- `ingest` with `adapter: "light"`: graph-native semantic markdown via the `L1GHT` protocol.
- `document_resolve`: resolve canonical local artifacts for an ingested document.
- `document_provider_health`: inspect optional provider availability and install hints.
- `document_bindings`: map document claims or sections to code.
- `document_drift`: detect stale, missing, or ambiguous document/code links.
- `auto_ingest_start`: start local-first document watchers.
- `auto_ingest_status`: inspect watcher state and provider/runtime counters.
- `auto_ingest_tick`: force one deterministic document reconciliation pass.
- `auto_ingest_stop`: stop watchers and persist manifest state.

Use this family when docs must be graph-grounded, not merely read. Distinguish the lanes:

- `light` for authored graph-native semantic markdown
- `universal` for ordinary docs that need canonicalization and binding/drift surfaces

## Surgical And Write Surfaces

- `surgical_context`: single-file edit context with callers, callees, and neighbors.
- `surgical_context_v2`: multi-file connected edit context, including source excerpts of related files.
- `apply`: one-file write through m1nd.
- `apply_batch`: atomic multi-file write through m1nd.
- `edit_preview`: two-phase preview without touching disk.
- `edit_commit`: freshness-checked commit for a preview.

In Codex, prefer this family for analysis and change-prep. Use local `apply_patch` for actual file mutation unless the task explicitly wants or benefits from m1nd's own write lane.

## Multi-Repo And Cross-Boundary Work

- `federate`: combine known repos into one graph.
- `federate_auto`: discover likely sibling repos from evidence, then optionally federate them.

Use this before acting as if the current repo is the whole system.

## Coordination, Locks, And Monitoring

- `lock_create`: snapshot a region.
- `lock_watch`: define a watch strategy on a lock.
- `lock_diff`: compare current state with the lock baseline.
- `lock_rebase`: accept current state as the new baseline.
- `lock_release`: free the lock.
- `daemon_start`: start long-lived structural monitoring.
- `daemon_stop`: stop monitoring without deleting alert history.
- `daemon_status`: inspect monitor liveness and counters.
- `daemon_tick`: force one reconciliation pass.
- `alerts_list`: list durable daemon or proactive alerts.
- `alerts_ack`: acknowledge alerts so they stop resurfacing.

Use locks when coordination or baselines matter. Use daemon and alerts when the task is ongoing.

## Extended Risk, Architecture, And RETROBUILDER

- `antibody_scan`: scan for known bug shapes.
- `antibody_list`: inspect stored antibody patterns.
- `antibody_create`: create, enable, disable, or delete antibody patterns.
- `flow_simulate`: concurrency flow simulation.
- `epidemic`: predict bug spread from known buggy nodes.
- `tremor`: accelerating change-frequency detection.
- `trust`: actuarial trust scores from defect history.
- `layers`: infer architectural layers and violations.
- `layer_inspect`: inspect a specific layer.
- `ghost_edges`: temporal co-change ghost edges.
- `taint_trace`: taint propagation over the graph.
- `twins`: structural equivalence or near-equivalence.
- `refactor_plan`: graph-native refactor proposals.
- `runtime_overlay`: runtime heat and error overlays.

Use these when basic retrieval and blast-radius work are no longer enough.

## Current Live Surface On This Machine

The live surface changes with each beta. Prefer `tools/list` or
`python3 skills/m1nd-operator/scripts/probe_m1nd.py tools` for exact counts.
Recent source builds include these canonical families:

- `activate`, `impact`, `missing`, `why`, `warmup`, `counterfactual`, `predict`, `fingerprint`, `drift`, `learn`
- `ingest`, `document_resolve`, `document_provider_health`, `document_bindings`, `document_drift`
- `auto_ingest_start`, `auto_ingest_stop`, `auto_ingest_status`, `auto_ingest_tick`, `resonate`, `health`
- `perspective_start`, `perspective_routes`, `perspective_inspect`, `perspective_peek`, `perspective_follow`, `perspective_suggest`, `perspective_affinity`, `perspective_branch`, `perspective_back`, `perspective_compare`, `perspective_list`, `perspective_close`
- `seek`, `scan`, `timeline`, `diverge`
- `trail_save`, `trail_resume`, `trail_merge`, `trail_list`
- `hypothesize`, `differential`, `trace`, `validate_plan`, `federate`
- `antibody_scan`, `antibody_list`, `antibody_create`, `flow_simulate`, `epidemic`, `tremor`, `trust`, `layers`, `layer_inspect`
- `ghost_edges`, `taint_trace`, `twins`, `refactor_plan`, `runtime_overlay`
- `heuristics_surface`, `surgical_context`, `apply`, `view`, `batch_view`, `surgical_context_v2`, `apply_batch`, `edit_preview`, `edit_commit`
- `search`, `glob`, `scan_all`, `cross_verify`, `coverage_session`, `external_references`, `federate_auto`, `help`, `report`, `audit`
- `daemon_start`, `daemon_stop`, `daemon_status`, `daemon_tick`, `alerts_list`, `alerts_ack`
- `panoramic`, `persist`, `boot_memory`, `metrics`, `type_trace`, `diagram`
- `mission_start`, `mission_event`, `mission_next`, `mission_verify`,
  `mission_handoff`, `mission_close`
- `memorize`, `promote`
