🇬🇧 [English](README.md) | 🇧🇷 [Português](i18n/README.pt-BR.md) | 🇪🇸 [Español](i18n/README.es.md) | 🇮🇹 [Italiano](i18n/README.it.md) | 🇫🇷 [Français](i18n/README.fr.md) | 🇩🇪 [Deutsch](i18n/README.de.md) | 🇨🇳 [中文](i18n/README.zh.md) | 🇯🇵 [日本語](i18n/README.ja.md)

<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="400" />
</p>

<h3 align="center">Operational intelligence for coding agents</h3>

<p align="center">
  <strong>A local intelligence layer for coding agents.</strong><br/>
  <em>Local execution. MCP over stdio. Optional HTTP/UI surface in the default build.</em>
</p>

<p align="center">
  <a href="https://crates.io/crates/m1nd-core"><img src="https://img.shields.io/crates/v/m1nd-core.svg" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://docs.rs/m1nd-core"><img src="https://img.shields.io/docsrs/m1nd-core" alt="docs.rs" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/releases"><img src="https://img.shields.io/badge/release-v0.9.0--beta.0-00f5ff" alt="Release" /></a>
</p>

<p align="center">
  <a href="#what-m1nd-is">What m1nd Is</a> &middot;
  <a href="#what-that-intelligence-covers">What That Intelligence Covers</a> &middot;
  <a href="#what-m1nd-is-not">What m1nd Is Not</a> &middot;
  <a href="#capability-map">Capability Map</a> &middot;
  <a href="#quick-start">Quick Start</a> &middot;
  <a href="#agent-pack-install">Agent Pack</a> &middot;
  <a href="#try-the-agent-demo">Agent Demo</a> &middot;
  <a href="#default-agent-workflow">Default Agent Workflow</a> &middot;
  <a href="#evidence">Evidence</a> &middot;
  <a href="#agent-testimonials">Agent Testimonials</a> &middot;
  <a href="#limits">Limits</a> &middot;
  <a href="#architecture-at-a-glance">Architecture</a> &middot;
  <a href="https://m1nd.world/wiki/">Wiki</a> &middot;
  <a href="EXAMPLES.md">Examples</a> &middot;
  <a href="docs/use-cases.md">Use Cases</a>
</p>

<p align="center">
  <a href="https://claude.ai/download"><img src="https://img.shields.io/badge/Claude_Code-f0ebe3?logo=claude&logoColor=d97706" alt="Claude Code" /></a>
  <a href="https://cursor.sh"><img src="https://img.shields.io/badge/Cursor-000?logo=cursor&logoColor=fff" alt="Cursor" /></a>
  <a href="https://codeium.com/windsurf"><img src="https://img.shields.io/badge/Windsurf-0d1117?logo=windsurf&logoColor=3ec9a7" alt="Windsurf" /></a>
  <a href="https://github.com/features/copilot"><img src="https://img.shields.io/badge/GitHub_Copilot-000?logo=githubcopilot&logoColor=fff" alt="GitHub Copilot" /></a>
  <a href="https://zed.dev"><img src="https://img.shields.io/badge/Zed-084ccf?logo=zedindustries&logoColor=fff" alt="Zed" /></a>
  <a href="https://github.com/cline/cline"><img src="https://img.shields.io/badge/Cline-000?logo=cline&logoColor=fff" alt="Cline" /></a>
  <a href="https://roocode.com"><img src="https://img.shields.io/badge/Roo_Code-6d28d9?logoColor=fff" alt="Roo Code" /></a>
  <a href="https://github.com/continuedev/continue"><img src="https://img.shields.io/badge/Continue-000?logoColor=fff" alt="Continue" /></a>
  <a href="https://opencode.ai"><img src="https://img.shields.io/badge/OpenCode-18181b?logoColor=fff" alt="OpenCode" /></a>
  <a href="https://aws.amazon.com/q/developer"><img src="https://img.shields.io/badge/Amazon_Q-232f3e?logo=amazonaws&logoColor=f90" alt="Amazon Q" /></a>
