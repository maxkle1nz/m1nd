# M1ND-10 G2→G3 Authority Bridge — Proof Receipt

**Date:** 2026-07-18

**Checkout:** `/Users/kle1nz/m1nd`

**Scope:** G2 AuthorityRuntime → durable owner authorization lease → G3 MissionService → signed
AuthorityWAL `COMMIT`

**Overall gate:** **NOT_COMPLETE / HUMAN_GATED**

**Working-tree implementation:** **DONE; G2/G3 focused gates green**

**Focused macOS fixture proof:** **PROVEN**

**Real-machine protected authority and live acceptance:** **NOT_INSTALLED / NOT_PROVEN**

**Final askGOD diff verdict:** **NO_VALID_VERDICT / ROUTE_UNAVAILABLE**

**Published or deployed:** **NO**

This receipt deliberately separates a mechanically exercised software path from the authority that
does not exist on this machine yet. Nothing here authorizes an agent to synthesize a key, session,
receipt, lease, or human decision.

## 1. Decision history

The read-only askGOD preflight is recorded in
`docs/proofs/m1nd10-g2-g3-authority-bridge-askgod-preflight-20260718.md`. Its verdict was
**CHANGE**, not approval. The implementation applied its required changes:

1. `LandIntent` is an authorized canonical read, not an unauthenticated helper.
2. G2 issuance and G3 consumption share one process-local owner coordinator.
3. Consumption revalidates freeze/RED, active mode, policy, epochs, protected root, journal root,
   expiry, operation, session, and ingress context at finalization.
4. Owner security configuration has a separate protected root, anti-rollback chain, relative roots,
   and symlink refusal.
5. PATHOS, UML atlas, use cases, changelog, and this receipt state the working truth without editing
   the frozen M1ND-10 PRD/UML.
6. The authority receipt and MissionService context carry the complete decision and wire bindings.

The final askGOD review of the actual diff was dispatched through every available route, but no
route returned a valid `APPROVE | CHANGE | REJECT` contract. Fable refused for insufficient
credits. The Fugu full review and its one permitted retry were interrupted after exceeding the
review window without a verdict. The exact route receipts are preserved in
`docs/proofs/m1nd10-g2-g3-authority-bridge-askgod-final-20260718.md`. Therefore this gate remains
open and no approval is inferred from partial reviewer traces.

## 2. Implemented structure

### 2.1 Control contracts

- `m1nd-control/src/action_catalog.rs` — canonical action/effect tuples, including
  `authority.authorize`, `mission.service.land_intent`, and `mission.service.land`.
- `m1nd-control/src/crypto_authority.rs` — strict `AuthorityCapabilityV1`, public verification-key
  registry, Ed25519 verification, canonical signed-body digest, and replay context.
- `m1nd-control/src/authority_wal.rs` — Positive/Safety transaction contracts, exact transaction
  digest, WAL phase graph, record digest, and record-signature message.
- `m1nd-control/src/canonical.rs`, `identity.rs`, `policy.rs`, `autonomy.rs`, and `lib.rs` — the
  canonical identity, role, ingress, effect, mode, policy, and export vocabulary used end to end.

Load-bearing signature domains:

| Artifact | Domain |
|---|---|
| Capability body | `m1nd-authority-capability-signature-v1` |
| Authority transaction digest | `m1nd-authority-transaction-v1` |
| Complete sealed authority transaction, excluding signature | `m1nd-authority-transaction-signature-v1` |
| Authorization receipt, excluding signature | `m1nd-runtime-authorization-receipt-signature-v1` |
| Execution result, excluding signature | `m1nd-execution-result-signature-v1` |
| Review result, excluding signature | `m1nd-review-result-signature-v1` |
| WAL record signature | `m1nd-authority-wal-record-signature-v1` |

### 2.2 G2 runtime and owner ceremony

- `m1nd-mcp/src/authority_runtime.rs` — HUMAN_GATED/FROZEN bootstrap, authenticated session
  challenges, exact action-policy authorization, positive/service/safety variants, replay ledger,
  hash-chained journal, protected epoch CAS, prepared-transition recovery, and enriched receipts.
