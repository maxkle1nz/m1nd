# Installing m1nd (for coding agents)

**You are an agent installing m1nd into a host so that you — the agent — can call it.**
m1nd is a local-first, MCP-native code-graph runtime. Once wired, your first move each
session is `north(task)`: one call that returns binding trust, task focus + PageRank
anchors, prior cross-session memory, a sufficiency signal, and `honest_gaps`. Nothing
is sent to an LLM by the graph engine. Installation/model downloads can use the network, optional document providers may transmit documents to a configured endpoint, and the MCP host has its own data policy.

This file is the machine-legible install path. The human-facing version is the README
[Quick Start](README.md#get-started); the exhaustive per-host reference is
[`docs/HOST-INTEGRATION-MATRIX.md`](docs/HOST-INTEGRATION-MATRIX.md) (its §3 holds the
copy-pasteable recipes referenced below — read it, do not duplicate it).

---

## Version boundary

This source line is `1.6.5`; the latest published release at this refresh is `1.6.3`. The npx commands below pin the published wrapper; require native 1.6.3 for the direct-stdio recipe. The automatic isolated first-workspace path is source-preview until publication. Use matching wrapper/native versions. Generic client `replace`/`merge` remain refused; the newer launcher path is not a shared-brain authority grant.

## 0. Prerequisites

- **Node.js ≥ 18** and **npm** (for the installer/agent-pack; the installer is pure JS).
- The native runtime is **`m1nd-mcp`** (Rust). The npm package installs the agent
  doctrine + host adapters and tells you whether the native binary is visible; `doctor`
  reports general runtime visibility. Use the [signed 1.6.3 native installation](docs/INSTALL-1.6.3.md)
  for this released recipe.
- **macOS or Linux** for session-start hooks that inject context (some hosts are
  hook-gated to those platforms — noted per host in the matrix). MCP itself works
  everywhere.

## 1. Pin the native runtime, then select it

Follow [the signed native 1.6.3 installation guide](docs/INSTALL-1.6.3.md). It selects a platform asset from the exact release and verifies its signature before installation. Pinning `npx @maxkle1nz/m1nd@1.6.3` alone does **not** pin `update apply`; the updater follows the npm registry channel. Do not use it to reproduce this versioned recipe.

```bash
export M1ND_BINARY="$HOME/.local/share/m1nd/versions/1.6.3/m1nd-mcp"
"$M1ND_BINARY" --version
```

Require `m1nd-mcp 1.6.3` before continuing. Pass this absolute binary path to the setup commands below. `doctor` remains a general runtime-visibility diagnostic, not a version selector. Automatic isolated preparation belongs to the matching newer source-preview wrapper/binary, not this pinned released route.

## 2. Wire your host — plan, then apply

`m1nd hosts plan` is **read-only**: it prints the exact MCP config JSON, the
session-start hook, and the doctrine file for a host, writing nothing. `m1nd hosts apply
--yes` performs the opt-in local mutation (it merges owned hook JSON without clobbering
existing hooks, and prints — rather than writes — host-managed configs like Claude /
Cline / Kiro).

```bash
# See exactly what will be wired for your host (writes nothing):
npx -y @maxkle1nz/m1nd@1.6.3 hosts plan --host <host> --project /abs/path/to/project --binary "$M1ND_BINARY"

# Apply it (opt-in mutation; dry-run without --yes):
npx -y @maxkle1nz/m1nd@1.6.3 hosts apply --host <host> --project /abs/path/to/project --binary "$M1ND_BINARY" --yes
```

`<host>` is one of: `codex`, `claude`, `gemini`, `antigravity`, `generic`, `qwen`,
`kiro`, `cline`, `continue`, `grok`, `cursor`, `windsurf`, `zed`, `vscode`, `opencode`,
`warp`, `trae`, `jetbrains`, `amp`, `goose`, `crush`, `aider`, or `all`.

The two lower-level commands `apply` composes, if you want them by hand:

```bash
npx -y @maxkle1nz/m1nd@1.6.3 install-skills <host> --project /abs/path   # the agent pack (5 protocols)
npx -y @maxkle1nz/m1nd@1.6.3 mcp-config <host> --project /abs/path --binary "$M1ND_BINARY" # the MCP server registration
```

For the published 1.6.3 direct-stdio path, the human birth and the host MUST use the same explicit absolute runtime inside the repository. Add `M1ND_RUNTIME_DIR` (for example `/absolute/path/to/project/.m1nd`) to the generated MCP environment, retaining `M1ND_WORKSPACE_ROOT`; `hosts apply` does not supply that selector. Check the birth receipt's `store_dir`. Do not combine this nested-runtime recipe with the newer source-preview automatic launcher grant.

## 2a. Make the ambient north hook work — and verify it fired

Registering the MCP server (above) lets your agent *call* `north`. The **ambient** layer is one
more thing: a SessionStart-family hook that injects the `north` packet as `additionalContext`
before your first turn, through the shipped `m1nd-north-shim` bin. Claude's `settings.json` is
host-managed, so `m1nd hosts apply --host claude` **prints** the block — it does not write it.
Paste it into `.claude/settings.json` (or `~/.claude/settings.json`):

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "startup|resume",
        "hooks": [
          { "type": "command",
            "command": "m1nd-north-shim --repo \"$CLAUDE_PROJECT_DIR\" --query \"orient\"" }
        ]
      }
    ]
  }
}
```

**Verify it fired** — run the shim by hand and confirm it prints the envelope:

```bash
m1nd-north-shim --repo "$PWD" --query orient
#  → {"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":"[m1nd north] …"}}
```

An empty output with `exit 0` is the fail-open default (an absent or broken runtime never blocks
the session), not an error. Otherwise open a fresh session and look for the injected orientation:
when a served m1nd owner is live, the shim opens with its `north` **voice card** (the `human_view`
lines lead, then the `[m1nd north]` summary); with no owner reachable it falls back to the
standalone `first-minute` summary. Session-start
hooks are **macOS/Linux-only** (§0); MCP itself works everywhere, and the `M1ND_INSTRUCTIONS`
north-first line reinforces orientation on hosts that render it.

## 3. MCP server config (paste-in, per host)

If your host takes a raw MCP-servers block rather than the installer, register the
runtime. **The recommended shape is one served owner with each host attached to it**
(see *Shared owner, many agents* below) — that is the path that shares one graph, serves
the UI, and compounds across hosts. **Direct stdio is the single-agent fallback**: no
shared graph, no UI, no cross-host compounding. **Canonical recipes live in
[`docs/HOST-INTEGRATION-MATRIX.md`](docs/HOST-INTEGRATION-MATRIX.md) §3** — the blocks
below are the common shape; defer to the matrix for host-specific keys and gotchas.

**Single-agent fallback — direct stdio.** Claude Code / Claude Agent SDK / Cursor /
Windsurf / generic (`.mcp.json`, `.cursor/mcp.json`, or the host's `mcpServers` map):

```json
{
  "mcpServers": {
    "m1nd": {
      "command": "/absolute/path/to/the/verified/m1nd-mcp",
      "args": ["--stdio", "--no-gui"],
      "env": {
        "M1ND_WORKSPACE_ROOT": "/absolute/path/to/project",
        "M1ND_RUNTIME_DIR": "/absolute/path/to/project/.m1nd"
      }
    }
  }
}
```

**Codex CLI** (`~/.codex/config.toml`):

```toml
[mcp_servers.m1nd]
command = "/absolute/path/to/the/verified/m1nd-mcp"
args = ["--stdio", "--no-gui"]

