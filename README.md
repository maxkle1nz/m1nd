<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

# m1nd 2.0

**A local code graph and evidence-linked memory server for MCP coding agents.**

m1nd builds a structural graph of your repository in Rust, answers orientation and impact queries over MCP, and carries verified findings across sessions with explicit trust signals and declared gaps. Everything runs locally on your machine; graph queries do not require an LLM API key.

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?label=npm" alt="npm version" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT license" /></a>
</p>

[Install](#install) · [Host setup](#host-setup) · [First use](#first-use) · [Memory & freshness](#memory-and-freshness) · [Multiple sessions](#multiple-sessions-on-one-brain) · [Limits](#limits)

---

## Install

<a id="get-started"></a>
<a id="sixty-seconds"></a>
<a id="quick-start"></a>

You need **Node.js 18+** (for the dependency-free `@maxkle1nz/m1nd` CLI/installer) and **Rust (stable)** to build the native `m1nd-mcp` binary from this 2.0.0 source tree (signed 2.0.0 release artifacts and the npm 2.0.0 package are not yet published):

```bash
git clone https://github.com/maxkle1nz/m1nd.git
cd m1nd
cargo build --release -p m1nd-mcp
./target/release/m1nd-mcp --version
```

Place `./target/release/m1nd-mcp` on your `PATH` (for example `~/.local/bin/m1nd-mcp`) or pass `--binary /path/to/m1nd-mcp` to the installer. Verify the wrapper and native binary with:

```bash
node npm/bin/m1nd.js version
node npm/bin/m1nd.js doctor
```

Once `@maxkle1nz/m1nd@2.0.0` is published, replace `node npm/bin/m1nd.js` in the commands below with `npx @maxkle1nz/m1nd` (or `m1nd` if installed globally).

---

## Host setup

Use `hosts status`, `hosts plan` (read-only), and `hosts apply` (dry-run unless `--yes` is passed):

```bash
# Inspect current host wiring and preview changes without editing files
node npm/bin/m1nd.js hosts status --host claude --project ~/src/my-repo
node npm/bin/m1nd.js hosts plan   --host claude --project ~/src/my-repo --binary /path/to/m1nd-mcp

# Apply skills, MCP config, and host-owned hooks
node npm/bin/m1nd.js hosts apply  --host claude --project ~/src/my-repo --binary /path/to/m1nd-mcp --yes
```

Pass `--no-skills`, `--no-config`, or `--no-hooks` to skip individual steps. Every host config write is atomic; existing config files are backed up under `~/.m1nd/backups/` with private `0700`/`0600` permissions on Unix. MCP `env` values are kept out of host scoping and status output.

| Host (`--host`) | Skills / agent pack | MCP config target | Hooks & doctrine |
|---|---|---|---|
| **Claude Code** (`claude`) | Global `~/.claude/skills/` (`m1nd-first`, `m1nd-guardian`, `m1nd-operator`) | `<project>/.mcp.json` (`~/.claude.json` is **never modified**; Claude resolves `local > project (.mcp.json) > user`, so only a local project-scope entry in `~/.claude.json` hides `.mcp.json` — remove it with `claude mcp remove m1nd -s local`) | Doctrine → `<project>/.claude/CLAUDE.md`. `SessionStart` hook for `~/.claude/settings.json` is **printed** to paste manually, never written. |
| **Codex** (`codex`) | `$CODEX_HOME/skills/` (honors `CODEX_HOME`, default `~/.codex`; `hosts status` detects outdated skills) | `$CODEX_HOME/config.toml` (`[mcp_servers.m1nd]`) | Doctrine → `<project>/AGENTS.md`. `SessionStart` hook written to `$CODEX_HOME/hooks.json` (enable `[features] hooks = true` in `config.toml`). |
| **Antigravity** (`antigravity`) | Plugin at `~/.gemini/config/plugins/m1nd/` (`plugin.json` + `skills/`; refuses symlinks or foreign directories) | Global `~/.gemini/config/mcp_config.json` | Doctrine → `~/.gemini/GEMINI.md` (shared with Gemini CLI). No session-start hook. |

Other hosts (`gemini`, `generic`, and the hosts listed in [docs/HOST-INTEGRATION-MATRIX.md](docs/HOST-INTEGRATION-MATRIX.md)) are also supported. Active hosts must be restarted or reconnected after config changes; verify the binding in-session by calling `trust_selftest` or `session_handshake`.

---

## First use

A fresh repository needs a graph before retrieval works:

- **Direct stdio with an explicit workspace grant:** when a host launches `m1nd-mcp --stdio --no-gui` with `M1ND_WORKSPACE_ROOT=~/src/my-repo` and a private writable runtime, startup prepares the empty bound graph automatically (see [docs/AGENT-AUTONOMY.md](docs/AGENT-AUTONOMY.md)).
- **Explicit human birth (for a shared or pre-created runtime):** run once in a terminal yourself:
  ```bash
  node npm/bin/m1nd.js init --birth ~/src/my-repo --binary /path/to/m1nd-mcp
  ```
- **Host-free CLI probe:** run a one-shot query without configuring an IDE:
  ```bash
  node npm/bin/m1nd.js agent first-minute --repo ~/src/my-repo \
    --binary /path/to/m1nd-mcp --query "where is configuration loaded?" --json
  ```
  Add `--no-attach --state-dir /path/to/private-cache` (outside the repository) to force an isolated cache; `agent scope` previews paths without writing.

### Core workflow and tool menu

Start every task with `north`, narrow candidates with `seek` or `search`, read code with `surgical_context` or `view`, and inspect blast radius with `impact` before editing:

- **`north`** — one-call orientation returning focus nodes, bounded source excerpts (up to 3 files, 2,000 chars by default, `code_budget_chars` to widen), binding trust, prior memory, a suggested next move, and `honest_gaps`.
- **`seek`** — structural + semantic candidate retrieval with an explicit trust envelope (`reverify` / `unprovable`) and sufficiency signal.
- **`impact`** — inspect structural neighbors of a node before changing it.

`tools/list` advertises a small **core menu of 15 tools** by default (`M1ND_TOOL_TIER=core` or unset):
`north`, `memorize`, `boot_memory`, `ingest`, `seek`, `search`, `view`, `surgical_context`, `impact`, `health`, `trust_selftest`, `session_handshake`, `recovery_playbook`, `doctor`, `help`.

- `M1ND_TOOL_TIER=investigate` advertises **20 tools** (the 15 core tools plus 5 session-local graph snapshot tools: `lock_create`, `lock_watch`, `lock_diff`, `lock_rebase`, `lock_release`).
- `M1ND_TOOL_TIER=full` advertises all **147 registered tools**.
- `help` catalogs the full 147-tool registry at every tier. Unlisted tools remain callable by name when your host permits calling tools outside `tools/list`.

---

## Memory and freshness

- **`memorize`** stores durable claims with repo-relative evidence paths in the runtime's `agent-memory` sidecar. `north` and `boot_memory` recall them with author and real age (absent when unknown, never faked).
- **Isolated CLI freshness:** Only when reusing the automatic isolated cache does the agent CLI run `ingest {mode: "refresh"}` before retrieval. The cache is keyed by canonical worktree root + Git branch + `HEAD` and reverifies root/branch/`HEAD` at query boundaries; owned memory is prepared alongside code before the 60% shrink guard ([docs/AUTOMATIC-GRAPH-FRESHNESS.md](docs/AUTOMATIC-GRAPH-FRESHNESS.md), [docs/REFRESH-OWNED-MEMORY.md](docs/REFRESH-OWNED-MEMORY.md)).
- **Attached-owner freshness:** Attached-owner freshness follows the owner's own semantics. On a live owner, an agent can refresh an already-declared root with `ingest {mode: "refresh"}` from that exact root.
- **Explicit boundaries:** refresh runs on invocation (there is no continuous code watcher); boundary checks are not an atomic filesystem snapshot (`atomic_snapshot: false`, `working_tree_contents: not_attested_by_identity`); restored claims carry `claim_origin: not_attested` and `claims_freshness: not_revalidated` — verify claims against current source before acting.

---

## Multiple sessions on one brain

To share one live graph and memory across multiple agents or editor windows, run **one served owner** and attach every session to it:

```bash
# 1. Start one persistent owner on loopback (default port 1337; drop --no-gui for the web UI)
m1nd-mcp --serve --no-gui --port 1337 --runtime-dir /path/to/runtime

# 2. Point each agent session at the owner (run from inside a covered repo)
m1nd-mcp --attach auto --stdio
```

- `--attach auto` loads no graph, builds no engines, and takes no writer lease. It asks the instance registry two questions in order: first for a live serve owner matching this `runtime_root`, and failing that for a live serve owner whose declared ingest roots cover the caller's repo (resolving git worktrees to their main repo and refusing if multiple owners match). Use `m1nd-mcp --discover-owner` to inspect that decision as JSON, or override with `--attach http://127.0.0.1:1337 --stdio` / `M1ND_ATTACH_URL`.
- **Single-writer rule:** a second plain `m1nd-mcp --stdio` process targeting an already-owned `runtime_root` **refuses to start** (`runtime_root is already owned`) and exits nonzero instead of racing on disk. Use `--attach auto` to share a live owner, or `--read-only` (`M1ND_READ_ONLY=1`) for lease-free read-only queries.

See [docs/deployment.md](docs/deployment.md) for `launchd` and `systemd` service templates.

---

## Limits

- **Extraction is approximate.** `m1nd-ingest` ships 5 manual extractors (Python, TypeScript/JavaScript, Rust, Go, Java) and 22 tree-sitter languages in the default Tier 2 build (14 Tier 1 + 8 Tier 2), plus a generic `contains`-only fallback. It is not a compiler semantic model: macros, dynamic dispatch, and external imports stay unresolved, and Python call extraction covers a conservative plain same-module subset.
- **Calibration is contained.** All v1 calibration rows are unqualified evidence ([docs/CALIBRATION-QUALIFICATION.md](docs/CALIBRATION-QUALIFICATION.md)). Prediction abstains and `seek` stays at `reverify` or `unprovable`; running calibration does not enable `act`. Direct proof (source, compiler, tests) remains the final truth.
- **Learning feedback is relevance-only.** `learn(correct|wrong)` adjusts graph relevance weights only — it records no defects, false alarms, observed co-change, or automatic antibodies ([docs/LEARNING-EVENT-SEMANTICS.md](docs/LEARNING-EVENT-SEMANTICS.md)). Ambiguous `partial` feedback refuses without mutation; zero strength is a no-op; public learning and calibration authority floors remain blocked.
- **Generic `ingest` is policy-disabled.** `ingest` with `mode: "replace"` or `"merge"` refuses for every client, and the published schema carries neither `project_root` nor `allow_overlap`. Only exact-root `ingest {mode: "refresh"}` is callable by an agent; creating a brain on a shared runtime requires the human terminal command `m1nd init --birth <repo>`.
- **Local is not remote-authenticated.** `--serve` binds `127.0.0.1` only and refuses non-loopback binds (even with legacy `--allow-remote`). Loopback does not authenticate other same-UID local processes; see [SECURITY.md](SECURITY.md).
- **Optional embeddings fetch a local model.** The default `embed` feature loads `model2vec-rs` (`potion-base-8M`, ~29 MB, fetched and cached on first use, with trigram fallback if unavailable). Build without it using `--no-default-features --features serve`.
- **Not yet in this 2.0.0 tree:** legacy brain upgrade/rollback and signed 2.0.0 release/updater artifacts are not yet qualified or published.

---

## Workspace and further reading

Five Rust workspace crates (`m1nd-core`, `m1nd-ingest`, `m1nd-mcp`, `m1nd-control`, `m1nd-runnerd`) plus the embedded web UI (`m1nd-ui/`), npm wrapper (`npm/`), and agent skills (`skills/`).

- [Documentation index](docs/README.md) · [Examples](EXAMPLES.md) · [Host integration matrix](docs/HOST-INTEGRATION-MATRIX.md)
- [Automatic freshness contract](docs/AUTOMATIC-GRAPH-FRESHNESS.md) · [Deployment](docs/deployment.md)
- [Contributing](CONTRIBUTING.md) · [Changelog](CHANGELOG.md) · [Security](SECURITY.md) · [License](LICENSE) (MIT)
