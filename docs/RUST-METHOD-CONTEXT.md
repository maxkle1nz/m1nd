# Rust method context

Unreleased source contract. This behavior is absent from the sealed 1.6.5 artifacts.

`surgical_context` and `surgical_context_v2` supplement their legacy Rust symbol
list with methods declared directly inside module-level `impl` and `trait`
bodies, including inline modules. The existing Rust grammar supplies source
ranges without building graph identities or resolving types. A method uses the
existing wire type `"function"`; there is no new method selector or tool.

For example, `surgical_context(file_path:"src/engine.rs", symbol:"remove_edges")`
can focus a direct `impl Engine` method that the legacy container scan skipped.
Ranges use one-based inclusive lines and excerpts use the original source lines.
Required trait signatures stop at their semicolon. Modifiers split across lines,
UTF-8, CRLF and braces inside literals retain grammar-derived method anchors.
Preceding attributes and documentation comments are outside the method range.
The existing excerpt limit still applies; a range can exceed its excerpt.

Existing symbols keep their name, wire type and list position. When an exact
declaration or `fn` keyword anchor and name identify an existing method, its
range and excerpt are corrected in place. Previously skipped methods append
in source order. Free functions and containers retain their existing behavior.
The selector still takes the first matching name using its existing matching
rules; same-name methods are not resolved by receiver or qualified path.

The added scan does not descend into function bodies, closures or macro token
trees. Malformed methods do not gain precise ranges. It does not remove legacy
aliases or false positives, expand macros, resolve references, or establish a
complete Rust symbol catalog. Without the `tier1` grammar, the existing fallback
remains. No new dependency is required.

Bounded V1 requests use the same extraction, but symbols and focused-symbol
metadata may arrive on later pages. Follow continuation before treating omitted
metadata as absent. Reassemble source by byte anchors and verify the full-source
digest before editing. The output cap does not limit full-file reads, parser
work, graph traversal or server memory; extraction can run again on each page.
See [bounded context](BOUNDED-SURGICAL-CONTEXT.md).

Regressions cover direct and generic impls, required/default trait methods,
split modifiers, literal braces, comments, UTF-8/CRLF, same-line homonyms,
legacy selection/order, nested declarations, malformed methods, excerpt limits
and the no-grammar fallback. Source extraction is not compiler or executed-test
evidence. Full integration, hosted and native Windows results belong to their
exact tested candidate.