[mcp_servers.m1nd.env]
M1ND_WORKSPACE_ROOT = "/absolute/path/to/project"
M1ND_RUNTIME_DIR = "/absolute/path/to/project/.m1nd"
```

**Shared owner, many agents (recommended for real work).** Run one owner that holds the
live graph, and point every host at it as a thin bridge (loads no graph, takes no lease):

```bash
# once, per project — the owner (add --open, and drop --no-gui, to open the served web UI):
"$M1ND_BINARY" --serve --no-gui --port 1337 --runtime-dir /abs/path/to/project/.m1nd
```

```json
{ "mcpServers": { "m1nd": { "command": "/absolute/path/to/the/verified/m1nd-mcp", "args": ["--attach", "auto", "--stdio"], "env": {"M1ND_WORKSPACE_ROOT":"/absolute/path/to/project", "M1ND_RUNTIME_DIR":"/absolute/path/to/project/.m1nd"} } } }
```

`--attach auto` auto-discovers the live owner — no hardcoded port (the default is 1337, but
an existing owner may serve elsewhere, e.g. 1338). It looks for an owner of this runtime
first and, failing that, for a live owner that has already **ingested the repo you are
working in**, so a central always-on owner is found from inside its own projects. Pass an
explicit `http://127.0.0.1:<port>` to pin it, or `M1ND_ATTACH_URL` to override both. See
README → *One graph, many agents*, and [`docs/deployment.md`](docs/deployment.md) for the
always-on launchd/systemd owner.

## 4. Environment variables

| Variable | Purpose |
|---|---|
| `M1ND_RUNTIME_DIR` | Where the graph/plasticity/instance state for this project live (e.g. `<project>/.m1nd`). |
| `M1ND_ATTACH_URL` | If set, every invocation attaches to this owner URL (wins over `--attach`). |
| `M1ND_READ_ONLY=1` | Attach read-only: serve queries, never write, never take a lease. Same as `--read-only`. |
| `M1ND_EXPECTED_VERSION` / `M1ND_EXPECTED_SHA` | Pin the binary; with `M1ND_STRICT_VERSION` the host refuses a drifted binary. |

