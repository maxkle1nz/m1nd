# MASSIF PRD

## 1. Problem and thesis

MASSIF is a Human View lens for reading a codebase as proof topography: a stable isometric maquette of blocks where each piece keeps its place across snapshots and carries the structural evidence state already computed by m1nd.

The problem is not missing graph data. The engine already computes structural evidence per node and can persist it as `xray:state:*` tags. The current UI can fetch snapshots whose nodes include `tags`, but no tool renders those states as a stable place an operator can revisit. Existing graph surfaces show nodes and edges; familiar result grammars tend to encode execution outcome. MASSIF makes structural evidence a spatial grammar: block fill, border, glyph, texture, and detail copy communicate what is wired, unwired, growing, drifting, or not scanned yet.

MASSIF is not a parallel product. It enters the existing Human View shell beside Hall, LivingTree, and GraphCanvas, reusing the same navigation and honest empty-state stance.

## 2. Users

1. **Owner-operator.** A technical owner who wants to see structural evidence state at a glance before opening source files.
2. **Vibecoders.** People building software by directing AI agents who do not read code line by line, but still need a reliable map of where structural evidence exists and where attention is owed.
3. **AI agents.** Agents read the same truth through existing verbs and snapshot data, so the visual language and machine language stay aligned.

## 3. Source-grounded facts

| Fact | Repo grounding |
|---|---|
| Snapshot nodes already carry `tags`, so persisted `xray:state:*` tags can be read by the UI without a new backend endpoint. | `m1nd-ui/src/lib/snapshot.ts:32-40`, `m1nd-ui/src/api/client.ts:102-105` |
| `xray_paint` is a write verb that persists `xray:state:<state>` tags only on commit; dry-run computes counts and writes nothing. | `m1nd-mcp/src/xray_handlers.rs:2353-2386`, `m1nd-mcp/src/xray_handlers.rs:2615-2737`, `m1nd-mcp/src/xray_handlers.rs:2739-2780`, `m1nd-mcp/src/server.rs:2558-2582`, `m1nd-mcp/src/server.rs:2611-2630` |
| The persistent node tags are `xray:state:bedrock`, `xray:state:overgrowth`, `xray:state:unproven`, and `xray:state:erosion-candidate`; MASSIF adds a client-side `unpainted` display state for absence of any `xray:state:*` tag. | `m1nd-mcp/src/xray_handlers.rs:2392-2473`, `m1nd-ui/src/lib/snapshot.ts:32-40` |
| Bedrock means proof evidence exists: test-exercised or grounded, not semantic correctness. | `m1nd-mcp/src/xray_handlers.rs:2415-2426`, `m1nd-mcp/src/xray_handlers.rs:2487-2513`, `m1nd-mcp/src/server.rs:2561-2563` |
| Erosion-candidate is derived from manifest rules and the source side of a flagged cross-module edge. Manifest ratification changes `xray_gate` severity, not whether paint can flag a candidate. | `m1nd-mcp/src/xray_handlers.rs:1779-1815`, `m1nd-mcp/src/xray_handlers.rs:1951-1966`, `m1nd-mcp/src/xray_handlers.rs:2115-2130`, `m1nd-mcp/src/xray_handlers.rs:2288-2306`, `m1nd-mcp/src/xray_handlers.rs:2536-2582` |
| `layers` output is architectural layering and truncates node lists by default. It is not full container membership unless requested that way. | `m1nd-core/src/layer.rs:113-128`, `m1nd-mcp/src/layer_handlers.rs:8538-8588`, `m1nd-mcp/src/layer_handlers.rs:8666-8679` |
| `coverage_session` measures what an agent visited in the current session, not product proof coverage. | `m1nd-mcp/src/audit_handlers.rs:1039-1075` |
| Trust envelope has a fourth verdict, `unprovable`, in the Rust protocol; the frontend type currently omits it and must be fixed before relying on trust envelope data. | `m1nd-mcp/src/protocol/layers.rs:141-152`, `m1nd-mcp/src/protocol/layers.rs:229-233`, `m1nd-ui/src/api/toolTypes.ts:131-145` |
| The current Human View already has Hall, LivingTree, layer lenses, best-effort trust and tremor decorations, meaning search, and a ReactFlow graph surface. MASSIF must integrate as a new lens or view in that shell. | `m1nd-ui/src/components/hall/HallView.tsx:203-238`, `m1nd-ui/src/components/hall/HallView.tsx:254-318`, `m1nd-ui/src/components/tree/LivingTree.tsx:141-153`, `m1nd-ui/src/components/tree/LivingTree.tsx:241-254`, `m1nd-ui/src/components/tree/LivingTree.tsx:366-382`, `m1nd-ui/src/components/GraphCanvas.tsx:119-171`, `m1nd-ui/src/lib/treeLenses.ts:83-125` |
| The current UI dependency set is React, ReactDOM, ReactFlow, Dagre, Lucide, and Zustand; Pixi, Three, and D3 are not present today. | `m1nd-ui/package.json:16-22` |
| The documentation gate requires UI slices to use real captured envelopes and same-PR docs when behavior ships. | `docs/PATHOS.md:207-221` |

