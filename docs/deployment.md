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
   relays the owner's push notifications back. `--attach auto` discovers the owner by its
   lease.
4. **Incremental ingest** — the owner updates the graph as files change, skipping noise
   (`node_modules`, `Pods`, Rust `target/`).

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
        <string>/Users/youruser/.m1nd/bin/m1nd-mcp</string>
        <string>--serve</string>
        <string>--no-gui</string>
        <string>--port</string>
        <string>1337</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>M1ND_RUNTIME_DIR</key>
        <string>/Users/youruser/.m1nd</string>
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

`--attach auto` discovers the live owner for this runtime by its lease; pass
`--attach http://127.0.0.1:1337` to pin a URL, or set `M1ND_ATTACH_URL` to override both.
Any number of attach bridges share the owner's one live graph, so what one agent
`memorize`s another recalls immediately — no reingest, no per-agent copy. Queries go over
`127.0.0.1`, so it stays local-first.

## 3. Noise reduction (large workspaces)

If your workspace contains massive dependencies (iOS Pods, `node_modules`, Rust `target/`),
a raw ingest pollutes the graph and degrades retrieval. The helper scripts in
`scripts/macos/` scope the graph to the namespaces you actually work in:

1. `smart-ingest.py` — ingest only specific relevant namespaces (`mode="merge"`).
2. `file-watcher.py` (with `world.m1nd.file-watcher.plist`) — trigger incremental syncs
   when files change.

Keeping the graph resident in one owner and attaching every host to it, `m1nd` runs at full
speed with zero per-session startup overhead.
