# Retained graph diff application (Unreleased)

`GraphDiff::apply` removes stale edges from the retained graph and restores a soft-deleted node at its existing `NodeId`. Removing a call while both functions remain in the source no longer leaves the old call edge behind. Removing and restoring a source identity clears its tombstone without renumbering unrelated nodes.

## Edge removal and retained state

`Graph::remove_edges` accepts `(source, target, relation)` selectors. A forward edge matches the supplied endpoint order; a bidirectional edge matches either order and loses both materialized arcs. Other relations between the same endpoints and separately declared opposite forward edges remain. An absent match leaves the graph, string interner, generation, and finalized state unchanged.

A matching batch rebuilds forward and reverse adjacency and PageRank once. Each retained arc keeps its weight, metadata, and all seven plasticity columns, including asymmetric learning on the two arcs of a bidirectional edge. Re-finalization also retains the existing multiplicity of bidirectional self-loops. The staging distinction between materialized arcs and newly added edges is local to the rebuild; this change adds no `Graph` field or persistence format.

Consecutive `RemoveEdge` actions share one rebuild. Actions separated by additions or other mutations keep their original order. A restored node receives its extracted label, type, tags, and line range while retaining its identity, activation, and learned node state. Later node or edge additions still require `Graph::finalize` before queries. Applying a diff is not an atomic source-edit authority transaction.

## Proof boundary

The regression compares a retained graph after `GraphDiff::compute` and `apply` with independent clean ingestion at nine TypeScript and JavaScript source states: addition, call removal and restoration, file removal and restoration, rename and restoration, and export changes. It checks active node/edge sets and stable IDs for unchanged importers and barrels. Separate regressions exercise relation-specific removal, reverse adjacency, plasticity, self-loops, action order, and no-op behavior.

These tests supply already resolved extraction rows to `GraphDiff`. They do not prove dirty-file discovery or reference re-resolution. Missing barrel or re-export relationships in both graphs are not counted as supported relationships.

The public `ingest` refresh path currently performs a full rescan, including when its request sets `incremental: true`; it does not call this retained-graph diff path. There is no public `Ingestor::ingest_incremental` API. This repair makes no public refresh speed claim and changes neither authority checks nor the sealed 1.6.5 release.
