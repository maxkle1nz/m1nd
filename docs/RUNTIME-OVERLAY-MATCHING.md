# Runtime overlay matching

Unreleased source contract. This change does not alter sealed 1.6.5 artifacts
or enable any public runtime-overlay authority surface.

The overlay keeps its existing span-to-node rules:

| Strategy | Existing matching behavior |
| --- | --- |
| `LabelMatch` | Lowercase exact label match first, including empty/short labels and duplicate labels. Otherwise both lowercase strings must have at least four UTF-8 bytes, the shorter must be at least half the longer, and either string must contain the other. |
| `CodeAttribute` | Exact lowercase label lookup from `attributes["code.function"]`, without substring fallback. This matches the implementation's existing behavior. |
| `ExactId` | Case-sensitive external ID lookup, with the existing node-index bound. |

Byte length remains byte length after Unicode lowercase conversion; it is not
a character-count rule. A span below the configured minimum duration remains
processed but is neither mapped nor unmapped. Heat arithmetic, decay, counts,
errors, durations, boosts and the 100-hot-node limit retain their behavior.

For label substring fallback, one ingest lazily builds a table in the existing
label-map visit order. Each distinct lowercase label with at least four bytes
gets its byte-presence signature once. Exact-only batches, short fallback names
and the other strategies never build this table. A second list retains only the
eligible table indices for the last fallback name's byte length. A changed length
rebuilds those indices without recomputing label signatures. Both lists use O(U)
extra storage; neither persists across ingests or grows with distinct query names.

A fallback can reject a pair only when some byte-signature bit of the shorter
string is absent from the longer string. Signatures fold each UTF-8 byte's high
bit, so collisions remain possible and never establish a match: either string
must still contain the other. Label-map order and duplicate node expansion stay
unchanged. Dense signatures can add work without rejecting candidates, and the
first fallback signs even labels outside its length ratio. The extra table and
index indirection trade bounded memory and initial work for signature reuse;
worst-case containment work is unchanged.

Exact IDs use `Graph::resolve_id` instead of rebuilding a cloned external-ID
map. Only label-based strategies populate the lowercase label index. Reverse
output IDs borrow graph strings until an owned result is needed, preserving
the last alias encountered in the existing graph-map iteration. Removed IDs,
retained labels and out-of-range ID entries keep their previous behavior.

Hot-node collection, stable heat-only sorting and truncation stay in their
existing order. Equal-heat order and cutoff membership are not deterministic
across fresh hash maps; the change adds no ID tie-breaker. No graph field,
persistence format, dependency or configuration is added.

Regressions pin mapping boundaries, Unicode, duplicates, aliases, removed and
out-of-range IDs, duration filtering, per-node counters, decay and the existing
tie order. Controlled timing on one fixed graph does not establish general
throughput, productivity or the cause of a historical suite failure. The local
stress limit remains five seconds; CI's separate allowance does not prove that
local gate. Full-suite and platform results belong to the exact tested source.
