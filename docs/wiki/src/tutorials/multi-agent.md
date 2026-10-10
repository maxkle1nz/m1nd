# Share One Local Owner

Use one served owner for a repository that has already been prepared through [Quick Start](quickstart.md). Use the exact `store_dir` returned by the birth receipt as the owner's `--runtime-dir` (the placeholder below is the nested runtime from the published quick start). Attach each agent from that repository rather than starting several writers against the same runtime directory.

```bash
export M1ND_BINARY="$HOME/.local/share/m1nd/versions/1.6.3/m1nd-mcp"
"$M1ND_BINARY" --serve --no-gui --port 1337 --runtime-dir /absolute/path/to/project/.m1nd
# Each MCP client, launched from the intended repository:
"$M1ND_BINARY" --attach auto --stdio
```

The owner holds the graph and writer lease. Attached clients share its context and memory; they do not load independent mutable copies of its persisted state. Auto-discovery must find an owner that already covers the caller's repository. A mismatch is a reason to stop writes and reconnect, not to replace another graph.

## Give sessions distinct identities

Use stable, task-appropriate `agent_id` values such as `review-auth` and `implement-config`. An agent ID helps attribute tool activity and state ownership; it is not authentication or permission to impersonate another session.

Start each task with `north`. Inspect binding, prior memory, task context, and any presence or overlap notices. When working sets overlap, coordinate file ownership before editing. Presence is activity visible to m1nd and can expire; absence from the roster does not prove no other process is writing.

## Share verified findings

A finding recorded through `memorize` can be recalled by another attached session. Include evidence paths and distinguish a verified fact from a hypothesis. The next reader checks freshness and provenance rather than inheriting confidence blindly.

Trails and perspectives can support longer investigations. Discover them through `help`; a hidden menu entry may still be callable, while mutation and authority requirements remain in force.

## Keep safety boundaries separate

- Overlap notices are advisory; they do not lock source files.
- Shared graph context does not replace independent review or the repository's tests.
- Human-only birth, ratification, receipt, and custody operations remain human-only.
- A loopback owner is not an authenticated remote team service. Non-loopback binds are refused.

See [deployment](https://github.com/maxkle1nz/m1nd/blob/main/docs/deployment.md) for persistent startup and routing, and the [API reference](../api-reference/overview.md) for tool-specific contracts.