- `m1nd-mcp/src/authority_transport.rs` — strict wire DTOs, owner-injected context/time/keys,
  challenge/authenticate/authorize ceremony, one-shot lease issuance, and the shared authority
  coordinator.
- `m1nd-mcp/src/owner_authorization_broker.rs` — durable lease journal and state machine,
  reservation, finalization revalidation, WAL witness binding, conservative recovery, and GC.
- `m1nd-mcp/src/owner_security_config.rs` — public trust-anchor configuration, distinct protected
  config root, canonical epoch chain, safe root resolution, production assembly, and all-at-once
  installation into `AppState`.

Production assembly requires all of the following injections and refuses software-test assurance:

1. a hardware-protected OwnerSecurityConfig `(config_epoch, config_digest)` root;
2. a separate hardware-protected AuthorityRuntime epoch root;
3. reviewed public verification keys and exact policy registry in `OwnerSecurityConfigV1`;
4. an injected production AuthorityWAL signer/verifier;
5. a hardware-protected broker/WAL anti-rollback head;
6. an owner clock and explicit `BootstrapFrozen` or `OpenExisting` startup choice.

Private keys are not fields in OwnerSecurityConfig. No environment-key fallback is present.
All assurance checks run before runtime/broker/mission roots are created. The no-effects
preflight regression is mechanically green.

### 2.3 G3 consumption and commit

- `m1nd-mcp/src/mission_service_transport.rs` — the only external typed facade, canonical authority
  object digests, LandIntent read authority, lease reservation/consumption, and the WAL commit
  coordinator.
- `m1nd-mcp/src/mission_service.rs` — MissionService invariants, canonical LandIntent, landing
  transaction validation, provisional plan, MissionService state, and durable outcome.
- `m1nd-mcp/src/authority_wal.rs` — production crypto injection, exact record signing after
  sequence/root assignment, strict replay verification, torn-tail handling, and terminal replay.
- `m1nd-mcp/src/http_server.rs`, `mcp_http.rs`, `server.rs`, and `action_routes.rs` — REST and
  Streamable-HTTP MCP surfaces, status/refusal mapping, tool registration, and route parity.

Stdio has no owner-observed wire correlation/ingress bridge for this ceremony and therefore stays
fail-closed.

## 3. Wire contract

### 3.1 REST

| Endpoint | Strict schema | Owner-injected/observed facts |
|---|---|---|
| `POST /api/authority/session/challenge` | `m1nd-authority-session-challenge-request-v1` | REST transport session, caller root, brain, ingress digest, owner time, pinned keys |
| `POST /api/authority/session/authenticate` | `m1nd-authority-session-authenticate-request-v1` | same exact wire/root/brain context, pending challenge, owner time, pinned keys |
| `POST /api/authority/authorize` | `m1nd-authority-authorize-request-v1` | transport session, caller root, brain, ingress, policy, live runtime status, owner time |
| `POST /api/tools/mission_service` | `m1nd-mission-service-transport-request-v1` | same routing/context plus exact one-shot authority lease |

REST headers:

- `M1nd-Transport-Session-Id` — caller-supplied correlation label observed and bound by the owner;
  required for authority paths, but never proof of subject identity.
- `M1nd-Caller-Root` — optional at first use but sticky when present; participates in the ingress
  digest.
- `M1nd-Authority-Lease-Id` — required to consume an authorized MissionService operation.
- `?brain=<absolute-root>` — must resolve to the same owner brain used by the authority runtime.

Challenge request fields are strict and deny unknown fields:
`schema`, `request_id`, `subject_id`, `key_id`, `app_host_identity`, `nonce`, and
`requested_ttl_ms`. TTL is non-zero and at most 300,000 ms. Authenticate carries only
`schema`, `request_id`, `challenge_id`, and the signed `AuthorityCapabilityV1`.

### 3.2 Streamable-HTTP MCP

