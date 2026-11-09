# Automatic code freshness and branch views (unreleased)

The isolated agent CLI already rebuilds a reused automatic cache through the
public exact-root refresh door before trust or retrieval. `--state-dir` still
selects that automatic cache; it does **not** select `--runtime-dir`. The cache
identity remains canonical repository/worktree root plus Git branch and HEAD.
Edits without a commit keep the same cache identity and are handled by the
full refresh, rather than being mistaken for unchanged inputs.

This is automatic preparation/refresh **on a CLI invocation**, not a continuous
code watcher. The existing `auto_ingest` watcher handles document formats and
its authority floors are unchanged. An attached owner or an explicit/shared
runtime has different freshness semantics: this CLI reports its identity and
graph version as not checked/observed rather than implying an automatic refresh.

## Isolated launch policy

`kickstart` applies the same `--no-attach --state-dir` storage selection as
`agent` commands and exits nonzero when its result is refused. Isolated launches
reject an inherited attach argument before native runtime startup. Cache-base
preparation may precede that refusal; it does not imply a native process ran.
Automatic caches also reject graph, plasticity, configuration and runtime-base
overrides that could direct native persistence away from the versioned cache.
Errors name the conflicting selector without echoing its value. Explicit/manual
runtimes retain their separate configuration semantics and unverified receipts.

Native fixture proof checks that a warm `first-minute`, `orient` (including
`--skip-ingest`) and anchored `context` refresh before retrieval. Search must
return the newly edited graph-backed symbol, not merely persist it afterward.
This covers edits completed before invocation; it is not an atomic live view.
`kickstart` still includes `audit`, whose sovereign authority floor is unchanged;
these query proofs do not establish a successful ordinary native audit.

## Identity at query boundaries

`runtime.source_identity` records the selected root and branch/HEAD plus the
checkpoints where they were reverified: after acquiring the cache lease,
after initialization, around public tool calls, and after shutdown before
return. The ephemeral guard also observes root device/inode/birth time so a
directory replaced during the command cannot pass solely by reusing its path.
Git errors, a changed root or branch/HEAD fail the command without an
`ok/full_trust` response. The first detected failure remains latched even if
the branch returns during cleanup. Ownership and cleanup fences remain intact;
there is no automatic retry against another root or deletion of existing state.

This is checkpoint verification, not a filesystem transaction. The guard does
not attest dirty file contents, detect every change-and-revert between checks,
or persist inode history across invocations. The output explicitly says
`working_tree_contents: not_attested_by_identity` and `atomic_snapshot: false`.

## Source and graph versions

The native refresh now retains the existing complete code-ingest bundle long
enough to revalidate its sources after preparing owned memory and before
admission. This reuses the same source digests, discovery controls, build and
VCS context checks already implemented by the ingester. A changed receipt
refuses the candidate. No second scanner or optimistic early-stop policy was
introduced. When sameness is not established, the normal path is a full rebuild
or an explicit refusal, not reuse asserted from HEAD alone.

Successful refresh responses include `code_snapshot`:

- `update_mode: full_rebuild`, canonical root and discovered source count;
- code projection, ownership, pipeline, policy and VCS-context digests;
- producer build identity and `source_revalidated_before_admission: true`;
- scanned/parsed counts, binary/encoding skips and unresolved/ambiguous
  references; unreadable-path coverage is explicitly not attested;
- `code_ownership_coverage: COMPLETE`, scoped to the ingester's emitted code
  nodes/edges, with `semantic_completeness: not_proven`.

The projection describes code before memory/learned-state restoration, not a
digest of the final mixed graph. It does not prove every repository concept or
unsupported file was understood. Revalidation immediately before admission is
also not an atomic source snapshot: files can change after the last check.

`graph_generation` is the session's generation counter; it is not a permanent
global version across restarts. For an automatic cache, the CLI additionally
hashes `graph_snapshot.json` after confirmed native shutdown while it still
holds the cache lease. `runtime.graph_snapshot_version` identifies those exact
persisted bytes using SHA-256 and size. `update_status` distinguishes an
observed successful refresh, a refusal, and an update not observed by the CLI.
Missing, unsafe or changing snapshot files produce an unavailable receipt,
never a fabricated digest. This receipt does not parse or certify the graph's
semantics. Graph bytes can change because of learned weights as well as source
changes; their hash is not interchangeable with a commit ID or code projection.

## Branches and memory

Distinct worktrees, branches and commits select distinct existing cache
identities. Returning to a prior branch/HEAD reuses its cache only after the
usual refresh. Deleting a branch does not erase its retained cache or memory;
merging changes HEAD and selects its corresponding view. Local source edits,
deletions and renames are reflected by the next successful refresh.

Owned memory remains in its runtime's separate `agent-memory` sidecar. Refresh
restores it before applying the unchanged global 60% retention floor. A new
branch/commit view does not silently inherit another view's claims. This batch
does not implement a shared cross-branch memory store, migration or validated
promotion of common facts. Retention is not proof that an old claim remains
true: evidence freshness and direct source verification still matter.

## Relation-edit substrate

`GraphDiff` now applies relation removal and matches weight edits by endpoints
**and relation**, including pending edges. It removes topology rather than
simulating deletion with a zero weight; unrelated real zero-weight edges stay.
Callers must finalize before queries/persistence. Ambiguous representations
are refused before mutation instead of silently discarding learned state.

This library diff is not currently wired into the production code refresh or
document watcher. Its rebuild-equivalence tests cover representable unique
edge triples; `compute` remains set-valued, not a general multiset delta.
Node removal's existing tombstone semantics are also not a claim of full graph
compaction. No faster incremental production path is claimed by these fixes.

## Evidence boundary

The native test matrix uses the same explicit state directory across process
restarts, dirty edits, delete/rename, checkout back, branch deletion and merge.
It compares the code projection against independent full rebuilds, excluding
learned current weights and owned-memory nodes/edges. It also writes grounded
memory, recalls it after refresh/restart and checks that other commit views do
not inherit it or erase the old sidecar. These are controlled local fixtures,
not general latency, semantic-quality or uninterrupted-liveness benchmarks.

The research radar informed the conservative design principles, not adoption:
[Graphicle 0.1.13](https://github.com/Graphicle-AI/graphicle/releases/tag/v0.1.13)
describes proving inputs/configuration/extractor dependencies unchanged before
early reuse; [trace-mcp 3.39.0](https://github.com/nikolai-vysotskyi/trace-mcp/releases/tag/v3.39.0)
records deletion reconciliation and watcher-rule fixes. No dependency or code
from those projects was installed/copied, and their benchmarks are not evidence
for this candidate. Publication, release and installed-runtime adoption remain
separate, currently deferred actions.
