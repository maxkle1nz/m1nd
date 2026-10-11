# Graph change tracking

Unreleased source-build recovery: five existing `lock_*` handlers are discoverable again.
The published signed 1.6.3 release does not include this menu restoration; the
1.6.5 source candidate is not a release or a promise about an installed binary.
The default MCP menu remains 15 tools. Set `M1ND_TOOL_TIER=investigate` in the
**server's launch environment**, then restart/reconnect the MCP host. Its
`tools/list` advertises the core plus these five tools (20 total):
`lock_create`, `lock_watch`, `lock_diff`, `lock_rebase`, `lock_release`.
`M1ND_TOOL_TIER=full` advertises the full registry instead. Unknown values retain
core. Neither setting changes authority, proof, or host-binding requirements.

## First investigation

1. Use `north` to orient and `seek` or `search` to find the target. Copy a live
   canonical `node_id` from the result; do not invent one from a filename.
2. Ask `help` with `tool_name: "lock_create"` for the live input schema. Create a
   baseline using your `agent_id`, `scope: "node"`, and `root_nodes: [node_id]`.
3. Save the returned `lock_id`. Every later call uses that exact ID and the same
   `agent_id`, in the same running session.
4. Call `lock_diff` after a graph update that is independently authorized. Read
   changes before deciding whether to call `lock_rebase` to accept the new baseline.
5. Call `lock_release` when finished.

These are **graph snapshots**, not filesystem locks, reservations, mutual
exclusion, Git diffs, or source-content checks. Editing a file alone does not
update the graph. An empty graph diff does not establish unchanged source text.
Snapshots and watches are session-local and lost on restart; they are not a
resume/learning feature or durable investigation journal.

In this unreleased source build, graph-linked `search` results from a single-file
ingest root retain the graph's basename ID, including when the filesystem uses a
canonical Windows path. Check `graph_linked`: disk-only fallback rows are not
proof of graph membership. If distinct ingest roots make one relative graph ID
ambiguous, search omits the ambiguous file-content mapping rather than choosing
one root. This does not authorize the file's parent or siblings; existing
workspace/ingest read-root checks still apply.

`help(intent: "track changes in the graph")` offers this sequence. Help exposes
the full registry, but help text does not register tools in a host. If a host
cannot call an unadvertised tool, use `investigate` and confirm its actual
`tools/list` before calling it. Public stdio discovery/calls are testable here;
this is not a claim that every host UI has been tested.

## Advanced recipe

With the investigate menu selected, use the following arguments in consecutive
`tools/call` requests. Replace `LIVE_NODE_ID` with a returned canonical ID and
`RETURNED_LOCK_ID` with the create response; keep the process and agent unchanged.

```json
{"name":"lock_create","arguments":{"agent_id":"investigator","scope":"subgraph","root_nodes":["LIVE_NODE_ID"],"radius":2}}
{"name":"lock_watch","arguments":{"agent_id":"investigator","lock_id":"RETURNED_LOCK_ID","strategy":"on_ingest"}}
{"name":"lock_diff","arguments":{"agent_id":"investigator","lock_id":"RETURNED_LOCK_ID"}}
{"name":"lock_rebase","arguments":{"agent_id":"investigator","lock_id":"RETURNED_LOCK_ID"}}
{"name":"lock_release","arguments":{"agent_id":"investigator","lock_id":"RETURNED_LOCK_ID"}}
```

Between watch and diff, use only a graph update allowed in your current context.
For example, existing `ingest(mode: "refresh")` requires the exact declared root
and retains all its guards; the menu does not grant refresh permission.
`on_learn` observes admitted learning notifications and does not unlock `learn`.

- `node`: the explicit root set. `subgraph`: outgoing-edge BFS, radius 1–4,
  default 2. `path`: roots plus optional `path_nodes`, an explicit node set;
  it does not calculate or validate a connected path.
- Roots must be nonempty. Exact canonical IDs are preferred; an exact unique
  label is also accepted. Unknown or ambiguous identifiers refuse. `query` and
  `query_neighborhood` refuse rather than silently producing an empty baseline.
- Baselines capture scoped nodes and all incident edges, including incoming and
  outgoing boundary edges. `baseline_edges` counts directed CSR arcs: a
  bidirectional logical edge contributes two. Parallel arcs use deterministic weight-sorted multiset pairing, not stable
  per-edge-instance identity. Edge keys are opaque collision-safe JSON tuples;
  do not parse them as delimited filenames.
  Diff reports canonical node additions/removals, internal edge changes,
  boundary edge changes, weight changes and generation staleness.
  Weight changes are reported only when the absolute delta exceeds 0.001;
  `no_changes` means no detected change above that tolerance. `diff_truncated`
  flags capped added-node, added-internal-edge and added-boundary-edge previews
  only. `no_changes` is computed before those caps, so truncation cannot hide a
  detected change behind an empty verdict.
- `manual` buffers no notifications. `on_ingest` and `on_learn` buffer coalesced
  graph-trigger notifications for the session; diff drains them but keeps the
  baseline.
  The drained count is notification batches, not the number of mutations.
  There is no background scheduler or filesystem watcher; `periodic` refuses.
- Rebase resamples the scope and preserves watch configuration. Review first:
  rebasing discards the old comparison point. If the entire observed scope has
  vanished, rebase refuses and preserves the old baseline so you can inspect
  removals or release it. Release discards the snapshot and
  watcher. Another agent ID cannot operate the snapshot; this ownership check
  is not authentication against a caller that can choose its agent ID.

## Verification boundary

The regression suite covers registry/menu/help consistency and the runtime
snapshot lifecycle. An isolated public stdio probe must establish
`tools/list` → `tools/call`, graph change → diff, rebase and release. Exact
commands/results belong to the candidate's test report. This slice does not
claim hosted UI proof, durable trails, a learning authority change, a new
release, or the full cross-platform integration gate.