Equivalent tools are `authority_session_challenge`, `authority_session_authenticate`,
`authority_authorize`, and `mission_service`. `Mcp-Session-Id` is an owner-observed correlation
label; `M1nd-Authority-Lease-Id` carries the one-shot lease at consumption. Neither REST nor MCP
transport labels authenticate a subject. Only the signed G2 capability checked against the
owner-pinned public key does that. The transport and G2 authority sessions remain different spaces.

### 3.3 Refusal mapping

| HTTP | Meaning |
|---|---|
| `400` | malformed/unknown/contract-invalid request |
| `401` | required transport or G2 session is missing, unknown, or expired |
| `403` | brain, wire context, policy, receipt, operation, or crypto binding differs |
| `409` | challenge/lease replay or expiry, issuance frozen, stale state/head/version, or authority changed before finalization |
| `410` | legacy direct mission mutation path refused |
| `503` | runtime, verifier, broker, or MissionService unavailable/poisoned |
| `500` | durable authority/MissionService I/O or corruption invariant |

MCP returns the equivalent structured refusal schema and code without weakening the decision.

## 4. Distinct identities and bindings

The following identifiers are not interchangeable:

| Identity | Minted by | Purpose |
|---|---|---|
| REST `M1nd-Transport-Session-Id` / MCP `Mcp-Session-Id` | caller/transport, then owner-observed | correlation continuity and ingress-context binding; never subject authentication |
| `challenge_id` | owner | one pending handshake challenge bound to wire/subject/key/host/time |
| G2 `authority_session_id` | AuthorityRuntime after verified handshake | Ordinary/Positive caller authentication; process-memory only |
| `request_id` | caller | strict response correlation; never an authority fact |
| `authorization_lease_id` | owner | one-shot durable right to attempt one exact operation |
| `reservation_id` | broker | exact in-flight lease consumption |
| `transaction_id` | signed authority transaction | WAL transaction identity |

An `AuthorityAuthorizationReceiptV1` canonically binds and signs at least:

- organism, brain, subject, role, capability id/kind, and authority variant/assurance;
- exact verified object digest and optional mission/head;
- transport session and ingress-context digest;
- action, ingress, and the complete effect set;
- active mode, constitution digest/epoch, autonomy epoch, protected epoch, policy registry, and
  exact reachable policy tuple;
- authority-decision/body digest, replay sequence, journal sequence/root, decision time, and expiry.

The receipt self-digest covers its core. A separate domain-separated signature covers the complete
sealed receipt plus signer metadata while excluding only the signature field, avoiding a circular
signed subset. G3 verifies both the core digest and signature before reserving a lease.

The broker then binds that receipt to the exact operation object, wire session, ingress context,
reservation window, finalization snapshot, and (for landing) committed WAL witness. MissionService
receives the receipt digest as its `authorization_snapshot_digest`; it does not re-create G2
authentication from request-body data.

## 5. Read-before-write landing protocol

1. Authenticate a G2 session with a real signed `runtime.session.handshake` capability.
2. Compute the canonical `LandIntent` read-authority object digest.
3. Authorize Ordinary `mission.service.land_intent` with exact effects `[READ]`.
4. Consume that lease through MissionService and obtain the canonical `LandIntent` core/digest.
5. Sign a Positive capability for `mission.service.land`, binding that intent, mission/head,
   payload, decision, and full effects
   `[MISSION_STATE_WRITE, RUNTIME_STORE_WRITE, COORDINATION_RECORD, SOVEREIGN_MUTATION]`.
6. Authorize it to obtain a second one-shot lease and exact authorization receipt.
7. Bind `PositiveAuthorityTransactionV1.authorization_snapshot_digest` to
   `authorization_receipt.receipt_digest`, seal it, sign the complete sealed transaction excluding
   only the signature, and submit the typed Land request.
8. MissionService verifies the outer transaction against the active owner-pinned public key,
   validates every binding, builds the provisional plan, and asks the broker/WAL coordinator to
   finalize.
