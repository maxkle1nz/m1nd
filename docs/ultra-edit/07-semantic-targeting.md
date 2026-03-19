---
Protocol: L1GHT/1.0
Node: UltraEditSemanticTargeting
State: draft
Color: yellow
Glyph: 𝔻
Completeness: 27%
Proof: semantic target model outlined before code
Depends on:
- UltraEditContracts
- UltraEditAdversary
- SurgicalContextV2
Next:
- SymbolResolver
- ASTLocator
- AmbiguityScore
---

# Ultra Edit — Semantic Targeting

## Purpose

Semantic targeting allows the agent to express intent in structural terms instead of brittle text snippets.

[⍂ entity: SymbolResolver]
[⍂ entity: ASTLocator]
[⍂ entity: AmbiguityScore]
[⍂ entity: DeterministicFallback]

## Supported Target Modes

- function by name
- method by receiver + name
- struct field insertion point
- enum/union branch insertion point
- insert_before_symbol
- insert_after_symbol
- replace_function_body
- replace_match_arm

[⟁ tests: semantic_target_function_by_name]
[⟁ tests: semantic_target_method_by_receiver]
[⟁ tests: semantic_insert_after_symbol]

## Hard Problems

### Ambiguity
Same symbol name can exist in multiple scopes.

[RED blocker: semantic targeting without explicit ambiguity scoring is unsafe]
[⟁ tests: ambiguity_score_exposed_to_agent]

### Parser Incompleteness
Some languages may not support high-fidelity AST targeting immediately.

[AMBER warning: feature parity across languages should not be assumed]
[⍂ entity: LanguageCapabilityMatrix]

### Fallback Behavior
If semantic targeting is not safe, the system must fall back explicitly, not silently.

[⟁ tests: deterministic_fallback_is_reported]
[⟁ binds_to: TextualPreviewMode]

## Principle

Semantic targeting is not about convenience. It is about reducing edit ambiguity while preserving explicitness.

[𝔻 confidence: medium]
[𝔻 evidence: safer than raw text when resolvers are honest about ambiguity]
