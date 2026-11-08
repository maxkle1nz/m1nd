# Refreshing code with owned agent memory

Unreleased source contract. The existing declared-root admission and the
ratified global 60% shrink floor remain unchanged.

An isolated agent cache can hold both source-code nodes and agent-authored
L1GHT memory. Refresh scans the source root again and restores the runtime's
owned memory. Comparing only the source scan with the mixed live graph could
refuse an unchanged repository: a nine-node source scan was compared with
seventeen live nodes after one grounded memory write.

Refresh now prepares the complete code-plus-memory candidate before applying
the floor. It uses the same light adapter, namespace, file selection, recency
cap and legacy-file exemption as normal memory reload. Evidence is resolved
and the candidate is finalized without mutating the live session or durable
state. Read/parse errors returned by the existing light adapter abort before
the graph swap; directory enumeration retains that adapter's existing
best-effort behavior rather than promising complete error detection.

The exact-root, single-flight and root-set checks still run. The global guard
still refuses when `candidate_nodes * 100 < live_nodes * 60`, naming both
counts and leaving the live graph/snapshot untouched. This is a global graph
retention check, not a separate proportional retention promise for code nodes.
Memory that is missing on disk or excluded by the existing cap earns no
candidate credit.

Finalization installs the prepared code-plus-memory graph and preserves the
existing learned-state reconstruction. It does not reload memory to construct
another graph after the floor. An explicit prepared marker also covers the
case where no memory existed during preparation, so a late sidecar cannot
silently change the admitted candidate. Freshness reporting uses the updated
inventory after installation.

Regressions cover memory-dominated unchanged code, genuinely narrow scans,
missing memory, cap/legacy selection, grounded evidence, root invariance,
snapshot invariance on refusal and a sidecar appearing after preparation.
The earlier refused warm-cache probe remains a RED receipt for its revision.
First-query, cross-process recall and full-suite results belong to the exact
tested candidate; this change does not update an installed runtime or release.