9. Signed AuthorityWAL `COMMIT` + fsync is the sovereign commit point. Only its exact witness lets
   the broker append/fsync `CONSUMED`; the opaque production witness also carries the exact reserved
   transaction id and cannot be constructed from caller JSON.

Cross-source vectors pinned for h4nd:

| Object | SHA-256 canonical digest |
|---|---|
| LandIntent read-authority object | `e9d3f0d445d682cb05353e75d9ee013d936a7e458cdf91dbd36a879dde248a54` |
| LandIntent core | `70586f404444c71b76f2ab3815c2623381310b99a0dcd2a3d1eed7d35a0f6818` |

These vectors let h4nd detect cross-language canonicalization drift; they do not give h4nd signing
authority.

## 6. Linearization, recovery, and retention

Production lock/durability order for sovereign Land:

1. MissionService facade operation mutex;
2. shared issuance/consumption `broker_operation` mutex;
3. broker durable writer/journal ownership;
4. current AuthorityRuntime status read;
5. named `OWNER_AUTHORITY_TRANSACTION_V1` linearization mutex;
6. revalidate lease plus freeze/RED/mode/policy/epochs/roots/expiry;
7. append/fsync broker `FINALIZATION_PREPARED`;
8. append/fsync signed AuthorityWAL `COMMIT` — the commit point;
9. validate exact witness and append/fsync broker `CONSUMED`.

The durable broker state is `UNUSED → RESERVED → FINALIZATION_PREPARED → CONSUMED|ABORTED`.
`FINALIZATION_PREPARED` is represented as a journal event plus finalization snapshot while the
lease remains reserved; it is not an externally spendable state.

Recovery rules:

- an exact committed WAL witness advances prepared/reserved authority to `CONSUMED`;
- no witness plus an expired reservation advances to `ABORTED`;
- no witness plus an unexpired reservation remains reserved;
- an error returned after `FINALIZATION_PREPARED` is an uncertain WAL outcome and stays prepared;
  it never writes an immediate ABORT that could contradict a COMMIT recovered after restart;
- a witness with wrong transaction, snapshot, phase, record digest, or time is corruption/refusal;
- a partial final WAL record may be truncated only as the exact torn tail; internal or
  newline-terminated corruption refuses open;
- runtime prepared descriptors recover exactly old-or-new; unbound valid tails are never inferred
  as success.

The broker journal and AuthorityWAL each publish their exact `(domain, sequence, head_digest)` to a
separate domain slot in the injected protected compare-and-advance backend after journal fsync. Replacing either journal with an
older internally valid prefix is refused on open. A journal fsync followed by failed protected-head
CAS poisons the writer and is availability-fatal; success is never inferred.

GC requires both `now >= retain_until` and an external predicate proving no checkpoint, mission,
release, WAL terminal outcome, or idempotency record references the terminal lease. GC appends a
durable tombstone; it does not silently forget a live or referenced authorization.

## 7. Mechanical evidence — 2026-07-18 current run

Every Rust command used the external target
`CARGO_TARGET_DIR=/Volumes/Cofre/.codex-m1nd-build-20260718` with `CARGO_INCREMENTAL=0`.
Tests used disposable roots and deterministic software fixtures. No live owner, hardware key, or
production store was touched.

### 7.1 Focused G2/G3 gates — PASS

