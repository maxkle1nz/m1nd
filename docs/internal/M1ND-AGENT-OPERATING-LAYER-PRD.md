# PRD: m1nd Agent Operating Layer

Status: strategic PRD, not an implemented capability claim
Date: 2026-05-17
Owner lens: Jimi, build agent and m1nd guardian

## Summary

The next frontier for `m1nd` is not another retrieval tool. It is an agent
operating layer: a local-first runtime that gives coding agents mission state,
workspace truth, tool policy, evidence discipline, handoff continuity, recovery
paths, and benchmark-driven self-improvement.

The current `m1nd` already has graph memory, L1GHT document structure, recovery
surfaces, Mission Control v0, host update helpers, and real-world benchmark
artifacts. The gap is synthesis. Agents need one compact operating loop that
knows when to use graph reasoning, when to stop, when to prove directly, when
to recover a host, when not to close, and how to pass a verifiable mission to
the next agent.

North star:

```text
m1nd becomes the local nervous system for software agents.
```

It should not replace the agent, compiler, tests, or human judgment. It should
make agents more consistent, better grounded, easier to recover, and easier to
measure.

## Why Now

Recent internal benchmark rounds produced a clear product lesson:

- `m1nd-trained` is strongest when the agent follows a disciplined loop.
- `m1nd-short-audit` can tie direct file work but sometimes costs more time.
- prompt-heavy temporal doctrine can add overhead if it becomes checklist drag.
- Mission Control v0 improved after adding a bug-hunt `direct_sweep`.
- In the latest p-limit round, `m1nd-mission-control` found 10/10 seeded bugs,
  while `direct` and `m1nd-trained` each found 9/10.

That result is not a public benchmark claim. It is product evidence. The lesson
is that `m1nd` improves when it moves from "agent asks graph questions" to
"runtime governs the mission loop".

Live operational evidence from this PRD session also matters:

- `trust_selftest` correctly detected `wrong_workspace_binding` before graph
  work.
- explicit `ingest` moved the active graph to `/Users/kle1nz/m1nd`.
- semantic `seek` then returned blocked despite a populated graph.
- `recovery_playbook` and `doctor` provided the correct split-brain/stale
  binding diagnostic path.
- literal `search` succeeded and found the relevant Mission Control and L1GHT
  docs.

This is the product thesis in miniature: the system should explain itself when
its own intelligence is partially degraded.

## Donor Map

These donors should guide `m1nd` by concept, not by code copy.

| Donor | What to absorb | What to avoid |
|---|---|---|
| LangGraph durable execution and persistence | checkpointed agent state, interrupts, resumable flows, time-travel style debugging | forcing `m1nd` into a framework-specific graph app model |
| Temporal workflows | deterministic replay, durable workflow history, activity boundaries, retry semantics | pretending MCP host sessions are durable workflows before proven |
| OpenTelemetry | spans, traces, attributes, context propagation, local observability vocabulary | cloud dependency, telemetry by default, vendor lock-in |
| W3C PROV | standard provenance language for entities, activities, agents, derivation, attribution | overly academic proof packets agents cannot use quickly |
| Model Context Protocol roots/resources | explicit workspace roots and host-provided context boundaries | trusting host roots without Context Guard verification |
| SWE-agent Agent-Computer Interface | agent-native command and edit interfaces designed around how agents act | copying a single benchmark interface as universal truth |
| SWE-bench and SWE-bench Verified | realistic software engineering evaluation discipline | treating one benchmark family as full product truth |
| AgentBench | multi-environment agent evaluation mindset | broad scores without operational trace evidence |
| ReAct | interleaving reasoning and acting | unbounded thought/action loops without budget or direct proof |
| Reflexion and Self-Refine | feedback memory, self-critique, iterative improvement | prompt-only reflection that is not grounded in evidence |
| Voyager | executable skill libraries and lifelong accumulation | accumulating skills without gates, decay, or retrieval hygiene |
| Tree of Thoughts and LATS | branching search and deliberative route selection | expensive search on tasks where direct proof is cheaper |
| DSPy | optimizing programs and prompts against metrics | hiding behavior behind opaque prompt optimization |

## Five-Year Bet

The durable opportunity is not "better semantic search". The five-year bet is:

```text
Every serious coding agent host will need a local mission runtime.
```

That runtime must answer:

- Which workspace am I really in?
- Is the graph fresh enough?
- Which route should this task take?
- What is the next best move?
- Which tools should I avoid now?
- Is this claim proven by direct evidence?
- What did another agent already cover?
- What failed, and why?
- Can this mission be resumed tomorrow?
- Did the update/rebind really happen, or was it only described?

`m1nd` is already unusually close to this because it combines graph memory,
L1GHT, recovery diagnostics, Mission Control, host installers, update helpers,
and benchmark harnesses in one local runtime.

