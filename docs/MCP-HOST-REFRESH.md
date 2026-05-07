# MCP Host Refresh

Use this when the repo-local `m1nd-mcp` binary works, but a client such as
Codex, Claude, Cursor, Windsurf, Cline, or another MCP host still exposes an
older or partial tool surface.

The most common symptom is simple:

- local smoke sees `trust_selftest`, `ingest`, `seek`, `help`, `recovery_playbook`, and `doctor`
- the host session only sees some of those tools
- retrieval looks blocked or stale even after ingest

Another common symptom is more abrupt:

- a tool call fails with `Transport closed`

That is not graph staleness. It means the MCP transport died before m1nd could
execute a tool. Recovery tools such as `doctor`, `recovery_playbook`, and
`ingest` cannot run through that closed transport. Prove the binary locally,
restart/rebind the host MCP client or open a fresh session, then call
`trust_selftest` or `session_handshake` on the newly launched binding.

## 1. Prove The Local Binary First

From the repo root:

```bash
cargo build -p m1nd-mcp
python3 scripts/m1nd_agent_demo.py --repo . --transport stdio --json
python3 scripts/m1nd_agent_demo.py --repo . --transport http --json
```

Both outputs should report:

- `schema=m1nd-agent-first-demo-v0`
- `trust.verdict=full_trust`
- `checks.trust_selftest_full_trust=true`
- `checks.seek_scanned_ingested_graph=true`
- `checks.negative_trust_selftest_validated=true`

If these fail, fix the local binary or repo ingest path first. Do not blame the
host binding yet.

## 2. Compare The Host Tool Surface

In the client, inspect its `tools/list` result. A safe host surface should expose
at least:

```text
health
trust_selftest
session_handshake
recovery_playbook
doctor
ingest
seek
help
```

If `trust_selftest` is visible, call it first:

```json
{"agent_id":"dev"}
```

If it returns `full_trust`, continue with m1nd-first work. If it returns any
other verdict, follow the embedded `recovery_playbook` or call
`recovery_playbook` with the same evidence.

## 3. When Only `health` Is Visible

Some hosts can still expose `health` while hiding newly added tools. In that
case, call `health` and inspect:

- `tool_surface_contract.required_host_visible_tools`
- `host_binding_alignment`
- `binding_fingerprint`

If the contract requires tools the host does not list, treat the session as
`degraded_host_tool_surface`. Use the local demo output as runtime truth and
verify final answers against files until the client refreshes its MCP binding.

## 4. Refresh The Client Binding

Recommended order:

1. Rebuild `m1nd-mcp`.
2. Confirm the MCP config points at the rebuilt binary.
3. Set `M1ND_WORKSPACE_ROOT` to the intended repo/workspace when the host lets
   you configure environment variables.
4. Restart the MCP server process if the host manages it separately.
5. Reload or restart the client window/session.
6. Re-run `tools/list`.
7. Call `trust_selftest`.

If the error is `Transport closed`, the old binding is already gone. Skip graph
recovery calls in that session and relaunch the host binding first.

For hosts that cache tool schemas per conversation or workspace, start a new
conversation/session after rebuilding the binary. The old conversation may keep
the previous tool registry even though the local binary is correct.

## 5. Recovery Payload For Agents

When a host surface is suspicious, pass the host evidence into `trust_selftest`
or `recovery_playbook`:

```json
{
  "agent_id": "dev",
  "observed_tool": "tools/list",
  "observed_proof_state": "blocked",
  "observed_tool_count": 3,
  "available_tools": ["health", "seek", "doctor"],
  "missing_tools": ["trust_selftest", "ingest", "recovery_playbook"]
}
```

For blocked retrieval after a populated ingest:

```json
{
  "agent_id": "dev",
  "observed_tool": "seek",
  "observed_proof_state": "blocked",
  "observed_candidates": 0
}
```

The important rule is to compare binding fingerprints before falling back to
manual search. If the local stdio/HTTP demo and the host session disagree, the
problem is likely host binding freshness, not the graph model itself.

## Limits

This guide does not force any client to reload its MCP registry. It gives the
agent a deterministic way to classify the session, preserve evidence, and avoid
treating a stale host surface as a failed graph.
