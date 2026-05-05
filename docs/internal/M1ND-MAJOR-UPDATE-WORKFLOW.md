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

Use the repo-local agent smoke harness as the default first pass:

```bash
python3 scripts/mcp_agent_smoke.py --repo . --json
python3 scripts/mcp_agent_smoke.py --repo . --transport http --json
```

For cheap session startup, use the official `trust_selftest` tool when the live
surface exposes it. It composes host-surface evidence, graph state,
`session_handshake`, and `recovery_playbook` into one verdict without ingesting,
repairing, mutating, or probing retrieval. If the selftest verdict is not
`full_trust`, follow its `recovery_playbook` or call `recovery_playbook` with
the same evidence before guessing the next action.

Use `session_handshake` as the cheaper sub-check or fallback when a host has not
refreshed to expose `trust_selftest`. The repo-local harness calls
`trust_selftest` when available, calls `session_handshake`, and falls back to
its local implementation for older binaries:

```bash
python3 scripts/mcp_agent_smoke.py --repo . --handshake-only --json
python3 scripts/mcp_agent_smoke.py --repo . --handshake-only --handshake-probe --json
```

The default selftest/handshake path is diagnostic-only: it inspects the host
tool surface and active graph state without ingesting, repairing, or probing
retrieval. `--handshake-probe` adds one tiny `seek` probe when the task depends
on retrieval trust.

`recovery_playbook` is also diagnostic-only. It returns ordered steps and a
binding fingerprint so agents can compare host, stdio, HTTP, runtime root,
graph path, generation counters, and ingest roots before blaming the graph.

If the host exposes `health` but not `trust_selftest`, `session_handshake`, or
`recovery_playbook`, inspect `health.tool_surface_contract` and
`health.host_binding_alignment`. That is the fallback proof that the host is
showing a partial tool surface rather than the full m1nd runtime contract.

This proves the minimum agent trust loop over real stdio framing and the HTTP
tool API:

```text
initialize -> tools/list -> trust_selftest -> session_handshake -> recovery_playbook when needed -> ingest -> seek -> help -> doctor
```

If these pass locally but a host-provided MCP binding fails the same flow, treat
the problem as host-binding/session continuity until proven otherwise. When a
live tool surface is available, call `doctor` with the suspicious tool output
before falling back to shell; it reports the active graph, runtime root,
workspace root, ingest roots, agent session, and stale-binding clues.

If `tools/list` itself is incomplete, treat it as a degraded host tool surface.
The critical recovery set is `ingest`, `seek`, `help`, and `doctor`. If any of
those are missing, call `doctor` when available with:

```json
{
  "agent_id": "codex-m1nd",
  "observed_tool": "tools/list",
  "observed_proof_state": "blocked",
  "observed_tool_count": 3,
  "available_tools": ["seek", "audit", "doctor"],
  "missing_tools": ["ingest"]
}
```

Without `ingest`, the agent cannot refresh or repair the active graph inside
that host session. Use m1nd as orientation only, cross-check with local files,
then restart or rebind the MCP surface.

Retrieval tools should make that recovery path explicit. When `seek`, `search`,
or `activate` returns `proof_state=blocked` or zero actionable candidates, the
response should include:

- compact `graph_state`;
- `next_suggested_tool=recovery_playbook`;
- `recovery.suggested_tool=recovery_playbook`;
- `recovery.arguments` copied directly into the `recovery_playbook` call.

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
