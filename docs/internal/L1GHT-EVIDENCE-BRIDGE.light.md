---
Protocol: L1GHT/1.0
Node:     L1GHT_EVIDENCE_BRIDGE
State:    shipped
Color:    green
Glyph:    bridge
Completeness: implemented
Proof:    m1nd-mcp/src/tools.rs
Depends on:
- M1ND_L1GHT_DOCUMENT_LANE
- M1ND_RECOVERY_OS
Next:
- L1GHT_SYMBOL_LEVEL_EVIDENCE
- L1GHT_AUTHORING_LOOP
---

# L1GHT Evidence Bridge

This document is itself L1GHT corpus: it is the durable, machine-legible record
of the work that turned the `𝔻` epistemic markers from advertised-but-ignored
into structured graph edges that anchor agent-authored knowledge to live code.
Ingest it with `adapter: "light"` after a `code` ingest of this repo, then run
`cross_verify` with `check: "evidence_freshness"` — every claim below whose
cited code changes will flag itself as stale.

## Epistemic Marker Parsing

The [⍂ entity: EpistemicMarkerParser] turns `[𝔻 confidence:]`, `[𝔻 ambiguity:]`,
and `[𝔻 evidence:]` into typed edges that qualify the preceding claim, not the
section. Confidence becomes the edge weight, so uncertain knowledge spreads less
activation through `seek` / `activate` / `impact`.

[⍐ state: shipped]
[𝔻 confidence: certain]
[𝔻 evidence: m1nd-ingest/src/l1ght_adapter.rs]
[⟁ tests: m1nd-ingest/src/lib.rs]

## Confidence Word Forms

The [⍂ entity: ConfidenceParser] accepts both numeric (`0.6`) and word forms
(`low`/`medium`/`high`/`certain`) because real authored corpus uses words. This
is why `[𝔻 confidence: certain]` above resolves to a 0.95-weight edge instead of
silently defaulting to neutral.

[𝔻 confidence: high]
[𝔻 evidence: m1nd-ingest/src/l1ght_adapter.rs]

## Evidence Resolution To Code

The [⍂ entity: EvidenceResolver] runs after a light ingest is merged with the
code graph and links each evidence marker to the real `file::<path>` code node
via a `grounded_in` edge. It is the join that makes authored memory verifiable
against code truth instead of pointing at a dead leaf node.

[⍐ state: shipped]
[𝔻 confidence: certain]
[𝔻 evidence: m1nd-mcp/src/tools.rs]
[⟁ depends_on: EpistemicMarkerParser]

## Staleness Detection

The [⍂ entity: EvidenceFreshnessCheck] is a `cross_verify` check that re-hashes
every `grounded_in` target against the hash recorded at ingest and reports any
claim whose cited code drifted, with attribution: which marker, which claim,
which file, and why. This is the payoff — memory that knows when it is stale.

[⍐ state: shipped]
[𝔻 confidence: certain]
[𝔻 evidence: m1nd-mcp/src/audit_handlers.rs]
[⟁ depends_on: EvidenceResolver]
[AMBER warning: file-level granularity only; symbol-level evidence is Next]

## Known Boundary

Evidence currently resolves at file granularity. A marker like
`[𝔻 evidence: m1nd-mcp/src/tools.rs:719]` resolves to the file node, not the
function at line 719. Symbol-level pinning is the next slice.

[⍌ event: SymbolLevelEvidenceDeferred]
[𝔻 confidence: high]
[𝔻 ambiguity: best id scheme for line-to-symbol resolution undecided]
