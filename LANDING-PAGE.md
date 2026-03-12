# m1nd Landing Page Copy

Structured for direct implementation. Each section includes content, visual direction, and implementation notes.

---

## HERO SECTION

**Headline:**
Your Agent's Nervous System for Code

**Subheadline:**
Spreading activation on knowledge graphs. 13 MCP tools that let LLM agents navigate, predict, and learn from any codebase -- in Rust.

**CTA Button:**
`View on GitHub`

**Secondary CTA:**
`Read the Integration Guide`

**Hero Visual Direction:**
Animated property graph. Dark background. Nodes are labeled with real code symbols (auth.rs, SessionManager, handle_login). User types a query -- "authentication flow" -- and a wavefront of light propagates outward from the seed node, decaying in intensity at each hop. Nodes that score high pulse brighter. One node in the periphery blinks red: structural hole detected. No particle effects, no 3D. Clean 2D graph with weighted edges visible. The animation should feel like watching a circuit board light up, not a marketing demo.

---

## PROBLEM SECTION

**Title:**
Agents reason well. They navigate blind.

**Pain Point 1:**
Icon: explosion / blast radius
**Invisible Blast Radius**
An agent modifies auth.rs. It has no idea that session middleware, JWT validation, and the password reset flow all depend on it. Changes propagate silently, bugs surface later.

**Pain Point 2:**
Icon: hole / gap
**Undetectable Structural Holes**
Keyword search finds what exists. It cannot find what is missing. No test file for a critical module. No error handler for a new edge case. No connection between two systems that should be linked. These gaps are invisible to grep.

**Pain Point 3:**
Icon: static / flat line
**Zero Memory Between Sessions**
Every session starts cold. The agent re-discovers the same relationships, makes the same exploratory queries, and has no record of what worked last time. Flat-file memory stores text. It does not store structure.

---

## SOLUTION SECTION

**Title:**
A graph that activates, learns, and remembers.

**How m1nd addresses each pain point:**

| Problem | m1nd Response |
|---------|---------------|
| Invisible blast radius | `m1nd.impact("auth.rs")` returns every affected node, ranked by signal strength, across structural and causal dimensions. The agent sees the full propagation before committing a single change. |
| Undetectable structural holes | `m1nd.missing("authentication")` finds nodes that should be connected but are not. It detects gaps by analyzing sibling activation patterns -- if 4 of 5 related modules activate strongly, the missing 5th is the structural hole. |
| Zero memory between sessions | `m1nd.learn()` applies Hebbian plasticity to edge weights. Correct results strengthen connections (LTP). Wrong results weaken them (LTD). `m1nd.drift()` shows what changed since the last session. The graph gets smarter with every query. |

**Before / After Comparison:**

```
FLAT FILE MEMORY                        m1nd GRAPH MEMORY
-----------------                       -----------------
auth.md:                                m1nd.activate("authentication"):
  "Authentication uses JWT tokens.        auth.rs         → 0.94 (seed)
   Session timeout is 30 min.             session.rs      → 0.78 (structural)
   See also: session management."         jwt_validator   → 0.71 (causal)
                                          middleware.rs   → 0.63 (imports auth)
What you get: text.                       password_reset  → 0.55 (co-change)
What you miss: everything structural.     rate_limiter    → MISSING (structural hole)
                                          user_model.rs   → 0.41 (semantic)

No blast radius.                        Full blast radius.
No gap detection.                       Gap detection built in.
No learning.                            Hebbian plasticity on every edge.
Same results every time.                Results improve with feedback.
```

---

## HOW IT WORKS

**Title:**
Four steps. One feedback loop.

**Step 1: Ingest**
Build a property graph from source data. m1nd walks your codebase, extracts symbols (functions, structs, imports, calls) across 6 languages, resolves cross-file references, and constructs a CSR graph with PageRank and community detection. Or point it at a JSON descriptor for any non-code domain.

**Step 2: Activate**
Query the graph with spreading activation. Signal propagates from seed nodes across four dimensions -- structural (graph topology), semantic (label similarity), temporal (co-change history), and causal (dependency direction). XLR noise cancellation suppresses high-PageRank hub nodes that activate non-specifically, borrowing differential signal processing from audio engineering.

**Step 3: Learn**
Tell m1nd what worked. `m1nd.learn("correct", [...nodes])` strengthens connections via long-term potentiation. `m1nd.learn("wrong", [...nodes])` weakens them via long-term depression. The graph encodes which paths matter for which queries. Every session makes the next session better.

**Step 4: Persist**
Graph state and plasticity weights save to disk automatically. On restart, m1nd loads the full graph -- no re-ingestion needed. The accumulated learning from every agent, every session, every feedback signal is preserved.

