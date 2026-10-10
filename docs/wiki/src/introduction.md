# m1nd: A Checkable Map for Your Coding Agent

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

This wiki follows the `1.6.4` source line. The latest published release at this refresh is `1.6.3`; [releases](https://github.com/maxkle1nz/m1nd/releases) tell you what has shipped. Automatic isolated first-workspace preparation requires the matching newer source until publication. The [quick start](tutorials/quickstart.md) separates these paths.

The default `tools/list` is a small advertised menu. Use `help` for full discovery and the installed runtime's schemas for exact arguments. A tool's existence does not remove its authority requirements. Historical changelog entries describe past behavior; PRDs are design documents, not current setup instructions.

## Local processing and honest limits

Graph queries do not require an LLM API key. Code and graph state are processed locally; installation, updates, initial model downloads, and optional network document providers may use the network. Your MCP host's data handling is separate.

Extraction is approximate and differs by language. Graph/static checks do not execute your tests. Hardware-backed and human-only authority boundaries remain separate from ordinary retrieval. See the [FAQ](faq.md), [API reference](api-reference/overview.md), and [benchmark evidence guide](benchmarks.md).

## Go deeper

- [Tutorials](tutorials/quickstart.md): first result and common recovery paths
- [Architecture](architecture/overview.md): the six-crate workspace and runtime boundaries
- [Concepts](concepts/spreading-activation.md): activation, learning, and structural signals
- [API reference](api-reference/overview.md): tool families and contracts
- [Changelog](changelog.md): historical changes, not an installation recipe
