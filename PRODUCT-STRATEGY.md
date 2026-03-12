# m1nd Product Strategy

**Author**: Max Elias Kleinschmidt -- COSMOPHONIX INTELLIGENCE
**Date**: 2026-03-12
**Status**: Active

---

## 1. Naming Analysis

### The Problem with "Semantic Circuit Simulator"

The current descriptor fails on three counts.

**It sounds passive.** "Simulator" implies m1nd models something external -- that it renders a visualization or runs a simulation of a circuit someone else designed. In reality, m1nd IS the circuit. It activates, propagates, learns, and adapts. It does not simulate cognition; it performs it on graphs.

**It invites the wrong analogy.** "Circuit" pulls the reader toward electronics -- PCB layout, SPICE simulation, chip design. The intended analogy (neural circuits, spreading activation, signal propagation) gets lost. A developer scanning a tools list sees "circuit simulator" and skips it because they are not building hardware.

**"Semantic" is overloaded to the point of meaninglessness.** In 2026, "semantic" appears in the descriptor of every vector database, every RAG pipeline, every search tool. It no longer differentiates. m1nd's semantic dimension is one of four -- leading with it undersells the other three (structural, temporal, causal) and the learning system that ties them together.

The net effect: the descriptor communicates the wrong category, the wrong capability, and the wrong audience.

### Five Alternative Descriptors

| # | Descriptor | Pros | Cons |
|---|-----------|------|------|
| 1 | **Cognitive Graph Engine** | Accurate: it is an engine that performs cognition over graphs. "Cognitive" signals intelligence without overpromising. "Engine" says infrastructure, not application. | "Cognitive" could imply consciousness-adjacent hype to skeptics. |
| 2 | **Adaptive Knowledge Engine** | Emphasizes the learning/plasticity dimension. "Knowledge" is closer to the actual domain than "circuit." "Adaptive" differentiates from static graph tools. | "Knowledge Engine" has some overlap with knowledge base / knowledge management branding. Slightly enterprise-flavored. |
| 3 | **Graph Activation Engine** | Technically precise. Directly describes what happens: activation propagates across a graph. No ambiguity about what category this is. | Less evocative. Does not convey the learning dimension. Could be mistaken for a graph database feature. |
| 4 | **Neural Graph Engine** | Captures the neuro-inspired architecture (spreading activation, Hebbian plasticity, signal/noise cancellation). Strong metaphor. | "Neural" in 2026 might suggest neural network / deep learning, which this is not. Risks a different miscategorization. |
| 5 | **Activation Engine for Knowledge Graphs** | Most explicit. Zero ambiguity about what it does (activation) and what it operates on (knowledge graphs). Self-documenting. | Longer than the others. "For Knowledge Graphs" is a constraint statement rather than a capability statement. |

### Recommendation: Cognitive Graph Engine

"Cognitive Graph Engine" is the right descriptor. Here is why.

**Accurate without overreach.** m1nd does perform cognitive operations on graphs -- it activates, learns, remembers, detects gaps, predicts changes, and reasons about causality. "Cognitive" is a factual description of the capability class, not a marketing stretch.

**Correct category signal.** "Engine" tells developers this is infrastructure they build on, not an application they use. It sits alongside "rendering engine," "physics engine," "search engine" -- tools that power other tools.

**Differentiating.** No competing product in the AI agent tooling space calls itself a cognitive graph engine. Vector databases are storage. RAG pipelines are retrieval. Knowledge graphs are data structures. m1nd is the thing that thinks over any of them.

**Memorable and compact.** Three words. Easy to say, easy to type, easy to remember.

The product name remains **m1nd**. The full identifier becomes: **m1nd -- Cognitive Graph Engine**.

---

## 2. Positioning

### Target Audience

**Primary: AI agent developers.** Engineers building autonomous or semi-autonomous LLM agents (coding assistants, research agents, build orchestrators) who need their agents to understand context beyond the current prompt window. These developers are hitting the wall where retrieval-augmented generation returns relevant chunks but not connected understanding.

**Secondary: Researchers.** People working with concept graphs, citation networks, ontologies, or any domain where entity-relationship structures carry meaning. They want to ask "what's connected to X and why?" and get answers that improve over time.

**Tertiary: Dev tool builders.** Teams building IDEs, code review tools, CI/CD intelligence, or documentation systems that need structural understanding of codebases -- not just text search, but dependency-aware, change-aware, gap-aware analysis.

### Competitive Landscape

