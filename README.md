<p align="center">
  <img src=".github/m1nd-mark-deviance.svg" width="560" alt="m1nd — a DEViance Intelligence code graph" />
</p>

# m1nd

A local code graph served over MCP — the map an agent reads before it touches your code.

**Start here** — the five verbs I reach for first: <img src=".github/m1nd-mark-deviance-icon.svg" height="15" alt="" /> [`north`](docs/wiki/src/api-reference/lifecycle.md#m1ndnorth) to orient · <img src=".github/m1nd-mark-deviance-icon.svg" height="15" alt="" /> [`seek`](docs/wiki/src/api-reference/exploration.md#m1ndseek) to find by purpose · <img src=".github/m1nd-mark-deviance-icon.svg" height="15" alt="" /> [`impact`](docs/wiki/src/api-reference/analysis.md#m1ndimpact) to see what a change touches · <img src=".github/m1nd-mark-deviance-icon.svg" height="15" alt="" /> [`health`](docs/wiki/src/api-reference/lifecycle.md#m1ndhealth) to check the graph · <img src=".github/m1nd-mark-deviance-icon.svg" height="15" alt="" /> [`help`](docs/wiki/src/api-reference/overview.md) to reach every other verb

## What this is

m1nd is an MCP server, written in Rust, that turns a repository into a graph of files, symbols, and the edges between them, then answers an agent's questions about that graph: where a behavior lives, what reaches a function, whether the graph even covers the repository the agent is standing in. The graph and its memory sidecar stay on your machine, and graph queries need no LLM API key. Installation, updates, optional model downloads, and network-backed document providers can use the network; your MCP host has its own data policy.

It is built for agents first. The default menu is 15 verbs. Every answer says how far to trust it. `absent`, `abstain`, and `insufficient_evidence` are answers, not failures.

## Why this exists

I run coding agents every day, and the expensive part is never the edit. It is the agent reading whole files to find where something lives, then changing a function without knowing what calls it. m1nd's own repository is 18,078 nodes. No agent holds that in context, and none should have to. So the agent asks the graph, gets a file and a line span, opens the span, and confirms. The graph locates. The source decides.

## Capabilities

### FIND CODE BY PURPOSE. GET THE SPAN.

Ask `seek` a question in plain language — "where is retry backoff decided" — and it returns ranked files with a trust envelope and a sufficiency signal. On the `main` source tree (2.0, unreleased), each of the first ten file rows also carries up to three symbols with `line_start` and `line_end`, best first ([#602](https://github.com/maxkle1nz/m1nd/pull/602)), so the agent opens a span instead of a whole file. On two fixed question sets, the facts whose span came back rose from 10 to 17 of 24 and from 5 to 11 of 24; packets shrank by about 8%; file ranking stayed identical in 16 of 16 questions. Since [#601](https://github.com/maxkle1nz/m1nd/pull/601) (2.0, unreleased) the ranking no longer depends on hash order, so the same graph ranks the same way twice. **PROVEN (n=16 questions, 48 facts).**

### SEE WHAT A CHANGE TOUCHES BEFORE YOU MAKE IT.

`impact` walks the graph out from one node. With `direction: "reverse"` it follows the edges that point into the node — for a function, the code that reaches it, which is what breaks when it changes. Aim it at a function `seek` returned, not at a type or a file, where same-named methods blur together. Routing "who calls / what breaks" questions to `impact` added one or two facts out of 24 over `seek` alone. That is a small gain on a small sample: a direction, not a law. Confirm callers with your compiler or a literal `search` before you edit. **PROVEN (small n).**

### ORIENTATION IN ONE CALL.

`north` takes the task as a sentence and answers in one round trip: focus nodes, bounded source excerpts of the top ones (up to 3 files and 2,000 characters by default; `code_budget_chars` widens it), whether the graph is bound to the repository you are in, prior memory with author and real age, a suggested next move, and `honest_gaps` — what it could not see. It is the one verb agents picked up on their own in real traffic, without being told to. That is adoption, not a measured benefit. **OBSERVED.**

### BOUNDED TRUST. REAL REFUSALS.

m1nd says what it does not know. Under the current calibration every `seek` envelope stays at `reverify` or `unprovable` — open the source and check — and prediction abstains: all v1 calibration rows are unqualified evidence, and running calibration does not enable `act`. A verb above the generic authority floor refuses with `generic_action_authority_required` instead of trying. A graph bound to another repository answers with `caller_root_mismatch`, and the write laws forbid writing through it. The default menu is 15 verbs; `help` is one of them, and it catalogs every registered verb — on the menu or not — with its schema and its `POLICY-DISABLED` mark where one applies. **RULE.**

### ONE LOCAL BRAIN FOR MANY SESSIONS.

Run one served owner per runtime and attach every other session to it: they read one graph instead of each building its own. A second plain stdio process on an owned runtime refuses to start rather than race the owner on disk. One owner can host several project brains and routes each attached client to its own by caller root. m1nd's repository — 18,078 nodes, 78,981 edges — ingests in 19 s; an exact-root refresh of a ~2.6k-node repository took 0.49 s. The graph is as fresh as your last refresh; there is no continuous watcher. **PROVEN (sandbox).**

## Proof

What I measured, on how much, and where it stops.

| Claim | Measure | n | Caveat |
|---|---|---|---|
| Symbol spans on `seek` (#602, 2.0, unreleased) | Facts whose span is returned: 10→17 of 24 and 5→11 of 24; packets ~8% smaller; file ranking identical 16/16 | 16 questions, 48 facts | Two fixed question sets with a check fixed in advance; not a blind benchmark |
| Stable ranking (#601, 2.0, unreleased) | Before the fix, two runs of one binary agreed on 14/16 and 13/16 questions; sums now run in a fixed order and ties break by node id | 16 questions | Every ranking change now runs a bracketed old/new/old comparison |
| Blast radius with `impact` | Relation questions routed to `impact`: +1/+2 facts of 24 (11/24 against 10/24 with symbols alone) | 24 facts | Small sample; re-measure before treating it as a rule |
| Ingest and refresh | 18,078 nodes and 78,981 edges in 19 s; exact-root refresh 0.49 s | one machine | Sandbox; the refresh was measured on a ~2.6k-node repository |
| Standing decisions | A decisions block in `AGENTS.md` answered 5/5 by cold sessions on three hosts; the same facts through `memorize`: 0/5 | 5 per channel | This is why I do not claim memory carries decisions |
| Orientation | `north` is the only verb agents adopted unprompted | real traffic | An observation; the benefit is not measured |

Not measured yet: whether m1nd beats `grep` and plain file reads on real tasks. That experiment has not run, so I make no such claim.

## Install

<!-- Keep the former setup anchors working for existing links. -->
<a id="get-started"></a>
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

Then ask your connected agent:

> Use m1nd to orient on this repository. Find where configuration is loaded, show the relevant source locations, and tell me what you still need to verify. Do not edit files.

The agent should call `north` first, then `seek` and source reads as needed. **Success means the response covers the correct repository and points to real code you can open.** A `reverify` verdict is expected under the current calibration containment: inspect the evidence. A missing graph or a wrong-workspace binding needs recovery; an empty result may need a different query. [Quick-start troubleshooting](docs/wiki/src/tutorials/quickstart.md) covers missing binaries, host configuration, and binding errors; [your first query](docs/wiki/src/tutorials/first-query.md) shows the actual MCP calls.

<a id="unreleased-main"></a>

### Unreleased main: the 2.0.0 source tree

Everything in this section is **unreleased**: it describes the current `main` source tree, not the published 1.6.5 artifacts. `main` adds PR580's learning-event separation, stable ranking (#601), symbol spans on `seek` (#602), automatic-freshness guards, a leaner tree, and a reworked host installer; see the [2.0.0 changelog entry](CHANGELOG.md) and the [release proposal](docs/RELEASE-2.0.0-PROPOSAL.md). The [dependency security updates](CHANGELOG.md#security) change the source lockfile; they do not update the published 1.6.5 binaries.

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
node npm/bin/m1nd.js version
node npm/bin/m1nd.js doctor
```

Place `./target/release/m1nd-mcp` on your `PATH` or pass `--binary /path/to/m1nd-mcp`. The commands below use the checkout's own wrapper, `node npm/bin/m1nd.js`. Until a 2.0.0 package is published, `npx @maxkle1nz/m1nd` resolves to 1.6.5, which does not include the installer changes described here.

<a id="host-setup"></a>

#### Host setup with the 2.0.0 installer

`hosts status` and `hosts plan` are read-only; `hosts apply` is a dry run unless you pass `--yes`:

```bash
node npm/bin/m1nd.js hosts status --host claude --project ~/src/my-repo
node npm/bin/m1nd.js hosts plan   --host claude --project ~/src/my-repo --binary /path/to/m1nd-mcp
node npm/bin/m1nd.js hosts apply  --host claude --project ~/src/my-repo --binary /path/to/m1nd-mcp --yes
```

The installer writes skills, MCP config, and host-owned hooks for Claude Code (`claude`), Codex (`codex`), and Antigravity (`antigravity`); pass `--no-skills`, `--no-config`, or `--no-hooks` to skip a step. Every config write is atomic, and existing files are backed up under `~/.m1nd/backups/` with private `0700`/`0600` permissions on Unix. For Claude Code it writes `<project>/.mcp.json`, never modifies `~/.claude.json`, and prints the `SessionStart` hook for you to paste instead of writing it. Exact targets for every host are in the [host integration matrix](docs/HOST-INTEGRATION-MATRIX.md). Restart or reconnect the host after a change, then verify the binding in-session with `trust_selftest` or `session_handshake`.

<a id="first-use"></a>

#### First use on the source tree

A fresh repository needs a graph before retrieval works:

- **Direct stdio with an explicit workspace grant:** when a host launches `m1nd-mcp --stdio --no-gui` with `M1ND_WORKSPACE_ROOT=~/src/my-repo` and a private writable runtime, startup prepares the empty bound graph automatically (see [agent autonomy](docs/AGENT-AUTONOMY.md)).
- **Explicit human birth (for a shared or pre-created runtime):** run once in a terminal yourself: `node npm/bin/m1nd.js init --birth ~/src/my-repo --binary /path/to/m1nd-mcp`. An agent offers this command; it never runs it.
- **Host-free CLI probe:** `node npm/bin/m1nd.js agent first-minute --repo ~/src/my-repo --binary /path/to/m1nd-mcp --query "where is configuration loaded?" --json`. Add `--no-attach --state-dir /path/to/private-cache` (outside the repository) to force an isolated cache. See [storage selection and sandbox limits](docs/AGENT-AUTONOMY.md#explicit-storage-in-an-already-writable-workspace-unreleased-cli).

<a id="multiple-sessions-on-one-brain"></a>

#### Many sessions, one owner

```bash
# 1. Start one persistent owner on loopback (default port 1337; drop --no-gui for the web UI)
m1nd-mcp --serve --no-gui --port 1337 --runtime-dir /path/to/runtime

# 2. Point each agent session at the owner (run from inside a covered repo)
m1nd-mcp --attach auto --stdio
```

`--attach auto` loads no graph and takes no writer lease. It looks first for a live owner of the same runtime root, then for one whose declared roots cover the caller's repository, resolving git worktrees to their main repository and refusing when several owners match. `m1nd-mcp --discover-owner` prints that decision as JSON; `--attach http://127.0.0.1:1337 --stdio` or `M1ND_ATTACH_URL` overrides it. Do not export `M1ND_WORKSPACE_ROOT` or the other `*_ROOT` variables globally: they override the working directory and route every session to one project. Service templates for `launchd` and `systemd` are in [deployment](docs/deployment.md).

## What to try next

| Task | Start with | Check before acting |
|---|---|---|
| Understand unfamiliar code | `north` → `seek` → open the returned span | Is the graph bound to this repository? Read `honest_gaps`. |
| Find where a behavior lives | `seek` with a plain-language question | Open the symbol span. A top score near 0.4 with unrelated hits usually means the concept is absent; confirm with a literal `search`. |
| Plan a cross-file change | `impact` with `direction: "reverse"` on a function | Confirm callers with the compiler, tests, or a literal `search`; unresolved edges are gaps. |
| Check that the graph is current | `health` once, then `ingest {"mode": "refresh"}` from the exact declared root | Compare `git rev-parse HEAD` with the commit you last ingested. |
| Find a verb that is not on the menu | `help` with `tool_name` | Read its `POLICY-DISABLED` mark before planning around it. |

See [worked examples](EXAMPLES.md), [use cases](docs/use-cases.md), and the [tool reference](docs/wiki/src/api-reference/overview.md). Published 1.6.5 also offers `M1ND_TOOL_TIER=investigate`: the core plus five session-local snapshot tools ([recipe](docs/GRAPH-CHANGE-TRACKING.md)). A listed or callable tool may still require authority the current session does not have.

## For agents

If you are an agent, this section is yours. The full manual is [docs/AGENT-BEST-PRACTICES.md](docs/AGENT-BEST-PRACTICES.md); every practice in it carries PROVEN, OBSERVED, or RULE. The short version:

1. Pass a stable, descriptive `agent_id` on every call — `<host>-<role>`, not a random id.
2. `north` to orient, `seek` for where and how, `impact` with `direction: "reverse"` for who calls. Open the span before you quote it: m1nd locates, your file read confirms.
3. Call `health` once per session, not per question. When `HEAD` moved, refresh from the exact declared root.
4. Read the envelope. `reverify` means open the source. `absent` and `abstain` are answers.
5. Before you conclude a capability is missing, ask `help` with `tool_name`. Ignore suggestions that name `learn`, `ghost_edges`, `audit`, or `ingest` with merge or replace: they refuse every generic client.
6. Put standing decisions in the agent file your host always loads (`AGENTS.md`, `CLAUDE.md`), not in `memorize`: 5/5 against 0/5.

Host packs ship in [skills/](skills/) (`m1nd-first`, `m1nd-guardian`, `m1nd-operator`); setup for coding agents is in the [agent installation guide](llms-install.md).

## Status — honest

<a id="what-it-does-and-does-not-prove"></a>
<a id="limits"></a>
<a id="memory-and-freshness"></a>

- **Published:** 1.6.5 — GitHub/native, crates.io, and npm `latest` — from the commit named in [Install](#install).
- **Unreleased:** everything on `main` after it, including PR580's learning-event separation, #601, #602, and the 2.0.0 host installer. Versions on `main` say 2.0.0; no 2.0.0 artifact, crate, or npm package is published or signed, and legacy brain upgrade/rollback is not qualified.
- **Not proven:** that m1nd beats `grep` on real tasks (the experiment has not run); that `north` helps beyond being adopted; that the `impact` gain holds beyond a small sample; that memory carries decisions (measured 0/5).
- **Calibration stays contained:** all v1 calibration rows are [unqualified evidence](docs/CALIBRATION-QUALIFICATION.md); prediction abstains; `seek` stays at `reverify` or `unprovable`; running calibration does not enable `act`. Source, compiler, and tests remain the final truth.
- **The authority plane is not callable by agents:** verbs above the generic floor — `learn`, `ghost_edges`, `apply`, the daemon and auto-ingest controls, generic `ingest` with replace or merge, and others — refuse every generic MCP or REST caller, and no payload lifts that. Creating a brain on a shared runtime is the human terminal command `m1nd init --birth <repo>`. So I make no claim that m1nd learns from use — the `learn` feedback verb sits behind the floor — and `predict` answers from structure alone: git co-change needs `ghost_edges`.
- **Freshness is on request:** the graph is as fresh as its last refresh, with no continuous watcher. A git worktree binds to its main repository's brain, so a worktree on another commit reads main's lines; re-read spans in your own checkout.
- **Extraction is approximate:** dedicated extractors and tree-sitter grammars, not a compiler. Macros, dynamic dispatch, and external imports stay unresolved.
- **Local is not authenticated:** `--serve` binds loopback only and refuses anything else; loopback does not authenticate other processes of the same user. See [SECURITY.md](SECURITY.md).
- **What I do not claim:** graph visualization (the web UI is a tree and panels), multi-agent coordination, autonomous editing, security analysis. Some verbs in those areas exist in the registry; none is on the menu, and none is proven.
- **Stability contract:** the 15-verb menu, `help` included, is the stable surface. Hidden verbs stay reachable by name through `help` but are unstable: one may leave in a minor release, with a CHANGELOG note.

## Continue from here

MIT licensed — fork it. The [documentation index](docs/README.md), [examples](EXAMPLES.md), and [changelog](CHANGELOG.md) go deeper; contributions follow [CONTRIBUTING.md](CONTRIBUTING.md), and security reports follow [SECURITY.md](SECURITY.md). Found a wrong answer? Open an issue with the call, the response, and what the source says. That is the proof I need.

The workspace: five Rust crates (`m1nd-core`, `m1nd-ingest`, `m1nd-mcp`, `m1nd-control`, `m1nd-runnerd`), the embedded web UI (`m1nd-ui/`), the npm wrapper (`npm/`), and the agent skills (`skills/`).

---
<p align="center">
  <img src=".github/m1nd-mark-deviance-icon.svg" width="44" alt="m1nd mark" />
  <br>
  <sub>An <b>m1nd</b> graph by <b>DEViance Intelligence</b> — <i>beyond the edge</i> · by <a href="https://github.com/maxkle1nz">Max Kle1nz</a></sub>
</p>