## 5. Verify

```bash
"$M1ND_BINARY" --version                     # require the selected 1.6.3 runtime
npx -y @maxkle1nz/m1nd@1.6.3 doctor --json     # general visibility; may find a different runtime
```

Then, inside a live MCP session, confirm the front door answers:

```jsonc
north({ "agent_id": "install-check", "task": "orient" })
```

A healthy response carries `binding.trust_mode` (e.g. `full_trust`), a `context` block
with `focus_nodes` + `anchors`, and `honest_gaps`. If the graph is empty it returns
`needs: "needs_ingest"`, and its `next_move` carries the door: **you do not ingest.**
For the published shared/served setup, minting a brain is the human's one-time terminal gesture. Offer this command and
stop; do not try to mint a brain through the agent transport:

```bash
M1ND_RUNTIME_DIR=/absolute/path/to/project/.m1nd \
  npx -y @maxkle1nz/m1nd@1.6.3 init --birth /absolute/path/to/project --binary "$M1ND_BINARY"
```

The ceremony accepts only an empty destination. It ingests the repo and prints what it
built (`node_count`, `edge_count`). For direct stdio, require `brain: "owner_bound_graph"`, the intended `born_root`, and `store_dir` matching the exact runtime configured in the host; it exits non-zero if the scan produced nothing, so
it never reports success over an empty graph. Calling generic `ingest` with default/`replace` or `merge` is refused on every transport
(`generic_action_authority_required`) — the refusal names this same command. Once the
graph exists, `ingest({ "mode": "refresh" })` from that exact root is the agent's own
door for keeping it fresh.

For a transport-level smoke without a host, from a repo checkout:

```bash
python3 scripts/mcp_agent_smoke.py --repo . --json
python3 scripts/mcp_agent_smoke.py --repo . --transport http --json
```

Outside a host entirely, the published first-value command below can attach to an already prepared covering owner. Automatic isolated preparation requires the separate matching source-preview wrapper/binary:

```bash
npx -y @maxkle1nz/m1nd@1.6.3 agent first-minute --repo . --query "map this repo" --binary "$M1ND_BINARY" --json
```

It asks `m1nd-mcp --discover-owner` first (read-only, no lease — `--attach auto`'s two
questions) and bridges to a live serve owner whose declared ingest roots cover the repo,
so on a machine with a served owner it reads that owner's real graph. With no covering
owner, published older builds may report `needs_authority` / `NOT_PROVEN`. The matching
1.6.5 source wrapper/native runtime can instead prepare an isolated graph for the exact
launcher-granted repo in an external private cache. It cannot mint a shared brain or move
a binding. See [agent autonomy](docs/AGENT-AUTONOMY.md) for the precise source contract. Read `runtime.boot`
(`attached_serve_owner` | `isolated_runtime`) and `runtime.owner_discovery` before
concluding the machine has no graph; `--no-attach` forces the isolated runtime.

## 6. Troubleshooting

- **`doctor` says the runtime is not visible** → the npm package is installed but the
  general diagnostic may not see your version-specific binary. Follow the [pinned signed installation](docs/INSTALL-1.6.3.md), check `"$M1ND_BINARY" --version`, and pass the same `--binary` path to setup commands. Do not substitute an unpinned installation for this recipe.
- **"Failed to connect" / lease held** → m1nd is single-instance per `runtime_root`. If
  an owner already holds the lease (another session, or a `--serve` owner), that message
  is **normal** — attach instead of taking a second lease: `--attach auto` or
  `M1ND_ATTACH_URL`. Do not fight the lock.
- **Retrieval looks empty or wrong after the repo obviously has code** → suspect a stale
  or wrong-workspace binding before blaming the graph. `doctor` reports
  `stale_binding_suspected` / `wrong_workspace_binding`; if repo-local smokes pass but
  the host retrieval is blocked, it is a host-binding / session split-brain, not the
  graph.
- **Host trust prompt on the hook (Codex, etc.)** → approve it once. Never script a
  bypass of hook-trust; changing the hook command re-prompts by design.
- **Verdicts never reach `act`** → the conformal gate is armed per-repo by running
  `calibrate_predict` once; until then verdicts cap at `reverify`. That is honest
  calibration, not a failure.

## 7. What you get, once wired

- `north(task)` — the oriented packet, before your first action.
- `seek` / `activate` / `impact` — retrieval and blast-radius, each carrying a calibrated
  verdict and a sufficiency signal (`abstain` / `insufficient_evidence` are real answers).
- `memorize` — write durable, evidence-anchored knowledge that the next session inherits.
- `doctor` / `trust_selftest` — one-call diagnostics and recovery.

Operate in a loop: **orient → act on verdicts → capture what you proved.** What one agent
memorizes, the next agent reads.
