# M1ND-10 G5 — Evidence Correlation Spine Integration Proof

Date: 2026-07-18  
Platform: Darwin 24.6.0 arm64  
Toolchain: rustc 1.95.0, cargo 1.95.0

## Scope

This receipt covers the durable G5 projection, owner adapters, canonical-source
auto-population, coordination joins, read-only query surface, action routing,
and real REST/Streamable-HTTP MCP proof implemented in:

- `m1nd-control/src/action_catalog.rs`
- `m1nd-mcp/src/action_routes.rs`
- `m1nd-mcp/src/delegation_handlers.rs`
- `m1nd-mcp/src/evidence_spine.rs`
- `m1nd-mcp/src/evidence_spine_owner.rs`
- `m1nd-mcp/src/evidence_spine_wire_tests.rs`
- `m1nd-mcp/src/http_server.rs`
- `m1nd-mcp/src/lib.rs`
- `m1nd-mcp/src/mission_handlers.rs`
- `m1nd-mcp/src/mission_service.rs`
- `m1nd-mcp/src/mission_service_transport.rs`
- `m1nd-mcp/src/protocol/layers.rs`
- `m1nd-mcp/src/server.rs`
- `m1nd-mcp/tests/evidence_spine.rs`

The frozen PRD and UML were not edited. Their SHA-256 values remained:

- `docs/M1ND-10-PRD.md`: `00658cd88ce9dc5866f9b1fc6b9fbe594923e32fb900bde5bbc7740894c25c38`
- `docs/M1ND-10-UML.md`: `8a8a5fe9b9d2a4fc62c419e160e8dc2dcb4115f58d98f3f15a2d5031881dd32b`

## DONE

- The spine remains a projection, never a new authority. ReceiptV1,
  MissionService letters, delegation records, and Mission Control each remain
  canonical in their own stores.
- Every row is durably sequence/hash chained and bound to the persisted
  `organism_id + brain_id + canonical workspace_root` identity plus
  `mission_id + iteration_id` correlation identity.
- Canonical G3 `MissionLetterV1` and `ReceiptV1` state is synchronized
  idempotently after MissionService recovery and after an accepted transport
  result. Restart replays existing rows and appends only missing authority
  events.
- The facade emits an owner-derived `EvidenceCorrelationLinkV1`; clients cannot
  submit a receipt, letter, or raw evidence event through this seam.
- Delegate, debrief, and Mission Control consume only that narrow link. Before
  projecting, the owner reopens the selected spine and requires an exact
  existing G3 Receipt/MissionLetter head and optional transaction anchor.
- Missing links do not fabricate joins: the original coordination record stays
  canonical and carries an explicit `canonical_evidence_link_absent` gap.
  Post-authority projection failures likewise return a gap without rolling back
  or falsifying the already-committed domain result.
- Debrief outcomes have their own source event and cannot collapse into the
  delegation packet's idempotency key.
- `evidence_query` is an explicit `evidence.query` action with exactly the
  `Read` effect and both REST and MCP ingresses. It is also excluded from daemon
  and auto-ingest traffic ticks.
- Read-only query opens no writer, creates no identity/directory/lock, repairs
  no tail, and writes no cache. A torn uncommitted suffix is reported and
  excluded from the verified prefix without truncation.
- REST and MCP presence tracking is bypassed before strict payload decoding for
  `evidence_query`, `m1nd.evidence_query`, and `m1nd_evidence_query`. Even a
  refused payload carrying `agent_id` leaves the session map, registry/presence
  tree, evidence identity, evidence log, and lock inventory unchanged.
- The query payload has no brain/organism/workspace selector. The served owner
  chooses the brain; persisted identity must match its canonical workspace.
  A client `brain_id` field is rejected, and a cross-workspace owner selection
  is refused.
- Mission-head filtering checks every canonical event in a correlation, so
  non-landed heads and landed heads resolve while an unrelated head returns no
  correlation.
