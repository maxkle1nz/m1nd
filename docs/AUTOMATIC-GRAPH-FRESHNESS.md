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

## View receipts and Git selection

`runtime.view` identifies the selected automatic cache view using its existing
cache key, canonical worktree root and branch/HEAD identity. The code version
is reported separately, only when successful refresh and persisted graph bytes
were both observed. Local edits can change that code version while keeping the
same view ID. A view ID is not an archive of every dirty working-tree state.

The memory section names the cache view as its scope and reports only observed
restoration counts/caps from a successful refresh. `claim_origin: not_attested`
and `claims_freshness: not_revalidated` remain explicit even when restoration
succeeds: evidence resolution counts do not prove the text of a claim true.
Restoration also preserves an allowlisted `evidence_check` state from the native
report: `verified_against_stored_inventory`,
`unverifiable_until_code_reingest`, or `not_reported`. A zero stale-evidence count
is a reported count, not proof that claims are true or that every evidence item
was verified. The native inventory label can cover a refreshed inventory rather
than the bytes present when the claim was authored; it does not attest complete
per-claim read coverage. Unrecognized diagnostic text is not forwarded.

No memory is copied between views. Cold/manual/shared/attached paths do not
invent restoration or source-version observations.

Automatic identity selection refuses inherited `GIT_DIR`, `GIT_WORK_TREE`,
`GIT_COMMON_DIR` and `GIT_NAMESPACE` selectors rather than labeling the selected
source with foreign Git metadata. It also requires Git's inside-worktree probe
to answer `true`; a bare repository is not a source worktree. Errors name the
selector without echoing its value. Remove the conflicting selector explicitly;
the CLI does not clear it silently. Non-Git source roots remain supported.

Linked-worktree fixture proof includes two detached worktrees at the same HEAD,
divergent histories, dirty delete/rename, branch deletion/recreation and worktree
removal/recreation across process restarts. Each exercised source projection is
compared with an independent rebuild for that root. A separate memory proof
returns to the exact old branch/HEAD that actually owns the claim and recalls it
without changing its sidecar bytes. Recreating a root at the same path can reuse
the same cache identity after refresh; root history and old claim origin are not
persistently attested by that identity.

## Context anchor changes during a command

`agent context` calls the ordinary read-only `am_i_stale` door by name for the
selected source anchor before retrieval. After retrieval it validates the
declared primary path against the canonical selected anchor (including the
legacy nested primary field), and the actual returned connected-file paths
(at most five) inside the selected repo,
then checks anchor plus those files against the native inventory. Malformed,
out-of-scope or over-budget capsules are discarded rather than truncated. A changed/missing anchor discards the capsule,
returns `source_changed_during_context` with no results and a nonzero exit, and
hands off to direct proof. The command does not refresh/retry automatically.
The next isolated invocation performs its normal full refresh.

Unknown files, unavailable tools, incomplete or malformed replies remain
`source_freshness.status: not_proven`; the unavailable precheck is not retried.
Only exact successful coverage at both boundaries yields `fresh_at_boundaries`.
The receipt distinguishes `before_scope: selected_anchor_only` from
`after_scope: anchor_and_returned_connected_files`. Older responses without a
connected-file list retain an explicit selected-anchor-only scope. The checks
cover only these bounded returned files, not new files or the whole tree. Neither boundary observes an atomic filesystem snapshot;
changes after the final check or change-and-revert races remain outside it.

## Read-only memory sharing review

The unreleased command inspects only the currently selected automatic cache:

```bash
node npm/bin/m1nd.js agent memory-check --repo /workspace/repo-alpha \
  --no-attach --state-dir /workspace/private-state --json
```

It does not start a native runtime, discover/attach an owner, create state,
copy memory or ingest anything. Git identity reads still occur. Missing views
are reported without creation; sibling views are not searched. Existing view
identity is checked against its manifest/key. Inspection requires no-follow
file reads; an unavailable filesystem capability refuses inspection.

Legacy memory reports `origin_not_attested`: `Created` and `Origin-Brain` are
not author-time evidence hashes or branch identity. Evidence paths/hashes and
missing/unsafe/unreadable states are observations, not proof of claim truth.

An optional `--provenance-manifest <json>` accepts a bounded explicit baseline:
`schema: m1nd-memory-sharing-baseline-v1`, `source_identity` in the existing
cache-identity-v1 shape, and `documents` containing each memory basename,
`sha256`, and complete `evidence: [{path, sha256}]`. Hashes are 64 lowercase hex
characters. The supplied source root must be the same canonical project root;
its branch/HEAD may differ and are returned separately from the selected view.
Different worktree roots are deliberately not treated as a common project.

The baseline is always `supplied_unverified`, with
`baseline_identity_authenticated: false`. Complete matching document/evidence
bytes produce only `candidate_only`. Changed documents/evidence, deletion or
rename invalidate the candidate; missing provenance, partial coverage and
unreadable evidence cannot produce a candidate. No baseline is minted
retroactively or authenticated by this command. Every result preserves
`semantic_validity: not_certified`, `sharing_authorized: false`,
`mutations: false` and `observations_atomic: false`. This is a proposal review,
not an automatic sharing/promotion implementation.

Reads reject symlinks, absolute/traversing evidence paths, duplicates and
in-flight mutations. Limits are 32 immediate memory files, 64 KiB/document,
16 evidence paths/document, 1 MiB/evidence file, 16 MiB evidence total and
128 KiB supplied provenance. Exceeding a limit refuses rather than truncates.
Native fixtures check dirty invalidation, source-evidence rename/delete,
branch isolation and return-to-origin consistency alongside rebuild parity.

## Plain Python call edges (unreleased)

Python's regex extractor previously emitted receiver-method calls from the file
node and did not emit bare `helper()` calls. The new path links a caller function
ID directly to a uniquely identified function in the same module, without a
global name-resolution guess. Different caller IDs keep their own call edges.
Existing node IDs and legacy receiver/reference extraction remain unchanged.

This is a conservative subset, not a complete Python AST or runtime call graph:
simple undecorated top-level definitions, simple parameter lists, and flat
assignment-free, quote-free caller bodies. Ambiguous names, rebinding, local
shadowing, nested/control/dynamic scopes, complex headers and multiline lexical
forms are omitted by the new path. A missing edge therefore does not establish
that a call cannot occur. Existing legacy reference heuristics keep their
separate limitations.

Positive fixtures require a direct function-to-function call before testing its
removal. Removing only the call keeps both nodes, drops the edge and compares
the supported full-root refresh with an independent full-root source-projection
digest. Governed v1 keeps exact-file refresh disabled; this change does not
enable it.
Unknown bare names must not bind to a same-name definition in another file.

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
