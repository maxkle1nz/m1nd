# M1nd 10 G1 source/runtime binding refinement

Date: 2026-07-18  
Scope: non-ratified implementation proof note; this file does not amend the frozen PRD or UML.

## Decision

`OrganismManifestV1.source` identifies the M1nd product source tree. The selected hosted brain remains the authority for `repo_id`, `brain_id`, project root, graph snapshot, and architecture snapshot, but it is not allowed to impersonate the source tree that produced the running M1nd binary.

The runtime authority revision is the exact full Git commit captured when `m1nd-mcp` was built. Semantic version is descriptive only and cannot establish source/runtime coherence. A dirty build, an unknown build commit, or a build commit different from the currently observed M1nd product source commit is fail-closed as runtime `DRIFT` or `UNAVAILABLE`.

This refinement preserves the ratified `RuntimeFact` wire shape. It changes the value projected into its existing `revision` field, from semantic version to exact source commit. No new optional wire field is used to weaken old readers.

## Implemented boundary

- `m1nd-mcp/build.rs` captures full `M1ND_BUILD_SOURCE_COMMIT` and a separate `M1ND_BUILD_SOURCE_DIRTY` bit.
- `m1nd-mcp/src/session.rs` exposes those immutable build facts.
- `m1nd-mcp/src/organism_manifest.rs` observes M1nd product source separately from the hosted brain and compares exact source/build identities.
- `m1nd-control/src/manifest.rs` verifies runtime authority against the source authority's exact commit.
- Same-semantic-version stale binaries and dirty builds have explicit negative tests.

## Mechanical proof run

The following command completed successfully on 2026-07-18:

```text
cargo test --locked -p m1nd-mcp \
  --test manifest_occ \
  --test ui_bundle_attestation \
  --test remote_bind_refusal
```

Observed results:

- `manifest_occ`: 2/2 PASS
- `remote_bind_refusal`: 2/2 PASS
- `ui_bundle_attestation`: 4/4 PASS

The frozen ratified inputs remained byte-identical after the implementation refinement:

- `docs/M1ND-10-PRD.md`: `00658cd88ce9dc5866f9b1fc6b9fbe594923e32fb900bde5bbc7740894c25c38`
- `docs/M1ND-10-UML.md`: `8a8a5fe9b9d2a4fc62c419e160e8dc2dcb4115f58d98f3f15a2d5031881dd32b`

## Proof boundary

The current worktree is intentionally dirty while M1nd 10 is being implemented, so a binary built from it must report runtime/source `DRIFT`. That is the expected safe result, not a G1 failure. A release-coherent `FRESH` result requires a clean, content-addressed candidate built from the exact tested commit; that belongs to the later release gates and is not claimed here.

The command above proves contract behavior and negative fixtures. It does not prove the final installed live binary, a clean release candidate, production signing keys, remote transport, or FULL_AUTONOMY.
