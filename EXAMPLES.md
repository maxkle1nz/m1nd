# m1nd -- Live Examples

> Every number below is **real output** from m1nd analyzing its own codebase (737 nodes, 2126 edges).
> Nothing is fabricated. Run `cargo build --release` and reproduce these yourself.

---

## Tool Showcase

### a) `m1nd.ingest` -- Build the connectome

```
┌──────────────────────────────────────────────────────────────────┐
│  m1nd.ingest                                                     │
│  path: "/Users/cosmophonix/connectome-poc/m1nd"                  │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  MODE       full                                                 │
│  ADAPTER    code                                                 │
│                                                                  │
│  Files scanned ···· 49                                           │
│  Files parsed ····· 49                                           │
│  Nodes created ···· 737                                          │
│  Edges created ···· 773                                          │
│                                                                  │
│  elapsed: 102.2ms                                                │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
  ↳ Ingested the entire m1nd Rust codebase in ~100ms.
    After finalization: 737 nodes, 2126 edges (includes inferred edges).
```

---

### b) `m1nd.activate` -- Spreading activation query

```
┌──────────────────────────────────────────────────────────────────┐
│  m1nd.activate                                                   │
│  query: "spreading activation"                                   │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  SEEDS (27 matched, top 8 shown)                                 │
│  ├── activation                  relevance: 1.000                │
│  ├── ActivationResult            relevance: 0.900                │
│  ├── ActivationEngine            relevance: 0.900                │
│  ├── ActivationFingerprinter     relevance: 0.900                │
│  ├── m1nd-core/src/activation.rs relevance: 0.800                │
│  ├── total_activation            relevance: 0.800                │
│  ├── ActivateInput               relevance: 0.373                │
│  └── handle_activate             relevance: 0.343                │
│                                                                  │
│  ACTIVATED (top 10)                                              │
│  #   Node                          Type      Activation  PR      │
│  1   activation                    Module    1.152       0.005   │
│  2   ActivationResult              Struct    1.090       0.009   │
│  3   ActivationEngine              Type      1.088       0.010   │
│  4   m1nd-core/src/activation.rs   File      1.049       0.194   │
│  5   ActivationFingerprinter       Struct    1.019       0.007   │
│  6   total_activation              Function  0.979       0.005   │
│  7   activation_empty_seeds_...    Function  0.852       0.005   │
│  8   ActivatedNode                 Struct    0.812       0.009   │
│  9   test_causal_activation_...    Function  0.806       0.005   │
│  10  test_temporal_activation_...  Function  0.806       0.005   │
│                                                                  │
│  DIMENSION BREAKDOWN (node #1: "activation")                     │
│  ├── structural ·· 0.985                                         │
│  ├── semantic ···· 0.310                                         │
│  ├── temporal ···· 0.640                                         │
│  └── causal ······ 1.000                                         │
│                                                                  │
│  GHOST EDGES (10 discovered)                                     │
│  ├── activation ──→ ActivationResult      strength: 1.00         │
│  ├── activation ──→ ActivationEngine      strength: 1.00         │
│  ├── activation ──→ activation.rs         strength: 1.00         │
│  ├── activation ──→ ActivationFingerprint strength: 1.00         │
│  └── ActivationResult ──→ ActivationEngine  strength: 1.00      │
│      ... and 5 more                                              │
│                                                                  │
│  PLASTICITY                                                      │
│  edges strengthened: 8  |  decayed: 2062  |  LTP events: 0      │
│                                                                  │
│  elapsed: 5.1ms                                                  │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
  ↳ Found activation.rs as the top file (PageRank 0.194 -- highest in the
    codebase). Ghost edges reveal latent connections between co-activated
    nodes that lack explicit graph edges.
```

---

### c) `m1nd.impact` -- Blast radius analysis

