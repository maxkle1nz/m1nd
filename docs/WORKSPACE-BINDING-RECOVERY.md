# Workspace binding after restart (Unreleased)

A bound owner may store its graph in an external runtime directory while its
persisted `ingest_roots.json` names the source repository. Previously, a restart
could infer that runtime directory as `workspace_root`. Literal search then
walked both the source and the runtime, returning internal checkpoint copies as
source matches.

The bound actor now restores the source read binding after accepting its existing
checkpoint identity and rebuilding the authoritative working set. It selects the
first canonical existing source directory from the recovered roots. Memory
sidecars and paths inside the runtime are excluded from that selection. Other
declared roots remain unchanged. The binding fingerprint reports
`workspace_root_source: "persisted_ingest_root"`.

A file-only graph retains its exact file as the workspace, with provenance
`persisted_ingest_file`; the parent directory gains no read authority. When
external source roots are declared but none is usable, `workspace_root` is absent
and provenance is `persisted_ingest_roots_unavailable`. The graph remains loaded,
but missing source does not authorize reads of checkpoint copies. A legacy relative
root has no recorded origin directory, so it is declared unavailable even if that
text names an existing path in the new process. Its stored bytes remain intact for
checkpoint identity; it grants no read root, caller coverage, discovery routing,
code-root enumeration, project selectors/runner targets, exact-root refresh, or
launcher compatibility. Live sessions and immutable HTTP read/identity snapshots
use the same code-root and project-root selection functions. Authenticated
`GET /api/file` therefore refuses a legacy relative root instead of reading the
new cwd. A foreign
launcher refuses before loading a public refresh or changing graph, roots, or
`CURRENT`. No original directory is guessed from the new cwd.

New ingests resolve their input to a canonical native filesystem path and validate
it through the shared exact path identity contract before changing session state.
That canonical path is persisted for replace and merge; an unresolvable or
unrepresentable source refuses without publishing a graph or roots. Native
Windows drive/UNC anchors are checked together; drive-relative and current-drive
rooted spellings do not become stored authority. Arbitrary source files remain
exact file roots. The existing owned `agent-memory` sidecar folding is preserved.

This is derived source binding, not an identity migration. Actor/checkpoint and
instance/lease identities remain unchanged, including legacy checkpoints keyed
by the runtime directory. Foreign checkpoint identities still refuse before
restoration. A source-bound owner must retain the launcher context needed by its
existing identity contract; losing it can still produce `BrainBindingMismatch`.
Project-brain manifest bindings remain authoritative.

Fresh or legacy states without an external source declaration, including pure
memory stores, retain their prior inference. Explicit ingest roots remain explicit
read roots: this change does not globally prohibit reads of runtime files or
rewrite the persisted root set. Launcher conflict, path authorization, checkpoint
validation, and generic mutation authority floors are preserved. Relative roots
are never explicit read authority, including in legacy no-source inference.

The portable unit tests cover canonical directory order, exact file authority,
missing sources, legacy relative declarations, canonical ingest identity, refused
refresh preimages, native path anchors, instance identity, and manifest/legacy
compatibility. The Unix
process regression in `m1nd-mcp/tests/persist_runtime_root.rs` starts two real stdio
owners, creates actual checkpoints, and verifies source search, restored binding,
and unchanged `CURRENT`/manifest bytes before shutdown. A second Unix process
regression changes cwd, denies its sentinel, and verifies that a foreign launcher
with a queued public refresh leaves graph, roots and `CURRENT` byte-identical.
These process probes are excluded on Windows by the file-level `cfg(unix)`; the
portable units run there. Native Windows, 32-bit targets, and hosted
CI remain independent proof requirements. This behavior is not part of the sealed
1.6.5 artifacts.
