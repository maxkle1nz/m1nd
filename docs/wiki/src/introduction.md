# m1nd: A Checkable Map for Your Coding Agent

> **Current source scope — 2026-10-15:** The source candidate removes transplant and execution proxies; sealed 1.6.5 is unchanged. Manual mission/clipboard delivery, candidate edits, retrieval and impact remain. See [../../FIRST-SIMPLIFICATION-SLICE.md](../../FIRST-SIMPLIFICATION-SLICE.md).

m1nd gives an MCP coding agent a local graph of your repository and memory linked to evidence. It helps answer three practical questions:

- Where is the code relevant to this task?
- What is connected to it before I change it?
- What did an earlier session learn, and should I still trust it?

Start with [Quick Start](tutorials/quickstart.md), then [your first query](tutorials/first-query.md). You do not need the full API or architecture first.

## Who it helps

Use m1nd when structural navigation or repeated context-building is a meaningful part of the work: entering an unfamiliar repository, planning a multi-file change, resuming an investigation, or coordinating agents on one local owner. A file read or text search may be simpler for a small, exact task.

## The working loop

1. **Orient:** `north` combines repository binding, task context, prior memory, a next move, and explicit gaps.
2. **Find and inspect:** `seek` retrieves candidates; `impact` and `why` expose graph relationships. Open the source before relying on a claim.
3. **Verify:** use the compiler, tests, and runtime checks your task requires. Trust bands describe evidence quality; they do not certify correctness.
4. **Remember:** `memorize` stores verified claims with evidence paths so later sessions can recall and check them.

A served local owner can share its graph across attached clients. Presence and overlapping-work notices are advisory; agents must still coordinate edits.

## Read the right version

GitHub/native, crates.io, and npm 1.6.5 are published at `363bc97f2336c73e90802916804dccd2109c5746`; npm `latest` is 1.6.5 as of 2026-10-12. Automatic isolated first-workspace preparation shipped in that release with a matching wrapper/native runtime. This wiki also follows `main`: PR580's later learning-event separation is **Unreleased**, absent from sealed 1.6.5. The [quick start](tutorials/quickstart.md) separates the current installation from the historical 1.6.3 recipe.

The default `tools/list` is a small advertised menu. Use `help` for full discovery and the installed runtime's schemas for exact arguments. A tool's existence does not remove its authority requirements. Historical changelog entries describe past behavior; PRDs are design documents, not current setup instructions.

## Local processing and honest limits

Graph queries do not require an LLM API key. Code and graph state are processed locally; installation, updates, initial model downloads, and optional network document providers may use the network. Your MCP host's data handling is separate.

Extraction is approximate and differs by language. Graph/static checks do not execute your tests. Hardware-backed and human-only authority boundaries remain separate from ordinary retrieval. See the [FAQ](faq.md), [API reference](api-reference/overview.md), and [benchmark evidence guide](benchmarks.md).

## Go deeper

- [Tutorials](tutorials/quickstart.md): first result and common recovery paths
- [Architecture](architecture/overview.md): the five-crate workspace and runtime boundaries
- [Concepts](concepts/spreading-activation.md): activation, learning, and structural signals
- [API reference](api-reference/overview.md): tool families and contracts
- [Changelog](changelog.md): historical changes, not an installation recipe
