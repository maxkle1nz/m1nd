🇬🇧 [English](README.md) | 🇧🇷 [Português](i18n/README.pt-BR.md) | 🇪🇸 [Español](i18n/README.es.md) | 🇮🇹 [Italiano](i18n/README.it.md) | 🇫🇷 [Français](i18n/README.fr.md) | 🇩🇪 [Deutsch](i18n/README.de.md) | 🇨🇳 [中文](i18n/README.zh.md) | 🇯🇵 [日本語](i18n/README.ja.md)

<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="400" />
</p>

<h1 align="center">Operational Intelligence for Coding Agents</h1>

<p align="center">
  <strong>Your coding agent stops starting blind.</strong><br/>
  <em>Local-first. MCP-native. Graph memory, trust, and change reasoning for agent hosts.</em>
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-core"><img src="https://img.shields.io/crates/v/m1nd-core.svg" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://docs.rs/m1nd-core"><img src="https://img.shields.io/docsrs/m1nd-core" alt="docs.rs" /></a>
</p>

<p align="center">
  <a href="https://github.com/openai/codex"><img src="https://img.shields.io/badge/OpenAI_Codex-412991?logo=openai&logoColor=fff" alt="OpenAI Codex" /></a>
  <a href="https://claude.ai/download"><img src="https://img.shields.io/badge/Claude_Code-f0ebe3?logo=claude&logoColor=d97706" alt="Claude Code" /></a>
  <a href="https://cursor.sh"><img src="https://img.shields.io/badge/Cursor-000?logo=cursor&logoColor=fff" alt="Cursor" /></a>
  <a href="https://codeium.com/windsurf"><img src="https://img.shields.io/badge/Windsurf-0d1117?logo=windsurf&logoColor=3ec9a7" alt="Windsurf" /></a>
  <a href="https://github.com/features/copilot"><img src="https://img.shields.io/badge/GitHub_Copilot-000?logo=githubcopilot&logoColor=fff" alt="GitHub Copilot" /></a>
  <a href="https://zed.dev"><img src="https://img.shields.io/badge/Zed-084ccf?logo=zedindustries&logoColor=fff" alt="Zed" /></a>
  <a href="https://github.com/cline/cline"><img src="https://img.shields.io/badge/Cline-000?logo=cline&logoColor=fff" alt="Cline" /></a>
  <a href="https://roocode.com"><img src="https://img.shields.io/badge/Roo_Code-6d28d9?logoColor=fff" alt="Roo Code" /></a>
  <a href="https://github.com/continuedev/continue"><img src="https://img.shields.io/badge/Continue-000?logoColor=fff" alt="Continue" /></a>
  <a href="https://opencode.ai"><img src="https://img.shields.io/badge/OpenCode-18181b?logoColor=fff" alt="OpenCode" /></a>
  <a href="https://aistudio.google.com"><img src="https://img.shields.io/badge/Gemini-4285F4?logo=google&logoColor=fff" alt="Gemini" /></a>
  <a href="https://aws.amazon.com/q/developer"><img src="https://img.shields.io/badge/Amazon_Q-232f3e?logo=amazonaws&logoColor=f90" alt="Amazon Q" /></a>
</p>

---

**m1nd is operational intelligence for coding agents — it governs the operating loop, not just retrieval.**

> grep finds text. Vector search finds similar chunks. `m1nd` gives agents a local graph of what connects, what changed, what breaks, what drifted, and where to resume.

Three things here exist together in no other tool:

- **Causal code graph** — `impact` before you edit shows the blast radius you didn't read; `ghost_edges` surfaces files that always change together but share no import.
- **Self-verifying memory** — `memorize` anchors findings to real code nodes; `cross_verify` flags them stale when that code changes.
- **A trust / recovery layer** — every result carries a trust mode; `trust_selftest` and `recovery_playbook` tell the agent when the workspace binding is wrong and how to recover.

Plus an **attention runtime** — `focus` hands the agent the minimal, budget-bounded working set for a goal, with an honest tail of what it left out and a signal for whether that's *enough* context yet.

<p align="center">
  <img src=".github/m1nd-agent-first-map-v2.jpeg" alt="Traditional agent loop vs m1nd-grounded loop" width="960" />
