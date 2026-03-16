# Ingest Adapters

m1nd supports three ingestion adapters plus a federation tool for multi-repo ingestion. The `adapter` parameter in the MCP `m1nd_ingest` call selects the adapter:

| Adapter | `adapter` value | Input | Use case |
|---------|----------------|-------|---------|
| Code (default) | `"code"` or omit | Source files (27+ languages) | Software codebases |
| Memory | `"memory"` | `.md`, `.txt`, `.markdown` files | Agent memory, docs, wikis |
| JSON | `"json"` | A single JSON descriptor file | Any other domain |
| Federate | — | Two or more repo roots | Multi-repo unified graph |

All adapters produce a `(Graph, IngestStats)` and implement the same trait:

```rust
pub trait IngestAdapter: Send + Sync {
    fn domain(&self) -> &str;
    fn ingest(&self, root: &std::path::Path) -> M1ndResult<(Graph, IngestStats)>;
}
```

> **Note:** The MCP tool name uses underscores. Both `m1nd_ingest` and `m1nd.ingest` are accepted (the server normalizes dots to underscores before dispatch). Throughout this wiki, underscore form is canonical.

---

## Automatic Re-Ingest via apply and apply_batch

You rarely need to call `m1nd_ingest` directly after editing files. The surgical write tools handle re-ingestion automatically:

- **`m1nd_apply`** — after writing a single file, triggers incremental re-ingest for that file when `reingest=true` (default). The graph stays coherent with every write.
- **`m1nd_apply_batch`** — after writing multiple files atomically, triggers a single bulk re-ingest pass across all modified files when `reingest=true` (default). One ingest pass regardless of how many files were written.
- **`m1nd_apply_batch` with `verify=true`** — triggers re-ingest first (required for verification), then runs 5 independent verification layers (graph diff, anti-pattern scan, BFS blast radius, test execution, compile check) on the updated graph. The re-ingest is a prerequisite: verification reads the post-write graph state. See [Surgical-Context](Surgical-Context) for full details.

Call `m1nd_ingest` directly only when: (a) ingesting a codebase for the first time, (b) doing a full replace after bulk external changes, or (c) ingesting docs/memory with the `memory` adapter.

---

## Code Adapter

**Source:** `m1nd-ingest/src/lib.rs` pipeline + `extract/` directory

### Overview

The code adapter walks a directory, dispatches each file to a language extractor, resolves cross-file references, enriches edges with git co-change history, then finalizes the graph.

```
DirectoryWalker
  → per-file language extractor → Vec<ExtractedNode> + Vec<ExtractedEdge>
  → ReferenceResolver (import hint disambiguation)
  → CrossFileResolver (imports, test→impl, registers edges)
  → Git enrichment (co-change weights, velocity)
  → Graph::finalize() (CSR + PageRank)
```

### Language Support

27+ languages across three tiers, selected by Cargo feature flags:

| Tier | Build flag | Languages | Count |
|------|-----------|-----------|-------|
| **Built-in (native parsers)** | always on | Python, Rust, TypeScript/JavaScript, Go, Java | 5 |
| **Generic fallback** | always on | Any file with `def`/`fn`/`class`/`struct` patterns | ∞ |
| **Tier 1 (tree-sitter)** | `--features tier1` | C/H, C++, C#, Ruby, PHP, Swift, Kotlin, Scala, Bash, Lua, R, HTML, CSS, JSON | 14 |
| **Tier 2 (tree-sitter)** | `--features tier2` (default) | Elixir, Dart, Zig, Haskell, OCaml, TOML, YAML, SQL | 8 |
| **Total with tier2** | default build | Built-in + Tier 1 + Tier 2 | **28** |

Tier 2 is the default build (`default = ["tier2"]` in `m1nd-ingest/Cargo.toml`). The released binary ships with full Tier 2 support (27+ languages).

```bash
# Build with full language support (same as default)
cargo build --release --features tier1,tier2

# Minimal build — native extractors only, smaller binary, faster compile
cargo build --release --no-default-features
```

### Node ID Scheme (Code)

```
file::<relative/path.py>
file::<relative/path.py>::class::<ClassName>
file::<relative/path.py>::fn::<function_name>
file::<relative/path.py>::struct::<StructName>
file::<relative/path.py>::enum::<EnumName>
file::<relative/path.py>::module::<ModName>
```

Paths are relative to the ingest root and use forward slashes on all platforms.

### Adding a Tree-Sitter Language