## 4. Live sample baseline captured 2026-07-06

Use these numbers verbatim in fixtures, examples, and acceptance screens:

- `scanned=199`
- `bedrock=0`
- `overgrowth=195`
- `unproven=4`
- `erosion_candidate=0`
- `proof_coverage=0.0`
- Manifest present.
- Layers detected: 4.
- Total nodes classified: 199.
- Layer names repeat: `data_access` appears at `L0` and `L1`, so a layer name alone is not a container key.
- The captured snapshot slice has 40 nodes, total graph nodes are 199, total edges are 184, and the slice has zero `xray:state:*` tags. The live graph is therefore currently 100% unpainted from the UI's snapshot perspective.

## 5. Five-state grammar

Copy law: MASSIF never tells the operator a block is semantically correct or finished. The approved words are **structural evidence**, **test-exercised or grounded**, **candidate drift**, and **not scanned yet**. Bedrock is a structural evidence state, not a correctness guarantee. Absence of a state tag renders as `unpainted`, never as `unproven`.

| State | Exact tag | Technical meaning | Operator label | Required redundant channel |
|---|---|---|---|---|
| `bedrock` | `xray:state:bedrock` | Node has proof evidence: test-exercised or grounded. | solid | filled stone glyph plus stable border |
| `overgrowth` | `xray:state:overgrowth` | Node is orphaned over reference relations: zero incoming reference edges. | growing | sprout glyph plus woven texture |
| `unproven` | `xray:state:unproven` | Node is used, but has no proof evidence signal. | unwired | hollow center glyph plus dashed fill |
| `erosion-candidate` | `xray:state:erosion-candidate` | Node is the source of a cross-module edge flagged by the active manifest rules. This is candidate drift, not a confirmed violation. | drifting | crack glyph plus notched edge |
| `unpainted` | no `xray:state:*` tag | The graph snapshot has no persisted paint state for this node yet. | not scanned yet | neutral hatch plus scan-needed glyph |

## 6. Functional requirements

**F1. Data source.** MASSIF MVP reads `xray:state:*` tags from nodes already present in `/api/graph/snapshot`. It adds no backend verb and does not call `xray_paint` as a read path. If a node has no `xray:state:*` tag, it renders as `unpainted`.

**F2. State derivation.** Client state derivation is deterministic: pick the first matching tag in this precedence order: `erosion-candidate`, `bedrock`, `overgrowth`, `unproven`; if none exists, use `unpainted`. If multiple state tags appear, display the highest-precedence state and surface a detail-panel warning that repaint should replace stale state tags.

**F3. Container membership.** The primary container axis is path-prefix and tree membership from snapshot node IDs and source paths. Architectural layers are an optional lens, not the canonical ownership model.

**F4. No rollup from truncated samples.** A layer lens may be shown only when the call requests full membership for the snapshot size, or when the UI marks the layer sample as incomplete and refuses to use it for rollup. For the live sample of 199 nodes, request `max_nodes_per_layer` greater than or equal to 199 if using the `layers` verb for membership. Never compute container ratios from the default 40-node-per-layer sample.

**F5. Layer identity.** Layer containers key by level plus name, not name alone, because the live sample has `data_access` at both `L0` and `L1`.

**F6. Container rollup.** A container fill is proportional: its visible packed sub-blocks represent the ratio of child states in full membership. It is never all-or-nothing. A container border is binary: wired when no holes are flagged, notched when structural holes or missing-edge signals are present.

**F7. Stable position.** A node keeps the same visual address across snapshots when its stable key is unchanged. Stable ordering uses deterministic keys: path-prefix, source path, external ID, then label. State changes alter appearance in place, not the place itself.