| Command/filter | Result |
|---|---|
| `cargo check -p m1nd-mcp --features serve --lib` | **PASS** |
| `cargo test -p m1nd-mcp --features serve --lib --no-run` | **PASS** |
| `cargo clippy -p m1nd-mcp --features serve --lib -- -D warnings` | **PASS** |
| `cargo test -p m1nd-control` | **147/147 PASS**: 132 unit + 12 Ed25519 integration + 3 P-256 integration |
| `cargo test -p m1nd-mcp --features serve --lib authority_runtime::tests` | **24/24 PASS** |
| `cargo test -p m1nd-mcp --features serve --lib owner_security_config::tests` | **6/6 PASS** |
| `cargo test -p m1nd-mcp --features serve --lib owner_authorization_broker::tests` | **6/6 PASS** |
| `cargo test -p m1nd-mcp --features serve --lib authority_wal::tests` | **10/10 PASS** |
| `cargo test -p m1nd-mcp --features serve --lib authority_transport::tests` | **1/1 PASS** |
| `cargo test -p m1nd-mcp --features serve --lib signed_artifact_tests` | **1/1 PASS** |
| `cargo test -p m1nd-mcp --features serve --lib authority_tool_schemas_close_capability_and_authority_unions` | **1/1 PASS** |
| `cargo test -p m1nd-mcp --features serve --lib authority_http_statuses_preserve_auth_overload_and_integrity_classes` | **1/1 PASS** |
| `cargo test -p m1nd-mcp --features serve --lib mission_http_statuses_preserve_signed_artifact_and_wal_classes` | **1/1 PASS** |
| `cargo test -p m1nd-mcp --features serve --lib required_authority_background_boot_refuses_before_endpoint_publication` | **1/1 PASS** |

These batteries include real deterministic Ed25519 verification for the session capability, outer
authority transaction, authorization receipt, ExecutionResult, ReviewResult, and WAL records;
wrong signature, body tamper, revoked key, rotated key, wrong subject/key provenance, replay after
restart, mandatory reauthentication after restart, owner-pinned role mismatch, one-shot lease,
exact transaction-id COMMIT witness, symlink/root overlap, and valid-prefix rollback refusal for
both broker and WAL protected heads.

### 7.2 Full `m1nd-mcp` library gate — superseding PASS

The first `cargo test -p m1nd-mcp --features serve --lib` receipt recorded
**1036 passed, 3 failed, 1 ignored**. The G2/G3 tests in that run passed. The three failures were
kept explicit rather than waived:

| Failure | Owner | Current classification |
|---|---|---|
| `audit_handlers::tests::collect_git_state_reports_clean_repo` | audit_handlers lane | **FAIL in historical receipt**: expected clean=true, observed false |
| `audit_handlers::tests::audit_auto_detects_coordination_profile_for_doc_heavy_repo` | audit_handlers lane | **FAIL in historical receipt**: expected `coordination`, observed `quick` |
| `project_brains::eviction_gate_tests::eviction_persists_unpersisted_state` | G4/project-brains lane | **FAIL in historical receipt**: co-change matrix/graph-size mismatch during eviction checkpoint |

Those failures were repaired in their owning lanes. The final cumulative rerun over the same working
tree recorded **1049 passed, 0 failed, 1 ignored**. `cargo clippy -p m1nd-mcp --features serve
--all-targets -- -D warnings` also passed. The historical failure receipt above remains evidence of
the progression; the current repository-wide m1nd-mcp library gate is **PASS**.

### 7.3 Format, diagrams, and cross-platform boundary

- `cargo fmt --all --check` and `git diff --check` both passed on the final pre-askGOD tree.
- Workspace-wide frozen-file hashes are recorded in section 9.
- The two UML supplement diagrams are source-reviewed but a fresh `mermaid.parse()` whole-atlas
  count is **NOT_RUN**; they remain outside the older 78-block historical parser claim.
- macOS fixture behavior is focused-proven. Linux and Windows G2/G3 builds and runtime tests are
  **NOT_RUN** in this run. The 3OS bar remains **NOT_PROVEN**.

## 8. Explicit nonclaims and blockers

1. **Production adapters are NOT_INSTALLED.** There is no concrete hardware-protected config-root,
   runtime-epoch, broker/WAL-head, attestation, platform-signing, or production WAL-crypto adapter
   in this checkout. Software fixtures never satisfy the production assembly.
2. **No live authority material.** No owner private key, protected root, authenticated production
   session, production receipt, or live one-shot lease was generated.
3. **No live deployment.** The served binary was not replaced or restarted by this slice; no live
   production data was mutated.
