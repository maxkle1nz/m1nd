# m1nd Major Update Workflow

Canonical workflow for shipping a large `m1nd` update after substantial
tool-surface, architecture, or docs changes.

This is meant for real product updates, not tiny patch fixes.

## Goals

- keep the live MCP surface, docs, and release story in sync
- validate from the perspective of an agent, not only from unit tests
- turn friction from real usage into the next improvement loop
- make release/bump/deploy a repeatable build discipline

## Phase 1 — Build the feature branch

1. Start from a clean `origin/main`.
2. Build on a dedicated branch.
3. Validate locally during development:
   - `cargo fmt --check`
   - `cargo check`
   - focused tests while iterating
4. If agent usage reveals friction, capture it immediately in:
   - `docs/AGENT-TASKNOTES.md`

## Phase 2 — Validate like an agent

Do not stop at unit tests.

Run real usage validation through the actual MCP server surface:

1. build the binary:

```bash
cargo build -p m1nd-mcp
```

2. run MCP stdio smoke(s):
   - one code-heavy repo
   - one doc-heavy / coordination-style repo
3. verify at least:
   - `tools/list`
   - top-level entrypoint tool(s) for the update
   - expected profile/behavior selection
   - graph-vs-disk truth where relevant
   - truncation / large-output behavior where relevant

If the agent still needs shell fallback for something the product should answer,
add a tasknote before moving on.

## Phase 3 — Surface parity

Before release, make sure the public story matches the real registry.

Update together:

- `README.md`
- top-level localized `README.*`
- `.github/wiki/` entry pages
- `docs/wiki/src/` source pages
- `CHANGELOG.md`
- `CONTRIBUTING.md` if the workflow changed

If wiki source changes, regenerate the published build:

```bash
mdbook build docs/wiki
rsync -a --delete wiki-build/ docs/wiki-build/
rm -rf wiki-build
```

Then grep for stale public claims:

```bash
rg -n "61 MCP tools|63 MCP tools|64 MCP|71 MCP|61 tool handlers|61 tool registrations|43 tool definitions" \
  README* .github/wiki docs/wiki/src docs/wiki-build CONTRIBUTING.md CHANGELOG.md
```

Historical release notes are allowed to keep historical counts.
Current-surface pages are not.

## Phase 4 — Full validation gate

Run the full gate on the touched crates:

```bash
cargo fmt --check
cargo check -p m1nd-mcp -p m1nd-ingest
cargo test -p m1nd-ingest -p m1nd-mcp -- --nocapture
cargo clippy -p m1nd-mcp -p m1nd-ingest -- -D warnings
```

If the update changed release/build surfaces, also run the relevant binary build:

```bash
cargo build --release --workspace
```

## Phase 5 — PR and merge discipline

Open a PR that includes:

- what changed
- why this matters for real agent use
- what was validated locally
- what was validated through real MCP smoke
- what still remains intentionally out of scope

Wait for GitHub Actions to pass before merge.

## Phase 6 — Release preparation

If the update materially changes public capability, treat it as release work.

### Version guidance

- patch bump: narrow fixes, no meaningful public-surface expansion
- minor bump: new MCP tools, new workflows, meaningful public capability growth
- major bump: breaking public contract or deep architecture reset

For a capability wave like new tools + new entrypoint + new docs surface, prefer
a **minor** bump.

### Release checklist

1. decide target version
2. update crate versions consistently
3. update `CHANGELOG.md`
4. ensure release workflow still matches shipped binaries/crates
5. tag only after `main` is green

Current release automation lives in:

- `.github/workflows/release.yml`
- `.github/workflows/deploy-wiki.yml`

## Phase 7 — Post-merge follow-through

After merge:

1. watch `main` CI
2. watch docs/site deploy
3. add any new friction found during post-merge smoke to:
   - `docs/AGENT-TASKNOTES.md`
4. turn those notes into the next branch instead of leaving them in chat history

## Canonical rule

Large `m1nd` updates are not complete when:

- code is merged
- tests are green

They are complete when:

- the live MCP surface is correct
- the public docs are aligned
- the built docs are aligned
- the release story is ready
- agent friction has been captured for the next evolution loop