**Visual Direction:**
Horizontal cycle diagram: INGEST --> ACTIVATE --> LEARN --> PERSIST, with an arrow from PERSIST back to ACTIVATE to show the feedback loop. Below each step, a small code snippet showing the actual MCP call. The cycle should communicate that this is not a pipeline with a terminal step -- it is a loop that compounds.

---

## THE 13 TOOLS

**Title:**
13 MCP tools. One JSON-RPC call each.

### Discovery

| Tool | What it does |
|------|-------------|
| `m1nd.activate` | Spreading activation query -- "what's related to X?" ranked by 4D relevance with XLR noise cancellation. |
| `m1nd.why` | Path explanation -- "how are A and B connected?" via bidirectional BFS with relationship annotations. |
| `m1nd.missing` | Structural hole detection -- "what's missing from this picture?" based on sibling activation gaps. |
| `m1nd.fingerprint` | Equivalence detection -- "are these two things duplicates?" via activation profile cosine similarity. |
| `m1nd.resonate` | Harmonic analysis -- standing wave propagation reveals resonant frequencies and sympathetic node pairs. |

### Change Analysis

| Tool | What it does |
|------|-------------|
| `m1nd.impact` | Blast radius -- "what does changing X affect?" with forward, reverse, or bidirectional propagation. |
| `m1nd.predict` | Co-change prediction -- "what else will need to change?" based on git history and structural proximity. |
| `m1nd.counterfactual` | Removal simulation -- "what breaks if we delete X?" with cascade analysis and keystone detection. |

### Learning

| Tool | What it does |
|------|-------------|
| `m1nd.learn` | Hebbian feedback -- agent reports correct/wrong/partial, edges adjust via LTP/LTD. |
| `m1nd.drift` | Weight drift analysis -- "what changed in the graph since my last session?" |
| `m1nd.warmup` | Context priming -- "prepare for task X" by pre-activating relevant subgraphs. |

### System

| Tool | What it does |
|------|-------------|
| `m1nd.ingest` | Load data -- code extractor (6 languages) or JSON descriptor (any domain) into the property graph. |
| `m1nd.health` | Diagnostics -- node/edge counts, queries processed, active sessions, persistence status. |

**Visual Direction:**
Tool grid with dark cards. Each card shows the tool name in monospace, the one-line description, and a subtle icon indicating the category (magnifying glass for Discovery, delta/diff for Change Analysis, brain for Learning, gear for System). Alternatively: a terminal mockup showing a sequence of real JSON-RPC calls and their responses, demonstrating a complete session flow (ingest, activate, learn).

---

## USE CASES

**Title:**
Three ways agents use m1nd today.

### Tab 1: LLM Agent Memory

Replace flat-file context with a semantic graph that activates, learns, and persists across sessions. At session start, `m1nd.drift()` shows what evolved while the agent was offline. During work, `m1nd.activate()` retrieves context by spreading activation -- not keyword match. After each decision, `m1nd.learn()` strengthens the paths that worked. The graph compounds intelligence over time.

```
m1nd.drift(since="last_session")        // what changed?
m1nd.activate(query="auth refactor")    // relevant context
m1nd.learn(feedback="correct", nodes=[...])  // reinforce
```

### Tab 2: Build Orchestration

An AI build orchestrator uses m1nd as its coordination layer. After each agent completes a module: re-ingest to update the graph, learn from what was touched, predict what else needs changes, prime the next agent's context, check for scope escape, and detect new structural holes. Each completed module makes the orchestrator smarter for the next one.

```
m1nd.ingest(path="/repo", incremental=true)
m1nd.predict(changed_node="auth_module")
m1nd.missing(query="authentication")
```

### Tab 3: Code Intelligence

Standard code analysis with spreading activation instead of keyword search. Ask "what does changing this file affect?" and get a ranked list of every downstream dependency, weighted by structural distance, semantic similarity, co-change frequency, and causal direction. Ask "what if we remove this module?" and get cascade analysis with keystone detection.

```
m1nd.impact(node_id="src/auth.rs", direction="both")
m1nd.counterfactual(node_ids=["src/legacy.rs"])
m1nd.why(source="auth.rs", target="session.rs")
```

**Visual Direction:**
Three tabs or cards, selectable. Each shows the description text plus a code block with real MCP commands. Active tab has a subtle accent color. No screenshots -- the code examples are the visual.

---

## TECHNICAL SPECS

**Title:**
What's under the hood.