</p>

## New in 1.2.0 — the first OMEGA-era release

1.2.0 turns the loop from "retrieve, then hope" into **pre-orient → act on calibrated verdicts → capture what you learned**. The theme is the same as the trust layer: an honest *no* beats a confident guess.

- **`north(task)` — pre-orient in one call.** The new front door composes trust, task context (focus nodes + PageRank anchors), prior cross-session memory, a sufficiency signal, one `next_move`, and `honest_gaps` (what m1nd does *not* yet know). `needs_ingest` is a real answer for an empty graph. (The L1GHT-recall composition that folds prior memory into the packet landed on `main` just after the 1.2.0 tag — it is not in the 1.2.0 binary.)
- **Conformal calibration on prediction.** `calibrate_predict` arms a per-repo gate; verdicts then read `act` / `reverify` / `abstain`, where `abstain` means *uncalibrated or insufficient* — a signal to stop, not a weak yes. Ships dark: until you calibrate, verdicts cap at `reverify`.
- **`trust_envelope` on `seek`** (ships dark) and a **`closure` verdict on `why`** — `blocked` means the path rests on an unresolved/guessed edge. **`trust_band: insufficient_evidence`** is now distinct from a risk band: it means *no evidence*, the honest cold-start answer, not "medium risk".
- **Memory grew a provenance spine** — claims carry real age + author, supersede older claims, age out, and respect a recency cap, so remembered knowledge states its own freshness instead of quietly going stale.
- **Smoothed-Jaccard co-change** — `ghost_edges` / `predict` now normalize coupling instead of counting raw co-commits (calibration-proven +3 points over raw counts).
- **Binary version + sha fingerprint** — `--version` prints `1.2.0 (<sha>)`; `M1ND_EXPECTED_VERSION` / `M1ND_EXPECTED_SHA` (+ `M1ND_STRICT_VERSION`) let a host detect and refuse a drifted binary.
- **Agent-native MCP instructions + local-only field reports.** The `initialize` instructions every host receives now *are* the operating loop above. Agents can leave one telemetry signal per session — `learn` on a retrieval verdict, or a line in `~/.m1nd/field-reports.jsonl` when m1nd itself misbehaves. That file is local-only; **m1nd never phones home.**

## Quick Start

The minimal happy path — install from source (always current), check health, wire your host:

```bash
git clone https://github.com/maxkle1nz/m1nd.git && cd m1nd
npm install -g .
m1nd doctor
m1nd install-skills codex          # or: claude / gemini / antigravity / generic
m1nd mcp-config codex --project /your/project
```

Or from npm: `npm install -g @maxkle1nz/m1nd`.

Full install map, host packs, native runtime build, and update flags: [docs/AGENT-PACKS.md](docs/AGENT-PACKS.md) · client-by-client setup: [integration matrix](docs/IDE-INTEGRATIONS.md).

### Agent Entry Point

Agents parse this README. When the host MCP session is stale, bound to the wrong repo, or not loaded yet, use the host-neutral CLI — it launches an isolated runtime, binds it to the repo, and returns one machine-readable envelope:

```bash
m1nd agent first-minute --repo /your/project --query "understand this system" --json
```

`m1nd agent first-minute` is the safest first contact for a new repo. It scopes the repo, establishes trust, ingests if needed, runs one bounded orientation pass, returns candidate anchors, and then tells the agent to prove directly from source, tests, compiler/runtime output, logs, or probes.

Inside an MCP session, the taught front door is one call — `north(task)` composes trust, task context, prior cross-session memory, a sufficiency signal, one `next_move`, and `honest_gaps` (what m1nd does *not* yet know) into a single packet:

```jsonc
{"method":"tools/call","params":{"name":"north",
  "arguments":{"agent_id":"dev","task":"harden the JWT auth token validation flow"}}}
```

The response is one oriented packet — trust verdict, memory the last session left, and an honest gap list, before any query. A real capture from the `main` binary, lightly trimmed:

```jsonc
{
  "binding": { "trust_mode": "full_trust", "ok": true },      // verdict before retrieval
  "memory": [                                                 // recalled from a PRIOR session
    { "claim": "AuthTokenFlow", "source_agent": "authbot", "age_ms": 221, "stale": false }
    // …other claims from the same authored note, trimmed…
  ],
  "sufficiency": { "state": "gathering", "top_score": 0.64,
    "why": "the strongest match left out still scores 0.30 — relevant context did not fit …" },
  "next_move": "Call `surgical_context` on the top focus node to ground the task before editing.",
  "honest_gaps": []                                           // nothing withheld on this graph
}
```

If `north` reports `needs: "needs_ingest"` (empty graph), or you are on an older binary without the L1GHT-recall composition, fall back to the explicit trust loop — establish trust *before* believing any retrieval:

```jsonc
// 0. Trust the binding in one call (verdict before retrieval)
{"method":"tools/call","params":{"name":"trust_selftest","arguments":{"agent_id":"dev"}}}

// 1. If the verdict is not full_trust, ask for the deterministic recovery path
{"method":"tools/call","params":{"name":"recovery_playbook","arguments":{"agent_id":"dev"}}}

// 2. Build graph truth
{"method":"tools/call","params":{"name":"ingest","arguments":{"path":"/your/project","agent_id":"dev"}}}

// 3. Ask a structural question — empty results say *why*, never just "no results"
{"method":"tools/call","params":{"name":"activate","arguments":{"query":"authentication flow","agent_id":"dev"}}}
```

**First-session loop, in four moves:** `north` (or `trust_selftest` → `ingest`) → `seek`/`audit` → `memorize` the durable finding so the next session starts ahead.

### Serve one graph, attach many agents

Quick Start above wires a stdio server per host — fine for one agent, but each process loads its own graph and holds its own lease. The deployment m1nd is built for is one owner, many attached agents. One owner process holds the live graph:

```bash
m1nd-mcp --serve --no-gui --port 1337 --runtime-dir /your/project/.m1nd
```

Every agent then attaches as a thin stdio↔HTTP bridge — it loads **no** graph, builds no engines, and takes **no** lease:

```bash
m1nd-mcp --attach http://127.0.0.1:1337 --stdio    # or set M1ND_ATTACH_URL and omit the flag
```

Any number of bridges point at the one owner and share its single live graph, so what one agent `memorize`s another recalls immediately — no reingest, no per-agent copy. Queries go over localhost, so it stays local-first (bind stays `127.0.0.1` unless you opt into `--bind 0.0.0.0`). Warm `seek` over the bridge measured ≈0.7ms on a small graph on one machine — order-of-magnitude, not a guarantee: attach adds a localhost round-trip, and latency scales with graph size and load.

## What m1nd Is Not

`m1nd` is not just:

- a code search tool with a larger index
- a repo RAG layer that only retrieves files or chunks
- a graph database that leaves workflow decisions to the client
- a static analysis replacement for the compiler, tests, or security tooling
- an MCP bundle of unrelated utilities

It is the layer that turns those surfaces into an operational system an agent can reason over and act through. Not for one-file lookups, simple grep, or compiler truth — use plain tools there.

## Why Agents Need It

Without m1nd, every session starts with grep loops and manual re-orientation; last week's findings are gone, and an empty search result is indistinguishable from a wrong-workspace bind. With m1nd, the session starts with a trust verdict, past findings auto-load already anchored to the code that backs them, and empty results say *why*.

Agents on real codebases do not fail because they cannot search. They fail because they have no operating model. They rebuild context from scratch every session, edit without knowing the blast radius, and cannot tell an empty result that means "nothing exists" from one that means "wrong repo."

That works for small codebases. It falls apart when the project has generated artifacts, specs, docs, hidden co-change history, multiple agents, and long handoffs. The problem is not only the agent's reasoning — the agent has no durable model of the codebase's structure. `m1nd` gives it one: a causal code graph with spreading activation across structural, semantic, temporal, and causal dimensions, plus Hebbian plasticity that compounds per-agent across sessions.

## Compounding Memory (L1GHT)

