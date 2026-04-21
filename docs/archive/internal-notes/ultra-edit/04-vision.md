---
Protocol: L1GHT/1.0
Node: UltraEditVision
State: draft
Color: blue
Glyph: ⍐
Completeness: 26%
Proof: architectural insertion points identified at high level
Depends on:
- UltraEdit
- m1ndApply
- ApplyBatch
- L1ghtAdapter
Next:
- TransactionBufferImplementation
- ValidationIntegration
- CommitProtocol
---

# Ultra Edit — Vision / Oracle

## Architectural Intuition

Ultra Edit should not replace m1nd's current surgical tools. It should sit above them as an intent-to-materialization layer.

[⍂ entity: IntentToMaterializationLayer]
[⍂ entity: TransactionBufferImplementation]
[⍂ entity: CommitProtocol]
[⍂ entity: PreviewValidator]

## Likely Reuse Points

[⟁ depends_on: SurgicalContextV2]
[⟁ depends_on: ApplyBatch]
[⟁ depends_on: VerifyPipeline]
[⟁ depends_on: L1GHTProtocol]

- SurgicalContextV2 already gives workspace snapshots.
- ApplyBatch already models atomic persistence.
- Verify pipeline already reasons about post-write coherence.
- L1GHT can model contracts and rollout as graph-native docs.

## Missing Primitives

[RED blocker: No buffer layer that represents pending edits as first-class objects]
[RED blocker: No preview handle lifecycle]
[RED blocker: No semantic target resolver contract]
[AMBER warning: no explicit snapshot freshness check before commit]

## System Shape

1. Resolve target.
2. Build candidate in memory.
3. Validate candidate.
4. Return preview object.
5. Commit approved preview atomically.
6. Reingest once.
7. Emit receipt.

[⍂ entity: PreviewObject]
[⍂ entity: SnapshotFreshnessCheck]
[⍂ entity: CommitReceipt]
[⍂ entity: CandidateDiff]

## Rollout Thought

The first release should not try to solve every edit problem.

Phase order:
- textual preview transaction
- batched transaction
- semantic targeting
- graph-guided auto-repair loop

[⟁ binds_to: UltraEditContracts]
[⟁ binds_to: UltraEditTests]
[⟁ tests: rollout_phase_1_textual_preview_is_stable]
[⟁ tests: rollout_phase_2_batch_atomicity_is_stable]
