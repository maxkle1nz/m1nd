# L1GHT And Document Lanes

This file captures the document side of `m1nd`, especially the distinction between the graph-native `L1GHT` lane and the more general `universal` document lane.

## What L1GHT Is

`L1GHT` is a graph-native semantic markdown protocol.

Use it when the document is meant to be an active part of the graph, not just a file to canonicalize and later bind. In the repo's public description, `light` is "structured markdown with typed YAML frontmatter and inline semantic markers" that turns specs, design decisions, and knowledge bases into first-class graph nodes with typed edges.

Practical meaning:

- code and structured docs can live in the same graph
- `activate`, `seek`, and other structural tools can surface both implementation and semantic spec nodes together
- this is the right lane when the doc itself is authored as machine-legible graph material

## When To Use `light` vs `universal`

Use `adapter: "light"` when:

- the markdown is intentionally authored in the `L1GHT` protocol
- the document should become graph-native semantic structure directly
- you want typed nodes and typed edges extracted from headers, sections, and inline semantic markers

Use `adapter: "universal"` or `adapter: "auto"` when:

- the source is ordinary markdown, wiki, HTML, PDF, office docs, or other non-L1GHT artifacts
- you want canonical local artifacts like `canonical.md`, `canonical.json`, and `claims.json`
- you want `document_resolve`, `document_bindings`, and `document_drift` on docs that were not authored as L1GHT

Short rule:

- authored graph-native spec -> `light`
- arbitrary doc you still want inside the graph -> `universal`

## What The Local Adapter Actually Recognizes

From the current `m1nd-ingest/src/l1ght_adapter.rs` implementation:

- it accepts `.md` and `.markdown`
- it positively recognizes `Protocol: L1GHT/`
- it also heuristically recognizes L1GHT-like files when enough semantic markers exist

The current heuristic marker set includes:

- `[⍂ entity: ...]`
- `[⍐ state: ...]`
- `[⍌ event: ...]`
- `[𝔻 confidence: ...]`
- `[𝔻 ambiguity: ...]`
- `[𝔻 evidence: ...]`
- `[⟁ depends_on: ...]`
- `[⟁ binds_to: ...]`
- `[⟁ tests: ...]`
- `[RED blocker: ...]`
- `[AMBER warning: ...]`

## Header Fields Understood By The Current Adapter

The adapter parses these header-level fields today:

- `Protocol:`
- `Node:`
- `State:`
- `Color:`
- `Glyph:`
- `Completeness:`
- `Proof:`
- `Depends on:`
- `Next:`

These become graph metadata nodes and/or typed edges.

Examples of edge semantics from the adapter:

- `depends_on` from header dependencies
- `next_binding` from `Next:`
- `defines_protocol`
- `has_state`
- `has_glyph`
- `has_color`
- generic `has_metadata`

## Structural Materialization In The Graph

The current adapter creates graph-native structure from the markdown:

- the file becomes a canonical file node
- `##` headings become section nodes
- header metadata becomes concept/reference nodes
- inline semantic markers become typed concept or process nodes

Current inline relation mapping includes:

- `[⍂ entity: ...]` -> `declares_entity`
- `[⍐ state: ...]` -> `declares_state`
- `[⍌ event: ...]` -> `declares_event`
- `[⟁ depends_on: ...]` -> `depends_on`
- `[⟁ binds_to: ...]` -> `binds_to`
- `[⟁ tests: ...]` -> `declares_test`
- `[RED blocker: ...]` -> `declares_blocker`
- `[AMBER warning: ...]` -> `declares_warning`
- everything else falls back to `declares_metadata`

This means `L1GHT` is not "markdown plus tags"; it is a semantic ingest format that turns authored knowledge into graph structure.

## Small Example

Illustrative shape from the docs:

```text
---
Protocol: L1GHT/1.0
Node:     AuthService
State:    production
Depends on:
- JWTService
- SessionStore
---

## Token Validation

The [⍂ entity: TokenValidator] runs HMAC-SHA256 checks.
[⟁ depends_on: RedisSessionStore]
[RED blocker: Connection pool not yet tuned for peak load]
```

Typical mixed-graph use:

```json
{"method":"tools/call","params":{"name":"ingest","arguments":{
  "agent_id":"dev",
  "path":"./src",
  "adapter":"code",
  "mode":"replace"
}}}

{"method":"tools/call","params":{"name":"ingest","arguments":{
  "agent_id":"dev",
  "path":"./docs/specs",
  "adapter":"light",
  "mode":"merge"
}}}
```

After that, graph queries can land in code or L1GHT nodes from one search space.

## How I Should Use This As An Agent

When a task involves docs, specs, or conceptual design artifacts:

1. Ask whether the source is authored as `L1GHT` or is just a regular document.
2. If it is `L1GHT`, prefer `ingest` with `adapter: "light"` and usually `mode: "merge"`.
3. If it is regular documentation, prefer `adapter: "universal"` or `auto`.
4. Query the combined graph with `search`, `seek`, `activate`, `impact`, or `validate_plan` before falling back to manual document reading.

## Important Distinction To Remember

`L1GHT` and `document_*` are related but not identical concerns.

- `L1GHT` is an authored semantic document protocol that becomes graph-native directly.
- `document_*` tools are most relevant for universal-document handling, canonical artifacts, and doc-to-code binding/drift workflows.

So if the user says "spec" or "wiki", do not assume `universal` immediately. First check whether the material is already authored in `L1GHT`.
