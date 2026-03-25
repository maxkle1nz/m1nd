# m1nd Landing Copy v0.6.1

## Positioning

`m1nd` should no longer be presented as a futuristic grep replacement.
The product truth in `v0.6.1` is:

- a local code graph engine for MCP agents
- grounded retrieval plus proof-aware state
- guided next-step handoff
- actionable continuity
- safer edit preparation
- observable multi-file execution
- recovery loops that teach the agent how to continue

## Hero

### Headline

`A local code graph engine for MCP agents.`

### Subhead

`m1nd helps agents trace failures, inspect impact, resume investigations, prepare safer edits, and recover from mistakes with less context churn.`

### Proof line

`It does not just return results. It exposes proof state, next-step guidance, and execution progress.`

### CTA

- `Install m1nd-mcp`
- `Read the docs`
- `See benchmark truth`

## Section: Why It Matters

### Title

`Without structure, agents keep rediscovering the repo.`

### Copy

`Raw text search can find strings, but it does not tell an agent whether it is still triaging, actively proving, or already safe to move into edit prep.`

Three points:

- `Know where you are`
  `proof_state` makes the current cognitive stage explicit.
- `Know what comes next`
  `next_suggested_tool`, `next_suggested_target`, and `next_step_hint` reduce hesitation and retry loops.
- `Keep continuity`
  `trail_resume` restores the investigation with actionable hints instead of making the agent restart from scratch.

## Section: Product Truth

### Title

`What m1nd actually changes`

Cards:

- `Trace and root-cause`
  `trace` ranks suspects and can hand off the next file worth opening.
- `Inspect blast radius`
  `impact` shows affected nodes and whether the seam still needs proof.
- `Prepare connected edits`
  `surgical_context_v2`, `validate_plan`, and `apply_batch` turn risky edits into a guided workflow.
- `Resume work without rediscovery`
  `trail_resume` returns the next focus, open question, and likely next tool.
- `Recover from mistakes`
  Invalid regex, ambiguous scope, stale route, stale trail, and protected-write failures now teach the next valid move.
- `Understand long-running writes`
  `apply_batch` exposes phases, progress, SSE events, and handoff metadata.

## Section: Benchmark Truth

### Title

`Measured on workflow behavior, not just output size`

### Copy

`The current warm-graph corpus shows where m1nd helps most: fewer false starts, lower context churn, guided follow-through, and shorter recovery loops on structural tasks.`

Metrics:

- `12,139 -> 6,428` token proxy in the recorded corpus
- `47.05%` aggregate reduction
- `14 -> 0` false starts
- `39` guided follow-throughs
- `12` successful recovery loops

### Footnote

`Not every scenario is a token win. Some wins are continuity, recovery, or execution clarity.`

## Section: Example Workflow

### Title

`A guided agent workflow`

Flow:

`trace -> view -> surgical_context_v2 -> validate_plan -> apply_batch`

Labels:

- `Find the likely fault`
- `Open the right target`
- `Pull connected edit context`
- `Check whether the plan is safe`
- `Write and verify with live progress`

## Section: When To Use It

### Title

`Use m1nd when the task is structural, connected, stateful, or risky`

Bullets:

- when an agent needs ranked structure instead of raw text hits
- when continuity matters across multiple steps
- when edit prep or blast radius matters
- when long-running writes need to stay observable

## Section: When Plain Tools Are Better

### Title

`Use plain tools for simple textual truth`

Bullets:

- one-file lookups
- simple grep
- compiler and test truth
- logs and direct runtime output

## Tone Guardrails

- do not over-index on `grep killer`
- do not use unverifiable sci-fi claims
- do not lead with percentages unless they are current corpus truth
- do lead with workflow change
- do emphasize local-first and MCP-native operation
