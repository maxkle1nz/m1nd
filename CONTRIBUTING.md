# Contributing to m1nd

This guide covers the workspace layout, build flags, and the exact test and CI gates used on this repository. For coding-agent-specific operational rules (git identity, no-leak, write laws, ceremony boundaries), also read [AGENTS.md](AGENTS.md).

## Prerequisites

- **Rust (stable)** with `clippy` and `rustfmt` components
- **`cargo-nextest`** (canonical local Rust test runner)
- **Node.js 18+** (CI uses Node 22) for the root `@maxkle1nz/m1nd` wrapper and `m1nd-ui`
- **Python 3.12+** for proof harnesses (`tests/`) and CI gate scripts (`scripts/`)

If you work across parallel git worktrees, isolate each checkout's Cargo target directory before building or running tests:

```bash
export CARGO_TARGET_DIR="$(scripts/cargo_target_dir.sh)"
```

## Workspace layout

The Cargo workspace (`Cargo.toml`, resolver 2) has **five crates**:

| Crate | Version | Role |
|---|---|---|
| `m1nd-core/` | `2.0.0` | In-memory graph engine, spreading activation, plasticity, temporal/semantic/topology/trust/calibration primitives |
| `m1nd-ingest/` | `2.0.0` | Directory walker, 5 manual extractors (Python, TypeScript/JavaScript, Rust, Go, Java), 22 tree-sitter languages, document/memory/JSON/L1GHT adapters, cross-file resolution, incremental diff |
| `m1nd-mcp/` | `2.0.0` | MCP server (stdio, loopback HTTP/REST + embedded UI, `--attach` bridge), 147 registered tools, 15-tool default core menu, per-brain actor runtime, single-writer lease |
| `m1nd-control/` | `0.2.4` | Control-plane contracts, action catalog, Ed25519/P-256 verification, authority WAL |
| `m1nd-runnerd/` | `1.6.5` (`publish = false`) | Dedicated runner daemon for pinned spawn missions in isolated worktrees |

Outside the Rust workspace:

- `npm/` and root `package.json` — dependency-free `@maxkle1nz/m1nd` (`2.0.0`) host installer and agent CLI (`m1nd`, `m1nd-north-shim`).
- `m1nd-ui/` — Vite/React web UI (`m1nd-ui/dist/` is committed and embedded into `m1nd-mcp` via `rust-embed`).
- `skills/` — agent skill packs installed into Claude Code, Codex, Antigravity, and other hosts.
- `tests/` — Python proof harnesses and documentation/CI contract tests.
- `docs/` (including `docs/wiki/` mdBook source), `scripts/`, and `m1nd-demo/`.

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for the checkout-private development kit (`scripts/m1nd-dev`).

## Building and feature flags

```bash
# Default workspace build
cargo build --locked --workspace

# Fast incremental dev profile (defined in Cargo.toml)
cargo build --locked --profile dev-fast --workspace

# Release workspace build
cargo build --locked --release --workspace
```

### `m1nd-mcp` features (`m1nd-mcp/Cargo.toml`)

- `default = ["serve", "embed"]`
- `serve` — Axum HTTP/REST server, embedded UI, and `--attach` / `--discover-owner` client.
- `embed` — local static embeddings via `m1nd-core/embed` (`model2vec-rs`).

```bash
# Lean stdio-only edge (also checked in CI)
cargo check --locked -p m1nd-mcp --no-default-features

# Served binary without local static embeddings
cargo build --locked --release -p m1nd-mcp --no-default-features --features serve
```

### `m1nd-ingest` features (`m1nd-ingest/Cargo.toml`)

