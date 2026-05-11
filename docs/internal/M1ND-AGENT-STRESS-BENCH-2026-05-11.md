# m1nd Agent-First Stress Bench - 2026-05-11

## Summary

Seven read-only agent lanes tested m1nd as an agent-first repo intelligence layer.
The benchmark compared paired tasks with and without m1nd, then added one chaos
lane for non-ordinary recovery situations.

Core result: m1nd's strongest advantage was not simple file discovery. Its
advantage was operational self-awareness: it named wrong workspace bindings,
cold graphs, degraded tool surfaces, stale/open-host limits, and recovery next
steps in machine-readable form. The controls could reconstruct much of the same
taxonomy from source, but they could not prove the live host state.

## Design

- Pair A: architecture audit and system mindmap.
- Pair B: host binding and recovery taxonomy.
- Pair C: future `m1nd hosts apply` impact/change plan.
- Chaos lane: correct repo, wrong repo, cold graph, host status, update status,
  smoke harness, audit/panoramic behavior.

All lanes were read-only for repo files. m1nd lanes were allowed to mutate
runtime graph state through normal diagnostic operations, which became one of
the important findings.

## Baseline Observations

- Repo-local smoke passed with full trust.
- Smoke graph: 4,988 nodes and 10,558 edges.
- Live Codex MCP binding initially pointed at an unrelated `OLDPWD` workspace.
- `m1nd hosts status --project /path/to/m1nd` correctly marked hosts as
  `attention` because workspace binding was not proven.
- `m1nd hosts plan` emitted host-specific snippets with `M1ND_WORKSPACE_ROOT`.

## Results By Pair

| Area | m1nd lane result | no-m1nd lane result | Comparative read |
| --- | --- | --- | --- |
| Architecture audit | Found central modules quickly with `panoramic`, `audit`, smoke, and graph signals. Also exposed generated-doc ranking pollution and live transport fragility. | Built an accurate mindmap with `rg`, `cargo metadata`, source reads, and manual counting. | Control was strong but slower and less runtime-aware. m1nd added centrality/risk signals and host-state truth. |
| Recovery diagnosis | Produced live taxonomy for `blocked`, `node_count=0`, wrong workspace, stale runtime, degraded tool surface, and dead transport. | Mapped the same taxonomy statically from source/docs. | m1nd won because it proved the active host state and returned recovery payloads instead of inferred guidance. |
| Change planning | `impact`, `surgical_context_v2`, and `validate_plan` narrowed the future `hosts apply` work to npm CLI/tests/docs and avoided Rust detours. | Found the same files manually and produced a solid plan, but required broader scanning. | m1nd saved orientation time and gave blast-radius framing; control remained viable for exact CLI work. |
| Chaos | Context Guard correctly identified wrong repo/cold graph; host status/plan avoided false rebind claims. | Not applicable. | Chaos exposed the highest-value hardening backlog. |

## What m1nd Did Very Well

1. Refused false negatives: blocked retrieval carried recovery hints instead of
   pretending nothing existed.
2. Context Guard clearly separated wrong workspace binding from stale graph.
3. Repo-local smoke gave a clean proof that the runtime itself worked even when
   the live host binding was wrong.
4. `hosts status` and `hosts plan` translated runtime confusion into concrete
   host recipes.
5. `impact` and `validate_plan` helped keep future work bounded to the npm CLI
   surface instead of drifting into Rust MCP internals.

## Problems Found

1. Top-level `trust_selftest` can show `needs_ingest` while the embedded
   playbook shows `wrong_workspace_binding`; agents may chase ingest before
   rebinding.
2. `panoramic` on a cold graph can return a valid-looking empty result without
   the same recovery payload shape as `seek`.
3. `audit` appears to be graph-mutating or graph-populating from an agent's
   perspective; this should be declared explicitly.
4. Generated docs/build artifacts can dominate graph ranking for architecture
   queries.
5. Live MCP tool surface can differ from repo-local smoke surface; agents need a
   one-command chaos-safe diagnostic that explains this without mutation.
6. Multiple binary/CLI entrypoints remain confusing when `m1nd` is not on PATH
   but `node npm/bin/m1nd.js`, `/usr/local/bin/m1nd-mcp`, and managed binaries
   exist.

## Ranked Hardening Backlog

1. Make wrong-workspace binding outrank `needs_ingest` in top-level trust verdicts
   when a requested absolute scope is outside the active workspace.
2. Add recovery payloads to empty `panoramic` and other valid-looking empty graph
   responses.
3. Add side-effect class metadata to tools: `repo_read_only`,
   `runtime_mutates_graph`, `host_mutates_config`, `process_control`.
4. Add `audit` flags such as `dry_run`, `no_ingest`, or `no_graph_write`, and
   report whether audit populated runtime state.
5. Demote or tag generated artifacts such as `docs/wiki-build` during graph
   ranking, especially for architecture queries.
6. Add `m1nd chaos-safe` or equivalent: trust, context guard, host status, update
   status, and smoke handshakes without graph mutation.
7. Add `m1nd hosts explain-current-binding --project <path>` to compare live
   binding fingerprint, host config, runtime, and workspace env in one report.
8. Normalize required tool constants and docs wording across host readiness,
   trust selftest, and degraded-tool-surface recovery.
9. Add a compact fixture/test matrix for dead transport, degraded surface, cold
   graph, wrong workspace, stale binding, and stale runtime.
10. Generate tool docs/counts from `tool_schemas()` and fail CI on drift.

## Follow-Up Patch - 2026-05-12

Implemented the first two hardening cuts from this bench:

- `session_handshake` now prioritizes `wrong_workspace_binding` before
  `needs_ingest`, even when the active graph is empty.
- `trust_selftest` now surfaces wrong-workspace as the top-level verdict and
  `blocked` status instead of burying it inside the recovery playbook.
- `panoramic` now returns `proof_state`, `graph_state`, and recovery guidance
  when the graph is empty or the resulting module set is empty, preventing
  agents from treating a quiet empty panorama as true repo health.

Focused gates added:

- `trust_selftest_prioritizes_wrong_workspace_over_empty_graph`
- `panoramic_empty_graph_points_to_recovery_playbook`

## Agent Testimonials

M1ND recovery lane: m1nd did not magically fix the host, but it gave a crisp
failure grammar: cold graph, wrong workspace, degraded surface, stale split
brain, dead transport. The scoped retrieval recovery payload made the diagnosis
snap into focus.

M1ND architecture lane: m1nd was useful for first-pass topology. `panoramic`
surfaced central modules, `audit` caught repo state, and the repo-local smoke
diagnosed host/session split. The awkward parts were generated-doc ranking and
live transport fragility.

M1ND impact lane: m1nd felt like adult supervision. It surfaced that the binding
itself was suspect before trusting retrieval, then narrowed the change plan to
the npm CLI surface.

Chaos lane: `trust_selftest`, `session_handshake`, `seek`, and
`recovery_playbook` felt like a brain because they named the mismatch and gave
the next move. `audit` and `panoramic` need clearer side-effect and empty-state
semantics.

## Benchmark Conclusion

m1nd already gives agents a real operational advantage in complex repositories.
The advantage is strongest in recovery, context selection, multi-agent safety,
blast-radius planning, and continuity. Plain shell search is still competitive
for exact file mapping, but it cannot prove whether the active agent host is
bound to the right graph, runtime, workspace, or tool surface.

The next scientific benchmark should add timing logs, scored answer keys, and
repeatable fixtures for the failure modes above.