</p>

<p align="center">
  <img src=".github/m1nd-agent-first-map-v2.jpeg" alt="Traditional agent loop vs m1nd-grounded loop" width="960" />
</p>

> grep finds text. `m1nd` helps agents recover structure, context, and continuity.

## What m1nd Is

`m1nd` is a local MCP runtime that gives coding agents structural retrieval, change reasoning, document grounding, operations, and continuity through a graph they can reason over before, during, and after change.

It ingests repositories, documentation, history, runtime-adjacent signals, and graph-native knowledge into a local graph. That graph is the operational model the agent works against instead of rebuilding context from scratch on every step.

It is not only a query surface. It is an operational layer: answers and edit surfaces can carry proof state, next-step guidance, recovery hints, observable execution, verified writes, stateful navigation, and persisted continuity across sessions.

With `m1nd`, an agent can:

- build a durable operational model of a codebase from code, docs, history, runtime signals, and graph-native knowledge
- retrieve and navigate the right context by text, path, intent, neighborhood, relationship, route, or failure trace
- explain blocked retrieval with compact graph state and a ready diagnostic payload, so agents know whether to re-ingest, adjust scope, or inspect the active runtime
- detect degraded host MCP surfaces, including sessions where m1nd is visible but recovery tools such as `ingest` are not exposed
- run a one-call trust selftest that reports whether the current agent should fully trust, re-ingest, recover, or treat m1nd as orientation-only
- reason about change before, during, and after it happens, including blast radius, co-change, missing work, structural claims, plan validity, drift, and counterfactuals
- analyze architecture, quality, security, duplication, type flow, trust boundaries, hidden dependencies, volatility, and refactor opportunities across the graph
- bind specs and docs back to implementation, including universal documents, graph-native `L1GHT`, provider health, automatic document ingest, and drift detection
- maintain continuity across turns, sessions, baselines, branches, and repo boundaries with perspectives, trails, session coverage, federation, persisted memory, and persisted state
- coordinate many agents against one shared runtime while preserving per-agent navigation state, perspective isolation, and resumable handoff context
- monitor and verify the system over time with audits, graph-vs-disk checks, daemon watches, alerts, metrics, diagrams, panoramic scans, reports, runtime overlays, and persisted state
- prepare, preview, and apply connected edits with graph-aware context, including atomic multi-file writes and post-write verification through `apply_batch`
- learn from feedback and reinforce useful paths over repeated investigations through automatic plasticity and explicit feedback
- measure savings, inspect the live runtime surface, and route itself with built-in reporting and `help`

## What That Intelligence Covers

- Structure: repo shape, dependencies, neighborhoods, hidden relationships, graph-aware retrieval, type flows, architectural layers, and guided routes beyond raw text matches.
- Change: blast radius, co-change prediction, missing work, structural claims, counterfactuals, drift, simulations, proof states, next-step hints, and graph-aware edit preparation, atomic multi-file execution, or post-write verification.
- Docs: universal document ingestion, graph-native `L1GHT`, provider health, automatic ingest, bindings between specs and implementation, local-first document runtime behavior, and document drift detection.
- Operations: audits, graph-vs-disk verification, daemon monitoring, alerts, metrics, diagrams, runtime overlays, panoramas, savings, reporting, built-in help, and recovery-oriented workflow routing.
- Continuity: perspectives, trails, session coverage, boot memory, persisted state, feedback-driven reinforcement, multi-agent isolation, and cross-repo or cross-session investigative state.

## What m1nd Is Not

`m1nd` is not just:

- a code search tool with a larger index
- a repo RAG layer that only retrieves files or chunks
- a graph database that leaves workflow decisions to the client
- a static analysis replacement for the compiler, tests, or security tooling
- an MCP bundle of unrelated utilities