## Product Shape

### 1. Mission Kernel v1

Mission Control v0 started with four tools: `mission_start`, `mission_next`,
`mission_verify`, and `mission_close`. The first v1 boundary adds
`mission_event` and `mission_handoff`; the complete mission kernel shape is:

- `mission_start`: create route, budget, trust envelope, starter moves, and
  non-goals.
- `mission_event`: record real actions, not only requested actions.
- `mission_next`: emit one next move plus `do_not` guardrails.
- `mission_claim`: register candidate conclusions.
- `mission_verify`: require direct evidence classes before accepting claims.
- `mission_handoff`: serialize verified claims, open hypotheses, dead paths,
  graph anchors, and next move.
- `mission_close`: emit the final proof packet.

Minimum effort path:

- keep the current v0 files and extend state schema rather than rewriting;
- add `mission_event` and `mission_handoff` first;
- keep `mission_claim` as a thin wrapper over `mission_verify` until the ledger
  needs richer state.

### 2. Context Guard v1

Context Guard already detects wrong workspace binding. v1 should become the
front door for every mission:

- bind each mission to `repo`, `workspace_root`, `ingest_roots`,
  `runtime_root`, `binary_version`, and `graph_generation`;
- classify multi-repo situations as `single_repo`, `federated`, or
  `wrong_workspace_binding`;
- require explicit intent before ingesting a second repo into one graph;
- emit a ready recovery payload for host rebind, same-binding ingest, or
  federation.

Minimum effort path:

- include Context Guard output inside `mission_start`;
- add test fixtures for repo A active, repo B requested, and two-repo
  federation intent.

### 3. Evidence Ledger

The proof packet should become a small provenance ledger:

- every mission event gets an id, timestamp, actor, action type, target, and
  evidence class;
- claims bind to event ids, not prose only;
- direct evidence and graph-only evidence stay distinct;
- final proof packet includes verified claims, rejected claims, gaps, non-claims,
  and a digest over mission events.

Donor fit:

- use W3C PROV vocabulary conceptually: `agent`, `activity`, `entity`,
  `wasDerivedFrom`, `wasAssociatedWith`;
- keep the schema compact enough for agents.

Minimum effort path:

- hash event JSON records in `mission_close`;
- add `evidence_class` enum and require it in `mission_verify`.

### 4. Agent Flight Recorder

Agents need a local black box. Not surveillance, not cloud telemetry. A small
trace stream that answers: what did the agent do, why, with which evidence, and
where did it loop?

Fields:

- `mission_id`
- `step_id`
- `tool_family`
- `target`
- `phase`
- `duration_ms`
- `outcome`
- `confidence_before`
- `confidence_after`
- `direct_evidence_count`
- `graph_call_count`
- `loop_warning_count`

Donor fit:

- OpenTelemetry spans and attributes give a mature vocabulary.
- The default must stay local, opt-in for export.

Minimum effort path:

- emit JSONL beside mission state;
- add optional `otel_export=false` placeholder only after local traces are
  stable.

### 5. Tool Policy Router

The main intelligence upgrade is not giving agents more tools. It is telling
them which tools not to call right now.

The router should produce:

- `allowed_tools`
- `preferred_tool`
- `do_not`
- `stop_condition`
- `fallback`
- `evidence_required`
- `budget_remaining`

Route examples:

- exact file already known: skip graph, read file or run test.
- broad unknown repo: trust, ingest, audit, then direct proof.
- bug hunt after one verified finding: direct sweep before close.
- retrieval blocked with populated graph: recovery, doctor, then literal search
  or direct file truth.
- stale host binary: update/hosts plan, rebind, then trust check.

Minimum effort path:

- encode route policy in Mission Control first, not as a separate tool;
- expose a read-only `route_explain` later if benchmarks show agents need it.

### 6. L1GHT Knowledge Synthesizer

Important PRDs, benchmark conclusions, route policies, donor maps, and product
decisions should be authored as L1GHT when they are meant to become graph
knowledge.

This PRD has a paired L1GHT file:

```text
docs/internal/M1ND-AGENT-OPERATING-LAYER.light.md
```

The L1GHT version should declare:

- core entities: Mission Kernel, Evidence Ledger, Context Guard, Tool Policy
  Router, Agent Flight Recorder, Benchmark Gym, Skill Memory;
- dependencies between them;
- evidence from current benchmark artifacts;
- blockers and warnings;
- next implementation steps.

Minimum effort path:

- create L1GHT for strategy docs before implementing code;
- ingest it with `adapter: "light"` and `mode: "merge"`;
- later add doc-to-code drift checks so stale PRDs cannot silently guide agents.

### 7. Skill Memory and Operating Doctrine

The universal agent pack should evolve from "how to use tools" into "how to
operate under mission control".

