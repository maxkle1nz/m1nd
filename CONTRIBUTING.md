# Contributing to m1nd

Thanks for your interest in contributing to m1nd. This document covers the basics.

If you are contributing through real agent usage, also keep
[docs/AGENT-TASKNOTES.md](docs/AGENT-TASKNOTES.md) current. It is the running
capture surface for moments where an agent used `m1nd`, did not get the exact
answer it needed, and had to compensate outside the graph.

## Getting Started

```bash
git clone https://github.com/cosmophonix/m1nd.git
cd m1nd
cargo build
cargo test --all
```

## Project Structure

```
m1nd-core/     Graph engine, plasticity, spreading activation, hypothesis engine
m1nd-ingest/   Language extractors (28 languages), memory adapter, JSON adapter
m1nd-mcp/      MCP server, 77 tool handlers, JSON-RPC over stdio
```

---

## Crate Architecture

m1nd is a three-crate Rust workspace. Understanding what lives where saves you from
editing the wrong crate.

### m1nd-core

The graph engine. No I/O, no file system, no LLM calls. Pure computation.

Key modules:

| Module | Purpose |
|--------|---------|
| `graph.rs` | CSR adjacency, `NodeProvenance`, `Graph::finalize()` (required before any query) |
| `activation.rs` | Spreading activation, `HybridEngine` auto-selection, XLR noise cancellation |
| `plasticity.rs` | Hebbian LTP/LTD, `QueryMemory` ring buffer, homeostatic normalization |
| `temporal.rs` | `CoChangeMatrix`, `TemporalDecayScorer`, per-NodeType half-lives |
| `semantic.rs` | Trigram `CharNgramIndex`, `CoOccurrenceIndex` with PPMI, `SynonymExpander` |
| `resonance.rs` | Standing wave analysis, `HarmonicAnalyzer`, `SympatheticResonanceDetector` |
| `counterfactual.rs` | Cascade simulation, synergy analysis for multi-node removal |
| `topology.rs` | Community detection, bridge detection, `ActivationFingerprinter` LSH |
| `antibody.rs` | Bug immune memory — subgraph pattern matching with DFS + timeout budget |
| `flow.rs` | Particle-based concurrent execution simulation, race condition detection |
| `epidemic.rs` | SIR bug propagation model, `R0` estimation, burnout detection |
| `tremor.rs` | Second-derivative acceleration detection on edge weight time series |
| `trust.rs` | Actuarial per-module defect density, Bayesian prior adjustment |
| `layer.rs` | Tarjan SCC + BFS depth → architectural layer detection and violation reporting |
| `domain.rs` | `DomainConfig` — multi-domain presets: `code`, `music`, `memory`, `generic` |
| `builder.rs` | Fluent `GraphBuilder` API for constructing graphs programmatically |
| `snapshot.rs` | `save_graph()` / `load_graph()`, atomic write via temp + rename |
| `seed.rs` | 5-level `SeedFinder`: exact → prefix → substring → tag → fuzzy trigram |
| `types.rs` | `NodeType`, `EdgeType`, `PropagationConfig`, `DIMENSION_WEIGHTS`, newtypes |
| `error.rs` | `M1ndError` variants — all map to MCP error responses |
| `query.rs` | `QueryConfig` — `xlr_enabled`, `include_ghost_edges`, `GhostEdge` struct |
| `xlr.rs` | XLR differential processing math — `sigmoid_gate()`, `spectral_overlap()` |

**Design rule**: m1nd-core must compile with `no_std` ambitions. Keep stdlib use minimal
and confined to `snapshot.rs` / persistence paths.

### m1nd-ingest

File system walker, language extractors, graph construction pipeline. Depends on m1nd-core.

Key modules:

| Module | Purpose |
|--------|---------|
| `lib.rs` | `Ingestor` pipeline, `IngestConfig`, `IngestStats` |
| `walker.rs` | `DirectoryWalker` — binary detection, git history enrichment |
| `cross_file.rs` | Post-ingest `CrossFileResolver` — imports/tests/registers edges |
| `resolve.rs` | `ReferenceResolver` — multi-value index, import hint disambiguation |
| `diff.rs` | `GraphDiff` — incremental ingest engine (`DiffAction` enum) |
| `merge.rs` | `merge_graphs()` — tag union, max weight, provenance merge (powers `federate`) |
| `memory_adapter.rs` | `MemoryIngestAdapter` — markdown/text → memory graph |
| `json_adapter.rs` | `JsonIngestAdapter` — JSON descriptor → any-domain graph |
| `extract/tree_sitter_ext.rs` | `TreeSitterExtractor` — universal tree-sitter extractor, 22 languages |
| `extract/generic.rs` | Regex fallback for unsupported file types |