1. Find a `tree-sitter-<lang>` crate that depends on `tree-sitter-language` (new API, not `tree-sitter 0.19/0.20` — old-API crates cause silent parse failures at runtime).

2. Add it to `m1nd-ingest/Cargo.toml` as an optional dependency under the appropriate tier:

   ```toml
   [features]
   tier1 = [..., "dep:tree-sitter-yourlang"]

   [dependencies]
   tree-sitter-yourlang = { version = "x.y", optional = true }
   ```

3. In `m1nd-ingest/src/extract/tree_sitter_ext.rs`, add a `LanguageConfig` entry:

   ```rust
   LanguageConfig {
       lang_tag: "yourlang",
       extensions: &["ext"],
       function_kinds: &["function_definition"],
       class_kinds: &["class_declaration"],
       struct_kinds: &[],
       enum_kinds: &[],
       type_kinds: &[],
       module_kinds: &["module"],
       import_kinds: &["import_statement"],
       name_field: "name",           // tree-sitter field for the definition's name
       alt_name_fields: &[],         // fallback fields for complex name positions
       name_from_first_child: false, // true for OCaml, TOML, etc.
   }
   ```

4. Gate the config behind `#[cfg(feature = "tier1")]` or `#[cfg(feature = "tier2")]`.

### Adding a Manual Extractor

For languages where tree-sitter is incomplete or you need deeper semantic understanding:

1. Create `m1nd-ingest/src/extract/your_lang.rs`. Return `Vec<ExtractedNode>` and `Vec<ExtractedEdge>`.
2. Register the file extension in `m1nd-ingest/src/lib.rs` pipeline dispatch.
3. Reference existing extractors in `m1nd-ingest/src/extract/` (Python, Rust, TypeScript, Go, Java) for the patterns.

---

## Memory Adapter

**Source:** `m1nd-ingest/src/memory_adapter.rs`

### Overview

The memory adapter turns markdown and plain-text files into a typed graph. It is the path for agent session memory, project wikis, PRDs, and any documentation corpus.

```jsonc
// MCP call
{
  "name": "m1nd_ingest",
  "arguments": {
    "path": "/path/to/notes/",
    "adapter": "memory",
    "namespace": "project-x",
    "agent_id": "your-agent"
  }
}
```

The `namespace` parameter scopes all node IDs (default: `"memory"`). Ingest multiple note directories with different namespaces and they coexist in the same graph without collision.

### Accepted File Types

`.md`, `.markdown`, `.txt`

Single files and directories are both accepted. When a directory is passed, all matching files are walked recursively.

### Node Types Produced

| Source element | `NodeType` | Example ID |
|---------------|-----------|-----------|
| Document file | `File` | `memory::ns::file::my-notes-md` |
| H1–H6 heading | `Module` | `memory::ns::section::my-notes-md::auth-system-1` |
| Bullet / checkbox / table row / plain line | `Concept` or `Process` | `memory::ns::entry::my-notes-md::14::use-m1nd-first` |
| File path cross-reference in text | `Reference` | `memory::ns::reference::backend-auth-py` |

### Node ID Scheme

```
memory::<namespace>::file::<file-slug>
memory::<namespace>::section::<file-slug>::<heading-slug>-<n>
memory::<namespace>::entry::<file-slug>::<line-no>::<entry-slug>
memory::<namespace>::reference::<referenced-path-slug>
```

Slugification: lowercase, alphanumeric + hyphens only, leading/trailing hyphens stripped. Heading counters (`-<n>`) handle duplicate headings within a document.

### Entry Classification

Entries are classified by keyword scan. The first matching rule wins:

```rust
fn classify_entry(text: &str) -> (NodeType, Vec<String>, String) {
    // "[ ]", "[x]", "todo", "task" → Process · tags: [memory:item, memory:task] · rel: "tracks"
    // "decision", "decided", "resolved" → Concept · tags: [..., memory:decision] · rel: "decided"
    // "mode", "state", "mood" → Concept · tags: [..., memory:state] · rel: "relates_to"
    // "meeting", "session", "today" → Process · tags: [..., memory:event] · rel: "happened_on"
    // default → Concept · tags: [..., memory:note] · rel: "contains"
}
```

To add a new classification rule: edit the `classify_entry` function in `memory_adapter.rs`. Each rule maps keywords to a `(NodeType, tags, relation)` triple.

### Canonical Source Detection

Some files receive a `canonical=true` provenance flag, which boosts their temporal score during activation. The `is_canonical_source()` function applies these patterns:

```rust
fn is_canonical_source(rel_path: &str) -> bool {
    // YYYY-MM-DD.md   — daily notes
    // memory.md       — core memory file
    // *-active.md     — active session state
    // *-history.md    — session history
    // files containing "briefing"
}
```

To add a new canonical pattern, extend `is_canonical_source()` in `memory_adapter.rs`.

### Document Kind Tags

Each file node receives a `kind:` tag:

| Filename pattern | Kind tag |
|-----------------|---------|
| `YYYY-MM-DD.md` | `kind:daily-note` |
| `memory.md` | `kind:long-term-memory` |
| anything else | `kind:memory-note` |

### Edge Weights

| Edge | Weight | Causal strength |
|------|--------|----------------|
| File → Section (`contains`, bidirectional) | 1.0 | 0.8 |
| Section/File → Entry (relation from classify_entry, forward) | 0.85 | 0.55 |
| Entry → Reference (`references`, forward) | 0.7 | 0.3 |

### Cross-File References

The adapter scans entry text for file path patterns matching common source extensions (`.rs`, `.py`, `.ts`, `.tsx`, `.js`, `.jsx`, `.json`, `.md`, `.toml`). Each match produces a `Reference` node and a `references` edge from the entry. This creates structural bridges between documentation and code when both are ingested into the same graph.

### Merge with Code Graph

The killer use case is ingesting code and docs into the same graph so a single query returns both the implementation and the specification:

```jsonc
// Step 1: ingest code
{"name": "m1nd_ingest", "arguments": {"path": "/project/backend", "agent_id": "dev"}}

// Step 2: merge docs on top
{"name": "m1nd_ingest", "arguments": {
  "path": "/project/docs",
  "adapter": "memory",
  "namespace": "docs",
  "mode": "merge",
  "agent_id": "dev"
}}

// Step 3: one query returns both
{"name": "m1nd_activate", "arguments": {"query": "antibody pattern matching", "agent_id": "dev"}}
// → pattern_models.py   (score 1.156) — implementation
// → PRD-ANTIBODIES.md   (score 0.904) — specification
// → CONTRIBUTING.md     (score 0.741) — guidelines
```

Empirically tested: 82 docs (PRDs, specs, notes) ingested in 138ms → 19,797 nodes, 21,616 edges, cross-domain queries working immediately.

---

## JSON Adapter

**Source:** `m1nd-ingest/src/json_adapter.rs`

### Overview

The JSON adapter is the escape hatch for any domain that does not need a custom extractor. Describe your graph in a simple JSON format, pass the file path with `adapter: "json"`, and m1nd builds a full typed graph.

```jsonc
// MCP call
{
  "name": "m1nd_ingest",
  "arguments": {
    "path": "/path/to/domain.json",
    "adapter": "json",
    "agent_id": "your-agent"
  }
}
```

### JSON Descriptor Format

```json
{
  "nodes": [
    {
      "id": "service::auth",
      "label": "AuthService",
      "type": "Module",
      "tags": ["critical", "security"]
    },
    {
      "id": "service::session",
      "label": "SessionStore",
      "type": "Module"
    }
  ],
  "edges": [
    {
      "source": "service::auth",
      "target": "service::session",
      "relation": "calls",
      "weight": 0.8
    }
  ]
}
```

All fields except `id` are optional:

| Field | Default | Notes |
|-------|---------|-------|
| `label` | same as `id` | Display name |
| `type` | `"Custom"` | See node type table below |
| `tags` | `[]` | Array of strings |
| `relation` | `"relates_to"` | Edge relation string |
| `weight` | `1.0` | Float in `[0.0, ∞)` |

### Supported Node Types

```
File, Directory, Function, Class, Struct, Enum, Type, Module,
Reference, Concept, Material, Process, Product, Supplier,
Regulatory, System, Cost
```

Unknown type strings map to `Custom(0)`. Node IDs are user-defined — the adapter uses them verbatim, so they must be unique within the descriptor.

### Causal Strength Inference

The adapter infers causal strength from the relation string since the JSON format has no explicit causal strength field:

```rust
let causal_strength = match edge.relation.as_str() {
    "contains"              => 0.8,
    "imports" | "depends_on" => 0.6,
    "implements"            => 0.7,
    "calls" | "routes_to"  => 0.5,
    "references"            => 0.3,
    _                       => 0.4,   // default for unknown relations
};
```

`contains` edges are also automatically made `Bidirectional`; all other relations are `Forward`.

### Example: Music Signal Chain

The adapter was designed to support non-code domains. From the test suite:

