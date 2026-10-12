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

GitHub/native, crates.io, and npm **1.6.5** are published at immutable commit `363bc97f2336c73e90802916804dccd2109c5746`; npm `latest` is 1.6.5 as of 2026-10-12. The commands below select matching 1.6.5 wrapper/native versions. Automatic isolated first-workspace preparation shipped in this release. PR580's later learning-event separation is **Unreleased** on `main`, absent from the sealed release despite matching package version strings. Generic client `replace`/`merge` remain refused; a launcher grant is not shared-brain authority. The [1.6.3 installation guide](docs/INSTALL-1.6.3.md) remains historical.

## 0. Prerequisites

- **Node.js ≥ 18** and **npm** (for the installer/agent-pack; the installer is pure JS).
- The native runtime is **`m1nd-mcp`** (Rust). The npm package installs the agent
  doctrine + host adapters and tells you whether the native binary is visible; `doctor`
  reports general runtime visibility. Use the [signed 1.6.5 native installation](docs/INSTALL-1.6.5.md)
  for this released recipe.
- **macOS or Linux** for session-start hooks that inject context (some hosts are
  hook-gated to those platforms — noted per host in the matrix). MCP itself works
  everywhere.

## 1. Pin the native runtime, then select it

Follow [the signed native 1.6.5 installation guide](docs/INSTALL-1.6.5.md). It selects a platform asset from the exact release and verifies its signature and embedded source identity before installation. Pinning `npx @maxkle1nz/m1nd@1.6.5` alone does **not** pin `update apply`; the updater follows the npm registry channel. Do not use it to reproduce this versioned recipe.

```bash
export M1ND_BINARY="$HOME/.local/share/m1nd/versions/1.6.5/m1nd-mcp"
"$M1ND_BINARY" --version
```

Require native `1.6.5` at revision `363bc97f2336c73e90802916804dccd2109c5746` before continuing. The signed macOS ARM64 artifact includes a `-dirty` build marker; the installation guide verifies the exact tag-bound asset and checks its version/revision. Pass this absolute binary path to the commands below. `doctor` remains a general runtime-visibility diagnostic, not a version selector.

For a first result before host setup, run from the repository in a clean subshell:

```bash
(
  unset M1ND_RUNTIME_DIR M1ND_MCP_ARGS
  npx -y @maxkle1nz/m1nd@1.6.5 agent first-minute --repo . \
    --binary "$M1ND_BINARY" --query "map this repo" --json
)
```

The CLI discovers a covering owner first; otherwise it prepares only the exact launcher-granted repository in an external private cache. It preserves source files and reuses the graph, refreshing the declared root before retrieval. Check `runtime.boot`, repository binding, source locations, gaps, and exit status. This does not mint a shared brain or move a binding. See [agent autonomy](docs/AGENT-AUTONOMY.md).

## 2. Wire your host — plan, then apply

`m1nd hosts plan` is **read-only**: it prints the exact MCP config JSON, the
session-start hook, and the doctrine file for a host, writing nothing. `m1nd hosts apply
--yes` performs the opt-in local mutation (it merges owned hook JSON without clobbering
existing hooks, and prints — rather than writes — host-managed configs like Claude /
Cline / Kiro).

```bash
# See exactly what will be wired for your host (writes nothing):
npx -y @maxkle1nz/m1nd@1.6.5 hosts plan --host <host> --project /abs/path/to/project --binary "$M1ND_BINARY"

# Apply it (opt-in mutation; dry-run without --yes):
npx -y @maxkle1nz/m1nd@1.6.5 hosts apply --host <host> --project /abs/path/to/project --binary "$M1ND_BINARY" --yes
```

`<host>` is one of: `codex`, `claude`, `gemini`, `antigravity`, `generic`, `qwen`,
`kiro`, `cline`, `continue`, `grok`, `cursor`, `windsurf`, `zed`, `vscode`, `opencode`,
`warp`, `trae`, `jetbrains`, `amp`, `goose`, `crush`, `aider`, or `all`.

The two lower-level commands `apply` composes, if you want them by hand:

```bash
npx -y @maxkle1nz/m1nd@1.6.5 install-skills <host> --project /abs/path   # the agent pack (5 protocols)
npx -y @maxkle1nz/m1nd@1.6.5 mcp-config <host> --project /abs/path --binary "$M1ND_BINARY" # the MCP server registration
```

For published 1.6.5 **direct stdio**, retain `M1ND_WORKSPACE_ROOT` and add an absolute external `M1ND_RUNTIME_DIR`; `hosts apply` does not supply that selector. Create a distinct runtime per repository before connecting, for example `install -d -m 700 "$HOME/.cache/m1nd/host/repo-alpha"`. It must be outside the source tree, a real directory owned by your user, with safe canonical ancestors and no group/other write permission. Use its actual absolute path in the host environment. Startup prepares only that granted root; `--birth` is not required on this path. A missing, nested, unsafe, or conflicting runtime refuses. Shared-owner and legacy paths retain their separate preparation requirements.

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
        "M1ND_RUNTIME_DIR": "/absolute/private/runtime/repo-alpha"
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
M1ND_RUNTIME_DIR = "/absolute/private/runtime/repo-alpha"
```

**Shared owner, many agents (recommended for real work).** Run one owner that holds the
live graph, and point every host at it as a thin bridge (loads no graph, takes no lease):

```bash
# Use an already prepared owner runtime (see the authority boundary below).
"$M1ND_BINARY" --serve --no-gui --port 1337 --runtime-dir /absolute/private/runtime/repo-alpha
```

```json
{ "mcpServers": { "m1nd": { "command": "/absolute/path/to/the/verified/m1nd-mcp", "args": ["--attach", "auto", "--stdio"], "env": {"M1ND_WORKSPACE_ROOT":"/absolute/path/to/project", "M1ND_RUNTIME_DIR":"/absolute/private/runtime/repo-alpha"} } } }
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
| `M1ND_RUNTIME_DIR` | Existing external private graph/plasticity/instance directory for launcher-granted stdio; use a distinct runtime per repository. |
| `M1ND_ATTACH_URL` | If set, every invocation attaches to this owner URL (wins over `--attach`). |
| `M1ND_READ_ONLY=1` | Attach read-only: serve queries, never write, never take a lease. Same as `--read-only`. |
| `M1ND_EXPECTED_VERSION` / `M1ND_EXPECTED_SHA` | Pin the binary; with `M1ND_STRICT_VERSION` the host refuses a drifted binary. |

