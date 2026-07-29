<p align="center">
  <img src=".github/m1nd-logo.svg" alt="m1nd" width="340" />
</p>

**m1nd** gives your coding agent a brain per repository: a local code graph served over MCP, memory anchored to the code it cites, and a trust verdict on every answer. "Insufficient evidence" is a real answer here. So is "don't trust this yet, and here is how to repair it".

Nothing leaves your machine. One Rust binary. MIT.

Agents on real codebases fail in a specific way. Each session rebuilds context from scratch. Edits land without knowing the blast radius. An empty search result could mean "nothing exists" or "wrong repo", and the agent cannot tell which. Notes from last week describe code that changed on Tuesday, and nothing warns anyone. m1nd exists to close that loop.

<p align="center">
  <a href="https://www.npmjs.com/package/@maxkle1nz/m1nd"><img src="https://img.shields.io/npm/v/@maxkle1nz/m1nd.svg?color=00f5ff&label=npm" alt="npm" /></a>
  <a href="https://crates.io/crates/m1nd-mcp"><img src="https://img.shields.io/crates/v/m1nd-mcp.svg?label=crates.io" alt="crates.io" /></a>
  <a href="https://github.com/maxkle1nz/m1nd/actions"><img src="https://github.com/maxkle1nz/m1nd/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License" /></a>
  <a href="https://registry.modelcontextprotocol.io/?search=io.github.maxkle1nz/m1nd"><img src="https://img.shields.io/badge/MCP_Registry-official-6d28d9" alt="MCP Registry" /></a>
</p>

<p align="center">Four commands to install: <a href="#sixty-seconds">Sixty seconds</a>. Reasons to close the tab first: <a href="#when-not-to-use-m1nd">When not to use m1nd</a>.</p>

<p align="center">
  <img src="docs/assets/demo.gif" width="760" alt="A real m1nd session: north returns trust, focus and honest gaps; seek answers with a reverify verdict; memorize anchors the finding to code" />
</p>

<p align="center"><em>A real session on this repo's 6,453-node graph (m1nd-mcp 1.4.0): <code>north</code> orients, <code>seek</code> answers wearing a <code>reverify</code> verdict, <code>memorize</code> anchors the finding to code.</em></p>

## What is actually new here

Five things here I have not seen shipped together anywhere else:

**It works even when the model forgets the tool exists.** Most MCP servers only help when the agent remembers to call them. `m1nd hosts apply` installs session hooks (`SessionStart`, `agentSpawn`, `TaskStart`, per host) that inject the orientation packet at spawn, so your agent and every subagent it spawns start oriented before anyone types a word.