**F8. Semantic zoom.** Zoom level changes grammar detail, not truth:

- Far: hue and broad texture only.
- Mid: proportional fills, border state, layer or path label, and compact glyphs.
- Near: individual pieces, readable state glyphs, count strips, and click targets.
- Detail: side panel with exact tags, evidence line, connectedness, and `manifest_source`.

**F9. Detail panel.** Clicking a piece opens a panel with: label, external ID, state, exact state tag or `no xray state tag`, evidence line, connectedness summary, source path when available, `manifest_source`, and a short copy-law-compliant explanation. If no manifest is active, show `manifest_source: no manifest active` and disable erosion interpretation.

**F10. First-run unpainted screen.** A 100% unpainted graph is not an error. It is a first-class screen using the real sample counts: `199 nodes in snapshot scope`, `0 painted tags in current snapshot`, and a call to run the X-RAY paint scan in the appropriate operator flow. In a read-only attach session the scan is unavailable by design (`xray_paint` is write-denied there): the control renders disabled with the copy `read-only attach: scanning unavailable here` — never a dead control.

**F11. Colorblind-safe redundancy.** Every state has at least two non-hue cues: glyph, texture, border, hatch, crack, dash, or shape. Hue alone is never the state channel.

**F12. Manifest source.** MASSIF always surfaces manifest state in detail and no-manifest screens. With a present manifest, say `manifest present`; with no active manifest, say `no manifest active`; with an invalid manifest, say `manifest unavailable` and disable erosion interpretation.

**F13. Proof coverage.** `proof_coverage` may appear only as an aggregate from X-RAY paint output, not as per-container coverage unless a full-membership persisted-state fixture exists. The live sample acceptance scenario uses `proof_coverage=0.0`.

**F14. Trust envelope.** Trust/confidence is excluded from the resting block channel in MVP. Phase 2 may show it only on hover or in the detail panel, explicitly sourced from the persisted defect ledger and calculated envelope. Before any UI reads it, fix `m1nd-ui/src/api/toolTypes.ts` so `TrustEnvelope.verdict` includes `unprovable`.

**F15. Human View integration.** MASSIF is exposed as a view or lens in the Human View shell, beside Hall, LivingTree, and GraphCanvas. It reuses the viewed-brain selector, same empty-state grammar, and same route-to-current-brain behavior.

**F16. Public copy discipline.** User-facing text must speak as an operator map, not as an implementation tutorial. Internal implementation terms may appear in this PRD and tests, but not in public UI labels.

## 7. Non-goals

- Trust/confidence in the resting block channel. Phase 2 hover may use the persisted defect ledger and trust envelope after the frontend type is fixed.
- 3D/WebGPU floor in MVP.
- Voronoi treemap.
- Any new backend verb in MVP.
- Using `coverage_session` as a health axis. It measures what the agent looked at, not structural evidence.
- Treating architectural layers as functional subsystems without an explicit product mapping.
- Rendering `BLUEPRINT` or `UNPROVABLE` as MASSIF node states. `BLUEPRINT` is a manifest existence axis; `unprovable` is a trust envelope verdict, not a block state.

## 8. Phasing

### MVP: Canvas 2D isometric

- React view in the existing Human View shell.
- Snapshot tags as source of truth.
- `d3-hierarchy` squarified layout for path-prefix containers.
- Hand-rolled Canvas 2D three-face prisms.
- `d3-zoom` for pan and semantic zoom.
- Estimated extra dependency footprint around 35KB, offline.
- Acceptance target: hundreds to low thousands of blocks.

### Target: PixiJS v8 plus viewport

Move to PixiJS v8 and a viewport layer when real graphs exceed roughly 5k visible blocks or when interaction latency exceeds the UI budget on supported machines.

### Ceiling: deferred 3D

React Three Fiber or WebGPU remains deferred unless the product truly requires free 3D rotation. MASSIF's core promise is stable proof topography, not a navigable 3D world.

## 9. Pre-work

P0 before implementation: update `m1nd-ui/src/api/toolTypes.ts` so `TrustEnvelope.verdict` accepts `unprovable`. The Rust protocol documents the verdict set as `act`, `reverify`, `abstain`, and `unprovable`, while the frontend type currently lists only `act`, `reverify`, and `abstain` (`m1nd-mcp/src/protocol/layers.rs:141-152`; `m1nd-ui/src/api/toolTypes.ts:131-145`).

