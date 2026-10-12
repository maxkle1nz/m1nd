# Bounded surgical context

Unreleased source candidate. This contract applies to `surgical_context` only;
it is not a claim about published 1.6.5 or a whole MCP response size limit.

The default request still returns the complete existing payload. Add
`max_output_bytes` to request pages. The budget is the number of UTF-8 bytes in
compact JSON of the handler payload after conversion to `serde_json::Value`.
It includes escaped source, symbols, neighbor records, elapsed time and page
metadata. Transport/runtime envelopes and pretty-print whitespace are excluded.
`context_page.serialized_bytes` reports the measured size. Integers below 1024,
negative/fractional/string budgets and budgets that cannot fit metadata or make
progress refuse. A long target path or indivisible metadata item may require
more than 1024 bytes; increasing the budget is the recovery door.

```json
{"agent_id":"editor","file_path":"src/core.rs","max_output_bytes":4096}
```

A bounded response adds `context_page` and may contain a fragment in
`file_contents`. `line_count` still describes the complete source. The source
anchor carries its full SHA-256 digest, total byte count, zero-based half-open
`byte_start`/`byte_end`, and one-based lines touched. An empty fragment has
`line_start:0` and `line_end:0`. Byte boundaries never split a UTF-8 character;
a page may split a line. There is no synthetic clipping text in the source.

`omitted` counts source bytes, symbols and neighbors absent from **this page**,
including parts already delivered on earlier pages. Boolean omission fields
cover the focused symbol and heuristic summary. These counts do not claim
that an omitted neighbor was explored. Coverage gains only neighbor records
actually emitted on successful pages; refusals and parse/read errors leave
previous visits and tool counts intact. The actor's refusal recovery restores
exact pre-call coverage and graph/plasticity/cache generations after verifying
the authoritative checkpoint. Partial callback changes are discarded; proof
capabilities and preview payloads remain invalidated by recovery.

Copy `context_page.continuation` into the next request, repeating the same file,
symbol, radius and include-tests options, plus a byte budget. A null continuation
means the ordered stream is complete. Source fragments come first, then symbols,
callers, callees, tests, focused-symbol details and heuristic summary. Neighbor
ordering is deterministic in bounded mode. Concatenate source fragments by their
byte anchors; metadata records are emitted whole and once when following the
returned cursor. A complete metadata item that cannot fit refuses rather than
being dropped; retry that cursor with a larger budget.

The cursor is a checked position, never an authorization token. Every page reads
through existing source authorization. Its SHA-256 context identity binds the
canonical target, source digest, node identity, symbol/radius/include-tests
options, symbol metadata, ordered neighbors, and graph/plasticity generations.
Changed source, target, options, neighborhood or generation refuses stale
continuation. Out-of-range and non-UTF-8 offsets also refuse. A cursor without a
budget refuses. Restart from the first page after a stale refusal.

Heuristic telemetry is recomputed when its item is returned. Its time-decayed
values are intentionally excluded from source/neighborhood snapshot identity;
the digest is not a frozen heuristic or a graph-wide integrity receipt.

A partial source page is not a complete file replacement or proof of edit
readiness. Retrieve and verify the complete source before editing. Internal
`north` callers and `surgical_context_v2` retain unbounded V1 inputs, preserving
V2 proof behavior. This output cap does not bound graph traversal, file reads,
server memory, HTTP/MCP framing or other tools.

Regression tests exercise compact byte accounting with escapes and UTF-8,
source reassembly, public stdio/actor refusal recovery, stable digests, omission counts, exact neighbor coverage,
invalid/stale cursors, source/path refusals, empty anchors, larger-budget recovery
and default compatibility. Hosted CI and native Windows proof remain separate.
