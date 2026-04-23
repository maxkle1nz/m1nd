# Frequently Asked Questions

---

## General

### When should I use `search` instead of grep?

`search` is a graph-aware grep replacement. Use it when you need exact text search (literal or regex) but want graph context alongside the results — node IDs, line provenance, and graph-linked metadata.

The key differences:

| | grep | search |
|---|---|---|
| Searches file contents | Yes | Yes (literal + regex modes) |
| Searches graph node labels | No | Yes |
| Returns line context | With `-C` | Yes (built-in `context_lines`) |
| Scope filtering | glob/path | `scope` prefix |
| Semantic mode | No | Yes (delegates to `seek`) |
| Graph-linked results | No | Yes — every match includes `node_id` |

**Rule of thumb:**
- You know exactly what you're looking for (a function name, a string) → `search(mode="literal")`
- You know the pattern shape but not the exact text → `search(mode="regex")`
- You know what it DOES but not what it's called → `seek` or `search(mode="semantic")`
- You're exploring relationships, finding missing structure → `activate` or `missing`

`search` does NOT replace `activate`. `activate` uses spreading activation with 4D scoring — it surfaces what MATTERS, not what MATCHES. For open-ended exploration, `activate` is still superior.

---

### How does the savings and CO2 tracking work?

m1nd tracks token savings cumulatively across sessions via two tools: `savings` and `report`.

**Estimation model:**
- Every m1nd query (activate, impact, hypothesize, etc.) replaces a grep/glob + LLM re-reading loop
- Estimated avoided tokens per m1nd query: ~500 tokens (conservative)
- CO2 rate: 0.0002g CO2 per avoided token (based on LLM inference energy estimates)
- Cost rate: $0.003/1K tokens saved (based on Sonnet-class pricing)

**Example:** 100 m1nd queries in a session = ~50,000 tokens saved = 10g CO2 = $0.15 USD.

**`m1nd.savings`** shows the cumulative report with a formatted terminal display (ANSI-colored).
**`m1nd.report`** shows per-agent session statistics with a markdown summary ready for logging.

The tracking is honest about its limitations: it's an estimation, not an accounting system. The real value is directional — it makes the efficiency of graph-based search visible and comparable across sessions.

---

### What is m1nd's visual identity? What are the ⍌⍐⍂𝔻⟁ glyphs?

m1nd has a visual identity expressed through five Unicode glyphs and an ANSI color palette. These appear in `m1nd.help` output and the HTTP server GUI.

| Glyph | Meaning | Used for |
|-------|---------|---------|
| `⍌` (U+234C) | Spreading activation signal | Foundation tools, query output |
| `⍐` (U+2350) | Paths through the graph | Perspective navigation, trail system |
| `⍂` (U+2342) | Structural analysis | Superpowers, audit tools |
| `𝔻` (U+1D53B) | 4D dimensional scoring | Dimension labels, efficiency tools |
| `⟁` (U+27C1) | Graph connections, edges | Lock system, surgical tools |

**ANSI color palette:**
- Cyan `#00D4FF` — primary activation, Foundation category
- Gold `#FFD700` — Superpowers, Trails
- Magenta `#FF00FF` — Perspective, Extended tools
- Green `#00FF88` — Surgical, efficiency
- Red `#FF4757` — critical alerts, panoramic warnings
- Blue `#4169E1` — Lock system, Report

Run `help(tool_name="about")` to see the full visual identity banner in your terminal.

---

### How is this different from grep?

grep searches text. m1nd searches structure.

When you grep for a bug, you're looking for a text pattern that signals something is wrong. That works
well for bugs that leave a visible trace: a typo, a wrong variable name, a missing keyword.

m1nd works differently. It builds a weighted graph of your codebase — files, functions, classes, their
relationships, their co-change history — and then uses that graph to reason about what SHOULD exist but
doesn't. The bugs it finds best are the ones with no text trace at all: a missing lock, a missing
error handler, a missing validator. grep returns nothing for those. m1nd returns a ranked list of
structural holes.

In a live audit session on 2026-03-14 (380 Python files, ~52K lines), m1nd found 28 bugs using
46 queries in 3.1 seconds. 8 of those 28 bugs (28.5%) were structurally invisible to grep — they
could not have been found by any text search, because they exist as absences, not presences.

See [Use Cases](Use-Cases) for the full comparison table.

---

### How is this different from Sourcegraph?

