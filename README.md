<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

# m1nd

**A local code graph and evidence-linked memory server for MCP coding agents.**

m1nd builds a structural graph of your repository in Rust, answers orientation and impact queries over MCP, and carries verified findings across sessions with explicit trust signals and declared gaps. The graph and memory live on your machine; graph queries do not require an LLM API key. Installation, updates, optional model downloads, and network-backed document providers can use the network; your MCP host has its own data policy.

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?label=npm" alt="Published npm version" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT license" /></a>
</p>

[Get started](#get-started) · [Unreleased main](#unreleased-main) · [What to try next](#what-to-try-next) · [Limits](#what-it-does-and-does-not-prove) · [Documentation](docs/README.md)

---

## Get started

<!-- Keep the former setup anchors working for existing links. -->
<a id="sixty-seconds"></a>
<a id="quick-start"></a>
<a id="install"></a>

**Version matters:** [GitHub/native 1.6.5](https://github.com/maxkle1nz/m1nd/releases/tag/v1.6.5), crates.io packages, and [npm 1.6.5](https://www.npmjs.com/package/@maxkle1nz/m1nd/v/1.6.5) are published from immutable commit `363bc97f2336c73e90802916804dccd2109c5746`; npm `latest` is 1.6.5 as of 2026-10-12. Everything merged after that release, from the learning-event separation in PR580 to the 2.0.0 source tree now on `main`, is **Unreleased**. The npm wrapper and the `m1nd-core`, `m1nd-ingest`, and `m1nd-mcp` source versions on `main` say 2.0.0, but no 2.0.0 artifact, crate, or npm package is published.

### Published 1.6.5: get a first result

You need **Node.js 18+** and a repository you can read. [Install the signed native 1.6.5 artifact](docs/INSTALL-1.6.5.md), then run this POSIX-shell recipe from your repository:

```bash
export M1ND_BINARY="$HOME/.local/share/m1nd/versions/1.6.5/m1nd-mcp"
"$M1ND_BINARY" --version
# Require native 1.6.5 at 363bc97f2336c73e90802916804dccd2109c5746.
(
  unset M1ND_RUNTIME_DIR M1ND_MCP_ARGS
  npx -y @maxkle1nz/m1nd@1.6.5 agent first-minute --repo . \
    --binary "$M1ND_BINARY" --query "where is configuration loaded?" --json
)
```

`first-minute` discovers a covering served owner first. Otherwise, the matching wrapper/native runtime prepares only the exact launcher-granted repository in an external private cache. Source files remain inputs. Only when reusing the automatic isolated cache does the CLI refresh the declared root before retrieval. Attached-owner freshness follows the owner's own semantics; inspect that owner's version, repository binding, and freshness rather than inferring them from the selected client binary. Check `runtime.boot`, returned source locations, and gaps. This is not authority to create a shared brain, move a binding, or call generic `ingest`.

To connect an MCP host, follow the [1.6.5 quick start](docs/wiki/src/tutorials/quickstart.md). Direct stdio needs an explicit workspace grant and an existing external private runtime; a shared owner has separate preparation and authority requirements. See [agent autonomy](docs/AGENT-AUTONOMY.md).

### Your first useful result

Ask your connected agent:

> Use m1nd to orient on this repository. Find where configuration is loaded, show the relevant source locations, and tell me what you still need to verify. Do not edit files.

The agent should call `north` first, then use `seek` and source inspection as needed. **Success means the response covers the correct repository and points to real code you can open.** A `reverify` verdict is expected under the current calibration containment: inspect the evidence. A missing graph or wrong-workspace binding needs recovery; an empty result may need a different query.

[Quick-start troubleshooting](docs/wiki/src/tutorials/quickstart.md) covers missing binaries, host configuration, and binding errors. [Your first query](docs/wiki/src/tutorials/first-query.md) shows the actual MCP calls.

<a id="unreleased-main"></a>

### Unreleased main: the 2.0.0 source tree

Everything in this section is **unreleased**: it describes the current `main` source tree, not the published 1.6.5 artifacts. `main` adds PR580's learning-event separation, automatic-freshness guards, a leaner tree, and a reworked host installer; see the [2.0.0 changelog entry](CHANGELOG.md) and the [release proposal](docs/RELEASE-2.0.0-PROPOSAL.md). The [dependency security updates](CHANGELOG.md#security) change the source lockfile; they do not update the published 1.6.5 binaries.

A version string alone does not establish which changes a binary contains, so select an exact `main` revision and use its matching wrapper and native runtime. `m1nd-mcp --version` reports the commit it was built from.

#### Pinned contributor recipe: learning-event separation

To test PR580's learning fix by itself, pin the commit that merged it. `--binary` selects the executable used by the CLI, but default discovery can still attach it to an older covering owner. Use `--no-attach` and clear ambient runtime/attach overrides to run this exact revision in an automatic isolated cache:

```bash
git clone https://github.com/maxkle1nz/m1nd.git
cd m1nd
git checkout --detach 74d0348c9a5a1e0d5d5e5b5a784ee4f3771d0808
cargo build --release -p m1nd-mcp
./target/release/m1nd-mcp --version
(
  unset M1ND_RUNTIME_DIR M1ND_MCP_ARGS M1ND_ATTACH_URL
  node npm/bin/m1nd.js agent first-minute --repo /absolute/path/to/your/repo \
    --binary ./target/release/m1nd-mcp --no-attach --query "map this repo" --json
)
```

PR580's learning semantics are available on `main` but are absent from the sealed 1.6.5 artifacts. Automatic isolated preparation already shipped in 1.6.5; it is not a preview feature of PR580. See [the launcher contract](docs/AGENT-AUTONOMY.md#public-npm-cli-and-process-status). [The contributor development kit](docs/DEVELOPMENT.md) provides checkout-private state for development.

#### Build the 2.0.0 source tree

You need **Node.js 18+** (for the dependency-free `@maxkle1nz/m1nd` CLI/installer) and **Rust (stable)** to build the native `m1nd-mcp` binary from a `main` checkout:

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

The commands below use the checkout's own wrapper, `node npm/bin/m1nd.js`. Until a 2.0.0 package is published, `npx @maxkle1nz/m1nd` resolves to 1.6.5, which does not include the installer changes described here.

<a id="host-setup"></a>

#### Host setup with the 2.0.0 installer

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

<a id="first-use"></a>

#### First use on the source tree

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
  Add `--no-attach --state-dir /path/to/private-cache` (outside the repository) to force an isolated cache; `agent scope` previews paths without writing. See [storage selection and sandbox limits](docs/AGENT-AUTONOMY.md#explicit-storage-in-an-already-writable-workspace-unreleased-cli); this grants no permissions and does not migrate existing memory.

Start every task with `north`, narrow candidates with `seek` or `search`, read code with `surgical_context` or `view`, and inspect blast radius with `impact` before editing:

- **`north`** — one-call orientation returning focus nodes, bounded source excerpts (up to 3 files, 2,000 chars by default, `code_budget_chars` to widen), binding trust, prior memory, a suggested next move, and `honest_gaps`.
- **`seek`** — structural + semantic candidate retrieval with an explicit trust envelope (`reverify` / `unprovable`) and sufficiency signal.
- **`impact`** — inspect structural neighbors of a node before changing it.

On `main`, `tools/list` advertises a small **core menu of 15 tools** by default (`M1ND_TOOL_TIER=core` or unset):
`north`, `memorize`, `boot_memory`, `ingest`, `seek`, `search`, `view`, `surgical_context`, `impact`, `health`, `trust_selftest`, `session_handshake`, `recovery_playbook`, `doctor`, `help`.

- `M1ND_TOOL_TIER=investigate` advertises **20 tools** (the 15 core tools plus 5 session-local graph snapshot tools: `lock_create`, `lock_watch`, `lock_diff`, `lock_rebase`, `lock_release`).
- `M1ND_TOOL_TIER=full` advertises all **147 registered tools**.
- `help` catalogs the full 147-tool registry at every tier. Unlisted tools remain callable by name when your host permits calling tools outside `tools/list`.

<a id="memory-and-freshness"></a>

#### Memory and freshness on main

- **`memorize`** stores durable claims with repo-relative evidence paths in the runtime's `agent-memory` sidecar. `north` and `boot_memory` recall them with author and real age (absent when unknown, never faked).
- **Isolated CLI freshness:** Only when reusing the automatic isolated cache does the agent CLI run `ingest {mode: "refresh"}` before retrieval. The cache is keyed by canonical worktree root + Git branch + `HEAD` and reverifies root/branch/`HEAD` at query boundaries; owned memory is prepared alongside code before the 60% shrink guard ([docs/AUTOMATIC-GRAPH-FRESHNESS.md](docs/AUTOMATIC-GRAPH-FRESHNESS.md), [docs/REFRESH-OWNED-MEMORY.md](docs/REFRESH-OWNED-MEMORY.md)).
- **Attached-owner freshness:** Attached-owner freshness follows the owner's own semantics. On a live owner, an agent can refresh an already-declared root with `ingest {mode: "refresh"}` from that exact root.
- **Explicit boundaries:** refresh runs on invocation (there is no continuous code watcher); boundary checks are not an atomic filesystem snapshot (`atomic_snapshot: false`, `working_tree_contents: not_attested_by_identity`); restored claims carry `claim_origin: not_attested` and `claims_freshness: not_revalidated` — verify claims against current source before acting.

<a id="multiple-sessions-on-one-brain"></a>

#### Multiple sessions on one brain

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

<a id="limits"></a>

#### Limits on main

- **Extraction is approximate.** `m1nd-ingest` ships 5 manual extractors (Python, TypeScript/JavaScript, Rust, Go, Java) and 22 tree-sitter languages in the default Tier 2 build (14 Tier 1 + 8 Tier 2), plus a generic `contains`-only fallback. It is not a compiler semantic model: macros, dynamic dispatch, and external imports stay unresolved, and Python call extraction covers a conservative plain same-module subset.
- **Calibration is contained.** All v1 calibration rows are unqualified evidence ([docs/CALIBRATION-QUALIFICATION.md](docs/CALIBRATION-QUALIFICATION.md)), as in published 1.6.5. Prediction abstains and `seek` stays at `reverify` or `unprovable`; running calibration does not enable `act`. Direct proof (source, compiler, tests) remains the final truth.
- **Learning feedback is relevance-only.** `learn(correct|wrong)` adjusts graph relevance weights only — it records no defects, false alarms, observed co-change, or automatic antibodies ([docs/LEARNING-EVENT-SEMANTICS.md](docs/LEARNING-EVENT-SEMANTICS.md)). Ambiguous `partial` feedback refuses without mutation; zero strength is a no-op; public learning and calibration authority floors remain blocked. This change is absent from sealed 1.6.5.
- **Generic `ingest` is policy-disabled.** `ingest` with `mode: "replace"` or `"merge"` refuses for every client, and the published schema carries neither `project_root` nor `allow_overlap`. Only exact-root `ingest {mode: "refresh"}` is callable by an agent; creating a brain on a shared runtime requires the human terminal command `m1nd init --birth <repo>`.
- **Local is not remote-authenticated.** `--serve` binds `127.0.0.1` only and refuses non-loopback binds (even with legacy `--allow-remote`). Loopback does not authenticate other same-UID local processes; see [SECURITY.md](SECURITY.md).
- **Optional embeddings fetch a local model.** The default `embed` feature loads `model2vec-rs` (`potion-base-8M`, ~29 MB, fetched and cached on first use, with trigram fallback if unavailable). Build without it using `--no-default-features --features serve`.
- **Not yet in this 2.0.0 tree:** legacy brain upgrade/rollback and signed 2.0.0 release/updater artifacts are not yet qualified or published.

## What to try next

| Task | Start with | Check before acting |
|---|---|---|
| Understand unfamiliar code | `north` → `seek` → `surgical_context` | Open the returned source; check the workspace binding and trust envelope. |
| Plan a cross-file change | `impact` and `why` on a returned node | Confirm callers and imports with source/compiler tooling; unresolved edges are gaps. |
| Resume an investigation | `north`, `boot_memory`, or `trail_resume` | Review evidence, author, age, and any stale flags before reusing a claim. |
| Preserve a verified finding | `memorize` with repo-relative evidence paths | Store what you actually verified, not an agent's untested conclusion. |
| Coordinate parallel agents | One served owner and attached clients | Treat overlap notices as a request to coordinate; they do not prevent edits. |

See [worked examples](EXAMPLES.md), [use cases](docs/use-cases.md), and the [tool reference](docs/wiki/src/api-reference/overview.md). The default tool menu is deliberately small. `help` discovers the full registry; `M1ND_TOOL_TIER=full` expands the advertised menu. Published 1.6.5 also offers `M1ND_TOOL_TIER=investigate`: the core plus five session-local snapshot tools. See the [beginner and advanced recipe](docs/GRAPH-CHANGE-TRACKING.md). A listed or callable tool may still require authority the current session does not have.

## What it does and does not prove

These boundaries hold for published 1.6.5 and for `main`; the [main-only limits](#limits) add detail for the source tree.

- **Best fit:** structural navigation, change preparation, and continuity across repeated work. For a tiny edit or exact string lookup, a file read or `rg` may be enough.
- **Extraction is approximate.** Language support varies, macro-generated and dynamic references can be missed, and external imports may remain unresolved. A graph is not a compiler's semantic model.
- **Trust is conditional.** Published 1.6.5 and current `main` treat all v1 calibration rows as [unqualified evidence](docs/CALIBRATION-QUALIFICATION.md): prediction abstains and the seek envelope stays at `reverify` or `unprovable`, while scores and rankings remain available. Running calibration does not enable `act`. Use source, compiler, tests, and runtime checks for your change. The [clock repair](docs/LEARNING-TIME-UNITS.md) does not establish statistical quality.
- **Writes have extra boundaries.** Preview/commit tools, proof permits, and authority checks are separate from read-only retrieval. Graph/static verification is not a substitute for executed tests. See [write reference](docs/wiki/src/api-reference/lifecycle.md).
- **Local does not mean authenticated.** The served owner refuses non-loopback binds; loopback alone does not authenticate other local processes. See [deployment](docs/deployment.md) and [security](SECURITY.md).

## Workspace and further reading

Five Rust workspace crates (`m1nd-core`, `m1nd-ingest`, `m1nd-mcp`, `m1nd-control`, `m1nd-runnerd`) plus the embedded web UI (`m1nd-ui/`), npm wrapper (`npm/`), and agent skills (`skills/`).

- [Documentation index](docs/README.md) · [Examples](EXAMPLES.md) · [Host integration matrix](docs/HOST-INTEGRATION-MATRIX.md)
- [Agent installation guide](llms-install.md) · [Automatic freshness contract](docs/AUTOMATIC-GRAPH-FRESHNESS.md) · [Deployment](docs/deployment.md)
- [Contributing](CONTRIBUTING.md) · [Changelog](CHANGELOG.md) · [Security](SECURITY.md) · [License](LICENSE) (MIT)
