# m1nd Reader — Donor Research & Advanced Viewer v1 Design

> Research deliverable. Repo read-only. Question: which OSS projects can DONATE
> capability to the m1nd-ui "Show Code" viewer, and how to shape an advanced
> reader v1 that spends the house's real trunfo — the code graph — instead of
> reinventing it. Web research: 4 searches + 1 fetch (Shiki, CM6/Lezer,
> web-tree-sitter, difftastic, Shiki best-performance).

---

## 0. Ground truth (what exists today, verified in-repo)

**The viewer today** (`m1nd-ui/src/components/map/ShowCode.tsx`, `hooks/useFileView.ts`):
a read-only modal, four tabs (Files / Tests / Receipts / Impact). The actual code
render is **raw text in a `<pre className="… whitespace-pre">`** (ShowCode.tsx:98-102)
— **no highlight, no line numbers, no folding, no in-file search, no symbol outline,
no navigation.** Files are grouped by role in a left rail; a right rail shows
Health (files/tests/receipts/runtime + state). File bytes come from
`GET /api/file?path=…&brain=…` (`api/client.ts:436`), capped ~256 KB with an honest
`truncated`. Globs are honestly deferred. "Open in editor" is disabled (needs a
local editor bridge). Impact tab already renders the block's **declared sockets**
(structural neighbours) — not a computed blast radius.

**The trunfo — the graph is ALREADY the navigation substrate** (confirmed in
`api/types.ts` + `lib/snapshot.ts` + fixtures):
- `GET /api/graph/snapshot` (and `/api/graph/subgraph?query=`) serve nodes+edges.
- **Every node carries `provenance { source_path, line_start, line_end, namespace,
  canonical }`** (`lib/snapshot.ts:24-40`). node_type ∈ File/Directory/**Function/
  Class/Struct/Enum/Type/Module**/Reference/Concept.
- node_id encodes location+kind+name: e.g. `file::m1nd-core/src/graph.rs::struct::Graph`
  (seek.json fixture, line_start 426). It is a **stable per-symbol anchor**.
- **Edges** carry `source_id → target_id`, `relation` (call/use/import/def…), `weight`,
  `direction`, `causal_strength` (`lib/snapshot.ts:42-50`).
- Graph tools already exposed over REST: `impact`, `trace`, `why`, `seek`, `focus`,
  pagerank/`graph_activation` (used in seek scoring).