Sourcegraph is an excellent tool for code navigation and text search at scale. The comparison:

| Capability | Sourcegraph | m1nd |
|------------|-------------|------|
| Text search across repos | Yes | No (use grep) |
| Code navigation | Yes (SCIP static graph) | Yes (dynamic weighted graph) |
| Learns from use | No | Yes (Hebbian plasticity) |
| Persists investigations | No | Yes (Trail system) |
| Tests hypotheses | No | Yes (89% accuracy) |
| Simulates module removal | No | Yes (Counterfactual engine) |
| Finds what's MISSING | No | Yes (Structural holes) |
| Race condition detection | No | Yes (Flow simulation) |
| Bug propagation prediction | No | Yes (SIR epidemic model) |
| Cost per query | Hosted SaaS | Zero (local binary) |

They solve different problems. If you need "find all usages of this function across 50 repos,"
use Sourcegraph. If you need "find the bugs that don't show up in any search result," use m1nd.

---

### How is this different from CodeScene?

CodeScene focuses on behavioral code analysis — identifying hotspots, technical debt, and team
patterns from git history. It's a dashboard tool for managers and architects.

m1nd is a query engine for agents and developers. The differences:

- **CodeScene**: web dashboard, team-level metrics, organizational patterns
- **m1nd**: MCP tools, query-per-query analysis, single-session investigations

Both use co-change history. m1nd uses it as one signal among four (structural, semantic, temporal,
causal). CodeScene uses it as the primary signal.

m1nd adds capabilities CodeScene doesn't have: hypothesis testing, flow simulation, antibody
scanning, trail persistence, and the memory adapter for code+docs unified search.

---

### What does "the graph learns" mean?

Every time you call `learn(feedback="correct")`, m1nd runs Hebbian Long-Term Potentiation (LTP) on
the edges that led to the result you confirmed. Edge weights along those paths increase. Next time
you run a similar query, the graph activates those paths faster and ranks them higher.

When you call `learn(feedback="wrong")`, Hebbian Long-Term Depression (LTD) runs instead. Those
paths weaken.

Over time, the graph adapts to how your team thinks about your codebase. If your team calls the
authentication layer "auth" sometimes and "identity" other times, the graph learns that those
concepts co-activate. Future queries for either term surface both regions.

Weights have a floor (0.05) and a cap (3.0), and homeostatic normalization prevents runaway
reinforcement. The graph won't over-optimize on a single path.

No other code intelligence tool does this. Static analysis graphs are frozen snapshots. RAG
embeddings don't strengthen from use. m1nd's graph evolves.

---

### How much memory does it use?

The graph lives in RAM. Rough rule of thumb:

- **10K nodes / 26K edges** (335-file Python backend): ~2MB
- **20K nodes** (82 docs + code merged): ~4MB
- **100K nodes**: ~20MB estimated

For most codebases (up to ~200K files), memory is not a constraint. If you have 400K+ files, it
works but it's not where m1nd was optimized.

The binary itself is ~8MB. Runtime cost per query is measured in microseconds to milliseconds —
you will not notice any CPU impact during normal use.

---

### Does it work with docs, specs, PDFs, and office files?

Yes. The current runtime is no longer limited to code + memory markdown.

You can ingest:

- markdown notes and project docs
- L1GHT protocol specs
- HTML/wiki pages
- office documents
- scholarly PDFs
- structured document feeds such as patent/article/BibTeX/CrossRef/RFC sources

The universal lane normalizes those sources into canonical local artifacts and exposes a second layer of document tools:

- `document_resolve`
- `document_bindings`
- `document_drift`
- `document_provider_health`
- `auto_ingest_start`, `auto_ingest_status`, `auto_ingest_tick`, `auto_ingest_stop`

So the answer is now “code, docs, specs, and papers on one local graph substrate,” not only “code plus memory adapter.”

---

### Does it work with my language?

m1nd ships extractors for 27+ languages across four tiers:

| Tier | Languages | How to enable |
|------|-----------|---------------|
| Built-in | Python, Rust, TypeScript/JavaScript, Go, Java (5) | Default build |
| Generic fallback | Any language with `def`/`fn`/`class`/`struct` patterns | Default build |
| Tier 1 (tree-sitter) | C, C++, C#, Ruby, PHP, Swift, Kotlin, Scala, Bash, Lua, R, HTML, CSS, JSON (14) | `--features tier1` |
| Tier 2 (tree-sitter) | Elixir, Dart, Zig, Haskell, OCaml, TOML, YAML, SQL (8) | `--features tier2` (default) |

