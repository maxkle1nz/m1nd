# newm1nd integration preview

This unpublished source integration combines local batch `097c073c` with `f0eafddc`, preserving both histories. The former already contains PR #592 at `e28999ee`; it is not imported twice. The destination is the development branch `newm1nd`. This is not a release candidate, main promotion, installed runtime or authority activation.

## Included source batches

The `097c073c` batch includes explicit agent state storage and continuation, root/branch/HEAD guards, bounded anchor and returned-context freshness checks, code/view provenance, read-only memory-evidence review, relation-edit corrections, conservative same-file Python call extraction and fixture registry isolation. Its own recorded local proof is 2,844 Rust tests and 108 npm tests, with independent reviews. These results belong to that exact source, not to this combined candidate. The freshness checks are bounded and non-atomic; memory claims are not certified or automatically copied between branches; Python extraction is not a complete runtime call graph. See [the freshness contract](AUTOMATIC-GRAPH-FRESHNESS.md).

Its inherited PR #592 batch supplies the earlier seven Rust dependency patches, runtime-overlay batch matching, owned-memory refresh preparation and Vite 8.1.6/Zod 4.6.7 updates. No authority floor or persisted-format migration is introduced by this integration. Inactive delegated-admission/journal studies, authority amendments and untracked local proposals are excluded.

## Versions and dependency scope

The separate `5236ba72` → `f0eafddc` lineage proposed npm/core/ingest/mcp 2.0.0 and control 0.2.4. Those source versions and internal requirements are retained; runnerd keeps its independent version. The strict major proposal accounts for main's existing PR #580 partial-feedback no-mutation refusal. Versions remain provisional and unpublished.

The combined Rust lock retains thirteen updates: the seven inherited security patches plus anyhow, Tokio, tokio-macros, Mio, chacha20 and wnaf. Its third-party records are the previously reviewed lock inventory, including six documented Windows dependency-edge reassignments. The pinned historical audit reported zero vulnerability/yanked findings and four retained maintenance advisories: bincode, number_prefix, paste and pin-utils. This is not a fresh audit or a general security guarantee.

## Historical tracked UI alignment

`f0eafddc` aligned the tracked 24-file UI bundle with actual rebuilt output from `5236ba72`; those bytes matched the verified published 1.6.5 crate UI. The historical tracked CSS had four additional utility rules whose original build cause and runtime reachability remain unknown. No harmlessness or browser/accessibility claim follows.

The 658 UI unit tests, lint and two actual builds remain evidence for their original inputs. This integration retains the later Vite/Zod inputs from #592, so the old UI build, native build and private executable placement cannot qualify the combined source. Its tracked bundle is retained as an explicit input awaiting fresh combined-candidate UI/no-drift and native qualification. The prior visual lane's approval-review rejection remains terminal and is not retried here.

## Qualification and recovery status

This integration is staged source only until reviewed and committed. Existing component receipts preserve their source identities and are not combined into a new CI pass. Required checks must bind the actual new candidate: applicable Rust/npm/UI/security gates, hosted platform coverage and package/native identities remain separate.

Actual old-state upgrade/rollback and broad brain migration remain unproven because a legitimate admitted old fixture is missing; no previously refused ingest is retried or authority fabricated. Private wrapper/executable placement and fixture updater tests do not prove genuine old-brain recovery. Signed candidate/updater/apply/rollback, fresh exact-main/tag identity, signatures and protected publication gates remain requirements for any future release. No push to main, force update, tag or release is part of this source preview. Development-branch publication does not establish a release or activation.