It should include:

- short mode router;
- when to use Mission Control;
- when to use short-audit;
- when to use direct proof;
- when to use L1GHT;
- how to record handoff and dead paths;
- how to recover host/session/runtime issues;
- how to dissent from `mission_next` without breaking auditability.

Donor fit:

- Reflexion/Self-Refine: remembered critiques.
- Voyager: reusable skill accumulation.
- DSPy: metric-gated prompt/route optimization.

Minimum effort path:

- keep the default pack compact;
- move full-spec routing into a reference file;
- make benchmark outcomes update the doctrine only through explicit PRs.

### 8. Benchmark Gym

`m1nd` should evaluate itself on real agent work, not only microbenchmarks.

Task families:

- seeded bug hunt;
- real bug audit without answer key;
- code review;
- bounded refactor;
- docs drift;
- host recovery;
- multi-repo orientation;
- handoff/resume;
- release readiness;
- L1GHT doc-to-code binding.

Metrics:

- seeded recall;
- first-good-finding time;
- direct evidence count;
- graph call count;
- repeated search count;
- claim rejection rate;
- false close rate;
- workspace drift catch rate;
- mission handoff resumability;
- host recovery correctness.

Minimum effort path:

- repeat the current MC0 direct-sweep round on at least two more fixtures;
- add first-good-finding time and tool-call counts to the existing event stream;
- add a judge pass for extra findings.

### 9. Recovery OS

The update/host/restart work should become a reliable recovery plane.

It should diagnose:

- stale npm package;
- stale native runtime;
- stale host tool surface;
- wrong workspace;
- dead MCP transport;
- missing recovery tools;
- mixed PATH/config binary selection;
- graph populated but retrieval blocked;
- current binary differs from expected build.

It should not claim:

- host rebind happened without a new handshake;
- graph contents were repaired by update;
- retrieval quality is fixed by binary replacement;
- every host on the machine has been updated.

Minimum effort path:

- keep CLI as mutating repair surface;
- expose MCP read-only `update_check` and `hosts_status` later;
- add a bounded `m1nd restart` recipe for selected host only.

### 10. Multi-Agent Handoff

The first valuable multi-agent feature is not real-time orchestration. It is
handoff that prevents repeated work.

`mission_handoff` should include:

- mission summary;
- verified claims;
- rejected claims;
- open hypotheses;
- dead paths;
- files read;
- tests run;
- graph anchors;
- next required move;
- non-claims;
- staleness criteria.

Minimum effort path:

- serialize handoff from mission state;
- let a second agent call `mission_start(parent_mission_id=...)`;
- benchmark whether a resumed agent avoids repeated searches.

## Contract Sketch

Mission state:

```json
{
  "schema": "m1nd-mission-control-state-v1",
  "mission_id": "msn_...",
  "repo": "/abs/repo",
  "mode": "bug_hunt",
  "route": "short_audit",
  "budget": {
    "tool_calls_max": 12,
    "files_read_max": 8,
    "soft_deadline_ms": 90000
  },
  "context_guard": {
    "workspace_match": true,
    "graph_generation": 3,
    "binary_version": "0.9.0-beta.x"
  },
  "events": [],
  "claims": [],
  "handoffs": [],
  "non_claims": []
}
```

Mission event:

```json
{
  "schema": "m1nd-mission-event-v1",
  "event_id": "evt_...",
  "mission_id": "msn_...",
  "phase": "verify",
  "event_type": "file_read",
  "target": "src/foo.rs",
  "evidence_class": "direct_source",
  "outcome": "hypothesis_supported",
  "duration_ms": 341,
  "agent_confidence": 0.72
}
```

Proof packet:

```json
{
  "schema": "m1nd-mission-proof-packet-v1",
  "mission_id": "msn_...",
  "verified_claims": [],
  "rejected_claims": [],
  "evidence_graph": [],
  "event_digest": "sha256:...",
  "gaps": [],
  "non_claims": []
}
```

## Implementation Waves

### Wave 1: Mission Kernel Completion

Goal: complete the missing operating loop without broad rewrites.

Build:

- add `mission_event`;
- add `mission_handoff`;
- add event digest in `mission_close`;
- include Context Guard envelope in mission state;
- document and test direct-evidence enums.

Gates:

- Rust unit tests for start/event/next/verify/handoff/close.
- Existing Mission Control tests still pass.
- One bug-hunt round confirms no regression in MC0 adherence.

### Wave 2: L1GHT Strategy Graph

Goal: make strategy docs graph-native.

Build:

- keep this PRD paired with `M1ND-AGENT-OPERATING-LAYER.light.md`;
- ingest the L1GHT doc with `adapter="light"`;
- add a tiny smoke that searches for `MissionKernelV1` and resolves edges to
  `EvidenceLedger` and `BenchmarkGym`.