```bash
# Full language support
cargo build --release --features tier1,tier2
```

If your language isn't in any tier, the generic fallback still extracts function/class shapes and
builds a partial graph. You lose import edge resolution, but structural queries still work.

---

### Can I use it without an LLM?

Yes. m1nd is a pure Rust binary. It runs entirely locally with no LLM calls, no API keys, and no
network connections. The MCP server communicates over stdio (JSON-RPC), so it connects to whatever
MCP client you use — but the graph engine itself is self-contained.

The tools that benefit most from an LLM are the ones where you describe a query in natural language
(`activate`, `hypothesize`, `missing`). These still work without an LLM — the query is processed
using trigram matching and spreading activation, not an LLM. An LLM adds interpretability (it can
explain *why* the graph returned these results) but isn't required for the graph to function.

---

## Specific Features

### What are antibodies?

Antibodies are stored structural patterns — subgraph signatures extracted from confirmed bugs.

When you confirm a bug with `learn(feedback="correct")`, m1nd can extract a topological fingerprint
from the subgraph around that bug: a pattern like `[async_function] → [except_handler] → [break]`
without a `[raise]` edge. That pattern becomes an antibody.

On every future `ingest`, m1nd scans new/changed code against all stored antibodies. If the same
structural shape appears in new code, it matches — and you know to investigate that code immediately,
before the bug reaches production.

Known bug shapes recur in 60-80% of codebases. Once you've fixed a race condition in one part of
your system and captured the antibody, you can automatically detect the same race condition anywhere
else it appears — today or three months from now.

Use `antibody_create`, `antibody_scan`, and `antibody_list` to manage the antibody registry.
See [API Reference](API-Reference) for details.

---

### How does hypothesis testing work?

`hypothesize` takes a natural language claim about your codebase and tests it against graph structure.

```jsonc
{
  "tool": "hypothesize",
  "claim": "session_pool leaks sessions when requests are cancelled",
  "depth": 8
}
// → likely_true, 99% confidence, 25,015 paths analyzed, 3 supporting evidence groups
```

Under the hood: m1nd parses the claim to extract entities and relationships, then runs a
path-exploration algorithm across the graph. It looks for structural paths that SUPPORT the claim
(positive evidence) and paths that CONTRADICT it (negative evidence). The confidence score is a
Bayesian combination of both.

Empirically validated across 10 live claims on a production codebase: 89% accuracy. The tool
correctly confirmed session_pool leak at 99% confidence (real bug, later fixed), correctly
rejected a circular dependency hypothesis at 1% (clean path, no bug), and discovered a new
unpatched bug mid-session at 83.5% confidence.

The tool does NOT read source code. It reasons purely from graph structure. When it says 99%
confidence, that means 25K structural paths converge on the same conclusion. That's not a guess.

---

### What is flow simulation? How does it detect race conditions?

`flow_simulate` runs an agent-based simulation on the graph. "Particles" (hypothetical concurrent
requests or tasks) enter at an entry point and propagate along causal edges through the graph.

Nodes with `asyncio.Lock`, `threading.Lock`, or similar synchronization primitives are treated
as "valves" — they serialize particle flow. Nodes that receive more than one particle simultaneously
WITHOUT a valve = **turbulence detected** = potential race condition.

```jsonc
{ "tool": "flow_simulate", "entry": "settings_routes", "particles": 4, "max_depth": 8 }
// → 51 turbulence points, 11 valves
// A turbulence point = a node that two concurrent requests can both reach simultaneously
// without passing through a lock. That's the definition of a race condition site.
```

Real results from the 2026-03-14 audit:
- **Core system (after fixes): 0 turbulence** — that's the target state
- **Settings subsystem: 51 turbulence points** — significant race condition surface
- **WhatsApp subsystem: 223 turbulence points** — highest-risk area in the system (4x the next highest)

The WhatsApp finding correctly identified that subsystem as requiring a feature hold. The decision
was made in under 3 minutes without reading a single file.

---

### What is the Trail system?

Trails are saved investigation states. When you're mid-investigation and need to end the session —
or hand the investigation to another agent — you call `trail_save`. m1nd serializes the full
activation state of the graph: which nodes are activated, which paths were explored, any hypotheses
you've tested, the current Hebbian weights.

When you resume (`trail_resume`), the graph restores to exactly that state. Not approximately —
exactly. You continue from where you left off with no re-reading or re-querying.

