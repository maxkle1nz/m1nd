# Test-source classification (Unreleased)

Context neighborhoods and impact share the source-path classifier used by xray.
It examines the path after `file::` or a namespaced `::file::` marker, stopping
before a symbol-kind suffix. Words in a namespace or symbol name do not make a
production file a test.

The path conventions are exact components `tests`, `test`, `__tests__`,
`__test__`, and `e2e`; components beginning with `test_`; the existing Rust
`_test.rs` and `tests.rs` names; and `.test`/`.spec` filename suffixes before
JavaScript/TypeScript extensions. Both slash directions are recognized.
`contest.ts`, `latest.ts`, `inspect.ts`, and ordinary files under `examples/`
are not tests just because their names contain related substrings.

Context also honors explicit `test` tags, including Rust inline unit tests, and
existing test relations. With `include_tests:true`, it places test neighbors in
the `tests` section. The existing `include_tests:false` behavior is preserved:
neighbors remain in callers/callees rather than being hidden. Traversal, weights,
and the union of returned neighbors are unchanged. Impact uses the shared helper
and explicit tags for ranking; this change adds no hard filter.

Xray remains path-based. A structural reference from a recognized test source can
mark a target exercised under its existing rules; a tag alone does not gain that
credit here. Examples and production-name substrings do not become test proof.
Structural references do not establish that a test was executed or passed.

The classifier recognizes `.mts`/`.cts` path conventions, but the current
TypeScript extractor does not ingest those extensions. Classification tests do
not establish extraction support. No language-resolution, authority, writer,
checkpoint, or calibration guarantee changes here. This is an unreleased source
change; it does not alter sealed 1.6.5.