## 5. Verify

```bash
"$M1ND_BINARY" --version                     # require native 1.6.5 at the sealed commit
npx -y @maxkle1nz/m1nd@1.6.5 doctor --json     # general visibility; may find a different runtime
```

Then, inside a live MCP session, confirm the front door answers:

```jsonc
north({ "agent_id": "install-check", "task": "orient" })
```

A healthy response carries `binding.trust_mode` (e.g. `full_trust`), a `context` block
with `focus_nodes` + `anchors`, and `honest_gaps`. The matching 1.6.5 isolated CLI
and explicitly granted direct-stdio path prepare their first graph at startup.
If a different binding returns `needs: "needs_ingest"`, follow its recovery
arguments; generic `ingest` is not a bootstrap shortcut. For a new shared/served
brain or a legacy launcher outside the explicit grant contract, the human's
terminal ceremony remains separate. Offer the applicable command and stop;
agents must not perform it. A person may prepare an empty owner destination:

```bash
M1ND_RUNTIME_DIR=/absolute/private/runtime/repo-alpha \
  npx -y @maxkle1nz/m1nd@1.6.5 init --birth /absolute/path/to/project --binary "$M1ND_BINARY"
```

The ceremony accepts only an empty destination. It ingests the repo and prints what it
built (`node_count`, `edge_count`). Check the intended `born_root` and `store_dir`.
An external shared-owner ceremony uses `brain: "project_brain"`; the historical
1.6.3 solo nested-runtime ceremony uses `brain: "owner_bound_graph"`. Do not mix
that nested recipe with 1.6.5 launcher-granted startup. Birth exits nonzero if the scan produced nothing, so
it never reports success over an empty graph. Calling generic `ingest` with default/`replace` or `merge` is refused on every transport
(`generic_action_authority_required`) — the refusal names this same command. Once the
graph exists, `ingest({ "mode": "refresh" })` from that exact root is the agent's own
door for keeping it fresh.

For a transport-level smoke without a host, from a repo checkout:

```bash
python3 scripts/mcp_agent_smoke.py --repo . --json
python3 scripts/mcp_agent_smoke.py --repo . --transport http --json
```

Outside a host entirely, the published 1.6.5 first-value command can attach to a covering owner or prepare its isolated graph:

```bash
npx -y @maxkle1nz/m1nd@1.6.5 agent first-minute --repo . --query "map this repo" --binary "$M1ND_BINARY" --json
```

It asks `m1nd-mcp --discover-owner` first (read-only, no lease — `--attach auto`'s two
questions) and bridges to a live serve owner whose declared ingest roots cover the repo,
so on a machine with a served owner it reads that owner's real graph. With no covering
owner, published older builds may report `needs_authority` / `NOT_PROVEN`. The matching
published 1.6.5 wrapper/native runtime prepares an isolated graph for the exact
launcher-granted repo in an external private cache. It cannot mint a shared brain or move
a binding. See [agent autonomy](docs/AGENT-AUTONOMY.md) for the precise launcher contract. Read `runtime.boot`
(`attached_serve_owner` | `isolated_runtime`) and `runtime.owner_discovery` before
concluding the machine has no graph; `--no-attach` forces the isolated runtime.

## 6. Troubleshooting

- **`doctor` says the runtime is not visible** → the npm package is installed but the
  general diagnostic may not see your version-specific binary. Follow the [pinned signed installation](docs/INSTALL-1.6.5.md), check `"$M1ND_BINARY" --version`, and pass the same `--binary` path to setup commands. Do not substitute an unpinned installation for this recipe.
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
- **Verdicts never reach `act`** → published 1.6.5 and current `main` treat every
  v1 calibration row as `legacy_evidence_unqualified`. Prediction abstains;
  seek envelopes stay at `reverify` or `unprovable`, retaining scores and ranking.
  `calibrate_predict` and `calibrate_envelope` are diagnostic handlers: running
  them does not enable `act`. Their generic public routes remain policy-disabled;
  there is no public calibration repair call. Verify source, compiler, tests,
  and runtime evidence. See [calibration qualification](docs/CALIBRATION-QUALIFICATION.md).

## 7. What you get, once wired

- `north(task)` — the oriented packet, before your first action.
- `seek` / `activate` / `impact` — retrieval and graph relationships to inspect;
  read each tool's actual evidence and gap fields. Seek envelopes remain unqualified
  for `act`; `abstain` / `insufficient_evidence` are real limits.
- `memorize` — write durable, evidence-anchored knowledge that the next session inherits.
- `doctor` / `trust_selftest` — one-call diagnostics and recovery.

Operate in a loop: **orient → act on verdicts → capture what you proved.** What one agent
memorizes, the next agent reads.