Multiple agents can investigate independently and then merge their trails (`trail_merge`). The
merge step detects where independent investigations converged on the same nodes and flags cases
where they reached conflicting conclusions.

---

### What is the Memory Adapter?

The Memory Adapter lets you ingest `.md`, `.txt`, and `.markdown` files into the same graph as your
code. Headings become nodes. Bullet entries become nodes. Cross-references produce edges.

After ingesting both code and docs with `mode="merge"`, a single query returns results from both:

```jsonc
{ "tool": "activate", "query": "antibody pattern matching" }
// → PRD-ANTIBODIES.md (score 1.156) — the spec
// → pattern_models.py (score 0.904) — the implementation
// One query. Both documents. Automatically cross-linked.
```

You can also find gaps: `missing("GUI web server")` returns specifications that exist WITHOUT a
corresponding implementation in the code graph.

This is the "sleeper feature." Most tools build separate indexes for code and docs. m1nd treats
them as a unified knowledge graph.

Empirically tested: 82 docs (PRDs, specs, notes) ingested in 138ms → 19,797 nodes, 21,616 edges.
Cross-domain queries working immediately.

---

### How does `verify=true` work in `apply_batch`?

`apply_batch` accepts a `verify` flag (default: `false`). When set to `true`, m1nd runs a 5-layer
verification pass on every file after writing it, before returning results.

**The 5 verification layers:**

1. **Syntax check** — The file is parsed by the language extractor. A parse failure (SyntaxError,
   unclosed brace, invalid token) immediately stops the batch and returns `verdict: "BROKEN"`.

2. **Import resolution** — All `import` / `use` / `require` statements are checked against the
   graph. Unknown imports that weren't present before = likely copy-paste error or missing dependency.

3. **Anti-pattern scan** — The file is checked against all stored antibodies. A match means the
   new code contains a structural signature of a previously confirmed bug.

4. **Co-change prediction** — `predict()` runs automatically on each written file. Files that were
   statistically likely to change together but were NOT included in the batch are listed as
   `co_change_warnings`. Not a blocker — informational.

5. **Graph coherence** — The re-ingested subgraph is checked for edge anomalies: dangling
   references, bidirectional edges that should be unidirectional, cycle introduction in acyclic
   regions.

**Verdicts:**
- `CLEAN` — All 5 layers passed. Safe to proceed.
- `WARNING` — Passed with informational notices (co-change gaps, new imports). Review suggested.
- `BROKEN` — Hard failure on layers 1, 2, or 3. The write is committed but flagged for immediate
  attention. See "What does BROKEN verdict mean?" below.

---

### Does `verify=true` slow down `apply_batch`?

Yes, by approximately **200–400ms** per batch (not per file — the overhead is largely fixed cost
regardless of batch size).

**Breakdown:**
- Syntax parsing: ~5–20ms per file (depends on file size and language)
- Import resolution: ~10ms (graph lookup)
- Antibody scan: ~2.68ms per 50 patterns (from criterion benchmarks)
- Co-change prediction: ~1ms per file
- Graph coherence: ~50–100ms (BFS traversal on written nodes)

For a 5-file batch, total verification overhead is typically 200–350ms. For a 20-file batch,
roughly 350–600ms.

**Is it worth it?**

Yes, in almost every case. The verification layer catches silent bugs — code that writes
successfully but introduces a structural regression that would only appear at runtime. In the
2026-03-14 audit session, 3 out of 28 confirmed bugs would have been caught by `verify=true`
at write time rather than requiring a follow-up hypothesize pass.

Disable it only for high-frequency incremental writes where you're certain about correctness
(e.g., generated boilerplate, scaffolding). For real implementation code, always use `verify=true`.

---

### What does `BROKEN` verdict mean?

A `BROKEN` verdict from `apply_batch` with `verify=true` means one of three things:

**1. Compile / parse error (Layer 1)**
The written file fails language-level parsing. Examples: mismatched braces, invalid syntax,
unclosed string literal, Python IndentationError. The exact error and line number are included
in the verdict detail.

**2. Test failure (Layer 2 — if tests are configured)**
If the project has a test runner configured (`pytest`, `cargo test`, `go test`), m1nd can
optionally run the test suite scoped to the affected files. A test failure produces `BROKEN`.
This is opt-in via `run_tests: true` in the batch parameters.

