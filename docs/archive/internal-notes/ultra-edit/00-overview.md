---
Protocol: L1GHT/1.0
Node: UltraEdit
State: draft
Color: cyan
Glyph: ⍌
Completeness: 18%
Proof: Wave 1 pre-build specification initiated
Depends on:
- SurgicalContextV2
- ApplyBatch
- VerifyPipeline
- L1GHTProtocol
Next:
- UltraEditAdversary
- UltraEditContracts
- UltraEditTests
- UltraEditVision
---

# Ultra Edit

Ultra Edit is m1nd's future transactional editing system for AI agents.

## Core Purpose

Transform intention into safe code changes through a staged pipeline:

[⍂ entity: EditPreviewBuffer]
[⍂ entity: TransactionWorkspace]
[⍂ entity: SemanticTargetResolver]
[⍂ entity: ValidationPipeline]
[⍂ entity: CommitCoordinator]

The system must allow an LLM to modify code in memory first, validate the result, and only then persist it to disk.

[⟁ depends_on: SurgicalContextV2]
[⟁ depends_on: ApplyBatch]
[⟁ depends_on: VerifyPipeline]
[⟁ binds_to: m1nd.apply]
[⟁ binds_to: m1nd.apply_batch]

## Non-Negotiable Principles

[𝔻 confidence: high]
[𝔻 evidence: apply currently performs full file replacement; patch semantics are external]
[⍐ state: design-first]

- No disk writes before preview approval.
- Preview state must be structurally inspectable.
- Validation must happen on the in-memory candidate.
- Commit must be atomic by default.
- Rollback must be a first-class primitive.
- Small edits should not require full-file regeneration by the LLM.

## Why This Matters

Current LLM editing is fragile because semantic intent is converted directly into file writes.

[RED blocker: No in-memory transactional edit layer exists today]
[AMBER warning: apply semantics can be misused as patch semantics by agents]

Ultra Edit aims to become the bridge between graph intelligence and trustworthy code materialization.

## Desired Outcome

A future where an AI can:

1. Understand the exact target.
2. Build a candidate state in memory.
3. Validate syntax, structure, graph impact, and optionally tests.
4. Iterate if needed.
5. Persist only the approved state.

[⟁ tests: ultra_edit_preview_does_not_touch_disk]
[⟁ tests: ultra_edit_commit_is_atomic]
[⟁ tests: ultra_edit_validation_rejects_broken_candidate]
