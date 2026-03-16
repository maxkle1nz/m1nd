# m1nd Use Cases

> "grep finds what you asked for. m1nd finds what's missing."

This page documents proven use cases across all five audiences m1nd serves. Every pipeline and metric
on this page comes from a live audit session on 2026-03-14 against a real Python/FastAPI codebase:
**10,401 nodes · 11,733 edges · 380 files · ~52K lines**. Not a demo. Not a simulation.

---

## Getting Started for Agents

Before diving into specific use cases, configure your agent to use m1nd as its primary code navigation tool.
This replaces grep, glob, and blind file reads with graph-aware queries that cost zero tokens.

**Add to your agent's system prompt:**

```
You have m1nd available via MCP. Use it BEFORE grep, glob, or file reads:
- m1nd.search(mode="literal") replaces grep — finds exact strings with graph context
- m1nd.activate replaces glob — finds related code by meaning, not filename
- m1nd.surgical_context_v2 replaces Read — returns source + all connected files in one call
- m1nd.impact replaces manual dependency checking — shows blast radius before edits
- m1nd.apply replaces Edit — writes code and auto-updates the graph
- m1nd.help() — call when unsure which tool to use
```

**First session workflow:**

```
m1nd.ingest(path="/your/project")   → build the graph (910ms for 335 files)
m1nd.activate(query="your topic")   → find what's relevant
m1nd.learn(feedback="correct")      → train the graph on useful results
```

After this, the graph knows your codebase. Every subsequent query is faster and more relevant.

For detailed configuration (Claude Code, Cursor, Windsurf, etc.), see the [Getting Started](Getting-Started) guide.

---

## Table of Contents

