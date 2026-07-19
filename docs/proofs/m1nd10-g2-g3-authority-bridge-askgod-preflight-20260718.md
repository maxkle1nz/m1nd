# M1nd 10 G2→G3 authority bridge — askGOD preflight

Date: 2026-07-18  
Mode: `verdict`, march `full`, Fable (`claude-fable-5`)  
Workspace judged: isolated snapshot of `/Users/kle1nz/m1nd` without build caches  
Mutation check: second dispatch pre/post `git status --porcelain` SHA-256 was identical: `f793becc4b145d006bed330bff0ac8faca9428eaca7c3f96601b51b621b05056`.

The first dispatch was discarded because its pre-dispatch snapshot-status hash was
captured while the live snapshot copy was still converging. The exact second,
stable, read-only verdict follows.

```text
VERDICT: CHANGE
CONFIDENCE: média

EVIDENCE:
- m1nd-mcp/src/mission_service.rs:283-380 — `AuthenticatedAuthorityContextV1` today carries `AuthenticationDisposition::UpstreamAuthenticationTrustedNotReverified` and `validate_for` checks only schema, three non-empty ids, six `require_digest` shape checks, brain/object equality, and lifetime. Confirms the dossier's core gap: the context is trust-on-construction, no cryptographic verification. Proposal items 1/2/6 are correctly motivated.
- m1nd-mcp/src/mission_service.rs:~260-280 (`WalRecordAuthenticationV1`) — four pre-supplied opaque phase signatures (`prepare/provisional/commit/abort`), checked only for non-emptiness, selected by `signature_for(phase)` before sequence/previous-root are known. Confirms proposal item 7's target is real: these signatures cannot bind the actual WAL record body.
- m1nd-control/src/authority_wal.rs:212-215 — `AuthorityWalIntegrityDisposition::OpaqueSignaturePresentUnverified` is the only disposition; WAL records are structurally validated, never signature-verified on open/recovery. Confirms item 7.
- m1nd-mcp/src/mission_service_wire_tests.rs:29-55 — the only existing provider pattern is `overrides.lock().get(object_digest).cloned()`: a clone-only lookup with no one-shot consumption, replayable indefinitely. Confirms item 5's lease (`UNUSED → RESERVED → CONSUMED`) is necessary.
- m1nd-mcp/src/authority_runtime.rs:2799-2837 — `authorize_mutation` returns `UncoveredAuthorityFloor` for `ServiceIdentity` + positive input: runner/reviewer result actions (`execution_started`/`execution_terminal`, floor `ServiceIdentity` per m1nd-control/src/action_catalog.rs:866-879) have NO authorization path at all today. The dossier acknowledges this may need new signed service-identity envelopes.
- m1nd-mcp/src/authority_runtime.rs:1652-1672 — `AuthorityAuthorizationReceiptCoreV1` binds action/ingress/effects/authority digest/epoch but NOT subject, role, capability id, object digest, mission/head, or session. Confirms the "sparse receipt" claim; the broker's persisted receipt must be materially extended (item 3), not merely wrapped.
- m1nd-mcp/src/http_server.rs:342-343, 2447-2464 — production `mission_service: None` fail-closed (`mission_service_unavailable`, 503), and the REST context is built with `transport_session_id: None, caller_root: None`. Confirms both the fail-closed posture and that the header/session plumbing does not yet exist.
- m1nd-mcp/src/mission_service_transport.rs:405-423 (`dispatch_external_mission_request`) — `LandIntent` executes `canonical_land_intent` with NO `require_authority` call and `verified_object_digest() == None`. Yet m1nd-control/src/action_catalog.rs:846-852 declares `mission.service.land_intent` at floor `Ordinary` ([Read] effect), and PRD §6.4 requires READ effects to satisfy scope/privacy/freshness policy under an authenticated session. The proposal as written binds authority only where an object digest exists — this read path escapes the broker entirely.
- docs/M1ND-10-PRD.md:566 (§6.5) — "AuthorityJournal e AuthorityWAL são duas projeções do mesmo log transacional, não arquivos com prepares independentes." The proposal introduces a broker authorization journal (item 1) AND a fsynced lease journal (item 5) alongside the AuthorityWAL without stating how the three linearize into one transactional decision.
- docs/M1ND-10-PRD.md:886-909 (G2/G3 gate lists) — the proposal's scope (identity, policy coverage, positive/safety separation, one-shot nonces, fail-closed epoch, no raw letter writes) matches the ratified gates. No contradiction found.
- AGENTS.md:14-32, 71-73 — CI gates are workspace-wide across ubuntu/macos/windows, and the documentation gate ("a feature is not done until the docs reflect it") requires docs/PATHOS.md/UML/PRD updates in the same PR. The dossier's VALIDATION plan tests only `-p m1nd-control` and `-p m1nd-mcp` and omits the docs gate entirely.
- m1nd-control/src/autonomy.rs:2599-2650, 2936-3010 — `AuthorityDecisionV1` union and `AutonomyCapabilityV1::validate` structural bindings exist as claimed; the broker can reuse them rather than reinventing decision validation.

RATIONALE: The proposal is architecturally correct and unusually well-grounded: every gap it names is real in the sources I read (trust-on-construction context, clone-only replayable provider, unverified opaque WAL signatures, uncovered ServiceIdentity floor, sparse receipt, absent production wiring), and its remedies (owner-side broker, distinct authorize ingress, transactional lease, real WAL signer, fail-closed config loader, explicit test-assurance marking) follow the ratified PRD G2/G3 contracts, including the PREPARED-current vs COMMITTED-historical recovery split and the no-silent-test-signer law. However, it is not yet approvable as the complete bridge: it leaves the `LandIntent` read path outside any authorization despite its `Ordinary` catalog floor; it multiplies durable journals without stating the PRD §6.5/6.7 single-transactional-log linearization between broker journal, lease journal, and AuthorityWAL; it does not explicitly require freeze/RED-latch/epoch revalidation at lease-consumption time (only at authorize time and crash reconciliation); and its validation plan violates the repo's own laws (3-OS workspace gates, mandatory docs coupling). These are specific, fixable design completions, not a rejection of the approach.

REQUIRED_CHANGES:
1. Close the `LandIntent` gap: require an enrolled-session (`Ordinary` floor) authority for `mission.service.land_intent`, or define an explicit canonical read-object digest for it so the broker path covers it; today `dispatch_external_mission_request` executes it with no `require_authority` while the catalog declares floor `Ordinary`.
2. State the linearization law for the three durable structures: broker authorization journal, capability lease journal, and AuthorityWAL must be projections of one transactional decision per sovereign operation, naming the single commit point and the exact ordering that recovery replays.
3. Require consumption-time revalidation: between `RESERVED` and `CONSUMED`, a lease finalize must re-check `issuance_frozen`, pending RED latch, authority/autonomy/protected epoch, and expiry against current owner state, not only during crash reconciliation.
4. Extend the validation plan to `cargo test --workspace` and explicit 3-OS portability proof for the new fsync/rename journal, lease store, and `OwnerSecurityConfigV1` symlink/rollback detection.
5. Add the documentation gate to the plan: PATHOS, architecture/UML and wiki must document the new public contracts while the ratified PRD/UML remain frozen.
6. Specify the extended receipt schema with subject/role, capability id/kind, verified object digest, mission/head, session and ingress-context bindings before the broker can mint `AuthenticatedAuthorityContextV1`.

RISKS_MISSED:
- Lock ordering/deadlock between the facade operation lock and new broker/lease locks.
- Multiple full open/recovery cycles per mutation and how the seam survives G4 actorization.
- Explicit HTTP status mapping for new broker refusals.
- Distinct MCP-session and REST-bearer identity spaces.
- GC/retention for expired authorizations and idempotency references.
```

## Binding disposition

`CHANGE` is binding on implementation. All six required changes are part of the
implementation task. No authority bridge or G2/G3 gate may be marked complete
until a final askGOD review of the real diff returns `APPROVE`.