## 10. Success metrics

1. **Correct source of state.** A fixture snapshot with persisted `xray:state:*` tags renders all five visual states, including `unpainted` when tags are absent.
2. **Rollup correctness.** Container proportions match full membership counts and fail closed when layer membership is truncated.
3. **Live-sample acceptance.** The overgrowth-dominant sample (`scanned=199`, `overgrowth=195`, `unproven=4`, `bedrock=0`, `erosion_candidate=0`, `proof_coverage=0.0`, manifest present) remains legible and does not look like a broken UI.
4. **First-run honesty.** A 100% unpainted snapshot produces the first-run screen, not an alarming failure state and not an `unproven` map.
5. **Colorblind-safe state reading.** Each state is identifiable without hue in a monochrome or simulated colorblind check.
6. **Manifest honesty.** No-manifest and invalid-manifest scenarios disable erosion interpretation and explain the source state.
7. **Human View continuity.** MASSIF opens from the same shell and brain selector as Hall and LivingTree.

## 11. Validation plan

- Add a real fixture snapshot with `xray:state:*` tags and one fixture with no state tags.
- Add a rollup unit test that compares path-prefix membership counts to rendered proportions.
- Add a layer-truncation test that refuses ratio rollup when `nodes_truncated=true` or when `nodes_returned < node_count`.
- Add no-manifest and invalid-manifest tests for copy and disabled erosion interpretation.
- Add colorblind redundancy review: glyph-only, texture-only, and monochrome snapshots must remain readable.
- Add acceptance coverage for the live sample captured 2026-07-06: `scanned=199`, `bedrock=0`, `overgrowth=195`, `unproven=4`, `erosion_candidate=0`, `proof_coverage=0.0`, manifest present, 4 detected layers with duplicate `data_access` names, and current snapshot state 100% unpainted.
- When MASSIF ships as product behavior, update the Human View documentation and `docs/PATHOS.md` in the same PR.

## 12. Risks and mitigations

| Risk | Why it matters | Mitigation |
|---|---|---|
| Intended-model dependency | MASSIF inherits the credibility of the active manifest. A weak manifest can make candidate drift look meaningful when the model is incomplete. | Always surface `manifest_source`; show `no manifest active` when absent; call the red state candidate drift, not a verdict. |
| False-confidence trap | A beautiful map can make structural evidence look stronger than it is. | Copy law: say structural evidence, test-exercised or grounded, candidate drift, and not scanned yet. Never imply semantic correctness. |
| Public-repo reputation | Public UI and docs can overclaim if they use result words carelessly. | Keep operator copy humble, observable, and source-tied. Reserve implementation mechanics for internal docs and tests. |
| Layer misuse | Architectural layers can be mistaken for product subsystems, and default layer output can be truncated. | Path-prefix tree is primary. Layers are optional and must request full membership before rollup. |
| First-run shock | The current live graph is 100% unpainted from the snapshot. | Treat unpainted as a normal first-run screen with a clear scan call-to-action. |

## 13. Required changes coverage map

| Required change | PRD section |
|---|---|
| 1. Specify per-node paint-state source or add read-only state surface; do not claim dry-run supplies per-node states. | Sections 3, 6 F1, 11 |
| 2. Correct erosion: manifest plus rule present; ratification only affects gate severity. | Sections 3, 5, 6 F12, 12 |
| 3. Remove `coverage_session` from health axis. | Sections 3, 7 |
| 4. Resolve layer membership truncation before rollup. | Sections 3, 6 F4, 11 |
| 5. Define containers explicitly; layers are architectural, not functional subsystems. | Sections 6 F3-F5, 7 |
| 6. Define honest rule for absent tags. | Sections 5, 6 F1-F2, 6 F10 |
| 7. Correct frontend trust envelope type path and include `unprovable`. | Sections 3, 6 F14, 9 |
| 8. Keep confidence/trust out of resting MVP channel. | Sections 6 F14, 7, 8 |
| 9. Integrate with existing Human View patterns, not a parallel UI. | Sections 1, 3, 6 F15 |
| 10. Replace overclaiming copy with structural evidence language. | Sections 5, 6 F16, 12 |
| 11. Include minimum validation before ship. | Sections 10, 11 |
| 12. Update PATHOS or Human View docs in same PR when behavior ships. | Sections 3, 11 |