### m1nd-mcp

JSON-RPC stdio server. Tool dispatch, session state, protocol types. Depends on both crates.

Key modules:

| Module | Purpose |
|--------|---------|
| `main.rs` | Entry point, env/config loading, `./m1nd-mcp [config.json]` |
| `server.rs` | `tool_schemas()` — 77 tool registrations, tool dispatch (normalize → match) |
| `tools.rs` | Core tool handlers (ingest, activate, impact, learn, drift, ...) |
| `layer_handlers.rs` | Antibody, flow, epidemic, tremor, trust, layers handlers |
| `engine_ops.rs` | Shared engine helpers |
| `session.rs` | Multi-agent session state, `SharedGraph`, generation counters |
| `protocol/core.rs` | JSON-RPC types, request/response shapes |
| `protocol/layers.rs` | Protocol types for the 9 Superpowers Extended tools |
| `perspective_handlers.rs` | 12 perspective navigation handlers |
| `lock_handlers.rs` | 5 lock system handlers |
| `perspective/state.rs` | In-process perspective state machine |
| `perspective/peek_security.rs` | Allowlist enforcement — only files within ingest roots |
| `perspective/confidence.rs` | Suggestion confidence scoring |

---

## Adding New MCP Tools

Every tool follows the same dispatch pattern in `server.rs` + handler in the appropriate
`.rs` file.

### Step 1: Add the tool schema

In `m1nd-mcp/src/server.rs`, find `tool_schemas()`. Add a new entry:

```rust
ToolSchema {
    name: "m1nd.your_tool".to_string(),
    description: "One sentence. What it does and when to use it.".to_string(),
    input_schema: json!({
        "type": "object",
        "properties": {
            "agent_id": { "type": "string", "description": "Caller agent identifier" },
            // your parameters here
        },
        "required": ["agent_id"]
    }),
},
```

### Step 2: Add the dispatch arm

In `server.rs`, find the tool dispatch `match` block. The tool name is normalized
(dots → underscores, leading `m1nd_` stripped) before matching:

```rust
"your_tool" => handle_your_tool(&state, params).await,
```

### Step 3: Write the handler

Add your handler to the appropriate file. Use `engine_ops.rs` helpers for graph access.
The standard signature:

```rust
pub async fn handle_your_tool(
    state: &ServerState,
    params: serde_json::Value,
) -> Result<serde_json::Value, M1ndError> {
    let agent_id = params["agent_id"].as_str().ok_or(M1ndError::InvalidInput(...))?;
    let graph = state.graph.read();
    // ...
    Ok(json!({ "result": ... }))
}
```

### Step 4: Add a protocol type (optional)

If your tool returns a complex struct, add request/response types in
`m1nd-mcp/src/protocol/`. Mirror the naming convention of existing protocol files.

### Step 5: Tests

Add unit tests in the handler file and, if the tool touches core logic, integration
tests in `m1nd-core/src/` next to the module it exercises.

---

## Adding Language Extractors

m1nd has two tiers of tree-sitter language support plus a manual extractor path.

### Tier system

| Tier | Feature flag | Languages |
|------|-------------|-----------|
| Tier 1 | `--features tier1` | C/H, C++, C#, Ruby, PHP, Swift, Kotlin, Scala, Bash, Lua, R, HTML, CSS, JSON (14) |
| Tier 2 | `--features tier2` (default) | Tier 1 + Elixir, Dart, Zig, Haskell, OCaml, TOML, YAML, SQL (22 total) |

Tier 2 is the default build (`default = ["tier2"]` in `m1nd-ingest/Cargo.toml`).

### Adding a tree-sitter language (recommended path)

1. Find a `tree-sitter-<lang>` crate that depends on `tree-sitter-language` (new API),
   NOT the old `tree-sitter 0.19/0.20`. Crates that depend on the old API cause symbol
   collisions at link time and will silently return `None` from `parse()`.

