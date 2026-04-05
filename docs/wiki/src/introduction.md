# Introduction

m1nd is a local code graph engine for MCP agents. It ingests a repository once, turns it into a queryable graph, and helps agents ask for structure, impact, connected context, continuity, and likely risk instead of reconstructing the repo from raw files every time.

The current public shape of the product is not just “graph search.” It is a guided runtime with:

- graph-grounded retrieval and impact analysis
- `proof_state` on the main structural flows
- `next_suggested_tool`, `next_suggested_target`, and `next_step_hint`
- actionable continuity through `trail_resume`
- observable multi-file writes through `apply_batch`
- recovery loops that teach the next valid move when a tool is used badly

m1nd ships as an [MCP](https://modelcontextprotocol.io/) server, runs locally, and works with any MCP-compatible client over stdio. The current exported schema exposes 78 MCP tools.

## The Problem

Most agent loops still waste time in the same place: navigation.

An LLM can reason about a file once it has the file. The expensive part is getting the right file, the right neighbors, and enough proof to act without reopening half the repo.

Without a structural layer, the loop usually looks like this:

1. grep for a symbol or phrase
2. open a file
3. grep for callers, callees, or related paths
4. open more files
5. repeat until the subsystem shape becomes clear

That cost shows up as:

- more file reads than necessary
- more token burn on repo reconstruction
- weaker stopping rules during triage
- more false starts before editing
- more friction resuming prior investigations

## What m1nd Changes

m1nd keeps the graph local and lets an agent ask for structure directly:

- `trace` maps stacktraces to likely suspects
- `impact` inspects blast radius before edits
- `seek` and `activate` find intent and connected structure
- `validate_plan` and `surgical_context_v2` prepare safer multi-file changes
- `trail_resume` restores investigations with next-focus and next-tool hints
- `apply_batch` exposes progress, phases, and final handoff signals

The result is less context churn and better decision quality per step.

Current benchmark truth from the recorded warm-graph corpus:

- `10518 -> 5182` aggregate token proxy
- `50.73%` aggregate reduction
- `14 -> 0` false starts
- `39` guided follow-throughs
- `12` successful recovery loops

Not every scenario is a token win. Some wins are continuity, recovery, or execution clarity. That is part of the product truth too.

## Core Runtime Ideas

### Graph-grounded retrieval

The graph is still the foundation. Activation, semantic retrieval, path search, temporal history, and blast-radius analysis all sit on top of a shared structural model rather than a stateless grep loop.

### Guided handoff

Several high-value tools now return more than raw results. They can expose:

- `proof_state`
- `next_suggested_tool`
- `next_suggested_target`
- `next_step_hint`

That turns the server from a catalog of answers into a layer that helps the agent decide what to do next.

### Continuity

`trail_resume` is no longer just bookmark restore. It can return compact resume hints, reactivated nodes, the next focus node, the next open question, and the likely next tool. This is one of the main reasons the benchmark corpus now records fewer false starts.

### Observable execution

`apply_batch` is now an observable write surface:

- `status_message`
- `proof_state`
- lifecycle phases such as `validate`, `write`, `reingest`, `verify`, and `done`
- coarse progress fields like `progress_pct`
- structured `progress_events`
- live SSE progress in serve mode

### Recovery loops

Common failures no longer have to be dead ends. Many invalid calls now return hints, examples, and a suggested next step so the agent can repair the call instead of rediscovering the workflow from scratch.

## Who This Is For

- agent builders who want a local structural layer for navigation and edit prep
- MCP client users who want better triage, continuity, and connected context
- multi-agent systems that need shared graph truth without shipping code to an API
- teams that want safer workflow around stacktrace triage, blast radius, and multi-file changes

m1nd is not a compiler, debugger, or test runner replacement. It is best when the real bottleneck is structural understanding and repo navigation.

## How To Read This Wiki

**[Architecture](architecture/overview.md)** explains how the three crates fit together and how the MCP server turns graph truth into agent-facing runtime behavior.

**[Concepts](concepts/spreading-activation.md)** covers the underlying graph ideas such as activation, plasticity, and structural holes.

**[API Reference](api-reference/overview.md)** documents the current MCP surface, including underscore-based canonical tool names, guided outputs, and transport behavior.

**[Tutorials](tutorials/quickstart.md)** walks through the main workflows from first ingest to connected edit prep.

The **[Benchmarks](benchmarks.md)** page is the current product-truth layer for token proxy, false starts, guided follow-through, and recovery loops. The **[Changelog](changelog.md)** tracks the release history from `v0.6.0` and `v0.6.1` onward.