- The generic tool schema/dispatcher used by both production AppState
  construction paths exposes the same query. The real-wire test drives the
  production router shape and Streamable-HTTP MCP handler.
- Legacy direct mutation names remain tombstoned before body parsing or
  authority lookup: REST returns 410, MCP returns `isError`, and stdio cannot
  reach the G3 mutation facade.

## PROVEN

All commands used `CARGO_INCREMENTAL=0`. The focused result is **49 passed,
0 failed** across the batteries below.

| Gate | Command | Result |
|---|---|---:|
| G5 spine + owner integration | `RUSTFLAGS='-D warnings' cargo test --locked -p m1nd-mcp --test evidence_spine` | 11 passed |
| G5 real REST/MCP wire | `RUSTFLAGS='-D warnings' cargo test --locked -p m1nd-mcp --lib evidence_spine_wire_tests` | 2 passed |
| Action catalog/route/schema parity | `RUSTFLAGS='-D warnings' cargo test --locked -p m1nd-mcp --lib action_routes` | 6 passed |
| Delegation regression | `RUSTFLAGS='-D warnings' cargo test --locked -p m1nd-mcp --lib delegation_handlers` | 3 passed |
| Mission Control regression | `RUSTFLAGS='-D warnings' cargo test --locked -p m1nd-mcp --lib mission_handlers` | 19 passed |
| MissionService transport regression | `RUSTFLAGS='-D warnings' cargo test --locked -p m1nd-mcp --lib mission_service_transport` | 5 passed |
| Existing G3 REST/MCP wire regression | `RUSTFLAGS='-D warnings' cargo test --locked -p m1nd-mcp --lib mission_service_wire_tests` | 3 passed |

The owner-integrated spine battery first passed **10/10**; the explicit
non-landed/landed/foreign-head regression raised the final battery to **11/11**.
It proves complete joins, restart reconstruction, exact
replay, landed mismatch refusal, full-row corruption refusal, writer recovery
of a torn tail, genuinely non-mutating read-only observation of a torn tail,
workspace/identity isolation, owner-link anchor enforcement, coordination
projection, and non-landed/landed/foreign head filtering.

The 2-test G5 wire battery proves:

1. a canonical G3 transition auto-populates G5 and returns an owner-derived link;
2. REST and Streamable MCP query the same committed chain/read model;
3. query calls leave `identity.json`, `correlations.jsonl`, lock inventory,
   session presence, and the durable registry tree byte-for-byte unchanged,
   including refused `agent_id` payloads on every tool alias;
4. facade restart performs an idempotent replay and preserves the exact chain head/read model;
5. forged client brain selection is REST 400 / MCP `isError`;
6. cross-workspace selection is REST 400 without evidence-store mutation;
7. raw mission/receipt/landed tombstones remain REST 410 / MCP `isError`;
8. catalog effects are exactly `[Read]`, and stdio cannot bypass the G3 wire.

Additional mechanical gates:

```text
CARGO_INCREMENTAL=0 cargo check --locked -p m1nd-mcp --lib
```

Result: **PASS**.

```text
CARGO_INCREMENTAL=0 cargo clippy --locked -p m1nd-mcp --lib -- -D warnings
CARGO_INCREMENTAL=0 cargo clippy --locked -p m1nd-mcp --test evidence_spine -- -D warnings
```

Result after the concurrent G2 integration stabilized: **PASS / PASS**. An
intermediate lib-clippy run exposed a G2 `too_many_arguments` helper; that lane
removed it, and the final rerun above completed green.

```text
rustfmt --edition 2021 --check \
  m1nd-mcp/src/evidence_spine.rs \
  m1nd-mcp/src/evidence_spine_owner.rs \
  m1nd-mcp/src/evidence_spine_wire_tests.rs \
  m1nd-mcp/tests/evidence_spine.rs
```

Result: **PASS**. Focused `git diff --check` over every G5-touched code/test file
also passed.

## SHA-256

Hashes below are filled from the final working-tree bytes after the gates:

- `m1nd-control/src/action_catalog.rs`: `69fdcbb2cb7ab3a4e718d8a363f0e2672d49c74f5be18aac8aac2b21b852e91c`
- `m1nd-mcp/src/action_routes.rs`: `563b70362ad6ba0738de12e6e0a162125e2306080d539b8d8a96636806b1d9c2`
- `m1nd-mcp/src/delegation_handlers.rs`: `ebde592fd3b02b3a5a1bf23c9b40e290c5a0aab2e1055aed91c52abbc44f0822`
- `m1nd-mcp/src/evidence_spine.rs`: `a3bb5c0d1e7c41564cef47542c50b7fe4db8011de752f851add01d8dab7c46f6`
- `m1nd-mcp/src/evidence_spine_owner.rs`: `71a5770c80938e4a598bcf38e4d535246989eb20a982c70fed7b4dc79d414d4b`
- `m1nd-mcp/src/evidence_spine_wire_tests.rs`: `b5c1cb86b6cbfa5f270353902f9a0c4eddf1257d64481060d752db691b010f61`
- `m1nd-mcp/src/http_server.rs`: `b4b7b8c9a1aec284d13b01e772e2277a7179881bc87ab54d3a80bec30cb3006d`
- `m1nd-mcp/src/lib.rs`: `98082c72bd9db546a9160f936ba5a177e8fb4f777e9df744b7a577c6e5ebb19b`
- `m1nd-mcp/src/mission_handlers.rs`: `3b4fc2394149aec0ff0d08407fdc5e0dbc232a7dabad7df7a863d7533c696981`
- `m1nd-mcp/src/mission_service.rs`: `f5f82e687d164f51a2ddc6980a6a9a1994b88779ab7b900c513775f20cba19f8`
- `m1nd-mcp/src/mission_service_transport.rs`: `cec397214171fe085b65b8f1772cf00d938a5144ff36ff46121ff5fd66be56e9`
- `m1nd-mcp/src/protocol/layers.rs`: `2617d43e5a1bc9c13c03d31bf34d4e69595a42be9a845343e8de8759c1ce8783`
- `m1nd-mcp/src/server.rs`: `10ee7502057211adb3d35d987682c6259cc16fbb6c7b5fa6a1f4ce70afcb13c3`
- `m1nd-mcp/tests/evidence_spine.rs`: `d913254d15e11af44de1253a144ceb84e9b41df15ca656b9d76640981b2497b1`

Shared-file hashes describe integrated working-tree bytes; they are not an
assertion that every line in those files belongs to G5.

## Operational incident during proof

The first lib-test link attempt stopped before test execution with `No space
left on device`. At that point `target/debug/incremental` occupied about 17 GiB
and the volume had 234 MiB free. Cargo/rustc were paused, and one coordinated
removal of only that reconstructible incremental cache restored headroom. Proof
resumed with `CARGO_INCREMENTAL=0`; no source, fixture, user artifact, or
non-reconstructible data was deleted. Final G5 runs remained above the 4 GiB
stop floor.

## NOT PROVEN

- **Golden mission landed with real production authority is still pending.**
  The passing land regression uses the explicitly named software-test,
  not-production AuthorityWAL signer. It proves transaction/replay mechanics,
  not real G2 cryptographic authority or a production landed mission.
- The production AppState constructors still install no sovereign G2 provider
  and therefore keep `mission_service: None` fail-closed. G5's optional facade
  constructor and real-wire transition are proven; production-live G2→G3→G5
  installation is not claimed.
- External evidence bytes are not re-fetched. G5 validates canonical references
  and digests; retention/freshness remains the source authority's concern.
- Production signing-key, Secure Enclave, and hardware-attestation authenticity
  are outside this projection.
- Windows directory-fsync semantics were not exercised; this battery ran on
  macOS arm64.
- Protected anti-rollback storage for same-UID whole-store replacement belongs
  to G2/G9 authority infrastructure.
- The repository-wide test suite was **NOT_RUN** by this lane; only the explicit
  focused gates above are claimed.
- No production-live claim is made.