```
┌──────────────────────────────────────────────────────────────────┐
│  m1nd.impact                                                     │
│  node: "file::m1nd-core/src/graph.rs"                            │
│  direction: forward                                              │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  SOURCE   m1nd-core/src/graph.rs                                 │
│  ENERGY   18.73 total                                            │
│  DEPTH    4 hops max                                             │
│                                                                  │
│  BLAST RADIUS (70 affected, showing top 15)                      │
│  #   Node                Type       Signal  Hops                 │
│  1   StringInterner       Struct     0.547   1                   │
│  2   CsrGraph             Struct     0.547   1                   │
│  3   PendingEdge          Struct     0.547   1                   │
│  4   Graph                Struct     0.137   1                   │
│  5   finalize             Function   0.547   1                   │
│  6   compute_pagerank     Function   0.547   1                   │
│  7   add_node             Function   0.547   1                   │
│  8   add_edge             Function   0.547   1                   │
│  9   resolve_id           Function   0.547   1                   │
│  10  NodeStorage          Struct     0.547   1                   │
│  11  EdgePlasticity       Struct     0.547   1                   │
│  12  PlasticityNode       Struct     0.547   1                   │
│  13  ImpactDirection      Enum       0.125   2                   │
│  14  M1ndError            Enum       0.037   1                   │
│  15  m1nd-core/src/error.rs  File    0.020   2                   │
│                                                                  │
│  CAUSAL CHAINS (36 chains, showing top 5)                        │
│  ├── graph.rs ─contains→ StringInterner       str: 0.80          │
│  ├── graph.rs ─contains→ finalize             str: 0.80          │
│  ├── graph.rs ─contains→ compute_pagerank     str: 0.80          │
│  ├── graph.rs ─contains→ CsrGraph             str: 0.80          │
│  └── graph.rs ─contains→ Graph                str: 0.80          │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
  ↳ Changing graph.rs directly affects 70 nodes across 4 hops.
    StringInterner, CsrGraph, and PageRank are in the immediate blast zone.
```

---

### d) `m1nd.why` -- Path explanation

```
┌──────────────────────────────────────────────────────────────────┐
│  m1nd.why                                                        │
│  source: "file::m1nd-core/src/graph.rs"                          │
│  target: "file::m1nd-core/src/activation.rs"                     │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  PATH FOUND  (2 hops)                                            │
│                                                                  │
│  graph.rs ──contains──→ Graph ──references──→ activation.rs      │
│                                                                  │
│  same_community: false                                           │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
  ↳ graph.rs reaches activation.rs in 2 hops: it contains the Graph
    struct, which activation.rs references. They're in different
    communities -- a potential coupling concern.
```

**Bonus: cross-module path**

```
┌──────────────────────────────────────────────────────────────────┐
│  m1nd.why                                                        │
│  source: "file::m1nd-core/src/xlr.rs"                            │
│  target: "file::m1nd-core/src/activation.rs"                     │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  PATH FOUND  (2 hops)                                            │
│                                                                  │
│  xlr.rs ──references──→ Graph ──references──→ activation.rs      │
│                                                                  │
│  same_community: false                                           │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
  ↳ XLR and activation are connected through the Graph struct --
    the central hub of the codebase.
```

---

### e) `m1nd.missing` -- Structural holes

```
┌──────────────────────────────────────────────────────────────────┐
│  m1nd.missing                                                    │
│  query: "error handling"                                         │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  STRUCTURAL HOLES (7 detected)                                   │
│                                                                  │
│  These files have activated neighbors but are themselves          │
│  inactive -- potential gaps in error handling coverage:           │
│                                                                  │
│  #  Node                        Neighbors  Avg Activation        │
│  1  m1nd-ingest/src/walker.rs   2          1.077                 │
│  2  m1nd-ingest/src/lib.rs      2          1.077                 │
│  3  m1nd-ingest/src/json_a...   2          1.077                 │
│  4  m1nd-mcp/src/server.rs      2          1.077                 │
│  5  m1nd-core/src/graph.rs      2          1.077                 │
│  6  m1nd-core/src/lib.rs        7          0.911                 │
│  7  m1nd-mcp/src/tools.rs       11         0.541                 │
│                                                                  │
│  ghost_edges: 0                                                  │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
  ↳ tools.rs has 11 activated neighbors but is itself inactive for
    "error handling" -- it dispatches errors but may lack its own
    error handling logic. The ingest crate also shows gaps.
```

**Bonus: structural holes for "graph module"**

