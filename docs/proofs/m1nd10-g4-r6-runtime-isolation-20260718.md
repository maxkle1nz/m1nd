# M1ND-10 G4 / R6 Runtime Isolation Proof — 2026-07-18

## Verdict

`MACOS COMPONENT PASS`. The ratified cross-platform `G4` gate remains
`NOT_PROVEN` because its Windows checkpoint primitive and recovery battery were
not run. Physical power-loss behavior is also `NOT_RUN`; injected failpoints do
not substitute for that proof.

The frozen authorities were not edited:

- `docs/M1ND-10-PRD.md` — `00658cd88ce9dc5866f9b1fc6b9fbe594923e32fb900bde5bbc7740894c25c38`
- `docs/M1ND-10-UML.md` — `8a8a5fe9b9d2a4fc62c419e160e8dc2dcb4115f58d98f3f15a2d5031881dd32b`

## What changed

### Mutex-free actor ownership

`BrainSessionCell` stores `Option<SessionState>` behind a short parking-lot
mutex. A brain actor checks the complete state out, drops the storage guard,
executes the command/checkpoint, and returns the state through an RAII drop
guard. Therefore filesystem, network, dispatch, persistence, checkpoint, and
long analysis can execute while actor ownership is exclusive without retaining
a `SessionState` mutex guard. Legacy clone-only readers wait on a condition
variable and cannot observe the state while it is checked out.

The four actor paths use checkout:

1. read snapshot;
2. generic transport execution;
3. OCC proposal commit;
4. explicit checkpoint.

### Production transport adoption

The bound owner and hosted project brains now use the same per-brain actor seam
for generic REST dispatch, Streamable-HTTP MCP dispatch, stdio dispatch,
bootstrap ingest/orientation, medulla promotion re-ingest, tier recall,
save/shutdown, and checkpoint/eviction. The bound brain has one lazy actor just
like hosted brains.

Candidate naming, curation spawn, subgraph querying, manifest composition,
graph/file/mailbox/instance reads, and project-brain summaries capture immutable
facts first. Filesystem and graph analysis occurs after the session guard has
been released.

### Durability and health

- mutating actor success crosses `SessionState::persist` and a content-addressed
  checkpoint ACK;
- dirty project-brain eviction requires the exact checkpoint ACK and refuses an
  active/busy victim;
- persistence failure keeps read snapshots available, refuses new mutations,
  publishes `degraded_persistence`, and clears only after a real retry ACK;
- `/health` reads an independent cached runtime snapshot and reports stale/busy
  truth without waiting for the owner session;
- checkpoint recovery validates `CURRENT`, complete file digests, authority
  refs, predecessor/fallback, and old-or-new semantics.

### Authority boot ordering found during the transport sweep

Foreground and background HTTP have callable production-injection seams. A
preflighted owner-authority assembly installs G2 issuance and G3 consumption
atomically while `AppState` is uniquely owned. `Required` plus no assembly
returns before endpoint publication, heartbeat creation, router construction,
or socket bind. HTTP error classification preserves auth (`401`), forbidden
binding/signature (`403`), conflict (`409`), bounded-capacity overload (`429`),
integrity/corruption (`500`), and not-installed/unavailable (`503`) classes.

## Mechanical evidence

All Rust commands used the ratified target boundary:

```text
CARGO_TARGET_DIR=/Volumes/Cofre/.codex-m1nd-build-20260718
CARGO_INCREMENTAL=0
```

| Gate | Result |
|---|---|
| `cargo check --locked -p m1nd-mcp --features serve --lib` | `PASS` |
| actor checkout storage-mutex proof | `1 PASS / 0 FAIL` |
| graph-analysis lock-release filter | `3 PASS / 0 FAIL` |
| authority boot/status table tests | `3 PASS / 0 FAIL` |
| eviction persistence regression | `1 PASS / 0 FAIL` |
| `project_brain_runtime` | `8 PASS / 0 FAIL` |
| `runtime_jobs` | `14 PASS / 0 FAIL` |
| `checkpoint_store` | `13 PASS / 0 FAIL` |
| full `m1nd-mcp` library | `1049 PASS / 0 FAIL / 1 ignored` |
| strict clippy, serve + all targets + `-D warnings` | `PASS` |
| source lock audit | `73 files / 23 scopes / 0 forbidden operations / PASS` |

