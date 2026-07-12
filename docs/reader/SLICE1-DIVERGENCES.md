# m1nd Reader — Slice 1 divergences (honest residue)

> Companion to `docs/reader/M1ND-READER-DONORS.md` (the slice-1 law). What the
> implementation actually did where it departed from the ideal, and why — recorded
> per the honesty law (absence/ambiguity shown, never faked). No backend/Rust was
> touched this slice; every gap that would want the server is deferred honestly.

## 1. Structure filters the snapshot CLIENT-SIDE (no `GET /api/file/symbols`)
The outline and click-to-def derive from the full `/api/graph/snapshot`, filtered in
the browser (`useGraphSnapshot`, cached once per brain root). The dossier's Risk 6
named the thin owner view `GET /api/file/symbols?path=` as the clean optimization if
per-file filtering of a 9k–14k-node snapshot proves hot. This slice is PROHIBITED
from touching the owner, so the client filter ships and the owner view is deferred.
Cost today: one snapshot fetch per brain when Show Code first opens a file (cached
thereafter). Not a correctness gap — a performance note. **Future:** a real backend
slice.

## 2. Click-to-def coverage is uneven BY LANGUAGE — by design, not a bug
Rust jumps by call-edge; TS/JS/Python/Go jump by def/import/use; a call-only
reference in a non-Rust language ABSTAINS ("no grounded target — call edges are not
tracked for this language yet"). This is the honest degradation the dossier mandates
(docs/PATHOS.md Known Problems: "Method-call edges exist for Rust but not
TS/Java/Go/Python"), encoded as data in `reader/languages.ts` (`callEdgesTrusted`).
Recorded here so it is known the reader IMPROVES as the graph's method-call edges
reach more languages — never a fabricated jump in the meantime. An ambiguous receiver
(≥2 grounded same-name targets) renders a CANDIDATE list; the human picks.

## 3. Freshness is per-BLOCK, not per-symbol
Dossier item 4 asked for freshness "where the block/node carries receipt state". The
UI knows receipt state per BLOCK (`rollup.state`), not per symbol — so the outline dot
carries the OWNING block's state (an existing fact, the house dot color), uniform
across a file's symbols, titled honestly ("the receipt state of the block this file
belongs to"). Per-symbol receipts do not exist on the server; inventing them was
refused. Node-level `change_frequency` / `last_modified` ARE carried on each
`OutlineSymbol` (existing graph facts) but are not dressed as "receipts" — surfacing
them as a churn hint is a later, honest add.

## 4. Fold ranges: DELIVERED (not deferred)
Item 5 permitted deferring folds to slice 2 if the spans forced a hack. The
line-based renderer (Shiki `codeToTokens` → own rows) made folds clean, so they
shipped: each multi-line symbol's `[line_start, line_end]` is a gutter-caret fold
(`foldRangesFromSymbols`). Honest edge: a fold whose start line is itself hidden by an
OUTER fold renders no placeholder (the outer collapse already hides it) — acceptable,
not a hack.

## 5. Highlight proven in two layers (theme by unit test, paint by e2e)
Shiki tokenization is async + browser-only (the donor is a LAZY chunk). node:test
does NOT load Shiki — the unit tests prove the pure logic (outline mapping,
edge→target resolution, degradation/abstain) and the THEME (every emitted color is in
the paper/ink palette; no violet, no neon). The rendered PAINT is proven in the
browser e2e (`e2e/reader.spec.ts`): real colored token spans appear and NONE carries a
violet/neon color. This keeps unit tests fast and wasm-free while proving the visible
result honestly.

## 6. Large-file guard (dossier Risk 5)
Above 200,000 chars the reader renders plain text with an honest note ("syntax paint
paused for a large file"). Windowed/virtualized highlighting (the CM6 re-entry
trigger) stays a later slice, decided on measured pain — never adopted by default.

## 7. Bundle (measured; the air-gap law holds)
- **Initial app chunk:** `index-*.js` 108.72 → **112.91 KB gz** (+4.19 KB gz — the
  reader UI logic; NOT the donor).
- **Donor is LAZY** (loaded only on first code paint, never at app start):
  `highlighter-*.js` (Shiki core + JS engine) **54.35 KB gz**; one grammar chunk per
  language, fetched only for the viewed language — gz: rust 2.71, typescript 16.23,
  tsx 16.62, javascript 16.62, python 9.09, go 5.14, markdown 5.66, bash 6.07,
  toml 1.28, json 0.77.
- **No wasm** (`shiki/engine/javascript`), **no CDN** (all chunks are same-origin
  local assets) — the air-gap law is intact. Reading a Rust file costs ~57 KB gz
  on demand (highlighter 54.35 + rust 2.71); the heaviest single language (TypeScript)
  is ~70 KB gz on demand — both well under the dossier's ~200 KB gz lazy threshold.

## 8. Refusals held (dossier "What NOT to bring")
No Monaco, no web-tree-sitter, no CodeMirror/Lezer, no SCIP/LSIF, no Prism/highlight.js,
no difftastic in the browser, no second highlighter. One donor (Shiki), the graph for
everything structural. In-file search + permalink-by-symbol are Fatia 2 (not built
here).