- `default = ["tier2"]`
- `tier1` — 14 routed tree-sitter languages (C, C++, C#, Ruby, PHP, Swift, Kotlin, Scala, Bash, Lua, R, HTML with embedded JS/CSS, CSS, JSON) plus tree-sitter Rust/JavaScript support.
- `tier2` — `tier1` plus 8 languages (Elixir, Dart, Zig, Haskell, OCaml, TOML, YAML, SQL), 22 routed tree-sitter languages total.

```bash
# Manual extractors only (Python, TypeScript/JavaScript, Rust, Go, Java + generic fallback)
cargo build --locked -p m1nd-ingest --no-default-features

# Tier 1 only
cargo build --locked -p m1nd-ingest --no-default-features --features tier1
```

## Running tests and gates

CI classifies every pull request with `scripts/ci_scope.py` and runs the selected gates in `.github/workflows/ci.yml` (`docs/TEST-PORTFOLIO.md` explains the path mapping). Run the gates that match your changes locally before opening a PR.

### Rust gates

Fast day-to-day signal over the core invariant profile (not a substitute for the full merge gate):

```bash
bash scripts/lightning_check.sh
```

Full local Rust proof:

```bash
cargo nextest run --locked --workspace --all-targets
cargo test --locked --workspace --doc
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo check --locked -p m1nd-mcp --no-default-features
```

Notes:
- `cargo test --locked --workspace --doc` is required in addition to `nextest` because `nextest` does not execute doctests; `m1nd-mcp` carries 13 `compile_fail` doctests guarding crate-internal boundaries.
- CI's `rust-full` job runs `cargo test --locked --workspace --all-targets` on Ubuntu, macOS, and Windows (plus `cargo build --locked --release --workspace` on pushes to `main`).
- Scoped PR runs (`rust-scoped`) execute `scripts/lightning_check.sh`, `scripts/rust_pr_check.py`, and `cargo fmt --all --check` on Ubuntu and Windows.

### npm host, pack, and native cache gates

The root `package.json` must remain dependency-free (`dependencies`, `devDependencies`, and `optionalDependencies` empty).

```bash
npm test
npm run m1nd:pack-check
npm run m1nd:pack-routing-check
npm pack --dry-run
```

To exercise the real-binary cache and concurrency tests run by CI's `agent-cache-real-gates` job:

```bash
cargo build --locked -p m1nd-mcp
M1ND_TEST_AGENT_CACHE_BINARY="$(pwd)/target/debug/m1nd-mcp" \
  node --test --test-concurrency=1 \
  npm/test/agent-cli-cache-real.test.js \
  npm/test/agent-cli-cache-concurrency-real.test.js
```

### Web UI gates (`m1nd-ui/`)

`m1nd-ui/dist/` is tracked in git and embedded into `m1nd-mcp`. Commit rebuilt `dist/` output alongside any UI source change; CI fails if a clean build differs.

```bash
cd m1nd-ui
npm ci
node --test ../scripts/devkit/ui-dev.test.mjs
npm test
npm run lint:soft
npm run lint
npm run build
git status --porcelain --ignored=matching -- dist   # must be empty
```

When touching UI shell chrome, navigation, landmarks, or labels, also run the separate browser suites (install Playwright Chromium once with `npx playwright install --with-deps chromium`):

```bash
cd m1nd-ui
npm run test:e2e
npm run test:e2e:a11y
```

### Python proof harnesses

```bash
python3 -m unittest discover -s tests -p 'test_*.py' -v
```

### Security, frozen contracts, and documentation coupling

These run on every PR in `.github/workflows/ci.yml`:

```bash
# Candidate source boundary guard (rejects private, secret, or oversized paths)
python3 scripts/m1nd10_candidate_source_guard.py --repo . --revision HEAD

# Frozen M1ND-10 contract checksums (must match ci.yml)
printf '%s  %s\n' \
  '2745560daf6e5cf6237b84663f895e81e2c4979de4190dfef649b032b680f87b' 'docs/M1ND-10-PRD.md' \
  'd5bc29776f516c300cb1a0668f0a53844286f75395fd0ad1e875b52ea3a067a5' 'docs/M1ND-10-UML.md' \
  | shasum -a 256 -c -

# Agent-workflow documentation coupling gate
python3 scripts/agent_docs_gate.py --base-ref main
```

CI additionally runs `gitleaks` over full commit history, `cargo audit --file ./Cargo.lock` on full coverage, and `npm audit --audit-level=high` inside `m1nd-ui/` when UI dependencies change.

## Adding MCP tools

1. **Schema (`m1nd-mcp/src/server.rs`):** register the tool in `all_tool_schemas_inner()`. Every tool's `inputSchema` must declare a top-level `"type": "object"` (enforced by `every_tool_input_schema_is_top_level_object`).
2. **Policy route (`m1nd-mcp/src/action_routes.rs`):** add the verb to `MCP_TOOL_ROUTE_NAMES` and map it to its semantic action and authority floor; parity tests enforce exact equality with the tool registry.
3. **Dispatch & handler:** wire dispatch in `m1nd-mcp/src/server.rs` and implement the handler (plus any protocol types in `m1nd-mcp/src/protocol/`). If the verb has custom REST interception ahead of the generic floor gate, declare it in `REST_ROUTE_SEATING` (`m1nd-mcp/src/http_server.rs`).
4. **Menu discipline:** new verbs enter the 147-tool registry and remain discoverable via `help`; adding a verb to the 15-tool default core menu (`CORE_TOOLS` / `core_menu_tool_names()`) is a separate, deliberate decision.
5. **Docs coupling:** `scripts/agent_docs_gate.py` requires any change to agent-workflow surfaces (`server.rs`, `tools.rs`, `protocol/`, `help_guidance.rs`, `universal_docs.rs`, `skills/`, `npm/bin/`, `npm/lib/{cli,agent-cli,agent-schemas}.js`) to also update an agent-facing doc (`skills/`, `docs/`, `README.md`, `CONTRIBUTING.md`, or `AGENTS.md`), unless the PR carries the `agent-docs-exempt` label for a pure refactor.

## Adding language extractors

- **Tree-sitter grammar (preferred):** choose a crate using the new `tree-sitter-language` API (not legacy `tree-sitter` 0.19/0.20, which causes C symbol collisions and silent parse failures). Add it as an optional dependency in `m1nd-ingest/Cargo.toml` under `tier1` or `tier2`, add its `LanguageConfig` in `m1nd-ingest/src/extract/tree_sitter_ext.rs`, and route its extensions in `m1nd-ingest/src/lib.rs`.
- **Manual extractor:** add a module in `m1nd-ingest/src/extract/` (see `python.rs`, `rust_lang.rs`, `typescript.rs`, `go.rs`, `java.rs`) and dispatch its extensions in `m1nd-ingest/src/lib.rs`.
- **Domain presets:** `m1nd-core/src/domain.rs` defines `code`, `music`, `memory`, `generic`, and `light`.

## Code standards and pull requests

- Format Rust code with `cargo fmt` and fix all Clippy warnings (`-D warnings`).
- Add unit or integration tests for new behavior; use neutral fixture names (`repo-alpha`, `tempfile` dirs) and never include personal paths, private hostnames, or secrets.
- Document every `unsafe` block with a `// SAFETY:` comment explaining its invariant.
- Use Conventional Commits (`feat:`, `fix:`, `docs:`, `chore:`) with English messages describing what and why. PRs are squash-merged by default.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