**3. Anti-pattern detected (Layer 3)**
The new code matches a stored antibody — a structural fingerprint extracted from a previously
confirmed bug. This is the most powerful case: the code compiles perfectly and may even pass
tests, but it reproduces a known structural bug pattern.

**What to do on BROKEN:**

```jsonc
// 1. Read the verdict detail — it includes which layer failed and why
// 2. If Layer 1 (syntax): fix the parse error, re-apply
// 3. If Layer 3 (antibody): inspect the matching pattern
{ "tool": "antibody_list", "agent_id": "dev" }
// → Find the antibody that matched. Read its description. Fix the structural issue.
// 4. Re-apply the corrected batch
{ "tool": "apply_batch", "edits": [...], "verify": true, "agent_id": "dev" }
```

A `BROKEN` verdict is not a catastrophic failure — the file was still written. It's a signal
to pause, diagnose, and fix before continuing. Treat it like a failed test in CI: required to
resolve before proceeding.

---

## Cross-Domain Features

### What is `flow_simulate` and how does it detect race conditions?

`flow_simulate` runs an agent-based simulation on the graph. Particles (hypothetical concurrent
requests) enter at an entry point and propagate through causal edges. Nodes that receive multiple
particles simultaneously without a synchronization valve (lock/mutex) are flagged as turbulence.

Turbulence = the structural signature of a race condition site.

Real results from the 2026-03-14 production audit:
- **Core system (after fixes): 0 turbulence** — the correct target state
- **Settings subsystem: 51 turbulence points** — significant race surface
- **WhatsApp subsystem: 223 turbulence points** — feature hold decision in under 3 minutes