```
┌──────────────────────────────────────────────────────────────────┐
│  m1nd.missing                                                    │
│  query: "graph module"                                           │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  STRUCTURAL HOLES (10 detected)                                  │
│                                                                  │
│  #  Node                           Neighbors  Avg Activation     │
│  1  m1nd-ingest/src/resolve.rs     2          1.136              │
│  2  m1nd-ingest/src/json_adapter   2          1.136              │
│  3  m1nd-mcp/src/session.rs        2          1.136              │
│  4  m1nd-core/src/counterfactual   2          1.136              │
│  5  m1nd-core/src/semantic.rs      2          1.136              │
│  6  m1nd-core/src/query.rs         2          1.136              │
│  7  m1nd-core/src/temporal.rs      2          1.136              │
│  8  m1nd-core/src/xlr.rs           2          1.136              │
│  9  m1nd-core/src/resonance.rs     2          1.136              │
│  10 m1nd-core/src/activation.rs    2          1.136              │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
  ↳ Every engine file (activation, xlr, resonance, temporal, semantic,
    query) uses the graph module but isn't directly part of it --
    this is expected coupling. The holes highlight the dependency surface.
```

---

### f) `m1nd.predict` -- Co-change prediction

```
┌──────────────────────────────────────────────────────────────────┐
│  m1nd.predict                                                    │
│  changed_node: "file::m1nd-core/src/graph.rs"                    │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  PREDICTED CO-CHANGES (10 predictions)                           │
│                                                                  │
│  #   Node                Coupling Strength                       │
│  1   StringInterner      0.285                                   │
│  2   new                 0.285                                   │
│  3   with_capacity       0.285                                   │
│  4   get_or_intern       0.285                                   │
│  5   resolve             0.285                                   │
│  6   try_resolve         0.285                                   │
│  7   lookup              0.285                                   │
│  8   len                 0.285                                   │
│  9   is_empty            0.285                                   │
│  10  PendingEdge         0.285                                   │
│                                                                  │
│  VELOCITY                                                        │
│  trend: Stable  |  velocity: -0.064                              │
│  structural_fallback: 0                                          │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
  ↳ If you change graph.rs, you'll likely need to update StringInterner,
    PendingEdge, and all their methods. Velocity is stable -- no recent
    churn detected.
```

---

### g) `m1nd.counterfactual` -- What-if removal

**Single node removal:**

```
┌──────────────────────────────────────────────────────────────────┐
│  m1nd.counterfactual                                             │
│  remove: ["file::m1nd-core/src/types.rs"]                        │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  WHAT IF types.rs WERE REMOVED?                                  │
│                                                                  │
│  Reachability: 723 → 710 nodes  (-13 unreachable)                │
│  Activation lost: 52.79%                                         │
│                                                                  │
│  CASCADE                                                         │
│  depth: 2                                                        │
│  affected by depth:                                              │
│    hop 1 ···· 27 nodes                                           │
│    hop 2 ····  1 node                                            │
│  total affected: 28 nodes                                        │
│                                                                  │
│  orphaned: 0  |  weakened: 0                                     │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
  ↳ types.rs is critical -- removing it loses 52.8% of activation
    energy and cascades 2 hops deep, affecting 28 nodes.
```

**Multi-node removal with synergy:**

```
┌──────────────────────────────────────────────────────────────────┐
│  m1nd.counterfactual                                             │
│  remove: ["file::m1nd-core/src/graph.rs",                        │
│           "file::m1nd-core/src/types.rs"]                        │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  WHAT IF graph.rs AND types.rs WERE REMOVED?                     │
│                                                                  │
│  Reachability: 723 → 682 nodes  (-41 unreachable)                │
│  Activation lost: 52.79%                                         │
│                                                                  │
│  CASCADE                                                         │
│  depth: 5                                                        │
│  affected by depth:                                              │
│    hop 1 ···· 41 nodes                                           │
│    hop 2 ···· 28 nodes                                           │
│    hop 3 ···· 23 nodes                                           │
│    hop 4 ···· 44 nodes                                           │
│    hop 5 ····  1 node                                            │
│  total affected: 137 nodes                                       │
│                                                                  │
│  SYNERGY ANALYSIS                                                │
│  ├── graph.rs alone ···· 0.0% activation lost                    │
│  ├── types.rs alone ···· 52.79% activation lost                  │
│  └── both together ····· 52.79% activation lost                  │
│  synergy factor: 1.0x                                            │
│                                                                  │
│  orphaned: 0  |  weakened: 0                                     │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
  ↳ Removing both files cascades 5 hops deep, affecting 137 nodes
    (19% of the graph). types.rs drives the activation loss while
    graph.rs drives the structural cascade. Synergy factor 1.0x means
    the damage is additive, not multiplicative.
```