It is the layer that turns those surfaces into an operational system an agent can reason over and act through.

## Capability Map

The live MCP surface evolves with releases. Use `tools/list` for the exact tool count and names in your current build.

| Area | What it enables | Representative tools |
|---|---|---|
| Graph foundation | ingest code, maintain graph state, diagnose session continuity, and reinforce useful paths over time | `trust_selftest`, `session_handshake`, `recovery_playbook`, `ingest`, `health`, `doctor`, `learn`, `warmup`, `resonate` |
| Retrieval and orientation | search by text, path, intent, structure, or relationship before manual file reads | `audit`, `search`, `glob`, `seek`, `activate`, `why`, `trace` |
| Docs and knowledge binding | ingest universal docs or graph-native `L1GHT`, then link concepts back to code | `ingest(adapter="universal"|"light")`, `document_resolve`, `document_provider_health`, `document_bindings`, `document_drift`, `auto_ingest_*` |
| Navigation and continuity | keep stateful routes, handoffs, baselines, and investigation memory across sessions | `perspective_*`, `trail_*`, `coverage_session`, `boot_memory`, `persist` |
| Change planning and proof | reason about impact, co-change, missing steps, failure paths, and structural claims | `impact`, `predict`, `validate_plan`, `missing`, `hypothesize`, `counterfactual`, `differential` |
| Quality, security, and architecture | detect patterns, taint paths, trust boundaries, duplication, layer violations, type flows, simulations, and refactor targets | `scan`, `scan_all`, `heuristics_surface`, `antibody_*`, `taint_trace`, `type_trace`, `trust`, `layers`, `layer_inspect`, `twins`, `fingerprint`, `flow_simulate`, `epidemic`, `tremor`, `refactor_plan` |
| Time, runtime, and multi-repo work | inspect git history, drift, hidden co-change edges, runtime overlays, and cross-repo references | `timeline`, `diverge`, `ghost_edges`, `runtime_overlay`, `external_references`, `federate`, `federate_auto` |
| Operations and monitoring | audit repo state, verify graph-vs-disk truth, run daemon watches, persist state, and surface durable alerts | `audit`, `cross_verify`, `daemon_*`, `alerts_*`, `panoramic`, `metrics`, `report`, `savings`, `persist`, `diagram`, `help` |
| Surgical edit prep and execution | pull compact connected context, preview writes, and apply graph-aware edits | `surgical_context`, `surgical_context_v2`, `view`, `batch_view`, `edit_preview`, `edit_commit`, `apply`, `apply_batch` |

## Quick Start

If you want the shortest path to value:

```bash
git clone https://github.com/maxkle1nz/m1nd.git
cd m1nd
npm install -g .
m1nd doctor
```

Then install the agent doctrine for your host:

```bash
m1nd install-skills codex
m1nd install-skills generic --project /your/project
```

For the native MCP runtime from the same checkout:

```bash
cargo build --release
./target/release/m1nd-mcp
```

Then connect it to your client using the [integration matrix](docs/IDE-INTEGRATIONS.md).

The canonical live tool names are the bare names returned by `tools/list`, such as `ingest`, `activate`, and `audit`.

Then start with this trust loop:

```jsonc
// 0. Trust the binding in one call
{"method":"tools/call","params":{"name":"trust_selftest","arguments":{"agent_id":"dev"}}}

// 0b. If you need the cheaper sub-check only
{"method":"tools/call","params":{"name":"session_handshake","arguments":{"agent_id":"dev"}}}

// If your host only exposes health, read its tool_surface_contract first
{"method":"tools/call","params":{"name":"health","arguments":{"agent_id":"dev"}}}

// 1. If the selftest is not full_trust, ask for the recovery path
{"method":"tools/call","params":{"name":"recovery_playbook","arguments":{"agent_id":"dev"}}}

// 2. Build graph truth
{"method":"tools/call","params":{"name":"ingest","arguments":{"path":"/your/project","agent_id":"dev"}}}

// 3. Get a single-request structural orientation pass
{"method":"tools/call","params":{"name":"audit","arguments":{"agent_id":"dev","path":"/your/project","profile":"auto"}}}

// 4. Ask a structural question
{"method":"tools/call","params":{"name":"activate","arguments":{"query":"authentication flow","agent_id":"dev"}}}
```