4. **Boot integration is fail-closed, not a hardware claim.** The production assembly and atomic
   AppState install helper accept only a fully preflighted injected assembly. `Required` plus no
   production assembly returns `NOT_INSTALLED` and must refuse before bind. Default deployments
   without adapters keep authority/MissionService unavailable instead of using test crypto.
5. **Sessions are process-memory state.** Restart deliberately invalidates every G2 authority
   session and requires a new owner challenge/authenticate ceremony. The durable replay ledger still
   rejects an already-consumed signed capability/nonce after restart.
6. **Transport ids do not authenticate.** REST `m1nd-transport-session-id` and MCP
   `Mcp-Session-Id` are correlation labels. They bind context but cannot replace the signed
   capability or owner-pinned public key.
7. **Signed evidence is fixture-proven, not hardware-proven.** Receipt, outer transaction,
   ExecutionResult, and ReviewResult signature checks are mechanically proven with deterministic
   real crypto; protected key custody and attestation are not.
8. **Recovery orchestration is local.** Broker/WAL reconciliation rules are proven; automatic
   fleet-wide invocation across every crash topology is not claimed.
9. **No FULL_AUTONOMY claim.** This remains a HUMAN_GATED authority substrate. Policy ratification,
   grant promotion, protected hardware ceremony, safety governance, audit sampling, 3OS release,
   and live acceptance remain gates.
10. **Release gates remain open.** Final askGOD approval is absent because all available review
    routes ended without a valid verdict. Cross-platform CI, live hardware ceremony, operator
    acceptance, publication, and deployment must remain separately visible even though the current
    macOS library/clippy gates are green.

h4nd remains a client of the owner ceremony. It may transport a public challenge and an externally
produced signature, but it never generates, stores, or composes private signing material; session
and one-shot lease ids are owner-minted.

## 9. Documentation and frozen-file receipt

Updated by this slice:

- `docs/PATHOS.md`
- `docs/UML-ORGANISM.md`
- `docs/use-cases.md`
- `docs/wiki/src/changelog.md`
- this proof receipt

Frozen design files were not edited by this lane. Immediately before final askGOD dispatch,
`git diff --check` passed and SHA-256 verification returned the expected values:

```text
00658cd88ce9dc5866f9b1fc6b9fbe594923e32fb900bde5bbc7740894c25c38  docs/M1ND-10-PRD.md
8a8a5fe9b9d2a4fc62c419e160e8dc2dcb4115f58d98f3f15a2d5031881dd32b  docs/M1ND-10-UML.md
```

## 10. Gate verdict

| Requirement | Verdict |
|---|---|
| Strict G2 owner session ceremony, role pinning, key provenance, restart reauth | **PROVEN in disposable macOS fixtures** |
| Durable nonce/capability replay refusal after restart | **PROVEN** |
| Authorized LandIntent and exact Positive Land binding | **PROVEN in disposable macOS fixtures** |
| Cryptographic outer transaction and authorization receipt verification | **PROVEN with real deterministic fixture crypto** |
| Signed ExecutionResult and ReviewResult verification | **PROVEN with real deterministic fixture crypto** |
| Durable one-shot lease and exact opaque WAL COMMIT witness | **PROVEN in disposable macOS fixtures** |
| Protected broker/WAL valid-prefix rollback refusal | **PROVEN with software anti-rollback fixtures** |
| Production assembly rejects software assurance before durable effects | **PROVEN** |
| Real hardware/key/attestation adapters | **NOT_INSTALLED** |
| Live owner/h4nd ceremony | **NOT_RUN** |
| Linux + Windows + macOS 3OS | **NOT_PROVEN** |
| Full m1nd-mcp library suite | **PASS: 1049 pass / 0 fail / 1 ignored** |
| Final askGOD diff approval | **NO_VALID_VERDICT / ROUTE_UNAVAILABLE** |
| Publish/deploy | **NOT_RUN** |

**Final status: working-tree G2→G3 implementation DONE and focused-fixture PROVEN; overall release
gate remains NOT_COMPLETE, HUMAN_GATED, fail-closed, and production adapters remain NOT_INSTALLED.**
