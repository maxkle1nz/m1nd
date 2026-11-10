# 2.0.0 local release proposal

This unpublished proposal starts from the verified main source and carries thirteen Rust dependency updates. It proposes npm/core/ingest/mcp 2.0.0 and control 0.2.4. These identities are provisional; no tag, registry publication, installation or owner ratification follows.

The major version accounts for the already merged PR #580 compatibility change: mixed `partial` learning feedback is refused without mutation rather than attributed to a defect or observed change. Correct/wrong feedback remains relevance-only. Existing authority and calibration floors remain unchanged. This proposal does not introduce a new persisted format or claim compatibility of every historical store.

The lock updates seven earlier security fixes plus anyhow, Tokio, tokio-macros, Mio, chacha20 and wnaf. The prior exact third-party lock inventory had zero vulnerability/yanked findings against its pinned advisory database, with four maintenance advisories retained: bincode, number_prefix, paste and pin-utils. That inventory is not a fresh audit of this proposal or a general security guarantee.

Automatic graph freshness, view/capsule changes, Python call extraction, runtime-overlay optimization and inactive delegated-admission/journal studies are excluded. UI source, dependency inputs and tracked bundle remain unchanged.

## Qualification status

Local source and package validation are pending on this exact proposal. Prior component probes and broader development-candidate tests are supporting evidence only and do not transfer native runtime qualification. Actual old-state upgrade/rollback and broad migration remain unproven. No previously refused ingest is retried. Full applicable tests, cross-platform CI, immutable exact-main/tag identity, sealed UI/npm/Cargo artifacts, signatures, updater/apply/rollback verification and protected publication gates remain required before release. Public version absence is an observation, not a reservation.