```json
{
  "nodes": [
    { "id": "room::studio_a", "label": "Studio A", "type": "System", "tags": ["room", "main"] },
    { "id": "bus::master", "label": "Master Bus", "type": "Process", "tags": ["bus"] }
  ],
  "edges": [
    { "source": "room::studio_a", "target": "bus::master", "relation": "routes_to", "weight": 1.0 }
  ]
}
```

Use the `music` domain preset (set via `M1ND_DOMAIN=music` or config file) alongside a JSON descriptor for best results on audio graphs — see [Domain-Presets.md](Domain-Presets.md).

---

## Merge Mode

The `mode` parameter in the `m1nd_ingest` call controls how the ingested graph interacts with the existing one:

| `mode` | Behavior |
|--------|---------|
| `"replace"` (default) | Clears the existing graph before ingesting |
| `"merge"` | Overlays new nodes and edges on top of the existing graph |

Merge semantics (from `merge.rs`):
- **Tags**: union of both sets
- **Edge weights**: max-wins (higher weight from either source is kept)
- **Provenance**: merged with existing provenance data

This is what powers both the code+docs cross-domain pattern and the `m1nd_federate` tool (which merges multiple repos into one graph).

---

## Federate — Multi-Repo Ingestion

**Source:** `m1nd-ingest/src/merge.rs` + `m1nd-mcp/src/tools.rs` (`handle_federate`)

### Overview

`m1nd_federate` ingests multiple repository roots and merges them into a single unified graph. It uses
the same `merge_graphs()` function that powers `mode: "merge"` in the `m1nd_ingest` call.

```jsonc
{
  "name": "m1nd_federate",
  "arguments": {
    "roots": ["/path/to/repo-a", "/path/to/repo-b"],
    "agent_id": "your-agent"
  }
}
// → Returns: unified graph statistics (total nodes, total edges, files per repo)
```

### Merge Semantics

When two graphs are merged (from `merge.rs`):

- **Tags**: union — both sets are preserved
- **Edge weights**: max-wins — the higher weight from either source is kept
- **Provenance**: merged — both source paths are tracked

If the same external node ID exists in both graphs, the merge updates that node in-place using
the above rules. Duplicate node IDs across repos are handled gracefully — they merge rather than error.

### Cross-Repo Edge Inference

Cross-repo edges are currently inferred from namespace matching. If module `auth.py` in repo A
has the same name as a module in repo B, m1nd infers a potential relationship.

Explicit cross-repo edge declaration (you specify which modules in repo A export to repo B) is
planned for v0.3 — see [Roadmap](Roadmap).

### Real Performance

```
Two repos federated: 1.3 seconds → 11,217 unified nodes
Incremental re-index after changes: 138ms
```

---

## Writing a Custom Adapter

If your domain needs more than the JSON escape hatch:

1. Create a new file in `m1nd-ingest/src/your_adapter.rs`
2. Implement `IngestAdapter`:

   ```rust
   pub struct YourAdapter { /* config */ }

   impl IngestAdapter for YourAdapter {
       fn domain(&self) -> &str { "your-domain" }

       fn ingest(&self, root: &std::path::Path) -> M1ndResult<(Graph, IngestStats)> {
           let start = std::time::Instant::now();
           let mut stats = IngestStats::default();
           let mut graph = Graph::with_capacity(estimated_nodes, estimated_edges);

           // Build nodes and edges...
           graph.add_node("your::id", "Label", NodeType::Module, &["tag"], timestamp, freq)?;
           graph.add_edge(src, tgt, "relation", FiniteF32::new(0.8),
                          EdgeDirection::Forward, false, FiniteF32::new(0.5))?;

           if graph.num_nodes() > 0 {
               graph.finalize()?;  // required — builds CSR + PageRank
           }
           stats.elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
           Ok((graph, stats))
       }
   }
   ```

3. Register it in the dispatch in `m1nd-ingest/src/lib.rs` (the `Ingestor` struct's adapter selection logic).
4. Add a `LanguageConfig` in `tree_sitter_ext.rs` if you also need source-level extraction.

**Key constraints:**
- `graph.finalize()` must be called before returning — it builds the CSR adjacency structure and computes PageRank. Queries on an unfinalized graph will fail.
- Use `FiniteF32::new(x)` for all float edge/weight values. It panics on NaN/Inf in debug builds and produces a soft error in release.
- Node IDs must be unique within a single ingest run. Duplicates return `M1ndError::DuplicateNode` and are skipped.