Gates:

- L1GHT ingest succeeds.
- literal search finds expected entities.
- no claim that L1GHT drift detection is complete.

### Wave 3: Flight Recorder

Goal: make agent behavior measurable without relying on transcripts.

Build:

- write mission JSONL event streams;
- add summary counters to proof packets;
- add loop/repeated-search detector.

Gates:

- benchmark reports include first-good-finding time, tool counts, direct
  evidence count, and loop warnings.
- no external telemetry by default.

### Wave 4: Route Policy Calibration

Goal: improve agent outcomes through measured routing.

Build:

- mode-specific route policies for bug hunt, review, refactor, docs drift, and
  release.
- direct-proof and graph-stop policies.
- dissent events when agents ignore policy.

Gates:

- repeated benchmark fixtures show equal or better recall with no major time
  regression.
- policy changes are backed by benchmark notes, not taste alone.

### Wave 5: Recovery OS Integration

Goal: connect mission state to host/runtime recovery.

Build:

- mission-level recovery events;
- read-only MCP update/host status;
- runtime mismatch event classification;
- host rebind proof requirement.

Gates:

- stale runtime test;
- wrong workspace test;
- missing recovery tool test;
- dead transport docs path.

### Wave 6: Multi-Agent Handoff

Goal: prove that another agent can resume without repeating dead paths.

Build:

- `mission_handoff`;
- `mission_resume`;
- parent/child mission links.

Gates:

- handoff benchmark where a new agent resumes and avoids known dead paths.
- no claim of autonomous swarm orchestration.

## Non-Claims

This PRD does not claim:

- Agent Operating Layer is implemented.
- Mission Control v1 exists.
- L1GHT strategy drift detection is complete.
- m1nd beats direct file work on every task.
- semantic retrieval is always reliable.
- host rebind can be proven without a fresh host handshake.
- `m1nd` replaces tests, compilers, runtime probes, or source reads.
- public benchmark claims are ready.
- production unattended auto-update is solved.
- multi-agent orchestration is complete.

## Acceptance Criteria For v1

v1 can be claimed only when:

- Mission Kernel has start/event/next/claim-or-verify/handoff/close.
- Context Guard is embedded in mission state.
- Proof packet includes event digest and direct evidence classes.
- At least three benchmark fixtures show no worse recall than `m1nd-trained`.
- One handoff/resume benchmark proves reduced repeated work.
- Recovery OS tests cover stale runtime, wrong workspace, missing recovery
  tools, and host rebind non-claims.
- L1GHT strategy doc can be ingested and queried as graph structure.

## Sources And Donor References

- [LangGraph durable execution](https://docs.langchain.com/oss/python/langgraph/durable-execution)
  and [LangGraph persistence](https://docs.langchain.com/oss/python/langgraph/persistence):
  durable execution, checkpoints, recovery, interrupts, and time travel.
- [Temporal docs](https://docs.temporal.io/): durable execution, workflow
  history, activities, retries, and recovery after failure.
- [OpenTelemetry traces](https://opentelemetry.io/docs/concepts/signals/traces/)
  and [OpenTelemetry specification overview](https://opentelemetry.io/docs/specs/otel/overview/):
  spans, attributes, context propagation, and local observability vocabulary.
- [W3C PROV-XML](https://www.w3.org/TR/prov-xml/) and the PROV family:
  provenance entities, activities, agents, derivation, attribution.
- [Model Context Protocol roots](https://modelcontextprotocol.io/docs/concepts/roots):
  explicit workspace/context boundaries exposed by clients.
- [ReAct](https://arxiv.org/abs/2210.03629): reasoning and acting loop.
- [Reflexion](https://arxiv.org/abs/2303.11366): verbal reinforcement and
  memory for agents.
- [Self-Refine](https://arxiv.org/abs/2303.17651): iterative feedback and
  refinement.
- [Voyager](https://arxiv.org/abs/2305.16291): skill library and lifelong
  agent learning.
- [SWE-agent](https://arxiv.org/abs/2405.15793): Agent-Computer Interface for
  software engineering agents.
- [SWE-bench Verified](https://www.swebench.com/verified.html) and
  [AgentBench](https://arxiv.org/abs/2308.03688): benchmark discipline for
  agents.
- [DSPy docs](https://dspy.ai/): metric-driven optimization of language model
  programs and agent loops.

## Immediate Next Move

Implement Wave 1 as a small proof-grown construction:

```text
Checkpoint: Agent Operating Layer - Mission Kernel v1 boundary
Contract: mission state/event/handoff/proof packet schemas
Build: extend current mission_handlers.rs
Proof: unit tests plus one benchmark smoke
Non-claim: not autonomous orchestration, not public benchmark claim
```