The ignored test is the existing manual cost benchmark
`daemon_handlers::tests::bench_daemon_tick_burst`; it is not silently counted as
executed.

## R6 measurements

### Health under a real 30-second owner stall

- measured owner stall: `31.277137958 s`;
- samples: `601` at `50 ms` intervals;
- empirical p99: `0.838625 ms`;
- maximum: `3.003625 ms`;
- samples at or above `100 ms`: `0`;
- one-sided exact zero-failure 99% upper bound: `0.007633230577054451`.

### Concurrent multi-brain workload

- brains: `8`;
- operations: `10,000` (`9,920` reads, `80` writes);
- elapsed: `7.268235291 s` under a `60 s` bound;
- lost writes: `0`;
- cross-brain observations: `0`.

### Checkpoint and degradation

- implemented checkpoint fault points enumerated: `15`;
- partial generations selected: `0`;
- selected state: old complete or new complete only;
- disk-full refusal, corruption, concurrent GC, fallback preservation, and
  retry confirmation tests passed;
- degraded reads stayed available;
- mutation apply calls while degraded: `0`;
- recovery required a real checkpoint ACK.

## Lock audit boundary

`scripts/m1nd10_g4_lock_audit.py` rejects the historical
`Mutex<SessionState>` shape and inspects every remaining production session-lock
scope for dispatch, persistence, filesystem, network, subprocess, graph
analysis, query orchestration, and checkpoint operations. It also requires all
four actor methods to use checkout and the checkout implementation to drop the
storage guard before returning state ownership.

This is conservative lexical evidence, not a Rust model checker. Runtime tests
corroborate the actor and the three highest-risk HTTP analysis paths by holding
the downstream operation open and probing the session boundary concurrently.

## Preserved failures

1. The first exact actor-test command used a short name and selected zero tests.
   The fully qualified name selected one test and passed.
2. The first strict all-target clippy found one needless borrow in the adopted
   all-brains recall call site. The borrow was removed; the identical strict
   command then passed.

No failure was relabeled as success, and the first commands are not counted as
mechanical proof.

## Independent review truth

The bounded askGOD review did not produce a valid verdict. The Fable route
returned `Credit balance is too low` before judgment. The full Fugu review and
its single narrow retry each exceeded the 10-minute review bound and were
stopped without returning the required `VERDICT / EVIDENCE / REQUIRED_CHANGES`
contract. Partial chain-of-thought-style progress output was discarded and is
neither approval nor rejection.

Therefore this component is **not askGOD-approved**. Its `MACOS COMPONENT PASS`
is supported by the mechanical source, compiler, lint, concurrency, latency,
fault-injection, and durability evidence above. The independent-review status
is `UNAVAILABLE`, and it does not promote or demote the explicitly bounded
mechanical result.

## Remaining proof debt

- `Windows`: `NOT_RUN`. Because the frozen G4 text explicitly requires the
  Windows checkpoint primitive and recovery battery, full G4 is `NOT_PROVEN`.
- `Linux`: `NOT_RUN` in this component window.
- physical power loss: `NOT_RUN`.
- the source tree is dirty and uncommitted; the result binds exact source-file
  digests rather than claiming a clean release artifact.

Machine-readable evidence:

- `docs/benchmarks/m1nd10-g4-r6-metric-spec-v1.json`
- `docs/benchmarks/m1nd10-g4-r6-runtime-result-v1.json`
- `docs/benchmarks/m1nd10-g4-r6-lock-audit-v1.json`
