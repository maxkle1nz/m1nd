# Agent-First Demo

This page is the shortest honest way to see why `m1nd` matters for coding
agents.

The demo does not use a canned transcript. It runs the same MCP smoke path an
agent needs before it can trust the repo graph:

```text
initialize -> tools/list -> trust_selftest -> session_handshake -> ingest -> seek -> help -> doctor -> recovery check
```

## Run It

From the repo root:

```bash
cargo build -p m1nd-mcp
m1nd smoke --repo . --transport stdio
```

For the HTTP tool API:

```bash
m1nd smoke --repo . --transport http
```

For machine-readable output:

```bash
m1nd smoke --repo . --transport stdio --json
```

## What It Shows

The demo proves the practical agent loop:

- the MCP binding is visible
- `trust_selftest` returns a usable verdict
- the graph can be populated from the current repo
- retrieval scans the populated graph
- `help` can explain the next move
- `doctor` can see the active graph
- an intentionally empty retrieval gets a recovery playbook instead of leaving
  the agent to guess

The point is not just search. The point is that the agent knows whether it can
trust the current session before it acts.

When a task points at a different repo than the active binding, the same trust
loop should return `wrong_workspace_binding` through `context_guard` rather than
letting the agent confuse workspace mismatch with a stale or empty graph.

## How To Read The Output

The Markdown output is shaped like a handoff:

- **What The Agent Learns**: session trust, tool count, and whether the agent
  can ingest, retrieve, and recover.
- **Graph Built**: how much repo structure was loaded.
- **Retrieval Result**: whether the structural query actually scanned the graph.
- **Recovery Behavior**: what happens when retrieval finds nothing useful.
- **Checks**: the small contract that prevents a pretty transcript from hiding a
  broken agent path.

The JSON output uses schema `m1nd-agent-first-demo-v0` and is safe for CI,
docs generation, or a client-side onboarding screen.

## Why This Is Different From Grep

`grep` can answer a string question. This demo is about a larger agent question:

> Is this session trustworthy enough for me to use graph context before I edit?

`m1nd` answers that before the agent spends the rest of the turn searching
manually. If the answer is no, it returns the next recovery step.

## Limits

The demo is a local operational check. It does not prove application behavior,
replace tests, replace the compiler, or guarantee that a host client has
refreshed its MCP tool schema. If a host-provided tool surface is stale, run the
repo-local demo and compare the binding fingerprint with the host session.
