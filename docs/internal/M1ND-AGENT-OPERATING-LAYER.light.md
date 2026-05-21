---
Protocol: L1GHT/1.0
Node:     M1ND_AGENT_OPERATING_LAYER
State:    strategic_prd
Color:    amber
Glyph:    mission
Completeness: draft
Proof:    docs/internal/M1ND-AGENT-OPERATING-LAYER-PRD.md
Depends on:
- M1ND_MISSION_CONTROL_V0
- M1ND_L1GHT_DOCUMENT_LANE
- M1ND_BENCHMARK_GYM
- M1ND_RECOVERY_OS
Next:
- MISSION_KERNEL_V1_BOUNDARY
---

# m1nd Agent Operating Layer

## North Star

The [⍂ entity: AgentOperatingLayer] turns `m1nd` from graph memory plus tools
into a local nervous system for software agents.

[⍐ state: strategic_prd]
[𝔻 confidence: medium]
[𝔻 evidence: docs/internal/M1ND-AGENT-OPERATING-LAYER-PRD.md]

## Core Thesis

Agents do not only need better retrieval. They need a mission runtime that
knows workspace truth, tool policy, evidence status, handoff continuity, and
recovery state.

[⍂ entity: MissionRuntime]
[⟁ depends_on: ContextGuardV1]
[⟁ depends_on: EvidenceLedger]
[⟁ depends_on: ToolPolicyRouter]
[⟁ depends_on: AgentFlightRecorder]
[⟁ depends_on: BenchmarkGym]

## Current Evidence

The latest internal p-limit bug-hunt sweep showed `m1nd-mission-control` at
10/10 seeded recall, while `direct` and `m1nd-trained` each reached 9/10.

[⍌ event: MC0DirectSweepBenchmarkSignal]
[⟁ binds_to: docs/benchmarks/bug-hunt-rounds/bughunt-p-limit-mc0-sweep-20260517T211556Z/ROUND-NOTES.md]
[AMBER warning: internal product learning, not public benchmark copy]

This PRD session also showed that `trust_selftest` can detect a wrong workspace
before retrieval, while semantic `seek` can still return blocked after ingest.
That makes self-explaining degraded intelligence a product requirement.

[⍌ event: PRDSessionRecoveryObservation]
[⟁ depends_on: recovery_playbook]
[⟁ depends_on: doctor]
[AMBER warning: populated graph plus blocked seek should trigger recovery, not silent shell fallback]

## Primary Modules

### Mission Kernel V1

[⍂ entity: MissionKernelV1]
[⍐ state: implementation_boundary]
[⟁ depends_on: M1ND_MISSION_CONTROL_V0]
[⟁ tests: start_event_next_verify_handoff_close_tests]

Mission Kernel V1 extends the current four-tool mission loop into
start/event/next/claim-or-verify/handoff/close.

### Context Guard V1

[⍂ entity: ContextGuardV1]
[⍐ state: pattern]
[⟁ binds_to: trust_selftest]
[⟁ binds_to: session_handshake]
[⟁ binds_to: recovery_playbook]

Context Guard V1 binds each mission to repo, workspace root, ingest roots,
runtime root, binary version, and graph generation.

### Evidence Ledger

[⍂ entity: EvidenceLedger]
[⍐ state: planned]
[⟁ depends_on: MissionKernelV1]
[⟁ binds_to: W3C_PROV_DONOR]

Evidence Ledger records mission events, direct evidence classes, graph-only
evidence, claim references, non-claims, gaps, and event digests.

### Agent Flight Recorder

[⍂ entity: AgentFlightRecorder]
[⍐ state: planned]
[⟁ depends_on: MissionKernelV1]
[⟁ binds_to: OPEN_TELEMETRY_DONOR]

Agent Flight Recorder writes local JSONL mission traces with phase, tool family,
target, duration, outcome, confidence, graph call count, direct evidence count,
and loop warnings.

### Tool Policy Router

[⍂ entity: ToolPolicyRouter]
[⍐ state: signal]
[⟁ binds_to: mission_next]
[⟁ depends_on: BenchmarkGym]

Tool Policy Router emits allowed tools, preferred next move, do-not guardrails,
stop conditions, fallback, evidence requirement, and budget remaining.

### L1GHT Knowledge Synthesizer

[⍂ entity: L1GHTKnowledgeSynthesizer]
[⍐ state: pattern]
[⟁ binds_to: m1nd-ingest/src/l1ght_adapter.rs]
[⟁ depends_on: M1ND_L1GHT_DOCUMENT_LANE]

L1GHT Knowledge Synthesizer turns strategy docs, PRDs, donor maps, benchmark
lessons, and operating doctrine into graph-native knowledge.

### Benchmark Gym

[⍂ entity: BenchmarkGym]
[⍐ state: signal]
[⟁ binds_to: scripts/benchmark/bug_hunt_round.py]
[⟁ depends_on: MissionKernelV1]

Benchmark Gym measures seeded recall, first-good-finding time, graph call count,
direct evidence count, repeated search count, claim rejection rate, false close
rate, drift catch rate, and handoff resumability.

### Recovery OS

[⍂ entity: RecoveryOS]
[⍐ state: pattern]
[⟁ binds_to: npm/lib/cli.js]
[⟁ binds_to: trust_selftest]
[⟁ binds_to: doctor]

Recovery OS diagnoses stale npm package, stale native runtime, stale host tool
surface, wrong workspace, dead MCP transport, missing recovery tools, and graph
retrieval split-brain.

## Donor Concepts

[⍂ entity: LANGGRAPH_DONOR]
[⟁ binds_to: durable_execution]
[⟁ binds_to: persistence]

[⍂ entity: TEMPORAL_DONOR]
[⟁ binds_to: deterministic_replay]
[⟁ binds_to: workflow_history]

[⍂ entity: OPEN_TELEMETRY_DONOR]
[⟁ binds_to: traces]
[⟁ binds_to: spans]

[⍂ entity: W3C_PROV_DONOR]
[⟁ binds_to: provenance_entities_activities_agents]

[⍂ entity: REACT_DONOR]
[⟁ binds_to: reasoning_acting_loop]

[⍂ entity: REFLEXION_DONOR]
[⟁ binds_to: feedback_memory]

[⍂ entity: VOYAGER_DONOR]
[⟁ binds_to: skill_library]

[⍂ entity: DSPY_DONOR]
[⟁ binds_to: metric_driven_optimization]

## Blockers

[AMBER warning: MissionKernelV1 has an initial implementation boundary, not repeated benchmark proof]
[AMBER warning: EvidenceLedger event digest exists as local hash64, not signed sha256 provenance]
[AMBER warning: semantic seek can be blocked even after populated ingest]
[AMBER warning: benchmark evidence is internal and fixture-limited]
[AMBER warning: host rebind cannot be claimed without fresh host handshake]

## Non Claims

This L1GHT node does not claim the full Agent Operating Layer exists, public
benchmark superiority is proven, semantic retrieval is always reliable, host
rebind can be inferred, event digests are signed cryptographic provenance, or
tests and source reads can be replaced.

[𝔻 ambiguity: strategic direction, not implementation proof]

## Next

Build [⍂ entity: MISSION_KERNEL_V1_BOUNDARY] as a proof-grown construction:
extend mission state with event, handoff, event digest, direct evidence classes,
and Context Guard envelope.

[⟁ tests: cargo_test_mission_handlers]
[⟁ tests: benchmark_smoke_mc0]
