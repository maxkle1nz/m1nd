---
Protocol: L1GHT/1.0
Node: UltraEditTests
State: draft
Color: green
Glyph: ⍂
Completeness: 31%
Proof: first behavioral test matrix documented
Depends on:
- UltraEdit
- UltraEditContracts
- UltraEditAdversary
Next:
- PreviewIsolationSuite
- AtomicCommitSuite
- SemanticResolutionSuite
---

# Ultra Edit — Tests

## Test Domains

[⍂ entity: PreviewIsolationSuite]
[⍂ entity: AtomicCommitSuite]
[⍂ entity: ValidationSuite]
[⍂ entity: SemanticResolutionSuite]
[⍂ entity: MultiFileCoherenceSuite]
[⍂ entity: RollbackSuite]

## Must-Pass Behaviors

[⟁ tests: preview_does_not_write_any_file]
[⟁ tests: preview_returns_candidate_snapshot]
[⟁ tests: preview_reports_syntax_errors]
[⟁ tests: preview_reports_graph_risk]
[⟁ tests: commit_aborts_when_source_changed]
[⟁ tests: commit_writes_all_files_atomically]
[⟁ tests: rollback_restores_pre_commit_state]
[⟁ tests: semantic_edit_rejects_ambiguous_target]
[⟁ tests: multiedit_rebases_offsets_correctly]
[⟁ tests: candidate_and_commit_diff_match]

## Edge Cases

- duplicate symbol names in same file
- duplicate symbol names across modules
- edit on file deleted after preview
- stale preview handle
- conflicting transactions
- syntax-valid but contract-invalid mutation
- one file in batch fails write permission
- preview built from truncated context

[RED blocker: no confidence if candidate and final diff are not provably identical]
[AMBER warning: semantic targeting needs deterministic fallback]

## Quality Bar

Ultra Edit is not done when it works on the happy path.
It is done when it fails safely and predictably.

[𝔻 confidence: high]
[𝔻 evidence: trust in AI editing depends more on failure behavior than success behavior]
