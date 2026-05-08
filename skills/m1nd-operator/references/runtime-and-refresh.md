# m1nd Runtime And Refresh Notes

This file captures the local installation facts, the source-of-truth docs that were studied, and how to refresh them later without re-deriving everything.

## Local Installation

Most hosts should point their MCP config at the same native binary:

- MCP server name: `m1nd`
- Binary name: `m1nd-mcp`
- Default launch args: `--stdio --no-gui`
- Optional env override: `M1ND_MCP_BINARY=/absolute/path/to/m1nd-mcp`
- Compatibility alias accepted by the probe helper:
  `M1ND_MCP_BIN=/absolute/path/to/m1nd-mcp`

Use the bundled helper script to verify the current runtime instead of trusting memory:

```bash
python3 scripts/probe_m1nd.py tools
python3 scripts/probe_m1nd.py call health '{"agent_id":"codex-m1nd"}'
```

## Official Sources Used

Primary source repo:

- [maxkle1nz/m1nd](https://github.com/maxkle1nz/m1nd)

Docs that mattered most:

- `README.md`
- `EXAMPLES.md`
- `docs/deployment.md`
- `docs/wiki/src/tool-matrix.md`
- `docs/wiki/src/faq.md`
- `docs/wiki/src/benchmarks.md`
- `docs/wiki/src/tutorials/quickstart.md`
- `docs/wiki/src/tutorials/first-query.md`
- `docs/wiki/src/tutorials/multi-agent.md`
- `docs/wiki/src/architecture/overview.md`
- `docs/wiki/src/architecture/graph-engine.md`
- `docs/wiki/src/architecture/ingest.md`
- `docs/wiki/src/architecture/mcp-server.md`
- `docs/wiki/src/concepts/spreading-activation.md`
- `docs/wiki/src/concepts/hebbian-plasticity.md`
- `docs/wiki/src/concepts/xlr-noise-cancellation.md`
- `docs/wiki/src/concepts/structural-holes.md`
- `docs/wiki/src/api-reference/overview.md`
- `docs/wiki/src/api-reference/activation.md`
- `docs/wiki/src/api-reference/analysis.md`
- `docs/wiki/src/api-reference/memory.md`
- `docs/wiki/src/api-reference/exploration.md`
- `docs/wiki/src/api-reference/perspectives.md`
- `docs/wiki/src/api-reference/lifecycle.md`

Reference timestamp for this study:

- Date: 2026-04-20
- Official repo commit checked locally: `d4a84000a3ae3b9848f8ce9505fab3ab00acd871`
- Local clone matched `origin/HEAD` at the time of study. Re-check the live
  runtime with `tools/list`, `trust_selftest`, or `scripts/probe_m1nd.py` before
  relying on exact tool counts.

## Important Truth Hierarchy

When sources disagree, use this order:

1. Live `tools/list` from the local binary
2. `tool-matrix.md`
3. API reference pages
4. README, tutorials, FAQ, prose pages

Reason: prose pages in the repo already contain stale counts in some places.

## Count Discrepancy To Remember

Official docs repeatedly describe the current surface as 93 tools.

The local binary on this machine returned 92 canonical tool names via `tools/list` on 2026-04-20.

Operational rule:

- never hardcode the count
- use the live runtime when counts or exact names matter
- use the docs for intent, workflow, and semantics

## What m1nd Is Best At

- graph-grounded structural retrieval
- blast-radius and hidden-neighbor analysis
- pre-flight validation for risky changes
- session continuity and cross-agent handoff
- document-to-code bindings
- long-lived structural monitoring

## What Still Belongs Elsewhere

- exact text lookup: `search` or `rg`
- one known file: `view` or direct file read
- compiler truth: compiler and tests
- runtime truth: logs, traces, debugger, profiler

## Live Probe Helper

The helper script is intentionally generic so future sessions can refresh the environment quickly.

Examples:

```bash
python3 scripts/probe_m1nd.py tools
python3 scripts/probe_m1nd.py call help '{"agent_id":"codex-m1nd","tool_name":"validate_plan"}'
python3 scripts/probe_m1nd.py call ingest '{"agent_id":"codex-m1nd","path":"/path/to/repo"}'
python3 scripts/probe_m1nd.py run '[{"name":"ingest","arguments":{"agent_id":"codex-m1nd","path":"/path/to/repo"}},{"name":"activate","arguments":{"agent_id":"codex-m1nd","query":"session management","top_k":5}}]'
```

Behavior:

- launches the configured local `m1nd-mcp` binary
- performs MCP `initialize`
- runs `tools/list` or `tools/call`
- `run` keeps one `m1nd` process alive across multiple calls so in-memory graph state survives `ingest -> query` flows
- prints parsed JSON instead of raw MCP envelopes when possible

## Refresh Procedure

When this skill starts feeling stale:

1. Re-clone or pull the official `maxkle1nz/m1nd` repo in a scratch workspace.
2. Re-read `tool-matrix.md`, `api-reference/overview.md`, `EXAMPLES.md`, `faq.md`, `benchmarks.md`, and any changed API pages.
3. Run `python3 scripts/probe_m1nd.py tools` against the local installed binary.
4. Update this skill only where the live runtime or official docs actually changed.

## External Restart Helper

When an agent sees `Transport closed`, an old binary version, stale host tools,
or repeated blocked retrieval after a good local graph, use an external restart
instead of trying to repair through the dead MCP binding:

```bash
m1nd restart --source /path/to/m1nd --yes
```

This command builds the latest source checkout, installs the native runtime to
the default m1nd binary path, and stops visible `m1nd-mcp` processes. It does
not refresh the host's cached tool list, choose a workspace, or ingest a graph.
After it runs, restart/rebind the host client and call `trust_selftest` or
`session_handshake` with the intended `scope`.

For live multi-agent sessions, prefer:

```bash
m1nd restart --source /path/to/m1nd --yes --no-kill
```

Then restart/rebind only the host that needs the new binary.

## Host-Specific Note For Codex

At the protocol level, `m1nd`'s canonical tool names are bare names like `activate` and `validate_plan`.

If Codex exposes the MCP server as first-class tools, the host wrapper may namespace them differently, but the underlying routing logic in this skill should still be based on the canonical tool semantics above.