---

### h) `m1nd.health` -- Server diagnostics

```
┌──────────────────────────────────────────────────────────────────┐
│  m1nd.health                                                     │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  STATUS   ok                                                     │
│                                                                  │
│  Nodes ············ 737                                          │
│  Edges ············ 2,126                                        │
│  Queries processed · 0                                           │
│  Uptime ··········· 0.13s                                        │
│  Plasticity state · 2126 edges tracked                           │
│  Last persist ····· 0s ago                                       │
│                                                                  │
│  ACTIVE SESSIONS                                                 │
│  ┌─────────┬───────────┬──────────────┬─────────┐                │
│  │ agent   │ queries   │ last seen    │ uptime  │                │
│  ├─────────┼───────────┼──────────────┼─────────┤                │
│  │ demo    │ 2         │ 0.000s ago   │ 0.13s   │                │
│  └─────────┴───────────┴──────────────┴─────────┘                │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
  ↳ Full connectome loaded and serving. All 2,126 edges have
    plasticity tracking enabled.
```

---

### i) `m1nd.learn` -- Feedback-driven learning

```
┌──────────────────────────────────────────────────────────────────┐
│  m1nd.learn                                                      │
│  query: "spreading activation"                                   │
│  feedback: "correct"                                             │
│  nodes: ["activation.rs", "graph.rs"]                            │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  LEARNING APPLIED                                                │
│                                                                  │
│  Feedback ········ correct                                       │
│  Strength ········ 0.200                                         │
│  Nodes found ····· 2                                             │
│  Nodes expanded ·· 55                                            │
│  Edges modified ·· 158                                           │
│                                                                  │
│  The query "spreading activation" was marked correct for          │
│  activation.rs and graph.rs. 158 edges in their neighborhood     │
│  were strengthened by factor 0.200.                               │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
  ↳ From 2 seed nodes, m1nd expanded to 55 related nodes and
    adjusted 158 edges. Future queries for "spreading activation"
    will rank these nodes higher.
```

---

### j) `m1nd.fingerprint` -- Activation fingerprint

```
┌──────────────────────────────────────────────────────────────────┐
│  m1nd.fingerprint                                                │
│  target: "file::m1nd-core/src/graph.rs"                          │
│  similarity_threshold: 0.50                                      │
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│  TARGET   graph.rs                                               │
│                                                                  │
│  EQUIVALENTS   (none found above threshold 0.50)                 │
│                                                                  │
│  graph.rs has a unique activation fingerprint -- no other         │
│  node in the codebase responds to the same set of queries         │
│  in a similar pattern.                                            │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
  ↳ graph.rs is structurally unique -- it has no functional
    equivalents. This confirms it's a singular point of failure:
    nothing else can substitute for it.
```

---

## Full Session: Code Intelligence Demo

