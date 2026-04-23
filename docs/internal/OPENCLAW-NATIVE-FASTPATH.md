# OpenClaw Native Fast Path for m1nd

## Why this exists

`m1nd-mcp` is the universal transport surface.
It is correct for Codex, Claude, Cursor, Windsurf, and any generic MCP client.

It is not the lowest-latency path for OpenClaw.

In the current local setup, the graph engine is fast, but the transport stack adds avoidable cost:

- subprocess spawn
- stdio framing
- JSON-RPC encode/decode
- bridge CLI overhead (`mcporter`)
- optional GUI boot attempts in non-UI flows

Measured locally on this machine:

- `m1nd_cli.py health`: ~148 ms average
- `mcporter -> health`: ~522 ms average
- `m1nd_cli.py search(literal)`: ~777 ms average
- `mcporter -> search(literal)`: ~994 ms average

That means the bridge layer is currently adding roughly 200-400 ms of avoidable latency per call.

Observed with the new native Unix-socket bridge scaffold on the same machine:

- `m1nd-openclaw` + loaded `SISTEMA` graph (`16122` nodes / `42338` edges)
- `health` roundtrip overhead: ~`0.23 ms` average over 5 calls
- warm samples landed between ~`0.07 ms` and `0.11 ms`

That is already well below the original sub-30 ms target and leaves room for future binary/shared-memory upgrades on larger payload tools.

## Target

OpenClaw should have a native `m1nd` fast lane with overhead closer to **single-digit milliseconds** than to hundreds of milliseconds.

The design goal is:

- keep MCP for everyone else
- give OpenClaw a native transport
- keep one hot `SessionState`
- avoid process spawn on every request

## Transport decision

### Chosen direction

Use a **persistent Unix domain socket control plane** now, with a design that can later adopt a **shared-memory data plane** for very large responses.

Why:

- UDS on the same machine is dramatically faster than stdio subprocess hops
- the implementation cost is much lower than jumping straight to a full custom shared-memory RPC stack
- it keeps the bridge operational quickly while preserving a future upgrade path

### Frontier donors considered

#### 1. `iceoryx2`

- GitHub: [eclipse-iceoryx/iceoryx2](https://github.com/eclipse-iceoryx/iceoryx2)
- Docs: [docs.rs/iceoryx2](https://docs.rs/iceoryx2/latest/iceoryx2/)

Why it matters:

- zero-copy IPC
- request/response pattern
- excellent for giant payloads and ultra-low-latency local transport

Status in this plan:

- **Phase 2 candidate**
- strongest donor for large-response transport once the OpenClaw native lane is proven

#### 2. Cap'n Proto RPC

- GitHub: [capnproto/capnproto-rust](https://github.com/capnproto/capnproto-rust)

Why it matters:

- strongly typed RPC
- compact binary messages
- excellent long-term protocol discipline

Why not first:

- more protocol machinery than we need to prove the fast path
- lower immediate delivery speed than a direct UDS bridge

#### 3. `tarpc`

- Docs: [docs.rs/tarpc](https://docs.rs/tarpc/latest/tarpc/)

Why it matters:

- fast Rust RPC
- simpler than full MCP

Why not first:

- still introduces an RPC framework layer we do not strictly need
- direct UDS request/response is easier to wire into `SessionState`

## Architecture

### Canonical shape

1. `m1nd-mcp`
   - universal MCP surface
   - stdio + HTTP/UI
   - stays public and stable

2. `m1nd-openclaw`
   - native low-latency bridge
   - persistent daemon
   - OpenClaw-only fast path

3. OpenClaw adapter
   - prefers native bridge when available
   - falls back to MCP when needed

### Fast path

OpenClaw request:

```text
OpenClaw -> Unix socket -> m1nd-openclaw -> dispatch_tool(SessionState) -> response
```

No subprocess spawn per call.
No JSON-RPC envelope.
No GUI boot.
No `mcporter` in the hot path.

### Data plane upgrade

For large payload tools such as:

- `surgical_context_v2`
- `apply_batch`
- `perspective_routes`
- `report`

the bridge can later evolve to:

- UDS control plane for metadata and coordination
- `iceoryx2` shared-memory payload blocks for large response bodies

That gives us:

- low handshake overhead
- low copy overhead
- better scaling on giant graph/context payloads

## Protocol

Current scaffold request:

```json
{
  "id": "req-1",
  "tool": "search",
  "arguments": {
    "agent_id": "openclaw",
    "query": "m1nd_cli.py",
    "mode": "literal",
    "top_k": 3
  }
}
```

Current scaffold response:

```json
{
  "id": "req-1",
  "ok": true,
  "result": { "...": "tool response" },
  "elapsed_ms": 3.1
}
```

## Hot-path tool set

These should be first-class in the native lane:

- `health`
- `search`
- `seek`
- `view`
- `impact`
- `why`
- `surgical_context_v2`
- `apply`
- `apply_batch`
- `symbol_splice`

## Operational surfaces added in this patch

### Daemon

- [`scripts/macos/m1nd-openclaw-bridge.sh`](/Users/cosmophonix/SISTEMA/m1nd/scripts/macos/m1nd-openclaw-bridge.sh)
- [`scripts/macos/ai.m1nd.openclaw-bridge.plist`](/Users/cosmophonix/SISTEMA/m1nd/scripts/macos/ai.m1nd.openclaw-bridge.plist)

### Client

- [`scripts/macos/m1nd-openclaw-call.sh`](/Users/cosmophonix/SISTEMA/m1nd/scripts/macos/m1nd-openclaw-call.sh)
- binary: `target/release/m1nd-openclaw-client`

### Example local flow

```bash
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/ai.m1nd.openclaw-bridge.plist
/Users/cosmophonix/SISTEMA/m1nd/scripts/macos/m1nd-openclaw-call.sh \
  health \
  '{"agent_id":"openclaw"}'
```

## Patch choices in this repo

This patch introduces:

1. `m1nd-openclaw`
   - a new crate in the workspace
   - persistent Unix-socket daemon
   - direct calls into `dispatch_tool`

2. `m1nd-openclaw-client`
   - a tiny CLI client for the native socket bridge
   - immediate consumption point for OpenClaw-side wrappers, skills, or future runtime adapters
   - lets OpenClaw avoid `mcporter` in the hot path without needing the full native plugin patch on day one

3. OpenClaw-side immediate optimization
   - `mcporter` config should pass `--no-gui` when using stdio
   - one dedicated HTTP/UI instance should remain separate

## Why this is the right first patch

Because it creates a native lane **inside the m1nd repo** without breaking MCP compatibility.

That means:

- the public product stays MCP-native
- OpenClaw gets a privileged integration
- the fastest path lives close to the graph instead of in an external wrapper

## Next steps

### Phase 1

- wire OpenClaw to call the Unix socket bridge directly
- keep MCP as fallback
- benchmark overhead against `mcporter`

Immediate command shape:

```bash
m1nd-openclaw-client health '{"agent_id":"openclaw"}'
m1nd-openclaw-client search '{"agent_id":"openclaw","query":"websocket","mode":"literal","top_k":5}'
```

### Phase 2

- add request multiplexing and pooled worker scheduling
- add optional binary framing
- classify hot vs cold tools

### Phase 3

- add `iceoryx2` payload lanes for very large result sets
- keep UDS as control plane

## Success criteria

- `health` bridge overhead under 10 ms
- `search` bridge overhead under 5-15 ms beyond raw tool execution
- zero process spawn per request
- OpenClaw can prefer native bridge automatically
