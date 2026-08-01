# m1nd Deployment & Production Setup

`m1nd` can run as a per-host stdio process (the IDE starts and stops it), but each such
process loads its own graph — which can exceed 100MB — into RAM on every launch. For
always-on use, run **one persistent owner** that keeps the graph resident, and point every
IDE/agent at it through the **native attach bridge** (`m1nd-mcp --attach`): a thin
stdio↔HTTP client that loads no graph, builds no engines, and takes no lease.

> **Migrating from the Python proxy.** Earlier releases used a Python stdio-to-HTTP proxy
> (`scripts/macos/m1nd-proxy.py`). That lane is superseded by `m1nd-mcp --attach` — no
> separate script, the same runtime on both ends. If you still point an IDE at
> `m1nd-proxy.py`, switch it to the attach block in §2.

## Architecture

1. **Persistent owner (`m1nd-mcp --serve`)** — runs constantly, keeping the graph in RAM
   for sub-millisecond queries. Default port **1337**; add `--open` (and drop `--no-gui`)
   to open the served web UI.
2. **Boot manager** — a launchd LaunchAgent (macOS) or a systemd unit (Linux) that starts
   the owner on boot and restarts it if it crashes.
3. **Native attach bridge (`m1nd-mcp --attach`)** — each IDE/agent runs this instead of a
   graph-loading server; it forwards every JSON-RPC frame to the owner's `POST /mcp` and
   relays the owner's push notifications back. `--attach auto` discovers the owner from the
   instance registry — by its lease, and failing that by what it has ingested (§2).
4. **Incremental ingest** — the owner updates the graph as files change, skipping noise
   (`node_modules`, `Pods`, Rust `target/`).

## 0. The first graph — one human command, once per repo

Everything below assumes a graph exists. Creating one is the single gesture no agent may
perform on any transport: generic `ingest` classifies as `graph.ingest.replace` at the
`POSITIVE_SOVEREIGN` floor and is refused for every client, on MCP and REST alike. The
door is the birth ceremony, and it is a flag on the binary — no header, payload or claimed
origin can reach it, because the ingress *is* the human-origin fact:

```bash
m1nd init --birth /path/to/repo        # or: m1nd-mcp --birth /path/to/repo
```

It ingests the repo, prints `node_count`/`edge_count` and exits 0; it exits 1 and names
what to check when the scan produces nothing, so it can never report success over an empty
graph. Which brain it fills depends on where the runtime it is standing in lives, and it
says which one it chose in the `brain` field of its receipt:

| Runtime | `brain` | Where the graph lands | Who reads it |
|---|---|---|---|
| Inside the repo you named (`<repo>/.m1nd`, the solo setup) | `owner_bound_graph` | that runtime's own graph | the next `m1nd-mcp --stdio` in that repo, directly |
| Elsewhere (a served owner, §1) | `project_brain` | `<runtime>/project-brains/<key>/` | agents that **attach** to that owner from inside the repo (§2) |

The second row is the one to get right in a served deployment: a hosted brain is reached
through the owner's caller-root routing, so an agent that starts its own stdio runtime in
that repo would see an empty graph and conclude the ceremony did nothing. Attach instead —
the receipt's `reach_it_with` field prints the exact command.

Born once, kept fresh afterwards by the agent: `ingest {mode:"refresh"}` from that exact
root re-scans a root the brain already declares, at `SCOPED_GRANT_A2`, with no lease.

## 1. Run the persistent owner

### macOS — launchd

