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
- `[𝔻 confidence: X]` -> `epistemic_confidence` edge; WEIGHT equals X (numeric 0.0–1.0 or word: low/medium/high/certain)
- `[𝔻 ambiguity: ...]` -> `epistemic_ambiguity` edge
- `[𝔻 evidence: path]` -> `evidenced_by` edge at ingest time; after merge with the ingested code graph, resolves to a `grounded_in` edge pointing to the actual code node
- everything else falls back to `declares_metadata`

The 𝔻 markers are fully parsed — they produce typed epistemic edges in the graph, not just heuristic recognition signals. Confidence and ambiguity sharpen activation scoring; evidence paths create cross-domain bridges between knowledge nodes and code nodes.

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

## Authoring Agent Memory With `memorize` + The Freshness Lifecycle

When an agent concludes something durable — a verified finding, a design decision, why code is structured a certain way — it can persist that knowledge with `memorize`. The result is a valid L1GHT `.light.md` that lives in the graph alongside code, survives sessions, and self-flags when the code it cites changes.

### What `memorize` does

Input fields:

```
agent_id        required
node_label      required — the memory node name
claims          required — array of claim objects:
  label         required
  text          optional — claim prose
  kind          optional — entity | state | event (default: entity)
  confidence    optional — low | medium | high | certain, or 0.0–1.0
  ambiguity     optional — prose describing open questions
  evidence      optional — array of repo-relative code paths
  depends_on    optional — array of other claim labels this depends on
title           optional
state           optional
output_path     optional
namespace       optional
ingest_after    optional — default true; set false to only write the file
mode            optional — default merge
```

It writes a `.light.md` under `<runtime_root>/agent-memory/`, ingests it (adapter: light, mode: merge by default), and anchors each `evidence` path to the real `file::<path>` code node via a `grounded_in` edge. Returns `path`, `light_evidence_resolved`, `light_evidence_unresolved`, and a `next_action` hint.

IMPORTANT: ingest the target code BEFORE calling `memorize` so evidence paths resolve to real code nodes. If evidence is unresolved, `next_action` will say so.

### Promotion — the audited crossing (`promote`, MEDULLA M6)

A `memorize` is ALWAYS project-private (born in the routed brain, stamped `Origin-Brain`). Knowledge becomes shared doctrine only through an EXPLICIT `promote` — never automatically. When a VERIFIED claim is genuinely transversal (true across projects, not one repo's fact), call:

```
promote { agent_id, brain: <source project root>, claim: <slug>, reason: <one line why it is transversal> }
```

What it does (owner-level cross-store, served at the routed HTTP door):

1. Loads the claim from the source brain's store (hard error on an unknown slug — no guessing).
2. **C8.3 evidence-class gate:** only `State: verified` OR `Source-Agent: human:maintainer` may promote. A declared maker finding is refused with a typed reason — it must be verified in its home brain first.
3. **Hygiene floor:** the claim text is scanned for secrets + merge-conflict markers (the medulla is the most-read store and must be the cleanest); a hit is refused.
4. **C8.2 evidence re-anchor:** code evidence is rewritten origin-qualified (`<origin_root>#<path>`) so medulla-side freshness delegates back to the origin brain. When no origin root resolves, the claim is stamped `Evidence-Unverifiable: true` and renders as declared tissue — a medulla claim never reads fresher than it can prove.
5. Writes the medulla copy through the same supersession gate `memorize` uses (a weaker re-promotion of a live medulla claim bounces `WouldDowngrade`), stamping the four provenance fields `Origin-Brain` / `Origin-Claim` / `Promoted-By` / `Promotion-Reason` — the readable chain ("learned in Y by A, promoted by B, reason R").
6. Stamps the project ORIGINAL `Promoted-To: medulla@<slug>@<ms>` (a same-strength supersession; the `.history/` keeps the pre-promotion copy). Promotion ELEVATES, never moves — the witness stays.
7. Re-ingests the medulla copy so it immediately surfaces in every brain's default beat under `tier: medulla`.

**Etiquette (provenance, not a security boundary):** any `agent_id` CAN call `promote` — `agent_id` is self-declared — but promotion is an ORCHESTRATOR / maintainer act. A maker PROPOSES (memorizing "candidate for promotion" is just a claim); the orchestrator executes. Every promotion is auditably attributed by `Promoted-By`; don't promote an unverified maker hunch.

**Demotion (un-share, never destroy):** `learn wrong` on the MEDULLA copy (kills its trust, marks it for the consolidation pass) or a superseding medulla `memorize` (e.g. a `moved_to:` back-pointer when a "cross-project" finding turns out to be one repo's quirk). Demotion NEVER touches the project witness — the local truth that was and remains correct at home is preserved.

### Boot auto-load

On every session start, m1nd auto-ingests all `<runtime_root>/agent-memory/*.light.md` files. This is gated by `M1ND_AUTO_LOAD_AGENT_MEMORY` (default ON). Past findings are present in the graph at the start of the next session — no explicit re-ingest needed. The result is reported in `session_handshake.agent_memory` with `{dir, file_count, loaded, nodes_added, ...}`.

### `session_handshake` now includes `graph_intelligence`

The handshake response includes a `graph_intelligence` block:

- `top_pagerank`: structural entry points ranked by PageRank — useful for knowing where to start in an unfamiliar repo
- `attention_anchors`: top nodes by query-access frequency; empty with an explanatory note if no queries have run yet this session
- `memory`: `{light_nodes, grounded_in_edges}` — how many agent-memory nodes are loaded and how many are anchored to code

These are honest-zero when the signal is not yet computed.

### Staleness detection

`cross_verify(check: ["evidence_freshness"])` re-hashes each `grounded_in` code target against the hash recorded at ingest and returns `stale_evidence[]` + `stale_evidence_count` naming which memorized claims cite changed code. `check` is an array; other valid values: `existence`, `loc`, `hash`.

After a CODE re-ingest (`mode: merge`), the ingest result itself includes `memory_freshness: {stale_evidence_count, stale_evidence[]}` — memory flags stale right at the moment code changes, without needing a separate cross_verify call.

Caveat: `ingest mode: replace` wipes light memory nodes and `grounded_in` edges. Use `mode: merge` to preserve agent memory across code re-ingests, or rely on boot auto-load to restore it at the next session start.

### One-step persistence from Mission Control

`mission_close(write_light_memory: true)` persists the mission's verified claims as L1GHT memory in a single step. The returned `light_memory` field gives the path to the written file.
