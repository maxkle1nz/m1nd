# M1ND-10 G6 Knowledge Quality Proof Receipt

Date: 2026-07-18  
Scope: G6 only  
Frozen contracts changed: none (`docs/M1ND-10-PRD.md` and `docs/M1ND-10-UML.md` untouched)

## Outcome

| Slice | Implementation | Mechanical proof | Verdict |
|---|---|---|---|
| Universal ingest outcome honesty | Per-document `INGESTED/DEGRADED/UNSUPPORTED/FAILED/EMPTY`, provider `EXTRACTED/EMPTY/FAILED/*`, bounded diagnostics, noncommittable negative-only bundles | 20 ingest tests + 2 persistence/audit tests | PASS |
| Temporal/co-change restart state | Both runtime matrices, exact graph-ID binding, schema/version plus inner and envelope SHA-256, atomic durable write, strict corrupt/drift refusal | 2 core + 3 envelope + 1 real Session persist/restart tests | PASS |
| Boot KV retirement | Deterministic Config vs L1GHT classification, exact entry-set conservation, durable journal, replay at five interruption points, byte-exact rollback, compatibility reads, write refusal, checkpoint artifacts | 5 migration + 1 handler + 6 project-brain checkpoint/recovery tests | PASS |
| Held-out retrieval/calibration gate | Versioned fail-closed scorer and partially ratified MetricSpec; exact corpus/result alignment; top-5, abstention, wrong-ground act, paired regression and p95 checks | 5 scorer tests plus missing-evidence CLI probe | HARNESS PASS; PRODUCT GATE NOT_PROVEN |

## Exact green commands

- `cargo test -p m1nd-core co_change_ --lib` — 2 passed, 0 failed.
- `cargo test -p m1nd-mcp temporal_state::tests --lib` — 3 passed, 0 failed.
- `cargo test -p m1nd-mcp session::tests::session_persist_and_restart_restore_both_temporal_matrices --lib` — 1 passed, 0 failed.
- `cargo test -p m1nd-mcp boot_kv_migration::tests --lib` — 5 passed, 0 failed.
- `cargo test -p m1nd-mcp boot_memory_handlers::tests::boot_memory_migrates_then_refuses_hidden_dual_write --lib` — 1 passed, 0 failed.
- `CARGO_INCREMENTAL=0 RUSTFLAGS='-D warnings' cargo test --locked -p m1nd-mcp --lib north_carries_boot_memory_with_age_and_author -- --nocapture` — 1 passed, 0 failed, 1002 filtered out.
- `cargo test -p m1nd-ingest universal_adapter --lib` — 20 passed, 0 failed.
- `cargo test -p m1nd-mcp universal_auto_ingest --lib` — 1 passed, 0 failed.
- `cargo test -p m1nd-mcp unsupported_and_failed_auto_ingest_are_noncommittable_zero_mutation_audit_events --lib` — 1 passed, 0 failed.
- `cargo test -p m1nd-mcp --test project_brain_runtime` — 6 passed, 0 failed.
- `python3 -m unittest tests/test_m1nd10_g6_retrieval.py -v` — 5 passed, 0 failed.
- Missing corpus/baseline CLI probe — exit 1 with `status=NOT_PROVEN` and every missing artifact named.
- `cargo check --workspace --all-targets` — PASS.
- `cargo clippy -p m1nd-core -p m1nd-ingest -p m1nd-mcp --lib --all-features -- -D warnings` — PASS.
- `git diff --check` — PASS.

## Persistence invariants proved

Temporal restore accepts `None` only when the sidecar is absent. A present
truncated file, unknown version, digest mismatch, graph ordering change,
duplicate target, invalid strength, impossible accounting or partial envelope
is an error. A fault before atomic rename leaves the prior complete generation.
The brain checkpoint inventory now includes the temporal sidecar.

Boot migration writes the complete deterministic plan and exact original bytes
to a durable journal before publishing targets. Config-tagged or explicitly
prefixed keys move to `boot_config_v1.json`; all other keys move to deterministic
provenance-bearing `.light.md` files. The source is emptied only after target
digests and one-to-one conservation validate. Restart replays forward at every
commit boundary. Rollback restores the old source byte-for-byte and removes only
migration-created targets whose digests are still unchanged. `get/list/status`
remain read-compatible and name `migrated_config` or `migrated_light` plus the
target; `set/delete` return `retired` without mutating either store.

## Honest open evidence

The G6 R2 product-quality claim is **NOT_PROVEN**, not failed and not passed.
This checkout does not contain:

1. a blinded, adjudication-sealed held-out corpus of at least 200 tasks across
   multiple languages and repository sizes;
2. an exact-revision current run and paired ratified baseline over that corpus;
3. a ratified `seek` p95 latency SLO.

The PRD thresholds and existing warm composed `north` p95 <= 2 seconds contract
are represented in the MetricSpec, but the spec remains `partially_ratified`
until the missing `seek` SLO is supplied. Synthetic unit evidence proves only
that the scorer passes complete good evidence and fails closed on incomplete or
bad evidence; it is deliberately excluded from product-quality claims.

## Integration follow-up outside the G6 file boundary

`m1nd-mcp/src/server.rs` now presents the Boot KV compatibility surface as
retired and seeds the north-memory test's legacy state before Session
initialization, so the real one-way migration owns the transition. Static
review found the integration semantically consistent with the G6 contract, and
the server-owned regression was then mechanically rerun against the integrated
checkout with warnings denied: 1 passed, 0 failed. This closes the Boot KV
integration follow-up; it does not change the separately honest
`NOT_PROVEN` retrieval-quality verdict above.