Create `~/Library/LaunchAgents/world.m1nd.mcp-server.plist` (a generic template ships at
[`scripts/macos/world.m1nd.mcp-server.plist`](../scripts/macos/world.m1nd.mcp-server.plist)):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>world.m1nd.mcp-server</string>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/<name>/.m1nd/bin/m1nd-mcp</string>
        <string>--serve</string>
        <string>--no-gui</string>
        <string>--port</string>
        <string>1337</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>M1ND_RUNTIME_DIR</key>
        <string>/Users/<name>/.m1nd</string>
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
```

Load it:
```bash
launchctl load ~/Library/LaunchAgents/world.m1nd.mcp-server.plist
```

### Linux — systemd

Create a user service at `~/.config/systemd/user/m1nd-serve.service` (a template ships at
[`scripts/linux/m1nd-serve.service`](../scripts/linux/m1nd-serve.service)):

```ini
[Unit]
Description=m1nd served owner (persistent code graph over HTTP)
After=network.target

[Service]
ExecStart=%h/.m1nd/bin/m1nd-mcp --serve --no-gui --port 1337
Environment=M1ND_RUNTIME_DIR=%h/.m1nd
Restart=on-failure
RestartSec=2

[Install]
WantedBy=default.target
```

Enable and start it:
```bash
systemctl --user daemon-reload
systemctl --user enable --now m1nd-serve.service
```

## 2. Point your IDE/agent at the owner (native attach)

Register `m1nd-mcp --attach` as the host's MCP server — **not** the owner binary. It speaks
stdio to the host and forwards to the owner over localhost:

```json
{
  "mcpServers": {
    "m1nd": {
      "command": "m1nd-mcp",
      "args": ["--attach", "auto", "--stdio"]
    }
  }
}
```

`--attach auto` asks the instance registry two questions, in order, and takes no lease
either way:

1. **Is there a live serve owner for this client's runtime root?** (its lease) — the
   classic shape: one owner, one runtime, the bridges beside it.
2. **Is there a live serve owner whose declared ingest roots cover this repo?** — the
   fallback, for the far more common shape: the owner lives somewhere central (a launchd
   or systemd runtime dir) and has already ingested the repo you are working in. Without
   this question, an agent in such a repo gets its own empty local runtime while the graph
   it needs is alive one port away.

How question 2 decides: a caller **inside** a declared root is covered (a monorepo
subpackage reaches the repo's owner); a **git worktree** resolves to its main repository,
matching the rule that a worktree never gets a brain of its own; comparison is canonical
path identity, never a text prefix, so `<repo>-scratch` never matches `<repo>`; and if
**two** live owners cover the same repo, attach refuses and names both — that ambiguity is
yours to resolve, not auto-discovery's. The bearer token is read from the runtime root of
the owner actually resolved, which under question 2 is not this client's.

Pass `--attach http://127.0.0.1:1337` to pin a URL, or set `M1ND_ATTACH_URL` to override
both. Any number of attach bridges share the owner's one live graph, so what one agent
`memorize`s another recalls immediately — no reingest, no per-agent copy. Queries go over
`127.0.0.1`, so it stays local-first.

The same two questions are askable on their own. `m1nd-mcp --discover-owner` prints the
answer as one JSON object and exits — no bridge, no graph, no lease, no port — with exit
code `0` when an owner answered and `1` when none did, the refusal carried in `reason`.
It is what the npm agent CLI asks before deciding how to boot, and the honest way to
check from any language, or a shell, which owner a given directory would reach:

```bash
cd /path/to/repo && m1nd-mcp --discover-owner
# → {"schema":"m1nd-owner-discovery-v0","found":true,"discovery":"ingest_coverage",
#    "base_url":"http://127.0.0.1:1338","declared_root":"/path/to/repo", …}
```

## 3. Noise reduction (large workspaces)

If your workspace contains massive dependencies (iOS Pods, `node_modules`, Rust `target/`),
a raw ingest pollutes the graph and degrades retrieval. The helper scripts in
`scripts/macos/` scope the graph to the namespaces you actually work in:

1. `smart-ingest.py` — ingest only specific relevant namespaces (`mode="merge"`).
2. `file-watcher.py` (with `world.m1nd.file-watcher.plist`) — trigger incremental syncs
   when files change.

Keeping the graph resident in one owner and attaching every host to it, `m1nd` runs at full
speed with zero per-session startup overhead.
