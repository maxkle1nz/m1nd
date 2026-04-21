---
Protocol: L1GHT/1.0
Node: UltraEditTransactionModel
State: draft
Color: cyan
Glyph: ⍂
Completeness: 34%
Proof: first transaction lifecycle documented
Depends on:
- UltraEditContracts
- UltraEditAdversary
- UltraEditTests
- ApplyBatch
Next:
- PreviewHandleStore
- SnapshotFreshnessProtocol
- CandidateRebaseEngine
---

# Ultra Edit — Transaction Model

## Core Lifecycle

[⍂ entity: PreviewHandleStore]
[⍂ entity: CandidateBuffer]
[⍂ entity: SourceSnapshot]
[⍂ entity: CandidateDiff]
[⍂ entity: CandidateRebaseEngine]
[⍂ entity: CommitLock]
[⍂ entity: RollbackReceipt]

Ultra Edit must behave like a transaction system, not a text replacement helper.

## Lifecycle Stages

### 1. Preview Creation
- read source snapshot
- resolve targets
- apply intended edits in memory
- compute candidate diff
- validate candidate
- emit preview handle

[⟁ tests: preview_handle_contains_snapshot_and_diff]
[⟁ tests: preview_creation_is_side_effect_free]

### 2. Preview Iteration
- refine candidate in memory
- re-run validation without touching disk
- preserve transaction identity where possible

[⍂ entity: PreviewRevision]
[AMBER warning: repeated edits must not leak stale offsets across revisions]

### 3. Commit Gate
Before writing, commit must verify:
- source snapshot still matches disk
- validation report is still acceptable
- atomic boundary is intact
- lock can be acquired

[⍂ entity: SnapshotFreshnessProtocol]
[⍂ entity: CommitEligibility]
[⟁ tests: commit_rejects_stale_preview]
[⟁ tests: commit_requires_lock]

### 4. Persistence
- write all touched files atomically
- reingest once
- emit commit receipt
- optionally persist rollback material

[⟁ depends_on: ApplyBatch]
[⟁ binds_to: m1nd.edit_commit]
[⟁ tests: atomic_commit_produces_single_receipt]

### 5. Rollback
Rollback must be trivial and explicit.

[⍂ entity: RollbackMaterial]
[⍂ entity: RollbackCommand]
[RED blocker: rollback cannot depend on the LLM reconstructing previous file state]
[⟁ tests: rollback_restores_exact_pre_commit_snapshot]

## Invariants

[⍐ state: required]

- every preview is tied to a source snapshot
- every commit derives from a specific preview or equivalent snapshot bundle
- candidate diff is stable between preview and commit
- rebase is explicit, never implicit magic
- rollback data is generated before destructive persistence

## Design Principle

Ultra Edit should feel like a tiny VCS transaction inside m1nd.

[𝔻 confidence: high]
[𝔻 evidence: trust requires snapshot, diff, commit, and rollback semantics]