Most tools give an agent better *retrieval*. `m1nd` also lets an agent **author durable, machine-legible knowledge** that compounds across sessions and stays honest against the code. L1GHT turns authored knowledge into graph-native structure that self-flags when the code it cites changes — confident claims spread more activation than uncertain ones.

The loop, end to end:

1. **Conclude** — the agent reaches something durable (a decision, a verified finding, why code is the way it is) and calls `memorize` with structured claims and `evidence` paths.

```jsonc
memorize({
  "agent_id": "authbot",
  "node_label": "AuthTokenFlow",
  "claims": [
    { "label": "TokenValidator",
      "text": "TokenValidator validates JWTs via HMAC — rotate keys via KMS only",
      "confidence": "high", "evidence": ["src/auth/token.rs"] }
  ]
})
```

The call returns proof it landed — this is a real captured response, trimmed:

```jsonc
{
  "ok": true,
  "claims_written": 1,
  "light_evidence_resolved": 1, "light_evidence_unresolved": 0,   // the evidence path bound to a real code node
  "path": ".../agent-memory/authtokenflow.light.md",
  "next_action": "Memory anchored to code and will auto-load next session; cross_verify(check:[\"evidence_freshness\"]) flags it if the cited code changes."
}
```

2. **Anchor** — m1nd writes a graph-native `.light.md` under `<runtime>/agent-memory/`, ingests it (`adapter=light mode=merge`), and resolves each `evidence` path to the real code node via a `grounded_in` edge — so the knowledge lives in the same activation space as code and surfaces in `seek` / `activate` / `impact`.
3. **Auto-load** — on every future session start, `m1nd` ingests `agent-memory/` automatically and reports it in `session_handshake.agent_memory`. Past findings survive a `mode=replace` ingest and are just *there*.

The compounding is the point: kill that process, start a **fresh** one against the same runtime, and its first `north(task)` already carries the earlier session's claim — this is a real captured exchange (the two calls above ran in separate processes), trimmed:

```jsonc
// north.memory, from a process that never called memorize itself:
"memory": [
  { "claim": "AuthTokenFlow",                   "source_agent": "authbot", "age_ms": 221, "stale": false },
  { "claim": "𝔻 evidence: src/auth/token.rs",   "source_agent": "authbot", "age_ms": 221, "stale": false },
  { "claim": "⍂ entity: TokenValidator",        "source_agent": "authbot", "age_ms": 221, "stale": false },
  { "claim": "𝔻 confidence: high",              "source_agent": "authbot", "age_ms": 221, "stale": false }
  // …the authored-note file node, trimmed…
]
```

`source_agent` names who authored it and `stale` re-checks the cited code — the next session inherits the knowledge *and* its provenance, not a bare string.

4. **Self-flag staleness** — `cross_verify(check: ["evidence_freshness"])` re-hashes every cited file and names which claims have gone stale because their code changed — so memory tells you when it lies instead of misleading you.

This loop has been proven live end-to-end: `memorize` → `grounded_in` edge → freshness flag on an edited file → survives `mode=replace` → boot auto-load. Closing a bounded mission? Pass `write_light_memory: true` to `mission_close` to persist its verified claims the same way. The habit is documented in the server `instructions` every MCP client receives at `initialize` — host-agnostic, no client-specific plugin required.

## The Trust / Honesty Layer

This is the most defensible thing m1nd does, and no competitor ships it. The doctrine: **credibility comes from honesty, not from always winning.**

- **`trust_selftest`** returns a verdict *before* any retrieval: `full_trust`, `needs_ingest`, `wrong_workspace_binding`, `stale_binding_suspected`, or `degraded_host_tool_surface`. The agent knows whether to proceed, ingest, rebind, or fall back.
- **`agent_runtime_contract`** rides on every retrieve response, carrying a `trust_mode`. An empty result is disambiguated — bound to the wrong repo vs. genuinely nothing there — never silently reported as "no results."
- **`non_claims` arrays** ship on every mission tool. m1nd tells the agent what it did *not* prove.
- **`mission_verify` can say no — and does, in tested code.** It rejects graph-only evidence: a claim cannot close without a file read, a test run, or a runtime probe. The test is literally named `graph_only_evidence_is_not_enough`.
- **`recovery_playbook`** returns a deterministic, ordered step list to repair the binding.

