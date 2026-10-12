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
but missing source does not authorize reads of checkpoint copies. Relative roots
are not resolved against a new process's current directory for workspace recovery.

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
validation, and generic mutation authority floors are unchanged.

The portable unit tests cover canonical directory order, exact file authority,
missing sources, instance identity, and manifest/legacy compatibility. The Unix
process regression in `m1nd-mcp/tests/persist_runtime_root.rs` starts two real stdio
owners, creates actual checkpoints, and verifies source search, restored binding,
and unchanged `CURRENT`/manifest bytes before shutdown. Native Windows and hosted
CI remain independent proof requirements. This behavior is not part of the sealed
1.6.5 artifacts.
