# M1nd 10 G2 checkpoint — PREPARED authority recovery

Date: 2026-07-18  
Scope: owner-side `AuthorityRuntime` foundation  
Gate status: **checkpoint; macOS and isolated Windows-source gates complete;
final askGOD review pending**

## Outcome

`m1nd-mcp/src/authority_runtime.rs` now uses an fsynced, self-digested
`PREPARED` descriptor to recover one authority transition to exactly its old or
new state across the replay ledger, authority journal, protected epoch marker,
and authoritative state record.

This is a descriptor-bound recovery protocol. It is **not** a claim that the
four artifacts are one physically atomic transaction, and it does not promote
M1nd to `FULL_AUTONOMY`.

## Publication protocol

One process owns the runtime for its lifetime. Under that owner lease, a
transition publishes in this order:

1. Build an exact `AuthorityTransitionDescriptorV1` containing the prior state,
   next state, journal record, optional replay record, prior byte lengths, and
   prior replay-tail digest.
2. Bind those artifacts with a transaction id and bind the descriptor body with
   its own digest.
3. Atomically replace and fsync `authority-transition.prepared.json`, including
   the parent-directory sync.
4. Append and fsync the exact staged replay record when the transition consumes
   a replay claim.
5. Append and fsync the exact hash-chained authority journal record.
6. compare-and-advance the protected epoch snapshot. This is the commit marker.
7. Atomically replace and fsync `authority-state.json`, including the
   parent-directory sync.
8. Remove the PREPARED descriptor and fsync its parent directory.

The next state digest binds both the journal root and the replay root. The
descriptor validator also requires one exact revision/epoch/journal increment,
the expected replay increment, and a non-regressing transition timestamp shared
by the next state and journal record.

## Recovery decision table

Recovery reads and validates the descriptor before changing any durable
artifact. It never calls protected compare-and-advance while recovering.

| Protected snapshot | Required disk evidence | Recovery action |
|---|---|---|
| Exact descriptor prior | Disk state is the exact prior state; journal/replay are either exact prior tails or the descriptor's exact prepared tails | Truncate only exact descriptor-bound prepared tails to their recorded prior byte lengths; remove descriptor |
| Exact descriptor next | Journal and any required replay tail are the exact descriptor-bound prepared records; disk state is exact prior or exact next | Forward-write only the descriptor's exact next state when needed; remove descriptor |
| Anything else | Any epoch, state, descriptor, digest, length, or tail not matching the two cases above | Fail closed; do not infer, truncate, or advance |

A valid-looking replay or journal tail without the PREPARED descriptor is not
evidence of success. A corrupt descriptor or an unbound extra tail is not
repaired heuristically.

## Fault model covered by the deterministic battery

The Unix test harness injects a stop immediately after each named boundary:

- `descriptor`;
- `replay`;
- `journal`;
- `protected-cas`;
- `state`;
- `cleanup`.

The same six boundaries are exercised for bootstrap and for an authenticated
runtime mutation. Separate backends model both ambiguous protected-CAS outcomes:

- the backend returns an error without advancing, which must recover the exact
  prior state;
- the backend advances and then returns an error, which must forward-complete
  the exact next state.

Additional cases cover corrupt descriptors, torn/unbound replay tails, exact
valid tails with no descriptor, restart parity, the unique owner lease, replay
competition, and fail-closed bootstrap/open states.

## Platform posture

| Platform | Owner lease | Runtime posture | Test posture |
|---|---|---|---|
| Unix/macOS | Lifetime in-process token plus nonblocking OS `flock` on the canonical lock path | Supported by this checkpoint | Authority runtime battery enabled |
| Windows/non-Unix | No equivalent lease adapter is implemented | Explicitly unavailable and fail-closed before runtime files are created | Unix-specific battery is not compiled; cross-target compilation is still required |

A successful Windows cross-compile would prove only that the fail-closed path
compiles. It would not prove a supported Windows authority runtime.

## Mechanical evidence

Final macOS rerun after the non-Unix cfg split, timestamp hardening, and stable
integration-tree declaration:

```text
RUSTFLAGS='-D warnings' cargo test --locked -p m1nd-mcp --lib \
  authority_runtime::tests --no-fail-fast
16 passed; 0 failed; 974 filtered

RUSTFLAGS='-D warnings' cargo check --locked -p m1nd-mcp --lib
PASS

RUSTFLAGS='-D warnings' cargo clippy --locked -p m1nd-mcp --lib -- -D warnings
PASS
```

The full Windows cross-target command was also run:

```text
RUSTFLAGS='-D warnings' cargo check --locked -p m1nd-mcp --lib \
  --no-default-features --target x86_64-pc-windows-gnu
BLOCKED before m1nd-mcp source compilation:
  x86_64-w64-mingw32-gcc: No such file or directory
```

A second non-mutating attempt used the installed Apple Clang as the COFF
compiler. Clang emitted a Windows COFF object, but the full crate remained
blocked in Tree-sitter C build scripts because this machine has no Windows C
SDK headers (`stdlib.h` not found).

To test the G2 source boundary independently of those unrelated native parser
dependencies, a temporary crate imported the real file by absolute `#[path]`
and depended on the real workspace `m1nd-control` plus `parking_lot`, `serde`,
and `serde_json`. With the same crate-root `#![allow(unused)]` posture as
`m1nd-mcp`, the exact source compiled warning-clean:

```text
RUSTFLAGS='-D warnings' cargo check --locked \
  --target x86_64-pc-windows-gnu
PASS: m1nd-g2-windows-compile
```

This proves that the actual non-Unix G2 source and its control-plane dependency
compile for the installed Windows Rust target. It does **not** prove the whole
M1nd crate cross-compiles, that the fail-closed branch was executed on Windows,
or that Windows is a supported authority-runtime platform.

The cumulative close still requires:

- run a final read-only askGOD review over the resulting diff and this proof;
- record final source/proof digests.

## Explicit non-claims and residual risks

- No production private key, HSM, Secure Enclave key, or hardware-protected
  epoch backend is wired or proven here.
- The software epoch backend is labeled
  `SOFTWARE_TEST_ONLY_NOT_PROVEN`; it is deterministic test infrastructure, not
  anti-rollback hardware.
- `multi_artifact_atomicity_proven` remains `false`. The claim is exact
  descriptor-bound old-or-new recovery at synchronized boundaries, not physical
  atomicity under arbitrary storage-controller or torn-sector failure.
- Corrupt, partial, unknown, or unbound states fail closed; automatic repair is
  intentionally not claimed for them.
- Path-level symlink refusals do not close the directory replacement / symlink
  TOCTOU race between validation and open. Descriptor and state durability do
  not prove directory-handle pinning against a hostile same-UID process.
- Alternate bind-mount or mount-alias paths depend on the operating system's
  `flock` identity semantics; alias resistance is not independently proven.
- Session challenges are volatile across restart.
- The semantic action catalog is gated, but parity with every transport schema
  remains a separate integration proof; `transport_schema_parity_proven` stays
  `false`.
- No `FULL_AUTONOMY`, self-promotion, G2 completion, production readiness, or
  hardware assurance claim follows from this checkpoint.

## Ratified-document boundary

This addendum is deliberately separate from the frozen M1nd 10 PRD and UML.
Neither ratified document is modified by this checkpoint, so their ratified
hashes remain outside this implementation proof.