**Consequence, and the spine of this design:** the outline of a file, click-to-
definition, fold ranges, and freshness-per-symbol are all **derivable from the graph
with zero new parser in the browser** — filter snapshot nodes by
`provenance.source_path === file`, sort by `line_start` (outline + fold ranges);
follow a symbol node's outgoing call/use edge to `target.provenance.source_path +
line_start` (click-to-def); read `last_modified` / `change_frequency` + block receipts
(freshness). **This is the capability no market viewer (Monaco, Sourcegraph, GitHub)
has for free — they need an LSP/LSIF index; m1nd already owns it.**

**Languages m1nd indexes** (`m1nd-ingest/src/lib.rs:160-165` + `Cargo.toml`):
dedicated symbol extractors for **Rust, TS/JS (regex), Python (regex), Go (regex)**;
plus **native tree-sitter 0.26** grammars for ~20 more (C, C++, C#, Ruby, PHP, Swift,
Kotlin, Scala, Bash, Lua, R, HTML, CSS, JSON, Elixir, Dart, Zig, Haskell, OCaml,
TOML, YAML, SQL). **Honest gap (PATHOS Known Problems):** *method-call edges exist for
Rust, not yet TS/Java/Go/Python* — so click-to-def is Rust-rich and degrades honestly
elsewhere (def/import/use edges still navigate; ambiguous receiver → show candidates
or abstain, never fake a jump). **All this parsing lives in the Rust owner** — the
browser gets the result, never re-parses.

**House laws that bind every donor** (`docs/PATHOS.md`, `index.css`):
1. **"the served UI must work air-gapped; no CDN fonts"** — ZERO runtime external
   requests. Fonts self-hosted in `public/fonts/`. Any donor must bundle/self-host;
   any wasm must be same-origin like the fonts.
2. **Calm paper/ink, "nothing glows"** — porcelain/bone/ink palette, IBM Plex Mono for
   code, matte shadows, no emission gradients. **Violet (#7c3aed) is QUARANTINED to
   abstain/unknown** and enforced by `scripts/violet-lint.mjs`. → A highlighter theme
   MUST be a custom calm theme; stock dark/neon themes would violate the house and the
   lint.
3. **Honesty** — truncation, absence, and ambiguity are shown, never faked.
4. **Reuse-first (CLAUDE.md mother rule)** — the graph > any external donor wherever
   they would compete.

---

## 1. Donors evaluated

Verdict legend: **TRAZER** (fatia 1) · **DEPOIS/DEPENDE** (later, gated on a proven
need) · **NÃO** (reference only / competes with the graph).

| Donor | Capability it would donate | License | Honest weight | Verdict | Why (does it pay rent?) |
|---|---|---|---|---|---|
| **Shiki** (JS engine, fine-grained) | Real syntax highlight over the existing `<pre>`; TextMate grammars; JSON themes | MIT | **Wasm-free** via `@shikijs/engine-javascript` (97.2% of langs); `shiki/core` + N `@shikijs/langs/*` + 1 custom theme ≈ **~50–150 KB gz** for a handful of langs (AVOID the 6.4 MB/1.2 MB full bundle). Lazy-load grammar per file. | **TRAZER (fatia 1)** | Highlight is the one capability the graph does NOT give. JS engine = **no wasm** (honors "no CDN"); TextMate theme is JSON → a calm paper/ink theme is trivial and lints clean; decorates the current `<pre>` with the smallest surface. Pays rent by pure capability the house lacks. |
| **CodeMirror 6 + Lezer** | Full read-only viewer: gutters/line-numbers, **native folding + in-file search + line virtualization**, selection | MIT | Editor framework: `basic-setup` ~**93 KB gz**; trimmed read-only less, +per-lang Lezer grammars (fewer langs than TextMate). | **DEPOIS / DEPENDE** | Real, but most of what it buys (folding ranges, outline) **the graph already provides**. Adopt ONLY if large-file **virtualization** + rich in-file search prove that hand-rolling them over Shiki gets *uglier* than CM6 (the reuse-first "cleanly" test). Not fatia 1 — its editor machinery doesn't pay rent yet. |
| **web-tree-sitter (wasm)** | In-browser AST parsing for outline/highlight | MIT | tree-sitter.wasm + **one .wasm per grammar** (~hundreds of KB each); self-hostable, works offline. | **NÃO (por ora)** | **Redundant with the server.** The Rust owner already parses every file into the graph (tree-sitter 0.26 + extractors). Re-parsing in the browser for STRUCTURE duplicates the trunfo. Highlight doesn't need full ASTs (Shiki tokenizes cosmetically). Reconsider only if graph-driven + TextMate highlight both prove insufficient. |
| **difftastic** | Structural (syntax-aware) diff | MIT (vendored parsers MIT/Apache) | Rust crate `difftastic-lib`; **no browser/wasm build** as of 2026. | **DEPOIS (SERVER-SIDE)** | Right capability, wrong layer for the browser. When structural diff lands, run it **in the owner** (already has tree-sitter+grammars) and serve it via REST — mirrors the ingest architecture. **Never bundle in the browser.** |
| **highlight.js / Prism** | Baseline regex highlight | MIT | Small (~tens of KB), no wasm. | **NÃO** | Superseded by Shiki's JS-engine path (better fidelity, already wasm-free). No reason to carry a second highlighter. Mental fallback only if Shiki startup cost ever surprises. |
| **Monaco** | VS Code editor in the browser | MIT | **~2–5 MB**, ships web workers, dark-editor gravity. | **NÃO (nunca, p/ leitor)** | Overkill for read-only; fights the calm aesthetic and the air-gap/bundle law. The exact anti-pattern. |
| **Lezer (standalone)** | Incremental parser | MIT | small core + per-lang grammar | **NÃO (chega junto do CM6)** | Only meaningful as CM6's engine; no standalone need — the graph is the structure source. |
| **SCIP / LSIF** | Cross-ref index *format* | Apache-2.0 / MIT | format, not a bundle | **NÃO (referência)** | **The m1nd graph IS the xref index.** Adopting SCIP would stand a parallel truth beside the graph — the classic competing-source anti-pattern. Study the *format* for edge-shape ideas; import nothing. |
| **zoekt** | Trigram code search (server) | Apache-2.0 | server binary | **NÃO** | In-file search is trivial (fatia 2); cross-repo search is already `seek`/`search` over the graph. |
| **ast-grep** | Structural search/lint UX | MIT | Rust CLI/lib | **NÃO (referência de UX)** | Inspiration for a future structural-search UX; if it lands it's a server-side tool over the graph/tree-sitter, not a browser dep. |

---

## 2. The advanced reader v1 — slices ordered by value/cost

**The hypothesis is largely VALIDATED, with three sharpenings** (below). The organizing
principle: **the graph is the brain of the reader; the donor is only the paint.** Every
capability that is *structure* comes from `/api/graph/*`; the single donor (Shiki) does
the one thing the graph cannot — make text legible.

### Fatia 1 — the legible, navigable, honest file (highest value / lowest cost)
The mission's fatia 1, adjudicated and made precise:

1. **Real highlight = Shiki, JS engine, custom paper/ink theme** (NOT CM6, NOT wasm).
   Decorate the existing `<pre>` in `Viewer` (ShowCode.tsx:83-104). Author one TextMate
   theme in the house tokens (ink/ink-soft/socket-blue for structure; violet stays
   quarantined — comments/strings in muted paper tones, "nothing glows"). Lazy-load the
   grammar for the file's language; unknown extension → plain text, honestly.
2. **Symbol outline FROM THE GRAPH** (not a new parser): query the file's symbol nodes
   (`provenance.source_path === path`, node_type ∈ Function/Class/Struct/Enum/Type/
   Module), sort by `line_start`. Render as a per-file outline rail (reuse the existing
   "Files by role" rail slot). Clicking a symbol scrolls the `<pre>` to its `line_start`.
3. **Click-to-definition FROM THE EDGES** (not an LSP): a symbol/token's outgoing
   call/use/def edge → resolve `target_id` → jump to `target.provenance.source_path +
   line_start`, loading that file via the same `/api/file`. **Honest degradation:**
   Rust gets call-edge jumps; TS/Python/Go get def/import/use jumps; ambiguous receiver
   → present candidates (or abstain) — **never a fabricated jump** (matches m1nd's
   absent/abstain doctrine).
4. **Receipts + freshness inline per symbol:** decorate each outline entry with the
   owning block's receipt state (already in `rollup`) + node `last_modified` /
   `change_frequency`. This is the "who vouches for this symbol, how fresh" line —
   again, **a market viewer cannot draw this**; m1nd can, for free.

*Cost:* one MIT dep (Shiki, wasm-free), one custom theme, and client glue over data the
owner already serves. No new backend if the reader filters the snapshot client-side; a
thin `GET /api/file/symbols?path=` owner view is the clean optimization if per-file
filtering of a 9k–14k-node snapshot proves wasteful (see Risks).

### Fatia 2 — folding, in-file search, permalink-by-symbol
1. **Folding = graph fold ranges.** Each symbol node's `[line_start, line_end]` is a
   collapsible region — fold ranges come from the graph, **likely without CM6**.
2. **In-file search** — trivial client-side (match + scroll + count), calm styling.
3. **Permalink-by-symbol** — the node_id (`file::path::kind::Symbol`) is already a
   stable anchor; deep-link Show Code to a symbol. Near-zero cost (reuse existing ids).

   *CM6 re-enters the decision ONLY here* — if (a) files routinely exceed what a full
   Shiki DOM renders smoothly and virtualization is needed, or (b) search/fold hand-roll
   turns uglier than adopting CM6. Decide on measured pain, per the reuse-first "cleanly"
   test — not by default.

### Fatia 3 — structural diff + blame (later, mostly server-side)
1. **Structural diff = difftastic in the OWNER, served via REST** (never a browser
   bundle). Reuses the owner's tree-sitter + grammars; the browser renders the result.
2. **Blame** — a `git blame` read behind the same read-only file route, decorating the
   gutter. Gated on a proven need; no donor required.

---

## 3. What NOT to bring, and why (anti-bloat — the graph > the donor)

- **NÃO web-tree-sitter in the browser** — re-parsing for structure duplicates the graph
  (the owner already ran tree-sitter). The browser needs *paint*, not a second AST.
- **NÃO SCIP/LSIF** — the graph is the xref index. Importing an index format stands a
  competing source of truth beside the trunfo. Reference the format, import nothing.
- **NÃO Monaco** — 2–5 MB, own workers, dark-editor gravity: violates air-gap/bundle law
  and the calm aesthetic. Read-only never needs it.
- **NÃO a second highlighter** (Prism/highlight.js) once Shiki is in — one paint layer.
- **NÃO zoekt / ast-grep as browser deps** — search is `seek`/`search` + trivial in-file
  match; structural search, if ever, is a server tool over the graph.
- **NÃO stock Shiki themes** — author the paper/ink theme; enforce with violet-lint.
- **Where graph and donor compete, the graph wins:** outline, navigation, folding
  ranges, xref, freshness = graph. The donor's *only* job is tokenized color.

---

## 4. Risks

1. **Bundle honesty.** Shiki's *full* bundle is 6.4 MB / 1.2 MB gz — the thing we AVOID.
   Fine-grained core + JS engine + N langs + 1 theme should land ~50–150 KB gz, but
   **MEASURE at integration** and lazy-load grammars per file language. Guard against
   accidentally pulling `shiki` (monolithic) instead of `shiki/core`.
2. **No wasm at fatia 1** — the JS engine removes the wasm-loading/self-host risk
   entirely. If an edge language in the 2.8% ever needs the Oniguruma engine, its
   `.wasm` MUST be self-hosted from `public/` (like the fonts) — same-origin, no CDN.
3. **Two independent language sets.** Shiki TextMate grammars ≠ the languages m1nd
   extracts symbols for. Keep ONE source-of-truth map `ext → { shikiGrammar?,
   hasGraphOutline? }` and degrade honestly: highlight-without-outline, or
   plain-text-without-highlight for unknown extensions. Never imply a symbol map that
   isn't there.
4. **Click-to-def coverage is uneven (PATHOS gap).** Method-call edges are Rust-rich;
   TS/Java/Go/Python lack them today. The UI must degrade honestly (candidates/abstain),
   and this is a REASON the reader improves as the graph improves — not a blocker.
5. **Large-file rendering / virtualization.** 256 KB ≈ thousands of lines; a full
   Shiki-tokenized DOM can get heavy (today's `<pre>` already dumps the whole capped
   file, so this is not new — Shiki only adds spans). Mitigate with windowed
   highlighting; if it bites, that is the CM6 (fatia 2) trigger.
6. **Snapshot payload.** `/api/graph/snapshot` is 9k–14k nodes. The reader should NOT
   re-fetch it per file — filter the already-loaded snapshot client-side, or add a thin
   `GET /api/file/symbols?path=` owner view (a real, small backend slice) if profiling
   shows the client filter is hot.
7. **Theme/lint compliance.** The custom theme must pass `violet-lint` + `icon-lint` and
   the "nothing glows" bar. Build the theme from the existing CSS tokens, not from a
   stock palette.

---

## 5. Sources
- Shiki: https://shiki.style/guide/best-performance · https://shiki.style/guide/bundles · https://shiki.style/blog/v2 · https://github.com/shikijs/shiki (MIT)
- CodeMirror 6 / Lezer: https://lezer.codemirror.net/ · https://github.com/codemirror/dev/issues/760 (MIT)
- web-tree-sitter: https://www.npmjs.com/package/web-tree-sitter · https://github.com/tree-sitter/tree-sitter/tree/master/lib/binding_web (MIT)
- difftastic: https://github.com/Wilfred/difftastic · https://crates.io/crates/difftastic-lib (MIT)
- In-repo: `m1nd-ui/src/components/map/ShowCode.tsx`, `hooks/useFileView.ts`, `api/client.ts`, `lib/snapshot.ts`, `api/types.ts`, `m1nd-ingest/src/lib.rs`, `m1nd-ingest/Cargo.toml`, `docs/PATHOS.md`, `m1nd-ui/src/index.css`.