Shown, not told. Call `trust_selftest` on an unbound runtime and the verdict *is* the repair instruction — a real capture, trimmed:

```jsonc
{
  "ok": false,
  "status": "blocked",
  "verdict": "needs_ingest",          // not "no results" — it says why
  "next_action": "call_ingest",
  "checks": { "graph_populated": false, "needs_ingest": true, "recovery_playbook_attached": true },
  "recovery_playbook": {
    "recovery_goal": "Populate this binding's active graph for the intended repository.",
    "steps": [ { "action": "Call ingest for the intended repository on this same binding." } /* …trimmed… */ ]
  }
}
```

The same honesty rides on retrieval. A `seek` hit carries a `sufficiency` readout and a `trust_envelope` — and when the envelope has no calibration row measured yet, it caps its own verdict instead of overclaiming. A real capture, trimmed (the top hit is a memory the last session authored):

```jsonc
{
  "results": [
    { "label": "AuthTokenFlow", "source_agent": "authbot", "authored_ms_ago": 101161, "score": 0.48 }
    // …code-node hits, trimmed…
  ],
  "sufficiency": { "state": "gathering", "top_score": 0.48,
    "why": "the strongest match left out still scores 0.25 — relevant context did not fit …" },
  "trust_envelope": {
    "calibrated": false,               // no calibration row measured
    "verdict": "reverify",             // …so the verdict is capped below `act`
    "next_repair_call": "trust_selftest"
  }
}
```

The proof of the commitment is what was killed for it: `savings` and `resonate` were pulled from the advertised surface in beta.7 because a tool that always claims to win is not credible. No competitor — not mem0, Zep, Letta, Sourcegraph, or any code-graph MCP — ships a layer that tells the agent what *not* to trust and how to recover.

**The field-triage loop closes on itself.** The session telemetry agents leave in `~/.m1nd/field-reports.jsonl` (local-only — m1nd never phones home) is not a passive log: reports get triaged, and a *confirmed* field bug becomes a red battery case **before** the fix, so the regression is proven, not just described. That loop has already run once end-to-end: two field-reported bugs turned into failing battery cases and then merged fixes — `north` now composes L1GHT recall into its memory packet, and the `temp` graph sentinel resolves to a real tempdir instead of littering the working directory.

## Language Coverage

Graph reasoning (`impact`, `why`, `predict`, `trace`, `taint_trace`) is only as good as the extractor. m1nd resolves both **`calls` edges** (call graph) and **cross-file `imports`** (file→file dependency resolution) per language. The matrix below was proven live in a single polyglot ingest:

| Language | `calls` | cross-file imports |
|---|:---:|:---:|
| Rust | ✅ | ✅ (`mod`/`use crate::`) |
| Python | ✅ | ✅ |
| JavaScript / TypeScript | ✅ | ✅ |
| Go | ✅ | ✅ (package) |
| Java | ✅ | ✅ (FQCN + wildcard) |
| C / C++ | ✅ | ✅ (`#include "..."`) |
| Kotlin | ✅ | ✅ (package) |
| PHP | ✅ | ✅ (PSR-4) |
| Scala | ✅ | ✅ (package) |
| Ruby | ⏳ | ✅ (`require_relative`) |
| C# | ✅ | — (namespaces don't map 1:1 to files) |
| Swift | ✅ | — |

All ✅ rows are verified end-to-end (a `caller`→`callee` import resolves and the caller emits call edges). Other languages fall back to the generic extractor (`contains` only). Unresolvable imports (external packages, gems, stdlib, system headers) are honestly left unresolved rather than guessed.

## Capability Map

The live MCP surface evolves with releases. Use `tools/list` for the exact tool count and names in your current build.