2. Add the crate to `m1nd-ingest/Cargo.toml` as an optional dependency under the
   appropriate tier feature:

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
       name_field: "name",
       alt_name_fields: &[],
       name_from_first_child: false,
   }
   ```

   The `name_field` is the tree-sitter field used to extract a definition's name.
   Use `alt_name_fields` for languages with complex name positions (e.g., C declarators).
   Set `name_from_first_child: true` for languages like OCaml or TOML where the name
   is the first named child.

4. Gate the config behind `#[cfg(feature = "tier1")]` or `#[cfg(feature = "tier2")]`
   matching the tier you added it to.

### Adding a manual extractor

For languages where tree-sitter support is incomplete or you need deeper semantic
understanding, add a manual extractor in `m1nd-ingest/src/extract/`:

1. Create `your_lang.rs` implementing the extractor logic. Return `Vec<ExtractedNode>`
   and `Vec<ExtractedEdge>`.
2. Register the file extension in `m1nd-ingest/src/lib.rs` pipeline dispatch.
3. Existing examples: `m1nd-ingest/src/extract/` (Python, Rust, TypeScript, Go, Java).

---

## Memory Adapter

`m1nd-ingest/src/memory_adapter.rs` turns markdown and plain text files into a graph.
This is the path for AI agent memory, project wikis, and knowledge bases.

### How it works

The adapter parses `.md`, `.markdown`, and `.txt` files and creates nodes for:
- `file::` — the document itself
- `section::` — H1–H6 headings
- `entry::` — bullet points, checkboxes, table rows, plain text lines
- `reference::` — file paths cross-referenced in text

Entries are classified by keyword: `todo`/`task` → `Process` with tag `memory:task`;
`decision`/`decided` → `Concept` with tag `memory:decision`; etc.

Canonical source detection marks `YYYY-MM-DD.md`, `memory.md`, `*-active.md`,
`*-history.md`, and files containing `briefing` as `canonical=true` in provenance.

Node ID scheme:
```
memory::<namespace>::file::<file-slug>
memory::<namespace>::section::<file-slug>::<heading-slug>-<n>
memory::<namespace>::entry::<file-slug>::<line-no>::<entry-slug>
memory::<namespace>::reference::<path-slug>
```

### Using the adapter via MCP

Pass `adapter: "memory"` to `m1nd.ingest`:

```json
{
  "name": "m1nd.ingest",
  "arguments": {
    "path": "/path/to/notes/",
    "adapter": "memory",
    "namespace": "project-x",
    "agent_id": "your-agent"
  }
}
```

The `namespace` parameter scopes all node IDs (default: `"memory"`). Ingest multiple
note directories with different namespaces and they coexist in the same graph.

### Extending the adapter

To add a new content classification rule, edit the entry classification block in
`memory_adapter.rs`. Each rule matches keywords in entry text and maps to a
`(NodeType, tag, relation)` triple. The adapter uses the first matching rule,
with a default catch-all of `(Concept, "memory:note", "contains")`.

To add a new canonical source pattern, add to the `is_canonical()` function.

---

## Domain Configuration

`M1ND_DOMAIN` (env var) or the `domain` field in the config JSON controls which
`DomainConfig` preset is active. This affects temporal decay half-lives and which
edge types are considered meaningful for co-change analysis.

| Domain | Use case | git_co_change |
|--------|---------|---------------|
| `code` | Software codebases | true |
| `music` | Audio/DAW graphs | false |
| `memory` | Agent memory, wikis | false |
| `generic` | Any other graph | false |

New domain presets go in `m1nd-core/src/domain.rs`. Implement `DomainConfig::your_domain()`
and add it to the `from_str()` dispatch.

---

## Testing

### Unit tests

```bash
# All crates
cargo test --all

# Single crate
cargo test -p m1nd-core
cargo test -p m1nd-ingest
cargo test -p m1nd-mcp
```

Each module has inline tests at the bottom of the file (`#[cfg(test)] mod tests { ... }`).

### E2E tests

The `mcp/m1nd/` directory contains end-to-end test scripts that drive the server via
its JSON-RPC interface:

```bash
# Shell-based E2E
./test_e2e.sh
./test_mcp.sh
./test_perspective_e2e.sh

# Python-based scenarios
python3 test_layers_e2e.py
python3 test_advanced_usecases.py
python3 test_perspective_usecases.py
```

