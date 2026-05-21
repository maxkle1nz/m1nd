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
m1nd agent scope --repo /path/to/repo --json
m1nd agent trust --repo /path/to/repo --ensure-ingest --json
m1nd agent orient --repo /path/to/repo --query "focused subsystem or bug surface" --mode short --json
python3 scripts/probe_m1nd.py tools
python3 scripts/probe_m1nd.py call help '{"agent_id":"codex-m1nd","tool_name":"validate_plan"}'
python3 scripts/probe_m1nd.py call ingest '{"agent_id":"codex-m1nd","path":"/path/to/repo"}'
python3 scripts/probe_m1nd.py run '[{"name":"ingest","arguments":{"agent_id":"codex-m1nd","path":"/path/to/repo"}},{"name":"activate","arguments":{"agent_id":"codex-m1nd","query":"session management","top_k":5}}]'
```

Behavior:

- launches the configured local `m1nd-mcp` binary
- injects a temporary isolated `--runtime-dir` by default so independent agent
  probes can run concurrently without runtime owner-lock collisions
- performs MCP `initialize`
- runs `tools/list` or `tools/call`
- `run` keeps one `m1nd` process alive across multiple calls so in-memory graph state survives `ingest -> query` flows
- prints parsed JSON instead of raw MCP envelopes when possible
- use `--shared-runtime` only when you intentionally need to inspect shared
  runtime state; if you see `runtime_root ... is already owned by instance`,
  rerun with the current helper or a unique explicit `--runtime-dir` before
  diagnosing graph/retrieval health

## Refresh Procedure

When this skill starts feeling stale:

1. Re-clone or pull the official `maxkle1nz/m1nd` repo in a scratch workspace.
2. Re-read `tool-matrix.md`, `api-reference/overview.md`, `EXAMPLES.md`, `faq.md`, `benchmarks.md`, and any changed API pages.
3. Run `python3 scripts/probe_m1nd.py tools` against the local installed binary.
4. Update this skill only where the live runtime or official docs actually changed.

## External Self-Update Helper

When an agent sees `Transport closed`, an old binary version, stale host tools,
or repeated blocked retrieval after a good local graph, use the external
self-update CLI instead of trying to repair through the dead MCP binding:

```bash
m1nd update check --channel beta
m1nd update status --channel beta
m1nd update plan --channel beta
m1nd update apply --channel beta --yes
m1nd hosts status --host all --project /path/to/project --json
m1nd hosts plan --host all --project /path/to/project --json
m1nd hosts apply --host all --project /path/to/project --yes --json
```

`check`, `status`, `plan`, `hosts status`, and `hosts plan` are read-only.
`hosts status` is the host-readiness cockpit: it reports agent-pack files,
likely MCP config wiring, runtime/PATH alignment, workspace hints, and
`host_rebind_proven=false` per supported host. `hosts plan` is the recipe layer:
it emits install, MCP-config, `M1ND_WORKSPACE_ROOT`, rebind, and verification
steps without editing host files. `hosts apply` is the host-local mutation step:
without `--yes` it stays a dry-run preview; with `--yes` it can install or
refresh agent-pack files and write canonical MCP config snippets for known
hosts, but generic-host config stays manual. `update apply` is the runtime and
package mutation step: it mutates only with `--yes`, updates the npm package
when the selected channel is ahead, installs the native runtime from a GitHub
Release binary when available, falls back to Cargo when needed, records a
runtime backup for rollback, and can stop visible `m1nd-mcp` processes. Neither
mutating command refreshes the host's cached tool list, chooses a workspace,
ingests a graph, or fixes semantic retrieval. After either mutating command
runs, restart/rebind the host client and call `trust_selftest` or
`session_handshake` with the intended `scope`.

If a host config points to an absolute current managed runtime, a stale
`m1nd-mcp` on `PATH` is only a shadow warning, not proof the host is stale. If
the host launches `PATH` or the config target is unknown, stale `PATH` is
actionable. Verify that distinction with `hosts status` first and `hosts plan`
when needed, then rebind or open a fresh host session. Do not claim the
client's cached tool list refreshed until that new host session is actually
running.

For live multi-agent sessions, prefer:

```bash
m1nd update apply --channel beta --yes --no-kill
```

Then restart/rebind only the host that needs the new binary. `m1nd restart
--source /path/to/m1nd --yes` remains the lower-level source-checkout repair
path for development builds.

## Host-Specific Note For Codex

At the protocol level, `m1nd`'s canonical tool names are bare names like `activate` and `validate_plan`.

If Codex exposes the MCP server as first-class tools, the host wrapper may namespace them differently, but the underlying routing logic in this skill should still be based on the canonical tool semantics above.
