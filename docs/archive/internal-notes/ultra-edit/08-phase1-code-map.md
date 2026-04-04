---
Protocol: L1GHT/1.0
Node: UltraEditPhase1CodeMap
State: draft
Color: cyan
Glyph: ⍌
Completeness: 41%
Proof: code map grounded against current Rust implementation
Depends on:
- UltraEditContracts
- UltraEditTransactionModel
- SurgicalContextV2
- ApplyBatch
Next:
- Phase1Implementation
- PreviewHandleStore
- EditPreviewProtocol
---

# Ultra Edit — Phase 1 Code Map

## Relevant Rust Files

[⍂ entity: m1nd-mcp/src/protocol/surgical.rs]
[⍂ entity: m1nd-mcp/src/surgical_handlers.rs]
[⍂ entity: m1nd-mcp/src/server.rs]
[⍂ entity: m1nd-core/src/snapshot.rs]
[⍂ entity: m1nd-core/src/snapshot_bin.rs]
[⍂ entity: m1nd-ingest/src/diff.rs]

## Existing Reusable Primitives

[⍂ entity: ApplyInput]
[⍂ entity: ApplyOutput]
[⍂ entity: ApplyBatchInput]
[⍂ entity: ApplyBatchOutput]
[⍂ entity: VerificationReport]
[⍂ entity: GraphSnapshot]
[⍂ entity: GraphDiff]

## Existing Handlers

[⍂ entity: handle_surgical_context_v2]
[⍂ entity: handle_apply]
[⍂ entity: handle_apply_batch]
[⟁ binds_to: current surgical tool dispatch]

## Phase 1 Insertion Strategy

- Add new protocol structs beside ApplyInput/ApplyOutput.
- Add preview/commit handlers beside handle_apply.
- Reuse diff generation from current surgical handler path.
- Reuse snapshot infrastructure for freshness material.
- Reuse apply/apply_batch only at final commit boundary.

[AMBER warning: server schema/tool registration must stay in lockstep with protocol structs]
[RED blocker: preview state storage mechanism does not exist yet]
[⟁ tests: phase1_code_map_matches_real_files]