Before risky edits, move to `impact`, `predict`, and `validate_plan`, then use `surgical_context_v2` for connected edit prep.

If docs or specs matter too:

```jsonc
{"method":"tools/call","params":{"name":"ingest","arguments":{
  "path":"/your/docs","adapter":"universal","mode":"merge","agent_id":"dev"
}}}
```

For graph-native semantic docs, use `adapter: "light"` instead.

## Agent Pack Install

`m1nd` includes a universal agent pack so the same operating model can be used
from Codex, Claude, Gemini, Antigravity, Cursor, Cline, Roo, Continue, OpenCode,
and other MCP-capable hosts.

Install the beta agent pack:

```bash
npm install -g @maxkle1nz/m1nd@beta
m1nd doctor
m1nd pack-check
```

From a source checkout:

```bash
npm install -g .
m1nd install-skills codex
m1nd install-skills claude --project /your/project
m1nd install-skills gemini --project /your/project
m1nd install-skills antigravity --project /your/project
```

The npm installer currently installs the doctrine, portable host files, config
snippets, and diagnostics. The native runtime is still `m1nd-mcp`; build it
from source or point your host at an installed binary.

```bash
m1nd mcp-config codex
m1nd mcp-config generic
m1nd pack-check
```

For every host that supports environment variables, prefer setting
`M1ND_WORKSPACE_ROOT` to the real repository/workspace. It is the portable
signal used across Codex, Claude Code, Antigravity, Gemini, Cursor, Windsurf,
VS Code, and generic MCP clients.

See [docs/AGENT-PACKS.md](docs/AGENT-PACKS.md) for the full install map.

Windows is part of the universal target. The installer emits Windows-safe MCP
paths and looks for `m1nd-mcp.exe` on `PATH`, through `M1ND_MCP_BINARY`, or at
`%USERPROFILE%\.m1nd\bin\m1nd-mcp.exe`.

The Windows support boundary is the universal MCP lane: `m1nd-core`,
`m1nd-ingest`, and `m1nd-mcp`. The `m1nd-openclaw` fast path remains a Unix
socket lane today.

## Try The Agent Demo

The fastest way to see the agent-first loop is to run the local demo transcript:

```bash
cargo build -p m1nd-mcp
m1nd smoke --repo . --transport stdio
```

It starts the MCP server, checks `trust_selftest`, ingests the repo, runs
retrieval, asks for help, calls `doctor`, and verifies that an empty retrieval
returns a recovery path. The JSON mode is useful for CI or client onboarding:

```bash
m1nd smoke --repo . --transport stdio --json
```

See [docs/AGENT-FIRST-DEMO.md](docs/AGENT-FIRST-DEMO.md) for the transcript
shape and how to read it.

If your local demo sees `trust_selftest` but your editor or agent host does not,
use the [MCP host refresh guide](docs/MCP-HOST-REFRESH.md) to compare the host
tool surface against the local runtime.

If a host returns `Transport closed`, treat it as a dead MCP transport, not a
stale graph. Restart/rebind the host MCP client or open a fresh session, then
run `trust_selftest` or `session_handshake` before relying on retrieval.

## Default Agent Workflow

Make `m1nd` the default investigative layer before `rg`, filesystem globbing, or manual file reads when the task depends on structure, docs, impact, or change.

