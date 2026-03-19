---
Protocol: L1GHT/1.0
Node: UltraEditPhase1BuildCut
State: draft
Color: green
Glyph: ⍐
Completeness: 52%
Proof: build cut frozen before code changes
Depends on:
- UltraEditPhase1CodeMap
- UltraEditContracts
- UltraEditTransactionModel
Next:
- EditPreviewInputOutput
- EditCommitInputOutput
- PreviewStateStore
---

# Ultra Edit — Phase 1 Build Cut

## What will be built now

[⍂ entity: EditPreviewInputOutput]
[⍂ entity: EditCommitInputOutput]
[⍂ entity: PreviewStateStore]
[⍂ entity: SourceFileSnapshot]
[⍂ entity: CandidateDiffReport]

Phase 1 is intentionally textual and transactional.

- preview takes full replacement content for a single file
- preview reads source snapshot
- preview computes diff summary and unified diff
- preview stores candidate state in memory
- preview returns handle + validation metadata
- commit consumes preview handle
- commit re-checks source freshness
- commit persists atomically through existing apply path

## What is explicitly out of scope

- semantic targeting
- symbol-aware editing
- AST transforms
- auto-repair loop
- multi-file preview transactions

[AMBER warning: phase 1 must not pretend to solve semantic edit safety yet]
[⟁ tests: phase1_scope_is_textual_only]

## Why this cut is correct

It proves the most important thing first:

[𝔻 evidence: an LLM can edit in memory, inspect the candidate, and commit only after validation]

That is the trust foundation. Everything else stacks on top.