See [Use Cases § 6.2](Use-Cases#62-race-condition-detection--flow-simulation) for the full pipeline.

---

### What is the epidemic model?

`epidemic` runs a SIR (Susceptible-Infected-Recovered) model on the graph. You mark recently
modified modules as "infected" and the model spreads infection through structural connections,
estimating R₀ (reproduction number) and the likely blast radius.

R₀ > 1 means the bug is "spreading" through the graph — more modules are likely affected than
have been reviewed so far. Use epidemic output to prioritize the next review sprint.

---

### What is the antibody system?

The antibody system is immune memory for your codebase. When you confirm a bug, m1nd extracts
a structural fingerprint (subgraph pattern) from the bug site. Future `ingest` calls scan new
and changed code against all stored antibodies.

If new code matches a known bug pattern, it's flagged immediately — before the PR merges.
Known bug shapes recur in 60-80% of codebases.

Tools: `antibody_create`, `antibody_scan`, `antibody_list`. See [API Reference](API-Reference).

---

### What does `tremor` measure?

`tremor` computes second-derivative acceleration on edge weight time series. Velocity = how fast
a module is changing. Acceleration = whether that velocity is increasing.

High acceleration = instability building. A module with rising velocity acceleration is the
highest-probability location for the next bug. `tremor` surfaces these before they break.

Run it nightly. Post results to a dashboard. Alert if acceleration rises more than 20%
week-over-week.

---

### What does `trust` measure?

`trust` computes per-module defect density from historical `learn()` events. Every confirmed bug
increments that module's defect count. The trust score is a Bayesian estimate of the probability
that a future change introduces a bug.

Use trust scores to route PR review: low-trust modules get mandatory senior review; high-trust
modules get fast-track merge. Data-driven, not intuitive.

---

### What does `layers` detect?

`layers` auto-detects architectural layers using Tarjan SCC + BFS depth — no configuration
needed. It then counts violations: paths that skip a layer.

Real result from production audit: `score 0.0` (zero layer separation) + 13,618 violations.
This was the structural evidence for why the WhatsApp subsystem had 223 turbulence points —
it was bypassing the validation layer entirely.

Layer violations are architectural risk. Track the violation count over time. Rising count
= architecture degrading. Use `layer_inspect` to find the worst offenders.

---

## HTTP Server and GUI

### Does m1nd have a web UI?

Yes. Build with `--features serve` and start with `--serve`:

```bash
cargo build --release --features serve
./m1nd-mcp --serve --open   # opens browser automatically
```

The HTTP server starts on port 1337 by default. The React UI is compiled directly into the
binary via `rust-embed` — no separate frontend installation required.

For development with Vite HMR:
```bash
./m1nd-mcp --serve --dev   # serves UI from disk at ../m1nd-ui/dist/
```

---

### What API does the HTTP server expose?

| Route | Purpose |
|-------|---------|
| `GET /api/health` | Server health, uptime, node/edge counts |
| `GET /api/tools` | List all 77 tool schemas |
| `POST /api/tools/{tool_name}` | Execute any MCP tool via HTTP |
| `GET /api/graph/stats` | Graph statistics |
| `GET /api/graph/subgraph?query=...&top_k=N` | Activation-based subgraph for visualization |
| `GET /api/graph/snapshot` | Full graph export (all nodes + edges) |
| `GET /api/events` | Server-Sent Events stream (tool results, timeouts) |

The HTTP API uses the same `dispatch_tool()` function as the stdio JSON-RPC transport — all 61
tools are available over HTTP with identical behavior.

---

### Can I run stdio and HTTP simultaneously?

Yes. Use `--serve --stdio`:

```bash
./m1nd-mcp --serve --stdio
```

This runs both transports on the same process with the same shared graph state. Tool calls from
stdio are broadcast as SSE events to HTTP subscribers — useful for building dashboards that
observe Claude Code or other MCP clients in real time.

For cross-process visibility (two separate processes), use `--event-log`:

```bash
# Process 1: stdio client (Claude Code)
./m1nd-mcp --stdio --event-log /tmp/m1nd_events.jsonl

# Process 2: HTTP server watching the event log
./m1nd-mcp --serve --watch-events /tmp/m1nd_events.jsonl
```

---

## Open Source and Community

### Is it open source? What license?

Yes. m1nd is MIT licensed. See [LICENSE](https://github.com/cosmophonix/m1nd/blob/main/LICENSE).

MIT means: use it commercially, modify it, distribute it, integrate it — with attribution. No
copyleft requirement, no usage restrictions.

---

### How do I contribute?

m1nd is early-stage and moving fast. Contributions are welcome in four areas:

**Language extractors** — Add tree-sitter parsers for additional languages in `m1nd-ingest`.
The extractor interface is straightforward: implement the `LanguageExtractor` trait.

**Graph algorithms** — Improve spreading activation, add community detection, or propose new
analytical methods. The graph engine lives in `m1nd-core`.

**New MCP tools** — If you have a use case that doesn't fit the current 77 tools, open an issue.
The MCP layer in `m1nd-mcp` is thin: most new tools are 20-50 lines of wiring.

**Benchmarks** — Run m1nd against your codebase and report performance. We want to understand
where it works well and where it doesn't.

See [CONTRIBUTING.md](https://github.com/cosmophonix/m1nd/blob/main/CONTRIBUTING.md) for
guidelines on pull requests, testing, and commit style.

---

### How do I report a bug?

Open an issue on GitHub. Include:

1. Your m1nd version (`m1nd-mcp --version` or the crate version from `Cargo.toml`)
2. Your OS and Rust version
3. The minimal reproduction: which tool, what input, what you expected, what happened
4. The full output (or a sanitized version if it contains sensitive paths)

For security issues, please do not open a public issue. Email the maintainer directly.

---

## Performance and Limits

### How long does indexing take?

Real measurements from production use:

| Scale | Time |
|-------|------|
| Single file (incremental) | 0.07ms |
| 82 markdown docs | 138ms |
| 335 Python files (~52K lines) | 910ms |
| 380 Python files (~52K lines) | 1.3s |
| 2 repos federated (11K+ nodes) | 1.3s |

After the initial ingest, use `incremental: true` on subsequent ingests — it only re-indexes
changed files. For a single-file change, that's under 1ms.

---

### Does it work on Windows?

m1nd is pure Rust with no platform-specific dependencies. It compiles on Linux, macOS, and Windows.
The binary communicates over stdio (JSON-RPC), so the MCP client integration is the same on all
platforms.

There are no known Windows-specific bugs, but Windows is less tested than Linux/macOS. If you
encounter issues, please report them.

---

### Can multiple agents use it simultaneously?

Yes. The MCP server handles multiple concurrent agents with atomic graph writes. When two agents
call `ingest` or `learn` simultaneously, writes are serialized without corrupting the graph state.

For parallel builds where multiple agents work on different subsystems, use the Lock system
(`lock_create`) to partition the graph. Each agent locks its region — other agents can read but
won't overwrite. After finishing, call `lock_diff` (0.08µs) to see exactly what changed, then
`lock_release`.

Real session data: 16 Sonnet agents built in parallel using m1nd for coordination with zero
graph state corruption.