| Metric | Value |
|--------|-------|
| Language | Rust |
| Lines of code | ~15,500 |
| Source files | 32 |
| Crates | 3 (m1nd-core, m1nd-ingest, m1nd-mcp) |
| Tests | 159 |
| MCP tools | 13 |
| Supported languages | Rust, Python, TypeScript, Go, Java + generic fallback |
| Protocol | MCP 2024-11-05 (JSON-RPC stdio) |
| License | MIT |

**Key Internals:**

- **Graph**: CSR (Compressed Sparse Row) with AtomicU32 edge weights for lock-free concurrent plasticity
- **Type Safety**: FiniteF32 / PosF32 newtypes -- NaN-free by construction, not by validation
- **Activation**: WavefrontEngine with configurable decay, hop limits, and energy budgets across 4 independent dimensions
- **Noise Cancellation**: XLR differential signal processing -- maintains parallel signal and noise CSR graphs, gates activation through adaptive sigmoid (borrowed from balanced audio cable design)
- **Plasticity**: Hebbian LTP/LTD with generation tracking, persisted across restarts
- **Community Detection**: Louvain algorithm on undirected adjacency for module clustering
- **Resonance**: Standing wave propagation, harmonic analysis, sympathetic pair detection
- **Persistence**: Dual-file JSON snapshot (graph + plasticity state), auto-persist every N queries + on SIGINT

**Architecture Diagram Direction:**
Three-layer horizontal diagram. Bottom: m1nd-core (engine, graph, activation, plasticity). Middle: m1nd-ingest (code extractors for 6 languages, JSON adapter, reference resolver). Top: m1nd-mcp (JSON-RPC stdio server, 13 tool handlers, session management). Arrows flow upward (core provides to ingest, both provide to mcp). Side annotation: "One process. One graph. Multiple agents."

---

## SOCIAL PROOF SECTION

**Title:**
Built to ingest itself.

Before there are external users, the most honest proof is self-ingestion. m1nd eats its own codebase as a stress test.

| Self-Ingest Metric | Value |
|--------------------|-------|
| Files ingested | 32 source files across 3 crates |
| Nodes extracted | 693 |
| Edges resolved | 2,007 (including bidirectional and reverse) |
| Cross-crate references | Resolved via use-path extraction + label matching |
| Tool response time | All 13 tools under 100ms |
| NaN / panic / parse error | Zero across 15+ sequential RPC sessions |
| Persistence round-trip | Verified: save, kill, reload, query -- identical results |

**Additional credibility signals:**

- 9 PRD documents (~13,300 lines of specification written before code)
- 9 hardening reports covering 211 identified failure modes (39 critical, 57 high)
- Every critical failure mode addressed in the type system (FiniteF32 eliminates NaN at the type level, not with runtime checks)
- Built using the Grounded One-Shot Build methodology: spec-first, harden-before-build, 10 parallel agents across 4 waves

**Visual Direction:**
A metrics dashboard. Clean number cards showing the self-ingest stats. Below, a single terminal screenshot or animation showing m1nd ingesting its own source, then running `m1nd.activate("spreading activation")` and returning ranked results from its own code. The message: this system understands itself.

---

## CTA SECTION

**Title:**
Give your agent a nervous system.

**Subtext:**
m1nd is MIT-licensed, single-binary, zero-dependency at runtime. `cargo build --release` and connect via MCP.

**Primary CTA:**
`View on GitHub` --> [github link]

**Secondary CTA:**
`Read the Integration Guide` --> INTEGRATION-GUIDE.md

**Tertiary CTA (code block):**
```bash
cargo build --release
./target/release/m1nd-mcp
```

---

## FOOTER

**Left:**
MIT License

**Center:**
COSMOPHONIX INTELLIGENCE

**Right:**
GitHub | Integration Guide | Architecture Report

---

## IMPLEMENTATION NOTES FOR DEVELOPER

**Typography:** Monospace for tool names, code, and metrics. Sans-serif for body copy. No serif fonts.

**Color palette:** Dark background (near-black). Accent color for activation signal (warm amber or electric blue -- not both). Muted gray for inactive elements. White for text. Red for structural holes / warnings. Green only for "correct" feedback indicators.

**Animations:** Limit to the hero graph activation and subtle hover states on tool cards. No scroll-jacking, no parallax, no auto-playing video. The reader is a developer. Respect their scroll.

**Responsive:** Tool grid collapses to single column on mobile. Before/after comparison stacks vertically. Code blocks get horizontal scroll.

**Performance:** No framework-rendered graph visualization in the hero -- use a pre-rendered animation (Lottie or CSS keyframes on SVG paths). The page should load in under 2 seconds on a 3G connection.

**SEO targets:** "MCP tools for LLM agents", "spreading activation knowledge graph", "LLM agent memory", "code intelligence Rust", "Hebbian plasticity graph".
