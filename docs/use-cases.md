# Workflows Worth Trying

Start with [setup](../README.md#get-started) and a read-only task. These workflows describe intended use, not measured time savings or guaranteed bug detection.

## Understand an unfamiliar repository

Ask: “Where is configuration loaded, and what calls it?”

1. `north` establishes the repository binding and task context.
2. `seek` finds candidate code; `surgical_context` or a source read supplies evidence.
3. `why` can explain a graph connection. Check unresolved or inferred edges.

A useful result is a small set of real source locations with a checkable explanation. It is not a count of files the agent avoided reading.

## Prepare a multi-file change

Ask: “Before changing this function, which connected code should I inspect?”

Use a returned node ID with `impact`; inspect its callers, imports, and relevant tests. Graph relationships help choose the review surface. They do not tell you exactly what will break or replace compiler/type checking.

Advanced editing tools have additional preview, proof-permit, and authority boundaries. Consult the [write reference](wiki/src/api-reference/lifecycle.md) rather than copying an old `apply` example into a live repository. Static/graph verification is not an executed test suite.

## Resume with evidence

Ask: “What did the previous session establish about this subsystem, and is it still true?”

`north` and `boot_memory` surface prior claims; `trail_resume` can restore an investigation. Inspect provenance, age, evidence paths, and freshness. `cross_verify` checks cited evidence for staleness. After a new finding is actually verified, `memorize` preserves it with repo-relative evidence.

## Coordinate several agents locally

Run one served owner for an already prepared repository and attach clients from that repository. The graph and memory are shared; live presence and overlap notices help agents recognize concurrent work. These notices do not lock files or prove edits are compatible. Coordinate ownership and retain independent review/tests.

See [deployment](deployment.md) and [multi-agent workflows](wiki/src/tutorials/multi-agent.md).

## Connect documents to code

Document adapters can represent supported notes, specifications, and papers in the graph. Queries can traverse document and code nodes when they are already present. Provider dependencies and authority matter: generic public `ingest` with `mode: "merge"` is not an available import shortcut. Optional remote providers may transmit documents to a configured endpoint.

Check provider health and the [lifecycle reference](wiki/src/api-reference/lifecycle.md). A document/code association is a candidate relationship to inspect, not proof that the implementation satisfies the specification.

## Know when to use something simpler

Use a direct file read or `rg` for a small exact lookup. Use the compiler/LSP for language semantics, tests for executed behavior, and a profiler for performance. Structural heuristics such as `missing`, `hypothesize`, and `antibody_scan` suggest investigation targets; they are not certified bug detectors.

For measurements, read [the evidence guide](wiki/src/benchmarks.md). Earlier self-authored case studies do not establish universal token savings, ROI, or superiority to a human review.