These scripts start the binary, send JSON-RPC calls over stdin, and assert on stdout.
They are the ground truth for behavioral correctness.

### Integration test guidelines

- New tools: add a test in the E2E shell script that exercises the happy path + one
  error case.
- New extractors: add a fixture file in the test corpus and assert on node/edge counts.
- Core algorithm changes: add both a unit test at the function level and an E2E test
  that exercises the full stack.

### Testing `apply` and `apply_batch` with `verify=true`

`m1nd.apply` and `m1nd.apply_batch` both accept an optional `verify` flag (v0.5.0+).
When `verify=true`, the server performs a post-write graph consistency check: it re-reads
the written file, confirms the content round-trips through ingest cleanly, and returns
a `verify` block in the response with `passed`, `node_delta`, and `edge_delta`.

When adding tests for tools that call `apply` or `apply_batch`, include a case that
sets `verify=true` and asserts on the `verify.passed` field:

```bash
# E2E: apply with verify
echo '{"method":"tools/call","params":{"name":"m1nd.apply","arguments":{
  "agent_id":"test","file_path":"/tmp/test_apply.py",
  "new_content":"def hello(): pass\n","verify":true
}}}' | ./m1nd-mcp | jq '.result.verify'
# Expected: {"passed": true, "node_delta": 1, "edge_delta": 0}

# E2E: apply_batch with verify
echo '{"method":"tools/call","params":{"name":"m1nd.apply_batch","arguments":{
  "agent_id":"test",
  "edits":[
    {"file_path":"/tmp/a.py","new_content":"x = 1\n"},
    {"file_path":"/tmp/b.py","new_content":"y = 2\n"}
  ],
  "verify":true
}}}' | ./m1nd-mcp | jq '.result.verify'
# Expected: {"passed": true, "files_verified": 2}
```

The `verify` flag is designed for CI and agent harnesses where silent write failures
are unacceptable. It adds ~1–3ms per file written (one ingest round-trip).

---

## Feature Flags

`m1nd-ingest/Cargo.toml` defines the tier system:

```toml
[features]
default = ["tier2"]
tier1 = [...]    # 14 tree-sitter languages
tier2 = ["tier1", ...]  # 8 more languages (default build)
```

`m1nd-mcp` has no additional feature flags — it inherits from m1nd-ingest via the
workspace dependency chain.

To build with only native extractors (smaller binary, faster compile):

```bash
cargo build --release --no-default-features
```

To build with only Tier 1:

```bash
cargo build --release --no-default-features --features tier1
```

The full Tier 2 build (default) produces the release binary shipped in
`target/release/m1nd-mcp`.

---

## What to Work On

### Language Extractors (high impact)

m1nd currently supports 22 languages via tree-sitter (Tier 1+2) plus Python, Rust,
TypeScript/JavaScript, Go, and Java via manual extractors. Adding more tree-sitter
grammars is the fastest path to expanding language coverage.

Before adding a grammar crate: verify it depends on `tree-sitter-language` (new API),
not `tree-sitter 0.19/0.20`. Old-API crates cause silent parse failures at runtime.

### Graph Algorithms

The core engine in `m1nd-core/` has room for improvement:
- Community detection algorithms
- Better spreading activation decay functions
- Smarter ghost edge inference
- Embedding-based semantic scoring (V1 is trigram-only)

### MCP Tools

New tools that leverage the graph are welcome. Each tool is a handler in `m1nd-mcp/src/`.
The pattern is consistent -- look at existing tools for the structure.

### Benchmarks

Run m1nd on your codebase and report performance. We track real-world numbers, not synthetic benchmarks.

---

## Code Standards

- `cargo fmt` before committing
- `cargo clippy -- -D warnings` must pass
- All new code needs tests
- No `unsafe` without a comment explaining why

## Pull Requests

1. Fork the repo and create a branch from `main`
2. Make your changes with tests
3. Ensure `cargo test --all` passes
4. Ensure `cargo clippy --all -- -D warnings` passes
5. Ensure `cargo fmt --all -- --check` passes
6. Open a PR with a clear description of what and why

## Issues

Use GitHub issues for bugs, feature requests, and questions. Label your issue:
- `bug` -- something doesn't work
- `enhancement` -- new feature or improvement
- `good first issue` -- suitable for new contributors
- `language-extractor` -- new language support
- `algorithm` -- graph algorithm work

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
