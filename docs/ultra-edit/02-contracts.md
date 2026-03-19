---
Protocol: L1GHT/1.0
Node: UltraEditContracts
State: draft
Color: yellow
Glyph: 𝔻
Completeness: 28%
Proof: first contract surface documented
Depends on:
- UltraEdit
- SurgicalContextV2
- ApplyBatch
Next:
- EditPreviewAPI
- EditCommitAPI
- SemanticEditAPI
---

# Ultra Edit — Contracts

## API Surface

[⍂ entity: EditPreviewAPI]
[⍂ entity: EditCommitAPI]
[⍂ entity: EditAbortAPI]
[⍂ entity: SemanticEditAPI]
[⍂ entity: MultiEditTransactionAPI]

## Core Contract

Ultra Edit is split into preview and commit phases.

[⍂ entity: PreviewHandle]
[⍂ entity: CandidateSnapshot]
[⍂ entity: ValidationReport]
[⍂ entity: CommitReceipt]

### Preview
- Receives edit intent.
- Produces an in-memory candidate.
- Returns diff, validation, risk, and metadata.
- Does not modify disk.

[⟁ binds_to: m1nd.edit_preview]
[⟁ tests: preview_returns_diff_and_validation]

### Commit
- Accepts an approved preview handle or snapshot.
- Checks source freshness.
- Writes atomically.
- Reingests graph once.

[⟁ binds_to: m1nd.edit_commit]
[⟁ tests: commit_requires_fresh_snapshot]

## Required Inputs

[⍂ entity: EditIntent]
[⍂ entity: EditTarget]
[⍂ entity: EditOperation]
[⍂ entity: ValidationMode]

Possible target modes:
- exact_text
- symbol
- ast_node
- insert_after_symbol
- insert_before_symbol
- replace_function_body
- transaction_batch

## Required Guarantees

[⍐ state: required]

- preview is side-effect free
- commit is atomic by default
- snapshot mismatch aborts commit
- validation report is machine-readable
- semantic target resolution returns ambiguity score
- batch edits preserve deterministic ordering

## Compatibility

Ultra Edit must complement, not break:

[⟁ depends_on: m1nd.apply]
[⟁ depends_on: m1nd.apply_batch]
[⟁ depends_on: m1nd.surgical_context_v2]
[⟁ binds_to: VerifyPipeline]

[AMBER warning: legacy apply remains necessary for full-file replacement workflows]