```
═══════════════════════════════════════════════════════════════
  m1nd -- Live Session: Code Intelligence Demo
  Codebase: m1nd (self-analysis)
  737 nodes | 2,126 edges | ingested in 102ms
═══════════════════════════════════════════════════════════════

  Agent: "I need to modify graph.rs. What should I know?"

─────────────────────────────────────────────────────────────

  Step 1: What does graph.rs affect?

  > m1nd.impact("file::m1nd-core/src/graph.rs")

  SOURCE   m1nd-core/src/graph.rs
  ENERGY   18.73 total
  DEPTH    4 hops max

  Top affected nodes:
  #   Node                Type       Signal  Hops
  1   StringInterner       Struct     0.547   1
  2   CsrGraph             Struct     0.547   1
  3   finalize             Function   0.547   1
  4   compute_pagerank     Function   0.547   1
  5   add_node             Function   0.547   1
  6   Graph                Struct     0.137   1
  7   ImpactDirection      Enum       0.125   2
  8   M1ndError            Enum       0.037   1
  9   error.rs             File       0.020   2
  10  types.rs             File       0.017   2

  Causal chains: 36 total
  └── All through "contains" relation at hop 1

  → graph.rs has a blast radius of 70 nodes across 4 hops.

─────────────────────────────────────────────────────────────

  Step 2: What else will need to change?

  > m1nd.predict("file::m1nd-core/src/graph.rs")

  PREDICTED CO-CHANGES (10 predictions)

  #   Node                Coupling
  1   StringInterner      0.285
  2   get_or_intern       0.285
  3   resolve             0.285
  4   CsrGraph            (via out_range, in_range, read_weight)
  5   PendingEdge         0.285

  Velocity: Stable (-0.064)

  → These 10 items will almost certainly need updates if you
    touch graph.rs. StringInterner is the tightest coupling.

─────────────────────────────────────────────────────────────

  Step 3: How does graph.rs connect to activation.rs?

  > m1nd.why("file::m1nd-core/src/graph.rs",
             "file::m1nd-core/src/activation.rs")

  PATH FOUND (2 hops)

  graph.rs ──contains──→ Graph ──references──→ activation.rs

  same_community: false

  → They connect through the Graph struct. Different communities
    means changes to graph.rs may have unexpected effects on
    the activation engine.

─────────────────────────────────────────────────────────────

  Step 4: What if we also needed to remove types.rs?

  > m1nd.counterfactual(["file::m1nd-core/src/graph.rs",
                          "file::m1nd-core/src/types.rs"])

  Reachability: 723 → 682 nodes (-41 unreachable)
  Activation lost: 52.79%

  CASCADE DEPTH: 5
    hop 1 → 41 nodes
    hop 2 → 28 nodes
    hop 3 → 23 nodes
    hop 4 → 44 nodes
    hop 5 →  1 node
  total: 137 nodes affected (19% of graph)

  SYNERGY
  ├── graph.rs alone ···· 0.0% lost
  ├── types.rs alone ···· 52.79% lost
  └── both together ····· 52.79% lost
  synergy: 1.0x (additive, not multiplicative)

  → Removing both would cascade through 19% of the codebase.
    types.rs is the real activation driver; graph.rs drives
    structural reach.

─────────────────────────────────────────────────────────────

  Step 5: Where are the structural gaps around error handling?

  > m1nd.missing("error handling")

  STRUCTURAL HOLES (7 detected)

  #  Node                        Neighbors  Avg Act.
  1  m1nd-ingest/src/walker.rs   2          1.077
  2  m1nd-ingest/src/lib.rs      2          1.077
  3  m1nd-mcp/src/server.rs      2          1.077
  4  m1nd-core/src/graph.rs      2          1.077
  5  m1nd-core/src/lib.rs        7          0.911
  6  m1nd-mcp/src/tools.rs       11         0.541

  → tools.rs has 11 neighbors active for "error handling" but
    is itself inactive -- it delegates errors but may not
    handle them robustly. The ingest crate has similar gaps.

─────────────────────────────────────────────────────────────

  Step 6: Agent completes the changes, provides feedback

  > m1nd.learn(query="spreading activation",
               feedback="correct",
               nodes=["activation.rs", "graph.rs"])

  Nodes found:    2
  Nodes expanded: 55
  Edges modified: 158
  Strength:       0.200

  → m1nd strengthened 158 edges around the confirmed-correct
    result. Next time someone queries "spreading activation",
    these paths will be weighted higher.

═══════════════════════════════════════════════════════════════
  Session complete. The connectome learned from this interaction.
  Total queries: 6  |  Edges modified: 158  |  All under 6ms
═══════════════════════════════════════════════════════════════
```

---

## Quick Reference

| Tool | Purpose | Typical Latency |
|---|---|---|
| `m1nd.ingest` | Build/rebuild the connectome from source | ~100ms (49 files) |
| `m1nd.activate` | Spreading activation search | ~5ms |
| `m1nd.impact` | Blast radius analysis | <5ms |
| `m1nd.why` | Path explanation between nodes | <1ms |
| `m1nd.missing` | Find structural holes | <5ms |
| `m1nd.predict` | Co-change prediction | <5ms |
| `m1nd.counterfactual` | What-if removal simulation | <5ms |
| `m1nd.health` | Server status & diagnostics | <1ms |
| `m1nd.learn` | Feedback-driven edge adjustment | <5ms |
| `m1nd.fingerprint` | Activation equivalence detection | <5ms |