**Verdicts at read time.** Every answer carries how much to trust it: `act`, `reverify` or `abstain`, calibrated per repo. Refusals are typed and come with a repair plan. Memory layers usually gate what goes *in*; m1nd also tells the agent what to believe on the way *out*. Details in [Why trust the answers](#why-trust-the-answers).

**Memory that self-flags.** Agent-written memory is anchored to real code nodes. When the cited code changes, the claim marks itself stale instead of quietly lying. Details in [Memory that knows when it is stale](#memory-that-knows-when-it-is-stale).

**A brain per repository.** One graph, its own memory, its own persistence, bound to one repo root. A served owner hosts many brains and routes each session to the right one; a session from a repo it does not host gets a typed refusal instead of wrong answers.

**Documents in the same graph (l1ght).** Ask why a function looks the way it does and get back the function, the RFC it implements and the note that settled the argument, in one answer. The whole lane: [One graph for code and knowledge](#one-graph-for-code-and-knowledge-l1ght).

## What your agent gets

m1nd wraps the agent's whole loop around a graph of your repo that outlives the session:

```mermaid
flowchart LR
    B["<b>BEFORE</b><br/>born oriented<br/>map + memory + trust + honest gaps"]
    D["<b>DURING</b><br/>verdicts worn while working<br/>impact before touching · act / reverify / abstain"]
    A["<b>AFTER</b><br/>memorized with evidence<br/>anchored to real code"]
    C["<b>COMPOUND</b><br/>the next session starts ahead<br/>any host, any agent"]
    B --> D --> A --> C --> B
```

The front door is one call. `north(task)` returns the whole orientation in a single packet, before any retrieval:

```jsonc
{"method":"tools/call","params":{"name":"north",
  "arguments":{"agent_id":"dev","task":"harden the JWT auth token validation flow"}}}
```

```jsonc
{
  "binding": { "trust_mode": "full_trust", "ok": true },      // verdict before retrieval
  "memory": [                                                 // recalled from a PRIOR session
    { "claim": "AuthTokenFlow", "source_agent": "authbot", "age_ms": 221, "stale": false }
  ],
  "sufficiency": { "state": "gathering", "top_score": 0.64 },
  "next_move": "Call `surgical_context` on the top focus node before editing.",
  "honest_gaps": []                                           // nothing withheld on this graph
}
```

While the agent works, `impact` shows the blast radius before an edit lands, `why` explains a connection and admits when the path rests on a guess, and `xray_gate` warns before a change crosses an architecture boundary. When the work is done, `memorize` writes the conclusion down with the evidence that backs it. The next session starts with last session's conclusions already in hand, on any MCP host: Claude Code, Codex, Cursor, Gemini, Zed, 22 hosts in total.

You never run any of these verbs yourself. The agent does. Your surface is a small setup CLI, and then you keep talking to your agent as always.

## Sixty seconds

The npm package is the installer. The native runtime is a separate Rust binary that step 1 fetches as a signed release.

```bash
# 1 · install the native runtime (signed, verified, with rollback)
npx -y @maxkle1nz/m1nd update apply --yes

# 2 · confirm it is visible (prints a JSON verdict; good looks like "status": "ok")
npx -y @maxkle1nz/m1nd doctor

# 3 · wire your host: MCP config + the session hooks that make m1nd ambient
npx -y @maxkle1nz/m1nd hosts apply --host claude --project . --yes

# 4 · first value: the orientation packet for YOUR repo, read-only, no host config touched
npx -y @maxkle1nz/m1nd agent first-minute --repo . --query "map this repo" --json
```

Step 1 verifies the signature with [`cosign`](https://docs.sigstore.dev/cosign/system_config/installation/), so install that first if it is not on your PATH. If you prefer the source registry and accept skipping verification, `cargo install m1nd-mcp` works too. Prefer to see before you write: `hosts plan` prints everything `hosts apply` would touch, and writes nothing. There is no uninstall command yet; `hosts plan` doubles as the list of what to remove by hand.

The hooks from step 3 are what make m1nd ambient: the orientation packet is injected at every session and subagent spawn, and the agent drives itself from there. Installing from an agent instead of a terminal? There is a machine-legible twin of this section in [`llms-install.md`](llms-install.md).

A tampered or truncated release cannot land on your machine, and a bad upgrade is one rollback away: the updater checks the signature against the exact build identity, then the SHA-256 and the size, before it touches anything. If verification fails, it refuses rather than falling back to an unverified path. Details in [docs/AGENT-PACKS.md](docs/AGENT-PACKS.md).

## If I disappear

m1nd is MIT and there is no server to lose. The runtime is one Rust binary already on your disk. The memory it writes is plain markdown under `agent-memory/`, readable and greppable with no m1nd installed at all. The graph is derived from your code and rebuilds from scratch on any machine. If this project stops tomorrow, you keep the files and lose a tool. That is deliberate. It is why memory is markdown and why there is no cloud between your agent and its own knowledge.

## Why trust the answers

This is why I built m1nd. Retrieval layers are good at answering. Almost none of them are good at refusing. m1nd treats the refusal as a first-class result:

```jsonc
// trust_selftest on an unbound runtime. The verdict IS the repair instruction:
{
  "ok": false,
  "verdict": "needs_ingest",          // never a bare "no results"
  "next_action": "call_ingest",
  "recovery_playbook": {
    "steps": [ { "action": "Call ingest for the intended repository on this same binding." } ]
  }
}
```

A `seek` hit carries a sufficiency readout and a trust envelope. When no calibration has been measured yet, the envelope caps its own verdict at `reverify` instead of overclaiming. `predict`'s gate is tuned for coverage (α=0.10); on this repo's history that lands at roughly a third precision in the `act` band, and most of the time it abstains, which is the honest output of a weak signal. `abstain` tells the agent to stop. `insufficient_evidence` means no evidence at all, which is a different thing from medium risk, and the API keeps the two apart.

Two tools, `savings` and `resonate`, were deleted outright in beta (handlers, types and state files, all gone) because they returned a win on every input I gave them, and a tool that never loses has stopped measuring. That is the bar every claim in this file is held to.

I keep looking for another memory layer or code-graph server that ships a read-time trust verdict. I have checked the public docs of Mem0, Zep, Letta, Cognee and Supermemory, as of July 2026, and have not found one. If you know one, open an issue and I will link it here.

## Memory that knows when it is stale

Most memory layers store text and hope. m1nd anchors memory to the graph. When an agent calls `memorize`, each claim's `evidence` path is resolved to the real code node, so the note surfaces whenever the agent touches that code, without anyone remembering it exists:

```jsonc
memorize({
  "agent_id": "authbot",
  "node_label": "AuthTokenFlow",
  "claims": [{
    "label": "TokenValidator",
    "text": "TokenValidator validates JWTs via HMAC. Rotate keys via KMS only.",
    "confidence": "high", "evidence": ["src/auth/token.rs"]
  }]
})
```

Because the memory is anchored, it can be audited against reality. `cross_verify` re-hashes every cited file and names which claims went stale because their code changed. Claims carry age and author, supersede older claims, and age out. This loop is proven live end to end in this repo: memorize, anchor, edit the cited file, watch the claim flag itself, survive a full re-ingest, auto-load on the next boot. Kill the process, start a fresh one, and the first `north` already carries the earlier session's claims with provenance attached.

## One graph for code and knowledge (l1ght)

l1ght is the second lane of the same engine: documents become graph nodes in the same activation space as code, so one query traverses both. It is not a bolted-on RAG folder. There are 7,400 lines of dedicated adapters in this tree: Markdown, HTML, PDF, plain text, RST and JSON, plus scholarly routes for BibTeX, DOI/Crossref, JATS papers, RFCs and patents.

Different people get different products out of the same lane:

- A researcher drops a folder of PDFs and DOIs next to the analysis code and asks which paper contradicts the claim this function implements.
- A student walks a textbook chapter and the exercise code as one graph, and the agent explains each in terms of the other.
- A teacher ingests the course notes once; every student's agent answers from the same grounded corpus instead of improvising.
- An engineer binds RFCs and design docs to the functions that implement them; the spec section sits one hop from the code.
- A vibecoder's pile of chat exports and scattered notes stops being a folder and becomes memory the agent actually consults mid-edit.

Same binary, same MCP verbs, same trust layer. `seek` on a mixed graph returns code and documents in one ranked answer.

## When not to use m1nd

Some honest reasons to close this tab:

- Small repos. Under a few hundred files, grep is already cheap and the graph's edge shrinks toward nothing. Independent measurement of comparable graph tooling on a ~110 file repo put the advantage at about 20 percent. Real, and not worth running a runtime for.
- Fuzzy questions. A symbol graph answers "what connects to what". It does not answer "why does this feel slow". Agentic search is better at open-ended questions.
- Compiler and runtime truth. Your LSP, your tests and your profiler are right and m1nd is guessing. m1nd points; they prove.
- Tiny tasks. One file and twenty lines does not need an ingest. Skip it.
- `predict` mostly abstains today. Calibrated on this repo's own history it reaches roughly a third precision in the `act` band at low coverage. Abstention is the honest output of a weak signal, and right now it is also most of the output.

m1nd complements the compiler, the test runner and your security tooling. It replaces none of them.

## Evidence

Everything in this file ships in the current release and is backed by an artifact in this tree. The documents under `docs/` marked PRD are design intent, and I keep the two labeled apart. If the narrative ever outruns the artifact, that is a bug. File it.

Every row is hedged to exactly what was measured. m1nd does not lead with token savings or ROI, and that is deliberate: those are the least falsifiable numbers in this category.

| Claim | Result | Reproduce / hedge |
|---|---|---|
| Graph latency | ~1.4µs `activate`, ~0.5µs `impact` on a 1K-node synthetic graph | `cargo bench -p m1nd-core` on Apple silicon. Order of magnitude only, hardware dependent. |
| Capability battery vs grep | 37/37 pass; head to head 16 wins, 12 ties, 0 grep wins | `python3 scratchpad/m1nd_battery.py ./target/release/m1nd-mcp . --suite m1nd`. One repo (this one), self-authored cases. |
| Coverage-tuned `predict` | roughly a third precision in the `act` band at low coverage (α=0.10) | Measured on this repo's git history, n≈9.2k held-out predictions. The gate mostly abstains, by design. |
| Memory self-verification | 6-step loop proven live | memorize → anchor → freshness flag on an edited file → survives replace → boot auto-load. |
| Persistence across boots and crashes | the gate drives the real binary over stdio across four clean boots, and across a kill -9 | `m1nd-mcp/tests/persist_runtime_root.rs`. Reverting either boot fix turns it red with a message naming the regression. |

## One graph, many agents

For one agent, the stdio server from [Sixty seconds](#sixty-seconds) is all you need, and the agent may call `ingest` directly on an empty graph. For real work, run one served owner that holds the live graph, and attach every agent to it as a thin bridge:

```bash
m1nd-mcp --serve --no-gui --port 1337 --runtime-dir /your/project/.m1nd
m1nd-mcp --attach auto --stdio     # each agent: no graph load, no lease, shared memory
```

What one agent memorizes, another recalls immediately. The served owner also hosts per-repo brains, renders the web UI, and registers each session as a presence, so two agents about to collide on the same code get warned before either lands. Queries stay on localhost; every non-loopback bind is refused until authenticated transport exists.

One gate to know about: a served owner refuses generic `ingest` for repos it does not already host. Minting a new brain on a served owner is a governed gesture, and it fails closed by design. For a first session on a new repo, use the stdio path or `m1nd agent first-minute`. Attach to the owner once it hosts your repo. Full deployment guide: [docs/deployment.md](docs/deployment.md).

## Language coverage

Dedicated extractors cover more than twenty languages, so a polyglot repo does not come back half-mapped: Python and TypeScript through Elixir, Haskell and Zig, routed by file extension in `m1nd-ingest`. The table below is the stricter claim, proven end to end in a single polyglot ingest: call-graph edges plus cross-file import resolution.

| Language | `calls` | cross-file imports |
|---|:---:|:---:|
| Rust | ✅ | ✅ |
| Python | ✅ | ✅ |
| JavaScript / TypeScript | ✅ | ✅ |
| Go | ✅ | ✅ |
| Java | ✅ | ✅ |
| C / C++ | ✅ | ✅ |
| Kotlin | ✅ | ✅ |
| PHP | ✅ | ✅ |
| Scala | ✅ | ✅ |
| Ruby | ⏳ | ✅ |
| C# | ✅ | namespaces don't map 1:1 to files |
| Swift | ✅ | not yet |

Unresolvable imports (external packages, stdlib, system headers) are left unresolved rather than guessed. Everything else falls back to a generic extractor with `contains` edges only.

## The human is the second reader

Most developer tools are built for a person and then grow an API. m1nd runs the other way: the agent is the user, and the verbs are its verbs.

That choice shapes the design in ways you can check. Refusals are typed and carry a recovery playbook, because the reader acting on them is a machine. An error message that needs human interpretation is a design failure here. The same orientation packet the agent reads as `north` is rendered for you as a short card in the conversation and as the Living Tree in the served web UI (your repo drawn as a navigable tree, memory notes pinned to it): computed once, projected per reader, so the human view can never drift into a second truth.

Humans are welcome. You are just the second reader, and the system is more honest to both readers because of it.

## How this repo is built

Read the commit log with a raised eyebrow, then read this. I'm Max. I build m1nd by directing a system of coding agents, under rules stricter than most human teams I have worked on:

- Every substantial change starts as a spec confronted by an independent oracle model before code is written. The objections are recorded inside the spec files.
- Every fix lands with a test that was demonstrated failing first. A test that has never been red proves nothing.
- The reviewer is never the author. Each agent hand works in an isolated worktree.
- A green gate is a candidate. The landing gesture is mine, and I answer for every line.
- The laws are test names: `letter_cannot_color_the_store`, `gate_zero_cannot_land`, `graph_only_evidence_is_not_enough`.
- The tree holds 2,462 test functions, and the full gate runs green on Linux, macOS and Windows.

The skeptic's question ("no human writes this much this fast") is correct. No human does. A human directing a proof-bound system of agents does. This tree is what came out. m1nd's trust layer was born from that daily practice: I needed my own agents to stop trusting stale answers before I could ship anything at this pace.

## Architecture at a glance

Three core Rust crates plus auxiliaries: `m1nd-mcp` (the MCP server and runtime surface), `m1nd-core` (the graph engine: spreading activation, Hebbian plasticity, CSR adjacency, git-derived ghost edges), `m1nd-ingest` (extractors and adapters for code, documents and memory). Your agent sees 48 tools by default instead of 130+, so it picks the right one more often and pays for a shorter tool list on every request; the full surface is one env var away (`M1ND_TOOL_TIER=full`), and tiering only trims the advertised menu, never availability.

<p align="center">
  <img src=".github/m1nd-architecture-overview-v2.jpeg" alt="m1nd architecture overview" width="880" />
</p>

Depth lives in the [wiki](https://m1nd.world/wiki/), [docs/AGENT-PACKS.md](docs/AGENT-PACKS.md), [EXAMPLES.md](EXAMPLES.md) and [CHANGELOG.md](CHANGELOG.md).

## Translations

🇧🇷 [Português](i18n/README.pt-BR.md) · 🇪🇸 [Español](i18n/README.es.md) · 🇮🇹 [Italiano](i18n/README.it.md) · 🇫🇷 [Français](i18n/README.fr.md) · 🇩🇪 [Deutsch](i18n/README.de.md) · 🇨🇳 [中文](i18n/README.zh.md) · 🇯🇵 [日本語](i18n/README.ja.md)

Translations follow the English text with some lag. When they disagree, English is canonical.

## Contributing

Contributions are welcome across extractors, adapters, MCP tooling, benchmarks, docs and graph algorithms. See [CONTRIBUTING.md](CONTRIBUTING.md). There is a live room on [CodeRooms](https://coderooms.com/github/maxkle1nz/m1nd) if you want to talk first. And if you read this far and want to try it: [four commands](#sixty-seconds).

## License

MIT. See [LICENSE](LICENSE).