| Area | What it enables | Representative tools |
|---|---|---|
| Graph foundation | ingest code, maintain graph state, diagnose session continuity, reinforce useful paths, and detect cross-session weight drift | `trust_selftest`, `session_handshake`, `recovery_playbook`, `ingest`, `health`, `doctor`, `learn`, `warmup`, `drift` |
| Retrieval and orientation | search by text, path, intent, structure, or relationship before manual file reads | `audit`, `search`, `glob`, `seek`, `activate`, `why`, `trace` |
| Docs and knowledge binding | ingest universal docs or graph-native `L1GHT`, then link concepts back to code | `ingest(adapter="universal"\|"light")`, `document_resolve`, `document_provider_health`, `document_bindings`, `document_drift`, `auto_ingest_*` |
| Navigation and continuity | keep stateful routes, handoffs, baselines, and investigation memory across sessions | `perspective_*`, `trail_*`, `coverage_session`, `boot_memory`, `persist` |
| Mission control and proof discipline | keep a bounded route, record events, switch from graph orientation to direct proof, hand off, and close with explicit gaps | `mission_start`, `mission_event`, `mission_next`, `mission_verify`, `mission_handoff`, `mission_close` |
| Change planning and proof | reason about impact, co-change, missing steps, failure paths, and structural claims | `impact`, `predict`, `validate_plan`, `missing`, `hypothesize`, `counterfactual`, `differential` |
| Quality, security, and architecture | detect patterns, taint paths, trust boundaries, duplication, layer violations, type flows, and refactor targets | `scan`, `scan_all`, `heuristics_surface`, `antibody_*`, `taint_trace`, `type_trace`, `trust`, `layers`, `layer_inspect`, `twins`, `fingerprint`, `flow_simulate`, `epidemic`, `tremor`, `refactor_plan` |
| Time, runtime, and multi-repo work | inspect git history, drift, hidden co-change edges, runtime overlays, and cross-repo references | `timeline`, `diverge`, `ghost_edges`, `runtime_overlay`, `external_references`, `federate`, `federate_auto` |
| Operations and monitoring | audit repo state, verify graph-vs-disk truth, run daemon watches, persist state, and surface durable alerts | `audit`, `cross_verify`, `daemon_*`, `alerts_*`, `panoramic`, `metrics`, `report`, `persist`, `diagram`, `help` |
| Surgical edit prep and execution | pull compact connected context, preview writes, and apply graph-aware edits | `surgical_context`, `surgical_context_v2`, `view`, `batch_view`, `edit_preview`, `edit_commit`, `apply`, `apply_batch` |

**Tiering:** 27 essential tools are advertised by default to reduce tool-selection cost; set `M1ND_TOOL_TIER=full` to advertise the full surface (100+ tools: RETROBUILDER, perspectives, federation, daemon). A few tools (`resonate`, `savings`, `lock_*`) remain callable by name but are not on the advertised surface. Hidden tools are always callable via `tools/call` — tiering only controls what `tools/list` surfaces.

## The Operating Loops

The agent pack is part of the product, not decorative documentation. m1nd is strongest when the agent receives the *operating loop*, not merely a graph endpoint. Five named protocols ship in the pack:

- **Session Start** — `trust_selftest` → `recovery_playbook` if trust is not full → `ingest` if needed → `seek`/`audit`.
- **Research** — `ingest` → `activate(query)` → `why(source, target)` → `missing(topic)` → `learn(feedback)` → `memorize` any durable finding.
- **Code Change** — `impact(node)` for blast radius → `predict(node)` → `counterfactual(nodes)` → `surgical_context_v2` → `memorize` the decision and why.
- **Deep Analysis** — `fingerprint`, `diverge`, `ghost_edges`, `taint_trace`, `twins`, `refactor_plan`, `runtime_overlay` (the RETROBUILDER lens) for hidden coupling, security paths, structural duplicates, and runtime heat.
- **Memory** — persist durable conclusions with `memorize`, carrying `confidence` and `evidence` paths.

Mission Control is proof discipline, not a feature list. `mission_next` returns exactly one move plus `do_not` guardrails; `mission_verify` rejects graph-only claims; `mission_close` always nudges the agent to persist verified knowledge and records gaps and non-claims. In `bug_hunt` mode, MC0 requires a final direct `direct_sweep` after verified findings before close, so agents check negative space.

**Caveat:** `predict` has **structural fallback only** until `ghost_edges` loads the git co-change matrix — run `ghost_edges` first when you need real co-change likelihood.

## Evidence