```text
exact text                -> `search`
path pattern              -> `glob`
purpose or subsystem      -> `seek` or `activate`
unfamiliar repo           -> `audit`
runtime error or trace    -> `trace`
risky change              -> `impact`, `predict`, `validate_plan`, then usually `surgical_context_v2`
docs or specs             -> `ingest` with `universal` or `light`, then `document_*`
long-lived investigation  -> `perspective_*`, `trail_*`, `coverage_session`, `daemon_*`, `alerts_*`, `persist`
unsure what to call       -> `help(stage=..., intent=...)` or `help(error_text="...")`
```

Detailed client-by-client setup lives in the [canonical wiki](https://m1nd.world/wiki/), the local [integration matrix](docs/IDE-INTEGRATIONS.md), and deeper examples in [EXAMPLES.md](EXAMPLES.md).

## Evidence

| Metric | Observed result |
|---|---|
| Live runtime check | Verified locally with `ingest`, `audit(path=...)`, `activate`, and `help` |
| Public MCP surface | Use `tools/list` for the exact live count; the verified runtime behind this README returned bare names such as `ingest`, `activate`, `audit`, and `diagram` |
| `activate` on 1K nodes | **1.36 µs** ([benchmarks](https://m1nd.world/wiki/benchmarks.html)) |
| `impact` depth=3 | **543 ns** ([benchmarks](https://m1nd.world/wiki/benchmarks.html)) |
| Post-write validation sample | **12/12** classified correctly |

## Agent Testimonials

### Jimi - build agent on SAMBA/DOOB

I used `m1nd` on a large multi-agent builder system with generated artifacts, documentation, tools, and repeated handoffs across long sessions.

The biggest difference was continuity.

Without `m1nd`, every session starts by rebuilding context from scratch: searching files, reopening docs, and guessing which parts of the system still matter. `m1nd` changed that. It gave me a structural memory of the project, so I could re-enter through concepts, follow connected neighborhoods, and verify the exact files that mattered.

It did not replace tests, code review, or judgment. It made them easier to reach without losing the thread.

> `m1nd` gave me working memory for a repo that was too alive to navigate by grep alone.

That is why I would want `m1nd` early in any serious agentic build: not after the project is clean and obvious, but exactly when it starts becoming too interconnected for one conversation to hold.

## Limits

`m1nd` complements rather than replaces:

- your LSP
- your compiler
- your test runner
- your security scanners
- your observability stack

It is most useful before search, review, or change, and whenever docs, impact, or continuity matter.

It is less useful when:

- exact text search already answers the question
- compiler or runtime truth is the only thing you need
- the task is a trivial local file action with no structural uncertainty

## Architecture At A Glance

The workspace is split into three core crates plus one auxiliary bridge crate:

- `m1nd-core` — graph engine and reasoning primitives
- `m1nd-ingest` — extraction, routing, and graph construction
- `m1nd-mcp` — MCP server and operational runtime surface
- `m1nd-openclaw` — auxiliary OpenClaw integration surface

Current crate versions:

- `m1nd-core` `0.9.0-beta.0`
- `m1nd-ingest` `0.9.0-beta.0`
- `m1nd-mcp` `0.9.0-beta.0`

<p align="center">
  <img src=".github/m1nd-architecture-overview-v2.jpeg" alt="m1nd architecture overview" width="960" />
</p>

## Learn More

- [Canonical wiki](https://m1nd.world/wiki/)
- [API reference](https://m1nd.world/wiki/api-reference/overview.html)
- [Tool matrix](https://m1nd.world/wiki/tool-matrix.html)
- [Architecture overview](https://m1nd.world/wiki/architecture/overview.html)
- [Examples](EXAMPLES.md)
- [Use Cases](docs/use-cases.md)
- [Deployment & Production Setup](docs/deployment.md)
- [Docs surface guide](docs/README.md)
- [Release notes](https://github.com/maxkle1nz/m1nd/releases)

## Contributing

Contributions are welcome across:

- extractors and adapters
- MCP/runtime tooling
- benchmarks
- docs
- graph algorithms

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT. See [LICENSE](LICENSE).