1. [For AI Agents](#1-for-ai-agents)
2. [For Human Developers](#2-for-human-developers)
3. [For CI/CD Pipelines](#3-for-cicd-pipelines)
4. [For Security Audits](#4-for-security-audits)
5. [For Teams](#5-for-teams)
6. [Cross-Domain Features](#6-cross-domain-features)
7. [Proven Pipelines — Real Session Data](#7-proven-pipelines--real-session-data)
8. [PLUG Integration — Connect External Systems to Any Codebase](#8-plug-integration--connect-external-systems-to-any-codebase)
9. [Comparative Benchmarks](#9-comparative-benchmarks)

---

## 1. For AI Agents

AI agents are m1nd's primary audience. The tools are designed for machine speed, machine precision,
and machine-native interfaces (MCP, JSON, graph topology). No GUIs. No guessing.

### 1.1 System-Wide Bug Hunt

The core problem: you need to audit an entire codebase for reliability issues. grep shows you what's
there. It cannot show you what should be there but isn't.

```
Pipeline: scan → missing → resonate → activate → hypothesize → predict → learn
```

**Real session results:**
- 46 m1nd queries
- 39 bugs found total (28 confirmed fixed + 9 new high-confidence): 3 critical, 11 high, 9 medium, 5 low + 9 high-confidence new findings
- 8 bugs (28.5%) were structurally invisible to grep — they required detecting ABSENCE
- 170+ new tests written, zero regressions
- Total m1nd latency: ~3.1 seconds
- LLM tokens consumed by m1nd: 0

```jsonc
// Step 1: Targeting — identify structural regions to investigate
{ "tool": "scan", "patterns": ["concurrency", "resource_cleanup", "error_handling", "state_mutation"] }
// → Returns node clusters matching each pattern. Time: 7.6ms. LLM tokens: 0.

// Step 2: Find missing guards (structural holes)
{ "tool": "missing", "query": "worker pool session reuse timeout cleanup" }
// → Returns: is_alive TOCTOU hole at worker_pool.py (score 1.12), circuit breaker gap
// → These holes DON'T EXIST in the source. grep can't find them.

// Step 3: Harmonic anomalies
{ "tool": "resonate", "query": "stormender phase timeout cancel", "harmonics": 5 }
// → Returns: 9 "cancel" nodes at amplitude 1.4 — not converging
// → Interpretation: lifecycle.py and control.py both cancel phases → TOCTOU race

// Step 4: Test a specific claim
{ "tool": "hypothesize", "claim": "session_pool leaks on storm cancel", "depth": 8 }
// → Returns: 99% confidence, 25,000+ paths analyzed, 3 supporting evidence groups
// → Read the specific files. Confirm. Fix.

// Step 5: After fixes, validate co-changes
{ "tool": "predict", "node": "session_pool.py" }
// → Returns: QueuedTask, ErrorType likely need corresponding updates
// → Check them. They did.

// Step 6: Train the graph
{ "tool": "learn", "feedback": "correct", "agent_id": "audit-001", "context": "session_pool TOCTOU" }
// → Strengthens the edges that led to this finding. Next audit is faster.
```

**Why this beats grep:**
The TOCTOU race in the circuit breaker was found because `missing()` detected a dict as a structural
hole in a concurrency context — no lock on read-check-modify. There is no text pattern to grep for
when the bug IS the absence of a lock.

---

### 1.2 Pre-Code Grounding (Before Any Edit)

Before touching code, an agent should understand the structural blast radius.

```
Pipeline: impact → validate_plan → warmup → [edit] → predict → ingest (incremental)
```

```jsonc
// What breaks if I touch this?
{ "tool": "impact", "node": "worker_pool.py", "depth": 3 }
// → Returns: 350 affected nodes, 9,551 causal chains. Proceed carefully.

// Is my plan safe?
{
  "tool": "validate_plan",
  "plan": "modify session_pool and worker_pool to fix CancelledError handling",
  "files": ["session_pool.py", "worker_pool.py"]
}
// → Returns: risk=0.70, 347 structural gaps identified
// → Not a blocker — expected for high-coupling modules. A warning to test everything.

// Prime the graph for the focus area
{ "tool": "warmup", "task": "fix async cancellation in session and worker pools", "agent_id": "forge-001" }
// → Pre-activates the subgraph. Subsequent queries return more relevant results faster.

// After editing, check what else needs to change
{ "tool": "predict", "node": "session_pool.py" }
// → Returns co-change predictions. Follow them. Don't skip.
```

---

### 1.3 Build Orchestration

During a parallel build with 16 agents working simultaneously, m1nd is the coordination layer.

```
Per module: warmup → [build] → ingest(incremental) → learn("correct") → predict → warmup(next)
```

**Real session data:**
- 16 Sonnet agents building in parallel
- warmup before each module primed the subgraph for that agent's context
- `ingest(incremental)` re-indexed each completed module in ~138ms
- `predict()` after each fix caught 2 missed co-changes before agents diverged
- Zero merge conflicts in graph state (multi-agent writes are atomic)

```jsonc
// Before agent takes a module
{ "tool": "warmup", "task": "implement asyncio.shield on pool.release", "agent_id": "forge-pool" }

// Agent writes code. Then after:
{ "tool": "ingest", "source": "/path/to/session_pool.py", "incremental": true }
// → Re-indexes only changed nodes. Time: 0.07ms for a single file.

// Feedback so the graph learns from this build
{ "tool": "learn", "feedback": "correct", "agent_id": "forge-pool", "context": "asyncio.shield fix" }

// Check completeness
{ "tool": "predict", "node": "session_pool.py" }
// → Returns: QueuedTask.py should also change — co-change prediction.
```

---

### 1.4 Investigation Persistence — Trail System

An investigation that spans multiple sessions or multiple agents. The trail system saves and
restores exact cognitive context — not just notes, but the actual graph activation state.

```
Pipeline: trail_save → trail_list → trail_resume → trail_merge (multi-agent)
```

```jsonc
// Save investigation state mid-session
{
  "tool": "trail_save",
  "trail_id": "trail_audit_001",
  "agent_id": "auditor-001",
  "notes": "3 bugs found in session_pool. Next: sacred_memory concurrent write."
}
// → Returns: trail_audit_001_af21f9bc saved. Restores to this exact graph activation state.

// Days later, in a new session
{ "tool": "trail_resume", "trail_id": "trail_audit_001_af21f9bc" }
// → Graph reactivates to investigation context. Pick up from exactly where you left off.

// Merge two agents' independent trails
{ "tool": "trail_merge", "trails": ["trail_attestation_001", "trail_whatsapp_001"] }
// → Detects conflicts (same node with different hypotheses). Surfaces them for resolution.
```

---

### 1.5 Architecture Exploration and Context Recovery

When onboarding to an unfamiliar codebase or recovering context after session compaction:

```
Pipeline: drift → activate(domain) → why(a, b) → counterfactual(modules)
```

```jsonc
// What changed since last session?
{ "tool": "drift", "since": "last_session", "agent_id": "jimi" }
// → Returns: which modules gained/lost edges. Where the system "moved" while you were away.

// What's related to this concept?
{ "tool": "activate", "query": "chat escalation deep work", "agent_id": "jimi" }
// → Returns: ranked nodes. Signal spreads across structural, semantic, temporal, and causal dimensions.

// Why does A connect to B?
{ "tool": "why", "from": "chat_handler", "to": "sacred_memory" }
// → Returns: 1 hop via `inspect`. Hidden coupling made visible.

// What's the blast radius if we removed this module?
{ "tool": "counterfactual", "nodes": ["storm_manager", "config", "lifespan"] }
// → Returns: 4,041 nodes affected (41% of entire graph). Do not remove lightly.
```

---

### 1.6 Verified Multi-File Refactoring

When refactoring spans multiple files simultaneously and silent regressions are unacceptable.
`apply_batch` with `verify=true` writes all files atomically and runs a 5-layer verification
pass before returning — catching compile errors, anti-patterns, and co-change gaps in one step.

```
Pipeline: surgical_context_v2 → impact → validate_plan → apply_batch(verify=true) → iterate if BROKEN
```

```jsonc
// Step 1: Understand the full dependency context of the target files
{
  "tool": "surgical_context_v2",
  "file_path": "session_pool.py",
  "include_connected_sources": true,
  "max_connected_files": 8,
  "agent_id": "forge-refactor"
}
// → Returns source + 8 connected files (callers, callees, tests) in one call.
// → No manual file reading. Complete context for the refactor.

// Step 2: Assess blast radius before writing anything
{ "tool": "impact", "node": "session_pool.py", "depth": 3, "agent_id": "forge-refactor" }
// → 350 affected nodes. Know what you're touching before you touch it.

// Step 3: Validate the refactor plan
{
  "tool": "validate_plan",
  "plan": "extract session lifecycle into SessionLifecycle class, update all callers",
  "files": ["session_pool.py", "worker_pool.py", "chat_handler.py"],
  "agent_id": "forge-refactor"
}
// → risk=0.62, 12 structural gaps. Expected. Proceed with verify=true.

// Step 4: Write all files atomically with verification
{
  "tool": "apply_batch",
  "edits": [
    { "file_path": "session_pool.py",  "new_content": "..." },
    { "file_path": "worker_pool.py",   "new_content": "..." },
    { "file_path": "chat_handler.py",  "new_content": "..." }
  ],
  "atomic": true,
  "verify": true,
  "agent_id": "forge-refactor"
}
// → Verification runs 5 layers: syntax, import resolution, antibody scan, co-change prediction, graph coherence.
// → verdict: CLEAN — all 3 files passed. Graph re-indexed in one pass.
// → co_change_warnings: ["queued_task.py"] — also needs updating (non-blocking)

// Step 5: If verdict is BROKEN, read the detail and iterate
// BROKEN means: syntax error (layer 1), unknown import (layer 2), or known bug pattern (layer 3).
// Fix the flagged issue, re-apply the batch. Do not skip — BROKEN is a hard signal.
{ "tool": "apply_batch", "edits": [...], "verify": true, "agent_id": "forge-refactor" }
// → Iterate until verdict: CLEAN.
```

**Why `verify=true` matters:**

Silent regressions — code that writes successfully but breaks structurally — are the hardest
class of bugs to catch. A syntax check catches typos. An antibody match catches reintroduced
race conditions. A co-change warning catches the one file you forgot to update.

The overhead is ~200–400ms per batch. The cost of a silent regression in production is orders
of magnitude higher. Always use `verify=true` for real implementation code.

---

## 2. For Human Developers

Human developers use m1nd through GUI tools (Cursor, Claude Code, Windsurf, etc.) that surface
the graph visually. The mental models map directly to common developer questions.

### 2.1 "Where's the Bug?"

```
Pipeline: trace → activate → perspective_start → perspective_follow → perspective_peek
```

You have a stacktrace. You want the root cause, not just the crash site.

```jsonc
// Feed the stacktrace directly
{
  "tool": "trace",
  "stacktrace": "BrokenPipeError: session_pool.py:_send_persistent line 247\n  connection reset by peer",
  "agent_id": "dev"
}
// → Returns: suspects ranked by suspiciousness score (centrality × depth × recency)
// → session_pool.py:_send_persistent — suspiciousness 0.401, risk=critical
// → WHY: high betweenness centrality, recent weight increase, 3 inbound causal paths
// Real result: found the exact CancelledError interrupting asyncio.shield in finally block. 9.8ms.

// Activate around the suspect
{ "tool": "activate", "query": "session pool persistent connection failure cleanup", "agent_id": "dev" }
// → Graph highlights connected modules — the blast area of this bug.

// Navigate from the crash site structurally (not by file tree)
{ "tool": "perspective_start", "entry": "session_pool.py", "agent_id": "dev" }
{ "tool": "perspective_routes", "agent_id": "dev" }
// → Available structural routes from here. Follow toward the bug's origin.

{ "tool": "perspective_peek", "agent_id": "dev" }
// → Source code at the current perspective focus. Exact context, no file browsing.
```

---

### 2.2 "Is It Safe to Deploy?"

```
Pipeline: epidemic → flow_simulate → trust → validate_plan
```

```jsonc
// Spread infection from recently modified modules
{
  "tool": "epidemic",
  "infected": ["worker_pool", "session_pool", "chat_handler"],
  "auto_calibrate": true
}
// → Returns: lifespan.py (high), config.py (medium), models.py (low)
// → These modules have NOT been checked but are statistically likely to be affected.
// Real result: lifespan.py was indeed impacted — found during subsequent review.

// Simulate concurrent requests through the system
{ "tool": "flow_simulate", "entry": "settings_routes", "particles": 4, "max_depth": 8 }
// → Returns: 51 turbulence points, 11 valves (locks working correctly)
// → Turbulence = node receiving multiple simultaneous flows WITHOUT a lock → race condition
// → After bug fixes, core system showed 0 turbulence. That's the target state.

// Module trust scores (requires learn() history)
{ "tool": "trust", "agent_id": "dev" }
// → Per-module defect density from historical learn() events
// → worker_pool.py (5 historical bugs) = low trust. config.py (0 bugs) = high trust.
```

**The deploy decision table:**

| flow_simulate turbulence | epidemic reach | trust scores | Decision |
|--------------------------|----------------|--------------|----------|
| 0 | < 5% graph | All high | Ship it |
| < 20 | < 15% graph | Mixed | Ship with monitoring |
| > 50 | > 25% graph | Any low | Hold. Fix first. |
| 200+ | > 40% graph | Any low | Do not ship. |

---

### 2.3 "How Does This System Work?"

```
Pipeline: layers → activate(domain) → perspective_start → perspective_follow → why
```

Onboarding to an unfamiliar codebase, or returning after months away.

```jsonc
// Auto-detect architectural layers
{ "tool": "layers", "agent_id": "dev" }
// → L0: ConnectionManager (98 nodes) — entry points
// → L1: API (161 nodes) — routing and handling
// → L2: Core (5,036 nodes) — business logic
// → Plus: 892 utility nodes, 20 layer violations
// A layer violation = path that skips a layer (e.g., L0 directly to L2 without L1 validation)

// Navigate through the architecture
{ "tool": "perspective_start", "entry": "chat_handler", "agent_id": "dev" }
{ "tool": "perspective_suggest", "agent_id": "dev" }
// → AI-recommended next node to visit based on your investigation context

{ "tool": "perspective_branch", "agent_id": "dev" }
// → Fork the current perspective. Explore an alternative path without losing position.
```

---

### 2.4 "What Changed Since Last Release?"

```
Pipeline: drift → timeline → lock_diff → differential
```

```jsonc
// What moved in the graph since last week?
{ "tool": "drift", "since": "2026-03-07", "agent_id": "dev" }
// → Not "what files changed" — "what RELATIONSHIPS changed"

// Churn velocity per module
{ "tool": "timeline", "node": "chat_handler.py", "window": "30d" }
// → Commit frequency, co-change patterns, velocity acceleration
// → High velocity + acceleration = "code tremor" — instability building up

// Lock a region, deploy, diff
{ "tool": "lock_create", "region": "settings_routes", "agent_id": "dev" }
// Deploy to staging
{ "tool": "lock_diff", "agent_id": "dev" }
// → Exactly which structural changes occurred since the lock. Time: 0.08µs.
```

---

### 2.5 Architecture Review

```
Pipeline: fingerprint → counterfactual → layers → resonate
```

```jsonc
// Find duplicate or near-duplicate modules
{ "tool": "fingerprint", "threshold": 0.8, "agent_id": "dev" }
// → Pairs with structural similarity > 80%
// → 0 twins on a clean codebase. 10-20% twins on a typical legacy codebase.

// What if we removed/consolidated these modules?
{ "tool": "counterfactual", "nodes": ["legacy_auth_v1", "legacy_auth_v2"] }
// → N nodes affected, K% of graph. Informs consolidation risk.

// Find harmonic anomalies
{ "tool": "resonate", "query": "daemon lifecycle crash recovery", "harmonics": 5 }
// → 149 antinodes, energy 14.4, cluster around daemon modules
// → A tight daemon cluster vibrating together = they're coupled more than they look.
```

---

## 3. For CI/CD Pipelines

m1nd is a zero-token, local binary. It runs in CI in milliseconds, not minutes. No API calls.
No rate limits. No subscription fees per query.

### 3.1 Pre-Merge Gate

Run on every PR. Block merge if risk thresholds are breached.

```jsonc
// Scan for known bug patterns (antibodies)
{
  "tool": "antibody_scan",
  "scope": ["settings_routes.py", "chat_handler.py"],
  "agent_id": "ci-gate"
}
// → If bugs were FIXED: scan returns 0 matches. Correct — like vaccination.
// → If a NEW file reintroduces a known pattern: match detected. Block merge.

// Validate the plan of changes
{
  "tool": "validate_plan",
  "plan": "PR #447: refactor session pool locking strategy",
  "files": ["session_pool.py", "worker_pool.py"],
  "agent_id": "ci-gate"
}
// → Returns: risk score (0-1), structural gaps
// → risk > 0.8: add mandatory review gate. risk > 0.95: block merge, require architectural review.
```

---

### 3.2 Post-Merge Analysis

```jsonc
// Re-index after merge (incremental — fast)
{ "tool": "ingest", "source": "/repo", "incremental": true, "agent_id": "ci-post" }
// → Time: ~138ms for entire repo.

// Predict co-changes that might have been missed
{ "tool": "predict", "node": "session_pool.py", "agent_id": "ci-post" }
// → Post to PR as comment: "m1nd predicts QueuedTask.py may need review"

// Epidemic spread from changed modules
{
  "tool": "epidemic",
  "infected": ["session_pool.py", "worker_pool.py"],
  "auto_calibrate": true,
  "agent_id": "ci-post"
}
// → Flag high-probability modules for follow-up review in next sprint
```

---

### 3.3 Nightly Health Dashboard

```jsonc
// Code tremor detection
{ "tool": "tremor", "window": "7d", "agent_id": "ci-nightly" }
// → Modules with accelerating change velocity → investigate before they break

// Module trust heatmap
{ "tool": "trust", "agent_id": "ci-nightly" }
// → Per-module defect density. Post to dashboard: which modules have the worst reliability record.

// Architectural layer health
{ "tool": "layers", "agent_id": "ci-nightly" }
// → Violation count (L2 importing L1, etc.). Rising count = architecture degrading.

// Full-graph flow health
{ "tool": "flow_simulate", "entry": "main_entrypoints", "particles": 8, "max_depth": 10 }
// → Total turbulence score. Target: 0 in core. Alert if turbulence rises > 20% week-over-week.
```

---

### 3.4 Incident Response

Production is down. You have a stacktrace.

```jsonc
{ "tool": "trace", "stacktrace": "full stacktrace text here", "agent_id": "incident" }
// → Suspects ranked by suspiciousness in 9.8ms.
// Real result: BrokenPipeError → session_pool.py:_send_persistent suspiciousness 0.401, risk=critical
// → Correct root cause. Found in under 10ms.

{ "tool": "hypothesize", "claim": "session_pool is leaking sessions on cancellation", "depth": 8 }
// → 99% confidence, 25K paths, 3 evidence groups
// → Read the top 3. They contain the bug.
```

---

## 4. For Security Audits

m1nd found a SECURITY CRITICAL bug at 99% confidence with 20 supporting evidence paths.
Zero code reading required to find it.

### 4.1 Auth Boundary Analysis

```
Pipeline: missing → flow_simulate → layers → validate_plan
```

```jsonc
// Find structural holes in the auth/validation layer
{ "tool": "missing", "query": "principal import attestation validation signature verification" }
// → Returns: agent_identity.py (score 0.875) — structural hole in attestation validation
// → This module SHOULD connect to a signature verifier. It doesn't.
// → This hole IS the bug. No code reading required to find it.

// Simulate flows that skip validation
{ "tool": "flow_simulate", "entry": "agent_identity", "particles": 4, "max_depth": 8 }
// → Turbulence at nodes that receive flow without passing through a validator → auth bypass path

// Check layer violations
{ "tool": "layers", "agent_id": "security-audit" }
// → Layer violations at L0→L2 (entry → core, skipping L1 validation) = auth bypass risk
```

---

### 4.2 Secret Exposure Detection

```jsonc
// Create antibody patterns for known secret exposure shapes
{
  "tool": "antibody_create",
  "name": "api_key_in_log",
  "pattern": "api_key_handler → logger → output_stream",
  "severity": "critical"
}
// Scan new code on every commit:
{ "tool": "antibody_scan", "scope": "changed_files", "agent_id": "ci-security" }
```

---

### 4.3 Input Validation Gap Detection

```
Pipeline: flow_simulate (surface → core) → layers (violations) → missing (validators)
```

```jsonc
// Simulate input flow from HTTP entry to core data store
{ "tool": "flow_simulate", "entry": "http_routes", "particles": 10, "max_depth": 15 }
// → Turbulence at data store nodes WITHOUT passing through a validation node
//   = unvalidated input reaching core storage
// Real result: WhatsApp subsystem had 223 turbulence points — highest in the system.
// This means WhatsApp routes have more unguarded flow paths than any other subsystem.
```

---

### 4.4 Identity Forgery Detection

This is the most powerful case. m1nd found a SECURITY CRITICAL bug in under 60 seconds.

```jsonc
// Test the hypothesis
{
  "tool": "hypothesize",
  "claim": "agent_identity principal import can accept manifest with forged attestation signature",
  "depth": 10
}
// → 99% confidence, 20 supporting evidence paths (highest evidence count of any claim tested)
// → Not a guess. 20 independent structural paths all support this claim.

// Understand the coupling
{ "tool": "why", "from": "agent_identity", "to": "sacred_memory" }
// → 1 hop via `inspect` — tight coupling. If identity is forged, sacred_memory is compromised.

// Quantify blast radius
{ "tool": "counterfactual", "nodes": ["agent_identity"] }
// → 3,685 nodes affected = 35% of entire graph. This is a Tier 1 critical module.
```

**Total time to find this bug: under 60 seconds. Zero code reading.**

---

## 5. For Teams

### 5.1 Region Locking for Parallel Work

When multiple agents or developers work on the same codebase simultaneously:

```jsonc
// Agent A locks the settings subsystem
{ "tool": "lock_create", "region": "settings_routes", "agent_id": "forge-settings", "strategy": "on_ingest" }
// → Locks 1,639 nodes. Other agents can READ but won't overwrite this region.

// Agent B works on auth (different region — no conflict)
{ "tool": "lock_create", "region": "agent_identity", "agent_id": "forge-auth" }

// Agent A finishes, checks what changed
{ "tool": "lock_diff", "agent_id": "forge-settings" }
// → Structural diff since lock was created. Time: 0.08µs.

{ "tool": "lock_release", "agent_id": "forge-settings" }
```

---

### 5.2 Investigation Handoff

You investigated a bug for 4 hours. You need to hand it to another agent or continue tomorrow.

```jsonc
// Save with full context
{
  "tool": "trail_save",
  "trail_id": "trail_session_pool_audit",
  "agent_id": "auditor-001",
  "notes": "Found 3 bugs in session_pool. Next: sacred_memory. Check shard read after concurrent write."
}
// → Saves exact graph activation state, not just notes.

// Next agent resumes from exact cognitive position
{ "tool": "trail_resume", "trail_id": "trail_session_pool_audit_af21f9bc", "agent_id": "auditor-002" }
// → No re-reading files. No re-running queries. Continue immediately.
```

---

### 5.3 Code Review Enrichment

Before approving a PR, use m1nd to understand structural impact beyond the diff.

```jsonc
// Structural similarity check — is this a duplicate of existing code?
{ "tool": "fingerprint", "nodes": ["new_auth_handler"], "threshold": 0.75 }
// → 0 twins = novel code. Score 0.9+ = near-duplicate of existing module.

// Trust score of touched modules
{ "tool": "trust", "agent_id": "reviewer" }
// → worker_pool.py: 5 historical defects → LOW trust. This PR touches it? Double-review.

// Layer coherence
{ "tool": "layers", "agent_id": "reviewer" }
// → Did this PR introduce layer violations?
```

---

### 5.4 Onboarding Acceleration

New team member (human or agent) needs to understand the codebase:

```jsonc
// Start with domain concepts, not files
{ "tool": "activate", "query": "chat escalation deep work hot lane", "agent_id": "new-dev" }
// → Returns: ranked modules connected to this concept. Start there.

// Get the architectural picture
{ "tool": "layers", "agent_id": "new-dev" }
// → 3 layers auto-detected. Architecture without reading a diagram.

// Navigate from a concept you understand
{ "tool": "perspective_start", "entry": "chat_handler", "agent_id": "new-dev" }
{ "tool": "perspective_suggest", "agent_id": "new-dev" }
// → AI recommendation for next node to visit, based on your current context.

// Ask specific structural questions
{ "tool": "why", "from": "chat_handler", "to": "sacred_memory" }
// → "How does chat connect to memory?" — answered structurally in milliseconds.
```

---

## 6. Cross-Domain Features

These six capabilities were added in v0.2 and are proven on production data. Each addresses a class
of problem that traditional static analysis cannot reach.

---

### 6.1 Immune Memory — Antibody Scanning

**Problem:** Your team fixed a race condition last month. The same structural pattern re-appears in
a new module three months later, written by someone who wasn't in the original code review.

**Solution:** Antibodies are structural fingerprints extracted from confirmed bugs. Once captured,
they scan every future ingest automatically — like vaccination.

```
Pipeline: learn(correct) → antibody_create → [future PR] → antibody_scan → block or flag
```

```jsonc
// After confirming a bug, create an antibody from it
{
  "tool": "antibody_create",
  "name": "dict_race_without_lock",
  "pattern": "dict_mutation → concurrent_access (no lock edge)",
  "severity": "high"
}

// On every PR, scan changed files
{
  "tool": "antibody_scan",
  "scope": ["changed_files"],
  "agent_id": "ci-gate"
}
// → match: 0 = clean (vaccinated codebase, no regression)
// → match: 1+ = known bug pattern detected. Block merge. Link to original fix.
```

**Real result:** The same circuit breaker corruption pattern (dict read-check-modify without a lock)
appeared in 2 different modules. The antibody caught the second occurrence before it reached production.

Known bug shapes recur in 60-80% of codebases. Antibody scanning turns past pain into permanent protection.

---

### 6.2 Race Condition Detection — Flow Simulation

**Problem:** You need to assess whether a subsystem is safe to add concurrency features to. The code
looks correct. But "looks correct" is not a sufficient answer for concurrent systems.

**Solution:** `flow_simulate` runs agent-based particle simulation. Particles represent concurrent
requests. Nodes that receive multiple particles without passing through a lock = turbulence = race
condition sites.

```
Pipeline: flow_simulate(entry) → inspect turbulence nodes → validate_plan → ship/hold
```

```jsonc
{ "tool": "flow_simulate", "entry": "whatsapp_chat_bridge", "particles": 4, "max_depth": 10 }
// → 223 turbulence points detected
// vs settings: 51 turbulence points
// vs core (after fixes): 0 turbulence points
```

**Real result:** WhatsApp subsystem identified as 4x higher risk than any other subsystem.
Feature hold decision made in under 3 minutes without reading a single file.
The `layers` violation report (score 0.0 separation + 13,618 violations) confirmed the finding.

**Turbulence decision table:**

| Turbulence count | Decision |
|-----------------|----------|
| 0 | Safe for concurrency |
| 1-20 | Targeted review of turbulent nodes |
| 21-100 | Hold new concurrency features; fix first |
| 100+ | Do not ship; architectural hardening required |

---

### 6.3 Bug Spread Prediction — Epidemic Model

**Problem:** You fixed 3 bugs in `session_pool` and `worker_pool`. Which other modules are likely
to be affected — whether you've looked at them yet or not?

**Solution:** `epidemic` runs a SIR (Susceptible-Infected-Recovered) bug propagation model. It
spreads infection from known-buggy modules through the graph, estimating reach and R₀.

```
Pipeline: [fix bugs] → epidemic(infected_modules) → flag high-probability modules for review
```

```jsonc
{
  "tool": "epidemic",
  "infected": ["worker_pool", "session_pool", "chat_handler"],
  "auto_calibrate": true,
  "agent_id": "ci-post"
}
// → lifespan.py: high probability
// → config.py: medium probability
// → models.py: low probability
// → R₀: 1.4 (epidemic spreading — take action)
```

**Real result:** `lifespan.py` was subsequently found to be impacted by the session pool bug.
The epidemic model predicted it before anyone read the file.

Use epidemic output to prioritize the next review sprint. Modules with high epidemic score but
not yet reviewed = highest unaddressed risk in the system.

---

### 6.4 Change Impact Monitoring — Tremor Detection

**Problem:** A module is changing constantly. Is that normal, or is instability building up?

**Solution:** `tremor` computes second-derivative acceleration on edge weight time series.
High acceleration = change velocity is increasing = structural instability building.

```
Pipeline: tremor(window) → flag accelerating modules → investigate before they break
```

```jsonc
{ "tool": "tremor", "window": "7d", "agent_id": "ci-nightly" }
// → chat_handler.py: velocity +0.4, acceleration +0.18 — "code tremor" detected
// → session_pool.py: velocity +0.2, acceleration +0.05 — stable
```

High tremor does not mean the module is broken. It means change is accelerating there — it is the
highest-probability location for the NEXT bug. Investigate proactively.

Post tremor results to a dashboard. Rising acceleration across multiple modules = upcoming regression
cluster.

---

### 6.5 Module Trust Scoring — Actuarial Risk

**Problem:** PR review time is finite. Some modules deserve more scrutiny than others based on their
historical defect record.

**Solution:** `trust` computes per-module defect density from `learn()` history. Every time you
confirm a bug in a module, its defect count increments. The trust score is a Bayesian estimate of
the probability that a future change to this module introduces a bug.

```
Pipeline: [learn(correct) for bugs] → trust → use scores in PR review routing
```

```jsonc
{ "tool": "trust", "agent_id": "reviewer" }
// → worker_pool.py: 5 historical bugs → trust_score: 0.21 (LOW — requires senior review)
// → session_pool.py: 3 historical bugs → trust_score: 0.44 (MEDIUM)
// → config.py: 0 historical bugs → trust_score: 0.97 (HIGH — lightweight review OK)
```

Use trust scores to route PRs: low-trust modules get mandatory senior review; high-trust modules
get async merge. This makes review time allocation data-driven rather than intuitive.

---

### 6.6 Architecture Validation — Layer Detection

**Problem:** Your system was designed with clean layers (HTTP → service → domain → data). Over time,
shortcuts accumulate. L0 starts calling L2 directly, bypassing validation. You don't know how bad
it's gotten.

**Solution:** `layers` uses Tarjan SCC + BFS depth to auto-detect architectural layers without
any configuration. Then it counts violations — paths that skip a layer.

```
Pipeline: layers → inspect violations → validate_plan(PRs that touch layer boundaries)
```

```jsonc
{ "tool": "layers", "agent_id": "architect" }
// Real result from production audit:
// → L0: ConnectionManager (98 nodes) — entry points
// → L1: API (161 nodes) — routing and validation
// → L2: Core (5,036 nodes) — business logic
// → Violations: 13,618 (paths that skip L1, going L0 directly to L2)
// → Layer separation score: 0.0 (zero separation — layers not being respected)
```

**Real result:** Found `score 0.0` (zero layer separation) + 13,618 violations in a production
codebase. This was the architectural evidence for why the WhatsApp subsystem had 223 turbulence
points — it was bypassing the validation layer entirely.

Use `layer_inspect` to drill into specific violation paths:

```jsonc
{ "tool": "layer_inspect", "from_layer": 0, "to_layer": 2, "limit": 20, "agent_id": "architect" }
// → Returns the 20 most frequent L0→L2 bypass paths. Fix the worst ones first.
```

---

## 7. Proven Pipelines — Real Session Data

These four pipelines were executed live on 2026-03-14 against a real Python/FastAPI codebase.
Results are empirical.

### Pipeline 1: System-Wide Bug Hunt

**Codebase:** Python/FastAPI, ~52K lines, 380 files
**m1nd time:** ~3.1 seconds total across 46 queries
**Result:** 28 confirmed bugs, 8 of which grep could never find

```
Round 2 (11 queries, ~0.6s, 7 bugs):
  scan × 4 → targeting
  missing("stormender lifecycle") → TOCTOU race
  missing("worker pool session reuse timeout cleanup") → is_alive TOCTOU, circuit breaker
  resonate("worker_pool timeout") → session_routes amplitude anomaly
  resonate("stormender phase timeout cancel") → 9 cancel nodes, amplitude 1.4 → TOCTOU race
  activate("lifespan shutdown") → double-stop + anyma coordination
  activate("ws_relay websocket") → concurrent modification
  activate("whatsapp state") → reconnect hang

Round 6 (15 queries, ~1.1s, 4 bugs):
  hypothesize × 4 → session_pool leak (99%), sacred_memory race (69%),
                     whatsapp circular (1% = correctly negated)
  fingerprint → 0 twins (clean)
  counterfactual → 3 modules = 41% of graph
  trace × 1 → BrokenPipeError → root cause in 9.8ms
```

**Totals:** 46 queries, 3.1s, 39 bugs (28 confirmed fixed + 9 new high-confidence), 8 grep-invisible

---

### Pipeline 2: Settings Subsystem Audit

**Goal:** Find race conditions and validation gaps before shipping a new feature
**Time:** Under 2 minutes

```
hypothesize("settings can save invalid provider config")     → 96% confidence
hypothesize("concurrent PUT to settings can overwrite")      → 88% confidence
flow_simulate(settings_routes, particles=4, max_depth=8)     → 51 turbulence points
missing("settings validation recovery before persist")       → 3 structural holes
```

4 bugs found in under 2 minutes. Zero code reading to find them.

---

### Pipeline 3: Security Attestation Investigation

**Goal:** Verify whether identity forgery is possible
**Time:** Under 5 minutes

```
hypothesize("agent_identity can accept forged attestation") → 99% confidence, 20 evidence paths
why(agent_identity, sacred_memory)                          → 1 hop. Tight coupling.
missing("attestation signature cryptographic verification") → agent_identity IS the hole
counterfactual(["agent_identity"])                          → 3,685 nodes = 35% of graph
```

SECURITY CRITICAL bug found and fully scoped in under 5 minutes.

---

### Pipeline 4: WhatsApp Race Condition Detection

**Goal:** Assess race condition severity before adding concurrency features
**Time:** Under 3 minutes

```
hypothesize("webhook dedup missing")           → 99%, 13 evidence paths
hypothesize("chat dual escalation unguarded")  → 99%, 19 evidence paths
flow_simulate(whatsapp_chat_bridge)            → 223 TURBULENCE POINTS
                                                  (vs settings=51, vs core=0 after fixes)
```

WhatsApp correctly identified as 4x higher risk than any other subsystem.
Feature hold decision made in 3 minutes without reading a single file.

---

## 8. PLUG Integration — Connect External Systems to Any Codebase

You have an external system (plugin, webhook, CLI tool, proxy) that needs to integrate with a
codebase you've never read. The traditional approach: days of reading source code to find the
right hooks. With m1nd: 30 minutes, zero manual reading.

**Proven on OpenCode** (Go, 140 files): 1,888 nodes ingested in 1 second. 15 entry points found.
10 hook points identified. 23 risks surfaced. 70% of integration required zero code changes.

```
Workflow: ingest → layers → activate(entry/hook/plugin) → impact → surgical_context_v2 → hypothesize → apply_batch
```

```jsonc
// Step 1: Graph the target codebase
{ "tool": "ingest", "path": "/path/to/opencode", "agent_id": "plug-agent" }
// → 140 Go files, 1,888 nodes, ingested in 1 second. You now understand its structure.

// Step 2: Understand the architecture before touching anything
{ "tool": "layers", "agent_id": "plug-agent" }
// → 3 layers auto-detected: L0 CLI entry (8 nodes), L1 command routing (34 nodes), L2 core (1,846 nodes)
// → 2 layer violations flagged (L0 → L2 shortcuts)

// Step 3: Find where external systems can attach
{ "tool": "activate", "query": "entry point hook plugin extension middleware", "agent_id": "plug-agent", "top_k": 15 }
// → 15 integration candidates returned, ranked by centrality and structural openness
// → Identifies plugin registration, event hook, and middleware injection points

// Step 4: Before touching anything, assess the risk
{ "tool": "impact", "node": "cmd/root.go", "depth": 3, "agent_id": "plug-agent" }
// → Blast radius: 847 nodes affected. High centrality. Modify with care.
// → Immediately narrows integration target to lower-risk hook points.

// Step 5: Get full dependency context for the integration point
{
  "tool": "surgical_context_v2",
  "file_path": "internal/app/app.go",
  "include_connected_sources": true,
  "max_connected_files": 8,
  "agent_id": "plug-agent"
}
// → Target source + 8 connected files returned in one call (1.3ms)
// → No manual file reading. Complete context for implementation.

// Step 6: Validate your integration assumption before writing code
{
  "tool": "hypothesize",
  "claim": "adding a middleware hook before app.Run() will intercept all CLI commands",
  "depth": 8,
  "agent_id": "plug-agent"
}
// → confidence: 0.91, 14 supporting evidence paths
// → Safe to proceed. No guessing.

// Step 7: Identify risks across the integration surface
{ "tool": "missing", "query": "plugin lifecycle teardown cleanup on exit", "agent_id": "plug-agent" }
// → 3 structural holes: no plugin deregister path, no panic recovery in hook chain
// → 23 total risks surfaced across the integration surface

// Step 8: Implement atomically
{
  "tool": "apply_batch",
  "edits": [
    { "file_path": "internal/app/hooks.go", "new_content": "..." },
    { "file_path": "internal/app/app.go", "new_content": "..." }
  ],
  "atomic": true,
  "agent_id": "plug-agent"
}
// → Both files written atomically, graph re-indexed in one pass
// → predict() auto-runs post-apply to surface any missed co-changes
```

**Real session metrics (OpenCode, Go, 140 files):**

| Metric | Result |
|--------|--------|
| Files ingested | 140 Go files |
| Graph build time | 1 second |
| Nodes created | 1,888 |
| Entry points found | 15 |
| Hook points identified | 10 |
| Integration risks surfaced | 23 |
| Files requiring changes | 30% (70% needed zero code changes) |
| Time from cold start to integration plan | 30 minutes |

**Why this matters:**

Integrating into an unfamiliar codebase normally takes days of source reading. m1nd eliminates
that by answering "where do I attach?" structurally, not textually. The graph understands
which nodes are load-bearing, which are extensible, and what breaks if you attach at the wrong point.

70% of integration required zero code changes — the entry points were already there, invisible
to anyone who hadn't built the graph first.

---

## 9. Comparative Benchmarks

### Operation Latency (criterion benchmarks, real hardware)

| Operation | Time | Notes |
|-----------|------|-------|
| `activate` 1K nodes | 1.36 µs | Spreading activation |
| `impact` depth=3 | 543 ns | Blast radius |
| `flow_simulate` 4 particles | 552 µs | Race condition detection |
| `epidemic` SIR 50 iterations | 110 µs | Bug propagation prediction |
| `antibody_scan` 50 patterns | 2.68 ms | Known-pattern detection |
| `tremor` 500 nodes | 236 µs | Change velocity analysis |
| `trust` 500 nodes | 70 µs | Module reliability report |
| `layers` 500 nodes | 862 µs | Architecture detection |
| `resonance` 5 harmonics | 8.17 µs | Harmonic analysis |
| `lock_diff` 1,639 nodes | 0.08 µs | Structural diff |
| `trace` (stacktrace→suspects) | 9.8 ms | Root cause ranking |
| `ingest` 380 Python files | 1.3 s | Full repo indexing |
| `ingest` 82 markdown docs | 138 ms | Documentation corpus |
| `ingest` single file | 0.07 ms | Incremental update |

---

### m1nd vs grep — Empirical Comparison (same audit session)

| Metric | m1nd | grep/glob/read | Advantage |
|--------|------|----------------|-----------|
| Operations to find 39 bugs | 46 queries | ~210 estimated | 4.6x fewer |
| Total search latency | ~3.1 seconds | ~35+ minutes | 680x faster |
| LLM tokens consumed | 0 | ~1.8M estimated | 100% savings |
| Bugs found | 39 | ~20 estimated | 95% more |
| Bugs invisible to grep | 8 (28.5%) | 0 | m1nd exclusive |
| False positive rate | ~15% | ~50% estimated | 3.3x more precise |

**Cost savings at Opus pricing (~$15/M input, ~$75/M output):**
- Per audit session: **$27–135 saved**
- Monthly (2–3 audits): **$50–400 saved**
- Annual per developer: **$600–4,800 saved**

---

### The 8 Bugs Grep Could Never Find

These bugs exist in the ABSENCE of something, not in the presence of text patterns.
grep cannot find what is not there.

| Bug | Tool | Detection mechanism |
|-----|------|---------------------|
| Stormender TOCTOU race | `resonate` | 9 cancel nodes at amplitude 1.4 not converging |
| Lifespan double-stop | `activate` | 12 activated neighbors but all inactive |
| Circuit breaker corruption | `missing` | Dict as structural hole in concurrency context |
| CancelledError swallowing | `activate` | Systemic pattern across 4 modules |
| Session pool leak on storm cancel | `hypothesize` 99% | CancelledError interrupts finally block |
| Session pool release unshielded | `hypothesize` | asyncio.shield missing on pool.release() |
| Worker pool CancelledError bypass | `hypothesize` | except Exception misses BaseException |
| Sacred memory concurrent corruption | `hypothesize` 69% | Check-then-write not atomic |

---

### Memory Adapter: Cross-Domain Search

Ingest 82 documentation files (PRDs, specs, audits) alongside code. One graph. One query.

```jsonc
{ "tool": "activate", "query": "antibody pattern matching" }
// → PRD-ANTIBODIES.md (score 1.156) AND pattern_models.py (score 0.904)
// → One query. The spec and the implementation. Automatically.

{ "tool": "missing", "query": "GUI web server" }
// → Specs that exist WITHOUT a corresponding implementation
// → Gap detection across documentation and code simultaneously.
```

---

> grep operates in **text space** — it finds what you asked for.
> m1nd operates in **structure space** — it finds what SHOULD exist but DOESN'T.

The most dangerous production bugs are not bugs in the text. They are bugs in the structure:
missing locks, missing guards, missing validators, missing error handlers. 28.5% of the bugs found
in the live audit were structurally invisible to grep. Not harder to find — impossible.

---

For tool-level documentation, see the [API Reference](API-Reference).

*All data from live sessions (2026-03-14 and 2026-03-15). Primary codebase: Python/FastAPI ~52K lines, 10,401 nodes. PLUG Integration: OpenCode Go, 140 files, 1,888 nodes.*
*m1nd v0.2 (Rust). Claude Code (Opus 4.6) + 16 Sonnet 4.6 agents.*
