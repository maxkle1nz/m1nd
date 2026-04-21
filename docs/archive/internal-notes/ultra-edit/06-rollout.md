---
Protocol: L1GHT/1.0
Node: UltraEditRollout
State: draft
Color: green
Glyph: ⍐
Completeness: 29%
Proof: phased rollout documented before implementation
Depends on:
- UltraEditContracts
- UltraEditTests
- UltraEditVision
Next:
- Phase1TextualPreview
- Phase2AtomicBatchCommit
- Phase3SemanticTargeting
- Phase4AutoRepairLoop
---

# Ultra Edit — Rollout

## Rollout Strategy

Ultra Edit should be shipped in phases, each independently valuable and testable.

[⍂ entity: Phase1TextualPreview]
[⍂ entity: Phase2AtomicBatchCommit]
[⍂ entity: Phase3SemanticTargeting]
[⍂ entity: Phase4AutoRepairLoop]

## Phase 1 — Textual Preview Transaction
Scope:
- preview handle
- in-memory candidate buffer
- diff generation
- validation report
- commit from approved preview

[⟁ tests: phase1_preview_is_stable]
[⟁ tests: phase1_commit_requires_fresh_snapshot]
[⟁ tests: phase1_no_disk_write_before_commit]

## Phase 2 — Atomic Batch Commit
Scope:
- multi-file transaction
- atomic all-or-nothing persistence
- rollback material
- single reingest pass

[⟁ tests: phase2_multi_file_atomicity]
[⟁ tests: phase2_rollback_exactness]

## Phase 3 — Semantic Targeting
Scope:
- target by symbol
- target by AST node
- ambiguity scoring
- deterministic fallback to textual mode

[⍂ entity: AmbiguityScore]
[⟁ tests: phase3_rejects_ambiguous_symbol_target]
[AMBER warning: semantic targeting must not silently degrade into unsafe text replacement]

## Phase 4 — Auto-Repair Loop
Scope:
- automatic retry strategies in preview only
- context expansion
- alternative targeting
- iterative validation

[⍂ entity: RepairStrategyPlanner]
[⟁ tests: phase4_repairs_preview_without_disk_mutation]

## Release Discipline

[RED blocker: no phase may be merged without phase-specific failure tests]
[𝔻 confidence: high]
[𝔻 evidence: layered rollout prevents shipping a magical but untrustworthy monolith]

## Success Condition

Each phase must be good enough to stand alone.
No phase should depend on future magic to be trustworthy.