| Category | What They Do | Limitation m1nd Addresses |
|----------|-------------|--------------------------|
| **Vector databases** (Pinecone, Weaviate, Qdrant) | Store and retrieve embeddings by similarity | Similarity is not understanding. No structural awareness, no causal reasoning, no learning from feedback, no gap detection. |
| **RAG pipelines** (LlamaIndex, LangChain retrieval) | Chunk documents, embed them, retrieve top-k | Returns fragments, not connected subgraphs. Cannot answer "what's missing?" or "what else will change?" No plasticity. |
| **Knowledge graphs** (Neo4j, Amazon Neptune) | Store entities and relationships, run graph queries | The graph is the data structure, not the intelligence. You write the queries; the graph does not activate, learn, or detect its own gaps. |
| **Code search / intelligence** (Sourcegraph, GitHub code search, ast-grep) | Pattern match, symbol search, reference finding | Search returns what matches the query. m1nd returns what the query activates -- including indirect connections, co-change predictions, and structural holes the query did not ask about. |
| **Code analysis** (CodeQL, Semgrep) | Static analysis with predefined rule sets | Rule-based, not learning. Finds what you tell it to look for. Cannot discover unknown unknowns or improve from feedback. |

### How m1nd Is Different

Three structural differentiators separate m1nd from everything listed above.

**Not search -- activation.** Search returns what matches a query. m1nd activates a wavefront that propagates across the graph, losing energy at each hop, boosted or dampened by four independent dimensions. The results are not "things that match" but "things that respond to the signal." This is a fundamentally different retrieval model.

**Not static -- learning.** Every other tool in the landscape returns the same results for the same query regardless of history. m1nd runs Hebbian plasticity: connections that lead to correct results strengthen, connections that lead to wrong results weaken. The same query returns better results on the tenth run than on the first.

**Not single-dimension -- 4D.** Vector search operates on one dimension (embedding similarity). Graph queries operate on structure. m1nd merges four dimensions (structural topology, semantic similarity, temporal co-change patterns, causal dependency flow) into a single ranked activation. This is how you get answers like "these files are structurally distant but always change together and have a causal dependency chain through three intermediaries."

### Positioning Statement

**m1nd gives LLM agents structural intelligence over knowledge graphs -- not search, but activation that propagates, learns, and reveals what is missing.**

---

## 3. Tagline Options

| # | Tagline | Rationale |
|---|---------|-----------|
| 1 | **Activation intelligence for LLM agents.** | Leads with the core mechanism (activation), states the capability class (intelligence), names the audience (LLM agents). |
| 2 | **The graph engine that thinks.** | Bold claim, but defensible -- it activates, learns, detects gaps. Differentiates from passive graph databases. |
| 3 | **Graphs that activate, learn, and reason.** | Describes the three core capabilities in sequence. Technically precise. |
| 4 | **Give your agents a nervous system.** | Metaphor from the README. Resonant, memorable, speaks to the agent developer audience. |
| 5 | **Structural intelligence for AI agents.** | "Structural intelligence" is a category claim. Compact, professional, unambiguous. |

**Recommendation for primary tagline: "Give your agents a nervous system."**

It is the most resonant phrase in the existing documentation for a reason. It communicates the core value proposition in terms the audience immediately understands: your agent is a brain with no body, no senses, no memory. m1nd is the nervous system that connects it to structured reality.

For contexts where a more technical register is appropriate (documentation, API references, conference talks), use: **"Activation intelligence for LLM agents."**

---

## 4. Value Propositions

### For AI Agent Developers

**Problem**: Your agent can reason about what is in its context window but is blind to everything outside it. RAG gives it fragments. Vector search gives it similar chunks. Neither gives it structural understanding -- how things connect, what is missing, what else will change, or how the graph evolves over feedback.

**What m1nd solves**: m1nd gives your agent a persistent, learning graph that responds to queries with spreading activation across four dimensions. Instead of "here are 20 similar chunks," your agent gets "here are the 20 most activated nodes, here is a structural hole you should know about, here is what else will need to change, and by the way, this result is 30% better than last session because I learned from your feedback."

**Concrete capabilities**:
- Query context that improves with every feedback cycle (Hebbian plasticity)
- Blast radius analysis before any change ("what does touching this affect?")
- Co-change prediction from historical patterns ("what else will break?")
- Structural hole detection ("what is missing from this picture?")
- Multi-agent shared memory with per-agent session tracking
- Automatic persistence across restarts -- agents resume where they left off

### For Researchers