Every row is hedged to exactly what was measured. m1nd does not lead with savings or ROI numbers — that is the point.

| Claim | Result | Source / hedge |
|---|---|---|
| `activate` / `impact` latency | ~1µs `activate`, sub-µs `impact` on a 1K-node synthetic graph | Criterion benches — **reproduce it yourself: `cargo bench -p m1nd-core`** (measured `activate_1k_nodes` ≈1.4µs, `impact_depth3` ≈0.5µs on an Apple-silicon Mac); [methodology](https://m1nd.world/wiki/benchmarks.html); order-of-magnitude, hardware-dependent. |
| Language matrix | calls + cross-file imports for 10 languages (+ Ruby cross-file) | Verified end-to-end in a single polyglot ingest; per-language tests in `m1nd-ingest`. See [Language Coverage](#language-coverage). |
| Post-write validation sample | 12/12 classified correctly | Internal runtime check. |
| Seeded bug-hunt | 16/20 in the first accepted `humanize` seeded-defect round (m1nd-trained); `m1nd-basic` and direct each 8/15 | Internal product evidence, `public_claim_worthy=false` — not a universal benchmark. |
| Memory self-verification | proven live end-to-end | `memorize` → `grounded_in` → freshness flag on edited file → survives replace → boot auto-load. |
| Capability battery vs grep | 37/37 pass; head-to-head 16 m1nd-wins / 12 ties / **0 grep-wins** | In-repo harness `scratchpad/m1nd_battery.py` (37 cases, fresh ingest + ground-truth PASS/FAIL + `rg` head-to-head). **Reproduce: `python3 scratchpad/m1nd_battery.py ./target/release/m1nd-mcp . --suite m1nd`.** Hedge: one repo (m1nd itself), self-authored cases; ~5 of the ties are structural tools scored against a literal-grep proxy that can't express what they answer. |
| Conformal calibration (`predict`) | act-band ≈32% precision @ ≈13.5% coverage (α=0.10) | On m1nd's own git history (n≈9.2k held-out predictions), +3pts over raw counts after the smoothed-Jaccard change. Hedge: one repo, a coarse count-based signal — the gate mostly abstains today, **by design**: abstention is the honest output of a weak signal, not a failure. |

## Limits

`m1nd` complements rather than replaces your LSP, compiler, test runner, security scanners, and observability stack. It is most useful before search, review, or change, and whenever docs, impact, or continuity matter.

It is **less useful** when:

- exact text search already answers the question
- compiler or runtime truth is the only thing you need
- the task is a trivial local file action with no structural uncertainty

**Needs feeding:** `trust` and `tremor` start with neutral priors until `learn` feedback / `ghost_edges` data accumulates, and `predict` needs `ghost_edges` loaded first before its co-change signal is meaningful. These improve with use; they are honest about being uninformed at boot.

## Architecture At A Glance

Three core Rust crates plus one auxiliary bridge:

- **`m1nd-mcp`** — the MCP server and operational runtime surface.
- **`m1nd-core`** — the graph engine: a `WavefrontEngine` doing spreading activation, Hebbian plasticity, CSR adjacency, and git-derived ghost edges.
- **`m1nd-ingest`** — extraction, routing, and graph construction adapters (code, universal docs, L1GHT).
- **`m1nd-openclaw`** — auxiliary OpenClaw bridge (Unix-socket lane, independently versioned).

Current crate versions: `m1nd-core`, `m1nd-ingest`, `m1nd-mcp` all `1.2.0` (`m1nd-openclaw` is versioned independently at `0.1.0`).

<p align="center">
  <img src=".github/m1nd-architecture-overview-v2.jpeg" alt="m1nd architecture overview" width="960" />
</p>

For federation, perspectives, RETROBUILDER, multi-agent coordination, and the full agent-pack and operator reference, see the [canonical wiki](https://m1nd.world/wiki/), [docs/AGENT-PACKS.md](docs/AGENT-PACKS.md), and [EXAMPLES.md](EXAMPLES.md).

## Contributing

Contributions are welcome across extractors and adapters, MCP/runtime tooling, benchmarks, docs, and graph algorithms. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT. See [LICENSE](LICENSE).