**Problem**: You have a knowledge graph (concepts, citations, experimental data, ontologies) and you want to explore it in ways that go beyond "show me the neighbors of X." You want to ask "what connects A to B through three intermediaries?", "what is conspicuously absent from this subgraph?", "if I remove this node, what collapses?" -- and you want the system to learn your exploration patterns.

**What m1nd enables**:
- Multi-dimensional activation that discovers non-obvious connections across structural, semantic, temporal, and causal dimensions simultaneously
- Counterfactual simulation: remove a concept and measure what loses activation (keystone analysis)
- Standing wave resonance: discover harmonic relationships and sympathetic node pairs that no query explicitly asks for
- A system that gets better at answering YOUR questions the more you use it, because plasticity adapts edge weights to your feedback patterns

### For Dev Tool Builders

**Problem**: You are building a tool that needs to understand code (or any structured domain) at a level deeper than text search. You need dependency awareness, change prediction, gap detection, and structural analysis -- but building this from scratch means implementing graph algorithms, activation models, temporal analysis, and a persistence layer.

**What m1nd enables**:
- 13 MCP tools accessible via JSON-RPC stdio -- drop m1nd into any tool as an intelligence backend
- Domain-agnostic engine: point it at code, research data, supply chains, audio production graphs -- any entity-relationship domain
- Built-in ingestion for 6 programming languages plus a JSON adapter for arbitrary graphs
- 3.8 MB binary, zero external dependencies at runtime, sub-100ms response times
- The learning layer is automatic: every agent interaction refines the graph for all future queries

---

## 5. Elevator Pitch

### 30-Second Version

LLM agents are powerful reasoners but poor navigators -- they cannot see how things connect, what is missing, or what their changes will break. m1nd is a cognitive graph engine that gives agents structural intelligence: query a concept and the graph activates across four dimensions, cancels noise, learns from feedback, and reveals gaps no search could find. Built in Rust, exposed as 13 MCP tools, domain-agnostic, and it gets smarter every session.

### 2-Minute Version

Every LLM agent today operates on flat context. RAG retrieves relevant chunks, vector search finds similar embeddings, but neither gives the agent structural understanding of how things connect, what is missing, or how the system will respond to change. m1nd is a cognitive graph engine that solves this. You ingest any knowledge graph -- a codebase, a research domain, a supply chain -- and m1nd builds a property graph with spreading activation across four dimensions: structural topology, semantic similarity, temporal co-change patterns, and causal dependency flow. When an agent queries "what is related to authentication?", m1nd does not search for the word -- it activates a wavefront that propagates through the graph, decaying with distance, boosted by semantic resonance and co-change history, with noise cancelled via differential signal processing borrowed from audio engineering. The results are not matches; they are the subgraph that responds to the signal. Then m1nd goes further: it detects structural holes (things that should be connected but are not), predicts co-changes (what else will need to change), simulates removal (what breaks if we delete this), and learns from feedback via Hebbian plasticity so the same query returns better results next time. It is built in Rust, ships as a 3.8 MB binary, exposes 13 tools over MCP, and works on any domain with entities and relationships. The core insight: intelligence is not search -- it is activation, propagation, and learning over structure.

---

## 6. README Header Recommendation

```
# m1nd

**Cognitive Graph Engine** -- spreading activation, noise cancellation, and Hebbian learning over knowledge graphs. Built for LLM agents via MCP.

Give your agents a nervous system.
```

**Rationale**: Line 1 is the product name. Line 3 is the descriptor followed by the three core mechanisms (activation, noise cancellation, learning) and the target integration (LLM agents via MCP). Line 5 is the tagline that makes it stick. A developer scanning GitHub repos sees the name, understands the category, grasps the mechanism, and knows the audience -- all in three lines.

---

## Appendix: Naming Decision Matrix

| Criterion | Semantic Circuit Simulator | Cognitive Graph Engine |
|-----------|---------------------------|----------------------|
| Communicates what it does | No -- sounds like it renders circuit diagrams | Yes -- cognitive operations on graphs |
| Names the right category | No -- "simulator" is wrong | Yes -- "engine" is infrastructure |
| Differentiates from competitors | No -- "semantic" is everywhere | Yes -- no competitor uses this descriptor |
| Speaks to the audience | No -- agent devs do not build circuits | Yes -- agent devs understand engines |
| Technically accurate | Partially -- "circuit" is a metaphor | Yes -- cognition over graphs is literal |
| Memorable | Moderate | Strong -- three words, clear image |
| Risk of miscategorization | High -- EE/hardware confusion | Low -- "cognitive" is unambiguous in context |

---

**MAX ELIAS KLEINSCHMIDT -- COSMOPHONIX INTELLIGENCE**
